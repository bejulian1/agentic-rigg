//! Tjenestens tilstand på hovedtråden: klientene, historikken, statistikken,
//! de siste prøvene og livsløpet. Alt her kjører i GLib-hovedløkka; NVML og
//! filene i `/proc`/`/sys` eies av prøvetråden (`sampler.rs`).

use crate::devices::Devices;
use crate::logg::Logger;
use crate::sampler::{Cmd, Round, Sampler};
use crate::server::Server;
use crate::session::Session;
use crate::store::Store;
use crate::varsler::Alarms;
use rigg_core::history::History;
use rigg_core::model::{GpuStatic, Telemetry};
use rigg_core::proto::{self, LineEvent};
use rigg_core::settings::ServiceSettings;
use rigg_core::stats::Stats;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

pub struct Options {
    pub backend: String,
    pub root: PathBuf,
    pub grace_ms: u32,
    /// Stiene til root-hjelperen (`/`, eller en falsk rot i tester og dev).
    pub hjelper: rigg_core::hjelper::Paths,
    /// `--backend sim:hete` (runde 8A): prosessoren og diskene følger
    /// hete-syklusen også (`rigg_hw::gpu::sim::hete`). Bare skjermbilder.
    pub hete: bool,
}

pub struct Daemon {
    pub me: Weak<RefCell<Daemon>>,
    pub opts: Options,
    pub started_at: i64,
    pub store: Store,
    pub settings: ServiceSettings,
    pub history: History,
    pub stats: Stats,
    pub devices: Devices,
    pub statics: BTreeMap<String, (GpuStatic, bool)>,
    pub latest: Option<Telemetry>,
    pub sessions: BTreeMap<u64, Session>,
    pub server: Server,
    pub sampler: Sampler,
    pub interval_ms: u32,
    /// De trege verdiene i full takt (schedule::detailed).
    pub detailed: bool,
    pub logger: Logger,
    pub alarms: Alarms,
    pub system: Option<Value>,
    pub ready: bool,
    pub idle_shutdown: bool,
    grace: Option<glib::SourceId>,
    pub stopping: bool,
    last_history_save: i64,
    last_minmax_save: i64,
    last_stats_event: i64,
    pub main_loop: glib::MainLoop,
    pub notes: Vec<String>,
    /// Siste rundes kostnad (µs), til feilsøking.
    pub last_cost_us: u64,
    /// Bekreft eller rull tilbake. Svarer `write_locked` for alt annet enn
    /// `--backend sim` (`rigg_core::txn::Book::handle`).
    pub txn: rigg_core::txn::Book,
    /// Prosesslista per kort (`get_processes`), lest av prøvetråden.
    pub processes: BTreeMap<String, Value>,
    /// Til når prøvetråden skal lese prosesslista (ms). 0 = av.
    pub processes_until: i64,
    /// Disklista fra prøvetråden (`get_disks`, runde 6A).
    pub disks: Option<rigg_core::disk::DiskList>,
    /// Sist `get_disks` ba prøvetråden om en ny liste (ms).
    pub disks_asked: i64,
    /// Diskvarselet er på (sendt til prøvetråden).
    pub disk_alarm: bool,
    /// Viftesløyfen og varmemodellen for det simulerte kortet (runde 7B).
    pub vifter: rigg_hw::gpu::sim_vifter::SimVifter,
    /// Klienten mot root-hjelperen (runde 7A, `hjelper.rs`).
    pub hjelper: crate::hjelper::Hjelper,
}

impl Daemon {
    #[allow(clippy::too_many_arguments)]
    pub fn new(opts: Options, store: Store, server: Server, sampler: Sampler, interval_ms: u32, main_loop: glib::MainLoop, now: i64) -> Rc<RefCell<Daemon>> {
        let mut notes = Vec::new();
        let (settings, note) = store.load_settings(now);
        notes.extend(note);
        let mut history = History::new(Some(store.history_path()));
        history.load(now);
        notes.extend(history.load_note.clone());
        let (stats, note) = if settings.keep_min_max { Stats::load(&store.minmax_path(), now) } else { (Stats::new(now), None) };
        notes.extend(note);
        let mut statics = BTreeMap::new();
        for st in store.cached_statics() {
            statics.insert(st.id.clone(), (st, false));
        }
        for n in &notes {
            eprintln!("{n}");
        }
        let hjelper = crate::hjelper::Hjelper::new(opts.hjelper.clone());
        let d = Rc::new(RefCell::new(Daemon {
            me: Weak::new(),
            opts,
            started_at: now,
            store,
            settings,
            history,
            stats,
            devices: Devices::default(),
            statics,
            latest: None,
            sessions: BTreeMap::new(),
            server,
            sampler,
            interval_ms,
            detailed: false,
            logger: Logger::new(),
            alarms: Alarms::new(),
            system: None,
            ready: false,
            idle_shutdown: false,
            grace: None,
            stopping: false,
            last_history_save: now,
            last_minmax_save: now,
            last_stats_event: now,
            main_loop,
            notes,
            last_cost_us: 0,
            txn: rigg_core::txn::Book::new(),
            processes: BTreeMap::new(),
            processes_until: 0,
            disks: None,
            disks_asked: 0,
            disk_alarm: false,
            vifter: Default::default(),
            hjelper,
        }));
        {
            let mut b = d.borrow_mut();
            b.txn.confirm_s = b.settings.confirm_sec;
        }
        d.borrow_mut().me = Rc::downgrade(&d);
        d.borrow_mut().sync_disk_alarm();
        d.borrow_mut().hjelper.probe(now);
        d
    }

