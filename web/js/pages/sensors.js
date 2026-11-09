// Sensorer: én tett tabell (v0.0.9), gruppert etter enhet (Skjermkort,
// Prosessor, Minne, Andre sensorer, Nett). Kolonnene er Nå, Min, Maks og
// Snitt, pluss en sparklinje for siste 60 s tegnet mot tid. Overskriftsraden
// står én gang (thead, klistrer seg øverst). Hver gruppe er en egen tbody med
// en sammenleggbar overskriftsrad. Statistikken samles hele tiden, men
// tabellen tegnes bare når siden vises.
//
// Sammenlegging: setOpen() setter data-open="true|false" på gruppens tbody,
// og radene skjules i CSS. Tabellrader kan ikke gli i høyden, så de toner inn
// når gruppen åpnes (samme som «Per tråd», tr.subhead).
//
// Graf for én sensor: klikk på navnet (eller Enter) åpner en større graf for
// siste 60 s rett under raden (én om gangen). Esc eller nytt klikk lukker.
//
// Min og maks huskes mellom økter (v0.0.6, kan slås av): ved oppstart hentes
// det lagrede settet fra serveren, og fanen sender sine tall høyst hvert 10. s
// når noe er endret (serveren skriver fila enda sjeldnere). Snitt gjelder
// alltid denne økten. Nullstilling i én fane når de andre via BroadcastChannel.
//
// Feste (v0.0.6): et feste-ikon til venstre for navnet vises ved hover og
// fokus, og alltid på festede rader. Festede rader står i statusblokken.

import { $, el, icon, local, onFrame, setIcon, setText, animate, dur, EASE } from '../util.js';
import * as F from '../fmt.js';
import { drawSpark, sparkline } from '../sparkline.js';
import { hs } from '../charts.js';
import { chartPanel, windowSelect } from '../chartpanel.js';
import { genText, shortGpuName, throttleList, throttleSerious, throttleShort } from '../gpufmt.js';
import { cpuTempSensor, netName, sensorName } from '../sensormeta.js';
import { menu, setTip, togglePopover } from '../ui.js';
import { level, LEVEL_TEXT } from '../meters.js';
import { beaconMinmax } from '../api.js';
import { isPinned, togglePin } from '../pins.js';
import { filterQuip } from '../easter.js';

// Rekkefølgen her er rekkefølgen i tabellen.
const GROUPS = {
  gpu: { title: 'Skjermkort' },
  cpu: { title: 'Prosessor' },
  mem: { title: 'Minne' },
  hwmon: { title: 'Andre sensorer' },
  net: { title: 'Nett' },
};
const COLLAPSE_KEY = 'ag-sensors-collapsed';
const THREADS_KEY = 'ag-sensors-threads-open';
const SPAN = 60_000;
const MM_EVERY = 10_000; // så ofte fanen sender min/maks til serveren (når noe er endret)

// Korte navn i statusblokken (sidefeltet har ca. 100 px til navnet).
const GPU_SHORT = {
  temp: 'GPU-temp',
  gpuUtil: 'GPU-last',
  memUtil: 'Minnekontr.',
  encUtil: 'Encoder',
  decUtil: 'Decoder',
  coreClock: 'GPU-klokke',
  smClock: 'SM-klokke',
  memClock: 'VRAM-klokke',
  videoClock: 'Videoklokke',
  power: 'Effekt',
  powerLimit: 'Effektgrense',
  fan: 'Vifte',
  vramUsed: 'VRAM',
  pcieGen: 'PCIe',
};
const MEM_SHORT = { used: 'RAM brukt', usedPct: 'Minne', avail: 'RAM ledig', swap: 'Swap' };
function shortName(id, group, label) {
  const k = id.split('/').slice(1).join('/');
  if (group === 'gpu') return GPU_SHORT[k] || label;
  if (group === 'mem') return MEM_SHORT[k] || label;
  if (group === 'cpu') {
    if (k === 'temp') return 'CPU-temp';
    if (k === 'load') return 'CPU-last';
    if (/^t\d+$/.test(k)) return `CPU-tråd ${k.slice(1)}`;
    const m = label.match(/\((.+)\)$/);
    return `CPU ${m ? m[1] : label}`;
  }
  if (group === 'net') {
    const [name, dir] = k.split('/');
    return `${name} ${dir === 'rx' ? 'ned' : 'opp'}`;
  }
  return label;
}

// Formaterere. dec() følger Innstillinger → Desimaler.
const FM = {
  temp: (v) => F.temp(v, F.dec(1)),
  pct: (v) => F.pct(v, F.dec(1)),
  pct0: (v) => F.pct(v, F.dec(0)),
  mhz: (v) => F.mhz(v),
  w: (v) => F.watt(v, F.dec(1)),
  w0: (v) => F.watt(v, F.dec(0)),
  mib: (v) => F.mib(v, { unit: 'MiB' }),
  gib: (v) => (v == null ? '–' : F.bytes(v * 1024 ** 3, 2)),
  rpm: (v) => F.withUnit(F.num(v), 'o/min'),
  volt: (v) => F.withUnit(F.num(v, 3), 'V'),
  amp: (v) => F.withUnit(F.num(v, 3), 'A'),
  gen: (v) => genText(v),
};
const HWMON_FMT = { temp: FM.temp, fan: FM.rpm, in: FM.volt, power: FM.w, curr: FM.amp };
// Statusblokken viser hele grader, prosent og watt (som de faste verdiene der).
const SIDE_FMT = new Map([
  [FM.temp, (v) => F.temp(v, F.dec(0))],
  [FM.pct, FM.pct0],
  [FM.w, FM.w0],
]);

