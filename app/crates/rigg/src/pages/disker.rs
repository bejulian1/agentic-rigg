//! Disker (1.1.0, runde 6A): én oppføring per disk i CrystalDiskInfo- og
//! HWiNFO-tone, med helse (OK, Advarsel, Feil), temperatur og lese- og
//! skrivehastighet i overskriften. Klikk folder ut detaljene: disken,
//! SMART-sammendraget, partisjonene med ledig plass, en graf for MB/s og hele
//! SMART-tabellen (som kan foldes for seg).
//!
//! Dataene kommer fra tjenesten: `get_disks` (UDisks2 og sysfs, hvert 15. s
//! mens siden vises; tjenesten lager lista hvert 60. s) og prøvene
//! (`sys.disks`: MB/s, aktivitet og temperatur hvert sekund). Vinduet leser
//! ingenting selv, og ingenting her sender SMART-kommandoer til diskene.

use super::{Key, Page};
use crate::state::{App, Conn};
use crate::widgets::bar::Bar;
use crate::widgets::chart::{ChartOpts, ChartPanel, ChartSet, PanelOpts, SeriesDef};
use crate::widgets::spark::SparkOpts;
use crate::widgets::rows::KvList;
use crate::widgets::{Icon, Spark, class, hbox, label, set_text, vbox};
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use rigg_core::disk::{self as D, Disk, DiskList, Level};
use rigg_core::fmt as F;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

/// Hvor ofte lista hentes mens siden vises (tjenesten lager den hvert 60. s).
const POLL_S: u32 = 15;
/// Under denne vindusbredden står overskriftene uten sparklinjer.
const COMPACT_BELOW: i32 = 1000;

// ---------------------------------------------------------------- Tekster

/// «12,3 MB/s» (desimalt, som diskverktøyene), «1,52 GB/s» over 1 000 MB/s.
pub fn mbps(v: Option<f64>) -> String {
    let Some(b) = v.filter(|b| b.is_finite()) else { return F::DASH.into() };
    let m = b / 1e6;
    if m >= 1000.0 {
        F::with_unit(F::num(Some(m / 1000.0), 2), "GB/s")
    } else {
        F::with_unit(F::num(Some(m), if m >= 100.0 { 0 } else { 1 }), "MB/s")
    }
}

/// «for 4 min siden», «for 3 t siden», ellers datoen.
pub fn ago(then: i64, now: i64) -> String {
    let s = (now - then).max(0) / 1000;
    if s < 90 {
        "nå nettopp".into()
    } else if s < 3600 {
        format!("for {} min siden", s / 60)
    } else if s < 48 * 3600 {
        format!("for {} t siden", s / 3600)
    } else {
        let off = crate::rapport_ui::utc_offset(then);
        F::since(F::local(then, off), F::local(now, off))
    }
}

fn level_class(l: Level) -> &'static str {
    match l {
        Level::Ok => "ok",
        Level::Warn => "warn",
        Level::Fail => "danger",
        Level::Unknown => "",
    }
}

fn level_icon(l: Level) -> &'static str {
    match l {
        Level::Ok => "circle-check",
        Level::Warn => "triangle-alert",
        Level::Fail => "circle-alert",
        Level::Unknown => "circle-help",
    }
}

fn temp_level(kind: &str, t: Option<f64>) -> &'static str {
    let (w, f) = D::temp_limits(kind);
    match t {
        Some(t) if t >= f => "danger",
        Some(t) if t >= w => "warn",
        _ => "",
    }
}

/// «SSD · SATA 6,0 Gb/s · 1 TB» som brikker.
fn chips(d: &Disk) -> Vec<(String, &'static str, Option<String>)> {
    let mut v = Vec::new();
    if !d.kind.is_empty() {
        let tip = match (d.kind.as_str(), d.rpm) {
            ("HDD", Some(r)) if r > 0 => Some(format!("Harddisk med {} o/min", F::num(Some(r as f64), 0))),
            ("HDD", _) => Some("Harddisk (plater)".to_string()),
            ("NVMe", _) => Some("SSD på PCIe (NVMe)".to_string()),
            _ => Some("SSD (ingen bevegelige deler)".to_string()),
        };
        v.push((d.kind.clone(), "", tip));
    }
    let bus = match (&d.link, d.bus.as_str()) {
        (Some(l), "NVMe") => Some(l.clone()),
        (Some(l), b) if !b.is_empty() => Some(format!("{b} {l}")),
        (_, b) if !b.is_empty() => Some(b.to_string()),
        _ => None,
    };
    if let Some(b) = bus {
        // «bus»: skjules i smale vinduer, så modellnavnet får plass (står
        // også under Tilkobling i Disk-panelet).
        v.push((b, "bus", Some("Tilkoblingen og hastigheten lenken går på nå".into())));
    }
    if let Some(s) = d.size {
        v.push((D::disk_size_label(s), "", Some(format!("{} byte", F::num(Some(s), 0)))));
    }
    if d.system {
        v.push(("System".into(), "accent", Some("Operativsystemet (/) ligger på denne disken".into())));
    }
    if d.removable || d.bus == "USB" {
        v.push(("Kan tas ut".into(), "", None));
    }
    v
}

// ---------------------------------------------------------------- Én disk

