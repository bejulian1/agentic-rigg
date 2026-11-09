// npm start: starter bakgrunnstjenesten (daemon-mock) og serveren, med samme
// socket og datamappe. Serveren selv starter aldri andre programmer (bortsett
// fra nvidia-smi i nvidia.js), så mocken startes her.
//
// - Mocken startes først; serveren startes når socketen finnes (høyst 5 s).
// - Krasjer mocken (kode ≠ 0), startes den igjen etter 2 s, høyst 5 ganger per
//   minutt, omtrent som systemd Restart=on-failure. Stopper den rent (kode 0,
//   f.eks. etter SIGTERM), får den ligge, så appen viser «ikke tilkoblet».
// - Stopper serveren, stoppes mocken også, og prosessen avslutter med
//   serverens kode. SIGINT/SIGTERM/SIGHUP sendes videre til begge.
//
// Miljø: AG_PORT, AG_DATA_DIR, AG_SIMULATE … gjelder serveren som før.
// AG_DAEMON_SOCKET overstyrer socketen. AG_NO_DAEMON=1 starter bare serveren.

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { socketPath } from '../server/daemon-protocol.js';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const env = { ...process.env, AG_DAEMON_SOCKET: socketPath(process.env) };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let mock = null;
let server = null;
let stopping = false;
const restarts = [];

function startMock() {
  mock = spawn(process.execPath, [join(ROOT, 'daemon-mock', 'index.js')], { cwd: ROOT, env, stdio: 'inherit' });
  const me = mock;
  me.on('exit', (code, signal) => {
    if (mock === me) mock = null;
    if (stopping) return;
    if (code === 0 || signal === 'SIGTERM' || signal === 'SIGINT') {
      console.log('[dev] Bakgrunnstjenesten stoppet. Appen viser «ikke tilkoblet» til den startes igjen.');
      return;
    }
    const now = Date.now();
    while (restarts.length && now - restarts[0] > 60_000) restarts.shift();
    if (restarts.length >= 5) {
      console.error('[dev] Bakgrunnstjenesten krasjet fem ganger på ett minutt. Starter den ikke igjen.');
      return;
    }
    restarts.push(now);
    console.error(`[dev] Bakgrunnstjenesten stoppet uventet (kode ${code}, signal ${signal}). Starter igjen om 2 s.`);
    setTimeout(() => !stopping && startMock(), 2000);
  });
}

function stopAll(sig = 'SIGTERM') {
  if (stopping) return;
  stopping = true;
  server?.kill(sig);
  mock?.kill('SIGTERM');
  // Høyst 3 s til begge har ryddet.
  setTimeout(() => process.exit(1), 3000).unref();
}

for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => stopAll(sig));

if (process.env.AG_NO_DAEMON !== '1') {
  startMock();
  for (let i = 0; i < 50 && !existsSync(env.AG_DAEMON_SOCKET) && mock; i++) await sleep(100);
}

server = spawn(process.execPath, [join(ROOT, 'server', 'index.js')], { cwd: ROOT, env, stdio: 'inherit' });
server.on('exit', async (code, signal) => {
  stopping = true;
  if (mock) {
    mock.kill('SIGTERM');
    for (let i = 0; i < 30 && mock; i++) await sleep(100);
  }
  process.exit(code ?? (signal ? 1 : 0));
});
