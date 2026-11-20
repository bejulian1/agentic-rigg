//! Bekreft eller rull tilbake (docs/daemon-api.md kap. 6) for den simulerte
//! backenden. Ingenting her når skjermkortet: dette er bare tilstand i
//! minnet, og [`Book::handle`] svarer `write_locked` for alle andre
//! backender enn `sim`, uansett hva som spørres om.
//!
//! **Eies av 2C (Styring og Lys).** Port av `daemon-mock/card.js`
//! (`MockCard.write`, `confirm`, `revert`, `rollback`, `reset`,
//! `sessionClosed`) uten tidtakere: tiden sendes inn (`now`), og tjenesten
//! kaller [`Book::tick`] hver runde. Da kan alt testes uten å vente.
//!
//! Kontrakten (daemon-api.md kap. 6):
//! 1. `apply` gir en txn som venter på `confirm` innen fristen (10–60 s).
//! 2. Bare klienten som gjorde endringen, kan bekrefte eller rulle tilbake.
//! 3. Én ventende endring om gangen: en annen klient får `busy`.
//! 4. Fristen går ut → tilbake til siste bekreftede oppsett.
//! 5. Klienten kobler fra → det den ikke har bekreftet, rulles tilbake.
//! 6. `reset` er trygg retning og bekreftes straks.
//!
//! Vinduet bruker også [`write_gate`]: Bruk er bare aktiv når tjenesten
//! melder `capabilities.write.state == "sim"` og profilen er `dev`.

use crate::proto::{ErrorCode, OpResult, ProtoError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxnStatus {
    Pending,
    Confirmed,
    RolledBack,
}

/// En endring som venter på Behold.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Txn {
    pub id: String,
    pub owner: String,
    pub timeout_s: u32,
    pub deadline: i64,
    pub changed: Vec<String>,
    pub status: TxnStatus,
}

// ---------------------------------------------------------------- grenser

/// Et område `min..=max`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub const fn new(min: f64, max: f64) -> Range {
        Range { min, max }
    }
    pub fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.min, self.max)
    }
}

/// Grensene kortet melder (RTX 2080, målt med NVML: maskinvare-revisjon kap. 8).
pub const HW_POWER: Range = Range::new(105.0, 275.0);
pub const HW_CORE: Range = Range::new(-1000.0, 1000.0);
/// Effektiv MHz. NVML oppgir dobbel verdi (−2000…+6000).
pub const HW_MEM: Range = Range::new(-1000.0, 3000.0);
pub const HW_TEMP: Range = Range::new(65.0, 88.0);
pub const HW_FAN: Range = Range::new(35.0, 100.0);
/// Tjenestens egen policy, strengere enn kortet (som `daemon-mock/card.js`).
pub const POLICY_CORE: Range = Range::new(-200.0, 300.0);
pub const POLICY_MEM: Range = Range::new(-1000.0, 1500.0);
/// Fristen for Behold (s): standard, minst og mest.
pub const CONFIRM_DEFAULT_S: u32 = 15;
pub const CONFIRM_MIN_S: u32 = 10;
pub const CONFIRM_MAX_S: u32 = 60;
/// Nødregelen: full vifte over 85 °C til temperaturen er under 80 °C.
pub const EMERGENCY_FAN_C: f64 = 85.0;
pub const EMERGENCY_RELEASE_C: f64 = 80.0;
/// Viftekurven: 2–10 punkter, 0–110 °C.
pub const CURVE_MIN_POINTS: usize = 2;
pub const CURVE_MAX_POINTS: usize = 10;

/// Standardkurven (Balansert), som demoens `DEFAULT_CURVE`.
pub const DEFAULT_CURVE: [(f64, f64); 6] = [
    (30.0, 35.0),
    (50.0, 40.0),
    (62.0, 50.0),
    (72.0, 65.0),
    (82.0, 85.0),
    (90.0, 100.0),
];

// ---------------------------------------------------------------- oppsettet

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FanMode {
    /// Driveren styrer viften (policy 0).
    #[default]
    Auto,
    Curve,
    Manual,
}

impl FanMode {
    pub fn parse(s: &str) -> Option<FanMode> {
        match s {
            "auto" => Some(FanMode::Auto),
            "curve" => Some(FanMode::Curve),
            "manual" => Some(FanMode::Manual),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            FanMode::Auto => "auto",
            FanMode::Curve => "curve",
            FanMode::Manual => "manual",
        }
    }
}

/// Det som kan settes (demoens `GpuSettings`, flatt).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Values {
    pub power_limit_w: f64,
    pub core_offset_mhz: f64,
    pub mem_offset_mhz: f64,
    pub temp_target_c: f64,
    pub fan_mode: FanMode,
    pub fan_percent: f64,
    /// Den felles kurven, og kurven for vifter som mangler i `fan_curves`.
    pub fan_curve: Vec<(f64, f64)>,
    // ---- Runde 7B (viftemotoren, `crate::vifte`). Alle har standardverdier,
    // så utkast og profiler fra 1.1.0 leses som før.
    /// Egen kurve per vifte (`fan_curves`) i stedet for den felles.
    pub fan_per_fan: bool,
    /// Én kurve per vifte (vifte 1 først). Brukes bare med `fan_per_fan`.
    pub fan_curves: Vec<Vec<(f64, f64)>>,
    /// Hysterese (°C): viftene går ikke ned før temperaturen har falt så mye.
    pub fan_hysteresis_c: f64,
    /// Rampefart (% per sekund) opp og ned.
    pub fan_ramp_up_pct_s: f64,
    pub fan_ramp_down_pct_s: f64,
    /// Utjevning av temperaturen (s, 0 = av).
    pub fan_smoothing_s: f64,
    /// Hold minste fart til denne temperaturen (`None`: av).
    pub fan_quiet_until_c: Option<f64>,
}

impl Default for Values {
    /// Kortets standard: 245 W, ingen offset, 83 °C, driveren styrer viften.
    /// Viftekurven: Balansert, felles, 3 °C hysterese, 10 %/s opp og 3 %/s ned.
    fn default() -> Self {
        Values {
            power_limit_w: 245.0,
            core_offset_mhz: 0.0,
            mem_offset_mhz: 0.0,
            temp_target_c: 83.0,
            fan_mode: FanMode::Auto,
            fan_percent: 50.0,
            fan_curve: DEFAULT_CURVE.to_vec(),
            fan_per_fan: false,
            fan_curves: Vec::new(),
            fan_hysteresis_c: crate::vifte::HYSTERESE_STANDARD_C,
            fan_ramp_up_pct_s: crate::vifte::RAMPE_OPP_STANDARD,
            fan_ramp_down_pct_s: crate::vifte::RAMPE_NED_STANDARD,
            fan_smoothing_s: 0.0,
            fan_quiet_until_c: None,
        }
    }
}

impl Values {
    /// Feltene som er ulike (navnene i protokollen).
    pub fn diff(&self, o: &Values) -> Vec<String> {
        let mut out = Vec::new();
        let mut d = |c: bool, name: &str| {
            if c {
                out.push(name.to_string());
            }
        };
        d(self.power_limit_w != o.power_limit_w, "powerLimitW");
        d(
            self.core_offset_mhz != o.core_offset_mhz,
            "clockOffsets.coreMhz",
        );
        d(
            self.mem_offset_mhz != o.mem_offset_mhz,
            "clockOffsets.memMhz",
        );
        d(self.temp_target_c != o.temp_target_c, "tempTargetC");
        d(self.fan_mode != o.fan_mode, "fan.mode");
        d(self.fan_percent != o.fan_percent, "fan.manualPercent");
        d(self.fan_curve != o.fan_curve, "fan.curve");
        d(self.fan_per_fan != o.fan_per_fan, "fan.perFan");
        d(self.fan_curves != o.fan_curves, "fan.curves");
        d(
            self.fan_hysteresis_c != o.fan_hysteresis_c,
            "fan.hysteresisC",
        );
        d(
            self.fan_ramp_up_pct_s != o.fan_ramp_up_pct_s,
            "fan.rampUpPctS",
        );
        d(
            self.fan_ramp_down_pct_s != o.fan_ramp_down_pct_s,
            "fan.rampDownPctS",
        );
        d(self.fan_smoothing_s != o.fan_smoothing_s, "fan.smoothingS");
        d(
            self.fan_quiet_until_c != o.fan_quiet_until_c,
            "fan.quietUntilC",
        );
        out
    }

