//! Formatering av tall og enheter (port av `web/js/fmt.js`). Regler:
//!
//! - Smalt, ikke-brytende mellomrom (U+202F) mellom tall og enhet, også «38 %».
//! - Desimaltegn og siffergruppering følger visningsspråket.
//! - Ekte minustegn (U+2212), og aldri «−0».
//! - Enheter og desimaler følger Innstillinger ([`set_format`]).
//!
//! Valgene ligger i en trådlokal variabel, som demoens modul-variabel, så
//! sidene kan kalle `fmt::temp(v, 0)` uten å sende innstillingene rundt.

use crate::i18n::{self, Language};
use std::cell::RefCell;

pub const NNBSP: char = '\u{202F}';
pub const MINUS: char = '\u{2212}';
pub const DASH: &str = "–";

#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub unit: String,
    pub decimals: String,
    pub mem_unit: String,
    pub net_unit: String,
    pub clock: String,
}

impl Default for Format {
    fn default() -> Self {
        Format { unit: "C".into(), decimals: "auto".into(), mem_unit: "bin".into(), net_unit: "byte".into(), clock: "system".into() }
    }
}

impl Format {
    pub fn from_settings(s: &crate::settings::Settings) -> Format {
        Format { unit: s.unit.clone(), decimals: s.decimals.clone(), mem_unit: s.mem_unit.clone(), net_unit: s.net_unit.clone(), clock: s.clock.clone() }
    }
}

thread_local! {
    static P: RefCell<Format> = RefCell::new(Format::default());
}

pub fn set_format(f: Format) {
    P.with(|p| *p.borrow_mut() = f);
}

pub fn format() -> Format {
    P.with(|p| p.borrow().clone())
}

fn with<R>(f: impl FnOnce(&Format) -> R) -> R {
    P.with(|p| f(&p.borrow()))
}

/// Tall med fast antall desimaler. `None`/NaN gir «–».
pub fn num(v: Option<f64>, d: usize) -> String {
    localized_num(v, d, i18n::active())
}

fn localized_num(v: Option<f64>, d: usize, lang: Language) -> String {
    let Some(mut v) = v.filter(|v| v.is_finite()) else { return DASH.into() };
    if v.abs() < 0.5 * 10f64.powi(-(d as i32)) {
        v = 0.0;
    }
    let neg = v < 0.0;
    let s = format!("{:.*}", d, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s.as_str(), None),
    };
    let (decimal, group, min_digits) = match lang {
        Language::En | Language::ZhCn | Language::Ja => ('.', ',', 4),
        Language::De | Language::PtBr | Language::It => (',', '.', 4),
        Language::Es => (',', '.', 5),
        Language::Pl => (',', NNBSP, 5),
        Language::Fr | Language::Nb => (',', NNBSP, 4),
    };
    let mut out = String::with_capacity(s.len());
    if neg {
        out.push(MINUS);
    }
    for (i, c) in int.bytes().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) && int.len() >= min_digits {
            out.push(group);
        }
        out.push(char::from(c));
    }
    if let Some(f) = frac {
        out.push(decimal);
        out.push_str(f);
    }
    out
}

pub fn with_unit(text: String, unit: &str) -> String {
    if text == DASH { text } else { format!("{text}{NNBSP}{unit}") }
}

/// Desimaler: «auto» gir standarden for stedet, ellers brukerens valg.
pub fn dec(auto: usize) -> usize {
    with(|p| p.decimals.parse().unwrap_or(auto))
}

pub fn signed(v: f64, d: usize) -> String {
    if v > 0.0 {
        format!("+{}", num(Some(v), d))
    } else if v < 0.0 {
        format!("{MINUS}{}", num(Some(-v), d))
    } else {
        num(Some(0.0), d)
    }
}

// ---- Temperatur (lagres alltid i °C) ----

pub fn fahrenheit() -> bool {
    with(|p| p.unit == "F")
}

pub fn temp_out(c: Option<f64>) -> Option<f64> {
    c.map(|c| if fahrenheit() { c * 9.0 / 5.0 + 32.0 } else { c })
}

pub fn temp_in(v: f64) -> f64 {
    if fahrenheit() { (v - 32.0) * 5.0 / 9.0 } else { v }
}

pub fn temp_label() -> &'static str {
    if fahrenheit() { "°F" } else { "°C" }
}

pub fn temp_num(c: Option<f64>, d: usize) -> String {
    num(temp_out(c), d)
}

pub fn temp(c: Option<f64>, d: usize) -> String {
    with_unit(num(temp_out(c), d), temp_label())
}

/// Temperaturforskjell (ikke absolutt): 10 °C = 18 °F.
pub fn temp_delta(c: f64, d: usize) -> String {
    with_unit(num(Some(if fahrenheit() { c * 9.0 / 5.0 } else { c }), d), temp_label())
}

