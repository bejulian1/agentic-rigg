//! Panel (demoens `.group`): 1 px ramme, liten radius, overskrift i en lav
//! stripe med en linje under, og `aside` til høyre i stripen.

use super::{hbox, label, vbox};
use gtk::prelude::*;

#[derive(Clone)]
pub struct Panel {
    pub root: gtk::Box,
    pub head: gtk::Box,
    pub title: gtk::Label,
    pub aside: gtk::Box,
    pub body: gtk::Box,
}

impl Panel {
    pub fn new(title: &str) -> Panel {
        let root = vbox(0, &["group"]);
        let head = hbox(8, &["group-head"]);
        let t = label(title, &["group-title"]);
        t.set_accessible_role(gtk::AccessibleRole::Heading);
        head.append(&t);
        let aside = hbox(8, &["aside"]);
        aside.set_hexpand(true);
        aside.set_halign(gtk::Align::End);
        head.append(&aside);
        let body = vbox(0, &["group-body"]);
        root.append(&head);
        root.append(&body);
        root.update_property(&[gtk::accessible::Property::Label(title)]);
        Panel { root, head, title: t, aside, body }
    }
}
