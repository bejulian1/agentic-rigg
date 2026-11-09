// Grafer: linjegrafer fra 1 min til 48 t (v0.0.11). Historikken kommer fra
// serveren (history.js), og de levende prøvene flettes inn. Hvilke grafer som
// vises, velges i menyen i toppfeltet (og under Innstillinger). Rutenettet
// fyller høyden. Hårkorset kan vises på alle grafene eller bare den du holder
// over (Innstillinger › Grafer og sensorer, eller knappen i toppfeltet).

import { $, onFrame, setIcon } from '../util.js';
import * as F from '../fmt.js';
import { bindSeg, menu, setSeg, setTip, togglePopover } from '../ui.js';
import { hs } from '../charts.js';
import { chartPanel } from '../chartpanel.js';

const tempRange = () => (F.tempLabel() === '°F' ? [68, 212] : [20, 100]);
/** Minste spenn på y-aksen ved «Tilpass»: 20 °C (36 °F). */
export const tempSpan = () => (F.tempLabel() === '°F' ? 36 : 20);
/**
 * Minste spenn for effekt: en fjerdedel av effektgrensen (minst 40 W), så
 * ±10 W støy ved 230 W ikke fyller hele grafen.
 */
let powerRef = null;
export const setPowerRef = (fn) => (powerRef = fn);
export const powerSpan = () => Math.max(40, (powerRef?.() || 0) * 0.25);

const netScaled = (bps) => (F.fmtPrefs().netUnit === 'bit' ? (bps * 8) / 1e6 : bps / 1e6);

/**
 * Seriene, med fast farge per serie (seriepaletten i styles.css). Samme
 * serie har samme farge overalt: Oversikt, Grafer, Kontroller og Sensorer.
 */
export const SERIES = {
  gpuTemp: () => hs('gpu.temp', { label: 'Temperatur', color: 'c-temp', fill: true, map: F.tempOut }),
  gpuLoad: () => hs('gpu.gpuUtil', { label: 'GPU', color: 'c-load', fill: true }),
  memCtl: () => hs('gpu.memUtil', { label: 'Minnekontroller', color: 'c-memctl' }),
  coreClock: () => hs('gpu.coreClock', { label: 'Kjerne', color: 'c-clock', fill: true }),
  memClock: () => hs('gpu.memClock', { label: 'Minne', color: 'c-mem', fill: true }),
  power: () => hs('gpu.power', { label: 'Trekk', color: 'c-power', fill: true }),
  powerLimit: () => hs('gpu.powerLimit', { label: 'Grense', color: 'c-limit', dashed: true, step: true, band: false }),
  fan: () => hs('gpu.fan', { label: 'Vifte', color: 'c-clock', fill: true }),
  vram: () => hs('gpu.vramUsed', { label: 'Brukt', color: 'c-mem', fill: true }),
  cpuLoad: () => hs('cpu.load', { label: 'Total', color: 'c-load', fill: true }),
  cpuTemp: () => hs('cpu.temp', { label: 'Temperatur', color: 'c-temp', fill: true, map: F.tempOut }),
  mem: () => hs('mem.usedPct', { label: 'Brukt', color: 'c-mem', fill: true }),
  netDown: () => hs('net.rx', { label: 'Ned', color: 'c-load', fill: true, map: netScaled }),
  netUp: () => hs('net.tx', { label: 'Opp', color: 'c-memctl', map: netScaled }),
};

