//! Linjene under toppfeltet (demoens `.banners`): tjenesten svarer ikke,
//! tjenesten er fra et annet bygg, og tilstanden til kortet på GPU-sidene
//! (produktspesifikasjonen 3.8). Bare ved feil; ingen spinnere. «Kobler til
//! tjenesten …» vises bare hvis det tar mer enn 300 ms.

use rigg_core::{tr, trf};
use crate::state::{App, Conn};
use crate::widgets::{Icon, hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::model::DeviceState;
use std::rc::Rc;
use std::time::Duration;

pub struct Infobar {
    pub root: gtk::Box,
    conn: gtk::Revealer,
    conn_line: Line,
    conn_btn: gtk::Button,
    build: gtk::Revealer,
    build_line: Line,
    device: gtk::Revealer,
    device_line: Line,
    device_box: gtk::Box,
}

/// Tittel og tekst i én etikett som brytes, så linja aldri gjør vinduet bredere.
#[derive(Clone)]
struct Line {
    label: gtk::Label,
    title: std::rc::Rc<std::cell::RefCell<String>>,
}

impl Line {
    fn set(&self, title: &str, text: &str) {
        *self.title.borrow_mut() = title.to_string();
        let esc = |t: &str| gtk::glib::markup_escape_text(t).to_string();
        self.label.set_markup(&format!("<b>{}</b> {}", esc(title), esc(text)));
    }
}

fn banner(icon: &str, kind: &str) -> (gtk::Revealer, gtk::Box, Line) {
    let b = hbox(12, &["banner"]);
    if !kind.is_empty() {
        b.add_css_class(kind);
    }
    b.append(&Icon::new(icon, 16));
    let l = label("", &[]);
    l.set_wrap(true);
    l.set_hexpand(true);
    b.append(&l);
    let r = gtk::Revealer::new();
    r.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    r.set_child(Some(&b));
    (r, b, Line { label: l, title: Default::default() })
}

impl Infobar {
    pub fn new(app: &Rc<App>) -> Rc<Infobar> {
        let root = vbox(0, &[]);
        let (conn, cb, conn_line) = banner("unplug", "");
        let conn_btn = gtk::Button::with_label(tr!("Start på nytt"));
        conn_btn.add_css_class("sm");
        conn_btn.set_valign(gtk::Align::Center);
        cb.append(&conn_btn);
        let (build, bb, build_line) = banner("refresh-cw", "warn");
        let restart = gtk::Button::with_label(tr!("Start tjenesten på nytt"));
        restart.add_css_class("sm");
        restart.set_valign(gtk::Align::Center);
        bb.append(&restart);
        let (device, device_box, device_line) = banner("triangle-alert", "warn");
        root.append(&conn);
        root.append(&build);
        root.append(&device);
        let me = Rc::new(Infobar { root, conn, conn_line, conn_btn, build, build_line, device, device_line, device_box });
        {
            let a = app.clone();
            me.conn_btn.connect_clicked(move |_| a.client.reconnect_now());
        }
        {
            let a = app.clone();
            restart.connect_clicked(move |_| {
                // Ber den gamle tjenesten stoppe når vinduet kobler fra; vinduet starter en ny.
                a.client.request("shutdown_when_idle", serde_json::json!({}), |_| {});
                a.toast(crate::toast::Toast::new(tr!("Tjenesten startes på nytt")).body(tr!("Den gamle stopper når vinduet kobler fra. Lukk og åpne vinduet.")));
            });
        }
        let m = Rc::downgrade(&me);
        let a = app.clone();
        app.on_conn.connect(move |c| {
            if let Some(me) = m.upgrade() {
                me.render_conn(&a, *c);
            }
        });
        let m = Rc::downgrade(&me);
        let a = app.clone();
        app.on_devices.connect(move |_| {
            if let Some(me) = m.upgrade() {
                me.render_device(&a);
                me.render_build(&a);
            }
        });
        let m = Rc::downgrade(&me);
        let a = app.clone();
        app.on_page.connect(move |_| {
            if let Some(me) = m.upgrade() {
                me.render_device(&a);
            }
        });
        // «Kobler til tjenesten …» bare hvis det tar mer enn 300 ms.
        let m = Rc::downgrade(&me);
        let a = app.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(me) = m.upgrade()
                && !a.client.is_connected()
            {
                me.conn_line.set(tr!("Kobler til tjenesten …"), "");
                me.conn_btn.set_visible(false);
                if let Some(b) = me.conn.child() {
                    b.remove_css_class("danger");
                    b.add_css_class("info");
                }
                me.conn.set_reveal_child(true);
            }
        });
        me
    }

    fn render_conn(&self, app: &App, c: Conn) {
        match c {
            Conn::Connected => self.conn.set_reveal_child(false),
            Conn::Connecting => {}
            Conn::Lost(_) => {
                let a = app.client.clone();
                let (r, line, b) = (self.conn.clone(), self.conn_line.clone(), self.conn_btn.clone());
                // Linja først etter 2 s, så en kort omstart ikke blinker.
                gtk::glib::timeout_add_local_once(Duration::from_secs(2), move || {
                    if a.is_connected() {
                        return;
                    }
                    line.set(tr!("Tjenesten svarer ikke."), tr!("Verdiene under er gamle. Appen prøver igjen av seg selv."));
                    b.set_visible(true);
                    if let Some(bx) = r.child() {
                        bx.remove_css_class("info");
                    }
                    r.set_reveal_child(true);
                    r.announce(tr!("Tjenesten svarer ikke. Verdiene er gamle."), gtk::AccessibleAnnouncementPriority::Medium);
                });
            }
        }
    }

    fn render_build(&self, app: &App) {
        let hello = app.data.borrow().hello.clone();
        let theirs = hello.as_ref().and_then(|h| h["daemon"]["build"].as_u64());
        let mine = rigg_core::version::build() as u64;
        match theirs {
            Some(b) if b != mine => {
                self.build_line.set(tr!("Tjenesten er fra et annet bygg."), &trf!("Tjenesten er fra bygg {b}, vinduet fra bygg {mine}. Start tjenesten på nytt.", b = b, mine = mine));
                self.build.set_reveal_child(true);
            }
            _ => self.build.set_reveal_child(false),
        }
    }

    /// Kortets tilstand på sidene som viser kortet. Ikke på Oversikt (der står
    /// den i tilstand-linja) eller Styring-sidene (der står den i
    /// «Styrer»-linja), så det ikke står to ganger (runde 8A).
    fn render_device(&self, app: &App) {
        let page = app.page.borrow().clone();
        let gpu_page = matches!(page.as_str(), "grafer" | "grafikkort");
        let info = app.gpu_info();
        let msg = info.as_ref().and_then(|i| {
            let since = crate::app::local_time(i.since);
            match i.state {
                DeviceState::DriverUpdated => Some((
                    tr!("Driveren er oppdatert."),
                    trf!(
                        "Ny NVIDIA-driver ({ny}) er installert, men den gamle ({gammel}) kjører fortsatt. Start maskinen på nytt for å lese skjermkortet.",
                        ny = i.driver_installed.as_deref().unwrap_or(tr!("ny")),
                        gammel = i.driver.as_deref().unwrap_or(tr!("gammel"))
                    ),
                    "warn",
                )),
                DeviceState::NoDriver => Some((tr!("Fant ingen NVIDIA-driver."), tr!("Prosessor, minne og sensorer vises som vanlig.").to_string(), "info")),
                DeviceState::Lost => Some((
                    tr!("Skjermkortet svarer ikke."),
                    trf!("Siden {tid}. Det skjer når kortet faller av bussen eller trekkes ut. Tallene før det er tatt vare på.", tid = rigg_core::fmt::time(since, false)),
                    "",
                )),
                _ => None,
            }
        });
        match msg {
            Some((t, s, kind)) if gpu_page => {
                self.device_line.set(t, &s);
                for k in ["warn", "info"] {
                    crate::widgets::class(&self.device_box, k, k == kind);
                }
                if let Some(i) = &info
                    && let Some(d) = &i.detail
                {
                    self.device_box.set_tooltip_text(Some(d));
                }
                self.device.set_reveal_child(true);
            }
            _ => self.device.set_reveal_child(false),
        }
    }
}
