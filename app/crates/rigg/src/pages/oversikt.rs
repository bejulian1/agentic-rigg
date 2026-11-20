//! Oversikt (runde 8A): hele maskinen på én side, i appens rolige
//! HWiNFO-stil (inspirert av GNOME Resources). Det viktigste øverst:
//!
//! 1. **Seks fliser** for hele systemet: Prosessor, Skjermkort, Minne, VRAM,
//!    Disker og Nettverk, hver med hovedtall, et par undertall og en
//!    sparklinje for siste minutt. Klikk åpner siden for delen.
//! 2. **Tilstand-linja**, bare når noe er verdt å se: kortet svarer ikke,
//!    nedklokking, temperatur over varselgrensene, nesten fullt minne, en
//!    disk med advarsel ([`state_items`]).
//! 3. **Grafene**: Last, Temperaturer, GPU-effekt, Minne og VRAM, Disk-I/O
//!    og Nettverk, med tidsvinduet i toppfeltet (`windows.oversikt`).
//!    Aksene kutter aldri (`chart::math::Axis`).
//! 4. **Skjermkortet**: målerne for temperatur og effekt, og det en
//!    Afterburner-bruker ser etter (P-tilstand, klokker, vifter, PCIe).
//!    Resten står på Grafikkort.
//!
//! Uten NVIDIA-driver viser flisa for skjermkortet det, VRAM og
//! skjermkortdelen skjules, og grafene viser prosessor, minne og disker.

#[cfg(debug_assertions)]
#[path = "oversikt/bilder.rs"]
pub(crate) mod bilder;

use rigg_core::{tr, trf, trn};
use super::Page;
use crate::state::{App, Conn};
use crate::widgets::bar::Bar;
use crate::widgets::chart::{self, ChartOpts, ChartPanel, ChartSet, Colors, Cols, CursorSync, DefOpts, PanelOpts, SeriesDef, math};
use crate::widgets::gauge::{self, Gauge, Mark, Spec};
use crate::widgets::rows::KvList;
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Icon, Panel, Spark, class, hbox, label, set_text, vbox};
use gtk::prelude::*;
use rigg_core::disk::{DiskList, Level as DiskLevel};
use rigg_core::fmt as F;
use rigg_core::model::{DeviceState, GpuSample};
use rigg_core::settings::Thresholds;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

/// Hvor ofte disklista (helse) hentes mens siden vises. Tjenesten lager den
/// selv hvert 60. s mens et vindu er åpent, så dette er bare å lese den.
const DISKS_POLL_S: u32 = 30;
/// Minnet: gul og rød over (% brukt, uten buffer og cache).
const MEM_WARN: f64 = 90.0;
const MEM_CRIT: f64 = 95.0;

// ================================================================ Flisene

/// En flis: navn (med ikon), en kort status til høyre, hovedtall med enhet,
/// to undertall og sparklinja for siste minutt.
struct Tile {
    id: &'static str,
    root: gtk::Button,
    aside: gtk::Label,
    value: gtk::Label,
    unit: gtk::Label,
    sub: [gtk::Label; 2],
    sep: gtk::Label,
    spark: Spark,
    level: Cell<&'static str>,
}

