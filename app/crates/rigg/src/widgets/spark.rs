//! Sparklinjen (`RiggSpark`): siste minutt for én verdi, i seriens faste
//! farge (CSS-klassen `serie-temp` osv. på noden `spark`) og med gul eller
//! rød når verdien er over grensen. Tegnes bare når nye data kommer.
//!
//! Brukes av statusblokken og flisene. Grafagenten (2A) bruker den også på
//! Sensorer.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, Default)]
pub struct SparkOpts {
    /// Tidsvinduet (ms) som vises, med «nå» helt til høyre.
    pub span_ms: i64,
    /// Fast skala (prosent er alltid 0–100).
    pub fixed: Option<(f64, f64)>,
    /// Minste spenn, så små svingninger ikke ser dramatiske ut.
    pub min_span: f64,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Spark {
        /// Punkter som (x 0–1, y 0–1). `None` er et hull.
        pub pts: RefCell<Vec<Option<(f32, f32)>>>,
        pub width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spark {
        const NAME: &'static str = "RiggSpark";
        type Type = super::Spark;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("spark");
            klass.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Spark {}

    impl WidgetImpl for Spark {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (self.width.get(), self.width.get(), -1, -1),
                _ => (14, 14, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let pts = self.pts.borrow();
            if pts.iter().flatten().count() < 2 {
                return;
            }
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            let color = w.color();
            let fill = gdk::RGBA::new(color.red(), color.green(), color.blue(), color.alpha() * 0.16);
            let inset = 1.0;
            let y = |v: f32| inset + (1.0 - v) * (hh - 2.0 * inset);
            let mut runs: Vec<Vec<(f32, f32)>> = vec![Vec::new()];
            for p in pts.iter() {
                match p {
                    Some((px, py)) => runs.last_mut().unwrap().push((px * ww, y(*py))),
                    None => runs.push(Vec::new()),
                }
            }
            let paint = crate::widgets::Painter::new(snapshot, &gtk::graphene::Rect::new(-1.0, -1.0, ww + 2.0, hh + 2.0));
            for run in runs.iter().filter(|r| r.len() >= 2) {
                let first = (run[0].0, hh);
                let last = (run[run.len() - 1].0, hh);
                paint.fill(std::iter::once(first).chain(run.iter().copied()).chain(std::iter::once(last)), &fill);
            }
            for run in runs.iter().filter(|r| r.len() >= 2) {
                paint.stroke(run.iter().copied(), 1.25, true, None, &color);
            }
        }
    }
}

glib::wrapper! {
    pub struct Spark(ObjectSubclass<imp::Spark>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Spark {
    /// `serie`: «temp», «load», «power», «clock», «mem», «memctl» eller «limit».
    pub fn new(serie: &str, width: i32) -> Spark {
        let s: Spark = glib::Object::new();
        s.imp().width.set(width);
        s.add_css_class(&format!("serie-{serie}"));
        s.set_can_target(false);
        s
    }

    /// Nivået (gul/rød) gir linja advarselsfargen.
    pub fn set_level(&self, level: &str) {
        for l in ["warn", "danger"] {
            if l == level {
                self.add_css_class(l);
            } else {
                self.remove_css_class(l);
            }
        }
    }

    /// Nye punkter `(t ms, verdi)`. Et hull over 3 s (eller 3 × takten) brytes.
    pub fn set_points(&self, pts: &[(i64, f64)], now: i64, opts: SparkOpts) {
        let span = opts.span_ms.max(1000);
        let t0 = now - span;
        let vis: Vec<(i64, f64)> = pts.iter().copied().filter(|(t, v)| *t >= t0 - 2000 && v.is_finite()).collect();
        let Some((lo, hi)) = rigg_core::downsample::range(vis.iter().map(|p| p.1), opts.min_span, opts.fixed) else {
            self.imp().pts.borrow_mut().clear();
            self.queue_draw();
            return;
        };
        let mut out: Vec<Option<(f32, f32)>> = Vec::with_capacity(vis.len());
        let mut prev: Option<i64> = None;
        for (t, v) in vis {
            if let Some(p) = prev
                && t - p > 3_500
            {
                out.push(None);
            }
            prev = Some(t);
            let x = ((t - t0) as f64 / span as f64).clamp(0.0, 1.0) as f32;
            let yv = ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;
            out.push(Some((x, yv)));
        }
        *self.imp().pts.borrow_mut() = out;
        self.queue_draw();
    }
}