pub fn pct(v: Option<f64>, d: usize) -> String {
    with_unit(num(v, d), "%")
}

pub fn watt(v: Option<f64>, d: usize) -> String {
    with_unit(num(v, d), "W")
}

pub fn mhz(v: Option<f64>) -> String {
    with_unit(num(v, 0), "MHz")
}

pub fn rpm(v: Option<f64>) -> String {
    with_unit(num(v, 0), "o/min")
}

// ---- Minne ----

/// MiB i valgt system: binært (MiB/GiB) eller desimalt (MB/GB). `unit` tvinger en enhet.
pub fn mib(v: Option<f64>, d: Option<usize>, unit: Option<&str>) -> String {
    let Some(v) = v.filter(|v| v.is_finite()) else { return DASH.into() };
    if with(|p| p.mem_unit == "dec") {
        let mb = v * 1.048576;
        let u = unit.unwrap_or(if mb >= 10000.0 { "GB" } else { "MB" });
        if u == "GB" {
            return with_unit(num(Some(mb / 1000.0), d.unwrap_or(1)), "GB");
        }
        return with_unit(num(Some(mb), d.unwrap_or(0)), "MB");
    }
    let u = unit.unwrap_or(if v >= 10240.0 { "GiB" } else { "MiB" });
    if u == "GiB" {
        return with_unit(num(Some(v / 1024.0), d.unwrap_or(1)), "GiB");
    }
    with_unit(num(Some(v), d.unwrap_or(0)), "MiB")
}

/// Byte i valgt system, med enhet etter størrelse.
pub fn bytes(b: Option<f64>, d: usize) -> String {
    let Some(mut b) = b.filter(|v| v.is_finite()) else { return DASH.into() };
    let bin = with(|p| p.mem_unit != "dec");
    let base = if bin { 1024.0 } else { 1000.0 };
    let units: [&str; 5] = if bin { ["B", "KiB", "MiB", "GiB", "TiB"] } else { ["B", "kB", "MB", "GB", "TB"] };
    let mut i = 0;
    while b.abs() >= base && i < units.len() - 1 {
        b /= base;
        i += 1;
    }
    // Byte og MiB uten desimaler («123 MiB», ikke «123,50 MiB»).
    with_unit(num(Some(b), if i == 0 || i == 2 { 0 } else { d }), units[i])
}

/// «47 GiB» for et minnetall i kB.
pub fn kb(k: Option<f64>, d: usize) -> String {
    bytes(k.map(|k| k * 1024.0), d)
}

// ---- Nettrate ----

#[derive(Clone, Debug, PartialEq)]
pub struct RateUnit {
    pub div: f64,
    pub label: &'static str,
    pub d: usize,
}

/// Enheten ut fra største verdi raden har sett (byttes bare oppover).
pub fn rate_unit(max_bps: f64) -> RateUnit {
    let bit = with(|p| p.net_unit == "bit");
    let list: [(f64, &str, usize); 4] = if bit {
        [(1.0, "bit/s", 0), (1e3, "kbit/s", 0), (1e6, "Mbit/s", 1), (1e9, "Gbit/s", 2)]
    } else {
        [(1.0, "B/s", 0), (1e3, "kB/s", 1), (1e6, "MB/s", 2), (1e9, "GB/s", 2)]
    };
    let m = if bit { max_bps * 8.0 } else { max_bps };
    let mut u = list[1];
    for x in list {
        if m >= x.0 * 0.95 {
            u = x;
        }
    }
    RateUnit { div: u.0 / if bit { 8.0 } else { 1.0 }, label: u.1, d: u.2 }
}

pub fn rate(bps: Option<f64>, unit: Option<&RateUnit>) -> String {
    let Some(v) = bps.filter(|v| v.is_finite()) else { return DASH.into() };
    let u = unit.cloned().unwrap_or_else(|| rate_unit(v));
    with_unit(num(Some(v / u.div), u.d), u.label)
}

// ---- Tid ----

/// Lokal tid fra ms siden 1970. Uten tidssonedatabase brukes forskyvningen fra
/// `/etc/localtime` via libc i vinduet; her tar vi den som argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Dag, måned og år fra dager siden 1970 (proleptisk gregoriansk).
pub fn civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Lokal tid fra ms og forskyvningen fra UTC i sekunder.
pub fn local(ms: i64, utc_offset_s: i64) -> LocalTime {
    let s = ms.div_euclid(1000) + utc_offset_s;
    let days = s.div_euclid(86_400);
    let rem = s.rem_euclid(86_400);
    let (year, month, day) = civil(days);
    LocalTime { year, month, day, hour: (rem / 3600) as u32, minute: (rem % 3600 / 60) as u32, second: (rem % 60) as u32 }
}