/// Det en flis skal vise nå.
#[derive(Default)]
struct TileView {
    value: String,
    unit: String,
    /// «Ingen driver»: rolig tekst i stedet for et tall.
    na: bool,
    /// Hele flisa (tallet) er gul eller rød.
    level: &'static str,
    /// (tekst, nivå) for undertallene.
    sub: [(String, &'static str); 2],
    aside: (String, &'static str),
    tip: String,
    spark: Vec<(i64, f64)>,
    spark_opts: SparkOpts,
}

type TileDef = (&'static str, &'static str, &'static str, &'static str, &'static str);

fn tiles() -> [TileDef; 6] {
    [
    // (id, navn, ikon, serie, siden klikket åpner)
    ("cpu", tr!("Prosessor"), "cpu", "memctl", "prosessor"),
    ("gpu", tr!("Skjermkort"), "gpu", "load", "grafikkort"),
    ("mem", tr!("Minne"), "memory-stick", "mem", "system"),
    ("vram", "VRAM", "memory-stick", "clock", "grafikkort"),
    ("disk", tr!("Disker"), "hard-drive", "load", "disker"),
    ("net", tr!("Nettverk"), "network", "load", "sensorer?vis=net/rx"),
    ]
}

fn page_name(page: &str) -> &'static str {
    match page {
        "prosessor" => tr!("Prosessor"),
        "grafikkort" => tr!("Grafikkort"),
        "system" => tr!("System"),
        "disker" => tr!("Disker"),
        "kontroller" => tr!("Klokker og effekt"),
        _ => tr!("Sensorer"),
    }
}

fn build_tile(app: &Rc<App>, (id, name, icon, serie, page): TileDef) -> Tile {
    let b = gtk::Button::new();
    b.add_css_class("tile");
    b.add_css_class("ov-tile");
    let col = vbox(0, &[]);
    let head = hbox(6, &["ov-tile-head"]);
    let ic = Icon::new(icon, 14);
    ic.add_css_class("ov-tile-icon");
    head.append(&ic);
    let nl = label(name, &["tile-label"]);
    nl.set_hexpand(true);
    nl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    head.append(&nl);
    let aside = label("", &["tile-aside", "num"]);
    aside.set_visible(false);
    head.append(&aside);
    col.append(&head);
    let vrow = hbox(3, &["ov-tile-value"]);
    let value = label("–", &["tile-value", "num"]);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let unit = label("", &["tile-unit"]);
    unit.set_ellipsize(gtk::pango::EllipsizeMode::End);
    value.set_valign(gtk::Align::Baseline);
    unit.set_valign(gtk::Align::Baseline);
    vrow.append(&value);
    vrow.append(&unit);
    col.append(&vrow);
    let srow = hbox(0, &["ov-tile-sub"]);
    let s0 = label("", &["tile-sub", "num"]);
    let sep = label(" · ", &["tile-sub"]);
    let s1 = label("", &["tile-sub", "num"]);
    s1.set_ellipsize(gtk::pango::EllipsizeMode::End);
    s0.set_ellipsize(gtk::pango::EllipsizeMode::End);
    srow.append(&s0);
    srow.append(&sep);
    srow.append(&s1);
    col.append(&srow);
    let spark = Spark::new(serie, 24);
    spark.set_hexpand(true);
    col.append(&spark);
    b.set_child(Some(&col));
    b.update_property(&[gtk::accessible::Property::Label(name)]);
    let a = app.clone();
    b.connect_clicked(move |_| a.go(page));
    Tile { id, root: b, aside, value, unit, sub: [s0, s1], sep, spark, level: Cell::new("") }
}

impl Tile {
    fn show(&self, v: &TileView) {
        set_text(&self.value, &v.value);
        class(&self.value, "na", v.na);
        set_text(&self.unit, &v.unit);
        for (l, (t, lvl)) in self.sub.iter().zip(&v.sub) {
            set_text(l, t);
            l.set_visible(!t.is_empty());
            for c in ["lvl-warn", "lvl-danger"] {
                class(l, c, gauge::level_class(lvl) == Some(c));
            }
        }
        self.sep.set_visible(!v.sub[0].0.is_empty() && !v.sub[1].0.is_empty());
        set_text(&self.aside, &v.aside.0);
        self.aside.set_visible(!v.aside.0.is_empty());
        for c in ["lvl-warn", "lvl-danger"] {
            class(&self.aside, c, gauge::level_class(v.aside.1) == Some(c));
        }
        if self.level.get() != v.level {
            self.level.set(v.level);
            for l in ["warn", "danger"] {
                class(&self.root, l, l == v.level);
            }
            self.spark.set_level(v.level);
        }
        if self.root.tooltip_text().as_deref() != Some(v.tip.as_str()) {
            self.root.set_tooltip_text(Some(&v.tip));
        }
    }
}

/// «18,4» og «GiB» fra «18,4 GiB».
fn split_unit(s: &str) -> (String, String) {
    match s.split_once(F::NNBSP) {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s.to_string(), String::new()),
    }
}

/// Teksten i verktøytipset: hva flisa viser, og hvor klikket går.
fn tile_tip(what: &str, page: &str) -> String {
    trf!("{what}\nSparklinja er siste minutt. Klikk for {side}.", what = what, side = page_name(page))
}

/// Summen av to serier med samme tidspunkter (lesing og skriving).
fn sum_series(a: &[(i64, f64)], b: &[(i64, f64)]) -> Vec<(i64, f64)> {
    let mut j = 0;
    let mut out = Vec::with_capacity(a.len());
    for &(t, v) in a {
        while j < b.len() && b[j].0 < t {
            j += 1;
        }
        let w = b.get(j).filter(|p| p.0 == t).map(|p| p.1).unwrap_or(0.0);
        out.push((t, v + w));
    }
    out
}

fn tile_views(app: &App, disks: Option<&DiskList>) -> Vec<(&'static str, TileView)> {
    let tele = app.telemetry();
    let th = app.settings().thresholds.clone();
    let gib = app.settings().mem_unit != "dec";
    let mu = if gib { "GiB" } else { "GB" };
    let span = |min: f64| SparkOpts { span_ms: 60_000, fixed: None, min_span: min };
    let pct100 = SparkOpts { span_ms: 60_000, fixed: Some((0.0, 100.0)), min_span: 0.0 };
    let mut out = Vec::new();
    let sys = tele.as_ref().map(|t| &t.sys);

    // ---- Prosessor: last, Tdie og snittklokke ----
    {
        let mut v = TileView { spark: app.recent("cpu/load"), spark_opts: pct100, ..Default::default() };
        let load = sys.and_then(|s| s.cpu.load);
        v.value = F::num(load, F::dec(0));
        v.unit = if load.is_some() { "%".into() } else { String::new() };
        v.level = math::level(load, Some(th.load_warn), None);
        let temp = sys.and_then(rigg_core::sensormeta::cpu_temp);
        v.sub[0] = (if temp.is_some() { F::temp(temp, F::dec(0)) } else { String::new() }, math::level(temp, Some(th.cpu_temp_warn), Some(th.cpu_temp_crit)));
        let mhz: Vec<f64> = sys.map(|s| s.cpu.core_mhz.iter().flatten().copied().collect()).unwrap_or_default();
        let avg = (!mhz.is_empty()).then(|| mhz.iter().sum::<f64>() / mhz.len() as f64);
        v.sub[1] = (if avg.is_some() { F::mhz(avg) } else { String::new() }, "");
        v.tip = tile_tip(tr!("Prosessor: total last, temperatur (Tdie) og snittklokke over trådene."), "prosessor");
        out.push(("cpu", v));
    }

    // ---- Skjermkort: last, temperatur, effekt av grensen og P-tilstand ----
    let info = app.gpu_info();
    let state = info.as_ref().map(|i| i.state);
    let no_card = info.as_ref().is_none_or(|i| i.id.is_empty()) && app.statics().is_none();
    let s: GpuSample = app.gpu_sample().unwrap_or_default();
    {
        let mut v = TileView { spark_opts: pct100, ..Default::default() };
        if let Some(k) = app.gpu_key("gpuUtil") {
            v.spark = app.recent(&k);
        }
        let lost = state == Some(DeviceState::Lost);
        if no_card && matches!(state, Some(DeviceState::NoDriver | DeviceState::DriverUpdated)) {
            let upd = state == Some(DeviceState::DriverUpdated);
            v.value = if upd { tr!("Ny driver").into() } else { tr!("Ingen driver").into() };
            v.na = true;
            v.sub[0] = (if upd { tr!("start maskinen på nytt").into() } else { tr!("fant ikke NVIDIA-driveren").into() }, if upd { "warn" } else { "" });
            v.tip = tile_tip(if upd { tr!("En ny NVIDIA-driver er installert, men den gamle kjører fortsatt.") } else { tr!("Fant ingen NVIDIA-driver. Prosessor, minne og disker vises som vanlig.") }, "grafikkort");
        } else if lost {
            v.value = F::DASH.into();
            v.sub[0] = (tr!("svarer ikke").into(), "danger");
            v.tip = tile_tip(tr!("Skjermkortet svarer ikke. Tjenesten prøver å finne det igjen hvert 10. s."), "grafikkort");
        } else {
            v.value = F::num(s.gpu_util, F::dec(0));
            v.unit = if s.gpu_util.is_some() { "%".into() } else { String::new() };
            v.level = math::level(s.gpu_util, Some(th.load_warn), None);
            let temp_lvl = math::level(s.temp, Some(th.gpu_temp_warn), Some(th.gpu_temp_crit));
            v.sub[0] = (if s.temp.is_some() { F::temp(s.temp, F::dec(0)) } else { String::new() }, temp_lvl);
            let at_limit = matches!((s.power, s.power_limit), (Some(p), Some(l)) if p >= l * 0.97);
            v.sub[1] = (
                match (s.power, s.power_limit) {
                    (Some(p), Some(l)) => trf!("{brukt} av {totalt}", brukt = F::num(Some(p), F::dec(0)), totalt = F::watt(Some(l), 0)),
                    (Some(p), None) => F::watt(Some(p), F::dec(0)),
                    _ => String::new(),
                },
                if at_limit { "warn" } else { "" },
            );
            v.aside = (s.pstate.clone().unwrap_or_default(), "");
            v.tip = tile_tip(tr!("Skjermkort: GPU-last, temperatur, effekt av effektgrensen og P-tilstand (P0 er full ytelse, P8 er tomgang)."), "grafikkort");
        }
        out.push(("gpu", v));
    }

    // ---- Minne: brukt av totalt ----
    {
        let mut v = TileView { spark: app.recent("mem/usedPct"), spark_opts: pct100, ..Default::default() };
        let used = sys.and_then(|s| s.mem.used_kb);
        let total = sys.and_then(|s| s.mem.total_kb);
        let pct = sys.and_then(rigg_core::sensormeta::mem_used_pct);
        (v.value, v.unit) = if used.is_some() { split_unit(&F::kb(used, 1)) } else { (F::DASH.into(), String::new()) };
        v.level = math::level(pct, Some(MEM_WARN), Some(MEM_CRIT));
        v.sub[0] = (if total.is_some() { trf!("av {verdi}", verdi = F::kb(total, 1)) } else { String::new() }, "");
        v.sub[1] = (if pct.is_some() { F::pct(pct, 0) } else { String::new() }, "");
        v.tip = tile_tip(tr!("Minne: det programmene bruker, av totalt. Buffer og cache er ikke med."), "system");
        out.push(("mem", v));
    }

    // ---- VRAM ----
    {
        let mut v = TileView { spark_opts: SparkOpts { span_ms: 60_000, fixed: s.vram_total.map(|t| (0.0, t)), min_span: 0.0 }, ..Default::default() };
        if let Some(k) = app.gpu_key("vramUsed") {
            v.spark = app.recent(&k);
        }
        let frac = s.vram_used.zip(s.vram_total).filter(|(_, t)| *t > 0.0).map(|(u, t)| u / t);
        (v.value, v.unit) = if s.vram_used.is_some() { split_unit(&F::mib(s.vram_used, Some(1), Some(mu))) } else { (F::DASH.into(), String::new()) };
        v.level = math::level(frac, Some(0.85), Some(0.95));
        v.sub[0] = (s.vram_total.map(|t| trf!("av {verdi}", verdi = F::mib(Some(t), Some(1), Some(mu)))).unwrap_or_default(), "");
        v.sub[1] = (frac.map(|f| F::pct(Some(f * 100.0), 0)).unwrap_or_default(), "");
        v.tip = tile_tip(tr!("Videominne brukt på skjermkortet, av totalt. Gult over 85 %, rødt over 95 %."), "grafikkort");
        out.push(("vram", v));
    }

    // ---- Disker: samlet hastighet, varmeste disk og helsa ----
    {
        let mut v = TileView { spark: sum_series(&app.recent("disk/read"), &app.recent("disk/write")), spark_opts: span(5e6), ..Default::default() };
        let list = sys.map(|s| s.disks.as_slice()).unwrap_or(&[]);
        if list.is_empty() {
            v.value = F::DASH.into();
            v.sub[0] = (if tele.is_some() { tr!("ingen disker").into() } else { String::new() }, "");
        } else {
            let rd: f64 = list.iter().filter_map(|d| d.read).sum();
            let wr: f64 = list.iter().filter_map(|d| d.write).sum();
            (v.value, v.unit) = split_unit(&super::disker::mbps(Some(rd + wr)));
            let hot = list.iter().filter_map(|d| d.temp.map(|t| (d, t))).max_by(|a, b| a.1.total_cmp(&b.1));
            v.sub[0] = match hot {
                Some((_, t)) => (trf!("maks {verdi}", verdi = F::temp(Some(t), 0)), ""),
                None => (tr!("uten temperatur").into(), ""),
            };
            // Verktøytipset har ingen tall som endres hvert sekund (hvert nytt
            // verktøytips går også til skjermleseren).
            let lines: Vec<String> = list
                .iter()
                .map(|d| {
                    let h = disks.and_then(|l| l.disks.iter().find(|x| x.id == d.id)).map(|x| x.health.level.text());
                    let t = d.temp.map(|t| F::temp(Some(t), 0));
                    [Some(d.name.clone()), t, h.map(str::to_string)].into_iter().flatten().collect::<Vec<_>>().join(" · ")
                })
                .collect();
            v.tip = trf!("Disker: samlet hastighet (lesing og skriving), varmeste disk og SMART-helsa.\n{helse}\nSparklinja er siste minutt. Klikk for Disker.", helse = lines.join("\n"));
        }
        // Helsa fra get_disks: «Alle OK», «1 advarsel», «1 feil».
        if let Some(l) = disks.filter(|l| !l.disks.is_empty()) {
            let fail = l.disks.iter().filter(|d| d.health.level == DiskLevel::Fail).count();
            let warn = l.disks.iter().filter(|d| d.health.level == DiskLevel::Warn).count();
            let known = l.disks.iter().filter(|d| d.health.level != DiskLevel::Unknown).count();
            v.aside = if fail > 0 {
                (trn!(fail, "{fail} feil", "{fail} med feil", fail = fail), "danger")
            } else if warn > 0 {
                (trn!(warn, "{antall} advarsel", "{antall} advarsler", antall = F::num(Some(warn as f64), 0)), "warn")
            } else if known > 0 {
                (tr!("alle OK").into(), "")
            } else {
                (tr!("helse ukjent").into(), "")
            };
        }
        out.push(("disk", v));
    }

    // ---- Nettverk: ned og opp ----
    {
        let mut v = TileView { spark: app.recent("net/rx"), spark_opts: span(50_000.0), ..Default::default() };
        let (rx, tx) = sys.map(rigg_core::sensormeta::net_totals).unwrap_or((None, None));
        // Enheten etter verdien (kB/s eller MB/s), så små tall ikke blir «0,00».
        if rx.is_some() {
            let (n, u) = split_unit(&F::rate(rx, None));
            v.value = n;
            v.unit = trf!("{u} ned", u = u);
        } else {
            v.value = F::DASH.into();
        }
        v.sub[0] = (tx.map(|t| trf!("{verdi} opp", verdi = F::rate(Some(t), None))).unwrap_or_default(), "");
        v.tip = tile_tip(tr!("Nettverk: mottatt og sendt over de fysiske nettverkskortene."), "sensorer?vis=net/rx");
        out.push(("net", v));
    }
    out
}

// ================================================================ Tilstand-linja

/// Én ting i tilstand-linja.
#[derive(Clone, Debug, PartialEq)]
pub struct StateItem {
    /// «danger», «warn» eller «info».
    pub level: &'static str,
    pub title: String,
    pub text: String,
    pub page: &'static str,
}

/// Det tilstand-linja ser på.
pub struct StateIn<'a> {
    pub gpu_state: Option<DeviceState>,
    /// «18:03» (når kortet sluttet å svare).
    pub gpu_since: Option<String>,
    /// Kortet har ingen data (ingen driver, eller ny driver uten data).
    pub no_card: bool,
    pub gpu_temp: Option<f64>,
    pub slowdown: Option<f64>,
    pub throttle: Option<u64>,
    pub cpu_temp: Option<f64>,
    pub mem_pct: Option<f64>,
    pub th: &'a Thresholds,
    pub disks: Option<&'a DiskList>,
}

/// Klokkebegrensninger som er nedklokking (ikke tomgang eller effektgrensen,
/// som er vanlig under last): HW-nedklokking, termisk og strømbrems.
const THROTTLE_HW: u64 = 0x8 | 0x40 | 0x80;
const THROTTLE_SW_THERMAL: u64 = 0x20;

/// Det som er verdt å se nå, det alvorligste først. Tom liste: linja skjules.
pub fn state_items(i: &StateIn) -> Vec<StateItem> {
    let mut out = Vec::new();
    let item = |level, title: String, text: String, page| StateItem { level, title, text, page };
    match i.gpu_state {
        Some(DeviceState::Lost) => out.push(item(
            "danger",
            tr!("Skjermkortet svarer ikke").into(),
            match &i.gpu_since {
                Some(s) => trf!("siden {s} · tjenesten prøver igjen hvert 10. s", s = s),
                None => tr!("tjenesten prøver igjen hvert 10. s").into(),
            },
            "grafikkort",
        )),
        Some(DeviceState::DriverUpdated) => out.push(item("warn", tr!("Driveren er oppdatert").into(), tr!("start maskinen på nytt for å lese skjermkortet").into(), "grafikkort")),
        Some(DeviceState::NoDriver) if i.no_card => out.push(item("info", tr!("Fant ingen NVIDIA-driver").into(), tr!("prosessor, minne og disker vises som vanlig").into(), "grafikkort")),
        _ => {}
    }
    if i.gpu_state.is_none_or(|s| s == DeviceState::Ok) {
        if let Some(m) = i.throttle.filter(|m| m & (THROTTLE_HW | THROTTLE_SW_THERMAL) != 0) {
            let why: Vec<String> = math::throttle_list(Some(m & (THROTTLE_HW | THROTTLE_SW_THERMAL))).unwrap_or_default().iter().map(|s| s.to_lowercase()).collect();
            out.push(item(if m & THROTTLE_HW != 0 { "danger" } else { "warn" }, tr!("Skjermkortet nedklokker").into(), why.join(", "), "kontroller"));
        }
        if let Some(t) = i.gpu_temp {
            let title = format!("GPU {}", F::temp(Some(t), 0));
            if let Some(slow) = i.slowdown.filter(|s| t >= *s) {
                let verdi = F::temp(Some(slow), 0);
                let detail = if t > slow + 0.5 { trf!("over nedklokkingsgrensen ({verdi})", verdi = verdi) } else { trf!("ved nedklokkingsgrensen ({verdi})", verdi = verdi) };
                out.push(item("danger", title, detail, "grafikkort"));
            } else if t >= i.th.gpu_temp_crit {
                out.push(item("danger", title, trf!("over rød grense ({verdi})", verdi = F::temp(Some(i.th.gpu_temp_crit), 0)), "grafikkort"));
            } else if t >= i.th.gpu_temp_warn {
                out.push(item("warn", title, trf!("over gul grense ({verdi})", verdi = F::temp(Some(i.th.gpu_temp_warn), 0)), "grafikkort"));
            }
        }
    }
    if let Some(t) = i.cpu_temp {
        let title = format!("CPU {}", F::temp(Some(t), 0));
        if t >= i.th.cpu_temp_crit {
            out.push(item("danger", title, trf!("over rød grense ({verdi})", verdi = F::temp(Some(i.th.cpu_temp_crit), 0)), "prosessor"));
        } else if t >= i.th.cpu_temp_warn {
            out.push(item("warn", title, trf!("over gul grense ({verdi})", verdi = F::temp(Some(i.th.cpu_temp_warn), 0)), "prosessor"));
        }
    }
    if let Some(p) = i.mem_pct.filter(|p| *p >= MEM_WARN) {
        out.push(item(if p >= MEM_CRIT { "danger" } else { "warn" }, trf!("Minne {verdi}", verdi = F::pct(Some(p), 0)), tr!("nesten fullt").into(), "system"));
    }
    for d in i.disks.map(|l| l.disks.as_slice()).unwrap_or(&[]) {
        let lvl = match d.health.level {
            DiskLevel::Fail => "danger",
            DiskLevel::Warn => "warn",
            _ => continue,
        };
        let why = d.health.reasons.first().cloned().unwrap_or_else(|| tr!("se SMART").into());
        out.push(item(lvl, format!("{} · {}", d.name, d.health.level.text().to_lowercase()), why, "disker"));
    }
    let rank = |l: &str| match l {
        "danger" => 0,
        "warn" => 1,
        _ => 2,
    };
    out.sort_by_key(|x| rank(x.level));
    out
}

/// Linja under flisene: ikon, inntil tre ting (klikk åpner siden) og «+N».
struct StateStrip {
    root: gtk::Box,
    icon: Icon,
    flow: gtk::FlowBox,
    shown: RefCell<Vec<StateItem>>,
}

const STATE_MAX: usize = 3;

impl StateStrip {
    fn new() -> StateStrip {
        let root = hbox(10, &["ov-state"]);
        let icon = Icon::new("triangle-alert", 16);
        icon.add_css_class("ov-state-icon");
        icon.set_valign(gtk::Align::Start);
        root.append(&icon);
        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_hexpand(true);
        flow.set_max_children_per_line(STATE_MAX as u32 + 1);
        flow.set_column_spacing(0);
        flow.set_row_spacing(0);
        flow.set_homogeneous(false);
        root.append(&flow);
        root.set_visible(false);
        root.set_accessible_role(gtk::AccessibleRole::Status);
        StateStrip { root, icon, flow, shown: RefCell::new(Vec::new()) }
    }

