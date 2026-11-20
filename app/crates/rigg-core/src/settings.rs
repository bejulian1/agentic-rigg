//! Innstillinger: port av `mergeSettings` og `lenientSettings` i
//! `server/store.js`, med samme nøkler, grenser og meldinger, så en eksport
//! fra demoen kan importeres (produktspesifikasjonen 3.10).
//!
//! To filer, én skriver hver (arkitektur.md kap. 7):
//! - `ui.json` ([`Settings`]) eies av vinduet.
//! - `tjeneste.json` ([`ServiceSettings`]) eies av tjenesten; vinduet endrer den med `set_settings`.
//!
//! Ukjente nøkler ignoreres. Ved innlesing får ugyldige felt standardverdi
//! hver for seg ([`lenient`]); ved import avvises hele fila med en norsk
//! melding ([`merge`]).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError(pub String);

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

fn err<T>(m: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError(m.into()))
}

/// Sidene i rekkefølgen i sidefeltet (også tastene 1–9 og 0): (id, navn).
/// Prosessor og Disker kom i 1.1.0 (runde 6A), og Styring-sidene fikk nye
/// navn i runde 6B (`id` er den samme).
pub const PAGES: [(&str, &str); 10] = [
    ("oversikt", "Oversikt"),
    ("sensorer", "Sensorer"),
    ("grafer", "Grafer"),
    ("grafikkort", "Grafikkort"),
    ("prosessor", "Prosessor"),
    ("disker", "Disker"),
    ("system", "System"),
    ("kontroller", "Skjermkort › Klokker og effekt"),
    ("viftekurve", "Skjermkort › Vifter"),
    ("lys", "Skjermkort › RGB-lys"),
];
pub const INTERVALS: [u32; 3] = [500, 1000, 2000];
pub const THEMES: [&str; 3] = ["system", "light", "dark"];
pub const PALETTES: [&str; 4] = ["grafitt", "sand", "fjord", "midnatt"];
pub const ACCENTS: [&str; 6] = ["blue", "teal", "green", "purple", "pink", "slate"];
const OLD_ACCENTS: [&str; 3] = ["yellow", "orange", "red"];
pub const DENSITIES: [&str; 3] = ["compact", "standard", "comfortable"];
pub const FONT_SCALES: [u32; 4] = [92, 100, 108, 116];
pub const MOTIONS: [&str; 4] = ["system", "full", "reduced", "off"];
pub const CHART_WINDOWS: [u32; 7] = [60, 300, 900, 3600, 21600, 86400, 172800];
pub const WINDOW_PAGES: [&str; 3] = ["oversikt", "kontroller", "sensorer"];
pub const STATUS_ITEMS: [&str; 10] = ["gpuTemp", "gpuLoad", "gpuPower", "vram", "gpuFan", "cpuTemp", "cpuLoad", "mem", "netDown", "netUp"];
pub const STATUS_MAX: usize = 8;
pub const CHARTS: [&str; 13] = ["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan", "vram", "cpuLoad", "cpuTemp", "mem", "net", "diskTemp", "diskIo"];
pub const INFOBARS: [&str; 3] = ["kontroller", "viftekurve", "lys"];
pub const CONFIRM_CHOICES: [u32; 4] = [10, 15, 30, 60];
pub const SECTIONS: [&str; 8] = ["generelt", "utseende", "format", "sidefelt", "grafer", "logging", "data", "om"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub gpu_temp_warn: f64,
    pub gpu_temp_crit: f64,
    pub cpu_temp_warn: f64,
    pub cpu_temp_crit: f64,
    pub load_warn: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds { gpu_temp_warn: 75.0, gpu_temp_crit: 85.0, cpu_temp_warn: 80.0, cpu_temp_crit: 90.0, load_warn: 90.0 }
    }
}

const THRESH: [(&str, f64, f64); 5] =
    [("gpuTempWarn", 40.0, 100.0), ("gpuTempCrit", 40.0, 110.0), ("cpuTempWarn", 40.0, 100.0), ("cpuTempCrit", 40.0, 110.0), ("loadWarn", 50.0, 100.0)];

impl Thresholds {
    fn get_mut(&mut self, k: &str) -> &mut f64 {
        match k {
            "gpuTempWarn" => &mut self.gpu_temp_warn,
            "gpuTempCrit" => &mut self.gpu_temp_crit,
            "cpuTempWarn" => &mut self.cpu_temp_warn,
            "cpuTempCrit" => &mut self.cpu_temp_crit,
            _ => &mut self.load_warn,
        }
    }

