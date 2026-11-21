//! Geometri (port av `web/js/lys3d/gl/mesh.js`): en bygger som samler
//! posisjon, normal, uv og en ekstra verdi per hjørne (indeks til en
//! lysdiode, ellers −1), og primitivene skjermkortet bygges av. Alle mål er
//! i millimeter.

use glam::{Mat3, Mat4, Vec3};
use std::f64::consts::{PI, TAU};

/// Et hvitt punkt i etikettatlaset.
pub const WHITE_UV: [f32; 2] = [0.004, 0.004];

#[derive(Default, Clone)]
pub struct MeshData {
    pub pos: Vec<f32>,
    pub nrm: Vec<f32>,
    pub uv: Vec<f32>,
    pub led: Vec<f32>,
    pub idx: Vec<u32>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[cfg(test)]
impl MeshData {
    pub fn verts(&self) -> usize {
        self.pos.len() / 3
    }
}

/// uv-rektangel (u0, v0, u1, v1).
pub type UvRect = [f32; 4];

fn rect_uv(r: UvRect) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// Uv for enkeltflater av en boks. Flatene heter etter normalen.
#[derive(Default, Clone, Copy)]
pub struct BoxUv<'a> {
    pub pz: Option<UvRect>,
    pub skip: &'a [&'a str],
}

pub struct MeshBuilder {
    p: Vec<f32>,
    n: Vec<f32>,
    uv: Vec<f32>,
    led: Vec<f32>,
    idx: Vec<u32>,
    m: Option<Mat4>,
    nm: Option<Mat3>,
    pub led_index: f32,
    pub default_uv: [f32; 2],
}

