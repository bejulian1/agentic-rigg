//! Rapport som tekst eller Markdown (produktspesifikasjonen 3.6), og
//! innholdet på Grafikkort og System.
//!
//! Sidene og rapporten bygger på de samme seksjonene ([`gpu::sections`],
//! [`system::sections`]), så «Kopier som tekst» gir det som står på siden,
//! og rapporten sier det samme som sidene. En seksjon er en liste med
//! etikett/verdi-rader, tabeller og merknader; [`render_sections`] gjør den
//! om til tekst eller Markdown.
//!
//! Serienumre, UUID og vertsnavn er **ikke** med som standard
//! ([`ReportInput::include_serials`]).

pub mod gpu;
pub mod system;

use crate::model::{DeviceState, GpuInfo, GpuStatic, Telemetry};
use crate::{tr, trf};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Markdown,
}

impl ReportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ReportFormat::Text => "txt",
            ReportFormat::Markdown => "md",
        }
    }
}

/// Én etikett/verdi-rad. `v` er `None` når verdien ikke kan leses; da står
/// `na` («Ikke støttet», «Krever root») og `info` sier hvorfor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kv {
    /// Stabil ID, så siden kan finne raden igjen (oppdatere, legge til knapper).
    pub id: &'static str,
    pub k: String,
    pub v: Option<String>,
    pub na: Option<String>,
    /// Forklaring (verktøytipset på siden).
    pub info: Option<String>,
}

impl Kv {
    pub fn new(id: &'static str, k: &str, v: Option<String>) -> Kv {
        Kv { id, k: k.into(), v, ..Default::default() }
    }

    pub fn na(mut self, text: &str) -> Kv {
        self.na = Some(text.into());
        self
    }

    pub fn info(mut self, text: impl Into<String>) -> Kv {
        let t: String = text.into();
        self.info = (!t.is_empty()).then_some(t);
        self
    }

    pub fn info_opt(mut self, text: Option<String>) -> Kv {
        self.info = text.filter(|t| !t.is_empty());
        self
    }

    /// Teksten som vises: verdien, eller grunnen til at den mangler.
    pub fn text(&self) -> String {
        self.v.clone().or_else(|| self.na.clone()).unwrap_or_else(|| crate::fmt::DASH.into())
    }

