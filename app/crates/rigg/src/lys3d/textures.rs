//! Teksturene (port av `web/js/lys3d/textures.js`), tegnet med cairo i
//! stedet for canvas 2D: kretskortets for- og bakside (loddemaske, ledere,
//! gjennomføringer, puter, silketrykk), et etikettatlas for brikkene,
//! braketten og bakplaten (med åpninger i alfakanalen) og stripene for
//! kjøleribben på avstand. Ingen bildefiler, ingenting lastes ned.
//!
//! Tegnes én gang i en egen tråd ([`all`]) og lastes opp som RGBA8 med
//! mipmaps. Bildene er vendt (v opp), som `UNPACK_FLIP_Y` i demoen.

use super::layout::*;
use gtk::cairo::{self, Context, Format, ImageSurface};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    Atlas,
    PcbFront,
    PcbBack,
    Bracket,
    Backplate,
    FinStripe,
}

pub struct Image {
    pub tex: Tex,
    pub w: i32,
    pub h: i32,
    /// RGBA, ikke forhåndsmultiplisert, nederste rad først.
    pub rgba: Vec<u8>,
    pub repeat: bool,
}

const MONO: &str = "Ubuntu Sans Mono";
const SANS: &str = "Ubuntu Sans";

pub const ATLAS: f64 = 1024.0;

fn region(name: &str) -> [f64; 4] {
    match name {
        "white" => [0.0, 0.0, 16.0, 16.0],
        "mem" => [16.0, 0.0, 280.0, 240.0],
        "die" => [300.0, 0.0, 254.0, 216.0],
        "sub" => [560.0, 0.0, 400.0, 400.0],
        "choke" => [16.0, 250.0, 200.0, 200.0],
        "ps" => [220.0, 250.0, 120.0, 100.0],
        "cap" => [220.0, 360.0, 128.0, 128.0],
        "hub" => [16.0, 500.0, 256.0, 256.0],
        "ic" => [360.0, 410.0, 160.0, 160.0],
        "hdr" => [280.0, 580.0, 168.0, 64.0],
        _ => [960.0, 960.0, 64.0, 64.0],
    }
}

/// Uv-rektangelet for en region i atlaset (v opp).
pub fn atlas_uv(name: &str, inset: f64) -> [f32; 4] {
    let [x, y, w, h] = region(name);
    [((x + inset) / ATLAS) as f32, (1.0 - (y + h - inset) / ATLAS) as f32, ((x + w - inset) / ATLAS) as f32, (1.0 - (y + inset) / ATLAS) as f32]
}

/// Et punkt i regionens kant (sidene på brikken får samme farge).
pub fn atlas_point(name: &str) -> [f32; 2] {
    let [x, y, _, _] = region(name);
    [((x + 3.0) / ATLAS) as f32, (1.0 - (y + 3.0) / ATLAS) as f32]
}

pub fn bracket_uv(y: f64, z: f64) -> [f32; 2] {
    [((z - BRACKET.z0) / (BRACKET.z1 - BRACKET.z0)) as f32, ((y - BRACKET.y0) / (BRACKET.y1 - BRACKET.y0)) as f32]
}

pub fn backplate_uv(x: f64, y: f64) -> [f32; 2] {
    let b = &BACKPLATE;
    [(1.0 - (x - b.x0) / (b.x1 - b.x0)) as f32, ((y - b.y0) / (b.y1 - b.y0)) as f32]
}

// ---------------------------------------------------------------- hjelpere

fn hex(cr: &Context, h: &str) {
    let n = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0);
    cr.set_source_rgb(((n >> 16) & 255) as f64 / 255.0, ((n >> 8) & 255) as f64 / 255.0, (n & 255) as f64 / 255.0);
}

fn rgba(cr: &Context, r: f64, g: f64, b: f64, a: f64) {
    cr.set_source_rgba(r / 255.0, g / 255.0, b / 255.0, a);
}

fn fill_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64) {
    cr.rectangle(x, y, w, h);
    let _ = cr.fill();
}

fn circle(cr: &Context, x: f64, y: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x, y, r, 0.0, TAU);
    let _ = cr.fill();
}

fn round_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w.abs() / 2.0).min(h.abs() / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// Tekst med canvas' `textBaseline = 'middle'` og `textAlign`.
#[allow(clippy::too_many_arguments)]
fn text_at(cr: &Context, s: &str, x: f64, y: f64, size: f64, family: &str, bold: bool, align: f64) {
    cr.select_font_face(family, cairo::FontSlant::Normal, if bold { cairo::FontWeight::Bold } else { cairo::FontWeight::Normal });
    cr.set_font_size(size);
    let (w, mid) = match cr.text_extents(s) {
        Ok(e) => (e.x_advance(), e.y_bearing() + e.height() / 2.0),
        Err(_) => (0.0, 0.0),
    };
    cr.move_to(x - w * align, y - mid);
    let _ = cr.show_text(s);
}

fn surface(w: i32, h: i32) -> (ImageSurface, Context) {
    let s = ImageSurface::create(Format::ARgb32, w, h).expect("cairo-flate");
    let cr = Context::new(&s).expect("cairo");
    (s, cr)
}

/// Fra cairos ARGB32 (forhåndsmultiplisert, BGRA i minnet) til RGBA, vendt.
fn finish(tex: Tex, s: ImageSurface, cr: Context, repeat: bool) -> Image {
    drop(cr);
    s.flush();
    let (w, h) = (s.width(), s.height());
    let stride = s.stride() as usize;
    let mut out = vec![0u8; (w * h * 4) as usize];
    let data = s.take_data().expect("bildedata");
    for y in 0..h as usize {
        let src = &data[y * stride..y * stride + w as usize * 4];
        let dy = h as usize - 1 - y;
        let dst = &mut out[dy * w as usize * 4..(dy + 1) * w as usize * 4];
        for (d, p) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            let a = p[3] as u32;
            if a == 0 {
                d.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                d[0] = ((p[2] as u32 * 255 + a / 2) / a).min(255) as u8;
                d[1] = ((p[1] as u32 * 255 + a / 2) / a).min(255) as u8;
                d[2] = ((p[0] as u32 * 255 + a / 2) / a).min(255) as u8;
                d[3] = a as u8;
            }
        }
    }
    Image { tex, w, h, rgba: out, repeat }
}

