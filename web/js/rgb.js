// Lys (RGB): effektmotoren og den flate SVG-tegneren. Ingenting her snakker
// med kortet. «Bruk» lagrer bare valget på serveren.
//
// v0.0.13: delt i to.
//   RgbEngine regner ut fargen på alle 56 lysdiodene (lyslist 20, vifte 1
//     18, vifte 2 18), pusten og bakgrunnslyset, og sender dem til et mål
//     (LightTarget, nederst i fila). Matematikken er den samme som før.
//   SvgCard er det flate kortet fra før, nå som ett av målene. 3D-kortet
//     (lys3d/view.js, CardView) er det andre. SvgCard brukes når WebGL
//     mangler, ved programvaregrafikk, når grafikken tegner for tregt, og når
//     du velger 2D.
//   RgbPreview = motor + SvgCard, som før (brukes av prototypen i prototyper/).
//
// SvgCard tegnes i tre lag, så hvert bilde koster minst mulig (docs/bevegelse.md):
//   1. Kortet (én statisk SVG, tegnes én gang).
//   2. To viftehjul som egne elementer, rotert med Web Animations på
//      kompositoren. JS endrer bare farten (playbackRate) når viftefarten
//      endres, aldri per bilde.
//   3. Lysdiodene i en egen SVG over kortet (eget lag). Bare fargene som
//      faktisk endres, skrives, og bare mens effekten beveger seg. Valgt
//      sone fremheves ved at de andre sonene toner ned (CSS, --dur-3).
//      (Et canvas ble prøvd: billigere med GPU, men dyrere i programvare-
//      kompositering, der hele bildet må kopieres hvert bilde.)
// Pust endrer bare lagets opasitet (kompositoren), ikke fargene.
// v0.0.12: setEgg(fn) viser et påskeegg (Nordlys, se easter.js) over valgt
// effekt, bare i forhåndsvisningen. fn(u, t) gir fargen [r, g, b] for en
// lysdiode. Valget (cfg) er urørt, så setEgg(null) gir tilbake det som var.
// Fila importerer bare util.js, så prototypen i prototyper/ kan laste den.
// Bakgrunnslyset er ett element med farge og opasitet; opasiteten skrives
// per bilde, fargen bare når den endres merkbart. Bytte av effekt eller
// farge kryssfades over --dur-4 (250 ms).

import { clamp, motion, dur } from './util.js';

/** Regnbue og fargesyklus flytter fargen i trinn på 1,5° (under det øyet skiller). */
const HUE_STEP = 1.5;

const NS = 'http://www.w3.org/2000/svg';
const BAR_LEDS = 20;
const RING_LEDS = 18;

function svgEl(tag, attrs = {}) {
  const n = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
  return n;
}