    pub fn missing(&self) -> bool {
        self.v.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub id: &'static str,
    pub head: Vec<String>,
    /// Høyrejusterte kolonner (tall).
    pub end: Vec<bool>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Kv(Kv),
    /// Underoverskrift i en liste.
    Sub(String),
    Table(Table),
    /// Merknad under innholdet.
    Note(String),
    /// Fri tekst (instruksjonssettene).
    Text(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub id: &'static str,
    pub title: String,
    /// Merket i overskriften («Referanse, ikke målt»).
    pub aside: Option<String>,
    pub items: Vec<Item>,
}

impl Section {
    pub fn new(id: &'static str, title: &str) -> Section {
        Section { id, title: title.into(), aside: None, items: vec![] }
    }

    pub fn kv(&mut self, kv: Kv) -> &mut Section {
        self.items.push(Item::Kv(kv));
        self
    }

    pub fn find(&self, id: &str) -> Option<&Kv> {
        self.items.iter().find_map(|i| match i {
            Item::Kv(k) if k.id == id => Some(k),
            _ => None,
        })
    }

    pub fn table(&self) -> Option<&Table> {
        self.items.iter().find_map(|i| match i {
            Item::Table(t) => Some(t),
            _ => None,
        })
    }
}

/// «Referanse, ikke målt» → «referanse, ikke målt», men «MHz» står.
/// På tysk beholdes stor forbokstav i substantiver («Referenzwert, nicht gemessen»).
fn lower_first(s: &str) -> String {
    if crate::i18n::active() == crate::i18n::Language::De {
        return s.to_string();
    }
    let mut c = s.chars();
    match (c.next(), c.next()) {
        (Some(a), Some(b)) if a.is_uppercase() && b.is_lowercase() => a.to_lowercase().chain(s.chars().skip(1)).collect(),
        _ => s.to_string(),
    }
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Seksjonene som tekst eller Markdown. `level` er overskriftsnivået i
/// Markdown (2 = `##`).
pub fn render_sections(sections: &[Section], fmt: ReportFormat, level: usize) -> String {
    let mut out = String::new();
    for s in sections {
        let title = match &s.aside {
            Some(a) => format!("{} ({})", s.title, lower_first(a)),
            None => s.title.clone(),
        };
        match fmt {
            _ if s.title.is_empty() => {}
            ReportFormat::Markdown => out.push_str(&format!("\n{} {title}\n\n", "#".repeat(level))),
            ReportFormat::Text => out.push_str(&format!("\n[{title}]\n")),
        }
        let mut in_kv = false;
        for item in &s.items {
            match item {
                Item::Kv(kv) => {
                    if fmt == ReportFormat::Markdown && !in_kv {
                        out.push_str(&format!("| {} | {} |\n|---|---|\n", tr!("Felt"), tr!("Verdi")));
                        in_kv = true;
                    }
                    // Forklaringen tas med der verdien mangler: den sier hvorfor.
                    let why = if kv.missing() { kv.info.clone() } else { None };
                    match fmt {
                        ReportFormat::Markdown => {
                            let v = match why {
                                Some(w) => format!("{} – {}", kv.text(), w),
                                None => kv.text(),
                            };
                            out.push_str(&format!("| {} | {} |\n", md_cell(&kv.k), md_cell(&v)));
                        }
                        ReportFormat::Text => {
                            out.push_str(&format!("{}: {}\n", kv.k, kv.text()));
                            if let Some(w) = why {
                                out.push_str(&format!("  ({w})\n"));
                            }
                        }
                    }
                }
                Item::Sub(t) => {
                    in_kv = false;
                    match fmt {
                        ReportFormat::Markdown => out.push_str(&format!("\n**{}**\n\n", md_cell(t))),
                        ReportFormat::Text => out.push_str(&format!("-- {t}\n")),
                    }
                }
                Item::Table(t) => {
                    in_kv = false;
                    if fmt == ReportFormat::Markdown {
                        if !out.ends_with("\n\n") {
                            out.push('\n');
                        }
                        out.push_str(&format!("| {} |\n", t.head.iter().map(|h| md_cell(h)).collect::<Vec<_>>().join(" | ")));
                        let al: Vec<&str> = (0..t.head.len()).map(|i| if t.end.get(i).copied().unwrap_or(false) { "---:" } else { "---" }).collect();
                        out.push_str(&format!("|{}|\n", al.join("|")));
                        for r in &t.rows {
                            out.push_str(&format!("| {} |\n", r.iter().map(|c| md_cell(c)).collect::<Vec<_>>().join(" | ")));
                        }
                    } else {
                        // Kolonner med fast bredde, så tabellen kan leses i en tekstfil.
                        let n = t.head.len();
                        let mut w = vec![0usize; n];
                        for r in std::iter::once(&t.head).chain(t.rows.iter()) {
                            for (i, c) in r.iter().enumerate().take(n) {
                                w[i] = w[i].max(c.chars().count());
                            }
                        }
                        for r in std::iter::once(&t.head).chain(t.rows.iter()) {
                            let cells: Vec<String> = r
                                .iter()
                                .enumerate()
                                .take(n)
                                .map(|(i, c)| {
                                    let pad = w[i].saturating_sub(c.chars().count());
                                    if t.end.get(i).copied().unwrap_or(false) { format!("{}{c}", " ".repeat(pad)) } else { format!("{c}{}", " ".repeat(pad)) }
                                })
                                .collect();
                            out.push_str(cells.join("  ").trim_end());
                            out.push('\n');
                        }
                    }
                }
                Item::Note(t) | Item::Text(t) => {
                    in_kv = false;
                    match fmt {
                        ReportFormat::Markdown => out.push_str(&format!("\n{t}\n")),
                        ReportFormat::Text => out.push_str(&format!("{t}\n")),
                    }
                }
            }
        }
    }
    out
}

/// Én side som tekst eller Markdown (Kopier på Grafikkort og System).
pub fn render_page(title: &str, sections: &[Section], fmt: ReportFormat) -> String {
    let head = match fmt {
        ReportFormat::Markdown => format!("# {title}\n"),
        ReportFormat::Text => format!("{title}\n"),
    };
    head + &render_sections(sections, fmt, 2)
}

/// Det rapporten bygges av. Feltene er JSON fra tjenesten (`list_gpus`,
/// `get_static`, `get_system`, `get_stats`, `get_telemetry`, `hello`).
#[derive(Clone, Debug, Default)]
pub struct ReportInput {
    /// `list_gpus.gpus`.
    pub gpus: Value,
    /// Liste med `GpuStatic`.
    pub statics: Value,
    /// `get_system`.
    pub system: Value,
    /// `get_stats`.
    pub stats: Value,
    /// Siste `telemetry`.
    pub telemetry: Value,
    /// `hello.daemon` (versjon, bygg, backend).
    pub service: Value,
    /// Det vinduet vet selv, som ekstra rader øverst (GTK, tegner).
    pub env: Vec<(String, String)>,
    /// Skrivebord (`XDG_CURRENT_DESKTOP`) og økttype fra vinduets miljø.
    pub desktop: Option<String>,
    pub session: Option<String>,
    /// Ta med serienumre, UUID og vertsnavn (av som standard).
    pub include_serials: bool,
    /// Når rapporten ble laget, som lokal tekst («7. nov. 2026 14:32»).
    pub created: String,
    /// Tidssonen (sekunder fra UTC) til tidspunktene i rapporten.
    pub utc_offset_s: i64,
}

/// Tilstanden til et kort i klartekst (produktspesifikasjonen 3.8).
pub fn state_text(g: &GpuInfo) -> String {
    match g.state {
        DeviceState::Ok => tr!("Leses med NVML").into(),
        DeviceState::DriverUpdated => trf!(
            "Driveren er oppdatert ({installert} er installert, {kjorer} kjører). Start maskinen på nytt for å lese skjermkortet.",
            installert = g.driver_installed.as_deref().unwrap_or(tr!("ny")),
            kjorer = g.driver.as_deref().unwrap_or(tr!("den gamle"))
        ),
        DeviceState::NoDriver => tr!("Fant ingen NVIDIA-driver").into(),
        DeviceState::Lost => tr!("Skjermkortet svarer ikke").into(),
        DeviceState::Suspended => tr!("Maskinen sover").into(),
    }
}

fn sensor_label(key: &str, tele: Option<&Telemetry>) -> Option<String> {
    if let Some((id, field)) = crate::sensor_key::split_gpu(key) {
        let field = match field {
            "temp" => tr!("Temperatur"),
            "gpuUtil" => tr!("GPU-last"),
            "memUtil" => tr!("Minnekontroller-last"),
            "encUtil" => tr!("Videokoder"),
            "decUtil" => tr!("Videodekoder"),
            "coreClock" => tr!("Kjerneklokke"),
            "smClock" => tr!("SM-klokke"),
            "memClock" => tr!("Minneklokke"),
            "videoClock" => tr!("Videoklokke"),
            "power" => tr!("Effekttrekk"),
            "powerLimit" => tr!("Effektgrense"),
            "fan" => tr!("Viftefart"),
            "vramUsed" => tr!("Videominne brukt"),
            "pcieGen" => tr!("PCIe-generasjon"),
            field => {
                let fan = |prefix| field.strip_prefix(prefix)?.parse::<usize>().ok()?.checked_add(1);
                let label = if let Some(nr) = fan("fanRpm") {
                    trf!("Vifte {nr}, turtall", nr = nr)
                } else {
                    trf!("Vifte {nr}", nr = fan("fan")?)
                };
                return Some(format!("{} · {label}", crate::csv::gpu_prefix(id, tele)));
            }
        };
        return Some(format!("{} · {field}", crate::csv::gpu_prefix(id, tele)));
    }
    let parts: Vec<_> = key.split('/').collect();
    let direction = |field| if field == "rx" { tr!("mottatt") } else { tr!("sendt") };
    let disk_field = |field| match field {
        "read" => Some(tr!("lesing")),
        "write" => Some(tr!("skriving")),
        "busy" => Some(tr!("aktivitet")),
        "temp" => Some(tr!("temperatur")),
        _ => None,
    };
    let (device, field) = match parts.as_slice() {
        ["cpu", "load"] => (tr!("Prosessor"), tr!("Total last")),
        ["cpu", "temp"] => return Some(tr!("CPU-temperatur").into()),
        ["cpu", "tctl"] => (tr!("Prosessor"), "Tctl"),
        ["cpu", "thread", nr, field @ ("load" | "mhz")] => {
            let thread = trf!("Tråd {nr}", nr = nr);
            let field = if *field == "load" { tr!("Last") } else { tr!("Klokke") };
            return Some(format!("{} · {thread} · {field}", tr!("Prosessor")));
        }
        ["mem", "usedPct"] => (tr!("Minne"), tr!("Brukt av totalt")),
        ["mem", "used"] => return Some(tr!("RAM brukt").into()),
        ["mem", "avail"] => return Some(tr!("RAM ledig").into()),
        ["mem", "swap"] => return Some(tr!("Swap brukt").into()),
        ["net", field @ ("rx" | "tx")] => (tr!("Nett"), direction(*field)),
        ["net", iface, field @ ("rx" | "tx")] => (*iface, direction(*field)),
        ["disk", field @ ("read" | "write")] => (tr!("Disker"), disk_field(*field)?),
        ["disk", id, field] => {
            let disk = tele?.sys.disks.iter().find(|disk| disk.id == *id)?;
            let base = if disk.model.is_empty() { disk.name.clone() } else { format!("{} ({})", disk.model, disk.name) };
            return Some(format!("{base} · {}", disk_field(*field)?));
        }
        _ => return None,
    };
    Some(format!("{device} · {field}"))
}

fn sensors_section(input: &ReportInput, tele: Option<&Telemetry>) -> Section {
    let mut s = Section::new("sensorer", tr!("Sensorer"));
    let flat = tele.map(crate::sensor_key::flatten).unwrap_or_default();
    let stats = input.stats.get("stats").and_then(Value::as_object);
    let localized = crate::i18n::active() != crate::i18n::Language::Nb;
    let mut rows = Vec::new();
    for (k, v) in &flat {
        let c = crate::csv::sensor_column(k, tele, localized);
        let name = localized.then(|| sensor_label(k, tele)).flatten().unwrap_or(c.name);
        let f = |x: Option<f64>| {
            let n = crate::fmt::num(x, c.decimals);
            if x.is_none() || c.unit.is_empty() { n } else { crate::fmt::with_unit(n, c.unit) }
        };
        let st = stats.and_then(|m| m.get(k));
        let g = |f: &str| st.and_then(|x| x.get(f)).and_then(Value::as_f64);
        rows.push(vec![name, f(Some(*v)), f(g("min")), f(g("max")), f(g("avg"))]);
    }
    s.items.push(Item::Table(Table { id: "sensorer", head: [tr!("Sensor"), tr!("Nå"), tr!("Min"), tr!("Maks"), tr!("Snitt")].map(String::from).to_vec(), end: vec![false, true, true, true, true], rows }));
    let when = |key: &str| input.stats.get(key).and_then(Value::as_i64).filter(|t| *t > 0).map(|t| crate::csv::iso_time(t, input.utc_offset_s, false));
    let mut note = Vec::new();
    if let Some(t) = tele.filter(|t| t.t > 0) {
        note.push(trf!("Nå: prøven fra {tid}.", tid = crate::csv::iso_time(t.t, input.utc_offset_s, false)));
    }
    if let Some(t) = when("since") {
        note.push(trf!("Min og maks siden {tid}.", tid = t));
    }
    if let Some(t) = when("avgSince") {
        note.push(trf!("Snitt siden {tid}.", tid = t));
    }
    if !note.is_empty() {
        s.items.push(Item::Note(note.join(" ")));
    }
    s
}

/// Hele rapporten: app og versjon, OS, driver, alle skjermkort (fra driveren og
/// referansen hver for seg), prosessor, hovedkort, minne, disker, alle
/// sensorer med Nå/Min/Maks/Snitt og tilstanden til hvert kort.
pub fn render(input: &ReportInput, fmt: ReportFormat) -> String {
    let title = trf!("{app} · rapport", app = crate::version::NAME);
    let mut out = String::new();
    match fmt {
        ReportFormat::Markdown => out.push_str(&format!("# {title}\n\n")),
        ReportFormat::Text => out.push_str(&format!("{title}\n{}\n\n", "=".repeat(title.chars().count()))),
    }
    let tele: Option<Telemetry> = serde_json::from_value(input.telemetry.clone()).ok().filter(|t: &Telemetry| t.t > 0);
    let gpus: Vec<GpuInfo> = serde_json::from_value(input.gpus.clone()).unwrap_or_default();
    let statics: Vec<GpuStatic> = serde_json::from_value(input.statics.clone()).unwrap_or_default();
    let os = &input.system["os"];

    // ---- Oversikt ----
    let mut head = Section::new("rapport", "");
    head.kv(Kv::new("app", tr!("App"), Some(trf!("{navn} {versjon} · bygg {bygg}", navn = crate::version::NAME, versjon = crate::version::VERSION, bygg = crate::version::BUILD_STR))));
    head.kv(Kv::new("laget", tr!("Laget"), Some(input.created.clone())));
    let svc = &input.service;
    if svc.is_object() {
        head.kv(Kv::new(
            "tjeneste",
            tr!("Tjenesten"),
            Some(trf!(
                "{navn} {versjon} · bygg {bygg} · {kilde}",
                navn = svc["name"].as_str().unwrap_or("agentic-riggd"),
                versjon = svc["version"].as_str().unwrap_or("?"),
                bygg = svc["build"],
                kilde = match svc["backend"].as_str() {
                    Some("nvml") => tr!("NVML, bare lesing"),
                    Some("sim") => tr!("testdata"),
                    Some("ingen") => tr!("uten skjermkort"),
                    _ => tr!("ukjent kilde"),
                }
            )),
        ));
    }
    head.kv(Kv::new("os", tr!("Operativsystem"), os["distro"].as_str().map(str::to_string)));
    head.kv(Kv::new("kjerne", tr!("Kjerne"), os["kernel"].as_str().map(str::to_string)));
    for (k, v) in &input.env {
        head.kv(Kv { id: "env", k: k.clone(), v: Some(v.clone()), ..Default::default() });
    }
    let first = statics.first();
    head.kv(Kv::new("driver", tr!("NVIDIA-driver"), first.and_then(|s| s.driver.clone()).or_else(|| gpus.first().and_then(|g| g.driver.clone()))).na(tr!("Ikke funnet")));
    head.kv(Kv::new("nvml", "NVML", first.and_then(|s| s.nvml_version.clone())).na("–"));
    head.kv(Kv::new("cuda", tr!("CUDA (driver)"), first.and_then(|s| s.cuda_version.clone())).na("–"));
    head.kv(Kv::new(
        "serienumre",
        tr!("Serienumre og UUID"),
        Some(if input.include_serials { tr!("Tatt med").into() } else { tr!("Ikke tatt med (velg «Ta med serienumre og UUID» når du lagrer)").into() }),
    ));
    out.push_str(&render_sections(&[head], fmt, 2));

    // ---- Skjermkortene ----
    let real: Vec<&GpuInfo> = gpus.iter().filter(|g| !g.id.is_empty()).collect();
    if real.is_empty() {
        let mut s = Section::new("gpu", tr!("Skjermkort"));
        let why = gpus.first().map(state_text).unwrap_or_else(|| tr!("Ingen skjermkort funnet").into());
        s.kv(Kv::new("tilstand", tr!("Tilstand"), Some(why)).info_opt(gpus.first().and_then(|g| g.detail.clone())));
        out.push_str(&render_sections(&[s], fmt, 2));
    }
    for (i, g) in real.iter().enumerate() {
        let st = statics.iter().find(|s| s.id == g.id).cloned().unwrap_or_else(|| GpuStatic { id: g.id.clone(), name: g.name.clone(), ..Default::default() });
        let sample = tele.as_ref().and_then(|t| t.gpus.get(&g.id));
        let label = if real.len() > 1 {
            trf!("Skjermkort {nr} · {navn}", nr = i + 1, navn = crate::model::short_gpu_name(&st.name))
        } else {
            trf!("Skjermkort · {navn}", navn = crate::model::short_gpu_name(&st.name))
        };
        match fmt {
            ReportFormat::Markdown => out.push_str(&format!("\n## {label}\n")),
            ReportFormat::Text => out.push_str(&format!("\n{label}\n{}\n", "-".repeat(label.chars().count()))),
        }
        let mut state = Section::new("tilstand", tr!("Tilstand"));
        state.kv(Kv::new("tilstand", tr!("Tilstand"), Some(state_text(g))).info_opt(g.detail.clone()));
        if g.state != DeviceState::Ok {
            state.kv(Kv::new("siden", tr!("Siden"), Some(crate::csv::iso_time(g.since, input.utc_offset_s, false))));
        }
        let secs = gpu::sections(&gpu::GpuInput { info: Some(g), st: &st, s: sample, show_uuid: input.include_serials, egg: None, utc_offset_s: input.utc_offset_s });
        let mut all = vec![state];
        all.extend(secs);
        out.push_str(&render_sections(&all, fmt, 3));
    }

    // ---- Maskinen ----
    if input.system.is_object() {
        let label = tr!("Maskinen");
        match fmt {
            ReportFormat::Markdown => out.push_str(&format!("\n## {label}\n")),
            ReportFormat::Text => out.push_str(&format!("\n{label}\n{}\n", "-".repeat(label.chars().count()))),
        }
        let sys_sections = system::sections(&system::SysInput {
            system: &input.system,
            sys: tele.as_ref().map(|t| &t.sys),
            show_hostname: input.include_serials,
            desktop: input.desktop.clone(),
            session: input.session.clone(),
            report: true,
            max_thread_mhz: None,
        });
        out.push_str(&render_sections(&sys_sections, fmt, 3));
    }

    // ---- Sensorene ----
    out.push_str(&render_sections(&[sensors_section(input, tele.as_ref())], fmt, 2));
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seksjon_som_tekst_og_markdown() {
        let mut s = Section::new("x", "Grenser");
        s.kv(Kv::new("a", "Effektgrense", Some("245 W".into())));
        s.kv(Kv::new("b", "Minnetemperatur", None).na("Ikke støttet").info("NVML har én termisk sensor."));
        s.items.push(Item::Table(Table { id: "t", head: vec!["Klokke".into(), "Nå".into()], end: vec![false, true], rows: vec![vec!["Grafikk".into(), "300".into()]] }));
        let t = render_sections(&[s.clone()], ReportFormat::Text, 2);
        assert!(t.contains("[Grenser]\nEffektgrense: 245 W\nMinnetemperatur: Ikke støttet\n  (NVML har én termisk sensor.)\n"));
        assert!(t.contains("Klokke    Nå\nGrafikk  300\n"), "{t}");
        let m = render_sections(&[s], ReportFormat::Markdown, 2);
        assert!(m.contains("## Grenser\n\n| Felt | Verdi |\n|---|---|\n| Effektgrense | 245 W |\n| Minnetemperatur | Ikke støttet – NVML har én termisk sensor. |\n"), "{m}");
        assert!(m.contains("| Klokke | Nå |\n|---|---:|\n| Grafikk | 300 |\n"), "{m}");
    }
}
