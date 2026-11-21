//! Oversettelser og språkvalg for vinduet og brukertjenesten.

use std::collections::HashMap;
use std::fmt::{Display, Write};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Nb,
    En,
    De,
    Fr,
    Es,
    PtBr,
    It,
    Pl,
    ZhCn,
    Ja,
}

impl Language {
    pub const ALL: [Self; 10] = [
        Self::Nb, Self::En, Self::De, Self::Fr, Self::Es, Self::PtBr, Self::It,
        Self::Pl, Self::ZhCn, Self::Ja,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Nb => "nb",
            Self::En => "en",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Es => "es",
            Self::PtBr => "pt-BR",
            Self::It => "it",
            Self::Pl => "pl",
            Self::ZhCn => "zh-CN",
            Self::Ja => "ja",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Nb => "Norsk bokmål",
            Self::En => "English",
            Self::De => "Deutsch",
            Self::Fr => "Français",
            Self::Es => "Español",
            Self::PtBr => "Português (Brasil)",
            Self::It => "Italiano",
            Self::Pl => "Polski",
            Self::ZhCn => "简体中文",
            Self::Ja => "日本語",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lang| lang.code() == code)
    }

    fn from_locale(locale: &str) -> Option<Self> {
        let tag = locale.trim().split(['.', '@']).next()?.replace('_', "-").to_ascii_lowercase();
        let mut parts = tag.split('-');
        let primary = parts.next()?;
        match primary {
            "nb" | "nn" | "no" => Some(Self::Nb),
            "pt" if parts.next().is_none_or(|region| region == "br") => Some(Self::PtBr),
            "zh" if parts.next().is_none_or(|tag| matches!(tag, "hans" | "cn" | "sg")) => Some(Self::ZhCn),
            _ => Self::from_code(primary),
        }
    }

    fn plural_index(self, count: u128) -> usize {
        match self {
            Self::ZhCn | Self::Ja => 0,
            Self::Fr | Self::PtBr => usize::from(count > 1),
            Self::Pl if count == 1 => 0,
            Self::Pl if matches!(count % 10, 2..=4) && !matches!(count % 100, 12..=14) => 1,
            Self::Pl => 2,
            _ => usize::from(count != 1),
        }
    }
}

pub const SYSTEM: &str = "system";

fn detect(language: Option<&str>, messages_locale: Option<&str>) -> Language {
    language
        .and_then(|list| list.split(':').find_map(Language::from_locale))
        .or_else(|| messages_locale.and_then(Language::from_locale))
        .unwrap_or(Language::En)
}

pub fn resolve(preference: &str) -> Language {
    if let Some(lang) = Language::from_code(preference) {
        return lang;
    }
    let language = std::env::var("LANGUAGE").ok();
    let messages = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty());
    detect(language.as_deref(), messages.as_deref())
}

