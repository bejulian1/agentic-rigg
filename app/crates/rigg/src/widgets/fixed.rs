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
        // Rammen står stille også om innholdet en gang krever mer enn bredden:
        // det klippes i kanten i stedet for å tegnes inn over naboflaten.
        f.set_overflow(gtk::Overflow::Hidden);
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

// ---------------------------------------------------------------- Ønsket bredde

mod pref_imp {
    use super::*;

    #[derive(Default)]
    pub struct Preferred {
        pub width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Preferred {
        const NAME: &'static str = "RiggPreferred";
        type Type = super::Preferred;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Preferred {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Preferred {
        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(c) = self.obj().first_child() else { return (0, 0, -1, -1) };
            let (min, nat, _, _) = c.measure(o, for_size);
            if o == gtk::Orientation::Horizontal { (min, self.width.get().max(min), -1, -1) } else { (min, nat, -1, -1) }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if let Some(c) = self.obj().first_child() {
                c.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
            }
        }
    }
}

glib::wrapper! {
    /// Barnet ønsker seg `width`, men kan krympe til sin egen minste bredde.
    /// En `GtkScale` har ellers lik minste og naturlig bredde: med en fast
    /// størrelse på 260 px holdt glideren på RGB-siden vinduet bredere enn
    /// appens minste bredde.
    pub struct Preferred(ObjectSubclass<pref_imp::Preferred>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Preferred {
    pub fn new(child: &impl IsA<gtk::Widget>, width: i32) -> Preferred {
        let f: Preferred = glib::Object::new();
        f.imp().width.set(width);
        child.set_parent(&f);
        f
    }
}

// ---------------------------------------------------------------- Bredde uavhengig av høyden

mod steady_imp {
    use super::*;

    #[derive(Default)]
    pub struct Steady;

    #[glib::object_subclass]
    impl ObjectSubclass for Steady {
        const NAME: &'static str = "RiggSteady";
        type Type = super::Steady;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Steady {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Steady {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            // Et skjult barn tar ingen plass (delnavnene i Innstillinger i smale vinduer).
            let Some(c) = self.obj().first_child().filter(|c| c.is_visible()) else { return (0, 0, -1, -1) };
            // Bredden spørres aldri «for en høyde».
            let (min, nat, _, _) = c.measure(o, if o == gtk::Orientation::Horizontal { -1 } else { for_size });
            (min, nat, -1, -1)
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if let Some(c) = self.obj().first_child().filter(|c| c.is_visible()) {
                c.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
            }
        }
    }
}

glib::wrapper! {
    /// En etikett som kan brytes, i en vannrett rad: raden spør etiketten om
    /// bredden *for radens høyde*. Er raden høy nok til to linjer (tetthet
    /// «Luftig», større skrift), svarer etiketten med bredden til to linjer,
    /// og «GeForce» ble til «GeF-/orce» selv om det var god plass. Her får
    /// barnet alltid spørsmålet uten høyde, så det brytes bare når bredden
    /// faktisk er for liten.
    pub struct Steady(ObjectSubclass<steady_imp::Steady>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Steady {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Steady {
        let f: Steady = glib::Object::new();
        child.set_parent(&f);
        f
    }
}
