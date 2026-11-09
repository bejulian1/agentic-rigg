// Agentic Rigg – lokal server. Kun innebygde Node-moduler.
//
// SIKKERHET: Serveren leser bare fra skjermkortet (se nvidia.js). Alle
// skrivende handlinger («Bruk», bekreft, rull tilbake, tilbakestill) går til
// bakgrunnstjenesten over en Unix-socket (daemon-client.js). I denne versjonen
// er tjenesten daemon-mock/, som bare simulerer. Svaret har alltid
// simulated: true. Ingenting sendes til kortet.

import { createServer } from 'node:http';
import { readFileSync, statSync, createReadStream } from 'node:fs';
import { dirname, extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { History, WINDOWS as HISTORY_WINDOWS, KEY_RE, MAX_KEYS, MIN_POINTS, MAX_POINTS, gpuValues, sysValues, fillSynthetic } from './history.js';
import { cpuTemp, memUsedPct, netTotals } from '../web/js/sensormeta.js';
import { Store, MinMaxStore, SLOTS, ValidationError, LIMITS, EFFECTS, INTERVALS, DEFAULT_CURVE, FAN_START_GAP, STATUS_MAX, CONFIRM_CHOICES, defaultSettings, defaultApplied } from './store.js';
import { DaemonClient, DaemonError } from './daemon-client.js';
import { flatKey, socketPath, toFlat, toSettings } from './daemon-protocol.js';
import { Telemetry } from './telemetry.js';
import { SystemMonitor, readPciGpu, readStaticSystem, sensorNotes } from './system.js';
import { lookupSpec } from './specs.js';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WEB = join(ROOT, 'web');
const HOST = '127.0.0.1';
const PORT = Number.parseInt(process.env.AG_PORT || '4318', 10);
const DATA_DIR = resolve(process.env.AG_DATA_DIR || join(ROOT, '.data'));
const FORCE_SIM = /^(1|true|ja|yes)$/i.test(process.env.AG_SIMULATE || '');
const VERSION = JSON.parse(readFileSync(join(ROOT, 'version.json'), 'utf8'));
// Appnavnet kommer bare fra version.json, så et navnebytte er én linje.
const APP = Object.freeze({
  name: String(VERSION.name || 'Agentic Rigg'),
  version: VERSION.version,
  channel: VERSION.channel,
  build: VERSION.build,
  label: `${VERSION.channel} v${VERSION.version} · bygg ${VERSION.build}`,
  node: process.version,
});
const SLUG = APP.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');

if (!Number.isInteger(PORT) || PORT < 1 || PORT > 65535) {
  console.error(`Ugyldig AG_PORT: ${process.env.AG_PORT}`);
  process.exit(1);
}

const log = (...a) => console.log(`[${SLUG} ${new Date().toLocaleTimeString('nb-NO')}]`, ...a);

const store = new Store(DATA_DIR);
if (store.loadNote) log(store.loadNote);
// Historikk for grafene (v0.0.11): 1 s i 15 min, 10 s i 6 t og 60 s i 48 t,
// med min, snitt og maks per bøtte. Lagres i history.json hvert 5. minutt og
// ved avslutning. AG_TEST_HISTORY=1 fyller den med syntetiske 48 t (bare for
// tester og skjermbilder).
const history = new History({ dir: DATA_DIR, log });
history.load();
if (history.loadNote) log(history.loadNote);
if (/^(1|true|ja|yes)$/i.test(process.env.AG_TEST_HISTORY || '')) fillSynthetic(history, { mode: FORCE_SIM ? 'sim' : 'live' });
history.startAutosave();
// Min/maks på Sensorer mellom økter (egen fil, skrives sjelden).
const minmax = new MinMaxStore(DATA_DIR);
if (minmax.loadNote) log(minmax.loadNote);
// Systeminfo som ikke endrer seg, leses én gang ved oppstart.
readStaticSystem();
let telemetry = null;
const sysmon = new SystemMonitor({
  intervalMs: 1000,
  onSample: (s) => {
    history.ingest(s.t || Date.now(), sysValues(s, { netTotals, memUsedPct, cpuTemp }));
    telemetry?.broadcast('sys', s);
  },
});
// Bakgrunnstjenesten: alle skrivende handlinger går hit. Startes av
// scripts/dev.js (npm start), aldri av serveren selv.
const DAEMON_SOCKET = socketPath(process.env);
const daemon = new DaemonClient({ path: DAEMON_SOCKET, client: `${SLUG}/${APP.version}`, log });
telemetry = new Telemetry({ store, forceSim: FORCE_SIM, sysmon, log, daemon, onSample: (s, mode) => history.ingest(s.t || Date.now(), gpuValues(s, mode)) });

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

const SECURITY_HEADERS = {
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'no-referrer',
  'Content-Security-Policy':
    "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; frame-ancestors 'self'",
};

/** Utseendet som attributter på <html> (verdiene er allerede validert av store.js). */
function htmlPrefs(st) {
  const a = {
    'data-prefs': 'server',
    'data-accent': st.accent,
    'data-palette': st.palette,
    'data-contrast': st.highContrast ? 'high' : 'normal',
    'data-density': st.density,
    'data-kbd': st.showKbd ? 'on' : 'off',
    'data-motion-pref': st.motion,
    style: `--fs-scale: ${Number(st.fontScale) / 100}`,
  };
  if (st.theme === 'light' || st.theme === 'dark') a['data-theme'] = st.theme;
  return Object.entries(a)
    .map(([k, v]) => `${k}="${escHtml(String(v))}"`)
    .join(' ');
}

function allowedHost(h) {
  return h === `127.0.0.1:${PORT}` || h === `localhost:${PORT}`;
}

function json(res, status, body) {
  const data = JSON.stringify(body);
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', ...SECURITY_HEADERS });
  res.end(data);
}

function readBody(req, limit = 64 * 1024) {
  return new Promise((resolveBody, reject) => {
    let size = 0;
    const chunks = [];
    let tooBig = false;
    req.on('data', (c) => {
      if (tooBig) return;
      size += c.length;
      if (size > limit) {
        // Svar med en ordentlig feilmelding i stedet for å bryte forbindelsen
        // (da ser nettleseren bare «Failed to fetch»). Resten av kroppen leses
        // og kastes.
        tooBig = true;
        chunks.length = 0;
        const err = new ValidationError(`For stor forespørsel (maks ${Math.round(limit / 1024)} kB)`);
        err.status = 413;
        reject(err);
        return;
      }
      chunks.push(c);
    });
    req.on('end', () => {
      if (tooBig) return;
      if (!size) return resolveBody({});
      try {
        resolveBody(JSON.parse(Buffer.concat(chunks).toString('utf8')));
      } catch {
        reject(new ValidationError('Ugyldig JSON'));
      }
    });
    req.on('error', reject);
  });
}

function snapshot() {
  // Meldingen om en ødelagt state.json vises én gang (i første fane som åpnes).
  const storeNote = store.loadNote || null;
  store.loadNote = null;
  return {
    app: APP,
    telemetry: telemetry.publicInfo(),
    forceSim: FORCE_SIM,
    applied: store.state.applied,
    daemon: daemonView(),
    limitInfo: limitInfo(),
    confirmChoices: CONFIRM_CHOICES,
    rgb: store.state.rgb,
    settings: store.state.settings,
    profiles: store.state.profiles,
    limits: limitsFlat(),
    effects: EFFECTS,
    intervals: INTERVALS,
    defaultCurve: DEFAULT_CURVE,
    fanStartGap: FAN_START_GAP,
    statusMax: STATUS_MAX,
    defaultApplied: defaultApplied(),
    defaultSettings: defaultSettings(),
    dataDir: DATA_DIR,
    storeNote,
    rgbStatus: {
      connected: false,
      text: 'Lyskontroller: ikke koblet til (simulert)',
      detail: 'Kortet har trolig en Palit-lyskontroller. Den er ikke testet, og appen sender ingenting til den.',
    },
  };
}

// ---------------------------------------------------------------- Bakgrunnstjeneste

const SIM_NOTE = 'Simulert: ingenting ble sendt til skjermkortet.';
const FALLBACK_SAFETY = { emergencyFanC: 85, emergencyReleaseC: 80, emergencyPercent: 100, fanOnExit: 'auto' };
/** Hvilken fane som gjorde endringen som venter, så den kan rulles tilbake når fanen lukkes. */
let txnTab = null; // { txn, tab, reason? }
let applyInFlight = null; // { tab } mens et Bruk er på vei til tjenesten
/** Siste utfall (bekreftet, rullet tilbake, tjenesten stoppet), vist i nye faner i 60 s. */
let recent = null;
const tabStreams = new Map(); // tab-id → antall åpne SSE-strømmer

const clampFlat = (c) => ({ key: flatKey(c.field), field: c.field, requested: c.requested, applied: c.applied, bound: c.bound, source: c.source, points: c.points });
const uniq = (a) => [...new Set(a)];

/** Ventende endring i appens felt. `mine`: gjort av denne serveren. */
function pendingFlat(p, st) {
  if (!p) return null;
  return {
    txn: p.txn,
    timeoutS: p.timeoutS,
    remainingMs: Math.max(0, p.deadline - Date.now()),
    changed: uniq(p.changed.map((f) => flatKey(f) || (f.startsWith('fan.curve') ? 'fanCurve' : null)).filter(Boolean)),
    clamped: (p.clamped || []).map(clampFlat),
    before: toFlat(st.confirmed),
    mine: p.owner === daemon.hello?.session,
    // Fanen som trykket Bruk (v0.0.8). Alle faner viser endringen, men bare
    // denne kan bruke mer før den er beholdt eller rullet tilbake.
    // Hendelsene fra tjenesten kommer før svaret på Bruk; da er det fanen som venter på svaret.
    tab: txnTab && txnTab.txn === p.txn ? txnTab.tab : applyInFlight ? applyInFlight.tab : null,
  };
}

function daemonView() {
  const st = daemon.state;
  const h = daemon.hello;
  return {
    connected: daemon.ready,
    socket: DAEMON_SOCKET,
    retryAt: daemon.ready ? null : daemon.retryAt || null,
    info: h ? { name: h.daemon?.name, version: h.daemon?.version, mock: !!h.daemon?.mock, protocol: h.protocol, confirm: h.confirm } : null,
    pending: daemon.ready && st?.pending ? pendingFlat(st.pending, st) : null,
    emergency: daemon.ready ? !!st?.emergency : false,
    safety: daemon.limits?.safety || FALLBACK_SAFETY,
    recent: recent && Date.now() - recent.at < 60_000 ? recent : null,
  };
}

/** Grensene appen bruker (glidere og tallfelt): fra kortet via tjenesten, ellers appens egne. */
function limitsFlat() {
  const L = { ...LIMITS };
  const d = daemon.limits;
  if (!d) return L;
  const r = (x) => [x.min, x.max];
  Object.assign(L, {
    powerLimit: r(d.powerLimitW),
    coreOffset: r(d.clockOffsets.coreMhz),
    memOffset: r(d.clockOffsets.memMhz),
    tempTarget: r(d.tempTargetC),
    fanPercent: r(d.fan.percent),
    fanMinPercent: r(d.fan.minPercent),
    fanHysteresis: r(d.fan.spindownC),
    fanRampUp: r(d.fan.rampUpS),
    fanRampDown: r(d.fan.rampDownS),
    fanStartTemp: r(d.fan.startC),
    fanStopTemp: r(d.fan.stopC),
  });
  return L;
}

/** Hvor grensene kommer fra (verktøytips ved hver kontroll). */
function limitInfo() {
  const d = daemon.limits;
  if (!d) return { source: 'app' };
  const pick = (x) => ({ min: x.min, max: x.max, source: x.source, hardware: x.hardware || null, default: x.default ?? null });
  return {
    source: 'daemon',
    powerLimit: pick(d.powerLimitW),
    coreOffset: pick(d.clockOffsets.coreMhz),
    memOffset: pick(d.clockOffsets.memMhz),
    tempTarget: pick(d.tempTargetC),
    fanPercent: pick(d.fan.percent),
    fans: d.fan.count,
  };
}

function broadcastDaemon(extra = {}) {
  telemetry.broadcast('daemon', { ...daemonView(), applied: store.state.applied, limits: limitsFlat(), limitInfo: limitInfo(), ...extra });
}

daemon.on('ready', () => {
  if (daemon.state?.active) store.cacheApplied(toFlat(daemon.state.active));
  broadcastDaemon();
});
daemon.on('status', (st) => {
  if (!st.connected && txnTab) {
    // Serveren mistet tjenesten: den ruller selv tilbake det som ventet.
    recent = { txn: txnTab.txn, status: 'rolled_back', reason: 'client_disconnected', at: Date.now(), values: null };
    txnTab = null;
  }
  broadcastDaemon();
});
daemon.on('state', (st) => {
  if (st?.active) store.cacheApplied(toFlat(st.active));
  broadcastDaemon({ reason: st?.reason || null });
});
daemon.on('txn', (t) => {
  if (t.status === 'pending') return;
  const mine = txnTab && txnTab.txn === t.txn;
  const reason = mine && txnTab.reason ? txnTab.reason : t.reason;
  // Verdiene som ble rullet tilbake (tilstanden er ennå ikke oppdatert når txn kommer), så «Bruk igjen» virker.
  const values = t.status === 'rolled_back' && daemon.state?.active ? toFlat(daemon.state.active) : null;
  recent = { txn: t.txn, status: t.status, reason, at: Date.now(), values, mine: !!mine };
  if (mine) txnTab = null;
  if (t.status === 'rolled_back') log(`Endring ${t.txn} rullet tilbake (${reason})`);
});
daemon.on('safety', () => broadcastDaemon());
daemon.on('shutdown', (info) => {
  recent = { txn: null, status: 'shutdown', reason: info?.reason || null, at: Date.now(), fan: info?.fan || 'auto', values: null };
  broadcastDaemon({ shutdown: info });
});

const DAEMON_HTTP = {
  invalid_argument: 400,
  unknown_gpu: 400,
  busy: 409,
  no_pending: 409,
  txn_mismatch: 409,
  not_owner: 409,
  permission_denied: 403,
  not_supported: 501,
  not_implemented: 501,
  shutting_down: 503,
  daemon_unavailable: 503,
  daemon_timeout: 504,
};
function daemonFail(res, err) {
  if (!(err instanceof DaemonError)) throw err;
  const msg = err.code === 'daemon_unavailable' ? 'Bakgrunnstjenesten er ikke tilkoblet. Ingenting er endret.' : err.message;
  return json(res, DAEMON_HTTP[err.code] || 502, { error: msg, code: err.code, details: err.details ?? null });
}
const TAB_RX = /^[A-Za-z0-9_-]{1,64}$/;
/**
 * Fanen som trykket Bruk, er borte (pagehide eller lukket SSE-strøm): rull
 * tilbake det som venter, slik tjenesten gjør når en klient kobler fra.
 * Nettleseren holder av og til strømmen åpen etter at siden er forlatt
 * (tilbake/fram-hurtigbuffer), derfor sier siden også fra selv.
 */
function tabGone(tab) {
  if (!tab || !txnTab || txnTab.tab !== tab || daemon.state?.pending?.txn !== txnTab.txn) return false;
  txnTab.reason = 'tab_closed';
  daemon.request('revert', { gpu: '0', txn: txnTab.txn }).catch(() => {});
  return true;
}
const txnArg = (body) => {
  if (typeof body?.txn !== 'string' || !/^[A-Za-z0-9_-]{1,32}$/.test(body.txn)) throw new ValidationError('txn mangler');
  return body.txn;
};
const INDEX = join(WEB, 'index.html');
const escHtml = (v) => String(v).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

/** Skjermkortinfo til Grafikkort-siden: driveren, sysfs og spesifikasjonstabellen. */
function gpuInfo() {
  const live = telemetry.mode === 'live';
  const info = telemetry.info || {};
  const pci = live ? readPciGpu(info.busId) : null;
  const ids = {
    vendor: pci?.vendor || info.pciDevice?.vendor || null,
    device: pci?.device || info.pciDevice?.device || null,
    subVendor: pci?.subVendor || info.pciSub?.vendor || null,
    subDevice: pci?.subDevice || info.pciSub?.device || null,
    source: pci ? 'sysfs' : live ? 'nvidia-smi' : 'simulert',
  };
  return {
    mode: telemetry.mode,
    reason: telemetry.reason,
    simulated: !live,
    driver: info,
    query: telemetry.query || {},
    pci,
    ids,
    spec: lookupSpec(ids.vendor, ids.device),
    latest: telemetry.history[telemetry.history.length - 1] || null,
  };
}

function sensorsSnapshot() {
  const sys = sysmon.sampleOnce();
  return {
    gpu: { mode: telemetry.mode, sample: telemetry.history[telemetry.history.length - 1] || null },
    sys,
    notes: sensorNotes(sys.hwmon, telemetry.mode),
  };
}

function serveStatic(req, res, pathname) {
  let p = pathname === '/' ? '/index.html' : pathname;
  try {
    p = decodeURIComponent(p);
  } catch {
    return json(res, 400, { error: 'Ugyldig sti' });
  }
  const file = normalize(join(WEB, p));
  if (!file.startsWith(WEB + sep)) return json(res, 403, { error: 'Ikke tillatt' });
  let st;
  try {
    st = statSync(file);
  } catch {
    return json(res, 404, { error: 'Ikke funnet' });
  }
  if (!st.isFile()) return json(res, 404, { error: 'Ikke funnet' });
  if (file === INDEX) {
    // Appnavnet settes inn her, så HTML-en ikke har navnet hardkodet. Det
    // samme gjelder utseendet (tema, fargetema, aksent, kontrast, tetthet,
    // skrift, animasjoner), så første tegning alltid har riktige farger, også
    // når nettleseren ikke har noe lagret lokalt (ingen blink av feil tema).
    const body = Buffer.from(
      readFileSync(file, 'utf8')
        .replace('<html lang="nb" data-motion="full">', `<html lang="nb" data-motion="full" ${htmlPrefs(store.state.settings)}>`)
        .replaceAll('{{APP_NAME}}', escHtml(APP.name))
        .replaceAll('{{APP_LABEL}}', escHtml(APP.label)),
    );
    res.writeHead(200, { 'Content-Type': MIME['.html'], 'Content-Length': body.length, 'Cache-Control': 'no-cache', ...SECURITY_HEADERS });
    return res.end(req.method === 'HEAD' ? undefined : body);
  }
  res.writeHead(200, {
    'Content-Type': MIME[extname(file)] || 'application/octet-stream',
    'Content-Length': st.size,
    'Cache-Control': 'no-cache',
    ...SECURITY_HEADERS,
  });
  if (req.method === 'HEAD') return res.end();
  createReadStream(file).pipe(res);
}

function openStream(req, res, url) {
  const tab = TAB_RX.test(url.searchParams.get('tab') || '') ? url.searchParams.get('tab') : null;
  res.writeHead(200, {
    'Content-Type': 'text/event-stream; charset=utf-8',
    'Cache-Control': 'no-store',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
    ...SECURITY_HEADERS,
  });
  res.write('retry: 2000\n\n');
  telemetry.send(res, 'hello', telemetry.helloData());
  telemetry.send(res, 'daemon', { ...daemonView(), applied: store.state.applied, limits: limitsFlat(), limitInfo: limitInfo() });
  telemetry.addClient(res);
  if (tab) tabStreams.set(tab, (tabStreams.get(tab) || 0) + 1);
  const ping = setInterval(() => res.write(': ping\n\n'), 15_000);
  req.on('close', () => {
    clearInterval(ping);
    telemetry.removeClient(res);
    if (!tab) return;
    const n = (tabStreams.get(tab) || 1) - 1;
    if (n > 0) return void tabStreams.set(tab, n);
    tabStreams.delete(tab);
    // Fanen som gjorde endringen, er lukket eller lastet på nytt: rull tilbake
    // straks, slik tjenesten gjør når en klient kobler fra.
    tabGone(tab);
  });
}

async function handleApi(req, res, pathname, url) {
  const m = req.method;

  if (pathname === '/api/state' && m === 'GET') return json(res, 200, snapshot());
  if (pathname === '/api/stream' && m === 'GET') return openStream(req, res, url);
  if (pathname === '/api/system' && m === 'GET') return json(res, 200, { static: readStaticSystem(), live: sysmon.sampleOnce() });
  if (pathname === '/api/sensors' && m === 'GET') return json(res, 200, sensorsSnapshot());
  if (pathname === '/api/gpuinfo' && m === 'GET') return json(res, 200, gpuInfo());
  if (pathname === '/api/history' && m === 'GET') return json(res, 200, historyQuery(url));

  if (m !== 'GET' && m !== 'HEAD') {
    // Enkel CSRF-vern: avvis kryss-opphav fra andre nettsteder.
    const origin = req.headers.origin;
    if (origin && origin !== `http://127.0.0.1:${PORT}` && origin !== `http://localhost:${PORT}`) {
      return json(res, 403, { error: 'Fremmed opphav avvist' });
    }
    const ct = req.headers['content-type'] || '';
    if (m !== 'DELETE' && !ct.startsWith('application/json')) return json(res, 415, { error: 'Forventet application/json' });
  }

  if (pathname === '/api/apply' && m === 'POST') {
    const body = await readBody(req);
    if (!body || typeof body !== 'object' || Array.isArray(body)) throw new ValidationError('Forventet et JSON-objekt');
    const tab = typeof body.tab === 'string' && TAB_RX.test(body.tab) ? body.tab : null;
    const confirmTimeoutS = body.confirmSec !== undefined ? body.confirmSec : store.state.settings.confirmSec;
    // Én ventende endring om gangen (v0.0.8). Alle faner deler den, og bare
    // fanen som trykket Bruk, kan bruke mer før den er beholdt eller rullet
    // tilbake. Et Bruk som allerede er på vei, teller også, så to faner som
    // trykker samtidig, ikke havner i samme txn.
    const p = daemon.ready ? daemon.state?.pending : null;
    const owner = p && txnTab?.txn === p.txn ? txnTab.tab : null;
    if (applyInFlight || (p && owner !== tab)) {
      return json(res, 409, { error: 'En endring fra en annen fane venter på Behold. Behold eller rull tilbake den først.', code: 'pending_elsewhere', details: p ? { txn: p.txn } : null });
    }
    applyInFlight = { tab };
    try {
      // Tjenesten validerer, klemmer til kortets grenser og gir en txn som må bekreftes.
      const r = await daemon.request('apply', { gpu: '0', settings: toSettings(body), confirmTimeoutS });
      if (r.txn) txnTab = { txn: r.txn, tab };
      store.cacheApplied(toFlat(r.active));
      const clamped = r.clamped.map(clampFlat);
      log(`Simulert bruk${r.txn ? ` (${r.txn}, ${r.pending.timeoutS} s)` : ''}:`, r.changed.join(', ') || 'ingen endring', clamped.length ? `· klemt: ${clamped.map((c) => `${c.field} ${c.requested} → ${Array.isArray(c.applied) ? 'kurve' : c.applied}`).join(', ')}` : '');
      return json(res, 200, { simulated: true, note: SIM_NOTE, applied: store.state.applied, pending: pendingFlat(r.pending, r), clamped });
    } catch (err) {
      return daemonFail(res, err);
    } finally {
      applyInFlight = null;
    }
  }
  if (pathname === '/api/confirm' && m === 'POST') {
    const txn = txnArg(await readBody(req));
    try {
      const st = await daemon.request('confirm', { gpu: '0', txn });
      store.cacheApplied(toFlat(st.active));
      return json(res, 200, { simulated: true, applied: store.state.applied, pending: null });
    } catch (err) {
      return daemonFail(res, err);
    }
  }
  if (pathname === '/api/revert' && m === 'POST') {
    const txn = txnArg(await readBody(req));
    try {
      const st = await daemon.request('revert', { gpu: '0', txn });
      store.cacheApplied(toFlat(st.active));
      return json(res, 200, { simulated: true, applied: store.state.applied, pending: null });
    } catch (err) {
      return daemonFail(res, err);
    }
  }
  if (pathname === '/api/reset' && m === 'POST') {
    try {
      const st = await daemon.request('reset', { gpu: '0' });
      store.cacheApplied(toFlat(st.active));
      return json(res, 200, { simulated: true, note: SIM_NOTE, applied: store.state.applied });
    } catch (err) {
      return daemonFail(res, err);
    }
  }
  if (pathname === '/api/tab/close' && m === 'POST') {
    const body = await readBody(req);
    const tab = typeof body?.tab === 'string' && TAB_RX.test(body.tab) ? body.tab : null;
    return json(res, 200, { reverted: tabGone(tab) });
  }
  if (pathname === '/api/daemon' && m === 'GET') return json(res, 200, { daemon: daemonView(), limits: limitsFlat(), limitInfo: limitInfo() });
  if (pathname === '/api/daemon/retry' && m === 'POST') {
    daemon.retryNow();
    return json(res, 200, { daemon: daemonView() });
  }
  if (pathname === '/api/rgb' && m === 'POST') {
    const body = await readBody(req);
    const rgb = store.setRgb(body);
    return json(res, 200, { simulated: true, note: 'Simulert: lyskontrolleren er ikke koblet til ennå.', rgb });
  }
  if (pathname === '/api/settings' && m === 'POST') {
    const body = await readBody(req);
    const settings = store.setSettings(body);
    // «Husk min og maks» slått av: det som er lagret, glemmes også.
    if (!settings.sensorKeepMinMax) minmax.clear();
    return json(res, 200, { simulated: true, settings });
  }
  if (pathname === '/api/settings/reset' && m === 'POST') {
    return json(res, 200, { simulated: true, settings: store.resetSettings() });
  }
  if (pathname === '/api/settings/import' && m === 'POST') {
    const body = await readBody(req);
    if (!body || typeof body !== 'object' || typeof body.settings !== 'object' || body.settings === null || Array.isArray(body.settings)) {
      throw new ValidationError('Fila mangler «settings»');
    }
    const settings = store.replaceSettings(body.settings);
    if (!settings.sensorKeepMinMax) minmax.clear();
    return json(res, 200, { simulated: true, settings });
  }
  if (pathname === '/api/mode' && m === 'POST') {
    const body = await readBody(req);
    // body kan være null, et tall eller en liste (gyldig JSON); det skal gi 400, ikke 500.
    if (body?.mode !== 'live' && body?.mode !== 'sim') throw new ValidationError('mode må være live eller sim');
    const r = await telemetry.setMode(body.mode);
    return json(res, r.ok ? 200 : 409, { ...r, telemetry: telemetry.publicInfo() });
  }
  if (pathname === '/api/profiles' && m === 'GET') return json(res, 200, { profiles: store.state.profiles });

  if (pathname === '/api/minmax') {
    if (m === 'GET') return json(res, 200, { minmax: minmax.get() });
    if (m === 'POST') {
      const body = await readBody(req);
      if (!store.state.settings.sensorKeepMinMax) return json(res, 409, { error: 'Min og maks huskes ikke (slått av under Innstillinger)' });
      return json(res, 200, { minmax: minmax.put(body) });
    }
    if (m === 'DELETE') {
      minmax.clear();
      return json(res, 200, { minmax: minmax.get() });
    }
  }

  const pm = pathname.match(/^\/api\/profiles\/([1-9])$/);
  if (pm) {
    const slot = Number(pm[1]) - 1;
    if (slot >= SLOTS) return json(res, 404, { error: 'Ukjent profilplass' });
    if (m === 'GET') {
      const p = store.state.profiles[slot];
      return p ? json(res, 200, { profile: p }) : json(res, 404, { error: 'Tom profilplass' });
    }
    if (m === 'POST') {
      const body = await readBody(req);
      const profile = store.saveProfile(slot, body);
      return json(res, 200, { simulated: true, slot: slot + 1, profile, profiles: store.state.profiles });
    }
    if (m === 'PUT') {
      // Angre etter tømming: legger profilen tilbake med opprinnelig tidspunkt.
      const body = await readBody(req);
      const profile = store.restoreProfile(slot, body);
      return json(res, 200, { simulated: true, slot: slot + 1, profile, profiles: store.state.profiles });
    }
    if (m === 'PATCH') {
      const body = await readBody(req);
      const profile = store.renameProfile(slot, body?.name);
      return json(res, 200, { simulated: true, slot: slot + 1, profile, profiles: store.state.profiles });
    }
    if (m === 'DELETE') {
      store.deleteProfile(slot);
      return json(res, 200, { simulated: true, slot: slot + 1, profiles: store.state.profiles });
    }
  }
  return json(res, 404, { error: 'Ukjent API-kall' });
}

const server = createServer(async (req, res) => {
  if (!allowedHost(req.headers.host)) return json(res, 421, { error: 'Ukjent vert' });
  let url;
  try {
    url = new URL(req.url, `http://${req.headers.host}`);
  } catch {
    return json(res, 400, { error: 'Ugyldig URL' });
  }
  try {
    if (url.pathname.startsWith('/api/')) return await handleApi(req, res, url.pathname, url);
    if (req.method !== 'GET' && req.method !== 'HEAD') return json(res, 405, { error: 'Metode ikke tillatt' });
    return serveStatic(req, res, url.pathname);
  } catch (err) {
    if (err instanceof ValidationError) {
      if (err.status === 413) res.setHeader('Connection', 'close');
      return json(res, err.status || 400, { error: err.message });
    }
    log('Feil:', err);
    if (!res.headersSent) json(res, 500, { error: 'Intern feil' });
    else res.end();
  }
});

/**
 * GET /api/history?window=3600&keys=gpu.temp,gpu.power&points=800
 * window: ett av WINDOWS (s). keys: 1–16 seriernavn (KEY_RE), uten doble.
 * points: 10–4000 (standard 1000). Andre parametre avvises. Bare lesing.
 * GPU-seriene (gpu.*) hentes fra den kilden som er i bruk (live eller simulert).
 */
function historyQuery(url) {
  for (const k of url.searchParams.keys()) if (!['window', 'keys', 'points'].includes(k)) throw new ValidationError(`Ukjent parameter: ${k.slice(0, 20)}`);
  const w = url.searchParams.get('window') || '';
  const window = /^\d{1,6}$/.test(w) ? Number(w) : NaN;
  if (!HISTORY_WINDOWS.includes(window)) throw new ValidationError(`window må være en av ${HISTORY_WINDOWS.join(', ')}`);
  const raw = url.searchParams.get('keys') || '';
  const keys = raw.split(',');
  if (!raw || keys.length > MAX_KEYS || !keys.every((k) => KEY_RE.test(k))) throw new ValidationError(`keys må være 1–${MAX_KEYS} seriernavn`);
  if (new Set(keys).size !== keys.length) throw new ValidationError('keys har doble navn');
  const p = url.searchParams.get('points');
  let points = 1000;
  if (p !== null) {
    points = /^\d{1,5}$/.test(p) ? Number(p) : NaN;
    if (!(points >= MIN_POINTS && points <= MAX_POINTS)) throw new ValidationError(`points må være ${MIN_POINTS}–${MAX_POINTS}`);
  }
  const sim = telemetry.mode !== 'live';
  return history.query({ window, keys, points, map: (k) => (sim && k.startsWith('gpu.') ? `gsim.${k.slice(4)}` : k) });
}

let shuttingDown = false;
function shutdown(sig) {
  if (shuttingDown) return;
  shuttingDown = true;
  log(`Stopper (${sig})`);
  minmax.flush();
  history.stop();
  history.save();
  daemon.close();
  telemetry.close();
  server.close(() => process.exit(0));
  server.closeAllConnections?.();
  setTimeout(() => process.exit(0), 2000).unref();
}
process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGHUP', () => shutdown('SIGHUP'));
process.on('exit', () => telemetry.close());

await telemetry.init();
daemon.start();
server.listen(PORT, HOST, () => {
  log(`${APP.name} ${APP.label}`);
  log(`Kilde: ${telemetry.mode === 'live' ? 'Live (nvidia-smi)' : 'Simulert'} – ${telemetry.reason}`);
  log(`Data: ${DATA_DIR}`);
  log(`Bakgrunnstjeneste: ${DAEMON_SOCKET}`);
  log(`Åpne http://127.0.0.1:${PORT}/`);
});
server.on('error', (err) => {
  console.error(`Kunne ikke starte serveren: ${err.message}`);
  telemetry.close();
  process.exit(1);
});
