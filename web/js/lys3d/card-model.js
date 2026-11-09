// Bygger skjermkortet: geometri, materialer, lag og LED-plasseringer.
// Alt i kortets koordinater (mm, se layout.js). Visningen (view.js) flytter
// lagene og sentrerer kortet.

import { MeshBuilder } from './gl/mesh.js';
import { mat4, hexToLinear, DEG } from './gl/math.js';
import * as Lo from './layout.js';
import { atlasUv, atlasPoint, bracketUv, backplateUv } from './textures.js';

const { PCB, PCB_TEX, SHROUD, FANS, FAN, RING, BAR, GPU, FINGERS, MEMORY, CHOKES, POWERSTAGES, CAPS, POWER, PORTS, BRACKET, HOLES, ICS, HEADERS, COOLER, BACKPLATE } = Lo;

/**
 * Lagene, i rekkefølge forfra. dir: retningen laget løftes av i (kortets
 * koordinater). spread: hvor langt laget flyttes ved full eksplodert visning.
 * step: fra hvilket trinn laget er tatt av (0 Hel, 1 Uten deksel, 2 Uten kjøler, 3 Bare kretskort).
 */
export const LAYERS = [
  { id: 'deksel', name: 'Deksel med lys', dir: [0, 0, 1], spread: 124, step: 1 },
  { id: 'vifter', name: 'Vifter', dir: [0, 0, 1], spread: 84, step: 1 },
  { id: 'kjoler', name: 'Kjøler', dir: [0, 0, 1], spread: 44, step: 2 },
  { id: 'kretskort', name: 'Kretskort', dir: [0, 0, 0], spread: 0, step: 99 },
  { id: 'kryss', name: 'Festekryss', dir: [0, 0, -1], spread: 16, step: 2 },
  { id: 'brakett', name: 'Brakett', dir: [-1, 0, 0], spread: 34, step: 3 },
  { id: 'bakplate', name: 'Bakplate', dir: [0, 0, -1], spread: 36, step: 3 },
];

/** Delene som kan pekes på. Navn og kort beskrivelse vises ved peking og i dellisten. */
export const GROUPS = {
  shroud: ['Deksel', 'Plast, med lyslist og to lysringer'],
  bar: ['Lyslist', 'Sone «Lyslist», 20 lysdioder', 'bar'],
  ring1: ['Lysring, vifte 1', 'Sone «Vifte 1», 18 lysdioder', 'fan1'],
  ring2: ['Lysring, vifte 2', 'Sone «Vifte 2», 18 lysdioder', 'fan2'],
  fan1: ['Vifte 1', '90 mm, 11 blader'],
  fan2: ['Vifte 2', '90 mm, 11 blader'],
  fins: ['Kjøleribbe', 'Lameller i aluminium'],
  pipes: ['Varmerør', '5 × Ø 6 mm, kobber'],
  plate: ['Kontaktplate', 'Forniklet kobber over GPU-en'],
  midplate: ['Kjøleplate', 'Over minnet'],
  vrmsink: ['VRM-kjøler', 'Over spolene og effekttrinnene'],
  pads: ['Termiske puter', 'Leder varme fra minnet og effekttrinnene'],
  xbracket: ['Festekryss', 'Fjærstål, holder kjøleren mot GPU-en'],
  gpu: ['GPU-brikke', 'TU104-klasse, 545 mm², 12 nm'],
  memory: ['GDDR6-minne', '8 × 1 GB, 256 bit, 14 Gbps'],
  chokes: ['Spoler', 'Strømfaser: 8 for kjernen, 2 for minnet'],
  stages: ['Effekttrinn', 'Én MOSFET-pakke per fase'],
  caps: ['Polymerkondensatorer', 'Glatter spenningen fra fasene'],
  smd: ['Små komponenter', 'Kondensatorer og motstander (0402 og 0603)'],
  ics: ['Kontrollere', 'Spenningsregulering, BIOS og lys'],
  pcie: ['PCIe x16-kontakt', 'Gullbelagte kontakter, 82 per side'],
  power: ['Strømkontakter', '8-pin og 6-pin, 12 V'],
  ports: ['Utganger', '3 × DisplayPort, HDMI og USB-C'],
  headers: ['Pinnehoder', 'Vifter og lys'],
  pcb: ['Kretskort', '267 × 111 mm'],
  backcaps: ['Kondensatorer bak GPU-en', 'Keramiske (MLCC)'],
  bracket: ['Brakett', 'Stål, to spor'],
  backplate: ['Bakplate', 'Aluminium, med vindu bak GPU-en'],
};
export const GROUP_IDS = Object.keys(GROUPS);

const SQ = Math.SQRT1_2;

