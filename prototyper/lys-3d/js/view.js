// CardView: 3D-visningen av skjermkortet. Eier rendereren, modellen,
// kameraet, lagene (ta av / eksplodert), viftene og input (mus, berøring,
// tastatur). Implementerer LightTarget (se engine.js), så effektmotoren kan
// skrive fargene rett inn.
//
// Tegner bare når noe endrer seg: input, lag som flytter seg, kamera som
// glir, effekter som beveger seg eller vifter som snurrer. Ellers sover
// løkka. Ingenting tegnes mens fanen er skjult eller visningen er utenfor
// skjermen. Løpende animasjoner (vifter, effekter) tegnes høyst ~60 ganger i
// sekundet, også på skjermer med høyere frekvens; input tegnes straks.

import { Renderer } from './gl/renderer.js';
import { mat4, DEG, clamp, lerp, smoothstep, srgbToLinear, hexToLinear, easeInOut } from './gl/math.js';
import { buildCard, LAYERS, GROUPS, GROUP_IDS } from './card-model.js';
import { makeAtlas, makePcbFront, makePcbBack, makeBracket, makeBackplate, makeFinStripe } from './textures.js';
import { COOLER } from './layout.js';

export const VIEWS = {
  forfra: { yaw: 0, pitch: 4, name: 'Forfra' },
  skraatt: { yaw: -33, pitch: 18, name: 'Skrått' },
  ovenfra: { yaw: -8, pitch: 62, name: 'Ovenfra' },
  bakfra: { yaw: 155, pitch: 14, name: 'Bakfra' },
};
export const STEPS = ['Hel', 'Uten deksel', 'Uten kjøler', 'Bare kretskort'];
const STEP_TEXT = [
  'Alle deler er på: deksel med vifter og lys, kjøler, kretskort, brakett og bakplate.',
  'Dekselet og viftene er tatt av, så kjøleribben og varmerørene synes.',
  'Kjøleren er også tatt av. Kretskortet synes med brakett og bakplate.',
  'Bare kretskortet: GPU-brikken, åtte minnebrikker, strømfaser, kontakter og utganger.',
];
const FOV = 26 * DEG;
const RATE_MAX = 1.6; // omdreininger per sekund ved 100 % (som rgb.js: (60 + 520) / 360)
const ZONE_OF_GROUP = { bar: 'bar', ring1: 'fan1', ring2: 'fan2' };
const smoothTo = (cur, target, dt, tau) => cur + (target - cur) * (1 - Math.exp(-dt / tau));

let texCache = null;
function textures() {
  // Teksturene tegnes én gang per side (også ved tapt og gjenopprettet kontekst).
  if (!texCache) {
    const t0 = performance.now();
    texCache = { atlas: makeAtlas(), pcbFront: makePcbFront(), pcbBack: makePcbBack(), bracket: makeBracket(), backplate: makeBackplate(), finStripe: makeFinStripe(COOLER.fins.t / COOLER.fins.pitch) };
    texCache.ms = performance.now() - t0;
  }
  return texCache;
}

export class CardView {
  /**
   * host: elementet visningen fyller (får et <canvas>). opts:
   *   motion(): 'full' | 'reduced' | 'off'
   *   onHover(info | null, x, y): delen under pekeren (navn, beskrivelse, sone)
   *   onZone(zone): en lyssone ble klikket
   *   onChange(): kamera, lag eller vifter endret seg (for beskrivelsen)
   */
  constructor(host, opts = {}) {
    this.host = host;
    this.opts = opts;
    this.motion = opts.motion || (() => 'full');
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'gl';
    this.canvas.setAttribute('aria-hidden', 'true');
    host.prepend(this.canvas);
    this.tickers = new Set();
    this.running = false;
    this.visible = document.visibilityState === 'visible';
    this.onScreen = true;
    this.raf = 0;
    this.dirty = true;
    this.lastDraw = 0;
    this.lastNow = 0;
    // Kamera: nå-verdier og mål (glir mot målet, unntatt ved dra).
    this.cam = { yaw: VIEWS.skraatt.yaw, pitch: VIEWS.skraatt.pitch, zoom: 1, dist: 700, target: [0, 0, 0] };
    this.goal = { yaw: this.cam.yaw, pitch: this.cam.pitch, zoom: 1, dist: 700, target: [0, 0, 0] };
    this.vel = 0; // treghet etter dra (grader per sekund)
    this.step = 0;
    this.explode = 0;
    this.explodeGoal = 0;
    this.fanPct = 35;
    this.fanRate = 0;
    this.fanAngle = [0.3, 1.9];
    this.leds = new Float32Array(56 * 3); // sRGB 0–255 fra motoren
    this.ledLin = new Float32Array(56 * 3);
    this.layerK = 1;
    this.dimZone = 'all';
    this.ambientOn = false;
    this.ambient = { color: [0, 0, 0], a: 0 };
    this.hoverGroup = null;
    this.focusGroup = null;
    this.pendingPick = null;
    this.stats = { frames: 0, drawMs: [], lastDrawMs: 0, textureMs: 0, buildMs: 0 };
    // Vern mot treg grafikk (f.eks. programvare-GPU): først lavere oppløsning,
    // så stillbilde for løpende animasjoner. opts.adapt === false slår det av.
    this.scale = 1;
    this.slow = false;
    this.slowEma = 0;
    this.slowN = 0;
    this.adaptAfter = Infinity; // settes når det første bildet er tegnet (oppvarming først)
    this.layerState = Object.fromEntries(LAYERS.map((l) => [l.id, { out: 0, goal: 0, delay: 0 }]));

    this.init();
    this.bindInput();
    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(host);
    this.io = new IntersectionObserver(([e]) => {
      this.onScreen = e.isIntersecting;
      this.kick();
    });
    this.io.observe(host);
    this.onVis = () => {
      this.visible = document.visibilityState === 'visible';
      this.kick();
    };
    document.addEventListener('visibilitychange', this.onVis);
    this.canvas.addEventListener('webglcontextlost', (e) => {
      e.preventDefault();
      this.lost = true;
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    });
    this.canvas.addEventListener('webglcontextrestored', () => {
      this.lost = false;
      this.init();
      this.dirty = true;
      this.kick();
    });
    this.resize();
    this.fit(true);
  }

