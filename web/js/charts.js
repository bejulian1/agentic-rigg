// Linjegrafer på canvas (v0.0.11: fra 1 min til 48 t).
//
// Kilden er en liste med punkter { t, v: { nøkkel: snitt }, lo?, hi? } fra en
// HistoryView (history.js): serverens historikk pluss de levende prøvene.
// Lange vinduer har min og maks per bøtte, og da tegnes et svakt bånd rundt
// snittlinja, så topper ikke forsvinner. Det tegnes aldri flere punkter enn
// plottet er bredt i piksler: er det flere, slås de sammen per pikselkolonne
// (snitt for linja, laveste min og høyeste maks for båndet).
//
// Tidsaksen følger vinduet: «−45 s» opptil 5 min, klokkeslett (14:30) for
// lengre vinduer, og dato der døgnet skifter. Der det mangler data (serveren
// var av, ingen fane var åpen), brytes linja, og flaten er skravert. Til
// venstre står hvor langt tilbake dataene går («Data fra i dag 16:43»).
//
// Rulling: grafen tegnes bare når tidsaksen har flyttet seg ⅓ fysisk piksel
// eller nye data har kommet, så en 240 Hz-skjerm ikke gir mer arbeid. Når
// y-aksen skalerer om («Tilpass»), glir den til nytt område. Med data-motion
// 'reduced'/'off' tegnes grafen trinnvis, og aksen hopper.

import { cssVar, fitCanvas, clamp, motion, smooth, wakeIn } from './util.js';
import { ago, dayShort, num, NNBSP, time, when } from './fmt.js';

/** Pene aksemerker (1-2-2,5-5 × 10ⁿ). Gir { lo, hi, step }. */
export function niceScale(min, max, maxTicks = 5) {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { lo: 0, hi: 1, step: 0.25 };
  if (max - min < 1e-9) {
    const pad = Math.abs(max) > 1 ? Math.abs(max) * 0.1 : 1;
    min -= pad;
    max += pad;
  }
  const raw = (max - min) / Math.max(1, maxTicks - 1);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const f = raw / mag;
  const nice = f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10;
  const step = nice * mag;
  return { lo: Math.floor(min / step + 1e-9) * step, hi: Math.ceil(max / step - 1e-9) * step, step };
}

const REL_STEPS = { 60: 15, 300: 60 };
const CLOCK_STEPS = [60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200].map((s) => s * 1000);
// Lengre opphold enn dette mellom to levende prøver gir brudd i linjen.
export const GAP_MS = 5000;

/** Serie: nøkkel i punktet, valgfri omregning (°F, MB/s). */
export function hs(key, { label, color, map = (x) => x, fill = false, dashed = false, step = false, band = true, fmt = null } = {}) {
  const val = (o) => {
    const x = o?.[key];
    return x == null || !Number.isFinite(x) ? null : map(x);
  };
  return { key, label, color, fill, dashed, step, band, fmt, get: (p) => val(p.v), lo: (p) => (p.lo ? val(p.lo) : null), hi: (p) => (p.hi ? val(p.hi) : null) };
}

