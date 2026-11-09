//! Diskene (1.1.0): hastighet og aktivitet fra `/proc/diskstats` (én `pread`
//! per runde), og de faste dataene fra sysfs, udev-databasen
//! (`/run/udev/data/b<maj>:<min>`, lesbar for alle) og `statvfs`.
//!
//! SMART kommer fra UDisks2 (`udisks.rs`), som leser den selv omtrent hvert
//! 10. min uten å vekke disker i hvile. Her åpnes aldri `/dev/sd*`, og ingen
//! SMART-kommando sendes. Uten UDisks2 (den private økta har en tom
//! systembuss) står resten av siden likevel.

use crate::PreadFile;
use rigg_core::disk::{Disk, DiskList, DiskSample, Partition};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn txt(p: &Path) -> Option<String> {
    crate::read_text(p).filter(|s| !s.is_empty())
}

/// Blokkenheter som ikke er fysiske disker.
pub fn is_virtual(name: &str) -> bool {
    ["loop", "ram", "zram", "dm-", "md", "sr", "fd", "nbd", "zd"].iter().any(|p| name.starts_with(p))
}

// ---------------------------------------------------------------- /proc/diskstats

/// Tellerne vi bruker fra én linje: sektorer lest og skrevet (alltid 512 byte)
/// og millisekunder med I/O i gang.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub read_sectors: u64,
    pub write_sectors: u64,
    pub io_ms: u64,
}

/// `major minor navn` og feltene etter (kernel Documentation/admin-guide/iostats).
pub fn parse_diskstats(text: &str) -> Vec<(String, Counters)> {
    let mut out = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 13 {
            continue;
        }
        let n = |i: usize| f[i].parse::<u64>().unwrap_or(0);
        out.push((f[2].to_string(), Counters { read_sectors: n(5), write_sectors: n(9), io_ms: n(12) }));
    }
    out
}

/// Rater mellom to lesinger: (les byte/s, skriv byte/s, aktivitet %).
pub fn rates(a: Counters, b: Counters, dt_ms: i64) -> (f64, f64, f64) {
    if dt_ms <= 0 {
        return (0.0, 0.0, 0.0);
    }
    let s = dt_ms as f64 / 1000.0;
    let r = b.read_sectors.saturating_sub(a.read_sectors) as f64 * 512.0 / s;
    let w = b.write_sectors.saturating_sub(a.write_sectors) as f64 * 512.0 / s;
    let busy = (b.io_ms.saturating_sub(a.io_ms) as f64 / dt_ms as f64 * 100.0).clamp(0.0, 100.0);
    (r, w, busy)
}

// ---------------------------------------------------------------- udev og sysfs

/// `E:NØKKEL=verdi`-linjene i udev-databasen for en blokkenhet.
pub fn udev(root: &Path, dev: &str) -> HashMap<String, String> {
    let Some(text) = std::fs::read_to_string(root.join("run/udev/data").join(format!("b{dev}"))).ok() else { return HashMap::new() };
    text.lines().filter_map(|l| l.strip_prefix("E:")).filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// `\x20` og andre `\xHH` i udevs `*_ENC`-felt.
pub fn udev_unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1] == b'x'
            && let Some(v) = std::str::from_utf8(&b[i + 2..i + 4]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 4;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// «6.0 Gbps» → «6,0 Gb/s».
pub fn sata_speed(s: &str) -> Option<String> {
    let v: f64 = s.trim().strip_suffix("Gbps")?.trim().parse().ok()?;
    Some(format!("{} Gb/s", format!("{v:.1}").replace('.', ",")))
}

/// «8.0 GT/s PCIe» og bredden 4 → «PCIe 3.0 ×4».
pub fn pcie_link(speed: &str, width: Option<u32>) -> Option<String> {
    let gt: f64 = speed.split_whitespace().next()?.parse().ok()?;
    let generation = match gt {
        g if g < 3.0 => 1,
        g if g < 6.0 => 2,
        g if g < 12.0 => 3,
        g if g < 24.0 => 4,
        g if g < 48.0 => 5,
        _ => 6,
    };
    Some(match width {
        Some(w) => format!("PCIe {generation}.0 ×{w}"),
        None => format!("PCIe {generation}.0"),
    })
}

/// En partisjon fra sysfs.
#[derive(Clone, Debug, Default)]
pub struct PartDev {
    pub name: String,
    pub dev: String,
    pub number: Option<u32>,
    pub size: Option<f64>,
    pub fs: Option<String>,
    pub label: Option<String>,
}

/// En disk fra sysfs og udev (alt som ikke er SMART).
#[derive(Clone, Debug, Default)]
pub struct DiskDev {
    pub name: String,
    /// «8:0».
    pub dev: String,
    pub id: String,
    pub model: String,
    pub vendor: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub wwn: Option<String>,
    pub size: Option<f64>,
    pub rotational: Option<bool>,
    pub rpm: Option<u32>,
    pub bus: String,
    pub removable: bool,
    pub link: Option<String>,
    pub scheduler: Option<String>,
    pub logical_block: Option<u32>,
    pub physical_block: Option<u32>,
    pub parts: Vec<PartDev>,
}

impl DiskDev {
    pub fn kind(&self) -> &'static str {
        if self.bus == "NVMe" {
            "NVMe"
        } else if self.rpm == Some(0) || self.rotational == Some(false) && self.rpm.is_none() {
            "SSD"
        } else if self.rotational == Some(true) || self.rpm.is_some_and(|r| r > 0) {
            "HDD"
        } else {
            ""
        }
    }
}