    fn render(&self, app: &Rc<App>, items: Vec<StateItem>) {
        if *self.shown.borrow() == items {
            return;
        }
        while let Some(c) = self.flow.first_child() {
            self.flow.remove(&c);
        }
        let worst = items.first().map(|i| i.level).unwrap_or("info");
        for l in ["warn", "danger", "info"] {
            class(&self.root, l, l == worst);
        }
        self.icon.set_icon(if worst == "info" { "info" } else { "triangle-alert" });
        for (n, it) in items.iter().take(STATE_MAX).enumerate() {
            let b = gtk::Button::new();
            b.add_css_class("flat");
            b.add_css_class("ov-state-item");
            if n > 0 {
                b.add_css_class("not-first");
            }
            b.add_css_class(it.level);
            let bx = hbox(6, &[]);
            let t = label(&it.title, &["ov-state-title", "num"]);
            let x = label(&it.text, &["ov-state-text"]);
            x.set_ellipsize(gtk::pango::EllipsizeMode::End);
            bx.append(&t);
            bx.append(&x);
            b.set_child(Some(&bx));
            b.set_tooltip_text(Some(&trf!("{status} {detalj}. Klikk for {side}.", status = it.title, detalj = it.text, side = page_name(it.page))));
            let (a, page) = (app.clone(), it.page);
            b.connect_clicked(move |_| a.go(page));
            let child = gtk::FlowBoxChild::new();
            child.set_focusable(false);
            child.set_child(Some(&b));
            self.flow.insert(&child, -1);
        }
        if items.len() > STATE_MAX {
            let rest: Vec<String> = items[STATE_MAX..].iter().map(|i| format!("{} {}", i.title, i.text)).collect();
            let more = label(&format!("+{}", items.len() - STATE_MAX), &["ov-state-more", "num"]);
            more.set_tooltip_text(Some(&rest.join("\n")));
            let child = gtk::FlowBoxChild::new();
            child.set_focusable(false);
            child.set_child(Some(&more));
            self.flow.insert(&child, -1);
        }
        let summary = items.iter().map(|i| format!("{} {}", i.title, i.text)).collect::<Vec<_>>().join(". ");
        self.root.update_property(&[gtk::accessible::Property::Label(&trf!("Tilstand: {summary}", summary = summary))]);
        self.root.set_visible(!items.is_empty());
        *self.shown.borrow_mut() = items;
    }
}

// ================================================================ Grafene

/// Diskene med temperatur, varmeste først. Grafen tar de to første (så
/// forklaringen får plass ved siden av tittelen); den bygges bare på nytt når
/// lista over disker med temperatur endres, ikke når rekkefølgen gjør det.
fn disk_temps(app: &App) -> Vec<(String, String)> {
    let Some(t) = app.telemetry() else { return vec![] };
    let mut l: Vec<(&rigg_core::disk::DiskSample, f64)> = t.sys.disks.iter().filter_map(|d| d.temp.map(|x| (d, x))).collect();
    l.sort_by(|a, b| b.1.total_cmp(&a.1));
    l.into_iter().map(|(d, _)| (format!("disk/{}/temp", d.id), d.name.clone())).collect()
}

fn gpu_key(app: &App, f: &str) -> String {
    app.gpu_key(f).unwrap_or_else(|| format!("gpu:-/{f}"))
}

fn panel(app: &Rc<App>, title: &str, series: Vec<SeriesDef>, opts: ChartOpts, win: i64, sync: &Rc<CursorSync>) -> Rc<ChartPanel> {
    ChartPanel::new(app, PanelOpts { title: title.into(), series, window: win, chart: opts, sync: Some(sync.clone()), stats: false, now: false, extra: None, flat: false })
}

/// Last for prosessor og skjermkort i samme graf (prosent, hard grense).
fn load_chart(app: &Rc<App>, gpu: bool, win: i64, sync: &Rc<CursorSync>) -> Rc<ChartPanel> {
    let mut series = Vec::new();
    if gpu {
        series.push(SeriesDef::new(&gpu_key(app, "gpuUtil"), "GPU", "--c-load").fill());
    }
    let cpu = SeriesDef::new("cpu/load", "CPU", "--c-memctl");
    series.push(if gpu { cpu } else { cpu.fill() });
    let mut o = ChartOpts::new(app, || (0.0, 100.0));
    o.unit = Rc::new(|| "%".into());
    o.auto = Rc::new(|| false);
    o.hard = chart::HARD_PCT;
    if gpu {
        let wa = Rc::downgrade(app);
        o.stopped = Rc::new(move || wa.upgrade().and_then(|a| chart::gpu_stopped(&a)));
    }
    panel(app, tr!("Last"), series, o, win, sync)
}

/// Temperaturene: skjermkortet, prosessoren og de varmeste diskene, med
/// nedklokking og avstenging for kortet som streker.
fn temp_chart(app: &Rc<App>, gpu: bool, disks: &[(String, String)], win: i64, sync: &Rc<CursorSync>) -> Rc<ChartPanel> {
    let temp = |v: f64| F::temp_out(Some(v)).unwrap_or(v);
    let mut series = Vec::new();
    if gpu {
        series.push(SeriesDef::new(&gpu_key(app, "temp"), "GPU", "--c-temp").map(temp));
    }
    series.push(SeriesDef::new("cpu/temp", "CPU", "--c-memctl").map(temp));
    const DISK: [&str; 3] = ["--c-clock", "--c-mem", "--c-power"];
    for (i, (k, n)) in disks.iter().take(2).enumerate() {
        series.push(SeriesDef::new(k, n, DISK[i % DISK.len()]).map(temp));
    }
    let mut o = ChartOpts::new(app, chart::temp_range);
    o.unit = Rc::new(|| F::temp_label().into());
    o.min_span = Rc::new(chart::temp_span);
    if gpu {
        let wa = Rc::downgrade(app);
        o.limits = Rc::new(move || wa.upgrade().map(|a| chart::gpu_temp_limits(&a, "GPU-")).unwrap_or_default());
    }
    panel(app, tr!("Temperaturer"), series, o, win, sync)
}

/// Minne og VRAM i prosent av totalt, med GiB i verktøytipset.
fn mem_chart(app: &Rc<App>, gpu: bool, win: i64, sync: &Rc<CursorSync>) -> Rc<ChartPanel> {
    let wa = Rc::downgrade(app);
    let mem_total = move || wa.upgrade().and_then(|a| a.sys()).and_then(|s| s.mem.total_kb);
    let ram = SeriesDef::new("mem/usedPct", tr!("Minne"), "--c-mem").fill().fmt(move |p| match mem_total() {
        Some(t) => format!("{} · {}", F::pct(Some(p), 0), F::kb(Some(t * p / 100.0), 1)),
        None => F::pct(Some(p), 0),
    });
    let mut series = vec![ram];
    if gpu {
        // Totalen fra de faste dataene, så punktene før kortet falt av står
        // der de stod. Leses én gang per tegning, ikke per punkt (`map` kalles
        // for hvert punkt).
        let cell: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
        let wa = Rc::downgrade(app);
        let c2 = cell.clone();
        let total = move || {
            if c2.get().is_none() {
                c2.set(wa.upgrade().and_then(|a| a.with_statics(|s| s.vram_total).flatten().or_else(|| a.gpu_sample().and_then(|s| s.vram_total))).filter(|t| *t > 0.0));
            }
            c2.get()
        };
        let t2 = total.clone();
        series.push(
            SeriesDef::new(&gpu_key(app, "vramUsed"), "VRAM", "--c-clock")
                .map(move |m| total().map(|t| m / t * 100.0).unwrap_or(0.0))
                .fmt(move |p| match t2() {
                    Some(t) => format!("{} · {}", F::pct(Some(p), 0), F::mib(Some(t * p / 100.0), Some(1), None)),
                    None => F::pct(Some(p), 0),
                }),
        );
    }
    let mut o = ChartOpts::new(app, || (0.0, 100.0));
    o.unit = Rc::new(|| "%".into());
    o.auto = Rc::new(|| false);
    o.hard = chart::HARD_PCT;
    panel(app, if gpu { tr!("Minne og VRAM") } else { tr!("Minne") }, series, o, win, sync)
}

// ================================================================ Skjermkortet

struct Clock {
    field: &'static str,
    bar: Bar,
    num: gtk::Label,
}

struct GpuUi {
    root: Cols,
    temp_num: gtk::Label,
    temp_unit: gtk::Label,
    temp_rest: gtk::Label,
    temp_peak: gtk::Label,
    temp_part: gtk::Box,
    temp_gauge: Gauge,
    power_text: gtk::Label,
    power_aside: gtk::Label,
    power_gauge: Gauge,
    pstate: gtk::Label,
    clocks: Vec<Clock>,
    fans: gtk::Label,
    fan_mode: gtk::Label,
    pcie: gtk::Label,
    video: gtk::Label,
}

fn build_gpu(app: &Rc<App>) -> GpuUi {
    let root = Cols::new(&[1.25, 1.0], 8, 720);
    root.add_css_class("ov-gpu");

    // Temperatur og effekt: ett panel med to deler, samme måler.
    let te = Panel::new(tr!("Temperatur og effekt"));
    te.root.add_css_class("ov-te");
    let part = |title: &str| {
        let p = vbox(0, &["ov-part"]);
        let sub = hbox(8, &["ov-sub"]);
        sub.append(&label(title, &["ov-h"]));
        p.append(&sub);
        (p, sub)
    };
    let (tp, tsub) = part(tr!("GPU-temperatur"));
    let tlead = hbox(0, &["temp-lead"]);
    let temp_num = label("–", &["bold", "num"]);
    let temp_unit = label("", &["bold"]);
    let temp_rest = label("", &["muted"]);
    temp_rest.set_ellipsize(gtk::pango::EllipsizeMode::End);
    tlead.append(&temp_num);
    tlead.append(&temp_unit);
    tlead.append(&temp_rest);
    tsub.append(&tlead);
    let temp_peak = label("", &["ov-aside", "num"]);
    temp_peak.set_hexpand(true);
    temp_peak.set_xalign(1.0);
    temp_peak.set_tooltip_text(Some(tr!("Høyeste GPU-temperatur de siste 5 minuttene")));
    tsub.append(&temp_peak);
    let temp_gauge = Gauge::new(app, tr!("GPU-temperatur"));
    // Stolpene og målerne hopper rett til ny verdi: under last endrer klokkene
    // og effekten seg nesten hvert sekund, og glidingen kostet mer enn resten
    // av siden (runde 8A, maalinger.md).
    temp_gauge.set_still(true);
    tp.append(&temp_gauge.root);
    // Panelene i raden er like høye: ledig høyde deles på de to delene.
    tp.set_vexpand(true);
    temp_gauge.root.set_vexpand(true);
    temp_gauge.root.set_valign(gtk::Align::Center);
    te.body.append(&tp);
    let (pp, psub) = part(tr!("GPU-effekt"));
    let power_text = label("–", &["temp-lead", "num"]);
    psub.append(&power_text);
    let power_aside = label("", &["ov-aside", "num"]);
    power_aside.set_hexpand(true);
    power_aside.set_xalign(1.0);
    psub.append(&power_aside);
    let power_gauge = Gauge::new(app, tr!("GPU-effekt"));
    power_gauge.set_still(true);
    pp.append(&power_gauge.root);
    pp.set_vexpand(true);
    power_gauge.root.set_vexpand(true);
    power_gauge.root.set_valign(gtk::Align::Center);
    te.body.append(&pp);
    te.body.set_vexpand(true);
    root.append(&te.root);

    // Skjermkort: det en Afterburner-bruker ser etter. Resten på Grafikkort.
    let gp = Panel::new(tr!("Skjermkort"));
    gp.root.add_css_class("ov-card");
    let more = crate::widgets::button(None, None, &["flat", "sm", "ov-more"]);
    let bx = hbox(4, &[]);
    bx.append(&label(tr!("Grafikkort"), &[]));
    bx.append(&Icon::new("chevron-right", 14));
    more.set_child(Some(&bx));
    more.set_tooltip_text(Some(tr!("Alt driveren og kortet oppgir, på Grafikkort")));
    let a = app.clone();
    more.connect_clicked(move |_| a.go("grafikkort"));
    gp.aside.append(&more);
    let kv = KvList::new();
    gp.body.append(&kv.grid);
    let pstate = kv.add(tr!("P-tilstand"), "–");
    let mut clocks = Vec::new();
    for (name, field) in [(tr!("Kjerneklokke"), "coreClock"), (tr!("Minneklokke"), "memClock")] {
        let bar = Bar::new(app, true);
        bar.set_still(true);
        bar.set_hexpand(true);
        bar.set_valign(gtk::Align::Center);
        let num = label("–", &["num", "meter-num"]);
        num.set_xalign(1.0);
        let m = hbox(12, &["meter"]);
        m.append(&bar);
        m.append(&num);
        m.set_hexpand(true);
        m.set_tooltip_text(Some(tr!("Nå og kortets høyeste (fra driveren)")));
        kv.add_widget(name, &m);
        clocks.push(Clock { field, bar, num });
    }
    let fans = kv.add(tr!("Vifter"), "–");
    let fan_mode = kv.add(tr!("Viftestyring"), "–");
    let pcie = kv.add("PCIe", "–");
    let video = kv.add(tr!("Video (enc / dek)"), "–");
    video.set_tooltip_text(Some(tr!("Last på maskinvarens videokoder (encoder) og -dekoder (decoder).")));
    // Ledig høyde fordeles på radene (panelene i raden er like høye).
    gp.body.set_vexpand(true);
    kv.grid.set_vexpand(true);
    let mut c = kv.grid.first_child();
    while let Some(w) = c {
        w.set_vexpand(true);
        c = w.next_sibling();
    }
    root.append(&gp.root);
    GpuUi { root, temp_num, temp_unit, temp_rest, temp_peak, temp_part: tp, temp_gauge, power_text, power_aside, power_gauge, pstate, clocks, fans, fan_mode, pcie, video }
}

/// «Ikke støttet» for et felt kortet ikke kan lese (NOT_SUPPORTED).
fn unsupported(s: &GpuSample, field: &str) -> bool {
    s.na.iter().any(|n| n == field || (field == "fan" && n == "fans"))
}

fn render_gpu(app: &Rc<App>, g: &GpuUi, peak: &RefCell<VecDeque<(i64, f64)>>) {
    let lost = app.gpu_info().is_some_and(|i| i.state != DeviceState::Ok);
    let s: GpuSample = if lost { GpuSample::default() } else { app.gpu_sample().unwrap_or_default() };
    let st = app.statics().unwrap_or_default();
    let t = app.settings().thresholds.clone();

    // ---- Temperatur ----
    let slow = st.temp_slowdown.unwrap_or(97.0);
    let shut = st.temp_shutdown.unwrap_or((slow + 3.0).max(100.0));
    let target = st.temp_target.unwrap_or(83.0);
    if let Some(temp) = s.temp {
        let pk = peak.borrow().iter().map(|p| p.1).fold(temp, f64::max);
        set_text(&g.temp_peak, &trf!("maks 5 min: {verdi}", verdi = F::temp(Some(pk), F::dec(0))));
        let lv = math::level(Some(temp), Some(t.gpu_temp_warn), Some(t.gpu_temp_crit));
        if !g.temp_num.has_css_class("egg-42") {
            set_text(&g.temp_num, &F::temp_num(Some(temp), F::dec(0)));
        }
        set_text(&g.temp_unit, &format!("{}{}", F::NNBSP, F::temp_label()));
        for c in ["lvl-warn", "lvl-danger"] {
            for l in [&g.temp_num, &g.temp_unit, &g.temp_rest] {
                class(l, c, gauge::level_class(lv) == Some(c));
            }
        }
        let margin = slow - temp;
        // Det viktigste først, og bare det: ved nedklokking er varselgrensene
        // uten betydning.
        let mut rest = if margin > 0.0 {
            trf!(" · {verdi} under nedklokking", verdi = F::temp_delta(margin, 0))
        } else if margin < -0.5 {
            tr!(" · over nedklokkingsgrensen").into()
        } else {
            tr!(" · ved nedklokkingsgrensen").into()
        };
        match lv {
            _ if margin <= 0.0 => {}
            "danger" => rest.push_str(&trf!(" · over rød grense ({verdi})", verdi = F::temp(Some(t.gpu_temp_crit), 0))),
            "warn" => rest.push_str(&trf!(" · over gul grense ({verdi})", verdi = F::temp(Some(t.gpu_temp_warn), 0))),
            _ => {}
        }
        set_text(&g.temp_rest, &rest);
        // Fra 20 °C (tomgang er ca. 30 °C), men lavere når kortet er kaldere.
        let min = 20f64.min((temp / 10.0).floor() * 10.0);
        g.temp_gauge.set(Spec {
            min,
            max: shut.max(temp),
            value: Some(temp),
            level: lv,
            marks: {
                let mut m = vec![Mark { v: target, num: F::temp(Some(target), 0), name: tr!("Mål").into() }, Mark { v: slow, num: F::temp(Some(slow), 0), name: tr!("Nedklokking").into() }];
                // Over avstengingen slutter skalaen på verdien; avstengingen står som merke.
                if temp > shut {
                    m.push(Mark { v: shut, num: F::temp(Some(shut), 0), name: tr!("Avstenging").into() });
                }
                m
            },
            end: Some((F::temp(Some(shut.max(temp)), 0), if temp > shut { tr!("Nå").into() } else { tr!("Avstenging").into() })),
            tip: trf!("Fra {min} til avstenging ({avstenging}). Over målet ({target}) senker kortet klokken litt, ved {nedklokking} tvinger det nedklokking. Gul og rød fyll betyr over varselgrensene {gul} og {red} (Innstillinger › Grafer og sensorer).", min = F::temp(Some(min), 0), avstenging = F::temp(Some(shut), 0), target = F::temp(Some(target), 0), nedklokking = F::temp(Some(slow), 0), gul = F::temp(Some(t.gpu_temp_warn), 0), red = F::temp(Some(t.gpu_temp_crit), 0)),
        });
    } else {
        if !g.temp_num.has_css_class("egg-42") {
            set_text(&g.temp_num, if unsupported(&s, "temp") { tr!("Ikke støttet") } else { F::DASH });
        }
        set_text(&g.temp_unit, "");
        set_text(&g.temp_rest, "");
        set_text(&g.temp_peak, "");
        g.temp_gauge.clear();
    }
    // Påskeegg 4: nøyaktig 42 °C.
    crate::easter::mark42(app, &g.temp_num, s.temp, &g.temp_part);

    // ---- Effekt ----
    if let Some(p) = s.power {
        let at_limit = s.power_limit.is_some_and(|l| p >= l * 0.97);
        set_text(&g.power_text, &match s.power_limit {
            Some(l) => trf!("{trekk} av {grense}", trekk = F::watt(Some(p), F::dec(0)), grense = F::watt(Some(l), 0)),
            None => F::watt(Some(p), F::dec(0)),
        });
        class(&g.power_text, "lvl-warn", at_limit);
        let mut marks = Vec::new();
        if let Some(l) = s.power_limit {
            let is_def = st.power_default.is_some_and(|d| d.round() == l.round());
            marks.push(Mark { v: l, num: F::watt(Some(l), 0), name: if is_def { tr!("Grense (standard)").into() } else { tr!("Grense").into() } });
            if let Some(d) = st.power_default.filter(|d| d.round() != l.round()) {
                marks.push(Mark { v: d, num: F::watt(Some(d), 0), name: tr!("Standard").into() });
            }
        }
        let range = match (st.power_min, st.power_max) {
            (Some(a), Some(b)) => trf!(" Grensen kan settes fra {min} til {maks}.", min = F::watt(Some(a), 0), maks = F::watt(Some(b), 0)),
            _ => String::new(),
        };
        let pmax = st.power_max.or(s.power_limit).unwrap_or(300.0);
        if let Some(m) = st.power_max.filter(|m| p > *m) {
            marks.push(Mark { v: m, num: F::watt(Some(m), 0), name: tr!("Kortets maks").into() });
        }
        g.power_gauge.set(Spec {
            min: 0.0,
            max: pmax.max(p),
            value: Some(p),
            level: if at_limit { "warn" } else { "" },
            marks,
            end: st.power_max.map(|m| (F::watt(Some(m.max(p)), 0), if p > m { tr!("Nå").into() } else { tr!("Kortets maks").into() })),
            tip: trf!("Trekket nå mot effektgrensen, snitt fra energitelleren. Gult fyll betyr at kortet er ved grensen og holder igjen.{range}", range = range),
        });
        set_text(&g.power_aside, &s.power_limit.map(|l| trf!("{verdi} av grensen", verdi = F::pct(Some(p / l * 100.0), 0))).unwrap_or_default());
    } else {
        set_text(&g.power_text, if unsupported(&s, "power") { tr!("Ikke støttet") } else { F::DASH });
        class(&g.power_text, "lvl-warn", false);
        set_text(&g.power_aside, "");
        g.power_gauge.clear();
    }

    // ---- Skjermkort ----
    let (th, full) = math::throttle_short(s.throttle, 1);
    let has_reason = s.throttle.is_some_and(math::throttle_has_reason);
    let th_text = s.throttle.map(|_| if has_reason { th } else { tr!("ingen begrensning").to_string() });
    let ps = [s.pstate.clone(), th_text.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    set_text(&g.pstate, if ps.is_empty() { F::DASH } else { &ps });
    class(&g.pstate, "warn-text", math::throttle_serious(s.throttle));
    let mut ptip = String::from(tr!("P0 er full ytelse, P8 er dyp tomgang. Kortet bytter selv."));
    if !full.is_empty() {
        ptip.push_str(&trf!("\nBegrensninger nå: {full}.", full = full));
    } else if let Some(x) = th_text.filter(|_| has_reason) {
        ptip.push_str(&trf!("\nKlokken begrenses nå av: {x}.", x = x));
    }
    if s.throttle_inferred {
        ptip.push_str(tr!("\nI P8 leses ikke begrensningene (kallet koster 25 ms og vekker kortet)."));
    }
    if g.pstate.tooltip_text().as_deref() != Some(ptip.as_str()) {
        g.pstate.set_tooltip_text(Some(&ptip));
    }
    for c in &g.clocks {
        let now_v = rigg_core::sensor_key::gpu_field(&s, c.field);
        let max = if c.field == "coreClock" { st.core_max } else { st.mem_max };
        set_text(&c.num, &match (now_v, max) {
            (Some(n), Some(m)) => format!("{} / {}", F::num(Some(n), 0), F::mhz(Some(m))),
            (Some(n), None) => F::mhz(Some(n)),
            _ => F::DASH.into(),
        });
        c.bar.set(now_v.zip(max.filter(|m| *m > 0.0)).map(|(n, m)| n / m), "");
    }
    let rpm: Vec<String> = s.fan_rpm.iter().flatten().map(|r| F::num(Some(*r), 0)).collect();
    let fans = match (s.fan, rpm.is_empty()) {
        (Some(f), false) => trf!("{verdi1} · {verdi2} o/min", verdi1 = F::pct(Some(f), 0), verdi2 = rpm.join(" / ")),
        (Some(f), true) => F::pct(Some(f), 0),
        _ if unsupported(&s, "fan") => tr!("Ikke støttet").into(),
        _ => F::DASH.into(),
    };
    set_text(&g.fans, &fans);
    let (mode, tip) = match rigg_core::model::fan_control(&s) {
        rigg_core::model::FanControl::Driver => (tr!("Auto · driveren styrer"), tr!("Driveren styrer viftene etter temperaturen, som uten appen.")),
        rigg_core::model::FanControl::Manual => (tr!("Manuell · satt utenfor appen"), tr!("Minst én vifte er satt til fast fart utenfor appen.")),
        rigg_core::model::FanControl::Unknown => (F::DASH, tr!("Viftestyringen er ikke lest ennå, eller kortet oppgir den ikke.")),
    };
    set_text(&g.fan_mode, mode);
    if g.fan_mode.tooltip_text().as_deref() != Some(tip) {
        g.fan_mode.set_tooltip_text(Some(tip));
    }
    let pcie = if lost { F::DASH.to_string() } else { math::pcie_text(st.pcie_width_max, st.pcie_gen_max, s.pcie_width, s.pcie_gen).unwrap_or_else(|| F::DASH.into()) };
    set_text(&g.pcie, &pcie);
    let ex = math::pcie_explain(st.pcie_gen_max, s.pcie_gen);
    let ex = if ex.is_empty() { tr!("PCIe-lenken mellom kortet og hovedkortet.").to_string() } else { ex };
    if g.pcie.tooltip_text().as_deref() != Some(ex.as_str()) {
        g.pcie.set_tooltip_text(Some(&ex));
    }
    set_text(&g.video, &if s.enc_util.is_none() && s.dec_util.is_none() { F::DASH.to_string() } else { format!("{} / {}", F::pct(s.enc_util, 0), F::pct(s.dec_util, 0)) });
}

// ================================================================ Siden

/// Bygger grafene for (skjermkort, diskene med temperatur).
type Rebuild = Rc<dyn Fn(bool, Vec<(String, String)>)>;

struct Ui {
    tiles: Vec<Tile>,
    strip: StateStrip,
    gpu: GpuUi,
    /// Grafene er bygd for: (skjermkort, diskene med temperatur).
    built_for: RefCell<(bool, Vec<(String, String)>)>,
    rebuild: Rebuild,
    disks: RefCell<Option<DiskList>>,
}

pub struct Oversikt {
    root: gtk::Box,
    actions: gtk::Box,
    set: Rc<ChartSet>,
    visible: Rc<Cell<bool>>,
    refresh: Rc<dyn Fn()>,
    load_disks: Rc<dyn Fn()>,
    disk_timer: RefCell<Option<gtk::glib::SourceId>>,
}

impl Page for Oversikt {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.actions.clone().upcast())
    }

