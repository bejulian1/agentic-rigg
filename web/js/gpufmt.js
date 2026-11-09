// Felles formatering for skjermkortverdier.

// PCIe skrives likt overalt (Oversikt, Grafikkort, enhetskortet, Sensorer og
// Spesifikasjon): «x16 Gen 1 · maks Gen 3», og bare «x16 Gen 3» når lenken går
// på full hastighet. Overføringsraten står i verktøytipset.
const NB = '\u00a0';
const GT = { 1: '2,5', 2: '5', 3: '8', 4: '16', 5: '32', 6: '64' };
/** «Gen 3» – brukt der generasjonen står alene. */
export const genText = (g) => (g == null ? '–' : `Gen${NB}${Math.round(g)}`);
const link = (w, g) => `x${w ?? '?'} ${genText(g)}`;

/** «x16 Gen 1 · maks Gen 3», «x8 Gen 3 · maks x16» eller «x16 Gen 3». Bare maks: «x16 Gen 3». */
export function pcieText(maxW, maxG, curW, curG) {
  if (curW == null && curG == null) return maxW == null && maxG == null ? null : link(maxW, maxG);
  const now = link(curW ?? maxW, curG ?? maxG);
  const lowG = maxG != null && curG != null && curG < maxG;
  const lowW = maxW != null && curW != null && curW < maxW;
  if (!lowG && !lowW) return now;
  return `${now} · maks ${lowW ? `x${maxW}` : ''}${lowW && lowG ? ' ' : ''}${lowG ? genText(maxG) : ''}`;
}

/** Spesifikasjonens «PCIe 3.0 x16» i samme form: «x16 Gen 3». */
export function pcieSpec(s) {
  const m = /PCIe\s*(\d)(?:\.\d)?\s*x(\d+)/i.exec(s || '');
  return m ? link(Number(m[2]), Number(m[1])) : s || null;
}

/** Forklaring til verktøytipset, med overføringsrate per lane. */
export function pcieExplain(maxG, curG) {
  if (maxG == null || curG == null) return '';
  const rate = (g) => (GT[g] ? `${genText(g)} = ${GT[g]}${'\u202f'}GT/s per lane` : genText(g));
  if (curG < maxG) return `Lenken går på ${genText(curG)} nå fordi kortet sparer strøm i tomgang, og går opp til ${genText(maxG)} under last. ${rate(curG)}, ${rate(maxG)}.`;
  return `Lenken går på full hastighet. ${rate(curG)}.`;
}

// Bitmaske fra clocks_event_reasons.active (NVML).
const REASONS = [
  [0x1, 'Tomgang'],
  [0x2, 'App-klokker'],
  [0x4, 'Effektgrense'],
  [0x8, 'HW-nedklokking'],
  [0x10, 'Sync boost'],
  [0x20, 'Termisk (SW)'],
  [0x40, 'Termisk (HW)'],
  [0x80, 'Strømbrems (HW)'],
  [0x100, 'Skjermklokke'],
];

export function throttleList(hex) {
  if (hex == null) return null;
  let n;
  try {
    n = Number(BigInt(hex));
  } catch {
    return [String(hex)];
  }
  if (!n) return [];
  const list = REASONS.filter(([b]) => n & b).map(([, t]) => t);
  return list.length ? list : [`0x${n.toString(16)}`];
}

export function throttleText(hex) {
  const l = throttleList(hex);
  if (l == null) return '–';
  return l.length ? l.join(', ') : 'Ingen';
}

/**
 * Kort form til trange steder: de viktigste årsakene først (tomgang og
 * skjermklokke sist), høyst `max`, og «+N» for resten. Hele lista i `full`.
 */
export function throttleShort(hex, max = 2) {
  const l = throttleList(hex);
  if (l == null) return { text: '–', full: '' };
  if (!l.length) return { text: 'Ingen', full: '' };
  // Alvorligst først: det som faktisk holder klokken nede.
  const ORDER = ['Termisk (HW)', 'Strømbrems (HW)', 'HW-nedklokking', 'Termisk (SW)', 'Effektgrense', 'Sync boost', 'App-klokker', 'Skjermklokke', 'Tomgang'];
  const rank = (x) => (ORDER.includes(x) ? ORDER.indexOf(x) : 5);
  const sorted = [...l].sort((a, b) => rank(a) - rank(b));
  const shown = sorted.slice(0, max).join(', ');
  return { text: sorted.length > max ? `${shown} +${sorted.length - max}` : shown, full: l.length > max ? l.join(', ') : '' };
}

/** Er begrensningen noe annet enn tomgang? Brukes for varselfarge. */
export function throttleSerious(hex) {
  const l = throttleList(hex);
  return !!l && l.some((x) => x !== 'Tomgang' && x !== 'Skjermklokke');
}

/** Maskerer en UUID: GPU-1234••••-…. Viser de fire første tegnene. */
export function maskUuid(u) {
  if (!u) return null;
  return u.replace(/^(GPU-)?([0-9a-f]{4})[0-9a-f]{4}(-[0-9a-f-]+)$/i, (_, p = '', a, rest) => `${p}${a}••••${rest.replace(/[0-9a-f]/gi, '•')}`);
}

export const hexId = (v, d) => (v && d ? `${v.toUpperCase()}:${d.toUpperCase()}` : null);

/** «NVIDIA GeForce RTX 2080» → «GeForce RTX 2080». */
export const shortGpuName = (n) => (n ? n.replace(/^NVIDIA\s+/i, '') : null);