/// Planleggeren i bruk: «none [mq-deadline] kyber» → «mq-deadline».
pub fn scheduler(s: &str) -> Option<String> {
    let a = s.find('[')?;
    let b = s[a..].find(']')?;
    Some(s[a + 1..a + b].to_string())
}

/// De fysiske diskene i `sys/block`, sortert etter navn.
pub fn scan(root: &Path) -> Vec<DiskDev> {
    let dir = root.join("sys/block");
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !is_virtual(n)).collect();
    names.sort();
    names.into_iter().map(|n| read_disk(root, &dir.join(&n), &n)).collect()
}

fn read_disk(root: &Path, d: &Path, name: &str) -> DiskDev {
    let dev = txt(&d.join("dev")).unwrap_or_default();
    let u = udev(root, &dev);
    let uget = |k: &str| u.get(k).map(String::as_str).filter(|s| !s.is_empty());
    let path = std::fs::canonicalize(d).map(|p| p.display().to_string()).unwrap_or_default();
    let nvme = name.starts_with("nvme");
    let bus = if nvme {
        "NVMe"
    } else if path.contains("/usb") || uget("ID_BUS") == Some("usb") {
        "USB"
    } else if path.contains("/ata") {
        "SATA"
    } else if name.starts_with("mmcblk") {
        "SD/eMMC"
    } else if name.starts_with("vd") || path.contains("/virtio") {
        "Virtuell"
    } else {
        ""
    };
    let model = uget("ID_MODEL_ENC")
        .map(udev_unescape)
        .or_else(|| uget("ID_MODEL").map(|m| m.replace('_', " ")))
        .or_else(|| txt(&d.join("device/model")))
        .unwrap_or_default();
    let vendor = txt(&d.join("device/vendor")).filter(|v| v != "ATA" && !v.is_empty()).or_else(|| (bus == "USB").then(|| uget("ID_VENDOR_ENC").map(udev_unescape)).flatten());
    let serial = uget("ID_SERIAL_SHORT").map(str::to_string).or_else(|| txt(&d.join("device/serial")));
    let firmware = uget("ID_REVISION").map(str::to_string).or_else(|| txt(&d.join("device/firmware_rev"))).or_else(|| txt(&d.join("device/rev")));
    let rpm = uget("ID_ATA_ROTATION_RATE_RPM").and_then(|r| r.parse().ok());
    // Lenken: SATA fra ata_link, NVMe fra PCI-enheten bak kontrolleren.
    let link = if nvme {
        let pci = d.join("device/device");
        txt(&pci.join("current_link_speed")).and_then(|s| pcie_link(&s, txt(&pci.join("current_link_width")).and_then(|w| w.parse().ok())))
    } else {
        path.split('/').find_map(|seg| seg.strip_prefix("ata").filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))).and_then(|n| txt(&root.join(format!("sys/class/ata_link/link{n}/sata_spd")))).and_then(|s| sata_speed(&s))
    };
    let mut parts: Vec<PartDev> = std::fs::read_dir(d)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join("partition").exists())
                .map(|e| {
                    let p = e.path();
                    let pname = e.file_name().to_string_lossy().into_owned();
                    let pdev = txt(&p.join("dev")).unwrap_or_default();
                    let pu = udev(root, &pdev);
                    let pg = |k: &str| pu.get(k).filter(|s| !s.is_empty()).cloned();
                    PartDev {
                        name: pname,
                        dev: pdev,
                        number: txt(&p.join("partition")).and_then(|n| n.parse().ok()),
                        size: txt(&p.join("size")).and_then(|s| s.parse::<f64>().ok()).map(|s| s * 512.0),
                        fs: pg("ID_FS_TYPE"),
                        label: pg("ID_FS_LABEL_ENC").map(|l| udev_unescape(&l)).or_else(|| pg("ID_FS_LABEL")),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    parts.sort_by_key(|p| (p.number.unwrap_or(0), p.name.clone()));
    let id = serial.as_deref().map(rigg_core::disk::key_part).filter(|s| !s.is_empty()).unwrap_or_else(|| rigg_core::disk::key_part(name));
    DiskDev {
        name: name.to_string(),
        dev,
        id,
        model,
        vendor,
        serial,
        firmware,
        wwn: uget("ID_WWN").map(str::to_string),
        size: txt(&d.join("size")).and_then(|s| s.parse::<f64>().ok()).map(|s| s * 512.0),
        rotational: txt(&d.join("queue/rotational")).map(|s| s == "1"),
        rpm,
        bus: bus.to_string(),
        removable: txt(&d.join("removable")).is_some_and(|s| s == "1"),
        link,
        scheduler: txt(&d.join("queue/scheduler")).and_then(|s| scheduler(&s)),
        logical_block: txt(&d.join("queue/logical_block_size")).and_then(|s| s.parse().ok()),
        physical_block: txt(&d.join("queue/physical_block_size")).and_then(|s| s.parse().ok()),
        parts,
    }
}

// ---------------------------------------------------------------- Monteringer

/// Én montering fra `/proc/self/mountinfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct Mount {
    pub dev: String,
    pub point: String,
    pub fs: String,
    pub source: String,
}

/// `\040` (mellomrom) og de andre oktale tegnene i monteringspunktet.
fn unescape_octal(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 4 <= b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 4]).ok().and_then(|h| u8::from_str_radix(h, 8).ok())
        {
            out.push(v);
            i += 4;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    let mut out = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split(' ').collect();
        let Some(sep) = f.iter().position(|x| *x == "-") else { continue };
        if f.len() < sep + 3 || f.len() < 5 {
            continue;
        }
        out.push(Mount { dev: f[2].to_string(), point: unescape_octal(f[4]), fs: f[sep + 1].to_string(), source: f[sep + 2].to_string() });
    }
    out
}

