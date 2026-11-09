// Måler for Temperatur og effekt på Oversikt (v0.0.11). Erstatter skalaen
// med prikk og fargede soner.
//
//   ████████████░░░░░░░░░░░░│░░░░░░│░░
//                          83     97  100
//   Mål 83 °C · Nedklokking 97 °C · Avstenging 100 °C
//
// - Fyllet går fra skalaens start til verdien nå, så det er lett å se hvor
//   den er. Fargen sier nivået (aksent = normalt, gul = advarsel, rød =
//   fare), og det samme står i tekst ved siden av verdien og i verktøytipset.
// - Merkene er tynne streker med tallet rett under. Hva tallene betyr, står
//   i forklaringen under, så måleren kan leses uten farger.
// - Tallene skal aldri krasje: står to for tett (høye verdier, smale
//   vinduer), viker skalaens sluttall først, deretter merket lengst til
//   høyre. Forklaringen har alltid alle.
// - Temperatur og effekt bruker den samme måleren.

import { el } from './util.js';
import { Bar } from './meters.js';

export class Gauge {
  constructor(host, { label }) {
    this.fill = el('i');
    this.barEl = el('div', { class: 'bar g-bar' }, this.fill);
    this.bar = new Bar(this.barEl);
    // Strekene ligger i et eget lag over stolpen (stolpen klipper fyllet).
    this.ticksEl = el('div', { class: 'g-ticks', 'aria-hidden': 'true' });
    this.marksEl = el('div', { class: 'g-marks', 'aria-hidden': 'true' });
    this.legendEl = el('p', { class: 'g-legend' });
    this.root = el('div', { class: 'gauge', role: 'group', 'aria-label': label }, el('div', { class: 'g-track' }, this.barEl, this.ticksEl), this.marksEl, this.legendEl);
    host.replaceChildren(this.root);
    this.sig = '';
    this.ro = new ResizeObserver(() => this.layout());
    this.ro.observe(this.marksEl);
  }

  /**
   * min, max: skalaen. value: verdien nå. level: '' | 'warn' | 'danger'.
   * marks: [{ v, num, name }] – tall under streken og navnet i forklaringen.
   * end: { num, name } – skalaens slutt (f.eks. avstenging), eller null.
   * tip: tekst i verktøytipset.
   */
  set({ min, max, value, level = '', marks = [], end = null, tip = '' }) {
    const pos = (v) => Math.max(0, Math.min(100, ((v - min) / (max - min)) * 100));
    if (value != null && Number.isFinite(value)) this.bar.set((value - min) / (max - min), level);
    this.root.dataset.tip = tip;
    const sig = JSON.stringify([min, max, marks, end]);
    if (sig === this.sig) return;
    this.sig = sig;
    // Merkene på stolpen.
    this.ticksEl.replaceChildren(...marks.map((m) => el('span', { class: 'g-tick', style: { left: `${pos(m.v)}%` } })));
    // Tallene under: merkene først, skalaens slutt sist (den viker først).
    this.marksEl.replaceChildren(
      ...marks.map((m) => el('span', { class: 'g-num', 'data-at': String(pos(m.v)), text: m.num })),
      ...(end ? [el('span', { class: 'g-num g-end', 'data-at': '100', text: end.num })] : []),
    );
    // Forklaringen: alle navnene, i stigende rekkefølge.
    const items = [...marks.map((m) => [m.v, `${m.name} ${m.num}`]), ...(end ? [[max, `${end.name} ${end.num}`]] : [])].sort((a, b) => a[0] - b[0]);
    this.legendEl.replaceChildren(...items.flatMap(([, t], i) => (i ? [el('span', { class: 'sep', text: ' · ' }), el('span', { text: t })] : [el('span', { text: t })])));
    this.layout();
  }

  /** Plasserer tallene under merkene og skjuler dem som ville krasjet. */
  layout() {
    const w = this.marksEl.clientWidth;
    if (!w) return;
    const nums = [...this.marksEl.children];
    const placed = [];
    // Merkene i stigende rekkefølge, så slutten.
    const order = nums.filter((n) => !n.classList.contains('g-end')).sort((a, b) => a.dataset.at - b.dataset.at);
    const endEl = nums.find((n) => n.classList.contains('g-end'));
    if (endEl) order.push(endEl);
    for (const n of order) {
      n.style.visibility = 'hidden';
      const nw = n.offsetWidth;
      const at = (Number(n.dataset.at) / 100) * w;
      const left = n === endEl ? w - nw : Math.max(0, Math.min(w - nw, at - nw / 2));
      n.style.transform = `translateX(${Math.round(left)}px)`;
      const span = [left - 6, left + nw + 6];
      if (placed.some(([a, b]) => span[0] < b && span[1] > a)) continue;
      placed.push(span);
      n.style.visibility = '';
    }
  }

  tick(dt) {
    return this.bar.tick(dt);
  }
}
