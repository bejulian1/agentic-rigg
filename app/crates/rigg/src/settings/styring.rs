//! Innstillinger › Skjermkortstyring: én administratoraktivering og støtte per funksjon.
use super::{Group, GroupOpts, custom_row, group, kw, live, request_render, row};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::locked;
use crate::widgets::rows::KvList;
use crate::widgets::{button, hbox, label, vbox};
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

fn admin(app: &App) -> Option<PathBuf> {
    if app.profile.is_dev() {
        let fake = PathBuf::from(std::env::var_os("RIGG_PKEXEC")?);
        let root = PathBuf::from(std::env::var_os("RIGG_HJELPER_ROT")?);
        return (fake.ends_with("falsk-pkexec.sh") && fake.is_absolute() && fake.canonicalize().is_ok_and(|path| path == fake) && root.starts_with("/tmp")
            && data_dir(app).is_absolute() && app.data.borrow().backend.starts_with("sim")).then_some(fake);
    }
    (!app.data.borrow().backend.starts_with("sim") && crate::system_install::ready())
        .then(|| "/usr/bin/pkexec".into())
}

fn saved_binding(app: &App) -> bool {
    if app.profile.is_dev() || app.data.borrow().backend.starts_with("sim") { return false; }
    let path = std::path::Path::new(rigg_core::hjelper::CONFIG_PATH);
    if crate::system_install::trusted(path) {
        return std::fs::read_to_string(path).ok()
            .and_then(|raw| rigg_core::hjelper::Config::parse(&raw).ok())
            .is_some_and(|grant| !grant.legacy && grant.uid == unsafe { libc::geteuid() });
    }
    // Eldre RGB-tillatelser er private. Filen gir tilgang til opprydding i Innstillinger.
    crate::system_install::trusted(std::path::Path::new(rigg_core::rgb_auth::CONFIG_PATH))
}

