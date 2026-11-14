//! Sidefeltet (port av demoens `.side-head` og `.sidebar`): appikon og navn,
//! maskinhodet, sidene i tre seksjoner, statusblokken og foten med
//! Innstillinger, Tastatursnarveier og knappen som legger sammen.
//!
//! Maskinhodet (runde 6B) viser skjermkortet og prosessoren med ikon, navn og
//! en kort linje under. Skjermkortet går til Grafikkort, prosessoren til
//! Prosessor. Detaljer og kortvalg åpnes under Skjermkortstyring. Bare feil vises,
//! diskret, i linja under navnet (og som et lite merke i ikonlista). Under
//! [`COMPACT_H`] px høyde skjules linjene under navnene.
//!
//! Styring-seksjonen viser status når den ikke er klar til å endre det ekte
//! kortet. Merket skjules først når den autoritative skriveporten er klar.
//!
//! Ikonlista (rail): når brukeren har lagt sidefeltet sammen (Ctrl+B,
//! `sideCollapsed`), og alltid under 860 px. Navnene står i verktøytipset.
//! Statusblokken viser korte tall med enheten under og et kildemerke (GPU,
//! CPU, RAM …) over hver gruppe. Ikonlista ruller aldri: får ikke alle
//! verdiene plass, vises så mange som får plass.
//!
//! **For runde 2:** [`Sidebar::set_log_indicator`] (2B, loggstatusen ved
//! appnavnet) og [`status_label`] (navn og format for en festet sensor).

use crate::pages::{PAGES, Section, section_title};
use crate::state::App;
use crate::widgets::icon::Spin;
use crate::widgets::popover::Menu;
use crate::widgets::rows::{dot, set_dot};
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Icon, Spark, class, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::subclass::prelude::ObjectSubclassIsExt;
use rigg_core::fmt as F;
use rigg_core::model::DeviceState;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

pub const SIDE_W: i32 = 236;
pub const RAIL_W: i32 = 56;
/// Lavere vindu enn dette: maskinhodet viser bare navnene.
pub const COMPACT_H: i32 = 760;

/// Merket ved Styring-seksjonen (runde 7A): «Bare visning» når hjelperen
/// for endring ikke er på, ellers «Endring på · effekt» (eller «· låst» mot
/// det ekte kortet i 1.2.0), «Endring: feil» eller «Endring: slå på igjen».
/// Teksten og verktøytipset står i `widgets::locked`.
fn lock_badge(app: &crate::state::App) -> gtk::Box {
    let (text, icon, tiptext) = crate::widgets::locked::badge(app);
    let kind = if icon == "triangle-alert" || icon == "circle-alert" { "warn" } else { "quiet" };
    let t = crate::widgets::rows::tag(&text, Some(icon), kind);
    t.add_css_class("nav-lock");
    t.set_visible(!crate::widgets::locked::control_ready(app));
    t.set_tooltip_text(Some(&tiptext));
    t.set_can_target(true);
    t.update_property(&[gtk::accessible::Property::Label(&text), gtk::accessible::Property::Description(&tiptext)]);
    t
}

/// Underteksten følger samme status som merket, også ved kortbytte og feil.
fn control_description(app: &App, hint: &str) -> String {
    if crate::widgets::locked::control_ready(app) {
        hint.to_string()
    } else {
        format!("{hint}. {}", crate::widgets::locked::badge(app).2)
    }
}

/// Hva en side på Styring styrer (verktøytipset i sidefeltet).
pub fn page_hint(id: &str) -> Option<&'static str> {
    match id {
        "kontroller" => Some("Skjermkortets effektgrense, klokke-offset og temperaturmål"),
        "viftekurve" => Some("Skjermkortets viftekurve og turtall"),
        "lys" => Some("Effekter og farger på skjermkortet"),
        _ => None,
    }
}

/// Tasten for side nummer `i` (fra 0): 1–9, og 0 for den tiende.
pub fn page_key(i: usize) -> Option<String> {
    match i {
        0..=8 => Some((i + 1).to_string()),
        9 => Some("0".into()),
        _ => None,
    }
}

/// Kort prosessornavn til maskinhodet: «AMD Ryzen 7 2700X Eight-Core
/// Processor» blir «Ryzen 7 2700X», «Intel(R) Core(TM) i7-8700K CPU @
/// 3.70GHz» blir «Core i7-8700K».
pub fn short_cpu_name(name: &str) -> String {
    let mut n = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    for cut in [" CPU @", " @ ", " with Radeon", " w/ Radeon"] {
        if let Some(i) = n.find(cut) {
            n.truncate(i);
        }
    }
    let words: Vec<&str> = n.split_whitespace().collect();
    let mut out: Vec<&str> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let lower = w.to_ascii_lowercase();
        let ordinal = lower.ends_with("th") || lower.ends_with("st") || lower.ends_with("nd") || lower.ends_with("rd");
        let skip = matches!(lower.as_str(), "amd" | "intel" | "processor" | "cpu" | "gen")
            || lower.ends_with("-core")
            || (ordinal && lower.chars().next().is_some_and(|c| c.is_ascii_digit()) && words.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case("gen")));
        if !skip {
            out.push(w);
        }
    }
    if out.is_empty() { name.trim().to_string() } else { out.join(" ") }
}

/// Én rad i maskinhodet: ikon, navn, linja under og en pil som synes ved hover.
struct MachineRow {
    bx: gtk::Box,
    name: gtk::Label,
    sub: gtk::Label,
    chev: Icon,
    badge: gtk::Box,
}

fn machine_row(icon: &str, chev: &str) -> MachineRow {
    let bx = hbox(10, &[]);
    let wrap = gtk::Overlay::new();
    wrap.add_css_class("icon-wrap");
    wrap.set_valign(gtk::Align::Center);
    let ic = Icon::new(icon, 16);
    ic.set_halign(gtk::Align::Center);
    ic.set_valign(gtk::Align::Center);
    wrap.set_child(Some(&ic));
    let badge = dot("");
    badge.add_css_class("badge");
    badge.set_halign(gtk::Align::End);
    badge.set_valign(gtk::Align::End);
    badge.set_visible(false);
    wrap.add_overlay(&badge);
    bx.append(&wrap);
    let text = vbox(0, &["machine-text"]);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let name = label("–", &["machine-name"]);
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&name);
    let sub = label("", &["machine-sub", "num"]);
    sub.set_xalign(0.0);
    sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    sub.set_visible(false);
    text.append(&sub);
    bx.append(&text);
    let chev = Icon::new(chev, 14);
    chev.add_css_class("chev");
    chev.set_valign(gtk::Align::Center);
    bx.append(&chev);
    MachineRow { bx, name, sub, chev, badge }
}

/// Prosessoren fra `get_system`: (hele navnet, kort navn, linja under).
#[derive(Clone, Default)]
struct CpuInfo {
    full: String,
    short: String,
    sub: String,
}

fn cpu_info(v: &serde_json::Value) -> Option<CpuInfo> {
    let full = v["cpu"]["name"].as_str().filter(|s| !s.is_empty())?.to_string();
    let cores = v["cpu"]["cores"].as_u64();
    let threads = v["cpu"]["threads"].as_u64().or_else(|| v["threads"].as_u64());
    let sub = match (cores, threads) {
        (Some(c), Some(t)) if c != t => format!("{} · {}", F::plural(c as i64, "kjerne", "kjerner"), F::plural(t as i64, "tråd", "tråder")),
        (Some(c), _) => F::plural(c as i64, "kjerne", "kjerner"),
        (None, Some(t)) => F::plural(t as i64, "tråd", "tråder"),
        _ => String::new(),
    };
    Some(CpuInfo { short: short_cpu_name(&full), full, sub })
}

/// Navn, kilde og format for en verdi i statusblokken.
pub struct StatusDef {
    pub label: String,
    pub long: String,
    /// «gpu», «cpu», «mem», «net», «disk» eller «hw» (kildemerket i ikonlista).
    pub src: &'static str,
    pub serie: &'static str,
    pub opts: SparkOpts,
}

fn fixed100() -> SparkOpts {
    SparkOpts { span_ms: 60_000, fixed: Some((0.0, 100.0)), min_span: 0.0 }
}

fn span(min: f64) -> SparkOpts {
    SparkOpts { span_ms: 60_000, fixed: None, min_span: min }
}

/// Minste spenn i sparklinja for en festet nøkkel, så støy ikke ser ut som
/// store hopp (demoens `minSpan`): turtall 200 o/min, temperatur 5 °C,
/// effekt 20 W, prosent 10, klokker 400 MHz.
fn key_span(key: &str) -> SparkOpts {
    let tail = key.rsplit('/').next().unwrap_or(key);
    let lower = tail.to_ascii_lowercase();
    let min = if lower.starts_with("fanrpm") || key.starts_with("hwmon/") && lower.starts_with("fan") {
        200.0
    } else if lower.contains("temp") || lower == "tctl" {
        5.0
    } else if lower.starts_with("power") {
        20.0
    } else if lower.ends_with("load") || lower.ends_with("util") || lower.ends_with("pct") || lower.starts_with("fan") {
        10.0
    } else if lower.ends_with("mhz") || lower.ends_with("clock") {
        400.0
    } else {
        1.0
    };
    span(min)
}

