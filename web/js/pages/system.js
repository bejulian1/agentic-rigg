// System: prosessor, cache, instruksjonssett, klokke og last per tråd,
// hovedkort, minne og operativsystem. Faste data hentes én gang; klokker,
// minne og oppetid oppdateres fra systemstrømmen hvert sekund.

import { $, el, onFrame, setText } from '../util.js';
import * as F from '../fmt.js';
import { Bar, level } from '../meters.js';
import { copyText, kvToMd, kvToText, tableToMd, tableToText } from '../copy.js';
import { kv as kvBase, setKv, skelKv, skelTable } from '../rows.js';

// Systemdata kommer ikke fra skjermkortdriveren: manglende verdier er bare «Ikke oppgitt».
const kv = (host, label, value, opts = {}) => kvBase(host, label, value, { naText: 'Ikke oppgitt', ...opts });
import { confirmDone, menu, setTip, togglePopover } from '../ui.js';

const CACHE_TYPE = { Data: 'Data', Instruction: 'Instruksjon', Unified: 'Felles' };
const hex2 = (n) => (n == null ? '?' : n.toString(16).toUpperCase().padStart(2, '0'));
const MON = { Jan: 0, Feb: 1, Mar: 2, Apr: 3, May: 4, Jun: 5, Jul: 6, Aug: 7, Sep: 8, Oct: 9, Nov: 10, Dec: 11 };

/** DMI-dato «03/06/2019» (MM/DD/ÅÅÅÅ) → «6. mars 2019». */
function dmiDate(s) {
  const m = /^(\d{2})\/(\d{2})\/(\d{4})$/.exec(s || '');
  return m ? F.dateLong(Number(m[3]), Number(m[1]) - 1, Number(m[2])) : s || null;
}

/** «#31-Ubuntu SMP PREEMPT_DYNAMIC Sat Aug 1 04:26:38 UTC 2026» → «Bygg #31 · 1. aug. 2026». */
function kernelBuild(s) {
  if (!s) return null;
  const n = /^#(\d+)/.exec(s)?.[1];
  const d = /\b(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{1,2})\s+[\d:]+\s+\w+\s+(\d{4})/.exec(s);
  const parts = [];
  if (n) parts.push(`Bygg #${n}`);
  if (d) parts.push(F.dateShort(Number(d[3]), MON[d[1]], Number(d[2])));
  if (/PREEMPT_DYNAMIC/.test(s)) parts.push('dynamisk forkjøpping');
  return parts.length ? parts.join(' · ') : s;
}

/** XDG_CURRENT_DESKTOP «ubuntu:GNOME» → «GNOME (Ubuntu-økt)». */
function desktopText(raw) {
  if (!raw) return null;
  const parts = raw.split(':').filter(Boolean);
  const main = parts[parts.length - 1];
  const variant = parts.length > 1 ? parts[0] : null;
  const nice = { ubuntu: 'Ubuntu-økt', pop: 'Pop-økt', zorin: 'Zorin-økt' };
  return variant ? `${main} (${nice[variant.toLowerCase()] || `${variant}-økt`})` : main;
}