impl Default for MeshBuilder {
    fn default() -> Self {
        MeshBuilder::new()
    }
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder { p: vec![], n: vec![], uv: vec![], led: vec![], idx: vec![], m: None, nm: None, led_index: -1.0, default_uv: WHITE_UV }
    }

    pub fn count(&self) -> u32 {
        (self.p.len() / 3) as u32
    }

    /// Legger til det `f` bygger med en transformasjon (nøstes).
    pub fn with(&mut self, m: Mat4, f: impl FnOnce(&mut MeshBuilder)) {
        let pm = self.m;
        let pn = self.nm;
        let next = match pm {
            Some(p) => p * m,
            None => m,
        };
        self.m = Some(next);
        self.nm = Some(Mat3::from_mat4(next).inverse().transpose());
        f(self);
        self.m = pm;
        self.nm = pn;
    }

    pub fn vert(&mut self, p: [f64; 3], n: [f64; 3], uv: Option<[f32; 2]>) -> u32 {
        let mut pv = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
        let mut nv = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32);
        if let (Some(m), Some(nm)) = (self.m, self.nm) {
            pv = m.transform_point3(pv);
            nv = nm * nv;
            let l = nv.length();
            if l > 0.0 {
                nv /= l;
            }
        }
        let uv = uv.unwrap_or(self.default_uv);
        self.p.extend_from_slice(&[pv.x, pv.y, pv.z]);
        self.n.extend_from_slice(&[nv.x, nv.y, nv.z]);
        self.uv.extend_from_slice(&uv);
        self.led.push(self.led_index);
        self.count() - 1
    }

    /// Firkant a-b-c-d. Vindingen rettes etter normalen.
    pub fn quad(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3], n: [f64; 3], uv: Option<[[f32; 2]; 4]>) {
        let u = uv.unwrap_or([self.default_uv; 4]);
        let i = self.vert(a, n, Some(u[0]));
        self.vert(b, n, Some(u[1]));
        self.vert(c, n, Some(u[2]));
        self.vert(d, n, Some(u[3]));
        self.add_quad_oriented(i, i + 1, i + 2, i + 3);
    }

    fn pv(&self, i: u32) -> [f32; 3] {
        let k = i as usize * 3;
        [self.p[k], self.p[k + 1], self.p[k + 2]]
    }
    fn nv(&self, i: u32) -> [f32; 3] {
        let k = i as usize * 3;
        [self.n[k], self.n[k + 1], self.n[k + 2]]
    }

    /// Legger til en firkant av eksisterende hjørner og snur den om nødvendig.
    pub fn add_quad_oriented(&mut self, a: u32, b: u32, c: u32, d: u32) {
        let (pa, pb, pc, pd) = (Vec3::from(self.pv(a)), Vec3::from(self.pv(b)), Vec3::from(self.pv(c)), Vec3::from(self.pv(d)));
        let u = pb - pa;
        let v = pd - pa;
        let mut cr = u.cross(v);
        if cr.abs().element_sum() < 1e-12 {
            let w = pc - pa;
            cr = u.cross(w);
            if cr.abs().element_sum() < 1e-12 {
                cr = w.cross(v);
            }
        }
        let nn = Vec3::from(self.nv(a)) + Vec3::from(self.nv(c));
        if cr.dot(nn) >= 0.0 {
            self.idx.extend_from_slice(&[a, b, c, a, c, d]);
        } else {
            self.idx.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }

    /// Boks med sentrum `c` og størrelse `s`.
    pub fn bx(&mut self, c: [f64; 3], s: [f64; 3], uv: BoxUv) -> &mut Self {
        let (hx, hy, hz) = (s[0] / 2.0, s[1] / 2.0, s[2] / 2.0);
        let (x0, x1, y0, y1, z0, z1) = (c[0] - hx, c[0] + hx, c[1] - hy, c[1] + hy, c[2] - hz, c[2] + hz);
        let sk = |k: &str| uv.skip.contains(&k);
        if !sk("pz") {
            self.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0.0, 0.0, 1.0], uv.pz.map(rect_uv));
        }
        if !sk("nz") {
            self.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0.0, 0.0, -1.0], None);
        }
        if !sk("px") {
            self.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1.0, 0.0, 0.0], None);
        }
        if !sk("nx") {
            self.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1.0, 0.0, 0.0], None);
        }
        if !sk("py") {
            self.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0.0, 1.0, 0.0], None);
        }
        if !sk("ny") {
            self.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0.0, -1.0, 0.0], None);
        }
        self
    }

    pub fn boxs(&mut self, c: [f64; 3], s: [f64; 3]) -> &mut Self {
        self.bx(c, s, BoxUv::default())
    }

    /// Boks med avrundede kanter (radius r, k trinn per 45°). `uv_top`:
    /// uv-rektangel for den flate delen av fremsiden (+z).
    pub fn rounded_box(&mut self, c: [f64; 3], s: [f64; 3], r: f64, k: usize, uv_top: Option<UvRect>) -> &mut Self {
        let h = [s[0] / 2.0, s[1] / 2.0, s[2] / 2.0];
        let r = r.min(h[0]).min(h[1]).min(h[2]);
        let inner = [h[0] - r, h[1] - r, h[2] - r];
        let coords = |hh: f64, ii: f64| -> Vec<f64> {
            let mut out = vec![-hh];
            for j in (1..k).rev() {
                out.push(-ii - r * ((j as f64 * PI) / 4.0 / k as f64).tan());
            }
            out.push(-ii);
            if ii > 1e-6 {
                out.push(ii);
            }
            for j in 1..k {
                out.push(ii + r * ((j as f64 * PI) / 4.0 / k as f64).tan());
            }
            out.push(hh);
            out
        };
        let ax = [coords(h[0], inner[0]), coords(h[1], inner[1]), coords(h[2], inner[2])];
        let faces: [(usize, usize, usize, f64); 6] = [(2, 0, 1, 1.0), (2, 1, 0, -1.0), (0, 1, 2, 1.0), (0, 2, 1, -1.0), (1, 2, 0, 1.0), (1, 0, 2, -1.0)];
        let ca = |v: f64, lim: f64| v.clamp(-lim, lim);
        for (n_axis, u_axis, v_axis, sign) in faces {
            let us = &ax[u_axis];
            let vs = &ax[v_axis];
            let base = self.count();
            for vj in vs.iter() {
                for ui in us.iter() {
                    let mut q = [0.0; 3];
                    q[n_axis] = sign * h[n_axis];
                    q[u_axis] = *ui;
                    q[v_axis] = *vj;
                    let cl = [ca(q[0], inner[0]), ca(q[1], inner[1]), ca(q[2], inner[2])];
                    let mut d = [q[0] - cl[0], q[1] - cl[1], q[2] - cl[2]];
                    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    let l = if l > 0.0 { l } else { 1.0 };
                    d = [d[0] / l, d[1] / l, d[2] / l];
                    let pos = [c[0] + cl[0] + d[0] * r, c[1] + cl[1] + d[1] * r, c[2] + cl[2] + d[2] * r];
                    let mut uv = None;
                    if let Some(t) = uv_top
                        && n_axis == 2
                        && sign > 0.0
                    {
                        let fu = ((q[0] + h[0]) / (2.0 * h[0])) as f32;
                        let fv = ((q[1] + h[1]) / (2.0 * h[1])) as f32;
                        uv = Some([t[0] + (t[2] - t[0]) * fu, t[1] + (t[3] - t[1]) * fv]);
                    }
                    self.vert(pos, d, uv);
                }
            }
            let w = us.len() as u32;
            for j in 0..(vs.len() as u32 - 1) {
                for i in 0..(w - 1) {
                    let a = base + j * w + i;
                    self.add_quad_oriented(a, a + 1, a + w + 1, a + w);
                }
            }
        }
        self
    }

    /// Sylinder langs z fra z0 til z1, med valgfrie lokk. `r1`: radius på z1.
    #[allow(clippy::too_many_arguments)]
    pub fn cylinder(&mut self, cx: f64, cy: f64, r: f64, z0: f64, z1: f64, seg: usize, caps: (bool, bool), uv_cap: Option<UvRect>, r1: Option<f64>) -> &mut Self {
        let r1 = r1.unwrap_or(r);
        let base = self.count();
        for i in 0..=seg {
            let a = i as f64 / seg as f64 * TAU;
            let (sa, ca) = a.sin_cos();
            let n = [ca, sa, (r - r1) / (z1 - z0).max(1e-6)];
            self.vert([cx + ca * r, cy + sa * r, z0], n, None);
            self.vert([cx + ca * r1, cy + sa * r1, z1], n, None);
        }
        for i in 0..seg as u32 {
            let a = base + i * 2;
            self.idx.extend_from_slice(&[a, a + 2, a + 3, a, a + 3, a + 1]);
        }
        if caps.1 {
            self.disc(cx, cy, r1, z1, 1.0, seg, uv_cap, 0.0);
        }
        if caps.0 {
            self.disc(cx, cy, r, z0, -1.0, seg, None, 0.0);
        }
        self
    }

    /// Skive i planet z med normal ±z (ring når r0 > 0).
    #[allow(clippy::too_many_arguments)]
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, z: f64, dir: f64, seg: usize, uv_rect: Option<UvRect>, r0: f64) -> &mut Self {
        let base = self.count();
        for i in 0..=seg {
            let a = i as f64 / seg as f64 * TAU;
            let (sa, ca) = a.sin_cos();
            let (uvo, uvi) = match uv_rect {
                Some(u) => (
                    Some([u[0] + (u[2] - u[0]) * (0.5 + ca * 0.5) as f32, u[1] + (u[3] - u[1]) * (0.5 + sa * 0.5) as f32]),
                    Some([u[0] + (u[2] - u[0]) * (0.5 + ca * 0.5 * r0 / r) as f32, u[1] + (u[3] - u[1]) * (0.5 + sa * 0.5 * r0 / r) as f32]),
                ),
                None => (None, None),
            };
            self.vert([cx + ca * r, cy + sa * r, z], [0.0, 0.0, dir], uvo);
            self.vert([cx + ca * r0, cy + sa * r0, z], [0.0, 0.0, dir], uvi);
        }
        for i in 0..seg as u32 {
            let a = base + i * 2;
            if dir > 0.0 {
                self.idx.extend_from_slice(&[a, a + 2, a + 3, a, a + 3, a + 1]);
            } else {
                self.idx.extend_from_slice(&[a, a + 3, a + 2, a, a + 1, a + 3]);
            }
        }
        self
    }

    /// Rør (åpen sylinder) med innside.
    #[allow(clippy::too_many_arguments)]
    pub fn tube(&mut self, cx: f64, cy: f64, ro: f64, ri: f64, z0: f64, z1: f64, seg: usize) -> &mut Self {
        self.cylinder(cx, cy, ro, z0, z1, seg, (false, false), None, None);
        let base = self.count();
        for i in 0..=seg {
            let a = i as f64 / seg as f64 * TAU;
            let (sa, ca) = a.sin_cos();
            self.vert([cx + ca * ri, cy + sa * ri, z0], [-ca, -sa, 0.0], None);
            self.vert([cx + ca * ri, cy + sa * ri, z1], [-ca, -sa, 0.0], None);
        }
        for i in 0..seg as u32 {
            let a = base + i * 2;
            self.idx.extend_from_slice(&[a, a + 3, a + 2, a, a + 1, a + 3]);
        }
        self.disc(cx, cy, ro, z1, 1.0, seg, None, ri);
        self.disc(cx, cy, ro, z0, -1.0, seg, None, ri);
        self
    }

    /// Del av en torus (rørbøy) i xz-planet: sentrum c, bøyeradius R, rørradius r.
    #[allow(clippy::too_many_arguments)]
    pub fn torus_arc(&mut self, c: [f64; 3], big_r: f64, r: f64, a0: f64, a1: f64, seg_a: usize, seg_r: usize) -> &mut Self {
        let base = self.count();
        for i in 0..=seg_a {
            let a = a0 + (a1 - a0) * i as f64 / seg_a as f64;
            let (sa, ca) = a.sin_cos();
            for j in 0..=seg_r {
                let b = j as f64 / seg_r as f64 * TAU;
                let (sb, cb) = b.sin_cos();
                let n = [ca * cb, sb, sa * cb];
                self.vert([c[0] + ca * (big_r + r * cb), c[1] + r * sb, c[2] + sa * (big_r + r * cb)], n, None);
            }
        }
        let w = seg_r as u32 + 1;
        for i in 0..seg_a as u32 {
            for j in 0..seg_r as u32 {
                let a = base + i * w + j;
                self.add_quad_oriented(a, a + w, a + w + 1, a + 1);
            }
        }
        self
    }

    /// Konveks mangekant i xy-planet ekstrudert fra z0 til z1.
    pub fn extrude_convex(&mut self, pts: &[[f64; 2]], z0: f64, z1: f64, front: bool, back: bool, sides: bool) -> &mut Self {
        if front {
            let b = self.count();
            for p in pts {
                self.vert([p[0], p[1], z1], [0.0, 0.0, 1.0], None);
            }
            for i in 1..pts.len() as u32 - 1 {
                self.idx.extend_from_slice(&[b, b + i, b + i + 1]);
            }
        }
        if back {
            let b = self.count();
            for p in pts {
                self.vert([p[0], p[1], z0], [0.0, 0.0, -1.0], None);
            }
            for i in 1..pts.len() as u32 - 1 {
                self.idx.extend_from_slice(&[b, b + i + 1, b + i]);
            }
        }
        if sides {
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
                let l = (ex * ex + ey * ey).sqrt().max(1e-9);
                let n = [ey / l, -ex / l, 0.0];
                self.quad([a[0], a[1], z0], [b[0], b[1], z0], [b[0], b[1], z1], [a[0], a[1], z1], n, None);
            }
        }
        self
    }

    /// Flate fra et rutenett av punkter, normaler fra naboene.
    pub fn grid(&mut self, rows: usize, cols: usize, f: impl Fn(f64, f64) -> [f64; 3]) -> &mut Self {
        let base = self.count();
        let mut pts = Vec::with_capacity(rows * cols);
        for j in 0..rows {
            for i in 0..cols {
                pts.push(f(i as f64 / (cols - 1) as f64, j as f64 / (rows - 1) as f64));
            }
        }
        for j in 0..rows {
            for i in 0..cols {
                let p = pts[j * cols + i];
                let pu = pts[j * cols + (i + 1).min(cols - 1)];
                let pu0 = pts[j * cols + i.saturating_sub(1)];
                let pv = pts[(j + 1).min(rows - 1) * cols + i];
                let pv0 = pts[j.saturating_sub(1) * cols + i];
                let du = [pu[0] - pu0[0], pu[1] - pu0[1], pu[2] - pu0[2]];
                let dv = [pv[0] - pv0[0], pv[1] - pv0[1], pv[2] - pv0[2]];
                let mut n = [du[1] * dv[2] - du[2] * dv[1], du[2] * dv[0] - du[0] * dv[2], du[0] * dv[1] - du[1] * dv[0]];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                let l = if l > 0.0 { l } else { 1.0 };
                n = [n[0] / l, n[1] / l, n[2] / l];
                self.vert(p, n, None);
            }
        }
        let c = cols as u32;
        for j in 0..rows as u32 - 1 {
            for i in 0..c - 1 {
                let a = base + j * c + i;
                self.idx.extend_from_slice(&[a, a + 1, a + c + 1, a, a + c + 1, a + c]);
            }
        }
        self
    }

    /// Bue av en flat stripe i planet z (lysring), med lave kantvegger.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_strip(&mut self, cx: f64, cy: f64, r0: f64, r1: f64, a0: f64, a1: f64, z0: f64, z1: f64, seg: usize) -> &mut Self {
        let base = self.count();
        for i in 0..=seg {
            let a = a0 + (a1 - a0) * i as f64 / seg as f64;
            let (sa, ca) = a.sin_cos();
            self.vert([cx + ca * r0, cy + sa * r0, z1], [0.0, 0.0, 1.0], None);
            self.vert([cx + ca * r1, cy + sa * r1, z1], [0.0, 0.0, 1.0], None);
        }
        for i in 0..seg as u32 {
            let a = base + i * 2;
            self.add_quad_oriented(a, a + 2, a + 3, a + 1);
        }
        for (rr, sgn) in [(r1, 1.0), (r0, -1.0)] {
            let b = self.count();
            for i in 0..=seg {
                let a = a0 + (a1 - a0) * i as f64 / seg as f64;
                let (sa, ca) = a.sin_cos();
                self.vert([cx + ca * rr, cy + sa * rr, z0], [ca * sgn, sa * sgn, 0.0], None);
                self.vert([cx + ca * rr, cy + sa * rr, z1], [ca * sgn, sa * sgn, 0.0], None);
            }
            for i in 0..seg as u32 {
                let a = b + i * 2;
                self.add_quad_oriented(a, a + 2, a + 3, a + 1);
            }
        }
        self
    }

    pub fn build(self) -> MeshData {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in self.p.chunks_exact(3) {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        MeshData { pos: self.p, nrm: self.n, uv: self.uv, led: self.led, idx: self.idx, min, max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boks_og_rundet_boks() {
        let mut b = MeshBuilder::new();
        b.boxs([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let d = b.build();
        assert_eq!(d.verts(), 24);
        assert_eq!(d.idx.len(), 36);
        assert_eq!(d.min, [-1.0; 3]);
        let mut r = MeshBuilder::new();
        r.rounded_box([0.0, 0.0, 0.0], [10.0, 6.0, 2.0], 0.5, 2, None);
        let d = r.build();
        assert!(d.idx.len() > 36);
        assert!((d.max[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn transformasjonen_flytter_hjornene() {
        let mut b = MeshBuilder::new();
        b.with(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)), |b| {
            b.boxs([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        });
        let d = b.build();
        assert!((d.min[0] - 9.0).abs() < 1e-5);
    }
}