    fn merge(&self, p: &Value) -> Result<Thresholds, ValidationError> {
        let Some(o) = p.as_object() else { return err("thresholds må være et objekt") };
        let mut t = self.clone();
        for (k, lo, hi) in THRESH {
            let Some(v) = o.get(k) else { continue };
            let Some(n) = v.as_f64().filter(|n| n.is_finite()) else { return err(format!("{k} må være et tall")) };
            *t.get_mut(k) = n.clamp(lo, hi).round();
        }
        if t.gpu_temp_crit <= t.gpu_temp_warn {
            return err("Rød grense for GPU-temperatur må være over gul");
        }
        if t.cpu_temp_crit <= t.cpu_temp_warn {
            return err("Rød grense for CPU-temperatur må være over gul");
        }
        Ok(t)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Windows {
    pub oversikt: u32,
    pub kontroller: u32,
    pub sensorer: u32,
}

impl Default for Windows {
    fn default() -> Self {
        Windows { oversikt: 60, kontroller: 60, sensorer: 300 }
    }
}

/// Vindusstørrelsen som huskes (posisjon kan ikke huskes på Wayland).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeom {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl Default for WindowGeom {
    fn default() -> Self {
        WindowGeom { width: 1280, height: 820, maximized: false }
    }
}

/// `ui.json`: alle app-innstillingene fra demoen unntatt `startSimulated`,
/// pluss `appIcon`, `eggsFound`, `settingsSection`, `lastPage`, `window` og `lys3d`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_language")]
    pub language: String,
    pub interval_ms: u32,
    pub unit: String,
    pub theme: String,
    pub accent: String,
    pub palette: String,
    pub high_contrast: bool,
    pub density: String,
    pub font_scale: u32,
    pub motion: String,
    pub start_page: String,
    pub show_kbd: bool,
    pub hidden_pages: Vec<String>,
    pub status_items: Vec<String>,
    pub status_sparks: bool,
    pub side_collapsed: bool,
    pub thresholds: Thresholds,
    pub chart_window: u32,
    pub windows: Windows,
    pub chart_sync: bool,
    pub chart_scale: String,
    pub chart_fill: bool,
    pub charts: Vec<String>,
    pub sensor_sparks: bool,
    pub sensor_keep_min_max: bool,
    pub decimals: String,
    pub mem_unit: String,
    pub net_unit: String,
    pub clock: String,
    pub dismissed: Vec<String>,
    pub confirm_sec: u32,
    pub surprises: bool,
    // ---- nytt i 1.0 ----
    pub app_icon: String,
    pub eggs_found: Vec<String>,
    pub settings_section: String,
    pub last_page: String,
    pub window: WindowGeom,
    /// 3D-kortet på Lys: «auto» (2D til du har valgt 3D én gang, deretter det
    /// sist valgte), «3d» (alltid 3D) eller «2d» (alltid 2D).
    pub lys3d: String,
    // ---- nytt i 1.3 ----
    /// Produsenten brukeren har satt per kort (UUID, ellers PCI-adressen):
    /// «Palit». Kortet melder ofte NVIDIA-referanse (10de) i PCI-subsystemet,
    /// så produsenten kan ikke alltid leses (`vendor::board_maker`).
    #[serde(default)]
    pub gpu_makers: std::collections::BTreeMap<String, String>,
}

fn default_language() -> String {
    crate::i18n::SYSTEM.into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: default_language(),
            // 1 s, ikke demoens 0,5 s (produktspesifikasjonen 3.10).
            interval_ms: 1000,
            unit: "C".into(),
            theme: "system".into(),
            accent: "blue".into(),
            palette: "grafitt".into(),
            high_contrast: false,
            density: "standard".into(),
            font_scale: 100,
            motion: "system".into(),
            start_page: "oversikt".into(),
            show_kbd: false,
            hidden_pages: vec![],
            status_items: ["gpuTemp", "gpuLoad", "gpuPower", "cpuTemp", "mem"].map(String::from).to_vec(),
            status_sparks: true,
            side_collapsed: false,
            thresholds: Thresholds::default(),
            chart_window: 300,
            windows: Windows::default(),
            chart_sync: true,
            chart_scale: "auto".into(),
            chart_fill: true,
            charts: ["gpuTemp", "gpuLoad", "coreClock", "memClock", "power", "fan"].map(String::from).to_vec(),
            sensor_sparks: true,
            sensor_keep_min_max: true,
            decimals: "auto".into(),
            mem_unit: "bin".into(),
            net_unit: "byte".into(),
            clock: "system".into(),
            dismissed: vec![],
            confirm_sec: 15,
            surprises: true,
            app_icon: "standard".into(),
            eggs_found: vec![],
            settings_section: "generelt".into(),
            last_page: "oversikt".into(),
            window: WindowGeom::default(),
            lys3d: "auto".into(),
            gpu_makers: Default::default(),
        }
    }
}

