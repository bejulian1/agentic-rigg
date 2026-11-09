//! Generelt: oppstart, oppdateringsintervall, tjenesten i bakgrunnen og
//! påskeeggene (produktspesifikasjonen 3.10). «Datakilde» og «Start med
//! simulert GPU» fra demoen finnes ikke lenger.
//!
//! Gruppa «Bakgrunn» ([`bakgrunn`]) eies av 3B i runde 3 (start ved
//! innlogging og stopp nå); resten eies av grunnmuren.

use super::{Group, GroupOpts, group, live, row, save, seg, switch_row};
use crate::state::App;
use gtk::prelude::*;
use rigg_core::fmt as F;
use rigg_core::settings::PAGES;
use serde_json::{Value, json};
use std::rc::Rc;

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    // Startside
    let mut names: Vec<&str> = PAGES.iter().map(|p| p.1).collect();
    names.push("Sist brukte side");
    let dd = gtk::DropDown::from_strings(&names);
    dd.update_property(&[gtk::accessible::Property::Label("Startside")]);
    let ids: Vec<&str> = PAGES.iter().map(|p| p.0).chain(std::iter::once("last")).collect();
    dd.set_selected(ids.iter().position(|i| *i == s.start_page).unwrap_or(0) as u32);
    dd.set_valign(gtk::Align::Center);
    {
        let a = app.clone();
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        dd.connect_selected_notify(move |d| {
            if let Some(id) = ids.get(d.selected() as usize) {
                save(&a, json!({"startPage": id}));
            }
        });
    }
    let interval = seg(app, &[("500", "0,5 s"), ("1000", "1 s"), ("2000", "2 s")], &s.interval_ms.to_string(), "intervalMs", "Oppdateringsintervall", true);
    let found = app.settings().eggs_found.len();
    let eggs_sub = format!(
        "Noen få skjulte detaljer rundt om i appen. De endrer aldri tall, innstillinger eller kortet.{}",
        if found > 0 { format!(" Du har funnet {found} av {}.", rigg_core::easter::EGGS.len()) } else { String::new() }
    );
    let a = app.clone();
    vec![
        // Bakgrunn først: det viktigste valget i appen er hva som skjer når vinduet lukkes.
        bakgrunn(app),
        group("Oppstart", GroupOpts::default(), vec![row("Startside", Some("Siden som åpnes når appen starter."), &dd, "forside start")]),
        group(
            "Skjermkortdata",
            GroupOpts { k: "gpu data nvml", ..Default::default() },
            vec![row(
                "Oppdateringsintervall",
                Some("Hvor ofte tallene og grafene får nye verdier mens vinduet er åpent. 0,5 s koster dobbelt så mye CPU som 1 s, og kortet spørres aldri oftere."),
                &interval.root,
                "intervall frekvens hastighet takt",
            )],
        ),
        group("Annet", GroupOpts::default(), vec![switch_row("Små overraskelser", Some(&eggs_sub), s.surprises, "påskeegg egg", move |on| {
            save(&a, json!({"surprises": on}));
        })]),
    ]
}

// ---------------------------------------------------------------- Bakgrunn (3B)

/// «Kjører siden 09:12 (2 t 14 min) · PID 4242 · 31 MB minne · opptak i
/// bakgrunnen av». Tallene kommer fra `ping`, som hentes hvert 5. s mens
/// raden synes.
pub fn service_status(app: &App) -> String {
    if !app.client.is_connected() {
        return "Svarer ikke. Appen prøver igjen av seg selv.".into();
    }
    let d = app.data.borrow();
    let Some(p) = d.ping.as_ref() else { return "Kjører".into() };
    let mut parts = Vec::new();
    match p["startedAt"].as_i64() {
        Some(start) => {
            let since = F::since(crate::app::local_time(start), crate::app::local_now());
            let up_s = p["t"].as_i64().map(|t| (t - start) as f64 / 1000.0).unwrap_or(0.0);
            parts.push(if up_s >= 60.0 { format!("Kjører siden {since} ({})", F::uptime(Some(up_s))) } else { format!("Kjører siden {since}") });
        }
        None => parts.push("Kjører".into()),
    }
    if let Some(pid) = p["pid"].as_u64() {
        parts.push(format!("PID {pid}"));
    }
    // PSS: eget minne pluss en rettferdig del av de delte bibliotekene.
    if let Some(kb) = p["pssKB"].as_f64().or(p["privateKB"].as_f64()).or(p["rssKB"].as_f64()) {
        parts.push(format!("{} MB minne", F::num(Some(kb / 1024.0), 0)));
    }
    parts.push((if d.service["background"].as_bool() == Some(true) { "opptak i bakgrunnen på" } else { "opptak i bakgrunnen av" }).into());
    if d.log["active"].as_bool() == Some(true) {
        parts.push("logger til fil".into());
    }
    parts.join(" · ")
}

