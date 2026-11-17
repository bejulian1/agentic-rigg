//! Separat, kortbundet RGB-tillatelse. GPU-hjelperens globale NVML-tillatelse
//! brukes aldri her. Modellen leser ingen filer og åpner ingen enheter.
//!
//! Produksjonskalleren må selv lese tillatelsen fra en trygg, root-eid fil
//! skrevet ved eksplisitt administratoraktivering. JSON fra en klient er
//! ingen autorisasjon, selv om den kan parses til denne typen.

use crate::rgb::Binding;
use serde::{Deserialize, Serialize};

pub const CONFIG_PATH: &str = "/etc/agentic-rigg/rgb.json";
/// Bare RGB-formatet og sikkerhetsmodellen fra dette bygget er gjennomgått.
/// Dette er uavhengig av NVML-hjelperens kompatibilitet og autorisasjon.
pub const MIN_COMPATIBLE_BUILD: u32 = 24;

pub fn compatible_build(build: u32) -> bool {
    (MIN_COMPATIBLE_BUILD..=crate::version::build()).contains(&build)
}

const SCHEMA_VERSION: u32 = 1;
const MAX_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Data", into = "Data")]
pub struct Authorization {
    data: Data,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Data {
    version: u32,
    uid: u32,
    enabled: bool,
    binding: Binding,
    build: u32,
}

impl TryFrom<Data> for Authorization {
    type Error = String;

    fn try_from(data: Data) -> Result<Self, Self::Error> {
        if data.version != SCHEMA_VERSION {
            return Err("Ukjent format for RGB-tillatelse.".into());
        }
        if data.uid == 0 || data.uid == u32::MAX {
            return Err("RGB-tillatelsen må gjelde en vanlig bruker.".into());
        }
        if data.build == 0 {
            return Err("RGB-tillatelsen mangler hjelperens byggnummer.".into());
        }
        data.binding.validate()?;
        Ok(Self { data })
    }
}

impl From<Authorization> for Data {
    fn from(value: Authorization) -> Self {
        value.data
    }
}

impl Authorization {
    /// Bindingen skal komme fra hjelperens egen ferske identifikasjon.
    /// Dette lagrer ingen fil og gir ingen privilegier i seg selv.
    pub fn new(uid: u32, binding: Binding, build: u32) -> Result<Self, String> {
        Data {
            version: SCHEMA_VERSION,
            uid,
            enabled: true,
            binding,
            build,
        }
        .try_into()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err("RGB-tillatelsen er for stor.".into());
        }
        serde_json::from_str(text).map_err(|e| format!("Ugyldig RGB-tillatelse: {e}"))
    }

    /// Identiteten sammenlignes med hjelperens nye observasjon, ikke med
    /// klientens påstand. UID hentes fra socketens peer credentials.
    pub fn allows(&self, peer_uid: u32, observed: &Binding) -> bool {
        self.data.enabled && peer_uid == self.data.uid && observed == &self.data.binding
    }

    pub fn binding(&self) -> &Binding {
        &self.data.binding
    }

    pub fn uid(&self) -> u32 {
        self.data.uid
    }

    pub fn build(&self) -> u32 {
        self.data.build
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb::{AdapterIdentity, PciIdentity, Signature};
    use serde_json::{Value, json};

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-rgb-authorization-fixture".into(),
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
            address: 0x49,
            signature: Signature::Palit100,
        }
    }

    fn data() -> Value {
        serde_json::to_value(Authorization::new(1000, binding(), 24).unwrap()).unwrap()
    }

    #[test]
    fn tillatelsen_er_egen_og_kortbundet() {
        let a = Authorization::parse(&data().to_string()).unwrap();
        assert!(a.allows(1000, &binding()));
        assert!(!a.allows(0, &binding()));
        assert!(!a.allows(1001, &binding()));
        let mut other = binding();
        other.uuid = "GPU-other-fixture".into();
        assert!(!a.allows(1000, &other));
        other = binding();
        other.signature = Signature::Palit150;
        assert!(!a.allows(1000, &other));
        let gpu_config = r#"{"versjon":2,"uid":1000,"enabled":true,"pci":"0000:1c:00.0","uuid":"GPU-rgb-authorization-fixture","bygg":23}"#;
        assert!(Authorization::parse(gpu_config).is_err());
        assert_eq!(a.uid(), 1000);
        assert_eq!(a.build(), 24);
    }

    #[test]
    fn deaktivering_og_feil_metadata_gir_ingen_tillatelse() {
        let mut d = data();
        d["enabled"] = json!(false);
        let a = Authorization::parse(&d.to_string()).unwrap();
        assert!(!a.allows(1000, &binding()));
        for (field, value) in [
            ("version", json!(2)),
            ("uid", json!(0)),
            ("uid", json!(u32::MAX)),
            ("uid", json!(-1)),
            ("build", json!(0)),
            ("enabled", json!("true")),
        ] {
            let mut d = data();
            d[field] = value;
            assert!(
                Authorization::parse(&d.to_string()).is_err(),
                "{field}: {d}"
            );
        }
        let mut d = data();
        d["binding"]["adapter"]["port"] = json!(2);
        assert!(Authorization::parse(&d.to_string()).is_err());
    }

    #[test]
    fn vilkarlige_stier_registre_og_duplikater_avvises() {
        for field in ["path", "bus", "address", "register", "persist", "steps"] {
            let mut d = data();
            d[field] = json!("/dev/i2c-8");
            assert!(Authorization::parse(&d.to_string()).is_err());
        }
        let text = data().to_string();
        let duplicate = text.replacen('{', "{\"uid\":1001,", 1);
        assert!(Authorization::parse(&duplicate).is_err());
        assert!(Authorization::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
    }

    #[test]
    fn rgb_har_egen_kompatibilitetsgrense() {
        assert!(!compatible_build(23));
        assert!(compatible_build(MIN_COMPATIBLE_BUILD));
        assert!(compatible_build(crate::version::build()));
        assert!(!compatible_build(crate::version::build() + 1));
    }
}
