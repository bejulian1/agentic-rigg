//! Hodet på et grafkort: tittel, forklaring (serienavnene), «min · maks» og
//! halen (verdien nå, ekstraknapper og pause).
//!
//! Plassen deles etter `fit::head_fit`: halen står alltid, tittelen får
//! resten, forklaringen står etter tittelen når hele får plass og går ellers
//! ned på egen linje, og «min · maks» vises når hele får plass i det som er
//! igjen. Før var hodet en `GtkBox`, og plassen ble delt ut til valgfrie
//! deler som likevel ikke ble vist, så tittelen ble kuttet ved siden av et
//! tomrom, og forklaringen forsvant helt i smale kort.
//!
//! Kortene på samme side deler en [`HeadGroup`] og legges ut likt: «min ·
//! maks» vises på alle eller ingen (ellers avgjorde lengden på hver tittel),
//! og må én forklaring ned på egen linje, står alle der, så plottene i samme
//! rad blir like høye.

use crate::widgets::wrap::Wrap;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Mellom tittel, forklaring og hale (som `--sp-3`).
const GAP: i32 = 12;
/// Mellom «min · maks» og halen (som `--sp-2`).
const STATS_GAP: i32 = 8;
/// Mellom tittellinja og forklaringen under.
const ROW_GAP: i32 = 2;

/// Det et hode melder til gruppa etter hver utlegging.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Report {
    /// «min · maks» får plass på tittellinja sammen med forklaringen.
    stats_beside: bool,
    /// «min · maks» får plass når forklaringen står under tittelen.
    stats_alone: bool,
    /// Forklaringen får ikke plass etter tittelen.
    below: bool,
    /// Høyden på forklaringen når den står under tittelen (0 uten forklaring).
    legend_h: i32,
}

/// Det som gjelder for alle hodene i gruppa.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shared {
    /// «min · maks» får plass på alle.
    stats: bool,
    /// Minst én forklaring må stå under tittelen: da gjør alle det.
    below: bool,
    /// Høyden på linja under tittelen (den høyeste forklaringen).
    below_h: i32,
}

/// Kortene på samme side. De legges ut likt: «min · maks» vises på alle
/// eller ingen, og må én forklaring ned på egen linje, står alle der og alle
/// hodene er like høye. Ellers ble plottene i samme rad ulike høye.
pub struct HeadGroup {
    heads: RefCell<Vec<(glib::WeakRef<ChartHead>, Report)>>,
    shared: Cell<Shared>,
}

impl HeadGroup {
    pub fn new() -> Rc<HeadGroup> {
        Rc::new(HeadGroup { heads: RefCell::new(vec![]), shared: Cell::new(Shared { stats: true, below: false, below_h: 0 }) })
    }

    fn shared(&self) -> Shared {
        self.shared.get()
    }

    /// Hodet melder hva som får plass hos seg (med bredden det faktisk
    /// fikk). Endrer det felles svaret seg, legges alle hodene ut på nytt.
    fn report(self: &Rc<Self>, head: &ChartHead, report: Report) {
        let next = {
            let mut heads = self.heads.borrow_mut();
            heads.retain(|(w, _)| w.upgrade().is_some());
            match heads.iter_mut().find(|(w, _)| w.upgrade().as_ref() == Some(head)) {
                Some(entry) => entry.1 = report,
                None => heads.push((head.downgrade(), report)),
            }
            // Bare kortene som vises nå, teller.
            let shown = || heads.iter().filter(|(w, _)| w.upgrade().is_some_and(|h| h.is_mapped())).map(|(_, r)| *r);
            let below = shown().any(|r| r.below);
            // Tallene måles i det oppsettet alle kortene faktisk får.
            let stats = shown().all(|r| if below { r.stats_alone } else { r.stats_beside });
            Shared { stats, below, below_h: if below { shown().map(|r| r.legend_h).max().unwrap_or(0) } else { 0 } }
        };
        if self.shared.replace(next) != next {
            let me = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                let Some(g) = me.upgrade() else { return };
                let all: Vec<ChartHead> = g.heads.borrow().iter().filter_map(|(w, _)| w.upgrade()).collect();
                for h in all {
                    h.queue_resize();
                }
            });
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ChartHead {
        pub title: RefCell<Option<gtk::Label>>,
        pub legend: RefCell<Option<Wrap>>,
        pub stats: RefCell<Option<gtk::Label>>,
        pub tail: RefCell<Option<gtk::Box>>,
        pub group: RefCell<Option<Rc<HeadGroup>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChartHead {
        const NAME: &'static str = "RiggChartHead";
        type Type = super::ChartHead;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("charthead");
        }
    }

