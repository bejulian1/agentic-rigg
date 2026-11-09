// WebGL 2-renderer for skjermkortet. Skrevet for hånd, uten bibliotek.
//
// Passene i ett bilde:
//   1. Skyggekart fra hovedlyset (bare når noe har flyttet seg eller lyset
//      har dreid; viftehjulene kaster ikke skygge, så snurrende vifter alene
//      tegner ikke kartet på nytt).
//   2. Bakgrunnslys (valgfritt): myk farget flekk bak kortet.
//   3. Ugjennomsiktige deler: lys fra tre retninger, halvkule-omgivelse,
//      enkel studio-refleksjon for metall, skygge med PCF, lys fra de tre
//      LED-sonene (lyslist som linjelys, ringene som punktlys) og LED-ene selv
//      som selvlysende flater. ACES-tonekurve og gamma til slutt.
//   4. Gulvet (fanger skyggen og litt av LED-lyset, toner ut mot kantene).
//   5. Lag som tones ut (fjernes/settes på), sortert bakfra.
//   6. Glorie rundt hver LED (additiv, instansiert).
// Plukking (hvilken del er under pekeren) tegner bare én piksel.

import { mat4 } from './math.js';

const VS_MAIN = `#version 300 es
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
layout(location=3) in float a_led;
uniform mat4 u_model;
uniform mat3 u_nmat;
uniform mat4 u_vp;
uniform mat4 u_lvp;
out vec3 v_w;
out vec3 v_n;
out vec2 v_uv;
out vec3 v_l;
out vec4 v_ls;
flat out int v_led;
void main() {
  vec4 w = u_model * vec4(a_pos, 1.0);
  v_w = w.xyz;
  v_n = u_nmat * a_nrm;
  v_uv = a_uv;
  v_l = a_pos;
  v_ls = u_lvp * w;
  v_led = int(floor(a_led + 0.5));
  gl_Position = u_vp * w;
}`;

const LIGHT_COMMON = `
uniform vec3 u_barA;
uniform vec3 u_barB;
uniform vec3 u_barCol;
uniform vec3 u_f1Pos;
uniform vec3 u_f1Col;
uniform vec3 u_f2Pos;
uniform vec3 u_f2Col;
uniform highp sampler2DShadow u_shadow;
uniform float u_shadowTexel;
in vec4 v_ls;

float shadowAt(vec4 ls, float soft) {
  vec3 p = ls.xyz / ls.w * 0.5 + 0.5;
  if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 || p.z > 1.0) return 1.0;
  float s = 0.0;
  float t = u_shadowTexel * soft;
  for (int i = -1; i <= 1; i++)
    for (int j = -1; j <= 1; j++)
      s += texture(u_shadow, vec3(p.xy + vec2(float(i), float(j)) * t, p.z - 0.0006));
  return s / 9.0;
}
vec3 pointLight(vec3 pos, vec3 col, float range, vec3 P, vec3 N, vec3 diff, vec3 F0, float shin, vec3 V) {
  vec3 d = pos - P;
  float dist = max(length(d), 1.0);
  vec3 L = d / dist;
  float att = 1.0 / (1.0 + dist * dist / (range * range));
  att *= 1.0 - smoothstep(range * 2.5, range * 4.0, dist);
  float ndl = max(dot(N, L), 0.0);
  vec3 H = normalize(L + V);
  float spec = (shin + 8.0) / 25.0 * pow(max(dot(N, H), 0.0), shin);
  return col * att * ndl * (diff + F0 * spec);
}
vec3 ledLights(vec3 P, vec3 N, vec3 diff, vec3 F0, float shin, vec3 V) {
  vec3 ab = u_barB - u_barA;
  float t = clamp(dot(P - u_barA, ab) / max(dot(ab, ab), 1e-3), 0.0, 1.0);
  vec3 c = pointLight(u_barA + ab * t, u_barCol, 26.0, P, N, diff, F0, shin, V);
  c += pointLight(u_f1Pos, u_f1Col, 40.0, P, N, diff, F0, shin, V);
  c += pointLight(u_f2Pos, u_f2Col, 40.0, P, N, diff, F0, shin, V);
  return c;
}
`;

