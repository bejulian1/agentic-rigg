// Små overraskelser (påskeegg), v0.0.12. Alle eggene står her, i én fil.
//
// Fasit med begrunnelse: docs/paaskeegg.md. Lista EGGS under er den samme,
// i samme rekkefølge, og er stedet å endre eller ta bort et egg.
//
// Reglene alle eggene følger:
//   - De endrer aldri et målt tall, later aldri som noe er ekte, og sender
//     ingenting til skjermkortet, bakgrunnstjenesten eller serveren.
//   - De rører ikke utkastet, bekreftelsesflyten, kontrollene eller
//     innstillingene. Nordlys på Lys er bare i forhåndsvisningen og går
//     tilbake av seg selv (Esc, et nytt valg eller en annen side).
//   - Ingen lyder, ingen dialoger og ingen blinking. Det som beveger seg,
//     følger data-motion via animate() i util.js: «Redusert» og «Av» gir
//     stillbilder og ingen Web Animations.
//   - Ingen gul, oransje eller rød: de fargene betyr advarsel og fare
//     (docs/grafer-og-malere-v0.0.11.md). Eggene bruker aksent og dempet tekst.
//   - Bryteren «Små overraskelser» (Innstillinger › Generelt, settings.surprises,
//     på som standard) slår av alt med en gang.
//   - Hva du har funnet, huskes bare i nettleseren (localStorage 'ag-eggs'),
//     ikke i innstillingene.
//
// Kroker i andre filer (korte, merket «Påskeegg»; Lys har litt mer):
//   app.js                  initEaster(app)
//   ui.js                   toast(…, { icon }) – egg-ikonet i varselet
//   pages/overview.js       mark42()
//   pages/gpu.js            launchDay()
//   pages/sensors.js        filterQuip()
//   pages/fancurve-page.js  curveNickname()
//   pages/lights.js + rgb.js  egg.set('nordlys') og RgbPreview.setEgg()
//   pages/settings.js       surprisesRow() og aboutEgg()
//   index.html              ikonene egg og cake, merket #rgb-egg-note på Lys
//   styles.css              siste seksjon, «v0.0.12 · Små overraskelser»

import { $, el, icon, local, animate } from './util.js';
import { announce, setTip, toast } from './ui.js';
import { hslToRgb } from './rgb.js';

/** Alle eggene. id brukes i koden, i 'ag-eggs' og i røyktesten. */
export const EGGS = [
  { id: 'konami', name: 'Juksekoden', trigger: '↑ ↑ ↓ ↓ ← → ← → B A, hvor som helst utenfor et tekstfelt', what: 'Varsel: «Juksekode godtatt. +30 liv.» Ingenting annet endres.' },
  { id: 'stov', name: 'Støvtørking', trigger: 'Fem raske klikk på appikonet øverst i sidefeltet', what: 'Ikonet snurrer to runder og bremser opp (bare ved full bevegelse), og et varsel sier at støvet er blåst av.' },
  { id: 'utvikler', name: 'Utviklermodus', trigger: 'Sju klikk på versjonen i Innstillinger › Om', what: 'Nedtelling under versjonen, så «Du er nå utvikler» og en rad med hvor mange egg du har funnet.' },
  { id: 'svaret', name: 'Svaret på alt', trigger: 'GPU-temperaturen er nøyaktig 42 °C (Oversikt)', what: 'Temperaturen får en prikket understrek i aksentfarge og et verktøytips om livet, universet og alt.' },
  { id: 'bursdag', name: 'Bursdag', trigger: '20. september på Grafikkort, når kortet er et RTX 2080', what: 'Raden «Lansert» får et kakeikon og «8 år i dag».' },
  { id: 'natt', name: 'Nattevakt', trigger: 'Klokka 02:00–04:59', what: 'En liten måne ved alpha-merket i sidefeltet, med et verktøytips.' },
  { id: 'kaffe', name: 'Kaffe', trigger: '«kaffe» eller «hei» i filteret på Sensorer', what: 'Tomtilstanden svarer med en egen setning i stedet for «Ingen sensorer passer filteret».' },
  { id: 'kallenavn', name: 'Kallenavn på kurven', trigger: 'Alle punktene på viftekurven på 100 % (eller alle på 35 %)', what: 'Tittelen sier «Kurve · Jetmotor» (eller «Kurve · Bibliotek») i stedet for «Egendefinert».' },
  { id: 'konsoll', name: 'Hilsen i konsollen', trigger: 'Åpne utviklerverktøyene i nettleseren', what: 'En hilsen og rigg.hint(), som gir et hint om et egg du ikke har funnet.' },
  { id: 'nordlys', name: 'Nordlys', trigger: 'Skriv «nordlys» på Lys (utenfor et tekstfelt)', what: 'Forhåndsvisningen viser nordlys til du trykker Esc, velger noe eller går til en annen side. Lagres ikke.' },
];

