//! Motoren i hjelperen: trinnene, grensesjekken, bekreft eller rull tilbake
//! (`rigg_core::txn::Book`), nødregelen, viftekurven og journalen. Ingen
//! socket og ingen klokke her: tiden sendes inn, så alt kan testes direkte.
//!
//! Reglene (hjelper-sikkerhet.md kap. 3):
//! - Et felt hvis trinn ikke er godkjent i `helper.json`, gir `step_not_enabled`.
//! - Et tall utenfor området kortet melder, gir `invalid_argument` (ikke klemming).
//! - Alt som skrives, er midlertidig til `confirm` (frist 10–60 s, standard 15 s).
//! - Forbindelsen som skrev, forsvinner (`riggd` borte): ventende endring
//!   rulles tilbake, og kortet går til standard med viftene på Auto.
//! - Over 85 °C: ventende endring rulles tilbake, og viftene går til 100 %
//!   (når trinnet Vifter er godkjent; ellers styrer driveren dem som før) til
//!   kortet er under 80 °C.
//! - Hjelperen stopper: kortet går til standard.

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
}

fn owner(conn: u64) -> String {
    format!("c{conn}")
}

/// Et tall i `patch` som må ligge i `r`, ellers avvist.
fn in_range(field: &str, label: &str, unit: &str, v: Option<&Value>, r: Range) -> Result<(), ProtoError> {
    let Some(v) = v else { return Ok(()) };
    let Some(x) = v.as_f64().filter(|x| x.is_finite()) else {
        return Err(ProtoError::msg(ErrorCode::InvalidArgument, format!("{label} må være et tall")));
    };
    if x < r.min || x > r.max {
        return Err(ProtoError::msg(ErrorCode::InvalidArgument, format!("{label} må være {}–{} {unit} (kortets område). Ingenting er endret.", r.min, r.max))
            .details(json!({"field": field, "value": x, "min": r.min, "max": r.max, "source": "hardware", "reason": "out_of_range"})));
    }
    Ok(())
}

/// Grensesjekken før noe annet: verdier utenfor kortets områder avvises.
pub fn check_ranges(patch: &Value, l: &Limits) -> Result<(), ProtoError> {
    in_range("powerLimitW", "Effektgrensen", "W", patch.get("powerLimitW"), l.power_w)?;
    in_range("tempTargetC", "Temperaturmålet", "°C", patch.get("tempTargetC"), l.temp_target_c)?;
    if let Some(c) = patch.get("clockOffsets") {
        in_range("clockOffsets.coreMhz", "Kjerneklokke-offset", "MHz", c.get("coreMhz"), l.core_mhz)?;
        in_range("clockOffsets.memMhz", "Minneklokke-offset", "MHz", c.get("memMhz"), l.mem_mhz)?;
    }
    if let Some(f) = patch.get("fan") {
        in_range("fan.manualPercent", "Viftefarten", "%", f.get("manualPercent"), l.fan_pct)?;
        // Felles kurve og kurvene per vifte (7B): hvert punkt i kortets område.
        let per_fan = f.get("curves").and_then(Value::as_array).into_iter().flatten();
        for pts in f.get("curve").into_iter().chain(per_fan).filter_map(Value::as_array) {
            for p in pts {
                in_range("fan.curve", "Viftefarten i kurven", "%", p.get(1), l.fan_pct)?;
                in_range("fan.curve", "Temperaturen i kurven", "°C", p.get(0), Range::new(0.0, 110.0))?;
            }
        }
    }
    Ok(())
}

impl Motor {
    pub fn new(hw: Option<Box<dyn Skriver>>, backend: &'static str, locked: Option<String>) -> Motor {
        let limits = hw.as_ref().map(|h| h.limits()).unwrap_or_else(Limits::rtx2080);
        let mut book = Book::new();
        book.confirmed = limits.stock();
        book.active = limits.stock();
        Motor { book, hw, backend, locked, cfg: None, cfg_err: None, emergency: false, lease: None, events: Vec::new(), curve_pct: None, vifte: None, vifte_ms: None, limits }
    }

