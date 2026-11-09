//! Alle hwmon-enheter: `temp*`, `fan*`, `in*`, `power*` og `curr*`.
//! Enheter i sysfs: milligrader, o/min, millivolt, mikrowatt og milliampere.
//! Wi-Fi-kortet (`iwlwifi`) spør fastvaren ved hver lesing (60–90 µs), så det
//! leses hvert 5. s i stedet for hvert sekund (maskinvare-revisjon kap. 4).

use crate::PreadFile;
use rigg_core::model::{HwmonDevice, HwmonSensor};
use std::path::{Path, PathBuf};

struct Sensor {
    meta: HwmonSensor,
    file: Option<PreadFile>,
    scale: f64,
}

struct Dev {
    id: String,
    name: String,
    sensors: Vec<Sensor>,
    slow: bool,
    last: i64,
}

pub struct Hwmon {
    dir: PathBuf,
    devs: Vec<Dev>,
}

fn scale(kind: &str) -> f64 {
    match kind {
        "temp" | "in" | "curr" => 1000.0,
        "power" => 1_000_000.0,
        _ => 1.0,
    }
}

/// «temp2_input» → («temp», «2»).
fn split_input(f: &str) -> Option<(&'static str, &str)> {
    let base = f.strip_suffix("_input")?;
    for k in ["temp", "fan", "in", "power", "curr"] {
        if let Some(n) = base.strip_prefix(k)
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
        {
            return Some((k, n));
        }
    }
    None
}

impl Hwmon {
    pub fn new(root: &Path) -> Hwmon {
        Hwmon { dir: root.join("sys/class/hwmon"), devs: vec![] }
    }

    /// Finner enhetene (hvert 10. s). Enheter som allerede er åpne, beholdes.
    pub fn scan(&mut self) {
        let mut ids: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("hwmon")).collect())
            .unwrap_or_default();
        ids.sort_by_key(|n| n[5..].parse::<u32>().unwrap_or(u32::MAX));
        self.devs.retain(|d| ids.contains(&d.id));
        for id in ids {
            if self.devs.iter().any(|d| d.id == id) {
                continue;
            }
            let dir = self.dir.join(&id);
            let name = crate::read_text(&dir.join("name")).unwrap_or_else(|| id.clone());
            let mut files: Vec<String> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            files.sort();
            let mut sensors = Vec::new();
            for f in files {
                let Some((kind, n)) = split_input(&f) else { continue };
                let key = format!("{kind}{n}");
                let label = crate::read_text(&dir.join(format!("{key}_label"))).unwrap_or_else(|| key.clone());
                let (file, err) = match PreadFile::open(&dir.join(&f)) {
                    Ok(p) => (Some(p), None),
                    Err(e) => (None, Some(crate::read_err(&e))),
                };
                sensors.push(Sensor {
                    meta: HwmonSensor { id: format!("{id}/{key}"), kind: kind.into(), key, label, value: None, err },
                    file,
                    scale: scale(kind),
                });
            }
            let slow = name.starts_with("iwlwifi");
            self.devs.push(Dev { id, name, sensors, slow, last: i64::MIN / 2 });
        }
        self.devs.sort_by_key(|d| d.id[5..].parse::<u32>().unwrap_or(u32::MAX));
    }

    pub fn sample(&mut self, now_ms: i64) -> Vec<HwmonDevice> {
        let mut out = Vec::with_capacity(self.devs.len());
        for d in &mut self.devs {
            let due = !d.slow || now_ms - d.last >= 4_900;
            if due {
                d.last = now_ms;
                for s in &mut d.sensors {
                    if let Some(f) = s.file.as_mut() {
                        match f.read(64) {
                            Ok(t) => {
                                s.meta.value = t.trim().parse::<f64>().ok().map(|v| v / s.scale);
                                s.meta.err = None;
                            }
                            Err(e) => {
                                s.meta.value = None;
                                s.meta.err = Some(crate::read_err(&e));
                            }
                        }
                    }
                }
            }
            out.push(HwmonDevice { id: d.id.clone(), name: d.name.clone(), sensors: d.sensors.iter().map(|s| s.meta.clone()).collect() });
        }
        out
    }
}
