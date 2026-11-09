//! Grafens regnestykker (2A): nedsampling til pikselkolonner, båndet, hull,
//! akser, tidsaksen og formateringen. Modulen er ren Rust uten GTK, så den
//! tas inn her med `#[path]` og testes uten skjerm.

#[allow(dead_code)]
#[path = "../src/widgets/chart/math.rs"]
mod math;

use math::*;

const NN: char = '\u{202f}';
const T0: i64 = 1_794_052_800_000; // 7. nov. 2026 12:00 UTC

fn ident() -> (impl Fn(i64) -> f32, impl Fn(f64) -> f32, impl Fn(f64) -> f64) {
    (|t: i64| ((t - T0) / 1000) as f32, |v: f64| (100.0 - v) as f32, |v: f64| v)
}

#[test]
fn pene_akser() {
    let s = nice_scale(0.0, 100.0, 5);
    assert_eq!((s.lo, s.hi, s.step), (0.0, 100.0, 25.0));
    let s = nice_scale(31.2, 47.9, 5);
    assert_eq!(s.step, 5.0);
    assert!(s.lo <= 31.2 && s.hi >= 47.9);
    // Flat kurve: aldri null spenn.
    let s = nice_scale(40.0, 40.0, 5);
    assert!(s.hi > s.lo);
    assert_eq!(y_ticks(&nice_scale(0.0, 100.0, 5)), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    assert_eq!(axis_decimals(0.5), 1);
    assert_eq!(axis_decimals(0.05), 2);
    assert_eq!(axis_decimals(250.0), 0);
}

#[test]
fn y_aksen_følger_dataene_med_minste_spenn() {
    let y_range = |d: Option<(f64, f64)>, pref: (f64, f64), min_span: f64, auto: bool| y_axis(d, &Axis { pref, hard: None, min_span, auto }, &[]);
    // 38–40 °C med minste spenn 20 °C: aksen er minst 20 °C høy og innenfor 20–100.
    let s = y_range(Some((38.0, 40.0)), (20.0, 100.0), 20.0, true);
    assert!(s.hi - s.lo >= 20.0, "{s:?}");
    assert!(s.lo >= 20.0 && s.hi <= 100.0);
    assert!(s.lo <= 38.0 && s.hi >= 40.0);
    // Fast skala: dataene innenfor betyr ingenting.
    assert_eq!(y_range(Some((38.0, 40.0)), (0.0, 100.0), 0.0, false), nice_scale(0.0, 100.0, 5));
    // Ingen data: det faste området.
    assert_eq!(y_range(None, (0.0, 300.0), 40.0, true), nice_scale(0.0, 300.0, 5));
    // Topp i vinduet (75 °C) kuttes ikke.
    let s = y_range(Some((33.0, 75.0)), (20.0, 100.0), 20.0, true);
    assert!(s.hi >= 75.0);
    // Fast skala med data utenfor: aksen utvides (runde 8A, flere eksempler i math.rs).
    let s = y_range(Some((38.0, 110.0)), (20.0, 100.0), 0.0, false);
    assert!(s.hi > 110.0, "{s:?}");
}

#[test]
fn relative_merker_og_klokkeslett() {
    let r = rel_ticks(60, T0);
    assert_eq!(r.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), vec![format!("−15{NN}s"), format!("−30{NN}s"), format!("−45{NN}s")]);
    assert_eq!(r[0].t, T0 - 15_000);
    let r = rel_ticks(300, T0);
    assert_eq!(r.len(), 4);
    assert_eq!(r[0].label, format!("−1{NN}min"));
    // 24 t i en smal graf (300 px): merker hver 6. time, midnatt som dato.
    let t = clock_ticks(T0 - 86_400_000, T0, 300.0, 3600);
    assert!(!t.is_empty() && t.len() <= 5, "{t:?}");
    assert!(t.iter().any(|x| x.label == "7. nov."), "{t:?}");
    assert!(t.windows(2).all(|w| w[1].t - w[0].t == 21_600_000));
    // 48 t i en bred graf: ett merke per ca. 64 px.
    let t = clock_ticks(T0 - 172_800_000, T0, 1200.0, 3600);
    assert!(t.len() <= 1200 / 64 + 1, "{}", t.len());
}

