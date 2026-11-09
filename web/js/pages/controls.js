// Kontroller (simulert): effektgrense, offset, temperaturmål, vifte og
// profiler. Hver kontroll er én rad: tittel og beskrivelse, glider med
// standardmerke, tallfelt og ↺ (vises bare når verdien avviker fra standard).
// Under justeringene står effekt og kjerneklokke for siste minutt, så du ser
// hva modellen gjør med det du bruker. Til høyre: endringer, profiler og
// spennings- og klokkekurven. Forkast og Bruk står i toppfeltet.
//
// v0.0.7: Bruk går til bakgrunnstjenesten (simulert). Grensene ved hver
// glider er de kortet melder gjennom tjenesten. Tallfeltet lar deg skrive
// utenfor; tjenesten klemmer da verdien og sier fra, og raden viser det.
// Etter Bruk spør linja under toppfeltet om du vil beholde endringene
// (confirm.js). Ingenting sendes til kortet.

import { $, el, fitCanvas, icon, cssVar, onFrame, setText } from '../util.js';
import * as F from '../fmt.js';
import { confirmDone, fillRange, menu, numField, segEl, setSeg, setTip, togglePopover } from '../ui.js';
import { curveAt, restState } from '../fancurve.js';
import { niceScale } from '../charts.js';
import { windowSelect } from '../chartpanel.js';
import { defPanel } from './monitor.js';

const KEYS = ['powerLimit', 'coreOffset', 'memOffset', 'tempTarget', 'fanPercent'];
export const FORM_KEYS = [...KEYS, 'fanMode', 'fanStartStop', 'fanStartTemp', 'fanStopTemp', 'fanMinPercent', 'fanHysteresis', 'fanRampUp', 'fanRampDown', 'fanSource'];

export const sameCurve = (a, b) => a.length === b.length && a.every((p, i) => p[0] === b[i][0] && p[1] === b[i][1]);

export const LABEL = {
  powerLimit: 'Effektgrense',
  coreOffset: 'Kjerneklokke-offset',
  memOffset: 'Minneklokke-offset',
  tempTarget: 'Temperaturmål',
  fanPercent: 'Viftefart (manuell)',
  fanMode: 'Viftemodus',
  fanStartStop: 'Start og stopp',
  fanStartTemp: 'Start ved',
  fanStopTemp: 'Stopp under',
  fanMinPercent: 'Minste viftefart',
  fanHysteresis: 'Senk først etter',
  fanRampUp: 'Reaksjonstid opp',
  fanRampDown: 'Reaksjonstid ned',
  fanSource: 'Temperaturkilde',
};

/** Verdien som tekst, med enhet. */
export function valText(k, v) {
  switch (k) {
    case 'powerLimit':
      return F.watt(v);
    case 'coreOffset':
    case 'memOffset':
      return F.withUnit(F.signed(v), 'MHz');
    case 'tempTarget':
    case 'fanStartTemp':
    case 'fanStopTemp':
      return F.temp(v);
    case 'fanStartStop':
      return v ? 'På' : 'Av';
    case 'fanPercent':
    case 'fanMinPercent':
      return F.pct(v);
    case 'fanMode':
      return v === 'curve' ? 'Kurve' : v === 'manual' ? 'Manuell' : 'Auto';
    case 'fanHysteresis':
      return F.tempDelta(v);
    case 'fanRampUp':
    case 'fanRampDown':
      return F.withUnit(F.num(v), 's');
    case 'fanSource':
      return v === 'cpu' ? 'CPU' : 'GPU';
    default:
      return String(v);
  }
}

const fanSummary = (v) => (v.fanMode === 'manual' ? `vifte ${F.pct(v.fanPercent)}` : v.fanMode === 'curve' ? 'vifte kurve' : 'vifte auto');
export function summary(v) {
  return `${F.watt(v.powerLimit)} · kjerne ${F.withUnit(F.signed(v.coreOffset), 'MHz')} · minne ${F.withUnit(F.signed(v.memOffset), 'MHz')} · mål ${F.temp(v.tempTarget)} · ${fanSummary(v)}`;
}

/** Hvor grensen kommer fra, til verktøytipset på glidernes ender. */
function limitTip(app, key, end) {
  const li = app.state.limitInfo?.[key];
  const word = end === 'min' ? 'minimum' : 'maks';
  if (!li) return app.state.limitInfo?.source === 'daemon' ? null : `Appens ${word}. Bakgrunnstjenesten er ikke tilkoblet, så kortets grenser er ikke lest.`;
  if (li.source === 'policy' && li.hardware) {
    const u = key === 'coreOffset' || key === 'memOffset' ? 'MHz' : '';
    return `Tjenestens ${word}. Kortet melder ${F.signed(li.hardware.min)} til ${F.withUnit(F.signed(li.hardware.max), u)}, men tjenesten tillater mindre.`;
  }
  return `Kortets ${word}, meldt av bakgrunnstjenesten.`;
}

