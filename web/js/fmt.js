// Formatering av tall og enheter. Regler:
// – Smalt, ikke-brytende mellomrom (U+202F) mellom tall og enhet, også «38 %».
// – Tusenskille er også U+202F (norsk standard), desimalkomma.
// – Ekte minustegn (U+2212).
// – Enheter og desimaler følger Innstillinger (setFormat).

export const NNBSP = ' ';
const MINUS = '−';

const P = { unit: 'C', decimals: 'auto', memUnit: 'bin', netUnit: 'byte', clock: 'system' };

export function setFormat(s) {
  for (const k of Object.keys(P)) if (s[k] !== undefined) P[k] = s[k];
}
export const fmtPrefs = () => ({ ...P });

const cache = new Map();
function nf(d) {
  let f = cache.get(d);
  if (!f) cache.set(d, (f = new Intl.NumberFormat('nb-NO', { minimumFractionDigits: d, maximumFractionDigits: d })));
  return f;
}

/** Tall med fast antall desimaler. null/NaN gir «–». */
export function num(v, d = 0) {
  if (v == null || !Number.isFinite(v)) return '–';
  // Unngå «−0».
  if (Math.abs(v) < 0.5 * 10 ** -d) v = 0;
  return nf(d).format(v).replace(/[  ]/g, NNBSP).replace('-', MINUS);
}

export const withUnit = (text, unit) => (text === '–' ? '–' : `${text}${NNBSP}${unit}`);

/** Desimaler: «auto» gir standarden for stedet, ellers brukerens valg. */
export const dec = (auto) => (P.decimals === 'auto' ? auto : Number(P.decimals));

export const signed = (v, d = 0) => (v > 0 ? `+${num(v, d)}` : v < 0 ? `${MINUS}${num(-v, d)}` : num(0, d));

// ---- Temperatur (lagres alltid i °C) ----
export const tempOut = (c) => (c == null ? null : P.unit === 'F' ? (c * 9) / 5 + 32 : c);
export const tempIn = (v) => (v == null ? null : P.unit === 'F' ? ((v - 32) * 5) / 9 : v);
export const tempLabel = () => (P.unit === 'F' ? '°F' : '°C');
export const tempNum = (c, d = 0) => num(tempOut(c), d);
export const temp = (c, d = 0) => withUnit(num(tempOut(c), d), tempLabel());
/** Temperaturforskjell (ikke absolutt): 10 °C = 18 °F. */
export const tempDelta = (c, d = 0) => withUnit(num(P.unit === 'F' ? (c * 9) / 5 : c, d), tempLabel());

export const pct = (v, d = 0) => withUnit(num(v, d), '%');
export const watt = (v, d = 0) => withUnit(num(v, d), 'W');
export const mhz = (v) => withUnit(num(v, 0), 'MHz');

// ---- Minne ----
/** MiB (fra nvidia-smi) i valgt system: binært (MiB/GiB) eller desimalt (MB/GB). */
export function mib(v, { d = null, unit = null } = {}) {
  if (v == null || !Number.isFinite(v)) return '–';
  if (P.memUnit === 'dec') {
    const mb = v * 1.048576;
    if ((unit ?? (mb >= 10000 ? 'GB' : 'MB')) === 'GB') return withUnit(num(mb / 1000, d ?? 1), 'GB');
    return withUnit(num(mb, d ?? 0), 'MB');
  }
  if ((unit ?? (v >= 10240 ? 'GiB' : 'MiB')) === 'GiB') return withUnit(num(v / 1024, d ?? 1), 'GiB');
  return withUnit(num(v, d ?? 0), 'MiB');
}

/** Byte i valgt system, med enhet etter størrelse. */
export function bytes(b, d = 1) {
  if (b == null || !Number.isFinite(b)) return '–';
  const bin = P.memUnit !== 'dec';
  const base = bin ? 1024 : 1000;
  const units = bin ? ['B', 'KiB', 'MiB', 'GiB', 'TiB'] : ['B', 'kB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (Math.abs(b) >= base && i < units.length - 1) {
    b /= base;
    i++;
  }
  return withUnit(num(b, i === 0 ? 0 : d), units[i]);
}

/** «47 GiB» eller «50 GB» for et helt minnetall i kB (fra /proc/meminfo). */
export const kb = (k, d = 1) => (k == null ? '–' : bytes(k * 1024, d));

