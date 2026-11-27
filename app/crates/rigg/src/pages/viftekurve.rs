//! Viftekurve (produktspesifikasjonen 6.5), redesignet i runde 6C:
//!
//! - **Øverst** viftene nå: temperatur, vifte 1 og 2 (% og o/min), mål-fart,
//!   modus og hva kurven ville gitt ved temperaturen nå.
//! - **Kurveredigereren:** punkter som dras og snapper (hele grader og
//!   prosent, Shift: steg på 5), tastaturet, live-punktet med hjelpelinjer,
//!   minste manuelle fart (driveren) og nødregelen markert, og driverens faktiske kurve
//!   fra historikken (punktsky og median per grad) i bakgrunnen. Aksene
//!   starter på 20–100 °C og 0–100 % og utvides for målinger utenfor.
//! - **Til høyre:** modusene (Auto, Kurve, Manuell) med forklaring, punktene
//!   som tall, hvordan kurven følges (hysterese, start og stopp) og døgnet.
//! - **Under:** temperatur og tilgjengelige viftemålekanaler over tid.
//!
//! Endringer er låst mot det ekte kortet: kurven og modusen blir et utkast.
//!
//! **Eies av 6C (Styring).**

use super::kontroller::{Field, draft_diff, mode_name, read, reset_field, stock};
use super::{Key, Page};
use crate::state::App;
use crate::widgets::chart::{self, ChartOpts, ChartPanel, ChartSet, Cols, DefOpts, PanelOpts, SeriesDef};
use crate::widgets::locked::{self, Columns, ConfirmBar, LockStrip};
use crate::widgets::oc::{self, Stat, StatStrip};
use crate::widgets::rows::{KvList, tag};
use crate::widgets::{Icon, Panel, Seg, button, class, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use rigg_core::fmt as F;
use rigg_core::txn::{self, FanMode, Gate, Values};
use rigg_core::vifte;
use rigg_core::{tr, trf, trn};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Kurven er endret i redigereren.
pub type CurveFn = dyn Fn(Vec<(f64, f64)>);

/// Et forhåndsvalg: kurven og hvordan den følges (runde 7B).
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub id: &'static str,
    pub name: fn() -> &'static str,
    pub curve: &'static [(f64, f64)],
    pub hyst: f64,
    pub up: f64,
    pub down: f64,
    pub smooth: f64,
    pub quiet: Option<f64>,
    /// Verktøytipset på knappen.
    pub about: fn() -> &'static str,
}

/// Forhåndsvalgene. Balansert er standarden (`Values::default`).
pub const PRESETS: [Preset; 3] = [
    Preset {
        id: "quiet",
        name: || tr!("Stille"),
        curve: &[(45.0, 35.0), (60.0, 40.0), (70.0, 50.0), (78.0, 65.0), (85.0, 85.0), (92.0, 100.0)],
        hyst: 5.0,
        up: 5.0,
        down: 2.0,
        smooth: 5.0,
        quiet: Some(55.0),
        about: || tr!("Minste fart til 55 °C, 5 °C hysterese, rolig rampe (5 %/s opp, 2 %/s ned) og 5 s utjevning"),
    },
    Preset {
        id: "balanced",
        name: || tr!("Balansert"),
        curve: &txn::DEFAULT_CURVE,
        hyst: vifte::HYSTERESE_STANDARD_C,
        up: vifte::RAMPE_OPP_STANDARD,
        down: vifte::RAMPE_NED_STANDARD,
        smooth: 0.0,
        quiet: None,
        about: || tr!("Standardkurven: 3 °C hysterese, 10 %/s opp og 3 %/s ned, ingen utjevning"),
    },
    Preset {
        id: "perf",
        name: || tr!("Ytelse"),
        curve: &[(30.0, 40.0), (45.0, 50.0), (58.0, 66.0), (68.0, 80.0), (76.0, 92.0), (84.0, 100.0)],
        hyst: 2.0,
        up: 20.0,
        down: 5.0,
        smooth: 0.0,
        quiet: None,
        about: || tr!("Kjøligst: høyere kurve, 2 °C hysterese og rask rampe (20 %/s opp, 5 %/s ned)"),
    },
];

impl Preset {
    /// Setter kurven (felles for alle viftene) og oppførselen.
    pub fn apply(&self, d: &mut Values) {
        d.fan_curve = self.curve.to_vec();
        d.fan_per_fan = false;
        d.fan_hysteresis_c = self.hyst;
        d.fan_ramp_up_pct_s = self.up;
        d.fan_ramp_down_pct_s = self.down;
        d.fan_smoothing_s = self.smooth;
        d.fan_quiet_until_c = self.quiet;
    }

    /// Lik kurve (felles) og lik oppførsel.
    pub fn matches(&self, v: &Values) -> bool {
        self.curve_matches(v) && (v.fan_hysteresis_c, v.fan_ramp_up_pct_s, v.fan_ramp_down_pct_s, v.fan_smoothing_s, v.fan_quiet_until_c) == (self.hyst, self.up, self.down, self.smooth, self.quiet)
    }

    /// Bare kurven er lik (oppførselen er endret).
    pub fn curve_matches(&self, v: &Values) -> bool {
        !v.fan_per_fan && v.fan_curve.as_slice() == self.curve
    }
}

/// Navnet på oppsettet: «Balansert», «Stille · endret» (kurven er et
/// forhåndsvalg, men oppførselen er endret) eller `None` (egendefinert).
pub fn preset_name(v: &Values) -> Option<String> {
    if let Some(p) = PRESETS.iter().find(|p| p.matches(v)) {
        return Some((p.name)().to_string());
    }
    PRESETS.iter().find(|p| p.curve_matches(v)).map(|p| trf!("{navn} · endret", navn = (p.name)()))
}

/// Kurven og alt om hvordan den følges tilbake til standard (`st`).
pub fn reset_fan_shape(d: &mut Values, st: &Values) {
    reset_field(d, Field::Curve, st);
    d.fan_per_fan = st.fan_per_fan;
    d.fan_curves = st.fan_curves.clone();
    d.fan_hysteresis_c = st.fan_hysteresis_c;
    d.fan_ramp_up_pct_s = st.fan_ramp_up_pct_s;
    d.fan_ramp_down_pct_s = st.fan_ramp_down_pct_s;
    d.fan_smoothing_s = st.fan_smoothing_s;
    d.fan_quiet_until_c = st.fan_quiet_until_c;
}

/// Er kurven og oppførselen som standard?
pub fn fan_shape_is(d: &Values, st: &Values) -> bool {
    let mut x = d.clone();
    reset_fan_shape(&mut x, st);
    x == *d
}

/// Det kortet har. Med det simulerte kortet melder tjenesten også kurven
/// og oppførselen som er i bruk (`extra.styring.fan`, runde 7B); det ekte
/// kortet har ikke feltet, og da er det driverens Auto og standardkurven.
pub fn card_values(app: &App) -> Values {
    if let Some(v) = locked::control_values(app) { return v; }
    let b = read(app).values();
    let fan = app.statics().map(|s| s.extra["styring"]["fan"].clone()).filter(Value::is_object);
    match fan.map(|f| txn::merge(&b, &json!({"fan": f}))) {
        Some(Ok((v, _))) => v,
        _ => b,
    }
}

// ================================================================ felles kurve eller per vifte

/// Hvilken kurve redigereren viser: 0 = felles, 1 = vifte 1, 2 = vifte 2.
/// Uten egne kurver er det alltid den felles.
pub fn target_of(v: &Values, want: usize) -> usize {
    if v.fan_per_fan { want.max(1) } else { 0 }
}

/// Kurven for `target` (0 = felles).
pub fn edited_curve(v: &Values, target: usize) -> Vec<(f64, f64)> {
    match target {
        0 => v.fan_curve.clone(),
        n => v.curve_for(n - 1).to_vec(),
    }
}

/// Setter kurven for `target`. Med egne kurver fylles de som mangler fra
/// den felles først, så vifte 2 ikke får en tom kurve.
pub fn set_edited_curve(d: &mut Values, target: usize, c: Vec<(f64, f64)>) {
    if target == 0 || !d.fan_per_fan {
        d.fan_curve = c;
        return;
    }
    if d.fan_curves.len() < target {
        let fill = d.fan_curve.clone();
        d.fan_curves.resize(target, fill);
    }
    for k in d.fan_curves.iter_mut().filter(|k| k.is_empty()) {
        *k = d.fan_curve.clone();
    }
    d.fan_curves[target - 1] = c;
}

/// Egen kurve for hver av `n` vifter: de som mangler, starter som den felles.
pub fn split_curves(d: &mut Values, n: usize) {
    d.fan_per_fan = true;
    let fill = d.fan_curve.clone();
    if d.fan_curves.len() < n {
        d.fan_curves.resize(n, fill.clone());
    }
    for k in d.fan_curves.iter_mut().filter(|k| k.is_empty()) {
        *k = fill.clone();
    }
}

/// Tillatt temperatur for redigerbare punkter (°C), uavhengig av måleaksen.
pub const AXIS_T: (f64, f64) = (20.0, 100.0);

/// Visningsakser: beholder normalområdet og utvider i grove trinn ved behov.
/// En måletopp blir stående i skalaen mens redigereren lever, så aksene ikke
/// pumper når nye prøver kommer. Dette endrer ingen grenser for brukerinput.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CurveScale {
    temp: (f64, f64),
    fan: (f64, f64),
}

impl Default for CurveScale {
    fn default() -> Self {
        Self { temp: AXIS_T, fan: (0.0, 100.0) }
    }
}

impl CurveScale {
    fn expanded(mut self, points: impl Iterator<Item = (f64, f64)>) -> Self {
        let grow = |range: &mut (f64, f64), v: f64, step: f64| {
            if !v.is_finite() {
                return;
            }
            if v < range.0 {
                range.0 = (v / step).ceil() * step - step;
            }
            if v > range.1 {
                range.1 = (v / step).floor() * step + step;
            }
        };
        let (mut temp, mut fan) = (self.temp, self.fan);
        for (t, f) in points {
            if t.is_finite() {
                temp = (temp.0.min(t), temp.1.max(t));
            }
            if f.is_finite() {
                fan = (fan.0.min(f), fan.1.max(f));
            }
        }
        grow(&mut self.temp, temp.0, 10.0);
        grow(&mut self.temp, temp.1, 10.0);
        grow(&mut self.fan, fan.0, 25.0);
        grow(&mut self.fan, fan.1, 25.0);
        self
    }

    /// Temperatur og fart til andeler fra venstre og fra bunnen.
    fn position(self, t: f64, f: f64) -> (f64, f64) {
        ((t - self.temp.0) / (self.temp.1 - self.temp.0), (f - self.fan.0) / (self.fan.1 - self.fan.0))
    }

    /// Pekeren følger visningsaksen; input stopper ved de lovlige grensene.
    fn input_at(self, x: f64, y: f64) -> (f64, f64) {
        let t = self.temp.0 + x.clamp(0.0, 1.0) * (self.temp.1 - self.temp.0);
        let f = self.fan.0 + y.clamp(0.0, 1.0) * (self.fan.1 - self.fan.0);
        (t.clamp(AXIS_T.0, AXIS_T.1), f.clamp(0.0, 100.0))
    }
}

/// Høyst elleve rutenettstreker, også når en måling ligger langt utenfor.
fn curve_ticks((lo, hi): (f64, f64), base: f64) -> Vec<f64> {
    let step = base * 2f64.powf(((hi - lo) / (base * 10.0)).log2().ceil().max(0.0));
    let first = (lo / step).ceil() * step;
    (0..=10).map(|i| first + i as f64 * step).take_while(|v| *v <= hi + 1e-9).collect()
}

/// Median av viftefart per hele grad (minst tre målinger per grad).
pub fn median_per_degree(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut by: std::collections::BTreeMap<i64, Vec<f64>> = Default::default();
    for (t, f) in pts {
        by.entry(t.round() as i64).or_default().push(*f);
    }
    by.into_iter()
        .filter(|(_, v)| v.len() >= 3)
        .map(|(t, mut v)| {
            v.sort_by(|a, b| a.total_cmp(b));
            let n = v.len();
            let m = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 };
            (t as f64, m)
        })
        .collect()
}

/// Andelen av målingene der viften gikk på minste fart (±0,5 %).
pub fn share_at_min(fans: &[f64], min: f64) -> Option<f64> {
    if fans.is_empty() {
        return None;
    }
    Some(fans.iter().filter(|f| **f <= min + 0.5).count() as f64 / fans.len() as f64)
}

// ================================================================ kurvelogikken

/// Snap for et punkt som dras: hele grader og prosent, eller steg på 5 (Shift).
pub fn snap_value(v: f64, coarse: bool) -> f64 {
    if coarse { (v / 5.0).round() * 5.0 } else { v.round() }
}

/// Flytter punkt `i`: mellom naboene (minst 1 °C mellom), ikke under minste
/// fart, aldri lavere enn punktet før eller høyere enn punktet etter, og
/// innenfor temperaturaksen.
pub fn move_point(c: &[(f64, f64)], i: usize, t: f64, f: f64, floor: f64) -> Vec<(f64, f64)> {
    let mut c = c.to_vec();
    let n = c.len();
    if i >= n {
        return c;
    }
    let tlo = if i > 0 { c[i - 1].0 + 1.0 } else { AXIS_T.0 };
    let thi = if i + 1 < n { c[i + 1].0 - 1.0 } else { AXIS_T.1 };
    let flo = if i > 0 { c[i - 1].1.max(floor) } else { floor };
    let fhi = if i + 1 < n { c[i + 1].1.min(100.0) } else { 100.0 };
    c[i] = (t.clamp(tlo, thi.max(tlo)), f.clamp(flo, fhi.max(flo)));
    c
}

/// Nytt punkt ved `t` på kurven (farten der kurven går). `None` når det er
/// fullt, eller et punkt allerede står der.
pub fn add_point(c: &[(f64, f64)], t: f64, floor: f64) -> Option<(Vec<(f64, f64)>, usize)> {
    let t = t.round().clamp(AXIS_T.0, AXIS_T.1);
    if c.len() >= txn::CURVE_MAX_POINTS || c.iter().any(|p| p.0 == t) {
        return None;
    }
    let f = txn::curve_at(c, t).round().max(floor);
    let mut c = c.to_vec();
    c.push((t, f));
    c.sort_by(|a, b| a.0.total_cmp(&b.0));
    let i = c.iter().position(|p| p.0 == t)?;
    Some((c, i))
}

/// Nytt punkt midt mellom punkt `i` og det neste (eller 5 °C etter det siste).
pub fn add_after(c: &[(f64, f64)], i: usize, floor: f64) -> Option<(Vec<(f64, f64)>, usize)> {
    let a = c.get(i)?.0;
    let t = match c.get(i + 1) {
        Some(b) if b.0 - a >= 2.0 => ((a + b.0) / 2.0).round(),
        Some(_) => return None,
        None if a + 5.0 <= AXIS_T.1 => a + 5.0,
        None => return None,
    };
    add_point(c, t, floor)
}

/// Fjerner punkt `i`, men aldri under to punkter.
pub fn remove_point(c: &[(f64, f64)], i: usize) -> Option<Vec<(f64, f64)>> {
    if c.len() <= txn::CURVE_MIN_POINTS || i >= c.len() {
        return None;
    }
    let mut c = c.to_vec();
    c.remove(i);
    Some(c)
}

// ================================================================ redigereren

mod edit_imp {
    use super::*;

    pub struct CurveEdit {
        pub pts: RefCell<Vec<(f64, f64)>>,
        pub median: RefCell<Vec<(f64, f64)>>,
        pub now: Cell<Option<(f64, f64)>>,
        pub(super) scale: Cell<CurveScale>,
        /// Nye topper under dragging vises når museknappen slippes.
        pub(super) pending_scale: Cell<Option<CurveScale>>,
        pub curve: RefCell<Vec<(f64, f64)>>,
        /// Kurven er et utkast (heltrukket), ellers standardkurven (stiplet).
        pub draft: Cell<bool>,
        pub sel: Cell<Option<usize>>,
        pub drag: Cell<Option<usize>>,
        pub hover: Cell<Option<(f32, f32)>>,
        pub floor: Cell<f64>,
        pub floor_known: Cell<bool>,
        /// prikker, median, nå, kurve, rutenett, tekst, sone, kort-bakgrunn, dempet kurve
        pub colors: RefCell<[gdk::RGBA; 9]>,
        pub on_curve: crate::widgets::Slot<CurveFn>,
        pub on_sel: crate::widgets::Slot<dyn Fn(Option<usize>)>,
        /// (venstre, topp, bredde, høyde)
        pub geom: Cell<(f32, f32, f32, f32)>,
        pub empty: RefCell<String>,
        /// Den andre viftens kurve (svakt i bakgrunnen), når hver vifte har sin.
        pub ghost: RefCell<Vec<(f64, f64)>>,
        /// Hysteresen (°C): båndet og den stiplede nedkurven.
        pub hyst: Cell<f64>,
        /// «Hold minimum til» (°C).
        pub quiet: Cell<Option<f64>>,
    }

