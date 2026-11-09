//! Skjermbilder til dokumentasjonen (arkitektur.md kap. 11). Bare med
//! `RIGG_PROFIL=dev` og `RIGG_SKJERMBILDER=<mappe>`, og bare i en privat økt
//! (`scripts/bilder.sh`, egen `HOME` og egen `gnome-shell --headless`).
//!
//! Vinduet går gjennom en liste med steg (innstillinger, side, venting) og
//! tegner seg selv til PNG med `GtkWidgetPaintable` og vinduets egen tegner,
//! så ingenting av brukerens skrivebord er med. Innstillingene skrives ikke
//! til disk (økta har sin egen `HOME` uansett).
//!
//! `RIGG_PLAN` velger planen: `grunnmur` (standard), `3a` (det ekte kortet,
//! bare lesing), `3a-sim` (Bruk, Behold og Rull tilbake mot sim) og
//! `3a-borte` (sim-scenarioet der kortet faller av). Planene fra runde 2
//! ligger der agentene laget dem (`pages/oversikt/bilder.rs`,
//! `logg_ui/bilder.rs`, `pages/styring_bilder.rs`); `scripts/bilder.sh
//! --plan …` kjører alle sammen med riktig backend.

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
    Search(&'static str),
    CloseSettings,
    Wait(u64),
    Shot(&'static str),
    Keys(&'static str),
    /// Åpner en dialog og tar bilde av den (navn, hvordan den åpnes).
    Dialog(&'static str, fn(&Rc<App>)),
    Toast,
    /// Gjør noe i vinduet (feste en sensor, starte loggen …).
    Eval(fn(&Rc<App>)),
    /// Trykker på knappen med teksten (som en bruker).
    Click(&'static str),
    /// Ruller siden som vises (0,0 = toppen, 1,0 = bunnen).
    Scroll(f64),
}

fn plan() -> Vec<Step> {
    match std::env::var("RIGG_PLAN").as_deref() {
        Ok("3a") => plan_3a(),
        Ok("3a-sim") => plan_3a_sim(),
        Ok("3a-borte") => plan_3a_borte(),
        Ok("3a-egg") => plan_3a_egg(),
        Ok("3c") => plan_3c(),
        Ok("3c-640") => plan_3c_640(),
        Ok("6a") => plan_6a(),
        Ok("6a-ekte") => plan_6a_ekte(),
        Ok("6b") => plan_6b(),
        Ok("6b-tilstand") => plan_6b_tilstand(),
        Ok("7a") => plan_7a(),
        Ok("7a-ekte") => plan_7a_ekte(),
        Ok("8a") => plan_8a(),
        Ok("8a-hete") => plan_8a_hete(),
        _ => plan_grunnmur(),
    }
}

fn base() -> Value {
    json!({"theme": "light", "palette": "grafitt", "accent": "blue", "highContrast": false, "density": "standard", "fontScale": 100, "sideCollapsed": false, "motion": "off", "dismissed": []})
}

/// Festede sensorer med nøkler (navnene fra Sensorer) i statusblokken.
fn pin_keys(app: &Rc<App>) {
    let mut items = vec!["gpuTemp".to_string(), "gpuPower".into()];
    for f in ["fanRpm0", "memClock"] {
        if let Some(k) = app.gpu_key(f) {
            items.push(k);
        }
    }
    items.extend(["cpuTemp".to_string(), "cpu/thread/3/mhz".into(), "mem".into()]);
    app.set(json!({"statusItems": items}));
}

/// Runde 3A mot det ekte kortet (NVML, bare lesing).
fn plan_3a() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1440, 900),
        Set(base()),
        Go("oversikt"),
        Wait(2500),
        Shot("oversikt-viftestyring-lys"),
        Go("kontroller"),
        Wait(2500),
        Shot("kontroller-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("kontroller-mork"),
        Go("viftekurve"),
        Wait(2000),
        Shot("viftekurve-mork"),
        Set(json!({"theme": "light"})),
        Go("grafikkort"),
        Wait(6500),
        Scroll(1.0),
        Wait(500),
        Shot("grafikkort-prosesser-lys"),
        Scroll(0.0),
        Eval(pin_keys),
        Go("sensorer"),
        Wait(1500),
        Shot("sidefelt-festede-nokler-lys"),
        Size(660, 760),
        Wait(900),
        Size(640, 760),
        Wait(1500),
        Shot("sensorer-640-logg-ikon-lys"),
        Go("kontroller"),
        Wait(1500),
        Shot("kontroller-640-lys"),
        Settings("utseende"),
        Wait(1000),
        Shot("innstillinger-640-lys"),
        CloseSettings,
        Size(1440, 900),
    ]
}

/// Bruk → Behold og Bruk → Rull tilbake mot `--backend sim` (bare dev).
fn plan_3a_sim() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1440, 900),
        Set(base()),
        Go("kontroller"),
        Wait(2000),
        // Gliderne lager utkastet selv (runde 6C): samme vei som en endring på siden.
        Eval(|app| {
            let b = crate::pages::kontroller::read(app).values();
            crate::widgets::locked::edit_draft(app, b, |d| {
                d.power_limit_w = 200.0;
                d.core_offset_mhz = 100.0;
            });
        }),
        Wait(700),
        Shot("sim-kontroller-utkast-lys"),
        Click("Bruk"),
        Wait(2500),
        Shot("sim-kontroller-behold-lys"),
        Click("Behold*"),
        Wait(2500),
        Shot("sim-kontroller-beholdt-lys"),
        Set(json!({"theme": "dark"})),
        Eval(|app| {
            let b = crate::pages::kontroller::read(app).values();
            crate::widgets::locked::edit_draft(app, b, |d| d.power_limit_w = 160.0);
        }),
        Wait(500),
        Click("Bruk"),
        Wait(1500),
        Shot("sim-kontroller-behold-mork"),
        Click("Rull tilbake"),
        Wait(2500),
        Shot("sim-kontroller-rullet-tilbake-mork"),
    ]
}

