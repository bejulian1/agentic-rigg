// Historikk for grafene (v0.0.11): lagdelt oppløsning med min, snitt og maks
// per bøtte, fast øvre grense for minne og fil, og lagring i datamappa.
//
//   Lag 0: 1 s-bøtter  i 15 min   (900 bøtter)
//   Lag 1: 10 s-bøtter i 6 t      (2 160 bøtter)
//   Lag 2: 60 s-bøtter i 48 t     (2 880 bøtter)
//
// Hver prøve legges i den åpne bøtta i alle tre lagene. Når tiden går over i
// neste bøtte, lukkes den gamle og blir en rad: min, snitt og maks for hver
// serie (Float32Array, NaN der serien manglet). Topper forsvinner derfor ikke
// når dataene blir grovere: maks i en 60 s-bøtte er den høyeste prøven i hele
// minuttet. Der det ikke kom prøver (serveren var av, eller ingen lyttet),
// finnes ingen rader, og grafen viser et hull.
//
// Grenser: høyst MAX_SERIES serier og høyst `keep / step` rader per lag, så
// minnet er høyst ca. 5 940 × 96 × 12 byte ≈ 6,8 MB. Fila er høyst
// MAX_FILE_BYTES; blir den større, kuttes de eldste radene før skriving.
//
// Bare lesing fra maskinen: denne fila leser prøver som allerede er hentet
// (nvidia-smi, /proc, /sys) og skriver bare history.json i AG_DATA_DIR,
// atomisk (temp-fil og rename), slik store.js gjør det.

import { existsSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const TIERS = Object.freeze([
  Object.freeze({ step: 1, keep: 15 * 60 }),
  Object.freeze({ step: 10, keep: 6 * 3600 }),
  Object.freeze({ step: 60, keep: 48 * 3600 }),
]);
/** Tidsvinduene API-et godtar (sekunder). */
export const WINDOWS = Object.freeze([60, 300, 900, 3600, 21600, 86400, 172800]);
export const MAX_SERIES = 96;
export const MAX_KEYS = 16; // per forespørsel
export const MIN_POINTS = 10;
export const MAX_POINTS = 4000;
export const MAX_FILE_BYTES = 16 * 1024 * 1024;
export const KEY_RE = /^[A-Za-z0-9_.:/-]{1,80}$/;
const FILE_VERSION = 1;

const round = (x) => Math.round(x * 1000) / 1000;

class Tier {
  constructor(step, keep) {
    this.stepMs = step * 1000;
    this.keepMs = keep * 1000;
    this.cap = Math.ceil(keep / step) + 1;
    this.rows = []; // { t, d: Float32Array(n*3) } – eldst først
    this.open = null; // { t, min, max, sum, cnt }
  }

  newOpen(t) {
    return {
      t,
      min: new Float64Array(MAX_SERIES).fill(Infinity),
      max: new Float64Array(MAX_SERIES).fill(-Infinity),
      sum: new Float64Array(MAX_SERIES),
      cnt: new Uint32Array(MAX_SERIES),
    };
  }

  /** Den åpne bøtta som en rad (null hvis den er tom). */
  openRow(nKeys) {
    const o = this.open;
    if (!o) return null;
    let any = false;
    const d = new Float32Array(nKeys * 3).fill(NaN);
    for (let i = 0; i < nKeys; i++) {
      if (!o.cnt[i]) continue;
      any = true;
      d[i * 3] = o.min[i];
      d[i * 3 + 1] = o.sum[i] / o.cnt[i];
      d[i * 3 + 2] = o.max[i];
    }
    return any ? { t: o.t, d } : null;
  }

  close(nKeys) {
    const row = this.openRow(nKeys);
    this.open = null;
    if (row) this.append(row);
  }

  append(row) {
    const last = this.rows[this.rows.length - 1];
    if (last && row.t <= last.t) return; // aldri baklengs eller dobbelt
    this.rows.push(row);
    this.trim(row.t);
  }

  trim(now) {
    const cut = now - this.keepMs;
    let n = 0;
    while (n < this.rows.length && this.rows[n].t < cut) n++;
    if (this.rows.length - n > this.cap) n = this.rows.length - this.cap;
    if (n > 0) this.rows.splice(0, n);
  }
}

export class History {
  constructor({ dir = null, log = () => {}, now = () => Date.now() } = {}) {
    this.file = dir ? join(dir, 'history.json') : null;
    this.log = log;
    this.now = now;
    this.keys = [];
    this.index = new Map();
    this.tiers = TIERS.map((t) => new Tier(t.step, t.keep));
    this.dropped = 0; // serier som ikke fikk plass (over MAX_SERIES)
    this.loadNote = null;
    this.timer = null;
    this.writes = 0;
  }

  keyIndex(key) {
    let i = this.index.get(key);
    if (i !== undefined) return i;
    if (this.keys.length >= MAX_SERIES || !KEY_RE.test(key)) {
      this.dropped++;
      return -1;
    }
    i = this.keys.length;
    this.keys.push(key);
    this.index.set(key, i);
    return i;
  }

  /** Én prøve: { nøkkel: tall }. t i ms. */
  ingest(t, values) {
    const pairs = [];
    for (const [k, v] of Object.entries(values)) {
      if (v == null || typeof v !== 'number' || !Number.isFinite(v)) continue;
      const i = this.keyIndex(k);
      if (i >= 0) pairs.push([i, v]);
    }
    if (!pairs.length) return;
    for (const tier of this.tiers) {
      const b = Math.floor(t / tier.stepMs) * tier.stepMs;
      if (tier.open && tier.open.t !== b) {
        if (b < tier.open.t) continue; // klokka gikk baklengs: hopp over prøven
        tier.close(this.keys.length);
      }
      if (!tier.open) {
        const last = tier.rows[tier.rows.length - 1];
        if (last && b <= last.t) continue;
        tier.open = tier.newOpen(b);
      }
      const o = tier.open;
      for (const [i, v] of pairs) {
        if (v < o.min[i]) o.min[i] = v;
        if (v > o.max[i]) o.max[i] = v;
        o.sum[i] += v;
        o.cnt[i]++;
      }
    }
  }

  /** Lukker de åpne bøttene (ved avslutning). */
  closeAll() {
    for (const tier of this.tiers) tier.close(this.keys.length);
  }

  /** Antall rader per lag (for tester og feilsøking). */
  sizes() {
    return this.tiers.map((t) => t.rows.length);
  }

  /** Eldste tidspunkt med data i noe lag (ms), eller null. */
  oldest() {
    let o = null;
    for (const t of this.tiers) if (t.rows.length && (o == null || t.rows[0].t < o)) o = t.rows[0].t;
    return o;
  }

  /**
   * Henter et tidsvindu. window i sekunder (WINDOWS), keys som liste, points =
   * høyst antall bøtter i svaret (typisk grafens bredde i piksler). map gjør
   * om en nøkkel før oppslag (f.eks. gpu.* → gsim.* i simulert modus).
   * Svar: { window, step (s), from, to, oldest, t: [...], series: { key: { min, avg, max } } }.
   */
  query({ window, keys, points = 1000, map = (k) => k }) {
    const now = this.now();
    const ti = this.tiers.findIndex((t) => t.keepMs >= window * 1000);
    const tier = this.tiers[ti < 0 ? this.tiers.length - 1 : ti];
    const from = now - window * 1000;
    const per = Math.max(1, Math.ceil((window * 1000) / points / tier.stepMs));
    const stepMs = per * tier.stepMs;
    const idx = keys.map((k) => this.index.get(map(k)) ?? -1);
    const rows = tier.rows.filter((r) => r.t >= from);
    const open = tier.openRow(this.keys.length);
    if (open && open.t >= from) rows.push(open);
    const t = [];
    const out = keys.map(() => ({ min: [], avg: [], max: [] }));
    let cur = null;
    const flush = () => {
      if (!cur) return;
      let any = false;
      for (let k = 0; k < keys.length; k++) {
        const a = cur.acc[k];
        const has = a.n > 0;
        if (has) any = true;
        out[k].min.push(has ? round(a.min) : null);
        out[k].avg.push(has ? round(a.sum / a.n) : null);
        out[k].max.push(has ? round(a.max) : null);
      }
      if (any) t.push(cur.t);
      else for (const o of out) (o.min.pop(), o.avg.pop(), o.max.pop());
      cur = null;
    };
    for (const r of rows) {
      const b = Math.floor(r.t / stepMs) * stepMs;
      if (!cur || cur.t !== b) {
        flush();
        cur = { t: b, acc: keys.map(() => ({ min: Infinity, max: -Infinity, sum: 0, n: 0 })) };
      }
      for (let k = 0; k < keys.length; k++) {
        const i = idx[k];
        if (i < 0 || i * 3 + 2 >= r.d.length) continue;
        const lo = r.d[i * 3];
        if (Number.isNaN(lo)) continue;
        const a = cur.acc[k];
        if (lo < a.min) a.min = lo;
        const hi = r.d[i * 3 + 2];
        if (hi > a.max) a.max = hi;
        a.sum += r.d[i * 3 + 1];
        a.n++;
      }
    }
    flush();
    // Bøttene er justert til hele steg, så vinduet kan dekke ett steg for mye.
    const extra = t.length - points;
    if (extra > 0) {
      t.splice(0, extra);
      for (const o of out) for (const a of [o.min, o.avg, o.max]) a.splice(0, extra);
    }
    const series = {};
    keys.forEach((k, i) => (series[k] = out[i]));
    return { window, step: stepMs / 1000, from, to: now, oldest: this.oldest(), t, series };
  }

  // ---- Lagring ----

  /** Gjør historikken om til et lite JSON-objekt (tall rundet til tre desimaler, NaN → null). */
  serialize() {
    const n = this.keys.length;
    const tiers = this.tiers.map((tier) => {
      const rows = tier.rows.slice();
      const open = tier.openRow(n);
      if (open) rows.push(open);
      return { step: tier.stepMs / 1000, rows: rows.map((r) => [r.t, Array.from(r.d, (x) => (Number.isNaN(x) ? null : round(x)))]) };
    });
    return { v: FILE_VERSION, saved: this.now(), keys: this.keys.slice(), tiers };
  }

  /** Skriver fila atomisk. Er den for stor, kuttes de eldste radene (finest lag først). */
  save() {
    if (!this.file) return false;
    const obj = this.serialize();
    let text = JSON.stringify(obj);
    for (let guard = 0; text.length > MAX_FILE_BYTES && guard < 40; guard++) {
      const t = obj.tiers.find((x) => x.rows.length > 0 && x.step === 1) || obj.tiers.reduce((a, b) => (b.rows.length > a.rows.length ? b : a));
      t.rows.splice(0, Math.max(1, Math.ceil(t.rows.length / 4)));
      text = JSON.stringify(obj);
    }
    if (text.length > MAX_FILE_BYTES) return false;
    const tmp = `${this.file}.${process.pid}.tmp`;
    try {
      writeFileSync(tmp, text);
      renameSync(tmp, this.file);
      this.writes++;
      return true;
    } catch (err) {
      this.log(`Kunne ikke lagre historikken: ${err.message}`);
      return false;
    }
  }

  /** Lagrer med jevne mellomrom (og ved avslutning, se index.js). */
  startAutosave(ms = 5 * 60_000) {
    clearInterval(this.timer);
    this.timer = setInterval(() => this.save(), ms);
    this.timer.unref?.();
  }

  stop() {
    clearInterval(this.timer);
    this.timer = null;
  }

  /**
   * Leser fila. En ødelagt eller for stor fil flyttes til history.json.bad, og
   * historikken begynner på nytt (loadNote sier fra). Rader som er for gamle
   * for laget sitt, eller i framtida, hoppes over.
   */
  load() {
    if (!this.file || !existsSync(this.file)) return false;
    try {
      if (statSync(this.file).size > MAX_FILE_BYTES) throw new Error('fila er større enn grensen');
      const obj = JSON.parse(readFileSync(this.file, 'utf8'));
      this.restore(obj);
      return true;
    } catch (err) {
      this.loadNote = `history.json kunne ikke leses (${err.message}). Historikken begynner på nytt.`;
      try {
        renameSync(this.file, `${this.file}.bad`);
      } catch {}
      this.keys = [];
      this.index = new Map();
      this.tiers = TIERS.map((t) => new Tier(t.step, t.keep));
      return false;
    }
  }

  restore(obj) {
    if (!obj || typeof obj !== 'object' || obj.v !== FILE_VERSION) throw new Error('ukjent format');
    if (!Array.isArray(obj.keys) || obj.keys.length > MAX_SERIES || !obj.keys.every((k) => typeof k === 'string' && KEY_RE.test(k))) throw new Error('ugyldige serier');
    if (new Set(obj.keys).size !== obj.keys.length) throw new Error('doble serier');
    if (!Array.isArray(obj.tiers) || obj.tiers.length !== TIERS.length) throw new Error('ugyldige lag');
    const n = obj.keys.length;
    const now = this.now();
    const tiers = TIERS.map((def, i) => {
      const src = obj.tiers[i];
      if (!src || src.step !== def.step || !Array.isArray(src.rows)) throw new Error(`ugyldig lag ${i}`);
      const tier = new Tier(def.step, def.keep);
      for (const row of src.rows) {
        if (!Array.isArray(row) || row.length !== 2) throw new Error('ugyldig rad');
        const [t, vals] = row;
        if (typeof t !== 'number' || !Number.isFinite(t) || !Array.isArray(vals) || vals.length > n * 3 || vals.length % 3) throw new Error('ugyldig rad');
        const d = new Float32Array(vals.length);
        for (let j = 0; j < vals.length; j++) {
          const x = vals[j];
          if (x === null) d[j] = NaN;
          else if (typeof x === 'number' && Number.isFinite(x)) d[j] = x;
          else throw new Error('ugyldig verdi');
        }
        // For gamle for laget, eller i framtida: hoppes over (ikke en feil).
        if (t > now + 60_000 || t < now - def.keep * 1000) continue;
        tier.append({ t, d });
      }
      return tier;
    });
    this.keys = obj.keys.slice();
    this.index = new Map(this.keys.map((k, i) => [k, i]));
    this.tiers = tiers;
  }
}

// ---- Fra prøvene til serier ----

/** GPU-feltene som lagres. I simulert modus får de prefikset gsim. i stedet for gpu. */
export const GPU_FIELDS = Object.freeze(['temp', 'gpuUtil', 'memUtil', 'encUtil', 'decUtil', 'coreClock', 'smClock', 'memClock', 'videoClock', 'power', 'powerLimit', 'fan', 'vramUsed', 'pcieGen']);

export function gpuValues(s, mode) {
  const p = mode === 'live' ? 'gpu.' : 'gsim.';
  const out = {};
  for (const f of GPU_FIELDS) out[p + f] = s[f];
  return out;
}

/**
 * Systemprøven som serier: cpu.temp (samme sensor som nettleseren viser), cpu.load, cpu.t0 …, mem.*, net.rx/tx (sum over
 * fysiske kort, samme regel som i nettleseren), net.<kort>.rx/tx og hw.<id>.
 * netTotals og memUsedPct er de samme funksjonene som nettleseren bruker.
 */
export function sysValues(s, { netTotals, memUsedPct, cpuTemp }) {
  const out = {};
  out['cpu.temp'] = cpuTemp(s);
  if (s.cpu) {
    out['cpu.load'] = s.cpu.load;
    (s.cpu.coreLoad || []).slice(0, 64).forEach((v, i) => (out[`cpu.t${i}`] = v));
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

/**
 * Syntetisk historikk for skjermbilder og tester (AG_TEST_HISTORY=1): 48 t
 * med døgnrytme, en topp og et hull på 40 minutter der «serveren var av».
 * Brukes aldri ellers.
 */
export function fillSynthetic(h, { now = Date.now(), hours = 48, mode = 'sim' } = {}) {
  const start = now - hours * 3600_000;
  const holeA = now - 7 * 3600_000;
  const holeB = holeA + 40 * 60_000;
  const p = mode === 'live' ? 'gpu.' : 'gsim.';
  for (let t = start; t < now - 2000; ) {
    if (t >= holeA && t < holeB) {
      t = holeB;
      continue;
    }
    const day = Math.sin(((t / 3600_000) % 24) / 24 * Math.PI * 2 - Math.PI / 2) * 0.5 + 0.5; // 0 om natta, 1 midt på dagen
    const burst = Math.abs(((t / 60_000) % 180) - 90) < 12 ? 1 : 0; // spill/arbeid ca. hver 3. time
    const noise = Math.sin(t / 7_000) * 0.5 + Math.sin(t / 23_000) * 0.5;
    const load = Math.max(0, Math.min(100, 4 + day * 18 + burst * 70 + noise * 6));
    const temp = 33 + load * 0.42 + noise;
    h.ingest(t, {
      [p + 'temp']: temp,
      [p + 'gpuUtil']: load,
      [p + 'memUtil']: load * 0.6,
      [p + 'coreClock']: 300 + load * 16,
      [p + 'memClock']: load > 10 ? 7000 : 405,
      [p + 'power']: 14 + load * 2.1,
      [p + 'powerLimit']: 245,
      [p + 'fan']: Math.max(35, Math.min(100, 35 + (temp - 55) * 2.2)),
      [p + 'vramUsed']: 600 + load * 40,
      'cpu.load': Math.max(0, Math.min(100, 6 + day * 20 + burst * 35 + noise * 4)),
      'cpu.temp': 45 + day * 12 + burst * 18 + noise * 2,
      'mem.usedPct': 22 + day * 6 + burst * 10,
      'net.rx': Math.max(0, (day * 400 + burst * 3000 + noise * 200) * 1000),
      'net.tx': Math.max(0, (day * 60 + noise * 30) * 1000),
    });
    // Tettere prøver de siste 15 minuttene (1 s), ellers hvert 10. s.
    t += t > now - 15 * 60_000 ? 1000 : 10_000;
  }
}
