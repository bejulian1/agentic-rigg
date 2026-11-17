//! Lys: egen fysisk kortstyring og en tydelig atskilt lokal modell.
//! Kortlyset bruker sin egen RGB-tillatelse og bekreftelsesflyt i lys_kort.
//! «Lagre i appen» lagrer bare modellens valg og sender aldri maskinvarekall.
//!
//! Forhåndsvisningen fyller høyden til venstre; til høyre står Effekt,
//! Farge og (i 3D) Deler, eller (i 2D) modellens soner. 3D-kortet
//! er `lys3d::CardView` (GtkGLArea); det flate kortet er reserven når GL
//! mangler, og når du velger 2D. Vinkel, zoom, lagtrinn og «Skill lagene»
//! huskes mellom økter (`profiler.json`). Nordlys (påskeegg 10): skriv
//! «nordlys» på siden.
//!
//! **Eies av 2C (Styring og Lys).**

use super::{Key, Page};
use crate::lys3d::{self, CardView, STEPS, VIEWS, flat::FlatCard};
use crate::motion::Level;
use crate::rgb::{self, EFFECTS, Engine, RgbConfig, SWATCHES, ZONES};
use crate::state::App;
use crate::widgets::locked::{self, Columns};
use crate::widgets::rows::tag;
use crate::widgets::{Icon, Panel, Seg, button, class, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use rigg_core::fmt as F;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Det Lys åpner med (Innstillinger › Utseende › «3D-kortet»): «Alltid 3D»,
/// «Alltid 2D», eller «Auto», som viser 2D til brukeren har valgt 3D én gang
/// og deretter det sist valgte. 2D koster lite; 3D vekker kortet (P0 i ca. 4 s
/// første gang) og bruker ca. 120 MB mer minne i vinduet.
pub fn initial_view(setting: &str, picked: Option<&str>) -> &'static str {
    match (setting, picked) {
        ("3d", _) => "3d",
        ("2d", _) => "2d",
        (_, Some("3d")) => "3d",
        _ => "2d",
    }
}

/// 3D-ressursene frigjøres når du har vært borte fra Lys så lenge.
const RELEASE_S: u32 = 30;

// ================================================================ fargeprøven

mod dot_imp {
    use super::*;

    #[derive(Default)]
    pub struct Dot {
        pub kind: RefCell<String>,
        pub colors: RefCell<Vec<gdk::RGBA>>,
        pub size: Cell<i32>,
        pub ring: Cell<bool>,
        pub ring_color: RefCell<Option<gdk::RGBA>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dot {
        const NAME: &'static str = "RiggColorDot";
        type Type = super::Dot;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("colordot");
            klass.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Dot {}

    impl WidgetImpl for Dot {
        fn measure(&self, _o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            let s = self.size.get().max(8);
            (s, s, -1, -1)
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let sz = w.width().min(w.height()) as f32;
            let (cx, cy) = (w.width() as f32 / 2.0, w.height() as f32 / 2.0);
            let r = sz / 2.0 - if self.ring.get() { 3.0 } else { 1.0 };
            let cols = self.colors.borrow();
            let rect = graphene::Rect::new(cx - r, cy - r, r * 2.0, r * 2.0);
            if let Some(rc) = self.ring_color.borrow().as_ref()
                && self.ring.get()
            {
                let b = gsk::PathBuilder::new();
                b.add_circle(&graphene::Point::new(cx, cy), r + 2.2);
                s.append_stroke(&b.to_path(), &gsk::Stroke::new(1.5), rc);
            }
            s.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, r));
            match self.kind.borrow().as_str() {
                "rainbow" => {
                    let stops: Vec<gsk::ColorStop> = ["#e5383b", "#f2c12e", "#3fb950", "#2bb3c8", "#3a7bf0", "#8e5cf0", "#e5383b"]
                        .iter()
                        .enumerate()
                        .map(|(i, h)| gsk::ColorStop::new(i as f32 / 6.0, hex(h)))
                        .collect();
                    s.append_conic_gradient(&rect, &graphene::Point::new(cx, cy), 0.0, &stops);
                }
                "cycle" if !cols.is_empty() => {
                    let c = cols[0];
                    let clear = gdk::RGBA::new(c.red(), c.green(), c.blue(), 0.0);
                    s.append_conic_gradient(&rect, &graphene::Point::new(cx, cy), 90.0, &[gsk::ColorStop::new(0.0, c), gsk::ColorStop::new(0.6, clear), gsk::ColorStop::new(1.0, c)]);
                }
                "temperature" if cols.len() >= 2 => {
                    s.append_linear_gradient(&rect, &graphene::Point::new(cx - r, cy), &graphene::Point::new(cx + r, cy), &[gsk::ColorStop::new(0.0, cols[0]), gsk::ColorStop::new(1.0, cols[1])]);
                }
                "breathe" if !cols.is_empty() => {
                    let c = cols[0];
                    let soft = gdk::RGBA::new(c.red(), c.green(), c.blue(), 0.25);
                    s.append_radial_gradient(&rect, &graphene::Point::new(cx, cy), r, r, 0.0, 1.0, &[gsk::ColorStop::new(0.35, c), gsk::ColorStop::new(0.7, soft), gsk::ColorStop::new(1.0, soft)]);
                }
                _ => {
                    if let Some(c) = cols.first() {
                        s.append_color(c, &rect);
                    }
                }
            }
            s.pop();
            // Tynn kant, så hvitt og svart synes mot alle flater.
            let b = gsk::PathBuilder::new();
            b.add_circle(&graphene::Point::new(cx, cy), r - 0.5);
            s.append_stroke(&b.to_path(), &gsk::Stroke::new(1.0), &gdk::RGBA::new(0.0, 0.0, 0.0, 0.18));
        }
    }
}