    fn show(&self, _q: Option<&str>) {
        self.visible.set(true);
        self.set.set_visible(true);
        (self.refresh)();
        (self.load_disks)();
        if self.disk_timer.borrow().is_none() {
            let f = self.load_disks.clone();
            *self.disk_timer.borrow_mut() = Some(gtk::glib::timeout_add_seconds_local(DISKS_POLL_S, move || {
                f();
                gtk::glib::ControlFlow::Continue
            }));
        }
    }

    fn hide(&self) {
        self.visible.set(false);
        self.set.set_visible(false);
        if let Some(t) = self.disk_timer.borrow_mut().take() {
            t.remove();
        }
    }
}

/// Har vi et kort å vise (ikke «ingen driver»)?
fn has_gpu(app: &App) -> bool {
    let info = app.gpu_info();
    let no_card = info.as_ref().is_none_or(|i| i.id.is_empty()) && app.statics().is_none();
    !(no_card && matches!(info.map(|i| i.state), Some(DeviceState::NoDriver | DeviceState::DriverUpdated)))
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "ov-page"]);

    // ---- Flisene: seks spalter, tre under 900 px og to under 560 px ----
    let tiles_box = Cols::new(&[1.0; 6], 0, 0);
    tiles_box.add_css_class("tiles");
    tiles_box.set_breaks(&[(900, 3), (560, 2)]);
    tiles_box.set_bleed();
    let tiles: Vec<Tile> = tiles().into_iter().map(|d| build_tile(app, d)).collect();
    for t in &tiles {
        tiles_box.append(&t.root);
    }
    root.append(&tiles_box);

    // ---- Tilstand ----
    let strip = StateStrip::new();
    root.append(&strip.root);

    // ---- Grafene: tre spalter på brede skjermer, to, og én under 700 px ----
    let set = ChartSet::new(app);
    let charts = Cols::new(&[1.0, 1.0, 1.0], 8, 700);
    charts.set_breaks(&[(1240, 2)]);
    charts.add_css_class("ov-charts");
    let sync = {
        let a = Rc::downgrade(app);
        CursorSync::new(move || a.upgrade().is_none_or(|a| a.settings().chart_sync))
    };
    let rebuild: Rebuild = {
        let (app, set, charts, sync) = (app.clone(), set.clone(), charts.clone(), sync.clone());
        Rc::new(move |gpu: bool, disks: Vec<(String, String)>| {
            charts.clear();
            set.clear();
            let win = app.settings().windows.oversikt as i64;
            let mut list = vec![load_chart(&app, gpu, win, &sync), temp_chart(&app, gpu, &disks, win, &sync)];
            if gpu {
                list.push(chart::def_panel(&app, "power", DefOpts { window: win, sync: Some(sync.clone()), title: Some(tr!("GPU-effekt").into()), stats: false, now: false }));
            }
            list.push(mem_chart(&app, gpu, win, &sync));
            list.push(chart::def_panel(&app, "diskIo", DefOpts { window: win, sync: Some(sync.clone()), title: Some(tr!("Disk-I/O").into()), stats: false, now: false }));
            list.push(chart::def_panel(&app, "net", DefOpts { window: win, sync: Some(sync.clone()), title: Some(tr!("Nettverk").into()), stats: false, now: false }));
            for p in list {
                // Flytter seg med prøvene, i samme bilde som flisene (≤ 2 % i ro).
                p.chart.set_calm(true);
                p.chart.set_size_request(-1, 150);
                p.root.set_vexpand(false);
                charts.append(&p.root);
                set.add(&p);
            }
        })
    };
    root.append(&charts);

    // ---- Skjermkortet ----
    let gpu = build_gpu(app);
    root.append(&gpu.root);

    #[cfg(debug_assertions)]
    {
        let set = set.clone();
        bilder::hook("oversikt.markor", move |a| {
            if let Some(p) = set.list().get(1) {
                p.chart.set_hover_frac(a.parse().ok());
            }
        });
    }

    // ---- Toppfeltet: tidsvindu for grafene ----
    let actions = hbox(4, &[]);
    let dd = {
        let a = app.clone();
        chart::window_dropdown(app.settings().windows.oversikt as i64, tr!("Tidsvindu for grafene på Oversikt"), move |sec| {
            a.set(json!({"windows": {"oversikt": sec}}));
        })
    };
    actions.append(&dd);

    let ui = Rc::new(Ui { tiles, strip, gpu, built_for: RefCell::new((true, Vec::new())), rebuild, disks: RefCell::new(None) });
    let g0 = has_gpu(app);
    let d0 = disk_temps(app);
    (ui.rebuild)(g0, d0.clone());
    *ui.built_for.borrow_mut() = (g0, d0);

    let peak: Rc<RefCell<VecDeque<(i64, f64)>>> = Rc::new(RefCell::new(VecDeque::new()));
    let visible = Rc::new(Cell::new(false));

    let refresh: Rc<dyn Fn()> = {
        let (app, ui, peak) = (app.clone(), ui.clone(), peak.clone());
        Rc::new(move || render(&app, &ui, &peak))
    };
    {
        let (r, v, app2, peak2) = (refresh.clone(), visible.clone(), app.clone(), peak.clone());
        app.on_telemetry.connect(move |_| {
            // Toppen for 5 min samles også når siden ikke vises.
            if let (Some(s), Some(t)) = (app2.gpu_sample(), app2.telemetry())
                && let Some(temp) = s.temp
            {
                let mut p = peak2.borrow_mut();
                if p.back().is_none_or(|b| b.0 < t.t) {
                    p.push_back((t.t, temp));
                }
                while p.front().is_some_and(|f| f.0 < t.t - 300_000) {
                    p.pop_front();
                }
            }
            if v.get() {
                r();
            }
        });
    }
    {
        let r = refresh.clone();
        let (dd, set2) = (dd.clone(), set.clone());
        let a = app.clone();
        app.on_settings.connect(move |k| {
            if k.iter().any(|k| k == "windows") {
                chart::set_window_dropdown(&dd, a.settings().windows.oversikt as i64);
                let win = a.settings().windows.oversikt as i64;
                for p in set2.list() {
                    p.set_window(win);
                }
            }
            if k.iter().any(|k| matches!(k.as_str(), "thresholds" | "memUnit" | "unit" | "decimals" | "netUnit")) {
                r();
            }
        });
    }
    {
        let (ui, a) = (ui.clone(), app.clone());
        app.on_theme.connect(move |_| {
            let c = Colors::from_app(&a);
            ui.gpu.temp_gauge.set_colors(&c);
            ui.gpu.power_gauge.set_colors(&c);
        });
    }
    {
        let r = refresh.clone();
        app.on_devices.connect(move |_| r());
    }
    {
        // Kortet byttes: grafene bygges på nytt (ChartSet sier fra).
        let (ui, a) = (ui.clone(), app.clone());
        set.connect_gpu(move || {
            let (g, d) = (has_gpu(&a), disk_temps(&a));
            (ui.rebuild)(g, d.clone());
            *ui.built_for.borrow_mut() = (g, d);
        });
    }
    // Disklista (helsa) fra tjenesten; billig, tjenesten har den allerede.
    let load_disks: Rc<dyn Fn()> = {
        let (a, ui, r, v) = (app.clone(), ui.clone(), refresh.clone(), visible.clone());
        Rc::new(move || {
            if !a.client.is_connected() || !v.get() {
                return;
            }
            let (ui, r) = (ui.clone(), r.clone());
            a.client.request("get_disks", json!({}), move |res| {
                let Ok(v) = res else { return };
                if v["pending"].as_bool() == Some(true) {
                    return;
                }
                if let Ok(l) = serde_json::from_value::<DiskList>(v) {
                    *ui.disks.borrow_mut() = Some(l);
                    r();
                }
            });
        })
    };
    {
        let (l, r) = (load_disks.clone(), refresh.clone());
        app.on_conn.connect(move |c| {
            if *c == Conn::Connected {
                l();
            }
            r();
        });
    }
    // Toppen for 5 min fra historikken, så «maks 5 min» stemmer straks.
    {
        let (a, peak) = (app.clone(), peak.clone());
        let fill = move || {
            let Some(k) = a.gpu_key("temp") else { return };
            let p = peak.clone();
            a.client.request("get_history", json!({"window": 300, "keys": [k.clone()], "points": 300}), move |res| {
                let Ok(v) = res else { return };
                let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
                let mx: Vec<Option<f64>> = serde_json::from_value(v["series"][&k]["max"].clone()).unwrap_or_default();
                let mut q = p.borrow_mut();
                let first = q.front().map(|f| f.0).unwrap_or(i64::MAX);
                let old: Vec<(i64, f64)> = t.iter().zip(mx).filter_map(|(t, m)| m.map(|m| (*t, m))).filter(|x| x.0 < first).collect();
                for x in old.into_iter().rev() {
                    q.push_front(x);
                }
            });
        };
        fill();
        app.on_conn.connect(move |c| {
            if *c == Conn::Connected {
                fill();
            }
        });
    }
    refresh();
    #[cfg(debug_assertions)]
    bilder::start(app);
    Box::new(Oversikt { root, actions, set, visible, refresh, load_disks, disk_timer: RefCell::new(None) })
}

