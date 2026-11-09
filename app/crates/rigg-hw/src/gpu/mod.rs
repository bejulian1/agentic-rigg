//! Skjermkortet: felles grensesnitt for backendene og prøveplanen.
//!
//! Prøveplanen følger `maskinvare-revisjon.md` kap. 12 (arkitektur.md 4.3):
//!
//! | Når | Hva |
//! |---|---|
//! | hver runde (1 s med vindu, 1/2/5 s i bakgrunnen, aldri under 0,5 s) | temperatur, energiteller (effekt), klokke grafikk og minne, last, `MemoryInfo_v2`, P-tilstand, vifte 0, PCIe-generasjon fra sysfs |
//! | hvert 2. s | vifte 1 …, turtall, SM- og videoklokke, koder/dekoder, klokkebegrensninger bare utenfor P8 |
//! | hvert 5. s | håndhevet effektgrense, PCIe-bredde (sysfs), viftestyring og målfart (bare med vindu) |
//!
//! Uten vindu (bakgrunn, minimert) kommer «hvert 2. s» hvert 10. s og «hvert
//! 5. s» hvert 30. s (runde 3C: bakgrunnen skal koste under 1,2 % av én kjerne).
//! | én gang, fordelt | faste data ([`GpuBackend::static_info`]) |
//!
//! NVML kalles bare fra prøvetråden i tjenesten, aldri på en GTK-tråd.

pub mod nvml;
pub mod nvml_info;
pub mod sim;
/// Viftesløyfen mot det simulerte kortet (7B), felles for `riggd` og hjelperens tester.
pub mod sim_vifter;
/// Skrivedelen, bare for root-hjelperen (runde 7A). Det ekte kortet er sperret.
pub mod skriv;
#[cfg(feature = "ekte-skriving")]
pub mod skriv_nvml;

use rigg_core::model::{DeviceState, GpuInfo, GpuSample, GpuStatic};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Hva denne runden skal lese.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plan {
    /// Klokka for runden (ms, veggtid).
    pub now_ms: i64,
    /// Hvert 2. s.
    pub every2: bool,
    /// Hvert 5. s.
    pub every5: bool,
    /// Hvert 60. s (driverversjonen).
    pub every60: bool,
    /// Første runde etter oppstart eller ny initialisering.
    pub first: bool,
    /// Et vindu viser verdiene (eller loggen går). Uten vindu leses ikke det
    /// bare vinduet viser (viftestyring og målfart), og `every2`/`every5`
    /// kommer sjeldnere (sampler.rs).
    pub watched: bool,
}

/// Resultatet av én runde.
#[derive(Clone, Debug, Default)]
pub struct Round {
    pub samples: BTreeMap<String, GpuSample>,
}

pub trait GpuBackend: Send {
    /// «nvml» eller «sim».
    fn kind(&self) -> &'static str;
    /// Kortene og tilstanden deres.
    fn gpus(&self) -> Vec<GpuInfo>;
    /// Én runde etter planen. Kort som ikke kan leses, er ikke med.
    fn sample(&mut self, plan: &Plan) -> Round;
    /// Faste data for et kort. Leses gradvis de første sekundene (én del per
    /// runde), så kortet ikke vekkes fra P8; `complete` sier om alt er lest.
    fn static_info(&self, id: &str) -> Option<(GpuStatic, bool)>;
    /// Driverversjonen og tilstandene sjekkes (hvert 60. s og ved behov).
    fn check(&mut self, now_ms: i64);
    /// Prosessene som bruker kortet nå (`get_processes`). Leses bare mens
    /// Grafikkort vises, høyst hvert 5. s. `None` når kortet ikke kan leses.
    fn processes(&mut self, _id: &str) -> Option<serde_json::Value> {
        None
    }
    /// Lukker biblioteket (ved stopp og før dvale).
    fn shutdown(&mut self) {}
}

/// Tom backend når ingen skjermkort skal leses (`--backend ingen`).
pub struct NoGpu;

impl GpuBackend for NoGpu {
    fn kind(&self) -> &'static str {
        "ingen"
    }
    fn gpus(&self) -> Vec<GpuInfo> {
        vec![]
    }
    fn sample(&mut self, _plan: &Plan) -> Round {
        Round::default()
    }
    fn static_info(&self, _id: &str) -> Option<(GpuStatic, bool)> {
        None
    }
    fn check(&mut self, _now_ms: i64) {}
}

/// PCIe-generasjon fra sysfs `current_link_speed` («2.5 GT/s PCIe» = gen 1).
pub fn pcie_gen_from_speed(s: &str) -> Option<f64> {
    let gt: f64 = s.split_whitespace().next()?.parse().ok()?;
    Some(match gt {
        x if x < 3.0 => 1.0,
        x if x < 6.0 => 2.0,
        x if x < 10.0 => 3.0,
        x if x < 20.0 => 4.0,
        x if x < 40.0 => 5.0,
        _ => 6.0,
    })
}