struct Row {
    id: String,
    root: gtk::Box,
    head: gtk::Button,
    chev: Icon,
    icon: Icon,
    title: gtk::Label,
    chips: gtk::Box,
    meta: gtk::Label,
    /// Hele linja og den korte (bare kjernenavnet) til smale vinduer.
    meta_text: RefCell<(String, String)>,
    health: gtk::Box,
    health_icon: Icon,
    health_text: gtk::Label,
    temp: gtk::Label,
    read: gtk::Label,
    write: gtk::Label,
    busy: gtk::Label,
    spark_r: Spark,
    spark_w: Spark,
    detail: gtk::Box,
    /// Det detaljene sist ble bygd fra (byggs bare på nytt ved endring).
    sig: RefCell<String>,
    kind: RefCell<String>,
    chart: RefCell<Option<Rc<ChartPanel>>>,
    /// Stolpene for partisjonene: (stolpe, prosent).
    open: Cell<bool>,
}

fn stat(name: &str, tip: &str) -> (gtk::Box, gtk::Label) {
    let b = hbox(4, &["dk-stat"]);
    let k = label(name, &["dk-k"]);
    let v = label(F::DASH, &["dk-v", "num"]);
    v.set_xalign(1.0);
    b.append(&k);
    b.append(&v);
    b.set_tooltip_text(Some(tip));
    (b, v)
}

impl Row {
    fn new(id: &str) -> Rc<Row> {
        let root = vbox(0, &["group", "dk-disk"]);
        let head = gtk::Button::new();
        head.add_css_class("flat");
        head.add_css_class("dk-head");
        let hb = vbox(2, &[]);
        // Linje 1: ikon, modell, brikker, helse, pil.
        let l1 = hbox(8, &["dk-l1"]);
        let icon = Icon::new("hard-drive", 16);
        icon.add_css_class("dk-icon");
        icon.set_valign(gtk::Align::Center);
        l1.append(&icon);
        let title = label("", &["dk-title"]);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_hexpand(false);
        l1.append(&title);
        let chips = hbox(4, &["dk-chips"]);
        chips.set_valign(gtk::Align::Center);
        chips.set_hexpand(true);
        l1.append(&chips);
        let health = hbox(5, &["tag", "tag-lg", "dk-health"]);
        health.set_valign(gtk::Align::Center);
        // Barna strekkes inni merket, men merket selv strekkes ikke i raden.
        health.set_hexpand(false);
        let health_icon = Icon::new("circle-help", 12);
        let health_text = gtk::Label::new(Some("Ukjent"));
        // Ikon og tekst midt i merket (det er like bredt for «OK» og «Advarsel»).
        health_icon.set_hexpand(true);
        health_icon.set_halign(gtk::Align::End);
        health_text.set_hexpand(true);
        health_text.set_halign(gtk::Align::Start);
        health.append(&health_icon);
        health.append(&health_text);
        l1.append(&health);
        let chev = Icon::new("chevron-right", 14);
        chev.add_css_class("dk-chev");
        chev.set_valign(gtk::Align::Center);
        l1.append(&chev);
        hb.append(&l1);
        // Linje 2: kjernenavn, serienummer og fastvare, så de levende tallene.
        let l2 = hbox(12, &["dk-l2"]);
        // Brytes i smale vinduer i stedet for å kuttes (serienummeret skal kunne leses).
        let meta = label("", &["dk-meta"]);
        meta.set_wrap(true);
        meta.set_wrap_mode(pango::WrapMode::WordChar);
        meta.set_xalign(0.0);
        meta.set_hexpand(true);
        l2.append(&meta);
        let (rb, read) = stat("Les", "Lesehastighet nå (/proc/diskstats)");
        let spark_r = Spark::new("load", 44);
        spark_r.set_valign(gtk::Align::Center);
        rb.append(&spark_r);
        l2.append(&rb);
        let (wb, write) = stat("Skriv", "Skrivehastighet nå (/proc/diskstats)");
        let spark_w = Spark::new("memctl", 44);
        spark_w.set_valign(gtk::Align::Center);
        wb.append(&spark_w);
        l2.append(&wb);
        let (bb, busy) = stat("Aktiv", "Andel av tiden disken hadde lesing eller skriving i gang");
        busy.set_width_chars(4);
        l2.append(&bb);
        let (tb, temp) = stat("Temp", "Fra SMART. UDisks2 leser den omtrent hvert 10. min.");
        temp.set_width_chars(5);
        l2.append(&tb);
        hb.append(&l2);
        head.set_child(Some(&hb));
        root.append(&head);
        let detail = vbox(8, &["dk-detail"]);
        detail.set_visible(false);
        root.append(&detail);
        head.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        Rc::new(Row {
            id: id.to_string(),
            root,
            head,
            chev,
            icon,
            title,
            chips,
            meta,
            meta_text: RefCell::new((String::new(), String::new())),
            health,
            health_icon,
            health_text,
            temp,
            read,
            write,
            busy,
            spark_r,
            spark_w,
            detail,
            sig: RefCell::new(String::new()),
            kind: RefCell::new(String::new()),
            chart: RefCell::new(None),
            open: Cell::new(false),
        })
    }

