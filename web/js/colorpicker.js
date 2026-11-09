// Egen fargevelger: metning/lyshet-flate, fargetone-glider, redigerbart
// hex-felt med validering og kuraterte fargeprøver. Ikke nettleserens dialog.

import { el, icon } from './util.js';
import { closePopover, fillRange, togglePopover } from './ui.js';

/** Nøkterne, rene LED-farger (ikke neon). */
export const SWATCHES = [
  ['#f2f2f2', 'Hvit'],
  ['#e5383b', 'Rød'],
  ['#f07a1a', 'Oransje'],
  ['#f2c12e', 'Gul'],
  ['#3fb950', 'Grønn'],
  ['#2bb3c8', 'Cyan'],
  ['#3a7bf0', 'Blå'],
  ['#8e5cf0', 'Lilla'],
  ['#e2649e', 'Rosa'],
];

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const toHex = (r, g, b) => `#${[r, g, b].map((v) => clamp(Math.round(v), 0, 255).toString(16).padStart(2, '0')).join('')}`;

/** Godtar «3584e4», «#3584E4», «#38e» og «rgb(53, 132, 228)». Gir #rrggbb eller null. */
export function parseColor(s) {
  const t = String(s || '').trim().toLowerCase();
  let m = /^#?([0-9a-f]{6})$/.exec(t);
  if (m) return `#${m[1]}`;
  m = /^#?([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(t);
  if (m) return `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}`;
  m = /^rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)$/.exec(t);
  if (m && [m[1], m[2], m[3]].every((x) => Number(x) <= 255)) return toHex(m[1], m[2], m[3]);
  return null;
}

