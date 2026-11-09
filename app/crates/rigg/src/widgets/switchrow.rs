//! Bryter og avkrysning. `make_row_toggle` gjør hele raden klikkbar for en
//! bryter (demoens `label.row`), uten å slå to ganger når selve bryteren
//! trykkes.

use gtk::prelude::*;

pub fn switch(active: bool, label: &str, on_change: impl Fn(bool) + 'static) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_active(active);
    s.set_valign(gtk::Align::Center);
    s.update_property(&[gtk::accessible::Property::Label(label)]);
    s.connect_active_notify(move |s| on_change(s.is_active()));
    s
}

/// Klikk hvor som helst på `row` slår bryteren av eller på.
pub fn make_row_toggle(row: &impl IsA<gtk::Widget>, sw: &gtk::Switch) {
    let g = gtk::GestureClick::new();
    let sw = sw.clone();
    let rw = row.clone().upcast::<gtk::Widget>();
    g.connect_released(move |g, _, x, y| {
        if let Some(hit) = rw.pick(x, y, gtk::PickFlags::DEFAULT) {
            let mut w = Some(hit);
            while let Some(c) = w {
                if c == sw.clone().upcast::<gtk::Widget>() || c.is::<gtk::Button>() || c.is::<gtk::CheckButton>() {
                    return;
                }
                if c == rw {
                    break;
                }
                w = c.parent();
            }
        }
        if sw.is_sensitive() {
            sw.set_active(!sw.is_active());
        }
        g.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(g);
}

pub fn check(text: &str, active: bool, on_change: impl Fn(&gtk::CheckButton) + 'static) -> gtk::CheckButton {
    let c = gtk::CheckButton::with_label(text);
    c.set_active(active);
    c.connect_toggled(on_change);
    c
}
