//! Modellspesifikke CPU-fakta fra produsenten. Dette er oppslag, ikke
//! maskinvaremålinger. Familie/modell fra CPUID identifiserer ikke en SKU,
//! så treff krever både riktig leverandør og et helt, kjent modellnavn.

/// Referansedata for én eksakt CPU-modell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuSpec {
    pub name: &'static str,
    pub launched: &'static str,
    pub source_url: &'static str,
    vendor: &'static str,
    aliases: &'static [&'static str],
}

const SPECS: &[CpuSpec] = &[
    CpuSpec {
        name: "AMD Ryzen 7 2700X",
        launched: "19. april 2018",
        source_url: "https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-2000-series/amd-ryzen-7-2700x.html",
        vendor: "AuthenticAMD",
        aliases: &["AMD Ryzen 7 2700X Eight-Core Processor", "AMD Ryzen 7 2700X", "Ryzen 7 2700X"],
    },
];

fn normalize_name(name: &str) -> String {
    name.replace(['™', '®'], "").split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

/// Hele modellnavnet må passe et eksplisitt alias. Ingen delstrengtreff
/// eller CPUID-fallback: ukjente modeller får ingen av naboens fakta.
pub fn lookup(vendor: &str, name: &str) -> Option<&'static CpuSpec> {
    let name = normalize_name(name);
    SPECS.iter().find(|s| s.vendor.eq_ignore_ascii_case(vendor.trim()) && s.aliases.iter().any(|a| normalize_name(a) == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eksakt_modell_med_fullt_eller_kort_navn() {
        for name in ["AMD Ryzen 7 2700X Eight-Core Processor", "AMD Ryzen 7 2700X", "Ryzen 7 2700X", "  amd Ryzen™  7\t2700x  "] {
            let spec = lookup("AuthenticAMD", name).expect("kjent 2700X-navn");
            assert_eq!(spec.launched, "19. april 2018");
            assert_eq!(spec.name, "AMD Ryzen 7 2700X");
            assert!(spec.source_url.ends_with("/amd-ryzen-7-2700x.html"));
        }
    }

    #[test]
    fn nabomodeller_og_ukjent_leverandoer_arver_ikke_datoen() {
        for name in [
            "AMD Ryzen 7 2700 Eight-Core Processor",
            "AMD Ryzen 7 2700U with Radeon Vega Mobile Gfx",
            "AMD Ryzen 7 PRO 2700X",
            "AMD Ryzen 7 2700X PRO",
            "AMD Ryzen 7 2700XPRO",
            "AMD Ryzen 7 2700X Eight-Core Processor PRO",
            "AMD Ryzen 7 2700XT",
            "Pinnacle Ridge",
            "",
        ] {
            assert!(lookup("AuthenticAMD", name).is_none(), "ukjent SKU: {name}");
        }
        for vendor in ["GenuineIntel", "", "Ukjent"] {
            assert!(lookup(vendor, "AMD Ryzen 7 2700X Eight-Core Processor").is_none());
        }
    }
}
