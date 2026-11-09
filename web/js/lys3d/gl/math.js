// Liten matrise- og vektormatte for WebGL (kolonnevis, som GLSL).
// Bare det modellen og kameraet trenger. Ingen avhengigheter.

export const DEG = Math.PI / 180;
export const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
export const lerp = (a, b, t) => a + (b - a) * t;
export const smoothstep = (a, b, x) => {
  const t = clamp((x - a) / (b - a), 0, 1);
  return t * t * (3 - 2 * t);
};

export const mat4 = {
  create() {
    const m = new Float32Array(16);
    m[0] = m[5] = m[10] = m[15] = 1;
    return m;
  },
  identity(o = new Float32Array(16)) {
    o.fill(0);
    o[0] = o[5] = o[10] = o[15] = 1;
    return o;
  },
  copy(o, a) {
    o.set(a);
    return o;
  },
  /** o = a · b */
  mul(o, a, b) {
    const r = new Float32Array(16);
    for (let c = 0; c < 4; c++) {
      for (let rI = 0; rI < 4; rI++) {
        r[c * 4 + rI] = a[rI] * b[c * 4] + a[4 + rI] * b[c * 4 + 1] + a[8 + rI] * b[c * 4 + 2] + a[12 + rI] * b[c * 4 + 3];
      }
    }
    o.set(r);
    return o;
  },
  translation(x, y, z, o = mat4.create()) {
    mat4.identity(o);
    o[12] = x;
    o[13] = y;
    o[14] = z;
    return o;
  },
  scaling(x, y, z, o = mat4.create()) {
    mat4.identity(o);
    o[0] = x;
    o[5] = y;
    o[10] = z;
    return o;
  },
  rotX(a, o = mat4.create()) {
    mat4.identity(o);
    const c = Math.cos(a);
    const s = Math.sin(a);
    o[5] = c;
    o[6] = s;
    o[9] = -s;
    o[10] = c;
    return o;
  },
  rotY(a, o = mat4.create()) {
    mat4.identity(o);
    const c = Math.cos(a);
    const s = Math.sin(a);
    o[0] = c;
    o[2] = -s;
    o[8] = s;
    o[10] = c;
    return o;
  },
  rotZ(a, o = mat4.create()) {
    mat4.identity(o);
    const c = Math.cos(a);
    const s = Math.sin(a);
    o[0] = c;
    o[1] = s;
    o[4] = -s;
    o[5] = c;
    return o;
  },
  /** Komponerer en liste av matriser fra venstre mot høyre: m0 · m1 · … */
  chain(...ms) {
    const o = mat4.create();
    for (const m of ms) mat4.mul(o, o, m);
    return o;
  },
  perspective(fovy, aspect, near, far, o = mat4.create()) {
    const f = 1 / Math.tan(fovy / 2);
    o.fill(0);
    o[0] = f / aspect;
    o[5] = f;
    o[10] = (far + near) / (near - far);
    o[11] = -1;
    o[14] = (2 * far * near) / (near - far);
    return o;
  },
  ortho(l, r, b, t, n, f, o = mat4.create()) {
    o.fill(0);
    o[0] = 2 / (r - l);
    o[5] = 2 / (t - b);
    o[10] = -2 / (f - n);
    o[12] = -(r + l) / (r - l);
    o[13] = -(t + b) / (t - b);
    o[14] = -(f + n) / (f - n);
    o[15] = 1;
    return o;
  },
  lookAt(eye, target, up, o = mat4.create()) {
    let zx = eye[0] - target[0];
    let zy = eye[1] - target[1];
    let zz = eye[2] - target[2];
    let l = Math.hypot(zx, zy, zz) || 1;
    zx /= l;
    zy /= l;
    zz /= l;
    let xx = up[1] * zz - up[2] * zy;
    let xy = up[2] * zx - up[0] * zz;
    let xz = up[0] * zy - up[1] * zx;
    l = Math.hypot(xx, xy, xz) || 1;
    xx /= l;
    xy /= l;
    xz /= l;
    const yx = zy * xz - zz * xy;
    const yy = zz * xx - zx * xz;
    const yz = zx * xy - zy * xx;
    o[0] = xx;
    o[1] = yx;
    o[2] = zx;
    o[3] = 0;
    o[4] = xy;
    o[5] = yy;
    o[6] = zy;
    o[7] = 0;
    o[8] = xz;
    o[9] = yz;
    o[10] = zz;
    o[11] = 0;
    o[12] = -(xx * eye[0] + xy * eye[1] + xz * eye[2]);
    o[13] = -(yx * eye[0] + yy * eye[1] + yz * eye[2]);
    o[14] = -(zx * eye[0] + zy * eye[1] + zz * eye[2]);
    o[15] = 1;
    return o;
  },
  /** Normalmatrise (3 × 3, invers transponert) av en 4 × 4. */
  normal3(m, o = new Float32Array(9)) {
    const a00 = m[0], a01 = m[1], a02 = m[2];
    const a10 = m[4], a11 = m[5], a12 = m[6];
    const a20 = m[8], a21 = m[9], a22 = m[10];
    const b01 = a22 * a11 - a12 * a21;
    const b11 = -a22 * a10 + a12 * a20;
    const b21 = a21 * a10 - a11 * a20;
    let det = a00 * b01 + a01 * b11 + a02 * b21;
    det = det ? 1 / det : 0;
    o[0] = b01 * det;
    o[1] = (-a22 * a01 + a02 * a21) * det;
    o[2] = (a12 * a01 - a02 * a11) * det;
    o[3] = b11 * det;
    o[4] = (a22 * a00 - a02 * a20) * det;
    o[5] = (-a12 * a00 + a02 * a10) * det;
    o[6] = b21 * det;
    o[7] = (-a21 * a00 + a01 * a20) * det;
    o[8] = (a11 * a00 - a01 * a10) * det;
    // Transponer (GLSL leser kolonnevis).
    const t = [o[0], o[3], o[6], o[1], o[4], o[7], o[2], o[5], o[8]];
    o.set(t);
    return o;
  },
  transformPoint(m, p) {
    const x = p[0], y = p[1], z = p[2];
    const w = m[3] * x + m[7] * y + m[11] * z + m[15] || 1;
    return [(m[0] * x + m[4] * y + m[8] * z + m[12]) / w, (m[1] * x + m[5] * y + m[9] * z + m[13]) / w, (m[2] * x + m[6] * y + m[10] * z + m[14]) / w];
  },
  transformDir(m, d) {
    return [m[0] * d[0] + m[4] * d[1] + m[8] * d[2], m[1] * d[0] + m[5] * d[1] + m[9] * d[2], m[2] * d[0] + m[6] * d[1] + m[10] * d[2]];
  },
};

export const vec3 = {
  add: (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  sub: (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  scale: (a, k) => [a[0] * k, a[1] * k, a[2] * k],
  dot: (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  cross: (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  len: (a) => Math.hypot(a[0], a[1], a[2]),
  norm: (a) => {
    const l = Math.hypot(a[0], a[1], a[2]) || 1;
    return [a[0] / l, a[1] / l, a[2] / l];
  },
  lerp: (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t],
};

/** sRGB (0–255 eller 0–1) til lineær. */
export function srgbToLinear(c) {
  const v = c > 1 ? c / 255 : c;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}
export function hexToLinear(hex) {
  const n = parseInt(hex.replace('#', ''), 16);
  return [srgbToLinear((n >> 16) & 255), srgbToLinear((n >> 8) & 255), srgbToLinear(n & 255)];
}

export const easeInOut = (t) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);
export const easeOut = (t) => 1 - (1 - t) ** 3;
export const easeIn = (t) => t * t * t;
