//! Simulerte disker til tester og skjermbilder (`--backend sim`): en NVMe med
//! systemet, en SATA-SSD, to HDD-er (den ene med omallokerte sektorer, som
//! gir «Advarsel») og en USB-disk uten SMART. Hastighetene går i rolige
//! bølger, så grafene og sparklinjene har noe å vise.

use rigg_core::disk::{Disk, DiskList, DiskSample, NvmeHealth, Partition, Smart, SmartAttr};

pub struct SimDisks {
    start: i64,
    disks: Vec<Disk>,
}

#[allow(clippy::too_many_arguments)]
fn attr(id: u8, name: &str, flags: u16, value: i32, worst: i32, threshold: i32, pretty: i64, unit: i32) -> SmartAttr {
    SmartAttr { id, name: name.into(), flags, value, worst, threshold, pretty, unit }
}

/// Timer som libatasmart gir dem (ms).
fn h(hours: i64) -> i64 {
    hours * 3_600_000
}

#[allow(clippy::too_many_arguments)]
fn part(name: &str, n: u32, size: f64, fs: &str, label: Option<&str>, mount: Option<&str>, used: f64) -> Partition {
    let mounted = mount.is_some();
    Partition {
        name: name.into(),
        dev: format!("/dev/{name}"),
        number: Some(n),
        size: Some(size),
        fs: Some(fs.into()),
        label: label.map(str::to_string),
        mount: mount.map(str::to_string),
        total: mounted.then_some(size * 0.98),
        used: mounted.then_some(size * 0.98 * used),
        avail: mounted.then_some(size * 0.98 * (1.0 - used) * 0.95),
    }
}

