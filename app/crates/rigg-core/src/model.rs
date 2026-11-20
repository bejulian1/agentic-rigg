//! Datamodellen: prøver fra skjermkortet og maskinen, faste data og
//! tilstanden til hvert kort. Feltnavnene er de samme som demoens `sample` og
//! `sys` (camelCase), så sidene kan porteres rett (arkitektur.md kap. 5).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stabil ID for et skjermkort: PCI-adressen slik sysfs skriver den
/// («0000:1c:00.0»). Aldri bare indeksen (produktspesifikasjonen 3.7).
pub type GpuId = String;

/// Én prøve fra ett skjermkort. `None` betyr at verdien ikke kunne leses
/// akkurat nå, eller at kortet ikke støtter den (se `na`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GpuSample {
    /// GPU-temperatur, °C.
    pub temp: Option<f64>,
    /// GPU-last og minnekontroller-last, %.
    pub gpu_util: Option<f64>,
    pub mem_util: Option<f64>,
    /// Videokoder og -dekoder, % (hvert 2. s).
    pub enc_util: Option<f64>,
    pub dec_util: Option<f64>,
    /// Klokker, MHz. SM og video leses hvert 2. s.
    pub core_clock: Option<f64>,
    pub sm_clock: Option<f64>,
    pub mem_clock: Option<f64>,
    pub video_clock: Option<f64>,
    /// Effekt, W: snitt for perioden fra energitelleren (ΔmJ/Δms), eller
    /// øyeblikksverdien når telleren ikke kan leses.
    pub power: Option<f64>,
    /// Håndhevet effektgrense, W (hvert 5. s).
    pub power_limit: Option<f64>,
    /// Høyeste viftefart av viftene, %. Alle står i `fans`.
    pub fan: Option<f64>,
    pub fans: Vec<Option<f64>>,
    /// Turtall per vifte, o/min (hvert 2. s).
    pub fan_rpm: Vec<Option<f64>>,
    /// Viftestyringen per vifte (`nvmlDeviceGetFanControlPolicy_v2`, hvert
    /// 5. s): 0 = driveren styrer, 1 = manuell. Tom når den ikke kan leses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fan_policy: Vec<Option<u32>>,
    /// Viftens målfart per vifte, % (`nvmlDeviceGetTargetFanSpeed`, hvert 5. s).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fan_target: Vec<Option<f64>>,
    /// Videominne i MiB, fra `nvmlDeviceGetMemoryInfo_v2` (ikke v1).
    pub vram_used: Option<f64>,
    pub vram_total: Option<f64>,
    pub vram_reserved: Option<f64>,
    /// «P0» … «P12».
    pub pstate: Option<String>,
    /// PCIe-generasjon og bredde nå, fra sysfs.
    pub pcie_gen: Option<f64>,
    pub pcie_width: Option<f64>,
    /// Klokkebegrensninger (maske fra NVML). I P8 leses de ikke (kallet koster
    /// 25 ms), og da er masken «tomgang» og `throttle_inferred` sann.
    pub throttle: Option<u64>,
    pub throttle_inferred: bool,
    /// Energitelleren, mJ siden driveren ble lastet.
    pub energy_mj: Option<f64>,
    /// Verdier kortet ikke støtter (NOT_SUPPORTED), etter feltnavn.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub na: Vec<String>,
}

/// CPU-delen av en systemprøve.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CpuSample {
    pub load: Option<f64>,
    pub core_load: Vec<Option<f64>>,
    pub core_mhz: Vec<Option<f64>>,
    /// Aktive Linux-CPU-ID-er i prøven (sortert). Tom for eldre tjenester uten feltet.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub thread_ids: Vec<u32>,
    /// CPU-temperaturen som vises: Tdie når den finnes (maskinvare-revisjon kap. 7).
    pub temp: Option<f64>,
    /// Tctl (10 °C over Tdie på 2700X), står i verktøytipset.
    pub tctl: Option<f64>,
    pub governor: Option<String>,
    pub boost: Option<bool>,
}

impl CpuSample {
    /// Aktive Linux-CPU-ID-er i prøven: `thread_ids` når tjenesten sender dem,
    /// ellers indeksene som har last eller klokke (eldre tjenester).
    pub fn active_thread_ids(&self) -> Vec<usize> {
        if !self.thread_ids.is_empty() {
            return self.thread_ids.iter().filter_map(|&id| usize::try_from(id).ok()).collect();
        }
        let n = self.core_load.len().max(self.core_mhz.len());
        (0..n)
            .filter(|&i| {
                self.core_load.get(i).copied().flatten().is_some()
                    || self.core_mhz.get(i).copied().flatten().is_some()
            })
            .collect()
    }
}

