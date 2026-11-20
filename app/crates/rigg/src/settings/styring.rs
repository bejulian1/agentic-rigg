//! Innstillinger › Skjermkortstyring: én administratoraktivering og støtte per funksjon.
//!
//! Bryteren kjører `pkexec <mappe>/aktiver.sh` eller `deaktiver.sh`. Det er
//! brukerens egen handling: passordvinduet kommer fra polkit, og skriptene
//! står i `app/data/hjelper/` (lagt i `~/.local/share/agentic-rigg/hjelper/`
//! av `install.sh`). I utviklerprofilen kan `RIGG_PKEXEC` peke på en falsk
//! pkexec (`scripts/falsk-pkexec.sh`), `RIGG_HJELPER_DATA` på mappa med
//! skriptene og `RIGG_HJELPER_ROT` på en falsk rot. Utenfor `dev` brukes de
//! aldri.
//!
use super::{Group, GroupOpts, custom_row, group, live, request_render, row};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::locked;
use crate::widgets::rows::KvList;
use crate::widgets::{button, hbox, label};
use gtk::glib;
use gtk::prelude::*;
use rigg_core::hjelper::{Paths, Step};
use rigg_core::{tr, trf};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    /// Bryteren venter på pkexec (passordvinduet er oppe).
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// pkexec: den ekte, eller den falske i utviklerprofilen.
fn pkexec(app: &App) -> PathBuf {
    if app.profile.is_dev()
        && let Some(p) = std::env::var_os("RIGG_PKEXEC").filter(|p| !p.is_empty())
    {
        return p.into();
    }
    PathBuf::from("/usr/bin/pkexec")
}

/// Mappa med skriptene brukeren selv har installert (`install.sh`).
fn data_dir(app: &App) -> PathBuf {
    if app.profile.is_dev()
        && let Some(p) = std::env::var_os("RIGG_HJELPER_DATA").filter(|p| !p.is_empty())
    {
        return p.into();
    }
    let base = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("agentic-rigg/hjelper")
}

/// Roten der hjelperen ligger når den er aktivert (`/`, eller falsk i dev).
fn root(app: &App) -> Paths {
    match std::env::var_os("RIGG_HJELPER_ROT") {
        Some(r) if app.profile.is_dev() && !r.is_empty() => Paths::new(r),
        _ => Paths::new("/"),
    }
}

/// Deaktiveringen kjøres helst fra kopien bare root kan endre.
fn deactivate_script(app: &App) -> PathBuf {
    let own = root(app).libexec("deaktiver.sh");
    if own.exists() { own } else { data_dir(app).join("deaktiver.sh") }
}



/// Statusen i klartekst. Merket foran sier tilstanden (Av, På, Feil), så
/// teksten gjentar den ikke. Det tekniske står i verktøytipset ([`state_tip`]).
fn state_text(app: &App) -> String {
    if BUSY.with(Cell::get) { return tr!("Venter på administratorgodkjenning …").into(); }
    if !app.client.is_connected() { return tr!("Tjenesten svarer ikke. Endringer er låst.").into(); }
    let h = locked::helper(app);
    match locked::helper_state(app).as_str() {
        "on" if locked::control_ready(app) => tr!("Styring er klar for det valgte skjermkortet. Endringer brukes først når du trykker Bruk.").into(),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => tr!("Simulert styring. Det ekte skjermkortet endres ikke.").into(),
        "on" | "error" => h["message"].as_str().map_or_else(|| tr!("Hjelperen er ikke klar for kortet som vises. Se funksjonsstøtten under, eller trykk Sjekk igjen.").into(), |message| rigg_core::hjelper::display_diagnostic(message).into_owned()),
        "outdated" => tr!("Hjelperen må oppdateres. Slå bryteren av og på igjen for å aktivere denne versjonen.").into(),
        _ if !data_dir(app).join("aktiver.sh").exists() => tr!("Hjelperfilene mangler. Installer den nye appversjonen først.").into(),
        _ => tr!("Bare visning. Endring av skjermkortet er slått av.").into(),
    }
}