export class LineChart {
  /**
   * opts:
   *   view:     HistoryView (punkter, versjon, hull, «data fra»)
   *   series:   [hs(...)] – color er et token (c-temp, c-load …)
   *   range:    () => [min, maks] – fast område (og yttergrenser for «tilpass»)
   *   minSpan:  () => minste spenn på y-aksen ved «tilpass»
   *   scale:    () => 'auto' | 'fixed'
   *   fill:     () => bool – fylt flate under linjer med fill:true
   *   unit:     () => enhet til verktøytipset
   *   lagMs:    hvor langt bak «nå» grafen tegnes (glatt innløp)
   *   decimals, sync ({ t }), syncOn: () => bool
   */
  constructor(canvas, opts) {
    this.canvas = canvas;
    this.o = { scale: () => 'fixed', fill: () => true, unit: () => '', lagMs: 590, decimals: 0, syncOn: () => true, ...opts };
    this.view = opts.view;
    this.paused = false;
    this.pausedAt = 0;
    this.snap = null;
    this.hoverX = null;
    this.sync = opts.sync || null;
    this.hidden = new Set(); // serier som er skjult fra forklaringen
    this.lastVersion = -1;
    this.needs = true;
    this.lastDraw = 0;
    this.lastPw = 0;
    this.ys = null;
    this.yAnim = false;
    const { ctx, box } = fitCanvas(canvas, (b) => {
      this.needs = true;
      this.view?.setPoints?.(b.w);
    });
    this.ctx = ctx;
    this.box = box;
    this.readColors();
    canvas.addEventListener('pointermove', (e) => {
      const r = canvas.getBoundingClientRect();
      this.hoverX = e.clientX - r.left;
      this.needs = true;
    });
    canvas.addEventListener('pointerleave', () => {
      this.hoverX = null;
      if (this.sync) this.sync.t = null;
      this.needs = true;
    });
  }

  get windowSec() {
    return this.view?.window ?? 60;
  }

  readColors() {
    this.c = {
      grid: cssVar('--grid'),
      axis: cssVar('--muted'),
      dim: cssVar('--dim'),
      text: cssVar('--text'),
      hatch: cssVar('--hatch'),
      tipBg: cssVar('--tip-bg'),
      tipFg: cssVar('--tip-fg'),
    };
    this.cols = new Map();
    for (const s of this.series()) this.cols.set(s, cssVar(`--${s.color}`) || cssVar('--pal-blue'));
    this.hc = document.documentElement.dataset.contrast === 'high';
    this.font = cssVar('--font') || 'system-ui, sans-serif';
    this.fsCap = parseFloat(cssVar('--fs-cap')) || 11;
    this.fsSm = parseFloat(cssVar('--fs-sm')) || 12;
    this.needs = true;
  }

  color(s) {
    return this.cols.get(s) || cssVar(`--${s.color}`);
  }

  series() {
    return typeof this.o.series === 'function' ? this.o.series() : this.o.series;
  }

  visibleSeries() {
    return this.series().filter((s) => !this.hidden.has(s.key));
  }

  setHidden(key, hide) {
    if (hide) this.hidden.add(key);
    else this.hidden.delete(key);
    this.needs = true;
  }

  setPaused(p) {
    this.paused = p;
    if (p) {
      this.pausedAt = this.now();
      this.snap = this.points().slice();
    } else this.snap = null;
    this.needs = true;
  }

  points() {
    return this.view?.data || [];
  }

  now() {
    return Date.now() - this.o.lagMs;
  }

  syncOn() {
    return !!this.sync && this.o.syncOn() !== false;
  }

  /** Tegner hvis noe har endret seg. Kalles fra rAF-løkka. Returnerer true
   *  når grafen vil ha neste bilde (aksen glir). Ellers bestiller den selv en
   *  vekking når rullingen har kommet ⅓ piksel, så løkka kan sove imens. */
  frame() {
    const ver = this.view?.version ?? 0;
    const fresh = ver !== this.lastVersion;
    const syncHover = this.syncOn() && this.sync.t !== this.lastSync;
    const smoothScroll = motion() === 'full' && !this.paused;
    const now = performance.now();
    const pxPerMs = ((this.lastPw || 600) * (this.box.dpr || 1)) / (this.windowSec * 1000);
    const stepMs = 1 / 3 / pxPerMs;
    if (!fresh && !this.needs && !syncHover && !this.yAnim) {
      if (!smoothScroll) return false;
      const wait = this.lastDraw + stepMs - now;
      if (wait > 4) {
        wakeIn(Math.min(wait, 60_000));
        return false;
      }
    }
    this.lastVersion = ver;
    this.lastSync = this.sync?.t;
    this.needs = false;
    const dt = this.lastDraw ? Math.min(0.1, (now - this.lastDraw) / 1000) : 0;
    this.lastDraw = now;
    this.draw(dt);
    if (this.yAnim) return true;
    if (smoothScroll) wakeIn(Math.min(stepMs, 60_000));
    return false;
  }

