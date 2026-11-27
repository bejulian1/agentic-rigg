//! Skjermbilder og målinger av Kontroller og Viftekurve (runde 6C). Bare med `RIGG_PROFIL=dev` og `RIGG_BILDER_2C=<mappe>`, i en privat
//! økt (`scripts/okt.sh`), og bare når vinduet startes med `--side kontroller`
//! (planen starter når Kontroller bygges). Finnes også i release-bygget,
//! fordi målingene i `maalinger.md` skal gjøres med det.
//!
//! Planene (`RIGG_6C_PLAN`):
//! - `ekte`: det ekte kortet (NVML, bare lesing, låst). Begge sidene i lyst
//!   og mørkt, 1920 og 640 px, (i) og «Hva skal til?», et utkast som ikke
//!   kan sendes, og CPU i ro på hver side (`RIGG_2C_MAAL`). Uten
//!   `RIGG_6C_PLAN` velges `ekte` når skriving er låst.
//! - `sim`: `--backend sim`. Utkast, Bruk → Behold, Bruk → Rull tilbake og
//!   Tilbakestill alt, på begge sidene.
//! - `7b-ekte` og `7b-sim` (runde 7B): Vifter med viftemotoren. Låst mot det
//!   ekte kortet (utkast med stille flate, per vifte, hysterese, °F, høy
//!   kontrast, 640 og 1920 px, CPU i ro), og mot `sim:spill` med kurven i
//!   bruk (Bruk → Behold), per vifte og hysterese.
//!
//! Planen trykker på knappene slik en bruker gjør (finner dem på teksten
//! eller verktøytipset), og tegner vinduet til PNG med `GtkWidgetPaintable`
//! og vinduets egen tegner, som `skjermbilder.rs`. `ShotPage` tegner hele
//! siden, også det som ligger under kanten av vinduet.

use crate::state::App;
use crate::widgets::locked;
use gtk::prelude::*;
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

