// Prototypen: kobler kontrollene, effektmotoren og 3D-visningen sammen.
// Samme konfigurasjon som Lys-siden i appen (state.rgb), så lights.js kan
// ta over denne fila nesten uendret (se README.md).

import { CardView, VIEWS, STEPS } from './view.js';
import { RgbEngine } from './engine.js';
import { hexToLinear } from './gl/math.js';

const $ = (s) => document.querySelector(s);
const root = document.documentElement;
const motion = () => root.dataset.motion || 'full';
const cssVar = (n) => getComputedStyle(root).getPropertyValue(n).trim();
const durMs = (n) => {
  const v = cssVar(n);
  return v.endsWith('ms') ? parseFloat(v) : v.endsWith('s') ? parseFloat(v) * 1000 : 0;
};
const local = {
  get: (k, d) => {
    try {
      return localStorage.getItem(k) ?? d;
    } catch {
      return d;
    }
  },
  set: (k, v) => {
    try {
      localStorage.setItem(k, v);
    } catch {}
  },
};

const EFFECTS = [
  ['static', 'Statisk', true, 'Én fast farge.'],
  ['breathe', 'Pust', true, 'Fargen tones rolig opp og ned.'],
  ['rainbow', 'Regnbue', false, 'Alle farger løper rundt.'],
  ['cycle', 'Fargesyklus', false, 'Hele kortet bytter farge sakte.'],
  ['temperature', 'Temperatur', false, 'Fargen følger GPU-temperaturen, fra kald til varm.'],
  ['off', 'Av', true, 'Lysene er slått av.'],
];
const SWATCHES = [
  ['#f2f2f2', 'Hvit'],
  ['#e5383b', 'Rød'],
  ['#f07a1a', 'Oransje'],
  ['#f2c12e', 'Gul'],
  ['#3fb950', 'Grønn'],
  ['#2bb3c8', 'Cyan'],
  ['#3a7bf0', 'Blå'],
  ['#8e5cf0', 'Lilla'],
  ['#e2649e', 'Rosa'],
];
const ZONES = { all: 'Alle soner', bar: 'Lyslist', fan1: 'Vifte 1', fan2: 'Vifte 2' };

// ---------------------------------------------------------------- Tema og bevegelse
const mqDark = matchMedia('(prefers-color-scheme: dark)');
const mqReduce = matchMedia('(prefers-reduced-motion: reduce)');
function applyTheme() {
  const t = root.dataset.themePref || 'system';
  root.dataset.resolved = t === 'light' || t === 'dark' ? t : mqDark.matches ? 'dark' : 'light';
}
function applyMotion() {
  const m = root.dataset.motionPref || 'system';
  const resolved = m === 'full' || m === 'reduced' || m === 'off' ? m : mqReduce.matches ? 'reduced' : 'full';
  if (root.dataset.motion !== resolved) {
    root.dataset.motion = resolved;
    onMotion();
  }
}
mqDark.addEventListener('change', applyTheme);
mqReduce.addEventListener('change', applyMotion);

/** Segmentkontroll: radioknapper med piltaster (roving tabindex), som appens bindSeg. */
function bindSeg(seg, onPick) {
  const btns = [...seg.querySelectorAll('button')];
  seg.addEventListener('click', (e) => {
    const b = e.target.closest('button');
    if (b && !b.disabled) onPick(b.dataset.v);
  });
  seg.addEventListener('keydown', (e) => {
    const i = btns.indexOf(document.activeElement);
    if (i < 0) return;
    const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    for (let k = 1; k <= btns.length; k++) {
      const n = btns[(i + d * k + btns.length) % btns.length];
      if (!n.disabled) {
        n.focus();
        onPick(n.dataset.v);
        break;
      }
    }
  });
}
function setSeg(seg, v) {
  const btns = [...seg.querySelectorAll('button')];
  let any = false;
  for (const b of btns) {
    const on = b.dataset.v === String(v);
    b.setAttribute('aria-checked', String(on));
    b.tabIndex = on ? 0 : -1;
    any ||= on;
  }
  if (!any && btns[0]) btns[0].tabIndex = 0;
}
function fillRange(r) {
  const p = ((Number(r.value) - Number(r.min)) / (Number(r.max) - Number(r.min))) * 100;
  r.style.setProperty('--p', `${p}%`);
}

