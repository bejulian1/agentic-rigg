//! Nettverk: aktiv tilkobling og trafikk nå øverst, ruting og DNS ved siden
//! av Wi-Fi, så én oppføring per grensesnitt (som Disker: klikk folder ut
//! adressene, maskinvaren, tellerne og en graf for ned og opp), og til slutt
//! portene som lytter. Virtuelle grensesnitt og loopback står samlet og
//! sammenfoldet nederst i lista.
//!
//! Dataene kommer fra tjenesten: `get_network` (sysfs, `/proc/net`,
//! `getifaddrs` og NetworkManager over D-Bus, hvert 3. s mens siden vises)
//! og prøvene (`sys.net`: ned og opp per kort hvert sekund, med historikk for
//! grafene). Siden endrer ingenting og sender ingenting ut på nettet.

use super::{Key, Page};
use crate::state::{App, Conn};
use crate::widgets::bar::Bar;
use crate::widgets::chart::{self, ChartOpts, ChartPanel, ChartSet, Cols, CursorSync, PanelOpts, SeriesDef};
use crate::widgets::rows::{KvList, tag};
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Icon, Panel, Spark, class, hbox, label, set_text, vbox};
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use rigg_core::fmt as F;
use rigg_core::net::{self as N, NetIface, NetInfo};
use rigg_core::{tr, trf};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

/// Hvor ofte lista hentes mens siden vises (ms).
const POLL_MS: u64 = 3000;
/// Så mange porter vises før lista foldes sammen.
const PORTS_SHORT: usize = 10;
/// Under denne vindusbredden: to fliser per rad og ingen sparklinjer i radene.
const COMPACT_BELOW: i32 = 1000;

// ---------------------------------------------------------------- Tekster

pub fn kind_name(kind: &str) -> &'static str {
    match kind {
        "ethernet" => tr!("Ethernet"),
        "wifi" => tr!("Wi-Fi"),
        "mobile" => tr!("Mobilnett"),
        "loopback" => tr!("Loopback"),
        "bridge" => tr!("Bro"),
        "vpn" => tr!("VPN eller tunnel"),
        "virtual" => tr!("Virtuelt"),
        _ => tr!("Nettverkskort"),
    }
}

fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "wifi" => "wifi",
        "vpn" => "shield",
        "loopback" => "rotate-ccw",
        _ => "network",
    }
}

/// Status som tekst og nivå (`ok`, `warn` eller `""`).
pub fn state_text(i: &NetIface) -> (&'static str, &'static str) {
    if i.connected() {
        return (tr!("nettverksstatus", "Tilkoblet"), "ok");
    }
    match (i.operstate.as_str(), i.kind.as_str()) {
        ("dormant", _) => (tr!("nettverksstatus", "Venter"), "warn"),
        (_, "ethernet") if i.carrier == Some(false) || i.operstate == "down" => (tr!("nettverksstatus", "Ingen kabel"), ""),
        (_, "wifi") => (tr!("nettverksstatus", "Ikke tilkoblet"), ""),
        ("unknown", _) if i.kind == "loopback" => (tr!("nettverksstatus", "Lokal"), ""),
        ("down" | "lowerlayerdown" | "notpresent", _) => (tr!("nettverksstatus", "Nede"), ""),
        _ => (tr!("nettverksstatus", "Ukjent"), ""),
    }
}

/// `operstate` slik kjernen skriver den, til verktøytipset.
fn operstate_text(s: &str) -> String {
    match s {
        "up" => tr!("operstate", "oppe").into(),
        "down" => tr!("operstate", "nede").into(),
        "dormant" => tr!("operstate", "venter på tilknytning").into(),
        "lowerlayerdown" => tr!("operstate", "underliggende lenke nede").into(),
        "notpresent" => tr!("operstate", "ikke til stede").into(),
        "testing" => tr!("operstate", "tester").into(),
        "unknown" => tr!("operstate", "ukjent").into(),
        other => other.to_string(),
    }
}

/// «1 Gbit/s», «2,5 Gbit/s», «100 Mbit/s».
pub fn speed_text(mbps: Option<u32>) -> String {
    match mbps {
        Some(m) if m >= 1000 => F::with_unit(F::num(Some(m as f64 / 1000.0), if m % 1000 == 0 { 0 } else { 1 }), "Gbit/s"),
        Some(m) => F::with_unit(F::num(Some(m as f64), 0), "Mbit/s"),
        None => F::DASH.into(),
    }
}

pub fn security_text(s: &str) -> &'static str {
    match s {
        "open" => tr!("Åpent (ingen kryptering)"),
        "wep" => "WEP",
        "wpa" => "WPA",
        "wpa2" => "WPA2",
        "wpa3" => "WPA3",
        "wpa2-enterprise" => "WPA2 Enterprise",
        "wpa3-enterprise" => "WPA3 Enterprise",
        "owe" => tr!("Enhanced Open (OWE)"),
        _ => F::DASH,
    }
}

/// «5 GHz · kanal 36».
pub fn band_text(freq: u32) -> String {
    let band = N::wifi_band(freq).map(|b| match b {
        "2.4" => F::with_unit(F::num(Some(2.4), 1), "GHz"),
        b => F::with_unit(b.to_string(), "GHz"),
    });
    match (band, N::wifi_channel(freq)) {
        (Some(b), Some(c)) => trf!("{band} · kanal {kanal}", band = b, kanal = c),
        (Some(b), None) => b,
        (None, Some(c)) => trf!("kanal {kanal}", kanal = c),
        (None, None) => F::DASH.into(),
    }
}

/// «−52 dBm · 96 %», med det som finnes.
pub fn signal_text(dbm: Option<f64>, pct: Option<f64>) -> String {
    match (dbm, pct) {
        (Some(d), Some(p)) => format!("{} · {}", F::with_unit(F::num(Some(d), 0), "dBm"), F::pct(Some(p), 0)),
        (Some(d), None) => F::with_unit(F::num(Some(d), 0), "dBm"),
        (None, Some(p)) => F::pct(Some(p), 0),
        (None, None) => F::DASH.into(),
    }
}

/// Signalnivå for fargen: under 30 % er svakt, under 50 % middels.
fn signal_level(pct: Option<f64>) -> &'static str {
    match pct {
        Some(p) if p < 30.0 => "danger",
        Some(p) if p < 50.0 => "warn",
        _ => "",
    }
}

/// Adressen i Lytteporter: «alle (IPv4)», «bare lokalt» eller adressen.
fn listen_addr(l: &N::Listener) -> (String, Option<&'static str>) {
    if N::any_addr(&l.addr) {
        (if l.family == 6 { tr!("alle (IPv6)") } else { tr!("alle (IPv4)") }.into(), Some("accent"))
    } else if N::local_only(&l.addr) {
        (l.addr.clone(), Some("quiet"))
    } else {
        (l.addr.clone(), None)
    }
}

fn count(v: Option<u64>) -> String {
    v.map(|v| F::num(Some(v as f64), 0)).unwrap_or_else(|| F::DASH.into())
}

fn dns_source(s: Option<&str>) -> String {
    match s {
        Some("networkmanager") => "NetworkManager".into(),
        Some("resolved") => "systemd-resolved".into(),
        Some("resolv.conf") => "/etc/resolv.conf".into(),
        _ => F::DASH.into(),
    }
}

