//! Det felles for root-hjelperen (`agentic-rigg-helper`), klienten i
//! `riggd` og Innstillinger › Styring: global autorisasjon, kortbinding,
//! `helper.json`, feltgruppene og stiene.
//!
//! **Låser mot det ekte kortet**:
//!
//! 1. Root-eid konfigurasjon v2 gir én global tillatelse for én PCI/UUID.
//!    Eldre trinnoppsett oppgraderes aldri stilltiende til global tillatelse.
//! 2. Bare hjelperen kompileres med HW-skriveren; oppdagelse leser kun.
//!    Hvert endret felt krever tilgjengelig funksjon og et lagret grunnopptak.
//! 3. `txn::write_gate` krever at hjelperen faktisk er klar og autorisert;
//!    vinduet kontrollerer også at valgt kort samsvarer med bindingen.
//!
//! `riggd` sender en skriveoperasjon videre bare når hjelperen svarer, er fra
//! kompatibelt bygg/protokoll, samme backend og samsvarende kortidentitet.
//! Ellers svarer den `write_locked` med `details.reason`. Trinn beholdes som
//! feltgrupper og for eldre simulerte tester, ikke som separate godkjenninger.

use crate::proto::{ErrorCode, ProtoError};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Protokollen mellom `riggd` og hjelperen (NDJSON, som `proto`).
pub const PROTOCOL: u32 = 3;
/// Gjennomgått sikkerhetsmotor. Øk ved sikkerhetsrettinger som eldre hjelpere
/// ikke må få omgå; rene UI-patcher kan beholde autorisasjonen fra dette bygget.
/// Bygg 34 bevarer nødgjenoppretting ved et generasjonsbytte.
pub const MIN_COMPATIBLE_BUILD: u32 = 34;

/// Bare kjente hjelpere innen samme protokoll godtas. En nyere hjelper enn
/// appen krever også oppdatering; ingen fremtidig kompatibilitet antas.
pub fn compatible(protocol: u32, build: u32) -> bool {
    protocol == PROTOCOL && (MIN_COMPATIBLE_BUILD..=crate::version::build()).contains(&build)
}
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
pub const OPS: &[&str] = &[
    "status", "apply", "confirm", "revert", "reset", "rgb_get", "rgb_set",
];

/// Eldre feltgrupper; produksjonen bruker én global autorisasjon.
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
    let o = patch
        .as_object()
        .ok_or_else(|| ProtoError::msg(ErrorCode::InvalidArgument, "settings må være et objekt"))?;
    let mut out = Vec::new();
    for k in o.keys() {
        let s = step_for_field(k).ok_or_else(|| {
            ProtoError::msg(
                ErrorCode::InvalidArgument,
                format!("Ukjent felt: {}", k.chars().take(40).collect::<String>()),
            )
        })?;
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

/// Valider identitet uten oppslag eller maskinvaretilgang.
pub fn valid_pci(pci: &str) -> bool {
    let b = pci.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && b.iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10) || c.is_ascii_hexdigit())
        && b[11] <= b'7'
}

pub fn valid_uuid(uuid: &str) -> bool {
    uuid.starts_with("GPU-")
        && (8..=80).contains(&uuid.len())
        && uuid.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

pub fn hardware_error(message: String) -> ProtoError {
    let prefix = message.split(':').next().unwrap_or("");
    let code = match prefix {
        "not_supported" => ErrorCode::NotSupported,
        "no_permission" => ErrorCode::PermissionDenied,
        "identity_mismatch" => ErrorCode::UnknownGpu,
        "gpu_lost" | "readback_mismatch" | "baseline_required" => ErrorCode::Unavailable,
        _ => ErrorCode::Internal,
    };
    ProtoError::msg(code, message.clone())
        .details(json!({"reason": prefix, "hardwareError": message}))
}

// ---------------------------------------------------------------- helper.json

/// Innholdet i `helper.json`. Skrives av `aktiver.sh` (uid, tom trinnliste)
/// og `trinn.sh`; leses av hjelperen og `riggd` (fila er `0644 root:root`).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Brukeren som aktiverte hjelperen. Bare den får koble til.
    pub uid: u32,
    pub enabled: bool,
    /// Versjon 1 beholdes kun for eldre sim-fixtures, aldri som NVML-tillatelse.
    pub legacy: bool,
    pub pci: Option<String>,
    pub uuid: Option<String>,
    /// Godkjente trinn. Tom ved aktivering.
    pub steps: Vec<Step>,
    /// Bygget som ble aktivert (fra `--version` på binæren).
    pub build: u32,
    pub activated: String,
    pub generation: Option<String>,
}

