//! Regnestykkene bak grafene, uten GTK (testes i `rigg/tests/chart.rs`):
//! pene akser, tidsaksen, hull, nedsampling til pikselkolonner, båndet,
//! tekstene i verktøytipset og CSV-eksporten. Port av `web/js/charts.js`.
//!
//! Alt her er rene funksjoner av tall inn og tall ut, så de kan kjøres i en
//! test uten skjerm. Tidspunkter er ms siden 1970, og tidssonen kommer inn
//! som forskyvning fra UTC i sekunder (vinduet slår den opp med libc).

use rigg_core::{tr, trf};
use rigg_core::fmt as F;

/// Tidsvinduene brukeren kan velge (sekunder, kort navn med U+202F).
pub const WINDOWS: [(i64, &str); 7] = [
    (60, "1\u{202f}min"),
    (300, "5\u{202f}min"),
    (900, "15\u{202f}min"),
    (3600, "1\u{202f}t"),
    (21600, "6\u{202f}t"),
    (86400, "24\u{202f}t"),
    (172800, "48\u{202f}t"),
];

fn window_choice_label(sec: i64) -> Option<&'static str> {
    Some(match sec {
        60 => tr!("1\u{202f}min"),
        300 => tr!("5\u{202f}min"),
        900 => tr!("15\u{202f}min"),
        3600 => tr!("1\u{202f}t"),
        21600 => tr!("6\u{202f}t"),
        86400 => tr!("24\u{202f}t"),
        172800 => tr!("48\u{202f}t"),
        _ => return None,
    })
}

pub fn window_choices() -> impl Iterator<Item = (i64, &'static str)> {
    WINDOWS.into_iter().map(|(sec, source)| (sec, window_choice_label(sec).unwrap_or(source)))
}

pub fn window_label(sec: i64) -> String {
    window_choice_label(sec).map(str::to_owned).unwrap_or_else(|| trf!("{verdi}\u{202f}min", verdi = sec / 60))
}

/// Lange vinduer får klokkeslett på aksen og min/maks-bånd.
pub fn is_long(sec: i64) -> bool {
    sec > 300
}

// ---------------------------------------------------------------- Y-aksen

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
}

/// Lineær overføring til plottet. Klipp først ved tegning, ellers endres
/// helningen når et endepunkt ligger like utenfor tids- eller verdiaksen.
pub fn y_position(value: f64, scale: Scale, top: f32, height: f32) -> f32 {
    top + height - ((value - scale.lo) / (scale.hi - scale.lo).max(1e-9)) as f32 * height
}

