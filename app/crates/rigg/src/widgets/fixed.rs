//! Fast bredde (som `AdwClamp` med lik minste og naturlige bredde): barnet får
//! akkurat denne bredden, uansett hvor lange etikettene inni er. Brukes for
//! sidefeltet og navigasjonen i Innstillinger, så rammen står stille.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Fixed {
        pub width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Fixed {
        const NAME: &'static str = "RiggFixed";
        type Type = super::Fixed;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Fixed {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Fixed {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(c) = self.obj().first_child() else { return (0, 0, -1, -1) };
            let w = self.width.get();
            match o {
                gtk::Orientation::Horizontal if w > 0 => {
                    let min = c.measure(o, -1).0.min(w);
                    (w.max(min), w.max(min), -1, -1)
                }
                gtk::Orientation::Horizontal => {
                    let (a, b, _, _) = c.measure(o, for_size);
                    (a, b, -1, -1)
                }
                _ => {
                    let fw = if w > 0 { w.max(c.measure(gtk::Orientation::Horizontal, -1).0) } else { for_size };
                    let (a, b, _, _) = c.measure(o, fw);
                    (a, b, -1, -1)
                }
            }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if let Some(c) = self.obj().first_child() {
                let min = c.measure(gtk::Orientation::Horizontal, h).0;
                c.size_allocate(&gtk::Allocation::new(0, 0, w.max(min), h), -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct Fixed(ObjectSubclass<imp::Fixed>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Fixed {
    pub fn new(child: &impl IsA<gtk::Widget>, width: i32) -> Fixed {
        let f: Fixed = glib::Object::new();
        f.imp().width.set(width);
        child.set_parent(&f);
        f
    }

    /// Ny bredde (0 = følg barnet).
    pub fn set_width(&self, w: i32) {
        if self.imp().width.replace(w) != w {
            self.queue_resize();
        }
    }
}

// ---------------------------------------------------------------- Helt eller ikke

mod all_imp {
    use super::*;

    /// Ber om bredde 0 (naturlig: barnets). Får den mindre enn barnets
    /// naturlige bredde, tegnes ikke barnet i det hele tatt.
    #[derive(Default)]
    pub struct AllOrNothing;

    #[glib::object_subclass]
    impl ObjectSubclass for AllOrNothing {
        const NAME: &'static str = "RiggAllOrNothing";
        type Type = super::AllOrNothing;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for AllOrNothing {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for AllOrNothing {
        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(c) if c.is_visible() => {
                    let (min, nat, _, _) = c.measure(o, for_size);
                    if o == gtk::Orientation::Horizontal { (0, nat, -1, -1) } else { (min, nat, -1, -1) }
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if let Some(c) = self.obj().first_child() {
                let nat = c.measure(gtk::Orientation::Horizontal, h).1;
                let fits = w >= nat;
                if c.is_child_visible() != fits {
                    c.set_child_visible(fits);
                }
                if fits {
                    c.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
                }
            }
        }
    }
}

glib::wrapper! {
    /// Barnet helt eller ikke i det hele tatt: forklaringen i et grafhode
    /// forsvinner før tittelen kuttes («Eff… ▬ Tr… ‐‐ G…»).
    pub struct AllOrNothing(ObjectSubclass<all_imp::AllOrNothing>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AllOrNothing {
    pub fn new(child: &impl IsA<gtk::Widget>) -> AllOrNothing {
        let f: AllOrNothing = glib::Object::new();
        child.set_parent(&f);
        f
    }
}
