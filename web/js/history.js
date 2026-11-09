// Historikk for grafene (v0.0.11). En HistoryView henter et tidsvindu fra
// serveren (/api/history, min/snitt/maks per bøtte) og fletter inn de levende
// prøvene fra SSE-strømmen, så grafen oppdateres som før. Punktene er
// { t, v: { nøkkel: snitt }, lo?: { nøkkel: min }, hi?: { nøkkel: maks } }.
//
// Korte vinduer (1 s-bøtter) får de levende prøvene som de er. Lengre vinduer
// samler dem i bøtter med samme bredde som serverens svar, så antall punkter
// holder seg på omtrent det grafen har plass til. Den siste, åpne bøtta
// oppdateres mens den fylles.
//
// Nøklene er de samme som på serveren (server/history.js): gpu.temp,
// gpu.power, cpu.load, cpu.t0, mem.usedPct, net.rx, hw.<id> …

import { cpuTemp, memUsedPct, netTotals } from './sensormeta.js';

/** Tidsvinduene brukeren kan velge (sekunder, kort navn). */
export const WINDOWS = [
  [60, '1\u202fmin'],
  [300, '5\u202fmin'],
  [900, '15\u202fmin'],
  [3600, '1\u202ft'],
  [21600, '6\u202ft'],
  [86400, '24\u202ft'],
  [172800, '48\u202ft'],
];
export const windowLabel = (sec) => WINDOWS.find((w) => w[0] === sec)?.[1] || `${Math.round(sec / 60)}\u202fmin`;
/** Lange tidsvinduer får klokkeslett på aksen og min/maks-bånd. */
export const isLong = (sec) => sec > 300;

const GPU_FIELDS = ['temp', 'gpuUtil', 'memUtil', 'encUtil', 'decUtil', 'coreClock', 'smClock', 'memClock', 'videoClock', 'power', 'powerLimit', 'fan', 'vramUsed', 'pcieGen'];

/** GPU-prøven som nøkler (samme navn som serveren bruker for kilden som er i bruk). */
export function gpuFlat(s) {
  const out = {};
  for (const f of GPU_FIELDS) out[`gpu.${f}`] = s[f];
  return out;
}

/** Systemprøven som nøkler (samme regler som server/history.js, sysValues). */
export function sysFlat(s) {
  const out = {};
  out['cpu.temp'] = cpuTemp(s);
  if (s.cpu) {
    out['cpu.load'] = s.cpu.load;
    (s.cpu.coreLoad || []).forEach((v, i) => (out[`cpu.t${i}`] = v));
  }
  const m = s.mem || {};
  const GiB = 1024 * 1024;
  out['mem.usedPct'] = memUsedPct(s);
  out['mem.used'] = m.usedKB == null ? null : m.usedKB / GiB;
  out['mem.avail'] = m.availableKB == null ? null : m.availableKB / GiB;
  out['mem.swap'] = m.swapUsedKB == null ? null : m.swapUsedKB / GiB;
  const nt = netTotals(s);
  out['net.rx'] = nt.rx;
  out['net.tx'] = nt.tx;
  for (const [name, n] of Object.entries(s.net || {})) {
    out[`net.${name}.rx`] = n.rx;
    out[`net.${name}.tx`] = n.tx;
  }
  for (const d of s.hwmon || []) for (const x of d.sensors || []) out[`hw.${x.id}`] = x.value;
  return out;
}

export class HistoryView {
  /**
   * feed: LiveFeed. keys: nøkler. window: sekunder. points: høyst antall
   * bøtter fra serveren (grafens bredde). fetchFn: (query) => Promise<svar>.
   */
  constructor(feed, { keys, window, points = 800, fetchFn }) {
    this.feed = feed;
    this.keys = keys.slice();
    this.window = window;
    this.points = points;
    this.fetchFn = fetchFn;
    this.data = [];
    this.stepMs = 1000;
    this.version = 0;
    this.loading = false;
    this.error = null;
    this.oldest = null; // eldste data på serveren (ms)
    this.loadedAt = 0;
    this.seq = 0;
    this.gpuKeys = this.keys.filter((k) => k.startsWith('gpu.'));
    this.sysKeys = this.keys.filter((k) => !k.startsWith('gpu.'));
    this.offs = [];
    if (this.gpuKeys.length) this.offs.push(feed.on('sample', (s) => this.live(s.t || Date.now(), gpuFlat(s), this.gpuKeys)));
    if (this.sysKeys.length) this.offs.push(feed.on('sys', (s) => this.live(s.t || Date.now(), sysFlat(s), this.sysKeys)));
    // Ny tilkobling eller ny kilde (live/simulert): hent på nytt, så hull fylles.
    this.offs.push(feed.on('hello', () => this.load()));
    this.load();
  }