const FS_MAIN = `#version 300 es
precision highp float;
in vec3 v_w;
in vec3 v_n;
in vec2 v_uv;
in vec3 v_l;
flat in int v_led;
uniform vec3 u_color;
uniform float u_rough;
uniform float u_metal;
uniform float u_alpha;
uniform sampler2D u_map;
uniform float u_useMap;
uniform float u_alphaTest;
uniform float u_double;
uniform vec4 u_holes[2];
uniform float u_highlight;
uniform vec3 u_accent;
uniform vec3 u_led[56];
uniform vec3 u_zoneGain;
uniform vec3 u_cam;
uniform vec3 u_keyDir;
uniform vec3 u_keyCol;
uniform vec3 u_fillDir;
uniform vec3 u_fillCol;
uniform vec3 u_rimDir;
uniform vec3 u_rimCol;
uniform vec3 u_sky;
uniform vec3 u_ground;
uniform float u_exposure;
uniform float u_ledDiffuse;
uniform float u_env;
${LIGHT_COMMON}
out vec4 o;

vec3 envLight(vec3 R, float r) {
  float y = R.y;
  vec3 c = mix(u_ground, u_sky, smoothstep(-0.3, 0.5, y));
  float w = 0.03 + r * 0.55;
  c += u_keyCol * 0.55 * smoothstep(1.0 - w, 1.0 - w * 0.2, max(dot(R, u_keyDir), 0.0));
  c += u_fillCol * 0.9 * smoothstep(1.0 - w * 1.6, 1.0 - w * 0.3, max(dot(R, u_fillDir), 0.0));
  c += u_rimCol * 0.6 * smoothstep(1.0 - w * 1.6, 1.0 - w * 0.3, max(dot(R, u_rimDir), 0.0));
  c += vec3(0.07) * exp(-pow((y - 0.08) / (0.05 + r * 0.4), 2.0));
  return c;
}
vec3 aces(vec3 x) {
  return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
vec3 dirLight(vec3 L, vec3 C, vec3 N, vec3 V, vec3 diff, vec3 F0, float shin) {
  float ndl = max(dot(N, L), 0.0);
  vec3 H = normalize(L + V);
  float spec = (shin + 8.0) / 25.0 * pow(max(dot(N, H), 0.0), shin);
  vec3 F = F0 + (1.0 - F0) * pow(1.0 - max(dot(H, V), 0.0), 5.0);
  return C * ndl * (diff + F * spec);
}
void main() {
  for (int i = 0; i < 2; i++) {
    if (u_holes[i].w > 0.5 && distance(v_l.xy, u_holes[i].xy) < u_holes[i].z) discard;
  }
  vec4 tex = u_useMap > 0.5 ? texture(u_map, v_uv) : vec4(1.0);
  if (tex.a < u_alphaTest) discard;
  vec3 albedo = u_color * pow(tex.rgb, vec3(2.2));
  vec3 N = normalize(v_n);
  if (u_double > 0.5 && !gl_FrontFacing) N = -N;
  vec3 V = normalize(u_cam - v_w);
  float rough = clamp(u_rough, 0.05, 1.0);
  float r4 = rough * rough * rough * rough;
  float shin = clamp(2.0 / (r4 + 1e-4) - 2.0, 1.0, 2048.0);
  vec3 F0 = mix(vec3(0.04), albedo, u_metal);
  vec3 diff = albedo * (1.0 - u_metal);
  float sh = shadowAt(v_ls, 1.25);
  vec3 col = diff * mix(u_ground, u_sky, N.y * 0.5 + 0.5) * 1.6;
  col += dirLight(u_keyDir, u_keyCol * sh, N, V, diff, F0, shin);
  col += dirLight(u_fillDir, u_fillCol, N, V, diff, F0, shin);
  col += dirLight(u_rimDir, u_rimCol, N, V, diff, F0, shin);
  float ndv = max(dot(N, V), 0.0);
  vec3 Fe = F0 + (max(vec3(1.0 - rough), F0) - F0) * pow(1.0 - ndv, 5.0);
  col += envLight(reflect(-V, N), rough) * Fe * mix(1.0, 0.6, rough) * mix(0.6, 1.0, sh) * u_env;
  col += ledLights(v_w, N, diff, F0, shin, V);
  if (v_led >= 0) {
    float g = v_led < 20 ? u_zoneGain.x : (v_led < 38 ? u_zoneGain.y : u_zoneGain.z);
    vec3 e = u_led[v_led] * g;
    // Diffusoren: fargen slår igjennom, med en lysere kjerne når den er sterk.
    col = col * (1.0 - u_ledDiffuse * 0.6) + e * 2.4 + vec3(dot(e, vec3(0.3))) * 0.35;
  }
  col += u_highlight * u_accent * (0.10 + 0.55 * pow(1.0 - ndv, 2.0));
  col = aces(col * u_exposure);
  o = vec4(pow(col, vec3(1.0 / 2.2)), u_alpha);
}`;

