//! Motoren i hjelperen: autorisasjon, grensesjekk, bekreft eller rull tilbake
//! (`rigg_core::txn::Book`), nødregelen, viftekurven og journalen. Ingen
//! socket og ingen klokke her: tiden sendes inn, så alt kan testes direkte.
//! Tiden er serverens monotone `CLOCK_BOOTTIME`-tidslinje (ms), ikke
//! veggklokka: den alene håndhever fristene og gir `remainingMs`. Bare på
//! ledningen (`handle`) skrives `pending.deadline` om til veggklokke-ms
//! (`nå + remainingMs`), samme kontrakt som tjenestens egen sim-sti.
//!
//! Reglene (hjelper-sikkerhet.md kap. 3):
//! - Én global autorisasjon; hvert endret felt krever faktisk HW-støtte.
//! - Et tall utenfor området kortet melder, gir `invalid_argument` (ikke klemming).
//! - Alt som skrives, er midlertidig til `confirm` (frist 10–60 s, standard 15 s).
//! - Forbindelsen som skrev, forsvinner (`riggd` borte): ventende endring
//!   rulles tilbake, og kortet går til standard med viftene på Auto.
//! - Over 85 °C: ventende endring rulles tilbake, og viftene går til 100 %
//!   (når viftestyring støttes; ellers styrer driveren dem som før) til
//!   kortet er under 80 °C.
//! - Hjelperen stopper etter egen skriving: fangede grunnverdier gjenopprettes.

use crate::journal;
use rigg_core::hjelper::{self, Config, Step};
use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use rigg_core::txn::{self, Book, FanMode, Range, Values};
use rigg_core::vifte;
use rigg_hw::gpu::skriv::{Limits, Skriver};
use serde_json::{Value, json};

pub struct Motor {
    pub book: Book,
    /// `None`: det ekte kortet er sperret (`locked` sier hvorfor).
    pub hw: Option<Box<dyn Skriver>>,
    pub backend: &'static str,
    pub locked: Option<String>,
    pub cfg: Option<Config>,
    pub cfg_err: Option<String>,
    pub emergency: bool,
    /// Forbindelsen som skrev sist. Forsvinner den, går kortet til standard.
    lease: Option<u64>,
    lease_seen: i64,
    last_tick: Option<i64>,
    pub hardware_error: Option<String>,
    dirty_marker: Option<std::path::PathBuf>,
    events: Vec<Value>,
    /// Siste fart kurven ga (for å ikke skrive samme fart hver runde).
    curve_pct: Option<Vec<f64>>,
    /// Viftemotoren fra 7B (`rigg_core::vifte`): kurve, hysterese, rampe og
    /// stille flate. Bare i modus Kurve.
    vifte: Option<vifte::Motor>,
    /// Klokka for siste steg i viftemotoren: ett steg per runde, som i
    /// tjenestens sim-sløyfe, selv når `apply` og `tick` kommer samme ms.
    vifte_ms: Option<i64>,
    pub limits: Limits,
    /// Veggklokka for `pending.deadline` i svarene (testene bytter den ut).
    wall: fn() -> i64,
}

