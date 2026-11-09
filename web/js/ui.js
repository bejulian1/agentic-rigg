// Felles UI-biter: varsler, segmentkontroller, brytere, glidere,
// verktøytips, popover/meny, dialoger og små bekreftelser.
//
// Bevegelse (docs/bevegelse.md):
//   .toast              toner inn med @starting-style. Ved lukking får den
//                       .is-leaving og fjernes når overgangen er ferdig.
//                       Stabelen glir på plass med FLIP (flip()).
//   .seg                markøren (.seg-ind) glir mellom valg med FLIP.
//   .tooltip/.popover   [data-open] styrer inn/ut i CSS. Popover skaleres
//                       fra knappen den hører til (transform-origin).
//   dialog.dialog       inn/ut i CSS (@starting-style og allow-discrete).
//   confirmDone()       hake på knappen i 1,2 s etter kopier/lagre.
//   announce()          kort melding til skjermlesere (én høflig region).

import { $, $$, el, icon, motion, dur, EASE } from './util.js';

// ---------------------------------------------------------------- Skjermleser

/** Leser opp en kort melding (aria-live=polite). Brukes med måte. */
export function announce(text) {
  const live = $('#live');
  if (!live || !text) return;
  live.textContent = '';
  requestAnimationFrame(() => (live.textContent = text));
}

// ---------------------------------------------------------------- FLIP

/**
 * Lar søsken gli til ny plass når mutate() legger til eller fjerner noe
 * (First–Last–Invert–Play). Bare transform, bare ved full bevegelse.
 */
export function flip(host, mutate) {
  if (!host || motion() !== 'full') return mutate();
  const kids = [...host.children];
  const before = new Map(kids.map((k) => [k, k.getBoundingClientRect().top]));
  mutate();
  const d = dur(3);
  for (const [k, top] of before) {
    if (!k.isConnected || k.classList.contains('is-leaving')) continue;
    const dy = top - k.getBoundingClientRect().top;
    if (Math.abs(dy) > 0.5) k.animate([{ transform: `translateY(${dy}px)` }, { transform: 'translateY(0)' }], { duration: d, easing: EASE.out, composite: 'add' });
  }
}

// ---------------------------------------------------------------- Varsler

/**
 * Viser et varsel nede til høyre.
 * opts: body, error, ms (standard 4 s, 6 s med handling), action: { label, run }
 */
export function toast(title, { body = '', error = false, ms = null, action = null, icon: ic = null } = {}) {
  const host = $('#toasts');
  const t = el(
    'div',
    { class: `toast${error ? ' err' : ''}`, role: error ? 'alert' : 'status' },
    icon(error ? 'circle-alert' : ic || 'circle-check'), // ic: v0.0.12, egg-ikonet for påskeeggene (easter.js)
    el('div', { class: 't-text' }, el('div', { class: 't-title', text: title }), body ? el('div', { class: 't-body', text: body }) : null),
  );
  let timer = null;
  const close = () => closeToast(t, timer);
  if (action) {
    const b = el('button', { type: 'button', class: 'btn sm', text: action.label });
    b.addEventListener('click', () => {
      close();
      action.run();
    });
    t.append(b);
  }
  const x = el('button', { type: 'button', class: 'btn sm flat icon-only', 'aria-label': 'Lukk', 'data-tip': 'Lukk' }, icon('x'));
  x.addEventListener('click', close);
  t.append(x);
  flip(host, () => host.append(t));
  // Høyst fire synlige; de eldste går ut (de som allerede er på vei ut telles ikke).
  const live = [...host.children].filter((c) => !c.classList.contains('is-leaving'));
  for (const old of live.slice(0, Math.max(0, live.length - 4))) closeToast(old);
  timer = setTimeout(close, ms ?? (action ? 6000 : 4000));
  // Pause nedtellingen mens pekeren er over varselet.
  t.addEventListener('pointerenter', () => clearTimeout(timer));
  t.addEventListener('pointerleave', () => {
    clearTimeout(timer);
    timer = setTimeout(close, 2500);
  });
  return { close };
}

