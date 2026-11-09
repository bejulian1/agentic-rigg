//! Diskene (1.1.0): datatypene som tjenesten sender i `get_disks` og i
//! prøvene (`sys.disks`), og tolkningen av SMART som begge sider bruker:
//! norske navn på attributtene, enhetene, slitasje, skrevet totalt og helsa
//! (OK, Advarsel, Feil).
//!
//! Kildene (lest av `rigg-hw/src/sys/disk.rs` og `udisks.rs`): UDisks2 over
//! systembussen for SMART (uten root, det UDisks2 alt har lest; vi ber aldri
//! om en ny SMART-lesing eller selvtest), `/proc/diskstats` for hastighetene
//! og sysfs, udev-databasen og `statvfs` for resten.

use serde::{Deserialize, Serialize};

/// Én disk i en prøve (hver runde). Rater i byte/s, aktivitet i %.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiskSample {
    /// Stabil ID i sensornøklene: serienummeret, ellers kjernenavnet.
    pub id: String,
    /// Kjernenavnet («sda»). Kan bytte mellom oppstarter; `id` gjør ikke det.
    pub name: String,
    pub model: String,
    pub read: Option<f64>,
    pub write: Option<f64>,
    /// Andel av tiden disken hadde I/O i gang (`io_ticks`), %.
    pub busy: Option<f64>,
    /// Fra SMART (UDisks2 leser den selv omtrent hvert 10. min). `None` når
    /// den ikke finnes, eller er eldre enn [`TEMP_MAX_AGE_MS`].
    pub temp: Option<f64>,
}

/// Eldre SMART-temperatur enn dette brukes ikke som sensorverdi.
pub const TEMP_MAX_AGE_MS: i64 = 30 * 60_000;

/// Svaret på `get_disks`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiskList {
    /// Når lista ble laget (ms).
    pub t: i64,
    /// `udisks` (SMART fra UDisks2), `sysfs` (UDisks2 svarer ikke) eller `sim`.
    pub source: String,
    /// Hvorfor SMART mangler («UDisks2 svarer ikke på systembussen»).
    pub note: Option<String>,
    pub disks: Vec<Disk>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Disk {
    pub id: String,
    pub name: String,
    /// «/dev/sda».
    pub dev: String,
    pub model: String,
    pub vendor: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub wwn: Option<String>,
    /// Byte.
    pub size: Option<f64>,
    /// 0 = SSD (ingen plater), `None` = ukjent.
    pub rpm: Option<u32>,
    /// «HDD», «SSD» eller «NVMe».
    pub kind: String,
    /// «SATA», «USB», «NVMe», «SD/eMMC», «Virtuell» eller «».
    pub bus: String,
    pub removable: bool,
    /// Systemet (`/`) ligger på denne disken.
    pub system: bool,
    /// «6,0 Gb/s» (SATA) eller «PCIe 3.0 ×4» (NVMe).
    pub link: Option<String>,
    pub scheduler: Option<String>,
    pub logical_block: Option<u32>,
    pub physical_block: Option<u32>,
    pub smart: Smart,
    pub partitions: Vec<Partition>,
    pub health: Health,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Smart {
    /// `udisks`, `sim` eller `""` (ingen SMART-kilde).
    pub source: String,
    pub supported: bool,
    pub enabled: bool,
    /// Disken melder selv at den feiler (SMART-status).
    pub failing: bool,
    /// °C.
    pub temp: Option<f64>,
    pub power_on_hours: Option<f64>,
    pub power_cycles: Option<f64>,
    pub bad_sectors: Option<i64>,
    pub attrs_failing: Option<i64>,
    pub attrs_failed_past: Option<i64>,
    /// UDisks2: `success`, `inprogress`, `aborted`, `interrupted`, `fatal`, `error_*`.
    pub selftest: Option<String>,
    pub selftest_remaining: Option<i32>,
    /// Når UDisks2 sist leste SMART (ms).
    pub updated: Option<i64>,
    /// Gjenværende levetid, % (SSD).
    pub life_left: Option<f64>,
    /// Skrevet og lest totalt, byte.
    pub written: Option<f64>,
    pub read: Option<f64>,
    pub attributes: Vec<SmartAttr>,
    pub nvme: Option<NvmeHealth>,
    /// Hvorfor SMART mangler for denne disken.
    pub note: Option<String>,
}

/// Én rad i SMART-tabellen (ATA), som UDisks2 gir den
/// (`a(ysqiiixia{sv})`: id, navn, flagg, verdi, verste, terskel, tolket
/// verdi og enheten til den tolkede verdien).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SmartAttr {
    pub id: u8,
    pub name: String,
    pub flags: u16,
    /// Normalisert verdi (høyere er bedre), -1 når den mangler.
    pub value: i32,
    pub worst: i32,
    pub threshold: i32,
    /// libatasmart sin tolkede verdi i enheten [`SmartAttr::unit`].
    pub pretty: i64,
    /// 0 ukjent, 1 ingen, 2 ms, 3 sektorer, 4 mK, 5 % (liten), 6 %, 7 MB.
    pub unit: i32,
}

