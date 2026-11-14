//! Klienten mot root-hjelperen (`agentic-rigg-helper`, runde 7A).
//!
//! Tjenesten finner hjelperen ut fra `helper.json` og socketen
//! (`rigg_core::hjelper::Paths`; `/` på et ekte system, en falsk rot med
//! `--hjelper-rot` i testene og `RIGG_HJELPER_ROT` i utviklerprofilen), spør om
//! `status`, og sender `apply`, `confirm`, `revert` og `reset` videre når
//! hjelperen er på, har kompatibelt bygg/protokoll, samme backend, global autorisasjon
//! og samsvarende kortidentitet. Ellers svarer `ops.rs` med `write_locked` og en
//! presis grunn i `details.reason` (`helper_off`, `helper_error`,
//! `helper_outdated`, `backend_mismatch` eller `identity_mismatch`).
//!
//! Bare den separat kompilerte root-hjelperen bruker HW-skriveren. NVML
//! oppdages uten testskriving, og hvert eksplisitt endret felt må støttes.
//!
//! Forbindelsen holdes åpen mens hjelperen er på: lukkes den (tjenesten
//! stopper eller krasjer), gjenoppretter hjelperen egne endringer. Samtalen er
//! blokkerende med 2 s frist, men hjelperen svarer på under ett millisekund.

use crate::daemon::Daemon;
use rigg_core::hjelper::{self, Paths, Step};
use rigg_core::proto::{self, ErrorCode, Incoming, LineBuffer, LineEvent, OpResult, ProtoError};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Hvor ofte en hjelper som er av, sjekkes (en `stat`), og hvor ofte en som
/// er på, får `status` uten at noe venter (ms).
const PROBE_OFF_MS: i64 = 60_000;
const PROBE_ON_MS: i64 = 30_000;
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Ikke aktivert (ingen `helper.json`).
    Off,
    /// På og med kompatibelt bygg/protokoll.
    On,
    /// Aktivert, men svarer ikke (eller svarte feil).
    Error(String),
    /// På, men med inkompatibelt bygg/protokoll: hjelperen må oppdateres.
    Outdated(u32),
}

fn state_for_status(status: &Value) -> State {
    let build = status["helper"]["build"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok());
    let protocol = status["helper"]["protocol"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok());
    if let (Some(protocol), Some(build)) = (protocol, build)
        && hjelper::compatible(protocol, build)
    {
        State::On
    } else {
        State::Outdated(build.unwrap_or(0))
    }
}

struct Conn {
    stream: UnixStream,
    buf: LineBuffer,
}

pub struct Hjelper {
    pub paths: Paths,
    pub state: State,
    /// Siste `status` fra hjelperen.
    pub status: Value,
    /// Økta i tjenesten som eier endringen som venter i hjelperen.
    pub owner: Option<u64>,
    conn: Option<Conn>,
    seq: u64,
    last_probe: i64,
}

impl Hjelper {
    pub fn new(paths: Paths) -> Hjelper {
        Hjelper {
            paths,
            state: State::Off,
            status: Value::Null,
            owner: None,
            conn: None,
            seq: 0,
            last_probe: 0,
        }
    }

    /// Stiene: `--hjelper-rot` (testene) eller `RIGG_HJELPER_ROT` i
    /// utviklerprofilen. Ellers alltid `/`.
    pub fn paths_from(arg: Option<std::path::PathBuf>, dev: bool) -> Paths {
        if let Some(r) = arg {
            return Paths::new(r);
        }
        match std::env::var_os("RIGG_HJELPER_ROT") {
            Some(r) if dev && !r.is_empty() => Paths::new(r),
            _ => Paths::new("/"),
        }
    }

    pub fn is_on(&self) -> bool {
        self.state == State::On
    }

    pub fn steps(&self) -> Vec<Step> {
        if !self.is_on() {
            return vec![];
        }
        self.status["steps"]
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|s| s.as_str().and_then(Step::parse))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn backend(&self) -> &str {
        self.status["helper"]["backend"].as_str().unwrap_or("")
    }

