//! Alle operasjonene (arkitektur.md kap. 5 og docs/produksjon/protokoll.md).
//!
//! Leseoperasjonene står her. Logg-operasjonene kaller `logg.rs` (2B).
//! Skriveoperasjonene (runde 7A): er root-hjelperen aktivert, går de til
//! `hjelper.rs` (videre til hjelperen når den er på, fra samme bygg, med samme
//! backend og trinnet godkjent; ellers `write_locked` med grunn eller
//! `step_not_enabled`). Uten hjelper går de som før til
//! `rigg_core::txn::Book::handle`, som svarer `write_locked` for alt annet
//! enn `--backend sim`. Med sim er det bekreftelsesflyten i minnet.

use crate::daemon::Daemon;
use rigg_core::history::{MAX_KEYS, MAX_POINTS, MIN_POINTS, WINDOWS};
use rigg_core::proto::{self, ErrorCode, OpResult, ProtoError, Request};
use rigg_core::settings::merge_service;
use serde_json::{Value, json};

/// Hvor lenge prosesslista leses etter siste `get_processes` (ms).
const PROCESSES_KEEP_MS: i64 = 15_000;

fn invalid(msg: impl Into<String>) -> ProtoError {
    ProtoError::msg(ErrorCode::InvalidArgument, msg)
}

/// `keys` som liste med gyldige sensornøkler (høyst `max`).
fn keys_arg(args: &Value, max: usize) -> Result<Option<Vec<String>>, ProtoError> {
    let Some(v) = args.get("keys") else { return Ok(None) };
    if v.is_null() {
        return Ok(None);
    }
    let a = v.as_array().ok_or_else(|| invalid("keys må være en liste"))?;
    if a.len() > max {
        return Err(invalid(format!("Høyst {max} nøkler per forespørsel")));
    }
    let mut out = Vec::new();
    for k in a {
        let s = k.as_str().filter(|s| rigg_core::sensor_key::valid(s)).ok_or_else(|| invalid("Ugyldig sensornøkkel"))?;
        out.push(s.to_string());
    }
    Ok(Some(out))
}

impl Daemon {
    pub fn handle(&mut self, sid: u64, req: &Request) -> OpResult {
        let hello = self.sessions.get(&sid).is_some_and(|s| s.hello);
        if req.op != "hello" && !hello {
            return Err(ProtoError::new(ErrorCode::HelloRequired));
        }
        if self.stopping {
            return Err(ProtoError::new(ErrorCode::ShuttingDown));
        }
        let a = &req.args;
        match req.op.as_str() {
            "hello" => self.op_hello(sid, a),
            "ping" => Ok(self.op_ping()),
            "list_gpus" => Ok(self.op_list_gpus()),
            "get_static" => self.op_get_static(a),
            "get_system" => Ok(self.op_get_system()),
            "get_limits" => self.op_get_limits(a),
            "get_state" => self.op_get_state(a),
            "get_telemetry" => Ok(serde_json::to_value(&self.latest).unwrap_or(Value::Null)),
            "subscribe" => self.op_subscribe(sid, a, true),
            "unsubscribe" => self.op_subscribe(sid, a, false),
            "set_focus" => self.op_set_focus(sid, a),
            "get_history" => self.op_get_history(a),
            "get_stats" => Ok(self.stats.to_json(keys_arg(a, 4096)?.as_deref())),
            "reset_stats" => self.op_reset_stats(a),
            "get_processes" => self.op_get_processes(a),
            "get_tasks" => self.op_get_tasks(sid, a),
            "get_task_detail" => self.op_get_task_detail(a),
            "terminate_task" => self.op_terminate_task(a),
            "get_disks" => Ok(self.op_get_disks()),
            "log_start" => self.op_log_start(a),
            "log_stop" => self.op_log_stop(),
            "log_status" => Ok(self.logger.status()),
            "get_settings" => Ok(self.op_get_settings()),
            "set_settings" => self.op_set_settings(a),
            "clear_history" => {
                self.history.clear();
                Ok(json!({"cleared": true}))
            }
            "get_helper" => self.op_get_helper(a),
            "shutdown_when_idle" => {
                self.idle_shutdown = true;
                self.check_idle();
                let v = self.op_get_settings();
                self.broadcast("settings", &v);
                Ok(json!({"idleShutdown": true, "graceMs": self.opts.grace_ms}))
            }
            op if proto::WRITE_OPS.contains(&op) => {
                // Root-hjelperen først (hjelper.rs, runde 7A). Er den ikke
                // aktivert: to lås som før, her og i `Book::handle`, som
                // også svarer `write_locked` for alt annet enn sim.
                if let Some(r) = self.via_hjelper(sid, op, a) {
                    return r;
                }
                if self.opts.backend != "sim" {
                    return Err(rigg_core::hjelper::locked("helper_off"));
                }
                let r = self.txn.handle(&self.opts.backend, op, a, &format!("s{sid}"), rigg_core::now_ms());
                self.after_txn();
                r
            }
            _ => Err(ProtoError::new(ErrorCode::UnknownOp).details(json!({"op": req.op}))),
        }
    }

