//! Påskeeggene: tekst og logikk (port av `web/js/easter.js`, fasit i
//! `docs/paaskeegg.md`, endringene i produktspesifikasjonen kap. 7).
//!
//! Reglene alle eggene følger: de endrer aldri et målt tall, sender ingenting
//! til tjenesten eller kortet, har ingen lyd, dialog eller blinking, og bruker
//! aksent og dempet tekst (ingen gul, oransje eller rød). Bryteren «Små
//! overraskelser» slår av alt. Det du har funnet, lagres i `ui.json` (`eggsFound`).
//!
//! Krokene i vinduet står i `rigg/src/easter.rs`; sidene kaller én funksjon hver.

use crate::i18n::{self, Language};
use crate::{tr, trf, trn};

pub struct Egg {
    pub id: &'static str,
    pub name: &'static str,
    pub trigger: &'static str,
    pub what: &'static str,
    pub hint: &'static str,
}

pub const EGGS: [Egg; 9] = [
    Egg {
        id: "konami",
        name: "Juksekoden",
        trigger: "↑ ↑ ↓ ↓ ← → ← → B A, hvor som helst utenfor et tekstfelt",
        what: "Varsel: «Juksekode godtatt. +30 liv.» Ingenting annet endres.",
        hint: "Noen koder husker man fra åttitallet. Opp, opp …",
    },
    Egg {
        id: "stov",
        name: "Støvtørking",
        trigger: "Fem raske klikk på appikonet øverst i sidefeltet",
        what: "Ikonet snurrer to runder og bremser opp (bare ved full bevegelse), og et varsel sier at støvet er blåst av.",
        hint: "Appikonet har en vifte. Vifter samler støv.",
    },
    Egg {
        id: "utvikler",
        name: "Utviklermodus",
        trigger: "Sju klikk på versjonen i Innstillinger › Om",
        what: "Nedtelling under versjonen, så «Du er nå utvikler» og en rad med hvor mange egg du har funnet.",
        hint: "På telefoner blir man utvikler ved å trykke på byggnummeret. Hvor står versjonen her?",
    },
    Egg {
        id: "svaret",
        name: "Svaret på alt",
        trigger: "GPU-temperaturen er nøyaktig 42 °C (Oversikt)",
        what: "Temperaturen får en prikket understrek i aksentfarge og et verktøytips om livet, universet og alt.",
        hint: "Ett tall er viktigere enn alle andre. Følg med på GPU-temperaturen.",
    },
    Egg {
        id: "bursdag",
        name: "Bursdag",
        trigger: "20. september på Grafikkort, når kortet er et RTX 2080",
        what: "Raden «Lansert» får et kakeikon og «8 år i dag».",
        hint: "Kortet har bursdag én gang i året. Se etter lanseringen på Grafikkort.",
    },
    Egg {
        id: "natt",
        name: "Nattevakt",
        trigger: "Klokka 02:00–04:59",
        what: "En liten måne ved appnavnet øverst i sidefeltet, med et verktøytips.",
        hint: "Noen detaljer viser seg bare når resten av huset sover.",
    },
    Egg {
        id: "kaffe",
        name: "Kaffe",
        trigger: "«kaffe» eller «hei» i filteret på Sensorer",
        what: "Tomtilstanden svarer med en egen setning i stedet for «Ingen sensorer passer filteret».",
        hint: "Filteret på Sensorer leter etter mer enn sensorer. Hva drikker du nå?",
    },
    Egg {
        id: "kallenavn",
        name: "Kallenavn på kurven",
        trigger: "Alle punktene i utkastet på Vifter på 100 % (eller alle på 35 %)",
        what: "Tittelen sier «Kurve · Jetmotor» (eller «Kurve · Bibliotek»).",
        hint: "En viftekurve som står rett på taket, fortjener et navn.",
    },
    Egg {
        id: "konsoll",
        name: "Hilsen i terminalen",
        trigger: "Start appen fra en terminal, eller kjør agentic-rigg --hint",
        what: "En hilsen på standard ut, og --hint gir et hint om et egg du ikke har funnet.",
        hint: "Du fant denne. Godt jobbet.",
    },
];

