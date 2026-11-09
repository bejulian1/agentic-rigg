// Geometri: en enkel bygger som samler posisjon, normal, uv og en ekstra
// verdi per hjørne (indeks til en lysdiode, ellers −1), og primitiver som
// skjermkortet bygges av. Alle mål er i millimeter.

import { mat4 } from './math.js';

const WHITE_UV = [0.004, 0.004]; // et hvitt punkt i etikettatlaset

export class MeshBuilder {
  constructor() {
    this.p = [];
    this.n = [];
    this.uv = [];
    this.led = [];
    this.idx = [];
    this.m = null; // gjeldende transformasjon (eller null)
    this.nm = null;
    this.ledIndex = -1;
    this.defaultUv = WHITE_UV; // uv for flater uten eget uv-rektangel
  }
  get count() {
    return this.p.length / 3;
  }
  /** Setter en transformasjon for det som legges til etterpå. */
  with(m, fn) {
    const pm = this.m;
    const pn = this.nm;
    this.m = pm ? mat4.mul(mat4.create(), pm, m) : m;
    this.nm = mat4.normal3(this.m);
    fn(this);
    this.m = pm;
    this.nm = pn;
    return this;
  }
  vert(p, n, uv = this.defaultUv) {
    let [x, y, z] = p;
    let [nx, ny, nz] = n;
    if (this.m) {
      const m = this.m;
      const X = m[0] * x + m[4] * y + m[8] * z + m[12];
      const Y = m[1] * x + m[5] * y + m[9] * z + m[13];
      const Z = m[2] * x + m[6] * y + m[10] * z + m[14];
      x = X;
      y = Y;
      z = Z;
      const q = this.nm;
      const NX = q[0] * nx + q[3] * ny + q[6] * nz;
      const NY = q[1] * nx + q[4] * ny + q[7] * nz;
      const NZ = q[2] * nx + q[5] * ny + q[8] * nz;
      const l = Math.hypot(NX, NY, NZ) || 1;
      nx = NX / l;
      ny = NY / l;
      nz = NZ / l;
    }
    this.p.push(x, y, z);
    this.n.push(nx, ny, nz);
    this.uv.push(uv[0], uv[1]);
    this.led.push(this.ledIndex);
    return this.count - 1;
  }
  tri(a, b, c) {
    this.idx.push(a, b, c);
  }
  /** Firkant a-b-c-d mot klokka sett fra normalen. */
  quad(a, b, c, d, n, uv = null) {
    const du = this.defaultUv;
    const u = uv || [du, du, du, du];
    const i = this.vert(a, n, u[0]);
    this.vert(b, n, u[1]);
    this.vert(c, n, u[2]);
    this.vert(d, n, u[3]);
    // Vindingen rettes etter normalen, så en firkant aldri peker feil vei.
    this.addQuadOriented(i, i + 1, i + 2, i + 3);
  }