const VS_DEPTH = `#version 300 es
layout(location=0) in vec3 a_pos;
layout(location=2) in vec2 a_uv;
uniform mat4 u_model;
uniform mat4 u_vp;
out vec3 v_l;
out vec2 v_uv;
void main() {
  v_l = a_pos;
  v_uv = a_uv;
  gl_Position = u_vp * u_model * vec4(a_pos, 1.0);
}`;
const FS_DEPTH = `#version 300 es
precision mediump float;
in vec3 v_l;
in vec2 v_uv;
uniform vec4 u_holes[2];
uniform sampler2D u_map;
uniform float u_useMap;
uniform float u_alphaTest;
uniform vec3 u_id;
out vec4 o;
void main() {
  for (int i = 0; i < 2; i++) {
    if (u_holes[i].w > 0.5 && distance(v_l.xy, u_holes[i].xy) < u_holes[i].z) discard;
  }
  if (u_useMap > 0.5 && texture(u_map, v_uv).a < u_alphaTest) discard;
  o = vec4(u_id, 1.0);
}`;

const VS_FLOOR = `#version 300 es
layout(location=0) in vec2 a_c;
uniform mat4 u_vp;
uniform mat4 u_lvp;
uniform vec3 u_center;
uniform float u_size;
out vec3 v_w;
out vec4 v_ls;
void main() {
  vec3 w = u_center + vec3(a_c.x * u_size, 0.0, a_c.y * u_size);
  v_w = w;
  v_ls = u_lvp * vec4(w, 1.0);
  gl_Position = u_vp * vec4(w, 1.0);
}`;
const FS_FLOOR = `#version 300 es
precision highp float;
in vec3 v_w;
uniform vec3 u_center;
uniform float u_size;
uniform float u_alpha;
${LIGHT_COMMON}
out vec4 o;
void main() {
  float s = shadowAt(v_ls, 4.0);
  float r = length(v_w.xz - u_center.xz) / u_size;
  float fade = 1.0 - smoothstep(0.18, 0.95, r);
  vec3 led = ledLights(v_w, vec3(0.0, 1.0, 0.0), vec3(0.35), vec3(0.04), 8.0, vec3(0.0, 1.0, 0.0));
  led = 1.0 - exp(-led * 1.4);
  float a = fade * u_alpha;
  // Premultiplisert: bare skyggen gjør gulvet mørkere, og LED-lyset gir en
  // svak farget flekk. Utenfor skyggen er gulvet lik bakgrunnen.
  float dark = (1.0 - s) * 0.42 * a;
  o = vec4(pow(led, vec3(1.0 / 2.2)) * 0.22 * a, dark);
}`;

const VS_QUAD = `#version 300 es
layout(location=0) in vec2 a_c;
out vec2 v_uv;
void main() {
  v_uv = a_c * 0.5 + 0.5;
  gl_Position = vec4(a_c, 0.0, 1.0);
}`;
const FS_AMBIENT = `#version 300 es
precision mediump float;
in vec2 v_uv;
uniform vec2 u_c;
uniform vec2 u_r;
uniform vec3 u_col;
uniform float u_alpha;
out vec4 o;
void main() {
  vec2 d = (v_uv - u_c) / u_r;
  float a = exp(-dot(d, d) * 2.2) * u_alpha;
  o = vec4(u_col * a, 0.0);
}`;

