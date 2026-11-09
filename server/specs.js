// Innebygd spesifikasjonstabell for skjermkort, slått opp på PCI-ID.
// Dette er REFERANSEVERDIER fra produsentens/brikkens spesifikasjon. De er ikke
// målt på maskinen, og kortprodusenten kan ha endret klokker eller minne.

const TU104_2080 = Object.freeze({
  chip: 'TU104',
  variant: 'TU104-400A-A1',
  arch: 'Turing',
  process: '12 nm (TSMC 12FFN)',
  dieMm2: 545,
  transistorsBn: 13.6,
  cuda: 2944,
  tmu: 184,
  rop: 64,
  sm: 46,
  tensor: 368,
  rt: 46,
  l2KB: 4096,
  memType: 'GDDR6',
  busWidth: 256,
  memGB: 8,
  memGbps: 14,
  bandwidthGBs: 448,
  baseMHz: 1515,
  boostMHz: 1710,
  tdpW: 215,
  launch: 'september 2018',
  bus: 'PCIe 3.0 x16',
  directx: '12 Ultimate (12_2)',
  vulkan: '1.4',
  opengl: '4.6',
  memVendor: 'Ukjent på Linux',
  memVendorNote: 'Minneleverandøren (Samsung eller Micron) står i VBIOS-en, men nvidia-smi oppgir den ikke, og VBIOS kan ikke leses uten root.',
});

// Norsk desimalkomma i formlene.
const nb = (v, d) => v.toFixed(d).replace('.', ',');

const TABLE = Object.freeze({
  '10de:1e87': { ...TU104_2080, name: 'GeForce RTX 2080 Rev. A' },
  '10de:1e82': { ...TU104_2080, name: 'GeForce RTX 2080', variant: 'TU104-400-A1' },
});

/** Slår opp på «vendor:device» (små bokstaver). Gir null hvis ukjent. */
export function lookupSpec(vendor, device) {
  if (!vendor || !device) return null;
  const key = `${vendor.toLowerCase()}:${device.toLowerCase()}`;
  const s = TABLE[key];
  if (!s) return null;
  // Avledede tall. Formlene sendes med, så UI-et kan vise dem.
  const boostG = s.boostMHz / 1000;
  return {
    key,
    ...s,
    derived: {
      pixelFill: {
        value: Math.round(s.rop * boostG * 10) / 10,
        unit: 'GPixel/s',
        formula: `${s.rop} ROP × ${s.boostMHz} MHz (referanse-boost) = ${nb(s.rop * boostG, 1)} GPixel/s`,
      },
      texFill: {
        value: Math.round(s.tmu * boostG * 10) / 10,
        unit: 'GTexel/s',
        formula: `${s.tmu} TMU × ${s.boostMHz} MHz (referanse-boost) = ${nb(s.tmu * boostG, 1)} GTexel/s`,
      },
      fp32: {
        value: Math.round(s.cuda * 2 * boostG) / 1000,
        unit: 'TFLOPS',
        formula: `${s.cuda} CUDA-kjerner × 2 operasjoner (FMA) × ${s.boostMHz} MHz = ${nb((s.cuda * 2 * boostG) / 1000, 2)} TFLOPS`,
      },
      bandwidth: {
        value: (s.busWidth / 8) * s.memGbps,
        unit: 'GB/s',
        formula: `${s.busWidth} bit ÷ 8 × ${s.memGbps} Gbps = ${(s.busWidth / 8) * s.memGbps} GB/s`,
      },
    },
  };
}

export const KNOWN_IDS = Object.freeze(Object.keys(TABLE));
