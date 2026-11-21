//! Innstillinger › Skjermkortstyring: én administratoraktivering og støtte per funksjon.
use super::{Group, GroupOpts, custom_row, group, kw, live, request_render, row};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::locked;
use crate::widgets::rows::KvList;
use crate::widgets::{button, hbox, label};
use gtk::glib;
use gtk::prelude::*;
use rigg_core::hjelper::{Step, display_diagnostic};
use rigg_core::{tr, trf};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    /// Bryteren venter på pkexec (passordvinduet er oppe).
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn data_dir(app: &App) -> PathBuf {
    if app.profile.is_dev() {
        return std::env::var_os("RIGG_HJELPER_DATA").map(PathBuf::from).unwrap_or_default();
    }
    PathBuf::from("/usr/local/libexec/agentic-rigg")
}

fn admin(app: &App, activate: bool) -> Option<PathBuf> {
    if app.profile.is_dev() {
        let fake = PathBuf::from(std::env::var_os("RIGG_PKEXEC")?);
        let root = PathBuf::from(std::env::var_os("RIGG_HJELPER_ROT")?);
        return (fake.ends_with("falsk-pkexec.sh") && fake.is_absolute() && fake.canonicalize().is_ok_and(|path| path == fake) && root.starts_with("/tmp")
            && data_dir(app).is_absolute() && app.data.borrow().backend.starts_with("sim")).then_some(fake);
    }
    let dir = data_dir(app);
    (!app.data.borrow().backend.starts_with("sim") && crate::system_install::ready(if activate { crate::system_install::Need::Nvml } else { crate::system_install::Need::Cleanup })
        && [PathBuf::from("/usr/local/libexec/agentic-rigg-helper"), dir.join("agentic-rigg-helper.service"), dir.join("aktiver.sh"), dir.join("deaktiver.sh"), dir.join("nvml-admin.py"),
            PathBuf::from("/usr/share/polkit-1/actions/io.github.AgenticRigg.helper.policy")]
            .iter().all(|path| crate::system_install::trusted(path))).then(|| "/usr/bin/pkexec".into())
}

/// Statusen i klartekst. Merket foran sier tilstanden (Av, På, Feil), så
/// teksten gjentar den ikke. Det tekniske står i verktøytipset ([`state_tip`]).
fn state_text(app: &App) -> String {
    if BUSY.with(Cell::get) { return tr!("Venter på administratorgodkjenning …").into(); }
    if crate::system_install::busy() { return tr!("Installerer systemkomponentene …").into(); }
    if !app.client.is_connected() { return tr!("Tjenesten svarer ikke. Endringer er låst.").into(); }
    let h = locked::helper(app);
    match locked::helper_state(app).as_str() {
        "on" if locked::control_ready(app) => tr!("Styring er klar for det valgte skjermkortet. Endringer brukes først når du trykker Bruk.").into(),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => tr!("Simulert styring. Det ekte skjermkortet endres ikke.").into(),
        "on" | "error" => h["message"].as_str().map(display_diagnostic).map(|text| text.into_owned()).unwrap_or_else(|| tr!("Hjelperen er ikke klar for kortet som vises. Se funksjonsstøtten under, eller trykk Sjekk igjen.").into()),
        "outdated" => tr!("Hjelperen må oppdateres. Slå på bryteren for å oppdatere den og aktivere styring.").into(),
        _ if installable(app) => tr!("Systemkomponentene mangler. Slå på bryteren for å installere dem og aktivere styring.").into(),
        _ if admin(app, true).is_none() => tr!("Systemkomponentene mangler eller må oppdateres.").into(),
        _ => tr!("Bare visning. Endring av skjermkortet er slått av.").into(),
    }
}

