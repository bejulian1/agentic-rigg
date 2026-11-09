// Telemetri: velger kilde (live fra nvidia-smi eller simulert), holder en
// ringbuffer med historikk og sender prøver til nettleserne over SSE.
// Kilden kjører bare mens minst én nettleser lytter (pluss en kort grace).
//
// Simulert kilde (v0.0.7): når bakgrunnstjenesten er tilkoblet, kommer
// prøvene fra den (der viftekurven og nødregelen kjører). Ellers brukes den
// lokale modellen med siste kjente verdier, så grafene ikke stopper.

import { POLL_MS, readQuery, readStatic, startLoop } from './nvidia.js';
import { SIM_QUERY, SIM_STATIC, Simulator } from './simulator.js';

const HISTORY = 260; // ca. 130 s ved 500 ms
const IDLE_STOP_MS = 10_000;
const STALL_MS = 5_000;

export class Telemetry {
  constructor({ store, forceSim, sysmon = null, log = () => {}, daemon = null, onSample = null }) {
    this.store = store;
    // v0.0.11: hver GPU-prøve går også til historikken (server/history.js).
    this.onSample = onSample;
    this.sysmon = sysmon;
    this.query = { ...SIM_QUERY };
    this.forceSim = forceSim;
    this.log = log;
    this.mode = 'sim';
    this.reason = 'starter';
    this.info = { ...SIM_STATIC };
    this.history = [];
    this.clients = new Set();
    this.sim = new Simulator(() => this.store.state.applied);
    this.loop = null;
    this.simTimer = null;
    this.idleTimer = null;
    this.watchdog = null;
    this.lastSampleAt = 0;
    this.failures = [];
    this.running = false;
    this.closed = false;
    this.daemon = daemon;
    this.lastDaemonAt = 0;
    daemon?.on('telemetry', (d) => this.onDaemonSample(d?.sample));
  }

  /** Prøve fra bakgrunnstjenesten. Den lokale modellen følger med, så den kan ta over uten hopp. */
  onDaemonSample(s) {
    if (!s) return;
    this.lastDaemonAt = Date.now();
    const m = this.sim;
    m.temp = s.temp;
    m.fan = s.fan;
    m.power = s.power;
    m.core = s.coreClock;
    m.mem = s.memClock;
    m.load = s.gpuUtil;
    if (this.mode === 'sim' && this.running) this.push({ ...s, t: Date.now() });
  }

  /** true når bakgrunnstjenesten har sendt en prøve det siste halvannet sekundet. */
  daemonFresh() {
    return !!this.daemon?.ready && Date.now() - this.lastDaemonAt < 1500;
  }

  async init() {
    if (this.forceSim) return this.useSim('Valgt ved oppstart av serveren');
    if (this.store.state.settings.startSimulated) return this.useSim('Valgt i innstillinger (start i simulert modus)');
    const ok = await this.tryLive();
    if (!ok) this.useSim(this.reason);
  }

  async tryLive() {
    try {
      const info = await readStatic();
      this.info = info;
      // -q er ikke kritisk. Mangler den, viser UI-et «ikke oppgitt».
      this.query = (await readQuery()) || {};
      this.mode = 'live';
      this.reason = 'nvidia-smi';
      this.failures = [];
      return true;
    } catch (err) {
      this.reason = err && err.code === 'ENOENT' ? 'nvidia-smi finnes ikke' : `nvidia-smi feilet: ${shortErr(err)}`;
      return false;
    }
  }

  useSim(reason) {
    this.mode = 'sim';
    this.reason = reason;
    this.info = { ...SIM_STATIC };
    this.query = { ...SIM_QUERY };
  }

  /** Bytter kilde mens serveren kjører. Returnerer { ok, error? }. */
  async setMode(mode) {
    if (mode === this.mode) return { ok: true };
    const wasRunning = this.running;
    this.stopSource();
    if (mode === 'live') {
      if (this.forceSim) {
        if (wasRunning) this.startSource();
        return { ok: false, error: 'Serveren er startet med AG_SIMULATE=1, så live er slått av.' };
      }
      const ok = await this.tryLive();
      if (!ok) {
        const reason = this.reason;
        this.useSim(reason);
        if (wasRunning) this.startSource();
        this.broadcastHello();
        return { ok: false, error: reason };
      }
    } else {
      this.useSim('Byttet manuelt');
    }
    this.history = [];
    if (this.clients.size) this.startSource();
    this.broadcastHello();
    return { ok: true };
  }

