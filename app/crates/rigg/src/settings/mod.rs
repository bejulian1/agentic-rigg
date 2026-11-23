//! Innstillinger (v0.0.15 i demoen): dekker hele vinduet, med egen
//! navigasjon til venstre (tilbake, tittel, søk og delene) og panelene i én
//! til tre spalter. Alt trer i kraft straks og lagres i `ui.json` uten varsel.
//! Søket leter i alle delene og viser radene som passer, med kontrollene i
//! behold. Tilbake på tre måter: knappen, Esc og Ctrl+, (og sidefeltets
//! tannhjul), til siden du kom fra.
//!
//! **Slik lager en del seg (2A: `grafer.rs`, 2B: `logging.rs`, 3B: gruppa
//! «Bakgrunn» i `generelt.rs`):** `pub fn build(app: &Rc<App>) -> Vec<Group>`
//! med byggeklossene her: [`group`], [`row`], [`row_wrap`], [`switch_row`],
//! [`seg`]. Søket finner radene av seg selv (tittel, forklaring og `k`).
//! Trenger delen å tegnes på nytt etter en endring, kall [`request_render`].
//! Tekst som endres mens delen vises (status for tjenesten), registreres med
//! [`live`].

pub mod data;
pub mod format;
pub mod generelt;
pub mod grafer;
pub mod logging;
pub mod om;
pub mod search;
pub mod sidefelt;
pub mod styring;
mod sprak;
pub mod utseende;

use crate::state::App;
use crate::widgets::masonry::Masonry;
use crate::widgets::{Icon, Seg, hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::{tr, trf};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub struct SectionDef {
    pub id: &'static str,
    pub name: fn() -> &'static str,
    pub icon: &'static str,
    pub lead: fn() -> &'static str,
    pub build: fn(&Rc<App>) -> Vec<Group>,
}

pub const SECTIONS: [SectionDef; 9] = [
    SectionDef { id: "generelt", name: || tr!("Generelt"), icon: "settings", lead: || tr!("Språk, oppstart, oppdatering og bakgrunnstjeneste"), build: generelt::build },
    SectionDef { id: "utseende", name: || tr!("Utseende"), icon: "sun-moon", lead: || tr!("Lyst eller mørkt, farger, appikon, størrelse og bevegelse"), build: utseende::build },
    SectionDef { id: "format", name: || tr!("Enheter og format"), icon: "ruler", lead: || tr!("Enheter, desimaler og klokkeslett"), build: format::build },
    SectionDef { id: "sidefelt", name: || tr!("Sidefelt"), icon: "panel-left", lead: || tr!("Statusblokken, bredden og sidene i sidefeltet"), build: sidefelt::build },
    SectionDef { id: "grafer", name: || tr!("Grafer og sensorer"), icon: "chart-area", lead: || tr!("Tidsvindu, grafer, sensorer og varselfarger"), build: grafer::build },
    SectionDef { id: "logging", name: || tr!("Logging og varsler"), icon: "activity", lead: || tr!("Logg til fil og skrivebordsvarsler"), build: logging::build },
    SectionDef { id: "styring", name: || tr!("Skjermkortstyring"), icon: "shield", lead: || tr!("Tillatelse, funksjonsstøtte og trygg gjenoppretting"), build: styring::build },
    SectionDef { id: "data", name: || tr!("Data og tilbakestilling"), icon: "database", lead: || tr!("Mappene, eksport, import og tilbakestilling"), build: data::build },
    SectionDef { id: "om", name: || tr!("Om"), icon: "info", lead: || tr!("Versjon, hjelp og feilsøking"), build: om::build },
];

// ---------------------------------------------------------------- Byggeklossene

/// En rad i et panel. `text` er det søket leter i.
pub struct Row {
    pub widget: gtk::Widget,
    pub text: String,
    pub titles: Vec<gtk::Label>,
}

/// Et panel. `whole`: søket viser hele panelet eller ingenting (lister og rutenett).
pub struct Group {
    pub widget: gtk::Box,
    pub head: gtk::Box,
    pub title: String,
    pub title_label: gtk::Label,
    pub text: String,
    pub rows: Vec<Row>,
    pub whole: bool,
}

#[derive(Default)]
pub struct GroupOpts<'a> {
    pub desc: Option<&'a str>,
    pub note: Option<&'a str>,
    pub aside: Option<gtk::Widget>,
    pub whole: bool,
    pub k: &'a str,
}

