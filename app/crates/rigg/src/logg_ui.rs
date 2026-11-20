//! Logging i vinduet: knappen «Logg til fil» (Ctrl+L) i toppfeltet på
//! Sensorer, loggstatusen ved appnavnet i sidefeltet, og «Vis i Filer»
//! (produktspesifikasjonen 3.2). Loggen kjøres av tjenesten; vinduet bare
//! starter, stopper og viser den.
//!
//! Grunnmuren kaller [`attach`] én gang ved oppstart. Den kobler også inn
//! appens handling `vis-side`, som skrivebordsvarslene fra tjenesten bruker
//! for å åpne appen på riktig side. Ctrl+L går til [`toggle`] fra skallets
//! taster (`shortcuts.rs`), så den virker på alle sidene og i Innstillinger.
//! I smale vinduer viser knappen bare ikonet ([`set_compact`]).

#[cfg(debug_assertions)]
mod bilder;

use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{hbox, label, set_text};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::fmt as F;
use rigg_core::{tr, trf, trn};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// Opptaksikonet (Lucide `circle-dot`): en ring med en prikk. Prikken blir
/// større og rød mens loggen går. Tegnes med Cairo, så det er skarpt i alle
/// størrelser og følger tekstfargen.
fn rec_icon(size: i32) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::new();
    d.add_css_class("rec");
    d.set_content_width(size);
    d.set_content_height(size);
    d.set_valign(gtk::Align::Center);
    d.set_draw_func(move |w, cr, wd, ht| {
        let c = w.color();
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        let s = wd.min(ht) as f64 / 24.0;
        let (cx, cy) = (wd as f64 / 2.0, ht as f64 / 2.0);
        cr.set_line_width(1.75 * s);
        cr.arc(cx, cy, 9.25 * s, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
        let r = if w.has_css_class("on") { 4.5 } else { 1.9 };
        cr.arc(cx, cy, r * s, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });
    d
}

/// «0:42», «12:04», «1:02:33».
pub fn elapsed(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let (h, m, sec) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m}:{sec:02}") }
}

fn file_name(v: &Value) -> String {
    v["file"].as_str().map(|f| std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.to_string())).unwrap_or_default()
}

/// «1 234 rader · 2,1 MB».
pub fn rows_text(v: &Value) -> String {
    let rows = v["rows"].as_f64().unwrap_or(0.0) as i64;
    let bytes = v["bytes"].as_f64();
    format!("{} · {}", trn!(rows, "{antall} rad", "{antall} rader", antall = F::num(Some(rows as f64), 0)), F::bytes(bytes, 1))
}

struct Btn {
    b: glib::WeakRef<gtk::Button>,
    icon: gtk::DrawingArea,
    text: gtk::Label,
}

struct LogUi {
    app: Weak<App>,
    buttons: RefCell<Vec<Btn>>,
    active: Cell<bool>,
    busy: Cell<bool>,
    ticker: RefCell<Option<glib::SourceId>>,
    last_poll: Cell<i64>,
    /// Smalt vindu: knappen viser bare ikonet.
    compact: Cell<bool>,
}