/// Leser bare språkfeltet fra vinduets innstillinger. Brukertjenesten kan
/// bruke dette ved oppstart uten å endre tjenestens protokoll.
pub fn read_preference(profile: crate::paths::Profile) -> String {
    let path = crate::paths::config_dir(profile).join("ui.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json["language"].as_str().and_then(Language::from_code))
        .map_or_else(|| SYSTEM.to_owned(), |lang| lang.code().to_owned())
}

static ACTIVE: OnceLock<Language> = OnceLock::new();

/// Kalles før første brukerflate eller varsel i hver prosess.
pub fn init(preference: &str) {
    ACTIVE.get_or_init(|| resolve(preference));
}

/// Bokmål før oppstart, blant annet i eksisterende kjernetester.
pub fn active() -> Language {
    ACTIVE.get().copied().unwrap_or_default()
}

type MsgKey = (&'static str, &'static str, &'static str);
type Forms = &'static [&'static str];
type Catalog = HashMap<MsgKey, Forms>;

include!(concat!(env!("OUT_DIR"), "/i18n.rs"));

fn forms(lang: Language, key: MsgKey) -> Option<Forms> {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    if lang == Language::Nb {
        return None;
    }
    lookup(CATALOG.get_or_init(|| catalog(lang).iter().copied().collect()), key)
}

/// Katalogene ligger per kildefil. Tekst i en nestet modul (for eksempel
/// `mod imp` i en GTK-widget) har en lengre `module_path!()` enn fila, så
/// oppslaget går opp til foreldremodulen til det finnes et treff.
fn lookup(catalog: &Catalog, (module, context, source): MsgKey) -> Option<Forms> {
    let mut module = module;
    loop {
        if let Some(forms) = catalog.get(&(module, context, source)) {
            return Some(*forms);
        }
        module = &module[..module.rfind("::")?];
    }
}

#[doc(hidden)]
pub fn text(module: &'static str, context: &'static str, source: &'static str) -> &'static str {
    forms(active(), (module, context, source))
        .and_then(|forms| forms.first().copied())
        .unwrap_or(source)
}

#[doc(hidden)]
pub fn plural(module: &'static str, context: &'static str, one: &'static str, many: &'static str, count: i128) -> &'static str {
    let lang = active();
    forms(lang, (module, context, one))
        .and_then(|forms| forms.get(lang.plural_index(count.unsigned_abs())).copied())
        .unwrap_or(if count == 1 { one } else { many })
}

#[doc(hidden)]
pub fn interpolate(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(next) = tail.strip_prefix("{{") {
            out.push('{');
            rest = next;
        } else if let Some(next) = tail.strip_prefix("}}") {
            out.push('}');
            rest = next;
        } else if let Some((name, next)) = tail.strip_prefix('{').and_then(|s| s.split_once('}')) {
            if let Some((_, val)) = args.iter().find(|(key, _)| *key == name) {
                let _ = write!(out, "{val}");
            } else {
                out.push_str(&tail[..name.len() + 2]);
            }
            rest = next;
        } else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Statisk tekst. Valgfri kontekst står først, som i `tr!("diskstatus", "Aktiv")`.
#[macro_export]
macro_rules! tr {
    ($source:literal $(,)?) => {
        $crate::i18n::text(module_path!(), "", $source)
    };
    ($context:literal, $source:literal $(,)?) => {
        $crate::i18n::text(module_path!(), $context, $source)
    };
}

/// Formatert tekst. Bruk navngitte plassholdere. Valgfri kontekst står først.
#[macro_export]
macro_rules! trf {
    ($source:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate($crate::i18n::text(module_path!(), "", $source), &[$((stringify!($name), &$value)),+])
    };
    ($context:literal, $source:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate($crate::i18n::text(module_path!(), $context, $source), &[$((stringify!($name), &$value)),+])
    };
}

/// Flertall. Med navngitte argumenter formateres teksten etter oppslaget.
#[macro_export]
macro_rules! trn {
    ($count:expr, $one:literal, $many:literal $(,)?) => {
        $crate::i18n::plural(module_path!(), "", $one, $many, $count as i128)
    };
    ($count:expr, $context:literal, $one:literal, $many:literal $(,)?) => {
        $crate::i18n::plural(module_path!(), $context, $one, $many, $count as i128)
    };
    ($count:expr, $one:literal, $many:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate($crate::i18n::plural(module_path!(), "", $one, $many, $count as i128), &[$((stringify!($name), &$value)),+])
    };
    ($count:expr, $context:literal, $one:literal, $many:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::interpolate($crate::i18n::plural(module_path!(), $context, $one, $many, $count as i128), &[$((stringify!($name), &$value)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miljøspråk_og_reservevalg() {
        assert_eq!(detect(Some("ru:nn_NO:fr"), Some("de_DE")), Language::Nb);
        assert_eq!(detect(Some("ru:fr_CA"), Some("nb_NO")), Language::Fr);
        assert_eq!(detect(None, Some("no_NO.UTF-8")), Language::Nb);
        assert_eq!(detect(None, Some("pt_BR.UTF-8")), Language::PtBr);
        assert_eq!(detect(None, Some("zh_Hans_CN")), Language::ZhCn);
        assert_eq!(detect(None, Some("zh_TW")), Language::En);
        assert_eq!(detect(None, Some("C.UTF-8")), Language::En);
        assert_eq!(detect(None, None), Language::En);
    }

    #[test]
    fn flertallsregler() {
        for (n, form) in [(0, 2), (1, 0), (2, 1), (12, 2), (22, 1), (101, 2)] {
            assert_eq!(Language::Pl.plural_index(n), form);
        }
        assert_eq!(Language::Ja.plural_index(9), 0);
        assert_eq!(Language::Fr.plural_index(0), 0);
        assert_eq!(Language::PtBr.plural_index(2), 1);
    }

    #[test]
    fn nestet_modul_finner_filens_katalog() {
        static FORMS: [&str; 1] = ["Emergency"];
        let catalog: Catalog = [(("agentic_rigg::pages::viftekurve", "", "Nødregel"), &FORMS[..])].into_iter().collect();
        assert_eq!(lookup(&catalog, ("agentic_rigg::pages::viftekurve::edit_imp", "", "Nødregel")), Some(&FORMS[..]));
        assert_eq!(lookup(&catalog, ("agentic_rigg::pages::viftekurve", "", "Nødregel")), Some(&FORMS[..]));
        assert_eq!(lookup(&catalog, ("agentic_rigg::pages::viftekurve::edit_imp", "", "Annet")), None);
        assert_eq!(lookup(&catalog, ("agentic_rigg::pages::kontroller", "", "Nødregel")), None);
    }

    #[test]
    fn format_og_reservetekst() {
        assert_eq!(interpolate("{b} {{x}} {a} {b}", &[("a", &1), ("b", &"tekst")]), "tekst {x} 1 tekst");
        assert_eq!(text("rigg_core::i18n", "Visning", "Aktiv"), "Aktiv");
        assert_eq!(plural("rigg_core::i18n", "", "disk", "disker", 2), "disker");
    }
}
