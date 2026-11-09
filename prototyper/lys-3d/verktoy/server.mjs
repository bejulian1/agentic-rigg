// Liten statisk server for prototypen (bare 127.0.0.1, bare lesing).
// Roten er agentic-rigg/, så kontrollen kan laste web/js/rgb.js ved siden av
// prototypen og sammenligne fargene. Selve prototypen trenger bare sin egen mappe.

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const PROJECT = fileURLToPath(new URL('../../../', import.meta.url));
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.png': 'image/png', '.svg': 'image/svg+xml', '.json': 'application/json' };

export function serve(root = PROJECT, port = 0) {
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, 'http://127.0.0.1');
      let p = normalize(decodeURIComponent(url.pathname)).replace(/^([/\\])+/, '');
      if (p.endsWith('/') || p === '') p += 'index.html';
      const file = join(root, p);
      if (!file.startsWith(root.endsWith(sep) ? root : root + sep)) throw Object.assign(new Error('utenfor'), { code: 'EACCES' });
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(body);
    } catch (e) {
      res.writeHead(e.code === 'EACCES' ? 403 : 404, { 'content-type': 'text/plain' });
      res.end('Fant ikke');
    }
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve({ server, port: server.address().port, close: () => new Promise((r) => server.close(r)) })));
}

// Kjørt direkte: server bare prototypen. node verktoy/server.mjs [port]
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = fileURLToPath(new URL('../', import.meta.url));
  const { port } = await serve(root, Number(process.argv[2]) || 8765);
  console.log(`Lys i 3D: http://127.0.0.1:${port}/  (Ctrl+C stopper)`);
}
