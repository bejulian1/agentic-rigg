// Oppstart og skall: navigasjon, toppfelt, enhetskort, statusblokk,
// sidefeltets fot (Innstillinger, Tastatursnarveier, legg sammen), dialoger,
// hurtigtaster og felles tilstand.
//
// Bevegelse (docs/bevegelse.md):
//   Sidebytte      route(): siden toner inn og løftes 4 px (Web Animations,
//                  bare ved full bevegelse, ikke ved første visning).
//                  Toppfeltets tittel byttes straks.
//   Temabytte      applySettings(): fargene kryssfades med View Transitions.
//   Statusblokk    renderStatus() – .stat-row[data-level] toner farge i CSS.
//                  Verdiene er faste (STATUS_DEFS) eller festet fra Sensorer
//                  (pins.js), i brukerens rekkefølge, høyst 8.
//   Frakoblet      .app[data-stale] demper innholdet; #conn-banner glir inn
//                  (CSS), og skjermlesere får én melding ved brudd og retur.
//   Tjenesten      #daemon-banner (Kontroller og Viftekurve) når bakgrunns-
//                  tjenesten ikke er tilkoblet; #confirm-bar etter Bruk
//                  (confirm.js). Én melding til skjermleser per overgang.
//   Sidefeltet     v0.0.10: data-side="full|rail" på <html> (applySide()).
//                  Ikonlist når brukeren har lagt det sammen (sideCollapsed)
//                  eller vinduet er under 860 px. Bryteren endrer bredden
//                  straks (ingen layout per bilde), og innholdet i sidefeltet
//                  toner inn (bare opasitet, følger data-motion).
//   Innstillinger  v0.0.15: data-view="settings" på <html> skjuler sidefeltet
//                  og toppfeltet, og Innstillinger fyller vinduet (route()).
//                  Siden toner inn og løftes 6 px; tilbake (knappen, Esc,
//                  Ctrl+, eller nettleserens tilbake) gir siden du kom fra,
//                  med rulling og fokus som før (closeSettings()).

import { api, beaconTabClosed, LiveFeed } from './api.js';
import { $, $$, el, icon, local, setIcon, setText, animate, dur, EASE, viewTransition, enableViewTransitions, wake } from './util.js';
import * as F from './fmt.js';
import { closePopover, initPopovers, initTooltips, menu, openDialog, setSeg, bindSeg, toast, togglePopover, setTip, announce } from './ui.js';
import { describe, moveItem } from './pins.js';
import { LEVEL_TEXT } from './meters.js';
import { onTheme } from './theme.js';
import { applyPrefs } from './prefs.js';
import { drawSpark, sparkline } from './sparkline.js';
import { pcieText, shortGpuName } from './gpufmt.js';
import { initOverview } from './pages/overview.js';
import { initSensors } from './pages/sensors.js';
import { initMonitor } from './pages/monitor.js';
import { initGpu } from './pages/gpu.js';
import { initSystem } from './pages/system.js';
import { initControls } from './pages/controls.js';
import { initFanCurve } from './pages/fancurve-page.js';
import { initLights } from './pages/lights.js';
import { initSettings } from './pages/settings.js';
import { initConfirm } from './confirm.js';
import { FORM_KEYS, sameCurve } from './pages/controls.js';
import { initEaster } from './easter.js';

// Valg som gjelder denne nettleseren først og serveren etterpå (v0.0.14).
// sideCollapsed: sidefeltet skal alltid kunne legges sammen og utvides med en
// gang, også mot en server som ikke kjenner innstillingen (eldre enn v0.0.10),
// avviser den eller ikke svarer. Verdien som ikke er bekreftet av serveren,
// står i unsynced; uten den og uten verdi fra serveren brukes utdraget i
// localStorage (det samme som theme-boot.js leser før første tegning).
const LOCAL_FIRST = ['sideCollapsed'];
const unsynced = new Map();
function withLocal(s) {
  if (!s) return s;
  let out = s;
  for (const k of LOCAL_FIRST) {
    const v = unsynced.has(k) ? unsynced.get(k) : typeof s[k] === 'boolean' ? s[k] : !!local.get('ag-prefs', null)?.side;
    if (s[k] !== v) out = { ...out, [k]: v };
  }
  return out;
}
const bus = new EventTarget();
/** «Valgt ved oppstart» → «valgt ved oppstart», men «nvidia-smi …» står som det er. */
const lcFirst = (t) => (t ? t.charAt(0).toLowerCase() + t.slice(1) : '');
const feed = new LiveFeed();
let saveSeq = 0;
// Andre faner får beskjed når innstillingene endres her (og henter dem selv).
let tabs = null;
try {
  tabs = new BroadcastChannel('agentic-rigg');
} catch {}

import { PAGES } from './defs.js';

const SETTINGS = 'innstillinger';
const ROUTES = [...PAGES.map((p) => p[0]), SETTINGS];
const ALIAS = { overvaking: 'grafer' };

