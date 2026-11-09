//! Skrivebordsvarsler ved rød grense og når kortet forsvinner (BØR,
//! produktspesifikasjonen 3.5), sendt med `org.freedesktop.Notifications`.
//!
//! Grunnmuren kaller [`Alarms::on_sample`] for hver prøve og
//! [`Alarms::on_device`] når et kort bytter tilstand, og sender det som kommer
//! tilbake som `alarm`-hendelser.
//!
//! Reglene (av som standard, `alarms.desktop`):
//! - Varsel når en verdi har vært **på eller over** den røde grensen i 10 s.
//!   Tittel «GPU-temperaturen er 86 °C», tekst «Over den røde grensen på 85 °C
//!   siden 14:31.»
//! - Hysterese på 3 °C: verdien må under grensen minus 3 før den kan varsle igjen.
//! - Høyst ett varsel per verdi per 15 min.
//! - Klikk på varselet åpner appen på Sensorer (appens handling `vis-side`).
//! - Kortet forsvinner, eller driveren oppdateres eller ikke kan leses: ett
//!   varsel ved overgangen (ikke ved oppstart).
//! - Diskhelse (runde 6A, `alarms.diskHealth`): når SMART sier at en disk
//!   feiler (også første gang den sees), og når antallet dårlige sektorer
//!   øker. Klikk åpner Disker. Høyst ett varsel per disk og grunn per 6 t.

use rigg_core::disk::{DiskList, Level};
use rigg_core::model::{DeviceState, GpuInfo, Telemetry};
use rigg_core::settings::ServiceSettings;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Hvor lenge verdien må ligge over grensen.
pub const HOLD_MS: i64 = 10_000;
/// Hysterese (°C eller prosentpoeng).
pub const HYSTERESIS: f64 = 3.0;
/// Høyst ett varsel per verdi så ofte.
pub const REPEAT_MS: i64 = 15 * 60_000;
/// Høyst ett diskvarsel per disk og grunn så ofte.
pub const DISK_REPEAT_MS: i64 = 6 * 3_600_000;

/// Tilstanden for én verdi.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub above_since: Option<i64>,
    pub armed: bool,
    pub last_sent: Option<i64>,
}

impl Default for Track {
    fn default() -> Self {
        Track { above_since: None, armed: true, last_sent: None }
    }
}

impl Track {
    /// Én prøve. Gir `Some(siden)` når det skal varsles nå.
    pub fn step(&mut self, value: Option<f64>, limit: f64, now: i64) -> Option<i64> {
        let Some(v) = value.filter(|v| v.is_finite()) else {
            self.above_since = None;
            return None;
        };
        if v >= limit {
            let since = *self.above_since.get_or_insert(now);
            if self.armed && now - since >= HOLD_MS && self.last_sent.is_none_or(|l| now - l >= REPEAT_MS) {
                self.armed = false;
                self.last_sent = Some(now);
                return Some(since);
            }
            return None;
        }
        self.above_since = None;
        if v <= limit - HYSTERESIS {
            self.armed = true;
        }
        None
    }
}

/// Lokal klokke «14:31».
fn hm(ms: i64) -> String {
    let t = rigg_core::fmt::local(ms, crate::logg::utc_offset(ms));
    format!("{:02}:{:02}", t.hour, t.minute)
}

fn deg(v: f64) -> String {
    rigg_core::fmt::with_unit(format!("{}", v.round() as i64), "°C")
}

/// Teksten i varselet: («GPU-temperaturen er 86 °C», «Over den røde grensen på 85 °C siden 14:31.»).
pub fn threshold_text(what: &str, value: f64, limit: f64, since: i64) -> (String, String) {
    (format!("{what} er {}", deg(value)), format!("Over den røde grensen på {} siden {}.", deg(limit), hm(since)))
}

#[derive(Default)]
pub struct Alarms {
    tracks: HashMap<String, Track>,
    /// Siste kjente tilstand per kort (overgangene varsles, ikke oppstarten).
    states: HashMap<String, DeviceState>,
    device_sent: HashMap<String, i64>,
    /// Siste kjente dårlige sektorer per disk (økningen varsles).
    disk_bad: HashMap<String, i64>,
    notifier: Option<Rc<Notifier>>,
    /// Av i testene: ingen D-Bus.
    pub quiet: bool,
}