/// Lenkefilene i sysfs for ett kort, åpnet én gang.
pub struct PcieLink {
    speed: Option<crate::PreadFile>,
    width: Option<crate::PreadFile>,
    pub max_speed: Option<String>,
    pub max_width: Option<f64>,
}

impl PcieLink {
    pub fn open(root: &Path, addr: &str) -> PcieLink {
        let d = pci_dir(root, addr);
        let open = |f: &str| crate::PreadFile::open(&d.join(f)).ok();
        PcieLink {
            speed: open("current_link_speed"),
            width: open("current_link_width"),
            max_speed: crate::read_text(&d.join("max_link_speed")),
            max_width: crate::read_text(&d.join("max_link_width")).and_then(|s| s.parse().ok()),
        }
    }

    pub fn gen_now(&mut self) -> Option<f64> {
        let s = self.speed.as_mut()?.read(64).ok()?.to_string();
        pcie_gen_from_speed(&s)
    }

    pub fn width_now(&mut self) -> Option<f64> {
        self.width.as_mut()?.read_f64()
    }

    pub fn gen_max(&self) -> Option<f64> {
        pcie_gen_from_speed(self.max_speed.as_deref()?)
    }
}

/// `/sys/bus/pci/devices/<adresse>` under roten.
pub fn pci_dir(root: &Path, addr: &str) -> PathBuf {
    root.join("sys/bus/pci/devices").join(addr)
}

/// «00000000:1C:00.0» (NVML) → «0000:1c:00.0» (sysfs).
pub fn sysfs_addr(nvml_bus_id: &str) -> String {
    let s = nvml_bus_id.trim().to_ascii_lowercase();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let dom = parts[0];
        let dom = if dom.len() > 4 { &dom[dom.len() - 4..] } else { dom };
        format!("{dom:0>4}:{}:{}", parts[1], parts[2])
    } else {
        s
    }
}

/// Driverversjonen som kjører (kjernemodulen).
pub fn running_driver(root: &Path) -> Option<String> {
    crate::read_text(&root.join("sys/module/nvidia/version"))
}

/// Driverversjonen som er installert: målet for symlenken `libnvidia-ml.so.1`
/// («libnvidia-ml.so.610.43.02»). Etter en oppdatering uten omstart er den
/// nyere enn den som kjører.
pub fn installed_driver(root: &Path) -> Option<String> {
    for dir in ["usr/lib/x86_64-linux-gnu", "usr/lib64", "usr/lib", "usr/lib/nvidia"] {
        let link = root.join(dir).join("libnvidia-ml.so.1");
        if let Ok(target) = std::fs::read_link(&link) {
            let name = target.file_name()?.to_string_lossy().to_string();
            let v = name.strip_prefix("libnvidia-ml.so.")?;
            if v.chars().next().is_some_and(|c| c.is_ascii_digit()) && v.contains('.') {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Tilstanden ut fra driverversjonene: `driver_updated` når det installerte
/// biblioteket er nyere enn modulen som kjører.
pub fn driver_state(running: Option<&str>, installed: Option<&str>) -> DeviceState {
    match (running, installed) {
        (Some(r), Some(i)) if r != i => DeviceState::DriverUpdated,
        _ => DeviceState::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcie_og_adresser() {
        assert_eq!(pcie_gen_from_speed("2.5 GT/s PCIe"), Some(1.0));
        assert_eq!(pcie_gen_from_speed("8.0 GT/s PCIe"), Some(3.0));
        assert_eq!(pcie_gen_from_speed("16.0 GT/s PCIe"), Some(4.0));
        assert_eq!(pcie_gen_from_speed("Unknown"), None);
        assert_eq!(sysfs_addr("00000000:1C:00.0"), "0000:1c:00.0");
        assert_eq!(driver_state(Some("610.43.02"), Some("610.51")), DeviceState::DriverUpdated);
        assert_eq!(driver_state(Some("610.43.02"), Some("610.43.02")), DeviceState::Ok);
        assert_eq!(driver_state(Some("610.43.02"), None), DeviceState::Ok);
    }

    #[test]
    fn lenken_fra_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maskin");
        let mut l = PcieLink::open(&root, "0000:1c:00.0");
        assert!(l.gen_now().is_some());
        assert_eq!(l.width_now(), Some(16.0));
        assert_eq!(l.gen_max(), Some(3.0));
        assert_eq!(running_driver(&root).as_deref(), Some("610.43.02"));
    }
}
