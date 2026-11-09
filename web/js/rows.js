// Etikett/verdi-rader (K1): etikett i --muted til venstre, verdien som ren,
// markerbar tekst til høyre. Ingen felt-utseende – det er bare for ting du
// kan skrive i.

import { el, icon } from './util.js';

export const NA = 'Ikke oppgitt av driveren';

/**
 * Legger til en rad i en <dl class="kv">. value kan være tekst, tall, node eller null.
 * opts: na (dempet), tip (verktøytips på verdien), info (forklaring bak et (i)-ikon),
 *       id, copy (fulltekst til «Kopier som tekst» når verdien er forkortet), sel ('all'),
 *       naText (tekst når verdien mangler), skel (bredde på en plassholder, f.eks.
 *       '6em', som vises til første verdi kommer, så ingenting hopper)
 */
export function kv(host, label, value, { na = false, tip = null, info = null, id = null, copy = null, sel = null, naText = NA, cls = '', skel = null } = {}) {
  const missing = value == null || value === '';
  const dd = el('dd', { id, class: [na || missing ? 'na' : '', cls].filter(Boolean).join(' ') || null, 'data-tip': tip, 'data-copy': copy });
  const v = el('span', { class: sel === 'all' ? 'sel-all' : null });
  if (missing && skel) v.append(el('span', { class: 'skel', style: { '--w': skel } }));
  else if (missing) v.textContent = naText;
  else if (typeof value === 'string' || typeof value === 'number') v.textContent = String(value);
  else v.append(value);
  dd.append(v);
  if (info) dd.append(infoBtn(info));
  const row = el('div', { class: 'kv-row' }, el('dt', { text: label }), dd);
  host.append(row);
  return dd;
}

/** Underoverskrift i en kv-liste. */
export function kvSub(host, text) {
  host.append(el('div', { class: 'kv-sub', text }));
}

export function infoBtn(text, name = 'info') {
  return el('button', { type: 'button', class: 'info-btn', 'data-tip': text, 'aria-label': text }, icon(name));
}

/** Setter verdien i en dd laget av kv() uten å røre (i)-knappen. */
export function setKv(dd, text, { na = false } = {}) {
  if (!dd) return;
  const v = dd.firstElementChild;
  if (v && v.textContent !== text) v.textContent = text;
  dd.classList.toggle('na', na);
}

// ---- Plassholdere (skjelett) før første data ----
// Rolige grå blokker i samme høyde som de ekte radene (ingen skimmer), så
// ingenting hopper når dataene kommer. Bredden varierer litt så det ser ut
// som tekst.
const W_LABEL = ['7em', '9em', '6em', '8em', '10em', '7.5em'];
const W_VALUE = ['6em', '9em', '4em', '7em', '11em', '5em', '8em'];
const skel = (w) => el('span', { class: 'skel', style: { '--w': w } });

/** Fyller en <dl class="kv"> med n plassholderrader (og underoverskrifter før indeksene i subs). */
export function skelKv(host, n, subs = []) {
  host.replaceChildren();
  for (let i = 0; i < n; i++) {
    if (subs.includes(i)) host.append(el('div', { class: 'kv-sub' }, skel('5em')));
    host.append(el('div', { class: 'kv-row', 'aria-hidden': 'true' }, el('dt', {}, skel(W_LABEL[i % W_LABEL.length])), el('dd', {}, skel(W_VALUE[(i * 3) % W_VALUE.length]))));
  }
}

/** Fyller en tabell med plassholdere: én overskriftsrad og n rader med cols kolonner. */
export function skelTable(table, n, cols) {
  const row = (tag, i) => el('tr', { 'aria-hidden': 'true' }, ...Array.from({ length: cols }, (_, c) => el(tag, { 'data-align': c ? 'end' : null }, skel(c ? '3em' : W_LABEL[(i + c) % W_LABEL.length]))));
  table.replaceChildren(el('thead', {}, row('th', 0)), el('tbody', {}, ...Array.from({ length: n }, (_, i) => row('td', i + 1))));
}