/// Pene aksemerker (1-2-2,5-5 × 10ⁿ), som demoens `niceScale`.
pub fn nice_scale(mut min: f64, mut max: f64, max_ticks: usize) -> Scale {
    if !min.is_finite() || !max.is_finite() {
        return Scale { lo: 0.0, hi: 1.0, step: 0.25 };
    }
    if max - min < 1e-9 {
        let pad = if max.abs() > 1.0 { max.abs() * 0.1 } else { 1.0 };
        min -= pad;
        max += pad;
    }
    let raw = (max - min) / (max_ticks.max(2) - 1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let nice = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 2.5 {
        2.5
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    let step = nice * mag;
    Scale { lo: (min / step + 1e-9).floor() * step, hi: (max / step - 1e-9).ceil() * step, step }
}

/// Hvordan y-aksen til en graf skal oppføre seg (runde 8A).
///
/// `pref` er det **foretrukne** området: det aksen viser når dataene ligger
/// innenfor (GPU-temperatur 20–100 °C, effekt 0–grensen). Det er ikke en
/// grense. Går dataene over eller under, utvides aksen til en pen skala som
/// får med alt, med litt luft, både med «Tilpass» og «Fast». Bare `hard`
/// klemmer, og den brukes bare der verdien ikke kan gå utenfor (last og
/// minne i prosent; NVML kan melde vifter over 100 %, så de har ingen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub pref: (f64, f64),
    pub hard: Option<(f64, f64)>,
    /// Minste spenn med «Tilpass», så en nesten flat kurve (38–40 °C) ser flat ut.
    pub min_span: f64,
    /// «Tilpass»: aksen følger dataene innenfor det foretrukne området.
    pub auto: bool,
}

/// Luft over og under når dataene går utenfor det foretrukne området
/// (andel av spennet).
pub const AXIS_PAD_OUT: f64 = 0.06;
/// En grenselinje (nedklokking 97 °C) så nær kanten tas med i aksen
/// (andel av spennet), så den synes når kurven nærmer seg.
pub const MARK_NEAR: f64 = 0.15;

/// Y-aksen for en graf. `data`: laveste og høyeste verdi i vinduet (med
/// båndet). `marks`: grenselinjer (nedklokking, avstenging) som tas med når
/// de ligger like utenfor. Kutter aldri en verdi utenfor `hard`.
pub fn y_axis(data: Option<(f64, f64)>, a: &Axis, marks: &[f64]) -> Scale {
    let (mut plo, mut phi) = a.pref;
    if !plo.is_finite() || !phi.is_finite() {
        (plo, phi) = (0.0, 1.0);
    }
    if phi <= plo {
        phi = plo + 1.0;
    }
    let hard = |v: f64| match a.hard {
        Some((l, h)) => v.clamp(l, h),
        None => v,
    };
    let data = data.filter(|d| d.0.is_finite() && d.1.is_finite()).map(|(l, h)| (hard(l.min(h)), hard(l.max(h))));
    // 1. Rammen: det foretrukne området, utvidet til dataene med litt luft.
    let (mut blo, mut bhi) = (plo, phi);
    let (mut ext_lo, mut ext_hi) = (false, false);
    if let Some((dlo, dhi)) = data {
        let pad = (phi.max(dhi) - plo.min(dlo)) * AXIS_PAD_OUT;
        if dlo < plo {
            blo = hard(dlo - pad);
            ext_lo = true;
        }
        if dhi > phi {
            bhi = hard(dhi + pad);
            ext_hi = true;
        }
    }
    // 2. «Tilpass»: følg dataene innenfor rammen. «Fast»: hele rammen.
    let (mut lo, mut hi) = match data {
        Some((dlo, dhi)) if a.auto => {
            let (mut lo, mut hi) = (dlo, dhi);
            let min_span = a.min_span.min(bhi - blo);
            if hi - lo < min_span {
                let mid = (hi + lo) / 2.0;
                lo = mid - min_span / 2.0;
                hi = mid + min_span / 2.0;
                if lo < blo {
                    hi += blo - lo;
                    lo = blo;
                }
                if hi > bhi {
                    lo -= hi - bhi;
                    hi = bhi;
                }
            } else {
                let pad = ((hi - lo) * 0.12).max((bhi - blo) * 0.03);
                lo = (lo - pad).max(blo);
                hi = (hi + pad).min(bhi);
            }
            (lo, hi)
        }
        _ => (blo, bhi),
    };
    // 3. Grenselinjer like utenfor tas med, så de synes før kurven når dem.
    let span = (hi - lo).max(1e-9);
    for &m in marks.iter().filter(|m| m.is_finite()) {
        let m = hard(m);
        if m > hi && m <= hi + span * MARK_NEAR {
            hi = m;
            ext_hi |= m > bhi;
        }
        if m < lo && m >= lo - span * MARK_NEAR {
            lo = m;
            ext_lo |= m < blo;
        }
    }
    // 4. Pene merker. Innenfor rammen holdes kanten (20 °C står nederst);
    //    utenfor får aksen merket under eller over (0, 125 °C).
    // Fem merker, eller seks når det gir en tettere akse (8–117 °C blir
    // 0–125 i steg på 25, ikke 0–150 i steg på 50).
    let (n5, n6) = (nice_scale(lo, hi, 5), nice_scale(lo, hi, 6));
    let n = if n6.hi - n6.lo < n5.hi - n5.lo - 1e-9 { n6 } else { n5 };
    let lo = if ext_lo { n.lo } else { n.lo.max(blo) };
    let hi = if ext_hi { n.hi } else { n.hi.min(bhi) };
    let (lo, hi) = (hard(lo), hard(hi));
    Scale { lo, hi: if hi > lo { hi } else { lo + n.step.max(1e-9) }, step: n.step }
}

/// Så lenge aksen holdes før den krymper (ms).
pub const AXIS_HOLD_MS: i64 = 15_000;

/// Hindrer at aksen pumper: den **utvides straks** når dataene trenger mer
/// plass (ingenting kuttes), men **krymper først** når den har ønsket seg
/// mindre i [`AXIS_HOLD_MS`] i strekk, og da bare til det største den har
/// ønsket seg i den tiden. En kurve som veksler mellom 40 og 48 °C, står
/// derfor på samme akse.
#[derive(Clone, Debug, Default)]
pub struct AxisHold {
    cur: Option<Scale>,
    /// Når aksen begynte å ønske seg mindre, og det største den har ønsket seg siden.
    shrink: Option<(i64, Scale)>,
}

/// Den minste pene aksen som har plass til begge.
fn union(a: Scale, b: Scale) -> Scale {
    let (lo, hi) = (a.lo.min(b.lo), a.hi.max(b.hi));
    if lo == a.lo && hi == a.hi {
        return a;
    }
    if lo == b.lo && hi == b.hi {
        return b;
    }
    let n = nice_scale(lo, hi, 5);
    Scale { lo: n.lo.max(lo), hi: n.hi.min(hi), step: n.step }
}

impl AxisHold {
    pub fn reset(&mut self) {
        self.cur = None;
        self.shrink = None;
    }

    /// Uten data i vinduet (historikken hentes på nytt, alt er skjult): aksen
    /// som står, ellers `want`. Lagrer ingenting, så aksen ikke hopper til
    /// hele det foretrukne området og blir stående der.
    pub fn idle(&self, want: Scale) -> Scale {
        self.cur.unwrap_or(want)
    }

    /// Aksen som skal tegnes nå, når grafen ønsker seg `want`.
    pub fn apply(&mut self, want: Scale, now: i64) -> Scale {
        let Some(cur) = self.cur else {
            self.cur = Some(want);
            return want;
        };
        let eps = (cur.hi - cur.lo).abs().max(1e-9) * 1e-6;
        if want.lo < cur.lo - eps || want.hi > cur.hi + eps {
            // Utvid straks; den andre kanten holdes.
            let next = union(cur, want);
            self.cur = Some(next);
            self.shrink = None;
            return next;
        }
        if (want.lo - cur.lo).abs() <= eps && (want.hi - cur.hi).abs() <= eps {
            self.shrink = None;
            return cur;
        }
        let (since, peak) = match self.shrink {
            Some((t, p)) => (t, union(p, want)),
            None => (now, want),
        };
        if now - since < AXIS_HOLD_MS {
            self.shrink = Some((since, peak));
            return cur;
        }
        // Ventet lenge nok: krymp til det største som er ønsket i perioden.
        self.shrink = None;
        if (peak.lo - cur.lo).abs() <= eps && (peak.hi - cur.hi).abs() <= eps {
            return cur;
        }
        self.cur = Some(peak);
        peak
    }
}

/// En kritisk grense som strek i grafen (nedklokking, avstenging), i
/// verdien som vises (°C eller °F).
#[derive(Clone, Debug, PartialEq)]
pub struct LimitLine {
    pub v: f64,
    pub label: String,
    /// «warn» eller «danger».
    pub level: &'static str,
}

/// Tekstene til grenselinjene: y (piksler) for hver linje som er innenfor
/// aksen, og om teksten får plass uten å kollidere med en annen. Linjene
/// sorteres ovenfra; en tekst som ville truffet den forrige, skjules (linja
/// står). `text_h`: høyden på teksten.
pub fn limit_labels(ys: &[Option<f32>], text_h: f32) -> Vec<bool> {
    let mut order: Vec<usize> = (0..ys.len()).filter(|i| ys[*i].is_some()).collect();
    order.sort_by(|a, b| ys[*a].unwrap_or(0.0).total_cmp(&ys[*b].unwrap_or(0.0)));
    let mut out = vec![false; ys.len()];
    let mut last: Option<f32> = None;
    for i in order {
        let y = ys[i].unwrap_or(0.0);
        if last.is_none_or(|l| y - l >= text_h + 2.0) {
            out[i] = true;
            last = Some(y);
        }
    }
    out
}

/// Desimaler på y-aksen: så mange som steget trenger (0,25 → 2, 2,5 → 1,
/// 20 → 0), høyst 3. Før ga 0,25 «0,2 0,5 0,8» og 2,5 «3 5 8» (runde 8A).
pub fn axis_decimals(step: f64) -> usize {
    if !step.is_finite() || step <= 0.0 {
        return 0;
    }
    let mut d = 0;
    while d < 3 {
        let x = step * 10f64.powi(d as i32);
        if (x - x.round()).abs() < 1e-6 * x.max(1.0) {
            break;
        }
        d += 1;
    }
    d
}

/// Merkene på y-aksen, nedenfra.
pub fn y_ticks(s: &Scale) -> Vec<f64> {
    let mut out = Vec::new();
    if s.step <= 0.0 || !s.step.is_finite() {
        return out;
    }
    let mut v = (s.lo / s.step - 1e-6).ceil() * s.step;
    while v <= s.hi + s.step * 0.001 && out.len() < 50 {
        out.push(if v.abs() < s.step * 1e-6 { 0.0 } else { v });
        v += s.step;
    }
    out
}

// ---------------------------------------------------------------- Tidsaksen

/// «nå», «−45 s», «−4 min» (som demoens `ago`).
pub fn ago(sec: i64, window_sec: i64) -> String {
    if sec <= 0 {
        return tr!("nå").into();
    }
    if (window_sec > 120 || window_sec == 0) && sec % 60 == 0 {
        return format!("{}{}{}min", F::MINUS, sec / 60, F::NNBSP);
    }
    format!("{}{}{}s", F::MINUS, sec, F::NNBSP)
}

/// Et merke på tidsaksen: tidspunkt (ms) og tekst.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub t: i64,
    pub label: String,
}

