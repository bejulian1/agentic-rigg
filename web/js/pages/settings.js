// Innstillinger (v0.0.15): dekker hele vinduet, med egen navigasjon til
// venstre (tilbake, tittel, søk og delene) og gruppene som paneler i én til
// tre spalter. Skallet (data-view="settings", tilbake, rulling og fokus) står
// i app.js, route() og closeSettings(); valgene i docs/innstillinger-v0.0.15.md.
//
// Alt trer i kraft straks og lagres i datamappa (state.json) uten toast. Toast
// brukes bare når noe skjer som ikke synes (f.eks. «gjelder neste gang
// serveren starter»). Søket leter i alle delene og viser radene som passer,
// med kontrollene i behold, så de kan endres rett fra treffene.

import { $, el, icon, local, animate, dur, EASE } from '../util.js';
import * as F from '../fmt.js';
import { bindSeg, closePopover, confirmDone, numField, segEl, setSeg, setTip, switchEl, toast, togglePopover } from '../ui.js';
import { PAGES, STATUS_DEFS } from '../defs.js';
import { describe, pinKey, statusMax } from '../pins.js';
import { CHART_DEFS, CHART_ORDER } from './monitor.js';
import { WINDOWS } from '../history.js';
import { copyText } from '../copy.js';
import { resolveMotion } from '../prefs.js';
import { aboutEgg, surprisesRow } from '../easter.js';

// [id, navn, ikon, ingress]. id-ene brukes av openSettings(), localStorage og testene.
const SECTIONS = [
  ['generelt', 'Generelt', 'settings', 'Oppstart, datakilde og bakgrunnstjenesten'],
  ['utseende', 'Utseende', 'sun-moon', 'Lyst eller mørkt, farger, størrelse og bevegelse'],
  ['format', 'Enheter og format', 'ruler', 'Enheter, desimaler og klokkeslett'],
  ['sidefelt', 'Sidefelt', 'panel-left', 'Statusblokken, bredden og sidene i sidefeltet'],
  ['grafer', 'Grafer og sensorer', 'chart-area', 'Tidsvindu, grafer, sensorer og varselfarger'],
  ['data', 'Data og tilbakestilling', 'database', 'Datamappe, eksport, import og tilbakestilling'],
  ['om', 'Om', 'info', 'Versjon, hjelp og feilsøking'], // v0.0.10: var en dialog i hovedmenyen
];
// v0.0.11: seks aksentfarger. Gul, oransje og rød er tatt bort, fordi de
// samme fargene betyr advarsel og fare i appen (docs/grafer-og-malere-v0.0.11.md).
const ACCENTS = [
  ['blue', 'Blå', '#3584e4'],
  ['teal', 'Blågrønn', '#2190a4'],
  ['green', 'Grønn', '#3a944a'],
  ['purple', 'Lilla', '#9141ac'],
  ['pink', 'Rosa', '#d56199'],
  ['slate', 'Skifer', '#6f8396'],
];
// Fargetemaer: egne navn, ingen merkenavn. Verdiene står i styles.css, seksjon 2.
const PALETTES = [
  ['grafitt', 'Grafitt', 'Nøytral grå'],
  ['sand', 'Sand', 'Varm, som papir'],
  ['fjord', 'Fjord', 'Kjølig blågrå'],
  ['midnatt', 'Midnatt', 'Dypt mørkt, skarpt lyst'],
];
const STATUS_ORDER = ['gpuTemp', 'gpuLoad', 'gpuPower', 'vram', 'gpuFan', 'cpuTemp', 'cpuLoad', 'mem', 'netDown', 'netUp'];
/** Under denne bredden står navigasjonen øverst med bare ikoner (samme grense som i styles.css). */
const MQ_TOP = '(max-width: 700px)';

