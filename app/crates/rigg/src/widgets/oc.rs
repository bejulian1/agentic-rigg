//! Overklokkingsflaten på Kontroller og Viftekurve (runde 6C):
//!
//! - [`Slider`]: glideren med område, standardmerke, «kortet nå» og snap.
//!   Tegnet selv, så lesevisning og utkast ser like ut og tallene under
//!   aldri overlapper.
//! - [`OcRow`]: én kontroll (tittel, (i), forklaring, nå-linje, glider,
//!   tallfelt, enhet og Tilbakestill).
//! - [`Stat`] og [`StatStrip`]: de store tallene øverst, med en liten
//!   [`Meter`] under.
//! - [`info_button`]: (i)-knappen med detaljer i en popover.
//!
//! Ingenting her tegner av seg selv: alt tegnes på nytt bare når en verdi
//! endres (≤ 2 % CPU i ro, runde 6C i `maalinger.md`).
//!
//! **Eies av 6C (Styring).**

use rigg_core::{tr, trf};
use crate::state::App;
use crate::widgets::{Icon, button, class, hbox, label, set_text, vbox};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Tallfelt for en temperatur som lagres i °C: viser og leser i enheten fra
/// Innstillinger (°F eller °C). Verdien i feltet er fortsatt °C, så
/// området, stegene og utkastet er uendret; bare teksten regnes om.
pub fn temp_spin(sp: &gtk::SpinButton) {
    use rigg_core::fmt as F;
    sp.connect_output(|s| {
        s.set_text(&temp_spin_text(s.value()));
        glib::Propagation::Stop
    });
    sp.connect_input(|s| {
        let t = s.text().replace([',', '\u{202F}', ' '], ".").replace('\u{2212}', "-");
        let t = t.trim_end_matches(|c: char| c.is_alphabetic() || c == '°' || c == '.');
        Some(t.parse::<f64>().map(F::temp_in).map_err(|_| ()))
    });
}

/// Teksten i et [`temp_spin`]-felt for `c` °C.
pub fn temp_spin_text(c: f64) -> String {
    format!("{:.0}", rigg_core::fmt::temp_out(Some(c)).unwrap_or(c))
}

/// Skriver teksten i et [`temp_spin`]-felt på nytt (enheten er byttet).
pub fn temp_spin_refresh(sp: &gtk::SpinButton) {
    if !sp.has_focus() {
        sp.set_text(&temp_spin_text(sp.value()));
    }
}

/// Verdi fra glideren: avrundet til nærmeste steg, trukket til standard når
/// den er innenfor `magnet` (i verdiens enhet), og innenfor `allow`.
pub fn snap(v: f64, step: f64, allow: (f64, f64), stock: Option<f64>, magnet: f64) -> f64 {
    let mut s = if step > 0.0 { (v / step).round() * step } else { v };
    if let Some(st) = stock
        && (v - st).abs() <= magnet
        && (allow.0..=allow.1).contains(&st)
    {
        s = st;
    }
    // −0 blir 0 (ellers «−0 MHz» i tallfeltet).
    s.clamp(allow.0, allow.1) + 0.0
}

/// Tegner en avrundet flate (sirkel når `r` er halve høyden).
fn round_rect(s: &gtk::Snapshot, r: graphene::Rect, radius: f32, c: &gdk::RGBA) {
    s.push_rounded_clip(&gsk::RoundedRect::from_rect(r, radius));
    s.append_color(c, &r);
    s.pop();
}

fn ring(s: &gtk::Snapshot, cx: f32, cy: f32, r: f32, w: f32, c: &gdk::RGBA) {
    let rr = gsk::RoundedRect::from_rect(graphene::Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r), r);
    s.append_border(&rr, &[w; 4], &[*c; 4]);
}

// ================================================================ glideren

/// Formaterer en verdi for merkene under glideren («245 W», «+300 MHz»).
pub type FmtFn = Rc<dyn Fn(f64) -> String>;

const PAD: f32 = 10.0;
const KNOB: f32 = 7.5;
const ROW: f32 = 24.0;

mod slider_imp {
    use super::*;

    pub struct Slider {
        pub lo: Cell<f64>,
        pub hi: Cell<f64>,
        pub allow: Cell<(f64, f64)>,
        pub step: Cell<f64>,
        pub big: Cell<f64>,
        pub stock: Cell<Option<f64>>,
        pub card: Cell<Option<f64>>,
        pub value: Cell<Option<f64>>,
        pub fmt: RefCell<FmtFn>,
        /// spor, fyll, merke, tekst, knott, kant, fokus, sone
        pub colors: RefCell<[gdk::RGBA; 8]>,
        pub on_change: crate::widgets::Slot<dyn Fn(f64)>,
        pub drag: Cell<bool>,
        pub hover: Cell<bool>,
    }