/// Relative merker for korte vinduer (opptil 5 min): «−15 s» … «−45 s».
/// `t_now` er høyre kant.
pub fn rel_ticks(window_sec: i64, t_now: i64) -> Vec<Tick> {
    let step = match window_sec {
        60 => 15,
        300 => 60,
        w => (w / 4).max(15),
    };
    let mut out = Vec::new();
    let mut s = step;
    while s < window_sec {
        out.push(Tick { t: t_now - s * 1000, label: ago(s, window_sec) });
        s += step;
    }
    out
}

const CLOCK_STEPS: [i64; 11] = [60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200];

/// Klokkeslett på hele tidspunkter for lange vinduer, og dato («5. nov.»)
/// der døgnet skifter. Ett merke per ca. 64 px (så 24 t i en smal graf får
/// 00:00, 06:00, 12:00 og 18:00).
pub fn clock_ticks(t0: i64, t_now: i64, plot_w: f32, utc_offset_s: i64) -> Vec<Tick> {
    let span = (t_now - t0).max(1);
    let max_ticks = ((plot_w / 64.0).floor() as i64).max(2);
    let step = CLOCK_STEPS.iter().map(|s| s * 1000).find(|s| span / s <= max_ticks).unwrap_or(CLOCK_STEPS[CLOCK_STEPS.len() - 1] * 1000);
    let off = utc_offset_s * 1000;
    let mut tk = (t0 + off).div_euclid(step) * step - off;
    if tk < t0 {
        tk += step;
    }
    let mut out = Vec::new();
    while tk < t_now && out.len() < 64 {
        let lt = F::local(tk, utc_offset_s);
        let label = if lt.hour == 0 && lt.minute == 0 { day_short(tk, utc_offset_s) } else { F::time(lt, false) };
        out.push(Tick { t: tk, label });
        tk += step;
    }
    out
}

/// «6. nov.»: dato på tidsaksen der døgnet skifter.
pub fn day_short(t: i64, utc_offset_s: i64) -> String {
    let lt = F::local(t, utc_offset_s);
    F::month_day(lt.month, lt.day)
}

/// «i dag 16:43», «i går 23:10» eller «3. nov. 14:32».
pub fn when(t: i64, now: i64, utc_offset_s: i64, seconds: bool) -> String {
    let lt = F::local(t, utc_offset_s);
    let hm = F::time(lt, seconds);
    let day = |ms: i64| (ms / 1000 + utc_offset_s).div_euclid(86_400);
    let (d, dn) = (day(t), day(now));
    if d == dn {
        trf!("i dag {hm}", hm = hm)
    } else if d == dn - 1 {
        trf!("i går {hm}", hm = hm)
    } else {
        format!("{} {hm}", day_short(t, utc_offset_s))
    }
}

/// ISO 8601 med sone: «2026-11-07T14:32:10+01:00» (CSV-eksporten).
pub fn iso_time(t: i64, utc_offset_s: i64) -> String {
    let lt = F::local(t, utc_offset_s);
    let sign = if utc_offset_s < 0 { '-' } else { '+' };
    let o = utc_offset_s.abs();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}", lt.year, lt.month, lt.day, lt.hour, lt.minute, lt.second, o / 3600, o % 3600 / 60)
}

/// Hvor mange ms én piksel er. Grafen flytter seg bare når tidsaksen har
/// gått en hel piksel (arkitektur.md 6.4).
pub fn ms_per_px(span_ms: i64, plot_px: f32) -> f64 {
    span_ms as f64 / (plot_px.max(1.0) as f64)
}

/// Hvor ofte grafen vekkes (ms): hver piksel, men høyst hvert `min_ms`.
/// Grafene på en side vekkes på det samme rutenettet i tid (hele multipler
/// av dette fra 1970), så de tegnes i samme bilde. Hver tegning står likevel
/// på hele piksler ([`quantize`]), så en 1 min-graf går to–tre piksler om
/// gangen i stedet for å tegne 20 ganger i sekundet.
pub fn scroll_tick(ms_px: f64, min_ms: f64) -> f64 {
    ms_px.max(min_ms).max(1.0)
}

/// «Nå» rundet ned til hele piksler, så hver ny tegning flytter alt nøyaktig
/// én piksel (skarpt og uten flimmer).
pub fn quantize(now: i64, ms_px: f64) -> i64 {
    if ms_px <= 1.0 {
        return now;
    }
    ((now as f64 / ms_px).floor() * ms_px) as i64
}

/// Hvor lenge til tidsaksen har flyttet seg én hel piksel (ms).
pub fn next_pixel_in(now: i64, ms_px: f64) -> u64 {
    if ms_px <= 1.0 {
        return 16;
    }
    let next = ((now as f64 / ms_px).floor() + 1.0) * ms_px;
    ((next - now as f64).ceil() as u64).max(4)
}

// ---------------------------------------------------------------- Hull

/// Lengre opphold enn dette mellom to punkter gir brudd i linja. Bøttene i
/// lange vinduer, og takten i bakgrunnen (2 eller 5 s), er ikke brudd.
pub fn gap_ms(step_ms: i64, cadence_ms: i64) -> i64 {
    5000i64.max((step_ms as f64 * 2.5) as i64).max(cadence_ms * 3)
}

