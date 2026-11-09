// Lys (RGB, simulert): forhåndsvisningen i full høyde og en smal kolonne
// til høyre med effekt, farge og (i 3D) delene på kortet.
// Ingenting sendes til lyskontrolleren.
//
// v0.0.13: kortet vises i 3D (lys3d/view.js, WebGL 2) og kan vris, tas fra
// hverandre og skilles i lag. Det flate SVG-kortet (rgb.js, SvgCard) er
// reserven: uten WebGL, ved programvaregrafikk, når grafikken tegner for
// tregt, og når du velger 2D. Fargene regnes av den samme motoren
// (RgbEngine) i begge, så effekter, soner, bakgrunnslys og påskeegget virker
// likt. 3D-koden lastes og lages først når Lys åpnes, og frigjøres 20 s
// etter at du har gått til en annen side.

import { $, el, icon, onFrame, setText, wake, local, motion } from '../util.js';
import * as F from '../fmt.js';
import { announce, bindSeg, confirmDone, fillRange, numField, segEl, setSeg, setTip } from '../ui.js';
import { RgbEngine, SvgCard } from '../rgb.js';
import { createPicker } from '../colorpicker.js';
import { onMotion } from '../prefs.js';
import { aurora } from '../easter.js';

const EFFECTS = [
  ['static', 'Statisk', true, 'Én fast farge.'],
  ['breathe', 'Pust', true, 'Fargen tones rolig opp og ned.'],
  ['rainbow', 'Regnbue', false, 'Alle farger løper rundt.'],
  ['cycle', 'Fargesyklus', false, 'Hele kortet bytter farge sakte.'],
  ['temperature', 'Temperatur', false, 'Fargen følger GPU-temperaturen, fra kald til varm.'],
  ['off', 'Av', true, 'Lysene er slått av.'],
];
const NAME = Object.fromEntries(EFFECTS.map((e) => [e[0], e[1]]));
const ZONES = { all: 'Alle soner', bar: 'Lyslist', fan1: 'Vifte 1', fan2: 'Vifte 2' };
const KEYS = ['effect', 'brightness', 'speed', 'direction', 'tempCold', 'tempHot', 'tempMin', 'tempMax', 'ambient'];
const STEPS = ['Hel', 'Uten deksel', 'Uten kjøler', 'Bare kretskort'];
const RELEASE_MS = 20_000;

/** Kan nettleseren tegne 3D, og er det på en ekte GPU? Spørres én gang. */
let glInfo = null;
function probeGl() {
  if (glInfo) return glInfo;
  glInfo = { ok: false, soft: false, renderer: '' };
  try {
    const g = document.createElement('canvas').getContext('webgl2');
    if (g) {
      const d = g.getExtension('WEBGL_debug_renderer_info');
      const r = String((d && g.getParameter(d.UNMASKED_RENDERER_WEBGL)) || g.getParameter(g.RENDERER) || '');
      glInfo = { ok: true, soft: /swiftshader|llvmpipe|softpipe|software|basic render/i.test(r), renderer: r };
      g.getExtension('WEBGL_lose_context')?.loseContext();
    }
  } catch {}
  return glInfo;
}