    /// Diskvarselet trenger SMART også uten vindu (hvert 10. min).
    pub fn sync_disk_alarm(&mut self) {
        let a = &self.settings.alarms;
        let on = a.desktop && a.disk_health;
        if on != self.disk_alarm {
            self.disk_alarm = on;
            self.sampler.send(Cmd::DiskAlarm(on));
        }
    }

    // ---------------------------------------------------------------- Klienter

    /// Lytteren har nye klienter.
    pub fn on_accept(&mut self) {
        for id in self.server.accept() {
            let uid_ok = self.server.clients.get(&id).and_then(|c| c.uid) == Some(crate::server::my_uid());
            if !uid_ok {
                eprintln!("Avviste en klient med annen bruker-ID.");
                let err = proto::response_line(&Value::Null, &Err(proto::ProtoError::msg(proto::ErrorCode::PermissionDenied, "Bare samme bruker kan koble til.")));
                self.server.queue(id, &err);
                self.server.clients.remove(&id);
                continue;
            }
            if self.sessions.len() >= proto::MAX_CLIENTS {
                let err = proto::response_line(&Value::Null, &Err(proto::ProtoError::new(proto::ErrorCode::TooManyClients)));
                self.server.queue(id, &err);
                self.server.clients.remove(&id);
                continue;
            }
            let pid = self.server.clients.get(&id).and_then(|c| c.pid);
            self.sessions.insert(id, Session::new(id, pid));
            let fd = std::os::fd::AsRawFd::as_raw_fd(&self.server.clients[&id].stream);
            let weak = self.me.clone();
            let w = glib_unix::unix_fd_add_local(fd, glib::IOCondition::IN | glib::IOCondition::HUP | glib::IOCondition::ERR, move |_, _| {
                let Some(d) = weak.upgrade() else { return glib::ControlFlow::Break };
                let keep = d.borrow_mut().on_readable(id);
                if keep { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
            });
            if let Some(c) = self.server.clients.get_mut(&id) {
                c.in_watch = Some(w);
            }
            self.cancel_grace();
        }
    }

    /// Klienten har data. Gir `false` når kilden skal fjernes (klienten er borte).
    fn on_readable(&mut self, id: u64) -> bool {
        let mut buf = Vec::new();
        let closed = self.server.read(id, &mut buf).unwrap_or(true);
        if !buf.is_empty() {
            let events = match self.server.clients.get_mut(&id) {
                Some(c) => c.lines.push(&buf, proto::MAX_LINE),
                None => return false,
            };
            for ev in events {
                match ev {
                    LineEvent::Line(line) => {
                        let reply = self.handle_line(id, &line);
                        if let Some(r) = reply {
                            self.send(id, &r);
                        }
                    }
                    LineEvent::TooLarge => {
                        let e = proto::response_line(&Value::Null, &Err(proto::ProtoError::new(proto::ErrorCode::MessageTooLarge)));
                        self.server.queue(id, &e);
                        self.disconnect(id, true);
                        return false;
                    }
                }
                if !self.server.clients.contains_key(&id) {
                    return false;
                }
            }
        }
        if closed {
            self.disconnect(id, true);
            return false;
        }
        self.server.clients.contains_key(&id)
    }

    fn handle_line(&mut self, id: u64, line: &[u8]) -> Option<Vec<u8>> {
        let req = match proto::parse_request(line) {
            Ok(r) => r,
            Err((rid, e)) => return Some(proto::response_line(&rid, &Err(e))),
        };
        let res = self.handle(id, &req);
        Some(proto::response_line(&req.id, &res))
    }

    /// Sender til én klient. En klient som ikke tar imot, kobles fra.
    pub fn send(&mut self, id: u64, bytes: &[u8]) {
        if !self.server.queue(id, bytes) {
            self.disconnect(id, false);
            return;
        }
        if self.server.pending(id) && self.server.clients.get(&id).is_some_and(|c| c.out_watch.is_none()) {
            let fd = std::os::fd::AsRawFd::as_raw_fd(&self.server.clients[&id].stream);
            let weak = self.me.clone();
            let w = glib_unix::unix_fd_add_local(fd, glib::IOCondition::OUT, move |_, _| {
                let Some(d) = weak.upgrade() else { return glib::ControlFlow::Break };
                let mut d = d.borrow_mut();
                let ok = d.server.flush(id);
                if !ok {
                    if let Some(c) = d.server.clients.get_mut(&id) {
                        c.out_watch = None;
                    }
                    d.disconnect(id, false);
                    return glib::ControlFlow::Break;
                }
                if d.server.pending(id) {
                    return glib::ControlFlow::Continue;
                }
                if let Some(c) = d.server.clients.get_mut(&id) {
                    c.out_watch = None;
                }
                glib::ControlFlow::Break
            });
            if let Some(c) = self.server.clients.get_mut(&id) {
                c.out_watch = Some(w);
            }
        }
    }

    /// Kobler fra en klient. `from_in_watch`: kalt fra lesekilden, som fjernes
    /// ved å returnere `Break` (og ikke skal fjernes her).
    pub fn disconnect(&mut self, id: u64, from_in_watch: bool) {
        if let Some(mut c) = self.server.clients.remove(&id) {
            if let Some(w) = c.out_watch.take() {
                w.remove();
            }
            if let Some(w) = c.in_watch.take()
                && !from_in_watch
            {
                w.remove();
            }
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
        if self.sessions.remove(&id).is_some() {
            // Det klienten ikke har bekreftet, rulles tilbake straks (sim).
            if self.txn.disconnect(&format!("s{id}")) {
                self.after_txn();
            }
            self.hjelper_session_closed(id);
            self.reschedule();
            self.check_idle();
        }
    }

    /// Sender en hendelse til alle som abonnerer på emnet (`shutdown` til alle).
    pub fn broadcast(&mut self, event: &str, data: &Value) {
        let line = proto::event_line(event, data);
        let ids: Vec<u64> = self.sessions.values().filter(|s| s.hello && (event == "shutdown" || s.wants(event))).map(|s| s.id).collect();
        for id in ids {
            self.send(id, &line);
        }
    }

    // ---------------------------------------------------------------- Takt og livsløp

    pub fn keep_alive(&self) -> bool {
        // Hjelperen setter kortet til standard når tjenesten forsvinner, så
        // tjenesten blir værende mens hjelperen har andre verdier (7A).
        (self.settings.background && !self.idle_shutdown) || self.logger.active() || self.hjelper.holds_values()
    }

    /// Ny takt når klientene eller innstillingene endres.
    pub fn reschedule(&mut self) {
        let iv = crate::schedule::interval(self.sessions.values(), self.settings.background_interval_ms, self.logger.interval_ms());
        if iv != self.interval_ms {
            self.interval_ms = iv;
            self.sampler.send(Cmd::Interval(iv));
        }
        let detailed = crate::schedule::detailed(self.sessions.values(), self.logger.active());
        if detailed != self.detailed {
            self.detailed = detailed;
            self.sampler.send(Cmd::Detailed(detailed));
        }
    }

    /// Starter eller avbryter fristen før stopp.
    pub fn check_idle(&mut self) {
        if self.stopping {
            return;
        }
        let clients = !self.sessions.is_empty();
        if clients || self.keep_alive() {
            self.cancel_grace();
            return;
        }
        if self.grace.is_some() {
            return;
        }
        let weak = self.me.clone();
        // «Stopp tjenesten nå»: ikke vent de vanlige 60 s etter siste vindu.
        let ms = if self.idle_shutdown { self.opts.grace_ms.min(1500) } else { self.opts.grace_ms };
        self.grace = Some(glib::timeout_add_local_once(Duration::from_millis(ms as u64), move || {
            if let Some(d) = weak.upgrade() {
                let mut d = d.borrow_mut();
                d.grace = None;
                if d.sessions.is_empty() && !d.keep_alive() {
                    eprintln!("Ingen klienter på {:.1} s. Stopper.", ms as f64 / 1000.0);
                    d.stop("idle");
                }
            }
        }));
    }

    fn cancel_grace(&mut self) {
        if let Some(g) = self.grace.take() {
            g.remove();
        }
    }

    /// Lagrer, lukker og stopper hovedløkka.
    pub fn stop(&mut self, reason: &str) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        crate::lifecycle::sd_notify("STOPPING=1");
        self.cancel_grace();
        self.broadcast("shutdown", &json!({"reason": reason}));
        let now = rigg_core::now_ms();
        self.history.save(now);
        if self.settings.keep_min_max {
            let _ = self.stats.save(&self.store.minmax_path());
        }
        self.logger.close();
        self.sampler.stop();
        let ids: Vec<u64> = self.server.clients.keys().copied().collect();
        for id in ids {
            let _ = self.server.flush(id);
            self.disconnect(id, false);
        }
        if let Some(w) = self.server.listen_watch.take() {
            w.remove();
        }
        let _ = std::fs::remove_file(&self.server.path);
        self.main_loop.quit();
    }

    /// Dvale: stopp prøvene og lagre før, start igjen etter.
    pub fn on_sleep(&mut self, going: bool) {
        self.devices.suspended = going;
        self.sampler.send(Cmd::Pause(going));
        if going {
            let now = rigg_core::now_ms();
            self.history.save(now);
            self.last_history_save = now;
        }
        let list = serde_json::to_value(self.devices.view()).unwrap_or(Value::Null);
        self.broadcast("device", &json!({"gpus": list}));
    }

    // ---------------------------------------------------------------- Bekreft eller rull tilbake (sim)

    /// Etter en endring i boka: `txn`-hendelsene ut, og de simulerte faste
    /// dataene oppdatert (vinduet henter dem på nytt etter `device`).
    pub fn after_txn(&mut self) {
        for ev in self.txn.take_events() {
            self.broadcast("txn", &ev.to_json());
        }
        if self.opts.backend == "sim" {
            self.sync_sim_statics();
            let list = serde_json::to_value(self.devices.view()).unwrap_or(Value::Null);
            self.broadcast("device", &json!({"gpus": list}));
        }
    }

    /// Den simulerte backenden viser det som er i bruk i boka: offset og
    /// temperaturmål i `extra.styring`. **Bare `sim`**; med NVML er boka tom.
    fn sync_sim_statics(&mut self) {
        let a = self.txn.active.clone();
        for (st, _) in self.statics.values_mut() {
            let x = &mut st.extra["styring"];
            if !x.is_object() {
                continue;
            }
            x["coreOffsetMHz"]["value"] = json!(a.core_offset_mhz);
            x["memOffsetMHz"]["value"] = json!(a.mem_offset_mhz);
            x["tempTarget"]["value"] = json!(a.temp_target_c);
            let manual = a.fan_mode != rigg_core::txn::FanMode::Auto;
            let n = st.fan_count.unwrap_or(2) as usize;
            x["fanPolicy"] = json!(vec![u32::from(manual); n]);
            // Viftekurven og oppførselen som er i bruk (runde 7B), så Vifter
            // kan vise kurven kortet følger. Bare sim; NVML har ikke feltet.
            x["fan"] = a.to_json()["fan"].clone();
            st.temp_target = Some(a.temp_target_c);
        }
    }

    /// Prøven fra den simulerte backenden med det som er i bruk i boka:
    /// effektgrensen, og temperaturen og viftene fra viftesløyfen
    /// (`rigg_hw::gpu::sim_vifter`: varmemodellen og viftemotoren, runde 7B).
    fn overlay_sim(&mut self, gpus: &mut BTreeMap<String, rigg_core::model::GpuSample>, now: i64) {
        let a = &self.txn.active;
        for g in gpus.values_mut() {
            g.power_limit = g.power_limit.map(|_| a.power_limit_w);
        }
        let min = self.statics.values().find_map(|(s, _)| s.fan_min).unwrap_or(rigg_core::txn::HW_FAN.min);
        self.vifter.runde(gpus, a, min, now);
        self.txn.emergency = self.vifter.nod;
    }

    // ---------------------------------------------------------------- Prøvene

    pub fn on_round(&mut self, r: Round) {
        if self.stopping {
            return;
        }
        let now = r.t;
        self.last_cost_us = r.cost_us;
        let changed = self.devices.update(r.infos);
        let had_statics = !r.statics.is_empty();
        for (st, done) in r.statics {
            if done {
                self.store.save_static(&st);
            }
            self.statics.insert(st.id.clone(), (st, done));
        }
        if self.opts.backend == "sim" && had_statics {
            self.sync_sim_statics();
        }
        for (id, p) in r.processes {
            self.processes.insert(id, p);
        }
        let mut disk_alarms = Vec::new();
        if let Some(list) = r.disks {
            disk_alarms = self.alarms.on_disks(&list, &self.settings, now);
            self.disks = Some(list);
        }
        if self.processes_until > 0 && now >= self.processes_until {
            self.processes_until = 0;
            self.sampler.send(Cmd::Processes(false));
        }
        let mut gpus = r.gpus;
        let mut sys = r.sys;
        if self.opts.backend == "sim" {
            self.overlay_sim(&mut gpus, now);
        }
        if self.opts.hete {
            overlay_hete(&mut gpus, &mut sys, (now - self.started_at) as f64 / 1000.0);
        }
        let tele = Telemetry { t: r.t, gpus, sys, interval_ms: r.interval_ms };
        let flat = rigg_core::sensor_key::flatten(&tele);
        self.history.note_cadence(now, r.interval_ms);
        self.history.ingest(now, flat.iter().map(|(k, v)| (k.as_str(), *v)));
        self.stats.ingest(flat.iter().map(|(k, v)| (k.as_str(), *v)));
        let log_ev = self.logger.on_sample(&tele, &flat, now);
        let mut alarms = self.alarms.on_sample(&tele, &self.settings, now);
        for g in &changed {
            alarms.extend(self.alarms.on_device(g, &self.settings, now));
        }
        // Uten noen som abonnerer (bakgrunnen), lages ingen JSON.
        if self.sessions.values().any(|s| s.wants("telemetry")) {
            let data = serde_json::to_value(&tele).unwrap_or(Value::Null);
            self.broadcast("telemetry", &data);
        }
        self.latest = Some(tele);
        if !changed.is_empty() {
            let list = serde_json::to_value(self.devices.view()).unwrap_or(Value::Null);
            self.broadcast("device", &json!({"gpus": list, "changed": changed}));
        }
        if let Some(ev) = log_ev {
            self.broadcast("log", &ev);
        }
        for a in alarms.into_iter().chain(disk_alarms) {
            self.broadcast("alarm", &a);
        }
        if let Some(ev) = self.logger.tick(now) {
            // Loggen kan stoppe av seg selv (full disk): ny takt og kanskje stopp.
            self.broadcast("log", &ev);
            self.reschedule();
            self.check_idle();
        }
        if self.txn.tick(now) {
            self.after_txn();
        }
        self.hjelper_tick(now);
        if now - self.last_stats_event >= 5_000 {
            self.last_stats_event = now;
            if self.sessions.values().any(|s| s.wants("stats")) {
                let s = self.stats.to_json(None);
                self.broadcast("stats", &s);
            }
        }
        if now - self.last_history_save >= 60_000 {
            self.last_history_save = now;
            self.history.save(now);
            // Gi ledig malloc-minne tilbake til systemet (store svar på
            // get_history, lagringen, prøvetrådens arena), så privat minne
            // følger det tjenesten faktisk holder (runde 3C, lekkasjetesten).
            // SAFETY: malloc_trim har ingen forutsetninger.
            unsafe {
                libc::malloc_trim(0);
            }
        }
        if self.settings.keep_min_max && self.stats.dirty && now - self.last_minmax_save >= 30_000 {
            self.last_minmax_save = now;
            let _ = self.stats.save(&self.store.minmax_path());
        }
        if !self.ready {
            self.ready = true;
            crate::lifecycle::sd_notify("READY=1");
            self.check_idle();
        }
    }
}

/// `sim:hete` (runde 8A): temperaturene fra hete-syklusen, også etter
/// viftesløyfen (som ellers regner temperaturen fra effekten), og
/// prosessoren og to av de simulerte diskene. Viftene følger driverens kurve.
fn overlay_hete(gpus: &mut BTreeMap<String, rigg_core::model::GpuSample>, sys: &mut rigg_core::model::SysSample, since_s: f64) {
    let h = rigg_hw::gpu::sim::hete(since_s);
    for g in gpus.values_mut() {
        g.temp = Some(h.gpu);
        let fan = rigg_hw::gpu::sim_vifter::driverkurve(h.gpu).round();
        g.fan = Some(fan);
        for f in g.fans.iter_mut().chain(g.fan_target.iter_mut()) {
            *f = Some(fan);
        }
    }
    sys.cpu.temp = Some(h.cpu);
    if let Some(d) = sys.disks.first_mut() {
        d.temp = Some(h.disk_varm);
    }
    if let Some(d) = sys.disks.get_mut(3) {
        d.temp = Some(h.disk_kald);
    }
}
