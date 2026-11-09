// Simulert skjermkort. Gir troverdige, levende tall uten å røre maskinvare.
// De simulerte «Bruk»-verdiene påvirker modellen: høyere effektgrense gir
// høyere klokke under last, klokke-offset gir høyere klokke og strøm, og
// viftekurven/manuell vifte styrer viften og dermed temperaturen.
//
// Viftemodus (v0.0.7): 'auto' er driverens egen styring (her en fast,
// innebygd kurve), 'curve' er brukerens kurve med start/stopp, «senk først
// etter» og reaksjonstid, og 'manual' er fast fart. `forceFan` (satt av
// bakgrunnstjenesten når nødregelen gjelder) overstyrer alt. `holdTemp` holder
// temperaturen fast (bare tester i daemon-mock).

export const SIM_STATIC = Object.freeze({
  name: 'NVIDIA GeForce RTX 2080',
  driver: 'simulert',
  vbios: '90.04.0B.40.8C',
  busId: '—',
  powerMin: 105,
  powerMax: 275,
  powerDefault: 245,
  powerLimit: 245,
  coreMax: 2190,
  smMax: 2190,
  memMax: 7000,
  vramTotal: 8192,
  uuid: 'GPU-00000000-0000-0000-0000-000000000000',
  pciDevice: Object.freeze({ vendor: '10de', device: '1e87' }),
  pciSub: Object.freeze({ vendor: '10de', device: '1e87' }),
  computeCap: '7.5',
  pcieGenMax: 3,
  pcieGenHostMax: 3,
  pcieWidthMax: 16,
  coreDefault: null,
  memDefault: null,
  simulated: true,
});

// Det `nvidia-smi -q` ville gitt, med verdiene research målte på kortet.
export const SIM_QUERY = Object.freeze({
  productBrand: 'GeForce',
  productArch: 'Turing',
  partNumber: '1E87-400-A1',
  cudaVersion: null,
  bar1Total: 256,
  bar1Used: 60,
  tempShutdown: 100,
  tempSlowdown: 97,
  tempMaxOperating: 88,
  tempTarget: 83,
  maxClockVideo: 2040,
  fanSpeedCount: 1,
  simulated: true,
});

const SCENES = [
  { name: 'tomgang', load: [2, 9], dur: [8, 16] },
  { name: 'skrivebord', load: [10, 28], dur: [6, 12] },
  { name: 'spill', load: [82, 99], dur: [14, 30] },
  { name: 'video', load: [30, 45], dur: [8, 14] },
  { name: 'tung', load: [97, 100], dur: [10, 20] },
];

/** Driverens egen viftestyring (auto), omtrent slik kortet gjør det uten appen. */
export const DRIVER_CURVE = Object.freeze([
  [30, 35],
  [50, 40],
  [62, 50],
  [72, 65],
  [82, 85],
  [90, 100],
]);

const rnd = (a, b) => a + Math.random() * (b - a);
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

export function curveAt(curve, temp) {
  if (!curve || !curve.length) return 50;
  if (temp <= curve[0][0]) return curve[0][1];
  for (let i = 1; i < curve.length; i++) {
    const [t1, f1] = curve[i];
    const [t0, f0] = curve[i - 1];
    if (temp <= t1) return f0 + ((f1 - f0) * (temp - t0)) / Math.max(1e-6, t1 - t0);
  }
  return curve[curve.length - 1][1];
}

/**
 * Start/stopp med hysterese (v0.0.6). Gir neste hviletilstand:
 * - av (fanStartStop false): null, kurven styrer alltid
 * - i hvile: blir der til temperaturen når starttemperaturen
 * - på kurven: går i hvile først når temperaturen er under stopptemperaturen
 * - første gang (prev null): i hvile hvis temperaturen er under start
 * Mellom stopp og start beholdes tilstanden, så viften ikke pendler.
 */