/// Hva som koster hva (målt, `maalinger.md` runde 3B og 3C).
const BAKGRUNN_NOTE: &str = "Tjenesten heter agentic-riggd og kjører som systemd --user. Den leser bare, og skriver aldri til kortet. Alt blir på maskinen: ingenting sendes noe sted.";

const BG_SUB: &str = "Av: ingenting kjører når appen er lukket, og grafene får et hull for den tiden. På: historikken fortsetter til du logger ut, for ca. 21–28\u{202f}MB og 0,85\u{202f}% av én kjerne med 2\u{202f}s\u{2011}takt, og ikonet vises i systemfeltet.";

const LOGIN_SUB: &str = "Tjenesten starter når du logger inn og tar opp historikk før du åpner appen, med ca. 21–28\u{202f}MB minne. Uten «Fortsett i bakgrunnen» stopper den igjen etter 60\u{202f}s.";

const RATE_SUB: &str = "Hvor ofte kortet og maskinen leses uten vindu: 1,5\u{202f}% av én kjerne med 1\u{202f}s, 0,85\u{202f}% med 2\u{202f}s og 0,4\u{202f}% med 5\u{202f}s. Gjelder når bakgrunnsopptaket er på eller en logg går.";

/// «Stopp tjenesten nå»: hva som stoppes, etter om en logg går.
fn stop_sub(app: &App) -> String {
    let d = app.data.borrow();
    if d.service["idleShutdown"].as_bool() == Some(true) {
        return "Stopper når vinduet lukkes. Vinduet starter tjenesten igjen neste gang det åpnes.".into();
    }
    if d.log["active"].as_bool() == Some(true) {
        "Stopper loggen til fil og opptaket i bakgrunnen. Tjenesten avslutter når vinduet lukkes, og vinduet starter den igjen neste gang det åpnes.".into()
    } else {
        "Stopper opptaket i bakgrunnen. Tjenesten avslutter når vinduet lukkes, og vinduet starter den igjen neste gang det åpnes.".into()
    }
}

/// «Stopp tjenesten nå» med angre i 5 s: stopper loggen og ber tjenesten
/// avslutte når siste vindu kobler fra (`shutdown_when_idle`).
fn stop_row(app: &Rc<App>) -> super::Row {
    let btn = gtk::Button::with_label("Stopp nå");
    btn.add_css_class("sm");
    btn.set_valign(gtk::Align::Center);
    let r = row("Stopp tjenesten nå", Some(&stop_sub(app)), &btn, "stopp avslutt tjeneste daemon logging opptak");
    if let Some(sub) = r.widget.first_child().and_then(|t| t.last_child()).and_then(|l| l.downcast::<gtk::Label>().ok()) {
        let b = btn.downgrade();
        live(&sub, move |a| {
            let asked = a.data.borrow().service["idleShutdown"].as_bool() == Some(true);
            if let Some(b) = b.upgrade() {
                b.set_sensitive(a.client.is_connected() && !asked);
            }
            stop_sub(a)
        });
    }
    btn.set_sensitive(app.client.is_connected());
    let a = app.clone();
    btn.connect_clicked(move |b| {
        b.set_sensitive(false);
        let cancelled = Rc::new(std::cell::Cell::new(false));
        let logging = a.data.borrow().log["active"].as_bool() == Some(true);
        let c2 = cancelled.clone();
        let bw = b.downgrade();
        a.toast(
            crate::toast::Toast::new(if logging { "Loggen og tjenesten stoppes" } else { "Tjenesten stoppes" })
                .body("Om 5 s. Tjenesten avslutter når vinduet lukkes.")
                .action("Angre", move || {
                    c2.set(true);
                    if let Some(b) = bw.upgrade() {
                        b.set_sensitive(true);
                    }
                })
                .ms(5000),
        );
        let a = a.clone();
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(5000), move || {
            if cancelled.get() {
                return;
            }
            if a.data.borrow().log["active"].as_bool() == Some(true) {
                a.client.request("log_stop", json!({}), |_| {});
            }
            a.client.request("shutdown_when_idle", json!({}), |_| {});
        });
    });
    r
}