export function initLights(app) {
  const page = $('#page-lys');
  const stage = $('#rgb-stage');
  const card = $('#rgb-card');
  const viewHost = $('#rgb-view');
  const label = app.name.split(' ')[0].toUpperCase();
  const engine = new RgbEngine();
  // Tegner valgt 3D for tregt, står effektene stille (som ved «Redusert»).
  engine.motion = () => (view?.slow && motion() === 'full' ? 'reduced' : motion());
  const clone = (r) => ({ ...r, zones: { ...r.zones }, recent: [...(r.recent || [])] });
  let draft = clone(app.state.rgb);
  let zone = local.get('ag-rgb-zone', 'all');
  let tempTarget = 'cold';
  let egg = null; // påskeegg over forhåndsvisningen (easter.js), se setEgg()
  let lastFan = null;
  let visible = false;
  let stop = null;

  // ---- 2D eller 3D ----
  // Valget huskes i nettleseren: 'auto' (3D på ekte GPU), '3d' eller '2d'.
  let pref = local.get('ag-rgb-view', 'auto');
  let tooSlow = false; // 3D tegnet for tregt i denne økta (automatisk)
  let mode = null; // det som er bygd: '2d' | '3d' | null
  let svg = null;
  let view = null;
  let viewMod = null;
  let building = null;
  let releaseTimer = 0;
  // Kamera og lag huskes når 3D frigjøres og lages på nytt.
  let cam3d = null;

  function wanted() {
    const gl = probeGl();
    if (pref === '2d' || !gl.ok) return '2d';
    if (pref === '3d') return '3d';
    return gl.soft || tooSlow ? '2d' : '3d';
  }

  function buildSvg() {
    if (svg) return;
    card.hidden = false;
    svg = new SvgCard(card, stage, label);
    fitCard(stage.clientWidth, stage.clientHeight);
    svg.setFan(lastFan);
    svg.setRunning(visible);
    engine.setTarget(svg);
  }
  function dropSvg() {
    svg?.destroy();
    svg = null;
    card.hidden = true;
  }

  function saveCam() {
    if (!view) return;
    cam3d = { yaw: view.goal.yaw, pitch: view.goal.pitch, zoom: view.goal.zoom, step: view.step, explode: view.explodeGoal };
  }
  function drop3d() {
    clearTimeout(releaseTimer);
    if (!view) return;
    saveCam();
    hideTip();
    view.destroy();
    view = null;
    viewHost.hidden = true;
  }

  const loading = $('#rgb-loading');
  async function build3d() {
    loading.hidden = false;
    viewMod ||= await import('../lys3d/view.js');
    if (view || wanted() !== '3d') {
      loading.hidden = true;
      return;
    }
    viewHost.hidden = false;
    try {
      view = new viewMod.CardView(viewHost, {
        motion,
        wake,
        onHover: showTip,
        onZone: (z) => {
          if (zoneLocked()) return;
          pickZone(z);
          announce(`Sone: ${ZONES[z]}`);
        },
        onPart: (info) => togglePinned(info.id),
        onChange: describeSoon,
        onSlow: () => {
          if (pref === '3d') {
            // Du valgte 3D selv: behold det, men som stillbilde.
            engine.dirty = true;
            syncNotes();
            return;
          }
          tooSlow = true;
          ensureRenderer().then(() => app.toast('Lys vises i 2D', { body: 'Grafikken tegnet 3D-kortet for tregt. Velg 3D over forhåndsvisningen for å prøve igjen.' }));
        },
      });
    } catch (err) {
      console.warn('3D-visningen kunne ikke startes, viser 2D:', err);
      loading.hidden = true;
      view = null;
      viewHost.hidden = true;
      glInfo = { ok: false, soft: false, renderer: '' };
      return;
    }
    themeColors();
    view.setFanSpeed(lastFan ?? 35);
    if (cam3d) {
      view.setCamera({ yaw: cam3d.yaw, pitch: cam3d.pitch, zoom: cam3d.zoom });
      if (cam3d.step) {
        // Rett til trinnet, uten animasjon.
        view.setStep(cam3d.step);
        for (const s of Object.values(view.layerState)) s.out = s.goal;
      }
      view.setExplode(cam3d.explode, { follow: true });
      view.fit(true);
    }
    view.setAmbientEnabled(draft.ambient);
    engine.setTarget(view);
    view.setRunning(visible);
  }

  /** Bygger det som skal vises (2D eller 3D) og river det andre. */
  async function ensureRenderer() {
    if (building) return building;
    building = (async () => {
      const want = wanted();
      if (want === '3d') {
        if (!view) await build3d();
        if (view) dropSvg();
        else buildSvg();
      } else {
        drop3d();
        buildSvg();
      }
      mode = view ? '3d' : '2d';
      page.dataset.mode = mode;
      renderMode();
      renderParts();
      render();
      describeSoon();
    })();
    try {
      await building;
    } finally {
      building = null;
    }
  }

  // Kortet (660 × 280) skaleres inn i scenen med luft rundt (2D). Målt med
  // ResizeObserver, ikke med cqh: container-enheter ga 0 × 0 når scenen fikk
  // høyden fra min-height (smale vinduer, lave vinduer).
  const fitCard = (w, h) => {
    const cw = Math.max(0, Math.min(w - 48, ((h - 32) * 660) / 280));
    card.style.width = `${cw}px`;
    card.style.height = `${(cw * 280) / 660}px`;
  };
  new ResizeObserver(([e]) => fitCard(e.contentRect.width, e.contentRect.height)).observe(stage);

  // Scenens farger følger temaet (bakgrunn og fremheving i 3D).
  function themeColors() {
    if (!view) return;
    const cs = getComputedStyle(document.documentElement);
    const hex = (n, d) => {
      const v = cs.getPropertyValue(n).trim();
      return /^#[0-9a-f]{6}$/i.test(v) ? v : d;
    };
    const st = hex('--stage', '#16181b');
    view.bg = [1, 3, 5].map((i) => parseInt(st.slice(i, i + 2), 16) / 255);
    const ac = hex('--stage-focus', '#559cf7');
    view.accent = [1, 3, 5].map((i) => {
      const v = parseInt(ac.slice(i, i + 2), 16) / 255;
      return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    });
    view.dirty = true;
    view.kick();
  }
  new MutationObserver(() => themeColors()).observe(document.documentElement, { attributes: true, attributeFilter: ['data-resolved', 'data-palette', 'data-contrast', 'data-accent'] });

  // ---- Status fra serveren: kort merke, detaljen i (?) ----
  const st = app.state.rgbStatus || {};
  setTip($('#rgb-status-info'), [st.text ? `${st.text}.` : 'Lyskontroller: ikke koblet til (simulert).', 'Endringer vises bare i forhåndsvisningen.', st.detail || ''].filter(Boolean).join(' '));

  // ---- Visning: 2D/3D, vinkel, tilbakestill, lag, skill lagene ----
  const modeSeg = $('#rgb-mode');
  bindSeg(modeSeg, (v) => {
    pref = v;
    local.set('ag-rgb-view', v);
    if (v === '3d') tooSlow = false;
    ensureRenderer();
  });
  function renderMode() {
    const gl = probeGl();
    setSeg(modeSeg, mode || wanted());
    const b3 = modeSeg.querySelector('[data-v="3d"]');
    b3.disabled = !gl.ok;
    setTip(b3, !gl.ok ? 'Krever WebGL 2. Nettleseren eller grafikkdriveren har det ikke slått på.' : gl.soft ? 'Programvaregrafikk: 3D kan gå tregt. 2D er valgt automatisk.' : tooSlow ? 'Grafikken tegnet 3D for tregt. Klikk for å prøve igjen.' : 'Kortet i 3D. Dra for å vri.');
    setTip(modeSeg.querySelector('[data-v="2d"]'), 'Flatt kort sett forfra. Bruker minst.');
  }
  bindSeg($('#rgb-angle'), (v) => {
    view?.setView(v);
    describeSoon();
  });
  $('#rgb-reset-view').addEventListener('click', () => {
    view?.resetView();
    describeSoon();
  });
  bindSeg($('#rgb-step'), (v) => {
    const n = Number(v);
    view?.setStep(n);
    setSeg($('#rgb-step'), n);
    renderParts();
    announce(`${STEPS[n]}.`);
  });
  const explode = $('#rgb-explode');
  explode.addEventListener('input', () => {
    view?.setExplode(Number(explode.value) / 100, { follow: true });
    renderExplode();
  });
  function renderExplode() {
    const v = Math.round((view?.explodeGoal ?? 0) * 100);
    explode.value = String(v);
    fillRange(explode);
    setText($('#rgb-explode-out'), `${v} %`);
  }

  // Delen under pekeren (3D).
  const tip = $('#rgb-tip');
  function hideTip() {
    tip.removeAttribute('data-on');
  }
  function showTip(info, x, y) {
    if (!info) return hideTip();
    tip.replaceChildren(el('b', { text: info.name }), el('span', { text: info.desc }));
    if (info.zone) tip.append(el('em', { text: zoneLocked() ? 'Sonen kan ikke velges med denne effekten.' : 'Klikk for å velge sonen.' }));
    const w = viewHost.clientWidth;
    const h = viewHost.clientHeight;
    const px = Math.min(w - tip.offsetWidth - 8, x + 14);
    const py = y + 18 + tip.offsetHeight > h - 8 ? y - tip.offsetHeight - 12 : y + 18;
    tip.style.transform = `translate(${Math.max(8, px)}px, ${Math.max(8, py)}px)`;
    tip.setAttribute('data-on', '');
  }

  // Dellisten (3D): peking eller fokus fremhever delen, klikk fester den.
  let pinned = null;
  function togglePinned(id) {
    pinned = pinned === id ? null : id;
    view?.setFocusGroup(pinned);
    renderParts();
  }
  function renderParts() {
    const ul = $('#rgb-parts');
    if (!view) {
      ul.replaceChildren();
      setText($('#rgb-parts-count'), '');
      return;
    }
    const list = view.visibleGroups();
    if (pinned && !list.some((g) => g.id === pinned)) {
      pinned = null;
      view.setFocusGroup(null);
    }
    ul.replaceChildren(
      ...list.map((g) => {
        const b = el('button', { type: 'button', class: 'part', 'aria-pressed': String(pinned === g.id), 'data-g': g.id }, el('span', { class: 'n', text: g.name }), el('span', { class: 'd', text: g.desc }));
        const on = () => view?.setFocusGroup(g.id);
        const off = () => view?.setFocusGroup(pinned);
        b.addEventListener('mouseenter', on);
        b.addEventListener('focus', on);
        b.addEventListener('mouseleave', off);
        b.addEventListener('blur', off);
        b.addEventListener('click', () => togglePinned(g.id));
        return el('li', {}, b);
      }),
    );
    // Ett tabulatorstopp for hele lista; piltaster, Home og End flytter.
    const btns = [...ul.querySelectorAll('.part')];
    const cur = btns.find((b) => b.dataset.g === pinned) || btns[0];
    for (const b of btns) b.tabIndex = b === cur ? 0 : -1;
    setText($('#rgb-parts-count'), `${list.length} synlige`);
  }
  $('#rgb-parts').addEventListener('keydown', (e) => {
    const btns = [...e.currentTarget.querySelectorAll('.part')];
    const i = btns.indexOf(document.activeElement);
    if (i < 0) return;
    const n = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: btns.length - 1 }[e.key];
    if (n == null) return;
    e.preventDefault();
    const b = btns[Math.max(0, Math.min(btns.length - 1, n))];
    for (const x of btns) x.tabIndex = x === b ? 0 : -1;
    b.focus();
  });

  // Beskrivelse for skjermlesere og bildeteksten på scenen.
  let descT = 0;
  function describeSoon() {
    clearTimeout(descT);
    descT = setTimeout(() => {
      const lys = egg ? 'Lys: nordlys (bare forhåndsvisning).' : draft.effect === 'off' ? 'Lysene er av.' : `Lys: ${NAME[draft.effect]}, ${draft.brightness} % lysstyrke.`;
      if (view) {
        setText($('#rgb-view-desc'), `${view.describe()} ${lys}`);
        setText($('#rgb-caption'), `${STEPS[view.step]}${view.explodeGoal > 0.02 ? ' · lagene skilt' : ''}`);
        const v = Object.entries(viewMod.VIEWS).find(([, w]) => Math.abs(viewMod.wrap(view.goal.yaw - w.yaw)) < 0.5 && Math.abs(view.goal.pitch - w.pitch) < 0.5);
        setSeg($('#rgb-angle'), v ? v[0] : '');
        setSeg($('#rgb-step'), view.step);
        renderExplode();
      } else setText($('#rgb-view-desc'), `Flatt skjermkort sett forfra. ${lys}`);
    }, 120);
  }

  // ---- Soner og bakgrunnslys ----
  const zoneSeg = $('#rgb-zone');
  function pickZone(v) {
    if (egg) setEgg(null);
    zone = v;
    local.set('ag-rgb-zone', v);
    render();
  }
  bindSeg(zoneSeg, pickZone);
  const amb = $('#rgb-ambient');
  amb.addEventListener('change', () => set({ ambient: amb.checked }));

  // ---- Effekter ----
  const effHost = $('#rgb-effects');
  const effRows = EFFECTS.map(([id, name, ok, desc]) => {
    const radio = el('input', { type: 'radio', class: 'radio', name: 'rgb-effect', value: id, 'aria-label': name });
    radio.addEventListener('change', () => set({ effect: id }));
    const sample = el('span', { class: 'sample', 'data-fx': id });
    const row = el(
      'label',
      { class: 'effect', 'data-tip': desc },
      radio,
      sample,
      el('span', { class: 'fx-name', text: name }),
      // Bare unntakene merkes: et lite øye for effekter som bare finnes i forhåndsvisningen.
      ok ? el('span') : el('span', { class: 'fx-preview', 'data-tip': 'Bare forhåndsvisning. Ikke dokumentert for kontrolleren.', 'aria-label': 'bare forhåndsvisning' }, icon('eye')),
    );
    effHost.append(row);
    return { id, radio, sample };
  });

  // ---- Farge ----
  const picker = createPicker($('#rgb-picker'), {
    recent: () => draft.recent || [],
    onChange: (hex) => {
      if (draft.effect === 'temperature') set({ [tempTarget === 'cold' ? 'tempCold' : 'tempHot']: hex });
      else if (zone === 'all') set({ color: hex, zones: { bar: hex, fan1: hex, fan2: hex } });
      else set({ zones: { ...draft.zones, [zone]: hex }, color: zone === 'bar' ? hex : draft.color });
    },
  });
  const tempSeg = segEl(
    [
      ['cold', 'Kald'],
      ['hot', 'Varm'],
    ],
    tempTarget,
    (v) => {
      tempTarget = v;
      render();
    },
    { label: 'Hvilken farge', size: 'sm' },
  );
  $('#rgb-color-aside').append(tempSeg);

  // ---- Justering ----
  const adj = $('#rgb-adjust');
  const rangeRow = (key, label, min, max, unit) => {
    const range = el('input', { type: 'range', min, max, step: '1', 'aria-label': label });
    const nf = numField({ min, max, step: 1, value: draft[key], unit, size: 'sm', label, onInput: (v) => set({ [key]: Math.min(max, Math.max(min, Math.round(v))) }) });
    range.addEventListener('input', () => set({ [key]: Number(range.value) }));
    const row = el('div', { class: 'adj-row' }, el('span', { class: 'k', text: label }), range, nf.root);
    adj.append(row);
    return { row, range, nf };
  };
  const bri = rangeRow('brightness', 'Lysstyrke', 0, 100, '%');
  const spd = rangeRow('speed', 'Hastighet', 1, 10, '');
  spd.row.dataset.tip = '1 er langsomst, 10 raskest.';
  const dirSeg = segEl(
    [
      ['cw', 'Med klokka'],
      ['ccw', 'Mot klokka'],
    ],
    draft.direction,
    (v) => set({ direction: v }),
    { label: 'Retning', size: 'sm' },
  );
  const dirRow = el('div', { class: 'adj-row wide' }, el('span', { class: 'k', text: 'Retning' }), dirSeg);
  adj.append(dirRow);
  const tMin = numField({ min: 20, max: 95, step: 1, value: Math.round(F.tempOut(draft.tempMin)), unit: F.tempLabel(), size: 'sm', label: 'Kaldest', onInput: (v) => set({ tempMin: Math.round(F.tempIn(v)) }) });
  const tMax = numField({ min: 25, max: 100, step: 1, value: Math.round(F.tempOut(draft.tempMax)), unit: F.tempLabel(), size: 'sm', label: 'Varmest', onInput: (v) => set({ tempMax: Math.round(F.tempIn(v)) }) });
  const tempRow = el('div', { class: 'adj-row wide', 'data-tip': 'Under det kaldeste er fargen helt kald, over det varmeste helt varm.' }, el('span', { class: 'k', text: 'Område' }), el('span', { class: 'range-pair' }, tMin.root, '–', tMax.root));
  adj.append(tempRow);
  const nowRow = el('div', { class: 'adj-row wide' }, el('span', { class: 'k', text: 'GPU nå' }), el('span', { class: 'muted', id: 'rgb-gpu-now', text: '–' }));
  adj.append(nowRow);

  // ---- Påskeegg (easter.js): Nordlys over forhåndsvisningen ----
  // Utkastet og «Bruk» er urørt. Egget går bort når du velger noe, trykker
  // Esc, går til en annen side eller slår av «Små overraskelser».
  const eggNote = $('#rgb-egg-note');
  function setEgg(name) {
    egg = name === 'nordlys' ? 'nordlys' : null;
    engine.setEgg(egg ? aurora : null);
    eggNote.hidden = !egg;
    render();
  }

  function set(patch) {
    if (egg) setEgg(null);
    if (patch.tempMin != null && patch.tempMin > draft.tempMax - 5) return;
    if (patch.tempMax != null && patch.tempMax < draft.tempMin + 5) return;
    draft = { ...draft, ...patch, zones: { ...draft.zones, ...(patch.zones || {}) } };
    render();
  }

  function isDirty() {
    const a = app.state.rgb;
    return KEYS.some((k) => a[k] !== draft[k]) || ['bar', 'fan1', 'fan2'].some((z) => a.zones[z] !== draft.zones[z]);
  }
  const zoneLocked = () => !!egg || ['temperature', 'rainbow', 'off'].includes(draft.effect);

  function syncNotes() {
    const m = motion();
    const slow = view?.slow && m === 'full';
    const note = $('#rgb-stage-note');
    note.hidden = m === 'full' && !slow;
    setText(note, slow ? 'Stillbilde: grafikken tegner for tregt' : 'Animasjon av');
  }

  function render() {
    engine.setConfig(draft);
    const e = draft.effect;
    setSeg(zoneSeg, zone);
    engine.setZone(zoneLocked() ? 'all' : zone);
    stage.dataset.ambient = draft.ambient ? 'on' : 'off';
    view?.setAmbientEnabled(draft.ambient);
    amb.checked = !!draft.ambient;
    for (const r of effRows) {
      r.radio.checked = r.id === e;
      r.sample.style.setProperty('--sw', draft.zones.bar);
      r.sample.style.setProperty('--c1', draft.tempCold);
      r.sample.style.setProperty('--c2', draft.tempHot);
    }
    // Fargegruppen: sone eller kald/varm.
    const temp = e === 'temperature';
    tempSeg.hidden = !temp;
    setSeg(tempSeg, tempTarget);
    zoneSeg.querySelectorAll('button').forEach((b) => (b.disabled = temp || e === 'rainbow' || e === 'off'));
    const cur = temp ? draft[tempTarget === 'cold' ? 'tempCold' : 'tempHot'] : zone === 'all' ? draft.zones.bar : draft.zones[zone];
    picker.set(cur);
    const mixed = !temp && zone === 'all' && (draft.zones.bar !== draft.zones.fan1 || draft.zones.bar !== draft.zones.fan2);
    setText($('#rgb-color-title'), temp ? 'Farge · temperatur' : zone === 'all' ? `Farge${mixed ? ' · ulike soner' : ''}` : `Farge · ${ZONES[zone]}`);
    const noColor = e === 'rainbow' || e === 'off';
    $('#rgb-picker').inert = noColor;
    $('#rgb-picker').style.opacity = noColor ? '0.45' : '';
    if (noColor) picker.close();
    setTip($('#rgb-color-title'), e === 'rainbow' ? 'Regnbuen bruker alle farger.' : e === 'off' ? 'Lysene er av.' : mixed ? 'Sonene har ulike farger. Velg en sone for å se fargen den har.' : null);
    // Justering: vis bare det som gjelder effekten.
    for (const [r, k] of [[bri, 'brightness'], [spd, 'speed']]) {
      r.range.value = String(draft[k]);
      fillRange(r.range);
      if (document.activeElement !== r.nf.input) r.nf.input.value = String(draft[k]);
    }
    bri.row.hidden = e === 'off';
    spd.row.hidden = !['breathe', 'rainbow', 'cycle'].includes(e);
    dirRow.hidden = !['rainbow', 'cycle'].includes(e);
    setSeg(dirSeg, draft.direction);
    tempRow.hidden = !temp;
    nowRow.hidden = !temp;
    // Grensene står i valgt enhet (20–95 °C = 68–203 °F), ellers klemmes °F-verdier feil.
    tMin.input.min = String(Math.round(F.tempOut(20)));
    tMin.input.max = String(Math.round(F.tempOut(95)));
    tMax.input.min = String(Math.round(F.tempOut(25)));
    tMax.input.max = String(Math.round(F.tempOut(100)));
    if (document.activeElement !== tMin.input) tMin.input.value = String(Math.round(F.tempOut(draft.tempMin)));
    if (document.activeElement !== tMax.input) tMax.input.value = String(Math.round(F.tempOut(draft.tempMax)));
    for (const u of tempRow.querySelectorAll('.u')) setText(u, F.tempLabel());
    if (e === 'off') adj.querySelector('.empty-adj') || adj.append(el('p', { class: 'group-note empty-adj', text: 'Ingen justering når lysene er av.' }));
    else adj.querySelector('.empty-adj')?.remove();
    // Ikke brukt-tilstand
    const dirty = isDirty();
    $('#rgb-dirty').dataset.state = dirty ? 'dirty' : 'clean';
    setText($('#rgb-dirty-text'), dirty ? 'Forhåndsvisning – ikke brukt' : `Brukt: ${NAME[app.state.rgb.effect]}`);
    for (const id of ['#rgb-discard', '#rgb-apply']) {
      const b = $(id);
      // Forkast virker uten server; Bruk venter til forbindelsen er tilbake.
      const wait = app.offline && id === '#rgb-apply';
      b.disabled = !dirty || wait;
      setTip(b, wait ? 'Venter på serveren' : dirty ? (id === '#rgb-apply' ? 'Lagre lysvalget (simulert)' : 'Forkast endringene') : id === '#rgb-apply' ? 'Ingen endringer å bruke' : 'Ingen endringer å forkaste');
    }
    describeSoon();
  }

  function discard() {
    if (egg) setEgg(null);
    if (!isDirty()) return;
    draft = clone(app.state.rgb);
    render();
  }

  // Dobbeltklikk eller Ctrl+Enter to ganger skal bare gi én lagring.
  let applying = false;
  async function apply() {
    if (egg) setEgg(null);
    if (applying || app.offline || !isDirty()) return;
    applying = true;
    const btn = $('#rgb-apply');
    btn.disabled = true;
    try {
      const used = draft.effect === 'temperature' ? [draft.tempCold, draft.tempHot] : [draft.zones.bar, draft.zones.fan1, draft.zones.fan2];
      const recent = [...new Set([...used, ...(draft.recent || [])])].slice(0, 6);
      const res = await app.api.rgb({ ...draft, recent });
      app.state.rgb = res.rgb;
      draft = clone(res.rgb);
      picker.refreshRecent();
      render();
      confirmDone(btn);
      app.toast('Lys lagret (simulert)', { body: `${NAME[res.rgb.effect]}, ${res.rgb.brightness} %. Lyskontrolleren er ikke koblet til.` });
    } catch (err) {
      app.toast('Kunne ikke lagre lys', { body: err.message, error: true });
    } finally {
      applying = false;
      render();
    }
  }
  $('#rgb-discard').addEventListener('click', discard);
  $('#rgb-apply').addEventListener('click', apply);

  app.feed.on('sample', (s) => {
    if (s.fan != null) {
      lastFan = s.fan;
      svg?.setFan(s.fan);
      view?.setFanSpeed(s.fan);
    }
    engine.setTemp(s.temp);
    if (visible && draft.effect === 'temperature') setText($('#rgb-gpu-now'), F.temp(s.temp));
  });
  app.on('unit', render);
  app.on('conn', render);
  onMotion(() => {
    svg?.setRunning(visible);
    view?.change();
    engine.dirty = true;
    syncNotes();
  });
  syncNotes();
  // 2D/3D avgjøres første gang Lys åpnes (ingen WebGL-sonde ved appstart).
  page.dataset.mode = '2d';
  render();

  return {
    show() {
      visible = true;
      clearTimeout(releaseTimer);
      // Første gang (eller etter frigjøring): bygg 2D eller 3D nå.
      ensureRenderer().then(() => {
        if (!visible) return;
        svg?.setRunning(true);
        view?.setRunning(true);
        engine.dirty = true;
        wake();
      });
      stop?.();
      stop = onFrame((dt, now) => {
        const a = engine.tick(dt);
        const b = view ? view.tick(dt, now) : false;
        if (!loading.hidden && (!view || view.stats.frames > 0)) loading.hidden = true;
        return a || b || !loading.hidden;
      });
    },
    hide() {
      // Ingenting tegnes eller roteres mens siden ikke vises. 3D frigjøres etter 20 s.
      visible = false;
      if (egg) setEgg(null);
      svg?.setRunning(false);
      view?.setRunning(false);
      hideTip();
      picker.close();
      stop?.();
      stop = null;
      clearTimeout(releaseTimer);
      releaseTimer = setTimeout(() => {
        if (visible) return;
        drop3d();
        mode = null;
      }, RELEASE_MS);
    },
    egg: { active: () => !!egg, set: setEgg },
    /** For testene og feilsøking: hva vises nå, og 3D-visningen hvis den finnes. */
    debug: {
      mode: () => mode,
      view: () => view,
      release: () => {
        drop3d();
        mode = null;
      },
      leds: () => (view ? [...view.leds].map((v) => v | 0).join(',') : [...engine.cur].map((v) => v | 0).join(',')),
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