/// Første indeks med `t >= t_min` (binærsøk, `t` er sortert).
pub fn first_index(t: &[i64], t_min: i64) -> usize {
    t.partition_point(|x| *x < t_min)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleKind {
    /// Før første punkt i vinduet.
    Lead,
    /// Mellom to punkter.
    Gap,
    /// Etter siste punkt: kilden har stoppet (kortet svarer ikke). Lages av
    /// grafen selv, ikke av [`holes`].
    Tail,
}

/// Skraverte flater: før første punkt og der det mangler data.
/// `first`: tidspunktet dataene begynner (bøttas start i lange vinduer).
pub fn holes(t: &[i64], start: usize, t0: i64, t_now: i64, gap: i64, first: Option<i64>) -> Vec<(i64, i64, HoleKind)> {
    let mut out = Vec::new();
    let span = t_now - t0;
    match first {
        None => out.push((t0, t_now, HoleKind::Lead)),
        Some(f) => {
            let f = f.max(t0);
            if f - t0 > gap.min(span / 100) {
                out.push((t0, f, HoleKind::Lead));
            }
        }
    }
    for i in start.max(1)..t.len() {
        let (a, b) = (t[i - 1], t[i]);
        if b - a > gap && b > t0 && a < t_now {
            out.push((a.max(t0), b.min(t_now), HoleKind::Gap));
        }
    }
    out
}

// ---------------------------------------------------------------- Linja og båndet

/// Én serie slik grafen ser den: snitt per punkt, og min og maks i lange
/// vinduer (bøtter).
pub struct SeriesView<'a> {
    pub avg: &'a [Option<f64>],
    pub lo: Option<&'a [Option<f64>]>,
    pub hi: Option<&'a [Option<f64>]>,
    /// Trapp (effektgrensen): verdien holdes til neste punkt.
    pub step: bool,
    pub band: bool,
}

/// Et sammenhengende stykke av en serie. `line` er (x, y); `band` er
/// (x, y for maks, y for min).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seg {
    pub line: Vec<(f32, f32)>,
    pub band: Vec<(f32, f32, f32)>,
}

/// Mapping fra tid og verdi til piksler.
pub struct Mapping<'a> {
    pub x: &'a dyn Fn(i64) -> f32,
    pub y: &'a dyn Fn(f64) -> f32,
    /// Omregning av verdien (°F, MB/s) før y.
    pub val: &'a dyn Fn(f64) -> f64,
}

fn snap(y: f32) -> f32 {
    y.round() + 0.5
}