glib::wrapper! {
    pub struct Dot(ObjectSubclass<dot_imp::Dot>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

fn hex(h: &str) -> gdk::RGBA {
    let c = rgb::hex_to_rgb(h);
    gdk::RGBA::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0)
}

impl Dot {
    pub fn new(size: i32) -> Dot {
        let d: Dot = glib::Object::new();
        d.imp().size.set(size);
        d.set_valign(gtk::Align::Center);
        d.set_halign(gtk::Align::Center);
        d.set_can_target(false);
        d
    }
    pub fn set(&self, kind: &str, colors: &[&str]) {
        *self.imp().kind.borrow_mut() = kind.to_string();
        *self.imp().colors.borrow_mut() = colors.iter().map(|c| hex(c)).collect();
        self.queue_draw();
    }
    pub fn set_ring(&self, on: bool, color: gdk::RGBA) {
        self.imp().ring.set(on);
        *self.imp().ring_color.borrow_mut() = Some(color);
        self.queue_draw();
    }
}

// ================================================================ siden

struct Ui {
    card_light: Rc<super::lys_kort::CardLight>,
    stack: gtk::Stack,
    overlay: gtk::Overlay,
    flat: FlatCard,
    mode_seg: Seg,
    angle_seg: Seg,
    reset_btn: gtk::Button,
    step_seg: Seg,
    explode: gtk::Scale,
    explode_out: gtk::Label,
    ambient: gtk::Switch,
    caption: gtk::Label,
    keys: gtk::Label,
    loading: gtk::Label,
    note: gtk::Label,
    egg_note: gtk::Label,
    tip_box: gtk::Box,
    tip_name: gtk::Label,
    tip_desc: gtk::Label,
    tip_zone: gtk::Label,
    effects: Vec<(&'static str, gtk::CheckButton, Dot)>,
    bri: (gtk::Box, gtk::Scale, gtk::SpinButton),
    spd: (gtk::Box, gtk::Scale, gtk::SpinButton),
    dir_row: gtk::Box,
    dir_seg: Seg,
    temp_row: gtk::Box,
    t_min: gtk::SpinButton,
    t_max: gtk::SpinButton,
    now_row: gtk::Box,
    now_val: gtk::Label,
    off_note: gtk::Label,
    color_title: gtk::Label,
    temp_seg: Seg,
    zone_seg: Seg,
    swatches: Vec<(String, gtk::Button, Dot)>,
    recent_box: gtk::Box,
    recent_label: gtk::Label,
    now_dot: Dot,
    hex_entry: gtk::Entry,
    custom_btn: gtk::Button,
    picker: gtk::Box,
    parts_panel: Panel,
    parts_list: gtk::Box,
    parts_count: gtk::Label,
    zones_panel: Panel,
    zone_rows: Vec<(gtk::Button, Dot, gtk::Label)>,
    dirty: gtk::Label,
    discard: gtk::Button,
    save: gtk::Button,
    root: gtk::Box,
    actions: gtk::Box,
    info_btn: gtk::Button,
    desc: gtk::Label,
}

pub struct Lys {
    app: Rc<App>,
    ui: Ui,
    engine: RefCell<Engine>,
    draft: RefCell<RgbConfig>,
    view: RefCell<Option<Rc<CardView>>>,
    gl_error: RefCell<Option<String>>,
    /// Det som skal vises nå: «3d» eller «2d».
    pref: RefCell<String>,
    /// Sist valgt med knappene (huskes i `profiler.json` som `lys.picked`).
    picked: RefCell<Option<String>>,
    zone: Cell<usize>,
    temp_target: Cell<bool>,
    egg: Cell<bool>,
    visible: Cell<bool>,
    ticking: Cell<bool>,
    last_us: Cell<i64>,
    flat_draw: Cell<i64>,
    quiet: Cell<bool>,
    pinned: RefCell<Option<&'static str>>,
    release: RefCell<Option<glib::SourceId>>,
    save_view: RefCell<Option<glib::SourceId>>,
    describe_t: RefCell<Option<glib::SourceId>>,
    slow_warned: Cell<bool>,
    spin_demo: Cell<bool>,
    flat_frames: Cell<u64>,
}

thread_local! {
    static CURRENT: RefCell<std::rc::Weak<Lys>> = const { RefCell::new(std::rc::Weak::new()) };
}

fn current() -> Option<Rc<Lys>> {
    CURRENT.with(|c| c.borrow().upgrade())
}

// ---- Til skjermbildene og målingene (styring_bilder.rs) ----

pub fn debug_ready() -> bool {
    current().is_some_and(|p| p.mode() == "2d" || p.view.borrow().as_ref().is_some_and(|v| v.ready()))
}

pub fn debug_info() -> String {
    let Some(p) = current() else { return String::new() };
    let Some(v) = p.view.borrow().clone() else { return format!("2D ({})", p.gl_error.borrow().clone().unwrap_or_default()) };
    let t = v.timing.get();
    let renderer = p.ui.root.native().and_then(|n| n.renderer()).map(|r| r.type_().name().to_string()).unwrap_or_default();
    format!("bygd {:.0} ms, lastet opp {:.0} ms, første bilde {:.0} ms etter at visningen ble laget · {} · vinduets tegner {renderer}", t.build_ms, t.upload_ms, t.first_frame_ms, v.gl_info())
}

/// (bilder tegnet, sum ms i render).
pub fn debug_frames() -> (u64, f64) {
    let Some(p) = current() else { return (0, 0.0) };
    if p.mode() == "2d" {
        return (p.flat_frames.get(), 0.0);
    }
    p.view.borrow().as_ref().map(|v| (v.timing.get().frames, v.timing.get().draw_ms_sum)).unwrap_or((0, 0.0))
}

pub fn debug_bench() -> Option<f64> {
    current().and_then(|p| p.view.borrow().as_ref().and_then(|v| v.bench(60)))
}

pub fn debug_explode(e: f64) {
    if let Some(p) = current()
        && let Some(v) = p.view.borrow().as_ref()
    {
        v.set_explode(e, true);
    }
}

pub fn debug_effect(id: &str) {
    if let Some(p) = current() {
        let id = id.to_string();
        p.set(|d| d.effect = id);
    }
}

pub fn debug_spin(on: bool) {
    if let Some(p) = current() {
        p.spin_demo.set(on);
        p.kick();
    }
}

struct Wrap(Rc<Lys>);

impl Page for Wrap {
    fn widget(&self) -> gtk::Widget {
        self.0.ui.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.0.ui.actions.clone().upcast())
    }
    fn scrolls(&self) -> bool {
        false
    }
    fn show(&self, _q: Option<&str>) {
        self.0.show();
    }
    fn hide(&self) {
        self.0.hide();
    }
    fn on_key(&self, k: &Key) -> bool {
        self.0.key(k)
    }
}

fn zone_index(z: &str) -> usize {
    match z {
        "bar" => 1,
        "fan1" => 2,
        "fan2" => 3,
        _ => 0,
    }
}

fn zone_name(i: usize) -> &'static str {
    match i {
        1 => "Lyslist",
        2 => "Vifte 1",
        3 => "Vifte 2",
        _ => "Alle soner",
    }
}