impl Config {
    /// Tolker fila strengt: ukjente felt og ukjente trinn er feil, så en
    /// skrivefeil aldri gir et trinn ingen har godkjent.
    pub fn parse(text: &str) -> Result<Config, String> {
        let v: Value = serde_json::from_str(text)
            .map_err(|e| format!("helper.json er ikke gyldig JSON: {e}"))?;
        let o = v.as_object().ok_or("helper.json må være et objekt")?;
        for k in o.keys() {
            if !matches!(
                k.as_str(),
                "versjon" | "uid" | "trinn" | "bygg" | "aktivert" | "enabled" | "pci" | "uuid" | "scope" | "generation"
            ) {
                return Err(format!("Ukjent felt i helper.json: {k}"));
            }
        }
        if (o.contains_key("scope") || o.contains_key("generation"))
            && (o.get("scope").and_then(Value::as_str) != Some("card") || !o.get("generation").and_then(Value::as_str)
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()))) {
            return Err("Ugyldig korttillatelse.".into());
        }
        let schema = o
            .get("versjon")
            .and_then(Value::as_u64)
            .ok_or("helper.json: versjon mangler")?;
        if !matches!(schema, 1 | 2) {
            return Err("helper.json: versjon må være 1 eller 2".into());
        }
        let legacy = schema == 1;
        let enabled = if legacy {
            false
        } else {
            o.get("enabled")
                .and_then(Value::as_bool)
                .ok_or("helper.json: enabled må være boolsk")?
        };
        let pci = o.get("pci").and_then(Value::as_str).map(str::to_string);
        let uuid = o.get("uuid").and_then(Value::as_str).map(str::to_string);
        if !legacy
            && (!pci.as_deref().is_some_and(valid_pci) || !uuid.as_deref().is_some_and(valid_uuid))
        {
            return Err("helper.json: gyldig pci og uuid kreves".into());
        }
        if !legacy && o.contains_key("trinn") {
            return Err("helper.json: trinn hører bare til eldre format".into());
        }
        let uid = o
            .get("uid")
            .and_then(Value::as_u64)
            .filter(|u| *u > 0 && *u < u32::MAX as u64)
            .ok_or("helper.json: uid mangler eller er 0 (root får ikke være klient)")?
            as u32;
        let empty = Vec::new();
        let list = if legacy {
            o.get("trinn")
                .and_then(Value::as_array)
                .ok_or("helper.json: trinn må være en liste")?
        } else {
            &empty
        };
        let mut steps = Vec::new();
        for s in list {
            let s = s
                .as_str()
                .and_then(Step::parse)
                .ok_or_else(|| format!("helper.json: ukjent trinn {s}"))?;
            if !steps.contains(&s) {
                steps.push(s);
            }
        }
        steps.sort();
        let build = o.get("bygg").and_then(Value::as_u64).unwrap_or(0) as u32;
        let activated = o
            .get("aktivert")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(40)
            .collect();
        Ok(Config {
            uid,
            enabled,
            legacy,
            pci,
            uuid,
            steps,
            build,
            activated,
            generation: o.get("generation").and_then(Value::as_str).map(str::to_owned),
        })
    }

    pub fn to_json(&self) -> Value {
        if self.legacy {
            json!({"versjon": 1, "uid": self.uid, "trinn": self.steps.iter().map(|s| s.id()).collect::<Vec<_>>(), "bygg": self.build, "aktivert": self.activated})
        } else {
            let mut value = json!({"versjon": 2, "uid": self.uid, "enabled": self.enabled, "pci": self.pci, "uuid": self.uuid, "bygg": self.build, "aktivert": self.activated});
            if let Some(id) = &self.generation {
                value["scope"] = json!("card");
                value["generation"] = json!(id);
            }
            value
        }
    }

    pub fn enabled(&self, s: Step) -> bool {
        // V2 gjelder bare NVML-styring. En gammel GPU-godkjenning skal
        // aldri bli RGB-tillatelse når en ny backend kobles til.
        self.enabled && s != Step::Lys || self.legacy && self.steps.contains(&s)
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
        [one] => format!(
            "Trinnet «{one}» er ikke godkjent. Godkjenn det i Innstillinger › Styring først."
        ),
        _ => format!(
            "Trinnene {} er ikke godkjent. Godkjenn dem i Innstillinger › Styring først.",
            names.join(", ")
        ),
    };
    ProtoError::msg(ErrorCode::StepNotEnabled, msg)
        .details(json!({"steps": steps.iter().map(|s| s.id()).collect::<Vec<_>>()}))
}

