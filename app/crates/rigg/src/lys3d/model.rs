//! Skjermkortet (port av `web/js/lys3d/card-model.js`): geometri,
//! materialer, lag og plasseringen av lysdiodene. Alt i kortets
//! koordinater (mm, se `layout.rs`). Visningen flytter lagene og sentrerer.

use super::layout::*;
use super::mesh::{BoxUv, MeshBuilder, MeshData};
use super::textures::{Tex, atlas_point, atlas_uv, backplate_uv, bracket_uv};
use glam::{Mat4, Vec3};
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};

const DEG: f64 = PI / 180.0;

/// Et lag: id, retningen det løftes av i, hvor langt det flyttes ved
/// full eksplodert visning, og fra hvilket trinn det er tatt av
/// (0 Hel, 1 Uten deksel, 2 Uten kjøler, 3 Bare kretskort).
pub struct Layer {
    pub id: &'static str,
    pub dir: [f32; 3],
    pub spread: f32,
    pub step: usize,
}

pub const LAYERS: [Layer; 7] = [
    Layer { id: "deksel", dir: [0.0, 0.0, 1.0], spread: 124.0, step: 1 },
    Layer { id: "vifter", dir: [0.0, 0.0, 1.0], spread: 84.0, step: 1 },
    Layer { id: "kjoler", dir: [0.0, 0.0, 1.0], spread: 44.0, step: 2 },
    Layer { id: "kretskort", dir: [0.0, 0.0, 0.0], spread: 0.0, step: 99 },
    Layer { id: "kryss", dir: [0.0, 0.0, -1.0], spread: 16.0, step: 2 },
    Layer { id: "brakett", dir: [-1.0, 0.0, 0.0], spread: 34.0, step: 3 },
    Layer { id: "bakplate", dir: [0.0, 0.0, -1.0], spread: 36.0, step: 3 },
];

pub fn layer_index(id: &str) -> usize {
    LAYERS.iter().position(|l| l.id == id).unwrap_or(3)
}

/// Delene som kan pekes på: id, navn, kort beskrivelse og sone (lyssonene).
pub const GROUPS: [(&str, &str, &str, Option<&str>); 28] = [
    ("shroud", "Deksel", "Plast, med lyslist og to lysringer", None),
    ("bar", "Lyslist", "Sone «Lyslist», 20 lysdioder", Some("bar")),
    ("ring1", "Lysring, vifte 1", "Sone «Vifte 1», 18 lysdioder", Some("fan1")),
    ("ring2", "Lysring, vifte 2", "Sone «Vifte 2», 18 lysdioder", Some("fan2")),
    ("fan1", "Vifte 1", "90 mm, 11 blader", None),
    ("fan2", "Vifte 2", "90 mm, 11 blader", None),
    ("fins", "Kjøleribbe", "Lameller i aluminium", None),
    ("pipes", "Varmerør", "5 × Ø 6 mm, kobber", None),
    ("plate", "Kontaktplate", "Forniklet kobber over GPU-en", None),
    ("midplate", "Kjøleplate", "Over minnet", None),
    ("vrmsink", "VRM-kjøler", "Over spolene og effekttrinnene", None),
    ("pads", "Termiske puter", "Leder varme fra minnet og effekttrinnene", None),
    ("xbracket", "Festekryss", "Fjærstål, holder kjøleren mot GPU-en", None),
    ("gpu", "GPU-brikke", "TU104-klasse, 545 mm², 12 nm", None),
    ("memory", "GDDR6-minne", "8 × 1 GB, 256 bit, 14 Gbps", None),
    ("chokes", "Spoler", "Strømfaser: 8 for kjernen, 2 for minnet", None),
    ("stages", "Effekttrinn", "Én MOSFET-pakke per fase", None),
    ("caps", "Polymerkondensatorer", "Glatter spenningen fra fasene", None),
    ("smd", "Små komponenter", "Kondensatorer og motstander (0402 og 0603)", None),
    ("ics", "Kontrollere", "Spenningsregulering, BIOS og lys", None),
    ("pcie", "PCIe x16-kontakt", "Gullbelagte kontakter, 82 per side", None),
    ("power", "Strømkontakter", "8-pin og 6-pin, 12 V", None),
    ("ports", "Utganger", "3 × DisplayPort, HDMI og USB-C", None),
    ("headers", "Pinnehoder", "Vifter og lys", None),
    ("pcb", "Kretskort", "267 × 111 mm", None),
    ("backcaps", "Kondensatorer bak GPU-en", "Keramiske (MLCC)", None),
    ("bracket", "Brakett", "Stål, to spor", None),
    ("backplate", "Bakplate", "Aluminium, med vindu bak GPU-en", None),
];

pub fn group_index(id: &str) -> usize {
    GROUPS.iter().position(|g| g.0 == id).unwrap_or(0)
}

/// Materialet til en del (lineære farger, som skyggeleggeren venter).
#[derive(Clone, Copy)]
pub struct Mat {
    pub color: [f32; 3],
    pub rough: f32,
    pub metal: f32,
    pub map: Option<Tex>,
    pub alpha_test: f32,
    pub double: bool,
    pub holes: Option<[f32; 8]>,
    pub led: bool,
}

fn srgb(c: f64) -> f32 {
    (if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }) as f32
}

