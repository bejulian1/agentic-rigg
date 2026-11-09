//! RGB-motoren (port av `web/js/rgb.js`, `RgbEngine`): fargen per lysdiode
//! for effektene, til både 3D-kortet og det flate kortet. Nordlys (påskeegg
//! 10) bruker `crate::easter::aurora`.
//!
//! Motoren regner ut fargen på alle 56 lysdiodene (lyslist 20, vifte 1 18,
//! vifte 2 18), pusten (styrken på hele laget) og bakgrunnslyset. Tegnerne
//! leser [`Engine::cur`], [`Engine::layer_k`] og [`Engine::ambient`] etter
//! [`Engine::tick`]. Matematikken er den samme som i demoen: regnbue og
//! fargesyklus i trinn på 1,5°, Nordlys høyst 20 ganger i sekundet, og en
//! kryssfade på 250 ms når effekt eller farge byttes.
//!
//! **Eies av 2C (Styring og Lys).** Ingenting her snakker med kortet.

use serde_json::{Value, json};

pub const BAR_LEDS: usize = 20;
pub const RING_LEDS: usize = 18;
pub const N: usize = BAR_LEDS + 2 * RING_LEDS;
/// Fargen på en lysdiode som er av (mørk diffusor).
const OFF: [f64; 3] = [26.0, 31.0, 37.0];
/// Regnbue og fargesyklus flytter fargen i trinn på 1,5° (under det øyet skiller).
const HUE_STEP: f64 = 1.5;

/// Sonene: lyslist, vifte 1 og vifte 2 (indekser i `cur`).
pub const ZONES: [(&str, &str, usize, usize); 3] = [("bar", "Lyslist", 0, 20), ("fan1", "Vifte 1", 20, 38), ("fan2", "Vifte 2", 38, 56)];

/// Effektene: id, navn, dokumentert for kontrolleren, forklaring.
pub const EFFECTS: [(&str, &str, bool, &str); 6] = [
    ("static", "Statisk", true, "Én fast farge."),
    ("breathe", "Pust", true, "Fargen tones rolig opp og ned."),
    ("rainbow", "Regnbue", false, "Alle farger løper rundt."),
    ("cycle", "Fargesyklus", false, "Hele kortet bytter farge sakte."),
    ("temperature", "Temperatur", false, "Fargen følger GPU-temperaturen, fra kald til varm."),
    ("off", "Av", true, "Lysene er slått av."),
];

pub fn effect_name(id: &str) -> &'static str {
    EFFECTS.iter().find(|e| e.0 == id).map(|e| e.1).unwrap_or("Statisk")
}

/// Fargeprøvene (som demoens `SWATCHES`).
pub const SWATCHES: [(&str, &str); 9] = [
    ("#f2f2f2", "Hvit"),
    ("#e5383b", "Rød"),
    ("#f07a1a", "Oransje"),
    ("#f2c12e", "Gul"),
    ("#3fb950", "Grønn"),
    ("#2bb3c8", "Cyan"),
    ("#3a7bf0", "Blå"),
    ("#8e5cf0", "Lilla"),
    ("#e2649e", "Rosa"),
];

/// Plassen langs sonen (0–1) for hver lysdiode, som demoens `LEDS[i].u`.
pub fn led_u(i: usize) -> f64 {
    if i < BAR_LEDS {
        i as f64 / (BAR_LEDS - 1) as f64
    } else if i < BAR_LEDS + RING_LEDS {
        (i - BAR_LEDS) as f64 / RING_LEDS as f64 / 2.0
    } else {
        0.5 + (i - BAR_LEDS - RING_LEDS) as f64 / RING_LEDS as f64 / 2.0
    }
}

pub fn led_zone(i: usize) -> usize {
    if i < 20 {
        0
    } else if i < 38 {
        1
    } else {
        2
    }
}

pub fn hex_to_rgb(hex: &str) -> [f64; 3] {
    let h = hex.trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).unwrap_or(0x3a7bf0);
    [((n >> 16) & 255) as f64, ((n >> 8) & 255) as f64, (n & 255) as f64]
}

pub fn rgb_to_hex(c: [f64; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0].round().clamp(0.0, 255.0) as u8, c[1].round().clamp(0.0, 255.0) as u8, c[2].round().clamp(0.0, 255.0) as u8)
}