    pub fn pending(&self) -> bool {
        !self.status["state"]["pending"].is_null() && self.status["state"]["pending"].is_object()
    }

    fn emergency(&self) -> bool {
        self.status["emergency"].as_bool().unwrap_or(false)
    }

    /// Har hjelperen verdier som ikke er standard? Da holdes tjenesten i
    /// live, for når forbindelsen lukkes, går kortet til standard.
    pub fn holds_values(&self) -> bool {
        if !self.is_on() {
            return false;
        }
        if let Some(holding) = self.status["holding"].as_bool() {
            return holding || self.pending() || self.status["state"]["hardwareKnown"] == false;
        }
        self.pending() || self.status["hardware"]["stock"] == false
    }

    fn drop_conn(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn connect(&mut self) -> Result<(), String> {
        if self.conn.is_some() {
            return Ok(());
        }
        let s = UnixStream::connect(self.paths.socket())
            .map_err(|e| format!("Fikk ikke kontakt med hjelperen ({e})"))?;
        s.set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        s.set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        self.conn = Some(Conn {
            stream: s,
            buf: LineBuffer::new(),
        });
        Ok(())
    }

    /// Én forespørsel til hjelperen. Feil i forbindelsen gir `Error`.
    pub fn request(&mut self, op: &str, args: Value) -> OpResult {
        match self.request_inner(op, args) {
            Ok(r) => r,
            Err(e) => {
                self.drop_conn();
                self.state = State::Error(e.clone());
                Err(hjelper::locked("helper_error")
                    .details(json!({"reason": "helper_error", "error": e})))
            }
        }
    }

    fn request_inner(&mut self, op: &str, args: Value) -> Result<OpResult, String> {
        self.connect()?;
        self.seq += 1;
        let id = self.seq;
        let c = self.conn.as_mut().expect("forbindelsen");
        let mut line = serde_json::to_vec(&json!({"id": id, "op": op, "args": args}))
            .map_err(|e| e.to_string())?;
        line.push(b'\n');
        if let Err(e) = c.stream.write_all(&line) {
            // Hjelperen kan ha avvist forbindelsen (feil bruker) og lukket den
            // før vi rakk å skrive: les avvisningen som allerede ligger der,
            // så grunnen ikke blir «Broken pipe» (runde 8A, sett under last).
            let mut chunk = [0u8; 8192];
            if let Ok(n) = c.stream.read(&mut chunk)
                && n > 0
            {
                for ev in c.buf.push(&chunk[..n], 1 << 20) {
                    if let LineEvent::Line(l) = ev
                        && let Some(Incoming::Response { id: rid, result }) =
                            proto::parse_incoming(&l)
                        && rid.is_null()
                    {
                        return Ok(result);
                    }
                }
            }
            return Err(format!("Hjelperen tok ikke imot ({e})"));
        }
        let mut chunk = [0u8; 8192];
        loop {
            let n = c
                .stream
                .read(&mut chunk)
                .map_err(|e| format!("Hjelperen svarte ikke ({e})"))?;
            if n == 0 {
                return Err("Hjelperen lukket forbindelsen".into());
            }
            for ev in c.buf.push(&chunk[..n], 1 << 20) {
                let LineEvent::Line(l) = ev else {
                    return Err("For stort svar fra hjelperen".into());
                };
                if let Some(Incoming::Response { id: rid, result }) = proto::parse_incoming(&l) {
                    if rid == json!(id) {
                        return Ok(result);
                    }
                    // Svar uten id: hjelperen avviste forbindelsen.
                    if rid.is_null() {
                        return Ok(result);
                    }
                }
            }
        }
    }

    /// Ser etter hjelperen og spør om `status`.
    pub fn probe(&mut self, now: i64) {
        self.last_probe = now;
        let cfg = self.paths.config();
        let sock = self.paths.socket();
        if !cfg.exists() && !sock.exists() {
            self.drop_conn();
            self.state = State::Off;
            self.status = Value::Null;
            self.owner = None;
            return;
        }
        match self.request("status", json!({})) {
            Ok(s) => {
                self.state = state_for_status(&s);
                self.status = s;
                if !self.pending() {
                    self.owner = None;
                }
            }
            Err(e) => {
                self.drop_conn();
                let msg = match e.code {
                    ErrorCode::PermissionDenied => {
                        "Hjelperen er aktivert for en annen bruker.".to_string()
                    }
                    _ => e
                        .details
                        .as_ref()
                        .and_then(|d| d["error"].as_str())
                        .map(str::to_string)
                        .unwrap_or(e.message),
                };
                self.state = State::Error(if cfg.exists() {
                    msg
                } else {
                    "Hjelperen kjører, men helper.json mangler.".into()
                });
                self.status = Value::Null;
            }
        }
    }

    /// Til `hello`, `get_helper` og Innstillinger › Styring.
    pub fn view(&self) -> Value {
        let (state, message) = match &self.state {
            State::Off => ("off", None),
            State::On => ("on", None),
            State::Error(e) => ("error", Some(e.clone())),
            State::Outdated(b) => (
                "outdated",
                Some(format!(
                    "Hjelperens bygg {b} eller protokoll er inkompatibel. Appbygg {} støtter hjelperbygg {}–{} med protokoll {}. Oppdater hjelperen via bryteren.",
                    rigg_core::version::build(),
                    hjelper::MIN_COMPATIBLE_BUILD,
                    rigg_core::version::build(),
                    hjelper::PROTOCOL
                )),
            ),
        };
        let h = &self.status["helper"];
        json!({
            "state": state,
            "message": message,
            "steps": self.steps().iter().map(|s| s.id()).collect::<Vec<_>>(),
            "backend": h["backend"],
            "version": h["version"],
            "build": h["build"],
            "realWrites": h["realWrites"].as_bool().unwrap_or(false),
            "enabled": self.is_on() && self.status["enabled"] == true,
            "ready": self.is_on() && self.status["ready"] == true,
            "identity": self.status["identity"],
            "capabilities": self.status["capabilities"],
            "hardware": self.status["hardware"],
            "hardwareError": self.status["state"]["hardwareError"],
            "legacyConfig": self.status["legacyConfig"],
            "emergency": self.emergency(),
            "pending": self.pending(),
            "paths": {"config": self.paths.config(), "socket": self.paths.socket()},
        })
    }
}

impl Daemon {
    /// `capabilities.write`: trinnene fra `proto`, med de godkjente fra
    /// hjelperen, og `helper` med tilstanden.
    pub fn write_caps(&self) -> Value {
        let mut w = proto::write_capabilities(&self.opts.backend);
        let fields = if self.hjelper.is_on() {
            self.hjelper.status["capabilities"].clone()
        } else {
            w["fields"].clone()
        };
        if let Some(list) = w["steps"].as_array_mut() {
            for step in list {
                let id = step["id"].as_str().unwrap_or("").to_string();
                let keys: Vec<&str> = match id.as_str() {
                    "clocks" => vec!["core_clock", "mem_clock"],
                    _ => vec![id.as_str()],
                };
                let available = keys.iter().any(|key| fields[*key]["available"] == true);
                step["status"] = json!(if available {
                    "supported"
                } else {
                    "unavailable"
                });
                step["reason"] = json!(
                    keys.iter()
                        .filter_map(|key| fields[*key]["reason"].as_str())
                        .collect::<Vec<_>>()
                        .join(" · ")
                );
            }
        }
        w["helper"] = self.hjelper.view();
        w["fields"] = fields;
        w["identity"] = self.hjelper.status["identity"].clone();
        if self.opts.backend == "nvml"
            && w["helper"]["ready"] == true
            && w["helper"]["enabled"] == true
        {
            w["state"] = json!("ready");
        }
        w
    }

