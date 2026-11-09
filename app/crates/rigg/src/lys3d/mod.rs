//! 3D-kortet på Lys (arkitektur.md 6.5): `GtkGLArea` med GLES 3.0 og
//! shaderne fra `web/js/lys3d/`, og det flate kortet som reserve.
//!
//! [`CardView`] er porten av `web/js/lys3d/view.js`: kameraet, lagene (ta
//! av, skill lagene), viftene, peking og input. Den tegner bare når noe
//! endrer seg (input, lag eller kamera som glir, effekter eller vifter som
//! går), og løpende animasjoner høyst ca. 60 ganger i sekundet. I ro tegnes
//! ingenting. Geometrien og teksturene lages i en egen tråd første gang
//! Lys vises, og GL-ressursene frigjøres når GLArea-en tas bort.
//!
//! **Eies av 2C (Styring og Lys).**

pub mod flat;
pub mod layout;
pub mod mesh;
pub mod model;
pub mod renderer;
pub mod textures;

use crate::motion::Level;
use glam::{Mat3, Mat4, Vec3, Vec4};
use gtk::prelude::*;
use gtk::{gdk, glib};
use model::{GROUPS, LAYERS, Mat};
use renderer::{Frame, Leds, Lights, PartDraw, Renderer};
use std::cell::{Cell, RefCell};
use std::f32::consts::PI;
use std::rc::Rc;

const DEG: f32 = PI / 180.0;
const FOV: f32 = 26.0 * DEG;

/// De fire vinklene: id, navn, yaw og pitch (grader).
pub const VIEWS: [(&str, &str, f32, f32); 4] = [("forfra", "Forfra", 0.0, 4.0), ("skraatt", "Skrått", -33.0, 18.0), ("ovenfra", "Ovenfra", -8.0, 62.0), ("bakfra", "Bakfra", 155.0, 14.0)];
pub const STEPS: [&str; 4] = ["Hel", "Uten deksel", "Uten kjøler", "Bare kretskort"];
const STEP_TEXT: [&str; 4] = [
    "Alle deler er på: deksel med vifter og lys, kjøler, kretskort, brakett og bakplate.",
    "Dekselet og viftene er tatt av, så kjøleribben og varmerørene synes.",
    "Kjøleren er også tatt av. Kretskortet synes med brakett og bakplate.",
    "Bare kretskortet: GPU-brikken, åtte minnebrikker, strømfaser, kontakter og utganger.",
];

fn clamp(v: f32, a: f32, b: f32) -> f32 {
    v.clamp(a, b)
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}
fn smooth_to(cur: f32, target: f32, dt: f32, tau: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-dt / tau).exp())
}
/// Vinkel til −180…180.
pub fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}
fn srgb_lin(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cam {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    dist: f32,
    target: Vec3,
}

#[derive(Clone, Copy, Default)]
struct LayerState {
    out: f32,
    goal: f32,
    delay: f32,
}

struct PartMeta {
    group: usize,
    layer: usize,
    mat: Mat,
    rotor: i32,
    lod: Option<&'static str>,
    cast: bool,
}

/// Det visningen husker mellom økter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Saved {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub step: usize,
    pub explode: f32,
}

impl Default for Saved {
    fn default() -> Self {
        Saved { yaw: VIEWS[1].2, pitch: VIEWS[1].3, zoom: 1.0, step: 0, explode: 0.0 }
    }
}

/// Kameraet, lagene, viftene og lysene (uten GL).
struct View {
    cam: Cam,
    goal: Cam,
    vel: f32,
    step: usize,
    explode: f32,
    explode_goal: f32,
    fan_pct: f32,
    fan_rate: f32,
    fan_angle: [f32; 2],
    led_lin: [f32; 168],
    layer_k: f32,
    dim_zone: usize,
    ambient_on: bool,
    ambient: ([f32; 3], f32),
    hover: Option<usize>,
    focus: Option<usize>,
    layers: [LayerState; 7],
    layer_bounds: [(Vec3, Vec3); 7],
    parts: Vec<PartMeta>,
    led_pos: Vec<[f32; 3]>,
    bar_a: [f32; 3],
    bar_b: [f32; 3],
    fan_light: [[f32; 3]; 2],
    fan_centers: [[f32; 2]; 2],
    center: Vec3,
    dirty: bool,
    shadow_dirty: bool,
    fit_soon: bool,
    fins_near: bool,
    last_now: f64,
    last_draw: f64,
    drag: bool,
    aspect: f32,
    accent: [f32; 3],
    bg: [f32; 3],
    motion: Level,
    // Vern mot treg grafikk (som demoen): stillbilde for løpende animasjoner.
    slow: bool,
    slow_ema: f64,
    slow_n: u32,
    adapt_after: f64,
}

