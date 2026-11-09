// Redigerbar viftekurve (temperatur → viftefart) på canvas.
// Dra punkter, dobbeltklikk for å legge til/fjerne, piltaster for finjustering.
//
// Bevegelse (docs/bevegelse.md): punktet følger pekeren uten easing. Ved
// hover og grep vokser punktet (1 → 1,3 over --dur-1), og mens du drar vises
// hjelpelinjer til aksene med verdiene. Forhåndsvalg morfer kurven over
// --dur-4. Canvaset tegnes bare når noe har endret seg.
//
// Start og stopp (v0.0.6): to stiplede loddrette linjer med et skravert bånd
// mellom. Linjene og båndet kan dras langs x-aksen, og Tab går videre fra
// punktene til «start» og «stopp», som flyttes med piltastene. Til venstre
// for stopp er kurven dempet, og viften hviler på minste fart (flat linje).

import { clamp, cssVar, fitCanvas, motion, smooth, dur } from './util.js';
import { num, tempLabel, tempOut, NNBSP } from './fmt.js';
const fmt0 = (v) => num(v, 0);
import { withAlpha } from './charts.js';

const TMIN = 20;
const TMAX = 100;
const HIT = 14;
const FMIN = 35; // minste viftefart på kortet (research)
// Start/stopp: samme grenser som server/store.js (LIMITS, FAN_START_GAP).
export const SS = Object.freeze({ start: [30, 90], stop: [25, 87], gap: 3 });

/**
 * Hviletilstand for start/stopp med hysterese (samme regel som simulatoren):
 * i hvile til temperaturen når start, på kurven til den er under stopp.
 * Gir null når start/stopp er av eller temperaturen mangler.
 */
export function restState(prev, temp, ss) {
  if (!ss?.on || temp == null || !Number.isFinite(temp)) return null;
  if (prev == null || prev) return temp < ss.start;
  return temp < ss.stop;
}

/** Legger start og stopp innenfor grensene med minst SS.gap mellom; `fixed` holdes fast. */
export function clampStartStop(start, stop, fixed = 'start') {
  if (fixed === 'start') {
    start = Math.round(clamp(start, SS.start[0], SS.start[1]));
    stop = Math.round(clamp(stop, SS.stop[0], Math.min(SS.stop[1], start - SS.gap)));
    start = Math.max(start, stop + SS.gap);
  } else {
    stop = Math.round(clamp(stop, SS.stop[0], SS.stop[1]));
    start = Math.round(clamp(start, Math.max(SS.start[0], stop + SS.gap), SS.start[1]));
    stop = Math.min(stop, start - SS.gap);
  }
  return { start, stop };
}

export function curveAt(curve, temp) {
  if (!curve.length) return 0;
  if (temp <= curve[0][0]) return curve[0][1];
  for (let i = 1; i < curve.length; i++) {
    const [t1, f1] = curve[i];
    const [t0, f0] = curve[i - 1];
    if (temp <= t1) return f0 + ((f1 - f0) * (temp - t0)) / Math.max(1e-6, t1 - t0);
  }
  return curve[curve.length - 1][1];
}

/** Lovlig viftefart for punkt i: mellom forrige og neste punkt (kurven faller aldri). */
export function fanBounds(pts, i) {
  const lo = i > 0 ? Math.max(FMIN, pts[i - 1][1]) : FMIN;
  const hi = i < pts.length - 1 ? Math.min(100, pts[i + 1][1]) : 100;
  return [lo, Math.max(lo, hi)];
}

export class FanCurveEditor {
  constructor(canvas, { onChange, onStartStop = () => {} }) {
    this.canvas = canvas;
    this.onChange = onChange;
    this.onStartStop = onStartStop;
    this.ss = { on: false, start: 55, stop: 48 }; // utkastet
    this.rest = null; // hviletilstand nå (regnet her, fra utkastet)
    this.hoverMark = null; // 'start' | 'stop' | 'band'
    this.dragMark = null; // { m, off }
    this.points = [];
    this.sel = -1;
    this.hover = -1;
    this.drag = -1;
    this.temp = null;
    this.tempShown = null;
    this.fan = null;
    this.fanShown = null;
    this.ghost = null; // brukt kurve, vises stiplet mens du redigerer
    this.minFan = FMIN;
    this.grow = []; // vekst per punkt, 0 → 1 (hover/grep)
    this.morph = null; // { from: kurve, k: 0 → 1 } ved forhåndsvalg
    this.needs = true;
    this.sig = '';
    const { ctx, box } = fitCanvas(canvas, () => (this.needs = true));
    this.ctx = ctx;
    this.box = box;
    this.readColors();
    canvas.tabIndex = 0;
    canvas.setAttribute('role', 'application');
    canvas.setAttribute('aria-label', 'Viftekurve. Velg punkt med Tab-tast og flytt med piltastene.');
    this.ariaBase = canvas.getAttribute('aria-label');
    this.bind();
  }