/// «14:32:10» eller «2:32:10 PM», avhengig av språk og klokkevalg.
pub fn time(t: LocalTime, seconds: bool) -> String {
    let lang = i18n::active();
    let hour12 = with(|p| p.clock == "12" || (p.clock == "system" && matches!(lang, Language::En | Language::ZhCn)));
    localized_time(t, seconds, lang, hour12)
}

fn localized_time(t: LocalTime, seconds: bool, lang: Language, hour12: bool) -> String {
    if !hour12 {
        return if seconds { format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second) } else { format!("{:02}:{:02}", t.hour, t.minute) };
    }
    let hour = if t.hour.is_multiple_of(12) { 12 } else { t.hour % 12 };
    let pm = t.hour >= 12;
    let mut clock = if lang == Language::Nb {
        format!("{hour:02}:{:02}", t.minute)
    } else {
        format!("{hour}:{:02}", t.minute)
    };
    if seconds {
        use std::fmt::Write;
        let _ = write!(clock, ":{:02}", t.second);
    }
    match (lang, pm) {
        (Language::ZhCn, false) => format!("上午{clock}"),
        (Language::ZhCn, true) => format!("下午{clock}"),
        (Language::Ja, false) => format!("午前{clock}"),
        (Language::Ja, true) => format!("午後{clock}"),
        (Language::Nb, false) => format!("{clock} a.m."),
        (Language::Nb, true) => format!("{clock} p.m."),
        (Language::Es, false) => format!("{clock} a. m."),
        (Language::Es, true) => format!("{clock} p. m."),
        (_, false) => format!("{clock} AM"),
        (_, true) => format!("{clock} PM"),
    }
}

pub const MONTHS: [&str; 12] = ["januar", "februar", "mars", "april", "mai", "juni", "juli", "august", "september", "oktober", "november", "desember"];
pub const MON_SHORT: [&str; 12] = ["jan.", "feb.", "mars", "apr.", "mai", "juni", "juli", "aug.", "sep.", "okt.", "nov.", "des."];

/// Brukerrettet dato, med månedsnavn på bokmål.
pub fn date(year: i32, month: u32, day: u32, short: bool) -> String {
    date_parts(i18n::active(), Some(year), month, day, short)
}

pub fn month_day(month: u32, day: u32) -> String {
    date_parts(i18n::active(), None, month, day, true)
}

fn date_parts(lang: Language, year: Option<i32>, month: u32, day: u32, short: bool) -> String {
    let m = month.clamp(1, 12) as usize - 1;
    match (lang, year) {
        (Language::Nb, y) => {
            let name = if short { MON_SHORT[m] } else { MONTHS[m] };
            y.map_or_else(|| format!("{day}. {name}"), |y| format!("{day}. {name} {y}"))
        }
        (Language::En, y) => {
            let name = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][m];
            y.map_or_else(|| format!("{name} {day}"), |y| format!("{name} {day}, {y}"))
        }
        (Language::ZhCn | Language::Ja, Some(y)) => format!("{y}年{month}月{day}日"),
        (Language::ZhCn | Language::Ja, None) => format!("{month}月{day}日"),
        (Language::De | Language::Pl, Some(y)) => format!("{day:02}.{month:02}.{y}"),
        (Language::De | Language::Pl, None) => format!("{day:02}.{month:02}."),
        (Language::Es, Some(y)) => format!("{day}/{month}/{y}"),
        (Language::Es, None) => format!("{day}/{month}"),
        (_, Some(y)) => format!("{day:02}/{month:02}/{y}"),
        (_, None) => format!("{day:02}/{month:02}"),
    }
}

/// «23:37» i dag, «3. nov. 23:37» en annen dag og «3. nov. 2025 23:37» et annet år.
pub fn since(t: LocalTime, now: LocalTime) -> String {
    let hm = time(t, false);
    if (t.year, t.month, t.day) == (now.year, now.month, now.day) {
        return hm;
    }
    let date = date_parts(i18n::active(), (t.year != now.year).then_some(t.year), t.month, t.day, true);
    format!("{date} {hm}")
}

pub fn uptime(sec: Option<f64>) -> String {
    let Some(sec) = sec.filter(|s| s.is_finite()) else { return DASH.into() };
    let sec = sec as i64;
    let d = sec / 86_400;
    let h = sec % 86_400 / 3600;
    let m = sec % 3600 / 60;
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(crate::trf!("{verdi} døgn", verdi = d));
    }
    if d > 0 || h > 0 {
        parts.push(crate::trf!("{verdi} t", verdi = h));
    }
    parts.push(crate::trf!("{verdi} min", verdi = m));
    parts.join(" ")
}

