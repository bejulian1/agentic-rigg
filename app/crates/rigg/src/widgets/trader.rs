//! Trådrutenettet på Prosessor (`RiggThreadGrid`, runde 6A): klokke og last
//! per tråd, gruppert etter CCX og kjerne, tegnet i én widget i stedet for
//! 48 etiketter og 16 stolper. Det koster ett bilde uten ny layout når tallene
//! endres, og ingenting når de står stille (`set_values` sammenligner det som
//! vises). Høyden følger bredden: fire kjerner per rad, eller to i smale vinduer.
//!
//! Utseendet er det samme som rutenettet før: fliser som deler kant, «Kjerne 0»
//! og høyeste klokke øverst, og én linje per tråd med stolpe, last og klokke.
//! Tekstene tegnes med pango (`append_layout`), stolper og kanter som fargede
//! flater, så det er like raskt med cairo og GL. Tilgjengelig navn og
//! beskrivelse med alle verdiene, og verktøytips per tråd.

use rigg_core::{tr, trf};
use crate::state::App;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene, gsk, pango};
use rigg_core::fmt as F;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// En CCX (eller «Alle kjerner»): navn, tekst etter navnet og kjernene med trådene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupDef {
    pub name: String,
    pub info: String,
    pub cores: Vec<(String, Vec<usize>)>,
}

/// Klokke (MHz) og last (%) for én tråd.
pub type ThreadValue = (Option<f64>, Option<f64>);
/// En tegnet flis: (x, y, bredde, høyde, trådene).
type Hit = (f32, f32, f32, f32, Vec<usize>);

/// Minste bredde på en flis (px ved 100 % skrift).
const MIN_COL: f32 = 170.0;
const HEAD_H: f32 = 26.0;
const LINE_H: f32 = 18.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 4.0;
const GROUP_GAP: f32 = 8.0;

/// Kolonner for en bredde: 4 eller 2 (aldri 3 og 1), høyst så mange kjerner som finnes.
pub fn columns(width: f32, cores: usize, scale: f32) -> usize {
    let fit = (width / (MIN_COL * scale)).floor().max(1.0) as usize;
    let c = if fit >= 4 { 4 } else if fit >= 2 { 2 } else { 1 };
    c.min(cores.max(1))
}

fn tile_h(threads: usize) -> f32 {
    PAD_Y * 2.0 + LINE_H * (1 + threads) as f32 + 2.0
}

