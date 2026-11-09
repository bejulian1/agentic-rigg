//! Logging til CSV (produktspesifikasjonen 3.2), kjørt av tjenesten så den
//! fortsetter når vinduet lukkes.
//!
//! Grunnmuren har registrert operasjonene `log_start`, `log_stop` og
//! `log_status` i `ops.rs`, og kaller [`Logger::on_sample`] for hver prøve og
//! [`Logger::tick`] etter hver runde. Mens [`Logger::active`] er sann, holder
//! tjenesten seg i live.
//!
//! - Fila heter `rigg-2026-11-07_14-32-10.csv` og ligger i den valgte mappa
//!   (standard `~/Dokumenter/Agentic Rigg/`, fra XDG-mappa for dokumenter).
//! - Kolonnene låses ved første prøve etter start: alle sensorene som finnes
//!   da, eller de som er valgt. Forsvinner en sensor, står feltet tomt. Nye
//!   sensorer tas med i neste logg.
//! - Skrives med buffer, som tømmes hvert 5. s og ved stopp. Ny fil
//!   (`…-del2.csv`) ved 100 MB. Stopper med en melding når disken har under
//!   500 MB ledig.

use rigg_core::csv::{self, Column, CsvFormat};
use rigg_core::model::Telemetry;
use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use rigg_core::settings::{LOG_INTERVALS, LogSettings};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Ny fil når den passerer denne størrelsen.
pub const PART_BYTES: u64 = 100 * 1024 * 1024;
/// Under dette stopper loggen.
pub const MIN_FREE_BYTES: u64 = 500 * 1024 * 1024;
/// Bufferen tømmes så ofte.
pub const FLUSH_MS: i64 = 5_000;

/// Tidssonen akkurat nå (sekunder fra UTC), fra systemets `localtime`.
pub fn utc_offset(ms: i64) -> i64 {
    let t: libc::time_t = (ms / 1000) as libc::time_t;
    // SAFETY: localtime_r skriver bare i `tm`, som vi eier.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff
}

/// Ledig plass for vanlige brukere på fila sitt filsystem.
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fyller bare strukturen vi eier.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some(s.f_bavail.saturating_mul(s.f_frsize))
}

/// `~/Dokumenter/Agentic Rigg/` (XDG-mappa for dokumenter, med det navnet systemet bruker).
pub fn default_dir() -> PathBuf {
    let docs = glib::user_special_dir(glib::UserDirectory::Documents).unwrap_or_else(|| glib::home_dir().join("Dokumenter"));
    docs.join("Agentic Rigg")
}

struct Active {
    dir: PathBuf,
    /// Tiden filnavnet bygger på (start), og delen vi er på.
    name_t: rigg_core::fmt::LocalTime,
    part: u32,
    path: PathBuf,
    w: std::io::BufWriter<std::fs::File>,
    file_bytes: u64,
    bytes: u64,
    rows: u64,
    started_at: i64,
    interval_ms: u32,
    format: CsvFormat,
    /// `None`: alle sensorene som finnes ved første prøve.
    wanted: Option<Vec<String>>,
    cols: Option<Vec<Column>>,
    last_row: i64,
    last_flush: i64,
    last_space: i64,
}

#[derive(Default)]
pub struct Logger {
    on: Option<Active>,
    /// Den siste loggen etter stopp (fil, rader, størrelse, eventuell feil).
    last: Option<Value>,
}

fn invalid(m: &str) -> ProtoError {
    ProtoError::msg(ErrorCode::InvalidArgument, m)
}

impl Logger {
    pub fn new() -> Logger {
        Logger::default()
    }