export function closeToast(t, timer) {
  if (!t || t.classList.contains('is-leaving')) return;
  clearTimeout(timer);
  if (motion() === 'off' || !t.isConnected) return t.remove();
  // Fokus skal ikke forsvinne med varselet (f.eks. etter «Angre» med tastatur).
  const hadFocus = t.contains(document.activeElement);
  t.classList.add('is-leaving');
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    const host = t.parentElement;
    if (host) flip(host, () => t.remove());
    if (hadFocus) $('#content')?.focus({ preventScroll: true });
  };
  // Bare varselets egen opasitet teller (knappene inni har egne overganger).
  t.addEventListener('transitionend', (e) => e.target === t && e.propertyName === 'opacity' && finish());
  setTimeout(finish, dur(3) + 150);
}

// ---------------------------------------------------------------- Segment

/** Segmentkontroll: knapper med data-v og role=radio. Piltaster flytter valget. */
export function bindSeg(seg, onPick) {
  seg.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-v]');
    if (!b || b.disabled) return;
    onPick(b.dataset.v);
  });
  seg.addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) return;
    const btns = [...seg.querySelectorAll('button[data-v]:not(:disabled)')];
    if (!btns.length) return;
    const i = btns.findIndex((b) => b.getAttribute('aria-checked') === 'true');
    let n;
    if (e.key === 'Home') n = btns[0];
    else if (e.key === 'End') n = btns[btns.length - 1];
    else n = btns[(i + (e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1) + btns.length) % btns.length];
    n.focus();
    onPick(n.dataset.v);
    e.preventDefault();
  });
}

export function setSeg(seg, value) {
  let any = false;
  for (const b of seg.querySelectorAll('button[data-v]')) {
    const on = b.dataset.v === String(value);
    any ||= on;
    if (b.getAttribute('aria-checked') !== String(on)) b.setAttribute('aria-checked', on ? 'true' : 'false');
    b.tabIndex = on ? 0 : -1;
  }
  if (!any) seg.querySelector('button[data-v]')?.setAttribute('tabindex', '0');
  if (seg.classList.contains('seg')) {
    ensureInd(seg);
    placeInd(seg, true);
  }
}

// Markøren i segmentkontrollen: ett element bak knappene. Ny plass settes
// én gang (bredde og translateX), og overgangen spilles som FLIP med bare
// transform: fra gammel plass og skalert bredde til ny. ResizeObserver
// plasserer den på nytt uten animasjon når kontrollen blir synlig eller
// endrer størrelse (skrift, tetthet).
const segRO =
  typeof ResizeObserver === 'function'
    ? new ResizeObserver((entries) => {
        for (const e of entries) {
          // Kontroller som er fjernet (f.eks. når Innstillinger tegnes om), slippes.
          if (!e.target.isConnected) segRO.unobserve(e.target);
          else placeInd(e.target, false);
        }
      })
    : null;

function ensureInd(seg) {
  if (seg._ind) return;
  for (const b of seg.querySelectorAll('button[data-v]')) if (!b.dataset.text && !b.children.length) b.dataset.text = b.textContent.trim();
  const ind = el('span', { class: 'seg-ind', 'aria-hidden': 'true' });
  seg.prepend(ind);
  seg._ind = ind;
  seg._pos = null;
  segRO?.observe(seg);
}

function placeInd(seg, animated) {
  const ind = seg._ind;
  if (!ind) return;
  const b = seg.querySelector('button[data-v][aria-checked="true"]');
  if (!b) {
    delete seg.dataset.ind;
    seg._pos = null;
    return;
  }
  if (!seg.offsetWidth) return; // skjult: plasseres når den blir synlig
  // Mål uten transform (popover kan være midt i skaleringen).
  const sr = seg.getBoundingClientRect();
  const k = sr.width / seg.offsetWidth || 1;
  const br = b.getBoundingClientRect();
  const x = Math.round(((br.left - sr.left) / k) * 100) / 100;
  const w = Math.round((br.width / k) * 100) / 100;
  const prev = seg._pos;
  if (prev && Math.abs(prev.x - x) < 0.01 && Math.abs(prev.w - w) < 0.01) return;
  seg._pos = { x, w };
  ind.style.width = `${w}px`;
  ind.style.transform = `translateX(${x}px)`;
  seg.dataset.ind = '';
  if (animated && prev && motion() === 'full') {
    ind.animate([{ transform: `translateX(${prev.x}px) scaleX(${prev.w / w})` }, { transform: `translateX(${x}px) scaleX(1)` }], { duration: dur(2), easing: EASE.out });
  }
}

