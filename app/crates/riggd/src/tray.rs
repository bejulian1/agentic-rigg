//! Systemfeltet (StatusNotifierItem, BØR i produktspesifikasjonen 3.1).
//!
//! Ikonet finnes **bare mens tjenesten går i bakgrunnen**: «Fortsett i
//! bakgrunnen» er på eller en logg går, og ingen vinduer er koblet til.
//! Verktøytipset har GPU-temperatur, effekt og last, og «Logger» når loggen
//! går. Menyen (produktspesifikasjonen 3.1): statuslinja (ikke klikkbar),
//! «Åpne Agentic Rigg», «Start logging» eller «Stopp logging» (alltid synlig)
//! og «Stopp bakgrunnsopptak», som slår av «Fortsett i bakgrunnen» og stopper
//! tjenesten hvis ingen logg går. Dobbeltklikk på ikonet åpner appen (`Activate`).
//!
//! Håndskrevet over GIO D-Bus, som tjenesten bruker fra før: to objekter
//! (`org.kde.StatusNotifierItem` og `com.canonical.dbusmenu`) på en egen
//! forbindelse til sesjonsbussen. Ingen ny crate og ingen ny tråd.
//!
//! **Livsløpet** (kostnaden er målt i `maalinger.md`, runde 3B):
//! - [`attach`] legger inn én tidtaker (hvert 2. s) som leser tjenestens
//!   tilstand ([`snapshot`]) og regner ut [`wanted`]. Ellers gjøres ingenting.
//! - Når ikonet er ønsket, følger vi navnet `org.kde.StatusNotifierWatcher`
//!   (verten, i Ubuntu AppIndicator-utvidelsen). Uten vert skjer ingenting
//!   mer, og ingenting skrives i journalen.
//! - Når verten finnes, lages [`Item`]: en egen forbindelse, navnet
//!   `org.kde.StatusNotifierItem-<pid>-1`, objektene og
//!   `RegisterStatusNotifierItem`.
//! - Når ikonet ikke lenger er ønsket (et vindu åpnes, bakgrunnen slås av,
//!   loggen stopper), lukkes forbindelsen. Verten ser at navnet forsvinner og
//!   fjerner ikonet.
//!
//! Koblingen inn i tjenesten er én linje i `main.rs`, før hovedløkka:
//! `let _tray = tray::attach(&d);`

use rigg_core::{tr, trf, trn};
use crate::daemon::Daemon;
use gio::prelude::*;
use glib::variant::{ObjectPath, ToVariant, Variant};
use rigg_core::fmt as F;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

pub const ITEM_PATH: &str = "/StatusNotifierItem";
pub const MENU_PATH: &str = "/MenuBar";
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub const SNI_IFACE: &str = "org.kde.StatusNotifierItem";
pub const MENU_IFACE: &str = "com.canonical.dbusmenu";
/// Det symbolske appikonet (`docs/produksjon/ikon.md` kap. 3), installert av `install.sh`.
pub const ICON: &str = "io.github.AgenticRigg-symbolic";
/// Hvor ofte tilstanden sjekkes.
pub const POLL_MS: u64 = 2000;
/// Uten sesjonsbuss: prøv igjen så sjelden.
const NO_BUS_RETRY_MS: i64 = 60_000;
/// Verktøytipset meldes på nytt høyst så ofte (hver melding vekker verten,
/// som henter det på nytt, og lasten svinger ved hver prøve).
pub const TOOLTIP_MIN_MS: i64 = 10_000;

// ---------------------------------------------------------------- Ren logikk

/// Det systemfeltet bygger på, lest fra tjenesten.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// «Fortsett i bakgrunnen» er på, og ingen har bedt tjenesten stoppe.
    pub background: bool,
    pub logging: bool,
    /// Vinduer som er koblet til (klienter som sier `agentic-rigg/…` i `hello`).
    pub windows: usize,
    pub stopping: bool,
    /// Første kort: temperatur (°C), effekt (W) og last (%).
    pub temp: Option<f64>,
    pub power: Option<f64>,
    pub load: Option<f64>,
    pub log_rows: Option<u64>,
}

/// Skal ikonet vises? Bare i bakgrunnen, og aldri mens et vindu er åpent.
pub fn wanted(s: &Snapshot) -> bool {
    !s.stopping && s.windows == 0 && (s.background || s.logging)
}

/// Er klienten et vindu? (Vinduet sier `agentic-rigg/1.0.0` i `hello`.)
pub fn is_window(client: &str) -> bool {
    client.starts_with("agentic-rigg/")
}

/// Leser det systemfeltet trenger fra tjenesten.
pub fn snapshot(d: &Daemon) -> Snapshot {
    let gpu = d.latest.as_ref().and_then(|t| t.gpus.values().next());
    let logging = d.logger.active();
    Snapshot {
        background: d.settings.background && !d.idle_shutdown,
        logging,
        windows: d.sessions.values().filter(|s| s.hello && is_window(&s.client)).count(),
        stopping: d.stopping,
        temp: gpu.and_then(|g| g.temp),
        power: gpu.and_then(|g| g.power),
        load: gpu.and_then(|g| g.gpu_util),
        log_rows: if logging { d.logger.status()["rows"].as_u64() } else { None },
    }
}

/// «GPU 52 °C · 12 W · 8 % last», med « · Logger» mens loggen går.
pub fn status_line(s: &Snapshot) -> String {
    let mut out = if s.temp.is_none() && s.load.is_none() && s.power.is_none() {
        tr!("Ingen tall fra skjermkortet").to_string()
    } else {
        trf!("GPU {temperatur} · {effekt} · {last} last", temperatur = F::with_unit(F::num(s.temp, 0), "°C"), effekt = F::watt(s.power, 0), last = F::pct(s.load, 0))
    };
    if s.logging {
        out.push_str(tr!(" · Logger"));
    }
    out
}