export function initControls(app) {
  const stock = app.state.defaultApplied;
  const L = app.state.limits;
  const host = $('#ctl-rows');
  const ctls = {};

  // ---- Rader ----
  const DEFS = [
    { key: 'powerLimit', title: 'Effektgrense', desc: () => `Hvor mange watt kortet får trekke. Standard\u00a0${F.watt(stock.powerLimit)}.`, step: 1, unit: () => 'W' },
    { key: 'coreOffset', title: 'Kjerneklokke-offset', desc: () => 'Flytter hele klokkekurven opp eller ned.', step: 5, unit: () => 'MHz', signed: true },
    { key: 'memOffset', title: 'Minneklokke-offset', desc: () => `Effektiv minnefrekvens. Standard\u00a0${F.mhz(7000)}.`, step: 25, unit: () => 'MHz', signed: true },
    { key: 'tempTarget', title: 'Temperaturmål', desc: () => `Kortet senker klokken når temperaturen nås. Standard\u00a0${F.temp(stock.tempTarget)}.`, step: 1, unit: () => F.tempLabel(), temp: true },
  ];
  const toDisp = (d, v) => (d.temp ? Math.round(F.tempOut(v)) : v);
  const fromDisp = (d, v) => (d.temp ? Math.round(F.tempIn(v)) : v);

  function sliderEl(key, min, max, step) {
    const range = el('input', { type: 'range', min, max, step, 'aria-label': LABEL[key] });
    const stockMark = el('span', { class: 'stock', 'data-tip': 'Standard' });
    stockMark.style.setProperty('--at', String((stock[key] - min) / (max - min)));
    const lo = el('span', { class: 'num' });
    const hi = el('span', { class: 'num' });
    return { root: el('div', { class: 'slider' }, lo, el('div', { class: 'track-wrap' }, range, stockMark), hi), range, lo, hi };
  }

  for (const d of DEFS) {
    const [min, max] = L[d.key];
    const sl = sliderEl(d.key, min, max, d.step);
    const nf = numField({
      min: toDisp(d, min),
      max: toDisp(d, max),
      step: d.step,
      value: toDisp(d, app.draft[d.key]),
      unit: d.unit(),
      label: `${d.title} i ${d.unit()}`,
      // Ingen klemming her: utenfor grensene sier raden fra, og tjenesten klemmer.
      clamp: false,
      onInput: (v) => setDraft(d.key, Math.round(fromDisp(d, v)), { fromField: true }),
      onCommit: () => render(),
    });
    nf.input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') applyNow();
    });
    sl.range.addEventListener('input', () => setDraft(d.key, Number(sl.range.value)));
    const reset = el('button', { type: 'button', class: 'btn flat icon-only reset-btn', 'data-tip': 'Tilbake til standard', 'aria-label': `${d.title}: tilbake til standard` }, icon('undo-2'));
    reset.addEventListener('click', () => setDraft(d.key, stock[d.key]));
    const desc = el('p');
    const note = el('p', { class: 'ctl-note', hidden: true }, icon('info'), el('span'));
    const root = el('div', { class: 'ctl', 'data-key': d.key }, el('div', { class: 'ctl-text' }, el('h3', {}, el('span', { class: 'changed-dot', 'data-tip': 'Endret, ikke brukt' }), d.title), desc, note), sl.root, el('div', { class: 'right' }, nf.root), reset);
    host.append(root);
    ctls[d.key] = { d, root, range: sl.range, num: nf.input, unitEl: nf.root.querySelector('.u'), lo: sl.lo, hi: sl.hi, desc, note, min, max, stockMark: sl.root.querySelector('.stock') };
  }

  // Vifte: Auto/Manuell. I Auto skjules glideren og raden viser kurvens verdi nå.
  {
    const [min, max] = L.fanPercent;
    const sl = sliderEl('fanPercent', min, max, 1);
    const nf = numField({ min, max, step: 1, value: app.draft.fanPercent, unit: '%', label: 'Viftefart i prosent', clamp: false, onInput: (v) => setDraft('fanPercent', Math.round(v), { fromField: true }), onCommit: () => render() });
    sl.range.addEventListener('input', () => setDraft('fanPercent', Number(sl.range.value)));
    const seg = segEl(
      [
        ['auto', 'Auto', 'Driveren styrer viften, som uten appen'],
        ['curve', 'Kurve', 'Bakgrunnstjenesten følger viftekurven'],
        ['manual', 'Manuell', 'Fast fart'],
      ],
      app.draft.fanMode,
      (v) => setDraft('fanMode', v),
      { label: 'Viftemodus' },
    );
    const autoNote = el('div', { class: 'auto-note' }, el('span', { class: 'txt' }), el('a', { class: 'btn link', href: '#/viftekurve', 'data-tip': 'Rediger viftekurven' }, 'Rediger', icon('arrow-right')));
    const reset = el('button', { type: 'button', class: 'btn flat icon-only reset-btn', 'data-tip': 'Tilbake til standard (Auto, driveren styrer)', 'aria-label': 'Vifte: tilbake til standard' }, icon('undo-2'));
    reset.addEventListener('click', () => {
      app.draft.fanMode = stock.fanMode;
      app.draft.fanPercent = stock.fanPercent;
      render();
      changed();
    });
    // Glideren og tallfeltet for Manuell står i midten, der Auto og Kurve har
    // teksten sin. Da står modusvelgeren på samme sted i alle tre modi, og
    // raden får ikke ny høyde når du bytter (v0.0.8).
    const mid = el('div', { class: 'fan-mid' }, sl.root, nf.root, autoNote);
    const note = el('p', { class: 'ctl-note', hidden: true }, icon('info'), el('span'));
    const root = el(
      'div',
      { class: 'ctl', 'data-key': 'fan' },
      el('div', { class: 'ctl-text' }, el('h3', {}, el('span', { class: 'changed-dot', 'data-tip': 'Endret, ikke brukt' }), 'Vifte'), el('p', { text: 'Auto lar driveren styre. Kurve følger viftekurven. Manuell gir fast fart (minst 35 %).' }), note),
      mid,
      el('div', { class: 'right' }, seg),
      reset,
    );
    host.append(root);
    ctls.fanPercent = { root, range: sl.range, num: nf.input, numRoot: nf.root, lo: sl.lo, hi: sl.hi, slider: sl.root, autoNote, seg, note, min, max, d: { key: 'fanPercent' } };
  }

  function setDraft(k, v, { fromField = false } = {}) {
    app.draft[k] = v;
    render({ skipField: fromField ? k : null });
    changed();
  }

  function isDirty() {
    const a = app.state.applied;
    const d = app.draft;
    const keys = FORM_KEYS.filter((k) => d[k] !== a[k]);
    const curve = !sameCurve(d.fanCurve, a.fanCurve);
    return { keys, curve, any: keys.length > 0 || curve, n: keys.length + (curve ? 1 : 0) };
  }

  function render(opts) {
    const skipField = opts?.skipField ?? null;
    const d = app.draft;
    const a = app.state.applied;
    for (const k of KEYS) {
      const c = ctls[k];
      c.range.value = String(d[k]);
      fillRange(c.range);
      if (skipField !== k && document.activeElement !== c.num) c.num.value = String(c.d.temp ? Math.round(F.tempOut(d[k])) : d[k]);
      const sign = c.d.signed ? F.signed : (v) => F.num(v);
      setText(c.lo, c.d.temp ? F.tempNum(c.min) : sign(c.min));
      setText(c.hi, c.d.temp ? F.tempNum(c.max) : sign(c.max));
      if (c.desc) setText(c.desc, c.d.desc());
      if (c.unitEl) setText(c.unitEl, c.d.unit());
      c.root.toggleAttribute('data-nonstock', k !== 'fanPercent' && d[k] !== stock[k]);
      c.root.toggleAttribute('data-changed', k !== 'fanPercent' && d[k] !== a[k]);
      setTip(c.lo, limitTip(app, k, 'min'));
      setTip(c.hi, limitTip(app, k, 'max'));
      renderNote(k);
    }
    const f = ctls.fanPercent;
    const manual = d.fanMode === 'manual';
    setSeg(f.seg, d.fanMode);
    f.slider.hidden = !manual;
    f.numRoot.hidden = !manual;
    f.autoNote.hidden = manual;
    setText(f.lo, F.num(f.min));
    setText(f.hi, F.num(f.max));
    f.root.toggleAttribute('data-nonstock', d.fanMode !== stock.fanMode);
    f.root.toggleAttribute('data-changed', d.fanMode !== a.fanMode || (manual && d.fanPercent !== a.fanPercent));
    setTip(f.slider, `Fra ${F.pct(f.min)} til ${F.pct(f.max)}. ${app.state.limitInfo?.source === 'daemon' ? 'Grensene er meldt av bakgrunnstjenesten.' : 'Appens egne grenser (bakgrunnstjenesten er ikke tilkoblet).'}`);
    renderAutoNote();
    renderDirty();
  }

  /**
   * Under tittelen: «Over kortets maks …» før Bruk, og «Klemt til …» når
   * tjenesten har klemt verdien som nå er i bruk.
   */
  function renderNote(k) {
    const c = ctls[k];
    const v = app.draft[k];
    const a = app.state.applied[k];
    const show = (txt) => {
      c.note.hidden = !txt;
      if (txt) setText(c.note.lastChild, txt);
    };
    if (k === 'fanPercent' && app.draft.fanMode !== 'manual') return show(null);
    const over = v > c.max ? 'max' : v < c.min ? 'min' : null;
    if (over) {
      const lim = over === 'max' ? c.max : c.min;
      const whose = app.state.limitInfo?.[k]?.source === 'policy' ? 'tjenestens' : 'kortets';
      return show(`${over === 'max' ? 'Over' : 'Under'} ${whose} ${over === 'max' ? 'maks' : 'minimum'}. Klemmes til ${valText(k, lim)}.`);
    }
    const cl = app.clamped?.[k];
    if (cl && v === a && a === cl.applied) return show(`Klemt av tjenesten: ${valText(k, cl.requested)} → ${valText(k, cl.applied)}`);
    show(null);
  }

  // Start/stopp: hviletilstanden regnes her fra utkastet (samme regel som simulatoren).
  let rest = null;
  function renderAutoNote() {
    const s = app.feed.latest;
    const t = s?.temp;
    const d = app.draft;
    rest = restState(rest, t, { on: d.fanStartStop, start: d.fanStartTemp, stop: d.fanStopTemp });
    const v = t == null ? null : rest ? d.fanMinPercent : Math.max(d.fanMinPercent, curveAt(d.fanCurve, t));
    const want = v == null ? null : `${F.pct(v)}${rest ? ' (under start)' : ''}`;
    const curve = d.fanMode === 'curve';
    const link = ctls.fanPercent.autoNote.querySelector('a');
    link.hidden = !curve;
    // I live-modus styrer driveren viften. Kurven er bare simulert, så raden sier hva den ville gitt.
    let txt;
    const emergency = app.mode !== 'live' && s?.emergency;
    const safe = app.daemon?.safety || { emergencyReleaseC: 80 };
    if (emergency) txt = `Nødregel: viften går ${F.pct(100)} til temperaturen er under ${F.temp(safe.emergencyReleaseC)}.`;
    else if (!curve) txt = app.mode === 'live' ? 'Driveren styrer viften.' : s?.fan != null ? `Driveren styrer viften · nå ${F.pct(s.fan)}` : 'Driveren styrer viften.';
    else if (app.mode === 'live') txt = want ? `Driveren styrer viften. Den simulerte kurven ville gitt ${want}.` : 'Driveren styrer viften.';
    else if (!want) txt = 'Følger viftekurven.';
    else if (rest) txt = `Hviler til ${F.temp(d.fanStartTemp)} · nå ${want}`;
    else txt = `Kurven gir ${want} ved\u00a0${F.temp(t)}`;
    setText(ctls.fanPercent.autoNote.querySelector('.txt'), txt);
  }

  /** Nederst i Justeringer: tjenestens sikkerhetsregler, uten varselfarge. */
  function renderSafety() {
    const sf = app.daemon?.safety || { emergencyFanC: 85 };
    const sec = app.settings.confirmSec || 15;
    setText($('#ctl-safety-text'), `Over ${F.temp(sf.emergencyFanC)} går viften alltid ${F.pct(100)}. Stopper bakgrunnstjenesten, går viften til auto. Uten Behold rulles endringer tilbake etter ${sec}${F.NNBSP}s.`);
    const tag = $('#ctl-limits-tag');
    const fromCard = app.state.limitInfo?.source === 'daemon';
    tag.textContent = fromCard ? 'Grenser fra kortet' : 'Grenser i appen';
    setTip(tag, fromCard ? 'Glidernes ender er grensene kortet melder gjennom bakgrunnstjenesten. Skriver du en verdi utenfor, klemmer tjenesten den og sier fra.' : 'Bakgrunnstjenesten er ikke tilkoblet, så kortets grenser er ikke lest. Appen bruker sine egne.');
  }

  function renderDirty() {
    const st = isDirty();
    const dirty = $('#ctl-dirty');
    const pending = !!app.confirm?.pending;
    dirty.dataset.state = st.any ? 'dirty' : pending ? 'pending' : 'clean';
    setText($('#ctl-dirty-text'), st.any ? `${st.n} ${st.n === 1 ? 'endring' : 'endringer'} ikke brukt` : pending ? 'Venter på Behold' : 'Ingen endringer');
    for (const id of ['#ctl-discard', '#ctl-apply']) {
      const b = $(id);
      // Forkast virker uten server; Bruk venter til serveren og tjenesten er tilbake.
      const why = id === '#ctl-apply' ? waitReason(app) : null;
      b.disabled = !st.any || !!why;
      setTip(b, why || (st.any ? (id === '#ctl-apply' ? 'Bruk endringene (simulert)' : 'Forkast endringene') : id === '#ctl-apply' ? applyIdleTip(app) : 'Ingen endringer å forkaste'));
    }
    // Endringsliste: bare forskjeller.
    const list = $('#ctl-diff');
    const a = app.state.applied;
    const d = app.draft;
    list.replaceChildren();
    for (const k of st.keys) {
      if (k === 'fanPercent' && d.fanMode !== 'manual') continue;
      list.append(el('div', { class: 'kv-row' }, el('dt', { text: LABEL[k] }), el('dd', {}, el('span', { class: 'from', text: valText(k, a[k]) }), icon('arrow-right'), el('b', { text: valText(k, d[k]) }))));
    }
    if (st.curve) list.append(el('div', { class: 'kv-row' }, el('dt', { text: 'Viftekurve' }), el('dd', {}, el('a', { class: 'btn link', href: '#/viftekurve', text: `Endret · ${d.fanCurve.length} punkter` }))));
    // Uten endringer er kortet bare én linje (toppfeltet sier det samme).
    $('#ctl-diff-body').hidden = !st.any;
    $('#ctl-diff-group').classList.toggle('compact', !st.any);
    setText($('#ctl-diff-aside'), st.any ? 'Trykk Bruk for å ta dem i bruk' : pending ? 'Venter på Behold' : 'Ingen ulagrede endringer');
    vf.needs = true;
    renderSlotsHint();
  }

  function changed() {
    app.emit('draft-local');
  }

  function renderMode() {
    const live = app.mode === 'live';
    setText(
      $('#ctl-banner-text'),
      live
        ? 'Alt her er simulert. Bruk sender verdiene til en simulert bakgrunnstjeneste. Skjermkortet blir ikke endret, og live-tallene påvirkes ikke.'
        : 'Alt her er simulert. Bruk sender verdiene til en simulert bakgrunnstjeneste, og modellen reagerer på dem. Skjermkortet blir ikke endret.',
    );
    setText($('#ctl-mode-note'), live ? 'Påvirker ikke live-tallene' : 'Modellen reagerer på det du bruker');
  }

  // Dobbeltklikk eller Ctrl+Enter to ganger skal bare gi én lagring.
  let applying = false;
  async function applyNow() {
    if (applying || waitReason(app) || !isDirty().any) return;
    applying = true;
    const btn = $('#ctl-apply');
    const hadFocus = document.activeElement === btn;
    btn.disabled = true;
    try {
      const d = app.draft;
      const patch = { fanCurve: d.fanCurve };
      for (const k of FORM_KEYS) patch[k] = d[k];
      const res = await app.api.apply(patch);
      app.afterApply(res, { focus: hadFocus });
      // Venter endringen på Behold, sier bekreftelseslinja fra. Da får Bruk
      // ingen hake, bare den vanlige deaktiverte formen.
      if (!res.pending) confirmDone(btn);
    } catch (err) {
      app.toast('Kunne ikke bruke innstillingene', { body: err.message, error: true });
    } finally {
      applying = false;
      renderDirty();
    }
  }

  function discard() {
    if (!isDirty().any) return;
    app.resetDraft();
    render();
  }

  async function resetAll() {
    const prev = app.state.applied;
    try {
      const res = await app.api.reset();
      app.setApplied(res.applied);
      app.resetDraft();
      app.toast('Tilbakestilt til standard (simulert)', {
        body: 'Viften er på Auto, og alt annet på kortets standard.',
        action: {
          label: 'Angre',
          run: async () => {
            try {
              // Angre er en ny endring: den må også beholdes innen fristen.
              app.afterApply(await app.api.apply(prev));
            } catch (err) {
              app.toast('Kunne ikke angre', { body: err.message, error: true });
            }
          },
        },
      });
    } catch (err) {
      app.toast('Kunne ikke tilbakestille', { body: err.message, error: true });
    }
  }

  $('#ctl-apply').addEventListener('click', applyNow);
  $('#ctl-discard').addEventListener('click', discard);
  $('#ctl-reset').addEventListener('click', resetAll);

  // ---- Profiler ----
  // Hver profil er én rad: navn, sammendrag, Hent og en ⋯-meny med lagre over,
  // gi nytt navn og tøm. Er verdiene på siden lik en profil, sier knappen det i
  // stedet for å lagre en kopi.
  const slotsEl = $('#slots');
  let renaming = -1;
  let addRow = null;
  let slotMenus = [];

  const sameValues = (a, b) => FORM_KEYS.every((k) => a[k] === b[k]) && sameCurve(a.fanCurve, b.fanCurve);
  const twin = () => app.state.profiles.find((p) => p && sameValues(p.values, app.draft));

  function renderSlots(flash = -1) {
    const items = [];
    for (const m of slotMenus) m.remove();
    slotMenus = [];
    const profiles = app.state.profiles;
    profiles.forEach((p, i) => {
      if (!p) return;
      const n = i + 1;
      let nameEl;
      if (renaming === i) {
        nameEl = el('input', { class: 'field rename', type: 'text', maxlength: '32', value: p.name, 'aria-label': `Nytt navn på ${p.name}` });
        nameEl.addEventListener('keydown', (e) => {
          if (e.key === 'Enter') rename(n, nameEl.value);
          if (e.key === 'Escape') {
            renaming = -1;
            renderSlots();
            e.stopPropagation();
          }
        });
        nameEl.addEventListener('blur', () => renaming === i && rename(n, nameEl.value));
        requestAnimationFrame(() => {
          nameEl.focus();
          nameEl.select();
        });
      } else {
        nameEl = el('span', { class: 's-name' }, el('span', { class: 'ellipsis', text: p.name, 'data-tip': 'Dobbeltklikk for å gi nytt navn' }));
        nameEl.addEventListener('dblclick', () => {
          renaming = i;
          renderSlots();
        });
      }
      const custom = !sameCurve(p.values.fanCurve, app.state.defaultCurve);
      const load = el('button', { type: 'button', class: 'btn sm', 'data-tip': 'Fyll inn verdiene fra profilen. Trykk Bruk etterpå.', text: 'Hent' });
      load.addEventListener('click', () => loadSlot(n));
      const more = el('button', { type: 'button', class: 'btn sm flat icon-only', 'aria-haspopup': 'menu', 'aria-expanded': 'false', 'data-tip': 'Flere valg', 'aria-label': `Flere valg for ${p.name}` }, icon('ellipsis-vertical'));
      const m = menu(
        [
          { label: 'Lagre over med verdiene på siden', icon: 'save', run: () => saveSlot(n, p.name) },
          { label: 'Gi nytt navn', icon: 'pencil', run: () => ((renaming = i), renderSlots()) },
          { label: 'Tøm plassen', icon: 'trash-2', run: () => deleteSlot(n, p) },
        ],
        { label: `Valg for ${p.name}` },
      );
      slotMenus.push(m);
      more.addEventListener('click', () => togglePopover(more, m, { align: 'end' }));
      items.push(
        el(
          'div',
          { class: `slot${flash === i ? ' flash' : ''}` },
          nameEl,
          el('div', { class: 's-btns' }, load, more),
          el('span', { class: 's-sum', 'data-tip': `${summary(p.values)}\nViftekurve: ${custom ? `egen, ${p.values.fanCurve.length} punkter` : 'standard'}\nLagret ${F.dateTime(p.savedAt)}`, text: `${summary(p.values)}${custom ? ' · egen kurve' : ''}` }),
        ),
      );
    });
    const free = profiles.findIndex((p) => !p);
    const used = profiles.filter(Boolean).length;
    setText($('#slots-count'), `${used} av ${profiles.length}`);
    addRow = null;
    if (free >= 0) {
      const add = el('button', { type: 'button', class: 'btn sm' }, icon('plus'), 'Lagre som ny profil');
      add.addEventListener('click', () => !twin() && saveSlot(free + 1, ''));
      const hint = el('span', { class: 'muted' });
      addRow = { add, hint };
      items.push(el('div', { class: 'slot-add' }, add, hint));
    } else items.push(el('p', { class: 'group-note', text: 'Alle fem plassene er i bruk. Lagre over en profil fra ⋯-menyen.' }));
    slotsEl.replaceChildren(...items);
    renderSlotsHint();
  }

  /** «Lik «Stille»» når verdiene på siden allerede finnes som profil. */
  function renderSlotsHint() {
    if (!addRow) return;
    const t = twin();
    addRow.add.disabled = !!t;
    setTip(addRow.add, t ? `Verdiene på siden er de samme som i «${t.name}»` : 'Lagrer verdiene på siden som en ny profil');
    setText(addRow.hint, t ? `Lik «${t.name}»` : '');
  }

  async function rename(n, name) {
    renaming = -1;
    try {
      const res = await app.api.renameProfile(n, name);
      app.state.profiles = res.profiles;
    } catch (err) {
      app.toast('Kunne ikke gi nytt navn', { body: err.message, error: true });
    }
    renderSlots();
  }

  async function saveSlot(n, name) {
    try {
      const res = await app.api.saveProfile(n, name, { ...app.draft });
      app.state.profiles = res.profiles;
      renderSlots(n - 1);
      app.toast(`Lagret i «${res.profile.name}»`, { body: summary(res.profile.values) });
    } catch (err) {
      app.toast('Kunne ikke lagre profilen', { body: err.message, error: true });
    }
  }

  async function loadSlot(n) {
    try {
      const res = await app.api.loadProfile(n);
      const v = res.profile.values;
      Object.assign(app.draft, { ...v, fanCurve: v.fanCurve.map((p) => [...p]) });
      render();
      app.emit('draft-local');
      app.toast(`Hentet «${res.profile.name}»`, { body: 'Trykk Bruk for å ta den i bruk.' });
    } catch (err) {
      app.toast('Kunne ikke hente profilen', { body: err.message, error: true });
    }
  }

  async function deleteSlot(n, p) {
    try {
      const res = await app.api.deleteProfile(n);
      app.state.profiles = res.profiles;
      renderSlots();
      app.toast(`«${p.name}» er tømt`, {
        action: {
          label: 'Angre',
          run: async () => {
            try {
              const r = await app.api.restoreProfile(n, p);
              app.state.profiles = r.profiles;
              renderSlots(n - 1);
            } catch (err) {
              app.toast('Kunne ikke angre', { body: err.message, error: true });
            }
          },
        },
      });
    } catch (err) {
      app.toast('Kunne ikke tømme plassen', { body: err.message, error: true });
    }
  }

  // ---- Effekt og kjerneklokke ----
  // Viser hva den simulerte modellen gjør med det du bruker (i live-modus de
  // ekte tallene, som ikke påvirkes). Samme graf-panel som på Grafer (v0.0.11),
  // med eget tidsvindu i toppfeltet.
  const csync = { t: null };
  const cwin = () => app.settings.windows?.kontroller || 60;
  const panels = [
    defPanel(app, 'power', { title: 'Effekt', window: cwin(), sync: csync, stats: false, now: false }),
    defPanel(app, 'coreClock', { title: 'Kjerneklokke', window: cwin(), sync: csync, stats: false, now: false }),
  ];
  for (const p of panels) $('#ctl-charts').append(p.root);
  const charts = panels.map((p) => p.chart);
  const cwinSel = windowSelect(cwin(), (sec) => app.saveSettings({ windows: { ...app.settings.windows, kontroller: sec } }), { label: 'Tidsvindu for grafene på Kontroller' });
  $('#ctl-win-host').append(cwinSel);
  app.on('settings', (k) => {
    if (!k.includes('windows')) return;
    cwinSel.value = String(cwin());
    for (const p of panels) p.setWindow(cwin());
  });

  // ---- V/F-kurve (illustrasjon) ----
  // Typisk Turing-kurve (RTX 2080): spenning (mV) → klokke (MHz). Ikke lest fra kortet.
  const VF = [
    [700, 1350],
    [750, 1455],
    [800, 1560],
    [850, 1665],
    [900, 1755],
    [950, 1845],
    [1000, 1920],
    [1050, 1980],
    [1068, 1995],
  ];
  const vfCanvas = $('#vf-canvas');
  const vf = { needs: true };
  const { ctx, box } = fitCanvas(vfCanvas, () => (vf.needs = true));
  let vfColors = null;
  const readVf = () => {
    vfColors = { grid: cssVar('--grid'), axis: cssVar('--muted'), accent: cssVar('--c-clock'), ghost: cssVar('--c-limit'), font: cssVar('--font'), cap: parseFloat(cssVar('--fs-cap')) || 11 };
    vf.needs = true;
  };
  readVf();
  function drawVf() {
    const { w, h, dpr } = box;
    if (w < 40 || h < 40) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const off = app.draft.coreOffset;
    // Fast akse over hele det mulige området (standard + laveste til høyeste
    // offset), så du ser kurven løftes og senkes i stedet for at aksen flytter seg.
    const [oMin, oMax] = L.coreOffset;
    const ys = niceScale(VF[0][1] + oMin, VF[VF.length - 1][1] + oMax, 5);
    const xs = { lo: 700, hi: 1100, step: 50 };
    ctx.font = `${vfColors.cap}px ${vfColors.font}`;
    const padL = 44;
    const padB = vfColors.cap + 22;
    const padT = 8;
    const padR = 10;
    const pw = w - padL - padR;
    const ph = h - padT - padB;
    const X = (v) => padL + ((v - xs.lo) / (xs.hi - xs.lo)) * pw;
    const Y = (v) => padT + ph - ((v - ys.lo) / (ys.hi - ys.lo)) * ph;
    ctx.lineWidth = 1;
    ctx.fillStyle = vfColors.axis;
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    for (let v = ys.lo; v <= ys.hi + 0.1; v += ys.step) {
      const y = Math.round(Y(v)) + 0.5;
      ctx.strokeStyle = vfColors.grid;
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(w - padR, y);
      ctx.stroke();
      ctx.fillText(F.num(v), padL - 6, y);
    }
    ctx.textAlign = 'center';
    ctx.textBaseline = 'alphabetic';
    for (let v = xs.lo; v <= xs.hi; v += 100) {
      const x = Math.round(X(v)) + 0.5;
      ctx.strokeStyle = vfColors.grid;
      ctx.beginPath();
      ctx.moveTo(x, padT);
      ctx.lineTo(x, padT + ph);
      ctx.stroke();
      ctx.textAlign = v === xs.lo ? 'left' : v >= xs.hi ? 'right' : 'center';
      ctx.fillText(F.num(v), x, padT + ph + vfColors.cap + 4);
    }
    ctx.textAlign = 'center';
    ctx.fillText('Spenning (mV)', padL + pw / 2, h - 3);
    ctx.save();
    ctx.translate(10, padT + ph / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText('MHz', 0, 0);
    ctx.restore();
    const line = (pts, col, dash) => {
      ctx.strokeStyle = col;
      ctx.lineWidth = dash ? 1.25 : 2;
      ctx.setLineDash(dash ? [4, 4] : []);
      ctx.beginPath();
      pts.forEach(([v, f], i) => (i ? ctx.lineTo(X(v), Y(f)) : ctx.moveTo(X(v), Y(f))));
      ctx.lineTo(X(1100), Y(pts[pts.length - 1][1]));
      ctx.stroke();
      ctx.setLineDash([]);
    };
    line(VF, vfColors.ghost, true);
    if (off !== 0) line(VF.map(([v, f]) => [v, f + off]), vfColors.accent, false);
    else line(VF, vfColors.accent, false);
  }

  app.on('applied', () => {
    render();
    renderSlots();
  });
  app.on('draft', render);
  app.on('draft-local', renderDirty);
  app.on('conn', renderDirty);
  app.on('pending', renderDirty);
  app.on('clamped', render);
  app.on('daemon', () => {
    applyLimits();
    render();
    renderSafety();
  });
  app.on('settings', (k) => k.includes('confirmSec') && renderSafety());
  app.on('mode', renderMode);
  app.on('unit', () => {
    for (const k of KEYS) {
      const c = ctls[k];
      if (c.d.temp) {
        c.num.min = String(Math.round(F.tempOut(c.min)));
        c.num.max = String(Math.round(F.tempOut(c.max)));
      }
    }
    render();
    renderSlots();
  });
  app.on('theme', () => {
    readVf();
    charts.forEach((c) => c.readColors());
  });
  app.on('settings', (k) => {
    if (k.includes('chartFill') || k.includes('chartScale')) for (const c of charts) c.needs = true;
  });
  app.feed.on('sample', (smp) => {
    // Hviletilstanden følges også når siden er skjult, så den stemmer når du kommer tilbake.
    const d = app.draft;
    if (!visible) rest = restState(rest, smp.temp, { on: d.fanStartStop, start: d.fanStartTemp, stop: d.fanStopTemp });
    else renderAutoNote();
  });
  /** Nye grenser fra tjenesten (f.eks. når den kobles til etter at siden er lastet). */
  function applyLimits() {
    const L2 = app.state.limits;
    for (const k of KEYS) {
      const c = ctls[k];
      const [min, max] = L2[k] || [c.min, c.max];
      if (min === c.min && max === c.max) continue;
      c.min = min;
      c.max = max;
      c.range.min = String(min);
      c.range.max = String(max);
      if (c.stockMark) c.stockMark.style.setProperty('--at', String((stock[k] - min) / (max - min)));
      const disp = (v) => (c.d.temp ? Math.round(F.tempOut(v)) : v);
      c.num.min = String(disp(min));
      c.num.max = String(disp(max));
    }
  }

  render();
  renderSlots();
  renderMode();
  renderSafety();

  let visible = false;
  let stop = null;
  return {
    show() {
      visible = true;
      readVf();
      charts.forEach((c) => c.readColors());
      render();
      stop = onFrame(() => {
        if (vf.needs) {
          vf.needs = false;
          drawVf();
        }
        let busy = false;
        for (const c of charts) busy = c.frame() || busy;
        return busy;
      });
    },
    hide() {
      visible = false;
      stop?.();
      stop = null;
    },
    onKey(e, typing) {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        applyNow();
        return true;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !typing) {
        discard();
        return true;
      }
      return false;
    },
  };
}

/** Hvorfor Bruk er låst akkurat nå, eller null. */
export function waitReason(app) {
  if (app.offline) return 'Venter på serveren';
  if (app.daemon && !app.daemon.connected) return 'Venter på bakgrunnstjenesten';
  // Én ventende endring om gangen: en annen fane har trykket Bruk.
  if (app.confirm?.pending?.elsewhere) return 'Venter på Behold i en annen fane';
  return null;
}

/** Verktøytipset på Bruk når knappen ikke har noe å gjøre. */
export function applyIdleTip(app) {
  return app.confirm?.pending ? 'Endringen venter på Behold' : 'Ingen endringer å bruke';
}
