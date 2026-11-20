//! Varsler nederst til høyre (demoens toast): tittel, en setning, valgfri
//! handling (Angre, Åpne) og en lukkeknapp. Forsvinner av seg selv. Ingen lyd.

use rigg_core::tr;
use crate::state::App;
use crate::widgets::{Icon, hbox, label, vbox};
use gtk::prelude::*;
use std::rc::Rc;
use std::time::Duration;

pub struct Toast {
    pub title: String,
    pub body: Option<String>,
    pub action: Option<(String, Rc<dyn Fn()>)>,
    pub error: bool,
    pub icon: Option<String>,
    pub ms: u32,
}

impl Toast {
    pub fn new(title: &str) -> Toast {
        Toast { title: title.into(), body: None, action: None, error: false, icon: None, ms: 4000 }
    }
    pub fn body(mut self, b: &str) -> Toast {
        self.body = Some(b.into());
        self
    }
    pub fn action(mut self, label: &str, run: impl Fn() + 'static) -> Toast {
        self.action = Some((label.into(), Rc::new(run)));
        self.ms = self.ms.max(5000);
        self
    }
    pub fn error(mut self) -> Toast {
        self.error = true;
        self.ms = self.ms.max(6000);
        self
    }
    pub fn icon(mut self, i: &str) -> Toast {
        self.icon = Some(i.into());
        self
    }
    pub fn ms(mut self, ms: u32) -> Toast {
        self.ms = ms;
        self
    }
}

pub struct Toasts {
    pub root: gtk::Box,
    app: std::rc::Weak<App>,
}

impl Toasts {
    pub fn new(app: &Rc<App>) -> Rc<Toasts> {
        let root = vbox(8, &["toasts"]);
        root.set_halign(gtk::Align::End);
        root.set_valign(gtk::Align::End);
        root.set_can_target(true);
        Rc::new(Toasts { root, app: Rc::downgrade(app) })
    }

    pub fn show(&self, t: Toast) {
        let Some(app) = self.app.upgrade() else { return };
        let card = hbox(12, &["toast"]);
        if t.error {
            card.add_css_class("err");
        }
        let icon = t.icon.clone().unwrap_or_else(|| if t.error { "circle-alert".into() } else { "circle-check".into() });
        card.append(&Icon::new(&icon, 16));
        let text = vbox(0, &["t-text"]);
        text.set_hexpand(true);
        let title = label(&t.title, &["t-title"]);
        title.set_wrap(true);
        title.set_max_width_chars(48);
        text.append(&title);
        if let Some(b) = &t.body {
            let body = label(b, &["t-body"]);
            body.set_wrap(true);
            body.set_max_width_chars(52);
            text.append(&body);
        }
        card.append(&text);
        let rev = gtk::Revealer::new();
        rev.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        rev.set_transition_duration(crate::motion::dur(&app, 3));
        rev.set_child(Some(&card));
        if let Some((l, run)) = &t.action {
            let b = gtk::Button::with_label(l);
            b.add_css_class("sm");
            b.set_valign(gtk::Align::Center);
            let run = run.clone();
            let r = rev.clone();
            let root = self.root.clone();
            b.connect_clicked(move |_| {
                run();
                dismiss(&root, &r);
            });
            card.append(&b);
        }
        let close = crate::widgets::button(Some("x"), None, &["flat", "sm", "icon-only"]);
        close.set_valign(gtk::Align::Center);
        close.update_property(&[gtk::accessible::Property::Label(tr!("Lukk"))]);
        close.set_tooltip_text(Some(tr!("Lukk")));
        {
            let r = rev.clone();
            let root = self.root.clone();
            close.connect_clicked(move |_| dismiss(&root, &r));
        }
        card.append(&close);
        // Høyst tre på en gang: den eldste går først.
        let mut n = 0;
        let mut c = self.root.first_child();
        while let Some(w) = c {
            n += 1;
            c = w.next_sibling();
        }
        if n >= 3
            && let Some(first) = self.root.first_child()
        {
            self.root.remove(&first);
        }
        self.root.append(&rev);
        rev.set_reveal_child(true);
        card.announce(&format!("{} {}", t.title, t.body.clone().unwrap_or_default()), gtk::AccessibleAnnouncementPriority::Medium);
        let root = self.root.clone();
        let r = rev.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(t.ms as u64), move || dismiss(&root, &r));
    }
}

fn dismiss(root: &gtk::Box, r: &gtk::Revealer) {
    if r.parent().is_none() || !r.reveals_child() {
        return;
    }
    r.set_reveal_child(false);
    let root = root.clone();
    let r2 = r.clone();
    let ms = r.transition_duration();
    gtk::glib::timeout_add_local_once(Duration::from_millis(ms as u64 + 20), move || {
        if r2.parent().is_some() {
            root.remove(&r2);
        }
    });
}