/// `pending.deadline` (monoton) til veggklokke-ms: `wall + remainingMs`.
fn wall_deadlines(v: &mut Value, wall: i64) {
    match v {
        Value::Object(map) => {
            if map.contains_key("deadline")
                && let Some(rem) = map.get("remainingMs").and_then(Value::as_i64)
            {
                map.insert("deadline".into(), json!(wall.saturating_add(rem.max(0))));
            }
            for x in map.values_mut() {
                wall_deadlines(x, wall);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| wall_deadlines(x, wall)),
        _ => {}
    }
}

fn owner(conn: u64) -> String {
    format!("c{conn}")
}

/// Et tall i `patch` som må ligge i `r`, ellers avvist.
fn in_range(
    field: &str,
    label: &str,
    unit: &str,
    v: Option<&Value>,
    r: Range,
) -> Result<(), ProtoError> {
    let Some(v) = v else { return Ok(()) };
    let Some(x) = v.as_f64().filter(|x| x.is_finite()) else {
        return Err(ProtoError::msg(
            ErrorCode::InvalidArgument,
            format!("{label} må være et tall"),
        ));
    };
    if x < r.min || x > r.max {
        return Err(ProtoError::msg(ErrorCode::InvalidArgument, format!("{label} må være {}–{} {unit} (kortets område). Ingenting er endret.", r.min, r.max))
            .details(json!({"field": field, "value": x, "min": r.min, "max": r.max, "source": "hardware", "reason": "out_of_range"})));
    }
    Ok(())
}

/// Grensesjekken før noe annet: verdier utenfor kortets områder avvises.
pub fn check_ranges(patch: &Value, l: &Limits) -> Result<(), ProtoError> {
    in_range(
        "powerLimitW",
        "Effektgrensen",
        "W",
        patch.get("powerLimitW"),
        l.power_w,
    )?;
    in_range(
        "tempTargetC",
        "Temperaturmålet",
        "°C",
        patch.get("tempTargetC"),
        l.temp_target_c,
    )?;
    if let Some(c) = patch.get("clockOffsets") {
        in_range(
            "clockOffsets.coreMhz",
            "Kjerneklokke-offset",
            "MHz",
            c.get("coreMhz"),
            l.core_mhz,
        )?;
        in_range(
            "clockOffsets.memMhz",
            "Minneklokke-offset",
            "MHz",
            c.get("memMhz"),
            l.mem_mhz,
        )?;
    }
    if let Some(f) = patch.get("fan") {
        in_range(
            "fan.manualPercent",
            "Viftefarten",
            "%",
            f.get("manualPercent"),
            l.fan_pct,
        )?;
        // Felles kurve og kurvene per vifte (7B): hvert punkt i kortets område.
        let per_fan = f
            .get("curves")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for pts in f
            .get("curve")
            .into_iter()
            .chain(per_fan)
            .filter_map(Value::as_array)
        {
            for p in pts {
                in_range(
                    "fan.curve",
                    "Viftefarten i kurven",
                    "%",
                    p.get(1),
                    l.fan_pct,
                )?;
                in_range(
                    "fan.curve",
                    "Temperaturen i kurven",
                    "°C",
                    p.get(0),
                    Range::new(0.0, 110.0),
                )?;
            }
        }
    }
    Ok(())
}

impl Motor {
    pub fn new(
        hw: Option<Box<dyn Skriver>>,
        backend: &'static str,
        locked: Option<String>,
    ) -> Motor {
        let limits = hw
            .as_ref()
            .map(|h| h.limits())
            .unwrap_or_else(Limits::rtx2080);
        let initial = hw
            .as_ref()
            .map(|h| h.current_values())
            .unwrap_or_else(|| limits.stock());
        let mut book = Book::new();
        book.confirmed = initial.clone();
        book.active = initial;
        Motor {
            book,
            hw,
            backend,
            locked,
            cfg: None,
            cfg_err: None,
            emergency: false,
            lease: None,
            lease_seen: 0,
            last_tick: None,
            hardware_error: None,
            dirty_marker: None,
            events: Vec::new(),
            curve_pct: None,
            vifte: None,
            vifte_ms: None,
            limits,
            wall: rigg_core::now_ms,
        }
    }

    pub fn set_dirty_marker(&mut self, path: std::path::PathBuf) {
        self.dirty_marker = Some(path);
    }

    /// Rot-eid, varig eierskap før første setter, også ved delvis feil/krasj.
    fn mark_dirty(&self) -> Result<(), ProtoError> {
        use std::io::Write;
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let Some(path) = &self.dirty_marker else {
            return Ok(());
        };
        let identity = self
            .hw
            .as_ref()
            .map(|h| h.identity())
            .unwrap_or(Value::Null);
        // Viftene som kan bli endret: recovery må finne og bekrefte like
        // mange på Auto før merkøren kan fjernes.
        let fans = self.hw.as_ref().map_or(0, |h| h.fan_count());
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let data = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            if !meta.is_file()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o077 != 0
                || data.as_ref().is_none_or(|v| v["identity"] != identity)
            {
                return Err(ProtoError::msg(
                    ErrorCode::PermissionDenied,
                    "Gjenopprettingsmerkøren har feil identitet eller utrygg eier/modus.",
                ));
            }
            return Ok(());
        }
        let save = || -> Result<(), std::io::Error> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            let data = json!({"version": 1, "identity": identity, "fans": fans}).to_string();
            f.write_all(data.as_bytes())?;
            f.sync_all()?;
            std::fs::File::open(path.parent().unwrap_or(std::path::Path::new("/")))?.sync_all()
        };
        save().map_err(|e| {
            ProtoError::msg(
                ErrorCode::Unavailable,
                format!("Kan ikke lagre gjenopprettingsmerkør før endring: {e}"),
            )
        })
    }

    fn clear_dirty(&self) -> Result<(), String> {
        let Some(path) = &self.dirty_marker else {
            return Ok(());
        };
        match std::fs::remove_file(path) {
            Ok(()) => std::fs::File::open(path.parent().unwrap_or(std::path::Path::new("/")))
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn steps(&self) -> Vec<Step> {
        if self.cfg.as_ref().is_some_and(|c| c.enabled) {
            vec![Step::Effekt, Step::Vifter, Step::Klokker]
        } else if self.backend == "sim" {
            self.cfg
                .as_ref()
                .map(|c| c.steps.clone())
                .unwrap_or_default()
        } else {
            vec![]
        }
    }

    fn enabled(&self, s: Step) -> bool {
        self.steps().contains(&s)
    }

    fn authorized(&self) -> bool {
        self.cfg
            .as_ref()
            .is_some_and(|c| c.enabled || self.backend == "sim" && c.legacy && !c.steps.is_empty())
    }

    pub fn set_config(&mut self, cfg: Result<Config, String>, now: i64) {
        let before = self.cfg.clone();
        match cfg {
            Ok(c) => {
                self.cfg = Some(c);
                self.cfg_err = None;
            }
            Err(e) => {
                self.cfg = None;
                self.cfg_err = Some(e);
            }
        }
        if before.is_some()
            && before != self.cfg
            && (self.lease.is_some() || self.hardware_error.is_some())
        {
            self.til_standard("autorisasjonen er endret", now);
            self.lease = None;
        }
    }

    fn nod(&self) -> bool {
        self.emergency || self.vifte.as_ref().is_some_and(vifte::Motor::nod)
    }

    fn state_view(&self, now: i64) -> Value {
        let mut s = self.book.state_view(now);
        s["activeValues"] = json!(self.book.active);
        s["confirmedValues"] = json!(self.book.confirmed);
        s["stockValues"] = json!(self.limits.stock());
        s["hardwareKnown"] = json!(self.hardware_error.is_none() && self.hw.is_some());
        s["hardwareError"] = json!(self.hardware_error);
        s["emergency"] = json!(self.nod());
        s
    }

    fn enrich(&self, mut response: Value) -> Value {
        response["activeValues"] = json!(self.book.active);
        response["confirmedValues"] = json!(self.book.confirmed);
        response["stockValues"] = json!(self.limits.stock());
        response["hardwareKnown"] = json!(self.hardware_error.is_none());
        response["emergency"] = json!(self.nod());
        response
    }

    fn identity_matches(&self, args: &Value) -> bool {
        if self.backend == "sim" {
            return true;
        }
        let Some(h) = self.hw.as_ref() else {
            return false;
        };
        let id = h.identity();
        let Some(c) = self.cfg.as_ref() else {
            return false;
        };
        args["gpu"].as_str() == id["pci"].as_str()
            && args["uuid"].as_str() == id["uuid"].as_str()
            && c.pci.as_deref() == id["pci"].as_str()
            && c.uuid.as_deref() == id["uuid"].as_str()
    }

    pub fn status(&mut self, now: i64) -> Value {
        let identity = self
            .hw
            .as_ref()
            .map(|h| h.identity())
            .unwrap_or(Value::Null);
        let caps = self
            .hw
            .as_ref()
            .map(|h| h.capabilities())
            .unwrap_or_else(|| json!({}));
        let bound = self.backend == "sim"
            || self.cfg.as_ref().is_some_and(|c| {
                c.pci.as_deref() == identity["pci"].as_str()
                    && c.uuid.as_deref() == identity["uuid"].as_str()
            });
        let ready = self.authorized()
            && bound
            && self.hw.is_some()
            && self.locked.is_none()
            && self.hardware_error.is_none();
        let l = &self.limits;
        json!({
            "helper": {"name": "agentic-rigg-helper", "version": rigg_core::version::VERSION,
                "build": rigg_core::version::build(), "protocol": hjelper::PROTOCOL, "strictRequests": true,
                "pid": std::process::id(), "backend": self.backend,
                "realWrites": self.backend == "nvml" && self.hw.is_some(), "locked": self.locked,
                "enabled": self.authorized(), "ready": ready, "identity": identity,
                "capabilities": caps},
            "uid": self.cfg.as_ref().map(|c| c.uid), "enabled": self.authorized(), "ready": ready,
            "holding": self.lease.is_some() || self.hardware_error.is_some(),
            "legacyConfig": self.cfg.as_ref().is_some_and(|c| c.legacy),
            "steps": self.steps().iter().map(|s| s.id()).collect::<Vec<_>>(),
            "identity": identity, "capabilities": caps, "configError": self.cfg_err,
            "state": self.state_view(now),
            "limits": {"powerLimitW": [l.power_w.min, l.power_w.max],
                "coreOffsetMhz": [l.core_mhz.min, l.core_mhz.max], "memOffsetMhz": [l.mem_mhz.min, l.mem_mhz.max],
                "tempTargetC": [l.temp_target_c.min, l.temp_target_c.max], "fanPercent": [l.fan_pct.min, l.fan_pct.max]},
            "hardware": self.hw.as_ref().map(|h| h.snapshot()), "emergency": self.nod(),
            "events": std::mem::take(&mut self.events),
        })
    }

    /// Én forespørsel. `now` er monoton; svaret får `pending.deadline` i
    /// veggklokke-ms for klientene.
    pub fn handle(&mut self, conn: u64, op: &str, args: &Value, now: i64) -> OpResult {
        let mut r = self.handle_op(conn, op, args, now);
        if let Ok(v) = r.as_mut() {
            wall_deadlines(v, (self.wall)());
        }
        r
    }

    fn handle_op(&mut self, conn: u64, op: &str, args: &Value, now: i64) -> OpResult {
        self.tick(now);
        if self.lease == Some(conn) {
            self.lease_seen = now;
        }
        if op == "status" {
            return Ok(self.status(now));
        }
        if op == "sim_temp" && self.backend == "sim" {
            let c = args["celsius"]
                .as_f64()
                .filter(|c| (0.0..=120.0).contains(c))
                .ok_or_else(|| {
                    ProtoError::msg(ErrorCode::InvalidArgument, "celsius må være 0–120")
                })?;
            if let Some(h) = self.hw.as_mut() {
                h.set_sim_temperature(c);
            }
            self.tick(now);
            return Ok(json!({"tempC": c, "emergency": self.emergency}));
        }
        if !hjelper::OPS.contains(&op) {
            return Err(ProtoError::new(ErrorCode::UnknownOp));
        }
        if self.hw.is_none() || self.locked.is_some() {
            return Err(hjelper::locked("real_writes_off"));
        }
        if !self.identity_matches(args) {
            return Err(hjelper::locked("identity_mismatch"));
        }
        if !self.authorized() {
            if self.backend == "sim" {
                return Err(hjelper::step_not_enabled(&[Step::Effekt]));
            }
            return Err(hjelper::locked("authorization_required"));
        }
        if self.hardware_error.is_some() && op != "reset" {
            return Err(hjelper::locked("recovery_failed"));
        }
        match op {
            "apply" => self.apply(conn, args, now),
            "confirm" => {
                // Behold endrer bare transaksjonen, aldri maskinvaren igjen.
                let r = self.book.confirm(&owner(conn), args["txn"].as_str(), now)?;
                self.collect_events();
                Ok(self.enrich(r))
            }
            "revert" => {
                self.book.revert(&owner(conn), args["txn"].as_str(), now)?;
                self.push_hw("rull tilbake", now)?;
                self.collect_events();
                Ok(self.state_view(now))
            }
            "reset" => self.reset(conn, now),
            "rgb_get" | "rgb_set" => Err(ProtoError::msg(
                ErrorCode::NotSupported,
                "RGB-backenden er ikke identifisert eller tilgjengelig.",
            )),
            _ => Err(ProtoError::new(ErrorCode::UnknownOp)),
        }
    }

    fn check_capabilities(&self, patch: &Value) -> Result<(), ProtoError> {
        let Some(h) = self.hw.as_ref() else {
            return Err(hjelper::locked("real_writes_off"));
        };
        let caps = h.capabilities();
        let mut required = Vec::new();
        if patch.get("powerLimitW").is_some() {
            required.push("power");
        }
        if patch.get("tempTargetC").is_some() {
            required.push("temp_target");
        }
        if patch["clockOffsets"].get("coreMhz").is_some() {
            required.push("core_clock");
        }
        if patch["clockOffsets"].get("memMhz").is_some() {
            required.push("mem_clock");
        }
        if let Some(fan) = patch.get("fan") {
            let curve =
                fan["mode"] == "curve" || fan.get("curve").is_some() || fan.get("curves").is_some();
            required.push(if curve { "fan_curve" } else { "fan_manual" });
        }
        for key in required {
            if caps[key]["available"] != true {
                return Err(ProtoError::msg(
                    ErrorCode::NotSupported,
                    format!(
                        "Funksjonen {key} er ikke tilgjengelig: {}",
                        caps[key]["reason"]
                            .as_str()
                            .unwrap_or("kortet eller driveren mangler støtte")
                    ),
                )
                .details(json!({"field": key, "reason": "not_supported"})));
            }
        }
        Ok(())
    }

    fn apply(&mut self, conn: u64, args: &Value, now: i64) -> OpResult {
        let patch = args.get("settings").cloned().unwrap_or(Value::Null);
        let need = hjelper::steps_for_patch(&patch)?;
        // Eldre begrensede sim-fixtures har ingen produksjonsautorisasjon.
        if self.backend == "sim" && self.cfg.as_ref().is_some_and(|c| c.legacy) {
            let missing: Vec<Step> = need.into_iter().filter(|s| !self.enabled(*s)).collect();
            if !missing.is_empty() {
                return Err(hjelper::step_not_enabled(&missing));
            }
        }
        self.check_capabilities(&patch)?;
        let fan_mode = patch["fan"]["mode"].as_str();
        if matches!(fan_mode, Some("manual" | "curve"))
            && self.hw.as_mut().and_then(|h| h.temperature()).is_none()
        {
            return Err(ProtoError::msg(
                ErrorCode::Unavailable,
                "Viftestyring krever en lesbar temperatur for overtemperaturvernet.",
            ));
        }
        if self.emergency {
            return Err(ProtoError::msg(
                ErrorCode::Busy,
                "Nødregelen gjelder. Vent til kortet er under 80 °C.",
            ));
        }
        check_ranges(&patch, &self.limits)?;
        let l = &self.limits;
        let limits = txn::HardwareLimits {
            power: l.power_w,
            core: l.core_mhz,
            memory: l.mem_mhz,
            temperature: l.temp_target_c,
            fan: l.fan_pct,
        };
        let changing = self.book.validate_apply_with_limits(
            &owner(conn),
            &patch,
            args.get("confirmTimeoutS"),
            &limits,
        )?;
        if changing {
            self.mark_dirty()?;
        }
        let before = self.book.active.clone();
        let r = self.book.apply_with_limits(
            &owner(conn),
            &patch,
            args.get("confirmTimeoutS"),
            now,
            Some(&limits),
        )?;
        if self.book.pending().is_some() {
            self.lease = Some(conn);
            self.lease_seen = now;
        }
        if r["txn"].is_null() {
            return Ok(self.enrich(r));
        }
        self.curve_pct = None;
        journal::notice(&format!(
            "Endring: {}. Venter på Behold.",
            describe(&before, &self.book.active)
        ));
        self.push_hw("apply", now)?;
        Ok(self.enrich(r))
    }

    fn reset(&mut self, conn: u64, now: i64) -> OpResult {
        if self.book.pending().is_some_and(|p| p.owner != owner(conn)) {
            return Err(ProtoError::new(ErrorCode::Busy));
        }
        self.mark_dirty()?;
        self.restore_stock("tilbakestill", now)?;
        Ok(self.state_view(now))
    }

    pub fn til_standard(&mut self, why: &str, now: i64) {
        if let Err(e) = self.restore_stock(why, now) {
            journal::err(&e.message);
        }
    }

    fn restore_stock(&mut self, why: &str, _now: i64) -> Result<(), ProtoError> {
        self.restore_stock_with(why, true)
    }

    /// Som `restore_stock`; `clear_marker == false` lar merkøren på disk stå
    /// urørt (den kunne ikke bekreftes og skal kontrolleres).
    fn restore_stock_with(&mut self, why: &str, clear_marker: bool) -> Result<(), ProtoError> {
        self.curve_pct = None;
        self.vifte = None;
        self.vifte_ms = None;
        let Some(h) = self.hw.as_mut() else {
            return Err(hjelper::locked("real_writes_off"));
        };
        if let Err(e) = h.reset() {
            self.hardware_error = Some(e.clone());
            self.book.rollback("reset_failed");
            self.collect_events();
            return Err(hjelper::hardware_error(format!(
                "readback_mismatch: Gjenoppretting kunne ikke bekreftes ({why}): {e}"
            )));
        }
        if h.snapshot()["stock"] != true {
            let e = "Kortets standardtilstand kunne ikke bekreftes ved tilbakelesing.".to_string();
            self.hardware_error = Some(e.clone());
            self.book.rollback("reset_unverified");
            self.collect_events();
            return Err(hjelper::hardware_error(format!("readback_mismatch: {e}")));
        }
        if clear_marker && let Err(e) = self.clear_dirty() {
            self.hardware_error = Some(e.clone());
            return Err(ProtoError::msg(
                ErrorCode::Unavailable,
                format!("Kortet ble gjenopprettet, men merkøren kunne ikke ryddes: {e}"),
            ));
        }
        self.lease = None;
        self.book.rollback("reset");
        self.book.confirmed = self.limits.stock();
        self.book.active = self.limits.stock();
        self.hardware_error = None;
        self.emergency = false;
        self.collect_events();
        journal::notice(&format!("Gjenopprettet kortets grunnverdier ({why})."));
        Ok(())
    }

    fn collect_events(&mut self) {
        for e in self.book.take_events() {
            let mut event = e.to_json();
            if self.hardware_error.is_some() {
                event["status"] = json!("failed");
                event["reason"] = json!("recovery_failed");
            }
            self.events.push(event);
        }
        if self.events.len() > 64 {
            self.events.drain(..self.events.len() - 64);
        }
    }

    /// Merkøren kunne ikke lagres eller bekreftes før en skriving kortet
    /// venter på (nødregel, frist, tilbakerulling). Feilen kan ikke bare
    /// returneres: takten forkaster den, og nødregelen ville stått som aktiv
    /// med lav vifte. Kortet går til grunnverdiene med Auto-vifter, merkøren
    /// på disk står urørt, og tilstanden er en feil til den er avklart.
    fn marker_failed(&mut self, why: &str, mut e: ProtoError) -> ProtoError {
        self.emergency = false;
        self.book.rollback("hardware_error");
        self.curve_pct = None;
        let restored = self.restore_stock_with(why, false).is_ok();
        let msg = if restored {
            format!(
                "{why}: {} Kortet er satt tilbake til grunnverdiene med Auto-vifter, \
                 men gjenopprettingsmerkøren må kontrolleres.",
                e.message
            )
        } else {
            format!("{why}: {} Kortets tilstand er ukjent.", e.message)
        };
        journal::err(&msg);
        self.hardware_error = Some(msg);
        self.collect_events();
        e.details = Some(json!({"reason": "recovery_marker", "recovered": false,
            "stockRestored": restored, "recoveryError": self.hardware_error}));
        e
    }

    fn push_hw(&mut self, why: &str, now: i64) -> Result<(), ProtoError> {
        if let Err(e) = self.mark_dirty() {
            return Err(self.marker_failed(why, e));
        }
        let v = self.effective();
        let result = self
            .hw
            .as_mut()
            .ok_or_else(|| hjelper::locked("real_writes_off"))?
            .apply(&v)
            .map_err(hjelper::hardware_error)
            .and_then(|_| self.drive_fans(now));
        if let Err(original) = result {
            self.book.rollback("hardware_error");
            self.curve_pct = None;
            let confirmed = self.book.confirmed.clone();
            // Under nødregelen er bekreftet oppsett (for eksempel lav manuell
            // vifte) ingen trygg gjenoppretting: da skal driveren styre viftene
            // igjen (bekreftet Auto), ellers er kortet i feiltilstand.
            let emergency = self.emergency;
            let recovered = !emergency
                && confirmed.fan_mode != FanMode::Curve
                && self
                    .hw
                    .as_mut()
                    .is_some_and(|h| h.restore_values(&confirmed).is_ok());
            if !recovered {
                // Nødregelen er ikke i kraft når 100 % ikke ble skrevet.
                self.emergency = false;
                self.hardware_error = Some(format!(
                    "{why}: {}. {}",
                    original.message,
                    if emergency {
                        "Full viftefart kunne ikke skrives."
                    } else {
                        "Gjenoppretting til bekreftet oppsett feilet."
                    }
                ));
                // Forsøk alle fabrikk-/grunnverdier selv om bekreftet oppsett feilet.
                if self
                    .restore_stock("feil etter delvis endring", now)
                    .is_err()
                {
                    self.hardware_error = Some(format!(
                        "{why}: {}. Kortets tilstand er ukjent.",
                        original.message
                    ));
                } else if emergency {
                    journal::err(&format!(
                        "Nødregelen kunne ikke gi full viftefart ({}). Kortet er satt \
                         tilbake til grunnverdiene, og driveren styrer viftene (Auto).",
                        original.message
                    ));
                }
            }
            self.collect_events();
            let mut e = original;
            e.details = Some(
                json!({"reason": "hardware_error", "recovered": self.hardware_error.is_none(), "hardwareError": e.message, "recoveryError": self.hardware_error}),
            );
            return Err(e);
        }
        Ok(())
    }

    fn fan_control_available(&self) -> bool {
        self.enabled(Step::Vifter)
            && self
                .hw
                .as_ref()
                .is_some_and(|h| h.capabilities()["fan_manual"]["available"] == true)
    }

    fn effective(&self) -> Values {
        let mut v = self.book.active.clone();
        if self.emergency && self.fan_control_available() {
            v.fan_mode = FanMode::Manual;
            v.fan_percent = self.limits.fan_pct.max;
        }
        v
    }

    fn drive_fans(&mut self, now: i64) -> Result<(), ProtoError> {
        if self.book.active.fan_mode != FanMode::Curve || !self.enabled(Step::Vifter) {
            self.vifte = None;
            self.vifte_ms = None;
            return Ok(());
        }
        let Some(h) = self.hw.as_mut() else {
            return Ok(());
        };
        let o =
            vifte::Oppsett::fra_values(&self.book.active, h.fan_count(), self.limits.fan_pct.min);
        match self.vifte.as_mut() {
            Some(m) if m.oppsett() != &o => m.sett_oppsett(o),
            Some(_) => {}
            None => self.vifte = Some(vifte::Motor::new(o).med_start(&h.fan_percents())),
        }
        if self.vifte_ms == Some(now) {
            return Ok(());
        }
        self.vifte_ms = Some(now);
        let t = h.temperature();
        let Some(m) = self.vifte.as_mut() else {
            return Ok(());
        };
        let step = m.steg(t, now);
        if self.emergency {
            return Ok(());
        }
        let pct: Vec<f64> = step
            .heltall(self.limits.fan_pct.min)
            .into_iter()
            .map(f64::from)
            .collect();
        if self.curve_pct.as_ref() != Some(&pct) {
            h.set_fan_percents(&pct).map_err(hjelper::hardware_error)?;
            self.curve_pct = Some(pct);
        }
        Ok(())
    }

    pub fn tick(&mut self, now: i64) {
        if self.hw.is_none() || !self.authorized() || self.hardware_error.is_some() {
            return;
        }
        // Tidslinjen er monoton og teller hvile; deltaene klemmes likevel,
        // så et avvik aldri gir negative tider.
        if self.lease.is_some()
            && (now.saturating_sub(self.lease_seen) > 65_000
                || self
                    .last_tick
                    .is_some_and(|last| now.saturating_sub(last) > 15_000))
        {
            self.lease = None;
            self.til_standard("livssignal mangler eller maskinen har vært i hvile", now);
            if self.hardware_error.is_some() {
                return;
            }
        }
        self.last_tick = Some(now);
        if self.book.tick(now) {
            let _ = self.push_hw("frist", now);
            self.collect_events();
            if self.hardware_error.is_some() {
                return;
            }
        }
        let temp = self.hw.as_mut().and_then(|h| h.temperature());
        // Ikke grip inn på en uberørt maskin bare fordi hjelperen er startet.
        if self.lease.is_some() && self.fan_control_available() {
            if temp.is_none() && self.book.active.fan_mode == FanMode::Manual {
                self.til_standard(
                    "temperaturen kan ikke leses under manuell viftestyring",
                    now,
                );
                return;
            }
            if let Some(t) = temp {
                if !self.emergency && t >= txn::EMERGENCY_FAN_C {
                    self.emergency = true;
                    self.book.rollback("emergency");
                    self.curve_pct = None;
                    let _ = self.push_hw("nødregel", now);
                    self.collect_events();
                    if self.hardware_error.is_some() {
                        return;
                    }
                } else if self.emergency && t < txn::EMERGENCY_RELEASE_C {
                    self.emergency = false;
                    self.curve_pct = None;
                    let _ = self.push_hw("nødregel over", now);
                    if self.hardware_error.is_some() {
                        return;
                    }
                }
            }
        }
        if let Err(e) = self.drive_fans(now) {
            journal::err(&format!("Viftekurven feilet: {}", e.message));
            self.hardware_error = Some(e.message);
            self.til_standard("viftekurven kunne ikke skrives", now);
        }
    }

    pub fn disconnect(&mut self, conn: u64, now: i64) {
        self.book.disconnect(&owner(conn));
        if self.lease == Some(conn) {
            self.lease = None;
            self.til_standard("tjenesten koblet fra", now);
        }
        self.collect_events();
    }

    pub fn shutdown(&mut self, now: i64) {
        self.book.rollback("daemon_stopping");
        if self.lease.is_some() || self.hardware_error.is_some() {
            self.til_standard("hjelperen stopper", now);
        }
    }
}

/// «effekt 245 → 200 W, vifte auto → manual» til journalen.
fn describe(a: &Values, b: &Values) -> String {
    let mut out = Vec::new();
    let mut num = |n: &str, x: f64, y: f64, unit: &str| {
        if x != y {
            out.push(format!("{n} {x} → {y} {unit}"));
        }
    };
    num("effektgrense", a.power_limit_w, b.power_limit_w, "W");
    num("kjerne-offset", a.core_offset_mhz, b.core_offset_mhz, "MHz");
    num("minne-offset", a.mem_offset_mhz, b.mem_offset_mhz, "MHz");
    num("temperaturmål", a.temp_target_c, b.temp_target_c, "°C");
    num("fast viftefart", a.fan_percent, b.fan_percent, "%");
    if a.fan_mode != b.fan_mode {
        out.push(format!(
            "viftemodus {} → {}",
            a.fan_mode.as_str(),
            b.fan_mode.as_str()
        ));
    }
    if a.fan_curve != b.fan_curve {
        out.push(format!("viftekurve ({} punkter)", b.fan_curve.len()));
    }
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_hw::gpu::skriv::SimSkriver;

    /// Feilinjeksjon uten NVML, drivere, root eller korttilgang.
    struct FaultWriter {
        sim: SimSkriver,
        flags: std::rc::Rc<std::cell::Cell<u8>>,
        missing_temp: bool,
        missing_memory: bool,
        wide_power: bool,
    }

    impl Skriver for FaultWriter {
        fn backend(&self) -> &'static str {
            "nvml"
        }
        fn limits(&self) -> Limits {
            let mut l = self.sim.limits();
            if self.wide_power {
                l.power_w = Range::new(400.0, 800.0);
            }
            l
        }
        fn current_values(&self) -> Values {
            self.sim.applied.clone()
        }
        fn identity(&self) -> Value {
            json!({"pci":"0000:01:00.0", "uuid":"GPU-fake-card", "driver":"fake", "vbios":"fake"})
        }
        fn capabilities(&self) -> Value {
            let mut c = self.sim.capabilities();
            if self.missing_temp {
                c["temp_target"]["available"] = json!(false);
            }
            if self.missing_memory {
                c["mem_clock"]["available"] = json!(false);
            }
            if self.flags.get() & 64 != 0 {
                c["fan_manual"]["available"] = json!(false);
                c["fan_curve"]["available"] = json!(false);
            }
            c
        }
        fn temperature(&mut self) -> Option<f64> {
            if self.flags.get() & 32 != 0 {
                None
            } else {
                Some(if self.flags.get() & 128 != 0 {
                    86.0
                } else {
                    45.0
                })
            }
        }
        fn apply(&mut self, v: &Values) -> Result<(), String> {
            if self.flags.get() & 1 != 0 {
                // Første delkall kan ha lyktes før resten av forespørselen feiler.
                self.sim.applied.power_limit_w = v.power_limit_w;
                return Err("no_permission: feil etter effektendring".into());
            }
            self.sim.apply(v)
        }
        fn restore_values(&mut self, v: &Values) -> Result<(), String> {
            if self.flags.get() & 2 != 0 {
                return Err("readback_mismatch: rollback feilet".into());
            }
            self.sim.apply(v)
        }
        fn set_fan_percent(&mut self, p: f64) -> Result<(), String> {
            if self.flags.get() & 8 != 0 {
                return Err("gpu_lost: vifta svarte ikke".into());
            }
            self.sim.set_fan_percent(p)
        }
        fn reset(&mut self) -> Result<(), String> {
            if self.flags.get() & 4 != 0 {
                return Err("readback_mismatch: Auto kunne ikke bekreftes".into());
            }
            self.sim.reset()
        }
        fn snapshot(&self) -> Value {
            let mut v = self.sim.snapshot();
            if self.flags.get() & 16 != 0 {
                v["stock"] = Value::Null;
            }
            v
        }
    }

    fn bound_motor(
        missing_temp: bool,
        missing_memory: bool,
        wide_power: bool,
    ) -> (Motor, std::rc::Rc<std::cell::Cell<u8>>) {
        let flags = std::rc::Rc::new(std::cell::Cell::new(0));
        let writer = FaultWriter {
            sim: SimSkriver::new(),
            flags: flags.clone(),
            missing_temp,
            missing_memory,
            wide_power,
        };
        let mut m = Motor::new(Some(Box::new(writer)), "nvml", None);
        m.set_config(Config::parse(r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"0000:01:00.0","uuid":"GPU-fake-card"}"#), T0);
        (m, flags)
    }

    #[test]
    fn uverifisert_reset_beholder_merkoren_og_sperrer_ready() {
        use std::os::unix::fs::PermissionsExt;
        let (mut m, flags) = bound_motor(false, false, false);
        let dir = std::env::temp_dir().join(format!("rigg-dirty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("dirty.json");
        m.set_dirty_marker(marker.clone());
        let noop = m
            .handle(1, "apply", &bound_args(json!({"powerLimitW":245})), T0)
            .unwrap();
        assert!(noop["txn"].is_null());
        assert!(
            !marker.exists(),
            "Uendret oppsett oppretter ingen eierskapsmarkør"
        );
        m.handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
            .unwrap();
        assert!(marker.exists());
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&marker).unwrap()).unwrap();
        assert_eq!(saved["fans"], 2, "recovery må vite hvor mange vifter som kan være endret");
        assert_eq!(
            std::fs::metadata(&marker).unwrap().permissions().mode() & 0o777,
            0o600
        );
        flags.set(16);
        assert!(m.handle(1, "reset", &bound_args(json!({})), T0).is_err());
        assert!(marker.exists());
        assert_eq!(m.status(T0)["ready"], false);
        flags.set(0);
        m.handle(1, "reset", &bound_args(json!({})), T0).unwrap();
        assert!(!marker.exists());
        std::fs::remove_dir(dir).unwrap();
    }

    fn bound_args(settings: Value) -> Value {
        json!({"gpu":"0000:01:00.0", "uuid":"GPU-fake-card", "settings":settings})
    }

    #[test]
    fn global_autorisasjon_gir_kun_faktiske_funksjoner() {
        let (mut m, _) = bound_motor(true, true, false);
        assert!(m.status(T0)["ready"].as_bool().unwrap());
        let r = m
            .handle(
                1,
                "apply",
                &bound_args(json!({"powerLimitW":200,"clockOffsets":{"coreMhz":50}})),
                T0,
            )
            .unwrap();
        assert_eq!(r["activeValues"]["powerLimitW"], 200.0);
        assert_eq!(r["activeValues"]["coreOffsetMhz"], 50.0);
        assert_eq!(
            m.handle(1, "apply", &bound_args(json!({"tempTargetC":80})), T0)
                .unwrap_err()
                .code,
            ErrorCode::NotSupported
        );
        assert_eq!(
            m.handle(
                1,
                "apply",
                &bound_args(json!({"clockOffsets":{"memMhz":50}})),
                T0
            )
            .unwrap_err()
            .code,
            ErrorCode::NotSupported
        );
        assert_eq!(
            m.handle(1, "rgb_set", &bound_args(json!({})), T0)
                .unwrap_err()
                .code,
            ErrorCode::NotSupported
        );
    }

    #[test]
    fn maskinvareomraadet_klemmes_ikke_til_rtx2080() {
        let (mut m, _) = bound_motor(true, false, true);
        let r = m
            .handle(1, "apply", &bound_args(json!({"powerLimitW":650})), T0)
            .unwrap();
        assert_eq!(r["activeValues"]["powerLimitW"], 650.0);
        assert_eq!(m.status(T0)["hardware"]["applied"]["powerLimitW"], 650.0);
    }

    #[test]
    fn feil_identitet_eller_legacy_tillatelse_skriver_ikke() {
        let (mut m, _) = bound_motor(false, false, false);
        let mut args = bound_args(json!({"powerLimitW":200}));
        args["gpu"] = json!("0000:02:00.0");
        assert_eq!(
            m.handle(1, "apply", &args, T0)
                .unwrap_err()
                .details
                .unwrap()["reason"],
            "identity_mismatch"
        );
        assert!(m.book.pending().is_none());
        m.set_config(
            Config::parse(
                r#"{"versjon":1,"uid":1000,"trinn":["effekt","vifter","klokker","lys"]}"#,
            ),
            T0,
        );
        assert_eq!(m.status(T0)["enabled"], false);
        assert_eq!(m.status(T0)["ready"], false);
        assert!(
            m.handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
                .is_err()
        );
    }

    #[test]
    fn delvis_feil_gjenopprettes_og_rapporteres_som_feil() {
        let (mut m, flags) = bound_motor(false, false, false);
        flags.set(1);
        let error = m
            .handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.details.unwrap()["recovered"], true);
        assert_eq!(m.status(T0)["hardware"]["applied"]["powerLimitW"], 245.0);
        assert!(m.book.pending().is_none());
    }

    #[test]
    fn mislykket_gjenoppretting_sperrer_og_later_ikke_som_standard() {
        let (mut m, flags) = bound_motor(false, false, false);
        flags.set(1 | 2 | 4);
        let error = m
            .handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
            .unwrap_err();
        assert_eq!(error.details.unwrap()["recovered"], false);
        let status = m.status(T0);
        assert_eq!(status["ready"], false);
        assert_eq!(status["state"]["hardwareKnown"], false);
        assert!(
            status["events"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["status"] != "rolled_back")
        );
        assert!(
            m.handle(1, "apply", &bound_args(json!({"powerLimitW":220})), T0)
                .is_err()
        );
        assert!(m.handle(1, "reset", &bound_args(json!({})), T0).is_err());
        flags.set(0);
        m.handle(1, "reset", &bound_args(json!({})), T0).unwrap();
        assert_eq!(m.status(T0)["ready"], true);
    }

    #[test]
    fn kurveskrivefeil_ender_i_auto_uten_falsk_fartscache() {
        let (mut m, flags) = bound_motor(false, false, false);
        flags.set(8);
        assert!(
            m.handle(1, "apply", &bound_args(json!({"fan":{"mode":"curve"}})), T0)
                .is_err()
        );
        assert!(m.curve_pct.is_none());
        assert_eq!(m.status(T0)["hardware"]["stock"], true);
    }

    #[test]
    fn bekreftet_styring_holdes_og_gjenopprettes_ved_manglende_livssignal() {
        let (mut m, _) = bound_motor(false, false, false);
        let r = m
            .handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
            .unwrap();
        let mut args = bound_args(json!({}));
        args["txn"] = r["txn"].clone();
        m.handle(1, "confirm", &args, T0).unwrap();
        assert_eq!(m.status(T0)["holding"], true);
        m.tick(T0 + 66_000);
        assert_eq!(m.status(T0 + 66_000)["hardware"]["stock"], true);
    }

    #[test]
    fn frist_og_nodregel_rapporterer_ikke_falsk_vellykket_rollback() {
        for emergency in [false, true] {
            let (mut m, flags) = bound_motor(false, false, false);
            m.handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
                .unwrap();
            flags.set(1 | 2 | 4 | if emergency { 128 } else { 0 });
            let now = if emergency { T0 + 1 } else { T0 + 15_000 };
            m.tick(now);
            let s = m.status(now);
            assert_eq!(s["state"]["hardwareKnown"], false);
            assert_eq!(s["ready"], false);
            let events = s["events"].as_array().unwrap();
            assert!(!events.is_empty());
            assert!(events.iter().all(|e| e["status"] == "failed"));
        }
    }

    #[test]
    fn nodregel_med_skrivefeil_blir_ikke_staaende_paa_lav_manuell_vifte() {
        for reset_fails in [false, true] {
            let (mut m, flags) = bound_motor(false, false, false);
            let r = m
                .handle(
                    1,
                    "apply",
                    &bound_args(json!({"fan":{"mode":"manual","manualPercent":35}})),
                    T0,
                )
                .unwrap();
            let mut args = bound_args(json!({}));
            args["txn"] = r["txn"].clone();
            m.handle(1, "confirm", &args, T0).unwrap();
            assert_eq!(m.book.confirmed.fan_mode, FanMode::Manual);
            assert_eq!(m.status(T0)["hardware"]["fanNow"], 35.0);
            // 86 °C: 100 % feiler, mens bekreftet 35 % ville latt seg skrive.
            flags.set(1 | 128 | if reset_fails { 4 } else { 0 });
            m.tick(T0 + 1000);
            let s = m.status(T0 + 1000);
            assert!(
                !(s["hardware"]["fanNow"] == 35.0 && m.hardware_error.is_none()),
                "stille lav manuell vifte under nødregelen: {s}"
            );
            assert!(!m.emergency, "nødregelen meldes ikke som aktiv uten skriving");
            assert_eq!(s["emergency"], false);
            if reset_fails {
                assert!(m.hardware_error.is_some());
                assert_eq!(s["ready"], false);
                assert_eq!(s["state"]["hardwareKnown"], false);
            } else {
                assert!(m.hardware_error.is_none(), "{s}");
                assert_eq!(s["hardware"]["stock"], true, "{s}");
                assert!(s["hardware"]["fanNow"].is_null(), "driveren styrer (Auto)");
                assert_eq!(m.book.active.fan_mode, FanMode::Auto);
                assert!(!m.emergency, "nødregelen meldes ikke som aktiv uten skriving");
                assert_eq!(s["emergency"], false);
            }
        }
    }

    #[test]
    fn merkorfeil_under_nodregel_og_frist_gir_auto_og_feiltilstand() {
        for emergency in [true, false] {
            let (mut m, flags) = bound_motor(false, false, false);
            let dir = std::env::temp_dir().join(format!(
                "rigg-merkorfeil-{}-{emergency}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let marker = dir.join("dirty.json");
            m.set_dirty_marker(marker.clone());
            let r = m
                .handle(
                    1,
                    "apply",
                    &bound_args(json!({"fan":{"mode":"manual","manualPercent":35}})),
                    T0,
                )
                .unwrap();
            if emergency {
                let mut args = bound_args(json!({}));
                args["txn"] = r["txn"].clone();
                m.handle(1, "confirm", &args, T0).unwrap();
            }
            assert_eq!(m.status(T0)["hardware"]["fanNow"], 35.0);
            // Merkøren tilhører nå et annet kort: mark_dirty feiler.
            std::fs::write(&marker, r#"{"version":1,"identity":"annet"}"#).unwrap();
            let corrupt = std::fs::read(&marker).unwrap();
            let now = if emergency {
                flags.set(128);
                T0 + 1000
            } else {
                T0 + 15_000
            };
            m.tick(now);
            let s = m.status(now);
            assert!(!m.emergency, "{s}");
            assert_eq!(s["emergency"], false);
            assert!(m.hardware_error.is_some(), "{s}");
            assert_eq!(s["ready"], false);
            assert_eq!(s["hardware"]["stock"], true, "Auto er forsøkt og bekreftet: {s}");
            assert!(s["hardware"]["fanNow"].is_null());
            assert_eq!(std::fs::read(&marker).unwrap(), corrupt, "merkøren står urørt");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn frist_haandheves_monotont_men_meldes_i_veggklokke() {
        const WALL: i64 = 1_763_800_000_000;
        let mut m = motor(r#""effekt""#);
        m.wall = || WALL;
        let r = m
            .handle(
                1,
                "apply",
                &json!({"settings": {"powerLimitW": 200}, "confirmTimeoutS": 15}),
                T0,
            )
            .unwrap();
        assert_eq!(r["pending"]["remainingMs"], 15_000);
        assert_eq!(r["pending"]["deadline"], WALL + 15_000, "{r}");
        let s = m.handle(1, "status", &json!({}), T0 + 5_000).unwrap();
        assert_eq!(s["state"]["pending"]["remainingMs"], 10_000);
        assert_eq!(s["state"]["pending"]["deadline"], WALL + 10_000);
        // Håndhevingen bruker bare den monotone tiden.
        assert_eq!(m.book.pending().unwrap().deadline, T0 + 15_000);
        m.tick(T0 + 14_999);
        assert!(m.book.pending().is_some());
        m.tick(T0 + 15_000);
        assert!(m.book.pending().is_none());
    }

    #[test]
    fn hvile_paa_den_monotone_tidslinjen_tilbakestiller_men_avvik_bakover_ikke() {
        for gap in [-50_000, 16_000] {
            let (mut m, _) = bound_motor(false, false, false);
            let r = m
                .handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
                .unwrap();
            let mut args = bound_args(json!({}));
            args["txn"] = r["txn"].clone();
            m.handle(1, "confirm", &args, T0).unwrap();
            m.tick(T0 + 1000);
            m.tick(T0 + 1000 + gap);
            let s = m.status(T0);
            if gap < 0 {
                // Et avvik bakover gir ingen negative deltaer og ingen reset.
                assert_eq!(s["holding"], true);
                assert_eq!(s["hardware"]["applied"]["powerLimitW"], 200.0);
            } else {
                // CLOCK_BOOTTIME teller hvile: 16 s uten runde er hvile.
                assert_eq!(s["hardware"]["stock"], true);
                assert_eq!(s["holding"], false);
            }
        }
    }

    #[test]
    fn temperaturbortfall_under_manuell_vifte_gjenoppretter_auto() {
        let (mut m, flags) = bound_motor(false, false, false);
        m.handle(
            1,
            "apply",
            &bound_args(json!({"fan":{"mode":"manual","manualPercent":40}})),
            T0,
        )
        .unwrap();
        flags.set(32);
        m.tick(T0 + 1);
        assert_eq!(m.status(T0 + 1)["hardware"]["stock"], true);
        assert_eq!(m.book.active.fan_mode, FanMode::Auto);
    }

    #[test]
    fn nodregel_skriver_aldri_ustottet_vifte_ved_effektstyring() {
        let (mut m, flags) = bound_motor(false, false, false);
        flags.set(64 | 128);
        m.handle(1, "apply", &bound_args(json!({"powerLimitW":200})), T0)
            .unwrap();
        m.tick(T0 + 1);
        let s = m.status(T0 + 1);
        assert_eq!(s["hardware"]["applied"]["powerLimitW"], 200.0);
        assert!(s["hardware"]["fanNow"].is_null());
        assert_eq!(s["ready"], true);
        assert!(!m.emergency);
    }

    const T0: i64 = 1_000_000;

    fn motor(steps: &str) -> Motor {
        let mut m = Motor::new(Some(Box::new(SimSkriver::new())), "sim", None);
        m.set_config(
            Config::parse(&format!(r#"{{"versjon":1,"uid":1000,"trinn":[{steps}]}}"#)),
            T0,
        );
        m
    }

    fn hw(m: &mut Motor) -> Value {
        m.status(T0)["hardware"].clone()
    }

    #[test]
    fn uten_trinn_avvises_alt() {
        let mut m = motor("");
        for p in [
            json!({"powerLimitW": 200}),
            json!({"fan": {"mode": "manual"}}),
            json!({"clockOffsets": {"coreMhz": 50}}),
        ] {
            let e = m
                .handle(1, "apply", &json!({"settings": p}), T0)
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::StepNotEnabled);
        }
        assert_eq!(
            m.handle(1, "rgb_set", &json!({}), T0).unwrap_err().code,
            ErrorCode::StepNotEnabled
        );
        assert!(hw(&mut m)["stock"].as_bool().unwrap());
    }

    #[test]
    fn godkjent_trinn_gir_txn_og_behold() {
        let mut m = motor(r#""effekt""#);
        let r = m
            .handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0)
            .unwrap();
        assert_eq!(hw(&mut m)["applied"]["powerLimitW"], 200.0);
        // Et annet trinn i samme forespørsel avvises helt.
        assert_eq!(
            m.handle(
                1,
                "apply",
                &json!({"settings": {"powerLimitW": 210, "fan": {"mode": "manual"}}}),
                T0
            )
            .unwrap_err()
            .code,
            ErrorCode::StepNotEnabled
        );
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0 + 1000)
            .unwrap();
        assert_eq!(m.book.confirmed.power_limit_w, 200.0);
    }

    #[test]
    fn utenfor_kortets_omrade_avvises() {
        let mut m = motor(r#""effekt","vifter","klokker""#);
        for p in [
            json!({"powerLimitW": 300}),
            json!({"powerLimitW": 50}),
            json!({"tempTargetC": 95}),
            json!({"clockOffsets": {"coreMhz": 1500}}),
            json!({"fan": {"manualPercent": 20}}),
            json!({"fan": {"curve": [[30, 10], [60, 50]]}}),
            json!({"fan": {"curves": [[[30, 40], [60, 50]], [[30, 20], [60, 50]]]}}),
        ] {
            let e = m
                .handle(1, "apply", &json!({"settings": p}), T0)
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::InvalidArgument, "{p}");
        }
        assert!(m.book.pending().is_none());
        assert!(hw(&mut m)["stock"].as_bool().unwrap());
    }

    #[test]
    fn fristen_og_frakobling_ruller_tilbake() {
        let mut m = motor(r#""vifter""#);
        m.handle(1, "apply", &json!({"settings": {"fan": {"mode": "manual", "manualPercent": 70}}, "confirmTimeoutS": 10}), T0).unwrap();
        assert_eq!(hw(&mut m)["fanNow"], 70.0);
        m.tick(T0 + 10_000);
        assert!(hw(&mut m)["fanNow"].is_null());
        let r = m
            .handle(
                1,
                "apply",
                &json!({"settings": {"fan": {"mode": "manual", "manualPercent": 60}}}),
                T0,
            )
            .unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0)
            .unwrap();
        m.disconnect(2, T0);
        assert_eq!(
            hw(&mut m)["fanNow"],
            60.0,
            "en annen forbindelse betyr ingenting"
        );
        m.disconnect(1, T0);
        assert!(
            hw(&mut m)["stock"].as_bool().unwrap(),
            "riggd borte: standard"
        );
    }

    #[test]
    fn nodregelen_gir_full_vifte_og_slipper_under_80() {
        let mut m = motor(r#""vifter","effekt""#);
        m.handle(
            1,
            "apply",
            &json!({"settings": {"powerLimitW": 260, "fan": {"mode": "curve"}}}),
            T0,
        )
        .unwrap();
        m.handle(1, "sim_temp", &json!({"celsius": 86}), T0)
            .unwrap();
        assert!(m.emergency);
        assert_eq!(hw(&mut m)["fanNow"], 100.0);
        assert_eq!(
            m.book.active.power_limit_w, 245.0,
            "ventende endring rullet tilbake"
        );
        assert_eq!(
            m.handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0)
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );
        m.handle(1, "sim_temp", &json!({"celsius": 82}), T0)
            .unwrap();
        assert!(m.emergency, "hysterese: 85 inn, 80 ut");
        m.handle(1, "sim_temp", &json!({"celsius": 79}), T0)
            .unwrap();
        assert!(!m.emergency);
        assert!(hw(&mut m)["fanNow"].is_null(), "tilbake til Auto");
    }

    #[test]
    fn kurven_folger_temperaturen_med_viftemotoren() {
        let mut m = motor(r#""vifter""#);
        let r = m
            .handle(
                1,
                "apply",
                &json!({"settings": {"fan": {"mode": "curve", "curve": [[40, 40], [70, 80]]}}}),
                T0,
            )
            .unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0)
            .unwrap();
        m.handle(1, "sim_temp", &json!({"celsius": 55}), T0)
            .unwrap();
        // Rampen (10 %/s opp) når kurven etter noen sekunder.
        for s in 1..=20 {
            m.tick(T0 + s * 1000);
        }
        let st = m.status(T0);
        assert_eq!(st["hardware"]["fanNow"], 60.0, "{st}");
    }

    /// Kurve-modus i hjelperen (sim) gir samme fart som tjenestens
    /// sim-sløyfe (`rigg_hw::gpu::sim_vifter`, 7B) når temperaturene er de
    /// samme: kortet i ro, spill-last til nødregelen slår inn, og i ro igjen.
    /// Med egen kurve per vifte, hysterese, rampe, utjevning og stille flate.
    #[test]
    fn kurven_er_lik_tjenestens_sim_sloyfe() {
        use rigg_core::model::GpuSample;
        use rigg_hw::gpu::sim_vifter::SimVifter;
        use std::collections::BTreeMap;
        // Lave kurver, så kortet blir varmt nok til nødregelen under last.
        let fan = json!({"mode": "curve", "perFan": true, "curve": [[40, 35], [90, 35], [100, 70]],
            "curves": [[[40, 35], [90, 35], [100, 70]], [[40, 37], [90, 37], [100, 80]]],
            "hysteresisC": 4, "rampUpPctS": 8, "rampDownPctS": 2, "smoothingS": 3, "quietUntilC": 50});
        let effekt = |s: i64| if (60..300).contains(&s) { 245.0 } else { 20.0 };

        // Tjenesten: varmemodellen og viftesløyfen, ett steg i sekundet.
        let (a, _) = txn::merge(&Values::default(), &json!({"fan": fan})).unwrap();
        let mut tjenesten = SimVifter::default();
        let spor: Vec<(f64, Vec<f64>)> = (0..480)
            .map(|s| {
                let mut g = BTreeMap::from([(
                    "k".to_string(),
                    GpuSample {
                        temp: Some(40.0),
                        power: Some(effekt(s)),
                        fan: Some(35.0),
                        fans: vec![Some(35.0); 2],
                        ..Default::default()
                    },
                )]);
                tjenesten.runde(&mut g, &a, 35.0, T0 + s * 1000);
                (
                    g["k"].temp.unwrap(),
                    g["k"].fans.iter().map(|f| f.unwrap()).collect(),
                )
            })
            .collect();

        // Hjelperen: kurven tas imot fra Auto ved samme temperatur og tid, og
        // får så de samme temperaturene, én runde i sekundet.
        let mut m = motor(r#""vifter""#);
        m.handle(1, "sim_temp", &json!({"celsius": spor[0].0}), T0 - 1000)
            .unwrap();
        let r = m
            .handle(1, "apply", &json!({"settings": {"fan": fan}}), T0)
            .unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0)
            .unwrap();
        assert_eq!(m.book.active, a);
        let mut nod = 0;
        for (s, (t, ventet)) in spor.iter().enumerate() {
            // Som på ekte HW må målingen finnes før rundens eneste steg.
            // sim_temp kjører også et innledende tick med forrige temperatur;
            // det ville forskjøvet sporet én prøve når samme ms ikke får gå to ganger.
            assert!(m.hw.as_mut().unwrap().set_sim_temperature(*t));
            m.handle(1, "status", &json!({}), T0 + s as i64 * 1000)
                .unwrap();
            let h = hw(&mut m);
            let fikk: Vec<f64> = match h["fans"].as_array() {
                Some(f) => f.iter().map(|x| x.as_f64().unwrap()).collect(),
                None => vec![h["fanNow"].as_f64().unwrap(); 2],
            };
            assert_eq!(&fikk, ventet, "sekund {s}, {t} °C");
            nod += usize::from(m.emergency);
        }
        let hoyest = spor.iter().map(|p| p.0).fold(0.0, f64::max);
        assert!(
            nod > 0,
            "nødregelen skal være med i sporet (høyest {hoyest} °C)"
        );
        assert!(
            spor.windows(2).any(|w| w[1].1[0] < w[0].1[0]),
            "viftene skal gå ned igjen"
        );
        assert_ne!(spor[200].1[0], spor[200].1[1], "egen kurve per vifte");
    }

    #[test]
    fn trinn_trukket_tilbake_gir_standard() {
        let mut m = motor(r#""effekt""#);
        let r = m
            .handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0)
            .unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0)
            .unwrap();
        m.set_config(Config::parse(r#"{"versjon":1,"uid":1000,"trinn":[]}"#), T0);
        assert!(hw(&mut m)["stock"].as_bool().unwrap());
        m.set_config(Err("borte".into()), T0);
        assert!(m.steps().is_empty());
    }

    #[test]
    fn sperret_kort_svarer_write_locked() {
        let mut m = Motor::new(None, "nvml", Some("sperret".into()));
        m.set_config(
            Config::parse(r#"{"versjon":1,"uid":1000,"trinn":["effekt"]}"#),
            T0,
        );
        let e = m
            .handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::WriteLocked);
        assert_eq!(e.details.unwrap()["reason"], "real_writes_off");
        assert_eq!(
            m.handle(1, "sim_temp", &json!({"celsius": 90}), T0)
                .unwrap_err()
                .code,
            ErrorCode::UnknownOp
        );
        assert_eq!(
            m.handle(1, "exec", &json!({"cmd": "sh"}), T0)
                .unwrap_err()
                .code,
            ErrorCode::UnknownOp
        );
    }
}