/// Verktøytipset: (tittel, tekst).
pub fn tooltip(s: &Snapshot) -> (String, String) {
    let mut body = status_line(s);
    body.push('\n');
    if s.logging {
        match s.log_rows {
            Some(n) => body.push_str(&trn!(n, "Logger til fil · {antall} rader", "Logger til fil · {antall} rader", antall = F::num(Some(n as f64), 0))),
            None => body.push_str(tr!("Logger til fil")),
        }
    } else {
        body.push_str(tr!("Tar opp historikk i bakgrunnen"));
    }
    ("Agentic Rigg".to_string(), body)
}

/// Skal verten få beskjed om nytt verktøytips? Straks når loggen starter
/// eller stopper, ellers høyst hvert [`TOOLTIP_MIN_MS`].
pub fn tooltip_due(last: &Snapshot, last_ms: i64, now: &Snapshot, now_ms: i64) -> bool {
    last.logging != now.logging || (tooltip(last) != tooltip(now) && now_ms - last_ms >= TOOLTIP_MIN_MS)
}

/// Det menyen kan gjøre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    StartLog,
    StopLog,
    /// Slår av «Fortsett i bakgrunnen»; tjenesten stopper hvis ingen logg går.
    StopBackground,
}

/// Én rad i menyen.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub separator: bool,
    pub action: Option<Action>,
}

pub const ID_STATUS: i32 = 1;
pub const ID_OPEN: i32 = 3;
/// «Start logging» eller «Stopp logging».
pub const ID_LOG: i32 = 4;
pub const ID_STOP_BG: i32 = 6;

/// Menyen. ID-ene er faste, så et klikk alltid treffer riktig handling.
pub fn menu(s: &Snapshot) -> Vec<MenuItem> {
    let it = |id, label: &str, enabled, visible, action| MenuItem { id, label: label.to_string(), enabled, visible, separator: false, action };
    let sep = |id, visible| MenuItem { id, label: String::new(), enabled: true, visible, separator: true, action: None };
    vec![
        it(ID_STATUS, &status_line(s), false, true, None),
        sep(2, true),
        it(ID_OPEN, tr!("Åpne Agentic Rigg"), true, true, Some(Action::Open)),
        if s.logging { it(ID_LOG, tr!("Stopp logging"), true, true, Some(Action::StopLog)) } else { it(ID_LOG, tr!("Start logging"), true, true, Some(Action::StartLog)) },
        sep(5, true),
        it(ID_STOP_BG, tr!("Stopp bakgrunnsopptak"), true, true, Some(Action::StopBackground)),
    ]
}

/// Radene som har byttet synlighet eller tekst (statuslinja ikke medregnet):
/// `(id, synlig nå, tekst nå)`.
pub fn item_changes(old: &Snapshot, new: &Snapshot) -> Vec<(i32, bool, String)> {
    menu(old).iter().zip(menu(new)).filter(|(a, b)| b.id != ID_STATUS && (a.visible != b.visible || a.label != b.label)).map(|(_, b)| (b.id, b.visible, b.label)).collect()
}

/// Handlingen bak en menyrad, bare når raden synes og kan klikkes.
pub fn action_for(s: &Snapshot, id: i32) -> Option<Action> {
    menu(s).into_iter().find(|m| m.id == id && m.visible && m.enabled).and_then(|m| m.action)
}

fn item_props(m: &MenuItem) -> HashMap<String, Variant> {
    let mut p = HashMap::new();
    if m.separator {
        p.insert("type".to_string(), "separator".to_variant());
    } else {
        p.insert("label".to_string(), m.label.to_variant());
    }
    if !m.enabled {
        p.insert("enabled".to_string(), false.to_variant());
    }
    if !m.visible {
        p.insert("visible".to_string(), false.to_variant());
    }
    p
}

/// `GetLayout`-svaret: `(u(ia{sv}av))`.
pub fn layout(s: &Snapshot, revision: u32, parent: i32) -> Variant {
    let node = |id: i32, props: HashMap<String, Variant>, children: Vec<Variant>| Variant::tuple_from_iter([id.to_variant(), props.to_variant(), children.to_variant()]);
    let items = menu(s);
    let tree = if parent == 0 {
        let mut root = HashMap::new();
        root.insert("children-display".to_string(), "submenu".to_variant());
        let children: Vec<Variant> = items.iter().map(|m| node(m.id, item_props(m), vec![])).collect();
        node(0, root, children)
    } else {
        match items.iter().find(|m| m.id == parent) {
            Some(m) => node(m.id, item_props(m), vec![]),
            None => node(parent, HashMap::new(), vec![]),
        }
    };
    Variant::tuple_from_iter([revision.to_variant(), tree])
}

/// `GetGroupProperties`-svaret: `(a(ia{sv}))`. Tom liste betyr alle.
pub fn group_properties(s: &Snapshot, ids: &[i32]) -> Variant {
    let list: Vec<(i32, HashMap<String, Variant>)> = menu(s).iter().filter(|m| ids.is_empty() || ids.contains(&m.id)).map(|m| (m.id, item_props(m))).collect();
    Variant::tuple_from_iter([list.to_variant()])
}

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

fn tooltip_variant(s: &Snapshot) -> Variant {
    let (title, body) = tooltip(s);
    (ICON.to_string(), Pixmaps::new(), title, body).to_variant()
}

/// Navnet ikonet eier på bussen (KDE-mønsteret, som verten også leter etter).
pub fn bus_name() -> String {
    format!("org.kde.StatusNotifierItem-{}-1", std::process::id())
}

