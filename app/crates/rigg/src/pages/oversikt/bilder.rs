//! Skjermbildene for runde 2A (bare i debug-bygget, bare med `RIGG_PROFIL=dev`
//! og `RIGG_SKJERMBILDER_2A=<mappe>`, og bare i en privat økt fra
//! `scripts/okt.sh`). Samme metode som `skjermbilder.rs` (grunnmuren): vinduet
//! tegner seg selv til PNG med sin egen tegner, så ingenting av brukerens
//! skrivebord er med. Egen liste med steg, så de tre agentene i runde 2 ikke
//! skriver i samme fil; hovedøkta kan flytte stegene inn i `skjermbilder.rs`.
//!
//! Sidene registrerer kroker ([`hook`]) for det et skjermbilde trenger å
//! stille inn: pause, markøren, grafen under en rad og filteret.

use crate::state::App;
use gtk::prelude::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// En krok fra en side.
type Hook = Rc<dyn Fn(&str)>;

thread_local! {
    static HOOKS: RefCell<HashMap<&'static str, Hook>> = RefCell::new(HashMap::new());
    static STARTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// En side sier hva den kan stille inn (`"grafer.pause"`, `"sensorer.apne"` …).
pub fn hook(name: &'static str, f: impl Fn(&str) + 'static) {
    HOOKS.with(|h| h.borrow_mut().insert(name, Rc::new(f)));
}

pub(crate) fn call(name: &str, arg: &str) {
    let f = HOOKS.with(|h| h.borrow().get(name).cloned());
    match f {
        Some(f) => f(arg),
        None => eprintln!("RIGG_2A mangler kroken {name}"),
    }
}

enum Step {
    Set(Value),
    Size(i32, i32),
    Go(&'static str),
    Settings(&'static str),
    CloseSettings,
    Hook(&'static str, &'static str),
    Wait(u64),
    Shot(&'static str),
}

fn plan() -> Vec<Step> {
    use Step::*;
    let base = json!({"theme": "light", "palette": "grafitt", "accent": "blue", "highContrast": false, "density": "standard", "fontScale": 100, "sideCollapsed": false, "motion": "off", "chartSync": true, "charts": ["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan"]});
    let mut v = vec![Size(1440, 900), Set(base.clone())];
    // «historikk»: tjenesten med `--backend sim --syntetisk-historikk` (48 t).
    // Ellers: det ekte kortet (NVML, bare lesing) med noen minutter historikk.
    if std::env::var("RIGG_2A_DEL").as_deref() == Ok("historikk") {
        v.push(Set(json!({"windows": {"oversikt": 86400, "sensorer": 172800}, "chartWindow": 172800})));
        v.extend([Go("oversikt"), Wait(3000), Shot("oversikt-24t-lys"), Set(json!({"theme": "dark"})), Wait(800), Shot("oversikt-24t-mork")]);
        v.extend([Set(json!({"theme": "light"})), Wait(600), Hook("oversikt.markor", "0.62"), Wait(500), Shot("oversikt-24t-markor-lys"), Hook("oversikt.markor", "")]);
        v.extend([Go("sensorer"), Wait(1200), Hook("sensorer.apne", "temp"), Wait(2500), Shot("sensorer-graf-48t-lys"), Hook("sensorer.markor", "0.7"), Wait(500), Shot("sensorer-graf-48t-markor-lys")]);
        v.extend([Set(json!({"theme": "dark"})), Wait(800), Shot("sensorer-graf-48t-mork"), Hook("sensorer.apne", "memClock"), Wait(2500), Shot("sensorer-graf-minneklokke-48t-mork")]);
        v.extend([Set(json!({"theme": "light"})), Wait(800), Shot("sensorer-graf-minneklokke-48t-lys"), Hook("sensorer.lukk", "")]);
        v.extend([Go("grafer"), Wait(3000), Shot("grafer-48t-lys"), Set(json!({"theme": "dark"})), Wait(800), Shot("grafer-48t-mork")]);
        v.extend([Hook("grafer.markor", "0.55"), Wait(600), Shot("grafer-48t-markor-alle-mork"), Set(json!({"chartSync": false})), Wait(500), Shot("grafer-48t-markor-en-mork"), Hook("grafer.markor", ""), Set(json!({"chartSync": true, "theme": "light"}))]);
        v.extend([Set(json!({"chartWindow": 3600})), Wait(2500), Hook("grafer.pause", "1"), Wait(800), Shot("grafer-1t-pause-lys"), Set(json!({"theme": "dark"})), Wait(800), Shot("grafer-1t-pause-mork"), Hook("grafer.pause", "0")]);
        v.extend([
            Set(json!({"charts": ["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan", "vram", "cpuLoad", "cpuTemp", "mem", "net"], "chartWindow": 21600})),
            Wait(3000),
            Shot("grafer-alle-6t-mork"),
            Set(json!({"highContrast": true})),
            Wait(1000),
            Shot("grafer-alle-6t-hoykontrast-mork"),
            Set(json!({"highContrast": false, "theme": "light", "charts": ["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan"], "chartWindow": 172800})),
        ]);
        v.extend([Size(640, 760), Wait(900), Size(640, 760), Wait(2500), Shot("grafer-48t-smal-lys"), Go("oversikt"), Wait(2000), Shot("oversikt-24t-smal-lys")]);
    } else {
        v.push(Set(json!({"windows": {"oversikt": 60, "sensorer": 300}, "chartWindow": 60})));
        v.extend([Go("oversikt"), Wait(2500), Shot("oversikt-1min-lys"), Set(json!({"theme": "dark"})), Wait(800), Shot("oversikt-1min-mork")]);
        v.extend([Go("sensorer"), Wait(1800), Shot("sensorer-mork"), Set(json!({"theme": "light"})), Wait(800), Shot("sensorer-lys")]);
        v.extend([Hook("sensorer.apne", "temp"), Wait(2000), Shot("sensorer-graf-5min-lys"), Hook("sensorer.markor", "0.8"), Wait(500), Shot("sensorer-graf-5min-markor-lys"), Hook("sensorer.lukk", "")]);
        v.extend([Hook("sensorer.rull", "bunn"), Wait(800), Shot("sensorer-bunn-lys"), Hook("sensorer.rull", "topp")]);
        v.extend([Hook("sensorer.filter", "temp"), Wait(800), Shot("sensorer-filter-lys"), Hook("sensorer.filter", "kaffe"), Wait(800), Shot("sensorer-filter-kaffe-lys"), Hook("sensorer.filter", "")]);
        v.extend([Go("grafer"), Wait(2500), Shot("grafer-1min-lys"), Hook("grafer.markor", "0.7"), Wait(500), Shot("grafer-1min-markor-lys"), Hook("grafer.markor", ""), Set(json!({"theme": "dark"})), Wait(800), Shot("grafer-1min-mork")]);
        v.extend([Settings("grafer"), Wait(900), Shot("innstillinger-grafer-mork"), Set(json!({"theme": "light"})), Wait(700), Shot("innstillinger-grafer-lys"), CloseSettings]);
        // To ulike størrelser: samme størrelse to ganger endrer ingenting.
        v.extend([Size(660, 760), Go("oversikt"), Wait(900), Size(652, 760), Wait(400), Size(640, 760), Wait(1500), Shot("oversikt-smal-lys")]);
        v.extend([Go("sensorer"), Wait(1200), Shot("sensorer-smal-lys"), Go("grafer"), Wait(1800), Shot("grafer-smal-lys"), Set(json!({"theme": "dark"})), Wait(800), Shot("grafer-smal-mork")]);
    }
    v.extend([Size(1440, 900), Set(base)]);
    v
}

/// Starter planen (én gang per prosess) når `RIGG_SKJERMBILDER_2A` er satt.
pub fn start(app: &Rc<App>) {
    let Some(dir) = std::env::var_os("RIGG_SKJERMBILDER_2A").map(PathBuf::from) else { return };
    if STARTED.with(|s| s.replace(true)) {
        return;
    }
    if !app.profile.is_dev() {
        eprintln!("RIGG_SKJERMBILDER_2A krever RIGG_PROFIL=dev.");
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let steps = Rc::new(plan());
    let a = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(2500), move || run(a, steps, 0, dir));
}

fn run(app: Rc<App>, steps: Rc<Vec<Step>>, i: usize, dir: PathBuf) {
    let Some(step) = steps.get(i) else {
        eprintln!("RIGG_2A ferdig: {}", dir.display());
        app.gtk.quit();
        return;
    };
    let mut wait = 60;
    match step {
        Step::Set(v) => {
            let next = rigg_core::settings::merge(&app.settings(), v).unwrap_or_else(|e| {
                eprintln!("RIGG_2A ugyldig steg: {}", e.0);
                app.settings().clone()
            });
            app.replace_settings(next);
        }
        Step::Size(w, h) => {
            if let Some(win) = app.window() {
                win.unmaximize();
                win.set_default_size(*w, *h);
            }
            wait = 700;
        }
        Step::Go(p) => app.go(p),
        Step::Settings(s) => app.open_settings(s),
        Step::CloseSettings => app.shell().close_settings(),
        Step::Hook(n, a) => call(n, a),
        Step::Wait(ms) => wait = *ms,
        Step::Shot(name) => {
            if let Some(win) = app.window() {
                shot(win.upcast_ref(), &dir.join(format!("{name}.png")));
            }
        }
    }
    gtk::glib::timeout_add_local_once(Duration::from_millis(wait), move || run(app, steps, i + 1, dir));
}

/// Feilsøking (`RIGG_2A_BREDDE=1`): flatene som holder vinduet bredt.
fn wide(w: &gtk::Widget, depth: usize) {
    let min = w.measure(gtk::Orientation::Horizontal, -1).0;
    if min > 250 && w.is_visible() && depth < 40 {
        eprintln!("RIGG_BREDDE {}{} {:?} min={min}", "  ".repeat(depth.min(20)), w.css_name(), w.css_classes());
        let mut c = w.first_child();
        while let Some(ch) = c {
            wide(&ch, depth + 1);
            c = ch.next_sibling();
        }
    }
}

fn shot(win: &gtk::Window, path: &Path) {
    if std::env::var_os("RIGG_2A_BREDDE").is_some() {
        wide(win.upcast_ref(), 0);
    }
    let (w, h) = (win.width(), win.height());
    let paintable = gtk::WidgetPaintable::new(Some(win));
    let snap = gtk::Snapshot::new();
    paintable.snapshot(&snap, w as f64, h as f64);
    let Some(node) = snap.to_node() else { return };
    let Some(renderer) = win.renderer() else { return };
    let tex = renderer.render_texture(&node, Some(&gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32)));
    match tex.save_to_png(path) {
        Ok(()) => eprintln!("RIGG_SKJERMBILDE {} {w}×{h}", path.display()),
        Err(e) => eprintln!("Kunne ikke lagre {}: {e}", path.display()),
    }
}