fn one_of(v: &Value, list: &[&str], msg: &str) -> Result<String, ValidationError> {
    match v.as_str() {
        Some(s) if list.contains(&s) => Ok(s.to_string()),
        _ => err(msg),
    }
}

fn one_of_num(v: &Value, list: &[u32], msg: &str) -> Result<u32, ValidationError> {
    match v.as_u64() {
        Some(n) if v.is_u64() && list.contains(&(n as u32)) => Ok(n as u32),
        _ => err(msg),
    }
}

fn boolean(v: &Value, name: &str) -> Result<bool, ValidationError> {
    v.as_bool().ok_or_else(|| ValidationError(format!("{name} må være true/false")))
}

fn subset(v: &Value, list: &[&str], name: &str, min: usize, max: usize) -> Result<Vec<String>, ValidationError> {
    let Some(a) = v.as_array() else { return err(format!("{name} må være en liste")) };
    let mut out: Vec<String> = Vec::new();
    for x in a {
        let s = x.as_str().filter(|s| list.contains(s));
        let Some(s) = s else {
            let shown: String = match x.as_str() {
                Some(t) => t.chars().take(40).collect(),
                None => x.to_string().chars().take(40).collect(),
            };
            return err(format!("Ukjent verdi i {name}: {shown}"));
        };
        if !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    }
    if out.len() < min || out.len() > max {
        return err(format!("{name} må ha {min}–{max} valg"));
    }
    Ok(out)
}

fn segment_ok(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => {}
        _ => return false,
    }
    s.len() <= 40 && cs.all(|c| c.is_ascii_alphanumeric() || "_.:@+-".contains(c))
}

/// En festet sensor: demoens rad-ID-er (`gpu/temp`, `cpu/t3`, `hw/hwmon3/temp1`)
/// og 1.0-nøklene (`gpu:0000:1c:00.0/temp`, `hwmon/k10temp/temp1`, `cpu/thread/3/load`).
pub fn pin_ok(s: &str) -> bool {
    if s.len() > 80 {
        return false;
    }
    let mut parts = s.split('/');
    let first = parts.next().unwrap_or("");
    let first_ok = matches!(first, "gpu" | "cpu" | "mem" | "net" | "hw" | "hwmon")
        || first.strip_prefix("gpu:").is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.'));
    let rest: Vec<&str> = parts.collect();
    first_ok && (1..=3).contains(&rest.len()) && rest.iter().all(|p| segment_ok(p))
}

fn status_list(v: &Value) -> Result<Vec<String>, ValidationError> {
    let Some(a) = v.as_array() else { return err("statusItems må være en liste") };
    let mut out: Vec<String> = Vec::new();
    for x in a {
        let Some(s) = x.as_str().filter(|s| STATUS_ITEMS.contains(s) || pin_ok(s)) else {
            let shown: String = x.as_str().map(|t| t.chars().take(40).collect()).unwrap_or_else(|| x.to_string());
            return err(format!("Ukjent verdi i statusItems: {shown}"));
        };
        if !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    }
    if out.len() > STATUS_MAX {
        return err(format!("Statusblokken kan ha høyst {STATUS_MAX} verdier"));
    }
    Ok(out)
}

fn ident(v: &Value, name: &str) -> Result<String, ValidationError> {
    match v.as_str() {
        Some(s) if !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') => Ok(s.into()),
        _ => err(format!("{name} er ugyldig")),
    }
}