/// Tastene til siden som vises, som om de ble trykket.
fn type_keys(app: &Rc<App>, keys: &[gtk::gdk::Key]) {
    for k in keys {
        crate::easter::on_key(app, *k);
        app.shell().page_key(&crate::pages::Key { key: *k, ctrl: false, shift: false, alt: false, typing: false });
    }
}

/// Filteret på Sensorer (kroken fra 2A finnes bare i debug-bygget).
fn sensor_filter(q: &str) {
    #[cfg(debug_assertions)]
    crate::pages::oversikt::bilder::call("sensorer.filter", q);
    #[cfg(not(debug_assertions))]
    eprintln!("RIGG_SKJERMBILDER: filteret «{q}» krever debug-bygget");
}

/// Påskeeggene som kan prøves uten klokke, dato eller terminal (1, 7, 10).
/// 3 (Om), 4 (42 °C), 5 (20. september), 6 (natt), 8 (kurven) og 9 (terminal)
/// har egne tester i `rigg-core/src/easter.rs` og skjermbilder fra runde 2.
fn plan_3a_egg() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1440, 900),
        Set(base()),
        Go("oversikt"),
        Wait(1500),
        Eval(|app| {
            use gtk::gdk::Key as K;
            type_keys(app, &[K::Up, K::Up, K::Down, K::Down, K::Left, K::Right, K::Left, K::Right, K::b, K::a]);
        }),
        Wait(600),
        Shot("egg-1-juksekode-lys"),
        Go("sensorer"),
        Wait(1200),
        Eval(|_| sensor_filter("kaffe")),
        Wait(700),
        Shot("egg-7-kaffe-lys"),
        Eval(|_| sensor_filter("")),
        Go("lys"),
        Wait(4000),
        Eval(|app| {
            use gtk::gdk::Key as K;
            type_keys(app, &[K::n, K::o, K::r, K::d, K::l, K::y, K::s]);
        }),
        Wait(2500),
        Shot("egg-10-nordlys-lys"),
        Eval(|app| type_keys(app, &[gtk::gdk::Key::Escape])),
        Wait(600),
    ]
}

/// Kortet faller av bussen (sim `borte`: etter 15 s, oppdages innen 10 s).
fn plan_3a_borte() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1440, 900),
        Set(base()),
        Go("oversikt"),
        Wait(1500),
        Shot("borte-for-oversikt-lys"),
        Wait(21_000),
        Shot("borte-oversikt-lys"),
        Go("sensorer"),
        Wait(1200),
        Shot("borte-sensorer-lys"),
        Go("kontroller"),
        Wait(1500),
        Shot("borte-kontroller-lys"),
        Go("viftekurve"),
        Wait(1500),
        Shot("borte-viftekurve-lys"),
        Go("lys"),
        Wait(4000),
        Shot("borte-lys-lys"),
        Go("grafikkort"),
        Wait(1200),
        Shot("borte-grafikkort-lys"),
        Set(json!({"theme": "dark"})),
        Go("oversikt"),
        Wait(1000),
        Shot("borte-oversikt-mork"),
        // Kortet kommer tilbake etter 45 s.
        Wait(22_000),
        Shot("tilbake-oversikt-mork"),
    ]
}

/// Alle ti sidene i rekkefølgen i sidefeltet (tastene 1–9 og 0).
const PAGES: [&str; 10] = ["oversikt", "sensorer", "grafer", "grafikkort", "prosessor", "disker", "system", "kontroller", "viftekurve", "lys"];