fn render(app: &Rc<App>, ui: &Ui, peak: &RefCell<VecDeque<(i64, f64)>>) {
    let gpu = has_gpu(app);
    // Grafene bygges på nytt når kortet kommer eller går, eller diskene med
    // temperatur blir andre (sjelden).
    {
        let disks = disk_temps(app);
        let same_disks = {
            let b = ui.built_for.borrow();
            let mut a: Vec<&String> = b.1.iter().map(|d| &d.0).collect();
            let mut c: Vec<&String> = disks.iter().map(|d| &d.0).collect();
            a.sort();
            c.sort();
            a == c
        };
        if ui.built_for.borrow().0 != gpu || !same_disks {
            (ui.rebuild)(gpu, disks.clone());
            *ui.built_for.borrow_mut() = (gpu, disks);
        }
    }
    let disks = ui.disks.borrow();
    let now = app.telemetry().map(|t| t.t).unwrap_or(0);
    for (id, v) in tile_views(app, disks.as_ref()) {
        let Some(t) = ui.tiles.iter().find(|t| t.id == id) else { continue };
        t.root.set_visible(id != "vram" || gpu);
        t.show(&v);
        t.spark.set_points(&v.spark, now, v.spark_opts);
    }
    ui.gpu.root.set_visible(gpu);

    // ---- Tilstand ----
    let info = app.gpu_info();
    let s = app.gpu_sample();
    let sys = app.telemetry();
    let th = app.settings().thresholds.clone();
    let items = state_items(&StateIn {
        gpu_state: info.as_ref().map(|i| i.state),
        gpu_since: info.as_ref().filter(|i| i.since > 0).map(|i| F::since(crate::app::local_time(i.since), crate::app::local_now())),
        no_card: !gpu,
        gpu_temp: s.as_ref().and_then(|s| s.temp),
        slowdown: app.statics().and_then(|s| s.temp_slowdown),
        throttle: s.as_ref().filter(|s| !s.throttle_inferred).and_then(|s| s.throttle),
        cpu_temp: sys.as_ref().and_then(|t| rigg_core::sensormeta::cpu_temp(&t.sys)),
        mem_pct: sys.as_ref().and_then(|t| rigg_core::sensormeta::mem_used_pct(&t.sys)),
        th: &th,
        disks: disks.as_ref(),
    });
    ui.strip.render(app, items);

    if gpu {
        render_gpu(app, &ui.gpu, peak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::disk::{Disk, Health};

    fn input<'a>(th: &'a Thresholds) -> StateIn<'a> {
        StateIn { gpu_state: Some(DeviceState::Ok), gpu_since: None, no_card: false, gpu_temp: Some(40.0), slowdown: Some(97.0), throttle: Some(1), cpu_temp: Some(38.0), mem_pct: Some(30.0), th, disks: None }
    }

    #[test]
    fn tilstand_er_tom_når_alt_er_rolig() {
        let th = Thresholds::default();
        assert!(state_items(&input(&th)).is_empty());
        // Effektgrensen under last er vanlig, ikke en tilstand.
        assert!(state_items(&StateIn { throttle: Some(0x4), ..input(&th) }).is_empty());
    }

    #[test]
    fn tilstand_viser_det_som_er_verdt_å_se_alvorligst_først() {
        let th = Thresholds::default();
        let disks = DiskList {
            disks: vec![Disk { name: "sdb".into(), health: Health { level: DiskLevel::Warn, reasons: vec!["2 dårlige sektorer".into()] }, ..Default::default() }],
            ..Default::default()
        };
        let items = state_items(&StateIn { gpu_temp: Some(110.0), throttle: Some(0x40), cpu_temp: Some(82.0), mem_pct: Some(93.0), disks: Some(&disks), ..input(&th) });
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(items[0].level, "danger");
        assert!(titles.contains(&"Skjermkortet nedklokker"), "{titles:?}");
        assert!(items.iter().any(|i| i.title.starts_with("GPU ") && i.text.contains("nedklokkingsgrensen")));
        assert!(items.iter().any(|i| i.title.starts_with("CPU ") && i.level == "warn"));
        assert!(items.iter().any(|i| i.title == "sdb · advarsel" && i.text == "2 dårlige sektorer" && i.page == "disker"));
        assert!(items.iter().any(|i| i.title.starts_with("Minne") && i.page == "system"));
        let ranks: Vec<u8> = items.iter().map(|i| if i.level == "danger" { 0 } else { 1 }).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "alvorligst først: {items:?}");
    }

    #[test]
    fn tilstand_når_kortet_er_borte_eller_driveren_mangler() {
        let th = Thresholds::default();
        let lost = state_items(&StateIn { gpu_state: Some(DeviceState::Lost), gpu_since: Some("18:03".into()), gpu_temp: None, ..input(&th) });
        assert_eq!(lost[0].title, "Skjermkortet svarer ikke");
        assert!(lost[0].text.starts_with("siden 18:03"));
        let none = state_items(&StateIn { gpu_state: Some(DeviceState::NoDriver), no_card: true, gpu_temp: None, throttle: None, ..input(&th) });
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].level, "info");
    }

    #[test]
    fn disk_hastighet_summeres_på_samme_tidspunkt() {
        assert_eq!(sum_series(&[(1, 1.0), (2, 2.0), (3, 3.0)], &[(2, 10.0), (3, 20.0)]), vec![(1, 1.0), (2, 12.0), (3, 23.0)]);
        assert_eq!(split_unit(&format!("18,4{}GiB", F::NNBSP)), ("18,4".into(), "GiB".into()));
    }
}