function hexToHsv(hex) {
  const n = parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const d = mx - mn;
  let h = 0;
  if (d) {
    if (mx === r) h = ((g - b) / d) % 6;
    else if (mx === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  return [h, mx ? d / mx : 0, mx];
}

function hsvToHex(h, s, v) {
  const f = (n) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return toHex(f(5) * 255, f(3) * 255, f(1) * 255);
}

/**
 * Lager velgeren i host. onChange(hex) kalles mens du drar/skriver.
 * recent(): liste med nylig brukte farger.
 *
 * v0.0.13: slankere. De ni fargene står på én rad (20 px, 24 px treffflate),
 * med hex-feltet under. Metning/lyshet-flaten og fargetonen ligger bak
 * «Egendefinert …» i en popover, så velgeren tar ca. 110 px i stedet for 300.
 */
export function createPicker(host, { onChange, recent = () => [] }) {
  let hsv = [210, 0.75, 0.94];
  let hex = '#3a7bf0';

  const knob = el('span', { class: 'knob' });
  const sv = el('div', { class: 'sv', tabindex: '0', role: 'slider', 'aria-label': 'Metning og lyshet', 'aria-valuetext': '' }, knob);
  const hue = el('input', { type: 'range', class: 'hue', min: '0', max: '359', step: '1', 'aria-label': 'Fargetone' });
  const now = el('span', { class: 'swatch-now' });
  const field = el('input', { class: 'field', type: 'text', spellcheck: 'false', autocomplete: 'off', 'aria-label': 'Hex-farge', maxlength: '24' });
  const msg = el('span', { class: 'msg', 'aria-live': 'polite' });
  const sw = el('div', { class: 'swatches', role: 'radiogroup', 'aria-label': 'Fargeprøver' });
  const recentLabel = el('div', { class: 'swatch-label', text: 'Nylig brukt' });
  const rec = el('div', { class: 'swatches', 'aria-label': 'Nylig brukte farger' });
  const custom = el('button', { type: 'button', class: 'btn sm flat custom-btn', 'aria-haspopup': 'dialog', 'aria-expanded': 'false', 'data-tip': 'Velg en hvilken som helst farge' }, icon('palette'), 'Egendefinert …');
  const pop = el('div', { class: 'popover picker-pop', role: 'dialog', 'aria-label': 'Egendefinert farge' }, sv, hue);
  host.replaceChildren(sw, el('div', { class: 'hex-row' }, now, field, custom), msg, recentLabel, rec);
  custom.addEventListener('click', () => togglePopover(custom, pop, { align: 'end' }));

  const swatchBtn = (c, name) => {
    const b = el('button', { type: 'button', role: 'radio', 'data-c': c, 'data-tip': `${name} · ${c.toUpperCase()}`, 'aria-label': name, style: { '--sw': c } });
    b.addEventListener('click', () => pick(c, true));
    return b;
  };
  sw.append(...SWATCHES.map(([c, n]) => swatchBtn(c, n)));
  // Piltaster mellom fargeknappene (én tabulatorstopp for raden).
  sw.addEventListener('keydown', (e) => {
    const btns = [...sw.children];
    const i = btns.indexOf(document.activeElement);
    const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (i < 0 || !d) return;
    e.preventDefault();
    const n = btns[(i + d + btns.length) % btns.length];
    n.focus();
    pick(n.dataset.c, true);
  });

  function paint({ keepField = false } = {}) {
    sv.style.setProperty('--hue', String(Math.round(hsv[0])));
    knob.style.left = `${hsv[1] * 100}%`;
    knob.style.top = `${(1 - hsv[2]) * 100}%`;
    knob.style.background = hex;
    sv.setAttribute('aria-valuetext', `Metning ${Math.round(hsv[1] * 100)} %, lyshet ${Math.round(hsv[2] * 100)} %`);
    hue.value = String(Math.round(hsv[0]));
    fillRange(hue);
    hue.style.setProperty('--p', '0%');
    now.style.setProperty('--sw', hex);
    if (!keepField) {
      field.value = hex.toUpperCase();
      delete field.dataset.invalid;
      msg.textContent = '';
    }
    const btns = [...sw.children];
    const on = btns.find((b) => b.dataset.c === hex);
    for (const b of btns) {
      b.setAttribute('aria-checked', String(b === on));
      b.tabIndex = b === (on || btns[0]) ? 0 : -1;
    }
    custom.setAttribute('aria-pressed', String(!on));
    const r = recent().filter((c) => c !== hex);
    recentLabel.hidden = !r.length;
    rec.replaceChildren(...r.slice(0, 6).map((c) => swatchBtn(c, 'Nylig brukt')));
  }

  function pick(c, fromSwatch = false, opts = {}) {
    hex = c;
    hsv = hexToHsv(c);
    paint(opts);
    onChange(hex, { fromSwatch });
  }

  function fromSv(e) {
    const r = sv.getBoundingClientRect();
    hsv[1] = clamp((e.clientX - r.left) / r.width, 0, 1);
    hsv[2] = clamp(1 - (e.clientY - r.top) / r.height, 0, 1);
    hex = hsvToHex(...hsv);
    paint();
    onChange(hex, {});
  }
  sv.addEventListener('pointerdown', (e) => {
    sv.setPointerCapture(e.pointerId);
    fromSv(e);
    const move = (ev) => fromSv(ev);
    const up = () => {
      sv.removeEventListener('pointermove', move);
      sv.removeEventListener('pointerup', up);
    };
    sv.addEventListener('pointermove', move);
    sv.addEventListener('pointerup', up);
  });
  sv.addEventListener('keydown', (e) => {
    const st = e.shiftKey ? 0.1 : 0.02;
    const map = { ArrowLeft: [-st, 0], ArrowRight: [st, 0], ArrowUp: [0, st], ArrowDown: [0, -st] };
    if (!map[e.key]) return;
    hsv[1] = clamp(hsv[1] + map[e.key][0], 0, 1);
    hsv[2] = clamp(hsv[2] + map[e.key][1], 0, 1);
    hex = hsvToHex(...hsv);
    paint();
    onChange(hex, {});
    e.preventDefault();
  });
  hue.addEventListener('input', () => {
    hsv[0] = Number(hue.value);
    hex = hsvToHex(...hsv);
    paint();
    onChange(hex, {});
  });
  const commitField = () => {
    const c = parseColor(field.value);
    if (!c) {
      field.dataset.invalid = '';
      msg.textContent = 'Skriv f.eks. #3A7BF0';
      return;
    }
    pick(c, false);
  };
  field.addEventListener('input', () => {
    const c = parseColor(field.value);
    if (c) {
      delete field.dataset.invalid;
      msg.textContent = '';
      pick(c, false, { keepField: true });
    }
  });
  field.addEventListener('change', commitField);
  field.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') commitField();
  });

  return {
    set(c) {
      if (!c || c === hex) return paint();
      hex = c;
      hsv = hexToHsv(c);
      paint();
    },
    refreshRecent: () => paint({ keepField: document.activeElement === field }),
    /** Lukker «Egendefinert …» hvis den er åpen. */
    close: () => {
      if (pop.dataset.open !== undefined) closePopover();
    },
    icon,
  };
}
