//! Meny og popover i appens stil: ingen pil, skygge, skaleres fra knappen.
//! Menyen lukkes når et valg er gjort.

use super::{Icon, hbox, label};
use gtk::prelude::*;

pub struct Menu {
    pub popover: gtk::Popover,
    pub list: gtk::Box,
}

impl Menu {
    pub fn new(anchor: &impl IsA<gtk::Widget>) -> Menu {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_halign(gtk::Align::Start);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.set_accessible_role(gtk::AccessibleRole::Menu);
        popover.set_child(Some(&list));
        popover.set_parent(anchor);
        // Popoveren hører til ankeret; fjern den når den lukkes.
        popover.connect_closed(|p| {
            let p = p.clone();
            gtk::glib::idle_add_local_once(move || p.unparent());
        });
        Menu { popover, list }
    }

    pub fn item(&self, icon: Option<&str>, text: &str, key: Option<&str>, run: impl Fn() + 'static) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("flat");
        b.add_css_class("menu-item");
        b.set_accessible_role(gtk::AccessibleRole::MenuItem);
        let bx = hbox(10, &[]);
        if let Some(i) = icon {
            bx.append(&Icon::new(i, 16));
        }
        let l = label(text, &[]);
        l.set_hexpand(true);
        bx.append(&l);
        if let Some(k) = key {
            bx.append(&label(k, &["key"]));
        }
        b.set_child(Some(&bx));
        let p = self.popover.clone();
        b.connect_clicked(move |_| {
            p.popdown();
            run();
        });
        self.list.append(&b);
        b
    }

    pub fn sep(&self) {
        let s = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        s.add_css_class("menu-sep");
        self.list.append(&s);
    }

    pub fn heading(&self, text: &str) {
        self.list.append(&label(text, &["menu-label"]));
    }

    pub fn show(&self) {
        self.popover.popup();
    }
}

/// En popover med fritt innhold under ankeret.
pub fn popover(anchor: &impl IsA<gtk::Widget>, child: &impl IsA<gtk::Widget>) -> gtk::Popover {
    let p = gtk::Popover::new();
    p.set_has_arrow(false);
    p.set_position(gtk::PositionType::Bottom);
    p.set_child(Some(child));
    p.set_parent(anchor);
    p.connect_closed(|p| {
        let p = p.clone();
        gtk::glib::idle_add_local_once(move || p.unparent());
    });
    p
}
