//! Måleren for temperatur og effekt på Oversikt (demoens `gauge.js`, v0.0.11):
//!
//! ```text
//!   ████████████░░░░░░░░░░░░│░░░░░░│░░
//!                          83     97  100
//!   Mål 83 °C · Nedklokking 97 °C · Avstenging 100 °C
//! ```
//!
//! - Fyllet går fra skalaens start til verdien nå. Fargen sier nivået
//!   (aksent normalt, gul og rød bare over brukerens grenser), og det samme
//!   står i ord ved verdien og i verktøytipset.
//! - Merkene er tynne streker med tallet rett under. Hva tallene betyr, står i
//!   forklaringen under, så måleren kan leses uten farger.
//! - Tallene krasjer aldri: står to for tett, viker skalaens slutt først, så
//!   merket lengst til høyre (`math::gauge_labels`). Forklaringen har alle.

use super::bar::{HasSmooth, Smooth};
use super::chart::{Colors, alpha, math};
use crate::state::App;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk, pango};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Et merke på skalaen: verdien, tallet under streken og navnet i forklaringen.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub v: f64,
    pub num: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Spec {
    pub min: f64,
    pub max: f64,
    pub value: Option<f64>,
    /// «», «warn» eller «danger».
    pub level: &'static str,
    pub marks: Vec<Mark>,
    /// Skalaens slutt (avstenging, kortets maks): (tall, navn).
    pub end: Option<(String, String)>,
    pub tip: String,
}

const BAR_H: f32 = 10.0;
const TICK_OUT: f32 = 3.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct GaugeBar {
        pub smooth: Smooth,
        pub spec: RefCell<Spec>,
        pub colors: RefCell<Option<Colors>>,
        pub app: RefCell<Weak<App>>,
        pub layouts: RefCell<Vec<(String, pango::Layout)>>,
        /// Hopp rett til ny verdi (Oversikt, se `Gauge::set_still`).
        pub still: std::cell::Cell<bool>,
        /// (min, maks, verdi) sist meldt til skjermleseren, avrundet.
        pub said: std::cell::Cell<Option<(i64, i64, i64)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GaugeBar {
        const NAME: &'static str = "RiggGaugeBar";
        type Type = super::GaugeBar;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("gaugebar");
            klass.set_accessible_role(gtk::AccessibleRole::Meter);
        }
    }

    impl ObjectImpl for GaugeBar {}

    impl WidgetImpl for GaugeBar {
        fn measure(&self, o: gtk::Orientation, _f: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (60, 200, -1, -1),
                _ => {
                    let nh = self.obj().create_pango_layout(Some("0")).pixel_size().1;
                    let h = (6.0 + TICK_OUT * 2.0 + BAR_H) as i32 + 3 + nh + 2;
                    (h, h, -1, -1)
                }
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
        }
    }
}

