// Kjøres før stilarket tegnes, så valgt tema, fargetema, kontrast, aksent, tetthet, skrift,
// animasjonsnivå, sidefeltets bredde og Innstillinger i hele vinduet ikke blinker ved lasting. Serveren skriver valgene inn i <html>
// (data-prefs="server"); da er de fasit, og bare det som avhenger av systemet
// (lyst/mørkt ved «System» og redusert bevegelse ved «Automatisk») regnes ut her.
// Uten serverens attributter brukes utdraget i localStorage. «System» for tema
// lagres ikke som attributt; da følger CSS prefers-color-scheme. data-motion
// settes alltid til et oppløst nivå (full/reduced/off).
(function () {
  var r = document.documentElement;
  var fromServer = r.getAttribute('data-prefs') === 'server';
  var p = {};
  if (!fromServer) {
    try {
      var t = localStorage.getItem('ag-theme');
      if (t === '"light"' || t === '"dark"' || t === 'light' || t === 'dark') r.setAttribute('data-theme', t.replace(/"/g, ''));
      p = JSON.parse(localStorage.getItem('ag-prefs') || '{}') || {};
    } catch (e) {}
    r.setAttribute('data-accent', p.accent || 'blue');
    r.setAttribute('data-palette', p.palette || 'grafitt');
    r.setAttribute('data-contrast', p.contrast ? 'high' : 'normal');
    r.setAttribute('data-density', p.density || 'standard');
    if (typeof p.fontScale === 'number' && p.fontScale >= 80 && p.fontScale <= 130) r.style.setProperty('--fs-scale', String(p.fontScale / 100));
    r.setAttribute('data-kbd', p.kbd ? 'on' : 'off');
    r.setAttribute('data-motion-pref', p.motion || 'system');
  }
  var th = r.getAttribute('data-theme');
  var dark = false;
  try {
    dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  } catch (e) {}
  r.setAttribute('data-resolved', th === 'light' || th === 'dark' ? th : dark ? 'dark' : 'light');
  var m = r.getAttribute('data-motion-pref') || 'system';
  if (m === 'system') {
    var reduce = false;
    try {
      reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    } catch (e) {}
    m = reduce ? 'reduced' : 'full';
  }
  r.setAttribute('data-motion', m);
  // Sidefeltet (v0.0.10): ikonlist hvis det var lagt sammen sist (utdraget i
  // localStorage) eller vinduet er smalt. app.js tar over når innstillingene er lest.
  var side = false;
  try {
    side = !!(JSON.parse(localStorage.getItem('ag-prefs') || '{}') || {}).side;
  } catch (e) {}
  var narrow = false;
  try {
    narrow = window.matchMedia('(max-width: 860px)').matches;
  } catch (e) {}
  r.setAttribute('data-side', side || narrow ? 'rail' : 'full');
  // Innstillinger dekker hele vinduet (v0.0.15). Lastes siden på nytt der, skal
  // sidefeltet ikke vises et øyeblikk før app.js tar over (route()).
  if (/^#\/?innstillinger(?:[?]|$)/.test(location.hash)) r.setAttribute('data-view', 'settings');
})();