// ---------------------------------------------------------------- Tilstand
let cfg = {
  color: '#3a7bf0',
  zones: { bar: '#3a7bf0', fan1: '#3a7bf0', fan2: '#3a7bf0' },
  effect: 'static',
  brightness: 80,
  speed: 5,
  direction: 'cw',
  tempCold: '#3a7bf0',
  tempHot: '#e5383b',
  tempMin: 40,
  tempMax: 80,
  ambient: false,
};
let zone = 'all';
let tempTarget = 'cold';
const sim = { temp: 58, fan: 0, auto: true };
let step = 0;

// URL-parametre (brukt av skjermbildene, nyttige for å dele en tilstand).
const q = new URLSearchParams(location.search);
if (q.get('effekt')) cfg.effect = q.get('effekt');
if (q.get('farge')) {
  const c = `#${q.get('farge').replace('#', '')}`;
  cfg.color = c;
  cfg.zones = { bar: c, fan1: c, fan2: c };
}
if (q.get('lysstyrke')) cfg.brightness = Number(q.get('lysstyrke'));
if (q.get('temp')) sim.temp = Number(q.get('temp'));
if (q.get('bakgrunnslys') === 'på') cfg.ambient = true;
if (q.get('skall') === 'av') $('#shell').dataset.skall = 'av';
const fanCurve = (t) => (t < 46 ? 0 : Math.round(Math.min(100, 30 + (t - 46) * 1.9)));
sim.fan = fanCurve(sim.temp);

// ---------------------------------------------------------------- 3D
const vp = $('#viewport');
const tip = $('#hover-tip');
let view = null;
let engine = null;
try {
  view = new CardView(vp, {
    motion,
    onHover: (info, x, y) => showTip(info, x, y),
    onZone: (z) => {
      if (zoneLocked()) return;
      zone = z;
      local.set('lys3d-zone', z);
      render();
      announce(`Sone: ${ZONES[z]}`);
    },
    onPart: (info) => togglePinned(info.id),
    onChange: () => describeSoon(),
    onSlow: () => onMotion(),
    adapt: q.get('kvalitet') !== 'fast',
  });
  engine = new RgbEngine(view, { motion: () => view.effectiveMotion(), crossfadeMs: () => durMs('--dur-4') });
  view.addTicker((dt) => engine.tick(dt));
} catch (err) {
  console.warn('3D-visningen kunne ikke startes:', err);
  vp.removeAttribute('role');
  vp.removeAttribute('tabindex');
  vp.innerHTML = `<div class="fallback"><p><b>3D-visningen krever WebGL 2.</b><br />Nettleseren eller grafikkdriveren har det ikke slått på. Kontrollene virker, men kortet vises ikke.</p></div>`;
  $('#view-keys').hidden = true;
}

function syncColors() {
  if (!view) return;
  view.bg = cssVar('--stage')
    .replace('#', '')
    .match(/../g)
    .map((h) => parseInt(h, 16) / 255);
  view.accent = hexToLinear(cssVar('--a-d') || '#559cf7');
  view.dirty = true;
  view.kick();
}

function showTip(info, x, y) {
  if (!info) {
    tip.removeAttribute('data-on');
    return;
  }
  const zoneHint = info.zone ? (zoneLocked() ? '<em>Sonen kan ikke velges med denne effekten.</em>' : '<em>Klikk for å velge sonen.</em>') : '';
  tip.innerHTML = `<b></b><span></span>${zoneHint}`;
  tip.querySelector('b').textContent = info.name;
  tip.querySelector('span').textContent = info.desc;
  const w = vp.clientWidth;
  const h = vp.clientHeight;
  const tw = tip.offsetWidth;
  const th = tip.offsetHeight;
  const px = Math.min(w - tw - 8, x + 14);
  const py = y + 18 + th > h - 8 ? y - th - 12 : y + 18;
  tip.style.transform = `translate(${Math.max(8, px)}px, ${Math.max(8, py)}px)`;
  tip.setAttribute('data-on', '');
}

