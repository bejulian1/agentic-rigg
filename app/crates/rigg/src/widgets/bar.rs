//! Prosentstolper med varselfarge (demoens `meters.js`, `Bar`): sporet og
//! fyllet tegnes med `snapshot()`, fargene kommer fra CSS (`bar` og
//! `bar.warn`/`bar.danger` i `oversikt.css`).
//!
//! Fyllet glir mot ny verdi på ca. 0,2 s med full bevegelse (en
//! tick-callback som bare lever mens det glir), og hopper rett dit med
//! «Redusert» eller «Av». Første verdi settes direkte, så ingenting kryper
//! opp ved oppstart.

use crate::state::App;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Glatting mot et mål (delt av stolpen og måleren).
#[derive(Default)]
pub struct Smooth {
    pub value: Cell<f64>,
    pub target: Cell<f64>,
    pub has: Cell<bool>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    last: Cell<i64>,
}

/// En widget med glatting (stolpen og måleren).
pub trait HasSmooth: IsA<gtk::Widget> {
    fn smooth(&self) -> &Smooth;
}

impl Smooth {
    /// Nytt mål for widgeten `w` (som eier denne glattingen). `animate`: full bevegelse.
    pub fn set<W: HasSmooth>(&self, w: &W, target: f64, animate: bool) {
        let t = target.clamp(0.0, 1.0);
        self.target.set(t);
        // Små endringer (klokka som går 300 → 315 MHz hvert sekund) hopper
        // rett dit: å gli koster ti bilder i sekundet for nesten ingenting.
        let small = (t - self.value.get()).abs() < 0.03;
        if !self.has.get() || !animate || (small && self.tick.borrow().is_none()) {
            self.has.set(true);
            self.value.set(t);
            if let Some(id) = self.tick.take() {
                id.remove();
            }
            w.queue_draw();
            return;
        }
        if (self.value.get() - t).abs() < 0.0005 || self.tick.borrow().is_some() {
            return;
        }
        self.last.set(0);
        let id = w.add_tick_callback(|w, clock| {
            let s = w.smooth();
            let now = clock.frame_time();
            let dt = if s.last.get() == 0 { 0.016 } else { ((now - s.last.get()) as f64 / 1e6).min(0.1) };
            s.last.set(now);
            // Samme kurve som demoens `smooth(…, 0.06)`: ca. 0,2 s til målet.
            let k = 1.0 - 0.06f64.powf(dt / 0.2);
            let v = s.value.get() + (s.target.get() - s.value.get()) * k;
            let done = (v - s.target.get()).abs() < 0.0005;
            s.value.set(if done { s.target.get() } else { v });
            w.queue_draw();
            if done {
                s.tick.take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        *self.tick.borrow_mut() = Some(id);
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Bar {
        pub smooth: Smooth,
        pub app: RefCell<Weak<App>>,
        /// Glir mot ny verdi (standard). Av for stolper som skifter hvert
        /// sekund i store rutenett (trådene på System): 16 stolper som glir
        /// etter hverandre ga 24 bilder i sekundet hele tiden (runde 3C).
        pub still: Cell<bool>,
        /// Siste hele prosent meldt til skjermleseren (meldes bare når den endres).
        pub said: Cell<Option<i64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Bar {
        const NAME: &'static str = "RiggBar";
        type Type = super::Bar;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("bar");
            klass.set_accessible_role(gtk::AccessibleRole::Meter);
        }
    }

    impl ObjectImpl for Bar {}

    impl WidgetImpl for Bar {
        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            if ww < 2.0 || hh < 1.0 || !self.smooth.has.get() {
                return;
            }
            let r = hh / 2.0;
            let track = gsk::RoundedRect::from_rect(graphene::Rect::new(0.0, 0.0, ww, hh), r);
            s.push_rounded_clip(&track);
            // Fyllet skyves inn fra venstre, så den runde enden beholder formen.
            let x = (self.smooth.value.get() as f32 * ww) - ww;
            let p = gsk::PathBuilder::new();
            p.add_rounded_rect(&gsk::RoundedRect::from_rect(graphene::Rect::new(x, 0.0, ww, hh), r));
            s.append_fill(&p.to_path(), gsk::FillRule::Winding, &w.color());
            s.pop();
        }
    }
}

glib::wrapper! {
    pub struct Bar(ObjectSubclass<imp::Bar>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl HasSmooth for Bar {
    fn smooth(&self) -> &Smooth {
        &self.imp().smooth
    }
}

impl Bar {
    /// `thin`: 4 px i stedet for 8 px (klokkene).
    pub fn new(app: &Rc<App>, thin: bool) -> Bar {
        let b: Bar = glib::Object::new();
        *b.imp().app.borrow_mut() = Rc::downgrade(app);
        if thin {
            b.add_css_class("thin");
        }
        b.set_valign(gtk::Align::Center);
        b.set_hexpand(true);
        b.update_property(&[gtk::accessible::Property::ValueMin(0.0), gtk::accessible::Property::ValueMax(100.0)]);
        b
    }

    /// Hopp rett til ny verdi i stedet for å gli (se `still`).
    pub fn set_still(&self, on: bool) {
        self.imp().still.set(on);
    }

    /// Andel 0–1 og nivå («», «warn», «danger»). Fargen står også i ord i
    /// verktøytipset (v0.0.11). `None` (verdien kan ikke leses nå, kortet
    /// svarer ikke) tømmer stolpen, så den ikke viser et gammelt tall.
    pub fn set(&self, frac: Option<f64>, level: &str) {
        let Some(f) = frac.filter(|f| f.is_finite()) else {
            self.imp().smooth.set(self, 0.0, false);
            for l in ["warn", "danger"] {
                super::class(self, l, false);
            }
            return;
        };
        let animate = !self.imp().still.get() && self.imp().app.borrow().upgrade().is_some_and(|a| crate::motion::level(&a) == crate::motion::Level::Full);
        self.imp().smooth.set(self, f, animate);
        // Skjermleseren får hele prosent, og bare når den endres (hver melding
        // går over tilgjengelighetsbussen; runde 8A).
        let pct = (f * 100.0).round() as i64;
        if self.imp().said.replace(Some(pct)) != Some(pct) {
            self.update_property(&[gtk::accessible::Property::ValueNow(pct as f64)]);
        }
        for l in ["warn", "danger"] {
            super::class(self, l, l == level);
        }
    }
}
