// Tema: «system» følger prefers-color-scheme live, «light»/«dark» tvinger.
// data-theme på <html> er brukerens valg (fravær = system), og data-resolved
// er alltid light eller dark. CSS-tokenene leser bare data-resolved, så det
// finnes én kilde per tema. Lyttere varsles så canvas-grafer leser fargene på nytt.

import { viewTransition } from './util.js';

const root = document.documentElement;
const mq = window.matchMedia('(prefers-color-scheme: dark)');
let pref = 'system';
const listeners = new Set();

export const resolvedTheme = () => (pref === 'system' ? (mq.matches ? 'dark' : 'light') : pref);
export const themePref = () => pref;

function notify() {
  root.dataset.resolved = resolvedTheme();
  for (const fn of listeners) fn(resolvedTheme());
}

export function setTheme(p) {
  const next = p === 'light' || p === 'dark' ? p : 'system';
  const changed = next !== pref || root.dataset.resolved !== (next === 'system' ? (mq.matches ? 'dark' : 'light') : next);
  pref = next;
  if (pref === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', pref);
  try {
    localStorage.setItem('ag-theme', pref);
  } catch {}
  if (changed) notify();
  else root.dataset.resolved = resolvedTheme();
}

export function onTheme(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

// Systemet bytter lyst/mørkt: samme kryssfade som når du bytter selv.
mq.addEventListener('change', () => {
  if (pref === 'system') viewTransition(notify);
});