// ---------------------------------------------------------------- atlaset

fn make_atlas() -> Image {
    let n = ATLAS as i32;
    let (s, g) = surface(n, n);
    hex(&g, "#808080");
    fill_rect(&g, 0.0, 0.0, ATLAS, ATLAS);
    let mut r = Rng::new(7);
    let reg = |g: &Context, name: &str, fill: &str| -> [f64; 4] {
        let q = region(name);
        hex(g, fill);
        fill_rect(g, q[0], q[1], q[2], q[3]);
        q
    };
    reg(&g, "white", "#ffffff");
    reg(&g, "plain", "#ffffff");
    // GDDR6: matt svart med lasermerking og pinne 1-prikk.
    {
        let [x, y, w, h] = reg(&g, "mem", "#16171a");
        for _ in 0..900 {
            let a = 0.012 + r.next() * 0.02;
            rgba(&g, 255.0, 255.0, 255.0, a);
            fill_rect(&g, x + r.next() * w, y + r.next() * h, 2.0, 2.0);
        }
        hex(&g, "#26282c");
        circle(&g, x + 26.0, y + 26.0, 9.0);
        hex(&g, "#80848b");
        text_at(&g, "GDDR6 8Gb", x + 52.0, y + 74.0, 30.0, MONO, true, 0.0);
        text_at(&g, "14G  X2A41", x + 52.0, y + 118.0, 24.0, MONO, false, 0.0);
        text_at(&g, "BGA180 · 2042", x + 52.0, y + 156.0, 24.0, MONO, false, 0.0);
        hex(&g, "#6a6e75");
        text_at(&g, "8 Gb · 1.35 V", x + 52.0, y + 194.0, 20.0, MONO, false, 0.0);
    }
    // GPU-brikken: mørk, med svak struktur og kant.
    {
        let [x, y, w, h] = reg(&g, "die", "#1d1c26");
        let gr = cairo::LinearGradient::new(x, y, x + w, y + h);
        gr.add_color_stop_rgba(0.0, 120.0 / 255.0, 110.0 / 255.0, 170.0 / 255.0, 0.10);
        gr.add_color_stop_rgba(0.5, 0.0, 0.0, 0.0, 0.0);
        gr.add_color_stop_rgba(1.0, 90.0 / 255.0, 120.0 / 255.0, 150.0 / 255.0, 0.08);
        let _ = g.set_source(&gr);
        fill_rect(&g, x, y, w, h);
        rgba(&g, 160.0, 160.0, 190.0, 0.10);
        g.set_line_width(1.0);
        for i in 1..6 {
            let i = i as f64;
            g.rectangle(x + w * i / 12.0, y + h * i / 12.0, w - w * i / 6.0, h - h * i / 6.0);
            let _ = g.stroke();
        }
        rgba(&g, 200.0, 200.0, 220.0, 0.25);
        g.set_line_width(3.0);
        g.rectangle(x + 2.0, y + 2.0, w - 4.0, h - 4.0);
        let _ = g.stroke();
    }
    // Substratet: mørk grønnbrun, gylne merker og små kondensatorer.
    {
        let [x, y, w, h] = reg(&g, "sub", "#2a3324");
        for _ in 0..1600 {
            let light = r.next() < 0.5;
            let a = 0.03 + r.next() * 0.05;
            if light {
                rgba(&g, 200.0, 190.0, 140.0, a);
            } else {
                rgba(&g, 0.0, 0.0, 0.0, a);
            }
            fill_rect(&g, x + r.next() * w, y + r.next() * h, 2.0, 2.0);
        }
        hex(&g, "#c9a24e");
        g.move_to(x + 14.0, y + 14.0);
        g.line_to(x + 44.0, y + 14.0);
        g.line_to(x + 14.0, y + 44.0);
        g.close_path();
        let _ = g.fill();
        let sc = w / 40.0;
        let (dw, dh) = (GPU.die_w * sc, GPU.die_h * sc);
        let (ox, oy) = (x + (w - dw) / 2.0, y + (h - dh) / 2.0);
        for i in 0..18 {
            let fi = i as f64 / 17.0;
            for (px, py, rot) in [(ox + fi * dw, oy - 2.6 * sc, false), (ox + fi * dw, oy + dh + 2.6 * sc, false), (ox - 2.6 * sc, oy + fi * dh, true), (ox + dw + 2.6 * sc, oy + fi * dh, true)] {
                let cw = if rot { 0.5 } else { 1.0 } * sc;
                let ch = if rot { 1.0 } else { 0.5 } * sc;
                hex(&g, "#9c8a64");
                fill_rect(&g, px - cw / 2.0, py - ch / 2.0, cw, ch);
                hex(&g, "#c9cbc8");
                if rot {
                    fill_rect(&g, px - cw / 2.0, py - ch / 2.0, cw, ch * 0.22);
                    fill_rect(&g, px - cw / 2.0, py + ch / 2.0 - ch * 0.22, cw, ch * 0.22);
                } else {
                    fill_rect(&g, px - cw / 2.0, py - ch / 2.0, cw * 0.22, ch);
                    fill_rect(&g, px + cw / 2.0 - cw * 0.22, py - ch / 2.0, cw * 0.22, ch);
                }
            }
        }
        rgba(&g, 210.0, 205.0, 170.0, 0.55);
        text_at(&g, "GPU-545", x + 26.0, y + h - 30.0, 22.0, MONO, true, 0.0);
        text_at(&g, "A1  2042", x + w - 120.0, y + h - 30.0, 18.0, MONO, false, 0.0);
    }
    // Spole: grå metallkompositt med merking.
    {
        let [x, y, w, h] = reg(&g, "choke", "#3d4045");
        for _ in 0..1400 {
            let light = r.next() < 0.5;
            let a = 0.03 + r.next() * 0.05;
            if light {
                rgba(&g, 255.0, 255.0, 255.0, a);
            } else {
                rgba(&g, 0.0, 0.0, 0.0, a);
            }
            fill_rect(&g, x + r.next() * w, y + r.next() * h, 2.0, 2.0);
        }
        rgba(&g, 0.0, 0.0, 0.0, 0.35);
        g.set_line_width(4.0);
        g.rectangle(x + 6.0, y + 6.0, w - 12.0, h - 12.0);
        let _ = g.stroke();
        hex(&g, "#a7abb1");
        text_at(&g, "R22", x + w / 2.0, y + h / 2.0 - 6.0, 52.0, MONO, true, 0.5);
        hex(&g, "#8a8e94");
        text_at(&g, "2042", x + w / 2.0, y + h / 2.0 + 40.0, 22.0, MONO, false, 0.5);
    }
    // Effekttrinn (MOSFET).
    {
        let [x, y, w, h] = reg(&g, "ps", "#1a1b1e");
        hex(&g, "#6d7178");
        text_at(&g, "PS50", x + w / 2.0, y + h / 2.0 - 10.0, 22.0, MONO, true, 0.5);
        text_at(&g, "A21", x + w / 2.0, y + h / 2.0 + 16.0, 16.0, MONO, false, 0.5);
        hex(&g, "#2a2c30");
        circle(&g, x + 14.0, y + 14.0, 6.0);
    }
    // Polymerkondensator sett ovenfra.
    {
        let [x, y, w, h] = reg(&g, "cap", "#b9bdc2");
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let rg = cairo::RadialGradient::new(cx - 14.0, cy - 14.0, 4.0, cx, cy, w / 2.0);
        rg.add_color_stop_rgb(0.0, 0xd9 as f64 / 255.0, 0xdc as f64 / 255.0, 0xdf as f64 / 255.0);
        rg.add_color_stop_rgb(1.0, 0xa3 as f64 / 255.0, 0xa8 as f64 / 255.0, 0xae as f64 / 255.0);
        let _ = g.set_source(&rg);
        fill_rect(&g, x, y, w, h);
        rgba(&g, 40.0, 44.0, 50.0, 0.55);
        g.set_line_width(5.0);
        g.move_to(cx - w * 0.3, cy);
        g.line_to(cx + w * 0.3, cy);
        g.move_to(cx, cy - h * 0.3);
        g.line_to(cx, cy + h * 0.3);
        let _ = g.stroke();
        hex(&g, "#2b2f36");
        g.set_line_width(14.0);
        g.new_sub_path();
        g.arc(cx, cy, w / 2.0 - 8.0, PI * 0.75, PI * 1.25);
        let _ = g.stroke();
    }
    // Navet på viften.
    {
        let [x, y, w, h] = reg(&g, "hub", "#1f2226");
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        hex(&g, "#3b4048");
        g.set_line_width(3.0);
        g.new_sub_path();
        g.arc(cx, cy, w * 0.36, 0.0, TAU);
        let _ = g.stroke();
        hex(&g, "#2c3036");
        g.set_line_width(2.0);
        g.new_sub_path();
        g.arc(cx, cy, w * 0.2, 0.0, TAU);
        let _ = g.stroke();
        hex(&g, "#2a2e34");
        circle(&g, cx, cy, w * 0.06);
    }
    // Generisk IC.
    {
        let [x, y, w, h] = reg(&g, "ic", "#18191c");
        hex(&g, "#2a2c30");
        circle(&g, x + 20.0, y + 20.0, 8.0);
        hex(&g, "#74787f");
        text_at(&g, "8+2", x + w / 2.0, y + h / 2.0 - 12.0, 26.0, MONO, true, 0.5);
        text_at(&g, "2042A", x + w / 2.0, y + h / 2.0 + 20.0, 18.0, MONO, false, 0.5);
    }
    // Pinnehode: hvit plast med åpninger.
    {
        let [x, y, _, _] = reg(&g, "hdr", "#e8e6df");
        hex(&g, "#3b3c3e");
        for i in 0..4 {
            fill_rect(&g, x + 12.0 + i as f64 * 40.0, y + 18.0, 24.0, 28.0);
        }
    }
    finish(Tex::Atlas, s, g, false)
}