const GPU_ROWS = [
  ['temp', 'Temperatur', FM.temp, { minSpan: 20 }],
  ['gpuUtil', 'GPU-last', FM.pct0, { fixed: [0, 100] }],
  ['memUtil', 'Minnekontroller-last', FM.pct0, { fixed: [0, 100] }],
  ['encUtil', 'Videokoder (encoder)', FM.pct0, { fixed: [0, 100] }],
  ['decUtil', 'Videodekoder (decoder)', FM.pct0, { fixed: [0, 100] }],
  ['coreClock', 'Grafikklokke', FM.mhz, { minSpan: 400 }],
  ['smClock', 'SM-klokke', FM.mhz, { minSpan: 400 }],
  ['memClock', 'Minneklokke', FM.mhz, { minSpan: 400 }],
  ['videoClock', 'Videoklokke', FM.mhz, { minSpan: 400 }],
  ['power', 'Effekttrekk', FM.w, { minSpan: 40 }],
  ['powerLimit', 'Effektgrense', FM.w0],
  ['fan', 'Viftefart', FM.pct0, { fixed: [0, 100] }],
  ['vramUsed', 'Videominne brukt', FM.mib],
  ['pcieGen', 'PCIe-generasjon', FM.gen, { discrete: true, tip: 'Generasjonen lenken går på nå. Kortet går ned i tomgang for å spare strøm.' }],
];

