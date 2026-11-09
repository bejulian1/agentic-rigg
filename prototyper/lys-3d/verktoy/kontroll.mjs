// Kontroll av prototypen i headless Chrome:
//   1. Effektmotoren i prototypen mot web/js/rgb.js (samme farger, bilde for bilde).
//   2. Interaksjon: mus, berøring, tastatur, lagtrinn, redusert bevegelse, skjult fane.
//   3. Ytelse: tegnetid per bilde, bilder per sekund under animasjon og at løkka sover.
//   4. Skjermbilder i lyst og mørkt tema til ../skjermbilder/.
//   5. Ingen konsollfeil.
//
// Bruker test/helpers/chrome.js (uendret) for Chrome med programvare-GPU
// (SwiftShader, verste tilfelle). Med --gpu startes i tillegg en egen headless
// Chrome som tegner på maskinens skjermkort (ANGLE/OpenGL), bare for å måle.
// Ingenting skrives til kortet; det brukes bare til å tegne bilder.
//
//   node prototyper/lys-3d/verktoy/kontroll.mjs            alt unntatt GPU-måling
//   node prototyper/lys-3d/verktoy/kontroll.mjs --gpu      også måling på maskinens GPU
//   node prototyper/lys-3d/verktoy/kontroll.mjs --uten-bilder

import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { launchChrome, Page, CHROME } from '../../../test/helpers/chrome.js';
import { serve } from './server.mjs';

const OUT = fileURLToPath(new URL('../skjermbilder/', import.meta.url));
const args = new Set(process.argv.slice(2));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const ok = (name, pass, detail = '') => {
  results.push({ name, pass, detail });
  console.log(`${pass ? 'ok  ' : 'FEIL'} ${name}${detail ? ` – ${detail}` : ''}`);
};

const srv = await serve();
const base = `http://127.0.0.1:${srv.port}/prototyper/lys-3d/index.html`;

async function open(page, query = '', { w = 1440, h = 900, scheme = 'dark', reduce = false } = {}) {
  page.where = query || '(standard)';
  await page.size(w, h);
  await page.media(scheme, reduce);
  await page.navigate(`${base}${query}`);
  await page.waitFor('window.lys3d && window.lys3d.view && window.lys3d.view.stats.frames > 0', { timeout: 20000, what: '3D-visningen' });
}
const settle = (page) => page.eval('new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))');

// ---------------------------------------------------------------- 1. Effektmotoren mot rgb.js
async function engineCheck(page) {
  await open(page, '?skall=av');
  const r = await page.eval(`(async () => {
    const { RgbPreview } = await import('/web/js/rgb.js');
    const { RgbEngine } = await import('/prototyper/lys-3d/js/engine.js');
    const stage = document.createElement('div');
    const host = document.createElement('div');
    stage.style.cssText = 'position:absolute;left:-9999px;width:660px;height:280px';
    stage.append(host);
    document.body.append(stage);
    const dur = () => { const v = getComputedStyle(document.documentElement).getPropertyValue('--dur-4').trim(); return v.endsWith('ms') ? parseFloat(v) : parseFloat(v) * 1000; };
    const sink = { setZoneColor() {}, setLedColors() {}, commit() {} };
    const base = { color: '#3a7bf0', zones: { bar: '#3a7bf0', fan1: '#e5383b', fan2: '#3fb950' }, brightness: 80, speed: 5, direction: 'cw', tempCold: '#3a7bf0', tempHot: '#e5383b', tempMin: 40, tempMax: 80, ambient: true };
    const out = [];
    for (const motion of ['full', 'reduced', 'off']) {
      document.documentElement.dataset.motion = motion;
      const prev = new RgbPreview(host, stage, 'X');
      const eng = new RgbEngine(sink, { motion: () => document.documentElement.dataset.motion, crossfadeMs: dur });
      prev.setTemp(63); eng.setTemp(63);
      let worst = 0, worstK = 0, steps = 0;
      for (const effect of ['static', 'breathe', 'rainbow', 'cycle', 'temperature', 'off', 'rainbow']) {
        for (const direction of ['cw', 'ccw']) {
          const cfg = { ...base, effect, direction, speed: effect === 'cycle' ? 8 : 5 };
          prev.setConfig(cfg); eng.setConfig(cfg);
          for (let i = 0; i < 90; i++) {
            const dt = 1 / 60 + (i % 7) * 0.0013;
            prev.tick(dt); eng.tick(dt);
            for (let j = 0; j < prev.cur.length; j++) worst = Math.max(worst, Math.abs(prev.cur[j] - eng.cur[j]));
            worstK = Math.max(worstK, Math.abs(prev.layerK - eng.layerK));
            steps++;
          }
        }
      }
      out.push({ motion, worst, worstK, steps });
    }
    stage.remove();
    document.documentElement.dataset.motion = 'full';
    return out;
  })()`);
  for (const x of r) ok(`motor = rgb.js (${x.motion})`, x.worst < 1e-3 && x.worstK < 1e-6, `${x.steps} bilder, største avvik ${x.worst.toFixed(6)} (farge 0–255), ${x.worstK} (pust)`);
}