fn adj_row(name: &str, lo: f64, hi: f64, unit: &str) -> (gtk::Box, gtk::Scale, gtk::SpinButton) {
    let row = hbox(8, &["adj-row"]);
    let k = label(name, &["k", "muted"]);
    k.set_width_chars(9);
    row.append(&k);
    let sc = gtk::Scale::with_range(gtk::Orientation::Horizontal, lo, hi, 1.0);
    sc.set_draw_value(false);
    sc.set_hexpand(true);
    sc.add_css_class("ctl-scale");
    sc.update_property(&[gtk::accessible::Property::Label(name)]);
    row.append(&sc);
    let sp = gtk::SpinButton::with_range(lo, hi, 1.0);
    sp.set_digits(0);
    sp.set_width_chars(3);
    sp.add_css_class("ctl-spin");
    sp.update_property(&[gtk::accessible::Property::Label(name)]);
    row.append(&sp);
    if !unit.is_empty() {
        sp.set_tooltip_text(Some(unit));
    }
    (row, sc, sp)
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "styring", "lys", "fill"]);
    root.append(&crate::widgets::styrer::TargetLine::new(app).root);
    let card_light = super::lys_kort::CardLight::new(app);

    // ---- Forhåndsvisningen ----
    let prev = Panel::new("Lokal lysmodell");
    prev.root.add_css_class("rgb-preview");
    prev.root.set_vexpand(true);
    let ptag = tag("Forhåndsvisning", None, "quiet");
    prev.head.insert_child_after(&ptag, Some(&prev.title));
    let info_btn = button(Some("circle-help"), None, &["flat", "sm"]);
    info_btn.update_property(&[gtk::accessible::Property::Label("Om den lokale lysmodellen")]);
    prev.head.insert_child_after(&info_btn, Some(&ptag));
    let angle_seg = Seg::new(&VIEWS.map(|v| (v.0, v.1)), "skraatt", "Vinkel").small();
    let reset_btn = button(Some("rotate-ccw"), None, &["flat", "sm"]);
    tip(&reset_btn, "Tilbakestill visningen", Some("0"));
    reset_btn.update_property(&[gtk::accessible::Property::Label("Tilbakestill visningen")]);
    let mode_seg = Seg::new(&[("3d", "3D"), ("2d", "2D")], "3d", "Visning").small();
    prev.aside.append(&mode_seg.root);
    prev.body.add_css_class("flush");

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    let flat = FlatCard::new(rigg_core::version::NAME.split(' ').next().unwrap_or("Agentic"));
    stack.add_named(&flat, Some("2d"));
    let overlay = gtk::Overlay::new();
    overlay.add_css_class("rgb-stage");
    overlay.set_child(Some(&stack));
    overlay.set_vexpand(true);
    let chip = |text: &str, h: gtk::Align, v: gtk::Align, cls: &str| {
        let l = label(text, &["stage-chip", cls]);
        l.set_halign(h);
        l.set_valign(v);
        l.set_can_target(false);
        l.set_wrap(true);
        overlay.add_overlay(&l);
        l
    };
    let caption = chip("", gtk::Align::Start, gtk::Align::Start, "stage-caption");
    let keys = chip("Dra for å vri · rull for å zoome · ← → ↑ ↓ vrir · + − zoomer · 0 tilbakestiller", gtk::Align::Start, gtk::Align::End, "stage-hint");
    let loading = chip("Bygger 3D-kortet …", gtk::Align::Center, gtk::Align::Center, "stage-loading");
    let note = chip("Animasjon av", gtk::Align::End, gtk::Align::End, "stage-note");
    let egg_note = chip("Nordlys · lagres ikke · Esc avslutter", gtk::Align::Start, gtk::Align::Start, "egg-note");
    egg_note.set_visible(false);
    let tip_box = vbox(1, &["stage-tip"]);
    tip_box.set_halign(gtk::Align::Start);
    tip_box.set_valign(gtk::Align::Start);
    tip_box.set_can_target(false);
    let tip_name = label("", &["bold"]);
    let tip_desc = label("", &[]);
    let tip_zone = label("", &["sm"]);
    for l in [&tip_name, &tip_desc, &tip_zone] {
        l.set_wrap(true);
        l.set_max_width_chars(34);
        tip_box.append(l);
    }
    tip_box.set_visible(false);
    overlay.add_overlay(&tip_box);
    prev.body.append(&overlay);

    // Linja under scenen flyter over flere linjer i smale vinduer.
    let bar = gtk::FlowBox::new();
    bar.add_css_class("rgb-bar");
    bar.set_selection_mode(gtk::SelectionMode::None);
    bar.set_max_children_per_line(4);
    bar.set_min_children_per_line(1);
    bar.set_column_spacing(16);
    bar.set_row_spacing(6);
    bar.set_homogeneous(false);
    let angle_pair = hbox(6, &["pair", "only-3d"]);
    angle_pair.append(&label("Vinkel", &["muted", "sm"]));
    angle_pair.append(&angle_seg.root);
    angle_pair.append(&reset_btn);
    bar.insert(&angle_pair, -1);
    let step_seg = Seg::new(&[("0", STEPS[0]), ("1", STEPS[1]), ("2", STEPS[2]), ("3", STEPS[3])], "0", "Lag").small();
    let step_pair = hbox(8, &["pair", "only-3d"]);
    step_pair.append(&label("Lag", &["muted", "sm"]));
    step_pair.append(&step_seg.root);
    bar.insert(&step_pair, -1);
    let ex_pair = hbox(8, &["pair", "only-3d"]);
    ex_pair.append(&label("Skill lagene", &["muted", "sm"]));
    let explode = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    explode.set_draw_value(false);
    explode.set_width_request(110);
    explode.add_css_class("ctl-scale");
    explode.update_property(&[gtk::accessible::Property::Label("Skill lagene")]);
    ex_pair.append(&explode);
    let explode_out = label("0 %", &["muted", "sm", "num"]);
    explode_out.set_width_chars(5);
    explode_out.set_xalign(1.0);
    ex_pair.append(&explode_out);
    bar.insert(&ex_pair, -1);
    let amb_pair = hbox(8, &["pair"]);
    amb_pair.append(&label("Bakgrunnslys", &["muted", "sm"]));
    let ambient = gtk::Switch::new();
    ambient.set_valign(gtk::Align::Center);
    ambient.update_property(&[gtk::accessible::Property::Label("Bakgrunnslys")]);
    amb_pair.append(&ambient);
    bar.insert(&amb_pair, -1);
    prev.root.append(&bar);
    let desc = label("", &[]);
    desc.set_visible(false);
    prev.root.append(&desc);

    // ---- Sidekolonnen ----
    let side = vbox(8, &["rgb-side"]);
    let fx = Panel::new("Effekt i modellen");
    let legend = hbox(5, &["fx-legend", "muted", "sm"]);
    legend.append(&Icon::new("circle-help", 14));
    legend.append(&label("lokalt", &[]));
    legend.set_tooltip_text(Some("Alle effektene her tegnes lokalt. Kortlyset styres i den separate seksjonen over."));
    fx.aside.append(&legend);
    let effects_box = vbox(0, &["effects"]);
    effects_box.set_accessible_role(gtk::AccessibleRole::RadioGroup);
    effects_box.update_property(&[gtk::accessible::Property::Label("Effekt")]);
    let mut effects = Vec::new();
    let mut first: Option<gtk::CheckButton> = None;
    for (id, name, _, d) in EFFECTS {
        let row = hbox(8, &["effect"]);
        let radio = gtk::CheckButton::new();
        if let Some(f) = &first {
            radio.set_group(Some(f));
        } else {
            first = Some(radio.clone());
        }
        radio.update_property(&[gtk::accessible::Property::Label(name)]);
        let dot = Dot::new(16);
        let nl = label(name, &["fx-name"]);
        nl.set_hexpand(true);
        let inner = hbox(8, &[]);
        inner.append(&dot);
        inner.append(&nl);
        radio.set_child(Some(&inner));
        radio.set_hexpand(true);
        row.append(&radio);
        row.set_tooltip_text(Some(d));
        effects_box.append(&row);
        effects.push((id, radio, dot));
    }
    fx.body.append(&effects_box);
    let adjust = vbox(0, &["adjust"]);
    let bri = adj_row("Lysstyrke", 0.0, 100.0, "%");
    let spd = adj_row("Hastighet", 1.0, 10.0, "");
    spd.0.set_tooltip_text(Some("1 er langsomst, 10 raskest."));
    let dir_row = hbox(8, &["adj-row"]);
    let dk = label("Retning", &["k", "muted"]);
    dk.set_width_chars(10);
    dir_row.append(&dk);
    let dir_seg = Seg::new(&[("cw", "Med klokka"), ("ccw", "Mot klokka")], "cw", "Retning").small();
    dir_row.append(&dir_seg.root);
    let temp_row = hbox(6, &["adj-row"]);
    let tk = label("Område", &["k", "muted"]);
    tk.set_width_chars(10);
    temp_row.append(&tk);
    let t_min = gtk::SpinButton::with_range(20.0, 95.0, 1.0);
    let t_max = gtk::SpinButton::with_range(25.0, 100.0, 1.0);
    for (sp, n) in [(&t_min, "Kaldest"), (&t_max, "Varmest")] {
        sp.set_digits(0);
        sp.set_width_chars(3);
        sp.add_css_class("ctl-spin");
        sp.update_property(&[gtk::accessible::Property::Label(n)]);
    }
    temp_row.append(&t_min);
    temp_row.append(&label("–", &["muted"]));
    temp_row.append(&t_max);
    temp_row.append(&label(F::temp_label(), &["muted", "sm", "temp-unit"]));
    temp_row.set_tooltip_text(Some("Under det kaldeste er fargen helt kald, over det varmeste helt varm."));
    let now_row = hbox(8, &["adj-row"]);
    let nk = label("GPU nå", &["k", "muted"]);
    nk.set_width_chars(10);
    now_row.append(&nk);
    let now_val = label("–", &["num"]);
    now_row.append(&now_val);
    let off_note = label("Ingen justering når lysene er av.", &["muted", "sm", "empty-adj"]);
    for w in [&bri.0, &spd.0, &dir_row, &temp_row, &now_row] {
        adjust.append(w);
    }
    adjust.append(&off_note);
    fx.body.append(&adjust);
    side.append(&fx.root);

    let color = Panel::new("Farge i modellen");
    let color_title = color.title.clone();
    let temp_seg = Seg::new(&[("cold", "Kald"), ("hot", "Varm")], "cold", "Hvilken farge").small();
    color.aside.append(&temp_seg.root);
    let zone_row = hbox(8, &["zone-row"]);
    zone_row.append(&label("Sone", &["muted", "sm"]));
    let zone_seg = Seg::new(&[("all", "Alle"), ("bar", "Lyslist"), ("fan1", "Vifte 1"), ("fan2", "Vifte 2")], "all", "Sone").small();
    zone_seg.root.set_hexpand(true);
    zone_row.append(&zone_seg.root);
    color.body.append(&zone_row);
    let picker = vbox(6, &["picker"]);
    let sw_row = hbox(4, &["swatches"]);
    sw_row.set_accessible_role(gtk::AccessibleRole::RadioGroup);
    sw_row.update_property(&[gtk::accessible::Property::Label("Fargeprøver")]);
    let mut swatches = Vec::new();
    for (h, n) in SWATCHES {
        let b = gtk::Button::new();
        b.add_css_class("swatch");
        b.add_css_class("flat");
        let d = Dot::new(24);
        d.set("static", &[h]);
        b.set_child(Some(&d));
        b.set_tooltip_text(Some(n));
        b.update_property(&[gtk::accessible::Property::Label(n)]);
        sw_row.append(&b);
        swatches.push((h.to_string(), b, d));
    }
    picker.append(&sw_row);
    let recent_label = label("Nylig brukt", &["muted", "sm"]);
    picker.append(&recent_label);
    let recent_box = hbox(4, &["swatches"]);
    picker.append(&recent_box);
    let hex_row = hbox(8, &["hex-row"]);
    let now_dot = Dot::new(22);
    hex_row.append(&now_dot);
    let hex_entry = gtk::Entry::new();
    hex_entry.set_max_length(7);
    hex_entry.set_width_chars(8);
    hex_entry.add_css_class("mono");
    hex_entry.update_property(&[gtk::accessible::Property::Label("Farge som hex")]);
    hex_row.append(&hex_entry);
    let custom_btn = button(Some("palette"), Some("Egendefinert …"), &["sm", "flat"]);
    hex_row.append(&custom_btn);
    picker.append(&hex_row);
    color.body.append(&picker);
    side.append(&color.root);

    let parts_panel = Panel::new("Deler i modellen");
    let parts_count = label("", &["muted", "sm"]);
    parts_panel.aside.append(&parts_count);
    let parts_list = vbox(0, &["rgb-parts"]);
    parts_panel.body.append(&parts_list);
    parts_panel.root.add_css_class("grow");
    parts_panel.root.set_vexpand(true);
    side.append(&parts_panel.root);

    let zones_panel = Panel::new("Soner i modellen");
    zones_panel.aside.append(&label("56 tegnede punkter", &["muted", "sm"]));
    let mut zone_rows = Vec::new();
    for (_, name, a, b) in ZONES.iter() {
        let btn = gtk::Button::new();
        btn.add_css_class("flat");
        btn.add_css_class("zone-item");
        let r = hbox(8, &[]);
        let d = Dot::new(16);
        r.append(&d);
        let n = label(name, &[]);
        n.set_hexpand(true);
        r.append(&n);
        r.append(&label(&format!("{} punkter", b - a), &["muted", "sm"]));
        let hx = label("", &["mono", "muted", "sm"]);
        hx.set_width_chars(8);
        hx.set_xalign(1.0);
        r.append(&hx);
        btn.set_child(Some(&r));
        btn.set_tooltip_text(Some(&format!("Velg sonen «{name}»")));
        zones_panel.body.append(&btn);
        zone_rows.push((btn, d, hx));
    }
    side.append(&zones_panel.root);
    let side_scroll = gtk::ScrolledWindow::new();
    side_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    side_scroll.set_propagate_natural_height(true);
    side_scroll.set_child(Some(&side));

    let cols = Columns::new(&prev.root, &side_scroll, 320, 860);
    cols.set_fill(true);
    cols.set_vexpand(true);
    let outer = gtk::ScrolledWindow::new();
    outer.set_hscrollbar_policy(gtk::PolicyType::Never);
    let content = vbox(8, &[]);
    content.append(&card_light.root);
    content.append(&cols);
    outer.set_child(Some(&content));
    outer.set_vexpand(true);
    root.append(&outer);

    let actions = hbox(6, &["styring-actions"]);
    actions.set_valign(gtk::Align::Center);
    let dirty = label("", &["muted", "sm", "dirty-text"]);
    let discard = button(None, Some("Forkast"), &["sm", "flat"]);
    let save = button(Some("save"), Some("Lagre i appen"), &["sm", "suggested"]);
    actions.append(&dirty);
    actions.append(&discard);
    actions.append(&save);

    let stored = locked::stored(app);
    let saved = if stored.rgb.is_object() { RgbConfig::from_json(&stored.rgb) } else { RgbConfig::default() };
    // Valget brukeren sist gjorde med knappene 3D/2D (bare det, ikke det som vises).
    let picked = stored.lys["picked"].as_str().filter(|v| matches!(*v, "3d" | "2d")).map(String::from);
    let pref = initial_view(&app.settings().lys3d, picked.as_deref()).to_string();
    let zone = zone_index(stored.lys["zone"].as_str().unwrap_or("all"));

    let ui = Ui {
        card_light,
        stack,
        overlay,
        flat,
        mode_seg,
        angle_seg,
        reset_btn,
        step_seg,
        explode,
        explode_out,
        ambient,
        caption,
        keys,
        loading,
        note,
        egg_note,
        tip_box,
        tip_name,
        tip_desc,
        tip_zone,
        effects,
        bri,
        spd,
        dir_row,
        dir_seg,
        temp_row,
        t_min,
        t_max,
        now_row,
        now_val,
        off_note,
        color_title,
        temp_seg,
        zone_seg,
        swatches,
        recent_box,
        recent_label,
        now_dot,
        hex_entry,
        custom_btn,
        picker,
        parts_panel,
        parts_list,
        parts_count,
        zones_panel,
        zone_rows,
        dirty,
        discard,
        save,
        root,
        actions,
        info_btn,
        desc,
    };
    let page = Rc::new(Lys {
        app: app.clone(),
        ui,
        engine: RefCell::new(Engine::new()),
        draft: RefCell::new(saved),
        view: RefCell::new(None),
        gl_error: RefCell::new(None),
        pref: RefCell::new(pref),
        picked: RefCell::new(picked),
        zone: Cell::new(zone),
        temp_target: Cell::new(false),
        egg: Cell::new(false),
        visible: Cell::new(false),
        ticking: Cell::new(false),
        last_us: Cell::new(0),
        flat_draw: Cell::new(0),
        quiet: Cell::new(false),
        pinned: RefCell::new(None),
        release: RefCell::new(None),
        save_view: RefCell::new(None),
        describe_t: RefCell::new(None),
        slow_warned: Cell::new(false),
        spin_demo: Cell::new(false),
        flat_frames: Cell::new(0),
    });
    CURRENT.with(|c| *c.borrow_mut() = Rc::downgrade(&page));
    page.wire();
    page.theme_colors();
    page.render();
    page.render_mode();
    Box::new(Wrap(page))
}

