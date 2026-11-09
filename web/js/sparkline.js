// Små SVG-sparklinjer tegnet mot tid (ikke mot antall prøver), så alle
// linjer deler samme tidsakse uansett intervall. Der det ikke finnes
// historikk ennå, står bare en tynn stiplet grunnlinje (en grå flate i
// 18–22 px høyde så ut som en tom stolpe). De store grafene har skravert flate.

const NS = 'http://www.w3.org/2000/svg';
const W = 100;
const H = 20;
const GAP_MS = 5000;

export function sparkline(cls = 'spark') {
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('class', cls);
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('aria-hidden', 'true');
  const gather = document.createElementNS(NS, 'line');
  gather.setAttribute('class', 'gather');
  gather.setAttribute('y1', String(H - 0.5));
  gather.setAttribute('y2', String(H - 0.5));
  gather.setAttribute('x1', '0');
  const area = document.createElementNS(NS, 'path');
  area.setAttribute('class', 'area');
  const line = document.createElementNS(NS, 'path');
  line.setAttribute('class', 'line');
  svg.append(gather, area, line);
  return { svg, gather, area, line };
}

/**
 * pts: [[t, v], …] eldst først. span: tidsvindu i ms. fixed: [min, maks] eller null
 * (skaleres da mot min/maks i vinduet). minSpan: minste spenn ved skalering,
 * så en nesten flat temperatur (39–41 °C) ikke ser ut som store utslag.
 */
export function drawSpark(sp, pts, { now = Date.now(), span = 60_000, fixed = null, minSpan = 0 } = {}) {
  const t0 = now - span;
  let lo = Infinity;
  let hi = -Infinity;
  let firstT = null;
  for (const [t, v] of pts) {
    if (t < t0 - 2000 || v == null || !Number.isFinite(v)) continue;
    if (firstT == null) firstT = t;
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  if (firstT == null) {
    sp.line.setAttribute('d', '');
    sp.area.setAttribute('d', '');
    sp.gather.setAttribute('x2', String(W));
    return;
  }
  if (fixed) [lo, hi] = fixed;
  else if (hi - lo < minSpan) {
    const mid = (hi + lo) / 2;
    lo = mid - minSpan / 2;
    hi = mid + minSpan / 2;
  }
  if (hi - lo < 1e-9) {
    hi = lo + 1;
    lo -= 1;
  }
  const X = (t) => Math.max(0, ((t - t0) / span) * W);
  const Y = (v) => H - 1 - ((Math.min(hi, Math.max(lo, v)) - lo) / (hi - lo)) * (H - 2);
  let line = '';
  let area = '';
  let seg = '';
  let segX0 = null;
  let segX1 = null;
  let prevT = null;
  const close = () => {
    if (seg && segX0 !== segX1) area += `M${segX0},${H} ${seg.replace(/^M/, 'L')}L${segX1},${H}Z`;
  };
  for (const [t, v] of pts) {
    if (t < t0 - 2000 || v == null || !Number.isFinite(v)) continue;
    const x = X(t).toFixed(1);
    const y = Y(v).toFixed(1);
    if (prevT == null || t - prevT > GAP_MS) {
      close();
      seg = `M${x},${y}`;
      segX0 = x;
    } else seg += `L${x},${y}`;
    line += prevT == null || t - prevT > GAP_MS ? `M${x},${y}` : `L${x},${y}`;
    segX1 = x;
    prevT = t;
  }
  close();
  sp.line.setAttribute('d', line);
  sp.area.setAttribute('d', area);
  const gx = X(firstT);
  sp.gather.setAttribute('x2', gx > 1 ? gx.toFixed(1) : '0');
}