// ---------------------------------------------------------------- 2. Interaksjon
async function interaction(page) {
  await open(page, '?skall=av');
  const vpRect = await page.eval(`(() => { const r = document.querySelector('#viewport').getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })()`);
  const cx = vpRect.x + vpRect.w / 2;
  const cy = vpRect.y + vpRect.h / 2;
  // Dra med mus
  const yaw0 = await page.eval('lys3d.view.goal.yaw');
  await page.cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: cx, y: cy, button: 'left', buttons: 1, clickCount: 1 });
  for (let i = 1; i <= 10; i++) {
    await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: cx + i * 20, y: cy + i * 4, button: 'left', buttons: 1 });
    await sleep(16);
  }
  await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: cx + 200, y: cy + 40, button: 'left', buttons: 0, clickCount: 1 });
  await sleep(400);
  const yaw1 = await page.eval('lys3d.view.goal.yaw');
  ok('dra med mus vrir kortet', yaw1 < yaw0 - 40, `yaw ${yaw0.toFixed(1)} → ${yaw1.toFixed(1)}`);
  // Berøring
  await page.eval('lys3d.view.setCamera({ yaw: -33, pitch: 18 })');
  const p0 = await page.eval('lys3d.view.goal.pitch');
  await page.cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: cx, y: cy, id: 1 }] });
  for (let i = 1; i <= 8; i++) {
    await page.cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: cx - i * 10, y: cy + i * 12, id: 1 }] });
    await sleep(16);
  }
  await page.cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await sleep(200);
  const t1 = await page.eval('({ yaw: lys3d.view.goal.yaw, pitch: lys3d.view.goal.pitch })');
  ok('dra med berøring vrir kortet', t1.pitch > p0 + 10 && t1.yaw > -33 + 10, `yaw −33 → ${t1.yaw.toFixed(1)}, pitch ${p0} → ${t1.pitch.toFixed(1)}`);
  // Tastatur
  await page.eval('lys3d.view.setCamera({ yaw: -33, pitch: 18 }); document.querySelector("#viewport").focus()');
  await page.key('ArrowRight', 'ArrowRight', 39);
  await page.key('ArrowUp', 'ArrowUp', 38);
  const k1 = await page.eval('({ yaw: lys3d.view.goal.yaw, pitch: lys3d.view.goal.pitch })');
  ok('piltaster vrir (15° og 10°)', Math.abs(k1.yaw - -18) < 0.01 && Math.abs(k1.pitch - 28) < 0.01, `yaw ${k1.yaw}, pitch ${k1.pitch}`);
  await page.key('+', 'Equal', 187);
  const z = await page.eval('lys3d.view.goal.zoom');
  await page.key('0', 'Digit0', 48);
  const k2 = await page.eval('({ yaw: lys3d.view.goal.yaw, pitch: lys3d.view.goal.pitch, zoom: lys3d.view.goal.zoom })');
  ok('+ zoomer og 0 tilbakestiller', z > 1.1 && Math.abs(k2.yaw - -33) < 0.01 && k2.zoom === 1, `zoom ${z.toFixed(2)} → ${k2.zoom}, yaw ${k2.yaw}`);
  const btn = await page.eval(`(() => { lys3d.view.setCamera({ yaw: 100, pitch: -20, zoom: 2 }); document.querySelector('#reset-view').click(); return lys3d.view.goal.yaw; })()`);
  ok('knappen Tilbakestill', Math.abs(((btn % 360) + 360) % 360 - 327) < 0.01, `yaw ${btn}`);
  // Lagtrinn med tastatur i segmentet
  await page.eval(`document.querySelector('#step-seg button[aria-checked="true"]').focus()`);
  await page.key('ArrowRight', 'ArrowRight', 39);
  await page.key('ArrowRight', 'ArrowRight', 39);
  await page.key('ArrowRight', 'ArrowRight', 39);
  await page.waitFor('Object.values(lys3d.view.layerState).every((s) => s.out === s.goal)', { timeout: 8000, what: 'at lagene er ferdige' }).catch(() => {});
  await sleep(200);
  const st = await page.eval(`({ step: lys3d.view.step, out: Object.fromEntries(Object.entries(lys3d.view.layerState).map(([k, v]) => [k, v.out])), parts: document.querySelectorAll('#parts li').length, desc: document.querySelector('#view-desc').textContent })`);
  ok('Bare kretskort med piltaster i lagvalget', st.step === 3 && st.out.deksel === 1 && st.out.kjoler === 1 && st.out.bakplate === 1 && st.out.kretskort === 0, `${st.parts} deler i lista, ${JSON.stringify(st.out)}`);
  ok('beskrivelsen følger visningen', /Bare kretskortet/.test(st.desc), st.desc.slice(0, 90));
  // Fokus synlig
  await page.key('Tab', 'Tab', 9, 1);
  await page.key('Tab', 'Tab', 9);
  const fv = await page.eval(`(() => { const v = document.querySelector('#viewport'); v.focus(); const a = getComputedStyle(v, '::after'); return document.activeElement === v && v.matches(':focus-visible') ? a.opacity + ' ' + a.boxShadow : 'ikke fokus'; })()`);
  ok('3D-visningen har synlig fokus', /^1 .*inset/.test(fv), fv);
  // Klikk på en lyssone velger sonen
  await page.eval(`(() => { document.querySelector('#step-seg button[data-v="0"]').click(); lys3d.set({ effect: 'static' }); })()`);
  await sleep(1500);
  await page.eval('lys3d.view.setCamera({ yaw: 0, pitch: 4 })');
  await settle(page);
  const ring = await page.eval('lys3d.view.projectCard([217 + Math.cos(1.05) * 49.6, 57.5 + Math.sin(1.05) * 49.6, 46.6])');
  await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: vpRect.x + ring[0], y: vpRect.y + ring[1] });
  await sleep(150);
  await page.cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: vpRect.x + ring[0], y: vpRect.y + ring[1], button: 'left', buttons: 1, clickCount: 1 });
  await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: vpRect.x + ring[0], y: vpRect.y + ring[1], button: 'left', buttons: 0, clickCount: 1 });
  await sleep(200);
  const zc = await page.eval(`document.querySelector('#zone-seg button[aria-checked="true"]').dataset.v`);
  ok('klikk på lysringen velger sonen', zc === 'fan2', `sone: ${zc}`);
  const tipTxt = await page.eval(`document.querySelector('#hover-tip').textContent`);
  ok('pekeren viser delens navn', /Lysring, vifte 2/.test(tipTxt), tipTxt);
}