/** Bygger en segmentkontroll. options: [[verdi, tekst, tooltip?], …] */
export function segEl(options, value, onPick, { label = '', size = '' } = {}) {
  const s = el(
    'div',
    { class: `seg${size ? ` ${size}` : ''}`, role: 'radiogroup', 'aria-label': label },
    ...options.map(([v, t, tip]) => el('button', { type: 'button', role: 'radio', 'data-v': String(v), 'data-tip': tip || null, text: t })),
  );
  setSeg(s, value);
  bindSeg(s, (v) => {
    setSeg(s, v);
    onPick(v);
  });
  return s;
}

/** Bryter. Returnerer { root, input }. */
export function switchEl(checked, onChange, { label = '' } = {}) {
  const input = el('input', { type: 'checkbox', 'aria-label': label || null });
  input.checked = !!checked;
  input.addEventListener('change', () => onChange(input.checked));
  const root = el('span', { class: 'switch' }, input, el('span', { class: 'track' }), el('span', { class: 'knob' }));
  return { root, input };
}

/** Oppdaterer fylt del av en glider. */
export function fillRange(input) {
  const min = Number(input.min);
  const max = Number(input.max);
  const p = (Number(input.value) - min) / (max - min);
  // Fyllet slutter midt under knotten (14 px bred, se «Glider» i styles.css).
  input.style.setProperty('--p', `calc(7px + (100% - 14px) * ${Math.round(p * 10000) / 10000})`);
}

/**
 * Lite tallfelt med enhet. clamp: false lar verdier utenfor min/maks stå
 * (Kontroller: bakgrunnstjenesten klemmer og sier fra), ellers legges de
 * innenfor når feltet forlates.
 */
export function numField({ min, max, step = 1, value, unit = '', label = '', size = '', clamp = true, onInput, onCommit }) {
  const input = el('input', { type: 'number', min, max, step, inputmode: 'numeric', 'aria-label': label });
  input.value = String(value);
  const root = el('label', { class: `numfield${size ? ` ${size}` : ''}` }, input, unit ? el('span', { class: 'u', text: unit }) : null);
  input.addEventListener('input', () => {
    const v = Number(input.value);
    if (input.value === '' || !Number.isFinite(v)) return;
    onInput?.(v);
  });
  input.addEventListener('change', () => {
    // Grensene leses fra feltet, så sider kan endre dem (f.eks. ved °C/°F).
    const lo = input.min === '' ? -Infinity : Number(input.min);
    const hi = input.max === '' ? Infinity : Number(input.max);
    let v = Number(input.value);
    if (input.value.trim() === '' || !Number.isFinite(v)) v = Number.isFinite(lo) ? lo : 0;
    if (clamp) v = Math.min(hi, Math.max(lo, v));
    input.value = String(v);
    onCommit?.(v);
  });
  return { root, input };
}

// ---------------------------------------------------------------- Verktøytips
// Egen komponent i stedet for title: 500 ms ventetid første gang, straks og
// uten overgang på neste element (gruppe-varme) og alltid synlig ved
// tastaturfokus. Teksten står i data-tip, og en hurtigtast kan stå i
// data-tip-key. Avkortet tekst (ellipsis) får hele teksten i tipset, også
// der elementet ikke har data-tip.

let tipEl = null;
let tipTarget = null;
let tipTimer = null;
let warmUntil = 0;

// Elementer som kan bli avkortet med ellipsis.
const TRUNC = '.ellipsis, .n, .note, .sub, .s-sum, .path-box code, .main-title, .app-name, .tile-value';
const isTrunc = (n) => n && n.scrollWidth > n.clientWidth + 1 && getComputedStyle(n).textOverflow === 'ellipsis';
/** Første avkortede tekst i eller på target, eller null. */
function truncatedIn(target) {
  if (isTrunc(target)) return target;
  for (const n of target.querySelectorAll(TRUNC)) if (isTrunc(n)) return n;
  return null;
}

