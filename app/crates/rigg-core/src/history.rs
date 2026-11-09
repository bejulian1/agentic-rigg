//! Historikk for grafene: port av `server/history.js` (v0.0.11) med samme
//! lag, grenser og spørring, men lagret binært i `historikk.bin`.
//!
//! ```text
//! Lag 0: 1 s-bøtter  i 15 min   (900 bøtter)
//! Lag 1: 10 s-bøtter i 6 t      (2 160 bøtter)
//! Lag 2: 60 s-bøtter i 48 t     (2 880 bøtter)
//! ```
//!
//! Hver prøve legges i den åpne bøtta i alle tre lagene. Når tiden går over i
//! neste bøtte, lukkes den gamle og blir en rad med min, snitt og maks per
//! serie (`f32`, NaN der serien manglet). Topper forsvinner derfor ikke når
//! dataene blir grovere. Der det ikke kom prøver, finnes ingen rader, og
//! grafen viser et hull. Høyst [`MAX_SERIES`] serier, så minnet og fila er
//! høyst ca. 5 940 × 112 × 12 byte ≈ 8,0 MB (7,6 MiB, under kravet på 8 MiB;
//! 96 til 1.1.0, da diskene kom til).
//!
//! I tillegg lagres takten per tidsrom (`cadence`), så grafene kan behandle et
//! hull som sammenhengende når det er kortere enn tre ganger takten
//! (arkitektur.md 4.3): bakgrunnsopptak hvert 5. s er ikke et brudd.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct TierDef {
    pub step: i64,
    pub keep: i64,
}

pub const TIERS: [TierDef; 3] = [TierDef { step: 1, keep: 15 * 60 }, TierDef { step: 10, keep: 6 * 3600 }, TierDef { step: 60, keep: 48 * 3600 }];
/// Tidsvinduene som godtas (sekunder).
pub const WINDOWS: [i64; 7] = [60, 300, 900, 3600, 21600, 86400, 172800];
pub const MAX_SERIES: usize = 112;
pub const MAX_KEYS: usize = 16;
pub const MIN_POINTS: usize = 10;
pub const MAX_POINTS: usize = 4000;
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"RIGGHIST";
const FILE_VERSION: u32 = 1;
const MAX_CADENCE: usize = 2000;

pub fn key_ok(k: &str) -> bool {
    crate::sensor_key::valid(k)
}

#[derive(Clone, Debug)]
struct Row {
    t: i64,
    d: Vec<f32>,
}

struct Open {
    t: i64,
    min: Vec<f64>,
    max: Vec<f64>,
    sum: Vec<f64>,
    cnt: Vec<u32>,
}

impl Open {
    fn new(t: i64) -> Open {
        Open { t, min: vec![f64::INFINITY; MAX_SERIES], max: vec![f64::NEG_INFINITY; MAX_SERIES], sum: vec![0.0; MAX_SERIES], cnt: vec![0; MAX_SERIES] }
    }
}

struct Tier {
    step_ms: i64,
    keep_ms: i64,
    cap: usize,
    rows: std::collections::VecDeque<Row>,
    open: Option<Open>,
}

impl Tier {
    fn new(def: TierDef) -> Tier {
        Tier { step_ms: def.step * 1000, keep_ms: def.keep * 1000, cap: (def.keep / def.step) as usize + 1, rows: Default::default(), open: None }
    }

    fn open_row(&self, n: usize) -> Option<Row> {
        let o = self.open.as_ref()?;
        let mut any = false;
        let mut d = vec![f32::NAN; n * 3];
        for i in 0..n {
            if o.cnt[i] == 0 {
                continue;
            }
            any = true;
            d[i * 3] = o.min[i] as f32;
            d[i * 3 + 1] = (o.sum[i] / o.cnt[i] as f64) as f32;
            d[i * 3 + 2] = o.max[i] as f32;
        }
        any.then_some(Row { t: o.t, d })
    }

    fn close(&mut self, n: usize) {
        let row = self.open_row(n);
        self.open = None;
        if let Some(r) = row {
            self.append(r);
        }
    }

    fn append(&mut self, row: Row) {
        if let Some(last) = self.rows.back()
            && row.t <= last.t
        {
            return;
        }
        let t = row.t;
        self.rows.push_back(row);
        self.trim(t);
    }

