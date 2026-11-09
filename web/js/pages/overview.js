// Oversikt: nøkkelrad med seks fliser, detaljer i tre kolonner og tre små
// grafer som fyller resten av høyden (tidsvindu i toppfeltet, v0.0.11).

import { $, el, icon, onFrame, setText } from '../util.js';
import * as F from '../fmt.js';
import { Bar, level, LEVEL_TEXT } from '../meters.js';
import { setTip } from '../ui.js';
import { Gauge } from '../gauge.js';
import { mark42 } from '../easter.js';
import { windowSelect } from '../chartpanel.js';
import { drawSpark, sparkline } from '../sparkline.js';
import { pcieExplain, pcieText, throttleSerious, throttleShort } from '../gpufmt.js';
import { kv, setKv } from '../rows.js';
import { defPanel } from './monitor.js';

export function initOverview(app) {
  const { feed } = app;
  const T = () => app.settings.thresholds;

  // ---- Nøkkelrad ----
  const TILES = [
    { id: 'temp', color: 'c-temp', label: 'Temperatur', sensor: 'gpu/temp', get: (s) => s.temp, minSpan: 20, val: (s) => F.tempNum(s.temp, F.dec(0)), unit: () => F.tempLabel(), subFn: () => '', lvl: (s) => level(s.temp, T().gpuTempWarn, T().gpuTempCrit) },
    { id: 'load', color: 'c-load', label: 'GPU-last', sensor: 'gpu/gpuUtil', get: (s) => s.gpuUtil, fixed: [0, 100], val: (s) => F.num(s.gpuUtil, F.dec(0)), unit: () => '%', subFn: () => '', lvl: (s) => level(s.gpuUtil, T().loadWarn) },
    { id: 'power', color: 'c-power', label: 'Effekt', sensor: 'gpu/power', get: (s) => s.power, minSpan: 40, val: (s) => F.num(s.power, F.dec(0)), unit: () => 'W', subFn: (s) => (s.powerLimit ? `av ${F.watt(s.powerLimit)}` : ''), lvl: () => '' },
    { id: 'clock', color: 'c-clock', label: 'Kjerneklokke', sensor: 'gpu/coreClock', get: (s) => s.coreClock, minSpan: 400, val: (s) => F.num(s.coreClock, 0), unit: () => 'MHz', subFn: () => '', lvl: () => '' },
    { id: 'vram', color: 'c-mem', label: 'VRAM', sensor: 'gpu/vramUsed', get: (s) => s.vramUsed, minSpan: 1024, val: (s) => F.mib(s.vramUsed, { unit: app.settings.memUnit === 'dec' ? 'GB' : 'GiB', d: 1 }).split(F.NNBSP)[0], unit: () => (app.settings.memUnit === 'dec' ? 'GB' : 'GiB'), subFn: (s) => (s.vramTotal ? `av ${F.mib(s.vramTotal, { unit: app.settings.memUnit === 'dec' ? 'GB' : 'GiB', d: 0 })}` : ''), lvl: (s) => (s.vramTotal ? level(s.vramUsed / s.vramTotal, 0.85, 0.95) : '') },
    { id: 'fan', color: 'c-clock', label: 'Vifte', sensor: 'gpu/fan', get: (s) => s.fan, fixed: [0, 100], val: (s) => F.num(s.fan, 0), unit: () => '%', subFn: () => '', lvl: () => '' },
  ];
  const tilesHost = $('#ov-tiles');
  for (const t of TILES) {
    t.value = el('span', { class: 'tile-value' }, el('span', { class: 'skel', style: { '--w': '3em' } }));
    t.sub = el('span', { class: 'sub' });
    t.sp = sparkline('spark');
    t.sp.svg.style.setProperty('--spark', `var(--${t.color})`);
    t.root = el('a', { class: 'tile', href: `#/sensorer?vis=${t.sensor}`, 'data-tip': `${t.label} · siste minutt. Klikk for Sensorer.` }, el('span', { class: 'tile-label' }, el('span', { text: t.label }), t.sub), t.value, t.sp.svg);
    t.lvl0 = '';
    tilesHost.append(t.root);
  }

  // ---- Skjermkort ----
  // Bare det som ikke står andre steder på siden: GPU-last og minnekontroller
  // står i flisene og Last-grafen, så her er VRAM med stolpe og P-tilstand
  // sammen med det som begrenser klokken.
  const gpu = $('#ov-gpu');
  // Plassholdere i omtrent samme bredde som verdiene, til første prøve kommer.
  const pcieDd = kv(gpu, 'PCIe', null, { naText: '–', info: '–', skel: '9em' });
  const pstateDd = kv(gpu, 'P-tilstand', null, { naText: '–', skel: '7em' });
  const fanModeDd = kv(gpu, 'Viftestyring', null, { naText: '–', skel: '7em' });
  const meterRow = (label, id) => {
    const bar = el('div', { class: 'bar' }, el('i'));
    const num = el('span', { class: 'num' }, el('span', { class: 'skel', style: { '--w': '2.4em' } }));
    const dd = el('dd', {}, el('div', { class: 'meter' }, bar, num));
    gpu.append(el('div', { class: 'kv-row meter-row' }, el('dt', { text: label }), dd));
    return { bar: new Bar(bar), num, id, dd };
  };
  const mVram = meterRow('Videominne');
  mVram.bar.box.dataset.tip = 'Brukt videominne. Gult over 85 %, rødt over 95 %.';
  const videoDd = kv(gpu, 'Video (enc / dek)', null, { naText: '–', skel: '5em', tip: 'Last på maskinvarens videokoder (encoder) og -dekoder (decoder).' });

  // ---- Temperatur og effekt: samme måler (gauge.js, v0.0.11) ----
  const tempGauge = new Gauge($('#ov-temp-gauge'), { label: 'GPU-temperatur' });
  const powerGauge = new Gauge($('#ov-power-gauge'), { label: 'GPU-effekt' });
  let peak = []; // [t, °C] siste 5 min

  // ---- Klokker ----
  const CLK = [
    ['Grafikk', 'coreClock', (g) => g.coreMax],
    ['SM', 'smClock', (g) => g.smMax ?? g.coreMax],
    ['Minne', 'memClock', (g) => g.memMax],
    ['Video', 'videoClock', (g, q) => q.maxClockVideo],
  ];
  const clkRows = CLK.map(([name]) => {
    const bar = el('div', { class: 'bar thin' }, el('i'));
    const now = el('td', { 'data-align': 'end', text: '–' });
    const max = el('td', { 'data-align': 'end', class: 'muted', text: '–' });
    return { tr: el('tr', {}, el('td', { text: name }), el('td', { style: { width: '40%' } }, bar), now, max), bar: new Bar(bar), now, max };
  });
  $('#ov-clocks').replaceChildren(
    el('thead', {}, el('tr', {}, el('th', { text: '' }), el('th', { text: '' }), el('th', { 'data-align': 'end', text: 'Nå' }), el('th', { 'data-align': 'end', text: 'Maks' }))),
    el('tbody', {}, ...clkRows.map((r) => r.tr)),
  );

  // ---- Grafer ----
  // Samme graf-panel som på Grafer (v0.0.11): fast seriefarge, forklaring som
  // kan skjule serier, pause og eget tidsvindu (toppfeltet, huskes).
  const sync = { t: null };
  const win = () => app.settings.windows?.oversikt || 60;
  const panels = [
    defPanel(app, 'gpuLoad', { title: 'Last', window: win(), sync, stats: false, now: false }),
    defPanel(app, 'gpuTemp', { title: 'Temperatur', window: win(), sync, stats: false, now: false }),
    defPanel(app, 'power', { title: 'Effekt', window: win(), sync, stats: false, now: false }),
  ];
  for (const p of panels) $('#ov-charts').append(p.root);
  const charts = panels.map((p) => p.chart);
  const winSel = windowSelect(win(), (sec) => app.saveSettings({ windows: { ...app.settings.windows, oversikt: sec } }), { label: 'Tidsvindu for grafene på Oversikt' });
  $('#ov-win-host').append(winSel);
  app.on('settings', (k) => {
    if (!k.includes('windows')) return;
    winSel.value = String(win());
    for (const p of panels) p.setWindow(win());
  });

  // ---- Data ----
  function onSample(s) {
    const g = feed.info?.gpu || {};
    const q = feed.info?.query || {};
    const now = Date.now();
    for (const t of TILES) {
      const v = t.get(s);
      t.value.replaceChildren(v == null ? '–' : t.val(s), el('span', { class: 'unit', text: v == null ? '' : t.unit() }));
      setText(t.sub, v == null ? '' : t.subFn(s));
      const lv = t.lvl(s) || '';
      if (lv !== t.lvl0) {
        t.lvl0 = lv;
        if (lv) t.root.dataset.level = lv;
        else delete t.root.dataset.level;
        setTip(t.root, `${t.label} · siste minutt. Klikk for Sensorer.${lv ? ` Fargen betyr ${LEVEL_TEXT[lv]}.` : ''}`);
      }
    }
    drawTileSparks(now);

    // Skjermkort
    setKv(pcieDd, pcieText(g.pcieWidthMax, g.pcieGenMax, s.pcieWidth, s.pcieGen) || '–');
    const ib = pcieDd.querySelector('.info-btn');
    if (ib) ib.dataset.tip = pcieExplain(g.pcieGenMax, s.pcieGen) || 'PCIe-lenken mellom kortet og hovedkortet.';
    const th = throttleShort(s.throttle, 1);
    const serious = throttleSerious(s.throttle);
    const thText = th.text === 'Ingen' ? 'ingen begrensning' : th.text === '–' ? null : th.text.toLowerCase();
    setKv(pstateDd, [s.pstate, thText].filter(Boolean).join(' · ') || '–');
    pstateDd.dataset.tip = `P0 er full ytelse, P8 er dyp tomgang. Kortet bytter selv.${th.full ? `\nBegrensninger nå: ${th.full}.` : thText && thText !== 'ingen begrensning' ? `\nKlokken begrenses nå av: ${thText}.` : ''}`;
    pstateDd.style.color = serious ? 'var(--warn)' : '';
    const a = app.state.applied;
    const sim = feed.info?.mode !== 'live';
    // Samme ord som modusvelgeren på Kontroller: modus først, så hva den gjør nå.
    // I live-modus styrer driveren den ekte viften (appen skriver ingenting).
    setKv(
      fanModeDd,
      !sim
        ? 'Auto · driveren styrer'
        : s.emergency
          ? `Nødregel · ${F.pct(100)}`
          : a.fanMode === 'curve'
            ? s.fanRest
              ? `Kurve · hviler på ${F.pct(Math.max(35, a.fanMinPercent))}`
              : 'Kurve · følger viftekurven'
            : a.fanMode === 'manual'
              ? `Manuell · ${F.pct(a.fanPercent)}`
              : 'Auto · driveren styrer',
    );
    const memU = app.settings.memUnit === 'dec' ? 'GB' : 'GiB';
    if (s.vramUsed != null && s.vramTotal) {
      mVram.bar.set(s.vramUsed / s.vramTotal, level(s.vramUsed / s.vramTotal, 0.85, 0.95));
      setText(mVram.num, `${F.mib(s.vramUsed, { unit: memU, d: 1 }).split(F.NNBSP)[0]} / ${F.mib(s.vramTotal, { unit: memU, d: 1 })}`);
    } else setText(mVram.num, '–');
    setKv(videoDd, s.encUtil == null && s.decUtil == null ? '–' : `${F.pct(s.encUtil)} / ${F.pct(s.decUtil)}`);

    // Temperatur: fyll til verdien nå, merker for mål og nedklokking, og
    // skalaen slutter ved avstenging. Fargen følger varselgrensene under
    // Innstillinger › Sidefelt (samme som flisene og statusblokken).
    if (s.temp != null) {
      peak.push([now, s.temp]);
      while (peak.length && peak[0][0] < now - 300_000) peak.shift();
      const pk = Math.max(...peak.map((p) => p[1]));
      setText($('#ov-temp-peak'), `maks 5 min: ${F.temp(pk)}`);
      const slow = q.tempSlowdown ?? 97;
      const shut = q.tempShutdown ?? Math.max(100, slow + 3);
      const target = q.tempTarget ?? 83;
      const lv = level(s.temp, T().gpuTempWarn, T().gpuTempCrit);
      const margin = slow - s.temp;
      const state = lv === 'danger' ? `over rød grense (${F.temp(T().gpuTempCrit)})` : lv === 'warn' ? `over gul grense (${F.temp(T().gpuTempWarn)})` : '';
      const tempB = el('b', { class: lv ? `lvl-${lv}` : null, text: F.temp(s.temp, F.dec(0)) });
      mark42($('#ov-temp-margin'), tempB); // Påskeegg (easter.js): nøyaktig 42 °C
      $('#ov-temp-margin').replaceChildren(
        tempB,
        el('span', { class: 'muted', text: margin > 0 ? ` · ${F.tempDelta(margin)} under nedklokking` : ' · over nedklokkingsgrensen' }),
        ...(state ? [el('span', { class: `lvl-${lv}`, text: ` · ${state}` })] : []),
      );
      tempGauge.set({
        min: 30,
        max: shut,
        value: s.temp,
        level: lv,
        marks: [
          { v: target, num: F.temp(target), name: 'Mål' },
          { v: slow, num: F.temp(slow), name: 'Nedklokking' },
        ],
        end: { num: F.temp(shut), name: 'Avstenging' },
        tip: `Fra ${F.temp(30)} til avstenging (${F.temp(shut)}). Over målet (${F.temp(target)}) senker kortet klokken litt, ved ${F.temp(slow)} tvinger det nedklokking. Gul og rød fyll betyr over varselgrensene ${F.temp(T().gpuTempWarn)} og ${F.temp(T().gpuTempCrit)} (Innstillinger › Sidefelt).`,
      });
    }

    // Effekt: fyll til trekket nå, merke for grensen (og standard hvis den er
    // en annen), skalaen slutter ved kortets maks. Gul ved grensen.
    const pmax = g.powerMax || s.powerLimit || 300;
    if (s.power != null) {
      const atLimit = s.powerLimit && s.power >= s.powerLimit * 0.97;
      setText($('#ov-power-text'), s.powerLimit ? `${F.watt(s.power, F.dec(0))} av ${F.watt(s.powerLimit)}` : F.watt(s.power, F.dec(0)));
      $('#ov-power-text').classList.toggle('lvl-warn', !!atLimit);
      const marks = [];
      if (s.powerLimit) marks.push({ v: s.powerLimit, num: F.watt(s.powerLimit), name: Math.round(s.powerLimit) === Math.round(g.powerDefault) ? 'Grense (standard)' : 'Grense' });
      if (g.powerDefault && s.powerLimit && Math.round(s.powerLimit) !== Math.round(g.powerDefault)) marks.push({ v: g.powerDefault, num: F.watt(g.powerDefault), name: 'Standard' });
      const enforced = s.powerEnforced != null && s.powerLimit != null && Math.round(s.powerEnforced) !== Math.round(s.powerLimit) ? ` Håndhevet grense: ${F.watt(s.powerEnforced)}.` : '';
      powerGauge.set({
        min: 0,
        max: pmax,
        value: s.power,
        level: atLimit ? 'warn' : '',
        marks,
        end: g.powerMax ? { num: F.watt(g.powerMax), name: 'Kortets maks' } : null,
        tip: `Trekket nå mot effektgrensen. Gult fyll betyr at kortet er ved grensen og holder igjen.${g.powerMin != null ? ` Grensen kan settes fra ${F.watt(g.powerMin)} til ${F.watt(g.powerMax)}.` : ''}${enforced}`,
      });
    } else setText($('#ov-power-text'), '–');
    setText($('#ov-power-aside'), s.powerLimit && s.power != null ? `${F.pct((s.power / s.powerLimit) * 100)} av grensen` : '');

    // Klokker
    CLK.forEach(([, key, maxFn], i) => {
      const r = clkRows[i];
      const mx = maxFn(g, q);
      setText(r.now, F.num(s[key]));
      setText(r.max, mx == null ? '–' : F.num(mx));
      if (mx) r.bar.set(s[key] / mx);
    });
  }

  function drawTileSparks(now) {
    const hist = feed.history;
    let i = hist.length - 1;
    while (i > 0 && hist[i - 1].t > now - 62_000) i--;
    const slice = hist.slice(i);
    for (const t of TILES) drawSpark(t.sp, slice.map((p) => [p.t, t.get(p)]), { now, span: 60_000, fixed: t.fixed || null, minSpan: t.minSpan || 0 });
  }

  feed.on('sample', (s) => visible && onSample(s));
  app.on('unit', () => feed.latest && onSample(feed.latest));
  app.on('settings', (k) => {
    if (k.includes('thresholds') || k.includes('memUnit')) feed.latest && onSample(feed.latest);
    if (k.includes('chartFill') || k.includes('chartScale')) for (const c of charts) c.needs = true;
  });
  app.on('applied', () => feed.latest && onSample(feed.latest));
  app.on('theme', () => charts.forEach((c) => c.readColors()));
  feed.on('hello', () => (peak = []));

  let stop = null;
  let visible = false;
  const bars = [mVram.bar, tempGauge, powerGauge, ...clkRows.map((r) => r.bar)];
  return {
    show() {
      visible = true;
      charts.forEach((c) => c.readColors());
      if (feed.latest) onSample(feed.latest);
      stop = onFrame((dt) => {
        let busy = false;
        for (const b of bars) busy = b.tick(dt) || busy;
        for (const c of charts) busy = c.frame() || busy;
        return busy;
      });
    },
    hide() {
      visible = false;
      stop?.();
      stop = null;
    },
  };
}
