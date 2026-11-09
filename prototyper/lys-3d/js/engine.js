// Effektmotoren: samme fargematematikk som RgbPreview.tick() i
// web/js/rgb.js, men uten SVG og DOM. Den regner ut fargen på alle 56
// lysdiodene (lyslist 20, vifte 1 18, vifte 2 18, i samme rekkefølge og med
// samme u som rgb.js), lagets opasitet (pust) og bakgrunnslyset, og sender
// det til et mål gjennom et lite grensesnitt (se LightTarget nederst).
//
// Prototypen har sin egen kopi fordi den skal kunne åpnes for seg selv.
// verktoy/kontroll.mjs laster begge i samme side og sjekker at de gir
// nøyaktig de samme fargene. Ved flytting inn i appen erstattes denne fila
// av rgb.js (se README.md, «Plan for å flytte den inn»).

const HUE_STEP = 1.5;
const BAR_LEDS = 20;
const RING_LEDS = 18;
const OFF = [26, 31, 37];
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const easeOut = (t) => 1 - (1 - t) ** 3;
const scale = ([r, g, b], k) => [r * k, g * k, b * k];

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

/** LED-ene i rgb.js-rekkefølge: { zone, u }. */
export const LEDS = [
  ...Array.from({ length: BAR_LEDS }, (_, i) => ({ zone: 'bar', u: i / (BAR_LEDS - 1) })),
  ...Array.from({ length: RING_LEDS }, (_, i) => ({ zone: 'fan1', u: i / RING_LEDS / 2 })),
  ...Array.from({ length: RING_LEDS }, (_, i) => ({ zone: 'fan2', u: 0.5 + i / RING_LEDS / 2 })),
];
export const ZONES = { bar: [0, 20], fan1: [20, 38], fan2: [38, 56] };

export class RgbEngine {
  /**
   * target: LightTarget (se nederst). motion(): 'full' | 'reduced' | 'off'.
   * crossfadeMs(): varigheten på kryssfaden (--dur-4 i appen).
   */
  constructor(target, { motion = () => 'full', crossfadeMs = () => 250 } = {}) {
    this.target = target;
    this.motion = motion;
    this.crossfadeMs = crossfadeMs;
    this.leds = LEDS;
    this.n = LEDS.length;
    this.t = 0;
    this.gpuTemp = null;
    this.cfg = null;
    this.cur = new Float32Array(this.n * 3);
    this.from = new Float32Array(this.n * 3);
    this.xf = 1;
    this.has = false;
    this.layerK = 1;
    this.dirty = true;
    this.zone = 'all';
  }

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
    const swap = prev && this.has && (prev.effect !== cfg.effect || ['bar', 'fan1', 'fan2'].some((z) => prev.zones[z] !== cfg.zones[z]) || prev.tempCold !== cfg.tempCold || prev.tempHot !== cfg.tempHot);
    if (swap && this.motion() === 'full') {
      for (let j = 0; j < this.cur.length; j++) this.from[j] = this.cur[j] * this.layerK;
      this.xf = 0;
    }
    if (cfg.effect === 'breathe' && prev?.effect !== 'breathe') this.t = (6.6 - cfg.speed * 0.58) / 2;
    this.dirty = true;
  }

  /** Valgt sone: de andre sonene tones ned til 30 % (som i rgb.js). */
  setZone(z) {
    this.zone = z;
    this.target.setDimmedZones?.(z);
  }

  setTemp(c) {
    if (c != null) {
      this.gpuTemp = c;
      this.dirty = true;
    }
  }

  tempColor() {
    const { tempMin = 40, tempMax = 80 } = this.cfg;
    const k = this.gpuTemp == null ? 0 : clamp((this.gpuTemp - tempMin) / Math.max(1, tempMax - tempMin), 0, 1);
    return this.cold.map((c, i) => c + (this.hot[i] - c) * k);
  }

  /** Som RgbPreview.tick(): returnerer true så lenge lysene beveger seg eller kryssfader. */
  tick(dt) {
    if (!this.cfg) return false;
    const animate = this.motion() === 'full';
    const { effect, speed } = this.cfg;
    const moving = animate && (effect === 'breathe' || effect === 'rainbow' || effect === 'cycle');
    const fading = this.xf < 1;
    if (!moving && !fading && !this.dirty) return false;
    const wasDirty = this.dirty;
    this.dirty = false;
    if (moving) this.t += dt;
    if (fading) this.xf = animate ? Math.min(1, this.xf + (dt * 1000) / Math.max(1, this.crossfadeMs())) : 1;
    const k = fading ? easeOut(this.xf) : 1;
    const dir = this.cfg.direction === 'ccw' ? -1 : 1;
    const bri = clamp(this.cfg.brightness, 0, 100) / 100;
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
    const phase = effect === 'rainbow' || effect === 'cycle' ? Math.round((dir * this.t * speed * (effect === 'rainbow' ? 30 : 22)) / HUE_STEP) * HUE_STEP : 0;
    if (moving && !fading && !wasDirty && phase === this.lastPhase && effect === this.lastEffect) {
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

  /** Sender fargene til målet. Hele soner med én farge går som setZoneColor. */
  push(colorsChanged) {
    const T = this.target;
    if (colorsChanged) {
      for (const [zone, [a, b]] of Object.entries(ZONES)) {
        const c = this.cur;
        let same = true;
        for (let i = a + 1; i < b && same; i++) same = c[i * 3] === c[a * 3] && c[i * 3 + 1] === c[a * 3 + 1] && c[i * 3 + 2] === c[a * 3 + 2];
        if (same) T.setZoneColor(zone, [c[a * 3], c[a * 3 + 1], c[a * 3 + 2]]);
        else T.setLedColors(zone, c.subarray(a * 3, b * 3));
      }
    }
    T.setLayerOpacity?.(this.layerK);
    // Bakgrunnslyset som i rgb.js: snittfargen normalisert, styrken i opasiteten.
    let ar = 0;
    let ag = 0;
    let ab = 0;
    for (let i = 0; i < this.n; i++) {
      ar += this.cur[i * 3];
      ag += this.cur[i * 3 + 1];
      ab += this.cur[i * 3 + 2];
    }
    ar /= this.n;
    ag /= this.n;
    ab /= this.n;
    const m = Math.max(ar, ag, ab);
    T.setAmbient?.(m < 1 ? null : [(ar / m) * 255, (ag / m) * 255, (ab / m) * 255], (m / 255) * this.layerK);
    T.commit?.();
  }
}

/**
 * LightTarget – det modellen må kunne for å vise lyset. Alle farger er
 * sRGB 0–255, slik rgb.js regner dem.
 *
 *   setZoneColor(zone, [r, g, b])   hele sonen i én farge ('bar' | 'fan1' | 'fan2')
 *   setLedColors(zone, rgbArray)    én farge per LED (Float32Array, 3 per LED, rgb.js-rekkefølge)
 *   setLayerOpacity(k)              pust: styrken på alle LED-ene (0–1)
 *   setDimmedZones(zone)            'all' eller sonen som er valgt (de andre tones til 30 %)
 *   setAmbient([r, g, b] | null, a) bakgrunnslys bak kortet
 *   commit()                        ber om et nytt bilde (tegner ikke selv)
 */