    /// Kurven vifte `i` følger (den felles uten `fan_per_fan`).
    pub fn curve_for(&self, i: usize) -> &[(f64, f64)] {
        if self.fan_per_fan {
            self.fan_curves
                .get(i)
                .filter(|c| !c.is_empty())
                .unwrap_or(&self.fan_curve)
        } else {
            &self.fan_curve
        }
    }

    /// Som protokollens `GpuSettings` (nøstet).
    pub fn to_json(&self) -> Value {
        json!({
            "powerLimitW": self.power_limit_w,
            "clockOffsets": {"coreMhz": self.core_offset_mhz, "memMhz": self.mem_offset_mhz},
            "tempTargetC": self.temp_target_c,
            "fan": {
                "mode": self.fan_mode.as_str(),
                "manualPercent": self.fan_percent,
                "curve": curve_json(&self.fan_curve),
                "perFan": self.fan_per_fan,
                "curves": self.fan_curves.iter().map(|c| curve_json(c)).collect::<Vec<_>>(),
                "hysteresisC": self.fan_hysteresis_c,
                "rampUpPctS": self.fan_ramp_up_pct_s,
                "rampDownPctS": self.fan_ramp_down_pct_s,
                "smoothingS": self.fan_smoothing_s,
                "quietUntilC": self.fan_quiet_until_c,
            },
        })
    }
}

fn curve_json(c: &[(f64, f64)]) -> Value {
    json!(c.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>())
}

/// En verdi tjenesten klemte (`clamped` i svaret).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clamped {
    pub field: String,
    pub requested: f64,
    pub applied: f64,
    /// «min» eller «max».
    pub bound: &'static str,
    /// «hardware» (kortets grense) eller «policy» (tjenestens strengere).
    pub source: &'static str,
}

fn invalid(msg: impl Into<String>) -> ProtoError {
    ProtoError::msg(ErrorCode::InvalidArgument, msg)
}

pub(crate) fn display_validation_message(message: &str) -> &str {
    match message {
        LOCKED_TIP => crate::tr!("Aktiver skjermkortstyring i Innstillinger, og vent til hjelperen er klar"),
        "Viftekurven må være en liste med punkter" => crate::tr!("Viftekurven må være en liste med punkter"),
        "Temperaturene i viftekurven må stige" => crate::tr!("Temperaturene i viftekurven må stige"),
        "fan.curves må være en liste med kurver" => crate::tr!("fan.curves må være en liste med kurver"),
        "settings må være et objekt" => crate::tr!("settings må være et objekt"),
        "clockOffsets må være et objekt" => crate::tr!("clockOffsets må være et objekt"),
        "fan må være et objekt" => crate::tr!("fan må være et objekt"),
        "fan.mode må være auto, curve eller manual" => crate::tr!("fan.mode må være auto, curve eller manual"),
        "fan.perFan må være true eller false" => crate::tr!("fan.perFan må være true eller false"),
        "confirmTimeoutS må være et tall" => crate::tr!("confirmTimeoutS må være et tall"),
        "Oppsettet må være innenfor rapporterte grenser og viftekurven må være stigende." => crate::tr!("Oppsettet må være innenfor rapporterte grenser og viftekurven må være stigende."),
        _ => message,
    }
}

pub(crate) fn display_validation_details(message: &str) -> Option<String> {
    fn number(text: &str) -> Option<String> {
        let value = text.parse::<u32>().ok()?;
        Some(crate::fmt::num(Some(f64::from(value)), 0))
    }

    if let Some((fan, cause)) = message.strip_prefix("Vifte ").and_then(|s| s.split_once(": "))
        && let Some(fan) = number(fan)
    {
        let fixed = display_validation_message(cause);
        let cause = display_curve_details(cause).or_else(|| (fixed != cause).then(|| fixed.to_owned()))?;
        return Some(crate::trf!("Vifte {nummer}: {feil}", nummer = fan, feil = cause));
    }
    if let Some(field) = message.strip_prefix("Ukjent felt: ") {
        return Some(crate::trf!("Ukjent felt: {felt}", felt = field));
    }
    if let Some(field) = message.strip_suffix(" må være et tall") {
        let name = match field {
            "Effektgrensen" => crate::tr!("Effektgrensen"),
            "Temperaturmålet" => crate::tr!("Temperaturmålet"),
            "Kjerneklokke-offset" => crate::tr!("Kjerneklokke-offset"),
            "Minneklokke-offset" => crate::tr!("Minneklokke-offset"),
            "Viftefarten" => crate::tr!("Viftefarten"),
            "Viftefarten i kurven" => crate::tr!("Viftefarten i kurven"),
            "Temperaturen i kurven" => crate::tr!("Temperaturen i kurven"),
            "Hysteresen" => crate::tr!("Hysteresen"),
            "Rampefarten opp" => crate::tr!("Rampefarten opp"),
            "Rampefarten ned" => crate::tr!("Rampefarten ned"),
            "Utjevningen" => crate::tr!("Utjevningen"),
            "«Hold minimum til»" => crate::tr!("«Hold minimum til»"),
            _ => return None,
        };
        return Some(crate::trf!("{felt} må være et tall", felt = name));
    }
    display_curve_details(message)
}

fn display_curve_details(message: &str) -> Option<String> {
    fn count(text: &str) -> Option<u32> {
        text.parse().ok()
    }

    if let Some((min, max)) = message.strip_prefix("Viftekurven må ha ")
        .and_then(|s| s.strip_suffix(" punkter"))
        .and_then(|s| s.split_once('–'))
        && let (Some(min), Some(max)) = (count(min), count(max))
    {
        return Some(crate::trf!("Viftekurven må ha {min}–{maks} punkter", min = min, maks = max));
    }
    if let Some(point) = message.strip_prefix("Ugyldig punkt ")
        .and_then(|s| s.strip_suffix(" i viftekurven")).and_then(count)
    {
        return Some(crate::trf!("Ugyldig punkt {nummer} i viftekurven", nummer = point));
    }
    if let Some(point) = message.strip_prefix("Punkt ")
        .and_then(|s| s.strip_suffix(": temperaturen må være 0–110 °C")).and_then(count)
    {
        return Some(crate::trf!("Punkt {nummer}: temperaturen må være {min}–{maks}", nummer = point, min = crate::fmt::temp_num(Some(0.0), 0), maks = crate::fmt::temp(Some(110.0), 0)));
    }
    if let Some(max) = message.strip_prefix("fan.curves kan ha høyst ")
        .and_then(|s| s.strip_suffix(" kurver")).and_then(count)
    {
        return Some(crate::trf!("fan.curves kan ha høyst {maks} kurver", maks = max));
    }
    None
}

