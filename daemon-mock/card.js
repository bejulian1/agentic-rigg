// Simulert skjermkort bak bakgrunnstjenesten (daemon-mock). Ren simulering:
// ingenting her kaller NVML, nvidia-smi eller noe annet på maskinen.
//
// Her ligger alt den ekte daemonen må håndheve selv, uavhengig av klienten:
//   - grenser: verdier utenfor det kortet (eller tjenestens policy) tillater,
//     klemmes og rapporteres i `clamped`
//   - bekreft-eller-rull-tilbake: en skriving gir en midlertidig tilstand
//     (txn) med frist; uten `confirm` innen fristen rulles alt tilbake, og det
//     samme skjer når klienten som skrev, kobler fra
//   - nødregel: full vifte over 85 °C uansett modus, tilbake under 80 °C
//   - vifte til auto når tjenesten stopper (shutdown)
// Kontrakten står i docs/daemon-api.md.

import { EventEmitter } from 'node:events';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { ERR, getPath, setPath, toFlat, toSettings } from '../server/daemon-protocol.js';
import { SIM_STATIC, Simulator } from '../server/simulator.js';
import { legacyFanMode } from '../server/store.js';

export class DaemonError extends Error {
  constructor(code, message, details) {
    super(message);
    this.code = code;
    this.details = details;
  }
}

export const GPU_ID = '0';

/** Grenser kortet melder (målt på RTX 2080 med NVML, se research kap. 1). */
export const HARDWARE = Object.freeze({
  powerLimitW: { min: 105, max: 275, default: 245 },
  coreMhz: { min: -1000, max: 1000 },
  // NVML oppgir minne-offset i dobbel enhet (−2000…+6000). Kontrakten bruker
  // effektiv MHz, som er det appen viser; daemonen ganger med 2 mot NVML.
  memMhz: { min: -1000, max: 3000 },
  tempTargetC: { min: 65, max: 88, default: 83 },
  fanPercent: { min: 35, max: 100 },
  fans: 2,
});

/** Tjenestens egen policy (strammere enn kortet). Kan bare endres av root i den ekte daemonen. */
export const POLICY = Object.freeze({
  coreMhz: { min: -200, max: 300 },
  memMhz: { min: -1000, max: 1500 },
});

export const SAFETY = Object.freeze({ emergencyFanC: 85, emergencyReleaseC: 80, emergencyPercent: 100, fanOnExit: 'auto' });

export const DEFAULT_CURVE = Object.freeze([
  [30, 35],
  [50, 40],
  [62, 50],
  [72, 65],
  [82, 85],
  [90, 100],
]);

export function defaultSettings() {
  return {
    powerLimitW: HARDWARE.powerLimitW.default,
    clockOffsets: { coreMhz: 0, memMhz: 0 },
    tempTargetC: HARDWARE.tempTargetC.default,
    fan: {
      mode: 'auto',
      manualPercent: 50,
      curve: DEFAULT_CURVE.map((p) => [...p]),
      minPercent: 35,
      spindownC: 2,
      rampUpS: 2,
      rampDownS: 5,
      source: 'gpu',
      startStop: { enabled: false, startC: 55, stopC: 48 },
    },
  };
}

const clone = (v) => JSON.parse(JSON.stringify(v));
const isObj = (v) => v != null && typeof v === 'object' && !Array.isArray(v);
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/**
 * Numeriske felt: [sti, navn i meldinger, effektivt område, maskinvareområde, kilde].
 * Kilde er 'hardware' når grensen kommer fra kortet, 'policy' når tjenesten er strengere.
 */