    impl Default for CurveEdit {
        fn default() -> Self {
            CurveEdit {
                pts: RefCell::default(),
                median: RefCell::default(),
                now: Cell::new(None),
                scale: Cell::new(CurveScale::default()),
                pending_scale: Cell::new(None),
                curve: RefCell::new(txn::DEFAULT_CURVE.to_vec()),
                draft: Cell::new(false),
                sel: Cell::new(None),
                drag: Cell::new(None),
                hover: Cell::new(None),
                floor: Cell::new(35.0),
                floor_known: Cell::new(false),
                colors: RefCell::new([gdk::RGBA::BLACK; 9]),
                on_curve: RefCell::new(None),
                on_sel: RefCell::new(None),
                geom: Cell::new((0.0, 0.0, 1.0, 1.0)),
                empty: RefCell::new(String::new()),
                ghost: RefCell::default(),
                hyst: Cell::new(0.0),
                quiet: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CurveEdit {
        const NAME: &'static str = "RiggFanCurveEdit";
        type Type = super::CurveEdit;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("fancurve");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for CurveEdit {}

    impl WidgetImpl for CurveEdit {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (300, 640, -1, -1),
                _ => (300, 440, -1, -1),
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            let [dots, med, now_c, curve_c, grid, text, zone, card, faint] = *self.colors.borrow();
            let scale = w.update_scale(self.pts.borrow().iter().copied().chain(self.now.get()));
            let (lo, hi) = scale.temp;
            // Plass til tallene på y-aksen til venstre og x-aksen under.
            let (pl, pt, pr, pb) = (48.0f32, 14.0f32, 14.0f32, 40.0f32);
            let pw = (ww - pl - pr).max(10.0);
            let ph = (hh - pt - pb).max(10.0);
            self.geom.set((pl, pt, pw, ph));
            let x = |t: f64| pl + (scale.position(t, 0.0).0 as f32).clamp(0.0, 1.0) * pw;
            let y = |f: f64| pt + ph - (scale.position(0.0, f).1 as f32).clamp(0.0, 1.0) * ph;
            let temp_ticks = curve_ticks(scale.temp, 10.0);
            let fan_ticks = curve_ticks(scale.fan, 25.0);
            // Tekst uten overlapp: hver tekst får en flate, og det som ville
            // havnet oppå noe som allerede står, tegnes ikke.
            let placed: RefCell<Vec<graphene::Rect>> = RefCell::new(Vec::new());
            let text_at = |txt: &str, xx: f32, yy: f32, ax: f32, ay: f32, col: &gdk::RGBA, bg: bool| -> bool {
                let lay = w.create_pango_layout(Some(txt));
                let (lw, lh) = lay.pixel_size();
                let (lw, lh) = (lw as f32, lh as f32);
                let x0 = (xx - lw * ax).round().clamp(0.0, (ww - lw).max(0.0));
                let y0 = (yy - lh * ay).round().clamp(0.0, (hh - lh).max(0.0));
                let r = graphene::Rect::new(x0 - 3.0, y0 - 1.0, lw + 6.0, lh + 2.0);
                if placed.borrow().iter().any(|p| p.intersection(&r).is_some()) {
                    return false;
                }
                placed.borrow_mut().push(r);
                if bg {
                    s.append_color(&card, &r);
                }
                s.save();
                s.translate(&graphene::Point::new(x0, y0));
                s.append_layout(&lay, col);
                s.restore();
                true
            };
            let floor = self.floor.get();
            // Under minste fart: en rolig sone med skravering.
            let fy = y(floor);
            s.append_color(&zone, &graphene::Rect::new(pl, fy, pw, pt + ph - fy));
            let zr = graphene::Rect::new(pl, fy, pw, pt + ph - fy);
            s.push_clip(&zr);
            {
                let p = crate::widgets::Painter::new(s, &zr);
                let h = zr.height();
                let mut k = -h;
                while k < pw {
                    p.stroke([(pl + k, fy + h), (pl + k + h, fy)], 1.0, false, None, &grid);
                    k += 9.0;
                }
            }
            s.pop();
            // Nødregelen over 85 °C.
            let ex = x(txn::EMERGENCY_FAN_C);
            s.append_color(&zone, &graphene::Rect::new(ex, pt, pl + pw - ex, fy - pt));
            // Rutenettet.
            for &f in &fan_ticks {
                let yy = y(f).round();
                s.append_color(&grid, &graphene::Rect::new(pl, yy - 0.5, pw, 1.0));
            }
            for &t in &temp_ticks {
                let xx = x(t).round();
                s.append_color(&grid, &graphene::Rect::new(xx - 0.5, pt, 1.0, ph));
            }
            // Minste fart som linje.
            {
                let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, fy - 2.0, pw, 4.0));
                p.stroke([(pl, fy.round()), (pl + pw, fy.round())], 1.0, false, Some(&[4.0f32, 3.0][..]), &text);
            }
            // Tallene på aksene. Etiketter inne i grafen først, så aksene
            // viker for dem der de ville overlappet (de står ytterst).
            let mut faint_text = text;
            faint_text.set_alpha(text.alpha() * 0.85);
            for &f in &fan_ticks {
                text_at(&F::pct(Some(f), 0), pl - 8.0, y(f), 1.0, 0.5, &faint_text, false);
            }
            for (i, &t) in temp_ticks.iter().enumerate() {
                // Første tall begynner ved aksen og siste slutter ved kanten,
                // så de aldri treffer «0 %» eller går utenfor.
                let ax = if i == 0 {
                    0.0
                } else if i + 1 == temp_ticks.len() {
                    1.0
                } else {
                    0.5
                };
                text_at(&F::temp(Some(t), 0), x(t), pt + ph + 6.0, ax, 0.0, &faint_text, false);
            }
            text_at(tr!("GPU-temperatur"), pl + pw / 2.0, hh - 2.0, 0.5, 1.0, &text, false);
            let min_label = if self.floor_known.get() { trf!("Minst {fart} (driver)", fart = F::pct(Some(floor), 0)) } else { tr!("Minste fart er ikke avlest").into() };
            text_at(&min_label, pl + pw - 6.0, fy - 3.0, 1.0, 1.0, &text, true);
            if self.floor_known.get() && floor > scale.fan.0 {
                text_at(&trf!("Under {fart} er ikke mulig", fart = F::pct(Some(floor), 0)), pl + 8.0, fy + (pt + ph - fy) / 2.0, 0.0, 0.5, &text, true);
            }
            // Nederst i feltet, over «Minst 35 %», ikke øverst der kurven går
            // mot 100 % eller midt i der den krysser i smale vinduer (7C).
            text_at(&trf!("Nødregel {fart} over {temperatur}", fart = F::pct(Some(100.0), 0), temperatur = F::temp(Some(txn::EMERGENCY_FAN_C), 0)), pl + pw - 6.0, y(47.0), 1.0, 0.5, &text, true);
            // Driverens faktiske kurve: punktene og medianen per grad.
            let pts = self.pts.borrow();
            if !pts.is_empty() {
                let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, pt, pw, ph));
                for (t, f) in pts.iter() {
                    let (cx, cy) = (x(*t), y(*f));
                    p.fill((0..6).map(|k| {
                        let a = k as f32 * std::f32::consts::PI / 3.0;
                        (cx + 2.0 * a.cos(), cy + 2.0 * a.sin())
                    }), &dots);
                }
            } else {
                let e = self.empty.borrow();
                if !e.is_empty() {
                    text_at(&e, pl + pw * 0.3, pt + ph * 0.3, 0.5, 0.5, &text, true);
                }
            }
            let m = self.median.borrow();
            if m.len() >= 2 {
                let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, pt, pw, ph));
                p.stroke(m.iter().map(|(t, f)| (x(*t), y(*f))), 1.5, true, None, &med);
            }
            // Kurven: heltrukket i utkastet, stiplet når det er standardkurven.
            let c = self.curve.borrow();
            let draft = self.draft.get();
            let col = if draft { curve_c } else { faint };
            let quiet = self.quiet.get();
            let hyst = self.hyst.get();
            let clip = graphene::Rect::new(pl, pt, pw, ph);
            // Den andre viftens kurve, svakt bak.
            let ghost = self.ghost.borrow();
            if ghost.len() >= 2 {
                let p = crate::widgets::Painter::new(s, &clip);
                let mut g = faint;
                g.set_alpha(faint.alpha() * 0.55);
                p.stroke(effective_line(&ghost, quiet, floor, lo, hi).into_iter().map(|(t, f)| (x(t), y(f))), 1.5, true, Some(&[3.0f32, 3.0][..]), &g);
            }
            if !c.is_empty() {
                // Hysteresen: båndet mellom kurven og kurven forskjøvet mot
                // venstre (der viftene går ned), og nedkurven stiplet.
                if hyst > 0.0 {
                    let steps = (((hi - lo) / 0.25).ceil() as usize).clamp(1, 4096);
                    let ts: Vec<f64> = (0..=steps).map(|k| lo + (hi - lo) * k as f64 / steps as f64).collect();
                    let up: Vec<(f32, f32)> = ts.iter().map(|t| (x(*t), y(effective_at(&c, quiet, floor, *t)))).collect();
                    let down: Vec<(f32, f32)> = ts.iter().map(|t| (x(*t), y(effective_at(&c, quiet, floor, t + hyst)))).collect();
                    let mut band = col;
                    band.set_alpha(if draft { 0.13 } else { 0.08 });
                    let p = crate::widgets::Painter::new(s, &clip);
                    p.fill(down.iter().copied().chain(up.iter().rev().copied()), &band);
                    let mut dl = col;
                    dl.set_alpha(col.alpha() * 0.75);
                    p.stroke(down, 1.25, true, Some(&[2.0f32, 3.0][..]), &dl);
                }
                // «Hold minimum til»: en stiplet strek opp fra minste fart.
                if let Some(q) = quiet.filter(|q| *q > lo && *q < hi) {
                    let qx = x(q).round();
                    let p = crate::widgets::Painter::new(s, &clip);
                    p.stroke([(qx, fy), (qx, y(effective_at(&c, None, floor, q)))], 1.0, false, Some(&[2.0f32, 2.0][..]), &text);
                    text_at(&trf!("Minst {fart} til {temperatur}", fart = F::pct(Some(floor), 0), temperatur = F::temp(Some(q), 0)), qx - 6.0, fy - 3.0, 1.0, 1.0, &text, true);
                }
                let line: Vec<(f32, f32)> = effective_line(&c, quiet, floor, lo, hi).into_iter().map(|(t, f)| (x(t), y(f))).collect();
                let p = crate::widgets::Painter::new(s, &graphene::Rect::new(0.0, 0.0, ww, hh));
                p.stroke(line, if draft { 2.0 } else { 1.75 }, true, if draft { None } else { Some(&[6.0f32, 4.0][..]) }, &col);
                if hyst > 0.0 {
                    // Merket ved båndet, der det er bredest i midten av kurven.
                    let tm = c.iter().map(|p| p.0).sum::<f64>() / c.len() as f64;
                    let fm = effective_at(&c, quiet, floor, tm + hyst);
                    text_at(&trf!("Ned · {grader} hysterese", grader = F::temp_delta(hyst, 0)), x(tm) - 8.0, y(fm) - 4.0, 1.0, 1.0, &col, true);
                }
            }
            // Live-punktet: ring, hjelpelinjer til aksene og «Nå».
            if let Some((t, f)) = self.now.get() {
                let (nx, ny) = (x(t), y(f));
                {
                    let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, pt, pw, ph));
                    let mut g = now_c;
                    g.set_alpha(0.6);
                    p.stroke([(nx, ny), (nx, pt + ph)], 1.0, false, Some(&[3.0f32, 3.0][..]), &g);
                    p.stroke([(pl, ny), (nx, ny)], 1.0, false, Some(&[3.0f32, 3.0][..]), &g);
                }
                let bg = graphene::Rect::new(nx - 5.0, ny - 5.0, 10.0, 10.0);
                s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(bg, 5.0));
                s.append_color(&card, &bg);
                s.pop();
                oc_ring(s, nx, ny, 6.0, 2.0, &now_c);
                // Hva kurven ville gitt ved temperaturen nå.
                if !c.is_empty() {
                    let cf = effective_at(&c, quiet, floor, t);
                    let cy = y(cf);
                    let r = graphene::Rect::new(nx - 3.5, cy - 3.5, 7.0, 7.0);
                    s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(r, 1.0));
                    s.append_color(&col, &r);
                    s.pop();
                }
                let label = trf!("Nå {temperatur} · {fart}", temperatur = F::temp(Some(t), 0), fart = F::pct(Some(f), 0));
                if !text_at(&label, nx + 10.0, ny - 8.0, 0.0, 1.0, &now_c, true) {
                    text_at(&label, nx - 10.0, ny + 8.0, 1.0, 0.0, &now_c, true);
                }
            }
            // Punktene oppå.
            let sel = self.sel.get();
            for (i, (t, f)) in c.iter().enumerate() {
                let (cx, cy) = (x(*t), y(*f));
                let r = if sel == Some(i) { 6.0 } else { 5.0 };
                let rect = graphene::Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r);
                // Under «Hold minimum til» brukes ikke punktet: hult.
                let idle = quiet.is_some_and(|q| *t < q);
                if draft && !idle {
                    s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(rect, r));
                    s.append_color(&col, &rect);
                    s.pop();
                } else {
                    s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(rect, r));
                    s.append_color(&card, &rect);
                    s.pop();
                    oc_ring(s, cx, cy, r, 1.5, &col);
                }
            }
            if let Some(i) = sel
                && let Some((t, f)) = c.get(i).copied()
            {
                let (cx, cy) = (x(t), y(f));
                let focus = w.state_flags().contains(gtk::StateFlags::FOCUS_VISIBLE) || self.drag.get().is_some();
                let mut ring_c = col;
                ring_c.set_alpha(if focus { 0.9 } else { 0.5 });
                oc_ring(s, cx, cy, 10.0, 1.5, &ring_c);
                if self.drag.get().is_some() {
                    let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, pt, pw, ph));
                    let mut g = col;
                    g.set_alpha(0.5);
                    p.stroke([(cx, cy), (cx, pt + ph)], 1.0, false, Some(&[2.0f32, 3.0][..]), &g);
                    p.stroke([(pl, cy), (cx, cy)], 1.0, false, Some(&[2.0f32, 3.0][..]), &g);
                }
                let txt = format!("{} · {}", F::temp(Some(t), 0), F::pct(Some(f), 0));
                if !text_at(&txt, cx, cy - 14.0, 0.5, 1.0, &col, true) {
                    text_at(&txt, cx, cy + 14.0, 0.5, 0.0, &col, true);
                }
            }
            // Markøren: medianen ved temperaturen under pekeren.
            if let (Some((hx, hy)), None) = (self.hover.get(), self.drag.get())
                && hx >= pl
                && hx <= pl + pw
                && hy >= pt
                && hy <= pt + ph
            {
                let t = (lo + ((hx - pl) / pw) as f64 * (hi - lo)).round();
                s.append_color(&grid, &graphene::Rect::new(x(t).round(), pt, 1.0, ph));
                let n = pts.iter().filter(|p| p.0.round() == t).count();
                let cf = effective_at(&c, quiet, floor, t);
                let mut txt = trf!("Ved {temperatur}: kurven {fart}", temperatur = F::temp(Some(t), 0), fart = F::pct(Some(cf.round()), 0));
                let down = effective_at(&c, quiet, floor, t + hyst);
                if hyst > 0.0 && down.round() != cf.round() {
                    txt.push_str(&trf!(" (ned {fart})", fart = F::pct(Some(down.round()), 0)));
                }
                match m.iter().find(|p| p.0 == t) {
                    Some((_, f)) => txt.push_str(&trf!(", driveren {fart} (median av {n})", fart = F::pct(Some(*f), 0), n = n)),
                    None if n > 0 => txt.push_str(&if rigg_core::i18n::active() == rigg_core::i18n::Language::Nb { format!(", {n} målinger") } else { trn!(n, ", {n} måling", ", {n} målinger", n = n) }),
                    None => {}
                }
                text_at(&txt, pl + 8.0, pt + 6.0, 0.0, 0.0, &text, true);
            }
        }
    }
}

/// Farten kurven gir ved `t` med «hold minimum til» og minste fart, som
/// viftemotoren (`Oppsett::maal`).
pub fn effective_at(c: &[(f64, f64)], quiet: Option<f64>, floor: f64, t: f64) -> f64 {
    if quiet.is_some_and(|q| t < q) {
        return floor;
    }
    if c.is_empty() {
        return floor;
    }
    txn::curve_at(c, t).clamp(floor, 100.0)
}

/// Linja som tegnes fra `lo` til `hi`: flat før første punkt og etter siste,
/// og et loddrett sprang der «hold minimum til» slutter.
pub fn effective_line(c: &[(f64, f64)], quiet: Option<f64>, floor: f64, lo: f64, hi: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(c.len() + 4);
    let q = quiet.filter(|q| *q > lo);
    match q {
        Some(q) => {
            out.push((lo, floor));
            out.push((q.min(hi), floor));
            if q < hi {
                out.push((q, effective_at(c, None, floor, q)));
            }
        }
        None => out.push((lo, effective_at(c, None, floor, lo))),
    }
    let from = q.unwrap_or(lo);
    for p in c.iter().filter(|p| p.0 > from && p.0 < hi) {
        out.push((p.0, p.1.clamp(floor, 100.0)));
    }
    if from < hi {
        out.push((hi, effective_at(c, None, floor, hi)));
    }
    out
}

fn oc_ring(s: &gtk::Snapshot, cx: f32, cy: f32, r: f32, w: f32, c: &gdk::RGBA) {
    let rr = gtk::gsk::RoundedRect::from_rect(graphene::Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r), r);
    s.append_border(&rr, &[w; 4], &[*c; 4]);
}

glib::wrapper! {
    /// Kurveredigereren: driverens faktiske kurve i bakgrunnen og kurven oppå.
    pub struct CurveEdit(ObjectSubclass<edit_imp::CurveEdit>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CurveEdit {
    pub fn new(app: &Rc<App>) -> CurveEdit {
        let s: CurveEdit = glib::Object::new();
        s.set_hexpand(true);
        s.set_vexpand(true);
        s.set_focusable(true);
        s.set_focus_on_click(true);
        s.update_property(&[gtk::accessible::Property::Label(tr!("Viftekurven: viftefart mot GPU-temperatur"))]);
        let set = {
            let w = s.downgrade();
            let a = app.clone();
            move || {
                let Some(s) = w.upgrade() else { return };
                let c = |t: &str| crate::theme::rgba(&a, t);
                let mut dots = c("--muted");
                dots.set_alpha(0.28);
                let mut med = c("--text");
                med.set_alpha(0.55);
                let mut faint = c("--muted");
                faint.set_alpha(0.95);
                *s.imp().colors.borrow_mut() = [dots, med, c("--c-temp"), c("--accent"), c("--grid"), c("--muted"), c("--zone"), c("--card"), faint];
                s.queue_draw();
            }
        };
        set();
        app.on_theme.connect(move |_| set());
        s.connect_state_flags_changed(|s, _| s.queue_draw());
        s.bind_input();
        s
    }

    fn update_scale(&self, points: impl Iterator<Item = (f64, f64)>) -> CurveScale {
        let i = self.imp();
        let current = i.scale.get();
        let next = i.pending_scale.get().unwrap_or(current).expanded(points);
        if i.drag.get().is_some() {
            i.pending_scale.set(Some(next));
            current
        } else {
            i.pending_scale.set(None);
            i.scale.set(next);
            next
        }
    }

    fn to_val(&self, x: f32, y: f32) -> (f64, f64) {
        let (pl, pt, pw, ph) = self.imp().geom.get();
        self.imp().scale.get().input_at(((x - pl) / pw) as f64, (1.0 - (y - pt) / ph) as f64)
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let (pl, pt, pw, ph) = self.imp().geom.get();
        let scale = self.imp().scale.get();
        let c = self.imp().curve.borrow();
        // Nærmeste punkt innenfor 14 px.
        c.iter()
            .enumerate()
            .map(|(i, (t, f))| {
                let (tx, fy) = scale.position(*t, *f);
                let px = pl + tx as f32 * pw;
                let py = pt + ph - fy as f32 * ph;
                (i, (px - x).hypot(py - y))
            })
            .filter(|(_, d)| *d < 14.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn select(&self, i: Option<usize>) {
        if self.imp().sel.replace(i) != i {
            if let Some(f) = self.imp().on_sel.borrow().as_ref() {
                f(i);
            }
            self.queue_draw();
        }
    }

    fn emit(&self, c: Vec<(f64, f64)>) {
        if let Some(f) = self.imp().on_curve.borrow().as_ref() {
            f(c);
        }
    }

    fn bind_input(&self) {
        let motion = gtk::EventControllerMotion::new();
        let w = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(s) = w.upgrade() {
                s.imp().hover.set(Some((x as f32, y as f32)));
                let over = s.hit(x as f32, y as f32).is_some();
                s.set_cursor_from_name(Some(if over || s.imp().drag.get().is_some() { "grab" } else { "default" }));
                s.queue_draw();
            }
        });
        let w = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(s) = w.upgrade() {
                s.imp().hover.set(None);
                s.queue_draw();
            }
        });
        self.add_controller(motion);
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let w = self.downgrade();
        drag.connect_drag_begin(move |_, x, y| {
            let Some(s) = w.upgrade() else { return };
            s.grab_focus();
            let hit = s.hit(x as f32, y as f32);
            s.imp().drag.set(hit);
            s.select(hit);
            if hit.is_some() {
                s.set_cursor_from_name(Some("grabbing"));
            }
        });
        let w = self.downgrade();
        drag.connect_drag_update(move |g, dx, dy| {
            let Some(s) = w.upgrade() else { return };
            let Some(i) = s.imp().drag.get() else { return };
            if let Some((x0, y0)) = g.start_point() {
                let coarse = g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
                let (t, f) = s.to_val((x0 + dx) as f32, (y0 + dy) as f32);
                let c = s.imp().curve.borrow().clone();
                let next = move_point(&c, i, snap_value(t, coarse), snap_value(f, coarse), s.imp().floor.get());
                if next != c {
                    s.emit(next);
                }
            }
        });
        let w = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            if let Some(s) = w.upgrade() {
                s.imp().drag.set(None);
                s.set_cursor_from_name(Some("default"));
                s.queue_draw();
            }
        });
        self.add_controller(drag);
        // Dobbeltklikk: legg til et punkt, eller fjern punktet som ble klikket.
        let click = gtk::GestureClick::new();
        let w = self.downgrade();
        click.connect_pressed(move |_, n, x, y| {
            let Some(s) = w.upgrade() else { return };
            if n != 2 {
                return;
            }
            let c = s.imp().curve.borrow().clone();
            let floor = s.imp().floor.get();
            if let Some(i) = s.hit(x as f32, y as f32) {
                if let Some(next) = remove_point(&c, i) {
                    s.select(None);
                    s.emit(next);
                }
            } else if let Some((next, i)) = add_point(&c, s.to_val(x as f32, y as f32).0, floor) {
                s.select(Some(i));
                s.emit(next);
            }
        });
        self.add_controller(click);
        let keys = gtk::EventControllerKey::new();
        let w = self.downgrade();
        keys.connect_key_pressed(move |_, k, _, m| {
            let Some(s) = w.upgrade() else { return glib::Propagation::Proceed };
            if m.contains(gdk::ModifierType::CONTROL_MASK) || m.contains(gdk::ModifierType::ALT_MASK) {
                return glib::Propagation::Proceed;
            }
            let c = s.imp().curve.borrow().clone();
            let floor = s.imp().floor.get();
            let big = if m.contains(gdk::ModifierType::SHIFT_MASK) { 5.0 } else { 1.0 };
            let n = c.len();
            let sel = s.imp().sel.get();
            let pick = |i: usize| s.select(Some(i.min(n.saturating_sub(1))));
            match k {
                gdk::Key::bracketleft | gdk::Key::Page_Up => pick(sel.map(|i| i.saturating_sub(1)).unwrap_or(0)),
                gdk::Key::bracketright | gdk::Key::Page_Down => pick(sel.map(|i| i + 1).unwrap_or(0)),
                gdk::Key::Home => pick(0),
                gdk::Key::End => pick(n.saturating_sub(1)),
                gdk::Key::Left | gdk::Key::Right | gdk::Key::Up | gdk::Key::Down => {
                    let Some(i) = sel else {
                        pick(0);
                        return glib::Propagation::Stop;
                    };
                    let (t, f) = c[i];
                    let (dt, df) = match k {
                        gdk::Key::Left => (-big, 0.0),
                        gdk::Key::Right => (big, 0.0),
                        gdk::Key::Up => (0.0, big),
                        _ => (0.0, -big),
                    };
                    let next = move_point(&c, i, t + dt, f + df, floor);
                    if next != c {
                        s.emit(next);
                    }
                }
                gdk::Key::Delete | gdk::Key::BackSpace | gdk::Key::minus | gdk::Key::KP_Subtract => {
                    if let Some(i) = sel
                        && let Some(next) = remove_point(&c, i)
                    {
                        s.select(Some(i.saturating_sub(1)));
                        s.emit(next);
                    }
                }
                gdk::Key::Insert | gdk::Key::plus | gdk::Key::KP_Add => {
                    if let Some((next, i)) = add_after(&c, sel.unwrap_or(0), floor) {
                        s.select(Some(i));
                        s.emit(next);
                    }
                }
                gdk::Key::Escape if sel.is_some() => s.select(None),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.add_controller(keys);
    }

    pub fn set_points(&self, mut pts: Vec<(f64, f64)>, empty: &str) {
        pts.retain(|(t, f)| t.is_finite() && f.is_finite());
        *self.imp().median.borrow_mut() = median_per_degree(&pts);
        *self.imp().pts.borrow_mut() = pts;
        *self.imp().empty.borrow_mut() = empty.to_string();
        self.queue_draw();
    }

    pub fn set_now(&self, now: Option<(f64, f64)>) {
        let now = now.filter(|(t, f)| t.is_finite() && f.is_finite());
        if self.imp().now.get() != now {
            self.imp().now.set(now);
            self.queue_draw();
        }
    }

    /// Kurven som vises, og om den er et utkast.
    pub fn set_curve(&self, c: Vec<(f64, f64)>, draft: bool) {
        let i = self.imp();
        if *i.curve.borrow() != c || i.draft.get() != draft {
            if i.sel.get().is_some_and(|s| s >= c.len()) {
                i.sel.set(None);
            }
            *i.curve.borrow_mut() = c;
            i.draft.set(draft);
            self.queue_draw();
        }
    }

    /// Den andre viftens kurve (tom: ingen), hysteresen og «hold minimum til».
    pub fn set_shape(&self, ghost: Vec<(f64, f64)>, hyst: f64, quiet: Option<f64>) {
        let i = self.imp();
        if *i.ghost.borrow() != ghost || i.hyst.get() != hyst || i.quiet.get() != quiet {
            *i.ghost.borrow_mut() = ghost;
            i.hyst.set(hyst);
            i.quiet.set(quiet);
            self.queue_draw();
        }
    }

    pub fn set_floor(&self, f: f64, known: bool) {
        let changed = self.imp().floor.replace(f) != f;
        let known_changed = self.imp().floor_known.replace(known) != known;
        if changed || known_changed {
            self.queue_draw();
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.imp().sel.get()
    }

    pub fn connect_curve(&self, f: impl Fn(Vec<(f64, f64)>) + 'static) {
        *self.imp().on_curve.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_select(&self, f: impl Fn(Option<usize>) + 'static) {
        *self.imp().on_sel.borrow_mut() = Some(Box::new(f));
    }
}

// ================================================================ forhåndsvisningen

/// Siste fem minutter fra historikken, og hva viftene ville gjort med kurven.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewData {
    pub t: Vec<i64>,
    pub temp: Vec<Option<f64>>,
    /// Den høyeste av viftene, slik de faktisk gikk.
    pub actual: Vec<Option<f64>>,
    /// Med kurven: én rad per vifte (bare én når kurven er felles).
    pub sim: Vec<Vec<f64>>,
}

/// Snitt, høyeste og hvor mange ganger farten endret seg (hele prosent):
/// et mål på hvor mye viftene «suser» opp og ned.
pub fn fan_summary(v: &[f64]) -> Option<(f64, f64, usize)> {
    if v.is_empty() {
        return None;
    }
    let avg = v.iter().sum::<f64>() / v.len() as f64;
    let max = v.iter().copied().fold(f64::MIN, f64::max);
    let changes = v.windows(2).filter(|w| w[0].round() != w[1].round()).count();
    Some((avg, max, changes))
}

/// Skalaen i «Siste 5 minutter» (% og °C på samme akse): 0–100, utvidet i
/// steg på 25 når temperaturen eller viftene går utenfor.
pub fn preview_scale(vals: impl Iterator<Item = f64>) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0f64, 100.0f64);
    for v in vals.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    ((lo / 25.0).floor() * 25.0, (hi / 25.0).ceil() * 25.0 + if hi > 100.0 && hi % 25.0 == 0.0 { 25.0 } else { 0.0 })
}