impl View {
    fn new(card: &model::Card) -> View {
        let mut layer_bounds = [(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)); 7];
        let mut parts = Vec::new();
        for p in &card.parts {
            let layer = model::layer_index(p.layer);
            let mut min = Vec3::from(p.data.min);
            let mut max = Vec3::from(p.data.max);
            if p.rotor >= 0 {
                let c = card.fan_centers[p.rotor as usize];
                min += Vec3::new(c[0], c[1], 0.0);
                max += Vec3::new(c[0], c[1], 0.0);
            }
            let lb = &mut layer_bounds[layer];
            lb.0 = lb.0.min(min);
            lb.1 = lb.1.max(max);
            parts.push(PartMeta { group: model::group_index(p.group), layer, mat: p.mat, rotor: p.rotor, lod: p.lod, cast: p.cast });
        }
        let cam = Cam { yaw: VIEWS[1].2, pitch: VIEWS[1].3, zoom: 1.0, dist: 700.0, target: Vec3::ZERO };
        View {
            cam,
            goal: cam,
            vel: 0.0,
            step: 0,
            explode: 0.0,
            explode_goal: 0.0,
            fan_pct: 35.0,
            fan_rate: 0.0,
            fan_angle: [0.3, 1.9],
            led_lin: [0.0; 168],
            layer_k: 1.0,
            dim_zone: 0,
            ambient_on: false,
            ambient: ([0.0; 3], 0.0),
            hover: None,
            focus: None,
            layers: [LayerState::default(); 7],
            layer_bounds,
            parts,
            led_pos: card.led_pos.clone(),
            bar_a: card.bar_a,
            bar_b: card.bar_b,
            fan_light: card.fan_light,
            fan_centers: card.fan_centers,
            center: Vec3::from(card.center),
            dirty: true,
            shadow_dirty: true,
            fit_soon: true,
            fins_near: false,
            last_now: 0.0,
            last_draw: 0.0,
            drag: false,
            aspect: 1.6,
            accent: [0.09, 0.33, 0.93],
            bg: [0x16 as f32 / 255.0, 0x18 as f32 / 255.0, 0x1b as f32 / 255.0],
            motion: Level::Full,
            slow: false,
            slow_ema: 0.0,
            slow_n: 0,
            adapt_after: f64::INFINITY,
        }
    }

    fn eff_motion(&self) -> Level {
        if self.motion == Level::Full && self.slow { Level::Reduced } else { self.motion }
    }

    fn set_step(&mut self, n: usize) {
        let n = n.min(3);
        if n == self.step {
            return;
        }
        let up = n > self.step;
        self.step = n;
        let m = self.motion;
        let changing: Vec<usize> = (0..7).filter(|i| (self.layers[*i].goal == 1.0) != (n >= LAYERS[*i].step)).collect();
        let mut order: Vec<usize> = changing.iter().map(|i| LAYERS[*i].step).collect();
        order.sort();
        order.dedup();
        if !up {
            order.reverse();
        }
        for i in changing {
            let s = &mut self.layers[i];
            s.goal = if n >= LAYERS[i].step { 1.0 } else { 0.0 };
            s.delay = if m == Level::Full { order.iter().position(|x| *x == LAYERS[i].step).unwrap_or(0) as f32 * 0.14 } else { 0.0 };
            if m == Level::Off {
                s.out = s.goal;
            }
        }
        self.fit_soon = true;
        self.dirty = true;
    }

    fn set_explode(&mut self, e: f32, follow: bool) {
        self.explode_goal = clamp(e, 0.0, 1.0);
        if follow || self.motion != Level::Full {
            self.explode = self.explode_goal;
        }
        self.shadow_dirty = true;
        self.fit_soon = true;
        self.dirty = true;
    }

    fn jump_if_still(&mut self) {
        if self.motion != Level::Full {
            self.cam = self.goal;
            self.shadow_dirty = true;
        }
    }

    fn set_view(&mut self, name: &str) {
        let Some(v) = VIEWS.iter().find(|v| v.0 == name) else { return };
        self.goal.yaw = self.cam.yaw + wrap(v.2 - self.cam.yaw);
        self.goal.pitch = v.3;
        self.goal.zoom = 1.0;
        self.vel = 0.0;
        self.fit(false, Some((self.goal.yaw, v.3)));
        self.jump_if_still();
        self.dirty = true;
    }

    fn rotate_by(&mut self, dy: f32, dp: f32) {
        self.goal.yaw += dy;
        self.goal.pitch = clamp(self.goal.pitch + dp, -60.0, 85.0);
        self.vel = 0.0;
        self.jump_if_still();
        self.dirty = true;
    }

    fn zoom_by(&mut self, f: f32) {
        self.goal.zoom = clamp(self.goal.zoom * f, 0.6, 3.2);
        self.jump_if_still();
        self.dirty = true;
    }

    fn layer_offset(&self, i: usize) -> Vec3 {
        let l = &LAYERS[i];
        let out = self.layers[i].out;
        let spread = l.spread.abs() * self.explode;
        let lift = if self.motion == Level::Full { ease_in_out(clamp(out, 0.0, 1.0)) } else { 0.0 };
        let d = spread + lift * 150.0;
        Vec3::new(l.dir[0] * d, l.dir[1] * d + lift * 40.0, l.dir[2] * d)
    }

    fn layer_alpha(&self, i: usize) -> f32 {
        let out = self.layers[i].out;
        if self.motion == Level::Full { 1.0 - smoothstep(0.3, 0.95, out) } else { 1.0 - out }
    }

    /// Rammen rundt lagene som er på, relativt til kortets sentrum.
    fn visible_bounds(&self, goal: bool) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for (i, l) in LAYERS.iter().enumerate() {
            let s = &self.layers[i];
            if (if goal { s.goal } else { s.out }) >= 0.5 {
                continue;
            }
            let (bmin, bmax) = self.layer_bounds[i];
            if !bmin.is_finite() {
                continue;
            }
            let spread = l.spread.abs() * if goal { self.explode_goal } else { self.explode };
            let off = Vec3::from(l.dir) * spread;
            min = min.min(bmin + off - self.center);
            max = max.max(bmax + off - self.center);
        }
        if !min.is_finite() {
            return (Vec3::splat(-10.0), Vec3::splat(10.0));
        }
        (min, max)
    }

    /// Avstand og sentrum som får de synlige lagene til å fylle visningen.
    fn fit(&mut self, jump: bool, at: Option<(f32, f32)>) {
        let (min, max) = self.visible_bounds(true);
        let target = (min + max) / 2.0;
        let (yaw, pitch) = at.unwrap_or((self.goal.yaw, self.goal.pitch));
        let (yaw, pitch) = (yaw * DEG, pitch * DEG);
        let ty = (FOV / 2.0).tan();
        let tx = ty * self.aspect;
        let fwd = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let up = Vec3::new(-pitch.sin() * yaw.sin(), pitch.cos(), -pitch.sin() * yaw.cos());
        let margin = 0.86;
        let mut dist = 0.0f32;
        for i in 0..8 {
            let p = Vec3::new(if i & 1 != 0 { max.x } else { min.x }, if i & 2 != 0 { max.y } else { min.y }, if i & 4 != 0 { max.z } else { min.z });
            let d = p - target;
            let x = d.dot(right);
            let y = d.dot(up);
            let z = d.dot(fwd);
            dist = dist.max(x.abs() / (tx * margin) + z).max(y.abs() / (ty * margin) + z);
        }
        self.goal.dist = dist;
        self.goal.target = target;
        if jump || self.motion != Level::Full {
            self.cam.dist = dist;
            self.cam.target = target;
        }
    }

    /// Ett steg i løkka. Gir (mer å gjøre, tegn nå).
    fn advance(&mut self, now: f64) -> (bool, bool) {
        let interval = if self.last_now > 0.0 { (now - self.last_now) * 1000.0 } else { 16.0 };
        let dt = if self.last_now > 0.0 { (interval / 1000.0).min(0.1) as f32 } else { 1.0 / 60.0 };
        let dt_real = if self.last_now > 0.0 { (interval / 1000.0).min(0.5) as f32 } else { 1.0 / 60.0 };
        self.last_now = now;
        let m = self.eff_motion();
        let mut busy = false;
        // Lag
        let mut layers_moving = false;
        for s in self.layers.iter_mut() {
            if s.out == s.goal {
                continue;
            }
            if s.delay > 0.0 {
                s.delay -= dt_real;
                layers_moving = true;
                continue;
            }
            let dur = match m {
                Level::Full => 0.62,
                Level::Reduced => 0.12,
                Level::Off => 0.0,
            };
            s.out = if dur > 0.0 {
                if s.goal > s.out { (s.out + dt_real / dur).min(s.goal) } else { (s.out - dt_real / dur).max(s.goal) }
            } else {
                s.goal
            };
            layers_moving = true;
        }
        if (self.explode - self.explode_goal).abs() > 1e-4 {
            self.explode = if m == Level::Full { smooth_to(self.explode, self.explode_goal, dt_real, 0.12) } else { self.explode_goal };
            layers_moving = true;
        } else {
            self.explode = self.explode_goal;
        }
        if self.fit_soon {
            self.fit_soon = false;
            self.fit(false, None);
        }
        if layers_moving {
            busy = true;
            self.shadow_dirty = true;
        }
        // Kamera: treghet etter dra, og glidning mot målet.
        if self.vel != 0.0 && m == Level::Full && !self.drag {
            self.goal.yaw += self.vel * dt;
            self.cam.yaw = self.goal.yaw;
            self.vel *= (-dt / 0.28).exp();
            if self.vel.abs() < 2.0 {
                self.vel = 0.0;
            }
            self.shadow_dirty = true;
            busy = true;
        }
        let (c, g) = (self.cam, self.goal);
        let cam_moving = (c.yaw - g.yaw).abs() > 0.01 || (c.pitch - g.pitch).abs() > 0.01 || (c.zoom - g.zoom).abs() > 1e-4 || (c.dist - g.dist).abs() > 0.05 || (c.target - g.target).abs().max_element() > 0.02;
        if cam_moving {
            if m == Level::Full && !self.drag {
                let before = self.cam.yaw;
                self.cam.yaw = smooth_to(c.yaw, g.yaw, dt_real, 0.11);
                self.cam.pitch = smooth_to(c.pitch, g.pitch, dt_real, 0.11);
                self.cam.zoom = smooth_to(c.zoom, g.zoom, dt_real, 0.09);
                self.cam.dist = smooth_to(c.dist, g.dist, dt_real, 0.16);
                self.cam.target = Vec3::new(smooth_to(c.target.x, g.target.x, dt_real, 0.16), smooth_to(c.target.y, g.target.y, dt_real, 0.16), smooth_to(c.target.z, g.target.z, dt_real, 0.16));
                if (self.cam.yaw - before).abs() > 1e-3 {
                    self.shadow_dirty = true;
                }
            } else {
                if c.yaw != g.yaw {
                    self.shadow_dirty = true;
                }
                self.cam = g;
            }
            busy = true;
        }
        // Vifter: farten glir, og de står stille ved redusert eller ingen bevegelse.
        let fans_shown = self.layers[1].out < 1.0;
        let target_rate = if m == Level::Full && self.fan_pct > 0.0 { (60.0 + self.fan_pct / 100.0 * 520.0) / 360.0 } else { 0.0 };
        self.fan_rate = if m == Level::Full { smooth_to(self.fan_rate, target_rate, dt, 0.7) } else { 0.0 };
        if (self.fan_rate - target_rate).abs() < if target_rate > 0.0 { 0.002 } else { 0.03 } {
            self.fan_rate = target_rate;
        }
        if self.fan_rate > 0.0 && fans_shown {
            for i in 0..2 {
                self.fan_angle[i] -= self.fan_rate * PI * 2.0 * dt * if i == 1 { 0.985 } else { 1.0 };
            }
            busy = true;
        }
        // Tegn: alltid ved endring fra input, ellers høyst ~60 bilder i sekundet.
        // 14 ms: tåler at frame clock på 60 Hz går litt ujevnt, og gir 60 bilder/s også på 240 Hz (hvert 4. tikk).
        let due = (now - self.last_draw) * 1000.0 >= 14.0;
        let draw = self.dirty || (busy && due);
        if draw {
            self.last_draw = now;
        }
        self.watch_speed(now, interval, busy && !self.drag && !layers_moving && !cam_moving);
        if !(busy || self.dirty) {
            self.last_now = 0.0;
        }
        (busy, draw)
    }

    fn watch_speed(&mut self, now: f64, interval: f64, steady: bool) {
        if self.slow || !steady || now < self.adapt_after {
            self.slow_n = 0;
            self.slow_ema = 0.0;
            return;
        }
        self.slow_ema = if self.slow_ema > 0.0 { self.slow_ema + (interval - self.slow_ema) * 0.25 } else { interval };
        // Tolv bilder på rad med snitt over 50 ms: ett enkelt hikk (en stor
        // jobb på hovedtråden) skal ikke sende kortet til 2D. Med cairo går
        // løkka med ca. 33 ms (30 bilder/s), så marginen er mindre (runde 3C).
        if self.slow_ema >= 50.0 {
            self.slow_n += 1;
        } else {
            self.slow_n = 0;
        }
        if self.slow_n < 12 {
            return;
        }
        self.slow = true;
    }

    fn frame_data(&mut self, w: i32, h: i32) -> Frame {
        self.aspect = w as f32 / h.max(1) as f32;
        let c = self.cam;
        let yaw = c.yaw * DEG;
        let pitch = c.pitch * DEG;
        let dist = c.dist / c.zoom;
        let t = c.target;
        let eye = t + Vec3::new(dist * pitch.cos() * yaw.sin(), dist * pitch.sin(), dist * pitch.cos() * yaw.cos());
        let view = Mat4::look_at_rh(eye, t, Vec3::Y);
        let r = 190.0;
        let proj = Mat4::perspective_rh_gl(FOV, self.aspect, (dist - r * 1.6).max(4.0), dist + r * 2.4);
        let view_proj = proj * view;
        let rot = |v: Vec3| -> [f32; 3] {
            let v = v.normalize();
            let (s, cs) = yaw.sin_cos();
            [v.x * cs + v.z * s, v.y, -v.x * s + v.z * cs]
        };
        let lights = Lights {
            key: rot(Vec3::new(-0.45, 0.78, 0.68)),
            key_col: [2.0, 1.96, 1.88],
            fill: rot(Vec3::new(0.8, 0.22, 0.55)),
            fill_col: [0.5, 0.52, 0.57],
            rim: rot(Vec3::new(0.2, 0.55, -0.85)),
            rim_col: [0.75, 0.8, 0.9],
            sky: [0.1, 0.105, 0.12],
            ground: [0.028, 0.029, 0.033],
            exposure: 1.0,
            env: 2.0,
        };
        let offsets: Vec<Vec3> = (0..7).map(|i| self.layer_offset(i)).collect();
        let alphas: Vec<f32> = (0..7).map(|i| self.layer_alpha(i)).collect();
        let hl = self.hover.or(self.focus);
        // Nivådetaljer for lamellene: detaljert først når de ikke gir moaré (≈ 9 px per periode).
        let px_per_mm = h as f32 / (2.0 * dist * (FOV / 2.0).tan());
        let pitch_px = layout::FINS.pitch as f32 * px_per_mm;
        let near = if self.fins_near { pitch_px > 8.5 } else { pitch_px > 9.5 };
        if near != self.fins_near {
            self.fins_near = near;
            self.shadow_dirty = true;
        }
        let mut parts = Vec::with_capacity(self.parts.len());
        for (i, p) in self.parts.iter().enumerate() {
            let off = offsets[p.layer];
            let base = Mat4::from_translation(off - self.center);
            let model = if p.rotor >= 0 {
                let fc = self.fan_centers[p.rotor as usize];
                base * Mat4::from_translation(Vec3::new(fc[0], fc[1], 0.0)) * Mat4::from_rotation_z(self.fan_angle[p.rotor as usize])
            } else {
                base
            };
            let alpha = alphas[p.layer];
            let lb = self.layer_bounds[p.layer];
            let cz = (lb.0 + lb.1) / 2.0 + off - self.center;
            parts.push(PartDraw {
                mesh: i,
                mat: p.mat,
                model,
                nmat: Mat3::from_mat4(model).inverse().transpose(),
                alpha,
                visible: alpha > 0.004 && p.lod.is_none_or(|l| (l == "near") == near),
                highlight: if hl == Some(p.group) { 1.0 } else { 0.0 },
                depth: cz.distance(eye),
                pick_id: p.group as u32 + 1,
                cast: p.cast,
            });
        }
        // LED-ene: farge (lineær), sonedemping, pust og lagets synlighet.
        let d_a = alphas[0];
        let d_off = offsets[0];
        let zone_gain = [0, 1, 2].map(|z| (if self.dim_zone == 0 || self.dim_zone == z + 1 { 1.0 } else { 0.3 }) * self.layer_k);
        let to_world = |p: [f32; 3]| -> [f32; 3] { (Vec3::from(p) + d_off - self.center).to_array() };
        let avg = |a: usize, b: usize, gain: f32| -> [f32; 3] {
            let mut s = [0.0f32; 3];
            for i in a..b {
                for (k, sk) in s.iter_mut().enumerate() {
                    *sk += self.led_lin[i * 3 + k];
                }
            }
            s.map(|v| v / (b - a) as f32 * gain)
        };
        let leds = Leds {
            colors: self.led_lin,
            zone_gain,
            bar_a: to_world(self.bar_a),
            bar_b: to_world(self.bar_b),
            bar_col: avg(0, 20, zone_gain[0] * d_a * 3.2),
            f1_pos: to_world(self.fan_light[0]),
            f1_col: avg(20, 38, zone_gain[1] * d_a * 1.8),
            f2_pos: to_world(self.fan_light[1]),
            f2_col: avg(38, 56, zone_gain[2] * d_a * 1.8),
        };
        let mut glow = [0.0f32; 56 * 6];
        for i in 0..56 {
            let w3 = to_world(self.led_pos[i]);
            let gz = zone_gain[rigg_led_zone(i)] * d_a;
            glow[i * 6..i * 6 + 3].copy_from_slice(&w3);
            for k in 0..3 {
                glow[i * 6 + 3 + k] = self.led_lin[i * 3 + k] * gz;
            }
        }
        // Scenens kule (skyggekartet) og gulvet under kortet.
        let (vmin, vmax) = self.visible_bounds(false);
        let sc = (vmin + vmax) / 2.0;
        let sr = ((vmax - vmin).length() / 2.0).max(60.0) * 1.02;
        let floor_y = -self.center.y - 26.0;
        let floor_alpha = smoothstep(4.0, 70.0, eye.y - floor_y);
        let cp = view_proj * Vec4::new(t.x, t.y, t.z, 1.0);
        let cpw = if cp.w != 0.0 { cp.w } else { 1.0 };
        let ambient = if self.ambient_on && self.ambient.1 > 0.0 { Some(([cp.x / cpw * 0.5 + 0.5, cp.y / cpw * 0.5 + 0.5], self.ambient.0, self.ambient.1 * 0.42)) } else { None };
        Frame {
            parts,
            view,
            proj,
            view_proj,
            cam_pos: eye.to_array(),
            lights,
            leds,
            accent: self.accent,
            glow,
            glow_count: if d_a > 0.01 { 56 } else { 0 },
            floor_center: [sc.x, floor_y, sc.z],
            floor_alpha,
            ambient,
            bg: self.bg,
            scene_center: sc.to_array(),
            scene_radius: sr,
            shadow_dirty: self.shadow_dirty,
        }
    }

    fn describe(&self) -> String {
        let y = wrap(self.goal.yaw);
        let p = self.goal.pitch;
        let side = if y < 0.0 { "fra venstre" } else { "fra høyre" };
        let ay = y.abs();
        let angle = if p > 50.0 {
            "ovenfra, med lyslisten mot deg".to_string()
        } else if p < -25.0 {
            "nedenfra".to_string()
        } else if ay < 12.0 {
            "rett forfra".to_string()
        } else if ay < 70.0 {
            format!("skrått forfra {side}")
        } else if ay < 110.0 {
            if y < 0.0 { "fra siden med braketten og utgangene".to_string() } else { "fra enden".to_string() }
        } else if ay < 168.0 {
            format!("skrått bakfra {side}")
        } else {
            "rett bakfra, mot bakplaten".to_string()
        };
        let ex = if self.explode_goal > 0.05 { format!(" Lagene er skilt fra hverandre ({} %).", (self.explode_goal * 100.0).round()) } else { String::new() };
        let fans = if self.layers[1].goal >= 0.5 {
            String::new()
        } else if self.fan_pct > 0.0 {
            format!(" Viftene går på {} %.", self.fan_pct.round())
        } else {
            " Viftene står stille.".into()
        };
        format!("Skjermkort i 3D, sett {angle}. {}{ex}{fans}", STEP_TEXT[self.step])
    }

    fn visible_groups(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for (li, _) in LAYERS.iter().enumerate() {
            if self.layers[li].goal >= 0.5 {
                continue;
            }
            for p in self.parts.iter().filter(|p| p.layer == li) {
                if !out.contains(&p.group) {
                    out.push(p.group);
                }
            }
        }
        out
    }

    fn current_view(&self) -> Option<&'static str> {
        VIEWS.iter().find(|v| wrap(self.goal.yaw - v.2).abs() < 0.5 && (self.goal.pitch - v.3).abs() < 0.5).map(|v| v.0)
    }
}