function placeTip(target) {
  const r = target.getBoundingClientRect();
  // offsetWidth/-Height: målt uten transform (tipset glir 2 px når det åpnes).
  const tw = tipEl.offsetWidth;
  const th = tipEl.offsetHeight;
  const side = target.dataset.tipSide;
  let x;
  let y;
  let dx = '0px';
  let dy = '-2px';
  if (side === 'right') {
    x = r.right + 8;
    y = r.top + r.height / 2 - th / 2;
    dx = '-2px';
    dy = '0px';
  } else {
    x = r.left + r.width / 2 - tw / 2;
    y = r.bottom + 6;
    if (y + th > window.innerHeight - 4) {
      y = r.top - th - 6;
      dy = '2px';
    }
  }
  x = Math.max(4, Math.min(window.innerWidth - tw - 4, x));
  y = Math.max(4, y);
  tipEl.style.left = `${Math.round(x)}px`;
  tipEl.style.top = `${Math.round(y)}px`;
  tipEl.style.setProperty('--tip-dx', dx);
  tipEl.style.setProperty('--tip-dy', dy);
}

function tipText(target) {
  let text = target.dataset.tip || '';
  if (text === '–') text = '';
  const tr = truncatedIn(target);
  const full = tr?.textContent.trim();
  if (full && !text.includes(full)) text = text ? `${full}\n${text}` : full;
  return text;
}

function showTip(target, instant = false) {
  const text = tipText(target);
  if (!text) return;
  // I det brede sidefeltet står navnet allerede; vis bare tips når navnet er skjult.
  if (target.classList.contains('nav-item') && getComputedStyle(target.querySelector('.label')).display !== 'none' && document.documentElement.dataset.kbd === 'on') return;
  tipTarget = target;
  if (instant) tipEl.dataset.instant = '';
  tipEl.replaceChildren(text);
  if (target.dataset.tipKey) tipEl.append(el('kbd', { text: target.dataset.tipKey }));
  tipEl.dataset.open = '';
  placeTip(target);
  if (instant) requestAnimationFrame(() => requestAnimationFrame(() => delete tipEl.dataset.instant));
}

function hideTip() {
  clearTimeout(tipTimer);
  if (tipTarget) warmUntil = performance.now() + 300;
  tipTarget = null;
  if (tipEl) delete tipEl.dataset.open;
}

/** Elementet som skal ha verktøytips for et pekermål: data-tip, eller avkortet tekst. */
function tipHost(node) {
  const t = node.closest?.('[data-tip]');
  if (t) return t;
  const tr = node.closest?.(TRUNC);
  return tr && isTrunc(tr) ? tr : null;
}

/**
 * Ikonknapper uten tekst får navnet fra verktøytipset, så skjermlesere ikke
 * møter en tom knapp. Merkes med data-auto-label, så setTip() kan holde det likt.
 */
export function labelIconButtons(root = document) {
  for (const b of root.querySelectorAll('.icon-only[data-tip]:not([aria-label]), .info-btn[data-tip]:not([aria-label])')) {
    b.setAttribute('aria-label', b.dataset.tip);
    b.dataset.autoLabel = '';
  }
}

export function initTooltips() {
  tipEl = $('#tooltip');
  labelIconButtons();
  const enter = (target, delay) => {
    clearTimeout(tipTimer);
    // Varm gruppe: neste nabo får tipset straks og uten overgang.
    if (performance.now() < warmUntil || tipTarget) {
      showTip(target, true);
      return;
    }
    tipTimer = setTimeout(() => showTip(target), delay);
  };
  document.addEventListener('pointerover', (e) => {
    const t = tipHost(e.target);
    if (!t) return;
    if (t === tipTarget) return;
    if (tipTarget) hideTip();
    enter(t, 500);
  });
  document.addEventListener('pointerout', (e) => {
    const t = tipHost(e.target);
    if (!t || (e.relatedTarget && t.contains(e.relatedTarget))) return;
    hideTip();
  });
  document.addEventListener('focusin', (e) => {
    const t = e.target.closest?.('[data-tip]');
    if (t && e.target.matches(':focus-visible')) enter(t, 300);
  });
  document.addEventListener('focusout', hideTip);
  document.addEventListener('pointerdown', hideTip, true);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') hideTip();
  });
  document.addEventListener('scroll', hideTip, true);
}