  readColors() {
    this.c = {
      accent: cssVar('--series-1'),
      grid: cssVar('--grid'),
      axis: cssVar('--muted'),
      text: cssVar('--text'),
      muted: cssVar('--muted'),
      panel: cssVar('--field'),
      border: cssVar('--field-border'),
      warn: cssVar('--warn'),
      // Temperatur nå har temperaturens seriefarge; gult betyr bare advarsel (nødregel aktiv).
      temp: cssVar('--c-temp'),
      zone: cssVar('--zone'),
      ghost: cssVar('--series-3'),
      tipBg: cssVar('--tip-bg'),
      tipFg: cssVar('--tip-fg'),
      accentBg: cssVar('--accent-bg'),
      accentFg: cssVar('--accent-fg') || '#fff',
    };
    this.font = cssVar('--font') || 'system-ui, sans-serif';
    this.fsCap = parseFloat(cssVar('--fs-cap')) || 11;
    this.fsSm = parseFloat(cssVar('--fs-sm')) || 12;
    this.needs = true;
  }

  /** animate: kurven morfer fra den forrige (forhåndsvalg, forkast). */
  setPoints(pts, { animate = false } = {}) {
    const next = pts.map((p) => [...p]);
    const same = next.length === this.points.length && next.every((p, i) => p[0] === this.points[i][0] && p[1] === this.points[i][1]);
    if (animate && !same && this.points.length >= 2 && motion() === 'full') this.morph = { from: this.points.map((p) => [...p]), k: 0 };
    this.points = next;
    if (this.sel >= this.points.length) this.sel = -1;
    this.needs = true;
  }

  setGhost(pts) {
    this.ghost = pts ? pts.map((p) => [...p]) : null;
    this.needs = true;
  }

  setLive(temp, fan) {
    this.temp = temp;
    this.fan = fan;
    this.rest = restState(this.rest, temp, this.ss);
  }

  /**
   * Nødregelen i bakgrunnstjenesten: full vifte over `at` °C uansett kurve.
   * Vises som en rolig sone til høyre; gul tekst bare mens den gjelder.
   */
  setEmergency(e) {
    const next = e ? { at: e.at, active: !!e.active } : null;
    if (JSON.stringify(next) === JSON.stringify(this.emg || null)) return;
    this.emg = next;
    this.needs = true;
  }

  /** Start/stopp fra utkastet: { on, start, stop } i °C. */
  setStartStop(ss) {
    const next = { on: !!ss.on, start: ss.start, stop: ss.stop };
    if (next.on === this.ss.on && next.start === this.ss.start && next.stop === this.ss.stop) return;
    this.ss = next;
    if (!next.on && this.sel >= this.points.length) this.sel = -1;
    this.rest = restState(this.rest, this.temp, next);
    this.canvas.setAttribute('aria-label', next.on ? 'Viftekurve. Tab velger punkt, start eller stopp. Piltastene flytter.' : this.ariaBase);
    this.needs = true;
  }

  /** Antall ting Tab går gjennom: punktene, og start og stopp når valget er på. */
  stops() {
    return this.points.length + (this.ss.on ? 2 : 0);
  }

  /** Valgt merke ved tastatur: 'start' | 'stop' | null. */
  selMark() {
    if (!this.ss.on || this.sel < this.points.length) return null;
    return this.sel === this.points.length ? 'start' : 'stop';
  }

  /** Treff på start- eller stopplinja (±7 px) eller båndet mellom dem. */
  hitMark(x, y) {
    if (!this.ss.on) return null;
    const g = this.geom();
    if (y < g.padT || y > g.padT + g.ph + 4 || x < g.padL - 4 || x > g.padL + g.pw + 4) return null;
    const xs = g.X(this.ss.start);
    const xp = g.X(this.ss.stop);
    const ds = Math.abs(x - xs);
    const dp = Math.abs(x - xp);
    if (Math.min(ds, dp) <= 7) return ds <= dp ? 'start' : 'stop';
    if (x > xp && x < xs) return 'band';
    return null;
  }

  /** Flytter start, stopp eller begge (båndet). t i °C. */
  moveMark(m, t) {
    let { start, stop } = this.ss;
    if (m === 'start') ({ start, stop } = clampStartStop(t, stop, 'start'));
    else if (m === 'stop') ({ start, stop } = clampStartStop(start, t, 'stop'));
    else {
      // Båndet: begge flyttes og avstanden beholdes.
      const gap = start - stop;
      stop = Math.round(clamp(t, SS.stop[0], Math.min(SS.stop[1], SS.start[1] - gap)));
      start = stop + gap;
    }
    if (start === this.ss.start && stop === this.ss.stop) return;
    this.ss = { ...this.ss, start, stop };
    this.rest = restState(this.rest, this.temp, this.ss);
    this.morph = null;
    this.needs = true;
    this.onStartStop({ start, stop });
  }