// ---------------------------------------------------------------- kretskortet

const PW: f64 = 2048.0;
const PH: f64 = 1024.0;
const SX: f64 = PW / (PCB_TEX.x1 - PCB_TEX.x0);
const SY: f64 = PH / (PCB_TEX.y1 - PCB_TEX.y0);

const MASK: &str = "#0e1f17";
const COPPER: &str = "#15322a";
const GOLD: &str = "#c9a24e";
const PAD: &str = "#b9b8ae";

fn trace(g: &Context, a: f64) {
    rgba(g, 46.0, 96.0, 72.0, a);
}
fn silk(g: &Context) {
    rgba(g, 226.0, 232.0, 226.0, 0.86);
}

/// Millimeter med y opp. Baksiden tegnes speilvendt i x (sett bakfra).
fn mm(g: &Context, back: bool) {
    let m = if back { cairo::Matrix::new(-SX, 0.0, 0.0, -SY, PW, PCB_TEX.y1 * SY) } else { cairo::Matrix::new(SX, 0.0, 0.0, -SY, 0.0, PCB_TEX.y1 * SY) };
    g.set_matrix(m);
}

/// Tekst i mm-koordinater (rett vei).
#[allow(clippy::too_many_arguments)]
fn text(g: &Context, s: &str, x: f64, y: f64, size: f64, align: f64, color: Option<&str>, family: &str, bold: bool, back: bool, rot: f64) {
    let _ = g.save();
    g.translate(x, y);
    g.scale(if back { -1.0 } else { 1.0 }, -1.0);
    if rot != 0.0 {
        g.rotate(rot);
    }
    match color {
        Some(c) => hex(g, c),
        None => silk(g),
    }
    text_at(g, s, 0.0, 0.0, size, family, bold, align);
    let _ = g.restore();
}

