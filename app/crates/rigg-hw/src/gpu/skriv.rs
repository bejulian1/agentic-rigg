//! Skrivedelen for root-hjelperen (`agentic-rigg-helper`, runde 7A).
//! **Bare hjelperen bruker denne modulen.** Tjenesten og vinduet gjør det
//! ikke (`tests/bare_lesing.rs` sjekker det).
//!
//! - [`SimSkriver`]: et simulert kort i minnet, til tester.
//! - [`open_nvml_bound`]: kortbundet, valgfri NVML-backend. Åpning leser bare;
//!   hjelperen må også kontrollere brukerens globale tillatelse og lagre
//!   baseline før noe kan skrives. Ubundet [`open_nvml`] avvises alltid.
//!
//! Grensesnittet tar hele [`Values`] (det `txn::Book` har i bruk) og legger
//! på det som er ulikt det forrige. Ingen vilkårlige kall, stier eller
//! adresser: bare de fem tingene kortet kan endre.

use rigg_core::txn::{self, FanMode, Range, Values};

/// Bygget inneholder en implementasjon. Cargo-funksjonen og hjelperens
/// root-eide autorisasjon, kortbinding og vedvarende baseline kreves i tillegg.
pub const EKTE_SKRIVING: bool = true;

/// Områdene kortet selv melder. Verdier utenfor avvises av hjelperen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub power_w: Range,
    pub core_mhz: Range,
    /// Effektiv MHz (NVML oppgir dobbel verdi).
    pub mem_mhz: Range,
    pub temp_target_c: Range,
    pub fan_pct: Range,
    /// Kortets standard (det «tilbakestill alt» går til).
    pub default: StockValues,
}

/// Standardverdiene til kortet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockValues {
    pub power_w: f64,
    pub temp_target_c: f64,
    pub core_offset_mhz: f64,
    pub mem_offset_mhz: f64,
    pub fan_percent: f64,
}

impl Limits {
    /// RTX 2080 målt med NVML (maskinvare-revisjon kap. 8), som `txn`.
    pub fn rtx2080() -> Limits {
        let d = Values::default();
        Limits {
            power_w: txn::HW_POWER,
            core_mhz: txn::HW_CORE,
            mem_mhz: txn::HW_MEM,
            temp_target_c: txn::HW_TEMP,
            fan_pct: txn::HW_FAN,
            default: StockValues {
                power_w: d.power_limit_w,
                temp_target_c: d.temp_target_c,
                core_offset_mhz: d.core_offset_mhz,
                mem_offset_mhz: d.mem_offset_mhz,
                fan_percent: d.fan_percent,
            },
        }
    }

    /// Standardoppsettet: kortets effekt og temperaturmål, ingen offset,
    /// driveren styrer viftene.
    pub fn stock(&self) -> Values {
        Values {
            power_limit_w: self.default.power_w,
            temp_target_c: self.default.temp_target_c,
            core_offset_mhz: self.default.core_offset_mhz,
            mem_offset_mhz: self.default.mem_offset_mhz,
            fan_percent: self.default.fan_percent,
            ..Values::default()
        }
    }
}

