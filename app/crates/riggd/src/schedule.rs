//! Takten (arkitektur.md 4.3): den raskeste klienten bestemmer, men aldri
//! under 500 ms. Et minimert vindu ber om sakte prøver (2 s). Uten klienter
//! gjelder «Takt i bakgrunnen» (1, 2 eller 5 s, standard 2 s). Mens loggen
//! går, er takten minst så rask som loggintervallet (aldri under 500 ms).

use crate::session::Session;
use rigg_core::proto::MIN_INTERVAL_MS;

/// Takten når vinduet er minimert eller skjult.
pub const HIDDEN_INTERVAL_MS: u32 = 2000;

/// Leser tjenesten de trege verdiene i full takt (hvert 2. og 5. s)? Bare
/// når et vindu som synes abonnerer på prøvene, eller en logg går. Ellers
/// (bakgrunn, minimert vindu) leses de hvert 10. og 30. s, og verdiene bare
/// vinduet viser (viftestyring og målfart), ikke i det hele tatt (runde 3C).
pub fn detailed<'a>(mut sessions: impl Iterator<Item = &'a Session>, logging: bool) -> bool {
    logging || sessions.any(|s| s.wants("telemetry") && !s.hidden())
}

pub fn interval<'a>(sessions: impl Iterator<Item = &'a Session>, background_ms: u32, log_ms: Option<u32>) -> u32 {
    let mut best: Option<u32> = None;
    for s in sessions {
        if !s.wants("telemetry") {
            continue;
        }
        let want = if s.hidden() { HIDDEN_INTERVAL_MS.max(s.interval_ms.unwrap_or(1000)) } else { s.interval_ms.unwrap_or(1000) };
        best = Some(best.map_or(want, |b| b.min(want)));
    }
    let iv = best.unwrap_or(background_ms);
    let iv = match log_ms {
        Some(l) => iv.min(l),
        None => iv,
    };
    iv.max(MIN_INTERVAL_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(iv: u32, focus: Option<Vec<String>>) -> Session {
        let mut x = Session::new(1, None);
        x.hello = true;
        x.topics.insert("telemetry".into());
        x.interval_ms = Some(iv);
        x.focus = focus;
        x
    }

    #[test]
    fn raskeste_vinner_men_aldri_over_2_hz() {
        assert_eq!(interval([s(1000, None), s(2000, None)].iter(), 2000, None), 1000);
        assert_eq!(interval([s(100, None)].iter(), 2000, None), 500);
        assert_eq!(interval([].iter(), 5000, None), 5000);
        assert_eq!(interval([s(1000, Some(vec![]))].iter(), 5000, None), 2000, "minimert: sakte");
        assert_eq!(interval([s(1000, Some(vec!["oversikt".into()]))].iter(), 5000, None), 1000);
    }

    #[test]
    fn trege_verdier_i_full_takt_bare_med_vindu_som_synes_eller_logg() {
        assert!(!detailed([].iter(), false), "bakgrunn");
        assert!(detailed([].iter(), true), "loggen går");
        assert!(detailed([s(1000, None)].iter(), false), "vindu");
        assert!(detailed([s(1000, Some(vec!["oversikt".into()]))].iter(), false));
        assert!(!detailed([s(1000, Some(vec![]))].iter(), false), "minimert");
        let mut uten = s(1000, None);
        uten.topics.clear();
        assert!(!detailed([uten].iter(), false), "klient uten prøver (systemfeltet, rigg-cli)");
    }

    #[test]
    fn loggen_bestemmer_takten_når_den_er_raskere() {
        assert_eq!(interval([].iter(), 2000, Some(1000)), 1000, "1 s-logg uten vindu");
        assert_eq!(interval([].iter(), 2000, Some(10_000)), 2000, "sakte logg senker ikke takten");
        assert_eq!(interval([s(1000, Some(vec![]))].iter(), 5000, Some(500)), 500, "minimert vindu og 0,5 s-logg");
        assert_eq!(interval([].iter(), 5000, Some(100)), 500, "aldri over 2 Hz");
    }
}