fn rigg_led_zone(i: usize) -> usize {
    crate::rgb::led_zone(i)
}

// ================================================================ GLArea

/// En del under pekeren: gruppe, navn, beskrivelse, sone.
#[derive(Clone, Debug)]
pub struct PartInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub zone: Option<&'static str>,
}

pub fn group_info(i: usize) -> PartInfo {
    let g = GROUPS[i.min(GROUPS.len() - 1)];
    PartInfo { id: g.0, name: g.1, desc: g.2, zone: g.3 }
}

type Built = (model::Card, Vec<textures::Image>);

/// Hvor lang tid ting tok (til målingene i maalinger.md).
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub build_ms: f64,
    pub upload_ms: f64,
    pub first_frame_ms: f64,
    pub frames: u64,
    pub draw_ms_sum: f64,
    pub draw_ms_max: f64,
}

pub struct CardView {
    pub area: gtk::GLArea,
    view: RefCell<Option<View>>,
    gpu: RefCell<Option<Renderer>>,
    built: RefCell<Option<Built>>,
    /// Teksturene bygges på nytt fordi CPU-kopiene er sluppet (se `render`).
    rebuilding: Cell<bool>,
    last: RefCell<Option<Frame>>,
    size: Cell<(i32, i32)>,
    failed: Cell<bool>,
    running: Cell<bool>,
    started: std::time::Instant,
    pub timing: Cell<Timing>,
    pending_pick: Cell<Option<(f32, f32)>>,
    pick_idle: Cell<bool>,
    pub on_hover: crate::widgets::Slot<dyn Fn(Option<PartInfo>, f32, f32)>,
    pub on_zone: crate::widgets::Slot<dyn Fn(&'static str)>,
    pub on_part: crate::widgets::Slot<dyn Fn(PartInfo)>,
    pub on_change: RefCell<Option<Box<dyn Fn()>>>,
    pub on_wake: RefCell<Option<Box<dyn Fn()>>>,
    pub on_fail: crate::widgets::Slot<dyn Fn(String)>,
    pub on_ready: RefCell<Option<Box<dyn Fn()>>>,
    pending_saved: Cell<Option<Saved>>,
    drag_last: Cell<(f64, f64)>,
    drag_moved: Cell<bool>,
    drag_vel: Cell<(f32, f64)>,
}

impl CardView {
    pub fn new() -> Rc<CardView> {
        let area = gtk::GLArea::new();
        area.set_allowed_apis(gdk::GLAPI::GLES);
        area.set_required_version(3, 0);
        area.set_has_depth_buffer(false);
        area.set_has_stencil_buffer(false);
        area.set_auto_render(false);
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_focusable(true);
        area.set_focus_on_click(true);
        area.add_css_class("rgb-view");
        area.set_accessible_role(gtk::AccessibleRole::Application);
        area.update_property(&[gtk::accessible::Property::Label("Skjermkortet i 3D"), gtk::accessible::Property::RoleDescription("3D-visning")]);
        let v = Rc::new(CardView {
            area,
            view: RefCell::new(None),
            gpu: RefCell::new(None),
            built: RefCell::new(None),
            rebuilding: Cell::new(false),
            last: RefCell::new(None),
            size: Cell::new((1, 1)),
            failed: Cell::new(false),
            running: Cell::new(false),
            started: std::time::Instant::now(),
            timing: Cell::new(Timing::default()),
            pending_pick: Cell::new(None),
            pick_idle: Cell::new(false),
            on_hover: RefCell::new(None),
            on_zone: RefCell::new(None),
            on_part: RefCell::new(None),
            on_change: RefCell::new(None),
            on_wake: RefCell::new(None),
            on_fail: RefCell::new(None),
            on_ready: RefCell::new(None),
            pending_saved: Cell::new(None),
            drag_last: Cell::new((0.0, 0.0)),
            drag_moved: Cell::new(false),
            drag_vel: Cell::new((0.0, 0.0)),
        });
        v.wire();
        v.start_build();
        v
    }