const NUMERIC = [
  ['powerLimitW', 'Effektgrensen', HARDWARE.powerLimitW, HARDWARE.powerLimitW],
  ['clockOffsets.coreMhz', 'Kjerneklokke-offset', POLICY.coreMhz, HARDWARE.coreMhz],
  ['clockOffsets.memMhz', 'Minneklokke-offset', POLICY.memMhz, HARDWARE.memMhz],
  ['tempTargetC', 'Temperaturmålet', HARDWARE.tempTargetC, HARDWARE.tempTargetC],
  ['fan.manualPercent', 'Viftefarten', HARDWARE.fanPercent, HARDWARE.fanPercent],
  ['fan.minPercent', 'Minste viftefart', HARDWARE.fanPercent, HARDWARE.fanPercent],
  ['fan.spindownC', '«Senk først etter»', { min: 0, max: 10 }, null],
  ['fan.rampUpS', 'Reaksjonstid opp', { min: 0, max: 10 }, null],
  ['fan.rampDownS', 'Reaksjonstid ned', { min: 0, max: 20 }, null],
  ['fan.startStop.startC', 'Starttemperaturen', { min: 30, max: 90 }, null],
  ['fan.startStop.stopC', 'Stopptemperaturen', { min: 25, max: 87 }, null],
];
const NUM_BY_PATH = new Map(NUMERIC.map((n) => [n[0], n]));
const ALLOWED = {
  '': ['powerLimitW', 'clockOffsets', 'tempTargetC', 'fan'],
  clockOffsets: ['coreMhz', 'memMhz'],
  fan: ['mode', 'manualPercent', 'curve', 'minPercent', 'spindownC', 'rampUpS', 'rampDownS', 'source', 'startStop'],
  'fan.startStop': ['enabled', 'startC', 'stopC'],
};
export const START_STOP_GAP_C = 3;
export const CURVE = Object.freeze({ minPoints: 2, maxPoints: 10, tempMin: 0, tempMax: 110 });

/** Grensene slik get_limits gir dem. */
export function limitsView() {
  const r = (eff, hw, extra = {}) => ({ min: eff.min, max: eff.max, ...extra, ...(hw && (hw.min !== eff.min || hw.max !== eff.max) ? { hardware: { min: hw.min, max: hw.max } } : {}), source: hw && (hw.min !== eff.min || hw.max !== eff.max) ? 'policy' : hw ? 'hardware' : 'daemon' });
  return {
    gpu: GPU_ID,
    powerLimitW: r(HARDWARE.powerLimitW, HARDWARE.powerLimitW, { default: HARDWARE.powerLimitW.default, step: 1, unit: 'W' }),
    clockOffsets: {
      coreMhz: r(POLICY.coreMhz, HARDWARE.coreMhz, { default: 0, step: 5, unit: 'MHz' }),
      memMhz: r(POLICY.memMhz, HARDWARE.memMhz, { default: 0, step: 25, unit: 'MHz', note: 'Effektiv MHz. NVML bruker dobbel verdi.' }),
    },
    tempTargetC: r(HARDWARE.tempTargetC, HARDWARE.tempTargetC, { default: HARDWARE.tempTargetC.default, step: 1, unit: '°C' }),
    fan: {
      count: HARDWARE.fans,
      modes: ['auto', 'curve', 'manual'],
      percent: r(HARDWARE.fanPercent, HARDWARE.fanPercent, { unit: '%' }),
      zeroRpm: false,
      curve: { ...CURVE },
      minPercent: r(HARDWARE.fanPercent, HARDWARE.fanPercent, { unit: '%' }),
      spindownC: r({ min: 0, max: 10 }, null, { unit: '°C' }),
      rampUpS: r({ min: 0, max: 10 }, null, { unit: 's' }),
      rampDownS: r({ min: 0, max: 20 }, null, { unit: 's' }),
      startC: r({ min: 30, max: 90 }, null, { unit: '°C' }),
      stopC: r({ min: 25, max: 87 }, null, { unit: '°C' }),
      startStopGapC: START_STOP_GAP_C,
      sources: ['gpu', 'cpu'],
    },
    safety: { ...SAFETY },
    unsupported: {
      memLockedClocks: 'Låst minneklokke krever Ampere eller nyere.',
      voltageOffset: 'NVIDIA har ingen spenningsoffset på Linux.',
      zeroRpm: 'Kortet tillater ikke under 35 %.',
    },
  };
}

function checkKeys(obj, at) {
  for (const k of Object.keys(obj)) {
    if (!ALLOWED[at].includes(k)) throw new DaemonError(ERR.INVALID_ARGUMENT, `Ukjent felt: ${at ? `${at}.` : ''}${String(k).slice(0, 40)}`, { field: at ? `${at}.${k}` : k });
  }
}

/**
 * Fletter en delvis GpuSettings inn i base. Ukjente felt og feil typer gir
 * invalid_argument. Tall utenfor grensene klemmes og står i `clamped`.
 * Returnerer { settings, clamped, changed }.
 */
