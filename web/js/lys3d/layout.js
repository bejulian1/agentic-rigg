// Oppsettet av kortet i millimeter: ett sted for plasseringene, så
// teksturene (silketrykk, puter, ledere) og geometrien alltid stemmer.
//
// Koordinater: x langs kortet fra braketten (0) mot enden, y opp fra
// PCIe-kanten (0) mot toppkanten, z ut av kretskortets komponentside (mot
// viftene). Kretskortet ligger fra z = −1,6 til 0.
//
// Generisk kort i RTX 2080-størrelse: TU104-klasse GPU (545 mm²), 8 × GDDR6,
// 8 + 2 strømfaser, 8- og 6-pins strøm, 3 × DP, HDMI og USB-C, to vifter.
// Ingen produsentlogoer eller varemerker.

export const PCB = { x0: 0, x1: 267, y0: 0, y1: 111, t: 1.6 };
/** Teksturene for kretskortet dekker y fra −9 (under kontaktfingrene) til 111. */
export const PCB_TEX = { x0: 0, x1: 267, y0: -9, y1: 111 };

export const SHROUD = { x0: 2, x1: 292, y0: 2, y1: 113, zFront: 46, zChamfer: 41, zBack: 33.5 };
export const FANS = [
  { id: 'fan1', cx: 77, cy: 57.5 },
  { id: 'fan2', cx: 217, cy: 57.5 },
];
export const FAN = { rHole: 46.5, rRotor: 44.6, rHub: 15.5, zMid: 40.5, blades: 11 };
export const RING = { r0: 48.4, r1: 50.8, leds: 18 };
export const BAR = { leds: 20, x0: 22, x1: 272 };

export const GPU = { cx: 112, cy: 60, sub: 40, dieW: 25.4, dieH: 21.5 };
export const FINGERS = { tabA: [48, 61.6], tabB: [63.4, 138], y0: -8.2, pitch: 1, w: 0.68 };

// GDDR6, 14 × 12 mm. r: rotert (14 mm langs y).
export const MEMORY = [
  { x: 72, y: 34, r: true },
  { x: 72, y: 60, r: true },
  { x: 72, y: 86, r: true },
  { x: 101, y: 95.5, r: false },
  { x: 123, y: 95.5, r: false },
  { x: 152, y: 34, r: true },
  { x: 152, y: 60, r: true },
  { x: 152, y: 86, r: true },
].map((m, i) => ({ ...m, w: m.r ? 12 : 14, h: m.r ? 14 : 12, ref: `M${i + 1}` }));

// Strømfaser: 8 for kjernen i en kolonne til høyre, 2 for minnet oppe til venstre.
export const CHOKES = [
  ...Array.from({ length: 8 }, (_, i) => ({ x: 181, y: 14 + i * 11.4, ref: `L${i + 1}` })),
  { x: 40, y: 92, ref: 'L9' },
  { x: 40, y: 80.5, ref: 'L10' },
];
export const POWERSTAGES = CHOKES.map((c, i) => ({ x: i < 8 ? 168.5 : 28.5, y: c.y, ref: `Q${i + 1}` }));
// Polymerkondensatorer (Ø 6,3 mm): bak spolene og ved strøminngangen.
export const CAPS = [
  ...Array.from({ length: 8 }, (_, i) => ({ x: 195, y: 14 + i * 11.4, r: 3.15, h: 5.8, ref: `C${101 + i}` })),
  ...[
    [212, 80],
    [212, 92],
    [222, 86],
    [52, 92],
    [52, 80.5],
  ].map(([x, y], i) => ({ x, y, r: 3.15, h: 5.8, ref: `C${121 + i}` })),
];
export const POWER = [
  { id: '8pin', x0: 224, x1: 245.5, pins: 8, label: '8-PIN', ref: 'J2' },
  { id: '6pin', x0: 249.5, x1: 264.1, pins: 6, label: '6-PIN', ref: 'J3' },
].map((p) => ({ ...p, y0: 98.5, y1: 112.4, z1: 9.8 }));

// Utganger på braketten (y og z for åpningen; x går fra −3 til 14).
export const PORTS = [
  { id: 'usbc', label: 'USB-C', y0: 8, y1: 16.6, z0: 0.6, z1: 3.4, r: 1.3 },
  { id: 'hdmi', label: 'HDMI', y0: 22, y1: 37, z0: 0.5, z1: 6.1, r: 0.6 },
  { id: 'dp1', label: 'DP', y0: 43, y1: 59, z0: 0.5, z1: 6.6, r: 0.6 },
  { id: 'dp2', label: 'DP', y0: 64, y1: 80, z0: 0.5, z1: 6.6, r: 0.6 },
  { id: 'dp3', label: 'DP', y0: 85, y1: 101, z0: 0.5, z1: 6.6, r: 0.6 },
];
export const BRACKET = { x0: -2.2, x1: -1.4, y0: -12, y1: 118, z0: -6.5, z1: 34.3 };