pub const XML: &str = r#"<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionMovieName" type="s" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewAttentionIcon"/>
    <signal name="NewOverlayIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus"><arg name="status" type="s"/></signal>
  </interface>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
    <method name="GetLayout">
      <arg name="parentId" type="i" direction="in"/>
      <arg name="recursionDepth" type="i" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="revision" type="u" direction="out"/>
      <arg name="layout" type="(ia{sv}av)" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg name="ids" type="ai" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="properties" type="a(ia{sv})" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg name="id" type="i" direction="in"/>
      <arg name="name" type="s" direction="in"/>
      <arg name="value" type="v" direction="out"/>
    </method>
    <method name="Event">
      <arg name="id" type="i" direction="in"/>
      <arg name="eventId" type="s" direction="in"/>
      <arg name="data" type="v" direction="in"/>
      <arg name="timestamp" type="u" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg name="events" type="a(isvu)" direction="in"/>
      <arg name="idErrors" type="ai" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg name="id" type="i" direction="in"/>
      <arg name="needUpdate" type="b" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg name="ids" type="ai" direction="in"/>
      <arg name="updatesNeeded" type="ai" direction="out"/>
      <arg name="idErrors" type="ai" direction="out"/>
    </method>
    <signal name="ItemsPropertiesUpdated">
      <arg name="updatedProps" type="a(ia{sv})"/>
      <arg name="removedProps" type="a(ias)"/>
    </signal>
    <signal name="LayoutUpdated">
      <arg name="revision" type="u"/>
      <arg name="parent" type="i"/>
    </signal>
    <signal name="ItemActivationRequested">
      <arg name="id" type="i"/>
      <arg name="timestamp" type="u"/>
    </signal>
  </interface>
</node>"#;

// ---------------------------------------------------------------- Objektene på bussen

struct ItemState {
    snap: RefCell<Snapshot>,
    revision: Cell<u32>,
    on_action: Rc<dyn Fn(Action)>,
    /// Statuslinja verten sist fikk. Den oppdateres når menyen åpnes
    /// (`AboutToShow`), ikke ved hver prøve.
    shown_label: RefCell<String>,
    /// Tilstanden og tiden da verktøytipset sist ble meldt.
    tip: RefCell<(Snapshot, i64)>,
}

impl ItemState {
    /// Kjører handlingen etter at D-Bus-svaret er sendt (aldri inne i en
    /// metodehåndterer, så `Item` kan slippes i handlingen).
    fn fire(&self, a: Action) {
        let f = self.on_action.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move { f(a) });
    }
}

/// Ikonet på bussen. Lever bare mens det vises; `Drop` lukker forbindelsen,
/// og verten fjerner ikonet.
pub struct Item {
    conn: gio::DBusConnection,
    regs: Vec<gio::RegistrationId>,
    owner: Option<gio::OwnerId>,
    st: Rc<ItemState>,
}

impl Item {
    /// Egen forbindelse til sesjonsbussen, så ikonet forsvinner helt når den lukkes.
    pub fn new(snap: Snapshot, on_action: Rc<dyn Fn(Action)>) -> Result<Item, glib::Error> {
        let addr = gio::dbus_address_get_for_bus_sync(gio::BusType::Session, None::<&gio::Cancellable>)?;
        let flags = gio::DBusConnectionFlags::AUTHENTICATION_CLIENT | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION;
        let conn = gio::DBusConnection::for_address_sync(&addr, flags, None, None::<&gio::Cancellable>)?;
        Item::on_connection(conn, &bus_name(), snap, on_action)
    }

    /// Legger objektene på `conn`, tar navnet og melder seg til verten.
    pub fn on_connection(conn: gio::DBusConnection, name: &str, snap: Snapshot, on_action: Rc<dyn Fn(Action)>) -> Result<Item, glib::Error> {
        let node = gio::DBusNodeInfo::for_xml(XML)?;
        let missing = || glib::Error::new(gio::IOErrorEnum::Failed, "grensesnittet mangler i XML");
        let sni = node.lookup_interface(SNI_IFACE).ok_or_else(missing)?;
        let dbm = node.lookup_interface(MENU_IFACE).ok_or_else(missing)?;
        let tip = RefCell::new((snap.clone(), rigg_core::now_ms()));
        let st = Rc::new(ItemState { snap: RefCell::new(snap), revision: Cell::new(1), on_action, shown_label: RefCell::new(String::new()), tip });
        let mut item = Item { conn: conn.clone(), regs: Vec::new(), owner: None, st: st.clone() };

        let (s1, s2) = (st.clone(), st.clone());
        item.regs.push(
            conn.register_object(ITEM_PATH, &sni)
                .method_call(move |_c, _sender, _path, _iface, method, _params, inv| {
                    inv.return_value(None);
                    if method == "Activate" {
                        s1.fire(Action::Open);
                    }
                })
                .property(move |_c, _sender, _path, _iface, prop| sni_property(&s2, prop))
                .build()?,
        );
        let s3 = st.clone();
        item.regs.push(
            conn.register_object(MENU_PATH, &dbm)
                .method_call(move |c, _sender, _path, _iface, method, params, inv| menu_call(&s3, &c, method, &params, inv))
                .property(|_c, _sender, _path, _iface, prop| match prop {
                    "Version" => 3u32.to_variant(),
                    "TextDirection" => "ltr".to_variant(),
                    "Status" => "normal".to_variant(),
                    // IconThemePath
                    _ => Vec::<String>::new().to_variant(),
                })
                .build()?,
        );
        item.owner = Some(gio::bus_own_name_on_connection(
            &conn,
            name,
            gio::BusNameOwnerFlags::DO_NOT_QUEUE,
            |c, name| register_with_watcher(&c, name),
            |_c, _name| {},
        ));
        Ok(item)
    }