enum Step {
    Set(Value),
    Size(i32, i32),
    Go(&'static str),
    Wait(u64),
    Shot(&'static str),
    /// Hele siden (klassen på sidens rot), også under kanten av vinduet.
    ShotPage(&'static str, &'static str),
    /// Klikk på knappen med denne teksten («Behold*»: teksten begynner med).
    Click(&'static str),
    /// Popoveren fra knappen med denne teksten eller dette verktøytipset.
    Popover(&'static str, &'static str),
    Eval(fn(&Rc<App>)),
    /// CPU og bilder tegnet mens vinduet står i ro (ms).
    Idle(&'static str, u64),
}

thread_local! {
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

fn base() -> Value {
    json!({"theme": "light", "palette": "grafitt", "accent": "blue", "highContrast": false, "density": "standard", "fontScale": 100, "sideCollapsed": false, "motion": "off", "dismissed": []})
}

/// Et utkast med tre endringer (og fast viftefart), lagd slik glidere gjør det.
fn draft_kontroller(app: &Rc<App>) {
    let b = crate::pages::kontroller::read(app).values();
    locked::edit_draft(app, b, |d| {
        d.power_limit_w = 260.0;
        d.core_offset_mhz = 75.0;
        d.mem_offset_mhz = 500.0;
    });
}

/// Et mindre utkast (etter at det første er brukt i sim).
fn draft_kontroller_2(app: &Rc<App>) {
    let b = crate::pages::kontroller::read(app).values();
    locked::edit_draft(app, b, |d| {
        d.power_limit_w = 220.0;
        d.core_offset_mhz = 50.0;
    });
}

fn draft_vifte(app: &Rc<App>) {
    let b = crate::pages::kontroller::read(app).values();
    locked::edit_draft(app, b, |d| {
        d.fan_mode = rigg_core::txn::FanMode::Curve;
        crate::pages::viftekurve::PRESETS[2].apply(d);
    });
}

// ---- Runde 7B: Vifter ----

fn vifte_base(app: &Rc<App>) -> rigg_core::txn::Values {
    crate::pages::viftekurve::card_values(app)
}

/// Stille: minste fart til 55 °C, 5 °C hysterese, rolig rampe, 5 s utjevning.
fn draft_7b_stille(app: &Rc<App>) {
    locked::edit_draft(app, vifte_base(app), |d| {
        d.fan_mode = rigg_core::txn::FanMode::Curve;
        crate::pages::viftekurve::PRESETS[0].apply(d);
    });
}

/// Egen kurve per vifte: vifte 2 følger Ytelse-kurven.
fn draft_7b_per_vifte(app: &Rc<App>) {
    locked::edit_draft(app, vifte_base(app), |d| {
        d.fan_mode = rigg_core::txn::FanMode::Curve;
        crate::pages::viftekurve::split_curves(d, 2);
        d.fan_curves[1] = crate::pages::viftekurve::PRESETS[2].curve.to_vec();
    });
}

/// Bred hysterese, så båndet synes godt.
fn draft_7b_hysterese(app: &Rc<App>) {
    locked::edit_draft(app, vifte_base(app), |d| {
        d.fan_mode = rigg_core::txn::FanMode::Curve;
        d.fan_hysteresis_c = 8.0;
        d.fan_ramp_down_pct_s = 2.0;
    });
}

/// Det ekte kortet (låst): Vifter i alle variantene, og CPU i ro.
fn plan_7b_ekte() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1920, 1080),
        Set(base()),
        Eval(no_draft),
        Go("viftekurve"),
        Wait(3000),
        Shot("ekte-1920-lys"),
        ShotPage("viftekurve", "ekte-1920-hel-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("ekte-1920-mork"),
        Eval(draft_7b_stille),
        Wait(900),
        Shot("ekte-utkast-stille-1920-mork"),
        ShotPage("viftekurve", "ekte-utkast-stille-1920-hel-mork"),
        Eval(no_draft),
        Eval(draft_7b_per_vifte),
        Wait(600),
        Click("Vifte 2"),
        Wait(900),
        Shot("ekte-per-vifte-1920-mork"),
        Set(json!({"theme": "light"})),
        Wait(800),
        Shot("ekte-per-vifte-1920-lys"),
        Click("Felles kurve"),
        Eval(no_draft),
        Eval(draft_7b_hysterese),
        Wait(900),
        Shot("ekte-hysterese-1920-lys"),
        Set(json!({"unit": "F"})),
        Wait(900),
        Shot("ekte-fahrenheit-1920-lys"),
        Set(json!({"unit": "C", "highContrast": true})),
        Wait(900),
        Shot("ekte-hoy-kontrast-1920-lys"),
        Set(json!({"theme": "dark"})),
        Wait(900),
        Shot("ekte-hoy-kontrast-1920-mork"),
        Set(json!({"theme": "light", "highContrast": false})),
        Eval(no_draft),
        Eval(draft_7b_stille),
        Size(640, 759),
        Wait(1200),
        Size(640, 760),
        Wait(1500),
        Shot("ekte-640-lys"),
        ShotPage("viftekurve", "ekte-640-hel-lys"),
        Set(json!({"theme": "dark"})),
        Wait(900),
        Shot("ekte-640-mork"),
        ShotPage("viftekurve", "ekte-640-hel-mork"),
        Set(json!({"theme": "light"})),
        Eval(no_draft),
        // CPU i ro, i vanlig størrelse og med bevegelse som standard.
        Size(1440, 900),
        Set(json!({"motion": "full"})),
        Wait(5000),
        Idle("vifter-i-ro", 60_000),
        Eval(draft_7b_per_vifte),
        Wait(3000),
        Idle("vifter-i-ro-med-utkast", 60_000),
        Eval(no_draft),
    ]
}

/// `sim:spill`: kurven brukt og beholdt, så viftene følger motoren.
fn plan_7b_sim() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1920, 1080),
        Set(base()),
        Eval(no_draft),
        Go("viftekurve"),
        Wait(3000),
        Shot("sim-auto-1920-lys"),
        Eval(draft_7b_stille),
        Wait(900),
        Shot("sim-utkast-stille-1920-lys"),
        Click("Bruk"),
        Wait(1800),
        Shot("sim-behold-1920-lys"),
        Click("Behold*"),
        // To og et halvt minutt med kurven i bruk: forhåndsvisningen og grafene viser motoren.
        Wait(150_000),
        Shot("sim-kurve-i-bruk-1920-lys"),
        ShotPage("viftekurve", "sim-kurve-i-bruk-1920-hel-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("sim-kurve-i-bruk-1920-mork"),
        Idle("vifter-i-ro-sim-kurve", 60_000),
        Eval(draft_7b_per_vifte),
        Wait(600),
        Click("Vifte 2"),
        Wait(900),
        Shot("sim-per-vifte-utkast-1920-mork"),
        Click("Bruk"),
        Wait(1800),
        Click("Behold*"),
        Wait(90_000),
        Shot("sim-per-vifte-i-bruk-1920-mork"),
        ShotPage("viftekurve", "sim-per-vifte-i-bruk-1920-hel-mork"),
        Set(json!({"theme": "light"})),
        Wait(800),
        Shot("sim-per-vifte-i-bruk-1920-lys"),
        Click("Felles kurve"),
        Wait(600),
        Eval(draft_7b_hysterese),
        Wait(900),
        Shot("sim-hysterese-utkast-1920-lys"),
        Size(640, 759),
        Wait(1200),
        Size(640, 760),
        Wait(1500),
        Shot("sim-640-lys"),
        ShotPage("viftekurve", "sim-640-hel-lys"),
        Set(json!({"theme": "dark", "highContrast": true})),
        Wait(900),
        Shot("sim-640-hoy-kontrast-mork"),
        Set(json!({"theme": "light", "highContrast": false})),
        Eval(no_draft),
        Size(1440, 900),
        Wait(600),
    ]
}

fn no_draft(app: &Rc<App>) {
    locked::update(app, "utkast", |s| s.utkast = None);
    // Disse eldre planene prøver eksplisitt hele kurveverktøyet.
    locked::set_fan_advanced(app, true);
}

/// Det ekte kortet: låst. Bilder og CPU i ro.
fn plan_ekte() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1920, 1080),
        Set(base()),
        Eval(no_draft),
        Go("kontroller"),
        Wait(2500),
        Shot("kontroller-1920-lys"),
        ShotPage("kontroller", "kontroller-1920-hel-lys"),
        Popover("Mer om effektgrense", "kontroller-info-effektgrense-lys"),
        Popover("Hva skal til?", "hva-skal-til-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("kontroller-1920-mork"),
        Popover("Mer om gpu-justering", "kontroller-info-kjerneklokke-mork"),
        Eval(draft_kontroller),
        Wait(700),
        Shot("kontroller-utkast-1920-mork"),
        Set(json!({"theme": "light"})),
        Wait(700),
        Shot("kontroller-utkast-1920-lys"),
        Click("Gjenopprett utgangspunkt"),
        Wait(900),
        Shot("kontroller-tilbakestill-alt-1920-lys"),
        Eval(no_draft),
        Go("viftekurve"),
        Wait(2500),
        Shot("viftekurve-1920-lys"),
        ShotPage("viftekurve", "viftekurve-1920-hel-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("viftekurve-1920-mork"),
        Eval(draft_vifte),
        Wait(800),
        Shot("viftekurve-utkast-1920-mork"),
        Set(json!({"theme": "light"})),
        Wait(700),
        Shot("viftekurve-utkast-1920-lys"),
        Eval(no_draft),
        Size(640, 759),
        Go("kontroller"),
        Wait(1200),
        Size(640, 760),
        Wait(1200),
        Shot("kontroller-640-lys"),
        ShotPage("kontroller", "kontroller-640-hel-lys"),
        Set(json!({"theme": "dark"})),
        Wait(700),
        Shot("kontroller-640-mork"),
        Go("viftekurve"),
        Wait(1500),
        Shot("viftekurve-640-mork"),
        ShotPage("viftekurve", "viftekurve-640-hel-mork"),
        Set(json!({"theme": "light"})),
        Wait(700),
        Shot("viftekurve-640-lys"),
        // CPU i ro, i vanlig størrelse og med bevegelse som standard.
        Size(1440, 900),
        Set(json!({"motion": "full"})),
        Go("kontroller"),
        Wait(4000),
        Idle("kontroller-i-ro", 60_000),
        Go("viftekurve"),
        Wait(4000),
        Idle("viftekurve-i-ro", 60_000),
        Eval(draft_kontroller),
        Go("kontroller"),
        Wait(3000),
        Idle("kontroller-i-ro-med-utkast", 20_000),
        Eval(no_draft),
    ]
}

