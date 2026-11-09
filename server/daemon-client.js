// Klient for bakgrunnstjenesten over Unix-socket (node:net). Brukes av
// serveren for alle skrivende handlinger. Kobler til på nytt av seg selv,
// med økende ventetid, og sier fra når forbindelsen kommer og går.
//
// Hendelser (EventEmitter):
//   status     { connected, retryAt, error }   tilkoblet/frakoblet
//   ready      etter hello, subscribe, get_limits og get_state
//   telemetry  prøve fra den simulerte modellen
//   state      { reason, active, confirmed, pending, emergency }
//   txn        { txn, status: pending | confirmed | rolled_back, reason }
//   safety     { emergency, tempC, thresholdC, releaseC }
//   shutdown   { reason, fan, rolledBack }  tjenesten stopper
//
// Starter ingen programmer: mocken startes av scripts/dev.js, ikke herfra.

import { EventEmitter } from 'node:events';
import { createConnection } from 'node:net';
import { MAX_LINE_BYTES, PROTOCOL } from './daemon-protocol.js';

const RETRY_MS = [500, 1000, 2000, 3000, 5000];

export class DaemonError extends Error {
  constructor(code, message, details) {
    super(message);
    this.code = code;
    this.details = details;
  }
}

export class DaemonClient extends EventEmitter {
  constructor({ path, client = 'agentic-rigg', log = () => {}, timeoutMs = 5000 } = {}) {
    super();
    this.path = path;
    this.clientName = client;
    this.log = log;
    this.timeoutMs = timeoutMs;
    this.sock = null;
    this.ready = false;
    this.connected = false;
    this.hello = null;
    this.limits = null;
    this.state = null;
    this.pending = new Map();
    this.seq = 0;
    this.attempt = 0;
    this.retryTimer = null;
    this.retryAt = 0;
    this.lastError = null;
    this.closed = false;
  }

  start() {
    this.connect();
    return this;
  }

  connect() {
    if (this.closed) return;
    clearTimeout(this.retryTimer);
    this.retryTimer = null;
    const sock = createConnection(this.path);
    this.sock = sock;
    let buf = [];
    let bufLen = 0;
    sock.on('connect', () => this.onConnect(sock));
    sock.on('data', (chunk) => {
      let start = 0;
      for (;;) {
        const nl = chunk.indexOf(10, start);
        if (nl === -1) break;
        const part = chunk.subarray(start, nl);
        start = nl + 1;
        const line = bufLen ? Buffer.concat([...buf, part]).toString('utf8') : part.toString('utf8');
        buf = [];
        bufLen = 0;
        if (line.trim()) this.onLine(line);
      }
      const rest = chunk.subarray(start);
      if (rest.length) {
        buf.push(rest);
        bufLen += rest.length;
        // Et svar over grensen er en feil i tjenesten; bryt og koble til på nytt.
        if (bufLen > MAX_LINE_BYTES * 4) sock.destroy(new Error('For stor melding fra tjenesten'));
      }
    });
    sock.on('error', (err) => (this.lastError = err));
    sock.on('close', () => this.onClose(sock));
  }

  async onConnect(sock) {
    try {
      const hello = await this.request('hello', { protocol: [PROTOCOL], client: this.clientName }, { force: true });
      if (this.sock !== sock) return;
      this.hello = hello;
      this.connected = true;
      this.attempt = 0;
      await this.request('subscribe', { topics: ['telemetry', 'state', 'txn', 'safety'] }, { force: true });
      this.limits = await this.request('get_limits', { gpu: '0' }, { force: true });
      this.state = await this.request('get_state', { gpu: '0' }, { force: true });
      if (this.sock !== sock) return;
      this.ready = true;
      this.lastError = null;
      this.log(`Bakgrunnstjeneste tilkoblet (${hello.daemon?.name} ${hello.daemon?.version}, protokoll ${hello.protocol})`);
      this.emit('status', { connected: true });
      this.emit('ready', hello);
    } catch (err) {
      this.lastError = err;
      this.log(`Bakgrunnstjenesten svarte ikke riktig: ${err.message}`);
      sock.destroy();
    }
  }

  onLine(line) {
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      this.log('Ugyldig JSON fra tjenesten');
      return;
    }
    if (msg && typeof msg.event === 'string') {
      if (msg.event === 'state') this.state = { ...msg.data };
      this.emit(msg.event, msg.data);
      return;
    }
    const p = this.pending.get(msg?.id);
    if (!p) return;
    this.pending.delete(msg.id);
    clearTimeout(p.timer);
    if (msg.ok) p.resolve(msg.result);
    else p.reject(new DaemonError(msg.error?.code || 'internal', msg.error?.message || 'Ukjent feil fra tjenesten', msg.error?.details));
  }

  onClose(sock) {
    if (this.sock !== sock) return;
    const was = this.ready;
    this.sock = null;
    this.ready = false;
    this.connected = false;
    for (const p of this.pending.values()) {
      clearTimeout(p.timer);
      p.reject(new DaemonError('daemon_unavailable', 'Forbindelsen til bakgrunnstjenesten ble brutt'));
    }
    this.pending.clear();
    if (this.closed) return;
    const wait = RETRY_MS[Math.min(this.attempt, RETRY_MS.length - 1)];
    this.attempt++;
    this.retryAt = Date.now() + wait;
    if (was) this.log('Bakgrunnstjenesten er frakoblet. Prøver igjen.');
    this.emit('status', { connected: false, retryAt: this.retryAt, error: this.lastError?.code || this.lastError?.message || null });
    this.retryTimer = setTimeout(() => this.connect(), wait);
    this.retryTimer.unref?.();
  }

  /** Prøv å koble til nå (knappen «Prøv nå»). */
  retryNow() {
    if (this.ready || this.closed) return;
    if (this.sock) return;
    this.connect();
  }

  /** Sender en forespørsel. Avvises med DaemonError (code fra kontrakten, eller daemon_unavailable). */
  request(op, args = {}, { force = false, timeoutMs = this.timeoutMs } = {}) {
    const sock = this.sock;
    if (!sock || (!force && !this.ready)) return Promise.reject(new DaemonError('daemon_unavailable', 'Bakgrunnstjenesten er ikke tilkoblet'));
    const id = ++this.seq;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new DaemonError('daemon_timeout', 'Bakgrunnstjenesten svarte ikke i tide'));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, { resolve, reject, timer });
      sock.write(`${JSON.stringify({ id, op, args })}\n`);
    });
  }

  close() {
    this.closed = true;
    clearTimeout(this.retryTimer);
    this.sock?.destroy();
  }
}