/// `write_locked` med en presis grunn (`details.reason`).
pub fn locked(reason: &str) -> ProtoError {
    ProtoError::msg(ErrorCode::WriteLocked, locked_message(reason))
        .details(json!({"reason": reason}))
}

fn locked_message(reason: &str) -> &'static str {
    match reason {
        "helper_off" => {
            "Endringer er låst: endring av skjermkortet er ikke slått på. Slå på «Tillat endring av skjermkortet» i Innstillinger › Styring."
        }
        "helper_error" => "Hjelperen for endring svarer ikke. Se Innstillinger › Styring.",
        "helper_outdated" => {
            "Hjelperen er fra en annen versjon av appen. Oppdater systemkomponentene fra Innstillinger › Skjermkortstyring."
        }
        "backend_mismatch" => {
            "Hjelperen og tjenesten bruker ulike kort (simulert og ekte). Ingenting er sendt."
        }
        "real_writes_off" => "NVML-hjelperen er ikke klar til å endre dette kortet.",
        "authorization_required" => "Skjermkortstyring må aktiveres med bryteren i Innstillinger.",
        "identity_mismatch" => "Det valgte kortet er ikke kortet hjelperen er aktivert for.",
        "recovery_failed" => "Gjenoppretting kunne ikke bekreftes. Nye endringer er sperret.",
        "rgb_not_implemented" => "Lys er ikke laget ennå.",
        _ => "Endringer er låst. Skriving til skjermkortet er ikke slått på.",
    }
}

/// Lokaliser visningen uten å endre feilmeldingen eller protokolldataene.
pub fn display_error(error: &ProtoError) -> Cow<'_, str> {
    if crate::i18n::active() == crate::i18n::Language::Nb {
        return Cow::Borrowed(&error.message);
    }
    let details = error.details.as_ref();
    let reason = details.and_then(|value| value["reason"].as_str());
    if error.code == ErrorCode::StepNotEnabled
        && let Some(steps) = details.and_then(|value| value["steps"].as_array())
        && let Some(names) = steps.iter().map(|step| Step::parse(step.as_str()?).map(|step| match step {
            Step::Effekt => crate::tr!("Effekt"),
            Step::Vifter => crate::tr!("Vifter"),
            Step::Klokker => crate::tr!("Klokker"),
            Step::Lys => crate::tr!("RGB-lys"),
        })).collect::<Option<Vec<_>>>()
        && !names.is_empty()
    {
        return Cow::Owned(match names.as_slice() {
            [one] => crate::trf!("Trinnet «{one}» er ikke godkjent. Godkjenn det i Innstillinger › Styring først.", one = one),
            _ => crate::trf!("Trinnene {steps} er ikke godkjent. Godkjenn dem i Innstillinger › Styring først.", steps = names.join(", ")),
        });
    }
    if error.code == ErrorCode::WriteLocked && let Some(reason) = reason {
        let original = locked_message(reason);
        let fallback = locked_message("");
        if original != fallback || error.message == fallback {
            return Cow::Borrowed(display_message(original));
        }
    }
    if reason == Some("out_of_range") && let Some(details) = details
        && let (Some(min), Some(max)) = (details["min"].as_f64(), details["max"].as_f64())
        && let Some((field, min, max)) = display_range(details["field"].as_str().unwrap_or(""), &error.message, min, max)
    {
        return Cow::Owned(crate::trf!("{felt}: Kortet tillater {min}–{max}. Ingenting er endret.", felt = field, min = min, max = max));
    }
    if error.code == ErrorCode::NotSupported && reason == Some("not_supported") && details.is_some_and(|d| d["field"].is_string()) {
        return Cow::Borrowed(crate::tr!("Kortet eller driveren støtter ikke denne funksjonen."));
    }
    display_diagnostic(&error.message)
}

