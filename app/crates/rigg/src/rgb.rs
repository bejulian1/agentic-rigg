//! Visningsmotor for 2D/3D-skjermkortet på Lys-siden.
//!
//! Alle 56 lysdiodene (lyslist 20, vifte 1 18, vifte 2 18) speiler kortets ene
//! fysiske lyssone: valgt modus, farge og lysstyrke (eller sist leste farge fra
//! kortet). Effektene bruker samme regnestykker som `rigg_core::rgb_effect`,
//! med en rolig kryssfade på 250 ms ved bytte av modus eller farge.
//! Ingenting her snakker med maskinvaren.

use rigg_core::rgb::{Brightness, Request};
use rigg_core::rgb_effect::Effect;
use rigg_core::tr;

pub const BAR_LEDS: usize = 20;
pub const RING_LEDS: usize = 18;
pub const N: usize = BAR_LEDS + 2 * RING_LEDS;

/// Fargen på en lysdiode som er av (mørk diffusor).
const OFF: [f32; 3] = [26.0, 31.0, 37.0];

/// Hurtigfarge: heksadesimal kode og oversatt navn.
pub type Swatch = (&'static str, fn() -> &'static str);

/// Hurtigfarger som fyller inn fargefeltet i fysisk kortlys (sender aldri noe
/// før brukeren trykker «Bruk»).
pub const SWATCHES: [Swatch; 9] = [
    ("#F2F2F2", || tr!("Hvit")),
    ("#E5383B", || tr!("Rød")),
    ("#F07A1A", || tr!("Oransje")),
    ("#F2C12E", || tr!("Gul")),
    ("#3FB950", || tr!("Grønn")),
    ("#2BB3C8", || tr!("Cyan")),
    ("#3A7BF0", || tr!("Blå")),
    ("#8E5CF0", || tr!("Lilla")),
    ("#E2649E", || tr!("Rosa")),
];

pub fn led_zone(i: usize) -> usize {
    if i < BAR_LEDS {
        0
    } else if i < BAR_LEDS + RING_LEDS {
        1
    } else {
        2
    }
}

/// Tilstanden 2D/3D-modellen skal speile (én fysisk lyssone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewTarget {
    /// `"static"`, `"off"`, `"breathing"`, `"rainbow"` eller `"cycle"`.
    pub mode: String,
    pub color: [u8; 3],
    pub brightness: u8,
}

impl Default for PreviewTarget {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            color: [58, 123, 240],
            brightness: 80,
        }
    }
}

impl PreviewTarget {
    fn effect(&self) -> Option<Effect> {
        match self.mode.as_str() {
            "breathing" => Some(Effect::Breathing),
            "rainbow" => Some(Effect::Rainbow),
            "cycle" => Some(Effect::Cycle),
            _ => None,
        }
    }
}

/// Beregner RGB-fargen (0–255) for kortets ene fysiske lyssone og fyller alle
/// 56 lysdiodene med samme farge.
pub struct Engine {
    pub cur: [f32; N * 3],
    pub layer_k: f32,
    pub ambient: Option<[f32; 3]>,
    pub ambient_a: f32,
    /// Alltid 0 (hele kortet er én fysisk lyssone; ingen sonedemping).
    pub zone: usize,
    pub changed: bool,
    target: PreviewTarget,
    elapsed_s: f64,
    fade_from: [f32; 3],
    fade_t: f32,
}

impl Engine {
    pub fn new() -> Self {
        let mut cur = [0.0f32; N * 3];
        for i in 0..N {
            cur[i * 3..i * 3 + 3].copy_from_slice(&OFF);
        }
        Self {
            cur,
            layer_k: 1.0,
            ambient: None,
            ambient_a: 0.0,
            zone: 0,
            changed: true,
            target: PreviewTarget::default(),
            elapsed_s: 0.0,
            fade_from: OFF,
            fade_t: 1.0,
        }
    }

    pub fn set_target(&mut self, next: PreviewTarget, animate: bool) {
        if self.target == next {
            return;
        }
        let reset_phase = self.target.mode != next.mode;
        self.fade_from = [self.cur[0], self.cur[1], self.cur[2]];
        self.fade_t = if animate { 0.0 } else { 1.0 };
        if reset_phase {
            self.elapsed_s = 0.0;
        }
        self.target = next;
        self.changed = true;
    }