/** Hint fra rigg.hint() i konsollen, ett per egg. */
const HINTS = {
  konami: 'Noen koder husker man fra åttitallet. Opp, opp …',
  stov: 'Appikonet har en vifte. Vifter samler støv.',
  utvikler: 'På telefoner blir man utvikler ved å trykke på byggnummeret. Hvor står versjonen her?',
  svaret: 'Ett tall er viktigere enn alle andre. Følg med på GPU-temperaturen.',
  bursdag: 'Kortet har bursdag én gang i året. Se etter lanseringen på Grafikkort.',
  natt: 'Noen detaljer viser seg bare når resten av huset sover.',
  kaffe: 'Filteret på Sensorer leter etter mer enn sensorer. Hva drikker du nå?',
  kallenavn: 'En viftekurve som står rett på taket, fortjener et navn.',
  konsoll: 'Du fant denne. Godt jobbet.',
  nordlys: 'På Lys finnes det en effekt som ikke står i lista. Den er norsk.',
};

const KONAMI = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a'];
const NIGHT = [2, 5]; // fra kl. 02:00 til 04:59
/** Lanseringsdager: kortnavnet (fra spesifikasjonen) → dato. Bare kort vi vet datoen for. */
const LAUNCH_DAYS = [{ match: /RTX 2080(?!\s*(Ti|Super))/i, y: 2018, m: 9, d: 20, name: 'GeForce RTX 2080' }];
const FILTER_QUIPS = [
  [/^(kaffe|kaffi|kaffen|coffee)$/, 'Ingen sensor måler kaffe ennå. Det nærmeste er CPU-temperaturen.'],
  [/^(hei|hallo|heisann|hei hei|hoi)$/, 'Hei! Ingen sensor heter det, men takk for at du spør.'],
];

let app = null;
let nowFn = () => new Date();
const found = new Set(local.get('ag-eggs', []).filter((id) => EGGS.some((e) => e.id === id)));

/** Er overraskelsene slått på? (På som standard, også før innstillingene er lest.) */
export const enabled = () => app?.settings?.surprises !== false;

/** Teller et egg som funnet (bare i nettleseren). Returnerer true første gang. */
function mark(id) {
  if (found.has(id)) return false;
  found.add(id);
  local.set('ag-eggs', [...found]);
  return true;
}
export const foundCount = () => found.size;

/** Et lite varsel med egg-ikonet. Samme varsel som ellers i appen (ingen dialog, forsvinner av seg selv). */
function say(title, body) {
  toast(title, { body, icon: 'egg', ms: 5000 });
}

// ---------------------------------------------------------------- 1 · Juksekoden

let konamiAt = 0;
function onKonami(e) {
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  const t = e.target;
  if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (k === KONAMI[konamiAt]) konamiAt++;
  else konamiAt = k === KONAMI[0] ? 1 : 0;
  if (konamiAt < KONAMI.length) return;
  konamiAt = 0;
  if (!enabled()) return;
  mark('konami');
  say('Juksekode godtatt', '+30 liv. Klokkene, effekten og viften er som før – kortet får aldri vite om dette.');
}

// ---------------------------------------------------------------- 2 · Støvtørking