  geom() {
    const { w, h } = this.box;
    const padL = 52;
    const padR = 18;
    const padT = 14;
    const padB = 44;
    const pw = w - padL - padR;
    const ph = h - padT - padB;
    return {
      padL,
      padT,
      pw,
      ph,
      X: (t) => padL + ((t - TMIN) / (TMAX - TMIN)) * pw,
      Y: (f) => padT + ph - (f / 100) * ph,
      T: (x) => TMIN + ((x - padL) / pw) * (TMAX - TMIN),
      F: (y) => ((padT + ph - y) / ph) * 100,
    };
  }

  hit(x, y) {
    const g = this.geom();
    let best = -1;
    let bd = HIT * HIT;
    this.points.forEach(([t, f], i) => {
      const dx = g.X(t) - x;
      const dy = g.Y(f) - y;
      const d = dx * dx + dy * dy;
      if (d <= bd) {
        bd = d;
        best = i;
      }
    });
    return best;
  }

  movePoint(i, t, f) {
    const lo = i > 0 ? this.points[i - 1][0] + 1 : TMIN;
    const hi = i < this.points.length - 1 ? this.points[i + 1][0] - 1 : TMAX;
    // Kurven skal aldri falle: viften går ikke ned når temperaturen stiger.
    const [fLo, fHi] = fanBounds(this.points, i);
    const next = [Math.round(clamp(t, lo, hi)), Math.round(clamp(f, fLo, fHi))];
    const cur = this.points[i];
    if (cur[0] === next[0] && cur[1] === next[1]) return;
    this.points[i] = next;
    this.morph = null;
    this.needs = true;
    this.onChange(this.points);
  }