export function initSensors(app) {
  const { feed } = app;
  const T = () => app.settings.thresholds;
  // Terskelfarger på Nå-cellen (Innstillinger › Sidefelt › Varselfarger).
  const LVL = {
    'gpu/temp': (v) => level(v, T().gpuTempWarn, T().gpuTempCrit),
    'gpu/gpuUtil': (v) => level(v, T().loadWarn),
    'cpu/temp': (v) => level(v, T().cpuTempWarn, T().cpuTempCrit),
    'cpu/load': (v) => level(v, T().loadWarn),
  };
  const rows = new Map(); // id → rad
  const groups = new Map(); // id → { root, tbody, … }
  let collapsed = new Set(local.get(COLLAPSE_KEY, []));
  let threadsOpen = local.get(THREADS_KEY, false);
  let since = new Date();
  let stored = null; // min/maks fra serveren: { since, mode, rows }
  let mmDirty = false; // min eller maks er endret siden sist det ble sendt
  let mmSending = false;
  let sysSig = '';
  let visible = false;
  let dirty = true;
  let filter = '';
  let notesLoaded = false;
  let gpuState = { pstate: null, throttle: null };

  function makeRow(id, group, label, fmt, opts = {}) {
    let r = rows.get(id);
    if (r) {
      Object.assign(r, { label, raw: opts.raw ?? r.raw, extra: opts.extra ?? r.extra });
      return r;
    }
    r = { id, group, label, fmt, lvl: opts.lvl || LVL[id] || (opts.thread != null ? (v) => level(v, T().loadWarn) : null), raw: opts.raw || null, tip: opts.tip || null, fixed: opts.fixed || null, minSpan: opts.minSpan || 0, discrete: !!opts.discrete, rate: !!opts.rate, thread: opts.thread ?? null, now: null, min: Infinity, max: -Infinity, sum: 0, n: 0, hist: [], unit: null, tr: null };
    r.short = shortName(id, group, label);
    r.sideFmt = SIDE_FMT.get(fmt) || fmt;
    r.groupTitle = GROUPS[group].title;
    r.long = `${r.groupTitle} · ${label}`;
    rows.set(id, r);
    seed(r);
    return r;
  }

  function push(r, v, t) {
    if (!r) return;
    r.now = v == null || !Number.isFinite(v) ? null : v;
    r.hist.push([t, r.now]);
    while (r.hist.length && r.hist[0][0] < t - SPAN - 3000) r.hist.shift();
    if (r.now == null) return;
    if (r.now < r.min) {
      r.min = r.now;
      mmDirty = true;
    }
    if (r.now > r.max) {
      r.max = r.now;
      mmDirty = true;
    }
    r.sum += r.now;
    r.n++;
    // Fast enhet per rad for nettrate: velges ut fra største verdi og byttes bare oppover.
    if (r.rate) {
      const u = F.rateUnit(r.max);
      if (!r.unit || u.div > r.unit.div) r.unit = u;
    }
  }

  // ---- Struktur ----
  const order = { gpu: [], cpu: [], mem: [], net: [], hwmon: [] };

  function buildOrder(sys) {
    for (const k in order) order[k] = [];
    for (const [k, label, fmt, o] of GPU_ROWS) order.gpu.push(makeRow(`gpu/${k}`, 'gpu', label, fmt, o || {}));
    if (sys) {
      const ct = cpuTempSensor(sys);
      const dev = sys.hwmon.find((d) => d.name === 'k10temp' || d.name === 'coretemp' || d.name === 'zenpower');
      for (const x of dev?.sensors || []) {
        const tip = x.label === 'Tctl' ? 'Tctl er styringstemperaturen som viftestyringen bruker. På Ryzen 2000 X-modeller ligger den 10 °C over Tdie.' : x.label === 'Tdie' ? 'Tdie er den faktiske brikketemperaturen.' : null;
        const label = x.label === 'Tctl' ? 'Temperatur (Tctl)' : x.label === 'Tdie' ? 'Temperatur (Tdie)' : sensorName(dev, x).name.replace(/^Prosessor · /, '');
        order.cpu.push(makeRow(x === ct ? 'cpu/temp' : `cpu/${x.id}`, 'cpu', label, HWMON_FMT[x.kind] || FM.pct, { raw: `${dev.name} · ${x.key}`, tip, minSpan: x.kind === 'temp' ? 20 : 0 }));
      }
      order.cpu.push(makeRow('cpu/load', 'cpu', 'Total last', FM.pct, { fixed: [0, 100] }));
      order.cpu.push({ sub: 'threads', n: sys.cpu.coreLoad.length });
      for (let i = 0; i < sys.cpu.coreLoad.length; i++) order.cpu.push(makeRow(`cpu/t${i}`, 'cpu', `Tråd ${i}`, FM.pct, { fixed: [0, 100], thread: i }));

      order.mem.push(makeRow('mem/used', 'mem', 'Brukt', FM.gib));
      order.mem.push(makeRow('mem/usedPct', 'mem', 'Brukt av totalt', FM.pct, { fixed: [0, 100] }));
      order.mem.push(makeRow('mem/avail', 'mem', 'Tilgjengelig', FM.gib));
      order.mem.push(makeRow('mem/swap', 'mem', 'Swap brukt', FM.gib));

      for (const [name, n] of Object.entries(sys.net)) {
        const base = netName(name, n);
        const down = n.state && n.state !== 'up' && n.state !== 'unknown';
        const extra = down ? 'ikke tilkoblet' : null;
        order.net.push(makeRow(`net/${name}/rx`, 'net', `${base} · ned`, null, { rate: true, raw: `${name} · rx_bytes`, extra }));
        order.net.push(makeRow(`net/${name}/tx`, 'net', `${base} · opp`, null, { rate: true, raw: `${name} · tx_bytes`, extra }));
      }

      for (const d of sys.hwmon) {
        if (d === dev) continue;
        for (const x of d.sensors) {
          const nm = sensorName(d, x);
          order.hwmon.push(makeRow(`hw/${x.id}`, 'hwmon', nm.name, HWMON_FMT[x.kind] || FM.pct, { raw: nm.raw, minSpan: x.kind === 'temp' ? 20 : 0 }));
        }
      }
      // Rader for enheter som er borte (f.eks. Docker-grensesnitt som kommer og
      // går) slettes, så lista ikke vokser i lange økter.
      const keep = new Set(Object.values(order).flat().map((r) => r.id));
      for (const id of rows.keys()) if (!keep.has(id)) rows.delete(id);
    }
    render();
  }

  function groupNote(id) {
    const info = feed.info;
    if (id === 'gpu') return `${shortGpuName(info?.gpu?.name) || 'Skjermkort'} · ${info?.mode === 'live' ? 'nvidia-smi' : 'simulert'}`;
    if (id === 'cpu') return '/proc/stat, cpufreq og hwmon';
    if (id === 'mem') return '/proc/meminfo';
    if (id === 'net') return '/sys/class/net';
    return '/sys/class/hwmon';
  }

  /** Historikknøkkelen til en rad (samme navn som server/history.js). */
  function rowKey(r) {
    const [g, ...rest] = r.id.split('/');
    const k = rest.join('/');
    if (g === 'gpu') return `gpu.${k}`;
    if (g === 'cpu') return k === 'temp' || k === 'load' || /^t\d+$/.test(k) ? `cpu.${k}` : `hw.${k}`;
    if (g === 'mem') return `mem.${k}`;
    if (g === 'net') {
      const [name, dir] = k.split('/');
      return `net.${name}.${dir}`;
    }
    if (g === 'hw') return `hw.${k}`;
    return null;
  }
  /** Fast seriefarge per slags verdi (samme som i grafene, se styles.css). */
  function rowColor(r) {
    const k = r.id.split('/').slice(1).join('/');
    if (r.fmt === FM.temp) return 'c-temp';
    if (r.group === 'gpu') return { memUtil: 'c-memctl', coreClock: 'c-clock', smClock: 'c-clock', videoClock: 'c-clock', fan: 'c-clock', power: 'c-power', powerLimit: 'c-limit', memClock: 'c-mem', vramUsed: 'c-mem', encUtil: 'c-memctl', decUtil: 'c-memctl', pcieGen: 'c-limit' }[k] || 'c-load';
    if (r.group === 'mem') return 'c-mem';
    if (r.group === 'net') return k.endsWith('/tx') ? 'c-memctl' : 'c-load';
    if (r.fmt === FM.rpm) return 'c-clock';
    if (r.fmt === FM.w) return 'c-power';
    return 'c-load';
  }

  function render() {
    const table = $('#sn-table');
    for (const tb of table.querySelectorAll('tbody')) tb.remove();
    groups.clear();
    for (const [id, g] of Object.entries(GROUPS)) {
      const list = order[id];
      if (!list.length && id !== 'gpu') continue;
      const chev = icon('chevron-down', 'icon chev');
      const state = el('span', { class: 'state' });
      const btn = el('button', { type: 'button', class: 'grp-btn', 'aria-expanded': 'true' }, chev, el('span', { class: 'group-title', text: g.title }), el('span', { class: 'note', text: groupNote(id) }));
      const head = el('tr', { class: 'grp' }, el('th', { scope: 'rowgroup', colspan: '6' }, el('div', { class: 'grp-line' }, btn, state)));
      const tbody = el('tbody', { class: 'sn-group', 'data-open': 'true', 'data-group': id }, head);
      for (const r of list) {
        if (r.sub === 'threads') {
          const sb = el('button', { type: 'button', class: 'sub-btn', 'aria-expanded': String(threadsOpen) }, icon('chevron-down'), `Per tråd (${r.n})`);
          const tr = el('tr', { class: 'subhead', 'data-open': String(threadsOpen) }, el('td', { colspan: '6' }, sb));
          sb.addEventListener('click', () => setThreads(!threadsOpen));
          tbody.append(tr);
          continue;
        }
        tbody.append(rowEl(r));
      }
      table.append(tbody);
      btn.addEventListener('click', () => setOpen(id, collapsed.has(id)));
      groups.set(id, { root: tbody, btn, state, tbody });
      applyOpen(id);
      tbody.addEventListener('keydown', rowKeys);
      tbody.addEventListener('focusin', (e) => e.target.classList?.contains('rv') && roving(tbody, e.target));
    }
    // Grafen for valgt sensor følger raden når tabellen bygges på nytt.
    if (detail.row) {
      if (detail.row.tr?.isConnected) placeDetail();
      else closeDetail(false);
    }
    renderGpuState();
    applyThreads();
    applyFilter();
    dirty = true;
  }

  function rowEl(r) {
    // Én tab-stopp per gruppe: piltastene flytter mellom radene (opp/ned) og
    // mellom nålen og navnet (venstre/høyre). Se roving().
    const pin = el('button', { type: 'button', class: 'pin-btn rv', 'aria-pressed': 'false', tabindex: '-1' }, icon('pin'));
    pin.addEventListener('click', (e) => {
      e.stopPropagation();
      togglePin(app, r.id);
    });
    r.pinBtn = pin;
    const nameBtn = el('button', { type: 'button', class: 'sn-name rv', 'aria-expanded': 'false', tabindex: '-1' }, el('span', { class: 'n', text: r.label }));
    nameBtn.addEventListener('click', () => toggleDetail(r));
    r.nameBtn = nameBtn;
    const name = el('span', { class: 'name' }, pin, nameBtn);
    if (r.extra) name.append(el('span', { class: 'x', text: r.extra }));
    if (r.thread != null) {
      r.clockEl = el('span', { class: 'x' });
      name.append(r.clockEl);
    }
    const tip = [r.tip, r.raw ? `Kilde: ${r.raw}` : null].filter(Boolean).join('\n');
    const sp = sparkline('spark');
    sp.svg.style.setProperty('--spark', `var(--${rowColor(r)})`);
    const c = {
      name: el('td', { 'data-tip': tip || null }, name),
      now: el('td', { class: 'now' }),
      min: el('td', { class: 'min' }),
      max: el('td', { class: 'max' }),
      avg: el('td', { class: 'avg' }),
      spark: el('td', { class: 'c-spark-cell' }, sp.svg),
    };
    r.tr = el('tr', { 'data-id': r.id, class: r.thread != null ? 'thread' : null }, c.name, c.now, c.min, c.max, c.avg, c.spark);
    r.cells = c;
    r.sp = sp;
    if (detail.row === r) nameBtn.setAttribute('aria-expanded', 'true');
    renderPin(r);
    return r.tr;
  }

  // ---- Graf for én sensor ----
  // Et graf-panel (chartpanel.js) i en egen rad under den valgte, med
  // historikk fra serveren (1 min til 48 t, tidsvindu i panelet, huskes).
  // Panelet lages på nytt for hver rad og ryddes bort når den lukkes.
  const detail = { row: null, tr: null, panel: null };
  const isTemp = (r) => r.fmt === FM.temp;
  /** Verdien slik den vises (°F, nettrate i valgt enhet). */
  function conv(r, v) {
    if (v == null) return null;
    if (isTemp(r)) return F.tempOut(v);
    if (r.rate) return v / (r.unit?.div || F.rateUnit(r.max).div);
    return v;
  }
  function unconv(r, x) {
    if (isTemp(r)) return F.tempIn(x);
    if (r.rate) return x * (r.unit?.div || F.rateUnit(r.max).div);
    return x;
  }
  const sWin = () => app.settings.windows?.sensorer || 300;
  function openDetail(r) {
    closeDetail(false);
    const key = rowKey(r);
    if (!key) return;
    detail.row = r;
    const series = hs(key, { label: r.label, color: rowColor(r), fill: true, map: (v) => conv(r, v), fmt: (x) => (r.rate ? F.rate(unconv(r, x), r.unit) : r.fmt(unconv(r, x))) });
    const close = el('button', { type: 'button', class: 'btn sm flat icon-only', 'aria-label': 'Lukk grafen', 'data-tip': 'Lukk grafen (Esc)' }, icon('x'));
    close.addEventListener('click', () => closeDetail(true));
    const sel = windowSelect(sWin(), (sec) => app.saveSettings({ windows: { ...app.settings.windows, sensorer: sec } }), { label: `Tidsvindu for grafen` });
    const extra = el('span', { class: 'sn-detail-tools' }, sel, close);
    detail.panel = chartPanel(app, {
      title: r.label,
      keys: [key],
      series: [series],
      window: sWin(),
      tag: 'div',
      className: 'sn-detail-panel',
      now: false,
      extra,
      chart: {
        lagMs: r.group === 'gpu' ? app.settings.intervalMs + 90 : 1100,
        fill: () => app.settings.chartFill,
        scale: () => (r.fixed ? 'fixed' : 'auto'),
        // Øvre grense fra dataene i vinduet (også lange vinduer med høyere
        // topper enn denne økten), ikke bare fra økta.
        range: () => {
          if (r.fixed) return r.fixed;
          let hi = Number.isFinite(r.max) ? r.max : 0;
          for (const p of detail.panel?.view.data || []) {
            const v = p.hi?.[key] ?? p.v[key];
            if (v != null && v > hi) hi = v;
          }
          return [0, Math.max(1, (conv(r, hi) || 1) * 1.3)];
        },
        minSpan: () => (!r.minSpan ? 0 : isTemp(r) && F.tempLabel() === '°F' ? r.minSpan * 1.8 : r.minSpan),
      },
    });
    detail.panel.canvas.setAttribute('aria-label', `Graf: ${r.label} (${r.groupTitle}${r.raw ? `, kilde ${r.raw}` : ''}). Mellomrom setter på pause.`);
    const box = el('div', { class: 'sn-detail', id: 'sn-detail' }, detail.panel.root);
    detail.tr = el('tr', { class: 'sn-detail-row' }, el('td', { colspan: '6' }, box));
    r.nameBtn?.setAttribute('aria-expanded', 'true');
    r.nameBtn?.setAttribute('aria-controls', 'sn-detail');
    placeDetail();
    animate(detail.tr, [{ opacity: 0 }, { opacity: 1 }], { duration: dur(3), easing: EASE.out }, [{ opacity: 0 }, { opacity: 1 }]);
    dirty = true;
  }
  function placeDetail() {
    detail.row.tr.after(detail.tr);
    detail.panel.chart.needs = true;
  }
  function closeDetail(focus) {
    const r = detail.row;
    if (!r) return;
    detail.row = null;
    detail.panel?.destroy();
    detail.panel = null;
    detail.tr?.remove();
    detail.tr = null;
    r.nameBtn?.setAttribute('aria-expanded', 'false');
    if (focus && r.nameBtn?.isConnected) r.nameBtn.focus();
  }
  function toggleDetail(r) {
    if (detail.row === r) closeDetail(false);
    else openDetail(r);
  }
  app.on('settings', (k) => k.includes('windows') && detail.panel?.setWindow(sWin()));
  app.on('theme', () => detail.panel?.chart.readColors());


  // ---- Feste ----
  function renderPin(r) {
    if (!r.pinBtn) return;
    const on = isPinned(app, r.id);
    r.pinBtn.setAttribute('aria-pressed', String(on));
    r.pinBtn.setAttribute('aria-label', `${on ? 'Løsne' : 'Fest'} «${r.label}» ${on ? 'fra' : 'i'} statusblokken`);
    setTip(r.pinBtn, on ? 'Løsne fra statusblokken' : 'Fest i statusblokken');
    r.tr.toggleAttribute('data-pinned', on);
  }
  function renderPins() {
    for (const r of rows.values()) renderPin(r);
  }
  /** Knappene i synlige rader: [[nål, navn], …]. */
  const visibleRows = (tbody) => [...tbody.querySelectorAll('tr[data-id]:not([hidden])')].map((tr) => [...tr.querySelectorAll('.rv')]);
  /** Knappen som kan nås med Tab i en gruppe: den sist brukte, ellers navnet i første synlige rad. */
  function roving(tbody, cur = null) {
    const all = visibleRows(tbody).flat();
    const keepCur = cur && all.includes(cur) ? cur : all.find((b) => b.tabIndex === 0) || visibleRows(tbody)[0]?.[1];
    for (const b of tbody.querySelectorAll('.rv')) b.tabIndex = b === keepCur ? 0 : -1;
  }
  function rowKeys(e) {
    if (e.key === 'Escape' && detail.row && detail.tr?.contains(e.target) && e.target.tagName !== 'SELECT') {
      e.preventDefault();
      closeDetail(true);
      return;
    }
    if (!e.target.classList?.contains('rv') || e.altKey || e.ctrlKey || e.metaKey) return;
    const grid = visibleRows(e.currentTarget);
    const ri = grid.findIndex((row) => row.includes(e.target));
    if (ri < 0) return;
    const ci = grid[ri].indexOf(e.target);
    let r = ri;
    let c = ci;
    if (e.key === 'ArrowDown') r++;
    else if (e.key === 'ArrowUp') r--;
    else if (e.key === 'ArrowLeft') c = 0;
    else if (e.key === 'ArrowRight') c = 1;
    else if (e.key === 'Home') r = 0;
    else if (e.key === 'End') r = grid.length - 1;
    else if (e.key === 'Escape' && detail.row && grid[ri][1] === detail.row.nameBtn) {
      e.preventDefault();
      closeDetail(true);
      return;
    } else return;
    const next = grid[r]?.[c];
    if (!next) return;
    e.preventDefault();
    roving(e.currentTarget, next);
    next.focus();
  }
  // Høyreklikk på en rad gir det samme valget som feste-ikonet.
  let ctxMenu = null;
  function rowMenu(e) {
    const tr = e.target.closest('tr[data-id]');
    const r = tr && rows.get(tr.dataset.id);
    if (!r?.pinBtn) return;
    e.preventDefault();
    const on = isPinned(app, r.id);
    ctxMenu?.remove();
    ctxMenu = menu(
      [
        { label: on ? 'Løsne fra statusblokken' : 'Fest i statusblokken', icon: on ? 'pin-off' : 'pin', run: () => togglePin(app, r.id) },
        { label: 'Rekkefølge i statusblokken …', icon: 'arrow-down-up', run: () => app.openSettings?.('sidefelt') },
      ],
      { label: r.label },
    );
    togglePopover(r.pinBtn, ctxMenu);
  }
  $('#sn-table').addEventListener('contextmenu', rowMenu);

  function renderGpuState() {
    const g = groups.get('gpu');
    if (!g) return;
    const tags = [];
    if (gpuState.pstate) tags.push(el('span', { class: 'tag', 'data-tip': 'P-tilstand. P0 er full ytelse, P8 er dyp tomgang.', text: gpuState.pstate }));
    const tl = throttleList(gpuState.throttle);
    if (tl) {
      const th = throttleShort(gpuState.throttle, 1);
      tags.push(el('span', { class: `tag${throttleSerious(gpuState.throttle) ? ' warn' : ''}`, 'data-tip': `Klokkebegrensning akkurat nå${th.full ? `: ${th.full}` : ''}`, text: tl.length ? th.text : 'Ingen begrensning' }));
    }
    const sig = tags.map((t) => t.textContent + t.className).join('|');
    if (sig !== g.stateSig) {
      g.stateSig = sig;
      g.state.replaceChildren(...tags);
    }
  }

  // ---- Sammenlegging ----
  function applyOpen(id) {
    const g = groups.get(id);
    if (!g) return;
    const open = filter ? true : !collapsed.has(id);
    g.root.dataset.open = String(open);
    g.btn.setAttribute('aria-expanded', String(open));
  }

  function setOpen(id, open) {
    if (open) collapsed.delete(id);
    else collapsed.add(id);
    local.set(COLLAPSE_KEY, [...collapsed]);
    applyOpen(id);
    renderToggle();
    dirty = true;
    const g = groups.get(id);
    if (!g) return;
    if (!open && detail.row?.group === id) closeDetail(false);
    // Radene toner inn (tabellrader kan ikke gli i høyden).
    if (open) for (const tr of g.tbody.querySelectorAll('tr:not(.grp):not([hidden])')) animate(tr, [{ opacity: 0 }, { opacity: 1 }], { duration: dur(3), easing: EASE.out }, [{ opacity: 0 }, { opacity: 1 }]);
  }

  function setThreads(open) {
    threadsOpen = open;
    local.set(THREADS_KEY, open);
    applyThreads();
    dirty = true;
    // Tabellrader kan ikke sammenlegges med grid-rader; de toner inn i stedet.
    if (open) for (const r of rows.values()) if (r.thread != null && r.tr && !r.tr.hidden) animate(r.tr, [{ opacity: 0 }, { opacity: 1 }], { duration: dur(3), easing: EASE.out }, [{ opacity: 0 }, { opacity: 1 }]);
  }

  function applyThreads() {
    const sub = $('#sn-table tr.subhead');
    const open = threadsOpen || !!filter;
    if (sub) {
      sub.dataset.open = String(open);
      sub.querySelector('button').setAttribute('aria-expanded', String(open));
    }
    for (const r of rows.values()) if (r.thread != null && r.tr) r.tr.hidden = !open || !matches(r);
    if (detail.row?.tr?.hidden) closeDetail(false);
  }

  function renderToggle() {
    const allClosed = [...groups.keys()].every((id) => collapsed.has(id));
    const btn = $('#sn-toggle');
    setIcon(btn, allClosed ? 'chevrons-up-down' : 'chevrons-down-up');
    setTip(btn, allClosed ? 'Fold ut alle' : 'Fold sammen alle');
  }

  // ---- Filter ----
  const matches = (r) => !filter || `${r.label} ${r.raw || ''}`.toLowerCase().includes(filter);
  function applyFilter() {
    let any = false;
    for (const [id, g] of groups) {
      let n = 0;
      for (const r of order[id]) {
        if (!r.tr) continue;
        const ok = matches(r);
        if (r.thread == null) r.tr.hidden = !ok;
        if (ok) n++;
      }
      g.root.hidden = filter && n === 0;
      if (n) any = true;
      applyOpen(id);
    }
    $('#sn-panel').hidden = !!filter && !any;
    applyThreads();
    // «Ikke tilgjengelig» filtreres også, så et søk uten treff ikke står over åtte rader.
    let notes = 0;
    for (const n of $('#sn-notes').children) {
      const ok = !filter || n.textContent.toLowerCase().includes(filter);
      n.hidden = !ok;
      if (ok) notes++;
    }
    $('#sn-notes-group').hidden = !notesLoaded || notes === 0;
    $('#sn-empty').hidden = !filter || any || notes > 0;
    // Påskeegg (easter.js): noen få ord får sitt eget svar i tomtilstanden.
    if (!$('#sn-empty').hidden) setText($('#sn-empty span'), filterQuip(filter) || 'Ingen sensorer passer filteret.');
    for (const g of groups.values()) roving(g.tbody);
    dirty = true;
  }
  $('#sn-search').addEventListener('input', (e) => {
    filter = e.target.value.trim().toLowerCase();
    applyFilter();
  });
  app.on('settings', (keys) => keys.includes('surprises') && filter && applyFilter());

  // ---- Tegning ----
  /** Tall og enhet i hver sin del, så enheten kan dempes (v0.0.9). Skriver bare når teksten endres. */
  function setNum(td, text) {
    if (td.$t === text) return;
    td.$t = text;
    const i = text.lastIndexOf(F.NNBSP);
    if (i < 0) td.textContent = text;
    else td.replaceChildren(text.slice(0, i + 1), el('span', { class: 'unit', text: text.slice(i + 1) }));
  }
  function paint() {
    const now = Date.now();
    for (const r of rows.values()) {
      if (!r.tr || r.tr.hidden) continue;
      if (r.group !== 'gpu' && r.group !== 'cpu' && r.group !== 'mem' && r.group !== 'net' && r.group !== 'hwmon') continue;
      const g = groups.get(r.group);
      if (g && g.root.dataset.open === 'false') continue;
      const c = r.cells;
      const f = r.rate ? (v) => F.rate(v, r.unit) : r.fmt;
      setNum(c.now, f(r.now));
      // Samme gul/rød som på Oversikt og i statusblokken.
      const lv = r.lvl ? r.lvl(r.now) : '';
      if (lv !== (c.now.dataset.level || '')) {
        if (lv) c.now.dataset.level = lv;
        else delete c.now.dataset.level;
        if (lv) c.now.dataset.tip = LEVEL_TEXT[lv].replace(/^./, (x) => x.toUpperCase());
        else delete c.now.dataset.tip;
      }
      // Min og maks kan komme fra en tidligere økt (r.n teller bare denne).
      setNum(c.min, Number.isFinite(r.min) ? f(r.min) : '–');
      setNum(c.max, Number.isFinite(r.max) ? f(r.max) : '–');
      setNum(c.avg, r.discrete || !r.n ? '–' : f(r.sum / r.n));
      if (r.discrete) c.avg.dataset.tip = 'Snitt gir ikke mening for en generasjon.';
      if (app.settings.sensorSparks) drawSpark(r.sp, r.hist, { now, span: SPAN, fixed: r.fixed, minSpan: r.minSpan });
    }
    const sinceText = F.since(since);
    setText($('#sn-since'), `Min/maks siden ${sinceText}`);
    setTip($('#sn-since'), keep() ? 'Min og maks huskes mellom økter. Snitt gjelder denne økten.' : 'Min, maks og snitt gjelder denne økten.');
    setTip($('#sn-reset'), `Nullstill min, maks og snitt (regnet fra ${sinceText})`);
  }

  // ---- Data inn ----
  function onGpu(s) {
    const t = s.t || Date.now();
    for (const [k] of GPU_ROWS) push(rows.get(`gpu/${k}`), s[k], t);
    if (s.pstate !== gpuState.pstate || s.throttle !== gpuState.throttle) {
      gpuState = { pstate: s.pstate, throttle: s.throttle };
      renderGpuState();
    }
    dirty = true;
  }

  function onSys(s) {
    const sig = [s.cpu.coreLoad.length, Object.keys(s.net).join(','), s.hwmon.map((d) => d.sensors.map((x) => x.id).join(',')).join(';')].join('|');
    if (sig !== sysSig) {
      sysSig = sig;
      buildOrder(s);
    }
    const t = s.t || Date.now();
    const g = (id) => rows.get(id);
    const ct = cpuTempSensor(s);
    const dev = s.hwmon.find((d) => d.name === 'k10temp' || d.name === 'coretemp' || d.name === 'zenpower');
    for (const x of dev?.sensors || []) push(g(x.id === ct?.id ? 'cpu/temp' : `cpu/${x.id}`), x.value, t);
    push(g('cpu/load'), s.cpu.load, t);
    s.cpu.coreLoad.forEach((v, i) => {
      const r = g(`cpu/t${i}`);
      push(r, v, t);
      if (r?.clockEl && visible) setText(r.clockEl, s.cpu.coreMHz[i] == null ? '' : F.mhz(s.cpu.coreMHz[i]));
    });
    const m = s.mem;
    const GiB = 1024 * 1024;
    push(g('mem/used'), m.usedKB == null ? null : m.usedKB / GiB, t);
    push(g('mem/usedPct'), m.usedKB == null || !m.totalKB ? null : (m.usedKB / m.totalKB) * 100, t);
    push(g('mem/avail'), m.availableKB == null ? null : m.availableKB / GiB, t);
    push(g('mem/swap'), m.swapUsedKB == null ? null : m.swapUsedKB / GiB, t);
    for (const [name, n] of Object.entries(s.net)) {
      push(g(`net/${name}/rx`), n.rx, t);
      push(g(`net/${name}/tx`), n.tx, t);
    }
    for (const d of s.hwmon) {
      if (d === dev) continue;
      for (const x of d.sensors) push(g(`hw/${x.id}`), x.value, t);
    }
    dirty = true;
  }

  async function loadNotes() {
    try {
      const r = await app.api.sensors();
      $('#sn-notes').replaceChildren(
        ...r.notes.map((n) => {
          const root = /root/i.test(n.why);
          return el('div', { class: 'sn-note' }, el('span', { 'data-tip': root ? 'Krever root' : 'Ikke støttet eller ikke tilgjengelig' }, icon(root ? 'lock' : 'circle-slash')), el('b', { text: `${n.group} · ${n.sensor}` }), el('span', { class: 'why', text: n.why }));
        }),
      );
      // Gruppen vises først når den har innhold; den står sist, så ingenting over den flytter seg.
      notesLoaded = r.notes.length > 0;
      applyFilter();
    } catch {}
  }

  // ---- Handlinger ----
  $('#sn-reset').addEventListener('click', () => {
    const snap = [...rows.values()].map((r) => [r, r.min, r.max, r.sum, r.n]);
    const oldSince = since;
    for (const r of rows.values()) {
      const has = r.now != null;
      r.min = has ? r.now : Infinity;
      r.max = has ? r.now : -Infinity;
      r.sum = has ? r.now : 0;
      r.n = has ? 1 : 0;
    }
    since = new Date();
    dirty = true;
    sendMinmax({ replace: true, tell: true });
    // Angre er viktigere når tallene er flere dager gamle.
    app.toast('Min, maks og snitt er nullstilt', {
      body: keep() ? `Tallene var regnet fra ${F.since(oldSince)}.` : '',
      action: {
        label: 'Angre',
        run: () => {
          for (const [r, a, b, c, d] of snap) Object.assign(r, { min: a, max: b, sum: c, n: d });
          since = oldSince;
          dirty = true;
          sendMinmax({ replace: true, tell: true });
        },
      },
    });
  });
  $('#sn-toggle').addEventListener('click', () => {
    const allClosed = [...groups.keys()].every((id) => collapsed.has(id));
    for (const id of groups.keys()) setOpen(id, allClosed);
  });

  /** ?vis=gpu/temp: åpner gruppen, ruller til raden og fremhever den. */
  function reveal(q) {
    const id = q?.get('vis');
    if (!id) return;
    history.replaceState(null, '', '#/sensorer');
    const r = rows.get(id) || [...rows.values()].find((x) => x.id.startsWith(`${id}/`));
    if (!r?.tr) return;
    if (collapsed.has(r.group)) setOpen(r.group, true);
    if (r.thread != null && !threadsOpen) setThreads(true);
    requestAnimationFrame(() => {
      r.tr.scrollIntoView({ block: 'center' });
      r.tr.dataset.flash = '';
      setTimeout(() => delete r.tr.dataset.flash, 1600);
    });
  }

  buildOrder(null);
  for (const s of feed.history) onGpu(s);
  for (const s of feed.sysHistory) onSys(s);
  feed.on('sample', onGpu);
  feed.on('sys', onSys);
  let lastMode = null;
  feed.on('hello', (info) => {
    if (info.mode !== lastMode) {
      lastMode = info.mode;
      for (const r of rows.values()) if (r.group === 'gpu') Object.assign(r, { now: null, min: Infinity, max: -Infinity, sum: 0, n: 0, hist: [] });
      // Lagrede GPU-tall gjelder bare samme kilde (live eller simulert).
      for (const r of rows.values()) if (r.group === 'gpu') seed(r);
      for (const s of feed.history.slice(-130, -1)) onGpu(s);
      mmDirty = true;
    }
    for (const [id, g] of groups) setText(g.btn.querySelector('.note'), groupNote(id));
    if (visible) loadNotes();
  });
  app.on('unit', () => {
    for (const r of rows.values()) r.unit = r.rate && r.n ? F.rateUnit(r.max) : null;
    dirty = true;
  });
  app.on('settings', (k) => {
    if (k.includes('sensorSparks') || k.includes('decimals') || k.includes('thresholds')) dirty = true;
    if (k.includes('statusItems')) renderPins();
    if (k.includes('sensorKeepMinMax')) {
      // Slått på: det denne økten har sett, blir utgangspunktet. Slått av: serveren har glemt alt.
      if (keep()) sendMinmax({ replace: true });
      else stored = null;
      dirty = true;
    }
  });

  // ---- Min og maks mellom økter ----
  function keep() {
    return !!app.settings.sensorKeepMinMax;
  }
  function curMode() {
    return lastMode || feed.info?.mode || 'sim';
  }

  /** Utvider radens min/maks med det lagrede settet (GPU bare fra samme kilde). */
  function seed(r) {
    const v = stored?.rows?.[r.id];
    if (!v || (r.group === 'gpu' && stored.mode !== curMode())) return;
    if (v[0] < r.min) r.min = v[0];
    if (v[1] > r.max) r.max = v[1];
  }

  /**
   * Tar i bruk et sett fra serveren.
   * merge: laveste min og høyeste maks (samme økt, eller første lasting).
   * replace: nullstilt eller angret i en annen fane – serverens tall gjelder.
   */
  function adopt(m, how) {
    if (!m?.since) return;
    const t = new Date(m.since);
    if (how === 'response') {
      if (t.getTime() > since.getTime() + 1000) how = 'replace';
      else if (t.getTime() < since.getTime() - 1000) return;
      else how = 'merge';
    }
    stored = m;
    if (how === 'replace') {
      for (const r of rows.values()) {
        const has = r.now != null;
        r.min = has ? r.now : Infinity;
        r.max = has ? r.now : -Infinity;
      }
      since = t;
    } else if (t < since) since = t;
    for (const r of rows.values()) seed(r);
    dirty = true;
  }

  function minmaxBody(extra = {}) {
    const out = {};
    for (const r of rows.values()) if (Number.isFinite(r.min) && Number.isFinite(r.max)) out[r.id] = [r.min, r.max];
    return { since: since.toISOString(), mode: curMode(), rows: out, ...extra };
  }

  /** Sender min/maks til serveren. replace: nullstilling eller angre. tell: si fra til andre faner. */
  async function sendMinmax({ replace = false, tell = false } = {}) {
    if (!keep() || app.offline) return;
    if (mmSending && !replace) return;
    mmSending = true;
    mmDirty = false;
    try {
      const r = await app.api.putMinmax(minmaxBody(replace ? { replace: true } : {}));
      if (!replace) adopt(r.minmax, 'response');
      else stored = r.minmax;
      if (tell) app.notifyTabs?.('minmax');
    } catch {
      mmDirty = true; // prøv igjen neste runde
    } finally {
      mmSending = false;
    }
  }

  async function loadMinmax(how) {
    if (!keep()) return;
    try {
      const r = await app.api.minmax();
      if (r.minmax?.since) adopt(r.minmax, how);
      mmDirty = true; // send det denne fanen har sett (flettes på serveren)
    } catch {}
  }
  loadMinmax('merge');
  setInterval(() => {
    if (mmDirty) sendMinmax();
  }, MM_EVERY);
  // Siste tall når fanen lukkes eller går i bakgrunnen.
  const flushMinmax = () => {
    if (keep() && mmDirty) {
      mmDirty = false;
      beaconMinmax(minmaxBody());
    }
  };
  window.addEventListener('pagehide', flushMinmax);
  document.addEventListener('visibilitychange', () => document.visibilityState === 'hidden' && flushMinmax());
  // Nullstilt eller angret i en annen fane.
  app.on('tabs', (msg) => msg === 'minmax' && loadMinmax('replace'));

  // Statusblokken leser festede sensorer herfra (verdi, historikk, navn).
  app.sensors = {
    row: (id) => rows.get(id) || null,
    color: (id) => (rows.get(id) ? rowColor(rows.get(id)) : null),
    list: () => [...rows.values()].filter((r) => r.tr || r.group === 'gpu'),
  };

  let stop = null;
  return {
    show(q) {
      visible = true;
      dirty = true;
      renderToggle();
      loadNotes();
      stop = onFrame(() => {
        if (dirty) {
          dirty = false;
          paint();
        }
        return detail.panel ? detail.panel.chart.frame() : false;
      });
      reveal(q);
    },
    query: reveal,
    hide() {
      visible = false;
      stop?.();
      stop = null;
    },
    onKey(e) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        $('#sn-search').focus();
        $('#sn-search').select();
        return true;
      }
      if (e.key === 'Escape' && document.activeElement === $('#sn-search') && filter) {
        $('#sn-search').value = '';
        filter = '';
        applyFilter();
        return true;
      }
      return false;
    },
  };
}