/** Alle grafer som finnes. */
export const CHART_DEFS = {
  gpuTemp: { title: 'GPU-temperatur', unit: () => F.tempLabel(), range: tempRange, minSpan: tempSpan, series: ['gpuTemp'] },
  gpuLoad: { title: 'GPU-last', unit: () => '%', range: () => [0, 100], fixedOnly: true, series: ['gpuLoad', 'memCtl'] },
  coreClock: { title: 'Kjerneklokke', unit: () => 'MHz', range: () => [0, 2500], cap: (g) => g.coreMax, minSpan: () => 400, series: ['coreClock'] },
  memClock: { title: 'Minneklokke', unit: () => 'MHz', range: () => [0, 8000], cap: (g) => g.memMax, minSpan: () => 1000, series: ['memClock'] },
  power: { title: 'GPU-effekt', unit: () => 'W', range: () => [0, 300], cap: (g) => g.powerMax, minSpan: powerSpan, decimals: 0, series: ['power', 'powerLimit'] },
  fan: { title: 'GPU-vifte', unit: () => '%', range: () => [0, 100], fixedOnly: true, series: ['fan'] },
  vram: { title: 'Videominne', unit: () => 'MiB', range: () => [0, 8192], minSpan: () => 1024, series: ['vram'] },
  cpuLoad: { title: 'CPU-last', unit: () => '%', range: () => [0, 100], fixedOnly: true, decimals: 1, series: ['cpuLoad'] },
  cpuTemp: { title: 'CPU-temperatur', unit: () => F.tempLabel(), range: tempRange, minSpan: tempSpan, decimals: 1, series: ['cpuTemp'] },
  mem: { title: 'Minne brukt', unit: () => '%', range: () => [0, 100], fixedOnly: true, decimals: 1, series: ['mem'] },
  net: { title: 'Nett', unit: () => (F.fmtPrefs().netUnit === 'bit' ? 'Mbit/s' : 'MB/s'), range: () => [0, 1000], decimals: 2, series: ['netDown', 'netUp'] },
};
/** Området til en graf, utvidet oppover hvis kortet oppgir en høyere grense. */
export function withCap(d, g) {
  const r = d.range();
  const cap = g && d.cap ? d.cap(g) : null;
  return cap != null && Number.isFinite(cap) && cap > r[1] ? [r[0], cap * 1.05] : r;
}
export const CHART_ORDER = ['gpuTemp', 'gpuLoad', 'coreClock', 'memClock', 'power', 'fan', 'vram', 'cpuLoad', 'cpuTemp', 'mem', 'net'];

/** Et graf-panel for en graf i CHART_DEFS (brukes også av Oversikt og Kontroller). */
export function defPanel(app, id, { window, sync, title = null, scale = null, stats = true, now = true } = {}) {
  const d = CHART_DEFS[id];
  const series = d.series.map((k) => SERIES[k]());
  const feed = app.feed;
  return chartPanel(app, {
    title: title || d.title,
    stats,
    now,
    keys: series.map((s) => s.key),
    series,
    window,
    chart: {
      range: id === 'vram' ? () => [0, feed.latest?.vramTotal || 8192] : () => withCap(d, feed.info?.gpu),
      minSpan: d.minSpan,
      scale: scale || (() => (d.fixedOnly ? 'fixed' : app.settings.chartScale)),
      fill: () => app.settings.chartFill,
      unit: d.unit,
      decimals: d.decimals ?? 0,
      lagMs: series[0].key.startsWith('gpu.') ? app.settings.intervalMs + 90 : 1150,
      sync,
    },
  });
}