/// Byte i alt, brukt og ledig for vanlige brukere (`statvfs`).
pub fn statvfs(path: &str) -> Option<(f64, f64, f64)> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: strukturen eies her, og stien er nullavsluttet.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let fr = s.f_frsize as f64;
    let total = s.f_blocks as f64 * fr;
    let free = s.f_bfree as f64 * fr;
    let avail = s.f_bavail as f64 * fr;
    Some((total, total - free, avail))
}

/// Blokkenheten bak et `maj:min`, gjennom dm/md ned til en partisjon eller disk.
fn resolve_block(root: &Path, dev: &str, depth: u8) -> Option<String> {
    let p = std::fs::canonicalize(root.join("sys/dev/block").join(dev)).ok()?;
    let name = p.file_name()?.to_string_lossy().into_owned();
    if depth < 4 && (name.starts_with("dm-") || name.starts_with("md")) {
        let slave = std::fs::read_dir(p.join("slaves")).ok()?.flatten().next()?;
        let sdev = txt(&slave.path().join("dev"))?;
        return resolve_block(root, &sdev, depth + 1);
    }
    Some(name)
}

/// Disken som `/` er montert fra («sdb»).
pub fn root_disk(root: &Path, mounts: &[Mount], devs: &[DiskDev]) -> Option<String> {
    let m = mounts.iter().find(|m| m.point == "/")?;
    let name = resolve_block(root, &m.dev, 0).or_else(|| m.source.strip_prefix("/dev/").map(str::to_string))?;
    devs.iter().find(|d| d.name == name || d.parts.iter().any(|p| p.name == name)).map(|d| d.name.clone())
}

