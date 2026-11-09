//! Innstillinger › Skjermkortstyring (runde 7A, 1.2.0): bryteren «Tillat endring av
//! skjermkortet», status for root-hjelperen og trinnene.
//!
//! Bryteren kjører `pkexec <mappe>/aktiver.sh` eller `deaktiver.sh`. Det er
//! brukerens egen handling: passordvinduet kommer fra polkit, og skriptene
//! står i `app/data/hjelper/` (lagt i `~/.local/share/agentic-rigg/hjelper/`
//! av `install.sh`). I utviklerprofilen kan `RIGG_PKEXEC` peke på en falsk
//! pkexec (`scripts/falsk-pkexec.sh`), `RIGG_HJELPER_DATA` på mappa med
//! skriptene og `RIGG_HJELPER_ROT` på en falsk rot. Utenfor `dev` brukes de
//! aldri.
//!
//! Et trinn godkjennes bare etter et eget spørsmål med advarsel, og så med
//! passordet (`pkexec /usr/local/libexec/agentic-rigg/trinn.sh <trinn> på`).
//! Mot det ekte kortet er knappene av i 1.2.0: hjelperen kan ikke skrive dit
//! ennå (`rigg-hw`: `ekte-skriving` er av).

use super::{Group, GroupOpts, custom_row, group, live, request_render, row};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::locked;
use crate::widgets::rows::KvList;
use crate::widgets::{button, hbox, label, vbox};
use gtk::glib;
use gtk::prelude::*;
use rigg_core::hjelper::{Paths, STEPS, Step};
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

use locked::{real_card_locked, steps_later};

/// Statusen i klartekst. Merket foran sier tilstanden (Av, På, Feil), så
/// teksten gjentar den ikke. Det tekniske står i verktøytipset ([`state_tip`]).
fn state_text(app: &App) -> String {
    if BUSY.with(Cell::get) {
        return "Venter på passordet …".into();
    }
    if !app.client.is_connected() {
        return "Tjenesten svarer ikke, så statusen er ukjent.".into();
    }
    let h = locked::helper(app);
    let steps = locked::helper_steps(app);
    match locked::helper_state(app).as_str() {
        "on" => {
            let sim = if h["backend"] == "sim" { " (simulert kort)" } else { "" };
            if real_card_locked(app) {
                "Hjelperen kjører. Denne versjonen endrer ikke det ekte kortet ennå, så alt står som før.".into()
            } else if steps.is_empty() {
                format!("Hjelperen kjører{sim}. Ingen trinn er godkjent ennå, så ingenting kan endres.")
            } else {
                format!("Hjelperen kjører{sim}. Godkjent: {}.", steps.join(", ").to_lowercase())
            }
        }
        "error" if h["message"].as_str().is_some_and(|m| m.contains("annen bruker")) => "Hjelperen er slått på for en annen bruker på maskinen. Slå bryteren av og på for å bruke den selv.".into(),
        "error" => "Hjelperen svarer ikke. Den kan ha stoppet. Trykk Sjekk igjen, eller slå bryteren av og på.".into(),
        "outdated" => "Hjelperen er fra en annen versjon av appen. Slå bryteren av og på igjen for å oppdatere den.".into(),
        _ if !data_dir(app).join("aktiver.sh").exists() => "Filene til hjelperen mangler. Installer appen på nytt med install.sh.".into(),
        _ => "Appen leser bare. Ingenting på skjermkortet kan endres.".into(),
    }
}