/// Den simulerte backenden: utkast, Bruk, Behold og Rull tilbake.
fn plan_sim() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1920, 1080),
        Set(base()),
        Eval(no_draft),
        Go("kontroller"),
        Wait(2500),
        Shot("sim-kontroller-1920-lys"),
        Eval(draft_kontroller),
        Wait(700),
        Shot("sim-kontroller-utkast-1920-lys"),
        Click("Bruk"),
        Wait(1800),
        Shot("sim-kontroller-behold-1920-lys"),
        Click("Behold*"),
        Wait(3000),
        Shot("sim-kontroller-beholdt-1920-lys"),
        Click("Gjenopprett utgangspunkt"),
        Wait(900),
        Shot("sim-kontroller-tilbakestill-alt-1920-lys"),
        Set(json!({"theme": "dark"})),
        Wait(700),
        Shot("sim-kontroller-tilbakestill-alt-1920-mork"),
        Click("Bruk"),
        Wait(1800),
        Shot("sim-kontroller-behold-1920-mork"),
        Click("Rull tilbake"),
        Wait(3000),
        Shot("sim-kontroller-rullet-tilbake-1920-mork"),
        Eval(no_draft),
        Go("viftekurve"),
        Wait(2500),
        Eval(draft_vifte),
        Wait(800),
        Shot("sim-viftekurve-utkast-1920-mork"),
        Click("Bruk"),
        Wait(1800),
        Shot("sim-viftekurve-behold-1920-mork"),
        Click("Behold*"),
        Wait(5000),
        Set(json!({"theme": "light"})),
        Wait(800),
        Shot("sim-viftekurve-beholdt-1920-lys"),
        Click("Manuell"),
        Wait(800),
        Shot("sim-viftekurve-manuell-1920-lys"),
        Eval(no_draft),
        Size(640, 759),
        Go("kontroller"),
        Wait(1200),
        Size(640, 760),
        Eval(draft_kontroller_2),
        Wait(1200),
        Shot("sim-kontroller-utkast-640-lys"),
        ShotPage("kontroller", "sim-kontroller-utkast-640-hel-lys"),
        Eval(no_draft),
        Size(1440, 900),
        Wait(600),
    ]
}