impl Alarms {
    pub fn new() -> Alarms {
        Alarms::default()
    }

    pub fn on_sample(&mut self, t: &Telemetry, settings: &ServiceSettings, now: i64) -> Vec<Value> {
        let a = &settings.alarms;
        if !a.desktop {
            self.tracks.clear();
            return vec![];
        }
        let th = &settings.thresholds;
        let mut items: Vec<(String, String, Option<f64>, f64)> = Vec::new();
        if a.gpu_temp {
            let many = t.gpus.len() > 1;
            for (i, (id, s)) in t.gpus.iter().enumerate() {
                let what = if many { format!("Temperaturen på skjermkort {}", i + 1) } else { "GPU-temperaturen".to_string() };
                items.push((rigg_core::sensor_key::gpu(id, "temp"), what, s.temp, th.gpu_temp_crit));
            }
        }
        if a.cpu_temp {
            items.push(("cpu/temp".into(), "CPU-temperaturen".into(), rigg_core::sensormeta::cpu_temp(&t.sys), th.cpu_temp_crit));
        }
        let mut out = Vec::new();
        for (key, what, value, limit) in items {
            let tr = self.tracks.entry(key.clone()).or_default();
            if let Some(since) = tr.step(value, limit, now) {
                let v = value.unwrap_or(limit);
                let (title, body) = threshold_text(&what, v, limit, since);
                self.notify(&key, &title, &body, "sensorer");
                out.push(json!({"kind": "threshold", "key": key, "value": v, "limit": limit, "since": since, "t": now, "title": title, "body": body}));
            }
        }
        out
    }

    pub fn on_device(&mut self, info: &GpuInfo, settings: &ServiceSettings, now: i64) -> Vec<Value> {
        let prev = self.states.insert(info.id.clone(), info.state);
        let a = &settings.alarms;
        // Første gang kortet sees, eller ingen endring: ikke varsle.
        let Some(prev) = prev else { return vec![] };
        if prev == info.state || !a.desktop || !a.gpu_lost {
            return vec![];
        }
        let name = rigg_core::model::short_gpu_name(&info.name);
        let (title, body) = match info.state {
            DeviceState::Lost => (
                "Skjermkortet svarer ikke".to_string(),
                format!("{}Siden {}. Det skjer når kortet faller av bussen eller trekkes ut. Tallene før det er tatt vare på.", if name.is_empty() { String::new() } else { format!("{name}. ") }, hm(info.since.max(1))),
            ),
            DeviceState::DriverUpdated => (
                "Driveren er oppdatert".to_string(),
                format!(
                    "Ny NVIDIA-driver ({}) er installert, men den gamle ({}) kjører fortsatt. Start maskinen på nytt for å lese skjermkortet.",
                    info.driver_installed.as_deref().unwrap_or("ny"),
                    info.driver.as_deref().unwrap_or("gammel")
                ),
            ),
            DeviceState::NoDriver => ("Fant ingen NVIDIA-driver".to_string(), "Prosessor, minne og sensorer vises som vanlig.".to_string()),
            DeviceState::Ok if prev == DeviceState::Lost => ("Skjermkortet svarer igjen".to_string(), format!("{}Det er funnet igjen og leses som før.", if name.is_empty() { String::new() } else { format!("{name} ") })),
            _ => return vec![],
        };
        let key = format!("device:{}:{}", info.id, info.state.as_str());
        if self.device_sent.get(&key).is_some_and(|t| now - t < REPEAT_MS) {
            return vec![];
        }
        self.device_sent.insert(key.clone(), now);
        self.notify(&key, &title, &body, "grafikkort");
        vec![json!({"kind": "device", "gpu": info.id, "state": info.state.as_str(), "t": now, "title": title, "body": body})]
    }