  /** Glir det viste y-området mot målet (bare ved full bevegelse). */
  easeY(tgt, dt) {
    const ys = this.ys;
    if (!ys || motion() !== 'full' || !dt || (ys.lo === tgt.lo && ys.hi === tgt.hi)) {
      this.ys = { lo: tgt.lo, hi: tgt.hi };
      this.yAnim = false;
      return this.ys;
    }
    ys.lo = smooth(ys.lo, tgt.lo, dt, 0.08);
    ys.hi = smooth(ys.hi, tgt.hi, dt, 0.08);
    const eps = (tgt.hi - tgt.lo) * 0.002;
    if (Math.abs(ys.lo - tgt.lo) < eps && Math.abs(ys.hi - tgt.hi) < eps) {
      ys.lo = tgt.lo;
      ys.hi = tgt.hi;
    }
    this.yAnim = ys.lo !== tgt.lo || ys.hi !== tgt.hi;
    return ys;
  }

  yRange(hist, start, tNow, t0) {
    const [fmin, fmax] = this.o.range();
    if (this.o.scale() !== 'auto') return niceScale(fmin, fmax, 5);
    let lo = Infinity;
    let hi = -Infinity;
    for (let i = start; i < hist.length; i++) {
      const p = hist[i];
      if (p.t < t0 - 2000 || p.t > tNow + 2000) continue;
      for (const s of this.visibleSeries()) {
        const a = s.band ? s.lo(p) ?? s.get(p) : s.get(p);
        const b = s.band ? s.hi(p) ?? s.get(p) : s.get(p);
        if (a != null && a < lo) lo = a;
        if (b != null && b > hi) hi = b;
      }
    }
    if (!Number.isFinite(lo)) return niceScale(fmin, fmax, 5);
    // Minste spenn: en nesten flat kurve (f.eks. 38–40 °C) skal se flat ut.
    const minSpan = Math.min(fmax - fmin, this.o.minSpan ? this.o.minSpan() : 0);
    if (hi - lo < minSpan) {
      const mid = (hi + lo) / 2;
      lo = mid - minSpan / 2;
      hi = mid + minSpan / 2;
      if (lo < fmin) [lo, hi] = [fmin, hi + (fmin - lo)];
      if (hi > fmax) [lo, hi] = [lo - (hi - fmax), fmax];
      const n = niceScale(Math.max(fmin, lo), hi, 5);
      return { lo: Math.max(fmin, n.lo), hi: Math.min(fmax, n.hi), step: n.step };
    }
    const pad = Math.max((hi - lo) * 0.12, (fmax - fmin) * 0.03);
    lo = Math.max(fmin, lo - pad);
    hi = Math.min(fmax, hi + pad);
    const n = niceScale(lo, hi, 5);
    return { lo: Math.max(fmin, n.lo), hi: Math.min(fmax, n.hi), step: n.step };
  }

  draw(dt = 0) {
    const { ctx, box, c } = this;
    const { w, h, dpr } = box;
    if (w < 8 || h < 8) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const hist = this.paused && this.snap ? this.snap : this.points();
    const tNow = this.paused ? this.pausedAt : this.now();
    const win = this.windowSec;
    const span = win * 1000;
    const t0 = tNow - span;
    const gapMs = this.view?.gapMs ?? GAP_MS;
    let lo = 0;
    let hi = hist.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if (hist[m].t < t0 - gapMs) lo = m + 1;
      else hi = m;
    }
    const start = lo;