    /// Overskriften fra disklista.
    fn set_disk(&self, d: &Disk) {
        set_text(&self.title, if d.model.is_empty() { &d.name } else { &d.model });
        self.title.set_tooltip_text(d.vendor.as_deref().map(|v| format!("{v} {}", d.model)).as_deref());
        self.icon.set_icon(if d.bus == "USB" { "unplug" } else { "hard-drive" });
        crate::widgets::clear(&self.chips);
        for (t, kind, tip) in chips(d) {
            let c = crate::widgets::rows::tag(&t, None, if kind.is_empty() || kind == "bus" { "quiet" } else { kind });
            c.set_tooltip_text(tip.as_deref());
            if kind == "bus" {
                c.add_css_class("dk-bus");
            }
            self.chips.append(&c);
        }
        let mut meta = vec![d.name.clone()];
        if let Some(s) = &d.serial {
            meta.push(format!("S/N {s}"));
        }
        if let Some(f) = &d.firmware {
            meta.push(format!("fastvare {f}"));
        }
        set_text(&self.meta, &meta.join(" · "));
        self.meta.set_tooltip_text(Some(&meta.join(" · ")));
        *self.meta_text.borrow_mut() = (meta.join(" · "), d.name.clone());
        let lv = d.health.level;
        for c in ["ok", "warn", "danger"] {
            class(&self.health, c, c == level_class(lv));
        }
        self.health_icon.set_icon(level_icon(lv));
        set_text(&self.health_text, lv.text());
        let tip = if d.health.reasons.is_empty() {
            match lv {
                Level::Ok => "SMART viser ingen feil.".to_string(),
                _ => d.smart.note.clone().unwrap_or_else(|| "Ingen SMART å vurdere.".into()),
            }
        } else {
            d.health.reasons.join("\n")
        };
        self.health.set_tooltip_text(Some(&tip));
        self.health.update_property(&[gtk::accessible::Property::Label(&format!("Helse: {}. {tip}", lv.text()))]);
        *self.kind.borrow_mut() = d.kind.clone();
        self.head.update_property(&[gtk::accessible::Property::Label(&format!("{} {}, helse {}", d.model, d.name, lv.text().to_lowercase()))]);
    }

    /// De levende tallene fra prøven.
    fn set_live(&self, app: &App, s: Option<&D::DiskSample>, fallback_temp: Option<f64>) {
        set_text(&self.read, &mbps(s.and_then(|s| s.read)));
        set_text(&self.write, &mbps(s.and_then(|s| s.write)));
        set_text(&self.busy, &F::pct(s.and_then(|s| s.busy), 0));
        let t = s.and_then(|s| s.temp).or(fallback_temp);
        set_text(&self.temp, &F::temp(t, 0));
        let lv = temp_level(&self.kind.borrow(), t);
        for c in ["warn", "danger"] {
            class(&self.temp, c, c == lv);
        }
        let now = app.telemetry().map(|t| t.t).unwrap_or_else(rigg_core::now_ms);
        let o = SparkOpts { span_ms: 60_000, fixed: None, min_span: 2e6 };
        self.spark_r.set_points(&app.recent(&format!("disk/{}/read", self.id)), now, o);
        self.spark_w.set_points(&app.recent(&format!("disk/{}/write", self.id)), now, o);
    }

    fn set_open(&self, open: bool) {
        self.open.set(open);
        self.detail.set_visible(open);
        self.chev.set_icon(if open { "chevron-down" } else { "chevron-right" });
        class(&self.root, "open", open);
        self.head.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
    }
}

// ---------------------------------------------------------------- Detaljene

fn kv_na(kv: &KvList, k: &str, v: Option<String>, na: &str) -> gtk::Label {
    match v {
        Some(v) => kv.add(k, &v),
        None => {
            let l = kv.add(k, na);
            l.add_css_class("na");
            l
        }
    }
}

/// Et kort som er lavere enn naboen i raden, fordeler den ledige høyden på
/// radene i tabellen (`.kv`), i stedet for et tomrom nederst.
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

fn panel(title: &str) -> crate::widgets::Panel {
    let p = crate::widgets::Panel::new(title);
    p.root.add_css_class("dk-panel");
    p
}

fn disk_panel(d: &Disk) -> gtk::Widget {
    let p = panel("Disk");
    let kv = KvList::new();
    kv.add("Modell", &d.model);
    if let Some(v) = &d.vendor {
        kv.add("Produsent", v);
    }
    kv_na(&kv, "Serienummer", d.serial.clone(), "Ikke oppgitt");
    kv_na(&kv, "Fastvare", d.firmware.clone(), "Ikke oppgitt");
    if let Some(w) = &d.wwn {
        kv.add("WWN", w);
    }
    kv_na(
        &kv,
        "Kapasitet",
        d.size.map(|s| format!("{} · {}", D::disk_bytes(s), F::with_unit(F::num(Some(s / 1024f64.powi(3)), 1), "GiB"))),
        "Ikke oppgitt",
    );
    let ty = match (d.kind.as_str(), d.rpm) {
        ("HDD", Some(r)) if r > 0 => format!("Harddisk · {} o/min", F::num(Some(r as f64), 0)),
        ("HDD", _) => "Harddisk".into(),
        ("NVMe", _) => "SSD (NVMe)".into(),
        ("SSD", _) => "SSD".into(),
        _ => F::DASH.into(),
    };
    kv.add("Type", &ty);
    let link = match (&d.link, d.bus.as_str()) {
        (Some(l), "NVMe") => format!("NVMe · {l}"),
        (Some(l), b) => format!("{b} · {l}"),
        (None, "") => F::DASH.into(),
        (None, b) => b.to_string(),
    };
    let l = kv.add("Tilkobling", &link);
    if d.bus == "SATA" {
        l.set_tooltip_text(Some("Hastigheten SATA-lenken går på nå (sysfs, ata_link)."));
    }
    if let (Some(lb), Some(pb)) = (d.logical_block, d.physical_block) {
        kv.add("Sektorer", &format!("{} B logisk · {} B fysisk", F::num(Some(lb as f64), 0), F::num(Some(pb as f64), 0)));
    }
    kv_na(&kv, "I/O-planlegger", d.scheduler.clone(), "Ikke oppgitt");
    kv.add("Enhet", &d.dev);
    p.body.append(&kv.grid);
    p.root.upcast()
}