/// Et kort som kan endres. Bare hjelperen holder en.
pub trait Skriver {
    /// «sim» eller «nvml».
    fn backend(&self) -> &'static str;
    fn limits(&self) -> Limits;
    fn identity(&self) -> serde_json::Value {
        serde_json::json!({"pci": "sim", "uuid": "sim", "driver": "sim", "vbios": "sim"})
    }
    /// Gettere og tilgjengelige symboler, aldri en automatisk skriveprøve.
    fn capabilities(&self) -> serde_json::Value {
        let l = self.limits();
        let cap = |r: Range| serde_json::json!({"available": true, "min": r.min, "max": r.max, "readback": true});
        serde_json::json!({"power": cap(l.power_w), "core_clock": cap(l.core_mhz), "mem_clock": cap(l.mem_mhz), "fan_manual": cap(l.fan_pct), "fan_curve": cap(l.fan_pct), "temp_target": cap(l.temp_target_c), "rgb": {"available": false, "reason": "not_implemented", "readback": false}})
    }
    fn current_values(&self) -> Values {
        self.limits().stock()
    }
    /// Hjelperen må lagre dette atomisk før load_baseline og første skriving.
    fn baseline(&self) -> serde_json::Value {
        serde_json::json!({"version": 1, "identity": self.identity(), "captured": self.current_values().to_json(), "restore": self.limits().stock().to_json()})
    }
    fn load_baseline(&mut self, _baseline: &serde_json::Value) -> Result<(), String> {
        Ok(())
    }
    /// Temperaturen nå (°C), til nødregelen og viftekurven.
    fn temperature(&mut self) -> Option<f64>;
    /// Legger på `v` (det som er ulikt det forrige). Med `fan_mode = curve`
    /// settes ikke farten her; hjelperen kaller [`Skriver::set_fan_percent`].
    fn apply(&mut self, v: &Values) -> Result<(), String>;
    /// Gjenoppretting etter en delvis feil må ikke stole på skrivecachen.
    fn restore_values(&mut self, v: &Values) -> Result<(), String> {
        self.apply(v)
    }
    /// Fast fart på alle viftene (nødregelen og Manuell).
    fn set_fan_percent(&mut self, pct: f64) -> Result<(), String>;
    /// Fart per vifte (viftekurven fra `rigg_core::vifte`). Standard: den
    /// høyeste på alle.
    fn set_fan_percents(&mut self, pct: &[f64]) -> Result<(), String> {
        self.set_fan_percent(pct.iter().copied().fold(0.0, f64::max))
    }
    /// Antall vifter (2 på RTX 2080).
    fn fan_count(&self) -> usize {
        2
    }
    /// Gjenoppretting etter stopp/krasj: så mange vifter kunne styres da
    /// endringene startet (`dirty.json`). Finner kortet færre nå, kan Auto
    /// ikke bekreftes, og `reset`/`snapshot` skal ikke melde standard.
    fn require_fan_count(&mut self, _n: usize) {}
    /// Farten viftene har nå (%), én per vifte. Viftemotoren starter der
    /// når Kurve slås på (`Motor::med_start`), så rampen ikke hopper. Tom:
    /// ukjent (motoren går da rett til kurven).
    fn fan_percents(&mut self) -> Vec<f64> {
        Vec::new()
    }
    /// Alt tilbake til standard, viftene til driveren (Auto).
    fn reset(&mut self) -> Result<(), String>;
    /// Det som faktisk er lagt på kortet (til `status`).
    fn snapshot(&self) -> serde_json::Value;
    /// Bare det simulerte kortet: holder temperaturen fast (testene).
    fn set_sim_temperature(&mut self, _c: f64) -> bool {
        false
    }
}

// ---------------------------------------------------------------- sim

/// Et simulert kort: husker det som er lagt på, og hvor mange skrivinger
/// som er gjort. Temperaturen settes av testene.
#[derive(Clone, Debug)]
pub struct SimSkriver {
    pub applied: Values,
    /// Farten viftene faktisk har fått (`None`: driveren styrer). Med
    /// fart per vifte: den høyeste.
    pub fan_now: Option<f64>,
    /// Farten per vifte når kurven har satt dem hver for seg.
    pub fans: Option<Vec<f64>>,
    pub temp_c: f64,
    pub writes: u64,
    limits: Limits,
}

impl Default for SimSkriver {
    fn default() -> Self {
        SimSkriver::new()
    }
}

impl SimSkriver {
    pub fn new() -> SimSkriver {
        let limits = Limits::rtx2080();
        SimSkriver {
            applied: limits.stock(),
            fan_now: None,
            fans: None,
            temp_c: 45.0,
            writes: 0,
            limits,
        }
    }

    /// Er kortet på standard (effekt, offset, temperaturmål og Auto)?
    pub fn is_stock(&self) -> bool {
        let s = self.limits.stock();
        self.fan_now.is_none()
            && self.applied.power_limit_w == s.power_limit_w
            && self.applied.core_offset_mhz == 0.0
            && self.applied.mem_offset_mhz == 0.0
            && self.applied.temp_target_c == s.temp_target_c
            && self.applied.fan_mode == FanMode::Auto
    }
}

