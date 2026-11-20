//! Menneskelige navn på hwmon-enheter, sensorer og nettverkskort (port av
//! `web/js/sensormeta.js`), og de avledede verdiene statusblokken og grafene
//! bruker. Rettelse fra maskinvarerevisjonen kap. 7: CPU-temperaturen er
//! **Tdie** når den finnes, ikke Tctl.

use crate::model::{HwmonDevice, HwmonSensor, SysSample};
use crate::{tr, trf};

const DEVICES: &[(&str, fn() -> &'static str)] = &[
    ("k10temp", || tr!("Prosessor")),
    ("coretemp", || tr!("Prosessor")),
    ("zenpower", || tr!("Prosessor")),
    ("iwlwifi", || tr!("Wi-Fi-kort")),
    ("ucsi_source_psy", || tr!("USB-C-kontroller (skjermkort)")),
    ("hidpp_battery", || tr!("Batteri i trådløs mus/tastatur")),
    ("nvme", || tr!("NVMe-disk")),
    ("drivetemp", || tr!("Disk")),
    ("amdgpu", || tr!("AMD-grafikk")),
    ("nouveau", || tr!("Skjermkort (nouveau)")),
    ("acpitz", || tr!("ACPI-termisk sone")),
    ("spd5118", || tr!("Minnemodul")),
    ("jc42", || tr!("Minnemodul")),
    ("cros_ec", || tr!("Innebygd kontroller")),
];

fn starts_digits(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

pub fn is_motherboard(raw: &str) -> bool {
    ["nct", "it", "f71", "w83"].iter().any(|p| starts_digits(raw, p))
        || ["asus", "gigabyte_wmi", "dell_smm", "thinkpad"].iter().any(|p| raw.starts_with(p))
}

/// «Prosessor» for k10temp, ellers det rå navnet.
pub fn device_name(raw: &str) -> String {
    let exact = ["k10temp", "coretemp", "zenpower", "nvme", "drivetemp", "amdgpu", "nouveau", "acpitz", "spd5118", "jc42"];
    for (k, v) in DEVICES {
        let hit = if exact.contains(k) { raw == *k } else { raw.starts_with(k) };
        if hit {
            return v().to_string();
        }
    }
    let ath = raw.strip_prefix("ath").is_some_and(|r| {
        let rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
        rest.len() < r.len() && rest.starts_with('k')
    });
    if ath || starts_digits(raw, "mt7") {
        return tr!("Wi-Fi-kort").into();
    }
    if is_motherboard(raw) {
        return tr!("Hovedkort").into();
    }
    if raw.to_ascii_lowercase().starts_with("battery") || starts_digits(raw, "BAT") {
        return tr!("Batteri").into();
    }
    if ["r8169", "igb", "e1000"].iter().any(|p| raw.starts_with(p)) {
        return tr!("Nettverkskort").into();
    }
    raw.to_string()
}

fn kind_name(kind: &str) -> &str {
    match kind {
        "temp" => tr!("temperatur"),
        "fan" => tr!("vifte"),
        "in" => tr!("spenning"),
        "power" => tr!("effekt"),
        "curr" => tr!("strøm"),
        other => other,
    }
}

/// Navn på én sensor: «Prosessor · Tdie» eller «USB-C-kontroller (skjermkort) · strøm».
/// Gir `(navn, rått navn til verktøytipset)`.
pub fn sensor_name(dev: &HwmonDevice, x: &HwmonSensor) -> (String, String) {
    let d = device_name(&dev.name);
    let generic = x.label.is_empty() || x.label == x.key || is_generic_label(&x.label);
    let label = if generic { kind_name(&x.kind).to_string() } else { x.label.clone() };
    let raw = format!("{} · {} ({})", dev.name, x.key, x.id.split('/').next().unwrap_or(""));
    (format!("{d} · {label}"), raw)
}

fn is_generic_label(l: &str) -> bool {
    ["temp", "fan", "in", "power", "curr"].iter().any(|p| starts_digits(l, p) && l[p.len()..].chars().all(|c| c.is_ascii_digit()))
}

const VIRTUAL: &[&str] = &["docker", "veth", "br-", "virbr", "vnet", "tap", "tun", "wg", "lxc", "lxd", "vmnet", "zt"];

pub fn is_physical_net(name: &str) -> bool {
    !VIRTUAL.iter().any(|p| name.starts_with(p)) && name != "lo"
}

/// «Wi-Fi (wlo1)», «Ethernet (enp24s0)».
pub fn net_name(name: &str, wireless: bool) -> String {
    if !is_physical_net(name) {
        return trf!("Virtuelt ({navn})", navn = name);
    }
    if wireless || name.starts_with("wl") {
        return trf!("Wi-Fi ({navn})", navn = name);
    }
    if name.starts_with("en") || name.starts_with("eth") {
        return trf!("Ethernet ({navn})", navn = name);
    }
    if name.starts_with("ww") {
        return trf!("Mobilnett ({navn})", navn = name);
    }
    name.to_string()
}

/// CPU-temperaturen som vises: Tdie (AMD), Package (Intel) eller første
/// temperatur. Tctl bare når Tdie mangler, for da er forskyvningen 0.
pub fn cpu_temp_sensor(sys: &SysSample) -> Option<&HwmonSensor> {
    let dev = sys.hwmon.iter().find(|d| matches!(d.name.as_str(), "k10temp" | "coretemp" | "zenpower"))?;
    let by = |l: &str| dev.sensors.iter().find(|x| x.label == l);
    by("Tdie")
        .or_else(|| by("Tctl"))
        .or_else(|| dev.sensors.iter().find(|x| x.label.contains("Package")))
        .or_else(|| dev.sensors.iter().find(|x| x.kind == "temp"))
}

pub fn cpu_temp(sys: &SysSample) -> Option<f64> {
    sys.cpu.temp.or_else(|| cpu_temp_sensor(sys).and_then(|s| s.value))
}

/// Tctl når den finnes (til verktøytipset).
pub fn cpu_tctl(sys: &SysSample) -> Option<f64> {
    let dev = sys.hwmon.iter().find(|d| d.name == "k10temp")?;
    dev.sensors.iter().find(|x| x.label == "Tctl").and_then(|x| x.value)
}

/// Sum ned/opp over fysiske nettverkskort (byte/s).
pub fn net_totals(sys: &SysSample) -> (Option<f64>, Option<f64>) {
    let mut rx: Option<f64> = None;
    let mut tx: Option<f64> = None;
    for (name, n) in &sys.net {
        if !is_physical_net(name) {
            continue;
        }
        if let Some(v) = n.rx {
            rx = Some(rx.unwrap_or(0.0) + v);
        }
        if let Some(v) = n.tx {
            tx = Some(tx.unwrap_or(0.0) + v);
        }
    }
    (rx, tx)
}

pub fn mem_used_pct(sys: &SysSample) -> Option<f64> {
    match (sys.mem.used_kb, sys.mem.total_kb) {
        (Some(u), Some(t)) if t > 0.0 => Some(u / t * 100.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn k10() -> SysSample {
        SysSample {
            hwmon: vec![HwmonDevice {
                id: "hwmon0".into(),
                name: "k10temp".into(),
                sensors: vec![
                    HwmonSensor { id: "hwmon0/temp1".into(), kind: "temp".into(), key: "temp1".into(), label: "Tctl".into(), value: Some(47.375), err: None },
                    HwmonSensor { id: "hwmon0/temp2".into(), kind: "temp".into(), key: "temp2".into(), label: "Tdie".into(), value: Some(37.375), err: None },
                ],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn cpu_temp_er_tdie() {
        let s = k10();
        assert_eq!(cpu_temp(&s), Some(37.375));
        assert_eq!(cpu_tctl(&s), Some(47.375));
    }

    #[test]
    fn navn() {
        assert_eq!(device_name("k10temp"), "Prosessor");
        assert_eq!(device_name("ucsi_source_psy_10_00081"), "USB-C-kontroller (skjermkort)");
        assert_eq!(device_name("nct6775"), "Hovedkort");
        assert_eq!(device_name("ukjent"), "ukjent");
        assert_eq!(net_name("wlo1", true), "Wi-Fi (wlo1)");
        assert_eq!(net_name("enp24s0", false), "Ethernet (enp24s0)");
        assert_eq!(net_name("docker0", false), "Virtuelt (docker0)");
        let s = k10();
        let (n, raw) = sensor_name(&s.hwmon[0], &s.hwmon[0].sensors[1]);
        assert_eq!(n, "Prosessor · Tdie");
        assert_eq!(raw, "k10temp · temp2 (hwmon0)");
    }

    #[test]
    fn nett_og_minne() {
        let mut s = SysSample::default();
        s.net.insert("eth0".into(), NetSample { rx: Some(100.0), tx: Some(50.0), ..Default::default() });
        s.net.insert("docker0".into(), NetSample { rx: Some(9.0), tx: Some(9.0), ..Default::default() });
        assert_eq!(net_totals(&s), (Some(100.0), Some(50.0)));
        s.mem.used_kb = Some(1024.0);
        s.mem.total_kb = Some(4096.0);
        assert_eq!(mem_used_pct(&s), Some(25.0));
    }
}
