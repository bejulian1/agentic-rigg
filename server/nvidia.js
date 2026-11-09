// Kun lesing fra nvidia-smi.
//
// SIKKERHET: Denne fila er det eneste stedet som starter nvidia-smi.
// Argumentlistene under er faste konstanter. Ingenting fra brukeren, nettleseren
// eller miljøet havner i dem, og vi bruker aldri et shell. Ingen flagg som
// setter noe på kortet (-pl, -lgc, -lmc, -pm, -rgc, ...) skal noen gang legges til.
// Tre faste kall finnes:
//   1. STATIC_ARGS  – faste egenskaper, én gang ved oppstart
//   2. QUERY_ARGS   – `nvidia-smi -q --id=0`, én gang ved oppstart (grenser, CUDA, BAR1)
//   3. LOOP_ARGS    – én langlevende prosess med -lms 500 mens en nettleser lytter

import { execFile, spawn } from 'node:child_process';

const BIN = 'nvidia-smi';

// [felt i nvidia-smi, nøkkel i JS, type]
const LIVE = Object.freeze([
  ['timestamp', null, 's'],
  ['temperature.gpu', 'temp', 'n'],
  ['utilization.gpu', 'gpuUtil', 'n'],
  ['utilization.memory', 'memUtil', 'n'],
  ['clocks.gr', 'coreClock', 'n'],
  ['clocks.mem', 'memClock', 'n'],
  ['power.draw', 'power', 'n'],
  ['power.limit', 'powerLimit', 'n'],
  ['fan.speed', 'fan', 'n'],
  ['memory.used', 'vramUsed', 'n'],
  ['memory.total', 'vramTotal', 'n'],
  ['pstate', 'pstate', 's'],
  ['clocks.sm', 'smClock', 'n'],
  ['clocks.video', 'videoClock', 'n'],
  ['enforced.power.limit', 'powerEnforced', 'n'],
  ['utilization.encoder', 'encUtil', 'n'],
  ['utilization.decoder', 'decUtil', 'n'],
  ['pcie.link.gen.current', 'pcieGen', 'n'],
  ['pcie.link.width.current', 'pcieWidth', 'n'],
  ['clocks_event_reasons.active', 'throttle', 's'],
]);

const STATIC = Object.freeze([
  ['name', 'name', 's'],
  ['driver_version', 'driver', 's'],
  ['vbios_version', 'vbios', 's'],
  ['pci.bus_id', 'busId', 's'],
  ['power.min_limit', 'powerMin', 'n'],
  ['power.max_limit', 'powerMax', 'n'],
  ['power.default_limit', 'powerDefault', 'n'],
  ['clocks.max.gr', 'coreMax', 'n'],
  ['clocks.max.mem', 'memMax', 'n'],
  ['memory.total', 'vramTotal', 'n'],
  ['uuid', 'uuid', 's'],
  ['pci.device_id', 'pciDeviceId', 's'],
  ['pci.sub_device_id', 'pciSubId', 's'],
  ['compute_cap', 'computeCap', 's'],
  ['pcie.link.gen.max', 'pcieGenMax', 'n'],
  ['pcie.link.gen.hostmax', 'pcieGenHostMax', 'n'],
  ['pcie.link.width.max', 'pcieWidthMax', 'n'],
  ['clocks.max.sm', 'smMax', 'n'],
  ['clocks.default_applications.gr', 'coreDefault', 'n'],
  ['clocks.default_applications.mem', 'memDefault', 'n'],
  ['temperature.gpu.tlimit', 'tlimit', 'n'],
  ['power.limit', 'powerLimit', 'n'],
]);

// Faste, frosne argumentlister. Ikke bygg disse dynamisk.
const STATIC_ARGS = Object.freeze([
  `--query-gpu=${STATIC.map((f) => f[0]).join(',')}`,
  '--format=csv,noheader,nounits',
  '--id=0',
]);

const QUERY_ARGS = Object.freeze(['-q', '--id=0']);

const LOOP_ARGS = Object.freeze([
  `--query-gpu=${LIVE.map((f) => f[0]).join(',')}`,
  '--format=csv,noheader,nounits',
  '--id=0',
  '-lms',
  '500',
]);