/// Klemmer ett tall og noterer det.
fn clamp_num(field: &str, v: f64, eff: Range, hw: Range, out: &mut Vec<Clamped>) -> f64 {
    let r = v.round();
    let c = eff.clamp(r);
    if c != r {
        let bound = if r > eff.max { "max" } else { "min" };
        let from_hw = if bound == "max" {
            hw.max == eff.max
        } else {
            hw.min == eff.min
        };
        out.push(Clamped {
            field: field.into(),
            requested: v,
            applied: c,
            bound,
            source: if from_hw { "hardware" } else { "policy" },
        });
    }
    c
}

/// Viftekurven: 2–10 punkter, stigende temperatur, fart 35–100 % og aldri
/// fallende (et punkt under forrige løftes, som demoen).
pub fn check_curve(v: &Value) -> Result<(Vec<(f64, f64)>, bool), ProtoError> {
    check_curve_in(v, HW_FAN)
}

fn check_curve_in(v: &Value, fan: Range) -> Result<(Vec<(f64, f64)>, bool), ProtoError> {
    let a = v
        .as_array()
        .ok_or_else(|| invalid("Viftekurven må være en liste med punkter"))?;
    if a.len() < CURVE_MIN_POINTS || a.len() > CURVE_MAX_POINTS {
        return Err(invalid(format!(
            "Viftekurven må ha {CURVE_MIN_POINTS}–{CURVE_MAX_POINTS} punkter"
        )));
    }
    let mut lifted = false;
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(a.len());
    for (i, p) in a.iter().enumerate() {
        let (t, f) = match p.as_array().map(|x| x.as_slice()) {
            Some([t, f]) => match (t.as_f64(), f.as_f64()) {
                (Some(t), Some(f)) if t.is_finite() && f.is_finite() => (t.round(), f.round()),
                _ => return Err(invalid(format!("Ugyldig punkt {} i viftekurven", i + 1))),
            },
            _ => return Err(invalid(format!("Ugyldig punkt {} i viftekurven", i + 1))),
        };
        if !(0.0..=110.0).contains(&t) {
            return Err(invalid(format!(
                "Punkt {}: temperaturen må være 0–110 °C",
                i + 1
            )));
        }
        let mut f2 = fan.clamp(f);
        if let Some(prev) = pts.last() {
            if t <= prev.0 {
                return Err(invalid("Temperaturene i viftekurven må stige"));
            }
            if f2 < prev.1 {
                f2 = prev.1;
            }
        }
        lifted |= f2 != f;
        pts.push((t, f2));
    }
    Ok((pts, lifted))
}

/// Én viftekurve: (°C, %), stigende temperatur.
pub type Curve = Vec<(f64, f64)>;

/// Kurvene per vifte (`fan.curves`): 0–8 kurver, hver som [`check_curve`].
pub fn check_curves(v: &Value) -> Result<(Vec<Curve>, bool), ProtoError> {
    check_curves_in(v, HW_FAN)
}

fn check_curves_in(v: &Value, fan: Range) -> Result<(Vec<Curve>, bool), ProtoError> {
    let a = v
        .as_array()
        .ok_or_else(|| invalid("fan.curves må være en liste med kurver"))?;
    if a.len() > crate::vifte::MAKS_VIFTER {
        return Err(invalid(format!(
            "fan.curves kan ha høyst {} kurver",
            crate::vifte::MAKS_VIFTER
        )));
    }
    let mut lifted = false;
    let mut out = Vec::with_capacity(a.len());
    for (i, c) in a.iter().enumerate() {
        let (pts, l) = check_curve_in(c, fan)
            .map_err(|e| invalid(format!("Vifte {}: {}", i + 1, e.message)))?;
        lifted |= l;
        out.push(pts);
    }
    Ok((out, lifted))
}

/// Fletter en delvis `GpuSettings` inn i `base`. Ukjente felt og feil typer
/// gir `invalid_argument`. Tall utenfor grensene klemmes.
#[derive(Clone, Copy, Debug)]
pub struct HardwareLimits {
    pub power: Range,
    pub core: Range,
    pub memory: Range,
    pub temperature: Range,
    pub fan: Range,
}

pub fn merge(base: &Values, patch: &Value) -> Result<(Values, Vec<Clamped>), ProtoError> {
    merge_in(base, patch, None)
}

fn merge_in(
    base: &Values,
    patch: &Value,
    limits: Option<&HardwareLimits>,
) -> Result<(Values, Vec<Clamped>), ProtoError> {
    let power = limits.map_or(HW_POWER, |l| l.power);
    let core = limits.map_or(HW_CORE, |l| l.core);
    let memory = limits.map_or(HW_MEM, |l| l.memory);
    let temperature = limits.map_or(HW_TEMP, |l| l.temperature);
    let fan = limits.map_or(HW_FAN, |l| l.fan);
    let o = patch
        .as_object()
        .ok_or_else(|| invalid("settings må være et objekt"))?;
    let mut out = base.clone();
    let mut cl = Vec::new();
    let num = |v: &Value, name: &str| -> Result<f64, ProtoError> {
        v.as_f64()
            .filter(|x| x.is_finite())
            .ok_or_else(|| invalid(format!("{name} må være et tall")))
    };
    for (k, v) in o {
        match k.as_str() {
            "powerLimitW" => {
                out.power_limit_w = clamp_num(
                    "powerLimitW",
                    num(v, "Effektgrensen")?,
                    power,
                    power,
                    &mut cl,
                )
            }
            "tempTargetC" => {
                out.temp_target_c = clamp_num(
                    "tempTargetC",
                    num(v, "Temperaturmålet")?,
                    temperature,
                    temperature,
                    &mut cl,
                )
            }
            "clockOffsets" => {
                let c = v
                    .as_object()
                    .ok_or_else(|| invalid("clockOffsets må være et objekt"))?;
                for (ck, cv) in c {
                    match ck.as_str() {
                        "coreMhz" => {
                            out.core_offset_mhz = clamp_num(
                                "clockOffsets.coreMhz",
                                num(cv, "Kjerneklokke-offset")?,
                                limits.map_or(POLICY_CORE, |l| l.core),
                                core,
                                &mut cl,
                            )
                        }
                        "memMhz" => {
                            out.mem_offset_mhz = clamp_num(
                                "clockOffsets.memMhz",
                                num(cv, "Minneklokke-offset")?,
                                limits.map_or(POLICY_MEM, |l| l.memory),
                                memory,
                                &mut cl,
                            )
                        }
                        other => {
                            return Err(invalid(format!(
                                "Ukjent felt: clockOffsets.{}",
                                other.chars().take(40).collect::<String>()
                            )));
                        }
                    }
                }
            }
            "fan" => {
                let f = v
                    .as_object()
                    .ok_or_else(|| invalid("fan må være et objekt"))?;
                for (fk, fv) in f {
                    match fk.as_str() {
                        "mode" => {
                            out.fan_mode =
                                fv.as_str().and_then(FanMode::parse).ok_or_else(|| {
                                    invalid("fan.mode må være auto, curve eller manual")
                                })?
                        }
                        "manualPercent" => {
                            out.fan_percent = clamp_num(
                                "fan.manualPercent",
                                num(fv, "Viftefarten")?,
                                fan,
                                fan,
                                &mut cl,
                            )
                        }
                        "curve" => {
                            let (pts, lifted) = check_curve_in(fv, fan)?;
                            if lifted {
                                cl.push(Clamped {
                                    field: "fan.curve".into(),
                                    requested: 0.0,
                                    applied: 0.0,
                                    bound: "min",
                                    source: "hardware",
                                });
                            }
                            out.fan_curve = pts;
                        }
                        "perFan" => {
                            out.fan_per_fan = fv
                                .as_bool()
                                .ok_or_else(|| invalid("fan.perFan må være true eller false"))?
                        }
                        "curves" => {
                            let (list, lifted) = check_curves_in(fv, fan)?;
                            if lifted {
                                cl.push(Clamped {
                                    field: "fan.curves".into(),
                                    requested: 0.0,
                                    applied: 0.0,
                                    bound: "min",
                                    source: "hardware",
                                });
                            }
                            out.fan_curves = list;
                        }
                        "hysteresisC" => {
                            out.fan_hysteresis_c = clamp_num(
                                "fan.hysteresisC",
                                num(fv, "Hysteresen")?,
                                crate::vifte::HYSTERESE,
                                crate::vifte::HYSTERESE,
                                &mut cl,
                            )
                        }
                        "rampUpPctS" => {
                            out.fan_ramp_up_pct_s = clamp_num(
                                "fan.rampUpPctS",
                                num(fv, "Rampefarten opp")?,
                                crate::vifte::RAMPE,
                                crate::vifte::RAMPE,
                                &mut cl,
                            )
                        }
                        "rampDownPctS" => {
                            out.fan_ramp_down_pct_s = clamp_num(
                                "fan.rampDownPctS",
                                num(fv, "Rampefarten ned")?,
                                crate::vifte::RAMPE,
                                crate::vifte::RAMPE,
                                &mut cl,
                            )
                        }
                        "smoothingS" => {
                            out.fan_smoothing_s = clamp_num(
                                "fan.smoothingS",
                                num(fv, "Utjevningen")?,
                                crate::vifte::UTJEVNING,
                                crate::vifte::UTJEVNING,
                                &mut cl,
                            )
                        }
                        "quietUntilC" => {
                            out.fan_quiet_until_c = match fv {
                                Value::Null => None,
                                v => Some(clamp_num(
                                    "fan.quietUntilC",
                                    num(v, "«Hold minimum til»")?,
                                    crate::vifte::STILLE,
                                    crate::vifte::STILLE,
                                    &mut cl,
                                )),
                            }
                        }
                        other => {
                            return Err(invalid(format!(
                                "Ukjent felt: fan.{}",
                                other.chars().take(40).collect::<String>()
                            )));
                        }
                    }
                }
            }
            other => {
                return Err(invalid(format!(
                    "Ukjent felt: {}",
                    other.chars().take(40).collect::<String>()
                )));
            }
        }
    }
    Ok((out, cl))
}

