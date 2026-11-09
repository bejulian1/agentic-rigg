//! Nedsampling for grafer (arkitektur.md 6.4): høyst to punkter per
//! pikselkolonne (min og maks i kolonnen), regnet når nye data kommer, ikke i
//! `snapshot()`. Grafagenten (2A) bygger videre her.

/// Min og maks per kolonne for punkter `(t, verdi)` sortert etter tid.
/// Kolonner uten data er `None` (hull).
pub fn minmax_columns(points: &[(i64, Option<f64>)], t0: i64, t1: i64, columns: usize) -> Vec<Option<(f64, f64)>> {
    let mut out = vec![None; columns];
    if columns == 0 || t1 <= t0 {
        return out;
    }
    let span = (t1 - t0) as f64;
    for &(t, v) in points {
        let Some(v) = v.filter(|v| v.is_finite()) else { continue };
        if t < t0 || t > t1 {
            continue;
        }
        let c = (((t - t0) as f64 / span) * columns as f64).floor() as usize;
        let c = c.min(columns - 1);
        out[c] = Some(match out[c] {
            None => (v, v),
            Some((lo, hi)) => (lo.min(v), hi.max(v)),
        });
    }
    out
}

/// Største og minste verdi, med et minste spenn (sparklinjene: små
/// svingninger skal ikke se dramatiske ut). `fixed` er et foretrukket
/// område (prosent: 0–100), ikke en grense: verdier utenfor (en vifte NVML
/// melder til 105 %) utvider området, så toppen aldri kuttes (runde 8A).
pub fn range(values: impl Iterator<Item = f64>, min_span: f64, fixed: Option<(f64, f64)>) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if let Some((a, b)) = fixed {
        return Some(if lo.is_finite() { (a.min(lo), b.max(hi)) } else { (a, b) });
    }
    if !lo.is_finite() {
        return None;
    }
    if hi - lo < min_span {
        let mid = (hi + lo) / 2.0;
        lo = mid - min_span / 2.0;
        hi = mid + min_span / 2.0;
    }
    if hi - lo < 1e-9 {
        hi = lo + 1.0;
    }
    Some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kolonner_og_hull() {
        let pts = [(0, Some(1.0)), (1, Some(5.0)), (9, None), (10, Some(3.0))];
        let c = minmax_columns(&pts, 0, 10, 5);
        assert_eq!(c[0], Some((1.0, 5.0)));
        assert_eq!(c[2], None);
        assert_eq!(c[4], Some((3.0, 3.0)));
        assert_eq!(range([40.0, 41.0].into_iter(), 20.0, None), Some((30.5, 50.5)));
    }

    #[test]
    fn fast_område_kutter_ikke() {
        let pct = Some((0.0, 100.0));
        assert_eq!(range([3.0, 40.0].into_iter(), 0.0, pct), Some((0.0, 100.0)));
        assert_eq!(range([35.0, 105.0].into_iter(), 0.0, pct), Some((0.0, 105.0)), "viften over 100 % kuttes ikke");
        assert_eq!(range(std::iter::empty(), 0.0, pct), Some((0.0, 100.0)));
        // Uten fast område følger linja dataene, også ved 110 °C og under 20 °C.
        assert_eq!(range([12.0, 110.0].into_iter(), 20.0, None), Some((12.0, 110.0)));
    }
}