/** Felles tilstand som sidene deler. */
const app = {
  feed,
  api,
  toast,
  state: null, // fra /api/state
  draft: null, // skjemaet under Kontroller + viftekurven
  name: document.title,
  pages: {},
  current: null,
  offline: false, // true mens forbindelsen til serveren er brutt
  daemon: null, // bakgrunnstjenesten: { connected, pending, emergency, safety, … }
  clamped: {}, // siste klemming fra tjenesten, per felt
  confirm: null, // bekreftelseslinja (confirm.js)
  on(ev, fn) {
    bus.addEventListener(ev, (e) => fn(e.detail));
  },
  emit(ev, detail) {
    bus.dispatchEvent(new CustomEvent(ev, { detail }));
    wake();
  },
  get mode() {
    return feed.info ? feed.info.mode : 'sim';
  },
  get settings() {
    return this.state.settings;
  },
  resetDraft(keys) {
    const a = this.state.applied;
    if (!this.draft || !keys) this.draft = { ...a, fanCurve: a.fanCurve.map((p) => [...p]) };
    else for (const k of keys) this.draft[k] = k === 'fanCurve' ? a.fanCurve.map((p) => [...p]) : a[k];
    this.emit('draft');
  },
  setApplied(applied) {
    this.state.applied = applied;
    this.emit('applied', applied);
  },
  /**
   * Etter Bruk (Kontroller, Viftekurve, Angre, Bruk igjen): verdiene er i
   * bruk, men venter på Behold. focus: knappen som ble trykket, ble låst.
   */
  afterApply(res, { focus = false } = {}) {
    this.setApplied(res.applied);
    this.resetDraft();
    this.clamped = Object.fromEntries((res.clamped || []).map((c) => [c.key, c]));
    this.emit('clamped', this.clamped);
    if (res.pending) this.confirm.showPending(res.pending, { focus });
    else toast('Ingen endring', { body: 'Verdiene er allerede i bruk.' });
  },
  async applyValues(values, opts) {
    try {
      const patch = { fanCurve: values.fanCurve };
      for (const k of FORM_KEYS) if (values[k] !== undefined) patch[k] = values[k];
      this.afterApply(await api.apply(patch), opts);
    } catch (err) {
      toast('Kunne ikke bruke innstillingene', { body: err.message, error: true });
    }
  },
  /**
   * Lagrer app-innstillinger. Endringen tas i bruk straks (optimistisk) og
   * rulles tilbake hvis serveren avviser den. Ingen toast ved suksess.
   * Unntak (v0.0.14): valg for denne nettleseren (LOCAL_FIRST, nå bare
   * sidefeltets bredde) står lokalt til serveren har bekreftet dem, og rulles
   * aldri tilbake. Avviser serveren bare slike valg, sies det ingenting.
   */
  async saveSettings(patch) {
    const before = this.state.settings;
    const seq = ++saveSeq;
    const own = LOCAL_FIRST.filter((k) => k in patch);
    for (const k of own) unsynced.set(k, patch[k]);
    this.applySettings({ ...before, ...patch }, Object.keys(patch));
    try {
      const r = await api.settings(patch);
      // Bekreftet bare når serveren svarer med samme verdi. En gammel server
      // som ikke kjenner nøkkelen, sender den ikke tilbake; da står valget lokalt.
      for (const k of own) if (unsynced.get(k) === patch[k] && r?.settings?.[k] === patch[k]) unsynced.delete(k);
      // Bare svaret på siste lagring tas i bruk, så raske klikk som får svar i
      // feil rekkefølge ikke ruller tilbake et nyere valg. Serverens svar kan
      // også inneholde endringer fra en annen fane.
      if (seq === saveSeq) this.syncSettings(r?.settings);
      tabs?.postMessage('settings');
      return true;
    } catch (err) {
      if (seq === saveSeq) this.applySettings(before, Object.keys(patch));
      if (own.length < Object.keys(patch).length) toast('Kunne ikke lagre innstillingen', { body: err.message, error: true });
      return false;
    }
  },
  /** Tar i bruk innstillinger fra serveren hvis de skiller seg fra det som vises. */
  syncSettings(next) {
    if (!next) return;
    next = withLocal(next);
    const cur = this.state.settings;
    if (JSON.stringify(next) === JSON.stringify(cur)) return;
    this.applySettings(next);
  },
  /**
   * replace: innstillingene er byttet ut på serveren (tilbakestilt eller
   * importert), så lokale valg som ikke er bekreftet, glemmes.
   */
  applySettings(s, keys = null, { replace = false } = {}) {
    if (replace) unsynced.clear();
    s = withLocal(s);
    const prev = this.state.settings;
    this.state.settings = s;
    // Også nøkler som er borte i det nye (et svar uten nøkkelen er en endring).
    const all = new Set([...Object.keys(s), ...Object.keys(prev || {})]);
    const changed = keys || [...all].filter((k) => JSON.stringify(s[k]) !== JSON.stringify(prev?.[k]));
    // Nye farger kryssfades (aldri ved oppstart). Canvas-grafer leser fargene
    // på nytt inni overgangen, så de er med i det nye bildet.
    const colors = ['accent', 'palette', 'highContrast', 'theme'].some((k) => changed.includes(k)) && prev && Object.keys(prev).length;
    const paint = () => {
      applyPrefs(s);
      if (colors || changed.includes('fontScale')) this.emit('theme');
    };
    if (colors) viewTransition(paint);
    else paint();
    feed.setInterval(s.intervalMs);
    if (changed.includes('unit') || changed.includes('decimals') || changed.includes('memUnit') || changed.includes('netUnit') || changed.includes('clock')) this.emit('unit');
    this.emit('settings', changed);
  },
  go(page) {
    location.hash = `#/${page}`;
  },
};
window.__app = app; // til feilsøking i konsollen

// ---------------------------------------------------------------- Navigasjon