/// Kjører viftemotoren over historikken: temperaturen per sekund og farten
/// viftene hadde først. Gir én rad per kurve (felles: én).
pub fn simulate(v: &Values, fans: usize, floor: f64, t: &[i64], temp: &[Option<f64>], start: Option<f64>) -> Vec<Vec<f64>> {
    let o = vifte::Oppsett::fra_values(v, fans.max(1), floor);
    let rows = if o.felles() { 1 } else { o.vifter };
    let prover: Vec<(i64, Option<f64>)> = t.iter().copied().zip(temp.iter().copied()).collect();
    // Hull i historikken (tjenesten sov): motoren holder farten, som på kortet.
    let steps = vifte::simuler(&o, &prover, start.map(|s| vec![s; o.vifter]).as_deref());
    (0..rows).map(|i| steps.iter().map(|s| s.prosent.get(i).copied().unwrap_or(floor)).collect()).collect()
}

mod preview_imp {
    use super::*;

    pub struct Preview {
        pub data: RefCell<PreviewData>,
        pub floor: Cell<f64>,
        /// temperatur, faktisk, kurven, rutenett, tekst, kort
        pub colors: RefCell<[gdk::RGBA; 6]>,
        pub empty: RefCell<String>,
    }

    impl Default for Preview {
        fn default() -> Self {
            Preview { data: RefCell::default(), floor: Cell::new(35.0), colors: RefCell::new([gdk::RGBA::BLACK; 6]), empty: RefCell::default() }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Preview {
        const NAME: &'static str = "RiggFanPreview";
        type Type = super::Preview;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("fanpreview");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Preview {}

    impl WidgetImpl for Preview {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (240, 600, -1, -1),
                _ => (150, 170, -1, -1),
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            let [temp_c, actual_c, sim_c, grid, text, card] = *self.colors.borrow();
            let (pl, pt, pr, pb) = (44.0f32, 8.0f32, 44.0f32, 20.0f32);
            let pw = (ww - pl - pr).max(10.0);
            let ph = (hh - pt - pb).max(10.0);
            let d = self.data.borrow();
            // Samme skala for % og °C, 0–100 som utgangspunkt. En temperatur
            // over 100 °C (eller under 0) utvider skalaen i steg på 25, så
            // kurven aldri kuttes (runde 8A).
            let (vlo, vhi) = preview_scale(d.temp.iter().flatten().chain(d.actual.iter().flatten()).copied());
            let y = |v: f64| pt + ph - (((v - vlo) / (vhi - vlo)) as f32).clamp(0.0, 1.0) * ph;
            let lay = |txt: &str, xx: f32, yy: f32, ax: f32, ay: f32, col: &gdk::RGBA| {
                let l = w.create_pango_layout(Some(txt));
                let (lw, lh) = l.pixel_size();
                s.save();
                s.translate(&graphene::Point::new((xx - lw as f32 * ax).round(), (yy - lh as f32 * ay).round()));
                s.append_layout(&l, col);
                s.restore();
            };
            // Rutenettet: samme skala for % (venstre) og °C (høyre).
            let mut v = vlo + 25.0;
            while v <= vhi + 1e-9 {
                s.append_color(&grid, &graphene::Rect::new(pl, y(v).round() - 0.5, pw, 1.0));
                if (0.0..=100.0).contains(&v) {
                    lay(&F::pct(Some(v), 0), pl - 6.0, y(v), 1.0, 0.5, &text);
                }
                lay(&F::temp(Some(v), 0), pl + pw + 6.0, y(v), 0.0, 0.5, &text);
                v += 25.0;
            }
            let floor = self.floor.get();
            {
                let p = crate::widgets::Painter::new(s, &graphene::Rect::new(pl, y(floor) - 2.0, pw, 4.0));
                p.stroke([(pl, y(floor).round()), (pl + pw, y(floor).round())], 1.0, false, Some(&[4.0f32, 3.0][..]), &text);
            }
            let (Some(t0), Some(t1)) = (d.t.first().copied(), d.t.last().copied()) else {
                let e = self.empty.borrow();
                lay(&e, pl + pw / 2.0, pt + ph / 2.0, 0.5, 0.5, &text);
                return;
            };
            let span = (t1 - t0).max(1) as f32;
            let x = |t: i64| pl + ((t - t0) as f32 / span) * pw;
            lay("−5 min", pl, pt + ph + 4.0, 0.0, 0.0, &text);
            lay(tr!("nå"), pl + pw, pt + ph + 4.0, 1.0, 0.0, &text);
            let clip = graphene::Rect::new(pl, pt - 2.0, pw, ph + 4.0);
            let p = crate::widgets::Painter::new(s, &clip);
            // Linjer med hull der verdien mangler.
            let line = |vals: &mut dyn Iterator<Item = (i64, Option<f64>)>, width: f32, dash: Option<&[f32]>, col: &gdk::RGBA| {
                let mut run: Vec<(f32, f32)> = Vec::new();
                for (t, v) in vals {
                    match v {
                        Some(v) => run.push((x(t), y(v))),
                        None if run.len() > 1 => p.stroke(std::mem::take(&mut run), width, true, dash, col),
                        None => run.clear(),
                    }
                }
                if run.len() > 1 {
                    p.stroke(run, width, true, dash, col);
                }
            };
            line(&mut d.t.iter().copied().zip(d.temp.iter().copied()), 1.25, None, &temp_c);
            line(&mut d.t.iter().copied().zip(d.actual.iter().copied()), 1.5, None, &actual_c);
            for (i, row) in d.sim.iter().enumerate() {
                line(&mut d.t.iter().copied().zip(row.iter().map(|v| Some(*v))), if i == 0 { 2.0 } else { 1.5 }, if i == 0 { None } else { Some(&[5.0f32, 3.0][..]) }, &sim_c);
            }
            let _ = card;
        }
    }
}

glib::wrapper! {
    /// Forhåndsvisningen: temperaturen, viftene slik de gikk, og med kurven.
    pub struct Preview(ObjectSubclass<preview_imp::Preview>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Preview {
    pub fn new(app: &Rc<App>) -> Preview {
        let s: Preview = glib::Object::new();
        s.set_hexpand(true);
        s.imp().floor.set(35.0);
        s.update_property(&[gtk::accessible::Property::Label(tr!("Viftene de siste fem minuttene, slik de gikk og med kurven"))]);
        let set = {
            let w = s.downgrade();
            let a = app.clone();
            move || {
                let Some(s) = w.upgrade() else { return };
                let c = |t: &str| crate::theme::rgba(&a, t);
                let mut actual = c("--muted");
                actual.set_alpha(0.9);
                *s.imp().colors.borrow_mut() = [c("--c-temp"), actual, c("--accent"), c("--grid"), c("--muted"), c("--card")];
                s.queue_draw();
            }
        };
        set();
        app.on_theme.connect(move |_| set());
        s
    }

    pub fn set_data(&self, d: PreviewData, floor: f64, empty: &str) {
        let i = self.imp();
        if *i.data.borrow() != d || i.floor.get() != floor || *i.empty.borrow() != empty {
            *i.data.borrow_mut() = d;
            i.floor.set(floor);
            *i.empty.borrow_mut() = empty.to_string();
            self.queue_draw();
        }
    }
}

// ================================================================ siden

/// Modusene med forklaring.
fn mode_options() -> [(FanMode, &'static str, &'static str); 3] {
    [
        (FanMode::Auto, tr!("Auto (driveren)"), tr!("Driveren styrer viftene etter temperaturen, som uten appen. Standard.")),
        (FanMode::Curve, tr!("Kurve"), tr!("Agentic HW Monitor setter farten etter viftekurven hvert sekund.")),
        (FanMode::Manual, tr!("Manuell"), tr!("Fast fart, uansett temperatur. Pass på temperaturen under last.")),
    ]
}

fn simple_curve_message(card: FanMode, shown: FanMode, editing: bool) -> Option<&'static str> {
    if card == FanMode::Curve {
        Some(if editing && shown != FanMode::Curve {
            tr!("Egen kurve er aktiv på kortet. Modusbyttet ligger bare i utkastet til du trykker Bruk og Behold.")
        } else if editing {
            tr!("Egen kurve er aktiv på kortet. Du har et kurveutkast; åpne Avansert for å se og redigere det.")
        } else {
            tr!("Egen kurve er aktiv. Viftene fortsetter å følge den i Enkel visning. Åpne Avansert for å se og redigere kurven.")
        })
    } else if shown == FanMode::Curve {
        Some(tr!("Egen kurve er valgt i utkastet. Åpne Avansert for å se kurven. Kortet endres først med Bruk og Behold."))
    } else {
        None
    }
}

/// Some(0) er et faktisk svar; manglende vifteantall er fortsatt ukjent.
fn actual_fan_count(app: &App) -> Option<usize> {
    app.statics().and_then(|s| s.fan_count).map(|n| n as usize)
}

fn fan_target_choices(count: Option<usize>) -> Vec<(String, String)> {
    let mut choices = vec![("0".into(), tr!("Felles kurve").into())];
    for i in 1..=count.unwrap_or(0).min(vifte::MAKS_VIFTER) {
        choices.push((i.to_string(), trf!("Vifte {i}", i = i)));
    }
    choices
}

fn make_target_selector(count: Option<usize>) -> Seg {
    let choices = fan_target_choices(count);
    let refs: Vec<_> = choices.iter().map(|(id, title)| (id.as_str(), title.as_str())).collect();
    let seg = Seg::new(&refs, "0", tr!("Kurven som redigeres")).small();
    seg.root.set_valign(gtk::Align::Start);
    seg.root.add_css_class("target-seg");
    if let Some(b) = seg.button("0") { tip(&b, tr!("Én felles kurve for kortets vifter"), None); }
    for (id, _) in choices.iter().skip(1) {
        if let Some(b) = seg.button(id) { tip(&b, &trf!("Egen kurve for vifte {id}. En annen vifte kan vises svakt bak.", id = id), None); }
    }
    seg
}

fn measured_fan_range(fans: &[Option<f64>], count: Option<usize>) -> Option<(f64, f64, usize)> {
    fans.iter().take(count.unwrap_or(fans.len())).flatten().copied().filter(|v| v.is_finite()).fold(None, |range, v| {
        Some(range.map_or((v, v, 1), |(lo, hi, n): (f64, f64, usize)| (lo.min(v), hi.max(v), n + 1)))
    })
}

fn fan_control_unavailable(count: Option<usize>, mode: FanMode) -> Option<String> {
    match count {
        None => Some(tr!("Antall GPU-vifter er ikke lest. Viftestyring venter på kortets informasjon.").into()),
        Some(0) => Some(tr!("Kortet rapporterer ingen GPU-vifter. Viftestyring er utilgjengelig.").into()),
        Some(n) if mode == FanMode::Curve && n > vifte::MAKS_VIFTER => Some(trf!("Kortet rapporterer {n} vifter. Kurvemotoren støtter høyst {maks} vifter på ett kort.", n = n, maks = vifte::MAKS_VIFTER)),
        _ => None,
    }
}

#[derive(Debug, Default, PartialEq)]
struct HiddenDraft {
    lines: Vec<String>,
    controls: bool,
    advanced_fans: bool,
}

/// Hele patchen deles med Kontroller. Også inaktive kurveinnstillinger kan
/// følge med en annen endring, selv om draft_diff utelater dem fra Bruk-porten.
fn simple_hidden_draft(before: &Values, after: &Values) -> HiddenDraft {
    fn curve(points: &[(f64, f64)]) -> String {
        if points.is_empty() { return tr!("ingen").into(); }
        points.iter().map(|(t, p)| format!("{} / {}", F::temp(Some(*t), 0), F::pct(Some(*p), 0))).collect::<Vec<_>>().join(" · ")
    }
    fn text(v: &Values, field: &str) -> (&'static str, String) {
        match field {
            "powerLimitW" => (tr!("Effektgrense"), F::watt(Some(v.power_limit_w), 0)),
            "clockOffsets.coreMhz" => (tr!("GPU-justering"), locked::signed(v.core_offset_mhz, "MHz")),
            "clockOffsets.memMhz" => (tr!("Minnejustering"), locked::signed(v.mem_offset_mhz, "MHz")),
            "tempTargetC" => (tr!("Temperaturmål"), F::temp(Some(v.temp_target_c), 0)),
            "fan.manualPercent" => (tr!("Lagret fast fart"), F::pct(Some(v.fan_percent), 0)),
            "fan.curve" => (tr!("Felles viftekurve"), curve(&v.fan_curve)),
            "fan.perFan" => (tr!("Viftekurver"), if v.fan_per_fan { tr!("per vifte") } else { tr!("felles") }.into()),
            "fan.curves" => (tr!("Kurver per vifte"), if v.fan_curves.is_empty() { tr!("ingen egne kurver").into() } else { v.fan_curves.iter().enumerate().map(|(i, c)| trf!("Vifte {nummer}: {antall}", nummer = i + 1, antall = curve(c))).collect::<Vec<_>>().join("; ") }),
            "fan.hysteresisC" => (tr!("Hysterese"), F::temp_delta(v.fan_hysteresis_c, 0)),
            "fan.rampUpPctS" => (tr!("Rampe opp"), rate_text(v.fan_ramp_up_pct_s)),
            "fan.rampDownPctS" => (tr!("Rampe ned"), rate_text(v.fan_ramp_down_pct_s)),
            "fan.smoothingS" => (tr!("Utjevning"), if v.fan_smoothing_s > 0.0 { format!("{}{}s", v.fan_smoothing_s, F::NNBSP) } else { tr!("av").into() }),
            "fan.quietUntilC" => (tr!("Hold minimum til"), v.fan_quiet_until_c.map(|t| F::temp(Some(t), 0)).unwrap_or_else(|| tr!("av").into())),
            _ => (tr!("Annen innstilling"), field.into()),
        }
    }
    let mut result = HiddenDraft::default();
    for field in before.diff(after) {
        if field == "fan.mode" || field == "fan.manualPercent" && after.fan_mode == FanMode::Manual {
            continue;
        }
        let (name, from) = text(before, &field);
        let (_, to) = text(after, &field);
        result.lines.push(format!("{name}: {from} → {to}"));
        if field.starts_with("fan.") { result.advanced_fans = true; }
        else { result.controls = true; }
    }
    result
}

pub struct Viftekurve {
    app: Rc<App>,
    outer: gtk::Box,
    root: gtk::Box,
    view_seg: Seg,
    advanced_area: Columns,
    mode_titles: Vec<(FanMode, gtk::Label)>,
    simple_curve: gtk::Box,
    simple_curve_text: gtk::Label,
    open_advanced: gtk::Button,
    apply_note: gtk::Label,
    simple_changes: gtk::Box,
    simple_changes_text: gtk::Label,
    simple_changes_note: gtk::Label,
    open_controls: gtk::Button,
    changes_advanced: gtk::Button,
    fan_readouts: FanReadouts,
    actions: gtk::Box,
    strip: Rc<LockStrip>,
    stats: Vec<Rc<Stat>>,
    /// Flisene øverst: 6, 3 eller 2 per rad, aldri én alene (7C).
    editor: CurveEdit,
    window: Cell<i64>,
    win_seg: Seg,
    count: gtk::Label,
    head_note: gtk::Label,
    name: gtk::Label,
    reset_curve: gtk::Button,
    use_curve: gtk::Button,
    mode_btns: Vec<(FanMode, gtk::ToggleButton, gtk::Box)>,
    manual_box: gtk::Box,
    manual: oc::Slider,
    manual_spin: gtk::SpinButton,
    pts_box: gtk::Box,
    add_pt: gtk::Button,
    rules: gtk::Label,
    day_kv: Vec<gtk::Label>,
    day_line: gtk::Label,
    dirty: gtk::Label,
    discard: gtk::Button,
    apply: gtk::Button,
    confirm: Rc<ConfirmBar>,
    charts: Rc<ChartSet>,
    visible: Cell<bool>,
    narrow: Rc<Cell<bool>>,
    timer: RefCell<Option<glib::SourceId>>,
    quiet: Cell<bool>,
    // ---- Runde 7B ----
    /// Kurven som redigeres: 0 = felles, 1 = vifte 1, 2 = vifte 2.
    target: Cell<usize>,
    target_seg: RefCell<Seg>,
    target_host: gtk::ScrolledWindow,
    target_count: Cell<Option<usize>>,
    lg_down: gtk::Widget,
    lg_ghost: gtk::Widget,
    tune: Tuning,
    preview: Preview,
    preview_note: gtk::Label,
    preview_title: gtk::Label,
    /// Forklaringen over forhåndsvisningen: «Med kurven» (felles) eller
    /// «Vifte 1 med kurven» og «Vifte 2» stiplet (egne kurver per vifte).
    preview_lg: (gtk::Label, gtk::FlowBoxChild),
    /// Historikken til forhåndsvisningen: tid, temperatur, viftene (høyeste).
    preview_hist: RefCell<Option<History5>>,
    /// Det forhåndsvisningen sist ble regnet ut for (unngår å regne hver runde).
    preview_key: RefCell<Option<(Values, Option<usize>, f64)>>,
    preview_timer: RefCell<Option<glib::SourceId>>,
    /// Nødregelen i «Slik følges kurven» (enheten kan byttes).
    rule_emerg: gtk::Label,
}

/// Fem minutter fra historikken: tid (ms), temperatur og viftene (høyeste).
type History5 = (Vec<i64>, Vec<Option<f64>>, Vec<Option<f64>>);

/// Kontrollene for hvordan kurven følges (panelet «Oppførsel»).
struct Tuning {
    hyst: oc::Slider,
    hyst_val: gtk::Label,
    hyst_note: gtk::Label,
    up: oc::Slider,
    up_val: gtk::Label,
    up_note: gtk::Label,
    down: oc::Slider,
    down_val: gtk::Label,
    down_note: gtk::Label,
    smooth: Seg,
    smooth_note: gtk::Label,
    quiet_sw: gtk::Switch,
    quiet_spin: gtk::SpinButton,
    quiet_unit: gtk::Label,
    quiet_note: gtk::Label,
    stop_note: gtk::Label,
}

struct FanReadouts {
    now: gtk::Label,
    target_title: gtk::Label,
    target: gtk::Label,
    target_note: gtk::Label,
    max_root: gtk::Box,
    max: gtk::Label,
    max_note: gtk::Label,
}

fn fan_readout(title: &str) -> (gtk::Box, gtk::Label, gtk::Label, gtk::Label) {
    let root = vbox(2, &["fan-reading"]);
    let title = label(title, &["muted", "sm"]);
    let value = label("–", &["num", "fan-reading-value"]);
    let note = label("", &["muted", "sm"]);
    for text in [&title, &value, &note] {
        text.set_wrap(true);
        text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        text.set_xalign(0.0);
        root.append(text);
    }
    (root, title, value, note)
}

/// Én kontroll i «Oppførsel»: tittel og verdi, kontrollen og en forklaring.
fn tune_cell(title: &str, ctl: &impl IsA<gtk::Widget>) -> (gtk::Box, gtk::Label, gtk::Label) {
    let b = vbox(4, &["tune"]);
    let top = hbox(6, &[]);
    let t = label(title, &["sm", "bold"]);
    t.set_hexpand(true);
    t.set_xalign(0.0);
    top.append(&t);
    let v = label("", &["sm", "num", "tune-val"]);
    v.set_xalign(1.0);
    top.append(&v);
    b.append(&top);
    b.append(ctl);
    let n = label("", &["sm", "muted", "tune-note"]);
    n.set_wrap(true);
    n.set_xalign(0.0);
    n.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    b.append(&n);
    (b, v, n)
}

/// «7 s», «1 min 5 s»: hvor lang tid rampen bruker.
fn secs_text(s: f64) -> String {
    let s = s.round().max(1.0) as i64;
    if s < 60 { format!("{s}{}s", F::NNBSP) } else { format!("{}{}min {}{}s", s / 60, F::NNBSP, s % 60, F::NNBSP) }
}

/// «10 %/s»
fn rate_text(v: f64) -> String {
    format!("{}{}%/s", F::num(Some(v), 0), F::NNBSP)
}

struct Wrap(Rc<Viftekurve>);

impl Page for Wrap {
    fn widget(&self) -> gtk::Widget {
        self.0.outer.clone().upcast()
    }
    fn scrolls(&self) -> bool {
        false
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.0.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        self.0.visible.set(true);
        self.0.charts.set_visible(true);
        self.0.load();
        self.0.refresh();
        let w = Rc::downgrade(&self.0);
        if let Some(t) = self.0.timer.borrow_mut().take() {
            t.remove();
        }
        *self.0.timer.borrow_mut() = Some(glib::timeout_add_seconds_local(60, move || {
            let Some(p) = w.upgrade() else { return glib::ControlFlow::Break };
            p.load();
            glib::ControlFlow::Continue
        }));
        // Forhåndsvisningen følger de siste fem minuttene (hvert 10. s).
        let w = Rc::downgrade(&self.0);
        if let Some(t) = self.0.preview_timer.borrow_mut().take() {
            t.remove();
        }
        *self.0.preview_timer.borrow_mut() = Some(glib::timeout_add_seconds_local(10, move || {
            let Some(p) = w.upgrade() else { return glib::ControlFlow::Break };
            p.load_preview();
            glib::ControlFlow::Continue
        }));
    }
    fn hide(&self) {
        self.0.visible.set(false);
        self.0.charts.set_visible(false);
        if let Some(t) = self.0.timer.borrow_mut().take() {
            t.remove();
        }
        if let Some(t) = self.0.preview_timer.borrow_mut().take() {
            t.remove();
        }
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.ctrl && matches!(k.key, gdk::Key::Return | gdk::Key::KP_Enter) {
            if locked::stored(&self.0.app).utkast.is_some() && self.0.apply.is_sensitive() {
                self.0.do_apply();
            }
            return true;
        }
        if k.ctrl && !k.typing && matches!(k.key, gdk::Key::z | gdk::Key::Z | gdk::Key::y | gdk::Key::Y) {
            locked::undo_draft(&self.0.app, k.shift || matches!(k.key, gdk::Key::y | gdk::Key::Y));
            return true;
        }
        false
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "styring", "viftekurve"]);
    root.append(&crate::widgets::styrer::TargetLine::new(app).root);
    let strip = LockStrip::new(app, tr!("Viser hvordan driveren faktisk har styrt viftene. Det du endrer, blir et utkast som lagres i appen og ikke sendes til kortet."));
    root.append(&strip.root);
    let confirm = ConfirmBar::new(app);
    // Behold må være synlig også når Bruk trykkes etter rulling til bunnen.
    let outer = vbox(0, &["fan-page-shell"]);
    confirm.root.add_css_class("sticky-confirm");
    outer.append(&confirm.root);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vexpand(true);
    scroll.set_hexpand(true);
    scroll.set_child(Some(&root));
    outer.append(&scroll);

    // ---- Viftene nå ----
    let st = StatStrip::new(6, 3);
    let stats: Vec<Rc<Stat>> = [tr!("Temperatur"), tr!("Viftefart nå"), tr!("Vifter oppdaget"), tr!("Mål-fart"), tr!("Modus"), tr!("Kurven gir")].iter().map(|n| Stat::new(app, n)).collect();
    for s in &stats {
        st.add(s);
    }
    stats[3].meter.set_visible(false);
    stats[4].meter.set_visible(false);
    stats[2].meter.set_visible(false);
    let live = vbox(0, &["group", "oc-live"]);
    live.append(&st.root);
    root.append(&live);

    // ---- Kurven ----
    let main = Panel::new(tr!("Kortets viftekurve"));
    main.root.add_css_class("curve-card");
    let name = label("", &["tag", "quiet", "curve-name"]);
    name.set_valign(gtk::Align::Center);
    // Kuttes heller enn å holde vinduet bredt (640 px).
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_width_chars(6);
    name.set_hexpand(true);
    name.set_halign(gtk::Align::Start);
    main.head.insert_child_after(&name, Some(&main.title));
    let mut preset_btns = Vec::new();
    let presets = hbox(2, &["preset-row"]);
    for p in PRESETS {
        let b = button(None, Some((p.name)()), &["sm", "flat"]);
        tip(&b, &trf!("{navn}: {om} (i utkastet, felles for kortets vifter)", navn = (p.name)(), om = (p.about)()), None);
        presets.append(&b);
        preset_btns.push((p.id, b));
    }
    let reset_curve = button(Some("rotate-ccw"), Some(tr!("Tilbakestill kurven")), &["sm"]);
    tip(&reset_curve, tr!("Kurven, hysteresen, rampen og resten av oppførselen tilbake til standard (Balansert)"), None);
    main.aside.append(&presets);
    main.aside.append(&reset_curve);
    let legend_row = hbox(10, &["legend-row"]);
    // Felles kurve eller én per vifte til venstre, tidsvinduet og dekningen
    // til høyre. Forklaringen står under grafen og brytes i smale vinduer.
    let target_count = actual_fan_count(app);
    let target_seg = make_target_selector(target_count);
    let target_host = gtk::ScrolledWindow::new();
    target_host.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    target_host.set_min_content_width(140);
    target_host.set_hexpand(true);
    target_host.set_valign(gtk::Align::Start);
    target_host.set_child(Some(&target_seg.root));
    let legend = gtk::FlowBox::new();
    legend.add_css_class("legend");
    legend.set_selection_mode(gtk::SelectionMode::None);
    legend.set_column_spacing(16);
    legend.set_row_spacing(2);
    legend.set_min_children_per_line(2);
    legend.set_max_children_per_line(6);
    legend.set_homogeneous(false);
    legend.set_halign(gtk::Align::Start);
    legend.set_focusable(false);
    let mut lg_items: Vec<gtk::Widget> = Vec::new();
    for (cls, txt) in [("lg-curve", tr!("Kurven")), ("lg-down", tr!("Ned (hysterese)")), ("lg-ghost", tr!("Annen vifte")), ("lg-dots", tr!("Målinger")), ("lg-med", tr!("Driveren (median)")), ("lg-now", tr!("Nå"))] {
        let b = hbox(5, &[]);
        let sw = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sw.add_css_class("lg");
        sw.add_css_class(cls);
        sw.set_valign(gtk::Align::Center);
        sw.set_halign(gtk::Align::Center);
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cell.set_size_request(14, -1);
        cell.append(&sw);
        b.append(&cell);
        b.append(&label(txt, &["muted", "sm"]));
        let ch = gtk::FlowBoxChild::new();
        ch.set_child(Some(&b));
        // Forklaringen er ikke et stopp for Tab.
        ch.set_focusable(false);
        ch.set_can_target(false);
        legend.insert(&ch, -1);
        lg_items.push(ch.upcast());
    }
    let lg_down = lg_items[1].clone();
    let lg_ghost = lg_items[2].clone();
    // Tidsvinduet og dekningen under hverandre til høyre: «Data for 1 min av
    // 48 t» får alltid plass, også ved 640 px.
    let count = label("", &["muted", "sm", "num", "curve-count"]);
    count.set_xalign(1.0);
    let win_seg = Seg::new(&[("3600", tr!("1 t")), ("21600", tr!("6 t")), ("86400", tr!("24 t")), ("172800", tr!("48 t"))], "172800", tr!("Tidsvindu for målingene")).small();
    let win_box = vbox(2, &["curve-window"]);
    win_box.set_valign(gtk::Align::Start);
    win_box.set_hexpand(true);
    win_box.set_halign(gtk::Align::End);
    win_seg.root.set_halign(gtk::Align::End);
    win_box.append(&win_seg.root);
    win_box.append(&count);
    legend_row.append(&target_host);
    legend_row.append(&win_box);
    main.body.append(&legend_row);
    let edit = CurveEdit::new(app);
    main.body.append(&edit);
    main.body.append(&legend);
    let foot = hbox(8, &["curve-foot"]);
    let head_note = label("", &["muted", "sm"]);
    head_note.set_wrap(true);
    head_note.set_hexpand(true);
    let use_curve = button(None, Some(tr!("Bruk kurven")), &["sm"]);
    tip(&use_curve, tr!("Setter viftemodus til Kurve i utkastet"), None);
    foot.append(&head_note);
    foot.append(&use_curve);
    main.body.append(&foot);
    let help = label(tr!("Dra punktene; de festes til hele grader og prosent (Shift: steg på 5). Dobbeltklikk legger til eller fjerner et punkt (2–10 punkter). Tastaturet: [ og ] velger punkt, piltastene flytter, + legger til, Delete fjerner, Ctrl+Z angrer."), &["muted", "sm", "curve-help"]);
    help.set_wrap(true);
    main.body.append(&help);

    // ---- Oppførsel: hysterese, rampe, utjevning og stille flate ----
    let tune_p = Panel::new(tr!("Oppførsel"));
    tune_p.root.add_css_class("tune-card");
    let tune_grid = Cols::new(&[1.0, 1.0], 16, 520);
    let mk_slider = |name: &str, lo: f64, hi: f64, step: f64, big: f64, fmt: fn(f64) -> String| {
        let sl = oc::Slider::new(app);
        sl.update_property(&[gtk::accessible::Property::Label(name)]);
        sl.set_range(lo, hi, (lo, hi));
        sl.set_steps(step, big);
        sl.set_fmt(fmt);
        sl
    };
    let hyst = mk_slider(tr!("Hysterese"), vifte::HYSTERESE.min, vifte::HYSTERESE.max, 1.0, 2.0, |v| F::temp_delta(v, 0));
    hyst.set_stock(Some(vifte::HYSTERESE_STANDARD_C));
    let (hyst_cell, hyst_val, hyst_note) = tune_cell(tr!("Hysterese"), &hyst);
    let smooth = Seg::new(&[("0", tr!("Av")), ("3", "3 s"), ("5", "5 s"), ("10", "10 s")], "0", tr!("Utjevning av temperaturen")).small();
    let (smooth_cell, smooth_val, smooth_note) = tune_cell(tr!("Utjevning av temperaturen"), &smooth.root);
    smooth_val.set_visible(false);
    let up = mk_slider(tr!("Rampe opp"), vifte::RAMPE.min, 50.0, 1.0, 5.0, rate_text);
    up.set_stock(Some(vifte::RAMPE_OPP_STANDARD));
    let (up_cell, up_val, up_note) = tune_cell(tr!("Rampe opp"), &up);
    let down = mk_slider(tr!("Rampe ned"), vifte::RAMPE.min, 50.0, 1.0, 5.0, rate_text);
    down.set_stock(Some(vifte::RAMPE_NED_STANDARD));
    let (down_cell, down_val, down_note) = tune_cell(tr!("Rampe ned"), &down);
    let quiet_row = hbox(8, &["quiet-row"]);
    let quiet_sw = gtk::Switch::new();
    quiet_sw.set_valign(gtk::Align::Center);
    quiet_sw.update_property(&[gtk::accessible::Property::Label(tr!("Hold minste fart til en temperatur"))]);
    let ql = label(tr!("Hold minimum til"), &["sm"]);
    let quiet_spin = gtk::SpinButton::with_range(vifte::STILLE.min, vifte::STILLE.max, 1.0);
    quiet_spin.set_digits(0);
    quiet_spin.set_width_chars(4);
    quiet_spin.set_value(50.0);
    quiet_spin.add_css_class("ctl-spin");
    quiet_spin.set_valign(gtk::Align::Center);
    quiet_spin.update_property(&[gtk::accessible::Property::Label(tr!("Hold minste fart til denne temperaturen"))]);
    oc::temp_spin(&quiet_spin);
    let qu = label(F::temp_label(), &["muted", "sm", "quiet-unit"]);
    quiet_row.append(&quiet_sw);
    quiet_row.append(&ql);
    quiet_row.append(&quiet_spin);
    quiet_row.append(&qu);
    crate::widgets::switchrow::make_row_toggle(&ql, &quiet_sw);
    let (quiet_cell, quiet_val, quiet_note) = tune_cell(tr!("Stille flate"), &quiet_row);
    quiet_val.set_visible(false);
    // Viftestopp: kort og ærlig, ved siden av den stille flaten.
    let stop = vbox(4, &["tune", "stop-note"]);
    let st_top = hbox(6, &[]);
    let si = Icon::new("circle-slash", 14);
    si.set_valign(gtk::Align::Center);
    st_top.append(&si);
    st_top.append(&label(tr!("Viftestopp"), &["sm", "bold"]));
    stop.append(&st_top);
    let stop_note = label("", &["sm", "muted"]);
    stop_note.set_wrap(true);
    stop_note.set_xalign(0.0);
    stop.append(&stop_note);
    for c in [&hyst_cell, &smooth_cell, &up_cell, &down_cell, &quiet_cell, &stop] {
        tune_grid.append(c);
    }
    tune_p.body.append(&tune_grid);
    let tune = Tuning { hyst, hyst_val, hyst_note, up, up_val, up_note, down, down_val, down_note, smooth, smooth_note, quiet_sw, quiet_spin, quiet_unit: qu, quiet_note, stop_note };

    // ---- Forhåndsvisning: siste 5 minutter med kurven ----
    let prev_p = Panel::new(tr!("Siste 5 minutter med kurven"));
    prev_p.root.add_css_class("preview-card");
    let preview_title = prev_p.title.clone();
    let plg = gtk::FlowBox::new();
    plg.add_css_class("legend");
    plg.add_css_class("preview-legend");
    plg.set_selection_mode(gtk::SelectionMode::None);
    plg.set_column_spacing(16);
    plg.set_min_children_per_line(1);
    plg.set_max_children_per_line(4);
    plg.set_halign(gtk::Align::Start);
    plg.set_focusable(false);
    let mut lg = Vec::new();
    for (cls, txt) in [("lg-curve", tr!("Med kurven")), ("lg-curve2", tr!("Vifte 2 med kurven")), ("lg-actual", tr!("Slik viftene gikk")), ("lg-temp", tr!("Temperatur"))] {
        let b = hbox(5, &[]);
        let sw = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sw.add_css_class("lg");
        sw.add_css_class(cls);
        sw.set_valign(gtk::Align::Center);
        b.append(&sw);
        let l = label(txt, &["muted", "sm"]);
        b.append(&l);
        let ch = gtk::FlowBoxChild::new();
        ch.set_child(Some(&b));
        ch.set_focusable(false);
        ch.set_can_target(false);
        plg.insert(&ch, -1);
        lg.push((l, ch));
    }
    lg[1].1.set_visible(false);
    let preview_lg = (lg[0].0.clone(), lg[1].1.clone());
    let preview = Preview::new(app);
    let preview_note = label("", &["muted", "sm", "preview-note"]);
    preview_note.set_wrap(true);
    preview_note.set_xalign(0.0);
    prev_p.body.append(&plg);
    prev_p.body.append(&preview);
    prev_p.body.append(&preview_note);

    // ---- Sidekolonnen ----
    let side = vbox(8, &[]);
    let mode_p = Panel::new(tr!("Viftestyring"));
    let view_seg = Seg::new(&[("simple", tr!("Enkel")), ("advanced", tr!("Avansert"))], if locked::fan_advanced(app) { "advanced" } else { "simple" }, tr!("Visning av viftestyring")).small();
    mode_p.aside.append(&view_seg.root);
    mode_p.root.add_css_class("fan-mode-card");
    let modes = Cols::new(&[1.0, 1.0, 1.0], 8, 620);
    modes.add_css_class("mode-list");
    let mut mode_btns = Vec::new();
    let mut mode_titles = Vec::new();
    let mut first: Option<gtk::ToggleButton> = None;
    for (m, title, desc) in mode_options() {
        let b = gtk::ToggleButton::new();
        b.add_css_class("mode-opt");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let bx = vbox(2, &[]);
        let top = hbox(6, &[]);
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("mode-dot");
        dot.set_valign(gtk::Align::Center);
        top.append(&dot);
        let t = label(title, &["mode-title"]);
        t.set_hexpand(true);
        t.set_wrap(true);
        mode_titles.push((m, t.clone()));
        top.append(&t);
        let now_tag = tag(tr!("kortet nå"), None, "quiet");
        now_tag.set_visible(false);
        top.append(&now_tag);
        bx.append(&top);
        let d = label(desc, &["muted", "sm", "mode-desc"]);
        d.set_wrap(true);
        d.set_xalign(0.0);
        bx.append(&d);
        b.set_child(Some(&bx));
        b.update_property(&[gtk::accessible::Property::Label(title), gtk::accessible::Property::Description(desc)]);
        modes.append(&b);
        mode_btns.push((m, b, now_tag));
    }
    mode_p.body.append(&modes);
    let readings = Cols::new(&[1.0, 1.0, 1.0], 12, 540);
    readings.add_css_class("fan-readings");
    let (now_root, _, now, now_note) = fan_readout(tr!("Avlest nå"));
    set_text(&now_note, tr!("Rapportert av NVIDIA-driveren"));
    let (target_root, target_title, target, target_note) = fan_readout(tr!("Valgt mål"));
    let (max_root, _, max, max_note) = fan_readout(tr!("Høyest i kurven"));
    for cell in [&now_root, &target_root, &max_root] { readings.append(cell); }
    let fan_readouts = FanReadouts { now, target_title, target, target_note, max_root, max, max_note };
    mode_p.body.append(&readings);
    let manual_box = hbox(8, &["manual-row"]);
    let manual = oc::Slider::new(app);
    manual.update_property(&[gtk::accessible::Property::Label(tr!("Fast viftefart"))]);
    manual.set_steps(1.0, 5.0);
    manual.set_fmt(|v| F::pct(Some(v), 0));
    let manual_spin = gtk::SpinButton::with_range(35.0, 100.0, 1.0);
    manual_spin.set_digits(0);
    manual_spin.set_width_chars(4);
    manual_spin.add_css_class("ctl-spin");
    manual_spin.update_property(&[gtk::accessible::Property::Label(tr!("Fast viftefart i prosent"))]);
    manual_box.append(&manual);
    let mr = hbox(4, &[]);
    mr.set_valign(gtk::Align::Start);
    mr.append(&manual_spin);
    let pct = label("%", &["muted", "sm"]);
    pct.set_valign(gtk::Align::Center);
    mr.append(&pct);
    manual_box.append(&mr);
    manual_box.set_visible(false);
    mode_p.body.append(&manual_box);
    let simple_curve = vbox(6, &["fan-curve-notice"]);
    let simple_curve_text = label("", &["sm"]);
    simple_curve_text.set_wrap(true);
    simple_curve.append(&simple_curve_text);
    let open_advanced = button(Some("sliders-horizontal"), Some(tr!("Åpne avansert")), &["sm"]);
    open_advanced.set_halign(gtk::Align::Start);
    simple_curve.append(&open_advanced);
    mode_p.body.append(&simple_curve);
    let apply_note = label("", &["sm", "muted", "fan-apply-note"]);
    apply_note.set_wrap(true);
    apply_note.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    mode_p.body.append(&apply_note);
    root.append(&mode_p.root);

    let changes = Panel::new(tr!("Endringer som brukes"));
    let simple_changes_text = label("", &["sm"]);
    simple_changes_text.set_wrap(true);
    simple_changes_text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    simple_changes_text.set_xalign(0.0);
    changes.body.append(&simple_changes_text);
    let simple_changes_note = label("", &["muted", "sm"]);
    simple_changes_note.set_wrap(true);
    simple_changes_note.set_xalign(0.0);
    changes.body.append(&simple_changes_note);
    let change_links = hbox(8, &[]);
    let open_controls = button(Some("arrow-right"), Some(tr!("Åpne Klokker og effekt")), &["sm", "flat"]);
    let changes_advanced = button(Some("sliders-horizontal"), Some(tr!("Åpne Avansert")), &["sm", "flat"]);
    change_links.append(&open_controls);
    change_links.append(&changes_advanced);
    changes.body.append(&change_links);
    changes.root.set_visible(false);
    root.append(&changes.root);

    let pts_p = Panel::new(tr!("Punktene"));
    let add_pt = button(Some("plus"), Some(tr!("Legg til")), &["sm", "flat"]);
    tip(&add_pt, tr!("Nytt punkt etter det valgte (+)"), None);
    pts_p.aside.append(&add_pt);
    let pts_box = vbox(0, &["pts"]);
    pts_p.body.append(&pts_box);
    side.append(&pts_p.root);

    let rules_p = Panel::new(tr!("Slik følges kurven"));
    let mut rule_emerg: Option<gtk::Label> = None;
    let rb = vbox(8, &["rules"]);
    let rules = label("", &["sm"]);
    for (icon, head, text) in [
        ("triangle-alert", tr!("Nødregelen"), trf!("Fra {terskel} går viftene straks til {fart}, uten rampe, og tilbake til kurven under {slipp}. Den kan ikke slås av.", terskel = F::temp(Some(txn::EMERGENCY_FAN_C), 0), fart = F::pct(Some(100.0), 0), slipp = F::temp(Some(txn::EMERGENCY_RELEASE_C), 0))),
        ("activity", tr!("Rekkefølgen"), tr!("Utjevning, hysterese, kurven med stille flate og minste fart, og til slutt rampen. Kan temperaturen ikke leses på 5 s, går viftene til 100 %.").replace(" %", "\u{202F}%")),
        ("chart-line", tr!("Mellom punktene"), tr!("Rett linje mellom punktene, flat før det første og etter det siste. Kurven faller aldri.").to_string()),
    ] {
        let r = hbox(8, &[]);
        let i = Icon::new(icon, 14);
        i.set_valign(gtk::Align::Start);
        r.append(&i);
        let c = vbox(1, &[]);
        c.set_hexpand(true);
        c.append(&label(head, &["sm", "bold"]));
        let l = label(&text, &["sm", "muted"]);
        l.set_wrap(true);
        c.append(&l);
        r.append(&c);
        rb.append(&r);
        if icon == "triangle-alert" {
            rule_emerg = Some(l);
        }
    }
    let r = hbox(8, &[]);
    let i = Icon::new("timer", 14);
    i.set_valign(gtk::Align::Start);
    r.append(&i);
    let c = vbox(1, &[]);
    c.set_hexpand(true);
    c.append(&label(tr!("Når endring er slått på"), &["sm", "bold"]));
    rules.add_css_class("muted");
    rules.set_wrap(true);
    c.append(&rules);
    r.append(&c);
    rb.append(&r);
    rules_p.body.append(&rb);
    side.append(&rules_p.root);

    let day_p = Panel::new(tr!("Siste døgn"));
    let dk = KvList::new();
    day_p.body.append(&dk.grid);
    let day_kv: Vec<gtk::Label> = [tr!("På minste fart"), tr!("Høyeste fart"), tr!("Varmeste"), tr!("Data for")].iter().map(|k| dk.add(k, "–")).collect();
    let day_line = label("", &["muted", "sm"]);
    day_line.set_wrap(true);
    day_line.add_css_class("group-foot");
    day_p.root.append(&day_line);
    side.append(&day_p.root);

    let left = vbox(8, &[]);
    left.append(&main.root);
    left.append(&tune_p.root);
    left.append(&prev_p.root);
    let cols = Columns::new(&left, &side, 340, 1040);
    cols.set_natural(true);
    // Under 720 px på kurvekortet: «Tilbakestill kurven» bare som ikon, så
    // navnet på kurven får plass i hodet.
    cols.set_row_breakpoint(720);
    root.append(&cols);
    let narrow = Rc::new(Cell::new(false));

    // ---- Over tid ----
    let charts_row = Cols::new(&[1.0, 1.0], 8, 700);
    charts_row.set_fill(170);
    let charts = ChartSet::new(app);
    let build_charts = {
        let (app, set, row) = (app.clone(), charts.clone(), charts_row.clone());
        move || {
            row.clear();
            set.clear();
            let t = chart::def_panel(&app, "gpuTemp", DefOpts { window: 300, sync: None, title: Some(tr!("Temperatur").into()), stats: false, now: false });
            row.append(&t.root);
            set.add(&t);
            let f = fans_panel(&app);
            row.append(&f.root);
            set.add(&f);
        }
    };
    build_charts();
    charts.connect_gpu(build_charts);
    root.append(&charts_row);

    let actions = hbox(6, &["styring-actions"]);
    actions.set_valign(gtk::Align::Center);
    let dirty = label("", &["muted", "sm", "dirty-text"]);
    let discard = button(None, Some(tr!("Forkast utkast")), &["sm", "flat"]);
    tip(&discard, tr!("Tilbake til det kortet har. Utkastet slettes (Ctrl+Z angrer)."), None);
    let apply = button(None, Some(tr!("Bruk")), &["sm", "suggested"]);
    actions.append(&dirty);
    actions.append(&discard);
    actions.append(&apply);
    locked::mark_topbar(app, "viftekurve");

    let page = Rc::new(Viftekurve {
        app: app.clone(),
        outer,
        root,
        view_seg,
        advanced_area: cols.clone(),
        mode_titles,
        simple_curve,
        simple_curve_text,
        open_advanced,
        apply_note,
        simple_changes: changes.root,
        simple_changes_text,
        simple_changes_note,
        open_controls,
        changes_advanced,
        fan_readouts,
        actions,
        strip,
        stats,
        editor: edit,
        window: Cell::new(172_800),
        win_seg,
        count,
        head_note,
        name,
        reset_curve,
        use_curve,
        mode_btns,
        manual_box,
        manual,
        manual_spin,
        pts_box,
        add_pt,
        rules,
        day_kv,
        day_line,
        dirty,
        discard,
        apply,
        confirm,
        charts,
        visible: Cell::new(false),
        narrow: narrow.clone(),
        timer: RefCell::new(None),
        quiet: Cell::new(false),
        target: Cell::new(0),
        target_seg: RefCell::new(target_seg),
        target_host,
        target_count: Cell::new(target_count),
        lg_down,
        lg_ghost,
        tune,
        preview,
        preview_note,
        preview_title,
        preview_lg,
        preview_hist: RefCell::new(None),
        preview_key: RefCell::new(None),
        preview_timer: RefCell::new(None),
        rule_emerg: rule_emerg.unwrap_or_else(|| label("", &[])),
    });
    let w = Rc::downgrade(&page);
    cols.connect_narrow(move |stacked, compact| {
        if let Some(p) = w.upgrade() {
            if let Some(t) = p.reset_curve.child().and_then(|b| b.last_child()).and_downcast::<gtk::Label>() {
                t.set_visible(!compact);
            }
            p.narrow.set(stacked);
            p.refresh();
        }
    });
    page.wire(preset_btns);
    page.refresh();
    Box::new(Wrap(page))
}

/// Tilgjengelige målekanaler. Historikken har foreløpig fan0/fan1, ikke
/// nødvendigvis én kanal for hver fysisk vifte kortet rapporterer.
fn fans_panel(app: &Rc<App>) -> Rc<ChartPanel> {
    let gk = |f: &str| app.gpu_key(f).unwrap_or_else(|| format!("gpu:-/{f}"));
    let n = actual_fan_count(app);
    let series = if n.is_some_and(|n| n >= 2) {
        vec![SeriesDef::new(&gk("fan0"), tr!("Vifte 1"), "--c-clock"), SeriesDef::new(&gk("fan1"), tr!("Vifte 2"), "--c-memctl")]
    } else {
        vec![SeriesDef::new(&gk("fan"), tr!("Avlest viftefart"), "--c-clock").fill()]
    };
    let mut opts = ChartOpts::new(app, || (0.0, 100.0));
    opts.unit = Rc::new(|| "%".into());
    opts.auto = Rc::new(|| false);
    let wa = Rc::downgrade(app);
    opts.stopped = Rc::new(move || wa.upgrade().and_then(|a| chart::gpu_stopped(&a)));
    let panel = ChartPanel::new(app, PanelOpts { title: tr!("Avleste viftekanaler").into(), series, window: 300, chart: opts, sync: None, stats: false, now: false, extra: None, flat: false });
    panel.root.set_tooltip_text(Some(tr!("Historikken viser tilgjengelige målekanaler. Backendens fan0/fan1 dekker ikke nødvendigvis alle fysiske vifter på kortet.")));
    panel
}

impl Viftekurve {
    fn base(&self) -> Values {
        locked::control_values(&self.app).unwrap_or_else(|| card_values(&self.app))
    }

