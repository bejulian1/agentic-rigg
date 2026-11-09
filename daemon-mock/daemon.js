// Unix-socket-serveren til daemon-mock: linjeavgrenset JSON (NDJSON), én
// forespørsel per linje, svar med samme id, og hendelser uten id til klienter
// som abonnerer. Kontrakten står i docs/daemon-api.md.
//
// Tilgang: socketen ligger i en privat mappe (0700) og har selv 0600, så bare
// brukeren som startet mocken, kan koble til. Den ekte daemonen kjører som
// root med socket 0660 og gruppe, sjekker SO_PEERCRED og spør polkit før
// skriving (beskrevet i kontrakten, ikke laget her).

import { createConnection, createServer } from 'node:net';
import { chmodSync, lstatSync, mkdirSync, rmSync, statSync } from 'node:fs';
import { dirname } from 'node:path';
import { ERR, MAX_CLIENTS, MAX_LINE_BYTES, PROTOCOL } from '../server/daemon-protocol.js';
import { DaemonError, GPU_ID, MockCard, SIM_STATIC, limitsView } from './card.js';

const TOPICS = ['telemetry', 'state', 'txn', 'safety'];
/** Klienter som henger etter, får ikke telemetri før køen er tømt (ingen ubegrenset buffer). */
const SLOW_CLIENT_BYTES = 1024 * 1024;

const isObj = (v) => v != null && typeof v === 'object' && !Array.isArray(v);

/** Lager den private mappa (0700) og sjekker at den er vår og ikke en symlenke. */
export function prepareSocketDir(path) {
  // sockaddr_un har plass til 108 byte inkludert avsluttende null.
  if (Buffer.byteLength(path) > 107) throw new Error(`Stien til socketen er for lang (${Buffer.byteLength(path)} byte, maks 107): ${path}`);
  const dir = dirname(path);
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const st = lstatSync(dir);
  if (!st.isDirectory()) throw new Error(`${dir} er ikke en mappe`);
  if (typeof process.getuid === 'function' && st.uid !== process.getuid()) throw new Error(`${dir} eies av en annen bruker`);
  if ((st.mode & 0o777) !== 0o700) chmodSync(dir, 0o700);
}

/** Fjerner en gammel socket hvis ingen svarer på den. Kaster hvis en daemon allerede kjører. */
async function clearStale(path) {
  let st;
  try {
    st = lstatSync(path);
  } catch {
    return;
  }
  if (!st.isSocket()) throw new Error(`${path} finnes og er ikke en socket`);
  const alive = await new Promise((res) => {
    const c = createConnection(path);
    c.once('connect', () => (c.destroy(), res(true)));
    c.once('error', () => res(false));
  });
  if (alive) throw new Error(`En bakgrunnstjeneste svarer allerede på ${path}`);
  rmSync(path, { force: true });
}

/**
 * Starter mocken. Returnerer { card, server, path, close(reason) }.
 * opts: socketPath, dataDir, confirm { defaultS, minS, maxS }, testOps, tickMs, log, version.
 */