export function initSystem(app) {
  const { feed } = app;
  let data = null;
  let visible = false;
  let showAll = false;
  let flagFilter = '';
  const cores = [];
  const live = {};

  function render() {
    if (!data) return;
    const { cpu, board, os, memory } = data.static;

    // ---- Prosessor ----
    const c = $('#sy-cpu');
    c.replaceChildren();
    kv(c, 'Navn', cpu.name);
    kv(c, 'Produsent', cpu.vendorName || cpu.vendor, { tip: cpu.vendor });
    kv(c, 'Familie · modell · rev.', `${cpu.family} · ${cpu.model} · ${cpu.stepping}`, { tip: `Familie, modell og stepping (revisjon). Heksadesimalt: ${hex2(cpu.family)}h · ${hex2(cpu.model)}h · ${cpu.stepping}` });
    kv(c, 'Mikrokode-revisjon', cpu.microcode, { sel: 'all' });
    kv(c, 'Kjerner og tråder', `${F.plural(cpu.cores, 'kjerne', 'kjerner')} · ${F.plural(cpu.threads, 'tråd', 'tråder')}`);
    kv(c, 'Klokke (cpufreq)', cpu.freq.available ? `${F.num(cpu.freq.minKHz / 1000)}–${F.mhz(cpu.freq.maxKHz / 1000)}` : null, {
      naText: 'cpufreq er ikke tilgjengelig',
      info: cpu.freq.available ? 'Min og maks fra cpufreq. Med boost på kan kjernene gå over maks.' : null,
    });
    live.gov = kv(c, 'Governor og driver', '–');
    live.boost = kv(c, 'Boost', '–');
    kv(c, 'Adresser', cpu.addressSizes ? cpu.addressSizes.replace('bits physical', 'bit fysisk').replace('bits virtual', 'bit virtuelt').replace(/ bit/g, `${F.NNBSP}bit`) : null);

    // ---- Cache: tekst venstre, tall høyre ----
    $('#sy-cache').replaceChildren(
      el(
        'thead',
        {},
        el('tr', {}, el('th', { text: 'Nivå' }), el('th', { text: 'Type' }), el('th', { 'data-align': 'end', text: 'Per instans' }), el('th', { 'data-align': 'end', text: 'Antall' }), el('th', { 'data-align': 'end', text: 'Totalt' }), el('th', { 'data-align': 'end', text: 'Assosiativitet' })),
      ),
      el(
        'tbody',
        {},
        ...cpu.caches.map((k) =>
          el(
            'tr',
            {},
            el('td', { text: `L${k.level}` }),
            el('td', { text: CACHE_TYPE[k.type] || k.type }),
            el('td', { 'data-align': 'end', text: F.bytes(k.size, 0) }),
            el('td', { 'data-align': 'end', text: `×${F.NNBSP}${k.instances}` }),
            el('td', { 'data-align': 'end', text: F.bytes(k.total, 0) }),
            el('td', { 'data-align': 'end', text: k.ways ? `${k.ways}-veis` : '–' }),
          ),
        ),
      ),
    );

    renderFlags();

    // ---- Kjerner: 8 × 2 fliser ----
    const grid = $('#sy-cores');
    grid.replaceChildren();
    cores.length = 0;
    for (let i = 0; i < cpu.threads; i++) {
      const mhz = el('b', { text: '–' });
      const pct = el('span', { class: 'num', text: '–' });
      const bar = el('div', { class: 'bar thin' }, el('i'));
      grid.append(el('div', { class: 'core', 'data-tip': `Tråd ${i}` }, el('div', { class: 'core-top' }, el('span', { class: 't', text: `T${i}` }), mhz), el('div', { class: 'meter' }, bar, pct)));
      cores.push({ mhz, pct, bar: new Bar(bar) });
    }

    // ---- Hovedkort ----
    const b = $('#sy-board');
    b.replaceChildren();
    const s = (v) => (v && typeof v === 'object' && v.root ? null : v);
    kv(b, 'Produsent', s(board.board_vendor));
    kv(b, 'Modell', s(board.board_name));
    kv(b, 'Versjon', s(board.board_version));
    kv(b, 'System', [s(board.sys_vendor), s(board.product_name)].filter(Boolean).join(' · ') || null);
    kv(b, 'BIOS', [s(board.bios_vendor), s(board.bios_version)].filter(Boolean).join(' · ') || null, { tip: board.bios_release ? `Utgave ${board.bios_release}` : null });
    kv(b, 'BIOS-dato', dmiDate(s(board.bios_date)), { tip: s(board.bios_date) ? `Rå verdi: ${board.bios_date}` : null });
    kv(b, 'Kabinett', board.chassis || null);
    kv(b, 'Serienumre', null, { naText: 'Leses ikke (krever root)', info: 'Serienummerfilene i /sys/class/dmi/id er bare lesbare for root. Appen åpner dem ikke.' });

    // ---- Minne ----
    const m = $('#sy-mem');
    m.replaceChildren();
    live.total = kv(m, 'Totalt', '–');
    const meterRow = (label) => {
      const bar = el('div', { class: 'bar' }, el('i'));
      const num = el('span', { class: 'num', text: '–' });
      const dd = el('dd', {}, el('div', { class: 'meter' }, bar, num));
      m.append(el('div', { class: 'kv-row meter-row' }, el('dt', { text: label }), dd));
      return { bar: new Bar(bar), num, dd };
    };
    live.used = meterRow('Brukt');
    live.avail = kv(m, 'Tilgjengelig', '–');
    live.cache = kv(m, 'Buffer og cache', '–');
    live.swap = meterRow('Swap');
    kv(m, 'Moduler og timinger', null, { naText: 'Krever bakgrunnstjeneste (root)', info: memory.note || 'Moduler, hastighet og timinger ligger i SMBIOS-tabellen, som bare root kan lese.' });

    // ---- OS ----
    const o = $('#sy-os');
    o.replaceChildren();
    kv(o, 'Distribusjon', os.distro);
    kv(o, 'Kjerne', os.kernel, { sel: 'all' });
    kv(o, 'Kjernebygg', kernelBuild(os.kernelBuild), { tip: os.kernelBuild, copy: os.kernelBuild });
    kv(o, 'Arkitektur', os.arch);
    kv(o, 'Skrivebord', [desktopText(os.desktop), os.session].filter(Boolean).join(' · ') || null, {
      tip: [os.desktop ? `XDG_CURRENT_DESKTOP=${os.desktop}` : null, os.session ? `Økttype fra ${os.sessionSource} i miljøet til serveren.` : 'Serveren ble startet uten XDG_SESSION_TYPE, WAYLAND_DISPLAY og DISPLAY.'].filter(Boolean).join('\n'),
    });
    kv(o, 'Vertsnavn', os.hostname);
    live.uptime = kv(o, 'Oppetid', '–');
    live.loadavg = kv(o, 'Last (1 / 5 / 15 min)', '–', { info: 'Gjennomsnittlig antall prosesser som kjører eller venter. Over antall tråder betyr kø.' });

    if (feed.sysLatest) onSys(feed.sysLatest);
  }

  // Instruksjonssett: de utvalgte først. Kortet strekker seg til samme høyde
  // som høyre kolonne, og resten av settene fyller plassen som er igjen (bare
  // hele rader). Utvid viser alle med søk. Knappen er en fast ikonknapp, så
  // ingenting endrer bredde.
  const flagsCard = $('#sy-flags-card');
  const more = $('#sy-flags-more');
  function renderFlags() {
    const { cpu } = data.static;
    const host = $('#sy-flags');
    const btn = $('#sy-flags-toggle');
    $('#sy-flags-search-wrap').hidden = !showAll;
    flagsCard.toggleAttribute('data-all', showAll);
    btn.setAttribute('aria-expanded', String(showAll));
    setTip(btn, showAll ? 'Vis utvalget' : 'Vis alle og søk');
    const imp = new Set(cpu.important.map((f) => f.flag));
    if (!showAll) {
      host.hidden = false;
      host.replaceChildren(...cpu.important.map((f) => el('span', { class: 'chip key', 'data-tip': f.flag, text: f.label })));
      more.replaceChildren(...cpu.flags.filter((f) => !imp.has(f)).map((f) => el('span', { class: 'chip', text: f })));
      fitFlags();
    } else {
      host.hidden = true;
      const list = cpu.flags.filter((f) => !flagFilter || f.includes(flagFilter));
      more.replaceChildren(...list.map((f) => el('span', { class: `chip${imp.has(f) ? ' key' : ''}`, text: f })));
      if (!list.length) more.append(el('span', { class: 'muted', text: 'Ingen treff.' }));
      setText($('#sy-flags-count'), flagFilter ? `${list.length} av ${cpu.flags.length}` : `${cpu.flags.length} i alt`);
    }
  }

  /** Skjuler sett som ikke får plass i en hel rad, og oppdaterer tallet i overskriften. */
  function fitFlags() {
    if (!data || showAll) return;
    const { cpu } = data.static;
    const chips = [...more.children];
    for (const c of chips) c.hidden = false;
    const box = more.getBoundingClientRect();
    // Mål alle først, skjul etterpå (ellers flytter de skjulte de neste opp).
    const fit = chips.map((c) => box.height > 0 && c.getBoundingClientRect().bottom <= box.bottom + 0.5);
    let shown = 0;
    chips.forEach((c, i) => {
      c.hidden = !fit[i];
      if (fit[i]) shown++;
    });
    const n = cpu.important.length + shown;
    setText($('#sy-flags-count'), `${n} av ${cpu.flags.length}`);
    $('#sy-flags-count').dataset.tip = `${cpu.important.length} utvalgte først, deretter så mange av de andre som får plass. Utvid for å se og søke i alle.`;
  }
  let fitW = 0;
  let fitH = 0;
  new ResizeObserver(([e]) => {
    const { width, height } = e.contentRect;
    if (Math.abs(width - fitW) < 1 && Math.abs(height - fitH) < 1) return;
    fitW = width;
    fitH = height;
    fitFlags();
  }).observe(more);

  function onSys(s) {
    if (!visible || !data) return;
    const { cpu } = data.static;
    s.cpu.coreMHz.forEach((v, i) => {
      const c = cores[i];
      if (!c) return;
      setText(c.mhz, v == null ? '–' : F.mhz(v));
      const l = s.cpu.coreLoad[i];
      setText(c.pct, l == null ? '–' : F.pct(l, 0));
      c.bar.set(l == null ? 0 : l / 100, level(l, app.settings.thresholds.loadWarn));
    });
    setText($('#sy-gov'), s.cpu.governor ? `governor ${s.cpu.governor}` : '');
    setKv(live.gov, [s.cpu.governor, cpu.freq.driver].filter(Boolean).join(' · ') || '–');
    setKv(live.boost, s.cpu.boost == null ? 'Ikke oppgitt' : s.cpu.boost ? 'På' : 'Av');

    const m = s.mem;
    setKv(live.total, F.kb(m.totalKB));
    if (m.usedKB != null) {
      live.used.bar.set(m.usedKB / m.totalKB, level(m.usedKB / m.totalKB, 0.85, 0.95));
      setText(live.used.num, `${F.kb(m.usedKB)} · ${F.pct((m.usedKB / m.totalKB) * 100)}`);
    }
    setKv(live.avail, F.kb(m.availableKB));
    setKv(live.cache, F.kb(m.buffersKB + m.cachedKB));
    if (m.swapTotalKB) {
      const frac = m.swapUsedKB / m.swapTotalKB;
      live.swap.bar.set(frac, frac >= 0.99 ? 'warn' : level(frac, 0.8));
      setText(live.swap.num, frac >= 0.99 ? `Full · ${F.kb(m.swapTotalKB, 0)}` : `${F.kb(m.swapUsedKB)} av ${F.kb(m.swapTotalKB, 0)}`);
      live.swap.dd.dataset.tip = frac >= 0.99 ? 'Swap er helt brukt. Systemet kan bli tregt hvis minnet også fylles opp.' : '';
    } else setText(live.swap.num, 'Ingen swap');
    setKv(live.uptime, F.uptime(s.uptime));
    setKv(live.loadavg, s.loadavg.map((v) => F.num(v, 2)).join(' / '));
  }

  // Plassholdere i samme høyde som innholdet til første svar er her.
  function skeleton() {
    skelKv($('#sy-cpu'), 9);
    skelTable($('#sy-cache'), 4, 4);
    $('#sy-flags').replaceChildren(...Array.from({ length: 12 }, (_, i) => el('span', { class: 'chip', 'aria-hidden': 'true' }, el('span', { class: 'skel', style: { '--w': `${2 + (i % 4)}em` } }))));
    skelKv($('#sy-board'), 8);
    skelKv($('#sy-mem'), 6);
    skelKv($('#sy-os'), 8);
    const n = navigator.hardwareConcurrency || 8;
    $('#sy-cores').replaceChildren(...Array.from({ length: n }, () => el('div', { class: 'core', 'aria-hidden': 'true' }, el('div', { class: 'core-top' }, el('span', { class: 'skel', style: { '--w': '1.6em' } }), el('span', { class: 'skel', style: { '--w': '3em' } })), el('div', { class: 'meter' }, el('div', { class: 'bar thin' }), el('span', { class: 'num' }, el('span', { class: 'skel', style: { '--w': '2em' } }))))));
  }
  skeleton();

  async function load() {
    if (data) return render();
    try {
      data = await app.api.system();
      render();
    } catch (err) {
      app.toast('Kunne ikke hente systeminfo', { body: err.message, error: true });
    }
  }

  $('#sy-flags-toggle').addEventListener('click', () => {
    showAll = !showAll;
    if (data) renderFlags();
    if (showAll) $('#sy-flags-search').focus();
  });
  $('#sy-flags-search').addEventListener('input', (e) => {
    flagFilter = e.target.value.trim().toLowerCase();
    renderFlags();
  });

  async function doCopy(md) {
    if (!data) return;
    const head = `${app.name} – System (${new Date().toLocaleString('nb-NO')})`;
    const sec = (t) => (md ? `\n## ${t}\n` : `\n[${t}]`);
    const kvf = md ? kvToMd : kvToText;
    const tf = md ? tableToMd : tableToText;
    const lines = [
      md ? `# ${head}` : head,
      sec('Prosessor'),
      ...kvf($('#sy-cpu')),
      sec('Cache'),
      ...tf($('#sy-cache')),
      sec('Instruksjonssett'),
      data.static.cpu.flags.join(' '),
      sec('Hovedkort og BIOS'),
      ...kvf($('#sy-board')),
      sec('Minne'),
      ...kvf($('#sy-mem')),
      sec('Operativsystem'),
      ...kvf($('#sy-os')),
    ];
    const ok = await copyText(lines.join('\n'));
    if (ok) confirmDone($('#sy-copy'), { tip: `Kopiert som ${md ? 'Markdown' : 'tekst'} (${lines.length} linjer)` });
    else app.toast('Kunne ikke kopiere', { body: 'Utklippstavlen er ikke tilgjengelig.', error: true });
  }
  const copyMenu = menu([
    { label: 'Kopier som tekst', icon: 'copy', run: () => doCopy(false) },
    { label: 'Kopier som Markdown', icon: 'copy', run: () => doCopy(true) },
  ]);
  $('#sy-copy').addEventListener('click', () => togglePopover($('#sy-copy'), copyMenu, { align: 'end' }));

  feed.on('sys', onSys);
  app.on('unit', () => visible && render());

  let stop = null;
  return {
    show() {
      visible = true;
      load();
      stop = onFrame((dt) => {
        let busy = false;
        for (const c of cores) busy = c.bar.tick(dt) || busy;
        busy = live.used?.bar.tick(dt) || busy;
        busy = live.swap?.bar.tick(dt) || busy;
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