fn smart_panel(d: &Disk, now: i64) -> gtk::Widget {
    let s = &d.smart;
    let p = panel("SMART");
    if !s.source.is_empty() && s.supported {
        let src = label(if s.source == "sim" { "simulert" } else { "UDisks2" }, &[]);
        src.set_tooltip_text(Some("UDisks2 leser SMART selv omtrent hvert 10. minutt og vekker ikke disker i hvile. Agentic Rigg leser bare det UDisks2 alt har, og sender aldri kommandoer til disken."));
        p.aside.append(&src);
    }
    let kv = KvList::new();
    // Helse først, med grunnene.
    let hb = hbox(6, &[]);
    let t = crate::widgets::rows::tag(d.health.level.text(), Some(level_icon(d.health.level)), level_class(d.health.level));
    hb.append(&t);
    let why = if d.health.reasons.is_empty() {
        match d.health.level {
            Level::Ok => "Ingen feil".to_string(),
            _ => s.note.clone().unwrap_or_default(),
        }
    } else {
        d.health.reasons.join(" · ")
    };
    let wl = label(&why, &["v"]);
    wl.set_wrap(true);
    wl.set_xalign(0.0);
    wl.set_hexpand(true);
    hb.append(&wl);
    kv.add_widget("Helse", &hb);
    if s.source.is_empty() || !s.supported {
        // Grunnen står alt i Helse-raden.
        if s.note.is_none() {
            set_text(&wl, "SMART er ikke tilgjengelig for denne disken.");
        }
        p.body.append(&kv.grid);
        return p.root.upcast();
    }
    let tl = kv_na(&kv, "Temperatur", s.temp.map(|t| F::temp(Some(t), 0)), "Ikke oppgitt");
    let (w, f) = D::temp_limits(&d.kind);
    let lim = match s.nvme.as_ref().and_then(|n| n.warning_temp.zip(n.critical_temp)) {
        Some((a, b)) => format!("Kontrolleren varsler ved {} og er kritisk ved {}.", F::temp(Some(a), 0), F::temp(Some(b), 0)),
        None => format!("Gul fra {}, rød fra {} for {}.", F::temp(Some(w), 0), F::temp(Some(f), 0), if d.kind == "HDD" { "harddisker" } else { "SSD-er" }),
    };
    tl.set_tooltip_text(Some(&lim));
    kv_na(
        &kv,
        "Driftstid",
        s.power_on_hours.map(|h| format!("{} · {}", F::with_unit(F::num(Some(h), 0), "t"), F::with_unit(F::num(Some(h / 24.0), 0), "døgn"))),
        "Ikke oppgitt",
    );
    if let Some(c) = s.power_cycles {
        kv.add("Strømsykluser", &F::num(Some(c), 0));
    }
    if s.nvme.is_none() {
        let bad = kv_na(&kv, "Dårlige sektorer", s.bad_sectors.map(|b| F::num(Some(b as f64), 0)), "Ikke oppgitt");
        bad.set_tooltip_text(Some("Omallokerte, ventende og ukorrigerbare sektorer (05, C5 og C6) til sammen."));
        if s.bad_sectors.unwrap_or(0) > 0 {
            bad.add_css_class("warn-text");
        }
        let fl = kv.add(
            "Attributter som feiler",
            &match (s.attrs_failing, s.attrs_failed_past) {
                (Some(a), Some(b)) if b > 0 => format!("{a} nå · {b} tidligere"),
                (Some(a), _) => F::num(Some(a as f64), 0),
                _ => F::DASH.into(),
            },
        );
        if s.attrs_failing.unwrap_or(0) > 0 {
            fl.add_css_class("danger-text");
        }
    }
    if d.kind != "HDD" {
        let ll = kv_na(&kv, "Gjenværende levetid", s.life_left.map(|l| F::pct(Some(l), 0)), "Ikke oppgitt");
        ll.set_tooltip_text(Some(if s.nvme.is_some() { "100 % minus «Percentage Used» fra NVMe-helseloggen." } else { "Den normaliserte verdien produsenten bruker for slitasje (231, 233, 232 eller 177)." }));
    }
    if let Some(n) = &s.nvme {
        if let (Some(a), Some(t)) = (n.avail_spare, n.spare_thresh) {
            kv.add("Reservekapasitet", &format!("{} · grense {}", F::pct(Some(a), 0), F::pct(Some(t), 0)));
        }
        if let Some(m) = n.media_errors {
            kv.add("Mediefeil", &F::num(Some(m), 0));
        }
        if let Some(u) = n.unsafe_shutdowns {
            kv.add("Usikre avslutninger", &F::num(Some(u), 0)).set_tooltip_text(Some("Strømmen ble borte før disken fikk beskjed om å slå seg av."));
        }
        if let Some(e) = n.error_log_entries {
            kv.add("Oppføringer i feilloggen", &F::num(Some(e), 0));
        }
    }
    if let Some(wr) = s.written {
        kv.add("Skrevet totalt", &D::disk_bytes(wr)).set_tooltip_text(d.size.map(|sz| format!("{} ganger diskens størrelse.", F::num(Some(wr / sz), 1))).as_deref());
    }
    if let Some(r) = s.read {
        kv.add("Lest totalt", &D::disk_bytes(r));
    }
    let st = match (s.selftest.as_deref(), s.selftest_remaining) {
        (Some("inprogress"), Some(r)) => format!("Pågår · {} igjen", F::pct(Some(r as f64), 0)),
        (Some(x), _) => D::selftest_text(x),
        (None, _) => F::DASH.into(),
    };
    kv.add("Siste selvtest", &st);
    kv_na(&kv, "Lest av UDisks2", s.updated.map(|u| ago(u, now)), "Ikke lest ennå")
        .set_tooltip_text(s.updated.map(crate::rapport_ui::stamp_long).as_deref());
    p.body.append(&kv.grid);
    p.root.upcast()
}

