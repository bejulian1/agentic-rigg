// Grafikkort: opplysninger fra driveren (nvidia-smi og sysfs) ved siden av en
// innebygd spesifikasjonstabell som er tydelig merket som referanse.

import { $, el, icon, onFrame } from '../util.js';
import * as F from '../fmt.js';
import { copyText, kvToMd, kvToText, tableToMd, tableToText } from '../copy.js';
import { hexId, maskUuid, pcieExplain, pcieSpec, pcieText } from '../gpufmt.js';
import { Bar } from '../meters.js';
import { kv, kvSub, setKv, skelKv, skelTable } from '../rows.js';
import { confirmDone, menu, togglePopover } from '../ui.js';
import { launchDay } from '../easter.js';

const VENDORS = { '10de': 'NVIDIA', 1002: 'AMD', 8086: 'Intel', 1569: 'Palit', 1462: 'MSI', 1043: 'ASUS', 3842: 'EVGA', 1458: 'Gigabyte', '19da': 'Zotac' };

export function initGpu(app) {
  const { feed } = app;
  let info = null;
  let showUuid = false;
  let visible = false;
  let pcieDd = null;
  let plimitDd = null;
  const clockCells = [];

  function render() {
    if (!info) return;
    const g = info.driver || {};
    const q = info.query || {};
    const pci = info.pci;
    const spec = info.spec;
    const ids = info.ids || {};
    const sim = info.simulated;
    const s = feed.latest || info.latest || {};

    const src = $('#gc-src');
    src.textContent = sim ? 'Simulert' : 'nvidia-smi og sysfs';
    src.className = `tag${sim ? ' warn' : ''}`;

    // ---- Fra driveren ----
    const d = $('#gc-driver');
    d.replaceChildren();
    kv(d, 'Navn', g.name);
    kv(d, 'Merke', q.productBrand || null);
    kv(d, 'GPU-brikke', spec ? spec.chip : null, { info: q.partNumber ? `Delenummer ${q.partNumber}. Brikken er slått opp på PCI-ID.` : 'Slått opp på PCI-ID.' });
    const dev = hexId(ids.vendor, ids.device);
    kv(d, 'PCI-ID, enhet', dev ? `${dev} · ${VENDORS[ids.vendor] || 'ukjent leverandør'}` : null, { sel: 'all' });
    const sub = hexId(ids.subVendor, ids.subDevice);
    const subSame = ids.subVendor && ids.subVendor === ids.vendor;
    kv(d, 'PCI-ID, subsystem', sub ? `${sub} · ${subSame ? 'samme som enheten' : VENDORS[ids.subVendor] || 'ukjent produsent'}` : null, {
      info: subSame ? 'Subsystemet melder NVIDIAs egen ID, ikke kortprodusentens. Derfor kan ikke appen lese hvem som har laget kortet.' : null,
    });
    // Simulert kilde: verdier som ikke finnes, står som «–» (merket øverst sier at kilden er simulert).
    const simNa = sim ? { naText: '–', tip: 'Simulert kilde' } : {};
    const real = (v) => (v && v !== '—' && !/^simulert$/i.test(v) ? v : null);
    kv(d, 'Bus-ID', real(g.busId), { info: pci?.driver ? `Kjernedriver: ${pci.driver}` : null, ...simNa });
    kv(d, 'VBIOS', g.vbios, { sel: 'all' });
    kv(d, 'Driver', real(g.driver), { sel: 'all', ...simNa });
    kv(d, 'CUDA', real(q.cudaVersion), simNa);
    kv(d, 'Compute capability', g.computeCap);
    pcieDd = kv(d, 'PCIe', pcieText(g.pcieWidthMax, g.pcieGenMax, s.pcieWidth, s.pcieGen), {
      info: [pcieExplain(g.pcieGenMax, s.pcieGen), pci ? `sysfs: ${pci.linkSpeed} x${pci.linkWidth}, maks ${pci.linkSpeedMax} x${pci.linkWidthMax}` : null].filter(Boolean).join('\n') || null,
    });
    kv(d, 'Videominne', g.vramTotal != null ? F.mib(g.vramTotal) : null);
    kv(d, 'BAR1', q.bar1Total != null ? `${F.mib(q.bar1Total)}${q.bar1Used != null ? ` · ${F.mib(q.bar1Used)} i bruk` : ''}` : null);
    kv(d, 'Resizable BAR', rebarText(pci, g, q, sim));
    kv(d, 'Vifter', q.fanSpeedCount > 1 ? String(q.fanSpeedCount) : null, {
      naText: 'Ikke oppgitt',
      info: q.fanSpeedCount > 1 ? null : 'nvidia-smi oppgir én samlet viftefart og ikke antall vifter.',
    });
    if (g.uuid) {
      const text = el('span', { class: `uuid${showUuid ? ' sel-all' : ''}`, text: showUuid ? g.uuid : maskUuid(g.uuid) });
      const eye = el('button', { type: 'button', class: 'btn sm flat icon-only', 'data-tip': showUuid ? 'Skjul UUID' : 'Vis UUID', 'aria-label': showUuid ? 'Skjul UUID' : 'Vis UUID' }, icon(showUuid ? 'eye-off' : 'eye'));
      eye.addEventListener('click', () => {
        showUuid = !showUuid;
        render();
      });
      const cp = el('button', { type: 'button', class: 'btn sm flat icon-only', 'data-tip': 'Kopier UUID', 'aria-label': 'Kopier UUID' }, icon('copy'));
      cp.addEventListener('click', async () => {
        const ok = await copyText(g.uuid);
        if (ok) confirmDone(cp, { tip: 'UUID kopiert' });
        else app.toast('Kunne ikke kopiere', { body: 'Utklippstavlen er ikke tilgjengelig.', error: true });
      });
      const dd = kv(d, 'UUID', el('span', { style: { display: 'inline-flex', alignItems: 'center', gap: '2px' } }, text, eye, cp));
      dd.dataset.copy = showUuid ? g.uuid : maskUuid(g.uuid);
    } else kv(d, 'UUID', null);
    if (q.gspFirmware) kv(d, 'GSP-fastvare', q.gspFirmware);
    if (q.inforomImage) kv(d, 'InfoROM', q.inforomImage);

    // ---- Klokker (uten død «Standard»-kolonne) ----
    const clocks = [
      ['Grafikk', s.coreClock, g.coreMax],
      ['SM', s.smClock, g.smMax],
      ['Minne', s.memClock, g.memMax],
      ['Video', s.videoClock, q.maxClockVideo],
    ];
    // Samme form som Klokker på Oversikt: stolpe for nå mot maks, så tallene
    // ikke står alene midt i kortet.
    clockCells.length = 0;
    $('#gc-clocks').replaceChildren(
      el('thead', {}, el('tr', {}, el('th', { text: '' }), el('th', { class: 'c-bar' }), el('th', { 'data-align': 'end', text: 'Nå' }), el('th', { 'data-align': 'end', text: 'Maks (driver)' }))),
      el(
        'tbody',
        {},
        ...clocks.map(([name, now, max]) => {
          const td = el('td', { 'data-align': 'end', text: F.num(now) });
          const barEl = el('div', { class: 'bar thin' }, el('i'));
          const bar = new Bar(barEl);
          if (max) bar.set(now / max);
          clockCells.push({ td, bar, max });
          return el('tr', {}, el('td', { text: name }), el('td', { class: 'c-bar' }, barEl), td, el('td', { 'data-align': 'end', class: max == null ? 'na' : null, text: max == null ? '–' : F.num(max) }));
        }),
      ),
    );
    const std = g.coreDefault ?? q.defaultClockGr;
    $('#gc-clock-note').textContent = [
      std == null ? 'Driveren oppgir ikke standardklokker (applications clocks er avviklet).' : `Standard grafikkklokke: ${F.mhz(std)}.`,
      spec ? `Boost etter spesifikasjonen er ${F.mhz(spec.boostMHz)} (referanse, ikke målt).` : null,
    ]
      .filter(Boolean)
      .join(' ');

    // ---- Grenser ----
    const l = $('#gc-limits');
    l.replaceChildren();
    plimitDd = kv(l, 'Effektgrense nå', F.watt(s.powerLimit ?? g.powerLimit));
    kv(l, 'Standard / område', g.powerMin != null ? `${F.watt(g.powerDefault)} · ${F.num(g.powerMin)}–${F.watt(g.powerMax)}` : null);
    kv(l, 'Temperaturmål', q.tempTarget != null ? F.temp(q.tempTarget) : null, { info: 'Akustisk mål: kortet senker klokken for å holde seg her.' });
    kv(l, 'Maks driftstemperatur', q.tempMaxOperating != null ? F.temp(q.tempMaxOperating) : null);
    kv(l, 'Nedklokking', q.tempSlowdown != null ? F.temp(q.tempSlowdown) : null);
    kv(l, 'Avstenging', q.tempShutdown != null ? F.temp(q.tempShutdown) : null);

    // ---- Referanse ----
    const r = $('#gc-spec');
    r.replaceChildren();
    const note = $('#gc-ref-note');
    if (!spec) {
      note.hidden = false;
      note.textContent = `Ingen oppføring for PCI-ID ${dev || 'ukjent'} i den innebygde tabellen.`;
      return;
    }
    note.hidden = true;
    const dv = spec.derived;
    const formula = (o) => el('span', { 'data-tip': o.formula, text: F.withUnit(F.num(o.value, o.value < 100 ? 1 : 0), o.unit) });
    kvSub(r, 'Brikke');
    kv(r, 'Modell', spec.name);
    kv(r, 'Brikke', `${spec.chip} · ${spec.variant}`);
    kv(r, 'Arkitektur', spec.arch);
    kv(r, 'Prosess', spec.process);
    kv(r, 'Brikkestørrelse', F.withUnit(F.num(spec.dieMm2), 'mm²'));
    kv(r, 'Transistorer', `${F.num(spec.transistorsBn, 1)} mrd.`);
    kv(r, 'CUDA-kjerner', F.num(spec.cuda));
    kv(r, 'SM-er', F.num(spec.sm));
    kv(r, 'Tensor / RT', `${F.num(spec.tensor)} / ${F.num(spec.rt)}`);
    kv(r, 'TMU / ROP', `${F.num(spec.tmu)} / ${F.num(spec.rop)}`);
    kv(r, 'L2-cache', F.withUnit(F.num(spec.l2KB / 1024), 'MiB'));
    kv(r, 'Grunnklokke', F.mhz(spec.baseMHz));
    kv(r, 'Boostklokke', F.mhz(spec.boostMHz));
    kvSub(r, 'Minne');
    kv(r, 'Type', spec.memType);
    kv(r, 'Buss', F.withUnit(spec.busWidth, 'bit'));
    kv(r, 'Mengde', F.withUnit(spec.memGB, 'GB'));
    kv(r, 'Hastighet', `${F.withUnit(spec.memGbps, 'Gbps')} effektivt`);
    kv(r, 'Båndbredde', formula(dv.bandwidth));
    kv(r, 'Leverandør', spec.memVendor, { na: true, info: spec.memVendorNote });
    kvSub(r, 'Ytelse og kort');
    kv(r, 'Pikselfyllrate', formula(dv.pixelFill));
    kv(r, 'Teksturfyllrate', formula(dv.texFill));
    kv(r, 'FP32', formula(dv.fp32));
    kv(r, 'TDP (referansekort)', F.watt(spec.tdpW), { info: 'Kortprodusenten kan sette en annen grense. Driveren oppgir standardgrensen under Grenser.' });
    kv(r, 'PCIe', pcieSpec(spec.bus), { tip: spec.bus });
    kv(r, 'DirectX / Vulkan / OpenGL', `${spec.directx} / ${spec.vulkan} / ${spec.opengl}`);
    launchDay(kv(r, 'Lansert', spec.launch), spec); // Påskeegg (easter.js): lanseringsdagen
  }

  function rebarText(pci, g, q, sim) {
    if (sim) return 'Ukjent i simulert modus';
    if (!pci) return null;
    const bar1 = pci.bars.find((b) => b.bar === 1);
    const bar1MiB = bar1 ? bar1.size / 1024 / 1024 : q.bar1Total;
    const vram = g.vramTotal;
    if (bar1MiB && vram && bar1MiB >= vram) return `På · BAR1 dekker hele videominnet`;
    if (pci.bar1Sizes?.length) {
      const maxS = Math.max(...pci.bar1Sizes);
      if (vram && maxS < vram) return `Ikke støttet · største BAR1 er ${F.mib(maxS)}`;
      return `Av · kortet støtter opptil ${F.mib(maxS)}`;
    }
    return `Av · BAR1 er ${F.mib(bar1MiB)}`;
  }

  function live(s) {
    if (!visible || !info) return;
    const g = info.driver || {};
    if (pcieDd) setKv(pcieDd, pcieText(g.pcieWidthMax, g.pcieGenMax, s.pcieWidth, s.pcieGen) || '–');
    const vals = [s.coreClock, s.smClock, s.memClock, s.videoClock];
    clockCells.forEach((c, i) => {
      const t = F.num(vals[i]);
      if (c.td.textContent !== t) c.td.textContent = t;
      if (c.max) c.bar.set(vals[i] / c.max);
    });
    if (plimitDd && s.powerLimit != null) setKv(plimitDd, F.watt(s.powerLimit));
  }

  async function load() {
    try {
      info = await app.api.gpuinfo();
      render();
    } catch (err) {
      app.toast('Kunne ikke hente skjermkortinfo', { body: err.message, error: true });
    }
  }

  async function doCopy(md) {
    if (!info) return;
    const head = `${app.name} – Grafikkort (${info.simulated ? 'simulert' : 'nvidia-smi og sysfs'}, ${new Date().toLocaleString('nb-NO')})`;
    const sec = (t) => (md ? `\n## ${t}\n` : `\n[${t}]`);
    const kvf = md ? kvToMd : kvToText;
    const tf = md ? tableToMd : tableToText;
    const lines = [md ? `# ${head}` : head, sec('Fra driveren'), ...kvf($('#gc-driver')), sec('Klokker, MHz'), ...tf($('#gc-clocks')), sec('Grenser'), ...kvf($('#gc-limits')), sec('Spesifikasjon – referanse, ikke målt'), ...kvf($('#gc-spec'))];
    const ok = await copyText(lines.join('\n'));
    // Bekreftelse på knappen i stedet for et varsel; feil får varsel.
    if (ok) confirmDone($('#gc-copy'), { tip: `Kopiert som ${md ? 'Markdown' : 'tekst'} (${lines.length} linjer${showUuid ? '' : ', UUID maskert'})` });
    else app.toast('Kunne ikke kopiere', { body: 'Utklippstavlen er ikke tilgjengelig.', error: true });
  }
  const copyMenu = menu([
    { label: 'Kopier som tekst', icon: 'copy', run: () => doCopy(false) },
    { label: 'Kopier som Markdown', icon: 'copy', run: () => doCopy(true) },
  ]);
  $('#gc-copy').addEventListener('click', () => togglePopover($('#gc-copy'), copyMenu, { align: 'end' }));

  feed.on('sample', live);
  feed.on('hello', () => visible && load());
  app.on('unit', render);

  // Plassholdere i samme høyde som innholdet til første svar er her.
  function skeleton() {
    skelKv($('#gc-driver'), 18);
    skelTable($('#gc-clocks'), 4, 3);
    skelKv($('#gc-spec'), 26, [0, 13, 19]);
    skelKv($('#gc-limits'), 6);
  }
  skeleton();

  let stop = null;
  return {
    show() {
      visible = true;
      load();
      stop = onFrame((dt) => {
        let busy = false;
        for (const c of clockCells) busy = c.bar.tick(dt) || busy;
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