export function mergeSettings(base, patch) {
  if (!isObj(patch)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'settings må være et objekt');
  checkKeys(patch, '');
  if (patch.clockOffsets !== undefined) {
    if (!isObj(patch.clockOffsets)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'clockOffsets må være et objekt', { field: 'clockOffsets' });
    checkKeys(patch.clockOffsets, 'clockOffsets');
  }
  if (patch.fan !== undefined) {
    if (!isObj(patch.fan)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan må være et objekt', { field: 'fan' });
    checkKeys(patch.fan, 'fan');
    if (patch.fan.startStop !== undefined) {
      if (!isObj(patch.fan.startStop)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan.startStop må være et objekt', { field: 'fan.startStop' });
      checkKeys(patch.fan.startStop, 'fan.startStop');
    }
  }
  const out = clone(base);
  const clamped = [];
  for (const [path, name, eff, hw] of NUMERIC) {
    const v = getPath(patch, path);
    if (v === undefined) continue;
    if (!isNum(v)) throw new DaemonError(ERR.INVALID_ARGUMENT, `${name} må være et tall`, { field: path });
    const r = Math.round(v);
    const c = Math.min(eff.max, Math.max(eff.min, r));
    if (c !== r) {
      const bound = r > eff.max ? 'max' : 'min';
      const fromHw = hw && hw[bound] === eff[bound];
      clamped.push({ field: path, requested: v, applied: c, bound, source: hw ? (fromHw ? 'hardware' : 'policy') : 'daemon' });
    }
    setPath(out, path, c);
  }
  const mode = getPath(patch, 'fan.mode');
  if (mode !== undefined) {
    if (!['auto', 'curve', 'manual'].includes(mode)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan.mode må være auto, curve eller manual', { field: 'fan.mode' });
    out.fan.mode = mode;
  }
  const src = getPath(patch, 'fan.source');
  if (src !== undefined) {
    if (src !== 'gpu' && src !== 'cpu') throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan.source må være gpu eller cpu', { field: 'fan.source' });
    out.fan.source = src;
  }
  const en = getPath(patch, 'fan.startStop.enabled');
  if (en !== undefined) {
    if (typeof en !== 'boolean') throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan.startStop.enabled må være true eller false', { field: 'fan.startStop.enabled' });
    out.fan.startStop.enabled = en;
  }
  if (out.fan.startStop.startC - out.fan.startStop.stopC < START_STOP_GAP_C) {
    throw new DaemonError(ERR.INVALID_ARGUMENT, `Starttemperaturen må være minst ${START_STOP_GAP_C} °C over stopptemperaturen`, { field: 'fan.startStop' });
  }
  const curve = getPath(patch, 'fan.curve');
  if (curve !== undefined) {
    const r = checkCurve(curve);
    out.fan.curve = r.points;
    if (r.lifted.length) clamped.push({ field: 'fan.curve', points: r.lifted, applied: r.points, bound: 'min', source: 'hardware' });
  }
  const changed = diffPaths(base, out);
  return { settings: out, clamped, changed };
}

/** Viftekurve: 2–10 punkter, stigende temperatur, fart 35–100 % og aldri fallende. */
function checkCurve(c) {
  if (!Array.isArray(c) || c.length < CURVE.minPoints || c.length > CURVE.maxPoints) throw new DaemonError(ERR.INVALID_ARGUMENT, `Viftekurven må ha ${CURVE.minPoints}–${CURVE.maxPoints} punkter`, { field: 'fan.curve' });
  const lifted = [];
  const pts = c.map((p, i) => {
    if (!Array.isArray(p) || p.length !== 2 || !isNum(p[0]) || !isNum(p[1])) throw new DaemonError(ERR.INVALID_ARGUMENT, `Ugyldig punkt ${i + 1} i viftekurven`, { field: 'fan.curve', index: i });
    const t = Math.round(p[0]);
    if (t < CURVE.tempMin || t > CURVE.tempMax) throw new DaemonError(ERR.INVALID_ARGUMENT, `Punkt ${i + 1}: temperaturen må være ${CURVE.tempMin}–${CURVE.tempMax} °C`, { field: 'fan.curve', index: i });
    let f = Math.round(p[1]);
    const lim = HARDWARE.fanPercent;
    if (f < lim.min || f > lim.max) {
      lifted.push(i);
      f = Math.min(lim.max, Math.max(lim.min, f));
    }
    return [t, f];
  });
  for (let i = 1; i < pts.length; i++) {
    if (pts[i][0] <= pts[i - 1][0]) throw new DaemonError(ERR.INVALID_ARGUMENT, 'Temperaturene i viftekurven må stige', { field: 'fan.curve', index: i });
    // Kurven skal aldri falle: punktet løftes til forrige fart.
    if (pts[i][1] < pts[i - 1][1]) {
      pts[i][1] = pts[i - 1][1];
      if (!lifted.includes(i)) lifted.push(i);
    }
  }
  return { points: pts, lifted };
}

/** Alle bladstier i et objekt ({ fan: { curve } } → ['fan.curve']). */
export function leafPaths(o, at = '') {
  const out = [];
  for (const [k, v] of Object.entries(o || {})) {
    const p = at ? `${at}.${k}` : k;
    if (isObj(v)) out.push(...leafPaths(v, p));
    else out.push(p);
  }
  return out;
}

/** Stiene (bladnoder) der a og b er ulike. Kurven regnes som én verdi. */
export function diffPaths(a, b, at = '') {
  const out = [];
  for (const k of new Set([...Object.keys(a || {}), ...Object.keys(b || {})])) {
    const p = at ? `${at}.${k}` : k;
    const x = a?.[k];
    const y = b?.[k];
    if (isObj(x) && isObj(y)) out.push(...diffPaths(x, y, p));
    else if (JSON.stringify(x) !== JSON.stringify(y)) out.push(p);
  }
  return out;
}

/**
 * Kortet og tjenestens tilstand. Hendelser: 'telemetry' (prøve), 'state',
 * 'txn' ({ txn, status, reason }), 'safety' ({ emergency, tempC }).
 */
export class MockCard extends EventEmitter {
  constructor({ dataDir, confirm = { defaultS: 15, minS: 10, maxS: 60 }, log = () => {}, now = () => Date.now() } = {}) {
    super();
    this.dataDir = dataDir;
    this.file = dataDir ? join(dataDir, 'daemon-mock.json') : null;
    this.confirmCfg = confirm;
    this.log = log;
    this.now = now;
    this.confirmed = defaultSettings();
    this.active = clone(this.confirmed);
    this.pending = null;
    this.emergency = false;
    this.lastExit = null;
    this.latest = null;
    this.txnSeq = 0;
    this.stopping = false;
    this.load();
    this.sim = new Simulator(() => this.simApplied());
  }

  /** Kalles når socketen lytter: fila sier «kjører» til tjenesten stopper. */
  started() {
    this.persist({ running: true });
  }

  // ---------------------------------------------------------------- lagring

  load() {
    if (!this.file) return;
    try {
      mkdirSync(this.dataDir, { recursive: true });
    } catch {}
    let raw = null;
    try {
      raw = JSON.parse(readFileSync(this.file, 'utf8'));
    } catch {}
    if (isObj(raw) && isObj(raw.confirmed)) {
      try {
        this.confirmed = mergeSettings(defaultSettings(), raw.confirmed).settings;
      } catch {
        this.log('daemon-mock.json hadde ugyldige verdier. Standard brukes.');
      }
      // Fila sier «kjører» hvis forrige prosess døde uten å rydde (SIGKILL, krasj).
      this.lastExit = isObj(raw.lastExit) ? raw.lastExit : null;
      if (raw.running) this.lastExit = { at: raw.startedAt || null, clean: false, reason: 'ukjent (prosessen ble drept eller krasjet)', fan: SAFETY.fanOnExit };
    } else this.importLegacy();
    this.active = clone(this.confirmed);
  }

  /**
   * Første oppstart: verdiene fra demoens state.json (v0.0.6 og eldre) tas
   * med som bekreftet oppsett, så ingenting forsvinner ved oppgraderingen.
   */
  importLegacy() {
    const legacy = join(this.dataDir, 'state.json');
    if (!existsSync(legacy)) return;
    try {
      const st = JSON.parse(readFileSync(legacy, 'utf8'));
      if (!isObj(st?.applied)) return;
      const flat = { ...st.applied };
      if (st.schema == null) flat.fanMode = legacyFanMode(flat);
      // Felt for felt, så ett dårlig felt ikke stopper resten.
      let s = defaultSettings();
      for (const [k, v] of Object.entries(flat)) {
        if (k === 'fanStartTemp' || k === 'fanStopTemp') continue;
        try {
          s = mergeSettings(s, toSettings({ [k]: v })).settings;
        } catch {}
      }
      try {
        s = mergeSettings(s, toSettings({ fanStartTemp: flat.fanStartTemp, fanStopTemp: flat.fanStopTemp })).settings;
      } catch {}
      this.confirmed = s;
      this.log('Tok med de simulerte verdiene fra state.json (første oppstart).');
    } catch {}
  }

  persist(extra = {}) {
    if (!this.file) return;
    const body = { version: 1, confirmed: this.confirmed, running: !this.stopping, startedAt: this.startedAt || (this.startedAt = new Date(this.now()).toISOString()), lastExit: this.lastExit, ...extra };
    const tmp = `${this.file}.${process.pid}.tmp`;
    try {
      writeFileSync(tmp, JSON.stringify(body, null, 2), { mode: 0o600 });
      renameSync(tmp, this.file);
    } catch (err) {
      this.log(`Kunne ikke lagre daemon-mock.json: ${err.message}`);
    }
  }

  // ---------------------------------------------------------------- simulering

  simApplied() {
    const flat = toFlat(this.active);
    if (this.emergency) flat.forceFan = SAFETY.emergencyPercent;
    return flat;
  }

  /** Én prøve (hvert 500. ms). Nødregelen sjekkes her, som i den ekte viftesløyfa. */
  tick(dt = 0.5) {
    const s = this.sim.step(dt);
    const t = this.sim.temp;
    if (!this.emergency && t >= SAFETY.emergencyFanC) {
      this.emergency = true;
      this.log(`Nødregel: ${Math.round(t)} °C, viften settes til ${SAFETY.emergencyPercent} %`);
      this.emit('safety', { gpu: GPU_ID, emergency: true, tempC: Math.round(t), thresholdC: SAFETY.emergencyFanC, releaseC: SAFETY.emergencyReleaseC });
      this.emit('state', { reason: 'emergency' });
    } else if (this.emergency && t < SAFETY.emergencyReleaseC) {
      this.emergency = false;
      this.log(`Nødregel opphevet: ${Math.round(t)} °C`);
      this.emit('safety', { gpu: GPU_ID, emergency: false, tempC: Math.round(t), thresholdC: SAFETY.emergencyFanC, releaseC: SAFETY.emergencyReleaseC });
      this.emit('state', { reason: 'emergency_over' });
    }
    s.fanMode = this.active.fan.mode;
    s.emergency = this.emergency;
    s.simulated = true;
    s.source = 'daemon-mock';
    this.latest = s;
    this.emit('telemetry', s);
    return s;
  }

  /** Bare tester: holder temperaturen fast (null slipper den). */
  holdTemperature(c) {
    if (c !== null && !isNum(c)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'celsius må være et tall eller null');
    this.sim.holdTemp = c;
    if (c !== null) this.sim.temp = c;
    this.tick(0);
    return { heldC: c };
  }

  // ---------------------------------------------------------------- tilstand

  pendingView() {
    const p = this.pending;
    if (!p) return null;
    return { txn: p.txn, owner: p.owner, timeoutS: p.timeoutS, deadline: p.deadline, remainingMs: Math.max(0, p.deadline - this.now()), changed: diffPaths(this.confirmed, this.active), clamped: p.clamped };
  }

  stateView() {
    return {
      gpu: GPU_ID,
      active: clone(this.active),
      confirmed: clone(this.confirmed),
      pending: this.pendingView(),
      emergency: this.emergency,
      fanOnExit: SAFETY.fanOnExit,
    };
  }

  timeoutFor(v) {
    const c = this.confirmCfg;
    if (v === undefined) return c.defaultS;
    if (!isNum(v)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'confirmTimeoutS må være et tall', { field: 'confirmTimeoutS' });
    return Math.min(c.maxS, Math.max(c.minS, Math.round(v)));
  }

  /**
   * Skriver en delvis GpuSettings for en klient (session). Gir en ny eller
   * forlenget txn som må bekreftes innen fristen. Uten faktisk endring gis
   * ingen txn.
   */
  write(session, patch, { confirmTimeoutS } = {}) {
    if (this.stopping) throw new DaemonError(ERR.SHUTTING_DOWN, 'Tjenesten stopper');
    if (this.pending && this.pending.owner !== session) {
      throw new DaemonError(ERR.BUSY, 'En annen klient har endringer som venter på bekreftelse', { owner: this.pending.owner, remainingMs: Math.max(0, this.pending.deadline - this.now()) });
    }
    const timeoutS = this.timeoutFor(confirmTimeoutS);
    const { settings, clamped } = mergeSettings(this.active, patch);
    const changedFromConfirmed = diffPaths(this.confirmed, settings);
    if (!this.pending && !changedFromConfirmed.length) {
      return { txn: null, pending: null, clamped, changed: [], active: clone(this.active), confirmed: clone(this.confirmed) };
    }
    this.active = settings;
    if (!this.pending) {
      this.pending = { txn: `t${++this.txnSeq}`, owner: session, clamped: [], timer: null };
    }
    const p = this.pending;
    p.timeoutS = timeoutS;
    p.deadline = this.now() + timeoutS * 1000;
    // Klemminger gjelder til feltet skrives på nytt i samme txn.
    const touched = leafPaths(patch);
    p.clamped = [...p.clamped.filter((c) => !touched.includes(c.field)), ...clamped];
    clearTimeout(p.timer);
    p.timer = setTimeout(() => this.rollback('timeout'), timeoutS * 1000);
    p.timer.unref?.();
    this.log(`Endring ${p.txn} venter på bekreftelse (${timeoutS} s): ${changedFromConfirmed.join(', ') || 'ingen'}`);
    this.emit('txn', { gpu: GPU_ID, txn: p.txn, status: 'pending', deadline: p.deadline, remainingMs: timeoutS * 1000 });
    this.emit('state', { reason: 'write' });
    return { txn: p.txn, pending: this.pendingView(), clamped, changed: changedFromConfirmed, active: clone(this.active), confirmed: clone(this.confirmed) };
  }

  checkTxn(session, txn) {
    if (!this.pending) throw new DaemonError(ERR.NO_PENDING, 'Ingen endring venter på bekreftelse');
    if (txn !== this.pending.txn) throw new DaemonError(ERR.TXN_MISMATCH, 'Endringen finnes ikke lenger (rullet tilbake eller erstattet)', { pending: this.pending.txn });
    if (session !== this.pending.owner) throw new DaemonError(ERR.NOT_OWNER, 'Bare klienten som gjorde endringen, kan bekrefte eller rulle tilbake');
  }

  confirm(session, txn) {
    this.checkTxn(session, txn);
    const p = this.pending;
    clearTimeout(p.timer);
    this.confirmed = clone(this.active);
    this.pending = null;
    this.persist();
    this.log(`Endring ${p.txn} bekreftet`);
    this.emit('txn', { gpu: GPU_ID, txn: p.txn, status: 'confirmed', reason: 'confirm' });
    this.emit('state', { reason: 'confirm' });
    return this.stateView();
  }

  revert(session, txn) {
    this.checkTxn(session, txn);
    this.rollback('revert');
    return this.stateView();
  }

  /** Ruller tilbake til siste bekreftede oppsett. reason: timeout, revert, client_disconnected, reset, daemon_stopping. */
  rollback(reason) {
    const p = this.pending;
    if (!p) return false;
    clearTimeout(p.timer);
    this.active = clone(this.confirmed);
    this.pending = null;
    this.log(`Endring ${p.txn} rullet tilbake (${reason})`);
    this.emit('txn', { gpu: GPU_ID, txn: p.txn, status: 'rolled_back', reason });
    this.emit('state', { reason: 'rollback' });
    return true;
  }

  /** En klient koblet fra: dens ubekreftede endring rulles tilbake straks. */
  sessionClosed(session) {
    if (this.pending && this.pending.owner === session) this.rollback('client_disconnected');
  }

  /** Tilbake til kortets standard. Trygg retning, så den bekreftes straks. */
  reset(session) {
    if (this.stopping) throw new DaemonError(ERR.SHUTTING_DOWN, 'Tjenesten stopper');
    if (this.pending && this.pending.owner !== session) throw new DaemonError(ERR.BUSY, 'En annen klient har endringer som venter på bekreftelse', { owner: this.pending.owner });
    if (this.pending) this.rollback('reset');
    this.confirmed = defaultSettings();
    this.active = clone(this.confirmed);
    this.persist();
    this.log('Tilbakestilt til standard');
    this.emit('state', { reason: 'reset' });
    return this.stateView();
  }

  /**
   * Tjenesten stopper (signal eller krasj): ubekreftet endring rulles
   * tilbake, og viften går til driverens auto. I den ekte daemonen er det
   * nvmlDeviceSetDefaultFanSpeed_v2 for hver vifte (pluss ExecStopPost i
   * systemd hvis prosessen dør uten å rydde).
   */
  shutdown(reason, { clean = true } = {}) {
    if (this.stopping) return null;
    const rolledBack = this.rollback('daemon_stopping');
    this.stopping = true;
    this.active.fan.mode = SAFETY.fanOnExit;
    this.emergency = false;
    this.lastExit = { at: new Date(this.now()).toISOString(), clean, reason, fan: SAFETY.fanOnExit };
    this.persist({ running: false });
    this.log(`Stopper (${reason}). Viften er satt til auto.`);
    return { reason, fan: SAFETY.fanOnExit, rolledBack };
  }
}

export { SIM_STATIC };