#[test]
fn tid_i_tekst() {
    assert_eq!(ago(0, 60), "nå");
    assert_eq!(ago(45, 60), format!("−45{NN}s"));
    assert_eq!(ago(240, 300), format!("−4{NN}min"));
    // 7. nov. 12:00 UTC = 13:00 i Oslo (+01:00).
    assert_eq!(when(T0, T0, 3600, false), "i dag 13:00");
    assert_eq!(when(T0 - 86_400_000, T0, 3600, false), "i går 13:00");
    assert_eq!(when(T0 - 3 * 86_400_000, T0, 3600, true), "4. nov. 13:00:00");
    assert_eq!(iso_time(T0, 3600), "2026-11-07T13:00:00+01:00");
    assert_eq!(iso_time(T0, -18_000), "2026-11-07T07:00:00-05:00");
    assert_eq!(hover_head(T0 - 12_000, T0, 60, 1000, T0, 3600), format!("for 12{NN}s siden"));
    assert_eq!(hover_head(T0 - 60_000, T0, 86_400, 60_000, T0, 3600), format!("i dag 12:58 · snitt over 1{NN}min"));
    assert_eq!(window_label(172_800), format!("48{NN}t"));
}

#[test]
fn piksler_og_rulling() {
    // 1 min over 1200 px = 50 ms per piksel (20 ganger i sekundet).
    assert_eq!(ms_per_px(60_000, 1200.0), 50.0);
    // 48 t over 1000 px: én flytting hvert 173. sekund.
    assert!(ms_per_px(172_800_000, 1000.0) > 170_000.0);
    assert_eq!(quantize(1_049, 50.0), 1_000);
    assert_eq!(next_pixel_in(1_049, 50.0), 4); // aldri under 4 ms
    assert_eq!(next_pixel_in(1_010, 50.0), 40);
    // Høyst hvert 400. ms; lange vinduer en gang per piksel.
    assert_eq!(scroll_tick(50.0, 400.0), 400.0);
    assert_eq!(scroll_tick(3000.0, 400.0), 3000.0);
    // To grafer med ulik bredde vekkes på samme tidspunkt (felles rutenett).
    assert_eq!(next_pixel_in(1_000_130, scroll_tick(181.0, 400.0)), next_pixel_in(1_000_130, scroll_tick(176.0, 400.0)));
}

#[test]
fn hull_og_takt() {
    // Bakgrunnstakt 5 s er ikke et brudd; 1 s-bøtter ellers 5 s.
    assert_eq!(gap_ms(1000, 1000), 5000);
    assert_eq!(gap_ms(1000, 5000), 15_000);
    assert_eq!(gap_ms(180_000, 1000), 450_000);
    let t: Vec<i64> = vec![T0 - 50_000, T0 - 49_000, T0 - 20_000, T0 - 19_000];
    let h = holes(&t, 0, T0 - 60_000, T0, 5000, Some(t[0]));
    assert_eq!(h, vec![(T0 - 60_000, T0 - 50_000, HoleKind::Lead), (T0 - 49_000, T0 - 20_000, HoleKind::Gap)]);
    // Ingen data i det hele tatt: hele flaten.
    assert_eq!(holes(&[], 0, T0 - 60_000, T0, 5000, None), vec![(T0 - 60_000, T0, HoleKind::Lead)]);
}

#[test]
fn linja_brytes_ved_hull() {
    let (x, y, val) = ident();
    let m = Mapping { x: &x, y: &y, val: &val };
    let t: Vec<i64> = (0..10).map(|i| T0 + i * 1000).chain((20..25).map(|i| T0 + i * 1000)).collect();
    let avg: Vec<Option<f64>> = t.iter().map(|_| Some(50.0)).collect();
    let s = SeriesView { avg: &avg, lo: None, hi: None, step: false, band: false };
    let segs = segments(&t, &s, 0, T0 + 60_000, 5000, &m, 600.0);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].line.len(), 10);
    assert_eq!(segs[1].line.len(), 5);
    assert!(segs.iter().all(|s| s.band.is_empty()), "korte vinduer har ikke bånd");
}