export const HOLES = [
  [GPU.cx - 26.5, GPU.cy - 26.5],
  [GPU.cx + 26.5, GPU.cy - 26.5],
  [GPU.cx - 26.5, GPU.cy + 26.5],
  [GPU.cx + 26.5, GPU.cy + 26.5],
  [8, 104],
  [8, 7],
  [205, 104],
  [205, 6],
  [261, 7],
];

export const ICS = [
  { x: 214, y: 34, w: 6, h: 6, ref: 'U3', label: 'PWM' },
  { x: 214, y: 14, w: 5, h: 4, ref: 'U12', label: 'BIOS' },
  { x: 36, y: 64, w: 4, h: 4, ref: 'U5', label: 'PWM' },
  { x: 240, y: 30, w: 3.5, h: 3.5, ref: 'U7', label: 'LED' },
];
export const HEADERS = [
  { x: 254, y: 12, w: 8.4, h: 3.2, pins: 4, ref: 'J5', label: 'FAN' },
  { x: 254, y: 21, w: 8.4, h: 3.2, pins: 4, ref: 'J6', label: 'LED' },
];

/** Kjøleren. */
export const COOLER = {
  plate: { x0: GPU.cx - 18, x1: GPU.cx + 18, y0: GPU.cy - 18, y1: GPU.cy + 18, z0: 2.45, z1: 8.0 },
  fins: { x0: 8, x1: 288, y0: 6, y1: 106, z0: 10, z1: 33, pitch: 2.5, t: 0.42 },
  pipes: [30, 45, 60, 75, 90].map((y, i) => ({ y, r: 3, z: 11, bendLeft: i % 2 === 0 })),
  vrm: { x0: 174.5, x1: 201.5, y0: 8, y1: 104, z0: 6.9, z1: 9.6 },
};

/** Bakplaten. */
export const BACKPLATE = { x0: 0.5, x1: 270, y0: 1, y1: 110, z0: -4.2, z1: -2.6, window: { x0: GPU.cx - 31, x1: GPU.cx + 31, y0: GPU.cy - 31, y1: GPU.cy + 31 } };