fn texts_in(w: &gtk::Widget, out: &mut Vec<String>) {
    if w.is::<gtk::DropDown>() {
        return; // valgene i en nedtrekksliste er ikke med i søket
    }
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        out.push(l.text().to_string());
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        texts_in(&ch, out);
        c = ch.next_sibling();
    }
}

fn text_box(title: &str, subtitle: Option<&str>) -> (gtk::Box, gtk::Label) {
    let t = vbox(0, &["text"]);
    t.set_hexpand(true);
    t.set_valign(gtk::Align::Center);
    let tl = label(title, &["title"]);
    tl.set_wrap(true);
    t.append(&tl);
    if let Some(s) = subtitle {
        let sl = label(s, &["subtitle"]);
        sl.set_wrap(true);
        sl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        t.append(&sl);
    }
    (t, tl)
}

fn make_row(title: &str, subtitle: Option<&str>, ctrl: &gtk::Widget, k: &str, wrap: bool) -> Row {
    let (t, tl) = text_box(title, subtitle);
    let c = hbox(8, &["ctrl"]);
    c.set_valign(gtk::Align::Center);
    if wrap {
        c.set_halign(gtk::Align::Fill);
    }
    c.append(ctrl);
    // Kontrollen legger seg under teksten når raden blir for trang (setrow.rs).
    let r = crate::widgets::setrow::SetRow::new(&t, &c, wrap);
    r.add_css_class("set-row");
    if wrap {
        r.add_css_class("wrap");
    }
    // Forklaringen kobles til kontrollen for skjermlesere.
    ctrl.update_property(&[gtk::accessible::Property::Description(subtitle.unwrap_or(""))]);
    let mut tx = vec![title.to_string(), subtitle.unwrap_or("").to_string(), k.to_string()];
    texts_in(ctrl, &mut tx);
    Row { widget: r.upcast(), text: tx.join(" "), titles: vec![tl] }
}

/// Kombinerer oversatte søkeord med de norske, så søk virker på begge språk.
pub fn kw(nb: &'static str, localized: &'static str) -> Cow<'static, str> {
    if nb == localized || nb.is_empty() {
        Cow::Borrowed(localized)
    } else {
        Cow::Owned(format!("{localized} {nb}"))
    }
}

/// Rad med tekst til venstre og kontrollen til høyre.
pub fn row(title: &str, subtitle: Option<&str>, ctrl: &impl IsA<gtk::Widget>, k: &str) -> Row {
    make_row(title, subtitle, ctrl.upcast_ref(), k, false)
}

/// Rad med kontrollen under teksten (miniatyrer, fargeprøver, lister).
pub fn row_wrap(title: &str, subtitle: Option<&str>, ctrl: &impl IsA<gtk::Widget>, k: &str) -> Row {
    make_row(title, subtitle, ctrl.upcast_ref(), k, true)
}

/// Rad med bryter; hele raden kan klikkes.
pub fn switch_row(title: &str, subtitle: Option<&str>, active: bool, k: &str, on: impl Fn(bool) + 'static) -> Row {
    let sw = crate::widgets::switchrow::switch(active, title, on);
    let r = make_row(title, subtitle, sw.upcast_ref(), k, false);
    crate::widgets::switchrow::make_row_toggle(&r.widget, &sw);
    r
}

/// En rad som allerede er bygd (`text`: det søket leter i).
pub fn custom_row(w: &impl IsA<gtk::Widget>, text: &str) -> Row {
    Row { widget: w.clone().upcast(), text: text.to_string(), titles: vec![] }
}

/// Segmentknapp som lagrer `key` med verdien (tall når `num`).
pub fn seg(app: &Rc<App>, opts: &[(&str, &str)], value: &str, key: &'static str, label: &str, num: bool) -> Seg {
    let s = Seg::new(opts, value, label);
    let a = app.clone();
    s.connect(move |v| {
        let val = if num { v.parse::<u64>().map(serde_json::Value::from).unwrap_or(serde_json::Value::Null) } else { serde_json::Value::from(v) };
        save(&a, serde_json::json!({ key: val }));
    });
    s
}