/// Gruppa «Bakgrunn»: status, «Fortsett i bakgrunnen», «Takt i bakgrunnen»
/// og «Start tjenesten ved innlogging».
pub fn bakgrunn(app: &Rc<App>) -> Group {
    let svc: Value = app.data.borrow().service.clone();
    let connected = app.client.is_connected();
    // Status som i demoen: forklaringen under tittelen, og et merke til høyre.
    let tag = crate::widgets::rows::tag(if connected { "Kjører" } else { "Svarer ikke" }, None, if connected { "ok" } else { "warn" });
    let status_row = row("Status", Some(&service_status(app)), &tag, "tjeneste status daemon kjører minne pid oppetid");
    if let Some(tl) = tag.last_child().and_then(|l| l.downcast::<gtk::Label>().ok()) {
        let t2 = tag.clone();
        live(&tl, move |a| {
            let on = a.client.is_connected();
            crate::widgets::class(&t2, "ok", on);
            crate::widgets::class(&t2, "warn", !on);
            (if on { "Kjører" } else { "Svarer ikke" }).to_string()
        });
    }
    // Oppetid og minne holdes ferske mens raden synes (ett `ping` hvert 5. s).
    {
        let timer: Rc<std::cell::RefCell<Option<gtk::glib::SourceId>>> = Rc::default();
        let (a, t) = (app.clone(), timer.clone());
        status_row.widget.connect_map(move |_| {
            if t.borrow().is_none() {
                let a = a.clone();
                *t.borrow_mut() = Some(gtk::glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
                    if a.client.is_connected() {
                        crate::app::ping(&a);
                    }
                    gtk::glib::ControlFlow::Continue
                }));
            }
        });
        status_row.widget.connect_unmap(move |_| {
            if let Some(id) = timer.borrow_mut().take() {
                id.remove();
            }
        });
    }
    let bg_on = svc["background"].as_bool().unwrap_or(false);
    let a = app.clone();
    let bg = switch_row(
        "Fortsett i bakgrunnen",
        Some(BG_SUB),
        bg_on,
        "bakgrunn lukket opptak historikk systemfelt ikon",
        // Bare når valget faktisk er endret (bryteren følger også tjenesten, under).
        move |on| {
            if a.data.borrow().service["background"].as_bool().unwrap_or(false) != on {
                a.set_service(json!({"background": on}));
            }
        },
    );
    let iv = svc["backgroundIntervalMs"].as_u64().unwrap_or(2000).to_string();
    let rate = Rc::new(crate::widgets::Seg::new(&[("1000", "1 s"), ("2000", "2 s"), ("5000", "5 s")], &iv, "Takt i bakgrunnen"));
    {
        let a = app.clone();
        rate.connect(move |v| a.set_service(json!({"backgroundIntervalMs": v.parse::<u64>().unwrap_or(2000)})));
    }
    let rate_row = row(
        "Takt i bakgrunnen",
        Some(RATE_SUB),
        &rate.root,
        "takt intervall bakgrunn",
    );
    let log_on = app.data.borrow().log["active"].as_bool() == Some(true);
    bg.widget.set_sensitive(connected);
    // Takten betyr bare noe når tjenesten går uten vindu.
    rate_row.widget.set_sensitive(connected && (bg_on || log_on));
    // Gruppa bygges ofte før tjenesten har svart. Statuslinja, bryteren og
    // takten følger derfor tjenesten (`live` kalles ved `on_service`,
    // `on_conn` og `ping`), uten å sende noe tilbake.
    if let Some(sub) = status_row.widget.first_child().and_then(|t| t.last_child()).and_then(|l| l.downcast::<gtk::Label>().ok()) {
        let bg_sw = bg.widget.last_child().and_then(|c| c.first_child()).and_then(|w| w.downcast::<gtk::Switch>().ok()).map(|w| w.downgrade());
        let rows = [bg.widget.downgrade(), rate_row.widget.downgrade()];
        let rate = rate.clone();
        live(&sub, move |a| {
            let on = a.client.is_connected();
            let (bg_on, iv) = {
                let d = a.data.borrow();
                (d.service["background"].as_bool(), d.service["backgroundIntervalMs"].as_u64())
            };
            if let (Some(want), Some(sw)) = (bg_on, bg_sw.as_ref().and_then(|w| w.upgrade()))
                && sw.is_active() != want
            {
                sw.set_active(want);
            }
            if let Some(iv) = iv.map(|v| v.to_string())
                && rate.value().as_deref() != Some(iv.as_str())
            {
                rate.set(&iv);
            }
            let log_on = a.data.borrow().log["active"].as_bool() == Some(true);
            for (i, w) in rows.iter().enumerate().filter_map(|(i, w)| w.upgrade().map(|w| (i, w))) {
                let want = on && (i == 0 || bg_on == Some(true) || log_on);
                if w.is_sensitive() != want {
                    w.set_sensitive(want);
                }
            }
            service_status(a)
        });
    }
    group(
        "Bakgrunn",
        GroupOpts { k: "tjeneste daemon systemd bakgrunn bakgrunnstjeneste", note: Some(BAKGRUNN_NOTE), ..Default::default() },
        vec![status_row, bg, rate_row, login_row(app), stop_row(app)],
    )
}