/// Fletter en delvis innstilling inn i `base`. Ukjente felt ignoreres, og et
/// ugyldig felt avviser hele endringen med en norsk melding.
pub fn merge(base: &Settings, patch: &Value) -> Result<Settings, ValidationError> {
    let Some(p) = patch.as_object() else { return err("Forventet et JSON-objekt") };
    let mut o = base.clone();
    for (k, v) in p {
        match k.as_str() {
            "language" => {
                let code = v.as_str().filter(|code| *code == crate::i18n::SYSTEM || crate::i18n::Language::from_code(code).is_some());
                o.language = code.ok_or_else(|| ValidationError(crate::tr!("Ukjent språkvalg").into()))?.into();
            }
            "intervalMs" => o.interval_ms = one_of_num(v, &INTERVALS, "Ugyldig intervall")?,
            "unit" => o.unit = one_of(v, &["C", "F"], "Enhet må være C eller F")?,
            "theme" => o.theme = one_of(v, &THEMES, "Tema må være system, light eller dark")?,
            "accent" => {
                o.accent = if v.as_str().is_some_and(|s| OLD_ACCENTS.contains(&s)) { "blue".into() } else { one_of(v, &ACCENTS, "Ukjent aksentfarge")? }
            }
            "palette" => o.palette = one_of(v, &PALETTES, "Ukjent fargetema")?,
            "highContrast" => o.high_contrast = boolean(v, "highContrast")?,
            "density" => o.density = one_of(v, &DENSITIES, "Ukjent tetthet")?,
            "fontScale" => o.font_scale = one_of_num(v, &FONT_SCALES, "Ugyldig skriftstørrelse")?,
            "motion" => o.motion = one_of(v, &MOTIONS, "Animasjoner må være system, full, reduced eller off")?,
            "startPage" => {
                let mut l: Vec<&str> = PAGES.iter().map(|p| p.0).collect();
                l.push("last");
                o.start_page = one_of(v, &l, "Ukjent startside")?
            }
            "showKbd" => o.show_kbd = boolean(v, "showKbd")?,
            "hiddenPages" => {
                let l: Vec<&str> = PAGES.iter().map(|p| p.0).collect();
                o.hidden_pages = subset(v, &l, "hiddenPages", 0, PAGES.len() - 1)?
            }
            "statusItems" => o.status_items = status_list(v)?,
            "statusSparks" => o.status_sparks = boolean(v, "statusSparks")?,
            "sideCollapsed" => o.side_collapsed = boolean(v, "sideCollapsed")?,
            "thresholds" => o.thresholds = o.thresholds.merge(v)?,
            "chartWindow" => {
                let msg = format!("Tidsvindu må være en av {} s", CHART_WINDOWS.map(|w| w.to_string()).join(", "));
                o.chart_window = if v.as_u64() == Some(120) { 300 } else { one_of_num(v, &CHART_WINDOWS, &msg)? }
            }
            "windows" => {
                let Some(w) = v.as_object() else { return err("windows må være et objekt") };
                let msg = format!("Tidsvindu må være en av {} s", CHART_WINDOWS.map(|w| w.to_string()).join(", "));
                for (page, win) in w {
                    let n = one_of_num(win, &CHART_WINDOWS, &msg)?;
                    match page.as_str() {
                        "oversikt" => o.windows.oversikt = n,
                        "kontroller" => o.windows.kontroller = n,
                        "sensorer" => o.windows.sensorer = n,
                        other => return err(format!("Ukjent side i windows: {}", other.chars().take(20).collect::<String>())),
                    }
                }
            }
            "chartSync" => o.chart_sync = boolean(v, "chartSync")?,
            "chartScale" => o.chart_scale = one_of(v, &["auto", "fixed"], "chartScale må være auto eller fixed")?,
            "chartFill" => o.chart_fill = boolean(v, "chartFill")?,
            "charts" => o.charts = subset(v, &CHARTS, "charts", 1, CHARTS.len())?,
            "sensorSparks" => o.sensor_sparks = boolean(v, "sensorSparks")?,
            "sensorKeepMinMax" => o.sensor_keep_min_max = boolean(v, "sensorKeepMinMax")?,
            "decimals" => o.decimals = one_of(v, &["auto", "0", "1"], "Desimaler må være auto, 0 eller 1")?,
            "memUnit" => o.mem_unit = one_of(v, &["bin", "dec"], "memUnit må være bin eller dec")?,
            "netUnit" => o.net_unit = one_of(v, &["byte", "bit"], "netUnit må være byte eller bit")?,
            "clock" => o.clock = one_of(v, &["system", "24", "12"], "Klokke må være system, 24 eller 12")?,
            "dismissed" => o.dismissed = subset(v, &INFOBARS, "dismissed", 0, INFOBARS.len())?,
            "confirmSec" => {
                o.confirm_sec = one_of_num(v, &CONFIRM_CHOICES, &format!("Fristen må være {} s", CONFIRM_CHOICES.map(|c| c.to_string()).join(", ")))?
            }
            "surprises" => o.surprises = boolean(v, "surprises")?,
            "appIcon" => o.app_icon = ident(v, "appIcon")?,
            "lys3d" => o.lys3d = one_of(v, &["auto", "3d", "2d"], "lys3d må være auto, 3d eller 2d")?,
            "gpuMakers" => o.gpu_makers = gpu_makers(v)?,
            "eggsFound" => o.eggs_found = subset(v, &crate::easter::IDS, "eggsFound", 0, crate::easter::IDS.len())?,
            "settingsSection" => o.settings_section = one_of(v, &SECTIONS, "Ukjent del av Innstillinger")?,
            "lastPage" => {
                let l: Vec<&str> = PAGES.iter().map(|p| p.0).collect();
                o.last_page = one_of(v, &l, "Ukjent side")?
            }
            "window" => {
                let Some(w) = v.as_object() else { return err("window må være et objekt") };
                let dim = |key: &str, lo: i64| w.get(key).and_then(Value::as_i64).map(|n| n.clamp(lo, 16384) as i32);
                if let Some(x) = dim("width", 640) {
                    o.window.width = x;
                }
                if let Some(x) = dim("height", 480) {
                    o.window.height = x;
                }
                if let Some(m) = w.get("maximized") {
                    o.window.maximized = boolean(m, "window.maximized")?;
                }
            }
            _ => {} // ukjent: ignoreres (også startSimulated fra demoen)
        }
    }
    Ok(o)
}

