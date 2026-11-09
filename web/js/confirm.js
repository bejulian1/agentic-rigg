// Bekreft eller rull tilbake (v0.0.7). Etter Bruk på Kontroller eller
// Viftekurve gjelder endringene bare midlertidig: linja under toppfeltet
// spør «Behold endringene?» og teller ned. Uten Behold innen fristen ruller
// bakgrunnstjenesten tilbake selv. Det samme skjer når fanen lukkes eller
// lastes på nytt, og når forbindelsen til tjenesten brytes.
//
// Detaljer:
//   - Fokus flyttes ikke, bortsett fra når knappen du trykket (Bruk), ble
//     låst: da går fokus til Behold, så tastaturet ikke havner i ingenting.
//   - Skjermlesere får én melding når nedtellingen starter og én når den er
//     ferdig. Sekundene står i et aria-hidden-felt.
//   - Ctrl+Shift+Enter er Behold overalt i appen.
//   - Linja krymper med transform (seksjon 8 i styles.css). Ved «Av» hopper
//     den hvert sekund, og ingen animasjon kjører.
//   - Er en verdi klemt av tjenesten, står det i linja (og ved kontrollen).
//   - Flere faner (v0.0.8): alle faner deler den samme ventende endringen og
//     viser samme linje. Behold og Rull tilbake nå virker fra alle, men bare
//     fanen som trykket Bruk, kan bruke mer før den er avgjort. I de andre
//     står «i en annen fane», og Bruk er låst til endringen er avgjort.

import { $ } from './util.js';
import { TAB } from './api.js';
import * as F from './fmt.js';
import { announce, setTip, toast } from './ui.js';
import { LABEL, valText, waitReason } from './pages/controls.js';

/** Hvorfor endringene ble rullet tilbake. */
export const REASON = {
  timeout: 'Fristen gikk ut.',
  revert: 'Du valgte å rulle tilbake.',
  tab_closed: 'Siden ble lukket eller lastet på nytt før du trykket Behold.',
  tab_closed_other: 'Fanen som trykket Bruk, ble lukket eller lastet på nytt.',
  client_disconnected: 'Forbindelsen til bakgrunnstjenesten brøt.',
  daemon_stopping: 'Bakgrunnstjenesten stoppet.',
  reset: 'Alt ble tilbakestilt til standard.',
};

const lc = (t) => (t ? t.charAt(0).toLowerCase() + t.slice(1) : t);

/** «Effektgrense», «Effektgrense og viftemodus», «Effektgrense, kjerneklokke-offset og viftemodus», «Effektgrense, kjerneklokke-offset og 2 andre». */
export function changedText(keys) {
  const names = keys.map((k) => (k === 'fanCurve' ? 'Viftekurve' : LABEL[k] || k));
  if (!names.length) return 'Endringene';
  if (names.length === 1) return names[0];
  if (names.length === 2) return `${names[0]} og ${lc(names[1])}`;
  // Tre navn får plass; «og 1 til» sier mindre enn navnet selv.
  if (names.length === 3) return `${names[0]}, ${lc(names[1])} og ${lc(names[2])}`;
  return `${names[0]}, ${lc(names[1])} og ${names.length - 2} andre`;
}

/** «Effektgrense: 300 W er over kortets maks. Tjenesten bruker 275 W.» */
export function clampText(c) {
  if (c.key === 'fanCurve') return `Viftekurve: ${F.plural(c.points?.length || 1, 'punkt', 'punkter')} er løftet, så kurven ikke går under ${F.pct(35)} eller faller.`;
  const name = LABEL[c.key] || c.key;
  const whose = c.source === 'policy' ? 'tjenestens' : 'kortets';
  const bound = c.bound === 'max' ? 'maks' : 'minimum';
  return `${name}: ${valText(c.key, c.requested)} er ${c.bound === 'max' ? 'over' : 'under'} ${whose} ${bound}. Tjenesten bruker ${valText(c.key, c.applied)}.`;
}