pub fn valid_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    rigg_core::easter::hsl_to_rgb(h, s, l)
}

pub fn rgb_to_hue(c: [f64; 3]) -> f64 {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    if d == 0.0 {
        return 0.0;
    }
    let h = if mx == r {
        ((g - b) / d) % 6.0
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0 + 360.0) % 360.0
}

/// HSL fra RGB (0–255): (fargetone 0–360, metning 0–1, lyshet 0–1).
pub fn rgb_to_hsl(c: [f64; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    (rgb_to_hue(c), s.clamp(0.0, 1.0), l)
}

// ================================================================ lysvalget

/// Lysvalget («Lagre i appen»), med demoens nøkler og grenser.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbConfig {
    pub color: String,
    /// bar, fan1, fan2
    pub zones: [String; 3],
    pub effect: String,
    pub brightness: f64,
    pub speed: f64,
    /// «cw» eller «ccw».
    pub direction: String,
    pub temp_cold: String,
    pub temp_hot: String,
    pub temp_min: f64,
    pub temp_max: f64,
    pub ambient: bool,
    pub recent: Vec<String>,
}

impl Default for RgbConfig {
    fn default() -> Self {
        let c = "#3a7bf0".to_string();
        RgbConfig {
            color: c.clone(),
            zones: [c.clone(), c.clone(), c],
            effect: "static".into(),
            brightness: 80.0,
            speed: 5.0,
            direction: "cw".into(),
            temp_cold: "#3a7bf0".into(),
            temp_hot: "#e5383b".into(),
            temp_min: 40.0,
            temp_max: 80.0,
            ambient: false,
            recent: vec![],
        }
    }
}

impl RgbConfig {
    /// Leser et lagret valg. Et ugyldig felt får standardverdien for seg.
    pub fn from_json(v: &Value) -> RgbConfig {
        let d = RgbConfig::default();
        let hex = |k: &Value, def: &str| k.as_str().filter(|s| valid_hex(s)).unwrap_or(def).to_string();
        let num = |k: &Value, lo: f64, hi: f64, def: f64| k.as_f64().map(|x| x.round().clamp(lo, hi)).unwrap_or(def);
        let color = hex(&v["color"], &d.color);
        let mut c = RgbConfig {
            zones: [hex(&v["zones"]["bar"], &color), hex(&v["zones"]["fan1"], &color), hex(&v["zones"]["fan2"], &color)],
            color,
            effect: v["effect"].as_str().filter(|e| EFFECTS.iter().any(|x| x.0 == *e)).unwrap_or("static").to_string(),
            brightness: num(&v["brightness"], 0.0, 100.0, d.brightness),
            speed: num(&v["speed"], 1.0, 10.0, d.speed),
            direction: if v["direction"] == "ccw" { "ccw".into() } else { "cw".into() },
            temp_cold: hex(&v["tempCold"], &d.temp_cold),
            temp_hot: hex(&v["tempHot"], &d.temp_hot),
            temp_min: num(&v["tempMin"], 20.0, 100.0, d.temp_min),
            temp_max: num(&v["tempMax"], 20.0, 100.0, d.temp_max),
            ambient: v["ambient"].as_bool().unwrap_or(false),
            recent: v["recent"].as_array().map(|a| a.iter().filter_map(Value::as_str).filter(|s| valid_hex(s)).take(6).map(String::from).collect()).unwrap_or_default(),
        };
        if c.temp_max - c.temp_min < 5.0 {
            c.temp_min = d.temp_min;
            c.temp_max = d.temp_max;
        }
        c
    }

    pub fn to_json(&self) -> Value {
        json!({
            "color": self.color,
            "zones": {"bar": self.zones[0], "fan1": self.zones[1], "fan2": self.zones[2]},
            "effect": self.effect,
            "brightness": self.brightness,
            "speed": self.speed,
            "direction": self.direction,
            "tempCold": self.temp_cold,
            "tempHot": self.temp_hot,
            "tempMin": self.temp_min,
            "tempMax": self.temp_max,
            "ambient": self.ambient,
            "recent": self.recent,
        })
    }