/// Høyden på hele rutenettet for en bredde.
pub fn height(groups: &[GroupDef], width: f32, scale: f32) -> f32 {
    let mut h = 0.0;
    for (i, g) in groups.iter().enumerate() {
        if i > 0 {
            h += GROUP_GAP;
        }
        let cols = columns(width, g.cores.len(), scale);
        let per = g.cores.iter().map(|c| c.1.len()).max().unwrap_or(1);
        let rows = g.cores.len().div_ceil(cols);
        h += HEAD_H * scale + rows as f32 * tile_h(per) * scale + 1.0;
    }
    h.ceil()
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ThreadGrid {
        pub app: RefCell<Weak<App>>,
        pub groups: RefCell<Vec<GroupDef>>,
        /// (MHz, last %) per tråd.
        pub values: RefCell<Vec<ThreadValue>>,
        pub warn: Cell<f64>,
        /// Flisene som ble tegnet: (x, y, b, h, trådene), til verktøytipsene.
        pub hits: RefCell<Vec<Hit>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ThreadGrid {
        const NAME: &'static str = "RiggThreadGrid";
        type Type = super::ThreadGrid;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("threadgrid");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for ThreadGrid {}

    impl WidgetImpl for ThreadGrid {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let s = self.obj().scale_f();
            match o {
                gtk::Orientation::Horizontal => {
                    let n = self.groups.borrow().iter().map(|g| g.cores.len()).max().unwrap_or(1).min(4);
                    ((MIN_COL * s * n.min(2) as f32) as i32, (MIN_COL * s * n as f32) as i32, -1, -1)
                }
                _ => {
                    let w = if for_size > 0 { for_size as f32 } else { MIN_COL * s * 4.0 };
                    let h = height(&self.groups.borrow(), w, s) as i32;
                    (h, h, -1, -1)
                }
            }
        }

        fn snapshot(&self, sn: &gtk::Snapshot) {
            self.obj().draw(sn);
        }
    }
}

glib::wrapper! {
    pub struct ThreadGrid(ObjectSubclass<imp::ThreadGrid>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// Det som vises for én tråd (sammenlignes for å la være å tegne).
fn shown(v: ThreadValue) -> (String, String) {
    (F::mhz(v.0), F::pct(v.1, 0))
}

impl ThreadGrid {
    pub fn new(app: &Rc<App>) -> ThreadGrid {
        let g: ThreadGrid = glib::Object::new();
        *g.imp().app.borrow_mut() = Rc::downgrade(app);
        g.set_hexpand(true);
        g.set_has_tooltip(true);
        g.update_property(&[gtk::accessible::Property::Label(tr!("Klokke og last per tråd"))]);
        g.connect_query_tooltip(|w, x, y, _kb, tip| {
            let hits = w.imp().hits.borrow();
            let Some((_, ty, _, _, threads)) = hits.iter().find(|(hx, hy, hw, hh, _)| (x as f32) >= *hx && (x as f32) < hx + hw && (y as f32) >= *hy && (y as f32) < hy + hh) else { return false };
            let line = ((y as f32 - ty - PAD_Y) / (LINE_H * w.scale_f())).floor() as i64 - 1;
            let vals = w.imp().values.borrow();
            let text = match usize::try_from(line).ok().and_then(|l| threads.get(l)) {
                Some(&t) => {
                    let (m, l) = shown(vals.get(t).copied().unwrap_or_default());
                    trf!("Tråd {t}: {m}, {l} last\nKlokke er snittet fra APERF/MPERF (cpufreq).", t = t, m = m, l = l)
                }
                None => trf!("{threads} deler kjerne (SMT)", threads = threads.iter().map(|t| format!("T{t}")).collect::<Vec<_>>().join(tr!(" og "))),
            };
            tip.set_text(Some(&text));
            true
        });
        g
    }

    fn scale_f(&self) -> f32 {
        (crate::theme::font_scale() as f32).max(0.5)
    }

    /// Ny gruppering (når topologien er lest). Gir ny høyde.
    pub fn set_groups(&self, groups: Vec<GroupDef>) {
        if *self.imp().groups.borrow() == groups {
            return;
        }
        *self.imp().groups.borrow_mut() = groups;
        self.queue_resize();
        self.queue_draw();
    }

    /// Nye verdier. Tegnes bare når noe som vises, har endret seg.
    pub fn set_values(&self, values: Vec<ThreadValue>, warn: f64) {
        let changed = {
            let old = self.imp().values.borrow();
            old.len() != values.len() || self.imp().warn.get() != warn || old.iter().zip(&values).any(|(a, b)| shown(*a) != shown(*b))
        };
        if !changed {
            return;
        }
        self.imp().warn.set(warn);
        *self.imp().values.borrow_mut() = values;
        self.update_description();
        self.queue_draw();
    }

    /// Beskrivelsen til skjermlesere: alle trådene med klokke og last.
    fn update_description(&self) {
        let vals = self.imp().values.borrow();
        let mut parts = Vec::new();
        for g in self.imp().groups.borrow().iter() {
            let mut t = vec![g.name.clone()];
            for (name, threads) in &g.cores {
                let th: Vec<String> = threads.iter().map(|&i| {
                    let (m, l) = shown(vals.get(i).copied().unwrap_or_default());
                    format!("T{i} {m} {l}")
                }).collect();
                t.push(format!("{name}: {}", th.join(", ")));
            }
            parts.push(t.join(". "));
        }
        self.update_property(&[gtk::accessible::Property::Description(&parts.join(". "))]);
    }

    /// Snitt-klokke og snitt-last for trådene.
    fn averages(&self, threads: &[usize]) -> ThreadValue {
        let vals = self.imp().values.borrow();
        let avg = |f: &dyn Fn(&ThreadValue) -> Option<f64>| {
            let v: Vec<f64> = threads.iter().filter_map(|i| vals.get(*i).and_then(f)).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        (avg(&|x| x.0), avg(&|x| x.1))
    }

    fn layout(&self, text: &str, cap: bool, bold: bool) -> pango::Layout {
        let l = self.create_pango_layout(Some(text));
        let attrs = pango::AttrList::new();
        attrs.insert(pango::AttrFontFeatures::new("tnum"));
        if cap {
            attrs.insert(pango::AttrFloat::new_scale(11.0 / 12.0));
        }
        if bold {
            attrs.insert(pango::AttrInt::new_weight(pango::Weight::Semibold));
        }
        l.set_attributes(Some(&attrs));
        l
    }

    fn text(&self, sn: &gtk::Snapshot, l: &pango::Layout, x: f32, y: f32, h: f32, color: &gdk::RGBA) {
        let (_, th) = l.pixel_size();
        sn.save();
        sn.translate(&graphene::Point::new(x.round(), (y + (h - th as f32) / 2.0).round()));
        sn.append_layout(l, color);
        sn.restore();
    }

    /// Tekst høyrejustert mot `right`.
    fn text_r(&self, sn: &gtk::Snapshot, l: &pango::Layout, right: f32, y: f32, h: f32, color: &gdk::RGBA) {
        let w = l.pixel_size().0 as f32;
        self.text(sn, l, right - w, y, h, color);
    }

    fn draw(&self, sn: &gtk::Snapshot) {
        let Some(app) = self.imp().app.borrow().upgrade() else { return };
        let c = |t: &str| crate::theme::rgba(&app, t);
        let (muted, border, card, track, fill, warn_c) = (c("--muted"), c("--border-soft"), c("--card"), c("--track"), c("--accent-fill"), c("--warn-fill"));
        let text_c = self.color();
        let s = self.scale_f();
        let width = self.width() as f32;
        let warn = self.imp().warn.get();
        let groups = self.imp().groups.borrow().clone();
        let vals = self.imp().values.borrow().clone();
        let rect = |sn: &gtk::Snapshot, x: f32, y: f32, w: f32, h: f32, col: &gdk::RGBA| sn.append_color(col, &graphene::Rect::new(x, y, w, h));
        let mut hits = Vec::new();
        let mut y = 0.0;
        let mhz_w = self.layout("8\u{202F}888\u{202F}MHz", true, false).pixel_size().0 as f32;
        let pct_w = self.layout("100\u{202F}%", true, false).pixel_size().0 as f32;
        let t_w = self.layout("T88", true, false).pixel_size().0 as f32;
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                y += GROUP_GAP;
            }
            // Overskriften: navn, kjerner og L3, og snittet til høyre.
            let hh = HEAD_H * s;
            let name = self.layout(&g.name, false, true);
            self.text(sn, &name, 0.0, y, hh, &text_c);
            let nw = name.pixel_size().0 as f32;
            self.text(sn, &self.layout(&g.info, false, false), nw + 10.0, y, hh, &muted);
            let all: Vec<usize> = g.cores.iter().flat_map(|c| c.1.iter().copied()).collect();
            let (am, al) = self.averages(&all);
            self.text_r(sn, &self.layout(&trf!("snitt {snitt} · last {last}", snitt = F::mhz(am), last = F::pct(al, 0)), false, false), width, y, hh, &muted);
            y += hh;
            // Flisene: rutenett med delte kanter (topp og venstre på rutenettet,
            // høyre og bunn på hver flis).
            let cols = columns(width, g.cores.len(), s);
            let per = g.cores.iter().map(|c| c.1.len()).max().unwrap_or(1);
            let th = tile_h(per) * s;
            let rows = g.cores.len().div_ceil(cols);
            let cw = ((width - 1.0) / cols as f32).floor();
            let gw = cw * cols as f32 + 1.0;
            rect(sn, 0.0, y, gw, rows as f32 * th + 1.0, &card);
            rect(sn, 0.0, y, gw, 1.0, &border);
            rect(sn, 0.0, y, 1.0, rows as f32 * th + 1.0, &border);
            let lh = LINE_H * s;
            for (i, (cname, threads)) in g.cores.iter().enumerate() {
                let tx = 1.0 + (i % cols) as f32 * cw;
                let ty = y + 1.0 + (i / cols) as f32 * th;
                rect(sn, tx + cw - 1.0, ty, 1.0, th, &border);
                rect(sn, tx, ty + th - 1.0, cw, 1.0, &border);
                hits.push((tx, ty, cw, th, threads.clone()));
                let (x0, x1) = (tx + PAD_X, tx + cw - 1.0 - PAD_X);
                let mut ly = ty + PAD_Y;
                // Kjernen og høyeste klokke.
                let top = threads.iter().filter_map(|t| vals.get(*t).and_then(|v| v.0)).reduce(f64::max);
                self.text(sn, &self.layout(cname, false, false), x0, ly, lh, &muted);
                self.text_r(sn, &self.layout(&F::mhz(top), false, true), x1, ly, lh, &text_c);
                ly += lh;
                // Én linje per tråd: «T0 [stolpe] 41 % 3 975 MHz».
                for &t in threads {
                    let (m, l) = vals.get(t).copied().unwrap_or_default();
                    self.text(sn, &self.layout(&format!("T{t}"), true, false), x0, ly, lh, &muted);
                    let bx = x0 + t_w + 6.0;
                    let bw = (x1 - mhz_w - 6.0 - pct_w - 8.0 - bx).max(8.0);
                    let by = ly + (lh - 4.0 * s.min(1.5)) / 2.0;
                    let bh = (4.0 * s.min(1.5)).round();
                    let r = gsk::RoundedRect::from_rect(graphene::Rect::new(bx, by, bw, bh), bh / 2.0);
                    sn.push_rounded_clip(&r);
                    rect(sn, bx, by, bw, bh, &track);
                    if let Some(l) = l {
                        let col = if l >= warn { &warn_c } else { &fill };
                        rect(sn, bx, by, (bw * (l / 100.0).clamp(0.0, 1.0) as f32).round(), bh, col);
                    }
                    sn.pop();
                    self.text_r(sn, &self.layout(&F::pct(l, 0), true, false), x1 - mhz_w - 6.0, ly, lh, &muted);
                    self.text_r(sn, &self.layout(&F::mhz(m), true, false), x1, ly, lh, &text_c);
                    ly += lh;
                }
            }
            y += rows as f32 * th + 1.0;
        }
        *self.imp().hits.borrow_mut() = hits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kolonner_og_høyde() {
        assert_eq!(columns(1600.0, 4, 1.0), 4);
        assert_eq!(columns(560.0, 4, 1.0), 2, "aldri tre og én");
        assert_eq!(columns(300.0, 4, 1.0), 1);
        assert_eq!(columns(1600.0, 2, 1.0), 2, "høyst så mange kjerner som finnes");
        let g = GroupDef { name: "CCX 0".into(), info: String::new(), cores: (0..4).map(|c| (format!("Kjerne {c}"), vec![c, c + 8])).collect() };
        let wide = height(std::slice::from_ref(&g), 1600.0, 1.0);
        let narrow = height(std::slice::from_ref(&g), 560.0, 1.0);
        assert!(narrow > wide, "to rader i smalt vindu");
        assert_eq!(wide, (HEAD_H + tile_h(2) + 1.0).ceil());
    }
}