    fn trim(&mut self, now: i64) {
        let cut = now - self.keep_ms;
        while self.rows.front().is_some_and(|r| r.t < cut) {
            self.rows.pop_front();
        }
        while self.rows.len() > self.cap {
            self.rows.pop_front();
        }
    }
}

/// Én serie i et svar: min, snitt og maks per punkt (`None` = ingen data).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Series {
    pub min: Vec<Option<f64>>,
    pub avg: Vec<Option<f64>>,
    pub max: Vec<Option<f64>>,
}

/// Svaret på `get_history` (samme form som `/api/history`).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct QueryResult {
    pub window: i64,
    /// Sekunder per punkt.
    pub step: f64,
    pub from: i64,
    pub to: i64,
    pub oldest: Option<i64>,
    pub t: Vec<i64>,
    pub series: BTreeMap<String, Series>,
    /// Takten prøvene ble tatt med i vinduet: `[[fra (ms), takt (ms)], …]`.
    pub cadence: Vec<(i64, u32)>,
}

pub struct History {
    path: Option<PathBuf>,
    keys: Vec<String>,
    index: HashMap<String, usize>,
    tiers: Vec<Tier>,
    cadence: Vec<(i64, u32)>,
    /// Serier som ikke fikk plass (over [`MAX_SERIES`]).
    pub dropped: u64,
    /// Satt når fila ikke kunne leses.
    pub load_note: Option<String>,
    pub writes: u64,
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

impl History {
    pub fn new(path: Option<PathBuf>) -> History {
        History {
            path,
            keys: Vec::new(),
            index: HashMap::new(),
            tiers: TIERS.iter().map(|d| Tier::new(*d)).collect(),
            cadence: Vec::new(),
            dropped: 0,
            load_note: None,
            writes: 0,
        }
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    fn key_index(&mut self, key: &str) -> Option<usize> {
        if let Some(&i) = self.index.get(key) {
            return Some(i);
        }
        if self.keys.len() >= MAX_SERIES || !key_ok(key) {
            self.dropped += 1;
            return None;
        }
        let i = self.keys.len();
        self.keys.push(key.to_string());
        self.index.insert(key.to_string(), i);
        Some(i)
    }

    /// Takten fra og med `t` (ms). Lagres bare når den endres.
    pub fn note_cadence(&mut self, t: i64, interval_ms: u32) {
        if self.cadence.last().map(|c| c.1) == Some(interval_ms) {
            return;
        }
        self.cadence.push((t, interval_ms));
        let cut = t - TIERS[2].keep * 1000;
        // Behold den siste før grensen, så starten av vinduet har en takt.
        while self.cadence.len() > 1 && self.cadence[1].0 < cut {
            self.cadence.remove(0);
        }
        if self.cadence.len() > MAX_CADENCE {
            self.cadence.remove(0);
        }
    }

    /// Én prøve. `t` i ms.
    pub fn ingest<'a>(&mut self, t: i64, values: impl IntoIterator<Item = (&'a str, f64)>) {
        let mut pairs = Vec::new();
        for (k, v) in values {
            if !v.is_finite() {
                continue;
            }
            if let Some(i) = self.key_index(k) {
                pairs.push((i, v));
            }
        }
        if pairs.is_empty() {
            return;
        }
        let n = self.keys.len();
        for tier in &mut self.tiers {
            let b = t.div_euclid(tier.step_ms) * tier.step_ms;
            if let Some(o) = &tier.open
                && o.t != b
            {
                if b < o.t {
                    continue; // klokka gikk baklengs: hopp over prøven
                }
                tier.close(n);
            }
            if tier.open.is_none() {
                if tier.rows.back().is_some_and(|last| b <= last.t) {
                    continue;
                }
                tier.open = Some(Open::new(b));
            }
            let o = tier.open.as_mut().unwrap();
            for &(i, v) in &pairs {
                if v < o.min[i] {
                    o.min[i] = v;
                }
                if v > o.max[i] {
                    o.max[i] = v;
                }
                o.sum[i] += v;
                o.cnt[i] += 1;
            }
        }
    }

    pub fn close_all(&mut self) {
        let n = self.keys.len();
        for t in &mut self.tiers {
            t.close(n);
        }
    }