#[test]
fn nedsampling_gir_høyst_ett_punkt_per_kolonne_og_beholder_toppene() {
    // 4000 punkter over 100 px: 40 per kolonne. Én topp på 99.
    let n = 4000;
    let t: Vec<i64> = (0..n).map(|i| T0 + i * 10).collect();
    let avg: Vec<Option<f64>> = (0..n).map(|i| Some(if i == 2001 { 99.0 } else { 10.0 + (i % 7) as f64 })).collect();
    let x = |ti: i64| ((ti - T0) as f32) / 400.0; // 40 000 ms over 100 px
    let y = |v: f64| (100.0 - v) as f32;
    let val = |v: f64| v;
    let m = Mapping { x: &x, y: &y, val: &val };
    let s = SeriesView { avg: &avg, lo: None, hi: None, step: false, band: true };
    let segs = segments(&t, &s, 0, T0 + 1_000_000, 5000, &m, 100.0);
    assert_eq!(segs.len(), 1);
    let line = &segs[0].line;
    assert!(line.len() <= 101, "{} punkter for 100 px", line.len());
    let xs: Vec<f32> = line.iter().map(|p| p.0).collect();
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "én verdi per kolonne, i rekkefølge");
    // Båndet har toppen (y for 99 = 1 px fra toppen), selv om snittet i kolonnen er lavt.
    let top = segs[0].band.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
    assert_eq!(top, 1.0);
}

#[test]
fn bøttenes_min_og_maks_blir_båndet() {
    let (x, y, val) = ident();
    let m = Mapping { x: &x, y: &y, val: &val };
    let t: Vec<i64> = (0..30).map(|i| T0 + i * 60_000).collect();
    let avg: Vec<Option<f64>> = (0..30).map(|_| Some(50.0)).collect();
    let lo: Vec<Option<f64>> = (0..30).map(|_| Some(40.0)).collect();
    let hi: Vec<Option<f64>> = (0..30).map(|_| Some(70.0)).collect();
    let s = SeriesView { avg: &avg, lo: Some(&lo), hi: Some(&hi), step: false, band: true };
    let segs = segments(&t, &s, 0, T0 + 3_600_000, 150_000, &m, 3600.0);
    assert_eq!(segs[0].band.len(), 30);
    assert_eq!(segs[0].band[0], (0.0, 30.0, 60.0));
    let r = window_range(&t, &s, T0, T0 + 3_600_000, &val).unwrap();
    assert_eq!(r, (40.0, 70.0));
}

#[test]
fn høyt_bånd_blir_svakt() {
    // Smalt bånd: full styrke. Bånd over hele plottet: en firedel.
    assert_eq!(band_level(0.04), 1.0);
    assert_eq!(band_level(1.0), 0.25);
    assert_eq!(band_level(0.5), 0.75);
    // Minneklokka ved 48 t: flatt (0 px) det meste av tiden, og noen
    // kolonner der klokka hopper mellom 405 og 7000 MHz (150 px av 160).
    let mut band: Vec<(f32, f32, f32)> = (0..40).map(|i| (i as f32, 10.0, 10.0)).collect();
    for b in band.iter_mut().skip(20).take(6) {
        b.2 = 160.0;
    }
    let runs = band_runs(&band, 160.0);
    // Det høye stykket (med to naboer på hver side) er svakt, resten fullt.
    assert!(runs.iter().any(|r| r.2 == 0.25 && r.0 <= 20 && r.1 >= 25), "{runs:?}");
    assert!(runs.iter().filter(|r| r.2 == 1.0).count() >= 2);
    // Stykkene henger sammen: hvert begynner der det forrige slutter.
    assert!(runs.windows(2).all(|w| w[0].1 == w[1].0));
    assert_eq!(runs.first().unwrap().0, 0);
    assert_eq!(runs.last().unwrap().1, 39);
}

