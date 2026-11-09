// Kjører `node --check` på alle JS-filer i server/, daemon-mock/, web/js/, scripts/ og test/.
import { execFileSync } from 'node:child_process';
import { readdirSync, statSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const files = [];
function walk(d) {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    if (statSync(p).isDirectory()) walk(p);
    else if (p.endsWith('.js')) files.push(p);
  }
}
for (const d of ['server', 'daemon-mock', 'web/js', 'scripts', 'test']) walk(join(root, d));
let bad = 0;
for (const f of files) {
  try {
    execFileSync(process.execPath, ['--check', f], { stdio: 'pipe' });
  } catch (err) {
    bad++;
    console.error(`FEIL ${f}\n${err.stderr}`);
  }
}
console.log(`${files.length - bad}/${files.length} filer ok`);
process.exit(bad ? 1 : 0);
