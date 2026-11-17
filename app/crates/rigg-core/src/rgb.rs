//! Avgrenset datamodell for Palit-kortets fysiske lys.
//!
//! Dette er ikke forhåndsvisningens 56 lysdioder eller tre tegnede soner.
//! Identifikasjon gir ingen skrivetillatelse. Registerfarge og lysstyrke kan
//! være lesbare selv om aktiv effekt er ukjent; ingen modus utledes fra dem.
//! Modellen åpner ingen enheter og inneholder ingen registerkommandoer.

use serde::{Deserialize, Serialize};

pub const CONTROLLER_ADDRESS: u8 = 0x49;

/// Den ene fysiske styringssonen i den avgrensede Palit-protokollen.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    CardLight,
}

pub const PHYSICAL_ZONES: [Zone; 1] = [Zone::CardLight];

/// Eksakte, kildebelagte signaturer. PCI-navnet alene er ingen signatur.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Signature {
    #[serde(rename = "palit_100")]
    Palit100,
    #[serde(rename = "palit_150")]
    Palit150,
}

impl Signature {
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, String> {
        [Self::Palit100, Self::Palit150]
            .into_iter()
            .find(|s| s.bytes() == bytes)
            .ok_or_else(|| "Lyskontrollerens signatur er ikke gjenkjent.".into())
    }

    pub fn bytes(self) -> [u8; 16] {
        match self {
            Self::Palit100 => *b"\0\0\0\0\0\0PALIT 100\0",
            Self::Palit150 => *b"\0\0\0\0\0\0PALIT 150\0",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PciIdentity {
    pub vendor: u16,
    pub device: u16,
    pub subsystem_vendor: u16,
    pub subsystem_device: u16,
}

/// Stabil adapteridentitet; Linux-bussnummeret er bevisst utelatt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterIdentity {
    pub port: u8,
    pub name: String,
    /// Nærmeste PCI-forfader, ikke en vilkårlig GPU høyere i sysfs-treet.
    pub parent_pci: String,
    pub driver: String,
}

/// Binding observert av hjelperen, aldri en klientvalgt I2C-sti.
/// `validate` kontrollerer innholdet; transporten må i tillegg bevise at
/// metadata, enhetsnode og signatur faktisk tilhører det samme kortet.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub pci: String,
    pub uuid: String,
    pub pci_ids: PciIdentity,
    pub adapter: AdapterIdentity,
    pub address: u8,
    pub signature: Signature,
}

impl Binding {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::hjelper::valid_pci(&self.pci) || !crate::hjelper::valid_uuid(&self.uuid) {
            return Err("Lysbindingen trenger gyldig PCI-adresse og GPU-UUID.".into());
        }
        if self.pci_ids.vendor != 0x10de
            || self.pci_ids.device != 0x1e87
            || self.pci_ids.subsystem_vendor != 0x10de
            || self.pci_ids.subsystem_device != 0x1e87
        {
            return Err("PCI-identiteten omfattes ikke av denne avgrensede lysstøtten.".into());
        }
        // NVIDIA-navnet skriver buss med %x, uten PCI-adressens ledende null.
        let bus = u8::from_str_radix(&self.pci[5..7], 16)
            .map_err(|_| "Lysbindingen har ugyldig PCI-buss.".to_string())?;
        let device = u8::from_str_radix(&self.pci[8..10], 16)
            .map_err(|_| "Lysbindingen har ugyldig PCI-enhet.".to_string())?;
        if device > 0x1f || !self.pci.ends_with(".0") {
            return Err("Lysbindingen krever en gyldig GPU-enhet på PCI-funksjon 0.".into());
        }
        if self.address != CONTROLLER_ADDRESS
            || self.adapter.port != 1
            || self.adapter.parent_pci != self.pci
            || self.adapter.driver != "nvidia"
            || self.adapter.name != format!("NVIDIA i2c adapter 1 at {bus:x}:{}", &self.pci[8..])
        {
            return Err("Lysadapteren samsvarer ikke med den avgrensede kortbindingen.".into());
        }
        Ok(())
    }
}

/// Bare de to semantiske valgene denne første styringsmodellen omfatter.
/// `Off` beskriver ønsket slukket lys, ikke en antatt registerverdi.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Static,
    Off,
}