pub fn group(title: &str, opts: GroupOpts, rows: Vec<Row>) -> Group {
    let w = vbox(0, &["group", "set-group"]);
    let head = hbox(8, &["group-head"]);
    let tl = label(title, &["group-title"]);
    tl.set_accessible_role(gtk::AccessibleRole::Heading);
    head.append(&tl);
    if let Some(a) = &opts.aside {
        let ab = hbox(8, &["aside"]);
        ab.set_hexpand(true);
        ab.set_halign(gtk::Align::End);
        ab.append(a);
        head.append(&ab);
    }
    w.append(&head);
    if let Some(d) = opts.desc {
        let dl = label(d, &["set-desc"]);
        dl.set_wrap(true);
        w.append(&dl);
    }
    let n = rows.len();
    for (i, r) in rows.iter().enumerate() {
        if i + 1 == n {
            r.widget.add_css_class("last");
        }
        w.append(&r.widget);
    }
    if let Some(nt) = opts.note {
        let nl = label(nt, &["set-note"]);
        nl.set_wrap(true);
        w.append(&nl);
    }
    w.update_property(&[gtk::accessible::Property::Label(title)]);
    let text = format!("{title} {} {}", opts.desc.unwrap_or(""), opts.k);
    Group { widget: w, head, title: title.to_string(), title_label: tl, text, rows, whole: opts.whole }
}

// ---------------------------------------------------------------- Lagring og ny tegning

/// En tekst som oppdateres mens Innstillinger vises.
type Live = (gtk::glib::WeakRef<gtk::Label>, Box<dyn Fn(&App) -> String>);

thread_local! {
    static VIEW: RefCell<Weak<SettingsView>> = const { RefCell::new(Weak::new()) };
    static SELF_CHANGE: Cell<bool> = const { Cell::new(false) };
    static LIVE: RefCell<Vec<Live>> = RefCell::new(Vec::new());
}

/// Lagrer fra Innstillinger. Egne endringer tegner ikke delen på nytt (så
/// fokus og rulling står), andre gjør.
pub fn save(app: &Rc<App>, patch: serde_json::Value) -> bool {
    SELF_CHANGE.with(|s| s.set(true));
    let ok = app.set(patch);
    SELF_CHANGE.with(|s| s.set(false));
    ok
}

/// Tegner delen som vises på nytt (etter at egne endringer har endret innholdet).
pub fn request_render() {
    gtk::glib::idle_add_local_once(|| {
        if let Some(v) = VIEW.with(|v| v.borrow().upgrade()) {
            v.render(false);
        }
    });
}

/// Setter søket (skjermbildene og testene).
pub fn set_query(q: &str) {
    if let Some(v) = VIEW.with(|v| v.borrow().upgrade()) {
        v.search.set_text(q);
    }
}

/// En etikett som oppdateres når tjenesten, prøvene eller innstillingene endres.
pub fn live(l: &gtk::Label, f: impl Fn(&App) -> String + 'static) {
    LIVE.with(|v| v.borrow_mut().push((l.downgrade(), Box::new(f))));
}

fn update_live(app: &App) {
    LIVE.with(|v| {
        v.borrow_mut().retain(|(w, f)| match w.upgrade() {
            Some(l) => {
                let t = f(app);
                if l.text() != t {
                    l.set_text(&t);
                }
                true
            }
            None => false,
        })
    });
}

// ---------------------------------------------------------------- Visningen