// ---------------------------------------------------------------- Effekter
const effHost = $('#effects');
const effRows = EFFECTS.map(([id, name, ok, desc]) => {
  const label = document.createElement('label');
  label.className = 'effect';
  label.title = desc;
  label.innerHTML = `<input type="radio" class="radio" name="effect" value="${id}" /><span class="sample" data-fx="${id}"></span><span class="fx-name"></span>${ok ? '<span></span>' : '<span class="eye" title="Bare forhåndsvisning. Ikke dokumentert for kontrolleren."><svg class="icon" aria-hidden="true"><use href="#i-eye" /></svg><span class="sr-only">bare forhåndsvisning</span></span>'}`;
  label.querySelector('.fx-name').textContent = name;
  const radio = label.querySelector('input');
  radio.addEventListener('change', () => set({ effect: id }));
  effHost.append(label);
  return { id, radio, sample: label.querySelector('.sample') };
});

// ---------------------------------------------------------------- Farge
const swHost = $('#swatches');
const swBtns = SWATCHES.map(([c, n]) => {
  const b = document.createElement('button');
  b.type = 'button';
  b.setAttribute('role', 'radio');
  b.setAttribute('aria-label', n);
  b.title = n;
  b.dataset.v = c;
  b.style.setProperty('--sw', c);
  swHost.append(b);
  return b;
});
bindSeg(swHost, (c) => pickColor(c));
const hex = $('#hex');
hex.addEventListener('change', () => {
  const c = parseColor(hex.value);
  if (!c) {
    hex.dataset.invalid = '';
    $('#hex-msg').textContent = 'Ugyldig fargekode';
    return;
  }
  delete hex.dataset.invalid;
  $('#hex-msg').textContent = '';
  pickColor(c);
});
function parseColor(s) {
  const t = String(s || '').trim().toLowerCase();
  let m = /^#?([0-9a-f]{6})$/.exec(t);
  if (m) return `#${m[1]}`;
  m = /^#?([0-9a-f])([0-9a-f])([0-9a-f])$/.exec(t);
  if (m) return `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}`;
  return null;
}
function pickColor(c) {
  if (cfg.effect === 'temperature') set({ [tempTarget === 'cold' ? 'tempCold' : 'tempHot']: c });
  else if (zone === 'all') set({ color: c, zones: { bar: c, fan1: c, fan2: c } });
  else set({ zones: { ...cfg.zones, [zone]: c }, color: zone === 'bar' ? c : cfg.color });
}
bindSeg($('#temp-seg'), (v) => {
  tempTarget = v;
  render();
});

// ---------------------------------------------------------------- Justering
function rangePair(id, key, min, max) {
  const r = $(`#${id}`);
  const n = $(`#${id}-n`);
  r.addEventListener('input', () => set({ [key]: Number(r.value) }));
  n.addEventListener('change', () => {
    const v = Math.round(Number(n.value));
    if (Number.isFinite(v)) set({ [key]: Math.min(max, Math.max(min, v)) });
  });
  return { r, n, key };
}
const bri = rangePair('bri', 'brightness', 0, 100);
const spd = rangePair('spd', 'speed', 1, 10);
bindSeg($('#dir-seg'), (v) => set({ direction: v }));

// ---------------------------------------------------------------- Scene: lag, avstand, sone, vinkel
bindSeg($('#step-seg'), (v) => {
  step = Number(v);
  view?.setStep(step);
  render();
  renderParts();
  announce(`${STEPS[step]}.`);
});
const explode = $('#explode');
explode.addEventListener('input', () => {
  view?.setExplode(Number(explode.value) / 100, { follow: true });
  render();
});
bindSeg($('#zone-seg'), (v) => {
  zone = v;
  local.set('lys3d-zone', v);
  render();
});
bindSeg($('#view-seg'), (v) => {
  view?.setView(v);
  render();
});
$('#reset-view').addEventListener('click', () => {
  view?.resetView();
  render();
});
$('#ambient').addEventListener('change', (e) => set({ ambient: e.target.checked }));