/// Brukervalg har området 0–100. Rå avlesning har fortsatt hele byteområdet.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "u8", into = "u8")]
pub struct Brightness(u8);

impl Brightness {
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Brightness {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err("Lysstyrken må være et heltall fra 0 til 100.".into())
        }
    }
}

impl From<Brightness> for u8 {
    fn from(value: Brightness) -> Self {
        value.0
    }
}

/// Ingen registeradresser, lagringskommando, soneliste eller automatisk apply.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Static {
        color: [u8; 3],
        brightness: Brightness,
    },
    Off {},
}

/// Faktiske avlesninger. `None` betyr ukjent, ikke null eller standardverdi.
/// Primærfargen beviser verken aktiv effekt eller hva lyset viser akkurat nå.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observed {
    pub color: Option<[u8; 3]>,
    pub brightness_raw: Option<u8>,
    /// Settes bare når en støttet modus er bevist ved faktisk tilbakelesing.
    pub mode: Option<Mode>,
}

impl Observed {
    /// Bevarer en rå verdi over 100 som ukjent/ustøttet semantisk lysstyrke.
    pub fn brightness_percent(&self) -> Option<Brightness> {
        self.brightness_raw
            .and_then(|v| Brightness::try_from(v).ok())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    NotAuthorized,
    NotFound,
    Unavailable,
    Unrecognized,
    IdentifiedReadOnly,
}

/// Resultat av avgrenset oppdagelse. Selv en kjent signatur gir bare lesestatus.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Discovery {
    pub state: DiscoveryState,
    pub binding: Option<Binding>,
    /// Tom før kontrolleren er identifisert; én sone etterpå.
    pub zones: Vec<Zone>,
    pub observed: Observed,
    pub reason: Option<String>,
}

impl Discovery {
    pub fn identified(binding: Binding, observed: Observed) -> Result<Self, String> {
        binding.validate()?;
        let reason = observed
            .brightness_raw
            .filter(|v| *v > 100)
            .map(|_| "Avlest lysstyrke ligger utenfor det støttede området 0–100.".into());
        Ok(Self {
            state: DiscoveryState::IdentifiedReadOnly,
            binding: Some(binding),
            zones: PHYSICAL_ZONES.to_vec(),
            observed,
            reason,
        })
    }
}

/// Hvor kunnskapen om opprinnelig modus kommer fra. Brukerbekreftelse er
/// ingen avlesning og kan ikke brukes som bevis på senere gjenoppretting.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSource {
    ObservedMode,
    UserConfirmedStatic,
}

/// Et komplett, avgrenset opptak. Feltene er private, så ufullstendige
/// avlesninger ikke blir grunnverdier gjennom en vanlig struct-literal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "BaselineData", into = "BaselineData")]
pub struct Baseline {
    data: BaselineData,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BaselineData {
    binding: Binding,
    observed: Observed,
    source: BaselineSource,
}

impl TryFrom<BaselineData> for Baseline {
    type Error = String;

    fn try_from(data: BaselineData) -> Result<Self, Self::Error> {
        data.binding.validate()?;
        if data.observed.color.is_none() || data.observed.brightness_percent().is_none() {
            return Err("Lysgrunnverdier krever avlest farge og støttet lysstyrke.".into());
        }
        match data.source {
            BaselineSource::ObservedMode if data.observed.mode.is_none() => {
                return Err("Ukjent effekt kan ikke brukes som avlest lysgrunnverdi.".into());
            }
            BaselineSource::UserConfirmedStatic if data.observed.mode.is_some() => {
                return Err(
                    "Brukerbekreftet statisk effekt skal holdes atskilt fra avlest modus.".into(),
                );
            }
            _ => {}
        }
        Ok(Self { data })
    }
}

impl From<Baseline> for BaselineData {
    fn from(value: Baseline) -> Self {
        value.data
    }
}

/// Resultatet sammenligner bare med ny, kortbundet avlesning. Samme lesbare
/// RGB/lysstyrke ved ukjent modus er uttrykkelig svakere enn verifisert restore.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryReadback {
    Verified,
    ReadableFieldsMatch,
    Mismatch,
    Unknown,
}

impl Baseline {
    pub fn from_observed(binding: Binding, observed: Observed) -> Result<Self, String> {
        BaselineData {
            binding,
            observed,
            source: BaselineSource::ObservedMode,
        }
        .try_into()
    }