pub struct SettingsView {
    app: Rc<App>,
    pub root: gtk::Box,
    side: gtk::Box,
    side_fixed: crate::widgets::fixed::Fixed,
    side_top: gtk::Box,
    search: gtk::SearchEntry,
    slash: gtk::Label,
    nav: gtk::Box,
    tabs: Vec<(&'static str, gtk::Button, gtk::Label, gtk::Label)>,
    saved_line: gtk::Box,
    heading: gtk::Label,
    lead: gtk::Label,
    count: gtk::Label,
    body: gtk::Box,
    scroll: gtk::ScrolledWindow,
    masonry: Masonry,
    empty: gtk::Box,
    back: gtk::Button,
    section: RefCell<String>,
    query: RefCell<String>,
    top: Cell<bool>,
    visible: Cell<bool>,
}

impl SettingsView {
    pub fn new(app: &Rc<App>) -> Rc<SettingsView> {
        let root = hbox(0, &["settings"]);
        // ---- Navigasjonen ----
        let side = vbox(0, &["set-side"]);
        let side_head = hbox(4, &["set-side-head"]);
        let back = crate::widgets::button(Some("arrow-left"), None, &["flat", "icon-only", "btn-back"]);
        side_head.append(&back);
        let title = label(tr!("Innstillinger"), &["set-title"]);
        title.set_accessible_role(gtk::AccessibleRole::Heading);
        side_head.append(&title);
        let controls_start = crate::topbar::controls(gtk::PackType::Start, Rc::new(Cell::new(true)));
        side_head.prepend(&controls_start);
        let sh = gtk::WindowHandle::new();
        sh.set_child(Some(&side_head));
        let find = hbox(0, &["set-find"]);
        let search = gtk::SearchEntry::new();
        search.add_css_class("set-search");
        search.set_hexpand(true);
        search.set_placeholder_text(Some(tr!("Søk i innstillinger")));
        search.set_tooltip_text(Some(tr!("Søk i alle innstillingene. Ctrl+F eller / søker, Esc avslutter søket.")));
        search.update_property(&[gtk::accessible::Property::Label(tr!("Søk i innstillinger")), gtk::accessible::Property::KeyShortcuts("Control+F /")]);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&search));
        overlay.set_hexpand(true);
        let slash = label("/", &["keycap", "slash"]);
        slash.set_halign(gtk::Align::End);
        slash.set_valign(gtk::Align::Center);
        slash.set_can_target(false);
        slash.set_margin_end(6);
        overlay.add_overlay(&slash);
        find.append(&overlay);
        let side_top = vbox(0, &[]);
        side_top.append(&sh);
        side_top.append(&find);
        side.append(&side_top);
        let nav = vbox(1, &["set-nav"]);
        nav.set_vexpand(true);
        nav.set_accessible_role(gtk::AccessibleRole::TabList);
        nav.update_property(&[gtk::accessible::Property::Label(tr!("Deler av innstillingene"))]);
        let mut tabs = Vec::new();
        for s in SECTIONS.iter() {
            let b = gtk::Button::new();
            b.add_css_class("set-tab");
            b.set_accessible_role(gtk::AccessibleRole::Tab);
            let bx = hbox(10, &[]);
            bx.append(&Icon::new(s.icon, 16));
            let nm = label((s.name)(), &["nm"]);
            nm.set_hexpand(true);
            nm.set_ellipsize(gtk::pango::EllipsizeMode::End);
            // Et kuttet navn («Protokoll und Benachrichtigungen») står helt i verktøytipset.
            nm.set_has_tooltip(true);
            nm.connect_query_tooltip(|l, _, _, _, tip| {
                let cut = l.layout().is_ellipsized();
                if cut {
                    tip.set_text(Some(&l.text()));
                }
                cut
            });
            bx.append(&nm);
            let n = label("", &["set-n", "num"]);
            n.set_xalign(1.0);
            n.set_visible(false);
            bx.append(&n);
            b.set_child(Some(&bx));
            b.update_property(&[gtk::accessible::Property::Label((s.name)())]);
            nav.append(&b);
            tabs.push((s.id, b, nm, n));
        }
        side.append(&nav);
        let foot = vbox(8, &["set-side-foot"]);
        let saved_line = hbox(8, &[]);
        let saved_icon = Icon::new("check", 14);
        saved_icon.set_valign(gtk::Align::Center);
        saved_line.append(&saved_icon);
        let saved_label = label(tr!("Endringer lagres straks."), &[]);
        saved_label.set_wrap(true);
        saved_label.set_xalign(0.0);
        saved_label.set_hexpand(true);
        saved_line.append(&saved_label);
        foot.append(&saved_line);
        let version_log = crate::widgets::button(Some("rotate-ccw"), Some(tr!("Versjonslogg")), &["flat"]);
        version_log.set_halign(gtk::Align::Start);
        {
            let a = app.clone();
            version_log.connect_clicked(move |_| crate::widgets::version_log::show(&a));
        }
        foot.append(&version_log);
        side.append(&foot);
        let side_fixed = crate::widgets::fixed::Fixed::new(&side, crate::sidebar::SIDE_W);
        root.append(&side_fixed);