/// Produsentene per kort: hele lista byttes. Tom tekst fjerner kortet.
fn gpu_makers(v: &Value) -> Result<std::collections::BTreeMap<String, String>, ValidationError> {
    let Some(m) = v.as_object() else { return err("gpuMakers må være et objekt") };
    if m.len() > 16 {
        return err("Høyst 16 kort i gpuMakers");
    }
    let mut out = std::collections::BTreeMap::new();
    for (k, x) in m {
        let Some(name) = x.as_str() else { return err("Produsenten må være tekst") };
        let name = name.trim();
        if k.is_empty() || k.len() > 80 || k.chars().any(char::is_control) {
            return err("Ugyldig kort i gpuMakers");
        }
        if name.chars().count() > 40 || name.chars().any(char::is_control) {
            return err("Produsenten kan ha høyst 40 tegn");
        }
        if !name.is_empty() {
            out.insert(k.clone(), name.to_string());
        }
    }
    Ok(out)
}

/// Leser lagrede innstillinger felt for felt, så ett dårlig felt ikke
/// nullstiller resten (`lenientSettings`).
pub fn lenient(raw: &Value) -> Settings {
    let mut out = Settings::default();
    if let Some(o) = raw.as_object() {
        for (k, v) in o {
            let mut one = Map::new();
            one.insert(k.clone(), v.clone());
            if let Ok(next) = merge(&out, &Value::Object(one)) {
                out = next;
            }
        }
    }
    out
}

impl Settings {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Sidene som vises i sidefeltet, i rekkefølge.
    pub fn visible_pages(&self) -> Vec<&'static str> {
        PAGES.iter().map(|p| p.0).filter(|p| !self.hidden_pages.iter().any(|h| h == p)).collect()
    }

    pub fn start(&self) -> String {
        let p = if self.start_page == "last" { self.last_page.clone() } else { self.start_page.clone() };
        if PAGES.iter().any(|x| x.0 == p) { p } else { "oversikt".into() }
    }
}

// ---------------------------------------------------------------- tjeneste.json

pub const LOG_INTERVALS: [u32; 5] = [500, 1000, 2000, 5000, 10000];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSettings {
    /// Alle sensorer, eller bare `keys`.
    pub all: bool,
    pub keys: Vec<String>,
    pub interval_ms: u32,
    /// Mappa. `None` gir `~/Dokumenter/Agentic Rigg/`.
    pub dir: Option<String>,
    /// «standard» (komma, punktum) eller «norsk» (semikolon, komma).
    pub format: String,
}

