// Agentic Rigg – daemon-mock. Simulerer bakgrunnstjenesten (den senere
// Rust-daemonen med root) over en Unix-socket, som vanlig bruker.
//
// SIKKERHET: ren simulering. Ingen NVML, ingen nvidia-smi, ingen
// barneprosesser, ingen I2C, ingen root. Kontrakten: docs/daemon-api.md.
//
// Miljø:
//   AG_DAEMON_SOCKET   sti til socketen (ellers $XDG_RUNTIME_DIR/agentic-rigg/mock-<id>.sock)
//   AG_DATA_DIR        mappe for daemon-mock.json (bekreftet oppsett), som serveren
//   AG_MOCK_CONFIRM_MIN  minste frist i sekunder (standard 10; tester bruker 1)
//   AG_MOCK_TEST_OPS=1   slår på mock.set_temperature (bare tester)

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { socketPath } from '../server/daemon-protocol.js';
import { startDaemon } from './daemon.js';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const VERSION = JSON.parse(readFileSync(join(ROOT, 'version.json'), 'utf8'));
const DATA_DIR = resolve(process.env.AG_DATA_DIR || join(ROOT, '.data'));
const SOCKET = socketPath(process.env);
const minS = Math.max(1, Math.min(60, Number.parseInt(process.env.AG_MOCK_CONFIRM_MIN || '10', 10) || 10));
const log = (...a) => console.log(`[daemon-mock ${new Date().toLocaleTimeString('nb-NO')}]`, ...a);

let d;
try {
  d = await startDaemon({
    socketPath: SOCKET,
    dataDir: DATA_DIR,
    confirm: { defaultS: Math.max(15, minS), minS, maxS: 60 },
    testOps: process.env.AG_MOCK_TEST_OPS === '1',
    log,
    version: VERSION.version,
  });
} catch (err) {
  console.error(`[daemon-mock] Kunne ikke starte: ${err.message}`);
  process.exit(1);
}
log(`Simulert bakgrunnstjeneste ${VERSION.version} (protokoll 1)`);
log(`Socket: ${SOCKET}`);
if (d.card.lastExit && !d.card.lastExit.clean) log('Forrige kjøring stoppet uten å rydde. Viften ville vært satt til auto av systemd (ExecStopPost).');

let stopping = false;
async function stop(reason, code = 0, clean = true) {
  if (stopping) return;
  stopping = true;
  await d.close(reason, { clean });
  process.exit(code);
}
process.on('SIGTERM', () => stop('SIGTERM'));
process.on('SIGINT', () => stop('SIGINT'));
process.on('SIGHUP', () => stop('SIGHUP'));
// Krasj: samme opprydding (vifte til auto, ingen ubekreftet endring), så avslutt med feilkode.
process.on('uncaughtException', (err) => {
  console.error(`[daemon-mock] Krasj: ${err.stack || err}`);
  stop('krasj', 70, false);
});