pub const IDS: [&str; 9] = ["konami", "stov", "utvikler", "svaret", "bursdag", "natt", "kaffe", "kallenavn", "konsoll"];

pub fn egg(id: &str) -> Option<&'static Egg> {
    EGGS.iter().find(|e| e.id == id)
}

pub fn egg_hint(id: &str) -> &'static str {
    match id {
        "konami" => tr!("Noen koder husker man fra åttitallet. Opp, opp …"),
        "stov" => tr!("Appikonet har en vifte. Vifter samler støv."),
        "utvikler" => tr!("På telefoner blir man utvikler ved å trykke på byggnummeret. Hvor står versjonen her?"),
        "svaret" => tr!("Ett tall er viktigere enn alle andre. Følg med på GPU-temperaturen."),
        "bursdag" => tr!("Kortet har bursdag én gang i året. Se etter lanseringen på Grafikkort."),
        "natt" => tr!("Noen detaljer viser seg bare når resten av huset sover."),
        "kaffe" => tr!("Filteret på Sensorer leter etter mer enn sensorer. Hva drikker du nå?"),
        "kallenavn" => tr!("En viftekurve som står rett på taket, fortjener et navn."),
        "konsoll" => tr!("Du fant denne. Godt jobbet."),
        _ => "",
    }
}

// ---- 1 · Juksekoden ----

/// Tastene i rekkefølge, som navn fra GDK (`Up`, `b` …).
pub const KONAMI: [&str; 10] = ["Up", "Up", "Down", "Down", "Left", "Right", "Left", "Right", "b", "a"];

#[derive(Default)]
pub struct Konami {
    at: usize,
}

impl Konami {
    /// Gir `true` når hele koden er tastet.
    pub fn key(&mut self, name: &str) -> bool {
        let k = if name.chars().count() == 1 { name.to_lowercase() } else { name.to_string() };
        if k == KONAMI[self.at] {
            self.at += 1;
        } else if k == KONAMI[0] {
            // «Opp opp opp»: de to siste er fortsatt starten på koden.
            self.at = if self.at == 2 { 2 } else { 1 };
        } else {
            self.at = 0;
        }
        if self.at == KONAMI.len() {
            self.at = 0;
            return true;
        }
        false
    }
}

pub const KONAMI_TITLE: &str = "Juksekode godtatt";
pub const KONAMI_BODY: &str = "+30 liv. Klokkene, effekten og viften er som før – kortet får aldri vite om dette.";

pub fn konami_title() -> &'static str {
    tr!("Juksekode godtatt")
}

pub fn konami_body() -> &'static str {
    tr!("+30 liv. Klokkene, effekten og viften er som før – kortet får aldri vite om dette.")
}

// ---- 2 · Støvtørking ----

pub const DUST_TITLE: &str = "Pff!";
pub const DUST_BODY: &str = "Støvet er blåst av viften. Bare på ikonet – den ekte viften går som før.";

pub fn dust_title() -> &'static str {
    tr!("Pff!")
}

pub fn dust_body() -> &'static str {
    tr!("Støvet er blåst av viften. Bare på ikonet – den ekte viften går som før.")
}

/// Fem klikk innen 2 s. `clicks` er tidspunktene (ms) for tidligere klikk.
pub fn dust_click(clicks: &mut Vec<i64>, now: i64) -> bool {
    clicks.retain(|t| now - t < 2000);
    clicks.push(now);
    if clicks.len() >= 5 {
        clicks.clear();
        return true;
    }
    false
}

// ---- 3 · Utviklermodus ----

pub const DEV_CLICKS: u32 = 7;