fn parts_panel(app: &Rc<App>, d: &Disk) -> gtk::Widget {
    let p = panel("Partisjoner");
    if d.partitions.is_empty() {
        let n = label("Ingen partisjoner.", &["group-note"]);
        p.body.append(&n);
        return p.root.upcast();
    }
    let g = gtk::Grid::new();
    g.add_css_class("dk-table");
    g.add_css_class("dk-parts");
    g.set_column_spacing(12);
    let head = ["Partisjon", "Montert på", "Filsystem", "Størrelse", "Ledig", "Brukt"];
    for (c, h) in head.iter().enumerate() {
        let l = label(h, &["th"]);
        if c >= 3 {
            l.set_xalign(1.0);
        }
        g.attach(&l, c as i32, 0, 1, 1);
    }
    for (i, x) in d.partitions.iter().enumerate() {
        let r = i as i32 + 1;
        let last = i + 1 == d.partitions.len();
        let cell = |t: &str, c: i32, end: bool| {
            let l = label(t, &["td", "num"]);
            if end {
                l.set_xalign(1.0);
            }
            if last {
                l.add_css_class("last");
            }
            l.set_ellipsize(pango::EllipsizeMode::Middle);
            g.attach(&l, c, r, 1, 1);
            l
        };
        let n = cell(&x.name, 0, false);
        n.set_tooltip_text(x.label.as_deref().map(|l| format!("Etikett: {l}")).as_deref());
        let m = cell(x.mount.as_deref().unwrap_or("ikke montert"), 1, false);
        m.set_hexpand(true);
        m.set_tooltip_text(x.mount.as_deref());
        if x.mount.is_none() {
            m.add_css_class("na");
        }
        cell(x.fs.as_deref().unwrap_or(F::DASH), 2, false);
        cell(&x.size.map(D::disk_bytes).unwrap_or_else(|| F::DASH.into()), 3, true);
        cell(&x.avail.map(D::disk_bytes).unwrap_or_else(|| F::DASH.into()), 4, true);
        let bx = hbox(6, &["td", "dk-use"]);
        if last {
            bx.add_css_class("last");
        }
        let bar = Bar::new(app, true);
        bar.set_still(true);
        bar.set_size_request(56, -1);
        let pct = x.used_pct();
        bar.set(pct.map(|p| p / 100.0), match pct {
            Some(p) if p >= 95.0 => "danger",
            Some(p) if p >= 85.0 => "warn",
            _ => "",
        });
        bx.append(&bar);
        let pl = label(&F::pct(pct, 0), &["num", "dk-pct"]);
        pl.set_width_chars(4);
        pl.set_xalign(1.0);
        bx.append(&pl);
        if let (Some(u), Some(t)) = (x.used, x.total) {
            bx.set_tooltip_text(Some(&format!("{} brukt av {}", D::disk_bytes(u), D::disk_bytes(t))));
        }
        g.attach(&bx, 5, r, 1, 1);
    }
    p.body.append(&g);
    p.root.upcast()
}