// ---------------------------------------------------------------- boka

struct Pending {
    txn: Txn,
    clamped: Vec<Clamped>,
}

/// En hendelse tjenesten sender til alle (`txn`).
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub txn: String,
    pub status: TxnStatus,
    /// confirm, revert, timeout, client_disconnected, reset.
    pub reason: &'static str,
}

impl Event {
    pub fn to_json(&self) -> Value {
        json!({"txn": self.txn, "status": self.status, "reason": self.reason})
    }
}

/// Tilstanden for ett simulert kort: bekreftet oppsett, det som er i bruk,
/// og en eventuell ventende endring.
pub struct Book {
    pub confirmed: Values,
    pub active: Values,
    pending: Option<Pending>,
    seq: u64,
    /// Standard frist (fra `confirmSec` i tjeneste.json).
    pub confirm_s: u32,
    /// Lysvalget (`rgb_set`), bare lagret.
    pub rgb: Value,
    /// Nødregelen gjelder nå (satt av viftesløyfen i tjenesten, runde 7B).
    pub emergency: bool,
    events: Vec<Event>,
}

impl Default for Book {
    fn default() -> Self {
        Book::new()
    }
}

impl Book {
    pub fn new() -> Book {
        Book {
            confirmed: Values::default(),
            active: Values::default(),
            pending: None,
            seq: 0,
            confirm_s: CONFIRM_DEFAULT_S,
            rgb: json!({"effect": "static", "color": "#3a7bf0", "brightness": 80}),
            emergency: false,
            events: Vec::new(),
        }
    }

    pub fn pending(&self) -> Option<&Txn> {
        self.pending.as_ref().map(|p| &p.txn)
    }

