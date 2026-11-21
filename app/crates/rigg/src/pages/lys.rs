//! Lys: 2D/3D-skjermkortvisning og den ekte kortlysstyringen i `lys_kort`.
//!
//! 2D/3D-kortet ligger øverst som visning av kortets ene fysiske lyssone og
//! speiler valgene og sist leste tilstand fra `CardLight`. 3D-kortet
//! (`lys3d::CardView`) lastes bare når 3D er valgt, og frigjøres 30 s etter
//! sidebytte for å spare minne og unngå unødvendig GPU-vekking. Det flate
//! 2D-kortet (`lys3d::flat::FlatCard`) er standard i «Auto» og reserve ved
//! manglende eller treg GL.

use super::lys_kort::{CardLight, VisualState};
use super::{Key, Page};
use crate::lys3d::{self, CardView, STEPS, VIEWS, flat::FlatCard};
use crate::motion::Level;
use crate::rgb::{Engine, PreviewTarget};
use crate::state::App;
use crate::widgets::locked;
use crate::widgets::{Panel, Seg, button, class, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::{gdk, glib};
use rigg_core::{fmt, tr, trf};
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

struct Ui {
    root: gtk::Box,
    card_light: Rc<CardLight>,
    stack: gtk::Stack,
    overlay: gtk::Overlay,
    flat: FlatCard,
    mode_tag: gtk::Label,
    mode_seg: Seg,
    angle_seg: Seg,
    reset_btn: gtk::Button,
    step_seg: Seg,
    explode: gtk::Scale,
    explode_out: gtk::Label,
    ambient: gtk::Switch,
    readout_box: gtk::Box,
    readout_dot: gtk::DrawingArea,
    readout_color: Rc<Cell<Option<[u8; 3]>>>,
    readout_label: gtk::Label,
    caption: gtk::Label,
    keys: gtk::Label,
    loading: gtk::Label,
    note: gtk::Label,
    tip_box: gtk::Box,
    tip_name: gtk::Label,
    tip_desc: gtk::Label,
    tip_zone: gtk::Label,
    parts_expander: gtk::Expander,
    parts_list: gtk::Box,
    desc: gtk::Label,
}

struct Lys {
    app: Rc<App>,
    ui: Ui,
    engine: RefCell<Engine>,
    visual: RefCell<VisualState>,
    view: RefCell<Option<Rc<CardView>>>,
    gl_error: RefCell<Option<String>>,
    pref: RefCell<String>,
    picked: RefCell<Option<String>>,
    ambient_on: Cell<bool>,
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
}

thread_local! {
    static CURRENT: RefCell<std::rc::Weak<Lys>> = const { RefCell::new(std::rc::Weak::new()) };
}

fn current() -> Option<Rc<Lys>> {
    CURRENT.with(|c| c.borrow().upgrade())
}

pub(crate) fn debug_ready() -> bool {
    current().is_some_and(|p| p.mode() == "2d" || p.view.borrow().as_ref().is_some_and(|v| v.ready()))
}

pub(crate) fn debug_mode(mode: &str) {
    if let Some(p) = current()
        && matches!(mode, "2d" | "3d")
    {
        *p.pref.borrow_mut() = mode.to_string();
        *p.picked.borrow_mut() = Some(mode.to_string());
        if mode == "3d" {
            p.gl_error.borrow_mut().take();
            p.slow_warned.set(false);
        }
        p.ensure_renderer();
    }
}

pub(crate) fn debug_explode(e: f64) {
    if let Some(p) = current()
        && let Some(v) = p.view.borrow().as_ref()
    {
        v.set_explode(e, true);
    }
}

struct Wrap(Rc<Lys>);

impl Page for Wrap {
    fn widget(&self) -> gtk::Widget {
        self.0.ui.root.clone().upcast()
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

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "styring", "lys", "fill"]);
    root.append(&crate::widgets::styrer::TargetLine::new(app).root);
    let card_light = CardLight::new(app);

    // ---- 2D/3D-kortvisning øverst ----
    let prev = Panel::new(tr!("Skjermkort"));
    prev.root.add_css_class("rgb-preview");
    let mode_tag = label(tr!("Ikke aktivert"), &["tag", "quiet"]);
    prev.head.insert_child_after(&mode_tag, Some(&prev.title));
    let mode_seg = Seg::new(&[("3d", tr!("3D")), ("2d", tr!("2D"))], "2d", tr!("Visning")).small();
    prev.aside.append(&mode_seg.root);
    prev.body.add_css_class("flush");

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    let flat = FlatCard::new(rigg_core::version::NAME);
    stack.add_named(&flat, Some("2d"));

    let overlay = gtk::Overlay::new();
    overlay.add_css_class("rgb-stage");
    overlay.set_hexpand(true);
    overlay.set_size_request(-1, 290);
    overlay.set_child(Some(&stack));

    let caption = label("", &["stage-chip"]);
    caption.set_halign(gtk::Align::Start);
    caption.set_valign(gtk::Align::End);
    caption.set_can_target(false);
    overlay.add_overlay(&caption);

    let keys = label(tr!("Dra for å vri · rull for zoom · 0 tilbakestiller"), &["stage-chip", "stage-hint"]);
    keys.set_halign(gtk::Align::End);
    keys.set_valign(gtk::Align::End);
    keys.set_can_target(false);
    overlay.add_overlay(&keys);

    let loading = label(tr!("Bygger 3D-kortet …"), &["stage-chip"]);
    loading.set_halign(gtk::Align::Center);
    loading.set_valign(gtk::Align::Center);
    loading.set_can_target(false);
    loading.set_visible(false);
    overlay.add_overlay(&loading);

    let note = label("", &["stage-chip"]);
    note.set_halign(gtk::Align::Start);
    note.set_valign(gtk::Align::Start);
    note.set_can_target(false);
    note.set_visible(false);
    overlay.add_overlay(&note);

    let tip_box = vbox(2, &["stage-tip"]);
    tip_box.set_halign(gtk::Align::Start);
    tip_box.set_valign(gtk::Align::Start);
    tip_box.set_can_target(false);
    tip_box.set_visible(false);
    let tip_name = label("", &["bold"]);
    let tip_desc = label("", &["sm"]);
    let tip_zone = label("", &["sm", "muted"]);
    for l in [&tip_name, &tip_desc, &tip_zone] {
        tip_box.append(l);
    }
    overlay.add_overlay(&tip_box);
    prev.body.append(&overlay);

    // Kompakt visningslinje under scenen.
    let bar = gtk::FlowBox::new();
    bar.add_css_class("rgb-bar");
    bar.set_selection_mode(gtk::SelectionMode::None);
    bar.set_min_children_per_line(1);
    bar.set_max_children_per_line(8);
    bar.set_column_spacing(12);
    bar.set_row_spacing(6);

    let angle_items: Vec<(&str, &str)> = VIEWS.iter().map(|v| (v.0, (v.1)())).collect();
    let angle_seg = Seg::new(&angle_items, "skraatt", tr!("Vinkel")).small();
    let reset_btn = button(Some("rotate-ccw"), None, &["flat", "sm"]);
    tip(&reset_btn, tr!("Tilbakestill visningen"), Some("0"));
    reset_btn.update_property(&[gtk::accessible::Property::Label(tr!("Tilbakestill visningen"))]);
    let angle_pair = hbox(4, &["pair"]);
    angle_pair.set_valign(gtk::Align::Center);
    angle_pair.append(&label(tr!("Vinkel"), &["muted", "sm"]));
    angle_pair.append(&angle_seg.root);
    angle_pair.append(&reset_btn);
    bar.insert(&angle_pair, -1);

    let step_items: Vec<(String, &str)> = STEPS.iter().enumerate().map(|(i, f)| (i.to_string(), f())).collect();
    let step_refs: Vec<(&str, &str)> = step_items.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let step_seg = Seg::new(&step_refs, "0", tr!("Lag")).small();
    let step_pair = hbox(4, &["pair"]);
    step_pair.set_valign(gtk::Align::Center);
    step_pair.append(&label(tr!("Lag"), &["muted", "sm"]));
    step_pair.append(&step_seg.root);
    bar.insert(&step_pair, -1);

    let explode = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    explode.set_draw_value(false);
    explode.set_size_request(110, -1);
    explode.add_css_class("ctl-scale");
    explode.update_property(&[gtk::accessible::Property::Label(tr!("Skill lagene"))]);
    let explode_out = label(&fmt::pct(Some(0.0), 0), &["mono", "muted", "sm"]);
    explode_out.set_width_chars(4);
    let explode_pair = hbox(4, &["pair"]);
    explode_pair.set_valign(gtk::Align::Center);
    explode_pair.append(&label(tr!("Skill lagene"), &["muted", "sm"]));
    explode_pair.append(&explode);
    explode_pair.append(&explode_out);
    bar.insert(&explode_pair, -1);

    let ambient = gtk::Switch::new();
    ambient.set_active(true);
    ambient.set_valign(gtk::Align::Center);
    ambient.update_property(&[gtk::accessible::Property::Label(tr!("Bakgrunnslys"))]);
    let amb_pair = hbox(6, &["pair"]);
    amb_pair.set_valign(gtk::Align::Center);
    amb_pair.append(&label(tr!("Bakgrunnslys"), &["muted", "sm"]));
    amb_pair.append(&ambient);
    bar.insert(&amb_pair, -1);

    let readout_box = hbox(6, &["pair"]);
    readout_box.set_valign(gtk::Align::Center);
    let readout_dot = gtk::DrawingArea::new();
    readout_dot.set_content_width(14);
    readout_dot.set_content_height(14);
    let readout_color = Rc::new(Cell::new(None::<[u8; 3]>));
    let fill = readout_color.clone();
    readout_dot.set_draw_func(move |_, ctx, w, h| {
        if let Some([r, g, b]) = fill.get() {
            ctx.set_source_rgb(f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
            ctx.arc(f64::from(w) / 2.0, f64::from(h) / 2.0, 6.0, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
        }
    });
    let readout_label = label("", &["muted", "sm", "mono"]);
    readout_box.append(&readout_dot);
    readout_box.append(&readout_label);
    readout_box.set_visible(false);
    bar.insert(&readout_box, -1);

    prev.body.append(&bar);

    let parts_list = vbox(2, &["rgb-parts"]);
    let parts_expander = gtk::Expander::new(Some(tr!("Deler i 3D-modellen")));
    parts_expander.add_css_class("rgb-parts-expander");
    parts_expander.set_child(Some(&parts_list));
    parts_expander.set_visible(false);
    prev.body.append(&parts_expander);

    let desc = label("", &[]);
    desc.set_visible(false);
    prev.body.append(&desc);

    let outer = gtk::ScrolledWindow::new();
    outer.set_hscrollbar_policy(gtk::PolicyType::Never);
    outer.set_vexpand(true);
    let content = vbox(12, &[]);
    content.append(&prev.root);
    content.append(&card_light.root);
    outer.set_child(Some(&content));
    root.append(&outer);

    let stored = locked::stored(app);
    let picked = stored.lys["picked"]
        .as_str()
        .filter(|v| matches!(*v, "3d" | "2d"))
        .map(String::from);
    let pref = initial_view(&app.settings().lys3d, picked.as_deref()).to_string();
    let ambient_on = stored.lys["ambient"].as_bool().unwrap_or(true);
    ambient.set_active(ambient_on);
    let initial_visual = card_light.visual_state();

    let ui = Ui {
        root,
        card_light,
        stack,
        overlay,
        flat,
        mode_tag,
        mode_seg,
        angle_seg,
        reset_btn,
        step_seg,
        explode,
        explode_out,
        ambient,
        readout_box,
        readout_dot,
        readout_color,
        readout_label,
        caption,
        keys,
        loading,
        note,
        tip_box,
        tip_name,
        tip_desc,
        tip_zone,
        parts_expander,
        parts_list,
        desc,
    };

    let page = Rc::new(Lys {
        app: app.clone(),
        ui,
        engine: RefCell::new(Engine::new()),
        visual: RefCell::new(initial_visual.clone()),
        view: RefCell::new(None),
        gl_error: RefCell::new(None),
        pref: RefCell::new(pref),
        picked: RefCell::new(picked),
        ambient_on: Cell::new(ambient_on),
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
    });
    CURRENT.with(|c| *c.borrow_mut() = Rc::downgrade(&page));
    page.wire();
    page.apply_visual(&initial_visual);
    page.render_mode();
    Box::new(Wrap(page))
}

impl Lys {
    fn motion(&self) -> Level {
        crate::motion::level(&self.app)
    }

    fn wire(self: &Rc<Self>) {
        let ui = &self.ui;
        let w = Rc::downgrade(self);
        *ui.card_light.on_visual.borrow_mut() = Some(Box::new(move |state| {
            if let Some(p) = w.upgrade() {
                p.apply_visual(state);
            }
        }));
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
                p.ui.step_seg.root.update_property(&[
                    gtk::accessible::Property::Description(&format!("{}.", STEPS[n.min(STEPS.len() - 1)]())),
                ]);
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
                set_text(&p.ui.explode_out, &fmt::pct(Some(s.value().round()), 0));
            }
        });
        let w = Rc::downgrade(self);
        ui.ambient.connect_active_notify(move |s| {
            if let Some(p) = w.upgrade()
                && !p.quiet.get()
            {
                let on = s.is_active();
                p.ambient_on.set(on);
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.set_ambient_enabled(on);
                }
                p.ui.flat.set_lights(&p.engine.borrow(), on);
                p.persist_view();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_telemetry.connect(move |_| {
            if let Some(p) = w.upgrade()
                && p.visible.get()
                && let Some(f) = p.app.gpu_sample().and_then(|s| s.fan)
            {
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.show_fan_speed(f);
                }
                if f > 0.0 && p.motion() == Level::Full {
                    p.kick();
                }
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_theme.connect(move |_| {
            if let Some(p) = w.upgrade() {
                p.theme_colors();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_settings.connect(move |_| {
            if let Some(p) = w.upgrade() {
                let want = initial_view(&p.app.settings().lys3d, p.picked.borrow().as_deref());
                if *p.pref.borrow() != want {
                    *p.pref.borrow_mut() = want.to_string();
                    p.ensure_renderer();
                }
                if let Some(cv) = p.view.borrow().as_ref() {
                    cv.set_motion(p.motion());
                }
                p.sync_notes();
                p.kick();
            }
        });
    }

    fn apply_visual(self: &Rc<Self>, v: &VisualState) {
        *self.visual.borrow_mut() = v.clone();
        // Før aktivering forhåndsviser kortet valget (farge og effekt), slik
        // at brukeren ser hva «Bruk» vil sende.
        let target = if v.bound && v.pending {
            let off = v.observed_color == Some([0, 0, 0])
                || (v.observed_color.is_none() && v.selected_mode == "off");
            PreviewTarget {
                mode: if off { "off".into() } else { "static".into() },
                color: v.observed_color.or(v.selected_color).unwrap_or([58, 123, 240]),
                brightness: v.observed_brightness.unwrap_or(v.selected_brightness).min(100),
            }
        } else {
            PreviewTarget {
                mode: v.selected_mode.clone(),
                color: v.selected_color.or(v.observed_color).unwrap_or([58, 123, 240]),
                brightness: v.selected_brightness.min(100),
            }
        };
        let animate = self.visible.get() && self.motion() == Level::Full;
        self.engine.borrow_mut().set_target(target, animate);
        self.engine.borrow_mut().tick(0.0, animate);
        let amb = self.ambient_on.get();
        self.ui.flat.set_lights(&self.engine.borrow(), amb);
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_lights(&self.engine.borrow());
        }

        // Oppdater statusmerket og kortets sist leste farge.
        let tag_text = if !v.bound {
            tr!("Forhåndsvisning av valg")
        } else if v.pending {
            tr!("Prøve på kortet")
        } else if v.active_effect.is_some() {
            tr!("Aktiv effekt")
        } else {
            let matches_card = match v.selected_mode.as_str() {
                "off" => v.observed_color == Some([0, 0, 0]),
                "static" => {
                    v.selected_color.is_some()
                        && v.selected_color == v.observed_color
                        && Some(v.selected_brightness) == v.observed_brightness
                }
                _ => false,
            };
            if matches_card {
                tr!("Speiler kortlyset")
            } else {
                tr!("Forhåndsvisning av valg")
            }
        };
        set_text(&self.ui.mode_tag, tag_text);

        let show_readout = v.bound && v.observed_color.is_some() && v.observed_brightness.is_some();
        self.ui.readout_box.set_visible(show_readout);
        if let (Some(rgb), Some(bri)) = (v.observed_color, v.observed_brightness) {
            if self.ui.readout_color.replace(Some(rgb)) != Some(rgb) {
                self.ui.readout_dot.queue_draw();
            }
            let hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
            let label_text = if v.observation_source == "stored_baseline" {
                trf!("Utgangspunkt: {farge} · {styrke}", farge = hex, styrke = fmt::pct(Some(f64::from(bri)), 0))
            } else {
                trf!("Sist lest: {farge} · {styrke}", farge = hex, styrke = fmt::pct(Some(f64::from(bri)), 0))
            };
            set_text(&self.ui.readout_label, &label_text);
        }

        self.describe_soon();
        self.kick();
    }

    fn show(self: &Rc<Self>) {
        self.visible.set(true);
        self.ui.card_light.show();
        if let Some(t) = self.release.borrow_mut().take() {
            t.remove();
        }
        self.ensure_renderer();
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_running(true);
        }
        self.theme_colors();
        self.sync_notes();
        self.kick();
    }

    fn hide(self: &Rc<Self>) {
        self.visible.set(false);
        self.ui.card_light.hide();
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_running(false);
        }
        self.hide_tip();
        if self.view.borrow().is_some() && self.release.borrow().is_none() {
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
    }

    fn key(self: &Rc<Self>, k: &Key) -> bool {
        if k.typing || k.ctrl || k.alt {
            return false;
        }
        if self.mode() != "3d" {
            return false;
        }
        let Some(cv) = self.view.borrow().clone() else { return false };
        let step = if k.shift { 15.0 } else { 5.0 };
        match k.key {
            gdk::Key::Left => cv.rotate_by(-step, 0.0),
            gdk::Key::Right => cv.rotate_by(step, 0.0),
            gdk::Key::Up => cv.rotate_by(0.0, step),
            gdk::Key::Down => cv.rotate_by(0.0, -step),
            gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => cv.zoom_by(1.15),
            gdk::Key::minus | gdk::Key::KP_Subtract => cv.zoom_by(1.0 / 1.15),
            gdk::Key::_0 | gdk::Key::KP_0 => cv.reset_view(),
            _ => return false,
        }
        self.kick();
        true
    }

    fn wanted(&self) -> &'static str {
        if self.pref.borrow().as_str() == "3d"
            && self.gl_error.borrow().is_none()
            && !self.slow_warned.get()
        {
            "3d"
        } else {
            "2d"
        }
    }

    fn ensure_renderer(self: &Rc<Self>) {
        if !self.visible.get() {
            self.render_mode();
            return;
        }
        let want = self.wanted();
        if want == "3d" {
            if self.view.borrow().is_none() {
                self.create_3d();
            }
            self.ui.stack.set_visible_child_name("3d");
        } else {
            if self.view.borrow().is_some() {
                self.drop_3d();
            }
            self.ui.stack.set_visible_child_name("2d");
            self.ui.flat.set_lights(&self.engine.borrow(), self.ambient_on.get());
        }
        self.render_mode();
        self.describe_soon();
        self.kick();
    }

    fn create_3d(self: &Rc<Self>) {
        let saved = &locked::stored(&self.app).lys;
        let cv = CardView::new();
        cv.set_motion(self.motion());
        if saved.is_object() {
            let mut s = lys3d::Saved::default();
            if let Some(v) = saved["yaw"].as_f64() {
                s.yaw = v as f32;
            }
            if let Some(v) = saved["pitch"].as_f64() {
                s.pitch = v as f32;
            }
            if let Some(v) = saved["zoom"].as_f64() {
                s.zoom = v as f32;
            }
            if let Some(v) = saved["step"].as_u64() {
                s.step = (v as usize).min(STEPS.len() - 1);
            }
            if let Some(v) = saved["explode"].as_f64() {
                s.explode = v as f32;
            }
            cv.restore(s);
        }
        let w = Rc::downgrade(self);
        *cv.on_change.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.describe_soon();
                p.persist_view_soon();
                p.kick();
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_wake.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.kick();
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_hover.borrow_mut() = Some(Box::new(move |info, x, y| {
            if let Some(p) = w.upgrade() {
                p.show_tip(info, x, y);
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_part.borrow_mut() = Some(Box::new(move |info| {
            if let Some(p) = w.upgrade() {
                p.toggle_pinned(info.id);
            }
        }));
        let w = Rc::downgrade(self);
        *cv.on_ready.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.sync_3d_state();
            }
            let w = w.clone();
            glib::idle_add_local_once(move || {
                if let Some(p) = w.upgrade() {
                    p.ui.loading.set_visible(false);
                    p.sync_3d_state();
                    p.render_parts();
                    p.describe_soon();
                    p.kick();
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
                p2.app.toast(
                    crate::toast::Toast::new(tr!("Lys vises i 2D")).body(tr!(
                        "3D-kortet kunne ikke tegnes med grafikken her. Det flate kortet viser de samme lysvalgene."
                    )),
                );
            });
        }));
        let offload = gtk::GraphicsOffload::new(Some(&cv.area));
        offload.set_hexpand(true);
        offload.set_vexpand(true);
        self.ui.stack.add_named(&offload, Some("3d"));
        self.ui.loading.set_visible(true);
        let bg = crate::theme::rgba(&self.app, "--stage");
        let ac = crate::theme::rgba(&self.app, "--stage-focus");
        cv.set_colors(bg, ac);
        cv.set_ambient_enabled(self.ambient_on.get());
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
        let child: gtk::Widget = cv
            .area
            .parent()
            .filter(|p| p.is::<gtk::GraphicsOffload>())
            .unwrap_or_else(|| cv.area.clone().upcast());
        self.ui.stack.remove(&child);
        self.ui.loading.set_visible(false);
        glib::idle_add_local_once(crate::widgets::trim_memory);
    }

    fn mode(&self) -> &'static str {
        if self.view.borrow().is_some() && self.wanted() == "3d" {
            "3d"
        } else {
            "2d"
        }
    }

    fn theme_colors(&self) {
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_colors(
                crate::theme::rgba(&self.app, "--stage"),
                crate::theme::rgba(&self.app, "--stage-focus"),
            );
        }
    }

    /// Skyver gjeldende farger, lysdioder, bakgrunnslys, bevegelse og viftefart
    /// inn i 3D-visningen når modellen er klar (første ramme etter bygging).
    fn sync_3d_state(&self) {
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_colors(
                crate::theme::rgba(&self.app, "--stage"),
                crate::theme::rgba(&self.app, "--stage-focus"),
            );
            cv.set_motion(self.motion());
            cv.set_ambient_enabled(self.ambient_on.get());
            if let Some(f) = self.app.gpu_sample().and_then(|s| s.fan) {
                cv.show_fan_speed(f);
            }
            cv.set_lights(&self.engine.borrow());
        }
    }

    fn render_mode(&self) {
        let mode = self.mode();
        self.ui.mode_seg.set(mode);
        let three = mode == "3d";
        class(&self.ui.root, "mode-2d", !three);
        for w in [
            self.ui.caption.clone().upcast::<gtk::Widget>(),
            self.ui.keys.clone().upcast(),
        ] {
            w.set_visible(three);
        }
        for w in [
            self.ui.angle_seg.root.clone().upcast::<gtk::Widget>(),
            self.ui.step_seg.root.clone().upcast(),
            self.ui.explode.clone().upcast(),
        ] {
            if let Some(child) = w.parent().and_then(|p| p.parent()) {
                child.set_visible(three);
            }
        }
        self.ui.parts_expander.set_visible(three);
        if let Some(b) = self.ui.mode_seg.button("3d") {
            let t = match self.gl_error.borrow().as_ref() {
                Some(e) => trf!("3D kan ikke tegnes her ({feil}). Klikk for å prøve igjen.", feil = e),
                None if self.slow_warned.get() => {
                    tr!("Grafikken tegnet 3D for tregt. Klikk for å prøve igjen.").into()
                }
                None => tr!("Kortet i 3D. Dra for å vri.").into(),
            };
            b.set_tooltip_text(Some(&t));
        }
        if let Some(b) = self.ui.mode_seg.button("2d") {
            b.set_tooltip_text(Some(tr!("Flatt kort sett forfra. Bruker minst ressurser.")));
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
        set_text(
            &self.ui.note,
            if slow {
                tr!("Stillbilde: grafikken tegner for tregt")
            } else {
                tr!("Animasjon av")
            },
        );
    }

    fn persist_view_soon(self: &Rc<Self>) {
        if let Some(t) = self.save_view.borrow_mut().take() {
            t.remove();
        }
        let w = Rc::downgrade(self);
        *self.save_view.borrow_mut() = Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(800),
            move || {
                if let Some(p) = w.upgrade() {
                    p.save_view.borrow_mut().take();
                    p.persist_view();
                }
            },
        ));
    }

    fn persist_view(&self) {
        let cv = self.view.borrow().clone();
        self.persist_view_now(cv.as_ref());
    }

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
        v["ambient"] = json!(self.ambient_on.get());
        locked::update(&self.app, "lys", |s| s.lys = v);
    }

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
        if crate::widgets::cairo_renderer() {
            let prev = self.last_us.get();
            if prev > 0 && now_us - prev < 30_000 {
                return true;
            }
        }
        let last = self.last_us.replace(now_us);
        let dt = if last > 0 {
            ((now_us - last) as f64 / 1e6).min(0.1)
        } else {
            1.0 / 60.0
        };
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
                busy |= cv.advance();
                if cv.slow() && !self.slow_warned.get() {
                    self.slow_warned.set(true);
                    self.app.toast(
                        crate::toast::Toast::new(tr!("Lys vises i 2D")).body(tr!(
                            "Grafikken tegnet 3D-kortet for tregt. Velg 3D over visningen for å prøve igjen."
                        )),
                    );
                    self.sync_notes();
                }
                if !cv.ready() {
                    busy = true;
                }
            }
        } else {
            if changed {
                self.ui.flat.set_lights(&self.engine.borrow(), self.ambient_on.get());
            }
            let fan = self.app.gpu_sample().and_then(|s| s.fan).unwrap_or(0.0);
            if full && fan > 0.0 {
                let prev = self.flat_draw.get();
                if prev == 0 || now_us - prev >= 30_000 {
                    self.flat_draw.set(now_us);
                    self.ui.flat.spin(dt as f32, fan);
                }
                busy = true;
            }
        }
        busy
    }

    fn toggle_pinned(self: &Rc<Self>, id: &'static str) {
        let mut pin = self.pinned.borrow_mut();
        *pin = if *pin == Some(id) { None } else { Some(id) };
        let cur = *pin;
        drop(pin);
        if let Some(cv) = self.view.borrow().as_ref() {
            cv.set_focus_group(cur);
        }
        self.render_parts();
    }

    fn render_parts(self: &Rc<Self>) {
        while let Some(c) = self.ui.parts_list.first_child() {
            self.ui.parts_list.remove(&c);
        }
        let Some(cv) = self.view.borrow().clone() else { return };
        let list = cv.visible_groups();
        let pinned = *self.pinned.borrow();
        for info in &list {
            let id = info.id;
            let b = gtk::ToggleButton::new();
            b.add_css_class("flat");
            b.add_css_class("part");
            b.set_active(pinned == Some(id));
            let row = hbox(8, &[]);
            let n = label(info.name, &["n"]);
            n.set_xalign(0.0);
            let d = label(info.desc, &["muted", "sm"]);
            d.set_xalign(0.0);
            d.set_hexpand(true);
            d.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&n);
            row.append(&d);
            b.set_child(Some(&row));

            let w = Rc::downgrade(&cv);
            let hover = gtk::EventControllerMotion::new();
            hover.connect_enter(move |_, _, _| {
                if let Some(cv) = w.upgrade() {
                    cv.set_focus_group(Some(id));
                }
            });
            let w2 = Rc::downgrade(&cv);
            let p = Rc::downgrade(self);
            hover.connect_leave(move |_| {
                if let (Some(cv), Some(p)) = (w2.upgrade(), p.upgrade()) {
                    cv.set_focus_group(*p.pinned.borrow());
                }
            });
            b.add_controller(hover);

            let p = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = p.upgrade() {
                    let p2 = p.clone();
                    glib::idle_add_local_once(move || p2.toggle_pinned(id));
                }
            });
            self.ui.parts_list.append(&b);
        }
    }

    fn describe_soon(self: &Rc<Self>) {
        if let Some(t) = self.describe_t.borrow_mut().take() {
            t.remove();
        }
        let w = Rc::downgrade(self);
        *self.describe_t.borrow_mut() = Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(120),
            move || {
                let Some(p) = w.upgrade() else { return };
                p.describe_t.borrow_mut().take();
                p.describe();
            },
        ));
    }

    fn describe(self: &Rc<Self>) {
        let v = self.visual.borrow().clone();
        let lys = if !v.bound {
            tr!("Kortlys er ikke aktivert.").to_string()
        } else if v.selected_mode == "off" {
            tr!("Lysene er av.").to_string()
        } else {
            trf!(
                "Lysstyrke {styrke}.",
                styrke = fmt::pct(Some(f64::from(v.selected_brightness)), 0)
            )
        };
        let cv = self.view.borrow().clone();
        let text = match (&cv, self.mode()) {
            (Some(cv), "3d") => {
                let step = cv.step();
                let ex = cv.explode_goal();
                let step_label = STEPS[step.min(STEPS.len() - 1)]();
                set_text(
                    &self.ui.caption,
                    &if ex > 0.02 {
                        trf!("{lag} · lagene skilt", lag = step_label)
                    } else {
                        step_label.to_string()
                    },
                );
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
                set_text(&self.ui.explode_out, &fmt::pct(Some(pct), 0));
                self.quiet.set(false);
                format!("{} {lys}", cv.describe())
            }
            _ => format!("{} {lys}", tr!("Flatt skjermkort sett forfra.")),
        };
        set_text(&self.ui.desc, &text);
        self.ui
            .overlay
            .update_property(&[gtk::accessible::Property::Description(&text)]);
    }

    fn show_tip(&self, info: Option<lys3d::PartInfo>, x: f32, y: f32) {
        let Some(i) = info else {
            self.hide_tip();
            return;
        };
        set_text(&self.ui.tip_name, i.name);
        set_text(&self.ui.tip_desc, i.desc);
        self.ui.tip_zone.set_visible(i.zone.is_some());
        set_text(&self.ui.tip_zone, tr!("Kortet har én fysisk lyssone."));
        let (w, h) = (self.ui.overlay.width() as f32, self.ui.overlay.height() as f32);
        let (tw, th) = (
            self.ui.tip_box.width().max(160) as f32,
            self.ui.tip_box.height().max(44) as f32,
        );
        let px = (x + 14.0).min(w - tw - 8.0).max(8.0);
        let py = if y + 18.0 + th > h - 8.0 {
            (y - th - 12.0).max(8.0)
        } else {
            y + 18.0
        };
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
