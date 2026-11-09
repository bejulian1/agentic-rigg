// Systeminfo og sensorer, lest direkte fra /proc, /sys og /etc/os-release.
//
// SIKKERHET: Denne fila starter ingen programmer og skriver ingenting. Den leser
// bare filer som vanlige brukere har lov til å lese. Filer som bare root kan
// lese (serienumre, product_uuid, dmidecode-data) åpnes aldri. Hvis en lesing
// likevel gir EACCES, vises «krever root» i stedet for en feil.

import { accessSync, constants as FS, existsSync, readFileSync, readdirSync, readlinkSync } from 'node:fs';
import { basename, join } from 'node:path';
import os from 'node:os';

const CPU_DIR = '/sys/devices/system/cpu';
const DMI_DIR = '/sys/class/dmi/id';
const HWMON_DIR = '/sys/class/hwmon';
const NET_DIR = '/sys/class/net';
const PCI_DIR = '/sys/bus/pci/devices';

// DMI-filer vi leser. Serienumre og product_uuid er root-only og står bevisst ikke her.
const DMI_FILES = Object.freeze([
  'sys_vendor',
  'product_name',
  'product_version',
  'product_family',
  'board_vendor',
  'board_name',
  'board_version',
  'bios_vendor',
  'bios_version',
  'bios_date',
  'bios_release',
  'chassis_type',
]);

export const ROOT_NOTE = 'Krever bakgrunnstjeneste (root)';

/** Leser en tekstfil. Gir { v } eller { err: 'root' | 'mangler' | 'feil' }. */
function readText(path) {
  try {
    return { v: readFileSync(path, 'utf8').trim() };
  } catch (e) {
    if (e.code === 'EACCES' || e.code === 'EPERM') return { err: 'root' };
    if (e.code === 'ENOENT') return { err: 'mangler' };
    return { err: 'feil' };
  }
}
const txt = (p) => readText(p).v ?? null;
const int = (p) => {
  const v = txt(p);
  if (v == null) return null;
  const n = Number.parseInt(v, 10);
  return Number.isFinite(n) ? n : null;
};
const list = (dir) => {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
};

/** «0-3,8-11» → 8 */
function countCpuList(s) {
  if (!s) return 0;
  let n = 0;
  for (const part of s.split(',')) {
    const [a, b] = part.split('-').map(Number);
    n += Number.isFinite(b) ? b - a + 1 : 1;
  }
  return n;
}

/** «32K» → 32768 */
function parseSize(s) {
  const m = /^(\d+)\s*([KMG]?)/i.exec(s || '');
  if (!m) return null;
  const k = { '': 1, K: 1024, M: 1024 ** 2, G: 1024 ** 3 }[m[2].toUpperCase()];
  return Number(m[1]) * k;
}