fn silk_rect(g: &Context, x: f64, y: f64, w: f64, h: f64, pin1: bool) {
    silk(g);
    g.set_line_width(0.16);
    g.rectangle(x - w / 2.0, y - h / 2.0, w, h);
    let _ = g.stroke();
    if pin1 {
        circle(g, x - w / 2.0 - 0.9, y + h / 2.0 + 0.6, 0.35);
    }
}

fn corners(g: &Context, x: f64, y: f64, w: f64, h: f64, l: f64) {
    silk(g);
    g.set_line_width(0.16);
    let (x0, x1, y0, y1) = (x - w / 2.0, x + w / 2.0, y - h / 2.0, y + h / 2.0);
    for (cx, cy, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        g.move_to(cx + dx * l, cy);
        g.line_to(cx, cy);
        g.line_to(cx, cy + dy * l);
    }
    let _ = g.stroke();
}

fn base_board(g: &Context, r: &mut Rng) {
    g.identity_matrix();
    hex(g, MASK);
    fill_rect(g, 0.0, 0.0, PW, PH);
    for _ in 0..26000 {
        let light = r.next() < 0.5;
        let a = 0.012 + r.next() * 0.018;
        if light {
            rgba(g, 255.0, 255.0, 255.0, a);
        } else {
            rgba(g, 0.0, 0.0, 0.0, a);
        }
        fill_rect(g, r.next() * PW, r.next() * PH, 1.5, 1.5);
    }
}

fn pours(g: &Context, back: bool) {
    mm(g, back);
    let n = u32::from_str_radix(&COPPER[1..], 16).unwrap_or(0);
    let (cr, cg, cb) = (((n >> 16) & 255) as f64, ((n >> 8) & 255) as f64, (n & 255) as f64);
    rgba(g, cr, cg, cb, 0.55);
    g.move_to(160.0, 4.0);
    g.line_to(232.0, 4.0);
    g.line_to(262.0, 34.0);
    g.line_to(262.0, 96.0);
    g.line_to(232.0, 108.0);
    g.line_to(160.0, 108.0);
    g.close_path();
    let _ = g.fill();
    g.rectangle(18.0, 70.0, 30.0, 34.0);
    let _ = g.fill();
    rgba(g, cr, cg, cb, 0.35);
    fill_rect(g, 1.2, 1.2, 264.6, 2.2);
    fill_rect(g, 1.2, 107.6, 264.6, 2.2);
    hex(g, MASK);
    g.set_line_width(0.5);
    g.move_to(200.0, 4.0);
    g.line_to(200.0, 108.0);
    g.move_to(160.0, 56.0);
    g.line_to(200.0, 56.0);
    let _ = g.stroke();
}