    pub fn steps(&self) -> Vec<Step> {
        self.cfg.as_ref().map(|c| c.steps.clone()).unwrap_or_default()
    }

    fn enabled(&self, s: Step) -> bool {
        self.cfg.as_ref().is_some_and(|c| c.enabled(s))
    }

    /// Ny `helper.json` (eller ingen). Færre trinn enn før: alt til standard.
    pub fn set_config(&mut self, cfg: Result<Config, String>, now: i64) {
        let before = self.steps();
        match cfg {
            Ok(c) => {
                self.cfg = Some(c);
                self.cfg_err = None;
            }
            Err(e) => {
                journal::warn(&format!("helper.json kan ikke leses ({e}). Ingen trinn er godkjent."));
                self.cfg = None;
                self.cfg_err = Some(e);
            }
        }
        let after = self.steps();
        if before != after {
            journal::notice(&format!("Godkjente trinn: {}", names(&after)));
        }
        if before.iter().any(|s| !after.contains(s)) {
            self.til_standard("trinn trukket tilbake", now);
        }
    }

    /// Nødregelen gjelder: over 85 °C (hjelperens egen regel), eller
    /// viftemotoren har gått til 100 % fordi temperaturen ikke kan leses.
    fn nod(&self) -> bool {
        self.emergency || self.vifte.as_ref().is_some_and(vifte::Motor::nod)
    }

    pub fn status(&mut self, now: i64) -> Value {
        let mut state = self.book.state_view(now);
        state["emergency"] = json!(self.nod());
        let l = &self.limits;
        json!({
            "helper": {
                "name": "agentic-rigg-helper",
                "version": rigg_core::version::VERSION,
                "build": rigg_core::version::build(),
                "protocol": hjelper::PROTOCOL,
                "pid": std::process::id(),
                "backend": self.backend,
                "realWrites": rigg_hw::gpu::skriv::real_writes_available(),
                "locked": self.locked,
            },
            "uid": self.cfg.as_ref().map(|c| c.uid),
            "steps": self.steps().iter().map(|s| s.id()).collect::<Vec<_>>(),
            "configError": self.cfg_err,
            "state": state,
            "limits": {
                "powerLimitW": [l.power_w.min, l.power_w.max],
                "coreOffsetMhz": [l.core_mhz.min, l.core_mhz.max],
                "memOffsetMhz": [l.mem_mhz.min, l.mem_mhz.max],
                "tempTargetC": [l.temp_target_c.min, l.temp_target_c.max],
                "fanPercent": [l.fan_pct.min, l.fan_pct.max],
            },
            "hardware": self.hw.as_ref().map(|h| h.snapshot()),
            "emergency": self.nod(),
            "events": std::mem::take(&mut self.events),
        })
    }

    /// Én forespørsel fra forbindelsen `conn`.
    pub fn handle(&mut self, conn: u64, op: &str, args: &Value, now: i64) -> OpResult {
        match op {
            "status" => Ok(self.status(now)),
            "apply" => self.apply(conn, args, now),
            "confirm" | "revert" => {
                let txn = args.get("txn").and_then(Value::as_str);
                let r = if op == "confirm" { self.book.confirm(&owner(conn), txn, now) } else { self.book.revert(&owner(conn), txn, now) };
                let r = r?;
                self.collect_events();
                self.push_hw(&format!("{op} {}", txn.unwrap_or("")), now)?;
                let mut r = r;
                r["emergency"] = json!(self.emergency);
                Ok(r)
            }
            "reset" => self.reset(conn, now),
            "rgb_get" | "rgb_set" => {
                if !self.enabled(Step::Lys) {
                    return Err(hjelper::step_not_enabled(&[Step::Lys]));
                }
                Err(ProtoError::msg(ErrorCode::NotImplemented, "Lys er ikke laget ennå.").details(json!({"status": "planned"})))
            }
            "sim_temp" if self.backend == "sim" => {
                let c = args.get("celsius").and_then(Value::as_f64).filter(|c| (0.0..=120.0).contains(c)).ok_or_else(|| ProtoError::msg(ErrorCode::InvalidArgument, "celsius må være 0–120"))?;
                if let Some(h) = self.hw.as_mut() {
                    h.set_sim_temperature(c);
                }
                self.tick(now);
                Ok(json!({"tempC": c, "emergency": self.emergency}))
            }
            _ => Err(ProtoError::new(ErrorCode::UnknownOp)),
        }
    }