/** Endrer teksten i et verktøytips (og oppdaterer det hvis det vises). */
export function setTip(node, text) {
  if (!node) return;
  if (text) node.dataset.tip = text;
  else delete node.dataset.tip;
  if (text && node.dataset.autoLabel !== undefined) node.setAttribute('aria-label', text);
  if (tipTarget === node) {
    if (text) showTip(node);
    else hideTip();
  }
}

// ---------------------------------------------------------------- Popover og meny

let openPop = null;

function placePop(anchor, pop, align) {
  const r = anchor.getBoundingClientRect();
  // offsetWidth/-Height er uten transform (popoveren skaleres når den åpnes).
  const pw = pop.offsetWidth;
  const ph = pop.offsetHeight;
  let x = align === 'end' ? r.right - pw : r.left;
  let y = r.bottom + 6;
  const above = y + ph > window.innerHeight - 8;
  if (above) y = Math.max(8, r.top - ph - 6);
  x = Math.max(8, Math.min(window.innerWidth - pw - 8, x));
  x = Math.round(x);
  y = Math.round(y);
  pop.style.left = `${x}px`;
  pop.style.top = `${y}px`;
  // Skaleres fra knappen: midt på knappen vannrett, kanten mot knappen loddrett.
  const ox = Math.max(0, Math.min(pw, r.left + r.width / 2 - x));
  pop.style.transformOrigin = `${Math.round(ox)}px ${above ? ph : 0}px`;
}

export function closePopover(focusAnchor = false) {
  if (!openPop) return;
  const { pop, anchor, onClose } = openPop;
  delete pop.dataset.open;
  anchor.setAttribute('aria-expanded', 'false');
  openPop = null;
  onClose?.();
  if (focusAnchor) anchor.focus();
}

/** Åpner en popover under anchor. Lukkes med klikk utenfor, Esc og ved valg i menyer. */
export function openPopover(anchor, pop, { align = 'start', onClose = null } = {}) {
  if (openPop?.pop === pop) return closePopover();
  closePopover();
  if (pop.parentElement !== document.body) document.body.append(pop);
  // Plasser før åpning, så skaleringen starter fra riktig punkt.
  placePop(anchor, pop, align);
  pop.dataset.open = '';
  anchor.setAttribute('aria-expanded', 'true');
  openPop = { pop, anchor, onClose };
  const first = pop.querySelector('[role=menuitem], [role=menuitemcheckbox], button, a, input');
  first?.focus({ preventScroll: true });
}

export function togglePopover(anchor, pop, opts) {
  if (openPop?.pop === pop) closePopover();
  else openPopover(anchor, pop, opts);
}

export function initPopovers() {
  document.addEventListener('pointerdown', (e) => {
    if (!openPop) return;
    if (openPop.pop.contains(e.target) || openPop.anchor.contains(e.target)) return;
    closePopover();
  });
  document.addEventListener('keydown', (e) => {
    if (!openPop) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closePopover(true);
      return;
    }
    if (openPop.pop.getAttribute('role') === 'menu' && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
      // Deaktiverte valg hoppes over, ellers stopper piltastene på dem.
      const items = $$('[role^=menuitem]:not(:disabled)', openPop.pop);
      if (!items.length) return;
      const i = items.indexOf(document.activeElement);
      const n = e.key === 'Home' ? items[0] : e.key === 'End' ? items[items.length - 1] : items[(i + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length];
      n?.focus();
      e.preventDefault();
    }
  });
  // Tab ut av en popover lukker den (ellers blir den hengende åpen bak fokus).
  document.addEventListener('focusin', (e) => {
    if (!openPop) return;
    if (openPop.pop.contains(e.target) || openPop.anchor.contains(e.target)) return;
    closePopover();
  });
  window.addEventListener('resize', () => closePopover());
  document.getElementById('content')?.addEventListener('scroll', () => closePopover(), { passive: true });
}

