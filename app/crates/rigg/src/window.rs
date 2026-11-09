//! Vinduet og skallet (arkitektur.md 6.2): sidefelt og innhold i en ytre
//! `GtkStack` («app» / «innstillinger»), slik v0.0.15 gjør det med
//! `data-view`. Ingen tittellinje: toppstripene er `GtkWindowHandle` med
//! GNOMEs vinduskontroller, så rammen står stille når visningen byttes.
//!
//! Bare skallet og startsiden bygges ved oppstart. De andre sidene bygges
//! første gang de vises, og Innstillinger første gang den åpnes.

use crate::pages::{self, Key, Page};
use crate::settings::SettingsView;
use crate::sidebar::Sidebar;
use crate::state::App;
use crate::toast::Toasts;
use crate::topbar::Topbar;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------- Rot som ser størrelsen

mod root_imp {
    use super::*;

    #[derive(Default)]
    pub struct RootBin {
        pub on_size: crate::widgets::Slot<dyn Fn(i32, i32)>,
        pub last: std::cell::Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RootBin {
        const NAME: &'static str = "RiggRoot";
        type Type = super::RootBin;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for RootBin {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for RootBin {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            self.obj().first_child().map(|c| c.request_mode()).unwrap_or(gtk::SizeRequestMode::ConstantSize)
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(c) => {
                    let (a, b, _, _) = c.measure(o, for_size);
                    (a, b, -1, -1)
                }
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if self.last.get() != (w, h) {
                self.last.set((w, h));
                if let Some(f) = self.on_size.borrow().as_ref() {
                    f(w, h);
                }
            }
            if let Some(c) = self.obj().first_child() {
                c.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct RootBin(ObjectSubclass<root_imp::RootBin>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl RootBin {
    fn new(child: &impl IsA<gtk::Widget>) -> RootBin {
        let r: RootBin = glib::Object::new();
        child.set_parent(&r);
        r
    }
}

// ---------------------------------------------------------------- Skallet

pub struct Shell {
    app: std::rc::Weak<App>,
    pub window: gtk::ApplicationWindow,
    outer: gtk::Stack,
    pub sidebar: Rc<Sidebar>,
    pub topbar: Topbar,
    pub infobar: Rc<crate::infobar::Infobar>,
    pub toasts: Rc<Toasts>,
    content: gtk::Stack,
    content_wrap: gtk::Box,
    pages: RefCell<HashMap<String, Box<dyn Page>>>,
    settings: RefCell<Option<Rc<SettingsView>>>,
    back: RefCell<Option<String>>,
    booted: std::cell::Cell<bool>,
    width: std::cell::Cell<i32>,
}

impl Shell {
    pub fn new(app: &Rc<App>) -> Rc<Shell> {
        let window = gtk::ApplicationWindow::new(&app.gtk);
        window.set_title(Some(rigg_core::version::NAME));
        window.set_icon_name(Some(&crate::app::icon_name(app)));
        // Ingen tittellinje: toppstripene er dra-flater med vinduskontrollene.
        let empty = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        empty.set_visible(false);
        window.set_titlebar(Some(&empty));
        window.set_size_request(640, 480);
        let geom = app.settings().window.clone();
        window.set_default_size(geom.width, geom.height);
        if geom.maximized {
            window.maximize();
        }
        crate::theme::apply_to(app, &window);

        let sidebar = Sidebar::new(app);
        let topbar = Topbar::new();
        // Knappene til venstre i `gtk-decoration-layout` står i sidefeltets
        // toppfelt, og i innholdets toppfelt når sidefeltet er en ikonlist.
        topbar.set_start_here(sidebar.is_rail());
        {
            let start = topbar.start_setter();
            *sidebar.on_rail.borrow_mut() = Some(Box::new(start));
        }
        let infobar = crate::infobar::Infobar::new(app);
        let toasts = Toasts::new(app);
        let content = gtk::Stack::new();
        content.set_vexpand(true);
        content.set_hexpand(true);
        content.set_transition_type(gtk::StackTransitionType::None);
        // Ikke homogen: siden som vises, bestemmer minste bredde, ikke den
        // bredeste siden som noen gang er bygd (Innstillinger også).
        content.set_hhomogeneous(false);
        content.set_vhomogeneous(false);
        let content_wrap = crate::widgets::vbox(0, &["content"]);
        content_wrap.append(&content);
        content_wrap.set_vexpand(true);

        let main = crate::widgets::vbox(0, &[]);
        main.set_hexpand(true);
        main.append(&topbar.handle);
        main.append(&infobar.root);
        main.append(&content_wrap);
        let app_view = crate::widgets::hbox(0, &[]);
        app_view.append(&sidebar.column);
        app_view.append(&main);

        let outer = gtk::Stack::new();
        outer.set_transition_type(gtk::StackTransitionType::None);
        outer.set_hhomogeneous(false);
        outer.set_vhomogeneous(false);
        outer.add_named(&app_view, Some("app"));
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&outer));
        overlay.add_overlay(&toasts.root);
        let root = RootBin::new(&overlay);
        window.set_child(Some(&root));

        let shell = Rc::new(Shell {
            app: Rc::downgrade(app),
            window,
            outer,
            sidebar,
            topbar,
            infobar,
            toasts,
            content,
            content_wrap,
            pages: RefCell::new(HashMap::new()),
            settings: RefCell::new(None),
            back: RefCell::new(None),
            booted: std::cell::Cell::new(false),
            width: std::cell::Cell::new(0),
        });
        {
            let me = Rc::downgrade(&shell);
            *root.imp().on_size.borrow_mut() = Some(Box::new(move |w, h| {
                if let Some(s) = me.upgrade() {
                    s.on_size(w, h);
                }
            }));
        }
        shell.wire(app);
        shell
    }

    fn app(&self) -> Rc<App> {
        self.app.upgrade().expect("appen lever så lenge vinduet gjør")
    }

    fn wire(self: &Rc<Self>, app: &Rc<App>) {
        crate::shortcuts::attach(app, &self.window);
        // Tastaturrunden (runde 4B): `RIGG_FOKUS=1` i utviklingsprofilen skriver
        // hver fokusendring, så Tab-rekkefølgen kan leses av en test.
        if app.profile.is_dev() && std::env::var_os("RIGG_FOKUS").is_some() {
            self.window.connect_focus_widget_notify(|w| {
                if let Some(f) = gtk::prelude::GtkWindowExt::focus(w) {
                    eprintln!("RIGG_FOKUS {}", crate::shortcuts::describe(&f));
                }
            });
        }
        // Lukk: lagre vindusstørrelsen og innstillingene (noen byte), så avslutt.
        let a = app.clone();
        self.window.connect_close_request(move |w| {
            crate::timing::mark("close_request");
            let (width, height) = w.default_size();
            let max = w.is_maximized();
            let mut geom = a.settings().window.clone();
            if !max {
                geom.width = width.max(640);
                geom.height = height.max(480);
            }
            geom.maximized = max;
            a.settings.borrow_mut().window = geom;
            // Vinduet forsvinner først; lagringen (fsync) skjer etterpå, så
            // lukkeknappen aldri venter på disken (runde 3C: opptil 70 ms).
            w.set_visible(false);
            crate::timing::mark("hidden");
            crate::prefs::save_now(&a);
            glib::Propagation::Proceed
        });
        // Synlig eller ikke: minimert vindu gir sakte prøver og ingen tegning.
        let me = Rc::downgrade(self);
        self.window.connect_realize(move |w| {
            let Some(surface) = w.surface() else { return };
            let Ok(top) = surface.downcast::<gdk::Toplevel>() else { return };
            let me = me.clone();
            top.connect_state_notify(move |t| {
                if let Some(s) = me.upgrade() {
                    let hidden = t.state().intersects(gdk::ToplevelState::MINIMIZED | gdk::ToplevelState::SUSPENDED);
                    s.send_focus(hidden);
                }
            });
        });
        let me = Rc::downgrade(self);
        app.on_conn.connect(move |_| {
            if let Some(s) = me.upgrade() {
                let a = s.app();
                crate::widgets::class(&s.content_wrap, "stale", !a.client.is_connected() && a.telemetry().is_some());
            }
        });
    }

    fn on_size(&self, w: i32, h: i32) {
        self.width.set(w);
        self.sidebar.set_narrow(w < 860);
        crate::logg_ui::set_compact(w < 900);
        self.sidebar.set_height(h);
        if let Some(s) = self.settings.borrow().as_ref() {
            s.set_width(w);
        }
    }

    /// Sier til tjenesten hvilke sider som synes (`[]` når vinduet er minimert).
    pub fn send_focus(&self, hidden: bool) {
        let app = self.app();
        let pages: Vec<String> = if hidden { vec![] } else { vec![app.page.borrow().clone()] };
        app.client.request("set_focus", serde_json::json!({"pages": pages}), |_| {});
    }

    /// Går til en side. Siden bygges første gang den vises.
    pub fn go(&self, id: &str) {
        let app = self.app();
        let (id, query) = match id.split_once('?') {
            Some((a, b)) => (a, Some(b)),
            None => (id, None),
        };
        let Some(def) = pages::def(id) else { return };
        if self.outer.visible_child_name().as_deref() == Some("innstillinger") {
            self.leave_settings();
        }
        let cur = app.page.borrow().clone();
        if cur == id {
            if let Some(p) = self.pages.borrow().get(id) {
                p.show(query);
            }
            return;
        }
        if let Some(p) = self.pages.borrow().get(&cur) {
            p.hide();
        }
        if !self.pages.borrow().contains_key(id) {
            let p = (def.build)(&app);
            let w = p.widget();
            let child: gtk::Widget = if p.scrolls() {
                let sc = gtk::ScrolledWindow::new();
                sc.set_hscrollbar_policy(gtk::PolicyType::Never);
                sc.set_child(Some(&w));
                sc.upcast()
            } else {
                w
            };
            self.content.add_named(&child, Some(id));
            if let Some(a) = p.actions() {
                self.topbar.add_actions(id, &a);
            }
            self.pages.borrow_mut().insert(id.to_string(), p);
        }
        self.content.set_visible_child_name(id);
        if self.booted.get()
            && let Some(c) = self.content.visible_child()
        {
            crate::motion::fade_in(&app, &c, 3, 0.0);
        }
        *app.page.borrow_mut() = id.to_string();
        self.topbar.show(id, def.title);
        self.sidebar.mark_current(id);
        self.window.set_title(Some(&format!("{} · {}", crate::pages::full_title(id), rigg_core::version::NAME)));
        if let Some(p) = self.pages.borrow().get(id) {
            p.show(query);
        }
        app.on_page.emit(&id.to_string());
        self.send_focus(false);
        if app.settings().last_page != id {
            app.settings.borrow_mut().last_page = id.to_string();
            crate::prefs::schedule_save(&app);
        }
        self.booted.set(true);
    }

    /// Siden som vises får tastetrykket først.
    pub fn page_key(&self, k: &Key) -> bool {
        let app = self.app();
        if self.in_settings() {
            return self.settings.borrow().as_ref().is_some_and(|s| s.on_key(k));
        }
        let cur = app.page.borrow().clone();
        self.pages.borrow().get(&cur).is_some_and(|p| p.on_key(k))
    }

    pub fn in_settings(&self) -> bool {
        self.outer.visible_child_name().as_deref() == Some("innstillinger")
    }

    /// Åpner Innstillinger (hele vinduet). `section`: delen som vises.
    pub fn open_settings(&self, section: Option<&str>) {
        let app = self.app();
        if self.settings.borrow().is_none() {
            let v = SettingsView::new(&app);
            self.outer.add_named(&v.root, Some("innstillinger"));
            v.set_width(self.width.get());
            *self.settings.borrow_mut() = Some(v);
        }
        let v = self.settings.borrow().clone().unwrap();
        if !self.in_settings() {
            let from = app.page.borrow().clone();
            if let Some(p) = self.pages.borrow().get(&from) {
                p.hide();
            }
            *self.back.borrow_mut() = Some(from.clone());
            v.set_back(pages::def(&from).map(|d| d.title).unwrap_or("Oversikt"));
            self.outer.set_visible_child_name("innstillinger");
            crate::motion::fade_in(&app, &v.root, 4, 0.0);
            *app.page.borrow_mut() = "innstillinger".into();
            self.sidebar.mark_current("innstillinger");
            self.window.set_title(Some(&format!("Innstillinger · {}", rigg_core::version::NAME)));
            app.on_page.emit(&"innstillinger".to_string());
        }
        v.show(section);
    }

    fn leave_settings(&self) {
        let app = self.app();
        if let Some(v) = self.settings.borrow().as_ref() {
            v.hide();
        }
        self.outer.set_visible_child_name("app");
        crate::motion::fade_in(&app, &self.outer, 4, 0.0);
        // Siden vi kom fra er fortsatt bygd; «go» markerer den på nytt.
        app.page.borrow_mut().clear();
    }

    /// Tilbake fra Innstillinger (knappen, Esc og Ctrl+,): siden du kom fra.
    pub fn close_settings(&self) {
        if !self.in_settings() {
            return;
        }
        let to = self.back.borrow_mut().take().unwrap_or_else(|| self.app().settings().start());
        self.leave_settings();
        self.go(&to);
        self.sidebar.settings_focus();
    }

    pub fn toggle_settings(&self) {
        if self.in_settings() {
            self.close_settings();
        } else {
            self.open_settings(None);
        }
    }
}