/// Statusen i klartekst. Merket foran sier tilstanden (Av, På, Feil), så
/// teksten gjentar den ikke. Det tekniske står i verktøytipset ([`state_tip`]).
fn state_text(app: &App) -> String {
    if BUSY.with(Cell::get) { return tr!("Venter på administratorgodkjenning …").into(); }
    if crate::system_install::busy() { return tr!("Installerer systemkomponentene …").into(); }
    if !app.client.is_connected() { return tr!("Tjenesten svarer ikke. Endringer er låst.").into(); }
    let h = locked::helper(app);
    match locked::helper_state(app).as_str() {
        "waiting" => locked::helper_line(app).unwrap_or_else(|| tr!("Venter på enhet").into()),
        "on" if locked::control_ready(app) => tr!("Styring er klar for det valgte skjermkortet. Endringer brukes først når du trykker Bruk.").into(),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => tr!("Simulert styring. Det ekte skjermkortet endres ikke.").into(),
        "outdated" if app.profile.is_dev() || !crate::system_install::newer_installed() => tr!("Oppdater systemkomponenter").into(),
        "on" | "error" | "outdated" => h["message"].as_str().map(display_diagnostic).map(|text| text.into_owned()).unwrap_or_else(|| tr!("Hjelperen er ikke klar for kortet som vises. Se funksjonsstøtten under, eller trykk Sjekk igjen.").into()),
        _ if saved_binding(app) && installable(app) => tr!("Oppdater systemkomponenter").into(),
        _ if saved_binding(app) => tr!("Venter på enhet").into(),
        _ if installable(app) => tr!("Systemkomponentene mangler. Slå på bryteren for å installere dem og aktivere styring.").into(),
        _ if admin(app).is_none() => tr!("Systemkomponentene mangler eller må oppdateres.").into(),
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
        && (admin(app).is_none() || locked::helper_state(app) == "outdated")
}

/// Bare private skjermbildeplaner (utviklerprofil): vis «Venter på
/// administratorgodkjenning …» uten å starte noe.
pub(crate) fn debug_waiting(app: &App, on: bool) {
    if app.profile.is_dev() && std::env::var_os("RIGG_SKJERMBILDER").is_some() {
        BUSY.with(|b| b.set(on));
        request_render();
    }
}

/// Aktivering eller deaktivering pågår (installasjonen venter da).
pub(crate) fn admin_busy() -> bool {
    BUSY.with(Cell::get)
}

/// Meldingen etter en vellykket aktivering (helperstatusen er lest på nytt).
fn activated(app: &Rc<App>) {
    if matches!(locked::gate(app), rigg_core::txn::Gate::Real | rigg_core::txn::Gate::Sim) {
        app.toast(Toast::new(tr!("Skjermkortstyring er aktivert")).body(tr!("Velg endringer og trykk Bruk. Behold dem innen fristen, ellers rulles de tilbake.")).ms(5000));
    } else if locked::helper_state(app) == "waiting" {
        app.toast(Toast::new(tr!("Skjermkortstyring er aktivert")).body(tr!("Venter på enhet")).ms(5000));
    } else {
        app.toast(Toast::new(tr!("Hjelperen er installert, men styring er ikke klar")).body(&state_text(app)).error());
    }
}

pub(crate) fn toggle(app: &Rc<App>, on: bool) {
    if BUSY.with(Cell::get) || crate::system_install::busy() { return; }
    let install = if on { installable(app) } else {
        admin(app).is_none() && !app.profile.is_dev() && crate::system_install::can_install()
    };
    let pkexec = admin(app);
    if pkexec.is_none() && !install { return; }
    if on && !app.profile.is_dev() && app.data.borrow().backend != "nvml" { return; }
    if install && !on {
        let a = app.clone();
        crate::system_install::run(app, crate::system_install::CardAction::Revoke, move |ok| {
            if ok { a.toast(Toast::new(tr!("Skjermkortstyring er slått av")).body(tr!("Tillatelsen er fjernet.")).ms(4000)); }
        });
        return;
    }
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
            crate::system_install::run(app, crate::system_install::CardAction::Activate(pci, uuid), move |ok| {
                if ok {
                    let b = a.clone();
                    locked::refresh_helper(&a, move || { request_render(); activated(&b); });
                }
            });
            request_render();
            return;
        }
        vec!["--card-control".into(), "--pci".into(), pci, "--uuid".into(), uuid]
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
                None => a2.toast(Toast::new(tr!("Skjermkortstyring er slått av")).body(tr!("Tillatelsen er fjernet.")).ms(4000)),
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

fn rgb_state_text(helper: Option<&serde_json::Value>, selected: Option<&(String, String)>) -> &'static str {
    let Some(helper) = helper else { return tr!("Ikke klar"); };
    let status = &helper["status"];
    let matches = selected.is_some_and(|(pci, uuid)| status["identity"]["pci"] == pci.as_str()
        && status["identity"]["uuid"] == uuid.as_str());
    match helper["state"].as_str() {
        Some("on") if matches && crate::pages::lys_kort::lacks_current_features(helper) => tr!("Oppdater systemkomponenter"),
        Some("on") if matches && (status["review"].is_object() || crate::pages::lys_kort::is_blocked(&status["state"]) || status["runtimeError"].is_string()) => tr!("Må avklares"),
        Some("on") if matches => tr!("Tilgjengelig"),
        Some("unsupported") if matches => tr!("Ikke støttet"),
        Some("waiting") if matches => tr!("Venter på enhet"),
        Some("off") => tr!("Av"),
        Some("outdated") => tr!("Oppdater systemkomponenter"),
        _ => tr!("Ikke klar"),
    }
}