    fn apply(&mut self, conn: u64, args: &Value, now: i64) -> OpResult {
        if self.hw.is_none() {
            return Err(hjelper::locked("real_writes_off"));
        }
        let patch = args.get("settings").cloned().unwrap_or(Value::Null);
        let need = hjelper::steps_for_patch(&patch)?;
        let missing: Vec<Step> = need.iter().copied().filter(|s| !self.enabled(*s)).collect();
        if !missing.is_empty() {
            journal::info(&format!("Avvist (trinn ikke godkjent): {}", names(&missing)));
            return Err(hjelper::step_not_enabled(&missing));
        }
        if self.emergency {
            return Err(ProtoError::msg(ErrorCode::Busy, "Nødregelen gjelder (over 85 °C). Endringer venter til kortet er under 80 °C.").details(json!({"reason": "emergency"})));
        }
        if let Err(e) = check_ranges(&patch, &self.limits) {
            journal::info(&format!("Avvist (utenfor området): {}", e.message));
            return Err(e);
        }
        let before = self.book.active.clone();
        let r = self.book.apply(&owner(conn), &patch, args.get("confirmTimeoutS"), now)?;
        self.lease = Some(conn);
        self.curve_pct = None;
        let what = describe(&before, &self.book.active);
        if !what.is_empty() {
            let deadline = self.book.pending().map(|p| p.timeout_s).unwrap_or(0);
            journal::notice(&format!("Endring ({}): {what}. Venter på Behold i {deadline} s.", r["txn"].as_str().unwrap_or("–")));
        }
        self.push_hw("apply", now)?;
        Ok(r)
    }

    fn reset(&mut self, conn: u64, now: i64) -> OpResult {
        if let Some(p) = self.book.pending()
            && p.owner != owner(conn)
        {
            return Err(ProtoError::new(ErrorCode::Busy).details(json!({"owner": p.owner})));
        }
        self.til_standard("tilbakestill alt", now);
        self.collect_events();
        let mut s = self.book.state_view(now);
        s["emergency"] = json!(self.emergency);
        Ok(s)
    }

    /// Alt tilbake til standard: ventende endring rulles tilbake, det
    /// bekreftede glemmes, og kortet får standardverdiene (Auto på viftene).
    pub fn til_standard(&mut self, why: &str, now: i64) {
        let had = self.book.active != self.limits.stock() || self.book.pending().is_some();
        self.book.rollback("reset");
        self.book.confirmed = self.limits.stock();
        self.book.active = self.limits.stock();
        self.curve_pct = None;
        self.collect_events();
        if had {
            journal::notice(&format!("Tilbake til standard ({why}): effekt, offset og temperaturmål som kortet leverer, viftene på Auto."));
        }
        let _ = self.push_hw(why, now);
    }

    fn collect_events(&mut self) {
        for e in self.book.take_events() {
            self.events.push(e.to_json());
        }
        if self.events.len() > 64 {
            let n = self.events.len() - 64;
            self.events.drain(..n);
        }
    }

    /// Legger `book.active` på kortet. Feiler det, rulles alt tilbake til
    /// standard (det trygge), og feilen gis videre.
    fn push_hw(&mut self, why: &str, now: i64) -> Result<(), ProtoError> {
        let v = self.effective();
        let Some(h) = self.hw.as_mut() else { return Ok(()) };
        if let Err(e) = h.apply(&v) {
            journal::err(&format!("Kortet tok ikke imot ({why}): {e}. Tilbake til standard."));
            self.book.rollback("reset");
            self.book.confirmed = self.limits.stock();
            self.book.active = self.limits.stock();
            self.collect_events();
            if let Some(h) = self.hw.as_mut() {
                let _ = h.reset();
            }
            return Err(ProtoError::msg(ErrorCode::Internal, format!("Kortet tok ikke imot endringen: {e}")));
        }
        self.drive_fans(now);
        Ok(())
    }

