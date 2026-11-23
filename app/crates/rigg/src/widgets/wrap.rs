//! Brikker i naturlig bredde som brytes til neste linje (CSS `flex-wrap`).
//! `GtkFlowBox` legger barna i like spalter, som gir ujevne mellomrom.
//!
//! Brukes for instruksjonssettene på Prosessor og for seksjonsoverskriften
//! med merke i sidefeltet («Skjermkortstyring» + «Oppdater hjelper»): merket
//! går ned på egen linje når det ikke får plass, i stedet for å presse
//! sidefeltet bredere enn spalten sin.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod wrap_imp {
    use super::*;
    use std::cell::Cell;

    pub struct Wrap {
        pub gap: Cell<i32>,
    }

    impl Default for Wrap {
        fn default() -> Self {
            Wrap { gap: Cell::new(GAP) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Wrap {
        const NAME: &'static str = "RiggWrap";
        type Type = super::Wrap;
        type ParentType = gtk::Widget;
        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("wrap");
        }
    }

    impl ObjectImpl for Wrap {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    /// Standard avstand mellom brikkene og mellom linjene.
    pub const GAP: i32 = 4;

    impl Wrap {
        /// (barn, x, y, bredde, høyde) og total høyde for en bredde.
        pub fn layout(&self, width: i32) -> crate::widgets::Placed {
            let mut out = Vec::new();
            let (mut x, mut y, mut line_h) = (0, 0, 0);
            let mut c = self.obj().first_child();
            while let Some(w) = c {
                c = w.next_sibling();
                if !w.is_visible() {
                    continue;
                }
                let cw = w.measure(gtk::Orientation::Horizontal, -1).1.min(width.max(1));
                let ch = w.measure(gtk::Orientation::Vertical, cw).1;
                if x > 0 && x + cw > width {
                    x = 0;
                    y += line_h + GAP;
                    line_h = 0;
                }
                out.push((w, x, y, cw, ch));
                x += cw + self.gap.get();
                line_h = line_h.max(ch);
            }
            (out, if line_h > 0 { y + line_h } else { 0 })
        }
    }

    impl WidgetImpl for Wrap {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }
        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => {
                    let mut min = 0;
                    let mut c = self.obj().first_child();
                    while let Some(w) = c {
                        min = min.max(w.measure(gtk::Orientation::Horizontal, -1).0);
                        c = w.next_sibling();
                    }
                    (min, min.max(200), -1, -1)
                }
                _ => {
                    let (_, h) = self.layout(if for_size > 0 { for_size } else { 400 });
                    (h, h, -1, -1)
                }
            }
        }
        fn size_allocate(&self, width: i32, _h: i32, _b: i32) {
            for (w, x, y, cw, ch) in self.layout(width).0 {
                w.size_allocate(&gtk::Allocation::new(x, y, cw, ch), -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct Wrap(ObjectSubclass<wrap_imp::Wrap>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Wrap {
    fn default() -> Self {
        Self::new()
    }
}

impl Wrap {
    pub fn new() -> Wrap {
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
    /// Avstanden mellom brikkene på samme linje (standard 4 px).
    pub fn set_gap(&self, gap: i32) {
        self.imp().gap.set(gap);
        self.queue_resize();
    }
}