impl Lys {
    fn motion(&self) -> Level {
        crate::motion::level(&self.app)
    }

    fn saved(&self) -> RgbConfig {
        let s = locked::stored(&self.app);
        if s.rgb.is_object() { RgbConfig::from_json(&s.rgb) } else { RgbConfig::default() }
    }

    fn zone_locked(&self) -> bool {
        self.egg.get() || matches!(self.draft.borrow().effect.as_str(), "temperature" | "rainbow" | "off")
    }

    // ---------------------------------------------------------------- koblingene

    fn wire(self: &Rc<Self>) {
        let ui = &self.ui;
        let w = Rc::downgrade(self);
        ui.mode_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                *p.pref.borrow_mut() = v.to_string();
                *p.picked.borrow_mut() = Some(v.to_string());
                if v == "3d" {
                    p.gl_error.borrow_mut().take();
                    p.slow_warned.set(false);
                }
                p.persist_view();
                p.ensure_renderer();
            }
        });
        let w = Rc::downgrade(self);
        ui.angle_seg.connect(move |v| {
            if let Some(p) = w.upgrade()
                && let Some(cv) = p.view.borrow().as_ref()
            {
                cv.set_view(v);
            }
        });
        let w = Rc::downgrade(self);
        ui.reset_btn.connect_clicked(move |_| {
            if let Some(p) = w.upgrade()
                && let Some(cv) = p.view.borrow().as_ref()
            {
                cv.reset_view();
            }
        });
        let w = Rc::downgrade(self);
        ui.step_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                let n: usize = v.parse().unwrap_or(0);
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.set_step(n);
                }
                p.render_parts();
                p.ui.step_seg.root.update_property(&[gtk::accessible::Property::Description(&format!("{}.", STEPS[n]))]);
            }
        });
        let w = Rc::downgrade(self);
        ui.explode.connect_value_changed(move |s| {
            if let Some(p) = w.upgrade()
                && !p.quiet.get()
            {
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.set_explode(s.value() / 100.0, true);
                }
                set_text(&p.ui.explode_out, &format!("{}{}%", s.value().round(), F::NNBSP));
            }
        });
        let w = Rc::downgrade(self);
        ui.ambient.connect_active_notify(move |s| {
            if let Some(p) = w.upgrade()
                && !p.quiet.get()
            {
                let on = s.is_active();
                p.set(|d| d.ambient = on);
            }
        });
        for (id, radio, _) in &ui.effects {
            let w = Rc::downgrade(self);
            let id = *id;
            radio.connect_toggled(move |r| {
                if r.is_active()
                    && let Some(p) = w.upgrade()
                    && !p.quiet.get()
                {
                    p.set(|d| d.effect = id.to_string());
                }
            });
        }
        for (key, (_, sc, sp)) in [("brightness", &ui.bri), ("speed", &ui.spd)] {
            for r in [sc.clone().upcast::<gtk::Widget>(), sp.clone().upcast()] {
                let w = Rc::downgrade(self);
                let f = move |v: f64| {
                    if let Some(p) = w.upgrade()
                        && !p.quiet.get()
                    {
                        p.set(|d| if key == "brightness" { d.brightness = v.round() } else { d.speed = v.round() });
                    }
                };
                if let Some(s) = r.downcast_ref::<gtk::Scale>() {
                    s.connect_value_changed(move |s| f(s.value()));
                } else if let Some(s) = r.downcast_ref::<gtk::SpinButton>() {
                    s.connect_value_changed(move |s| f(s.value()));
                }
            }
        }
        let w = Rc::downgrade(self);
        ui.dir_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                p.set(|d| d.direction = v.to_string());
            }
        });
        for (sp, hot) in [(&ui.t_min, false), (&ui.t_max, true)] {
            let w = Rc::downgrade(self);
            sp.connect_value_changed(move |sp| {
                let Some(p) = w.upgrade() else { return };
                if p.quiet.get() {
                    return;
                }
                let c = F::temp_in(sp.value()).round();
                let d = p.draft.borrow().clone();
                if (!hot && c > d.temp_max - 5.0) || (hot && c < d.temp_min + 5.0) {
                    p.render();
                    return;
                }
                p.set(|d| if hot { d.temp_max = c } else { d.temp_min = c });
            });
        }
        let w = Rc::downgrade(self);
        ui.temp_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                p.temp_target.set(v == "hot");
                p.render();
            }
        });
        let w = Rc::downgrade(self);
        ui.zone_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                p.pick_zone(zone_index(v));
            }
        });
        for (i, (b, _, _)) in ui.zone_rows.iter().enumerate() {
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = w.upgrade()
                    && !p.zone_locked()
                {
                    let z = if p.zone.get() == i + 1 { 0 } else { i + 1 };
                    p.pick_zone(z);
                }
            });
        }
        for (h, b, _) in &ui.swatches {
            let w = Rc::downgrade(self);
            let h = h.clone();
            b.connect_clicked(move |_| {
                if let Some(p) = w.upgrade() {
                    p.pick_color(&h);
                }
            });
        }
        let w = Rc::downgrade(self);
        ui.hex_entry.connect_activate(move |e| {
            let Some(p) = w.upgrade() else { return };
            let mut t = e.text().trim().to_lowercase();
            if !t.starts_with('#') {
                t.insert(0, '#');
            }
            if rgb::valid_hex(&t) {
                p.pick_color(&t);
            } else {
                p.app.toast(crate::toast::Toast::new("Ugyldig farge").body("Skriv seks heksadesimale siffer, for eksempel #3a7bf0."));
                p.render();
            }
        });
        let w = Rc::downgrade(self);
        ui.custom_btn.connect_clicked(move |b| {
            if let Some(p) = w.upgrade() {
                p.custom_popover(b);
            }
        });
        let w = Rc::downgrade(self);
        ui.info_btn.connect_clicked(move |b| {
            if w.upgrade().is_some() {
                let text = label("Denne modellen tegner lokale effekter og tre illustrerte soner. Den er ikke en avlesning av kortet. Kortlyset har én sone og styres bare fra «Lys på kortet» over modellen.", &["pop-foot"]);
                text.set_wrap(true);
                text.set_max_width_chars(42);
                crate::widgets::popover::popover(b, &text).popup();
            }
        });
        ui.info_btn.set_tooltip_text(Some("Modellen og lagringen er lokale. De viser ikke den avleste effekten på kortet."));
        let w = Rc::downgrade(self);
        ui.discard.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.discard();
            }
        });
        let w = Rc::downgrade(self);
        ui.save.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.save();
            }
        });
        // Fra appen: prøver, tema, bevegelse, enhet og bryteren for påskeegg.
        let w = Rc::downgrade(self);
        self.app.on_telemetry.connect(move |_| {
            let Some(p) = w.upgrade() else { return };
            let s = p.app.gpu_sample().unwrap_or_default();
            p.engine.borrow_mut().set_temp(s.temp);
            if let Some(f) = s.fan
                && let Some(cv) = p.view.borrow().as_ref()
            {
                cv.show_fan_speed(f);
            }
            if p.visible.get() {
                if p.draft.borrow().effect == "temperature" {
                    set_text(&p.ui.now_val, &F::temp(s.temp, 0));
                }
                p.kick();
            }
        });
        for hub in [&self.app.on_service, &self.app.on_devices] {
            let w = Rc::downgrade(self);
            hub.connect(move |_| {
                if let Some(p) = w.upgrade() {
                    p.ui.card_light.render();
                    p.render();
                }
            });
        }
        let w = Rc::downgrade(self);
        self.app.on_theme.connect(move |_| {
            if let Some(p) = w.upgrade() {
                p.theme_colors();
                p.render();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_settings.connect(move |k| {
            let Some(p) = w.upgrade() else { return };
            if k.iter().any(|k| k == "lys3d") {
                let next = initial_view(&p.app.settings().lys3d, p.picked.borrow().as_deref()).to_string();
                if *p.pref.borrow() != next {
                    *p.pref.borrow_mut() = next;
                    p.ensure_renderer();
                }
            }
            if k.iter().any(|k| k == "motion") {
                let m = p.motion();
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.set_motion(m);
                }
                p.engine.borrow_mut().mark_dirty();
                p.sync_notes();
                p.kick();
            }
            if k.iter().any(|k| k == "surprises") && !p.app.settings().surprises && p.egg.get() {
                p.set_egg(false);
            }
            if k.iter().any(|k| k == "unit") {
                p.render();
            }
        });
    }

    fn theme_colors(&self) {
        let bg = crate::theme::rgba(&self.app, "--stage");
        let ac = crate::theme::rgba(&self.app, "--stage-focus");
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_colors(bg, ac);
        }
        let ring = crate::theme::rgba(&self.app, "--accent");
        for (_, _, d) in &self.ui.swatches {
            d.imp().ring_color.replace(Some(ring));
        }
    }

    // ---------------------------------------------------------------- 2D eller 3D

    fn wanted(&self) -> &'static str {
        if self.gl_error.borrow().is_some() || *self.pref.borrow() == "2d" {
            return "2d";
        }
        if self.slow_warned.get() {
            return "2d";
        }
        "3d"
    }

    /// Bygger det som skal vises (3D eller 2D) og river det andre.
    fn ensure_renderer(self: &Rc<Self>) {
        let want = self.wanted();
        if want == "3d" && self.visible.get() {
            if let Some(t) = self.release.borrow_mut().take() {
                t.remove();
            }
            if self.view.borrow().is_none() {
                self.build_3d();
            }
            self.ui.stack.set_visible_child_name("3d");
        } else if want == "2d" {
            self.drop_3d();
            self.ui.stack.set_visible_child_name("2d");
        }
        self.render_mode();
        self.render_parts();
        self.describe_soon();
        self.kick();
    }

    fn build_3d(self: &Rc<Self>) {
        let cv = CardView::new();
        cv.set_motion(self.motion());
        let s = locked::stored(&self.app);
        let l = &s.lys;
        if l["yaw"].is_number() {
            cv.restore(lys3d::Saved {
                yaw: l["yaw"].as_f64().unwrap_or(-33.0) as f32,
                pitch: l["pitch"].as_f64().unwrap_or(18.0) as f32,
                zoom: l["zoom"].as_f64().unwrap_or(1.0) as f32,
                step: l["step"].as_u64().unwrap_or(0) as usize,
                explode: l["explode"].as_f64().unwrap_or(0.0) as f32,
            });
        }
        let w = Rc::downgrade(self);
        *cv.on_wake.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.kick();
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_change.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.describe_soon();
                p.persist_view_soon();
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_hover.borrow_mut() = Some(Box::new(move |info, x, y| {
            if let Some(p) = w.upgrade() {
                p.show_tip(info, x, y);
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_zone.borrow_mut() = Some(Box::new(move |z| {
            if let Some(p) = w.upgrade()
                && !p.zone_locked()
            {
                p.pick_zone(zone_index(z));
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_part.borrow_mut() = Some(Box::new(move |info| {
            if let Some(p) = w.upgrade() {
                p.toggle_pinned(info.id);
            }
        }));
        let w = Rc::downgrade(self);
        // `on_ready` kalles fra GLAreas tegning (midt i snapshot). Endrer vi
        // sidekolonnen der, mangler den allokering når den tegnes rett etter
        // («Trying to snapshot GtkScrolledWindow … without a current
        // allocation»). Derfor først når bildet er ferdig.
        *cv.on_ready.borrow_mut() = Some(Box::new(move || {
            let w = w.clone();
            glib::idle_add_local_once(move || {
                if let Some(p) = w.upgrade() {
                    p.ui.loading.set_visible(false);
                    p.render_parts();
                    p.describe_soon();
                }
            });
        }));
        let w = Rc::downgrade(self);
        *cv.on_fail.borrow_mut() = Some(Box::new(move |msg| {
            let Some(p) = w.upgrade() else { return };
            *p.gl_error.borrow_mut() = Some(msg);
            let p2 = p.clone();
            glib::idle_add_local_once(move || {
                p2.ensure_renderer();
                p2.app.toast(crate::toast::Toast::new("Lys vises i 2D").body("3D-kortet kunne ikke tegnes med grafikken her. Det flate kortet viser de samme effektene."));
            });
        }));
        // GL-bildet går rett til kompositoren som en egen flate (dmabuf) når
        // det går. Da slipper vinduets tegner å laste det ned fra GPU-en
        // hvert bilde, som er det som koster med cairo (runde 3C).
        let offload = gtk::GraphicsOffload::new(Some(&cv.area));
        offload.set_hexpand(true);
        offload.set_vexpand(true);
        self.ui.stack.add_named(&offload, Some("3d"));
        self.ui.loading.set_visible(true);
        let bg = crate::theme::rgba(&self.app, "--stage");
        let ac = crate::theme::rgba(&self.app, "--stage-focus");
        cv.set_colors(bg, ac);
        cv.set_ambient_enabled(self.draft.borrow().ambient);
        if let Some(f) = self.app.gpu_sample().and_then(|s| s.fan) {
            cv.show_fan_speed(f);
        }
        cv.set_running(self.visible.get());
        cv.set_lights(&self.engine.borrow());
        *self.view.borrow_mut() = Some(cv);
    }

    fn drop_3d(&self) {
        if let Some(t) = self.release.borrow_mut().take() {
            t.remove();
        }
        let Some(cv) = self.view.borrow_mut().take() else { return };
        self.persist_view_now(Some(&cv));
        self.hide_tip();
        cv.set_running(false);
        let child: gtk::Widget = cv.area.parent().filter(|p| p.is::<gtk::GraphicsOffload>()).unwrap_or_else(|| cv.area.clone().upcast());
        self.ui.stack.remove(&child);
        self.ui.loading.set_visible(false);
        // GL-ressursene er borte; gi minnet tilbake når CardView er sluppet.
        glib::idle_add_local_once(crate::widgets::trim_memory);
    }

    fn mode(&self) -> &'static str {
        if self.view.borrow().is_some() && self.wanted() == "3d" { "3d" } else { "2d" }
    }

    fn render_mode(&self) {
        let mode = self.mode();
        self.ui.mode_seg.set(mode);
        let three = mode == "3d";
        class(&self.ui.root, "mode-2d", !three);
        for w in [self.ui.caption.clone().upcast::<gtk::Widget>(), self.ui.keys.clone().upcast()] {
            w.set_visible(three);
        }
        // Parene for 3D ligger i hvert sitt FlowBoxChild: skjul det, ikke bare innholdet.
        for w in [self.ui.angle_seg.root.clone().upcast::<gtk::Widget>(), self.ui.step_seg.root.clone().upcast(), self.ui.explode.clone().upcast()] {
            if let Some(child) = w.parent().and_then(|p| p.parent()) {
                child.set_visible(three);
            }
        }
        self.ui.parts_panel.root.set_visible(three);
        self.ui.zones_panel.root.set_visible(!three);
        if let Some(b) = self.ui.mode_seg.button("3d") {
            let t = match self.gl_error.borrow().as_ref() {
                Some(e) => format!("3D kan ikke tegnes her ({e}). Klikk for å prøve igjen."),
                None if self.slow_warned.get() => "Grafikken tegnet 3D for tregt. Klikk for å prøve igjen.".to_string(),
                None => "Kortet i 3D. Dra for å vri.".to_string(),
            };
            b.set_tooltip_text(Some(&t));
        }
        if let Some(b) = self.ui.mode_seg.button("2d") {
            b.set_tooltip_text(Some("Flatt kort sett forfra. Bruker minst."));
        }
        if !three {
            self.ui.loading.set_visible(false);
        }
        self.sync_notes();
    }

    fn sync_notes(&self) {
        let m = self.motion();
        let slow = self.view.borrow().as_ref().is_some_and(|v| v.slow()) && m == Level::Full;
        self.ui.note.set_visible(m != Level::Full || slow);
        set_text(&self.ui.note, if slow { "Stillbilde: grafikken tegner for tregt" } else { "Animasjon av" });
    }

    fn persist_view_soon(self: &Rc<Self>) {
        if let Some(t) = self.save_view.borrow_mut().take() {
            t.remove();
        }
        let w = Rc::downgrade(self);
        *self.save_view.borrow_mut() = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
            if let Some(p) = w.upgrade() {
                p.save_view.borrow_mut().take();
                p.persist_view();
            }
        }));
    }

    fn persist_view(&self) {
        let cv = self.view.borrow().clone();
        self.persist_view_now(cv.as_ref());
    }

    /// Vinkel, zoom, lag og skill lagene, valget 2D/3D og sonen (`profiler.json`).
    fn persist_view_now(&self, cv: Option<&Rc<CardView>>) {
        let mut v = locked::stored(&self.app).lys;
        if !v.is_object() {
            v = json!({});
        }
        if let Some(s) = cv.and_then(|c| c.saved()) {
            v["yaw"] = json!((s.yaw * 10.0).round() / 10.0);
            v["pitch"] = json!((s.pitch * 10.0).round() / 10.0);
            v["zoom"] = json!((s.zoom * 100.0).round() / 100.0);
            v["step"] = json!(s.step);
            v["explode"] = json!((s.explode * 100.0).round() / 100.0);
        }
        v["view"] = json!(*self.pref.borrow());
        if let Some(pk) = self.picked.borrow().as_ref() {
            v["picked"] = json!(pk);
        }
        v["zone"] = json!(["all", "bar", "fan1", "fan2"][self.zone.get()]);
        locked::update(&self.app, "lys", |s| s.lys = v);
    }

    // ---------------------------------------------------------------- løkka

    /// Starter løkka (effekter, 3D, viftene i 2D) til ingenting beveger seg.
    fn kick(self: &Rc<Self>) {
        if !self.visible.get() || self.ticking.replace(true) {
            return;
        }
        self.last_us.set(0);
        let w = Rc::downgrade(self);
        self.ui.overlay.add_tick_callback(move |_, clock| {
            let Some(p) = w.upgrade() else { return glib::ControlFlow::Break };
            if p.step(clock.frame_time()) {
                glib::ControlFlow::Continue
            } else {
                p.ticking.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    fn step(&self, now_us: i64) -> bool {
        if !self.visible.get() {
            return false;
        }
        // Med cairo-tegneren lastes hvert 3D-bilde ned fra GPU-en, og 2D-kortet
        // rastreres på CPU: høyst ca. 30 bilder/s i løkka (input tegnes straks).
        // Det halverer CPU-en mens Lys animerer (runde 3C, maalinger.md).
        if crate::widgets::cairo_renderer() {
            let prev = self.last_us.get();
            if prev > 0 && now_us - prev < 30_000 {
                return true;
            }
        }
        let last = self.last_us.replace(now_us);
        let dt = if last > 0 { ((now_us - last) as f64 / 1e6).min(0.1) } else { 1.0 / 60.0 };
        let full = self.motion() == Level::Full;
        let slow = self.view.borrow().as_ref().is_some_and(|v| v.slow());
        let animate = full && !slow;
        let busy_fx = self.engine.borrow_mut().tick(dt, animate);
        let changed = std::mem::take(&mut self.engine.borrow_mut().changed);
        let three = self.mode() == "3d";
        let mut busy = busy_fx;
        if three {
            let cv = self.view.borrow().clone();
            if let Some(cv) = cv {
                if changed {
                    cv.set_lights(&self.engine.borrow());
                }
                if self.spin_demo.get() {
                    cv.nudge_yaw((dt * 40.0) as f32);
                    busy = true;
                }
                busy |= cv.advance();
                if cv.slow() && !self.slow_warned.get() {
                    self.slow_warned.set(true);
                    self.app.toast(crate::toast::Toast::new("Lys vises i 2D").body("Grafikken tegnet 3D-kortet for tregt. Velg 3D over forhåndsvisningen for å prøve igjen."));
                    self.sync_notes();
                }
                if !cv.ready() {
                    busy = true;
                }
            }
        } else {
            if changed {
                self.ui.flat.set_lights(&self.engine.borrow(), self.draft.borrow().ambient);
            }
            if full {
                // Viftene snurrer høyst ca. 60 ganger i sekundet.
                if now_us - self.flat_draw.get() >= 14_000 {
                    let pct = self.app.gpu_sample().and_then(|s| s.fan).unwrap_or(35.0);
                    let el = if self.flat_draw.get() > 0 { ((now_us - self.flat_draw.get()) as f64 / 1e6).min(0.1) } else { 1.0 / 60.0 };
                    self.ui.flat.spin(el as f32, pct);
                    self.flat_draw.set(now_us);
                    self.flat_frames.set(self.flat_frames.get() + 1);
                }
                busy = true;
            } else {
                self.flat_draw.set(0);
            }
        }
        busy
    }

    fn show(self: &Rc<Self>) {
        self.visible.set(true);
        self.ui.card_light.show();
        if let Some(t) = self.release.borrow_mut().take() {
            t.remove();
        }
        self.engine.borrow_mut().mark_dirty();
        self.ensure_renderer();
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_running(true);
        }
        self.ui.flat.set_lights(&self.engine.borrow(), self.draft.borrow().ambient);
        self.render();
        self.kick();
    }

    fn hide(self: &Rc<Self>) {
        // Ingenting tegnes eller roteres mens siden ikke vises. 3D frigjøres etter 30 s.
        self.visible.set(false);
        self.ui.card_light.hide();
        if self.egg.get() {
            self.set_egg(false);
        }
        self.hide_tip();
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_running(false);
        }
        self.persist_view();
        let w = Rc::downgrade(self);
        *self.release.borrow_mut() = Some(glib::timeout_add_seconds_local_once(RELEASE_S, move || {
            if let Some(p) = w.upgrade() {
                p.release.borrow_mut().take();
                if !p.visible.get() {
                    p.drop_3d();
                }
            }
        }));
    }

    fn key(self: &Rc<Self>, k: &Key) -> bool {
        if k.typing || k.ctrl || k.alt {
            if k.ctrl && matches!(k.key, gdk::Key::Return | gdk::Key::KP_Enter) {
                self.save();
                return true;
            }
            if k.ctrl && !k.typing && matches!(k.key, gdk::Key::z | gdk::Key::Z) {
                self.discard();
                return true;
            }
            return false;
        }
        match crate::easter::on_type_lys(&self.app, k.key) {
            Some(true) => {
                self.set_egg(true);
                true
            }
            Some(false) if self.egg.get() => {
                self.set_egg(false);
                true
            }
            _ => false,
        }
    }

    // ---------------------------------------------------------------- påskeegget

    /// Nordlys over forhåndsvisningen. Utkastet og «Lagre i appen» er urørt.
    fn set_egg(self: &Rc<Self>, on: bool) {
        if self.egg.replace(on) == on {
            return;
        }
        self.engine.borrow_mut().set_egg(on, self.motion() == Level::Full);
        self.ui.egg_note.set_visible(on);
        self.ui.caption.set_visible(!on && self.mode() == "3d");
        if on {
            self.ui.overlay.update_property(&[gtk::accessible::Property::Description("Nordlys · lagres ikke · Esc avslutter")]);
        }
        self.render();
        self.kick();
    }

    // ---------------------------------------------------------------- valget

    fn set(self: &Rc<Self>, f: impl FnOnce(&mut RgbConfig)) {
        if self.egg.get() {
            self.set_egg(false);
        }
        let mut d = self.draft.borrow().clone();
        f(&mut d);
        if d.temp_max - d.temp_min < 5.0 {
            return;
        }
        *self.draft.borrow_mut() = d;
        self.render();
    }

    fn pick_zone(self: &Rc<Self>, z: usize) {
        if self.egg.get() {
            self.set_egg(false);
        }
        self.zone.set(z);
        self.persist_view();
        self.render();
        self.ui.zone_seg.root.update_property(&[gtk::accessible::Property::Description(&format!("Sone: {}", zone_name(z)))]);
    }

    fn pick_color(self: &Rc<Self>, h: &str) {
        let h = h.to_lowercase();
        let temp = self.draft.borrow().effect == "temperature";
        let z = self.zone.get();
        let hot = self.temp_target.get();
        self.set(|d| {
            if temp {
                if hot {
                    d.temp_hot = h.clone();
                } else {
                    d.temp_cold = h.clone();
                }
            } else if z == 0 {
                d.color = h.clone();
                d.zones = [h.clone(), h.clone(), h.clone()];
            } else {
                d.zones[z - 1] = h.clone();
                if z == 1 {
                    d.color = h.clone();
                }
            }
        });
    }

    fn current_color(&self) -> String {
        let d = self.draft.borrow();
        if d.effect == "temperature" {
            return if self.temp_target.get() { d.temp_hot.clone() } else { d.temp_cold.clone() };
        }
        match self.zone.get() {
            0 => d.zones[0].clone(),
            z => d.zones[z - 1].clone(),
        }
    }

    fn custom_popover(self: &Rc<Self>, anchor: &gtk::Button) {
        let (h0, s0, l0) = rgb::rgb_to_hsl(rgb::hex_to_rgb(&self.current_color()));
        let bx = vbox(6, &["custom-pop"]);
        bx.append(&label("Egendefinert farge", &["pop-title"]));
        let mk = |name: &str, lo: f64, hi: f64, v: f64| {
            let r = hbox(8, &[]);
            let k = label(name, &["muted", "sm"]);
            k.set_width_chars(9);
            r.append(&k);
            let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, lo, hi, 1.0);
            s.set_draw_value(false);
            s.set_hexpand(true);
            s.set_width_request(180);
            s.set_value(v);
            s.add_css_class("ctl-scale");
            s.update_property(&[gtk::accessible::Property::Label(name)]);
            r.append(&s);
            bx.append(&r);
            s
        };
        let hs = mk("Fargetone", 0.0, 359.0, h0.round());
        let ss = mk("Metning", 0.0, 100.0, (s0 * 100.0).round());
        let ls = mk("Lyshet", 5.0, 95.0, (l0 * 100.0).round());
        let pv = Dot::new(28);
        pv.set("static", &[&self.current_color()]);
        bx.append(&pv);
        let update = {
            let (hs, ss, ls, pv) = (hs.clone(), ss.clone(), ls.clone(), pv.clone());
            let w = Rc::downgrade(self);
            move || {
                let c = rgb::hsl_to_rgb(hs.value(), ss.value() / 100.0, ls.value() / 100.0);
                let h = rgb::rgb_to_hex(c);
                pv.set("static", &[&h]);
                if let Some(p) = w.upgrade() {
                    p.pick_color(&h);
                }
            }
        };
        for s in [&hs, &ss, &ls] {
            let u = update.clone();
            s.connect_value_changed(move |_| u());
        }
        let p = crate::widgets::popover::popover(anchor, &bx);
        p.popup();
    }

    fn discard(self: &Rc<Self>) {
        if self.egg.get() {
            self.set_egg(false);
        }
        let saved = self.saved();
        if !self.draft.borrow().differs(&saved) {
            return;
        }
        *self.draft.borrow_mut() = saved;
        self.render();
    }

    /// «Lagre i appen»: husker bare den lokale modellen.
    fn save(self: &Rc<Self>) {
        if self.egg.get() {
            self.set_egg(false);
        }
        let saved = self.saved();
        let mut d = self.draft.borrow().clone();
        if !d.differs(&saved) {
            return;
        }
        let used: Vec<String> = if d.effect == "temperature" { vec![d.temp_cold.clone(), d.temp_hot.clone()] } else { d.zones.to_vec() };
        let mut recent: Vec<String> = Vec::new();
        for c in used.into_iter().chain(d.recent.iter().cloned()) {
            if !recent.contains(&c) {
                recent.push(c);
            }
        }
        recent.truncate(6);
        d.recent = recent;
        let v = d.to_json();
        locked::update(&self.app, "rgb", |s| s.rgb = v);
        *self.draft.borrow_mut() = d.clone();
        self.render();
        self.app.toast(crate::toast::Toast::new("Forhåndsvisningen er lagret").body(&format!("{}, {} %. Dette endrer ikke lyset på kortet.", rgb::effect_name(&d.effect), d.brightness)));
    }

    // ---------------------------------------------------------------- tegning av kontrollene

    fn render(self: &Rc<Self>) {
        self.ui.card_light.render();
        let d = self.draft.borrow().clone();
        self.quiet.set(true);
        {
            let mut e = self.engine.borrow_mut();
            e.set_config(&d, self.motion() == Level::Full);
            e.set_zone(if self.zone_locked() { 0 } else { self.zone.get() });
        }
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_ambient_enabled(d.ambient);
        }
        self.ui.ambient.set_active(d.ambient);
        let eff = d.effect.as_str();
        for (id, radio, dot) in &self.ui.effects {
            if (*id == eff) != radio.is_active() && *id == eff {
                radio.set_active(true);
            }
            match *id {
                "temperature" => dot.set("temperature", &[&d.temp_cold, &d.temp_hot]),
                "off" => dot.set("static", &["#4a4a50"]),
                k => dot.set(k, &[&d.zones[0]]),
            }
        }
        let temp = eff == "temperature";
        self.ui.temp_seg.root.set_visible(temp);
        self.ui.temp_seg.set(if self.temp_target.get() { "hot" } else { "cold" });
        self.ui.zone_seg.set(["all", "bar", "fan1", "fan2"][self.zone.get()]);
        let zl = matches!(eff, "temperature" | "rainbow" | "off");
        self.ui.zone_seg.root.set_sensitive(!zl);
        let no_color = matches!(eff, "rainbow" | "off");
        self.ui.picker.set_sensitive(!no_color);
        class(&self.ui.picker, "inert", no_color);
        let mixed = !temp && self.zone.get() == 0 && (d.zones[0] != d.zones[1] || d.zones[0] != d.zones[2]);
        set_text(
            &self.ui.color_title,
            &if temp {
                "Modellfarge · temperatur".to_string()
            } else if self.zone.get() == 0 {
                format!("Modellfarge{}", if mixed { " · ulike soner" } else { "" })
            } else {
                format!("Modellfarge · {}", zone_name(self.zone.get()))
            },
        );
        self.ui.color_title.set_tooltip_text(match eff {
            "rainbow" => Some("Regnbuen bruker alle farger."),
            "off" => Some("Lysene er av."),
            _ if mixed => Some("Sonene har ulike farger. Velg en sone for å se fargen den har."),
            _ => None,
        });
        let cur = self.current_color();
        let ring = crate::theme::rgba(&self.app, "--accent");
        for (h, b, dot) in &self.ui.swatches {
            let on = *h == cur;
            dot.set_ring(on, ring);
            b.update_property(&[gtk::accessible::Property::Description(if on { "valgt" } else { "" })]);
        }
        self.ui.now_dot.set("static", &[&cur]);
        if !self.ui.hex_entry.has_focus() {
            self.ui.hex_entry.set_text(&cur);
        }
        crate::widgets::clear(&self.ui.recent_box);
        let rec: Vec<String> = d.recent.iter().take(6).cloned().collect();
        self.ui.recent_label.set_visible(!rec.is_empty());
        self.ui.recent_box.set_visible(!rec.is_empty());
        for h in rec {
            let b = gtk::Button::new();
            b.add_css_class("swatch");
            b.add_css_class("flat");
            let dot = Dot::new(24);
            dot.set("static", &[&h]);
            dot.set_ring(h == cur, ring);
            b.set_child(Some(&dot));
            b.set_tooltip_text(Some(&format!("Nylig brukt · {h}")));
            let w = Rc::downgrade(self);
            let h2 = h.clone();
            b.connect_clicked(move |_| {
                if let Some(p) = w.upgrade() {
                    p.pick_color(&h2);
                }
            });
            self.ui.recent_box.append(&b);
        }
        // Justering: bare det som gjelder effekten.
        for ((_, sc, sp), v) in [(&self.ui.bri, d.brightness), (&self.ui.spd, d.speed)] {
            if sc.value() != v {
                sc.set_value(v);
            }
            if !sp.has_focus() && sp.value() != v {
                sp.set_value(v);
            }
        }
        self.ui.bri.0.set_visible(eff != "off");
        self.ui.spd.0.set_visible(matches!(eff, "breathe" | "rainbow" | "cycle"));
        self.ui.dir_row.set_visible(matches!(eff, "rainbow" | "cycle"));
        self.ui.dir_seg.set(&d.direction);
        self.ui.temp_row.set_visible(temp);
        self.ui.now_row.set_visible(temp);
        self.ui.off_note.set_visible(eff == "off");
        let lo = F::temp_out(Some(20.0)).unwrap_or(20.0).round();
        let hi = F::temp_out(Some(100.0)).unwrap_or(100.0).round();
        self.ui.t_min.set_range(lo, F::temp_out(Some(95.0)).unwrap_or(95.0).round());
        self.ui.t_max.set_range(F::temp_out(Some(25.0)).unwrap_or(25.0).round(), hi);
        for (sp, c) in [(&self.ui.t_min, d.temp_min), (&self.ui.t_max, d.temp_max)] {
            let v = F::temp_out(Some(c)).unwrap_or(c).round();
            if !sp.has_focus() && sp.value() != v {
                sp.set_value(v);
            }
        }
        if let Some(u) = self.ui.temp_row.last_child().and_downcast::<gtk::Label>() {
            set_text(&u, F::temp_label());
        }
        if temp {
            set_text(&self.ui.now_val, &F::temp(self.app.gpu_sample().and_then(|s| s.temp), 0));
        }
        // Sonene (2D): farge, navn og hex.
        for (i, (b, dot, hx)) in self.ui.zone_rows.iter().enumerate() {
            let c = if temp { d.temp_cold.clone() } else { d.zones[i].clone() };
            dot.set(if eff == "rainbow" { "rainbow" } else { "static" }, &[&c]);
            set_text(hx, if no_color || temp { "" } else { &d.zones[i] });
            class(b, "selected", self.zone.get() == i + 1 && !zl);
            b.set_sensitive(!zl);
        }
        // Ikke lagret-tilstand.
        let saved = self.saved();
        let dirty = d.differs(&saved);
        set_text(&self.ui.dirty, &if dirty { "Forhåndsvisning – ikke lagret".to_string() } else { format!("Lagret: {}", rgb::effect_name(&saved.effect)) });
        self.ui.discard.set_sensitive(dirty);
        self.ui.save.set_sensitive(dirty);
        tip(&self.ui.save, if dirty { "Lagre forhåndsvisningen lokalt. Dette endrer ikke kortlyset." } else { "Ingen endringer å lagre" }, Some("Ctrl+Enter"));
        tip(&self.ui.discard, if dirty { "Forkast endringene" } else { "Ingen endringer å forkaste" }, Some("Ctrl+Z"));
        self.quiet.set(false);
        self.describe_soon();
        self.kick();
    }

    fn toggle_pinned(self: &Rc<Self>, id: &'static str) {
        let next = if *self.pinned.borrow() == Some(id) { None } else { Some(id) };
        *self.pinned.borrow_mut() = next;
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_focus_group(next);
        }
        self.render_parts();
    }

    /// Dellisten (3D): peking eller fokus fremhever delen, klikk fester den.
    fn render_parts(self: &Rc<Self>) {
        crate::widgets::clear(&self.ui.parts_list);
        let Some(cv) = self.view.borrow().clone() else {
            set_text(&self.ui.parts_count, "");
            return;
        };
        let list = cv.visible_groups();
        if self.pinned.borrow().is_some_and(|p| !list.iter().any(|g| g.id == p)) {
            *self.pinned.borrow_mut() = None;
            cv.set_focus_group(None);
        }
        for g in &list {
            let b = gtk::ToggleButton::new();
            b.add_css_class("flat");
            b.add_css_class("part");
            b.set_active(*self.pinned.borrow() == Some(g.id));
            let bx = vbox(0, &[]);
            bx.append(&label(g.name, &["n"]));
            let dl = label(g.desc, &["d", "muted", "sm"]);
            dl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            bx.append(&dl);
            b.set_child(Some(&bx));
            b.set_tooltip_text(Some(&format!("{} · {}", g.name, g.desc)));
            let id = g.id;
            let motion = gtk::EventControllerMotion::new();
            let w = Rc::downgrade(&cv);
            motion.connect_enter(move |_, _, _| {
                if let Some(cv) = w.upgrade() {
                    cv.set_focus_group(Some(id));
                }
            });
            let w = Rc::downgrade(&cv);
            let p = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let (Some(cv), Some(p)) = (w.upgrade(), p.upgrade()) {
                    cv.set_focus_group(*p.pinned.borrow());
                }
            });
            b.add_controller(motion);
            let w = Rc::downgrade(&cv);
            let p = Rc::downgrade(self);
            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(move |_| {
                if let Some(cv) = w.upgrade() {
                    cv.set_focus_group(Some(id));
                }
            });
            let w2 = Rc::downgrade(&cv);
            focus.connect_leave(move |_| {
                if let (Some(cv), Some(p)) = (w2.upgrade(), p.upgrade()) {
                    cv.set_focus_group(*p.pinned.borrow());
                }
            });
            b.add_controller(focus);
            let p = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = p.upgrade() {
                    let p2 = p.clone();
                    glib::idle_add_local_once(move || p2.toggle_pinned(id));
                }
            });
            self.ui.parts_list.append(&b);
        }
        set_text(&self.ui.parts_count, &if list.is_empty() { String::new() } else { format!("{} synlige", list.len()) });
    }

    fn describe_soon(self: &Rc<Self>) {
        if let Some(t) = self.describe_t.borrow_mut().take() {
            t.remove();
        }
        let w = Rc::downgrade(self);
        *self.describe_t.borrow_mut() = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
            let Some(p) = w.upgrade() else { return };
            p.describe_t.borrow_mut().take();
            p.describe();
        }));
    }

    /// Bildeteksten på scenen og beskrivelsen for skjermlesere.
    fn describe(self: &Rc<Self>) {
        let d = self.draft.borrow().clone();
        let lys = if self.egg.get() {
            "Lys: nordlys (bare forhåndsvisning).".to_string()
        } else if d.effect == "off" {
            "Lysene er av.".to_string()
        } else {
            format!("Lys: {}, {} % lysstyrke.", rgb::effect_name(&d.effect), d.brightness)
        };
        let cv = self.view.borrow().clone();
        let text = match (&cv, self.mode()) {
            (Some(cv), "3d") => {
                let step = cv.step();
                let ex = cv.explode_goal();
                set_text(&self.ui.caption, &format!("{}{}", STEPS[step], if ex > 0.02 { " · lagene skilt" } else { "" }));
                self.quiet.set(true);
                self.ui.angle_seg.set(cv.current_view().unwrap_or(""));
                if cv.current_view().is_none() {
                    for (id, ..) in VIEWS {
                        if let Some(b) = self.ui.angle_seg.button(id) {
                            b.set_active(false);
                        }
                    }
                }
                self.ui.step_seg.set(&step.to_string());
                let pct = (ex * 100.0).round();
                if self.ui.explode.value() != pct {
                    self.ui.explode.set_value(pct);
                }
                set_text(&self.ui.explode_out, &format!("{pct}{}%", F::NNBSP));
                self.quiet.set(false);
                format!("{} {lys}", cv.describe())
            }
            _ => format!("Flatt skjermkort sett forfra. {lys}"),
        };
        set_text(&self.ui.desc, &text);
        self.ui.overlay.update_property(&[gtk::accessible::Property::Description(&text)]);
    }

    fn show_tip(&self, info: Option<lys3d::PartInfo>, x: f32, y: f32) {
        let Some(i) = info else {
            self.hide_tip();
            return;
        };
        set_text(&self.ui.tip_name, i.name);
        set_text(&self.ui.tip_desc, i.desc);
        let zl = self.zone_locked();
        self.ui.tip_zone.set_visible(i.zone.is_some());
        set_text(&self.ui.tip_zone, if zl { "Sonen kan ikke velges med denne effekten." } else { "Klikk for å velge sonen." });
        let (w, h) = (self.ui.overlay.width() as f32, self.ui.overlay.height() as f32);
        let (tw, th) = (self.ui.tip_box.width().max(160) as f32, self.ui.tip_box.height().max(44) as f32);
        let px = (x + 14.0).min(w - tw - 8.0).max(8.0);
        let py = if y + 18.0 + th > h - 8.0 { (y - th - 12.0).max(8.0) } else { y + 18.0 };
        self.ui.tip_box.set_margin_start(px as i32);
        self.ui.tip_box.set_margin_top(py as i32);
        self.ui.tip_box.set_visible(true);
    }

    fn hide_tip(&self) {
        self.ui.tip_box.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::initial_view;

    #[test]
    fn lys_aapner_i_2d_til_3d_er_valgt() {
        assert_eq!(initial_view("auto", None), "2d");
        assert_eq!(initial_view("auto", Some("2d")), "2d");
        assert_eq!(initial_view("auto", Some("3d")), "3d");
        assert_eq!(initial_view("3d", None), "3d");
        assert_eq!(initial_view("3d", Some("2d")), "3d");
        assert_eq!(initial_view("2d", Some("3d")), "2d");
    }
}