    /// Det som skal ligge på kortet: `active`, med viftene på 100 % mens
    /// nødregelen gjelder (og trinnet Vifter er godkjent).
    fn effective(&self) -> Values {
        let mut v = self.book.active.clone();
        if self.emergency && self.enabled(Step::Vifter) {
            v.fan_mode = FanMode::Manual;
            v.fan_percent = self.limits.fan_pct.max;
        }
        v
    }

    /// Viftekurven: viftemotoren (`rigg_core::vifte`) ved temperaturen nå,
    /// slik 7B beskrev den og slik tjenestens sim-sløyfe bruker den
    /// (`rigg_hw::gpu::sim_vifter`): `Oppsett::fra_values`, `Motor::new(o)
    /// .med_start(farten viftene har)` når Kurve slås på, `sett_oppsett` bare
    /// når oppsettet er nytt, og ett `steg` per runde. Mens nødregelen
    /// gjelder, går motoren videre (den har den samme regelen og står på
    /// 100 %), men hjelperen skriver 100 % selv; etterpå rampes viftene ned
    /// fra 100 % i stedet for å falle rett til kurven.
    fn drive_fans(&mut self, now: i64) {
        if self.book.active.fan_mode != FanMode::Curve || !self.enabled(Step::Vifter) {
            self.vifte = None;
            self.vifte_ms = None;
            return;
        }
        let Some(h) = self.hw.as_mut() else { return };
        let o = vifte::Oppsett::fra_values(&self.book.active, h.fan_count(), self.limits.fan_pct.min);
        match self.vifte.as_mut() {
            Some(m) if m.oppsett() != &o => m.sett_oppsett(o),
            Some(_) => {}
            None => self.vifte = Some(vifte::Motor::new(o).med_start(&h.fan_percents())),
        }
        if self.vifte_ms == Some(now) {
            return;
        }
        self.vifte_ms = Some(now);
        let t = h.temperature();
        let Some(m) = self.vifte.as_mut() else { return };
        let steg = m.steg(t, now);
        if self.emergency {
            return;
        }
        let pct: Vec<f64> = steg.heltall(self.limits.fan_pct.min).into_iter().map(f64::from).collect();
        if self.curve_pct.as_ref() != Some(&pct) {
            let _ = h.set_fan_percents(&pct);
            self.curve_pct = Some(pct);
        }
    }

    /// Hvert sekund: fristen, nødregelen og viftekurven.
    pub fn tick(&mut self, now: i64) {
        if self.book.tick(now) {
            self.collect_events();
            journal::notice("Fristen gikk ut uten Behold. Rullet tilbake til det bekreftede.");
            let _ = self.push_hw("frist", now);
        }
        let temp = self.hw.as_mut().and_then(|h| h.temperature());
        if let Some(t) = temp {
            if !self.emergency && t >= txn::EMERGENCY_FAN_C {
                self.emergency = true;
                journal::warn(&format!("Nødregelen: {t:.0} °C (grense {:.0} °C). Viftene til 100 %, ventende endring rulles tilbake.", txn::EMERGENCY_FAN_C));
                if self.book.rollback("emergency") {
                    self.collect_events();
                }
                self.curve_pct = None;
                let _ = self.push_hw("nødregel", now);
            } else if self.emergency && t < txn::EMERGENCY_RELEASE_C {
                self.emergency = false;
                self.curve_pct = None;
                journal::notice(&format!("Nødregelen er over: {t:.0} °C. Viftene tilbake til oppsettet."));
                let _ = self.push_hw("nødregel over", now);
            }
        }
        self.drive_fans(now);
    }

    /// En forbindelse er lukket. Var det den som skrev (`riggd`), går kortet
    /// til standard.
    pub fn disconnect(&mut self, conn: u64, now: i64) {
        if self.book.disconnect(&owner(conn)) {
            self.collect_events();
        }
        if self.lease == Some(conn) {
            self.lease = None;
            self.til_standard("tjenesten koblet fra", now);
        }
    }