export function initConfirm(app) {
  const bar = $('#confirm-bar');
  const title = $('#cb-title');
  const body = $('#cb-body');
  const clampEl = $('#cb-clamp');
  const count = $('#cb-count');
  const fill = $('#cb-bar');
  const iconUse = bar.querySelector('.cb-icon use');
  const btn = { keep: $('#cb-keep'), revert: $('#cb-revert'), again: $('#cb-again'), close: $('#cb-close') };
  let cur = null; // { txn, deadline, total, changed, clamped }
  let result = null; // { txn, reason, values }
  let ticker = null;
  let hideTimer = null;
  let busy = false;
  let keptTxn = null; // Behold trykket her (så en annen fane kan si fra når den beholder)

  const left = () => Math.max(0, Math.ceil((cur.deadline - Date.now()) / 1000));

  function tick() {
    if (!cur) return;
    const s = left();
    count.textContent = s > 0 ? `Ruller tilbake om ${s}${F.NNBSP}s` : 'Ruller tilbake …';
    // Linja viser hvor mye som er igjen ett sekund fram, så den glir jevnt mot neste tall.
    const k = Math.max(0, Math.min(1, (cur.deadline - Date.now() - 1000) / cur.total));
    fill.style.transform = `scaleX(${k})`;
  }

  function setButtons(state) {
    const p = state === 'pending';
    btn.keep.hidden = !p;
    btn.revert.hidden = !p;
    btn.again.hidden = p || !result?.values;
    btn.close.hidden = p;
    count.hidden = !p;
    renderAgain();
  }

  /** Bruk igjen er låst med grunn når serveren eller tjenesten er borte. */
  function renderAgain() {
    const why = waitReason(app);
    btn.again.disabled = !!why;
    setTip(btn.again, why || 'Ta de samme verdiene i bruk igjen');
  }

  function hide() {
    clearInterval(ticker);
    clearTimeout(hideTimer);
    ticker = null;
    const hadFocus = bar.contains(document.activeElement);
    bar.hidden = true;
    cur = null;
    result = null;
    if (hadFocus) $('#content')?.focus({ preventScroll: true });
    app.emit('pending', null);
  }

  /**
   * Viser nedtellingen for en ventende endring (fra svaret på Bruk eller fra
   * SSE). Samme txn to ganger gir ingen ny melding.
   */
  function showPending(p, { focus = false } = {}) {
    if (!p) return;
    const same = cur && cur.txn === p.txn;
    const deadline = Date.now() + p.remainingMs;
    // Svaret på Bruk og SSE-hendelsen kommer i vilkårlig rekkefølge; samme txn tegnes én gang.
    if (same && Math.abs(deadline - cur.deadline) < 1200 && cur.elsewhere === (p.tab !== TAB) && JSON.stringify(cur.changed) === JSON.stringify(p.changed) && JSON.stringify(cur.clamped) === JSON.stringify(p.clamped)) {
      if (focus) btn.keep.focus({ preventScroll: true });
      return;
    }
    clearTimeout(hideTimer);
    result = null;
    // Fanen som trykket Bruk, eier endringen; de andre viser den, men kan ikke bruke mer.
    const elsewhere = p.tab !== TAB;
    cur = { txn: p.txn, deadline, total: p.timeoutS * 1000, changed: p.changed || [], clamped: p.clamped || [], elsewhere };
    bar.dataset.state = 'pending';
    iconUse.setAttribute('href', '#i-timer');
    title.textContent = 'Behold endringene?';
    const what = changedText(cur.changed);
    body.textContent = elsewhere ? `${what} er tatt i bruk i en annen fane.` : `${what} er tatt i bruk.`;
    clampEl.hidden = !cur.clamped.length;
    clampEl.textContent = cur.clamped.map(clampText).join(' ');
    setButtons('pending');
    // Linja starter full og uten overgang (ny txn eller ny frist).
    fill.style.transition = 'none';
    fill.style.transform = 'scaleX(1)';
    void fill.offsetWidth;
    fill.style.transition = '';
    bar.hidden = false;
    tick();
    clearInterval(ticker);
    // Tikk på hele sekunder fra fristen, så tallet bytter jevnt.
    const align = (cur.deadline - Date.now()) % 1000;
    ticker = setTimeout(() => {
      tick();
      ticker = setInterval(tick, 1000);
    }, align || 1000);
    if (!same) announce(`${what} er tatt i bruk. Trykk Behold innen ${p.timeoutS} sekunder, ellers rulles endringene tilbake.${cur.clamped.length ? ' Noen verdier er klemt til kortets grenser.' : ''}`);
    if (focus) btn.keep.focus({ preventScroll: true });
    app.emit('pending', cur);
  }

  /** Etter tilbakerulling: nøytral linje med grunnen, Bruk igjen og ×. */
  function showResult(r) {
    clearInterval(ticker);
    clearTimeout(ticker);
    ticker = null;
    cur = null;
    result = r;
    bar.dataset.state = 'result';
    iconUse.setAttribute('href', '#i-undo-2');
    title.textContent = 'Endringene er rullet tilbake.';
    body.textContent = `${REASON[r.reason] || ''} Verdiene fra før er i bruk igjen.`.trim();
    clampEl.hidden = true;
    setButtons('result');
    bar.hidden = false;
    announce(`Endringene er rullet tilbake. ${REASON[r.reason] || ''}`);
    app.emit('pending', null);
    // Blir stående en stund, men ikke for alltid. Ikke mens du er inni den.
    clearTimeout(hideTimer);
    hideTimer = setTimeout(function later() {
      if (bar.contains(document.activeElement) || bar.matches(':hover')) hideTimer = setTimeout(later, 3000);
      else if (result === r) hide();
    }, 20_000);
  }

  async function keep() {
    if (!cur || busy) return;
    busy = true;
    const txn = cur.txn;
    keptTxn = txn;
    try {
      const r = await app.api.confirm(txn);
      app.setApplied(r.applied);
      hide();
      toast('Endringene er beholdt', { body: 'Bakgrunnstjenesten bruker dem videre (simulert).' });
    } catch (err) {
      toast('Kunne ikke beholde endringene', { body: err.message, error: true });
    } finally {
      busy = false;
    }
  }

  async function revertNow() {
    if (!cur || busy) return;
    busy = true;
    try {
      await app.api.revert(cur.txn);
      // Svaret fra SSE (daemon) viser resultatet; her bare hvis det ikke kommer.
    } catch (err) {
      toast('Kunne ikke rulle tilbake', { body: err.message, error: true });
    } finally {
      busy = false;
    }
  }

  async function again() {
    const v = result?.values;
    if (!v) return;
    hide();
    await app.applyValues(v, { focus: true });
  }

  btn.keep.addEventListener('click', keep);
  btn.revert.addEventListener('click', revertNow);
  btn.again.addEventListener('click', again);
  btn.close.addEventListener('click', hide);

  /** Ny tilstand fra serveren (SSE 'daemon' eller /api/state). */
  function sync(d) {
    if (!d) return;
    renderAgain();
    if (d.pending) return showPending(d.pending);
    if (cur) {
      // Ventet, og gjør det ikke lenger: bekreftet (her eller i en annen fane) eller rullet tilbake.
      const r = d.recent && d.recent.txn === cur.txn ? d.recent : null;
      if (r?.status === 'rolled_back') return showResult(cur.elsewhere && r.reason === 'tab_closed' ? { ...r, reason: 'tab_closed_other' } : r);
      // Beholdt i en annen fane: si det, så linja ikke bare forsvinner.
      if (r?.status === 'confirmed' && keptTxn !== cur.txn) {
        hide();
        return toast('Endringene er beholdt', { body: 'Det ble gjort i en annen fane.' });
      }
      if (!d.connected && !r) return showResult({ txn: cur.txn, reason: 'client_disconnected', values: null });
      return hide();
    }
  }

  app.on('conn', renderAgain);

  return {
    showPending,
    showResult,
    sync,
    keep,
    hide,
    get pending() {
      return cur;
    },
  };
}