/// Starter planen (én gang) når Kontroller bygges.
pub fn maybe_start(app: &Rc<App>) {
    let Some(dir) = std::env::var_os("RIGG_BILDER_2C").map(PathBuf::from) else { return };
    if !app.profile.is_dev() || STARTED.with(|s| s.replace(true)) {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let a = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(2500), move || {
        // Planen velges når tjenesten har svart (porten avgjør ekte eller sim).
        let name = std::env::var("RIGG_6C_PLAN").unwrap_or_else(|_| if locked::gate(&a) == rigg_core::txn::Gate::Sim { "sim".into() } else { "ekte".into() });
        let steps = match name.as_str() {
            "sim" => plan_sim(),
            "7b-ekte" => plan_7b_ekte(),
            "7b-sim" => plan_7b_sim(),
            _ => plan_ekte(),
        };
        eprintln!("RIGG_BILDER_2C plan {name}: {} steg", steps.len());
        run(a, Rc::new(steps), 0, dir);
    });
}

/// Knappen med denne teksten (eller dette verktøytipset). «Behold*»: teksten
/// begynner med «Behold».
fn find_button(w: &gtk::Widget, text: &str) -> Option<gtk::Button> {
    if w.is_visible() && w.is_mapped() {
        if let Some(b) = w.downcast_ref::<gtk::Button>()
            && b.is_sensitive()
            && (matches(b.label().as_deref(), text) || descendant_label(w, text) || b.tooltip_text().is_some_and(|t| t.starts_with(text)))
        {
            return Some(b.clone());
        }
        let mut c = w.first_child();
        while let Some(ch) = c {
            if let Some(b) = find_button(&ch, text) {
                return Some(b);
            }
            c = ch.next_sibling();
        }
    }
    None
}

fn matches(label: Option<&str>, text: &str) -> bool {
    match (label, text.strip_suffix('*')) {
        (Some(l), Some(prefix)) => l.starts_with(prefix),
        (Some(l), None) => l == text,
        _ => false,
    }
}

fn descendant_label(w: &gtk::Widget, text: &str) -> bool {
    let mut c = w.first_child();
    while let Some(ch) = c {
        if ch.downcast_ref::<gtk::Label>().is_some_and(|l| matches(Some(l.text().as_str()), text)) || descendant_label(&ch, text) {
            return true;
        }
        c = ch.next_sibling();
    }
    false
}