    const yr = this.yRange(hist, start, tNow, t0);
    const dec = yr.step < 1 ? (yr.step < 0.1 ? 2 : 1) : 0;
    const ys = this.easeY(yr, dt);
    ctx.font = `${this.fsCap}px ${this.font}`;
    // Minst plass til «0 000», så plottene i samme kolonne starter likt.
    const labelW = Math.max(ctx.measureText(num(8888)).width, ctx.measureText(num(yr.hi, dec)).width, ctx.measureText(num(yr.lo, dec)).width);
    const padL = Math.ceil(labelW) + 12;
    const padR = 8;
    const padT = 8;
    const padB = this.fsCap + 10;
    const pw = w - padL - padR;
    const ph = h - padT - padB;
    if (pw < 20 || ph < 20) return;
    this.lastPw = pw;
    const X = (t) => padL + ((t - t0) / span) * pw;
    const Y = (v) => padT + ph - ((clamp(v, ys.lo, ys.hi) - ys.lo) / (ys.hi - ys.lo || 1)) * ph;

    // Y-akse og vannrette linjer.
    ctx.lineWidth = 1;
    ctx.textBaseline = 'middle';
    ctx.textAlign = 'right';
    const v0 = Math.ceil(ys.lo / yr.step - 1e-6) * yr.step;
    for (let v = v0; v <= ys.hi + yr.step * 0.001; v += yr.step) {
      const y = Math.round(Y(v)) + 0.5;
      ctx.strokeStyle = c.grid;
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(w - padR, y);
      ctx.stroke();
      ctx.fillStyle = c.axis;
      ctx.fillText(num(v, dec), padL - 6, y);
    }

    // Tidsakse.
    this.drawTimeAxis(t0, tNow, span, padL, padT, pw, ph, w, h, padR, X);

