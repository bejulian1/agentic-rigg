// Prosentstolper. Fyllet glattes i den felles rAF-løkka (ca. 0,2 s til riktig
// verdi), og første verdi settes direkte, så ingenting «kryper» opp ved
// oppstart. Fyllet flyttes med translateX (bare transform, ingen layout).
// Med data-motion 'reduced' eller 'off' hopper stolpen rett til verdien.
// Fargen settes med data-level på .bar (warn/danger) og toner i CSS.

import { clamp, motion, smooth } from './util.js';

export class Bar {
  /** bar: .bar-elementet (med <i> inni). warn/danger: brøkgrenser (0–1) for farge. */
  constructor(bar, { warn = null, danger = null } = {}) {
    this.box = bar;
    this.fill = bar.querySelector('i') || bar.appendChild(document.createElement('i'));
    this.warn = warn;
    this.danger = danger;
    this.value = 0;
    this.target = 0;
    this.has = false;
    this.shown = -1;
    this.level = '';
  }

  set(frac, level = null) {
    if (frac == null || !Number.isFinite(frac)) return;
    this.target = clamp(frac, 0, 1);
    if (!this.has) {
      this.has = true;
      this.value = this.target;
    }
    const lvl = level ?? (this.danger != null && this.target >= this.danger ? 'danger' : this.warn != null && this.target >= this.warn ? 'warn' : '');
    if (lvl !== this.level) {
      this.level = lvl;
      if (lvl) this.box.dataset.level = lvl;
      else delete this.box.dataset.level;
      // Fargen står også i ord (v0.0.11). Et eget verktøytips på stolpen beholdes.
      if (this.baseTip === undefined) this.baseTip = this.box.dataset.tip || '';
      const word = lvl === 'danger' ? 'Rødt: over den røde grensen.' : lvl === 'warn' ? 'Gult: over varselgrensen.' : '';
      const tip = [this.baseTip, word].filter(Boolean).join(' ');
      if (tip) this.box.dataset.tip = tip;
      else delete this.box.dataset.tip;
    }
  }

  /** Returnerer true så lenge stolpen fortsatt glir. */
  tick(dt) {
    if (!this.has) return false;
    if (motion() !== 'full') this.value = this.target;
    else this.value = smooth(this.value, this.target, dt, 0.06);
    if (Math.abs(this.value - this.target) < 0.0005) this.value = this.target;
    const q = Math.round(this.value * 2000);
    if (q !== this.shown) {
      this.shown = q;
      // translateX i stedet for scaleX: den runde enden beholder formen.
      this.fill.style.transform = `translateX(${(q / 20 - 100).toFixed(2)}%)`;
    }
    return this.value !== this.target;
  }
}

/** Nivå for en verdi mot to terskler. */
export const level = (v, warn, crit) => (v == null ? '' : crit != null && v >= crit ? 'danger' : warn != null && v >= warn ? 'warn' : '');

/** Hva gul og rød betyr, i ord (v0.0.11: verktøytips på verdier med varselfarge). */
export const LEVEL_TEXT = { warn: 'over gul varselgrense (Innstillinger › Sidefelt)', danger: 'over rød varselgrense (Innstillinger › Sidefelt)' };