    fn compute_rgb(&self, animate: bool) -> ([f32; 3], Option<[f32; 3]>, f32) {
        if self.target.mode == "off" || self.target.brightness == 0 {
            return (OFF, None, 0.0);
        }
        let brightness = Brightness::try_from(self.target.brightness.min(100))
            .unwrap_or_else(|_| Brightness::try_from(100).expect("100 er gyldig"));
        let elapsed_ms = if animate {
            (self.elapsed_s * 1000.0).round().max(0.0) as u64
        } else {
            0
        };
        let raw = match self.target.effect() {
            Some(effect) => match effect.frame(self.target.color, brightness, elapsed_ms) {
                Request::Static { color, .. } => color,
                Request::Off {} => return (OFF, None, 0.0),
            },
            None => self.target.color,
        };
        let k = f32::from(brightness.get()) / 100.0;
        let shown = [
            OFF[0] + (f32::from(raw[0]) - OFF[0]) * k,
            OFF[1] + (f32::from(raw[1]) - OFF[1]) * k,
            OFF[2] + (f32::from(raw[2]) - OFF[2]) * k,
        ];
        let amb = [f32::from(raw[0]) / 255.0, f32::from(raw[1]) / 255.0, f32::from(raw[2]) / 255.0];
        let strength = (amb[0].max(amb[1]).max(amb[2]) * k).clamp(0.0, 1.0);
        (shown, Some(amb), strength * 0.55)
    }

    /// Oppdaterer lysdiodene. Returnerer `true` så lenge animasjon eller
    /// kryssfade fortsatt pågår.
    pub fn tick(&mut self, dt: f64, animate: bool) -> bool {
        let moving = animate && self.target.effect().is_some() && self.target.brightness > 0;
        if moving {
            self.elapsed_s = (self.elapsed_s + dt) % 12.0;
        }
        if self.fade_t < 1.0 {
            self.fade_t = if animate {
                (self.fade_t + (dt as f32) / 0.25).min(1.0)
            } else {
                1.0
            };
        }
        let (goal, amb, amb_a) = self.compute_rgb(animate);
        let t = self.fade_t;
        let ease = t * t * (3.0 - 2.0 * t);
        let rgb = [
            self.fade_from[0] + (goal[0] - self.fade_from[0]) * ease,
            self.fade_from[1] + (goal[1] - self.fade_from[1]) * ease,
            self.fade_from[2] + (goal[2] - self.fade_from[2]) * ease,
        ];
        let prev = [self.cur[0], self.cur[1], self.cur[2]];
        if (rgb[0] - prev[0]).abs() > 0.05
            || (rgb[1] - prev[1]).abs() > 0.05
            || (rgb[2] - prev[2]).abs() > 0.05
            || self.ambient != amb
            || (self.ambient_a - amb_a).abs() > 0.002
        {
            for i in 0..N {
                self.cur[i * 3..i * 3 + 3].copy_from_slice(&rgb);
            }
            self.ambient = amb;
            self.ambient_a = amb_a;
            self.changed = true;
        }
        moving || self.fade_t < 1.0
    }

    /// Fargen som vises på lysdiode `i` (0–255). Alle dioder følger kortets
    /// ene fysiske lyssone.
    pub fn shown(&self, i: usize) -> [f32; 3] {
        [
            self.cur[i * 3] * self.layer_k,
            self.cur[i * 3 + 1] * self.layer_k,
            self.cur[i * 3 + 2] * self.layer_k,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statisk_og_av_fyller_hele_sonen_likt() {
        let mut e = Engine::new();
        e.set_target(
            PreviewTarget {
                mode: "static".into(),
                color: [58, 123, 240],
                brightness: 100,
            },
            false,
        );
        assert!(!e.tick(0.016, true), "statisk står stille");
        assert_eq!(e.shown(0), [58.0, 123.0, 240.0]);
        assert_eq!(e.shown(25), e.shown(0));
        assert_eq!(e.shown(55), e.shown(0));

        e.set_target(
            PreviewTarget {
                mode: "off".into(),
                color: [58, 123, 240],
                brightness: 100,
            },
            false,
        );
        e.tick(0.016, false);
        assert_eq!(e.shown(0), OFF);
        assert!(e.ambient.is_none());
    }

    #[test]
    fn effektene_animerer_bare_ved_full_bevegelse() {
        let mut e = Engine::new();
        e.set_target(
            PreviewTarget {
                mode: "rainbow".into(),
                color: [58, 123, 240],
                brightness: 80,
            },
            false,
        );
        assert!(e.tick(0.016, true));
        let first = e.shown(0);
        e.tick(1.0, true);
        assert_ne!(first, e.shown(0));

        let mut still = Engine::new();
        still.set_target(
            PreviewTarget {
                mode: "breathing".into(),
                color: [58, 123, 240],
                brightness: 80,
            },
            false,
        );
        assert!(!still.tick(0.016, false));
    }
}