export function restState(prev, temp, a) {
  if (!a?.fanStartStop || temp == null || !Number.isFinite(temp)) return null;
  const start = a.fanStartTemp ?? 55;
  const stop = a.fanStopTemp ?? 48;
  if (prev == null) return temp < start;
  if (prev) return temp < start;
  return temp < stop;
}

export class Simulator {
  /** getApplied() returnerer gjeldende simulerte innstillinger. */
  constructor(getApplied) {
    this.getApplied = getApplied;
    this.temp = 34;
    this.load = 5;
    this.loadTarget = 5;
    this.memUtil = 3;
    this.power = 22;
    this.core = 300;
    this.mem = 405;
    this.fan = 35;
    this.fanRest = null; // start/stopp: true = hvile på minste fart
    this.holdTemp = null; // fast temperatur (bare tester)
    this.vram = 1150;
    this.vramTarget = 1150;
    this.sceneLeft = 0;
    this.pickScene(true);
  }

  pickScene(first = false) {
    const s = first ? SCENES[1] : SCENES[Math.floor(Math.random() * SCENES.length)];
    this.scene = s.name;
    this.loadTarget = rnd(s.load[0], s.load[1]);
    this.sceneLeft = rnd(s.dur[0], s.dur[1]);
    this.vramTarget = this.loadTarget > 60 ? rnd(4800, 7400) : this.loadTarget > 25 ? rnd(2000, 3200) : rnd(1050, 1500);
  }