function pageFromHash() {
  const raw = (location.hash.replace(/^#\/?/, '') || '').split('?')[0];
  const name = ALIAS[raw] || raw;
  return ROUTES.includes(name) ? name : null;
}

function startPage() {
  const s = app.settings;
  let p = s.startPage === 'last' ? local.get('ag-last-page', 'oversikt') : s.startPage;
  if (!ROUTES.includes(p)) p = 'oversikt';
  return p;
}

let booted = false;

// ---------------------------------------------------------------- Innstillinger i hele vinduet
//
// v0.0.15: Innstillinger er en egen visning i det samme skallet, ikke et lag
// over appen. Ruten (#/innstillinger), omlasting og nettleserens tilbake virker
// som for alle sider; data-view="settings" på <html> skjuler sidefeltet og
// toppfeltet. Begrunnelsen står i docs/innstillinger-v0.0.15.md.
//
// back husker siden Innstillinger ble åpnet fra, hvor langt den var rullet og
// hva som hadde fokus. Oppføringen i nettleserens historikk merkes med siden
// (agFrom), så tilbake kan bruke history.back() når forrige oppføring er den
// siden, også etter omlasting.
let back = null;

/** Siden Innstillinger går tilbake til: den du kom fra, sist brukte side eller startsiden. */
function settingsReturn() {
  if (back?.page) return back.page;
  const st = history.state?.agFrom;
  if (ROUTES.includes(st) && st !== SETTINGS) return st;
  const last = local.get('ag-last-page', null);
  return ROUTES.includes(last) && last !== SETTINGS ? last : startPage() === SETTINGS ? 'oversikt' : startPage();
}
app.settingsReturn = settingsReturn;

/** Lukker Innstillinger (tilbakeknappen, Esc og Ctrl+,). */
function closeSettings() {
  if (app.current !== SETTINGS) return;
  const to = settingsReturn();
  const from = history.state?.agFrom;
  // Forrige oppføring i historikken er siden vi kom fra: gå dit, så
  // nettleserens fram-knapp åpner Innstillinger igjen.
  if (from && from === to) return void history.back();
  // Åpnet direkte (lenke eller omlasting uten historikk): bytt ut oppføringen.
  history.replaceState(null, '', `#/${to}`);
  route();
}
app.closeSettings = closeSettings;

function route() {
  const raw = (location.hash.replace(/^#\/?/, '') || '').split('?')[0];
  if (ALIAS[raw]) {
    history.replaceState(null, '', `#/${ALIAS[raw]}`);
  }
  const target = pageFromHash() || startPage();
  if (!pageFromHash()) history.replaceState(null, '', `#/${target}`);
  const query = new URLSearchParams(location.hash.split('?')[1] || '');
  if (target === app.current) {
    app.pages[target]?.query?.(query);
    return;
  }
  const from = app.current;
  // Inn i Innstillinger: husk siden, rullingen og fokuset (før noe skjules).
  if (target === SETTINGS && from && from !== SETTINGS) {
    back = { page: from, top: $('#content').scrollTop, focus: document.activeElement };
    // En ny oppføring (lenke, Ctrl+,) har ingen state; den merkes med siden vi kom fra.
    if (history.state == null) history.replaceState({ agFrom: from }, '', location.href);
  }
  const restore = from === SETTINGS && back?.page === target ? back : null;
  if (from === SETTINGS) back = null;
  closePopover();
  if (app.current) {
    app.pages[app.current]?.hide?.();
    $(`#page-${app.current}`).hidden = true;
  }
  app.current = target;
  const root = document.documentElement;
  if (target === SETTINGS) root.dataset.view = 'settings';
  else delete root.dataset.view;
  const page = $(`#page-${target}`);
  page.hidden = false;
  // Sidebytte: kort tone og lite løft. Ingen forsinkelse og ingen layout
  // (bare opasitet og transform). Ikke ved første side etter oppstart.
  // Inn og ut av Innstillinger byttes hele vinduet: litt lengre og rolig, og
  // med «Redusert» en kort toning i stedet for et hopp.
  const swap = from === SETTINGS || target === SETTINGS;
  if (booted && swap) animate(page, [{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], { duration: dur(4), easing: EASE.out }, [{ opacity: 0 }, { opacity: 1 }]);
  else if (booted) animate(page, [{ opacity: 0, transform: 'translateY(4px)' }, { opacity: 1, transform: 'none' }], { duration: dur(3), easing: EASE.out });
  for (const a of $$('.nav-item, .side-link')) {
    if (a.dataset.page === target) a.setAttribute('aria-current', 'page');
    else a.removeAttribute('aria-current');
  }
  // Tab inn i sidefeltet skal lande på siden du er på.
  if (!$('#nav').contains(document.activeElement)) rovingNav();
  for (const h of $$('.head-actions')) h.hidden = h.dataset.for !== target;
  $('#content').scrollTop = 0;
  app.pages[target]?.show?.(query, { focus: booted });
  app.renderDaemonBanner?.();
  $('#main-title').textContent = page.dataset.title || '';
  if (target !== SETTINGS) local.set('ag-last-page', target);
  renderTitle();
  if (from === SETTINGS) {
    // Ute av Innstillinger: sidefeltet er synlig igjen, og plassen i ikonlista
    // kan ha endret seg mens det var skjult (vindu, skrift, tetthet).
    queueFit();
    if (restore) {
      // Samme rulling og samme fokus som da Innstillinger ble åpnet. Fokuset går
      // til tannhjulet hvis elementet er borte eller fokuset sto på ingenting.
      const c = $('#content');
      c.scrollTop = restore.top;
      requestAnimationFrame(() => {
        if (app.current === target && Math.abs(c.scrollTop - restore.top) > 1) c.scrollTop = restore.top;
      });
      const f = restore.focus;
      const ok = f && f !== document.body && f.isConnected && f.checkVisibility?.();
      (ok ? f : $('#side-settings')).focus({ preventScroll: true });
    }
  }
}

function renderTitle() {
  const page = app.current ? $(`#page-${app.current}`) : null;
  const title = page?.dataset.title || '';
  const sim = feed.info && feed.info.mode !== 'live' ? ' (simulert)' : '';
  document.title = title ? `${title} · ${app.name}${sim}` : `${app.name}${sim}`;
}

function renderNav() {
  const hidden = new Set(app.settings.hiddenPages || []);
  const items = $$('.nav-item');
  items.forEach((a) => (a.hidden = hidden.has(a.dataset.page)));
  // Skjul seksjonsoverskrifter uten synlige sider.
  for (const sec of $$('.nav-sec')) {
    let n = sec.nextElementSibling;
    let any = false;
    while (n && !n.classList.contains('nav-sec')) {
      if (n.classList.contains('nav-item') && !n.hidden) any = true;
      n = n.nextElementSibling;
    }
    sec.hidden = !any;
  }
  rovingNav();
}

/** Sidefeltet er én tab-stopp; piltaster flytter mellom sidene. */
function rovingNav() {
  const items = $$('.nav-item').filter((a) => !a.hidden);
  const cur = items.find((a) => a.getAttribute('aria-current') === 'page') || items[0];
  for (const a of items) a.tabIndex = a === cur ? 0 : -1;
}

function bindNavKeys() {
  $('#nav').addEventListener('keydown', (e) => {
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
    const items = $$('.nav-item').filter((a) => !a.hidden);
    const i = items.indexOf(document.activeElement);
    let n = i;
    if (e.key === 'ArrowDown') n = Math.min(items.length - 1, i + 1);
    if (e.key === 'ArrowUp') n = Math.max(0, i - 1);
    if (e.key === 'Home') n = 0;
    if (e.key === 'End') n = items.length - 1;
    items.forEach((a, j) => (a.tabIndex = j === n ? 0 : -1));
    items[n]?.focus();
    e.preventDefault();
  });
  $('#nav').addEventListener('focusout', () => setTimeout(rovingNav, 0));
}

// ---------------------------------------------------------------- Sidefeltet

// Under 860 px er sidefeltet alltid en ikonlist, og knappen som legger det
// sammen er skjult (CSS). Valget (sideCollapsed) huskes og gjelder igjen når
// vinduet blir bredt.
const mqNarrow = window.matchMedia('(max-width: 860px)');
function applySide() {
  const root = document.documentElement;
  const want = app.settings?.sideCollapsed || mqNarrow.matches ? 'rail' : 'full';
  const changed = root.dataset.side !== want;
  root.dataset.side = want;
  const collapsed = !!app.settings?.sideCollapsed;
  const btn = $('#side-toggle');
  const label = collapsed ? 'Utvid sidefeltet' : 'Legg sammen sidefeltet';
  btn.setAttribute('aria-expanded', String(!collapsed));
  btn.setAttribute('aria-label', label);
  setTip(btn, label);
  setIcon(btn, collapsed ? 'panel-left-open' : 'panel-left-close');
  if (changed && booted) {
    // Bredden byttes straks; innholdet i sidefeltet toner inn, så byttet ikke rykker.
    for (const n of [$('.side-head'), $('#sidebar')]) animate(n, [{ opacity: 0.25 }, { opacity: 1 }], { duration: dur(3), easing: EASE.out }, [{ opacity: 0.25 }, { opacity: 1 }]);
    wake();
  }
  if (changed) fitStatus();
}
// Det du ser, er det som byttes: neste tilstand regnes fra bredden på skjermen,
// ikke fra en innstilling som serveren kan ha mistet. applySide() kalles også
// direkte, så bredden følger med selv om ingen hendelse skulle komme.
function toggleSide() {
  const next = document.documentElement.dataset.side !== 'rail';
  app.saveSettings({ sideCollapsed: next });
  applySide();
  announce(next ? 'Sidefeltet er lagt sammen.' : 'Sidefeltet er utvidet.');
}

// ---------------------------------------------------------------- Enhetskort

function renderDevice() {
  const info = feed.info;
  if (!info) return;
  const g = info.gpu || {};
  const live = info.mode === 'live';
  const name = shortGpuName(g.name) || 'Ukjent skjermkort';
  setText($('#device-name'), name);
  const realDriver = g.driver && !/^simulert$/i.test(g.driver) ? g.driver : null;
  const sub = `${live ? 'Live' : 'Simulert'}${realDriver ? ` · driver ${realDriver}` : ''}`;
  setText($('#device-sub'), sub);
  for (const d of [$('#device-dot'), $('#device-dot2')]) d.dataset.state = live ? 'live' : 'sim';
  // Navnet står i raden; resten (Live/Simulert, driver) i verktøytipset.
  setTip($('#device-card'), `${name} · ${sub}`);
  // Popover
  setText($('#dp-name'), g.name || name);
  setText($('#dp-sub'), live ? 'Leses med nvidia-smi' : `Simulert (${lcFirst(info.reason)})`);
  const rows = $('#dp-rows');
  const s = feed.latest || {};
  const q = info.query || {};
  const list = [
    ['Driver', realDriver],
    ['VBIOS', g.vbios],
    ['CUDA', q.cudaVersion && !/^simulert$/i.test(q.cudaVersion) ? q.cudaVersion : null],
    ['PCIe', pcieText(g.pcieWidthMax, g.pcieGenMax, s.pcieWidth, s.pcieGen)],
    ['Intervall', app.settings.intervalMs === 500 ? `0,5${F.NNBSP}s` : `${app.settings.intervalMs / 1000}${F.NNBSP}s`],
    ['Tjeneste', app.daemon ? (app.daemon.connected ? 'Tilkoblet (simulert)' : 'Ikke tilkoblet') : null],
  ];
  rows.replaceChildren(...list.map(([k, v]) => el('div', { class: 'kv-row' }, el('dt', { text: k }), el('dd', { class: v ? null : 'na', text: v || '–' }))));
  setSeg($('#dp-source'), info.mode);
  const liveBtn = $('#dp-source [data-v="live"]');
  liveBtn.disabled = !!app.state.forceSim;
  setTip(liveBtn, app.state.forceSim ? 'Live er låst: serveren ble startet i simulert modus' : null);
}

async function switchSource(v) {
  if (v === app.mode) return;
  setSeg($('#dp-source'), v);
  try {
    await api.mode(v);
  } catch (err) {
    setSeg($('#dp-source'), app.mode);
    toast('Kunne ikke bytte datakilde', { body: err.message, error: true, ms: 6000 });
  }
}
app.switchSource = switchSource;

// ---------------------------------------------------------------- Statusblokk

const statusRows = new Map();
// Kilden over hver gruppe i ikonlista (v0.0.14): tallet og enheten sier ikke
// om 64 °C er skjermkortet eller prosessoren. Merket står over første verdi
// hver gang kilden skifter, er skjult i full bredde og for skjermlesere (der
// står navnet i .k, og verktøytipset har det fulle navnet).
const SRC_TAG = { gpu: 'GPU', cpu: 'CPU', mem: 'RAM', net: 'NETT', hw: 'ANDRE' };

function buildStatus() {
  const host = $('#side-status-rows');
  host.replaceChildren();
  statusRows.clear();
  const items = app.settings.statusItems || [];
  $('#side-status').hidden = false;
  // Tomtilstand: sier hvor verdiene kommer fra, i stedet for at blokken forsvinner.
  if (!items.length) host.append(el('a', { class: 'empty-note', href: '#/sensorer', 'data-tip': 'Pek på en rad på Sensorer og trykk på nålen', 'data-tip-side': 'right' }, icon('pin'), 'Fest sensorer fra Sensorer'));
  let src = null;
  for (const id of items) {
    const info = describe(app, id);
    const s = String(info.sensor || '').split('/')[0];
    if (s !== src && SRC_TAG[s]) host.append(el('div', { class: 'stat-src', 'aria-hidden': 'true', text: SRC_TAG[s] }));
    src = s;
    const k = el('span', { class: 'k', text: info.label });
    const v = el('span', { class: 'v', text: '–' });
    const sp = sparkline('spark');
    sp.svg.style.setProperty('--spark', `var(--${info.def?.color || app.sensors?.color?.(id) || 'c-load'})`);
    const tip = `${info.long} · vis på Sensorer`;
    const a = el('a', { class: 'stat-row', href: `#/sensorer?vis=${encodeURIComponent(info.sensor)}`, 'data-tip': tip, 'data-tip-side': 'right', 'data-id': id }, k, v, sp.svg);
    host.append(a);
    statusRows.set(id, { id, d: info.def || null, a, k, v, sp, lvl: '', label: info.label, tip });
  }
  renderStatus();
  fitStatus();
}

/**
 * Ikonlista ruller aldri (v0.0.14): foten med Innstillinger skal alltid synes.
 * Får ikke alle statusverdiene plass, vises så mange som får plass, i
 * brukerens rekkefølge, og et kildemerke uten verdier under seg tas bort.
 * Verdiene står fortsatt på Oversikt og Sensorer. Under 600 px høyde er hele
 * blokken skjult av CSS fra før.
 */
function fitStatus() {
  const side = $('#sidebar');
  const kids = [...$('#side-status-rows').children];
  for (const n of kids) n.hidden = false;
  if (document.documentElement.dataset.side !== 'rail' || !$('#side-status').checkVisibility()) return;
  const over = () => side.scrollHeight > side.clientHeight + 1;
  while (kids.length && over()) {
    kids.pop().hidden = true;
    while (kids.length && kids.at(-1).classList.contains('stat-src')) kids.pop().hidden = true;
  }
}
let fitQueued = false;
function queueFit() {
  if (fitQueued) return;
  fitQueued = true;
  requestAnimationFrame(() => {
    fitQueued = false;
    fitStatus();
  });
}

function renderStatus() {
  const t = app.settings.thresholds;
  const now = Date.now();
  for (const r of statusRows.values()) {
    let val;
    let text;
    let lv;
    let pts = null;
    let opts;
    if (r.d) {
      const src = r.d.src === 'gpu' ? feed.latest : feed.sysLatest;
      val = src ? r.d.get(src) : null;
      text = val == null ? '–' : r.d.fmt(val);
      lv = r.d.lvl(val, t) || '';
      opts = { now, span: 60_000, fixed: r.d.fixed || null, minSpan: r.d.minSpan || 0 };
      if (app.settings.statusSparks) {
        const hist = r.d.src === 'gpu' ? feed.history : feed.sysHistory;
        pts = [];
        for (let i = hist.length - 1; i >= 0 && hist[i].t > now - 62_000; i--) pts.push([hist[i].t, r.d.get(hist[i])]);
        pts.reverse();
      }
    } else {
      // Festet sensor: verdi, format, farge og historikk fra raden på Sensorer.
      const row = app.sensors?.row(r.id);
      if (row && r.label !== row.short) {
        r.label = row.short;
        r.k.textContent = row.short;
        r.tip = `${row.long} · vis på Sensorer`;
        setTip(r.a, r.tip);
      }
      val = row?.now ?? null;
      text = val == null ? '–' : row.rate ? F.rate(val, row.unit) : row.sideFmt(val);
      lv = (val != null && row.lvl?.(val)) || '';
      opts = { now, span: 60_000, fixed: row?.fixed || null, minSpan: row?.minSpan || 0 };
      if (app.settings.statusSparks) pts = row?.hist || [];
    }
    // Tall og enhet hver for seg: i ikonlista står enheten under tallet (v0.0.10).
    if (r.v.$t !== text) {
      r.v.$t = text;
      const i = text.lastIndexOf(F.NNBSP);
      if (i < 0) r.v.textContent = text;
      else r.v.replaceChildren(el('span', { class: 'vn', text: text.slice(0, i) }), el('span', { class: 'vu', text: text.slice(i + 1) }));
    }
    if (lv !== r.lvl) {
      r.lvl = lv;
      if (lv) r.a.dataset.level = lv;
      else delete r.a.dataset.level;
      // Fargen står også i ord (v0.0.11): gul = over gul grense, rød = over rød grense.
      setTip(r.a, lv ? `${r.tip} · ${LEVEL_TEXT[lv]}` : r.tip);
    }
    if (pts) drawSpark(r.sp, pts, opts);
  }
}

/** Høyreklikk (eller menytasten) på en statusrad: flytt, løsne eller vis på Sensorer. */
let statusMenu = null;
function statusContext(e) {
  const a = e.target.closest('.stat-row[data-id]');
  if (!a) return;
  e.preventDefault();
  const id = a.dataset.id;
  const items = app.settings.statusItems || [];
  const i = items.indexOf(id);
  const label = describe(app, id).long;
  statusMenu?.remove();
  statusMenu = menu(
    [
      { label: 'Flytt opp', icon: 'arrow-up', run: () => moveItem(app, id, -1) },
      { label: 'Flytt ned', icon: 'arrow-down', run: () => moveItem(app, id, 1) },
      { label: 'Løsne', icon: 'pin-off', run: () => app.saveSettings({ statusItems: items.filter((x) => x !== id) }) },
      { sep: true },
      { label: 'Vis på Sensorer', icon: 'arrow-right', run: () => (location.hash = a.getAttribute('href').slice(1)) },
      { label: 'Rekkefølge og utvalg …', icon: 'settings', run: () => app.openSettings?.('sidefelt') },
    ],
    { label: `Valg for ${label}` },
  );
  // Flytt opp/ned som ikke går, er deaktivert (menyen hopper over dem med piltastene).
  togglePopover(a, statusMenu, { align: 'start' });
  const btns = statusMenu.querySelectorAll('.menu-item');
  if (i <= 0) btns[0].disabled = true;
  if (i >= items.length - 1) btns[1].disabled = true;
  statusMenu.querySelector('.menu-item:not(:disabled)')?.focus({ preventScroll: true });
}

// ---------------------------------------------------------------- Status i toppfeltet

let conn = { connected: true, retryIn: 0 };
function renderStatusTag() {
  const tag = $('#status-tag');
  const sim = feed.info && feed.info.mode !== 'live';
  if (!conn.connected) {
    tag.hidden = false;
    tag.className = 'tag tag-lg danger status-tag';
    $('#status-dot').dataset.state = 'danger';
    setText($('#status-text'), 'Frakoblet');
    setTip(tag, 'Mistet forbindelsen til serveren.');
  } else if (sim) {
    tag.hidden = false;
    // Simulert er informasjon, ikke en advarsel: nøytralt merke med grå prikk.
    tag.className = 'tag tag-lg status-tag';
    $('#status-dot').dataset.state = 'sim';
    setText($('#status-text'), 'Simulert GPU');
    setTip(tag, `GPU-tallene er simulert (${lcFirst(feed.info.reason)}). CPU, minne og nett er ekte.`);
  } else tag.hidden = true;
}

// ---------------------------------------------------------------- Dialoger

/** Feilsøkingsradene i Innstillinger › Om (v0.0.10: var en egen dialog). */
function aboutRows() {
  const A = app.state.app;
  const info = feed.info;
  const rows = [
    ['Versjon', `${A.channel} v${A.version} · bygg ${A.build}`],
    ['Node.js (server)', A.node],
    ['Datakilde', info ? (info.mode === 'live' ? 'Live (nvidia-smi)' : `Simulert – ${info.reason}`) : '–'],
    ['Datamappe', app.state.dataDir || '–'],
    ['Bakgrunnstjeneste', app.daemon?.connected ? `${app.daemon.info?.name || '–'} ${app.daemon.info?.version || ''} · protokoll ${app.daemon.info?.protocol ?? '–'}` : 'Ikke tilkoblet'],
    ['Socket', app.daemon?.socket || '–'],
    ['Dokumentasjon', 'docs/PATCHLOG.md og docs/STATUS.md'],
    ['Nettleser', navigator.userAgent.match(/(Chrome|Firefox)\/[\d.]+/)?.[0] || '–'],
  ];
  return rows.map(([k, v]) => el('div', { class: 'kv-row' }, el('dt', { text: k }), el('dd', {}, el('span', { class: k === 'Datamappe' || k === 'Dokumentasjon' || k === 'Socket' ? 'mono' : null, text: v }))));
}
app.aboutRows = aboutRows;

const KEYS = [
  ['Navigasjon', [
    ['1–8', 'Gå til side (i rekkefølgen i sidefeltet)'],
    ['↑ ↓', 'Flytt i sidefeltet'],
    ['Ctrl+,', 'Åpne og lukk Innstillinger'],
    ['Ctrl+?', 'Denne oversikten'],
    ['Ctrl+B', 'Legg sammen eller utvid sidefeltet'],
    ['Esc', 'Lukk meny, popover, dialog eller Innstillinger'],
  ]],
  ['Sider', [
    ['Ctrl+F', 'Filtrer sensorer (Sensorer) eller søk (Innstillinger)'],
    ['/', 'Søk i Innstillinger'],
    ['Alt+↑ Alt+↓', 'Flytt en verdi i statusblokken (Innstillinger › Sidefelt)'],
    ['Mellomrom', 'Pause og fortsett (Grafer)'],
    ['Ctrl+Enter', 'Bruk (Kontroller, Viftekurve, Lys)'],
    ['Ctrl+Z', 'Forkast endringer (Kontroller, Viftekurve, Lys)'],
    ['Ctrl+Shift+Enter', 'Behold endringene etter Bruk'],
  ]],
  ['Viftekurve', [
    ['Tab', 'Velg neste punkt, så start og stopp'],
    ['Piltaster', 'Flytt punktet 1 °C / 1 %'],
    ['Shift+Piltaster', 'Flytt 5 om gangen'],
    ['Delete', 'Fjern punktet'],
    ['Dobbeltklikk', 'Legg til eller fjern punkt'],
  ]],
];
function renderKeys() {
  const body = $('#keys-body');
  if (body.childElementCount) return;
  for (const [title, list] of KEYS) {
    body.append(el('h4', { class: 'keys-h', text: title, style: { padding: 'var(--sp-4) 0 var(--sp-1)', color: 'var(--muted)', fontSize: 'var(--fs-sm)' } }));
    // Én tast per knapp, som i GNOME: [Ctrl] [,] og [↑] [↓].
    const caps = (k) => k.split(' ').flatMap((x) => (x.length > 1 && x.includes('+') ? x.split(/\+(?=.)/) : [x]));
    body.append(el('table', { class: 'keys' }, el('tbody', {}, ...list.map(([k, t]) => el('tr', {}, el('td', { text: t }), el('td', {}, ...caps(k).map((x) => el('kbd', { class: 'key', text: x }))))))));
  }
}

// ---------------------------------------------------------------- Oppstart

/**
 * Henter /api/state. Svarer ikke serveren (stoppet, eller treg oppstart),
 * vises frakoblet-linja med nedtelling, og appen prøver igjen til den får svar.
 */
async function firstState() {
  for (;;) {
    try {
      const st = await api.state();
      $('#conn-banner').hidden = true;
      delete $('#app').dataset.stale;
      return st;
    } catch (err) {
      setText($('#device-sub'), 'Ingen kontakt med serveren');
      if ($('#conn-banner').hidden) announce('Ingen kontakt med serveren. Prøver igjen.');
      $('#conn-banner').hidden = false;
      $('#app').dataset.stale = '';
      await new Promise((res) => {
        let left = 3;
        const show = () => setText($('#conn-text'), `${err.message} Prøver igjen om ${left}${F.NNBSP}s.`);
        show();
        const t = setInterval(() => {
          left--;
          if (left <= 0) done();
          else show();
        }, 1000);
        const btn = $('#conn-retry');
        function done() {
          clearInterval(t);
          btn.removeEventListener('click', done);
          setText($('#conn-text'), 'Kobler til …');
          res();
        }
        btn.addEventListener('click', done);
      });
    }
  }
}

async function boot() {
  initTooltips();
  initPopovers();
  // rAF-løkka sover når ingenting beveger seg. Nye data og all input vekker den.
  for (const ev of ['sample', 'sys', 'hello', 'status']) feed.on(ev, wake);
  for (const ev of ['pointerdown', 'pointermove', 'pointerup', 'keydown', 'wheel', 'input', 'change', 'focusin', 'focusout']) document.addEventListener(ev, wake, { capture: true, passive: true });
  window.addEventListener('resize', wake);
  app.state = await firstState();
  if (app.state.storeNote) toast('Innstillingene kunne ikke leses', { body: app.state.storeNote, error: true, ms: 12000 });
  app.name = app.state.app.name;
  for (const n of $$('[data-app-name]')) n.textContent = app.name;
  $('#channel-tag').textContent = app.state.app.channel;
  app.applySettings(app.state.settings, Object.keys(app.state.settings));
  onTheme(() => app.emit('theme'));
  app.resetDraft();
  app.daemon = app.state.daemon || null;

  app.pages.oversikt = initOverview(app);
  app.pages.sensorer = initSensors(app);
  app.pages.grafer = initMonitor(app);
  app.pages.grafikkort = initGpu(app);
  app.pages.system = initSystem(app);
  app.pages.kontroller = initControls(app);
  app.pages.viftekurve = initFanCurve(app);
  app.pages.lys = initLights(app);
  app.pages.innstillinger = initSettings(app);
  // Påskeegg (v0.0.12): alle står i easter.js og slås av med «Små overraskelser».
  initEaster(app);

  // Infolinjer som kan lukkes (huskes i innstillingene).
  const renderInfobars = () => {
    const d = new Set(app.settings.dismissed || []);
    for (const b of $$('[data-infobar]')) b.hidden = d.has(b.dataset.infobar);
  };
  document.addEventListener('click', (e) => {
    const b = e.target.closest('[data-close-infobar]');
    if (!b) return;
    const id = b.closest('[data-infobar]').dataset.infobar;
    app.saveSettings({ dismissed: [...new Set([...(app.settings.dismissed || []), id])] });
  });

  app.on('settings', (keys) => {
    if (keys.includes('hiddenPages')) renderNav();
    if (keys.includes('statusItems') || keys.includes('statusSparks')) buildStatus();
    if (keys.includes('dismissed')) renderInfobars();
    if (keys.includes('intervalMs')) renderDevice();
    if (keys.some((k) => ['thresholds', 'unit', 'decimals', 'netUnit'].includes(k))) renderStatus();
  });
  renderNav();
  buildStatus();
  renderInfobars();
  $('#side-status-rows').addEventListener('contextmenu', statusContext);

  let lastMode = null;
  feed.on('hello', (info) => {
    renderDevice();
    renderStatusTag();
    renderTitle();
    if (lastMode && lastMode !== info.mode) {
      toast(info.mode === 'live' ? 'Byttet til live-data' : 'Byttet til simulert GPU', { body: info.reason });
    }
    lastMode = info.mode;
    app.emit('mode', info);
  });
  let lastSide = 0;
  const sideTick = () => {
    const now = performance.now();
    if (now - lastSide < 450) return;
    lastSide = now;
    renderStatus();
  };
  feed.on('sample', sideTick);
  feed.on('sys', sideTick);
  feed.on('status', (st) => {
    if (st.connected !== conn.connected) announce(st.connected ? 'Tilkoblet serveren igjen.' : 'Frakoblet fra serveren. Verdiene er gamle.');
    conn = st;
    $('#conn-banner').hidden = st.connected;
    // Enhetskortet skal ikke si «Live» med grønn prikk mens forbindelsen er brutt.
    if (!st.connected) {
      for (const d of [$('#device-dot'), $('#device-dot2')]) d.dataset.state = 'off';
      setText($('#device-sub'), 'Frakoblet');
    } else renderDevice();
    if (st.connected) delete $('#app').dataset.stale;
    else $('#app').dataset.stale = '';
    // Bruk og Forkast låses mens serveren ikke svarer (sidene lytter på 'conn').
    if (app.offline !== !st.connected) {
      app.offline = !st.connected;
      app.emit('conn', st.connected);
    }
    if (!st.connected) setText($('#conn-text'), st.retryIn > 0 ? `Prøver igjen om ${st.retryIn}${F.NNBSP}s.` : 'Kobler til …');
    renderStatusTag();
  });
  $('#conn-retry').addEventListener('click', () => feed.connect());

  // ---- Bakgrunnstjenesten ----
  app.confirm = initConfirm(app);
  app.daemon = app.state.daemon || null;
  let daemonTimer = null;
  let daemonWas = app.daemon?.connected ?? true;
  /** «Ikke tilkoblet» på sidene som skriver. Serverens egen frakoblet-linje går foran. */
  function renderDaemonBanner() {
    const d = app.daemon;
    const show = !!d && !d.connected && !app.offline && ['kontroller', 'viftekurve'].includes(app.current);
    $('#daemon-banner').hidden = !show;
    clearInterval(daemonTimer);
    daemonTimer = null;
    if (!show) return;
    const tick = () => {
      const left = d.retryAt ? Math.ceil((d.retryAt - Date.now()) / 1000) : 0;
      setText($('#daemon-text'), `Endringer kan ikke brukes nå. ${left > 0 ? `Prøver igjen om ${left}${F.NNBSP}s.` : 'Kobler til …'}`);
    };
    tick();
    daemonTimer = setInterval(tick, 1000);
  }
  app.renderDaemonBanner = renderDaemonBanner;
  $('#daemon-retry').addEventListener('click', () => api.retryDaemon().catch(() => {}));
  const sameValues = (a, b) => !!a && !!b && FORM_KEYS.every((k) => a[k] === b[k]) && sameCurve(a.fanCurve, b.fanCurve);
  feed.on('daemon', (d) => {
    const prev = app.state.applied;
    app.daemon = d;
    if (d.limits) app.state.limits = d.limits;
    if (d.limitInfo) app.state.limitInfo = d.limitInfo;
    if (d.applied && !sameValues(d.applied, prev)) {
      // Rullet tilbake eller endret i en annen fane: utkastet følger med hvis
      // du ikke har endret noe selv siden.
      const clean = sameValues(app.draft, prev);
      app.setApplied(d.applied);
      if (clean) app.resetDraft();
    }
    if (d.connected !== daemonWas) {
      announce(d.connected ? 'Bakgrunnstjenesten er tilkoblet igjen.' : 'Bakgrunnstjenesten er ikke tilkoblet. Endringer kan ikke brukes.');
      daemonWas = d.connected;
    }
    // Ventet en endring, sier bekreftelseslinja at den ble rullet tilbake; varselet gjentar det ikke.
    if (d.shutdown) toast('Bakgrunnstjenesten stoppet', { body: `Viften gikk tilbake til auto.${d.shutdown.rolledBack && !app.confirm.pending ? ' Endringene som ventet, ble rullet tilbake.' : ''}`, ms: 8000 });
    app.confirm.sync(d);
    renderDaemonBanner();
    app.emit('daemon', d);
  });
  app.on('conn', renderDaemonBanner);
  // Lukket eller lastet på nytt midt i fristen: tjenesten skal rulle tilbake.
  window.addEventListener('pagehide', () => {
    if (app.confirm.pending) beaconTabClosed();
    feed.disconnect();
  });
  // Tilbake til en side fra hurtigbufferen: koble til på nytt.
  window.addEventListener('pageshow', (e) => e.persisted && feed.connect());
  // Lastet på nytt midt i fristen: tjenesten rullet tilbake, og den nye siden sier det.
  const rec = app.daemon?.recent;
  if (rec?.status === 'rolled_back' && rec.reason === 'tab_closed' && Date.now() - rec.at < 15_000) app.confirm.showResult(rec);

  // Innstillinger endret i en annen fane (eller mens denne lå i bakgrunnen).
  let refreshing = false;
  async function refreshSettings() {
    if (refreshing) return;
    refreshing = true;
    try {
      const st = await api.state();
      app.syncSettings(st.settings);
    } catch {
    } finally {
      refreshing = false;
    }
  }
  // 'settings': hent innstillingene på nytt. Andre meldinger (f.eks. 'minmax') går til sidene.
  if (tabs) tabs.onmessage = (e) => (e.data === 'settings' ? refreshSettings() : app.emit('tabs', e.data));
  document.addEventListener('visibilitychange', () => document.visibilityState === 'visible' && refreshSettings());
  app.notifyTabs = (msg = 'settings') => tabs?.postMessage(msg);
  app.on('unit', renderStatus);

  // Enhetskort og hovedmeny.
  $('#device-card').addEventListener('click', () => {
    renderDevice();
    togglePopover($('#device-card'), $('#device-pop'));
  });
  bindSeg($('#dp-source'), switchSource);
  $('#dp-more').addEventListener('click', () => closePopover());
  // Sidefeltets fot: Innstillinger (lenke), Tastatursnarveier og legg sammen.
  function showKeys() {
    renderKeys();
    openDialog($('#dlg-keys'));
  }
  app.showAbout = () => app.openSettings?.('om');
  app.showKeys = showKeys;
  $('#side-keys').addEventListener('click', showKeys);
  $('#keys-about').addEventListener('click', (e) => {
    e.preventDefault();
    $('#dlg-keys').close();
    app.showAbout();
  });
  $('#side-toggle').addEventListener('click', toggleSide);
  mqNarrow.addEventListener('change', applySide);
  app.on('settings', (keys) => keys.includes('sideCollapsed') && applySide());
  applySide();
  // Plassen i ikonlista endres med vindushøyden, skriftstørrelsen og tettheten.
  window.addEventListener('resize', queueFit);
  app.on('settings', queueFit);
  app.on('theme', queueFit);
  document.fonts?.ready.then(queueFit);

  bindNavKeys();
  feed.connect();
  window.addEventListener('hashchange', route);
  route();
  booted = true;
  enableViewTransitions();

  document.addEventListener('keydown', (e) => {
    if (document.querySelector('dialog[open]')) return;
    const t = e.target;
    const typing = t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
    // Ctrl+, åpner Innstillinger, og lukker dem igjen (v0.0.15).
    if ((e.ctrlKey || e.metaKey) && e.key === ',') {
      e.preventDefault();
      if (app.current === SETTINGS) closeSettings();
      else app.go(SETTINGS);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && (e.key === '?' || (e.shiftKey && e.key === '/'))) {
      e.preventDefault();
      showKeys();
      return;
    }
    // Legg sammen eller utvid sidefeltet (ikke i smale vinduer: der er det alltid en ikonlist).
    // Mens Innstillinger dekker vinduet, synes ikke sidefeltet; da gjør Ctrl+B ingenting.
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'b') {
      e.preventDefault();
      if (!mqNarrow.matches && app.current !== SETTINGS) toggleSide();
      return;
    }
    // Behold endringene som venter, uansett side.
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'Enter' && app.confirm?.pending) {
      e.preventDefault();
      app.confirm.keep();
      return;
    }
    // Sidene får tastene først (Ctrl+Enter, Ctrl+Z, Ctrl+F, mellomrom).
    if (app.pages[app.current]?.onKey?.(e, typing)) {
      e.preventDefault();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey || typing || (t && t.tagName === 'CANVAS')) return;
    const n = Number(e.key);
    if (n >= 1 && n <= PAGES.length) app.go(PAGES[n - 1][0]);
  });
}

boot();
