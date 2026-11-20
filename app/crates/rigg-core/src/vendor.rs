//! Produsenten av skjermkortet (runde 8A), fra PCI-subsystemet som NVML og
//! sysfs oppgir. Kortprodusenten setter sin egen leverandør-ID i
//! subsystemet (Palit 1569, MSI 1462 …). Står det NVIDIAs egen (10de), er
//! kortet enten et referansekort eller laget av en produsent som ikke setter
//! sin egen; da kan produsenten ikke leses sikkert. Palit RTX 2080 Rev. A i
//! denne maskinen melder 10de:1e87, så der må brukeren sette produsenten selv
//! (`Settings::gpu_makers`).

use crate::{tr, trf};

/// Det vi vet om produsenten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Maker {
    /// Kjent kortprodusent fra subsystemet.
    Known(&'static str),
    /// Subsystemet er NVIDIAs (10de): referansekort, eller ukjent produsent.
    Reference,
    /// Ukjent leverandør-ID (står som «PCI 1a2b»).
    Other(u16),
    /// Subsystemet er ikke lest.
    Unknown,
}

/// Kortprodusenter etter PCI-leverandør-ID (pci.ids).
const MAKERS: [(u16, &str); 22] = [
    (0x1569, "Palit"),
    (0x10b0, "Gainward"),
    (0x1462, "MSI"),
    (0x1043, "ASUS"),
    (0x3842, "EVGA"),
    (0x1458, "Gigabyte"),
    (0x19da, "Zotac"),
    (0x196e, "PNY"),
    (0x1b4c, "KFA2 / Galax"),
    (0x7377, "Colorful"),
    (0x1acc, "Point of View"),
    (0x1849, "ASRock"),
    (0x148c, "PowerColor"),
    (0x1da2, "Sapphire"),
    (0x1682, "XFX"),
    (0x174b, "PC Partner"),
    (0x1028, "Dell"),
    (0x103c, "HP"),
    (0x17aa, "Lenovo"),
    (0x1025, "Acer"),
    (0x1d05, "Tongfang"),
    (0x1558, "Clevo"),
];

/// Leverandør-ID-en i subsystemet. NVML gir `subsysDevice << 16 | subsysVendor`
/// (0x1e8710de for referansekortet).
pub fn subsystem_vendor(subsys: u32) -> u16 {
    (subsys & 0xffff) as u16
}

pub fn board_maker(subsys: Option<u32>) -> Maker {
    let Some(s) = subsys.filter(|s| *s != 0) else { return Maker::Unknown };
    let v = subsystem_vendor(s);
    if v == 0x10de {
        return Maker::Reference;
    }
    match MAKERS.iter().find(|m| m.0 == v) {
        Some((_, n)) => Maker::Known(n),
        None => Maker::Other(v),
    }
}

/// «10de:1e87» (leverandør:enhet), som `lspci -nn` skriver subsystemet.
pub fn subsystem_text(subsys: u32) -> String {
    format!("{:04x}:{:04x}", subsys & 0xffff, subsys >> 16)
}

/// Produsenten som tekst: det brukeren har satt, ellers det kortet melder.
/// Gir (tekst, kilde) der kilden er «bruker», «kort» eller «».
pub fn maker_text(user: Option<&str>, subsys: Option<u32>) -> (String, &'static str) {
    if let Some(u) = user.map(str::trim).filter(|u| !u.is_empty()) {
        return (u.to_string(), "bruker");
    }
    match board_maker(subsys) {
        Maker::Known(n) => (n.to_string(), "kort"),
        Maker::Reference => (tr!("NVIDIA-referansekort").into(), "kort"),
        Maker::Other(v) => (trf!("PCI-leverandør {id}", id = format!("{v:04x}")), "kort"),
        Maker::Unknown => (String::new(), ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produsenten_fra_subsystemet() {
        assert_eq!(board_maker(Some(0x1e87_1569)), Maker::Known("Palit"));
        assert_eq!(board_maker(Some(0x1e87_10b0)), Maker::Known("Gainward"));
        assert_eq!(board_maker(Some(0x3721_1462)), Maker::Known("MSI"));
        assert_eq!(board_maker(Some(0x8669_1043)), Maker::Known("ASUS"));
        // Denne maskinens Palit melder NVIDIAs referanse.
        assert_eq!(board_maker(Some(0x1e87_10de)), Maker::Reference);
        assert_eq!(board_maker(Some(0x0001_abcd)), Maker::Other(0xabcd));
        assert_eq!(board_maker(None), Maker::Unknown);
        assert_eq!(subsystem_text(0x1e87_10de), "10de:1e87");
    }

    #[test]
    fn brukerens_valg_går_foran() {
        assert_eq!(maker_text(Some("Palit"), Some(0x1e87_10de)), ("Palit".into(), "bruker"));
        assert_eq!(maker_text(Some("  "), Some(0x1e87_10de)), ("NVIDIA-referansekort".into(), "kort"));
        assert_eq!(maker_text(None, Some(0x1e87_1462)), ("MSI".into(), "kort"));
        assert_eq!(maker_text(None, None), (String::new(), ""));
    }
}