fn switch_active(state: &str, saved: bool) -> bool {
    saved || matches!(state, "waiting" | "on" | "outdated")
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
    let rgb = label(tr!("Ikke undersøkt"), &["tag", "quiet"]);
    let detail = label("", &["sm", "muted"]);
    detail.set_wrap(true);
    detail.set_visible(false);
    let info = vbox(3, &[]);
    rgb.set_halign(gtk::Align::End);
    info.append(&rgb);
    info.append(&detail);
    let timer = Rc::new(std::cell::RefCell::new(None::<glib::SourceId>));
    let running = timer.clone();
    let a = app.clone();
    rgb.connect_map(move |label| {
        if let Some(id) = running.borrow_mut().take() { id.remove(); }
        let detail = detail.downgrade();
        let a = a.clone();
        let target = label.downgrade();
        let pending = Rc::new(Cell::new(false));
        let refresh = move || {
            if pending.replace(true) { return; }
            let detail = detail.clone();
            let current = a.clone();
            let target = target.clone();
            let pending = pending.clone();
            a.client.request("get_rgb_helper", serde_json::json!({"refresh":true}), move |reply| {
                pending.set(false);
                if let Some(label) = target.upgrade() {
                    let selected = current.gpu_id().zip(current.statics().and_then(|s| s.uuid));
                    let (helper, error) = match &reply {
                        Ok(h) => (Some(h), None),
                        Err(e) => (None, Some(e.message.as_str())),
                    };
                    let text = rgb_state_text(helper, selected.as_ref());
                    label.set_text(text);
                    let kind = if text == tr!("Tilgjengelig") { "ok" }
                        else if text == tr!("Ikke klar") { "danger" }
                        else if [tr!("Venter på enhet"), tr!("Må avklares"), tr!("Oppdater systemkomponenter")].contains(&text) { "warn" }
                        else { "quiet" };
                    label.set_css_classes(&["tag", kind]);
                    let diagnostic = error.or_else(|| helper.and_then(|h| {
                        let s = &h["status"];
                        [s["runtimeError"].as_str(), s["state"]["error"].as_str(),
                         s["state"]["readError"].as_str(), h["message"].as_str()].into_iter().flatten().next()
                    })).map(display_diagnostic);
                    if let Some(detail) = detail.upgrade() {
                        let fallback = if text == tr!("Må avklares") { tr!("Se RGB-lys") }
                            else if text == tr!("Venter på enhet") { tr!("Venter på at RGB-arbeideren skal starte.") }
                            else if text == tr!("Ikke klar") { tr!("Sjekk igjen") } else { "" };
                        let message = diagnostic.as_deref().unwrap_or(fallback);
                        detail.set_text(message);
                        detail.set_visible(!message.is_empty());
                    }
                }
            });
        };
        refresh();
        *running.borrow_mut() = Some(glib::timeout_add_seconds_local(3, move || {
            refresh();
            glib::ControlFlow::Continue
        }));
    });
    rgb.connect_unmap(move |_| {
        if let Some(id) = timer.borrow_mut().take() { id.remove(); }
    });
    rows.push(row(tr!("Kortlys (RGB)"), None, &info, "RGB"));
    group(tr!("Tilgjengelige funksjoner"), GroupOpts {
        desc: Some(tr!("Én administratoraktivering gjelder støttede funksjoner på det valgte kortet. Listen er status, ikke ekstra godkjenninger.")),

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
    // Bryteren viser tillatelsen. Arbeiderne rapporterer beredskap hver for seg.
    let consent = saved_binding(app);
    let waiting = state == "waiting" || (state == "off" && consent && !installable(app));
    let active = switch_active(&state, consent);
    let busy = BUSY.with(Cell::get) || crate::system_install::busy();

    // ---- Bryteren ----
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_state(active);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(tr!("Tillat endring av skjermkortet"))]);
    let revoke_via_install = active && !app.profile.is_dev() && !app.data.borrow().backend.starts_with("sim")
        && crate::system_install::can_install();
    let can = app.client.is_connected() && (admin(app).is_some() || installable(app) || revoke_via_install)
        && (active || app.profile.is_dev() || app.data.borrow().backend == "nvml");
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
        Some(tr!("Tillater effekt, vifter, klokker og temperaturmål på valgt kort, samt kortlyset på støttede Palit-kort.")),
        &sw,
        &kw("root polkit pkexec hjelper skriving endre overklokking", tr!("søkeord", "root polkit pkexec hjelper skriving endre overklokking")),
    );
    crate::widgets::switchrow::make_row_toggle(&main_row.widget, &sw);

    // Status og tekst på én linje, knappene under: teksten får hele bredden,
    // så lange ord (for eksempel «administratorgodkjenning») ikke deles.
    let status_box = vbox(6, &[]);
    let status_line = hbox(8, &["helper-status"]);
    // Raden har en minstehøyde; statuslinjen står midt i den, ikke øverst.
    status_line.set_vexpand(true);
    status_line.set_valign(gtk::Align::Center);
    let status_actions = hbox(8, &[]);
    status_actions.set_halign(gtk::Align::Start);
    status_box.append(&status_line);
    let (kind, icon) = match state.as_str() {
        _ if busy => ("quiet", "timer"),
        _ if waiting => ("warn", "timer"),
        "on" if locked::control_ready(app) => ("ok", "shield"),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => ("quiet", "shield"),
        "on" => ("warn", "circle-alert"),
        "error" => ("danger", "triangle-alert"),
        "outdated" => ("warn", "circle-alert"),
        _ => ("quiet", "lock"),
    };
    let word = match state.as_str() {
        _ if busy => tr!("Venter"),
        _ if waiting => tr!("Venter på enhet"),
        "off" if consent => tr!("Oppdater hjelper"),
        "on" if locked::control_ready(app) => tr!("Klar"),
        "on" if locked::gate(app) == rigg_core::txn::Gate::Sim => tr!("Simulert"),
        "on" => tr!("Ikke klar"),
        "error" => tr!("Feil"),
        "outdated" => tr!("Oppdater hjelper"),
        _ => tr!("Av"),
    };
    status_line.append(&crate::widgets::rows::tag(word, Some(icon), kind));
    let status = label(&state_text(app), &["helper-status-text"]);
    status.set_wrap(true);
    status.set_wrap_mode(gtk::pango::WrapMode::Word);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    live(&status, state_text);
    status.set_tooltip_text(state_tip(app).as_deref());
    status_line.append(&status);
    // Mens administratorgodkjenningen venter, er det ingenting å åpne eller sjekke.
    if !busy && (locked::control_ready(app) || locked::gate(app) == rigg_core::txn::Gate::Sim) {
        let open = button(Some("arrow-right"), Some(tr!("Åpne styring")), &["sm"]);
        let a = app.clone();
        open.connect_clicked(move |_| { a.shell().close_settings(); a.go("kontroller"); });
        status_actions.append(&open);
    }
    if active && !busy && installable(app) {
        let update = button(Some("refresh-cw"), Some(tr!("Oppdater systemkomponenter")), &["sm"]);
        let a = app.clone();
        update.connect_clicked(move |_| toggle(&a, true));
        status_actions.append(&update);
    }
    if !busy && state == "error" && admin(app).is_some() {
        let restart = button(Some("refresh-cw"), Some(tr!("Start på nytt")), &["sm"]);
        let a = app.clone();
        restart.connect_clicked(move |_| toggle(&a, true));
        status_actions.append(&restart);
    }
    if !busy && (waiting || matches!(state.as_str(), "error" | "outdated")) {
        let again = button(Some("refresh-cw"), Some(tr!("Sjekk igjen")), &["sm"]);
        let a = app.clone();
        again.connect_clicked(move |_| locked::refresh_helper(&a, request_render));
        status_actions.append(&again);
    }
    if status_actions.first_child().is_some() {
        status_box.append(&status_actions);
    }
    let status_row = custom_row(&status_box, &format!("status {}", state_text(app)));
    status_row.widget.add_css_class("set-row");
    status_row.widget.add_css_class("helper-status-row");

    let mut rows = vec![main_row, status_row];
    if !app.profile.is_dev() && !crate::system_install::newer_installed()
        && (state == "outdated" || ((active || app.data.borrow().backend == "nvml") && admin(app).is_none())) {
        rows.push(custom_row(&crate::system_install::offline_fallback(), &kw("systemkomponenter installasjon terminal", tr!("søkeord", "systemkomponenter installasjon terminal"))));
    }
    let toggle_group = group(
        tr!("Endring av skjermkortet"),
        GroupOpts {
            note: Some(tr!("Når du slår av tilgangen, stoppes begge arbeiderne. Lagret lys og uavklarte endringer bevares.")),
            k: &kw("bryter av på status", tr!("søkeord", "bryter av på status")),
            ..Default::default()
        },
        rows,
    );

    // ---- Hva bryteren legger inn ----
    let kv = KvList::new();
    kv.grid.add_css_class("helper-kv");
    kv.add(tr!("Hjelperen"), tr!("To isolerte arbeidere for GPU-styring og kortlys"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bryteren_bevarer_tillatelsen_mens_arbeiderne_venter_eller_stopper() {
        assert!(!switch_active("off", false));
        assert!(!switch_active("error", false));
        for state in ["off", "waiting", "on", "outdated", "error"] {
            assert!(switch_active(state, true));
        }
        let selected = ("0000:01:00.0".to_string(), "GPU-fixture".to_string());
        for (state, text) in [("waiting", "Venter på enhet"), ("error", "Ikke klar"),
            ("unsupported", "Ikke støttet"), ("on", "Tilgjengelig")] {
            let helper = serde_json::json!({"state":state,"status":{
                "persist":true,"hold":true,"identity":{"pci":selected.0, "uuid":selected.1}}});
            assert_eq!(rgb_state_text(Some(&helper), Some(&selected)), text);
            assert_eq!(rgb_state_text(Some(&helper), None), "Ikke klar");
        }
        assert_eq!(rgb_state_text(Some(&serde_json::json!({"state":"on"})), None), "Ikke klar");
    }
}