    impl Default for Slider {
        fn default() -> Self {
            Slider {
                lo: Cell::new(0.0),
                hi: Cell::new(100.0),
                allow: Cell::new((0.0, 100.0)),
                step: Cell::new(1.0),
                big: Cell::new(10.0),
                stock: Cell::new(None),
                card: Cell::new(None),
                value: Cell::new(None),
                fmt: RefCell::new(Rc::new(|v| format!("{v:.0}"))),
                colors: RefCell::new([gdk::RGBA::BLACK; 8]),
                on_change: RefCell::new(None),
                drag: Cell::new(false),
                hover: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Slider {
        const NAME: &'static str = "RiggOcSlider";
        type Type = super::Slider;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("ocslider");
            klass.set_accessible_role(gtk::AccessibleRole::Slider);
        }
    }

    impl ObjectImpl for Slider {}

    impl WidgetImpl for Slider {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (100, 320, -1, -1),
                _ => {
                    let h = ROW as i32 + 2 + self.obj().create_pango_layout(Some("0")).pixel_size().1;
                    (h, h, -1, -1)
                }
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let ww = w.width() as f32;
            let (lo, hi) = (self.lo.get(), self.hi.get());
            if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) || ww < 2.0 * PAD + 10.0 {
                return;
            }
            let [track, fill, tick, text, knob, edge, focus, zone] = *self.colors.borrow();
            let dim = |c: &gdk::RGBA| {
                let mut c = *c;
                if !w.is_sensitive() {
                    c.set_alpha(c.alpha() * 0.45);
                }
                c
            };
            let x = |v: f64| PAD + ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * (ww - 2.0 * PAD);
            let cy = (ROW / 2.0).round();
            let (a0, a1) = self.allow.get();
            // Sporet. Utenfor det tjenesten tillater: tynnere og blekere.
            let th = 4.0;
            if a0 > lo || a1 < hi {
                round_rect(s, graphene::Rect::new(PAD, cy - 1.0, ww - 2.0 * PAD, 2.0), 1.0, &dim(&zone));
            }
            round_rect(s, graphene::Rect::new(x(a0), cy - th / 2.0, (x(a1) - x(a0)).max(1.0), th), th / 2.0, &dim(&track));
            let value = self.value.get();
            let stock = self.stock.get();
            // Fyll fra standard (eller starten) til verdien.
            if let Some(v) = value {
                let from = stock.unwrap_or(a0);
                let (p, q) = if v >= from { (x(from), x(v)) } else { (x(v), x(from)) };
                if q - p >= 1.0 {
                    round_rect(s, graphene::Rect::new(p, cy - th / 2.0, q - p, th), th / 2.0, &dim(&fill));
                }
            }
            // Standardmerket.
            if let Some(st) = stock {
                s.append_color(&dim(&tick), &graphene::Rect::new(x(st).round() - 0.5, cy - 7.0, 1.0, 14.0));
            }
            // Det kortet har nå, når utkastet er noe annet: en hul ring.
            if let (Some(c), Some(v)) = (self.card.get(), value)
                && (c - v).abs() > 1e-6
            {
                ring(s, x(c), cy, 4.5, 1.5, &dim(&tick));
            }
            // Knotten.
            if let Some(v) = value {
                let cx = x(v);
                if w.state_flags().contains(gtk::StateFlags::FOCUS_VISIBLE) {
                    let mut f = focus;
                    f.set_alpha(0.55);
                    ring(s, cx, cy, KNOB + 3.5, 3.0, &f);
                }
                round_rect(s, graphene::Rect::new(cx - KNOB, cy - KNOB, 2.0 * KNOB, 2.0 * KNOB), KNOB, &dim(&knob));
                let e = if self.drag.get() || self.hover.get() { fill } else { edge };
                ring(s, cx, cy, KNOB, 1.0, &dim(&e));
            }
            // Tallene under: minste, standard og største. Standard vinner når
            // de ville overlappet; endene viker.
            let f = self.fmt.borrow().clone();
            let ty = ROW + 1.0;
            let lay = |t: &str| {
                let l = w.create_pango_layout(Some(t));
                let (lw, _) = l.pixel_size();
                (l, lw as f32)
            };
            let draw = |l: &gtk::pango::Layout, xx: f32, c: &gdk::RGBA| {
                s.save();
                s.translate(&graphene::Point::new(xx.round(), ty));
                s.append_layout(l, &dim(c));
                s.restore();
            };
            let (l_lo, w_lo) = lay(&f(lo));
            let (l_hi, w_hi) = lay(&f(hi));
            let gap = 10.0;
            let mut lo_span = Some((0.0f32, w_lo));
            let mut hi_span = Some((ww - w_hi, ww));
            if let Some(st) = stock {
                let (l_st, w_st) = lay(&trf!("standard {verdi}", verdi = f(st)));
                let sx = (x(st) - w_st / 2.0).clamp(0.0, (ww - w_st).max(0.0));
                if lo_span.is_some_and(|(_, b)| sx < b + gap) {
                    lo_span = None;
                }
                if hi_span.is_some_and(|(a, _)| sx + w_st > a - gap) {
                    hi_span = None;
                }
                draw(&l_st, sx, &text);
            }
            let mut faint = text;
            faint.set_alpha(faint.alpha() * 0.75);
            if let Some((a, _)) = lo_span {
                draw(&l_lo, a, &faint);
            }
            if let Some((a, _)) = hi_span {
                draw(&l_hi, a, &faint);
            }
        }
    }
}