/** Små komponenter (0402/0603) med en fast tilfeldighet, så teksturen og geometrien er like hver gang. */
export function smallParts() {
  let s = 20801;
  const rnd = () => {
    s = (s * 16807) % 2147483647;
    return (s - 1) / 2147483646;
  };
  const out = [];
  const blocked = (x, y, w, h) => {
    const r = [x - w / 2 - 0.6, y - h / 2 - 0.6, x + w / 2 + 0.6, y + h / 2 + 0.6];
    const hit = (a) => r[0] < a[2] && r[2] > a[0] && r[1] < a[3] && r[3] > a[1];
    if (hit([GPU.cx - GPU.sub / 2 - 0.5, GPU.cy - GPU.sub / 2 - 0.5, GPU.cx + GPU.sub / 2 + 0.5, GPU.cy + GPU.sub / 2 + 0.5])) return true;
    for (const m of MEMORY) if (hit([m.x - m.w / 2 - 1, m.y - m.h / 2 - 1, m.x + m.w / 2 + 1, m.y + m.h / 2 + 1])) return true;
    for (const c of CHOKES) if (hit([c.x - 6, c.y - 6, c.x + 6, c.y + 6])) return true;
    for (const c of POWERSTAGES) if (hit([c.x - 3.5, c.y - 3.5, c.x + 3.5, c.y + 3.5])) return true;
    for (const c of CAPS) if (hit([c.x - 4, c.y - 4, c.x + 4, c.y + 4])) return true;
    for (const p of POWER) if (hit([p.x0 - 1, p.y0 - 1, p.x1 + 1, p.y1])) return true;
    for (const h of HOLES) if (hit([h[0] - 4, h[1] - 4, h[0] + 4, h[1] + 4])) return true;
    for (const i of [...ICS, ...HEADERS]) if (hit([i.x - i.w / 2 - 1.2, i.y - i.h / 2 - 1.2, i.x + i.w / 2 + 1.2, i.y + i.h / 2 + 1.2])) return true;
    if (hit([-3, 3, 16, 104])) return true; // utgangene
    if (hit([0, 0, 267, 1.8]) || hit([0, 109.2, 267, 111]) || hit([265, 0, 267, 111])) return true; // kanten
    for (const o of out) if (hit([o.x - o.w / 2, o.y - o.h / 2, o.x + o.w / 2, o.y + o.h / 2])) return true;
    return false;
  };
  const place = (x, y, big, rot, kind) => {
    const w = big ? 1.6 : 1.0;
    const h = big ? 0.8 : 0.5;
    const [W, H] = rot ? [h, w] : [w, h];
    if (blocked(x, y, W, H)) return false;
    out.push({ x, y, w: W, h: H, t: big ? 0.5 : 0.35, kind });
    return true;
  };
  // Ring av avkoblingskondensatorer rundt GPU-en og minnet.
  for (let i = 0; i < 64; i++) {
    const a = (i / 64) * Math.PI * 2;
    const R = 24 + (i % 2) * 2.2;
    const x = GPU.cx + Math.cos(a) * R * 1.02;
    const y = GPU.cy + Math.sin(a) * R;
    place(x, y, i % 3 === 0, Math.abs(Math.cos(a)) > 0.7, i % 5 === 0 ? 'res' : 'cap');
  }
  for (const m of MEMORY) {
    for (let k = 0; k < 6; k++) {
      const side = k % 2 ? 1 : -1;
      if (m.r) place(m.x + side * (m.w / 2 + 2.2), m.y - 4.5 + (k >> 1) * 4.5, false, true, 'cap');
      else place(m.x - 4.5 + (k >> 1) * 4.5, m.y + side * (m.h / 2 + 2.2), false, false, 'cap');
    }
  }
  // Par av AC-kondensatorer på PCIe-linjene.
  for (let i = 0; i < 16; i++) {
    place(66 + i * 4.2, 13, false, true, 'cap');
    place(67.6 + i * 4.2, 13, false, true, 'cap');
  }
  // Spredt over resten av kortet, tettere ved strømfasene.
  for (let i = 0; i < 1400 && out.length < 430; i++) {
    const x = 4 + rnd() * 259;
    const y = 3 + rnd() * 104;
    const nearVrm = x > 160 && x < 236;
    if (!nearVrm && rnd() < 0.45) continue;
    place(x, y, rnd() < 0.35, rnd() < 0.5, rnd() < 0.28 ? 'res' : 'cap');
  }
  return out;
}

/** MLCC-rutenettet bak GPU-en (baksiden). */
export function backCaps() {
  const out = [];
  for (let j = 0; j < 7; j++) {
    for (let i = 0; i < 7; i++) {
      if ((i === 3 && j === 3) || ((i === 0 || i === 6) && (j === 0 || j === 6))) continue;
      out.push({ x: GPU.cx - 10.5 + i * 3.5, y: GPU.cy - 10.5 + j * 3.5, w: 1.6, h: 0.8, t: 0.5 });
    }
  }
  for (let i = 0; i < 8; i++) out.push({ x: GPU.cx - 14 + i * 4, y: GPU.cy + 15.5, w: 1.0, h: 0.5, t: 0.35 });
  for (let i = 0; i < 8; i++) out.push({ x: GPU.cx - 14 + i * 4, y: GPU.cy - 15.5, w: 1.0, h: 0.5, t: 0.35 });
  return out;
}

/** Lysdiodene i samme rekkefølge og med samme u som web/js/rgb.js: lyslist (20), vifte 1 (18), vifte 2 (18). */
export function ledLayout() {
  const leds = [];
  for (let i = 0; i < BAR.leds; i++) leds.push({ zone: 'bar', i, u: i / (BAR.leds - 1) });
  for (const [fi, f] of FANS.entries()) {
    for (let i = 0; i < RING.leds; i++) {
      // rgb.js: a0 = i/18·2π − π/2 + 0,03 i SVG (y ned, med klokka). Her er y opp, så vinkelen speiles.
      const a0 = (i / RING.leds) * Math.PI * 2 - Math.PI / 2 + 0.03;
      const a1 = ((i + 1) / RING.leds) * Math.PI * 2 - Math.PI / 2 - 0.03;
      leds.push({ zone: f.id, i, u: (fi === 0 ? 0 : 0.5) + i / RING.leds / 2, a0: -a1, a1: -a0, cx: f.cx, cy: f.cy });
    }
  }
  return leds;
}
export const ZONE_RANGE = { bar: [0, 20], fan1: [20, 38], fan2: [38, 56] };