fn kv_na(kv: &KvList, k: &str, v: Option<String>, na: &str) -> gtk::Label {
    match v.filter(|v| !v.is_empty()) {
        Some(v) => kv.add(k, &v),
        None => {
            let l = kv.add(k, na);
            l.add_css_class("na");
            l
        }
    }
}

fn panel(title: &str) -> Panel {
    let p = Panel::new(title);
    p.root.add_css_class("dk-panel");
    p
}

// ---------------------------------------------------------------- Ett grensesnitt

/// Én rad. Utseendet er det samme som på Disker (`dk-*` i disker.css).
struct Row {
    name: String,
    root: gtk::Box,
    head: gtk::Button,
    chev: Icon,
    icon: Icon,
    title: gtk::Label,
    chips: gtk::Box,
    state: gtk::Box,
    state_text: gtk::Label,
    meta: gtk::Label,
    rx: gtk::Label,
    tx: gtk::Label,
    spark_rx: Spark,
    spark_tx: Spark,
    detail: gtk::Box,
    /// Tellerne i detaljene: (rad, kolonne) → etikett, oppdateres på stedet.
    counters: RefCell<Vec<gtk::Label>>,
    sig: RefCell<String>,
    chart: RefCell<Option<Rc<ChartPanel>>>,
    open: Cell<bool>,
}

fn stat(name: &str, tip: &str, serie: &str) -> (gtk::Box, gtk::Label, Spark) {
    let b = hbox(4, &["dk-stat"]);
    b.append(&label(name, &["dk-k"]));
    let v = label(F::DASH, &["dk-v", "num", "nw-rate"]);
    v.set_xalign(1.0);
    b.append(&v);
    let sp = Spark::new(serie, 44);
    sp.set_valign(gtk::Align::Center);
    b.append(&sp);
    b.set_tooltip_text(Some(tip));
    (b, v, sp)
}

impl Row {
    fn new(name: &str) -> Rc<Row> {
        let root = vbox(0, &["group", "dk-disk", "nw-iface"]);
        let head = gtk::Button::new();
        head.add_css_class("flat");
        head.add_css_class("dk-head");
        let hb = vbox(2, &[]);
        let l1 = hbox(8, &["dk-l1"]);
        let icon = Icon::new("network", 16);
        icon.add_css_class("dk-icon");
        icon.set_valign(gtk::Align::Center);
        l1.append(&icon);
        let title = label("", &["dk-title"]);
        title.set_ellipsize(pango::EllipsizeMode::End);
        l1.append(&title);
        let chips = hbox(4, &["dk-chips"]);
        chips.set_valign(gtk::Align::Center);
        chips.set_hexpand(true);
        l1.append(&chips);
        let state = hbox(5, &["tag", "tag-lg", "dk-health", "nw-state"]);
        state.set_valign(gtk::Align::Center);
        state.set_hexpand(false);
        let state_text = gtk::Label::new(None);
        state_text.set_hexpand(true);
        state.append(&state_text);
        l1.append(&state);
        let chev = Icon::new("chevron-right", 14);
        chev.add_css_class("dk-chev");
        chev.set_valign(gtk::Align::Center);
        l1.append(&chev);
        hb.append(&l1);
        let l2 = hbox(12, &["dk-l2"]);
        // Brytes i smale vinduer i stedet for å kuttes (som på Disker).
        let meta = label("", &["dk-meta"]);
        meta.set_wrap(true);
        meta.set_wrap_mode(pango::WrapMode::WordChar);
        meta.set_xalign(0.0);
        meta.set_hexpand(true);
        l2.append(&meta);
        let (rb, rx, spark_rx) = stat(tr!("Ned"), tr!("Mottatt nå (statistics/rx_bytes)"), "load");
        let (tb, tx, spark_tx) = stat(tr!("Opp"), tr!("Sendt nå (statistics/tx_bytes)"), "memctl");
        // Prøvene har ikke loopback (net.rs), så raden har ingen rater.
        if name != "lo" {
            l2.append(&rb);
            l2.append(&tb);
        }
        hb.append(&l2);
        head.set_child(Some(&hb));
        root.append(&head);
        let detail = vbox(8, &["dk-detail"]);
        detail.set_visible(false);
        root.append(&detail);
        head.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        Rc::new(Row {
            name: name.to_string(),
            root,
            head,
            chev,
            icon,
            title,
            chips,
            state,
            state_text,
            meta,
            rx,
            tx,
            spark_rx,
            spark_tx,
            detail,
            counters: RefCell::new(vec![]),
            sig: RefCell::new(String::new()),
            chart: RefCell::new(None),
            open: Cell::new(false),
        })
    }

    fn set_iface(&self, i: &NetIface) {
        set_text(&self.title, kind_name(&i.kind));
        self.icon.set_icon(kind_icon(&i.kind));
        crate::widgets::clear(&self.chips);
        let name = tag(&i.name, None, "quiet");
        name.add_css_class("mono");
        name.set_tooltip_text(Some(tr!("Navnet på grensesnittet i kjernen")));
        self.chips.append(&name);
        if let Some(s) = i.speed_mbps {
            let t = tag(&speed_text(Some(s)), None, "quiet");
            t.set_tooltip_text(Some(tr!("Lenkehastigheten som er forhandlet nå")));
            self.chips.append(&t);
        }
        if let Some(w) = i.wifi.as_ref().filter(|_| i.connected())
            && let Some(f) = w.freq_mhz
            && let Some(b) = N::wifi_band(f)
        {
            let band = if b == "2.4" { F::with_unit(F::num(Some(2.4), 1), "GHz") } else { F::with_unit(b.to_string(), "GHz") };
            self.chips.append(&tag(&band, None, "quiet"));
        }
        if i.default_route {
            let t = tag(tr!("Standardrute"), None, "accent");
            t.set_tooltip_text(Some(tr!("Trafikk ut av maskinen går hit som standard")));
            self.chips.append(&t);
        }
        let (st, lvl) = state_text(i);
        set_text(&self.state_text, st);
        class(&self.state, "ok", lvl == "ok");
        class(&self.state, "warn", lvl == "warn");
        self.state.set_tooltip_text(Some(&trf!(
            "operstate: {tilstand} · carrier: {carrier}",
            tilstand = operstate_text(&i.operstate),
            carrier = match i.carrier {
                Some(true) => tr!("ja"),
                Some(false) => tr!("nei"),
                None => F::DASH,
            }
        )));
        let mut meta: Vec<String> = Vec::new();
        if let Some(s) = i.wifi.as_ref().and_then(|w| w.ssid.clone()).filter(|_| i.connected()) {
            meta.push(s);
        } else if let Some(c) = &i.connection {
            meta.push(c.clone());
        }
        if let Some(m) = &i.model {
            meta.push(match &i.vendor {
                Some(v) if !m.starts_with(v.as_str()) => format!("{v} {m}"),
                _ => m.clone(),
            });
        }
        if let Some(a) = i.ipv4.first() {
            meta.push(a.cidr());
        } else if let Some(a) = i.ipv6.iter().find(|a| a.scope == "global") {
            meta.push(a.cidr());
        }
        if meta.is_empty() {
            meta.push(tr!("ingen adresse").into());
        }
        set_text(&self.meta, &meta.join(" · "));
        self.meta.set_tooltip_text(Some(&meta.join(" · ")));
        self.head.update_property(&[gtk::accessible::Property::Label(&trf!("{type} {navn}, {status}", type = kind_name(&i.kind), navn = i.name, status = st.to_lowercase()))]);
    }

