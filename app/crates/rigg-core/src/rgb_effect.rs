//! Programvaredrevne farger for kortets ene lyssone. Hvert bilde er en
//! vanlig statisk forespørsel, beregnet fra tiden siden brukeren trykket Behold.
use crate::rgb::{Brightness, Request};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Breathing,
    Rainbow,
    Cycle,
    /// Samme faste farge skrevet på nytt med jevne mellomrom, for kort som
    /// selv faller tilbake fra «Av» etter noen sekunder (keepalive). Bare
    /// mens appen eier lyset, som de andre effektene.
    Hold,
}

/// Hvor ofte «Hold» skriver den faste fargen på nytt.
pub const HOLD_INTERVAL_MS: u64 = 2_000;

impl Effect {
    pub fn frame(self, color: [u8; 3], brightness: Brightness, elapsed_ms: u64) -> Request {
        let color = match self {
            Self::Hold => color,
            Self::Breathing => {
                // Fire sekunder per pust. Demp RGB-kanalene sammen for å
                // beholde fargetonen og den valgte globale lysstyrkegrensen.
                let phase = (elapsed_ms % 4000) as f64 * std::f64::consts::TAU / 4000.0;
                let level = 0.1 + 0.9 * (1.0 + phase.cos()) / 2.0;
                color.map(|channel| (f64::from(channel) * level).round() as u8)
            }
            Self::Rainbow | Self::Cycle => {
                let time = (elapsed_ms % 12_000) as f64 / 2000.0;
                let offset = if self == Self::Cycle {
                    time.floor()
                } else {
                    time
                };
                spectrum(hue(color) + offset)
            }
        };
        Request::Static { color, brightness }
    }
}

// Hue i seks sektorer. En nøytral startfarge har ingen fargetone og gir rød start.
fn hue([r, g, b]: [u8; 3]) -> f64 {
    let max = r.max(g).max(b);
    let range = f64::from(max - r.min(g).min(b));
    if range == 0.0 {
        return 0.0;
    }
    let [red, green, blue] = [r, g, b].map(f64::from);
    if max == r {
        ((green - blue) / range).rem_euclid(6.0)
    } else if max == g {
        2.0 + (blue - red) / range
    } else {
        4.0 + (red - green) / range
    }
}

fn spectrum(hue: f64) -> [u8; 3] {
    let h = hue.rem_euclid(6.0);
    let up = (255.0 * h.fract()).round() as u8;
    let down = 255 - up;
    match h as u8 {
        0 => [255, up, 0],
        1 => [down, 255, 0],
        2 => [0, 255, up],
        3 => [0, down, 255],
        4 => [up, 0, 255],
        _ => [255, 0, down],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(effect: Effect, start: [u8; 3], ms: u64) -> [u8; 3] {
        let limit = Brightness::try_from(37).unwrap();
        match effect.frame(start, limit, ms) {
            Request::Static { color, brightness } if brightness == limit => color,
            _ => panic!("En ramme må beholde brukerens lysstyrke"),
        }
    }

    #[test]
    fn pust_dempe_og_returnere_til_valgt_farge() {
        let c = [40, 120, 200];
        assert_eq!(color(Effect::Breathing, c, 0), c);
        assert_eq!(color(Effect::Breathing, c, 2000), [4, 12, 20]);
        assert_eq!(color(Effect::Breathing, c, 4000), c);
    }

    #[test]
    fn hold_gir_alltid_samme_faste_farge() {
        for ms in [0, 1999, 2000, 60_000] {
            assert_eq!(color(Effect::Hold, [0, 0, 0], ms), [0, 0, 0]);
            assert_eq!(color(Effect::Hold, [9, 8, 7], ms), [9, 8, 7]);
        }
        assert_eq!(serde_json::to_value(Effect::Hold).unwrap(), "hold");
    }

    #[test]
    fn fargesyklus_holder_trinn_mens_regnbuen_glir() {
        let start = [0, 0, 255];
        assert_eq!(color(Effect::Cycle, start, 1999), start);
        assert_eq!(color(Effect::Cycle, start, 2000), [255, 0, 255]);
        assert_eq!(color(Effect::Rainbow, start, 1000), [128, 0, 255]);
        assert_eq!(color(Effect::Rainbow, start, 12000), start);
        assert_eq!(color(Effect::Rainbow, [255; 3], 0), [255, 0, 0]);
    }
}