glib::wrapper! {
    /// Glideren på Kontroller og Viftekurve. Klikk og dra, piltastene (Shift:
    /// større steg), PgUp/PgDn, Home/End. Snapper til steget og til standard.
    pub struct Slider(ObjectSubclass<slider_imp::Slider>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Slider {
    pub fn new(app: &Rc<App>) -> Slider {
        let s: Slider = glib::Object::new();
        s.set_hexpand(true);
        s.set_valign(gtk::Align::Center);
        s.set_focusable(true);
        s.set_focus_on_click(true);
        s.set_cursor_from_name(Some("pointer"));
        s.recolor(app);
        let w = s.downgrade();
        let a = app.clone();
        app.on_theme.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.recolor(&a);
            }
        });
        s.connect_state_flags_changed(|s, _| s.queue_draw());
        s.bind_input();
        s
    }

    fn recolor(&self, app: &App) {
        let c = |t: &str| crate::theme::rgba(app, t);
        let mut tick = c("--muted");
        tick.set_alpha(tick.alpha() * 0.8);
        *self.imp().colors.borrow_mut() = [c("--track"), c("--accent"), tick, c("--muted"), c("--knob"), c("--field-border"), c("--focus"), c("--border")];
        self.queue_draw();
    }

    fn x_to_value(&self, x: f64) -> f64 {
        let i = self.imp();
        let ww = self.width() as f64;
        let span = (ww - 2.0 * PAD as f64).max(1.0);
        i.lo.get() + ((x - PAD as f64) / span).clamp(0.0, 1.0) * (i.hi.get() - i.lo.get())
    }

    /// 6 px rundt standard trekker verdien dit.
    fn magnet(&self) -> f64 {
        let i = self.imp();
        let span = (self.width() as f64 - 2.0 * PAD as f64).max(1.0);
        6.0 / span * (i.hi.get() - i.lo.get())
    }

    fn user_set(&self, v: f64, magnet: f64) {
        let i = self.imp();
        let v = snap(v, i.step.get(), i.allow.get(), i.stock.get(), magnet);
        if i.value.get() == Some(v) {
            return;
        }
        i.value.set(Some(v));
        self.sync_a11y();
        self.queue_draw();
        if let Some(f) = i.on_change.borrow().as_ref() {
            f(v);
        }
    }

    fn bind_input(&self) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let w = self.downgrade();
        drag.connect_drag_begin(move |_, x, _| {
            let Some(s) = w.upgrade() else { return };
            if !s.is_sensitive() {
                return;
            }
            s.grab_focus();
            s.imp().drag.set(true);
            s.user_set(s.x_to_value(x), s.magnet());
        });
        let w = self.downgrade();
        drag.connect_drag_update(move |g, dx, _| {
            let Some(s) = w.upgrade() else { return };
            if let Some((x0, _)) = g.start_point()
                && s.imp().drag.get()
            {
                s.user_set(s.x_to_value(x0 + dx), s.magnet());
            }
        });
        let w = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            if let Some(s) = w.upgrade() {
                s.imp().drag.set(false);
                s.queue_draw();
            }
        });
        self.add_controller(drag);
        let motion = gtk::EventControllerMotion::new();
        for enter in [true, false] {
            let w = self.downgrade();
            let f = move || {
                if let Some(s) = w.upgrade() {
                    s.imp().hover.set(enter);
                    s.queue_draw();
                }
            };
            if enter {
                motion.connect_enter(move |_, _, _| f());
            } else {
                motion.connect_leave(move |_| f());
            }
        }
        self.add_controller(motion);
        let keys = gtk::EventControllerKey::new();
        let w = self.downgrade();
        keys.connect_key_pressed(move |_, k, _, m| {
            let Some(s) = w.upgrade() else { return glib::Propagation::Proceed };
            let i = s.imp();
            if m.contains(gdk::ModifierType::CONTROL_MASK) || m.contains(gdk::ModifierType::ALT_MASK) {
                return glib::Propagation::Proceed;
            }
            let (a0, a1) = i.allow.get();
            let cur = i.value.get().or(i.card.get()).or(i.stock.get()).unwrap_or(a0);
            let st = if m.contains(gdk::ModifierType::SHIFT_MASK) { i.big.get() } else { i.step.get() };
            let next = match k {
                gdk::Key::Left | gdk::Key::Down | gdk::Key::KP_Left | gdk::Key::KP_Down => cur - st,
                gdk::Key::Right | gdk::Key::Up | gdk::Key::KP_Right | gdk::Key::KP_Up => cur + st,
                gdk::Key::Page_Up => cur + i.big.get(),
                gdk::Key::Page_Down => cur - i.big.get(),
                gdk::Key::Home => a0,
                gdk::Key::End => a1,
                _ => return glib::Propagation::Proceed,
            };
            s.user_set(next, 0.0);
            glib::Propagation::Stop
        });
        self.add_controller(keys);
    }

    fn sync_a11y(&self) {
        let i = self.imp();
        let (a0, a1) = i.allow.get();
        if let Some(v) = i.value.get() {
            let t = (i.fmt.borrow())(v);
            self.update_property(&[gtk::accessible::Property::ValueMin(a0), gtk::accessible::Property::ValueMax(a1), gtk::accessible::Property::ValueNow(v), gtk::accessible::Property::ValueText(&t)]);
        }
    }

    /// Området som tegnes (`lo`–`hi`, det kortet melder) og det verdien kan ha (`allow`).
    pub fn set_range(&self, lo: f64, hi: f64, allow: (f64, f64)) {
        let i = self.imp();
        if (i.lo.get(), i.hi.get(), i.allow.get()) != (lo, hi, allow) {
            i.lo.set(lo);
            i.hi.set(hi);
            i.allow.set(allow);
            self.queue_draw();
        }
    }

    /// Steg for piltastene og snap, og stort steg (Shift, PgUp/PgDn).
    pub fn set_steps(&self, step: f64, big: f64) {
        self.imp().step.set(step);
        self.imp().big.set(big);
    }

    /// Tekstformatet for merkene. Tegner ikke på nytt av seg selv (kalles
    /// når enheten endres, sammen med `queue_draw`).
    pub fn set_fmt(&self, f: impl Fn(f64) -> String + 'static) {
        *self.imp().fmt.borrow_mut() = Rc::new(f);
    }

    pub fn set_stock(&self, v: Option<f64>) {
        if self.imp().stock.replace(v) != v {
            self.queue_draw();
        }
    }

    /// Det kortet har nå (en hul ring når verdien er en annen).
    pub fn set_card(&self, v: Option<f64>) {
        if self.imp().card.replace(v) != v {
            self.queue_draw();
        }
    }

    /// Setter verdien uten å si fra (fra kortet eller utkastet).
    pub fn set_value(&self, v: Option<f64>) {
        if self.imp().drag.get() {
            return;
        }
        if self.imp().value.replace(v) != v {
            self.sync_a11y();
            self.queue_draw();
        }
    }

    pub fn value(&self) -> Option<f64> {
        self.imp().value.get()
    }

    pub fn connect_change(&self, f: impl Fn(f64) + 'static) {
        *self.imp().on_change.borrow_mut() = Some(Box::new(f));
    }
}

