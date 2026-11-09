//! PCI-data fra sysfs: ID-er, revisjon, lenke, BAR-størrelser og Resizable
//! BAR, og brikkesettet fra `pci.ids`. Åpner aldri `config`, `reset`,
//! `remove`, `rescan` eller `rom` (port av `readPciGpu` i `server/system.js`).

use serde_json::{Value, json};
use std::path::Path;

fn txt(p: &Path) -> Option<String> {
    crate::read_text(p).filter(|s| !s.is_empty())
}

/// «0x10de» → «10de».
fn hex(p: &Path) -> Option<String> {
    txt(p).map(|s| s.trim_start_matches("0x").to_ascii_lowercase())
}

/// BAR-størrelsene fra `resource` (én linje per BAR: start, slutt, flagg).
pub fn bar_sizes(resource: &str) -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    for (i, line) in resource.lines().take(6).enumerate() {
        let p: Vec<u64> = line.split_whitespace().filter_map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).ok()).collect();
        if p.len() >= 2 && p[1] > p[0] {
            out.push((i, p[1] - p[0] + 1));
        }
    }
    out
}

/// `resourceN_resize`: bit n betyr at 2ⁿ MiB støttes. Gir størrelsene i MiB.
pub fn resize_sizes(mask: &str) -> Vec<u64> {
    let Ok(m) = u64::from_str_radix(mask.trim().trim_start_matches("0x"), 16) else { return vec![] };
    (0..40).filter(|n| m >> n & 1 == 1).map(|n| 1u64 << n).collect()
}

/// `TOTAL_ERR_COR 0` fra `aer_dev_correctable` (og tilsvarende).
fn aer_total(text: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.rsplit_once(' ')?;
        k.starts_with("TOTAL_ERR").then(|| v.trim().parse().ok()).flatten()
    })
}

/// Skjermkortet på `addr` («0000:1c:00.0») som JSON.
pub fn read_gpu(root: &Path, addr: &str) -> Value {
    let d = crate::gpu::pci_dir(root, addr);
    if !d.join("vendor").exists() {
        return Value::Null;
    }
    let bars: Vec<Value> = bar_sizes(&txt(&d.join("resource")).unwrap_or_default()).into_iter().map(|(bar, size)| json!({"bar": bar, "size": size})).collect();
    let bar1_sizes = txt(&d.join("resource1_resize")).map(|s| resize_sizes(&s));
    let driver = std::fs::read_link(d.join("driver")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    json!({
        "addr": addr,
        "vendor": hex(&d.join("vendor")),
        "device": hex(&d.join("device")),
        "subVendor": hex(&d.join("subsystem_vendor")),
        "subDevice": hex(&d.join("subsystem_device")),
        "revision": hex(&d.join("revision")),
        "class": hex(&d.join("class")),
        "linkSpeed": txt(&d.join("current_link_speed")),
        "linkSpeedMax": txt(&d.join("max_link_speed")),
        "linkWidth": txt(&d.join("current_link_width")).and_then(|s| s.parse::<u32>().ok()),
        "linkWidthMax": txt(&d.join("max_link_width")).and_then(|s| s.parse::<u32>().ok()),
        "bars": bars,
        "bar1Sizes": bar1_sizes,
        "driver": driver,
        "powerState": txt(&d.join("power_state")),
        "bootVga": txt(&d.join("boot_vga")).map(|s| s == "1"),
        "aerCorrectable": txt(&d.join("aer_dev_correctable")).and_then(|t| aer_total(&t)),
        "aerNonfatal": txt(&d.join("aer_dev_nonfatal")).and_then(|t| aer_total(&t)),
        "aerFatal": txt(&d.join("aer_dev_fatal")).and_then(|t| aer_total(&t)),
    })
}

/// Navnet på en enhet i `pci.ids` («400 Series Chipset SATA Controller»).
/// Leser bare seksjonen til leverandøren.
pub fn pci_name(ids: &str, vendor: &str, device: &str) -> (Option<String>, Option<String>) {
    let mut vname = None;
    let mut in_vendor = false;
    for line in ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if in_vendor {
                break;
            }
            if line.len() > 4 && line[..4].eq_ignore_ascii_case(vendor) {
                in_vendor = true;
                vname = Some(line[4..].trim().to_string());
            }
            continue;
        }
        if in_vendor && !line.starts_with("\t\t") {
            let l = &line[1..];
            if l.len() > 4 && l[..4].eq_ignore_ascii_case(device) {
                return (vname, Some(l[4..].trim().to_string()));
            }
        }
    }
    (vname, None)
}