#[test]
fn trapp_for_effektgrensen() {
    let (x, y, val) = ident();
    let m = Mapping { x: &x, y: &y, val: &val };
    let t: Vec<i64> = (0..4).map(|i| T0 + i * 1000).collect();
    let avg = vec![Some(50.0), Some(50.0), Some(60.0), Some(60.0)];
    let s = SeriesView { avg: &avg, lo: None, hi: None, step: true, band: false };
    let segs = segments(&t, &s, 0, T0 + 10_000, 5000, &m, 600.0);
    // 4 punkter + ett loddrett trinn.
    assert_eq!(segs[0].line.len(), 5);
    assert_eq!(segs[0].line[2], (2.0, 50.5));
    assert_eq!(segs[0].line[3], (2.0, 40.5));
}

#[test]
fn markøren_finner_nærmeste_punkt() {
    let t: Vec<i64> = vec![T0, T0 + 1000, T0 + 2000, T0 + 10_000];
    assert_eq!(nearest(&t, 0, T0 + 1400, T0 + 20_000, 3000), Some(1));
    assert_eq!(nearest(&t, 0, T0 + 1600, T0 + 20_000, 3000), Some(2));
    assert_eq!(nearest(&t, 0, T0 + 6000, T0 + 20_000, 3000), None);
    // Punkter etter «nå» er ikke med.
    assert_eq!(nearest(&t, 0, T0 + 9900, T0 + 5000, 3000), None);
}

#[test]
fn skjermkortet_i_tekst() {
    assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(16.0), Some(1.0)).unwrap(), "x16 Gen\u{a0}1 · maks Gen\u{a0}3");
    assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(16.0), Some(3.0)).unwrap(), "x16 Gen\u{a0}3");
    assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(8.0), Some(3.0)).unwrap(), "x8 Gen\u{a0}3 · maks x16");
    assert_eq!(pcie_text(None, None, None, None), None);
    assert!(pcie_explain(Some(3.0), Some(1.0)).contains("2,5\u{202f}GT/s"));
    assert_eq!(throttle_short(Some(0x1), 1).0, "Tomgang");
    assert_eq!(throttle_short(Some(0x0), 1).0, "Ingen");
    assert_eq!(throttle_short(Some(0x45), 1).0, "Termisk (HW) +2");
    assert!(throttle_serious(Some(0x4)));
    assert!(!throttle_serious(Some(0x1)));
    assert_eq!(level(Some(80.0), Some(75.0), Some(85.0)), "warn");
    assert_eq!(level(Some(85.0), Some(75.0), Some(85.0)), "danger");
    assert_eq!(level(None, Some(75.0), None), "");
}

#[test]
fn målerens_tall_krasjer_aldri() {
    // To merker tett (83 og 88) og slutten: det andre merket skjules, slutten står.
    let (xs, end) = gauge_labels(&[(0.75, 30.0), (0.78, 30.0)], Some(34.0), 300.0);
    assert!(xs[0].is_some());
    assert!(xs[1].is_none());
    assert!(end.is_some());
    // Slutten viker først når den kolliderer.
    let (_, end) = gauge_labels(&[(0.97, 30.0)], Some(34.0), 300.0);
    assert!(end.is_none());
    // Aldri utenfor kanten.
    let (xs, _) = gauge_labels(&[(0.0, 30.0)], None, 300.0);
    assert_eq!(xs[0], Some(0.0));
}

#[test]
fn csv_eksporten() {
    let t = vec![T0, T0 + 60_000];
    let cols = vec![
        CsvCol { head: "GPU-temperatur min [°C]".into(), values: vec![Some(40.25), None], decimals: 1 },
        CsvCol { head: "Effekt, trekk [W]".into(), values: vec![Some(12.5), Some(13.0)], decimals: 1 },
    ];
    let s = csv(&t, &cols, 3600, rigg_core::csv::CsvFormat::Standard);
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines[0], "Tid,Sekunder,GPU-temperatur min [°C],\"Effekt, trekk [W]\"");
    assert_eq!(lines[1], "2026-11-07T13:00:00+01:00,0,40.2,12.5");
    assert_eq!(lines[2], "2026-11-07T13:01:00+01:00,60,,13.0");
    let n = csv(&t, &cols, 3600, rigg_core::csv::CsvFormat::Norsk);
    assert_eq!(n.lines().nth(1).unwrap(), "2026-11-07T13:00:00+01:00;0;40,2;12,5");
}