/// Bunter av ledere langs en sti. Punktene: (x, y, ox, oy) der (ox, oy) er
/// retningen bunten spres i.
fn traces_to(g: &Context, pts: &[(f64, f64, f64, f64)], n: usize, spread: f64, w: f64) {
    trace(g, 0.55);
    g.set_line_width(w);
    g.set_line_join(cairo::LineJoin::Round);
    for k in 0..n {
        let o = (k as f64 - (n as f64 - 1.0) / 2.0) * spread;
        for (i, (x, y, ox, oy)) in pts.iter().enumerate() {
            let (xx, yy) = (x + o * ox, y + o * oy);
            if i == 0 {
                g.move_to(xx, yy);
            } else {
                g.line_to(xx, yy);
            }
        }
        let _ = g.stroke();
    }
}

fn vias(g: &Context, r: &mut Rng, x0: f64, y0: f64, x1: f64, y1: f64, n: usize) {
    for _ in 0..n {
        let x = x0 + r.next() * (x1 - x0);
        let y = y0 + r.next() * (y1 - y0);
        rgba(g, 60.0, 90.0, 74.0, 0.9);
        circle(g, x, y, 0.32);
        hex(g, "#07100c");
        circle(g, x, y, 0.14);
    }
}

fn holes(g: &Context) {
    for (x, y) in HOLES {
        hex(g, GOLD);
        circle(g, x, y, 3.1);
        hex(g, "#d9b968");
        circle(g, x, y, 2.2);
        hex(g, "#050706");
        circle(g, x, y, 1.45);
        for i in 0..8 {
            let a = i as f64 / 8.0 * TAU;
            hex(g, "#b08a3e");
            circle(g, x + a.cos() * 2.65, y + a.sin() * 2.65, 0.22);
        }
    }
}

fn fingers(g: &Context) {
    hex(g, GOLD);
    for x in finger_xs() {
        let [x0, y0, x1, y1] = finger_rect(x);
        fill_rect(g, x0, y0, x1 - x0, y1 - y0);
    }
}