export function hexToRgb(hex) {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function hslToRgb(h, s, l) {
  h = ((h % 360) + 360) % 360;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}

function rgbToHue([r, g, b]) {
  r /= 255;
  g /= 255;
  b /= 255;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const d = mx - mn;
  if (!d) return 0;
  let h;
  if (mx === r) h = ((g - b) / d) % 6;
  else if (mx === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return (h * 60 + 360) % 360;
}

/** Bygger kortet i tre lag og returnerer referanser til viftehjul og lysdioder. */
function buildCard(host, label) {
  const VB = '0 0 660 280';
  const svg = svgEl('svg', { viewBox: VB, class: 'gpu-svg', role: 'img', 'aria-label': 'Forhåndsvisning av skjermkortets lys' });
  svg.innerHTML = `
    <!-- Brakett -->
    <g>
      <rect x="10" y="22" width="20" height="226" rx="3" fill="#353c45"/>
      <rect x="15" y="40" width="10" height="36" rx="2" fill="#0b0d10"/>
      <rect x="15" y="86" width="10" height="36" rx="2" fill="#0b0d10"/>
      <rect x="15" y="132" width="10" height="20" rx="2" fill="#0b0d10"/>
      <g fill="#1a1f25">${Array.from({ length: 7 }, (_, i) => `<rect x="16" y="${162 + i * 10}" width="8" height="5" rx="1"/>`).join('')}</g>
    </g>
    <!-- Kretskort-kant og kontaktfingre -->
    <rect x="34" y="214" width="580" height="16" rx="2" fill="#0d1a14"/>
    <g fill="#b08a3e">${Array.from({ length: 38 }, (_, i) => `<rect x="${112 + i * 7 + (i > 10 ? 8 : 0)}" y="230" width="4.5" height="18" rx="0.8"/>`).join('')}</g>
    <!-- Strømkontakter -->
    <rect x="512" y="14" width="34" height="14" rx="2" fill="#15191e" stroke="#2b323a"/>
    <rect x="552" y="14" width="44" height="14" rx="2" fill="#15191e" stroke="#2b323a"/>
    <!-- Deksel -->
    <path d="M34 34 H600 L630 62 V196 L612 216 H58 L34 196 Z" fill="#1b2026" stroke="#2c333c" stroke-width="1.5"/>
    <path d="M318 60 L352 60 L340 196 L306 196 Z" fill="#1d2229"/>
    <path d="M604 70 L622 86 V178 L604 194 Z" fill="#1d2229"/>
    <path d="M44 70 L58 58 V196 L44 186 Z" fill="#1d2229"/>
    <text x="330" y="210" text-anchor="middle" fill="#3b444f" font-size="9" letter-spacing="4" font-family="Ubuntu Sans, sans-serif" font-weight="700">${label}</text>
    <g id="fans"></g>
  `;
  const ledSvg = svgEl('svg', { viewBox: VB, class: 'gpu-svg leds', 'aria-hidden': 'true' });
  const zoneG = { bar: svgEl('g', { 'data-zone': 'bar' }), fan1: svgEl('g', { 'data-zone': 'fan1' }), fan2: svgEl('g', { 'data-zone': 'fan2' }) };
  ledSvg.append(zoneG.bar, zoneG.fan1, zoneG.fan2);

  const fansG = svg.querySelector('#fans');
  const rotors = [];
  const rings = [];
  for (const [fi, cx] of [186, 476].entries()) {
    const cy = 128;
    fansG.append(svgEl('circle', { cx, cy, r: 82, fill: '#0c0e11', stroke: '#252b33', 'stroke-width': 2 }));
    fansG.append(svgEl('circle', { cx, cy, r: 72, fill: '#0b0d10' }));
    // Viftehjulet: eget element (eget lag), plassert i prosent av kortet.
    const R0 = 72;
    const rot = svgEl('svg', { viewBox: `${cx - R0} ${cy - R0} ${R0 * 2} ${R0 * 2}`, 'aria-hidden': 'true' });
    const wrap = document.createElement('div');
    wrap.className = 'rotor';
    wrap.style.left = `${((cx - R0) / 660) * 100}%`;
    wrap.style.top = `${((cy - R0) / 280) * 100}%`;
    wrap.style.width = `${((R0 * 2) / 660) * 100}%`;
    wrap.style.height = `${((R0 * 2) / 280) * 100}%`;
    wrap.append(rot);
    for (let i = 0; i < 11; i++) {
      const a = (i / 11) * 360;
      rot.append(
        svgEl('path', {
          d: `M ${cx} ${cy - 18} C ${cx + 22} ${cy - 30}, ${cx + 30} ${cy - 54}, ${cx + 14} ${cy - 68} C ${cx + 2} ${cy - 58}, ${cx - 6} ${cy - 40}, ${cx} ${cy - 18} Z`,
          fill: '#1c2129',
          stroke: '#2a313a',
          'stroke-width': 0.8,
          transform: `rotate(${a} ${cx} ${cy})`,
        }),
      );
    }
    rot.append(svgEl('circle', { cx, cy, r: 20, fill: '#262c34', stroke: '#2f3740' }));
    rot.append(svgEl('path', { d: `M ${cx - 7} ${cy + 6} L ${cx} ${cy - 8} L ${cx + 7} ${cy + 6} Z`, fill: 'none', stroke: '#56606b', 'stroke-width': 1.6, 'stroke-linejoin': 'round' }));
    rotors.push(wrap);

    // Lysring rundt viften.
    const R = 77;
    for (let i = 0; i < RING_LEDS; i++) {
      const a0 = (i / RING_LEDS) * Math.PI * 2 - Math.PI / 2 + 0.03;
      const a1 = ((i + 1) / RING_LEDS) * Math.PI * 2 - Math.PI / 2 - 0.03;
      const p = svgEl('path', {
        d: `M ${cx + Math.cos(a0) * R} ${cy + Math.sin(a0) * R} A ${R} ${R} 0 0 1 ${cx + Math.cos(a1) * R} ${cy + Math.sin(a1) * R}`,
        fill: 'none',
        'stroke-width': 3.5,
        'stroke-linecap': 'round',
      });
      zoneG[fi === 0 ? 'fan1' : 'fan2'].append(p);
      rings.push({ el: p, u: (cx < 300 ? 0 : 0.5) + i / RING_LEDS / 2, attr: 'stroke', zone: fi === 0 ? 'fan1' : 'fan2' });
    }
  }

  // Lyslist langs toppen.
  const bar = [];
  const x0 = 70;
  const x1 = 586;
  const segW = (x1 - x0) / BAR_LEDS;
  for (let i = 0; i < BAR_LEDS; i++) {
    const r = svgEl('rect', { x: x0 + i * segW + 1.2, y: 40, width: segW - 2.4, height: 4.5, rx: 2 });
    zoneG.bar.append(r);
    bar.push({ el: r, u: i / (BAR_LEDS - 1), attr: 'fill', zone: 'bar' });
  }
  host.replaceChildren(svg, ...rotors, ledSvg);
  return { rotors, leds: [...bar, ...rings], ledSvg };
}

const easeOut = (t) => 1 - (1 - t) ** 3;
const OFF = [26, 31, 37];
function scale([r, g, b], k) {
  return [r * k, g * k, b * k];
}

/** Lysdiodene i fast rekkefølge: { zone, u }. u er plassen langs sonen (regnbuen). */
export const LEDS = [
  ...Array.from({ length: BAR_LEDS }, (_, i) => ({ zone: 'bar', u: i / (BAR_LEDS - 1) })),
  ...Array.from({ length: RING_LEDS }, (_, i) => ({ zone: 'fan1', u: i / RING_LEDS / 2 })),
  ...Array.from({ length: RING_LEDS }, (_, i) => ({ zone: 'fan2', u: 0.5 + i / RING_LEDS / 2 })),
];
export const ZONE_RANGE = { bar: [0, 20], fan1: [20, 38], fan2: [38, 56] };

/** Et mål som ikke viser noe (før en tegner finnes). */
const NO_TARGET = { setZoneColor() {}, setLedColors() {}, setLayerOpacity() {}, setDimmedZones() {}, setAmbient() {}, commit() {} };

export class RgbEngine {
  constructor(target = null) {
    this.target = target || NO_TARGET;
    this.leds = LEDS;
    this.n = LEDS.length;
    this.layerK = 1; // pust: styrken på alle lysdiodene
    this.t = 0;
    this.gpuTemp = null;
    this.cfg = null;
    this.cur = new Float32Array(this.n * 3); // fargen som vises nå (sRGB 0–255)
    this.from = new Float32Array(this.n * 3); // ved kryssfade: fargen vi kom fra
    this.xf = 1; // kryssfade 0 → 1
    this.has = false;
    this.dirty = true;
    this.zone = 'all';
    this.egg = null; // påskeegg over effekten: fn(u, t) → [r, g, b], eller null
    // Animasjonsnivået motoren følger. Lys bytter det ut når 3D tegner for
    // tregt (da står effektene stille, som ved «Redusert»).
    this.motion = motion;
  }

  /** Bytter mål (2D ↔ 3D). Det nye målet får alt med en gang. */
  setTarget(target) {
    this.target = target || NO_TARGET;
    this.target.setDimmedZones(this.zone);
    if (this.has) this.push(true);
    this.dirty = true;
  }

  /** Påskeegg (easter.js): fargefunksjon eller null. Kryssfades som et bytte av effekt. */
  setEgg(fn) {
    if (this.egg === fn) return;
    if (this.has && this.motion() === 'full') {
      for (let j = 0; j < this.cur.length; j++) this.from[j] = this.cur[j] * this.layerK;
      this.xf = 0;
    }
    this.egg = fn || null;
    this.dirty = true;
  }

  /** cfg: { color, zones: {bar,fan1,fan2}, effect, brightness, speed, direction, tempCold, tempHot, tempMin, tempMax } */
  setConfig(cfg) {
    const prev = this.cfg;
    this.cfg = { ...cfg, zones: { ...cfg.zones } };
    this.base = {};
    this.baseHue = {};
    for (const z of ['bar', 'fan1', 'fan2']) {
      this.base[z] = hexToRgb(cfg.zones?.[z] || cfg.color);
      this.baseHue[z] = rgbToHue(this.base[z]);
    }
    this.cold = hexToRgb(cfg.tempCold || '#3a7bf0');
    this.hot = hexToRgb(cfg.tempHot || '#e5383b');
    // Kryssfade når effekten eller fargene byttes (ikke for glidere som
    // lysstyrke og hastighet, de skal følge hånden direkte).
    const swap = prev && this.has && (prev.effect !== cfg.effect || ['bar', 'fan1', 'fan2'].some((z) => prev.zones[z] !== cfg.zones[z]) || prev.tempCold !== cfg.tempCold || prev.tempHot !== cfg.tempHot);
    if (swap && this.motion() === 'full') {
      // Det som vises nå, er fargen ganget med lagets opasitet (pust).
      for (let j = 0; j < this.cur.length; j++) this.from[j] = this.cur[j] * this.layerK;
      this.xf = 0;
    }
    // Pust starter på toppen (full styrke), så det ikke blir et hopp ned.
    if (cfg.effect === 'breathe' && prev?.effect !== 'breathe') this.t = (6.6 - cfg.speed * 0.58) / 2;
    this.dirty = true;
  }

  /** Valgt sone: de andre sonene tones ned til 30 % ('all' for ingen). */
  setZone(z) {
    this.zone = z;
    this.target.setDimmedZones(z);
  }

  setTemp(c) {
    if (c != null) {
      this.gpuTemp = c;
      this.dirty = true;
    }
  }

  /** Fargen etter GPU-temperaturen (Temperatur-effekten). */
  tempColor() {
    const { tempMin = 40, tempMax = 80 } = this.cfg;
    const k = this.gpuTemp == null ? 0 : clamp((this.gpuTemp - tempMin) / Math.max(1, tempMax - tempMin), 0, 1);
    return this.cold.map((c, i) => c + (this.hot[i] - c) * k);
  }

  /** Returnerer true så lenge lysene beveger seg (eller kryssfader). */
  tick(dt) {
    if (!this.cfg) return false;
    const animate = this.motion() === 'full';
    const { speed } = this.cfg;
    const effect = this.egg ? 'egg' : this.cfg.effect;
    const moving = animate && (effect === 'breathe' || effect === 'rainbow' || effect === 'cycle' || effect === 'egg');
    const fading = this.xf < 1;
    // Statisk, temperatur og av (og alt ved redusert/av) regnes bare ved
    // endring; da sover rAF-løkka (tick returnerer false).
    if (!moving && !fading && !this.dirty) return false;
    const wasDirty = this.dirty;
    this.dirty = false;
    if (moving) this.t += dt;
    if (fading) this.xf = animate ? Math.min(1, this.xf + (dt * 1000) / Math.max(1, dur(4))) : 1;
    const k = fading ? easeOut(this.xf) : 1;
    const dir = this.cfg.direction === 'ccw' ? -1 : 1;
    // Nordlys lyser også når lysene ellers er av, og aldri svakere enn 40 %.
    const bri = effect === 'egg' ? (this.cfg.effect === 'off' ? 0.85 : Math.max(0.4, clamp(this.cfg.brightness, 0, 100) / 100)) : clamp(this.cfg.brightness, 0, 100) / 100;
    // Pust: animert går styrken i lagets opasitet; stillbildet (redusert/av) er 60 %.
    let breathK = 0.6;
    let layerK = 1;
    if (effect === 'breathe' && animate) {
      const period = 6.6 - speed * 0.58;
      const c = 0.5 - 0.5 * Math.cos((this.t / period) * Math.PI * 2);
      layerK = Math.round((0.08 + 0.92 * c * c) * 500) / 500;
      breathK = 1;
    }
    const layerChanged = layerK !== this.layerK;
    this.layerK = layerK;
    const tc = effect === 'temperature' ? this.tempColor() : null;
    // Regnbue og fargesyklus (v0.0.8): fasen rundes til hele HUE_STEP grader.
    // Fargen flytter seg da i trinn under det øyet skiller, og i bildene
    // mellom to trinn er alle fargene like, så ingenting skrives. Ved 240 Hz
    // og fart 5 skrives regnbuen 100 ganger i sekundet i stedet for 240.
    // Nordlys: tiden rundes til 1/20 s, så det skrives høyst 20 ganger i sekundet.
    const phase = effect === 'rainbow' || effect === 'cycle' ? Math.round((dir * this.t * speed * (effect === 'rainbow' ? 30 : 22)) / HUE_STEP) * HUE_STEP : effect === 'egg' ? Math.round(this.t * 20) / 20 : 0;
    if (moving && !fading && !wasDirty && phase === this.lastPhase && effect === this.lastEffect) {
      // Bare pusten endret seg: lagets styrke og bakgrunnslyset.
      if (layerChanged) this.push(false);
      return true;
    }
    this.lastPhase = phase;
    this.lastEffect = effect;
    const n = this.n;
    const cur = this.cur;
    for (let i = 0; i < n; i++) {
      const led = this.leds[i];
      const base = this.base[led.zone];
      let rgb;
      if (effect === 'off') rgb = OFF;
      else if (effect === 'static') rgb = scale(base, bri);
      else if (effect === 'temperature') rgb = scale(tc, bri);
      else if (effect === 'breathe') rgb = scale(base, bri * breathK);
      else if (effect === 'rainbow') rgb = scale(hslToRgb(led.u * 360 - phase, 1, 0.55), bri);
      else if (effect === 'egg') rgb = scale(this.egg(led.u, phase), bri);
      else rgb = scale(hslToRgb(this.baseHue[led.zone] + phase, 1, 0.55), bri);
      const j = i * 3;
      cur[j] = fading ? this.from[j] + (rgb[0] - this.from[j]) * k : rgb[0];
      cur[j + 1] = fading ? this.from[j + 1] + (rgb[1] - this.from[j + 1]) * k : rgb[1];
      cur[j + 2] = fading ? this.from[j + 2] + (rgb[2] - this.from[j + 2]) * k : rgb[2];
    }
    this.has = true;
    this.push(true);
    return moving || this.xf < 1;
  }

  /** Sender fargene til målet. En sone med én farge går som setZoneColor. */
  push(colors) {
    const T = this.target;
    const c = this.cur;
    if (colors) {
      for (const [zone, [a, b]] of Object.entries(ZONE_RANGE)) {
        let same = true;
        for (let i = a + 1; i < b && same; i++) same = c[i * 3] === c[a * 3] && c[i * 3 + 1] === c[a * 3 + 1] && c[i * 3 + 2] === c[a * 3 + 2];
        if (same) T.setZoneColor(zone, [c[a * 3], c[a * 3 + 1], c[a * 3 + 2]]);
        else T.setLedColors(zone, c.subarray(a * 3, b * 3));
      }
    }
    T.setLayerOpacity(this.layerK);
    // Bakgrunnslys: snittfargen normaliseres (styrken går i opasiteten), så
    // pust bare endrer opasiteten.
    let ar = 0;
    let ag = 0;
    let ab = 0;
    for (let i = 0; i < this.n; i++) {
      ar += c[i * 3];
      ag += c[i * 3 + 1];
      ab += c[i * 3 + 2];
    }
    ar /= this.n;
    ag /= this.n;
    ab /= this.n;
    const m = Math.max(ar, ag, ab);
    T.setAmbient(m < 1 ? null : [(ar / m) * 255, (ag / m) * 255, (ab / m) * 255], (m / 255) * this.layerK);
    T.commit();
  }
}

/** Det flate kortet (SVG). LightTarget, pluss vifter som kompositoren roterer. */
export class SvgCard {
  /** host: .rgb-card. stage: .rgb-stage (får bakgrunnslyset). */
  constructor(host, stage, label = '') {
    this.host = host;
    const card = buildCard(host, String(label).replace(/[<&>"]/g, ''));
    this.rotors = card.rotors;
    this.ledLayer = card.ledSvg;
    this.leds = card.leds;
    this.spin = [];
    this.stage = stage;
    // Bakgrunnslys: ytre element slås av/på (CSS), indre får farge og styrke.
    this.glow = document.createElement('div');
    this.glow.className = 'rgb-glow';
    this.amb = document.createElement('div');
    this.amb.className = 'rgb-ambient';
    this.amb.append(this.glow);
    stage.prepend(this.amb);
    this.fanPct = 35;
    this.lastStr = new Array(this.leds.length).fill('');
    this.layerK = 1;
    this.glowCol = '';
    this.glowA = -1;
    this.running = false;
  }
  write(i, r, g, b) {
    const str = `rgb(${r | 0},${g | 0},${b | 0})`;
    if (str !== this.lastStr[i]) {
      this.lastStr[i] = str;
      this.leds[i].el.setAttribute(this.leds[i].attr, str);
    }
  }
  setZoneColor(zone, [r, g, b]) {
    const [a, z] = ZONE_RANGE[zone];
    for (let i = a; i < z; i++) this.write(i, r, g, b);
  }
  setLedColors(zone, arr) {
    const a = ZONE_RANGE[zone][0];
    for (let k = 0; k * 3 < arr.length; k++) this.write(a + k, arr[k * 3], arr[k * 3 + 1], arr[k * 3 + 2]);
  }
  setLayerOpacity(k) {
    if (k === this.layerK) return;
    this.layerK = k;
    this.ledLayer.style.opacity = k === 1 ? '' : String(k);
  }
  /** Valgt sone: de andre sonene toner ned til 30 % (CSS på .rgb-stage[data-zone]). */
  setDimmedZones(z) {
    this.stage.dataset.zone = z;
  }
  /** Fargen skrives bare når den endres merkbart, opasiteten per bilde. */
  setAmbient(rgb, a) {
    if (rgb) {
      const col = `rgb(${rgb.map((v) => Math.round(v / 3) * 3).join(',')})`;
      if (col !== this.glowCol) {
        this.glowCol = col;
        this.glow.style.backgroundColor = col;
      }
    }
    const al = Math.round(a * 200) / 200;
    if (al !== this.glowA) {
      this.glowA = al;
      this.glow.style.opacity = String(al);
    }
  }
  commit() {}

  setFan(pct) {
    if (pct == null) return;
    this.fanPct = pct;
    this.syncSpin();
  }
  /** Starter, stopper eller justerer viftehjulene (kompositoren roterer dem). */
  syncSpin() {
    const on = this.running && motion() === 'full';
    if (!on) {
      for (const a of this.spin) a.cancel();
      this.spin = [];
      return;
    }
    const rate = (60 + (clamp(this.fanPct, 0, 100) / 100) * 520) / 360; // omdreininger per sekund
    if (!this.spin.length) {
      this.spin = this.rotors.map((r) => {
        const a = r.animate([{ transform: 'rotate(0turn)' }, { transform: 'rotate(1turn)' }], { duration: 1000, iterations: Infinity });
        a.currentTime = Math.random() * 1000;
        a.playbackRate = rate;
        return a;
      });
    } else for (const a of this.spin) if (Math.abs(a.playbackRate - rate) > 0.005) a.updatePlaybackRate(rate);
  }
  /** Kalles når siden vises eller skjules, og når animasjonsnivået endres. */
  setRunning(on) {
    this.running = on;
    this.syncSpin();
  }
  /** Fjerner kortet (ved bytte til 3D). */
  destroy() {
    this.running = false;
    this.syncSpin();
    this.amb.remove();
    this.host.replaceChildren();
    delete this.stage.dataset.zone;
  }
}

/** Motor + flatt kort i ett, som før v0.0.13 (prototypen i prototyper/ bruker den). */
export class RgbPreview {
  constructor(host, stage, label = '') {
    this.card = new SvgCard(host, stage, label);
    this.engine = new RgbEngine(this.card);
  }
  get cur() {
    return this.engine.cur;
  }
  get layerK() {
    return this.engine.layerK;
  }
  setConfig(cfg) {
    this.engine.setConfig(cfg);
  }
  setZone(z) {
    this.engine.setZone(z);
  }
  setTemp(c) {
    this.engine.setTemp(c);
  }
  setEgg(fn) {
    this.engine.setEgg(fn);
  }
  setFan(pct) {
    this.card.setFan(pct);
  }
  setRunning(on) {
    this.card.setRunning(on);
    this.engine.dirty = true;
  }
  tick(dt) {
    return this.engine.tick(dt);
  }
}

/*
 * LightTarget – det et kort (2D eller 3D) må kunne for å vise lyset. Alle
 * farger er sRGB 0–255.
 *   setZoneColor(zone, [r, g, b])   hele sonen i én farge ('bar' | 'fan1' | 'fan2')
 *   setLedColors(zone, rgbArray)    én farge per LED (3 tall per LED, rekkefølgen i LEDS)
 *   setLayerOpacity(k)              pust: styrken på alle LED-ene (0–1)
 *   setDimmedZones(zone)            'all' eller valgt sone (de andre tones til 30 %)
 *   setAmbient([r, g, b] | null, a) bakgrunnslys bak kortet
 *   commit()                        alt for dette bildet er sendt
 */