    /// Hjelperen stopper.
    pub fn shutdown(&mut self, now: i64) {
        self.book.rollback("daemon_stopping");
        self.til_standard("hjelperen stopper", now);
    }
}

fn names(s: &[Step]) -> String {
    if s.is_empty() { "ingen".into() } else { s.iter().map(|x| x.id()).collect::<Vec<_>>().join(", ") }
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
        out.push(format!("viftemodus {} → {}", a.fan_mode.as_str(), b.fan_mode.as_str()));
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

    const T0: i64 = 1_000_000;

    fn motor(steps: &str) -> Motor {
        let mut m = Motor::new(Some(Box::new(SimSkriver::new())), "sim", None);
        m.set_config(Config::parse(&format!(r#"{{"versjon":1,"uid":1000,"trinn":[{steps}]}}"#)), T0);
        m
    }

    fn hw(m: &mut Motor) -> Value {
        m.status(T0)["hardware"].clone()
    }

    #[test]
    fn uten_trinn_avvises_alt() {
        let mut m = motor("");
        for p in [json!({"powerLimitW": 200}), json!({"fan": {"mode": "manual"}}), json!({"clockOffsets": {"coreMhz": 50}})] {
            let e = m.handle(1, "apply", &json!({"settings": p}), T0).unwrap_err();
            assert_eq!(e.code, ErrorCode::StepNotEnabled);
        }
        assert_eq!(m.handle(1, "rgb_set", &json!({}), T0).unwrap_err().code, ErrorCode::StepNotEnabled);
        assert!(hw(&mut m)["stock"].as_bool().unwrap());
    }

    #[test]
    fn godkjent_trinn_gir_txn_og_behold() {
        let mut m = motor(r#""effekt""#);
        let r = m.handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0).unwrap();
        assert_eq!(hw(&mut m)["applied"]["powerLimitW"], 200.0);
        // Et annet trinn i samme forespørsel avvises helt.
        assert_eq!(m.handle(1, "apply", &json!({"settings": {"powerLimitW": 210, "fan": {"mode": "manual"}}}), T0).unwrap_err().code, ErrorCode::StepNotEnabled);
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0 + 1000).unwrap();
        assert_eq!(m.book.confirmed.power_limit_w, 200.0);
    }

    #[test]
    fn utenfor_kortets_omrade_avvises() {
        let mut m = motor(r#""effekt","vifter","klokker""#);
        for p in [json!({"powerLimitW": 300}), json!({"powerLimitW": 50}), json!({"tempTargetC": 95}), json!({"clockOffsets": {"coreMhz": 1500}}), json!({"fan": {"manualPercent": 20}}), json!({"fan": {"curve": [[30, 10], [60, 50]]}}), json!({"fan": {"curves": [[[30, 40], [60, 50]], [[30, 20], [60, 50]]]}})] {
            let e = m.handle(1, "apply", &json!({"settings": p}), T0).unwrap_err();
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
        let r = m.handle(1, "apply", &json!({"settings": {"fan": {"mode": "manual", "manualPercent": 60}}}), T0).unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0).unwrap();
        m.disconnect(2, T0);
        assert_eq!(hw(&mut m)["fanNow"], 60.0, "en annen forbindelse betyr ingenting");
        m.disconnect(1, T0);
        assert!(hw(&mut m)["stock"].as_bool().unwrap(), "riggd borte: standard");
    }

    #[test]
    fn nodregelen_gir_full_vifte_og_slipper_under_80() {
        let mut m = motor(r#""vifter","effekt""#);
        m.handle(1, "apply", &json!({"settings": {"powerLimitW": 260, "fan": {"mode": "curve"}}}), T0).unwrap();
        m.handle(1, "sim_temp", &json!({"celsius": 86}), T0).unwrap();
        assert!(m.emergency);
        assert_eq!(hw(&mut m)["fanNow"], 100.0);
        assert_eq!(m.book.active.power_limit_w, 245.0, "ventende endring rullet tilbake");
        assert_eq!(m.handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0).unwrap_err().code, ErrorCode::Busy);
        m.handle(1, "sim_temp", &json!({"celsius": 82}), T0).unwrap();
        assert!(m.emergency, "hysterese: 85 inn, 80 ut");
        m.handle(1, "sim_temp", &json!({"celsius": 79}), T0).unwrap();
        assert!(!m.emergency);
        assert!(hw(&mut m)["fanNow"].is_null(), "tilbake til Auto");
    }

    #[test]
    fn kurven_folger_temperaturen_med_viftemotoren() {
        let mut m = motor(r#""vifter""#);
        let r = m.handle(1, "apply", &json!({"settings": {"fan": {"mode": "curve", "curve": [[40, 40], [70, 80]]}}}), T0).unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0).unwrap();
        m.handle(1, "sim_temp", &json!({"celsius": 55}), T0).unwrap();
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
                let mut g = BTreeMap::from([("k".to_string(), GpuSample { temp: Some(40.0), power: Some(effekt(s)), fan: Some(35.0), fans: vec![Some(35.0); 2], ..Default::default() })]);
                tjenesten.runde(&mut g, &a, 35.0, T0 + s * 1000);
                (g["k"].temp.unwrap(), g["k"].fans.iter().map(|f| f.unwrap()).collect())
            })
            .collect();

        // Hjelperen: kurven tas imot fra Auto ved samme temperatur og tid, og
        // får så de samme temperaturene, én runde i sekundet.
        let mut m = motor(r#""vifter""#);
        m.handle(1, "sim_temp", &json!({"celsius": spor[0].0}), T0 - 1000).unwrap();
        let r = m.handle(1, "apply", &json!({"settings": {"fan": fan}}), T0).unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0).unwrap();
        assert_eq!(m.book.active, a);
        let mut nod = 0;
        for (s, (t, ventet)) in spor.iter().enumerate() {
            m.handle(1, "sim_temp", &json!({"celsius": t}), T0 + s as i64 * 1000).unwrap();
            let h = hw(&mut m);
            let fikk: Vec<f64> = match h["fans"].as_array() {
                Some(f) => f.iter().map(|x| x.as_f64().unwrap()).collect(),
                None => vec![h["fanNow"].as_f64().unwrap(); 2],
            };
            assert_eq!(&fikk, ventet, "sekund {s}, {t} °C");
            nod += usize::from(m.emergency);
        }
        let hoyest = spor.iter().map(|p| p.0).fold(0.0, f64::max);
        assert!(nod > 0, "nødregelen skal være med i sporet (høyest {hoyest} °C)");
        assert!(spor.windows(2).any(|w| w[1].1[0] < w[0].1[0]), "viftene skal gå ned igjen");
        assert_ne!(spor[200].1[0], spor[200].1[1], "egen kurve per vifte");
    }

    #[test]
    fn trinn_trukket_tilbake_gir_standard() {
        let mut m = motor(r#""effekt""#);
        let r = m.handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0).unwrap();
        m.handle(1, "confirm", &json!({"txn": r["txn"]}), T0).unwrap();
        m.set_config(Config::parse(r#"{"versjon":1,"uid":1000,"trinn":[]}"#), T0);
        assert!(hw(&mut m)["stock"].as_bool().unwrap());
        m.set_config(Err("borte".into()), T0);
        assert!(m.steps().is_empty());
    }

    #[test]
    fn sperret_kort_svarer_write_locked() {
        let mut m = Motor::new(None, "nvml", Some("sperret".into()));
        m.set_config(Config::parse(r#"{"versjon":1,"uid":1000,"trinn":["effekt"]}"#), T0);
        let e = m.handle(1, "apply", &json!({"settings": {"powerLimitW": 200}}), T0).unwrap_err();
        assert_eq!(e.code, ErrorCode::WriteLocked);
        assert_eq!(e.details.unwrap()["reason"], "real_writes_off");
        assert_eq!(m.handle(1, "sim_temp", &json!({"celsius": 90}), T0).unwrap_err().code, ErrorCode::UnknownOp);
        assert_eq!(m.handle(1, "exec", &json!({"cmd": "sh"}), T0).unwrap_err().code, ErrorCode::UnknownOp);
    }
}
