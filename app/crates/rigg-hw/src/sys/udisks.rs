//! UDisks2 over systembussen (`org.freedesktop.UDisks2`), uten root.
//!
//! Vi leser bare det UDisks2 alt har: egenskapene til `Drive`, `Drive.Ata`
//! og `NVMe.Controller` (én `GetManagedObjects`), og SMART-tabellen med
//! `SmartGetAttributes`, som gir det UDisks2 leste sist. UDisks2 oppdaterer
//! SMART selv omtrent hvert 10. min og vekker ikke disker i hvile
//! (`SmartUpdated` viser når). Vi kaller aldri `SmartUpdate`,
//! `SmartSelftestStart` eller noe annet som sender kommandoer til disken.
//!
//! Svarene gjøres om til JSON ([`variant_json`]) før de tolkes, så
//! tolkningen kan testes med `gdbus`-utskrift fra en ekte maskin
//! (`tests/fixtures/udisks`).

use rigg_core::disk::{Disk, NvmeHealth, SmartAttr};
use rigg_core::{tr, trf};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub const BUS: &str = "org.freedesktop.UDisks2";
const IF_DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const IF_ATA: &str = "org.freedesktop.UDisks2.Drive.Ata";
const IF_NVME: &str = "org.freedesktop.UDisks2.NVMe.Controller";
const IF_BLOCK: &str = "org.freedesktop.UDisks2.Block";
const IF_PART: &str = "org.freedesktop.UDisks2.Partition";
/// Høyst så lenge et kall kan holde prøvetråden (ms).
const TIMEOUT_MS: i32 = 2500;

/// En `GVariant` som JSON: tall, tekst, `ay` som tekst, `a{s*}` som objekt,
/// lister og tupler som lister.
pub fn variant_json(v: &glib::Variant) -> Value {
    use glib::VariantClass as C;
    match v.classify() {
        C::Boolean => json!(v.get::<bool>()),
        C::Byte => json!(v.get::<u8>()),
        C::Int16 => json!(v.get::<i16>()),
        C::Uint16 => json!(v.get::<u16>()),
        C::Int32 => json!(v.get::<i32>()),
        C::Uint32 => json!(v.get::<u32>()),
        C::Int64 => json!(v.get::<i64>()),
        C::Uint64 => json!(v.get::<u64>()),
        C::Double => json!(v.get::<f64>()),
        C::String | C::ObjectPath | C::Signature => json!(v.str()),
        C::Variant => v.as_variant().map(|x| variant_json(&x)).unwrap_or(Value::Null),
        C::Maybe => v.as_maybe().map(|x| variant_json(&x)).unwrap_or(Value::Null),
        C::Array => {
            let ty = v.type_().as_str();
            if ty == "ay" {
                let b = v.fixed_array::<u8>().unwrap_or(&[]);
                let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
                return json!(String::from_utf8_lossy(&b[..end]));
            }
            if ty.starts_with("a{") {
                let mut m = Map::new();
                for i in 0..v.n_children() {
                    let e = v.child_value(i);
                    let k = e.child_value(0);
                    let key = k.str().map(str::to_string).unwrap_or_else(|| variant_json(&k).to_string());
                    m.insert(key, variant_json(&e.child_value(1)));
                }
                return Value::Object(m);
            }
            Value::Array((0..v.n_children()).map(|i| variant_json(&v.child_value(i))).collect())
        }
        C::Tuple | C::DictEntry => Value::Array((0..v.n_children()).map(|i| variant_json(&v.child_value(i))).collect()),
        _ => Value::Null,
    }
}

/// Egenskapene vi bruker fra én disk i UDisks2.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UDrive {
    pub path: String,
    /// Blokkenheten for hele disken («/dev/sda»).
    pub device: Option<String>,
    pub drive: Value,
    pub ata: Option<Value>,
    pub nvme: Option<Value>,
    pub attrs: Vec<SmartAttr>,
    pub nvme_attrs: Option<Value>,
}