    /// Nye tall eller ny tilstand. Sender så lite som mulig: synlighet (og
    /// statuslinja) når en rad kommer eller går, og verktøytipset etter
    /// [`tooltip_due`]. Ellers oppdateres statuslinja når menyen åpnes.
    pub fn update(&self, snap: Snapshot) {
        let now = rigg_core::now_ms();
        let old = self.st.snap.replace(snap.clone());
        let due = {
            let (last, at) = &*self.st.tip.borrow();
            tooltip_due(last, *at, &snap, now)
        };
        if due {
            *self.st.tip.borrow_mut() = (snap.clone(), now);
            let _ = self.conn.emit_signal(None, ITEM_PATH, SNI_IFACE, "NewToolTip", None);
        }
        let changed = item_changes(&old, &snap);
        if !changed.is_empty() {
            // Verten (AppIndicator) henter ikke egenskapene til rader den
            // allerede har ved `LayoutUpdated`, så synlighet og tekst sendes
            // som egenskaper. Statuslinja følger med.
            let mut updated: Vec<(i32, HashMap<String, Variant>)> =
                changed.into_iter().map(|(id, v, l)| (id, HashMap::from([("visible".to_string(), v.to_variant()), ("label".to_string(), l.to_variant())]))).collect();
            let label = status_line(&snap);
            updated.push((ID_STATUS, HashMap::from([("label".to_string(), label.to_variant())])));
            *self.st.shown_label.borrow_mut() = label;
            self.st.revision.set(self.st.revision.get() + 1);
            let removed: Vec<(i32, Vec<String>)> = vec![];
            let _ = self.conn.emit_signal(None, MENU_PATH, MENU_IFACE, "ItemsPropertiesUpdated", Some(&(updated, removed).to_variant()));
        }
    }
}

impl Drop for Item {
    fn drop(&mut self) {
        if let Some(o) = self.owner.take() {
            gio::bus_unown_name(o);
        }
        for r in self.regs.drain(..) {
            let _ = self.conn.unregister_object(r);
        }
        self.conn.close(None::<&gio::Cancellable>, |_| {});
    }
}

fn sni_property(st: &ItemState, prop: &str) -> Variant {
    let s = st.snap.borrow();
    match prop {
        "Category" => "Hardware".to_variant(),
        "Id" => rigg_core::paths::Profile::current().app_id().to_variant(),
        "Title" => "Agentic Rigg".to_variant(),
        "Status" => "Active".to_variant(),
        "WindowId" => 0i32.to_variant(),
        "Menu" => ObjectPath::try_from(MENU_PATH).map(|p| p.to_variant()).unwrap_or_else(|_| "/".to_variant()),
        "ItemIsMenu" => false.to_variant(),
        "IconName" => ICON.to_variant(),
        "IconPixmap" | "OverlayIconPixmap" | "AttentionIconPixmap" => Pixmaps::new().to_variant(),
        "ToolTip" => tooltip_variant(&s),
        // IconThemePath, OverlayIconName, AttentionIconName, AttentionMovieName
        _ => "".to_variant(),
    }
}

fn menu_call(st: &ItemState, conn: &gio::DBusConnection, method: &str, params: &Variant, inv: gio::DBusMethodInvocation) {
    let snap = st.snap.borrow().clone();
    match method {
        "GetLayout" => {
            let parent = params.get::<(i32, i32, Vec<String>)>().map(|p| p.0).unwrap_or(0);
            *st.shown_label.borrow_mut() = status_line(&snap);
            inv.return_value(Some(&layout(&snap, st.revision.get(), parent)));
        }
        "GetGroupProperties" => {
            let ids = params.get::<(Vec<i32>, Vec<String>)>().map(|p| p.0).unwrap_or_default();
            inv.return_value(Some(&group_properties(&snap, &ids)));
        }
        "GetProperty" => {
            let (id, name) = params.get::<(i32, String)>().unwrap_or_default();
            let props = menu(&snap).iter().find(|m| m.id == id).map(item_props).unwrap_or_default();
            match props.get(&name) {
                Some(v) => inv.return_value(Some(&Variant::tuple_from_iter([Variant::from_variant(v)]))),
                None => inv.return_dbus_error("com.canonical.dbusmenu.Error.UnknownProperty", "Ukjent egenskap"),
            }
        }
        "Event" => {
            let (id, event) = params.get::<(i32, String, Variant, u32)>().map(|p| (p.0, p.1)).unwrap_or((-1, String::new()));
            inv.return_value(None);
            if event == "clicked"
                && let Some(a) = action_for(&snap, id)
            {
                st.fire(a);
            }
        }
        "EventGroup" => {
            let events = params.get::<(Vec<(i32, String, Variant, u32)>,)>().map(|p| p.0).unwrap_or_default();
            let mut errors = Vec::new();
            let mut actions = Vec::new();
            for (id, event, _, _) in events {
                if !menu(&snap).iter().any(|m| m.id == id) {
                    errors.push(id);
                } else if event == "clicked"
                    && let Some(a) = action_for(&snap, id)
                {
                    actions.push(a);
                }
            }
            inv.return_value(Some(&(errors,).to_variant()));
            for a in actions {
                st.fire(a);
            }
        }
        // Menyen åpnes: gi verten fersk statuslinje først.
        "AboutToShow" => {
            refresh_status_label(st, conn, &snap);
            inv.return_value(Some(&(false,).to_variant()));
        }
        "AboutToShowGroup" => {
            refresh_status_label(st, conn, &snap);
            inv.return_value(Some(&(Vec::<i32>::new(), Vec::<i32>::new()).to_variant()));
        }
        _ => inv.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Ukjent metode"),
    }
}