// ---------------------------------------------------------------- Leseren

/// SMART-verdiene prøvene trenger mellom lesingene fra UDisks2.
#[derive(Clone, Debug, Default)]
pub struct SmartCache {
    pub temp: Option<f64>,
    pub updated: Option<i64>,
}

/// Eies av prøvetråden. `sample` hver runde, `list` sjelden.
pub struct DiskReader {
    root: PathBuf,
    stats: Option<PreadFile>,
    pub devs: Vec<DiskDev>,
    prev: HashMap<String, (i64, Counters)>,
    last_scan: i64,
    /// Temperatur fra siste UDisks2-lesing, etter kjernenavn.
    pub smart: HashMap<String, SmartCache>,
    sim: Option<super::disk_sim::SimDisks>,
}

impl DiskReader {
    pub fn new(root: &Path) -> DiskReader {
        DiskReader { root: root.to_path_buf(), stats: PreadFile::open(&root.join("proc/diskstats")).ok(), devs: vec![], prev: HashMap::new(), last_scan: i64::MIN / 2, smart: HashMap::new(), sim: None }
    }

    /// Simulerte disker (tester og skjermbilder).
    pub fn sim(now: i64) -> DiskReader {
        let mut r = DiskReader::new(Path::new("/nonexistent"));
        r.stats = None;
        r.sim = Some(super::disk_sim::SimDisks::new(now));
        r
    }

    pub fn is_sim(&self) -> bool {
        self.sim.is_some()
    }

    /// Ingen disker (målinger: `RIGG_DISKER=ingen`).
    pub fn off() -> DiskReader {
        let mut r = DiskReader::new(Path::new("/nonexistent"));
        r.stats = None;
        r
    }

    pub fn is_off(&self) -> bool {
        self.sim.is_none() && self.stats.is_none() && self.root == Path::new("/nonexistent")
    }

    /// Nye disker oppdages hvert 60. s (USB-disker som settes i).
    pub fn rescan(&mut self, now: i64) {
        if self.sim.is_some() {
            return;
        }
        self.last_scan = now;
        self.devs = scan(&self.root);
    }

    /// Én prøve: rater fra forrige runde. Første runde har ingen rate.
    pub fn sample(&mut self, now: i64) -> Vec<DiskSample> {
        if let Some(s) = self.sim.as_mut() {
            return s.sample(now);
        }
        if now - self.last_scan >= 60_000 {
            self.rescan(now);
        }
        let Some(f) = self.stats.as_mut() else { return vec![] };
        let Ok(text) = f.read(64 * 1024) else { return vec![] };
        let counters: HashMap<String, Counters> = parse_diskstats(text).into_iter().filter(|(n, _)| !is_virtual(n)).collect();
        let mut out = Vec::with_capacity(self.devs.len());
        for d in &self.devs {
            let Some(c) = counters.get(&d.name).copied() else { continue };
            let (read, write, busy) = match self.prev.get(&d.name) {
                Some((t, p)) if now > *t => {
                    let (r, w, b) = rates(*p, c, now - t);
                    (Some(r), Some(w), Some(b))
                }
                _ => (None, None, None),
            };
            self.prev.insert(d.name.clone(), (now, c));
            let sc = self.smart.get(&d.name);
            let temp = sc.and_then(|s| s.temp).filter(|_| sc.and_then(|s| s.updated).is_some_and(|u| now - u <= rigg_core::disk::TEMP_MAX_AGE_MS));
            out.push(DiskSample { id: d.id.clone(), name: d.name.clone(), model: d.model.clone(), read, write, busy, temp });
        }
        out
    }