    impl ObjectImpl for ChartHead {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    /// Målene layouten regner med.
    pub struct Sizes {
        pub title: (i32, i32),
        pub tail_w: i32,
        pub legend_w: Option<i32>,
        pub stats_w: Option<i32>,
    }

    impl ChartHead {
        fn nat(w: &impl IsA<gtk::Widget>) -> (i32, i32) {
            let (min, nat, _, _) = w.measure(gtk::Orientation::Horizontal, -1);
            (min, nat)
        }

        pub fn sizes(&self) -> Sizes {
            let title = self.title.borrow().as_ref().map(Self::nat).unwrap_or((0, 0));
            let tail_w = self.tail.borrow().as_ref().filter(|t| t.is_visible()).map(|t| Self::nat(t).1).unwrap_or(0);
            let legend_w = self.legend.borrow().as_ref().filter(|l| l.is_visible() && l.first_child().is_some()).map(|l| l.line_width());
            // Skjult etikett eller tom tekst: ingen «min · maks» å plassere.
            let stats_w = self.stats.borrow().as_ref().filter(|s| s.is_visible() && !s.text().is_empty()).map(|s| Self::nat(s).1);
            Sizes { title, tail_w, legend_w, stats_w }
        }

        /// Det felles svaret fra gruppa, eller hodets eget når det står alene.
        fn shared(&self) -> Option<Shared> {
            self.group.borrow().as_ref().map(|g| g.shared())
        }

        /// Plasseringen for en bredde: (tittellinja, om forklaringen står under).
        pub fn fit(&self, width: i32, s: &Sizes) -> (crate::widgets::fit::HeadFit, bool) {
            let own = crate::widgets::fit::head_fit(width, GAP, s.title, s.tail_w, s.legend_w, s.stats_w, STATS_GAP);
            let below = s.legend_w.is_some() && (!own.legend_inline || self.shared().is_some_and(|g| g.below));
            if below && own.legend_inline {
                // Forklaringen står under fordi et annet kort trenger det:
                // tittellinja legges ut uten den.
                return (crate::widgets::fit::head_fit(width, GAP, s.title, s.tail_w, None, s.stats_w, STATS_GAP), true);
            }
            (own, below)
        }

        /// Høyden på linja under tittelen: forklaringen, eller like mye tomrom
        /// som de andre kortene bruker på sin.
        fn below_height(&self, width: i32, below: bool) -> i32 {
            let own = if below { self.legend.borrow().as_ref().map(|l| Self::height(l, width)).unwrap_or(0) } else { 0 };
            own.max(self.shared().map(|g| g.below_h).unwrap_or(0))
        }

        fn height(w: &impl IsA<gtk::Widget>, width: i32) -> i32 {
            w.measure(gtk::Orientation::Vertical, width).1
        }

        /// Høyden på tittellinja.
        fn row_height(&self, s: &Sizes, fit: &crate::widgets::fit::HeadFit) -> i32 {
            let mut h = 0;
            if let Some(t) = self.title.borrow().as_ref() {
                h = h.max(Self::height(t, fit.title_w.max(1)));
            }
            if let Some(t) = self.tail.borrow().as_ref().filter(|t| t.is_visible()) {
                h = h.max(Self::height(t, -1));
            }
            // Alltid med, så linja ikke endrer høyde når «min · maks» kommer og går.
            if let Some(st) = self.stats.borrow().as_ref().filter(|st| st.is_visible()) {
                h = h.max(Self::height(st, -1));
            }
            if fit.legend_inline
                && let Some(l) = self.legend.borrow().as_ref()
            {
                h = h.max(Self::height(l, s.legend_w.unwrap_or(-1)));
            }
            h
        }
    }