    fn draft(&self) -> Option<Values> {
        locked::validated_draft(&self.app, &self.base()).unwrap_or_else(|_| locked::stored(&self.app).utkast)
    }

    fn edit(&self, f: impl FnOnce(&mut Values)) {
        if self.quiet.get() {
            return;
        }
        locked::edit_draft(&self.app, self.base(), f);
    }

    fn set_curve(&self, c: Vec<(f64, f64)>) {
        // Samme regler som tjenesten: stigende temperatur, 35–100 %, aldri fallende.
        let v = json!(c.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>());
        if let Ok((pts, _)) = txn::check_curve(&v) {
            let t = self.target.get();
            self.edit(|d| set_edited_curve(d, target_of(d, t), pts));
        }
    }

    /// Det som vises: utkastet, ellers det kortet har.
    fn shown(&self) -> Values {
        self.draft().unwrap_or_else(|| self.base())
    }

    /// Kurven som redigeres (felles, eller viften som er valgt).
    fn curve(&self) -> Vec<(f64, f64)> {
        let v = self.shown();
        edited_curve(&v, target_of(&v, self.target.get()))
    }

    /// Ukjent antall blir aldri en oppfunnet vifte i grensesnittet.
    fn fan_count(&self) -> usize {
        actual_fan_count(&self.app).unwrap_or(0)
    }

