// Felles definisjoner: sidene og verdiene statusblokken kan vise.

import * as F from './fmt.js';
import { level } from './meters.js';
import { cpuTemp, memUsedPct, netTotals } from './sensormeta.js';

// Rekkefølgen her er også tastatursnarveiene 1–8.
export const PAGES = [
  ['oversikt', 'Oversikt'],
  ['sensorer', 'Sensorer'],
  ['grafer', 'Grafer'],
  ['grafikkort', 'Grafikkort'],
  ['system', 'System'],
  ['kontroller', 'Kontroller'],
  ['viftekurve', 'Viftekurve'],
  ['lys', 'Lys'],
];

/** Verdier statusblokken kan vise. color: seriefargen til sparklinja (fast, v0.0.11). sensor = rad-ID på Sensorer. minSpan = minste
 *  spenn i sparklinja (°C/W), så små svingninger ikke ser dramatiske ut. */
export const STATUS_DEFS = {
  gpuTemp: { color: 'c-temp', label: 'GPU-temp', long: 'GPU-temperatur', src: 'gpu', get: (s) => s.temp, fmt: (v) => F.temp(v, F.dec(0)), lvl: (v, t) => level(v, t.gpuTempWarn, t.gpuTempCrit), sensor: 'gpu/temp', minSpan: 20 },
  gpuLoad: { color: 'c-load', label: 'GPU-last', long: 'GPU-last', src: 'gpu', get: (s) => s.gpuUtil, fmt: (v) => F.pct(v, F.dec(0)), lvl: (v, t) => level(v, t.loadWarn), sensor: 'gpu/gpuUtil', fixed: [0, 100] },
  gpuPower: { color: 'c-power', label: 'Effekt', long: 'GPU-effekt', src: 'gpu', get: (s) => s.power, fmt: (v) => F.watt(v, F.dec(0)), lvl: () => '', sensor: 'gpu/power', minSpan: 40 },
  vram: { color: 'c-mem', label: 'VRAM', long: 'Videominne brukt', src: 'gpu', get: (s) => (s.vramUsed != null && s.vramTotal ? (s.vramUsed / s.vramTotal) * 100 : null), fmt: (v) => F.pct(v, F.dec(0)), lvl: (v) => level(v, 85, 95), sensor: 'gpu/vramUsed', exact: false, fixed: [0, 100] },
  gpuFan: { color: 'c-clock', label: 'Vifte', long: 'GPU-vifte', src: 'gpu', get: (s) => s.fan, fmt: (v) => F.pct(v, 0), lvl: () => '', sensor: 'gpu/fan', fixed: [0, 100] },
  cpuTemp: { color: 'c-temp', label: 'CPU-temp', long: 'CPU-temperatur', src: 'sys', get: (s) => cpuTemp(s), fmt: (v) => F.temp(v, F.dec(0)), lvl: (v, t) => level(v, t.cpuTempWarn, t.cpuTempCrit), sensor: 'cpu/temp', minSpan: 20 },
  cpuLoad: { color: 'c-load', label: 'CPU-last', long: 'CPU-last', src: 'sys', get: (s) => s.cpu?.load, fmt: (v) => F.pct(v, F.dec(0)), lvl: (v, t) => level(v, t.loadWarn), sensor: 'cpu/load', fixed: [0, 100] },
  mem: { color: 'c-mem', label: 'Minne', long: 'Minne brukt', src: 'sys', get: (s) => memUsedPct(s), fmt: (v) => F.pct(v, F.dec(0)), lvl: (v) => level(v, 85, 95), sensor: 'mem/usedPct', fixed: [0, 100] },
  netDown: { color: 'c-load', label: 'Ned', long: 'Nett ned', src: 'sys', get: (s) => netTotals(s).rx, fmt: (v) => F.rate(v), lvl: () => '', sensor: 'net' },
  netUp: { color: 'c-memctl', label: 'Opp', long: 'Nett opp', src: 'sys', get: (s) => netTotals(s).tx, fmt: (v) => F.rate(v), lvl: () => '', sensor: 'net' },
};