let iconClicks = [];
function onIconClick() {
  if (!enabled()) return;
  const now = performance.now();
  iconClicks = [...iconClicks.filter((t) => now - t < 2000), now];
  if (iconClicks.length < 5) return;
  iconClicks = [];
  const img = $('.app-icon');
  // To runder som bremser opp, som en vifte som går ned i fart. Bare ved full
  // bevegelse (animate() gjør ingenting ved «Av», og uten reducedFrames heller
  // ingenting ved «Redusert»).
  animate(img, [{ transform: 'rotate(0turn)' }, { transform: 'rotate(2turn)' }], { duration: 1400, easing: 'cubic-bezier(0.15, 0.6, 0.25, 1)' });
  mark('stov');
  say('Pff!', 'Støvet er blåst av viften. Bare på ikonet – den ekte viften går som før.');
}

// ---------------------------------------------------------------- 3 · Utviklermodus

const DEV_CLICKS = 7;
const WORDS = ['', 'ett', 'to', 'tre', 'fire', 'fem', 'seks'];
let devClicks = 0;
let devTimer = null;

/**
 * Innstillinger › Om: klikk på versjonen teller ned, og etter sju er du
 * «utvikler». Da står det også hvor mange egg du har funnet. Kalles hver gang
 * Om tegnes; tag er versjonsmerket, rows er feilsøkingslista (dl).
 */
export function aboutEgg(tag, rows) {
  // Rett under versjonen, så det er tydelig hva som teller. Tom linje tar ingen plass (CSS :empty).
  const whisper = el('p', { class: 'egg-whisper', 'aria-live': 'polite', 'data-egg-node': 'utvikler' });
  tag.after(whisper);
  const countRow = () => el('div', { class: 'kv-row', 'data-egg-node': 'utvikler' }, el('dt', { text: 'Små overraskelser' }), el('dd', {}, el('span', { text: `${found.size} av ${EGGS.length} funnet` })));
  if (enabled() && found.has('utvikler')) rows.append(countRow());
  devClicks = 0;
  tag.addEventListener('click', () => {
    if (!enabled()) return;
    clearTimeout(devTimer);
    devTimer = setTimeout(() => {
      devClicks = 0;
      whisper.textContent = '';
    }, 4000);
    devClicks++;
    // De to første klikkene sier ingenting, så et tilfeldig klikk ikke gir tekst.
    if (devClicks < 3) return;
    const left = DEV_CLICKS - devClicks;
    if (found.has('utvikler')) whisper.textContent = 'Du er allerede utvikler.';
    else if (left > 0) whisper.textContent = `${WORDS[left].charAt(0).toUpperCase()}${WORDS[left].slice(1)} klikk til, så er du utvikler.`;
    else {
      clearTimeout(devTimer);
      mark('utvikler');
      whisper.textContent = 'Du er nå utvikler. Det var du nok fra før.';
      rows.querySelector('[data-egg-node]')?.remove();
      rows.append(countRow());
    }
  });
}

// ---------------------------------------------------------------- 4 · Svaret på alt

/**
 * Oversikt: temperaturen som vises (hele grader) er nøyaktig 42 °C. host er
 * linja (#ov-temp-margin), som står fast, og b er <b>-elementet med tallet,
 * som lages på nytt for hver prøve. Verktøytipset ligger derfor på linja, så
 * det blir stående mens du holder over. Tallet er det målte; bare
 * understreken og tipset er lagt til.
 */
export function mark42(host, b) {
  const on = enabled() && /^42\s?°C$/.test(b.textContent.replace(/ /g, ' '));
  setTip(host, on ? '42 °C – svaret på livet, universet og alt. Målingen er ekte; det er bare tallet som er morsomt.' : null);
  if (!on) return;
  b.classList.add('egg-42');
  b.append(el('span', { class: 'sr-only', text: ' – svaret på livet, universet og alt' }));
  mark('svaret');
}

// ---------------------------------------------------------------- 5 · Bursdag