    fn set_live(&self, app: &App, compact: bool) {
        let s = app.telemetry();
        let n = s.as_ref().and_then(|t| t.sys.net.get(&self.name));
        set_text(&self.rx, &F::rate(n.and_then(|n| n.rx), None));
        set_text(&self.tx, &F::rate(n.and_then(|n| n.tx), None));
        let now = s.as_ref().map(|t| t.t).unwrap_or_else(rigg_core::now_ms);
        let o = SparkOpts { span_ms: 60_000, fixed: None, min_span: 50_000.0 };
        self.spark_rx.set_visible(!compact);
        self.spark_tx.set_visible(!compact);
        if !compact {
            self.spark_rx.set_points(&app.recent(&format!("net/{}/rx", self.name)), now, o);
            self.spark_tx.set_points(&app.recent(&format!("net/{}/tx", self.name)), now, o);
        }
    }

    fn set_open(&self, open: bool) {
        self.open.set(open);
        self.detail.set_visible(open);
        self.chev.set_icon(if open { "chevron-down" } else { "chevron-right" });
        class(&self.root, "open", open);
        self.head.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
    }

    /// Tellerne endres hver gang; de skrives på stedet.
    fn set_counters(&self, i: &NetIface) {
        let s = &i.stats;
        let vals = [
            F::bytes(s.rx_bytes.map(|v| v as f64), 2),
            count(s.rx_packets),
            count(s.rx_errors),
            count(s.rx_dropped),
            F::bytes(s.tx_bytes.map(|v| v as f64), 2),
            count(s.tx_packets),
            count(s.tx_errors),
            count(s.tx_dropped),
        ];
        let errs = [None, None, s.rx_errors, s.rx_dropped, None, None, s.tx_errors, s.tx_dropped];
        for ((l, v), e) in self.counters.borrow().iter().zip(vals.iter()).zip(errs) {
            set_text(l, v);
            class(l, "warn-text", e.is_some_and(|e| e > 0));
        }
    }
}

/// Signaturen for detaljene: alt unntatt tellerne og signalet.
fn detail_sig(i: &NetIface) -> String {
    let mut x = i.clone();
    x.stats = Default::default();
    if let Some(w) = x.wifi.as_mut() {
        w.signal_dbm = None;
        w.signal_pct = None;
        w.bitrate_mbps = None;
    }
    serde_json::to_string(&x).unwrap_or_default()
}

fn iface_panel(i: &NetIface, nm: bool) -> gtk::Widget {
    let p = panel(tr!("ett grensesnitt", "Grensesnitt"));
    let kv = KvList::new();
    let (st, _) = state_text(i);
    kv.add(tr!("Status"), &format!("{st} · {}", operstate_text(&i.operstate)));
    if i.connection.is_some() || i.kind != "loopback" {
        kv_na(&kv, tr!("Tilkobling"), i.connection.clone(), if nm { tr!("Ingen") } else { tr!("Krever NetworkManager") });
    }
    kv_na(&kv, "IPv4", (!i.ipv4.is_empty()).then(|| i.ipv4.iter().map(|a| a.cidr()).collect::<Vec<_>>().join("\n")), tr!("Ingen adresse"));
    let v6 = i
        .ipv6
        .iter()
        .map(|a| match (a.scope.as_str(), a.temporary) {
            (_, true) => trf!("{adresse} (midlertidig)", adresse = a.cidr()),
            ("link", _) => trf!("{adresse} (lenkelokal)", adresse = a.cidr()),
            _ => a.cidr(),
        })
        .collect::<Vec<_>>();
    kv_na(&kv, "IPv6", (!v6.is_empty()).then(|| v6.join("\n")), tr!("Ingen adresse"));
    if i.kind != "loopback" {
        kv_na(&kv, tr!("MAC-adresse"), i.mac.clone(), tr!("Ikke oppgitt"));
    }
    kv_na(&kv, "MTU", i.mtu.map(|m| F::with_unit(F::num(Some(m as f64), 0), "B")), tr!("Ikke oppgitt"));
    if i.physical() {
        let wifi = i.kind == "wifi";
        let no_speed = if wifi { tr!("Se bitrate under Wi-Fi") } else { tr!("Ukjent (ingen lenke)") };
        kv_na(&kv, tr!("Lenkehastighet"), i.speed_mbps.map(|s| speed_text(Some(s))), no_speed);
        if !wifi {
            kv_na(
                &kv,
                tr!("Dupleks"),
                i.duplex.as_deref().map(|d| if d == "full" { tr!("dupleks", "Full") } else { tr!("dupleks", "Halv") }.to_string()),
                tr!("Ukjent (ingen lenke)"),
            );
        }
    }
    p.body.append(&kv.grid);
    p.root.upcast()
}

fn hw_panel(i: &NetIface) -> gtk::Widget {
    let p = panel(tr!("Maskinvare"));
    let kv = KvList::new();
    if i.bus.is_none() {
        let n = label(tr!("Grensesnittet er laget av programvare og har ingen egen maskinvare."), &["group-note"]);
        n.set_wrap(true);
        n.set_xalign(0.0);
        p.body.append(&n);
        kv_na(&kv, tr!("Driver"), i.driver.clone(), tr!("Ingen"));
        p.body.append(&kv.grid);
        return p.root.upcast();
    }
    kv_na(&kv, tr!("Produsent"), i.vendor.clone(), tr!("Ukjent"));
    kv_na(&kv, tr!("Modell"), i.model.clone(), tr!("Ukjent"));
    kv_na(&kv, tr!("Driver"), i.driver.clone(), tr!("Ingen"));
    kv_na(&kv, tr!("Fastvare"), i.firmware.clone(), tr!("Ikke oppgitt"));
    kv_na(
        &kv,
        tr!("Buss"),
        i.bus.as_deref().map(|b| match &i.bus_addr {
            Some(a) => format!("{b} · {a}"),
            None => b.to_string(),
        }),
        tr!("Ukjent"),
    );
    kv_na(&kv, tr!("ID-er"), i.ids.clone(), tr!("Ikke oppgitt"));
    p.body.append(&kv.grid);
    p.root.upcast()
}

/// Tellerne: mottatt og sendt × data, pakker, feil og droppet.
fn counters_panel(r: &Row, i: &NetIface) -> gtk::Widget {
    let p = panel(tr!("Tellere"));
    p.aside.append(&label(tr!("siden grensesnittet kom opp"), &["muted", "sm"]));
    let g = gtk::Grid::new();
    g.add_css_class("dk-table");
    g.set_column_spacing(14);
    let heads = ["", tr!("Data"), tr!("Pakker"), tr!("Feil"), tr!("Droppet")];
    for (c, h) in heads.iter().enumerate() {
        let l = label(h, &["th"]);
        if c > 0 {
            l.set_xalign(1.0);
            l.set_hexpand(true);
        }
        g.attach(&l, c as i32, 0, 1, 1);
    }
    let mut cells = Vec::new();
    for (row, name) in [tr!("Mottatt"), tr!("Sendt")].iter().enumerate() {
        let last = row == 1;
        let n = label(name, &["td"]);
        if last {
            n.add_css_class("last");
        }
        g.attach(&n, 0, row as i32 + 1, 1, 1);
        for c in 1..5 {
            let l = label(F::DASH, &["td", "num"]);
            l.set_xalign(1.0);
            if last {
                l.add_css_class("last");
            }
            g.attach(&l, c, row as i32 + 1, 1, 1);
            cells.push(l);
        }
    }
    p.body.append(&g);
    let s = &i.stats;
    let extra = [(tr!("Multicast mottatt"), s.multicast), (tr!("Kollisjoner"), s.collisions)].iter().filter_map(|(k, v)| v.map(|v| format!("{k}: {}", count(Some(v))))).collect::<Vec<_>>();
    if !extra.is_empty() {
        g.set_tooltip_text(Some(&extra.join("\n")));
    }
    *r.counters.borrow_mut() = cells;
    r.set_counters(i);
    p.root.upcast()
}