    /// Geometrien og teksturene lages i en egen tråd (ingenting fryser).
    fn start_build(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        let t0 = std::time::Instant::now();
        let handle = gtk::gio::spawn_blocking(move || (model::build(), textures::all()));
        glib::spawn_future_local(async move {
            let Ok(built) = handle.await else { return };
            let Some(v) = w.upgrade() else { return };
            let mut t = v.timing.get();
            t.build_ms = t0.elapsed().as_secs_f64() * 1000.0;
            v.timing.set(t);
            if !v.rebuilding.replace(false) || v.view.borrow().is_none() {
                let mut view = View::new(&built.0);
                if let Some(s) = v.pending_saved.take() {
                    Self::apply_saved(&mut view, s);
                }
                *v.view.borrow_mut() = Some(view);
            }
            *v.built.borrow_mut() = Some(built);
            v.area.queue_render();
            v.change();
            v.wake();
        });
    }

    fn apply_saved(view: &mut View, s: Saved) {
        view.goal.yaw = s.yaw;
        view.goal.pitch = clamp(s.pitch, -60.0, 85.0);
        view.goal.zoom = clamp(s.zoom, 0.6, 3.2);
        view.cam = view.goal;
        view.step = s.step.min(3);
        for (i, l) in LAYERS.iter().enumerate() {
            let on = view.step >= l.step;
            view.layers[i] = LayerState { out: if on { 1.0 } else { 0.0 }, goal: if on { 1.0 } else { 0.0 }, delay: 0.0 };
        }
        view.explode = clamp(s.explode, 0.0, 1.0);
        view.explode_goal = view.explode;
        view.fit(true, None);
        view.shadow_dirty = true;
    }