  get gapMs() {
    return Math.max(5000, this.stepMs * 2.5);
  }

  setWindow(sec) {
    if (sec === this.window) return;
    this.window = sec;
    this.data = [];
    this.version++;
    this.load();
  }

  setPoints(n) {
    const p = Math.max(50, Math.min(2000, Math.round(n)));
    if (Math.abs(p - this.points) / this.points < 0.25) return;
    this.points = p;
    if (isLong(this.window)) this.load();
  }

  async load() {
    const seq = ++this.seq;
    this.loading = true;
    try {
      const r = await this.fetchFn({ window: this.window, keys: this.keys, points: this.points });
      if (seq !== this.seq) return;
      this.stepMs = r.step * 1000;
      this.oldest = r.oldest;
      const long = r.step > 1;
      const pts = r.t.map((t, i) => {
        const p = { t, v: {} };
        if (long) {
          p.lo = {};
          p.hi = {};
        }
        for (const k of this.keys) {
          const s = r.series[k];
          if (!s) continue;
          p.v[k] = s.avg[i];
          if (long) {
            p.lo[k] = s.min[i];
            p.hi[k] = s.max[i];
          }
        }
        // Bøtta midt i tidsrommet den dekker.
        p.t = t + (r.step * 1000) / 2;
        return p;
      });
      // Levende prøver som kom etter serverens siste bøtte, beholdes.
      const lastT = pts.length ? pts[pts.length - 1].t + (r.step * 1000) / 2 : 0;
      const tail = this.data.filter((p) => p.t > lastT && !p.open);
      this.data = pts.concat(tail);
      this.open = null;
      this.error = null;
      this.loadedAt = Date.now();
    } catch (err) {
      if (seq === this.seq) this.error = err.message;
    } finally {
      if (seq === this.seq) {
        this.loading = false;
        this.version++;
      }
    }
  }

  live(t, flat, keys) {
    if (this.loading && !this.data.length) return;
    if (this.stepMs <= 1000) {
      const p = { t, v: {} };
      for (const k of keys) p.v[k] = flat[k] ?? null;
      this.data.push(p);
    } else {
      // Samles i bøtter av samme bredde som serverens svar.
      const b = Math.floor(t / this.stepMs) * this.stepMs;
      let o = this.open;
      if (!o || o.b !== b) {
        o = this.open = { b, acc: {}, point: { t: b + this.stepMs / 2, v: {}, lo: {}, hi: {}, open: true } };
        const last = this.data[this.data.length - 1];
        if (last && last.t >= o.point.t) {
          // Serverens svar hadde allerede denne bøtta: fortsett i den.
          o.point = last;
          last.open = true;
          for (const k of this.keys) if (last.v[k] != null) o.acc[k] = { sum: last.v[k], n: 1, lo: last.lo?.[k] ?? last.v[k], hi: last.hi?.[k] ?? last.v[k] };
        } else this.data.push(o.point);
        for (const p of this.data) if (p !== o.point) delete p.open;
      }
      for (const k of keys) {
        const v = flat[k];
        if (v == null || !Number.isFinite(v)) continue;
        const a = (o.acc[k] ||= { sum: 0, n: 0, lo: Infinity, hi: -Infinity });
        a.sum += v;
        a.n++;
        if (v < a.lo) a.lo = v;
        if (v > a.hi) a.hi = v;
        o.point.v[k] = a.sum / a.n;
        o.point.lo[k] = a.lo;
        o.point.hi[k] = a.hi;
      }
    }
    // Eldre enn vinduet (pluss litt) tas bort.
    const cut = t - this.window * 1000 - this.stepMs * 2;
    let n = 0;
    while (n < this.data.length && this.data[n].t < cut) n++;
    if (n) this.data.splice(0, n);
    this.version++;
  }

  /** Første tidspunkt med data i vinduet (ms), eller null. */
  first() {
    return this.data.length ? this.data[0].t - (this.stepMs > 1000 ? this.stepMs / 2 : 0) : null;
  }

  destroy() {
    for (const off of this.offs) off?.();
    this.offs = [];
    this.seq++;
  }
}
