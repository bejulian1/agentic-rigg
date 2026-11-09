// Viftekurve (simulert): kurven fyller høyden, punktene kan redigeres både
// på canvas og i tabellen, og «Oppførsel» har minste fart, «senk først
// etter» (hvor mye temperaturen må falle før viften senkes), reaksjonstid og
// temperaturkilde. Kurven kan ikke falle. Temperatur, kurvens verdi og
// viftefart nå står i grafens etikett. Forkast og Bruk står i toppfeltet.
//
// Start og stopp (v0.0.6) står øverst i Oppførsel: under stopptemperaturen
// hviler viften på minste fart, og kurven tar over igjen ved
// starttemperaturen. Kortet har ikke viftestopp, så «stopp» er minste fart.
//
// v0.0.7: kurven kjøres av bakgrunnstjenesten når viftemodus er Kurve.
// Merket ved tittelen sier om den er i bruk, og «Bruk kurven» setter modusen
// i utkastet. Grafen viser nødregelen (full vifte over 85 °C) som en sone
// til høyre. Bruk gir samme bekreftelse som på Kontroller.

import { $, el, icon, onFrame, setText } from '../util.js';
import * as F from '../fmt.js';
import { FanCurveEditor, SS, clampStartStop, curveAt, fanBounds } from '../fancurve.js';
import { confirmDone, numField, segEl, setSeg, setTip, switchEl } from '../ui.js';
import { cpuTemp } from '../sensormeta.js';
import { applyIdleTip, sameCurve, waitReason } from './controls.js';
import { curveNickname } from '../easter.js';

const SS_FLOOR = 35; // minste viftefart på kortet

const PRESETS = {
  quiet: { name: 'Stille', pts: [[40, 35], [58, 38], [68, 46], [76, 60], [85, 82], [92, 100]] },
  balanced: { name: 'Balansert', pts: null }, // standardkurven fra serveren
  perf: { name: 'Ytelse', pts: [[30, 40], [45, 50], [58, 66], [68, 80], [76, 92], [84, 100]] },
};
// fanMode er med, så «Bruk kurven» og Bruk her tar modusen med seg.
const BEHAVIOR = ['fanMode', 'fanStartStop', 'fanStartTemp', 'fanStopTemp', 'fanMinPercent', 'fanHysteresis', 'fanRampUp', 'fanRampDown', 'fanSource'];