/**
 * Lager en meny. items: { label, icon?, kbd?, run } | { sep: true } | { title } |
 * { label, checked, toggle } (avkrysning, menyen holdes åpen).
 */
export function menu(items, { label = '' } = {}) {
  const pop = el('div', { class: 'popover', role: 'menu', 'aria-label': label });
  const render = () => {
    pop.replaceChildren(
      ...items.map((it) => {
        if (it.sep) return el('div', { class: 'menu-sep' });
        if (it.title) return el('div', { class: 'menu-label', text: it.title });
        if (it.toggle) {
          const on = it.checked();
          const b = el(
            'button',
            { type: 'button', class: 'menu-item', role: 'menuitemcheckbox', 'aria-checked': on ? 'true' : 'false', disabled: it.disabled?.() || null },
            el('input', { type: 'checkbox', class: 'check', tabindex: '-1', 'aria-hidden': 'true' }),
            it.label,
          );
          b.querySelector('input').checked = on;
          b.addEventListener('click', () => {
            it.toggle(!on);
            render();
            pop.querySelectorAll('[role^=menuitem]')[items.filter((x) => !x.sep && !x.title).indexOf(it)]?.focus();
          });
          return b;
        }
        const b = el('button', { type: 'button', class: 'menu-item', role: 'menuitem' }, it.icon ? icon(it.icon) : null, it.label, it.kbd ? el('kbd', { text: it.kbd }) : null);
        b.addEventListener('click', () => {
          closePopover(true);
          it.run();
        });
        return b;
      }),
    );
  };
  render();
  pop.rerender = render;
  return pop;
}

// ---------------------------------------------------------------- Dialoger

export function openDialog(dlg) {
  if (!dlg.dataset.bound) {
    dlg.dataset.bound = '1';
    dlg.addEventListener('click', (e) => {
      if (e.target.closest('[data-close]') || e.target === dlg) dlg.close();
    });
    // Fokus tilbake til det som åpnet dialogen (Esc, × eller klikk utenfor).
    // Nettleseren gir fokus tilbake til det som hadde fokus før showModal(),
    // men ikke hvis det var et valg i en meny som nå er lukket. Hendelsen
    // kommer litt etter; har fokus gått videre, får det være.
    dlg.addEventListener('close', () => {
      const back = dlg._returnFocus;
      dlg._returnFocus = null;
      const now = document.activeElement;
      const lost = !now || now === document.body || dlg.contains(now) || !now.getClientRects().length;
      if (lost && back?.isConnected && back.getClientRects().length) back.focus({ preventScroll: true });
    });
  }
  // Fra en meny: knappen som åpnet menyen. Ellers det som hadde fokus.
  const from = openPop?.anchor || document.activeElement;
  closePopover();
  if (!dlg.open) {
    dlg._returnFocus = from && from !== document.body && !from.closest('.popover') ? from : null;
    dlg.showModal();
  }
}

// ---------------------------------------------------------------- Bekreftelse

/**
 * Viser en hake på knappen i 1,2 s (kopiert, lagret). Knappen beholder
 * bredden. tip: midlertidig verktøytips og melding til skjermleser.
 */
export function confirmDone(btn, { tip = null, ms = 1200 } = {}) {
  if (!btn) return;
  let mark = btn.querySelector(':scope > .done-mark');
  if (!mark) {
    mark = el('span', { class: 'done-mark', 'aria-hidden': 'true' }, icon('check'));
    btn.append(mark);
  }
  if (!btn.classList.contains('is-done')) btn._tipBefore = btn.dataset.tip ?? null;
  btn.classList.add('is-done');
  if (tip) {
    setTip(btn, tip);
    announce(tip);
  }
  clearTimeout(btn._doneTimer);
  btn._doneTimer = setTimeout(() => {
    btn.classList.remove('is-done');
    if (tip && btn.dataset.tip === tip) setTip(btn, btn._tipBefore);
  }, ms);
}