// Simulert kort
const tempR = $('#temp');
const fanR = $('#fan');
tempR.addEventListener('input', () => {
  sim.temp = Number(tempR.value);
  if (sim.auto) sim.fan = fanCurve(sim.temp);
  renderSim();
});
fanR.addEventListener('input', () => {
  sim.fan = Number(fanR.value);
  sim.auto = false;
  renderSim();
});
$('#fan-auto').addEventListener('change', (e) => {
  sim.auto = e.target.checked;
  if (sim.auto) sim.fan = fanCurve(sim.temp);
  renderSim();
});
function renderSim() {
  tempR.value = String(sim.temp);
  fanR.value = String(sim.fan);
  fillRange(tempR);
  fillRange(fanR);
  $('#temp-out').textContent = `${sim.temp} °C`;
  $('#fan-out').textContent = sim.fan ? `${sim.fan} %` : 'Stopp';
  $('#fan-auto').checked = sim.auto;
  view?.setFanSpeed(sim.fan);
  engine?.setTemp(sim.temp);
  describeSoon();
}

// Tema og animasjoner
bindSeg($('#theme-seg'), (v) => {
  root.dataset.themePref = v;
  local.set('lys3d-theme', v);
  applyTheme();
  setSeg($('#theme-seg'), v);
  syncColors();
});
bindSeg($('#motion-seg'), (v) => {
  root.dataset.motionPref = v;
  local.set('lys3d-motion', v);
  setSeg($('#motion-seg'), v);
  applyMotion();
});
function onMotion() {
  const slow = view?.slow && motion() === 'full';
  $('#stage-note').hidden = motion() === 'full' && !slow;
  $('#stage-note').textContent = slow ? 'Stillbilde: grafikken tegner for tregt' : motion() === 'off' ? 'Animasjon av' : 'Redusert bevegelse';
  if (engine) engine.dirty = true;
  view?.change();
}

// ---------------------------------------------------------------- Deler
const pinned = { id: null };
function togglePinned(id) {
  pinned.id = pinned.id === id ? null : id;
  view?.setFocusGroup(pinned.id);
  renderParts();
}
function renderParts() {
  if (!view) return;
  const list = view.visibleGroups();
  const ul = $('#parts');
  ul.replaceChildren(
    ...list.map((g) => {
      const li = document.createElement('li');
      const b = document.createElement('button');
      b.type = 'button';
      b.dataset.g = g.id;
      b.setAttribute('aria-pressed', String(pinned.id === g.id));
      b.innerHTML = '<span class="n"></span><span class="d"></span>';
      b.querySelector('.n').textContent = g.name;
      b.querySelector('.d').textContent = g.desc;
      b.title = `${g.name}: ${g.desc}`;
      const on = () => view.setFocusGroup(g.id);
      const off = () => view.setFocusGroup(pinned.id);
      b.addEventListener('mouseenter', on);
      b.addEventListener('focus', on);
      b.addEventListener('mouseleave', off);
      b.addEventListener('blur', off);
      b.addEventListener('click', () => togglePinned(g.id));
      li.append(b);
      return li;
    }),
  );
  if (pinned.id && !list.some((g) => g.id === pinned.id)) {
    pinned.id = null;
    view.setFocusGroup(null);
  }
  $('#parts-count').textContent = `${list.length} synlige`;
}

