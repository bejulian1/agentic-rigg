//! Sensorer (demoens `sensors.js`, v0.0.9): én tett tabell i HWiNFO-stil,
//! gruppert etter enhet (Skjermkort, Prosessor, Minne, Disker, Andre sensorer, Nett),
//! med kolonnene Nå, Min, Maks, Snitt og en sparklinje for siste 60 s.
//! Overskriftsraden står én gang og blir stående øverst når siden rulles.
//!
//! - **Min, maks og snitt lever i tjenesten** (`get_stats`/hendelsen `stats`),
//!   så de dekker tiden vinduet var lukket. «Min/maks siden 14:32» står i
//!   toppfeltet. ↺ og Ctrl+R nullstiller med Angre i 5 s: tjenesten får
//!   beskjeden først når fristen er ute, og tabellen viser imens tallene fra
//!   trykket.
//! - **Graf for én sensor:** klikk på navnet (eller Enter) åpner et graf-panel
//!   under raden, med historikk fra tjenesten (1 min–48 t, eget tidsvindu).
//! - **Feste:** nålen ved navnet (synlig ved pek og fokus, alltid på festede
//!   rader) fester raden i statusblokken. Høyreklikk gir også «Ta med i
//!   loggen», «Nullstill min og maks» og rekkefølgen i statusblokken.
//! - **Tastatur:** én Tab-stopp per gruppe. Pil opp/ned mellom radene,
//!   venstre/høyre mellom nålen og navnet, Home/End, Enter åpner grafen og
//!   Esc lukker den. Ctrl+F filtrerer.
//! - Det kortet ikke støtter (NOT_SUPPORTED), står som «Ikke støttet» i raden.
//!   Det som ikke kan leses i det hele tatt (hotspot, spenning, hovedkortet),
//!   står under «Ikke tilgjengelig», med hvorfor.
//!
//! **Eies av 2A (Data).**

use super::{Key, Page};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::chart::{self, ChartOpts, ChartPanel, ChartSet, PanelOpts, SeriesDef, math};
use crate::widgets::popover::Menu;
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Icon, Panel, Spark, class, hbox, label, set_text, tip, vbox};
use gtk::gdk::Key as K;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, pango};
use rigg_core::fmt as F;
use rigg_core::sensor_key as SK;
use rigg_core::sensormeta as SM;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// ---------------------------------------------------------------- Raden som layout

mod trow_imp {
    use super::*;

    #[derive(Default)]
    pub struct TRow {
        pub sparks: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TRow {
        const NAME: &'static str = "RiggSensorRow";
        type Type = super::TRow;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("snrow");
        }
    }