/// NVMe-helseloggen (UDisks2 `NVMe.Controller`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NvmeHealth {
    /// `spare`, `temperature`, `degraded`, `readonly`, `volatile_mem`, `pmr_readonly`.
    pub critical_warning: Vec<String>,
    pub avail_spare: Option<f64>,
    pub spare_thresh: Option<f64>,
    pub percent_used: Option<f64>,
    pub unsafe_shutdowns: Option<f64>,
    pub media_errors: Option<f64>,
    pub error_log_entries: Option<f64>,
    pub warning_temp_minutes: Option<f64>,
    pub critical_temp_minutes: Option<f64>,
    /// Varmegrensene kontrolleren oppgir, °C.
    pub warning_temp: Option<f64>,
    pub critical_temp: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Partition {
    pub name: String,
    pub dev: String,
    pub number: Option<u32>,
    pub size: Option<f64>,
    pub fs: Option<String>,
    pub label: Option<String>,
    pub mount: Option<String>,
    /// Byte fra `statvfs` (bare montert).
    pub total: Option<f64>,
    pub used: Option<f64>,
    /// Ledig for vanlige brukere (`f_bavail`).
    pub avail: Option<f64>,
}

impl Partition {
    pub fn used_pct(&self) -> Option<f64> {
        let (u, a) = (self.used?, self.avail?);
        let t = u + a;
        (t > 0.0).then(|| u / t * 100.0)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Ingen SMART å vurdere (USB-bro, UDisks2 mangler).
    #[default]
    Unknown,
    Ok,
    Warn,
    Fail,
}

impl Level {
    pub fn text(self) -> &'static str {
        match self {
            Level::Unknown => "Ukjent",
            Level::Ok => "OK",
            Level::Warn => "Advarsel",
            Level::Fail => "Feil",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Health {
    pub level: Level,
    /// Korte grunner, viktigste først («2 dårlige sektorer»).
    pub reasons: Vec<String>,
}

// ---------------------------------------------------------------- SMART-attributtene

/// Norske navn på de vanlige ATA-attributtene (som CrystalDiskInfo).
pub fn attr_label(id: u8, name: &str) -> String {
    let s = match id {
        1 => "Lesefeilrate",
        2 => "Gjennomstrømning",
        3 => "Oppspinningstid",
        4 => "Start/stopp",
        5 => "Omallokerte sektorer",
        7 => "Søkefeilrate",
        8 => "Søketid",
        9 => "Driftstimer",
        10 => "Nye oppspinningsforsøk",
        11 => "Nye kalibreringsforsøk",
        12 => "Strømsykluser",
        13 => "Myke lesefeil",
        170 => "Reserveblokker",
        171 => "Programmeringsfeil",
        172 => "Slettefeil",
        173 => "Slitasjeutjevning",
        174 => "Uventede strømbrudd",
        175 => "Programmeringsfeil (brikke)",
        176 => "Slettefeil (brikke)",
        177 => "Slitasjeutjevning",
        178 => "Brukte reserveblokker (brikke)",
        179 => "Brukte reserveblokker",
        180 => "Ubrukte reserveblokker",
        181 => "Programmeringsfeil totalt",
        182 => "Slettefeil totalt",
        183 => "Dårlige blokker under drift",
        184 => "Ende-til-ende-feil",
        187 => "Rapporterte ukorrigerbare feil",
        188 => "Tidsavbrudd for kommandoer",
        189 => "Høye skrivehoder",
        190 => "Luftstrømtemperatur",
        191 => "G-sensor-feil",
        192 => "Nødparkeringer",
        193 => "Lastesykluser",
        194 => "Temperatur",
        195 => "Maskinvarekorrigerte feil",
        196 => "Omallokeringshendelser",
        197 => "Sektorer som venter",
        198 => "Ukorrigerbare sektorer",
        199 => "UDMA CRC-feil",
        200 => "Skrivefeilrate",
        202 => "Brukt levetid",
        231 => "Gjenværende levetid",
        232 => "Gjenværende utholdenhet",
        233 => "Slitasjeindikator",
        235 => "Andel gode blokker",
        240 => "Flytimer for hodene",
        241 => "Skrevet totalt",
        242 => "Lest totalt",
        243 => "Skrevet totalt (øvre)",
        246 => "Skrevet totalt",
        _ => "",
    };
    if !s.is_empty() {
        return s.to_string();
    }
    // «attribute-245» → «Attributt 245»; ellers libatasmart-navnet med mellomrom.
    if name.starts_with("attribute-") || name.is_empty() {
        return format!("Attributt {id}");
    }
    let mut t = name.replace('-', " ");
    if let Some(c) = t.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    t
}

/// Attributter der en rå verdi over 0 betyr skadde sektorer (CrystalDiskInfo
/// gir «Advarsel» for 05, C5 og C6).
pub const BAD_SECTOR_ATTRS: [u8; 3] = [5, 197, 198];

impl SmartAttr {
    /// Pre-fail (bit 0): når verdien når terskelen, er disken i ferd med å feile.
    pub fn prefail(&self) -> bool {
        self.flags & 1 == 1
    }

    /// Verdien er på eller under terskelen nå.
    pub fn failing(&self) -> bool {
        self.threshold > 0 && self.value >= 0 && self.value <= self.threshold
    }

    /// Verdien har vært på eller under terskelen.
    pub fn failed_past(&self) -> bool {
        self.threshold > 0 && self.worst >= 0 && self.worst <= self.threshold
    }

    /// Den rå verdien når enheten er et antall (sektorer eller «ingen»).
    pub fn count(&self) -> Option<i64> {
        matches!(self.unit, 1 | 3).then_some(self.pretty)
    }
}

/// Byte per enhet i attributt 241/242 for en modell: 512 (LBA-er) eller 32 MiB.
/// libatasmart regner alltid med 32 MiB (65 536 sektorer), som stemmer for
/// Transcend, men ikke for Samsung, Seagate og de fleste andre, som teller
/// LBA-er. Uten treff i tabellen velges det som gir et rimelig tall: mer enn
/// 3 000 hele disker skrevet er ikke troverdig.
pub fn lba_unit_bytes(model: &str, raw: f64, size: Option<f64>) -> f64 {
    const MIB32: f64 = 32.0 * 1024.0 * 1024.0;
    let m = model.to_ascii_uppercase();
    if m.starts_with("TS") || m.contains("TRANSCEND") {
        return MIB32;
    }
    if m.starts_with("SAMSUNG") || m.starts_with("ST") || m.starts_with("WDC") || m.starts_with("CT") || m.starts_with("CRUCIAL") || m.starts_with("KINGSTON") {
        return 512.0;
    }
    match size {
        Some(s) if s > 0.0 && raw * MIB32 > s * 3000.0 => 512.0,
        _ => MIB32,
    }
}

/// Den rå telleren bak libatasmarts «MB» for 241/242 (den regner rå × 32 MiB / 10⁶).
pub fn lba_raw_from_pretty(pretty_mb: i64) -> f64 {
    (pretty_mb as f64 * 1e6 / (65_536.0 * 512.0)).round()
}

/// Skrevet og lest totalt (byte) fra 241/242 (eller 246 for skrevet).
pub fn written_read(attrs: &[SmartAttr], model: &str, size: Option<f64>) -> (Option<f64>, Option<f64>) {
    let get = |id: u8| attrs.iter().find(|a| a.id == id);
    let bytes = |a: &SmartAttr| -> Option<f64> {
        match a.unit {
            7 => {
                let raw = lba_raw_from_pretty(a.pretty);
                (raw > 0.0).then(|| raw * lba_unit_bytes(model, raw, size))
            }
            // Tolket som et rått tall: LBA-er.
            1 | 0 if a.pretty > 0 => Some(a.pretty as f64 * 512.0),
            _ => None,
        }
    };
    let w = get(241).and_then(bytes).or_else(|| get(246).and_then(bytes));
    let r = get(242).and_then(bytes);
    (w, r)
}

/// Gjenværende levetid (%) fra de normaliserte verdiene som betyr det:
/// 231 (SSD Life Left), 233 (Intel Media Wearout), 232 (Endurance
/// Remaining), 177 og 173 (slitasjeutjevning), og 202 (Crucial: rå = brukt %).
pub fn life_left(attrs: &[SmartAttr]) -> Option<f64> {
    for id in [231u8, 233, 232, 177, 173] {
        if let Some(a) = attrs.iter().find(|a| a.id == id)
            && (0..=100).contains(&a.value)
        {
            return Some(a.value as f64);
        }
    }
    attrs.iter().find(|a| a.id == 202 && matches!(a.unit, 1 | 6)).and_then(|a| (0..=100).contains(&a.pretty).then_some(100.0 - a.pretty as f64))
}

/// Teksten i «Tolket»-kolonnen: «3 244 t», «24 °C», «0 sektorer», «8,5 TB».
/// `model` og `size` trengs for 241/242.
pub fn attr_text(a: &SmartAttr, model: &str, size: Option<f64>) -> String {
    use crate::fmt as F;
    match (a.id, a.unit) {
        (241 | 242 | 246, _) => {
            let (w, r) = written_read(std::slice::from_ref(a), model, size);
            let b = if a.id == 242 { r } else { w };
            match b {
                Some(b) => disk_bytes(b),
                None => F::num(Some(a.pretty as f64), 0),
            }
        }
        (_, 2) => F::with_unit(F::num(Some(a.pretty as f64 / 3.6e6), 0), "t"),
        (_, 4) => F::temp(Some(a.pretty as f64 / 1000.0 - 273.15), 0),
        (_, 3) => F::plural(a.pretty, "sektor", "sektorer"),
        (_, 5 | 6) => F::pct(Some(a.pretty as f64), 0),
        (_, 7) => disk_bytes(a.pretty as f64 * 1e6),
        _ => F::num(Some(a.pretty as f64), 0),
    }
}

/// Diskstørrelser desimalt, som produsenten: «1,0 TB», «128 GB», «8,48 TB».
pub fn disk_bytes(b: f64) -> String {
    use crate::fmt as F;
    if !b.is_finite() {
        return F::DASH.into();
    }
    let (v, u) = if b >= 1e12 {
        (b / 1e12, "TB")
    } else if b >= 1e9 {
        (b / 1e9, "GB")
    } else if b >= 1e6 {
        (b / 1e6, "MB")
    } else if b >= 1e3 {
        (b / 1e3, "kB")
    } else {
        (b, "B")
    };
    let d = if u == "B" || v >= 100.0 { 0 } else if v >= 10.0 { 1 } else { 2 };
    F::with_unit(F::num(Some(v), d), u)
}

/// Størrelsen på merkelappen: «2 TB», «1 TB», «128 GB» (avrundet som produsenten).
pub fn disk_size_label(b: f64) -> String {
    use crate::fmt as F;
    if b >= 1e12 {
        let tb = b / 1e12;
        let d = if (tb - tb.round()).abs() < 0.05 { 0 } else { 1 };
        F::with_unit(F::num(Some(tb), d), "TB")
    } else {
        F::with_unit(F::num(Some((b / 1e9).round()), 0), "GB")
    }
}

// ---------------------------------------------------------------- Helsa

/// Grenser for disktemperaturen (°C): advarsel, feil.
pub fn temp_limits(kind: &str) -> (f64, f64) {
    match kind {
        "HDD" => (55.0, 65.0),
        "NVMe" => (70.0, 80.0),
        _ => (60.0, 70.0),
    }
}

/// Helsa ut fra SMART, som CrystalDiskInfo: Feil når disken selv sier det,
/// eller en pre-fail-verdi er under terskelen nå. Advarsel ved dårlige
/// sektorer, verdier som har vært under terskelen, feilet selvtest, lite
/// levetid igjen, høy temperatur eller NVMe-advarsler.
pub fn health(kind: &str, s: &Smart) -> Health {
    use crate::fmt as F;
    if !s.supported || s.source.is_empty() {
        return Health::default();
    }
    let mut fail: Vec<String> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    if s.failing {
        fail.push("Disken melder at den feiler (SMART-status)".into());
    }
    let failing_now: Vec<&SmartAttr> = s.attributes.iter().filter(|a| a.failing()).collect();
    for a in &failing_now {
        let t = format!("{} er under terskelen ({} ≤ {})", attr_label(a.id, &a.name), a.value, a.threshold);
        if a.prefail() { fail.push(t) } else { warn.push(t) }
    }
    if failing_now.is_empty() && s.attrs_failing.unwrap_or(0) > 0 {
        fail.push(F::plural(s.attrs_failing.unwrap_or(0), "attributt under terskelen", "attributter under terskelen"));
    }
    // Dårlige sektorer: summen UDisks2 regner ut (05 + C5 + C6), ellers attributtene.
    let bad = s.bad_sectors.filter(|b| *b > 0).or_else(|| {
        let n: i64 = s.attributes.iter().filter(|a| BAD_SECTOR_ATTRS.contains(&a.id)).filter_map(|a| a.count()).filter(|c| *c > 0).sum();
        (n > 0).then_some(n)
    });
    if let Some(b) = bad {
        warn.push(F::plural(b, "dårlig sektor", "dårlige sektorer"));
    }
    let past: Vec<&SmartAttr> = s.attributes.iter().filter(|a| !a.failing() && a.failed_past()).collect();
    if !past.is_empty() {
        warn.push(format!("Har vært under terskelen: {}", past.iter().map(|a| attr_label(a.id, &a.name)).collect::<Vec<_>>().join(", ")));
    } else if s.attrs_failed_past.unwrap_or(0) > 0 {
        warn.push(F::plural(s.attrs_failed_past.unwrap_or(0), "attributt har vært under terskelen", "attributter har vært under terskelen"));
    }
    if let Some(st) = s.selftest.as_deref()
        && (st == "fatal" || st.starts_with("error"))
    {
        warn.push(format!("Siste selvtest feilet ({})", selftest_text(st).to_lowercase()));
    }
    if let Some(l) = s.life_left {
        if l <= 3.0 {
            fail.push(format!("{} levetid igjen", F::pct(Some(l), 0)));
        } else if l <= 10.0 {
            warn.push(format!("{} levetid igjen", F::pct(Some(l), 0)));
        }
    }
    if let Some(t) = s.temp {
        let (w, f) = temp_limits(kind);
        if t >= f {
            fail.push(format!("{} er for varmt", F::temp(Some(t), 0)));
        } else if t >= w {
            warn.push(format!("{} er varmt", F::temp(Some(t), 0)));
        }
    }
    if let Some(n) = &s.nvme {
        for w in &n.critical_warning {
            let (t, hard) = match w.as_str() {
                "spare" => ("Lite reservekapasitet igjen", true),
                "temperature" => ("Over temperaturgrensen", false),
                "degraded" => ("Påliteligheten er redusert", true),
                "readonly" => ("Disken er satt skrivebeskyttet", true),
                "volatile_mem" => ("Reservestrømmen for hurtigminnet har feilet", false),
                "pmr_readonly" => ("PMR er skrivebeskyttet", false),
                _ => ("Kritisk advarsel fra kontrolleren", false),
            };
            if hard { fail.push(t.into()) } else { warn.push(t.into()) }
        }
        if let Some(m) = n.media_errors.filter(|m| *m > 0.0) {
            warn.push(F::plural(m as i64, "mediefeil", "mediefeil"));
        }
    }
    let level = if !fail.is_empty() {
        Level::Fail
    } else if !warn.is_empty() {
        Level::Warn
    } else {
        Level::Ok
    };
    fail.extend(warn);
    Health { level, reasons: fail }
}

/// UDisks2-statusen for siste selvtest på norsk.
pub fn selftest_text(s: &str) -> String {
    match s {
        "success" => "Fullført uten feil",
        "inprogress" => "Pågår",
        "aborted" => "Avbrutt",
        "interrupted" => "Avbrutt (tilbakestilt)",
        "fatal" => "Feilet",
        "error_unknown" => "Feilet (ukjent del)",
        "error_electrical" => "Feilet (elektrisk)",
        "error_servo" => "Feilet (servo)",
        "error_read" => "Feilet (lesing)",
        "error_handling" => "Feilet (håndtering)",
        "" => "Aldri kjørt",
        other => other,
    }
    .to_string()
}

/// Fyller ut de avledede feltene etter at attributtene er lest: levetid,
/// skrevet og lest, strømsykluser og helse.
pub fn finish(d: &mut Disk) {
    let s = &mut d.smart;
    if !s.attributes.is_empty() {
        if s.life_left.is_none() && d.kind != "HDD" {
            s.life_left = life_left(&s.attributes);
        }
        let (w, r) = written_read(&s.attributes, &d.model, d.size);
        s.written = s.written.or(w);
        s.read = s.read.or(r);
        if s.power_cycles.is_none() {
            s.power_cycles = s.attributes.iter().find(|a| a.id == 12).map(|a| a.pretty as f64);
        }
    }
    d.health = health(&d.kind, &d.smart);
}

/// Gjør en tekst om til en gyldig del av en sensornøkkel.
pub fn key_part(s: &str) -> String {
    let t: String = s.trim().chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).collect();
    t.chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn a(id: u8, name: &str, flags: u16, v: i32, w: i32, th: i32, pretty: i64, unit: i32) -> SmartAttr {
        SmartAttr { id, name: name.into(), flags, value: v, worst: w, threshold: th, pretty, unit }
    }

    #[test]
    fn skrevet_totalt_med_riktig_enhet() {
        // Fra brukerens maskin (UDisks2, november 2026).
        let samsung = [a(241, "total-lbas-written", 50, 99, 99, 0, 555_844_507_837, 7)];
        let (w, _) = written_read(&samsung, "Samsung SSD 870 QVO 1TB", Some(1.0e12));
        let w = w.unwrap();
        assert!((w / 1e12 - 8.48).abs() < 0.01, "Samsung teller LBA-er: {w}");
        let ts = [a(241, "total-lbas-written", 0, 100, 100, 0, 38_193_869, 7), a(242, "total-lbas-read", 0, 100, 100, 0, 32_070_319, 7)];
        let (w, r) = written_read(&ts, "TS128GSSD370", Some(128e9));
        assert!((w.unwrap() / 1e12 - 38.19).abs() < 0.02, "Transcend teller 32 MiB");
        assert!(r.unwrap() > 30e12);
        let st = [a(241, "total-lbas-written", 0, 100, 253, 0, 100_187_617_735, 7)];
        let (w, _) = written_read(&st, "ST2000DM008-2FR102", Some(2e12));
        assert!((w.unwrap() / 1e12 - 1.53).abs() < 0.01);
        // Ukjent modell: det rimelige valget.
        assert_eq!(lba_unit_bytes("Ukjent", 1.0e10, Some(1e12)), 512.0);
        assert_eq!(lba_unit_bytes("Ukjent", 1.0e6, Some(128e9)), 32.0 * 1024.0 * 1024.0);
    }

    #[test]
    fn tolket_verdi_og_navn() {
        assert_eq!(attr_text(&a(9, "power-on-hours", 50, 99, 99, 0, 11_678_400_000, 2), "", None), "3\u{202F}244\u{202F}t");
        assert_eq!(attr_text(&a(190, "airflow-temperature-celsius", 50, 76, 59, 0, 297_150, 4), "", None), "24\u{202F}°C");
        assert_eq!(attr_text(&a(5, "reallocated-sector-count", 51, 100, 100, 10, 0, 3), "", None), "0 sektorer");
        assert_eq!(attr_label(5, "reallocated-sector-count"), "Omallokerte sektorer");
        assert_eq!(attr_label(245, "attribute-245"), "Attributt 245");
        assert_eq!(attr_label(160, "uncorrectable-sector-count"), "Uncorrectable sector count");
        assert_eq!(disk_size_label(1_000_204_886_016.0), "1\u{202F}TB");
        assert_eq!(disk_size_label(4_000_787_030_016.0), "4\u{202F}TB");
        assert_eq!(disk_size_label(128_035_676_160.0), "128\u{202F}GB");
        assert_eq!(key_part("S5SVNF0NC41586R"), "S5SVNF0NC41586R");
        assert_eq!(key_part("WD 12/3"), "WD_12_3");
    }

    fn smart(attrs: Vec<SmartAttr>) -> Smart {
        Smart { source: "udisks".into(), supported: true, enabled: true, temp: Some(30.0), attributes: attrs, ..Default::default() }
    }

    #[test]
    fn helse_som_crystaldiskinfo() {
        let ok = smart(vec![a(5, "reallocated-sector-count", 51, 100, 100, 10, 0, 3), a(177, "wear-leveling-count", 19, 98, 98, 0, 18, 1)]);
        assert_eq!(health("SSD", &ok).level, Level::Ok);
        // Omallokerte sektorer: advarsel.
        let mut s = smart(vec![a(5, "reallocated-sector-count", 51, 100, 100, 10, 8, 3)]);
        let h = health("HDD", &s);
        assert_eq!(h.level, Level::Warn);
        assert_eq!(h.reasons, vec!["8 dårlige sektorer"]);
        // UDisks2 sin sum går foran attributtene.
        s.bad_sectors = Some(12);
        assert_eq!(health("HDD", &s).reasons[0], "12 dårlige sektorer");
        // Pre-fail under terskelen: feil.
        let f = smart(vec![a(5, "reallocated-sector-count", 51, 9, 9, 10, 4000, 3)]);
        assert_eq!(health("HDD", &f).level, Level::Fail);
        // Disken sier selv at den feiler.
        let mut x = smart(vec![]);
        x.failing = true;
        assert_eq!(health("SSD", &x).level, Level::Fail);
        // Har vært under terskelen (worst), ikke nå: advarsel.
        let p = smart(vec![a(190, "airflow-temperature-celsius", 34, 65, 38, 40, 308_150, 4)]);
        let h = health("HDD", &p);
        assert_eq!(h.level, Level::Warn, "{h:?}");
        assert!(h.reasons[0].starts_with("Har vært under terskelen"));
        // Varm HDD.
        let mut t = smart(vec![]);
        t.temp = Some(58.0);
        assert_eq!(health("HDD", &t).level, Level::Warn);
        assert_eq!(health("SSD", &t).level, Level::Ok);
        // Feilet selvtest og lite levetid.
        let mut st = smart(vec![]);
        st.selftest = Some("error_read".into());
        st.life_left = Some(7.0);
        let h = health("SSD", &st);
        assert_eq!(h.level, Level::Warn);
        assert_eq!(h.reasons.len(), 2);
        // Uten SMART: ukjent.
        assert_eq!(health("SSD", &Smart::default()).level, Level::Unknown);
        // NVMe: lite reserve er feil.
        let mut n = smart(vec![]);
        n.nvme = Some(NvmeHealth { critical_warning: vec!["spare".into()], ..Default::default() });
        assert_eq!(health("NVMe", &n).level, Level::Fail);
    }

    #[test]
    fn levetid_og_avledede_felt() {
        let attrs = vec![
            a(12, "power-cycle-count", 50, 99, 99, 0, 616, 1),
            a(177, "wear-leveling-count", 19, 98, 98, 0, 18, 1),
            a(241, "total-lbas-written", 50, 99, 99, 0, 555_844_507_837, 7),
        ];
        assert_eq!(life_left(&attrs), Some(98.0));
        let mut d = Disk { model: "Samsung SSD 870 QVO 1TB".into(), kind: "SSD".into(), size: Some(1e12), smart: smart(attrs), ..Default::default() };
        finish(&mut d);
        assert_eq!(d.smart.life_left, Some(98.0));
        assert_eq!(d.smart.power_cycles, Some(616.0));
        assert!(d.smart.written.is_some());
        assert_eq!(d.health.level, Level::Ok);
        let p = Partition { used: Some(30.0), avail: Some(70.0), ..Default::default() };
        assert_eq!(p.used_pct(), Some(30.0));
    }
}
