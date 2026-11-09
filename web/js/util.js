// Små hjelpere: DOM, canvas og en felles animasjonsløkke.
// Formatering av tall og enheter ligger i fmt.js.

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));
export const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
export const lerp = (a, b, t) => a + (b - a) * t;

/** Leser en CSS-variabel fra <html>. */
export function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Gjeldende animasjonsnivå: 'full', 'reduced' eller 'off' (fra data-motion på <html>). */
export const motion = () => document.documentElement.dataset.motion || 'full';

/**
 * Kryssfade av hele vinduet (View Transitions) rundt en endring av farger.
 * Bare etter oppstart, bare når fanen vises og aldri med animasjoner av.
 * Varigheten står i CSS (--dur-vt).
 */
let vtReady = false;
export const enableViewTransitions = () => (vtReady = true);
export function viewTransition(fn) {
  if (!vtReady || motion() === 'off' || !document.startViewTransition || document.visibilityState !== 'visible') return fn();
  const root = document.documentElement;
  const run = () => {
    // Elementenes egne fargeoverganger av mens fargene byttes (styles.css, seksjon 8).
    root.dataset.switching = '';
    fn();
    requestAnimationFrame(() => requestAnimationFrame(() => delete root.dataset.switching));
  };
  try {
    const vt = document.startViewTransition(run);
    // To bytter rett etter hverandre: den første hoppes over, og ready avvises
    // med AbortError. Det er ventet og skal ikke bli en konsollfeil (v0.0.9).
    vt.ready.catch(() => {});
    vt.finished.catch(() => {});
  } catch {
    run();
  }
}

/** Varighet i ms fra tokenene (--dur-1 … --dur-4, --dur-in, --dur-out). Følger data-motion. */
export function dur(name) {
  const v = cssVar(`--dur-${name}`);
  return v.endsWith('ms') ? parseFloat(v) : v.endsWith('s') ? parseFloat(v) * 1000 : 0;
}
/** Samme kurver som --ease-out, --ease-in og --ease-std i styles.css. */
export const EASE = { out: 'cubic-bezier(0.2, 0, 0, 1)', in: 'cubic-bezier(0.4, 0, 1, 1)', std: 'cubic-bezier(0.25, 0.1, 0.25, 1)' };

/**
 * Web Animations som respekterer data-motion. «full» spiller keyframes,
 * «reduced» spiller reducedFrames (typisk bare opasitet) hvis de er gitt,
 * og «off» gjør ingenting. Returnerer Animation eller null.
 */
export function animate(node, frames, opts, reducedFrames = null) {
  const m = motion();
  if (m === 'off' || !node?.animate) return null;
  if (m === 'reduced') return reducedFrames ? node.animate(reducedFrames, { ...opts, duration: Math.min(opts.duration ?? 100, 100) }) : null;
  return node.animate(frames, opts);
}

/**
 * Holder et canvas skarpt på høy-DPI. ResizeObserver måler (aldri i tegneløkka),
 * og onResize kalles med { w, h, dpr }.
 */
export function fitCanvas(canvas, onResize) {
  const ctx = canvas.getContext('2d');
  const box = { w: 0, h: 0, dpr: 1 };
  const ro = new ResizeObserver((entries) => {
    for (const e of entries) {
      const cr = e.contentRect;
      const dpr = Math.min(window.devicePixelRatio || 1, 3);
      box.w = Math.max(1, cr.width);
      box.h = Math.max(1, cr.height);
      box.dpr = dpr;
      canvas.width = Math.round(box.w * dpr);
      canvas.height = Math.round(box.h * dpr);
      onResize?.(box);
      wake();
    }
  });
  ro.observe(canvas);
  return { ctx, box };
}

// ---- Én felles rAF-løkke. Komponenter registrerer seg og tegner bare når synlige. ----
// En oppgave returnerer false når den ikke trenger flere bilder akkurat nå
// (ingenting beveger seg). Når alle er i ro, sover løkka og ber ikke om nye
// bilder i det hele tatt, så en side som står stille ikke koster noe på en
// 240 Hz-skjerm. wake() starter den igjen; app.js vekker den ved nye data,
// brukerinput og hendelser, og fitCanvas ved ny størrelse.
const tasks = new Set();
let running = false;
let last = 0;
function frame(now) {
  const dt = last ? Math.min(0.1, (now - last) / 1000) : 1 / 60;
  last = now;
  let busy = false;
  for (const t of tasks) {
    try {
      if (t(dt, now) !== false) busy = true;
    } catch (err) {
      console.error(err);
    }
  }
  if (tasks.size && busy) requestAnimationFrame(frame);
  else {
    running = false;
    last = 0;
  }
}
/** Starter løkka hvis den sover (kalles når noe kan ha endret seg). */
export function wake() {
  if (running || !tasks.size) return;
  running = true;
  requestAnimationFrame(frame);
}
/** Vekker løkka om ms millisekunder (én felles timer; den tidligste vinner). */
let wakeTimer = null;
let wakeAt = Infinity;
export function wakeIn(ms) {
  const at = performance.now() + ms;
  if (wakeTimer && at >= wakeAt) return;
  clearTimeout(wakeTimer);
  wakeAt = at;
  wakeTimer = setTimeout(() => {
    wakeTimer = null;
    wakeAt = Infinity;
    wake();
  }, Math.max(0, ms - 2));
}
export function onFrame(fn) {
  tasks.add(fn);
  wake();
  return () => tasks.delete(fn);
}

/** Eksponentiell glatting uavhengig av bildefrekvens. */
export const smooth = (cur, target, dt, tau = 0.14) => cur + (target - cur) * (1 - Math.exp(-dt / tau));

/** Lager et element. attrs: class, text, html (bare egen tekst), on*-lyttere og vanlige attributter. */
export function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === 'class') n.className = v;
    else if (k === 'text') n.textContent = v;
    else if (k === 'style' && typeof v === 'object') {
      for (const [sk, sv] of Object.entries(v)) {
        if (sk.startsWith('--')) n.style.setProperty(sk, sv);
        else n.style[sk] = sv;
      }
    }
    else if (k.startsWith('on') && typeof v === 'function') n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v === true ? '' : v);
  }
  for (const c of children.flat()) if (c != null && c !== false) n.append(c);
  return n;
}

const NS = 'http://www.w3.org/2000/svg';
/** Ikon fra spriten i index.html: <svg class="icon"><use href="#i-navn"/></svg>. */
export function icon(name, cls = 'icon') {
  const s = document.createElementNS(NS, 'svg');
  s.setAttribute('class', cls);
  s.setAttribute('aria-hidden', 'true');
  const u = document.createElementNS(NS, 'use');
  u.setAttribute('href', `#i-${name}`);
  s.append(u);
  return s;
}

/** Setter tekst bare når den faktisk endres (unngår unødvendig layout). */
export function setText(node, text) {
  if (node && node.textContent !== text) node.textContent = text;
}

/** Bytter ikonet i en knapp med <use>. */
export function setIcon(btn, name) {
  const u = btn.querySelector('use');
  if (u && u.getAttribute('href') !== `#i-${name}`) u.setAttribute('href', `#i-${name}`);
}

export function debounce(fn, ms) {
  let t = null;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

/** Lokal lagring som tåler at nettleseren nekter. */
export const local = {
  get(k, d = null) {
    try {
      const v = localStorage.getItem(k);
      return v == null ? d : JSON.parse(v);
    } catch {
      return d;
    }
  },
  set(k, v) {
    try {
      localStorage.setItem(k, JSON.stringify(v));
    } catch {}
  },
};