/// Minne fra `/proc/meminfo`, i kB.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MemSample {
    #[serde(rename = "totalKB")]
    pub total_kb: Option<f64>,
    #[serde(rename = "availableKB")]
    pub available_kb: Option<f64>,
    #[serde(rename = "usedKB")]
    pub used_kb: Option<f64>,
    #[serde(rename = "freeKB")]
    pub free_kb: Option<f64>,
    #[serde(rename = "buffersKB")]
    pub buffers_kb: Option<f64>,
    #[serde(rename = "cachedKB")]
    pub cached_kb: Option<f64>,
    #[serde(rename = "shmemKB")]
    pub shmem_kb: Option<f64>,
    #[serde(rename = "sreclaimableKB")]
    pub sreclaimable_kb: Option<f64>,
    #[serde(rename = "swapTotalKB")]
    pub swap_total_kb: Option<f64>,
    #[serde(rename = "swapUsedKB")]
    pub swap_used_kb: Option<f64>,
}

/// Ett nettverkskort: rater i byte/s.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetSample {
    pub rx: Option<f64>,
    pub tx: Option<f64>,
    pub state: Option<String>,
    pub wireless: bool,
}

/// Én hwmon-verdi. `value` er i °C, o/min, V, W eller A.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HwmonSensor {
    /// «hwmon0/temp2» (som demoen).
    pub id: String,
    /// temp, fan, in, power, curr.
    pub kind: String,
    /// «temp2».
    pub key: String,
    pub label: String,
    pub value: Option<f64>,
    /// «root» når fila bare kan leses av root, ellers en kort feil.
    pub err: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HwmonDevice {
    pub id: String,
    pub name: String,
    pub sensors: Vec<HwmonSensor>,
}

/// Én prøve fra maskinen (`/proc` og `/sys`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SysSample {
    pub t: i64,
    pub cpu: CpuSample,
    pub mem: MemSample,
    pub net: BTreeMap<String, NetSample>,
    pub hwmon: Vec<HwmonDevice>,
    pub uptime: Option<f64>,
    pub loadavg: Vec<f64>,
    /// Diskene (1.1.0): MB/s, aktivitet og SMART-temperatur per disk.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disks: Vec<crate::disk::DiskSample>,
}

/// Hendelsen `telemetry`: `{t, gpus: {"<id>": Sample}, sys}`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Telemetry {
    pub t: i64,
    pub gpus: BTreeMap<GpuId, GpuSample>,
    pub sys: SysSample,
    /// Takten prøven ble tatt med, ms.
    pub interval_ms: u32,
}

/// Tilstanden til et kort (produktspesifikasjonen 3.8).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    #[default]
    Ok,
    DriverUpdated,
    NoDriver,
    Lost,
    Suspended,
}

impl DeviceState {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceState::Ok => "ok",
            DeviceState::DriverUpdated => "driver_updated",
            DeviceState::NoDriver => "no_driver",
            DeviceState::Lost => "lost",
            DeviceState::Suspended => "suspended",
        }
    }
}

/// Ett kort i `list_gpus` og i hendelsen `device`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GpuInfo {
    pub id: GpuId,
    pub index: u32,
    pub name: String,
    pub state: DeviceState,
    /// Når tilstanden sist ble endret (ms).
    pub since: i64,
    /// Driverversjonen som kjører, og den som er installert (ved `driver_updated`).
    pub driver: Option<String>,
    pub driver_installed: Option<String>,
    /// Kort grunn til tilstanden, med NVML-koden (til verktøytips og Om).
    pub detail: Option<String>,
}

/// Faste data for et kort som vinduet trenger straks (enhetskortet, Oversikt).
/// Det meste av Grafikkort-siden kommer fra `extra` (2B, `nvml_info.rs`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GpuStatic {
    pub id: GpuId,
    pub name: String,
    pub uuid: Option<String>,
    pub driver: Option<String>,
    pub nvml_version: Option<String>,
    pub cuda_version: Option<String>,
    pub vbios: Option<String>,
    pub pci_device_id: Option<u32>,
    pub pci_subsystem_id: Option<u32>,
    pub vram_total: Option<f64>,
    pub power_min: Option<f64>,
    pub power_max: Option<f64>,
    pub power_default: Option<f64>,
    pub core_max: Option<f64>,
    pub mem_max: Option<f64>,
    pub sm_max: Option<f64>,
    pub video_max: Option<f64>,
    pub fan_count: Option<u32>,
    pub fan_min: Option<f64>,
    pub fan_max: Option<f64>,
    pub temp_shutdown: Option<f64>,
    pub temp_slowdown: Option<f64>,
    pub temp_max: Option<f64>,
    pub temp_target: Option<f64>,
    pub pcie_gen_max: Option<f64>,
    pub pcie_width_max: Option<f64>,
    /// Alt annet (Grafikkort-siden, 2B), fritt format.
    pub extra: serde_json::Value,
}