thread_local! {
    static UI: RefCell<Option<Rc<LogUi>>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<LogUi>> {
    UI.with(|u| u.borrow().clone())
}

/// Loggstatusen fra tjenesten (`log_status` eller hendelsen `log`).
pub fn status(app: &App) -> Value {
    app.data.borrow().log.clone()
}

pub fn is_active(app: &App) -> bool {
    status(app)["active"].as_bool().unwrap_or(false)
}

impl LogUi {
    fn render(&self) {
        let Some(app) = self.app.upgrade() else { return };
        let st = status(&app);
        let on = st["active"].as_bool().unwrap_or(false) && app.client.is_connected();
        let started = st["startedAt"].as_i64().unwrap_or(0);
        let time = elapsed(rigg_core::now_ms() - started);
        let tip = if on {
            let iv = st["intervalMs"].as_u64().map(|i| F::seconds(i as u32)).unwrap_or_default();
            let fmt = if st["format"] == "norsk" { tr!("norsk regneark") } else { tr!("standard CSV") };
            trf!(
                "Logger til fil siden {tid} · {fil}\n{rader} · hvert {intervall} · {format}\nCtrl+L stopper loggen.",
                tid = F::time(crate::app::local_time(started), false),
                fil = file_name(&st),
                rader = rows_text(&st),
                intervall = iv,
                format = fmt,
            )
        } else {
            String::new()
        };
        if let Some(s) = app.shell.get() {
            if on {
                s.sidebar.set_log_indicator(Some((&time, &tip)));
            } else {
                s.sidebar.set_log_indicator(None);
            }
        }
        self.buttons.borrow_mut().retain(|x| {
            let Some(b) = x.b.upgrade() else { return false };
            crate::widgets::class(&x.icon, "on", on);
            crate::widgets::class(&b, "logging", on);
            x.icon.queue_draw();
            let what = if on { tr!("Stopp loggen") } else { tr!("Logg til fil") };
            set_text(&x.text, what);
            x.text.set_visible(!self.compact.get());
            b.update_property(&[gtk::accessible::Property::Label(what)]);
            let t = if on { tip.clone() } else { tr!("Logg alle eller utvalgte sensorer til en CSV-fil. Loggen fortsetter når vinduet lukkes.  ·  Ctrl+L").to_string() };
            if b.tooltip_text().as_deref() != Some(t.as_str()) {
                b.set_tooltip_text(Some(&t));
            }
            b.set_sensitive(app.client.is_connected() && !self.busy.get());
            true
        });
        // Tiden tikker hvert sekund mens loggen går; rader og størrelse hentes hvert 5. s.
        if on && self.ticker.borrow().is_none() {
            let me = ui();
            let id = glib::timeout_add_local(Duration::from_secs(1), move || {
                let Some(me) = me.clone() else { return glib::ControlFlow::Break };
                me.tick();
                glib::ControlFlow::Continue
            });
            *self.ticker.borrow_mut() = Some(id);
        } else if !on && let Some(id) = self.ticker.borrow_mut().take() {
            id.remove();
        }
    }

    fn tick(&self) {
        let Some(app) = self.app.upgrade() else { return };
        let now = rigg_core::now_ms();
        if now - self.last_poll.get() >= 5_000 {
            self.last_poll.set(now);
            let a = app.clone();
            app.client.request("log_status", json!({}), move |res| {
                if let Ok(v) = res {
                    a.data.borrow_mut().log = v.clone();
                    a.on_log.emit(&v);
                }
            });
        }
        self.render();
    }

    /// Overganger: varsel når loggen stopper av seg selv (disken er full).
    fn on_log(&self, v: &Value) {
        let Some(app) = self.app.upgrade() else { return };
        let on = v["active"].as_bool().unwrap_or(false);
        let was = self.active.replace(on);
        if was && !on
            && let Some(err) = v["error"].as_str()
        {
            let p = v["file"].as_str().map(std::path::PathBuf::from);
            let a = app.clone();
            let mut t = Toast::new(tr!("Loggen er stoppet")).body(err).error().ms(12_000);
            if let Some(p) = p {
                t = t.action(tr!("Vis i Filer"), move || crate::rapport_ui::show_in_files(&a, &p));
            }
            app.toast(t);
        }
        self.render();
    }
}

/// Starter eller stopper loggen (knappen, Ctrl+L og Innstillinger).
pub fn toggle(app: &Rc<App>) {
    if is_active(app) { stop(app) } else { start(app) }
}

pub fn start(app: &Rc<App>) {
    let Some(u) = ui() else { return };
    if u.busy.replace(true) {
        return;
    }
    u.render();
    let a = app.clone();
    app.client.request("log_start", json!({}), move |res| {
        if let Some(u) = ui() {
            u.busy.set(false);
        }
        match res {
            Ok(v) => {
                a.data.borrow_mut().log = v.clone();
                if let Some(u) = ui() {
                    u.active.set(true);
                }
                a.on_log.emit(&v);
                let dir = v["dir"].as_str().map(|d| rigg_core::paths::tilde(std::path::Path::new(d))).unwrap_or_default();
                let p = v["file"].as_str().map(std::path::PathBuf::from);
                let b = a.clone();
                let mut t = Toast::new(tr!("Logger til fil")).body(&trf!("{fil} i {mappe}", fil = file_name(&v), mappe = dir)).icon("check");
                if let Some(p) = p {
                    t = t.action(tr!("Vis i Filer"), move || crate::rapport_ui::show_in_files(&b, &p));
                }
                a.toast(t);
            }
            Err(e) => {
                a.toast(Toast::new(tr!("Loggen startet ikke")).body(&e.message).error());
                if let Some(u) = ui() {
                    u.render();
                }
            }
        }
    });
}

pub fn stop(app: &Rc<App>) {
    let Some(u) = ui() else { return };
    if u.busy.replace(true) {
        return;
    }
    let a = app.clone();
    app.client.request("log_stop", json!({}), move |res| {
        if let Some(u) = ui() {
            u.busy.set(false);
            u.active.set(false);
        }
        match res {
            Ok(v) => {
                a.data.borrow_mut().log = v.clone();
                a.on_log.emit(&v);
                let p = v["file"].as_str().map(std::path::PathBuf::from);
                let b = a.clone();
                let mut t = Toast::new(tr!("Loggen er lagret")).body(&format!("{} · {}", file_name(&v), rows_text(&v))).icon("check");
                if let Some(p) = p {
                    t = t.action(tr!("Vis i Filer"), move || crate::rapport_ui::show_in_files(&b, &p));
                }
                a.toast(t);
            }
            Err(e) => a.toast(Toast::new(tr!("Loggen stoppet ikke")).body(&e.message).error()),
        }
    });
}

/// Kobler loggstatusen til sidefeltet, Ctrl+L og handlingen `vis-side`.
pub fn attach(app: &Rc<App>) {
    let u = Rc::new(LogUi {
        app: Rc::downgrade(app),
        buttons: RefCell::new(vec![]),
        active: Cell::new(false),
        busy: Cell::new(false),
        ticker: RefCell::new(None),
        last_poll: Cell::new(0),
        compact: Cell::new(false),
    });
    UI.with(|x| *x.borrow_mut() = Some(u.clone()));
    {
        let u = u.clone();
        app.on_log.connect(move |v| u.on_log(v));
    }
    {
        let u = u.clone();
        app.on_conn.connect(move |_| u.render());
    }
    // Skrivebordsvarslene åpner appen på en side: `app.vis-side("sensorer")`.
    let act = gio::SimpleAction::new("vis-side", Some(glib::VariantTy::STRING));
    let a = app.clone();
    act.connect_activate(move |_, p| {
        if let Some(page) = p.and_then(|v| v.str()) {
            a.go(page);
        }
        if let Some(w) = a.window() {
            w.present();
        }
    });
    app.gtk.add_action(&act);
    #[cfg(debug_assertions)]
    bilder::start(app);
}

/// Smalt vindu: knappen viser bare ikonet (teksten står i verktøytipset).
pub fn set_compact(compact: bool) {
    if let Some(u) = ui()
        && u.compact.replace(compact) != compact
    {
        u.render();
    }
}

/// Knappen til toppfeltet på Sensorer (2A legger den inn med denne).
pub fn button(app: &Rc<App>) -> Option<gtk::Widget> {
    let b = gtk::Button::new();
    b.add_css_class("flat");
    b.add_css_class("log-btn");
    let bx = hbox(6, &[]);
    let icon = rec_icon(16);
    let text = label(tr!("Logg til fil"), &[]);
    bx.append(&icon);
    bx.append(&text);
    b.set_child(Some(&bx));
    b.update_property(&[gtk::accessible::Property::KeyShortcuts("Control+L")]);
    let a = app.clone();
    b.connect_clicked(move |_| toggle(&a));
    if let Some(u) = ui() {
        u.buttons.borrow_mut().push(Btn { b: b.downgrade(), icon, text });
        u.render();
    }
    Some(b.upcast())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tid_og_rader() {
        assert_eq!(super::elapsed(42_000), "0:42");
        assert_eq!(super::elapsed(724_000), "12:04");
        assert_eq!(super::elapsed(3_753_000), "1:02:33");
    }
}
