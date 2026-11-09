// Felles for bakgrunnstjenesten (daemon) og klienten i serveren: protokoll-
// versjon, feilkoder, grenser for meldinger, sti til socketen og oversetting
// mellom appens flate felt (powerLimit, coreOffset …) og kontraktens felt
// (powerLimitW, clockOffsets.coreMhz …). Kontrakten står i docs/daemon-api.md.
//
// Starter ingen programmer. Modulen brukes av server/daemon-client.js,
// daemon-mock/ og scripts/dev.js.

import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Protokollversjonen denne koden snakker. Økes ved brudd i kontrakten. */
export const PROTOCOL = 1;
/** Største tillatte melding (én linje JSON, uten linjeskift). */
export const MAX_LINE_BYTES = 64 * 1024;
/** Høyst så mange samtidige klienter per daemon. */
export const MAX_CLIENTS = 16;

/** Feilkoder i svar: { ok: false, error: { code, message, details? } }. */
export const ERR = Object.freeze({
  BAD_JSON: 'bad_json', // linja er ikke gyldig JSON (forbindelsen beholdes)
  TOO_LARGE: 'message_too_large', // linja er over MAX_LINE_BYTES (forbindelsen lukkes)
  BAD_REQUEST: 'bad_request', // mangler id/op, eller feil type
  HELLO_REQUIRED: 'hello_required', // første melding må være hello
  UNSUPPORTED_PROTOCOL: 'unsupported_protocol',
  UNKNOWN_OP: 'unknown_op',
  INVALID_ARGUMENT: 'invalid_argument', // feil type, ukjent felt eller umulig kombinasjon
  UNKNOWN_GPU: 'unknown_gpu',
  NOT_SUPPORTED: 'not_supported', // kortet/driveren kan ikke dette (NVML_ERROR_NOT_SUPPORTED)
  PERMISSION_DENIED: 'permission_denied', // polkit sa nei (ekte daemon)
  BUSY: 'busy', // en annen klient har en endring som venter på bekreftelse
  NO_PENDING: 'no_pending', // confirm/revert uten noe som venter
  TXN_MISMATCH: 'txn_mismatch', // confirm/revert med feil eller utløpt txn
  NOT_OWNER: 'not_owner', // confirm/revert fra en annen klient enn den som skrev
  NOT_IMPLEMENTED: 'not_implemented', // planlagt (RGB)
  TOO_MANY_CLIENTS: 'too_many_clients',
  SHUTTING_DOWN: 'shutting_down',
  INTERNAL: 'internal',
});

/** Hendelser fra daemonen (uten id): { event, data }. */
export const EVENTS = Object.freeze(['telemetry', 'state', 'txn', 'safety', 'shutdown']);

/** Kort, stabil ID for en datamappe, så flere demoer kan kjøre side om side. */
export function dirId(dir) {
  return createHash('sha256').update(resolve(dir)).digest('hex').slice(0, 10);
}

/**
 * Stien til socketen. AG_DAEMON_SOCKET vinner. Ellers en privat mappe:
 * $XDG_RUNTIME_DIR/agentic-rigg/ (0700, bare brukeren) eller, uten
 * XDG_RUNTIME_DIR, $TMPDIR/agentic-rigg-<uid>/. Filnavnet bygger på
 * datamappa, så brukerens forhåndsvisning og testene aldri deler socket.
 * Unix-socketer har maks ca. 107 tegn i stien, derfor ikke i datamappa.
 */
export function socketPath(env = process.env) {
  if (env.AG_DAEMON_SOCKET) return resolve(env.AG_DAEMON_SOCKET);
  const dataDir = resolve(env.AG_DATA_DIR || join(fileURLToPath(new URL('..', import.meta.url)), '.data'));
  const base = env.XDG_RUNTIME_DIR ? join(env.XDG_RUNTIME_DIR, 'agentic-rigg') : join(tmpdir(), `agentic-rigg-${process.getuid?.() ?? 'u'}`);
  return join(base, `mock-${dirId(dataDir)}.sock`);
}

// ---------------------------------------------------------------- Felt

/**
 * Appens flate felt → sti i kontraktens GpuSettings. Rekkefølgen er den
 * appen viser endringer i.
 */
export const FIELDS = Object.freeze([
  ['powerLimit', 'powerLimitW'],
  ['coreOffset', 'clockOffsets.coreMhz'],
  ['memOffset', 'clockOffsets.memMhz'],
  ['tempTarget', 'tempTargetC'],
  ['fanMode', 'fan.mode'],
  ['fanPercent', 'fan.manualPercent'],
  ['fanCurve', 'fan.curve'],
  ['fanMinPercent', 'fan.minPercent'],
  ['fanHysteresis', 'fan.spindownC'],
  ['fanRampUp', 'fan.rampUpS'],
  ['fanRampDown', 'fan.rampDownS'],
  ['fanSource', 'fan.source'],
  ['fanStartStop', 'fan.startStop.enabled'],
  ['fanStartTemp', 'fan.startStop.startC'],
  ['fanStopTemp', 'fan.startStop.stopC'],
]);
const TO_PATH = new Map(FIELDS);
const TO_FLAT = new Map(FIELDS.map(([a, b]) => [b, a]));
export const flatKey = (path) => TO_FLAT.get(path) || null;
export const contractPath = (key) => TO_PATH.get(key) || null;

function getPath(obj, path) {
  let o = obj;
  for (const p of path.split('.')) {
    if (o == null || typeof o !== 'object') return undefined;
    o = o[p];
  }
  return o;
}
function setPath(obj, path, v) {
  const parts = path.split('.');
  let o = obj;
  for (const p of parts.slice(0, -1)) {
    if (o[p] == null || typeof o[p] !== 'object') o[p] = {};
    o = o[p];
  }
  o[parts[parts.length - 1]] = v;
}

/** Flate felt fra nettleseren → delvis GpuSettings. Ukjente felt tas ikke med. */
export function toSettings(flat) {
  const out = {};
  for (const [k, path] of FIELDS) if (flat[k] !== undefined) setPath(out, path, flat[k]);
  return out;
}

/** Full GpuSettings → flate felt (det appen bruker i `applied`). */
export function toFlat(settings) {
  const out = {};
  for (const [k, path] of FIELDS) {
    const v = getPath(settings, path);
    if (v !== undefined) out[k] = k === 'fanCurve' ? v.map((p) => [...p]) : v;
  }
  return out;
}

export { getPath, setPath };