/// Teksten under versjonen etter klikk nummer `n` (de to første sier ingenting).
pub fn dev_whisper(n: u32, already: bool) -> Option<String> {
    if n < 3 {
        return None;
    }
    if already {
        return Some(tr!("Du er allerede utvikler.").into());
    }
    let left = DEV_CLICKS.saturating_sub(n);
    Some(match left {
        0 => tr!("Du er nå utvikler. Det var du nok fra før.").into(),
        1 => tr!("Ett klikk til, så er du utvikler.").into(),
        2 => tr!("To klikk til, så er du utvikler.").into(),
        3 => tr!("Tre klikk til, så er du utvikler.").into(),
        4 => tr!("Fire klikk til, så er du utvikler.").into(),
        5 => tr!("Fem klikk til, så er du utvikler.").into(),
        _ => tr!("Seks klikk til, så er du utvikler.").into(),
    })
}

// ---- 4 · Svaret på alt ----

/// Den viste temperaturen (hele grader, °C) er nøyaktig 42.
pub fn is_42(shown_celsius_rounded: Option<i64>, unit_is_c: bool) -> bool {
    unit_is_c && shown_celsius_rounded == Some(42)
}

pub const ANSWER_TIP: &str = "42 °C – svaret på livet, universet og alt. Målingen er ekte; det er bare tallet som er morsomt.";

pub fn answer_tip() -> &'static str {
    tr!("42 °C – svaret på livet, universet og alt. Målingen er ekte; det er bare tallet som er morsomt.")
}

// ---- 5 · Bursdag ----

/// «8 år i dag» når kortet er et RTX 2080 og datoen er 20. september.
pub fn launch_day(card_name: &str, year: i32, month: u32, day: u32) -> Option<(String, String)> {
    let n = card_name.to_ascii_lowercase();
    let is_2080 = n.contains("rtx 2080") && !n.contains("2080 ti") && !n.contains("2080 super");
    if !is_2080 || month != 9 || day != 20 {
        return None;
    }
    let years = year - 2018;
    (years >= 1).then(|| (
        trn!(years, "{ar} år i dag", "{ar} år i dag", ar = years),
        tr!("Gratulerer med dagen! GeForce RTX 2080 kom i butikkene 20. september 2018.").into(),
    ))
}

// ---- 6 · Nattevakt ----

pub fn is_night(hour: u32) -> bool {
    (2..5).contains(&hour)
}

pub const NIGHT_TIP: &str = "Sent oppe? Skjermkortet er våkent det også.";

pub fn night_tip() -> &'static str {
    tr!("Sent oppe? Skjermkortet er våkent det også.")
}

// ---- 7 · Kaffe ----