fn make_pcb_front() -> Image {
    let (s, g) = surface(PW as i32, PH as i32);
    let mut r = Rng::new(1234);
    base_board(&g, &mut r);
    pours(&g, false);
    mm(&g, false);
    let gx0 = GPU.cx - GPU.sub / 2.0;
    let gx1 = GPU.cx + GPU.sub / 2.0;
    let gy1 = GPU.cy + GPU.sub / 2.0;
    let p = |x: f64, y: f64| (x, y, 0.0, 1.0);
    let px = |x: f64, y: f64| (x, y, 1.0, 0.0);
    for m in memory() {
        if m.r && m.x < GPU.cx {
            traces_to(&g, &[p(m.x + m.w / 2.0, m.y), p(m.x + m.w / 2.0 + 4.0, m.y), p(gx0 - 4.0, m.y + (GPU.cy - m.y) * 0.55), p(gx0, m.y + (GPU.cy - m.y) * 0.55)], 22, 0.42, 0.16);
        } else if m.r {
            traces_to(&g, &[p(m.x - m.w / 2.0, m.y), p(m.x - m.w / 2.0 - 4.0, m.y), p(gx1 + 4.0, m.y + (GPU.cy - m.y) * 0.55), p(gx1, m.y + (GPU.cy - m.y) * 0.55)], 22, 0.42, 0.16);
        } else {
            traces_to(&g, &[px(m.x, m.y - m.h / 2.0), px(m.x, m.y - m.h / 2.0 - 3.0), px(m.x + (GPU.cx - m.x) * 0.5, gy1 + 2.0), px(m.x + (GPU.cx - m.x) * 0.5, gy1)], 22, 0.36, 0.16);
        }
    }
    for i in 0..16 {
        let fx = 66.8 + i as f64 * 4.2;
        let tx = GPU.cx - 15.0 + i as f64 * 2.0;
        traces_to(&g, &[px(fx, -1.0), px(fx, 11.0), px(fx, 15.0), px(tx + (fx - tx) * 0.2, 26.0), px(tx, GPU.cy - GPU.sub / 2.0)], 2, 0.34, 0.14);
    }
    for (k, pt) in PORTS.iter().enumerate() {
        let y = (pt.y0 + pt.y1) / 2.0;
        let yk = 104.0 - k as f64 * 2.4;
        traces_to(&g, &[p(15.0, y), p(24.0, y), p(30.0, yk), p(60.0, yk), p(GPU.cx - 24.0, yk), p(GPU.cx - 18.0, gy1)], 4, 0.3, 0.13);
    }
    trace(&g, 0.35);
    g.set_line_width(3.2);
    for ch in chokes().iter().take(8) {
        g.move_to(ch.0 - 6.0, ch.1);
        g.line_to(162.0, ch.1 + (GPU.cy - ch.1) * 0.25);
        let _ = g.stroke();
    }
    vias(&g, &mut r, 160.0, 4.0, 262.0, 108.0, 420);
    vias(&g, &mut r, 4.0, 4.0, 160.0, 108.0, 520);
    vias(&g, &mut r, GPU.cx - 24.0, GPU.cy - 24.0, GPU.cx + 24.0, GPU.cy + 24.0, 220);
    hex(&g, PAD);
    for q in small_parts() {
        if q.w > q.h {
            fill_rect(&g, q.x - q.w / 2.0 - 0.15, q.y - q.h / 2.0 - 0.1, q.w * 0.36, q.h + 0.2);
            fill_rect(&g, q.x + q.w / 2.0 - q.w * 0.36 + 0.15, q.y - q.h / 2.0 - 0.1, q.w * 0.36, q.h + 0.2);
        } else {
            fill_rect(&g, q.x - q.w / 2.0 - 0.1, q.y - q.h / 2.0 - 0.15, q.w + 0.2, q.h * 0.36);
            fill_rect(&g, q.x - q.w / 2.0 - 0.1, q.y + q.h / 2.0 - q.h * 0.36 + 0.15, q.w + 0.2, q.h * 0.36);
        }
    }
    for ch in chokes() {
        fill_rect(&g, ch.0 - 5.6, ch.1 - 4.0, 2.4, 8.0);
        fill_rect(&g, ch.0 + 3.2, ch.1 - 4.0, 2.4, 8.0);
    }
    for ps in powerstages() {
        fill_rect(&g, ps.0 - 3.1, ps.1 - 2.6, 6.2, 5.2);
    }
    for c in caps() {
        fill_rect(&g, c.0 - 3.9, c.1 - 1.0, 1.6, 2.0);
        fill_rect(&g, c.0 + 2.3, c.1 - 1.0, 1.6, 2.0);
    }
    fingers(&g);
    holes(&g);
    // Silketrykk.
    silk_rect(&g, GPU.cx, GPU.cy, GPU.sub + 1.4, GPU.sub + 1.4, true);
    text(&g, "U1", GPU.cx - GPU.sub / 2.0 - 0.6, GPU.cy - GPU.sub / 2.0 - 2.2, 2.2, 0.0, None, SANS, true, false, 0.0);
    for m in memory() {
        corners(&g, m.x, m.y, m.w + 1.2, m.h + 1.2, 1.8);
        let (tx, ty) = if m.r { (m.x + if m.x < GPU.cx { -m.w / 2.0 - 2.6 } else { m.w / 2.0 + 2.6 }, m.y) } else { (m.x, m.y + m.h / 2.0 + 1.9) };
        text(&g, &format!("M{}", m.n), tx, ty, 1.7, 0.5, None, SANS, true, false, 0.0);
    }
    for ch in chokes() {
        silk_rect(&g, ch.0, ch.1, 11.2, 10.4, false);
        text(&g, &ch.2, ch.0 + if ch.0 > 100.0 { 7.6 } else { -7.4 }, ch.1 + 4.3, 1.5, 0.5, None, SANS, true, false, 0.0);
    }
    for c in caps() {
        silk(&g);
        g.set_line_width(0.16);
        g.new_sub_path();
        g.arc(c.0, c.1, c.2 + 0.6, 0.0, TAU);
        let _ = g.stroke();
        rgba(&g, 226.0, 232.0, 226.0, 0.86 * 0.35);
        g.move_to(c.0, c.1);
        g.arc(c.0, c.1, c.2 + 0.6, PI * 0.75, PI * 1.25);
        g.close_path();
        let _ = g.fill();
    }
    for pw in &POWER {
        silk_rect(&g, (pw.x0 + pw.x1) / 2.0, (pw.y0 + 111.0) / 2.0, pw.x1 - pw.x0 + 1.2, 111.0 - pw.y0 + 0.6, false);
        text(&g, pw.label, (pw.x0 + pw.x1) / 2.0, pw.y0 - 2.4, 2.0, 0.5, None, SANS, true, false, 0.0);
        text(&g, pw.reference, pw.x0 - 0.6, pw.y0 - 2.4, 1.5, 1.0, None, SANS, true, false, 0.0);
    }
    for ic in &ICS {
        silk_rect(&g, ic.x, ic.y, ic.w + 1.2, ic.h + 1.2, true);
        text(&g, ic.reference, ic.x + ic.w / 2.0 + 1.2, ic.y + ic.h / 2.0 + 1.0, 1.4, 0.0, None, SANS, true, false, 0.0);
    }
    for h in &HEADERS {
        silk_rect(&g, h.x, h.y, h.w + 1.0, h.h + 1.0, false);
        text(&g, h.label, h.x - h.w / 2.0 - 1.0, h.y, 1.6, 1.0, None, SANS, true, false, 0.0);
    }
    for pt in &PORTS {
        text(&g, pt.label, 17.6, (pt.y0 + pt.y1) / 2.0, 1.6, 0.5, None, SANS, true, false, -PI / 2.0);
    }
    text(&g, "AR-2080 · REV A", 246.0, 54.0, 2.4, 0.5, None, SANS, true, false, 0.0);
    text(&g, "PCB 12L · 94V-0 · 2042", 246.0, 50.4, 1.6, 0.5, None, SANS, false, false, 0.0);
    text(&g, "PCIe x16", 100.0, 2.8, 1.6, 0.5, None, SANS, true, false, 0.0);
    text(&g, "1", FINGERS.tab_a.0 + 1.6, 1.4, 1.2, 0.5, None, SANS, true, false, 0.0);
    text(&g, "82", FINGERS.tab_b.1 - 1.6, 1.4, 1.2, 0.5, None, SANS, true, false, 0.0);
    rgba(&g, 226.0, 232.0, 226.0, 0.22);
    g.set_line_width(0.18);
    g.rectangle(1.4, 1.4, 264.2, 108.2);
    let _ = g.stroke();
    finish(Tex::PcbFront, s, g, false)
}