fn find_class(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if w.is_mapped() && w.has_css_class("page") && w.has_css_class(class) {
        return Some(w.clone());
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        if let Some(f) = find_class(&ch, class) {
            return Some(f);
        }
        c = ch.next_sibling();
    }
    None
}

fn run(app: Rc<App>, steps: Rc<Vec<Step>>, i: usize, dir: PathBuf) {
    let Some(step) = steps.get(i) else {
        eprintln!("RIGG_BILDER_2C ferdig: {}", dir.display());
        app.gtk.quit();
        return;
    };
    // Klokkeslett for hvert steg (`steg.txt`), så P-tilstanden kan legges ved siden av.
    {
        use std::io::Write;
        let what = match step {
            Step::Go(p) => format!("side {p}"),
            Step::Size(w, h) => format!("størrelse {w}×{h}"),
            Step::Idle(n, ms) => format!("i ro {n} ({} s)", ms / 1000),
            Step::Shot(n) | Step::ShotPage(_, n) => format!("bilde {n}"),
            Step::Click(t) => format!("klikk {t}"),
            _ => String::new(),
        };
        if !what.is_empty()
            && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("steg.txt"))
        {
            let now = gtk::glib::DateTime::now_local().ok().and_then(|d| d.format("%H:%M:%S").ok()).map(|s| s.to_string()).unwrap_or_default();
            let _ = writeln!(f, "{now} {what}");
        }
    }
    let mut wait = 60;
    let next = {
        let (a, s, d) = (app.clone(), steps.clone(), dir.clone());
        move |ms: u64| {
            let (a, s, d) = (a.clone(), s.clone(), d.clone());
            gtk::glib::timeout_add_local_once(Duration::from_millis(ms), move || run(a, s, i + 1, d));
        }
    };
    match step {
        Step::Set(v) => {
            let n = rigg_core::settings::merge(&app.settings(), v).unwrap_or_else(|_| app.settings().clone());
            app.replace_settings(n);
        }
        Step::Size(w, h) => {
            if let Some(win) = app.window() {
                win.unmaximize();
                win.set_default_size(*w, *h);
                if std::env::var_os("RIGG_BREDDE").is_some() && *w < 700 {
                    let win2 = win.clone();
                    let h = *h;
                    gtk::glib::timeout_add_local_once(Duration::from_millis(600), move || {
                        let w: &gtk::Widget = win2.upcast_ref();
                        eprintln!("RIGG_BREDDE etter størrelse: {}×{}, min {:?}, min for høyden {:?}", w.width(), w.height(), w.measure(gtk::Orientation::Horizontal, -1).0, w.measure(gtk::Orientation::Horizontal, h).0);
                        wide(w, 0);
                    });
                }
            }
            wait = 700;
        }
        Step::Go(p) => app.go(p),
        Step::Wait(ms) => wait = *ms,
        Step::Click(t) => match app.window().and_then(|w| find_button(w.upcast_ref(), t)) {
            Some(b) => {
                b.emit_clicked();
                wait = 300;
            }
            None => eprintln!("RIGG_BILDER_2C fant ikke knappen «{t}»"),
        },
        Step::Popover(t, name) => match app.window().and_then(|w| find_button(w.upcast_ref(), t)) {
            Some(b) => {
                b.emit_clicked();
                let d = dir.clone();
                let name = *name;
                gtk::glib::timeout_add_local_once(Duration::from_millis(600), move || {
                    let mut c = b.first_child();
                    while let Some(ch) = c {
                        if let Some(p) = ch.downcast_ref::<gtk::Popover>() {
                            // Bare innholdet (uten skyggen rundt popoveren).
                            let target: gtk::Widget = p.first_child().unwrap_or_else(|| p.clone().upcast());
                            shot(&target, &d.join(format!("{name}.png")));
                            p.popdown();
                        }
                        c = ch.next_sibling();
                    }
                });
                wait = 1000;
            }
            None => eprintln!("RIGG_BILDER_2C fant ikke knappen «{t}»"),
        },
        Step::Eval(f) => f(&app),
        Step::Idle(name, ms) => {
            let clock = app.window().and_then(|w| w.frame_clock());
            let f0 = clock.as_ref().map(|c| c.frame_counter()).unwrap_or(0);
            let cpu0 = cpu_ticks();
            let t0 = std::time::Instant::now();
            let (name, ms, n, d) = (*name, *ms, next, dir.clone());
            gtk::glib::timeout_add_local_once(Duration::from_millis(ms), move || {
                let secs = t0.elapsed().as_secs_f64();
                let cpu = (cpu_ticks() - cpu0) as f64 / 100.0 / secs * 100.0;
                let frames = clock.as_ref().map(|c| c.frame_counter() - f0).unwrap_or(0);
                let line = format!("RIGG_2C_MAAL {name}: vinduets CPU {cpu:.2} % av én kjerne, {frames} bilder tegnet på {secs:.1} s ({:.2} bilder/s)", frames as f64 / secs);
                eprintln!("{line}");
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("maal.txt")) {
                    let _ = writeln!(f, "{line}");
                }
                n(100);
            });
            return;
        }
        Step::Shot(name) => {
            if let Some(win) = app.window() {
                shot(win.upcast_ref(), &dir.join(format!("{name}.png")));
            }
        }
        Step::ShotPage(class, name) => match app.window().and_then(|w| find_class(w.upcast_ref(), class)) {
            Some(p) => shot(&p, &dir.join(format!("{name}.png"))),
            None => eprintln!("RIGG_BILDER_2C fant ikke siden «{class}»"),
        },
    }
    next(wait);
}