/// Det tekniske bak statusen (verktøytipset): bygg, kort og feilmeldingen.
fn state_tip(app: &App) -> Option<String> {
    let h = locked::helper(app);
    let mut parts = Vec::new();
    if let Some(b) = h["build"].as_u64() {
        parts.push(trf!("agentic-rigg-helper {versjon} · bygg {b}", versjon = h["version"].as_str().unwrap_or(""), b = b));
    }
    if let Some(b) = h["backend"].as_str() {
        parts.push(if b == "sim" { tr!("Simulert kort").to_string() } else { tr!("Det ekte kortet (NVML)").to_string() });
    }
    if let Some(m) = h["message"].as_str() {
        parts.push(rigg_core::hjelper::display_diagnostic(m).into_owned());
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Kjører `pkexec PROGRAM ARG…` uten å blokkere vinduet. `done(kode, feil)`.
fn run_pkexec(app: &Rc<App>, program: PathBuf, args: Vec<String>, done: impl FnOnce(i32, String) + 'static) {
    let mut argv: Vec<std::ffi::OsString> = vec![pkexec(app).into_os_string(), program.into_os_string()];
    argv.extend(args.into_iter().map(Into::into));
    let refs: Vec<&std::ffi::OsStr> = argv.iter().map(|a| a.as_os_str()).collect();
    // Unntaket fra «ingen programmer» (clippy.toml): brukeren slo selv på
    // bryteren, og pkexec viser passordvinduet fra polkit.
    #[allow(clippy::disallowed_methods)]
    let started = gtk::gio::Subprocess::newv(&refs, gtk::gio::SubprocessFlags::STDOUT_SILENCE | gtk::gio::SubprocessFlags::STDERR_PIPE);
    let proc = match started {
        Ok(p) => p,
        Err(e) => {
            done(127, e.to_string());
            return;
        }
    };
    glib::MainContext::default().spawn_local(async move {
        let (_, err) = proc.communicate_utf8_future(None).await.unwrap_or((None, None));
        let code = if proc.has_exited() { proc.exit_status() } else { 1 };
        let err = err.map(|s| s.to_string()).unwrap_or_default();
        done(code, err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").to_string());
    });
}

/// Svar fra pkexec som melding. `None` når alt gikk bra.
fn failure(code: i32, err: &str, title: &str) -> Option<Toast> {
    match code {
        0 => None,
        126 => Some(Toast::new(tr!("Avbrutt")).body(tr!("Passordet ble ikke gitt. Ingenting er endret.")).ms(3500)),
        127 => Some(Toast::new(title).body(tr!("Fant ikke pkexec, eller ingen kan spørre om passordet (polkit).")).error()),
        _ => {
            let body = rigg_core::hjelper::display_diagnostic(err);
            Some(Toast::new(title).body(if body.is_empty() { tr!("Skriptet feilet.") } else { body.as_ref() }).error())
        }
    }
}

pub(crate) fn toggle(app: &Rc<App>, on: bool) {
    if BUSY.with(Cell::get) { return; }
    let args = if on {
        let Some(pci) = app.gpu_id() else {
            app.toast(Toast::new(tr!("Ingen skjermkort valgt")).error());
            return;
        };
        let Some(uuid) = app.statics().and_then(|s| s.uuid) else {
            app.toast(Toast::new(tr!("Kortidentiteten er ikke klar")).body(tr!("Vent til driveren har lest UUID, og prøv igjen.")).error());
            return;
        };
        vec!["--pci".into(), pci, "--uuid".into(), uuid]
    } else { Vec::new() };
    BUSY.with(|b| b.set(true));
    request_render();
    let (program, title) = if on { (data_dir(app).join("aktiver.sh"), tr!("Kunne ikke aktivere styring")) } else { (deactivate_script(app), tr!("Kunne ikke slå av styring")) };
    let a = app.clone();
    run_pkexec(app, program, args, move |code, err| {
        BUSY.with(|b| b.set(false));
        let a2 = a.clone();
        locked::refresh_helper(&a, move || {
            request_render();
            match failure(code, &err, title) {
                Some(t) => a2.toast(t),
                None if on && matches!(locked::gate(&a2), rigg_core::txn::Gate::Real | rigg_core::txn::Gate::Sim) => a2.toast(Toast::new(tr!("Skjermkortstyring er aktivert")).body(tr!("Velg endringer og trykk Bruk. Behold dem innen fristen, ellers rulles de tilbake.")).ms(5000)),
                None if on => a2.toast(Toast::new(tr!("Hjelperen er installert, men styring er ikke klar")).body(&state_text(&a2)).error()),
                None => a2.toast(Toast::new(tr!("Skjermkortstyring er slått av")).body(tr!("Hjelperen har gjenopprettet utgangspunktet og er fjernet.")).ms(4000)),
            }
        });
    });
}

// Eldre interne skjermbildeplaner har disse inngangene. Ingen separat
// trinngodkjenning utføres; den nye brukerflyten har bare én aktivering.
pub(crate) fn confirm_step(app: &Rc<App>, _step: Step) {
    app.toast(Toast::new(tr!("Én aktivering")).body(tr!("Bruk Tillat endring av skjermkortet. Funksjonsstøtten bestemmes av kortet og driveren.")));
}
pub(crate) fn set_step(app: &Rc<App>, _step: Step, _on: bool) {
    locked::refresh_helper(app, request_render);
}

fn capabilities_group(app: &Rc<App>) -> Group {
    let caps = locked::capabilities(app);
    let known = caps.is_object();
    let mut rows = Vec::new();
    for (key, name) in [("power", tr!("Effektgrense")), ("fan_manual", tr!("Manuell viftefart og Auto")), ("fan_curve", tr!("Viftekurve")), ("core_clock", tr!("Kjerneklokke")), ("mem_clock", tr!("Minneklokke")), ("temp_target", tr!("Temperaturmål")), ("rgb", tr!("RGB-lys"))] {
        let available = locked::supports(app, key);
        let status = if available { tr!("Tilgjengelig") } else if !known { tr!("Ikke undersøkt") } else { tr!("Utilgjengelig") };
        let tag = crate::widgets::rows::tag(status, available.then_some("check"), if available { "ok" } else { "quiet" });
        let detail = if !known {
            tr!("Leses når administratorhjelperen aktiveres for dette kortet.").to_string()
        } else if available {
            match (caps[key]["min"].as_f64(), caps[key]["max"].as_f64()) {
                (Some(lo), Some(hi)) => {
                    let span = if key == "temp_target" {
                        format!("{}–{}", rigg_core::fmt::temp(Some(lo), 0), rigg_core::fmt::temp(Some(hi), 0))
                    } else {
                        let unit = match key { "power" => "W", "core_clock" | "mem_clock" => "MHz", _ => "%" };
                        format!("{}–{} {unit}", rigg_core::fmt::num(Some(lo), 0), rigg_core::fmt::num(Some(hi), 0))
                    };
                    trf!("Kortets rapporterte område: {span}.", span = span)
                },
                _ => tr!("Støttes av den tilkoblede hjelperen.").into(),
            }
        } else { rigg_core::hjelper::display_message(&locked::capability_reason(app, key)).into() };
        rows.push(row(name, Some(&detail), &tag, &format!("funksjonsstøtte {key} {status}")));
    }
    group(tr!("Tilgjengelige funksjoner"), GroupOpts {
        desc: Some(tr!("Én administratoraktivering gjelder støttede funksjoner på det valgte kortet. Listen er status, ikke ekstra godkjenninger.")),
        note: Some(tr!("RGB krever en egen identifisert lyskontroller. Funksjoner uten støtte kan ikke brukes på kortet.")),
        k: "funksjoner støtte effekt vifter klokker temperatur rgb",
        ..Default::default()
    }, rows)
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    // Ny status hver gang delen vises (bryteren kan være endret utenfra).
    if !BUSY.with(Cell::get) {
        let a = app.clone();
        glib::idle_add_local_once(move || {
            let before = locked::helper(&a);
            let a2 = a.clone();
            locked::refresh_helper(&a, move || {
                if locked::helper(&a2) != before {
                    request_render();
                }
            });
        });
    }
    let state = locked::helper_state(app);
    let active = matches!(state.as_str(), "on" | "error" | "outdated");
    let busy = BUSY.with(Cell::get);

    // ---- Bryteren ----
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_state(active);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(tr!("Tillat endring av skjermkortet"))]);
    let can = app.client.is_connected() && (active || data_dir(app).join("aktiver.sh").exists());
    sw.set_sensitive(can && !busy);
    {
        let a = app.clone();
        // Bryteren flyttes først når skriptet er ferdig (state-set stoppes).
        sw.connect_state_set(move |_, want| {
            if want != active {
                toggle(&a, want);
            }
            glib::Propagation::Stop
        });
    }
    let main_row = row(
        tr!("Tillat endring av skjermkortet"),
        Some(tr!("Gir Agentic Rigg tilgang til støttede GPU-funksjoner via en administratorhjelper. Du blir spurt om passord. Ingen verdier endres før du trykker Bruk på en styringsside.")),
        &sw,
        "root polkit pkexec hjelper skriving endre overklokking",
    );
    crate::widgets::switchrow::make_row_toggle(&main_row.widget, &sw);

    let status_box = hbox(8, &["helper-status"]);
    let (kind, icon) = match state.as_str() {
        _ if busy => ("quiet", "timer"),
        "on" if locked::control_ready(app) => ("ok", "shield"),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => ("quiet", "shield"),
        "on" => ("warn", "circle-alert"),
        "error" => ("danger", "triangle-alert"),
        "outdated" => ("warn", "circle-alert"),
        _ => ("quiet", "lock"),
    };
    let word = match state.as_str() {
        _ if busy => tr!("Venter"),
        "on" if locked::control_ready(app) => tr!("Klar"),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => tr!("Simulert"),
        "on" => tr!("Ikke klar"),
        "error" => tr!("Feil"),
        "outdated" => tr!("Slå på igjen"),
        _ => tr!("Av"),
    };
    status_box.append(&crate::widgets::rows::tag(word, Some(icon), kind));
    let status = label(&state_text(app), &["helper-status-text"]);
    status.set_wrap(true);
    status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    live(&status, state_text);
    status.set_tooltip_text(state_tip(app).as_deref());
    status_box.append(&status);
    if locked::control_ready(app) || locked::gate(app) == rigg_core::txn::Gate::Sim {
        let open = button(Some("arrow-right"), Some(tr!("Åpne styring")), &["sm"]);
        let a = app.clone();
        open.connect_clicked(move |_| { a.shell().close_settings(); a.go("kontroller"); });
        status_box.append(&open);
    }
    if matches!(state.as_str(), "error" | "outdated") {
        let again = button(Some("refresh-cw"), Some(tr!("Sjekk igjen")), &["sm"]);
        let a = app.clone();
        again.connect_clicked(move |_| locked::refresh_helper(&a, request_render));
        status_box.append(&again);
    }
    let status_row = custom_row(&status_box, &format!("status {}", state_text(app)));
    status_row.widget.add_css_class("set-row");
    status_row.widget.add_css_class("helper-status-row");

    let toggle_group = group(
        tr!("Endring av skjermkortet"),
        GroupOpts {
            note: Some(tr!("Slår du bryteren av, gjenopprettes det lagrede utgangspunktet før hjelperen fjernes. En feil vises hvis gjenopprettingen ikke kan bekreftes. Slå av styring før avinstallasjon.")),
            k: "bryter av på status",
            ..Default::default()
        },
        vec![main_row, status_row],
    );

    // ---- Hva bryteren legger inn ----
    let kv = KvList::new();
    kv.grid.add_css_class("helper-kv");
    kv.add(tr!("Hjelperen"), tr!("Et lite program som kjører i bakgrunnen med administratorrettigheter. Det kan bare endre effekt, vifter og klokker, innenfor det kortet selv tillater"));
    kv.add(tr!("Passord"), tr!("Én administratorgodkjenning ved aktivering. Vanlige endringer bruker Bruk og Behold"));
    kv.add(tr!("Hvem"), tr!("Bare brukeren din. Andre brukere på maskinen slipper ikke til"));
    kv.add(tr!("Vern"), tr!("Endringer rulles tilbake uten Behold innen fristen. Viftekurven har nødregel ved høy temperatur. Frakobling og stopp gjenoppretter utgangspunktet; eventuelle feil rapporteres"));
    let files = kv.add(tr!("Filene"), &trf!("{binaer}\n{konfig} (bruker, kort og aktivering)", binaer = rigg_core::hjelper::BINARY_PATH, konfig = rigg_core::hjelper::CONFIG_PATH));
    files.add_css_class("mono");
    let paths = kv.add(tr!("Skriptene"), &rigg_core::paths::tilde(&data_dir(app)));
    paths.add_css_class("mono");
    let mut what = group(tr!("Hva bryteren gjør"), GroupOpts { whole: true, k: "hjelper root polkit systemd sikkerhet passord", ..Default::default() }, vec![]);
    what.widget.append(&kv.grid);
    what.rows.push(custom_row(&gtk::Box::new(gtk::Orientation::Horizontal, 0), "hjelper root polkit systemd passord vern frist nødregel"));

    vec![toggle_group, capabilities_group(app), what]
}
