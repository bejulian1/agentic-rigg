// HTTP-kall og SSE-strømmen. Holder siste prøve og en historikk i nettleseren.

async function call(method, path, body, { timeoutMs = 10_000 } = {}) {
  let res;
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    res = await fetch(path, {
      method,
      headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      signal: ctl.signal,
    });
  } catch (err) {
    // Nettleserens egne meldinger («Failed to fetch») er på engelsk og sier lite.
    throw new Error(err?.name === 'AbortError' ? 'Serveren svarte ikke i tide.' : 'Ingen kontakt med serveren.');
  } finally {
    clearTimeout(timer);
  }
  let data = null;
  try {
    data = await res.json();
  } catch {}
  if (!res.ok) throw new Error((data && data.error) || `HTTP ${res.status}`);
  return data;
}

/**
 * ID for denne fanen. Sendes med SSE-strømmen og med Bruk, så serveren kan
 * rulle tilbake en ubekreftet endring når fanen lukkes eller lastes på nytt.
 */
export const TAB = (() => {
  try {
    return crypto.randomUUID();
  } catch {
    return `t${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
  }
})();

export const api = {
  state: () => call('GET', '/api/state'),
  gpuinfo: () => call('GET', '/api/gpuinfo'),
  system: () => call('GET', '/api/system'),
  sensors: () => call('GET', '/api/sensors'),
  apply: (patch) => call('POST', '/api/apply', { ...patch, tab: TAB }),
  confirm: (txn) => call('POST', '/api/confirm', { txn }),
  revert: (txn) => call('POST', '/api/revert', { txn }),
  retryDaemon: () => call('POST', '/api/daemon/retry', {}),
  reset: () => call('POST', '/api/reset', {}),
  rgb: (patch) => call('POST', '/api/rgb', patch),
  settings: (patch) => call('POST', '/api/settings', patch),
  resetSettings: () => call('POST', '/api/settings/reset', {}),
  importSettings: (settings) => call('POST', '/api/settings/import', { settings }),
  mode: (mode) => call('POST', '/api/mode', { mode }),
  saveProfile: (slot, name, values) => call('POST', `/api/profiles/${slot}`, { name, values }),
  restoreProfile: (slot, profile) => call('PUT', `/api/profiles/${slot}`, profile),
  renameProfile: (slot, name) => call('PATCH', `/api/profiles/${slot}`, { name }),
  loadProfile: (slot) => call('GET', `/api/profiles/${slot}`),
  deleteProfile: (slot) => call('DELETE', `/api/profiles/${slot}`),
  minmax: () => call('GET', '/api/minmax'),
  putMinmax: (body) => call('POST', '/api/minmax', body),
  /** Historikk for grafene (v0.0.11): { window (s), keys: [...], points }. */
  history: ({ window, keys, points }) => call('GET', `/api/history?window=${window}&keys=${encodeURIComponent(keys.join(','))}&points=${Math.max(10, Math.min(4000, Math.round(points)))}`),
};

/** Siden forlates mens en endring venter: serveren ruller tilbake (keepalive overlever at siden går bort). */
export function beaconTabClosed() {
  try {
    fetch('/api/tab/close', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ tab: TAB }), keepalive: true }).catch(() => {});
  } catch {}
}

/** Sender min/maks når fanen lukkes (keepalive, så kallet overlever at siden går bort). */
export function beaconMinmax(body) {
  try {
    fetch('/api/minmax', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), keepalive: true }).catch(() => {});
  } catch {}
}

// Historikk i nettleseren: nok til det lengste tidsvinduet (15 min) pluss litt.
const KEEP_MS = 16 * 60_000;
const RETRY_S = 3;
// Åpen strøm uten «hello» etter så lang tid regnes som feil og kobles på nytt.
const HELLO_TIMEOUT_MS = 10_000;

/**
 * Telemetri i nettleseren: SSE, historikk og lyttere.
 * Hendelser: hello (kilde), sample (GPU), sys (CPU, minne, nett, hwmon),
 * status ({ connected, retryIn }), daemon (bakgrunnstjenesten).
 */
export class LiveFeed {
  constructor() {
    this.history = []; // { t, ...prøve } med t i ms (Date.now)
    this.latest = null;
    this.info = null;
    this.intervalMs = 500;
    this.lastAccepted = 0;
    this.listeners = { sample: new Set(), hello: new Set(), status: new Set(), sys: new Set(), daemon: new Set() };
    this.sysHistory = [];
    this.sysLatest = null;
    this.connected = false;
    this.retryTimer = null;
    this.retryAt = 0;
  }

  on(ev, fn) {
    this.listeners[ev].add(fn);
    return () => this.listeners[ev].delete(fn);
  }

  emit(ev, data) {
    for (const fn of this.listeners[ev]) fn(data);
  }

  setInterval(ms) {
    this.intervalMs = ms;
  }

  /**
   * Lukker strømmen når siden forlates. Ellers kan nettleseren holde den åpen
   * i tilbake/fram-hurtigbufferen, og hver slik side tar én av de seks
   * forbindelsene nettleseren har per server.
   */
  disconnect() {
    clearInterval(this.retryTimer);
    this.retryTimer = null;
    clearTimeout(this.helloTimer);
    const es = this.es;
    this.es = null;
    es?.close();
  }

  connect() {
    clearInterval(this.retryTimer);
    this.retryTimer = null;
    this.es?.close();
    clearTimeout(this.helloTimer);
    const es = new EventSource(`/api/stream?tab=${encodeURIComponent(TAB)}`);
    this.es = es;
    const fail = () => {
      // Vi styrer gjenoppkoblingen selv, så UI-et kan vise nedtelling og «Prøv nå».
      es.close();
      clearTimeout(this.helloTimer);
      if (this.es !== es || this.retryTimer) return;
      this.connected = false;
      this.retryAt = Date.now() + RETRY_S * 1000;
      const tick = () => {
        const left = Math.max(0, Math.ceil((this.retryAt - Date.now()) / 1000));
        this.emit('status', { connected: false, retryIn: left });
        if (left <= 0) this.connect();
      };
      tick();
      if (!this.retryTimer && this.es === es) this.retryTimer = setInterval(tick, 1000);
    };
    es.addEventListener('error', fail);
    // Strømmen regnes som tilkoblet først når «hello» er kommet.
    this.helloTimer = setTimeout(fail, HELLO_TIMEOUT_MS);
    es.addEventListener('hello', (e) => {
      clearTimeout(this.helloTimer);
      if (!this.connected) {
        this.connected = true;
        this.emit('status', { connected: true });
      }
      const d = JSON.parse(e.data);
      const sameSource = this.info && this.info.mode === d.mode;
      this.info = d;
      const server = (d.history || []).slice();
      // Ved ny tilkobling til samme kilde beholdes den lengre lokale historikken,
      // også når serveren nettopp har startet og ikke har noe ennå. Hullet
      // mellom blir et brudd i grafene (GAP_MS), ikke en rett strek.
      if (sameSource && this.history.length) {
        const first = server.length ? server[0].t : Infinity;
        this.history = this.history.filter((s) => s.t < first).concat(server);
      } else this.history = server;
      this.latest = this.history[this.history.length - 1] || null;
      this.lastAccepted = 0;
      if (Array.isArray(d.sysHistory) && d.sysHistory.length) {
        const first = d.sysHistory[0].t;
        this.sysHistory = this.sysHistory.filter((s) => s.t < first).concat(d.sysHistory);
        this.sysLatest = this.sysHistory[this.sysHistory.length - 1];
      }
      this.emit('hello', d);
      if (this.latest) this.emit('sample', this.latest);
      if (this.sysLatest) this.emit('sys', this.sysLatest);
    });
    // Bakgrunnstjenesten: tilkoblet, ventende endring, grenser og nødregel.
    es.addEventListener('daemon', (e) => this.emit('daemon', JSON.parse(e.data)));
    es.addEventListener('sys', (e) => {
      const s = JSON.parse(e.data);
      this.sysLatest = s;
      this.sysHistory.push(s);
      trim(this.sysHistory, s.t);
      this.emit('sys', s);
    });
    es.addEventListener('sample', (e) => {
      const s = JSON.parse(e.data);
      // Mottakstid, så grafene ruller jevnt med nettleserens klokke.
      s.t = Date.now();
      if (this.lastAccepted && s.t - this.lastAccepted < this.intervalMs - 120) return;
      this.lastAccepted = s.t;
      this.latest = s;
      this.history.push(s);
      trim(this.history, s.t);
      this.emit('sample', s);
    });
  }
}

function trim(arr, now) {
  const cut = now - KEEP_MS;
  let i = 0;
  while (i < arr.length && arr[i].t < cut) i++;
  if (i > 0) arr.splice(0, i);
}