impl SimDisks {
    pub fn new(now: i64) -> SimDisks {
        let updated = Some(now - 4 * 60_000);
        let base = |id: &str, name: &str, model: &str, kind: &str, bus: &str, size: f64| Disk {
            id: id.into(),
            name: name.into(),
            dev: format!("/dev/{name}"),
            model: model.into(),
            serial: Some(id.into()),
            size: Some(size),
            kind: kind.into(),
            bus: bus.into(),
            scheduler: Some(if bus == "NVMe" { "none" } else { "mq-deadline" }.into()),
            logical_block: Some(512),
            physical_block: Some(if kind == "HDD" { 4096 } else { 512 }),
            ..Default::default()
        };
        let mut nvme = base("S6B0NL0W512345", "nvme0n1", "Samsung SSD 980 PRO 1TB", "NVMe", "NVMe", 1_000_204_886_016.0);
        nvme.firmware = Some("5B2QGXA7".into());
        nvme.rpm = Some(0);
        nvme.system = true;
        nvme.link = Some("PCIe 4.0 ×4".into());
        nvme.smart = Smart {
            source: "sim".into(),
            supported: true,
            enabled: true,
            temp: Some(41.0),
            power_on_hours: Some(5_210.0),
            power_cycles: Some(1_204.0),
            selftest: Some("success".into()),
            updated,
            written: Some(41.8e12),
            read: Some(63.2e12),
            nvme: Some(NvmeHealth { avail_spare: Some(100.0), spare_thresh: Some(10.0), percent_used: Some(4.0), unsafe_shutdowns: Some(37.0), media_errors: Some(0.0), error_log_entries: Some(2.0), warning_temp: Some(82.0), critical_temp: Some(85.0), ..Default::default() }),
            life_left: Some(96.0),
            ..Default::default()
        };
        nvme.partitions = vec![part("nvme0n1p1", 1, 536_870_912.0, "vfat", None, Some("/boot/efi"), 0.012), part("nvme0n1p2", 2, 999_666_221_056.0, "ext4", None, Some("/"), 0.41)];

        let mut ssd = base("S5SVNF0NC41586R", "sda", "Samsung SSD 870 QVO 1TB", "SSD", "SATA", 1_000_204_886_016.0);
        ssd.firmware = Some("SVQ01B6Q".into());
        ssd.rpm = Some(0);
        ssd.link = Some("6,0 Gb/s".into());
        ssd.smart = Smart {
            source: "sim".into(),
            supported: true,
            enabled: true,
            temp: Some(29.0),
            power_on_hours: Some(3_244.0),
            bad_sectors: Some(0),
            attrs_failing: Some(0),
            attrs_failed_past: Some(0),
            selftest: Some("success".into()),
            updated,
            attributes: vec![
                attr(5, "reallocated-sector-count", 51, 100, 100, 10, 0, 3),
                attr(9, "power-on-hours", 50, 99, 99, 0, h(3_244), 2),
                attr(12, "power-cycle-count", 50, 99, 99, 0, 616, 1),
                attr(177, "wear-leveling-count", 19, 98, 98, 0, 18, 1),
                attr(179, "used-reserved-blocks-total", 19, 100, 100, 10, 0, 1),
                attr(181, "program-fail-count-total", 50, 100, 100, 10, 0, 1),
                attr(182, "erase-fail-count-total", 50, 100, 100, 10, 0, 1),
                attr(183, "runtime-bad-block-total", 19, 100, 100, 10, 0, 1),
                attr(187, "reported-uncorrect", 50, 100, 100, 0, 0, 3),
                attr(190, "airflow-temperature-celsius", 50, 71, 59, 0, 302_150, 4),
                attr(195, "hardware-ecc-recovered", 26, 200, 200, 0, 0, 1),
                attr(199, "udma-crc-error-count", 62, 100, 100, 0, 0, 1),
                attr(235, "good-block-rate", 18, 99, 99, 0, 0, 0),
                attr(241, "total-lbas-written", 50, 99, 99, 0, 555_844_507_837, 7),
            ],
            ..Default::default()
        };
        ssd.partitions = vec![part("sda1", 1, 1_000_202_788_864.0, "ext4", Some("Spill"), Some("/mnt/spill"), 0.72)];

        let hdd_attrs = |hours: i64, realloc: i64, pending: i64, temp_mk: i64, v5: i32| {
            vec![
                attr(1, "raw-read-error-rate", 15, 78, 64, 6, 57_583_758, 1),
                attr(3, "spin-up-time", 3, 98, 98, 0, 0, 0),
                attr(4, "start-stop-count", 50, 99, 99, 20, 1_613, 1),
                attr(5, "reallocated-sector-count", 51, v5, v5, 10, realloc, 3),
                attr(7, "seek-error-rate", 15, 72, 60, 45, 17_148_679, 1),
                attr(9, "power-on-hours", 50, 97, 97, 0, h(hours), 2),
                attr(10, "spin-retry-count", 19, 100, 100, 97, 0, 1),
                attr(12, "power-cycle-count", 50, 100, 100, 20, 611, 1),
                attr(187, "reported-uncorrect", 50, 100, 100, 0, 0, 3),
                attr(188, "command-timeout", 50, 100, 100, 0, 0, 1),
                attr(190, "airflow-temperature-celsius", 34, 65, 60, 40, temp_mk, 4),
                attr(193, "load-cycle-count", 50, 98, 98, 0, 4_810, 1),
                attr(194, "temperature-celsius-2", 34, 35, 40, 0, temp_mk, 4),
                attr(197, "current-pending-sector", 18, 100, 100, 0, pending, 3),
                attr(198, "offline-uncorrectable", 16, 100, 100, 0, 0, 3),
                attr(199, "udma-crc-error-count", 62, 200, 200, 0, 0, 1),
                attr(241, "total-lbas-written", 0, 100, 253, 0, 100_187_617_735, 7),
                attr(242, "total-lbas-read", 0, 100, 253, 0, 57_509_483_277, 7),
            ]
        };
        let mut hdd = base("WFL454P6", "sdb", "ST2000DM008-2FR102", "HDD", "SATA", 2_000_398_934_016.0);
        hdd.firmware = Some("0001".into());
        hdd.rpm = Some(7200);
        hdd.link = Some("6,0 Gb/s".into());
        hdd.smart = Smart {
            source: "sim".into(),
            supported: true,
            enabled: true,
            temp: Some(35.0),
            power_on_hours: Some(3_243.0),
            bad_sectors: Some(0),
            attrs_failing: Some(0),
            attrs_failed_past: Some(0),
            selftest: Some("success".into()),
            updated,
            attributes: hdd_attrs(3_243, 0, 0, 308_150, 100),
            ..Default::default()
        };
        hdd.partitions = vec![part("sdb1", 1, 2_000_396_836_864.0, "ext4", Some("Arkiv"), Some("/mnt/arkiv"), 0.58)];

        let mut warn = base("WDH029VL", "sdc", "ST4000DM000-2AE166", "HDD", "SATA", 4_000_787_030_016.0);
        warn.firmware = Some("0001".into());
        warn.rpm = Some(5980);
        warn.link = Some("6,0 Gb/s".into());
        warn.smart = Smart {
            source: "sim".into(),
            supported: true,
            enabled: true,
            temp: Some(38.0),
            power_on_hours: Some(28_431.0),
            bad_sectors: Some(10),
            attrs_failing: Some(0),
            attrs_failed_past: Some(0),
            selftest: Some("success".into()),
            updated,
            attributes: hdd_attrs(28_431, 8, 2, 311_150, 98),
            ..Default::default()
        };
        warn.partitions = vec![part("sdc1", 1, 4_000_785_104_896.0, "ext4", Some("Backup"), None, 0.0)];

        let mut usb = base("0401_5583", "sdd", "Extreme 55AE", "SSD", "USB", 1_000_170_586_112.0);
        usb.vendor = Some("SanDisk".into());
        usb.removable = true;
        usb.serial = Some("32343133464E343032383335".into());
        usb.firmware = Some("1012".into());
        usb.scheduler = Some("mq-deadline".into());
        usb.smart = Smart { note: Some("USB-broen sender ikke SMART videre.".into()), ..Default::default() };
        usb.partitions = vec![part("sdd1", 1, 1_000_169_537_536.0, "exfat", Some("SANDISK"), Some("/media/rigg/SANDISK"), 0.23)];

        let mut disks = vec![nvme, ssd, hdd, warn, usb];
        for d in &mut disks {
            rigg_core::disk::finish(d);
        }
        SimDisks { start: now, disks }
    }