export function initMonitor(app) {
  const { feed } = app;
  setPowerRef(() => feed.latest?.powerLimit ?? feed.info?.gpu?.powerDefault);
  const grid = $('#chart-grid');
  const sync = { t: null };
  let panels = [];
  let paused = false;
  let visible = false;

  function build() {
    for (const p of panels) p.destroy();
    grid.replaceChildren();
    const ids = CHART_ORDER.filter((id) => app.settings.charts.includes(id));
    grid.style.setProperty('--cols', ids.length <= 3 ? '1' : '2');
    panels = ids.map((id) => {
      const p = defPanel(app, id, { window: app.settings.chartWindow, sync });
      p.onPause = () => syncPauseButton();
      grid.append(p.root);
      if (paused) p.setPaused(true, { quiet: true });
      return p;
    });
  }

  // Tidsvindu
  const winSeg = $('#mon-window');
  setSeg(winSeg, app.settings.chartWindow);
  bindSeg(winSeg, (v) => {
    const sec = Number(v);
    setSeg(winSeg, sec);
    app.saveSettings({ chartWindow: sec });
  });

  // Pause for alle grafene (mellomrom); hver graf har også sin egen.
  const pauseBtn = $('#mon-pause');
  function syncPauseButton() {
    const all = panels.length > 0 && panels.every((p) => p.chart.paused);
    paused = all;
    pauseBtn.setAttribute('aria-pressed', String(all));
    setIcon(pauseBtn, all ? 'play' : 'pause');
    setTip(pauseBtn, all ? 'Fortsett alle' : 'Pause alle');
    const tag = $('#mon-paused');
    tag.hidden = !all;
    if (all) tag.textContent = `På pause · ${F.time(new Date())}`;
  }
  function setPaused(p) {
    for (const x of panels) x.setPaused(p, { quiet: true });
    syncPauseButton();
  }
  pauseBtn.addEventListener('click', () => setPaused(!paused));

  // Hårkorset på alle grafene eller bare den du holder over.
  const syncBtn = $('#mon-sync');
  const renderSync = () => {
    const on = app.settings.chartSync !== false;
    syncBtn.setAttribute('aria-pressed', String(on));
    setTip(syncBtn, on ? 'Markøren vises på alle grafene. Klikk for bare grafen du holder over.' : 'Markøren vises bare på grafen du holder over. Klikk for alle grafene.');
  };
  syncBtn.addEventListener('click', () => app.saveSettings({ chartSync: app.settings.chartSync === false }));
  renderSync();

  // Velg grafer
  const pickMenu = menu(
    [
      { title: 'Grafer som vises' },
      ...CHART_ORDER.map((id) => ({
        label: CHART_DEFS[id].title,
        checked: () => app.settings.charts.includes(id),
        disabled: () => app.settings.charts.length === 1 && app.settings.charts.includes(id),
        toggle: (on) => {
          const set = new Set(app.settings.charts);
          if (on) set.add(id);
          else set.delete(id);
          if (set.size) app.saveSettings({ charts: CHART_ORDER.filter((x) => set.has(x)) });
        },
      })),
    ],
    { label: 'Velg grafer' },
  );
  $('#mon-pick').addEventListener('click', () => togglePopover($('#mon-pick'), pickMenu, { align: 'end' }));

  const numbers = () => visible && panels.forEach((p) => !p.chart.paused && p.renderNumbers());
  feed.on('sample', numbers);
  feed.on('sys', numbers);
  app.on('theme', () => panels.forEach((p) => p.chart.readColors()));
  app.on('unit', () => {
    numbers();
    for (const p of panels) p.chart.needs = true;
  });
  app.on('settings', (k) => {
    if (k.includes('charts')) {
      build();
      pickMenu.rerender?.();
    }
    if (k.includes('chartWindow')) {
      setSeg(winSeg, app.settings.chartWindow);
      for (const p of panels) p.setWindow(app.settings.chartWindow);
      syncPauseButton();
    }
    if (k.includes('chartSync')) renderSync();
    if (k.includes('chartScale') || k.includes('chartFill') || k.includes('chartSync')) for (const p of panels) p.chart.needs = true;
    if (k.includes('intervalMs')) for (const p of panels) if (p.view.gpuKeys.length) p.chart.o.lagMs = app.settings.intervalMs + 90;
  });
  build();

  let stop = null;
  return {
    show() {
      visible = true;
      for (const p of panels) p.chart.readColors();
      numbers();
      stop = onFrame(() => {
        let busy = false;
        for (const p of panels) busy = p.chart.frame() || busy;
        return busy;
      });
    },
    hide() {
      visible = false;
      stop?.();
      stop = null;
    },
    onKey(e, typing) {
      if (e.key === ' ' && !typing && !e.target.closest?.('button, a, [role=radio], canvas, select')) {
        setPaused(!paused);
        return true;
      }
      return false;
    },
  };
}