    /// Ny disklista: varsel når SMART melder feil, og når antallet dårlige
    /// sektorer har økt siden forrige liste (den første lista er utgangspunktet).
    pub fn on_disks(&mut self, list: &DiskList, settings: &ServiceSettings, now: i64) -> Vec<Value> {
        let a = &settings.alarms;
        let on = a.desktop && a.disk_health;
        let mut out = Vec::new();
        for d in &list.disks {
            let bad = d.smart.bad_sectors.unwrap_or_else(|| {
                d.smart.attributes.iter().filter(|x| rigg_core::disk::BAD_SECTOR_ATTRS.contains(&x.id)).filter_map(|x| x.count()).filter(|c| *c > 0).sum()
            });
            let before = self.disk_bad.insert(d.id.clone(), bad);
            if !on || d.smart.source.is_empty() {
                continue;
            }
            let name = if d.model.is_empty() { d.name.clone() } else { d.model.clone() };
            let mut send = |key: String, title: String, body: String, kind: &str| {
                if self.device_sent.get(&key).is_some_and(|t| now - t < DISK_REPEAT_MS) {
                    return;
                }
                self.device_sent.insert(key.clone(), now);
                out.push(json!({"kind": kind, "disk": d.id, "t": now, "title": title, "body": body}));
            };
            if d.health.level == Level::Fail {
                let why = d.health.reasons.first().cloned().unwrap_or_default();
                send(format!("disk:{}:fail", d.id), format!("Disken {name} feiler"), format!("{why}. Ta sikkerhetskopi av det som ligger på disken ({}).", d.dev), "disk_fail");
            }
            if let Some(b) = before
                && bad > b
            {
                send(
                    format!("disk:{}:bad:{bad}", d.id),
                    format!("Nye dårlige sektorer på {name}"),
                    format!("{} (var {b}). Ta sikkerhetskopi av det som ligger på disken ({}).", rigg_core::fmt::plural(bad, "dårlig sektor", "dårlige sektorer"), d.dev),
                    "disk_sectors",
                );
            }
        }
        for v in &out {
            let key = format!("disk:{}", v["disk"].as_str().unwrap_or(""));
            self.notify(&key, v["title"].as_str().unwrap_or(""), v["body"].as_str().unwrap_or(""), "disker");
        }
        out
    }

    fn notify(&mut self, key: &str, title: &str, body: &str, page: &str) {
        if self.quiet {
            return;
        }
        if self.notifier.is_none() {
            self.notifier = Notifier::connect().map(Rc::new);
        }
        if let Some(n) = &self.notifier {
            n.send(key, title, body, page);
        }
    }
}

// ---------------------------------------------------------------- D-Bus

/// `org.freedesktop.Notifications` på øktbussen. Husker hvilken side hvert
/// varsel skal åpne, og hvilket varsel som sist ble sendt for en verdi (så et
/// nytt erstatter det gamle i stedet for å stable dem).
pub struct Notifier {
    conn: gio::DBusConnection,
    /// Varsel-ID → (side, aktiveringsnøkkel).
    open: RefCell<HashMap<u32, (String, Option<String>)>>,
    last_id: RefCell<HashMap<String, u32>>,
}

const N_NAME: &str = "org.freedesktop.Notifications";
const N_PATH: &str = "/org/freedesktop/Notifications";

impl Notifier {
    fn connect() -> Option<Notifier> {
        let conn = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
        Some(Notifier { conn, open: RefCell::new(HashMap::new()), last_id: RefCell::new(HashMap::new()) })
    }

    fn send(self: &Rc<Self>, key: &str, title: &str, body: &str, page: &str) {
        use gio::prelude::*;
        let app_id = rigg_core::paths::Profile::current().app_id().to_string();
        let replaces = self.last_id.borrow().get(key).copied().unwrap_or(0);
        let mut hints: HashMap<String, glib::Variant> = HashMap::new();
        hints.insert("desktop-entry".into(), app_id.to_variant());
        hints.insert("urgency".into(), 1u8.to_variant());
        hints.insert("category".into(), "device".to_variant());
        let actions = vec!["default".to_string(), "Åpne".to_string()];
        let params = ("Agentic Rigg", replaces, app_id.as_str(), title, body, actions, hints, -1i32).to_variant();
        self.ensure_watch();
        let me = self.clone();
        let (key, page) = (key.to_string(), page.to_string());
        self.conn.call(
            Some(N_NAME),
            N_PATH,
            N_NAME,
            "Notify",
            Some(&params),
            Some(glib::VariantTy::new("(u)").unwrap()),
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
            move |res| match res {
                Ok(v) => {
                    if let Some((id,)) = v.get::<(u32,)>() {
                        me.last_id.borrow_mut().insert(key, id);
                        me.open.borrow_mut().insert(id, (page, None));
                    }
                }
                Err(e) => eprintln!("Kunne ikke sende skrivebordsvarsel: {e}"),
            },
        );
    }

