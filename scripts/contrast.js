// Regner ut WCAG-kontrast for alle fargekombinasjoner i appen:
// fargetema × lyst/mørkt × aksentfarge × høy kontrast.
//
// Kilden er web/styles.css, seksjon 2 (de samme tokenene nettleseren bruker),
// så tallene kan ikke gå ut av takt med stilarket. Kjør: npm run contrast
//
// Krav:
//   Tekst (4,5:1)       vanlig tekst, dempet tekst, aksenttekst og statusfarger
//                       på alle flater, hvit tekst på aksentfyll, verktøytips,
//                       tekst på valgt rad og på myke status-/aksentflater,
//                       og markerte søketreff i Innstillinger (v0.0.15).
//   UI-grenser (3:1)    feltkanter (felt, avkrysning, radio, bryter av),
//                       fokusring, fyll i stolper/glidere mot sporet og mot
//                       kortet, grafserier og statusfyll mot kortet, og hele
//                       seriepaletten (--pal-*) mot kortet (v0.0.11).
//   Scenen på Lys       tekst på --stage og på merkene (--stage-chip), også
//                       over et lyst bilde, og fokusringen (--stage-focus).
//   Høy kontrast        i tillegg rammer mot alle flater (3:1). Stolper,
//                       glidere og brytere får da kant i --border.
// Rene skillelinjer (--border, --border-soft) og rutenett er pynt og har ikke
// krav i vanlig modus (WCAG 1.4.11 gjelder ikke dekor).
//
// Avslutter med kode 1 hvis noe er under kravet.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const css = readFileSync(join(ROOT, 'web', 'styles.css'), 'utf8');

const PALETTES = ['grafitt', 'sand', 'fjord', 'midnatt'];
const MODES = ['light', 'dark'];
const ACCENTS = ['blue', 'teal', 'green', 'purple', 'pink', 'slate'];
const CONTRASTS = ['normal', 'high'];

// ---- Les seksjon 2 ----
const a = css.indexOf('2 · Temaer');
const b = css.indexOf('3 · Grunnlag');
if (a < 0 || b < 0) throw new Error('Fant ikke seksjon 2 i styles.css');
const section = css
  .slice(css.indexOf('*/', a) + 2, css.lastIndexOf('/*', b))
  .replace(/\/\*[\s\S]*?\*\//g, '');

const rules = [];
for (const m of section.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
  const decls = {};
  for (const d of m[2].split(';')) {
    const i = d.indexOf(':');
    if (i < 0) continue;
    const k = d.slice(0, i).trim();
    if (k.startsWith('--')) decls[k] = d.slice(i + 1).trim();
  }
  for (const sel of m[1].split(',').map((x) => x.trim())) {
    // Bare selektorer på <html> selv (ikke miniatyrene .pv[…]).
    const sm = /^:root((?:\[[\w-]+='[^']*'\])*)$/.exec(sel);
    if (!sm) continue;
    const attrs = [...sm[1].matchAll(/\[([\w-]+)='([^']*)'\]/g)].map((x) => [x[1], x[2]]);
    rules.push({ attrs, decls, spec: 1 + attrs.length, order: rules.length });
  }
}

function tokensFor(env) {
  const out = {};
  const hit = rules.filter((r) => r.attrs.every(([k, v]) => env[k] === v)).sort((x, y) => x.spec - y.spec || x.order - y.order);
  for (const r of hit) Object.assign(out, r.decls);
  return out;
}

// ---- Farger ----
function parseColor(s) {
  s = s.trim();
  if (s === 'transparent') return [0, 0, 0, 0];
  let m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s);
  if (m) {
    let h = m[1];
    if (h.length === 3) h = h.replace(/./g, '$&$&');
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)).concat(1);
  }
  m = /^rgba?\(([^)]+)\)$/.exec(s);
  if (m) {
    const p = m[1].split(',').map((x) => parseFloat(x));
    return [p[0], p[1], p[2], p[3] ?? 1];
  }
  return null;
}

function resolve(tok, name, depth = 0) {
  if (depth > 20) throw new Error(`Sirkel i ${name}`);
  const raw = tok[name];
  if (raw == null) throw new Error(`Mangler ${name}`);
  return evalColor(tok, raw, depth);
}