  bind() {
    const cv = this.canvas;
    const pos = (e) => {
      const r = cv.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    cv.addEventListener('pointerdown', (e) => {
      const [x, y] = pos(e);
      const i = this.hit(x, y);
      const m = i < 0 ? this.hitMark(x, y) : null;
      if (i >= 0) {
        this.drag = i;
        this.sel = i;
        this.shift = e.shiftKey;
        cv.setPointerCapture(e.pointerId);
        cv.classList.add('dragging');
        e.preventDefault();
      } else if (m) {
        // Start, stopp eller båndet. Båndet holdes der du tok tak i det.
        const t = this.geom().T(x);
        this.dragMark = { m, off: m === 'band' ? t - this.ss.stop : 0 };
        this.sel = m === 'stop' ? this.points.length + 1 : m === 'start' ? this.points.length : -1;
        this.shift = e.shiftKey;
        cv.setPointerCapture(e.pointerId);
        cv.classList.add('dragging');
        cv.style.cursor = m === 'band' ? 'grabbing' : 'ew-resize';
        e.preventDefault();
      } else {
        this.sel = -1;
      }
      this.needs = true;
    });
    cv.addEventListener('pointermove', (e) => {
      const [x, y] = pos(e);
      if (this.dragMark) {
        let t = this.geom().T(x) - this.dragMark.off;
        if (e.shiftKey) t = Math.round(t / 5) * 5;
        if (this.shift !== e.shiftKey) {
          this.shift = e.shiftKey;
          this.needs = true;
        }
        this.moveMark(this.dragMark.m, t);
      } else if (this.drag >= 0) {
        const g = this.geom();
        let t = g.T(x);
        let f = g.F(y);
        // Shift smekker til 5 °C / 5 %.
        if (e.shiftKey) {
          t = Math.round(t / 5) * 5;
          f = Math.round(f / 5) * 5;
        }
        if (this.shift !== e.shiftKey) {
          this.shift = e.shiftKey;
          this.needs = true;
        }
        this.movePoint(this.drag, t, f);
      } else {
        const h = this.hit(x, y);
        const hm = h < 0 ? this.hitMark(x, y) : null;
        if (h !== this.hover || hm !== this.hoverMark) {
          this.hover = h;
          this.hoverMark = hm;
          this.needs = true;
          cv.style.cursor = h >= 0 ? 'grab' : hm === 'band' ? 'grab' : hm ? 'ew-resize' : 'crosshair';
        }
      }
    });
    const end = (e) => {
      if (this.dragMark) {
        if (this.dragMark.m === 'band') this.sel = -1;
        this.dragMark = null;
        this.needs = true;
        cv.classList.remove('dragging');
        cv.style.cursor = this.hoverMark === 'band' ? 'grab' : this.hoverMark ? 'ew-resize' : 'crosshair';
        try {
          cv.releasePointerCapture(e.pointerId);
        } catch {}
      }
      if (this.drag >= 0) {
        this.drag = -1;
        this.needs = true;
        cv.classList.remove('dragging');
        try {
          cv.releasePointerCapture(e.pointerId);
        } catch {}
      }
    };
    cv.addEventListener('pointerup', end);
    cv.addEventListener('pointercancel', end);
    cv.addEventListener('pointerleave', () => {
      if (this.drag < 0 && !this.dragMark && (this.hover !== -1 || this.hoverMark)) {
        this.hover = -1;
        this.hoverMark = null;
        this.needs = true;
      }
    });
    cv.addEventListener('dblclick', (e) => {
      const [x, y] = pos(e);
      const i = this.hit(x, y);
      this.needs = true;
      if (i >= 0) {
        if (this.points.length > 2) {
          this.points.splice(i, 1);
          this.sel = -1;
          this.onChange(this.points);
        }
        return;
      }
      if (this.points.length >= 10) return;
      const g = this.geom();
      const t = Math.round(clamp(g.T(x), TMIN, TMAX));
      if (this.points.some((p) => p[0] === t)) return;
      // Nytt punkt legges mellom naboenes viftefart, så kurven fortsatt stiger.
      const below = this.points.filter((p) => p[0] < t).pop();
      const above = this.points.find((p) => p[0] > t);
      const f = Math.round(clamp(g.F(y), below ? below[1] : FMIN, above ? above[1] : 100));
      this.points.push([t, f]);
      this.points.sort((a, b) => a[0] - b[0]);
      this.sel = this.points.findIndex((p) => p[0] === t);
      this.onChange(this.points);
    });
    cv.addEventListener('keydown', (e) => {
      if (!this.points.length) return;
      this.needs = true;
      if (e.key === 'Tab') {
        const n = this.sel + (e.shiftKey ? -1 : 1);
        if (n >= 0 && n < this.stops()) {
          this.sel = n;
          e.preventDefault();
        } else this.sel = -1;
        return;
      }
      if (this.sel < 0) return;
      const step = e.shiftKey ? 5 : 1;
      const mark = this.selMark();
      if (mark) {
        // Start og stopp flyttes langs temperaturaksen (opp/ned gjør det samme).
        const d = { ArrowLeft: -step, ArrowDown: -step, ArrowRight: step, ArrowUp: step }[e.key];
        if (d) {
          this.moveMark(mark, this.ss[mark] + d);
          e.preventDefault();
        }
        return;
      }
      const [t, f] = this.points[this.sel];
      const map = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
      if (map[e.key]) {
        this.movePoint(this.sel, t + map[e.key][0], f + map[e.key][1]);
        e.preventDefault();
      } else if ((e.key === 'Delete' || e.key === 'Backspace') && this.points.length > 2) {
        this.points.splice(this.sel, 1);
        this.sel = -1;
        this.onChange(this.points);
        e.preventDefault();
      }
    });
    cv.addEventListener('focus', () => {
      if (this.sel < 0 && this.points.length) this.sel = 0;
      this.needs = true;
    });
    cv.addEventListener('blur', () => {
      if (this.drag < 0 && !this.dragMark) this.sel = -1;
      this.needs = true;
    });
  }

  /** Kalles hvert bilde fra rAF-løkka; tegner bare når noe har endret seg. */
  tick(dt) {
    const jump = motion() !== 'full';
    // Markøren for temperatur og viftefart glir til ny verdi og låses når den er framme.
    const glide = (shown, want) => (want == null ? shown : shown == null || jump || Math.abs(shown - want) < 0.02 ? want : smooth(shown, want, dt, 0.25));
    const ts = glide(this.tempShown, this.temp);
    const fs = glide(this.fanShown, this.fan);
    if (ts !== this.tempShown || fs !== this.fanShown) {
      this.tempShown = ts;
      this.fanShown = fs;
      this.needs = true;
    }
    // Punktene vokser ved hover og grep (1 → 1,3) over --dur-1.
    const d1 = Math.max(1, dur(1)) / 1000;
    for (let i = 0; i < this.points.length; i++) {
      const want = i === this.hover || i === this.drag || i === this.sel ? 1 : 0;
      const g = this.grow[i] ?? 0;
      if (g !== want) {
        this.grow[i] = jump ? want : want > g ? Math.min(want, g + dt / d1) : Math.max(want, g - dt / d1);
        this.needs = true;
      }
    }
    this.grow.length = this.points.length;
    if (this.morph) {
      this.morph.k = jump ? 1 : Math.min(1, this.morph.k + (dt * 1000) / Math.max(1, dur(4)));
      if (this.morph.k >= 1) this.morph = null;
      this.needs = true;
    }
    // Enheten (°C/°F) og minste fart kan endres utenfra.
    const sig = `${tempLabel()}|${this.minFan}|${this.hoverMark}`;
    if (sig !== this.sig) {
      this.sig = sig;
      this.needs = true;
    }
    // Fortsatt i bevegelse: markøren glir, et punkt vokser eller kurven morfer.
    const busy = !!this.morph || this.grow.some((g, i) => g !== (i === this.hover || i === this.drag || i === this.sel ? 1 : 0)) || (this.temp != null && this.tempShown !== this.temp) || (this.fan != null && this.fanShown !== this.fan);
    if (this.needs) {
      this.needs = false;
      this.draw();
    }
    return busy;
  }

  /** Kurven som tegnes: under morf en blanding av gammel og ny. */
  shape() {
    if (!this.morph) return null;
    const k = 1 - (1 - this.morph.k) ** 3;
    const out = [];
    for (let t = TMIN; t <= TMAX; t += 2) out.push([t, curveAt(this.morph.from, t) + (curveAt(this.points, t) - curveAt(this.morph.from, t)) * k]);
    return out;
  }

  /**
   * «Under 35 % er ikke mulig på kortet»: en rolig merkelapp nede i sonen,
   * på samme linje og i samme form som start og stopp. Den står til venstre
   * for nødregelsonen og tegnes over linjene (temperatur nå, hjelpelinjer),
   * så ingen stiplet linje krysser teksten. Viker for start og stopp: kortere
   * tekst, så venstre hjørne, ellers ingen.
   */
  zoneLabel(g) {
    const { ctx, c } = this;
    const { padL, padT, pw, ph } = g;
    ctx.font = `${this.fsCap}px ${this.font}`;
    const right = (this.emgX ?? padL + pw) - 6;
    const ly = padT + ph - 22;
    const free = (x0, x1) => x0 >= padL + 2 && x1 <= right && !this.ssChips.some(([a, b]) => x0 < b + 4 && x1 > a - 4);
    for (const text of [`Under ${FMIN}${NNBSP}% er ikke mulig på kortet`, `Under ${FMIN}${NNBSP}% går ikke`]) {
      const tw = ctx.measureText(text).width + 12;
      for (const x0 of [right - tw, padL + 6]) {
        if (!free(x0, x0 + tw)) continue;
        const x = Math.round(x0);
        ctx.fillStyle = c.panel;
        ctx.strokeStyle = c.border;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(x + 0.5, ly + 0.5, tw, 17, 4);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = c.axis;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText(text, x + 6.5, ly + 9.5);
        ctx.textBaseline = 'alphabetic';
        this.zoneShown = text;
        return;
      }
    }
    this.zoneShown = null;
  }

  draw() {
    const { ctx, box, c } = this;
    const { w, h, dpr } = box;
    if (w < 10) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const g = this.geom();
    const { padL, padT, pw, ph, X, Y } = g;
    // Hindre som etikettene må holde seg unna (settes under tegningen).
    this.emgText = null;
    this.emgX = null;
    this.ssChips = [];

    ctx.font = `${this.fsCap}px ${this.font}`;
    ctx.lineWidth = 1;
    // Rutenett.
    for (let f = 0; f <= 100; f += 10) {
      const y = Math.round(Y(f)) + 0.5;
      ctx.strokeStyle = c.grid;
      ctx.globalAlpha = f % 20 === 0 ? 1 : 0.5;
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(padL + pw, y);
      ctx.stroke();
      ctx.globalAlpha = 1;
      if (f % 20 === 0) {
        ctx.fillStyle = c.axis;
        ctx.textAlign = 'right';
        ctx.textBaseline = 'middle';
        ctx.fillText(String(f), padL - 8, y);
      }
    }
    for (let t = TMIN; t <= TMAX; t += 10) {
      const x = Math.round(X(t)) + 0.5;
      ctx.strokeStyle = c.grid;
      ctx.beginPath();
      ctx.moveTo(x, padT);
      ctx.lineTo(x, padT + ph);
      ctx.stroke();
      ctx.fillStyle = c.axis;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'alphabetic';
      ctx.fillText(fmt0(tempOut(t)), x, padT + ph + this.fsCap + 6);
    }
    // Aksetitler.
    ctx.fillStyle = c.axis;
    ctx.textAlign = 'center';
    ctx.fillText(`Temperatur (${tempLabel()})`, padL + pw / 2, padT + ph + this.fsCap * 2 + 14);
    ctx.save();
    ctx.translate(12, padT + ph / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText('Vifte (%)', 0, 0);
    ctx.restore();

    // Sone under minste viftefart.
    ctx.fillStyle = c.zone;
    ctx.fillRect(padL, Y(FMIN), pw, Y(0) - Y(FMIN));
    ctx.fillStyle = c.axis;
    ctx.textAlign = 'right';
    ctx.textBaseline = 'alphabetic';
    // Teksten om 35 % tegnes til slutt (zoneLabel), over linjene.
    const ss = this.ss.on ? this.ss : null;
    if (this.minFan > FMIN && !ss) {
      ctx.strokeStyle = c.axis;
      ctx.setLineDash([2, 4]);
      ctx.beginPath();
      ctx.moveTo(padL, Math.round(Y(this.minFan)) + 0.5);
      ctx.lineTo(padL + pw, Math.round(Y(this.minFan)) + 0.5);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.textAlign = 'left';
      ctx.fillText(`Minste fart ${this.minFan}${NNBSP}%`, padL + 8, Y(this.minFan) - 6);
    }

    // Nødregelen: sone fra terskelen og ut, med kort tekst øverst.
    if (this.emg && this.emg.at < TMAX) {
      const xe = Math.round(X(Math.max(TMIN, this.emg.at))) + 0.5;
      const col = this.emg.active ? c.warn : c.muted;
      ctx.fillStyle = withAlpha(col, this.emg.active ? 0.1 : 0.05);
      ctx.fillRect(xe, padT, padL + pw - xe, ph);
      ctx.strokeStyle = withAlpha(col, 0.4);
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(xe, padT);
      ctx.lineTo(xe, padT + ph);
      ctx.stroke();
      ctx.setLineDash([]);
      const room = padL + pw - xe - 12;
      const long = this.emg.active ? 'Nødregel aktiv' : 'Nødregel';
      ctx.font = `${this.fsCap}px ${this.font}`;
      ctx.textAlign = 'right';
      ctx.textBaseline = 'alphabetic';
      ctx.fillStyle = this.emg.active ? c.warn : c.axis;
      // Øverst til høyre: nede til høyre står teksten om 35 %.
      const y = padT + this.fsCap + 6;
      const right = padL + pw - 6;
      if (ctx.measureText(long).width <= room) {
        ctx.fillText(long, right, y);
        ctx.fillText(`100${NNBSP}%`, right, y + this.fsCap + 3);
        this.emgText = { x0: right - ctx.measureText(long).width - 4, x1: right + 4, y1: y + this.fsCap + 6 };
      } else if (ctx.measureText(`100${NNBSP}%`).width <= room) {
        ctx.fillText(`100${NNBSP}%`, right, y);
        this.emgText = { x0: right - ctx.measureText(`100${NNBSP}%`).width - 4, x1: right + 4, y1: y + 3 };
      }
      this.emgX = xe;
    }

    const pts = this.points;
    if (pts.length < 2) return;

    // Start og stopp: skravert bånd mellom linjene (der tilstanden avhenger av
    // om temperaturen stiger eller synker).
    const xs = ss ? X(ss.start) : 0;
    const xp = ss ? X(ss.stop) : 0;
    const mk = this.dragMark?.m || this.selMark() || this.hoverMark;
    if (ss) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(xp, padT, xs - xp, ph);
      ctx.clip();
      ctx.fillStyle = withAlpha(c.muted, mk === 'band' ? 0.1 : 0.06);
      ctx.fillRect(xp, padT, xs - xp, ph);
      ctx.strokeStyle = withAlpha(c.muted, mk === 'band' ? 0.32 : 0.22);
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let k = xp - ph; k < xs; k += 7) {
        ctx.moveTo(k, padT + ph);
        ctx.lineTo(k + ph, padT);
      }
      ctx.stroke();
      ctx.restore();
    }

    // Brukt kurve som stiplet «spøkelse» når den avviker.
    const g0 = this.ghost;
    if (g0 && g0.length >= 2 && !(g0.length === pts.length && g0.every((p, i) => p[0] === pts[i][0] && p[1] === pts[i][1]))) {
      ctx.strokeStyle = c.ghost;
      ctx.lineWidth = 1.5;
      ctx.setLineDash([5, 4]);
      ctx.beginPath();
      ctx.moveTo(padL, Y(g0[0][1]));
      for (const [t, f] of g0) ctx.lineTo(X(t), Y(f));
      ctx.lineTo(padL + pw, Y(g0[g0.length - 1][1]));
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // Under morf tegnes en blanding av gammel og ny kurve.
    const line = this.shape() || pts;
    const morphK = this.morph ? 1 - (1 - this.morph.k) ** 3 : 1;

    const curvePath = () => {
      ctx.beginPath();
      ctx.moveTo(padL, Y(line[0][1]));
      for (const [t, f] of line) ctx.lineTo(X(t), Y(f));
      ctx.lineTo(padL + pw, Y(line[line.length - 1][1]));
    };
    const fillCurve = () => {
      curvePath();
      ctx.lineTo(padL + pw, padT + ph);
      ctx.lineTo(padL, padT + ph);
      ctx.closePath();
      ctx.fill();
    };
    ctx.lineWidth = 2;
    ctx.lineJoin = 'round';
    if (!ss) {
      ctx.fillStyle = withAlpha(c.accent, 0.16);
      fillCurve();
      ctx.strokeStyle = c.accent;
      curvePath();
      ctx.stroke();
    } else {
      // Fra stopp og oppover styrer kurven (til venstre for start bare når
      // temperaturen synker). Under stopp hviler viften på minste fart: den
      // flate linja, og kurven er dempet der.
      const yr = Math.round(Y(Math.max(FMIN, this.minFan))) + 0.5;
      ctx.save();
      ctx.beginPath();
      ctx.rect(xp, padT - 4, padL + pw - xp + 4, ph + 8);
      ctx.clip();
      ctx.fillStyle = withAlpha(c.accent, 0.16);
      fillCurve();
      ctx.strokeStyle = c.accent;
      curvePath();
      ctx.stroke();
      ctx.restore();
      ctx.save();
      ctx.beginPath();
      ctx.rect(padL - 4, padT - 4, xp - padL + 4, ph + 8);
      ctx.clip();
      ctx.fillStyle = withAlpha(c.accent, 0.12);
      ctx.fillRect(padL, yr, xp - padL, padT + ph - yr);
      ctx.strokeStyle = withAlpha(c.accent, 0.35);
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 4]);
      curvePath();
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.restore();
      // Hvilelinja: hel fram til stopp, stiplet i båndet (gjelder på vei opp).
      ctx.strokeStyle = c.accent;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(padL, yr);
      ctx.lineTo(xp, yr);
      ctx.stroke();
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(xp, yr);
      ctx.lineTo(xs, yr);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.font = `${this.fsCap}px ${this.font}`;
      ctx.fillStyle = c.axis;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'alphabetic';
      const restText = `Minste fart ${Math.max(FMIN, this.minFan)}${NNBSP}%`;
      // Under linja (i sonen), så teksten ikke krysser punkter på kurven.
      ctx.textBaseline = 'top';
      if (xp - padL > ctx.measureText(restText).width + 16) ctx.fillText(restText, padL + 8, yr + 5);
      ctx.textBaseline = 'alphabetic';

      // Linjene for start og stopp, med merkelapp nede ved x-aksen.
      ctx.font = `600 ${this.fsCap}px ${this.font}`;
      for (const m of ['stop', 'start']) {
        const x = Math.round(m === 'start' ? xs : xp) + 0.5;
        const on = mk === m || mk === 'band';
        ctx.strokeStyle = on ? c.accent : c.text;
        ctx.globalAlpha = on ? 0.9 : 0.5;
        ctx.lineWidth = on ? 1.5 : 1;
        ctx.setLineDash([4, 3]);
        ctx.beginPath();
        ctx.moveTo(x, padT);
        ctx.lineTo(x, padT + ph);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
        const text = `${m === 'start' ? 'start' : 'stopp'} ${fmt0(tempOut(m === 'start' ? ss.start : ss.stop))}`;
        const tw = ctx.measureText(text).width + 10;
        // Stopp står til venstre for sin linje og start til høyre, så de ikke kolliderer.
        let lx = m === 'start' ? x + 4 : x - 4 - tw;
        lx = clamp(lx, padL + 2, padL + pw - tw - 2);
        const ly = padT + ph - 22;
        this.ssChips.push([lx, lx + tw]);
        ctx.fillStyle = on ? c.accentBg : c.panel;
        ctx.strokeStyle = on ? c.accentBg : c.border;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.roundRect(lx + 0.5, ly + 0.5, tw, 17, 4);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = on ? c.accentFg : c.muted;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText(text, lx + 5.5, ly + 9.5);
      }
      ctx.textBaseline = 'alphabetic';
      // Valgt med tastaturet: tydelig etikett øverst ved linja.
      const kb = this.selMark();
      if (kb && !this.dragMark) {
        const x = kb === 'start' ? xs : xp;
        const label = `${kb === 'start' ? 'Start ved' : 'Stopp under'} ${fmt0(tempOut(ss[kb]))}${NNBSP}${tempLabel()}`;
        ctx.font = `600 ${this.fsSm}px ${this.font}`;
        const tw = ctx.measureText(label).width + 14;
        const lx = clamp(x - tw / 2, padL, padL + pw - tw);
        ctx.fillStyle = c.tipBg;
        ctx.beginPath();
        ctx.roundRect(lx, padT + 30, tw, 20, 5);
        ctx.fill();
        ctx.fillStyle = c.tipFg;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, lx + tw / 2, padT + 40.5);
        ctx.textBaseline = 'alphabetic';
      }
    }

    // Mens du drar: hjelpelinjer til aksene (tettere prikker når Shift smekker).
    if (this.drag >= 0 && pts[this.drag]) {
      const [t, f] = pts[this.drag];
      const x = Math.round(X(t)) + 0.5;
      const y = Math.round(Y(f)) + 0.5;
      ctx.strokeStyle = c.accent;
      ctx.globalAlpha = 0.55;
      ctx.lineWidth = 1;
      ctx.setLineDash(this.shift ? [1, 3] : [3, 3]);
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(x, y);
      ctx.moveTo(x, y);
      ctx.lineTo(x, padT + ph);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
    }

    // Live-markør.
    if (this.tempShown != null) {
      const t = clamp(this.tempShown, TMIN, TMAX);
      const x = X(t);
      const resting = ss && this.rest;
      const f = resting ? Math.max(FMIN, this.minFan) : curveAt(pts, t);
      ctx.strokeStyle = c.temp;
      ctx.globalAlpha = 0.85;
      ctx.setLineDash([3, 4]);
      ctx.lineWidth = 1.25;
      ctx.beginPath();
      ctx.moveTo(x, padT);
      ctx.lineTo(x, padT + ph);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
      // Skjæringspunkt med kurven.
      ctx.fillStyle = c.temp;
      ctx.beginPath();
      ctx.arc(x, Y(f), 3.5, 0, Math.PI * 2);
      ctx.fill();
      // Faktisk viftefart.
      if (this.fanShown != null) {
        ctx.strokeStyle = c.text;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(x, Y(this.fanShown), 5, 0, Math.PI * 2);
        ctx.stroke();
      }
      // Etikett: temperatur → kurvens verdi, og viftefarten kilden melder.
      const short = `${fmt0(tempOut(this.tempShown))}${NNBSP}${tempLabel()}${this.srcLabel ? ` (${this.srcLabel})` : ''} → ${fmt0(Math.max(this.minFan, f))}${NNBSP}%${resting ? ' (hvile)' : ''}`;
      let label = this.fanShown != null ? `${short} · vifte ${fmt0(this.fanShown)}${NNBSP}%` : short;
      ctx.font = `600 ${this.fsSm}px ${this.font}`;
      let tw = ctx.measureText(label).width + 16;
      // Smalt plott: bare temperatur og kurvens verdi (ringen viser viftefarten).
      if (tw > pw - 16) {
        label = short;
        tw = ctx.measureText(label).width + 16;
      }
      // Til høyre for linja, ellers til venstre. Teksten «Nødregel 100 %»
      // øverst til høyre er et hinder på samme måte som kanten; går ingen
      // av sidene, flyttes etiketten ned under den.
      const emg = this.emgText;
      const hits = (a) => emg && a < emg.x1 && a + tw > emg.x0;
      let lx = x + 8;
      let ly = padT + 4;
      if (lx + tw > padL + pw || hits(lx)) lx = x - 8 - tw;
      lx = clamp(lx, padL, Math.max(padL, padL + pw - tw));
      if (hits(lx)) ly = emg.y1 + 4;
      ctx.fillStyle = c.tipBg;
      ctx.beginPath();
      ctx.roundRect(lx, ly, tw, 22, 5);
      ctx.fill();
      ctx.fillStyle = c.tipFg;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      ctx.fillText(label, lx + 8, ly + 11);
    }

    this.zoneLabel(g);

    // Punkter. Under morf toner de nye punktene inn.
    ctx.globalAlpha = morphK;
    pts.forEach(([t, f], i) => {
      const x = X(t);
      const y = Y(f);
      const active = i === this.sel || i === this.drag;
      const g = this.grow[i] ?? 0; // 0 → 1
      if (g > 0) {
        ctx.fillStyle = withAlpha(c.accent, (active ? 0.2 : 0.12) * g);
        ctx.beginPath();
        ctx.arc(x, y, 6 + 6 * g, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.fillStyle = c.panel;
      ctx.strokeStyle = c.accent;
      ctx.lineWidth = 2.25;
      ctx.beginPath();
      ctx.arc(x, y, 6 * (1 + 0.3 * g), 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      if (active) {
        const label = `${fmt0(tempOut(t))}${NNBSP}${tempLabel()} · ${f}${NNBSP}%`;
        ctx.font = `600 ${this.fsSm}px ${this.font}`;
        const tw = ctx.measureText(label).width + 14;
        let lx = x - tw / 2;
        lx = clamp(lx, padL, padL + pw - tw);
        const ly = y - 34 < padT ? y + 14 : y - 34;
        ctx.fillStyle = c.tipBg;
        ctx.beginPath();
        ctx.roundRect(lx, ly, tw, 20, 5);
        ctx.fill();
        ctx.fillStyle = c.tipFg;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, lx + tw / 2, ly + 10.5);
      }
    });
    ctx.globalAlpha = 1;
  }
}