    impl WidgetImpl for ChartHead {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let s = self.sizes();
            let tail = if s.tail_w > 0 { s.tail_w + GAP } else { 0 };
            match o {
                gtk::Orientation::Horizontal => {
                    let legend_min = self.legend.borrow().as_ref().filter(|_| s.legend_w.is_some()).map(|l| Self::nat(l).0).unwrap_or(0);
                    let min = (s.title.0 + tail).max(legend_min);
                    let nat = s.title.1 + tail + s.legend_w.map(|w| w + GAP).unwrap_or(0) + s.stats_w.map(|w| w + STATS_GAP).unwrap_or(0);
                    (min, nat.max(min), -1, -1)
                }
                _ => {
                    let width = if for_size > 0 { for_size } else { self.measure(gtk::Orientation::Horizontal, -1).1 };
                    let (fit, below) = self.fit(width, &s);
                    let mut h = self.row_height(&s, &fit);
                    let below_h = self.below_height(width, below);
                    if below_h > 0 {
                        h += ROW_GAP + below_h;
                    }
                    (h, h, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let s = self.sizes();
            // Meld først hva som får plass her, med bredden hodet faktisk fikk.
            let own = crate::widgets::fit::head_fit(width, GAP, s.title, s.tail_w, s.legend_w, s.stats_w, STATS_GAP);
            if let Some(g) = self.group.borrow().as_ref() {
                let legend_h = if s.legend_w.is_some() { self.legend.borrow().as_ref().map(|l| Self::height(l, width)).unwrap_or(0) } else { 0 };
                // Begge oppsettene meldes, så svaret ikke avhenger av hvor de
                // andre kortene har forklaringen sin akkurat nå.
                let none = s.stats_w.is_none();
                let stats_alone = none || crate::widgets::fit::head_fit(width, GAP, s.title, s.tail_w, None, s.stats_w, STATS_GAP).stats;
                g.report(&obj, Report { stats_beside: none || own.stats, stats_alone, below: s.legend_w.is_some() && !own.legend_inline, legend_h });
            }
            let (fit, below) = self.fit(width, &s);
            let legend_h = if below { self.legend.borrow().as_ref().map(|l| Self::height(l, width)).unwrap_or(0) } else { 0 };
            let below_h = self.below_height(width, below);
            // Tittellinja får det som er igjen av høyden, så innholdet står midt i den.
            let row_h = if below_h > 0 { (height - below_h - ROW_GAP).max(self.row_height(&s, &fit)) } else { height };
            if let Some(t) = self.title.borrow().as_ref() {
                t.size_allocate(&gtk::Allocation::new(0, 0, fit.title_w.max(1), row_h), -1);
            }
            let tail_x = width - s.tail_w;
            if let Some(t) = self.tail.borrow().as_ref().filter(|t| t.is_visible()) {
                t.size_allocate(&gtk::Allocation::new(tail_x, 0, s.tail_w, row_h), -1);
            }
            if let Some(l) = self.legend.borrow().as_ref().filter(|l| l.is_visible()) {
                if s.legend_w.is_none() {
                    l.size_allocate(&gtk::Allocation::new(0, 0, 1, 1), -1);
                } else if below {
                    l.size_allocate(&gtk::Allocation::new(0, row_h + ROW_GAP, width, legend_h), -1);
                } else {
                    let lw = s.legend_w.unwrap_or(0);
                    let lh = Self::height(l, lw).min(row_h);
                    l.size_allocate(&gtk::Allocation::new(fit.title_w + GAP, (row_h - lh) / 2, lw, lh), -1);
                }
            }
            if let Some(st) = self.stats.borrow().as_ref().filter(|st| st.is_visible()) {
                // Får plass her, og hos alle de andre kortene på siden.
                let show = fit.stats && self.shared().is_none_or(|g| g.stats);
                if st.is_child_visible() != show {
                    st.set_child_visible(show);
                }
                if show {
                    let sw = s.stats_w.unwrap_or(0);
                    let x = tail_x - if s.tail_w > 0 { STATS_GAP } else { 0 } - sw;
                    st.size_allocate(&gtk::Allocation::new(x, 0, sw, row_h), -1);
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct ChartHead(ObjectSubclass<imp::ChartHead>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ChartHead {
    /// `legend` kan være tom (én serie); `stats` kan være skjult.
    pub fn new(title: &gtk::Label, legend: &Wrap, stats: &gtk::Label, tail: &gtk::Box) -> ChartHead {
        let h: ChartHead = glib::Object::new();
        // Rekkefølgen er også rekkefølgen for Tab og skjermlesere.
        title.set_parent(&h);
        legend.set_parent(&h);
        stats.set_parent(&h);
        tail.set_parent(&h);
        let imp = h.imp();
        *imp.title.borrow_mut() = Some(title.clone());
        *imp.legend.borrow_mut() = Some(legend.clone());
        *imp.stats.borrow_mut() = Some(stats.clone());
        *imp.tail.borrow_mut() = Some(tail.clone());
        h
    }

    /// «min · maks» vises likt på alle hodene i gruppa.
    pub fn join(&self, group: &Rc<HeadGroup>) {
        *self.imp().group.borrow_mut() = Some(group.clone());
    }
}