  init() {
    const t0 = performance.now();
    this.r = new Renderer(this.canvas);
    const t1 = performance.now();
    const src = textures();
    this.stats.textureMs = src.ms;
    const tex = {};
    for (const k of ['atlas', 'pcbFront', 'pcbBack', 'bracket', 'backplate']) tex[k] = this.r.createTexture(src[k]);
    tex.finStripe = this.r.createTexture(src.finStripe, { repeat: true });
    const t2 = performance.now();
    const card = buildCard(tex);
    const t3 = performance.now();
    this.stats.parts = { shaders: t1 - t0, upload: t2 - t1 - (texCache.counted ? 0 : src.ms), geometry: t3 - t2 };
    texCache.counted = true;
    this.card = card;
    this.center = card.center;
    this.parts = card.parts.map((p) => ({
      ...p,
      mesh: this.r.createMesh(p.data),
      model: mat4.create(),
      nmat: new Float32Array(9),
      alpha: 1,
      visible: true,
      pickId: GROUP_IDS.indexOf(p.group) + 1,
      highlight: 0,
      depth: 0,
    }));
    for (const p of this.parts) delete p.data;
    // Rammen rundt hvert lag (for kameraet).
    this.layerBounds = {};
    for (const p of this.parts) {
      const b = p.mesh.bounds;
      let min = b.min;
      let max = b.max;
      if (p.rotor >= 0) {
        const [cx, cy] = card.fanCenters[p.rotor];
        min = [min[0] + cx, min[1] + cy, min[2]];
        max = [max[0] + cx, max[1] + cy, max[2]];
      }
      const lb = (this.layerBounds[p.layer] ||= { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] });
      for (let k = 0; k < 3; k++) {
        lb.min[k] = Math.min(lb.min[k], min[k]);
        lb.max[k] = Math.max(lb.max[k], max[k]);
      }
    }
    this.verts = this.parts.reduce((s, p) => s + p.mesh.verts, 0);
    this.tris = this.parts.reduce((s, p) => s + p.mesh.count / 3, 0);
    this.shadowDirty = true;
    this.stats.buildMs = performance.now() - t0;
    this.stats.parts.gpuBuffers = performance.now() - t3;
  }

  // ------------------------------------------------------------------ LightTarget
  setZoneColor(zone, rgb) {
    const [a, b] = { bar: [0, 20], fan1: [20, 38], fan2: [38, 56] }[zone];
    for (let i = a; i < b; i++) this.leds.set(rgb, i * 3);
  }
  setLedColors(zone, arr) {
    const a = { bar: 0, fan1: 20, fan2: 38 }[zone];
    this.leds.set(arr, a * 3);
  }
  setLayerOpacity(k) {
    this.layerK = k;
  }
  setDimmedZones(zone) {
    this.dimZone = zone || 'all';
    this.dirty = true;
  }
  setAmbient(rgb, a) {
    if (rgb) this.ambient.color = rgb.map((v) => v / 255);
    this.ambient.a = rgb ? a : 0;
  }
  commit() {
    for (let i = 0; i < this.leds.length; i++) this.ledLin[i] = srgbToLinear(this.leds[i] / 255);
    this.dirty = true;
    this.kick();
  }

  // ------------------------------------------------------------------ Styring
  setAmbientEnabled(on) {
    this.ambientOn = !!on;
    this.dirty = true;
    this.kick();
  }
  /** Lagtrinn 0–3. Lagene glir av (eller på) etter tur. */
  setStep(n) {
    n = clamp(Math.round(n), 0, 3);
    if (n === this.step) return;
    const up = n > this.step;
    this.step = n;
    const m = this.motion();
    // Trinnvis: det ytterste laget først når lag tas av, det innerste først når de settes på.
    const changing = LAYERS.filter((l) => (this.layerState[l.id].goal === 1) !== (n >= l.step));
    const order = [...new Set(changing.map((l) => l.step))].sort((a, b) => (up ? a - b : b - a));
    for (const l of changing) {
      const s = this.layerState[l.id];
      s.goal = n >= l.step ? 1 : 0;
      s.delay = m === 'full' ? order.indexOf(l.step) * 0.14 : 0;
      if (m === 'off') s.out = s.goal;
    }
    this.fitSoon = true;
    this.change();
  }
  setExplode(e, { follow = false } = {}) {
    this.explodeGoal = clamp(e, 0, 1);
    if (follow || this.motion() !== 'full') this.explode = this.explodeGoal;
    this.shadowDirty = true;
    this.fitSoon = true;
    this.change();
  }
  setFanSpeed(pct) {
    this.fanPct = clamp(pct, 0, 100);
    this.kick();
  }
  setView(name) {
    const v = VIEWS[name];
    if (!v) return;
    this.goal.yaw = this.cam.yaw + wrap(v.yaw - this.cam.yaw);
    this.goal.pitch = v.pitch;
    this.goal.zoom = 1;
    this.vel = 0;
    this.fit(false, v);
    this.jumpIfStill();
    this.change();
  }
  resetView() {
    this.setView('skraatt');
  }
  rotateBy(dyaw, dpitch) {
    this.goal.yaw += dyaw;
    this.goal.pitch = clamp(this.goal.pitch + dpitch, -60, 85);
    this.vel = 0;
    this.jumpIfStill();
    this.change();
  }
  zoomBy(f) {
    this.goal.zoom = clamp(this.goal.zoom * f, 0.6, 3.2);
    this.jumpIfStill();
    this.change();
  }
  setFocusGroup(g) {
    this.focusGroup = g;
    this.dirty = true;
    this.kick();
  }
  /** Uten animasjoner hopper kameraet rett til målet. */
  jumpIfStill() {
    if (this.motion() !== 'full') {
      Object.assign(this.cam, { yaw: this.goal.yaw, pitch: this.goal.pitch, zoom: this.goal.zoom, dist: this.goal.dist, target: [...this.goal.target] });
      this.shadowDirty = true;
    }
  }
  change() {
    this.dirty = true;
    this.kick();
    this.opts.onChange?.();
  }
  setRunning(on) {
    this.running = on;
    this.kick();
  }
  addTicker(fn) {
    this.tickers.add(fn);
    this.kick();
    return () => this.tickers.delete(fn);
  }

  // ------------------------------------------------------------------ Kamera og rammer
  layerOffset(l, out = this.layerState[l.id].out) {
    const m = this.motion();
    const spread = Math.abs(l.spread) * this.explode;
    // Når laget tas av, løftes det ut langs retningen og litt opp (ikke ved redusert bevegelse).
    const lift = m === 'full' ? easeInOut(clamp(out, 0, 1)) : 0;
    const d = spread + lift * 150;
    return [l.dir[0] * d, l.dir[1] * d + lift * 40, l.dir[2] * d];
  }
  layerAlpha(l) {
    const out = this.layerState[l.id].out;
    return this.motion() === 'full' ? 1 - smoothstep(0.3, 0.95, out) : 1 - out;
  }
  /** Rammen rundt lagene som er på (mål-tilstanden), relativt til kortets sentrum. */
  visibleBounds(goal = true) {
    const min = [Infinity, Infinity, Infinity];
    const max = [-Infinity, -Infinity, -Infinity];
    for (const l of LAYERS) {
      const s = this.layerState[l.id];
      if ((goal ? s.goal : s.out) >= 0.5) continue;
      const b = this.layerBounds[l.id];
      if (!b) continue;
      const spread = Math.abs(l.spread) * (goal ? this.explodeGoal : this.explode);
      const off = [l.dir[0] * spread, l.dir[1] * spread, l.dir[2] * spread];
      for (let k = 0; k < 3; k++) {
        min[k] = Math.min(min[k], b.min[k] + off[k] - this.center[k]);
        max[k] = Math.max(max[k], b.max[k] + off[k] - this.center[k]);
      }
    }
    return { min, max };
  }
  /** Avstand og sentrum som får de synlige lagene til å fylle visningen fra en gitt vinkel. */
  fit(jump = false, at = null) {
    const { min, max } = this.visibleBounds(true);
    const target = [(min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2];
    const yaw = (at?.yaw ?? this.goal.yaw) * DEG;
    const pitch = (at?.pitch ?? this.goal.pitch) * DEG;
    const aspect = this.canvas.width / Math.max(1, this.canvas.height) || 1.6;
    const ty = Math.tan(FOV / 2);
    const tx = ty * aspect;
    const fwd = [Math.cos(pitch) * Math.sin(yaw), Math.sin(pitch), Math.cos(pitch) * Math.cos(yaw)];
    const right = [Math.cos(yaw), 0, -Math.sin(yaw)];
    const up = [-Math.sin(pitch) * Math.sin(yaw), Math.cos(pitch), -Math.sin(pitch) * Math.cos(yaw)];
    let dist = 0;
    const margin = 0.86;
    for (let i = 0; i < 8; i++) {
      const p = [i & 1 ? max[0] : min[0], i & 2 ? max[1] : min[1], i & 4 ? max[2] : min[2]];
      const d = [p[0] - target[0], p[1] - target[1], p[2] - target[2]];
      const x = d[0] * right[0] + d[2] * right[2];
      const y = d[0] * up[0] + d[1] * up[1] + d[2] * up[2];
      const z = d[0] * fwd[0] + d[1] * fwd[1] + d[2] * fwd[2];
      dist = Math.max(dist, Math.abs(x) / (tx * margin) + z, Math.abs(y) / (ty * margin) + z);
    }
    this.goal.dist = dist;
    this.goal.target = target;
    if (jump || this.motion() !== 'full') {
      this.cam.dist = dist;
      this.cam.target = [...target];
    }
  }

  resize() {
    const r = this.host.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2) * this.scale;
    const w = Math.max(1, Math.round(r.width * dpr));
    const h = Math.max(1, Math.round(r.height * dpr));
    if (this.r && (w !== this.canvas.width || h !== this.canvas.height)) {
      this.r.resize(w, h);
      this.fit(true);
      this.dirty = true;
      this.kick();
    }
  }

  // ------------------------------------------------------------------ Løkka
  kick() {
    if (this.raf || !this.running || !this.visible || !this.onScreen || this.lost) return;
    this.raf = requestAnimationFrame((t) => this.frame(t));
  }
  /** Animasjonsnivået visningen faktisk bruker: «full» blir «redusert» hvis grafikken er for treg. */
  effectiveMotion() {
    const m = this.motion();
    return m === 'full' && this.slow ? 'reduced' : m;
  }
  /** Måler tiden mellom bildene under løpende animasjon og senker kvaliteten om den er for lang. */
  watchSpeed(interval, steady) {
    if (this.opts.adapt === false || this.slow || !steady || performance.now() < this.adaptAfter) {
      this.slowN = 0;
      this.slowEma = 0;
      return;
    }
    this.slowEma = this.slowEma ? this.slowEma + (interval - this.slowEma) * 0.25 : interval;
    if (++this.slowN < 12 || this.slowEma < 50) return;
    this.slowN = 0;
    this.slowEma = 0;
    if (this.scale > 0.5) {
      this.scale = Math.max(0.5, this.scale * 0.7);
      this.resize();
    } else {
      this.slow = true;
      this.opts.onSlow?.();
    }
  }
  frame(now) {
    this.raf = 0;
    const interval = this.lastNow ? now - this.lastNow : 16;
    // Effekter og vifter bruker høyst 0,1 s per bilde (som appens onFrame);
    // lag og kamera bruker den ekte tiden, så de blir ferdige i tide også når bildene er få.
    const dt = this.lastNow ? Math.min(0.1, interval / 1000) : 1 / 60;
    const dtReal = this.lastNow ? Math.min(0.5, interval / 1000) : 1 / 60;
    this.lastNow = now;
    const m = this.effectiveMotion();
    let busy = false;

    for (const t of this.tickers) if (t(dt)) busy = true;

    // Lag
    let layersMoving = false;
    for (const l of LAYERS) {
      const s = this.layerState[l.id];
      if (s.out === s.goal) continue;
      if (s.delay > 0) {
        s.delay -= dtReal;
        layersMoving = true;
        continue;
      }
      const durS = m === 'full' ? 0.62 : m === 'reduced' ? 0.12 : 0;
      s.out = durS ? (s.goal > s.out ? Math.min(s.goal, s.out + dtReal / durS) : Math.max(s.goal, s.out - dtReal / durS)) : s.goal;
      layersMoving = true;
    }
    if (Math.abs(this.explode - this.explodeGoal) > 1e-4) {
      this.explode = m === 'full' ? smoothTo(this.explode, this.explodeGoal, dtReal, 0.12) : this.explodeGoal;
      layersMoving = true;
    } else this.explode = this.explodeGoal;
    if (this.fitSoon) {
      this.fitSoon = false;
      this.fit(false);
    }
    if (layersMoving) {
      busy = true;
      this.shadowDirty = true;
    }

    // Kamera: treghet etter dra, og glidning mot målet.
    const c = this.cam;
    const g = this.goal;
    if (this.vel && m === 'full' && !this.drag) {
      g.yaw += this.vel * dt;
      c.yaw = g.yaw;
      this.vel *= Math.exp(-dt / 0.28);
      if (Math.abs(this.vel) < 2) this.vel = 0;
      this.shadowDirty = true;
      busy = true;
    }
    const camMoving = Math.abs(c.yaw - g.yaw) > 0.01 || Math.abs(c.pitch - g.pitch) > 0.01 || Math.abs(c.zoom - g.zoom) > 1e-4 || Math.abs(c.dist - g.dist) > 0.05 || c.target.some((v, i) => Math.abs(v - g.target[i]) > 0.02);
    if (camMoving) {
      if (m === 'full' && !this.drag) {
        const tau = 0.11;
        const yawBefore = c.yaw;
        c.yaw = smoothTo(c.yaw, g.yaw, dtReal, tau);
        c.pitch = smoothTo(c.pitch, g.pitch, dtReal, tau);
        c.zoom = smoothTo(c.zoom, g.zoom, dtReal, 0.09);
        c.dist = smoothTo(c.dist, g.dist, dtReal, 0.16);
        c.target = c.target.map((v, i) => smoothTo(v, g.target[i], dtReal, 0.16));
        if (Math.abs(c.yaw - yawBefore) > 1e-3) this.shadowDirty = true;
      } else {
        if (c.yaw !== g.yaw) this.shadowDirty = true;
        Object.assign(c, { yaw: g.yaw, pitch: g.pitch, zoom: g.zoom, dist: g.dist, target: [...g.target] });
      }
      busy = true;
    }

    // Vifter: farten glir (treghet), og de står stille ved redusert eller ingen bevegelse.
    const fansShown = this.layerState.vifter.out < 1;
    const targetRate = m === 'full' && this.fanPct > 0 ? (60 + (this.fanPct / 100) * 520) / 360 : 0;
    this.fanRate = m === 'full' ? smoothTo(this.fanRate, targetRate, dt, 0.7) : 0;
    if (Math.abs(this.fanRate - targetRate) < (targetRate ? 0.002 : 0.03)) this.fanRate = targetRate;
    if (this.fanRate > 0 && fansShown) {
      for (let i = 0; i < 2; i++) this.fanAngle[i] -= this.fanRate * Math.PI * 2 * dt * (i ? 0.985 : 1);
      busy = true;
    }

    // Tegn: alltid ved endring fra input, ellers høyst ~60 bilder i sekundet.
    const due = now - this.lastDraw >= 15.5;
    if ((this.dirty || busy) && (this.dirty || due)) {
      if (this.draw(now) === false) busy = true;
      else this.dirty = false;
    }
    if (this.pendingPick && !this.drag) this.doPick();
    this.watchSpeed(interval, busy && !this.drag && !layersMoving && !camMoving && !this.waiting);
    if (busy || this.dirty) this.kick();
    else this.lastNow = 0;
  }

  frameData() {
    const c = this.cam;
    const W = this.canvas.width;
    const H = this.canvas.height;
    const yaw = c.yaw * DEG;
    const pitch = c.pitch * DEG;
    const dist = c.dist / c.zoom;
    const t = c.target;
    const eye = [t[0] + dist * Math.cos(pitch) * Math.sin(yaw), t[1] + dist * Math.sin(pitch), t[2] + dist * Math.cos(pitch) * Math.cos(yaw)];
    const view = mat4.lookAt(eye, t, [0, 1, 0]);
    const R = 190;
    const proj = mat4.perspective(FOV, W / Math.max(1, H), Math.max(4, dist - R * 1.6), dist + R * 2.4);
    const viewProj = mat4.mul(mat4.create(), proj, view);

    // Lys: følger kameraets dreining, så kortet alltid er lyst forfra-oppe-venstre.
    const rot = (v) => {
      const cs = Math.cos(yaw);
      const sn = Math.sin(yaw);
      return [v[0] * cs + v[2] * sn, v[1], -v[0] * sn + v[2] * cs];
    };
    const norm = (v) => {
      const l = Math.hypot(...v);
      return v.map((x) => x / l);
    };
    const lights = {
      key: rot(norm([-0.45, 0.78, 0.68])),
      keyCol: [2.0, 1.96, 1.88],
      fill: rot(norm([0.8, 0.22, 0.55])),
      fillCol: [0.5, 0.52, 0.57],
      rim: rot(norm([0.2, 0.55, -0.85])),
      rimCol: [0.75, 0.8, 0.9],
      sky: [0.1, 0.105, 0.12],
      ground: [0.028, 0.029, 0.033],
      exposure: 1.0,
      env: 2.0,
    };

    // Lag og deler
    const offsets = {};
    const alphas = {};
    for (const l of LAYERS) {
      offsets[l.id] = this.layerOffset(l);
      alphas[l.id] = this.layerAlpha(l);
    }
    const hl = this.hoverGroup || this.focusGroup;
    // Nivådetaljer for lamellene: hvor mange piksler er én lamellavstand nå?
    const pxPerMm = H / (2 * dist * Math.tan(FOV / 2));
    const pitchPx = COOLER.fins.pitch * pxPerMm;
    // Detaljerte lameller først når de er store nok til å ikke gi moaré (≈ 9 px per periode).
    const near = this.finsNear ? pitchPx > 8.5 : pitchPx > 9.5;
    if (near !== this.finsNear) {
      this.finsNear = near;
      this.shadowDirty = true;
    }
    for (const p of this.parts) {
      const off = offsets[p.layer];
      const base = mat4.translation(off[0] - this.center[0], off[1] - this.center[1], off[2] - this.center[2]);
      if (p.rotor >= 0) {
        const [fx, fy] = this.card.fanCenters[p.rotor];
        mat4.mul(p.model, base, mat4.mul(mat4.create(), mat4.translation(fx, fy, 0), mat4.rotZ(this.fanAngle[p.rotor])));
      } else p.model.set(base);
      mat4.normal3(p.model, p.nmat);
      p.alpha = alphas[p.layer];
      p.visible = p.alpha > 0.004 && (!p.lod || (p.lod === 'near') === near);
      p.highlight = hl && p.group === hl ? 1 : 0;
      // Dybde for sortering av lag som tones: avstand fra kameraet til lagets sentrum.
      const lb = this.layerBounds[p.layer];
      const cz = [(lb.min[0] + lb.max[0]) / 2 + off[0] - this.center[0], (lb.min[1] + lb.max[1]) / 2 + off[1] - this.center[1], (lb.min[2] + lb.max[2]) / 2 + off[2] - this.center[2]];
      p.depth = Math.hypot(cz[0] - eye[0], cz[1] - eye[1], cz[2] - eye[2]);
    }

    // LED-ene: farge (lineær), sonedemping, pust og lagets synlighet.
    const dA = alphas.deksel;
    const dOff = offsets.deksel;
    const zoneGain = ['bar', 'fan1', 'fan2'].map((z) => (this.dimZone === 'all' || this.dimZone === z ? 1 : 0.3) * this.layerK);
    const toWorld = (p) => [p[0] + dOff[0] - this.center[0], p[1] + dOff[1] - this.center[1], p[2] + dOff[2] - this.center[2]];
    const avg = (a, b, gain) => {
      const s = [0, 0, 0];
      for (let i = a; i < b; i++) for (let k = 0; k < 3; k++) s[k] += this.ledLin[i * 3 + k];
      return s.map((v) => (v / (b - a)) * gain);
    };
    const L = this.card.ledLights;
    const leds = {
      colors: this.ledLin,
      zoneGain,
      barA: toWorld(L.barA),
      barB: toWorld(L.barB),
      barCol: avg(0, 20, zoneGain[0] * dA * 3.2),
      f1Pos: toWorld(L.fan1),
      f1Col: avg(20, 38, zoneGain[1] * dA * 1.8),
      f2Pos: toWorld(L.fan2),
      f2Col: avg(38, 56, zoneGain[2] * dA * 1.8),
    };
    // Glorier
    this.glowData ||= new Float32Array(56 * 6);
    const gd = this.glowData;
    const lp = this.card.ledPos;
    for (let i = 0; i < 56; i++) {
      const w = toWorld(lp[i].p);
      const gz = zoneGain[i < 20 ? 0 : i < 38 ? 1 : 2] * dA;
      gd.set(w, i * 6);
      gd[i * 6 + 3] = this.ledLin[i * 3] * gz;
      gd[i * 6 + 4] = this.ledLin[i * 3 + 1] * gz;
      gd[i * 6 + 5] = this.ledLin[i * 3 + 2] * gz;
    }

    // Scenens kule (for skyggekartet) og gulvet under kortet.
    const vb = this.visibleBounds(false);
    const sc = [(vb.min[0] + vb.max[0]) / 2, (vb.min[1] + vb.max[1]) / 2, (vb.min[2] + vb.max[2]) / 2];
    const sr = Math.max(60, Math.hypot(vb.max[0] - vb.min[0], vb.max[1] - vb.min[1], vb.max[2] - vb.min[2]) / 2) * 1.02;
    const floorY = -this.center[1] - 26;
    const floorAlpha = smoothstep(4, 70, eye[1] - floorY);

    // Bakgrunnslys: kortets sentrum på skjermen.
    const cp = mat4.transformPoint(viewProj, t);
    const amb = this.ambientOn && this.ambient.a > 0 ? { center: [cp[0] * 0.5 + 0.5, cp[1] * 0.5 + 0.5], radius: [0.34, 0.5], color: this.ambient.color, alpha: this.ambient.a * 0.42 } : null;

    return {
      parts: this.parts,
      view,
      proj,
      viewProj,
      camPos: eye,
      lights,
      leds,
      accent: this.accent || hexToLinear('#559cf7'),
      glow: { data: gd, count: dA > 0.01 ? 56 : 0, size: 5.2, gain: 2.2 },
      floor: { center: [sc[0], floorY, sc[2]], size: 330, alpha: floorAlpha },
      ambient: amb,
      bg: this.bg || [0x16 / 255, 0x18 / 255, 0x1b / 255],
      sceneCenter: sc,
      sceneRadius: sr,
      shadowDirty: this.shadowDirty,
    };
  }

  draw(now) {
    const t0 = performance.now();
    const f = this.frameData();
    if (!this.r.render(f)) {
      // Skyggeleggerne er ikke ferdige ennå: prøv igjen neste bilde.
      this.waiting = true;
      return false;
    }
    if (this.adaptAfter === Infinity) this.adaptAfter = performance.now() + 2500;
    if (this.waiting) {
      this.waiting = false;
      this.stats.firstFrameMs = performance.now();
      this.opts.onReady?.();
    }
    this.shadowDirty = false;
    this.lastFrame = f;
    const ms = performance.now() - t0;
    this.stats.lastDrawMs = ms;
    this.stats.drawMs.push(ms);
    if (this.stats.drawMs.length > 240) this.stats.drawMs.shift();
    this.stats.frames++;
    this.lastDraw = now;
    return true;
  }

  // ------------------------------------------------------------------ Plukking og input
  doPick() {
    const { x, y } = this.pendingPick;
    this.pendingPick = null;
    if (!this.lastFrame) return;
    const dpr = this.canvas.width / Math.max(1, this.host.clientWidth);
    const id = this.r.pick(this.lastFrame, Math.round(x * dpr), Math.round(y * dpr));
    const g = id ? GROUP_IDS[id - 1] : null;
    if (g !== this.hoverGroup) {
      this.hoverGroup = g;
      this.dirty = true;
      this.kick();
    }
    this.opts.onHover?.(g ? this.groupInfo(g) : null, x, y);
  }
  groupInfo(g) {
    const [name, desc, zone] = GROUPS[g];
    return { id: g, name, desc, zone: zone || null };
  }
  /** Gruppene som synes nå (for dellisten). */
  visibleGroups() {
    const out = [];
    for (const l of LAYERS) {
      if (this.layerState[l.id].goal >= 0.5) continue;
      for (const p of this.parts) if (p.layer === l.id && !out.includes(p.group)) out.push(p.group);
    }
    return out.map((g) => this.groupInfo(g));
  }

  bindInput() {
    const h = this.host;
    const pts = new Map();
    let start = null;
    let lastMove = null;
    let pinch = null;
    h.addEventListener('pointerdown', (e) => {
      if (e.button !== 0 && e.pointerType === 'mouse') return;
      h.setPointerCapture(e.pointerId);
      pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
      this.vel = 0;
      if (pts.size === 1) {
        start = { x: e.clientX, y: e.clientY, t: performance.now(), moved: false };
        lastMove = { x: e.clientX, t: performance.now(), v: 0 };
      } else if (pts.size === 2) {
        const [a, b] = [...pts.values()];
        pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), zoom: this.goal.zoom };
      }
    });
    h.addEventListener('pointermove', (e) => {
      const p = pts.get(e.pointerId);
      if (!p) {
        if (e.pointerType === 'mouse') {
          const r = h.getBoundingClientRect();
          this.pendingPick = { x: e.clientX - r.left, y: e.clientY - r.top };
          this.kick();
        }
        return;
      }
      const dx = e.clientX - p.x;
      const dy = e.clientY - p.y;
      p.x = e.clientX;
      p.y = e.clientY;
      if (pts.size === 2 && pinch) {
        const [a, b] = [...pts.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        this.goal.zoom = this.cam.zoom = clamp((pinch.zoom * d) / Math.max(1, pinch.d), 0.6, 3.2);
        this.change();
        return;
      }
      if (!start) return;
      if (!start.moved && Math.hypot(e.clientX - start.x, e.clientY - start.y) < 4) return;
      if (!start.moved) {
        start.moved = true;
        this.drag = true;
        h.classList.add('dragging');
        this.hoverGroup = null;
        this.opts.onHover?.(null);
      }
      const k = 0.36;
      this.goal.yaw = this.cam.yaw = this.cam.yaw - dx * k;
      this.goal.pitch = this.cam.pitch = clamp(this.cam.pitch + dy * k * 0.8, -60, 85);
      const now = performance.now();
      const dtm = Math.max(1, now - lastMove.t);
      lastMove = { x: e.clientX, t: now, v: lerp(lastMove.v, (-dx * k * 1000) / dtm, 0.5) };
      this.shadowDirty = true;
      this.change();
    });
    const end = (e) => {
      if (!pts.has(e.pointerId)) return;
      pts.delete(e.pointerId);
      if (pts.size < 2) pinch = null;
      if (pts.size) return;
      h.classList.remove('dragging');
      const wasDrag = start?.moved;
      this.drag = false;
      if (wasDrag && this.motion() === 'full' && performance.now() - lastMove.t < 80) this.vel = clamp(lastMove.v, -400, 400);
      if (!wasDrag && e.type === 'pointerup' && start) {
        // Klikk: velg sone hvis det var en lyssone.
        const r = h.getBoundingClientRect();
        const dpr = this.canvas.width / Math.max(1, h.clientWidth);
        if (this.lastFrame) {
          const id = this.r.pick(this.lastFrame, Math.round((e.clientX - r.left) * dpr), Math.round((e.clientY - r.top) * dpr));
          const g = id ? GROUP_IDS[id - 1] : null;
          if (g && ZONE_OF_GROUP[g]) this.opts.onZone?.(ZONE_OF_GROUP[g]);
          else if (g) this.opts.onPart?.(this.groupInfo(g));
        }
      }
      start = null;
      this.kick();
    };
    h.addEventListener('pointerup', end);
    h.addEventListener('pointercancel', end);
    h.addEventListener('pointerleave', (e) => {
      if (e.pointerType !== 'mouse' || pts.size) return;
      this.pendingPick = null;
      if (this.hoverGroup) {
        this.hoverGroup = null;
        this.dirty = true;
        this.kick();
      }
      this.opts.onHover?.(null);
    });
    h.addEventListener(
      'wheel',
      (e) => {
        e.preventDefault();
        const d = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
        this.goal.zoom = clamp(this.goal.zoom * Math.exp(-d * 0.0016), 0.6, 3.2);
        if (this.motion() !== 'full') this.cam.zoom = this.goal.zoom;
        this.change();
      },
      { passive: false },
    );
    h.addEventListener('dblclick', () => this.resetView());
    h.addEventListener('keydown', (e) => {
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      const big = e.shiftKey;
      let used = true;
      if (e.key === 'ArrowLeft') this.rotateBy(big ? -45 : -15, 0);
      else if (e.key === 'ArrowRight') this.rotateBy(big ? 45 : 15, 0);
      else if (e.key === 'ArrowUp') this.rotateBy(0, big ? 30 : 10);
      else if (e.key === 'ArrowDown') this.rotateBy(0, big ? -30 : -10);
      else if (e.key === '+' || e.key === '=') this.zoomBy(1.2);
      else if (e.key === '-' || e.key === '_') this.zoomBy(1 / 1.2);
      else if (e.key === '0' || e.key === 'Home') this.resetView();
      else used = false;
      if (used) e.preventDefault();
    });
  }

  // ------------------------------------------------------------------ Beskrivelse
  /** Kort tekst om hva som vises nå (for skjermlesere og bildeteksten). */
  describe() {
    const y = wrap(this.goal.yaw);
    const p = this.goal.pitch;
    const side = y < 0 ? 'fra venstre' : 'fra høyre';
    const ay = Math.abs(y);
    let angle;
    if (p > 50) angle = 'ovenfra, med lyslisten mot deg';
    else if (p < -25) angle = 'nedenfra';
    else if (ay < 12) angle = 'rett forfra';
    else if (ay < 70) angle = `skrått forfra ${side}`;
    else if (ay < 110) angle = y < 0 ? 'fra siden med braketten og utgangene' : 'fra enden';
    else if (ay < 168) angle = `skrått bakfra ${side}`;
    else angle = 'rett bakfra, mot bakplaten';
    const parts = STEP_TEXT[this.step];
    const ex = this.explodeGoal > 0.05 ? ` Lagene er skilt fra hverandre (${Math.round(this.explodeGoal * 100)} %).` : '';
    const fans = this.layerState.vifter.goal >= 0.5 ? '' : this.fanPct > 0 ? ` Viftene går på ${Math.round(this.fanPct)} %.` : ' Viftene står stille.';
    return `Skjermkort i 3D, sett ${angle}. ${parts}${ex}${fans}`;
  }

  /** Setter kameraet direkte (brukes av skjermbildene og kontrollen). */
  setCamera({ yaw = this.goal.yaw, pitch = this.goal.pitch, zoom = 1 } = {}) {
    this.goal.yaw = yaw;
    this.goal.pitch = clamp(pitch, -60, 85);
    this.goal.zoom = zoom;
    this.vel = 0;
    this.fit(true);
    Object.assign(this.cam, { yaw: this.goal.yaw, pitch: this.goal.pitch, zoom });
    this.shadowDirty = true;
    this.change();
  }
  /** Et punkt i kortets koordinater (mm) til piksler i visningen (CSS-piksler). */
  projectCard(p) {
    const f = this.lastFrame || this.frameData();
    const q = mat4.transformPoint(f.viewProj, [p[0] - this.center[0], p[1] - this.center[1], p[2] - this.center[2]]);
    return [(q[0] * 0.5 + 0.5) * this.host.clientWidth, (0.5 - q[1] * 0.5) * this.host.clientHeight];
  }
  /**
   * Måling: n bilder tegnet etter hverandre, hvert ventet ferdig (readPixels
   * tvinger GPU-en til å bli ferdig). Gir ms per bilde for CPU + GPU.
   * shadow: tegn skyggekartet hver gang også (verste tilfelle).
   */
  bench(n = 60, { shadow = false } = {}) {
    const gl = this.r.gl;
    const px = new Uint8Array(4);
    this.draw(performance.now());
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    const t0 = performance.now();
    for (let i = 0; i < n; i++) {
      this.fanAngle[0] -= 0.05;
      this.fanAngle[1] -= 0.05;
      if (shadow) this.shadowDirty = true;
      this.draw(performance.now());
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    }
    return (performance.now() - t0) / n;
  }

  get fps() {
    const a = this.stats.drawMs;
    return a.length ? a.reduce((s, v) => s + v, 0) / a.length : 0;
  }

  destroy() {
    cancelAnimationFrame(this.raf);
    this.ro.disconnect();
    this.io.disconnect();
    document.removeEventListener('visibilitychange', this.onVis);
  }
}

/** Vinkel til −180…180. */
export function wrap(a) {
  return ((((a + 180) % 360) + 360) % 360) - 180;
}