/// Navn til et bilde som lages i løkka (planen kjører én gang per prosess).
fn name(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Runde 3C: hele serien med standardtegneren (cairo). Alle sidene i lyst og
/// mørkt med de fire temaene, høy kontrast i lyst og mørkt, og alle sidene i
/// 640 px og 1920 px.
fn plan_3c() -> Vec<Step> {
    use Step::*;
    let mut v = vec![Size(1440, 900), Set(base())];
    let mut first = true;
    for pal in ["grafitt", "sand", "fjord", "midnatt"] {
        for (theme, t) in [("light", "lys"), ("dark", "mork")] {
            v.push(Set(json!({"palette": pal, "theme": theme, "highContrast": false})));
            for p in PAGES {
                v.push(Go(p));
                // Første gang bygges siden (og 3D-kortet på Lys).
                v.push(Wait(if first { if p == "lys" { 2500 } else { 1200 } } else { 600 }));
                v.push(Shot(name(format!("{p}-{pal}-{t}"))));
            }
            first = false;
        }
    }
    for (theme, t) in [("light", "lys"), ("dark", "mork")] {
        v.push(Set(json!({"palette": "grafitt", "theme": theme, "highContrast": true})));
        for p in PAGES {
            v.push(Go(p));
            v.push(Wait(600));
            v.push(Shot(name(format!("{p}-hoy-kontrast-{t}"))));
        }
    }
    v.push(Set(json!({"palette": "grafitt", "theme": "light", "highContrast": false})));
    for (w, h) in [(640, 760), (1920, 1080)] {
        // To ganger: sidefeltet blir ikonlist først når vinduet er smalere enn
        // 860 px, og først da kan vinduet bli 640 px.
        v.push(Size(w, h));
        v.push(Wait(800));
        v.push(Size(w, h));
        v.push(Wait(800));
        for p in PAGES {
            v.push(Go(p));
            v.push(Wait(700));
            v.push(Shot(name(format!("{p}-{w}-lys"))));
        }
    }
    v.push(Size(1440, 900));
    v.push(Settings("generelt"));
    v.push(Wait(800));
    v.push(Shot("innstillinger-generelt-lys"));
    v.push(Settings("utseende"));
    v.push(Wait(600));
    v.push(Shot("innstillinger-utseende-lys"));
    v.push(Settings("om"));
    v.push(Wait(600));
    v.push(Shot("innstillinger-om-lys"));
    v.push(Dialog("lisenser-lys", crate::settings::om::show_licenses));
    v.push(Keys("tastatursnarveier-lys"));
    v.push(CloseSettings);
    // Runde 4B: festede nøkler i statusblokken med standard skrift og «Stor»
    // (kuttet.txt skal ikke nevne dem), og Lys i 3D etter ett klikk.
    v.push(Eval(pin_keys));
    v.push(Go("sensorer"));
    v.push(Wait(1500));
    v.push(Shot("sidefelt-festede-nokler-lys"));
    v.push(Set(json!({"fontScale": 108})));
    v.push(Wait(800));
    v.push(Shot("sidefelt-festede-nokler-stor-lys"));
    v.push(Set(json!({"fontScale": 100})));
    v.push(Go("lys"));
    v.push(Wait(800));
    v.push(Shot("lys-apner-i-2d-lys"));
    v.push(Click("3D"));
    v.push(Wait(3000));
    v.push(Shot("lys-3d-valgt-lys"));
    v
}

/// Runde 3C: alle sidene i 640 px fra start (minste bredde per side).
fn plan_3c_640() -> Vec<Step> {
    use Step::*;
    let mut v = vec![Size(640, 760), Set(base()), Wait(800)];
    for p in PAGES {
        v.push(Go(p));
        v.push(Wait(if p == "lys" { 2500 } else { 1200 }));
        v.push(Size(640, 760));
        v.push(Wait(500));
        v.push(Shot(name(format!("{p}-640-fra-start-lys"))));
    }
    v
}

/// Fokusring på første rad i maskinhodet (som etter Tab).
fn focus_machine(app: &Rc<App>) {
    if let Some(w) = app.window() {
        w.set_focus_visible(true);
        if let Some(b) = find_class(w.upcast_ref(), "machine-row") {
            b.grab_focus();
        }
    }
}

/// Første synlige flate med CSS-klassen.
fn find_class(w: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if !w.is_visible() {
        return None;
    }
    if w.has_css_class(class) {
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

/// Runde 6A: Disker, Prosessor og System mot `--backend sim` (fem simulerte
/// disker: NVMe, SATA-SSD, to HDD-er der den ene har dårlige sektorer, og en
/// USB-disk uten SMART), i lyst og mørkt og i 1920 og 640 px. Prosessor og
/// System viser denne maskinen (de leser /proc og /sys også med sim).
fn plan_6a() -> Vec<Step> {
    use Step::*;
    let mut v = vec![
        Size(1920, 1080),
        Set(base()),
        Set(json!({"charts": ["diskTemp", "diskIo", "cpuTemp", "cpuLoad"]})),
        Go("disker"),
        Wait(4000),
        Shot("disker-1920-lys"),
        Scroll(1.0),
        Wait(600),
        Shot("disker-1920-smart-tabell-lys"),
        Scroll(0.0),
        Click("Extreme 55AE"),
        Wait(1500),
        Scroll(1.0),
        Wait(600),
        Shot("disker-1920-usb-uten-smart-lys"),
        Click("Extreme 55AE"),
        Scroll(0.0),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("disker-1920-mork"),
        Go("prosessor"),
        Wait(1500),
        Shot("prosessor-1920-mork"),
        Go("system"),
        Wait(1500),
        Shot("system-1920-mork"),
        Set(json!({"theme": "light"})),
        Wait(800),
        Shot("system-1920-lys"),
        Go("prosessor"),
        Wait(800),
        Shot("prosessor-1920-lys"),
        Go("sensorer"),
        Wait(1200),
        Scroll(0.62),
        Wait(600),
        Shot("sensorer-disker-1920-lys"),
        Scroll(0.0),
        Go("grafer"),
        Wait(2500),
        Shot("grafer-disker-1920-lys"),
        // To ganger: sidefeltet blir ikonlist først under 860 px, og først da kan vinduet bli 640 px.
        Size(640, 760),
        Wait(800),
        Size(640, 760),
        Wait(800),
    ];
    for (theme, t) in [("light", "lys"), ("dark", "mork")] {
        v.push(Set(json!({"theme": theme})));
        for p in ["disker", "prosessor", "system"] {
            v.push(Go(p));
            v.push(Wait(1000));
            v.push(Size(640, 760));
            v.push(Wait(700));
            v.push(Shot(name(format!("{p}-640-{t}"))));
        }
    }
    v.push(Set(json!({"theme": "light"})));
    v.push(Size(1440, 900));
    v
}

/// Runde 6A: de ekte diskene i den private økta, der systembussen er tom og
/// UDisks2 ikke svarer (merknaden og «Ukjent» helse), med det ekte kortet.
fn plan_6a_ekte() -> Vec<Step> {
    use Step::*;
    vec![
        Size(1920, 1080),
        Set(base()),
        Go("disker"),
        Wait(4000),
        Shot("disker-uten-udisks2-1920-lys"),
        Set(json!({"theme": "dark"})),
        Wait(800),
        Shot("disker-uten-udisks2-1920-mork"),
        Set(json!({"theme": "light"})),
        Go("prosessor"),
        Wait(1500),
        Shot("prosessor-ekte-1920-lys"),
        Size(1440, 900),
    ]
}

/// Runde 6B: skallet. Toppfeltet med vinduskontrollene på alle sidene, sidefeltet
/// utvidet og som ikonlist, lavt vindu, Om med signaturen, dialogene og
/// fokusringen, i lyst og mørkt, høy kontrast og 640 og 1920 px. Hover og trykk
/// tas med ekte peker i `docs/produksjon/verktoy/runde6b/kjor.sh`.
fn plan_6b() -> Vec<Step> {
    use Step::*;
    let mut v = vec![Size(1440, 900), Set(base())];
    for (theme, hc, t) in [("light", false, "lys"), ("dark", false, "mork"), ("light", true, "hoy-kontrast-lys"), ("dark", true, "hoy-kontrast-mork")] {
        v.push(Set(json!({"theme": theme, "highContrast": hc, "sideCollapsed": false})));
        v.push(Go("oversikt"));
        v.push(Wait(900));
        v.push(Shot(name(format!("skall-oversikt-{t}"))));
        v.push(Go("kontroller"));
        v.push(Wait(900));
        v.push(Shot(name(format!("skall-styring-{t}"))));
        v.push(Set(json!({"sideCollapsed": true})));
        v.push(Wait(700));
        v.push(Shot(name(format!("skall-ikonlist-{t}"))));
        v.push(Set(json!({"sideCollapsed": false})));
        v.push(Settings("om"));
        v.push(Wait(900));
        v.push(Shot(name(format!("om-signatur-{t}"))));
        v.push(CloseSettings);
        v.push(Wait(300));
        v.push(Eval(focus_machine));
        v.push(Wait(300));
        v.push(Shot(name(format!("skall-fokus-maskinhode-{t}"))));
    }
    v.push(Set(json!({"theme": "dark", "highContrast": false})));
    v.push(Keys("tastatursnarveier-mork"));
    v.push(Dialog("lisenser-mork", crate::settings::om::show_licenses));
    v.push(Set(json!({"theme": "light", "showKbd": true})));
    v.push(Go("oversikt"));
    v.push(Wait(600));
    v.push(Shot("skall-taster-1-0-lys"));
    v.push(Set(json!({"showKbd": false})));
    // Lav høyde: maskinhodet uten linjene, statusblokken som får plass.
    for (w, h) in [(1100, 560), (1100, 700), (1100, 800)] {
        v.push(Size(w, h));
        v.push(Wait(900));
        v.push(Shot(name(format!("skall-hoyde-{h}-lys"))));
    }
    for (w, h) in [(640, 760), (1920, 1080)] {
        v.push(Size(w, h));
        v.push(Wait(800));
        v.push(Size(w, h));
        v.push(Wait(800));
        for p in PAGES {
            v.push(Go(p));
            v.push(Wait(if p == "lys" { 1500 } else { 700 }));
            v.push(Shot(name(format!("skall-{p}-{w}-lys"))));
        }
        v.push(Settings("om"));
        v.push(Wait(700));
        v.push(Shot(name(format!("om-signatur-{w}-lys"))));
        v.push(CloseSettings);
    }
    v.push(Size(1440, 900));
    v
}

/// Runde 6B: maskinhodet i feiltilstandene. Kjøres mot sim-scenariene
/// (`--backend sim:borte`, `sim:ingen-driver`, `sim:to-kort` …); bildene får
/// navnet fra `RIGG_6B_TILSTAND`.
fn plan_6b_tilstand() -> Vec<Step> {
    use Step::*;
    let t = name(std::env::var("RIGG_6B_TILSTAND").unwrap_or_else(|_| "tilstand".into()));
    let wait = std::env::var("RIGG_6B_VENT").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
    vec![
        Size(1440, 900),
        Set(base()),
        Go("oversikt"),
        Wait(wait),
        Shot(name(format!("maskinhode-{t}-lys"))),
        Set(json!({"theme": "dark"})),
        Wait(600),
        Shot(name(format!("maskinhode-{t}-mork"))),
        Set(json!({"sideCollapsed": true})),
        Wait(700),
        Shot(name(format!("maskinhode-{t}-ikonlist-mork"))),
        // Runde 8A: Oversikt og Styrer-linja i tilstanden.
        Set(json!({"sideCollapsed": false, "theme": "light"})),
        Wait(700),
        Shot(name(format!("oversikt-{t}-lys"))),
        Go("kontroller"),
        Wait(1500),
        Shot(name(format!("styrer-{t}-klokker-og-effekt-lys"))),
        Go("viftekurve"),
        Wait(1200),
        Shot(name(format!("styrer-{t}-vifter-lys"))),
        Go("lys"),
        Wait(1500),
        Shot(name(format!("styrer-{t}-rgb-lys-lys"))),
        Go("oversikt"),
    ]
}

/// Runde 8A: den nye Oversikt mot sim (fem disker) i lyst og mørkt, 1920,
/// 1440 og 640 px, høy kontrast, hele siden (rullet til bunnen) og
/// tidsvinduene.
fn plan_8a() -> Vec<Step> {
    use Step::*;
    let mut v = vec![Size(1920, 1080), Set(base()), Set(json!({"windows": {"oversikt": 300}})), Go("oversikt"), Wait(3000)];
    for (theme, t) in [("light", "lys"), ("dark", "mork")] {
        v.extend([Set(json!({"theme": theme})), Wait(900), Shot(name(format!("oversikt-1920-{t}"))), Scroll(1.0), Wait(500), Shot(name(format!("oversikt-1920-bunn-{t}"))), Scroll(0.0)]);
    }
    v.extend([Set(json!({"theme": "light", "highContrast": true})), Wait(900), Shot("oversikt-1920-hoy-kontrast-lys"), Set(json!({"theme": "dark"})), Wait(700), Shot("oversikt-1920-hoy-kontrast-mork")]);
    v.extend([Set(json!({"highContrast": false, "theme": "light", "windows": {"oversikt": 60}})), Wait(2500), Shot("oversikt-1920-1min-lys")]);
    v.extend([Set(json!({"windows": {"oversikt": 300}})), Size(1440, 900), Wait(1500), Shot("oversikt-1440-lys"), Scroll(1.0), Wait(500), Shot("oversikt-1440-bunn-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"theme": "dark"})), Wait(700), Shot("oversikt-1440-mork")]);
    // To ganger: sidefeltet blir ikonlist først under 860 px.
    v.extend([Set(json!({"theme": "light"})), Size(640, 900), Wait(800), Size(640, 900), Wait(1500), Shot("oversikt-640-lys"), Scroll(0.5), Wait(500), Shot("oversikt-640-midt-lys")]);
    v.extend([Scroll(1.0), Wait(500), Shot("oversikt-640-bunn-lys"), Scroll(0.0), Set(json!({"theme": "dark"})), Wait(700), Shot("oversikt-640-mork")]);
    v.extend([Size(900, 900), Wait(1200), Set(json!({"theme": "light"})), Wait(600), Shot("oversikt-900-lys"), Size(1440, 900), Set(base())]);
    // Styring gjelder skjermkortet: crumb i toppfeltet, Styrer-linja og
    // kortet i sidefeltet, uten og med produsent satt for hånd.
    v.extend([Go("kontroller"), Wait(1800), Shot("styrer-klokker-og-effekt-lys"), Go("viftekurve"), Wait(1500), Shot("styrer-vifter-lys"), Go("lys"), Wait(1800), Shot("styrer-rgb-lys-lys")]);
    v.extend([Set(json!({"gpuMakers": {"GPU-sim0000-0000": "Palit"}})), Wait(600), Go("kontroller"), Wait(900), Shot("styrer-produsent-palit-lys")]);
    v.extend([Set(json!({"theme": "dark"})), Wait(700), Shot("styrer-produsent-palit-mork"), Go("viftekurve"), Wait(1200), Shot("styrer-vifter-produsent-mork")]);
    v.extend([Size(640, 900), Wait(800), Size(640, 900), Wait(1200), Set(json!({"theme": "light"})), Go("kontroller"), Wait(1200), Shot("styrer-klokker-og-effekt-640-lys")]);
    v.extend([Settings("styring"), Wait(900), Shot("innstillinger-skjermkortstyring-640-lys"), CloseSettings, Size(1440, 900), Wait(700), Keys("tastatursnarveier-lys")]);
    v.extend([Set(base()), Set(json!({"gpuMakers": {}}))]);
    v
}

/// Runde 8A: `sim:hete` (fra under 20 °C via 95 °C til 110 °C og 300 W, en
/// disk på 65 °C og en på 14 °C). Viser at ingen akse kutter: Oversikt med
/// «Tilpass» og «Fast», °F, Grafer, Sensorer, Klokker og effekt og Vifter.
fn plan_8a_hete() -> Vec<Step> {
    use Step::*;
    let all = json!(["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan", "vram", "cpuLoad", "cpuTemp", "mem", "net", "diskTemp", "diskIo"]);
    let mut v = vec![Size(1920, 1080), Set(base()), Set(json!({"windows": {"oversikt": 300, "sensorer": 300}, "chartWindow": 300, "chartScale": "auto", "charts": all})), Go("oversikt"), Wait(3500)];
    v.extend([Shot("hete-oversikt-tilpass-lys"), Scroll(0.35), Wait(500), Shot("hete-oversikt-tilpass-grafer-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"theme": "dark"})), Wait(800), Shot("hete-oversikt-tilpass-mork"), Set(json!({"theme": "light"}))]);
    v.extend([Set(json!({"chartScale": "fixed"})), Wait(1500), Shot("hete-oversikt-fast-lys"), Scroll(0.35), Wait(500), Shot("hete-oversikt-fast-grafer-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"chartScale": "auto", "unit": "F"})), Wait(1500), Shot("hete-oversikt-fahrenheit-lys"), Set(json!({"unit": "C"}))]);
    v.extend([Set(json!({"windows": {"oversikt": 60}})), Wait(2500), Shot("hete-oversikt-1min-lys"), Set(json!({"windows": {"oversikt": 300}}))]);
    v.extend([Go("grafer"), Wait(3000), Shot("hete-grafer-tilpass-lys"), Scroll(1.0), Wait(500), Shot("hete-grafer-tilpass-bunn-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"chartScale": "fixed"})), Wait(1500), Shot("hete-grafer-fast-lys"), Set(json!({"chartScale": "auto", "theme": "dark"})), Wait(1200), Shot("hete-grafer-tilpass-mork"), Set(json!({"theme": "light"}))]);
    v.extend([Go("kontroller"), Wait(2500), Scroll(1.0), Wait(600), Shot("hete-klokker-og-effekt-lys"), Scroll(0.0)]);
    v.extend([Go("viftekurve"), Wait(2500), Scroll(1.0), Wait(600), Shot("hete-vifter-lys"), Scroll(0.0)]);
    v.extend([Go("disker"), Wait(2000), Shot("hete-disker-lys")]);
    v.extend([Size(640, 900), Wait(800), Size(640, 900), Wait(1200), Go("oversikt"), Wait(1500), Shot("hete-oversikt-640-lys"), Scroll(0.4), Wait(500), Shot("hete-oversikt-640-grafer-lys")]);
    v.extend([Size(1440, 900), Set(base())]);
    v
}

/// Runde 7A: Innstillinger › Skjermkortstyring mot den falske pkexec
/// (`scripts/falsk-pkexec.sh`) og hjelperen med det simulerte kortet: av, på,
/// godkjenn et trinn, feil, av igjen, og merket i sidefeltet. Lyst og mørkt,
/// 1440 og 640 px. Kjøres med `docs/produksjon/verktoy/runde7a/bilder7a.sh`.
fn plan_7a() -> Vec<Step> {
    use crate::settings::styring;
    use Step::*;
    let mut v = vec![Size(1440, 900), Set(base()), Go("kontroller"), Wait(900), Shot("sidefelt-bare-visning-lys")];
    v.extend([Settings("styring"), Wait(1200), Shot("styring-av-lys"), Set(json!({"theme": "dark"})), Wait(700), Shot("styring-av-mork"), Set(json!({"theme": "light"}))]);
    // Bryteren: falsk pkexec, aktiver.sh mot den falske roten, hjelperen startes.
    v.extend([Eval(|a| styring::toggle(a, true)), Wait(500), Shot("styring-venter-lys"), Wait(3000), Shot("styring-pa-lys"), Scroll(1.0), Wait(400), Shot("styring-pa-trinn-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"theme": "dark"})), Wait(700), Shot("styring-pa-mork")]);
    // Et trinn: spørsmålet med advarsel, så godkjent (sim).
    v.push(Dialog("styring-godkjenn-effekt-mork", |a| styring::confirm_step(a, rigg_core::hjelper::Step::Effekt)));
    v.push(Dialog("styring-godkjenn-klokker-lys", |a| {
        a.set(json!({"theme": "light"}));
        styring::confirm_step(a, rigg_core::hjelper::Step::Klokker)
    }));
    v.extend([Set(json!({"theme": "dark"})), Eval(|a| styring::set_step(a, rigg_core::hjelper::Step::Effekt, true)), Wait(3500), Shot("styring-trinn-effekt-mork")]);
    v.extend([CloseSettings, Go("kontroller"), Wait(1200), Shot("sidefelt-endring-pa-mork"), Set(json!({"theme": "light"})), Wait(700), Shot("sidefelt-endring-pa-lys")]);
    // 640 px.
    v.extend([Size(640, 760), Wait(800), Size(640, 760), Wait(800), Shot("sidefelt-endring-pa-640-lys"), Settings("styring"), Wait(900), Shot("styring-pa-640-lys"), Scroll(1.0), Wait(400), Shot("styring-pa-640-trinn-lys"), Scroll(0.0)]);
    v.extend([Set(json!({"theme": "dark"})), Wait(700), Shot("styring-pa-640-mork"), Set(json!({"theme": "light"})), Size(1440, 900), Wait(900)]);
    // Feil: hjelperen stopper (som et krasj), tjenesten finner den ikke.
    v.extend([Eval(stop_fake_helper), Wait(1500), Eval(|a| crate::widgets::locked::refresh_helper(a, crate::settings::request_render)), Wait(1200), Shot("styring-feil-lys")]);
    v.extend([Set(json!({"theme": "dark"})), Wait(700), Shot("styring-feil-mork"), Set(json!({"theme": "light"}))]);
    v.extend([CloseSettings, Go("kontroller"), Wait(1000), Shot("sidefelt-feil-lys"), Settings("styring"), Wait(900)]);
    // Av igjen: deaktiver.sh fjerner alt fra den falske roten.
    v.extend([Eval(|a| styring::toggle(a, false)), Wait(3500), Shot("styring-av-igjen-lys")]);
    v.extend([CloseSettings, Go("kontroller"), Wait(1000), Shot("sidefelt-av-igjen-lys")]);
    v
}

/// Runde 7A mot det ekte kortet (bare lesing): hjelperen med `--backend
/// nvml`, der skriving er sperret i 1.2.0. Trinnene kan ikke godkjennes.
fn plan_7a_ekte() -> Vec<Step> {
    use crate::settings::styring;
    use Step::*;
    vec![
        Size(1440, 900),
        Set(base()),
        Settings("styring"),
        Wait(1200),
        Eval(|a| styring::toggle(a, true)),
        Wait(3500),
        Shot("styring-pa-ekte-lys"),
        Scroll(1.0),
        Wait(400),
        Shot("styring-pa-ekte-trinn-lys"),
        Scroll(0.0),
        Set(json!({"theme": "dark"})),
        Wait(700),
        Shot("styring-pa-ekte-mork"),
        CloseSettings,
        Go("kontroller"),
        Wait(1500),
        Shot("kontroller-endring-pa-ekte-mork"),
        Set(json!({"theme": "light"})),
        Wait(700),
        Shot("kontroller-endring-pa-ekte-lys"),
        // Låst-linja på de to andre Styring-sidene (7C).
        Go("viftekurve"),
        Wait(1500),
        Shot("vifter-endring-pa-ekte-lys"),
        Go("lys"),
        Wait(1500),
        Shot("lys-endring-pa-ekte-lys"),
        Set(json!({"theme": "dark"})),
        Wait(700),
        Shot("lys-endring-pa-ekte-mork"),
        Set(json!({"theme": "light"})),
        Settings("styring"),
        Wait(900),
        Eval(|a| styring::toggle(a, false)),
        Wait(3500),
        Shot("styring-av-igjen-ekte-lys"),
        CloseSettings,
        Go("kontroller"),
        Wait(1200),
        Shot("kontroller-av-igjen-ekte-lys"),
    ]
}

/// Stopper hjelperen den falske pkexec startet (planen 7a: «Feil»). Bare i
/// dev og bare i den falske roten.
fn stop_fake_helper(app: &Rc<App>) {
    let Some(root) = std::env::var_os("RIGG_HJELPER_ROT").filter(|_| app.profile.is_dev()) else { return };
    let pidfile = PathBuf::from(root).join("run/agentic-rigg/falsk.pid");
    if let Some(pid) = std::fs::read_to_string(&pidfile).ok().and_then(|s| s.trim().parse::<i32>().ok()).filter(|p| *p > 1) {
        // SAFETY: SIGTERM til hjelperen den falske pkexec startet i økta.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let _ = std::fs::remove_file(pidfile);
    }
}

fn plan_grunnmur() -> Vec<Step> {
    use Step::*;
    let base = json!({"theme": "light", "palette": "grafitt", "accent": "blue", "highContrast": false, "density": "standard", "fontScale": 100, "sideCollapsed": false, "motion": "off"});
    vec![
        Size(1440, 900),
        Set(base.clone()),
        Go("oversikt"),
        Wait(1500),
        Shot("oversikt-lys"),
        Set(json!({"theme": "dark"})),
        Wait(600),
        Shot("oversikt-mork"),
        Set(json!({"sideCollapsed": true})),
        Wait(600),
        Shot("oversikt-ikonlist-mork"),
        Set(json!({"theme": "light"})),
        Wait(600),
        Shot("oversikt-ikonlist-lys"),
        Set(json!({"sideCollapsed": false})),
        Settings("generelt"),
        Wait(900),
        Shot("innstillinger-generelt-lys"),
        Settings("utseende"),
        Wait(900),
        Shot("innstillinger-utseende-lys"),
        Set(json!({"theme": "dark"})),
        Wait(700),
        Shot("innstillinger-utseende-mork"),
        Settings("sidefelt"),
        Wait(700),
        Shot("innstillinger-sidefelt-mork"),
        Settings("om"),
        Wait(700),
        Shot("innstillinger-om-mork"),
        Search("tema"),
        Wait(700),
        Shot("innstillinger-sok-tema-mork"),
        Search(""),
        Settings("data"),
        Toast,
        Wait(900),
        Shot("innstillinger-data-varsel-mork"),
        Keys("tastatursnarveier-mork"),
        CloseSettings,
        Set(json!({"theme": "light", "palette": "sand", "accent": "teal"})),
        Go("oversikt"),
        Wait(700),
        Shot("tema-sand-lys-blagronn"),
        Set(json!({"theme": "dark", "palette": "fjord", "accent": "green", "density": "compact"})),
        Wait(700),
        Shot("tema-fjord-mork-gronn-kompakt"),
        Set(json!({"theme": "dark", "palette": "midnatt", "accent": "purple", "highContrast": true, "density": "standard"})),
        Wait(700),
        Shot("tema-midnatt-mork-lilla-hoykontrast"),
        Set(json!({"theme": "light", "palette": "midnatt", "accent": "pink", "highContrast": false, "fontScale": 116, "density": "comfortable"})),
        Wait(700),
        Shot("tema-midnatt-lys-rosa-storre-luftig"),
        Set(base),
        Size(640, 720),
        Go("oversikt"),
        Wait(700),
        // To ganger: sidefeltet blir ikonlist først når vinduet er under 860 px.
        Size(640, 720),
        Wait(900),
        Shot("oversikt-smal-lys"),
        Settings("utseende"),
        Wait(500),
        Size(640, 720),
        Wait(900),
        Shot("innstillinger-smal-lys"),
        CloseSettings,
        Size(1440, 900),
    ]
}

/// Starter planen når vinduet er vist. Avslutter appen etter siste bilde.
pub fn start(app: &Rc<App>) {
    let Some(dir) = std::env::var_os("RIGG_SKJERMBILDER").map(PathBuf::from) else { return };
    if !app.profile.is_dev() {
        eprintln!("RIGG_SKJERMBILDER krever RIGG_PROFIL=dev.");
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let steps = Rc::new(plan());
    // Venter på første prøve, så tallene står der.
    let a = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || run(a, steps, 0, dir));
}

fn run(app: Rc<App>, steps: Rc<Vec<Step>>, i: usize, dir: PathBuf) {
    let Some(step) = steps.get(i) else {
        eprintln!("RIGG_SKJERMBILDER ferdig: {}", dir.display());
        app.gtk.quit();
        return;
    };
    let mut wait = 50;
    match step {
        Step::Set(v) => {
            let next = rigg_core::settings::merge(&app.settings(), v).unwrap_or_else(|_| app.settings().clone());
            app.replace_settings(next);
        }
        Step::Size(w, h) => {
            if let Some(win) = app.window() {
                win.unmaximize();
                win.set_default_size(*w, *h);
            }
            wait = 600;
        }
        Step::Go(p) => app.go(p),
        Step::Settings(s) => app.open_settings(s),
        Step::Search(q) => crate::settings::set_query(q),
        Step::CloseSettings => app.shell().close_settings(),
        Step::Wait(ms) => wait = *ms,
        Step::Eval(f) => f(&app),
        Step::Click(t) => match app.window().and_then(|w| find_button(w.upcast_ref(), t)) {
            Some(b) => {
                b.emit_clicked();
                wait = 300;
            }
            None => eprintln!("RIGG_SKJERMBILDER fant ikke knappen «{t}»"),
        },
        Step::Scroll(f) => {
            if let Some(win) = app.window() {
                let mut best: Option<gtk::ScrolledWindow> = None;
                scrollers(win.upcast_ref(), &mut |sc| {
                    let h = sc.vadjustment().page_size();
                    if best.as_ref().is_none_or(|b| b.vadjustment().page_size() < h) {
                        best = Some(sc.clone());
                    }
                });
                if let Some(sc) = best {
                    let adj = sc.vadjustment();
                    adj.set_value(adj.lower() + (adj.upper() - adj.page_size() - adj.lower()) * f);
                }
            }
        }
        Step::Toast => app.toast(crate::toast::Toast::new("Historikken slettes").body("Grafene begynner på nytt om 5 s.").action("Angre", || {}).ms(4000)),
        Step::Keys(_) | Step::Dialog(..) => {
            let name = match step {
                Step::Dialog(n, open) => {
                    open(&app);
                    n
                }
                Step::Keys(n) => {
                    crate::shortcuts::show_dialog(&app);
                    n
                }
                _ => unreachable!(),
            };
            let (a, name, dir) = (app.clone(), *name, dir.clone());
            gtk::glib::timeout_add_local_once(Duration::from_millis(700), move || {
                for w in gtk::Window::list_toplevels() {
                    if let Ok(w) = w.downcast::<gtk::Window>()
                        && w.has_css_class("dialog")
                    {
                        shot(w.upcast_ref(), &dir.join(format!("{name}.png")));
                        w.close();
                    }
                }
                let _ = a;
            });
            wait = 1200;
        }
        Step::Shot(name) => {
            if let Some(win) = app.window() {
                shot(win.upcast_ref(), &dir.join(format!("{name}.png")));
            }
        }
    }
    gtk::glib::timeout_add_local_once(Duration::from_millis(wait), move || run(app, steps, i + 1, dir));
}

/// Alle synlige rullefelt som kan rulles loddrett.
fn scrollers(w: &gtk::Widget, f: &mut dyn FnMut(&gtk::ScrolledWindow)) {
    if !w.is_drawable() {
        return;
    }
    if let Some(sc) = w.downcast_ref::<gtk::ScrolledWindow>() {
        let a = sc.vadjustment();
        if a.upper() - a.lower() > a.page_size() + 1.0 {
            f(sc);
        }
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        scrollers(&ch, f);
        c = ch.next_sibling();
    }
}

/// Første synlige knapp med teksten. `*` til slutt: teksten begynner med det
/// foran («Behold*» finner «Behold (12)»).
fn find_button(w: &gtk::Widget, text: &str) -> Option<gtk::Button> {
    let matches = |s: &str| match text.strip_suffix('*') {
        Some(p) => s.starts_with(p),
        None => s == text,
    };
    fn labels(w: &gtk::Widget, f: &dyn Fn(&str) -> bool) -> bool {
        let mut c = w.first_child();
        while let Some(ch) = c {
            if ch.downcast_ref::<gtk::Label>().is_some_and(|l| f(&l.text())) || labels(&ch, f) {
                return true;
            }
            c = ch.next_sibling();
        }
        false
    }
    if !(w.is_visible() && w.is_mapped()) {
        return None;
    }
    if let Some(b) = w.downcast_ref::<gtk::Button>()
        && (b.label().is_some_and(|l| matches(&l)) || labels(w, &matches))
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
    None
}

/// Feilsøking: hvilke flater som holder vinduet bredt (minste bredde over 300 px).
fn wide(w: &gtk::Widget, depth: usize) {
    let min = w.measure(gtk::Orientation::Horizontal, -1).0;
    if min > 300 && w.is_visible() {
        eprintln!("RIGG_BREDDE {}{} {:?} min={min}", "  ".repeat(depth), w.css_name(), w.css_classes());
        let mut c = w.first_child();
        while let Some(ch) = c {
            wide(&ch, depth + 1);
            c = ch.next_sibling();
        }
    }
}

/// Etiketter som er kuttet med «…» i bildet (finpussen i 4B): skrives som
/// `RIGG_SKJERMBILDE kuttet` og i `kuttet.txt` ved siden av bildene.
fn cut_labels(w: &gtk::Widget, out: &mut Vec<String>) {
    if !w.is_drawable() {
        return;
    }
    if let Some(l) = w.downcast_ref::<gtk::Label>()
        && l.ellipsize() != gtk::pango::EllipsizeMode::None
        && l.layout().is_ellipsized()
    {
        out.push(format!("«{}» [{}]", l.text(), l.css_classes().join(" ")));
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        cut_labels(&ch, out);
        c = ch.next_sibling();
    }
}

fn shot(win: &gtk::Window, path: &std::path::Path) {
    if std::env::var_os("RIGG_BREDDE").is_some() {
        wide(win.upcast_ref(), 0);
    }
    let mut cut = Vec::new();
    cut_labels(win.upcast_ref(), &mut cut);
    if !cut.is_empty() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        eprintln!("RIGG_SKJERMBILDE kuttet i {name}: {}", cut.join(", "));
        if let Some(dir) = path.parent() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("kuttet.txt")) {
                let _ = writeln!(f, "{name}: {}", cut.join(", "));
            }
        }
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

#[cfg(test)]
mod tests {
    #[test]
    fn planene_har_alle_sidene_i_samme_rekkefølge() {
        let ids: Vec<&str> = crate::pages::PAGES.iter().map(|p| p.id).collect();
        assert_eq!(super::PAGES.to_vec(), ids);
    }
}