const norm = (t) => t.toLocaleLowerCase('nb').replace(/\s+/g, ' ');
const escRe = (t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

export function initSettings(app) {
  const S = () => app.settings;
  let section = local.get('ag-settings-section', 'generelt');
  if (!SECTIONS.some((s) => s[0] === section)) section = 'generelt';
  let visible = false;
  let selfChange = false;
  let query = '';
  let uid = 0;
  const nid = (p) => `${p}-${++uid}`;

  const nav = $('#set-nav');
  const body = $('#set-body');
  const search = $('#set-search');
  const backBtn = $('#set-back');
  const heading = $('#set-heading');
  const leadEl = $('#set-lead');
  const countEl = $('#set-count');
  const mqTop = window.matchMedia(MQ_TOP);

  async function save(patch) {
    // Bare den synkrone endringen (optimistisk) er «vår». Endringer som kommer
    // mens lagringen venter på svar (en annen fane, et annet sted i appen,
    // eller tilbakerulling ved feil), skal tegne siden på nytt.
    selfChange = true;
    let p;
    try {
      p = app.saveSettings(patch);
    } finally {
      selfChange = false;
    }
    return p;
  }

  // ---- Byggeklosser ----
  // En rad: etikett og forklaring til venstre, kontrollen til høyre. Forklaringen
  // kobles til kontrollen (aria-describedby), så skjermlesere leser den også.
  // k: ekstra søkeord som ikke står i teksten.
  function rowEl(tag, cls, title, subtitle, ctrl, k) {
    const sid = subtitle ? nid('sd') : null;
    const r = el(
      tag,
      { class: cls, 'data-k': k || null },
      el('div', { class: 'text' }, el('span', { class: 'title', text: title }), subtitle ? el('span', { class: 'subtitle', id: sid, text: subtitle }) : null),
      el('div', { class: 'ctrl' }, ctrl),
    );
    if (sid) r.querySelector('.ctrl [role=radiogroup], .ctrl input, .ctrl select, .ctrl button')?.setAttribute('aria-describedby', sid);
    return r;
  }
  const row = (title, subtitle, ctrl, { wrap = false, k = '' } = {}) => rowEl('div', `row${wrap ? ' wrap' : ''}`, title, subtitle, ctrl, k);
  /** Hele raden kan klikkes for en bryter. */
  const switchRow = (title, subtitle, checked, onChange, { k = '' } = {}) => rowEl('label', 'row', title, subtitle, switchEl(checked, onChange, { label: title }).root, k);
  const seg = (opts, val, key, label, map = (v) => v) => segEl(opts, val, (v) => save({ [key]: map(v) }), { label });
  /**
   * Et panel med overskriftsstripe. desc står over radene, note under dem.
   * whole: søket viser hele panelet eller ingenting (lister og rutenett).
   */
  function group(title, { desc = null, note = null, aside = null, whole = false, k = '', cls = '' } = {}, ...rows) {
    const hid = nid('sg');
    return el(
      'section',
      { class: `group set-group${cls ? ` ${cls}` : ''}`, 'aria-labelledby': hid, 'data-whole': whole ? '' : null, 'data-k': k || null },
      el('div', { class: 'group-head' }, el('h3', { class: 'group-title', id: hid, text: title }), aside ? el('div', { class: 'aside' }, aside) : null),
      desc ? el('p', { class: 'set-desc', text: desc }) : null,
      el('div', { class: 'list' }, ...rows),
      note ? el('p', { class: 'set-note', text: note }) : null,
    );
  }
  /** Datamappe: «/run/media/julian/…/agentic-rigg/.data», hele stien i verktøytipset. */
  const pathRow = (dir, copyBtn) => {
    const parts = dir.split('/');
    const tail = parts.length > 3 ? parts.slice(-2).join('/') : dir;
    const head = parts.length > 3 ? `${parts.slice(0, -2).join('/')}/` : '';
    const code = el('code', { class: 'mid-ellipsis', 'data-tip': dir, 'aria-label': dir }, el('span', { class: 'h', text: head }), el('span', { class: 't', text: tail }));
    return el('div', { class: 'row path-row', 'data-k': 'mappe sti lagring' }, el('div', { class: 'text' }, el('span', { class: 'title', text: 'Datamappe' })), el('div', { class: 'ctrl' }, el('span', { class: 'path-box' }, code, copyBtn)));
  };

  /** Status for bakgrunnstjenesten. Oppdateres på stedet når den kobles til eller fra. */
  const daemonSub = el('span', { class: 'subtitle' });
  const daemonTag = el('span', { class: 'tag' });
  const daemonNote = el('p', { class: 'set-note' });
  function renderDaemon() {
    const d = app.daemon;
    const on = !!d?.connected;
    daemonTag.className = `tag${on ? ' ok' : ' warn'}`;
    daemonTag.textContent = on ? 'Tilkoblet' : 'Ikke tilkoblet';
    daemonSub.textContent = on ? `Simulert: ${d.info?.name || 'daemon-mock'} ${d.info?.version || ''} · protokoll ${d.info?.protocol ?? '–'}` : 'Startes sammen med appen (npm start).';
    daemonNote.textContent = on
      ? 'Tjenesten klemmer verdiene til kortets grenser og ruller tilbake uten Behold. Ingenting sendes til skjermkortet.'
      : 'Kontroller og Viftekurve kan ikke brukes før tjenesten svarer.';
  }
  const daemonGroup = (confirmRow) => {
    renderDaemon();
    const g = group('Bakgrunnstjeneste', { k: 'daemon tjeneste' }, el('div', { class: 'row', 'data-k': 'tilkoblet status' }, el('div', { class: 'text' }, el('span', { class: 'title', text: 'Status' }), daemonSub), el('div', { class: 'ctrl' }, daemonTag)), confirmRow);
    g.append(daemonNote);
    return g;
  };
  app.on('daemon', renderDaemon);

  // ---- Delene ----
  const build = {
    generelt() {
      const sel = el('select', { class: 'field', 'aria-label': 'Startside' }, ...PAGES.map(([id, name]) => el('option', { value: id, text: name })), el('option', { value: 'last', text: 'Sist brukte side' }));
      sel.value = S().startPage;
      sel.addEventListener('change', () => save({ startPage: sel.value }));
      const info = app.feed.info;
      const live = info?.mode === 'live';
      // Én setning om kilden, uten miljøvariabler: «Simulert skjermkort (låst ved oppstart av serveren).»
      const why = app.state.forceSim ? 'låst ved oppstart av serveren' : info && info.reason !== 'nvidia-smi' ? info.reason.charAt(0).toLowerCase() + info.reason.slice(1) : '';
      const srcText = live ? 'Live fra nvidia-smi.' : `Simulert skjermkort${why ? ` (${why})` : ''}.`;
      const srcSeg = segEl(
        [
          ['live', 'Live', app.state.forceSim ? 'Live er låst: serveren ble startet i simulert modus' : null],
          ['sim', 'Simulert'],
        ],
        info?.mode || 'sim',
        (v) => app.switchSource(v),
        { label: 'Datakilde' },
      );
      if (app.state.forceSim) srcSeg.querySelector('[data-v="live"]').disabled = true;
      const startSim = switchRow('Start med simulert GPU', 'Neste gang serveren starter, også når nvidia-smi finnes.', S().startSimulated, async (on) => {
        if (await save({ startSimulated: on })) toast(on ? 'Starter i simulert modus' : 'Starter med live-data', { body: 'Gjelder neste gang serveren starter.' });
      }, { k: 'oppstart simulering' });
      // v0.0.12: påskeeggene (easter.js). På som standard; av slår av alle.
      const eggs = surprisesRow(switchRow, save);
      eggs.dataset.k = 'påskeegg egg';
      return [
        group('Oppstart', {}, row('Startside', 'Siden som åpnes når appen starter.', sel, { k: 'forside start' })),
        group(
          'Skjermkortdata',
          { k: 'gpu data' },
          row('Datakilde', `${srcText} CPU, minne og nett er alltid ekte.`, srcSeg, { k: 'live simulert nvidia-smi kilde' }),
          startSim,
          row('Oppdateringsintervall', 'Hvor ofte GPU-tallene og grafene får nye verdier. Systemsensorene leses hvert sekund.', seg([[500, '0,5 s'], [1000, '1 s'], [2000, '2 s']], S().intervalMs, 'intervalMs', 'Oppdateringsintervall', Number), { k: 'intervall frekvens hastighet' }),
        ),
        daemonGroup(row('Frist for å beholde endringer', 'Etter Bruk på Kontroller og Viftekurve må du trykke Behold innen fristen, ellers rulles endringen tilbake.', seg((app.state.confirmChoices || [10, 15, 30, 60]).map((v) => [v, `${v} s`]), S().confirmSec, 'confirmSec', 'Frist for å beholde endringer', Number), { k: 'behold bekreft tilbakerulling sekunder' })),
        group('Annet', {}, eggs),
      ];
    },

    utseende() {
      const themeSeg = seg([['system', 'System'], ['light', 'Lys'], ['dark', 'Mørk']], S().theme, 'theme', 'Lyst eller mørkt');
      // Fargetema som små miniatyrer. Hver miniatyr får temaets egne tokens via
      // .pv[data-palette] i styles.css, i lyst eller mørkt etter det som vises nå.
      const pals = el('div', { class: 'pal-cards', role: 'radiogroup', 'aria-label': 'Fargetema' });
      for (const [v, name, desc] of PALETTES) {
        pals.append(
          el(
            'button',
            { type: 'button', class: 'pal-card', role: 'radio', 'data-v': v, 'aria-label': `${name}: ${desc}` },
            el(
              'span',
              { class: 'pv-frame' },
              el(
                'span',
                { class: 'pv', 'data-palette': v, 'aria-hidden': 'true' },
                el('span', { class: 'pv-side' }, el('i'), el('i'), el('i')),
                el('span', { class: 'pv-main' }, el('span', { class: 'pv-card' }, el('b'), el('i'), el('s'))),
              ),
              el('span', { class: 'pv-check' }, icon('check')),
            ),
            el('span', { class: 'pal-name', text: name }),
            el('span', { class: 'pal-desc', text: desc }),
          ),
        );
      }
      setSeg(pals, S().palette);
      bindSeg(pals, (v) => {
        setSeg(pals, v);
        save({ palette: v });
      });
      const acc = el('div', { class: 'accent-swatches', role: 'radiogroup', 'aria-label': 'Aksentfarge' });
      for (const [v, name, col] of ACCENTS) {
        acc.append(el('button', { type: 'button', role: 'radio', 'data-v': v, 'aria-label': name, style: { '--sw': col } }, el('span', { class: 'sw' }, icon('check')), el('span', { class: 'sw-name', text: name })));
      }
      setSeg(acc, S().accent);
      bindSeg(acc, (v) => {
        setSeg(acc, v);
        save({ accent: v });
      });
      const sys = resolveMotion('system');
      return [
        group(
          'Farger',
          { note: 'Nettleseren kan ikke lese aksentfargen i GNOME, så den velges her.' },
          row('Lyst eller mørkt', 'System følger GNOME og bytter med en gang.', themeSeg, { k: 'tema mørk modus lys dark light' }),
          row('Fargetema', 'Tonen i flater, rammer og tekst, i både lyst og mørkt.', pals, { wrap: true, k: 'palett grafitt sand fjord midnatt' }),
          row('Aksentfarge', 'Knapper, valgt side, stolper og glidere. Grafene har faste farger per serie.', acc, { wrap: true, k: 'blå blågrønn grønn lilla rosa skifer' }),
          switchRow('Høy kontrast', 'Sterkere tekst, rammer, fokusringer og grafer i alle fargetemaer.', S().highContrast, (on) => save({ highContrast: on }), { k: 'tilgjengelighet synlighet' }),
        ),
        group(
          'Størrelse',
          {},
          row('Tetthet', 'Høyden på rader og luften mellom panelene.', seg([['compact', 'Kompakt'], ['standard', 'Standard'], ['comfortable', 'Luftig']], S().density, 'density', 'Tetthet'), { k: 'kompakt luftig tett' }),
          row('Skriftstørrelse', 'Teksten i hele appen. Ctrl og +/− i nettleseren zoomer alt.', seg([[92, 'Liten'], [100, 'Standard'], [108, 'Stor'], [116, 'Større']], S().fontScale, 'fontScale', 'Skriftstørrelse', Number), { k: 'tekst zoom skrift' }),
        ),
        group(
          'Bevegelse',
          { note: 'Redusert gir bare korte tonninger, og grafene oppdateres i trinn. Av slår av all animasjon.' },
          row('Animasjoner', `Automatisk følger systemets innstilling (nå: ${sys === 'reduced' ? 'redusert' : 'full'}).`, seg([['system', 'Automatisk'], ['full', 'Full'], ['reduced', 'Redusert'], ['off', 'Av']], S().motion, 'motion', 'Animasjoner'), { k: 'bevegelse animasjon redusert' }),
        ),
      ];
    },

    format() {
      return [
        group(
          'Enheter',
          {},
          row('Temperatur', 'Alle sider, grafer, temperaturmålet og viftekurven.', seg([['C', '°C'], ['F', '°F']], S().unit, 'unit', 'Temperaturenhet'), { k: 'celsius fahrenheit' }),
          row('Minne', 'GiB som Linux-verktøy, eller GB som produsentene.', seg([['bin', 'GiB'], ['dec', 'GB']], S().memUnit, 'memUnit', 'Minneenhet'), { k: 'binær desimal gigabyte' }),
          row('Nettrate', 'Byte per sekund eller bit per sekund.', seg([['byte', 'MB/s'], ['bit', 'Mbit/s']], S().netUnit, 'netUnit', 'Nettrate'), { k: 'nett hastighet' }),
        ),
        group(
          'Visning',
          {},
          row('Desimaler', 'Temperatur, effekt og prosent i tabeller og sidefeltet. Automatisk velger etter stedet.', seg([['auto', 'Automatisk'], ['0', '0'], ['1', '1']], S().decimals, 'decimals', 'Desimaler'), { k: 'desimal komma' }),
          row('Klokkeslett', `Min/maks på Sensorer, profiler og grafer. Nå: ${F.time(new Date(), { seconds: false })}.`, seg([['system', 'System'], ['24', '24 t'], ['12', '12 t']], S().clock, 'clock', 'Klokkeslett'), { k: 'tid klokke am pm' }),
        ),
      ];
    },

    sidefelt() {
      const items = S().statusItems || [];
      const max = statusMax(app);
      // Sidene som avkrysningsbokser i et rutenett.
      const hidden = new Set(S().hiddenPages);
      const pageChecks = el('div', { class: 'check-list' });
      PAGES.forEach(([id, name], i) => {
        const cb = el('input', { type: 'checkbox', class: 'check' });
        cb.checked = !hidden.has(id);
        cb.addEventListener('change', () => {
          const next = new Set(S().hiddenPages);
          if (cb.checked) next.delete(id);
          else next.add(id);
          if (next.size >= PAGES.length) {
            cb.checked = true;
            toast('Minst én side må vises', { error: true });
            return;
          }
          save({ hiddenPages: PAGES.map((p) => p[0]).filter((x) => next.has(x)) });
        });
        pageChecks.append(el('label', {}, cb, el('span', { text: name }), i >= 5 ? el('span', { class: 'muted', text: 'simulert' }) : null));
      });
      // v0.0.15: bredden kan også velges her. Den gjelder i nettleseren straks
      // (LOCAL_FIRST i app.js), også mot en server som ikke kjenner valget.
      const narrow = window.matchMedia('(max-width: 860px)').matches;
      const width = seg([['full', 'Full'], ['rail', 'Ikonlist']], S().sideCollapsed ? 'rail' : 'full', 'sideCollapsed', 'Bredde på sidefeltet', (v) => v === 'rail');
      return [
        statusGroup(items, max),
        group(
          'Visning',
          {},
          row('Bredde', narrow ? 'Vinduet er smalt, så sidefeltet er en ikonlist nå. Valget gjelder i bredere vinduer.' : 'Ikonlista viser bare ikoner og korte tall. Også Ctrl+B og knappen nederst i sidefeltet.', width, { k: 'ikonlist legg sammen utvid smal' }),
          switchRow('Sparklinjer', 'Liten graf for siste minutt ved hver verdi i statusblokken.', S().statusSparks, (on) => save({ statusSparks: on }), { k: 'statusblokk graf' }),
        ),
        group(
          'Sider',
          { desc: 'Sidene i sidefeltet. Skjulte sider kan fortsatt åpnes med hurtigtasten.', k: 'skjul vis navigasjon' },
          el('div', { class: 'row wrap check-row' }, pageChecks),
          switchRow('Vis hurtigtaster', 'Tallene 1–8 ved sidene. Ellers står de i verktøytipset og under Tastatursnarveier.', S().showKbd, (on) => save({ showKbd: on }), { k: 'taster tall snarvei' }),
        ),
      ];
    },

    grafer() {
      const set = new Set(S().charts);
      const checks = el('div', { class: 'check-list' });
      for (const id of CHART_ORDER) {
        const cb = el('input', { type: 'checkbox', class: 'check' });
        cb.checked = set.has(id);
        cb.addEventListener('change', () => {
          const next = new Set(S().charts);
          if (cb.checked) next.add(id);
          else next.delete(id);
          if (!next.size) {
            cb.checked = true;
            toast('Minst én graf må vises', { error: true });
            return;
          }
          save({ charts: CHART_ORDER.filter((x) => next.has(x)) });
        });
        checks.append(el('label', {}, cb, el('span', { text: CHART_DEFS[id].title })));
      }
      return [
        group(
          'Grafer',
          { note: 'Serveren tar vare på 48 timer med historikk mens den kjører og en fane er åpen, også over en omstart.' },
          row('Tidsvindu på Grafer', 'Hvor langt tilbake siden Grafer viser. Oversikt, Kontroller og Sensorer har eget valg ved grafene.', seg(WINDOWS, S().chartWindow, 'chartWindow', 'Tidsvindu', Number), { wrap: true, k: 'historikk minutter timer' }),
          switchRow('Markør på alle grafene', 'Når du holder over én graf, vises tidspunktet på de andre også. Av: bare på grafen du holder over.', S().chartSync !== false, (on) => save({ chartSync: on }), { k: 'synkron tidspunkt' }),
          row('Y-akse', 'Tilpass følger dataene i vinduet. Prosent har alltid fast skala 0\u2060–\u2060100.', seg([['auto', 'Tilpass'], ['fixed', 'Fast']], S().chartScale, 'chartScale', 'Y-akse'), { k: 'skala akse' }),
          switchRow('Fylt flate', 'Svak farge under linjen.', S().chartFill, (on) => save({ chartFill: on }), { k: 'fyll' }),
        ),
        group('Grafer som vises', { desc: 'På siden Grafer. Kan også velges i toppfeltet der.', k: 'velg skjul' }, el('div', { class: 'row wrap check-row' }, checks)),
        group(
          'Sensorer',
          {},
          switchRow('Sparklinjer', 'Liten graf for siste 60 sekunder ved hver sensor.', S().sensorSparks, (on) => save({ sensorSparks: on }), { k: 'graf' }),
          switchRow('Husk min og maks mellom økter', 'Nullstilles bare når du trykker ↺ på Sensorer. Snitt gjelder alltid denne økten.', S().sensorKeepMinMax, (on) => save({ sensorKeepMinMax: on }), { k: 'minimum maksimum' }),
        ),
        thresholdGroup(),
      ];
    },

    data() {
      const dir = app.state.dataDir || '–';
      const copyBtn = el('button', { type: 'button', class: 'btn flat icon-only sm', 'data-tip': 'Kopier stien', 'aria-label': 'Kopier stien' }, icon('copy'));
      copyBtn.addEventListener('click', async () => {
        const ok = await copyText(dir);
        if (ok) confirmDone(copyBtn, { tip: 'Stien er kopiert' });
        else toast('Kunne ikke kopiere', { body: 'Utklippstavlen er ikke tilgjengelig.', error: true });
      });
      const exp = el('button', { type: 'button', class: 'btn' }, icon('download'), 'Eksporter …');
      exp.addEventListener('click', exportSettings);
      const imp = el('button', { type: 'button', class: 'btn' }, icon('upload'), 'Importer …');
      imp.addEventListener('click', () => $('#import-file').click());
      const dismissed = S().dismissed || [];
      const showBars = el('button', { type: 'button', class: 'btn', disabled: !dismissed.length || null, 'data-tip': dismissed.length ? null : 'Ingen meldinger er skjult' }, 'Vis igjen');
      showBars.addEventListener('click', () => save({ dismissed: [] }).then(() => render()));
      // Ingen ellipse: knappen tilbakestiller straks (med Angre), den åpner ingen dialog.
      const reset = el('button', { type: 'button', class: 'btn destructive' }, 'Tilbakestill');
      reset.addEventListener('click', resetAll);
      return [
        // Én rad: etikett, stien med ellipse i midten (siste mapper synes alltid) og kopier-knappen.
        group('Lagring', { note: 'Innstillinger, profiler, lys, min/maks og historikk for grafene lagres her.' }, pathRow(dir, copyBtn)),
        group(
          'Innstillinger som fil',
          {},
          row('Eksporter', 'Lagrer app-innstillingene som JSON. Profiler og lys er ikke med.', exp, { k: 'lagre fil json sikkerhetskopi' }),
          row('Importer', 'Leser en eksportert fil. Er noe ugyldig, endres ingenting.', imp, { k: 'åpne fil json' }),
        ),
        group(
          'Tilbakestilling',
          {},
          row('Skjulte meldinger', dismissed.length ? `${dismissed.length} infomelding${dismissed.length === 1 ? '' : 'er'} er skjult.` : 'Ingen infomeldinger er skjult.', showBars, { k: 'infolinje melding' }),
          row('Tilbakestill alle innstillinger', 'Alt i Innstillinger går tilbake til standard. Profiler, lys og simulerte verdier blir. Kan angres i noen sekunder.', reset, { k: 'standard nullstill fabrikk' }),
        ),
      ];
    },

    om() {
      const A = app.state.app;
      const keys = el('button', { type: 'button', class: 'btn' }, icon('circle-help'), 'Tastatursnarveier …');
      keys.addEventListener('click', () => {
        app.showKeys?.();
      });
      const dbg = el('dl', { class: 'kv about-debug' }, ...app.aboutRows());
      const ver = el('span', { class: 'tag quiet', id: 'about-version', text: `${A.channel} v${A.version} · bygg ${A.build}` });
      // Navn, versjon og en setning i ett panel, så Om ikke blir en sentrert plakat i en bred spalte.
      const hero = el(
        'section',
        { class: 'group set-group about-hero', 'aria-label': `Om ${app.name}`, 'data-whole': '', 'data-k': 'versjon bygg om' },
        el(
          'div',
          { class: 'about-card' },
          el('img', { src: 'icons/agentic-rigg.svg', alt: '', width: '48', height: '48' }),
          el('div', { class: 'about-text' }, el('h3', { class: 'about-name', text: app.name }), el('div', { class: 'about-ver' }, ver), el('p', { text: 'Overvåking av skjermkort og maskin for Linux. Kontrollene er simulert, og ingenting sendes til kortet.' })),
        ),
      );
      aboutEgg(ver, dbg); // Påskeegg (easter.js): sju klikk på versjonen
      return [
        hero,
        group('Hjelp', {}, row('Tastatursnarveier', 'Alle hurtigtastene samlet. Også Ctrl+? og ?-knappen i sidefeltet.', keys, { k: 'taster hurtigtast snarvei' })),
        group('Feilsøking', { whole: true, note: 'Ikoner: Lucide (ISC). Lisensene står i docs/LISENSER.md.', cls: 'about-list', k: 'debug versjon lisens' }, dbg),
      ];
    },
  };

  /** Varselfarger: én rad per verdi, med gul og rød grense i hver sin kolonne. */
  function thresholdGroup() {
    const t = S().thresholds;
    const tf = (key, min, max, unit, temp, label) =>
      numField({
        min: temp ? Math.round(F.tempOut(min)) : min,
        max: temp ? Math.round(F.tempOut(max)) : max,
        step: 1,
        value: temp ? Math.round(F.tempOut(t[key])) : t[key],
        unit,
        size: 'sm',
        label,
        onCommit: (v) => save({ thresholds: { ...S().thresholds, [key]: temp ? Math.round(F.tempIn(v)) : v } }).then(() => render()),
      }).root;
    const C = F.tempLabel();
    const thr = (title, a, b) => el('div', { class: 'row thr-row' }, el('div', { class: 'text' }, el('span', { class: 'title', text: title })), el('div', { class: 'ctrl' }, a, b));
    return group(
      'Varselfarger',
      { desc: 'Verdien blir gul eller rød over grensen: i sidefeltet, på Oversikt, i Nå-kolonnen på Sensorer og i stolpene.', whole: true, k: 'grense terskel advarsel gul rød' },
      el('div', { class: 'row thr-head', 'aria-hidden': 'true' }, el('div', { class: 'text' }), el('div', { class: 'ctrl' }, el('span', { class: 'thr-h' }, el('i', { class: 'thr-dot warn' }), 'Gul'), el('span', { class: 'thr-h' }, el('i', { class: 'thr-dot danger' }), 'Rød'))),
      thr('GPU-temperatur', tf('gpuTempWarn', 40, 100, C, true, 'GPU-temperatur, gul grense'), tf('gpuTempCrit', 40, 110, C, true, 'GPU-temperatur, rød grense')),
      thr('CPU-temperatur', tf('cpuTempWarn', 40, 100, C, true, 'CPU-temperatur, gul grense'), tf('cpuTempCrit', 40, 110, C, true, 'CPU-temperatur, rød grense')),
      thr('Last (GPU og CPU)', tf('loadWarn', 50, 100, '%', false, 'Last, gul grense'), el('span', { class: 'thr-none', 'data-tip': 'Last har bare gul grense', text: '–' })),
    );
  }

  // ---- Status nederst: ordnet liste med dra, Alt+↑/↓, × og «Legg til» ----
  let focusAfter = null; // { id, part } – fokus etter ny tegning (tastatur)

  function statusGroup(items, max) {
    const list = el('div', { class: 'list order-list', role: 'list', 'aria-label': 'Verdier i statusblokken, i rekkefølge' });
    if (!items.length) {
      list.append(el('div', { class: 'row order-empty' }, icon('pin'), el('div', { class: 'text' }, el('span', { class: 'title', text: 'Statusblokken er tom' }), el('span', { class: 'subtitle', text: 'Legg til verdier her, eller fest sensorer med nålen på Sensorer.' }))));
    }
    items.forEach((id, i) => {
      const info = describe(app, id);
      const grip = el('span', { class: 'grip', 'aria-hidden': 'true' }, icon('grip-vertical'));
      const del = el('button', { type: 'button', class: 'btn flat icon-only sm', 'data-tip': 'Fjern fra statusblokken', 'aria-label': `Fjern ${info.long}` }, icon('x'));
      del.addEventListener('click', () => {
        const next = S().statusItems.filter((x) => x !== id);
        const n = next[Math.min(i, next.length - 1)];
        setItems(next, n ? { id: n, part: 'row' } : { id: null, part: 'add' });
      });
      const row = el(
        'div',
        { class: 'row order-row', role: 'listitem', tabindex: '0', 'data-id': id, 'aria-label': `${i + 1} av ${items.length}: ${info.long}. Alt+pil opp eller ned flytter.`, 'data-missing': info.missing ? '' : null },
        grip,
        el('div', { class: 'text' }, el('span', { class: 'title', text: info.long }), el('span', { class: 'subtitle', text: info.missing ? 'Ikke tilgjengelig nå' : `${info.src} · i sidefeltet: ${info.label}` })),
        el('span', { class: 'pos', text: String(i + 1) }),
        del,
      );
      row.addEventListener('keydown', (e) => {
        if (e.target !== row) return;
        if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
          e.preventDefault();
          move(id, e.key === 'ArrowUp' ? -1 : 1);
        } else if (!e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
          e.preventDefault();
          (e.key === 'ArrowUp' ? row.previousElementSibling : row.nextElementSibling)?.focus();
        } else if (e.key === 'Delete' || e.key === 'Backspace') {
          e.preventDefault();
          del.click();
        }
      });
      grip.addEventListener('pointerdown', (e) => startDrag(e, row, list));
      list.append(row);
    });
    // «+ Legg til …» nederst i lista, med søk i en popover.
    const full = items.length >= max;
    const add = el('button', { type: 'button', class: 'btn flat order-add', 'aria-haspopup': 'dialog', 'aria-expanded': 'false', disabled: full || null, 'data-tip': full ? `Statusblokken er full (${max} av ${max}). Fjern en verdi først.` : null }, icon('plus'), 'Legg til verdi …');
    add.addEventListener('click', () => openPicker(add));
    list.append(el('div', { class: 'row order-foot' }, add));
    const hid = nid('sg');
    return el(
      'section',
      { class: 'group set-group status-group', 'aria-labelledby': hid, 'data-whole': '', 'data-k': 'status nederst rekkefølge fest sensor verdier' },
      el('div', { class: 'group-head' }, el('h3', { class: 'group-title', id: hid, text: 'Statusblokken' }), el('div', { class: 'aside' }, el('span', { class: 'count', text: `${items.length} av ${max}` }))),
      el('p', { class: 'set-desc', text: 'Verdiene nederst i sidefeltet, i denne rekkefølgen. Dra i håndtaket, eller velg en rad og bruk Alt+↑/↓. Fest flere fra Sensorer med nålen.' }),
      list,
    );
  }

  function move(id, dir) {
    const cur = [...S().statusItems];
    const i = cur.indexOf(id);
    const j = i + dir;
    if (j < 0 || j >= cur.length) return;
    cur.splice(i, 1);
    cur.splice(j, 0, id);
    setItems(cur, { id, part: 'row' });
  }

  /** Lagrer lista og tegner straks (endringen tas i bruk før serveren svarer). */
  function setItems(next, focus) {
    focusAfter = focus;
    const p = save({ statusItems: next });
    render();
    // Avvist av serveren: innstillingen er rullet tilbake, tegn den på nytt.
    p.then((ok) => !ok && render());
  }

  /** Dra i håndtaket: raden følger pekeren (transform), naboene glir unna. */
  function startDrag(e, row, list) {
    if (e.button !== 0) return;
    e.preventDefault();
    const rowsEls = [...list.querySelectorAll('.order-row')];
    const from = rowsEls.indexOf(row);
    const rects = rowsEls.map((r) => r.getBoundingClientRect());
    const h = rects[from].height;
    const y0 = e.clientY;
    let to = from;
    row.setPointerCapture(e.pointerId);
    row.dataset.dragging = '';
    list.dataset.sorting = '';
    const onMove = (ev) => {
      const dy = Math.max(rects[0].top - rects[from].top, Math.min(rects[rects.length - 1].top - rects[from].top, ev.clientY - y0));
      row.style.transform = `translateY(${dy}px)`;
      const mid = rects[from].top + h / 2 + dy;
      to = rects.findIndex((r) => mid <= r.top + r.height / 2);
      if (to < 0) to = rects.length - 1;
      rowsEls.forEach((r, k) => {
        if (r === row) return;
        const shift = from < to && k > from && k <= to ? -h : from > to && k < from && k >= to ? h : 0;
        r.style.transform = shift ? `translateY(${shift}px)` : '';
      });
    };
    const onUp = () => {
      row.removeEventListener('pointermove', onMove);
      row.removeEventListener('pointerup', onUp);
      row.removeEventListener('pointercancel', onUp);
      delete row.dataset.dragging;
      delete list.dataset.sorting;
      if (to !== from) {
        const cur = [...S().statusItems];
        const [x] = cur.splice(from, 1);
        cur.splice(to, 0, x);
        setItems(cur, { id: x, part: 'none' });
      } else for (const r of rowsEls) r.style.transform = '';
    };
    row.addEventListener('pointermove', onMove);
    row.addEventListener('pointerup', onUp);
    row.addEventListener('pointercancel', onUp);
  }

  /** Popover med søk: faste verdier først, så alle sensorene fra Sensorer. */
  let picker = null;
  function openPicker(anchor) {
    picker?.remove();
    const q = el('input', { type: 'search', placeholder: 'Søk etter sensor', 'aria-label': 'Søk etter sensor', autocomplete: 'off' });
    const results = el('div', { class: 'pick-list', role: 'listbox', 'aria-label': 'Sensorer' });
    picker = el('div', { class: 'popover add-picker', role: 'dialog', 'aria-label': 'Legg til verdi i statusblokken' }, el('label', { class: 'field search-field' }, icon('search'), q), results);
    const candidates = () => {
      const cur = new Set(S().statusItems);
      // Verdien nå står til høyre, så det er lett å kjenne igjen riktig sensor.
      const fixedNow = (d) => {
        const src = d.src === 'gpu' ? app.feed.latest : app.feed.sysLatest;
        const v = src ? d.get(src) : null;
        return v == null ? '–' : d.fmt(v);
      };
      const fixed = STATUS_ORDER.map((id) => ({ id, name: STATUS_DEFS[id].long, now: fixedNow(STATUS_DEFS[id]), g: 'Faste verdier', key: STATUS_DEFS[id].label }));
      const sensors = (app.sensors?.list() || []).map((r) => ({ id: pinKey(r.id), name: r.label, now: r.now == null ? '–' : r.rate ? F.rate(r.now, r.unit) : r.sideFmt(r.now), g: r.groupTitle, key: `${r.short} ${r.raw || ''}` }));
      const seen = new Set();
      return [...fixed, ...sensors].filter((c) => {
        if (seen.has(c.id)) return false;
        seen.add(c.id);
        return true;
      }).map((c) => ({ ...c, on: cur.has(c.id) }));
    };
    const draw = () => {
      const f = q.value.trim().toLowerCase();
      const list = candidates().filter((c) => !f || `${c.name} ${c.key} ${c.g}`.toLowerCase().includes(f));
      results.replaceChildren();
      let g = null;
      for (const c of list.slice(0, 80)) {
        if (c.g !== g) {
          g = c.g;
          results.append(el('div', { class: 'menu-label', text: g }));
        }
        const b = el('button', { type: 'button', class: 'menu-item pick', role: 'option', 'aria-selected': String(c.on), disabled: c.on || null, 'data-tip': c.on ? 'Står allerede i statusblokken' : null }, el('span', { class: 'pn', text: c.name }), el('span', { class: 'ps', text: c.on ? 'Lagt til' : c.now }));
        b.addEventListener('click', () => {
          const cur = S().statusItems;
          if (cur.length >= statusMax(app)) return;
          closePopover();
          setItems([...cur, c.id], { id: c.id, part: 'row' });
        });
        results.append(b);
      }
      if (!list.length) results.append(el('div', { class: 'pick-empty' }, icon('search'), `Ingen sensor heter «${q.value.trim()}»`));
    };
    q.addEventListener('input', draw);
    q.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        results.querySelector('.pick:not(:disabled)')?.focus();
      }
    });
    results.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      e.preventDefault();
      const all = [...results.querySelectorAll('.pick:not(:disabled)')];
      const i = all.indexOf(document.activeElement);
      if (e.key === 'ArrowUp' && i <= 0) return q.focus();
      all[Math.max(0, Math.min(all.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))]?.focus();
    });
    draw();
    togglePopover(anchor, picker);
    q.focus({ preventScroll: true });
  }

  async function exportSettings() {
    const body = { format: 'agentic-rigg-settings', formatVersion: 1, app: app.state.app.version, exportedAt: new Date().toISOString(), settings: S() };
    const blob = new Blob([JSON.stringify(body, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = el('a', { href: url, download: `agentic-rigg-innstillinger-${new Date().toISOString().slice(0, 10)}.json` });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  $('#import-file').addEventListener('change', async (e) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (file.size > 64 * 1024) return toast('Fila er for stor', { body: 'En innstillingsfil er bare noen få kB.', error: true });
    let data;
    try {
      data = JSON.parse(await file.text());
    } catch {
      return toast('Kunne ikke lese fila', { body: 'Den er ikke gyldig JSON.', error: true });
    }
    const settings = data?.format === 'agentic-rigg-settings' ? data.settings : data?.settings || data;
    const before = S();
    try {
      const r = await app.api.importSettings(settings);
      app.applySettings(r.settings, null, { replace: true });
      app.notifyTabs?.();
      render();
      toast('Innstillinger importert', { action: { label: 'Angre', run: () => restore(before) } });
    } catch (err) {
      toast('Importen ble avvist', { body: err.message, error: true, ms: 7000 });
    }
  });

  async function restore(before) {
    try {
      const r = await app.api.importSettings(before);
      app.applySettings(r.settings, null, { replace: true });
      app.notifyTabs?.();
      render();
    } catch (err) {
      toast('Kunne ikke angre', { body: err.message, error: true });
    }
  }

  async function resetAll() {
    const before = S();
    try {
      const r = await app.api.resetSettings();
      app.applySettings(r.settings, null, { replace: true });
      app.notifyTabs?.();
      render();
      toast('Innstillingene er tilbakestilt', { action: { label: 'Angre', run: () => restore(before) } });
    } catch (err) {
      toast('Kunne ikke tilbakestille', { body: err.message, error: true });
    }
  }

  // ---- Søk i alle delene ----
  //
  // Hver del bygges som vanlig, og radene som ikke passer, tas bort. Et panel
  // med whole (lister og rutenett) vises helt eller ikke. Teksten det søkes i,
  // er delens navn, panelets tittel og forklaring, radens tekst og søkeordene i
  // data-k. Alle ordene i søket må passe. Kontrollene virker i treffene.
  function results(terms) {
    const out = [];
    const hits = {};
    for (const [id, name] of SECTIONS) {
      hits[id] = 0;
      for (const node of build[id]()) {
        const head = node.querySelector(':scope > .group-head');
        const base = `${name} ${head?.textContent || ''} ${node.dataset.k || ''} ${node.querySelector(':scope > .set-desc')?.textContent || ''}`;
        const fits = (txt) => {
          const h = norm(`${base} ${txt}`);
          return terms.every((t) => h.includes(t));
        };
        const rows = node.hasAttribute('data-whole') ? [] : [...node.querySelectorAll(':scope > .list > .row')];
        if (!rows.length) {
          if (!fits(node.textContent)) continue;
          hits[id]++;
        } else {
          let n = 0;
          for (const r of rows) {
            if (fits(rowText(r))) n++;
            else r.remove();
          }
          if (!n) continue;
          hits[id] += n;
        }
        // Hvor treffet står: «Utseende ›» foran panelets tittel, som en knapp dit.
        const crumb = el('button', { type: 'button', class: 'set-crumb', 'data-tip': `Gå til ${name}` }, name);
        crumb.addEventListener('click', () => selectSection(id, { group: head?.querySelector('.group-title')?.textContent || null }));
        // Om-panelet har ingen overskriftsstripe; i treffene får det en med bare delen.
        if (head) head.prepend(crumb, el('span', { class: 'set-crumb-sep', 'aria-hidden': 'true', text: '›' }));
        else node.prepend(el('div', { class: 'group-head' }, crumb));
        out.push(node);
      }
    }
    return { out, hits };
  }

  /**
   * Teksten i en rad det søkes i: etikett, forklaring, søkeord og det som står
   * på knappene (°F, Mørk, Ikonlist). Valgene i en nedtrekksliste er ikke med,
   * ellers ville «graf» funnet Startside (der Grafer er et valg).
   */
  const rowText = (r) => [r.querySelector(':scope > .text')?.textContent, r.dataset.k, ...[...r.querySelectorAll('.ctrl button, .ctrl label, .check-list label')].map((b) => b.textContent)].filter(Boolean).join(' ');

  /** Markerer ordene fra søket i titlene (bare tekst, ingen HTML fra søket). */
  function markHits(root, terms) {
    const re = new RegExp(terms.map(escRe).join('|'), 'gi');
    for (const n of root.querySelectorAll('.row .title, .group-title, .check-list label > span:not(.muted), .pal-name, .sw-name')) {
      for (const t of [...n.childNodes]) {
        if (t.nodeType !== Node.TEXT_NODE) continue;
        const s = t.nodeValue;
        const ms = [...s.matchAll(re)];
        if (!ms.length) continue;
        const frag = document.createDocumentFragment();
        let i = 0;
        for (const m of ms) {
          frag.append(s.slice(i, m.index), el('mark', { class: 'hit', text: m[0] }));
          i = m.index + m[0].length;
        }
        frag.append(s.slice(i));
        t.replaceWith(frag);
      }
    }
  }

  function renderResults() {
    const terms = norm(query).trim().split(' ').filter(Boolean);
    const { out, hits } = results(terms);
    const total = Object.values(hits).reduce((a, b) => a + b, 0);
    for (const b of nav.querySelectorAll('[role=tab]')) {
      const n = hits[b.dataset.s] || 0;
      b.dataset.hits = String(n);
      b.querySelector('.set-n').textContent = n ? String(n) : '';
    }
    heading.textContent = 'Søk';
    leadEl.textContent = `«${query}» i alle delene`;
    countEl.textContent = total ? `${total} treff` : 'Ingen treff';
    if (!out.length) {
      body.replaceChildren(
        el(
          'div',
          { class: 'set-empty' },
          icon('search'),
          el('p', {}, el('b', { text: `Ingen innstillinger passer «${query}».` }), el('span', { text: 'Prøv et ord fra navnet eller forklaringen, for eksempel tema, enhet eller graf.' })),
        ),
      );
      return;
    }
    const panel = el('div', { class: 'set-section set-results', id: 'set-panel', role: 'region', 'aria-label': 'Treff', 'data-cols': out.length >= 4 ? '3' : '2' }, ...out);
    markHits(panel, terms);
    body.replaceChildren(panel);
  }

  function setQuery(v) {
    const next = v.trim();
    if (next === query) return;
    query = next;
    render();
    body.scrollTop = 0;
  }
  function clearSearch() {
    search.value = '';
    query = '';
  }

  // ---- Navigasjonen og tegning ----
  for (const [id, name, ic] of SECTIONS) {
    const b = el(
      'button',
      { type: 'button', role: 'tab', id: `set-tab-${id}`, 'data-s': id, 'aria-controls': 'set-panel', 'data-tip-side': 'bottom' },
      icon(ic),
      el('span', { class: 'nm', text: name }),
      el('span', { class: 'set-n', 'aria-hidden': 'true' }),
    );
    b.addEventListener('click', () => selectSection(id));
    nav.append(b);
  }
  /**
   * Smalt vindu: navigasjonen viser bare ikoner, så navnet står i verktøytipset,
   * og søkefeltet er kort, så teksten i det er bare «Søk».
   */
  const syncTips = () => {
    for (const b of nav.querySelectorAll('[role=tab]')) setTip(b, mqTop.matches ? SECTIONS.find((s) => s[0] === b.dataset.s)[1] : null);
    search.placeholder = mqTop.matches ? 'Søk' : 'Søk i innstillinger';
  };
  syncTips();
  mqTop.addEventListener('change', syncTips);

  nav.addEventListener('keydown', (e) => {
    if (!['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const ids = SECTIONS.map((s) => s[0]);
    const i = ids.indexOf(section);
    const next = e.key === 'Home' ? ids[0] : e.key === 'End' ? ids[ids.length - 1] : ids[(i + (e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : -1) + ids.length) % ids.length];
    selectSection(next);
    nav.querySelector(`[data-s="${next}"]`).focus();
    e.preventDefault();
  });

  /**
   * Åpner en del. Fra et søketreff (group): søket tømmes, panelet rulles fram,
   * lyser kort opp og får fokus på første kontroll.
   */
  function selectSection(id, { group: title = null } = {}) {
    const fromSearch = !!query;
    if (fromSearch) clearSearch();
    const same = id === section;
    section = id;
    local.set('ag-settings-section', id);
    render();
    if (!same || fromSearch) body.scrollTop = 0;
    if (!title) return;
    const g = [...body.querySelectorAll('.set-group')].find((n) => n.querySelector(':scope > .group-head .group-title')?.textContent === title);
    if (!g) return;
    g.scrollIntoView({ block: 'nearest' });
    animate(g, [{ boxShadow: '0 0 0 2px var(--accent)' }, { boxShadow: '0 0 0 2px transparent' }], { duration: dur(4) * 4, easing: EASE.out }, [{ boxShadow: '0 0 0 2px var(--accent)' }, { boxShadow: '0 0 0 2px transparent' }]);
    g.querySelector('.ctrl :is(button, input, select, [tabindex="0"]):not(:disabled, [tabindex="-1"]), .order-row, .check-list input')?.focus({ preventScroll: true });
  }

  function render() {
    // Under søk er ingen del valgt (treffene kommer fra alle), men den siste
    // er fortsatt Tab-stoppet i navigasjonen.
    for (const b of nav.querySelectorAll('[role=tab]')) {
      const on = b.dataset.s === section;
      b.setAttribute('aria-selected', String(on && !query));
      b.tabIndex = on ? 0 : -1;
    }
    if (query) {
      nav.dataset.searching = '';
      renderResults();
      return;
    }
    delete nav.dataset.searching;
    for (const b of nav.querySelectorAll('[role=tab]')) {
      delete b.dataset.hits;
      b.querySelector('.set-n').textContent = '';
    }
    const [, name, , lead] = SECTIONS.find((s) => s[0] === section);
    heading.textContent = name;
    leadEl.textContent = lead;
    countEl.textContent = '';
    const nodes = build[section]();
    // Tre spalter bare når delen har fire paneler eller flere (styles.css).
    body.replaceChildren(el('div', { class: 'set-section', role: 'tabpanel', id: 'set-panel', 'aria-labelledby': `set-tab-${section}`, 'data-s': section, 'data-cols': nodes.length >= 4 ? '3' : '2' }, ...nodes));
    if (focusAfter) {
      const f = focusAfter;
      focusAfter = null;
      const row = f.id ? body.querySelector(`.order-row[data-id="${CSS.escape(f.id)}"]`) : null;
      if (f.part === 'row' && row) row.focus({ preventScroll: true });
      else if (f.part === 'add') body.querySelector('.order-add')?.focus({ preventScroll: true });
      // Raden som ble flyttet, toner kort inn på ny plass.
      if (row) animate(row, [{ backgroundColor: 'var(--accent-soft)' }, { backgroundColor: 'transparent' }], { duration: dur(4) * 3, easing: EASE.out });
    }
  }

  // ---- Søkefeltet, tilbake og taster ----
  // Ett søk per bilde, selv om tastene kommer fortere (alle delene bygges hver gang).
  let pending = 0;
  search.addEventListener('input', () => {
    cancelAnimationFrame(pending);
    pending = requestAnimationFrame(() => setQuery(search.value));
  });
  search.addEventListener('keydown', (e) => {
    // Enter eller pil ned: til første kontroll i treffene.
    if ((e.key === 'Enter' || e.key === 'ArrowDown') && query) {
      const first = body.querySelector('.ctrl :is(button, input, select, [tabindex="0"]):not(:disabled, [tabindex="-1"]), .order-row, .check-list input');
      if (first) {
        e.preventDefault();
        first.focus();
      }
    }
  });
  backBtn.addEventListener('click', () => app.closeSettings?.());
  // Popoveren («Legg til verdi») står fast i vinduet; den lukkes når innholdet rulles.
  body.addEventListener('scroll', () => closePopover(), { passive: true });

  /** Tilbakeknappen sier hvor den går: «Tilbake til Sensorer». */
  function renderBack() {
    const to = app.settingsReturn?.() || 'oversikt';
    const t = $(`#page-${to}`)?.dataset.title || 'forrige side';
    setTip(backBtn, `Tilbake til ${t}`);
    backBtn.setAttribute('aria-label', `Tilbake til ${t}`);
  }

  /** Åpner en del i Innstillinger (f.eks. fra statusblokken eller Sensorer). */
  app.openSettings = (id) => {
    if (!SECTIONS.some((x) => x[0] === id)) return;
    if (query) clearSearch();
    section = id;
    local.set('ag-settings-section', id);
    app.go('innstillinger');
    if (visible) {
      render();
      body.scrollTop = 0;
      // Står fokuset i navigasjonen, følger det delen som nå er valgt.
      if (nav.contains(document.activeElement)) nav.querySelector('[aria-selected="true"]')?.focus({ preventScroll: true });
    }
  };

  app.on('settings', () => {
    if (visible && !selfChange) render();
  });
  app.on('mode', () => visible && (query || section === 'generelt') && render());

  return {
    show(_q, { focus = false } = {}) {
      visible = true;
      render();
      body.scrollTop = 0;
      renderBack();
      // Fokus inn i Innstillinger: den valgte delen i navigasjonen (sidefeltet med
      // tannhjulet er skjult nå). Ikke ved første lasting.
      if (focus) nav.querySelector('[aria-selected="true"]')?.focus({ preventScroll: true });
    },
    hide() {
      visible = false;
      if (query) clearSearch();
    },
    /** Taster mens Innstillinger vises: Ctrl+F og / søker, Esc tømmer søket eller går tilbake. */
    onKey(e, typing) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
        search.focus();
        search.select();
        return true;
      }
      if (e.key === '/' && !mod && !e.altKey && !typing) {
        search.focus();
        search.select();
        return true;
      }
      if (e.key === 'Escape' && !e.defaultPrevented) {
        if (e.target === search && search.value) {
          clearSearch();
          render();
          return true;
        }
        app.closeSettings?.();
        return true;
      }
      return false;
    },
  };
}
