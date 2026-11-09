//! Segmentknapp (demoens `.seg`, som `AdwToggleGroup`): valgt knapp har egen
//! flate og halvfet tekst. Piltastene flytter valget, som en radiogruppe.

use super::hbox;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type ChangeSlot = super::Slot<dyn Fn(&str)>;

#[derive(Clone)]
pub struct Seg {
    pub root: gtk::Box,
    buttons: Rc<Vec<(String, gtk::ToggleButton)>>,
    on_change: Rc<ChangeSlot>,
    quiet: Rc<std::cell::Cell<bool>>,
}

impl Seg {
    /// `opts`: (verdi, tekst). `label`: navnet for skjermlesere.
    pub fn new(opts: &[(&str, &str)], selected: &str, label: &str) -> Seg {
        let root = hbox(2, &["seg"]);
        root.set_halign(gtk::Align::Start);
        root.set_valign(gtk::Align::Center);
        root.set_accessible_role(gtk::AccessibleRole::RadioGroup);
        root.update_property(&[gtk::accessible::Property::Label(label)]);
        let mut buttons = Vec::new();
        let mut first: Option<gtk::ToggleButton> = None;
        for (v, t) in opts {
            let b = gtk::ToggleButton::with_label(t);
            if let Some(f) = &first {
                b.set_group(Some(f));
            } else {
                first = Some(b.clone());
            }
            b.set_active(*v == selected);
            root.append(&b);
            buttons.push((v.to_string(), b));
        }
        let s = Seg { root, buttons: Rc::new(buttons), on_change: Rc::new(RefCell::new(None)), quiet: Rc::new(std::cell::Cell::new(false)) };
        for (v, b) in s.buttons.iter() {
            let s2 = s.clone();
            let v = v.clone();
            b.connect_toggled(move |b| {
                if b.is_active() && !s2.quiet.get()
                    && let Some(f) = s2.on_change.borrow().as_ref()
                {
                    f(&v);
                }
            });
        }
        // Piltastene flytter valget (som en radiogruppe).
        let keys = gtk::EventControllerKey::new();
        let s3 = s.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let dir = match key {
                gtk::gdk::Key::Left | gtk::gdk::Key::Up => -1,
                gtk::gdk::Key::Right | gtk::gdk::Key::Down => 1,
                _ => return glib::Propagation::Proceed,
            };
            let i = s3.buttons.iter().position(|(_, b)| b.is_active()).unwrap_or(0) as i32;
            let n = s3.buttons.len() as i32;
            let j = (i + dir).clamp(0, n - 1) as usize;
            s3.buttons[j].1.set_active(true);
            s3.buttons[j].1.grab_focus();
            glib::Propagation::Stop
        });
        s.root.add_controller(keys);
        s
    }

    pub fn small(self) -> Seg {
        self.root.add_css_class("sm");
        self
    }

    pub fn connect(&self, f: impl Fn(&str) + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    /// Setter valget uten å kalle `connect`-funksjonen.
    pub fn set(&self, value: &str) {
        self.quiet.set(true);
        for (v, b) in self.buttons.iter() {
            if v == value {
                b.set_active(true);
            }
        }
        self.quiet.set(false);
    }

    pub fn value(&self) -> Option<String> {
        self.buttons.iter().find(|(_, b)| b.is_active()).map(|(v, _)| v.clone())
    }

    pub fn button(&self, value: &str) -> Option<gtk::ToggleButton> {
        self.buttons.iter().find(|(v, _)| v == value).map(|(_, b)| b.clone())
    }
}

use gtk::glib;