/// Hele SMART-tabellen, som kan foldes. Rader under terskelen er røde, og
/// rader som har vært under terskelen eller teller dårlige sektorer, er gule.
fn attrs_panel(d: &Disk, open: bool, on_toggle: Rc<dyn Fn(bool)>) -> gtk::Widget {
    let s = &d.smart;
    let p = panel("SMART-attributter");
    p.root.add_css_class("dk-attrs");
    let count = label(&format!("{}", s.attributes.len()), &["num"]);
    p.aside.append(&count);
    let toggle = crate::widgets::button(Some(if open { "chevrons-down-up" } else { "chevron-down" }), None, &["flat", "sm", "icon-only"]);
    let t = if open { "Fold sammen tabellen" } else { "Vis hele SMART-tabellen" };
    toggle.set_tooltip_text(Some(t));
    toggle.update_property(&[gtk::accessible::Property::Label(t)]);
    toggle.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
    p.aside.append(&toggle);
    let body = vbox(0, &[]);
    body.set_visible(open);
    let note = label("Verdi, Verste og Terskel er produsentens normaliserte tall (høyere er bedre). «Tolket» er den rå verdien i riktig enhet.", &["group-note", "dk-attrs-note"]);
    note.set_wrap(true);
    let g = gtk::Grid::new();
    g.add_css_class("dk-table");
    g.set_column_spacing(14);
    for (c, h) in ["ID", "Attributt", "Verdi", "Verste", "Terskel", "Tolket", ""].iter().enumerate() {
        let l = label(h, &["th"]);
        if (2..=5).contains(&c) {
            l.set_xalign(1.0);
        }
        g.attach(&l, c as i32, 0, 1, 1);
    }
    for (i, a) in s.attributes.iter().enumerate() {
        let r = i as i32 + 1;
        let last = i + 1 == s.attributes.len();
        let bad = D::BAD_SECTOR_ATTRS.contains(&a.id) && a.count().is_some_and(|c| c > 0);
        let lvl = if a.failing() {
            "danger"
        } else if a.failed_past() || bad {
            "warn"
        } else {
            ""
        };
        let cell = |t: &str, c: i32, end: bool| {
            let l = label(t, &["td", "num"]);
            if end {
                l.set_xalign(1.0);
            }
            if last {
                l.add_css_class("last");
            }
            if !lvl.is_empty() {
                l.add_css_class(&format!("{lvl}-text"));
            }
            g.attach(&l, c, r, 1, 1);
            l
        };
        cell(&format!("{:02X}", a.id), 0, false).set_tooltip_text(Some(&format!("{} desimalt", a.id)));
        let n = cell(&D::attr_label(a.id, &a.name), 1, false);
        n.set_hexpand(true);
        n.set_ellipsize(pango::EllipsizeMode::End);
        n.set_tooltip_text(Some(&format!("{}{}", a.name, if a.prefail() { " · pre-fail: under terskelen betyr at disken er i ferd med å feile" } else { "" })));
        let num = |v: i32| if v < 0 { F::DASH.to_string() } else { v.to_string() };
        cell(&num(a.value), 2, true);
        cell(&num(a.worst), 3, true);
        cell(&if a.threshold > 0 { a.threshold.to_string() } else { F::DASH.to_string() }, 4, true);
        cell(&D::attr_text(a, &d.model, d.size), 5, true);
        let dot = crate::widgets::rows::dot(match lvl {
            "danger" => "danger",
            "warn" => "warn",
            _ => "ok",
        });
        dot.add_css_class("td");
        if last {
            dot.add_css_class("last");
        }
        dot.set_tooltip_text(Some(match lvl {
            "danger" => "Under terskelen nå",
            "warn" if bad => "Dårlige sektorer",
            "warn" => "Har vært under terskelen",
            _ => "OK",
        }));
        g.attach(&dot, 6, r, 1, 1);
    }
    body.append(&g);
    body.append(&note);
    p.body.append(&body);
    let b2 = body.clone();
    toggle.connect_clicked(move |b| {
        let open = !b2.is_visible();
        b2.set_visible(open);
        b.set_child(Some(&Icon::new(if open { "chevrons-down-up" } else { "chevron-down" }, 14)));
        let t = if open { "Fold sammen tabellen" } else { "Vis hele SMART-tabellen" };
        b.set_tooltip_text(Some(t));
        b.update_property(&[gtk::accessible::Property::Label(t)]);
        b.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
        on_toggle(open);
    });
    p.root.upcast()
}