        // ---- Innholdet ----
        let main = vbox(0, &["set-main"]);
        main.set_hexpand(true);
        let head = hbox(12, &["set-head"]);
        let heading = label("", &["set-heading"]);
        heading.set_accessible_role(gtk::AccessibleRole::Heading);
        head.append(&heading);
        let lead = label("", &["set-lead"]);
        lead.set_hexpand(true);
        lead.set_ellipsize(gtk::pango::EllipsizeMode::End);
        head.append(&lead);
        // Fyller raden når ingressen er skjult i smale vinduer, så
        // vindusknappene står i høyre kant (før sto de rett etter
        // overskriften). Fast hexpand: å bytte den mens vinduet legges ut,
        // slo ikke alltid igjennom.
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        head.append(&spacer);
        let count = label("", &["set-count"]);
        head.append(&count);
        head.append(&crate::topbar::controls(gtk::PackType::End, Rc::new(Cell::new(true))));
        let hh = gtk::WindowHandle::new();
        hh.set_child(Some(&head));
        main.append(&hh);
        let body = vbox(0, &["set-body"]);
        let masonry = Masonry::new();
        masonry.set_valign(gtk::Align::Start);
        body.append(&masonry);
        let empty = hbox(12, &["set-empty"]);
        empty.set_visible(false);
        body.append(&empty);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&body));
        main.append(&scroll);
        root.append(&main);

        let section = if SECTIONS.iter().any(|s| s.id == app.settings().settings_section) { app.settings().settings_section.clone() } else { "generelt".into() };
        let v = Rc::new(SettingsView {
            app: app.clone(),
            root,
            side,
            side_fixed,
            side_top,
            search,
            slash,
            nav,
            tabs,
            saved_line,
            heading,
            lead,
            count,
            body,
            scroll,
            masonry,
            empty,
            back,
            section: RefCell::new(section),
            query: RefCell::new(String::new()),
            top: Cell::new(false),
            visible: Cell::new(false),
        });
        VIEW.with(|w| *w.borrow_mut() = Rc::downgrade(&v));
        v.wire();
        v
    }

    fn wire(self: &Rc<Self>) {
        // Et klikk på tekst eller tomrom flytter ikke GTK-fokus av seg selv.
        // Slipp søkefokuset før målet behandler klikket, uten å spise hendelsen.
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        click.connect_pressed(move |_, _, x, y| {
            let Some(v) = me.upgrade() else { return };
            let inside = v.root.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == v.search.clone().upcast::<gtk::Widget>() || w.is_ancestor(&v.search));
            if !inside {
                v.release_search_focus();
            }
        });
        self.root.add_controller(click);
        for (id, b, _, _) in &self.tabs {
            let me = Rc::downgrade(self);
            let id = *id;
            b.connect_clicked(move |_| {
                if let Some(v) = me.upgrade() {
                    v.select(id, None);
                }
            });
        }
        // Piltastene i navigasjonen.
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(v) = me.upgrade() else { return gtk::glib::Propagation::Proceed };
            use gtk::gdk::Key as K;
            let ids: Vec<&str> = SECTIONS.iter().map(|s| s.id).collect();
            let i = ids.iter().position(|x| *x == v.section.borrow().as_str()).unwrap_or(0) as i32;
            let n = ids.len() as i32;
            let j = match key {
                K::Down | K::Right => (i + 1).rem_euclid(n),
                K::Up | K::Left => (i - 1).rem_euclid(n),
                K::Home => 0,
                K::End => n - 1,
                _ => return gtk::glib::Propagation::Proceed,
            };
            v.select(ids[j as usize], None);
            v.tabs[j as usize].1.grab_focus();
            gtk::glib::Propagation::Stop
        });
        self.nav.add_controller(keys);
        {
            let app = self.app.clone();
            self.back.connect_clicked(move |_| app.shell().close_settings());
        }
        {
            // Kryssknappen og sletting til tomt felt oppdaterer treffene straks.
            let me = Rc::downgrade(self);
            self.search.connect_changed(move |e| {
                let Some(v) = me.upgrade() else { return };
                v.slash.set_visible(e.text().is_empty() && !e.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN) && !v.top.get());
                if e.text().trim().is_empty() && !v.query.borrow().is_empty() {
                    v.query.borrow_mut().clear();
                    if v.visible.get() {
                        v.render(true);
                    }
                }
            });
        }
        {
            let me = Rc::downgrade(self);
            self.search.connect_search_changed(move |e| {
                let Some(v) = me.upgrade() else { return };
                v.slash.set_visible(e.text().is_empty() && !e.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN) && !v.top.get());
                let q = e.text().trim().to_string();
                if q == *v.query.borrow() {
                    return;
                }
                *v.query.borrow_mut() = q;
                if v.visible.get() {
                    v.render(true);
                }
            });
        }
        {
            let me = Rc::downgrade(self);
            self.search.connect_stop_search(move |_| {
                if let Some(v) = me.upgrade() {
                    v.finish_search();
                }
            });
        }
        {
            let me = Rc::downgrade(self);
            let fc = gtk::EventControllerFocus::new();
            fc.connect_enter(move |_| {
                if let Some(v) = me.upgrade() {
                    v.slash.set_visible(false);
                }
            });
            let me = Rc::downgrade(self);
            fc.connect_leave(move |_| {
                if let Some(v) = me.upgrade() {
                    v.slash.set_visible(v.search.text().is_empty() && !v.top.get());
                }
            });
            self.search.add_controller(fc);
        }
        {
            // Enter eller pil ned i søkefeltet: til første kontroll i treffene.
            let me = Rc::downgrade(self);
            self.search.connect_activate(move |_| {
                if let Some(v) = me.upgrade() {
                    v.masonry.child_focus(gtk::DirectionType::TabForward);
                }
            });
        }
        let me = Rc::downgrade(self);
        self.app.on_settings.connect(move |_| {
            let Some(v) = me.upgrade() else { return };
            if v.visible.get() && !SELF_CHANGE.with(|s| s.get()) {
                v.render(false);
            }
            update_live(&v.app);
        });
        let me = Rc::downgrade(self);
        self.app.on_service.connect(move |_| {
            if let Some(v) = me.upgrade()
                && v.visible.get()
            {
                update_live(&v.app);
            }
        });
        let me = Rc::downgrade(self);
        self.app.on_conn.connect(move |_| {
            if let Some(v) = me.upgrade()
                && v.visible.get()
            {
                update_live(&v.app);
            }
        });
        let me = Rc::downgrade(self);
        self.app.on_devices.connect(move |_| {
            if let Some(v) = me.upgrade()
                && v.visible.get()
            {
                update_live(&v.app);
            }
        });
    }

    pub fn set_back(&self, page_title: &str) {
        let t = trf!("Tilbake til {side}", side = page_title);
        crate::widgets::tip(&self.back, &t, Some("Esc"));
        self.back.update_property(&[gtk::accessible::Property::Label(&t)]);
    }

    /// Smalt vindu (under 700 px): delene som ikoner øverst. Litt smalere
    /// navigasjon mellom 700 og 860 px.
    pub fn set_width(&self, w: i32) {
        if w <= 0 {
            return;
        }
        let top = w < 700;
        self.side_fixed.set_width(if top { 0 } else if w < 860 { 220 } else { crate::sidebar::SIDE_W });
        if self.top.replace(top) == top {
            return;
        }
        self.root.set_orientation(if top { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
        self.side_top.set_orientation(if top { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
        crate::widgets::class(&self.side, "top", top);
        self.nav.set_orientation(if top { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
        self.nav.set_vexpand(!top);
        self.nav.set_homogeneous(top);
        for (id, b, nm, _) in &self.tabs {
            nm.set_visible(!top);
            let name = SECTIONS.iter().find(|s| s.id == *id).map(|s| (s.name)()).unwrap_or("");
            b.set_tooltip_text(if top { Some(name) } else { None });
        }
        self.saved_line.set_visible(!top);
        self.lead.set_visible(!top);
        self.slash.set_visible(!top && self.search.text().is_empty());
        self.search.set_placeholder_text(Some(if top { tr!("Søk") } else { tr!("Søk i innstillinger") }));
        crate::widgets::class(&self.body, "narrow", top);
    }

    pub fn show(self: &Rc<Self>, section: Option<&str>) {
        self.visible.set(true);
        if let Some(s) = section.filter(|s| SECTIONS.iter().any(|x| x.id == *s)) {
            *self.section.borrow_mut() = s.to_string();
            self.clear_search();
        }
        crate::app::ping(&self.app);
        self.render(true);
        let cur = self.section.borrow().clone();
        if let Some((_, b, _, _)) = self.tabs.iter().find(|t| t.0 == cur) {
            b.grab_focus();
        }
    }

    pub fn hide(&self) {
        self.visible.set(false);
        self.clear_search();
    }

    /// Tøm både teksten og det forsinkede søket før en ny del åpnes.
    fn clear_search(&self) -> bool {
        let active = !self.search.text().is_empty() || !self.query.borrow().is_empty();
        self.query.borrow_mut().clear();
        if !self.search.text().is_empty() {
            self.search.set_text("");
        }
        active
    }

    fn release_search_focus(&self) {
        if self.search.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN)
            && let Some(win) = self.app.window()
        {
            gtk::prelude::GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
        }
    }

    fn finish_search(self: &Rc<Self>) {
        self.clear_search();
        self.release_search_focus();
        if self.visible.get() {
            self.render(true);
        }
    }

    fn select(self: &Rc<Self>, id: &str, flash_group: Option<&str>) {
        let from_search = self.clear_search();
        self.release_search_focus();
        let same = *self.section.borrow() == id;
        *self.section.borrow_mut() = id.to_string();
        if self.app.settings().settings_section != id {
            self.app.settings.borrow_mut().settings_section = id.to_string();
            crate::prefs::schedule_save(&self.app);
        }
        self.render(!same || from_search);
        if let Some(title) = flash_group {
            self.flash(title);
        }
    }

    /// Fra et søketreff: panelet lyser kort opp, og første kontroll får fokus.
    fn flash(&self, title: &str) {
        let mut c = self.masonry.first_child();
        while let Some(w) = c {
            c = w.next_sibling();
            let t = w.first_child().and_then(|h| h.first_child()).and_then(|l| l.downcast::<gtk::Label>().ok()).map(|l| l.text().to_string());
            if t.as_deref() == Some(title) {
                w.add_css_class("set-flash");
                w.child_focus(gtk::DirectionType::TabForward);
                let w2 = w.clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(1000), move || w2.remove_css_class("set-flash"));
            }
        }
    }

    /// Tegner delen (eller treffene). `to_top`: rull til toppen.
    pub fn render(self: &Rc<Self>, to_top: bool) {
        LIVE.with(|v| v.borrow_mut().clear());
        self.masonry.clear();
        let q = self.query.borrow().clone();
        let cur = self.section.borrow().clone();
        for (id, b, _, n) in &self.tabs {
            let on = *id == cur && q.is_empty();
            crate::widgets::class(b, "current", on);
            if on {
                b.update_state(&[gtk::accessible::State::Selected(Some(true))]);
            } else {
                b.update_state(&[gtk::accessible::State::Selected(Some(false))]);
            }
            n.set_text("");
            n.set_visible(false);
            b.remove_css_class("no-hits");
        }
        crate::widgets::class(&self.nav, "searching", !q.is_empty());
        self.empty.set_visible(false);
        self.masonry.set_visible(true);
        let gap = match self.app.settings().density.as_str() {
            "compact" => 10,
            "comfortable" => 16,
            _ => 12,
        };
        self.masonry.set_gap(gap);
        if q.is_empty() {
            let def = SECTIONS.iter().find(|s| s.id == cur).unwrap_or(&SECTIONS[0]);
            self.heading.set_text((def.name)());
            self.lead.set_text((def.lead)());
            self.count.set_text("");
            let groups = (def.build)(&self.app);
            for g in groups {
                self.masonry.append(&g.widget);
            }
        } else {
            search::render(self, &q);
        }
        update_live(&self.app);
        if to_top {
            self.scroll.vadjustment().set_value(0.0);
        }
    }

    /// Ctrl+F og / søker. Esc avslutter et søk først; neste Esc går tilbake.
    pub fn on_key(self: &Rc<Self>, k: &crate::pages::Key) -> bool {
        use gtk::gdk::Key as K;
        if (k.ctrl && !k.shift && !k.alt && (k.key == K::f || k.key == K::F)) || (k.key == K::slash && !k.ctrl && !k.alt && !k.typing) {
            self.search.grab_focus();
            self.search.select_region(0, -1);
            return true;
        }
        if k.key == K::Escape {
            if self.search.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN) || !self.search.text().is_empty() || !self.query.borrow().is_empty() {
                self.finish_search();
                return true;
            }
            self.app.shell().close_settings();
            return true;
        }
        false
    }
}