    /// `log_start {keys?, intervalMs?, dir?, format?}`. Gir `log_status`.
    /// Det som mangler i `args`, kommer fra `tjeneste.json` (`log`).
    pub fn start(&mut self, args: &Value, settings: &LogSettings, now: i64) -> OpResult {
        if self.on.is_some() {
            return Ok(self.status());
        }
        let wanted = match args.get("keys").filter(|v| !v.is_null()) {
            Some(v) => {
                let a = v.as_array().ok_or_else(|| invalid("keys må være en liste"))?;
                let mut out: Vec<String> = Vec::new();
                for k in a.iter().take(512) {
                    let s = k.as_str().filter(|s| rigg_core::sensor_key::valid(s)).ok_or_else(|| invalid("Ugyldig sensornøkkel"))?;
                    if !out.iter().any(|x| x == s) {
                        out.push(s.to_string());
                    }
                }
                if out.is_empty() {
                    return Err(invalid("Velg minst én sensor å logge."));
                }
                Some(out)
            }
            None if !settings.all => {
                if settings.keys.is_empty() {
                    return Err(invalid("Ingen sensorer er valgt for loggen. Velg dem i Innstillinger › Logging og varsler, eller logg alle."));
                }
                Some(settings.keys.clone())
            }
            None => None,
        };
        let interval_ms = match args.get("intervalMs").filter(|v| !v.is_null()) {
            Some(v) => v.as_u64().map(|n| n as u32).filter(|n| LOG_INTERVALS.contains(n)).ok_or_else(|| invalid("Loggintervallet må være 0,5, 1, 2, 5 eller 10 s"))?,
            None => settings.interval_ms,
        };
        let format = match args.get("format").and_then(Value::as_str) {
            Some("standard") => CsvFormat::Standard,
            Some("norsk") => CsvFormat::Norsk,
            Some(_) => return Err(invalid("Formatet må være standard eller norsk")),
            None => CsvFormat::from_name(&settings.format),
        };
        let dir = match args.get("dir").filter(|v| !v.is_null()) {
            Some(v) => {
                let s = v.as_str().filter(|s| s.starts_with('/') && s.len() < 1024 && !s.contains('\0')).ok_or_else(|| invalid("Loggmappa må være en full sti"))?;
                PathBuf::from(s)
            }
            None => settings.dir.as_ref().map(PathBuf::from).unwrap_or_else(default_dir),
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return Err(ProtoError::msg(ErrorCode::Internal, format!("Kunne ikke lage mappa {}: {e}", dir.display())));
        }
        if free_bytes(&dir).is_some_and(|f| f < MIN_FREE_BYTES) {
            return Err(ProtoError::msg(ErrorCode::Busy, "Det er under 500 MB ledig på disken. Loggen er ikke startet."));
        }
        let name_t = rigg_core::fmt::local(now, utc_offset(now));
        let (path, file) = open_part(&dir, name_t, 1).map_err(|e| ProtoError::msg(ErrorCode::Internal, format!("Kunne ikke lage loggfila: {e}")))?;
        self.on = Some(Active {
            dir,
            name_t,
            part: 1,
            path,
            w: std::io::BufWriter::with_capacity(64 * 1024, file),
            file_bytes: 0,
            bytes: 0,
            rows: 0,
            started_at: now,
            interval_ms,
            format,
            wanted,
            cols: None,
            last_row: i64::MIN / 2,
            last_flush: now,
            last_space: now,
        });
        self.last = None;
        Ok(self.status())
    }

    pub fn stop(&mut self, now: i64) -> OpResult {
        self.finish(now, None);
        Ok(self.status())
    }

    /// Lukker loggen og husker hvordan den endte.
    fn finish(&mut self, now: i64, error: Option<String>) {
        if let Some(mut a) = self.on.take() {
            let flush_err = a.w.flush().err();
            let error = error.or_else(|| flush_err.map(|e| format!("Kunne ikke skrive til loggfila: {e}")));
            self.last = Some(json!({
                "active": false,
                "file": a.path.display().to_string(),
                "dir": a.dir.display().to_string(),
                "rows": a.rows,
                "bytes": a.bytes,
                "startedAt": a.started_at,
                "stoppedAt": now,
                "intervalMs": a.interval_ms,
                "format": a.format.name(),
                "columns": a.cols.as_ref().map(|c| c.len()).unwrap_or(0),
                "part": a.part,
                "error": error,
            }));
        }
    }

    /// `{active, file, rows, bytes, startedAt}`, og `dir`, `intervalMs`,
    /// `format`, `columns`, `part`. Etter stopp: den siste loggen med `active: false`.
    pub fn status(&self) -> Value {
        match &self.on {
            Some(a) => json!({
                "active": true,
                "file": a.path.display().to_string(),
                "dir": a.dir.display().to_string(),
                "rows": a.rows,
                "bytes": a.bytes,
                "startedAt": a.started_at,
                "intervalMs": a.interval_ms,
                "format": a.format.name(),
                "columns": a.cols.as_ref().map(|c| c.len()).unwrap_or(0),
                "part": a.part,
            }),
            None => self.last.clone().unwrap_or_else(|| json!({"active": false, "file": null, "rows": 0, "bytes": 0, "startedAt": null})),
        }
    }

    pub fn active(&self) -> bool {
        self.on.is_some()
    }