fn traffic_chart(app: &Rc<App>, name: &str, window: i64, sync: &Rc<CursorSync>) -> Rc<ChartPanel> {
    let rx = format!("net/{name}/rx");
    let tx = format!("net/{name}/tx");
    let mut opts = ChartOpts::new(app, || if F::format().net_unit == "bit" { (0.0, 8.0) } else { (0.0, 1.0) });
    opts.unit = Rc::new(chart::net_unit);
    opts.decimals = 2;
    opts.min_span = Rc::new(|| if F::format().net_unit == "bit" { 0.8 } else { 0.1 });
    opts.auto = Rc::new(|| true);
    let p = ChartPanel::new(
        app,
        PanelOpts {
            title: trf!("Trafikk · {navn}", navn = name),
            series: vec![SeriesDef::new(&rx, tr!("Mottatt"), "--c-load").fill().map(chart::net_scaled), SeriesDef::new(&tx, tr!("Sendt"), "--c-memctl").map(chart::net_scaled)],
            window,
            chart: opts,
            sync: Some(sync.clone()),
            stats: true,
            now: true,
            extra: None,
            flat: false,
        },
    );
    p.root.add_css_class("dk-chart");
    p.root.set_vexpand(false);
    p.chart.set_size_request(-1, 150);
    p
}

// ---------------------------------------------------------------- Flisene øverst

struct Tile {
    value: gtk::Label,
    sub: gtk::Label,
    spark: Option<Spark>,
}

fn tile(parent: &gtk::Grid, name: &str, tip: &str, serie: Option<&str>) -> Tile {
    let b = vbox(2, &["pr-tile"]);
    b.set_hexpand(true);
    b.append(&label(name, &["pr-k"]));
    let row = hbox(8, &[]);
    let value = label(F::DASH, &["pr-v", "num"]);
    value.set_hexpand(true);
    value.set_ellipsize(pango::EllipsizeMode::End);
    row.append(&value);
    let spark = serie.map(|s| {
        let sp = Spark::new(s, 72);
        sp.set_valign(gtk::Align::Center);
        row.append(&sp);
        sp
    });
    b.append(&row);
    let sub = label("", &["pr-sub", "num"]);
    sub.set_ellipsize(pango::EllipsizeMode::End);
    b.append(&sub);
    b.set_tooltip_text(Some(tip));
    let n = parent.observe_children().n_items() as i32;
    parent.attach(&b, n, 0, 1, 1);
    Tile { value, sub, spark }
}

struct Tiles {
    grid: gtk::Grid,
    cols: Cell<i32>,
    conn: Tile,
    down: Tile,
    up: Tile,
    socks: Tile,
}

// ---------------------------------------------------------------- Siden

struct Inner {
    app: Rc<App>,
    root: gtk::Box,
    actions: gtk::Box,
    tiles: Tiles,
    content: gtk::Box,
    pair: Cols,
    route_holder: gtk::Box,
    wifi_holder: gtk::Box,
    wifi_live: RefCell<Option<(gtk::Label, Bar, gtk::Label)>>,
    pair_sig: RefCell<String>,
    list_title: gtk::Label,
    list_box: gtk::Box,
    virt_toggle: gtk::Button,
    virt_box: gtk::Box,
    virt_open: Cell<bool>,
    ports_holder: gtk::Box,
    ports_sig: RefCell<String>,
    /// Hele portlista vises (ellers de første [`PORTS_SHORT`]).
    ports_all: Cell<bool>,
    empty: gtk::Box,
    empty_title: gtk::Label,
    empty_text: gtk::Label,
    rows: RefCell<Vec<Rc<Row>>>,
    data: RefCell<Option<NetInfo>>,
    open: RefCell<HashSet<String>>,
    first: Cell<bool>,
    visible: Cell<bool>,
    loading: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    set: Rc<ChartSet>,
    sync: Rc<CursorSync>,
    window_dd: gtk::DropDown,
}

pub struct Nettverk {
    inner: Rc<Inner>,
}

impl Page for Nettverk {
    fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.inner.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        let i = &self.inner;
        i.visible.set(true);
        i.set.set_visible(true);
        i.load();
        i.start_timer();
        i.live();
    }
    fn hide(&self) {
        let i = &self.inner;
        i.visible.set(false);
        i.set.set_visible(false);
        if let Some(t) = i.timer.borrow_mut().take() {
            t.remove();
        }
    }
    fn window_hidden(&self, hidden: bool) {
        let i = &self.inner;
        if hidden {
            if let Some(t) = i.timer.borrow_mut().take() {
                t.remove();
            }
        } else if i.visible.get() {
            i.load();
            i.start_timer();
        }
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.key == gtk::gdk::Key::F5 && !k.ctrl && !k.alt && !k.typing {
            self.inner.load();
            return true;
        }
        false
    }
}

impl Inner {
    fn start_timer(self: &Rc<Self>) {
        if self.timer.borrow().is_some() {
            return;
        }
        let w = Rc::downgrade(self);
        *self.timer.borrow_mut() = Some(glib::timeout_add_local(std::time::Duration::from_millis(POLL_MS), move || {
            let Some(s) = w.upgrade() else { return glib::ControlFlow::Break };
            s.load();
            glib::ControlFlow::Continue
        }));
    }