// ---------------------------------------------------------------- 3. Ytelse
async function perf(page, label) {
  const out = {};
  await open(page, '?skall=av');
  out.renderer = await page.eval(`(() => { const gl = lys3d.view.r.gl; const d = gl.getExtension('WEBGL_debug_renderer_info'); return d ? gl.getParameter(d.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER); })()`);
  out.startup = await page.eval('({ teksturer: lys3d.view.stats.textureMs, bygg: lys3d.view.stats.buildMs, hjorner: lys3d.view.verts, trekanter: lys3d.view.tris, kall: lys3d.view.r.stats.draws })');
  // Stillestående: statisk lys og stoppet vifte → løkka skal sove.
  await page.eval(`lys3d.set({ effect: 'static' }); lys3d.sim.auto = false; lys3d.sim.fan = 0; lys3d.renderSim();`);
  await page.waitFor('lys3d.view.fanRate === 0 && !lys3d.view.raf', { timeout: 20000, what: 'at viftene stopper og løkka sover' }).catch(() => {});
  await sleep(300);
  const f0 = await page.eval('lys3d.view.stats.frames');
  await sleep(2000);
  out.idleFrames = (await page.eval('lys3d.view.stats.frames')) - f0;
  // Animert: regnbue og vifter på 60 %.
  await page.eval(`lys3d.set({ effect: 'rainbow' }); lys3d.sim.fan = 60; lys3d.renderSim(); lys3d.view.stats.drawMs.length = 0;`);
  await sleep(1500);
  const a0 = await page.eval('({ f: lys3d.view.stats.frames, t: performance.now() })');
  await page.eval('lys3d.view.stats.drawMs.length = 0');
  await sleep(3000);
  const a1 = await page.eval('({ f: lys3d.view.stats.frames, t: performance.now(), ms: lys3d.view.stats.drawMs.slice() })');
  out.animFps = ((a1.f - a0.f) * 1000) / (a1.t - a0.t);
  const ms = a1.ms.sort((x, y) => x - y);
  out.cpuMsAvg = ms.reduce((s, v) => s + v, 0) / ms.length;
  out.cpuMsP95 = ms[Math.floor(ms.length * 0.95)];
  // Tid per bilde med GPU-en ventet ferdig (uten og med skyggekart).
  const soft = /SwiftShader/i.test(out.renderer);
  out.adapt = await page.eval('({ skala: lys3d.view.scale, stillbilde: lys3d.view.slow })');
  out.benchMs = await page.eval(`lys3d.view.bench(${soft ? 12 : 120})`);
  out.benchShadowMs = await page.eval(`lys3d.view.bench(${soft ? 6 : 60}, { shadow: true })`);
  // Skjult fane: ingenting tegnes.
  await page.eval(`Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange'));`);
  await sleep(300);
  const h0 = await page.eval('lys3d.view.stats.frames');
  await sleep(1500);
  out.hiddenFrames = (await page.eval('lys3d.view.stats.frames')) - h0;
  // Redusert bevegelse: regnbuen står stille, viftene står, løkka sover.
  await open(page, '?skall=av&effekt=rainbow', { reduce: true });
  await sleep(1500);
  const r0 = await page.eval('({ f: lys3d.view.stats.frames, m: document.documentElement.dataset.motion })');
  await sleep(2000);
  out.reducedFrames = (await page.eval('lys3d.view.stats.frames')) - r0.f;
  out.reducedMotion = r0.m;
  console.log(`\n${label}: ${out.renderer}`);
  console.log(`  oppstart: teksturer ${out.startup.teksturer.toFixed(0)} ms, bygging og opplasting ${out.startup.bygg.toFixed(0)} ms, ${out.startup.hjorner} hjørner, ${Math.round(out.startup.trekanter)} trekanter, ${out.startup.kall} tegnekall for delene`);
  console.log(`  animert (regnbue + vifter): ${out.animFps.toFixed(1)} bilder/s, JS per bilde snitt ${out.cpuMsAvg.toFixed(2)} ms (p95 ${out.cpuMsP95.toFixed(2)} ms)`);
  console.log(`  vern mot treg grafikk: oppløsning ${Math.round(out.adapt.skala * 100)} %, ${out.adapt.stillbilde ? 'løpende animasjoner stoppet' : 'animasjonene går'}`);
  console.log(`  ventet ferdig (CPU + GPU): ${out.benchMs.toFixed(2)} ms per bilde, ${out.benchShadowMs.toFixed(2)} ms med nytt skyggekart`);
  console.log(`  i ro: ${out.idleFrames} bilder på 2 s · skjult fane: ${out.hiddenFrames} bilder på 1,5 s · redusert bevegelse (${out.reducedMotion}): ${out.reducedFrames} bilder på 2 s`);
  ok(`${label}: løkka sover i ro`, out.idleFrames === 0, `${out.idleFrames} bilder`);
  ok(`${label}: ingenting tegnes i skjult fane`, out.hiddenFrames === 0, `${out.hiddenFrames} bilder`);
  ok(`${label}: redusert bevegelse er et stillbilde`, out.reducedMotion === 'reduced' && out.reducedFrames === 0, `${out.reducedFrames} bilder`);
  ok(`${label}: høyst ~60 bilder/s under animasjon`, out.animFps <= 66, `${out.animFps.toFixed(1)}`);
  return out;
}