  step(dt = 0.5) {
    const a = this.getApplied();
    this.sceneLeft -= dt;
    if (this.sceneLeft <= 0) this.pickScene();

    // Last med litt jitter og glatting.
    const jitter = this.loadTarget > 60 ? rnd(-4, 3) : rnd(-2, 2);
    this.load += (clamp(this.loadTarget + jitter, 0, 100) - this.load) * Math.min(1, dt * 1.8);
    const load = clamp(this.load, 0, 100);
    const busy = load > 22;

    // Klokker.
    const coreOffset = a.coreOffset || 0;
    const memOffset = a.memOffset || 0;
    const limit = clamp(a.powerLimit || 245, 105, 275);

    // Hva kortet «vil» trekke ved full boost.
    const boost = 1905 + coreOffset;
    const demandFull = 22 + (load / 100) * 238 * Math.pow(Math.max(0.5, boost / 1905), 1.6) * (1 + memOffset / 14000);
    let coreTarget;
    let powerTarget;
    if (!busy) {
      coreTarget = load < 10 ? 300 : 300 + (load - 10) * 45;
      powerTarget = 14 + load * 0.9 + (this.mem > 1000 ? 18 : 0);
    } else {
      // Effektgrensen kapper klokken når behovet overstiger grensen.
      const ratio = Math.min(1, limit / demandFull);
      coreTarget = boost * Math.pow(ratio, 0.42);
      // Temperaturtrinn som på ekte kort: ca. 15 MHz per 5 °C over 55 °C.
      if (this.temp > 55) coreTarget -= Math.floor((this.temp - 55) / 5) * 15;
      const target = clamp(a.tempTarget || 83, 65, 88);
      if (this.temp > target) coreTarget -= (this.temp - target) * 60;
      coreTarget = clamp(coreTarget + rnd(-8, 8), 300, 2190 + Math.max(0, coreOffset));
      powerTarget = Math.min(demandFull, limit) + rnd(-4, 2);
    }
    this.core += (coreTarget - this.core) * Math.min(1, dt * 3);
    const memTarget = busy || load > 12 ? 7000 + memOffset : 405;
    this.mem += (memTarget - this.mem) * Math.min(1, dt * 4);
    this.power += (powerTarget - this.power) * Math.min(1, dt * 2.5);
    this.power = Math.min(this.power, limit);

    // Vifte.
    // Auto følger kurven med minste fart, «senk først etter», start/stopp og
    // reaksjonstid (alt simulert).
    let fanTarget;
    let fanUp = a.fanRampUp ?? 2;
    let fanDown = a.fanRampDown ?? 5;
    if (Number.isFinite(a.forceFan)) {
      // Nødregelen: full fart straks, uansett modus.
      fanTarget = clamp(a.forceFan, 35, 100);
      fanUp = 0.5;
      this.fanRest = null;
    } else if (a.fanMode === 'manual') {
      fanTarget = clamp(a.fanPercent ?? 50, 35, 100);
      this.fanRest = null;
    } else if (a.fanMode !== 'curve') {
      // Driverens auto: innebygd kurve, litt hysterese, ingen start/stopp.
      const t = this.temp < (this.lastFanTemp ?? this.temp) ? this.temp + 3 : this.temp;
      this.lastFanTemp = this.temp;
      fanTarget = clamp(curveAt(DRIVER_CURVE, t), 35, 100);
      fanUp = 3;
      fanDown = 6;
      this.fanRest = null;
    } else {
      const floor = clamp(a.fanMinPercent ?? 35, 35, 100);
      const hyst = a.fanHysteresis ?? 2;
      // Når temperaturen synker, regnes kurven fra en litt høyere temperatur (hysterese).
      const t = this.temp < (this.lastFanTemp ?? this.temp) ? this.temp + hyst : this.temp;
      this.lastFanTemp = this.temp;
      this.fanRest = restState(this.fanRest, this.temp, a);
      // I hvile holder viften minste fart. Ekte 0 % går ikke på dette kortet.
      fanTarget = this.fanRest ? floor : clamp(curveAt(a.fanCurve, t), floor, 100);
    }
    const up = Math.max(0.25, fanUp);
    const down = Math.max(0.25, fanDown);
    const diff = fanTarget - this.fan;
    // Reaksjonstid: hvor mange sekunder det tar å nå målet (omtrent).
    this.fan += diff * Math.min(1, dt / (diff > 0 ? up : down));

    // Temperatur med termisk treghet. Mer vifte = kjøligere.
    const ambient = 26;
    // Temperaturmålet: kortet senker klokke/strøm når det nås.
    const tt = clamp(a.tempTarget || 83, 65, 88);
    if (busy && this.temp > tt) this.power -= (this.temp - tt) * 4 * dt;
    const tempTarget = ambient + this.power * 0.235 - (this.fan - 30) * 0.17;
    this.temp += (tempTarget - this.temp) * Math.min(1, dt / 9);
    if (Number.isFinite(this.holdTemp)) this.temp = this.holdTemp;

    this.memUtil += (clamp(load * 0.55 + rnd(-3, 3), 0, 100) - this.memUtil) * Math.min(1, dt * 2);
    this.vram += (this.vramTarget - this.vram) * Math.min(1, dt * 0.6);

    return {
      t: Date.now(),
      temp: Math.round(this.temp),
      gpuUtil: Math.round(load),
      memUtil: Math.round(clamp(this.memUtil, 0, 100)),
      coreClock: Math.round(this.core),
      memClock: Math.round(this.mem),
      power: Math.round(this.power * 100) / 100,
      powerLimit: limit,
      fan: Math.round(this.fan),
      vramUsed: Math.round(this.vram + rnd(-6, 6)),
      vramTotal: SIM_STATIC.vramTotal,
      pstate: busy ? 'P0' : load > 10 ? 'P5' : 'P8',
      smClock: Math.round(this.core),
      videoClock: Math.round(Math.min(2040, this.core * 0.9 + 270)),
      powerEnforced: limit,
      encUtil: this.scene === 'video' ? Math.round(rnd(8, 14)) : 0,
      decUtil: this.scene === 'video' ? Math.round(rnd(18, 30)) : 0,
      pcieGen: busy ? 3 : load > 10 ? 2 : 1,
      pcieWidth: 16,
      // Bare simulert: true når start/stopp holder viften på minste fart.
      fanRest: this.fanRest ?? null,
      throttle: busy && this.power >= limit - 3 ? '0x0000000000000004' : load < 10 ? '0x0000000000000001' : '0x0000000000000000',
    };
  }
}