    fn op_hello(&mut self, sid: u64, a: &Value) -> OpResult {
        let wanted = a.get("protocol").and_then(Value::as_array);
        let ok = wanted.is_some_and(|l| l.iter().any(|v| v.as_u64() == Some(proto::PROTOCOL as u64)));
        if !ok {
            return Err(ProtoError::new(ErrorCode::UnsupportedProtocol).details(json!({"supported": [proto::PROTOCOL]})));
        }
        let client = a.get("client").and_then(Value::as_str).unwrap_or("").chars().take(64).collect();
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.hello = true;
            s.client = client;
        }
        let info = proto::DaemonInfo {
            name: "agentic-riggd".into(),
            version: rigg_core::version::VERSION.into(),
            build: rigg_core::version::build(),
            mock: self.opts.backend == "sim",
            pid: std::process::id(),
            backend: self.opts.backend.clone(),
            started_at: self.started_at,
        };
        let gpus: Vec<String> = self.devices.list.iter().filter(|g| !g.id.is_empty()).map(|g| g.id.clone()).collect();
        let mut ops = proto::ops_for(&self.opts.backend);
        ops.push("get_rgb_helper");
        ops.extend_from_slice(rigg_core::rgb_auth::UI_OPS);
        if self.hjelper.is_on() {
            for o in proto::WRITE_OPS {
                if !ops.contains(o) {
                    ops.push(o);
                }
            }
        }
        Ok(json!({
            "protocol": proto::PROTOCOL,
            "session": format!("s{sid}"),
            "daemon": info,
            "capabilities": {
                "gpus": gpus,
                "ops": ops,
                "events": proto::EVENTS,
                "write": self.write_caps(),
            },
            "intervalMs": self.interval_ms,
            "maxMessageBytes": proto::MAX_LINE,
            "notes": self.notes,
        }))
    }

    /// `ping`: klokka, og tjenestens egen tilstand til Innstillinger › Om.
    fn op_ping(&self) -> Value {
        let mem = |key: &str| -> Option<u64> {
            let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
            text.lines().find(|l| l.starts_with(key))?.split_whitespace().nth(1)?.parse().ok()
        };
        let clients: Vec<Value> = self.sessions.values().filter(|s| s.hello).map(|s| json!({"session": format!("s{}", s.id), "client": s.client, "pid": s.pid})).collect();
        json!({
            "t": rigg_core::now_ms(),
            "startedAt": self.started_at,
            "pid": std::process::id(),
            "intervalMs": self.interval_ms,
            "background": self.settings.background,
            "rssKB": mem("Rss:"),
            "pssKB": mem("Pss:"),
            "privateKB": mem("Private_Dirty:"),
            "roundUs": self.last_cost_us,
            "clients": clients,
            "historySeries": self.history.keys().len(),
        })
    }

    fn op_list_gpus(&self) -> Value {
        json!({"gpus": self.devices.view(), "backend": self.opts.backend})
    }

    fn gpu_arg(&self, a: &Value) -> Result<String, ProtoError> {
        match a.get("gpu").and_then(Value::as_str) {
            Some(id) if self.devices.known(id) || self.statics.contains_key(id) => Ok(id.to_string()),
            Some(_) => Err(ProtoError::new(ErrorCode::UnknownGpu)),
            None => self.devices.first_id().ok_or_else(|| ProtoError::msg(ErrorCode::UnknownGpu, "Fant ikke noe skjermkort.")),
        }
    }

    fn op_get_static(&self, a: &Value) -> OpResult {
        let id = self.gpu_arg(a)?;
        match self.statics.get(&id) {
            Some((st, done)) => Ok(json!({"static": st, "complete": done})),
            None => Ok(json!({"static": null, "complete": false})),
        }
    }

    fn op_get_system(&mut self) -> Value {
        let root = self.opts.root.clone();
        if self.system.is_none() {
            self.system = Some(json!({
                "board": rigg_hw::sys::dmi::read(&root),
                "os": rigg_hw::sys::os::read(&root),
                "pci": rigg_hw::sys::pci::read_board(&root),
            }));
        }
        let mut v = self.system.clone().unwrap_or(Value::Null);
        // CPU-topologien leses fersk så CPU-hotplug alltid gir gjeldende
        // aktive tråd-ID-er, kjerner, L3-grupper og cpufreq-data.
        v["cpu"] = rigg_hw::sys::cpuinfo::read(&root);
        // Diskene med SMART-helse til rapporten (runde 6A), når lista finnes.
        if let Some(d) = &self.disks {
            v["disks"] = serde_json::to_value(d).unwrap_or(Value::Null);
        }
        if let Some(s) = self.latest.as_ref() {
            // CPU-vektorene bruker faktiske Linux-ID-er; hull er ikke tråder.
            v["threads"] = v["cpu"]["threads"].clone();
            v["memTotalKB"] = json!(s.sys.mem.total_kb);
        }
        v
    }

    fn op_get_limits(&self, a: &Value) -> OpResult {
        let id = self.gpu_arg(a)?;
        let st = self.statics.get(&id).map(|s| &s.0);
        let f = |v: Option<f64>| v.map_or(Value::Null, |x| json!(x));
        Ok(json!({
            "gpu": id,
            "powerLimitW": {"min": f(st.and_then(|s| s.power_min)), "max": f(st.and_then(|s| s.power_max)), "default": f(st.and_then(|s| s.power_default)), "source": "hardware"},
            "tempTargetC": {"value": f(st.and_then(|s| s.temp_target)), "max": f(st.and_then(|s| s.temp_max)), "slowdown": f(st.and_then(|s| s.temp_slowdown)), "shutdown": f(st.and_then(|s| s.temp_shutdown)), "source": "hardware"},
            "fan": {"count": st.and_then(|s| s.fan_count), "min": f(st.and_then(|s| s.fan_min)), "max": f(st.and_then(|s| s.fan_max)), "source": "hardware"},
            "write": self.write_caps(),
        }))
    }

    /// `get_processes {gpu?}`: prosessene som bruker kortet. Prøvetråden leser
    /// lista hvert 5. s i 15 s etter siste forespørsel (vinduet spør mens
    /// Grafikkort vises). Før første lesing gis øyeblikksbildet fra de faste
    /// dataene, med `live: false`.
    fn op_get_processes(&mut self, a: &Value) -> OpResult {
        let id = self.gpu_arg(a)?;
        let now = rigg_core::now_ms();
        if self.processes_until == 0 {
            self.sampler.send(crate::sampler::Cmd::Processes(true));
        }
        self.processes_until = now + PROCESSES_KEEP_MS;
        let (p, live) = match self.processes.get(&id) {
            Some(p) => (p.clone(), true),
            None => (self.statics.get(&id).map(|s| s.0.extra["processes"].clone()).unwrap_or(Value::Null), false),
        };
        Ok(json!({"gpu": id, "processes": p, "live": live}))
    }

    /// `get_disks`: diskene med SMART og partisjonene (runde 6A). Lista lages
    /// av prøvetråden hvert 60. s mens et vindu er åpent; er den eldre (eller
    /// mangler), bes det om en ny, og svaret er den forrige (`pending` når
    /// ingen finnes ennå).
    fn op_get_disks(&mut self) -> Value {
        let now = rigg_core::now_ms();
        let stale = self.disks.as_ref().is_none_or(|d| now - d.t > 65_000);
        if stale && now - self.disks_asked > 5_000 {
            self.disks_asked = now;
            self.sampler.send(crate::sampler::Cmd::Disks);
        }
        match &self.disks {
            Some(d) => {
                let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
                v["pending"] = json!(false);
                v
            }
            None => json!({"t": 0, "source": "", "note": null, "disks": [], "pending": true}),
        }
    }

    fn op_get_state(&mut self, a: &Value) -> OpResult {
        let id = self.gpu_arg(a)?;
        if let Some(v) = self.hjelper_state(&id) {
            return Ok(v);
        }
        if self.opts.backend == "sim" {
            let mut v = self.txn.state_view(rigg_core::now_ms());
            v["gpu"] = json!(id);
            v["write"] = self.write_caps();
            return Ok(v);
        }
        let s = self.latest.as_ref().and_then(|t| t.gpus.get(&id));
        Ok(json!({
            "gpu": id,
            "active": {
                "powerLimitW": s.and_then(|s| s.power_limit),
                "tempTargetC": self.statics.get(&id).and_then(|x| x.0.temp_target),
                "fan": {"percent": s.map(|s| s.fans.clone())},
            },
            "pending": null,
            "emergency": false,
            "write": self.write_caps(),
        }))
    }

    fn op_subscribe(&mut self, sid: u64, a: &Value, on: bool) -> OpResult {
        let topics: Vec<String> = match a.get("topics") {
            None => {
                if on {
                    vec!["telemetry".into()]
                } else {
                    vec![]
                }
            }
            Some(v) => {
                let l = v.as_array().ok_or_else(|| invalid("topics må være en liste"))?;
                let mut out = Vec::new();
                for t in l {
                    let t = t.as_str().filter(|t| proto::TOPICS.contains(t)).ok_or_else(|| invalid("Ukjent emne"))?;
                    out.push(t.to_string());
                }
                out
            }
        };
        let iv = a.get("intervalMs").and_then(Value::as_u64);
        if a.get("intervalMs").is_some_and(|v| !v.is_null() && !v.is_u64()) {
            return Err(invalid("intervalMs må være et tall"));
        }
        let s = self.sessions.get_mut(&sid).ok_or_else(|| ProtoError::new(ErrorCode::Internal))?;
        for t in &topics {
            if on {
                s.topics.insert(t.clone());
            } else {
                s.topics.remove(t);
            }
        }
        if on && iv.is_some() {
            s.interval_ms = Some(proto::clamp_interval(iv));
        }
        let mine: Vec<String> = s.topics.iter().cloned().collect();
        let mine_iv = s.interval_ms.unwrap_or(1000);
        self.reschedule();
        Ok(json!({"topics": mine, "intervalMs": mine_iv, "effectiveMs": self.interval_ms}))
    }

    fn op_set_focus(&mut self, sid: u64, a: &Value) -> OpResult {
        let pages = a.get("pages").and_then(Value::as_array).ok_or_else(|| invalid("pages må være en liste"))?;
        let mut list = Vec::new();
        for p in pages.iter().take(16) {
            list.push(p.as_str().ok_or_else(|| invalid("pages må være tekst"))?.chars().take(32).collect::<String>());
        }
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.focus = Some(list.clone());
        }
        self.reschedule();
        Ok(json!({"pages": list, "effectiveMs": self.interval_ms}))
    }

    fn op_get_history(&self, a: &Value) -> OpResult {
        let window = a.get("window").and_then(Value::as_i64).filter(|w| WINDOWS.contains(w)).ok_or_else(|| {
            invalid(format!("window må være en av {} s", WINDOWS.map(|w| w.to_string()).join(", ")))
        })?;
        let keys = keys_arg(a, MAX_KEYS)?.ok_or_else(|| invalid("keys mangler"))?;
        let points = match a.get("points") {
            None => 1000,
            Some(p) => p.as_u64().map(|p| p as usize).filter(|p| (MIN_POINTS..=MAX_POINTS).contains(p)).ok_or_else(|| invalid(format!("points må være {MIN_POINTS}–{MAX_POINTS}")))?,
        };
        let r = self.history.query(rigg_core::now_ms(), window, &keys, points);
        Ok(serde_json::to_value(&r).unwrap_or(Value::Null))
    }

    fn op_reset_stats(&mut self, a: &Value) -> OpResult {
        let keys = keys_arg(a, 4096)?;
        self.stats.reset(keys.as_deref(), rigg_core::now_ms());
        if self.settings.keep_min_max {
            let _ = self.stats.save(&self.store.minmax_path());
        }
        let s = self.stats.to_json(None);
        self.broadcast("stats", &s);
        Ok(s)
    }

    fn op_get_settings(&self) -> Value {
        let mut v = self.settings.to_value();
        // «Stopp tjenesten nå» er valgt: tjenesten avslutter når siste klient kobler fra.
        v["idleShutdown"] = json!(self.idle_shutdown);
        v["paths"] = json!({
            "config": self.store.config_dir.display().to_string(),
            "state": self.store.state_dir.display().to_string(),
            "socket": self.server.path.display().to_string(),
        });
        v
    }

    /// Start logging (også fra systemfeltet, med standardvalgene).
    pub(crate) fn op_log_start(&mut self, a: &Value) -> OpResult {
        let s = self.settings.log.clone();
        let r = self.logger.start(a, &s, rigg_core::now_ms());
        self.reschedule();
        self.check_idle();
        if let Ok(st) = &r {
            self.broadcast("log", st);
        }
        r
    }

    pub(crate) fn op_log_stop(&mut self) -> OpResult {
        let r = self.logger.stop(rigg_core::now_ms());
        self.reschedule();
        self.check_idle();
        if let Ok(st) = &r {
            self.broadcast("log", st);
        }
        r
    }

    pub(crate) fn op_set_settings(&mut self, a: &Value) -> OpResult {
        let next = merge_service(&self.settings, a).map_err(|e| invalid(e.0))?;
        let keep_changed = next.keep_min_max != self.settings.keep_min_max;
        self.settings = next;
        self.txn.confirm_s = self.settings.confirm_sec;
        self.sync_disk_alarm();
        if let Err(e) = self.store.save_settings(&self.settings) {
            eprintln!("Kunne ikke lagre tjeneste.json: {e}");
        }
        if keep_changed {
            if self.settings.keep_min_max {
                let _ = self.stats.save(&self.store.minmax_path());
            } else {
                let _ = std::fs::remove_file(self.store.minmax_path());
            }
        }
        if a.get("background").is_some() {
            self.idle_shutdown = false;
        }
        self.reschedule();
        self.check_idle();
        let v = self.op_get_settings();
        self.broadcast("settings", &v);
        Ok(v)
    }
}