    /// Hendelsene siden sist (tjenesten sender dem som `txn`).
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn timeout_for(&self, v: Option<&Value>) -> Result<u32, ProtoError> {
        match v {
            None | Some(Value::Null) => Ok(self.confirm_s.clamp(CONFIRM_MIN_S, CONFIRM_MAX_S)),
            Some(x) => {
                let n = x
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| invalid("confirmTimeoutS må være et tall"))?;
                Ok((n.round() as i64).clamp(CONFIRM_MIN_S as i64, CONFIRM_MAX_S as i64) as u32)
            }
        }
    }

    fn pending_view(&self, now: i64) -> Value {
        match &self.pending {
            None => Value::Null,
            Some(p) => json!({
                "txn": p.txn.id,
                "owner": p.txn.owner,
                "timeoutS": p.txn.timeout_s,
                "deadline": p.txn.deadline,
                "remainingMs": (p.txn.deadline - now).max(0),
                "changed": self.confirmed.diff(&self.active),
                "clamped": p.clamped,
            }),
        }
    }

    pub fn state_view(&self, now: i64) -> Value {
        json!({
            "active": self.active.to_json(),
            "confirmed": self.confirmed.to_json(),
            "pending": self.pending_view(now),
            "emergency": self.emergency,
        })
    }

    /// Forhåndsvalider uten å endre transaksjonen eller kreve en HW-skriving.
    pub fn validate_apply_with_limits(
        &self,
        owner: &str,
        patch: &Value,
        timeout: Option<&Value>,
        limits: &HardwareLimits,
    ) -> Result<bool, ProtoError> {
        if self.pending.as_ref().is_some_and(|p| p.txn.owner != owner) {
            return Err(ProtoError::new(ErrorCode::Busy));
        }
        self.timeout_for(timeout)?;
        let (next, clamped) = merge_in(&self.active, patch, Some(limits))?;
        if !clamped.is_empty() {
            return Err(invalid(
                "Oppsettet må være innenfor rapporterte grenser og viftekurven må være stigende.",
            ));
        }
        Ok(next != self.active || self.pending.is_some())
    }

    /// `apply`: ny eller forlenget txn. Uten faktisk endring gis ingen txn.
    pub fn apply(
        &mut self,
        owner: &str,
        patch: &Value,
        timeout: Option<&Value>,
        now: i64,
    ) -> OpResult {
        self.apply_with_limits(owner, patch, timeout, now, None)
    }

    /// Hjelperen har først validert støtte og kortets faktiske områder.
    pub fn apply_with_limits(
        &mut self,
        owner: &str,
        patch: &Value,
        timeout: Option<&Value>,
        now: i64,
        limits: Option<&HardwareLimits>,
    ) -> OpResult {
        if let Some(p) = &self.pending
            && p.txn.owner != owner
        {
            return Err(ProtoError::new(ErrorCode::Busy).details(
                json!({"owner": p.txn.owner, "remainingMs": (p.txn.deadline - now).max(0)}),
            ));
        }
        let timeout_s = self.timeout_for(timeout)?;
        let (next, clamped) = merge_in(&self.active, patch, limits)?;
        let changed = self.confirmed.diff(&next);
        if self.pending.is_none() && changed.is_empty() {
            return Ok(
                json!({"txn": null, "pending": null, "clamped": clamped, "changed": [], "active": self.active.to_json(), "confirmed": self.confirmed.to_json()}),
            );
        }
        self.active = next;
        if self.pending.is_none() {
            self.seq += 1;
            self.pending = Some(Pending {
                txn: Txn {
                    id: format!("t{}", self.seq),
                    owner: owner.into(),
                    timeout_s,
                    deadline: 0,
                    changed: vec![],
                    status: TxnStatus::Pending,
                },
                clamped: vec![],
            });
        }
        let p = self.pending.as_mut().expect("ventende endring");
        p.txn.timeout_s = timeout_s;
        p.txn.deadline = now + timeout_s as i64 * 1000;
        p.txn.changed = changed.clone();
        // Klemminger gjelder til feltet skrives på nytt i samme txn.
        let touched: Vec<String> = leaf_paths(patch, "");
        p.clamped.retain(|c| !touched.contains(&c.field));
        p.clamped.extend(clamped.iter().cloned());
        let id = p.txn.id.clone();
        Ok(
            json!({"txn": id, "pending": self.pending_view(now), "clamped": clamped, "changed": changed, "active": self.active.to_json(), "confirmed": self.confirmed.to_json()}),
        )
    }

    fn check(&self, owner: &str, txn: Option<&str>) -> Result<(), ProtoError> {
        let p = self
            .pending
            .as_ref()
            .ok_or_else(|| ProtoError::new(ErrorCode::NoPending))?;
        if txn != Some(p.txn.id.as_str()) {
            return Err(
                ProtoError::new(ErrorCode::TxnMismatch).details(json!({"pending": p.txn.id}))
            );
        }
        if p.txn.owner != owner {
            return Err(ProtoError::new(ErrorCode::NotOwner));
        }
        Ok(())
    }

    pub fn confirm(&mut self, owner: &str, txn: Option<&str>, now: i64) -> OpResult {
        self.check(owner, txn)?;
        let p = self.pending.take().expect("ventende endring");
        self.confirmed = self.active.clone();
        self.events.push(Event {
            txn: p.txn.id,
            status: TxnStatus::Confirmed,
            reason: "confirm",
        });
        Ok(self.state_view(now))
    }

    pub fn revert(&mut self, owner: &str, txn: Option<&str>, now: i64) -> OpResult {
        self.check(owner, txn)?;
        self.rollback("revert");
        Ok(self.state_view(now))
    }

    /// Tilbake til siste bekreftede oppsett. Gir `true` hvis noe ventet.
    pub fn rollback(&mut self, reason: &'static str) -> bool {
        let Some(p) = self.pending.take() else {
            return false;
        };
        self.active = self.confirmed.clone();
        self.events.push(Event {
            txn: p.txn.id,
            status: TxnStatus::RolledBack,
            reason,
        });
        true
    }

    /// Hver runde: fristen som har gått ut, rulles tilbake.
    pub fn tick(&mut self, now: i64) -> bool {
        match &self.pending {
            Some(p) if now >= p.txn.deadline => self.rollback("timeout"),
            _ => false,
        }
    }

    /// En klient koblet fra: det den ikke har bekreftet, rulles tilbake straks.
    pub fn disconnect(&mut self, owner: &str) -> bool {
        match &self.pending {
            Some(p) if p.txn.owner == owner => self.rollback("client_disconnected"),
            _ => false,
        }
    }

    /// Tilbake til kortets standard. Trygg retning, så den bekreftes straks.
    pub fn reset(&mut self, owner: &str, now: i64) -> OpResult {
        if let Some(p) = &self.pending
            && p.txn.owner != owner
        {
            return Err(ProtoError::new(ErrorCode::Busy).details(json!({"owner": p.txn.owner})));
        }
        self.rollback("reset");
        self.confirmed = Values::default();
        self.active = Values::default();
        Ok(self.state_view(now))
    }

    /// Skriveoperasjonene. **Bare `backend == "sim"` kommer forbi første
    /// linje.** Med `nvml` og `ingen` svarer alt `write_locked`, også
    /// operasjoner som ellers ville vært ugyldige.
    pub fn handle(
        &mut self,
        backend: &str,
        op: &str,
        args: &Value,
        owner: &str,
        now: i64,
    ) -> OpResult {
        if backend != "sim" {
            return Err(ProtoError::new(ErrorCode::WriteLocked));
        }
        let txn = args.get("txn").and_then(Value::as_str);
        let num = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
        match op {
            "apply" => self.apply(
                owner,
                args.get("settings").unwrap_or(&Value::Null),
                args.get("confirmTimeoutS"),
                now,
            ),
            "set_power_limit" => self.apply(
                owner,
                &json!({"powerLimitW": num("watts")}),
                args.get("confirmTimeoutS"),
                now,
            ),
            "set_clock_offsets" => {
                let mut c = serde_json::Map::new();
                for k in ["coreMhz", "memMhz"] {
                    if let Some(v) = args.get(k) {
                        c.insert(k.into(), v.clone());
                    }
                }
                self.apply(
                    owner,
                    &json!({"clockOffsets": c}),
                    args.get("confirmTimeoutS"),
                    now,
                )
            }
            "set_temp_target" => self.apply(
                owner,
                &json!({"tempTargetC": num("celsius")}),
                args.get("confirmTimeoutS"),
                now,
            ),
            "set_fan" => {
                let mut f = serde_json::Map::new();
                for k in [
                    "mode",
                    "manualPercent",
                    "curve",
                    "perFan",
                    "curves",
                    "hysteresisC",
                    "rampUpPctS",
                    "rampDownPctS",
                    "smoothingS",
                    "quietUntilC",
                ] {
                    if let Some(v) = args.get(k) {
                        f.insert(k.into(), v.clone());
                    }
                }
                self.apply(owner, &json!({"fan": f}), args.get("confirmTimeoutS"), now)
            }
            "confirm" => self.confirm(owner, txn, now),
            "revert" => self.revert(owner, txn, now),
            "reset" => self.reset(owner, now),
            "rgb_get" => Ok(json!({"rgb": self.rgb, "controller": {"state": "not_connected"}})),
            "rgb_set" => {
                let v = args
                    .get("rgb")
                    .filter(|v| v.is_object())
                    .ok_or_else(|| invalid("rgb må være et objekt"))?;
                self.rgb = v.clone();
                Ok(json!({"rgb": self.rgb, "controller": {"state": "not_connected"}}))
            }
            _ => Err(ProtoError::new(ErrorCode::UnknownOp)),
        }
    }
}

/// Viftefarten kurven gir ved en temperatur: rett linje mellom punktene,
/// flat før første og etter siste (som `daemon-mock/card.js`).
pub fn curve_at(curve: &[(f64, f64)], t: f64) -> f64 {
    let Some(first) = curve.first() else {
        return HW_FAN.min;
    };
    if t <= first.0 {
        return first.1;
    }
    for w in curve.windows(2) {
        let ((t0, p0), (t1, p1)) = (w[0], w[1]);
        if t <= t1 {
            return if t1 > t0 {
                p0 + (p1 - p0) * (t - t0) / (t1 - t0)
            } else {
                p1
            };
        }
    }
    curve.last().map_or(HW_FAN.min, |p| p.1)
}

