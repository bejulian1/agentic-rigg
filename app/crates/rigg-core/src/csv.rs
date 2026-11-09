//! CSV for logg og eksport (produktspesifikasjonen 3.2).
//!
//! - UTF-8, én overskriftsrad «Navn [enhet]», første kolonner `Tid` (ISO 8601
//!   med sone) og `Sekunder` (fra start). Tomt felt når en verdi mangler.
//! - **Standard CSV**: komma, punktum som desimaltegn.
//! - **Norsk regneark**: semikolon, komma som desimaltegn (LibreOffice og
//!   Excel med norsk oppsett åpner den rett).
//!
//! Navn og enheter på kolonnene kommer fra sensornøkkelen ([`column`]), så
//! loggen, eksporten og rapporten kaller ting det samme.

use crate::fmt::LocalTime;
use crate::model::Telemetry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsvFormat {
    /// Komma og punktum som desimaltegn (standard).
    Standard,
    /// Semikolon og komma som desimaltegn (norsk regneark).
    Norsk,
}

impl CsvFormat {
    pub fn from_name(s: &str) -> CsvFormat {
        if s == "norsk" { CsvFormat::Norsk } else { CsvFormat::Standard }
    }
    pub fn name(self) -> &'static str {
        match self {
            CsvFormat::Standard => "standard",
            CsvFormat::Norsk => "norsk",
        }
    }
    pub fn sep(self) -> char {
        match self {
            CsvFormat::Standard => ',',
            CsvFormat::Norsk => ';',
        }
    }
}

/// Én verdi som tekst i formatet. Tomt felt når verdien mangler.
pub fn cell(v: Option<f64>, decimals: usize, f: CsvFormat) -> String {
    match v.filter(|v| v.is_finite()) {
        None => String::new(),
        Some(v) => {
            let v = if v.abs() < 0.5 * 10f64.powi(-(decimals as i32)) { 0.0 } else { v };
            let s = format!("{v:.decimals$}");
            if f == CsvFormat::Norsk { s.replace('.', ",") } else { s }
        }
    }
}

/// Tekst med anførselstegn når den inneholder skilletegn, anførselstegn eller linjeskift.
pub fn text(s: &str, f: CsvFormat) -> String {
    if s.contains(f.sep()) || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// «2026-11-07T14:32:10+01:00» (eller med millisekunder: «…10.500+01:00»).
pub fn iso_time(ms: i64, utc_offset_s: i64, millis: bool) -> String {
    let t = crate::fmt::local(ms, utc_offset_s);
    let sign = if utc_offset_s < 0 { '-' } else { '+' };
    let off = utc_offset_s.abs();
    let frac = if millis { format!(".{:03}", ms.rem_euclid(1000)) } else { String::new() };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{frac}{sign}{:02}:{:02}",
        t.year,
        t.month,
        t.day,
        t.hour,
        t.minute,
        t.second,
        off / 3600,
        off % 3600 / 60
    )
}

/// «rigg-2026-11-07_14-32-10.csv», og «…-del2.csv» for del 2 og videre.
pub fn file_name(t: LocalTime, part: u32) -> String {
    let base = format!("rigg-{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second);
    if part <= 1 { format!("{base}.csv") } else { format!("{base}-del{part}.csv") }
}

/// Navn, enhet og antall desimaler for en kolonne.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub key: String,
    pub name: String,
    pub unit: &'static str,
    pub decimals: usize,
}

impl Column {
    /// «Temperatur [°C]», eller bare navnet når enheten er tom.
    pub fn header(&self) -> String {
        if self.unit.is_empty() { self.name.clone() } else { format!("{} [{}]", self.name, self.unit) }
    }
}

