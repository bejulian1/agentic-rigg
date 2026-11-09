// Teksturer tegnet med canvas 2D ved oppstart: kretskortets for- og
// bakside (loddemaske, ledere, gjennomføringer, puter, silketrykk), et
// etikettatlas for brikkene, braketten og bakplaten (med åpninger i
// alfakanalen). Ingen bildefiler, ingenting lastes ned.

import { PCB_TEX, GPU, MEMORY, CHOKES, POWERSTAGES, CAPS, POWER, PORTS, HOLES, ICS, HEADERS, FINGERS, BRACKET, BACKPLATE, smallParts, backCaps } from './layout.js';

const MONO = '"Ubuntu Sans Mono", "DejaVu Sans Mono", "Liberation Mono", monospace';
const SANS = '"Ubuntu Sans", "DejaVu Sans", "Liberation Sans", sans-serif';

function canvas(w, h) {
  const c = typeof OffscreenCanvas === 'function' ? new OffscreenCanvas(w, h) : Object.assign(document.createElement('canvas'), { width: w, height: h });
  // CPU-lerret: tegnes én gang og lastes rett opp, uten å lese tilbake fra GPU-en.
  return [c, c.getContext('2d', { willReadFrequently: true })];
}

/** Fast pseudotilfeldighet (samme tekstur hver gang). */
function rng(seed) {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return (s - 1) / 2147483646;
  };
}

// ---------------------------------------------------------------------------
// Etikettatlas (1024 × 1024): flater med egen farge og merking.
// Regionene oppgis i piksler (x, y, b, h) fra øverst til venstre; uv() gir
// uv-rektangelet (v opp, fordi teksturen lastes med UNPACK_FLIP_Y).
// ---------------------------------------------------------------------------
export const ATLAS = 1024;
const REG = {
  white: [0, 0, 16, 16],
  mem: [16, 0, 280, 240],
  die: [300, 0, 254, 216],
  sub: [560, 0, 400, 400],
  choke: [16, 250, 200, 200],
  ps: [220, 250, 120, 100],
  cap: [220, 360, 128, 128],
  hub: [16, 500, 256, 256],
  ic: [360, 410, 160, 160],
  hdr: [280, 580, 168, 64],
  plain: [960, 960, 64, 64],
};
export function atlasUv(name, inset = 0) {
  const [x, y, w, h] = REG[name];
  return [(x + inset) / ATLAS, 1 - (y + h - inset) / ATLAS, (x + w - inset) / ATLAS, 1 - (y + inset) / ATLAS];
}
/** Et punkt i regionens kant (brukes for sidene på brikken, så de får samme farge). */
export function atlasPoint(name) {
  const [x, y] = REG[name];
  return [(x + 3) / ATLAS, 1 - (y + 3) / ATLAS];
}