    fn connect_target_selector(self: &Rc<Self>, seg: &Seg) {
        let w = Rc::downgrade(self);
        seg.connect(move |value| {
            let Some(p) = w.upgrade() else { return };
            if p.quiet.get() { return; }
            let t: usize = value.parse().unwrap_or(0);
            let n = p.fan_count();
            if n == 0 || n > vifte::MAKS_VIFTER || t > n { return; }
            p.target.set(t);
            p.editor.select(None);
            if t == 0 { p.edit(|d| d.fan_per_fan = false); }
            else { p.edit(|d| split_curves(d, n)); }
            p.refresh();
        });
    }

    fn sync_target_selector(self: &Rc<Self>, count: Option<usize>, selected: usize) {
        let quiet = self.quiet.replace(true);
        if self.target_count.get() != count {
            let seg = make_target_selector(count);
            self.connect_target_selector(&seg);
            self.target_host.set_child(Some(&seg.root));
            drop(self.target_seg.replace(seg));
            self.target_count.set(count);
        }
        self.target_seg.borrow().set(&selected.to_string());
        self.target_host.set_sensitive(count.is_some_and(|n| n > 0 && n <= vifte::MAKS_VIFTER));
        self.target_host.set_visible(count.is_some_and(|n| n > 1));
        self.quiet.set(quiet);
    }