fn gpu_field(field: &str) -> (String, &'static str, usize) {
    let (n, u, d): (&str, &'static str, usize) = match field {
        "temp" => ("Temperatur", "°C", 0),
        "gpuUtil" => ("GPU-last", "%", 0),
        "memUtil" => ("Minnekontroller-last", "%", 0),
        "encUtil" => ("Videokoder", "%", 0),
        "decUtil" => ("Videodekoder", "%", 0),
        "coreClock" => ("Kjerneklokke", "MHz", 0),
        "smClock" => ("SM-klokke", "MHz", 0),
        "memClock" => ("Minneklokke", "MHz", 0),
        "videoClock" => ("Videoklokke", "MHz", 0),
        "power" => ("Effekttrekk", "W", 1),
        "powerLimit" => ("Effektgrense", "W", 0),
        "fan" => ("Viftefart", "%", 0),
        "vramUsed" => ("Videominne brukt", "MiB", 0),
        "pcieGen" => ("PCIe-generasjon", "", 0),
        "pcieWidth" => ("PCIe-bredde", "", 0),
        f => {
            if let Some(i) = f.strip_prefix("fanRpm").and_then(|n| n.parse::<usize>().ok()) {
                return (format!("Vifte {} turtall", i + 1), "o/min", 0);
            }
            if let Some(i) = f.strip_prefix("fan").and_then(|n| n.parse::<usize>().ok()) {
                return (format!("Vifte {}", i + 1), "%", 0);
            }
            return (f.to_string(), "", 2);
        }
    };
    (n.to_string(), u, d)
}

/// Navnet på et skjermkort i kolonnene: «GPU» med ett kort, «GPU 1», «GPU 2» med flere.
fn gpu_prefix(id: &str, t: Option<&Telemetry>) -> String {
    match t {
        Some(t) if t.gpus.len() > 1 => match t.gpus.keys().position(|k| k == id) {
            Some(i) => format!("GPU {}", i + 1),
            None => format!("GPU {id}"),
        },
        _ => "GPU".to_string(),
    }
}

/// Navn, enhet og desimaler for en sensornøkkel. `t` gir navnene på
/// hwmon-sensorene og antall skjermkort.
pub fn column(key: &str, t: Option<&Telemetry>) -> Column {
    let c = |name: String, unit: &'static str, decimals: usize| Column { key: key.to_string(), name, unit, decimals };
    if let Some((id, field)) = crate::sensor_key::split_gpu(key) {
        let (n, u, d) = gpu_field(field);
        return c(format!("{} · {n}", gpu_prefix(id, t)), u, d);
    }
    let parts: Vec<&str> = key.split('/').collect();
    match parts.as_slice() {
        ["cpu", "load"] => c("Prosessor · total last".into(), "%", 1),
        ["cpu", "temp"] => c("Prosessor · temperatur (Tdie)".into(), "°C", 1),
        ["cpu", "tctl"] => c("Prosessor · Tctl".into(), "°C", 1),
        ["cpu", "thread", n, "load"] => c(format!("Prosessor · tråd {n} last"), "%", 1),
        ["cpu", "thread", n, "mhz"] => c(format!("Prosessor · tråd {n} klokke"), "MHz", 0),
        ["mem", "usedPct"] => c("Minne · brukt av totalt".into(), "%", 1),
        ["mem", "used"] => c("Minne · brukt".into(), "GiB", 2),
        ["mem", "avail"] => c("Minne · tilgjengelig".into(), "GiB", 2),
        ["mem", "swap"] => c("Minne · swap brukt".into(), "GiB", 2),
        ["net", "rx"] => c("Nett · ned".into(), "B/s", 0),
        ["net", "tx"] => c("Nett · opp".into(), "B/s", 0),
        ["net", ifc, dir] => c(format!("Nett · {ifc} {}", if *dir == "rx" { "ned" } else { "opp" }), "B/s", 0),
        ["disk", "read"] => c("Disker · lesing totalt".into(), "B/s", 0),
        ["disk", "write"] => c("Disker · skriving totalt".into(), "B/s", 0),
        ["disk", id, field] => {
            // Modellen og kjernenavnet fra prøven («Disk ST2000DM008-2FR102 (sda)»), ellers ID-en.
            let name = t.and_then(|t| t.sys.disks.iter().find(|d| d.id == *id)).map(|d| format!("Disk {} ({})", d.model, d.name)).unwrap_or_else(|| format!("Disk {id}"));
            match *field {
                "read" => c(format!("{name} · lesing"), "B/s", 0),
                "write" => c(format!("{name} · skriving"), "B/s", 0),
                "busy" => c(format!("{name} · aktivitet"), "%", 1),
                "temp" => c(format!("{name} · temperatur"), "°C", 0),
                f => c(format!("{name} · {f}"), "", 2),
            }
        }
        ["hwmon", dev, sensor] => {
            let kind = sensor.trim_end_matches(|ch: char| ch.is_ascii_digit());
            let (unit, d) = match kind {
                "temp" => ("°C", 1),
                "fan" => ("o/min", 0),
                "in" => ("V", 3),
                "power" => ("W", 2),
                "curr" => ("A", 3),
                _ => ("", 2),
            };
            // Navnet fra prøven («Prosessor · Tdie»), ellers det rå navnet. Navnet
            // kan selv slutte på «_1» (iwlwifi_1); «_2» … betyr enhet nummer to med
            // samme navn (sensor_key::flatten).
            let find = |name: &str, nth: usize| t.and_then(|t| t.sys.hwmon.iter().filter(|h| h.name == name).nth(nth));
            let split = dev.rsplit_once('_').and_then(|(b, n)| n.parse::<usize>().ok().filter(|n| *n >= 2).map(|n| (b, n - 1)));
            let hit = find(dev, 0).map(|h| (h, *dev)).or_else(|| split.and_then(|(b, n)| find(b, n).map(|h| (h, b))));
            let base = hit.map(|(_, b)| b).or(split.map(|(b, _)| b)).unwrap_or(dev);
            let name = hit
                .and_then(|(h, _)| h.sensors.iter().find(|x| x.key == *sensor).map(|x| crate::sensormeta::sensor_name(h, x).0))
                .unwrap_or_else(|| format!("{} · {sensor}", crate::sensormeta::device_name(base)));
            c(name, unit, d)
        }
        _ => c(key.to_string(), "", 2),
    }
}