const VS_GLOW = `#version 300 es
layout(location=0) in vec2 a_c;
layout(location=1) in vec3 a_p;
layout(location=2) in vec3 a_col;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform float u_size;
out vec2 v_c;
out vec3 v_col;
void main() {
  vec4 vp = u_view * vec4(a_p, 1.0);
  vp.xy += a_c * u_size;
  vp.z += 3.0;
  v_c = a_c;
  v_col = a_col;
  gl_Position = u_proj * vp;
}`;
const FS_GLOW = `#version 300 es
precision mediump float;
in vec2 v_c;
in vec3 v_col;
uniform float u_gain;
out vec4 o;
void main() {
  float d2 = dot(v_c, v_c);
  float a = max(exp(-d2 * 3.2) - 0.04, 0.0);
  vec3 c = 1.0 - exp(-v_col * a * u_gain);
  o = vec4(c, 0.0);
}`;

function compile(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  return s;
}
/** Starter kompileringen uten å vente (status leses først i finalize). */
function program(gl, vs, fs) {
  const p = gl.createProgram();
  const v = compile(gl, gl.VERTEX_SHADER, vs);
  const f = compile(gl, gl.FRAGMENT_SHADER, fs);
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  return { p, v, f, u: null };
}
function finalize(gl, prog) {
  const { p, v, f } = prog;
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    const log = [gl.getShaderInfoLog(v), gl.getShaderInfoLog(f), gl.getProgramInfoLog(p)].filter(Boolean).join('\n');
    throw new Error(`Skyggeleggeren kompilerte ikke: ${log}`);
  }
  const u = {};
  const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS);
  for (let i = 0; i < n; i++) {
    const info = gl.getActiveUniform(p, i);
    u[info.name.replace(/\[0\]$/, '')] = gl.getUniformLocation(p, info.name);
  }
  prog.u = u;
  gl.deleteShader(v);
  gl.deleteShader(f);
}

const SHADOW = 2048;
const NO_HOLES = new Float32Array(8);