fn make_pcb_back() -> Image {
    let (s, g) = surface(PW as i32, PH as i32);
    let mut r = Rng::new(4321);
    base_board(&g, &mut r);
    pours(&g, true);
    mm(&g, true);
    trace(&g, 0.55);
    g.set_line_width(0.16);
    for _ in 0..46 {
        let x = 20.0 + r.next() * 230.0;
        let y = 6.0 + r.next() * 98.0;
        let len = 10.0 + r.next() * 40.0;
        let dx = if r.next() < 0.5 { 1.0 } else { -1.0 };
        let dy = if r.next() < 0.5 { 1.0 } else { -1.0 };
        let n = 3 + (r.next() * 6.0) as usize;
        for k in 0..n {
            let o = k as f64 * 0.5;
            g.move_to(x, y + o);
            g.line_to(x + dx * len * 0.6, y + o);
            g.line_to(x + dx * len * 0.6 + dy * 6.0, y + o + dy * 6.0);
            g.line_to(x + dx * len, y + o + dy * 6.0);
            let _ = g.stroke();
        }
    }
    for i in 0..16 {
        let fx = 66.8 + i as f64 * 4.2;
        g.move_to(fx, -1.0);
        g.line_to(fx, 10.0);
        let _ = g.stroke();
    }
    vias(&g, &mut r, 4.0, 4.0, 262.0, 108.0, 1300);
    vias(&g, &mut r, GPU.cx - 22.0, GPU.cy - 22.0, GPU.cx + 22.0, GPU.cy + 22.0, 380);
    hex(&g, PAD);
    for q in back_caps() {
        fill_rect(&g, q.x - q.w / 2.0 - 0.15, q.y - q.h / 2.0 - 0.1, q.w * 0.36, q.h + 0.2);
        fill_rect(&g, q.x + q.w / 2.0 - q.w * 0.36 + 0.15, q.y - q.h / 2.0 - 0.1, q.w * 0.36, q.h + 0.2);
    }
    silk_rect(&g, GPU.cx, GPU.cy, 30.0, 30.0, false);
    fingers(&g);
    holes(&g);
    // Etikett med strekkode (generisk).
    hex(&g, "#e9e7e1");
    fill_rect(&g, 200.0, 72.0, 42.0, 18.0);
    hex(&g, "#1b1c1e");
    let mut bx = 203.0;
    while bx < 238.0 {
        let w = 0.25 + r.next() * 0.7;
        fill_rect(&g, bx, 79.0, w, 9.0);
        bx += w + 0.25 + r.next() * 0.6;
    }
    text(&g, "AR-2080-8G", 221.0, 76.3, 2.0, 0.5, Some("#1b1c1e"), MONO, true, true, 0.0);
    text(&g, "S/N 0000-0000-0001", 221.0, 73.6, 1.6, 0.5, Some("#3b3c3e"), MONO, false, true, 0.0);
    text(&g, "AR-2080 · REV A", 60.0, 56.0, 2.4, 0.5, None, SANS, true, true, 0.0);
    for (k, pw) in POWER.iter().enumerate() {
        text(&g, if k == 1 { "+12V 6P" } else { "+12V 8P" }, (pw.x0 + pw.x1) / 2.0, 104.0, 1.6, 0.5, None, SANS, true, true, 0.0);
    }
    rgba(&g, 226.0, 232.0, 226.0, 0.22);
    g.set_line_width(0.18);
    g.rectangle(1.4, 1.4, 264.2, 108.2);
    let _ = g.stroke();
    finish(Tex::PcbBack, s, g, false)
}

// ---------------------------------------------------------------- braketten og bakplaten

fn cut(g: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let _ = g.save();
    g.set_operator(cairo::Operator::DestOut);
    g.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    round_rect(g, x, y, w, h, r);
    let _ = g.fill();
    let _ = g.restore();
}

fn make_bracket() -> Image {
    let (w, h) = (512.0, 1024.0);
    let (s, g) = surface(w as i32, h as i32);
    let sx = w / (BRACKET.z1 - BRACKET.z0);
    let sy = h / (BRACKET.y1 - BRACKET.y0);
    hex(&g, "#ffffff");
    fill_rect(&g, 0.0, 0.0, w, h);
    let mut r = Rng::new(99);
    for _ in 0..3000 {
        let a = 0.02 + r.next() * 0.04;
        rgba(&g, 0.0, 0.0, 0.0, a);
        let (x, y, l) = (r.next() * w, r.next() * h, 6.0 + r.next() * 30.0);
        fill_rect(&g, x, y, 1.0, l);
    }
    g.set_matrix(cairo::Matrix::new(sx, 0.0, 0.0, -sy, -BRACKET.z0 * sx, BRACKET.y1 * sy));
    for p in &PORTS {
        cut(&g, p.z0 - 0.7, p.y0 - 0.7, p.z1 - p.z0 + 1.4, p.y1 - p.y0 + 1.4, p.r + 0.6);
    }
    for j in 0..15 {
        for i in 0..3 {
            let z0 = 11.0 + i as f64 * 7.6;
            let y0 = 6.0 + j as f64 * 6.6;
            cut(&g, z0, y0, 6.0, 4.0, 1.4);
        }
    }
    finish(Tex::Bracket, s, g, false)
}

