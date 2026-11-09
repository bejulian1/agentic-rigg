// Statusblokken nederst i sidefeltet: faste verdier (STATUS_DEFS) og sensorer
// som er festet fra Sensorer, i rekkefølgen brukeren har valgt. Listen er
// innstillingen `statusItems` (lagres på serveren, høyst STATUS_MAX). Andre
// faner får beskjed gjennom BroadcastChannel i app.saveSettings().
//
// En rad på Sensorer som er det samme som en fast verdi (f.eks. gpu/temp og
// GPU-temp), festes som den faste verdien, så navn og format blir like.

import { STATUS_DEFS } from './defs.js';

export const statusMax = (app) => app.state.statusMax || 8;

/** Fast verdi som viser akkurat denne Sensorer-raden, eller null. */
export function defFor(rowId) {
  for (const [k, d] of Object.entries(STATUS_DEFS)) if (d.sensor === rowId && d.exact !== false) return k;
  return null;
}

/** ID-en som lagres når raden festes. */
export const pinKey = (rowId) => defFor(rowId) || rowId;

export const isPinned = (app, rowId) => (app.settings.statusItems || []).includes(pinKey(rowId));

/**
 * Navn og kilde for en verdi i statusblokken.
 * { label: kort navn i sidefeltet, long: fullt navn, src: kilde, sensor: rad-ID på Sensorer, def }
 */
export function describe(app, id) {
  const d = STATUS_DEFS[id];
  if (d) return { label: d.label, long: d.long, src: d.src === 'gpu' ? 'Skjermkort' : d.sensor === 'net' ? 'Nett (alle kort)' : d.sensor.startsWith('mem') ? 'Minne' : 'Prosessor', sensor: d.sensor, def: d };
  const r = app.sensors?.row(id);
  if (r) return { label: r.short, long: r.long, src: r.groupTitle, sensor: id, row: r };
  // Sensoren finnes ikke (ennå): f.eks. et nettkort som er borte.
  const tail = id.split('/').slice(1).join(' · ');
  return { label: tail, long: tail, src: 'Ikke tilgjengelig nå', sensor: id, missing: true };
}

/** Fester eller løsner en rad fra Sensorer. Gir false hvis blokken er full. */
export async function togglePin(app, rowId) {
  const key = pinKey(rowId);
  const cur = app.settings.statusItems || [];
  if (cur.includes(key)) {
    await app.saveSettings({ statusItems: cur.filter((x) => x !== key) });
    return true;
  }
  const max = statusMax(app);
  if (cur.length >= max) {
    app.toast(`Statusblokken er full (${max} av ${max})`, {
      body: 'Løsne en verdi først, eller bytt ut en under Innstillinger › Sidefelt.',
      action: { label: 'Åpne', run: () => app.openSettings?.('sidefelt') },
    });
    return false;
  }
  await app.saveSettings({ statusItems: [...cur, key] });
  return true;
}

/** Flytter en verdi ett steg opp (-1) eller ned (+1). */
export function moveItem(app, id, dir) {
  const cur = [...(app.settings.statusItems || [])];
  const i = cur.indexOf(id);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= cur.length) return false;
  [cur[i], cur[j]] = [cur[j], cur[i]];
  app.saveSettings({ statusItems: cur });
  return true;
}