// ---------------------------------------------------------------- 4. Skjermbilder
const SHOTS = [
  { name: 'morkt-hel-skraatt', q: '' },
  { name: 'lyst-hel-skraatt', q: '', scheme: 'light' },
  { name: 'morkt-hel-forfra-regnbue', q: '?vinkel=forfra&effekt=rainbow' },
  { name: 'lyst-hel-ovenfra-lyslist', q: '?vinkel=ovenfra&effekt=cycle', scheme: 'light' },
  { name: 'morkt-eksplodert-regnbue', q: '?skill=100&effekt=rainbow', cam: { yaw: -58, pitch: 16 } },
  { name: 'lyst-eksplodert-temperatur', q: '?skill=100&effekt=temperature&temp=74', cam: { yaw: 52, pitch: 20 }, scheme: 'light' },
  { name: 'morkt-uten-deksel', q: '?lag=1&effekt=rainbow', cam: { yaw: 34, pitch: 24 } },
  { name: 'lyst-uten-kjoler-bakfra', q: '?lag=2&vinkel=bakfra', scheme: 'light' },
  { name: 'morkt-bare-kretskort', q: '?lag=3' },
  { name: 'lyst-bare-kretskort-ovenfra', q: '?lag=3', cam: { yaw: 0, pitch: 42 }, scheme: 'light' },
  { name: 'morkt-kretskort-naerbilde-gpu', q: '?lag=3', cam: { yaw: -14, pitch: 34, zoom: 2.3 }, js: 'lys3d.view.goal.target = lys3d.view.cam.target = [112 - lys3d.view.center[0], 60 - lys3d.view.center[1], 0 - lys3d.view.center[2]]; lys3d.view.change()' },
  { name: 'morkt-bakfra-fargesyklus', q: '?vinkel=bakfra&effekt=cycle' },
  { name: 'lyst-braketten-utganger', q: '?effekt=temperature&temp=72', cam: { yaw: -92, pitch: 8 }, scheme: 'light' },
  { name: 'morkt-peker-pa-gpu', q: '?lag=3', hover: [112, 60, 2.4] },
  { name: 'lyst-tastaturfokus', q: '', scheme: 'light', focus: true },
  { name: 'morkt-redusert-bevegelse', q: '?effekt=breathe', reduce: true },
  { name: 'morkt-bakgrunnslys', q: '?vinkel=forfra&effekt=rainbow&bakgrunnslys=på' },
  { name: 'lyst-uten-skall-1100', q: '?skall=av', w: 1100, h: 760, scheme: 'light' },
  { name: 'morkt-mobil-390', q: '?lag=1', w: 390, h: 844 },
];
async function shots(page) {
  mkdirSync(OUT, { recursive: true });
  // Ingen valg fra kontrollene over (sone, tema) skal henge igjen i bildene.
  await open(page, '?skall=av');
  await page.eval('localStorage.clear()');
  for (const s of SHOTS) {
    // Fast kvalitet: programvare-GPU-en i headless Chrome er treg, og vernet
    // mot treg grafikk skal ikke senke oppløsningen i skjermbildene.
    const q = `${s.q || '?'}${s.q ? '&' : ''}kvalitet=fast`;
    await open(page, q, { w: s.w || 1440, h: s.h || 900, scheme: s.scheme || 'dark', reduce: !!s.reduce });
    if (s.cam) await page.eval(`lys3d.view.setCamera(${JSON.stringify(s.cam)})`);
    if (s.js) await page.eval(s.js);
    await sleep(900);
    if (s.hover) {
      const r = await page.eval(`(() => { const r = document.querySelector('#viewport').getBoundingClientRect(); const p = lys3d.view.projectCard(${JSON.stringify(s.hover)}); return [r.left + p[0], r.top + p[1]]; })()`);
      await page.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: r[0], y: r[1] });
      await sleep(300);
    }
    if (s.focus) {
      for (let i = 0; i < 30; i++) {
        if ((await page.eval('document.activeElement && document.activeElement.id')) === 'viewport') break;
        await page.key('Tab', 'Tab', 9);
      }
      await page.key('ArrowRight', 'ArrowRight', 39);
      await sleep(600);
    }
    await settle(page);
    const shot = await page.cdp.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(OUT, `${s.name}.png`), Buffer.from(shot.data, 'base64'));
  }
  ok(`skjermbilder (${SHOTS.length})`, true, OUT);
}