export const POLL_MS = 500;

// Bare disse flaggene er lov. -q er «vis all info» og leser bare.
const READ_ONLY_FLAGS = new Set(['--query-gpu', '--format', '--id', '-lms', '-q']);
export function assertReadOnly(args) {
  for (const a of args) {
    const flag = a.split('=')[0];
    if (!READ_ONLY_FLAGS.has(flag) && !/^\d+$/.test(a)) {
      throw new Error(`Avvist nvidia-smi-argument: ${a}`);
    }
  }
}
assertReadOnly(STATIC_ARGS);
assertReadOnly(QUERY_ARGS);
assertReadOnly(LOOP_ARGS);

// Eksportert bare for testene i test/ (listene er frosne og kan ikke endres utenfra).
export { STATIC_ARGS, QUERY_ARGS, LOOP_ARGS };

function num(v) {
  if (v == null) return null;
  const s = String(v).trim();
  if (!s || s.startsWith('[') || /n\/a/i.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

function str(v) {
  const s = String(v ?? '').trim();
  return !s || s.startsWith('[') || /^n\/a$/i.test(s) ? null : s;
}

function parseCsv(line, fields) {
  const p = line.split(',').map((s) => s.trim());
  if (p.length < fields.length) return null;
  const out = {};
  fields.forEach(([, key, type], i) => {
    if (key) out[key] = type === 'n' ? num(p[i]) : str(p[i]);
  });
  return out;
}

/** «0x1E8710DE» → { vendor: '10de', device: '1e87' }. */
function splitPciId(v) {
  const m = /^0x([0-9a-f]{4})([0-9a-f]{4})$/i.exec(String(v || ''));
  return m ? { device: m[1].toLowerCase(), vendor: m[2].toLowerCase() } : null;
}

export function parseStatic(line) {
  const s = parseCsv(line, STATIC);
  if (!s) return null;
  s.pciDevice = splitPciId(s.pciDeviceId);
  s.pciSub = splitPciId(s.pciSubId);
  return s;
}

export function parseLive(line) {
  const s = parseCsv(line, LIVE);
  if (!s) return null;
  s.t = Date.now();
  if (s.temp == null && s.gpuUtil == null && s.coreClock == null) return null;
  return s;
}

/**
 * Tolker utvalgte linjer fra `nvidia-smi -q`. Seksjonene er innrykket, så vi
 * holder en sti av overskrifter og slår opp «Seksjon/Nøkkel».
 */
export function parseQuery(text) {
  const flat = new Map();
  const stack = [];
  for (const raw of String(text).split('\n')) {
    if (!raw.trim() || /^=+/.test(raw.trim())) continue;
    const indent = raw.length - raw.trimStart().length;
    const line = raw.trim();
    // «Nøkkel   : verdi». Linjer uten mellomrom før kolon (f.eks.
    // «GPU 00000000:1C:00.0») er seksjonsoverskrifter.
    const m = /^(.*?)\s+:\s?(.*)$/.exec(line);
    const key = (m ? m[1] : /^GPU [0-9A-F]{8}:/i.test(line) ? 'GPU' : line).trim();
    const val = m ? m[2].trim() : '';
    while (stack.length && stack[stack.length - 1].indent >= indent) stack.pop();
    const path = [...stack.map((s) => s.key), key].join('/');
    if (val === '') stack.push({ indent, key });
    else if (!flat.has(path)) flat.set(path, val);
  }
  const find = (suffix) => {
    for (const [k, v] of flat) if (k.endsWith(suffix)) return v;
    return null;
  };
  const mhz = (v) => num(String(v || '').replace(/\s*MHz$/i, ''));
  const temp = (v) => num(String(v || '').replace(/\s*C$/i, ''));
  const mib = (v) => num(String(v || '').replace(/\s*MiB$/i, ''));
  const watt = (v) => num(String(v || '').replace(/\s*W$/i, ''));
  return {
    productBrand: str(find('/Product Brand')),
    productArch: str(find('/Product Architecture')),
    partNumber: str(find('/GPU Part Number')),
    cudaVersion: str(find('CUDA UMD Version')) || str(String(find('CUDA Version') || '').replace(/\s*\[.*$/, '')),
    driverModel: str(find('/Driver Model/Current')),
    displayActive: str(find('/Display Active')),
    persistence: str(find('/Persistence Mode')),
    addressingMode: str(find('/Addressing Mode')),
    bar1Total: mib(find('/BAR1 Memory Usage/Total')),
    bar1Used: mib(find('/BAR1 Memory Usage/Used')),
    tempShutdown: temp(find('/Temperature/GPU Shutdown Temp')),
    tempSlowdown: temp(find('/Temperature/GPU Slowdown Temp')),
    tempMaxOperating: temp(find('/Temperature/GPU Max Operating Temp')),
    tempTarget: temp(find('/Temperature/GPU Target Temperature Specification') ?? find('/Temperature/GPU Target Temperature')),
    memTempMax: temp(find('/Temperature/Memory Max Operating Temp')),
    defaultClockGr: mhz(find('/Default Applications Clocks/Graphics')),
    defaultClockMem: mhz(find('/Default Applications Clocks/Memory')),
    maxClockVideo: mhz(find('/Max Clocks/Video')),
    deferredClockMem: mhz(find('/Deferred Clocks/Memory')),
    powerDefault: watt(find('/Default Power Limit')),
    fanSpeedCount: [...flat.keys()].filter((k) => /\/Fan Speed$/.test(k)).length,
    fanSpeedText: str(find('/Fan Speed')),
    gspFirmware: str(find('/GSP Firmware Version')),
    inforomImage: str(find('/Inforom Version/Image Version')),
    pcieGenDeviceMax: num(find('/PCIe Generation/Device Max')),
    pcieGenHostMax: num(find('/PCIe Generation/Host Max')),
  };
}

function run(args, timeout = 8000) {
  return new Promise((resolve, reject) => {
    execFile(BIN, args, { timeout, shell: false, windowsHide: true, maxBuffer: 1024 * 1024 }, (err, stdout) => {
      if (err) return reject(err);
      resolve(String(stdout));
    });
  });
}

/** Leser faste egenskaper én gang. Avvises hvis nvidia-smi mangler eller feiler. */
export async function readStatic() {
  const out = await run(STATIC_ARGS);
  const first = out.split('\n').find((l) => l.trim());
  const info = first && parseStatic(first);
  if (!info || !info.name) throw new Error('Kunne ikke tolke svar fra nvidia-smi');
  return info;
}

/** Leser `nvidia-smi -q` én gang. Gir null hvis det feiler (ikke kritisk). */
export function readQueryText() {
  return run(QUERY_ARGS, 10000);
}

export async function readQuery() {
  try {
    return parseQuery(await readQueryText());
  } catch {
    return null;
  }
}

/**
 * Starter én langlevende nvidia-smi-prosess som skriver en linje hvert 500. ms.
 * Returnerer { stop() }. onSample(sample), onExit(code, signal, stderr).
 */
export function startLoop(onSample, onExit) {
  const child = spawn(BIN, LOOP_ARGS, { shell: false, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  let buf = '';
  let errText = '';
  let stopped = false;

  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    buf += chunk;
    let i;
    while ((i = buf.indexOf('\n')) >= 0) {
      const line = buf.slice(0, i);
      buf = buf.slice(i + 1);
      if (!line.trim()) continue;
      const s = parseLive(line);
      if (s) onSample(s);
    }
    if (buf.length > 8192) buf = '';
  });
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (c) => {
    if (errText.length < 2000) errText += c;
  });
  child.on('error', (err) => {
    if (!stopped) onExit(null, null, String(err.message || err));
  });
  child.on('exit', (code, signal) => {
    if (!stopped) onExit(code, signal, errText.trim());
  });

  return {
    pid: child.pid,
    stop() {
      if (stopped) return;
      stopped = true;
      try {
        child.kill('SIGTERM');
      } catch {}
      const t = setTimeout(() => {
        try {
          if (child.exitCode == null && child.signalCode == null) child.kill('SIGKILL');
        } catch {}
      }, 1500);
      t.unref();
    },
  };
}