export async function startDaemon({ socketPath, dataDir, confirm, testOps = false, tickMs = 500, log = () => {}, version = '0.0.0' }) {
  prepareSocketDir(socketPath);
  await clearStale(socketPath);
  const card = new MockCard({ dataDir, confirm, log });
  const sessions = new Set();
  let seq = 0;

  const send = (sess, msg) => {
    if (sess.closed) return;
    sess.sock.write(`${JSON.stringify(msg)}\n`);
  };
  const reply = (sess, id, result) => send(sess, { id, ok: true, result });
  const fail = (sess, id, code, message, details) => send(sess, { id, ok: false, error: { code, message, ...(details ? { details } : {}) } });

  const broadcast = (topic, data) => {
    for (const s of sessions) {
      if (!s.hello || s.closed) continue;
      if (topic !== 'shutdown' && !s.topics.has(topic)) continue;
      if (topic === 'telemetry' && s.sock.writableLength > SLOW_CLIENT_BYTES) continue;
      send(s, { event: topic, data });
    }
  };
  card.on('telemetry', (s) => broadcast('telemetry', { gpu: GPU_ID, sample: s }));
  card.on('state', ({ reason }) => broadcast('state', { reason, ...card.stateView() }));
  card.on('txn', (t) => broadcast('txn', t));
  card.on('safety', (t) => broadcast('safety', t));

  const helloResult = (sess) => ({
    protocol: PROTOCOL,
    session: sess.id,
    daemon: { name: 'agentic-rigg-daemon-mock', version, mock: true, pid: process.pid },
    capabilities: {
      gpus: [GPU_ID],
      ops: OPS_LIST.filter((o) => testOps || !o.startsWith('mock.')),
      events: ['telemetry', 'state', 'txn', 'safety', 'shutdown'],
      rgb: { status: 'planned', note: 'Krever brukerens godkjenning (I2C, se research kap. 4.5).' },
    },
    confirm: { ...card.confirmCfg },
    maxMessageBytes: MAX_LINE_BYTES,
    lastExit: card.lastExit,
  });

  const gpuArg = (args) => {
    const g = args?.gpu;
    if (g === undefined) throw new DaemonError(ERR.INVALID_ARGUMENT, 'gpu mangler', { field: 'gpu' });
    if (String(g) !== GPU_ID) throw new DaemonError(ERR.UNKNOWN_GPU, `Ukjent skjermkort: ${String(g).slice(0, 40)}`);
  };
  const writeResult = (r) => ({ txn: r.txn, pending: r.pending, clamped: r.clamped, changed: r.changed, active: r.active, confirmed: r.confirmed });

  const OPS = {
    hello: (sess, args) => {
      const want = Array.isArray(args?.protocol) ? args.protocol : args?.protocol !== undefined ? [args.protocol] : null;
      if (!want || !want.includes(PROTOCOL)) throw new DaemonError(ERR.UNSUPPORTED_PROTOCOL, `Protokollen støttes ikke. Daemonen snakker versjon ${PROTOCOL}.`, { supported: [PROTOCOL] });
      sess.hello = true;
      sess.client = typeof args.client === 'string' ? args.client.slice(0, 80) : null;
      return helloResult(sess);
    },
    ping: () => ({ t: Date.now() }),
    list_gpus: () => [{ id: GPU_ID, name: SIM_STATIC.name, uuid: SIM_STATIC.uuid, pci: { ...SIM_STATIC.pciDevice }, simulated: true }],
    get_limits: (s, a) => (gpuArg(a), limitsView()),
    get_state: (s, a) => (gpuArg(a), card.stateView()),
    get_telemetry: (s, a) => (gpuArg(a), { gpu: GPU_ID, sample: card.latest }),
    subscribe: (sess, a) => {
      const t = a?.topics;
      if (!Array.isArray(t) || t.some((x) => !TOPICS.includes(x))) throw new DaemonError(ERR.INVALID_ARGUMENT, `topics må være en liste med ${TOPICS.join(', ')}`, { field: 'topics' });
      for (const x of t) sess.topics.add(x);
      return { topics: [...sess.topics] };
    },
    unsubscribe: (sess, a) => {
      for (const x of Array.isArray(a?.topics) ? a.topics : []) sess.topics.delete(x);
      return { topics: [...sess.topics] };
    },
    apply: (sess, a) => (gpuArg(a), writeResult(card.write(sess.id, a.settings, { confirmTimeoutS: a.confirmTimeoutS }))),
    set_power_limit: (sess, a) => (gpuArg(a), writeResult(card.write(sess.id, { powerLimitW: a.watts }, a))),
    set_clock_offsets: (sess, a) => {
      gpuArg(a);
      const co = {};
      if (a.coreMhz !== undefined) co.coreMhz = a.coreMhz;
      if (a.memMhz !== undefined) co.memMhz = a.memMhz;
      return writeResult(card.write(sess.id, { clockOffsets: co }, a));
    },
    set_temp_target: (sess, a) => (gpuArg(a), writeResult(card.write(sess.id, { tempTargetC: a.celsius }, a))),
    set_fan: (sess, a) => {
      gpuArg(a);
      if (!isObj(a.fan)) throw new DaemonError(ERR.INVALID_ARGUMENT, 'fan må være et objekt', { field: 'fan' });
      return writeResult(card.write(sess.id, { fan: a.fan }, a));
    },
    confirm: (sess, a) => (gpuArg(a), card.confirm(sess.id, a.txn)),
    revert: (sess, a) => (gpuArg(a), card.revert(sess.id, a.txn)),
    reset: (sess, a) => (gpuArg(a), card.reset(sess.id)),
    rgb_get: () => {
      throw new DaemonError(ERR.NOT_IMPLEMENTED, 'RGB er planlagt og krever brukerens godkjenning før noe leses fra eller skrives til I2C.', { status: 'planned' });
    },
    rgb_set: () => {
      throw new DaemonError(ERR.NOT_IMPLEMENTED, 'RGB er planlagt og krever brukerens godkjenning før noe leses fra eller skrives til I2C.', { status: 'planned' });
    },
    // Bare med testOps (AG_MOCK_TEST_OPS=1). Ikke en del av den ekte kontrakten.
    'mock.set_temperature': (sess, a) => (gpuArg(a), card.holdTemperature(a.celsius ?? null)),
  };
  const OPS_LIST = Object.keys(OPS);

  function handleLine(sess, line) {
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      return fail(sess, null, ERR.BAD_JSON, 'Linja er ikke gyldig JSON');
    }
    if (!isObj(msg)) return fail(sess, null, ERR.BAD_REQUEST, 'Forventet et JSON-objekt');
    const { id, op, args } = msg;
    const idOk = (typeof id === 'number' && Number.isSafeInteger(id)) || (typeof id === 'string' && id.length > 0 && id.length <= 64);
    if (!idOk) return fail(sess, null, ERR.BAD_REQUEST, 'id må være et heltall eller en kort tekst');
    if (typeof op !== 'string') return fail(sess, id, ERR.BAD_REQUEST, 'op mangler');
    if (args !== undefined && !isObj(args)) return fail(sess, id, ERR.BAD_REQUEST, 'args må være et objekt');
    const fn = Object.hasOwn(OPS, op) && (testOps || !op.startsWith('mock.')) ? OPS[op] : null;
    if (!fn) return fail(sess, id, ERR.UNKNOWN_OP, `Ukjent operasjon: ${op.slice(0, 40)}`);
    if (!sess.hello && op !== 'hello') return fail(sess, id, ERR.HELLO_REQUIRED, 'Første melding må være hello');
    try {
      reply(sess, id, fn(sess, args || {}));
    } catch (err) {
      if (err instanceof DaemonError) return fail(sess, id, err.code, err.message, err.details);
      log(`Intern feil i ${op}: ${err.stack || err}`);
      fail(sess, id, ERR.INTERNAL, 'Intern feil i tjenesten');
    }
  }

  const server = createServer((sock) => {
    const sess = { id: `s${++seq}`, sock, hello: false, topics: new Set(), closed: false, buf: [], bufLen: 0, client: null };
    if (sessions.size >= MAX_CLIENTS) {
      sock.end(`${JSON.stringify({ id: null, ok: false, error: { code: ERR.TOO_MANY_CLIENTS, message: `Høyst ${MAX_CLIENTS} klienter` } })}\n`);
      return;
    }
    sessions.add(sess);
    sock.on('data', (chunk) => {
      let start = 0;
      for (;;) {
        const nl = chunk.indexOf(10, start);
        if (nl === -1) break;
        const part = chunk.subarray(start, nl);
        start = nl + 1;
        if (sess.bufLen + part.length > MAX_LINE_BYTES) return tooLarge();
        const line = sess.bufLen ? Buffer.concat([...sess.buf, part]).toString('utf8') : part.toString('utf8');
        sess.buf = [];
        sess.bufLen = 0;
        const t = line.replace(/\r$/, '');
        if (t.trim()) handleLine(sess, t);
        if (sess.closed) return;
      }
      const rest = chunk.subarray(start);
      if (rest.length) {
        if (sess.bufLen + rest.length > MAX_LINE_BYTES) return tooLarge();
        sess.buf.push(rest);
        sess.bufLen += rest.length;
      }
    });
    function tooLarge() {
      fail(sess, null, ERR.TOO_LARGE, `Meldingen er for stor (maks ${MAX_LINE_BYTES} byte per linje)`, { maxBytes: MAX_LINE_BYTES });
      sess.closed = true;
      sess.buf = [];
      sock.end();
    }
    const gone = () => {
      if (!sessions.has(sess)) return;
      sessions.delete(sess);
      sess.closed = true;
      // Klienten er borte: dens ubekreftede endring rulles tilbake straks.
      card.sessionClosed(sess.id);
    };
    sock.on('close', gone);
    sock.on('error', gone);
  });

  const oldUmask = process.umask(0o077);
  try {
    await new Promise((res, rej) => {
      server.once('error', rej);
      server.listen(socketPath, () => (server.off('error', rej), res()));
    });
  } finally {
    process.umask(oldUmask);
  }
  chmodSync(socketPath, 0o600);
  card.started();

  const timer = setInterval(() => card.tick(tickMs / 1000), tickMs);
  card.tick(tickMs / 1000);

  let closing = null;
  /** Stopper: rull tilbake, vifte til auto, si fra til klientene, fjern socketen. */
  function close(reason = 'stopp', { clean = true } = {}) {
    if (closing) return closing;
    clearInterval(timer);
    const info = card.shutdown(reason, { clean });
    if (info) broadcast('shutdown', info);
    closing = new Promise((res) => {
      server.close(() => res());
      for (const s of sessions) {
        s.closed = true;
        s.sock.end();
      }
      setTimeout(() => {
        for (const s of sessions) s.sock.destroy();
        res();
      }, 300).unref();
    }).then(() => {
      try {
        if (statSync(socketPath).isSocket()) rmSync(socketPath, { force: true });
      } catch {}
    });
    return closing;
  }

  return { card, server, path: socketPath, close, sessions };
}