  publicInfo() {
    return { mode: this.mode, reason: this.reason, gpu: this.info, query: this.query, pollMs: POLL_MS };
  }

  // ---- klienter ----

  addClient(res) {
    this.clients.add(res);
    clearTimeout(this.idleTimer);
    this.idleTimer = null;
    if (!this.running) this.startSource();
    if (this.sysmon && !this.sysmon.running) this.sysmon.start();
  }

  removeClient(res) {
    this.clients.delete(res);
    if (this.clients.size === 0 && !this.idleTimer) {
      this.idleTimer = setTimeout(() => {
        this.idleTimer = null;
        if (this.clients.size === 0) {
          this.stopSource();
          this.sysmon?.stop();
        }
      }, IDLE_STOP_MS);
      this.idleTimer.unref();
    }
  }

  send(res, event, data) {
    res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  }

  broadcast(event, data) {
    const msg = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
    for (const res of this.clients) res.write(msg);
  }

  helloData() {
    return { ...this.publicInfo(), history: this.history, sysHistory: this.sysmon ? this.sysmon.history : [] };
  }

  broadcastHello() {
    for (const res of this.clients) this.send(res, 'hello', this.helloData());
  }

  // ---- kilde ----

  push(sample) {
    sample.mode = this.mode;
    this.lastSampleAt = Date.now();
    this.history.push(sample);
    if (this.history.length > HISTORY) this.history.splice(0, this.history.length - HISTORY);
    this.onSample?.(sample, this.mode);
    this.broadcast('sample', sample);
  }

  startSource() {
    if (this.closed) return;
    this.running = true;
    this.lastSampleAt = Date.now();
    if (this.mode === 'sim') {
      const step = () => {
        if (!this.daemonFresh()) this.push(this.sim.step(POLL_MS / 1000));
      };
      step();
      this.simTimer = setInterval(step, POLL_MS);
      return;
    }
    this.loop = startLoop(
      (s) => this.push(s),
      (code, signal, stderr) => this.onLoopExit(code, signal, stderr),
    );
    this.log(`nvidia-smi startet (pid ${this.loop.pid})`);
    this.watchdog = setInterval(() => {
      if (Date.now() - this.lastSampleAt > STALL_MS) {
        this.log('nvidia-smi svarer ikke, starter på nytt');
        this.restartLive('ingen data på 5 s');
      }
    }, 1000);
  }

  stopSource() {
    this.running = false;
    clearInterval(this.simTimer);
    this.simTimer = null;
    clearInterval(this.watchdog);
    this.watchdog = null;
    if (this.loop) {
      this.loop.stop();
      this.log(`nvidia-smi stoppet (pid ${this.loop.pid})`);
      this.loop = null;
    }
  }

  onLoopExit(code, signal, stderr) {
    this.log(`nvidia-smi avsluttet uventet (kode ${code}, signal ${signal}) ${stderr || ''}`);
    this.loop = null;
    this.restartLive(stderr || `kode ${code}`);
  }

  restartLive(why) {
    this.stopSource();
    const now = Date.now();
    this.failures = this.failures.filter((t) => now - t < 30_000);
    this.failures.push(now);
    if (this.failures.length >= 3) {
      this.useSim(`Falt tilbake til simulert: nvidia-smi feilet gjentatte ganger (${String(why).slice(0, 120)})`);
      this.history = [];
      this.broadcastHello();
    }
    if (this.clients.size && !this.closed) {
      setTimeout(() => {
        if (!this.running && this.clients.size && !this.closed) this.startSource();
      }, 800).unref();
    }
  }

  close() {
    this.closed = true;
    clearTimeout(this.idleTimer);
    this.stopSource();
    this.sysmon?.stop();
    for (const res of this.clients) {
      try {
        res.end();
      } catch {}
    }
    this.clients.clear();
  }
}

function shortErr(err) {
  const s = String((err && (err.stderr || err.message)) || err || 'ukjent feil');
  return s.split('\n')[0].slice(0, 160);
}