/// «AMD 400 Series Chipset» fra «400 Series Chipset SATA Controller».
pub fn chipset_from(vendor_name: &str, dev_name: &str) -> Option<String> {
    let i = dev_name.find("Chipset")?;
    let v = vendor_name.split_whitespace().find(|w| ["AMD", "Intel", "ASMedia", "NVIDIA", "VIA"].contains(w)).unwrap_or(vendor_name);
    let short = vendor_name.split('[').nth(1).and_then(|s| s.split(']').next()).unwrap_or(v);
    let short = if short.contains("AMD") { "AMD" } else { short };
    Some(format!("{short} {}", dev_name[..i + "Chipset".len()].trim()))
}

/// Slaget kontroller ut fra PCI-klassen, for de klassene System-siden viser.
pub fn controller_kind(class: &str) -> Option<&'static str> {
    let c = class.get(..4)?;
    Some(match c {
        "0200" => "Ethernet",
        "0280" => "Trådløst nett",
        "0403" | "0401" => "Lyd",
        "0c03" => "USB",
        "0106" => "SATA",
        "0108" => "NVMe",
        "0c0a" => "Thunderbolt",
        _ => return None,
    })
}

/// Kortere enhetsnavn til tabellen: «400 Series Chipset USB 3.1 xHCI
/// Compliant Host Controller» → «400 Series Chipset USB 3.1 xHCI»,
/// «FCH SATA Controller [AHCI mode]» → «FCH SATA», «Family 17h (Models
/// 00h-0fh) HD Audio Controller» → «Family 17h HD Audio».
pub fn short_device(n: &str) -> String {
    let mut out = String::with_capacity(n.len());
    let (mut square, mut paren) = (0, 0);
    for (i, c) in n.char_indices() {
        match c {
            '[' => square += 1,
            ']' if square > 0 => square -= 1,
            '(' if n[i..].starts_with("(Models") || n[i..].starts_with("(rev") => paren += 1,
            ')' if paren > 0 => paren -= 1,
            _ if square == 0 && paren == 0 => out.push(c),
            _ => {}
        }
    }
    for p in [" Compliant Host Controller", " Host Controller", " Controller", "PCI Express "] {
        out = out.replace(p, "");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// «Advanced Micro Devices, Inc. [AMD]» → «AMD»; «Intel Corporation» → «Intel».
pub fn short_vendor(v: &str) -> String {
    if let Some(b) = v.split('[').nth(1).and_then(|s| s.split(']').next()) {
        return b.split('/').next().unwrap_or(b).trim().to_string();
    }
    v.split_whitespace().next().unwrap_or(v).trim_end_matches(',').to_string()
}

/// Brikkesett og andre PCI-enheter på hovedkortet.
pub fn read_board(root: &Path) -> Value {
    let dir = root.join("sys/bus/pci/devices");
    let Ok(rd) = std::fs::read_dir(&dir) else { return json!({}) };
    let mut devs: Vec<(String, String, String, String)> = Vec::new();
    for e in rd.flatten() {
        let d = e.path();
        let (Some(v), Some(dv), Some(c)) = (hex(&d.join("vendor")), hex(&d.join("device")), hex(&d.join("class"))) else { continue };
        devs.push((e.file_name().to_string_lossy().into_owned(), v, dv, c));
    }
    devs.sort();
    let ids = ["usr/share/hwdata/pci.ids", "usr/share/misc/pci.ids", "usr/share/pci.ids"].iter().find_map(|p| std::fs::read_to_string(root.join(p)).ok()).unwrap_or_default();
    let mut chipset = None;
    let mut chipset_ids = Vec::new();
    // Kandidatene: ISA/LPC-bro, SATA og USB fra brikkesettprodusentene.
    for (_, v, d, c) in &devs {
        if !["1022", "8086", "1b21"].contains(&v.as_str()) || !(c.starts_with("0601") || c.starts_with("0106") || c.starts_with("0c03") || c.starts_with("0604")) {
            continue;
        }
        let (vn, dn) = pci_name(&ids, v, d);
        if let (Some(vn), Some(dn)) = (vn, dn)
            && let Some(cs) = chipset_from(&vn, &dn)
        {
            chipset_ids.push(format!("{v}:{d}"));
            chipset.get_or_insert(cs);
        }
    }
    chipset_ids.dedup();
    // Kontrollerne på hovedkortet og i sporene (System-siden, runde 6A):
    // nettverk, lyd, USB, SATA og NVMe, med navnet fra pci.ids og driveren.
    let mut controllers = Vec::new();
    for (addr, v, d, c) in &devs {
        let Some(kind) = controller_kind(c) else { continue };
        let (vn, dn) = pci_name(&ids, v, d);
        let driver = std::fs::read_link(dir.join(addr).join("driver")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        controllers.push(json!({
            "addr": addr,
            "kind": kind,
            "vendor": vn.as_deref().map(short_vendor),
            "name": dn.as_deref().map(short_device),
            "ids": format!("{v}:{d}"),
            "driver": driver,
        }));
    }
    json!({
        "chipset": chipset,
        "chipsetIds": chipset_ids,
        "controllers": controllers,
        "devices": devs.len(),
        "source": if ids.is_empty() { None } else { Some("pci.ids") },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_og_resize() {
        let r = "0x00000000f6000000 0x00000000f6ffffff 0x0000000000040200\n0x00000000e0000000 0x00000000efffffff 0x000000000014220c\n0x0 0x0 0x0\n";
        assert_eq!(bar_sizes(r), vec![(0, 16 << 20), (1, 256 << 20)]);
        assert_eq!(resize_sizes("00000000000001c0"), vec![64, 128, 256]);
        assert_eq!(aer_total("RxErr 0\nTOTAL_ERR_COR 3\n"), Some(3));
    }

    #[test]
    fn brikkesett_fra_pci_ids() {
        let ids = "# kommentar\n1022  Advanced Micro Devices, Inc. [AMD]\n\t43c6  400 Series Chipset PCIe Bridge\n\t43c8  400 Series Chipset SATA Controller\n\t\t1462 7b85  MSI-kort\n1023  Trident\n\t43c8  Noe annet\n";
        let (v, d) = pci_name(ids, "1022", "43c8");
        assert_eq!(d.as_deref(), Some("400 Series Chipset SATA Controller"));
        assert_eq!(chipset_from(&v.unwrap(), &d.unwrap()).as_deref(), Some("AMD 400 Series Chipset"));
        assert_eq!(pci_name(ids, "1022", "ffff").1, None);
        assert_eq!(short_vendor("Advanced Micro Devices, Inc. [AMD]"), "AMD");
        assert_eq!(short_vendor("Advanced Micro Devices, Inc. [AMD/ATI]"), "AMD");
        assert_eq!(short_vendor("Intel Corporation"), "Intel");
        assert_eq!(short_vendor("Realtek Semiconductor Co., Ltd."), "Realtek");
        assert_eq!(controller_kind("0x020000".trim_start_matches("0x")), Some("Ethernet"));
        assert_eq!(controller_kind("040300"), Some("Lyd"));
        assert_eq!(controller_kind("030000"), None);
        assert_eq!(short_device("400 Series Chipset USB 3.1 xHCI Compliant Host Controller"), "400 Series Chipset USB 3.1 xHCI");
        assert_eq!(short_device("FCH SATA Controller [AHCI mode]"), "FCH SATA");
        assert_eq!(short_device("Family 17h (Models 00h-0fh) HD Audio Controller"), "Family 17h HD Audio");
        assert_eq!(short_device("Wi-Fi 5(802.11ac) Wireless-AC 9x6x [Thunder Peak]"), "Wi-Fi 5(802.11ac) Wireless-AC 9x6x");
        assert_eq!(short_device("I211 Gigabit Network Connection"), "I211 Gigabit Network Connection");
    }

    #[test]
    fn gpu_fra_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maskin");
        let v = read_gpu(&root, "0000:1c:00.0");
        assert_eq!(v["vendor"], "10de");
        assert_eq!(v["linkWidthMax"], 16);
        assert!(read_gpu(&root, "0000:99:00.0").is_null());
    }
}