    /// Loggintervallet mens loggen går (takten må være minst så rask).
    pub fn interval_ms(&self) -> Option<u32> {
        self.on.as_ref().map(|a| a.interval_ms)
    }

    /// Én prøve. Gir en `log`-hendelse ved overganger (ny fil, stopp med feil).
    pub fn on_sample(&mut self, t: &Telemetry, flat: &[(String, f64)], now: i64) -> Option<Value> {
        let a = self.on.as_mut()?;
        // Litt slakk, så en takt på 1 s med små forsinkelser ikke hopper over rader.
        let slack = (a.interval_ms as i64 / 10).min(200);
        if now - a.last_row < a.interval_ms as i64 - slack {
            return None;
        }
        if a.cols.is_none() {
            let keys: Vec<String> = match &a.wanted {
                Some(w) => w.clone(),
                None => flat.iter().map(|(k, _)| k.clone()).collect(),
            };
            let cols: Vec<Column> = keys.iter().map(|k| csv::column(k, Some(t))).collect();
            let head = csv::header_line(&cols, a.format);
            if let Err(e) = a.write(head.as_bytes()) {
                self.finish(now, Some(format!("Kunne ikke skrive til loggfila: {e}")));
                return Some(self.status());
            }
            a.cols = Some(cols);
        }
        let map: HashMap<&str, f64> = flat.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let cols = a.cols.as_ref().unwrap();
        let values: Vec<Option<f64>> = cols.iter().map(|c| map.get(c.key.as_str()).copied()).collect();
        let time = csv::iso_time(t.t, utc_offset(t.t), a.interval_ms < 1000);
        let secs = (t.t - a.started_at) as f64 / 1000.0;
        let line = csv::row_line(&time, secs.max(0.0), cols, &values, a.format);
        if let Err(e) = a.write(line.as_bytes()) {
            self.finish(now, Some(format!("Kunne ikke skrive til loggfila: {e}")));
            return Some(self.status());
        }
        a.rows += 1;
        a.last_row = now;
        if a.file_bytes >= PART_BYTES {
            return match a.next_part() {
                Ok(()) => Some(self.status()),
                Err(e) => {
                    self.finish(now, Some(format!("Kunne ikke lage neste loggfil: {e}")));
                    Some(self.status())
                }
            };
        }
        None
    }

    /// Etter hver runde: tøm bufferen hvert 5. s, og sjekk ledig plass.
    pub fn tick(&mut self, now: i64) -> Option<Value> {
        let a = self.on.as_mut()?;
        if now - a.last_flush >= FLUSH_MS {
            a.last_flush = now;
            if let Err(e) = a.w.flush() {
                self.finish(now, Some(format!("Kunne ikke skrive til loggfila: {e}")));
                return Some(self.status());
            }
        }
        if now - a.last_space >= FLUSH_MS {
            a.last_space = now;
            if free_bytes(&a.dir).is_some_and(|f| f < MIN_FREE_BYTES) {
                self.finish(now, Some("Loggen er stoppet: det er under 500 MB ledig på disken.".into()));
                return Some(self.status());
            }
        }
        None
    }

    /// Ved stopp av tjenesten.
    pub fn close(&mut self) {
        self.finish(rigg_core::now_ms(), None);
    }
}

/// Lager del `part` uten å skrive over noe som finnes fra før.
fn open_part(dir: &Path, t: rigg_core::fmt::LocalTime, part: u32) -> std::io::Result<(PathBuf, std::fs::File)> {
    let path = dir.join(csv::file_name(t, part));
    let f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    Ok((path, f))
}

impl Active {
    fn write(&mut self, b: &[u8]) -> std::io::Result<()> {
        self.w.write_all(b)?;
        self.file_bytes += b.len() as u64;
        self.bytes += b.len() as u64;
        Ok(())
    }