/// «1 kjerne», «16 tråder».
pub fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{} {}", num(Some(n as f64), 0), if n == 1 { one } else { many })
}

/// Intervall i sekunder: «0,5 s», «1 s».
pub fn seconds(ms: u32) -> String {
    let s = ms as f64 / 1000.0;
    with_unit(num(Some(s), if ms.is_multiple_of(1000) { 0 } else { 1 }), "s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tall() {
        set_format(Format::default());
        assert_eq!(num(Some(1350.0), 0), "1\u{202F}350");
        assert_eq!(num(Some(999.0), 0), "999");
        assert_eq!(num(Some(-2.5), 1), "\u{2212}2,5");
        assert_eq!(num(Some(-0.01), 1), "0,0", "aldri −0");
        assert_eq!(num(None, 0), "–");
        assert_eq!(num(Some(f64::NAN), 0), "–");
        assert_eq!(pct(Some(38.0), 0), "38\u{202F}%");
        assert_eq!(temp(Some(37.375), 0), "37\u{202F}°C");
        assert_eq!(watt(Some(12.34), 1), "12,3\u{202F}W");
        assert_eq!(signed(5.0, 0), "+5");
        assert_eq!(seconds(500), "0,5\u{202F}s");
        assert_eq!(num(Some(1234567.0), 0), "1\u{202F}234\u{202F}567");
    }

    #[test]
    fn lokal_tall_og_tid() {
        let tests = [
            (Language::Nb, "12 345,6"),
            (Language::En, "12,345.6"),
            (Language::De, "12.345,6"),
            (Language::Fr, "12 345,6"),
            (Language::Es, "12.345,6"),
            (Language::PtBr, "12.345,6"),
            (Language::It, "12.345,6"),
            (Language::Pl, "12 345,6"),
            (Language::ZhCn, "12,345.6"),
            (Language::Ja, "12,345.6"),
        ];
        for (lang, value) in tests {
            assert_eq!(localized_num(Some(12345.6), 1, lang), value);
        }
        assert_eq!(localized_num(Some(1234.0), 0, Language::Pl), "1234");
        assert_eq!(date_parts(Language::En, Some(2026), 11, 7, true), "Nov 7, 2026");
        assert_eq!(date_parts(Language::Ja, Some(2026), 11, 7, true), "2026年11月7日");
        let t = LocalTime { year: 2026, month: 11, day: 7, hour: 14, minute: 5, second: 1 };
        assert_eq!(localized_time(t, false, Language::En, true), "2:05 PM");
        assert_eq!(localized_time(t, true, Language::ZhCn, true), "下午2:05:01");
        assert_eq!(localized_time(t, false, Language::Nb, true), "02:05 p.m.");
    }

    #[test]
    fn enheter() {
        set_format(Format { unit: "F".into(), ..Default::default() });
        assert_eq!(temp(Some(100.0), 0), "212\u{202F}°F");
        assert_eq!(temp_delta(10.0, 0), "18\u{202F}°F");
        assert!((temp_in(212.0) - 100.0).abs() < 1e-9);
        set_format(Format { mem_unit: "dec".into(), ..Default::default() });
        assert_eq!(mib(Some(8192.0), None, None), "8\u{202F}590\u{202F}MB");
        assert_eq!(bytes(Some(1500.0), 1), "1,5\u{202F}kB");
        assert_eq!(bytes(Some(129_500_000.0), 2), "130\u{202F}MB");
        set_format(Format::default());
        assert_eq!(mib(Some(8192.0), None, None), "8\u{202F}192\u{202F}MiB");
        assert_eq!(mib(Some(10240.0), None, None), "10,0\u{202F}GiB");
        assert_eq!(kb(Some(49_270_320.0), 1), "47,0\u{202F}GiB");
        assert_eq!(rate(Some(1_500_000.0), None), "1,50\u{202F}MB/s");
        set_format(Format { net_unit: "bit".into(), ..Default::default() });
        assert_eq!(rate(Some(1_500_000.0), None), "12,0\u{202F}Mbit/s");
        set_format(Format::default());
    }

    #[test]
    fn tid() {
        set_format(Format::default());
        // 7. nov. 2026 14:32:10 UTC+1
        let t = local(1_794_065_530_000, 3600);
        assert_eq!((t.year, t.month, t.day), (2026, 11, 7));
        assert_eq!(time(t, true), format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second));
        let earlier = local(1_794_065_530_000 - 4 * 86_400_000, 3600);
        assert!(since(earlier, t).starts_with("3. nov. "));
        assert_eq!(uptime(Some(90_061.0)), "1\u{202F}døgn 1\u{202F}t 1\u{202F}min");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_454), (2026, 1, 1));
    }
}