    /// Kalles bare etter en særskilt brukerbekreftelse av statisk utgangspunkt;
    /// denne modellen gir verken tillatelse til skriving eller gjenoppretting.
    pub fn user_confirmed_static(binding: Binding, observed: Observed) -> Result<Self, String> {
        BaselineData {
            binding,
            observed,
            source: BaselineSource::UserConfirmedStatic,
        }
        .try_into()
    }

    pub fn binding(&self) -> &Binding {
        &self.data.binding
    }

    pub fn observed(&self) -> &Observed {
        &self.data.observed
    }

    pub fn source(&self) -> BaselineSource {
        self.data.source
    }

    pub fn expected_mode(&self) -> Mode {
        match self.data.source {
            BaselineSource::UserConfirmedStatic => Mode::Static,
            BaselineSource::ObservedMode => self.data.observed.mode.expect("validert grunnopptak"),
        }
    }

    pub fn recovery_readback(&self, binding: &Binding, observed: &Observed) -> RecoveryReadback {
        if binding != self.binding() {
            return RecoveryReadback::Mismatch;
        }
        if observed
            .color
            .is_some_and(|v| Some(v) != self.data.observed.color)
            || observed
                .brightness_raw
                .is_some_and(|v| Some(v) != self.data.observed.brightness_raw)
            || observed.mode.is_some_and(|v| v != self.expected_mode())
        {
            return RecoveryReadback::Mismatch;
        }
        if observed.color.is_none() || observed.brightness_raw.is_none() {
            return RecoveryReadback::Unknown;
        }
        if observed.mode.is_none() || self.source() == BaselineSource::UserConfirmedStatic {
            RecoveryReadback::ReadableFieldsMatch
        } else {
            RecoveryReadback::Verified
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-test-identity".into(),
            pci_ids: PciIdentity {
                vendor: 0x10de,
                device: 0x1e87,
                subsystem_vendor: 0x10de,
                subsystem_device: 0x1e87,
            },
            adapter: AdapterIdentity {
                port: 1,
                name: "NVIDIA i2c adapter 1 at 1c:00.0".into(),
                parent_pci: "0000:1c:00.0".into(),
                driver: "nvidia".into(),
            },
            address: CONTROLLER_ADDRESS,
            signature: Signature::Palit150,
        }
    }

    fn observed() -> Observed {
        Observed {
            color: Some([58, 123, 240]),
            brightness_raw: Some(80),
            mode: None,
        }
    }

    #[test]
    fn bare_eksakte_signaturer_godtas() {
        for signature in [Signature::Palit100, Signature::Palit150] {
            assert_eq!(Signature::from_bytes(signature.bytes()), Ok(signature));
            for i in 0..16 {
                let mut changed = signature.bytes();
                changed[i] ^= 0x80;
                assert!(Signature::from_bytes(changed).is_err());
            }
        }
        assert!(Signature::from_bytes([0xff; 16]).is_err());
    }