    /// Hele lista til Disker-siden. `drives` er UDisks2-dataene (eller en
    /// feil når den ikke svarer); SMART-cachen til prøvene oppdateres herfra.
    pub fn list(&mut self, now: i64, drives: Result<&[super::udisks::UDrive], &str>) -> DiskList {
        if let Some(s) = self.sim.as_ref() {
            return s.list(now);
        }
        self.rescan(now);
        let mounts = std::fs::read_to_string(self.root.join("proc/self/mountinfo")).map(|t| parse_mountinfo(&t)).unwrap_or_default();
        let sys_disk = root_disk(&self.root, &mounts, &self.devs);
        let mut disks = Vec::new();
        for d in &self.devs {
            let mut disk = Disk {
                id: d.id.clone(),
                name: d.name.clone(),
                dev: format!("/dev/{}", d.name),
                model: d.model.clone(),
                vendor: d.vendor.clone(),
                serial: d.serial.clone(),
                firmware: d.firmware.clone(),
                wwn: d.wwn.clone(),
                size: d.size,
                rpm: d.rpm,
                kind: d.kind().to_string(),
                bus: d.bus.clone(),
                removable: d.removable,
                system: sys_disk.as_deref() == Some(d.name.as_str()),
                link: d.link.clone(),
                scheduler: d.scheduler.clone(),
                logical_block: d.logical_block,
                physical_block: d.physical_block,
                ..Default::default()
            };
            // Partisjonene, og disken selv når den er montert uten partisjonstabell.
            let mut list: Vec<PartDev> = d.parts.clone();
            if d.parts.is_empty() && mounts.iter().any(|m| m.dev == d.dev) {
                list.push(PartDev { name: d.name.clone(), dev: d.dev.clone(), size: d.size, ..Default::default() });
            }
            for PartDev { name, dev, number, size, fs, label } in list {
                let m = mounts.iter().find(|m| m.dev == dev);
                let st = m.and_then(|m| statvfs(&m.point));
                disk.partitions.push(Partition {
                    name: name.clone(),
                    dev: format!("/dev/{name}"),
                    number,
                    size,
                    fs: m.map(|m| m.fs.clone()).or(fs),
                    label,
                    mount: m.map(|m| m.point.clone()),
                    total: st.map(|s| s.0),
                    used: st.map(|s| s.1),
                    avail: st.map(|s| s.2),
                });
            }
            match drives {
                Ok(list) => match list.iter().find(|u| u.device.as_deref() == Some(disk.dev.as_str())) {
                    Some(u) => super::udisks::apply(&mut disk, u, now),
                    None => {
                        disk.smart.note = Some(if d.bus == "USB" { "UDisks2 har ingen SMART for denne disken. USB-broen sender som regel ikke SMART videre." } else { "UDisks2 kjenner ikke disken." }.into())
                    }
                },
                Err(_) => disk.smart.note = Some("SMART kommer fra UDisks2, som ikke svarer på systembussen.".into()),
            }
            rigg_core::disk::finish(&mut disk);
            self.smart.insert(d.name.clone(), SmartCache { temp: disk.smart.temp, updated: disk.smart.updated });
            disks.push(disk);
        }
        let (source, note) = match drives {
            Ok(_) => ("udisks".to_string(), None),
            Err(e) => ("sysfs".to_string(), Some(e.to_string())),
        };
        DiskList { t: now, source, note, disks }
    }
}