fn speed_chart(app: &Rc<App>, d: &Disk) -> Rc<ChartPanel> {
    let to_mb = |b: f64| b / 1e6;
    let keys = vec![format!("disk/{}/read", d.id), format!("disk/{}/write", d.id)];
    let wa = Rc::downgrade(app);
    let mut opts = ChartOpts::new(app, move || (0.0, wa.upgrade().map(|a| crate::widgets::chart::mbps_top(&a, &keys, 10.0)).unwrap_or(50.0)));
    opts.unit = Rc::new(|| "MB/s".into());
    opts.decimals = 1;
    opts.min_span = Rc::new(|| 5.0);
    opts.auto = Rc::new(|| true);
    let p = ChartPanel::new(
        app,
        PanelOpts {
            title: format!("Hastighet · {}", d.name),
            series: vec![SeriesDef::new(&format!("disk/{}/read", d.id), "Lesing", "--c-load").fill().map(to_mb), SeriesDef::new(&format!("disk/{}/write", d.id), "Skriving", "--c-memctl").map(to_mb)],
            window: 300,
            chart: opts,
            sync: None,
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

// ---------------------------------------------------------------- Siden

struct Inner {
    app: Rc<App>,
    root: gtk::Box,
    actions: gtk::Box,
    notice: gtk::Box,
    notice_text: gtk::Label,
    list_box: gtk::Box,
    empty: gtk::Box,
    empty_title: gtk::Label,
    empty_text: gtk::Label,
    rows: RefCell<Vec<Rc<Row>>>,
    data: RefCell<Option<DiskList>>,
    open: RefCell<HashSet<String>>,
    attrs_open: RefCell<HashSet<String>>,
    first: Cell<bool>,
    visible: Cell<bool>,
    loading: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    set: Rc<ChartSet>,
}

pub struct Disker {
    inner: Rc<Inner>,
}

impl Page for Disker {
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
        *self.timer.borrow_mut() = Some(glib::timeout_add_seconds_local(POLL_S, move || {
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
        self.app.client.request("get_disks", json!({}), move |res| {
            let Some(s) = w.upgrade() else { return };
            s.loading.set(false);
            match res {
                Ok(v) => {
                    let pending = v["pending"].as_bool().unwrap_or(false);
                    if pending {
                        // Tjenesten lager lista i neste runde.
                        let w2 = Rc::downgrade(&s);
                        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                            if let Some(s) = w2.upgrade()
                                && s.visible.get()
                            {
                                s.load();
                            }
                        });
                        s.render_state();
                        return;
                    }
                    if let Ok(l) = serde_json::from_value::<DiskList>(v) {
                        *s.data.borrow_mut() = Some(l);
                        s.render();
                    }
                }
                Err(_) => s.render_state(),
            }
        });
    }

    /// Tomtilstandene og merknaden øverst.
    fn render_state(&self) {
        let data = self.data.borrow();
        let conn = self.app.conn.get();
        let (show_empty, title, text, icon) = match (&*data, conn) {
            (Some(l), _) if !l.disks.is_empty() => (false, "", String::new(), "hard-drive"),
            (Some(_), _) => (true, "Fant ingen disker", "Det er ingen fysiske disker i /sys/block (loop-, ram- og dm-enheter regnes ikke med).".to_string(), "hard-drive"),
            (None, Conn::Connected) => (true, "Leser diskene …", "Tjenesten henter modell, SMART og partisjoner. Det tar et sekund.".to_string(), "hard-drive"),
            (None, _) => (true, "Venter på tjenesten", "Diskene leses av tjenesten (agentic-riggd). Siden fylles når den svarer.".to_string(), "unplug"),
        };
        self.empty.set_visible(show_empty);
        self.list_box.set_visible(!show_empty);
        if let Some(i) = self.empty.first_child().and_then(|c| c.downcast::<Icon>().ok()) {
            i.set_icon(icon);
        }
        set_text(&self.empty_title, title);
        set_text(&self.empty_text, &text);
        let note = data.as_ref().filter(|l| l.source == "sysfs").map(|l| {
            format!(
                "SMART er ikke tilgjengelig: {} Helse, temperatur og SMART-tabellen mangler derfor. Modell, hastighet og ledig plass vises likevel.",
                l.note.clone().unwrap_or_else(|| "UDisks2 svarer ikke.".into())
            )
        });
        self.notice.set_visible(note.is_some() && !show_empty);
        set_text(&self.notice_text, note.as_deref().unwrap_or(""));
    }

    fn render(self: &Rc<Self>) {
        self.render_state();
        let data = self.data.borrow().clone();
        let Some(list) = data else { return };
        let now = rigg_core::now_ms();
        // Første gang: fold ut disker med advarsel eller feil, ellers den første.
        if self.first.replace(false) {
            let mut open = self.open.borrow_mut();
            for d in list.disks.iter().filter(|d| d.health.level >= Level::Warn) {
                open.insert(d.id.clone());
                self.attrs_open.borrow_mut().insert(d.id.clone());
            }
            if open.is_empty()
                && let Some(d) = list.disks.first()
            {
                open.insert(d.id.clone());
            }
        }
        let ids: Vec<&str> = list.disks.iter().map(|d| d.id.as_str()).collect();
        let same = self.rows.borrow().iter().map(|r| r.id.as_str()).eq(ids.iter().copied());
        if !same {
            crate::widgets::clear(&self.list_box);
            for r in self.rows.borrow().iter() {
                if let Some(c) = r.chart.borrow_mut().take() {
                    self.set.remove(&c);
                }
            }
            let mut rows = Vec::new();
            for d in &list.disks {
                let r = Row::new(&d.id);
                let w = Rc::downgrade(self);
                let rw = Rc::downgrade(&r);
                r.head.connect_clicked(move |_| {
                    let (Some(s), Some(r)) = (w.upgrade(), rw.upgrade()) else { return };
                    let open = !r.open.get();
                    if open {
                        s.open.borrow_mut().insert(r.id.clone());
                    } else {
                        s.open.borrow_mut().remove(&r.id);
                    }
                    s.apply_open(&r);
                });
                self.list_box.append(&r.root);
                rows.push(r);
            }
            *self.rows.borrow_mut() = rows;
        }
        let rows = self.rows.borrow().clone();
        for (r, d) in rows.iter().zip(&list.disks) {
            r.set_disk(d);
            // Detaljene byggs på nytt bare når disken er endret (ikke «for N min siden»).
            let mut sd = d.clone();
            sd.smart.updated = sd.smart.updated.map(|u| u / 60_000);
            let sig = serde_json::to_string(&sd).unwrap_or_default();
            if *r.sig.borrow() != sig || r.detail.first_child().is_none() {
                *r.sig.borrow_mut() = sig;
                self.build_detail(r, d, now);
            }
            self.apply_open(r);
        }
        self.live();
    }

    fn build_detail(self: &Rc<Self>, r: &Rc<Row>, d: &Disk, now: i64) {
        crate::widgets::clear(&r.detail);
        // Disk og SMART side om side med like høye kort (stables under
        // 720 px), og Partisjoner i hele bredden under, så tabellen får plass
        // og det ikke blir et tomrom under Disk (runde 8A; før sto
        // Partisjoner under SMART i en masonry).
        let pair = crate::widgets::chart::Cols::new(&[1.0, 1.0], 8, 720);
        pair.add_css_class("dk-pair");
        for w in [disk_panel(d), smart_panel(d, now)] {
            fill_rows(&w);
            pair.append(&w);
        }
        let col = vbox(8, &["dk-detail-panels"]);
        col.append(&pair);
        col.append(&parts_panel(&self.app, d));
        r.detail.append(&col);
        // Grafen lages når disken foldes ut (apply_open), og står over tabellen.
        let holder = vbox(0, &["dk-chart-holder"]);
        r.detail.append(&holder);
        if let Some(c) = r.chart.borrow_mut().take() {
            self.set.remove(&c);
        }
        if !d.smart.attributes.is_empty() {
            let w = Rc::downgrade(self);
            let id = d.id.clone();
            let open = self.attrs_open.borrow().contains(&d.id);
            r.detail.append(&attrs_panel(
                d,
                open,
                Rc::new(move |o| {
                    if let Some(s) = w.upgrade() {
                        if o {
                            s.attrs_open.borrow_mut().insert(id.clone());
                        } else {
                            s.attrs_open.borrow_mut().remove(&id);
                        }
                    }
                }),
            ));
        }
    }

    /// Folder ut eller sammen, og lager eller fjerner grafen.
    fn apply_open(self: &Rc<Self>, r: &Rc<Row>) {
        let open = self.open.borrow().contains(&r.id);
        r.set_open(open);
        let holder = r.detail.first_child().and_then(|m| m.next_sibling()).and_then(|h| h.downcast::<gtk::Box>().ok());
        if open && r.chart.borrow().is_none() {
            let data = self.data.borrow();
            if let (Some(d), Some(h)) = (data.as_ref().and_then(|l| l.disks.iter().find(|d| d.id == r.id)), holder) {
                let c = speed_chart(&self.app, d);
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

    /// De levende tallene i overskriftene.
    fn live(&self) {
        if !self.visible.get() {
            return;
        }
        // Smalt vindu: uten sparklinjer i overskriftene, så siden får plass i 640 px.
        let compact = self.app.window().is_some_and(|w| w.width() > 0 && w.width() < COMPACT_BELOW);
        let tel = self.app.telemetry();
        let data = self.data.borrow();
        for r in self.rows.borrow().iter() {
            let s = tel.as_ref().and_then(|t| t.sys.disks.iter().find(|d| d.id == r.id));
            let fallback = data.as_ref().and_then(|l| l.disks.iter().find(|d| d.id == r.id)).and_then(|d| d.smart.temp);
            r.set_live(&self.app, s, fallback);
            r.spark_r.set_visible(!compact);
            r.spark_w.set_visible(!compact);
            // Og uten tilkoblingsbrikken, så modellnavnet ikke kuttes (Kan 23 i kritikk-1.1.0).
            let mut c = r.chips.first_child();
            while let Some(ch) = c {
                if ch.has_css_class("dk-bus") && ch.is_visible() == compact {
                    ch.set_visible(!compact);
                }
                c = ch.next_sibling();
            }
            let (full, short) = r.meta_text.borrow().clone();
            if !full.is_empty() {
                set_text(&r.meta, if compact { &short } else { &full });
            }
        }
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "dk-page"]);
    // Merknaden når UDisks2 ikke svarer.
    let notice = hbox(8, &["infobar", "dk-notice"]);
    notice.append(&Icon::new("info", 16));
    let notice_text = label("", &[]);
    notice_text.set_wrap(true);
    notice_text.set_hexpand(true);
    notice.append(&notice_text);
    notice.set_visible(false);
    root.append(&notice);
    let list_box = vbox(8, &["dk-list"]);
    root.append(&list_box);
    let empty = vbox(8, &["empty", "dk-empty"]);
    empty.set_valign(gtk::Align::Center);
    empty.set_halign(gtk::Align::Center);
    empty.set_vexpand(true);
    empty.append(&Icon::new("hard-drive", 32));
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
    let foot = label("SMART kommer fra UDisks2, som leser den selv omtrent hvert 10. minutt uten å vekke disker i hvile. Hastighetene er fra /proc/diskstats, ledig plass fra statvfs.", &["muted", "sm", "dk-foot"]);
    foot.set_wrap(true);
    root.append(&foot);
    let actions = hbox(2, &[]);
    let refresh = crate::widgets::button(Some("refresh-cw"), None, &["flat", "icon-only"]);
    crate::widgets::tip(&refresh, "Hent disklista på nytt", Some("F5"));
    refresh.update_property(&[gtk::accessible::Property::Label("Hent disklista på nytt")]);
    actions.append(&refresh);
    let inner = Rc::new(Inner {
        app: app.clone(),
        root,
        actions,
        notice,
        notice_text,
        list_box,
        empty,
        empty_title,
        empty_text,
        rows: RefCell::new(vec![]),
        data: RefCell::new(None),
        open: RefCell::new(HashSet::new()),
        attrs_open: RefCell::new(HashSet::new()),
        first: Cell::new(true),
        visible: Cell::new(false),
        loading: Cell::new(false),
        timer: RefCell::new(None),
        set: ChartSet::new(app),
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
        app.on_telemetry.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.live();
            }
        });
    }
    {
        let w = Rc::downgrade(&inner);
        app.on_settings.connect(move |keys| {
            if let Some(s) = w.upgrade()
                && keys.iter().any(|k| matches!(k.as_str(), "unit" | "decimals" | "memUnit" | "clock"))
            {
                for r in s.rows.borrow().iter() {
                    r.sig.borrow_mut().clear();
                }
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
    Box::new(Disker { inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tekstene() {
        assert_eq!(mbps(Some(12_345_678.0)), "12,3\u{202F}MB/s");
        assert_eq!(mbps(Some(420e6)), "420\u{202F}MB/s");
        assert_eq!(mbps(Some(1.52e9)), "1,52\u{202F}GB/s");
        assert_eq!(mbps(None), "–");
        assert_eq!(ago(0, 30_000), "nå nettopp");
        assert_eq!(ago(0, 4 * 60_000 + 5_000), "for 4 min siden");
        assert_eq!(ago(0, 3 * 3_600_000), "for 3 t siden");
        assert_eq!(temp_level("HDD", Some(56.0)), "warn");
        assert_eq!(temp_level("SSD", Some(56.0)), "");
    }
}