/// Linje og bånd for én serie, delt der det mangler data. Er det flere
/// punkter enn piksler, slås de sammen per pikselkolonne: snitt for linja,
/// laveste min og høyeste maks for båndet. Grafen tegner derfor høyst ett
/// punkt per kolonne, og toppene forsvinner aldri.
#[allow(clippy::needless_range_loop)] // samme indeks i tid, snitt, min og maks
pub fn segments(t: &[i64], s: &SeriesView, start: usize, t_now: i64, gap: i64, m: &Mapping, plot_w: f32) -> Vec<Seg> {
    let n = t.len().min(s.avg.len());
    let mut out: Vec<Seg> = Vec::new();
    let mut cur = Seg::default();
    let mut prev_t: Option<i64> = None;
    let dense = (n.saturating_sub(start)) as f32 > plot_w * 1.2;
    // (x, sum, n, lo, hi)
    let mut col: Option<(f32, f64, u32, f64, f64)> = None;
    let get = |arr: Option<&[Option<f64>]>, i: usize| arr.and_then(|a| a.get(i).copied().flatten()).map(m.val);
    let flush_col = |col: &mut Option<(f32, f64, u32, f64, f64)>, cur: &mut Seg| {
        if let Some((x, sum, k, lo, hi)) = col.take() {
            let v = sum / k as f64;
            let y = (m.y)(v);
            cur.line.push((x, if s.step { snap(y) } else { y }));
            if s.band && lo.is_finite() && hi.is_finite() {
                cur.band.push((x, (m.y)(hi), (m.y)(lo)));
            }
        }
    };
    for i in start..n {
        let Some(raw) = s.avg[i] else { continue };
        let v = (m.val)(raw);
        let ti = t[i];
        if let Some(p) = prev_t
            && ti - p > gap
        {
            flush_col(&mut col, &mut cur);
            if !cur.line.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
        if ti > t_now {
            // Siste stykke fram til «nå» interpoleres for jevn rulling.
            if let Some(p) = prev_t
                && !cur.line.is_empty()
                && ti - p <= gap
                && let Some(pv) = s.avg[..i].iter().rev().find_map(|x| *x).map(m.val)
            {
                flush_col(&mut col, &mut cur);
                let f = (t_now - p) as f64 / (ti - p).max(1) as f64;
                let y = if s.step { snap((m.y)(pv)) } else { (m.y)(pv + (v - pv) * f) };
                cur.line.push(((m.x)(t_now), y));
            }
            break;
        }
        prev_t = Some(ti);
        let x = (m.x)(ti);
        let lo = if s.band { get(s.lo, i) } else { None };
        let hi = if s.band { get(s.hi, i) } else { None };
        if dense {
            let cx = x.round();
            if col.as_ref().is_some_and(|c| c.0 != cx) {
                flush_col(&mut col, &mut cur);
            }
            let c = col.get_or_insert((cx, 0.0, 0, f64::INFINITY, f64::NEG_INFINITY));
            c.1 += v;
            c.2 += 1;
            c.3 = c.3.min(lo.unwrap_or(v));
            c.4 = c.4.max(hi.unwrap_or(v));
            continue;
        }
        if s.step {
            let y = snap((m.y)(v));
            if let Some(last) = cur.line.last().copied()
                && last.1 != y
            {
                cur.line.push((x, last.1));
            }
            cur.line.push((x, y));
        } else {
            cur.line.push((x, (m.y)(v)));
        }
        if let (Some(lo), Some(hi)) = (lo, hi) {
            cur.band.push((x, (m.y)(hi), (m.y)(lo)));
        }
    }
    flush_col(&mut col, &mut cur);
    if !cur.line.is_empty() {
        out.push(cur);
    }
    // Uten båndpunkter for hele stykket (korte vinduer) er det ikke noe bånd.
    for sg in &mut out {
        if !dense && sg.band.len() * 3 < sg.line.len() * 2 {
            sg.band.clear();
        }
    }
    out
}

/// Hvor sterkt båndet tegnes for en høyde (andel av plottet). Et høyt bånd
/// (minneklokka som hopper mellom 405 og 7000 MHz i hver bøtte) ble tunge
/// blokker i demoen; nå blir det et svakt slør, mens et smalt bånd
/// (temperatur ± 2 °C) står tydelig. Fire trinn, så båndet kan tegnes i
/// få stykker.
pub fn band_level(frac: f32) -> f32 {
    let a = (1.0 - 0.75 * frac.clamp(0.0, 1.0)).clamp(0.25, 1.0);
    (a * 4.0).round() / 4.0
}

/// Båndet delt i stykker med samme styrke: (første, siste punkt, andel av
/// grunnstyrken). Stykkene deler endepunktet, så det ikke blir glipper.
/// Høyden glattes over to naboer på hver side, så styrken ikke flimrer.
pub fn band_runs(band: &[(f32, f32, f32)], plot_h: f32) -> Vec<(usize, usize, f32)> {
    let n = band.len();
    let mut out: Vec<(usize, usize, f32)> = Vec::new();
    if n == 0 || plot_h <= 0.0 {
        return out;
    }
    let h: Vec<f32> = band.iter().map(|b| (b.2 - b.1).max(0.0)).collect();
    let level = |i: usize| {
        let a = i.saturating_sub(2);
        let b = (i + 2).min(n - 1);
        let m = h[a..=b].iter().fold(0.0f32, |x, y| x.max(*y));
        band_level(m / plot_h)
    };
    let mut start = 0;
    let mut cur = level(0);
    for i in 1..n {
        let l = level(i);
        if l != cur {
            out.push((start, i, cur));
            start = i;
            cur = l;
        }
    }
    out.push((start, n - 1, cur));
    out
}

/// Laveste og høyeste verdi i vinduet (med båndet), for y-aksen og «min ·
/// maks» i overskriften.
///
/// Området som faktisk kan tegnes i vinduet. Punktene utenfor brukes bare
/// til interpolasjon ved kanten, med samme hullgrense som [`segments`].
/// Framtidsbøttas snitt kan bidra fram til «nå», men ikke hele toppen eller
/// båndet: `segments` forlenger bare snittlinja ved høyre kant.
#[allow(clippy::needless_range_loop)] // samme indeks i tid, snitt, min og maks
pub fn window_range(t: &[i64], s: &SeriesView, t0: i64, t_now: i64, gap: i64, val: &dyn Fn(f64) -> f64) -> Option<(f64, f64)> {
    if t_now < t0 {
        return None;
    }
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut include = |v: f64| {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    };
    let interpolate = |ta: i64, tb: i64, a: f64, b: f64, at: i64| a + (b - a) * ((at - ta) as f64 / (tb - ta).max(1) as f64);
    let mut prev: Option<(i64, f64)> = None;
    let mut band_prev: Option<(i64, f64, f64)> = None;
    let start = first_index(t, t0.saturating_sub(gap));
    for i in start..t.len().min(s.avg.len()) {
        let Some(raw) = s.avg[i] else { continue };
        let a = val(raw);
        let ti = t[i];
        let connected = prev.is_some_and(|(p, _)| ti - p <= gap);
        if !connected {
            band_prev = None;
        }
        if ti > t_now {
            if connected && let Some((p, pv)) = prev {
                for at in [t0.max(p), t_now] {
                    include(if s.step { pv } else { interpolate(p, ti, pv, a, at) });
                }
            }
            break;
        }
        if ti >= t0 {
            include(a);
            if connected && let Some((p, pv)) = prev && p < t0 {
                include(if s.step { pv } else { interpolate(p, ti, pv, a, t0) });
            }
        }
        if s.band {
            let l = s.lo.and_then(|x| x.get(i).copied().flatten()).map(val);
            let h = s.hi.and_then(|x| x.get(i).copied().flatten()).map(val);
            if let (Some(l), Some(h)) = (l, h) {
                if ti >= t0 {
                    include(l);
                    include(h);
                    if let Some((p, pl, ph)) = band_prev && p < t0 {
                        include(interpolate(p, ti, pl, l, t0));
                        include(interpolate(p, ti, ph, h, t0));
                    }
                }
                band_prev = Some((ti, l, h));
            }
        }
        prev = Some((ti, a));
    }
    lo.is_finite().then_some((lo, hi))
}

/// Punktet nærmest tidspunktet `th` (markøren), høyst `max_dist` unna.
pub fn nearest(t: &[i64], start: usize, th: i64, t_now: i64, max_dist: i64) -> Option<usize> {
    if t.is_empty() {
        return None;
    }
    let a = first_index(t, th).max(start);
    let mut best: Option<(usize, i64)> = None;
    for i in [a.saturating_sub(1), a] {
        let Some(&ti) = t.get(i) else { continue };
        if ti > t_now || i < start {
            continue;
        }
        let d = (ti - th).abs();
        if best.is_none_or(|b| d < b.1) {
            best = Some((i, d));
        }
    }
    best.filter(|b| b.1 <= max_dist).map(|b| b.0)
}

/// Overskriften i verktøytipset: «for 12 s siden» i korte vinduer, ellers
/// «i dag 17:50:00 · snitt over 40 s».
pub fn hover_head(t: i64, t_now: i64, window_sec: i64, step_ms: i64, now: i64, utc_offset_s: i64) -> String {
    if window_sec <= 300 && step_ms <= 1000 {
        let s = ((t_now - t) as f64 / 1000.0).round().max(0.0) as i64;
        return if s == 0 { tr!("nå").into() } else { trf!("for {s}{verdi}s siden", s = s, verdi = F::NNBSP) };
    }
    let start = if step_ms > 1000 { t - step_ms / 2 } else { t };
    let head = when(start, now, utc_offset_s, step_ms < 60_000);
    if step_ms > 1000 {
        let over = if step_ms >= 60_000 { format!("{}{}min", step_ms / 60_000, F::NNBSP) } else { format!("{}{}s", step_ms / 1000, F::NNBSP) };
        trf!("{head} · snitt over {over}", head = head, over = over)
    } else {
        head
    }
}

// ---------------------------------------------------------------- Skjermkortet i tekst

const GT: [(u32, &str); 6] = [(1, "2,5"), (2, "5"), (3, "8"), (4, "16"), (5, "32"), (6, "64")];

/// «Gen 3» (med hardt mellomrom).
pub fn gen_text(g: Option<f64>) -> String {
    match g {
        Some(g) if g.is_finite() => format!("Gen\u{a0}{}", g.round() as i64),
        _ => F::DASH.into(),
    }
}

/// «x16 Gen 1 · maks Gen 3», «x8 Gen 3 · maks x16» eller «x16 Gen 3» (som demoens `pcieText`).
pub fn pcie_text(max_w: Option<f64>, max_g: Option<f64>, cur_w: Option<f64>, cur_g: Option<f64>) -> Option<String> {
    let w = |v: Option<f64>| v.map(|x| format!("{}", x.round() as i64)).unwrap_or_else(|| "?".into());
    let link = |wv: Option<f64>, gv: Option<f64>| format!("x{} {}", w(wv), gen_text(gv));
    if cur_w.is_none() && cur_g.is_none() {
        return (max_w.is_some() || max_g.is_some()).then(|| link(max_w, max_g));
    }
    let now = link(cur_w.or(max_w), cur_g.or(max_g));
    let low_g = matches!((max_g, cur_g), (Some(m), Some(c)) if c < m);
    let low_w = matches!((max_w, cur_w), (Some(m), Some(c)) if c < m);
    if !low_g && !low_w {
        return Some(now);
    }
    let mut extra = String::new();
    if low_w {
        extra.push_str(&format!("x{}", w(max_w)));
    }
    if low_w && low_g {
        extra.push(' ');
    }
    if low_g {
        extra.push_str(&gen_text(max_g));
    }
    Some(trf!("{now} · maks {extra}", now = now, extra = extra))
}

/// Forklaringen i verktøytipset, med overføringsrate per lane.
pub fn pcie_explain(max_g: Option<f64>, cur_g: Option<f64>) -> String {
    let (Some(m), Some(c)) = (max_g, cur_g) else { return String::new() };
    let rate = |g: f64| {
        let gi = g.round() as u32;
        match GT.iter().find(|x| x.0 == gi) {
            Some((_, r)) => trf!("{verdi1} = {r}{verdi2}GT/s per lane", r = r, verdi1 = gen_text(Some(g)), verdi2 = F::NNBSP),
            None => gen_text(Some(g)),
        }
    };
    if c < m {
        trf!("Lenken går på {aktiv} nå fordi kortet sparer strøm i tomgang, og går opp til {maks} under last. {aktiv_hastighet}, {maks_hastighet}.", aktiv = gen_text(Some(c)), maks = gen_text(Some(m)), aktiv_hastighet = rate(c), maks_hastighet = rate(m))
    } else {
        trf!("Lenken går på full hastighet. {verdi}.", verdi = rate(c))
    }
}

type Reason = (u64, fn() -> &'static str);

/// Klokkebegrensningene i masken fra NVML.
const REASONS: [Reason; 9] = [
    (0x1, || tr!("Tomgang")),
    (0x2, || tr!("App-klokker")),
    (0x4, || tr!("Effektgrense")),
    (0x8, || tr!("HW-nedklokking")),
    (0x10, || tr!("Sync boost")),
    (0x20, || tr!("Termisk (SW)")),
    (0x40, || tr!("Termisk (HW)")),
    (0x80, || tr!("Strømbrems (HW)")),
    (0x100, || tr!("Skjermklokke")),
];

pub fn throttle_list(mask: Option<u64>) -> Option<Vec<&'static str>> {
    let n = mask?;
    Some(REASONS.iter().filter(|(b, _)| n & b != 0).map(|(_, text)| text()).collect())
}

pub fn throttle_has_reason(mask: u64) -> bool {
    REASONS.iter().any(|(bit, _)| mask & bit != 0)
}

/// Kort form: de alvorligste først, høyst `max`, og «+N» for resten. Gir
/// (tekst, hele lista når noe er kuttet).
pub fn throttle_short(mask: Option<u64>, max: usize) -> (String, String) {
    let Some(mask) = mask else { return (F::DASH.into(), String::new()) };
    let reasons: Vec<_> = REASONS.iter().filter(|(bit, _)| mask & bit != 0).collect();
    if reasons.is_empty() {
        return (tr!("Ingen").into(), String::new());
    }
    const ORDER: [u64; 9] = [0x40, 0x80, 0x8, 0x20, 0x4, 0x10, 0x2, 0x100, 0x1];
    let mut sorted = reasons.clone();
    sorted.sort_by_key(|(bit, _)| ORDER.iter().position(|entry| entry == bit));
    let shown = sorted.iter().take(max).map(|(_, text)| text()).collect::<Vec<_>>().join(", ");
    let text = if sorted.len() > max { format!("{shown} +{}", sorted.len() - max) } else { shown };
    (text, if reasons.len() > max { reasons.iter().map(|(_, text)| text()).collect::<Vec<_>>().join(", ") } else { String::new() })
}

/// Er begrensningen noe annet enn tomgang? (varselfarge)
pub fn throttle_serious(mask: Option<u64>) -> bool {
    REASONS.iter().any(|(bit, _)| matches!(bit, 0x2..=0x80) && mask.is_some_and(|value| value & bit != 0))
}

/// Nivå mot to grenser: «danger», «warn» eller «».
pub fn level(v: Option<f64>, warn: Option<f64>, crit: Option<f64>) -> &'static str {
    match v {
        Some(v) if crit.is_some_and(|c| v >= c) => "danger",
        Some(v) if warn.is_some_and(|w| v >= w) => "warn",
        _ => "",
    }
}