    fn load(self: &Rc<Self>) {
        if self.loading.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        self.app.client.request("get_network", json!({}), move |res| {
            let Some(s) = w.upgrade() else { return };
            s.loading.set(false);
            match res {
                Ok(v) => {
                    if v["pending"].as_bool().unwrap_or(false) {
                        // Tjenesten leser i en egen tråd; svaret er klart straks.
                        let w2 = Rc::downgrade(&s);
                        glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                            if let Some(s) = w2.upgrade()
                                && s.visible.get()
                            {
                                s.load();
                            }
                        });
                        s.render_state();
                        return;
                    }
                    if let Ok(l) = serde_json::from_value::<NetInfo>(v) {
                        *s.data.borrow_mut() = Some(l);
                        s.render();
                    }
                }
                Err(_) => s.render_state(),
            }
        });
    }

    fn render_state(&self) {
        let data = self.data.borrow();
        let conn = self.app.conn.get();
        let (show_empty, title, text, icon) = match (&*data, conn) {
            (Some(l), _) if !l.ifaces.is_empty() => (false, "", "", "network"),
            (Some(_), _) => (true, tr!("Fant ingen nettverkskort"), tr!("Det er ingen grensesnitt i /sys/class/net."), "network"),
            (None, Conn::Connected) => (true, tr!("Leser nettverket …"), tr!("Tjenesten henter grensesnittene, rutene og DNS. Det tar et øyeblikk."), "network"),
            (None, _) => (true, tr!("Venter på tjenesten"), tr!("Nettverket leses av tjenesten (agentic-riggd). Siden fylles når den svarer."), "unplug"),
        };
        self.empty.set_visible(show_empty);
        self.content.set_visible(!show_empty);
        if let Some(i) = self.empty.first_child().and_then(|c| c.downcast::<Icon>().ok()) {
            i.set_icon(icon);
        }
        set_text(&self.empty_title, title);
        set_text(&self.empty_text, text);
    }

    fn render(self: &Rc<Self>) {
        self.render_state();
        let data = self.data.borrow().clone();
        let Some(info) = data else { return };
        // Første gang: fold ut den aktive tilkoblingen.
        if self.first.replace(false)
            && let Some(p) = N::primary(&info)
        {
            self.open.borrow_mut().insert(p.name.clone());
        }
        self.render_pair(&info);
        self.render_rows(&info);
        self.render_ports(&info);
        self.live();
    }

    /// Ruting og DNS ved siden av Wi-Fi (bygges på nytt bare når noe er endret).
    fn render_pair(&self, info: &NetInfo) {
        let wifi = info.ifaces.iter().filter(|i| i.kind == "wifi").min_by_key(|i| !i.connected());
        let sig = serde_json::to_string(&(&info.routes, &info.dns, &info.hostname, info.nm, wifi.map(detail_sig))).unwrap_or_default();
        if *self.pair_sig.borrow() != sig {
            *self.pair_sig.borrow_mut() = sig;
            crate::widgets::clear(&self.route_holder);
            self.route_holder.append(&self.route_panel(info));
            crate::widgets::clear(&self.wifi_holder);
            *self.wifi_live.borrow_mut() = None;
            self.wifi_holder.set_visible(wifi.is_some());
            if let Some(w) = wifi {
                self.wifi_holder.append(&self.wifi_panel(w, info.nm));
            }
            self.pair.queue_resize();
        }
        // Signalet og bitraten endres hele tiden.
        if let (Some(w), Some((sig_l, bar, rate_l))) = (wifi.and_then(|i| i.wifi.as_ref().map(|w| (i, w))), self.wifi_live.borrow().as_ref()) {
            let (i, w) = w;
            let on = i.connected();
            set_text(sig_l, &if on { signal_text(w.signal_dbm, w.signal_pct) } else { F::DASH.into() });
            bar.set(w.signal_pct.filter(|_| on).map(|p| p / 100.0), signal_level(w.signal_pct));
            let rate = w.bitrate_mbps.filter(|_| on).map(|b| F::with_unit(F::num(Some(b), if b < 100.0 { 1 } else { 0 }), "Mbit/s"));
            class(rate_l, "na", rate.is_none());
            set_text(rate_l, &rate.unwrap_or_else(|| if on && !info.nm { tr!("Krever NetworkManager").into() } else { F::DASH.into() }));
        }
    }

    fn route_panel(&self, info: &NetInfo) -> gtk::Widget {
        let p = panel(tr!("Ruting og DNS"));
        let kv = KvList::new();
        kv_na(&kv, tr!("Vertsnavn"), info.hostname.clone(), tr!("Ukjent"));
        let gw = |g: &Option<N::Gateway>| g.as_ref().map(|g| trf!("{adresse} via {grensesnitt}", adresse = g.addr, grensesnitt = g.iface));
        kv_na(&kv, tr!("Standard gateway"), gw(&info.routes.gateway4), tr!("Ingen standardrute"));
        kv_na(&kv, tr!("IPv6-gateway"), gw(&info.routes.gateway6), tr!("Ingen standardrute"));
        kv_na(&kv, tr!("DNS-servere"), (!info.dns.servers.is_empty()).then(|| info.dns.servers.join("\n")), tr!("Ingen oppgitt"));
        kv_na(&kv, tr!("Søkedomener"), (!info.dns.domains.is_empty()).then(|| info.dns.domains.join(", ")), tr!("Ingen"));
        let src = kv_na(&kv, tr!("DNS fra"), info.dns.source.as_deref().map(|s| dns_source(Some(s))), tr!("Ukjent"));
        if let Some(stub) = &info.dns.stub {
            src.set_tooltip_text(Some(&trf!(
                "Programmene spør den lokale mellomtjeneren {adresse} (systemd-resolved), som sender videre til serverne over.",
                adresse = stub
            )));
            set_text(&src, &trf!("{kilde} · via {adresse}", kilde = dns_source(info.dns.source.as_deref()), adresse = stub));
        }
        p.body.append(&kv.grid);
        fill_rows(p.root.upcast_ref());
        p.root.upcast()
    }

    fn wifi_panel(&self, i: &NetIface, nm: bool) -> gtk::Widget {
        let p = panel(tr!("Wi-Fi"));
        let t = tag(&i.name, None, "quiet");
        t.add_css_class("mono");
        p.aside.append(&t);
        let kv = KvList::new();
        let w = i.wifi.clone().unwrap_or_default();
        let on = i.connected();
        let need = if nm { tr!("Ikke oppgitt") } else { tr!("Krever NetworkManager") };
        if !on {
            kv.add(tr!("Status"), state_text(i).0);
        }
        kv_na(&kv, tr!("Nettverk (SSID)"), w.ssid.clone().filter(|_| on), if on { need } else { tr!("Ikke tilkoblet") });
        let sig_box = hbox(8, &["nw-signal"]);
        let sig_l = label(F::DASH, &["v", "num"]);
        sig_box.append(&sig_l);
        let bar = Bar::new(&self.app, true);
        bar.set_still(true);
        bar.set_size_request(96, -1);
        bar.set_hexpand(false);
        bar.set_valign(gtk::Align::Center);
        sig_box.append(&bar);
        sig_box.set_tooltip_text(Some(tr!("Signalnivå fra /proc/net/wireless og signalstyrke fra NetworkManager. Over −60 dBm er godt, under −75 dBm er svakt.")));
        kv.add_widget(tr!("Signal"), &sig_box);
        kv_na(&kv, tr!("Frekvens"), w.freq_mhz.filter(|_| on).map(|f| format!("{} · {}", F::with_unit(F::num(Some(f as f64), 0), "MHz"), band_text(f))), if on { need } else { F::DASH });
        let rate_l = kv.add(tr!("Bitrate"), F::DASH);
        rate_l.set_tooltip_text(Some(tr!("Overføringshastigheten mot tilgangspunktet nå")));
        kv_na(&kv, tr!("Sikkerhet"), w.security.as_deref().filter(|_| on).map(|s| security_text(s).to_string()), if on { need } else { F::DASH });
        kv_na(&kv, tr!("Tilgangspunkt (BSSID)"), w.bssid.clone().filter(|_| on), if on { need } else { F::DASH });
        p.body.append(&kv.grid);
        *self.wifi_live.borrow_mut() = Some((sig_l, bar, rate_l));
        fill_rows(p.root.upcast_ref());
        p.root.upcast()
    }

    fn render_rows(self: &Rc<Self>, info: &NetInfo) {
        let (main, virt): (Vec<&NetIface>, Vec<&NetIface>) = info.ifaces.iter().partition(|i| i.physical() || (i.kind == "vpn" && i.connected()));
        let names: Vec<&str> = main.iter().chain(virt.iter()).map(|i| i.name.as_str()).collect();
        let same = self.rows.borrow().iter().map(|r| r.name.as_str()).eq(names.iter().copied());
        if !same {
            crate::widgets::clear(&self.list_box);
            crate::widgets::clear(&self.virt_box);
            for r in self.rows.borrow().iter() {
                if let Some(c) = r.chart.borrow_mut().take() {
                    self.set.remove(&c);
                }
            }
            let mut rows = Vec::new();
            for (n, i) in main.iter().chain(virt.iter()).enumerate() {
                let r = Row::new(&i.name);
                let w = Rc::downgrade(self);
                let rw = Rc::downgrade(&r);
                r.head.connect_clicked(move |_| {
                    let (Some(s), Some(r)) = (w.upgrade(), rw.upgrade()) else { return };
                    if r.open.get() {
                        s.open.borrow_mut().remove(&r.name);
                    } else {
                        s.open.borrow_mut().insert(r.name.clone());
                    }
                    s.apply_open(&r);
                });
                if n < main.len() { &self.list_box } else { &self.virt_box }.append(&r.root);
                rows.push(r);
            }
            *self.rows.borrow_mut() = rows;
        }
        if main.is_empty() {
            let n = label(tr!("Ingen fysiske nettverkskort. Virtuelle grensesnitt står under."), &["group-note", "nw-none"]);
            n.set_xalign(0.0);
            if self.list_box.first_child().is_none() {
                self.list_box.append(&n);
            }
        }
        set_text(&self.list_title, &format!("{} · {}", tr!("Grensesnitt"), main.len()));
        self.virt_toggle.set_visible(!virt.is_empty());
        self.update_virt_toggle(virt.len());
        let rows = self.rows.borrow().clone();
        for (r, i) in rows.iter().zip(main.iter().chain(virt.iter())) {
            r.set_iface(i);
            let sig = format!("{}{}", info.nm, detail_sig(i));
            if *r.sig.borrow() != sig || r.detail.first_child().is_none() {
                *r.sig.borrow_mut() = sig;
                self.build_detail(r, i, info.nm);
            } else {
                r.set_counters(i);
            }
            self.apply_open(r);
        }
    }

    fn update_virt_toggle(&self, n: usize) {
        let open = self.virt_open.get();
        let b = hbox(6, &[]);
        b.append(&Icon::new(if open { "chevron-down" } else { "chevron-right" }, 14));
        b.append(&label(&trf!("Virtuelle og loopback ({antall})", antall = n), &[]));
        self.virt_toggle.set_child(Some(&b));
        self.virt_toggle.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
        self.virt_box.set_visible(open);
    }

    fn build_detail(self: &Rc<Self>, r: &Rc<Row>, i: &NetIface, nm: bool) {
        crate::widgets::clear(&r.detail);
        let pair = Cols::new(&[1.0, 1.0], 8, 720);
        pair.add_css_class("dk-pair");
        for w in [iface_panel(i, nm), hw_panel(i)] {
            fill_rows(&w);
            pair.append(&w);
        }
        let col = vbox(8, &["dk-detail-panels"]);
        col.append(&pair);
        col.append(&counters_panel(r, i));
        r.detail.append(&col);
        let holder = vbox(0, &["dk-chart-holder"]);
        r.detail.append(&holder);
        if let Some(c) = r.chart.borrow_mut().take() {
            self.set.remove(&c);
        }
    }

    fn apply_open(self: &Rc<Self>, r: &Rc<Row>) {
        let open = self.open.borrow().contains(&r.name);
        r.set_open(open);
        let holder = r.detail.first_child().and_then(|m| m.next_sibling()).and_then(|h| h.downcast::<gtk::Box>().ok());
        if open && r.chart.borrow().is_none() && r.name != "lo" {
            if let Some(h) = holder {
                let c = traffic_chart(&self.app, &r.name, self.app.settings().windows.nettverk as i64, &self.sync);
                h.append(&c.root);
                self.set.add(&c);
                c.view.load();
                *r.chart.borrow_mut() = Some(c);
            }
        } else if !open && let Some(c) = r.chart.borrow_mut().take() {
            self.set.remove(&c);
            if let Some(p) = c.root.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) {
                p.remove(&c.root);
            }
        }
    }

    fn render_ports(self: &Rc<Self>, info: &NetInfo) {
        let s = &info.sockets;
        let sig = format!("{}{}", self.ports_all.get(), serde_json::to_string(&s.listeners).unwrap_or_default());
        if *self.ports_sig.borrow() == sig {
            return;
        }
        *self.ports_sig.borrow_mut() = sig;
        crate::widgets::clear(&self.ports_holder);
        let p = panel(tr!("Porter som lytter"));
        p.root.add_css_class("nw-ports");
        let tcp = s.listeners.iter().filter(|l| l.proto == "tcp").count();
        let udp = s.listeners.len() - tcp;
        p.aside.append(&label(&format!("TCP {tcp} · UDP {udp}"), &["num", "muted"]));
        if s.listeners.is_empty() {
            p.body.append(&label(tr!("Ingen porter lytter."), &["group-note"]));
            self.ports_holder.append(&p.root);
            return;
        }
        let g = gtk::Grid::new();
        g.add_css_class("dk-table");
        g.set_column_spacing(14);
        for (c, h) in [tr!("Protokoll"), tr!("Adresse"), tr!("Port"), tr!("Prosess"), tr!("Bruker")].iter().enumerate() {
            let l = label(h, &["th"]);
            if c == 2 {
                l.set_xalign(1.0);
            }
            g.attach(&l, c as i32, 0, 1, 1);
        }
        let total = s.listeners.len();
        let shown = if self.ports_all.get() || total <= PORTS_SHORT + 2 { total } else { PORTS_SHORT };
        for (n, l) in s.listeners.iter().take(shown).enumerate() {
            let r = n as i32 + 1;
            let last = n + 1 == shown;
            let cell = |t: &str, c: i32, end: bool| {
                let w = label(t, &["td", "num"]);
                if end {
                    w.set_xalign(1.0);
                }
                if last {
                    w.add_css_class("last");
                }
                w.set_ellipsize(pango::EllipsizeMode::End);
                g.attach(&w, c, r, 1, 1);
                w
            };
            cell(&format!("{} · IPv{}", l.proto.to_uppercase(), l.family), 0, false);
            let (addr, kind) = listen_addr(l);
            let a = cell(&addr, 1, false);
            a.set_hexpand(true);
            a.set_tooltip_text(Some(match kind {
                Some("accent") => tr!("Lytter på alle adresser: kan nås fra nettverket hvis brannmuren slipper gjennom"),
                Some(_) => tr!("Bare fra denne maskinen"),
                None => tr!("Lytter på én adresse"),
            }));
            if kind == Some("quiet") {
                a.add_css_class("muted");
            }
            cell(&l.port.to_string(), 2, true);
            match (&l.process, l.pid) {
                (Some(name), Some(pid)) => {
                    cell(name, 3, false).set_tooltip_text(Some(&format!("PID {pid}")));
                }
                _ => {
                    let w = cell(F::DASH, 3, false);
                    w.add_css_class("na");
                    w.set_tooltip_text(Some(tr!("Prosessen eies av en annen bruker og kan bare leses av root.")));
                }
            }
            cell(l.user.as_deref().unwrap_or(F::DASH), 4, false);
        }
        p.body.append(&g);
        if shown < total || self.ports_all.get() && total > PORTS_SHORT + 2 {
            let all = self.ports_all.get();
            let t = if all { tr!("Vis færre").to_string() } else { trf!("Vis alle {antall}", antall = total) };
            let b = crate::widgets::button(Some(if all { "chevrons-down-up" } else { "chevron-down" }), Some(&t), &["flat", "sm", "nw-more"]);
            b.set_halign(gtk::Align::Start);
            b.update_state(&[gtk::accessible::State::Expanded(Some(all))]);
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                let Some(s) = w.upgrade() else { return };
                s.ports_all.set(!s.ports_all.get());
                if let Some(info) = s.data.borrow().clone() {
                    s.render_ports(&info);
                }
            });
            p.body.append(&b);
        }
        self.ports_holder.append(&p.root);
    }

    /// De levende tallene: flisene og ratene i radene.
    fn live(&self) {
        if !self.visible.get() {
            return;
        }
        let compact = self.app.window().is_some_and(|w| w.width() > 0 && w.width() < COMPACT_BELOW);
        let per = if compact { 2 } else { 4 };
        let t = &self.tiles;
        if t.cols.get() != per {
            t.cols.set(per);
            let kids: Vec<gtk::Widget> = std::iter::successors(t.grid.first_child(), |w| w.next_sibling()).collect();
            for k in &kids {
                t.grid.remove(k);
            }
            for (i, k) in kids.iter().enumerate() {
                t.grid.attach(k, i as i32 % per, i as i32 / per, 1, 1);
            }
        }
        let tel = self.app.telemetry();
        let now = tel.as_ref().map(|t| t.t).unwrap_or_else(rigg_core::now_ms);
        let (rx, tx) = tel.as_ref().map(|t| rigg_core::sensormeta::net_totals(&t.sys)).unwrap_or((None, None));
        set_text(&t.down.value, &F::rate(rx, None));
        set_text(&t.up.value, &F::rate(tx, None));
        let o = SparkOpts { span_ms: 60_000, fixed: None, min_span: 50_000.0 };
        if let Some(sp) = &t.down.spark {
            sp.set_points(&self.app.recent("net/rx"), now, o);
        }
        if let Some(sp) = &t.up.spark {
            sp.set_points(&self.app.recent("net/tx"), now, o);
        }
        let data = self.data.borrow();
        if let Some(info) = data.as_ref() {
            let (rb, tb) = N::totals(info);
            set_text(&t.down.sub, &trf!("totalt {mengde}", mengde = F::bytes(rb.map(|v| v as f64), 1)));
            set_text(&t.up.sub, &trf!("totalt {mengde}", mengde = F::bytes(tb.map(|v| v as f64), 1)));
            match N::primary(info) {
                Some(p) => {
                    let what = match (&p.kind[..], p.wifi.as_ref().and_then(|w| w.ssid.clone())) {
                        ("wifi", Some(ssid)) => trf!("Wi-Fi · {ssid}", ssid = ssid),
                        _ => kind_name(&p.kind).to_string(),
                    };
                    set_text(&t.conn.value, &what);
                    let mut sub = vec![p.name.clone()];
                    if let Some(s) = p.speed_mbps {
                        sub.push(speed_text(Some(s)));
                    } else if let Some(b) = p.wifi.as_ref().and_then(|w| w.bitrate_mbps) {
                        sub.push(F::with_unit(F::num(Some(b), 0), "Mbit/s"));
                    }
                    if let Some(a) = p.ipv4.first() {
                        sub.push(a.addr.clone());
                    }
                    set_text(&t.conn.sub, &sub.join(" · "));
                    class(&t.conn.value, "na", false);
                }
                None => {
                    set_text(&t.conn.value, tr!("Frakoblet"));
                    set_text(&t.conn.sub, tr!("ingen standardrute"));
                    class(&t.conn.value, "na", true);
                }
            }
            let s = &info.sockets;
            set_text(&t.socks.value, &F::num(Some(s.tcp_established as f64), 0));
            set_text(&t.socks.sub, &trf!("{lytter} lytter · {udp} UDP", lytter = s.tcp_listen, udp = s.udp));
        }
        for r in self.rows.borrow().iter() {
            r.set_live(&self.app, compact);
        }
    }
}

