//! System: hovedkort og BIOS, minne, operativsystem og kontrollerne (nett,
//! lyd, USB, SATA og NVMe fra sysfs og pci.ids). Prosessoren har egen side
//! fra 1.1.0 (`prosessor.rs`), og diskene har Disker. Port av
//! `web/js/pages/system.js`, med rettingene fra maskinvare-revisjon kap. 7 og
//! 11: «Buffer og cache» skiller ut `Shmem`, og SPD krever godkjenning.
//!
//! De faste dataene hentes én gang (`get_system`); minne og oppetid
//! oppdateres fra prøvene, bare mens siden vises.

use super::{Key, Page};
use crate::rapport_ui::Sections;
use crate::state::App;
use crate::widgets::bar::Bar;
use crate::widgets::{hbox, vbox};
use gtk::prelude::*;
use rigg_core::{tr, trf};
use rigg_core::report::{self, ReportFormat, Section};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;


struct State {
    app: Rc<App>,
    visible: Cell<bool>,
    system: RefCell<Value>,
    loading: Cell<bool>,
    view: Rc<Sections>,
    /// Stolpene i Minne (Brukt og Swap), lagt inn når panelet bygges.
    mem_bars: RefCell<Vec<(&'static str, Bar)>>,
}

pub struct System {
    root: gtk::Box,
    actions: gtk::Box,
    st: Rc<State>,
}

impl Page for System {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        self.st.visible.set(true);
        if self.st.system.borrow().is_null() {
            self.st.load();
        }
        self.st.refresh();
    }
    fn hide(&self) {
        self.st.visible.set(false);
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.key == gtk::gdk::Key::F5 && !k.ctrl && !k.alt {
            self.st.load();
            return true;
        }
        false
    }
}

impl State {
    fn load(self: &Rc<Self>) {
        if self.loading.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        let first = self.system.borrow().is_null();
        self.app.client.request("get_system", json!({}), move |res| {
            let Some(s) = w.upgrade() else { return };
            s.loading.set(false);
            match res {
                Ok(v) => {
                    *s.system.borrow_mut() = v;
                    s.refresh();
                    if !first {
                        s.app.toast(crate::toast::Toast::new(tr!("Faste data er lest på nytt")).ms(2500));
                    }
                }
                Err(e) => {
                    if !first {
                        s.app.toast(crate::toast::Toast::new(tr!("Kunne ikke hente systeminfo")).body(&e.message).error());
                    }
                }
            }
        });
    }

    fn sections(&self, report: bool) -> Vec<Section> {
        let sys = self.app.sys();
        let system = self.system.borrow();
        let (desktop, session) = crate::rapport_ui::desktop_env();
        let mut v = report::system::sections(&report::system::SysInput { system: &system, sys: sys.as_ref(), show_hostname: !report, desktop, session, report, max_thread_mhz: None });
        v.retain(|s| report::system::SYSTEM.contains(&s.id));
        v
    }

    fn refresh(&self) {
        if !self.visible.get() {
            return;
        }
        let secs = self.sections(false);
        self.view.show(&secs);
        // Stolpene i Minne.
        if let Some(s) = self.app.sys() {
            let m = &s.mem;
            for (id, bar) in self.mem_bars.borrow().iter() {
                match *id {
                    "brukt" => {
                        let f = m.used_kb.zip(m.total_kb).map(|(u, t)| u / t);
                        let lvl = match f {
                            Some(f) if f >= 0.95 => "danger",
                            Some(f) if f >= 0.85 => "warn",
                            _ => "",
                        };
                        bar.set(f, lvl);
                    }
                    _ => {
                        let f = m.swap_used_kb.zip(m.swap_total_kb).filter(|(_, t)| *t > 0.0).map(|(u, t)| u / t);
                        bar.set(f, if f.is_some_and(|f| f >= 0.8) { "warn" } else { "" });
                    }
                }
            }
        }
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "sy-page"]);
    let view = Sections::new();
    root.append(&view.masonry);

    let st = Rc::new(State {
        app: app.clone(),
        visible: Cell::new(false),
        system: RefCell::new(Value::Null),
        loading: Cell::new(false),
        view,
        mem_bars: RefCell::new(vec![]),
    });
    {
        let w = Rc::downgrade(&st);
        *st.view.on_build.borrow_mut() = Some(Box::new(move |id, p| {
            let Some(s) = w.upgrade() else { return };
            if id != "minne" {
                return;
            }
            let mut bars = Vec::new();
            for rid in ["brukt", "swap"] {
                if let Some(r) = p.row(rid) {
                    // Stolpen først og tallet til høyre, som demoens måler.
                    let bar = Bar::new(&s.app, false);
                    bar.set_hexpand(true);
                    bar.set_valign(gtk::Align::Center);
                    r.value.set_hexpand(false);
                    r.value_box.prepend(&bar);
                    r.value_box.add_css_class("meter-row");
                    bars.push((rid, bar));
                }
            }
            *s.mem_bars.borrow_mut() = bars;
        }));
    }
    // Knappene i toppfeltet.
    let actions = hbox(2, &[]);
    actions.append(&crate::rapport_ui::save_button(app));
    let w = Rc::downgrade(&st);
    actions.append(&crate::rapport_ui::copy_button(app, move |fmt: ReportFormat| {
        let s = w.upgrade()?;
        if s.system.borrow().is_null() {
            return None;
        }
        let mut secs = s.sections(false);
        // Vertsnavnet står på siden, men ikke i det som kopieres (som rapporten).
        for sec in &mut secs {
            sec.items.retain(|i| !matches!(i, report::Item::Kv(k) if k.id == "vert"));
        }
        let title = trf!("{app} – System ({dato})", app = rigg_core::version::NAME, dato = crate::rapport_ui::stamp_long(rigg_core::now_ms()));
        Some((report::render_page(&title, &secs, fmt), tr!("uten vertsnavn").to_string()))
    }));

    let r = {
        let w = Rc::downgrade(&st);
        Rc::new(move || {
            if let Some(s) = w.upgrade() {
                s.refresh();
            }
        })
    };
    let r1 = r.clone();
    app.on_telemetry.connect(move |_| r1());
    let r2 = r.clone();
    app.on_settings.connect(move |_| r2());
    {
        let w = Rc::downgrade(&st);
        app.on_conn.connect(move |c| {
            if let Some(s) = w.upgrade()
                && *c == crate::state::Conn::Connected
                && s.visible.get()
                && s.system.borrow().is_null()
            {
                s.load();
            }
        });
    }
    Box::new(System { root, actions, st })
}