pub fn hex_lin(hex: &str) -> [f32; 3] {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    [srgb(((n >> 16) & 255) as f64 / 255.0), srgb(((n >> 8) & 255) as f64 / 255.0), srgb((n & 255) as f64 / 255.0)]
}

fn m(hex: &str, rough: f32, metal: f32) -> Mat {
    Mat { color: hex_lin(hex), rough, metal, map: None, alpha_test: 0.0, double: false, holes: None, led: false }
}
fn mt(tex: Tex, rough: f32, metal: f32) -> Mat {
    Mat { color: [1.0; 3], rough, metal, map: Some(tex), alpha_test: 0.0, double: false, holes: None, led: false }
}

pub struct Part {
    pub group: &'static str,
    pub layer: &'static str,
    pub mat: Mat,
    pub data: MeshData,
    pub cast: bool,
    /// Viftehjul 0 eller 1 (bygget rundt (0, 0), plasseres av visningen), ellers −1.
    pub rotor: i32,
    /// «near» eller «far» for lamellene (detaljert nær, stripet blokk på avstand).
    pub lod: Option<&'static str>,
}

pub struct Card {
    pub parts: Vec<Part>,
    pub led_pos: Vec<[f32; 3]>,
    pub bar_a: [f32; 3],
    pub bar_b: [f32; 3],
    pub fan_light: [[f32; 3]; 2],
    pub fan_centers: [[f32; 2]; 2],
    pub center: [f32; 3],
}

struct Opts {
    uv: Option<[f32; 2]>,
    cast: bool,
    rotor: i32,
    lod: Option<&'static str>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { uv: None, cast: true, rotor: -1, lod: None }
    }
}

fn t(x: f64, y: f64, z: f64) -> Mat4 {
    Mat4::from_translation(Vec3::new(x as f32, y as f32, z as f32))
}
fn rz(a: f64) -> Mat4 {
    Mat4::from_rotation_z(a as f32)
}