// ================================================================ måleren

mod meter_imp {
    use super::*;

    pub struct Meter {
        pub v: Cell<(Option<f64>, Option<f64>, u8)>,
        /// spor, aksent, varsel, fare, merke
        pub colors: RefCell<[gdk::RGBA; 5]>,
    }

    impl Default for Meter {
        fn default() -> Self {
            Meter { v: Cell::new((None, None, 0)), colors: RefCell::new([gdk::RGBA::BLACK; 5]) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Meter {
        const NAME: &'static str = "RiggOcMeter";
        type Type = super::Meter;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("ocmeter");
        }
    }

    impl ObjectImpl for Meter {}

    impl WidgetImpl for Meter {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (20, 80, -1, -1),
                _ => (6, 6, -1, -1),
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            let (frac, mark, tone) = self.v.get();
            let [track, acc, warn, danger, markc] = *self.colors.borrow();
            let h = 3.0;
            let y = ((hh - h) / 2.0).round();
            round_rect(s, graphene::Rect::new(0.0, y, ww, h), h / 2.0, &track);
            if let Some(f) = frac {
                let c = match tone {
                    1 => warn,
                    2 => danger,
                    _ => acc,
                };
                let fw = (f.clamp(0.0, 1.0) as f32 * ww).max(if f > 0.0 { h } else { 0.0 });
                if fw > 0.0 {
                    round_rect(s, graphene::Rect::new(0.0, y, fw, h), h / 2.0, &c);
                }
            }
            if let Some(m) = mark {
                let mx = (m.clamp(0.0, 1.0) as f32 * ww).round().clamp(1.0, ww - 1.0);
                s.append_color(&markc, &graphene::Rect::new(mx - 0.5, 0.0, 1.0, hh));
            }
        }
    }
}

glib::wrapper! {
    /// En tynn måler (andel av grensen) med valgfritt merke (målet).
    pub struct Meter(ObjectSubclass<meter_imp::Meter>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Meter {
    pub fn new(app: &Rc<App>) -> Meter {
        let m: Meter = glib::Object::new();
        m.set_hexpand(true);
        m.recolor(app);
        let w = m.downgrade();
        let a = app.clone();
        app.on_theme.connect(move |_| {
            if let Some(m) = w.upgrade() {
                m.recolor(&a);
            }
        });
        m
    }

    fn recolor(&self, app: &App) {
        let c = |t: &str| crate::theme::rgba(app, t);
        *self.imp().colors.borrow_mut() = [c("--track"), c("--accent"), c("--warn"), c("--danger"), c("--muted")];
        self.queue_draw();
    }

    /// `frac` 0–1, `mark` 0–1, `tone` 0 aksent, 1 varsel, 2 fare.
    pub fn set(&self, frac: Option<f64>, mark: Option<f64>, tone: u8) {
        // Hele piksler: ingen ny tegning for støy under en piksel.
        let q = |v: Option<f64>| v.map(|v| (v.clamp(0.0, 1.0) * 400.0).round() / 400.0);
        let next = (q(frac), q(mark), tone);
        if self.imp().v.replace(next) != next {
            self.queue_draw();
        }
    }
}

// ================================================================ tallflisene

/// Én flis øverst: navn, stort tall med enhet, en linje under og en måler.
pub struct Stat {
    pub root: gtk::Box,
    pub value: gtk::Label,
    pub unit: gtk::Label,
    pub sub: gtk::Label,
    pub meter: Meter,
}

impl Stat {
    pub fn new(app: &Rc<App>, caption: &str) -> Rc<Stat> {
        let root = vbox(1, &["oc-stat"]);
        let cap = label(caption, &["oc-cap"]);
        cap.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let row = hbox(4, &["oc-num-row"]);
        let value = label("–", &["oc-big", "num"]);
        let unit = label("", &["oc-unit"]);
        unit.set_valign(gtk::Align::End);
        value.set_valign(gtk::Align::End);
        row.append(&value);
        row.append(&unit);
        let sub = label("", &["oc-sub", "num"]);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let meter = Meter::new(app);
        meter.add_css_class("oc-meter");
        root.append(&cap);
        root.append(&row);
        root.append(&sub);
        root.append(&meter);
        Rc::new(Stat { root, value, unit, sub, meter })
    }

    pub fn set(&self, value: &str, unit: &str, sub: &str) {
        set_text(&self.value, value);
        set_text(&self.unit, unit);
        set_text(&self.sub, sub);
    }

    /// «warn» eller «danger» på tallet, eller ingenting.
    pub fn set_tone(&self, tone: &str) {
        for t in ["warn", "danger", "accent"] {
            class(&self.value, t, t == tone);
        }
    }

    pub fn set_tip(&self, t: &str) {
        if self.root.tooltip_text().as_deref() != Some(t) {
            self.root.set_tooltip_text(if t.is_empty() { None } else { Some(t) });
        }
    }
}

/// Raden med fliser: like brede, og to til åtte per linje etter bredden.
#[derive(Clone)]
pub struct StatStrip {
    pub root: gtk::FlowBox,
}

impl StatStrip {
    pub fn new(max_per_line: u32) -> StatStrip {
        let root = gtk::FlowBox::new();
        root.add_css_class("oc-strip");
        root.set_selection_mode(gtk::SelectionMode::None);
        root.set_homogeneous(true);
        root.set_min_children_per_line(2);
        root.set_max_children_per_line(max_per_line);
        root.set_column_spacing(0);
        root.set_row_spacing(0);
        root.set_focusable(false);
        root.set_accessible_role(gtk::AccessibleRole::Group);
        StatStrip { root }
    }

    /// Like mange fliser på hver rad: så mange per rad som får plass, men
    /// bare et tall som går opp i antallet (6 fliser: 6, 3 eller 2 per rad),
    /// så ingen flis blir stående alene på siste rad. Kalles når siden
    /// oppdateres; gjør ingenting før flisene har fått bredde.
    pub fn balance(&self, max: u32) {
        let mut n = 0u32;
        // Den naturlige bredden: flisene kan krympe (tekst med «…»), men
        // FlowBox legger dem ut etter det de vil ha. Flisene er like brede,
        // så den bredeste avgjør. Før ble bare den første målt: med en lengre
        // tittel lenger ut («Manuelle Mindestdrehzahl») ble det 5 + 1.
        let mut min_w = 1;
        let mut c = self.root.first_child();
        while let Some(ch) = c {
            n += 1;
            min_w = min_w.max(ch.measure(gtk::Orientation::Horizontal, -1).1);
            c = ch.next_sibling();
        }
        let w = self.root.width();
        if w <= 0 || n == 0 {
            return;
        }
        let fits = ((w / min_w).max(1) as u32).min(max);
        let low = self.root.min_children_per_line();
        let per = (1..=n).rev().find(|d| n.is_multiple_of(*d) && *d <= fits).unwrap_or(1).max(low);
        if self.root.max_children_per_line() != per {
            self.root.set_max_children_per_line(per);
        }
    }

    /// Holder radene like når vinduet endrer bredde (ellers ved neste
    /// oppdatering, innen ett sekund).
    pub fn balance_on_resize(&self, max: u32) {
        self.root.connect_realize(move |root| {
            if let Some(win) = root.root().and_downcast::<gtk::Window>() {
                // Svake referanser: vinduet skal ikke holde raden i live.
                let weak = root.downgrade();
                win.connect_default_width_notify(move |_| {
                    let weak = weak.clone();
                    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || {
                        if let Some(root) = weak.upgrade() {
                            StatStrip { root }.balance(max);
                        }
                    });
                });
            }
        });
    }

    pub fn add(&self, stat: &Stat) {
        self.root.insert(&stat.root, -1);
        if let Some(c) = stat.root.parent() {
            // Flisene er ikke stopp for Tab (tallene leses, de trykkes ikke på).
            c.set_focusable(false);
            c.set_can_focus(false);
        }
    }
}

// ================================================================ (i)

/// Innholdet i (i)-popoveren: tittel, avsnitt og fakta (navn, verdi).
pub fn info_content(title: &str, paras: &[String], facts: &[(String, String)]) -> gtk::Box {
    let bx = vbox(6, &["oc-info"]);
    bx.append(&label(title, &["pop-title"]));
    for p in paras {
        let l = label(p, &["oc-info-p"]);
        l.set_wrap(true);
        l.set_max_width_chars(52);
        l.set_width_chars(40);
        bx.append(&l);
    }
    if !facts.is_empty() {
        let g = gtk::Grid::new();
        g.add_css_class("oc-facts");
        g.set_column_spacing(16);
        g.set_row_spacing(2);
        for (i, (k, v)) in facts.iter().enumerate() {
            let kl = label(k, &["muted", "sm"]);
            kl.set_valign(gtk::Align::Start);
            let vl = label(v, &["sm", "num"]);
            vl.set_wrap(true);
            vl.set_max_width_chars(34);
            g.attach(&kl, 0, i as i32, 1, 1);
            g.attach(&vl, 1, i as i32, 1, 1);
        }
        bx.append(&g);
    }
    bx
}

/// (i)-knappen. `make` lager innholdet når den trykkes (tallene er ferske).
pub fn info_button(what: &str, make: impl Fn() -> gtk::Box + 'static) -> gtk::Button {
    let b = button(Some("info"), None, &["flat", "sm", "oc-i"]);
    b.set_valign(gtk::Align::Center);
    crate::widgets::tip(&b, &trf!("Mer om {what}", what = what), None);
    b.update_property(&[gtk::accessible::Property::Label(&trf!("Mer om {what}", what = what))]);
    b.connect_clicked(move |b| {
        let p = crate::widgets::popover::popover(b, &make());
        p.add_css_class("pop-wide");
        p.popup();
    });
    b
}

// ================================================================ kontrollraden

/// Én kontroll på Kontroller: tittel med (i), forklaring og nå-linje til
/// venstre, glideren i midten, tallfelt og enhet, og Tilbakestill til høyre.
/// I smale vinduer står glideren under teksten.
pub struct OcRow {
    pub root: gtk::Grid,
    pub desc: gtk::Label,
    pub now: gtk::Label,
    pub slider: Slider,
    pub spin: gtk::SpinButton,
    pub unit: gtk::Label,
    pub value: gtk::Label,
    pub reset: gtk::Button,
    reset_text: gtk::Label,
    /// Over glideren (viftemodus på Vifte-raden).
    pub top: gtk::Box,
    right_stack: gtk::Stack,
    signed: bool,
    text: gtk::Box,
    pub mid: gtk::Box,
    right: gtk::Box,
    narrow: Cell<bool>,
    quiet: Rc<Cell<bool>>,
    on_change: Rc<crate::widgets::Slot<dyn Fn(f64)>>,
}

impl OcRow {
    /// `signed`: tallfeltet viser fortegn («+75»).
    pub fn new(app: &Rc<App>, title: &str, unit: &str, info: gtk::Button, signed: bool) -> Rc<OcRow> {
        let root = gtk::Grid::new();
        root.add_css_class("oc-row");
        root.set_column_spacing(16);
        let text = vbox(2, &["oc-text"]);
        text.set_valign(gtk::Align::Center);
        let head = hbox(2, &[]);
        let t = label(title, &["oc-title"]);
        head.append(&t);
        head.append(&info);
        let d = label("", &["oc-desc"]);
        d.set_wrap(true);
        d.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        // Ingen fast bredde: raden må kunne bli smal før den legger seg i to linjer,
        // ellers holder den vinduet bredt (640 px).
        d.set_max_width_chars(40);
        let now = label("", &["oc-now", "num"]);
        now.set_wrap(true);
        now.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        now.set_max_width_chars(40);
        text.append(&head);
        text.append(&d);
        text.append(&now);
        let mid = vbox(4, &["oc-mid"]);
        mid.set_valign(gtk::Align::Center);
        mid.set_hexpand(true);
        let top = hbox(8, &["oc-top"]);
        top.set_visible(false);
        let slider = Slider::new(app);
        slider.update_property(&[gtk::accessible::Property::Label(title)]);
        mid.append(&top);
        mid.append(&slider);
        let right = hbox(6, &["oc-right"]);
        right.set_valign(gtk::Align::Center);
        let spin = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
        spin.set_digits(0);
        spin.set_width_chars(6);
        spin.set_valign(gtk::Align::Center);
        spin.add_css_class("ctl-spin");
        spin.add_css_class("oc-spin");
        spin.update_property(&[gtk::accessible::Property::Label(title)]);
        if signed {
            spin.connect_output(|s| {
                let v = s.value().round() as i64;
                s.set_text(&if v > 0 { format!("+{v}") } else { v.to_string() });
                glib::Propagation::Stop
            });
        }
        let value = label("–", &["oc-value", "num"]);
        value.set_xalign(1.0);
        let right_stack = gtk::Stack::new();
        right_stack.set_hhomogeneous(true);
        right_stack.add_named(&spin, Some("spin"));
        right_stack.add_named(&value, Some("les"));
        let unit_l = label(unit, &["oc-unit-r"]);
        unit_l.set_width_chars(4);
        let reset = gtk::Button::new();
        reset.add_css_class("flat");
        reset.add_css_class("sm");
        reset.add_css_class("oc-reset");
        let rb = hbox(5, &[]);
        rb.append(&Icon::new("rotate-ccw", 14));
        let reset_text = gtk::Label::new(Some(tr!("Tilbakestill")));
        reset_text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        rb.append(&reset_text);
        reset.set_child(Some(&rb));
        reset.update_property(&[gtk::accessible::Property::Label(&trf!("Tilbakestill {verdi} til standard", verdi = title.to_lowercase()))]);
        right.append(&right_stack);
        right.append(&unit_l);
        right.append(&reset);
        root.attach(&text, 0, 0, 1, 1);
        root.attach(&mid, 1, 0, 1, 1);
        root.attach(&right, 2, 0, 1, 1);
        let row = Rc::new(OcRow {
            root,
            desc: d,
            now,
            slider,
            spin,
            unit: unit_l,
            value,
            reset,
            reset_text,
            top,
            right_stack,
            signed,
            text,
            mid,
            right,
            narrow: Cell::new(false),
            quiet: Rc::new(Cell::new(false)),
            on_change: Rc::new(RefCell::new(None)),
        });
        let (q, cb) = (row.quiet.clone(), row.on_change.clone());
        row.slider.connect_change(move |v| {
            if !q.get()
                && let Some(f) = cb.borrow().as_ref()
            {
                f(v);
            }
        });
        let (q, cb) = (row.quiet.clone(), row.on_change.clone());
        row.spin.connect_value_changed(move |s| {
            if !q.get()
                && let Some(f) = cb.borrow().as_ref()
            {
                f(s.value().round());
            }
        });
        row
    }

    /// Område (det kortet melder, og det verdien kan ha), steg og stort steg.
    pub fn set_range(&self, lo: f64, hi: f64, allow: (f64, f64), step: f64, big: f64) {
        self.slider.set_range(lo, hi, allow);
        self.slider.set_steps(step, big);
        let (a, b) = self.spin.range();
        if (a, b) != allow {
            self.quiet.set(true);
            self.spin.set_range(allow.0, allow.1);
            self.spin.set_increments(step, big);
            self.quiet.set(false);
        }
    }

    pub fn connect_change(&self, f: impl Fn(f64) + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_reset(&self, f: impl Fn() + 'static) {
        self.reset.connect_clicked(move |_| f());
    }

    /// Verdien som vises (utkastet, ellers kortet), det kortet har, og standard.
    /// `None` som verdi: «–» i stedet for tallfeltet (leses ikke).
    pub fn show(&self, value: Option<f64>, card: Option<f64>, stock: Option<f64>) {
        self.quiet.set(true);
        self.slider.set_stock(stock);
        self.slider.set_card(card);
        self.slider.set_value(value);
        match value {
            Some(v) if v < self.spin.range().0 || v > self.spin.range().1 => {
                // En faktisk verdi utenfor appens redigeringspolicy må ikke
                // bli klampet til en annen verdi av GtkSpinButton.
                let text = if matches!(self.unit.text().as_str(), "°C" | "°F") { temp_spin_text(v) } else { rigg_core::fmt::num(Some(v), self.spin.digits() as usize) };
                set_text(&self.value, &if self.signed && v > 0.0 { format!("+{text}") } else { text });
                self.right_stack.set_visible_child_name("les");
            }
            Some(v) => {
                if self.right_stack.visible_child_name().as_deref() != Some("spin") {
                    self.right_stack.set_visible_child_name("spin");
                }
                if self.spin.value() != v && !self.spin.has_focus() {
                    self.spin.set_value(v);
                }
            }
            None => { set_text(&self.value, "–"); self.right_stack.set_visible_child_name("les"); },
        }
        self.quiet.set(false);
        let at_stock = match (value, stock) {
            (Some(v), Some(s)) => (v - s).abs() < 1e-6,
            _ => true,
        };
        self.reset.set_sensitive(!at_stock);
    }

    pub fn set_reset_tip(&self, t: &str) {
        if self.reset.tooltip_text().as_deref() != Some(t) {
            self.reset.set_tooltip_text(Some(t));
        }
    }

    /// Utkastet er noe annet enn kortet: aksentkant og aksenttittel.
    pub fn set_changed(&self, on: bool) {
        class(&self.root, "changed", on);
    }

    /// Smalt vindu: teksten og tallfeltet på første linje, glideren under.
    pub fn set_narrow(&self, narrow: bool) {
        if self.narrow.replace(narrow) == narrow {
            return;
        }
        for w in [self.text.clone().upcast::<gtk::Widget>(), self.mid.clone().upcast(), self.right.clone().upcast()] {
            self.root.remove(&w);
        }
        if narrow {
            self.root.attach(&self.text, 0, 0, 1, 1);
            self.root.attach(&self.right, 1, 0, 1, 1);
            self.root.attach(&self.mid, 0, 1, 2, 1);
            self.text.set_hexpand(true);
            self.right.set_valign(gtk::Align::Start);
        } else {
            self.root.attach(&self.text, 0, 0, 1, 1);
            self.root.attach(&self.mid, 1, 0, 1, 1);
            self.root.attach(&self.right, 2, 0, 1, 1);
            self.text.set_hexpand(false);
            self.right.set_valign(gtk::Align::Center);
        }
        // Smalt: bare ikonet på Tilbakestill (teksten står i verktøytipset).
        self.reset_text.set_visible(!narrow);
        class(&self.root, "narrow", narrow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_til_steg_standard_og_omraade() {
        // Steget: 5 MHz.
        assert_eq!(snap(73.0, 5.0, (-200.0, 300.0), Some(0.0), 0.0), 75.0);
        // Innenfor magneten: standard.
        assert_eq!(snap(243.2, 1.0, (105.0, 275.0), Some(245.0), 2.0), 245.0);
        assert_eq!(snap(242.0, 1.0, (105.0, 275.0), Some(245.0), 2.0), 242.0);
        // Utenfor det tjenesten tillater: klemmes.
        assert_eq!(snap(640.0, 5.0, (-200.0, 300.0), Some(0.0), 0.0), 300.0);
        assert_eq!(snap(-999.0, 5.0, (-200.0, 300.0), Some(0.0), 0.0), -200.0);
        // Aldri «−0».
        assert!(snap(-1.0, 5.0, (-200.0, 300.0), None, 0.0).is_sign_positive());
        // Standard utenfor området trekker ikke.
        assert_eq!(snap(36.0, 1.0, (35.0, 100.0), Some(20.0), 50.0), 36.0);
    }
}