    fn wire(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.area.connect_realize(move |a| {
            let Some(v) = w.upgrade() else { return };
            a.make_current();
            if let Some(e) = a.error() {
                v.fail(format!("GL kunne ikke startes: {e}"));
            }
        });
        let w = Rc::downgrade(self);
        self.area.connect_unrealize(move |a| {
            let Some(v) = w.upgrade() else { return };
            a.make_current();
            if a.error().is_none()
                && let Some(r) = v.gpu.borrow_mut().take()
            {
                r.destroy();
            }
            v.gpu.borrow_mut().take();
            v.last.borrow_mut().take();
            // Neste gang: last opp på nytt fra geometrien som er bygd.
        });
        let w = Rc::downgrade(self);
        self.area.connect_render(move |a, _ctx| {
            if let Some(v) = w.upgrade() {
                v.render(a);
            }
            glib::Propagation::Stop
        });
        self.bind_input();
    }

    fn fail(&self, msg: String) {
        if self.failed.replace(true) {
            return;
        }
        eprintln!("Lys: {msg}");
        if let Some(f) = self.on_fail.borrow().as_ref() {
            f(msg);
        }
    }

    pub fn ready(&self) -> bool {
        self.gpu.borrow().is_some() && self.view.borrow().is_some()
    }

    fn upload(&self) -> Result<(), String> {
        let t0 = std::time::Instant::now();
        let built = self.built.borrow();
        let Some((card, images)) = built.as_ref() else { return Err("ikke bygd".into()) };
        let mut r = Renderer::new(renderer::context())?;
        for p in &card.parts {
            r.add_mesh(&p.data)?;
        }
        for img in images {
            r.add_texture(img)?;
        }
        *self.gpu.borrow_mut() = Some(r);
        let mut t = self.timing.get();
        t.upload_ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.timing.set(t);
        Ok(())
    }