fn display_range(field: &str, message: &str, min: f64, max: f64) -> Option<(&'static str, String, String)> {
    use crate::fmt as F;
    let (name, format): (&str, fn(f64) -> String) = match field {
        "powerLimitW" => (crate::tr!("Effektgrense"), |value| F::watt(Some(value), 0)),
        "tempTargetC" => (crate::tr!("Temperaturmål"), |value| F::temp(Some(value), 0)),
        "clockOffsets.coreMhz" => (crate::tr!("GPU-justering"), |value| F::mhz(Some(value))),
        "clockOffsets.memMhz" => (crate::tr!("Minnejustering"), |value| F::mhz(Some(value))),
        "fan.manualPercent" => (crate::tr!("Viftefart"), |value| F::pct(Some(value), 0)),
        "fan.curve" if message.starts_with("Temperaturen i kurven må være ") => (crate::tr!("Temperaturen i kurven"), |value| F::temp(Some(value), 0)),
        "fan.curve" if message.starts_with("Viftefarten i kurven må være ") => (crate::tr!("Viftefarten i kurven"), |value| F::pct(Some(value), 0)),
        _ => return None,
    };
    Some((name, format(min), format(max)))
}

/// Lokaliser også kjente meldingsmaler fra eldre hjelpere, uten å endre råteksten.
pub fn display_diagnostic(message: &str) -> Cow<'_, str> {
    if crate::i18n::active() == crate::i18n::Language::Nb {
        return Cow::Borrowed(message);
    }
    if let Some(translated) = crate::txn::display_validation_details(message) {
        return Cow::Owned(translated);
    }
    if let Some(cause) = message.strip_prefix("Kan ikke lagre gjenopprettingsmerkør før endring: ") {
        return Cow::Owned(crate::trf!("Kan ikke lagre gjenopprettingsmerkør før endring: {feil}", feil = cause));
    }
    if let Some(cause) = message.strip_prefix("Kortet ble gjenopprettet, men merkøren kunne ikke ryddes: ") {
        return Cow::Owned(crate::trf!("Kortet ble gjenopprettet, men merkøren kunne ikke ryddes: {feil}", feil = cause));
    }
    if let Some(cause) = message.strip_prefix("Fikk ikke kontakt med hjelperen (").and_then(|s| s.strip_suffix(')')) {
        return Cow::Owned(crate::trf!("Fikk ikke kontakt med hjelperen ({feil})", feil = cause));
    }
    if let Some(cause) = message.strip_prefix("Hjelperen svarte ikke (").and_then(|s| s.strip_suffix(')')) {
        return Cow::Owned(crate::trf!("Hjelperen svarte ikke ({feil})", feil = cause));
    }
    if let Some(cause) = message.strip_prefix("Hjelperen tok ikke imot (").and_then(|s| s.strip_suffix(')')) {
        return Cow::Owned(crate::trf!("Hjelperen tok ikke imot ({feil})", feil = cause));
    }
    Cow::Borrowed(display_message(message))
}