/// Alle bladstiene i et objekt (`{fan: {curve}}` → `["fan.curve"]`).
fn leaf_paths(v: &Value, at: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(o) = v.as_object() {
        for (k, x) in o {
            let p = if at.is_empty() {
                k.clone()
            } else {
                format!("{at}.{k}")
            };
            if x.is_object() {
                out.extend(leaf_paths(x, &p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- vinduet

/// Kan vinduet sende skriveoperasjoner nå? Svaret avgjør om Bruk er aktiv.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Kortet er ekte (eller ukjent): alt er låst. Teksten er verktøytipset.
    Locked(&'static str),
    /// Den simulerte backenden i en utviklerforekomst: flyten kan prøves.
    Sim,
    /// Identifisert NVML-kort og globalt autorisert, klar hjelper.
    Real,
}

/// Verktøytipset på Bruk. Gjelder både med og uten bryteren i 1.2.0.
pub const LOCKED_TIP: &str =
    "Aktiver skjermkortstyring i Innstillinger, og vent til hjelperen er klar";

/// `hello` fra tjenesten og om vinduet er en utviklerforekomst. Bare
/// `backend: "sim"` **og** `capabilities.write.state: "sim"` **og** `dev`
/// gir [`Gate::Sim`]. Alt annet, også et `hello` som mangler, er låst.
pub fn write_gate(hello: Option<&Value>, dev: bool) -> Gate {
    let Some(h) = hello else {
        return Gate::Locked(LOCKED_TIP);
    };
    let backend = h["daemon"]["backend"].as_str().unwrap_or("");
    let state = h["capabilities"]["write"]["state"].as_str().unwrap_or("");
    if backend == "sim" && state == "sim" && dev {
        Gate::Sim
    } else if backend == "nvml"
        && state == "ready"
        && h["capabilities"]["write"]["helper"]["enabled"] == true
        && h["capabilities"]["write"]["helper"]["ready"] == true
        && h["capabilities"]["write"]["helper"]["realWrites"] == true
    {
        Gate::Real
    } else {
        Gate::Locked(LOCKED_TIP)
    }
}

/// Trinnene i «Hva skal til?» fra `capabilities.write.steps`, med teksten
/// produktspesifikasjonen 6.2 gir. Mangler lista, vises alle seks som ikke
/// godkjent (det er sant så lenge tjenesten ikke sier noe annet).
pub fn steps(hello: Option<&Value>) -> Vec<(&'static str, &'static str, String)> {
    let names = [
        ("power", crate::tr!("Effektgrense"), ""),
        ("clocks", crate::tr!("Klokke-offset (kjerne og minne)"), ""),
        ("fan_manual", crate::tr!("Vifte, manuell fart"), ""),
        ("fan_curve", crate::tr!("Viftekurve"), ""),
        (
            "temp_target",
            crate::tr!("Temperaturmål"),
            crate::tr!(" (ikke verifisert på GeForce)"),
        ),
        (
            "rgb",
            crate::tr!("RGB-lys (Palit-kontrolleren på I2C)"),
            crate::tr!(" (kontrolleren er ikke bekreftet)"),
        ),
    ];
    let list = hello.and_then(|h| h["capabilities"]["write"]["steps"].as_array());
    names
        .iter()
        .map(|(id, name, note)| {
            let st = list
                .and_then(|l| l.iter().find(|s| s["id"] == *id))
                .and_then(|s| s["status"].as_str())
                .unwrap_or("not_approved");
            let text = match st {
                "supported" => crate::tr!("Tilgjengelig").to_string(),
                "unavailable" => crate::trf!("Ikke tilgjengelig{note}", note = note),
                "approved" => crate::tr!("Godkjent (eldre protokoll)").to_string(),
                _ => crate::trf!("Ikke godkjent{note}", note = note),
            };
            (*id, *name, text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visningen_gjenkjenner_eldre_dynamiske_kurvefeil() {
        let error = check_curves(&json!([[[40, 20], [50, "feil"]]])).unwrap_err();
        assert_eq!(error.message, "Vifte 1: Ugyldig punkt 2 i viftekurven");
        assert_eq!(display_validation_details(&error.message).as_deref(), Some(error.message.as_str()));
        assert!(display_validation_details("Vifte 1: ukjent feilmelding").is_none());
        assert!(display_validation_details("Ugyldig punkt x i viftekurven").is_none());
    }

    #[test]
    fn ekte_gate_krever_global_tillatelse_og_faktisk_klar_hjelper() {
        let mut h = json!({"daemon":{"backend":"nvml"},"capabilities":{"write":{"state":"ready","helper":{"enabled":true,"ready":true,"realWrites":true}}}});
        assert_eq!(write_gate(Some(&h), false), Gate::Real);
        h["capabilities"]["write"]["helper"]["ready"] = json!(false);
        assert!(matches!(write_gate(Some(&h), false), Gate::Locked(_)));
    }

    const T0: i64 = 1_000_000;

    #[test]
    fn kurven_leses_av_mellom_punktene() {
        let c = DEFAULT_CURVE.to_vec();
        assert_eq!(curve_at(&c, 20.0), 35.0);
        assert_eq!(curve_at(&c, 50.0), 40.0);
        assert_eq!(curve_at(&c, 56.0), 45.0);
        assert_eq!(curve_at(&c, 95.0), 100.0);
        assert_eq!(curve_at(&[], 60.0), HW_FAN.min);
    }

    #[test]
    fn bruk_gir_txn_som_venter_og_kan_beholdes() {
        let mut b = Book::new();
        let r = b
            .apply("s1", &json!({"powerLimitW": 200}), None, T0)
            .unwrap();
        assert_eq!(r["txn"], "t1");
        assert_eq!(r["changed"], json!(["powerLimitW"]));
        assert_eq!(b.active.power_limit_w, 200.0);
        assert_eq!(b.confirmed.power_limit_w, 245.0);
        assert_eq!(b.pending().unwrap().deadline, T0 + 15_000);
        let s = b.confirm("s1", Some("t1"), T0 + 2000).unwrap();
        assert_eq!(s["confirmed"]["powerLimitW"], 200.0);
        assert!(b.pending().is_none());
        assert_eq!(
            b.take_events(),
            vec![Event {
                txn: "t1".into(),
                status: TxnStatus::Confirmed,
                reason: "confirm"
            }]
        );
    }

    #[test]
    fn fristen_ruller_tilbake() {
        let mut b = Book::new();
        b.apply("s1", &json!({"tempTargetC": 75}), Some(&json!(10)), T0)
            .unwrap();
        assert!(!b.tick(T0 + 9_999));
        assert!(b.tick(T0 + 10_000));
        assert_eq!(b.active.temp_target_c, 83.0);
        assert_eq!(b.take_events()[0].reason, "timeout");
        assert_eq!(
            b.confirm("s1", Some("t1"), T0 + 11_000).unwrap_err().code,
            ErrorCode::NoPending
        );
    }

    #[test]
    fn fristen_klemmes_til_10_60_s() {
        let mut b = Book::new();
        b.apply("s1", &json!({"powerLimitW": 200}), Some(&json!(2)), T0)
            .unwrap();
        assert_eq!(b.pending().unwrap().timeout_s, 10);
        b.apply("s1", &json!({"powerLimitW": 210}), Some(&json!(600)), T0)
            .unwrap();
        assert_eq!(b.pending().unwrap().timeout_s, 60);
        assert_eq!(
            b.apply("s1", &json!({}), Some(&json!("lenge")), T0)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn bare_eieren_kan_beholde_og_bare_en_venter() {
        let mut b = Book::new();
        b.apply("s1", &json!({"powerLimitW": 200}), None, T0)
            .unwrap();
        assert_eq!(
            b.apply("s2", &json!({"powerLimitW": 220}), None, T0)
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );
        assert_eq!(
            b.confirm("s2", Some("t1"), T0).unwrap_err().code,
            ErrorCode::NotOwner
        );
        assert_eq!(
            b.revert("s1", Some("t9"), T0).unwrap_err().code,
            ErrorCode::TxnMismatch
        );
        assert_eq!(b.reset("s2", T0).unwrap_err().code, ErrorCode::Busy);
        // Eieren kan bruke mer i samme txn; fristen starter på nytt.
        let r = b
            .apply(
                "s1",
                &json!({"clockOffsets": {"coreMhz": 100}}),
                None,
                T0 + 5000,
            )
            .unwrap();
        assert_eq!(r["txn"], "t1");
        assert_eq!(b.pending().unwrap().deadline, T0 + 20_000);
        assert_eq!(r["changed"], json!(["powerLimitW", "clockOffsets.coreMhz"]));
        b.revert("s1", Some("t1"), T0 + 6000).unwrap();
        assert_eq!(b.active, Values::default());
    }

    #[test]
    fn frakobling_ruller_tilbake() {
        let mut b = Book::new();
        b.apply(
            "s1",
            &json!({"fan": {"mode": "manual", "manualPercent": 60}}),
            None,
            T0,
        )
        .unwrap();
        assert!(!b.disconnect("s2"));
        assert!(b.disconnect("s1"));
        assert_eq!(b.active.fan_mode, FanMode::Auto);
        assert_eq!(b.take_events()[0].reason, "client_disconnected");
    }

    #[test]
    fn uten_endring_ingen_txn() {
        let mut b = Book::new();
        let r = b
            .apply("s1", &json!({"powerLimitW": 245}), None, T0)
            .unwrap();
        assert!(r["txn"].is_null());
        assert!(b.pending().is_none());
    }

    #[test]
    fn klemming_etter_kort_og_policy() {
        let mut b = Book::new();
        let r = b
            .apply(
                "s1",
                &json!({"powerLimitW": 300, "clockOffsets": {"coreMhz": 900, "memMhz": -2000}}),
                None,
                T0,
            )
            .unwrap();
        let cl = r["clamped"].as_array().unwrap();
        let by = |f: &str| cl.iter().find(|c| c["field"] == f).unwrap().clone();
        assert_eq!(by("powerLimitW")["applied"], 275.0);
        assert_eq!(by("powerLimitW")["source"], "hardware");
        assert_eq!(by("clockOffsets.coreMhz")["applied"], 300.0);
        assert_eq!(by("clockOffsets.coreMhz")["source"], "policy");
        assert_eq!(by("clockOffsets.memMhz")["applied"], -1000.0);
        assert_eq!(by("clockOffsets.memMhz")["source"], "hardware");
        assert_eq!(b.active.power_limit_w, 275.0);
    }

    #[test]
    fn ugyldige_felt_avvises() {
        let mut b = Book::new();
        for bad in [
            json!({"tull": 1}),
            json!({"powerLimitW": "mye"}),
            json!({"fan": {"mode": "turbo"}}),
            json!({"clockOffsets": {"voltMv": 1}}),
            json!(3),
        ] {
            assert_eq!(
                b.apply("s1", &bad, None, T0).unwrap_err().code,
                ErrorCode::InvalidArgument,
                "{bad}"
            );
        }
        assert!(b.pending().is_none());
    }

    #[test]
    fn kurven_kontrolleres_og_loftes() {
        let (pts, lifted) = check_curve(&json!([[30, 20], [50, 60], [70, 40]])).unwrap();
        assert_eq!(pts, vec![(30.0, 35.0), (50.0, 60.0), (70.0, 60.0)]);
        assert!(lifted);
        assert!(check_curve(&json!([[30, 40]])).is_err());
        assert!(check_curve(&json!([[50, 40], [40, 50]])).is_err());
        assert!(check_curve(&json!([[30, 40], [200, 50]])).is_err());
    }

    #[test]
    fn tilbakestill_bekreftes_straks() {
        let mut b = Book::new();
        b.apply("s1", &json!({"powerLimitW": 150}), None, T0)
            .unwrap();
        b.confirm("s1", Some("t1"), T0).unwrap();
        b.apply("s1", &json!({"tempTargetC": 70}), None, T0)
            .unwrap();
        let s = b.reset("s1", T0).unwrap();
        assert!(s["pending"].is_null());
        assert_eq!(b.confirmed, Values::default());
    }

    /// Kjernen i låsen: med det ekte kortet (og uten kort) svarer hver
    /// skriveoperasjon `write_locked`, og ingenting i boka endres.
    #[test]
    fn skriving_er_umulig_mot_ekte_kort() {
        for backend in ["nvml", "ingen", "", "NVML", "sim "] {
            let mut b = Book::new();
            for op in crate::proto::WRITE_OPS {
                let args = json!({"settings": {"powerLimitW": 150}, "watts": 150, "coreMhz": 100, "celsius": 70, "mode": "manual", "txn": "t1", "rgb": {}});
                let e = b.handle(backend, op, &args, "s1", T0).unwrap_err();
                assert_eq!(e.code, ErrorCode::WriteLocked, "{backend} {op}");
            }
            assert_eq!(b.active, Values::default());
            assert!(b.pending().is_none());
            assert!(b.take_events().is_empty());
        }
    }

    #[test]
    fn sim_tar_imot_alle_skriveoperasjonene() {
        let mut b = Book::new();
        let r = b
            .handle("sim", "set_power_limit", &json!({"watts": 200}), "s1", T0)
            .unwrap();
        let txn = r["txn"].as_str().unwrap().to_string();
        b.handle(
            "sim",
            "set_clock_offsets",
            &json!({"coreMhz": 50}),
            "s1",
            T0,
        )
        .unwrap();
        b.handle("sim", "set_temp_target", &json!({"celsius": 80}), "s1", T0)
            .unwrap();
        b.handle("sim", "set_fan", &json!({"mode": "curve"}), "s1", T0)
            .unwrap();
        b.handle("sim", "confirm", &json!({"txn": txn}), "s1", T0)
            .unwrap();
        assert_eq!(b.confirmed.core_offset_mhz, 50.0);
        assert_eq!(b.confirmed.fan_mode, FanMode::Curve);
        b.handle(
            "sim",
            "rgb_set",
            &json!({"rgb": {"effect": "rainbow"}}),
            "s1",
            T0,
        )
        .unwrap();
        assert_eq!(
            b.handle("sim", "rgb_get", &json!({}), "s1", T0).unwrap()["rgb"]["effect"],
            "rainbow"
        );
        assert_eq!(
            b.handle("sim", "tull", &json!({}), "s1", T0)
                .unwrap_err()
                .code,
            ErrorCode::UnknownOp
        );
    }

    #[test]
    fn vinduets_port_er_lukket_uten_sim_og_dev() {
        let sim =
            json!({"daemon": {"backend": "sim"}, "capabilities": {"write": {"state": "sim"}}});
        let nvml =
            json!({"daemon": {"backend": "nvml"}, "capabilities": {"write": {"state": "locked"}}});
        let lie =
            json!({"daemon": {"backend": "nvml"}, "capabilities": {"write": {"state": "sim"}}});
        assert_eq!(write_gate(Some(&sim), true), Gate::Sim);
        assert!(matches!(write_gate(Some(&sim), false), Gate::Locked(_)));
        assert!(matches!(write_gate(Some(&nvml), true), Gate::Locked(_)));
        assert!(matches!(write_gate(Some(&lie), true), Gate::Locked(_)));
        assert!(matches!(write_gate(None, true), Gate::Locked(_)));
    }

    // ---------------------------------------------------------------- viftefeltene (runde 7B)

    /// Et utkast lagret av 1.1.0 (`profiler.json`), uten de nye feltene.
    const UTKAST_1_1: &str = r#"{"powerLimitW":220.0,"coreOffsetMhz":50.0,"memOffsetMhz":0.0,"tempTargetC":83.0,"fanMode":"curve","fanPercent":60.0,"fanCurve":[[40.0,35.0],[58.0,38.0],[68.0,46.0],[76.0,60.0],[85.0,82.0],[92.0,100.0]]}"#;

    #[test]
    fn gamle_utkast_og_profiler_leses_med_standardene() {
        let v: Values = serde_json::from_str(UTKAST_1_1).unwrap();
        assert_eq!(v.power_limit_w, 220.0);
        assert_eq!(v.fan_mode, FanMode::Curve);
        assert_eq!(v.fan_curve.len(), 6);
        let d = Values::default();
        assert!(!v.fan_per_fan && v.fan_curves.is_empty());
        assert_eq!(
            (
                v.fan_hysteresis_c,
                v.fan_ramp_up_pct_s,
                v.fan_ramp_down_pct_s,
                v.fan_smoothing_s,
                v.fan_quiet_until_c
            ),
            (
                d.fan_hysteresis_c,
                d.fan_ramp_up_pct_s,
                d.fan_ramp_down_pct_s,
                d.fan_smoothing_s,
                None
            )
        );
        // Et helt tomt objekt (og ukjente felt) gir standarden.
        assert_eq!(
            serde_json::from_str::<Values>(r#"{"framtid": 1}"#).unwrap(),
            Values::default()
        );
        // Og et nytt utkast leses tilbake likt.
        let ny = Values {
            fan_per_fan: true,
            fan_curves: vec![DEFAULT_CURVE.to_vec(), vec![(30.0, 40.0), (80.0, 100.0)]],
            fan_hysteresis_c: 5.0,
            fan_quiet_until_c: Some(50.0),
            fan_smoothing_s: 5.0,
            ..v.clone()
        };
        let tekst = serde_json::to_string(&ny).unwrap();
        assert!(tekst.contains("\"fanPerFan\":true") && tekst.contains("\"fanQuietUntilC\":50.0"));
        assert_eq!(serde_json::from_str::<Values>(&tekst).unwrap(), ny);
    }

    #[test]
    fn gammel_gpusettings_endrer_ikke_de_nye_feltene() {
        let base = Values {
            fan_hysteresis_c: 6.0,
            fan_quiet_until_c: Some(45.0),
            ..Default::default()
        };
        let (v, cl) = merge(
            &base,
            &json!({"fan": {"mode": "curve", "curve": [[30, 35], [80, 100]]}}),
        )
        .unwrap();
        assert!(cl.is_empty());
        assert_eq!(v.fan_hysteresis_c, 6.0);
        assert_eq!(v.fan_quiet_until_c, Some(45.0));
    }

    #[test]
    fn de_nye_viftefeltene_flettes_og_klemmes() {
        let patch = json!({"fan": {"perFan": true, "curves": [[[30, 35], [80, 100]], [[30, 20], [80, 90]]], "hysteresisC": 14, "rampUpPctS": 0, "rampDownPctS": 2.6, "smoothingS": 5, "quietUntilC": 20}});
        let (v, cl) = merge(&Values::default(), &patch).unwrap();
        assert!(v.fan_per_fan);
        assert_eq!(
            v.fan_curves[1],
            vec![(30.0, 35.0), (80.0, 90.0)],
            "løftet til minimum"
        );
        assert_eq!(
            (
                v.fan_hysteresis_c,
                v.fan_ramp_up_pct_s,
                v.fan_ramp_down_pct_s,
                v.fan_smoothing_s,
                v.fan_quiet_until_c
            ),
            (10.0, 1.0, 3.0, 5.0, Some(30.0))
        );
        let felt: Vec<&str> = cl.iter().map(|c| c.field.as_str()).collect();
        // (serde_json sorterer nøklene.)
        assert_eq!(
            felt,
            vec![
                "fan.curves",
                "fan.hysteresisC",
                "fan.quietUntilC",
                "fan.rampUpPctS"
            ]
        );
        // null slår av den stille flaten.
        let (v2, _) = merge(&v, &json!({"fan": {"quietUntilC": null}})).unwrap();
        assert_eq!(v2.fan_quiet_until_c, None);
        // diff har navnene fra protokollen.
        assert_eq!(
            Values::default().diff(&v),
            vec![
                "fan.perFan",
                "fan.curves",
                "fan.hysteresisC",
                "fan.rampUpPctS",
                "fan.smoothingS",
                "fan.quietUntilC"
            ]
        );
    }

    #[test]
    fn ugyldige_viftefelt_avvises() {
        for bad in [
            json!({"fan": {"perFan": "ja"}}),
            json!({"fan": {"curves": [[[30, 35]]]}}),
            json!({"fan": {"curves": [[[50, 35], [40, 50]]]}}),
            json!({"fan": {"curves": vec![json!([[30, 35], [80, 100]]); 9]}}),
            json!({"fan": {"curves": "kurve"}}),
            json!({"fan": {"hysteresisC": "mye"}}),
            json!({"fan": {"quietUntilC": "varmt"}}),
            json!({"fan": {"stopAtC": 40}}),
        ] {
            assert_eq!(
                merge(&Values::default(), &bad).unwrap_err().code,
                ErrorCode::InvalidArgument,
                "{bad}"
            );
        }
        let e = merge(
            &Values::default(),
            &json!({"fan": {"curves": [[[30, 35], [80, 100]], [[30, 35]]]}}),
        )
        .unwrap_err();
        assert!(e.message.starts_with("Vifte 2:"), "{}", e.message);
    }

    #[test]
    fn to_json_gaar_rundt_gjennom_merge() {
        // Vinduet sender hele utkastet (`to_json`) med `apply`: det må tas imot uendret.
        let v = Values {
            fan_mode: FanMode::Curve,
            fan_per_fan: true,
            fan_curves: vec![DEFAULT_CURVE.to_vec(), vec![(30.0, 45.0), (70.0, 80.0)]],
            fan_hysteresis_c: 4.0,
            fan_ramp_up_pct_s: 20.0,
            fan_ramp_down_pct_s: 5.0,
            fan_smoothing_s: 3.0,
            fan_quiet_until_c: Some(48.0),
            ..Default::default()
        };
        let (back, cl) = merge(&Values::default(), &v.to_json()).unwrap();
        assert!(cl.is_empty());
        assert_eq!(back, v);
        let (back, _) = merge(&v, &Values::default().to_json()).unwrap();
        assert_eq!(back, Values::default());
    }

    #[test]
    fn set_fan_tar_de_nye_feltene_og_kurven_per_vifte() {
        let mut b = Book::new();
        b.handle("sim", "set_fan", &json!({"mode": "curve", "perFan": true, "curves": [[[30, 35], [80, 100]], [[30, 50], [80, 100]]], "hysteresisC": 2}), "s1", T0).unwrap();
        assert!(b.active.fan_per_fan);
        assert_eq!(b.active.curve_for(1), &[(30.0, 50.0), (80.0, 100.0)][..]);
        assert_eq!(b.active.fan_hysteresis_c, 2.0);
        assert_eq!(
            b.pending().unwrap().changed,
            vec!["fan.mode", "fan.perFan", "fan.curves", "fan.hysteresisC"]
        );
        assert_eq!(b.state_view(T0)["emergency"], false);
        b.emergency = true;
        assert_eq!(b.state_view(T0)["emergency"], true);
    }

    #[test]
    fn trinnene_fra_tjenesten() {
        let h = json!({"capabilities": {"write": crate::proto::write_capabilities("nvml")}});
        let s = steps(Some(&h));
        assert_eq!(s.len(), 6);
        assert_eq!(s[0].1, "Effektgrense");
        assert_eq!(s[4].2, "Ikke tilgjengelig (ikke verifisert på GeForce)");
        assert!(s.iter().all(|x| x.2.starts_with("Ikke tilgjengelig")));
        let ok =
            json!({"capabilities": {"write": {"steps": [{"id": "power", "status": "approved"}]}}});
        assert_eq!(steps(Some(&ok))[0].2, "Godkjent (eldre protokoll)");
        assert_eq!(steps(None).len(), 6);
    }
}