/// Det tekniske bak statusen (verktøytipset): bygg, kort og feilmeldingen.
fn state_tip(app: &App) -> Option<String> {
    let h = locked::helper(app);
    let mut parts = Vec::new();
    if let Some(b) = h["build"].as_u64() {
        parts.push(format!("agentic-rigg-helper {} · bygg {b}", h["version"].as_str().unwrap_or("")));
    }
    if let Some(b) = h["backend"].as_str() {
        parts.push(if b == "sim" { "Simulert kort".to_string() } else { "Det ekte kortet (NVML)".to_string() });
    }
    if let Some(m) = h["message"].as_str() {
        parts.push(m.to_string());
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
fn failure(code: i32, err: &str, what: &str) -> Option<Toast> {
    match code {
        0 => None,
        126 => Some(Toast::new("Avbrutt").body("Passordet ble ikke gitt. Ingenting er endret.").ms(3500)),
        127 => Some(Toast::new(&format!("Kunne ikke {what}")).body("Fant ikke pkexec, eller ingen kan spørre om passordet (polkit).").error()),
        _ => Some(Toast::new(&format!("Kunne ikke {what}")).body(if err.is_empty() { "Skriptet feilet." } else { err }).error()),
    }
}

pub(crate) fn toggle(app: &Rc<App>, on: bool) {
    if BUSY.with(Cell::get) {
        return;
    }
    BUSY.with(|b| b.set(true));
    request_render();
    let (program, what) = if on { (data_dir(app).join("aktiver.sh"), "slå på endring") } else { (deactivate_script(app), "slå av endring") };
    let a = app.clone();
    run_pkexec(app, program, vec![], move |code, err| {
        BUSY.with(|b| b.set(false));
        let a2 = a.clone();
        locked::refresh_helper(&a, move || {
            request_render();
            match failure(code, &err, what) {
                Some(t) => a2.toast(t),
                None if on && locked::helper_state(&a2) == "error" => a2.toast(Toast::new("Hjelperen svarer ikke").body("Bryteren er på, men hjelperen startet ikke. Se statusen i Innstillinger › Skjermkortstyring.").error()),
                None if on && real_card_locked(&a2) => a2.toast(Toast::new("Endring er slått på").body("Hjelperen kjører. Denne versjonen endrer ikke det ekte kortet ennå, så alt står som før.").ms(5000)),
                None if on => a2.toast(Toast::new("Endring er slått på").body("Hjelperen kjører. Godkjenn et trinn under før noe kan endres.").ms(5000)),
                None => a2.toast(Toast::new("Endring er slått av").body("Hjelperen er fjernet, og kortet er tilbake på standard.").ms(4000)),
            }
        });
    });
}

/// Eget spørsmål med advarsel før et trinn godkjennes.
pub(crate) fn confirm_step(app: &Rc<App>, step: Step) {
    let title = format!("Godkjenne trinnet {}?", step.name());
    let Some((w, root_box)) = crate::shortcuts::dialog(app, &title, 460, 340) else { return };
    let body = vbox(10, &["dialog-body", "helper-confirm"]);
    let head = hbox(10, &["helper-warn"]);
    head.append(&crate::widgets::Icon::new("triangle-alert", 18));
    let lead = label(&format!("Agentic Rigg får lov til å endre {} på skjermkortet.", step.covers().to_lowercase()), &["helper-warn-title"]);
    lead.set_wrap(true);
    lead.set_hexpand(true);
    head.append(&lead);
    body.append(&head);
    let risk = match step {
        Step::Effekt => "En høyere effektgrense gir mer varme og strømtrekk. En lavere gir litt lavere ytelse.",
        Step::Vifter => "Lavere viftefart gir et varmere kort. Over 85 °C går viftene alltid til 100 %, uansett hva du har valgt.",
        Step::Klokker => "For høye klokker kan gi bildefeil, at spill krasjer eller at maskinen henger. Gå opp i små steg.",
        Step::Lys => "Lyset er ikke laget ennå.",
    };
    for t in [
        risk,
        "Hver endring må bekreftes med Behold innen fristen, ellers rulles den tilbake av seg selv. Lukkes Agentic Rigg helt, går kortet tilbake til standard.",
        "Du blir spurt om passordet ditt. Trinnet kan trekkes tilbake her når som helst.",
    ] {
        let l = label(t, &["helper-confirm-text"]);
        l.set_wrap(true);
        l.set_xalign(0.0);
        body.append(&l);
    }
    let buttons = hbox(8, &["helper-confirm-buttons"]);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Avbryt");
    let ok = gtk::Button::with_label(&format!("Godkjenn {} …", step.name().to_lowercase()));
    ok.add_css_class("destructive");
    buttons.append(&cancel);
    buttons.append(&ok);
    body.append(&buttons);
    root_box.append(&body);
    {
        let w = w.clone();
        cancel.connect_clicked(move |_| w.close());
    }
    {
        let (w, a) = (w.clone(), app.clone());
        ok.connect_clicked(move |_| {
            w.close();
            set_step(&a, step, true);
        });
    }
    w.present();
    cancel.grab_focus();
}

pub(crate) fn set_step(app: &Rc<App>, step: Step, on: bool) {
    let program = root(app).libexec("trinn.sh");
    let a = app.clone();
    let what = if on { "godkjenne trinnet" } else { "trekke tilbake trinnet" };
    run_pkexec(app, program, vec![step.id().into(), if on { "på" } else { "av" }.into()], move |code, err| {
        let a2 = a.clone();
        // Hjelperen leser helper.json på nytt innen ett sekund.
        glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
            let a3 = a2.clone();
            locked::refresh_helper(&a2, move || {
                request_render();
                match failure(code, &err, what) {
                    Some(t) => a3.toast(t),
                    None => a3.toast(Toast::new(&format!("Trinnet {} er {}", step.name(), if on { "godkjent" } else { "trukket tilbake" })).ms(3500)),
                }
            });
        });
    });
}