    /// Ny fil (`…-del2.csv`) med samme overskrift.
    fn next_part(&mut self) -> std::io::Result<()> {
        self.w.flush()?;
        let (path, file) = open_part(&self.dir, self.name_t, self.part + 1)?;
        self.part += 1;
        self.path = path;
        self.w = std::io::BufWriter::with_capacity(64 * 1024, file);
        self.file_bytes = 0;
        if let Some(cols) = &self.cols {
            let head = csv::header_line(cols, self.format);
            self.write(head.as_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::model::GpuSample;

    fn tele(t: i64, temp: Option<f64>) -> Telemetry {
        let mut x = Telemetry { t, ..Default::default() };
        x.gpus.insert("0000:1c:00.0".into(), GpuSample { temp, ..Default::default() });
        x.sys.cpu.load = Some(5.5);
        x
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("riggd-logg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn kolonnene_ligger_fast_og_tomme_felt_står_tomme() {
        let dir = tmp("fast");
        let mut l = Logger::new();
        let s = LogSettings::default();
        let t0 = 1_794_058_330_000;
        let st = l.start(&json!({"dir": dir.display().to_string(), "format": "norsk", "intervalMs": 1000}), &s, t0).unwrap();
        assert_eq!(st["active"], true);
        assert!(l.active());
        for (i, temp) in [Some(40.0), None, Some(42.0)].into_iter().enumerate() {
            let t = tele(t0 + i as i64 * 1000, temp);
            let flat = rigg_core::sensor_key::flatten(&t);
            assert!(l.on_sample(&t, &flat, t.t).is_none());
        }
        // En prøve for tidlig (0,3 s) gir ingen rad med intervall 1 s.
        let t = tele(t0 + 2300, Some(50.0));
        l.on_sample(&t, &rigg_core::sensor_key::flatten(&t), t.t);
        let st = l.stop(t0 + 3000).unwrap();
        assert_eq!(st["active"], false);
        assert_eq!(st["rows"], 3);
        let text = std::fs::read_to_string(st["file"].as_str().unwrap()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("Tid;Sekunder;GPU · Temperatur [°C];"), "{}", lines[0]);
        let n = lines[0].split(';').count();
        assert!(lines.iter().all(|l| l.split(';').count() == n), "fast antall kolonner");
        assert!(lines[1].starts_with("2026-11-07T14:32:10+01:00;0,0;40;") || lines[1].contains(";0,0;40;"), "{}", lines[1]);
        assert!(lines[2].contains(";1,0;;"), "tomt felt når temperaturen mangler: {}", lines[2]);
        assert!(lines[3].contains(";5,5"), "{}", lines[3]);
        assert!(st["file"].as_str().unwrap().ends_with(".csv"));
        assert_eq!(st["bytes"].as_u64().unwrap(), text.len() as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn utvalgte_sensorer_og_feil() {
        let dir = tmp("utvalg");
        let mut l = Logger::new();
        let s = LogSettings { all: false, keys: vec![], ..LogSettings::default() };
        assert!(l.start(&json!({"dir": dir.display().to_string()}), &s, 0).is_err(), "ingen sensorer valgt");
        assert!(l.start(&json!({"dir": "relativ"}), &LogSettings::default(), 0).is_err());
        assert!(l.start(&json!({"dir": dir.display().to_string(), "intervalMs": 3000}), &LogSettings::default(), 0).is_err());
        let st = l.start(&json!({"dir": dir.display().to_string(), "keys": ["cpu/load", "gpu:0000:1c:00.0/temp"]}), &LogSettings::default(), 1000).unwrap();
        assert_eq!(st["format"], "standard");
        let t = tele(2000, Some(41.0));
        l.on_sample(&t, &rigg_core::sensor_key::flatten(&t), 2000);
        let st = l.stop(3000).unwrap();
        let text = std::fs::read_to_string(st["file"].as_str().unwrap()).unwrap();
        assert_eq!(text.lines().next().unwrap(), "Tid,Sekunder,Prosessor · total last [%],GPU · Temperatur [°C]");
        assert!(text.lines().nth(1).unwrap().ends_with(",1.0,5.5,41"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ny_del_ved_grensen() {
        let dir = tmp("del");
        let mut l = Logger::new();
        l.start(&json!({"dir": dir.display().to_string(), "intervalMs": 500}), &LogSettings::default(), 0).unwrap();
        let t = tele(1000, Some(40.0));
        l.on_sample(&t, &rigg_core::sensor_key::flatten(&t), 1000);
        l.on.as_mut().unwrap().file_bytes = PART_BYTES;
        let t = tele(2000, Some(40.0));
        let ev = l.on_sample(&t, &rigg_core::sensor_key::flatten(&t), 2000).expect("hendelse ved ny fil");
        assert_eq!(ev["part"], 2);
        assert!(ev["file"].as_str().unwrap().ends_with("-del2.csv"));
        let st = l.stop(3000).unwrap();
        let text = std::fs::read_to_string(st["file"].as_str().unwrap()).unwrap();
        assert!(text.starts_with("Tid,Sekunder,"), "ny del har overskrift");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