    /// Rolige bølger i MB/s per disk, med en kopi til USB-disken hvert minutt.
    pub fn sample(&mut self, now: i64) -> Vec<DiskSample> {
        let t = (now - self.start) as f64 / 1000.0;
        let wave = |p: f64, phase: f64| ((t / p + phase).sin() * 0.5 + 0.5).powi(3);
        self.disks
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let (r, w) = match i {
                    0 => (180e6 * wave(9.0, 0.0) + 2e6, 60e6 * wave(13.0, 1.0) + 1.5e6),
                    1 => (420e6 * wave(17.0, 2.0), 25e6 * wave(7.0, 0.5)),
                    2 => (95e6 * wave(23.0, 0.7), 4e6 * wave(5.0, 0.2)),
                    3 => (0.0, 0.0),
                    _ => {
                        let copying = (t % 60.0) < 22.0;
                        (if copying { 2e6 } else { 0.0 }, if copying { 310e6 * (0.85 + 0.15 * (t / 3.0).sin()) } else { 0.0 })
                    }
                };
                let busy = ((r + w) / match i {
                    0 => 3.0e9,
                    1 => 5.5e8,
                    4 => 3.5e8,
                    _ => 1.9e8,
                } * 100.0)
                    .clamp(0.0, 100.0);
                DiskSample { id: d.id.clone(), name: d.name.clone(), model: d.model.clone(), read: Some(r), write: Some(w), busy: Some(busy), temp: d.smart.temp.map(|x| x + (t / 40.0 + i as f64).sin() * 0.4) }
            })
            .collect()
    }

    pub fn list(&self, now: i64) -> DiskList {
        DiskList { t: now, source: "sim".into(), note: None, disks: self.disks.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::disk::Level;

    #[test]
    fn fem_disker_med_én_advarsel() {
        let mut s = SimDisks::new(0);
        let l = s.list(0);
        assert_eq!(l.disks.len(), 5);
        let lv: Vec<Level> = l.disks.iter().map(|d| d.health.level).collect();
        assert_eq!(lv, vec![Level::Ok, Level::Ok, Level::Ok, Level::Warn, Level::Unknown]);
        assert!(l.disks[3].health.reasons[0].contains("dårlige sektorer"));
        assert!(l.disks[0].system);
        let p = s.sample(30_000);
        assert_eq!(p.len(), 5);
        assert!(p.iter().all(|d| d.busy.is_some_and(|b| (0.0..=100.0).contains(&b))));
        assert!(p[4].temp.is_none(), "USB-disken har ingen temperatur");
    }
}