// ---------------------------------------------------------------- Felles
function zoneLocked() {
  return ['temperature', 'rainbow', 'off'].includes(cfg.effect);
}
function set(patch) {
  cfg = { ...cfg, ...patch, zones: { ...cfg.zones, ...(patch.zones || {}) } };
  render();
}
function render() {
  const e = cfg.effect;
  engine?.setConfig(cfg);
  engine?.setZone(zoneLocked() ? 'all' : zone);
  view?.setAmbientEnabled(cfg.ambient);
  $('#ambient').checked = !!cfg.ambient;
  for (const r of effRows) {
    r.radio.checked = r.id === e;
    r.sample.style.setProperty('--sw', cfg.zones.bar);
    r.sample.style.setProperty('--c1', cfg.tempCold);
    r.sample.style.setProperty('--c2', cfg.tempHot);
  }
  setSeg($('#zone-seg'), zoneLocked() ? 'all' : zone);
  for (const b of $('#zone-seg').querySelectorAll('button')) b.disabled = zoneLocked();
  setSeg($('#step-seg'), step);
  setSeg($('#dir-seg'), cfg.direction);
  const temp = e === 'temperature';
  $('#temp-seg').hidden = !temp;
  setSeg($('#temp-seg'), tempTarget);
  const cur = temp ? cfg[tempTarget === 'cold' ? 'tempCold' : 'tempHot'] : zone === 'all' ? cfg.zones.bar : cfg.zones[zone];
  for (const b of swBtns) b.setAttribute('aria-checked', String(b.dataset.v === cur));
  setSeg(swHost, cur);
  if (document.activeElement !== hex) hex.value = cur.toUpperCase();
  $('#color-now').style.setProperty('--sw', cur);
  const mixed = !temp && zone === 'all' && (cfg.zones.bar !== cfg.zones.fan1 || cfg.zones.bar !== cfg.zones.fan2);
  $('#color-title').textContent = temp ? 'Farge · temperatur' : zone === 'all' ? `Farge${mixed ? ' · ulike soner' : ''}` : `Farge · ${ZONES[zone]}`;
  const noColor = e === 'rainbow' || e === 'off';
  $('#color-body').inert = noColor;
  $('#color-body').style.opacity = noColor ? '0.45' : '';
  for (const p of [bri, spd]) {
    p.r.value = String(cfg[p.key]);
    fillRange(p.r);
    if (document.activeElement !== p.n) p.n.value = String(cfg[p.key]);
  }
  $('#row-bri').hidden = e === 'off';
  $('#row-spd').hidden = !['breathe', 'rainbow', 'cycle'].includes(e);
  $('#row-dir').hidden = !['rainbow', 'cycle'].includes(e);
  $('#adj-empty').hidden = e !== 'off';
  explode.value = String(Math.round((view?.explodeGoal ?? 0) * 100));
  fillRange(explode);
  $('#explode-out').textContent = `${explode.value} %`;
  const v = Object.entries(VIEWS).find(([, w]) => view && Math.abs(wrapDeg(view.goal.yaw - w.yaw)) < 0.5 && Math.abs(view.goal.pitch - w.pitch) < 0.5);
  setSeg($('#view-seg'), v ? v[0] : '');
  describeSoon();
}
const wrapDeg = (a) => ((((a + 180) % 360) + 360) % 360) - 180;

let descT = 0;
function describeSoon() {
  clearTimeout(descT);
  descT = setTimeout(() => {
    if (!view) return;
    const d = view.describe();
    const fx = EFFECTS.find((x) => x[0] === cfg.effect)[1];
    const lys = cfg.effect === 'off' ? 'Lysene er av.' : `Lys: ${fx}, ${cfg.brightness} % lysstyrke.`;
    $('#view-desc').textContent = `${d} ${lys}`;
    $('#stage-caption').textContent = `${STEPS[view.step]}${view.explodeGoal > 0.02 ? ' · lagene skilt' : ''}`;
    const v = Object.entries(VIEWS).find(([, w]) => Math.abs(wrapDeg(view.goal.yaw - w.yaw)) < 0.5 && Math.abs(view.goal.pitch - w.pitch) < 0.5);
    setSeg($('#view-seg'), v ? v[0] : '');
  }, 120);
}
function announce(s) {
  const a = $('#announce');
  a.textContent = '';
  setTimeout(() => (a.textContent = s), 30);
}

// ---------------------------------------------------------------- Start
zone = local.get('lys3d-zone', 'all');
if (!ZONES[zone]) zone = 'all';
setSeg($('#theme-seg'), root.dataset.themePref || 'system');
setSeg($('#motion-seg'), root.dataset.motionPref || 'system');
if (q.get('lag')) step = Math.max(0, Math.min(3, Number(q.get('lag')) || 0));
if (q.get('vinkel') && VIEWS[q.get('vinkel')]) view?.setView(q.get('vinkel'));
if (q.get('skill')) view?.setExplode(Number(q.get('skill')) / 100, { follow: true });
if (view && step) {
  // Start rett i valgt trinn, uten animasjon.
  const m = root.dataset.motion;
  root.dataset.motion = 'off';
  view.setStep(step);
  for (const s of Object.values(view.layerState)) s.out = s.goal;
  view.fit(true);
  root.dataset.motion = m;
}
syncColors();
renderSim();
render();
renderParts();
onMotion();
view?.setRunning(true);
if (view) view.fit(true);

// For verktoy/kontroll.mjs og feilsøking i konsollen.
window.lys3d = { view, engine, get cfg() { return cfg; }, set, sim, renderSim, render };