/// De fysiske diskene som enkel JSON (Operativsystem-delen i `get_system`).
pub fn basic_json(root: &Path) -> Vec<serde_json::Value> {
    let devs = scan(root);
    let mounts = std::fs::read_to_string(root.join("proc/self/mounts")).ok().map(|t| {
        t.lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let (src, dst) = (it.next()?, it.next()?);
                (dst == "/").then(|| src.trim_start_matches("/dev/").to_string())
            })
            .collect::<Vec<_>>()
    });
    let root_part = mounts.and_then(|m| m.into_iter().next());
    devs.iter()
        .map(|d| {
            let system = root_part.as_deref().is_some_and(|r| r == d.name || d.parts.iter().any(|p| p.name == r));
            serde_json::json!({
                "name": d.name,
                "model": (!d.model.is_empty()).then(|| d.model.clone()),
                "vendor": d.vendor,
                "bytes": d.size,
                "kind": d.kind(),
                "bus": d.bus,
                "removable": d.removable,
                "system": system,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskstats_og_rater() {
        let a = "   8       0 sda 206 0 7864 251 0 0 0 0 0 236 251 0 0 0 0 0 0\n   8      16 sdb 2299440 2149244 199594007 3187435 2535693 5316864 276143978 77957342 0 2718531 82098420 0 0 0 0 472174 953642\n   7       0 loop0 1 0 2 0 0 0 0 0 0 0 0\n";
        let v = parse_diskstats(a);
        assert_eq!(v.len(), 3);
        assert_eq!(v[1].0, "sdb");
        assert_eq!(v[1].1, Counters { read_sectors: 199_594_007, write_sectors: 276_143_978, io_ms: 2_718_531 });
        let b = Counters { read_sectors: 199_594_007 + 2048, write_sectors: 276_143_978 + 4096, io_ms: 2_718_531 + 250 };
        let (r, w, busy) = rates(v[1].1, b, 1000);
        assert_eq!(r, 1_048_576.0);
        assert_eq!(w, 2_097_152.0);
        assert_eq!(busy, 25.0);
        // Telleren går aldri bakover (ny disk med samme navn): 0, ikke negativt.
        assert_eq!(rates(b, Counters::default(), 1000), (0.0, 0.0, 0.0));
    }

    #[test]
    fn tekster_fra_sysfs_og_udev() {
        assert_eq!(udev_unescape("Samsung\\x20SSD\\x20870\\x20QVO\\x201TB\\x20\\x20\\x20"), "Samsung SSD 870 QVO 1TB");
        assert_eq!(sata_speed("6.0 Gbps").as_deref(), Some("6,0 Gb/s"));
        assert_eq!(sata_speed("<unknown>"), None);
        assert_eq!(pcie_link("8.0 GT/s PCIe", Some(4)).as_deref(), Some("PCIe 3.0 ×4"));
        assert_eq!(pcie_link("16.0 GT/s PCIe", Some(4)).as_deref(), Some("PCIe 4.0 ×4"));
        assert_eq!(scheduler("none [mq-deadline] kyber").as_deref(), Some("mq-deadline"));
        assert!(is_virtual("loop12") && is_virtual("dm-0") && !is_virtual("sda") && !is_virtual("nvme0n1"));
    }

    #[test]
    fn mountinfo_med_mellomrom() {
        let t = "33 2 8:18 / / rw,relatime shared:1 - ext4 /dev/sdb2 rw\n325 32 8:33 / /run/media/julian/Transcend\\040128GB rw,nosuid shared:1024 - ext4 /dev/sdc1 rw,errors=remount-ro\n";
        let m = parse_mountinfo(t);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], Mount { dev: "8:18".into(), point: "/".into(), fs: "ext4".into(), source: "/dev/sdb2".into() });
        assert_eq!(m[1].point, "/run/media/julian/Transcend 128GB");
    }

    /// En falsk rot med to disker, partisjoner, udev-data og diskstats.
    fn fake_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("rigg-disk-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&root);
        let w = |p: &str, s: &str| {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        };
        for (n, dev, model, size, rot) in [("sda", "8:0", "ST2000DM008-2FR1", "3907029168", "1"), ("sdb", "8:16", "Samsung SSD 870 ", "1953525168", "0")] {
            w(&format!("sys/block/{n}/dev"), dev);
            w(&format!("sys/block/{n}/device/model"), model);
            w(&format!("sys/block/{n}/device/vendor"), "ATA     ");
            w(&format!("sys/block/{n}/size"), size);
            w(&format!("sys/block/{n}/queue/rotational"), rot);
            w(&format!("sys/block/{n}/queue/scheduler"), "none [mq-deadline] ");
            w(&format!("sys/block/{n}/queue/logical_block_size"), "512");
            w(&format!("sys/block/{n}/removable"), "0");
        }
        w("sys/block/sdb/sdb2/partition", "2");
        w("sys/block/sdb/sdb2/dev", "8:18");
        w("sys/block/sdb/sdb2/size", "1951318016");
        w("sys/block/loop0/dev", "7:0");
        w("run/udev/data/b8:0", "E:ID_SERIAL_SHORT=WFL454P6\nE:ID_MODEL_ENC=ST2000DM008-2FR102\\x20\\x20\nE:ID_REVISION=0001\nE:ID_ATA_ROTATION_RATE_RPM=7200\n");
        w("run/udev/data/b8:16", "E:ID_SERIAL_SHORT=S5SVNF0NC41586R\nE:ID_MODEL_ENC=Samsung\\x20SSD\\x20870\\x20QVO\\x201TB\nE:ID_ATA_ROTATION_RATE_RPM=0\n");
        w("run/udev/data/b8:18", "E:ID_FS_TYPE=ext4\nE:ID_FS_LABEL_ENC=System\\x20disk\n");
        w("proc/diskstats", "   8       0 sda 206 0 7864 251 0 0 0 0 0 236 251 0 0 0 0 0 0\n   8      16 sdb 10 0 1000 0 10 0 2000 0 0 100 0\n   8      18 sdb2 10 0 1000 0 10 0 2000 0 0 100 0\n   7       0 loop0 1 0 2 0 0 0 0 0 0 0 0\n");
        w("proc/self/mountinfo", "33 2 8:18 / / rw,relatime shared:1 - ext4 /dev/sdb2 rw\n");
        root
    }

    #[test]
    fn disker_fra_en_falsk_rot() {
        let root = fake_root();
        let v = scan(&root);
        assert_eq!(v.len(), 2, "loop0 er ikke en disk");
        assert_eq!(v[0].id, "WFL454P6");
        assert_eq!(v[0].model, "ST2000DM008-2FR102");
        assert_eq!(v[0].kind(), "HDD");
        assert_eq!(v[0].rpm, Some(7200));
        assert_eq!(v[1].kind(), "SSD");
        assert_eq!(v[1].parts.len(), 1);
        assert_eq!(v[1].parts[0].label.as_deref(), Some("System disk"));
        assert!(v[0].vendor.is_none(), "«ATA» er ikke en produsent");
        let mut r = DiskReader::new(&root);
        let first = r.sample(1_000);
        assert_eq!(first.len(), 2);
        assert!(first[0].read.is_none(), "første runde har ingen rate");
        std::fs::write(root.join("proc/diskstats"), "   8       0 sda 206 0 7864 251 0 0 0 0 0 236 251 0 0 0 0 0 0\n   8      16 sdb 20 0 3048 0 20 0 6096 0 0 600 0\n").unwrap();
        let s = r.sample(2_000);
        assert_eq!(s[1].read, Some(1_048_576.0));
        assert_eq!(s[1].write, Some(2_097_152.0));
        assert_eq!(s[1].busy, Some(50.0));
        assert_eq!(s[0].read, Some(0.0));
        // Lista uten UDisks2: systemdisken og partisjonen, og en forklaring.
        let l = r.list(3_000, Err("UDisks2 svarer ikke"));
        assert_eq!(l.source, "sysfs");
        assert!(l.disks[1].system);
        assert_eq!(l.disks[1].partitions[0].mount.as_deref(), Some("/"));
        assert_eq!(l.disks[1].partitions[0].fs.as_deref(), Some("ext4"));
        assert!(l.disks[0].smart.note.is_some());
        assert_eq!(l.disks[0].health.level, rigg_core::disk::Level::Unknown);
        let b = basic_json(&root);
        assert_eq!(b.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "leser de ekte diskene (kjør med --ignored)"]
    fn denne_maskinen() {
        let mut r = DiskReader::new(Path::new("/"));
        r.sample(0);
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let s = r.sample(1000);
        for d in &s {
            println!("{} {} {:?} {:?} {:?}", d.name, d.id, d.read, d.write, d.busy);
        }
        assert!(!s.is_empty());
    }
}