    /// Er valget ulikt et annet (uten «nylig brukt»)?
    pub fn differs(&self, o: &RgbConfig) -> bool {
        let mut a = self.clone();
        a.recent = o.recent.clone();
        a != *o
    }
}

// ================================================================ motoren

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// Effektmotoren. Kall [`Engine::tick`] hvert bilde mens den er i gang.
pub struct Engine {
    cfg: Option<RgbConfig>,
    base: [[f64; 3]; 3],
    base_hue: [f64; 3],
    cold: [f64; 3],
    hot: [f64; 3],
    t: f64,
    gpu_temp: Option<f64>,
    /// Fargen som vises nå (sRGB 0–255), tre tall per lysdiode.
    pub cur: [f32; N * 3],
    from: [f32; N * 3],
    xf: f64,
    has: bool,
    dirty: bool,
    /// Pust: styrken på alle lysdiodene (0–1).
    pub layer_k: f32,
    last_phase: f64,
    last_effect: &'static str,
    egg: bool,
    /// Valgt sone (0 alle, 1 lyslist, 2 vifte 1, 3 vifte 2): de andre tones til 30 %.
    pub zone: usize,
    /// Bakgrunnslyset: normalisert farge (0–1) og styrke.
    pub ambient: Option<[f32; 3]>,
    pub ambient_a: f32,
    /// Har fargene eller styrken endret seg siden sist? (Tegneren nullstiller.)
    pub changed: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine {
            cfg: None,
            base: [[0.0; 3]; 3],
            base_hue: [0.0; 3],
            cold: [0.0; 3],
            hot: [0.0; 3],
            t: 0.0,
            gpu_temp: None,
            cur: [0.0; N * 3],
            from: [0.0; N * 3],
            xf: 1.0,
            has: false,
            dirty: true,
            layer_k: 1.0,
            last_phase: f64::NAN,
            last_effect: "",
            egg: false,
            zone: 0,
            ambient: None,
            ambient_a: 0.0,
            changed: true,
        }
    }

    pub fn config(&self) -> Option<&RgbConfig> {
        self.cfg.as_ref()
    }

    fn start_fade(&mut self, animate: bool) {
        if self.has && animate {
            for j in 0..self.cur.len() {
                self.from[j] = self.cur[j] * self.layer_k;
            }
            self.xf = 0.0;
        }
    }

    /// Nytt valg. Effekt- og fargebytter kryssfades; glidere følger hånden.
    pub fn set_config(&mut self, cfg: &RgbConfig, animate: bool) {
        let prev = self.cfg.clone();
        for z in 0..3 {
            self.base[z] = hex_to_rgb(&cfg.zones[z]);
            self.base_hue[z] = rgb_to_hue(self.base[z]);
        }
        self.cold = hex_to_rgb(&cfg.temp_cold);
        self.hot = hex_to_rgb(&cfg.temp_hot);
        let swap = prev.as_ref().is_some_and(|p| p.effect != cfg.effect || p.zones != cfg.zones || p.temp_cold != cfg.temp_cold || p.temp_hot != cfg.temp_hot);
        if swap {
            self.start_fade(animate);
        }
        // Pust starter på toppen (full styrke), så det ikke blir et hopp ned.
        if cfg.effect == "breathe" && prev.as_ref().is_none_or(|p| p.effect != "breathe") {
            self.t = (6.6 - cfg.speed * 0.58) / 2.0;
        }
        self.cfg = Some(cfg.clone());
        self.dirty = true;
    }

    /// Nordlys over effekten (bare forhåndsvisningen). Kryssfades som et effektbytte.
    pub fn set_egg(&mut self, on: bool, animate: bool) {
        if self.egg == on {
            return;
        }
        self.start_fade(animate);
        self.egg = on;
        self.dirty = true;
    }

    pub fn egg(&self) -> bool {
        self.egg
    }

    pub fn set_zone(&mut self, z: usize) {
        if self.zone != z {
            self.zone = z;
            self.changed = true;
        }
    }

    pub fn set_temp(&mut self, c: Option<f64>) {
        if let Some(c) = c
            && self.gpu_temp != Some(c)
        {
            self.gpu_temp = Some(c);
            self.dirty = true;
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn temp_color(&self, cfg: &RgbConfig) -> [f64; 3] {
        let k = match self.gpu_temp {
            None => 0.0,
            Some(t) => ((t - cfg.temp_min) / (cfg.temp_max - cfg.temp_min).max(1.0)).clamp(0.0, 1.0),
        };
        [0, 1, 2].map(|i| self.cold[i] + (self.hot[i] - self.cold[i]) * k)
    }

    /// Ett bilde. `dt` i sekunder, `animate` er sann ved full bevegelse.
    /// Gir `true` så lenge lysene beveger seg (eller kryssfader).
    pub fn tick(&mut self, dt: f64, animate: bool) -> bool {
        let Some(cfg) = self.cfg.clone() else { return false };
        let effect: &'static str = if self.egg { "egg" } else { EFFECTS.iter().find(|e| e.0 == cfg.effect).map(|e| e.0).unwrap_or("static") };
        let moving = animate && matches!(effect, "breathe" | "rainbow" | "cycle" | "egg");
        let fading = self.xf < 1.0;
        if !moving && !fading && !self.dirty {
            return false;
        }
        let was_dirty = self.dirty;
        self.dirty = false;
        if moving {
            self.t += dt;
        }
        if fading {
            self.xf = if animate { (self.xf + dt * 1000.0 / 250.0).min(1.0) } else { 1.0 };
        }
        let k = if fading { ease_out(self.xf) } else { 1.0 };
        let dir = if cfg.direction == "ccw" { -1.0 } else { 1.0 };
        // Nordlys lyser også når lysene ellers er av, og aldri svakere enn 40 %.
        let bri = if effect == "egg" {
            if cfg.effect == "off" { 0.85 } else { (cfg.brightness / 100.0).clamp(0.0, 1.0).max(0.4) }
        } else {
            (cfg.brightness / 100.0).clamp(0.0, 1.0)
        };
        // Pust: animert går styrken i lagets opasitet; stillbildet er 60 %.
        let mut breath_k = 0.6;
        let mut layer_k = 1.0f32;
        if effect == "breathe" && animate {
            let period = 6.6 - cfg.speed * 0.58;
            let c = 0.5 - 0.5 * (self.t / period * std::f64::consts::TAU).cos();
            layer_k = ((0.08 + 0.92 * c * c) * 500.0).round() as f32 / 500.0;
            breath_k = 1.0;
        }
        let layer_changed = layer_k != self.layer_k;
        self.layer_k = layer_k;
        let tc = self.temp_color(&cfg);
        let phase = match effect {
            "rainbow" | "cycle" => (dir * self.t * cfg.speed * if effect == "rainbow" { 30.0 } else { 22.0 } / HUE_STEP).round() * HUE_STEP,
            "egg" => (self.t * 20.0).round() / 20.0,
            _ => 0.0,
        };
        if moving && !fading && !was_dirty && phase == self.last_phase && effect == self.last_effect {
            // Bare pusten endret seg.
            if layer_changed {
                self.push();
            }
            return true;
        }
        self.last_phase = phase;
        self.last_effect = effect;
        let scale = |c: [f64; 3], s: f64| [c[0] * s, c[1] * s, c[2] * s];
        for i in 0..N {
            let z = led_zone(i);
            let base = self.base[z];
            let u = led_u(i);
            let rgb = match effect {
                "off" => OFF,
                "static" => scale(base, bri),
                "temperature" => scale(tc, bri),
                "breathe" => scale(base, bri * breath_k),
                "rainbow" => scale(hsl_to_rgb(u * 360.0 - phase, 1.0, 0.55), bri),
                "egg" => scale(crate::easter::aurora(u, phase), bri),
                _ => scale(hsl_to_rgb(self.base_hue[z] + phase, 1.0, 0.55), bri),
            };
            let j = i * 3;
            for (c, v) in rgb.iter().enumerate() {
                let v = *v as f32;
                self.cur[j + c] = if fading { self.from[j + c] + (v - self.from[j + c]) * k as f32 } else { v };
            }
        }
        self.has = true;
        self.push();
        moving || self.xf < 1.0
    }

    /// Bakgrunnslyset: snittfargen normaliseres, så pust bare endrer styrken.
    fn push(&mut self) {
        let mut s = [0.0f32; 3];
        for led in self.cur.chunks_exact(3) {
            for (sc, v) in s.iter_mut().zip(led) {
                *sc += v;
            }
        }
        let a = s.map(|v| v / N as f32);
        let m = a[0].max(a[1]).max(a[2]);
        if m < 1.0 {
            self.ambient_a = 0.0;
        } else {
            self.ambient = Some(a.map(|v| v / m));
            self.ambient_a = m / 255.0 * self.layer_k;
        }
        self.changed = true;
    }

    /// Fargen på lysdiode `i` slik den vises nå (med pust og sonedemping), 0–255.
    pub fn shown(&self, i: usize) -> [f32; 3] {
        let dim = if self.zone == 0 || self.zone == led_zone(i) + 1 { 1.0 } else { 0.3 };
        let k = self.layer_k * dim;
        [self.cur[i * 3] * k, self.cur[i * 3 + 1] * k, self.cur[i * 3 + 2] * k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statisk_og_av() {
        let mut e = Engine::new();
        let mut c = RgbConfig { brightness: 100.0, ..Default::default() };
        e.set_config(&c, false);
        assert!(!e.tick(0.016, true), "statisk står stille");
        assert_eq!(&e.cur[0..3], &[58.0, 123.0, 240.0]);
        c.effect = "off".into();
        e.set_config(&c, false);
        e.tick(0.016, false);
        assert_eq!(&e.cur[0..3], &[26.0, 31.0, 37.0]);
    }

    #[test]
    fn regnbuen_beveger_seg_bare_ved_full_bevegelse() {
        let mut e = Engine::new();
        e.set_config(&RgbConfig { effect: "rainbow".into(), ..Default::default() }, false);
        assert!(e.tick(0.016, true));
        let a = e.cur;
        e.tick(0.5, true);
        assert_ne!(a, e.cur);
        let mut r = Engine::new();
        r.set_config(&RgbConfig { effect: "rainbow".into(), ..Default::default() }, false);
        assert!(!r.tick(0.016, false), "redusert: stillbilde");
        assert!(!r.tick(0.016, false));
    }

    #[test]
    fn temperaturen_blander_kald_og_varm() {
        let mut e = Engine::new();
        e.set_config(&RgbConfig { effect: "temperature".into(), brightness: 100.0, ..Default::default() }, false);
        e.set_temp(Some(80.0));
        e.tick(0.0, false);
        assert_eq!(&e.cur[0..3], &[229.0, 56.0, 59.0]);
        e.set_temp(Some(40.0));
        e.tick(0.0, false);
        assert_eq!(&e.cur[0..3], &[58.0, 123.0, 240.0]);
    }

    #[test]
    fn nordlys_er_aldri_svakere_enn_40_prosent_og_beveger_seg() {
        let mut e = Engine::new();
        e.set_config(&RgbConfig { brightness: 5.0, ..Default::default() }, false);
        e.tick(0.0, true);
        e.set_egg(true, false);
        assert!(e.tick(0.1, true));
        let max = e.cur.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 30.0, "{max}");
        // Ingen gul, oransje eller rød: rødt er aldri størst.
        for i in 0..N {
            let c = &e.cur[i * 3..i * 3 + 3];
            assert!(c[0] <= c[1].max(c[2]) + 1.0);
        }
    }

    #[test]
    fn lagret_valg_leses_mildt() {
        let c = RgbConfig::from_json(&json!({"effect": "turbo", "brightness": 300, "color": "#00ff00", "tempMin": 70, "tempMax": 72}));
        assert_eq!(c.effect, "static");
        assert_eq!(c.brightness, 100.0);
        assert_eq!(c.zones[1], "#00ff00");
        assert_eq!((c.temp_min, c.temp_max), (40.0, 80.0));
        let back = RgbConfig::from_json(&c.to_json());
        assert_eq!(back, c);
    }

    #[test]
    fn soner_og_u_som_demoen() {
        assert_eq!(led_u(0), 0.0);
        assert_eq!(led_u(19), 1.0);
        assert_eq!(led_u(20), 0.0);
        assert_eq!(led_u(38), 0.5);
        assert_eq!(led_zone(37), 1);
        assert_eq!(rgb_to_hex(hex_to_rgb("#3a7bf0")), "#3a7bf0");
    }
}