    /// `get_helper {refresh?}`: tilstanden, og ny `status` når vinduet ber om
    /// det (etter bryteren).
    pub fn op_get_helper(&mut self, a: &Value) -> OpResult {
        if a.get("refresh").and_then(Value::as_bool).unwrap_or(false) {
            self.hjelper.probe(rigg_core::now_ms());
        }
        Ok(self.hjelper.view())
    }

    /// Ruter skriveoperasjonen. `None`: hjelperen er av, og `ops.rs` svarer
    /// som før (sim-boka eller `write_locked`).
    pub fn via_hjelper(&mut self, sid: u64, op: &str, a: &Value) -> Option<OpResult> {
        let now = rigg_core::now_ms();
        if self.hjelper.state == State::Off && now - self.hjelper.last_probe > 5_000 {
            // Kanskje aktivert siden sist (én stat).
            self.hjelper.probe(now);
        }
        match &self.hjelper.state {
            State::Off => return None,
            State::Error(_) => return Some(Err(hjelper::locked("helper_error"))),
            State::Outdated(_) => return Some(Err(hjelper::locked("helper_outdated"))),
            State::On => {}
        }
        if self.hjelper.backend() != self.opts.backend {
            return Some(Err(hjelper::locked("backend_mismatch").details(json!({"reason": "backend_mismatch", "helper": self.hjelper.backend(), "service": self.opts.backend}))));
        }
        let mut identity = json!({});
        if self.opts.backend == "nvml" {
            let Some(pci) = a["gpu"].as_str() else {
                return Some(Err(ProtoError::msg(
                    ErrorCode::InvalidArgument,
                    "gpu og uuid må oppgis ved styring",
                )));
            };
            let Some(uuid) = a["uuid"].as_str() else {
                return Some(Err(ProtoError::msg(
                    ErrorCode::InvalidArgument,
                    "uuid må oppgis ved styring",
                )));
            };
            let bound = &self.hjelper.status["identity"];
            let known_uuid = self.statics.get(pci).and_then(|(s, _)| s.uuid.as_deref());
            if bound["pci"] != pci || bound["uuid"] != uuid || known_uuid != Some(uuid) {
                return Some(Err(hjelper::locked("identity_mismatch")));
            }
            if self.hjelper.status["enabled"] != true
                || (op != "reset" && self.hjelper.status["ready"] != true)
            {
                return Some(Err(hjelper::locked("recovery_failed")));
            }
            identity = json!({"gpu": pci, "uuid": uuid});
        }
        let pending_owner = self.hjelper.owner.filter(|_| self.hjelper.pending());
        let r = match op {
            "confirm" | "revert" => {
                if pending_owner.is_some_and(|o| o != sid) {
                    return Some(Err(ProtoError::new(ErrorCode::NotOwner)));
                }
                {
                    let mut args = identity.clone();
                    args["txn"] = a.get("txn").cloned().unwrap_or(Value::Null);
                    self.hjelper.request(op, args)
                }
            }
            "reset" => {
                if pending_owner.is_some_and(|o| o != sid) {
                    return Some(Err(ProtoError::new(ErrorCode::Busy)));
                }
                self.hjelper.request("reset", identity.clone())
            }
            "rgb_get" | "rgb_set" => self.hjelper.request(op, a.clone()),
            _ => {
                let Some(patch) = hjelper::patch_for_op(op, a) else {
                    return Some(Err(ProtoError::new(ErrorCode::UnknownOp)));
                };
                if let Err(e) = hjelper::steps_for_patch(&patch) {
                    return Some(Err(e));
                }
                if let Some(o) = pending_owner
                    && o != sid
                {
                    return Some(Err(ProtoError::new(ErrorCode::Busy)));
                }
                let mut args = identity.clone();
                args["settings"] = patch;
                if let Some(t) = a.get("confirmTimeoutS") {
                    args["confirmTimeoutS"] = t.clone();
                } else {
                    args["confirmTimeoutS"] = json!(self.settings.confirm_sec);
                }
                let r = self.hjelper.request("apply", args);
                if r.as_ref().is_ok_and(|v| v["txn"].is_string()) {
                    self.hjelper.owner = Some(sid);
                }
                r
            }
        };
        self.hjelper_poll();
        Some(r)
    }