impl Skriver for SimSkriver {
    fn backend(&self) -> &'static str {
        "sim"
    }
    fn limits(&self) -> Limits {
        self.limits
    }
    fn current_values(&self) -> Values {
        self.applied.clone()
    }
    fn temperature(&mut self) -> Option<f64> {
        Some(self.temp_c)
    }
    fn apply(&mut self, v: &Values) -> Result<(), String> {
        if self.applied.diff(v).is_empty()
            && (v.fan_mode != FanMode::Manual || self.fan_now == Some(v.fan_percent))
        {
            return Ok(());
        }
        self.writes += 1;
        self.applied = v.clone();
        match v.fan_mode {
            FanMode::Auto => (self.fan_now, self.fans) = (None, None),
            FanMode::Manual => (self.fan_now, self.fans) = (Some(v.fan_percent), None),
            FanMode::Curve => {}
        }
        Ok(())
    }
    fn set_fan_percent(&mut self, pct: f64) -> Result<(), String> {
        let p = self.limits.fan_pct.clamp(pct.round());
        if self.fan_now != Some(p) || self.fans.is_some() {
            self.writes += 1;
            self.fan_now = Some(p);
            self.fans = None;
        }
        Ok(())
    }
    fn set_fan_percents(&mut self, pct: &[f64]) -> Result<(), String> {
        let p: Vec<f64> = pct
            .iter()
            .map(|p| self.limits.fan_pct.clamp(p.round()))
            .collect();
        if p.is_empty() {
            return Ok(());
        }
        if self.fans.as_ref() != Some(&p) {
            self.writes += 1;
            self.fan_now = p.iter().copied().reduce(f64::max);
            self.fans = Some(p);
        }
        Ok(())
    }
    /// Med driveren (Auto) går viftene etter driverens kurve, som i
    /// tjenestens sim-sløyfe ([`super::sim_vifter::driverkurve`]).
    fn fan_percents(&mut self) -> Vec<f64> {
        let n = self.fan_count();
        match (&self.fans, self.fan_now) {
            (Some(f), _) => f.clone(),
            (None, Some(p)) => vec![p; n],
            (None, None) => vec![super::sim_vifter::driverkurve(self.temp_c); n],
        }
    }
    fn reset(&mut self) -> Result<(), String> {
        if !self.is_stock() {
            self.writes += 1;
        }
        self.applied = self.limits.stock();
        self.fan_now = None;
        self.fans = None;
        Ok(())
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"applied": self.applied.to_json(), "fanNow": self.fan_now, "fans": self.fans, "tempC": self.temp_c, "writes": self.writes, "stock": self.is_stock()})
    }
    fn set_sim_temperature(&mut self, c: f64) -> bool {
        self.temp_c = c;
        true
    }
}

// ---------------------------------------------------------------- det ekte kortet

/// Er backenden kompilert inn? Dette er ingen runtime-tillatelse til skriving.
pub fn real_writes_available() -> bool {
    cfg!(feature = "ekte-skriving") && EKTE_SKRIVING
}

/// Den gamle, ubundne inngangen avvises uten å laste NVML.
pub fn open_nvml() -> Result<Box<dyn Skriver>, String> {
    Err("identity_required: NVML-skriving krever både PCI-adresse og UUID".into())
}

/// Åpner og leser bare. Første set-kall krever også load_baseline etter at
/// hjelperen har lagret baseline på disk. Ingen reserve til indeks 0.
pub fn open_nvml_bound(pci: &str, uuid: &str) -> Result<Box<dyn Skriver>, String> {
    if !valid_binding(pci, uuid) {
        return Err("identity_required: ugyldig PCI-adresse eller GPU-UUID".into());
    }
    if !real_writes_available() {
        return Err("Skriving til det ekte kortet er sperret i denne versjonen (rigg-hw: ekte-skriving er av).".into());
    }
    #[cfg(feature = "ekte-skriving")]
    {
        crate::gpu::skriv_nvml::open(pci, uuid).map(|s| Box::new(s) as Box<dyn Skriver>)
    }
    #[cfg(not(feature = "ekte-skriving"))]
    {
        unreachable!("sperret over")
    }
}

pub fn valid_binding(pci: &str, uuid: &str) -> bool {
    let b = pci.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && b.iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10) || c.is_ascii_hexdigit())
        && (b'0'..=b'7').contains(&b[11])
        && uuid.starts_with("GPU-")
        && uuid.len() <= 80
        && uuid.len() > 4
        && uuid[4..]
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ubundet_eller_ugyldig_kort_kan_aldri_aapnes() {
        assert!(open_nvml().is_err());
        assert!(open_nvml_bound("", "").is_err());
        assert!(open_nvml_bound("../../gpu", "GPU-1234").is_err());
        assert!(valid_binding("0000:1c:00.0", "GPU-1234-abcd"));
    }

    #[test]
    fn sim_husker_og_tilbakestiller() {
        let mut s = SimSkriver::new();
        assert!(s.is_stock());
        let v = Values {
            power_limit_w: 200.0,
            fan_mode: FanMode::Manual,
            fan_percent: 60.0,
            ..s.limits().stock()
        };
        s.apply(&v).unwrap();
        assert_eq!(s.fan_now, Some(60.0));
        assert!(!s.is_stock());
        s.apply(&v).unwrap();
        assert_eq!(s.writes, 1, "samme verdier skrives ikke på nytt");
        s.set_fan_percent(20.0).unwrap();
        assert_eq!(s.fan_now, Some(35.0), "under kortets minste fart klemmes");
        s.reset().unwrap();
        assert!(s.is_stock());
    }
}
