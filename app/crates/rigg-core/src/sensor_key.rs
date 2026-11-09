//! Sensornøkler (arkitektur.md kap. 5). Samme nøkkel brukes i historikk,
//! statistikk, logg, festede sensorer og alarmer:
//!
//! - `gpu:<pci-buss>/<felt>`: `gpu:0000:1c:00.0/temp`, `…/fan1`, `…/fanRpm0`
//! - `cpu/load`, `cpu/temp` (Tdie), `cpu/tctl`, `cpu/thread/3/load`, `cpu/thread/3/mhz`
//! - `mem/usedPct`, `mem/used`, `mem/avail`, `mem/swap` (GiB)
//! - `net/rx`, `net/tx` (sum over fysiske kort), `net/<grensesnitt>/rx`
//! - `hwmon/<navn>/<fil>`: `hwmon/k10temp/temp1`
//! - `disk/<serie>/read|write|busy|temp` (byte/s, %, °C) og `disk/read|write` (sum)
//!
//! Samme mønster som demoens `KEY_RE`: `^[A-Za-z0-9_.:/-]{1,80}$`.

use crate::model::{GpuSample, Telemetry};
use crate::sensormeta;

pub fn valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= 80 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:/-".contains(&b))
}

pub fn gpu(id: &str, field: &str) -> String {
    format!("gpu:{id}/{field}")
}

/// Kortet en GPU-nøkkel gjelder, og feltet: `("0000:1c:00.0", "temp")`.
pub fn split_gpu(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix("gpu:")?;
    let i = rest.rfind('/')?;
    Some((&rest[..i], &rest[i + 1..]))
}

/// Diskene som blir serier (fire serier hver, pluss summen).
pub const MAX_DISKS: usize = 8;

/// Disken en disknøkkel gjelder, og feltet: `disk/WFL454P6/temp` → `("WFL454P6", "temp")`.
pub fn split_disk(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix("disk/")?;
    let (id, f) = rest.rsplit_once('/')?;
    (!id.is_empty() && !id.contains('/')).then_some((id, f))
}

/// Feltene i en GPU-prøve som blir serier (i denne rekkefølgen).
pub const GPU_FIELDS: &[&str] = &[
    "temp", "gpuUtil", "memUtil", "encUtil", "decUtil", "coreClock", "smClock", "memClock", "videoClock", "power", "powerLimit", "fan", "vramUsed",
    "pcieGen",
];

pub fn gpu_field(s: &GpuSample, field: &str) -> Option<f64> {
    match field {
        "temp" => s.temp,
        "gpuUtil" => s.gpu_util,
        "memUtil" => s.mem_util,
        "encUtil" => s.enc_util,
        "decUtil" => s.dec_util,
        "coreClock" => s.core_clock,
        "smClock" => s.sm_clock,
        "memClock" => s.mem_clock,
        "videoClock" => s.video_clock,
        "power" => s.power,
        "powerLimit" => s.power_limit,
        "fan" => s.fan,
        "vramUsed" => s.vram_used,
        "pcieGen" => s.pcie_gen,
        "pcieWidth" => s.pcie_width,
        f => {
            if let Some(i) = f.strip_prefix("fanRpm").and_then(|n| n.parse::<usize>().ok()) {
                s.fan_rpm.get(i).copied().flatten()
            } else if let Some(i) = f.strip_prefix("fan").and_then(|n| n.parse::<usize>().ok()) {
                s.fans.get(i).copied().flatten()
            } else {
                None
            }
        }
    }
}