/// Meldinger fra hjelperstatus eller en feil uten eget felt for feilkode.
pub fn display_message(message: &str) -> &str {
    if crate::i18n::active() == crate::i18n::Language::Nb {
        return message;
    }
    match message {
        "celsius må være 0–120" => crate::tr!("celsius må være 0–120"),
        "RGB-backenden er ikke identifisert eller tilgjengelig." => crate::tr!("RGB-backenden er ikke identifisert eller tilgjengelig."),
        "Viftestyring krever en lesbar temperatur for overtemperaturvernet." => crate::tr!("Viftestyring krever en lesbar temperatur for overtemperaturvernet."),
        "Nødregelen gjelder. Vent til kortet er under 80 °C." => crate::tr!("Nødregelen gjelder. Vent til kortet er under 80 °C."),
        "Endringer er låst: endring av skjermkortet er ikke slått på. Slå på «Tillat endring av skjermkortet» i Innstillinger › Styring." => crate::tr!("Endringer er låst: endring av skjermkortet er ikke slått på. Slå på «Tillat endring av skjermkortet» i Innstillinger › Styring."),
        "Hjelperen for endring svarer ikke. Se Innstillinger › Styring." => crate::tr!("Hjelperen for endring svarer ikke. Se Innstillinger › Styring."),
        "Hjelperen er fra en annen versjon av appen. Oppdater systemkomponentene fra Innstillinger › Skjermkortstyring." => crate::tr!("Hjelperen er fra en annen versjon av appen. Oppdater systemkomponentene fra Innstillinger › Skjermkortstyring."),
        "Hjelperen er fra en annen versjon av appen. Slå bryteren i Innstillinger › Styring av og på igjen." => crate::tr!("Hjelperen er fra en annen versjon av appen. Slå bryteren i Innstillinger › Styring av og på igjen."),
        "Hjelperen og tjenesten bruker ulike kort (simulert og ekte). Ingenting er sendt." => crate::tr!("Hjelperen og tjenesten bruker ulike kort (simulert og ekte). Ingenting er sendt."),
        "NVML-hjelperen er ikke klar til å endre dette kortet." => crate::tr!("NVML-hjelperen er ikke klar til å endre dette kortet."),
        "Skjermkortstyring må aktiveres med bryteren i Innstillinger." => crate::tr!("Skjermkortstyring må aktiveres med bryteren i Innstillinger."),
        "Det valgte kortet er ikke kortet hjelperen er aktivert for." => crate::tr!("Det valgte kortet er ikke kortet hjelperen er aktivert for."),
        "Gjenoppretting kunne ikke bekreftes. Nye endringer er sperret." => crate::tr!("Gjenoppretting kunne ikke bekreftes. Nye endringer er sperret."),
        "Lys er ikke laget ennå." => crate::tr!("Lys er ikke laget ennå."),
        "Endringer er låst. Skriving til skjermkortet er ikke slått på." => crate::tr!("Endringer er låst. Skriving til skjermkortet er ikke slått på."),
        "Funksjonen er ikke tilgjengelig på dette kortet." => crate::tr!("Funksjonen er ikke tilgjengelig på dette kortet."),
        "Hjelperen er aktivert for en annen bruker." => crate::tr!("Hjelperen er aktivert for en annen bruker."),
        "Hjelperen kjører, men helper.json mangler." => crate::tr!("Hjelperen kjører, men helper.json mangler."),
        "Gjenopprettingsmerkøren har feil identitet eller utrygg eier/modus." => crate::tr!("Gjenopprettingsmerkøren har feil identitet eller utrygg eier/modus."),
        "Skjermkortet eller tjenesten svarer ikke. Endringer er låst." => crate::tr!("Skjermkortet eller tjenesten svarer ikke. Endringer er låst."),
        "Hjelperen er aktivert for et annet kort. Aktiver styring for kortet som vises." => crate::tr!("Hjelperen er aktivert for et annet kort. Aktiver styring for kortet som vises."),
        "Valgt kort ble endret mens forespørselen ventet. Svaret brukes ikke på det nye kortet." => crate::tr!("Valgt kort ble endret mens forespørselen ventet. Svaret brukes ikke på det nye kortet."),
        "Endringer på skjermkortet er slått av. Aktiver én gang med administratorpassord i Innstillinger › Skjermkortstyring." => crate::tr!("Endringer på skjermkortet er slått av. Aktiver én gang med administratorpassord i Innstillinger › Skjermkortstyring."),
        "Hjelperen er ikke skriveklar for kortet som vises." => crate::tr!("Hjelperen er ikke skriveklar for kortet som vises."),
        "Dette eldre utkastet mangler opprinnelig grunnlag. Behold det som referanse, eller forkast det og lag et nytt utkast fra kortets verdier." => crate::tr!("Dette eldre utkastet mangler opprinnelig grunnlag. Behold det som referanse, eller forkast det og lag et nytt utkast fra kortets verdier."),
        "Utkastet tilhører en annen kortidentitet. Velg det opprinnelige kortet, eller forkast utkastet før du lager et nytt." => crate::tr!("Utkastet tilhører en annen kortidentitet. Velg det opprinnelige kortet, eller forkast utkastet før du lager et nytt."),
        "Det finnes ikke noe utkast å bruke." => crate::tr!("Det finnes ikke noe utkast å bruke."),
        "Ugyldig argument." => crate::tr!("Ugyldig argument."),
        "Ukjent skjermkort." => crate::tr!("Ukjent skjermkort."),
        "Kortet eller driveren støtter ikke dette." => crate::tr!("Kortet eller driveren støtter ikke dette."),
        "Ikke tillatt." => crate::tr!("Ikke tillatt."),
        "En annen klient har en endring som venter." => crate::tr!("En annen klient har en endring som venter."),
        "Ingen endring venter." => crate::tr!("Ingen endring venter."),
        "Endringen finnes ikke lenger." => crate::tr!("Endringen finnes ikke lenger."),
        "Bare klienten som gjorde endringen, kan bekrefte den." => crate::tr!("Bare klienten som gjorde endringen, kan bekrefte den."),
        "Tjenesten stopper." => crate::tr!("Tjenesten stopper."),
        "Feil i tjenesten." => crate::tr!("Feil i tjenesten."),
        "Ikke tilgjengelig nå." => crate::tr!("Ikke tilgjengelig nå."),
        "Trinnet er ikke godkjent." => crate::tr!("Trinnet er ikke godkjent."),
        "Kortets standardtilstand kunne ikke bekreftes ved tilbakelesing." => crate::tr!("Kortets standardtilstand kunne ikke bekreftes ved tilbakelesing."),
        "Hjelperen lukket forbindelsen" => crate::tr!("Hjelperen lukket forbindelsen"),
        "For stort svar fra hjelperen" => crate::tr!("For stort svar fra hjelperen"),
        "Tjenesten svarte ikke på 5 s." => crate::tr!("Tjenesten svarte ikke på 5 s."),
        _ => crate::txn::display_validation_message(display_hardware_message(message)),
    }
}