impl Default for LogSettings {
    fn default() -> Self {
        LogSettings { all: true, keys: vec![], interval_ms: 1000, dir: None, format: "standard".into() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmSettings {
    /// Skrivebordsvarsler ved rød grense (BØR, av som standard).
    pub desktop: bool,
    pub gpu_temp: bool,
    pub cpu_temp: bool,
    pub gpu_lost: bool,
    /// Diskhelse (1.1.0): SMART melder feil, eller nye dårlige sektorer.
    #[serde(default = "yes")]
    pub disk_health: bool,
}

fn yes() -> bool {
    true
}

impl Default for AlarmSettings {
    fn default() -> Self {
        AlarmSettings { desktop: false, gpu_temp: true, cpu_temp: true, gpu_lost: true, disk_health: true }
    }
}

/// `tjeneste.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettings {
    /// «Fortsett i bakgrunnen»: av til brukeren har bestemt seg (kap. 10).
    pub background: bool,
    /// «Takt i bakgrunnen»: 1, 2 (standard) eller 5 s.
    pub background_interval_ms: u32,
    /// «Husk min og maks mellom økter» (= `sensorKeepMinMax`).
    pub keep_min_max: bool,
    pub log: LogSettings,
    pub alarms: AlarmSettings,
    pub thresholds: Thresholds,
    pub confirm_sec: u32,
}

impl Default for ServiceSettings {
    fn default() -> Self {
        ServiceSettings {
            background: false,
            background_interval_ms: 2000,
            keep_min_max: true,
            log: LogSettings::default(),
            alarms: AlarmSettings::default(),
            thresholds: Thresholds::default(),
            confirm_sec: 15,
        }
    }
}

pub fn merge_service(base: &ServiceSettings, patch: &Value) -> Result<ServiceSettings, ValidationError> {
    let Some(p) = patch.as_object() else { return err("Forventet et JSON-objekt") };
    let mut o = base.clone();
    for (k, v) in p {
        match k.as_str() {
            "background" => o.background = boolean(v, "background")?,
            "backgroundIntervalMs" => o.background_interval_ms = one_of_num(v, &crate::proto::BACKGROUND_INTERVALS, "Takten i bakgrunnen må være 1, 2 eller 5 s")?,
            "keepMinMax" | "sensorKeepMinMax" => o.keep_min_max = boolean(v, "keepMinMax")?,
            "thresholds" => o.thresholds = o.thresholds.merge(v)?,
            "confirmSec" => o.confirm_sec = one_of_num(v, &CONFIRM_CHOICES, "Fristen må være 10, 15, 30 eller 60 s")?,
            "log" => {
                let Some(l) = v.as_object() else { return err("log må være et objekt") };
                for (lk, lv) in l {
                    match lk.as_str() {
                        "all" => o.log.all = boolean(lv, "log.all")?,
                        "keys" => {
                            let Some(a) = lv.as_array() else { return err("log.keys må være en liste") };
                            let mut keys = Vec::new();
                            for x in a.iter().take(256) {
                                match x.as_str().filter(|s| crate::sensor_key::valid(s)) {
                                    Some(s) if !keys.iter().any(|k: &String| k == s) => keys.push(s.to_string()),
                                    Some(_) => {}
                                    None => return err("Ukjent sensor i log.keys"),
                                }
                            }
                            o.log.keys = keys;
                        }
                        "intervalMs" => o.log.interval_ms = one_of_num(lv, &LOG_INTERVALS, "Loggintervallet må være 0,5, 1, 2, 5 eller 10 s")?,
                        "dir" => {
                            o.log.dir = match lv {
                                Value::Null => None,
                                Value::String(s) if s.starts_with('/') && s.len() < 1024 && !s.contains('\0') => Some(s.clone()),
                                _ => return err("Loggmappa må være en full sti"),
                            }
                        }
                        "format" => o.log.format = one_of(lv, &["standard", "norsk"], "Formatet må være standard eller norsk")?,
                        _ => {}
                    }
                }
            }
            "alarms" => {
                let Some(a) = v.as_object() else { return err("alarms må være et objekt") };
                for (ak, av) in a {
                    match ak.as_str() {
                        "desktop" => o.alarms.desktop = boolean(av, "alarms.desktop")?,
                        "gpuTemp" => o.alarms.gpu_temp = boolean(av, "alarms.gpuTemp")?,
                        "cpuTemp" => o.alarms.cpu_temp = boolean(av, "alarms.cpuTemp")?,
                        "gpuLost" => o.alarms.gpu_lost = boolean(av, "alarms.gpuLost")?,
                        "diskHealth" => o.alarms.disk_health = boolean(av, "alarms.diskHealth")?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(o)
}

pub fn lenient_service(raw: &Value) -> ServiceSettings {
    let mut out = ServiceSettings::default();
    if let Some(o) = raw.as_object() {
        for (k, v) in o {
            let mut one = Map::new();
            one.insert(k.clone(), v.clone());
            if let Ok(next) = merge_service(&out, &Value::Object(one)) {
                out = next;
            }
        }
    }
    out
}

impl ServiceSettings {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------- Eksport og import

pub const EXPORT_FORMAT: &str = "agentic-rigg-settings";

/// Demoens format: `{"format", "formatVersion", "app", "exportedAt", "settings": {…}}`,
/// laget av `ui.json` og `tjeneste.json` sammen.
pub fn export(ui: &Settings, svc: &ServiceSettings, exported_at: &str) -> Value {
    let mut s = ui.to_value();
    if let Some(o) = s.as_object_mut() {
        // Det som bare gjelder denne maskinen og denne økta, følger ikke med.
        for k in ["window", "eggsFound", "settingsSection", "lastPage"] {
            o.remove(k);
        }
        o.insert("background".into(), json!(svc.background));
        o.insert("backgroundIntervalMs".into(), json!(svc.background_interval_ms));
        o.insert("log".into(), serde_json::to_value(&svc.log).unwrap_or(Value::Null));
        o.insert("alarms".into(), serde_json::to_value(&svc.alarms).unwrap_or(Value::Null));
    }
    json!({"format": EXPORT_FORMAT, "formatVersion": 1, "app": crate::version::VERSION, "exportedAt": exported_at, "settings": s})
}

/// Leser en eksport (fra demoen eller fra 1.0) og deler den på de to filene.
/// Ugyldig innhold avviser hele importen, som i demoen. Vinduets egne felt
/// (vindusstørrelse, funne egg) beholdes fra `keep`.
pub fn import(data: &Value, keep: &Settings, svc_base: &ServiceSettings) -> Result<(Settings, ServiceSettings), ValidationError> {
    let s = match data.get("settings") {
        Some(s) => s,
        None if data.get("format").is_some() => return err("Fila har ingen innstillinger"),
        None => data,
    };
    if !s.is_object() {
        return err("Forventet et JSON-objekt");
    }
    let mut ui = merge(&Settings::default(), s)?;
    ui.window = keep.window.clone();
    ui.eggs_found = keep.eggs_found.clone();
    ui.last_page = keep.last_page.clone();
    let mut svc = merge_service(&ServiceSettings::default(), s)?;
    svc.thresholds = ui.thresholds.clone();
    svc.keep_min_max = ui.sensor_keep_min_max;
    svc.confirm_sec = ui.confirm_sec;
    if s.get("background").is_none() {
        svc.background = svc_base.background;
        svc.background_interval_ms = svc_base.background_interval_ms;
    }
    Ok((ui, svc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imp(v: Value) -> Result<Settings, ValidationError> {
        merge(&Settings::default(), &v)
    }

    #[test]
    fn standardverdier_som_produksjonen_vil() {
        let d = Settings::default();
        assert_eq!(d.interval_ms, 1000, "1 s er standard i 1.0");
        assert!(d.sensor_keep_min_max);
        assert!(d.surprises);
        assert!(!d.side_collapsed);
        assert_eq!(d.confirm_sec, 15);
        let s = ServiceSettings::default();
        assert!(!s.background, "bakgrunn er av til brukeren har bestemt seg");
        assert_eq!(s.background_interval_ms, 2000);
    }

    #[test]
    fn feil_typer_avvises_med_norsk_melding() {
        for bad in [
            json!({"unit": "K"}),
            json!({"fontScale": "100"}),
            json!({"highContrast": "ja"}),
            json!({"palette": "neon"}),
            json!({"charts": []}),
            json!({"hiddenPages": "oversikt"}),
            json!({"statusItems": ["gpuTemp", "gpuLoad", "gpuPower", "vram", "gpuFan", "cpuTemp", "cpuLoad", "mem", "netDown"]}),
            json!({"statusItems": ["../../etc"]}),
            json!({"statusItems": [42]}),
            json!({"thresholds": {"gpuTempWarn": "x"}}),
            json!({"thresholds": []}),
            json!({"confirmSec": 20}),
            json!({"sideCollapsed": "true"}),
            json!({"surprises": 0}),
            json!({"intervalMs": 100}),
        ] {
            assert!(imp(bad.clone()).is_err(), "{bad}");
        }
        assert!(imp(Value::Null).is_err());
        assert!(imp(json!([1])).is_err());
        assert_eq!(imp(json!({"thresholds": {"gpuTempWarn": 95, "gpuTempCrit": 90}})).unwrap_err().0, "Rød grense for GPU-temperatur må være over gul");
    }

    #[test]
    fn ekstra_felt_ignoreres_og_tall_klemmes() {
        let out = imp(json!({"__proto__": {"x": 1}, "theme": "dark", "extra": "<script>", "startSimulated": true, "thresholds": {"gpuTempWarn": 5}})).unwrap();
        assert_eq!(out.theme, "dark");
        assert_eq!(out.thresholds.gpu_temp_warn, 40.0);
        assert_eq!(imp(json!({"accent": "orange"})).unwrap().accent, "blue", "gamle aksenter blir blå");
        assert_eq!(imp(json!({"chartWindow": 120})).unwrap().chart_window, 300);
    }

    #[test]
    fn ett_dårlig_felt_nullstiller_ikke_resten() {
        let s = lenient(&json!({"accent": "neon", "unit": "F", "thresholds": {"gpuTempWarn": 90, "gpuTempCrit": 80}, "statusItems": ["gpuTemp", "finnesIkke"]}));
        assert_eq!(s.unit, "F");
        assert_eq!(s.accent, "blue");
        assert_eq!(s.thresholds, Thresholds::default());
        assert_eq!(s.status_items, Settings::default().status_items);
        let s = lenient(&json!({"statusItems": ["gpuTemp", "cpuTemp", "mem"]}));
        assert_eq!(s.status_items, vec!["gpuTemp", "cpuTemp", "mem"]);
    }

    #[test]
    fn statusblokken_tar_både_demoens_og_nye_nøkler() {
        let pins = json!(["cpu/t3", "gpuTemp", "hw/hwmon3/temp1", "net/wlo1/rx", "mem/swap", "gpu:0000:1c:00.0/temp", "hwmon/k10temp/temp2", "cpu/thread/3/load"]);
        assert_eq!(imp(json!({"statusItems": pins})).unwrap().status_items.len(), 8);
        assert_eq!(imp(json!({"statusItems": ["mem", "gpuTemp", "mem"]})).unwrap().status_items, vec!["mem", "gpuTemp"]);
        assert!(imp(json!({"statusItems": []})).unwrap().status_items.is_empty());
        for bad in ["gpu", "gpu/", "disk/sda", "gpu/../x", "gpu/a b", &format!("gpu/{}", "x".repeat(41)), "gpu/a/b/c/d", "<script>"] {
            assert!(imp(json!({"statusItems": [bad]})).is_err(), "{bad}");
        }
    }

    #[test]
    fn eksport_og_import_gir_det_samme_tilbake() {
        let ui = Settings {
            theme: "dark".into(),
            palette: "fjord".into(),
            high_contrast: true,
            accent: "green".into(),
            unit: "F".into(),
            decimals: "1".into(),
            clock: "12".into(),
            charts: vec!["gpuTemp".into(), "net".into()],
            status_items: vec![],
            hidden_pages: vec!["lys".into()],
            app_icon: "teal".into(),
            ..Default::default()
        };
        let svc = ServiceSettings { background: true, background_interval_ms: 5000, ..Default::default() };
        let e = export(&ui, &svc, "2026-11-07T10:00:00Z");
        let text = serde_json::to_string(&e).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        let (ui2, svc2) = import(&back, &ui, &ServiceSettings::default()).unwrap();
        assert_eq!(ui2, ui);
        assert!(svc2.background);
        assert_eq!(svc2.background_interval_ms, 5000);
    }

    #[test]
    fn import_av_ekte_eksport_fra_demoen() {
        let text = include_str!("../tests/demo-eksport.json");
        let v: Value = serde_json::from_str(text).unwrap();
        let (ui, svc) = import(&v, &Settings::default(), &ServiceSettings::default()).unwrap();
        assert_eq!(ui.palette, "midnatt");
        assert_eq!(ui.interval_ms, 500, "valget fra demoen beholdes");
        assert_eq!(ui.chart_window, 300, "120 s fra eldre versjoner blir 5 min");
        assert_eq!(ui.status_items, vec!["gpuTemp", "gpuLoad", "gpuPower", "cpuTemp", "mem"]);
        assert!(!svc.background, "demoen hadde ikke valget; standard beholdes");
        assert!(import(&json!({"format": EXPORT_FORMAT}), &ui, &svc).is_err());
        assert!(import(&json!({"settings": {"unit": "K"}}), &ui, &svc).is_err());
    }

    #[test]
    fn tjenesten_validerer_sine_felt() {
        let b = ServiceSettings::default();
        assert!(merge_service(&b, &json!({"backgroundIntervalMs": 3000})).is_err());
        assert_eq!(merge_service(&b, &json!({"backgroundIntervalMs": 5000})).unwrap().background_interval_ms, 5000);
        assert!(merge_service(&b, &json!({"log": {"dir": "relativ"}})).is_err());
        let s = merge_service(&b, &json!({"log": {"format": "norsk", "keys": ["cpu/load", "cpu/load"]}})).unwrap();
        assert_eq!(s.log.format, "norsk");
        assert_eq!(s.log.keys, vec!["cpu/load"]);
        assert!(!lenient_service(&json!({"background": "ja", "keepMinMax": false})).keep_min_max);
    }

    #[test]
    fn produsenten_per_kort() {
        let s = merge(&Settings::default(), &json!({"gpuMakers": {"GPU-1234": " Palit ", "GPU-5678": ""}})).unwrap();
        assert_eq!(s.gpu_makers.get("GPU-1234").map(String::as_str), Some("Palit"));
        assert!(!s.gpu_makers.contains_key("GPU-5678"), "tom tekst fjerner kortet");
        assert!(merge(&s, &json!({"gpuMakers": {"GPU-1": "x".repeat(41)}})).is_err());
        assert!(merge(&s, &json!({"gpuMakers": "Palit"})).is_err());
        // Gamle innstillinger uten feltet leses som før.
        assert!(lenient(&json!({"theme": "dark"})).gpu_makers.is_empty());
    }
}