/// Gjør en prøve om til `(nøkkel, verdi)` for historikk, statistikk og logg.
/// Verdier som mangler, er ikke med.
pub fn flatten(t: &Telemetry) -> Vec<(String, f64)> {
    let mut out = Vec::with_capacity(96);
    let mut put = |k: String, v: Option<f64>| {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            out.push((k, v));
        }
    };
    for (id, s) in &t.gpus {
        for f in GPU_FIELDS {
            put(gpu(id, f), gpu_field(s, f));
        }
        if s.fans.len() > 1 {
            for (i, v) in s.fans.iter().enumerate() {
                put(gpu(id, &format!("fan{i}")), *v);
            }
        }
        for (i, v) in s.fan_rpm.iter().enumerate() {
            put(gpu(id, &format!("fanRpm{i}")), *v);
        }
    }
    let sys = &t.sys;
    put("cpu/load".into(), sys.cpu.load);
    put("cpu/temp".into(), sensormeta::cpu_temp(sys));
    put("cpu/tctl".into(), sys.cpu.tctl);
    for (i, v) in sys.cpu.core_load.iter().enumerate().take(64) {
        put(format!("cpu/thread/{i}/load"), *v);
    }
    for (i, v) in sys.cpu.core_mhz.iter().enumerate().take(64) {
        put(format!("cpu/thread/{i}/mhz"), *v);
    }
    const GIB: f64 = 1024.0 * 1024.0;
    put("mem/usedPct".into(), sensormeta::mem_used_pct(sys));
    put("mem/used".into(), sys.mem.used_kb.map(|k| k / GIB));
    put("mem/avail".into(), sys.mem.available_kb.map(|k| k / GIB));
    put("mem/swap".into(), sys.mem.swap_used_kb.map(|k| k / GIB));
    let (rx, tx) = sensormeta::net_totals(sys);
    put("net/rx".into(), rx);
    put("net/tx".into(), tx);
    for (name, n) in &sys.net {
        put(format!("net/{name}/rx"), n.rx);
        put(format!("net/{name}/tx"), n.tx);
    }
    // Diskene: `disk/<id>/read|write|busy|temp` og summen `disk/read|write`
    // (høyst 8 disker, så historikken har plass).
    let (mut rd, mut wr): (Option<f64>, Option<f64>) = (None, None);
    for d in sys.disks.iter().take(MAX_DISKS) {
        let id = &d.id;
        put(format!("disk/{id}/read"), d.read);
        put(format!("disk/{id}/write"), d.write);
        put(format!("disk/{id}/busy"), d.busy);
        put(format!("disk/{id}/temp"), d.temp);
        if let Some(v) = d.read {
            rd = Some(rd.unwrap_or(0.0) + v);
        }
        if let Some(v) = d.write {
            wr = Some(wr.unwrap_or(0.0) + v);
        }
    }
    put("disk/read".into(), rd);
    put("disk/write".into(), wr);
    let mut seen: Vec<&str> = Vec::new();
    for d in &sys.hwmon {
        // To enheter med samme navn får _2, _3 … (stabil rekkefølge fra hwmon-nummeret).
        let n = seen.iter().filter(|x| **x == d.name).count();
        seen.push(&d.name);
        let name = if n == 0 { d.name.clone() } else { format!("{}_{}", d.name, n + 1) };
        for x in &d.sensors {
            put(format!("hwmon/{name}/{}", x.key), x.value);
        }
    }
    out.retain(|(k, _)| valid(k));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn nøkler() {
        assert!(valid("gpu:0000:1c:00.0/temp"));
        assert!(valid("hwmon/k10temp/temp1"));
        assert!(!valid("gpu/a b"));
        assert!(!valid(""));
        assert_eq!(split_gpu("gpu:0000:1c:00.0/temp"), Some(("0000:1c:00.0", "temp")));
    }

    #[test]
    fn flatten_tar_med_alt_som_finnes() {
        let mut t = Telemetry::default();
        t.gpus.insert(
            "0000:1c:00.0".into(),
            GpuSample { temp: Some(40.0), fans: vec![Some(35.0), Some(36.0)], fan_rpm: vec![Some(801.0), None], ..Default::default() },
        );
        t.sys.cpu.load = Some(5.0);
        t.sys.cpu.core_load = vec![Some(1.0), Some(2.0)];
        t.sys.hwmon = vec![
            HwmonDevice { id: "hwmon0".into(), name: "k10temp".into(), sensors: vec![HwmonSensor { key: "temp2".into(), label: "Tdie".into(), kind: "temp".into(), id: "hwmon0/temp2".into(), value: Some(37.0), err: None }] },
            HwmonDevice { id: "hwmon5".into(), name: "k10temp".into(), sensors: vec![HwmonSensor { key: "temp1".into(), value: Some(1.0), ..Default::default() }] },
        ];
        let v: std::collections::HashMap<_, _> = flatten(&t).into_iter().collect();
        assert_eq!(v["gpu:0000:1c:00.0/temp"], 40.0);
        assert_eq!(v["gpu:0000:1c:00.0/fan1"], 36.0);
        assert_eq!(v["gpu:0000:1c:00.0/fanRpm0"], 801.0);
        assert!(!v.contains_key("gpu:0000:1c:00.0/fanRpm1"));
        assert_eq!(v["cpu/thread/1/load"], 2.0);
        assert_eq!(v["cpu/temp"], 37.0);
        assert_eq!(v["hwmon/k10temp/temp2"], 37.0);
        assert_eq!(v["hwmon/k10temp_2/temp1"], 1.0);
    }

    #[test]
    fn disker_blir_serier_med_sum() {
        let mut t = Telemetry::default();
        t.sys.disks = vec![
            crate::disk::DiskSample { id: "WFL454P6".into(), name: "sda".into(), read: Some(1e6), write: Some(0.0), busy: Some(3.0), temp: Some(35.0), ..Default::default() },
            crate::disk::DiskSample { id: "C181960464".into(), name: "sdc".into(), read: Some(2e6), write: Some(5e5), busy: None, temp: None, ..Default::default() },
        ];
        let v: std::collections::HashMap<_, _> = flatten(&t).into_iter().collect();
        assert_eq!(v["disk/WFL454P6/temp"], 35.0);
        assert_eq!(v["disk/C181960464/read"], 2e6);
        assert!(!v.contains_key("disk/C181960464/temp"));
        assert_eq!(v["disk/read"], 3e6);
        assert_eq!(v["disk/write"], 5e5);
        assert_eq!(split_disk("disk/WFL454P6/temp"), Some(("WFL454P6", "temp")));
        assert_eq!(split_disk("disk/read"), None);
    }
}