export function buildCard(tex) {
  const hex = hexToLinear;
  const atlasMat = (rough, metal) => ({ color: [1, 1, 1], rough, metal, map: tex.atlas });
  const M = {
    pcbFront: { color: [1, 1, 1], rough: 0.4, metal: 0, map: tex.pcbFront },
    pcbBack: { color: [1, 1, 1], rough: 0.4, metal: 0, map: tex.pcbBack },
    pcbEdge: { color: hex('#3d4934'), rough: 0.8, metal: 0 },
    gold: { color: hex('#e0b25c'), rough: 0.24, metal: 1 },
    tin: { color: hex('#c9cbcd'), rough: 0.35, metal: 1 },
    mem: atlasMat(0.55, 0),
    die: atlasMat(0.1, 0.5),
    sub: atlasMat(0.45, 0),
    choke: atlasMat(0.5, 0.3),
    ps: atlasMat(0.5, 0),
    cap: atlasMat(0.3, 1),
    ic: atlasMat(0.5, 0),
    hdr: atlasMat(0.6, 0),
    hub: atlasMat(0.5, 0),
    bga: { color: hex('#08090a'), rough: 0.9, metal: 0 },
    smdCap: { color: hex('#9d8460'), rough: 0.6, metal: 0 },
    smdRes: { color: hex('#141415'), rough: 0.5, metal: 0 },
    plastic: { color: hex('#141518'), rough: 0.55, metal: 0 },
    hole: { color: hex('#040404'), rough: 1, metal: 0 },
    portMetal: { color: hex('#c6cad0'), rough: 0.3, metal: 1 },
    bracket: { color: hex('#b3b8be'), rough: 0.36, metal: 1, map: tex.bracket, alphaTest: 0.5, double: true },
    bracketEdge: { color: hex('#9ea3a9'), rough: 0.4, metal: 1 },
    backplate: { color: hex('#2b2e33'), rough: 0.42, metal: 0.6, map: tex.backplate, alphaTest: 0.5, double: true },
    backplateEdge: { color: hex('#3a3e44'), rough: 0.35, metal: 0.7 },
    screw: { color: hex('#8e949b'), rough: 0.32, metal: 1 },
    pad: { color: hex('#7d8996'), rough: 0.9, metal: 0 },
    nickel: { color: hex('#d0cdc7'), rough: 0.16, metal: 1 },
    midplate: { color: hex('#1d1f23'), rough: 0.5, metal: 0.4 },
    alu: { color: hex('#c6cbd1'), rough: 0.34, metal: 1 },
    finAlu: { color: hex('#c9ced4'), rough: 0.4, metal: 0.72 },
    finBlock: { color: [1, 1, 1], rough: 0.5, metal: 0.65, map: tex.finStripe },
    copper: { color: hex('#c47e52'), rough: 0.26, metal: 1 },
    xbr: { color: hex('#17181b'), rough: 0.42, metal: 0.5 },
    shroud: { color: hex('#272a30'), rough: 0.6, metal: 0, double: true, holes: new Float32Array([FANS[0].cx, FANS[0].cy, FAN.rHole, 1, FANS[1].cx, FANS[1].cy, FAN.rHole, 1]) },
    skirt: { color: hex('#272a30'), rough: 0.6, metal: 0, double: true },
    trim: { color: hex('#101114'), rough: 0.2, metal: 0 },
    duct: { color: hex('#16171a'), rough: 0.7, metal: 0, double: true },
    blade: { color: hex('#1d1f24'), rough: 0.42, metal: 0, double: true },
    hubSide: { color: hex('#202328'), rough: 0.45, metal: 0 },
    motor: { color: hex('#141518'), rough: 0.6, metal: 0 },
    led: { color: hex('#30353c'), rough: 0.32, metal: 0, led: true },
  };

  const parts = [];
  /** Lager en del av en bygger-funksjon. */
  const part = (group, layer, mat, fn, opts = {}) => {
    const b = new MeshBuilder();
    if (opts.uv) b.defaultUv = opts.uv;
    fn(b);
    if (!b.count) return null;
    const p = { group, layer, mat, data: b.build(), cast: opts.cast !== false, rotor: opts.rotor ?? -1, lod: opts.lod || null };
    parts.push(p);
    return p;
  };
  const T = (x, y, z) => mat4.translation(x, y, z);
  const RZ = (a) => mat4.rotZ(a);

  // ------------------------------------------------------------ Kretskort
  const texUv = (x, y, back = false) => {
    const u = (x - PCB_TEX.x0) / (PCB_TEX.x1 - PCB_TEX.x0);
    return [back ? 1 - u : u, (y - PCB_TEX.y0) / (PCB_TEX.y1 - PCB_TEX.y0)];
  };
  const boardRects = [
    [PCB.x0, PCB.y0, PCB.x1, PCB.y1],
    [FINGERS.tabA[0], FINGERS.y0, FINGERS.tabA[1], 0],
    [FINGERS.tabB[0], FINGERS.y0, FINGERS.tabB[1], 0],
  ];
  part('pcb', 'kretskort', M.pcbFront, (b) => {
    for (const [x0, y0, x1, y1] of boardRects) b.quad([x0, y0, 0], [x1, y0, 0], [x1, y1, 0], [x0, y1, 0], [0, 0, 1], [texUv(x0, y0), texUv(x1, y0), texUv(x1, y1), texUv(x0, y1)]);
  });
  part('pcb', 'kretskort', M.pcbBack, (b) => {
    const z = -PCB.t;
    for (const [x0, y0, x1, y1] of boardRects) b.quad([x1, y0, z], [x0, y0, z], [x0, y1, z], [x1, y1, z], [0, 0, -1], [texUv(x1, y0, true), texUv(x0, y0, true), texUv(x0, y1, true), texUv(x1, y1, true)]);
  });
  part('pcb', 'kretskort', M.pcbEdge, (b) => {
    for (const [x0, y0, x1, y1] of boardRects) b.box([(x0 + x1) / 2, (y0 + y1) / 2, -PCB.t / 2], [x1 - x0, y1 - y0, PCB.t], { skip: ['pz', 'nz'] });
  });
  // Kontaktfingrene på begge sider.
  part('pcie', 'kretskort', M.gold, (b) => {
    for (let x = FINGERS.tabA[0] + 1.4; x < FINGERS.tabB[1] - 1.3; x += FINGERS.pitch) {
      if (x > FINGERS.tabA[1] - 0.6 && x < FINGERS.tabB[0] + 0.7) continue;
      const long = Math.round(x * 7) % 9 === 0 ? 0 : 0.6; // noen kortere (tilstedeværelse)
      b.box([x, FINGERS.y0 + 0.5 + (7.1 - long) / 2 + long, 0.02], [FINGERS.w, 7.1 - long, 0.04]);
      b.box([x, FINGERS.y0 + 0.5 + (7.1 - long) / 2 + long, -PCB.t - 0.02], [FINGERS.w, 7.1 - long, 0.04]);
    }
  });

  // GPU: kulegitter (mørkt), substrat og selve brikken.
  part('gpu', 'kretskort', M.bga, (b) => b.box([GPU.cx, GPU.cy, 0.22], [GPU.sub - 2, GPU.sub - 2, 0.44]));
  part('gpu', 'kretskort', M.sub, (b) => b.roundedBox([GPU.cx, GPU.cy, 1.02], [GPU.sub, GPU.sub, 1.15], 0.3, 1, atlasUv('sub')), { uv: atlasPoint('sub') });
  part('gpu', 'kretskort', M.die, (b) => b.roundedBox([GPU.cx, GPU.cy, 1.97], [GPU.dieW, GPU.dieH, 0.75], 0.12, 1, atlasUv('die', 2)), { uv: atlasPoint('die') });

  part('memory', 'kretskort', M.mem, (b) => {
    for (const m of MEMORY) {
      b.with(mat4.mul(mat4.create(), T(m.x, m.y, 0), m.r ? RZ(90 * DEG) : mat4.create()), () => {
        b.roundedBox([0, 0, 0.85], [14, 12, 1.1], 0.25, 1, atlasUv('mem', 4));
      });
    }
  }, { uv: atlasPoint('mem') });
  part('chokes', 'kretskort', M.choke, (b) => {
    for (const c of CHOKES) b.roundedBox([c.x, c.y, 3.0], [10.6, 10.0, 6.0], 0.7, 2, atlasUv('choke', 6));
  }, { uv: atlasPoint('choke') });
  part('chokes', 'kretskort', M.tin, (b) => {
    for (const c of CHOKES) {
      b.box([c.x - 5.55, c.y, 0.9], [0.5, 6.4, 1.8]);
      b.box([c.x + 5.55, c.y, 0.9], [0.5, 6.4, 1.8]);
    }
  });
  part('stages', 'kretskort', M.ps, (b) => {
    for (const p of POWERSTAGES) b.box([p.x, p.y, 0.5], [5, 6, 0.95], { pz: atlasUv('ps', 4) });
  }, { uv: atlasPoint('ps') });
  part('caps', 'kretskort', M.cap, (b) => {
    for (const c of CAPS) b.cylinder(c.x, c.y, c.r, 0.45, c.h, 28, { caps: [false, true], uvCap: atlasUv('cap', 2) });
  }, { uv: [atlasUv('cap')[0] + 0.004, atlasUv('cap')[1] + 0.06] });
  part('caps', 'kretskort', M.plastic, (b) => {
    for (const c of CAPS) b.box([c.x, c.y, 0.22], [6.6, 6.6, 0.44]);
  });
  part('ics', 'kretskort', M.ic, (b) => {
    for (const i of ICS) b.box([i.x, i.y, 0.45], [i.w, i.h, 0.9], { pz: atlasUv('ic', 6) });
  }, { uv: atlasPoint('ic') });
  part('headers', 'kretskort', M.hdr, (b) => {
    for (const h of HEADERS) b.box([h.x, h.y, 1.6], [h.w, h.h, 3.2], { pz: atlasUv('hdr', 2) });
  }, { uv: atlasPoint('hdr') });

  // Små komponenter: kropp og tinnender.
  const smd = Lo.smallParts();
  const smdBody = (kind) => (b) => {
    for (const p of smd) {
      if (p.kind !== kind) continue;
      const along = p.w > p.h;
      b.box([p.x, p.y, p.t / 2], along ? [p.w * 0.62, p.h, p.t] : [p.w, p.h * 0.62, p.t]);
    }
  };
  part('smd', 'kretskort', M.smdCap, smdBody('cap'), { cast: false });
  part('smd', 'kretskort', M.smdRes, smdBody('res'), { cast: false });
  part('smd', 'kretskort', M.tin, (b) => {
    for (const p of smd) {
      const along = p.w > p.h;
      const e = (along ? p.w : p.h) * 0.2;
      for (const s of [-1, 1]) {
        if (along) b.box([p.x + s * (p.w / 2 - e / 2), p.y, p.t / 2], [e, p.h + 0.02, p.t + 0.02], { skip: ['ny'] });
        else b.box([p.x, p.y + s * (p.h / 2 - e / 2), p.t / 2], [p.w + 0.02, e, p.t + 0.02], { skip: ['nz'] });
      }
    }
  }, { cast: false });
  // Baksiden: MLCC-er bak GPU-en.
  const back = Lo.backCaps();
  part('backcaps', 'kretskort', M.smdCap, (b) => {
    for (const p of back) b.box([p.x, p.y, -PCB.t - p.t / 2], [p.w * 0.62, p.h, p.t]);
  }, { cast: false });
  part('backcaps', 'kretskort', M.tin, (b) => {
    for (const p of back) for (const s of [-1, 1]) b.box([p.x + s * (p.w / 2 - p.w * 0.1), p.y, -PCB.t - p.t / 2], [p.w * 0.2, p.h + 0.02, p.t + 0.02]);
  }, { cast: false });

  // Strømkontakter: hus, åpninger for pinnene og lås.
  part('power', 'kretskort', M.plastic, (b) => {
    for (const p of POWER) {
      b.roundedBox([(p.x0 + p.x1) / 2, (p.y0 + p.y1) / 2, p.z1 / 2], [p.x1 - p.x0, p.y1 - p.y0, p.z1], 0.6, 1);
      b.box([(p.x0 + p.x1) / 2, p.y1 - 4.2, p.z1 + 0.7], [4.2, 6.4, 1.4]);
      b.box([(p.x0 + p.x1) / 2, p.y1 - 1.0, p.z1 + 1.3], [4.2, 1.6, 1.0]);
    }
  });
  part('power', 'kretskort', M.hole, (b) => {
    for (const p of POWER) {
      const cols = p.pins / 2;
      const pitch = (p.x1 - p.x0 - 1.8) / cols;
      for (let i = 0; i < cols; i++) {
        for (let j = 0; j < 2; j++) {
          b.box([p.x0 + 0.9 + pitch * (i + 0.5), p.y1 + 0.01, 2.45 + j * 4.9], [pitch - 1.0, 0.04, 3.6]);
        }
      }
    }
  }, { cast: false });

  // Utganger: metallskall og mørk åpning mot braketten.
  part('ports', 'kretskort', M.portMetal, (b) => {
    for (const p of PORTS) {
      b.roundedBox([5.5, (p.y0 + p.y1) / 2, (p.z0 + p.z1) / 2], [17, p.y1 - p.y0, p.z1 - p.z0], Math.min(p.r, 1.1), 1);
    }
  });
  part('ports', 'kretskort', M.hole, (b) => {
    for (const p of PORTS) {
      const ins = 0.55;
      const x = -3.03;
      b.quad([x, p.y0 + ins, p.z1 - ins], [x, p.y1 - ins, p.z1 - ins], [x, p.y1 - ins, p.z0 + ins], [x, p.y0 + ins, p.z0 + ins], [-1, 0, 0]);
    }
  }, { cast: false });

  // ------------------------------------------------------------ Brakett
  part('bracket', 'brakett', M.bracket, (b) => {
    const B = BRACKET;
    const uv = (y, z) => bracketUv(y, z);
    b.quad([B.x1, B.y0, B.z0], [B.x1, B.y0, B.z1], [B.x1, B.y1, B.z1], [B.x1, B.y1, B.z0], [1, 0, 0], [uv(B.y0, B.z0), uv(B.y0, B.z1), uv(B.y1, B.z1), uv(B.y1, B.z0)]);
    b.quad([B.x0, B.y0, B.z1], [B.x0, B.y0, B.z0], [B.x0, B.y1, B.z0], [B.x0, B.y1, B.z1], [-1, 0, 0], [uv(B.y0, B.z1), uv(B.y0, B.z0), uv(B.y1, B.z0), uv(B.y1, B.z1)]);
  });
  part('bracket', 'brakett', M.bracketEdge, (b) => {
    const B = BRACKET;
    b.box([(B.x0 + B.x1) / 2, (B.y0 + B.y1) / 2, (B.z0 + B.z1) / 2], [B.x1 - B.x0, B.y1 - B.y0, B.z1 - B.z0], { skip: ['px', 'nx'] });
    // Øvre flens (skrus fast i kabinettet) og tungen nederst.
    b.box([(B.x0 + 10.5) / 2, B.y1 - 0.4, (B.z0 + B.z1) / 2], [10.5 - B.x0, 0.8, B.z1 - B.z0]);
    b.box([(B.x0 + B.x1) / 2, B.y0 - 6, 2.8], [B.x1 - B.x0, 12, 7.5]);
  });

  // ------------------------------------------------------------ Bakplate
  part('backplate', 'bakplate', M.backplate, (b) => {
    const B = BACKPLATE;
    const uv = (x, y) => backplateUv(x, y);
    b.quad([B.x1, B.y0, B.z0], [B.x0, B.y0, B.z0], [B.x0, B.y1, B.z0], [B.x1, B.y1, B.z0], [0, 0, -1], [uv(B.x1, B.y0), uv(B.x0, B.y0), uv(B.x0, B.y1), uv(B.x1, B.y1)]);
    b.quad([B.x0, B.y0, B.z1], [B.x1, B.y0, B.z1], [B.x1, B.y1, B.z1], [B.x0, B.y1, B.z1], [0, 0, 1], [uv(B.x0, B.y0), uv(B.x1, B.y0), uv(B.x1, B.y1), uv(B.x0, B.y1)]);
  });
  part('backplate', 'bakplate', M.backplateEdge, (b) => {
    const B = BACKPLATE;
    b.box([(B.x0 + B.x1) / 2, (B.y0 + B.y1) / 2, (B.z0 + B.z1) / 2], [B.x1 - B.x0, B.y1 - B.y0, B.z1 - B.z0], { skip: ['pz', 'nz'] });
    // Vinduets innerkant (så platen har tykkelse der den er åpen).
    const w = B.window;
    b.box([(w.x0 + w.x1) / 2, w.y0 + 0.01, (B.z0 + B.z1) / 2], [w.x1 - w.x0 - 6, 0.02, B.z1 - B.z0], { skip: ['ny', 'pz', 'nz', 'px', 'nx'] });
    b.box([(w.x0 + w.x1) / 2, w.y1 - 0.01, (B.z0 + B.z1) / 2], [w.x1 - w.x0 - 6, 0.02, B.z1 - B.z0], { skip: ['py', 'pz', 'nz', 'px', 'nx'] });
    b.box([w.x0 + 0.01, (w.y0 + w.y1) / 2, (B.z0 + B.z1) / 2], [0.02, w.y1 - w.y0 - 6, B.z1 - B.z0], { skip: ['nx', 'pz', 'nz', 'py', 'ny'] });
    b.box([w.x1 - 0.01, (w.y0 + w.y1) / 2, (B.z0 + B.z1) / 2], [0.02, w.y1 - w.y0 - 6, B.z1 - B.z0], { skip: ['px', 'pz', 'nz', 'py', 'ny'] });
  });
  part('backplate', 'bakplate', M.screw, (b) => {
    for (const [x, y] of [
      [6, 6],
      [6, 105],
      [154, 6],
      [154, 105],
      [264, 6],
      [264, 105],
    ]) {
      b.cylinder(x, y, 2.3, BACKPLATE.z0 - 0.9, BACKPLATE.z0, 20, { caps: [false, true] });
    }
  });

  // ------------------------------------------------------------ Festekryss (bak GPU-en, del av kjøleren)
  const xz0 = -PCB.t - 0.85;
  part('xbracket', 'kryss', M.xbr, (b) => {
    for (const a of [45, 135]) {
      b.with(mat4.mul(mat4.create(), T(GPU.cx, GPU.cy, xz0 + 0.4), RZ(a * DEG)), () => b.roundedBox([0, 0, 0], [80, 7, 0.8], 0.3, 1));
    }
    b.cylinder(GPU.cx, GPU.cy, 10, xz0, xz0 + 0.8, 36);
  });
  part('xbracket', 'kryss', M.screw, (b) => {
    for (const [x, y] of HOLES.slice(0, 4)) {
      b.cylinder(x, y, 2.7, xz0 - 1.2, xz0, 24, { caps: [true, false] });
      b.box([x, y, xz0 - 1.21], [3.2, 0.7, 0.04]);
      b.box([x, y, xz0 - 1.21], [0.7, 3.2, 0.04]);
    }
  });

  // ------------------------------------------------------------ Kjøler
  const C = COOLER;
  part('plate', 'kjoler', M.nickel, (b) => {
    const p = C.plate;
    b.roundedBox([(p.x0 + p.x1) / 2, (p.y0 + p.y1) / 2, (p.z0 + p.z1) / 2], [p.x1 - p.x0, p.y1 - p.y0, p.z1 - p.z0], 0.9, 2);
  });
  part('pads', 'kjoler', M.pad, (b) => {
    for (const m of MEMORY) b.box([m.x, m.y, 2.0], [m.w + 0.6, m.h + 0.6, 1.2]);
    for (const p of POWERSTAGES) b.box([p.x, p.y, 1.7], [5.6, 6.4, 1.4]);
    for (const c of CHOKES) b.box([c.x, c.y, 6.45], [10.2, 9.6, 0.9]);
  });
  part('midplate', 'kjoler', M.midplate, (b) => {
    const z = 3.3;
    b.roundedBox([74, 59, z], [32, 94, 1.4], 0.6, 1);
    b.roundedBox([148, 59, z], [32, 94, 1.4], 0.6, 1);
    b.roundedBox([112, 95, z], [42, 22, 1.4], 0.6, 1);
    b.roundedBox([112, 28, z], [42, 22, 1.4], 0.6, 1);
  });
  part('vrmsink', 'kjoler', M.alu, (b) => {
    const v = C.vrm;
    b.roundedBox([(v.x0 + v.x1) / 2, (v.y0 + v.y1) / 2, (v.z0 + v.z1) / 2], [v.x1 - v.x0, v.y1 - v.y0, v.z1 - v.z0], 0.8, 1);
    b.roundedBox([169.25, (v.y0 + v.y1) / 2, 6.0], [10.5, v.y1 - v.y0, 7.2], 0.8, 1);
    b.roundedBox([40, 86.5, 8.25], [12, 24, 2.7], 0.8, 1);
    b.roundedBox([29.25, 86.5, 6.0], [10.5, 24, 7.2], 0.8, 1);
  });
  // Lamellene: detaljert nær, stripet blokk på avstand (visningen velger).
  part('fins', 'kjoler', M.finBlock, (b) => {
    const f = C.fins;
    const uvx = (x) => (x - f.x0) / f.pitch;
    const { x0, x1, y0, y1, z0, z1 } = f;
    const end = [0.06, 0.5];
    b.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0, 0, 1], [[uvx(x0), 0], [uvx(x1), 0], [uvx(x1), 1], [uvx(x0), 1]]);
    b.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0, 0, -1], [[uvx(x1), 0], [uvx(x0), 0], [uvx(x0), 1], [uvx(x1), 1]]);
    b.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0, 1, 0], [[uvx(x0), 0], [uvx(x1), 0], [uvx(x1), 1], [uvx(x0), 1]]);
    b.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0, -1, 0], [[uvx(x0), 0], [uvx(x1), 0], [uvx(x1), 1], [uvx(x0), 1]]);
    b.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1, 0, 0], [end, end, end, end]);
    b.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1, 0, 0], [end, end, end, end]);
  }, { lod: 'far' });
  part('fins', 'kjoler', M.finAlu, (b) => {
    const f = C.fins;
    const n = Math.floor((f.x1 - f.x0) / f.pitch);
    for (let i = 0; i <= n; i++) {
      const x = f.x0 + i * f.pitch;
      const end = i === 0 || i === n;
      b.box([x, (f.y0 + f.y1) / 2, (f.z0 + f.z1) / 2], [end ? 0.8 : f.t, f.y1 - f.y0, f.z1 - f.z0]);
    }
    // Tverrbånd langs topp og bunn (fingrene er brettet og låst sammen).
    b.box([(f.x0 + f.x1) / 2, f.y1 - 0.3, f.z1 - 0.2], [f.x1 - f.x0, 0.6, 0.4]);
    b.box([(f.x0 + f.x1) / 2, f.y0 + 0.3, f.z1 - 0.2], [f.x1 - f.x0, 0.6, 0.4]);
    b.box([(f.x0 + f.x1) / 2, (f.y0 + f.y1) / 2, f.z0 + 0.3], [f.x1 - f.x0, f.y1 - f.y0, 0.6]);
  }, { lod: 'near' });
  part('pipes', 'kjoler', M.copper, (b) => {
    const RY = mat4.rotY(Math.PI / 2);
    const run = (y, z, xa, xb) => b.with(RY, () => b.cylinder(-z, y, 3, xa, xb, 20, { caps: [false, false] }));
    const cap = (y, z, x, dir) => b.with(mat4.mul(mat4.create(), T(x, y, z), mat4.rotY((dir * Math.PI) / 2)), () => b.cylinder(0, 0, 3, 0, 0.6, 20, { caps: [false, true], r1: 2.2 }));
    for (const p of C.pipes) {
      if (p.bendLeft) {
        const xL = 12;
        run(p.y, 11, xL, 282);
        run(p.y, 27, xL, xL + 50);
        b.torusArc([xL, p.y, 19], 8, 3, Math.PI / 2, (3 * Math.PI) / 2, 14, 18);
        cap(p.y, 11, 282, 1);
        cap(p.y, 27, xL + 50, 1);
      } else {
        const xR = 282;
        run(p.y, 11, 12, xR);
        run(p.y, 27, xR - 50, xR);
        b.torusArc([xR, p.y, 19], 8, 3, -Math.PI / 2, Math.PI / 2, 14, 18);
        cap(p.y, 11, 12, -1);
        cap(p.y, 27, xR - 50, -1);
      }
    }
  });

  // ------------------------------------------------------------ Deksel
  const S = SHROUD;
  const A = [
    [S.x0, 15],
    [15, S.y0],
    [S.x1 - 13, S.y0],
    [S.x1, 15],
    [S.x1, S.y1 - 10],
    [S.x1 - 5, S.y1 - 5],
    [S.x0 + 5, S.y1 - 5],
    [S.x0, S.y1 - 10],
  ];
  const Bp = [
    [S.x0, 15],
    [15, S.y0],
    [S.x1 - 13, S.y0],
    [S.x1, 15],
    [S.x1, S.y1 - 5],
    [S.x1 - 5, S.y1],
    [S.x0 + 5, S.y1],
    [S.x0, S.y1 - 5],
  ];
  part('shroud', 'deksel', M.shroud, (b) => b.extrudeConvex(A, S.zFront - 2, S.zFront, { sides: false }));
  part('shroud', 'deksel', M.skirt, (b) => {
    const loft = (P, za, Q, zb) => {
      for (let i = 0; i < P.length; i++) {
        const j = (i + 1) % P.length;
        const a = [P[i][0], P[i][1], za];
        const c = [P[j][0], P[j][1], za];
        const d = [Q[j][0], Q[j][1], zb];
        const e = [Q[i][0], Q[i][1], zb];
        // Flat normal ut av flaten.
        const u = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        const v = [e[0] - a[0], e[1] - a[1], e[2] - a[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        const out = [u[1], -u[0], 0];
        if (n[0] * out[0] + n[1] * out[1] + n[2] * out[2] + n[2] * 0.001 < 0) n = [-n[0], -n[1], -n[2]];
        const l = Math.hypot(n[0], n[1], n[2]) || 1;
        n = [n[0] / l, n[1] / l, n[2] / l];
        const ia = b.vert(a, n);
        const ic = b.vert(c, n);
        const id = b.vert(d, n);
        const ie = b.vert(e, n);
        b.addQuadOriented(ia, ic, id, ie);
      }
    };
    loft(A, S.zFront, Bp, S.zChamfer);
    loft(Bp, S.zChamfer, Bp, S.zBack);
    // Kanten rundt vifteåpningene (innside av hullet) og luftkanalen bak.
    for (const f of FANS) {
      const seg = 72;
      const base = b.count;
      for (let i = 0; i <= seg; i++) {
        const a = (i / seg) * Math.PI * 2;
        const n = [-Math.cos(a), -Math.sin(a), 0];
        b.vert([f.cx + Math.cos(a) * FAN.rHole, f.cy + Math.sin(a) * FAN.rHole, S.zFront - 2], n);
        b.vert([f.cx + Math.cos(a) * FAN.rHole, f.cy + Math.sin(a) * FAN.rHole, S.zFront], n);
      }
      for (let i = 0; i < seg; i++) {
        const a = base + i * 2;
        b.addQuadOriented(a, a + 2, a + 3, a + 1);
      }
    }
  });
  part('shroud', 'deksel', M.duct, (b) => {
    for (const f of FANS) b.tube(f.cx, f.cy, FAN.rHole + 1.4, FAN.rHole, 34.5, S.zFront - 2, 72);
  });
  // Pyntefelt i blank svart (som i 2D-forhåndsvisningen).
  part('shroud', 'deksel', M.trim, (b) => {
    const z0 = S.zFront;
    const z1 = S.zFront + 0.8;
    b.extrudeConvex(
      [
        [134.4, 14.2],
        [150.9, 14.2],
        [156.7, 97.1],
        [140.2, 97.1],
      ],
      z0,
      z1,
    );
    b.extrudeConvex(
      [
        [279.4, 15.4],
        [288.2, 25.2],
        [288.2, 81.3],
        [279.4, 91.0],
      ],
      z0,
      z1,
    );
    b.extrudeConvex(
      [
        [6.9, 20.3],
        [13.7, 14.2],
        [13.7, 98.4],
        [6.9, 91.0],
      ],
      z0,
      z1,
    );
  });

  // Lysdiodene: lyslist på fasen øverst og en ring rundt hver vifte.
  const leds = Lo.ledLayout();
  const ledPos = [];
  part('bar', 'deksel', M.led, (b) => {
    const segW = (BAR.x1 - BAR.x0) / BAR.leds;
    for (let i = 0; i < BAR.leds; i++) {
      b.ledIndex = i;
      const cx = BAR.x0 + (i + 0.5) * segW;
      const cy = S.y1 - 2.5;
      const cz = S.zFront - 2.5;
      b.with(mat4.mul(mat4.create(), T(cx, cy, cz), mat4.rotX(-45 * DEG)), () => b.roundedBox([0, 0, 0.25], [segW - 1.6, 2.9, 0.7], 0.3, 1));
      ledPos.push({ p: [cx, cy + 0.5 * SQ, cz + 0.5 * SQ], n: [0, SQ, SQ] });
    }
  });
  for (const [fi, f] of FANS.entries()) {
    part(fi ? 'ring2' : 'ring1', 'deksel', M.led, (b) => {
      for (const l of leds.filter((q) => q.zone === f.id)) {
        b.ledIndex = 20 + fi * 18 + l.i;
        b.arcStrip(f.cx, f.cy, RING.r0, RING.r1, l.a0, l.a1, S.zFront - 0.1, S.zFront + 0.55, 6);
        const am = (l.a0 + l.a1) / 2;
        const rm = (RING.r0 + RING.r1) / 2;
        ledPos.push({ p: [f.cx + Math.cos(am) * rm, f.cy + Math.sin(am) * rm, S.zFront + 0.7], n: [0, 0, 1] });
      }
    });
  }

  // ------------------------------------------------------------ Vifter
  for (const [fi, f] of FANS.entries()) {
    const g = fi ? 'fan2' : 'fan1';
    // Rotoren bygges rundt (0, 0) og plasseres av visningen (rotor-indeksen).
    part(g, 'vifter', M.blade, (b) => {
      for (let k = 0; k < FAN.blades; k++) {
        const phi = (k / FAN.blades) * Math.PI * 2;
        b.grid(8, 7, (u, v) => {
          const r = FAN.rHub - 0.8 + (FAN.rRotor - FAN.rHub + 0.8) * v;
          const width = 0.5 + 0.12 * v;
          const sweep = -0.42 * v * v;
          const th = phi + sweep + (u - 0.5) * width;
          const pitch = 9.2 - 3.6 * v;
          const camber = 1.1 * Math.sin(u * Math.PI) * (1 - 0.3 * v);
          const z = FAN.zMid - (u - 0.5) * pitch + camber;
          return [Math.cos(th) * r, Math.sin(th) * r, z];
        });
      }
    }, { cast: false, rotor: fi });
    part(g, 'vifter', M.hubSide, (b) => b.cylinder(0, 0, FAN.rHub, 35.5, 45.0, 40, { caps: [false, false] }), { cast: false, rotor: fi });
    part(g, 'vifter', M.hub, (b) => {
      b.cylinder(0, 0, FAN.rHub, 45.0, 45.3, 40, { caps: [false, false], r1: FAN.rHub - 0.6 });
      b.disc(0, 0, FAN.rHub - 0.6, 45.3, 1, 40, atlasUv('hub', 4));
    }, { cast: false, rotor: fi, uv: atlasPoint('hub') });
    // Motor og stag (står stille).
    part(g, 'vifter', M.motor, (b) => {
      b.cylinder(f.cx, f.cy, 14, 31.5, 35.6, 32, { caps: [true, false] });
      for (const a of [100, 220, 340]) {
        b.with(mat4.mul(mat4.create(), T(f.cx, f.cy, 33), RZ(a * DEG)), () => b.box([30.5, 0, 0], [33, 3.2, 2]));
      }
    });
  }

  // Lysene som lyser på omgivelsene (i kortets koordinater, flyttes med dekselet).
  const ledLights = {
    barA: [BAR.x0 + 6, S.y1 + 3, S.zFront - 1],
    barB: [BAR.x1 - 6, S.y1 + 3, S.zFront - 1],
    fan1: [FANS[0].cx, FANS[0].cy, S.zFront + 7],
    fan2: [FANS[1].cx, FANS[1].cy, S.zFront + 7],
  };

  return { parts, ledPos, ledLights, fanCenters: FANS.map((f) => [f.cx, f.cy]), center: [(S.x0 + S.x1) / 2 - 2, 56, 20] };
}