/// Hva gul og rød betyr, i ord (verktøytipsene).
pub fn level_text(level: &str) -> &'static str {
    match level {
        "danger" => tr!("Over rød varselgrense (Innstillinger › Grafer og sensorer)."),
        "warn" => tr!("Over gul varselgrense (Innstillinger › Grafer og sensorer)."),
        _ => "",
    }
}

// ---------------------------------------------------------------- Måleren

/// Plassering av tallene under målerens merker: de står midt under streken,
/// men aldri utenfor kanten, og et tall som ville krasjet med et annet,
/// skjules. Skalaens slutt (siste) viker først. `marks`: (posisjon 0–1,
/// bredde i px). Gir x for hvert tall, eller `None` når det skjules.
pub fn gauge_labels(marks: &[(f32, f32)], end: Option<f32>, width: f32) -> (Vec<Option<f32>>, Option<f32>) {
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|a, b| marks[*a].0.total_cmp(&marks[*b].0));
    let mut out = vec![None; marks.len()];
    for i in order {
        let (at, w) = marks[i];
        let left = (at * width - w / 2.0).clamp(0.0, (width - w).max(0.0));
        let span = (left - 6.0, left + w + 6.0);
        if placed.iter().any(|(a, b)| span.0 < *b && span.1 > *a) {
            continue;
        }
        placed.push(span);
        out[i] = Some(left);
    }
    let end_x = end.and_then(|w| {
        let left = (width - w).max(0.0);
        let span = (left - 6.0, left + w + 6.0);
        (!placed.iter().any(|(a, b)| span.0 < *b && span.1 > *a)).then_some(left)
    });
    (out, end_x)
}