    // Hull: skravert flate før første punkt og der det mangler data.
    const holes = [];
    const firstT = start < hist.length ? Math.max(t0, this.view?.stepMs > 1000 ? hist[start].t - this.view.stepMs / 2 : hist[start].t) : null;
    if (firstT == null) holes.push([t0, tNow, 'lead']);
    else if (firstT - t0 > Math.min(gapMs, span * 0.01)) holes.push([t0, firstT, 'lead']);
    for (let i = Math.max(start, 1); i < hist.length; i++) {
      const a = hist[i - 1].t;
      const b = hist[i].t;
      if (b - a > gapMs && b > t0 && a < tNow) holes.push([Math.max(a, t0), Math.min(b, tNow), 'gap']);
    }
    for (const [a, b, kind] of holes) {
      const xa = Math.max(padL, X(a));
      const xb = Math.min(padL + pw, X(b));
      if (xb - xa < 1) continue;
      ctx.fillStyle = c.hatch;
      ctx.fillRect(xa, padT, xb - xa, ph);
      let label = null;
      if (kind === 'lead') {
        if (this.view?.loading && !hist.length) label = 'Henter historikk …';
        else if (firstT == null) label = this.view?.error ? 'Kunne ikke hente historikk' : 'Ingen data ennå';
        else label = win > 300 ? `Data fra ${when(firstT)}` : 'Samler data …';
      } else label = 'Ingen data';
      ctx.font = `${this.fsSm}px ${this.font}`;
      const tw = ctx.measureText(label).width;
      if (xb - xa > tw + 16) {
        ctx.fillStyle = c.dim;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, xa + (xb - xa) / 2, padT + ph / 2);
      }
    }
    ctx.font = `${this.fsCap}px ${this.font}`;

    ctx.save();
    ctx.beginPath();
    ctx.rect(padL, padT - 2, pw, ph + 4);
    ctx.clip();
    const fillOn = this.o.fill() && yr.lo <= this.o.range()[0] + 1e-9;
    const hc = this.hc;
    for (const s of this.visibleSeries()) {
      const col = this.color(s);
      const segs = this.segments(hist, start, s, X, Y, tNow, gapMs, pw);
      for (const seg of segs) {
        const pts = seg.line;
        if (pts.length < 2) continue;
        // Min/maks-bånd (lange vinduer): svakt, under linja.
        if (s.band && seg.band.length >= 4) {
          ctx.fillStyle = withAlpha(col, hc ? 0.28 : 0.2);
          ctx.beginPath();
          const bnd = seg.band;
          ctx.moveTo(bnd[0], bnd[1]);
          for (let i = 0; i < bnd.length; i += 3) ctx.lineTo(bnd[i], bnd[i + 2]);
          for (let i = bnd.length - 3; i >= 0; i -= 3) ctx.lineTo(bnd[i], bnd[i + 1]);
          ctx.closePath();
          ctx.fill();
        }
        if (s.fill && fillOn && !(s.band && seg.band.length >= 4)) {
          ctx.fillStyle = withAlpha(col, hc ? 0.22 : 0.14);
          ctx.beginPath();
          ctx.moveTo(pts[0], padT + ph);
          for (let i = 0; i < pts.length; i += 2) ctx.lineTo(pts[i], pts[i + 1]);
          ctx.lineTo(pts[pts.length - 2], padT + ph);
          ctx.closePath();
          ctx.fill();
        }
        ctx.strokeStyle = col;
        ctx.lineWidth = (s.dashed ? 1.25 : 1.75) * (hc ? 1.4 : 1);
        ctx.lineJoin = s.step ? 'miter' : 'round';
        ctx.setLineDash(s.dashed ? [4, 4] : []);
        ctx.beginPath();
        if (pts.length === 2) {
          // Ett enkelt punkt: en kort strek, så det synes.
          ctx.moveTo(pts[0] - 2, pts[1]);
          ctx.lineTo(pts[0] + 2, pts[1]);
        } else {
          ctx.moveTo(pts[0], pts[1]);
          for (let i = 2; i < pts.length; i += 2) ctx.lineTo(pts[i], pts[i + 1]);
        }
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }
    ctx.restore();
    this.drawHover(hist, start, t0, tNow, span, padL, padT, pw, ph, w, padR, X, Y, gapMs);
  }

  /**
   * Linje (og bånd) for én serie, delt der det mangler data. Er det flere
   * punkter enn piksler, slås de sammen per pikselkolonne.
   * Gir [{ line: [x, y, …], band: [x, yHi, yLo, …] }].
   */
  segments(hist, start, s, X, Y, tNow, gapMs, pw) {
    const out = [];
    let line = [];
    let band = [];
    let prevT = null;
    const dense = hist.length - start > pw * 1.2;
    let col = null; // { x, sum, n, lo, hi }
    const flushCol = () => {
      if (!col) return;
      const v = col.sum / col.n;
      line.push(col.x, s.step ? Math.round(Y(v)) + 0.5 : Y(v));
      if (Number.isFinite(col.lo)) band.push(col.x, Y(col.hi), Y(col.lo));
      col = null;
    };
    const cut = () => {
      flushCol();
      if (line.length) out.push({ line, band });
      line = [];
      band = [];
    };
    for (let i = start; i < hist.length; i++) {
      const p = hist[i];
      const v = s.get(p);
      if (v == null) continue;
      if (prevT != null && p.t - prevT > gapMs) cut();
      if (p.t > tNow) {
        // Siste stykke fram til «nå» interpoleres for jevn rulling.
        const prev = hist[i - 1];
        const pv = prev ? s.get(prev) : null;
        if (line.length && prev && pv != null && prev.t <= tNow && p.t - prev.t <= gapMs) {
          flushCol();
          const f = (tNow - prev.t) / (p.t - prev.t);
          line.push(X(tNow), s.step ? Math.round(Y(pv)) + 0.5 : Y(pv + (v - pv) * f));
        }
        break;
      }
      prevT = p.t;
      const x = X(p.t);
      const lo = s.band ? s.lo(p) : null;
      const hi = s.band ? s.hi(p) : null;
      if (dense) {
        const cx = Math.round(x);
        if (col && col.x !== cx) flushCol();
        if (!col) col = { x: cx, sum: 0, n: 0, lo: Infinity, hi: -Infinity };
        col.sum += v;
        col.n++;
        const a = lo ?? v;
        const b = hi ?? v;
        if (a < col.lo) col.lo = a;
        if (b > col.hi) col.hi = b;
        if (lo == null && hi == null && col.n === 1) {
          col.lo = v;
          col.hi = v;
        }
        continue;
      }
      if (s.step) {
        // Trappetrinn: verdien holdes til neste prøve og bytter loddrett.
        const y = Math.round(Y(v)) + 0.5;
        if (line.length >= 2 && line[line.length - 1] !== y) line.push(x, line[line.length - 1]);
        line.push(x, y);
      } else line.push(x, Y(v));
      if (lo != null && hi != null) band.push(x, Y(hi), Y(lo));
    }
    cut();
    // Uten båndpunkter (korte vinduer) er det ingen bånd.
    for (const sg of out) if (!dense && sg.band.length < sg.line.length * 1.5 - 3) sg.band = [];
    return out;
  }

  drawTimeAxis(t0, tNow, span, padL, padT, pw, ph, w, h, padR, X) {
    const { ctx, c } = this;
    const win = this.windowSec;
    ctx.textBaseline = 'alphabetic';
    ctx.fillStyle = c.axis;
    const yText = h - 4;
    const vline = (x) => {
      ctx.strokeStyle = c.grid;
      ctx.beginPath();
      ctx.moveTo(Math.round(x) + 0.5, padT);
      ctx.lineTo(Math.round(x) + 0.5, padT + ph);
      ctx.stroke();
    };
    // «nå» (eller «pauset») helt til høyre.
    ctx.textAlign = 'right';
    const nowLabel = this.paused ? `pauset ${time(new Date(this.pausedAt), { seconds: win <= 300 })}` : 'nå';
    ctx.fillText(nowLabel, w - padR, yText);
    const reserve = ctx.measureText(nowLabel).width + 16;
    if (win <= 300) {
      // Relativt: «−45 s», «−4 min».
      const step = REL_STEPS[win] || Math.max(15, Math.round(win / 4));
      ctx.textAlign = 'center';
      for (let s = step; s < win; s += step) {
        const x = padL + pw - (s / win) * pw;
        vline(x);
        if (x < w - padR - reserve) ctx.fillText(ago(s, win), x, yText);
      }
      return;
    }
    // Klokkeslett på hele tidspunkter, og dato der døgnet skifter.
    const maxTicks = Math.max(2, Math.floor(pw / 64));
    const stepMs = CLOCK_STEPS.find((s) => span / s <= maxTicks) || CLOCK_STEPS[CLOCK_STEPS.length - 1];
    const tzo = new Date(t0).getTimezoneOffset() * 60_000;
    let tk = Math.ceil((t0 - tzo) / stepMs) * stepMs + tzo;
    ctx.textAlign = 'center';
    for (; tk < tNow; tk += stepMs) {
      const x = X(tk);
      if (x < padL + 10) continue;
      vline(x);
      if (x > w - padR - reserve) continue;
      const d = new Date(tk);
      const midnight = d.getHours() === 0 && d.getMinutes() === 0;
      ctx.fillText(midnight ? dayShort(d) : time(d, { seconds: false }), x, yText);
    }
  }

  drawHover(hist, start, t0, tNow, span, padL, padT, pw, ph, w, padR, X, Y, gapMs) {
    const { ctx, c } = this;
    let th = null;
    const own = this.hoverX != null && this.hoverX >= padL && this.hoverX <= w - padR;
    if (own) {
      th = t0 + ((this.hoverX - padL) / pw) * span;
      if (this.syncOn()) this.sync.t = th;
    } else if (this.syncOn() && this.sync.t != null) th = this.sync.t;
    if (th == null || !hist.length) return;
    let a = start;
    let b = hist.length - 1;
    while (a < b) {
      const m = (a + b) >> 1;
      if (hist[m].t < th) a = m + 1;
      else b = m;
    }
    let best = null;
    let bd = Infinity;
    for (const i of [a - 1, a]) {
      const p = hist[i];
      if (!p || p.t > tNow) continue;
      const d = Math.abs(p.t - th);
      if (d < bd) {
        bd = d;
        best = p;
      }
    }
    if (!best || bd > Math.max(3000, gapMs / 2)) return;
    const x = Math.round(X(best.t)) + 0.5;
    ctx.strokeStyle = c.axis;
    ctx.setLineDash([2, 3]);
    ctx.beginPath();
    ctx.moveTo(x, padT);
    ctx.lineTo(x, padT + ph);
    ctx.stroke();
    ctx.setLineDash([]);
    const vis = this.visibleSeries();
    for (const s of vis) {
      const v = s.get(best);
      if (v == null) continue;
      ctx.fillStyle = this.color(s);
      ctx.beginPath();
      ctx.arc(x, Y(v), 3, 0, Math.PI * 2);
      ctx.fill();
    }
    if (!own) return;
    // Verktøytips (bare på grafen pekeren er over): tidspunkt og verdier.
    const unit = this.o.unit();
    const f = (s, v) => (s.fmt ? s.fmt(v) : `${num(v, this.o.decimals)}${unit ? NNBSP + unit : ''}`);
    const lines = vis
      .filter((s) => s.get(best) != null)
      .map((s) => {
        const v = s.get(best);
        const lo = s.band ? s.lo(best) : null;
        const hi = s.band ? s.hi(best) : null;
        const range = lo != null && hi != null && hi - lo > Math.abs(v) * 0.005 ? ` (${num(lo, this.o.decimals)}–${num(hi, this.o.decimals)})` : '';
        return { col: this.color(s), text: `${s.label}: ${f(s, v)}${range}` };
      });
    const win = this.windowSec;
    const secAgo = Math.max(0, Math.round((tNow - best.t) / 1000));
    const stepMs = this.view?.stepMs || 1000;
    const head = win <= 300 && stepMs <= 1000 ? (secAgo === 0 ? 'nå' : `for ${secAgo}${NNBSP}s siden`) : `${when(best.t - (stepMs > 1000 ? stepMs / 2 : 0), { seconds: stepMs < 60_000 })}${stepMs > 1000 ? ` · snitt over ${stepMs >= 60_000 ? `${stepMs / 60_000}${NNBSP}min` : `${stepMs / 1000}${NNBSP}s`}` : ''}`;
    lines.unshift({ col: null, text: head });
    ctx.font = `${this.fsSm}px ${this.font}`;
    const lh = this.fsSm + 6;
    const tw = Math.max(...lines.map((l) => ctx.measureText(l.text).width)) + 26;
    const tH = lines.length * lh + 10;
    let bx = x + 10;
    if (bx + tw > w - padR) bx = x - 10 - tw;
    bx = Math.max(2, bx);
    const by = padT + 4;
    ctx.fillStyle = c.tipBg;
    roundRect(ctx, bx, by, tw, tH, 6);
    ctx.fill();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    lines.forEach((l, i) => {
      const yy = by + 5 + lh * i + lh / 2;
      if (l.col) {
        ctx.fillStyle = l.col;
        ctx.fillRect(bx + 9, yy - 1, 8, 2);
      }
      ctx.fillStyle = c.tipFg;
      ctx.globalAlpha = i === 0 ? 0.75 : 1;
      ctx.fillText(l.text, bx + (l.col ? 22 : 9), yy);
      ctx.globalAlpha = 1;
    });
  }
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

const alphaCache = new Map();
/** Gir rgba() for en #hex- eller rgb()-farge. */
export function withAlpha(col, a) {
  const key = `${col}|${a}`;
  let v = alphaCache.get(key);
  if (v) return v;
  let r = 255;
  let g = 255;
  let b = 255;
  if (col.startsWith('#')) {
    const hex = col.length === 4 ? col.replace(/#(.)(.)(.)/, '#$1$1$2$2$3$3') : col;
    r = parseInt(hex.slice(1, 3), 16);
    g = parseInt(hex.slice(3, 5), 16);
    b = parseInt(hex.slice(5, 7), 16);
  } else {
    const m = col.match(/[\d.]+/g);
    if (m) [r, g, b] = m.map(Number);
  }
  v = `rgba(${r}, ${g}, ${b}, ${a})`;
  alphaCache.set(key, v);
  return v;
}