    fn wire(self: &Rc<Self>, presets: Vec<(&'static str, gtk::Button)>) {
        let a = self.app.clone();
        self.open_controls.connect_clicked(move |_| a.go("kontroller"));
        let a = self.app.clone();
        self.changes_advanced.connect_clicked(move |_| locked::set_fan_advanced(&a, true));
        let w = Rc::downgrade(self);
        self.view_seg.connect(move |value| {
            if let Some(p) = w.upgrade() { locked::set_fan_advanced(&p.app, value == "advanced"); }
        });
        let w = Rc::downgrade(self);
        self.open_advanced.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                locked::set_fan_advanced(&p.app, true);
                if let Some(b) = p.view_seg.button("advanced") { b.grab_focus(); }
            }
        });
        let w = Rc::downgrade(self);
        self.win_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                p.window.set(v.parse().unwrap_or(172_800));
                p.load();
            }
        });
        for (id, b) in presets {
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                let Some(p) = w.upgrade() else { return };
                if let Some(x) = PRESETS.iter().find(|x| x.id == id) {
                    p.target.set(0);
                    p.edit(|d| x.apply(d));
                }
            });
        }
        let w = Rc::downgrade(self);
        self.reset_curve.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                let st = stock(&read(&p.app));
                p.target.set(0);
                p.edit(|d| reset_fan_shape(d, &st));
            }
        });
        // Felles kurve eller én per vifte.
        self.connect_target_selector(&self.target_seg.borrow());
        // Oppførselen.
        let t = &self.tune;
        let w = Rc::downgrade(self);
        t.hyst.connect_change(move |v| {
            if let Some(p) = w.upgrade() {
                p.edit(|d| d.fan_hysteresis_c = v.round());
            }
        });
        let w = Rc::downgrade(self);
        t.up.connect_change(move |v| {
            if let Some(p) = w.upgrade() {
                p.edit(|d| d.fan_ramp_up_pct_s = v.round());
            }
        });
        let w = Rc::downgrade(self);
        t.down.connect_change(move |v| {
            if let Some(p) = w.upgrade() {
                p.edit(|d| d.fan_ramp_down_pct_s = v.round());
            }
        });
        let w = Rc::downgrade(self);
        t.smooth.connect(move |v| {
            if let Some(p) = w.upgrade() {
                let s: f64 = v.parse().unwrap_or(0.0);
                p.edit(|d| d.fan_smoothing_s = s);
            }
        });
        let w = Rc::downgrade(self);
        t.quiet_sw.connect_active_notify(move |sw| {
            if let Some(p) = w.upgrade() {
                let on = sw.is_active();
                let at = p.tune.quiet_spin.value().round();
                p.edit(|d| d.fan_quiet_until_c = on.then_some(at));
            }
        });
        let w = Rc::downgrade(self);
        t.quiet_spin.connect_value_changed(move |sp| {
            if let Some(p) = w.upgrade() {
                let at = sp.value().round();
                p.edit(|d| {
                    if d.fan_quiet_until_c.is_some() {
                        d.fan_quiet_until_c = Some(at);
                    }
                });
            }
        });
        let w = Rc::downgrade(self);
        self.editor.connect_curve(move |c| {
            if let Some(p) = w.upgrade() {
                p.set_curve(c);
            }
        });
        let w = Rc::downgrade(self);
        self.editor.connect_select(move |_| {
            if let Some(p) = w.upgrade() {
                p.mark_selected();
            }
        });
        let w = Rc::downgrade(self);
        self.use_curve.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.edit(|d| d.fan_mode = FanMode::Curve);
            }
        });
        let w = Rc::downgrade(self);
        self.add_pt.connect_clicked(move |_| {
            let Some(p) = w.upgrade() else { return };
            let c = p.curve();
            let floor = read(&p.app).floor();
            let at = p.editor.selected().unwrap_or(c.len().saturating_sub(2));
            if let Some((next, _)) = add_after(&c, at, floor).or_else(|| (0..c.len()).find_map(|i| add_after(&c, i, floor))) {
                p.set_curve(next);
            }
        });
        for (m, b, _) in &self.mode_btns {
            let m = *m;
            let w = Rc::downgrade(self);
            b.connect_toggled(move |b| {
                if b.is_active()
                    && let Some(p) = w.upgrade()
                {
                    p.edit(|d| d.fan_mode = m);
                }
            });
        }
        let w = Rc::downgrade(self);
        self.manual.connect_change(move |v| {
            if let Some(p) = w.upgrade() {
                p.edit(|d| d.fan_percent = v.round());
            }
        });
        let w = Rc::downgrade(self);
        self.manual_spin.connect_value_changed(move |s| {
            if let Some(p) = w.upgrade() {
                let v = s.value().round();
                p.edit(|d| d.fan_percent = v);
            }
        });
        let w = Rc::downgrade(self);
        self.discard.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                locked::update(&p.app, "utkast", |s| s.utkast = None);
                let a = p.app.clone();
                p.app.toast(crate::toast::Toast::new(tr!("Utkastet er forkastet")).action(tr!("Angre"), move || {
                    locked::undo_draft(&a, false);
                }));
            }
        });
        let w = Rc::downgrade(self);
        self.apply.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.do_apply();
            }
        });
        let w = Rc::downgrade(self);
        locked::on_store(move |what| {
            if let Some(p) = w.upgrade()
                && matches!(what, "utkast" | "fan-view")
            {
                p.refresh();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_telemetry.connect(move |_| {
            if let Some(p) = w.upgrade()
                && p.visible.get()
            {
                locked::refresh_control(&p.app, false);
                p.refresh();
            }
        });
        for hub in [&self.app.on_devices, &self.app.on_theme, &self.app.on_service] {
            let w = Rc::downgrade(self);
            hub.connect(move |_| {
                if let Some(p) = w.upgrade() {
                    p.refresh();
                }
            });
        }
        let w = Rc::downgrade(self);
        self.app.on_conn.connect(move |c| {
            if let Some(p) = w.upgrade()
                && *c == crate::state::Conn::Connected
                && p.visible.get()
            {
                p.load();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_settings.connect(move |k| {
            if let Some(p) = w.upgrade()
                && k.iter().any(|k| matches!(k.as_str(), "unit" | "confirmSec" | "surprises"))
            {
                // Tegnet tekst (aksene, merkene på gliderne) følger enheten.
                p.editor.queue_draw();
                p.preview.queue_draw();
                for sl in [&p.tune.hyst, &p.tune.up, &p.tune.down] {
                    sl.queue_draw();
                }
                p.refresh();
            }
        });
        let w = Rc::downgrade(self);
        *self.confirm.on_done.borrow_mut() = Some(Box::new(move |how| {
            if let Some(p) = w.upgrade() {
                if how == "confirm" {
                    locked::update(&p.app, "utkast", |s| s.utkast = None);
                }
                p.refresh();
            }
        }));
    }

    /// De siste fem minuttene (temperatur og viftene) til forhåndsvisningen.
    fn load_preview(self: &Rc<Self>) {
        let (Some(kt), Some(k0)) = (self.app.gpu_key("temp"), self.app.gpu_key("fan")) else { return };
        let k1 = self.app.gpu_key("fan0").unwrap_or_else(|| k0.clone());
        let k2 = self.app.gpu_key("fan1").unwrap_or_else(|| k0.clone());
        let w = Rc::downgrade(self);
        let keys = vec![kt.clone(), k0.clone(), k1.clone(), k2.clone()];
        self.app.client.request("get_history", json!({"window": 300, "keys": keys, "points": 300}), move |res| {
            let Some(p) = w.upgrade() else { return };
            let hist = res.ok().map(|v| {
                let col = |k: &str| -> Vec<Option<f64>> { serde_json::from_value(v["series"][k]["avg"].clone()).unwrap_or_default() };
                let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
                let temp = col(&kt);
                // Den høyeste av viftene (som `fan`), eller `fan` når de enkelte mangler.
                let (f0, f1, fmax) = (col(&k1), col(&k2), col(&k0));
                let fans: Vec<Option<f64>> = (0..t.len())
                    .map(|i| {
                        let a = f0.get(i).copied().flatten();
                        let b = f1.get(i).copied().flatten();
                        [a, b, fmax.get(i).copied().flatten()].into_iter().flatten().reduce(f64::max)
                    })
                    .collect();
                (t, temp, fans)
            });
            *p.preview_hist.borrow_mut() = hist;
            p.preview_key.borrow_mut().take();
            p.refresh_preview();
        });
    }

    /// Regner ut forhåndsvisningen på nytt når utkastet eller historikken er ny.
    fn refresh_preview(&self) {
        let v = self.shown();
        let floor = read(&self.app).floor();
        let n = self.fan_count();
        let key = (v.clone(), actual_fan_count(&self.app), floor);
        if self.preview_key.borrow().as_ref() == Some(&key) {
            return;
        }
        *self.preview_key.borrow_mut() = Some(key);
        let editing = locked::stored(&self.app).utkast.is_some();
        set_text(&self.preview_title, if editing { tr!("Siste 5 minutter med utkastet") } else { tr!("Siste 5 minutter med kurven") });
        if n == 0 || n > vifte::MAKS_VIFTER {
            self.preview_lg.1.set_visible(false);
            self.preview.set_data(PreviewData::default(), floor, tr!("Forhåndsvisning venter på støttet vifteantall."));
            set_text(&self.preview_note, &fan_control_unavailable(actual_fan_count(&self.app), FanMode::Curve).unwrap_or_default());
            return;
        }
        let hist = self.preview_hist.borrow();
        let Some((t, temp, actual)) = hist.as_ref().filter(|h| h.1.iter().any(Option::is_some)) else {
            let empty = if self.app.conn.get() != crate::state::Conn::Connected { tr!("Tjenesten svarer ikke.") } else { tr!("Ingen målinger de siste 5 minuttene ennå.") };
            self.preview.set_data(PreviewData::default(), floor, empty);
            set_text(&self.preview_note, tr!("Når tjenesten har målt en stund, vises hvordan viftene ville gått med kurven, ved siden av hvordan de faktisk gikk."));
            return;
        };
        let start = actual.iter().flatten().next().copied();
        let sim = simulate(&v, n, floor, t, temp, start);
        // Egne kurver: den første viften heltrukket, den andre stiplet.
        let per_fan = sim.len() > 1;
        set_text(&self.preview_lg.0, if per_fan { tr!("Vifte 1 med kurven") } else { tr!("Med kurven") });
        self.preview_lg.1.set_visible(per_fan);
        if let Some(label) = self.preview_lg.1.child().and_then(|w| w.last_child()).and_downcast::<gtk::Label>() {
            set_text(&label, if n > 2 { tr!("Øvrige vifter med kurven") } else { tr!("Vifte 2 med kurven") });
        }
        let mine = sim.iter().map(|r| fan_summary(r)).collect::<Vec<_>>();
        let real: Vec<f64> = actual.iter().flatten().copied().collect();
        let theirs = fan_summary(&real);
        let fmt = |x: Option<(f64, f64, usize)>| x.map(|(a, m, c)| trf!("snitt {snitt}, høyest {maks}, {endring}", snitt = F::pct(Some(a.round()), 0), maks = F::pct(Some(m.round()), 0), endring = trn!(c, "{antall} endring", "{antall} endringer", antall = c)));
        let mine_txt = if mine.len() > 1 {
            mine.iter().enumerate().filter_map(|(i, m)| fmt(*m).map(|t| trf!("vifte {nummer} {t}", nummer = i + 1, t = t))).collect::<Vec<_>>().join("; ")
        } else {
            mine.first().copied().flatten().map(|m| fmt(Some(m)).unwrap_or_default()).unwrap_or_default()
        };
        let mode = if v.fan_mode == FanMode::Curve { "" } else { tr!(" Kurven brukes bare i modus Kurve.") };
        let note = match fmt(theirs) {
            Some(th) => trf!("Med kurven: {mine_txt}. Slik viftene gikk: {th}.{mode}", mine_txt = mine_txt, th = th, mode = mode),
            None => trf!("Med kurven: {mine_txt}.{mode}", mine_txt = mine_txt, mode = mode),
        };
        set_text(&self.preview_note, &note);
        self.preview.set_data(PreviewData { t: t.clone(), temp: temp.clone(), actual: actual.clone(), sim }, floor, "");
    }

    /// Henter punktskyen og døgnet fra tjenestens historikk.
    fn load(self: &Rc<Self>) {
        self.load_preview();
        let (Some(kt), Some(kf)) = (self.app.gpu_key("temp"), self.app.gpu_key("fan")) else { return };
        let win = self.window.get();
        let w = Rc::downgrade(self);
        let (kt2, kf2) = (kt.clone(), kf.clone());
        self.app.client.request("get_history", json!({"window": win, "keys": [kt, kf], "points": 2000}), move |res| {
            let Some(p) = w.upgrade() else { return };
            let (pts, span) = match res {
                Ok(v) => (pairs(&v, &kt2, &kf2), span_of(&v, &kt2, &kf2)),
                Err(_) => (vec![], None),
            };
            let empty = if p.app.conn.get() != crate::state::Conn::Connected { tr!("Tjenesten svarer ikke.") } else { tr!("Ingen målinger i tidsvinduet ennå.") };
            // Dekningen som tid, ikke antall punkter (de er nedsamplet).
            let count = match span {
                Some((a, b)) if (b - a) as f64 >= win as f64 * 1000.0 * 0.98 => trf!("Data for hele {varighet}", varighet = span_text(win * 1000)),
                Some((a, b)) => trf!("Data for {malt} av {varighet}", malt = span_text(b - a), varighet = span_text(win * 1000)),
                None => tr!("Ingen data").into(),
            };
            set_text(&p.count, &count);
            p.editor.set_points(pts, empty);
        });
        let w = Rc::downgrade(self);
        let (kt2, kf2) = (kt.clone(), kf.clone());
        self.app.client.request("get_history", json!({"window": 86400, "keys": [kt, kf], "points": 1440}), move |res| {
            let Some(p) = w.upgrade() else { return };
            let (pts, span) = res.map(|v| (pairs(&v, &kt2, &kf2), span_of(&v, &kt2, &kf2))).unwrap_or_default();
            p.render_day(&pts, span);
        });
    }

    fn render_day(&self, pts: &[(f64, f64)], span: Option<(i64, i64)>) {
        let min = read(&self.app).fan_min;
        let fans: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let share = min.and_then(|min| share_at_min(&fans, min));
        set_text(&self.day_kv[0], &share.map(|s| trf!("{andel} av tiden", andel = F::pct(Some(s * 100.0), 0))).unwrap_or_else(|| F::DASH.into()));
        set_text(&self.day_kv[1], &F::pct(fans.iter().cloned().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v)))), 0));
        set_text(&self.day_kv[2], &F::temp(pts.iter().map(|p| p.0).fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v)))), 0));
        let full_day = span.is_some_and(|(a, b)| b - a >= 86_400_000 - 120_000);
        set_text(&self.day_kv[3], &match span {
            Some(_) if full_day => trf!("hele døgnet ({varighet})", varighet = span_text(86_400_000)),
            Some((a, b)) => trf!("{malt} av {varighet}", malt = span_text(b - a), varighet = span_text(86_400_000)),
            None => F::DASH.into(),
        });
        // Under ett døgn med data: «siden 18:11 (3 min)», ikke «siste døgn».
        let when = match span {
            Some((a, b)) if !full_day => trf!("siden {tid} ({varighet})", tid = F::since(crate::app::local_time(a), crate::app::local_now()), varighet = span_text(b - a)),
            _ => tr!("siste døgn").into(),
        };
        set_text(
            &self.day_line,
            &match share {
                Some(s) => trf!("Viftene har gått på {fart} i {andel} av tiden {when}.", fart = F::pct(min, 0), andel = F::pct(Some((s * 100.0).round()), 0), when = when),
                None if min.is_none() => tr!("Minste viftefart er ikke avlest; andelen på minimum kan ikke beregnes.").into(),
                None => tr!("Ingen målinger siste døgn ennå. Tjenesten tar vare på 48 timer.").into(),
            },
        );
    }

    fn do_apply(self: &Rc<Self>) {
        let Some(_) = self.draft() else { return };
        if matches!(locked::gate(&self.app), Gate::Locked(_)) || self.confirm.active() {
            return;
        }
        if locked::control_ready(&self.app) && locked::control_values(&self.app).is_none() {
            locked::refresh_control(&self.app, true);
            self.app.toast(crate::toast::Toast::new(tr!("Leser kortets innstillinger …")).body(tr!("Prøv Bruk igjen når innstillingene er lest.")));
            return;
        }
        let base = self.base();
        let d = match locked::draft_for_apply(&self.app, &base) {
            Ok(v) => v,
            Err(why) => {
                self.app.toast(crate::toast::Toast::new(tr!("Utkastet må oppdateres")).body(rigg_core::hjelper::display_diagnostic(&why).as_ref()).error());
                return;
            }
        };
        if let Some(why) = fan_control_unavailable(actual_fan_count(&self.app), d.fan_mode) {
            self.app.toast(crate::toast::Toast::new(tr!("Viftestyring er utilgjengelig")).body(rigg_core::hjelper::display_diagnostic(&why).as_ref()).error());
            return;
        }
        if let Some(why) = locked::patch_error(&self.app, &base, &d) {
            self.app.toast(crate::toast::Toast::new(tr!("Funksjonen er utilgjengelig")).body(rigg_core::hjelper::display_diagnostic(&why).as_ref()).error());
            return;
        }
        let patch = locked::changed_patch(&base, &d);
        let secs = self.app.settings().confirm_sec;
        let w = Rc::downgrade(self);
        locked::send_write(&self.app, "apply", json!({"settings": patch, "confirmTimeoutS": secs}), move |res| {
            let Some(p) = w.upgrade() else { return };
            match res {
                Ok(v) if v["txn"].is_string() => p.confirm.start(&p.app, &v),
                Ok(_) => p.app.toast(crate::toast::Toast::new(tr!("Ingen endringer å bruke"))),
                Err(e) => p.app.toast(crate::toast::Toast::new(tr!("Kunne ikke bruke utkastet")).body(rigg_core::hjelper::display_error(&e).as_ref()).error()),
            }
            p.refresh();
        });
    }

    fn refresh(self: &Rc<Self>) {
        let r = read(&self.app);
        let floor = r.floor();
        let draft = self.draft();
        let editing = draft.is_some();
        let base = self.base();
        let shown = draft.clone().unwrap_or_else(|| base.clone());
        let actual_count = actual_fan_count(&self.app);
        let n_fans = actual_count.unwrap_or(0);
        let has_fans = n_fans > 0;
        let advanced = locked::fan_advanced(&self.app);
        let hidden = draft.as_ref().map(|d| simple_hidden_draft(&base, d)).unwrap_or_default();
        self.simple_changes.set_visible(!advanced && !hidden.lines.is_empty());
        set_text(&self.simple_changes_text, &hidden.lines.join("\n"));
        set_text(&self.simple_changes_note, if hidden.advanced_fans {
            tr!("Fra utgangspunkt til utkast. Bruk gjelder hele utkastet. Kurveinnstillinger påvirker bare modus Kurve; lagret fast fart brukes bare i Fast fart.")
        } else {
            tr!("Fra utgangspunkt til utkast. Bruk gjelder hele utkastet, også innstillinger fra Klokker og effekt.")
        });
        self.open_controls.set_visible(hidden.controls);
        self.changes_advanced.set_visible(hidden.advanced_fans);
        self.view_seg.set(if advanced { "advanced" } else { "simple" });
        self.advanced_area.set_visible(advanced);
        self.advanced_area.set_sensitive(has_fans && n_fans <= vifte::MAKS_VIFTER);
        class(&self.root, "fan-simple", !advanced);
        for (mode, title) in &self.mode_titles {
            let text = match mode {
                FanMode::Auto if !advanced => tr!("Auto"),
                FanMode::Manual if !advanced => tr!("Fast fart"),
                FanMode::Auto => tr!("Auto (driveren)"),
                FanMode::Manual => tr!("Manuell"),
                FanMode::Curve => tr!("Kurve"),
            };
            set_text(title, text);
        }
        let curve_notice = simple_curve_message(base.fan_mode, shown.fan_mode, editing);
        self.simple_curve.set_visible(!advanced && curve_notice.is_some());
        set_text(&self.simple_curve_text, curve_notice.unwrap_or(""));
        let apply_note = if self.confirm.active() {
            tr!("Prøven pågår. Velg «Behold» eller «Rull tilbake» øverst.").to_string()
        } else {
            fan_control_unavailable(actual_count, shown.fan_mode).unwrap_or_else(|| trf!(
                "Bruk prøver endringen i {tid}. Trykk Behold i linjen øverst for å fortsette. Uten Behold går viftene tilbake til forrige innstilling.",
                tid = format!("{}{}s", self.app.settings().confirm_sec, F::NNBSP)
            ))
        };
        set_text(&self.apply_note, &apply_note);
        self.strip.sync(&self.app, r.lost, editing);
        locked::sync_topbar(&self.app, "viftekurve");
        class(&self.root, "drafting", editing);

        // ---- Tallene ----
        let s = &self.stats;
        s[0].set(&F::temp_num(r.temp, 0), F::temp_label(), &r.temp_target.map(|g| trf!("mål {maal}", maal = F::temp(Some(g), 0))).unwrap_or_default());
        let scale = self.editor.update_scale(r.temp.into_iter().map(|t| (t, 0.0)));
        s[0].meter.set(r.temp.map(|t| scale.position(t, 0.0).0), Some(scale.position(txn::EMERGENCY_FAN_C, 0.0).0), if r.temp.is_some_and(|t| t >= txn::EMERGENCY_FAN_C) { 2 } else { 0 });
        s[0].set_tip(&trf!("GPU-temperaturen. Måleren går fra {min} til {maks}. Merket er nødregelen ({nodregel}).", min = F::temp(Some(scale.temp.0), 0), maks = F::temp(Some(scale.temp.1), 0), nodregel = F::temp(Some(txn::EMERGENCY_FAN_C), 0)));
        let range = measured_fan_range(&r.fans, actual_count);
        let speed = range.map(|(lo, hi, _)| if (hi - lo).abs() < 0.5 { F::num(Some(hi), 0) } else { format!("{}–{}", F::num(Some(lo), 0), F::num(Some(hi), 0)) });
        let measured_note = match (range, actual_count) {
            (Some((_, _, measured)), Some(n)) => trf!("{measured} av {n} vifter målt", measured = measured, n = n),
            (Some((_, _, measured)), None) => trf!("{measured} avleste kanaler · antall ukjent", measured = measured),
            (None, Some(0)) => tr!("ingen vifter rapportert").into(),
            _ => tr!("farten er ikke avlest").into(),
        };
        s[1].set(speed.as_deref().unwrap_or(F::DASH), "%", &measured_note);
        s[1].meter.set(range.map(|(_, hi, _)| hi / 100.0), r.fan_min.map(|min| min / 100.0), 0);
        s[1].set_tip(tr!("Laveste og høyeste faktisk avleste vifteprosent. Alle tilgjengelige enkeltmålinger står under Faktisk nå."));
        s[2].set(&F::num(actual_count.map(|n| n as f64), 0), "", if actual_count.is_none() { tr!("antallet er ikke avlest") } else { tr!("rapportert av driveren") });
        s[2].set_tip(tr!("Fysisk vifteantall rapportert av kortet. Ukjent er ikke det samme som null."));
        let tmax = has_fans.then(|| r.fan_target.iter().cloned().reduce(f64::max)).flatten();
        let tsub = match r.fan_target.len() {
            0 => if r.lost { String::new() } else { tr!("leses ikke ennå").into() },
            1 => tr!("det driveren vil ha").into(),
            // Like mål: ikke gjenta tallet («35 % · 35 %» under «35 %»).
            n if r.fan_target.iter().all(|t| (t - r.fan_target[0]).abs() < 0.5) => trf!("{n} avleste målkanaler", n = n),
            _ => r.fan_target.iter().enumerate().map(|(i, t)| trf!("vifte {nummer} {fart}", nummer = i + 1, fart = F::pct(Some(*t), 0))).collect::<Vec<_>>().join(" · "),
        };
        s[3].set(&F::num(tmax, 0), "%", if has_fans { &tsub } else { tr!("ikke avlest") });
        s[3].set_tip(tr!("Farten driveren vil ha for hver vifte (nvmlDeviceGetTargetFanSpeed)."));
        let card_mode = base.fan_mode;
        let mode_read = match &r.fan_policy {
            _ if !has_fans => None,
            None => None,
            Some(p) if p.iter().all(|x| *x == 0) => Some(FanMode::Auto),
            // Bare det simulerte kortet melder kurven tjenesten følger.
            Some(_) if card_mode == FanMode::Curve => Some(FanMode::Curve),
            Some(_) => Some(FanMode::Manual),
        };
        let mode_now = match mode_read {
            Some(FanMode::Manual) if !advanced => tr!("Fast fart"),
            Some(mode) => mode_name(mode),
            None => F::DASH,
        };
        s[4].set(mode_now, "", &if !has_fans { tr!("vifteantall mangler eller er null").into() } else if editing && shown.fan_mode != card_mode { trf!("utkast: {modus}", modus = mode_name(shown.fan_mode)) } else if mode_read == Some(FanMode::Auto) { tr!("driveren styrer").into() } else if mode_read == Some(FanMode::Curve) { tr!("tjenesten styrer").into() } else if card_mode == FanMode::Manual && (locked::control_values(&self.app).is_some() || locked::gate(&self.app) == Gate::Sim) { tr!("fast fart fra Agentic HW Monitor").into() } else if r.fan_policy.is_some() { tr!("satt utenfor appen").into() } else { String::new() });
        s[4].set_tip(tr!("Fra nvmlDeviceGetFanControlPolicy_v2. Policy 0 betyr at driveren styrer viftene."));
        // Det kurven ville gitt nå: stille flate, minste fart og per vifte (uten rampe og hysterese).
        let o = vifte::Oppsett::fra_values(&shown, n_fans, floor);
        let curve_count_known = has_fans && n_fans <= vifte::MAKS_VIFTER;
        let at = r.temp.filter(|_| curve_count_known).map(|t| (0..o.vifter).map(|i| o.maal(i, t)).fold(floor, f64::max).round());
        let readings = &self.fan_readouts;
        let measured: Vec<_> = (0..actual_count.unwrap_or(r.fans.len().max(r.rpm.len()))).filter_map(|i| {
            let pct = r.fans.get(i).copied().flatten();
            let rpm = r.rpm.get(i).copied().flatten();
            if actual_count.is_none() && pct.is_none() && rpm.is_none() { return None; }
            Some(trf!("Vifte {nummer}: {fart}{rpm}", nummer = i + 1, fart = F::pct(pct, 0), rpm = rpm.map(|v| format!(" · {}", F::rpm(Some(v)))).unwrap_or_default()))
        }).collect();
        let measured_text = if measured.is_empty() { if actual_count == Some(0) { tr!("Ingen vifter rapportert").into() } else { tr!("Ingen avleste viftekanaler").into() } } else { measured.join(" · ") };
        set_text(&readings.now, &measured_text);
        let fan_draft = editing && (shown.fan_mode != base.fan_mode || shown.fan_percent != base.fan_percent || !fan_shape_is(&shown, &base));
        let curve_preview = advanced && shown.fan_mode == FanMode::Auto;
        if shown.fan_mode == FanMode::Curve || curve_preview {
            set_text(&readings.target_title, &r.temp.map_or(tr!("Kurven ved nåværende temperatur").into(), |t| trf!("Kurven ved {temperatur}", temperatur = F::temp(Some(t), 0))));
            let goals = r.temp.filter(|_| curve_count_known).map(|t| (0..o.vifter).map(|i| if shown.fan_per_fan { trf!("Vifte {nummer}: {fart}", nummer = i + 1, fart = F::pct(Some(o.maal(i, t)), 0)) } else { F::pct(Some(o.maal(i, t)), 0) }).take(if shown.fan_per_fan { o.vifter } else { 1 }).collect::<Vec<_>>().join(" · "));
            set_text(&readings.target, goals.as_deref().unwrap_or(F::DASH));
            set_text(&readings.target_note, if curve_preview { if fan_draft { tr!("Kurveutkast · ikke i bruk") } else { tr!("Forhåndsvisning · kurven er ikke i bruk") } } else if fan_draft { tr!("Kurveutkast · ikke brukt") } else { tr!("Kurvemål før hysterese og rampe") });
        } else {
            set_text(&readings.target_title, if fan_draft { tr!("Valgt mål · utkast") } else { tr!("Valgt mål") });
            let target = if shown.fan_mode == FanMode::Manual { F::pct(Some(shown.fan_percent), 0) } else { tr!("Auto").into() };
            set_text(&readings.target, &target);
            set_text(&readings.target_note, if fan_draft { tr!("Ikke brukt · trykk Bruk og Behold") } else if shown.fan_mode == FanMode::Auto { tr!("Driveren bestemmer farten") } else { tr!("Fast fart på kortet") });
        }
        readings.max_root.set_visible(advanced);
        let maximum = curve_count_known.then(|| (0..o.vifter).flat_map(|i| o.kurve(i).iter().map(|p| p.1)).reduce(f64::max)).flatten();
        set_text(&readings.max, &F::pct(maximum, 0));
        set_text(&readings.max_note, if shown.fan_mode != FanMode::Curve { tr!("Kurven er ikke i bruk") } else if fan_draft { tr!("I kurveutkastet") } else { tr!("I den aktive kurven") });
        if !has_fans {
            let why = fan_control_unavailable(actual_count, shown.fan_mode).unwrap_or_default();
            set_text(&readings.target, F::DASH);
            set_text(&readings.target_note, &why);
            set_text(&readings.max_note, &why);
        }
        if let Some(cap) = s[5].root.first_child().and_downcast::<gtk::Label>() {
            set_text(&cap, if advanced { tr!("Kurven gir") } else { tr!("Manuelt minimum") });
        }
        if advanced {
            s[5].set(&F::num(at, 0), "%", &match (r.temp, shown.fan_mode == FanMode::Curve) {
                (Some(t), true) => trf!("ved {temperatur}", temperatur = F::temp(Some(t), 0)),
                (Some(t), false) => trf!("ved {temperatur} · ikke i bruk", temperatur = F::temp(Some(t), 0)),
                _ => String::new(),
            });
            s[5].meter.set(at.map(|a| a / 100.0), Some(floor / 100.0), 0);
            s[5].set_tip(tr!("Farten kurven ville gitt ved temperaturen nå (uten rampe og hysterese). Den brukes bare i modus Kurve."));
        } else {
            s[5].set(&F::num(r.fan_min.filter(|_| has_fans), 0), "%", if r.fan_min.is_some() && has_fans { tr!("rapportert av driveren") } else { tr!("ikke avlest") });
            s[5].meter.set(r.fan_min.filter(|_| has_fans).map(|min| min / 100.0), None, 0);
            s[5].set_tip(tr!("Minste viftefart kortet tillater. Auto kan styre annerledes etter kortets BIOS."));
        }

        // ---- Kurven ----
        // Felles kurve, eller den valgte viften når hver vifte har sin.
        let target = if has_fans { target_of(&shown, self.target.get().min(n_fans.min(vifte::MAKS_VIFTER))) } else { 0 };
        self.target.set(target);
        self.sync_target_selector(actual_count, target);
        let curve = edited_curve(&shown, target);
        let ghost = if target > 0 && n_fans > 1 { edited_curve(&shown, if target == 1 { 2 } else { 1 }) } else { vec![] };
        let ghost = if ghost == curve { vec![] } else { ghost };
        self.lg_ghost.set_visible(!ghost.is_empty());
        self.lg_down.set_visible(shown.fan_hysteresis_c > 0.0);
        self.editor.set_floor(floor, r.fan_min.is_some() && has_fans);
        self.editor.set_now(r.temp.zip(r.fan_max_now()));
        self.editor.set_shape(ghost, shown.fan_hysteresis_c, shown.fan_quiet_until_c);
        let st = stock(&r);
        // Heltrukket: et utkast, eller kurven kortet følger nå (sim).
        let card_curve = !editing && card_mode == FanMode::Curve;
        self.editor.set_curve(curve.clone(), card_curve || editing && (!fan_shape_is(&shown, &base) || shown.fan_mode == FanMode::Curve));
        let nick = if shown.fan_per_fan { None } else { crate::easter::curve_nickname(&self.app, &shown.fan_curve) };
        let (cname, tipx): (String, Option<&str>) = match (preset_name(&shown), nick) {
            (Some(_), _) if PRESETS[1].matches(&shown) => (tr!("Balansert · standard").into(), None),
            (Some(p), _) => (p, None),
            (None, Some((n, t))) => (n.into(), Some(t)),
            _ if shown.fan_per_fan => (tr!("Egne kurver per vifte").into(), None),
            _ => (tr!("Egendefinert").into(), None),
        };
        set_text(&self.name, &cname);
        // Navnet kan være kuttet i smale vinduer: hele står i verktøytipset.
        let tip_text = tipx.unwrap_or(&cname);
        if self.name.tooltip_text().as_deref() != Some(tip_text) {
            self.name.set_tooltip_text(Some(tip_text));
        }
        self.reset_curve.set_sensitive(!fan_shape_is(&shown, &st));
        let in_use = shown.fan_mode == FanMode::Curve;
        set_text(&self.head_note, &match (in_use, editing) {
            (true, false) => tr!("Viftene følger denne kurven nå.").to_string(),
            (true, _) => tr!("Kurven er valgt i utkastet. Viftene følger den når utkastet brukes.").to_string(),
            (false, true) => trf!("Kurven brukes bare i modus Kurve. Utkastet har {modus}.", modus = mode_name(shown.fan_mode)),
            (false, false) => tr!("Stiplet: standardkurven. Den brukes bare i modus Kurve; nå styrer driveren. Dra et punkt for å lage et utkast.").to_string(),
        });
        self.use_curve.set_visible(!in_use);
        self.use_curve.set_sensitive(curve_count_known && (!locked::control_ready(&self.app) || locked::supports(&self.app, "fan_curve")));
        self.render_points(&curve, floor);
        self.sync_tuning(&shown, floor);
        set_text(&self.rule_emerg, &trf!("Fra {terskel} går viftene straks til {fart}, uten rampe, og tilbake til kurven under {slipp}. Den kan ikke slås av.", terskel = F::temp(Some(txn::EMERGENCY_FAN_C), 0), fart = F::pct(Some(100.0), 0), slipp = F::temp(Some(txn::EMERGENCY_RELEASE_C), 0)));
        self.refresh_preview();

        // ---- Modus ----
        self.quiet.set(true);
        for (m, b, now_tag) in &self.mode_btns {
            b.set_visible(advanced || *m != FanMode::Curve);
            let key = if *m == FanMode::Curve { "fan_curve" } else { "fan_manual" };
            let unavailable = fan_control_unavailable(actual_count, *m);
            let available = unavailable.is_none() && (!locked::control_ready(&self.app) || locked::supports(&self.app, key));
            b.set_sensitive(available);
            let unavailable = unavailable.or_else(|| (!available).then(|| locked::capability_reason(&self.app, key)));
            b.set_tooltip_text(unavailable.as_deref().map(rigg_core::hjelper::display_message));
            let title = if *m == FanMode::Manual && !advanced { tr!("Fast fart") } else { mode_name(*m) };
            b.update_property(&[gtk::accessible::Property::Label(title)]);
            if (*m == shown.fan_mode) != b.is_active() && *m == shown.fan_mode {
                b.set_active(true);
            }
            now_tag.set_visible(has_fans && r.fan_policy.is_some() && *m == card_mode);
            class(b, "changed", editing && *m == shown.fan_mode && shown.fan_mode != card_mode);
        }
        self.manual_box.set_visible(shown.fan_mode == FanMode::Manual);
        self.manual_box.set_sensitive(has_fans && (!locked::control_ready(&self.app) || locked::supports(&self.app, "fan_manual")));
        self.manual.set_range(floor, 100.0, (floor, 100.0));
        self.manual.set_card(r.fan_max_now().filter(|_| card_mode == FanMode::Manual));
        self.manual.set_value(Some(shown.fan_percent));
        if self.manual_spin.range() != (floor, 100.0) {
            self.manual_spin.set_range(floor, 100.0);
        }
        if self.manual_spin.value() != shown.fan_percent && !self.manual_spin.has_focus() {
            self.manual_spin.set_value(shown.fan_percent);
        }
        self.quiet.set(false);
        set_text(&self.rules, &trf!("Hver endring må beholdes innen {tid}, ellers rulles den tilbake. Lukkes Agentic HW Monitor helt, går viftene tilbake til Auto.", tid = format!("{}{}s", self.app.settings().confirm_sec, F::NNBSP)));

        // ---- Utkastet i toppfeltet ----
        let Some(d) = draft else {
            for w in [self.dirty.clone().upcast::<gtk::Widget>(), self.discard.clone().upcast(), self.apply.clone().upcast()] {
                w.set_visible(false);
            }
            return;
        };
        let n = draft_diff(&base, &d).len();
        // Tell alle lagrede patchfelt; porten beholder eksisterende regler
        // for inaktive kurvefelt gjennom n/draft_diff nedenfor.
        let stored_changes = base.diff(&d).len();
        set_text(&self.dirty, &match stored_changes {
            _ if n == 0 && self.confirm.active() => tr!("Prøve pågår").to_string(),
            0 => tr!("Ingen endringer").to_string(),
            n => trn!(n, "{n} endring i utkastet", "{n} endringer i utkastet", n = F::num(Some(n as f64), 0)),
        });
        for w in [self.discard.clone().upcast::<gtk::Widget>(), self.apply.clone().upcast()] {
            w.set_visible(true);
        }
        self.dirty.set_visible(!self.narrow.get());
        match locked::gate(&self.app) {
            Gate::Sim | Gate::Real => {
                let ok = n > 0 && !self.confirm.active()
                    && fan_control_unavailable(actual_count, d.fan_mode).is_none()
                    && (!locked::control_ready(&self.app) || locked::control_values(&self.app).is_some())
                    && locked::validated_draft(&self.app, &self.base()).is_ok()
                    && locked::patch_error(&self.app, &self.base(), &d).is_none();
                self.apply.set_sensitive(ok);
                let reason = fan_control_unavailable(actual_count, d.fan_mode).or_else(|| locked::validated_draft(&self.app, &base).err()).or_else(|| locked::patch_error(&self.app, &base, &d));
                let reason = reason.as_deref().map(rigg_core::hjelper::display_diagnostic);
                tip(&self.apply, reason.as_deref().unwrap_or(if self.confirm.active() { tr!("Endringen venter på Behold") } else if ok { tr!("Bruk utkastet") } else { tr!("Ingen endringer å bruke") }), Some("Ctrl+Enter"));
            }
            Gate::Locked(t) => {
                self.apply.set_sensitive(false);
                self.apply.set_tooltip_text(Some(rigg_core::hjelper::display_message(t)));
            }
        }
    }

    /// Gliderne og valgene i «Oppførsel», og forklaringene i klartekst.
    fn sync_tuning(&self, v: &Values, floor: f64) {
        let t = &self.tune;
        self.quiet.set(true);
        let h = v.fan_hysteresis_c;
        t.hyst.set_value(Some(h));
        set_text(&t.hyst_val, &F::temp_delta(h, 0));
        set_text(&t.hyst_note, &if h > 0.0 {
            trf!("Viftene går ikke ned før temperaturen har falt {grader} under der de gikk opp. Opp går de straks.", grader = F::temp_delta(h, 0))
        } else {
            tr!("Av: viftene følger kurven begge veier, også når temperaturen svinger litt.").into()
        });
        let span = 100.0 - floor;
        t.up.set_value(Some(v.fan_ramp_up_pct_s));
        set_text(&t.up_val, &rate_text(v.fan_ramp_up_pct_s));
        set_text(&t.up_note, &trf!("Fra {fra} til {til} på {tid}. Høyere svarer raskere på last.", fra = F::pct(Some(floor), 0), til = F::pct(Some(100.0), 0), tid = secs_text(span / v.fan_ramp_up_pct_s.max(1.0))));
        t.down.set_value(Some(v.fan_ramp_down_pct_s));
        set_text(&t.down_val, &rate_text(v.fan_ramp_down_pct_s));
        set_text(&t.down_note, &trf!("Fra {fra} til {til} på {tid}. Lavere gir mykere lyd når lasten slipper.", fra = F::pct(Some(100.0), 0), til = F::pct(Some(floor), 0), tid = secs_text(span / v.fan_ramp_down_pct_s.max(1.0))));
        let sm = v.fan_smoothing_s.round() as i64;
        t.smooth.set(&sm.to_string());
        set_text(&t.smooth_note, &if sm > 0 {
            trf!("Kurven leses ved snittet av de siste {tid}, så korte topper ikke gir viftesus. Nødregelen ser alltid på temperaturen nå.", tid = format!("{sm}{}s", F::NNBSP))
        } else {
            tr!("Av: hver måling brukes som den er.").into()
        });
        let q = v.fan_quiet_until_c;
        if t.quiet_sw.is_active() != q.is_some() {
            t.quiet_sw.set_active(q.is_some());
        }
        t.quiet_spin.set_sensitive(q.is_some());
        if let Some(q) = q
            && t.quiet_spin.value() != q
            && !t.quiet_spin.has_focus()
        {
            t.quiet_spin.set_value(q);
        }
        if t.quiet_unit.text() != F::temp_label() {
            set_text(&t.quiet_unit, F::temp_label());
            oc::temp_spin_refresh(&t.quiet_spin);
        }
        set_text(&t.quiet_note, &match q {
            Some(q) => trf!("Viftene står på {fart} til {temperatur}, deretter følger de kurven.", fart = F::pct(Some(floor), 0), temperatur = F::temp(Some(q), 0)),
            None => tr!("Av: kurven gjelder fra første punkt.").into(),
        });
        set_text(&t.stop_note, &trf!("Viftestopp (0{nb}%) er ikke mulig på dette kortet: fastvaren holder minst {fart}. Det gjelder også Afterburner.", nb = F::NNBSP, fart = F::pct(Some(floor), 0)));
        self.quiet.set(false);
    }

    /// Punktene som tall: temperatur og fart per punkt, og fjern-knappen.
    fn render_points(self: &Rc<Self>, c: &[(f64, f64)], floor: f64) {
        // Enheten er med i nøkkelen: overskriften og tallene bygges på nytt når den byttes.
        let key = format!("n{}{}", c.len(), F::temp_label());
        if self.pts_box.first_child().is_none_or(|g| g.widget_name() != key) {
            crate::widgets::clear(&self.pts_box);
            let g = gtk::Grid::new();
            g.set_widget_name(&key);
            g.add_css_class("pts-grid");
            g.set_column_spacing(6);
            g.set_row_spacing(2);
            for (col, t) in [(1, trf!("Temperatur ({enhet})", enhet = F::temp_label())), (2, tr!("Viftefart (%)").to_string())] {
                g.attach(&label(&t, &["muted", "sm"]), col, 0, 1, 1);
            }
            for i in 0..c.len() {
                let n = label(&format!("{}", i + 1), &["muted", "sm", "num", "pt-n"]);
                n.set_xalign(1.0);
                g.attach(&n, 0, i as i32 + 1, 1, 1);
                let t = gtk::SpinButton::with_range(AXIS_T.0, AXIS_T.1, 1.0);
                let f = gtk::SpinButton::with_range(floor, 100.0, 1.0);
                for (sp, txt) in [(&t, trf!("Punkt {nummer} temperatur", nummer = i + 1)), (&f, trf!("Punkt {nummer} viftefart", nummer = i + 1))] {
                    sp.set_digits(0);
                    sp.set_width_chars(4);
                    sp.add_css_class("ctl-spin");
                    sp.update_property(&[gtk::accessible::Property::Label(&txt)]);
                }
                crate::widgets::oc::temp_spin(&t);
                for (k, sp) in [(0, t.clone()), (1, f.clone())] {
                    let w = Rc::downgrade(self);
                    sp.connect_value_changed(move |sp| {
                        let Some(p) = w.upgrade() else { return };
                        if p.quiet.get() {
                            return;
                        }
                        let mut c = p.curve();
                        if i >= c.len() {
                            return;
                        }
                        // Hele grader og prosent, som i grafen (feltet regner om fra °F).
                        if k == 0 {
                            c[i].0 = sp.value().round();
                        } else {
                            c[i].1 = sp.value().round();
                        }
                        p.set_curve(c);
                    });
                }
                let rm = button(Some("x"), None, &["sm", "flat"]);
                rm.update_property(&[gtk::accessible::Property::Label(&trf!("Fjern punkt {nummer}", nummer = i + 1))]);
                tip(&rm, tr!("Fjern punktet"), None);
                rm.set_sensitive(c.len() > txn::CURVE_MIN_POINTS);
                let w = Rc::downgrade(self);
                rm.connect_clicked(move |_| {
                    if let Some(p) = w.upgrade()
                        && let Some(next) = remove_point(&p.curve(), i)
                    {
                        p.set_curve(next);
                    }
                });
                g.attach(&t, 1, i as i32 + 1, 1, 1);
                g.attach(&f, 2, i as i32 + 1, 1, 1);
                g.attach(&rm, 3, i as i32 + 1, 1, 1);
            }
            self.pts_box.append(&g);
        }
        self.add_pt.set_sensitive(c.len() < txn::CURVE_MAX_POINTS);
        self.quiet.set(true);
        if let Some(g) = self.pts_box.first_child().and_downcast::<gtk::Grid>() {
            for (i, (t, f)) in c.iter().enumerate() {
                for (col, v) in [(1, *t), (2, *f)] {
                    if let Some(sp) = g.child_at(col, i as i32 + 1).and_downcast::<gtk::SpinButton>() {
                        if col == 2 && sp.range() != (floor, 100.0) {
                            sp.set_range(floor, 100.0);
                        }
                        if sp.value() != v && !sp.has_focus() {
                            sp.set_value(v);
                        }
                    }
                }
            }
        }
        self.quiet.set(false);
        self.mark_selected();
    }

    /// Raden for punktet som er valgt i grafen, markeres i tabellen.
    fn mark_selected(&self) {
        let sel = self.editor.selected();
        if let Some(g) = self.pts_box.first_child().and_downcast::<gtk::Grid>() {
            let mut i = 0;
            while let Some(n) = g.child_at(0, i + 1) {
                class(&n, "sel", sel == Some(i as usize));
                i += 1;
            }
        }
    }
}