/// Diskene i svaret på `GetManagedObjects` (som JSON), med blokkenheten
/// som hører til (den uten partisjon).
pub fn drives_from_objects(objs: &Value) -> Vec<UDrive> {
    let Some(o) = objs.as_object() else { return vec![] };
    let mut devices: HashMap<&str, String> = HashMap::new();
    for ifs in o.values() {
        let Some(b) = ifs.get(IF_BLOCK) else { continue };
        if ifs.get(IF_PART).is_some() {
            continue;
        }
        if let (Some(drive), Some(dev)) = (b["Drive"].as_str().filter(|d| *d != "/"), b["Device"].as_str()) {
            devices.entry(drive).or_insert_with(|| dev.to_string());
        }
    }
    let mut out: Vec<UDrive> = o
        .iter()
        .filter_map(|(path, ifs)| {
            let d = ifs.get(IF_DRIVE)?;
            Some(UDrive {
                path: path.clone(),
                device: devices.get(path.as_str()).cloned(),
                drive: d.clone(),
                ata: ifs.get(IF_ATA).cloned(),
                nvme: ifs.get(IF_NVME).cloned(),
                attrs: vec![],
                nvme_attrs: None,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// SMART-tabellen fra `SmartGetAttributes` (JSON av `(a(ysqiiixia{sv}),)`).
pub fn parse_attrs(v: &Value) -> Vec<SmartAttr> {
    let list = v.get(0).and_then(Value::as_array).or_else(|| v.as_array().filter(|a| a.first().is_some_and(|x| x.is_array() && x.get(1).is_some_and(Value::is_string))));
    let Some(list) = list else { return vec![] };
    list.iter()
        .filter_map(|r| {
            let r = r.as_array()?;
            Some(SmartAttr {
                id: r.first()?.as_u64()? as u8,
                name: r.get(1)?.as_str()?.to_string(),
                flags: r.get(2)?.as_u64()? as u16,
                value: r.get(3)?.as_i64()? as i32,
                worst: r.get(4)?.as_i64()? as i32,
                threshold: r.get(5)?.as_i64()? as i32,
                pretty: r.get(6)?.as_i64()?,
                unit: r.get(7)?.as_i64()? as i32,
            })
        })
        .collect()
}

fn k_to_c(k: Option<f64>) -> Option<f64> {
    // 0 K (ikke oppgitt) og 0 °C (Transcend SSD370 oppgir ikke temperatur).
    k.filter(|k| *k > 273.5).map(|k| ((k - 273.15) * 10.0).round() / 10.0)
}

/// Fyller disken med det UDisks2 vet (navn og SMART).
pub fn apply(d: &mut Disk, u: &UDrive, now: i64) {
    let dr = &u.drive;
    let s = |k: &str| dr[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    if let Some(m) = s("Model") {
        d.model = m;
    }
    d.vendor = s("Vendor").or(d.vendor.take());
    d.serial = s("Serial").or(d.serial.take());
    d.firmware = s("Revision").or(d.firmware.take());
    d.wwn = s("WWN").or(d.wwn.take());
    if let Some(sz) = dr["Size"].as_f64().filter(|v| *v > 0.0) {
        d.size = Some(sz);
    }
    match dr["RotationRate"].as_i64() {
        Some(0) => {
            d.rpm = Some(0);
            if d.kind == "HDD" || d.kind.is_empty() {
                d.kind = "SSD".into();
            }
        }
        Some(r) if r > 0 => {
            d.rpm = Some(r as u32);
            d.kind = "HDD".into();
        }
        _ => {}
    }
    if dr["ConnectionBus"].as_str() == Some("usb") {
        d.bus = "USB".into();
    }
    d.removable = d.removable || dr["Removable"].as_bool().unwrap_or(false);
    let sm = &mut d.smart;
    sm.source = "udisks".into();
    if let Some(a) = &u.ata {
        sm.supported = a["SmartSupported"].as_bool().unwrap_or(false);
        sm.enabled = a["SmartEnabled"].as_bool().unwrap_or(false);
        sm.failing = a["SmartFailing"].as_bool().unwrap_or(false);
        let updated = a["SmartUpdated"].as_i64().filter(|t| *t > 0).map(|t| t * 1000);
        sm.updated = updated;
        if updated.is_some() {
            sm.temp = k_to_c(a["SmartTemperature"].as_f64());
            sm.power_on_hours = a["SmartPowerOnSeconds"].as_f64().filter(|v| *v > 0.0).map(|v| (v / 3600.0).round());
            sm.bad_sectors = a["SmartNumBadSectors"].as_i64().filter(|v| *v >= 0);
            sm.attrs_failing = a["SmartNumAttributesFailing"].as_i64().filter(|v| *v >= 0);
            sm.attrs_failed_past = a["SmartNumAttributesFailedInThePast"].as_i64().filter(|v| *v >= 0);
            sm.selftest = a["SmartSelftestStatus"].as_str().map(str::to_string);
            sm.selftest_remaining = a["SmartSelftestPercentRemaining"].as_i64().map(|v| v as i32);
        }
        if !sm.supported {
            sm.note = Some(tr!("Disken støtter ikke SMART.").into());
        } else if !sm.enabled {
            sm.note = Some(tr!("SMART er slått av på disken.").into());
        } else if updated.is_none() {
            sm.note = Some(tr!("UDisks2 har ikke lest SMART ennå.").into());
        }
        sm.attributes = u.attrs.clone();
    } else if let Some(n) = &u.nvme {
        sm.supported = true;
        sm.enabled = true;
        sm.updated = n["SmartUpdated"].as_i64().filter(|t| *t > 0).map(|t| t * 1000);
        sm.temp = k_to_c(n["SmartTemperature"].as_f64());
        sm.power_on_hours = n["SmartPowerOnHours"].as_f64();
        sm.selftest = n["SmartSelftestStatus"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
        sm.selftest_remaining = n["SmartSelftestPercentRemaining"].as_i64().map(|v| v as i32);
        let warn: Vec<String> = n["SmartCriticalWarning"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let x = u.nvme_attrs.as_ref().map(|v| v.get(0).unwrap_or(v).clone()).unwrap_or(Value::Null);
        let f = |k: &str| x[k].as_f64();
        let h = NvmeHealth {
            critical_warning: warn,
            avail_spare: f("avail_spare"),
            spare_thresh: f("spare_thresh"),
            percent_used: f("percent_used"),
            unsafe_shutdowns: f("unsafe_shutdowns"),
            media_errors: f("media_errors"),
            error_log_entries: f("num_err_log_entries"),
            warning_temp_minutes: f("warning_temp_time"),
            critical_temp_minutes: f("critical_temp_time"),
            warning_temp: k_to_c(f("wctemp")),
            critical_temp: k_to_c(f("cctemp")),
        };
        sm.life_left = h.percent_used.map(|p| (100.0 - p).max(0.0));
        sm.written = f("total_data_written").filter(|v| *v > 0.0);
        sm.read = f("total_data_read").filter(|v| *v > 0.0);
        sm.power_cycles = f("power_cycles");
        sm.nvme = Some(h);
    } else {
        sm.source.clear();
        sm.note = Some(if d.bus == "USB" { tr!("USB-broen sender ikke SMART videre.") } else { tr!("UDisks2 har ingen SMART for denne disken.") }.into());
    }
    let _ = now;
}

/// Klienten i prøvetråden. Kobler til systembussen ved første bruk, og
/// husker SMART-tabellen per disk til `SmartUpdated` endrer seg.
#[derive(Default)]
pub struct Udisks {
    conn: Option<gio::DBusConnection>,
    cache: HashMap<String, (i64, Vec<SmartAttr>, Option<Value>)>,
}

impl Udisks {
    pub fn new() -> Udisks {
        Udisks::default()
    }

    fn conn(&mut self) -> Result<gio::DBusConnection, String> {
        if let Some(c) = &self.conn
            && !c.is_closed()
        {
            return Ok(c.clone());
        }
        let c = gio::bus_get_sync(gio::BusType::System, None::<&gio::Cancellable>).map_err(|e| trf!("Fikk ikke kontakt med systembussen ({feil}).", feil = e))?;
        self.conn = Some(c.clone());
        Ok(c)
    }

    fn call(&mut self, path: &str, iface: &str, method: &str, args: Option<&glib::Variant>) -> Result<Value, String> {
        let c = self.conn()?;
        let r = c.call_sync(Some(BUS), path, iface, method, args, None, gio::DBusCallFlags::NO_AUTO_START, TIMEOUT_MS, None::<&gio::Cancellable>);
        match r {
            Ok(v) => Ok(variant_json(&v)),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Diskene med egenskapene. `attrs`: hent SMART-tabellen også (bare for
    /// disker der `SmartUpdated` har endret seg siden sist).
    pub fn drives(&mut self, attrs: bool) -> Result<Vec<UDrive>, String> {
        let objs = self.call("/org/freedesktop/UDisks2", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects", None).map_err(|e| {
            if e.contains("ServiceUnknown") || e.contains("NameHasNoOwner") || e.contains("was not provided") {
                tr!("UDisks2 svarer ikke på systembussen.").to_string()
            } else {
                trf!("UDisks2 svarte ikke ({feil}).", feil = e)
            }
        })?;
        let mut list = drives_from_objects(objs.get(0).unwrap_or(&objs));
        let opts = glib::Variant::tuple_from_iter([glib::VariantDict::new(None).end()]);
        for u in list.iter_mut() {
            let (updated, iface) = if let Some(a) = &u.ata {
                (a["SmartUpdated"].as_i64().unwrap_or(0), IF_ATA)
            } else if let Some(n) = &u.nvme {
                (n["SmartUpdated"].as_i64().unwrap_or(0), IF_NVME)
            } else {
                continue;
            };
            let smart_on = u.ata.as_ref().is_none_or(|a| a["SmartEnabled"].as_bool() == Some(true));
            let cached = self.cache.get(&u.path).filter(|c| c.0 == updated);
            match cached {
                Some((_, a, n)) => {
                    u.attrs = a.clone();
                    u.nvme_attrs = n.clone();
                }
                None if attrs && updated > 0 && smart_on => {
                    if let Ok(v) = self.call(&u.path, iface, "SmartGetAttributes", Some(&opts)) {
                        if iface == IF_ATA {
                            u.attrs = parse_attrs(&v);
                        } else {
                            u.nvme_attrs = Some(v);
                        }
                        self.cache.insert(u.path.clone(), (updated, u.attrs.clone(), u.nvme_attrs.clone()));
                    }
                }
                None => {
                    // Uten ny tabell: den forrige, så siden ikke blinker.
                    if let Some((_, a, n)) = self.cache.get(&u.path) {
                        u.attrs = a.clone();
                        u.nvme_attrs = n.clone();
                    }
                }
            }
        }
        self.cache.retain(|p, _| list.iter().any(|u| &u.path == p));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/udisks").join(name)).unwrap()
    }

    fn parse(text: &str, ty: &str) -> Value {
        let t = glib::VariantTy::new(ty).unwrap();
        variant_json(&glib::Variant::parse(Some(t), text.trim()).unwrap())
    }

    #[test]
    fn variant_som_json() {
        let v = parse("({'a': <uint64 3>, 'b': <b'/dev/sda'>, 'c': <@as ['x', 'y']>, 'd': <objectpath '/'>},)", "(a{sv})");
        assert_eq!(v, json!([{"a": 3, "b": "/dev/sda", "c": ["x", "y"], "d": "/"}]));
    }

    #[test]
    fn diskene_på_brukerens_maskin() {
        // `gdbus call --system … GetManagedObjects`, 8. november 2026.
        let objs = parse(&fixture("objekter.txt"), "(a{oa{sa{sv}}})");
        let list = drives_from_objects(&objs[0]);
        assert_eq!(list.len(), 5, "870 QVO, tre Seagate og SSD370");
        let s = list.iter().find(|u| u.drive["Model"] == "Samsung SSD 870 QVO 1TB").unwrap();
        assert_eq!(s.device.as_deref(), Some("/dev/sdb"));
        assert_eq!(s.ata.as_ref().unwrap()["SmartTemperature"], 297.0);
        let mut d = Disk { name: "sdb".into(), dev: "/dev/sdb".into(), kind: "SSD".into(), ..Default::default() };
        let mut u = s.clone();
        u.attrs = parse_attrs(&parse(&fixture("smart-870qvo.txt"), "(a(ysqiiixia{sv}))"));
        assert_eq!(u.attrs.len(), 14);
        apply(&mut d, &u, 0);
        rigg_core::disk::finish(&mut d);
        assert_eq!(d.model, "Samsung SSD 870 QVO 1TB");
        assert_eq!(d.serial.as_deref(), Some("S5SVNF0NC41586R"));
        assert_eq!(d.rpm, Some(0));
        assert_eq!(d.smart.temp, Some(23.9));
        assert_eq!(d.smart.power_on_hours, Some(3244.0));
        assert_eq!(d.smart.life_left, Some(98.0));
        assert!((d.smart.written.unwrap() / 1e12 - 8.48).abs() < 0.01);
        assert_eq!(d.health.level, rigg_core::disk::Level::Ok);
        // Transcend oppgir 0 °C: ingen temperatur.
        let t = list.iter().find(|u| u.drive["Model"] == "TS128GSSD370").unwrap();
        let mut d = Disk { kind: "SSD".into(), ..Default::default() };
        let mut tu = t.clone();
        tu.attrs = parse_attrs(&parse(&fixture("smart-ssd370.txt"), "(a(ysqiiixia{sv}))"));
        apply(&mut d, &tu, 0);
        rigg_core::disk::finish(&mut d);
        assert_eq!(d.smart.temp, None);
        assert_eq!(d.smart.life_left, Some(100.0), "232 gjenværende utholdenhet");
        assert_eq!(d.health.level, rigg_core::disk::Level::Ok, "{:?}", d.health);
        // Seagate: HDD med 7200 o/min.
        let h = list.iter().find(|u| u.drive["Model"] == "ST2000DM008-2FR102").unwrap();
        let mut d = Disk::default();
        let mut hu = h.clone();
        hu.attrs = parse_attrs(&parse(&fixture("smart-st2000.txt"), "(a(ysqiiixia{sv}))"));
        apply(&mut d, &hu, 0);
        rigg_core::disk::finish(&mut d);
        assert_eq!(d.kind, "HDD");
        assert_eq!(d.rpm, Some(7200));
        assert_eq!(d.smart.life_left, None, "HDD har ingen slitasje");
        assert_eq!(d.health.level, rigg_core::disk::Level::Ok, "{:?}", d.health);
    }

    #[test]
    fn nvme_fra_udisks() {
        let mut d = Disk { kind: "NVMe".into(), bus: "NVMe".into(), ..Default::default() };
        let u = UDrive {
            drive: json!({"Model": "Samsung SSD 980 PRO 1TB", "RotationRate": 0}),
            nvme: Some(json!({"SmartUpdated": 1_794_172_836u64, "SmartTemperature": 318, "SmartPowerOnHours": 812, "SmartCriticalWarning": []})),
            nvme_attrs: Some(json!([{"percent_used": 3, "avail_spare": 100, "spare_thresh": 10, "total_data_written": 12.5e12, "media_errors": 0, "wctemp": 355, "cctemp": 358}])),
            ..Default::default()
        };
        apply(&mut d, &u, 0);
        rigg_core::disk::finish(&mut d);
        assert_eq!(d.smart.temp, Some(44.9));
        assert_eq!(d.smart.life_left, Some(97.0));
        assert_eq!(d.smart.written, Some(12.5e12));
        assert_eq!(d.smart.nvme.as_ref().unwrap().warning_temp, Some(81.9));
        assert_eq!(d.health.level, rigg_core::disk::Level::Ok);
    }

    #[test]
    #[ignore = "spør den ekte UDisks2 på systembussen (kjør med --ignored --nocapture)"]
    fn ekte_udisks2() {
        let t0 = std::time::Instant::now();
        let mut u = Udisks::new();
        let list = u.drives(true).expect("UDisks2 svarer");
        let first = t0.elapsed();
        let t1 = std::time::Instant::now();
        let again = u.drives(true).unwrap();
        let cached = t1.elapsed();
        assert_eq!(list.len(), again.len());
        let mut r = super::super::disk::DiskReader::new(std::path::Path::new("/"));
        let l = r.list(rigg_core::now_ms(), Ok(&list));
        for d in &l.disks {
            println!(
                "{} {:<26} {:<16} {:<4} {:>5} {:?} {:?} t={:?} poh={:?} liv={:?} skrevet={:?} attr={} {:?}",
                d.name, d.model, d.serial.clone().unwrap_or_default(), d.kind, d.bus, d.link, d.health.level, d.smart.temp, d.smart.power_on_hours, d.smart.life_left,
                d.smart.written.map(rigg_core::disk::disk_bytes), d.smart.attributes.len(), d.health.reasons
            );
            for p in &d.partitions {
                println!("    {} {:?} {:?} {:?} {:?}", p.name, p.fs, p.mount, p.used.map(rigg_core::disk::disk_bytes), p.avail.map(rigg_core::disk::disk_bytes));
            }
        }
        println!("første lesing {first:?}, med tabellene i cache {cached:?}");
        assert!(!l.disks.is_empty());
    }
}