export function initFanCurve(app) {
  const { feed } = app;
  const L = app.state.limits;
  const editor = new FanCurveEditor($('#fan-canvas'), {
    onChange: (pts) => {
      app.draft.fanCurve = pts.map((p) => [...p]);
      renderSide();
      app.emit('draft-local');
    },
    // Start/stopp dratt eller flyttet med piltastene i grafen.
    onStartStop: ({ start, stop }) => {
      app.draft.fanStartTemp = start;
      app.draft.fanStopTemp = stop;
      renderSide();
      app.emit('draft-local');
    },
  });

  // ---- Oppførsel ----
  const beh = $('#fc-behavior');
  const bctl = {};

  // Start og stopp: bryter, og to tallfelt som glir fram når den er på.
  const ssSw = switchEl(app.draft.fanStartStop, (on) => set('fanStartStop', on), { label: 'Start og stopp' });
  const ssSub = el('small');
  const tOut = (c) => Math.round(F.tempOut(c));
  const ssField = (key, label) =>
    numField({
      min: tOut(SS[key === 'fanStartTemp' ? 'start' : 'stop'][0]),
      max: tOut(SS[key === 'fanStartTemp' ? 'start' : 'stop'][1]),
      step: 1,
      value: tOut(app.draft[key]),
      unit: F.tempLabel(),
      size: 'sm',
      label,
      onCommit: (v) => setStartStop(key, F.tempIn(v)),
    });
  const startNf = ssField('fanStartTemp', 'Starttemperatur');
  const stopNf = ssField('fanStopTemp', 'Stopptemperatur');
  const ssRow = el(
    'div',
    { class: 'small-row ss-row', 'data-open': String(!!app.draft.fanStartStop) },
    el('span', { class: 'k', 'data-tip': `Kortet har ikke viftestopp og tillater ikke under ${F.pct(SS_FLOOR)}. «Stopp» betyr at viften hviler på minste fart.` }, 'Start og stopp', ssSub),
    ssSw.root,
    el(
      'div',
      { class: 'collapse' },
      el(
        'div',
        { class: 'collapse-inner' },
        el('div', { class: 'ss-fields' }, el('span', { class: 'ss-f' }, el('span', { text: 'Start ved' }), startNf.root), el('span', { class: 'ss-f' }, el('span', { text: 'Stopp under' }), stopNf.root)),
      ),
    ),
  );
  beh.append(ssRow);

  /** Ny start- eller stopptemperatur fra tallfeltet. Den andre flyttes om avstanden blir for liten. */
  function setStartStop(key, c) {
    const fixed = key === 'fanStartTemp' ? 'start' : 'stop';
    const before = { start: app.draft.fanStartTemp, stop: app.draft.fanStopTemp };
    const want = { start: before.start, stop: before.stop, [fixed]: c };
    const next = clampStartStop(want.start, want.stop, fixed);
    app.draft.fanStartTemp = next.start;
    app.draft.fanStopTemp = next.stop;
    const other = fixed === 'start' ? 'stop' : 'start';
    if (next[other] !== before[other]) {
      app.toast(other === 'stop' ? `Stopp er flyttet til ${F.temp(next.stop)}` : `Start er flyttet til ${F.temp(next.start)}`, { body: `Start må ligge minst ${F.tempDelta(SS.gap)} over stopp.` });
    }
    renderSide();
    app.emit('draft-local');
  }
  const behRow = (key, title, sub, min, max, unit, toDisp = (v) => v, fromDisp = (v) => v) => {
    const nf = numField({ min: toDisp(min), max: toDisp(max), step: 1, value: toDisp(app.draft[key]), unit, size: 'sm', label: title, onInput: (v) => set(key, Math.min(max, Math.max(min, Math.round(fromDisp(v))))) });
    beh.append(el('div', { class: 'small-row' }, el('span', { class: 'k' }, title, el('small', { text: sub })), nf.root));
    bctl[key] = { nf, toDisp, min, max };
  };
  const dDisp = (v) => Math.round(F.fmtPrefs().unit === 'F' ? (v * 9) / 5 : v);
  const dIn = (v) => (F.fmtPrefs().unit === 'F' ? (v * 5) / 9 : v);
  behRow('fanMinPercent', 'Minste fart', 'Viften går aldri saktere med kurven', ...L.fanMinPercent, '%');
  behRow('fanHysteresis', 'Senk først etter', 'Temperaturfall før viften går ned', ...L.fanHysteresis, F.tempLabel(), dDisp, dIn);
  behRow('fanRampUp', 'Reaksjonstid opp', 'Sekunder til ny fart når det blir varmere', ...L.fanRampUp, 's');
  behRow('fanRampDown', 'Reaksjonstid ned', 'Sekunder til ny fart når det blir kaldere', ...L.fanRampDown, 's');
  const srcSeg = segEl(
    [
      ['gpu', 'GPU'],
      ['cpu', 'CPU'],
    ],
    app.draft.fanSource,
    (v) => set('fanSource', v),
    { label: 'Temperaturkilde', size: 'sm' },
  );
  beh.append(el('div', { class: 'small-row' }, el('span', { class: 'k', 'data-tip': 'Hotspot og minnetemperatur oppgis ikke av driveren.' }, 'Temperaturkilde', el('small', { text: 'Hotspot oppgis ikke av driveren' })), srcSeg));

  function set(k, v) {
    app.draft[k] = v;
    if (k === 'fanMinPercent') editor.minFan = v;
    if (k === 'fanSource') editor.srcLabel = v === 'cpu' ? 'CPU' : null;
    renderSide();
    app.emit('draft-local');
  }

  function srcTemp() {
    if (app.draft.fanSource === 'cpu') return cpuTemp(feed.sysLatest);
    return feed.latest?.temp ?? null;
  }

  function load({ animate = false } = {}) {
    editor.setPoints(app.draft.fanCurve, { animate });
    editor.setGhost(app.state.applied.fanCurve);
    editor.minFan = app.draft.fanMinPercent;
    editor.srcLabel = app.draft.fanSource === 'cpu' ? 'CPU' : null;
    renderSide();
  }

  function renderStartStop() {
    const d = app.draft;
    const on = !!d.fanStartStop;
    editor.setStartStop({ on, start: d.fanStartTemp, stop: d.fanStopTemp });
    ssSw.input.checked = on;
    ssRow.dataset.open = String(on);
    ssRow.querySelector('.collapse-inner').inert = !on;
    setText(ssSub, on ? `Hysterese ${F.tempDelta(d.fanStartTemp - d.fanStopTemp)}. Under ${F.temp(d.fanStopTemp)} holder viften ${F.pct(d.fanMinPercent)}.` : `Viften hviler på ${F.pct(d.fanMinPercent)} når det er kjølig`);
    for (const [nf, key, r] of [[startNf, 'fanStartTemp', SS.start], [stopNf, 'fanStopTemp', SS.stop]]) {
      nf.input.min = String(tOut(r[0]));
      nf.input.max = String(tOut(r[1]));
      if (document.activeElement !== nf.input) nf.input.value = String(tOut(d[key]));
      setText(nf.root.querySelector('.u'), F.tempLabel());
    }
    // Endringsprikk når utkastet avviker fra det som er brukt.
    const a = app.state.applied;
    ssRow.toggleAttribute('data-changed', d.fanStartStop !== a.fanStartStop || (on && (d.fanStartTemp !== a.fanStartTemp || d.fanStopTemp !== a.fanStopTemp)));
  }

  function isDirty() {
    const a = app.state.applied;
    const d = app.draft;
    const keys = BEHAVIOR.filter((k) => d[k] !== a[k]);
    return { curve: !sameCurve(d.fanCurve, a.fanCurve), keys, any: keys.length > 0 || !sameCurve(d.fanCurve, a.fanCurve) };
  }

  // ---- Punkttabell (redigerbar) ----
  const ptsTable = $('#fc-points');
  function renderPoints() {
    const pts = app.draft.fanCurve;
    const applied = app.state.applied.fanCurve;
    const active = document.activeElement;
    if (active && ptsTable.contains(active) && active.tagName === 'INPUT') return; // ikke tegn om mens du skriver
    ptsTable.replaceChildren(
      el('thead', {}, el('tr', {}, el('th', { text: '#' }), el('th', { 'data-align': 'end', text: `Temperatur (${F.tempLabel()})` }), el('th', { 'data-align': 'end', text: 'Vifte (%)' }), el('th', { text: '' }))),
      el(
        'tbody',
        {},
        ...pts.map(([t, f], i) => {
          const ti = el('input', { class: 'numfield-inline', type: 'number', step: '1', 'aria-label': `Temperatur for punkt ${i + 1}` });
          ti.value = String(Math.round(F.tempOut(t)));
          const [fLo, fHi] = fanBounds(pts, i);
          const fi = el('input', { class: 'numfield-inline', type: 'number', min: String(fLo), max: String(fHi), step: '1', 'aria-label': `Viftefart for punkt ${i + 1}` });
          fi.value = String(f);
          // Ugyldig tall gir rød kant mens du skriver, og verdien legges innenfor ved Enter/blur.
          const check = () => {
            const nt = Math.round(F.tempIn(Number(ti.value)));
            const nf = Number(fi.value);
            const lo = i > 0 ? pts[i - 1][0] + 1 : 20;
            const hi = i < pts.length - 1 ? pts[i + 1][0] - 1 : 100;
            ti.toggleAttribute('data-invalid', !Number.isFinite(nt) || nt < lo || nt > hi);
            fi.toggleAttribute('data-invalid', !Number.isFinite(nf) || nf < fLo || nf > fHi);
          };
          ti.addEventListener('input', check);
          fi.addEventListener('input', check);
          const commit = () => {
            const nt = Math.round(F.tempIn(Number(ti.value)));
            const nf = Math.round(Number(fi.value));
            const next = pts.map((p) => [...p]);
            const lo = i > 0 ? next[i - 1][0] + 1 : 20;
            const hi = i < next.length - 1 ? next[i + 1][0] - 1 : 100;
            if (!Number.isFinite(nt) || !Number.isFinite(nf) || nt < lo || nt > hi) {
              ti.value = String(Math.round(F.tempOut(t)));
              ti.removeAttribute('data-invalid');
              app.toast('Ugyldig temperatur', { body: `Punkt ${i + 1} må ligge mellom ${F.temp(lo)} og ${F.temp(hi)}.`, error: true });
              return;
            }
            if (nf < fLo || nf > fHi) app.toast('Viftefarten er justert', { body: `Punkt ${i + 1} må ligge mellom ${F.pct(fLo)} og ${F.pct(fHi)}, så kurven ikke faller.` });
            next[i] = [nt, Math.min(fHi, Math.max(fLo, nf))];
            app.draft.fanCurve = next;
            load();
            app.emit('draft-local');
          };
          ti.addEventListener('change', commit);
          fi.addEventListener('change', commit);
          const del = el('button', { type: 'button', class: 'btn sm flat icon-only', 'data-tip': pts.length > 2 ? 'Fjern punktet' : 'Kurven må ha minst to punkter', 'aria-label': `Fjern punkt ${i + 1}`, disabled: pts.length <= 2 || null }, icon('trash-2'));
          del.addEventListener('click', () => {
            app.draft.fanCurve = pts.filter((_, j) => j !== i).map((p) => [...p]);
            load();
            app.emit('draft-local');
          });
          // Endringsprikk på punkter som ikke er brukt ennå (samme som på Kontroller).
          const changed = !applied.some((p) => p[0] === t && p[1] === f);
          const num = el('td', { class: 'muted pt-n' }, el('span', { class: 'changed-dot', 'data-tip': 'Endret, ikke brukt' }), String(i + 1));
          return el('tr', { 'data-changed': changed ? '' : null }, num, el('td', { 'data-align': 'end' }, ti), el('td', { 'data-align': 'end' }, fi), el('td', { 'data-align': 'end' }, del));
        }),
      ),
    );
    setText($('#fc-pts-count'), `${pts.length} av 10`);
    const add = $('#fc-add');
    add.disabled = pts.length >= 10;
    setTip(add, pts.length >= 10 ? 'Kurven kan ha høyst 10 punkter' : 'Legg til et punkt i det største hullet');
  }

  $('#fc-add').addEventListener('click', () => {
    const pts = app.draft.fanCurve;
    if (pts.length >= 10) return;
    let best = 0;
    let gap = -1;
    for (let i = 0; i < pts.length - 1; i++) {
      const g = pts[i + 1][0] - pts[i][0];
      if (g > gap) {
        gap = g;
        best = i;
      }
    }
    if (gap < 2) return;
    const t = Math.round((pts[best][0] + pts[best + 1][0]) / 2);
    const f = Math.round(curveAt(pts, t));
    app.draft.fanCurve = [...pts.slice(0, best + 1), [t, f], ...pts.slice(best + 1)].map((p) => [...p]);
    load();
    app.emit('draft-local');
  });

  function renderSide() {
    renderPoints();
    renderStartStop();
    for (const [k, c] of Object.entries(bctl)) {
      // Grensene følger enheten (hysterese 10 °C = 18 °F), ellers klemmes verdien feil i °F.
      c.nf.input.min = String(c.toDisp(c.min));
      c.nf.input.max = String(c.toDisp(c.max));
      if (document.activeElement !== c.nf.input) c.nf.input.value = String(c.toDisp(app.draft[k]));
    }
    setSeg(srcSeg, app.draft.fanSource);
    const hu = bctl.fanHysteresis?.nf.root.querySelector('.u');
    if (hu) setText(hu, F.tempLabel());
    // Navnet på forhåndsvalget hvis kurven er lik et.
    const match = Object.entries(PRESETS).find(([, p]) => sameCurve(p.pts || app.state.defaultCurve, app.draft.fanCurve));
    // Påskeegg (easter.js): en kurve som står helt på 100 % (eller 35 %) får et kallenavn.
    const nick = match ? null : curveNickname(app.draft.fanCurve);
    setText($('#fc-title'), match ? `Kurve · ${match[1].name}` : `Kurve · ${nick ? nick.name : 'Egendefinert'}`);
    setTip($('#fc-title'), nick?.tip || null);
    for (const b of document.querySelectorAll('#fc-presets [data-p]')) b.setAttribute('aria-pressed', String(match?.[0] === b.dataset.p));
    const st = isDirty();
    const n = st.keys.length + (st.curve ? 1 : 0);
    $('#fc-dirty').dataset.state = st.any ? 'dirty' : 'clean';
    setText($('#fc-dirty-text'), st.any ? `${n} ${n === 1 ? 'endring' : 'endringer'} ikke brukt` : 'Ingen endringer');
    if (!st.any && app.confirm?.pending) {
      $('#fc-dirty').dataset.state = 'pending';
      setText($('#fc-dirty-text'), 'Venter på Behold');
    }
    for (const id of ['#fc-discard', '#fc-apply']) {
      const b = $(id);
      // Forkast virker uten server; Bruk venter til serveren og tjenesten er tilbake.
      const why = id === '#fc-apply' ? waitReason(app) : null;
      b.disabled = !st.any || !!why;
      setTip(b, why || (st.any ? (id === '#fc-apply' ? 'Bruk kurven (simulert)' : 'Forkast endringene') : id === '#fc-apply' ? applyIdleTip(app) : 'Ingen endringer å forkaste'));
    }
    renderChip();
  }

  /** Merket ved tittelen: brukes kurven nå? Og knappen som setter modusen til Kurve. */
  function renderChip() {
    const a = app.state.applied;
    const d = app.draft;
    const tag = $('#fc-mode-tag');
    const on = a.fanMode === 'curve';
    tag.textContent = on ? 'I bruk' : a.fanMode === 'manual' ? `Brukes ikke · Manuell ${F.pct(a.fanPercent)}` : 'Brukes ikke · Auto';
    tag.className = `tag${on ? ' accent' : ''}`;
    setTip(tag, on ? 'Viftemodus er Kurve: bakgrunnstjenesten følger kurven.' : a.fanMode === 'manual' ? `Viften står fast på ${F.pct(a.fanPercent)} (Kontroller › Vifte).` : 'Viftemodus er Auto: driveren styrer viften (Kontroller › Vifte).');
    $('#fc-use-curve').hidden = d.fanMode === 'curve';
  }
  $('#fc-use-curve').addEventListener('click', () => {
    app.draft.fanMode = 'curve';
    renderSide();
    app.emit('draft-local');
  });

  feed.on('sample', (s) => {
    editor.setLive(srcTemp(), s.fan);
    editor.setEmergency({ at: app.daemon?.safety?.emergencyFanC ?? 85, active: app.mode !== 'live' && !!s.emergency });
  });

  document.querySelector('#fc-presets').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-p]');
    if (!b) return;
    const pts = PRESETS[b.dataset.p].pts || app.state.defaultCurve;
    app.draft.fanCurve = pts.map((p) => [...p]);
    load({ animate: true });
    app.emit('draft-local');
  });

  function discard() {
    if (!isDirty().any) return;
    app.resetDraft(['fanCurve', ...BEHAVIOR]); // sender 'draft', som morfer kurven
  }

  // Dobbeltklikk eller Ctrl+Enter to ganger skal bare gi én lagring.
  let applying = false;
  async function apply() {
    if (applying || waitReason(app) || !isDirty().any) return;
    applying = true;
    const btn = $('#fc-apply');
    const hadFocus = document.activeElement === btn;
    btn.disabled = true;
    try {
      const patch = { fanCurve: app.draft.fanCurve };
      for (const k of BEHAVIOR) patch[k] = app.draft[k];
      const res = await app.api.apply(patch);
      app.afterApply(res, { focus: hadFocus });
      load();
      if (!res.pending) confirmDone(btn);
      if (res.applied.fanMode !== 'curve') app.toast('Kurven er lagret, men brukes ikke', { body: `Viftemodus er ${res.applied.fanMode === 'manual' ? 'Manuell' : 'Auto'}. Trykk «Bruk kurven» og Bruk for å la tjenesten følge den.` });
    } catch (err) {
      app.toast('Kunne ikke bruke kurven', { body: err.message, error: true });
    } finally {
      applying = false;
      renderSide();
    }
  }
  $('#fc-discard').addEventListener('click', discard);
  $('#fc-apply').addEventListener('click', apply);

  app.on('applied', () => {
    renderChip();
    editor.setGhost(app.state.applied.fanCurve);
    renderSide();
  });
  app.on('unit', () => {
    renderSide();
    renderPoints();
  });
  app.on('theme', () => editor.readColors());
  app.on('draft', () => load({ animate: visible }));
  app.on('draft-local', () => visible && renderSide());
  app.on('conn', renderSide);
  app.on('settings', (keys) => keys.includes('surprises') && renderSide());
  app.on('pending', renderSide);
  app.on('daemon', renderSide);
  load();
  renderChip();

  let visible = false;
  let stop = null;
  return {
    show() {
      visible = true;
      load();
      editor.readColors();
      stop = onFrame((dt) => editor.tick(dt));
    },
    hide() {
      visible = false;
      stop?.();
      stop = null;
    },
    onKey(e, typing) {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        apply();
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
