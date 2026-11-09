//! Det felles for root-hjelperen (`agentic-rigg-helper`), klienten i
//! `riggd` og Innstillinger › Styring (runde 7A, 1.2.0): trinnene,
//! `helper.json`, hvilke felt hvert trinn dekker, og stiene.
//!
//! **Tre lås mot det ekte kortet** (se `docs/produksjon/hjelper-sikkerhet.md`):
//!
//! 1. **Trinnlisten** i `/etc/agentic-rigg/helper.json` (eies av root). Tom ved
//!    aktivering. Et felt hvis trinn ikke står der, avvises av hjelperen med
//!    `step_not_enabled`. Bare root (via `pkexec` og polkit) kan endre lista.
//! 2. **Sperren i `rigg-hw`** (`gpu::skriv::EKTE_SKRIVING`, og cargo-funksjonen
//!    `ekte-skriving` som er av): uten den kan hjelperen ikke åpne NVML for
//!    skriving i det hele tatt, og svarer `write_locked` med grunnen
//!    `real_writes_off`. I 1.2.0 er begge av.
//! 3. **`write_gate` i vinduet** (`txn::write_gate`): Bruk er bare aktiv mot den
//!    simulerte backenden i en utviklerforekomst. Den åpnes for det ekte kortet
//!    først når et trinn er godkjent og sperren er av (trinn 2 i planen).
//!
//! `riggd` sender en skriveoperasjon videre bare når hjelperen svarer, er fra
//! samme bygg, har samme backend som tjenesten og har trinnet godkjent. Ellers
//! svarer den `write_locked` (eller `step_not_enabled`) med `details.reason`.

use crate::proto::{ErrorCode, ProtoError};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Protokollen mellom `riggd` og hjelperen (NDJSON, som `proto`).
pub const PROTOCOL: u32 = 1;
/// Største melding til hjelperen (byte per linje).
pub const MAX_LINE: usize = 16_384;
/// Høyst så mange samtidige forbindelser til hjelperen.
pub const MAX_CLIENTS: usize = 4;

/// Standardstiene på et aktivert system.
pub const CONFIG_PATH: &str = "/etc/agentic-rigg/helper.json";
pub const SOCKET_PATH: &str = "/run/agentic-rigg/helper.sock";
pub const BINARY_PATH: &str = "/usr/local/libexec/agentic-rigg-helper";
pub const LIBEXEC_DIR: &str = "/usr/local/libexec/agentic-rigg";

/// Operasjonene hjelperen kjenner. Ingen andre finnes.
pub const OPS: &[&str] = &["status", "apply", "confirm", "revert", "reset", "rgb_get", "rgb_set"];

/// Et trinn som brukeren godkjenner for seg (rekkefølgen i trinnplanen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    Effekt,
    Vifter,
    Klokker,
    Lys,
}

pub const STEPS: [Step; 4] = [Step::Effekt, Step::Vifter, Step::Klokker, Step::Lys];

impl Step {
    pub fn id(self) -> &'static str {
        match self {
            Step::Effekt => "effekt",
            Step::Vifter => "vifter",
            Step::Klokker => "klokker",
            Step::Lys => "lys",
        }
    }

    pub fn parse(s: &str) -> Option<Step> {
        STEPS.iter().copied().find(|x| x.id() == s)
    }

    /// Navnet i brukerflaten.
    pub fn name(self) -> &'static str {
        match self {
            Step::Effekt => "Effekt",
            Step::Vifter => "Vifter",
            Step::Klokker => "Klokker",
            Step::Lys => "RGB-lys",
        }
    }

    /// Hva trinnet dekker, kort.
    pub fn covers(self) -> &'static str {
        match self {
            Step::Effekt => "Effektgrense og temperaturmål",
            Step::Vifter => "Viftemodus, fast fart og viftekurve",
            Step::Klokker => "Kjerne- og minneklokke-offset",
            Step::Lys => "Fargene og effektene på kortet",
        }
    }

    /// Trinnene i `capabilities.write.steps` (produktspesifikasjonen 6.2)
    /// som dette trinnet godkjenner.
    pub fn write_steps(self) -> &'static [&'static str] {
        match self {
            Step::Effekt => &["power", "temp_target"],
            Step::Vifter => &["fan_manual", "fan_curve"],
            Step::Klokker => &["clocks"],
            Step::Lys => &["rgb"],
        }
    }
}

/// Trinnet et felt i `GpuSettings` hører til (`powerLimitW`, `fan`, …).
pub fn step_for_field(top: &str) -> Option<Step> {
    match top {
        "powerLimitW" | "tempTargetC" => Some(Step::Effekt),
        "clockOffsets" => Some(Step::Klokker),
        "fan" => Some(Step::Vifter),
        _ => None,
    }
}