pub fn build() -> Card {
    let mut parts: Vec<Part> = Vec::new();
    let mut part = |group: &'static str, layer: &'static str, mat: Mat, opts: Opts, f: &mut dyn FnMut(&mut MeshBuilder)| {
        let mut b = MeshBuilder::new();
        if let Some(uv) = opts.uv {
            b.default_uv = uv;
        }
        f(&mut b);
        if b.count() == 0 {
            return;
        }
        parts.push(Part { group, layer, mat, data: b.build(), cast: opts.cast, rotor: opts.rotor, lod: opts.lod });
    };
    let o = Opts::default;
    let ou = |uv: [f32; 2]| Opts { uv: Some(uv), ..Opts::default() };
    let nocast = || Opts { cast: false, ..Opts::default() };

    // ---- Materialer ----
    let pcb_front = mt(Tex::PcbFront, 0.4, 0.0);
    let pcb_back = mt(Tex::PcbBack, 0.4, 0.0);
    let pcb_edge = m("#3d4934", 0.8, 0.0);
    let gold = m("#e0b25c", 0.24, 1.0);
    let tin = m("#c9cbcd", 0.35, 1.0);
    let mem_m = mt(Tex::Atlas, 0.55, 0.0);
    let die = mt(Tex::Atlas, 0.1, 0.5);
    let sub = mt(Tex::Atlas, 0.45, 0.0);
    let choke = mt(Tex::Atlas, 0.5, 0.3);
    let ps_m = mt(Tex::Atlas, 0.5, 0.0);
    let cap_m = mt(Tex::Atlas, 0.3, 1.0);
    let ic_m = mt(Tex::Atlas, 0.5, 0.0);
    let hdr_m = mt(Tex::Atlas, 0.6, 0.0);
    let hub_m = mt(Tex::Atlas, 0.5, 0.0);
    let bga = m("#08090a", 0.9, 0.0);
    let smd_cap = m("#9d8460", 0.6, 0.0);
    let smd_res = m("#141415", 0.5, 0.0);
    let plastic = m("#141518", 0.55, 0.0);
    let hole = m("#040404", 1.0, 0.0);
    let port_metal = m("#c6cad0", 0.3, 1.0);
    let bracket_m = Mat { alpha_test: 0.5, double: true, ..Mat { color: hex_lin("#b3b8be"), ..mt(Tex::Bracket, 0.36, 1.0) } };
    let bracket_edge = m("#9ea3a9", 0.4, 1.0);
    let backplate_m = Mat { alpha_test: 0.5, double: true, ..Mat { color: hex_lin("#2b2e33"), ..mt(Tex::Backplate, 0.42, 0.6) } };
    let backplate_edge = m("#3a3e44", 0.35, 0.7);
    let screw = m("#8e949b", 0.32, 1.0);
    let pad = m("#7d8996", 0.9, 0.0);
    let nickel = m("#d0cdc7", 0.16, 1.0);
    let midplate = m("#1d1f23", 0.5, 0.4);
    let alu = m("#c6cbd1", 0.34, 1.0);
    let fin_alu = m("#c9ced4", 0.4, 0.72);
    let fin_block = mt(Tex::FinStripe, 0.5, 0.65);
    let copper = m("#c47e52", 0.26, 1.0);
    let xbr = m("#17181b", 0.42, 0.5);
    let holes = [FANS[0].cx as f32, FANS[0].cy as f32, FAN.r_hole as f32, 1.0, FANS[1].cx as f32, FANS[1].cy as f32, FAN.r_hole as f32, 1.0];
    let shroud_m = Mat { double: true, holes: Some(holes), ..m("#272a30", 0.6, 0.0) };
    let skirt = Mat { double: true, ..m("#272a30", 0.6, 0.0) };
    let trim = m("#101114", 0.2, 0.0);
    let duct = Mat { double: true, ..m("#16171a", 0.7, 0.0) };
    let blade = Mat { double: true, ..m("#1d1f24", 0.42, 0.0) };
    let hub_side = m("#202328", 0.45, 0.0);
    let motor = m("#141518", 0.6, 0.0);
    let led_m = Mat { led: true, ..m("#30353c", 0.32, 0.0) };

    // ---- Kretskort ----
    let tex_uv = |x: f64, y: f64, back: bool| -> [f32; 2] {
        let u = ((x - PCB_TEX.x0) / (PCB_TEX.x1 - PCB_TEX.x0)) as f32;
        [if back { 1.0 - u } else { u }, ((y - PCB_TEX.y0) / (PCB_TEX.y1 - PCB_TEX.y0)) as f32]
    };
    let board = [[PCB.0, PCB.2, PCB.1, PCB.3], [FINGERS.tab_a.0, FINGERS.y0, FINGERS.tab_a.1, 0.0], [FINGERS.tab_b.0, FINGERS.y0, FINGERS.tab_b.1, 0.0]];
    part("pcb", "kretskort", pcb_front, o(), &mut |b| {
        for [x0, y0, x1, y1] in board {
            b.quad([x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0], [0.0, 0.0, 1.0], Some([tex_uv(x0, y0, false), tex_uv(x1, y0, false), tex_uv(x1, y1, false), tex_uv(x0, y1, false)]));
        }
    });
    part("pcb", "kretskort", pcb_back, o(), &mut |b| {
        let z = -PCB_T;
        for [x0, y0, x1, y1] in board {
            b.quad([x1, y0, z], [x0, y0, z], [x0, y1, z], [x1, y1, z], [0.0, 0.0, -1.0], Some([tex_uv(x1, y0, true), tex_uv(x0, y0, true), tex_uv(x0, y1, true), tex_uv(x1, y1, true)]));
        }
    });
    part("pcb", "kretskort", pcb_edge, o(), &mut |b| {
        for [x0, y0, x1, y1] in board {
            b.bx([(x0 + x1) / 2.0, (y0 + y1) / 2.0, -PCB_T / 2.0], [x1 - x0, y1 - y0, PCB_T], BoxUv { skip: &["pz", "nz"], ..Default::default() });
        }
    });
    part("pcie", "kretskort", gold, o(), &mut |b| {
        for x in finger_xs() {
            let long = if (x * 7.0).round() as i64 % 9 == 0 { 0.0 } else { 0.6 };
            let cy = FINGERS.y0 + 0.5 + (7.1 - long) / 2.0 + long;
            b.boxs([x, cy, 0.02], [FINGERS.w, 7.1 - long, 0.04]);
            b.boxs([x, cy, -PCB_T - 0.02], [FINGERS.w, 7.1 - long, 0.04]);
        }
    });
    part("gpu", "kretskort", bga, o(), &mut |b| {
        b.boxs([GPU.cx, GPU.cy, 0.22], [GPU.sub - 2.0, GPU.sub - 2.0, 0.44]);
    });
    part("gpu", "kretskort", sub, ou(atlas_point("sub")), &mut |b| {
        b.rounded_box([GPU.cx, GPU.cy, 1.02], [GPU.sub, GPU.sub, 1.15], 0.3, 1, Some(atlas_uv("sub", 0.0)));
    });
    part("gpu", "kretskort", die, ou(atlas_point("die")), &mut |b| {
        b.rounded_box([GPU.cx, GPU.cy, 1.97], [GPU.die_w, GPU.die_h, 0.75], 0.12, 1, Some(atlas_uv("die", 2.0)));
    });
    part("memory", "kretskort", mem_m, ou(atlas_point("mem")), &mut |b| {
        for mm in memory() {
            let mx = t(mm.x, mm.y, 0.0) * if mm.r { rz(90.0 * DEG) } else { Mat4::IDENTITY };
            b.with(mx, |b| {
                b.rounded_box([0.0, 0.0, 0.85], [14.0, 12.0, 1.1], 0.25, 1, Some(atlas_uv("mem", 4.0)));
            });
        }
    });
    part("chokes", "kretskort", choke, ou(atlas_point("choke")), &mut |b| {
        for c in chokes() {
            b.rounded_box([c.0, c.1, 3.0], [10.6, 10.0, 6.0], 0.7, 2, Some(atlas_uv("choke", 6.0)));
        }
    });
    part("chokes", "kretskort", tin, o(), &mut |b| {
        for c in chokes() {
            b.boxs([c.0 - 5.55, c.1, 0.9], [0.5, 6.4, 1.8]);
            b.boxs([c.0 + 5.55, c.1, 0.9], [0.5, 6.4, 1.8]);
        }
    });
    part("stages", "kretskort", ps_m, ou(atlas_point("ps")), &mut |b| {
        for p in powerstages() {
            b.bx([p.0, p.1, 0.5], [5.0, 6.0, 0.95], BoxUv { pz: Some(atlas_uv("ps", 4.0)), skip: &[] });
        }
    });
    let cu = atlas_uv("cap", 0.0);
    part("caps", "kretskort", cap_m, ou([cu[0] + 0.004, cu[1] + 0.06]), &mut |b| {
        for c in caps() {
            b.cylinder(c.0, c.1, c.2, 0.45, c.3, 28, (false, true), Some(atlas_uv("cap", 2.0)), None);
        }
    });
    part("caps", "kretskort", plastic, o(), &mut |b| {
        for c in caps() {
            b.boxs([c.0, c.1, 0.22], [6.6, 6.6, 0.44]);
        }
    });
    part("ics", "kretskort", ic_m, ou(atlas_point("ic")), &mut |b| {
        for i in &ICS {
            b.bx([i.x, i.y, 0.45], [i.w, i.h, 0.9], BoxUv { pz: Some(atlas_uv("ic", 6.0)), skip: &[] });
        }
    });
    part("headers", "kretskort", hdr_m, ou(atlas_point("hdr")), &mut |b| {
        for h in &HEADERS {
            b.bx([h.x, h.y, 1.6], [h.w, h.h, 3.2], BoxUv { pz: Some(atlas_uv("hdr", 2.0)), skip: &[] });
        }
    });
    // Små komponenter: kropp og tinnender.
    let smd = small_parts();
    for (res, mat) in [(false, smd_cap), (true, smd_res)] {
        part("smd", "kretskort", mat, nocast(), &mut |b| {
            for p in smd.iter().filter(|p| p.res == res) {
                let along = p.w > p.h;
                b.boxs([p.x, p.y, p.t / 2.0], if along { [p.w * 0.62, p.h, p.t] } else { [p.w, p.h * 0.62, p.t] });
            }
        });
    }
    part("smd", "kretskort", tin, nocast(), &mut |b| {
        for p in &smd {
            let along = p.w > p.h;
            let e = (if along { p.w } else { p.h }) * 0.2;
            for s in [-1.0, 1.0] {
                if along {
                    b.bx([p.x + s * (p.w / 2.0 - e / 2.0), p.y, p.t / 2.0], [e, p.h + 0.02, p.t + 0.02], BoxUv { skip: &["ny"], ..Default::default() });
                } else {
                    b.bx([p.x, p.y + s * (p.h / 2.0 - e / 2.0), p.t / 2.0], [p.w + 0.02, e, p.t + 0.02], BoxUv { skip: &["nz"], ..Default::default() });
                }
            }
        }
    });
    let back = back_caps();
    part("backcaps", "kretskort", smd_cap, nocast(), &mut |b| {
        for p in &back {
            b.boxs([p.x, p.y, -PCB_T - p.t / 2.0], [p.w * 0.62, p.h, p.t]);
        }
    });
    part("backcaps", "kretskort", tin, nocast(), &mut |b| {
        for p in &back {
            for s in [-1.0, 1.0] {
                b.boxs([p.x + s * (p.w / 2.0 - p.w * 0.1), p.y, -PCB_T - p.t / 2.0], [p.w * 0.2, p.h + 0.02, p.t + 0.02]);
            }
        }
    });
    // Strømkontakter.
    part("power", "kretskort", plastic, o(), &mut |b| {
        for p in &POWER {
            b.rounded_box([(p.x0 + p.x1) / 2.0, (p.y0 + p.y1) / 2.0, p.z1 / 2.0], [p.x1 - p.x0, p.y1 - p.y0, p.z1], 0.6, 1, None);
            b.boxs([(p.x0 + p.x1) / 2.0, p.y1 - 4.2, p.z1 + 0.7], [4.2, 6.4, 1.4]);
            b.boxs([(p.x0 + p.x1) / 2.0, p.y1 - 1.0, p.z1 + 1.3], [4.2, 1.6, 1.0]);
        }
    });
    part("power", "kretskort", hole, nocast(), &mut |b| {
        for p in &POWER {
            let cols = p.pins / 2;
            let pitch = (p.x1 - p.x0 - 1.8) / cols as f64;
            for i in 0..cols {
                for j in 0..2 {
                    b.boxs([p.x0 + 0.9 + pitch * (i as f64 + 0.5), p.y1 + 0.01, 2.45 + j as f64 * 4.9], [pitch - 1.0, 0.04, 3.6]);
                }
            }
        }
    });
    // Utganger.
    part("ports", "kretskort", port_metal, o(), &mut |b| {
        for p in &PORTS {
            b.rounded_box([5.5, (p.y0 + p.y1) / 2.0, (p.z0 + p.z1) / 2.0], [17.0, p.y1 - p.y0, p.z1 - p.z0], p.r.min(1.1), 1, None);
        }
    });
    part("ports", "kretskort", hole, nocast(), &mut |b| {
        for p in &PORTS {
            let ins = 0.55;
            let x = -3.03;
            b.quad([x, p.y0 + ins, p.z1 - ins], [x, p.y1 - ins, p.z1 - ins], [x, p.y1 - ins, p.z0 + ins], [x, p.y0 + ins, p.z0 + ins], [-1.0, 0.0, 0.0], None);
        }
    });

    // ---- Brakett ----
    let br = &BRACKET;
    part("bracket", "brakett", bracket_m, o(), &mut |b| {
        let uv = |y: f64, z: f64| bracket_uv(y, z);
        b.quad([br.x1, br.y0, br.z0], [br.x1, br.y0, br.z1], [br.x1, br.y1, br.z1], [br.x1, br.y1, br.z0], [1.0, 0.0, 0.0], Some([uv(br.y0, br.z0), uv(br.y0, br.z1), uv(br.y1, br.z1), uv(br.y1, br.z0)]));
        b.quad([br.x0, br.y0, br.z1], [br.x0, br.y0, br.z0], [br.x0, br.y1, br.z0], [br.x0, br.y1, br.z1], [-1.0, 0.0, 0.0], Some([uv(br.y0, br.z1), uv(br.y0, br.z0), uv(br.y1, br.z0), uv(br.y1, br.z1)]));
    });
    part("bracket", "brakett", bracket_edge, o(), &mut |b| {
        b.bx([(br.x0 + br.x1) / 2.0, (br.y0 + br.y1) / 2.0, (br.z0 + br.z1) / 2.0], [br.x1 - br.x0, br.y1 - br.y0, br.z1 - br.z0], BoxUv { skip: &["px", "nx"], ..Default::default() });
        b.boxs([(br.x0 + 10.5) / 2.0, br.y1 - 0.4, (br.z0 + br.z1) / 2.0], [10.5 - br.x0, 0.8, br.z1 - br.z0]);
        b.boxs([(br.x0 + br.x1) / 2.0, br.y0 - 6.0, 2.8], [br.x1 - br.x0, 12.0, 7.5]);
    });

    // ---- Bakplate ----
    let bp = &BACKPLATE;
    part("backplate", "bakplate", backplate_m, o(), &mut |b| {
        let uv = |x: f64, y: f64| backplate_uv(x, y);
        b.quad([bp.x1, bp.y0, bp.z0], [bp.x0, bp.y0, bp.z0], [bp.x0, bp.y1, bp.z0], [bp.x1, bp.y1, bp.z0], [0.0, 0.0, -1.0], Some([uv(bp.x1, bp.y0), uv(bp.x0, bp.y0), uv(bp.x0, bp.y1), uv(bp.x1, bp.y1)]));
        b.quad([bp.x0, bp.y0, bp.z1], [bp.x1, bp.y0, bp.z1], [bp.x1, bp.y1, bp.z1], [bp.x0, bp.y1, bp.z1], [0.0, 0.0, 1.0], Some([uv(bp.x0, bp.y0), uv(bp.x1, bp.y0), uv(bp.x1, bp.y1), uv(bp.x0, bp.y1)]));
    });
    part("backplate", "bakplate", backplate_edge, o(), &mut |b| {
        let zc = (bp.z0 + bp.z1) / 2.0;
        let dz = bp.z1 - bp.z0;
        b.bx([(bp.x0 + bp.x1) / 2.0, (bp.y0 + bp.y1) / 2.0, zc], [bp.x1 - bp.x0, bp.y1 - bp.y0, dz], BoxUv { skip: &["pz", "nz"], ..Default::default() });
        let w = &bp.win;
        b.bx([(w.x0 + w.x1) / 2.0, w.y0 + 0.01, zc], [w.x1 - w.x0 - 6.0, 0.02, dz], BoxUv { skip: &["ny", "pz", "nz", "px", "nx"], ..Default::default() });
        b.bx([(w.x0 + w.x1) / 2.0, w.y1 - 0.01, zc], [w.x1 - w.x0 - 6.0, 0.02, dz], BoxUv { skip: &["py", "pz", "nz", "px", "nx"], ..Default::default() });
        b.bx([w.x0 + 0.01, (w.y0 + w.y1) / 2.0, zc], [0.02, w.y1 - w.y0 - 6.0, dz], BoxUv { skip: &["nx", "pz", "nz", "py", "ny"], ..Default::default() });
        b.bx([w.x1 - 0.01, (w.y0 + w.y1) / 2.0, zc], [0.02, w.y1 - w.y0 - 6.0, dz], BoxUv { skip: &["px", "pz", "nz", "py", "ny"], ..Default::default() });
    });
    part("backplate", "bakplate", screw, o(), &mut |b| {
        for (x, y) in [(6.0, 6.0), (6.0, 105.0), (154.0, 6.0), (154.0, 105.0), (264.0, 6.0), (264.0, 105.0)] {
            b.cylinder(x, y, 2.3, bp.z0 - 0.9, bp.z0, 20, (false, true), None, None);
        }
    });

    // ---- Festekryss ----
    let xz0 = -PCB_T - 0.85;
    part("xbracket", "kryss", xbr, o(), &mut |b| {
        for a in [45.0, 135.0] {
            b.with(t(GPU.cx, GPU.cy, xz0 + 0.4) * rz(a * DEG), |b| {
                b.rounded_box([0.0, 0.0, 0.0], [80.0, 7.0, 0.8], 0.3, 1, None);
            });
        }
        b.cylinder(GPU.cx, GPU.cy, 10.0, xz0, xz0 + 0.8, 36, (true, true), None, None);
    });
    part("xbracket", "kryss", screw, o(), &mut |b| {
        for (x, y) in HOLES.iter().take(4) {
            b.cylinder(*x, *y, 2.7, xz0 - 1.2, xz0, 24, (true, false), None, None);
            b.boxs([*x, *y, xz0 - 1.21], [3.2, 0.7, 0.04]);
            b.boxs([*x, *y, xz0 - 1.21], [0.7, 3.2, 0.04]);
        }
    });

    // ---- Kjøler ----
    part("plate", "kjoler", nickel, o(), &mut |b| {
        let p = &PLATE;
        b.rounded_box([(p.x0 + p.x1) / 2.0, (p.y0 + p.y1) / 2.0, (p.z0 + p.z1) / 2.0], [p.x1 - p.x0, p.y1 - p.y0, p.z1 - p.z0], 0.9, 2, None);
    });
    part("pads", "kjoler", pad, o(), &mut |b| {
        for mm in memory() {
            b.boxs([mm.x, mm.y, 2.0], [mm.w + 0.6, mm.h + 0.6, 1.2]);
        }
        for p in powerstages() {
            b.boxs([p.0, p.1, 1.7], [5.6, 6.4, 1.4]);
        }
        for c in chokes() {
            b.boxs([c.0, c.1, 6.45], [10.2, 9.6, 0.9]);
        }
    });
    part("midplate", "kjoler", midplate, o(), &mut |b| {
        let z = 3.3;
        b.rounded_box([74.0, 59.0, z], [32.0, 94.0, 1.4], 0.6, 1, None);
        b.rounded_box([148.0, 59.0, z], [32.0, 94.0, 1.4], 0.6, 1, None);
        b.rounded_box([112.0, 95.0, z], [42.0, 22.0, 1.4], 0.6, 1, None);
        b.rounded_box([112.0, 28.0, z], [42.0, 22.0, 1.4], 0.6, 1, None);
    });
    part("vrmsink", "kjoler", alu, o(), &mut |b| {
        let v = &VRM;
        b.rounded_box([(v.x0 + v.x1) / 2.0, (v.y0 + v.y1) / 2.0, (v.z0 + v.z1) / 2.0], [v.x1 - v.x0, v.y1 - v.y0, v.z1 - v.z0], 0.8, 1, None);
        b.rounded_box([169.25, (v.y0 + v.y1) / 2.0, 6.0], [10.5, v.y1 - v.y0, 7.2], 0.8, 1, None);
        b.rounded_box([40.0, 86.5, 8.25], [12.0, 24.0, 2.7], 0.8, 1, None);
        b.rounded_box([29.25, 86.5, 6.0], [10.5, 24.0, 7.2], 0.8, 1, None);
    });
    let f = &FINS;
    part("fins", "kjoler", fin_block, Opts { lod: Some("far"), ..Opts::default() }, &mut |b| {
        let uvx = |x: f64| ((x - f.x0) / f.pitch) as f32;
        let (x0, x1, y0, y1, z0, z1) = (f.x0, f.x1, f.y0, f.y1, f.z0, f.z1);
        let end = [0.06f32, 0.5];
        let side = |a: f64, c: f64| Some([[uvx(a), 0.0], [uvx(c), 0.0], [uvx(c), 1.0], [uvx(a), 1.0]]);
        b.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0.0, 0.0, 1.0], side(x0, x1));
        b.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0.0, 0.0, -1.0], side(x1, x0));
        b.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0.0, 1.0, 0.0], side(x0, x1));
        b.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0.0, -1.0, 0.0], side(x0, x1));
        b.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1.0, 0.0, 0.0], Some([end; 4]));
        b.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1.0, 0.0, 0.0], Some([end; 4]));
    });
    part("fins", "kjoler", fin_alu, Opts { lod: Some("near"), ..Opts::default() }, &mut |b| {
        let n = ((f.x1 - f.x0) / f.pitch).floor() as usize;
        for i in 0..=n {
            let x = f.x0 + i as f64 * f.pitch;
            let end = i == 0 || i == n;
            b.boxs([x, (f.y0 + f.y1) / 2.0, (f.z0 + f.z1) / 2.0], [if end { 0.8 } else { f.t }, f.y1 - f.y0, f.z1 - f.z0]);
        }
        b.boxs([(f.x0 + f.x1) / 2.0, f.y1 - 0.3, f.z1 - 0.2], [f.x1 - f.x0, 0.6, 0.4]);
        b.boxs([(f.x0 + f.x1) / 2.0, f.y0 + 0.3, f.z1 - 0.2], [f.x1 - f.x0, 0.6, 0.4]);
        b.boxs([(f.x0 + f.x1) / 2.0, (f.y0 + f.y1) / 2.0, f.z0 + 0.3], [f.x1 - f.x0, f.y1 - f.y0, 0.6]);
    });
    part("pipes", "kjoler", copper, o(), &mut |b| {
        let ry = Mat4::from_rotation_y(FRAC_PI_2 as f32);
        let run = |b: &mut MeshBuilder, y: f64, z: f64, xa: f64, xb: f64| {
            b.with(ry, |b| {
                b.cylinder(-z, y, 3.0, xa, xb, 20, (false, false), None, None);
            });
        };
        let cap = |b: &mut MeshBuilder, y: f64, z: f64, x: f64, dir: f64| {
            b.with(t(x, y, z) * Mat4::from_rotation_y((dir * FRAC_PI_2) as f32), |b| {
                b.cylinder(0.0, 0.0, 3.0, 0.0, 0.6, 20, (false, true), None, Some(2.2));
            });
        };
        for (y, left) in PIPES {
            if left {
                let xl = 12.0;
                run(b, y, 11.0, xl, 282.0);
                run(b, y, 27.0, xl, xl + 50.0);
                b.torus_arc([xl, y, 19.0], 8.0, 3.0, FRAC_PI_2, 3.0 * FRAC_PI_2, 14, 18);
                cap(b, y, 11.0, 282.0, 1.0);
                cap(b, y, 27.0, xl + 50.0, 1.0);
            } else {
                let xr = 282.0;
                run(b, y, 11.0, 12.0, xr);
                run(b, y, 27.0, xr - 50.0, xr);
                b.torus_arc([xr, y, 19.0], 8.0, 3.0, -FRAC_PI_2, FRAC_PI_2, 14, 18);
                cap(b, y, 11.0, 12.0, -1.0);
                cap(b, y, 27.0, xr - 50.0, -1.0);
            }
        }
    });

    // ---- Deksel ----
    let s = &SHROUD;
    let a_pts = [[s.x0, 15.0], [15.0, s.y0], [s.x1 - 13.0, s.y0], [s.x1, 15.0], [s.x1, s.y1 - 10.0], [s.x1 - 5.0, s.y1 - 5.0], [s.x0 + 5.0, s.y1 - 5.0], [s.x0, s.y1 - 10.0]];
    let b_pts = [[s.x0, 15.0], [15.0, s.y0], [s.x1 - 13.0, s.y0], [s.x1, 15.0], [s.x1, s.y1 - 5.0], [s.x1 - 5.0, s.y1], [s.x0 + 5.0, s.y1], [s.x0, s.y1 - 5.0]];
    part("shroud", "deksel", shroud_m, o(), &mut |b| {
        b.extrude_convex(&a_pts, s.z_front - 2.0, s.z_front, true, true, false);
    });
    part("shroud", "deksel", skirt, o(), &mut |b| {
        let loft = |b: &mut MeshBuilder, p: &[[f64; 2]], za: f64, q: &[[f64; 2]], zb: f64| {
            for i in 0..p.len() {
                let j = (i + 1) % p.len();
                let a = [p[i][0], p[i][1], za];
                let c = [p[j][0], p[j][1], za];
                let d = [q[j][0], q[j][1], zb];
                let e = [q[i][0], q[i][1], zb];
                let u = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let v = [e[0] - a[0], e[1] - a[1], e[2] - a[2]];
                let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let out = [u[1], -u[0], 0.0];
                if n[0] * out[0] + n[1] * out[1] + n[2] * out[2] + n[2] * 0.001 < 0.0 {
                    n = [-n[0], -n[1], -n[2]];
                }
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                n = [n[0] / l, n[1] / l, n[2] / l];
                let ia = b.vert(a, n, None);
                let ic = b.vert(c, n, None);
                let id = b.vert(d, n, None);
                let ie = b.vert(e, n, None);
                b.add_quad_oriented(ia, ic, id, ie);
            }
        };
        loft(b, &a_pts, s.z_front, &b_pts, s.z_chamfer);
        loft(b, &b_pts, s.z_chamfer, &b_pts, s.z_back);
        for fp in &FANS {
            let seg = 72;
            let base = b.count();
            for i in 0..=seg {
                let a = i as f64 / seg as f64 * std::f64::consts::TAU;
                let n = [-a.cos(), -a.sin(), 0.0];
                b.vert([fp.cx + a.cos() * FAN.r_hole, fp.cy + a.sin() * FAN.r_hole, s.z_front - 2.0], n, None);
                b.vert([fp.cx + a.cos() * FAN.r_hole, fp.cy + a.sin() * FAN.r_hole, s.z_front], n, None);
            }
            for i in 0..seg as u32 {
                let a = base + i * 2;
                b.add_quad_oriented(a, a + 2, a + 3, a + 1);
            }
        }
    });
    part("shroud", "deksel", duct, o(), &mut |b| {
        for fp in &FANS {
            b.tube(fp.cx, fp.cy, FAN.r_hole + 1.4, FAN.r_hole, 34.5, s.z_front - 2.0, 72);
        }
    });
    part("shroud", "deksel", trim, o(), &mut |b| {
        let (z0, z1) = (s.z_front, s.z_front + 0.8);
        b.extrude_convex(&[[134.4, 14.2], [150.9, 14.2], [156.7, 97.1], [140.2, 97.1]], z0, z1, true, true, true);
        b.extrude_convex(&[[279.4, 15.4], [288.2, 25.2], [288.2, 81.3], [279.4, 91.0]], z0, z1, true, true, true);
        b.extrude_convex(&[[6.9, 20.3], [13.7, 14.2], [13.7, 98.4], [6.9, 91.0]], z0, z1, true, true, true);
    });

    // ---- Lysdiodene ----
    let mut led_pos: Vec<[f32; 3]> = Vec::with_capacity(56);
    let sq = FRAC_1_SQRT_2;
    part("bar", "deksel", led_m, o(), &mut |b| {
        let seg_w = (BAR_X1 - BAR_X0) / BAR_LEDS as f64;
        for i in 0..BAR_LEDS {
            b.led_index = i as f32;
            let cx = BAR_X0 + (i as f64 + 0.5) * seg_w;
            let cy = s.y1 - 2.5;
            let cz = s.z_front - 2.5;
            b.with(t(cx, cy, cz) * Mat4::from_rotation_x((-45.0 * DEG) as f32), |b| {
                b.rounded_box([0.0, 0.0, 0.25], [seg_w - 1.6, 2.9, 0.7], 0.3, 1, None);
            });
            led_pos.push([cx as f32, (cy + 0.5 * sq) as f32, (cz + 0.5 * sq) as f32]);
        }
    });
    let rings = ring_leds();
    for (fi, fp) in FANS.iter().enumerate() {
        part(if fi == 1 { "ring2" } else { "ring1" }, "deksel", led_m, o(), &mut |b| {
            for (i, l) in rings.iter().filter(|l| l.fan == fi).enumerate() {
                b.led_index = (20 + fi * 18 + i) as f32;
                b.arc_strip(fp.cx, fp.cy, RING_R0, RING_R1, l.a0, l.a1, s.z_front - 0.1, s.z_front + 0.55, 6);
                let am = (l.a0 + l.a1) / 2.0;
                let rm = (RING_R0 + RING_R1) / 2.0;
                led_pos.push([(fp.cx + am.cos() * rm) as f32, (fp.cy + am.sin() * rm) as f32, (s.z_front + 0.7) as f32]);
            }
        });
    }

    // ---- Vifter ----
    for (fi, fp) in FANS.iter().enumerate() {
        let g = if fi == 1 { "fan2" } else { "fan1" };
        let rot = Opts { cast: false, rotor: fi as i32, ..Opts::default() };
        part(g, "vifter", blade, rot, &mut |b| {
            for k in 0..FAN.blades {
                let phi = k as f64 / FAN.blades as f64 * std::f64::consts::TAU;
                b.grid(8, 7, |u, v| {
                    let r = FAN.r_hub - 0.8 + (FAN.r_rotor - FAN.r_hub + 0.8) * v;
                    let width = 0.5 + 0.12 * v;
                    let sweep = -0.42 * v * v;
                    let th = phi + sweep + (u - 0.5) * width;
                    let pitch = 9.2 - 3.6 * v;
                    let camber = 1.1 * (u * PI).sin() * (1.0 - 0.3 * v);
                    let z = FAN.z_mid - (u - 0.5) * pitch + camber;
                    [th.cos() * r, th.sin() * r, z]
                });
            }
        });
        part(g, "vifter", hub_side, Opts { cast: false, rotor: fi as i32, ..Opts::default() }, &mut |b| {
            b.cylinder(0.0, 0.0, FAN.r_hub, 35.5, 45.0, 40, (false, false), None, None);
        });
        part(g, "vifter", hub_m, Opts { cast: false, rotor: fi as i32, uv: Some(atlas_point("hub")), ..Opts::default() }, &mut |b| {
            b.cylinder(0.0, 0.0, FAN.r_hub, 45.0, 45.3, 40, (false, false), None, Some(FAN.r_hub - 0.6));
            b.disc(0.0, 0.0, FAN.r_hub - 0.6, 45.3, 1.0, 40, Some(atlas_uv("hub", 4.0)), 0.0);
        });
        part(g, "vifter", motor, o(), &mut |b| {
            b.cylinder(fp.cx, fp.cy, 14.0, 31.5, 35.6, 32, (true, false), None, None);
            for a in [100.0, 220.0, 340.0] {
                b.with(t(fp.cx, fp.cy, 33.0) * rz(a * DEG), |b| {
                    b.boxs([30.5, 0.0, 0.0], [33.0, 3.2, 2.0]);
                });
            }
        });
    }

    let f32v = |v: [f64; 3]| [v[0] as f32, v[1] as f32, v[2] as f32];
    Card {
        parts,
        led_pos,
        bar_a: f32v([BAR_X0 + 6.0, s.y1 + 3.0, s.z_front - 1.0]),
        bar_b: f32v([BAR_X1 - 6.0, s.y1 + 3.0, s.z_front - 1.0]),
        fan_light: [f32v([FANS[0].cx, FANS[0].cy, s.z_front + 7.0]), f32v([FANS[1].cx, FANS[1].cy, s.z_front + 7.0])],
        fan_centers: [[FANS[0].cx as f32, FANS[0].cy as f32], [FANS[1].cx as f32, FANS[1].cy as f32]],
        center: [((s.x0 + s.x1) / 2.0 - 2.0) as f32, 56.0, 20.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kortet_bygges_med_alle_lysdiodene() {
        let c = build();
        assert_eq!(c.led_pos.len(), 56);
        assert!(c.parts.len() > 40, "{}", c.parts.len());
        for p in &c.parts {
            assert!(GROUPS.iter().any(|g| g.0 == p.group), "{}", p.group);
            assert!(LAYERS.iter().any(|l| l.id == p.layer), "{}", p.layer);
            assert!(p.data.idx.iter().all(|i| (*i as usize) < p.data.verts()));
        }
        let leds: f32 = c.parts.iter().filter(|p| p.mat.led).flat_map(|p| p.data.led.iter()).cloned().fold(-1.0, f32::max);
        assert_eq!(leds, 55.0);
    }
}
