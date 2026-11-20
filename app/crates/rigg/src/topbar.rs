//! Innholdets toppfelt: sidetittelen, sidens knapper (fra `Page::actions`),
//! låsemerket på Styring-sidene, og GNOMEs vinduskontroller.
//! Hele stripen kan dras i (`GtkWindowHandle`), og dobbeltklikk maksimerer.
//!
//! Vinduskontrollene følger `gtk-decoration-layout`: knappene til høyre står
//! her, og knappene til venstre (f.eks. «close,minimize:») står i sidefeltets
//! toppfelt, eller først i denne stripen når sidefeltet er en ikonlist
//! ([`Topbar::set_start_here`]). Tomme kontroller skjules, så de ikke gir
//! luft (runde 6B).
//!
//! **For 2C:** [`Topbar::set_locked`] setter merket «Låst» for en side.

use rigg_core::tr;
use crate::widgets::{Icon, hbox, label, set_text};
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

pub struct Topbar {
    pub handle: gtk::WindowHandle,
    pub bar: gtk::Box,
    title: gtk::Label,
    /// «Skjermkort ›» foran tittelen på Styring-sidene.
    crumb: gtk::Label,
    actions: gtk::Stack,
    lock: gtk::Box,
    locked: RefCell<HashMap<String, bool>>,
    current: RefCell<String>,
    start: gtk::WindowControls,
    start_here: std::rc::Rc<std::cell::Cell<bool>>,
}

/// Vinduskontroller som skjuler seg selv når layouten ikke gir dem knapper.
/// `extra`: et vilkår til (f.eks. at sidefeltet ikke er en ikonlist).
pub fn controls(side: gtk::PackType, extra: std::rc::Rc<std::cell::Cell<bool>>) -> gtk::WindowControls {
    let c = gtk::WindowControls::new(side);
    c.set_valign(gtk::Align::Center);
    let show = move |c: &gtk::WindowControls| c.set_visible(!c.is_empty() && extra.get());
    show(&c);
    c.connect_empty_notify(show);
    c
}

impl Topbar {
    pub fn new() -> Topbar {
        let bar = hbox(8, &["main-head"]);
        let start_here = std::rc::Rc::new(std::cell::Cell::new(false));
        let start = controls(gtk::PackType::Start, start_here.clone());
        bar.append(&start);
        let crumb = label("", &["main-crumb"]);
        crumb.set_visible(false);
        bar.append(&crumb);
        let title = label("", &["main-title"]);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_accessible_role(gtk::AccessibleRole::Heading);
        bar.append(&title);
        let actions = gtk::Stack::new();
        actions.set_hhomogeneous(false);
        actions.set_vhomogeneous(false);
        actions.set_interpolate_size(false);
        actions.add_named(&gtk::Box::new(gtk::Orientation::Horizontal, 0), Some("tom"));
        bar.append(&actions);
        let lock = crate::widgets::rows::tag(tr!("Låst"), Some("lock"), "");
        lock.add_css_class("tag-lg");
        lock.set_tooltip_text(Some(tr!("Endringer er låst. Verdiene er lest fra kortet.")));
        lock.set_valign(gtk::Align::Center);
        lock.set_visible(false);
        bar.append(&lock);
        bar.append(&controls(gtk::PackType::End, std::rc::Rc::new(std::cell::Cell::new(true))));
        let handle = gtk::WindowHandle::new();
        handle.set_child(Some(&bar));
        Topbar { handle, bar, title, crumb, actions, lock, locked: RefCell::new(HashMap::new()), current: RefCell::new(String::new()), start, start_here }
    }

    /// Knappene til venstre i layouten står her når sidefeltet er en ikonlist
    /// (der er det ikke plass til dem), ellers i sidefeltets toppfelt.
    pub fn set_start_here(&self, here: bool) {
        self.start_setter()(here);
    }

    /// [`Topbar::set_start_here`] som en lukking (til sidefeltet).
    pub fn start_setter(&self) -> impl Fn(bool) + 'static {
        let (start, here) = (self.start.clone(), self.start_here.clone());
        move |h| {
            here.set(h);
            start.set_visible(h && !start.is_empty());
        }
    }

    /// Knappene for en side (legges inn én gang, når siden bygges).
    pub fn add_actions(&self, page: &str, w: &gtk::Widget) {
        let bx = hbox(4, &["head-actions"]);
        bx.set_valign(gtk::Align::Center);
        bx.append(w);
        self.actions.add_named(&bx, Some(page));
    }

    pub fn show(&self, page: &str, title: &str) {
        set_text(&self.title, title);
        let crumb = crate::pages::crumb(page);
        set_text(&self.crumb, &crumb.map(|c| format!("{c}  ›")).unwrap_or_default());
        self.crumb.set_visible(crumb.is_some());
        self.title.update_property(&[gtk::accessible::Property::Label(&crate::pages::full_title(page))]);
        *self.current.borrow_mut() = page.to_string();
        if self.actions.child_by_name(page).is_some() {
            self.actions.set_visible_child_name(page);
        } else {
            self.actions.set_visible_child_name("tom");
        }
        self.lock.set_visible(self.locked.borrow().get(page).copied().unwrap_or(false));
    }

    /// Merket «Låst» der Forkast og Bruk sto i demoen (produktspesifikasjonen 6.2).
    pub fn set_locked(&self, page: &str, locked: bool) {
        self.locked.borrow_mut().insert(page.to_string(), locked);
        if *self.current.borrow() == page {
            self.lock.set_visible(locked);
        }
    }

    pub fn lock_icon() -> Icon {
        Icon::new("lock", 12)
    }
}

impl Default for Topbar {
    fn default() -> Self {
        Self::new()
    }
}