/** Grafikkort › Lansert: kakeikon og «N år i dag» på kortets lanseringsdag. dd er raden fra kv(). */
export function launchDay(dd, spec) {
  if (!dd || !spec || !enabled()) return;
  const hit = LAUNCH_DAYS.find((l) => l.match.test(spec.name || ''));
  const now = nowFn();
  if (!hit || now.getMonth() + 1 !== hit.m || now.getDate() !== hit.d) return;
  const years = now.getFullYear() - hit.y;
  if (years < 1) return;
  // «Kopier som tekst» skal bare ha lanseringsmåneden, ikke gratulasjonen.
  // Inne i verdien (ikke ved siden av), så linja brytes mellom ord i smale vinduer.
  dd.dataset.copy = dd.textContent.trim();
  (dd.firstElementChild || dd).append(
    ' ', // et vanlig mellomrom forsvinner der linja brytes, så ingenting rykkes inn
    el(
      'span',
      { class: 'egg-inline', 'data-egg-node': 'bursdag', 'data-tip': `Gratulerer med dagen! ${hit.name} kom i butikkene ${hit.d}. september ${hit.y}.` },
      icon('cake'),
      `${years} år i dag`,
    ),
  );
  mark('bursdag');
}

// ---------------------------------------------------------------- 6 · Nattevakt

function renderNight() {
  const head = $('.side-head');
  const cur = head.querySelector('.egg-moon');
  const h = nowFn().getHours();
  const on = enabled() && h >= NIGHT[0] && h < NIGHT[1];
  if (!on) return cur?.remove();
  if (cur) return;
  const text = 'Sent oppe? Skjermkortet er våkent det også.';
  const moon = el('span', { class: 'egg-moon', role: 'img', 'aria-label': text, 'data-tip': text, 'data-tip-side': 'right', 'data-egg-node': 'natt' }, icon('moon'));
  head.append(moon);
  mark('natt');
}

// ---------------------------------------------------------------- 7 · Kaffe

/** Sensorer: tekst i tomtilstanden for noen få ord, eller null. Kalles bare når filteret ikke har treff. */
export function filterQuip(filter) {
  if (!enabled() || !filter) return null;
  const q = FILTER_QUIPS.find(([re]) => re.test(filter.trim().toLowerCase()));
  if (!q) return null;
  mark('kaffe');
  return q[1];
}

// ---------------------------------------------------------------- 8 · Kallenavn på kurven

/**
 * Viftekurve: kallenavn når alle punktene står på 100 % (Jetmotor) eller på
 * kortets laveste 35 % (Bibliotek). Gir { name, tip } eller null. Kurven og
 * utkastet er urørt; det er bare tittelen som får et annet navn.
 */
export function curveNickname(pts) {
  if (!enabled() || !Array.isArray(pts) || pts.length < 2) return null;
  if (pts.every((p) => p[1] >= 100)) {
    mark('kallenavn');
    return { name: 'Jetmotor', tip: 'Alle punktene står på 100 %. Naboene hilser.' };
  }
  if (pts.every((p) => p[1] <= 35)) {
    mark('kallenavn');
    return { name: 'Bibliotek', tip: 'Alle punktene står på 35 %, så lavt kortet tillater. Hysj.' };
  }
  return null;
}

// ---------------------------------------------------------------- 9 · Hilsen i konsollen

function greet() {
  const A = app.state.app;
  const head = 'font: 600 13px/1.6 system-ui, sans-serif';
  const body = 'font: 12px/1.6 system-ui, sans-serif; color: #6f8396';
  console.info(
    `%c${A.name} ${A.channel} v${A.version} · bygg ${A.build}%c\nHei, nysgjerrige utvikler. Alt her leser bare; det som skriver, går til en simulert bakgrunnstjeneste.\nwindow.__app har tilstanden. Det finnes ${EGGS.length} små overraskelser – rigg.hint() gir et hint.`,
    head,
    body,
  );
}
/** rigg.hint() i konsollen: et hint om et egg du ikke har funnet ennå. */
function hint() {
  if (!enabled()) return 'Små overraskelser er slått av (Innstillinger › Generelt).';
  mark('konsoll');
  const left = EGGS.filter((e) => !found.has(e.id));
  if (!left.length) return `Du har funnet alle ${EGGS.length}. Fasiten står i docs/paaskeegg.md.`;
  const e = left[Math.floor(Math.random() * left.length)];
  return `${HINTS[e.id]} (${found.size} av ${EGGS.length} funnet)`;
}

