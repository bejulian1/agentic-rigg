//! Tidsmerker for oppstart og lukking (`RIGG_TIMING=1`, `scripts/maal.sh`).
//!
//! Hvert merke skrives som `RIGG_TIMING mark=<navn> ms=<fra main()> wall_ns=<CLOCK_REALTIME>`
//! til stderr. Veggklokka lar måleskriptet regne fra før `exec` (det tar tida
//! selv rett før det starter vinduet), uten tikkene på 10 ms i `/proc/self/stat`.
//! Uten variabelen koster et merke én lesing av en `OnceLock`.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static T0: OnceLock<Instant> = OnceLock::new();
static ONCE: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Slår på merkene (kalles først i `main()`).
pub fn init(t0: Instant) {
    if std::env::var_os("RIGG_TIMING").is_some() {
        let _ = T0.set(t0);
    }
}

pub fn on() -> bool {
    T0.get().is_some()
}

/// Veggklokka i ns (samme klokke som `date +%s%N`).
pub fn wall_ns() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Skriver et merke hvis `RIGG_TIMING` er satt.
pub fn mark(name: &str) {
    let Some(t0) = T0.get() else { return };
    // Én skriving per linje, så merker fra to tråder ikke flettes.
    let line = format!("RIGG_TIMING mark={name} ms={:.1} wall_ns={}\n", t0.elapsed().as_secs_f64() * 1000.0, wall_ns());
    use std::io::Write;
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

/// Millisekunder siden `main()` (eller `None` uten `RIGG_TIMING`).
pub fn since_main_ms() -> Option<f64> {
    T0.get().map(|t| t.elapsed().as_secs_f64() * 1000.0)
}

/// Som [`mark`], men bare første gang for hvert navn.
pub fn mark_once(name: &'static str) {
    if !on() {
        return;
    }
    {
        let mut o = ONCE.lock().unwrap_or_else(|e| e.into_inner());
        if o.contains(&name) {
            return;
        }
        o.push(name);
    }
    mark(name);
}