/// Overskriftsraden: `Tid`, `Sekunder` og kolonnene.
pub fn header_line(cols: &[Column], f: CsvFormat) -> String {
    let sep = f.sep().to_string();
    let mut out: Vec<String> = vec!["Tid".into(), "Sekunder".into()];
    out.extend(cols.iter().map(|c| text(&c.header(), f)));
    out.join(&sep) + "\n"
}

/// Én rad: tid, sekunder fra start og verdiene i kolonnerekkefølgen.
pub fn row_line(time: &str, seconds: f64, cols: &[Column], values: &[Option<f64>], f: CsvFormat) -> String {
    let sep = f.sep().to_string();
    let mut out: Vec<String> = Vec::with_capacity(cols.len() + 2);
    out.push(text(time, f));
    out.push(cell(Some(seconds), 1, f));
    for (i, c) in cols.iter().enumerate() {
        out.push(cell(values.get(i).copied().flatten(), c.decimals, f));
    }
    out.join(&sep) + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celler_i_begge_formatene() {
        assert_eq!(cell(Some(12.34), 1, CsvFormat::Standard), "12.3");
        assert_eq!(cell(Some(12.34), 1, CsvFormat::Norsk), "12,3");
        assert_eq!(cell(None, 1, CsvFormat::Norsk), "");
        assert_eq!(cell(Some(f64::NAN), 1, CsvFormat::Norsk), "");
        assert_eq!(cell(Some(-0.01), 1, CsvFormat::Standard), "0.0", "aldri -0.0");
        assert_eq!(text("a,b", CsvFormat::Standard), "\"a,b\"");
        assert_eq!(text("a,b", CsvFormat::Norsk), "a,b");
        assert_eq!(text("si \"hei\"", CsvFormat::Norsk), "\"si \"\"hei\"\"\"");
    }

    #[test]
    fn tid_og_filnavn() {
        // 7. november 2026 13:32:10 UTC = 14:32:10 i Norge (vintertid).
        let ms = 1_794_058_330_000;
        assert_eq!(iso_time(ms, 3600, false), "2026-11-07T14:32:10+01:00");
        assert_eq!(iso_time(ms + 500, 3600, true), "2026-11-07T14:32:10.500+01:00");
        assert_eq!(iso_time(ms, -5 * 3600 - 1800, false), "2026-11-07T08:02:10-05:30");
        let t = crate::fmt::local(ms, 3600);
        assert_eq!(file_name(t, 1), "rigg-2026-11-07_14-32-10.csv");
        assert_eq!(file_name(t, 2), "rigg-2026-11-07_14-32-10-del2.csv");
    }

    #[test]
    fn kolonnenavn() {
        assert_eq!(column("gpu:0000:1c:00.0/temp", None).header(), "GPU · Temperatur [°C]");
        assert_eq!(column("gpu:0000:1c:00.0/fanRpm1", None).header(), "GPU · Vifte 2 turtall [o/min]");
        assert_eq!(column("gpu:0000:1c:00.0/pcieGen", None).header(), "GPU · PCIe-generasjon");
        assert_eq!(column("cpu/thread/3/mhz", None).header(), "Prosessor · tråd 3 klokke [MHz]");
        assert_eq!(column("net/wlo1/rx", None).header(), "Nett · wlo1 ned [B/s]");
        assert_eq!(column("hwmon/k10temp/temp2", None).header(), "Prosessor · temp2 [°C]");
        assert_eq!(column("disk/WFL454P6/temp", None).header(), "Disk WFL454P6 · temperatur [°C]");
        assert_eq!(column("disk/read", None).header(), "Disker · lesing totalt [B/s]");
        let mut d = Telemetry::default();
        d.sys.disks.push(crate::disk::DiskSample { id: "WFL454P6".into(), name: "sda".into(), model: "ST2000DM008-2FR102".into(), ..Default::default() });
        assert_eq!(column("disk/WFL454P6/write", Some(&d)).header(), "Disk ST2000DM008-2FR102 (sda) · skriving [B/s]");
        let mut w = Telemetry::default();
        let s = |key: &str, label: &str| crate::model::HwmonSensor { key: key.into(), label: label.into(), kind: "temp".into(), ..Default::default() };
        w.sys.hwmon.push(crate::model::HwmonDevice { id: "hwmon1".into(), name: "iwlwifi_1".into(), sensors: vec![s("temp1", "temp1")] });
        w.sys.hwmon.push(crate::model::HwmonDevice { id: "hwmon0".into(), name: "k10temp".into(), sensors: vec![s("temp2", "Tdie")] });
        w.sys.hwmon.push(crate::model::HwmonDevice { id: "hwmon5".into(), name: "k10temp".into(), sensors: vec![s("temp1", "Tctl")] });
        assert_eq!(column("hwmon/iwlwifi_1/temp1", Some(&w)).header(), "Wi-Fi-kort · temperatur [°C]");
        assert_eq!(column("hwmon/k10temp/temp2", Some(&w)).header(), "Prosessor · Tdie [°C]");
        assert_eq!(column("hwmon/k10temp_2/temp1", Some(&w)).header(), "Prosessor · Tctl [°C]");
        let mut t = Telemetry::default();
        t.gpus.insert("0000:1c:00.0".into(), Default::default());
        t.gpus.insert("0000:2d:00.0".into(), Default::default());
        assert_eq!(column("gpu:0000:2d:00.0/power", Some(&t)).header(), "GPU 2 · Effekttrekk [W]");
    }

    #[test]
    fn rader() {
        let cols = vec![column("gpu:x/temp", None), column("gpu:x/power", None)];
        assert_eq!(header_line(&cols, CsvFormat::Norsk), "Tid;Sekunder;GPU · Temperatur [°C];GPU · Effekttrekk [W]\n");
        assert_eq!(row_line("t", 1.0, &cols, &[Some(41.0), None], CsvFormat::Standard), "t,1.0,41,\n");
        assert_eq!(row_line("t", 1.5, &cols, &[Some(41.0), Some(12.26)], CsvFormat::Norsk), "t;1,5;41;12,3\n");
    }
}