/// Trinnene en delvis `GpuSettings` trenger. Ukjente felt gir feil (de
/// avvises uansett av `txn::merge`, men her før noe annet).
pub fn steps_for_patch(patch: &Value) -> Result<Vec<Step>, ProtoError> {
    let o = patch.as_object().ok_or_else(|| ProtoError::msg(ErrorCode::InvalidArgument, "settings må være et objekt"))?;
    let mut out = Vec::new();
    for k in o.keys() {
        let s = step_for_field(k).ok_or_else(|| ProtoError::msg(ErrorCode::InvalidArgument, format!("Ukjent felt: {}", k.chars().take(40).collect::<String>())))?;
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.sort();
    Ok(out)
}

/// Skriveoperasjonene i protokollen mellom vinduet og `riggd`, som en
/// delvis `GpuSettings` (`apply` til hjelperen). `None` for operasjoner som
/// ikke endrer verdier (`confirm`, `revert`, `reset`, `rgb_*`).
pub fn patch_for_op(op: &str, args: &Value) -> Option<Value> {
    let pick = |keys: &[&str]| {
        let mut m = serde_json::Map::new();
        for k in keys {
            if let Some(v) = args.get(*k) {
                m.insert((*k).to_string(), v.clone());
            }
        }
        Value::Object(m)
    };
    let get = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    match op {
        "apply" => Some(args.get("settings").cloned().unwrap_or(Value::Null)),
        "set_power_limit" => Some(json!({"powerLimitW": get("watts")})),
        "set_clock_offsets" => Some(json!({"clockOffsets": pick(&["coreMhz", "memMhz"])})),
        "set_temp_target" => Some(json!({"tempTargetC": get("celsius")})),
        "set_fan" => Some(json!({"fan": pick(&["mode", "manualPercent", "curve"])})),
        _ => None,
    }
}

// ---------------------------------------------------------------- helper.json

/// Innholdet i `helper.json`. Skrives av `aktiver.sh` (uid, tom trinnliste)
/// og `trinn.sh`; leses av hjelperen og `riggd` (fila er `0644 root:root`).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Brukeren som aktiverte hjelperen. Bare den får koble til.
    pub uid: u32,
    /// Godkjente trinn. Tom ved aktivering.
    pub steps: Vec<Step>,
    /// Bygget som ble aktivert (fra `--version` på binæren).
    pub build: u32,
    pub activated: String,
}

impl Config {
    /// Tolker fila strengt: ukjente felt og ukjente trinn er feil, så en
    /// skrivefeil aldri gir et trinn ingen har godkjent.
    pub fn parse(text: &str) -> Result<Config, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("helper.json er ikke gyldig JSON: {e}"))?;
        let o = v.as_object().ok_or("helper.json må være et objekt")?;
        for k in o.keys() {
            if !matches!(k.as_str(), "versjon" | "uid" | "trinn" | "bygg" | "aktivert") {
                return Err(format!("Ukjent felt i helper.json: {k}"));
            }
        }
        if o.get("versjon").and_then(Value::as_u64) != Some(1) {
            return Err("helper.json: versjon må være 1".into());
        }
        let uid = o.get("uid").and_then(Value::as_u64).filter(|u| *u > 0 && *u < u32::MAX as u64).ok_or("helper.json: uid mangler eller er 0 (root får ikke være klient)")? as u32;
        let list = o.get("trinn").and_then(Value::as_array).ok_or("helper.json: trinn må være en liste")?;
        let mut steps = Vec::new();
        for s in list {
            let s = s.as_str().and_then(Step::parse).ok_or_else(|| format!("helper.json: ukjent trinn {s}"))?;
            if !steps.contains(&s) {
                steps.push(s);
            }
        }
        steps.sort();
        let build = o.get("bygg").and_then(Value::as_u64).unwrap_or(0) as u32;
        let activated = o.get("aktivert").and_then(Value::as_str).unwrap_or("").chars().take(40).collect();
        Ok(Config { uid, steps, build, activated })
    }

    pub fn to_json(&self) -> Value {
        json!({"versjon": 1, "uid": self.uid, "trinn": self.steps.iter().map(|s| s.id()).collect::<Vec<_>>(), "bygg": self.build, "aktivert": self.activated})
    }

    pub fn enabled(&self, s: Step) -> bool {
        self.steps.contains(&s)
    }
}

/// Stiene til hjelperen. `root` er `/` på et ekte system, og en tempmappe i
/// testene og i utviklerforekomsten (`RIGG_HJELPER_ROT`, falsk pkexec).
#[derive(Clone, Debug, PartialEq)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Paths {
        Paths { root: root.into() }
    }
    fn at(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }
    pub fn config(&self) -> PathBuf {
        self.at(CONFIG_PATH)
    }
    pub fn socket(&self) -> PathBuf {
        self.at(SOCKET_PATH)
    }
    pub fn binary(&self) -> PathBuf {
        self.at(BINARY_PATH)
    }
    /// Skriptene aktiveringen legger i en mappe bare root kan skrive i.
    pub fn libexec(&self, name: &str) -> PathBuf {
        self.at(LIBEXEC_DIR).join(name)
    }
    pub fn is_real(&self) -> bool {
        self.root == Path::new("/")
    }
}