fn make_backplate() -> Image {
    let (w, h) = (2048.0, 1024.0);
    let (s, g) = surface(w as i32, h as i32);
    let b = &BACKPLATE;
    let sx = w / (b.x1 - b.x0);
    let sy = h / (b.y1 - b.y0);
    hex(&g, "#ffffff");
    fill_rect(&g, 0.0, 0.0, w, h);
    let mut r = Rng::new(55);
    for _ in 0..9000 {
        let v = if r.next() < 0.5 { 0.0 } else { 255.0 };
        let a = 0.02 + r.next() * 0.035;
        rgba(&g, v, v, v, a);
        let (x, y, l) = (r.next() * w, r.next() * h, 20.0 + r.next() * 120.0);
        fill_rect(&g, x, y, l, 1.0);
    }
    g.set_matrix(cairo::Matrix::new(-sx, 0.0, 0.0, -sy, b.x1 * sx, b.y1 * sy));
    rgba(&g, 0.0, 0.0, 0.0, 0.38);
    g.set_line_width(0.6);
    for path in [&[(170.0, 8.0), (250.0, 8.0), (262.0, 20.0), (262.0, 92.0), (250.0, 104.0), (170.0, 104.0)][..], &[(150.0, 14.0), (150.0, 98.0)][..], &[(40.0, 104.0), (14.0, 104.0), (6.0, 96.0), (6.0, 16.0), (14.0, 8.0), (40.0, 8.0)][..]] {
        for (i, (x, y)) in path.iter().enumerate() {
            if i == 0 {
                g.move_to(*x, *y);
            } else {
                g.line_to(*x, *y);
            }
        }
    }
    let _ = g.stroke();
    rgba(&g, 255.0, 255.0, 255.0, 0.10);
    g.set_line_width(0.3);
    g.move_to(170.0, 8.6);
    g.line_to(250.0, 8.6);
    g.move_to(150.6, 14.0);
    g.line_to(150.6, 98.0);
    let _ = g.stroke();
    let wd = &b.win;
    rgba(&g, 0.0, 0.0, 0.0, 0.5);
    g.set_line_width(1.6);
    round_rect(&g, wd.x0, wd.y0, wd.x1 - wd.x0, wd.y1 - wd.y0, 4.0);
    let _ = g.stroke();
    cut(&g, wd.x0, wd.y0, wd.x1 - wd.x0, wd.y1 - wd.y0, 4.0);
    for i in 0..9 {
        let x = 176.0 + i as f64 * 8.6;
        rgba(&g, 0.0, 0.0, 0.0, 0.45);
        g.set_line_width(1.2);
        round_rect(&g, x, 30.0, 3.2, 52.0, 1.6);
        let _ = g.stroke();
        cut(&g, x, 30.0, 3.2, 52.0, 1.6);
    }
    finish(Tex::Backplate, s, g, false)
}

/// Kjøleribben på avstand: én periode (lamell + mellomrom), gjentatt.
fn make_fin_stripe(fill: f64) -> Image {
    let (s, g) = surface(64, 8);
    hex(&g, "#26292d");
    fill_rect(&g, 0.0, 0.0, 64.0, 8.0);
    let w = (64.0 * fill).round().max(2.0);
    hex(&g, "#dfe3e8");
    fill_rect(&g, 0.0, 0.0, w, 8.0);
    hex(&g, "#9ea4ab");
    fill_rect(&g, w, 0.0, 2.0, 8.0);
    finish(Tex::FinStripe, s, g, true)
}

/// Alle teksturene (kjøres i en egen tråd, ca. 0,1–0,3 s).
pub fn all() -> Vec<Image> {
    vec![make_atlas(), make_pcb_front(), make_pcb_back(), make_bracket(), make_backplate(), make_fin_stripe(FINS.t / FINS.pitch)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlaset_er_vendt_med_v_opp() {
        let a = make_atlas();
        assert_eq!((a.w, a.h), (1024, 1024));
        // Regionen «white» står øverst til venstre i tegningen (v nær 1).
        // Etter vendingen er det de siste radene i dataene, som i WebGL med UNPACK_FLIP_Y.
        let uv = atlas_uv("white", 2.0);
        let x = (uv[0] * 1024.0) as usize + 1;
        let y = (uv[1] * 1024.0) as usize + 1;
        let i = (y * 1024 + x) * 4;
        assert_eq!(&a.rgba[i..i + 4], &[255, 255, 255, 255]);
        let p = atlas_point("mem");
        let j = ((p[1] * 1024.0) as usize * 1024 + (p[0] * 1024.0) as usize) * 4;
        // Matt svart (#16171a), med litt struktur.
        let px = &a.rgba[j..j + 4];
        assert!(px.iter().zip([0x16u8, 0x17, 0x1a, 255]).all(|(a, b)| a.abs_diff(b) <= 4), "minnebrikkens kant: {px:?}");
    }

    #[test]
    fn braketten_har_hull() {
        let b = make_bracket();
        assert!(b.rgba.chunks_exact(4).any(|p| p[3] == 0));
        assert!(b.rgba.chunks_exact(4).any(|p| p[3] == 255));
    }
}