/// Hvem som styrer viftene, ut fra `fan_policy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanControl {
    /// Policy 0 på alle viftene: driveren styrer (`TEMPERATURE_CONTINOUS_SW`).
    Driver,
    /// Minst én vifte er satt til fast fart (utenfor appen).
    Manual,
    /// Ikke lest (ennå), eller kortet støtter det ikke.
    Unknown,
}

pub fn fan_control(s: &GpuSample) -> FanControl {
    let known: Vec<u32> = s.fan_policy.iter().flatten().copied().collect();
    if known.is_empty() {
        FanControl::Unknown
    } else if known.iter().all(|p| *p == 0) {
        FanControl::Driver
    } else {
        FanControl::Manual
    }
}

/// Navnet uten «NVIDIA GeForce» foran, som demoens `shortGpuName`.
pub fn short_gpu_name(name: &str) -> String {
    let n = name.trim();
    for p in ["NVIDIA GeForce ", "NVIDIA ", "GeForce "] {
        if let Some(r) = n.strip_prefix(p) {
            return r.to_string();
        }
    }
    n.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prøven_har_demoens_feltnavn() {
        let s = GpuSample { temp: Some(40.0), gpu_util: Some(3.0), vram_used: Some(674.0), ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["temp"], 40.0);
        assert_eq!(v["gpuUtil"], 3.0);
        assert_eq!(v["vramUsed"], 674.0);
        let m = MemSample { used_kb: Some(1.0), ..Default::default() };
        assert_eq!(serde_json::to_value(&m).unwrap()["usedKB"], 1.0);
        assert_eq!(serde_json::to_value(DeviceState::DriverUpdated).unwrap(), "driver_updated");
        assert!(v.get("fanPolicy").is_none(), "tom viftestyring sendes ikke");
        let s = GpuSample { fan_policy: vec![Some(0), Some(1)], fan_target: vec![Some(35.0), None], ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["fanPolicy"], serde_json::json!([0, 1]));
        assert_eq!(v["fanTarget"], serde_json::json!([35.0, null]));
    }

    #[test]
    fn viftestyringen_som_tekst() {
        let auto = GpuSample { fan_policy: vec![Some(0), Some(0)], ..Default::default() };
        assert_eq!(fan_control(&auto), FanControl::Driver);
        let man = GpuSample { fan_policy: vec![Some(0), Some(1)], ..Default::default() };
        assert_eq!(fan_control(&man), FanControl::Manual);
        assert_eq!(fan_control(&GpuSample::default()), FanControl::Unknown);
    }

    #[test]
    fn kort_navn() {
        assert_eq!(short_gpu_name("NVIDIA GeForce RTX 2080"), "RTX 2080");
        assert_eq!(short_gpu_name("Quadro P4000"), "Quadro P4000");
    }

    #[test]
    fn cpu_sample_active_thread_ids_og_serialisering() {
        let empty = CpuSample::default();
        assert!(serde_json::to_value(&empty).unwrap().get("threadIds").is_none());
        assert!(empty.active_thread_ids().is_empty());

        let with_ids = CpuSample { thread_ids: vec![0, 8], core_load: vec![Some(10.0), None, None, None, None, None, None, None, None], ..Default::default() };
        let v = serde_json::to_value(&with_ids).unwrap();
        assert_eq!(v["threadIds"], serde_json::json!([0, 8]));
        assert_eq!(with_ids.active_thread_ids(), vec![0, 8]);

        // Eldre tjeneste uten threadIds: utleder aktive ID-er fra last/klokke.
        let legacy: CpuSample = serde_json::from_value(serde_json::json!({
            "coreLoad": [12.0, null, null, null, null, null, null, null, 45.0],
            "coreMhz": [2000.0, null, null, null, null, null, null, null, 3600.0]
        }))
        .unwrap();
        assert!(legacy.thread_ids.is_empty());
        assert_eq!(legacy.active_thread_ids(), vec![0, 8]);
    }
}