// ---- Nettrate med fast enhet per rad ----
// Enheten velges ut fra største verdi raden har sett, og byttes bare oppover,
// så tallene ikke hopper mellom kB/s og MB/s. Desimale enheter (som nettverk).
const RATE_UNITS = {
  byte: [
    [1, 'B/s', 0],
    [1e3, 'kB/s', 1],
    [1e6, 'MB/s', 2],
    [1e9, 'GB/s', 2],
  ],
  bit: [
    [1, 'bit/s', 0],
    [1e3, 'kbit/s', 0],
    [1e6, 'Mbit/s', 1],
    [1e9, 'Gbit/s', 2],
  ],
};
export function rateUnit(maxBps) {
  const list = RATE_UNITS[P.netUnit] || RATE_UNITS.byte;
  const m = P.netUnit === 'bit' ? (maxBps || 0) * 8 : maxBps || 0;
  let u = list[1];
  for (const x of list) if (m >= x[0] * 0.95) u = x;
  return { div: u[0] / (P.netUnit === 'bit' ? 8 : 1), label: u[1], d: u[2], key: u[1] };
}
export function rate(bps, u = null) {
  if (bps == null || !Number.isFinite(bps)) return '–';
  const unit = u || rateUnit(bps);
  return withUnit(num(bps / unit.div, unit.d), unit.label);
}

// ---- Tid ----
function hour12() {
  return P.clock === '12' ? true : P.clock === '24' ? false : undefined;
}
export function time(d, { seconds = true } = {}) {
  const t = d instanceof Date ? d : new Date(d);
  return t.toLocaleTimeString('nb-NO', { hour: '2-digit', minute: '2-digit', second: seconds ? '2-digit' : undefined, hour12: hour12() });
}
export function dateTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  return d.toLocaleString('nb-NO', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hour12: hour12() });
}
const MONTHS = ['januar', 'februar', 'mars', 'april', 'mai', 'juni', 'juli', 'august', 'september', 'oktober', 'november', 'desember'];
const MON_SHORT = ['jan.', 'feb.', 'mars', 'apr.', 'mai', 'juni', 'juli', 'aug.', 'sep.', 'okt.', 'nov.', 'des.'];
export const dateLong = (y, m, d) => `${d}. ${MONTHS[m]} ${y}`;
export const dateShort = (y, m, d) => `${d}. ${MON_SHORT[m]} ${y}`;
/** «23:37» i dag, «3. nov. 23:37» en annen dag og «3. nov. 2025 23:37» et annet år. */
export function since(d) {
  const t = d instanceof Date ? d : new Date(d);
  const now = new Date();
  const hm = time(t, { seconds: false });
  if (t.toDateString() === now.toDateString()) return hm;
  const day = `${t.getDate()}. ${MON_SHORT[t.getMonth()]}`;
  return t.getFullYear() === now.getFullYear() ? `${day} ${hm}` : `${day} ${t.getFullYear()} ${hm}`;
}

export function uptime(sec) {
  if (sec == null || !Number.isFinite(sec)) return '–';
  const d = Math.floor(sec / 86400);
  const h = Math.floor((sec % 86400) / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const parts = [];
  if (d) parts.push(`${d}${NNBSP}døgn`);
  if (d || h) parts.push(`${h}${NNBSP}t`);
  parts.push(`${m}${NNBSP}min`);
  return parts.join(' ');
}

/** «1 kjerne», «16 tråder»: riktig entall og flertall. */
export const plural = (n, one, many) => `${n == null ? '?' : num(n)} ${n === 1 ? one : many}`;

/** «−2 min», «−30 s» eller «nå». */
/**
 * Tidsaksen: én enhet per vindu. Opptil 2 minutter i sekunder («−90 s · −60 s
 * · −30 s»), lengre vinduer i minutter («−4 min»).
 */
export function ago(sec, windowSec = 0) {
  if (sec <= 0) return 'nå';
  if (windowSec > 120 && sec % 60 === 0) return `${MINUS}${sec / 60}${NNBSP}min`;
  if (!windowSec && sec % 60 === 0) return `${MINUS}${sec / 60}${NNBSP}min`;
  return `${MINUS}${sec}${NNBSP}s`;
}

/** «i dag 16:43», «i går 23:10» eller «3. nov. 14:32» (v0.0.11, grafene). seconds: med sekunder. */
export function when(d, { seconds = false } = {}) {
  const t = d instanceof Date ? d : new Date(d);
  const now = new Date();
  const hm = time(t, { seconds });
  const y = new Date(now);
  y.setDate(now.getDate() - 1);
  if (t.toDateString() === now.toDateString()) return `i dag ${hm}`;
  if (t.toDateString() === y.toDateString()) return `i går ${hm}`;
  return `${t.getDate()}. ${MON_SHORT[t.getMonth()]} ${hm}`;
}
/** «6. nov.» – dato på tidsaksen der døgnet skifter. */
export const dayShort = (d) => `${d.getDate()}. ${MON_SHORT[d.getMonth()]}`;
