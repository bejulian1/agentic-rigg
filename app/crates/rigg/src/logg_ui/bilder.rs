//! Skjermbildene til runde 2B (bare i debug-bygget, bare med `RIGG_PROFIL=dev`
//! og `RIGG_SKJERMBILDER_2B=<mappe>`, og bare i en privat økt fra
//! `scripts/okt.sh`). Samme metode som `skjermbilder.rs`: vinduet tegner seg
//! selv til PNG, så ingenting av skrivebordet er med.
//!
//! `RIGG_2B_PLAN` velger planen: `kort` (det ekte kortet: Grafikkort og
//! System i lyst og mørkt, rapporten, loggingen, 640 px), eller en tilstand
//! mot den simulerte backenden: `ingen-driver`, `driver-oppdatert`, `borte`,
//! `to-kort`, `dvale` og `bygg` (de to siste setter tilstanden i vinduet, fordi
//! en privat økt ikke har logind og tjenesten er fra samme bygg).
//!
//! Ligger her fordi `skjermbilder.rs` eies av grunnmuren; kan flyttes inn dit.

use crate::state::App;
use gtk::prelude::*;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

enum Step {
    Set(Value),
    Size(i32, i32),
    Go(&'static str),
    Settings(&'static str),
    CloseSettings,
    Wait(u64),
    Shot(&'static str),
    /// Ruller siden som vises (0,0 = toppen, 1,0 = bunnen).
    Scroll(f64),
    /// Lagre rapporten som tekst og Markdown i mappa (eksempel til dokumentasjonen).
    Report,
    /// Popoveren «Lagre rapport» som eget bilde.
    ReportPopover(&'static str),
    LogStart,
    LogStop,
    /// Vinduets egne tilstander uten tjeneste-støtte (dvale, annet bygg).
    Inject(&'static str),
    /// Kort nummer to i kortvelgeren.
    SecondCard,
    /// Kortvelgerens popover som eget bilde.
    DevicePopover(&'static str),
}

fn base() -> Value {
    json!({"theme": "light", "palette": "grafitt", "accent": "blue", "highContrast": false, "density": "standard", "fontScale": 100, "sideCollapsed": false, "motion": "off"})
}

fn plan(name: &str) -> Vec<Step> {
    use Step::*;
    let mut p = vec![Size(1440, 900), Set(base())];
    match name {
        "kort" => p.extend([
            Go("grafikkort"),
            Wait(1500),
            Shot("grafikkort-lys"),
            Scroll(1.0),
            Wait(500),
            Shot("grafikkort-lys-nederst"),
            Scroll(0.0),
            Set(json!({"theme": "dark"})),
            Wait(700),
            Shot("grafikkort-mork"),
            Scroll(1.0),
            Wait(500),
            Shot("grafikkort-mork-nederst"),
            Scroll(0.0),
            Go("system"),
            Wait(1500),
            Shot("system-mork"),
            Scroll(1.0),
            Wait(500),
            Shot("system-mork-nederst"),
            Scroll(0.0),
            Set(json!({"theme": "light"})),
            Wait(700),
            Shot("system-lys"),
            Scroll(1.0),
            Wait(500),
            Shot("system-lys-nederst"),
            Scroll(0.0),
            ReportPopover("rapport-lagre-lys"),
            Report,
            Settings("om"),
            Wait(900),
            Shot("innstillinger-om-rapport-lys"),
            CloseSettings,
            Size(1920, 1080),
            Go("grafikkort"),
            Wait(900),
            Shot("grafikkort-1920-lys"),
            Size(1440, 900),
            LogStart,
            Wait(3500),
            Go("sensorer"),
            Wait(1200),
            Shot("logging-sensorer-sidefelt-lys"),
            Settings("logging"),
            Wait(900),
            Shot("logging-innstillinger-aktiv-lys"),
            Set(json!({"theme": "dark"})),
            Wait(700),
            Shot("logging-innstillinger-aktiv-mork"),
            CloseSettings,
            LogStop,
            Wait(1200),
            Set(json!({"theme": "light"})),
            Settings("logging"),
            Wait(900),
            Shot("logging-innstillinger-av-lys"),
            CloseSettings,
            Size(640, 720),
            Go("grafikkort"),
            Wait(700),
            Size(640, 720),
            Wait(900),
            Shot("grafikkort-640-lys"),
            Go("system"),
            Wait(900),
            Shot("system-640-lys"),
            Set(json!({"theme": "dark"})),
            Wait(700),
            Shot("system-640-mork"),
            Size(1440, 900),
        ]),
        "ingen-driver" => p.extend([Go("grafikkort"), Wait(1500), Shot("tilstand-ingen-driver-grafikkort"), Go("oversikt"), Wait(900), Shot("tilstand-ingen-driver-oversikt")]),
        "driver-oppdatert" => p.extend([Go("grafikkort"), Wait(2500), Shot("tilstand-driver-oppdatert-grafikkort"), Set(json!({"theme": "dark"})), Wait(700), Shot("tilstand-driver-oppdatert-grafikkort-mork")]),
        // Scenarioet «borte»: kortet faller av etter 15 s og oppdages ved neste sjekk (hvert 10. s).
        "borte" => p.extend([Go("grafikkort"), Wait(21_000), Shot("tilstand-borte-grafikkort"), DevicePopover("tilstand-borte-kortvelger")]),
        "to-kort" => p.extend([Go("grafikkort"), Wait(1500), DevicePopover("to-kort-kortvelger"), SecondCard, Wait(1200), Shot("to-kort-grafikkort-kort2")]),
        "dvale" => p.extend([Go("grafikkort"), Wait(1500), Inject("dvale"), Wait(600), Shot("tilstand-dvale-grafikkort")]),
        "bygg" => p.extend([Go("grafikkort"), Wait(1500), Inject("bygg"), Wait(900), Shot("tilstand-annet-bygg")]),
        // Klikk på et skrivebordsvarsel: skriptet kaller `app.vis-side("sensorer")` over D-Bus imens.
        "vis-side" => p.extend([Go("grafikkort"), Wait(7000), Shot("varsel-klikk-apner-sensorer")]),
        _ => {}
    }
    p
}

pub fn start(app: &Rc<App>) {
    let Some(dir) = std::env::var_os("RIGG_SKJERMBILDER_2B").map(PathBuf::from) else { return };
    if !app.profile.is_dev() {
        eprintln!("RIGG_SKJERMBILDER_2B krever RIGG_PROFIL=dev.");
        return;
    }
    let name = std::env::var("RIGG_2B_PLAN").unwrap_or_else(|_| "kort".into());
    let _ = std::fs::create_dir_all(&dir);
    let steps = Rc::new(plan(&name));
    let a = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || run(a, steps, 0, dir));
}

/// Første etterkommer som passer.
fn find(w: &gtk::Widget, pred: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if pred(w) {
        return Some(w.clone());
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        if let Some(x) = find(&ch, pred) {
            return Some(x);
        }
        c = ch.next_sibling();
    }
    None
}

fn shot_widget(w: &gtk::Widget, renderer: &gsk::Renderer, path: &std::path::Path) {
    let (wd, ht) = (w.width(), w.height());
    if wd <= 0 || ht <= 0 {
        eprintln!("RIGG_SKJERMBILDE tomt: {}", path.display());
        return;
    }
    let paintable = gtk::WidgetPaintable::new(Some(w));
    let snap = gtk::Snapshot::new();
    paintable.snapshot(&snap, wd as f64, ht as f64);
    let Some(node) = snap.to_node() else { return };
    let tex = renderer.render_texture(&node, Some(&gtk::graphene::Rect::new(0.0, 0.0, wd as f32, ht as f32)));
    match tex.save_to_png(path) {
        Ok(()) => eprintln!("RIGG_SKJERMBILDE {} {wd}×{ht}", path.display()),
        Err(e) => eprintln!("Kunne ikke lagre {}: {e}", path.display()),
    }
}

use gtk::gsk;

fn run(app: Rc<App>, steps: Rc<Vec<Step>>, i: usize, dir: PathBuf) {
    let Some(step) = steps.get(i) else {
        eprintln!("RIGG_SKJERMBILDER_2B ferdig: {}", dir.display());
        app.gtk.quit();
        return;
    };
    let mut wait = 50;
    let win = app.window();
    match step {
        Step::Set(v) => {
            let next = rigg_core::settings::merge(&app.settings(), v).unwrap_or_else(|_| app.settings().clone());
            app.replace_settings(next);
        }
        Step::Size(w, h) => {
            if let Some(win) = &win {
                win.unmaximize();
                win.set_default_size(*w, *h);
            }
            wait = 600;
        }
        Step::Go(p) => app.go(p),
        Step::Settings(s) => app.open_settings(s),
        Step::CloseSettings => app.shell().close_settings(),
        Step::Wait(ms) => wait = *ms,
        Step::Shot(name) => {
            if let Some(win) = &win
                && let Some(r) = win.renderer()
            {
                shot_widget(win.upcast_ref(), &r, &dir.join(format!("{name}.png")));
            }
        }
        Step::Scroll(f) => {
            if let Some(win) = &win {
                let page = app.page.borrow().clone();
                let cls = if page == "system" { "sy-page" } else { "gc-page" };
                if let Some(sc) = find(win.upcast_ref(), &|w| w.is::<gtk::ScrolledWindow>() && w.is_drawable() && find(w, &|c| c.has_css_class(cls)).is_some()) {
                    let sc = sc.downcast::<gtk::ScrolledWindow>().unwrap();
                    let adj = sc.vadjustment();
                    adj.set_value(adj.lower() + (adj.upper() - adj.page_size() - adj.lower()) * f);
                }
            }
        }
        Step::Report => {
            let d = dir.clone();
            crate::rapport_ui::gather(&app, false, move |input| {
                for fmt in [rigg_core::report::ReportFormat::Text, rigg_core::report::ReportFormat::Markdown] {
                    let p = d.join(format!("rapport-eksempel.{}", fmt.ext()));
                    let _ = std::fs::write(&p, rigg_core::report::render(&input, fmt));
                    eprintln!("RIGG_SKJERMBILDE {}", p.display());
                }
            });
            wait = 800;
        }
        Step::ReportPopover(name) => {
            if let Some(win) = &win
                && let Some(btn) = find(win.upcast_ref(), &|w| w.is::<gtk::Button>() && w.is_drawable() && w.tooltip_text().as_deref() == Some("Lagre rapport …"))
            {
                crate::rapport_ui::save_popover(&app, &btn);
                let (d, name, w2) = (dir.clone(), *name, win.clone());
                gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
                    if let Some(p) = find(w2.upcast_ref(), &|w| w.is::<gtk::Popover>() && w.is_visible())
                        && let Some(r) = p.clone().downcast::<gtk::Popover>().ok().and_then(|p| p.renderer())
                    {
                        shot_widget(&p, &r, &d.join(format!("{name}.png")));
                        p.downcast::<gtk::Popover>().unwrap().popdown();
                    }
                });
                wait = 1200;
            }
        }
        Step::DevicePopover(name) => {
            if let Some(win) = &win
                && let Some(t) = find(win.upcast_ref(), &|w| w.is::<gtk::ToggleButton>() && w.is_drawable() && w.has_css_class("device-card"))
                    .or_else(|| find(win.upcast_ref(), &|w| w.is::<gtk::ToggleButton>() && w.is_drawable()))
            {
                let t = t.downcast::<gtk::ToggleButton>().unwrap();
                t.set_active(true);
                let (d, name, w2) = (dir.clone(), *name, win.clone());
                gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
                    if let Some(p) = find(w2.upcast_ref(), &|w| w.is::<gtk::Popover>() && w.is_visible())
                        && let Some(r) = p.clone().downcast::<gtk::Popover>().ok().and_then(|p| p.renderer())
                    {
                        shot_widget(&p, &r, &d.join(format!("{name}.png")));
                        p.downcast::<gtk::Popover>().unwrap().popdown();
                    }
                    t.set_active(false);
                });
                wait = 1200;
            }
        }
        Step::LogStart => {
            super::start(&app);
            wait = 800;
        }
        Step::LogStop => {
            super::stop(&app);
            wait = 800;
        }
        Step::SecondCard => {
            let id = app.data.borrow().gpus.iter().filter(|g| !g.id.is_empty()).nth(1).map(|g| g.id.clone());
            if let Some(id) = id {
                app.data.borrow_mut().gpu = Some(id);
                app.on_devices.emit(&());
                app.on_telemetry.emit(&());
            }
        }
        Step::Inject(what) => {
            match *what {
                "dvale" => {
                    for g in app.data.borrow_mut().gpus.iter_mut() {
                        g.state = rigg_core::model::DeviceState::Suspended;
                        g.since = rigg_core::now_ms();
                    }
                }
                _ => {
                    if let Some(h) = app.data.borrow_mut().hello.as_mut() {
                        h["daemon"]["build"] = json!(rigg_core::version::build() - 1);
                    }
                }
            }
            app.on_devices.emit(&());
        }
    }
    gtk::glib::timeout_add_local_once(Duration::from_millis(wait), move || run(app, steps, i + 1, dir));
}