    /// `status` fra hjelperen, og `txn`-hendelsene den har samlet.
    fn hjelper_poll(&mut self) {
        if !self.hjelper.is_on() {
            return;
        }
        if let Ok(s) = self.hjelper.request("status", json!({})) {
            let events = s["events"].as_array().cloned().unwrap_or_default();
            self.hjelper.state = state_for_status(&s);
            self.hjelper.status = s;
            if !self.hjelper.pending() {
                self.hjelper.owner = None;
            }
            for ev in events {
                self.broadcast("txn", &ev);
            }
        }
    }

    /// Hver runde: mens noe venter eller nødregelen gjelder, `status` hver
    /// runde (fristen og hendelsene); ellers sjeldent.
    pub fn hjelper_tick(&mut self, now: i64) {
        let h = &self.hjelper;
        let due = match h.state {
            State::Off => now - h.last_probe >= PROBE_OFF_MS,
            _ if h.pending() || h.emergency() => true,
            _ => now - h.last_probe >= PROBE_ON_MS,
        };
        if !due {
            return;
        }
        if self.hjelper.is_on() {
            self.hjelper.last_probe = now;
            self.hjelper_poll();
        } else {
            self.hjelper.probe(now);
        }
    }

    /// En økt er lukket: det den har som venter i hjelperen, rulles tilbake.
    pub fn hjelper_session_closed(&mut self, sid: u64) {
        if self.hjelper.owner == Some(sid) && self.hjelper.pending() {
            let txn = self.hjelper.status["state"]["pending"]["txn"].clone();
            let _ = self.hjelper.request("revert", json!({"txn": txn, "gpu": self.hjelper.status["identity"]["pci"], "uuid": self.hjelper.status["identity"]["uuid"]}));
            self.hjelper.owner = None;
            self.hjelper_poll();
        }
    }