/// Hvorfor et trinn ikke kan godkjennes nå (`None`: det kan det).
fn step_blocked(app: &App, step: Step) -> Option<&'static str> {
    let h = locked::helper(app);
    if locked::helper_state(app) != "on" {
        return Some("Slå på «Tillat endring av skjermkortet» først.");
    }
    if step == Step::Lys {
        return Some("Lys er ikke laget ennå.");
    }
    if h["backend"] != "sim" && !h["realWrites"].as_bool().unwrap_or(false) {
        return Some("Kommer i en senere versjon. Denne versjonen endrer ikke det ekte kortet, så trinnene kan ikke godkjennes ennå.");
    }
    None
}

fn steps_group(app: &Rc<App>) -> Group {
    let enabled: Vec<String> = locked::helper(app)["steps"].as_array().map(|l| l.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut rows = Vec::new();
    for (i, step) in STEPS.iter().copied().enumerate() {
        let on = enabled.iter().any(|s| s == step.id());
        let ctrl = hbox(8, &[]);
        let blocked = if on { None } else { step_blocked(app, step) };
        // Samme ord som i «Hva skal til?»: Godkjent, Ikke godkjent eller
        // Kommer senere (lys, og alle trinn mot det ekte kortet i 1.2.0).
        let status = match locked::step_status(app, step) {
            (text, true) => crate::widgets::rows::tag(text, Some("check"), "ok"),
            (text, false) => crate::widgets::rows::tag(text, None, "quiet"),
        };
        ctrl.append(&status);
        let b = if on { button(None, Some("Trekk tilbake"), &["sm"]) } else { button(None, Some("Godkjenn …"), &["sm"]) };
        b.set_sensitive(blocked.is_none() && !BUSY.with(Cell::get));
        if let Some(why) = blocked {
            // Verktøytipset må nå fram selv om knappen er av: på raden.
            ctrl.set_tooltip_text(Some(why));
        }
        b.update_property(&[gtk::accessible::Property::Label(&format!("{} {}", if on { "Trekk tilbake" } else { "Godkjenn" }, step.name()))]);
        {
            let a = app.clone();
            b.connect_clicked(move |_| if on { set_step(&a, step, false) } else { confirm_step(&a, step) });
        }
        ctrl.append(&b);
        let title = format!("{}. {}", i + 1, step.name());
        rows.push(row(&title, Some(step.covers()), &ctrl, "trinn godkjenn effekt vifter klokker lys rgb"));
    }
    group(
        "Trinn",
        GroupOpts {
            desc: Some("Hvert trinn godkjennes for seg, med et eget spørsmål og passordet ditt. De kommer i denne rekkefølgen: effekt, vifter, klokker og til slutt lys."),
            note: Some(if steps_later(app) {
                "I denne versjonen er hjelperen bare prøvd mot et simulert kort. Den endrer ikke det ekte kortet ennå, så trinnene kan ikke godkjennes. Det kommer i en senere versjon."
            } else {
                "Trekker du et trinn tilbake, går kortet til standard med en gang."
            }),
            k: "trinn godkjenning steg",
            ..Default::default()
        },
        rows,
    )
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
    sw.update_property(&[gtk::accessible::Property::Label("Tillat endring av skjermkortet")]);
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
        "Tillat endring av skjermkortet",
        Some("Legger inn en liten hjelper med administratorrettigheter, så Agentic Rigg senere kan endre effekt, vifter og klokker. Du blir spurt om passordet ditt. Ingenting endres før du har godkjent et trinn under."),
        &sw,
        "root polkit pkexec hjelper skriving endre overklokking",
    );
    crate::widgets::switchrow::make_row_toggle(&main_row.widget, &sw);

    let status_box = hbox(8, &["helper-status"]);
    let (kind, icon) = match state.as_str() {
        _ if busy => ("quiet", "timer"),
        "on" => ("ok", "shield"),
        "error" => ("danger", "triangle-alert"),
        "outdated" => ("warn", "circle-alert"),
        _ => ("quiet", "lock"),
    };
    let word = match state.as_str() {
        _ if busy => "Venter",
        "on" => "På",
        "error" => "Feil",
        "outdated" => "Slå på igjen",
        _ => "Av",
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
    if matches!(state.as_str(), "error" | "outdated") {
        let again = button(Some("refresh-cw"), Some("Sjekk igjen"), &["sm"]);
        let a = app.clone();
        again.connect_clicked(move |_| locked::refresh_helper(&a, request_render));
        status_box.append(&again);
    }
    let status_row = custom_row(&status_box, &format!("status {}", state_text(app)));
    status_row.widget.add_css_class("set-row");
    status_row.widget.add_css_class("helper-status-row");

    let toggle_group = group(
        "Endring av skjermkortet",
        GroupOpts {
            note: Some("Slår du bryteren av, stoppes hjelperen og fjernes helt, og kortet går tilbake til standard. Skal du avinstallere appen, slår du bryteren av først."),
            k: "bryter av på status",
            ..Default::default()
        },
        vec![main_row, status_row],
    );

    // ---- Hva bryteren legger inn ----
    let kv = KvList::new();
    kv.grid.add_css_class("helper-kv");
    kv.add("Hjelperen", "Et lite program som kjører i bakgrunnen med administratorrettigheter. Det kan bare endre effekt, vifter og klokker, innenfor det kortet selv tillater");
    kv.add("Passord", "Du blir spurt når du slår på bryteren, og hver gang du godkjenner et trinn");
    kv.add("Hvem", "Bare brukeren din. Andre brukere på maskinen slipper ikke til");
    kv.add("Vern", "Hver endring rulles tilbake hvis du ikke trykker Behold innen fristen. Over 85 °C går viftene alltid til 100 %. Lukkes Agentic Rigg helt, går kortet til standard");
    let files = kv.add("Filene", "/usr/local/libexec/agentic-rigg-helper\n/etc/agentic-rigg/helper.json (brukeren din og trinnene)");
    files.add_css_class("mono");
    let paths = kv.add("Skriptene", &rigg_core::paths::tilde(&data_dir(app)));
    paths.add_css_class("mono");
    let mut what = group("Hva bryteren gjør", GroupOpts { whole: true, k: "hjelper root polkit systemd sikkerhet passord", ..Default::default() }, vec![]);
    what.widget.append(&kv.grid);
    what.rows.push(custom_row(&gtk::Box::new(gtk::Orientation::Horizontal, 0), "hjelper root polkit systemd passord vern frist nødregel"));

    vec![toggle_group, steps_group(app), what]
}