function evalColor(tok, raw, depth) {
  raw = raw.trim();
  let m = /^var\((--[\w-]+)\)$/.exec(raw);
  if (m) return resolve(tok, m[1], depth + 1);
  m = /^color-mix\(in srgb,\s*(.+?)\s+(\d+(?:\.\d+)?)%,\s*(.+)\)$/.exec(raw);
  if (m) {
    const c1 = evalColor(tok, m[1], depth + 1);
    const c2 = evalColor(tok, m[3], depth + 1);
    const p = Number(m[2]) / 100;
    // Premultiplisert blanding, som i CSS.
    const al = c1[3] * p + c2[3] * (1 - p);
    if (al === 0) return [0, 0, 0, 0];
    const ch = [0, 1, 2].map((i) => (c1[i] * c1[3] * p + c2[i] * c2[3] * (1 - p)) / al);
    return [...ch, al];
  }
  const c = parseColor(raw);
  if (!c) throw new Error(`Kan ikke tolke farge: ${raw}`);
  return c;
}

/** Legger en (delvis gjennomsiktig) farge over en ugjennomsiktig bakgrunn. */
const over = (fg, bg) => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3])).concat(1);
const lin = (v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const lum = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
function ratio(fg, bg) {
  const f = over(fg, bg);
  const [x, y] = [lum(f), lum(bg)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

// ---- Par som sjekkes ----
const SURFACES = ['--bg', '--card', '--card-alt', '--panel-head', '--sidebar', '--head', '--field'];
const TEXT_FG = ['--text', '--muted', '--dim', '--accent', '--ok', '--warn', '--danger'];

function checks(tok, high) {
  const C = (n) => resolve(tok, n);
  const solid = (n, on) => over(C(n), C(on));
  const list = [];
  const text = (label, fg, bg) => list.push({ kind: 'tekst', min: 4.5, label, r: ratio(fg, bg) });
  const ui = (label, fg, bg) => list.push({ kind: 'ui', min: 3, label, r: ratio(fg, bg) });

  for (const f of TEXT_FG) for (const s of SURFACES) text(`${f} på ${s}`, C(f), C(s));
  text('--accent-fg på --accent-bg', C('--accent-fg'), C('--accent-bg'));
  text('--tip-fg på --tip-bg', C('--tip-fg'), C('--tip-bg'));
  text('--text på valgt rad (--selected over --sidebar)', C('--text'), solid('--selected', '--sidebar'));
  text('--text på --hover over --card', C('--text'), solid('--hover', '--card'));
  text('--accent på --accent-soft over --card', C('--accent'), solid('--accent-soft', '--card'));
  // Innstillinger (v0.0.15): markerte ord i søketreffene, og delene uten treff (dempet i navigasjonen).
  text('--text på --accent-soft-strong over --card (søketreff)', C('--text'), solid('--accent-soft-strong', '--card'));
  text('--muted på --hover over --sidebar (del uten treff)', C('--muted'), solid('--hover', '--sidebar'));
  text('--warn på --warn-soft over --card', C('--warn'), solid('--warn-soft', '--card'));
  text('--danger på --danger-soft over --card', C('--danger'), solid('--danger-soft', '--card'));
  // Linjene under toppfeltet (v0.0.7): bekreftelsen, tjenesten ikke tilkoblet og frakoblet.
  for (const f of ['--text', '--muted']) {
    text(`${f} på --accent-soft over --bg (bekreftelsen)`, C(f), solid('--accent-soft', '--bg'));
    text(`${f} på --warn-soft over --bg (tjenesten ikke tilkoblet)`, C(f), solid('--warn-soft', '--bg'));
    text(`${f} på --info-soft over --bg (rullet tilbake)`, C(f), solid('--info-soft', '--bg'));
  }
  text('--text på --danger-soft over --bg (frakoblet)', C('--text'), solid('--danger-soft', '--bg'));
  text('--ok på 14 % --ok over --card (merke)', C('--ok'), over([...C('--ok').slice(0, 3), 0.14], C('--card')));

  for (const s of ['--field', '--card', '--bg']) ui(`--field-border mot ${s}`, C('--field-border'), C(s));
  for (const s of ['--bg', '--card', '--sidebar']) ui(`--focus mot ${s}`, C('--focus'), C(s));
  ui('--accent-fill mot --track', C('--accent-fill'), C('--track'));
  ui('--accent-fill mot --card', C('--accent-fill'), C('--card'));
  for (const n of ['--series-1', '--series-2', '--series-3', '--warn-fill', '--danger-fill', '--ok']) ui(`${n} mot --card`, C(n), C('--card'));
  // Seriepaletten (v0.0.11): hver graffarge mot kortet og flatene grafene står på.
  for (const n of ['--pal-blue', '--pal-magenta', '--pal-purple', '--pal-green', '--pal-teal', '--pal-cyan', '--pal-gray']) for (const s of ['--card', '--card-alt']) ui(`${n} mot ${s}`, C(n), C(s));
  ui('hake/knott (#fff) mot --accent-bg', [255, 255, 255, 1], C('--accent-bg'));
  // Scenen på Lys (v0.0.13): tekst på scenen og på merkene over den, også når
  // merket ligger over en lys del av bildet (#808080, f.eks. et opplyst kort),
  // og fokusringen rundt 3D-visningen.
  for (const f of ['--stage-text', '--stage-dim']) {
    text(`${f} på --stage`, C(f), C('--stage'));
    text(`${f} på --stage-chip over --stage`, C(f), solid('--stage-chip', '--stage'));
    text(`${f} på --stage-chip over lyst bilde (#808080)`, C(f), over(C('--stage-chip'), [128, 128, 128, 1]));
  }
  ui('--stage-focus mot --stage', C('--stage-focus'), C('--stage'));
  if (high) {
    // Sporet får kant (--border) i høy kontrast, så rammen er det som må synes.
    for (const s of ['--card', '--bg', '--sidebar']) ui(`--border mot ${s} (høy kontrast)`, C('--border'), C(s));
  }
  return list;
}

// ---- Kjør alle kombinasjoner ----
let fails = [];
let worstText = { r: 99 };
let worstUi = { r: 99 };
let n = 0;
let pairs = 0;
for (const palette of PALETTES)
  for (const mode of MODES)
    for (const accent of ACCENTS)
      for (const contrast of CONTRASTS) {
        const env = { 'data-palette': palette, 'data-resolved': mode, 'data-accent': accent, 'data-contrast': contrast };
        const tok = tokensFor(env);
        const name = `${palette}/${mode === 'light' ? 'lys' : 'mørk'}/${accent}${contrast === 'high' ? '/høy kontrast' : ''}`;
        n++;
        for (const c of checks(tok, contrast === 'high')) {
          pairs++;
          const item = { ...c, combo: name };
          if (c.kind === 'tekst' && c.r < worstText.r) worstText = item;
          if (c.kind === 'ui' && c.r < worstUi.r) worstUi = item;
          if (c.r < c.min - 1e-9) fails.push(item);
        }
      }

const f2 = (x) => x.toFixed(2).replace('.', ',');
console.log(`Kontrast: ${n} kombinasjoner (${PALETTES.length} fargetemaer × lys/mørk × ${ACCENTS.length} aksenter × vanlig/høy kontrast), ${pairs} par.`);
console.log(`Dårligste tekst: ${f2(worstText.r)}:1  (${worstText.label}, ${worstText.combo})`);
console.log(`Dårligste UI-grense: ${f2(worstUi.r)}:1  (${worstUi.label}, ${worstUi.combo})`);
if (fails.length) {
  fails.sort((x, y) => x.r - y.r);
  console.log(`\n${fails.length} par under kravet:`);
  const seen = new Set();
  for (const f of fails) {
    const key = `${f.label}|${f.combo.split('/').slice(0, 2).join('/')}|${f.combo.includes('høy') ? 'h' : ''}`;
    if (process.argv.includes('--alle') || !seen.has(key)) console.log(`  ${f2(f.r)}:1 < ${f2(f.min)}  ${f.label}  [${f.combo}]`);
    seen.add(key);
  }
  process.exit(1);
}
console.log('Alle par oppfyller WCAG AA (4,5:1 for tekst, 3:1 for UI-grenser).');