fn display_hardware_message(message: &str) -> &str {
    match message.split_once(':').map(|(kind, _)| kind) {
        Some("not_supported") => crate::tr!("Kortet eller driveren støtter ikke denne funksjonen."),
        Some("no_permission") => crate::tr!("Driveren ga ikke tilgang til å endre kortet."),
        Some("identity_mismatch") => crate::tr!("Det valgte kortet er ikke kortet hjelperen er aktivert for."),
        Some("gpu_lost") => crate::tr!("Kortet svarer ikke."),
        Some("readback_mismatch") => crate::tr!("Kortets innstillinger kunne ikke bekreftes ved tilbakelesing."),
        Some("baseline_required") => crate::tr!("De opprinnelige innstillingene for kortet mangler."),
        Some("hardware_fault") => crate::tr!("Kortet må gjenopprettes før nye endringer."),
        Some("invalid_argument") => crate::tr!("NVML avviste en ugyldig verdi."),
        Some("nvml_error") => crate::tr!("En NVML-operasjon mislyktes."),
        _ => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visning_beholder_bokmaal_og_ukjente_meldinger() {
        let original = hardware_error("no_permission: NVML".to_owned());
        assert_eq!(display_error(&original), original.message);
        assert_eq!(display_hardware_message("driver: ukjent"), "driver: ukjent");
        assert_eq!(display_hardware_message(&original.message), "Driveren ga ikke tilgang til å endre kortet.");
    }

    #[test]
    fn samme_kurvefelt_kan_bety_temperatur_eller_viftefart() {
        let temperature = display_range("fan.curve", "Temperaturen i kurven må være 0–110 °C", 0.0, 110.0).unwrap();
        let speed = display_range("fan.curve", "Viftefarten i kurven må være 35–100 %", 35.0, 100.0).unwrap();
        assert_eq!(temperature.0, "Temperaturen i kurven");
        assert_eq!(speed.0, "Viftefarten i kurven");
        assert!(display_range("fan.curve", "Ukjent feilmelding", 0.0, 110.0).is_none());
    }

    #[test]
    fn uioppdatering_beholder_kompatibel_hjelper_men_ikke_eldre_motor() {
        assert!(crate::version::build() >= 32);
        // 1.6.0 (bygg 32) retter sikkerhetsmotoren: 25–31 er utdatert.
        for old in [22, 23, 24, 25, 28, 31] {
            assert!(!compatible(PROTOCOL, old), "{old}");
        }
        assert!(!compatible(PROTOCOL, 32));
        assert!(!compatible(PROTOCOL, 33));
        assert!(compatible(PROTOCOL, crate::version::build()));
        assert!(!compatible(PROTOCOL - 1, 32));
        assert!(!compatible(PROTOCOL + 1, 32));
        assert!(!compatible(PROTOCOL, crate::version::build() + 1));
    }

    /// Første bygg med NVML-sikkerhetsrettingene i hjelperen (PR #23:
    /// monoton klokke, merkøren slettes aldri ubekreftet, overtemperaturvern
    /// i feiltilstand og IPC-herding). Eldre hjelpere mangler dem og skal
    /// avvises fra og med utgaven som tar dem med. Utgivelsen må derfor heve
    /// `MIN_COMPATIBLE_BUILD` til dette bygget sammen med byggnummeret.
    const NVML_SIKKERHET_BYGG: u32 = 32;

    #[test]
    fn utgaven_med_nvml_rettingene_avviser_eldre_hjelpere() {
        let build = crate::version::build();
        if build >= NVML_SIKKERHET_BYGG {
            assert!(
                !compatible(PROTOCOL, NVML_SIKKERHET_BYGG - 1),
                "Bygg {build} tar med NVML-rettingene: hev MIN_COMPATIBLE_BUILD til minst {NVML_SIKKERHET_BYGG}."
            );
            assert!(compatible(PROTOCOL, build));
        }
    }

    #[test]
    fn helper_json_tolkes_strengt() {
        let c = Config::parse(
            r#"{"versjon":1,"uid":1000,"trinn":[],"bygg":18,"aktivert":"2026-11-09T10:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(c.uid, 1000);
        assert!(c.steps.is_empty());
        let c = Config::parse(r#"{"versjon":1,"uid":1000,"trinn":["vifter","effekt","effekt"]}"#)
            .unwrap();
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
    fn global_tillatelse_er_bundet_og_eldre_trinn_eskalerer_ikke() {
        let legacy = Config::parse(
            r#"{"versjon":1,"uid":1000,"trinn":["effekt","vifter","klokker","lys"]}"#,
        )
        .unwrap();
        assert!(!legacy.enabled && legacy.legacy);
        let c = Config::parse(r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"0000:01:00.0","uuid":"GPU-kort-test","bygg":23}"#).unwrap();
        assert!(c.enabled && !c.legacy);
        for step in [Step::Effekt, Step::Vifter, Step::Klokker] {
            assert!(c.enabled(step));
        }
        assert!(!c.enabled(Step::Lys));
        // Eldre, eksplisitte sim-fixtures beholder sine trinn.
        assert!(legacy.enabled(Step::Lys));
        assert_eq!(Config::parse(&c.to_json().to_string()).unwrap(), c);
        for bad in [
            r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"../../dev/i2c-0","uuid":"GPU-kort-test"}"#,
            r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"0000:01:00.0","uuid":"GPU-kort-test","trinn":[]}"#,
            r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"0000:01:00.0"}"#,
        ] {
            assert!(Config::parse(bad).is_err());
        }
    }

    #[test]
    fn feltene_har_hvert_sitt_trinn() {
        assert_eq!(
            steps_for_patch(&json!({"powerLimitW": 200, "tempTargetC": 80})).unwrap(),
            vec![Step::Effekt]
        );
        assert_eq!(
            steps_for_patch(&json!({"fan": {"mode": "manual"}, "clockOffsets": {}})).unwrap(),
            vec![Step::Vifter, Step::Klokker]
        );
        assert!(steps_for_patch(&json!({"voltMv": 900})).is_err());
        assert!(steps_for_patch(&json!(3)).is_err());
        assert_eq!(
            patch_for_op("set_power_limit", &json!({"watts": 200})).unwrap(),
            json!({"powerLimitW": 200})
        );
        assert_eq!(
            patch_for_op("set_fan", &json!({"mode": "auto", "tull": 1})).unwrap(),
            json!({"fan": {"mode": "auto"}})
        );
        assert!(patch_for_op("confirm", &json!({})).is_none());
    }

    #[test]
    fn stiene_under_en_falsk_rot() {
        let p = Paths::new("/tmp/x");
        assert_eq!(p.config(), Path::new("/tmp/x/etc/agentic-rigg/helper.json"));
        assert_eq!(p.socket(), Path::new("/tmp/x/run/agentic-rigg/helper.sock"));
        assert!(!p.is_real());
        assert_eq!(Paths::new("/").binary(), Path::new(BINARY_PATH));
        assert_eq!(
            step_not_enabled(&[Step::Effekt]).code,
            ErrorCode::StepNotEnabled
        );
        assert_eq!(
            locked("helper_off").details.unwrap()["reason"],
            "helper_off"
        );
    }
}