    /// `get_state` når hjelperen er på: hjelperens bok.
    pub fn hjelper_state(&mut self, gpu: &str) -> Option<Value> {
        if !self.hjelper.is_on() || self.hjelper.backend() != self.opts.backend {
            return None;
        }
        self.hjelper_poll();
        if self.opts.backend == "nvml" && self.hjelper.status["identity"]["pci"] != gpu {
            return None;
        }
        let mut v = self.hjelper.status["state"].clone();
        v["hardware"] = self.hjelper.status["hardware"].clone();
        v["gpu"] = json!(gpu);
        v["write"] = self.write_caps();
        Some(v)
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn app24_kan_bruke_autorisert_hjelper23_med_samme_protokoll() {
        assert!(rigg_core::version::build() >= 24);
        assert_eq!(
            state_for_status(&json!({"helper":{"build":23,"protocol":2}})),
            State::On
        );
        assert_eq!(
            state_for_status(&json!({"helper":{"build":22,"protocol":2}})),
            State::Outdated(22)
        );
        assert_eq!(
            state_for_status(&json!({"helper":{"build":23,"protocol":1}})),
            State::Outdated(23)
        );
    }

    #[test]
    fn manglende_ugyldig_eller_fremtidig_identifikasjon_gir_ikke_klarstatus() {
        assert_ne!(state_for_status(&json!({"helper":{"build":23}})), State::On);
        assert_ne!(
            state_for_status(&json!({"helper":{"build":4_294_967_319_u64,"protocol":2}})),
            State::On
        );
        assert_ne!(
            state_for_status(&json!({"helper":{"build":23,"protocol":4_294_967_298_u64}})),
            State::On
        );
        assert_ne!(
            state_for_status(
                &json!({"helper":{"build":rigg_core::version::build()+1,"protocol":2}})
            ),
            State::On
        );
    }
}
