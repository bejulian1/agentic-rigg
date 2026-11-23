//! Spalter som i demoens CSS-spalter (`columns`): barna flyter nedover i
//! første spalte, så i neste, i rekkefølge, og spaltene blir omtrent like
//! høye. Tab og lesing går nedover i hver spalte, som i demoen.
//!
//! Antall spalter følger bredden: [`Masonry::set_breakpoints`]. Innstillinger
//! bruker 2 spalter fra 860 px og 3 fra 1480 px når delen har fire paneler
//! eller flere. Sidene i runde 2 kan bruke den samme.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;
use std::cell::Cell;

mod imp {
    use super::*;

    pub struct Masonry {
        pub gap: Cell<i32>,
        pub two: Cell<i32>,
        pub three: Cell<i32>,
        pub three_min: Cell<usize>,
    }

    impl Default for Masonry {
        fn default() -> Self {
            Masonry { gap: Cell::new(12), two: Cell::new(860), three: Cell::new(1480), three_min: Cell::new(4) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Masonry {
        const NAME: &'static str = "RiggMasonry";
        type Type = super::Masonry;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("masonry");
        }
    }

    impl ObjectImpl for Masonry {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl Masonry {
        pub fn children(&self) -> Vec<gtk::Widget> {
            let mut out = Vec::new();
            let mut c = self.obj().first_child();
            while let Some(w) = c {
                c = w.next_sibling();
                if w.is_visible() {
                    out.push(w);
                }
            }
            out
        }

        /// Bruddpunktene gir det høyeste antallet; det går ned til det
        /// bredeste panelet får plass i en spalte (`fit.rs`).
        pub fn cols(&self, width: i32, kids: &[gtk::Widget]) -> usize {
            let n = kids.len();
            let want = if width >= self.three.get() && n >= self.three_min.get() {
                3
            } else if width >= self.two.get() && n >= 2 {
                2
            } else {
                1
            };
            if want == 1 {
                return 1;
            }
            let widest = kids.iter().map(|k| k.measure(gtk::Orientation::Horizontal, -1).0).max().unwrap_or(0);
            crate::widgets::fit::fit_even(want, width, self.gap.get(), widest)
        }

        /// Deler barna i `n` spalter i rekkefølge, så den høyeste blir lavest mulig.
        pub fn split(&self, heights: &[i32], n: usize) -> Vec<usize> {
            let gap = self.gap.get();
            let fits = |limit: i32| -> Option<Vec<usize>> {
                let mut starts = vec![0usize];
                let mut acc = 0;
                for (i, h) in heights.iter().enumerate() {
                    let add = if acc == 0 { *h } else { acc + gap + h };
                    if add > limit && acc > 0 {
                        starts.push(i);
                        acc = *h;
                    } else {
                        acc = add;
                    }
                    if *h > limit {
                        return None;
                    }
                }
                (starts.len() <= n).then_some(starts)
            };
            let total: i32 = heights.iter().sum::<i32>() + gap * (heights.len().saturating_sub(1)) as i32;
            let (mut lo, mut hi) = (heights.iter().copied().max().unwrap_or(0), total.max(1));
            let mut best = fits(hi).unwrap_or_else(|| vec![0]);
            while lo <= hi {
                let mid = (lo + hi) / 2;
                match fits(mid) {
                    Some(s) => {
                        best = s;
                        hi = mid - 1;
                    }
                    None => lo = mid + 1,
                }
            }
            best
        }

        pub fn layout(&self, width: i32) -> crate::widgets::Placed {
            let kids = self.children();
            let n = self.cols(width, &kids);
            let gap = self.gap.get();
            let colw = ((width - gap * (n as i32 - 1)) / n as i32).max(1);
            let heights: Vec<i32> = kids.iter().map(|k| k.measure(gtk::Orientation::Vertical, colw).1).collect();
            let starts = self.split(&heights, n);
            let mut out = Vec::new();
            let mut max_h = 0;
            for (ci, s) in starts.iter().enumerate() {
                let end = starts.get(ci + 1).copied().unwrap_or(kids.len());
                let x = ci as i32 * (colw + gap);
                let mut y = 0;
                for i in *s..end {
                    out.push((kids[i].clone(), x, y, colw, heights[i]));
                    y += heights[i] + gap;
                }
                max_h = max_h.max(y - gap);
            }
            (out, max_h.max(0))
        }
    }

    impl WidgetImpl for Masonry {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => {
                    let min = self.children().iter().map(|k| k.measure(gtk::Orientation::Horizontal, -1).0).max().unwrap_or(0);
                    (min, min, -1, -1)
                }
                _ => {
                    let w = if for_size > 0 { for_size } else { 600 };
                    let (_, h) = self.layout(w);
                    (h, h, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let (items, _) = self.layout(width);
            for (w, x, y, cw, h) in items {
                w.size_allocate(&gtk::Allocation::new(x, y, cw, h), -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct Masonry(ObjectSubclass<imp::Masonry>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Masonry {
    fn default() -> Self {
        Self::new()
    }
}

impl Masonry {
    pub fn new() -> Masonry {
        glib::Object::new()
    }

    pub fn append(&self, w: &impl IsA<gtk::Widget>) {
        w.set_parent(self);
    }

    pub fn clear(&self) {
        while let Some(c) = self.first_child() {
            c.unparent();
        }
    }

    pub fn set_gap(&self, gap: i32) {
        self.imp().gap.set(gap);
        self.queue_resize();
    }

    /// Bredde for to og tre spalter, og hvor mange barn som trengs for tre.
    pub fn set_breakpoints(&self, two: i32, three: i32, three_min: usize) {
        self.imp().two.set(two);
        self.imp().three.set(three);
        self.imp().three_min.set(three_min);
        self.queue_resize();
    }
}