/// Første og siste tidspunkt (ms) der både temperatur og fart finnes.
fn span_of(v: &Value, kt: &str, kf: &str) -> Option<(i64, i64)> {
    let col = |k: &str| -> Vec<Option<f64>> { serde_json::from_value(v["series"][k]["avg"].clone()).unwrap_or_default() };
    let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
    let (a, b) = (col(kt), col(kf));
    let ok: Vec<i64> = t.iter().zip(a.iter().zip(b.iter())).filter(|(_, (x, y))| x.is_some() && y.is_some()).map(|(t, _)| *t).collect();
    Some((*ok.first()?, *ok.last()?))
}

/// «3 min», «31 t», «2 t 5 min»: hvor lang tid dataene dekker.
fn span_text(ms: i64) -> String {
    let min = (ms as f64 / 60_000.0).round().max(1.0) as i64;
    match (min / 60, min % 60) {
        (0, m) => format!("{m}{}min", F::NNBSP),
        (h, m) if h >= 10 || m == 0 => trf!("{timer}t", timer = format!("{h}{}", F::NNBSP)),
        (h, m) => trf!("{timer}t {minutter}min", timer = format!("{h}{}", F::NNBSP), minutter = format!("{m}{}", F::NNBSP)),
    }
}

/// Parene (temperatur, fart) fra et `get_history`-svar, der begge finnes.
fn pairs(v: &Value, kt: &str, kf: &str) -> Vec<(f64, f64)> {
    let col = |k: &str| -> Vec<Option<f64>> { serde_json::from_value(v["series"][k]["avg"].clone()).unwrap_or_default() };
    let (t, f) = (col(kt), col(kf));
    t.into_iter().zip(f).filter_map(|(a, b)| Some((a?, b?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: [(f64, f64); 4] = [(30.0, 35.0), (50.0, 40.0), (70.0, 65.0), (90.0, 100.0)];

    #[test]
    fn viftevalg_foelger_antallet_og_ukjent_er_ikke_en_vifte() {
        assert_eq!(fan_target_choices(None), vec![("0".into(), "Felles kurve".into())]);
        assert_eq!(fan_target_choices(Some(0)), fan_target_choices(None));
        assert_eq!(fan_target_choices(Some(1)).len(), 2);
        let three = fan_target_choices(Some(3));
        assert_eq!(three.len(), 4);
        assert_eq!(three.last(), Some(&("3".into(), "Vifte 3".into())));
        assert_eq!(fan_target_choices(Some(vifte::MAKS_VIFTER + 1)).len(), vifte::MAKS_VIFTER + 1);
        assert!(fan_control_unavailable(None, FanMode::Manual).is_some());
        assert!(fan_control_unavailable(Some(0), FanMode::Auto).is_some());
        assert!(fan_control_unavailable(Some(3), FanMode::Curve).is_none());
        assert!(fan_control_unavailable(Some(vifte::MAKS_VIFTER + 1), FanMode::Curve).is_some());
        assert!(fan_control_unavailable(Some(vifte::MAKS_VIFTER + 1), FanMode::Manual).is_none());
    }

    #[test]
    fn faktisk_vifteomraade_teller_bare_avleste_kanaler() {
        let fans = [Some(35.0), None, Some(72.0)];
        assert_eq!(measured_fan_range(&fans, Some(3)), Some((35.0, 72.0, 2)));
        assert_eq!(measured_fan_range(&fans, Some(1)), Some((35.0, 35.0, 1)));
        assert_eq!(measured_fan_range(&fans, Some(0)), None);
        assert_eq!(measured_fan_range(&fans, None), Some((35.0, 72.0, 2)));
        assert_eq!(measured_fan_range(&[None, Some(f64::NAN)], None), None);
    }

    #[test]
    fn enkel_visning_oppsummerer_klokkeutkast_uten_vifteendring() {
        let before = Values::default();
        let after = Values { core_offset_mhz: -15.0, ..before.clone() };
        let summary = simple_hidden_draft(&before, &after);
        assert!(summary.controls);
        assert!(!summary.advanced_fans);
        assert_eq!(summary.lines.len(), 1);
        assert!(summary.lines[0].starts_with("GPU-justering:") && summary.lines[0].contains('→'));
        assert!(summary.lines[0].contains("15") && summary.lines[0].contains("MHz"));
        assert_eq!(before.fan_mode, after.fan_mode);
        assert_eq!(before.fan_percent, after.fan_percent);
    }

    #[test]
    fn enkel_visning_viser_inaktivt_tuningutkast_i_auto_uten_aa_endre_porten() {
        let before = Values::default();
        let after = Values { fan_hysteresis_c: 7.0, ..before.clone() };
        let summary = simple_hidden_draft(&before, &after);
        assert_eq!(after.fan_mode, FanMode::Auto);
        assert!(!summary.controls);
        assert!(summary.advanced_fans);
        assert_eq!(summary.lines.len(), 1);
        assert!(summary.lines[0].starts_with("Hysterese:") && summary.lines[0].contains('→'));
        assert!(draft_diff(&before, &after).is_empty(), "inaktiv tuning åpner ikke Bruk-porten alene");
        assert_eq!(before.diff(&after).len(), 1, "det lagrede feltet skal likevel telles og vises");
    }

    #[test]
    fn vanlig_fast_fart_lager_ikke_et_ekstra_skjult_endringspanel() {
        let before = Values::default();
        let after = Values { fan_mode: FanMode::Manual, fan_percent: 60.0, ..before.clone() };
        assert_eq!(simple_hidden_draft(&before, &after), HiddenDraft::default());
        assert_eq!(simple_hidden_draft(&before, &before), HiddenDraft::default());
    }

    #[test]
    fn enkel_visning_skiller_aktiv_kurve_fra_kurve_i_utkast() {
        assert!(simple_curve_message(FanMode::Auto, FanMode::Auto, false).is_none());
        assert!(simple_curve_message(FanMode::Manual, FanMode::Manual, false).is_none());
        let active = simple_curve_message(FanMode::Curve, FanMode::Curve, false).unwrap();
        assert!(active.contains("aktiv") && active.contains("fortsetter"));
        let draft = simple_curve_message(FanMode::Auto, FanMode::Curve, true).unwrap();
        assert!(draft.contains("utkastet") && !draft.contains("er aktiv"));
        let leaving = simple_curve_message(FanMode::Curve, FanMode::Manual, true).unwrap();
        assert!(leaving.contains("aktiv på kortet") && leaving.contains("Modusbyttet ligger bare i utkastet"));
    }

    #[test]
    fn maaleaksen_utvides_uten_aa_endre_normalomraadet_eller_pumpe() {
        let normal = CurveScale::default();
        assert_eq!(normal.expanded(C.into_iter()), normal);
        let wide = normal.expanded([(12.0, 35.0), (110.0, 108.0)].into_iter());
        assert_eq!(wide.temp, (10.0, 120.0));
        assert_eq!(wide.fan, (0.0, 125.0));
        for (t, f) in [(12.0, 35.0), (110.0, 108.0)] {
            let (x, y) = wide.position(t, f);
            assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "maalt punkt har plass innenfor aksene");
        }
        assert_eq!(wide.expanded(C.into_iter()), wide, "aksene krymper ikke ved neste normale prøve");
        let peaks = [(110.0, 108.0), (120.0, 125.0)];
        assert_eq!(normal.expanded(peaks.into_iter()), normal.expanded(peaks.into_iter().rev()), "rekkefølgen på målingene endrer ikke skalaen");
        assert_eq!(normal.expanded([(f64::NAN, f64::INFINITY)].into_iter()), normal);
        assert_eq!(curve_ticks(normal.temp, 10.0), vec![20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0]);
        assert!(curve_ticks((-1000.0, 5000.0), 10.0).len() <= 11);
    }

    #[test]
    fn peker_og_punktinnsetting_foelger_maaleaksen_men_beholder_inputgrensene() {
        let scale = CurveScale::default().expanded([(12.0, 35.0), (110.0, 108.0)].into_iter());
        let (x, y) = scale.position(60.0, 53.0);
        let (t, f) = scale.input_at(x, y);
        assert!((t - 60.0).abs() < 1e-9 && (f - 53.0).abs() < 1e-9);
        let (inserted, i) = add_point(&C, t, 35.0).unwrap();
        assert_eq!(inserted[i], (60.0, 53.0), "nytt punkt settes der pekeren viser 60 grader");
        assert_eq!(scale.input_at(-1.0, -1.0), (20.0, 0.0));
        assert_eq!(scale.input_at(2.0, 2.0), (100.0, 100.0));
        let (t, f) = scale.input_at(1.0, 1.0);
        let moved = move_point(&C, 3, t, f, 35.0);
        assert_eq!(moved[3], (100.0, 100.0));
        let (left, _) = add_point(&C, scale.input_at(0.0, 0.0).0, 35.0).unwrap();
        assert_eq!(left[0], (20.0, 35.0));
        for c in [inserted, moved, left] {
            let (checked, lifted) = txn::check_curve(&json!(c)).expect("utvidet visning gir fortsatt gyldig input");
            assert_eq!(checked, c);
            assert!(!lifted);
        }
    }

    #[test]
    fn median_og_andel() {
        let pts = vec![(40.0, 35.0), (40.2, 36.0), (39.8, 40.0), (60.0, 50.0)];
        assert_eq!(median_per_degree(&pts), vec![(40.0, 36.0)]);
        assert_eq!(share_at_min(&[35.0, 35.0, 40.0, 35.4], 35.0), Some(0.75));
        assert_eq!(share_at_min(&[], 35.0), None);
    }

    #[test]
    fn snap_hele_og_femmer() {
        assert_eq!(snap_value(62.4, false), 62.0);
        assert_eq!(snap_value(62.6, false), 63.0);
        assert_eq!(snap_value(62.4, true), 60.0);
        assert_eq!(snap_value(63.0, true), 65.0);
    }

    #[test]
    fn punkt_holdes_mellom_naboene() {
        // Forbi neste punkt: stopper 1 °C før, og farten ikke over neste.
        let c = move_point(&C, 1, 80.0, 90.0, 35.0);
        assert_eq!(c[1], (69.0, 65.0));
        // Under minste fart og under forrige: løftes.
        let c = move_point(&C, 2, 60.0, 10.0, 35.0);
        assert_eq!(c[2], (60.0, 40.0));
        // Første og siste punkt holdes innenfor aksen.
        assert_eq!(move_point(&C, 0, 5.0, 35.0, 35.0)[0], (AXIS_T.0, 35.0));
        assert_eq!(move_point(&C, 3, 130.0, 120.0, 35.0)[3], (AXIS_T.1, 100.0));
        // Ugyldig indeks: uendret.
        assert_eq!(move_point(&C, 9, 1.0, 1.0, 35.0), C.to_vec());
        // Resultatet er alltid en gyldig kurve for tjenesten.
        for (i, t, f) in [(0, 99.0, 99.0), (1, 0.0, 0.0), (2, 51.0, 36.0), (3, 71.0, 50.0)] {
            let c = move_point(&C, i, t, f, 35.0);
            let v = json!(c.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>());
            let (ok, lifted) = txn::check_curve(&v).expect("gyldig kurve");
            assert_eq!(ok, c);
            assert!(!lifted);
        }
    }

    #[test]
    fn legg_til_og_fjern() {
        // Nytt punkt på kurven, sortert inn.
        let (c, i) = add_point(&C, 60.0, 35.0).expect("plass");
        assert_eq!((i, c[i]), (2, (60.0, 52.5_f64.round())));
        assert_eq!(c.len(), 5);
        // Ikke to punkter på samme grad.
        assert!(add_point(&C, 50.0, 35.0).is_none());
        // Midt mellom to punkter, og 5 °C etter det siste.
        assert_eq!(add_after(&C, 0, 35.0).map(|(c, i)| c[i].0), Some(40.0));
        let tail = [(30.0, 35.0), (80.0, 90.0)];
        assert_eq!(add_after(&tail, 1, 35.0).map(|(c, i)| c[i]), Some((85.0, 90.0)));
        // Ikke plass mellom 50 og 51.
        assert!(add_after(&[(50.0, 40.0), (51.0, 41.0)], 0, 35.0).is_none());
        // Høyst ti punkter.
        let full: Vec<(f64, f64)> = (0..10).map(|i| (25.0 + i as f64 * 7.0, 35.0 + i as f64 * 6.0)).collect();
        assert!(add_point(&full, 99.0, 35.0).is_none());
        // Aldri under to punkter.
        assert_eq!(remove_point(&C, 1).map(|c| c.len()), Some(3));
        assert!(remove_point(&C[..2], 0).is_none());
        assert!(remove_point(&C, 7).is_none());
    }

    #[test]
    fn tilbakestill_kurven() {
        let r = super::super::kontroller::Read { fan_min: Some(35.0), ..Default::default() };
        let st = stock(&r);
        let mut d = Values { fan_curve: C.to_vec(), fan_mode: FanMode::Curve, fan_per_fan: true, fan_curves: vec![C.to_vec(); 2], fan_hysteresis_c: 7.0, fan_ramp_up_pct_s: 30.0, fan_smoothing_s: 5.0, fan_quiet_until_c: Some(60.0), power_limit_w: 200.0, ..Default::default() };
        assert!(!fan_shape_is(&d, &st));
        reset_fan_shape(&mut d, &st);
        assert!(fan_shape_is(&d, &st));
        assert_eq!(d.fan_curve, txn::DEFAULT_CURVE.to_vec());
        assert!(!d.fan_per_fan && d.fan_curves.is_empty());
        assert_eq!(d.fan_mode, FanMode::Curve, "bare kurven og oppførselen");
        assert_eq!(d.power_limit_w, 200.0);
        // Standarden er Balansert, også oppførselen.
        assert_eq!(PRESETS[1].curve, &txn::DEFAULT_CURVE);
        assert!(PRESETS[1].matches(&Values::default()));
        assert_eq!(preset_name(&Values::default()).as_deref(), Some("Balansert"));
    }

    #[test]
    fn forhaandsvalgene_setter_kurve_og_oppfoersel() {
        for p in PRESETS {
            let mut d = Values { fan_per_fan: true, fan_curves: vec![C.to_vec(); 2], ..Default::default() };
            p.apply(&mut d);
            assert!(p.matches(&d), "{}", (p.name)());
            assert!(!d.fan_per_fan, "forhåndsvalgene er felles for viftene");
            assert_eq!(preset_name(&d).as_deref(), Some((p.name)()));
            // Kurven er gyldig for tjenesten.
            let v = json!(p.curve.iter().map(|q| json!([q.0, q.1])).collect::<Vec<_>>());
            let (ok, lifted) = txn::check_curve(&v).unwrap();
            assert!(!lifted && ok == p.curve.to_vec(), "{}", (p.name)());
            // Og oppførselen tas imot uten klemming.
            let (back, cl) = txn::merge(&Values::default(), &d.to_json()).unwrap();
            assert!(cl.is_empty(), "{}: {cl:?}", (p.name)());
            assert_eq!(back, d);
            d.fan_hysteresis_c += 1.0;
            assert_eq!(preset_name(&d), Some(format!("{} · endret", (p.name)())));
        }
        // Stille er roligst, Ytelse raskest.
        let (q, b, y) = (&PRESETS[0], &PRESETS[1], &PRESETS[2]);
        assert!(q.up < b.up && b.up < y.up);
        assert!(q.hyst > b.hyst && b.hyst > y.hyst);
        assert!(q.quiet.is_some() && b.quiet.is_none());
        let egen = Values { fan_curve: C.to_vec(), ..Default::default() };
        assert_eq!(preset_name(&egen), None);
    }

    #[test]
    fn felles_kurve_og_per_vifte() {
        let mut d = Values::default();
        assert_eq!(target_of(&d, 2), 0, "uten egne kurver er det alltid den felles");
        split_curves(&mut d, 2);
        assert!(d.fan_per_fan);
        assert_eq!(d.fan_curves, vec![txn::DEFAULT_CURVE.to_vec(); 2]);
        assert_eq!(target_of(&d, 0), 1);
        set_edited_curve(&mut d, 2, C.to_vec());
        assert_eq!(edited_curve(&d, 2), C.to_vec());
        assert_eq!(edited_curve(&d, 1), txn::DEFAULT_CURVE.to_vec());
        assert_eq!(edited_curve(&d, 0), txn::DEFAULT_CURVE.to_vec(), "den felles er urørt");
        // Tilbake til felles: kurvene per vifte huskes i utkastet.
        d.fan_per_fan = false;
        assert_eq!(edited_curve(&d, target_of(&d, 2)), txn::DEFAULT_CURVE.to_vec());
        set_edited_curve(&mut d, 2, vec![(30.0, 50.0), (80.0, 100.0)]);
        assert_eq!(d.fan_curve, vec![(30.0, 50.0), (80.0, 100.0)], "uten egne kurver endres den felles");
        assert_eq!(d.fan_curves[1], C.to_vec());
        // Mangler vifte 2 sin kurve, fylles den fra den felles.
        let mut e = Values { fan_per_fan: true, ..Default::default() };
        set_edited_curve(&mut e, 2, C.to_vec());
        assert_eq!(e.fan_curves, vec![txn::DEFAULT_CURVE.to_vec(), C.to_vec()]);
    }

    #[test]
    fn linja_som_tegnes() {
        // Uten stille flate: flat før og etter, punktene imellom.
        let l = effective_line(&C, None, 35.0, 20.0, 100.0);
        assert_eq!(l, vec![(20.0, 35.0), (30.0, 35.0), (50.0, 40.0), (70.0, 65.0), (90.0, 100.0), (100.0, 100.0)]);
        // Med stille flate til 60 °C: minste fart, så et sprang til kurven.
        let l = effective_line(&C, Some(60.0), 35.0, 20.0, 100.0);
        assert_eq!(l[..3], [(20.0, 35.0), (60.0, 35.0), (60.0, 52.5)]);
        assert_eq!(l[3], (70.0, 65.0));
        assert_eq!(effective_at(&C, Some(60.0), 35.0, 59.0), 35.0);
        assert_eq!(effective_at(&C, Some(60.0), 35.0, 60.0), 52.5);
        // Linja og motoren er enige.
        let v = Values { fan_curve: C.to_vec(), fan_quiet_until_c: Some(60.0), ..Default::default() };
        let o = vifte::Oppsett::fra_values(&v, 2, 35.0);
        for t in 20..100 {
            assert_eq!(effective_at(&C, Some(60.0), 35.0, t as f64), o.maal(0, t as f64), "{t}");
        }
    }

    #[test]
    fn forhaandsvisningen_regner_med_motoren() {
        // Fem minutter: 30 °C, så 80 °C. Med rampe 10 %/s går farten opp gradvis.
        let t: Vec<i64> = (0..300).map(|i| i * 1000).collect();
        let temp: Vec<Option<f64>> = (0..300).map(|i| Some(if i < 150 { 30.0 } else { 80.0 })).collect();
        let v = Values { fan_curve: C.to_vec(), ..Default::default() };
        let sim = simulate(&v, 2, 35.0, &t, &temp, Some(35.0));
        assert_eq!(sim.len(), 1, "felles kurve: én rad");
        assert_eq!(sim[0][149], 35.0);
        assert_eq!(sim[0][150], 45.0);
        assert!(sim[0][160] > 80.0);
        let per = Values { fan_per_fan: true, fan_curves: vec![C.to_vec(), vec![(20.0, 60.0), (100.0, 60.0)]], ..v };
        let sim = simulate(&per, 2, 35.0, &t, &temp, Some(35.0));
        assert_eq!(sim.len(), 2);
        assert_eq!(sim[1][100], 60.0);
        // Oppsummeringen teller endringer i hele prosent.
        assert_eq!(fan_summary(&[35.0, 35.2, 36.0, 36.0, 40.0]), Some((36.44, 40.0, 2)));
        assert_eq!(fan_summary(&[]), None);
    }

    #[test]
    fn siste_fem_minutter_kutter_ikke_varme() {
        assert_eq!(preview_scale([40.0, 60.0].into_iter()), (0.0, 100.0));
        assert_eq!(preview_scale([40.0, 110.0].into_iter()), (0.0, 125.0));
        assert_eq!(preview_scale([125.0].into_iter()), (0.0, 150.0), "luft over 125 °C");
        assert_eq!(preview_scale([-3.0, 50.0].into_iter()), (-25.0, 100.0));
    }
}