function parseKeyValues(text, sep = ':') {
  const out = {};
  for (const line of String(text || '').split('\n')) {
    const i = line.indexOf(sep);
    if (i < 0) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

// ---------------- Faste opplysninger (leses én gang) ----------------

const IMPORTANT_FLAGS = [
  ['sse', 'SSE'],
  ['sse2', 'SSE2'],
  ['pni', 'SSE3'],
  ['ssse3', 'SSSE3'],
  ['sse4_1', 'SSE4.1'],
  ['sse4_2', 'SSE4.2'],
  ['sse4a', 'SSE4A'],
  ['avx', 'AVX'],
  ['avx2', 'AVX2'],
  ['avx512f', 'AVX-512F'],
  ['fma', 'FMA3'],
  ['f16c', 'F16C'],
  ['aes', 'AES-NI'],
  ['sha_ni', 'SHA'],
  ['pclmulqdq', 'CLMUL'],
  ['bmi1', 'BMI1'],
  ['bmi2', 'BMI2'],
  ['popcnt', 'POPCNT'],
  ['abm', 'ABM'],
  ['adx', 'ADX'],
  ['rdrand', 'RDRAND'],
  ['rdseed', 'RDSEED'],
  ['lm', 'x86-64'],
  ['svm', 'AMD-V'],
  ['vmx', 'VT-x'],
  ['nx', 'NX'],
];

function readCpu() {
  const raw = txt('/proc/cpuinfo') || '';
  const blocks = raw.split(/\n\s*\n/).filter((b) => b.trim());
  const first = parseKeyValues(blocks[0] || '');
  const threads = blocks.length || os.cpus().length;
  const coreIds = new Set();
  for (const b of blocks) {
    const kv = parseKeyValues(b);
    coreIds.add(`${kv['physical id'] ?? 0}:${kv['core id'] ?? kv.processor}`);
  }
  const flags = (first.flags || '').split(/\s+/).filter(Boolean);
  const flagSet = new Set(flags);
  const important = IMPORTANT_FLAGS.filter(([f]) => flagSet.has(f)).map(([f, label]) => ({ flag: f, label }));

  // Cache fra cpu0. Antall instanser = tråder / tråder som deler cachen.
  const caches = [];
  for (const idx of list(join(CPU_DIR, 'cpu0', 'cache')).filter((n) => n.startsWith('index')).sort()) {
    const d = join(CPU_DIR, 'cpu0', 'cache', idx);
    const size = parseSize(txt(join(d, 'size')));
    const shared = countCpuList(txt(join(d, 'shared_cpu_list'))) || 1;
    const instances = Math.max(1, Math.round(threads / shared));
    caches.push({
      level: int(join(d, 'level')),
      type: txt(join(d, 'type')),
      size,
      instances,
      total: size == null ? null : size * instances,
      ways: int(join(d, 'ways_of_associativity')),
      lineSize: int(join(d, 'coherency_line_size')),
      sharedBy: shared,
    });
  }

  const fd = join(CPU_DIR, 'cpu0', 'cpufreq');
  const freq = {
    minKHz: int(join(fd, 'cpuinfo_min_freq')),
    maxKHz: int(join(fd, 'cpuinfo_max_freq')),
    driver: txt(join(fd, 'scaling_driver')),
    governors: (txt(join(fd, 'scaling_available_governors')) || '').split(/\s+/).filter(Boolean),
    available: !!txt(join(fd, 'scaling_driver')),
  };

  const family = first['cpu family'] != null ? Number(first['cpu family']) : null;
  const model = first.model != null ? Number(first.model) : null;
  const stepping = first.stepping != null ? Number(first.stepping) : null;
  return {
    name: (first['model name'] || os.cpus()[0]?.model || 'Ukjent').replace(/\s+/g, ' ').trim(),
    vendor: first.vendor_id || null,
    vendorName: { AuthenticAMD: 'AMD', GenuineIntel: 'Intel' }[first.vendor_id] || first.vendor_id || null,
    family,
    model,
    stepping,
    microcode: first.microcode || null,
    cores: coreIds.size || null,
    threads,
    caches,
    flags,
    important,
    freq,
    arch: os.arch(),
    bogomips: first.bogomips ? Number(first.bogomips) : null,
    addressSizes: first['address sizes'] || null,
  };
}

const CHASSIS = { 3: 'Stasjonær', 4: 'Lavprofil stasjonær', 6: 'Minitårn', 7: 'Tårn', 8: 'Bærbar', 9: 'Bærbar', 10: 'Bærbar', 13: 'Alt-i-ett', 35: 'Mini-PC' };

function readBoard() {
  const out = {};
  for (const f of DMI_FILES) {
    const r = readText(join(DMI_DIR, f));
    out[f] = r.v ?? (r.err === 'root' ? { root: true } : null);
  }
  // Plassholdere fra produsenten er ikke informasjon.
  for (const k of Object.keys(out)) {
    if (typeof out[k] === 'string' && /^(to be filled by o\.e\.m\.|default string|system product name|not applicable)$/i.test(out[k])) out[k] = null;
  }
  if (out.chassis_type) out.chassis = CHASSIS[Number(out.chassis_type)] || `Type ${out.chassis_type}`;
  return out;
}

function readOs() {
  const rel = parseKeyValues(txt('/etc/os-release') || '', '=');
  const unq = (s) => (s ? s.replace(/^"(.*)"$/, '$1') : null);
  const env = process.env;
  let session = null;
  let sessionSource = null;
  if (env.XDG_SESSION_TYPE) {
    session = env.XDG_SESSION_TYPE === 'wayland' ? 'Wayland' : env.XDG_SESSION_TYPE === 'x11' ? 'X11' : env.XDG_SESSION_TYPE;
    sessionSource = 'XDG_SESSION_TYPE';
  } else if (env.WAYLAND_DISPLAY) {
    session = 'Wayland';
    sessionSource = 'WAYLAND_DISPLAY';
  } else if (env.DISPLAY) {
    session = 'X11';
    sessionSource = 'DISPLAY';
  }
  return {
    distro: unq(rel.PRETTY_NAME) || unq(rel.NAME) || 'Linux',
    distroId: unq(rel.ID),
    version: unq(rel.VERSION),
    kernel: txt('/proc/sys/kernel/osrelease') || os.release(),
    kernelBuild: txt('/proc/sys/kernel/version'),
    arch: os.machine ? os.machine() : os.arch(),
    session,
    sessionSource,
    desktop: env.XDG_CURRENT_DESKTOP || null,
    hostname: txt('/proc/sys/kernel/hostname') || os.hostname(),
    node: process.version,
  };
}

/** Finner NVIDIA-kort i /sys/bus/pci og leser ID-er, PCIe-link og BAR-størrelser. */
export function readPciGpu(busId) {
  let addr = null;
  if (busId) {
    // nvidia-smi: «00000000:1C:00.0» → sysfs: «0000:1c:00.0»
    const m = /([0-9a-f]{4,8}):([0-9a-f]{2}):([0-9a-f]{2})\.([0-7])$/i.exec(busId);
    if (m) addr = `${m[1].slice(-4)}:${m[2]}:${m[3]}.${m[4]}`.toLowerCase();
  }
  if (!addr) {
    addr = list(PCI_DIR).find((d) => txt(join(PCI_DIR, d, 'vendor')) === '0x10de' && /^0x03/.test(txt(join(PCI_DIR, d, 'class')) || ''));
  }
  if (!addr) return null;
  const d = join(PCI_DIR, addr);
  const hex = (f) => (txt(join(d, f)) || '').replace(/^0x/, '').toLowerCase() || null;
  if (!hex('vendor')) return null;
  const bars = [];
  const res = (txt(join(d, 'resource')) || '').split('\n');
  res.slice(0, 6).forEach((line, i) => {
    const [a, b] = line.trim().split(/\s+/).map((x) => BigInt(x || '0'));
    if (b > a) bars.push({ bar: i, size: Number(b - a + 1n) });
  });
  // resourceN_resize: bitmaske over størrelser kortet støtter (bit n = 2^n MiB).
  let bar1Sizes = null;
  const rz = txt(join(d, 'resource1_resize'));
  if (rz && /^[0-9a-f]+$/i.test(rz)) {
    const mask = BigInt(`0x${rz}`);
    bar1Sizes = [];
    for (let n = 0n; n < 64n; n++) if ((mask >> n) & 1n) bar1Sizes.push(Number(2n ** n));
  }
  let driver = null;
  try {
    driver = basename(readlinkSync(join(d, 'driver')));
  } catch {}
  return {
    addr,
    vendor: hex('vendor'),
    device: hex('device'),
    subVendor: hex('subsystem_vendor'),
    subDevice: hex('subsystem_device'),
    revision: hex('revision'),
    linkSpeed: txt(join(d, 'current_link_speed')),
    linkSpeedMax: txt(join(d, 'max_link_speed')),
    linkWidth: int(join(d, 'current_link_width')),
    linkWidthMax: int(join(d, 'max_link_width')),
    bars,
    bar1Sizes, // MiB
    driver,
  };
}

let staticCache = null;
export function readStaticSystem() {
  if (staticCache) return staticCache;
  staticCache = {
    cpu: readCpu(),
    board: readBoard(),
    os: readOs(),
    memory: {
      modules: ROOT_NOTE,
      speed: ROOT_NOTE,
      timings: ROOT_NOTE,
      note: 'Moduler, hastighet og timinger ligger i SMBIOS-tabellen, som bare root kan lese.',
    },
    missing: [],
  };
  return staticCache;
}

// ---------------- Levende verdier ----------------

function readMeminfo() {
  const kv = parseKeyValues(txt('/proc/meminfo') || '');
  const kb = (k) => (kv[k] ? Number.parseInt(kv[k], 10) : null);
  const total = kb('MemTotal');
  const avail = kb('MemAvailable');
  const swapT = kb('SwapTotal');
  const swapF = kb('SwapFree');
  return {
    totalKB: total,
    availableKB: avail,
    usedKB: total != null && avail != null ? total - avail : null,
    freeKB: kb('MemFree'),
    buffersKB: kb('Buffers'),
    cachedKB: kb('Cached'),
    swapTotalKB: swapT,
    swapUsedKB: swapT != null && swapF != null ? swapT - swapF : null,
  };
}

function readProcStat() {
  const out = {};
  for (const line of (txt('/proc/stat') || '').split('\n')) {
    if (!line.startsWith('cpu')) break;
    const p = line.trim().split(/\s+/);
    const v = p.slice(1, 9).map(Number);
    const idle = v[3] + (v[4] || 0);
    const total = v.reduce((a, b) => a + b, 0);
    out[p[0]] = { idle, total };
  }
  return out;
}

function readCoreFreqs(threads) {
  const out = [];
  let fromCpuinfo = null;
  for (let i = 0; i < threads; i++) {
    let khz = int(join(CPU_DIR, `cpu${i}`, 'cpufreq', 'scaling_cur_freq'));
    if (khz == null) {
      if (!fromCpuinfo) {
        fromCpuinfo = (txt('/proc/cpuinfo') || '').split('\n').filter((l) => l.startsWith('cpu MHz')).map((l) => Number(l.split(':')[1]) * 1000);
      }
      khz = fromCpuinfo[i] ?? null;
    }
    out.push(khz == null ? null : Math.round(khz / 1000));
  }
  return out;
}

/** Leser alle hwmon-enheter: temp, fan, in, power (og curr). */
function readHwmon() {
  const devices = [];
  for (const h of list(HWMON_DIR).filter((n) => n.startsWith('hwmon')).sort((a, b) => Number(a.slice(5)) - Number(b.slice(5)))) {
    const dir = join(HWMON_DIR, h);
    const name = txt(join(dir, 'name')) || h;
    const sensors = [];
    for (const f of list(dir).sort()) {
      const m = /^(temp|fan|in|power|curr)(\d+)_input$/.exec(f);
      if (!m) continue;
      const r = readText(join(dir, f));
      const raw = r.v != null ? Number(r.v) : null;
      const kind = m[1];
      const label = txt(join(dir, `${kind}${m[2]}_label`));
      // Enheter i sysfs: milligrader, RPM, millivolt, mikrowatt, milliampere.
      const scale = { temp: 1000, fan: 1, in: 1000, power: 1e6, curr: 1000 }[kind];
      sensors.push({
        id: `${h}/${kind}${m[2]}`,
        kind,
        key: `${kind}${m[2]}`,
        label: label || `${kind}${m[2]}`,
        value: raw == null || !Number.isFinite(raw) ? null : raw / scale,
        err: r.err || null,
      });
    }
    devices.push({ id: h, name, sensors });
  }
  return devices;
}

function readNetCounters() {
  const out = {};
  for (const n of list(NET_DIR)) {
    if (n === 'lo') continue;
    const d = join(NET_DIR, n);
    out[n] = {
      rx: Number(txt(join(d, 'statistics', 'rx_bytes')) || 0),
      tx: Number(txt(join(d, 'statistics', 'tx_bytes')) || 0),
      state: txt(join(d, 'operstate')),
      wireless: list(join(d, 'wireless')).length > 0 || list(join(d, 'phy80211')).length > 0,
    };
  }
  return out;
}

const SYS_HISTORY = 120;

/** Poller /proc og /sys hvert sekund, men bare mens noen lytter. */
export class SystemMonitor {
  constructor({ intervalMs = 1000, onSample = () => {} } = {}) {
    this.intervalMs = intervalMs;
    this.onSample = onSample;
    this.timer = null;
    this.prevStat = null;
    this.prevNet = null;
    this.prevT = 0;
    this.history = [];
    this.latest = null;
    this.threads = readStaticSystem().cpu.threads;
  }

  get running() {
    return this.timer != null;
  }

  start() {
    if (this.timer) return;
    this.history = [];
    this.prevStat = readProcStat();
    this.prevNet = readNetCounters();
    this.prevT = Date.now();
    // Første prøve etter et kort øyeblikk, så last og nett har et grunnlag.
    this.timer = setTimeout(() => {
      this.tick();
      this.timer = setInterval(() => this.tick(), this.intervalMs);
    }, 250);
  }

  stop() {
    clearTimeout(this.timer);
    clearInterval(this.timer);
    this.timer = null;
  }

  /** Én prøve med en gang (for GET /api/sensors når ingen strøm kjører). */
  sampleOnce() {
    if (this.running && this.latest) return this.latest;
    // To kall rett etter hverandre gir ingen meningsfull rate; gjenbruk forrige.
    if (this.lastOnce && Date.now() - this.prevT < 900) return this.lastOnce;
    if (!this.prevStat) {
      this.prevStat = readProcStat();
      this.prevNet = readNetCounters();
      this.prevT = Date.now() - 1;
    }
    this.lastOnce = this.read();
    return this.lastOnce;
  }

  tick() {
    const s = this.read();
    this.history.push(s);
    if (this.history.length > SYS_HISTORY) this.history.splice(0, this.history.length - SYS_HISTORY);
    this.latest = s;
    this.onSample(s);
  }

  read() {
    const now = Date.now();
    const dt = Math.max(0.001, (now - this.prevT) / 1000);
    const stat = readProcStat();
    const load = (k) => {
      const a = this.prevStat?.[k];
      const b = stat[k];
      if (!a || !b) return null;
      const dTot = b.total - a.total;
      const dIdle = b.idle - a.idle;
      return dTot > 0 ? Math.round(Math.max(0, Math.min(100, (1 - dIdle / dTot) * 100)) * 10) / 10 : null;
    };
    const totalLoad = load('cpu');
    const coreLoad = [];
    for (let i = 0; i < this.threads; i++) coreLoad.push(load(`cpu${i}`));
    const net = readNetCounters();
    const netRates = {};
    for (const [n, c] of Object.entries(net)) {
      const p = this.prevNet?.[n];
      netRates[n] = {
        rx: p ? Math.max(0, (c.rx - p.rx) / dt) : null, // byte/s
        tx: p ? Math.max(0, (c.tx - p.tx) / dt) : null,
        state: c.state,
        wireless: c.wireless,
      };
    }
    this.prevStat = stat;
    this.prevNet = net;
    this.prevT = now;
    const governor = txt(join(CPU_DIR, 'cpu0', 'cpufreq', 'scaling_governor'));
    const boost = txt(join(CPU_DIR, 'cpufreq', 'boost'));
    return {
      t: now,
      cpu: {
        load: totalLoad,
        coreLoad,
        coreMHz: readCoreFreqs(this.threads),
        governor,
        boost: boost == null ? null : boost === '1',
      },
      mem: readMeminfo(),
      net: netRates,
      hwmon: readHwmon(),
      uptime: Number((txt('/proc/uptime') || '0').split(' ')[0]),
      loadavg: (txt('/proc/loadavg') || '').split(' ').slice(0, 3).map(Number),
    };
  }
}

/** Sjekker lesetilgang uten å åpne fila. */
function canRead(path) {
  try {
    accessSync(path, FS.R_OK);
    return true;
  } catch {
    return false;
  }
}

const BOARD_SENSOR_DRIVERS = /^(nct\d+|it\d+|f71\d+|w83\d+|asus.*|gigabyte_wmi|dell_smm|thinkpad|applesmc)$/;

/**
 * Forklarende linjer for sensorer som mangler. Dette er ikke feil, men
 * opplysninger som ikke kan leses uten root eller uten en annen driver.
 */
export function sensorNotes(hwmon, gpuMode) {
  const notes = [];
  const names = hwmon.map((d) => d.name);
  if (!names.some((n) => BOARD_SENSOR_DRIVERS.test(n))) {
    notes.push({
      group: 'Hovedkort',
      sensor: 'Vifter, spenninger og temperaturer',
      why: 'Hovedkortets sensorbrikke trenger en egen kjernedriver (for MSI B450 typisk nct6775), og den er ikke lastet.',
    });
  }
  notes.push({ group: 'Minne', sensor: 'Moduler, hastighet, timinger og temperatur', why: `${ROOT_NOTE}. SMBIOS/SPD kan bare leses av root.` });
  const k10 = hwmon.find((d) => d.name === 'k10temp');
  if (k10 && !k10.sensors.some((x) => /^Tccd/.test(x.label))) {
    notes.push({ group: 'CPU', sensor: 'Tccd (temperatur per CCD)', why: 'k10temp gir bare Tctl og Tdie for denne prosessoren. Tccd finnes fra Zen 2.' });
  }
  if (existsSync('/sys/class/powercap/intel-rapl:0/energy_uj') && !canRead('/sys/class/powercap/intel-rapl:0/energy_uj')) {
    notes.push({ group: 'CPU', sensor: 'Pakkeeffekt (RAPL)', why: `${ROOT_NOTE}. Energitelleren i /sys/class/powercap er bare lesbar for root.` });
  }
  notes.push({ group: 'GPU', sensor: 'Minnetemperatur og hotspot', why: 'nvidia-smi oppgir ikke disse for dette kortet (N/A).' });
  notes.push({ group: 'GPU', sensor: 'Viftens turtall (RPM) og kjernespenning', why: 'nvidia-smi gir bare viftefart i prosent og ingen spenning.' });
  if (gpuMode !== 'live') notes.push({ group: 'GPU', sensor: 'Alle GPU-verdier', why: 'Kilden er simulert. CPU, minne, nett og hwmon er fortsatt ekte.' });
  for (const d of hwmon) {
    for (const x of d.sensors) {
      if (x.err) notes.push({ group: 'Andre hwmon', sensor: `${d.name} · ${x.label}`, why: x.err === 'root' ? ROOT_NOTE : 'Kunne ikke leses akkurat nå (enheten kan være av).' });
    }
    if (!d.sensors.length) notes.push({ group: 'Andre hwmon', sensor: d.name, why: 'Enheten har ingen temp-, vifte-, spennings- eller effektverdier.' });
  }
  return notes;
}
