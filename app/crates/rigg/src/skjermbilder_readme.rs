//! Bildene til README (`RIGG_PLAN=readme`). Kjøres med engelsk brukerflate
//! (`RIGG_BILDER_SPRAK=en scripts/bilder.sh --plan readme`) mot `sim:spill`,
//! så GPU, disker og nett er simulerte. `RIGG_README_DEL=oppgaver` tar
//! Oppgaver mot en falsk `--root` med simulerte prosesser og brukere
//! (`scripts/readme-prosesser.py`), aldri maskinens egen prosessliste.
use super::Step;
use crate::state::App;
use crate::widgets::locked;
use gtk::prelude::*;
use std::rc::Rc;

const W: i32 = 1440;
const H: i32 = 1000;

fn no_draft(app: &Rc<App>) {
    locked::update(app, "utkast", |s| s.utkast = None);
}

/// Et utkast med tre endringer, slik gliderne lager det.
fn draft_clocks(app: &Rc<App>) {
    let b = crate::pages::kontroller::read(app).values();
    locked::edit_draft(app, b, |d| {
        d.power_limit_w = 260.0;
        d.core_offset_mhz = 75.0;
        d.mem_offset_mhz = 500.0;
    });
}

/// Viftekurve-utkast med den balanserte forhåndsinnstillingen.
fn draft_fans(app: &Rc<App>) {
    locked::set_fan_advanced(app, true);
    locked::edit_draft(app, crate::pages::viftekurve::card_values(app), |d| {
        d.fan_mode = rigg_core::txn::FanMode::Curve;
        crate::pages::viftekurve::PRESETS[1].apply(d);
    });
}

fn widget(app: &App, class: &str) -> Option<gtk::Widget> {
    super::find_class(app.window()?.upcast_ref(), class)
}

fn rows(app: &App) -> Vec<gtk::ListBoxRow> {
    let mut out = Vec::new();
    let Some(list) = widget(app, "task-rows") else { return out };
    let mut child = list.first_child();
    while let Some(c) = child {
        if let Some(r) = c.downcast_ref::<gtk::ListBoxRow>().filter(|r| r.is_visible()) {
            out.push(r.clone());
        }
        child = c.next_sibling();
    }
    out
}

/// Gruppen med navnet `name` (radnavnet inneholder ikke prosessnavnet, så
/// gruppen finnes på teksten i raden).
fn group_named(app: &App, name: &str) -> Option<gtk::ListBoxRow> {
    rows(app).into_iter().find(|r| r.has_css_class("task-group") && label_texts(r.upcast_ref()).iter().any(|t| t == name || t.starts_with(&format!("{name} ("))))
}

fn label_texts(w: &gtk::Widget) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        out.push(l.text().to_string());
    }
    let mut c = w.first_child();
    while let Some(x) = c {
        out.extend(label_texts(&x));
        c = x.next_sibling();
    }
    out
}

thread_local! {
    static GROUP: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn open_group(app: &Rc<App>) {
    let Some(group) = group_named(app, "rustc") else {
        eprintln!("RIGG_SKJERMBILDE fant ikke gruppen rustc");
        return;
    };
    GROUP.with(|g| *g.borrow_mut() = group.widget_name().to_string());
    if let Some(b) = group.child().and_then(|c| c.first_child()).and_then(|c| c.downcast::<gtk::Button>().ok()) {
        b.emit_clicked();
    }
}

/// Velger den andre prosessen i den utfoldede gruppen, så detaljfeltet viser én prosess.
fn select_member(app: &Rc<App>) {
    let name = GROUP.with(|g| g.borrow().clone());
    let all = rows(app);
    let Some(index) = all.iter().find(|r| r.widget_name() == name).map(|r| r.index()) else { return };
    let Some(member) = all.iter().find(|r| r.index() == index + 2) else { return };
    if let Some(list) = widget(app, "task-rows").and_then(|w| w.downcast::<gtk::ListBox>().ok()) {
        list.select_row(Some(member));
    }
}

/// Sekunder til sim:spill har vist spill-last i godt over ett minutt. Lasten
/// følger veggklokka (150 s: 90 s spill, 60 s meny), så ventetiden regnes
/// ut når planen starter, og Oversikt tas mot slutten av en spillperiode.
fn wait_for_game_ms() -> u64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let k = now.rem_euclid(150.0);
    ((80.0 - 6.0 - k).rem_euclid(150.0) * 1000.0) as u64
}

pub(super) fn plan() -> Vec<Step> {
    use Step::*;
    if std::env::var("RIGG_README_DEL").as_deref() == Ok("oppgaver") {
        return vec![
            Size(W, H),
            Set(super::base()),
            Wait(600),
            Size(W, H),
            Go("oppgaver"),
            Wait(9000),
            Eval(open_group),
            Wait(1500),
            Eval(select_member),
            Wait(3000),
            Shot("tasks"),
            Set(serde_json::json!({"theme": "dark"})),
            Wait(1200),
            Shot("tasks-dark"),
        ];
    }
    let mut v = vec![Size(W, H), Set(super::base()), Eval(no_draft), Wait(600), Size(W, H)];
    // `RIGG_README_DEL=lys-nett`: bare RGB lys og Nettverk (trenger ikke spill-last).
    if std::env::var("RIGG_README_DEL").as_deref() != Ok("lys-nett") {
        v.extend(game());
    }
    v.extend(light_and_network());
    v
}

fn game() -> Vec<Step> {
    use Step::*;
    vec![
        Go("oversikt"),
        Wait(3000 + wait_for_game_ms()),
        Shot("overview"),
        Go("viftekurve"),
        Wait(1500),
        Eval(draft_fans),
        Wait(2000),
        Shot("fans"),
        Scroll(0.4),
        Wait(600),
        Shot("fans-curve"),
        Eval(no_draft),
        Go("kontroller"),
        Wait(1500),
        Eval(draft_clocks),
        Wait(1500),
        Shot("clocks-power"),
        Eval(no_draft),
        Go("oversikt"),
        Set(serde_json::json!({"theme": "dark"})),
        Wait(1200),
        Shot("overview-dark"),
        Set(serde_json::json!({"theme": "light"})),
    ]
}

fn light_and_network() -> Vec<Step> {
    use Step::*;
    vec![
        Go("lys"),
        Wait(1500),
        Eval(|_| crate::pages::lys::debug_mode("2d")),
        Eval(|_| crate::pages::lys_kort::debug_fixture("pending_effects")),
        Wait(800),
        Shot("rgb-lighting"),
        Eval(|_| crate::pages::lys_kort::debug_fixture("stored")),
        Wait(800),
        Shot("rgb-lighting-stored"),
        Go("nettverk"),
        Wait(4000),
        Scroll(0.0),
        Wait(400),
        Shot("network"),
        Go("grafikkort"),
        Wait(1500),
        Shot("graphics-card"),
        Go("sensorer"),
        Wait(1500),
        Shot("sensors"),
    ]
}