fn refresh_status_label(st: &ItemState, conn: &gio::DBusConnection, snap: &Snapshot) {
    let label = status_line(snap);
    if *st.shown_label.borrow() == label {
        return;
    }
    let mut p = HashMap::new();
    p.insert("label".to_string(), label.to_variant());
    let removed: Vec<(i32, Vec<String>)> = vec![];
    let _ = conn.emit_signal(None, MENU_PATH, MENU_IFACE, "ItemsPropertiesUpdated", Some(&(vec![(ID_STATUS, p)], removed).to_variant()));
    *st.shown_label.borrow_mut() = label;
}

/// Melder ikonet til verten. Forsvant verten i mellomtiden, er det ikke en feil.
fn register_with_watcher(conn: &gio::DBusConnection, name: &str) {
    conn.call(
        Some(WATCHER),
        WATCHER_PATH,
        WATCHER,
        "RegisterStatusNotifierItem",
        Some(&(name,).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        5000,
        None::<&gio::Cancellable>,
        |res| {
            if let Err(e) = res {
                let gone = e.matches(gio::DBusError::ServiceUnknown) || e.matches(gio::DBusError::NameHasNoOwner) || e.matches(gio::IOErrorEnum::Closed);
                if !gone {
                    eprintln!("{}", trf!("Systemfeltet: verten tok ikke imot ikonet: {e}", e = e));
                }
            }
        },
    );
}

// ---------------------------------------------------------------- Koblingen til tjenesten

/// Systemfeltet til tjenesten. Holdes i live av `main.rs`.
pub struct Tray {
    d: Weak<RefCell<Daemon>>,
    me: Weak<Tray>,
    /// Slutter å følge verten. (`gio::WatcherId` fra `bus_watch_name` kan ikke
    /// navngis i gio 0.22; den skygges av en annen type med samme navn.)
    unwatch: RefCell<Option<Box<dyn FnOnce()>>>,
    item: RefCell<Option<Item>>,
    no_bus_until: Cell<i64>,
}

/// Starter systemfeltet: én tidtaker hvert [`POLL_MS`]. Resten skjer bare når
/// ikonet er ønsket og en vert finnes.
pub fn attach(d: &Rc<RefCell<Daemon>>) -> Rc<Tray> {
    let t = Rc::new_cyclic(|me| Tray { d: Rc::downgrade(d), me: me.clone(), unwatch: RefCell::new(None), item: RefCell::new(None), no_bus_until: Cell::new(0) });
    let weak = Rc::downgrade(&t);
    glib::timeout_add_local(Duration::from_millis(POLL_MS), move || match weak.upgrade() {
        Some(t) => {
            t.poll();
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
    t
}

impl Tray {
    fn poll(&self) {
        let Some(d) = self.d.upgrade() else { return };
        let snap = match d.try_borrow() {
            Ok(d) => snapshot(&d),
            Err(_) => return,
        };
        if !wanted(&snap) {
            self.item.take();
            if let Some(stop) = self.unwatch.take() {
                stop();
            }
            return;
        }
        if self.unwatch.borrow().is_none() {
            self.start_watch();
        }
        if let Some(it) = self.item.borrow().as_ref() {
            it.update(snap);
        }
    }

    /// Følger verten. Uten sesjonsbuss: prøv igjen om et minutt, uten melding.
    fn start_watch(&self) {
        let now = rigg_core::now_ms();
        if now < self.no_bus_until.get() {
            return;
        }
        let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
            self.no_bus_until.set(now + NO_BUS_RETRY_MS);
            return;
        };
        let (up, down) = (self.me.clone(), self.me.clone());
        let id = gio::bus_watch_name_on_connection(
            &bus,
            WATCHER,
            gio::BusNameWatcherFlags::NONE,
            move |_c, _name, _owner| {
                if let Some(t) = up.upgrade() {
                    t.on_host(true);
                }
            },
            move |_c, _name| {
                if let Some(t) = down.upgrade() {
                    t.on_host(false);
                }
            },
        );
        *self.unwatch.borrow_mut() = Some(Box::new(move || gio::bus_unwatch_name(id)));
    }

    fn on_host(&self, up: bool) {
        if !up {
            self.item.take();
            return;
        }
        if self.item.borrow().is_some() {
            return;
        }
        let Some(d) = self.d.upgrade() else { return };
        let snap = match d.try_borrow() {
            Ok(d) => snapshot(&d),
            Err(_) => Snapshot::default(),
        };
        if !wanted(&snap) {
            return;
        }
        let me = self.me.clone();
        let on_action: Rc<dyn Fn(Action)> = Rc::new(move |a| {
            if let Some(t) = me.upgrade() {
                t.act(a);
            }
        });
        match Item::new(snap, on_action) {
            Ok(it) => *self.item.borrow_mut() = Some(it),
            Err(e) => eprintln!("{}", trf!("Systemfeltet kunne ikke lages: {e}", e = e)),
        }
    }

    fn act(&self, a: Action) {
        let Some(d) = self.d.upgrade() else { return };
        match a {
            Action::Open => open_app(),
            Action::StartLog => {
                if let Err(e) = d.borrow_mut().op_log_start(&serde_json::json!({})) {
                    eprintln!("{}", trf!("Kunne ikke starte loggen fra systemfeltet: {verdi}", verdi = e.message));
                }
            }
            Action::StopLog => {
                let _ = d.borrow_mut().op_log_stop();
            }
            Action::StopBackground => {
                // Slår av valget (lagres i tjeneste.json). Uten logg og uten
                // vindu stopper tjenesten etter fristen, som etter et lukket vindu.
                let mut d = d.borrow_mut();
                eprintln!("{}", tr!("Stopper bakgrunnsopptaket fra systemfeltet."));
                let _ = d.op_set_settings(&serde_json::json!({"background": false}));
                if !d.logger.active() && d.sessions.is_empty() {
                    d.stop("idle");
                }
            }
        }
        self.poll();
    }
}

/// Åpner vinduet: skrivebordsfila når appen er installert, ellers
/// `agentic-rigg` ved siden av tjenesten (utviklingsbygg).
fn open_app() {
    let desktop = format!("{}.desktop", rigg_core::paths::Profile::current().app_id());
    let info = gio::AppInfo::all().into_iter().find(|a| a.id().as_deref() == Some(desktop.as_str())).or_else(|| {
        let exe = std::env::current_exe().ok()?.with_file_name("agentic-rigg");
        if !exe.exists() {
            return None;
        }
        gio::AppInfo::create_from_commandline(glib::shell_quote(&exe), Some("Agentic Rigg"), gio::AppInfoCreateFlags::NONE).ok()
    });
    match info {
        Some(i) => {
            if let Err(e) = i.launch(&[], None::<&gio::AppLaunchContext>) {
                eprintln!("{}", trf!("Kunne ikke åpne Agentic Rigg: {e}", e = e));
            }
        }
        None => eprintln!("{}", trf!("Fant ikke Agentic Rigg ({desktop}).", desktop = desktop)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(dict: &Variant, key: &str) -> Option<Variant> {
        let v = dict.get::<HashMap<String, Variant>>()?.remove(key)?;
        if v.type_() == glib::VariantTy::VARIANT { v.as_variant() } else { Some(v) }
    }

    fn snap() -> Snapshot {
        Snapshot { background: true, temp: Some(52.4), power: Some(12.3), load: Some(8.0), ..Default::default() }
    }

    #[test]
    fn vises_bare_i_bakgrunnen_uten_vindu() {
        let s = snap();
        assert!(wanted(&s), "bakgrunn på, ingen vindu");
        assert!(!wanted(&Snapshot { windows: 1, ..s.clone() }), "aldri mens et vindu er åpent");
        assert!(!wanted(&Snapshot { background: false, ..s.clone() }), "bakgrunn av og ingen logg");
        assert!(wanted(&Snapshot { background: false, logging: true, ..s.clone() }), "loggen holder tjenesten i live");
        assert!(!wanted(&Snapshot { logging: true, windows: 2, ..s.clone() }));
        assert!(!wanted(&Snapshot { stopping: true, ..s.clone() }), "ikke mens tjenesten stopper");
        assert!(!wanted(&Snapshot::default()));
    }

    #[test]
    fn vinduer_kjennes_igjen_på_hello() {
        assert!(is_window("agentic-rigg/1.0.0"));
        assert!(!is_window("agentic-riggd-test"));
        assert!(!is_window(""));
        assert!(!is_window("kontrakt/1"));
    }

    #[test]
    fn menyen_uten_og_med_logg() {
        let s = snap();
        let m = menu(&s);
        let shown: Vec<&str> = m.iter().filter(|i| i.visible && !i.separator).map(|i| i.label.as_str()).collect();
        assert_eq!(shown, ["GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last", "Åpne Agentic Rigg", "Start logging", "Stopp bakgrunnsopptak"]);
        assert!(!m[0].enabled, "statuslinja kan ikke klikkes");
        assert_eq!(action_for(&s, ID_OPEN), Some(Action::Open));
        assert_eq!(action_for(&s, ID_STOP_BG), Some(Action::StopBackground));
        assert_eq!(action_for(&s, ID_LOG), Some(Action::StartLog), "loggraden synes alltid");
        assert_eq!(action_for(&s, ID_STATUS), None);
        assert_eq!(action_for(&s, 99), None);

        let l = Snapshot { logging: true, log_rows: Some(1234), ..s };
        let shown: Vec<String> = menu(&l).into_iter().filter(|i| i.visible && !i.separator).map(|i| i.label).collect();
        assert_eq!(shown, ["GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last · Logger", "Åpne Agentic Rigg", "Stopp logging", "Stopp bakgrunnsopptak"]);
        assert_eq!(action_for(&l, ID_LOG), Some(Action::StopLog));
        assert_eq!(item_changes(&s, &l), [(ID_LOG, true, "Stopp logging".to_string())]);
        assert_eq!(item_changes(&l, &s), [(ID_LOG, true, "Start logging".to_string())]);
        assert!(item_changes(&s, &Snapshot { temp: Some(70.0), ..s.clone() }).is_empty());
        // ID-ene står fast uansett tilstand.
        let ids = |s: &Snapshot| menu(s).iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids(&l), ids(&snap()));
    }

    #[test]
    fn verktøytipset() {
        let (title, body) = tooltip(&snap());
        assert_eq!(title, "Agentic Rigg");
        assert_eq!(body, "GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last\nTar opp historikk i bakgrunnen");
        let (_, body) = tooltip(&Snapshot { logging: true, log_rows: Some(1234), ..snap() });
        assert_eq!(body, "GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last · Logger\nLogger til fil · 1\u{202f}234 rader");
        let (_, body) = tooltip(&Snapshot { background: true, ..Default::default() });
        assert!(body.starts_with("Ingen tall fra skjermkortet\n"));
        assert_eq!(tooltip_variant(&snap()).type_().as_str(), "(sa(iiay)ss)");
    }

    #[test]
    fn verktøytipset_meldes_sjelden() {
        let a = snap();
        let b = Snapshot { load: Some(40.0), ..a.clone() };
        assert!(!tooltip_due(&a, 0, &a, 60_000), "uendret tekst gir ingen melding");
        assert!(!tooltip_due(&a, 0, &b, 2_000), "ny last etter 2 s venter");
        assert!(tooltip_due(&a, 0, &b, TOOLTIP_MIN_MS), "etter fristen meldes det");
        assert!(tooltip_due(&a, 0, &Snapshot { logging: true, ..a.clone() }, 1), "loggen meldes straks");
    }

    #[test]
    fn oppsettet_har_riktig_form() {
        let v = layout(&snap(), 7, 0);
        assert_eq!(v.type_().as_str(), "(u(ia{sv}av))");
        assert_eq!(v.child_value(0).get::<u32>(), Some(7));
        let root = v.child_value(1);
        assert_eq!(root.child_value(0).get::<i32>(), Some(0));
        let children = root.child_value(2);
        assert_eq!(children.n_children(), 6);
        let first = children.child_value(0).as_variant().expect("v");
        assert_eq!(first.type_().as_str(), "(ia{sv}av)");
        let props = first.child_value(1);
        let label = prop(&props, "label").and_then(|l| l.get::<String>());
        assert_eq!(label.as_deref(), Some("GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last"));
        assert_eq!(prop(&props, "enabled").and_then(|l| l.get::<bool>()), Some(false));
        // «Start logging» uten logg, alltid synlig.
        let log = children.child_value(3).as_variant().expect("v").child_value(1);
        assert_eq!(prop(&log, "visible").and_then(|l| l.get::<bool>()), None);
        assert_eq!(prop(&log, "label").and_then(|l| l.get::<String>()).as_deref(), Some("Start logging"));
        assert_eq!(group_properties(&snap(), &[ID_OPEN]).type_().as_str(), "(a(ia{sv}))");
        assert_eq!(group_properties(&snap(), &[ID_OPEN]).child_value(0).n_children(), 1);
        assert_eq!(group_properties(&snap(), &[]).child_value(0).n_children(), 6);
    }

    #[test]
    fn grensesnittene_kan_leses() {
        let node = gio::DBusNodeInfo::for_xml(XML).expect("XML");
        let sni = node.lookup_interface(SNI_IFACE).expect("SNI");
        assert!(sni.lookup_method("Activate").is_some(), "verten åpner appen på dobbeltklikk bare når Activate finnes");
        assert!(sni.lookup_property("ToolTip").is_some());
        let m = node.lookup_interface(MENU_IFACE).expect("dbusmenu");
        for name in ["GetLayout", "GetGroupProperties", "GetProperty", "Event", "EventGroup", "AboutToShow", "AboutToShowGroup"] {
            assert!(m.lookup_method(name).is_some(), "{name}");
        }
        assert!(bus_name().starts_with("org.kde.StatusNotifierItem-"));
    }

    // ------------------------------------------------ Mot en privat buss, uten ekte vert

    struct Bus {
        child: std::process::Child,
        addr: String,
    }

    impl Bus {
        /// En egen `dbus-daemon` for testen. `None` hvis den ikke finnes.
        fn start() -> Option<Bus> {
            use std::io::BufRead;
            #[allow(clippy::disallowed_methods)] // bare i testen: en privat buss
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
                .arg(format!("--address=unix:tmpdir={}", std::env::temp_dir().display()))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;
            let mut line = String::new();
            std::io::BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
            let addr = line.trim().to_string();
            if addr.is_empty() {
                let _ = child.kill();
                return None;
            }
            Some(Bus { child, addr })
        }

        fn connect(&self) -> gio::DBusConnection {
            let flags = gio::DBusConnectionFlags::AUTHENTICATION_CLIENT | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION;
            gio::DBusConnection::for_address_sync(&self.addr, flags, None, None::<&gio::Cancellable>).expect("koble til testbussen")
        }
    }

    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn spin(ctx: &glib::MainContext, until: impl Fn() -> bool) -> bool {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < end {
            while ctx.iteration(false) {}
            if until() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn name_has_owner(conn: &gio::DBusConnection, name: &str) -> bool {
        conn.call_sync(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", Some(&(name,).to_variant()), None, gio::DBusCallFlags::NONE, 2000, None::<&gio::Cancellable>)
            .ok()
            .and_then(|v| v.get::<(bool,)>())
            .is_some_and(|v| v.0)
    }

    /// Ikonet på en privat buss med en falsk vert: navnet, registreringen,
    /// menyen over D-Bus, klikk som blir handlinger, og at ikonet forsvinner
    /// når `Item` slippes. Uten vert skjer ingen registrering og ingen feil.
    #[test]
    fn ikonet_på_en_privat_buss() {
        let Some(bus) = Bus::start() else {
            eprintln!("dbus-daemon finnes ikke; hopper over");
            return;
        };
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            let name = "org.kde.StatusNotifierItem-4242-1";
            let actions: Rc<RefCell<Vec<Action>>> = Rc::default();
            let rec = actions.clone();
            let on_action: Rc<dyn Fn(Action)> = Rc::new(move |a| rec.borrow_mut().push(a));

            // 1. Ingen vert: ikonet tar navnet, men melder seg ikke til noen.
            let item = Item::on_connection(bus.connect(), name, snap(), on_action.clone()).expect("ikonet");
            let probe = bus.connect();
            assert!(spin(&ctx, || name_has_owner(&probe, name)), "navnet tas");
            drop(item);
            assert!(spin(&ctx, || !name_has_owner(&probe, name)), "navnet slippes når ikonet slippes");

            // 2. En falsk vert som husker registreringene.
            let registered: Rc<RefCell<Vec<String>>> = Rc::default();
            let host = bus.connect();
            let wnode = gio::DBusNodeInfo::for_xml(
                r#"<node><interface name="org.kde.StatusNotifierWatcher"><method name="RegisterStatusNotifierItem"><arg name="service" type="s" direction="in"/></method></interface></node>"#,
            )
            .expect("xml");
            let r2 = registered.clone();
            let _reg = host
                .register_object(WATCHER_PATH, &wnode.lookup_interface(WATCHER).expect("watcher"))
                .method_call(move |_c, _s, _p, _i, _m, params, inv| {
                    if let Some((svc,)) = params.get::<(String,)>() {
                        r2.borrow_mut().push(svc);
                    }
                    inv.return_value(None);
                })
                .build()
                .expect("vert");
            let _own = gio::bus_own_name_on_connection(&host, WATCHER, gio::BusNameOwnerFlags::NONE, |_, _| {}, |_, _| {});
            assert!(spin(&ctx, || name_has_owner(&probe, WATCHER)));

            let item = Item::on_connection(bus.connect(), name, snap(), on_action).expect("ikonet");
            assert!(spin(&ctx, || registered.borrow().iter().any(|s| s == name)), "melder seg til verten med navnet");

            // 3. En klient i en annen tråd leser menyen og klikker, som verten gjør.
            let addr = bus.addr.clone();
            let (tx, rx) = std::sync::mpsc::channel::<(String, Vec<String>, String)>();
            let client = std::thread::spawn(move || {
                let flags = gio::DBusConnectionFlags::AUTHENTICATION_CLIENT | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION;
                let c = gio::DBusConnection::for_address_sync(&addr, flags, None, None::<&gio::Cancellable>).expect("klient");
                let call = |path: &str, iface: &str, method: &str, args: Variant| {
                    c.call_sync(Some(name), path, iface, method, Some(&args), None, gio::DBusCallFlags::NONE, 3000, None::<&gio::Cancellable>).expect(method)
                };
                let icon = call(ITEM_PATH, "org.freedesktop.DBus.Properties", "Get", (SNI_IFACE, "IconName").to_variant());
                let icon = icon.child_value(0).as_variant().and_then(|v| v.get::<String>()).unwrap_or_default();
                let lay = call(MENU_PATH, MENU_IFACE, "GetLayout", (0i32, -1i32, Vec::<String>::new()).to_variant());
                let kids = lay.child_value(1).child_value(2);
                let labels: Vec<String> = (0..kids.n_children())
                    .filter_map(|i| {
                        let p = kids.child_value(i).as_variant()?.child_value(1);
                        let visible = prop(&p, "visible").and_then(|v| v.get::<bool>()).unwrap_or(true);
                        prop(&p, "label").and_then(|v| v.get::<String>()).filter(|_| visible)
                    })
                    .collect();
                call(MENU_PATH, MENU_IFACE, "Event", (ID_OPEN, "clicked", 0i32.to_variant(), 0u32).to_variant());
                call(MENU_PATH, MENU_IFACE, "Event", (ID_LOG, "clicked", 0i32.to_variant(), 0u32).to_variant());
                call(MENU_PATH, MENU_IFACE, "Event", (ID_STOP_BG, "hovered", 0i32.to_variant(), 0u32).to_variant());
                call(ITEM_PATH, SNI_IFACE, "Activate", (0i32, 0i32).to_variant());
                let tip = call(ITEM_PATH, "org.freedesktop.DBus.Properties", "Get", (SNI_IFACE, "ToolTip").to_variant());
                let tip = tip.child_value(0).as_variant().map(|v| v.child_value(3)).and_then(|v| v.get::<String>()).unwrap_or_default();
                tx.send((icon, labels, tip)).expect("send");
            });
            let got = std::cell::RefCell::new(None);
            assert!(spin(&ctx, || {
                if let Ok(v) = rx.try_recv() {
                    *got.borrow_mut() = Some(v);
                }
                got.borrow().is_some()
            }));
            client.join().expect("klienten");
            let (icon, labels, tip) = got.take().expect("svar");
            assert_eq!(icon, ICON);
            assert_eq!(labels, ["GPU 52\u{202f}°C · 12\u{202f}W · 8\u{202f}% last", "Åpne Agentic Rigg", "Start logging", "Stopp bakgrunnsopptak"]);
            assert!(tip.contains("Tar opp historikk i bakgrunnen"));
            // «hovered» er ikke et klikk.
            assert!(spin(&ctx, || actions.borrow().len() >= 3));
            assert_eq!(*actions.borrow(), [Action::Open, Action::StartLog, Action::Open]);

            // 4. Loggen starter: verten får «Stopp logging» som ny tekst.
            let got_props: Rc<RefCell<Vec<(i32, bool)>>> = Rc::default();
            let gp = got_props.clone();
            #[allow(deprecated)]
            let sub = probe.signal_subscribe(None, Some(MENU_IFACE), Some("ItemsPropertiesUpdated"), Some(MENU_PATH), None, gio::DBusSignalFlags::NONE, move |_c, _s, _p, _i, _n, params| {
                if let Some((upd, _rem)) = params.get::<(Vec<(i32, HashMap<String, Variant>)>, Vec<(i32, Vec<String>)>)>() {
                    for (id, props) in upd {
                        if let Some(v) = props.get("visible").and_then(|v| v.get::<bool>()) {
                            gp.borrow_mut().push((id, v));
                        }
                    }
                }
            });
            item.update(Snapshot { logging: true, ..snap() });
            assert!(spin(&ctx, || !got_props.borrow().is_empty()), "ItemsPropertiesUpdated kom");
            assert_eq!(*got_props.borrow(), [(ID_LOG, true)]);
            #[allow(deprecated)]
            probe.signal_unsubscribe(sub);
            drop(item);
            assert!(spin(&ctx, || !name_has_owner(&probe, name)), "ikonet forsvinner fra bussen");
        })
        .expect("egen hovedkontekst");
    }
}