// ---------------------------------------------------------------- Chrome på maskinens GPU (bare måling)
async function launchGpuChrome() {
  const profile = mkdtempSync(join(tmpdir(), 'lys3d-gpu-'));
  const child = spawn(CHROME, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-background-networking', '--disable-component-update', '--proxy-server=http://127.0.0.1:9', '--use-angle=gl', '--enable-gpu', '--ignore-gpu-blocklist', '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore', detached: true });
  const kill = async () => {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch {}
    await sleep(300);
    rmSync(profile, { recursive: true, force: true });
  };
  process.once('exit', () => {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch {}
  });
  let port = 0;
  for (let i = 0; i < 150 && !port; i++) {
    try {
      port = Number(readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]);
    } catch {}
    if (!port) await sleep(100);
  }
  if (!port) {
    await kill();
    throw new Error('Chrome med GPU startet ikke');
  }
  const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const ws = new WebSocket(list.find((t) => t.type === 'page').webSocketDebuggerUrl);
  await new Promise((r, j) => {
    ws.addEventListener('open', r, { once: true });
    ws.addEventListener('error', j, { once: true });
  });
  // Minste CDP-klient som Page fra hjelperen kan bruke.
  let id = 0;
  const pending = new Map();
  const handlers = new Map();
  ws.addEventListener('message', (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(m.error.message)) : resolve(m.result);
    } else if (m.method) for (const fn of handlers.get(m.method) || []) fn(m.params);
  });
  const cdp = {
    send: (method, params = {}) =>
      new Promise((resolve, reject) => {
        const i = ++id;
        pending.set(i, { resolve, reject });
        ws.send(JSON.stringify({ id: i, method, params }));
      }),
    on: (m, fn) => handlers.set(m, [...(handlers.get(m) || []), fn]),
  };
  const page = new Page(cdp);
  await page.init();
  return {
    page,
    close: async () => {
      ws.close();
      await kill();
    },
  };
}

// ---------------------------------------------------------------- Kjør
const chrome = await launchChrome();
let gpu = null;
try {
  const page = chrome.page;
  await engineCheck(page);
  await interaction(page);
  await perf(page, 'Programvare-GPU (SwiftShader)');
  if (!args.has('--uten-bilder')) await shots(page);
  const errs = page.errors.filter((e) => !/favicon/.test(e.text));
  ok('ingen konsollfeil', errs.length === 0, errs.map((e) => `${e.where}: ${e.text}`).join(' | '));
  if (args.has('--gpu')) {
    gpu = await launchGpuChrome();
    await perf(gpu.page, 'Maskinens GPU');
    const e2 = gpu.page.errors;
    ok('ingen konsollfeil (GPU)', e2.length === 0, e2.map((e) => e.text).join(' | '));
  }
} finally {
  await chrome.close();
  await gpu?.close();
  await srv.close();
}
const failed = results.filter((r) => !r.pass);
console.log(`\n${results.length - failed.length} av ${results.length} kontroller i orden.`);
process.exit(failed.length ? 1 : 0);
