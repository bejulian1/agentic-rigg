// Et graf-panel (v0.0.11): overskrift med forklaring (klikk på en serie for å
// skjule eller vise den), nå-verdi og min/maks, pauseknapp og selve grafen.
// Brukes på Oversikt, Grafer, Kontroller og under en rad på Sensorer, så alle
// grafene ser like ut og oppfører seg likt.
//
// Pause: knappen (eller mellomrom når grafen har fokus) fryser grafen. Mens
// den står stille, sier overskriften «På pause · 14:32:10», og knappen heter
// Fortsett. Dataene samles videre i bakgrunnen; Fortsett hopper til nå.

import { el, icon, setText } from './util.js';
import * as F from './fmt.js';
import { setTip, announce } from './ui.js';
import { LineChart } from './charts.js';
import { HistoryView, WINDOWS, windowLabel } from './history.js';

/** Liten nedtrekksliste for tidsvindu. */
export function windowSelect(value, onPick, { label = 'Tidsvindu' } = {}) {
  const sel = el('select', { class: 'field sm win-select', 'aria-label': label, 'data-tip': label });
  for (const [v, name] of WINDOWS) sel.append(el('option', { value: String(v), text: name }));
  sel.value = String(value);
  sel.addEventListener('change', () => onPick(Number(sel.value)));
  return sel;
}

/**
 * opts: title, series (hs(...)), keys (nøkler til historikken), window,
 * chart (resten av LineChart-valgene), stats: bool (min/maks i vinduet),
 * now: (p) => tekst for siste verdi, extra: element i overskriften,
 * tag: 'section' | 'div', head: bool (false = ingen panelstripe).
 */
export function chartPanel(app, opts) {
  const view = new HistoryView(app.feed, { keys: opts.keys, window: opts.window, fetchFn: app.api.history });
  const canvas = el('canvas', { tabindex: '0', role: 'img', 'aria-label': `Graf: ${opts.title}, ${windowLabel(opts.window)}. Mellomrom setter på pause.` });
  const chart = new LineChart(canvas, { ...opts.chart, view, series: opts.series, syncOn: () => app.settings.chartSync !== false });

  // Forklaring: én knapp per serie. Fargen er seriens faste farge.
  const legend = el('span', { class: 'legend' });
  if (opts.series.length > 1 || opts.legend) {
    for (const s of opts.series) {
      const b = el('button', { type: 'button', class: 'lg-btn', 'aria-pressed': 'true', 'data-tip': `Skjul ${s.label.toLowerCase()}` }, el('i', { class: `lg${s.dashed ? ' dashed' : ''}`, style: { '--lg': `var(--${s.color})` } }), s.label);
      b.addEventListener('click', () => {
        const on = b.getAttribute('aria-pressed') === 'true';
        // Minst én serie skal synes.
        if (on && opts.series.filter((x) => !chart.hidden.has(x.key)).length <= 1) return;
        chart.setHidden(s.key, on);
        b.setAttribute('aria-pressed', String(!on));
        setTip(b, `${on ? 'Vis' : 'Skjul'} ${s.label.toLowerCase()}`);
      });
      legend.append(b);
    }
  }

  const stats = el('span', { class: 'stats' });
  const nowEl = el('span', { class: 'now' });
  const pausedTag = el('span', { class: 'tag paused-tag', hidden: true });
  const pauseBtn = el('button', { type: 'button', class: 'btn sm flat pause-btn icon-only', 'aria-pressed': 'false', 'aria-label': 'Pause', 'data-tip': 'Pause', 'data-tip-key': 'Mellomrom' }, icon('pause'), el('span', { class: 'pb-text', text: 'Fortsett' }));
  pauseBtn.addEventListener('click', () => setPaused(!chart.paused));
  canvas.addEventListener('keydown', (e) => {
    if (e.key === ' ' || e.key === 'Spacebar') {
      e.preventDefault();
      e.stopPropagation();
      setPaused(!chart.paused);
    }
  });

  function setPaused(p, { quiet = false } = {}) {
    if (p === chart.paused) return;
    chart.setPaused(p);
    pauseBtn.setAttribute('aria-pressed', String(p));
    pauseBtn.classList.toggle('icon-only', !p);
    pauseBtn.querySelector('use').setAttribute('href', `#i-${p ? 'play' : 'pause'}`);
    pauseBtn.setAttribute('aria-label', p ? 'Fortsett' : 'Pause');
    setTip(pauseBtn, p ? 'Fortsett og hopp til nå' : 'Pause');
    pausedTag.hidden = !p;
    if (p) setText(pausedTag, `På pause · ${F.time(new Date(chart.pausedAt + chart.o.lagMs))}`);
    if (!quiet) announce(p ? `${opts.title}: på pause.` : `${opts.title}: går igjen.`);
    panel.onPause?.(p);
    renderNumbers();
  }

  const head = el('div', { class: 'group-head' }, el('h2', { class: 'group-title', text: opts.title }), legend, el('div', { class: 'aside' }, pausedTag, stats, nowEl, opts.extra || null, pauseBtn));
  const body = el('div', { class: 'group-body' }, el('div', { class: 'chart' }, canvas));
  const root = el(opts.tag || 'section', { class: `group chart-card${opts.className ? ` ${opts.className}` : ''}` }, head, body);

  /** Min/maks i vinduet (fra båndet når det finnes) og siste verdi, for hovedserien. */
  function renderNumbers() {
    const s0 = opts.series[0];
    const pts = chart.paused && chart.snap ? chart.snap : view.data;
    const d = chart.o.decimals ?? 0;
    if (opts.stats !== false) {
      const t0 = (chart.paused ? chart.pausedAt : Date.now()) - view.window * 1000;
      let lo = Infinity;
      let hi = -Infinity;
      for (let i = pts.length - 1; i >= 0; i--) {
        const p = pts[i];
        if (p.t < t0) break;
        const a = s0.lo(p) ?? s0.get(p);
        const b = s0.hi(p) ?? s0.get(p);
        if (a != null && a < lo) lo = a;
        if (b != null && b > hi) hi = b;
      }
      setText(stats, Number.isFinite(lo) ? `min ${F.num(lo, d)} · maks ${F.num(hi, d)}` : '');
    }
    if (opts.now !== false) {
      const last = pts[pts.length - 1];
      const v = last ? s0.get(last) : null;
      const unit = chart.o.unit?.() || '';
      nowEl.replaceChildren(v == null ? '–' : F.num(v, d), v == null || !unit ? '' : el('span', { class: 'unit', text: `${F.NNBSP}${unit}` }));
    }
  }

  // Tallene i overskriften følger dataene (høyst to ganger i sekundet).
  let lastNum = 0;
  const numTick = () => {
    const now = Date.now();
    if (chart.paused || now - lastNum < 450 || !root.isConnected) return;
    lastNum = now;
    renderNumbers();
  };
  const offNum = [app.feed.on('sample', numTick), app.feed.on('sys', numTick)];

  const panel = {
    root,
    chart,
    view,
    canvas,
    setPaused,
    renderNumbers,
    setWindow(sec) {
      view.setWindow(sec);
      chart.needs = true;
      canvas.setAttribute('aria-label', `Graf: ${opts.title}, ${windowLabel(sec)}. Mellomrom setter på pause.`);
      if (chart.paused) setPaused(false, { quiet: true });
    },
    destroy() {
      view.destroy();
      for (const off of offNum) off();
    },
  };
  return panel;
}