/// Beskrivelsen av en verdi i statusblokken: de faste verdiene fra demoen,
/// og festede sensorer etter nøkkelen.
pub fn status_label(id: &str) -> StatusDef {
    let d = |label: &str, long: &str, src, serie, opts| StatusDef { label: label.into(), long: long.into(), src, serie, opts };
    match id {
        "gpuTemp" => d("GPU-temp", "GPU-temperatur", "gpu", "temp", span(20.0)),
        "gpuLoad" => d("GPU-last", "GPU-last", "gpu", "load", fixed100()),
        "gpuPower" => d("Effekt", "GPU-effekt", "gpu", "power", span(40.0)),
        "vram" => d("VRAM", "Videominne brukt", "gpu", "mem", fixed100()),
        "gpuFan" => d("Vifte", "GPU-vifte", "gpu", "clock", fixed100()),
        "cpuTemp" => d("CPU-temp", "CPU-temperatur", "cpu", "temp", span(20.0)),
        "cpuLoad" => d("CPU-last", "CPU-last", "cpu", "load", fixed100()),
        "mem" => d("Minne", "Minne brukt", "mem", "mem", fixed100()),
        "netDown" => d("Mottatt", "Nett mottatt", "net", "load", span(1.0)),
        "netUp" => d("Sendt", "Nett sendt", "net", "memctl", span(1.0)),
        other => {
            let tail = other.rsplit('/').next().unwrap_or(other);
            let src = if other.starts_with("gpu") {
                "gpu"
            } else if other.starts_with("cpu") {
                "cpu"
            } else if other.starts_with("mem") {
                "mem"
            } else if other.starts_with("net") {
                "net"
            } else if other.starts_with("disk") {
                "disk"
            } else {
                "hw"
            };
            // Festede nøkler får navnet fra Sensorer («Vifte 1 o/min»), kortet
            // litt der enheten uansett står ved tallet («Turtall 1»).
            let long = crate::pages::sensorer::short_name(other).unwrap_or_else(|| tail.to_string());
            let short = match long.strip_prefix("Vifte ").and_then(|r| r.strip_suffix(" o/min")) {
                Some(n) => format!("Turtall {n}"),
                None => long.clone(),
            };
            d(&short, &long, src, "load", key_span(other))
        }
    }
}

/// Verdien som tekst og nivået (gul/rød) for en verdi i statusblokken.
fn status_value(app: &App, id: &str) -> (Option<f64>, String, &'static str, Option<String>) {
    // Ny driver uten omstart: tallene fra kortet kan ikke stoles på (banneret sier det).
    let gpu_id = matches!(id, "gpuTemp" | "gpuLoad" | "gpuPower" | "vram" | "gpuFan") || id.starts_with("gpu:");
    if gpu_id && app.gpu_info().is_some_and(|i| i.state == DeviceState::DriverUpdated) {
        return (None, F::DASH.into(), "", None);
    }
    let t = app.settings().thresholds.clone();
    let lvl = |v: Option<f64>, w: Option<f64>, c: Option<f64>| -> &'static str {
        match v {
            Some(v) if c.is_some_and(|c| v >= c) => "danger",
            Some(v) if w.is_some_and(|w| v >= w) => "warn",
            _ => "",
        }
    };
    let gk = |f: &str| app.gpu_key(f);
    let get = |k: Option<String>| k.and_then(|k| app.value(&k));
    match id {
        "gpuTemp" => {
            let v = get(gk("temp"));
            (v, F::temp(v, F::dec(0)), lvl(v, Some(t.gpu_temp_warn), Some(t.gpu_temp_crit)), gk("temp"))
        }
        "gpuLoad" => {
            let v = get(gk("gpuUtil"));
            (v, F::pct(v, F::dec(0)), lvl(v, Some(t.load_warn), None), gk("gpuUtil"))
        }
        "gpuPower" => {
            let v = get(gk("power"));
            (v, F::watt(v, F::dec(0)), "", gk("power"))
        }
        "vram" => {
            let s = app.gpu_sample();
            let v = s.and_then(|s| s.vram_used.zip(s.vram_total).map(|(u, t)| u / t * 100.0));
            (v, F::pct(v, F::dec(0)), lvl(v, Some(85.0), Some(95.0)), None)
        }
        "gpuFan" => {
            let v = get(gk("fan"));
            (v, F::pct(v, 0), "", gk("fan"))
        }
        "cpuTemp" => {
            let v = app.value("cpu/temp");
            (v, F::temp(v, F::dec(0)), lvl(v, Some(t.cpu_temp_warn), Some(t.cpu_temp_crit)), Some("cpu/temp".into()))
        }
        "cpuLoad" => {
            let v = app.value("cpu/load");
            (v, F::pct(v, F::dec(0)), lvl(v, Some(t.load_warn), None), Some("cpu/load".into()))
        }
        "mem" => {
            let v = app.value("mem/usedPct");
            (v, F::pct(v, F::dec(0)), lvl(v, Some(85.0), Some(95.0)), Some("mem/usedPct".into()))
        }
        "netDown" => {
            let v = app.value("net/rx");
            (v, F::rate(v, None), "", Some("net/rx".into()))
        }
        "netUp" => {
            let v = app.value("net/tx");
            (v, F::rate(v, None), "", Some("net/tx".into()))
        }
        key => {
            let v = app.value(key);
            let text = if key.contains("temp") {
                F::temp(v, F::dec(0))
            } else if key.ends_with("load") || key.ends_with("Util") || key.ends_with("usedPct") {
                F::pct(v, F::dec(0))
            } else if key.ends_with("mhz") || key.ends_with("Clock") {
                F::mhz(v)
            } else if key.contains("fanRpm") {
                F::rpm(v)
            } else if key.starts_with("net/") {
                F::rate(v, None)
            } else {
                F::num(v, 1)
            };
            (v, text, "", Some(key.to_string()))
        }
    }
}

// ---------------------------------------------------------------- Statusblokkens plass

