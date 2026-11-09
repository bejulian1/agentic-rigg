//! Oppsettet av kortet i millimeter (port av `web/js/lys3d/layout.js`): ett
//! sted for plasseringene, så teksturene og geometrien alltid stemmer.
//!
//! Koordinater: x langs kortet fra braketten (0) mot enden, y opp fra
//! PCIe-kanten (0) mot toppkanten, z ut av kretskortets komponentside (mot
//! viftene). Kretskortet ligger fra z = −1,6 til 0. Generisk kort i RTX
//! 2080-størrelse, uten produsentlogoer eller varemerker.

pub struct Rect4 {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

pub const PCB: (f64, f64, f64, f64, f64) = (0.0, 267.0, 0.0, 111.0, 1.6); // x0, x1, y0, y1, t
pub const PCB_T: f64 = 1.6;
/// Teksturene for kretskortet dekker y fra −9 (under kontaktfingrene) til 111.
pub const PCB_TEX: Rect4 = Rect4 { x0: 0.0, x1: 267.0, y0: -9.0, y1: 111.0 };

pub struct Shroud {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z_front: f64,
    pub z_chamfer: f64,
    pub z_back: f64,
}
pub const SHROUD: Shroud = Shroud { x0: 2.0, x1: 292.0, y0: 2.0, y1: 113.0, z_front: 46.0, z_chamfer: 41.0, z_back: 33.5 };

pub struct FanPos {
    pub cx: f64,
    pub cy: f64,
}
pub const FANS: [FanPos; 2] = [FanPos { cx: 77.0, cy: 57.5 }, FanPos { cx: 217.0, cy: 57.5 }];

pub struct Fan {
    pub r_hole: f64,
    pub r_rotor: f64,
    pub r_hub: f64,
    pub z_mid: f64,
    pub blades: usize,
}
pub const FAN: Fan = Fan { r_hole: 46.5, r_rotor: 44.6, r_hub: 15.5, z_mid: 40.5, blades: 11 };

pub const RING_R0: f64 = 48.4;
pub const RING_R1: f64 = 50.8;
pub const RING_LEDS: usize = 18;
pub const BAR_LEDS: usize = 20;
pub const BAR_X0: f64 = 22.0;
pub const BAR_X1: f64 = 272.0;

pub struct Gpu {
    pub cx: f64,
    pub cy: f64,
    pub sub: f64,
    pub die_w: f64,
    pub die_h: f64,
}
pub const GPU: Gpu = Gpu { cx: 112.0, cy: 60.0, sub: 40.0, die_w: 25.4, die_h: 21.5 };

pub struct Fingers {
    pub tab_a: (f64, f64),
    pub tab_b: (f64, f64),
    pub y0: f64,
    pub pitch: f64,
    pub w: f64,
}
pub const FINGERS: Fingers = Fingers { tab_a: (48.0, 61.6), tab_b: (63.4, 138.0), y0: -8.2, pitch: 1.0, w: 0.68 };

/// Fingrene langs PCIe-kanten (x for hver), med åpningen mellom de to tungene.
pub fn finger_xs() -> Vec<f64> {
    let mut out = Vec::new();
    let mut x = FINGERS.tab_a.0 + 1.4;
    while x < FINGERS.tab_b.1 - 1.3 {
        if !(x > FINGERS.tab_a.1 - 0.6 && x < FINGERS.tab_b.0 + 0.7) {
            out.push(x);
        }
        x += FINGERS.pitch;
    }
    out
}

#[derive(Clone, Copy)]
pub struct Mem {
    pub x: f64,
    pub y: f64,
    pub r: bool,
    pub w: f64,
    pub h: f64,
    pub n: usize,
}

/// GDDR6, 14 × 12 mm. `r`: rotert (14 mm langs y).
pub fn memory() -> Vec<Mem> {
    [(72.0, 34.0, true), (72.0, 60.0, true), (72.0, 86.0, true), (101.0, 95.5, false), (123.0, 95.5, false), (152.0, 34.0, true), (152.0, 60.0, true), (152.0, 86.0, true)]
        .iter()
        .enumerate()
        .map(|(i, (x, y, r))| Mem { x: *x, y: *y, r: *r, w: if *r { 12.0 } else { 14.0 }, h: if *r { 14.0 } else { 12.0 }, n: i + 1 })
        .collect()
}

/// Strømfaser: 8 for kjernen i en kolonne til høyre, 2 for minnet oppe til venstre.
pub fn chokes() -> Vec<(f64, f64, String)> {
    let mut v: Vec<(f64, f64, String)> = (0..8).map(|i| (181.0, 14.0 + i as f64 * 11.4, format!("L{}", i + 1))).collect();
    v.push((40.0, 92.0, "L9".into()));
    v.push((40.0, 80.5, "L10".into()));
    v
}

pub fn powerstages() -> Vec<(f64, f64)> {
    chokes().iter().enumerate().map(|(i, c)| (if i < 8 { 168.5 } else { 28.5 }, c.1)).collect()
}

/// Polymerkondensatorer (Ø 6,3 mm): (x, y, r, h).
pub fn caps() -> Vec<(f64, f64, f64, f64)> {
    let mut v: Vec<(f64, f64, f64, f64)> = (0..8).map(|i| (195.0, 14.0 + i as f64 * 11.4, 3.15, 5.8)).collect();
    for (x, y) in [(212.0, 80.0), (212.0, 92.0), (222.0, 86.0), (52.0, 92.0), (52.0, 80.5)] {
        v.push((x, y, 3.15, 5.8));
    }
    v
}

pub struct Power {
    pub x0: f64,
    pub x1: f64,
    pub pins: usize,
    pub label: &'static str,
    pub reference: &'static str,
    pub y0: f64,
    pub y1: f64,
    pub z1: f64,
}
pub const POWER: [Power; 2] = [
    Power { x0: 224.0, x1: 245.5, pins: 8, label: "8-PIN", reference: "J2", y0: 98.5, y1: 112.4, z1: 9.8 },
    Power { x0: 249.5, x1: 264.1, pins: 6, label: "6-PIN", reference: "J3", y0: 98.5, y1: 112.4, z1: 9.8 },
];

pub struct Port {
    pub label: &'static str,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
    pub r: f64,
}
/// Utganger på braketten (y og z for åpningen; x går fra −3 til 14).
pub const PORTS: [Port; 5] = [
    Port { label: "USB-C", y0: 8.0, y1: 16.6, z0: 0.6, z1: 3.4, r: 1.3 },
    Port { label: "HDMI", y0: 22.0, y1: 37.0, z0: 0.5, z1: 6.1, r: 0.6 },
    Port { label: "DP", y0: 43.0, y1: 59.0, z0: 0.5, z1: 6.6, r: 0.6 },
    Port { label: "DP", y0: 64.0, y1: 80.0, z0: 0.5, z1: 6.6, r: 0.6 },
    Port { label: "DP", y0: 85.0, y1: 101.0, z0: 0.5, z1: 6.6, r: 0.6 },
];

pub struct Bracket {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
}
pub const BRACKET: Bracket = Bracket { x0: -2.2, x1: -1.4, y0: -12.0, y1: 118.0, z0: -6.5, z1: 34.3 };

pub const HOLES: [(f64, f64); 9] = [
    (112.0 - 26.5, 60.0 - 26.5),
    (112.0 + 26.5, 60.0 - 26.5),
    (112.0 - 26.5, 60.0 + 26.5),
    (112.0 + 26.5, 60.0 + 26.5),
    (8.0, 104.0),
    (8.0, 7.0),
    (205.0, 104.0),
    (205.0, 6.0),
    (261.0, 7.0),
];

pub struct Ic {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub reference: &'static str,
    pub label: &'static str,
}
pub const ICS: [Ic; 4] = [
    Ic { x: 214.0, y: 34.0, w: 6.0, h: 6.0, reference: "U3", label: "PWM" },
    Ic { x: 214.0, y: 14.0, w: 5.0, h: 4.0, reference: "U12", label: "BIOS" },
    Ic { x: 36.0, y: 64.0, w: 4.0, h: 4.0, reference: "U5", label: "PWM" },
    Ic { x: 240.0, y: 30.0, w: 3.5, h: 3.5, reference: "U7", label: "LED" },
];
pub const HEADERS: [Ic; 2] = [Ic { x: 254.0, y: 12.0, w: 8.4, h: 3.2, reference: "J5", label: "FAN" }, Ic { x: 254.0, y: 21.0, w: 8.4, h: 3.2, reference: "J6", label: "LED" }];

pub struct Box3 {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
}
pub const PLATE: Box3 = Box3 { x0: 112.0 - 18.0, x1: 112.0 + 18.0, y0: 60.0 - 18.0, y1: 60.0 + 18.0, z0: 2.45, z1: 8.0 };
pub struct Fins {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
    pub pitch: f64,
    pub t: f64,
}
pub const FINS: Fins = Fins { x0: 8.0, x1: 288.0, y0: 6.0, y1: 106.0, z0: 10.0, z1: 33.0, pitch: 2.5, t: 0.42 };
/// Varmerørene: y og om bøyen er til venstre.
pub const PIPES: [(f64, bool); 5] = [(30.0, true), (45.0, false), (60.0, true), (75.0, false), (90.0, true)];
pub const VRM: Box3 = Box3 { x0: 174.5, x1: 201.5, y0: 8.0, y1: 104.0, z0: 6.9, z1: 9.6 };

pub struct Backplate {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
    pub win: Rect4,
}
pub const BACKPLATE: Backplate = Backplate { x0: 0.5, x1: 270.0, y0: 1.0, y1: 110.0, z0: -4.2, z1: -2.6, win: Rect4 { x0: 112.0 - 31.0, x1: 112.0 + 31.0, y0: 60.0 - 31.0, y1: 60.0 + 31.0 } };

/// Fast pseudotilfeldighet (samme tekstur hver gang), som demoens `rng`.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    pub fn next(&mut self) -> f64 {
        self.0 = (self.0 * 16807) % 2_147_483_647;
        (self.0 as f64 - 1.0) / 2_147_483_646.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Smd {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub t: f64,
    pub res: bool,
}

/// Små komponenter (0402/0603) med en fast tilfeldighet.
pub fn small_parts() -> Vec<Smd> {
    let mut rnd = Rng::new(20801);
    let mut out: Vec<Smd> = Vec::new();
    let mem = memory();
    let ch = chokes();
    let ps = powerstages();
    let cp = caps();
    let blocked = |out: &[Smd], x: f64, y: f64, w: f64, h: f64| -> bool {
        let r = [x - w / 2.0 - 0.6, y - h / 2.0 - 0.6, x + w / 2.0 + 0.6, y + h / 2.0 + 0.6];
        let hit = |a: [f64; 4]| r[0] < a[2] && r[2] > a[0] && r[1] < a[3] && r[3] > a[1];
        if hit([GPU.cx - GPU.sub / 2.0 - 0.5, GPU.cy - GPU.sub / 2.0 - 0.5, GPU.cx + GPU.sub / 2.0 + 0.5, GPU.cy + GPU.sub / 2.0 + 0.5]) {
            return true;
        }
        for m in &mem {
            if hit([m.x - m.w / 2.0 - 1.0, m.y - m.h / 2.0 - 1.0, m.x + m.w / 2.0 + 1.0, m.y + m.h / 2.0 + 1.0]) {
                return true;
            }
        }
        for c in &ch {
            if hit([c.0 - 6.0, c.1 - 6.0, c.0 + 6.0, c.1 + 6.0]) {
                return true;
            }
        }
        for c in &ps {
            if hit([c.0 - 3.5, c.1 - 3.5, c.0 + 3.5, c.1 + 3.5]) {
                return true;
            }
        }
        for c in &cp {
            if hit([c.0 - 4.0, c.1 - 4.0, c.0 + 4.0, c.1 + 4.0]) {
                return true;
            }
        }
        for p in &POWER {
            if hit([p.x0 - 1.0, p.y0 - 1.0, p.x1 + 1.0, p.y1]) {
                return true;
            }
        }
        for h in &HOLES {
            if hit([h.0 - 4.0, h.1 - 4.0, h.0 + 4.0, h.1 + 4.0]) {
                return true;
            }
        }
        for i in ICS.iter().chain(HEADERS.iter()) {
            if hit([i.x - i.w / 2.0 - 1.2, i.y - i.h / 2.0 - 1.2, i.x + i.w / 2.0 + 1.2, i.y + i.h / 2.0 + 1.2]) {
                return true;
            }
        }
        if hit([-3.0, 3.0, 16.0, 104.0]) || hit([0.0, 0.0, 267.0, 1.8]) || hit([0.0, 109.2, 267.0, 111.0]) || hit([265.0, 0.0, 267.0, 111.0]) {
            return true;
        }
        for o in out {
            if hit([o.x - o.w / 2.0, o.y - o.h / 2.0, o.x + o.w / 2.0, o.y + o.h / 2.0]) {
                return true;
            }
        }
        false
    };
    let place = |out: &mut Vec<Smd>, x: f64, y: f64, big: bool, rot: bool, res: bool| {
        let (w, h) = if big { (1.6, 0.8) } else { (1.0, 0.5) };
        let (w, h) = if rot { (h, w) } else { (w, h) };
        if blocked(out, x, y, w, h) {
            return;
        }
        out.push(Smd { x, y, w, h, t: if big { 0.5 } else { 0.35 }, res });
    };
    for i in 0..64 {
        let a = i as f64 / 64.0 * std::f64::consts::TAU;
        let r = 24.0 + (i % 2) as f64 * 2.2;
        place(&mut out, GPU.cx + a.cos() * r * 1.02, GPU.cy + a.sin() * r, i % 3 == 0, a.cos().abs() > 0.7, i % 5 == 0);
    }
    for m in &mem {
        for k in 0..6 {
            let side = if k % 2 == 1 { 1.0 } else { -1.0 };
            if m.r {
                place(&mut out, m.x + side * (m.w / 2.0 + 2.2), m.y - 4.5 + (k >> 1) as f64 * 4.5, false, true, false);
            } else {
                place(&mut out, m.x - 4.5 + (k >> 1) as f64 * 4.5, m.y + side * (m.h / 2.0 + 2.2), false, false, false);
            }
        }
    }
    for i in 0..16 {
        place(&mut out, 66.0 + i as f64 * 4.2, 13.0, false, true, false);
        place(&mut out, 67.6 + i as f64 * 4.2, 13.0, false, true, false);
    }
    let mut i = 0;
    while i < 1400 && out.len() < 430 {
        i += 1;
        let x = 4.0 + rnd.next() * 259.0;
        let y = 3.0 + rnd.next() * 104.0;
        let near_vrm = x > 160.0 && x < 236.0;
        if !near_vrm && rnd.next() < 0.45 {
            continue;
        }
        let big = rnd.next() < 0.35;
        let rot = rnd.next() < 0.5;
        let res = rnd.next() < 0.28;
        place(&mut out, x, y, big, rot, res);
    }
    out
}

/// MLCC-rutenettet bak GPU-en (baksiden).
pub fn back_caps() -> Vec<Smd> {
    let mut out = Vec::new();
    for j in 0..7 {
        for i in 0..7 {
            if (i == 3 && j == 3) || ((i == 0 || i == 6) && (j == 0 || j == 6)) {
                continue;
            }
            out.push(Smd { x: GPU.cx - 10.5 + i as f64 * 3.5, y: GPU.cy - 10.5 + j as f64 * 3.5, w: 1.6, h: 0.8, t: 0.5, res: false });
        }
    }
    for i in 0..8 {
        out.push(Smd { x: GPU.cx - 14.0 + i as f64 * 4.0, y: GPU.cy + 15.5, w: 1.0, h: 0.5, t: 0.35, res: false });
    }
    for i in 0..8 {
        out.push(Smd { x: GPU.cx - 14.0 + i as f64 * 4.0, y: GPU.cy - 15.5, w: 1.0, h: 0.5, t: 0.35, res: false });
    }
    out
}

/// Én lysdiode i ringene: sone (0 eller 1), indeks og vinkler (y opp).
pub struct RingLed {
    pub fan: usize,
    pub a0: f64,
    pub a1: f64,
}

/// Ringene i samme rekkefølge og med samme u som rgb.rs. I SVG er y ned og
/// vinkelen går med klokka; her er y opp, så vinkelen speiles.
pub fn ring_leds() -> Vec<RingLed> {
    let mut v = Vec::new();
    for fan in 0..2 {
        for i in 0..RING_LEDS {
            let a0 = i as f64 / RING_LEDS as f64 * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2 + 0.03;
            let a1 = (i + 1) as f64 / RING_LEDS as f64 * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2 - 0.03;
            v.push(RingLed { fan, a0: -a1, a1: -a0 });
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faste_deler_er_like_hver_gang() {
        let a = small_parts();
        let b = small_parts();
        assert_eq!(a.len(), b.len());
        assert!(a.len() > 200, "{}", a.len());
        assert_eq!(back_caps().len(), 44 + 16);
        assert_eq!(ring_leds().len(), 36);
        assert!(finger_xs().len() > 70);
    }
}