/// Det tekniske bak statusen (verktøytipset): bygg, kort og feilmeldingen.
fn state_tip(app: &App) -> Option<String> {
    let h = locked::helper(app);
    let mut parts = Vec::new();
    if let Some(b) = h["build"].as_u64() {
        parts.push(trf!("agentic-rigg-helper {versjon} · bygg {bygg}", versjon = h["version"].as_str().unwrap_or(""), bygg = rigg_core::fmt::num(Some(b as f64), 0)));
    }
    if let Some(b) = h["backend"].as_str() {
        parts.push(if b == "sim" { tr!("Simulert kort").to_string() } else { tr!("Det ekte kortet (NVML)").to_string() });
    }
    if let Some(m) = h["message"].as_str() {
        parts.push(display_diagnostic(m).into_owned());
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Kjører `pkexec PROGRAM ARG…` uten å blokkere vinduet. `done(kode, feil)`.
fn run_pkexec(pkexec: PathBuf, program: PathBuf, args: Vec<String>, done: impl FnOnce(i32, String) + 'static) {
    let mut argv: Vec<std::ffi::OsString> = vec![pkexec.into_os_string(), program.into_os_string()];
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
        _ => Some(Toast::new(title).body(display_diagnostic(if err.is_empty() { tr!("Skriptet feilet.") } else { err }).as_ref()).error()),
    }
}

/// Systemkomponentene mangler eller hjelperen er utdatert, og appen kan
/// installere dem selv: bryteren installerer og aktiverer da med ett passord.
fn installable(app: &App) -> bool {
    !app.profile.is_dev() && app.data.borrow().backend == "nvml" && crate::system_install::can_install()
        && (admin(app, true).is_none() || locked::helper_state(app) == "outdated")
}

/// Aktivering eller deaktivering pågår (installasjonen venter da).
pub(crate) fn admin_busy() -> bool {
    BUSY.with(Cell::get)
}

/// Meldingen etter en vellykket aktivering (helperstatusen er lest på nytt).
fn activated(app: &Rc<App>) {
    if matches!(locked::gate(app), rigg_core::txn::Gate::Real | rigg_core::txn::Gate::Sim) {
        app.toast(Toast::new(tr!("Skjermkortstyring er aktivert")).body(tr!("Velg endringer og trykk Bruk. Behold dem innen fristen, ellers rulles de tilbake.")).ms(5000));
    } else {
        app.toast(Toast::new(tr!("Hjelperen er installert, men styring er ikke klar")).body(&state_text(app)).error());
    }
}

pub(crate) fn toggle(app: &Rc<App>, on: bool) {
    if BUSY.with(Cell::get) || crate::system_install::busy() { return; }
    let install = on && installable(app);
    let pkexec = admin(app, on);
    if pkexec.is_none() && !install { return; }
    if on && !app.profile.is_dev() && app.data.borrow().backend != "nvml" { return; }
    let args = if on {
        let Some(pci) = app.gpu_id() else {
            app.toast(Toast::new(tr!("Ingen skjermkort valgt")).error());
            return;
        };
        let Some(uuid) = app.statics().and_then(|s| s.uuid) else {
            app.toast(Toast::new(tr!("Kortidentiteten er ikke klar")).body(tr!("Vent til driveren har lest UUID, og prøv igjen.")).error());
            return;
        };
        if install {
            // Ett passord: hent og installer systemkomponentene, og aktiver.
            let a = app.clone();
            crate::system_install::run(app, crate::system_install::Activate::Nvml(pci, uuid), move |ok| {
                if ok {
                    let b = a.clone();
                    locked::refresh_helper(&a, move || { request_render(); activated(&b); });
                }
            });
            request_render();
            return;
        }
        vec!["--pci".into(), pci, "--uuid".into(), uuid]
    } else { Vec::new() };
    let Some(pkexec) = pkexec else { return };
    BUSY.with(|b| b.set(true));
    request_render();
    let (program, what) = if on { (data_dir(app).join("aktiver.sh"), tr!("Kunne ikke aktivere styring")) } else { (data_dir(app).join("deaktiver.sh"), tr!("Kunne ikke slå av styring")) };
    let a = app.clone();
    run_pkexec(pkexec, program, args, move |code, err| {
        BUSY.with(|b| b.set(false));
        let a2 = a.clone();
        locked::refresh_helper(&a, move || {
            request_render();
            match failure(code, &err, what) {
                Some(t) => a2.toast(t),
                None if on => activated(&a2),
                None => a2.toast(Toast::new(tr!("Skjermkortstyring er slått av")).body(tr!("Hjelperen har gjenopprettet utgangspunktet og tillatelsen er fjernet.")).ms(4000)),
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
    for (key, name) in [("power", tr!("Effektgrense")), ("fan_manual", tr!("Manuell viftefart og Auto")), ("fan_curve", tr!("Viftekurve")), ("core_clock", tr!("Kjerneklokke")), ("mem_clock", tr!("Minneklokke")), ("temp_target", tr!("Temperaturmål"))] {
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
                    trf!("Kortets rapporterte område: {omraade}.", omraade = span)
                },
                _ => tr!("Støttes av den tilkoblede hjelperen.").into(),
            }
        } else { locked::capability_reason(app, key) };
        rows.push(row(name, Some(&detail), &tag, &format!("funksjonsstøtte {key} {status}")));
    }
    group(tr!("Tilgjengelige funksjoner"), GroupOpts {
        desc: Some(tr!("Én administratoraktivering gjelder støttede funksjoner på det valgte kortet. Listen er status, ikke ekstra godkjenninger.")),
        note: Some(tr!("Funksjoner uten støtte kan ikke brukes på kortet. RGB har egen tillatelse på siden RGB-lys.")),
        k: &kw("funksjoner støtte effekt vifter klokker temperatur", tr!("søkeord", "funksjoner støtte effekt vifter klokker temperatur")),
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
    // En utdatert hjelper vises som av: bryteren oppdaterer og aktiverer.
    let active = matches!(state.as_str(), "on" | "error");
    let busy = BUSY.with(Cell::get);

    // ---- Bryteren ----
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_state(active);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(tr!("Tillat endring av skjermkortet"))]);
    let can = app.client.is_connected() && (admin(app, !active).is_some() || (!active && installable(app)))
        && (active || app.profile.is_dev() || app.data.borrow().backend == "nvml");
    sw.set_sensitive(can && !busy && !crate::system_install::busy());
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
        &kw("root polkit pkexec hjelper skriving endre overklokking", tr!("søkeord", "root polkit pkexec hjelper skriving endre overklokking")),
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
        "outdated" => tr!("Oppdater hjelper"),
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

    let mut rows = vec![main_row, status_row];
    if !app.profile.is_dev() && (state == "outdated" || ((active || app.data.borrow().backend == "nvml") && admin(app, true).is_none())) {
        rows.push(custom_row(&crate::system_install::offline_fallback(), &kw("systemkomponenter installasjon terminal", tr!("søkeord", "systemkomponenter installasjon terminal"))));
    }
    let toggle_group = group(
        tr!("Endring av skjermkortet"),
        GroupOpts {
            note: Some(tr!("Slår du bryteren av, gjenopprettes det lagrede utgangspunktet før tillatelsen fjernes. En feil vises hvis gjenopprettingen ikke kan bekreftes. Slå av styring før avinstallasjon.")),
            k: &kw("bryter av på status", tr!("søkeord", "bryter av på status")),
            ..Default::default()
        },
        rows,
    );

    // ---- Hva bryteren legger inn ----
    let kv = KvList::new();
    kv.grid.add_css_class("helper-kv");
    kv.add(tr!("Hjelperen"), tr!("Et lite program som kjører i bakgrunnen med administratorrettigheter. Det kan bare endre effekt, vifter og klokker, innenfor det kortet selv tillater"));
    kv.add(tr!("Passord"), tr!("Én administratorgodkjenning ved aktivering. Vanlige endringer bruker Bruk og Behold"));
    kv.add(tr!("Hvem"), tr!("Bare brukeren din. Andre brukere på maskinen slipper ikke til"));
    kv.add(tr!("Vern"), tr!("Endringer rulles tilbake uten Behold innen fristen. Viftekurven har nødregel ved høy temperatur. Frakobling og stopp gjenoppretter utgangspunktet; eventuelle feil rapporteres"));
    let files = kv.add(tr!("Filene"), &trf!("{hjelper}\n{konfigurasjon} (bruker, kort og aktivering)", hjelper = "/usr/local/libexec/agentic-rigg-helper", konfigurasjon = "/etc/agentic-rigg/helper.json"));
    files.add_css_class("mono");
    let paths = kv.add(tr!("Skriptene"), &rigg_core::paths::tilde(&data_dir(app)));
    paths.add_css_class("mono");
    let mut what = group(tr!("Hva bryteren gjør"), GroupOpts { whole: true, k: &kw("hjelper root polkit systemd sikkerhet passord", tr!("søkeord", "hjelper root polkit systemd sikkerhet passord")), ..Default::default() }, vec![]);
    what.widget.append(&kv.grid);
    what.rows.push(custom_row(&gtk::Box::new(gtk::Orientation::Horizontal, 0), &kw("hjelper root polkit systemd passord vern frist nødregel", tr!("søkeord", "hjelper root polkit systemd passord vern frist nødregel"))));

    vec![toggle_group, capabilities_group(app), what]
}