export class Renderer {
  constructor(canvas) {
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, depth: true, stencil: false, powerPreference: 'low-power', preserveDrawingBuffer: false });
    if (!gl) throw new Error('WebGL 2 er ikke tilgjengelig');
    this.gl = gl;
    this.canvas = canvas;
    // Skyggeleggerne kompileres parallelt når nettleseren kan det
    // (KHR_parallel_shader_compile), så siden ikke fryser mens driveren
    // jobber. isReady() spør uten å vente; render() tegner først når alt er klart.
    this.parallel = gl.getExtension('KHR_parallel_shader_compile');
    this.ready = false;
    this.prog = {
      main: program(gl, VS_MAIN, FS_MAIN),
      depth: program(gl, VS_DEPTH, FS_DEPTH),
      floor: program(gl, VS_FLOOR, FS_FLOOR),
      ambient: program(gl, VS_QUAD, FS_AMBIENT),
      glow: program(gl, VS_GLOW, FS_GLOW),
    };
    // Skyggekart: dybdetekstur med sammenligning (maskinvare-PCF).
    this.shadowTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.shadowTex);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.DEPTH_COMPONENT24, SHADOW, SHADOW);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_COMPARE_FUNC, gl.LEQUAL);
    this.shadowFbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.shadowFbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, this.shadowTex, 0);
    gl.drawBuffers([gl.NONE]);
    gl.readBuffer(gl.NONE);
    // Plukking: 1 × 1 piksel med farge og dybde.
    this.pickTex = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, this.pickTex);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.RGBA8, 1, 1);
    this.pickDepth = gl.createRenderbuffer();
    gl.bindRenderbuffer(gl.RENDERBUFFER, this.pickDepth);
    gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT24, 1, 1);
    this.pickFbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.pickFbo);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, this.pickTex);
    gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, this.pickDepth);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);

    // Firkant (−1…1) for gulv, bakgrunnslys og glorier.
    this.quadVao = gl.createVertexArray();
    gl.bindVertexArray(this.quadVao);
    this.quadBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    // Glorier: samme firkant, instansert med posisjon og farge per LED.
    this.glowVao = gl.createVertexArray();
    gl.bindVertexArray(this.glowVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    this.glowBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.glowBuf);
    gl.bufferData(gl.ARRAY_BUFFER, 56 * 6 * 4, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 24, 0);
    gl.vertexAttribDivisor(1, 1);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 3, gl.FLOAT, false, 24, 12);
    gl.vertexAttribDivisor(2, 1);
    gl.bindVertexArray(null);
    this.lvp = mat4.create();
    this.stats = { draws: 0, shadowPasses: 0, frames: 0 };
  }

  /** true når skyggeleggerne er ferdige (venter aldri hvis nettleseren kan kompilere parallelt). */
  isReady() {
    if (this.ready) return true;
    const gl = this.gl;
    if (this.parallel) {
      for (const pr of Object.values(this.prog)) if (!gl.getProgramParameter(pr.p, this.parallel.COMPLETION_STATUS_KHR)) return false;
    }
    for (const pr of Object.values(this.prog)) finalize(gl, pr);
    this.ready = true;
    return true;
  }

  createMesh(d) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const attr = (loc, data, size) => {
      const b = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
    };
    attr(0, d.position, 3);
    attr(1, d.normal, 3);
    attr(2, d.uv, 2);
    attr(3, d.led, 1);
    const ib = gl.createBuffer();
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, d.index, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    return { vao, count: d.count, type: d.index instanceof Uint32Array ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT, bounds: d.bounds, verts: d.position.length / 3 };
  }

  /**
   * Canvas → tekstur (RGBA8, mipmaps, anisotropi hvis tilgjengelig). Fargene
   * er sRGB og gjøres lineære i skyggeleggeren: SRGB8_ALPHA8 ga riktig
   * filtrering, men mipmap-genereringen tok ~0,7 s i ANGLE/OpenGL.
   */
  createTexture(src, { repeat = false } = {}) {
    const gl = this.gl;
    const t = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, src);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    const wrap = repeat ? gl.REPEAT : gl.CLAMP_TO_EDGE;
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, wrap);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, wrap);
    const an = gl.getExtension('EXT_texture_filter_anisotropic');
    if (an) gl.texParameterf(gl.TEXTURE_2D, an.TEXTURE_MAX_ANISOTROPY_EXT, Math.min(8, gl.getParameter(an.MAX_TEXTURE_MAX_ANISOTROPY_EXT)));
    return t;
  }

  resize(w, h) {
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
  }

  /** Lysets matrise: ortografisk fra hovedlysets retning, tilpasset scenens kule. */
  lightMatrix(keyDir, center, radius) {
    const eye = [center[0] + keyDir[0] * radius * 2, center[1] + keyDir[1] * radius * 2, center[2] + keyDir[2] * radius * 2];
    const up = Math.abs(keyDir[1]) > 0.95 ? [0, 0, 1] : [0, 1, 0];
    const view = mat4.lookAt(eye, center, up);
    const proj = mat4.ortho(-radius, radius, -radius, radius, radius * 0.5, radius * 3.5);
    return mat4.mul(this.lvp, proj, view);
  }

  setMaterial(u, m, tex0Unit = 0) {
    const gl = this.gl;
    if (u.u_color) gl.uniform3fv(u.u_color, m.color);
    if (u.u_rough) gl.uniform1f(u.u_rough, m.rough);
    if (u.u_metal) gl.uniform1f(u.u_metal, m.metal);
    if (u.u_double) gl.uniform1f(u.u_double, m.double ? 1 : 0);
    gl.uniform1f(u.u_useMap, m.map ? 1 : 0);
    if (u.u_alphaTest) gl.uniform1f(u.u_alphaTest, m.alphaTest || 0);
    if (m.map) {
      gl.activeTexture(gl.TEXTURE0 + tex0Unit);
      gl.bindTexture(gl.TEXTURE_2D, m.map);
    }
    if (u.u_ledDiffuse) gl.uniform1f(u.u_ledDiffuse, m.led ? 1 : 0);
  }

  /** Tegner ett bilde. f: se CardView.frame(). */
  render(f) {
    if (!this.isReady()) return false;
    const gl = this.gl;
    const W = this.canvas.width;
    const H = this.canvas.height;
    this.stats.frames++;
    this.stats.draws = 0;
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.disable(gl.CULL_FACE);

    // 1. Skyggekart
    if (f.shadowDirty) {
      this.stats.shadowPasses++;
      this.lightMatrix(f.lights.key, f.sceneCenter, f.sceneRadius);
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.shadowFbo);
      gl.viewport(0, 0, SHADOW, SHADOW);
      gl.clear(gl.DEPTH_BUFFER_BIT);
      gl.colorMask(false, false, false, false);
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(1.6, 3.0);
      const { p, u } = this.prog.depth;
      gl.useProgram(p);
      gl.uniformMatrix4fv(u.u_vp, false, this.lvp);
      gl.uniform1i(u.u_map, 0);
      for (const part of f.parts) {
        if (!part.cast || part.alpha < 0.5 || !part.visible) continue;
        gl.uniformMatrix4fv(u.u_model, false, part.model);
        gl.uniform4fv(u.u_holes, part.mat.holes || NO_HOLES);
        this.setMaterial(u, part.mat);
        gl.bindVertexArray(part.mesh.vao);
        gl.drawElements(gl.TRIANGLES, part.mesh.count, part.mesh.type, 0);
      }
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.colorMask(true, true, true, true);
    }

    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, W, H);
    const bg = f.bg;
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    // 2. Bakgrunnslys
    if (f.ambient && f.ambient.alpha > 0.002) {
      const { p, u } = this.prog.ambient;
      gl.useProgram(p);
      gl.disable(gl.DEPTH_TEST);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE);
      gl.uniform2fv(u.u_c, f.ambient.center);
      gl.uniform2fv(u.u_r, f.ambient.radius);
      gl.uniform3fv(u.u_col, f.ambient.color);
      gl.uniform1f(u.u_alpha, f.ambient.alpha);
      gl.bindVertexArray(this.quadVao);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      gl.disable(gl.BLEND);
      gl.enable(gl.DEPTH_TEST);
    }

    // 3. Delene
    const { p, u } = this.prog.main;
    gl.useProgram(p);
    gl.uniformMatrix4fv(u.u_vp, false, f.viewProj);
    gl.uniformMatrix4fv(u.u_lvp, false, this.lvp);
    gl.uniform3fv(u.u_cam, f.camPos);
    const L = f.lights;
    gl.uniform3fv(u.u_keyDir, L.key);
    gl.uniform3fv(u.u_keyCol, L.keyCol);
    gl.uniform3fv(u.u_fillDir, L.fill);
    gl.uniform3fv(u.u_fillCol, L.fillCol);
    gl.uniform3fv(u.u_rimDir, L.rim);
    gl.uniform3fv(u.u_rimCol, L.rimCol);
    gl.uniform3fv(u.u_sky, L.sky);
    gl.uniform3fv(u.u_ground, L.ground);
    gl.uniform1f(u.u_exposure, L.exposure);
    gl.uniform1f(u.u_env, L.env);
    gl.uniform3fv(u.u_accent, f.accent);
    gl.uniform3fv(u.u_led, f.leds.colors);
    gl.uniform3fv(u.u_zoneGain, f.leds.zoneGain);
    this.setLedLights(u, f.leds);
    gl.uniform1i(u.u_map, 0);
    gl.uniform1i(u.u_shadow, 1);
    gl.uniform1f(u.u_shadowTexel, 1 / SHADOW);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.shadowTex);
    gl.activeTexture(gl.TEXTURE0);

    const drawPart = (part) => {
      gl.uniformMatrix4fv(u.u_model, false, part.model);
      gl.uniformMatrix3fv(u.u_nmat, false, part.nmat);
      gl.uniform4fv(u.u_holes, part.mat.holes || NO_HOLES);
      gl.uniform1f(u.u_alpha, part.alpha);
      gl.uniform1f(u.u_highlight, part.highlight || 0);
      this.setMaterial(u, part.mat);
      gl.bindVertexArray(part.mesh.vao);
      gl.drawElements(gl.TRIANGLES, part.mesh.count, part.mesh.type, 0);
      this.stats.draws++;
    };
    const fading = [];
    for (const part of f.parts) {
      if (!part.visible || part.alpha <= 0.004) continue;
      if (part.alpha < 0.999) fading.push(part);
      else drawPart(part);
    }

    // 4. Gulvet
    if (f.floor && f.floor.alpha > 0.01) {
      const fp = this.prog.floor;
      gl.useProgram(fp.p);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);
      gl.uniformMatrix4fv(fp.u.u_vp, false, f.viewProj);
      gl.uniformMatrix4fv(fp.u.u_lvp, false, this.lvp);
      gl.uniform3fv(fp.u.u_center, f.floor.center);
      gl.uniform1f(fp.u.u_size, f.floor.size);
      gl.uniform1f(fp.u.u_alpha, f.floor.alpha);
      gl.uniform1i(fp.u.u_shadow, 1);
      gl.uniform1f(fp.u.u_shadowTexel, 1 / SHADOW);
      this.setLedLights(fp.u, f.leds);
      gl.bindVertexArray(this.quadVao);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
    }

    // 5. Lag som tones inn eller ut: bakerst først.
    if (fading.length) {
      fading.sort((a, b) => b.depth - a.depth);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      for (const part of fading) drawPart(part);
      gl.disable(gl.BLEND);
    }

    // 6. Glorier
    if (f.glow && f.glow.count) {
      const gp = this.prog.glow;
      gl.useProgram(gp.p);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE);
      gl.depthMask(false);
      gl.uniformMatrix4fv(gp.u.u_view, false, f.view);
      gl.uniformMatrix4fv(gp.u.u_proj, false, f.proj);
      gl.uniform1f(gp.u.u_size, f.glow.size);
      gl.uniform1f(gp.u.u_gain, f.glow.gain);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.glowBuf);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, f.glow.data, 0, f.glow.count * 6);
      gl.bindVertexArray(this.glowVao);
      gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, f.glow.count);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
    }
    gl.bindVertexArray(null);
    return true;
  }

  setLedLights(u, leds) {
    const gl = this.gl;
    gl.uniform3fv(u.u_barA, leds.barA);
    gl.uniform3fv(u.u_barB, leds.barB);
    gl.uniform3fv(u.u_barCol, leds.barCol);
    gl.uniform3fv(u.u_f1Pos, leds.f1Pos);
    gl.uniform3fv(u.u_f1Col, leds.f1Col);
    gl.uniform3fv(u.u_f2Pos, leds.f2Pos);
    gl.uniform3fv(u.u_f2Col, leds.f2Col);
  }

  /**
   * Hvilken del ligger under pikselen (px, py) i lerretets piksler?
   * Tegner bare den ene pikselen: projeksjonen forstørres rundt punktet.
   */
  pick(f, px, py) {
    if (!this.isReady()) return 0;
    const gl = this.gl;
    const W = this.canvas.width;
    const H = this.canvas.height;
    // NDC for pikselens sentrum, og en matrise som gjør den pikselen til hele visningen.
    const nx = ((px + 0.5) / W) * 2 - 1;
    const ny = 1 - ((py + 0.5) / H) * 2;
    const pickM = mat4.create();
    pickM[0] = W;
    pickM[5] = H;
    pickM[12] = -nx * W;
    pickM[13] = -ny * H;
    const vp = mat4.mul(mat4.create(), pickM, f.viewProj);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.pickFbo);
    gl.viewport(0, 0, 1, 1);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    const { p, u } = this.prog.depth;
    gl.useProgram(p);
    gl.uniformMatrix4fv(u.u_vp, false, vp);
    gl.uniform1i(u.u_map, 0);
    for (const part of f.parts) {
      if (!part.visible || part.alpha < 0.5 || !part.pickId) continue;
      gl.uniformMatrix4fv(u.u_model, false, part.model);
      gl.uniform4fv(u.u_holes, part.mat.holes || NO_HOLES);
      gl.uniform3f(u.u_id, (part.pickId & 255) / 255, ((part.pickId >> 8) & 255) / 255, 0);
      this.setMaterial(u, part.mat);
      gl.bindVertexArray(part.mesh.vao);
      gl.drawElements(gl.TRIANGLES, part.mesh.count, part.mesh.type, 0);
    }
    const px4 = new Uint8Array(4);
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px4);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.bindVertexArray(null);
    return px4[3] ? px4[0] + (px4[1] << 8) : 0;
  }
}