    /// Lytter på klikk (`ActionInvoked`) og aktiveringsnøkkelen som kommer før.
    fn ensure_watch(self: &Rc<Self>) {
        thread_local! {
            static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        if WATCHING.with(|w| w.replace(true)) {
            return;
        }
        let me = Rc::downgrade(self);
        #[allow(deprecated)]
        let _ = self.conn.signal_subscribe(Some(N_NAME), Some(N_NAME), None, Some(N_PATH), None, gio::DBusSignalFlags::NONE, move |_c, _s, _p, _i, signal, params| {
            let Some(n) = me.upgrade() else { return };
            match signal {
                "ActivationToken" => {
                    if let Some((id, token)) = params.get::<(u32, String)>()
                        && let Some(e) = n.open.borrow_mut().get_mut(&id)
                    {
                        e.1 = Some(token);
                    }
                }
                "ActionInvoked" => {
                    if let Some((id, _action)) = params.get::<(u32, String)>() {
                        let hit = n.open.borrow().get(&id).cloned();
                        if let Some((page, token)) = hit {
                            n.open_app(&page, token);
                        }
                    }
                }
                "NotificationClosed" => {
                    if let Some((id, _reason)) = params.get::<(u32, u32)>() {
                        n.open.borrow_mut().remove(&id);
                    }
                }
                _ => {}
            }
        });
    }

    /// Åpner vinduet på siden: appens handling `vis-side` over D-Bus når den
    /// kjører, ellers starter appen fra skrivebordsfila.
    fn open_app(&self, page: &str, token: Option<String>) {
        use gio::prelude::*;
        let app_id = rigg_core::paths::Profile::current().app_id().to_string();
        let path = format!("/{}", app_id.replace('.', "/"));
        let mut pd: HashMap<String, glib::Variant> = HashMap::new();
        if let Some(t) = token {
            pd.insert("activation-token".into(), t.to_variant());
            pd.insert("desktop-startup-id".into(), t.to_variant());
        }
        let params = ("vis-side", vec![page.to_variant()], pd).to_variant();
        let desktop = format!("{app_id}.desktop");
        self.conn.call(
            Some(&app_id),
            &path,
            "org.freedesktop.Application",
            "ActivateAction",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            None::<&gio::Cancellable>,
            move |res| {
                if res.is_err() {
                    // Vinduet kjører ikke: start appen (den åpner startsiden).
                    if let Some(info) = gio::AppInfo::all().into_iter().find(|a| a.id().as_deref() == Some(desktop.as_str())) {
                        let _ = info.launch(&[], None::<&gio::AppLaunchContext>);
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::model::GpuSample;

    #[test]
    fn over_grensen_i_ti_sekunder_med_hysterese() {
        let mut t = Track::default();
        assert_eq!(t.step(Some(86.0), 85.0, 0), None, "ikke før 10 s");
        assert_eq!(t.step(Some(87.0), 85.0, 9_000), None);
        assert_eq!(t.step(Some(86.0), 85.0, 10_000), Some(0), "varsel etter 10 s, med tiden det startet");
        assert_eq!(t.step(Some(88.0), 85.0, 30_000), None, "bare ett varsel");
        // Ned til 83 (under grensen, men innenfor hysteresen): ikke klar igjen.
        assert_eq!(t.step(Some(83.0), 85.0, 40_000), None);
        assert_eq!(t.step(Some(86.0), 85.0, 41_000), None);
        assert_eq!(t.step(Some(86.0), 85.0, 60_000), None, "ikke klar uten å ha vært 3 °C under");
        assert_eq!(t.step(Some(82.0), 85.0, 70_000), None);
        assert!(t.armed);
        assert_eq!(t.step(Some(86.0), 85.0, 80_000), None);
        assert_eq!(t.step(Some(86.0), 85.0, 95_000), None, "klar igjen, men høyst ett per 15 min");
        assert_eq!(t.step(Some(86.0), 85.0, 10_000 + REPEAT_MS), Some(80_000));
    }

    #[test]
    fn avbrudd_starter_tiden_på_nytt() {
        let mut t = Track::default();
        t.step(Some(90.0), 85.0, 0);
        t.step(Some(84.0), 85.0, 5_000);
        assert_eq!(t.step(Some(90.0), 85.0, 11_000), None);
        assert_eq!(t.step(None, 85.0, 12_000), None);
        assert_eq!(t.step(Some(90.0), 85.0, 13_000), None);
        assert_eq!(t.step(Some(90.0), 85.0, 23_000), Some(13_000));
    }

    #[test]
    fn diskhelse_varsler_feil_og_nye_dårlige_sektorer() {
        use rigg_core::disk::{Disk, Health, Smart};
        let disk = |bad: i64, level: Level| Disk {
            id: "WDH029VL".into(),
            name: "sde".into(),
            dev: "/dev/sde".into(),
            model: "ST4000DM000-2AE166".into(),
            smart: Smart { source: "udisks".into(), supported: true, bad_sectors: Some(bad), ..Default::default() },
            health: Health { level, reasons: vec!["Disken melder at den feiler (SMART-status)".into()] },
            ..Default::default()
        };
        let list = |d: Disk| DiskList { disks: vec![d], ..Default::default() };
        let mut a = Alarms { quiet: true, ..Default::default() };
        let mut s = ServiceSettings::default();
        assert!(s.alarms.disk_health, "på under hovedbryteren, som de andre");
        assert!(a.on_disks(&list(disk(0, Level::Ok)), &s, 0).is_empty(), "av som standard (hovedbryteren)");
        s.alarms.desktop = true;
        assert!(a.on_disks(&list(disk(0, Level::Ok)), &s, 1).is_empty());
        let ev = a.on_disks(&list(disk(8, Level::Warn)), &s, 2);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0]["title"], "Nye dårlige sektorer på ST4000DM000-2AE166");
        assert!(ev[0]["body"].as_str().unwrap().starts_with("8 dårlige sektorer (var 0)"));
        assert!(a.on_disks(&list(disk(8, Level::Warn)), &s, 3).is_empty(), "samme tall: ingen nytt varsel");
        let ev = a.on_disks(&list(disk(8, Level::Fail)), &s, 4);
        assert_eq!(ev[0]["kind"], "disk_fail");
        assert!(a.on_disks(&list(disk(8, Level::Fail)), &s, 5).is_empty(), "høyst ett per 6 t");
        s.alarms.disk_health = false;
        assert!(a.on_disks(&list(disk(20, Level::Fail)), &s, DISK_REPEAT_MS + 10).is_empty());
    }

    #[test]
    fn tekstene_og_reglene() {
        let (title, body) = threshold_text("GPU-temperaturen", 86.2, 85.0, 1_794_058_260_000);
        assert_eq!(title, "GPU-temperaturen er 86\u{202F}°C");
        assert!(body.starts_with("Over den røde grensen på 85\u{202F}°C siden ") && body.ends_with('.'));
        let mut a = Alarms { quiet: true, ..Default::default() };
        let mut s = ServiceSettings::default();
        let mut tele = Telemetry::default();
        tele.gpus.insert("0000:1c:00.0".into(), GpuSample { temp: Some(90.0), ..Default::default() });
        assert!(a.on_sample(&tele, &s, 0).is_empty());
        assert!(a.on_sample(&tele, &s, 20_000).is_empty(), "av som standard");
        s.alarms.desktop = true;
        assert!(a.on_sample(&tele, &s, 30_000).is_empty());
        let ev = a.on_sample(&tele, &s, 40_000);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0]["key"], "gpu:0000:1c:00.0/temp");
        assert_eq!(ev[0]["limit"], 85.0);
        // Kortet: oppstarten varsler ikke, overgangen til «borte» gjør.
        let mut g = GpuInfo { id: "0000:1c:00.0".into(), name: "NVIDIA GeForce RTX 2080".into(), ..Default::default() };
        assert!(a.on_device(&g, &s, 0).is_empty());
        g.state = DeviceState::Lost;
        let ev = a.on_device(&g, &s, 1000);
        assert_eq!(ev[0]["title"], "Skjermkortet svarer ikke");
        g.state = DeviceState::Ok;
        assert_eq!(a.on_device(&g, &s, 2000)[0]["title"], "Skjermkortet svarer igjen");
        s.alarms.gpu_lost = false;
        g.state = DeviceState::Lost;
        assert!(a.on_device(&g, &s, 3000).is_empty());
    }
}