  /**
   * Boks med sentrum c og størrelse s. uv: { top: [u0,v0,u1,v1], front, … }
   * gir egne uv-rektangler for enkeltflater (resten bruker det hvite punktet).
   * Flatene heter etter normalen: px, nx, py, ny, pz (fremside, +z), nz.
   */
  box(c, s, uv = {}) {
    const [cx, cy, cz] = c;
    const hx = s[0] / 2;
    const hy = s[1] / 2;
    const hz = s[2] / 2;
    const r = (k) => (uv[k] ? rectUv(uv[k]) : null);
    const x0 = cx - hx, x1 = cx + hx, y0 = cy - hy, y1 = cy + hy, z0 = cz - hz, z1 = cz + hz;
    if (!uv.skip?.includes('pz')) this.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0, 0, 1], r('pz'));
    if (!uv.skip?.includes('nz')) this.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0, 0, -1], r('nz'));
    if (!uv.skip?.includes('px')) this.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1, 0, 0], r('px'));
    if (!uv.skip?.includes('nx')) this.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1, 0, 0], r('nx'));
    if (!uv.skip?.includes('py')) this.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0, 1, 0], r('py'));
    if (!uv.skip?.includes('ny')) this.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0, -1, 0], r('ny'));
    return this;
  }

  /**
   * Boks med avrundede kanter (radius r, k trinn per 45°). Normalene følger
   * rundingen, så kantene fanger lyset slik støpte og frest deler gjør.
   * uvTop: uv-rektangel for den flate delen av fremsiden (+z).
   */
  roundedBox(c, s, r, k = 2, uvTop = null) {
    const h = [s[0] / 2, s[1] / 2, s[2] / 2];
    r = Math.min(r, h[0], h[1], h[2]);
    const inner = [h[0] - r, h[1] - r, h[2] - r];
    // Koordinater langs hver akse: flate, så k trinn med tan-fordeling i hjørnet.
    const coords = (hh, ii) => {
      const out = [-hh];
      for (let j = k - 1; j >= 1; j--) out.push(-ii - r * Math.tan((j * Math.PI) / 4 / k));
      out.push(-ii);
      if (ii > 1e-6) out.push(ii);
      for (let j = 1; j < k; j++) out.push(ii + r * Math.tan((j * Math.PI) / 4 / k));
      out.push(hh);
      return out;
    };
    const ax = [coords(h[0], inner[0]), coords(h[1], inner[1]), coords(h[2], inner[2])];
    const faces = [
      [2, 0, 1, 1],
      [2, 1, 0, -1],
      [0, 1, 2, 1],
      [0, 2, 1, -1],
      [1, 2, 0, 1],
      [1, 0, 2, -1],
    ];
    for (const [nAxis, uAxis, vAxis, sign] of faces) {
      const us = ax[uAxis];
      const vs = ax[vAxis];
      const base = this.count;
      for (let j = 0; j < vs.length; j++) {
        for (let i = 0; i < us.length; i++) {
          const q = [0, 0, 0];
          q[nAxis] = sign * h[nAxis];
          q[uAxis] = us[i];
          q[vAxis] = vs[j];
          const cl = [clampAbs(q[0], inner[0]), clampAbs(q[1], inner[1]), clampAbs(q[2], inner[2])];
          let d = [q[0] - cl[0], q[1] - cl[1], q[2] - cl[2]];
          const l = Math.hypot(d[0], d[1], d[2]) || 1;
          d = [d[0] / l, d[1] / l, d[2] / l];
          const pos = [c[0] + cl[0] + d[0] * r, c[1] + cl[1] + d[1] * r, c[2] + cl[2] + d[2] * r];
          let uv = this.defaultUv;
          if (uvTop && nAxis === 2 && sign > 0) {
            const fu = (q[0] + h[0]) / (2 * h[0]);
            const fv = (q[1] + h[1]) / (2 * h[1]);
            uv = [uvTop[0] + (uvTop[2] - uvTop[0]) * fu, uvTop[1] + (uvTop[3] - uvTop[1]) * fv];
          }
          this.vert(pos, d, uv);
        }
      }
      const W = us.length;
      for (let j = 0; j < vs.length - 1; j++) {
        for (let i = 0; i < W - 1; i++) {
          const a = base + j * W + i;
          // Vindingen velges ut fra normalen, ikke fra aksenes rekkefølge.
          this.addQuadOriented(a, a + 1, a + W + 1, a + W);
        }
      }
    }
    return this;
  }

  /** Legger til en firkant av eksisterende hjørner og snur den om nødvendig, så den peker langs normalen. */
  addQuadOriented(a, b, c, d) {
    const P = this.p;
    const N = this.n;
    const ux = P[b * 3] - P[a * 3], uy = P[b * 3 + 1] - P[a * 3 + 1], uz = P[b * 3 + 2] - P[a * 3 + 2];
    const vx = P[d * 3] - P[a * 3], vy = P[d * 3 + 1] - P[a * 3 + 1], vz = P[d * 3 + 2] - P[a * 3 + 2];
    let cx = uy * vz - uz * vy, cy = uz * vx - ux * vz, cz = ux * vy - uy * vx;
    if (Math.abs(cx) + Math.abs(cy) + Math.abs(cz) < 1e-12) {
      // Degenerert kant (null-lengde i hjørnet): bruk den andre diagonalen.
      const wx = P[c * 3] - P[a * 3], wy = P[c * 3 + 1] - P[a * 3 + 1], wz = P[c * 3 + 2] - P[a * 3 + 2];
      cx = uy * wz - uz * wy;
      cy = uz * wx - ux * wz;
      cz = ux * wy - uy * wx;
      if (Math.abs(cx) + Math.abs(cy) + Math.abs(cz) < 1e-12) {
        cx = wy * vz - wz * vy;
        cy = wz * vx - wx * vz;
        cz = wx * vy - wy * vx;
      }
    }
    const nx = N[a * 3] + N[c * 3], ny = N[a * 3 + 1] + N[c * 3 + 1], nz = N[a * 3 + 2] + N[c * 3 + 2];
    if (cx * nx + cy * ny + cz * nz >= 0) this.idx.push(a, b, c, a, c, d);
    else this.idx.push(a, c, b, a, d, c);
  }

  /**
   * Sylinder langs z fra z0 til z1 (sentrum cx, cy), med valgfrie lokk.
   * Bruk with() for andre akser. uvCap: uv-rektangel for lokket på z1.
   */
  cylinder(cx, cy, r, z0, z1, seg = 24, { caps = [true, true], uvCap = null, r1 = r } = {}) {
    const base = this.count;
    for (let i = 0; i <= seg; i++) {
      const a = (i / seg) * Math.PI * 2;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      const n = [ca, sa, (r - r1) / Math.max(1e-6, z1 - z0)];
      this.vert([cx + ca * r, cy + sa * r, z0], n);
      this.vert([cx + ca * r1, cy + sa * r1, z1], n);
    }
    for (let i = 0; i < seg; i++) {
      const a = base + i * 2;
      this.idx.push(a, a + 2, a + 3, a, a + 3, a + 1);
    }
    if (caps[1]) this.disc(cx, cy, r1, z1, 1, seg, uvCap);
    if (caps[0]) this.disc(cx, cy, r, z0, -1, seg, null);
    return this;
  }
  /** Skive i planet z med normal ±z. */
  disc(cx, cy, r, z, dir = 1, seg = 24, uvRect = null, r0 = 0) {
    const base = this.count;
    for (let i = 0; i <= seg; i++) {
      const a = (i / seg) * Math.PI * 2;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      const uvO = uvRect ? [uvRect[0] + (uvRect[2] - uvRect[0]) * (0.5 + ca * 0.5), uvRect[1] + (uvRect[3] - uvRect[1]) * (0.5 + sa * 0.5)] : this.defaultUv;
      const uvI = uvRect ? [uvRect[0] + (uvRect[2] - uvRect[0]) * (0.5 + (ca * 0.5 * r0) / r), uvRect[1] + (uvRect[3] - uvRect[1]) * (0.5 + (sa * 0.5 * r0) / r)] : this.defaultUv;
      this.vert([cx + ca * r, cy + sa * r, z], [0, 0, dir], uvO);
      this.vert([cx + ca * r0, cy + sa * r0, z], [0, 0, dir], uvI);
    }
    for (let i = 0; i < seg; i++) {
      const a = base + i * 2;
      if (dir > 0) this.idx.push(a, a + 2, a + 3, a, a + 3, a + 1);
      else this.idx.push(a, a + 3, a + 2, a, a + 1, a + 3);
    }
    return this;
  }
  /** Rør (åpen sylinder) med innside, f.eks. kanten rundt vifteåpningen. */
  tube(cx, cy, rOuter, rInner, z0, z1, seg = 48) {
    // Ytterside
    this.cylinder(cx, cy, rOuter, z0, z1, seg, { caps: [false, false] });
    // Innside (normal inn mot aksen)
    const base = this.count;
    for (let i = 0; i <= seg; i++) {
      const a = (i / seg) * Math.PI * 2;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      this.vert([cx + ca * rInner, cy + sa * rInner, z0], [-ca, -sa, 0]);
      this.vert([cx + ca * rInner, cy + sa * rInner, z1], [-ca, -sa, 0]);
    }
    for (let i = 0; i < seg; i++) {
      const a = base + i * 2;
      this.idx.push(a, a + 3, a + 2, a, a + 1, a + 3);
    }
    this.disc(cx, cy, rOuter, z1, 1, seg, null, rInner);
    this.disc(cx, cy, rOuter, z0, -1, seg, null, rInner);
    return this;
  }
  /** Kvart torus (rørbøy) i xz-planet: sentrum c, bøyeradius R, rørradius r, fra vinkel a0 til a1. */
  torusArc(c, R, r, a0, a1, segA = 12, segR = 14) {
    const base = this.count;
    for (let i = 0; i <= segA; i++) {
      const a = a0 + ((a1 - a0) * i) / segA;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      for (let j = 0; j <= segR; j++) {
        const b = (j / segR) * Math.PI * 2;
        const cb = Math.cos(b);
        const sb = Math.sin(b);
        // Ringen ligger i xz-planet; røret går rundt i (radial, y).
        const n = [ca * cb, sb, sa * cb];
        this.vert([c[0] + ca * (R + r * cb), c[1] + r * sb, c[2] + sa * (R + r * cb)], n);
      }
    }
    const W = segR + 1;
    for (let i = 0; i < segA; i++) {
      for (let j = 0; j < segR; j++) {
        const a = base + i * W + j;
        this.addQuadOriented(a, a + W, a + W + 1, a + 1);
      }
    }
    return this;
  }
  /** Konveks mangekant i xy-planet (mot klokka) ekstrudert fra z0 til z1. */
  extrudeConvex(pts, z0, z1, { front = true, back = true, sides = true, uvFront = null, bounds = null } = {}) {
    const uvOf = (x, y) => {
      if (!uvFront || !bounds) return this.defaultUv;
      return [uvFront[0] + ((x - bounds[0]) / (bounds[2] - bounds[0])) * (uvFront[2] - uvFront[0]), uvFront[1] + ((y - bounds[1]) / (bounds[3] - bounds[1])) * (uvFront[3] - uvFront[1])];
    };
    if (front) {
      const b = this.count;
      for (const [x, y] of pts) this.vert([x, y, z1], [0, 0, 1], uvOf(x, y));
      for (let i = 1; i < pts.length - 1; i++) this.idx.push(b, b + i, b + i + 1);
    }
    if (back) {
      const b = this.count;
      for (const [x, y] of pts) this.vert([x, y, z0], [0, 0, -1]);
      for (let i = 1; i < pts.length - 1; i++) this.idx.push(b, b + i + 1, b + i);
    }
    if (sides) {
      for (let i = 0; i < pts.length; i++) {
        const [ax, ay] = pts[i];
        const [bx, by] = pts[(i + 1) % pts.length];
        const ex = bx - ax;
        const ey = by - ay;
        const l = Math.hypot(ex, ey) || 1;
        const n = [ey / l, -ex / l, 0];
        this.quad([ax, ay, z0], [bx, by, z0], [bx, by, z1], [ax, ay, z1], n);
      }
    }
    return this;
  }
  /** Flate fra et rutenett av punkter (rows × cols), normaler fra naboene. Tosidig i skyggeleggingen. */
  grid(rows, cols, fn) {
    const base = this.count;
    const pts = [];
    for (let j = 0; j < rows; j++) for (let i = 0; i < cols; i++) pts.push(fn(i / (cols - 1), j / (rows - 1)));
    for (let j = 0; j < rows; j++) {
      for (let i = 0; i < cols; i++) {
        const p = pts[j * cols + i];
        const pu = pts[j * cols + Math.min(cols - 1, i + 1)];
        const pu0 = pts[j * cols + Math.max(0, i - 1)];
        const pv = pts[Math.min(rows - 1, j + 1) * cols + i];
        const pv0 = pts[Math.max(0, j - 1) * cols + i];
        const du = [pu[0] - pu0[0], pu[1] - pu0[1], pu[2] - pu0[2]];
        const dv = [pv[0] - pv0[0], pv[1] - pv0[1], pv[2] - pv0[2]];
        let n = [du[1] * dv[2] - du[2] * dv[1], du[2] * dv[0] - du[0] * dv[2], du[0] * dv[1] - du[1] * dv[0]];
        const l = Math.hypot(n[0], n[1], n[2]) || 1;
        n = [n[0] / l, n[1] / l, n[2] / l];
        this.vert(p, n);
      }
    }
    for (let j = 0; j < rows - 1; j++) {
      for (let i = 0; i < cols - 1; i++) {
        const a = base + j * cols + i;
        this.idx.push(a, a + 1, a + cols + 1, a, a + cols + 1, a + cols);
      }
    }
    return this;
  }
  /** Bue av en flat stripe i planet z (lysring): radius r0–r1, vinkel a0–a1, med kantvegger. */
  arcStrip(cx, cy, r0, r1, a0, a1, z0, z1, seg = 6) {
    const base = this.count;
    for (let i = 0; i <= seg; i++) {
      const a = a0 + ((a1 - a0) * i) / seg;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      this.vert([cx + ca * r0, cy + sa * r0, z1], [0, 0, 1]);
      this.vert([cx + ca * r1, cy + sa * r1, z1], [0, 0, 1]);
    }
    for (let i = 0; i < seg; i++) {
      const a = base + i * 2;
      this.addQuadOriented(a, a + 2, a + 3, a + 1);
    }
    // Ytter- og innerkant (lav vegg)
    for (const [rr, sgn] of [
      [r1, 1],
      [r0, -1],
    ]) {
      const b = this.count;
      for (let i = 0; i <= seg; i++) {
        const a = a0 + ((a1 - a0) * i) / seg;
        const ca = Math.cos(a);
        const sa = Math.sin(a);
        this.vert([cx + ca * rr, cy + sa * rr, z0], [ca * sgn, sa * sgn, 0]);
        this.vert([cx + ca * rr, cy + sa * rr, z1], [ca * sgn, sa * sgn, 0]);
      }
      for (let i = 0; i < seg; i++) {
        const a = b + i * 2;
        this.addQuadOriented(a, a + 2, a + 3, a + 1);
      }
    }
    return this;
  }

  build() {
    return {
      position: new Float32Array(this.p),
      normal: new Float32Array(this.n),
      uv: new Float32Array(this.uv),
      led: new Float32Array(this.led),
      index: this.count > 65535 ? new Uint32Array(this.idx) : new Uint16Array(this.idx),
      count: this.idx.length,
      bounds: bounds(this.p),
    };
  }
}

function clampAbs(v, lim) {
  return Math.max(-lim, Math.min(lim, v));
}
function rectUv(r) {
  return [
    [r[0], r[1]],
    [r[2], r[1]],
    [r[2], r[3]],
    [r[0], r[3]],
  ];
}
function bounds(p) {
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < p.length; i += 3) {
    for (let k = 0; k < 3; k++) {
      if (p[i + k] < min[k]) min[k] = p[i + k];
      if (p[i + k] > max[k]) max[k] = p[i + k];
    }
  }
  return { min, max };
}