    /// Rader per lag (tester og feilsøking).
    pub fn sizes(&self) -> [usize; 3] {
        [self.tiers[0].rows.len(), self.tiers[1].rows.len(), self.tiers[2].rows.len()]
    }

    pub fn oldest(&self) -> Option<i64> {
        self.tiers.iter().filter_map(|t| t.rows.front().map(|r| r.t)).min()
    }

    /// Henter et tidsvindu (sekunder) for nøklene, høyst `points` punkter.
    pub fn query(&self, now: i64, window: i64, keys: &[String], points: usize) -> QueryResult {
        let points = points.clamp(1, MAX_POINTS);
        let ti = self.tiers.iter().position(|t| t.keep_ms >= window * 1000).unwrap_or(self.tiers.len() - 1);
        let tier = &self.tiers[ti];
        let from = now - window * 1000;
        let per = (((window * 1000) as f64 / points as f64 / tier.step_ms as f64).ceil() as i64).max(1);
        let step_ms = per * tier.step_ms;
        let idx: Vec<Option<usize>> = keys.iter().map(|k| self.index.get(k).copied()).collect();
        let mut rows: Vec<&Row> = tier.rows.iter().filter(|r| r.t >= from).collect();
        let open = tier.open_row(self.keys.len());
        if let Some(o) = &open
            && o.t >= from
        {
            rows.push(o);
        }
        struct Acc {
            min: f64,
            max: f64,
            sum: f64,
            n: u32,
        }
        let mut t_out: Vec<i64> = Vec::new();
        let mut out: Vec<Series> = keys.iter().map(|_| Series::default()).collect();
        let mut cur: Option<(i64, Vec<Acc>)> = None;
        let flush = |cur: &mut Option<(i64, Vec<Acc>)>, t_out: &mut Vec<i64>, out: &mut Vec<Series>| {
            let Some((t, acc)) = cur.take() else { return };
            if acc.iter().all(|a| a.n == 0) {
                return;
            }
            t_out.push(t);
            for (s, a) in out.iter_mut().zip(acc.iter()) {
                let has = a.n > 0;
                s.min.push(has.then(|| round3(a.min)));
                s.avg.push(has.then(|| round3(a.sum / a.n as f64)));
                s.max.push(has.then(|| round3(a.max)));
            }
        };
        for r in rows {
            let b = r.t.div_euclid(step_ms) * step_ms;
            if cur.as_ref().map(|c| c.0) != Some(b) {
                flush(&mut cur, &mut t_out, &mut out);
                cur = Some((b, keys.iter().map(|_| Acc { min: f64::INFINITY, max: f64::NEG_INFINITY, sum: 0.0, n: 0 }).collect()));
            }
            let acc = &mut cur.as_mut().unwrap().1;
            for (k, i) in idx.iter().enumerate() {
                let Some(i) = *i else { continue };
                if i * 3 + 2 >= r.d.len() {
                    continue;
                }
                let lo = r.d[i * 3];
                if lo.is_nan() {
                    continue;
                }
                let a = &mut acc[k];
                a.min = a.min.min(lo as f64);
                a.max = a.max.max(r.d[i * 3 + 2] as f64);
                a.sum += r.d[i * 3 + 1] as f64;
                a.n += 1;
            }
        }
        flush(&mut cur, &mut t_out, &mut out);
        // Bøttene er justert til hele steg, så vinduet kan dekke ett steg for mye.
        if t_out.len() > points {
            let extra = t_out.len() - points;
            t_out.drain(..extra);
            for s in &mut out {
                s.min.drain(..extra);
                s.avg.drain(..extra);
                s.max.drain(..extra);
            }
        }
        let mut cadence: Vec<(i64, u32)> = Vec::new();
        for (i, c) in self.cadence.iter().enumerate() {
            let next = self.cadence.get(i + 1).map(|n| n.0).unwrap_or(i64::MAX);
            if next >= from {
                cadence.push(*c);
            }
        }
        QueryResult {
            window,
            step: step_ms as f64 / 1000.0,
            from,
            to: now,
            oldest: self.oldest(),
            t: t_out,
            series: keys.iter().cloned().zip(out).collect(),
            cadence,
        }
    }

    // ---- Lagring ----