/// «Start tjenesten ved innlogging»: `EnableUnitFiles`/`DisableUnitFiles` til
/// systemd --user over D-Bus. Bryteren viser alltid det systemd svarer, ikke
/// det som ble klikket.
fn login_row(app: &Rc<App>) -> super::Row {
    // Sann mens bryteren settes fra koden, så det ikke blir et nytt kall.
    let quiet = Rc::new(std::cell::Cell::new(true));
    let sw_ref: Rc<std::cell::RefCell<Option<gtk::glib::WeakRef<gtk::Switch>>>> = Rc::default();
    let sub_ref: Rc<std::cell::RefCell<Option<gtk::glib::WeakRef<gtk::Label>>>> = Rc::default();
    let (a, q, s, l) = (app.clone(), quiet.clone(), sw_ref.clone(), sub_ref.clone());
    let r = switch_row("Start tjenesten ved innlogging", Some(LOGIN_SUB), false, "innlogging logg inn oppstart autostart systemd enable", move |on| {
        if q.get() {
            return;
        }
        let Some(sw) = s.borrow().as_ref().and_then(|w| w.upgrade()) else { return };
        sw.set_sensitive(false);
        let (a, q, s, l) = (a.clone(), q.clone(), s.clone(), l.clone());
        login::set(on, move |res| {
            if let Err(msg) = res {
                a.toast(crate::toast::Toast::new(if on { "Kunne ikke slå på start ved innlogging" } else { "Kunne ikke slå av start ved innlogging" }).body(&msg).error());
            }
            login_refresh(&q, &s, &l);
        });
    });
    *sw_ref.borrow_mut() = r.widget.last_child().and_then(|c| c.first_child()).and_then(|w| w.downcast::<gtk::Switch>().ok()).map(|w| w.downgrade());
    *sub_ref.borrow_mut() = r.widget.first_child().and_then(|t| t.last_child()).and_then(|w| w.downcast::<gtk::Label>().ok()).map(|w| w.downgrade());
    if let Some(sw) = sw_ref.borrow().as_ref().and_then(|w| w.upgrade()) {
        sw.set_sensitive(false);
    }
    login_refresh(&quiet, &sw_ref, &sub_ref);
    r
}

/// Spør systemd og setter bryteren og forklaringen etter svaret.
fn login_refresh(quiet: &Rc<std::cell::Cell<bool>>, sw: &Rc<std::cell::RefCell<Option<gtk::glib::WeakRef<gtk::Switch>>>>, sub: &Rc<std::cell::RefCell<Option<gtk::glib::WeakRef<gtk::Label>>>>) {
    let (q, s, l) = (quiet.clone(), sw.clone(), sub.clone());
    login::state(move |st| {
        let Some(sw) = s.borrow().as_ref().and_then(|w| w.upgrade()) else { return };
        q.set(true);
        sw.set_active(st == login::State::Enabled);
        q.set(false);
        sw.set_sensitive(matches!(st, login::State::Enabled | login::State::Disabled));
        if let Some(lbl) = l.borrow().as_ref().and_then(|w| w.upgrade()) {
            let text = login::explain(&st);
            if lbl.text() != text {
                lbl.set_text(&text);
            }
        }
    });
}

/// Samtalen med systemd --user. Utviklingsprofilen bruker en egen enhet, så
/// et utviklingsbygg aldri kan slå på den installerte tjenesten.
mod login {
    use gtk::gio;
    use gtk::glib::{self, variant::ToVariant};

    const DEST: &str = "org.freedesktop.systemd1";
    const PATH: &str = "/org/freedesktop/systemd1";
    const IFACE: &str = "org.freedesktop.systemd1.Manager";

    #[derive(Clone, Debug, PartialEq)]
    pub enum State {
        Enabled,
        Disabled,
        /// Enhetsfila finnes ikke (ikke installert).
        Missing,
        /// Ingen systemd --user på sesjonsbussen.
        NoSystemd,
        /// «masked», «static», «linked» …
        Other(String),
    }