    impl ObjectImpl for TRow {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for TRow {
        fn measure(&self, o: gtk::Orientation, _f: i32) -> (i32, i32, i32, i32) {
            let mut min = 0;
            let mut nat = 0;
            let mut c = self.obj().first_child();
            while let Some(w) = c {
                let (a, b, _, _) = w.measure(o, -1);
                if o == gtk::Orientation::Vertical {
                    min = min.max(a);
                    nat = nat.max(b);
                } else {
                    nat += b;
                }
                c = w.next_sibling();
            }
            (min, nat, -1, -1)
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            let kids: Vec<gtk::Widget> = {
                let mut v = Vec::new();
                let mut c = self.obj().first_child();
                while let Some(x) = c {
                    v.push(x.clone());
                    c = x.next_sibling();
                }
                v
            };
            let cols = super::columns(w, self.sparks.get());
            for (k, (x, cw)) in kids.iter().zip(cols) {
                if cw <= 0 {
                    // Skjult kolonne: utenfor raden (raden klipper).
                    let mw = k.measure(gtk::Orientation::Horizontal, -1).0;
                    k.size_allocate(&gtk::Allocation::new(w + 50, 0, mw.max(1), h), -1);
                } else {
                    let mw = k.measure(gtk::Orientation::Horizontal, -1).0;
                    k.size_allocate(&gtk::Allocation::new(x, 0, cw.max(mw.min(cw.max(1))), h), -1);
                }
            }
        }
    }
}

glib::wrapper! {
    /// En rad i tabellen: Sensor, Nå, Min, Maks, Snitt og 60 s. Kolonnene
    /// er like i alle radene og i overskriften, fordi de regnes ut fra bredden.
    pub struct TRow(ObjectSubclass<trow_imp::TRow>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TRow {
    fn new(cells: &[gtk::Widget], sparks: bool) -> TRow {
        let r: TRow = glib::Object::new();
        r.imp().sparks.set(sparks);
        r.set_overflow(gtk::Overflow::Hidden);
        for c in cells {
            c.set_parent(&r);
        }
        r
    }

    fn set_sparks(&self, on: bool) {
        if self.imp().sparks.replace(on) != on {
            self.queue_allocate();
        }
    }
}

const NUM_W: i32 = 84;

/// (x, bredde) for de seks kolonnene. Under 760 px faller sparklinja bort,
/// under 520 px også Min og Snitt (Sensor, Nå og Maks står igjen).
fn columns(w: i32, sparks: bool) -> [(i32, i32); 6] {
    let spark = if sparks && w >= 760 { (w as f32 * 0.3) as i32 } else { 0 };
    let narrow = w < 520;
    let nums = if narrow { 2 } else { 4 };
    // Tallkolonnene vokser med skriften, så «10,93 MB/s» får plass med «Stor».
    let num_w = (NUM_W as f64 * crate::theme::font_scale().max(1.0)).round() as i32;
    let name = (w - spark - num_w * nums).max(60);
    let mut x = name;
    let mut col = |show: bool| {
        if show {
            let r = (x, num_w);
            x += num_w;
            r
        } else {
            (0, 0)
        }
    };
    let now = col(true);
    let min = col(!narrow);
    let max = col(true);
    let avg = col(!narrow);
    [(0, name), now, min, max, avg, (x, spark)]
}

// ---------------------------------------------------------------- Radene

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fmt {
    Temp,
    Pct,
    Pct0,
    Mhz,
    W,
    W0,
    Mib,
    Gib,
    Rpm,
    Volt,
    Amp,
    Gen,
    Rate,
    /// Diskene: alltid desimale MB/s («12,3 MB/s», «420 MB/s»), som diskverktøyene.
    DiskRate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lvl {
    None,
    GpuTemp,
    CpuTemp,
    Load,
}

#[derive(Clone)]
struct RowDef {
    key: String,
    label: String,
    fmt: Fmt,
    tip: Option<String>,
    raw: Option<String>,
    fixed: Option<(f64, f64)>,
    min_span: f64,
    discrete: bool,
    thread: Option<usize>,
    extra: Option<String>,
    serie: &'static str,
    lvl: Lvl,
    na: bool,
}

impl RowDef {
    fn new(key: &str, label: &str, fmt: Fmt, serie: &'static str) -> RowDef {
        RowDef {
            key: key.into(),
            label: label.into(),
            fmt,
            tip: None,
            raw: None,
            fixed: None,
            min_span: 0.0,
            discrete: false,
            thread: None,
            extra: None,
            serie,
            lvl: Lvl::None,
            na: false,
        }
    }
    fn pct(mut self) -> Self {
        self.fixed = Some((0.0, 100.0));
        self
    }
    fn span(mut self, s: f64) -> Self {
        self.min_span = s;
        self
    }
    fn tip(mut self, t: &str) -> Self {
        self.tip = Some(t.into());
        self
    }
    fn raw(mut self, r: &str) -> Self {
        self.raw = Some(r.into());
        self
    }
    fn lvl(mut self, l: Lvl) -> Self {
        self.lvl = l;
        self
    }
    fn is_temp(&self) -> bool {
        self.fmt == Fmt::Temp
    }
}

struct Row {
    def: RowDef,
    root: TRow,
    pin: gtk::Button,
    pin_icon: Icon,
    name: gtk::Button,
    extra: gtk::Label,
    now: gtk::Label,
    min: gtk::Label,
    max: gtk::Label,
    avg: gtk::Label,
    spark: Spark,
    level: Cell<&'static str>,
    hidden: Cell<bool>,
}

struct Group {
    id: String,
    root: gtk::Box,
    btn: gtk::Button,
    chev: Icon,
    state: gtk::Box,
    state_sig: RefCell<String>,
    rows: Vec<Rc<Row>>,
    threads: Option<(gtk::Button, Icon, gtk::Box)>,
}

struct Detail {
    key: String,
    root: gtk::Box,
    panel: Rc<ChartPanel>,
}

struct Pending {
    keys: Option<HashSet<String>>,
    acc: HashMap<String, (f64, f64, f64, u64)>,
    since: i64,
    timer: Option<glib::SourceId>,
}

struct Inner {
    app: Rc<App>,
    root: gtk::Overlay,
    actions: gtk::Box,
    scroller: gtk::ScrolledWindow,
    content: gtk::Box,
    table: gtk::Box,
    sticky: TRow,
    header: TRow,
    groups_box: gtk::Box,
    empty: gtk::Box,
    empty_text: gtk::Label,
    notes_panel: Panel,
    notes_box: gtk::Box,
    search: gtk::SearchEntry,
    since: gtk::Label,
    toggle_icon: Icon,
    toggle: gtk::Button,
    reset: gtk::Button,
    groups: RefCell<Vec<Rc<Group>>>,
    sig: RefCell<String>,
    system: RefCell<serde_json::Value>,
    system_loading: Cell<bool>,
    cpu_topology: RefCell<rigg_core::report::system::CpuTopologySync>,
    filter: RefCell<String>,
    collapsed: RefCell<HashSet<String>>,
    threads_open: Cell<bool>,
    detail: RefCell<Option<Detail>>,
    set: Rc<ChartSet>,
    visible: Cell<bool>,
    pending: RefCell<Option<Pending>>,
    units: RefCell<HashMap<String, F::RateUnit>>,
    menu_open: Cell<bool>,
    /// Sparklinjene er fylt fra historikken for denne tilkoblingen.
    prefilled: Cell<bool>,
}

pub struct Sensorer {
    inner: Rc<Inner>,
}

/// Kort navn i statusblokken (sidefeltet har ca. 100 px til navnet), som
/// demoens `shortName`. `None` for nøkler den ikke kjenner.
pub fn short_name(key: &str) -> Option<String> {
    if let Some((_, f)) = SK::split_gpu(key) {
        let s = match f {
            "temp" => "GPU-temp".to_string(),
            "gpuUtil" => "GPU-last".into(),
            "memUtil" => "Minnekontr.".into(),
            "encUtil" => "Encoder".into(),
            "decUtil" => "Decoder".into(),
            "coreClock" => "Kjerneklokke".into(),
            "smClock" => "SM-klokke".into(),
            "memClock" => "Minneklokke".into(),
            "videoClock" => "Videoklokke".into(),
            "power" => "Effekt".into(),
            "powerLimit" => "Effektgrense".into(),
            "fan" => "Vifte".into(),
            "vramUsed" => "VRAM".into(),
            "pcieGen" => "PCIe".into(),
            f if f.starts_with("fanRpm") => format!("Vifte {} o/min", f[6..].parse::<usize>().map(|n| n + 1).unwrap_or(1)),
            f if f.starts_with("fan") => format!("Vifte {}", f[3..].parse::<usize>().map(|n| n + 1).unwrap_or(1)),
            _ => return None,
        };
        return Some(s);
    }
    let parts: Vec<&str> = key.split('/').collect();
    Some(match parts.as_slice() {
        ["cpu", "temp"] => "CPU-temp".into(),
        ["cpu", "tctl"] => "CPU Tctl".into(),
        ["cpu", "load"] => "CPU-last".into(),
        ["cpu", "thread", n, "load"] => format!("CPU-tråd {n}"),
        ["cpu", "thread", n, "mhz"] => format!("Tråd {n}"),
        ["mem", "used"] => "RAM brukt".into(),
        ["mem", "usedPct"] => "Minne".into(),
        ["mem", "avail"] => "RAM ledig".into(),
        ["mem", "swap"] => "Swap".into(),
        ["net", "rx"] => "Mottatt".into(),
        ["net", "tx"] => "Sendt".into(),
        ["net", name, d] => format!("{name} {}", if *d == "rx" { "↓" } else { "↑" }),
        ["disk", "read"] => "Disk les".into(),
        ["disk", "write"] => "Disk skriv".into(),
        // Uten disklista (før første prøve): bare hva det er. Med lista gir
        // `disk_name` kjernenavnet («sda temp»).
        ["disk", _, "temp"] => "Disktemp".into(),
        ["disk", _, "read"] => "Disk les".into(),
        ["disk", _, "write"] => "Disk skriv".into(),
        ["disk", _, "busy"] => "Disk aktiv".into(),
        ["hwmon", dev, k] => format!("{} {k}", SM::device_name(dev.trim_end_matches(|c: char| c == '_' || c.is_ascii_digit()))),
        _ => return None,
    })
}

/// Navnet på en verdi for én disk (`disk/<id>/temp|read|write|busy`), med
/// disken fra lista: (kort, langt), for eksempel («sda temp»,
/// «Samsung SSD 870 QVO 1TB (sda) · temperatur»). Det korte navnet bruker
/// kjernenavnet, som står på Disker, fordi like disker har samme modell.
/// `None` når nøkkelen ikke er for én disk, eller disken ikke er i lista.
pub fn disk_name(key: &str, disks: &[rigg_core::disk::DiskSample]) -> Option<(String, String)> {
    let parts: Vec<&str> = key.split('/').collect();
    let ["disk", id, what] = parts.as_slice() else { return None;
    };
    let d = disks.iter().find(|d| d.id == *id)?;
    let (short, long) = match *what {
        "temp" => ("temp", "temperatur"),
        "read" => ("les", "lesing"),
        "write" => ("skriv", "skriving"),
        "busy" => ("aktiv", "aktivitet"),
        _ => return None,
    };
    let base = if d.model.is_empty() { d.name.clone() } else { format!("{} ({})", d.model, d.name) };
    Some((format!("{} {short}", d.name), format!("{base} · {long}")))
}

/// ID-en som lagres når raden festes: den faste verdien når raden er den
/// samme (så navn og format i statusblokken blir like), ellers nøkkelen.
fn pin_id(app: &App, key: &str) -> String {
    let sel = app.gpu_id();
    if let Some((id, f)) = SK::split_gpu(key)
        && sel.as_deref() == Some(id)
    {
        let fixed = match f {
            "temp" => Some("gpuTemp"),
            "gpuUtil" => Some("gpuLoad"),
            "power" => Some("gpuPower"),
            "fan" => Some("gpuFan"),
            _ => None,
        };
        if let Some(x) = fixed {
            return x.into();
        }
    }
    match key {
        "cpu/temp" => "cpuTemp".into(),
        "cpu/load" => "cpuLoad".into(),
        "mem/usedPct" => "mem".into(),
        "net/rx" => "netDown".into(),
        "net/tx" => "netUp".into(),
        k => k.into(),
    }
}

/// Raden en lenke peker på: nøkkelen, en fast verdi fra statusblokken eller
/// demoens rad-ID (`gpu/temp`, `cpu/t3`).
fn resolve(app: &App, id: &str) -> String {
    let g = |f: &str| app.gpu_key(f).unwrap_or_default();
    match id {
        "gpuTemp" | "gpu/temp" => g("temp"),
        "gpuLoad" | "gpu/gpuUtil" => g("gpuUtil"),
        "gpuPower" | "gpu/power" => g("power"),
        "gpuFan" | "gpu/fan" => g("fan"),
        "vram" | "gpu/vramUsed" => g("vramUsed"),
        "cpuTemp" => "cpu/temp".into(),
        "cpuLoad" => "cpu/load".into(),
        "mem" => "mem/usedPct".into(),
        x if x.starts_with("gpu/") => g(&x[4..]),
        x if x.starts_with("cpu/t") && x[5..].chars().all(|c| c.is_ascii_digit()) => {
            format!("cpu/thread/{}/load", &x[5..])
        }
        x => x.into(),
    }
}

fn fmt_value(f: Fmt, v: Option<f64>, unit: Option<&F::RateUnit>) -> String {
    match f {
        Fmt::Temp => F::temp(v, F::dec(1)),
        Fmt::Pct => F::pct(v, F::dec(1)),
        Fmt::Pct0 => F::pct(v, F::dec(0)),
        Fmt::Mhz => F::mhz(v),
        Fmt::W => F::watt(v, F::dec(1)),
        Fmt::W0 => F::watt(v, F::dec(0)),
        Fmt::Mib => F::mib(v, None, Some("MiB")),
        Fmt::Gib => F::bytes(v.map(|g| g * 1024.0 * 1024.0 * 1024.0), 2),
        Fmt::Rpm => F::rpm(v),
        Fmt::Volt => F::with_unit(F::num(v, 3), "V"),
        Fmt::Amp => F::with_unit(F::num(v, 3), "A"),
        Fmt::Gen => math::gen_text(v),
        Fmt::Rate => F::rate(v, unit),
        Fmt::DiskRate => super::disker::mbps(v),
    }
}

/// Tallet og enheten hver for seg, så enheten kan dempes (v0.0.9).
fn set_num(l: &gtk::Label, text: &str) {
    let markup = match text.rsplit_once(F::NNBSP) {
        Some((n, u)) => format!("{}<span alpha=\"62%\" weight=\"normal\">{}{}</span>", glib::markup_escape_text(n), F::NNBSP, glib::markup_escape_text(u)),
        None => glib::markup_escape_text(text).to_string(),
    };
    if l.label() != markup {
        l.set_markup(&markup);
    }
}

fn hwmon_fmt(kind: &str) -> (Fmt, &'static str) {
    match kind {
        "temp" => (Fmt::Temp, "temp"),
        "fan" => (Fmt::Rpm, "clock"),
        "in" => (Fmt::Volt, "load"),
        "power" => (Fmt::W, "power"),
        "curr" => (Fmt::Amp, "load"),
        _ => (Fmt::Pct, "load"),
    }
}

/// Skjermkortene i tabellen: de som har prøve, og de som står i lista fra
/// tjenesten uten prøve (kortet svarer ikke). Da får de samme rader som før,
/// med «–», så gruppa ikke forsvinner og kommer tilbake.
fn gpu_rows_source(app: &App, tel: &rigg_core::model::Telemetry) -> Vec<(String, rigg_core::model::GpuSample)> {
    let d = app.data.borrow();
    let mut out: Vec<(String, rigg_core::model::GpuSample)> = tel.gpus.iter().map(|(id, s)| (id.clone(), s.clone())).collect();
    for g in d.gpus.iter().filter(|g| !g.id.is_empty() && !tel.gpus.contains_key(&g.id)) {
        let n = d.statics.get(&g.id).and_then(|s| s.0.fan_count).unwrap_or(1) as usize;
        let s = rigg_core::model::GpuSample { fans: vec![None; n], fan_rpm: vec![None; n], ..Default::default() };
        out.push((g.id.clone(), s));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Rad-definisjonene for prøven som er kommet. Rekkefølgen er tabellens.
fn row_defs(app: &App, system: &serde_json::Value) -> Vec<(String, String, String, Vec<RowDef>)> {
    let Some(tel) = app.telemetry() else { return vec![];
    };
    let mut out = Vec::new();
    let cards = gpu_rows_source(app, &tel);
    let multi = cards.len() > 1;
    let gpus = app.data.borrow().gpus.clone();
    for (n, (id, s)) in cards.iter().enumerate() {
        let gid = format!("gpu:{id}");
        let k = |f: &str| SK::gpu(id, f);
        let na = |f: &str| {
            let base = if f.starts_with("fanRpm") {
                "fanRpm"
            } else if f.starts_with("fan") && f.len() > 3 {
                "fanN"
            } else {
                f
            };
            s.na.iter().any(|x| x == base || x == f)
        };
        let mut rows = vec![
            RowDef::new(&k("temp"), "Temperatur", Fmt::Temp, "temp").span(20.0).lvl(Lvl::GpuTemp).raw("NVML · den eneste temperatursensoren på kortet"),
            RowDef::new(&k("gpuUtil"), "GPU-last", Fmt::Pct0, "load").pct().lvl(Lvl::Load),
            RowDef::new(&k("memUtil"), "Minnekontroller-last", Fmt::Pct0, "memctl").pct(),
            RowDef::new(&k("encUtil"), "Videokoder (encoder)", Fmt::Pct0, "memctl").pct(),
            RowDef::new(&k("decUtil"), "Videodekoder (decoder)", Fmt::Pct0, "memctl").pct(),
            RowDef::new(&k("coreClock"), "Kjerneklokke", Fmt::Mhz, "clock").span(400.0),
            RowDef::new(&k("smClock"), "SM-klokke", Fmt::Mhz, "clock").span(400.0),
            RowDef::new(&k("memClock"), "Minneklokke", Fmt::Mhz, "mem").span(400.0),
            RowDef::new(&k("videoClock"), "Videoklokke", Fmt::Mhz, "clock").span(400.0),
            RowDef::new(&k("power"), "Effekttrekk", Fmt::W, "power").span(40.0).tip("Snitt for perioden, regnet fra kortets energiteller."),
            RowDef::new(&k("powerLimit"), "Effektgrense", Fmt::W0, "limit").tip("Grensen kortet håndhever nå."),
        ];
        if s.fans.len() > 1 {
            for i in 0..s.fans.len() {
                rows.push(RowDef::new(&k(&format!("fan{i}")), &format!("Vifte {}", i + 1), Fmt::Pct0, "clock").pct());
            }
        } else {
            rows.push(RowDef::new(&k("fan"), "Viftefart", Fmt::Pct0, "clock").pct());
        }
        let nrpm = s.fan_rpm.len();
        for i in 0..nrpm {
            let l = if nrpm > 1 { format!("Vifte {}, turtall", i + 1) } else { "Viftens turtall".into() };
            rows.push(RowDef::new(&k(&format!("fanRpm{i}")), &l, Fmt::Rpm, "clock").span(200.0));
        }
        rows.push(RowDef::new(&k("vramUsed"), "Videominne brukt", Fmt::Mib, "mem").raw("NVML · nvmlDeviceGetMemoryInfo_v2"));
        let mut pcie = RowDef::new(&k("pcieGen"), "PCIe-generasjon", Fmt::Gen, "limit")
            .tip("Generasjonen lenken går på nå. Kortet går ned i tomgang for å spare strøm.")
            .raw("sysfs · current_link_speed");
        pcie.discrete = true;
        rows.push(pcie);
        for r in &mut rows {
            let f = SK::split_gpu(&r.key).map(|x| x.1.to_string()).unwrap_or_default();
            r.na = na(&f);
            if r.raw.is_none() {
                r.raw = Some("NVML".into());
            }
        }
        let name = gpus.iter().find(|g| &g.id == id).map(|g| rigg_core::model::short_gpu_name(&g.name)).or_else(|| app.data.borrow().statics.get(id).map(|s| rigg_core::model::short_gpu_name(&s.0.name))).unwrap_or_default();
        let title = if multi { format!("Skjermkort {} · {name}", n + 1) } else { "Skjermkort".into() };
        let note = format!("{} · NVML", if name.is_empty() { "NVIDIA".to_string() } else { name });
        out.push((gid, title, note, rows));
    }
    let sys = &tel.sys;
    // ---- Prosessor ----
    let mut cpu = Vec::new();
    let dev = sys.hwmon.iter().find(|d| matches!(d.name.as_str(), "k10temp" | "coretemp" | "zenpower"));
    let ct = SM::cpu_temp_sensor(sys).map(|x| x.id.clone());
    if let Some(d) = dev {
        for x in &d.sensors {
            if x.value.is_none() && x.err.is_none() {
                continue;
            }
            let (f, serie) = hwmon_fmt(&x.kind);
            let r = if Some(&x.id) == ct.as_ref() {
                let label = if !x.label.is_empty() && x.label != x.key { format!("Temperatur ({})", x.label) } else { "CPU-temperatur".into() };
                RowDef::new("cpu/temp", &label, Fmt::Temp, "temp").span(20.0).lvl(Lvl::CpuTemp).tip("Temperatur fra den valgte CPU-sensoren. Samme verdi brukes i sidefeltet, på Oversikt og i Grafer.")
            } else if x.label == "Tctl" {
                RowDef::new("cpu/tctl", "Temperatur (Tctl)", Fmt::Temp, "temp")
                    .span(20.0)
                    .tip("Tctl rapporteres av CPU-driveren. Eventuell forskyvning fra brikketemperaturen avhenger av CPU-modellen.")
            } else {
                let (n, _) = SM::sensor_name(d, x);
                let mut r = RowDef::new(&format!("hwmon/{}/{}", d.name, x.key), n.trim_start_matches("Prosessor · "), f, serie);
                if f == Fmt::Temp {
                    r.min_span = 20.0;
                }
                r
            };
            cpu.push(r.raw(&format!("{} · {}", d.name, x.key)));
        }
    }
    cpu.push(RowDef::new("cpu/load", "Total last", Fmt::Pct, "load").pct().lvl(Lvl::Load).raw("/proc/stat"));
    for i in rigg_core::report::system::cpu_thread_ids(&system["cpu"]) {
        let mut r = RowDef::new(&format!("cpu/thread/{i}/load"), &format!("Tråd {i}"), Fmt::Pct, "load").pct().lvl(Lvl::Load).raw("/proc/stat · cpufreq");
        r.thread = Some(i);
        cpu.push(r);
    }
    out.push(("cpu".into(), "Prosessor".into(), "/proc/stat, cpufreq og hwmon".into(), cpu));
    // ---- Minne ----
    out.push((
        "mem".into(),
        "Minne".into(),
        "/proc/meminfo".into(),
        vec![
            RowDef::new("mem/used", "Brukt", Fmt::Gib, "mem").tip("Det programmene bruker. Buffer og cache er ikke med."),
            RowDef::new("mem/usedPct", "Brukt av totalt", Fmt::Pct, "mem").pct(),
            RowDef::new("mem/avail", "Tilgjengelig", Fmt::Gib, "mem").tip("Det som kan tas i bruk uten å bytte ut noe (MemAvailable)."),
            RowDef::new("mem/swap", "Swap brukt", Fmt::Gib, "mem"),
        ],
    ));
    // ---- Disker (runde 6A): temperatur fra SMART, MB/s og aktivitet ----
    let mut dk = Vec::new();
    for d in &sys.disks {
        // «sda · lesing · ST2000DM008-2FR102»: kjernenavnet og verdien først, så
        // de fire radene for én disk skilles også når navnet kuttes (640 px).
        let lbl = |what: &str| {
            if d.model.is_empty() { format!("{} · {what}", d.name) } else { format!("{} · {what} · {}", d.name, d.model) }
        };
        if d.temp.is_some() {
            dk.push(
                RowDef::new(&format!("disk/{}/temp", d.id), &lbl("temperatur"), Fmt::Temp, "temp")
                    .span(10.0)
                    .tip("Fra SMART. UDisks2 leser den selv omtrent hvert 10. min, uten å vekke disken.")
                    .raw("SMART · UDisks2"),
            );
        }
        dk.push(RowDef::new(&format!("disk/{}/read", d.id), &lbl("lesing"), Fmt::DiskRate, "load").raw(&format!("{} · /proc/diskstats", d.name)));
        dk.push(RowDef::new(&format!("disk/{}/write", d.id), &lbl("skriving"), Fmt::DiskRate, "memctl").raw(&format!("{} · /proc/diskstats", d.name)));
        dk.push(
            RowDef::new(&format!("disk/{}/busy", d.id), &lbl("aktivitet"), Fmt::Pct0, "load")
                .pct()
                .tip("Andel av tiden disken hadde lesing eller skriving i gang.")
                .raw(&format!("{} · io_ticks", d.name)),
        );
    }
    if !dk.is_empty() {
        out.push(("disk".into(), "Disker".into(), "/proc/diskstats og SMART fra UDisks2".into(), dk));
    }
    // ---- Andre sensorer ----
    let mut hw = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for d in &sys.hwmon {
        let n = seen.iter().filter(|x| **x == d.name).count();
        seen.push(&d.name);
        if Some(d.name.as_str()) == dev.map(|x| x.name.as_str()) {
            continue;
        }
        let kname = if n == 0 { d.name.clone() } else { format!("{}_{}", d.name, n + 1) };
        // USB-C-kontrolleren på kortet viser alltid 0 uten tilkoblet enhet (revisjonen kap. 4).
        let ucsi_idle = d.name.starts_with("ucsi_source_psy") && d.sensors.iter().all(|x| x.value.unwrap_or(0.0) == 0.0);
        if ucsi_idle {
            continue;
        }
        for x in &d.sensors {
            if x.value.is_none() {
                continue;
            }
            let (nm, raw) = SM::sensor_name(d, x);
            let (f, serie) = hwmon_fmt(&x.kind);
            let mut r = RowDef::new(&format!("hwmon/{kname}/{}", x.key), &nm, f, serie).raw(&raw);
            if f == Fmt::Temp {
                r.min_span = 20.0;
            }
            hw.push(r);
        }
    }
    if !hw.is_empty() {
        out.push(("hwmon".into(), "Andre sensorer".into(), "/sys/class/hwmon".into(), hw));
    }
    // ---- Nett ----
    let mut net = Vec::new();
    let mut names: Vec<(&String, &rigg_core::model::NetSample)> = sys.net.iter().collect();
    names.sort_by_key(|(n, _)| (!SM::is_physical_net(n), n.to_string()));
    for (name, n) in names {
        let base = SM::net_name(name, n.wireless);
        let down = n.state.as_deref().is_some_and(|s| s != "up" && s != "unknown");
        for (dir, word, serie) in [("rx", "mottatt", "load"), ("tx", "sendt", "memctl")] {
            let mut r = RowDef::new(&format!("net/{name}/{dir}"), &format!("{base} · {word}"), Fmt::Rate, serie).raw(&format!("{name} · {dir}_bytes"));
            if down {
                r.extra = Some("ikke tilkoblet".into());
            }
            net.push(r);
        }
    }
    if !net.is_empty() {
        out.push(("net".into(), "Nett".into(), "/sys/class/net".into(), net));
    }
    out
}

fn signature(app: &App, system: &serde_json::Value) -> String {
    let Some(tel) = app.telemetry() else { return String::new();
    };
    let mut s = String::new();
    for (id, g) in &gpu_rows_source(app, &tel) {
        s.push_str(&format!("{id}:{}:{}:{:?};", g.fans.len(), g.fan_rpm.len(), g.na));
    }
    s.push_str(&format!("|{:?}|", rigg_core::report::system::cpu_thread_ids(&system["cpu"])));
    for (n, x) in &tel.sys.net {
        s.push_str(&format!("{n}:{:?},", x.state));
    }
    for d in &tel.sys.disks {
        s.push_str(&format!("d:{}:{}:{},", d.id, d.model, d.temp.is_some() as u8));
    }
    for d in &tel.sys.hwmon {
        s.push_str(&d.name);
        for x in &d.sensors {
            s.push_str(&format!("{}:{},", x.key, x.value.is_some() as u8 + if d.name.starts_with("ucsi") && x.value.unwrap_or(0.0) != 0.0 { 2 } else { 0 }));
        }
    }
    s
}

// ---------------------------------------------------------------- Siden

impl Page for Sensorer {
    fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }

    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.inner.actions.clone().upcast())
    }

    fn scrolls(&self) -> bool {
        false
    }

    fn show(&self, q: Option<&str>) {
        let i = &self.inner;
        i.visible.set(true);
        i.set.set_visible(true);
        i.load_system();
        i.rebuild_if_needed();
        i.apply_filter();
        i.prefill();
        i.paint();
        i.render_notes();
        if let Some(q) = q
            && let Some(v) = q.split('&').find_map(|p| p.strip_prefix("vis="))
        {
            i.reveal(&resolve(&i.app, v));
        }
    }

    fn hide(&self) {
        self.inner.visible.set(false);
        self.inner.set.set_visible(false);
    }

    fn on_key(&self, k: &Key) -> bool {
        self.inner.on_key(k)
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    // ---- Toppfeltet ----
    let actions = hbox(4, &[]);
    let toggle = gtk::Button::new();
    toggle.add_css_class("flat");
    toggle.add_css_class("icon-only");
    let toggle_icon = Icon::new("chevrons-down-up", 16);
    toggle.set_child(Some(&toggle_icon));
    tip(&toggle, "Fold sammen alle", None);
    toggle.update_property(&[gtk::accessible::Property::Label("Fold sammen alle")]);
    actions.append(&toggle);
    let sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sep.add_css_class("sep");
    sep.set_valign(gtk::Align::Center);
    actions.append(&sep);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Filtrer"));
    search.add_css_class("search-field");
    search.add_css_class("sensor-filter");
    search.set_width_chars(10);
    search.update_property(&[gtk::accessible::Property::Label("Filtrer sensorer")]);
    tip(&search, "Filtrer sensorer. Esc tømmer filteret og avslutter søket.", Some("Ctrl+F"));
    search.set_valign(gtk::Align::Center);
    actions.append(&search);
    let since = label("–", &["head-note"]);
    since.set_tooltip_text(Some("Min, maks og snitt regnes fra tjenesten startet eller du trykket ↺."));
    // I smale vinduer faller «Min/maks siden …» bort (verktøytipset på ↺ sier det samme).
    actions.append(&chart::Adaptive::new(&since, &gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let reset = gtk::Button::new();
    reset.add_css_class("flat");
    reset.add_css_class("icon-only");
    reset.set_child(Some(&Icon::new("rotate-ccw", 16)));
    tip(&reset, "Nullstill min, maks og snitt", Some("Ctrl+R"));
    reset.update_property(&[gtk::accessible::Property::Label("Nullstill min, maks og snitt")]);
    actions.append(&reset);
    if let Some(log) = crate::logg_ui::button(app) {
        let sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sep.add_css_class("sep");
        sep.set_valign(gtk::Align::Center);
        actions.append(&sep);
        actions.append(&log);
    }

    // ---- Tabellen ----
    let content = vbox(8, &["page", "sn-page"]);
    let table = vbox(0, &["group", "sn-panel"]);
    table.set_overflow(gtk::Overflow::Hidden);
    table.update_property(&[gtk::accessible::Property::Label("Sensorer")]);
    let sparks = app.settings().sensor_sparks;
    let header = head_row(sparks, false);
    table.append(&header);
    let groups_box = vbox(0, &["sn-groups"]);
    table.append(&groups_box);
    content.append(&table);
    let empty = vbox(8, &["empty", "sn-empty"]);
    empty.set_halign(gtk::Align::Center);
    empty.append(&Icon::new("search", 32));
    let empty_text = label("Ingen sensorer passer filteret.", &[]);
    empty_text.set_wrap(true);
    empty_text.set_justify(gtk::Justification::Center);
    empty.append(&empty_text);
    empty.set_visible(false);
    content.append(&empty);
    let notes_panel = Panel::new("Ikke tilgjengelig");
    notes_panel.aside.append(&label("Det som ikke kan leses, og hvorfor", &[]));
    let notes_box = vbox(0, &["sn-notes"]);
    notes_panel.body.append(&notes_box);
    notes_panel.root.set_visible(false);
    content.append(&notes_panel.root);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&content));
    scroller.set_vexpand(true);
    let root = gtk::Overlay::new();
    root.set_child(Some(&scroller));
    let sticky = head_row(sparks, true);
    sticky.set_valign(gtk::Align::Start);
    sticky.set_visible(false);
    root.add_overlay(&sticky);

    let inner = Rc::new(Inner {
        app: app.clone(),
        root,
        actions,
        scroller,
        content,
        table,
        sticky,
        header,
        groups_box,
        empty,
        empty_text,
        notes_panel,
        notes_box,
        search,
        since,
        toggle_icon,
        toggle,
        reset: reset.clone(),
        groups: RefCell::new(vec![]),
        sig: RefCell::new(String::new()),
        system: RefCell::new(serde_json::Value::Null),
        system_loading: Cell::new(false),
        cpu_topology: RefCell::default(),
        filter: RefCell::new(String::new()),
        collapsed: RefCell::new(HashSet::new()),
        threads_open: Cell::new(false),
        detail: RefCell::new(None),
        set: ChartSet::new(app),
        visible: Cell::new(false),
        pending: RefCell::new(None),
        units: RefCell::new(HashMap::new()),
        menu_open: Cell::new(false),
        prefilled: Cell::new(false),
    });
    inner.wire(&reset);
    inner.rebuild_if_needed();
    #[cfg(debug_assertions)]
    inner.hooks();
    Box::new(Sensorer { inner })
}

fn head_row(sparks: bool, sticky: bool) -> TRow {
    let mk = |t: &str, end: bool| {
        let l = label(t, &["sn-th"]);
        if end {
            l.set_xalign(1.0);
        }
        l.upcast::<gtk::Widget>()
    };
    let snitt = mk("Snitt", true);
    snitt.set_tooltip_text(Some("Snitt siden tjenesten startet eller siden siste nullstilling"));
    let cells = [mk("Sensor", false), mk("Nå", true), mk("Min", true), mk("Maks", true), snitt, mk("60 s", false)];
    cells[5].add_css_class("sn-th-spark");
    let r = TRow::new(&cells, sparks);
    r.add_css_class("sn-head");
    if sticky {
        r.add_css_class("sticky");
    }
    r.set_accessible_role(gtk::AccessibleRole::Row);
    r
}

impl Inner {
    fn wire(self: &Rc<Self>, reset: &gtk::Button) {
        // Filteret ligger i toppfeltets actions, utenfor sideroten. Lytt på
        // vinduet så også tom toppstripe slipper fokus, uten å spise klikket
        // på knapper, grafer eller vindusrammen.
        if let Some(win) = self.app.window() {
            let click = gtk::GestureClick::new();
            click.set_button(gtk::gdk::BUTTON_PRIMARY);
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            let w = Rc::downgrade(self);
            click.connect_pressed(move |g, _, x, y| {
                let Some(s) = w.upgrade().filter(|s| s.visible.get()) else { return };
                let inside = g.widget().and_then(|root| root.pick(x, y, gtk::PickFlags::DEFAULT)).is_some_and(|hit| hit == s.search.clone().upcast::<gtk::Widget>() || hit.is_ancestor(&s.search));
                if !inside {
                    s.release_filter_focus();
                }
            });
            win.add_controller(click);
        }
        // Rader som rulles fram, får ferske tall straks (paint tar bare de som synes).
        let w = Rc::downgrade(self);
        self.scroller.vadjustment().connect_value_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.paint();
            }
        });
        // Større vindu eller en gruppe som åpnes: flere rader synes.
        let w = Rc::downgrade(self);
        self.scroller.vadjustment().connect_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.paint();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_telemetry.connect(move |_| {
            let Some(s) = w.upgrade() else { return };
            s.track_pending();
            if s.visible.get() {
                s.sync_cpu_topology();
                s.rebuild_if_needed();
                s.paint();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_stats.connect(move |_| {
            if let Some(s) = w.upgrade()
                && s.visible.get()
            {
                s.paint();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_devices.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.sig.borrow_mut().clear();
                if s.visible.get() {
                    s.rebuild_if_needed();
                    s.render_notes();
                }
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_settings.connect(move |keys| {
            let Some(s) = w.upgrade() else { return };
            if keys.iter().any(|k| k == "sensorSparks") {
                let on = s.app.settings().sensor_sparks;
                s.header.set_sparks(on);
                s.sticky.set_sparks(on);
                for g in s.groups.borrow().iter() {
                    for r in &g.rows {
                        r.root.set_sparks(on);
                        r.spark.set_visible(on);
                    }
                }
            }
            if keys.iter().any(|k| k == "statusItems") {
                s.render_pins();
            }
            if keys.iter().any(|k| k == "windows")
                && let Some(d) = s.detail.borrow().as_ref()
            {
                d.panel.set_window(s.app.settings().windows.sensorer as i64);
            }
            if keys.iter().any(|k| matches!(k.as_str(), "unit" | "netUnit" | "memUnit" | "decimals" | "thresholds" | "clock")) {
                s.units.borrow_mut().clear();
                s.paint();
            }
            if keys.iter().any(|k| k == "surprises") && !s.filter.borrow().is_empty() {
                s.apply_filter();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_conn.connect(move |c| {
            if *c == crate::state::Conn::Connected
                && let Some(s) = w.upgrade()
            {
                s.prefilled.set(false);
                *s.system.borrow_mut() = serde_json::Value::Null;
                *s.cpu_topology.borrow_mut() = Default::default();
                if s.visible.get() {
                    s.load_system();
                    s.prefill();
                }
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_service.connect(move |_| {
            if let Some(s) = w.upgrade()
                && s.visible.get()
            {
                s.paint();
            }
        });
        // Tømming (kryssknapp eller Backspace) skjer straks. Selve tekstsøket
        // beholder GtkSearchEntry sin korte forsinkelse.
        let w = Rc::downgrade(self);
        self.search.connect_changed(move |e| {
            let Some(s) = w.upgrade() else { return };
            if e.text().trim().is_empty() && !s.filter.borrow().is_empty() {
                s.filter.borrow_mut().clear();
                if s.visible.get() {
                    s.apply_filter();
                }
            }
        });
        let w = Rc::downgrade(self);
        self.search.connect_search_changed(move |e| {
            if let Some(s) = w.upgrade() {
                let filter = e.text().trim().to_lowercase();
                if filter != *s.filter.borrow() {
                    *s.filter.borrow_mut() = filter;
                    if s.visible.get() {
                        s.apply_filter();
                    }
                }
            }
        });
        let w = Rc::downgrade(self);
        self.search.connect_stop_search(move |_| {
            if let Some(s) = w.upgrade() {
                s.finish_filter();
            }
        });
        let w = Rc::downgrade(self);
        reset.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.reset(None);
            }
        });
        let w = Rc::downgrade(self);
        self.toggle.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                let all_closed = s.groups.borrow().iter().all(|g| s.collapsed.borrow().contains(&g.id));
                let ids: Vec<String> = s.groups.borrow().iter().map(|g| g.id.clone()).collect();
                for id in ids {
                    s.set_open(&id, all_closed);
                }
            }
        });
        // Overskriften klistrer seg øverst når tabellens egen har rullet bort.
        let w = Rc::downgrade(self);
        self.scroller.vadjustment().connect_value_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.place_sticky();
            }
        });
    }

    /// Sparklinjene for siste minutt fylles fra historikken første gang
    /// siden vises, så de ikke står nesten tomme det første minuttet.
    fn prefill(self: &Rc<Self>) {
        if self.prefilled.get() || !self.app.client.is_connected() {
            return;
        }
        let keys: Vec<String> = self.groups.borrow().iter().flat_map(|g| g.rows.iter()).filter(|r| !r.def.na && self.app.recent(&r.def.key).len() < 50).map(|r| r.def.key.clone()).collect();
        if keys.is_empty() {
            return;
        }
        self.prefilled.set(true);
        for chunk in keys.chunks(rigg_core::history::MAX_KEYS) {
            let w = Rc::downgrade(self);
            self.app.client.request("get_history", json!({"window": 60, "keys": chunk, "points": 60}), move |res| {
                let (Some(s), Ok(v)) = (w.upgrade(), res) else { return };
                let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
                if let Some(series) = v["series"].as_object() {
                    for (k, sv) in series {
                        let avg: Vec<Option<f64>> = serde_json::from_value(sv["avg"].clone()).unwrap_or_default();
                        // Bøtta midt i sekundet den dekker, som grafene.
                        let pts: Vec<(i64, f64)> = t.iter().zip(avg).filter_map(|(t, v)| v.map(|v| (*t + 500, v))).collect();
                        s.app.prefill(k, pts);
                    }
                }
                s.paint();
            });
        }
    }

    fn place_sticky(&self) {
        let v = self.scroller.vadjustment().value();
        let Some(p) = self.header.compute_point(&self.content, &gtk::graphene::Point::new(0.0, 0.0)) else { return };
        let show = v > p.y() as f64 + 1.0 && self.table.is_visible();
        if show
            && let Some(b) = self.table.compute_bounds(&self.root)
        {
            let x = b.x().round() as i32 + 1;
            self.sticky.set_margin_start(x);
            self.sticky.set_margin_end((self.root.width() - (b.x() + b.width()).round() as i32 + 1).max(0));
        }
        if self.sticky.is_visible() != show {
            self.sticky.set_visible(show);
        }
    }

    // ------------------------------------------------------------ Bygging

    fn load_system(self: &Rc<Self>) {
        if self.system_loading.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        self.app.client.request("get_system", json!({}), move |res| {
            let Some(s) = w.upgrade() else { return };
            s.system_loading.set(false);
            if let Ok(system) = res {
                *s.system.borrow_mut() = system;
                s.prefilled.set(false);
                if s.visible.get() {
                    s.rebuild_if_needed();
                    s.paint();
                    s.prefill();
                }
            }
        });
    }

    /// Henter `get_system` på nytt når telemetrien melder et nytt sett aktive CPU-ID-er.
    fn sync_cpu_topology(self: &Rc<Self>) {
        let Some(tel) = self.app.telemetry() else { return };
        let request = self.cpu_topology.borrow_mut().should_request(
            &self.system.borrow(),
            &tel.sys.cpu,
            self.system_loading.get(),
        );
        if request {
            self.load_system();
        }
    }

    fn rebuild_if_needed(self: &Rc<Self>) {
        let sig = signature(&self.app, &self.system.borrow());
        if sig.is_empty() || *self.sig.borrow() == sig {
            return;
        }
        *self.sig.borrow_mut() = sig;
        self.rebuild();
    }

    fn rebuild(self: &Rc<Self>) {
        let open_detail = self.detail.borrow().as_ref().map(|d| d.key.clone());
        self.close_detail(false);
        crate::widgets::clear(&self.groups_box);
        let sparks = self.app.settings().sensor_sparks;
        let mut groups = Vec::new();
        for (gi, (id, title, note, defs)) in row_defs(&self.app, &self.system.borrow()).into_iter().enumerate() {
            let root = vbox(0, &["sn-group"]);
            if gi == 0 {
                root.add_css_class("first");
            }
            let head = hbox(8, &["grp"]);
            let btn = gtk::Button::new();
            btn.add_css_class("grp-btn");
            btn.set_hexpand(true);
            let bx = hbox(8, &[]);
            let chev = Icon::new("chevron-down", 14);
            chev.add_css_class("chev");
            bx.append(&chev);
            bx.append(&label(&title, &["group-title"]));
            let note_l = label(&note, &["note"]);
            note_l.set_ellipsize(pango::EllipsizeMode::End);
            bx.append(&note_l);
            btn.set_child(Some(&bx));
            btn.update_property(&[gtk::accessible::Property::Label(&title)]);
            head.append(&btn);
            let state = hbox(4, &["state"]);
            head.append(&state);
            root.append(&head);
            let rows_box = vbox(0, &["sn-rows"]);
            let mut rows = Vec::new();
            let mut threads = None;
            let mut first_focus = true;
            let thread_count = defs.iter().filter(|d| d.thread.is_some()).count();
            for d in defs {
                if d.thread.is_some() && threads.is_none() {
                    let n = thread_count;
                    let sb = gtk::Button::new();
                    sb.add_css_class("sub-btn");
                    let sbx = hbox(8, &[]);
                    let ic = Icon::new("chevron-down", 14);
                    sbx.append(&ic);
                    sbx.append(&label(&format!("Per tråd ({n})"), &[]));
                    sb.set_child(Some(&sbx));
                    rows_box.append(&sb);
                    let tb = vbox(0, &["sn-threads"]);
                    rows_box.append(&tb);
                    let w = Rc::downgrade(self);
                    sb.connect_clicked(move |_| {
                        if let Some(s) = w.upgrade() {
                            let v = !s.threads_open.get();
                            s.set_threads(v);
                        }
                    });
                    threads = Some((sb, ic, tb));
                }
                let r = self.make_row(d, sparks);
                r.name.set_focusable(first_focus);
                r.pin.set_focusable(false);
                first_focus = false;
                match (&threads, r.def.thread) {
                    (Some((_, _, tb)), Some(_)) => tb.append(&r.root),
                    _ => rows_box.append(&r.root),
                }
                rows.push(r);
            }
            root.append(&rows_box);
            self.groups_box.append(&root);
            let g = Rc::new(Group { id: id.clone(), root, btn: btn.clone(), chev, state, state_sig: RefCell::new(String::new()), rows, threads });
            let w = Rc::downgrade(self);
            let gid = id.clone();
            btn.connect_clicked(move |_| {
                if let Some(s) = w.upgrade() {
                    let open = s.collapsed.borrow().contains(&gid);
                    s.set_open(&gid, open);
                }
            });
            groups.push(g);
        }
        *self.groups.borrow_mut() = groups;
        for g in self.groups.borrow().iter() {
            self.apply_open(g);
        }
        self.apply_threads();
        self.apply_filter();
        self.render_pins();
        self.render_toggle();
        if let Some(k) = open_detail
            && self.row(&k).is_some()
        {
            self.open_detail(&k);
        }
        self.paint();
    }

    fn make_row(self: &Rc<Self>, d: RowDef, sparks: bool) -> Rc<Row> {
        let name_box = hbox(2, &["name"]);
        let pin = gtk::Button::new();
        pin.add_css_class("pin-btn");
        pin.add_css_class("flat");
        let pin_icon = Icon::new("pin", 14);
        pin.set_child(Some(&pin_icon));
        name_box.append(&pin);
        let name = gtk::Button::new();
        name.add_css_class("sn-name");
        let nl = label(&d.label, &["n"]);
        nl.set_ellipsize(pango::EllipsizeMode::End);
        name.set_child(Some(&nl));
        name.set_hexpand(true);
        name.set_halign(gtk::Align::Start);
        let tip_text = [d.tip.clone(), d.raw.as_ref().map(|r| format!("Kilde: {r}"))].into_iter().flatten().collect::<Vec<_>>().join("\n");
        let full = if tip_text.is_empty() { format!("{}. Klikk for graf.", d.label) } else { format!("{}\n{tip_text}", d.label) };
        name.set_tooltip_text(Some(&full));
        name.update_property(&[gtk::accessible::Property::Label(&d.label), gtk::accessible::Property::Description(&tip_text)]);
        name_box.append(&name);
        let extra = label(d.extra.as_deref().unwrap_or(""), &["x"]);
        extra.set_visible(d.extra.is_some() || d.thread.is_some());
        name_box.append(&extra);
        let num = |c: &str| {
            let l = label("–", &["sn-td", c, "num"]);
            l.set_xalign(1.0);
            l.set_ellipsize(pango::EllipsizeMode::End);
            l
        };
        let now = num("now");
        let min = num("min");
        let max = num("max");
        let avg = num("avg");
        let spark = Spark::new(d.serie, 40);
        spark.set_visible(sparks);
        spark.set_valign(gtk::Align::Center);
        let spark_cell = hbox(0, &["sn-spark"]);
        spark.set_hexpand(true);
        spark_cell.append(&spark);
        let cells: [gtk::Widget; 6] = [name_box.upcast(), now.clone().upcast(), min.clone().upcast(), max.clone().upcast(), avg.clone().upcast(), spark_cell.upcast()];
        let root = TRow::new(&cells, sparks);
        root.add_css_class("sn-row");
        if d.thread.is_some() {
            root.add_css_class("thread");
        }
        if d.na {
            root.add_css_class("na");
        }
        let r = Rc::new(Row { def: d, root, pin, pin_icon, name, extra, now, min, max, avg, spark, level: Cell::new(""), hidden: Cell::new(false) });
        let (w, k) = (Rc::downgrade(self), r.def.key.clone());
        r.name.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.toggle_detail(&k);
            }
        });
        let (w, k) = (Rc::downgrade(self), r.def.key.clone());
        r.pin.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.toggle_pin(&k);
            }
        });
        // Høyreklikk: samme valg som nålen, og mer.
        let rc = gtk::GestureClick::new();
        rc.set_button(3);
        let (w, k) = (Rc::downgrade(self), r.def.key.clone());
        rc.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if let Some(s) = w.upgrade() {
                s.row_menu(&k, x, y);
            }
        });
        r.root.add_controller(rc);
        // Fokus på nålen eller navnet gjør den til gruppens Tab-stopp.
        for b in [&r.pin, &r.name] {
            let fc = gtk::EventControllerFocus::new();
            let (w, k) = (Rc::downgrade(self), r.def.key.clone());
            let which = b == &r.pin;
            fc.connect_enter(move |_| {
                if let Some(s) = w.upgrade() {
                    s.roving(&k, which);
                }
            });
            b.add_controller(fc);
        }
        r
    }

    fn row(&self, key: &str) -> Option<Rc<Row>> {
        self.groups.borrow().iter().flat_map(|g| g.rows.iter()).find(|r| r.def.key == key).cloned()
    }

    fn group_of(&self, key: &str) -> Option<Rc<Group>> {
        self.groups.borrow().iter().find(|g| g.rows.iter().any(|r| r.def.key == key)).cloned()
    }

    // ------------------------------------------------------------ Sammenlegging

    fn apply_open(&self, g: &Group) {
        let open = !self.filter.borrow().is_empty() || !self.collapsed.borrow().contains(&g.id);
        if let Some(rows) = g.root.last_child() {
            rows.set_visible(open);
        }
        g.chev.set_icon(if open { "chevron-down" } else { "chevron-right" });
        g.btn.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
        class(&g.root, "closed", !open);
    }

    fn set_open(self: &Rc<Self>, id: &str, open: bool) {
        if open {
            self.collapsed.borrow_mut().remove(id);
        } else {
            self.collapsed.borrow_mut().insert(id.to_string());
        }
        if !open && self.detail.borrow().as_ref().is_some_and(|d| self.group_of(&d.key).is_some_and(|g| g.id == id)) {
            self.close_detail(false);
        }
        if let Some(g) = self.groups.borrow().iter().find(|g| g.id == id) {
            self.apply_open(g);
            if open && let Some(rows) = g.root.last_child() {
                crate::motion::fade_in(&self.app, &rows, 3, 0.0);
            }
        }
        self.render_toggle();
        self.paint();
    }

    fn set_threads(self: &Rc<Self>, open: bool) {
        self.threads_open.set(open);
        self.apply_threads();
        if open && let Some((_, _, tb)) = self.groups.borrow().iter().find_map(|g| g.threads.clone()) {
            crate::motion::fade_in(&self.app, &tb, 3, 0.0);
        }
        self.paint();
    }

    fn apply_threads(&self) {
        let open = self.threads_open.get() || !self.filter.borrow().is_empty();
        for g in self.groups.borrow().iter() {
            if let Some((b, ic, tb)) = &g.threads {
                tb.set_visible(open);
                ic.set_icon(if open { "chevron-down" } else { "chevron-right" });
                b.update_state(&[gtk::accessible::State::Expanded(Some(open))]);
            }
        }
        if !open && self.detail.borrow().as_ref().is_some_and(|d| d.key.starts_with("cpu/thread/")) {
            self.close_detail(false);
        }
    }

    fn render_toggle(&self) {
        let all_closed = self.groups.borrow().iter().all(|g| self.collapsed.borrow().contains(&g.id));
        self.toggle_icon.set_icon(if all_closed { "chevrons-up-down" } else { "chevrons-down-up" });
        let t = if all_closed { "Fold ut alle" } else { "Fold sammen alle" };
        tip(&self.toggle, t, None);
        self.toggle.update_property(&[gtk::accessible::Property::Label(t)]);
    }

    // ------------------------------------------------------------ Filter

    /// Tøm tilstanden før set_text kan utløse synkrone søkesignaler. Også
    /// råteksten må tømmes når Esc kommer før den forsinkede søkehendelsen.
    fn clear_filter(&self) {
        self.filter.borrow_mut().clear();
        if !self.search.text().is_empty() {
            self.search.set_text("");
        }
    }

    fn release_filter_focus(&self) {
        if self.search.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN)
            && let Some(win) = self.app.window()
        {
            gtk::prelude::GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
        }
    }

    fn finish_filter(self: &Rc<Self>) {
        self.clear_filter();
        self.release_filter_focus();
        if self.visible.get() {
            self.apply_filter();
        }
    }

    fn matches(&self, r: &Row) -> bool {
        let f = self.filter.borrow();
        f.is_empty() || format!("{} {} {}", r.def.label, r.def.raw.as_deref().unwrap_or(""), r.def.key).to_lowercase().contains(f.as_str())
    }

    fn apply_filter(self: &Rc<Self>) {
        let filtering = !self.filter.borrow().is_empty();
        let mut any = false;
        for g in self.groups.borrow().iter() {
            let mut n = 0;
            for r in &g.rows {
                let ok = self.matches(r);
                r.hidden.set(!ok);
                r.root.set_visible(ok);
                if ok {
                    n += 1;
                }
            }
            if let Some((b, _, _)) = &g.threads {
                b.set_visible(!filtering);
            }
            g.root.set_visible(!filtering || n > 0);
            any |= n > 0;
            self.apply_open(g);
        }
        self.apply_threads();
        self.table.set_visible(!filtering || any);
        // «Ikke tilgjengelig» filtreres også.
        let mut notes = 0;
        let mut c = self.notes_box.first_child();
        while let Some(n) = c {
            let text = n.tooltip_text().map(|t| t.to_lowercase()).unwrap_or_default();
            let ok = !filtering || text.contains(self.filter.borrow().as_str());
            n.set_visible(ok);
            if ok {
                notes += 1;
            }
            c = n.next_sibling();
        }
        self.notes_panel.root.set_visible(self.notes_box.first_child().is_some() && notes > 0);
        let none = filtering && !any && notes == 0;
        self.empty.set_visible(none);
        if none {
            // Påskeegg 7: noen få ord får sitt eget svar.
            let q = crate::easter::filter_quip(&self.app, &self.filter.borrow()).unwrap_or("Ingen sensorer passer filteret.");
            set_text(&self.empty_text, q);
        }
        if let Some(d) = self.detail.borrow().as_ref().map(|d| d.key.clone())
            && self.row(&d).is_some_and(|r| r.hidden.get())
        {
            self.close_detail(false);
        }
        self.reset_roving();
        self.paint();
    }

    // ------------------------------------------------------------ Tall

    /// (min, maks, snitt) for en nøkkel: fra tjenesten, eller fra trykket på
    /// ↺ mens Angre-fristen går.
    fn stat(&self, key: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
        if let Some(p) = self.pending.borrow().as_ref()
            && p.keys.as_ref().is_none_or(|k| k.contains(key))
        {
            return match p.acc.get(key) {
                Some((lo, hi, sum, n)) => (Some(*lo), Some(*hi), (*n > 0).then(|| sum / *n as f64)),
                None => (None, None, None),
            };
        }
        let d = self.app.data.borrow();
        let s = &d.stats["stats"][key];
        (s["min"].as_f64(), s["max"].as_f64(), s["avg"].as_f64())
    }

    fn paint(&self) {
        if !self.visible.get() {
            return;
        }
        let app = &self.app;
        let t = app.settings().thresholds.clone();
        let now_ms = app.telemetry().map(|t| t.t).unwrap_or(0);
        let sparks = app.settings().sensor_sparks;
        // Bare radene som synes i rullefeltet, oppdateres; resten tas når de
        // rulles fram (vadjustment). Med cairo kostet tabellen 19 ms per
        // sekund, mest sparklinjene i rader ingen så (runde 3C).
        let view_h = self.scroller.height() as f32;
        let in_view = |r: &Row| view_h <= 0.0 || r.root.compute_bounds(&self.scroller).is_none_or(|b| b.y() + b.height() >= -40.0 && b.y() <= view_h + 40.0);
        for g in self.groups.borrow().iter() {
            if self.collapsed.borrow().contains(&g.id) && self.filter.borrow().is_empty() {
                continue;
            }
            let threads_hidden = !self.threads_open.get() && self.filter.borrow().is_empty();
            for r in &g.rows {
                if r.hidden.get() || (r.def.thread.is_some() && threads_hidden) || !in_view(r) {
                    continue;
                }
                let d = &r.def;
                let v = app.value(&d.key);
                let (mn, mx, av) = self.stat(&d.key);
                if d.na {
                    set_text(&r.now, "Ikke støttet");
                    r.now.set_tooltip_text(Some("Kan ikke leses på dette kortet. Driveren svarer at verdien ikke støttes (NOT_SUPPORTED)."));
                    for l in [&r.min, &r.max, &r.avg] {
                        set_text(l, "");
                    }
                    continue;
                }
                let unit = if d.fmt == Fmt::Rate {
                    let mut units = self.units.borrow_mut();
                    let top = mx.unwrap_or(0.0).max(v.unwrap_or(0.0));
                    let u = F::rate_unit(top);
                    let cur = units.entry(d.key.clone()).or_insert_with(|| u.clone());
                    if u.div > cur.div {
                        *cur = u;
                    }
                    Some(cur.clone())
                } else {
                    None
                };
                let f = |x: Option<f64>| fmt_value(d.fmt, x, unit.as_ref());
                set_num(&r.now, &f(v));
                set_num(&r.min, &f(mn));
                set_num(&r.max, &f(mx));
                if d.discrete {
                    set_text(&r.avg, F::DASH);
                    if r.avg.tooltip_text().is_none() {
                        r.avg.set_tooltip_text(Some("Snitt gir ikke mening for en generasjon."));
                    }
                } else {
                    set_num(&r.avg, &f(av));
                }
                let lvl = match d.lvl {
                    Lvl::GpuTemp => math::level(v, Some(t.gpu_temp_warn), Some(t.gpu_temp_crit)),
                    Lvl::CpuTemp => math::level(v, Some(t.cpu_temp_warn), Some(t.cpu_temp_crit)),
                    Lvl::Load => math::level(v, Some(t.load_warn), None),
                    Lvl::None => "",
                };
                if r.level.get() != lvl {
                    r.level.set(lvl);
                    for l in ["warn", "danger"] {
                        class(&r.now, l, l == lvl);
                    }
                    let lt = math::level_text(lvl);
                    r.now.set_tooltip_text(if lt.is_empty() { None } else { Some(lt) });
                }
                if let Some(i) = d.thread {
                    let mhz = app.value(&format!("cpu/thread/{i}/mhz"));
                    set_text(&r.extra, &if mhz.is_some() { F::mhz(mhz) } else { String::new() });
                }
                if sparks {
                    let pts = app.recent(&d.key);
                    let pts: Vec<(i64, f64)> = if d.is_temp() { pts.into_iter().map(|(t, v)| (t, F::temp_out(Some(v)).unwrap_or(v))).collect() } else { pts };
                    let span = if d.is_temp() && F::fahrenheit() { d.min_span * 1.8 } else { d.min_span };
                    r.spark.set_points(&pts, now_ms, SparkOpts { span_ms: 60_000, fixed: d.fixed, min_span: span });
                    r.spark.set_level(lvl);
                }
            }
            self.render_gpu_state(g);
        }
        // Min/maks siden …
        let since = match self.pending.borrow().as_ref() {
            Some(p) if p.keys.is_none() => Some(p.since),
            _ => self.app.data.borrow().stats["since"].as_i64(),
        };
        let avg_since = self.app.data.borrow().stats["avgSince"].as_i64();
        if let Some(s) = since {
            let text = F::since(crate::app::local_time(s), crate::app::local_now());
            set_text(&self.since, &format!("Min/maks siden {text}"));
            let keep = app.settings().sensor_keep_min_max;
            let avg = avg_since.map(|a| F::since(crate::app::local_time(a), crate::app::local_now())).unwrap_or_else(|| text.clone());
            tip(&self.reset, &format!("Nullstill min, maks og snitt (regnet fra {text})"), Some("Ctrl+R"));
            let tipt = if keep {
                format!("Min og maks huskes mellom økter, også når appen er lukket. Snitt gjelder siden {avg}.")
            } else {
                format!("Min og maks gjelder siden tjenesten startet ({text}). Snitt gjelder siden {avg}.")
            };
            if self.since.tooltip_text().as_deref() != Some(tipt.as_str()) {
                self.since.set_tooltip_text(Some(&tipt));
            }
        }
    }

    fn render_gpu_state(&self, g: &Group) {
        let Some(id) = g.id.strip_prefix("gpu:") else { return };
        let Some(tel) = self.app.telemetry() else { return };
        let Some(s) = tel.gpus.get(id) else {
            // Kortet står i lista, men svarer ikke.
            if *g.state_sig.borrow() != "borte" {
                *g.state_sig.borrow_mut() = "borte".into();
                crate::widgets::clear(&g.state);
                let info = self.app.data.borrow().gpus.iter().find(|x| x.id == id).cloned();
                let t = crate::widgets::rows::tag("Svarer ikke", Some("circle-alert"), "warn");
                t.set_tooltip_text(Some(&match info.and_then(|i| i.detail) {
                    Some(d) => format!("Skjermkortet svarer ikke ({d}). Tjenesten prøver å finne det igjen hvert 10. s."),
                    None => "Skjermkortet svarer ikke. Tjenesten prøver å finne det igjen hvert 10. s.".into(),
                }));
                g.state.append(&t);
            }
            return;
        };
        let (th, full) = math::throttle_short(s.throttle, 1);
        let sig = format!("{:?}{th}{}", s.pstate, s.throttle_inferred);
        if *g.state_sig.borrow() == sig {
            return;
        }
        *g.state_sig.borrow_mut() = sig;
        crate::widgets::clear(&g.state);
        if let Some(p) = &s.pstate {
            let t = crate::widgets::rows::tag(p, None, "");
            t.set_tooltip_text(Some("P-tilstand. P0 er full ytelse, P8 er dyp tomgang."));
            g.state.append(&t);
        }
        if s.throttle.is_some() {
            let text = if th == "Ingen" { "Ingen begrensning".to_string() } else { th };
            let t = crate::widgets::rows::tag(&text, None, if math::throttle_serious(s.throttle) { "warn" } else { "" });
            let more = if full.is_empty() { String::new() } else { format!(": {full}") };
            let inferred = if s.throttle_inferred { " I P8 leses den ikke (kallet koster 25 ms)." } else { "" };
            t.set_tooltip_text(Some(&format!("Klokkebegrensning akkurat nå{more}.{inferred}")));
            g.state.append(&t);
        }
    }

    // ------------------------------------------------------------ Feste

    fn render_pins(&self) {
        let items = self.app.settings().status_items.clone();
        let disks = self.app.sys().map(|s| s.disks).unwrap_or_default();
        for g in self.groups.borrow().iter() {
            for r in &g.rows {
                let on = items.contains(&pin_id(&self.app, &r.def.key)) || items.contains(&r.def.key);
                class(&r.root, "pinned", on);
                r.pin_icon.set_icon("pin");
                let short = disk_name(&r.def.key, &disks).map(|d| d.0).or_else(|| short_name(&r.def.key)).unwrap_or_else(|| r.def.label.clone());
                let t = if on { format!("Løsne «{short}» fra statusblokken") } else { format!("Fest «{short}» i statusblokken") };
                tip(&r.pin, &t, None);
                r.pin.update_property(&[gtk::accessible::Property::Label(&t)]);
                r.pin.update_state(&[gtk::accessible::State::Pressed(if on { gtk::AccessibleTristate::True } else { gtk::AccessibleTristate::False })]);
            }
        }
    }

    fn toggle_pin(&self, key: &str) {
        let id = pin_id(&self.app, key);
        let cur = self.app.settings().status_items.clone();
        if cur.contains(&id) || cur.iter().any(|x| x == key) {
            let next: Vec<String> = cur.into_iter().filter(|x| *x != id && x != key).collect();
            self.app.set(json!({"statusItems": next}));
            return;
        }
        let max = rigg_core::settings::STATUS_MAX;
        if cur.len() >= max {
            let a = self.app.clone();
            self.app.toast(Toast::new(&format!("Statusblokken er full ({max} av {max})")).body("Løsne en verdi først, eller bytt ut en under Innstillinger › Sidefelt.").action("Åpne", move || a.open_settings("sidefelt")));
            return;
        }
        let mut next = cur;
        next.push(id);
        self.app.set(json!({"statusItems": next}));
    }

    fn row_menu(self: &Rc<Self>, key: &str, x: f64, y: f64) {
        let Some(r) = self.row(key) else { return };
        let items = self.app.settings().status_items.clone();
        let pinned = items.contains(&pin_id(&self.app, key)) || items.iter().any(|i| i == key);
        let m = Menu::new(&r.root);
        m.popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        m.heading(&r.def.label);
        let (w, k) = (Rc::downgrade(self), key.to_string());
        m.item(Some(if pinned { "pin-off" } else { "pin" }), if pinned { "Løsne fra statusblokken" } else { "Fest i statusblokken" }, None, move || {
            if let Some(s) = w.upgrade() {
                s.toggle_pin(&k);
            }
        });
        // Loggen (tjeneste.json › log): alle sensorer, eller bare de valgte.
        let log = self.app.data.borrow().service["log"].clone();
        let all = log["all"].as_bool().unwrap_or(true);
        let in_log = log["keys"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(key)));
        if all {
            let a = self.app.clone();
            m.item(Some("circle-check"), "Logges · alle sensorer er valgt", None, move || a.open_settings("logging"));
        } else {
            let (a, k) = (self.app.clone(), key.to_string());
            m.item(Some(if in_log { "minus" } else { "plus" }), if in_log { "Ta ut av loggen" } else { "Ta med i loggen" }, None, move || {
                let cur: Vec<String> = a.data.borrow().service["log"]["keys"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                let next: Vec<String> = if cur.contains(&k) { cur.into_iter().filter(|x| *x != k).collect() } else { cur.into_iter().chain(std::iter::once(k.clone())).collect() };
                a.set_service(json!({"log": {"keys": next}}));
            });
        }
        let (w, k) = (Rc::downgrade(self), key.to_string());
        m.item(Some("rotate-ccw"), "Nullstill min og maks", None, move || {
            if let Some(s) = w.upgrade() {
                s.reset(Some(vec![k.clone()]));
            }
        });
        m.sep();
        let a = self.app.clone();
        m.item(Some("arrow-down-up"), "Rekkefølge i statusblokken …", None, move || a.open_settings("sidefelt"));
        self.menu_open.set(true);
        let w = Rc::downgrade(self);
        m.popover.connect_closed(move |_| {
            if let Some(s) = w.upgrade() {
                s.menu_open.set(false);
            }
        });
        m.show();
    }

    // ------------------------------------------------------------ Nullstilling

    fn reset(self: &Rc<Self>, keys: Option<Vec<String>>) {
        // En nullstilling som venter, sendes nå (ny nullstilling oppå den forrige).
        self.flush_pending();
        let old_since = self.app.data.borrow().stats["since"].as_i64();
        let now = rigg_core::now_ms();
        let mut acc = HashMap::new();
        let set: Option<HashSet<String>> = keys.as_ref().map(|k| k.iter().cloned().collect());
        for (k, v) in self.app.data.borrow().values.iter() {
            if set.as_ref().is_none_or(|s| s.contains(k)) {
                acc.insert(k.clone(), (*v, *v, *v, 1u64));
            }
        }
        let w = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(std::time::Duration::from_millis(5000), move || {
            if let Some(s) = w.upgrade() {
                if let Some(p) = s.pending.borrow_mut().as_mut() {
                    p.timer = None;
                }
                s.flush_pending();
            }
        });
        *self.pending.borrow_mut() = Some(Pending { keys: set, acc, since: now, timer: Some(timer) });
        self.paint();
        let (title, body) = match &keys {
            None => (
                "Min, maks og snitt er nullstilt".to_string(),
                old_since.map(|s| format!("Tallene var regnet fra {}.", F::since(crate::app::local_time(s), crate::app::local_now()))).unwrap_or_default(),
            ),
            Some(k) => {
                let n = k.first().and_then(|k| self.row(k)).map(|r| r.def.label.clone()).unwrap_or_default();
                (format!("Min og maks for «{n}» er nullstilt"), String::new())
            }
        };
        let w = Rc::downgrade(self);
        let mut t = Toast::new(&title).action("Angre", move || {
            if let Some(s) = w.upgrade() {
                if let Some(p) = s.pending.borrow_mut().take()
                    && let Some(id) = p.timer
                {
                    id.remove();
                }
                s.paint();
            }
        });
        if !body.is_empty() {
            t = t.body(&body);
        }
        self.app.toast(t.ms(5000));
    }

    fn flush_pending(self: &Rc<Self>) {
        let Some(p) = self.pending.borrow_mut().take() else { return };
        if let Some(id) = p.timer {
            id.remove();
        }
        let args = match &p.keys {
            Some(k) => json!({"keys": k.iter().collect::<Vec<_>>()}),
            None => json!({}),
        };
        let a = self.app.clone();
        self.app.client.request("reset_stats", args, move |res| match res {
            Ok(v) => {
                a.data.borrow_mut().stats = v;
                a.on_stats.emit(&());
            }
            Err(e) => a.toast(Toast::new("Kunne ikke nullstille").body(&e.message).error()),
        });
    }

    /// Mens Angre-fristen går, samles min, maks og snitt fra trykket.
    fn track_pending(&self) {
        let mut p = self.pending.borrow_mut();
        let Some(p) = p.as_mut() else { return };
        for (k, v) in self.app.data.borrow().values.iter() {
            if p.keys.as_ref().is_some_and(|s| !s.contains(k)) {
                continue;
            }
            let e = p.acc.entry(k.clone()).or_insert((*v, *v, 0.0, 0));
            e.0 = e.0.min(*v);
            e.1 = e.1.max(*v);
            e.2 += v;
            e.3 += 1;
        }
    }

    // ------------------------------------------------------------ Graf under raden

    fn toggle_detail(self: &Rc<Self>, key: &str) {
        if self.detail.borrow().as_ref().is_some_and(|d| d.key == key) {
            self.close_detail(true);
        } else {
            self.open_detail(key);
        }
    }

    fn open_detail(self: &Rc<Self>, key: &str) {
        self.close_detail(false);
        let Some(r) = self.row(key) else { return };
        if r.def.na {
            return;
        }
        let d = r.def.clone();
        let app = &self.app;
        let win = app.settings().windows.sensorer as i64;
        let tools = hbox(4, &["sn-detail-tools"]);
        let dd = {
            let a = app.clone();
            chart::window_dropdown(win, "Tidsvindu for grafen", move |sec| {
                a.set(json!({"windows": {"sensorer": sec}}));
            })
        };
        tools.append(&dd);
        let close = gtk::Button::new();
        close.add_css_class("flat");
        close.add_css_class("sm");
        close.add_css_class("icon-only");
        close.set_child(Some(&Icon::new("x", 14)));
        tip(&close, "Lukk grafen", Some("Esc"));
        close.update_property(&[gtk::accessible::Property::Label("Lukk grafen")]);
        tools.append(&close);
        let color: &'static str = match d.serie {
            "temp" => "--c-temp",
            "memctl" => "--c-memctl",
            "power" => "--c-power",
            "clock" => "--c-clock",
            "mem" => "--c-mem",
            "limit" => "--c-limit",
            _ => "--c-load",
        };
        // Nettraten i radens faste enhet.
        let unit = (d.fmt == Fmt::Rate).then(|| self.units.borrow().get(&d.key).cloned().unwrap_or_else(|| F::rate_unit(self.stat(&d.key).1.unwrap_or(1e6))));
        let div = if d.fmt == Fmt::DiskRate { 1e6 } else { unit.as_ref().map(|u| u.div).unwrap_or(1.0) };
        let fmt = d.fmt;
        let u2 = unit.clone();
        let mut s = SeriesDef::new(&d.key, &d.label, color).fill();
        if d.is_temp() {
            s = s.map(|v| F::temp_out(Some(v)).unwrap_or(v)).fmt(move |x| fmt_value(Fmt::Temp, Some(F::temp_in(x)), None));
        } else if fmt == Fmt::Rate {
            s = s.map(move |v| v / div).fmt(move |x| fmt_value(Fmt::Rate, Some(x * div), u2.as_ref()));
        } else if fmt == Fmt::DiskRate {
            s = s.map(|v| v / 1e6).fmt(|x| super::disker::mbps(Some(x * 1e6)));
        } else {
            s = s.fmt(move |x| fmt_value(fmt, Some(x), None));
        }
        if d.discrete {
            s.band = false;
        }
        // Øvre grense fra dataene i vinduet (også lange vinduer med høyere topper).
        let fixed = d.fixed;
        let temp_row = d.is_temp();
        let wself = Rc::downgrade(self);
        let key2 = d.key.clone();
        let mut opts = ChartOpts::new(app, move || {
            if let Some(f) = fixed {
                return f;
            }
            let hi = wself.upgrade().and_then(|s| s.detail.borrow().as_ref().map(|d| d.panel.view.clone())).map(|v| {
                let data = v.data.borrow();
                data.s.first().map(|c| c.hi.iter().chain(c.avg.iter()).flatten().fold(0.0f64, |a, b| a.max(*b))).unwrap_or(0.0)
            });
            let st = wself.upgrade().and_then(|s| s.stat(&key2).1).unwrap_or(0.0);
            let top = hi.unwrap_or(0.0).max(st);
            let conv = if temp_row { F::temp_out(Some(top)).unwrap_or(top) } else { top / div };
            (0.0, (conv * 1.3).max(1.0))
        });
        let fixed_scale = d.fixed.is_some();
        opts.auto = Rc::new(move || !fixed_scale);
        let ms = d.min_span;
        let is_temp = d.is_temp();
        opts.min_span = Rc::new(move || if is_temp && F::fahrenheit() { ms * 1.8 } else { ms });
        opts.decimals = match fmt {
            Fmt::Temp | Fmt::Pct | Fmt::W => 1,
            Fmt::Rate => unit.as_ref().map(|u| u.d).unwrap_or(1),
            Fmt::DiskRate => 1,
            Fmt::Gib => 2,
            Fmt::Volt | Fmt::Amp => 3,
            _ => 0,
        };
        let panel = ChartPanel::new(app, PanelOpts { title: d.label.clone(), series: vec![s], window: win, chart: opts, sync: None, stats: true, now: false, extra: Some(tools.upcast()), flat: true });
        panel.chart.update_property(&[gtk::accessible::Property::Label(&format!("Graf: {}{}. Mellomrom setter på pause.", d.label, d.raw.as_ref().map(|r| format!(", kilde {r}")).unwrap_or_default()))]);
        let root = vbox(0, &["sn-detail"]);
        root.append(&panel.root);
        if let Some(parent) = r.root.parent().and_downcast::<gtk::Box>() {
            parent.insert_child_after(&root, Some(&r.root));
        }
        self.set.add(&panel);
        let w = Rc::downgrade(self);
        close.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.close_detail(true);
            }
        });
        r.root.add_css_class("open");
        r.name.update_state(&[gtk::accessible::State::Expanded(Some(true))]);
        crate::motion::fade_in(app, &root, 3, 0.0);
        *self.detail.borrow_mut() = Some(Detail { key: key.to_string(), root, panel });
    }

    fn close_detail(&self, focus: bool) {
        let Some(d) = self.detail.borrow_mut().take() else { return };
        self.set.remove(&d.panel);
        if let Some(p) = d.root.parent().and_downcast::<gtk::Box>() {
            p.remove(&d.root);
        }
        if let Some(r) = self.row(&d.key) {
            r.root.remove_css_class("open");
            r.name.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
            if focus {
                r.name.set_focusable(true);
                r.name.grab_focus();
            }
        }
    }

    // ------------------------------------------------------------ Tastatur

    /// Synlige rader i gruppen, i rekkefølge.
    fn visible_rows(&self, g: &Group) -> Vec<Rc<Row>> {
        let threads = self.threads_open.get() || !self.filter.borrow().is_empty();
        g.rows.iter().filter(|r| !r.hidden.get() && (r.def.thread.is_none() || threads)).cloned().collect()
    }

    /// Gjør nålen eller navnet i raden til gruppens eneste Tab-stopp.
    fn roving(&self, key: &str, pin: bool) {
        let Some(g) = self.group_of(key) else { return };
        for r in &g.rows {
            let me = r.def.key == key;
            r.pin.set_focusable(me && pin);
            r.name.set_focusable(me && !pin);
        }
    }

    fn reset_roving(&self) {
        for g in self.groups.borrow().iter() {
            let vis = self.visible_rows(g);
            let has = vis.iter().any(|r| r.pin.is_focusable() || r.name.is_focusable());
            if !has && let Some(r) = vis.first() {
                r.name.set_focusable(true);
            }
        }
    }

    fn on_key(self: &Rc<Self>, k: &Key) -> bool {
        if k.ctrl && !k.alt && matches!(k.key, K::f | K::F) {
            self.search.grab_focus();
            self.search.select_region(0, -1);
            return true;
        }
        if k.ctrl && !k.alt && matches!(k.key, K::r | K::R) {
            self.reset(None);
            return true;
        }
        let win = self.app.window();
        let focus = win.as_ref().and_then(gtk::prelude::GtkWindowExt::focus);
        if k.key == K::Escape {
            if self.search.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN) || !self.search.text().is_empty() || !self.filter.borrow().is_empty() {
                self.finish_filter();
                return true;
            }
            if self.detail.borrow().is_some()
                && let Some(f) = &focus
                && !f.is::<gtk::Popover>()
                && (self.detail.borrow().as_ref().is_some_and(|d| f.is_ancestor(&d.root)) || self.detail.borrow().as_ref().and_then(|d| self.row(&d.key)).is_some_and(|r| f == r.name.upcast_ref::<gtk::Widget>()))
            {
                self.close_detail(true);
                return true;
            }
            return false;
        }
        if k.ctrl || k.alt || self.menu_open.get() {
            return false;
        }
        let Some(f) = focus else { return false };
        // Hvilken rad og kolonne har fokus?
        let groups = self.groups.borrow().clone();
        for g in groups.iter() {
            let vis = self.visible_rows(g);
            let Some((ri, ci)) = vis.iter().enumerate().find_map(|(i, r)| {
                if f == *r.pin.upcast_ref::<gtk::Widget>() {
                    Some((i, 0))
                } else if f == *r.name.upcast_ref::<gtk::Widget>() {
                    Some((i, 1))
                } else {
                    None
                }
            }) else {
                continue;
            };
            let (mut r, mut c) = (ri as i64, ci);
            match k.key {
                K::Down => r += 1,
                K::Up => r -= 1,
                K::Left => c = 0,
                K::Right => c = 1,
                K::Home => r = 0,
                K::End => r = vis.len() as i64 - 1,
                _ => return false,
            }
            let Some(next) = vis.get(r.clamp(0, vis.len() as i64 - 1) as usize) else { return true };
            self.roving(&next.def.key, c == 0);
            let b = if c == 0 { &next.pin } else { &next.name };
            b.grab_focus();
            return true;
        }
        false
    }

    /// `?vis=…`: åpner gruppen, ruller til raden og lyser den opp.
    fn reveal(self: &Rc<Self>, key: &str) {
        self.rebuild_if_needed();
        let Some(r) = self.row(key) else { return };
        self.clear_filter();
        self.apply_filter();
        if let Some(g) = self.group_of(key)
            && self.collapsed.borrow().contains(&g.id)
        {
            self.set_open(&g.id, true);
        }
        if r.def.thread.is_some() && !self.threads_open.get() {
            self.set_threads(true);
        }
        let w = Rc::downgrade(self);
        let k = key.to_string();
        glib::idle_add_local_once(move || {
            let Some(s) = w.upgrade() else { return };
            let Some(r) = s.row(&k) else { return };
            if let Some(p) = r.root.compute_point(&s.content, &gtk::graphene::Point::new(0.0, 0.0)) {
                let adj = s.scroller.vadjustment();
                let target = p.y() as f64 - adj.page_size() / 2.0;
                adj.set_value(target.clamp(0.0, (adj.upper() - adj.page_size()).max(0.0)));
            }
            // Lyser opp straks, og toner ut over ca. 1,3 s.
            r.root.add_css_class("flash");
            r.root.add_css_class("fading");
            let rr = r.root.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                rr.remove_css_class("flash");
                glib::timeout_add_local_once(std::time::Duration::from_millis(1400), move || rr.remove_css_class("fading"));
            });
            s.roving(&k, false);
            r.name.grab_focus();
        });
    }

    // ------------------------------------------------------------ Ikke tilgjengelig

    fn render_notes(self: &Rc<Self>) {
        crate::widgets::clear(&self.notes_box);
        let mut notes: Vec<(&str, String, String)> = Vec::new();
        let backend = self.app.data.borrow().backend.clone();
        let has_gpu = self.app.telemetry().is_some_and(|t| !t.gpus.is_empty());
        if has_gpu && backend != "sim" {
            let why = "Ikke innsamlet av appen. Tilgjengelighet for dette kortet og denne driveren er ikke verifisert her.";
            notes.push(("circle-slash", "Skjermkort · Hotspot-temperatur".into(), why.into()));
            notes.push(("circle-slash", "Skjermkort · Minnetemperatur".into(), why.into()));
            notes.push(("circle-slash", "Skjermkort · Kjernespenning".into(), "Ikke innsamlet: appen samler ikke inn GPU-kjernespenning.".into()));
        }
        if let Some(tel) = self.app.telemetry() {
            notes.push(("circle-slash", "Prosessor · Kjernespenning".into(), "Ikke innsamlet av appen. Målekilde og tilgang avhenger av CPU, hovedkort og driver.".into()));
            if !tel.sys.hwmon.iter().any(|d| SM::device_name(&d.name) == "Hovedkort") {
                notes.push(("circle-slash", "Hovedkort · vifter, spenninger og temperaturer".into(), "Ingen gjenkjent hovedkortsensor er tilgjengelig i hwmon. Sensorbrikke og driverstøtte er ikke verifisert; appen laster ikke kjernemoduler.".into()));
            }
            for d in &tel.sys.hwmon {
                for x in &d.sensors {
                    if x.err.as_deref() == Some("root") {
                        let (n, _) = SM::sensor_name(d, x);
                        notes.push(("lock", n, "Krever root".into()));
                    }
                }
            }
        }
        let group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        for (icon, what, why) in notes {
            let r = hbox(12, &["sn-note"]);
            let ic = Icon::new(icon, 14);
            ic.add_css_class("dim");
            ic.set_tooltip_text(Some(if icon == "lock" { "Krever root" } else { "Ikke støttet eller ikke tilgjengelig" }));
            r.append(&ic);
            let w = label(&what, &["bold", "sn-note-what"]);
            // Samme bredde i alle radene (forklaringene står i én kolonne).
            // I smale vinduer brytes navnet over to linjer i stedet for «…».
            w.set_wrap(true);
            w.set_wrap_mode(pango::WrapMode::WordChar);
            w.set_max_width_chars(28);
            w.set_xalign(0.0);
            group.add_widget(&w);
            r.append(&w);
            let y = label(&why, &["muted"]);
            y.set_wrap(true);
            y.set_wrap_mode(pango::WrapMode::WordChar);
            y.set_hexpand(true);
            r.append(&y);
            r.set_tooltip_text(Some(&format!("{what}: {why}")));
            self.notes_box.append(&r);
        }
        self.apply_filter();
    }

    #[cfg(debug_assertions)]
    fn hooks(self: &Rc<Self>) {
        use super::oversikt::bilder::hook;
        let w = Rc::downgrade(self);
        hook("sensorer.apne", move |f| {
            if let Some(s) = w.upgrade()
                && let Some(k) = s.app.gpu_key(f)
            {
                s.open_detail(&k);
            }
        });
        let w = Rc::downgrade(self);
        hook("sensorer.lukk", move |_| {
            if let Some(s) = w.upgrade() {
                s.close_detail(false);
            }
        });
        let w = Rc::downgrade(self);
        hook("sensorer.markor", move |a| {
            if let Some(s) = w.upgrade()
                && let Some(d) = s.detail.borrow().as_ref()
            {
                d.panel.chart.set_hover_frac(a.parse().ok());
            }
        });
        let w = Rc::downgrade(self);
        hook("sensorer.filter", move |q| {
            if let Some(s) = w.upgrade() {
                s.search.set_text(q);
                *s.filter.borrow_mut() = q.to_lowercase();
                s.apply_filter();
            }
        });
        let w = Rc::downgrade(self);
        hook("sensorer.rull", move |a| {
            if let Some(s) = w.upgrade() {
                let adj = s.scroller.vadjustment();
                adj.set_value(if a == "bunn" { adj.upper() } else { 0.0 });
            }
        });
    }
}