/// Et kort som er lavere enn naboen fordeler høyden på radene (som Disker).
fn fill_rows(w: &gtk::Widget) {
    if let Some(g) = w.downcast_ref::<gtk::Grid>()
        && g.has_css_class("kv")
    {
        let mut c = g.first_child();
        while let Some(x) = c {
            x.set_vexpand(true);
            c = x.next_sibling();
        }
        g.set_vexpand(true);
        return;
    }
    let mut c = w.first_child();
    while let Some(x) = c {
        if x.has_css_class("group-body") {
            x.set_vexpand(true);
        }
        fill_rows(&x);
        c = x.next_sibling();
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "dk-page", "nw-page"]);
    let content = vbox(8, &[]);
    // ---- Nå ----
    let now_panel = Panel::new(tr!("Nå"));
    now_panel.root.add_css_class("pr-now");
    let grid = gtk::Grid::new();
    grid.add_css_class("pr-tiles");
    grid.set_column_homogeneous(true);
    let tiles = Tiles {
        conn: tile(&grid, tr!("Aktiv tilkobling"), tr!("Grensesnittet standardruten går via, med lenkehastighet og IPv4-adresse."), None),
        down: tile(&grid, tr!("Ned"), tr!("Mottatt nå over de fysiske kortene. Under: alt som er mottatt siden kortene kom opp (vanligvis siden oppstart)."), Some("load")),
        up: tile(&grid, tr!("Opp"), tr!("Sendt nå over de fysiske kortene. Under: alt som er sendt siden kortene kom opp (vanligvis siden oppstart)."), Some("memctl")),
        socks: tile(&grid, tr!("TCP-tilkoblinger"), tr!("Åpne TCP-tilkoblinger (ESTABLISHED) i /proc/net/tcp og tcp6, porter som lytter og UDP-socketer."), None),
        grid: grid.clone(),
        cols: Cell::new(4),
    };
    now_panel.body.append(&grid);
    content.append(&now_panel.root);
    // ---- Ruting og DNS | Wi-Fi ----
    let pair = Cols::new(&[1.0, 1.0], 8, 720);
    pair.add_css_class("nw-pair");
    let route_holder = vbox(0, &["nw-holder"]);
    let wifi_holder = vbox(0, &["nw-holder"]);
    pair.append(&route_holder);
    pair.append(&wifi_holder);
    content.append(&pair);
    // ---- Grensesnittene ----
    // Radene er kort selv (som Disker), så lista har en overskrift i stedet for et panel.
    let list_title = label(tr!("Grensesnitt"), &["group-title", "nw-section-title"]);
    list_title.set_xalign(0.0);
    list_title.set_accessible_role(gtk::AccessibleRole::Heading);
    let list_box = vbox(8, &["dk-list", "nw-list"]);
    let virt_toggle = gtk::Button::new();
    virt_toggle.add_css_class("flat");
    virt_toggle.add_css_class("nw-virt-toggle");
    virt_toggle.set_halign(gtk::Align::Start);
    virt_toggle.set_tooltip_text(Some(tr!("Docker, veth, broer, tunneler og loopback")));
    let virt_box = vbox(8, &["dk-list", "nw-list", "nw-virt"]);
    virt_box.set_visible(false);
    let lists = vbox(8, &["nw-lists"]);
    lists.append(&list_box);
    lists.append(&virt_toggle);
    lists.append(&virt_box);
    content.append(&list_title);
    content.append(&lists);
    let ports_holder = vbox(0, &["nw-holder"]);
    content.append(&ports_holder);
    root.append(&content);
    // ---- Tom ----
    let empty = vbox(8, &["empty", "dk-empty"]);
    empty.set_valign(gtk::Align::Center);
    empty.set_halign(gtk::Align::Center);
    empty.set_vexpand(true);
    empty.append(&Icon::new("network", 32));
    let empty_title = label("", &["empty-title"]);
    empty_title.set_xalign(0.5);
    empty.append(&empty_title);
    let empty_text = label("", &[]);
    empty_text.set_xalign(0.5);
    empty_text.set_justify(gtk::Justification::Center);
    empty_text.set_wrap(true);
    empty_text.set_max_width_chars(60);
    empty.append(&empty_text);
    root.append(&empty);
    let foot = label(
        tr!("Lest fra /sys/class/net, /proc/net og NetworkManager over D-Bus, uten root. Siden endrer ikke nettverksoppsettet og sender ingenting ut på nettet. Prosessen bak en port vises bare for dine egne prosesser."),
        &["muted", "sm", "dk-foot"],
    );
    foot.set_wrap(true);
    foot.set_xalign(0.0);
    root.append(&foot);
    // ---- Toppfeltet: tidsvindu og hent på nytt ----
    let actions = hbox(4, &[]);
    let window_dd = {
        let a = app.clone();
        chart::window_dropdown(app.settings().windows.nettverk as i64, tr!("Tidsvindu for grafene på Nettverk"), move |sec| {
            a.set(json!({"windows": {"nettverk": sec}}));
        })
    };
    actions.append(&window_dd);
    let refresh = crate::widgets::button(Some("refresh-cw"), None, &["flat", "icon-only"]);
    crate::widgets::tip(&refresh, tr!("Les nettverket på nytt"), Some("F5"));
    refresh.update_property(&[gtk::accessible::Property::Label(tr!("Les nettverket på nytt"))]);
    actions.append(&refresh);
    let sync = {
        let a = Rc::downgrade(app);
        CursorSync::new(move || a.upgrade().is_none_or(|a| a.settings().chart_sync))
    };
    let inner = Rc::new(Inner {
        app: app.clone(),
        root,
        actions,
        tiles,
        content,
        pair,
        route_holder,
        wifi_holder,
        wifi_live: RefCell::new(None),
        pair_sig: RefCell::new(String::new()),
        list_title,
        list_box,
        virt_toggle,
        virt_box,
        virt_open: Cell::new(false),
        ports_holder,
        ports_sig: RefCell::new(String::new()),
        ports_all: Cell::new(false),
        empty,
        empty_title,
        empty_text,
        rows: RefCell::new(vec![]),
        data: RefCell::new(None),
        open: RefCell::new(HashSet::new()),
        first: Cell::new(true),
        visible: Cell::new(false),
        loading: Cell::new(false),
        timer: RefCell::new(None),
        set: ChartSet::new(app),
        sync,
        window_dd,
    });
    inner.render_state();
    {
        let w = Rc::downgrade(&inner);
        refresh.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.load();
            }
        });
    }
    {
        let w = Rc::downgrade(&inner);
        inner.virt_toggle.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.virt_open.set(!s.virt_open.get());
                let n = s.virt_box.observe_children().n_items() as usize;
                s.update_virt_toggle(n);
            }
        });
    }
    {
        let w = Rc::downgrade(&inner);
        app.on_telemetry.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.live();
            }
        });
    }
    {
        let w = Rc::downgrade(&inner);
        app.on_settings.connect(move |keys| {
            let Some(s) = w.upgrade() else { return };
            if keys.iter().any(|k| k == "windows") {
                let win = s.app.settings().windows.nettverk as i64;
                chart::set_window_dropdown(&s.window_dd, win);
                for p in s.set.list() {
                    p.set_window(win);
                }
            }
            if keys.iter().any(|k| matches!(k.as_str(), "unit" | "decimals" | "memUnit" | "netUnit" | "clock")) {
                for r in s.rows.borrow().iter() {
                    r.sig.borrow_mut().clear();
                }
                s.pair_sig.borrow_mut().clear();
                s.render();
            }
        });
    }
    {
        let w = Rc::downgrade(&inner);
        app.on_conn.connect(move |c| {
            let Some(s) = w.upgrade() else { return };
            s.render_state();
            if *c == Conn::Connected && s.visible.get() {
                s.load();
            }
        });
    }
    Box::new(Nettverk { inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(kind: &str, oper: &str, carrier: Option<bool>) -> NetIface {
        NetIface { name: "x".into(), kind: kind.into(), operstate: oper.into(), carrier, ..Default::default() }
    }

    #[test]
    fn tekstene() {
        assert_eq!(speed_text(Some(1000)), "1\u{202F}Gbit/s");
        assert_eq!(speed_text(Some(2500)), "2,5\u{202F}Gbit/s");
        assert_eq!(speed_text(Some(100)), "100\u{202F}Mbit/s");
        assert_eq!(speed_text(None), "–");
        assert_eq!(band_text(5180), "5\u{202F}GHz · kanal 36");
        assert_eq!(band_text(2437), "2,4\u{202F}GHz · kanal 6");
        assert_eq!(band_text(100), "–");
        assert_eq!(signal_text(Some(-52.0), Some(96.0)), "−52\u{202F}dBm · 96\u{202F}%");
        assert_eq!(signal_text(None, None), "–");
        assert_eq!(signal_level(Some(20.0)), "danger");
        assert_eq!(signal_level(Some(40.0)), "warn");
        assert_eq!(signal_level(Some(80.0)), "");
        assert_eq!(security_text("wpa2"), "WPA2");
        assert_eq!(security_text("rart"), "–");
        assert_eq!(state_text(&iface("ethernet", "up", Some(true))), ("Tilkoblet", "ok"));
        assert_eq!(state_text(&iface("ethernet", "down", Some(false))).0, "Ingen kabel");
        assert_eq!(state_text(&iface("wifi", "down", None)).0, "Ikke tilkoblet");
        assert_eq!(state_text(&iface("wifi", "dormant", Some(false))), ("Venter", "warn"));
        assert_eq!(state_text(&iface("loopback", "unknown", Some(true))).0, "Lokal");
        assert_eq!(state_text(&iface("bridge", "down", Some(false))).0, "Nede");
        let l = N::Listener { proto: "tcp".into(), family: 6, addr: "::".into(), port: 22, ..Default::default() };
        assert_eq!(listen_addr(&l), ("alle (IPv6)".into(), Some("accent")));
        let l = N::Listener { addr: "127.0.0.1".into(), family: 4, ..l };
        assert_eq!(listen_addr(&l).1, Some("quiet"));
        assert_eq!(dns_source(Some("resolved")), "systemd-resolved");
        assert_eq!(kind_name("vpn"), "VPN eller tunnel");
    }

    #[test]
    fn signaturen_overser_tellerne() {
        let mut a = iface("wifi", "up", Some(true));
        a.wifi = Some(N::Wifi { ssid: Some("x".into()), signal_dbm: Some(-50.0), ..Default::default() });
        let mut b = a.clone();
        b.stats.rx_bytes = Some(5);
        b.wifi.as_mut().unwrap().signal_dbm = Some(-60.0);
        assert_eq!(detail_sig(&a), detail_sig(&b));
        b.mtu = Some(9000);
        assert_ne!(detail_sig(&a), detail_sig(&b));
    }
}
