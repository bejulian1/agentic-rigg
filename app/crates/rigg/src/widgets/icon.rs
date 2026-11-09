//! Ikoner: Lucide-stiene fra demoen, tegnet med `GskPath` og strek i
//! tekstfargen (stroke-width 1,75 i et 24-rutenett, runde ender), så de er
//! skarpe i alle størrelser og følger temaet uten symbolske SVG-er.
//!
//! [`Spin`] snurrer ett barn (appikonet i påskeegg 2).

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static CACHE: RefCell<HashMap<String, Option<gsk::Path>>> = RefCell::new(HashMap::new());
}

/// Stien for et ikon (bufret). `None` når navnet ikke finnes.
pub fn path(name: &str) -> Option<gsk::Path> {
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| {
                let d = super::ikoner_data::ICONS.iter().find(|(n, _)| *n == name)?.1;
                let b = gsk::PathBuilder::new();
                for part in d.split('|') {
                    if let Ok(p) = gsk::Path::parse(part) {
                        b.add_path(&p);
                    }
                }
                Some(b.to_path())
            })
            .clone()
    })
}

mod imp {
    use super::*;

    pub struct Icon {
        pub name: RefCell<String>,
        pub path: RefCell<Option<gsk::Path>>,
        pub size: Cell<i32>,
        pub width: Cell<f32>,
    }

    impl Default for Icon {
        fn default() -> Self {
            Icon { name: RefCell::new(String::new()), path: RefCell::new(None), size: Cell::new(16), width: Cell::new(1.75) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Icon {
        const NAME: &'static str = "RiggIcon";
        type Type = super::Icon;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("icon");
            klass.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Icon {}

    impl WidgetImpl for Icon {
        fn measure(&self, _o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            let s = self.size.get();
            (s, s, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(path) = self.path.borrow().clone() else { return };
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            let s = self.size.get() as f32;
            let color = w.color();
            snapshot.save();
            snapshot.translate(&graphene::Point::new(((ww - s) / 2.0).round(), ((hh - s) / 2.0).round()));
            snapshot.scale(s / 24.0, s / 24.0);
            let stroke = gsk::Stroke::new(self.width.get());
            stroke.set_line_cap(gsk::LineCap::Round);
            stroke.set_line_join(gsk::LineJoin::Round);
            snapshot.append_stroke(&path, &stroke, &color);
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct Icon(ObjectSubclass<imp::Icon>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Icon {
    pub fn new(name: &str, size: i32) -> Icon {
        let i: Icon = glib::Object::new();
        i.imp().size.set(size);
        i.set_icon(name);
        i.set_valign(gtk::Align::Center);
        i.set_halign(gtk::Align::Center);
        i.set_can_target(false);
        i
    }

    pub fn set_icon(&self, name: &str) {
        if *self.imp().name.borrow() == name {
            return;
        }
        *self.imp().name.borrow_mut() = name.to_string();
        *self.imp().path.borrow_mut() = path(name);
        self.queue_draw();
    }

    pub fn icon_name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    /// Strektykkelse i 24-rutenettet (standard 1,75; hake og pil 2,25–2,75).
    pub fn set_stroke(&self, w: f32) {
        self.imp().width.set(w);
        self.queue_draw();
    }
}

// ---------------------------------------------------------------- Spin

mod spin_imp {
    use super::*;

    #[derive(Default)]
    pub struct Spin {
        pub angle: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Spin {
        const NAME: &'static str = "RiggSpin";
        type Type = super::Spin;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("spin");
        }
    }

    impl ObjectImpl for Spin {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Spin {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let w = self.obj();
            let Some(child) = w.first_child() else { return };
            let a = self.angle.get();
            if a == 0.0 {
                w.snapshot_child(&child, snapshot);
                return;
            }
            let (cx, cy) = (w.width() as f32 / 2.0, w.height() as f32 / 2.0);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(cx, cy));
            snapshot.rotate(a);
            snapshot.translate(&graphene::Point::new(-cx, -cy));
            w.snapshot_child(&child, snapshot);
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct Spin(ObjectSubclass<spin_imp::Spin>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Spin {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Spin {
        let s: Spin = glib::Object::new();
        child.set_parent(&s);
        s
    }

    pub fn set_angle(&self, deg: f32) {
        self.imp().angle.set(deg);
        self.queue_draw();
    }
}