mod fit_imp {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    /// Ber om høyde 0 (naturlig: barnets), klipper barnet og sier fra når
    /// høyden den får, endres.
    #[derive(Default)]
    pub struct FitBin {
        pub on_height: crate::widgets::Slot<dyn Fn(i32)>,
        pub last: std::cell::Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FitBin {
        const NAME: &'static str = "RiggFitBin";
        type Type = super::FitBin;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for FitBin {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_overflow(gtk::Overflow::Hidden);
        }

        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for FitBin {
        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(c) if c.is_visible() => {
                    let (min, nat, _, _) = c.measure(o, for_size);
                    if o == gtk::Orientation::Vertical { (0, nat, -1, -1) } else { (min, nat, -1, -1) }
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            if self.last.replace(h) != h
                && let Some(f) = self.on_height.borrow().as_ref()
            {
                f(h);
            }
            if let Some(c) = self.obj().first_child() {
                let min = c.measure(gtk::Orientation::Vertical, w).0;
                c.size_allocate(&gtk::Allocation::new(0, 0, w, h.max(min)), -1);
            }
        }
    }
}

gtk::glib::wrapper! {
    pub struct FitBin(ObjectSubclass<fit_imp::FitBin>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl FitBin {
    fn new(child: &impl IsA<gtk::Widget>) -> FitBin {
        let f: FitBin = gtk::glib::Object::new();
        child.set_parent(&f);
        f
    }
}

struct StatRow {
    id: String,
    button: gtk::Button,
    k: gtk::Label,
    v: gtk::Label,
    vu: gtk::Label,
    spark: Spark,
    def: StatusDef,
    /// Det lange navnet nå (verktøytipset): diskene får navnet når lista kommer.
    long: RefCell<String>,
    key: RefCell<Option<String>>,
    level: RefCell<&'static str>,
}

/// En side i sidefeltet: (id, knappen, navnet, tasten).
type NavItem = (String, gtk::Button, gtk::Label, Option<gtk::Label>);

/// Et lite felt i kortdetaljene, uten en bred etikettkolonne.
fn device_detail(name: &str, value: &str) -> gtk::Box {
    let field = vbox(2, &["device-detail"]);
    field.set_hexpand(true);
    field.append(&label(name, &["device-detail-key"]));
    let val = label(value, &["device-detail-value", "num"]);
    val.set_selectable(true);
    val.set_focusable(false);
    val.set_wrap(true);
    val.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    val.set_max_width_chars(20);
    field.append(&val);
    field
}

fn close_device_popover(button: &gtk::Button) {
    if let Some(pop) = button.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>() {
        pop.popdown();
    }
}

pub struct Sidebar {
    app: Rc<App>,
    /// Hele kolonnen (toppfelt + sidefelt) med fast bredde.
    pub column: crate::widgets::fixed::Fixed,
    pub head: gtk::Box,
    pub body: gtk::Box,
    pub app_icon: Spin,
    app_name: gtk::Label,
    log_dot: gtk::Box,
    log_time: gtk::Label,
    moon: Icon,
    /// Vinduskontrollene til venstre i layouten (f.eks. «close:»).
    head_controls: gtk::WindowControls,
    head_controls_ok: Rc<std::cell::Cell<bool>>,
    machine: gtk::Box,
    machine_sep: gtk::Box,
    device: gtk::Button,
    gpu: MachineRow,
    cpu: gtk::Button,
    cpu_row: MachineRow,
    cpu_info: RefCell<Option<CpuInfo>>,
    cpu_asked: std::cell::Cell<bool>,
    nav: gtk::Box,
    nav_items: RefCell<Vec<NavItem>>,
    sec_labels: RefCell<Vec<(Section, gtk::Widget, gtk::Box)>>,
    /// Kortet Styring gjelder, under seksjonstittelen (runde 8A).
    nav_target: RefCell<Option<(gtk::Button, gtk::Label)>>,
    status: gtk::Box,
    status_bin: FitBin,
    status_head: gtk::Label,
    status_rows: gtk::Box,
    rows: RefCell<Vec<Rc<StatRow>>>,
    srcs: RefCell<Vec<gtk::Label>>,
    foot: gtk::Box,
    settings_link: gtk::Button,
    settings_label: gtk::Label,
    keys_btn: gtk::Button,
    toggle: gtk::Button,
    rail: std::cell::Cell<bool>,
    narrow: std::cell::Cell<bool>,
    height: std::cell::Cell<i32>,
    /// Kalles når sidefeltet blir eller slutter å være ikonlist (skallet
    /// flytter da vinduskontrollene til venstre).
    pub on_rail: crate::widgets::Slot<dyn Fn(bool)>,
}

impl Sidebar {
    pub fn new(app: &Rc<App>) -> Rc<Sidebar> {
        // ---- Toppfeltet: appikon, navn, loggstatus og månen ----
        let head = hbox(8, &["side-head"]);
        let head_controls_ok = Rc::new(std::cell::Cell::new(true));
        let head_controls = crate::topbar::controls(gtk::PackType::Start, head_controls_ok.clone());
        head.append(&head_controls);
        let img = gtk::Image::from_icon_name(app.profile.app_id().strip_suffix(".Devel").unwrap_or(app.profile.app_id()));
        img.set_pixel_size(20);
        let app_icon = Spin::new(&img);
        app_icon.set_valign(gtk::Align::Center);
        app_icon.set_tooltip_text(Some(rigg_core::version::NAME));
        head.append(&app_icon);
        let app_name = label(rigg_core::version::NAME, &["app-name"]);
        app_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        head.append(&app_name);
        let log_dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        log_dot.add_css_class("log-dot");
        log_dot.set_valign(gtk::Align::Center);
        log_dot.set_visible(false);
        head.append(&log_dot);
        let log_time = label("", &["log-time", "num"]);
        log_time.set_visible(false);
        head.append(&log_time);
        let moon = Icon::new("moon", 14);
        moon.add_css_class("moon");
        moon.set_visible(false);
        moon.set_can_target(true);
        moon.set_tooltip_text(Some(rigg_core::easter::NIGHT_TIP));
        head.append(&moon);
        let head_handle = gtk::WindowHandle::new();
        head_handle.set_child(Some(&head));

        // ---- Sidefeltet ----
        let body = vbox(4, &["sidebar"]);
        body.set_vexpand(true);
        // Maskinhodet: skjermkortet og prosessoren i ett rolig kort.
        let machine = vbox(0, &["machine"]);
        let device = gtk::Button::new();
        device.add_css_class("machine-row");
        device.add_css_class("gpu-link");
        device.add_css_class("first");
        let gpu = machine_row("gpu", "chevron-right");
        device.set_child(Some(&gpu.bx));
        machine.append(&device);
        let machine_sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        machine_sep.add_css_class("machine-sep");
        machine.append(&machine_sep);
        let cpu = gtk::Button::new();
        cpu.add_css_class("machine-row");
        cpu.add_css_class("last");
        let cpu_row = machine_row("cpu", "chevron-right");
        set_text(&cpu_row.name, "Prosessor");
        cpu.set_child(Some(&cpu_row.bx));
        machine.append(&cpu);
        body.append(&machine);

        let nav = vbox(1, &["nav"]);
        nav.set_accessible_role(gtk::AccessibleRole::Navigation);
        nav.update_property(&[gtk::accessible::Property::Label("Sider")]);
        body.append(&nav);

        let status = vbox(1, &["side-status"]);
        status.set_valign(gtk::Align::End);
        let status_head = label("Status", &["side-status-head"]);
        status.append(&status_head);
        let status_rows = vbox(0, &[]);
        status.append(&status_rows);
        let status_bin = FitBin::new(&status);
        status_bin.set_vexpand(true);
        body.append(&status_bin);

        let foot = hbox(2, &["side-foot"]);
        let settings_link = gtk::Button::new();
        settings_link.add_css_class("side-link");
        settings_link.set_hexpand(true);
        let sl = hbox(10, &[]);
        sl.append(&Icon::new("settings", 16));
        let settings_label = label("Innstillinger", &[]);
        sl.append(&settings_label);
        settings_link.set_child(Some(&sl));
        tip(&settings_link, "Innstillinger", Some("Ctrl+,"));
        foot.append(&settings_link);
        let keys_btn = crate::widgets::button(Some("circle-help"), None, &["flat", "icon-only", "side-btn"]);
        keys_btn.update_property(&[gtk::accessible::Property::Label("Tastatursnarveier")]);
        tip(&keys_btn, "Tastatursnarveier", Some("Ctrl+?"));
        foot.append(&keys_btn);
        let toggle = crate::widgets::button(Some("panel-left-close"), None, &["flat", "icon-only", "side-btn"]);
        foot.append(&toggle);
        body.append(&foot);

        let inner = vbox(0, &[]);
        inner.append(&head_handle);
        inner.append(&body);
        let column = crate::widgets::fixed::Fixed::new(&inner, SIDE_W);
        column.set_hexpand(false);

        let s = Rc::new(Sidebar {
            app: app.clone(),
            column,
            head,
            body,
            app_icon,
            app_name,
            log_dot,
            log_time,
            moon,
            head_controls,
            head_controls_ok,
            machine,
            machine_sep,
            device,
            gpu,
            cpu,
            cpu_row,
            cpu_info: RefCell::new(None),
            cpu_asked: std::cell::Cell::new(false),
            nav,
            nav_items: RefCell::new(vec![]),
            sec_labels: RefCell::new(vec![]),
            nav_target: RefCell::new(None),
            status,
            status_bin,
            status_head,
            status_rows,
            rows: RefCell::new(vec![]),
            srcs: RefCell::new(vec![]),
            foot,
            settings_link,
            settings_label,
            keys_btn,
            toggle,
            rail: std::cell::Cell::new(false),
            narrow: std::cell::Cell::new(false),
            height: std::cell::Cell::new(0),
            on_rail: RefCell::new(None),
        });
        {
            // Ny høyde til statusblokken: velg radene etter at layouten er ferdig.
            let me = Rc::downgrade(&s);
            *s.status_bin.imp().on_height.borrow_mut() = Some(Box::new(move |_| {
                let me = me.clone();
                gtk::glib::idle_add_local_once(move || {
                    if let Some(s) = me.upgrade() {
                        s.fit();
                    }
                });
            }));
        }
        s.build_nav();
        s.build_status();
        s.wire();
        s.apply_side();
        s.render_device();
        s
    }

    fn wire(self: &Rc<Self>) {
        let app = self.app.clone();
        // Appikonet: fem raske klikk (påskeegg 2).
        let g = gtk::GestureClick::new();
        {
            let (app, spin) = (app.clone(), self.app_icon.clone());
            g.connect_released(move |_, _, _, _| crate::easter::icon_click(&app, &spin));
        }
        self.app_icon.add_controller(g);
        {
            let app = app.clone();
            self.settings_link.connect_clicked(move |_| app.shell().toggle_settings());
        }
        {
            let app = app.clone();
            self.keys_btn.connect_clicked(move |_| crate::shortcuts::show_dialog(&app));
        }
        {
            let me = Rc::downgrade(self);
            self.toggle.connect_clicked(move |_| {
                if let Some(s) = me.upgrade() {
                    s.toggle_collapsed();
                }
            });
        }
        {
            let app = app.clone();
            self.cpu.connect_clicked(move |_| app.go("prosessor"));
        }
        {
            let app = app.clone();
            self.device.connect_clicked(move |_| app.go("grafikkort"));
        }
        let me = Rc::downgrade(self);
        app.on_settings.connect(move |keys| {
            let Some(s) = me.upgrade() else { return };
            if keys.iter().any(|k| k == "hiddenPages" || k == "showKbd") {
                s.build_nav();
                s.mark_current(&s.app.page.borrow());
            }
            if keys.iter().any(|k| k == "statusItems" || k == "statusSparks") {
                s.build_status();
            }
            if keys.iter().any(|k| k == "sideCollapsed") {
                s.apply_side();
            }
            if keys.iter().any(|k| k == "surprises") {
                s.render_night();
            }
            if keys.iter().any(|k| k == "gpuMakers") {
                s.render_device();
            }
            s.render_status();
        });
        let me = Rc::downgrade(self);
        app.on_telemetry.connect(move |_| {
            if let Some(s) = me.upgrade() {
                s.render_status();
            }
        });
        let me = Rc::downgrade(self);
        app.on_devices.connect(move |_| {
            if let Some(s) = me.upgrade() {
                s.render_device();
                s.sync_lock_badge();
            }
        });
        // Merket ved Styring: «Bare visning» eller «Endring på · …» (7A).
        let me = Rc::downgrade(self);
        app.on_service.connect(move |_| {
            if let Some(s) = me.upgrade() {
                s.sync_lock_badge();
            }
        });
        let me = Rc::downgrade(self);
        app.on_conn.connect(move |_| {
            if let Some(s) = me.upgrade() {
                s.sync_lock_badge();
                s.render_device();
                s.ask_cpu();
                class(&s.status, "stale", !s.app.client.is_connected() && s.app.telemetry().is_some());
            }
        });
        self.ask_cpu();
        // Månen kommer og går av seg selv: én sjekk i minuttet, ingen animasjon.
        self.render_night();
        let me = Rc::downgrade(self);
        gtk::glib::timeout_add_seconds_local(60, move || {
            let Some(s) = me.upgrade() else { return gtk::glib::ControlFlow::Break };
            s.render_night();
            gtk::glib::ControlFlow::Continue
        });
    }

    /// Bytter merket ved Styring når hjelperens tilstand er endret.
    fn sync_lock_badge(&self) {
        let ready = crate::widgets::locked::control_ready(&self.app);
        for (id, b, _, _) in self.nav_items.borrow().iter() {
            if let Some(hint) = page_hint(id)
                && let Some(page) = crate::pages::def(id)
            {
                let desc = control_description(&self.app, hint);
                let key = PAGES.iter().position(|p| p.id == id.as_str()).and_then(page_key);
                tip(b, &format!("{}\n{desc}", page.title), key.as_deref());
                b.update_property(&[gtk::accessible::Property::Description(&desc)]);
            }
        }
        let (text, _, _) = crate::widgets::locked::badge(&self.app);
        for (sec, w, _) in self.sec_labels.borrow().iter() {
            if *sec != Section::Control {
                continue;
            }
            let Some(bx) = w.downcast_ref::<gtk::Box>() else { continue };
            let mut c = bx.first_child();
            while let Some(ch) = c {
                c = ch.next_sibling();
                if ch.has_css_class("nav-lock") {
                    ch.set_visible(!ready);
                    let same = ch.first_child().and_then(|i| i.next_sibling()).and_then(|l| l.downcast::<gtk::Label>().ok()).is_some_and(|l| l.text() == text);
                    if same {
                        return;
                    }
                    bx.remove(&ch);
                }
            }
            bx.append(&lock_badge(&self.app));
        }
    }

    fn render_night(&self) {
        self.moon.set_visible(crate::easter::night(&self.app) && !self.rail.get());
    }

    // ---------------------------------------------------------------- Sidene

    fn build_nav(self: &Rc<Self>) {
        crate::widgets::clear(&self.nav);
        self.nav_items.borrow_mut().clear();
        self.sec_labels.borrow_mut().clear();
        let hidden = self.app.settings().hidden_pages.clone();
        let show_kbd = self.app.settings().show_kbd;
        let mut last: Option<Section> = None;
        for (i, p) in PAGES.iter().enumerate() {
            if hidden.iter().any(|h| h == p.id) {
                continue;
            }
            if last != Some(p.section) {
                let sec = hbox(8, &["nav-sec"]);
                if last.is_none() {
                    sec.add_css_class("first");
                }
                let title = label(section_title(p.section), &["nav-sec-title"]);
                title.set_hexpand(true);
                title.set_xalign(0.0);
                title.set_accessible_role(gtk::AccessibleRole::Heading);
                sec.append(&title);
                if p.section == Section::Control {
                    // Ett merke for hele seksjonen, med forklaringen i
                    // verktøytipset. Teksten følger root-hjelperen (7A).
                    sec.append(&lock_badge(&self.app));
                }
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                line.add_css_class("nav-sec-line");
                line.set_visible(false);
                self.nav.append(&sec);
                self.nav.append(&line);
                if p.section == Section::Control {
                    // Kortet som styres, funnet av seg selv: «RTX 2080 · Palit».
                    // Detaljene åpnes her, ved kortet som faktisk ble trykket.
                    let b = gtk::Button::new();
                    b.add_css_class("nav-target");
                    b.add_css_class("flat");
                    b.update_property(&[gtk::accessible::Property::HasPopup(true), gtk::accessible::Property::Description("Detaljer og kortvalg")]);
                    b.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
                    let bx = hbox(8, &[]);
                    bx.append(&Icon::new("gpu", 14));
                    let l = label("", &["nav-target-name"]);
                    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    l.set_xalign(0.0);
                    l.set_hexpand(true);
                    bx.append(&l);
                    let chevron = Icon::new("chevron-down", 12);
                    chevron.add_css_class("target-chevron");
                    bx.append(&chevron);
                    b.set_child(Some(&bx));
                    let me = Rc::downgrade(self);
                    b.connect_clicked(move |b| {
                        if let Some(s) = me.upgrade() {
                            s.show_device_popover(b.upcast_ref());
                        }
                    });
                    self.nav.append(&b);
                    *self.nav_target.borrow_mut() = Some((b, l));
                }
                self.sec_labels.borrow_mut().push((p.section, sec.upcast(), line));
                last = Some(p.section);
            }
            let b = gtk::Button::new();
            b.add_css_class("nav-item");
            let bx = hbox(10, &[]);
            bx.append(&Icon::new(p.icon, 16));
            let l = label(p.title, &[]);
            l.set_hexpand(true);
            bx.append(&l);
            let num = page_key(i);
            let key = match (&num, show_kbd) {
                (Some(n), true) => {
                    let k = label(n, &["key", "num"]);
                    bx.append(&k);
                    Some(k)
                }
                _ => None,
            };
            b.set_child(Some(&bx));
            let description = page_hint(p.id).map(|h| control_description(&self.app, h));
            let tiptext = description.as_ref().map(|d| format!("{}\n{d}", p.title)).unwrap_or_else(|| p.title.to_string());
            tip(&b, &tiptext, num.as_deref());
            b.update_property(&[gtk::accessible::Property::Label(p.title)]);
            if let Some(desc) = description {
                b.update_property(&[gtk::accessible::Property::Description(&desc)]);
            }
            let app = self.app.clone();
            let id = p.id;
            b.connect_clicked(move |_| app.go(id));
            self.nav.append(&b);
            self.nav_items.borrow_mut().push((p.id.to_string(), b, l, key));
        }
        self.apply_rail_visibility();
        // Piltastene flytter mellom sidene.
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(s) = me.upgrade() else { return gtk::glib::Propagation::Proceed };
            let items: Vec<gtk::Button> = s.nav_items.borrow().iter().map(|i| i.1.clone()).collect();
            let i = items.iter().position(|b| b.has_focus());
            let n = items.len();
            let next = match (key, i) {
                (gtk::gdk::Key::Down, Some(i)) => (i + 1).min(n - 1),
                (gtk::gdk::Key::Up, Some(i)) => i.saturating_sub(1),
                (gtk::gdk::Key::Home, Some(_)) => 0,
                (gtk::gdk::Key::End, Some(_)) => n - 1,
                _ => return gtk::glib::Propagation::Proceed,
            };
            items[next].grab_focus();
            gtk::glib::Propagation::Stop
        });
        self.nav.add_controller(keys);
    }

    /// Markerer siden som vises.
    pub fn mark_current(&self, page: &str) {
        for (id, b, _, _) in self.nav_items.borrow().iter() {
            class(b, "current", id == page);
            if id == page {
                b.update_state(&[gtk::accessible::State::Selected(Some(true))]);
            } else {
                b.reset_state(gtk::AccessibleState::Selected);
            }
        }
        class(&self.settings_link, "current", page == "innstillinger");
    }

    // ---------------------------------------------------------------- Kortvelgeren

    /// Skjermkortet i maskinhodet: kort navn, og under det minnet og driveren,
    /// eller feilen når noe er galt (gul eller rød tekst, aldri en prikk som
    /// alltid står der).
    fn render_device(&self) {
        let info = self.app.gpu_info();
        let st = self.app.statics();
        let connected = self.app.client.is_connected();
        let name = st
            .as_ref()
            .filter(|s| !s.name.is_empty())
            .map(|s| rigg_core::model::short_gpu_name(&s.name))
            .or_else(|| info.as_ref().filter(|i| !i.name.is_empty()).map(|i| rigg_core::model::short_gpu_name(&i.name)));
        let gpus = self.app.data.borrow().gpus.iter().filter(|g| !g.id.is_empty()).count();
        let normal = || {
            let mut parts: Vec<String> = Vec::new();
            if gpus > 1 {
                let i = info.as_ref().map(|i| i.index as usize + 1).unwrap_or(1);
                parts.push(format!("Kort {i} av {gpus}"));
            }
            let vram = st.as_ref().and_then(|s| s.vram_total).or_else(|| self.app.gpu_sample().and_then(|s| s.vram_total));
            if let Some(v) = vram.filter(|v| *v > 0.0) {
                parts.push(format!("{}{}GB", (v / 1024.0).round(), F::NNBSP));
            }
            // Med flere kort står kortnummeret der driveren står (den er lik for alle).
            if gpus <= 1
                && let Some(d) = st.as_ref().and_then(|s| s.driver.clone()).or_else(|| info.as_ref().and_then(|i| i.driver.clone()))
            {
                parts.push(format!("driver {d}"));
            }
            parts.join(" · ")
        };
        let (kind, sub) = match (&info, connected) {
            (_, false) if self.app.telemetry().is_some() => ("danger", "Tjenesten svarer ikke".to_string()),
            (_, false) => ("", "Kobler til …".to_string()),
            (None, true) => ("", "Leser …".to_string()),
            (Some(i), true) => match i.state {
                DeviceState::Ok => ("", normal()),
                DeviceState::DriverUpdated => ("warn", "Ny driver · start på nytt".into()),
                DeviceState::NoDriver => ("danger", "Ingen NVIDIA-driver".into()),
                DeviceState::Lost => ("danger", "Svarer ikke".into()),
                DeviceState::Suspended => ("", "Dvale".into()),
            },
        };
        let name = name.unwrap_or_else(|| if info.as_ref().is_some_and(|i| i.state == DeviceState::NoDriver) { "Ingen skjermkort".into() } else { "Skjermkort".into() });
        // Produsenten foran navnet når den er kjent (satt av brukeren, eller
        // en kjent kortprodusent i PCI-subsystemet): «Palit RTX 2080» (8A).
        let maker = st.as_ref().and_then(|s| crate::widgets::styrer::maker_short(&self.app, s));
        let name = match &maker {
            Some(m) if st.is_some() => format!("{m} {name}"),
            _ => name,
        };
        set_text(&self.gpu.name, &name);
        set_text(&self.gpu.sub, &sub);
        for k in ["warn", "danger"] {
            class(&self.gpu.sub, k, k == kind);
            class(&self.device, k, k == kind);
        }
        set_dot(&self.gpu.badge, kind);
        self.apply_machine_visibility();
        let long = st.as_ref().map(|s| s.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| name.clone());
        let t = if sub.is_empty() { long.clone() } else { format!("{long} · {sub}") };
        self.device.set_tooltip_text(Some(&format!("{t}\nVis Grafikkort")));
        self.device.update_property(&[gtk::accessible::Property::Label(&t), gtk::accessible::Property::Description("Vis Grafikkort")]);
        // Kortet Styring gjelder, under seksjonstittelen.
        if let Some((b, l)) = self.nav_target.borrow().as_ref() {
            // Feil: «RTX 2080 · svarer ikke», eller bare feilen når det ikke er
            // noe kort å nevne («Ingen NVIDIA-driver», «Tjenesten svarer ikke»).
            let no_card = !connected || info.as_ref().is_some_and(|i| i.state == DeviceState::NoDriver);
            let text = match kind {
                "danger" | "warn" if no_card => sub.clone(),
                "danger" | "warn" => {
                    // Bare første del: «Ny driver · start på nytt» blir «ny driver».
                    let first = sub.split(" · ").next().unwrap_or(&sub).to_string();
                    let mut c = first.chars();
                    let low = c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default();
                    format!("{name} · {low}")
                }
                _ => name.clone(),
            };
            set_text(l, &text);
            for k in ["warn", "danger"] {
                class(b, k, k == kind);
            }
            let tip = format!("Styring gjelder dette kortet: {long}{}\nDetaljer og kortvalg", maker.as_ref().map(|m| format!(" ({m})")).unwrap_or_default());
            if b.tooltip_text().as_deref() != Some(tip.as_str()) {
                b.set_tooltip_text(Some(&tip));
            }
            b.update_property(&[gtk::accessible::Property::Label(&format!("Styring gjelder {text}"))]);
        }
    }

    /// Prosessornavnet hentes én gang per tilkobling (`get_system`, som
    /// tjenesten har i minnet); det står igjen om tjenesten blir borte.
    fn ask_cpu(self: &Rc<Self>) {
        if !self.app.client.is_connected() {
            self.cpu_asked.set(false);
            return;
        }
        if self.cpu_asked.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        self.app.client.request("get_system", json!({}), move |res| {
            let Some(s) = me.upgrade() else { return };
            match res.ok().as_ref().and_then(cpu_info) {
                Some(c) => {
                    *s.cpu_info.borrow_mut() = Some(c);
                    s.render_cpu();
                }
                None => s.cpu_asked.set(false),
            }
        });
    }

    fn render_cpu(&self) {
        let c = self.cpu_info.borrow().clone().unwrap_or_default();
        set_text(&self.cpu_row.name, if c.short.is_empty() { "Prosessor" } else { &c.short });
        set_text(&self.cpu_row.sub, &c.sub);
        self.apply_machine_visibility();
        let t = if c.full.is_empty() { "Prosessor".to_string() } else if c.sub.is_empty() { c.full.clone() } else { format!("{} · {}", c.full, c.sub) };
        tip(&self.cpu, &format!("{t}\nVis Prosessor"), page_key(PAGES.iter().position(|p| p.id == "prosessor").unwrap_or(99)).as_deref());
        self.cpu.update_property(&[gtk::accessible::Property::Label(&t), gtk::accessible::Property::Description("Vis Prosessor")]);
    }

    /// Navn og linjer i maskinhodet etter bredde (ikonlist) og høyde.
    fn apply_machine_visibility(&self) {
        let rail = self.rail.get();
        let compact = self.height.get() > 0 && self.height.get() < COMPACT_H;
        for (r, err) in [(&self.gpu, self.device.has_css_class("warn") || self.device.has_css_class("danger")), (&self.cpu_row, false)] {
            r.bx.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
            if let Some(text) = r.name.parent() {
                text.set_visible(!rail);
            }
            r.chev.set_visible(!rail);
            // Feil vises alltid, også i et lavt vindu; vanlige linjer bare når det er plass.
            r.sub.set_visible(!rail && !r.sub.text().is_empty() && (!compact || err));
            r.badge.set_visible(rail && err);
        }
        self.machine_sep.set_visible(!rail);
        class(&self.machine, "rail", rail);
        let align = if rail { gtk::Align::Center } else { gtk::Align::Fill };
        self.device.set_halign(align);
        self.cpu.set_halign(align);
    }

    fn show_device_popover(self: &Rc<Self>, anchor: &gtk::Widget) {
        let app = &self.app;
        let body = vbox(0, &["device-details"]);
        let st = app.statics().unwrap_or_default();
        let info = app.gpu_info();
        let full_name = if !st.name.is_empty() { st.name.clone() } else { info.as_ref().map(|i| i.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Skjermkort".into()) };
        let head = vbox(3, &["device-details-head"]);
        head.append(&label("Valgt skjermkort", &["device-details-caption"]));
        let title = label(&rigg_core::model::short_gpu_name(&full_name), &["device-details-title"]);
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title.set_max_width_chars(20);
        title.set_tooltip_text(Some(&full_name));
        head.append(&title);
        // Feilkoden fra NVML står bare i verktøytipset (spesifikasjonen kap. 5).
        let since = |i: &rigg_core::model::GpuInfo| F::since(crate::app::local_time(i.since), crate::app::local_now());
        let sub = match &info {
            Some(i) if i.state == DeviceState::Ok => "Tilkoblet".to_string(),
            Some(i) if i.state == DeviceState::Lost => format!("Svarer ikke siden {}", since(i)),
            Some(i) if i.state == DeviceState::NoDriver => "Fant ingen NVIDIA-driver".into(),
            Some(i) if i.state == DeviceState::DriverUpdated => "Driveren er oppdatert. Start maskinen på nytt.".into(),
            Some(i) if i.state == DeviceState::Suspended => format!("I dvale siden {}", since(i)),
            Some(_) => "–".into(),
            None => "Ikke tilkoblet".into(),
        };
        let subl = label(&sub, &["device-details-state"]);
        subl.set_wrap(true);
        subl.set_max_width_chars(22);
        if let Some(i) = &info {
            match i.state {
                DeviceState::Lost => subl.add_css_class("danger"),
                DeviceState::NoDriver | DeviceState::DriverUpdated => subl.add_css_class("warn"),
                _ => {}
            }
        }
        if let Some(d) = info.as_ref().filter(|i| i.state != DeviceState::Ok).and_then(|i| i.detail.clone()) {
            subl.set_tooltip_text(Some(&d));
        }
        head.append(&subl);
        body.append(&head);
        // Korte felt side om side; lange verdier får hele bredden og brytes.
        let facts = vbox(8, &["device-details-facts"]);
        let versions = hbox(12, &[]);
        versions.set_homogeneous(true);
        versions.append(&device_detail("Driver", st.driver.as_deref().unwrap_or("–")));
        versions.append(&device_detail("CUDA", st.cuda_version.as_deref().unwrap_or("–")));
        facts.append(&versions);
        facts.append(&device_detail("VBIOS", st.vbios.as_deref().unwrap_or("–")));
        let s = app.gpu_sample().unwrap_or_default();
        // Samme form som Oversikt: «x16 Gen 1 · maks Gen 3».
        let pcie = if s.pcie_gen.is_none() && s.pcie_width.is_none() {
            F::DASH.to_string()
        } else {
            crate::widgets::chart::math::pcie_text(st.pcie_width_max, st.pcie_gen_max, s.pcie_width, s.pcie_gen).unwrap_or_else(|| F::DASH.into())
        };
        facts.append(&device_detail("PCIe", &pcie));
        body.append(&facts);
        let service = vbox(8, &["device-details-service"]);
        service.append(&device_detail("Oppdatering", &F::seconds(app.settings().interval_ms)));
        let svc = match (app.client.is_connected(), app.data.borrow().hello.clone()) {
            (true, Some(h)) => format!("{} · bygg {}", h["daemon"]["version"].as_str().unwrap_or("–"), h["daemon"]["build"]),
            _ => "Ikke tilkoblet".into(),
        };
        service.append(&device_detail("Tjeneste", &svc));
        body.append(&service);
        // Flere kort: velg her.
        let gpus = app.data.borrow().gpus.clone();
        if gpus.iter().filter(|g| !g.id.is_empty()).count() > 1 {
            body.append(&label("Kort", &["menu-label"]));
            for g in gpus.into_iter().filter(|g| !g.id.is_empty()) {
                let selected = app.gpu_id().as_deref() == Some(g.id.as_str());
                let b = crate::widgets::button(Some(if selected { "check" } else { "gpu" }), Some(&rigg_core::model::short_gpu_name(&g.name)), &["flat", "menu-item", "device-details-choice"]);
                b.update_property(&[gtk::accessible::Property::Description(if selected { "Valgt skjermkort" } else { "Velg dette skjermkortet" })]);
                // Lange navn brytes også med flere kort og stor skrift.
                if let Some(row) = b.child() {
                    let mut child = row.first_child();
                    while let Some(w) = child {
                        if let Some(l) = w.downcast_ref::<gtk::Label>() {
                            l.set_wrap(true);
                            l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                            l.set_max_width_chars(18);
                        }
                        child = w.next_sibling();
                    }
                }
                let a = app.clone();
                let id = g.id.clone();
                b.connect_clicked(move |b| {
                    close_device_popover(b);
                    a.select_gpu(&id);
                });
                body.append(&b);
            }
        }
        let more = crate::widgets::button(Some("arrow-right"), Some("Mer om kortet"), &["flat", "menu-item", "device-details-more"]);
        {
            let a = app.clone();
            more.connect_clicked(move |b| {
                close_device_popover(b);
                a.go("grafikkort");
            });
        }
        body.append(&more);
        let width = if self.rail.get() { SIDE_W - 17 } else { anchor.compute_bounds(anchor).map(|b| b.width().round() as i32).unwrap_or(SIDE_W - 17) };
        let constrained = crate::widgets::fixed::Fixed::new(&body, (width - 8).max(160));
        let pop = crate::widgets::popover::popover(anchor, &constrained);
        pop.add_css_class("device-popover");
        pop.set_halign(gtk::Align::Start);
        // Samme bredde som raden i sidefeltet; ikonlista får normal kortbredde.
        anchor.add_css_class("open");
        anchor.update_state(&[gtk::accessible::State::Expanded(Some(true))]);
        let anchor = anchor.clone();
        pop.connect_closed(move |_| {
            anchor.remove_css_class("open");
            anchor.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        });
        pop.update_property(&[gtk::accessible::Property::Label(&format!("Detaljer om {full_name}"))]);
        pop.popup();
        // Fokus på knappen, ikke på første valgbare verdi (den ble markert).
        more.grab_focus();
    }

    // ---------------------------------------------------------------- Statusblokken

    fn build_status(self: &Rc<Self>) {
        crate::widgets::clear(&self.status_rows);
        self.rows.borrow_mut().clear();
        self.srcs.borrow_mut().clear();
        let items = self.app.settings().status_items.clone();
        let sparks = self.app.settings().status_sparks;
        if items.is_empty() {
            let b = gtk::Button::new();
            b.add_css_class("empty-note");
            let bx = hbox(8, &[]);
            bx.append(&Icon::new("pin", 14));
            bx.append(&label("Fest sensorer fra Sensorer", &[]));
            b.set_child(Some(&bx));
            tip(&b, "Pek på en rad på Sensorer og trykk på nålen", None);
            let app = self.app.clone();
            b.connect_clicked(move |_| app.go("sensorer"));
            self.status_rows.append(&b);
        }
        let mut src: &str = "";
        for id in items {
            let def = status_label(&id);
            if def.src != src {
                let tag = match def.src {
                    "gpu" => "GPU",
                    "cpu" => "CPU",
                    "mem" => "RAM",
                    "net" => "NETT",
                    "disk" => "DISK",
                    _ => "ANDRE",
                };
                let l = label(tag, &["stat-src"]);
                l.set_xalign(0.5);
                if self.srcs.borrow().is_empty() {
                    l.add_css_class("first");
                }
                l.set_accessible_role(gtk::AccessibleRole::Presentation);
                self.status_rows.append(&l);
                self.srcs.borrow_mut().push(l);
                src = def.src;
            }
            let b = gtk::Button::new();
            b.add_css_class("stat-row");
            let bx = hbox(8, &[]);
            let k = label(&def.label, &["k"]);
            // Navnet får plassen tallet og sparklinja ikke bruker; det kuttes
            // bare hvis det ikke finnes noen annen utvei.
            k.set_width_chars(8);
            k.set_xalign(0.0);
            k.set_ellipsize(gtk::pango::EllipsizeMode::End);
            bx.append(&k);
            let vb = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            vb.set_hexpand(true);
            vb.set_halign(gtk::Align::End);
            let v = label("–", &["v", "num"]);
            let vu = label("", &["vu"]);
            vb.append(&v);
            vb.append(&vu);
            bx.append(&vb);
            let spark = Spark::new(def.serie, 40);
            spark.set_visible(sparks);
            bx.append(&spark);
            b.set_child(Some(&bx));
            let long = RefCell::new(def.long.clone());
            let row = Rc::new(StatRow { id: id.clone(), button: b.clone(), k, v, vu, spark, def, long, key: RefCell::new(None), level: RefCell::new("") });
            let tiptext = format!("{} · vis på Sensorer", row.def.long);
            b.set_tooltip_text(Some(&tiptext));
            b.update_property(&[gtk::accessible::Property::Label(&row.def.long)]);
            let app = self.app.clone();
            let vis = format!("sensorer?vis={id}");
            b.connect_clicked(move |_| app.go(&vis));
            // Høyreklikk: flytt, løsne eller vis på Sensorer.
            let rc = gtk::GestureClick::new();
            rc.set_button(3);
            let me = Rc::downgrade(self);
            let rid = id.clone();
            rc.connect_pressed(move |g, _, _, _| {
                g.set_state(gtk::EventSequenceState::Claimed);
                if let Some(s) = me.upgrade() {
                    s.status_menu(&rid);
                }
            });
            b.add_controller(rc);
            self.status_rows.append(&b);
            self.rows.borrow_mut().push(row);
        }
        self.apply_rail_visibility();
        self.render_status();
        self.fit();
    }

    fn status_menu(self: &Rc<Self>, id: &str) {
        let Some(row) = self.rows.borrow().iter().find(|r| r.id == id).cloned() else { return };
        let items = self.app.settings().status_items.clone();
        let i = items.iter().position(|x| x == id).unwrap_or(0);
        let m = Menu::new(&row.button);
        m.popover.update_property(&[gtk::accessible::Property::Label(&format!("Valg for {}", row.long.borrow()))]);
        let mv = |dir: i32| {
            let app = self.app.clone();
            let id = id.to_string();
            move || {
                let mut cur = app.settings().status_items.clone();
                if let Some(i) = cur.iter().position(|x| *x == id) {
                    let j = i as i32 + dir;
                    if j >= 0 && (j as usize) < cur.len() {
                        cur.swap(i, j as usize);
                        app.set(json!({"statusItems": cur}));
                    }
                }
            }
        };
        let up = m.item(Some("arrow-up"), "Flytt opp", None, mv(-1));
        up.set_sensitive(i > 0);
        let down = m.item(Some("arrow-down"), "Flytt ned", None, mv(1));
        down.set_sensitive(i + 1 < items.len());
        {
            let app = self.app.clone();
            let id = id.to_string();
            m.item(Some("pin-off"), "Løsne", None, move || {
                let cur: Vec<String> = app.settings().status_items.iter().filter(|x| **x != id).cloned().collect();
                app.set(json!({"statusItems": cur}));
            });
        }
        m.sep();
        {
            let app = self.app.clone();
            let vis = format!("sensorer?vis={id}");
            m.item(Some("arrow-right"), "Vis på Sensorer", None, move || app.go(&vis));
        }
        {
            let app = self.app.clone();
            m.item(Some("settings"), "Rekkefølge og utvalg …", None, move || app.open_settings("sidefelt"));
        }
        m.show();
    }

    fn render_status(&self) {
        let tele = self.app.telemetry();
        let now = tele.as_ref().map(|t| t.t).unwrap_or(0);
        let sparks = self.app.settings().status_sparks && !self.rail.get();
        let disks = tele.as_ref().map(|t| t.sys.disks.as_slice()).unwrap_or(&[]);
        for r in self.rows.borrow().iter() {
            // Diskene: navnet fra disklista («sda temp»), også når den kommer
            // etter at blokken er bygget.
            let mut renamed = false;
            if r.id.starts_with("disk/")
                && let Some((short, long)) = crate::pages::sensorer::disk_name(&r.id, disks)
                && *r.long.borrow() != long
            {
                set_text(&r.k, &short);
                r.button.update_property(&[gtk::accessible::Property::Label(&long)]);
                *r.long.borrow_mut() = long;
                renamed = true;
            }
            let (_, text, lvl, key) = status_value(&self.app, &r.id);
            // Tall og enhet hver for seg: i ikonlista står enheten under tallet.
            let (num, unit) = match text.rfind(F::NNBSP) {
                Some(i) => (text[..i].to_string(), text[i + F::NNBSP.len_utf8()..].to_string()),
                None => (text.clone(), String::new()),
            };
            set_text(&r.v, &num);
            set_text(&r.vu, &unit);
            let prev = *r.level.borrow();
            if prev != lvl || renamed {
                *r.level.borrow_mut() = lvl;
                class(&r.button, "warn", lvl == "warn");
                class(&r.button, "danger", lvl == "danger");
                r.spark.set_level(lvl);
                let word = match lvl {
                    "warn" => " · over gul varselgrense",
                    "danger" => " · over rød varselgrense",
                    _ => "",
                };
                r.button.set_tooltip_text(Some(&format!("{} · vis på Sensorer{word}", r.long.borrow())));
            }
            if sparks && let Some(k) = &key {
                let pts = if r.id == "vram" { vec![] } else { self.app.recent(k) };
                r.spark.set_points(&pts, now, r.def.opts);
            }
            *r.key.borrow_mut() = key;
        }
        // VRAM i prosent regnes fra to nøkler.
        if sparks && let Some(r) = self.rows.borrow().iter().find(|r| r.id == "vram") {
            let (u, t) = (self.app.gpu_key("vramUsed"), self.app.gpu_sample().and_then(|s| s.vram_total));
            if let (Some(u), Some(t)) = (u, t) {
                let pts: Vec<(i64, f64)> = self.app.recent(&u).into_iter().map(|(ts, v)| (ts, v / t * 100.0)).collect();
                r.spark.set_points(&pts, now, r.def.opts);
            }
        }
    }

    // ---------------------------------------------------------------- Bredden

    fn toggle_collapsed(&self) {
        let next = !self.app.settings().side_collapsed;
        self.app.set(json!({"sideCollapsed": next}));
        self.column.announce(if next { "Sidefeltet er lagt sammen." } else { "Sidefeltet er utvidet." }, gtk::AccessibleAnnouncementPriority::Low);
    }

    /// Ctrl+B. Gjør ingenting i smale vinduer (der er det alltid en ikonlist).
    pub fn ctrl_b(&self) {
        if !self.narrow.get() {
            self.toggle_collapsed();
        }
    }

    /// Vinduet er smalere enn 860 px (alltid ikonlist).
    pub fn set_narrow(&self, narrow: bool) {
        if self.narrow.replace(narrow) != narrow {
            self.apply_side();
        }
    }

    pub fn set_height(&self, h: i32) {
        let old = self.height.replace(h);
        if old != h {
            if (old > 0 && old < COMPACT_H) != (h > 0 && h < COMPACT_H) {
                self.apply_machine_visibility();
            }
            self.fit();
        }
    }

    pub fn is_rail(&self) -> bool {
        self.rail.get()
    }

    fn apply_side(&self) {
        let collapsed = self.app.settings().side_collapsed;
        let rail = collapsed || self.narrow.get();
        let changed = self.rail.replace(rail) != rail;
        self.column.set_width(if rail { RAIL_W } else { SIDE_W });
        class(&self.body, "rail", rail);
        class(&self.head, "rail", rail);
        let label_text = if collapsed { "Utvid sidefeltet" } else { "Legg sammen sidefeltet" };
        tip(&self.toggle, label_text, Some("Ctrl+B"));
        self.toggle.update_property(&[gtk::accessible::Property::Label(label_text)]);
        self.toggle.update_state(&[gtk::accessible::State::Expanded(Some(!collapsed))]);
        self.toggle.set_child(Some(&Icon::new(if collapsed { "panel-left-open" } else { "panel-left-close" }, 16)));
        self.toggle.set_visible(!self.narrow.get());
        self.apply_rail_visibility();
        if let Some(f) = self.on_rail.borrow().as_ref() {
            f(rail);
        }
        if changed {
            // Bredden byttes straks; innholdet toner inn, så byttet ikke rykker.
            crate::motion::fade_in(&self.app, &self.column, 3, 0.25);
            self.render_status();
            self.fit();
        }
    }

    fn apply_rail_visibility(&self) {
        let rail = self.rail.get();
        self.app_name.set_visible(!rail);
        self.head.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        self.head_controls_ok.set(!rail);
        self.head_controls.set_visible(!rail && !self.head_controls.is_empty());
        self.apply_machine_visibility();
        for (_, b, l, k) in self.nav_items.borrow().iter() {
            if let Some(child) = b.child() {
                child.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
            }
            l.set_visible(!rail);
            if let Some(k) = k {
                k.set_visible(!rail);
            }
            b.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        }
        for (_, sec, line) in self.sec_labels.borrow().iter() {
            sec.set_visible(!rail);
            line.set_visible(rail);
        }
        if let Some((b, _)) = self.nav_target.borrow().as_ref() {
            b.set_visible(!rail);
        }
        if let Some((_, _, first_line)) = self.sec_labels.borrow().first() {
            first_line.set_visible(false);
        }
        self.status_head.set_visible(!rail);
        let sparks = self.app.settings().status_sparks;
        for r in self.rows.borrow().iter() {
            r.k.set_visible(!rail);
            r.spark.set_visible(sparks && !rail);
            r.button.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
            if let Some(bx) = r.button.child().and_then(|c| c.downcast::<gtk::Box>().ok())
                && let Some(vb) = bx.first_child().and_then(|k| k.next_sibling()).and_then(|v| v.downcast::<gtk::Box>().ok())
            {
                vb.set_orientation(if rail { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
                vb.set_halign(if rail { gtk::Align::Center } else { gtk::Align::End });
                r.v.set_xalign(if rail { 0.5 } else { 1.0 });
                r.vu.set_xalign(if rail { 0.5 } else { 0.0 });
            }
            r.vu.set_margin_start(if rail { 0 } else { 2 });
        }
        for l in self.srcs.borrow().iter() {
            l.set_visible(rail);
        }
        self.settings_label.set_visible(!rail);
        self.foot.set_orientation(if rail { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
        self.settings_link.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        self.settings_link.set_hexpand(!rail);
        for b in [&self.keys_btn, &self.toggle] {
            b.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        }
        if let Some(child) = self.settings_link.child() {
            child.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        }
        self.app_icon.set_hexpand(rail);
        self.app_icon.set_halign(if rail { gtk::Align::Center } else { gtk::Align::Fill });
        self.moon.set_visible(!rail && crate::easter::night(&self.app));
        let show_log = self.log_dot.is_visible() || !self.log_time.text().is_empty();
        self.log_time.set_visible(!rail && show_log && !self.log_time.text().is_empty());
    }

    /// Statusblokken ruller aldri: den viser så mange verdier som får plass
    /// under sidene (utvidet og ikonlist), og ingenting når ikke engang én
    /// verdi får plass (verdiene står på Oversikt og Sensorer). Blokken ber
    /// ikke om høyde selv ([`FitBin`]), så vinduet kan bli lavt; høyden den får,
    /// bestemmer hvor mange rader som vises.
    fn fit(&self) {
        let h = self.status_bin.imp().last.get();
        let kids: Vec<gtk::Widget> = {
            let mut v = Vec::new();
            let mut c = self.status_rows.first_child();
            while let Some(w) = c {
                c = w.next_sibling();
                v.push(w);
            }
            v
        };
        for k in &kids {
            k.set_visible(true);
        }
        self.apply_rail_visibility();
        self.status.set_visible(true);
        if h <= 0 {
            return;
        }
        // Rett etter et bytte mellom ikonlist og utvidet kan bredden være den gamle.
        let width = self.status_bin.width().max(self.status.measure(gtk::Orientation::Horizontal, -1).0).max(1);
        let nat = |w: &gtk::Widget| if w.is_visible() { w.measure(gtk::Orientation::Vertical, width).1 } else { 0 };
        // Luft, kant og overskrift: blokken uten radene.
        let rows_h: i32 = kids.iter().map(nat).sum();
        let overhead = self.status.measure(gtk::Orientation::Vertical, width).1 - rows_h;
        let mut left = h - overhead;
        let mut keep = Vec::new();
        for k in kids.iter().filter(|k| k.is_visible()) {
            let kh = nat(k);
            if left - kh < 0 {
                break;
            }
            left -= kh;
            keep.push(k.clone());
        }
        // Et kildemerke uten verdier under seg tas bort.
        while keep.last().is_some_and(|w| w.has_css_class("stat-src")) {
            keep.pop();
        }
        for k in &kids {
            if !keep.contains(k) {
                k.set_visible(false);
            }
        }
        // Plass til ingenting (bortsett fra «Fest sensorer …» som alltid står alene).
        let any = keep.iter().any(|w| !w.has_css_class("stat-src"));
        self.status.set_visible(any);
    }

    /// Fokus tilbake til tannhjulet etter Innstillinger (som demoen), når
    /// fokuset sto i Innstillinger.
    pub fn settings_focus(&self) {
        self.settings_link.grab_focus();
    }

    // ---------------------------------------------------------------- Loggstatus (2B)

    /// Loggen går: en liten prikk og tiden ved appnavnet øverst i sidefeltet,
    /// med filnavn, rader og størrelse i verktøytipset. `None` skjuler den.
    pub fn set_log_indicator(&self, state: Option<(&str, &str)>) {
        match state {
            Some((time, tooltip)) => {
                self.log_dot.set_visible(true);
                self.log_time.set_text(time);
                self.log_time.set_visible(!self.rail.get());
                self.log_dot.set_tooltip_text(Some(tooltip));
                self.log_time.set_tooltip_text(Some(tooltip));
            }
            None => {
                self.log_dot.set_visible(false);
                self.log_time.set_text("");
                self.log_time.set_visible(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::status_label;

    #[test]
    fn prosessornavnet_blir_kort() {
        use super::short_cpu_name as s;
        assert_eq!(s("AMD Ryzen 7 2700X Eight-Core Processor"), "Ryzen 7 2700X");
        assert_eq!(s("AMD Ryzen 9 7950X 16-Core Processor"), "Ryzen 9 7950X");
        assert_eq!(s("AMD Ryzen 7 5800H with Radeon Graphics"), "Ryzen 7 5800H");
        assert_eq!(s("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz"), "Core i7-8700K");
        assert_eq!(s("13th Gen Intel(R) Core(TM) i5-13600K"), "Core i5-13600K");
        assert_eq!(s("Intel(R) Core(TM) Ultra 7 155H"), "Core Ultra 7 155H");
        assert_eq!(s("AMD EPYC 7763 64-Core Processor"), "EPYC 7763");
        assert_eq!(s("Ukjent prosessor"), "Ukjent prosessor");
    }

    #[test]
    fn prosessorlinja_har_kjerner_og_tråder() {
        let v = serde_json::json!({"cpu": {"name": "AMD Ryzen 7 2700X Eight-Core Processor", "cores": 8, "threads": 16}});
        let c = super::cpu_info(&v).unwrap();
        assert_eq!(c.short, "Ryzen 7 2700X");
        assert_eq!(c.sub, "8 kjerner · 16 tråder");
        assert!(super::cpu_info(&serde_json::json!({})).is_none());
    }

    #[test]
    fn styring_sidene_sier_hva_de_styrer() {
        for p in crate::pages::PAGES.iter().filter(|p| p.section == crate::pages::Section::Control) {
            assert!(super::page_hint(p.id).is_some(), "{} mangler undertekst", p.id);
        }
        let titles: Vec<&str> = crate::pages::PAGES.iter().filter(|p| p.section == crate::pages::Section::Control).map(|p| p.title).collect();
        assert_eq!(titles, ["Klokker og effekt", "Vifter", "RGB-lys"]);
    }

    /// Navnene i statusblokken får plass i sidefeltet (236 px): navnet har
    /// 236 − 2 × 8 (luft) − 2 × 8 (avstand) − 40 (sparklinje) − ca. 64 (tallet
    /// «2 200 MHz») ≈ 100 px, og 12 tegn i 12 px skrift er ca. 80 px. Hvert
    /// navn står bare én gang, uten enheten (den står ved tallet). Bildene i
    /// planen 3c sjekker det samme med «Stor» skrift (kuttet.txt).
    #[test]
    fn statusnavnene_er_korte_og_uten_enhet() {
        let keys = [
            "gpuTemp", "gpuLoad", "gpuPower", "vram", "gpuFan", "cpuTemp", "cpuLoad", "mem", "netDown", "netUp",
            "gpu:0000:01:00.0/coreClock", "gpu:0000:01:00.0/memClock", "gpu:0000:01:00.0/smClock", "gpu:0000:01:00.0/videoClock",
            "gpu:0000:01:00.0/memUtil", "gpu:0000:01:00.0/fanRpm0", "gpu:0000:01:00.0/fanRpm1", "gpu:0000:01:00.0/fan1",
            "gpu:0000:01:00.0/powerLimit", "gpu:0000:01:00.0/vramUsed", "cpu/thread/3/mhz", "cpu/thread/15/load", "cpu/tctl",
            "mem/avail", "mem/swap",
        ];
        for k in keys {
            let d = status_label(k);
            assert!(d.label.chars().count() <= 12, "{k}: «{}» er for langt", d.label);
            for unit in ["MHz", "o/min", "°C", " W", "%"] {
                assert!(!d.label.contains(unit), "{k}: «{}» gjentar enheten", d.label);
            }
        }
        assert_eq!(status_label("gpu:0000:01:00.0/coreClock").label, "Kjerneklokke");
        assert_eq!(status_label("gpu:0000:01:00.0/memClock").label, "Minneklokke");
        assert_eq!(status_label("cpu/thread/3/mhz").label, "Tråd 3");
        assert_eq!(status_label("gpu:0000:01:00.0/fanRpm0").label, "Turtall 1");
    }

    /// Diskene i statusblokken har kjernenavnet (like disker har samme
    /// modell), og det lange navnet har modellen. Uten disklista: «Disktemp».
    #[test]
    fn disknøklene_har_disken_i_navnet() {
        use rigg_core::disk::DiskSample;
        let disks = vec![
            DiskSample { id: "S5SVNF0NC41586R".into(), name: "sda".into(), model: "Samsung SSD 870 QVO 1TB".into(), ..Default::default() },
            DiskSample { id: "S6B0NL0W512345".into(), name: "nvme0n1".into(), model: "Samsung SSD 980 PRO 1TB".into(), ..Default::default() },
        ];
        let n = |k: &str| crate::pages::sensorer::disk_name(k, &disks);
        assert_eq!(n("disk/S5SVNF0NC41586R/temp"), Some(("sda temp".into(), "Samsung SSD 870 QVO 1TB (sda) · temperatur".into())));
        assert_eq!(n("disk/S6B0NL0W512345/write").unwrap().0, "nvme0n1 skriv");
        assert_eq!(n("disk/read"), None, "summen er ikke én disk");
        assert_eq!(n("disk/ukjent/temp"), None);
        for k in ["disk/S6B0NL0W512345/temp", "disk/S6B0NL0W512345/read", "disk/S6B0NL0W512345/busy"] {
            assert!(n(k).unwrap().0.chars().count() <= 13, "{k}");
        }
        assert_eq!(status_label("disk/x/temp").label, "Disktemp");
        assert_eq!(status_label("disk/x/temp").src, "disk");
    }

    #[test]
    fn sparklinjene_har_minste_spenn() {
        assert_eq!(status_label("gpu:0000:01:00.0/fanRpm0").opts.min_span, 200.0);
        assert_eq!(status_label("cpu/tctl").opts.min_span, 5.0);
        assert_eq!(status_label("gpu:0000:01:00.0/powerLimit").opts.min_span, 20.0);
        assert_eq!(status_label("cpu/thread/3/load").opts.min_span, 10.0);
        assert_eq!(status_label("gpu:0000:01:00.0/coreClock").opts.min_span, 400.0);
    }
}