/// utime + stime for denne prosessen (klokkeslag, 100 per sekund).
fn cpu_ticks() -> u64 {
    let s = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest = s.rsplit(") ").next().unwrap_or("");
    let f: Vec<&str> = rest.split_whitespace().collect();
    f.get(11).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0)
}

/// Feilsøking (`RIGG_BREDDE=1`): flatene som holder vinduet bredt.
fn wide(w: &gtk::Widget, depth: usize) {
    let min = w.measure(gtk::Orientation::Horizontal, -1).0;
    if min > 400 && w.is_visible() {
        eprintln!("RIGG_BREDDE {}{} {:?} min={min}", "  ".repeat(depth), w.css_name(), w.css_classes());
        let mut c = w.first_child();
        while let Some(ch) = c {
            wide(&ch, depth + 1);
            c = ch.next_sibling();
        }
    }
}

/// Etiketter som er kuttet med «…» (skrives til `kuttet.txt` i mappa).
fn cut_labels(w: &gtk::Widget, out: &mut Vec<String>) {
    if !w.is_mapped() {
        return;
    }
    if let Some(l) = w.downcast_ref::<gtk::Label>()
        && l.ellipsize() != gtk::pango::EllipsizeMode::None
        && l.layout().is_ellipsized()
    {
        out.push(l.text().to_string());
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        cut_labels(&ch, out);
        c = ch.next_sibling();
    }
}

fn shot(w: &gtk::Widget, path: &std::path::Path) {
    if std::env::var_os("RIGG_BREDDE").is_some() && path.to_string_lossy().contains("640") {
        wide(w, 0);
    }
    let (ww, hh) = (w.width(), w.height());
    if ww <= 0 || hh <= 0 {
        eprintln!("RIGG_BILDER_2C tom flate for {}", path.display());
        return;
    }
    let paintable = gtk::WidgetPaintable::new(Some(w));
    let snap = gtk::Snapshot::new();
    paintable.snapshot(&snap, ww as f64, hh as f64);
    let Some(node) = snap.to_node() else { return };
    let Some(renderer) = w.native().and_then(|n| n.renderer()) else { return };
    let tex = renderer.render_texture(&node, Some(&gtk::graphene::Rect::new(0.0, 0.0, ww as f32, hh as f32)));
    match tex.save_to_png(path) {
        Ok(()) => eprintln!("RIGG_SKJERMBILDE {} {ww}×{hh}", path.display()),
        Err(e) => eprintln!("Kunne ikke lagre {}: {e}", path.display()),
    }
    let mut cut = Vec::new();
    cut_labels(w, &mut cut);
    if !cut.is_empty()
        && let Some(dir) = path.parent()
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("kuttet.txt")) {
            let _ = writeln!(f, "{}: {}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), cut.join(" | "));
        }
    }
}