/// Feilen hjelperen og `riggd` gir når et trinn ikke er godkjent.
pub fn step_not_enabled(steps: &[Step]) -> ProtoError {
    let names: Vec<&str> = steps.iter().map(|s| s.name()).collect();
    let msg = match names.as_slice() {
        [one] => format!("Trinnet «{one}» er ikke godkjent. Godkjenn det i Innstillinger › Styring først."),
        _ => format!("Trinnene {} er ikke godkjent. Godkjenn dem i Innstillinger › Styring først.", names.join(", ")),
    };
    ProtoError::msg(ErrorCode::StepNotEnabled, msg).details(json!({"steps": steps.iter().map(|s| s.id()).collect::<Vec<_>>()}))
}

/// `write_locked` med en presis grunn (`details.reason`).
pub fn locked(reason: &str) -> ProtoError {
    let msg = match reason {
        "helper_off" => "Endringer er låst: endring av skjermkortet er ikke slått på. Slå på «Tillat endring av skjermkortet» i Innstillinger › Styring.",
        "helper_error" => "Hjelperen for endring svarer ikke. Se Innstillinger › Styring.",
        "helper_outdated" => "Hjelperen er fra en annen versjon av appen. Slå bryteren i Innstillinger › Styring av og på igjen.",
        "backend_mismatch" => "Hjelperen og tjenesten bruker ulike kort (simulert og ekte). Ingenting er sendt.",
        "real_writes_off" => "Denne versjonen skriver ikke til det ekte kortet ennå. Trinnene slås på i senere versjoner.",
        "rgb_not_implemented" => "Lys er ikke laget ennå.",
        _ => "Endringer er låst. Skriving til skjermkortet er ikke slått på.",
    };
    ProtoError::msg(ErrorCode::WriteLocked, msg).details(json!({"reason": reason}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_json_tolkes_strengt() {
        let c = Config::parse(r#"{"versjon":1,"uid":1000,"trinn":[],"bygg":18,"aktivert":"2026-11-09T10:00:00Z"}"#).unwrap();
        assert_eq!(c.uid, 1000);
        assert!(c.steps.is_empty());
        let c = Config::parse(r#"{"versjon":1,"uid":1000,"trinn":["vifter","effekt","effekt"]}"#).unwrap();
        assert_eq!(c.steps, vec![Step::Effekt, Step::Vifter]);
        for bad in [
            r#"{"versjon":1,"uid":0,"trinn":[]}"#,
            r#"{"versjon":1,"uid":1000,"trinn":["alt"]}"#,
            r#"{"versjon":2,"uid":1000,"trinn":[]}"#,
            r#"{"versjon":1,"uid":1000,"trinn":[],"sti":"/dev/i2c-0"}"#,
            r#"{"versjon":1,"uid":1000}"#,
            "[]",
            "tull",
        ] {
            assert!(Config::parse(bad).is_err(), "{bad}");
        }
        let c = Config::parse(r#"{"versjon":1,"uid":1000,"trinn":["klokker"]}"#).unwrap();
        assert_eq!(Config::parse(&c.to_json().to_string()).unwrap(), c);
    }

    #[test]
    fn feltene_har_hvert_sitt_trinn() {
        assert_eq!(steps_for_patch(&json!({"powerLimitW": 200, "tempTargetC": 80})).unwrap(), vec![Step::Effekt]);
        assert_eq!(steps_for_patch(&json!({"fan": {"mode": "manual"}, "clockOffsets": {}})).unwrap(), vec![Step::Vifter, Step::Klokker]);
        assert!(steps_for_patch(&json!({"voltMv": 900})).is_err());
        assert!(steps_for_patch(&json!(3)).is_err());
        assert_eq!(patch_for_op("set_power_limit", &json!({"watts": 200})).unwrap(), json!({"powerLimitW": 200}));
        assert_eq!(patch_for_op("set_fan", &json!({"mode": "auto", "tull": 1})).unwrap(), json!({"fan": {"mode": "auto"}}));
        assert!(patch_for_op("confirm", &json!({})).is_none());
    }

    #[test]
    fn stiene_under_en_falsk_rot() {
        let p = Paths::new("/tmp/x");
        assert_eq!(p.config(), Path::new("/tmp/x/etc/agentic-rigg/helper.json"));
        assert_eq!(p.socket(), Path::new("/tmp/x/run/agentic-rigg/helper.sock"));
        assert!(!p.is_real());
        assert_eq!(Paths::new("/").binary(), Path::new(BINARY_PATH));
        assert_eq!(step_not_enabled(&[Step::Effekt]).code, ErrorCode::StepNotEnabled);
        assert_eq!(locked("helper_off").details.unwrap()["reason"], "helper_off");
    }
}