pub fn filter_quip(filter: &str) -> Option<&'static str> {
    match filter_kind(filter, i18n::active())? {
        FilterQuip::Coffee => Some(tr!("Ingen sensor måler kaffe ennå. Det nærmeste er CPU-temperaturen.")),
        FilterQuip::Greeting => Some(tr!("Hei! Ingen sensor heter det, men takk for at du spør.")),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FilterQuip {
    Coffee,
    Greeting,
}

fn filter_kind(filter: &str, language: Language) -> Option<FilterQuip> {
    use FilterQuip::{Coffee, Greeting};
    match (language, filter.trim().to_lowercase().as_str()) {
        (_, "kaffe" | "kaffi" | "kaffen" | "coffee")
        | (Language::De, "kaffee")
        | (Language::Fr | Language::Es | Language::PtBr, "café")
        | (Language::It, "caffè")
        | (Language::Pl, "kawa")
        | (Language::ZhCn, "咖啡")
        | (Language::Ja, "コーヒー") => Some(Coffee),
        (_, "hei" | "hallo" | "heisann" | "hei hei" | "hoi")
        | (Language::En, "hello")
        | (Language::Fr, "bonjour" | "salut")
        | (Language::Es, "hola")
        | (Language::PtBr, "olá")
        | (Language::It, "ciao")
        | (Language::Pl, "cześć")
        | (Language::ZhCn, "你好")
        | (Language::Ja, "こんにちは") => Some(Greeting),
        _ => None,
    }
}

// ---- 8 · Kallenavn på kurven ----

pub fn curve_nickname(points: &[(f64, f64)]) -> Option<(&'static str, &'static str)> {
    if points.len() < 2 {
        return None;
    }
    if points.iter().all(|p| p.1 >= 100.0) {
        return Some((tr!("Jetmotor"), tr!("Alle punktene står på 100 %. Naboene hilser.")));
    }
    if points.iter().all(|p| p.1 <= 35.0) {
        return Some((tr!("Bibliotek"), tr!("Alle punktene står på 35 %, så lavt kortet tillater. Hysj.")));
    }
    None
}

// ---- 9 · Hilsen i terminalen ----

pub fn greeting() -> String {
    trf!(
        "{navn} {versjon} · bygg {bygg}\nHei, nysgjerrige utvikler. Alt her leser bare; ingenting sendes til skjermkortet.\nDet finnes {antall} små overraskelser – agentic-rigg --hint gir et hint.",
        navn = crate::version::NAME,
        versjon = crate::version::VERSION,
        bygg = crate::version::BUILD_STR,
        antall = EGGS.len(),
    )
}

/// Et hint om et egg som ikke er funnet. `pick` velger blant dem (0..).
pub fn hint(found: &[String], pick: usize) -> String {
    let left: Vec<&Egg> = EGGS.iter().filter(|e| !found.iter().any(|f| f == e.id)).collect();
    if left.is_empty() {
        return trf!("Du har funnet alle {antall}. Fasiten står i docs/paaskeegg.md.", antall = EGGS.len());
    }
    let e = left[pick % left.len()];
    trf!("{hint} ({funnet} av {totalt} funnet)", hint = egg_hint(e.id), funnet = found.len(), totalt = EGGS.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn juksekoden() {
        let mut k = Konami::default();
        let seq = ["Up", "Up", "Down", "Down", "Left", "Right", "Left", "Right", "B", "a"];
        let hits: Vec<bool> = seq.iter().map(|s| k.key(s)).collect();
        assert!(hits[9] && !hits[..9].iter().any(|h| *h));
        let mut k = Konami::default();
        for s in ["Up", "Up", "Up", "Down", "Down", "Left", "Right", "Left", "Right", "b"] {
            assert!(!k.key(s));
        }
        assert!(k.key("a"), "en ekstra pil opp i starten ødelegger ikke");
    }

    #[test]
    fn egg_tekster() {
        assert_eq!(IDS.len(), EGGS.len());
        assert!(IDS.iter().zip(EGGS.iter()).all(|(a, b)| *a == b.id));
        assert_eq!(dev_whisper(2, false), None);
        assert_eq!(dev_whisper(3, false).unwrap(), "Fire klikk til, så er du utvikler.");
        assert_eq!(dev_whisper(7, false).unwrap(), "Du er nå utvikler. Det var du nok fra før.");
        assert_eq!(launch_day("NVIDIA GeForce RTX 2080", 2026, 9, 20).unwrap().0, "8 år i dag");
        assert!(launch_day("NVIDIA GeForce RTX 2080 Ti", 2026, 9, 20).is_none());
        assert!(is_night(3) && !is_night(5) && !is_night(1));
        assert!(filter_quip(" Kaffe ").is_some());
        assert_eq!(curve_nickname(&[(30.0, 100.0), (80.0, 100.0)]).unwrap().0, "Jetmotor");
        assert!(is_42(Some(42), true) && !is_42(Some(42), false));
        let mut c = vec![];
        assert!(!(0..4).any(|i| dust_click(&mut c, i * 100)));
        assert!(dust_click(&mut c, 500));
        assert!(hint(&IDS.map(String::from), 0).contains("alle 9"));
    }

    #[test]
    fn sensorfilter_spøker_bare_for_hele_ord_i_riktig_språk() {
        assert_eq!(filter_kind(" CAFÉ ", Language::Fr), Some(FilterQuip::Coffee));
        assert_eq!(filter_kind("こんにちは", Language::Ja), Some(FilterQuip::Greeting));
        assert_eq!(filter_kind("café", Language::Nb), None);
        for language in Language::ALL {
            for query in ["hi", "CAFE", "caffe", "Coffee Lake", "GPU temp", "咖啡 温度"] {
                assert_eq!(filter_kind(query, language), None, "{language:?}: {query}");
            }
        }
    }
}