// ---------------------------------------------------------------- CSV

/// Én kolonne i eksporten: overskrift og verdier (allerede omregnet).
pub struct CsvCol {
    pub head: String,
    pub values: Vec<Option<f64>>,
    pub decimals: usize,
}

/// CSV med `Tid` (ISO 8601 med sone) og `Sekunder` (fra første rad) først,
/// så kolonnene. Tomt felt der en verdi mangler (produktspesifikasjonen 3.2).
pub fn csv(t: &[i64], cols: &[CsvCol], utc_offset_s: i64, f: rigg_core::csv::CsvFormat) -> String {
    use rigg_core::csv::{cell, text};
    let sep = f.sep().to_string();
    let mut out = String::new();
    let mut head = vec![text("Tid", f), text("Sekunder", f)];
    head.extend(cols.iter().map(|c| text(&c.head, f)));
    out.push_str(&head.join(&sep));
    out.push('\n');
    let first = t.first().copied().unwrap_or(0);
    for (i, ti) in t.iter().enumerate() {
        let mut row = vec![iso_time(*ti, utc_offset_s), cell(Some((ti - first) as f64 / 1000.0), 0, f)];
        for c in cols {
            row.push(cell(c.values.get(i).copied().flatten(), c.decimals, f));
        }
        out.push_str(&row.join(&sep));
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------- Tester (aksene, runde 8A)

#[cfg(test)]
mod akse_tester {
    use super::*;

    /// GPU-temperatur i °C: foretrukket 20–100, minste spenn 20.
    fn temp(auto: bool) -> Axis {
        Axis { pref: (20.0, 100.0), hard: None, min_span: 20.0, auto }
    }

    fn holder(s: &Scale, lo: f64, hi: f64) {
        assert!(s.lo <= lo && s.hi >= hi, "{lo}–{hi} kuttes av {s:?}");
    }

    /// Verdien står ikke helt i kanten: det er litt luft.
    fn luft(s: &Scale, hi: f64) {
        assert!(s.hi > hi, "ingen luft over {hi}: {s:?}");
    }

    #[test]
    fn i_ro_20_til_45_grader_som_før() {
        let s = y_axis(Some((20.0, 45.0)), &temp(true), &[]);
        holder(&s, 20.0, 45.0);
        assert!(s.lo >= 20.0 && s.hi <= 60.0, "følger dataene: {s:?}");
        // «Fast»: hele det foretrukne området.
        assert_eq!(y_axis(Some((20.0, 45.0)), &temp(false), &[]), Scale { lo: 20.0, hi: 100.0, step: 20.0 });
    }

    #[test]
    fn åtti_og_nittifem_grader() {
        for auto in [true, false] {
            let s = y_axis(Some((40.0, 80.0)), &temp(auto), &[]);
            holder(&s, 40.0, 80.0);
            let s = y_axis(Some((40.0, 95.0)), &temp(auto), &[]);
            holder(&s, 40.0, 95.0);
            assert!(s.hi <= 100.0, "95 °C får plass innenfor det foretrukne: {s:?}");
        }
        // Bare 90–95 i vinduet: minst 20 °C høy akse, og 95 synes.
        let s = y_axis(Some((90.0, 95.0)), &temp(true), &[]);
        holder(&s, 90.0, 95.0);
        assert!(s.hi - s.lo >= 20.0);
    }

    #[test]
    fn hundre_og_ti_grader_kuttes_ikke() {
        for auto in [true, false] {
            let s = y_axis(Some((40.0, 110.0)), &temp(auto), &[]);
            holder(&s, 40.0, 110.0);
            luft(&s, 110.0);
            assert!(s.hi <= 125.0, "pen skala, ikke langt over: {s:?}");
            assert_eq!(s.hi % s.step, 0.0, "øverste merke er et pent tall: {s:?}");
        }
        // «Tilpass» med 100–110 °C i vinduet følger dataene.
        let s = y_axis(Some((100.0, 110.0)), &temp(true), &[]);
        holder(&s, 100.0, 110.0);
        luft(&s, 110.0);
        assert!(s.lo >= 80.0, "{s:?}");
    }

    #[test]
    fn under_tjue_grader_forsvinner_ikke() {
        for auto in [true, false] {
            let s = y_axis(Some((12.0, 18.0)), &temp(auto), &[]);
            holder(&s, 12.0, 18.0);
            assert!(s.lo < 12.0, "luft under: {s:?}");
            assert_eq!(s.lo % s.step, 0.0, "nederste merke er et pent tall: {s:?}");
        }
        // Under null (en kald disk i en kald bod).
        let s = y_axis(Some((-4.0, 30.0)), &temp(false), &[]);
        holder(&s, -4.0, 30.0);
    }

    #[test]
    fn fahrenheit() {
        let f = Axis { pref: (68.0, 212.0), hard: None, min_span: 36.0, auto: false };
        // 110 °C = 230 °F.
        let s = y_axis(Some((104.0, 230.0)), &f, &[]);
        holder(&s, 104.0, 230.0);
        luft(&s, 230.0);
        assert_eq!(s.lo, 68.0, "68 °F står nederst som før: {s:?}");
        // 15 °C = 59 °F.
        let s = y_axis(Some((59.0, 100.0)), &Axis { auto: true, ..f }, &[]);
        holder(&s, 59.0, 100.0);
    }

    #[test]
    fn effekt_over_taket() {
        // Effekt: 0 til kortets maks · 1,05 (288,75 W), men kortet trekker 330 W.
        let p = Axis { pref: (0.0, 288.75), hard: Some((0.0, f64::INFINITY)), min_span: 60.0, auto: false };
        let s = y_axis(Some((20.0, 330.0)), &p, &[]);
        holder(&s, 20.0, 330.0);
        luft(&s, 330.0);
        assert_eq!(s.lo, 0.0);
        // I ro med «Tilpass»: aldri under 0 W.
        let s = y_axis(Some((14.0, 22.0)), &Axis { auto: true, ..p }, &[]);
        assert!(s.lo >= 0.0 && s.hi >= 22.0, "{s:?}");
    }

    #[test]
    fn disk_på_65_grader() {
        let d = Axis { pref: (20.0, 60.0), hard: None, min_span: 10.0, auto: true };
        for auto in [true, false] {
            let s = y_axis(Some((30.0, 65.0)), &Axis { auto, ..d }, &[]);
            holder(&s, 30.0, 65.0);
            luft(&s, 65.0);
        }
    }

    #[test]
    fn prosent_er_en_hard_grense_bare_der_den_skal_være() {
        let last = Axis { pref: (0.0, 100.0), hard: Some((0.0, 100.0)), min_span: 0.0, auto: false };
        assert_eq!(y_axis(Some((0.0, 100.0)), &last, &[]), Scale { lo: 0.0, hi: 100.0, step: 25.0 });
        // En målefeil over 100 % gir ikke en akse til 125.
        assert_eq!(y_axis(Some((0.0, 100.4)), &last, &[]).hi, 100.0);
        // Viftene kan meldes over 100 % av NVML: ingen hard grense der.
        let vifte = Axis { hard: None, ..last };
        let s = y_axis(Some((35.0, 108.0)), &vifte, &[]);
        holder(&s, 35.0, 108.0);
    }

    #[test]
    fn grenselinjene_tas_med_når_kurven_nærmer_seg() {
        // 80–88 °C med «Tilpass»: nedklokking ved 97 °C er like over, og tas med.
        let s = y_axis(Some((80.0, 88.0)), &temp(true), &[97.0, 100.0]);
        assert!(s.hi >= 97.0, "{s:?}");
        // 30–40 °C: grensene er langt unna og drar ikke aksen opp.
        let s = y_axis(Some((30.0, 40.0)), &temp(true), &[97.0, 100.0]);
        assert!(s.hi < 97.0, "{s:?}");
        // Tekstene til 97 og 100 kolliderer i en lav graf: bare den øverste vises.
        assert_eq!(limit_labels(&[Some(40.0), Some(34.0)], 12.0), vec![false, true]);
        assert_eq!(limit_labels(&[Some(60.0), None, Some(20.0)], 12.0), vec![true, false, true]);
    }

    #[test]
    fn aksen_pumper_ikke() {
        let a = temp(true);
        let mut h = AxisHold::default();
        // Kurven veksler mellom 40–44 og 40–52 °C hvert sekund.
        let mut seen = Vec::new();
        for i in 0..30 {
            let hi = if i % 2 == 0 { 44.0 } else { 52.0 };
            let s = h.apply(y_axis(Some((40.0, hi)), &a, &[]), i * 1000);
            holder(&s, 40.0, hi);
            seen.push((s.lo, s.hi));
        }
        seen.dedup();
        assert_eq!(seen.len(), 2, "aksen utvides én gang og står: {seen:?}");
        // Varmere: utvides straks, uten å vente.
        let s = h.apply(y_axis(Some((40.0, 95.0)), &a, &[]), 31_000);
        holder(&s, 40.0, 95.0);
        let big = s;
        // Kaldt igjen: holdes i 15 s, så krymper den.
        let calm = y_axis(Some((40.0, 44.0)), &a, &[]);
        assert_eq!(h.apply(calm, 32_000), big);
        assert_eq!(h.apply(calm, 32_000 + AXIS_HOLD_MS - 1), big);
        assert_eq!(h.apply(calm, 32_000 + AXIS_HOLD_MS), calm);
        // En topp underveis nullstiller ventetiden.
        let mut h = AxisHold::default();
        h.apply(y_axis(Some((40.0, 95.0)), &a, &[]), 0);
        h.apply(calm, 1000);
        h.apply(y_axis(Some((40.0, 95.0)), &a, &[]), 10_000);
        assert_eq!(h.apply(calm, 17_000).hi, big.hi, "ventetiden begynte på nytt");
    }

    #[test]
    fn eksemplene_i_patchloggen() {
        let r = |d: (f64, f64), auto: bool| {
            let s = y_axis(Some(d), &temp(auto), &[]);
            (s.lo, s.hi)
        };
        assert_eq!(r((40.0, 110.0), true), (25.0, 125.0));
        assert_eq!(r((40.0, 110.0), false), (20.0, 120.0));
        assert_eq!(r((12.0, 18.0), true), (5.0, 30.0));
        assert_eq!(r((12.0, 18.0), false), (0.0, 100.0));
        let p = Axis { pref: (0.0, 288.75), hard: Some((0.0, f64::INFINITY)), min_span: 60.0, auto: false };
        let s = y_axis(Some((20.0, 330.0)), &p, &[]);
        assert_eq!((s.lo, s.hi), (0.0, 400.0));
    }

    #[test]
    fn aksetallene_har_desimalene_steget_trenger() {
        assert_eq!(axis_decimals(0.25), 2);
        assert_eq!(axis_decimals(2.5), 1);
        assert_eq!(axis_decimals(0.5), 1);
        assert_eq!(axis_decimals(20.0), 0);
        assert_eq!(axis_decimals(0.05), 2);
    }

    #[test]
    fn uten_data_står_aksen_der_den_er() {
        let a = temp(true);
        let mut h = AxisHold::default();
        assert_eq!(h.idle(y_axis(None, &a, &[])), Scale { lo: 20.0, hi: 100.0, step: 20.0 }, "før data: det foretrukne");
        let s = h.apply(y_axis(Some((35.0, 45.0)), &a, &[]), 0);
        // Historikken hentes på nytt: et bilde uten data drar ikke aksen til 20–100.
        assert_eq!(h.idle(y_axis(None, &a, &[])), s);
        assert_eq!(h.apply(y_axis(Some((35.0, 45.0)), &a, &[]), 1000), s);
    }

    #[test]
    fn aksen_holder_ikke_bare_for_seg_selv() {
        // Unionen når kurven flytter seg: begge kantene er med en stund.
        let mut h = AxisHold::default();
        let a = temp(true);
        h.apply(y_axis(Some((30.0, 40.0)), &a, &[]), 0);
        let s = h.apply(y_axis(Some((70.0, 80.0)), &a, &[]), 1000);
        holder(&s, 30.0, 80.0);
        assert!(y_ticks(&s).len() >= 3, "pene merker også for unionen: {s:?}");
    }
}