    fn render(self: &Rc<Self>, a: &gtk::GLArea) {
        if self.failed.get() {
            return;
        }
        if a.error().is_some() {
            self.fail("GL-feil i GLArea".into());
            return;
        }
        let scale = a.scale_factor();
        let (w, h) = ((a.width() * scale).max(1), (a.height() * scale).max(1));
        self.size.set((w, h));
        if self.view.borrow().is_none() {
            return;
        }
        if self.gpu.borrow().is_none() {
            // Teksturene er sluppet etter forrige opplasting: bygg dem på nytt
            // (0,2 s i en egen tråd), og behold kameraet.
            if self.built.borrow().as_ref().is_some_and(|b| b.1.is_empty()) {
                if !self.rebuilding.replace(true) {
                    self.start_build();
                }
                return;
            }
            if let Err(e) = self.upload() {
                self.fail(format!("3D-kortet kunne ikke startes: {e}"));
                return;
            }
            // GPU-en har teksturene nå. CPU-kopiene (ca. 22 MB) og det
            // byggetråden lot ligge igjen i malloc, gis tilbake (runde 3C).
            if let Some(b) = self.built.borrow_mut().as_mut() {
                b.1 = Vec::new();
            }
            crate::widgets::trim_memory();
            if let Some(f) = self.on_ready.borrow().as_ref() {
                f();
            }
        }
        let t0 = std::time::Instant::now();
        let frame = {
            let mut vb = self.view.borrow_mut();
            let Some(v) = vb.as_mut() else { return };
            if v.adapt_after.is_infinite() {
                v.adapt_after = self.now() + 2.5;
            }
            let aspect_before = v.aspect;
            v.aspect = w as f32 / h as f32;
            if (aspect_before - v.aspect).abs() > 1e-3 {
                v.fit(true, None);
            }
            let f = v.frame_data(w, h);
            v.shadow_dirty = false;
            v.dirty = false;
            f
        };
        if let Some(r) = self.gpu.borrow_mut().as_mut() {
            r.render(&frame, w, h);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let mut t = self.timing.get();
        if t.frames == 0 {
            t.first_frame_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        }
        t.frames += 1;
        t.draw_ms_sum += ms;
        t.draw_ms_max = t.draw_ms_max.max(ms);
        self.timing.set(t);
        *self.last.borrow_mut() = Some(frame);
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Ett steg i løkka (fra sidens tick). Gir `true` så lenge det er mer å gjøre.
    pub fn advance(&self) -> bool {
        if !self.running.get() || self.failed.get() {
            return false;
        }
        let now = self.now();
        let (busy, draw, dirty) = {
            let mut vb = self.view.borrow_mut();
            let Some(v) = vb.as_mut() else { return false };
            let (busy, draw) = v.advance(now);
            (busy, draw, v.dirty)
        };
        if draw {
            self.area.queue_render();
        }
        busy || dirty
    }

    pub fn set_running(&self, on: bool) {
        self.running.set(on);
        if !on && let Some(v) = self.view.borrow_mut().as_mut() {
            v.last_now = 0.0;
        }
        if on {
            self.wake();
        }
    }

    fn wake(&self) {
        if let Some(f) = self.on_wake.borrow().as_ref() {
            f();
        }
    }

    fn change(&self) {
        self.wake();
        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
    }

    fn with(&self, f: impl FnOnce(&mut View)) {
        if let Some(v) = self.view.borrow_mut().as_mut() {
            f(v);
        }
    }

    // ---- Styring fra siden ----

    /// Fargene fra motoren (sRGB 0–255), pusten, sonen og bakgrunnslyset.
    pub fn set_lights(&self, e: &crate::rgb::Engine) {
        let mut changed = false;
        self.with(|v| {
            for i in 0..168 {
                let l = srgb_lin(e.cur[i] / 255.0);
                if l != v.led_lin[i] {
                    v.led_lin[i] = l;
                    changed = true;
                }
            }
            if v.layer_k != e.layer_k || v.dim_zone != e.zone {
                changed = true;
            }
            v.layer_k = e.layer_k;
            v.dim_zone = e.zone;
            let amb = (e.ambient.unwrap_or([0.0; 3]), e.ambient_a);
            if amb != v.ambient {
                changed = changed || v.ambient_on;
                v.ambient = amb;
            }
            if changed {
                v.dirty = true;
            }
        });
        if changed {
            self.wake();
        }
    }

    /// Dreier kameraet litt (målingene: jevn dreining uten treghet).
    pub fn nudge_yaw(&self, deg: f32) {
        self.with(|v| {
            v.cam.yaw += deg;
            v.goal.yaw = v.cam.yaw;
            v.shadow_dirty = true;
            v.dirty = true;
        });
    }

    pub fn set_ambient_enabled(&self, on: bool) {
        self.with(|v| {
            if v.ambient_on != on {
                v.ambient_on = on;
                v.dirty = true;
            }
        });
        self.wake();
    }

    pub fn show_fan_speed(&self, pct: f64) {
        self.with(|v| v.fan_pct = clamp(pct as f32, 0.0, 100.0));
        self.wake();
    }

    pub fn set_motion(&self, m: Level) {
        self.with(|v| {
            v.motion = m;
            v.dirty = true;
        });
        self.wake();
    }

    pub fn slow(&self) -> bool {
        self.view.borrow().as_ref().is_some_and(|v| v.slow)
    }

    pub fn set_colors(&self, bg: gdk::RGBA, accent: gdk::RGBA) {
        self.with(|v| {
            v.bg = [bg.red(), bg.green(), bg.blue()];
            v.accent = [srgb_lin(accent.red()), srgb_lin(accent.green()), srgb_lin(accent.blue())];
            v.dirty = true;
        });
        self.wake();
    }

    pub fn set_step(&self, n: usize) {
        self.with(|v| v.set_step(n));
        self.change();
    }

    pub fn set_explode(&self, e: f64, follow: bool) {
        self.with(|v| v.set_explode(e as f32, follow));
        self.change();
    }

    pub fn set_view(&self, name: &str) {
        self.with(|v| v.set_view(name));
        self.change();
    }

    pub fn reset_view(&self) {
        self.set_view("skraatt");
    }

    pub fn set_focus_group(&self, g: Option<&str>) {
        let gi = g.map(model::group_index);
        self.with(|v| {
            if v.focus != gi {
                v.focus = gi;
                v.dirty = true;
            }
        });
        self.wake();
    }

    pub fn step(&self) -> usize {
        self.view.borrow().as_ref().map_or(0, |v| v.step)
    }

    pub fn explode_goal(&self) -> f64 {
        self.view.borrow().as_ref().map_or(0.0, |v| v.explode_goal as f64)
    }

    pub fn current_view(&self) -> Option<&'static str> {
        self.view.borrow().as_ref().and_then(|v| v.current_view())
    }

    pub fn describe(&self) -> String {
        self.view.borrow().as_ref().map(|v| v.describe()).unwrap_or_else(|| "Bygger 3D-kortet …".into())
    }

    pub fn visible_groups(&self) -> Vec<PartInfo> {
        self.view.borrow().as_ref().map(|v| v.visible_groups().into_iter().map(group_info).collect()).unwrap_or_default()
    }

    /// Det som huskes mellom økter.
    pub fn saved(&self) -> Option<Saved> {
        self.view.borrow().as_ref().map(|v| Saved { yaw: wrap(v.goal.yaw), pitch: v.goal.pitch, zoom: v.goal.zoom, step: v.step, explode: v.explode_goal })
    }

    /// Setter kamera og lag direkte (ved oppstart, uten animasjon).
    pub fn restore(&self, s: Saved) {
        let mut done = false;
        self.with(|v| {
            Self::apply_saved(v, s);
            done = true;
        });
        if !done {
            self.pending_saved.set(Some(s));
        }
        self.change();
    }

    // ---- Input ----

    fn bind_input(self: &Rc<Self>) {
        let a = &self.area;
        // Dra for å vri. Et klikk uten bevegelse velger sonen eller delen.
        let drag = gtk::GestureDrag::new();
        let w = Rc::downgrade(self);
        drag.connect_drag_begin(move |g, _, _| {
            let Some(v) = w.upgrade() else { return };
            v.area.grab_focus();
            v.drag_last.set((0.0, 0.0));
            v.drag_moved.set(false);
            v.drag_vel.set((0.0, v.now()));
            v.with(|s| s.vel = 0.0);
            let _ = g;
        });
        let w = Rc::downgrade(self);
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(v) = w.upgrade() else { return };
            if !v.drag_moved.get() {
                if dx.hypot(dy) < 4.0 {
                    return;
                }
                v.drag_moved.set(true);
                v.with(|s| {
                    s.drag = true;
                    s.hover = None;
                });
                v.area.add_css_class("dragging");
                if let Some(f) = v.on_hover.borrow().as_ref() {
                    f(None, 0.0, 0.0);
                }
            }
            let (lx, ly) = v.drag_last.get();
            let (ddx, ddy) = ((dx - lx) as f32, (dy - ly) as f32);
            v.drag_last.set((dx, dy));
            let k = 0.36;
            let now = v.now();
            let (pv, pt) = v.drag_vel.get();
            let dtm = ((now - pt) * 1000.0).max(1.0) as f32;
            v.drag_vel.set((pv + ((-ddx * k * 1000.0 / dtm) - pv) * 0.5, now));
            v.with(|s| {
                s.cam.yaw -= ddx * k;
                s.goal.yaw = s.cam.yaw;
                s.cam.pitch = clamp(s.cam.pitch + ddy * k * 0.8, -60.0, 85.0);
                s.goal.pitch = s.cam.pitch;
                s.shadow_dirty = true;
                s.dirty = true;
            });
            v.change();
        });
        let w = Rc::downgrade(self);
        drag.connect_drag_end(move |g, _, _| {
            let Some(v) = w.upgrade() else { return };
            v.area.remove_css_class("dragging");
            let moved = v.drag_moved.get();
            let (vel, t) = v.drag_vel.get();
            let full = v.view.borrow().as_ref().is_some_and(|s| s.motion == Level::Full);
            let recent = (v.now() - t) * 1000.0 < 80.0;
            v.with(|s| {
                s.drag = false;
                if moved && full && recent {
                    s.vel = clamp(vel, -400.0, 400.0);
                }
            });
            if !moved && let Some((x, y)) = g.start_point() {
                v.click(x as f32, y as f32);
            }
            v.wake();
        });
        a.add_controller(drag);
        // Dobbeltklikk: tilbake til standardvinkelen.
        let dbl = gtk::GestureClick::new();
        let w = Rc::downgrade(self);
        dbl.connect_pressed(move |_, n, _, _| {
            if n == 2
                && let Some(v) = w.upgrade()
            {
                v.reset_view();
            }
        });
        a.add_controller(dbl);
        // Peking: delen under pekeren (én piksel tegnes, høyst én gang per bilde).
        let motion = gtk::EventControllerMotion::new();
        let w = Rc::downgrade(self);
        motion.connect_motion(move |_, x, y| {
            let Some(v) = w.upgrade() else { return };
            if v.view.borrow().as_ref().is_some_and(|s| s.drag) {
                return;
            }
            v.pending_pick.set(Some((x as f32, y as f32)));
            if !v.pick_idle.replace(true) {
                let w2 = Rc::downgrade(&v);
                glib::idle_add_local_once(move || {
                    if let Some(v) = w2.upgrade() {
                        v.pick_idle.set(false);
                        v.do_hover();
                    }
                });
            }
        });
        let w = Rc::downgrade(self);
        motion.connect_leave(move |_| {
            let Some(v) = w.upgrade() else { return };
            v.pending_pick.set(None);
            let mut changed = false;
            v.with(|s| {
                if s.hover.take().is_some() {
                    s.dirty = true;
                    changed = true;
                }
            });
            if let Some(f) = v.on_hover.borrow().as_ref() {
                f(None, 0.0, 0.0);
            }
            if changed {
                v.wake();
            }
        });
        a.add_controller(motion);
        // Rull for å zoome.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let w = Rc::downgrade(self);
        scroll.connect_scroll(move |_, _, dy| {
            let Some(v) = w.upgrade() else { return glib::Propagation::Proceed };
            v.with(|s| {
                s.goal.zoom = clamp(s.goal.zoom * (-dy as f32 * 0.16).exp(), 0.6, 3.2);
                if s.motion != Level::Full {
                    s.cam.zoom = s.goal.zoom;
                }
                s.dirty = true;
            });
            v.change();
            glib::Propagation::Stop
        });
        a.add_controller(scroll);
        // Taster: piltaster vrir, + og − zoomer, 0 og Home tilbakestiller.
        let keys = gtk::EventControllerKey::new();
        let w = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, k, _, m| {
            let Some(v) = w.upgrade() else { return glib::Propagation::Proceed };
            if m.intersects(gdk::ModifierType::ALT_MASK | gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SUPER_MASK) {
                return glib::Propagation::Proceed;
            }
            let big = m.contains(gdk::ModifierType::SHIFT_MASK);
            let used = match k {
                gdk::Key::Left => {
                    v.with(|s| s.rotate_by(if big { -45.0 } else { -15.0 }, 0.0));
                    true
                }
                gdk::Key::Right => {
                    v.with(|s| s.rotate_by(if big { 45.0 } else { 15.0 }, 0.0));
                    true
                }
                gdk::Key::Up => {
                    v.with(|s| s.rotate_by(0.0, if big { 30.0 } else { 10.0 }));
                    true
                }
                gdk::Key::Down => {
                    v.with(|s| s.rotate_by(0.0, if big { -30.0 } else { -10.0 }));
                    true
                }
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => {
                    v.with(|s| s.zoom_by(1.2));
                    true
                }
                gdk::Key::minus | gdk::Key::underscore | gdk::Key::KP_Subtract => {
                    v.with(|s| s.zoom_by(1.0 / 1.2));
                    true
                }
                gdk::Key::_0 | gdk::Key::KP_0 | gdk::Key::Home => {
                    v.reset_view();
                    true
                }
                _ => false,
            };
            if used {
                v.change();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        a.add_controller(keys);
    }

    /// Delen under et punkt i GLArea-ens koordinater (logiske piksler).
    fn pick_at(&self, x: f32, y: f32) -> Option<usize> {
        let last = self.last.borrow();
        let f = last.as_ref()?;
        let gpu = self.gpu.borrow();
        let r = gpu.as_ref()?;
        self.area.make_current();
        if self.area.error().is_some() {
            return None;
        }
        let (w, h) = self.size.get();
        let s = self.area.scale_factor() as f32;
        let id = r.pick(f, (x * s).round() as i32, (y * s).round() as i32, w, h);
        (id > 0).then(|| id as usize - 1)
    }

    fn do_hover(&self) {
        let Some((x, y)) = self.pending_pick.take() else { return };
        let g = self.pick_at(x, y);
        let mut changed = false;
        self.with(|s| {
            if s.hover != g {
                s.hover = g;
                s.dirty = true;
                changed = true;
            }
        });
        if changed {
            self.wake();
        }
        if let Some(f) = self.on_hover.borrow().as_ref() {
            f(g.map(group_info), x, y);
        }
    }

    fn click(&self, x: f32, y: f32) {
        let Some(g) = self.pick_at(x, y) else { return };
        let info = group_info(g);
        match info.zone {
            Some(z) => {
                if let Some(f) = self.on_zone.borrow().as_ref() {
                    f(z);
                }
            }
            None => {
                if let Some(f) = self.on_part.borrow().as_ref() {
                    f(info);
                }
            }
        }
    }

    /// Måling: `n` bilder etter hverandre, hvert ventet ferdig på GPU-en.
    /// Gir ms per bilde (CPU + GPU). Bare til målingene.
    pub fn bench(&self, n: usize) -> Option<f64> {
        if !self.ready() {
            return None;
        }
        self.area.make_current();
        self.area.attach_buffers();
        let (w, h) = self.size.get();
        let t0 = std::time::Instant::now();
        for _ in 0..n {
            let f = {
                let mut vb = self.view.borrow_mut();
                let v = vb.as_mut()?;
                v.fan_angle[0] -= 0.05;
                v.fan_angle[1] -= 0.05;
                v.frame_data(w, h)
            };
            let mut gpu = self.gpu.borrow_mut();
            let r = gpu.as_mut()?;
            r.render(&f, w, h);
            r.finish();
        }
        Some(t0.elapsed().as_secs_f64() * 1000.0 / n as f64)
    }

    pub fn gl_info(&self) -> String {
        self.gpu.borrow().as_ref().map(|r| r.info.clone()).unwrap_or_default()
    }
}