// ---------------------------------------------------------------- 10 · Nordlys

const WORD = 'nordlys';
let typed = '';
function onType(e) {
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  const t = e.target;
  if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
  if (app.current !== 'lys') return void (typed = '');
  const lys = app.pages.lys?.egg;
  if (e.key === 'Escape' && lys?.active()) {
    lys.set(null);
    return;
  }
  if (e.key.length !== 1) return;
  typed = (typed + e.key.toLowerCase()).slice(-WORD.length);
  if (typed !== WORD || !enabled()) return;
  typed = '';
  lys?.set('nordlys');
  mark('nordlys');
  announce('Nordlys i forhåndsvisningen. Ingenting er lagret. Esc avslutter.');
}

/**
 * Nordlys som farger for én lysdiode. u: plassen langs sonen (0–1),
 * t: tid i sekunder. Tre langsomme bølger: én for styrken (gardinene), én
 * for fargen fra grønt til blågrønt og én som av og til drar inn fiolett.
 * Ingen gul, oransje eller rød. Styrken går aldri under 15 %, så ingenting blinker.
 */
export function aurora(u, t) {
  const TAU = Math.PI * 2;
  const w1 = 0.5 + 0.5 * Math.sin(u * TAU * 1.3 + t * 0.9);
  const w2 = 0.5 + 0.5 * Math.sin(u * TAU * 2.1 - t * 0.55 + 1.7);
  const w3 = 0.5 + 0.5 * Math.sin(u * TAU * 0.7 + t * 0.3 + 4.1);
  let hue = 128 + 48 * w2; // grønt → blågrønt
  if (w3 > 0.7) hue += (285 - hue) * ((w3 - 0.7) / 0.3); // innslag av fiolett
  const k = 0.15 + 0.85 * w1 * w1;
  return hslToRgb(hue, 0.85, 0.5).map((c) => c * k);
}

// ---------------------------------------------------------------- Bryteren og oppstart

/** Raden under Innstillinger › Generelt. switchRow er byggeklossen fra settings.js. */
export function surprisesRow(switchRow, save) {
  const n = found.size;
  const sub = `Noen få skjulte detaljer rundt om i appen. De endrer aldri tall, innstillinger eller kortet.${n ? ` Du har funnet ${n} av ${EGGS.length}.` : ''}`;
  return switchRow('Små overraskelser', sub, enabled(), (on) => save({ surprises: on }));
}

/** Slår av alt som synes nå (bryteren av), eller tegner det som hører til nå (på). */
function sync() {
  if (!enabled()) {
    for (const n of document.querySelectorAll('[data-egg-node]')) n.remove();
    for (const b of document.querySelectorAll('.egg-42')) b.classList.remove('egg-42');
    setTip($('#ov-temp-margin'), null);
    if (app.pages.lys?.egg?.active()) app.pages.lys.egg.set(null);
  }
  renderNight();
}

export function initEaster(a) {
  app = a;
  document.addEventListener('keydown', onKonami, true);
  document.addEventListener('keydown', onType);
  $('.app-icon').addEventListener('click', onIconClick);
  renderNight();
  // Månen kommer og går av seg selv. Én sjekk i minuttet, ingen animasjon.
  setInterval(renderNight, 60_000);
  app.on('settings', (keys) => keys.includes('surprises') && sync());
  if (enabled()) greet();
  window.rigg = Object.freeze({ hint });
  // For røyktesten og feilsøking: lista, hva som er funnet, og en klokke som kan stilles.
  window.__easter = Object.freeze({
    EGGS,
    enabled,
    found: () => [...found],
    forget() {
      found.clear();
      local.set('ag-eggs', []);
    },
    setNow(d) {
      nowFn = d ? () => new Date(d) : () => new Date();
      renderNight();
    },
  });
}
