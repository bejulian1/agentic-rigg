// Brukerens valg for utseende og format, lagt på <html> som attributter:
//   data-theme       light | dark (fravær = følg systemet)       (theme.js)
//   data-accent      blue | teal | green | yellow | orange | red | pink | purple | slate
//   data-palette     grafitt | sand | fjord | midnatt (fargetema for flater og tekst)
//   data-contrast    normal | high
//   data-density     compact | standard | comfortable
//   data-motion      full | reduced | off  – alltid oppløst. «system» følger
//                    prefers-reduced-motion live. Animasjonsagenten leser denne.
//   data-motion-pref system | full | reduced | off – brukerens valg
//   data-kbd, data-status-sparks, data-sensor-sparks  on | off
//   --fs-scale       skriftstørrelse (0,92–1,16)
//   data-side        full | rail – sidefeltet (settes av app.js, applySide(); theme-boot.js
//                    gjetter fra utdraget og vindusbredden før første tegning)
// Et lite utdrag lagres lokalt så theme-boot.js kan sette alt før første tegning.

import { setFormat } from './fmt.js';
import { setTheme } from './theme.js';
import { local } from './util.js';

const root = document.documentElement;
const mqReduce = window.matchMedia('(prefers-reduced-motion: reduce)');
let cur = null;
const listeners = new Set();

export function resolveMotion(pref) {
  if (pref === 'full' || pref === 'reduced' || pref === 'off') return pref;
  return mqReduce.matches ? 'reduced' : 'full';
}

function setMotion(pref) {
  const m = resolveMotion(pref);
  root.dataset.motionPref = pref || 'system';
  if (root.dataset.motion !== m) {
    root.dataset.motion = m;
    for (const fn of listeners) fn(m);
  }
}

/** Kalles når data-motion endres (for JS-animasjoner som må stoppe eller starte). */
export function onMotion(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function applyPrefs(s) {
  cur = s;
  setTheme(s.theme || 'system');
  root.dataset.accent = s.accent || 'blue';
  root.dataset.palette = s.palette || 'grafitt';
  root.dataset.contrast = s.highContrast ? 'high' : 'normal';
  root.dataset.density = s.density || 'standard';
  root.style.setProperty('--fs-scale', String((s.fontScale || 100) / 100));
  root.dataset.kbd = s.showKbd ? 'on' : 'off';
  root.dataset.statusSparks = s.statusSparks ? 'on' : 'off';
  root.dataset.sensorSparks = s.sensorSparks ? 'on' : 'off';
  setMotion(s.motion);
  setFormat(s);
  local.set('ag-prefs', { accent: s.accent, palette: s.palette, contrast: !!s.highContrast, density: s.density, fontScale: s.fontScale, motion: s.motion, kbd: s.showKbd, side: !!s.sideCollapsed });
}

mqReduce.addEventListener('change', () => {
  if (cur && (cur.motion || 'system') === 'system') setMotion('system');
});
