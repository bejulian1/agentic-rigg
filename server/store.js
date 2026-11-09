// Lagrer simulerte innstillinger, RGB, profiler og app-innstillinger i
// AG_DATA_DIR/state.json. Ingenting her når skjermkortet.
//
// Alt som kommer fra nettleseren valideres her. Ukjente felt ignoreres, og
// ugyldige verdier gir ValidationError (HTTP 400) med en norsk melding.

import { copyFileSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const LIMITS = Object.freeze({
  powerLimit: [105, 275],
  coreOffset: [-200, 300],
  memOffset: [-1000, 1500],
  fanPercent: [35, 100], // minste viftefart på kortet (research, NVML)
  tempTarget: [65, 88],
  fanMinPercent: [35, 100],
  fanHysteresis: [0, 10],
  fanRampUp: [0, 10],
  fanRampDown: [0, 20],
  // Start/stopp (v0.0.6): under stopptemperaturen holder viften minste fart,
  // og kurven tar over igjen først ved starttemperaturen.
  fanStartTemp: [30, 90],
  fanStopTemp: [25, 87],
  brightness: [0, 100],
  speed: [1, 10],
  rgbTemp: [20, 100],
});

/**
 * Viftemodus (v0.0.7): auto = driveren styrer, curve = viftekurven (i
 * bakgrunnstjenesten), manual = fast fart. Før v0.0.7 betydde «auto» kurven.
 */
export const FAN_MODES = Object.freeze(['auto', 'curve', 'manual']);
/** Versjonen av state.json. Mangler den, er fila fra før v0.0.7. */
export const SCHEMA = 7;
/** Frist for å beholde endringer (sekunder) som kan velges under Innstillinger. */
export const CONFIRM_CHOICES = Object.freeze([10, 15, 30, 60]);

/** Minste avstand mellom start- og stopptemperatur (°C), så viften ikke pendler. */
export const FAN_START_GAP = 3;

export const DEFAULT_CURVE = Object.freeze([
  [30, 35],
  [50, 40],
  [62, 50],
  [72, 65],
  [82, 85],
  [90, 100],
]);

export const EFFECTS = Object.freeze(['static', 'breathe', 'rainbow', 'cycle', 'temperature', 'off']);
export const INTERVALS = Object.freeze([500, 1000, 2000]);
export const THEMES = Object.freeze(['system', 'light', 'dark']);
export const SLOTS = 5;

// App-innstillinger. Listene her er de eneste lovlige verdiene.
export const PAGES = Object.freeze(['oversikt', 'sensorer', 'grafer', 'grafikkort', 'system', 'kontroller', 'viftekurve', 'lys']);
export const PALETTES = Object.freeze(['grafitt', 'sand', 'fjord', 'midnatt']);
// v0.0.11: seks aksentfarger. Gul, oransje og rød er fjernet, fordi de samme
// fargene betyr advarsel og fare i appen. Lagrede valg med dem blir blå.
export const ACCENTS = Object.freeze(['blue', 'teal', 'green', 'purple', 'pink', 'slate']);
const OLD_ACCENTS = Object.freeze(['yellow', 'orange', 'red']);
export const DENSITIES = Object.freeze(['compact', 'standard', 'comfortable']);
export const FONT_SCALES = Object.freeze([92, 100, 108, 116]);
export const MOTIONS = Object.freeze(['system', 'full', 'reduced', 'off']);
// v0.0.11: 1 min til 48 t (historikken ligger på serveren, se history.js).
// 120 (2 min) fra eldre filer blir 300.
export const CHART_WINDOWS = Object.freeze([60, 300, 900, 3600, 21600, 86400, 172800]);
/** Sider med eget tidsvindu for grafene (Grafer bruker chartWindow). */
export const WINDOW_PAGES = Object.freeze(['oversikt', 'kontroller', 'sensorer']);
export const STATUS_ITEMS = Object.freeze(['gpuTemp', 'gpuLoad', 'gpuPower', 'vram', 'gpuFan', 'cpuTemp', 'cpuLoad', 'mem', 'netDown', 'netUp']);
/** Høyst så mange verdier i statusblokken (ca. 8 rader får plass ved 1440 × 900). */
export const STATUS_MAX = 8;
/** Rad-ID-er fra Sensorer som kan festes: «gpu/temp», «cpu/t3», «hw/hwmon3/temp1», «net/wlo1/rx». */
// Hver del starter med bokstav, tall eller _ (aldri «.» eller «..»).
export const SENSOR_ID = /^(gpu|cpu|mem|net|hw)(\/[A-Za-z0-9_][A-Za-z0-9_.:@+-]{0,39}){1,3}$/;
export const CHARTS = Object.freeze(['gpuTemp', 'gpuLoad', 'coreClock', 'memClock', 'power', 'fan', 'vram', 'cpuLoad', 'cpuTemp', 'mem', 'net']);
export const INFOBARS = Object.freeze(['kontroller', 'viftekurve', 'lys']);

export function defaultApplied() {
  return {
    powerLimit: 245,
    coreOffset: 0,
    memOffset: 0,
    fanMode: 'auto',
    fanPercent: 50,
    tempTarget: 83,
    fanCurve: DEFAULT_CURVE.map((p) => [...p]),
    fanMinPercent: 35,
    fanHysteresis: 2,
    fanRampUp: 2,
    fanRampDown: 5,
    fanSource: 'gpu',
    fanStartStop: false,
    fanStartTemp: 55,
    fanStopTemp: 48,
  };
}

export function defaultRgb() {
  const c = '#3a7bf0';
  return {
    color: c,
    zones: { bar: c, fan1: c, fan2: c },
    effect: 'static',
    brightness: 80,
    speed: 5,
    direction: 'cw',
    tempCold: '#3a7bf0',
    tempHot: '#e5383b',
    tempMin: 40,
    tempMax: 80,
    ambient: false,
    recent: [],
  };
}

export function defaultSettings() {
  return {
    intervalMs: 500,
    unit: 'C',
    startSimulated: false,
    theme: 'system',
    accent: 'blue',
    palette: 'grafitt',
    highContrast: false,
    density: 'standard',
    fontScale: 100,
    motion: 'system',
    startPage: 'oversikt',
    showKbd: false,
    hiddenPages: [],
    statusItems: ['gpuTemp', 'gpuLoad', 'gpuPower', 'cpuTemp', 'mem'],
    statusSparks: true,
    sideCollapsed: false, // v0.0.10: sidefeltet som smal ikonlist
    thresholds: { gpuTempWarn: 75, gpuTempCrit: 85, cpuTempWarn: 80, cpuTempCrit: 90, loadWarn: 90 },
    chartWindow: 300,
    windows: { oversikt: 60, kontroller: 60, sensorer: 300 },
    chartSync: true, // hårkorset vises på alle grafene på siden
    chartScale: 'auto',
    chartFill: true,
    charts: ['gpuTemp', 'gpuLoad', 'coreClock', 'memClock', 'power', 'fan'],
    sensorSparks: true,
    sensorKeepMinMax: true,
    decimals: 'auto',
    memUnit: 'bin',
    netUnit: 'byte',
    clock: 'system',
    dismissed: [],
    confirmSec: 15,
    surprises: true, // v0.0.12: små overraskelser (påskeegg, web/js/easter.js)
  };
}

const sameCurve = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((p, i) => Array.isArray(p) && p[0] === b[i][0] && p[1] === b[i][1]);

/**
 * Viftemodus i filer fra før v0.0.7: «auto» betydde kurven. Med egen kurve
 * eller start/stopp på blir det «curve». Med standardkurven blir det driverens
 * auto, som i simuleringen oppfører seg likt.
 */
export function legacyFanMode(applied) {
  if (applied?.fanMode !== 'auto') return applied?.fanMode;
  const custom = Array.isArray(applied.fanCurve) && !sameCurve(applied.fanCurve, DEFAULT_CURVE);
  return custom || applied.fanStartStop === true ? 'curve' : 'auto';
}

const clamp = (v, [a, b]) => Math.min(b, Math.max(a, v));
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const isObj = (v) => v != null && typeof v === 'object' && !Array.isArray(v);
const HEX = /^#[0-9a-fA-F]{6}$/;

export class ValidationError extends Error {}

function validCurve(c) {
  if (!Array.isArray(c) || c.length < 2 || c.length > 10) throw new ValidationError('Viftekurven må ha 2–10 punkter');
  const pts = c.map((p) => {
    if (!Array.isArray(p) || p.length !== 2 || !isNum(p[0]) || !isNum(p[1])) throw new ValidationError('Ugyldig punkt i viftekurven');
    return [Math.round(clamp(p[0], [0, 110])), Math.round(clamp(p[1], LIMITS.fanPercent))];
  });
  pts.sort((a, b) => a[0] - b[0]);
  for (let i = 1; i < pts.length; i++) {
    if (pts[i][0] <= pts[i - 1][0]) throw new ValidationError('Temperaturene i viftekurven må stige');
    // Kurven skal aldri falle: et punkt løftes til forrige punkts viftefart
    // (gjelder også gamle filer og profiler fra før v0.0.6).
    if (pts[i][1] < pts[i - 1][1]) pts[i][1] = pts[i - 1][1];
  }
  return pts;
}

/** Fletter en delvis innstilling inn i base, med klemming til lovlige verdier. */
export function mergeApplied(base, patch) {
  if (!isObj(patch)) throw new ValidationError('Forventet et JSON-objekt');
  const out = { ...defaultApplied(), ...base, fanCurve: base.fanCurve.map((p) => [...p]) };
  for (const k of ['powerLimit', 'coreOffset', 'memOffset', 'fanPercent', 'tempTarget', 'fanMinPercent', 'fanHysteresis', 'fanRampUp', 'fanRampDown', 'fanStartTemp', 'fanStopTemp']) {
    if (patch[k] === undefined) continue;
    if (!isNum(patch[k])) throw new ValidationError(`${k} må være et tall`);
    out[k] = Math.round(clamp(patch[k], LIMITS[k]));
  }
  if (patch.fanMode !== undefined) {
    if (!FAN_MODES.includes(patch.fanMode)) throw new ValidationError('fanMode må være auto, curve eller manual');
    out.fanMode = patch.fanMode;
  }
  if (patch.fanSource !== undefined) {
    if (patch.fanSource !== 'gpu' && patch.fanSource !== 'cpu') throw new ValidationError('fanSource må være gpu eller cpu');
    out.fanSource = patch.fanSource;
  }
  if (patch.fanStartStop !== undefined) {
    if (typeof patch.fanStartStop !== 'boolean') throw new ValidationError('fanStartStop må være true/false');
    out.fanStartStop = patch.fanStartStop;
  }
  // Start må ligge minst FAN_START_GAP over stopp, også når valget er av, så
  // det som står i skjemaet alltid kan slås på.
  if (out.fanStartTemp - out.fanStopTemp < FAN_START_GAP) {
    throw new ValidationError(`Starttemperaturen må være minst ${FAN_START_GAP} °C over stopptemperaturen`);
  }
  if (patch.fanCurve !== undefined) out.fanCurve = validCurve(patch.fanCurve);
  return out;
}

function hex(v, name) {
  if (typeof v !== 'string' || !HEX.test(v)) throw new ValidationError(`${name} må være #rrggbb`);
  return v.toLowerCase();
}

export function mergeRgb(base, patch) {
  if (!isObj(patch)) throw new ValidationError('Forventet et JSON-objekt');
  const d = defaultRgb();
  const out = { ...d, ...base, zones: { ...d.zones, ...(base.zones || {}) }, recent: [...(base.recent || [])] };
  if (patch.color !== undefined) out.color = hex(patch.color, 'Farge');
  if (patch.zones !== undefined) {
    if (!isObj(patch.zones)) throw new ValidationError('zones må være et objekt');
    for (const z of ['bar', 'fan1', 'fan2']) if (patch.zones[z] !== undefined) out.zones[z] = hex(patch.zones[z], `Farge for ${z}`);
  } else if (patch.color !== undefined) {
    // Bare én farge sendt: gjelder alle soner (som i v0.0.3).
    out.zones = { bar: out.color, fan1: out.color, fan2: out.color };
  }
  for (const k of ['brightness', 'speed']) {
    if (patch[k] === undefined) continue;
    if (!isNum(patch[k])) throw new ValidationError(`${k} må være et tall`);
    out[k] = Math.round(clamp(patch[k], LIMITS[k]));
  }
  for (const k of ['tempMin', 'tempMax']) {
    if (patch[k] === undefined) continue;
    if (!isNum(patch[k])) throw new ValidationError(`${k} må være et tall`);
    out[k] = Math.round(clamp(patch[k], LIMITS.rgbTemp));
  }
  if (out.tempMax - out.tempMin < 5) throw new ValidationError('Temperaturområdet må være minst 5 °C');
  if (patch.tempCold !== undefined) out.tempCold = hex(patch.tempCold, 'Kald farge');
  if (patch.tempHot !== undefined) out.tempHot = hex(patch.tempHot, 'Varm farge');
  if (patch.effect !== undefined) {
    if (!EFFECTS.includes(patch.effect)) throw new ValidationError('Ukjent effekt');
    out.effect = patch.effect;
  }
  if (patch.direction !== undefined) {
    if (patch.direction !== 'cw' && patch.direction !== 'ccw') throw new ValidationError('Retning må være cw eller ccw');
    out.direction = patch.direction;
  }
  if (patch.ambient !== undefined) {
    if (typeof patch.ambient !== 'boolean') throw new ValidationError('ambient må være true/false');
    out.ambient = patch.ambient;
  }
  if (patch.recent !== undefined) {
    if (!Array.isArray(patch.recent) || patch.recent.length > 6) throw new ValidationError('recent må være en liste med høyst 6 farger');
    out.recent = patch.recent.map((c) => hex(c, 'Nylig brukt farge'));
  }
  return out;
}

function oneOf(v, list, msg) {
  if (!list.includes(v)) throw new ValidationError(msg);
  return v;
}

function bool(v, name) {
  if (typeof v !== 'boolean') throw new ValidationError(`${name} må være true/false`);
  return v;
}

function subset(v, list, name, { min = 0, max = list.length } = {}) {
  if (!Array.isArray(v)) throw new ValidationError(`${name} må være en liste`);
  const out = [];
  for (const x of v) {
    if (!list.includes(x)) throw new ValidationError(`Ukjent verdi i ${name}: ${String(x).slice(0, 40)}`);
    if (!out.includes(x)) out.push(x);
  }
  if (out.length < min || out.length > max) throw new ValidationError(`${name} må ha ${min}–${max} valg`);
  return out;
}

/**
 * Statusblokken: en ordnet liste (rekkefølgen er brukerens) med faste verdier
 * (STATUS_ITEMS) og festede sensorer fra Sensorer (SENSOR_ID). Høyst STATUS_MAX.
 */
function statusList(v) {
  if (!Array.isArray(v)) throw new ValidationError('statusItems må være en liste');
  const out = [];
  for (const x of v) {
    if (typeof x !== 'string' || !(STATUS_ITEMS.includes(x) || SENSOR_ID.test(x))) throw new ValidationError(`Ukjent verdi i statusItems: ${String(x).slice(0, 40)}`);
    if (!out.includes(x)) out.push(x);
  }
  if (out.length > STATUS_MAX) throw new ValidationError(`Statusblokken kan ha høyst ${STATUS_MAX} verdier`);
  return out;
}

const THRESH = {
  gpuTempWarn: [40, 100],
  gpuTempCrit: [40, 110],
  cpuTempWarn: [40, 100],
  cpuTempCrit: [40, 110],
  loadWarn: [50, 100],
};

export function mergeSettings(base, patch) {
  if (!isObj(patch)) throw new ValidationError('Forventet et JSON-objekt');
  const out = { ...defaultSettings(), ...base, thresholds: { ...defaultSettings().thresholds, ...(base.thresholds || {}) } };
  const P = patch;
  if (P.intervalMs !== undefined) out.intervalMs = oneOf(P.intervalMs, INTERVALS, 'Ugyldig intervall');
  if (P.unit !== undefined) out.unit = oneOf(P.unit, ['C', 'F'], 'Enhet må være C eller F');
  if (P.startSimulated !== undefined) out.startSimulated = bool(P.startSimulated, 'startSimulated');
  if (P.theme !== undefined) out.theme = oneOf(P.theme, THEMES, 'Tema må være system, light eller dark');
  if (P.accent !== undefined) out.accent = OLD_ACCENTS.includes(P.accent) ? 'blue' : oneOf(P.accent, ACCENTS, 'Ukjent aksentfarge');
  if (P.palette !== undefined) out.palette = oneOf(P.palette, PALETTES, 'Ukjent fargetema');
  if (P.highContrast !== undefined) out.highContrast = bool(P.highContrast, 'highContrast');
  if (P.density !== undefined) out.density = oneOf(P.density, DENSITIES, 'Ukjent tetthet');
  if (P.fontScale !== undefined) out.fontScale = oneOf(P.fontScale, FONT_SCALES, 'Ugyldig skriftstørrelse');
  if (P.motion !== undefined) out.motion = oneOf(P.motion, MOTIONS, 'Animasjoner må være system, full, reduced eller off');
  if (P.startPage !== undefined) out.startPage = oneOf(P.startPage, [...PAGES, 'last'], 'Ukjent startside');
  if (P.showKbd !== undefined) out.showKbd = bool(P.showKbd, 'showKbd');
  if (P.hiddenPages !== undefined) out.hiddenPages = subset(P.hiddenPages, PAGES, 'hiddenPages', { max: PAGES.length - 1 });
  if (P.statusItems !== undefined) out.statusItems = statusList(P.statusItems);
  if (P.statusSparks !== undefined) out.statusSparks = bool(P.statusSparks, 'statusSparks');
  if (P.sideCollapsed !== undefined) out.sideCollapsed = bool(P.sideCollapsed, 'sideCollapsed');
  if (P.thresholds !== undefined) {
    if (!isObj(P.thresholds)) throw new ValidationError('thresholds må være et objekt');
    for (const [k, r] of Object.entries(THRESH)) {
      const v = P.thresholds[k];
      if (v === undefined) continue;
      if (!isNum(v)) throw new ValidationError(`${k} må være et tall`);
      out.thresholds[k] = Math.round(clamp(v, r));
    }
    const t = out.thresholds;
    if (t.gpuTempCrit <= t.gpuTempWarn) throw new ValidationError('Rød grense for GPU-temperatur må være over gul');
    if (t.cpuTempCrit <= t.cpuTempWarn) throw new ValidationError('Rød grense for CPU-temperatur må være over gul');
  }
  if (P.chartWindow !== undefined) out.chartWindow = P.chartWindow === 120 ? 300 : oneOf(P.chartWindow, CHART_WINDOWS, `Tidsvindu må være en av ${CHART_WINDOWS.join(', ')} s`);
  if (P.windows !== undefined) {
    if (!isObj(P.windows)) throw new ValidationError('windows må være et objekt');
    const w = { ...defaultSettings().windows, ...(base.windows || {}) };
    for (const [k, v] of Object.entries(P.windows)) {
      if (!WINDOW_PAGES.includes(k)) throw new ValidationError(`Ukjent side i windows: ${String(k).slice(0, 20)}`);
      w[k] = oneOf(v, CHART_WINDOWS, `Tidsvindu må være en av ${CHART_WINDOWS.join(', ')} s`);
    }
    out.windows = w;
  }
  if (P.chartSync !== undefined) out.chartSync = bool(P.chartSync, 'chartSync');
  if (P.chartScale !== undefined) out.chartScale = oneOf(P.chartScale, ['auto', 'fixed'], 'chartScale må være auto eller fixed');
  if (P.chartFill !== undefined) out.chartFill = bool(P.chartFill, 'chartFill');
  if (P.charts !== undefined) out.charts = subset(P.charts, CHARTS, 'charts', { min: 1 });
  if (P.sensorSparks !== undefined) out.sensorSparks = bool(P.sensorSparks, 'sensorSparks');
  if (P.sensorKeepMinMax !== undefined) out.sensorKeepMinMax = bool(P.sensorKeepMinMax, 'sensorKeepMinMax');
  if (P.decimals !== undefined) out.decimals = oneOf(P.decimals, ['auto', '0', '1'], 'Desimaler må være auto, 0 eller 1');
  if (P.memUnit !== undefined) out.memUnit = oneOf(P.memUnit, ['bin', 'dec'], 'memUnit må være bin eller dec');
  if (P.netUnit !== undefined) out.netUnit = oneOf(P.netUnit, ['byte', 'bit'], 'netUnit må være byte eller bit');
  if (P.clock !== undefined) out.clock = oneOf(P.clock, ['system', '24', '12'], 'Klokke må være system, 24 eller 12');
  if (P.dismissed !== undefined) out.dismissed = subset(P.dismissed, INFOBARS, 'dismissed');
  if (P.confirmSec !== undefined) out.confirmSec = oneOf(P.confirmSec, CONFIRM_CHOICES, `Fristen må være ${CONFIRM_CHOICES.join(', ')} s`);
  if (P.surprises !== undefined) out.surprises = bool(P.surprises, 'surprises');
  return out;
}

export class Store {
  constructor(dir) {
    this.dir = dir;
    this.file = join(dir, 'state.json');
    this.state = {
      schema: SCHEMA,
      applied: defaultApplied(),
      rgb: defaultRgb(),
      settings: defaultSettings(),
      profiles: Array.from({ length: SLOTS }, () => null),
    };
    this.load();
  }

  load() {
    try {
      mkdirSync(this.dir, { recursive: true });
    } catch {}
    let text;
    try {
      text = readFileSync(this.file, 'utf8');
    } catch {
      return; // ingen fil ennå
    }
    if (!text.trim()) return; // tom fil: standardverdier
    let raw;
    try {
      raw = JSON.parse(text);
    } catch {
      this.backupCorrupt('ugyldig JSON');
      return;
    }
    if (!isObj(raw)) {
      this.backupCorrupt('ikke et JSON-objekt');
      return;
    }
    // Les defensivt, felt for felt, så ett dårlig felt (f.eks. fra en eldre
    // versjon eller en håndredigert fil) ikke nullstiller resten.
    // Før v0.0.7 betydde fanMode «auto» viftekurven (se legacyFanMode).
    const old = raw.schema == null;
    const migrate = (a) => (old && isObj(a) ? { ...a, fanMode: legacyFanMode(a) } : a);
    if (isObj(raw.applied)) this.state.applied = lenientApplied(migrate(raw.applied));
    if (isObj(raw.rgb)) this.state.rgb = lenientRgb(raw.rgb);
    if (isObj(raw.settings)) this.state.settings = lenientSettings(raw.settings);
    if (Array.isArray(raw.profiles)) {
      for (let i = 0; i < SLOTS; i++) {
        const p = raw.profiles[i];
        if (!isObj(p)) continue;
        this.state.profiles[i] = {
          name: cleanName(p.name, i),
          savedAt: typeof p.savedAt === 'string' && !Number.isNaN(Date.parse(p.savedAt)) ? p.savedAt : null,
          values: lenientApplied(isObj(p.values) ? migrate(p.values) : {}),
        };
      }
    }
  }

  /** Tar vare på en ødelagt state.json før den overskrives ved neste lagring. */
  backupCorrupt(why) {
    const dest = `${this.file}.ødelagt-${new Date().toISOString().replace(/[:.]/g, '-')}`;
    try {
      copyFileSync(this.file, dest);
      this.loadNote = `state.json kunne ikke leses (${why}). Kopi: ${dest}. Standardverdier brukes.`;
    } catch {
      this.loadNote = `state.json kunne ikke leses (${why}). Standardverdier brukes.`;
    }
  }

  save() {
    const tmp = `${this.file}.${process.pid}.tmp`;
    writeFileSync(tmp, JSON.stringify(this.state, null, 2));
    renameSync(tmp, this.file);
  }

  setApplied(patch) {
    this.state.applied = mergeApplied(this.state.applied, patch);
    this.save();
    return this.state.applied;
  }

  /**
   * Siste kjente verdier fra bakgrunnstjenesten (den bestemmer). Lagres så
   * appen har noe å vise og simulere hvis tjenesten ikke svarer ved oppstart.
   */
  cacheApplied(flat) {
    const next = lenientApplied(flat);
    if (JSON.stringify(next) === JSON.stringify(this.state.applied)) return this.state.applied;
    this.state.applied = next;
    this.save();
    return this.state.applied;
  }

  resetApplied() {
    this.state.applied = defaultApplied();
    this.save();
    return this.state.applied;
  }

  setRgb(patch) {
    this.state.rgb = mergeRgb(this.state.rgb, patch);
    this.save();
    return this.state.rgb;
  }

  setSettings(patch) {
    this.state.settings = mergeSettings(this.state.settings, patch);
    this.save();
    return this.state.settings;
  }

  /** Erstatter alle app-innstillinger (import). Ukjente felt ignoreres, ugyldige avvises. */
  replaceSettings(settings) {
    this.state.settings = mergeSettings(defaultSettings(), settings);
    this.save();
    return this.state.settings;
  }

  resetSettings() {
    this.state.settings = defaultSettings();
    this.save();
    return this.state.settings;
  }

  saveProfile(slot, body) {
    if (!isObj(body)) throw new ValidationError('Forventet et JSON-objekt');
    const values = mergeApplied(defaultApplied(), body.values ?? {});
    this.state.profiles[slot] = { name: this.uniqueName(body.name, slot), savedAt: new Date().toISOString(), values };
    this.save();
    return this.state.profiles[slot];
  }

  /** Gir en tømt profil tilbake (angre). */
  restoreProfile(slot, profile) {
    if (!isObj(profile)) throw new ValidationError('Forventet en profil');
    const values = mergeApplied(defaultApplied(), profile.values ?? {});
    const savedAt = typeof profile.savedAt === 'string' && !Number.isNaN(Date.parse(profile.savedAt)) ? profile.savedAt : new Date().toISOString();
    this.state.profiles[slot] = { name: this.uniqueName(profile.name, slot), savedAt, values };
    this.save();
    return this.state.profiles[slot];
  }

  renameProfile(slot, name) {
    const p = this.state.profiles[slot];
    if (!p) throw new ValidationError('Tom profilplass');
    p.name = this.uniqueName(name, slot);
    this.save();
    return p;
  }

  /** Navn som ikke allerede brukes av en annen plass: «Stille», «Stille (2)» … */
  uniqueName(name, slot) {
    const base = cleanName(name, slot);
    const taken = new Set(this.state.profiles.filter((p, i) => p && i !== slot).map((p) => p.name.toLocaleLowerCase('nb')));
    if (!taken.has(base.toLocaleLowerCase('nb'))) return base;
    for (let k = 2; ; k++) {
      const cand = `${base.slice(0, 32 - ` (${k})`.length)} (${k})`;
      if (!taken.has(cand.toLocaleLowerCase('nb'))) return cand;
    }
  }

  deleteProfile(slot) {
    this.state.profiles[slot] = null;
    this.save();
  }
}

/**
 * Simulerte verdier felt for felt. Ugyldig viftekurve gir standardkurven.
 * Start- og stopptemperaturen leses sammen (de avhenger av hverandre); et
 * ugyldig par gir standardparet. Filer fra før v0.0.6 har ingen av dem.
 */
function lenientApplied(raw) {
  let out = defaultApplied();
  for (const [k, v] of Object.entries(raw)) {
    if (k === 'fanStartTemp' || k === 'fanStopTemp') continue;
    try {
      out = mergeApplied(out, { [k]: v });
    } catch {}
  }
  if (raw.fanStartTemp !== undefined || raw.fanStopTemp !== undefined) {
    try {
      out = mergeApplied(out, { fanStartTemp: raw.fanStartTemp, fanStopTemp: raw.fanStopTemp });
    } catch {}
  }
  return out;
}

/** Lys felt for felt. tempMin/tempMax leses sammen (de avhenger av hverandre). */
function lenientRgb(raw) {
  let out = defaultRgb();
  for (const [k, v] of Object.entries(raw)) {
    if (k === 'tempMin' || k === 'tempMax' || k === 'zones') continue;
    try {
      out = mergeRgb(out, { [k]: v });
    } catch {}
  }
  // Eldre filer har bare én farge; den gjelder da alle soner (mergeRgb over).
  if (isObj(raw.zones)) {
    for (const z of ['bar', 'fan1', 'fan2']) {
      try {
        out = mergeRgb(out, { zones: { [z]: raw.zones[z] } });
      } catch {}
    }
  }
  try {
    out = mergeRgb(out, { tempMin: raw.tempMin, tempMax: raw.tempMax });
  } catch {}
  return out;
}

/** Leser lagrede innstillinger felt for felt, så ett dårlig felt ikke nullstiller resten. */
function lenientSettings(raw) {
  let out = defaultSettings();
  for (const [k, v] of Object.entries(raw)) {
    // Eldre filer kan ha chartWindow 60/120 og ingen nye felt; alt annet valideres som vanlig.
    try {
      out = mergeSettings(out, { [k]: v });
    } catch {}
  }
  return out;
}

function cleanName(name, slot) {
  const s = typeof name === 'string' ? name.replace(/[\u0000-\u001f]/g, '').trim().slice(0, 32) : '';
  return s || `Profil ${slot + 1}`;
}

// ---------------------------------------------------------------- Min/maks

/** Høyst så mange sensorrader i min/maks-fila (denne maskinen har ca. 60). */
export const MINMAX_MAX_ROWS = 1000;
const MINMAX_MODES = ['live', 'sim'];

/**
 * Validerer et min/maks-sett fra nettleseren:
 * { since: ISO-tid, mode: 'live' | 'sim', rows: { id: [min, max] }, replace?: bool }.
 * strict = false (lesing av fil) hopper over dårlige rader i stedet for å avvise alt.
 */
export function cleanMinMax(body, { strict = true } = {}) {
  if (!isObj(body)) throw new ValidationError('Forventet et JSON-objekt');
  const t = typeof body.since === 'string' ? Date.parse(body.since) : NaN;
  // Tidspunktet kan ikke ligge i framtiden (litt slark for klokkeforskjell mellom faner).
  if (!Number.isFinite(t) || t < Date.parse('2020-01-01') || t > Date.now() + 60_000) throw new ValidationError('since må være et gyldig tidspunkt');
  if (!MINMAX_MODES.includes(body.mode)) throw new ValidationError('mode må være live eller sim');
  if (!isObj(body.rows)) throw new ValidationError('rows må være et objekt');
  const ids = Object.keys(body.rows);
  if (ids.length > MINMAX_MAX_ROWS) throw new ValidationError(`For mange sensorer (maks ${MINMAX_MAX_ROWS})`);
  const rows = {};
  for (const id of ids) {
    const v = body.rows[id];
    const ok = SENSOR_ID.test(id) && Array.isArray(v) && v.length === 2 && isNum(v[0]) && isNum(v[1]) && v[0] <= v[1];
    if (!ok) {
      if (strict) throw new ValidationError(`Ugyldig min/maks for ${String(id).slice(0, 40)}`);
      continue;
    }
    rows[id] = [v[0], v[1]];
  }
  return { since: new Date(t).toISOString(), mode: body.mode, rows };
}

/**
 * Min og maks for Sensorer mellom økter, i AG_DATA_DIR/sensor-minmax.json.
 * Fanene sender sine tall sjelden (ca. hvert 10. s). Serveren fletter dem og
 * skriver fila høyst hvert `delayMs` (standard 30 s), og straks ved avslutning.
 * Fila er ikke en del av state.json, så den tas ikke med i eksport og
 * tilbakestilling, og en ødelagt fil gir bare et tomt sett.
 */
export class MinMaxStore {
  constructor(dir, { delayMs = 30_000 } = {}) {
    this.file = join(dir, 'sensor-minmax.json');
    this.delayMs = delayMs;
    this.data = null; // { since, mode, rows } eller null
    this.timer = null;
    this.writes = 0;
    try {
      mkdirSync(dir, { recursive: true });
    } catch {}
    let text = null;
    try {
      text = readFileSync(this.file, 'utf8');
    } catch {}
    if (text && text.trim()) {
      try {
        this.data = cleanMinMax(JSON.parse(text), { strict: false });
      } catch {
        // Ødelagt eller ukjent innhold: kopi for feilsøking, og et tomt sett.
        try {
          copyFileSync(this.file, `${this.file}.ødelagt-${new Date().toISOString().replace(/[:.]/g, '-')}`);
        } catch {}
        this.data = null;
        this.loadNote = 'sensor-minmax.json kunne ikke leses. Min og maks begynner på nytt.';
      }
    }
  }

  get() {
    return this.data ? { ...this.data, rows: { ...this.data.rows } } : { since: null, mode: null, rows: {} };
  }

  /**
   * Fletter et sett fra en fane og gir det lagrede settet tilbake.
   * - replace: fanen har nullstilt eller angret, settet erstattes.
   * - Nyere `since` enn det lagrede: nullstilt i en annen fane, erstattes.
   * - Eldre `since`: fanen har ikke fått med seg en nullstilling, ignoreres.
   * - Samme `since`: laveste min og høyeste maks per sensor.
   * Bytter datakilden (live ↔ simulert), gjelder GPU-radene bare den nye kilden.
   */
  put(body) {
    const replace = body?.replace === true;
    const inc = cleanMinMax(body);
    const cur = this.data;
    const ts = (x) => Date.parse(x.since);
    if (!cur || replace || ts(inc) > ts(cur) + 1000) this.data = inc;
    else if (ts(inc) >= ts(cur) - 1000) {
      const rows = { ...cur.rows };
      if (inc.mode !== cur.mode) for (const id of Object.keys(rows)) if (id.startsWith('gpu/')) delete rows[id];
      for (const [id, [lo, hi]] of Object.entries(inc.rows)) {
        const o = rows[id];
        rows[id] = o ? [Math.min(o[0], lo), Math.max(o[1], hi)] : [lo, hi];
      }
      if (Object.keys(rows).length > MINMAX_MAX_ROWS) throw new ValidationError(`For mange sensorer (maks ${MINMAX_MAX_ROWS})`);
      this.data = { since: cur.since, mode: inc.mode, rows };
    }
    this.schedule();
    return this.get();
  }

  /** Glemmer alt (valget «Husk min og maks» er slått av). */
  clear() {
    this.data = null;
    clearTimeout(this.timer);
    this.timer = null;
    try {
      rmSync(this.file, { force: true });
    } catch {}
  }

  schedule() {
    if (this.timer) return; // høyst én skriving per delayMs
    this.timer = setTimeout(() => this.flush(), this.delayMs);
    this.timer.unref?.();
  }

  /** Skriver straks (ved avslutning, og etter forsinkelsen). */
  flush() {
    clearTimeout(this.timer);
    this.timer = null;
    if (!this.data) return;
    const tmp = `${this.file}.${process.pid}.tmp`;
    try {
      writeFileSync(tmp, JSON.stringify(this.data));
      renameSync(tmp, this.file);
      this.writes++;
    } catch {}
  }
}
