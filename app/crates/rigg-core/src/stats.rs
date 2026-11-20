//! Min, maks og snitt per sensor, i tjenesten (produktspesifikasjonen 3.5).
//!
//! Min og maks lagres i `minmaks.json` (høyst hvert 30. s) når «Husk min og
//! maks» er på, så de overlever omstart. Snittet er sum og antall, og gjelder
//! siden nullstilling eller siden tjenesten startet (`avg_since`).

use crate::tr;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub const MAX_ROWS: usize = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub min: f64,
    pub max: f64,
    #[serde(skip)]
    pub sum: f64,
    #[serde(skip)]
    pub n: u64,
}

#[derive(Debug, Default)]
pub struct Stats {
    /// Når min og maks sist ble nullstilt (ms).
    pub since: i64,
    /// Når snittet begynte (nullstilling eller oppstart av tjenesten).
    pub avg_since: i64,
    pub rows: BTreeMap<String, Stat>,
    pub dirty: bool,
}

impl Stats {
    pub fn new(now: i64) -> Stats {
        Stats { since: now, avg_since: now, rows: BTreeMap::new(), dirty: false }
    }

    pub fn ingest<'a>(&mut self, values: impl IntoIterator<Item = (&'a str, f64)>) {
        for (k, v) in values {
            if !v.is_finite() {
                continue;
            }
            let room = self.rows.len() < MAX_ROWS;
            match self.rows.get_mut(k) {
                Some(s) => {
                    if v < s.min {
                        s.min = v;
                        self.dirty = true;
                    }
                    if v > s.max {
                        s.max = v;
                        self.dirty = true;
                    }
                    s.sum += v;
                    s.n += 1;
                }
                None if room => {
                    self.rows.insert(k.to_string(), Stat { min: v, max: v, sum: v, n: 1 });
                    self.dirty = true;
                }
                None => {}
            }
        }
    }

    /// Nullstiller alle (`keys = None`) eller noen.
    pub fn reset(&mut self, keys: Option<&[String]>, now: i64) {
        match keys {
            None => {
                self.rows.clear();
                self.since = now;
                self.avg_since = now;
            }
            Some(ks) => {
                for k in ks {
                    self.rows.remove(k);
                }
            }
        }
        self.dirty = true;
    }

    /// Svaret på `get_stats`: `{since, avgSince, stats: {key: {min, max, avg, n}}}`.
    pub fn to_json(&self, keys: Option<&[String]>) -> Value {
        let mut out = serde_json::Map::new();
        let mut put = |k: &str, s: &Stat| {
            let avg = if s.n > 0 { Some(s.sum / s.n as f64) } else { None };
            out.insert(k.to_string(), json!({"min": s.min, "max": s.max, "avg": avg, "n": s.n}));
        };
        match keys {
            Some(ks) => {
                for k in ks {
                    if let Some(s) = self.rows.get(k) {
                        put(k, s);
                    }
                }
            }
            None => {
                for (k, s) in &self.rows {
                    put(k, s);
                }
            }
        }
        json!({"since": self.since, "avgSince": self.avg_since, "stats": out})
    }

    /// Skriver min og maks (ikke snitt) til `minmaks.json`.
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let rows: BTreeMap<&str, [f64; 2]> = self.rows.iter().map(|(k, s)| (k.as_str(), [s.min, s.max])).collect();
        let v = json!({"since": self.since, "rows": rows});
        crate::write_atomic(path, serde_json::to_string(&v).unwrap_or_default().as_bytes())?;
        self.dirty = false;
        Ok(())
    }

    /// Leser `minmaks.json`. Dårlige rader hoppes over; en ødelagt fil gir et
    /// tomt sett og en kopi ved siden av. Snittet begynner alltid nå.
    pub fn load(path: &Path, now: i64) -> (Stats, Option<String>) {
        let mut s = Stats::new(now);
        let Ok(text) = std::fs::read_to_string(path) else { return (s, None) };
        if text.trim().is_empty() {
            return (s, None);
        }
        let parsed: Option<Value> = serde_json::from_str(&text).ok();
        let Some(obj) = parsed.as_ref().and_then(Value::as_object) else {
            let mut bad = path.as_os_str().to_owned();
            bad.push(format!(".ødelagt-{now}"));
            let _ = std::fs::copy(path, &bad);
            return (s, Some(tr!("minmaks.json kunne ikke leses. Min og maks begynner på nytt.").into()));
        };
        if let Some(t) = obj.get("since").and_then(Value::as_i64)
            && t > 1_577_836_800_000
            && t <= now + 60_000
        {
            s.since = t;
        }
        if let Some(rows) = obj.get("rows").and_then(Value::as_object) {
            for (k, v) in rows.iter().take(MAX_ROWS) {
                let pair = v.as_array().and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                if let Some((lo, hi)) = pair
                    && crate::sensor_key::valid(k)
                    && lo <= hi
                {
                    s.rows.insert(k.clone(), Stat { min: lo, max: hi, sum: 0.0, n: 0 });
                }
            }
        }
        (s, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_maks_snitt_og_nullstilling() {
        let mut s = Stats::new(1000);
        s.ingest([("a", 10.0), ("b", 1.0)]);
        s.ingest([("a", 30.0), ("b", f64::NAN)]);
        let j = s.to_json(None);
        assert_eq!(j["stats"]["a"]["min"], 10.0);
        assert_eq!(j["stats"]["a"]["max"], 30.0);
        assert_eq!(j["stats"]["a"]["avg"], 20.0);
        assert_eq!(j["stats"]["b"]["n"], 1);
        s.reset(Some(&["a".into()]), 2000);
        assert!(s.to_json(None)["stats"].get("a").is_none());
        assert_eq!(s.since, 1000);
        s.reset(None, 3000);
        assert_eq!(s.since, 3000);
        assert!(s.rows.is_empty());
    }

    #[test]
    fn lagring_beholder_min_og_maks() {
        let dir = std::env::temp_dir().join(format!("rigg-stats-{}", std::process::id()));
        let f = dir.join("minmaks.json");
        let mut s = Stats::new(1_793_000_000_000);
        s.ingest([("gpu:x/temp", 31.0), ("gpu:x/temp", 70.0)]);
        s.save(&f).unwrap();
        let (t, note) = Stats::load(&f, 1_793_000_100_000);
        assert!(note.is_none());
        assert_eq!(t.rows["gpu:x/temp"].min, 31.0);
        assert_eq!(t.rows["gpu:x/temp"].max, 70.0);
        assert_eq!(t.since, 1_793_000_000_000);
        assert_eq!(t.avg_since, 1_793_000_100_000, "snittet begynner på nytt");
        std::fs::write(&f, "{ødelagt").unwrap();
        let (t, note) = Stats::load(&f, 5);
        assert!(t.rows.is_empty());
        assert!(note.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