glib::wrapper! {
    /// Stolpen med merkene og tallene (forklaringen er en egen etikett i [`Gauge`]).
    pub struct GaugeBar(ObjectSubclass<imp::GaugeBar>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl HasSmooth for GaugeBar {
    fn smooth(&self) -> &Smooth {
        &self.imp().smooth
    }
}

impl GaugeBar {
    fn layout(&self, text: &str) -> pango::Layout {
        if let Some((_, l)) = self.imp().layouts.borrow().iter().find(|(t, _)| t == text) {
            return l.clone();
        }
        let l = self.create_pango_layout(Some(text));
        let mut v = self.imp().layouts.borrow_mut();
        if v.len() > 32 {
            v.clear();
        }
        v.push((text.to_string(), l.clone()));
        l
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let Some(c) = imp.colors.borrow().clone() else { return };
        let spec = imp.spec.borrow();
        let w = self.width() as f32;
        if w < 20.0 || spec.max <= spec.min {
            return;
        }
        let top = 6.0 + TICK_OUT;
        let r = BAR_H / 2.0;
        let pos = |v: f64| (((v - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)) as f32;
        // Sporet og fyllet.
        let track = gsk::RoundedRect::from_rect(graphene::Rect::new(0.0, top, w, BAR_H), r);
        let tp = gsk::PathBuilder::new();
        tp.add_rounded_rect(&track);
        s.append_fill(&tp.to_path(), gsk::FillRule::Winding, &c.track);
        if imp.smooth.has.get() {
            s.push_rounded_clip(&track);
            let fill = match spec.level {
                "danger" => c.danger,
                "warn" => c.warn,
                _ => c.accent,
            };
            let x = imp.smooth.value.get() as f32 * w - w;
            let p = gsk::PathBuilder::new();
            p.add_rounded_rect(&gsk::RoundedRect::from_rect(graphene::Rect::new(x, top, w, BAR_H), r));
            s.append_fill(&p.to_path(), gsk::FillRule::Winding, &fill);
            s.pop();
        }
        // Strekene ligger over stolpen.
        let tick = alpha(c.text, 0.7);
        for m in &spec.marks {
            let x = (pos(m.v) * w - 1.0).round().clamp(0.0, w - 2.0);
            s.append_color(&tick, &graphene::Rect::new(x, top - TICK_OUT, 2.0, BAR_H + TICK_OUT * 2.0));
        }
        // Tallene under, uten krasj.
        let ny = top + BAR_H + TICK_OUT + 3.0;
        let ls: Vec<pango::Layout> = spec.marks.iter().map(|m| self.layout(&m.num)).collect();
        let marks: Vec<(f32, f32)> = spec.marks.iter().zip(&ls).map(|(m, l)| (pos(m.v), l.pixel_size().0 as f32)).collect();
        let end_l = spec.end.as_ref().map(|e| self.layout(&e.0));
        let (xs, end_x) = math::gauge_labels(&marks, end_l.as_ref().map(|l| l.pixel_size().0 as f32), w);
        let put = |l: &pango::Layout, x: f32| {
            s.save();
            s.translate(&graphene::Point::new(x.round(), ny.round()));
            s.append_layout(l, &c.axis);
            s.restore();
        };
        for (l, x) in ls.iter().zip(&xs) {
            if let Some(x) = x {
                put(l, *x);
            }
        }
        if let (Some(l), Some(x)) = (&end_l, end_x) {
            put(l, x);
        }
    }
}

/// Måleren: stolpen med merker og tall, og forklaringen under.
#[derive(Clone)]
pub struct Gauge {
    pub root: gtk::Box,
    bar: GaugeBar,
    legend: gtk::Label,
}

impl Gauge {
    pub fn new(app: &Rc<App>, label: &str) -> Gauge {
        let root = super::vbox(0, &["gauge"]);
        root.update_property(&[gtk::accessible::Property::Label(label)]);
        let bar: GaugeBar = glib::Object::new();
        *bar.imp().app.borrow_mut() = Rc::downgrade(app);
        *bar.imp().colors.borrow_mut() = Some(Colors::from_app(app));
        bar.set_hexpand(true);
        bar.update_property(&[gtk::accessible::Property::Label(label)]);
        root.append(&bar);
        let legend = super::label("", &["g-legend"]);
        legend.set_wrap(true);
        legend.set_wrap_mode(pango::WrapMode::WordChar);
        root.append(&legend);
        Gauge { root, bar, legend }
    }

    pub fn set_colors(&self, c: &Colors) {
        *self.bar.imp().colors.borrow_mut() = Some(c.clone());
        // Temaet kan også ha endret skriftstørrelsen.
        self.bar.imp().layouts.borrow_mut().clear();
        self.bar.queue_resize();
    }

    /// Hopp rett til ny verdi i stedet for å gli. På Oversikt endrer effekten
    /// seg mer enn 3 % nesten hvert sekund under last, og glidingen ga da opptil
    /// 20 bilder i sekundet (runde 8A).
    pub fn set_still(&self, on: bool) {
        self.bar.imp().still.set(on);
    }

    /// Verdien kan ikke leses nå (kortet svarer ikke): tom måler, merkene står.
    pub fn clear(&self) {
        self.bar.imp().smooth.set(&self.bar, 0.0, false);
    }

    pub fn set(&self, spec: Spec) {
        let imp = self.bar.imp();
        if let Some(v) = spec.value.filter(|v| v.is_finite()) {
            let animate = !imp.still.get() && imp.app.borrow().upgrade().is_some_and(|a| crate::motion::level(&a) == crate::motion::Level::Full);
            imp.smooth.set(&self.bar, (v - spec.min) / (spec.max - spec.min).max(1e-9), animate);
            let said = (spec.min.round() as i64, spec.max.round() as i64, v.round() as i64);
            if imp.said.replace(Some(said)) != Some(said) {
                self.bar.update_property(&[gtk::accessible::Property::ValueMin(spec.min), gtk::accessible::Property::ValueMax(spec.max), gtk::accessible::Property::ValueNow(v.round())]);
            }
        }
        if self.root.tooltip_text().as_deref() != Some(spec.tip.as_str()) {
            self.root.set_tooltip_text(Some(&spec.tip));
        }
        if *imp.spec.borrow() == spec {
            return;
        }
        // Forklaringen: alle navnene, i stigende rekkefølge.
        let mut items: Vec<(f64, String)> = spec.marks.iter().map(|m| (m.v, format!("{} {}", m.name, m.num))).collect();
        if let Some((num, name)) = &spec.end {
            items.push((spec.max, format!("{name} {num}")));
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        super::set_text(&self.legend, &items.into_iter().map(|i| i.1).collect::<Vec<_>>().join(" · "));
        *imp.spec.borrow_mut() = spec;
        self.bar.queue_draw();
    }
}

/// Fargen på teksten ved verdien (samme som fyllet).
pub fn level_class(level: &str) -> Option<&'static str> {
    match level {
        "danger" => Some("lvl-danger"),
        "warn" => Some("lvl-warn"),
        _ => None,
    }
}