    pub fn unit_for(profile: rigg_core::paths::Profile) -> &'static str {
        if profile.is_dev() { "agentic-riggd-dev.service" } else { rigg_core::paths::UNIT }
    }

    pub fn unit() -> &'static str {
        unit_for(rigg_core::paths::Profile::current())
    }

    pub fn parse(state: &str) -> State {
        match state {
            "enabled" | "enabled-runtime" => State::Enabled,
            "disabled" => State::Disabled,
            other => State::Other(other.to_string()),
        }
    }

    /// Forklaringen under raden for tilstanden.
    pub fn explain(st: &State) -> String {
        match st {
            State::Enabled | State::Disabled => super::LOGIN_SUB.to_string(),
            State::Missing => "Tjenesten er ikke installert som systemd-enhet, så den kan ikke startes ved innlogging. install.sh legger den inn.".to_string(),
            State::NoSystemd => "systemd --user svarer ikke, så tjenesten kan ikke startes ved innlogging.".to_string(),
            State::Other(s) => format!("systemd sier at enheten er «{s}», så den kan ikke slås av eller på herfra."),
        }
    }

    /// «GDBus.Error:org.freedesktop.DBus.Error.AccessDenied: Tekst» → «Tekst».
    pub fn message(e: &glib::Error) -> String {
        let m = e.message();
        match m.strip_prefix("GDBus.Error:").and_then(|r| r.split_once(": ")) {
            Some((_, text)) => text.to_string(),
            None => m.to_string(),
        }
    }

    fn call(method: &str, params: glib::Variant, f: impl FnOnce(Result<glib::Variant, glib::Error>) + 'static) {
        match gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) {
            Ok(c) => c.call(Some(DEST), PATH, IFACE, method, Some(&params), None, gio::DBusCallFlags::NONE, 10_000, None::<&gio::Cancellable>, f),
            Err(e) => f(Err(e)),
        }
    }

    pub fn state(f: impl FnOnce(State) + 'static) {
        call("GetUnitFileState", (unit(),).to_variant(), move |r| {
            f(match r {
                Ok(v) => v.get::<(String,)>().map(|(s,)| parse(&s)).unwrap_or(State::Missing),
                Err(e) if e.matches(gio::DBusError::ServiceUnknown) || e.matches(gio::DBusError::NameHasNoOwner) || e.matches(gio::IOErrorEnum::NotFound) => State::NoSystemd,
                Err(_) => State::Missing,
            })
        });
    }

    /// Slår på eller av, og ber systemd lese enhetene på nytt (som
    /// `systemctl --user enable` gjør).
    pub fn set(on: bool, f: impl FnOnce(Result<(), String>) + 'static) {
        let files = vec![unit().to_string()];
        let (method, params) = if on { ("EnableUnitFiles", (files, false, false).to_variant()) } else { ("DisableUnitFiles", (files, false).to_variant()) };
        call(method, params, move |r| match r {
            Ok(_) => {
                call("Reload", ().to_variant(), |_| {});
                f(Ok(()))
            }
            Err(e) => f(Err(message(&e))),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::login::*;
    use rigg_core::paths::Profile;

    #[test]
    fn enheten_og_tilstandene() {
        assert_eq!(unit_for(Profile::Prod), "agentic-riggd.service");
        assert_eq!(unit_for(Profile::Dev), "agentic-riggd-dev.service", "et utviklingsbygg rører aldri den installerte enheten");
        assert_eq!(parse("enabled"), State::Enabled);
        assert_eq!(parse("enabled-runtime"), State::Enabled);
        assert_eq!(parse("disabled"), State::Disabled);
        assert_eq!(parse("masked"), State::Other("masked".into()));
        assert!(explain(&State::Other("masked".into())).contains("«masked»"));
        assert!(explain(&State::Missing).contains("ikke installert"));
        assert!(explain(&State::Enabled).contains("21–28\u{202f}MB"));
    }

    #[test]
    fn feilmeldingen_uten_dbus_navnet() {
        let e = gtk::glib::Error::new(gtk::gio::IOErrorEnum::Failed, "GDBus.Error:org.freedesktop.DBus.Error.AccessDenied: Ingen tilgang");
        assert_eq!(message(&e), "Ingen tilgang");
        let e = gtk::glib::Error::new(gtk::gio::IOErrorEnum::Failed, "Noe gikk galt");
        assert_eq!(message(&e), "Noe gikk galt");
    }
}