    /// Hele historikken som bytes (de åpne bøttene tas med som rader).
    pub fn serialize(&self, now: i64) -> Vec<u8> {
        let n = self.keys.len();
        let mut b = Vec::with_capacity(1024);
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FILE_VERSION.to_le_bytes());
        b.extend_from_slice(&now.to_le_bytes());
        b.extend_from_slice(&(n as u32).to_le_bytes());
        for k in &self.keys {
            b.extend_from_slice(&(k.len() as u16).to_le_bytes());
            b.extend_from_slice(k.as_bytes());
        }
        b.extend_from_slice(&(self.cadence.len() as u32).to_le_bytes());
        for (t, ms) in &self.cadence {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&ms.to_le_bytes());
        }
        for tier in &self.tiers {
            let open = tier.open_row(n);
            let count = tier.rows.len() + open.is_some() as usize;
            b.extend_from_slice(&((tier.step_ms / 1000) as u32).to_le_bytes());
            b.extend_from_slice(&(count as u32).to_le_bytes());
            for r in tier.rows.iter().chain(open.iter()) {
                b.extend_from_slice(&r.t.to_le_bytes());
                b.extend_from_slice(&((r.d.len() / 3) as u32).to_le_bytes());
                for x in &r.d {
                    b.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        b
    }

    /// Skriver fila atomisk. Gir `false` uten sti eller ved feil.
    pub fn save(&mut self, now: i64) -> bool {
        let Some(path) = self.path.clone() else { return false };
        let bytes = self.serialize(now);
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return false;
        }
        match crate::write_atomic(&path, &bytes) {
            Ok(()) => {
                self.writes += 1;
                true
            }
            Err(_) => false,
        }
    }

    /// Leser fila. En ødelagt fil flyttes til `<navn>.bad`, og historikken
    /// begynner på nytt (`load_note` sier fra). Rader som er for gamle for
    /// laget sitt, eller i framtida, hoppes over.
    pub fn load(&mut self, now: i64) -> bool {
        let Some(path) = self.path.clone() else { return false };
        if !path.exists() {
            return false;
        }
        let res = std::fs::metadata(&path)
            .map_err(|e| e.to_string())
            .and_then(|m| if m.len() > MAX_FILE_BYTES { Err("fila er større enn grensen".into()) } else { Ok(()) })
            .and_then(|_| std::fs::read(&path).map_err(|e| e.to_string()))
            .and_then(|b| self.restore(&b, now));
        match res {
            Ok(()) => true,
            Err(why) => {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("historikk.bin").to_string();
                self.load_note = Some(format!("{name} kunne ikke leses ({why}). Historikken begynner på nytt."));
                let _ = std::fs::rename(&path, bad_path(&path));
                *self = History { path: self.path.take(), load_note: self.load_note.take(), ..History::new(None) };
                false
            }
        }
    }

    fn restore(&mut self, b: &[u8], now: i64) -> Result<(), String> {
        let mut r = Reader { b, i: 0 };
        if r.take(8)? != MAGIC {
            return Err("ukjent format".into());
        }
        if r.u32()? != FILE_VERSION {
            return Err("ukjent versjon".into());
        }
        let _saved = r.i64()?;
        let n = r.u32()? as usize;
        if n > MAX_SERIES {
            return Err("for mange serier".into());
        }
        let mut keys = Vec::with_capacity(n);
        for _ in 0..n {
            let len = r.u16()? as usize;
            let k = std::str::from_utf8(r.take(len)?).map_err(|_| "ugyldig nøkkel")?.to_string();
            if !key_ok(&k) || keys.contains(&k) {
                return Err("ugyldige serier".into());
            }
            keys.push(k);
        }
        let nc = r.u32()? as usize;
        if nc > MAX_CADENCE {
            return Err("ugyldig takt".into());
        }
        let mut cadence = Vec::with_capacity(nc);
        for _ in 0..nc {
            cadence.push((r.i64()?, r.u32()?));
        }
        let mut tiers = Vec::new();
        for def in TIERS {
            if r.u32()? as i64 != def.step {
                return Err("ugyldige lag".into());
            }
            let count = r.u32()? as usize;
            let mut tier = Tier::new(def);
            if count > tier.cap + 1 {
                return Err("for mange rader".into());
            }
            for _ in 0..count {
                let t = r.i64()?;
                let m = r.u32()? as usize;
                if m > n {
                    return Err("ugyldig rad".into());
                }
                let mut d = Vec::with_capacity(m * 3);
                for _ in 0..m * 3 {
                    let x = f32::from_le_bytes(r.take(4)?.try_into().unwrap());
                    if x.is_infinite() {
                        return Err("ugyldig verdi".into());
                    }
                    d.push(x);
                }
                if t > now + 60_000 || t < now - def.keep * 1000 {
                    continue;
                }
                tier.append(Row { t, d });
            }
            tiers.push(tier);
        }
        if r.i != b.len() {
            return Err("ekstra data på slutten".into());
        }
        self.index = keys.iter().enumerate().map(|(i, k)| (k.clone(), i)).collect();
        self.keys = keys;
        self.tiers = tiers;
        self.cadence = cadence;
        Ok(())
    }

    /// Glemmer alt (Innstillinger › Data › Slett historikken) og fjerner fila.
    pub fn clear(&mut self) {
        let path = self.path.take();
        *self = History::new(None);
        self.path = path;
        if let Some(p) = &self.path {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn bad_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".bad");
    PathBuf::from(s)
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.i + n > self.b.len() {
            return Err("fila er avkortet".into());
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

/// Syntetisk historikk for skjermbilder og tester (som `fillSynthetic`): 48 t
/// med døgnrytme, en topp og et hull på 40 minutter. Brukes aldri ellers.
pub fn fill_synthetic(h: &mut History, now: i64, hours: i64, gpu: &str) {
    let start = now - hours * 3_600_000;
    let hole_a = now - 7 * 3_600_000;
    let hole_b = hole_a + 40 * 60_000;
    let k = |f: &str| crate::sensor_key::gpu(gpu, f);
    let keys: Vec<String> = ["temp", "gpuUtil", "memUtil", "coreClock", "memClock", "power", "powerLimit", "fan", "vramUsed"].iter().map(|f| k(f)).collect();
    let mut t = start;
    while t < now - 2000 {
        if t >= hole_a && t < hole_b {
            t = hole_b;
            continue;
        }
        let tf = t as f64;
        let day = ((tf / 3_600_000.0) % 24.0 / 24.0 * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2).sin() * 0.5 + 0.5;
        let burst = if (((tf / 60_000.0) % 180.0) - 90.0).abs() < 12.0 { 1.0 } else { 0.0 };
        let noise = (tf / 7_000.0).sin() * 0.5 + (tf / 23_000.0).sin() * 0.5;
        let load = (4.0 + day * 18.0 + burst * 70.0 + noise * 6.0).clamp(0.0, 100.0);
        let temp = 33.0 + load * 0.42 + noise;
        let vals = [
            temp,
            load,
            load * 0.6,
            300.0 + load * 16.0,
            if load > 10.0 { 7000.0 } else { 405.0 },
            14.0 + load * 2.1,
            245.0,
            (35.0 + (temp - 55.0) * 2.2).clamp(35.0, 100.0),
            600.0 + load * 40.0,
        ];
        let cpu_load = (6.0 + day * 20.0 + burst * 35.0 + noise * 4.0).clamp(0.0, 100.0);
        let mut pairs: Vec<(&str, f64)> = keys.iter().map(|s| s.as_str()).zip(vals).collect();
        pairs.push(("cpu/load", cpu_load));
        pairs.push(("cpu/temp", 38.0 + day * 12.0 + burst * 18.0 + noise * 2.0));
        pairs.push(("mem/usedPct", 22.0 + day * 6.0 + burst * 10.0));
        pairs.push(("net/rx", ((day * 400.0 + burst * 3000.0 + noise * 200.0) * 1000.0).max(0.0)));
        pairs.push(("net/tx", ((day * 60.0 + noise * 30.0) * 1000.0).max(0.0)));
        h.ingest(t, pairs);
        t += if t > now - 15 * 60_000 { 1000 } else { 10_000 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const T0: i64 = 1_793_880_000_000; // 5. nov. 2026 12:00 UTC

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("rigg-hist-{}-{}", std::process::id(), crate::now_ms() ^ (rand_ish() as i64)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn rand_ish() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        N.fetch_add(1, Ordering::Relaxed) * 7919
    }
    fn k(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    #[test]
    fn bøtter_min_snitt_maks_og_toppen_overlever() {
        let mut h = History::new(None);
        let mut t = T0 - 10 * 60_000;
        while t < T0 {
            h.ingest(t, [("gpu:x/temp", if t == T0 - 5 * 60_000 { 90.0 } else { 40.0 })]);
            t += 500;
        }
        let s1 = h.query(T0, 900, &k("gpu:x/temp"), 4000);
        assert_eq!(s1.step, 1.0, "15 min bruker 1 s-laget");
        let i = s1.t.iter().position(|&x| x == T0 - 5 * 60_000).expect("bøtta med toppen finnes");
        let s = &s1.series["gpu:x/temp"];
        assert_eq!((s.min[i], s.avg[i], s.max[i]), (Some(40.0), Some(65.0), Some(90.0)));
        let s48 = h.query(T0, 172800, &k("gpu:x/temp"), 100);
        assert!(s48.step >= 60.0);
        let s = &s48.series["gpu:x/temp"];
        assert_eq!(s.max.iter().flatten().cloned().fold(f64::MIN, f64::max), 90.0);
        assert_eq!(s.min.iter().flatten().cloned().fold(f64::MAX, f64::min), 40.0);
    }

    #[test]
    fn nedsampling_aldri_flere_punkter_enn_bedt_om() {
        let mut h = History::new(None);
        let mut t = T0 - 7 * H;
        while t < T0 {
            h.ingest(t, [("cpu/load", ((t / 10_000) % 100) as f64)]);
            t += 10_000;
        }
        for (win, step) in [(60, 1), (900, 1), (3600, 10), (21600, 10), (86400, 60), (172800, 60)] {
            let r = h.query(T0, win, &k("cpu/load"), 50);
            assert!(r.t.len() <= 50, "{win} s gir høyst 50 punkter (fikk {})", r.t.len());
            assert_eq!(r.step as i64 % step, 0, "{win} s bruker {step} s-laget");
            assert!(r.t.windows(2).all(|w| w[1] > w[0]), "stigende tid");
        }
    }

    #[test]
    fn hull_der_det_ikke_kom_prøver() {
        let mut h = History::new(None);
        let mut t = T0 - 2 * H;
        while t < T0 - H {
            h.ingest(t, [("gpu:x/power", 20.0)]);
            t += 10_000;
        }
        let mut t = T0 - 30 * 60_000;
        while t < T0 {
            h.ingest(t, [("gpu:x/power", 30.0)]);
            t += 10_000;
        }
        let r = h.query(T0, 21600, &k("gpu:x/power"), 2000);
        let gap = r.t.windows(2).map(|w| w[1] - w[0]).max().unwrap();
        assert!(gap >= 29 * 60_000, "et hull på ca. 30 min");
        assert!(r.t[0] >= T0 - 2 * H - 10_000);
    }

    #[test]
    fn grenser_for_serier_og_rader() {
        let mut h = History::new(None);
        let many: Vec<String> = (0..MAX_SERIES + 40).map(|i| format!("hwmon/s{i}/temp1")).collect();
        h.ingest(T0 - 1000, many.iter().map(|s| (s.as_str(), 1.0)));
        assert_eq!(h.keys().len(), MAX_SERIES);
        assert!(h.dropped >= 40);
        let mut g = History::new(None);
        let mut t = T0 - 50 * H;
        while t < T0 {
            g.ingest(t, [("gpu:x/temp", 50.0)]);
            t += 10_000;
        }
        let sizes = g.sizes();
        for (i, def) in TIERS.iter().enumerate() {
            assert!(sizes[i] <= (def.keep / def.step) as usize + 1, "lag {i}: {} rader", sizes[i]);
        }
        assert!(g.oldest().unwrap() >= T0 - 48 * H - 3 * 60_000);
        h.ingest(T0, [("gpu/a b", 1.0)]);
        assert!(h.keys().iter().all(|k| key_ok(k)));
    }

    #[test]
    fn lagring_og_innlesing_gir_samme_svar() {
        let dir = tmp();
        let file = dir.join("historikk.bin");
        let mut a = History::new(Some(file.clone()));
        fill_synthetic(&mut a, T0, 48, "0000:1c:00.0");
        a.note_cadence(T0 - 48 * H, 1000);
        assert!(a.save(T0));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "temp-fila er borte (rename)");
        let mut b = History::new(Some(file.clone()));
        assert!(b.load(T0));
        assert!(b.load_note.is_none());
        let keys = vec!["gpu:0000:1c:00.0/temp".to_string(), "cpu/load".to_string()];
        for win in [900, 21600, 172800] {
            let x = a.query(T0, win, &keys, 300);
            let y = b.query(T0, win, &keys, 300);
            assert_eq!(x.t, y.t, "samme tidspunkter for {win} s");
            assert_eq!(x.series, y.series, "samme verdier for {win} s");
        }
        assert_eq!(b.query(T0, 60, &keys, 10).cadence, vec![(T0 - 48 * H, 1000)]);
        let mut later = History::new(Some(file.clone()));
        later.load(T0 + 10 * H);
        assert_eq!(later.sizes()[0], 0, "1 s-laget er utløpt");
        assert_eq!(later.sizes()[1], 0, "10 s-laget er utløpt");
        assert!(later.sizes()[2] > 0, "60 s-laget har fortsatt data");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn verste_tilfelle_under_grensen() {
        let mut h = History::new(None);
        let keys: Vec<String> = (0..MAX_SERIES).map(|i| format!("hwmon/hwmon{i}/temp1")).collect();
        let mut t = T0 - 48 * H;
        while t < T0 {
            h.ingest(t, keys.iter().map(|s| (s.as_str(), 1234.567 + (t % 997) as f64 / 1000.0)));
            t += if t > T0 - 15 * 60_000 { 1000 } else if t > T0 - 6 * H { 10_000 } else { 60_000 };
        }
        let bytes = h.serialize(T0);
        assert!(bytes.len() as u64 <= 8 * 1024 * 1024, "fila er {:.1} MiB (krav ≤ 8 MB)", bytes.len() as f64 / 1048576.0);
    }

    #[test]
    fn ødelagt_fil_flyttes_og_historikken_begynner_på_nytt() {
        let mut good = History::new(None);
        good.ingest(T0, [("gpu:x/temp", 40.0)]);
        let ok = good.serialize(T0);
        let mut cut = ok.clone();
        cut.truncate(ok.len() - 3);
        let mut wrong_magic = ok.clone();
        wrong_magic[0] = b'X';
        let mut bad_key = History::new(None).serialize(T0);
        bad_key.truncate(20);
        for bad in [b"ikke en historikk".to_vec(), cut, wrong_magic, bad_key] {
            let dir = tmp();
            let f = dir.join("historikk.bin");
            std::fs::write(&f, &bad).unwrap();
            let mut h = History::new(Some(f.clone()));
            assert!(!h.load(T0));
            assert!(h.load_note.as_deref().unwrap().contains("kunne ikke leses"));
            assert!(dir.join("historikk.bin.bad").exists());
            assert_eq!(h.sizes(), [0, 0, 0]);
            h.ingest(T0, [("gpu:x/temp", 40.0)]);
            assert_eq!(h.keys().len(), 1, "virker som vanlig etterpå");
            assert!(h.save(T0), "kan lagre igjen");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn lesing_endrer_ikke_historikken() {
        let mut h = History::new(None);
        let mut t = T0 - 60_000;
        while t < T0 {
            h.ingest(t, [("gpu:x/temp", 40.0)]);
            t += 1000;
        }
        let before = h.serialize(T0);
        h.query(T0, 60, &["gpu:x/temp".into(), "finnes.ikke".into()], 10);
        assert_eq!(h.serialize(T0), before);
        assert!(h.query(T0, 60, &k("finnes.ikke"), 10).t.is_empty(), "ukjent serie gir ingen punkter");
    }

    #[test]
    fn takten_lagres_bare_ved_endring() {
        let mut h = History::new(None);
        h.note_cadence(T0 - 1000, 1000);
        h.note_cadence(T0 - 500, 1000);
        h.note_cadence(T0, 5000);
        assert_eq!(h.query(T0, 60, &[], 10).cadence, vec![(T0 - 1000, 1000), (T0, 5000)]);
    }
}