    #[test]
    fn forespoersler_har_strengt_omraade_og_ingen_bussvalg() {
        for brightness in [0, 100] {
            assert!(
                serde_json::from_value::<Request>(
                    json!({"mode":"static", "color":[0, 127, 255], "brightness":brightness})
                )
                .is_ok()
            );
        }
        assert!(serde_json::from_value::<Request>(json!({"mode":"off"})).is_ok());
        for invalid in [
            json!({"mode":"static", "color":[0, 127, 255], "brightness":101}),
            json!({"mode":"static", "color":[0, 127, 256], "brightness":50}),
            json!({"mode":"static", "color":[0, 127], "brightness":50}),
            json!({"mode":"static", "color":[0, 127, 255], "brightness":50.5}),
            json!({"mode":"static", "color":[0, 127, 255]}),
            json!({"mode":"rainbow"}),
            json!({"mode":"off", "address":8}),
            json!({"mode":"off", "persist":true}),
            json!({"mode":"off", "zones":["fan1"]}),
        ] {
            assert!(
                serde_json::from_value::<Request>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn lesbar_farge_og_lysstyrke_beviser_ikke_modus() {
        let mut state = observed();
        state.color = Some([0, 0, 0]);
        assert!(Baseline::from_observed(binding(), state.clone()).is_err());
        assert_eq!(state.mode, None);
        state.brightness_raw = Some(255);
        assert_eq!(state.brightness_percent(), None);
        let report = Discovery::identified(binding(), state.clone()).unwrap();
        assert_eq!(report.observed.brightness_raw, Some(255));
        assert!(report.reason.is_some());
        assert_eq!(report.zones, vec![Zone::CardLight]);
        assert_eq!(report.state, DiscoveryState::IdentifiedReadOnly);
        assert!(Baseline::user_confirmed_static(binding(), state).is_err());
    }

    #[test]
    fn brukerbekreftelse_blir_ikke_til_fiktiv_tilbakelesing() {
        let baseline = Baseline::user_confirmed_static(binding(), observed()).unwrap();
        assert_eq!(baseline.observed().mode, None);
        assert_eq!(baseline.expected_mode(), Mode::Static);
        assert_eq!(
            baseline.recovery_readback(&binding(), &observed()),
            RecoveryReadback::ReadableFieldsMatch
        );
        let mut known = observed();
        known.mode = Some(Mode::Static);
        assert_eq!(
            baseline.recovery_readback(&binding(), &known),
            RecoveryReadback::ReadableFieldsMatch
        );
        let captured = Baseline::from_observed(binding(), known.clone()).unwrap();
        assert_eq!(
            captured.recovery_readback(&binding(), &known),
            RecoveryReadback::Verified
        );
        assert_eq!(
            captured.recovery_readback(&binding(), &observed()),
            RecoveryReadback::ReadableFieldsMatch
        );
        known.color = Some([1, 2, 3]);
        assert_eq!(
            captured.recovery_readback(&binding(), &known),
            RecoveryReadback::Mismatch
        );
        assert_eq!(
            captured.recovery_readback(&binding(), &Observed::default()),
            RecoveryReadback::Unknown
        );
        let mut other = binding();
        other.uuid = "GPU-other-card".into();
        assert_eq!(
            captured.recovery_readback(&other, &observed()),
            RecoveryReadback::Mismatch
        );
    }

    #[test]
    fn lagret_grunnopptak_valideres_paa_nytt() {
        let baseline = Baseline::user_confirmed_static(binding(), observed()).unwrap();
        let mut value = serde_json::to_value(&baseline).unwrap();
        assert_eq!(
            serde_json::from_value::<Baseline>(value.clone()).unwrap(),
            baseline
        );
        value["source"] = json!("observed_mode");
        assert!(serde_json::from_value::<Baseline>(value.clone()).is_err());
        value["source"] = json!("user_confirmed_static");
        value["observed"]["color"] = json!(null);
        assert!(serde_json::from_value::<Baseline>(value).is_err());
    }

    #[test]
    fn feil_adapter_eller_kort_gir_ingen_identifisert_kontroller() {
        let valid = binding();
        let mut invalid = Vec::new();
        let mut b = valid.clone();
        b.adapter.parent_pci = "0000:1c:00.3".into();
        invalid.push(b);
        let mut b = valid.clone();
        b.adapter.driver = "ccgx-ucsi".into();
        invalid.push(b);
        let mut b = valid.clone();
        b.address = 0x08;
        invalid.push(b);
        let mut b = valid.clone();
        b.adapter.name = "SMBus PIIX4 adapter port 0".into();
        invalid.push(b);
        let mut b = valid;
        b.pci_ids.device = 0x1e81;
        invalid.push(b);
        for b in invalid {
            assert!(Discovery::identified(b, observed()).is_err());
        }
    }

    #[test]
    fn nvidia_adapternavnet_har_ikke_ledende_null_i_bussnummeret() {
        let mut b = binding();
        b.pci = "0000:05:00.0".into();
        b.adapter.parent_pci = b.pci.clone();
        b.adapter.name = "NVIDIA i2c adapter 1 at 5:00.0".into();
        assert!(b.validate().is_ok());
        b.adapter.name = "NVIDIA i2c adapter 1 at 05:00.0".into();
        assert!(b.validate().is_err());
    }

    #[test]
    fn lagret_binding_avviser_ugyldig_pci_enhet_og_annen_funksjon() {
        for pci in ["0000:1c:20.0", "0000:1c:00.3"] {
            let mut b = binding();
            b.pci = pci.into();
            b.adapter.parent_pci = pci.into();
            b.adapter.name = format!("NVIDIA i2c adapter 1 at {}", &pci[5..]);
            assert!(b.validate().is_err());
        }
    }
}