export function makeAtlas() {
  const [c, g] = canvas(ATLAS, ATLAS);
  g.fillStyle = '#808080';
  g.fillRect(0, 0, ATLAS, ATLAS);
  const R = rng(7);
  const region = (name, fill) => {
    const [x, y, w, h] = REG[name];
    g.fillStyle = fill;
    g.fillRect(x, y, w, h);
    return [x, y, w, h];
  };
  region('white', '#ffffff');
  region('plain', '#ffffff');

  // GDDR6: matt svart med lasermerking og pinne 1-prikk.
  {
    const [x, y, w, h] = region('mem', '#16171a');
    for (let i = 0; i < 900; i++) {
      g.fillStyle = `rgba(255,255,255,${0.012 + R() * 0.02})`;
      g.fillRect(x + R() * w, y + R() * h, 2, 2);
    }
    g.fillStyle = '#26282c';
    g.beginPath();
    g.arc(x + 26, y + 26, 9, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#80848b';
    g.font = `600 30px ${MONO}`;
    g.textBaseline = 'middle';
    g.fillText('GDDR6 8Gb', x + 52, y + 74);
    g.font = `500 24px ${MONO}`;
    g.fillText('14G  X2A41', x + 52, y + 118);
    g.fillText('BGA180 · 2042', x + 52, y + 156);
    g.font = `500 20px ${MONO}`;
    g.fillStyle = '#6a6e75';
    g.fillText('8 Gb · 1.35 V', x + 52, y + 194);
  }
  // GPU-brikken (silisium sett ovenfra): mørk, med svak struktur og kant.
  {
    const [x, y, w, h] = region('die', '#1d1c26');
    const gr = g.createLinearGradient(x, y, x + w, y + h);
    gr.addColorStop(0, 'rgba(120,110,170,0.10)');
    gr.addColorStop(0.5, 'rgba(0,0,0,0)');
    gr.addColorStop(1, 'rgba(90,120,150,0.08)');
    g.fillStyle = gr;
    g.fillRect(x, y, w, h);
    g.strokeStyle = 'rgba(160,160,190,0.10)';
    g.lineWidth = 1;
    for (let i = 1; i < 6; i++) {
      g.strokeRect(x + (w * i) / 12, y + (h * i) / 12, w - (w * i) / 6, h - (h * i) / 6);
    }
    g.strokeStyle = 'rgba(200,200,220,0.25)';
    g.lineWidth = 3;
    g.strokeRect(x + 2, y + 2, w - 4, h - 4);
  }
  // Substratet rundt brikken: mørk grønnbrun, gylne merker og små kondensatorer.
  {
    const [x, y, w, h] = region('sub', '#2a3324');
    for (let i = 0; i < 1600; i++) {
      g.fillStyle = `rgba(${R() < 0.5 ? '200,190,140' : '0,0,0'},${0.03 + R() * 0.05})`;
      g.fillRect(x + R() * w, y + R() * h, 2, 2);
    }
    // Trekant for pinne 1
    g.fillStyle = '#c9a24e';
    g.beginPath();
    g.moveTo(x + 14, y + 14);
    g.lineTo(x + 44, y + 14);
    g.lineTo(x + 14, y + 44);
    g.closePath();
    g.fill();
    // Kondensatorer i en ramme rundt brikken (brikken dekker midten).
    const s = w / 40; // px per mm
    const dw = GPU.dieW * s;
    const dh = GPU.dieH * s;
    const ox = x + (w - dw) / 2;
    const oy = y + (h - dh) / 2;
    for (let i = 0; i < 18; i++) {
      for (const [px, py, rot] of [
        [ox + (i / 17) * dw, oy - 2.6 * s, false],
        [ox + (i / 17) * dw, oy + dh + 2.6 * s, false],
        [ox - 2.6 * s, oy + (i / 17) * dh, true],
        [ox + dw + 2.6 * s, oy + (i / 17) * dh, true],
      ]) {
        const cw = (rot ? 0.5 : 1.0) * s;
        const ch = (rot ? 1.0 : 0.5) * s;
        g.fillStyle = '#9c8a64';
        g.fillRect(px - cw / 2, py - ch / 2, cw, ch);
        g.fillStyle = '#c9cbc8';
        if (rot) {
          g.fillRect(px - cw / 2, py - ch / 2, cw, ch * 0.22);
          g.fillRect(px - cw / 2, py + ch / 2 - ch * 0.22, cw, ch * 0.22);
        } else {
          g.fillRect(px - cw / 2, py - ch / 2, cw * 0.22, ch);
          g.fillRect(px + cw / 2 - cw * 0.22, py - ch / 2, cw * 0.22, ch);
        }
      }
    }
    g.fillStyle = 'rgba(210,205,170,0.55)';
    g.font = `600 22px ${MONO}`;
    g.fillText('GPU-545', x + 26, y + h - 30);
    g.font = `500 18px ${MONO}`;
    g.fillText('A1  2042', x + w - 120, y + h - 30);
  }
  // Spole: grå metallkompositt med merking.
  {
    const [x, y, w, h] = region('choke', '#3d4045');
    for (let i = 0; i < 1400; i++) {
      g.fillStyle = `rgba(${R() < 0.5 ? '255,255,255' : '0,0,0'},${0.03 + R() * 0.05})`;
      g.fillRect(x + R() * w, y + R() * h, 2, 2);
    }
    g.strokeStyle = 'rgba(0,0,0,0.35)';
    g.lineWidth = 4;
    g.strokeRect(x + 6, y + 6, w - 12, h - 12);
    g.fillStyle = '#a7abb1';
    g.font = `700 52px ${MONO}`;
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText('R22', x + w / 2, y + h / 2 - 6);
    g.font = `500 22px ${MONO}`;
    g.fillStyle = '#8a8e94';
    g.fillText('2042', x + w / 2, y + h / 2 + 40);
    g.textAlign = 'left';
  }
  // Effekttrinn (MOSFET): liten svart QFN.
  {
    const [x, y, w, h] = region('ps', '#1a1b1e');
    g.fillStyle = '#6d7178';
    g.font = `600 22px ${MONO}`;
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText('PS50', x + w / 2, y + h / 2 - 10);
    g.font = `500 16px ${MONO}`;
    g.fillText('A21', x + w / 2, y + h / 2 + 16);
    g.textAlign = 'left';
    g.fillStyle = '#2a2c30';
    g.beginPath();
    g.arc(x + 14, y + 14, 6, 0, Math.PI * 2);
    g.fill();
  }
  // Polymerkondensator sett ovenfra: aluminium med kryss (trykkavlastning) og polaritet.
  {
    const [x, y, w, h] = region('cap', '#b9bdc2');
    const cx = x + w / 2;
    const cy = y + h / 2;
    const rg = g.createRadialGradient(cx - 14, cy - 14, 4, cx, cy, w / 2);
    rg.addColorStop(0, '#d9dcdf');
    rg.addColorStop(1, '#a3a8ae');
    g.fillStyle = rg;
    g.fillRect(x, y, w, h);
    g.strokeStyle = 'rgba(40,44,50,0.55)';
    g.lineWidth = 5;
    g.beginPath();
    g.moveTo(cx - w * 0.3, cy);
    g.lineTo(cx + w * 0.3, cy);
    g.moveTo(cx, cy - h * 0.3);
    g.lineTo(cx, cy + h * 0.3);
    g.stroke();
    g.strokeStyle = '#2b2f36';
    g.lineWidth = 14;
    g.beginPath();
    g.arc(cx, cy, w / 2 - 8, Math.PI * 0.75, Math.PI * 1.25);
    g.stroke();
  }
  // Navet på viften: mørk skive med en tynn ring. Ingen logo.
  {
    const [x, y, w, h] = region('hub', '#1f2226');
    const cx = x + w / 2;
    const cy = y + h / 2;
    g.strokeStyle = '#3b4048';
    g.lineWidth = 3;
    g.beginPath();
    g.arc(cx, cy, w * 0.36, 0, Math.PI * 2);
    g.stroke();
    g.strokeStyle = '#2c3036';
    g.lineWidth = 2;
    g.beginPath();
    g.arc(cx, cy, w * 0.2, 0, Math.PI * 2);
    g.stroke();
    g.fillStyle = '#2a2e34';
    g.beginPath();
    g.arc(cx, cy, w * 0.06, 0, Math.PI * 2);
    g.fill();
  }
  // Generisk IC.
  {
    const [x, y, w, h] = region('ic', '#18191c');
    g.fillStyle = '#2a2c30';
    g.beginPath();
    g.arc(x + 20, y + 20, 8, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#74787f';
    g.font = `600 26px ${MONO}`;
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText('8+2', x + w / 2, y + h / 2 - 12);
    g.font = `500 18px ${MONO}`;
    g.fillText('2042A', x + w / 2, y + h / 2 + 20);
    g.textAlign = 'left';
  }
  // Pinnehode (vifte/LED): hvit plast med åpninger.
  {
    const [x, y, w, h] = region('hdr', '#e8e6df');
    g.fillStyle = '#3b3c3e';
    for (let i = 0; i < 4; i++) g.fillRect(x + 12 + i * 40, y + 18, 24, 28);
  }
  return c;
}

// ---------------------------------------------------------------------------
// Kretskortet: 2048 × 1024, dekker x 0–267 og y −9–111 mm.
// ---------------------------------------------------------------------------
const PW = 2048;
const PH = 1024;
const SX = PW / (PCB_TEX.x1 - PCB_TEX.x0);
const SY = PH / (PCB_TEX.y1 - PCB_TEX.y0);

const MASK = '#0e1f17';
const COPPER = '#15322a'; // kobber under masken (litt lysere)
const TRACE = 'rgba(46, 96, 72, 0.55)';
const SILK = 'rgba(226, 232, 226, 0.86)';
const PAD = '#b9b8ae';
const GOLD = '#c9a24e';

function mm(g, back = false) {
  // Millimeter med y opp. Baksiden tegnes speilvendt i x (sett bakfra).
  if (back) g.setTransform(-SX, 0, 0, -SY, PW, PCB_TEX.y1 * SY);
  else g.setTransform(SX, 0, 0, -SY, 0, PCB_TEX.y1 * SY);
}

/** Tekst i mm-koordinater (rett vei). */
function text(g, s, x, y, size, { align = 'left', color = SILK, font = SANS, weight = 600, back = false, rot = 0 } = {}) {
  g.save();
  g.translate(x, y);
  g.scale(back ? -1 : 1, -1);
  if (rot) g.rotate(rot);
  g.fillStyle = color;
  g.font = `${weight} ${size}px ${font}`;
  g.textAlign = align;
  g.textBaseline = 'middle';
  g.fillText(s, 0, 0);
  g.restore();
}

function silkRect(g, x, y, w, h, pin1 = true) {
  g.strokeStyle = SILK;
  g.lineWidth = 0.16;
  g.strokeRect(x - w / 2, y - h / 2, w, h);
  if (pin1) {
    g.fillStyle = SILK;
    g.beginPath();
    g.arc(x - w / 2 - 0.9, y + h / 2 + 0.6, 0.35, 0, Math.PI * 2);
    g.fill();
  }
}
function corners(g, x, y, w, h, l = 2) {
  g.strokeStyle = SILK;
  g.lineWidth = 0.16;
  const x0 = x - w / 2, x1 = x + w / 2, y0 = y - h / 2, y1 = y + h / 2;
  g.beginPath();
  for (const [cx, cy, dx, dy] of [
    [x0, y0, 1, 1],
    [x1, y0, -1, 1],
    [x0, y1, 1, -1],
    [x1, y1, -1, -1],
  ]) {
    g.moveTo(cx + dx * l, cy);
    g.lineTo(cx, cy);
    g.lineTo(cx, cy + dy * l);
  }
  g.stroke();
}

function baseBoard(g, R) {
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.fillStyle = MASK;
  g.fillRect(0, 0, PW, PH);
  // Matt struktur i masken.
  for (let i = 0; i < 26000; i++) {
    g.fillStyle = `rgba(${R() < 0.5 ? '255,255,255' : '0,0,0'},${0.012 + R() * 0.018})`;
    g.fillRect(R() * PW, R() * PH, 1.5, 1.5);
  }
}

function pours(g, back) {
  mm(g, back);
  g.fillStyle = COPPER;
  // Strømflater ved fasene og inngangen, og jord langs kantene.
  g.globalAlpha = 0.55;
  g.beginPath();
  g.moveTo(160, 4);
  g.lineTo(232, 4);
  g.lineTo(262, 34);
  g.lineTo(262, 96);
  g.lineTo(232, 108);
  g.lineTo(160, 108);
  g.closePath();
  g.fill();
  g.beginPath();
  g.rect(18, 70, 30, 34);
  g.fill();
  g.globalAlpha = 0.35;
  g.fillRect(1.2, 1.2, 264.6, 2.2);
  g.fillRect(1.2, 107.6, 264.6, 2.2);
  g.globalAlpha = 1;
  // Klaring (linjer) mellom flatene.
  g.strokeStyle = MASK;
  g.lineWidth = 0.5;
  g.beginPath();
  g.moveTo(200, 4);
  g.lineTo(200, 108);
  g.moveTo(160, 56);
  g.lineTo(200, 56);
  g.stroke();
}

function tracesTo(g, pts, n, spread, w = 0.16) {
  g.strokeStyle = TRACE;
  g.lineWidth = w;
  g.lineJoin = 'round';
  for (let k = 0; k < n; k++) {
    const o = (k - (n - 1) / 2) * spread;
    g.beginPath();
    pts.forEach(([x, y, ox, oy], i) => {
      const X = x + o * (ox ?? 0);
      const Y = y + o * (oy ?? 1);
      if (i === 0) g.moveTo(X, Y);
      else g.lineTo(X, Y);
    });
    g.stroke();
  }
}

function vias(g, R, x0, y0, x1, y1, n) {
  for (let i = 0; i < n; i++) {
    const x = x0 + R() * (x1 - x0);
    const y = y0 + R() * (y1 - y0);
    g.fillStyle = 'rgba(60,90,74,0.9)';
    g.beginPath();
    g.arc(x, y, 0.32, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#07100c';
    g.beginPath();
    g.arc(x, y, 0.14, 0, Math.PI * 2);
    g.fill();
  }
}

function holes(g) {
  for (const [x, y] of HOLES) {
    g.fillStyle = GOLD;
    g.beginPath();
    g.arc(x, y, 3.1, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#d9b968';
    g.beginPath();
    g.arc(x, y, 2.2, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#050706';
    g.beginPath();
    g.arc(x, y, 1.45, 0, Math.PI * 2);
    g.fill();
    // Små gjennomføringer rundt hullet.
    for (let i = 0; i < 8; i++) {
      const a = (i / 8) * Math.PI * 2;
      g.fillStyle = '#b08a3e';
      g.beginPath();
      g.arc(x + Math.cos(a) * 2.65, y + Math.sin(a) * 2.65, 0.22, 0, Math.PI * 2);
      g.fill();
    }
  }
}

export function makePcbFront() {
  const [c, g] = canvas(PW, PH);
  const R = rng(1234);
  baseBoard(g, R);
  pours(g, false);
  mm(g);

  // Minnebuss: bunter fra hver brikke inn til GPU-en.
  const gx0 = GPU.cx - GPU.sub / 2;
  const gx1 = GPU.cx + GPU.sub / 2;
  const gy1 = GPU.cy + GPU.sub / 2;
  for (const m of MEMORY) {
    if (m.r && m.x < GPU.cx) tracesTo(g, [[m.x + m.w / 2, m.y], [m.x + m.w / 2 + 4, m.y], [gx0 - 4, m.y + (GPU.cy - m.y) * 0.55], [gx0, m.y + (GPU.cy - m.y) * 0.55]], 22, 0.42);
    else if (m.r) tracesTo(g, [[m.x - m.w / 2, m.y], [m.x - m.w / 2 - 4, m.y], [gx1 + 4, m.y + (GPU.cy - m.y) * 0.55], [gx1, m.y + (GPU.cy - m.y) * 0.55]], 22, 0.42);
    else tracesTo(g, [[m.x, m.y - m.h / 2, 1, 0], [m.x, m.y - m.h / 2 - 3, 1, 0], [m.x + (GPU.cx - m.x) * 0.5, gy1 + 2, 1, 0], [m.x + (GPU.cx - m.x) * 0.5, gy1, 1, 0]], 22, 0.36);
  }
  // PCIe-linjer: differensialpar fra fingrene opp til GPU-en.
  for (let i = 0; i < 16; i++) {
    const fx = 66.8 + i * 4.2;
    const tx = GPU.cx - 15 + i * 2;
    tracesTo(g, [[fx, -1, 1, 0], [fx, 11, 1, 0], [fx, 15, 1, 0], [tx + (fx - tx) * 0.2, 26, 1, 0], [tx, GPU.cy - GPU.sub / 2, 1, 0]], 2, 0.34, 0.14);
  }
  // Skjermutganger: bunter fra utgangene bak minnet og inn til GPU-en.
  for (const [k, p] of PORTS.entries()) {
    const y = (p.y0 + p.y1) / 2;
    tracesTo(g, [[15, y], [24, y], [30, 104 - k * 2.4], [60, 104 - k * 2.4], [GPU.cx - 24, 104 - k * 2.4], [GPU.cx - 18, gy1]], 4, 0.3, 0.13);
  }
  // Strøm: brede ledere fra fasene inn mot GPU-en.
  g.strokeStyle = 'rgba(46, 96, 72, 0.35)';
  g.lineWidth = 3.2;
  for (const ch of CHOKES.slice(0, 8)) {
    g.beginPath();
    g.moveTo(ch.x - 6, ch.y);
    g.lineTo(162, ch.y + (GPU.cy - ch.y) * 0.25);
    g.stroke();
  }
  vias(g, R, 160, 4, 262, 108, 420);
  vias(g, R, 4, 4, 160, 108, 520);
  vias(g, R, GPU.cx - 24, GPU.cy - 24, GPU.cx + 24, GPU.cy + 24, 220);

  // Puter under komponentene (tinn), så endene synes rundt delene.
  g.fillStyle = PAD;
  for (const p of smallParts()) {
    if (p.w > p.h) {
      g.fillRect(p.x - p.w / 2 - 0.15, p.y - p.h / 2 - 0.1, p.w * 0.36, p.h + 0.2);
      g.fillRect(p.x + p.w / 2 - p.w * 0.36 + 0.15, p.y - p.h / 2 - 0.1, p.w * 0.36, p.h + 0.2);
    } else {
      g.fillRect(p.x - p.w / 2 - 0.1, p.y - p.h / 2 - 0.15, p.w + 0.2, p.h * 0.36);
      g.fillRect(p.x - p.w / 2 - 0.1, p.y + p.h / 2 - p.h * 0.36 + 0.15, p.w + 0.2, p.h * 0.36);
    }
  }
  for (const ch of CHOKES) {
    g.fillRect(ch.x - 5.6, ch.y - 4, 2.4, 8);
    g.fillRect(ch.x + 3.2, ch.y - 4, 2.4, 8);
  }
  for (const ps of POWERSTAGES) g.fillRect(ps.x - 3.1, ps.y - 2.6, 6.2, 5.2);
  for (const cp of CAPS) {
    g.fillRect(cp.x - 3.9, cp.y - 1, 1.6, 2);
    g.fillRect(cp.x + 2.3, cp.y - 1, 1.6, 2);
  }
  // Kontaktfingrene (gull) og fasingen langs PCIe-kanten.
  g.fillStyle = GOLD;
  for (let x = FINGERS.tabA[0] + 1.4; x < FINGERS.tabB[1] - 1.3; x += FINGERS.pitch) {
    if (x > FINGERS.tabA[1] - 0.6 && x < FINGERS.tabB[0] + 0.7) continue;
    g.fillRect(x - FINGERS.w / 2, FINGERS.y0 + 0.5, FINGERS.w, 7.1);
  }
  holes(g);

  // Silketrykk.
  silkRect(g, GPU.cx, GPU.cy, GPU.sub + 1.4, GPU.sub + 1.4);
  text(g, 'U1', GPU.cx - GPU.sub / 2 - 0.6, GPU.cy - GPU.sub / 2 - 2.2, 2.2, { align: 'left' });
  for (const m of MEMORY) {
    corners(g, m.x, m.y, m.w + 1.2, m.h + 1.2, 1.8);
    text(g, m.ref, m.r ? m.x + (m.x < GPU.cx ? -m.w / 2 - 2.6 : m.w / 2 + 2.6) : m.x, m.r ? m.y : m.y + m.h / 2 + 1.9, 1.7, { align: 'center' });
  }
  for (const ch of CHOKES) {
    silkRect(g, ch.x, ch.y, 11.2, 10.4, false);
    text(g, ch.ref, ch.x + (ch.x > 100 ? 7.6 : -7.4), ch.y + 4.3, 1.5, { align: 'center' });
  }
  for (const cp of CAPS) {
    g.strokeStyle = SILK;
    g.lineWidth = 0.16;
    g.beginPath();
    g.arc(cp.x, cp.y, cp.r + 0.6, 0, Math.PI * 2);
    g.stroke();
    g.fillStyle = SILK;
    g.beginPath();
    g.moveTo(cp.x, cp.y);
    g.arc(cp.x, cp.y, cp.r + 0.6, Math.PI * 0.75, Math.PI * 1.25);
    g.closePath();
    g.globalAlpha = 0.35;
    g.fill();
    g.globalAlpha = 1;
  }
  for (const p of POWER) {
    silkRect(g, (p.x0 + p.x1) / 2, (p.y0 + 111) / 2, p.x1 - p.x0 + 1.2, 111 - p.y0 + 0.6, false);
    text(g, p.label, (p.x0 + p.x1) / 2, p.y0 - 2.4, 2.0, { align: 'center' });
    text(g, p.ref, p.x0 - 0.6, p.y0 - 2.4, 1.5, { align: 'right' });
  }
  for (const ic of ICS) {
    silkRect(g, ic.x, ic.y, ic.w + 1.2, ic.h + 1.2);
    text(g, ic.ref, ic.x + ic.w / 2 + 1.2, ic.y + ic.h / 2 + 1, 1.4);
  }
  for (const h of HEADERS) {
    silkRect(g, h.x, h.y, h.w + 1, h.h + 1, false);
    text(g, h.label, h.x - h.w / 2 - 1, h.y, 1.6, { align: 'right' });
  }
  for (const p of PORTS) text(g, p.label, 17.6, (p.y0 + p.y1) / 2, 1.6, { rot: -Math.PI / 2, align: 'center' });
  // Kortets ID og en strek med pinnetall for kontakten.
  text(g, 'AR-2080 · REV A', 246, 54, 2.4, { align: 'center', weight: 700 });
  text(g, 'PCB 12L · 94V-0 · 2042', 246, 50.4, 1.6, { align: 'center', weight: 500 });
  text(g, 'PCIe x16', 100, 2.8, 1.6, { align: 'center', weight: 600 });
  text(g, '1', FINGERS.tabA[0] + 1.6, 1.4, 1.2, { align: 'center' });
  text(g, '82', FINGERS.tabB[1] - 1.6, 1.4, 1.2, { align: 'center' });
  // Tynn ramme langs kanten.
  g.strokeStyle = 'rgba(226, 232, 226, 0.22)';
  g.lineWidth = 0.18;
  g.strokeRect(1.4, 1.4, 264.2, 108.2);
  return c;
}

export function makePcbBack() {
  const [c, g] = canvas(PW, PH);
  const R = rng(4321);
  baseBoard(g, R);
  pours(g, true);
  mm(g, true);
  // Ledere på baksiden: mer synlige, spredt i lag.
  g.strokeStyle = TRACE;
  g.lineWidth = 0.16;
  for (let i = 0; i < 46; i++) {
    const x = 20 + R() * 230;
    const y = 6 + R() * 98;
    const len = 10 + R() * 40;
    const dx = R() < 0.5 ? 1 : -1;
    const dy = R() < 0.5 ? 1 : -1;
    for (let k = 0; k < 3 + ((R() * 6) | 0); k++) {
      const o = k * 0.5;
      g.beginPath();
      g.moveTo(x, y + o);
      g.lineTo(x + dx * len * 0.6, y + o);
      g.lineTo(x + dx * len * 0.6 + dy * 6, y + o + dy * 6);
      g.lineTo(x + dx * len, y + o + dy * 6);
      g.stroke();
    }
  }
  for (let i = 0; i < 16; i++) {
    const fx = 66.8 + i * 4.2;
    g.beginPath();
    g.moveTo(fx, -1);
    g.lineTo(fx, 10);
    g.stroke();
  }
  vias(g, R, 4, 4, 262, 108, 1300);
  vias(g, R, GPU.cx - 22, GPU.cy - 22, GPU.cx + 22, GPU.cy + 22, 380);
  // Fotavtrykk for MLCC-rutenettet bak GPU-en.
  g.fillStyle = PAD;
  for (const p of backCaps()) {
    g.fillRect(p.x - p.w / 2 - 0.15, p.y - p.h / 2 - 0.1, p.w * 0.36, p.h + 0.2);
    g.fillRect(p.x + p.w / 2 - p.w * 0.36 + 0.15, p.y - p.h / 2 - 0.1, p.w * 0.36, p.h + 0.2);
  }
  silkRect(g, GPU.cx, GPU.cy, 30, 30, false);
  g.fillStyle = GOLD;
  for (let x = FINGERS.tabA[0] + 1.4; x < FINGERS.tabB[1] - 1.3; x += FINGERS.pitch) {
    if (x > FINGERS.tabA[1] - 0.6 && x < FINGERS.tabB[0] + 0.7) continue;
    g.fillRect(x - FINGERS.w / 2, FINGERS.y0 + 0.5, FINGERS.w, 7.1);
  }
  holes(g);
  // Etikett med strekkode (generisk).
  g.fillStyle = '#e9e7e1';
  g.fillRect(200, 72, 42, 18);
  g.fillStyle = '#1b1c1e';
  let bx = 203;
  while (bx < 238) {
    const w = 0.25 + R() * 0.7;
    g.fillRect(bx, 79, w, 9);
    bx += w + 0.25 + R() * 0.6;
  }
  text(g, 'AR-2080-8G · SIMULERT', 221, 76.3, 2.0, { align: 'center', color: '#1b1c1e', back: true, font: MONO, weight: 600 });
  text(g, 'S/N 0000-0000-0001', 221, 73.6, 1.6, { align: 'center', color: '#3b3c3e', back: true, font: MONO, weight: 500 });
  text(g, 'AR-2080 · REV A', 60, 56, 2.4, { align: 'center', back: true, weight: 700 });
  for (const [k, p] of POWER.entries()) text(g, k ? '+12V 6P' : '+12V 8P', (p.x0 + p.x1) / 2, 104, 1.6, { align: 'center', back: true });
  g.strokeStyle = 'rgba(226, 232, 226, 0.22)';
  g.lineWidth = 0.18;
  g.strokeRect(1.4, 1.4, 264.2, 108.2);
  return c;
}

// ---------------------------------------------------------------------------
// Braketten: 512 × 1024, dekker z (bredde) og y (høyde). Åpninger i alfa.
// ---------------------------------------------------------------------------
export const BRACKET_TEX = { w: 512, h: 1024 };
export function bracketUv(y, z) {
  return [(z - BRACKET.z0) / (BRACKET.z1 - BRACKET.z0), (y - BRACKET.y0) / (BRACKET.y1 - BRACKET.y0)];
}
export function makeBracket() {
  const { w, h } = BRACKET_TEX;
  const [c, g] = canvas(w, h);
  const sx = w / (BRACKET.z1 - BRACKET.z0);
  const sy = h / (BRACKET.y1 - BRACKET.y0);
  g.fillStyle = '#ffffff';
  g.fillRect(0, 0, w, h);
  // Børstet stål (lys variasjon langs høyden).
  const R = rng(99);
  for (let i = 0; i < 3000; i++) {
    g.fillStyle = `rgba(0,0,0,${0.02 + R() * 0.04})`;
    g.fillRect(R() * w, R() * h, 1, 6 + R() * 30);
  }
  g.setTransform(sx, 0, 0, -sy, -BRACKET.z0 * sx, BRACKET.y1 * sy);
  const cut = (y0, y1, z0, z1, r = 0.6) => {
    g.save();
    g.globalCompositeOperation = 'destination-out';
    g.fillStyle = '#000'; // heldekkende, ellers fjernes bare litt av alfaen
    g.beginPath();
    g.roundRect(z0, y0, z1 - z0, y1 - y0, r);
    g.fill();
    g.restore();
  };
  // Utgangene, med litt klaring rundt skallet.
  for (const p of PORTS) cut(p.y0 - 0.7, p.y1 + 0.7, p.z0 - 0.7, p.z1 + 0.7, p.r + 0.6);
  // Luftspalter i det øvre sporet.
  for (let j = 0; j < 15; j++) {
    for (let i = 0; i < 3; i++) {
      const z0 = 11 + i * 7.6;
      const y0 = 6 + j * 6.6;
      cut(y0, y0 + 4, z0, z0 + 6, 1.4);
    }
  }
  return c;
}

// ---------------------------------------------------------------------------
// Bakplaten: 2048 × 1024, x 0,5–270, y 1–110. Sett bakfra (speilvendt i x).
// ---------------------------------------------------------------------------
export function backplateUv(x, y) {
  const B = BACKPLATE;
  return [1 - (x - B.x0) / (B.x1 - B.x0), (y - B.y0) / (B.y1 - B.y0)];
}
export function makeBackplate() {
  const W = 2048;
  const H = 1024;
  const [c, g] = canvas(W, H);
  const B = BACKPLATE;
  const sx = W / (B.x1 - B.x0);
  const sy = H / (B.y1 - B.y0);
  g.fillStyle = '#ffffff';
  g.fillRect(0, 0, W, H);
  // Børstet aluminium langs kortet.
  const R = rng(55);
  for (let i = 0; i < 9000; i++) {
    const v = R() < 0.5 ? 0 : 255;
    g.fillStyle = `rgba(${v},${v},${v},${0.02 + R() * 0.035})`;
    g.fillRect(R() * W, R() * H, 20 + R() * 120, 1);
  }
  // Speilvendt: x måles fra høyre, så det ser riktig ut sett bakfra.
  g.setTransform(-sx, 0, 0, -sy, B.x1 * sx, B.y1 * sy);
  // Fresede linjer (mørkere riller) i et nøkternt mønster.
  g.strokeStyle = 'rgba(0,0,0,0.38)';
  g.lineWidth = 0.6;
  g.beginPath();
  g.moveTo(170, 8);
  g.lineTo(250, 8);
  g.lineTo(262, 20);
  g.lineTo(262, 92);
  g.lineTo(250, 104);
  g.lineTo(170, 104);
  g.moveTo(150, 14);
  g.lineTo(150, 98);
  g.moveTo(40, 104);
  g.lineTo(14, 104);
  g.lineTo(6, 96);
  g.lineTo(6, 16);
  g.lineTo(14, 8);
  g.lineTo(40, 8);
  g.stroke();
  g.strokeStyle = 'rgba(255,255,255,0.10)';
  g.lineWidth = 0.3;
  g.beginPath();
  g.moveTo(170, 8.6);
  g.lineTo(250, 8.6);
  g.moveTo(150.6, 14);
  g.lineTo(150.6, 98);
  g.stroke();
  const cut = (x0, y0, x1, y1, r) => {
    g.save();
    g.globalCompositeOperation = 'destination-out';
    g.fillStyle = '#000'; // heldekkende, ellers fjernes bare litt av alfaen
    g.beginPath();
    g.roundRect(x0, y0, x1 - x0, y1 - y0, r);
    g.fill();
    g.restore();
  };
  // Vindu bak GPU-en (festekrysset og MLCC-ene synes) og luftspalter ved fasene.
  const wdw = B.window;
  // Mørk fas rundt vinduet.
  g.strokeStyle = 'rgba(0,0,0,0.5)';
  g.lineWidth = 1.6;
  g.beginPath();
  g.roundRect(wdw.x0, wdw.y0, wdw.x1 - wdw.x0, wdw.y1 - wdw.y0, 4);
  g.stroke();
  cut(wdw.x0, wdw.y0, wdw.x1, wdw.y1, 4);
  for (let i = 0; i < 9; i++) {
    const x = 176 + i * 8.6;
    g.strokeStyle = 'rgba(0,0,0,0.45)';
    g.lineWidth = 1.2;
    g.beginPath();
    g.roundRect(x, 30, 3.2, 52, 1.6);
    g.stroke();
    cut(x, 30, x + 3.2, 82, 1.6);
  }
  return c;
}

// ---------------------------------------------------------------------------
// Kjøleribben på avstand: én periode (lamell + mellomrom) per tekstur, gjentatt
// langs kortet. Mipmapene jevner stripene ut når de blir mindre enn en piksel,
// så det ikke blir moaré. Brukes når lamellene er under ~3 piksler.
// ---------------------------------------------------------------------------
export function makeFinStripe(fill = 0.17) {
  const [c, g] = canvas(64, 8);
  g.fillStyle = '#26292d';
  g.fillRect(0, 0, 64, 8);
  const w = Math.max(2, Math.round(64 * fill));
  g.fillStyle = '#dfe3e8';
  g.fillRect(0, 0, w, 8);
  g.fillStyle = '#9ea4ab';
  g.fillRect(w, 0, 2, 8);
  return c;
}
