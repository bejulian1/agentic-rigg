//! Prosessor, minne, nett, hwmon og disker fra `/proc` og `/sys` (SMART fra
//! UDisks2 i `udisks.rs`).
//!
//! Filene åpnes én gang og leses med `pread` ved hver prøve. Nye hwmon-enheter
//! og nettkort oppdages hvert 10. s. Filer som bare root kan lese, åpnes ikke
//! på nytt (`err = "root"`). Alle stier går gjennom `root`, så testene kan
//! bruke en kopi av denne maskinen (`tests/fixtures/maskin`).
//!
//! Rettinger fra maskinvarerevisjonen kap. 11: CPU-temperaturen er **Tdie**
//! (Tctl ligger 10 °C over på 2700X), og «buffer og cache» skiller ut `Shmem`.

pub mod cpu;
pub mod cpuinfo;
pub mod disk;
pub mod disk_sim;
pub mod dmi;
pub mod hwmon;
pub mod mem;
pub mod net;
pub mod os;
pub mod pci;
pub mod udisks;

use crate::PreadFile;
use rigg_core::model::SysSample;
use std::path::{Path, PathBuf};

pub struct SysReader {
    root: PathBuf,
    cpu: cpu::Cpu,
    mem: mem::Mem,
    net: net::Net,
    hwmon: hwmon::Hwmon,
    uptime: Option<PreadFile>,
    loadavg: Option<PreadFile>,
    last_scan: i64,
    last5: i64,
    loadavg_cache: Vec<f64>,
}

impl SysReader {
    /// `root` er `/` på maskinen, eller en fixture-mappe i testene.
    pub fn new(root: &Path) -> SysReader {
        let p = |s: &str| root.join(s);
        SysReader {
            root: root.to_path_buf(),
            cpu: cpu::Cpu::new(root),
            mem: mem::Mem::new(root),
            net: net::Net::new(root),
            hwmon: hwmon::Hwmon::new(root),
            uptime: PreadFile::open(&p("proc/uptime")).ok(),
            loadavg: PreadFile::open(&p("proc/loadavg")).ok(),
            last_scan: 0,
            last5: i64::MIN / 2,
            loadavg_cache: vec![],
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Én prøve. Rater (CPU-last, nett) regnes fra forrige kall.
    pub fn sample(&mut self, now_ms: i64) -> SysSample {
        if now_ms - self.last_scan >= 10_000 {
            self.last_scan = now_ms;
            self.net.scan();
            self.hwmon.scan();
        }
        let every5 = now_ms - self.last5 >= 4_900;
        if every5 {
            self.last5 = now_ms;
            self.loadavg_cache = self
                .loadavg
                .as_mut()
                .and_then(|f| f.read(256).ok().map(|s| s.split_whitespace().take(3).filter_map(|x| x.parse().ok()).collect()))
                .unwrap_or_default();
        }
        let mut cpu = self.cpu.sample(every5);
        let hwmon = self.hwmon.sample(now_ms);
        // Tdie som hovedverdi, Tctl i verktøytipset (Package/første temp for Intel og andre).
        if let Some(k) = hwmon.iter().find(|d| matches!(d.name.as_str(), "k10temp" | "coretemp" | "zenpower")) {
            let by = |l: &str| k.sensors.iter().find(|s| s.label == l).and_then(|s| s.value);
            cpu.tctl = by("Tctl");
            cpu.temp = by("Tdie")
                .or_else(|| by("Tctl"))
                .or_else(|| k.sensors.iter().find(|s| s.label.contains("Package")).and_then(|s| s.value))
                .or_else(|| k.sensors.iter().find(|s| s.kind == "temp").and_then(|s| s.value));
        }
        SysSample {
            t: now_ms,
            cpu,
            mem: self.mem.sample(),
            net: self.net.sample(now_ms),
            hwmon,
            uptime: self.uptime.as_mut().and_then(|f| f.read(128).ok()?.split_whitespace().next()?.parse().ok()),
            loadavg: self.loadavg_cache.clone(),
            // Fylles av prøvetråden (`disk::DiskReader`).
            disks: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maskin")
    }

    #[test]
    fn prøve_fra_denne_maskinen() {
        let mut r = SysReader::new(&fixtures());
        let a = r.sample(1_000_000);
        assert_eq!(a.cpu.core_load.len(), 16, "16 tråder");
        assert!(a.cpu.load.is_none(), "første prøve har ingen rate");
        let b = r.sample(1_001_000);
        assert_eq!(b.cpu.load, Some(0.0), "samme fil to ganger gir 0 % last");
        assert!(b.mem.total_kb.unwrap() > 40_000_000.0);
        assert!(b.mem.shmem_kb.is_some(), "Shmem skilles ut");
        let tdie = b.cpu.temp.unwrap();
        let tctl = b.cpu.tctl.unwrap();
        assert!((tctl - tdie - 10.0).abs() < 0.01, "Tdie er hovedverdien, Tctl 10 °C over ({tdie} / {tctl})");
        assert!(b.net.contains_key("wlo1") && !b.net.contains_key("lo"));
        assert!(b.net["wlo1"].wireless);
        assert_eq!(b.net["wlo1"].rx, Some(0.0));
        assert!(b.hwmon.iter().any(|d| d.name == "k10temp" && d.sensors.len() == 2));
        assert!(b.cpu.core_mhz.iter().all(|m| m.is_some()));
        assert_eq!(b.loadavg.len(), 3);
        assert!(b.uptime.unwrap() > 0.0);
    }
}
