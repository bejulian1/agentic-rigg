//! Grafen (`RiggChart`, arkitektur.md 6.4): port av `web/js/charts.js`,
//! `history.js` og `chartpanel.js`. Brukes på Oversikt, Grafer, under en rad
//! på Sensorer og (2C) på Kontroller, så alle grafene ser like ut.
//!
//! **Delene:**
//! - [`View`]: historikken for nøklene fra tjenesten (`get_history`, 1 min–48 t)
//!   med de levende prøvene flettet inn. Korte vinduer får prøvene som de er,
//!   lange samles i bøtter med samme bredde som svaret fra tjenesten.
//! - [`Chart`]: selve grafen, tegnet med `snapshot()`. Linjene er `GskPath`,
//!   rutenett og tekst er hele piksler, så den er skarp også med
//!   `GSK_RENDERER=cairo`. Bakgrunnen (rutenett, akser, etiketter) er én
//!   bufret node som bare bygges på nytt når størrelse, tema, skala eller
//!   merkene endrer seg.
//! - [`ChartPanel`]: panelet rundt (tittel, forklaring som skjuler serier,
//!   min/maks, nå-verdi, pause), og [`def_panel`] for grafene i [`DEFS`].
//! - [`ChartSet`]: grafene på en side. Kobler seg én gang til prøvene,
//!   temaet, innstillingene og forbindelsen, og sender videre.
//!
//! **Når den tegner:** bare når tidsaksen har flyttet seg en hel piksel
//! (1 min over 1200 px: 20 ganger i sekundet; 48 t: hvert tredje minutt),
//! eller når dataene endrer seg. «Nå» rundes ned til hele piksler, så hver
//! tegning flytter alt nøyaktig én piksel. Tidtakeren går bare mens grafen
//! er synlig (`map`/`unmap`), står stille på pause, og venter når vinduet er
//! minimert. Med «Redusert» eller «Av» bevegelse flytter grafen seg bare når
//! en ny prøve kommer.
//!
//! **For 2C (Kontroller):** `def_panel(app, "power", DefOpts { window, .. })`
//! gir et ferdig panel. Legg `panel.root` i siden og panelet i en
//! [`ChartSet`] (`set.add(&panel)`), så får det prøvene og temaet.

pub mod math;

use rigg_core::{tr, trf};
use crate::state::{App, Conn};
use crate::widgets::{Icon, hbox, label, set_text};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};
use math::{AxisHold, HoleKind, LimitLine, Mapping, Scale, SeriesView};
use rigg_core::fmt as F;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

pub use math::{WINDOWS, window_choices, window_label};

/// Grafen flytter seg hele piksler, men høyst ett steg per så mange ms
/// (målt: hver tegning koster ca. 1,3 ms CPU per graf med cairo, så 20
/// ganger i sekundet ga 4 % CPU på Grafer; se maalinger.md, runde 2A).
const MIN_STEP_MS: f64 = 400.0;

/// Forskyvningen fra UTC (s) for et tidspunkt, fra systemets tidssone.
pub fn utc_offset(ms: i64) -> i64 {
    let t: libc::time_t = (ms / 1000) as libc::time_t;
    // SAFETY: localtime_r skriver bare i `tm`, som vi eier.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff
}

// ---------------------------------------------------------------- Måling

/// `RIGG_2A_PROFIL=1`: hvor mange ganger grafene tegnes og hva `snapshot()`
/// koster, skrevet til stderr hvert 10. s (maalinger.md, runde 2A).
pub mod profile {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::time::Instant;

    /// Siden når, og (antall, ms) per del.
    type Acc = (Option<Instant>, BTreeMap<&'static str, (u32, f64)>);

    thread_local! {
        static ON: Cell<Option<bool>> = const { Cell::new(None) };
        static ACC: RefCell<Acc> = const { RefCell::new((None, BTreeMap::new())) };
    }

    fn on() -> bool {
        ON.with(|o| {
            if o.get().is_none() {
                o.set(Some(std::env::var_os("RIGG_2A_PROFIL").is_some()));
            }
            o.get().unwrap_or(false)
        })
    }

    /// Måler hele bildet i vinduet: oppsett (før tegning) og tegning.
    pub fn watch(w: &impl gtk::prelude::IsA<gtk::Widget>) {
        use gtk::prelude::*;
        thread_local! { static DONE: Cell<bool> = const { Cell::new(false) }; }
        if !on() || DONE.with(|d| d.replace(true)) {
            return;
        }
        let Some(fc) = w.frame_clock() else {
            DONE.with(|d| d.set(false));
            return;
        };
        let t = std::rc::Rc::new(Cell::new(None::<Instant>));
        let t1 = t.clone();
        fc.connect_before_paint(move |_| t1.set(Some(Instant::now())));
        let t2 = t.clone();
        fc.connect_layout(move |_| stop("bilde med oppsett", t2.get()));
        let t3 = t.clone();
        fc.connect_after_paint(move |_| stop("bilde i alt", t3.take()));
    }

    pub fn start() -> Option<Instant> {
        on().then(Instant::now)
    }

    pub fn stop(what: &'static str, t0: Option<Instant>) {
        let Some(t0) = t0 else { return };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        ACC.with(|a| {
            let mut a = a.borrow_mut();
            let since = *a.0.get_or_insert_with(Instant::now);
            let e = a.1.entry(what).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += ms;
            if since.elapsed().as_secs_f64() >= 10.0 {
                let s = since.elapsed().as_secs_f64();
                for (k, (n, t)) in &a.1 {
                    eprintln!("RIGG_PROFIL {k}: {:.1}/s, {:.2} ms per gang, {:.2} ms/s", *n as f64 / s, t / *n as f64, t / s);
                }
                a.1.clear();
                a.0 = Some(Instant::now());
            }
        });
    }
}

// ---------------------------------------------------------------- Farger

/// Fargene grafene tegner med, fra temaet som vises (`tokens.css`).
#[derive(Clone)]
pub struct Colors {
    pub grid: gdk::RGBA,
    pub axis: gdk::RGBA,
    pub dim: gdk::RGBA,
    pub text: gdk::RGBA,
    pub hatch: gdk::RGBA,
    pub tip_bg: gdk::RGBA,
    pub tip_fg: gdk::RGBA,
    pub accent: gdk::RGBA,
    pub warn: gdk::RGBA,
    pub danger: gdk::RGBA,
    /// Teksten til grenselinjene, og flaten bak den.
    pub warn_text: gdk::RGBA,
    pub danger_text: gdk::RGBA,
    pub label_bg: gdk::RGBA,
    pub track: gdk::RGBA,
    pub hc: bool,
    series: HashMap<&'static str, gdk::RGBA>,
}

pub const SERIES_TOKENS: [&str; 7] = ["--c-load", "--c-memctl", "--c-temp", "--c-power", "--c-clock", "--c-mem", "--c-limit"];

impl Colors {
    pub fn from_app(app: &App) -> Colors {
        let c = |t: &str| crate::theme::rgba(app, t);
        Colors {
            grid: c("--grid"),
            axis: c("--muted"),
            dim: c("--dim"),
            text: c("--text"),
            hatch: c("--hatch"),
            tip_bg: c("--tip-bg"),
            tip_fg: c("--tip-fg"),
            accent: c("--accent-fill"),
            warn: c("--warn-fill"),
            danger: c("--danger-fill"),
            warn_text: c("--warn"),
            danger_text: c("--danger"),
            label_bg: c("--card"),
            track: c("--track"),
            hc: app.settings().high_contrast,
            series: SERIES_TOKENS.iter().map(|t| (*t, c(t))).collect(),
        }
    }

    pub fn series(&self, token: &str) -> gdk::RGBA {
        self.series.get(token).copied().unwrap_or(self.accent)
    }
}

pub fn alpha(c: gdk::RGBA, a: f32) -> gdk::RGBA {
    gdk::RGBA::new(c.red(), c.green(), c.blue(), c.alpha() * a)
}

// ---------------------------------------------------------------- Historikken

/// Én serie i dataene: snitt per punkt, og min og maks i lange vinduer.
#[derive(Clone, Default)]
pub struct Col {
    pub avg: Vec<Option<f64>>,
    pub lo: Vec<Option<f64>>,
    pub hi: Vec<Option<f64>>,
}

/// Punktene for alle nøklene, sortert etter tid (ms, midt i bøtta).
#[derive(Clone, Default)]
pub struct Data {
    pub t: Vec<i64>,
    pub s: Vec<Col>,
}

impl Data {
    fn drain_front(&mut self, n: usize) {
        self.t.drain(..n);
        for c in &mut self.s {
            c.avg.drain(..n);
            c.lo.drain(..n);
            c.hi.drain(..n);
        }
    }
}

struct Open {
    bucket: i64,
    /// (sum, antall, min, maks) per nøkkel.
    acc: Vec<Option<(f64, u32, f64, f64)>>,
}

/// Historikken for et sett nøkler i ett tidsvindu (demoens `HistoryView`).
pub struct View {
    app: Weak<App>,
    pub keys: Vec<String>,
    window: Cell<i64>,
    points: Cell<usize>,
    pub data: RefCell<Data>,
    step_ms: Cell<i64>,
    cadence_ms: Cell<i64>,
    version: Cell<u64>,
    /// Øker når vinduet er hentet på nytt (ikke for hver levende prøve).
    loads: Cell<u64>,
    loading: Cell<bool>,
    error: RefCell<Option<String>>,
    seq: Cell<u64>,
    open: RefCell<Option<Open>>,
    on_change: RefCell<Option<Box<dyn Fn()>>>,
}

impl View {
    pub fn new(app: &Rc<App>, keys: Vec<String>, window: i64, points: usize) -> Rc<View> {
        let n = keys.len();
        let v = Rc::new(View {
            app: Rc::downgrade(app),
            keys,
            window: Cell::new(window),
            points: Cell::new(points.clamp(50, 2000)),
            data: RefCell::new(Data { t: vec![], s: vec![Col::default(); n] }),
            step_ms: Cell::new(1000),
            cadence_ms: Cell::new(app.settings().interval_ms as i64),
            version: Cell::new(0),
            loads: Cell::new(0),
            loading: Cell::new(false),
            error: RefCell::new(None),
            seq: Cell::new(0),
            open: RefCell::new(None),
            on_change: RefCell::new(None),
        });
        v.load();
        v
    }

    pub fn window(&self) -> i64 {
        self.window.get()
    }

    pub fn step_ms(&self) -> i64 {
        self.step_ms.get()
    }

    pub fn loading(&self) -> bool {
        self.loading.get()
    }

    pub fn error(&self) -> Option<String> {
        self.error.borrow().clone()
    }

    pub fn gap_ms(&self) -> i64 {
        math::gap_ms(self.step_ms.get(), self.cadence_ms.get())
    }

    fn changed(&self) {
        self.version.set(self.version.get() + 1);
        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
    }

    /// Den ferske målingen kan være høyere enn snittet i en åpen
    /// langtidsbøtte. Ta den med på aksen før bøttas midtpunkt er synlig.
    fn latest_value(&self, key: &str, now: i64) -> Option<f64> {
        let app = self.app.upgrade()?;
        if !app.client.is_connected() || (key.starts_with("gpu:") && gpu_stopped(&app).is_some()) {
            return None;
        }
        let tel = app.telemetry()?;
        let age = now.saturating_sub(tel.t);
        if age < -2000 || age > math::gap_ms(1000, self.cadence_ms.get()) {
            return None;
        }
        app.value(key).filter(|v| v.is_finite())
    }

    pub fn set_window(self: &Rc<Self>, sec: i64) {
        if sec == self.window.get() {
            return;
        }
        self.window.set(sec);
        {
            let mut d = self.data.borrow_mut();
            d.t.clear();
            for c in &mut d.s {
                *c = Col::default();
            }
        }
        *self.open.borrow_mut() = None;
        self.loads.set(self.loads.get() + 1);
        self.changed();
        self.load();
    }

    /// Grafens bredde i piksler: lange vinduer hentes på nytt når bredden
    /// har endret seg mer enn 25 %.
    pub fn set_points(self: &Rc<Self>, n: usize) {
        let p = n.clamp(50, 2000);
        let cur = self.points.get();
        if (p as f64 - cur as f64).abs() / (cur as f64) < 0.25 {
            return;
        }
        self.points.set(p);
        if math::is_long(self.window.get()) {
            self.load();
        }
    }

    /// Henter vinduet fra tjenesten.
    pub fn load(self: &Rc<Self>) {
        let Some(app) = self.app.upgrade() else { return };
        if self.keys.is_empty() {
            return;
        }
        let seq = self.seq.get() + 1;
        self.seq.set(seq);
        self.loading.set(true);
        let me = Rc::downgrade(self);
        let window = self.window.get();
        let points = if math::is_long(window) { self.points.get() } else { (window as usize).max(10) };
        app.client.request("get_history", json!({"window": window, "keys": self.keys, "points": points}), move |res| {
            let Some(v) = me.upgrade() else { return };
            if v.seq.get() != seq {
                return;
            }
            v.loading.set(false);
            v.loads.set(v.loads.get() + 1);
            match res {
                Ok(r) => v.adopt(&r),
                Err(e) => *v.error.borrow_mut() = Some(e.message),
            }
            v.changed();
        });
    }

    fn adopt(&self, r: &Value) {
        let step_ms = ((r["step"].as_f64().unwrap_or(1.0)) * 1000.0).round().max(1000.0) as i64;
        self.step_ms.set(step_ms);
        let cad = r["cadence"].as_array().map(|a| a.iter().filter_map(|c| c.get(1).and_then(Value::as_i64)).max().unwrap_or(0)).unwrap_or(0);
        if let Some(app) = self.app.upgrade() {
            self.cadence_ms.set(cad.max(app.settings().interval_ms as i64));
        }
        let ts: Vec<i64> = r["t"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
        let long = step_ms > 1000;
        let half = step_ms / 2;
        let mut d = Data { t: ts.iter().map(|t| t + half).collect(), s: Vec::with_capacity(self.keys.len()) };
        let arr = |s: &Value, k: &str| -> Vec<Option<f64>> { s[k].as_array().map(|a| a.iter().map(Value::as_f64).collect()).unwrap_or_default() };
        for k in &self.keys {
            let s = &r["series"][k];
            let mut c = Col { avg: arr(s, "avg"), lo: vec![], hi: vec![] };
            c.avg.resize(d.t.len(), None);
            if long {
                c.lo = arr(s, "min");
                c.hi = arr(s, "max");
            }
            c.lo.resize(d.t.len(), None);
            c.hi.resize(d.t.len(), None);
            d.s.push(c);
        }
        // Levende prøver som kom etter tjenestens siste bøtte, beholdes.
        let last = d.t.last().map(|t| t + half).unwrap_or(0);
        let old = self.data.borrow();
        if !long {
            for (i, t) in old.t.iter().enumerate() {
                if *t > last {
                    d.t.push(*t);
                    for (k, c) in d.s.iter_mut().enumerate() {
                        c.avg.push(old.s.get(k).and_then(|o| o.avg.get(i).copied().flatten()));
                        c.lo.push(None);
                        c.hi.push(None);
                    }
                }
            }
        }
        drop(old);
        *self.data.borrow_mut() = d;
        *self.open.borrow_mut() = None;
        *self.error.borrow_mut() = None;
    }

    /// Tar inn den siste prøven fra `app` (kalles av [`ChartSet`]).
    pub fn live(&self, app: &App) {
        let Some(tel) = app.telemetry() else { return };
        let t = tel.t;
        self.cadence_ms.set(self.cadence_ms.get().max(tel.interval_ms as i64).min(5000));
        if self.loading.get() && self.data.borrow().t.is_empty() {
            return;
        }
        let vals: Vec<Option<f64>> = self.keys.iter().map(|k| app.value(k)).collect();
        let step = self.step_ms.get();
        {
            let mut d = self.data.borrow_mut();
            if d.t.last().is_some_and(|l| *l >= t) && step <= 1000 {
                return; // samme prøve to ganger (ny tilkobling)
            }
            if step <= 1000 {
                d.t.push(t);
                for (c, v) in d.s.iter_mut().zip(&vals) {
                    c.avg.push(*v);
                    c.lo.push(None);
                    c.hi.push(None);
                }
            } else {
                let b = t.div_euclid(step) * step;
                let mid = b + step / 2;
                let mut open = self.open.borrow_mut();
                if open.as_ref().is_none_or(|o| o.bucket != b) {
                    let mut acc = vec![None; vals.len()];
                    if d.t.last().is_some_and(|l| *l >= mid) {
                        // Tjenestens svar hadde allerede denne bøtta: fortsett i den.
                        let i = d.t.len() - 1;
                        for (k, a) in acc.iter_mut().enumerate() {
                            if let Some(v) = d.s[k].avg[i] {
                                *a = Some((v, 1, d.s[k].lo[i].unwrap_or(v), d.s[k].hi[i].unwrap_or(v)));
                            }
                        }
                    } else {
                        d.t.push(mid);
                        for c in &mut d.s {
                            c.avg.push(None);
                            c.lo.push(None);
                            c.hi.push(None);
                        }
                    }
                    *open = Some(Open { bucket: b, acc });
                }
                let o = open.as_mut().unwrap();
                let i = d.t.len() - 1;
                for (k, v) in vals.iter().enumerate() {
                    let Some(v) = v.filter(|v| v.is_finite()) else { continue };
                    let a = o.acc[k].get_or_insert((0.0, 0, f64::INFINITY, f64::NEG_INFINITY));
                    a.0 += v;
                    a.1 += 1;
                    a.2 = a.2.min(v);
                    a.3 = a.3.max(v);
                    d.s[k].avg[i] = Some(a.0 / a.1 as f64);
                    d.s[k].lo[i] = Some(a.2);
                    d.s[k].hi[i] = Some(a.3);
                }
            }
            // Eldre enn vinduet (pluss litt) tas bort.
            let cut = t - self.window.get() * 1000 - step * 2;
            let n = math::first_index(&d.t, cut);
            if n > 0 {
                d.drain_front(n);
            }
        }
        self.changed();
    }

    /// Første tidspunkt med data (bøttas start i lange vinduer).
    pub fn first(&self, data: &Data) -> Option<i64> {
        let s = self.step_ms.get();
        data.t.first().map(|t| if s > 1000 { t - s / 2 } else { *t })
    }
}

// ---------------------------------------------------------------- Serier

/// Én serie i en graf. `map` regner om før tegning (°F, MB/s), og `fmt`
/// formaterer verdien som er regnet om (verktøytipset).
#[derive(Clone)]
pub struct SeriesDef {
    pub key: String,
    pub label: String,
    /// Token for den faste seriefargen («--c-temp»).
    pub color: &'static str,
    pub fill: bool,
    pub dashed: bool,
    pub step: bool,
    pub band: bool,
    pub map: Rc<dyn Fn(f64) -> f64>,
    pub fmt: Option<Rc<dyn Fn(f64) -> String>>,
}

impl SeriesDef {
    pub fn new(key: &str, label: &str, color: &'static str) -> SeriesDef {
        SeriesDef { key: key.into(), label: label.into(), color, fill: false, dashed: false, step: false, band: true, map: Rc::new(|v| v), fmt: None }
    }
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }
    pub fn limit(mut self) -> Self {
        self.dashed = true;
        self.step = true;
        self.band = false;
        self
    }
    pub fn map(mut self, f: impl Fn(f64) -> f64 + 'static) -> Self {
        self.map = Rc::new(f);
        self
    }
    pub fn fmt(mut self, f: impl Fn(f64) -> String + 'static) -> Self {
        self.fmt = Some(Rc::new(f));
        self
    }
}

/// Valgene for y-aksen og tallene.
#[derive(Clone)]
pub struct ChartOpts {
    /// Det foretrukne området (math::Axis): det «Fast» viser, og det
    /// «Tilpass» holder seg innenfor så lenge dataene gjør det. Verdier
    /// utenfor utvider aksen; ingenting kuttes.
    pub range: Rc<dyn Fn() -> (f64, f64)>,
    /// Hard grense, bare der verdien ikke kan gå utenfor (last i prosent).
    pub hard: Option<(f64, f64)>,
    /// Kritiske grenser som streker (nedklokking, avstenging), i verdien som vises.
    pub limits: Rc<dyn Fn() -> Vec<LimitLine>>,
    pub min_span: Rc<dyn Fn() -> f64>,
    /// Følger skalaen dataene («Tilpass»)?
    pub auto: Rc<dyn Fn() -> bool>,
    /// Temperaturserier: aksen følger [`math::temp_axis`] (alltid minst til
    /// 100 °C), uansett `auto` og `min_span`. Settes med [`ChartOpts::temp`].
    pub temp: bool,
    pub fill: Rc<dyn Fn() -> bool>,
    pub unit: Rc<dyn Fn() -> String>,
    pub decimals: usize,
    /// Hvorfor det er et hull (verktøytipset på den skraverte flaten).
    pub gap_reason: Rc<dyn Fn() -> String>,
    /// Kilden har stoppet («Kortet svarer ikke siden 18:03»): hullet etter
    /// siste punkt får teksten, og flaten før dataene sier ikke «Samler data».
    pub stopped: Rc<dyn Fn() -> Option<String>>,
    pub motion_full: Rc<dyn Fn() -> bool>,
    pub lag_ms: Rc<dyn Fn() -> i64>,
}

impl ChartOpts {
    pub fn new(app: &Rc<App>, range: impl Fn() -> (f64, f64) + 'static) -> ChartOpts {
        let a = Rc::downgrade(app);
        let a2 = a.clone();
        let a3 = a.clone();
        let a4 = a.clone();
        ChartOpts {
            range: Rc::new(range),
            hard: None,
            limits: Rc::new(Vec::new),
            min_span: Rc::new(|| 0.0),
            auto: Rc::new(move || a.upgrade().is_some_and(|a| a.settings().chart_scale == "auto")),
            temp: false,
            fill: Rc::new(move || a2.upgrade().is_none_or(|a| a.settings().chart_fill)),
            unit: Rc::new(String::new),
            decimals: 0,
            gap_reason: Rc::new(move || a3.upgrade().map(|a| gap_reason(&a)).unwrap_or_default()),
            stopped: Rc::new(|| None),
            motion_full: Rc::new(move || a4.upgrade().is_some_and(|a| crate::motion::level(&a) == crate::motion::Level::Full)),
            lag_ms: {
                let a = Rc::downgrade(app);
                Rc::new(move || a.upgrade().map(|a| a.settings().interval_ms as i64 + 150).unwrap_or(1150))
            },
        }
    }
}

impl ChartOpts {
    /// En graf med temperaturserier (GPU, CPU, disk eller flere sammen).
    /// Alle temperaturgrafene i appen lages her, så de får samme akseregel:
    /// fra `range` sin nedre grense til minst 100 °C ([`math::temp_axis`]).
    /// `range` er i enheten som vises (°C eller °F).
    pub fn temp(app: &Rc<App>, range: impl Fn() -> (f64, f64) + 'static) -> ChartOpts {
        let mut o = ChartOpts::new(app, range);
        o.temp = true;
        o.unit = Rc::new(|| F::temp_label().into());
        o
    }
}

/// Navnet midt i en setning: «Minnekontroller» → «minnekontroller», men
/// forkortelser som «GPU» og «CPU-last» står som de er.
pub fn lower_name(s: &str) -> String {
    let first = s.split([' ', '-']).next().unwrap_or("");
    if first.chars().filter(|c| c.is_uppercase()).count() >= 2 {
        return s.to_string();
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// «Kortet svarer ikke siden 18:03» når det valgte kortet er borte.
pub fn gpu_stopped(app: &App) -> Option<String> {
    let i = app.gpu_info()?;
    (i.state == rigg_core::model::DeviceState::Lost).then(|| trf!("Kortet svarer ikke siden {verdi}", verdi = F::since(crate::app::local_time(i.since), crate::app::local_now())))
}

/// Hvorfor grafen har et hull. Med «Fortsett i bakgrunnen» av tar tjenesten
/// ikke opp når appen er lukket (produktspesifikasjonen 3.1).
pub fn gap_reason(app: &App) -> String {
    let bg = app.data.borrow().service["background"].as_bool().unwrap_or(false);
    if bg {
        tr!("Tjenesten tok ikke opp her (maskinen var av eller sov).").into()
    } else {
        tr!("Hull i grafen: appen var lukket. Slå på «Fortsett i bakgrunnen» under Innstillinger › Generelt for å ta opp også da.").into()
    }
}

// ---------------------------------------------------------------- Markøren

/// Markøren som deles av grafene på en side (Innstillinger › Grafer og
/// sensorer › «Markør på alle grafene», og siktet i toppfeltet på Grafer).
pub struct CursorSync {
    pub t: Cell<Option<i64>>,
    pub on: Rc<dyn Fn() -> bool>,
    charts: RefCell<Vec<glib::WeakRef<Chart>>>,
}

impl CursorSync {
    pub fn new(on: impl Fn() -> bool + 'static) -> Rc<CursorSync> {
        Rc::new(CursorSync { t: Cell::new(None), on: Rc::new(on), charts: RefCell::new(vec![]) })
    }

    fn add(&self, c: &Chart) {
        let mut v = self.charts.borrow_mut();
        v.retain(|w| w.upgrade().is_some());
        v.push(c.downgrade());
    }

    pub fn set(&self, t: Option<i64>) {
        if self.t.get() == t {
            return;
        }
        self.t.set(t);
        for c in self.charts.borrow().iter().filter_map(|w| w.upgrade()) {
            c.queue_draw();
        }
    }
}

// ---------------------------------------------------------------- Grafen

struct Inner {
    series: Vec<SeriesDef>,
    opts: ChartOpts,
    view: Rc<View>,
    sync: Option<Rc<CursorSync>>,
    colors: Colors,
}

/// Bufret bakgrunn: rutenett, aksetall og tidsaksen.
struct BgCache {
    key: String,
    node: gsk::RenderNode,
}

/// Det siste oppsettet, så musa kan regnes om til tid.
#[derive(Clone, Copy, Default)]
struct Geom {
    pad_l: f32,
    pw: f32,
    t0: i64,
    span: i64,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Chart {
        pub(super) inner: RefCell<Option<Inner>>,
        pub(super) hidden: RefCell<HashSet<usize>>,
        pub(super) paused: Cell<bool>,
        pub(super) paused_at: Cell<i64>,
        pub(super) snap: RefCell<Option<Data>>,
        pub(super) hover_x: Cell<Option<f32>>,
        pub(super) geom: Cell<Geom>,
        pub(super) bg: RefCell<Option<BgCache>>,
        /// Aksen som holdes (math::AxisHold), med nøkkelen den gjelder for.
        pub(super) hold: RefCell<(String, AxisHold)>,
        pub(super) layouts: RefCell<HashMap<(String, bool), pango::Layout>>,
        pub(super) timer: RefCell<Option<glib::SourceId>>,
        pub(super) last_data_draw: Cell<i64>,
        /// Flytter seg bare med nye prøver, ikke hver piksel (Oversikt, 8A).
        pub(super) calm: Cell<bool>,
        pub(super) drawn_loads: Cell<u64>,
        pub(super) on_points: crate::widgets::Slot<dyn Fn(usize)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Chart {
        const NAME: &'static str = "RiggChart";
        type Type = super::Chart;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("chart");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Chart {
        fn dispose(&self) {
            if let Some(id) = self.timer.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for Chart {
        fn map(&self) {
            self.parent_map();
            profile::watch(&*self.obj());
            self.obj().schedule();
        }

        fn unmap(&self) {
            if let Some(id) = self.timer.take() {
                id.remove();
            }
            self.parent_unmap();
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
        }
    }
}

glib::wrapper! {
    pub struct Chart(ObjectSubclass<imp::Chart>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Chart {
    pub fn new(app: &Rc<App>, view: Rc<View>, series: Vec<SeriesDef>, opts: ChartOpts, sync: Option<Rc<CursorSync>>) -> Chart {
        let c: Chart = glib::Object::new();
        c.set_focusable(true);
        c.set_hexpand(true);
        c.set_vexpand(true);
        c.set_overflow(gtk::Overflow::Hidden);
        if let Some(sy) = &sync {
            sy.add(&c);
        }
        *c.imp().inner.borrow_mut() = Some(Inner { series, opts, view, sync, colors: Colors::from_app(app) });
        let motion = gtk::EventControllerMotion::new();
        let w = c.downgrade();
        motion.connect_motion(move |_, x, _| {
            if let Some(c) = w.upgrade() {
                c.hover(Some(x as f32));
            }
        });
        let w = c.downgrade();
        motion.connect_leave(move |_| {
            if let Some(c) = w.upgrade() {
                c.hover(None);
            }
        });
        c.add_controller(motion);
        c
    }

    fn with<R>(&self, f: impl FnOnce(&Inner) -> R) -> Option<R> {
        self.imp().inner.borrow().as_ref().map(f)
    }

    pub fn view(&self) -> Rc<View> {
        self.with(|i| i.view.clone()).expect("grafen har en visning")
    }

    pub fn series(&self) -> Vec<SeriesDef> {
        self.with(|i| i.series.clone()).unwrap_or_default()
    }

    /// Bredden i piksler (til `View::set_points`).
    pub fn connect_points(&self, f: impl Fn(usize) + 'static) {
        *self.imp().on_points.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_colors(&self, c: Colors) {
        if let Some(i) = self.imp().inner.borrow_mut().as_mut() {
            i.colors = c;
        }
        // Temaet kan også ha endret skriftstørrelsen.
        self.imp().layouts.borrow_mut().clear();
        *self.imp().bg.borrow_mut() = None;
        self.queue_draw();
    }

    pub fn is_hidden(&self, i: usize) -> bool {
        self.imp().hidden.borrow().contains(&i)
    }

    pub fn set_hidden(&self, i: usize, hide: bool) {
        let mut h = self.imp().hidden.borrow_mut();
        if hide {
            h.insert(i);
        } else {
            h.remove(&i);
        }
        drop(h);
        *self.imp().bg.borrow_mut() = None;
        self.queue_draw();
    }

    pub fn paused(&self) -> bool {
        self.imp().paused.get()
    }

    /// Tidspunktet grafen står på mens den er på pause (veggklokke, ms).
    pub fn paused_at(&self) -> i64 {
        self.imp().paused_at.get()
    }

    pub fn set_paused(&self, p: bool) {
        let imp = self.imp();
        if imp.paused.get() == p {
            return;
        }
        imp.paused.set(p);
        if p {
            let lag = self.with(|i| (i.opts.lag_ms)()).unwrap_or(0);
            imp.paused_at.set(rigg_core::now_ms() - lag);
            *imp.snap.borrow_mut() = Some(self.view().data.borrow().clone());
        } else {
            *imp.snap.borrow_mut() = None;
        }
        self.schedule();
        self.queue_draw();
    }

    /// Nye data i visningen. Korte vinduer tegnes på nytt straks (eller ved
    /// neste piksel når grafen ruller), lange høyst hvert 5. s.
    pub fn data_changed(&self) {
        let imp = self.imp();
        if imp.paused.get() || !self.is_mapped() {
            return;
        }
        let calm = imp.calm.get();
        let (long, scrolling) = self.with(|i| (math::is_long(i.view.window()), (i.opts.motion_full)() && !calm)).unwrap_or((false, false));
        let now = rigg_core::now_ms();
        // Nytt vindu fra tjenesten (eller tømt) tegnes straks.
        let version_jump = self.view().loads.get() != imp.drawn_loads.get();
        if long {
            if now - imp.last_data_draw.get() >= 5000 || version_jump {
                imp.last_data_draw.set(now);
                self.queue_draw();
            }
        } else if !scrolling || version_jump {
            self.queue_draw();
        }
    }

    /// Rolig graf: flytter seg bare når en ny prøve kommer (hvert sekund), i
    /// samme bilde som resten av siden, ikke hver piksel mellom prøvene. Brukes
    /// på Oversikt, der seks grafer ellers ga 4–5 bilder i sekundet (runde 8A).
    pub fn set_calm(&self, calm: bool) {
        self.imp().calm.set(calm);
        self.schedule();
    }

    /// Setter markøren som om musa sto der (skjermbildene). `frac`: 0–1 over plottet.
    pub fn set_hover_frac(&self, frac: Option<f32>) {
        let g = self.imp().geom.get();
        self.hover(frac.map(|f| g.pad_l + f * g.pw));
    }

    fn hover(&self, x: Option<f32>) {
        self.imp().hover_x.set(x);
        let g = self.imp().geom.get();
        let th = x.filter(|x| g.pw > 0.0 && *x >= g.pad_l && *x <= g.pad_l + g.pw).map(|x| g.t0 + (((x - g.pad_l) / g.pw) as f64 * g.span as f64) as i64);
        if let Some(sy) = self.with(|i| i.sync.clone()).flatten() {
            if (sy.on)() {
                sy.set(th);
            } else {
                sy.set(None);
            }
        }
        self.queue_draw();
    }

    /// Neste tegning når tidsaksen har gått en hel piksel.
    fn schedule(&self) {
        let imp = self.imp();
        if let Some(id) = imp.timer.take() {
            id.remove();
        }
        if !self.is_mapped() || imp.paused.get() {
            return;
        }
        let calm = imp.calm.get();
        let Some((full, span, lag)) = self.with(|i| ((i.opts.motion_full)() && !calm, i.view.window() * 1000, (i.opts.lag_ms)())) else { return };
        let g = imp.geom.get();
        let pw = if g.pw > 0.0 { g.pw } else { 600.0 };
        let ms_px = math::ms_per_px(span, pw);
        // Uten full bevegelse flytter grafen seg bare med nye prøver (data_changed).
        // Lange vinduer flytter seg sjelden; da holder en piksel om gangen.
        if !full && ms_px < 1000.0 {
            return;
        }
        // Felles rutenett i tid for korte vinduer, så grafene på siden tegnes
        // i samme bilde; lange vinduer når tidsaksen har gått en piksel.
        let tick = math::scroll_tick(ms_px, MIN_STEP_MS);
        let now = rigg_core::now_ms();
        let wait = if tick > MIN_STEP_MS { math::next_pixel_in(now - lag, tick) } else { math::next_pixel_in(now, tick) }.min(60_000);
        let w = self.downgrade();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(wait), move || {
            let Some(c) = w.upgrade() else { return };
            c.imp().timer.take();
            if c.window_hidden() {
                // Minimert: vent uten å tegne.
                let w2 = c.downgrade();
                let id = glib::timeout_add_local_once(std::time::Duration::from_secs(1), move || {
                    if let Some(c) = w2.upgrade() {
                        c.imp().timer.take();
                        c.schedule();
                    }
                });
                *c.imp().timer.borrow_mut() = Some(id);
                return;
            }
            c.queue_draw();
            c.schedule();
        });
        *imp.timer.borrow_mut() = Some(id);
    }

    fn window_hidden(&self) -> bool {
        let Some(root) = self.root() else { return true };
        let Some(surface) = root.surface() else { return true };
        surface.downcast::<gdk::Toplevel>().is_ok_and(|t| t.state().intersects(gdk::ToplevelState::MINIMIZED | gdk::ToplevelState::SUSPENDED))
    }

    // ------------------------------------------------------------ Tekst

    fn layout(&self, text: &str, small: bool) -> pango::Layout {
        let key = (text.to_string(), small);
        if let Some(l) = self.imp().layouts.borrow().get(&key) {
            return l.clone();
        }
        let l = self.create_pango_layout(Some(text));
        if small {
            // --fs-sm i forhold til grafens --fs-cap (12 mot 11 px).
            let attrs = l.attributes().unwrap_or_default();
            attrs.insert(pango::AttrFloat::new_scale(12.0 / 11.0));
            l.set_attributes(Some(&attrs));
        }
        let mut cache = self.imp().layouts.borrow_mut();
        if cache.len() > 400 {
            cache.clear();
        }
        cache.insert(key, l.clone());
        l
    }

    fn text_size(l: &pango::Layout) -> (f32, f32) {
        let (w, h) = l.pixel_size();
        (w as f32, h as f32)
    }

    /// Tekst med venstre kant (`ax` 0), midt (0,5) eller høyre kant (1) ved x,
    /// og midt på y.
    fn put(s: &gtk::Snapshot, l: &pango::Layout, x: f32, y: f32, ax: f32, color: &gdk::RGBA) {
        let (w, h) = Self::text_size(l);
        s.save();
        s.translate(&graphene::Point::new((x - w * ax).round(), (y - h / 2.0).round()));
        s.append_layout(l, color);
        s.restore();
    }

    // ------------------------------------------------------------ Tegning

    fn draw(&self, s: &gtk::Snapshot) {
        let t0 = profile::start();
        self.draw_inner(s);
        profile::stop("graf", t0);
    }

    fn draw_inner(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let inner = imp.inner.borrow();
        let Some(inner) = inner.as_ref() else { return };
        let (w, h) = (self.width() as f32, self.height() as f32);
        if w < 40.0 || h < 40.0 {
            return;
        }
        let c = &inner.colors;
        let view = &inner.view;
        imp.drawn_loads.set(view.loads.get());
        let snap_guard = imp.snap.borrow();
        let live_guard;
        let data: &Data = if imp.paused.get()
            && let Some(sn) = snap_guard.as_ref()
        {
            sn
        } else {
            live_guard = view.data.borrow();
            &live_guard
        };
        let win = view.window();
        let span = win * 1000;
        let lag = (inner.opts.lag_ms)();
        let g_old = imp.geom.get();
        let ms_px = math::ms_per_px(span, if g_old.pw > 0.0 { g_old.pw } else { w - 60.0 });
        let real_now = rigg_core::now_ms();
        let t_now = if imp.paused.get() { imp.paused_at.get() } else { math::quantize(real_now - if math::is_long(win) { 0 } else { lag }, ms_px) };
        let t0 = t_now - span;
        let gap = view.gap_ms();
        let start = math::first_index(&data.t, t0 - gap);
        let hidden = imp.hidden.borrow();
        let visible: Vec<usize> = (0..inner.series.len()).filter(|i| !hidden.contains(i)).collect();
        let sv = |k: usize| {
            let col = &data.s[k];
            SeriesView { avg: &col.avg, lo: Some(&col.lo), hi: Some(&col.hi), step: inner.series[k].step, band: inner.series[k].band }
        };

        // ---- Y-aksen ----
        let fixed = (inner.opts.range)();
        let mut dr: Option<(f64, f64)> = None;
        for &k in &visible {
            if k >= data.s.len() {
                continue;
            }
            let map = inner.series[k].map.clone();
            if let Some((a, b)) = math::window_range(&data.t, &sv(k), t0, t_now, gap, &*map) {
                dr = Some(dr.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
            }
            if !imp.paused.get()
                && let Some(v) = view.latest_value(&inner.series[k].key, real_now).map(|v| map(v)).filter(|v| v.is_finite())
            {
                dr = Some(dr.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
            }
        }
        // Temperaturgrafene har én felles regel og følger ikke «Tilpass».
        let axis = if inner.opts.temp { math::temp_axis(fixed, F::fahrenheit()) } else { math::Axis { pref: fixed, hard: inner.opts.hard, min_span: (inner.opts.min_span)(), auto: (inner.opts.auto)() } };
        let auto = axis.auto;
        let limits = (inner.opts.limits)();
        let limit_vals: Vec<f64> = limits.iter().map(|l| l.v).collect();
        let want = math::y_axis(dr, &axis, &limit_vals);
        // Aksen pumper ikke: utvides straks, krymper først etter en stund. Ny
        // enhet, skala, serie eller tidsvindu begynner på nytt.
        let yr = {
            let hold_key = format!("{auto}|{}|{visible:?}|{win}", (inner.opts.unit)());
            let mut h = imp.hold.borrow_mut();
            if h.0 != hold_key {
                h.0 = hold_key;
                h.1.reset();
            }
            if dr.is_some() { h.1.apply(want, real_now) } else { h.1.idle(want) }
        };
        let dec = math::axis_decimals(yr.step);
        let lw = |v: f64| Self::text_size(&self.layout(&F::num(Some(v), dec), false)).0;
        let label_w = Self::text_size(&self.layout(&F::num(Some(8888.0), 0), false)).0.max(lw(yr.hi)).max(lw(yr.lo));
        let cap_h = Self::text_size(&self.layout("0", false)).1;
        let pad_l = label_w.ceil() + 12.0;
        let (pad_r, pad_t) = (8.0f32, 8.0f32);
        let pad_b = cap_h + 8.0;
        let pw = (w - pad_l - pad_r).floor();
        let ph = (h - pad_t - pad_b).floor();
        if pw < 20.0 || ph < 20.0 {
            return;
        }
        if (g_old.pw - pw).abs() > 0.5
            && let Some(f) = imp.on_points.borrow().as_ref()
        {
            f(pw as usize);
        }
        imp.geom.set(Geom { pad_l, pw, t0, span });
        let x_of = move |t: i64| pad_l + ((t - t0) as f64 / span as f64 * pw as f64) as f32;
        let y_of = move |v: f64| math::y_position(v, yr, pad_t, ph);

        // ---- Bakgrunn: rutenett, aksetall og tidsaksen ----
        let off = utc_offset(t_now);
        let now_label = if imp.paused.get() {
            trf!("pauset {verdi}", verdi = F::time(F::local(imp.paused_at.get(), off), win <= 300))
        } else {
            tr!("nå").to_string()
        };
        let ticks = if win <= 300 { math::rel_ticks(win, t_now) } else { math::clock_ticks(t0, t_now, pw, off) };
        let tick_xs: Vec<i32> = ticks.iter().map(|t| x_of(t.t).round() as i32).collect();
        let key = format!("{w}x{h}|{:?}|{dec}|{now_label}|{:?}|{}|{:?}|{}|{limits:?}", (yr.lo, yr.hi, yr.step), tick_xs, ticks.iter().map(|t| t.label.as_str()).collect::<String>(), c.grid, c.hc);
        let cached = imp.bg.borrow().as_ref().filter(|b| b.key == key).map(|b| b.node.clone());
        let bg = match cached {
            Some(n) => Some(n),
            None => {
                let b = gtk::Snapshot::new();
                self.draw_background(&b, c, &yr, dec, &ticks, &tick_xs, &now_label, pad_l, pad_t, pw, ph, w, h, &y_of);
                self.draw_limits(&b, c, &yr, &limits, pad_l, pad_t, pw, ph, &y_of);
                let node = b.to_node();
                if let Some(n) = &node {
                    *imp.bg.borrow_mut() = Some(BgCache { key, node: n.clone() });
                }
                node
            }
        };
        if let Some(n) = bg {
            s.append_node(&n);
        }

        // ---- Hull ----
        let first = view.first(data).filter(|f| *f <= t_now);
        let mut holes = math::holes(&data.t, start, t0, t_now, gap, if start < data.t.len() { first.map(|f| f.max(data.t[start] - if view.step_ms() > 1000 { view.step_ms() / 2 } else { 0 })) } else { None });
        let stopped = (inner.opts.stopped)();
        if stopped.is_some() {
            // Siste punkt med en verdi i en av seriene som vises.
            let last = visible
                .iter()
                .filter(|&&k| k < data.s.len())
                .filter_map(|&k| data.s[k].avg.iter().rposition(Option::is_some).and_then(|i| data.t.get(i).copied()))
                .max();
            match last {
                Some(l) if l < t_now - gap.min(3_000) && l > t0 => holes.push((l, t_now, HoleKind::Tail)),
                _ => {}
            }
        }
        for (a, b, kind) in &holes {
            let xa = x_of(*a).max(pad_l).round();
            let xb = x_of(*b).min(pad_l + pw).round();
            if xb - xa < 1.0 {
                continue;
            }
            s.append_color(&c.hatch, &graphene::Rect::new(xa, pad_t, xb - xa, ph));
            let text = match kind {
                HoleKind::Lead if view.loading() && data.t.is_empty() => tr!("Henter historikk …").to_string(),
                HoleKind::Lead if first.is_none() => {
                    if view.error().is_some() {
                        tr!("Kunne ikke hente historikken").into()
                    } else {
                        tr!("Ingen data ennå").into()
                    }
                }
                HoleKind::Lead if win > 300 => trf!("Data fra {verdi}", verdi = math::when(first.unwrap_or(t0), real_now, off, false)),
                HoleKind::Lead if stopped.is_some() => trf!("Data fra {verdi}", verdi = F::time(F::local(first.unwrap_or(t0), off), false)),
                HoleKind::Lead => tr!("Samler data …").into(),
                HoleKind::Gap => tr!("Ingen data").into(),
                HoleKind::Tail => stopped.clone().unwrap_or_else(|| tr!("Ingen data").into()),
            };
            // Hullet etter at kortet stoppet er ofte smalt i starten: kortere tekst da.
            let mut texts = vec![text];
            if *kind == HoleKind::Tail {
                texts.push(tr!("Svarer ikke").into());
            }
            for t in texts {
                let l = self.layout(&t, true);
                if xb - xa > Self::text_size(&l).0 + 16.0 {
                    Self::put(s, &l, xa + (xb - xa) / 2.0, pad_t + ph / 2.0, 0.5, &c.dim);
                    break;
                }
            }
        }

        // ---- Seriene ----
        s.push_clip(&graphene::Rect::new(pad_l, pad_t - 2.0, pw, ph + 4.0));
        let fill_on = (inner.opts.fill)() && yr.lo <= fixed.0 + 1e-9;
        let hc = c.hc;
        {
            let paint = crate::widgets::Painter::new(s, &graphene::Rect::new(pad_l - 2.0, pad_t - 4.0, pw + 4.0, ph + 8.0));
            for &k in &visible {
                if k >= data.s.len() {
                    continue;
                }
                let def = &inner.series[k];
                let col = c.series(def.color);
                let map = def.map.clone();
                let m = Mapping { x: &x_of, y: &y_of, val: &*map };
                let segs = math::segments(&data.t, &sv(k), start, t_now, gap, &m, pw);
                for seg in &segs {
                    if seg.line.is_empty() {
                        continue;
                    }
                    let has_band = def.band && seg.band.len() >= 2;
                    if has_band {
                        // Svakere der båndet er høyt (se math::band_level).
                        let base = if hc { 0.28 } else { 0.2 };
                        for (a, b, lvl) in math::band_runs(&seg.band, ph) {
                            let part = &seg.band[a..=b];
                            paint.fill(part.iter().map(|pt| (pt.0, pt.1)).chain(part.iter().rev().map(|pt| (pt.0, pt.2))), &alpha(col, base * lvl));
                        }
                    }
                    if def.fill && fill_on && !has_band && seg.line.len() >= 2 {
                        let base = pad_t + ph;
                        let first = (seg.line[0].0, base);
                        let last = (seg.line[seg.line.len() - 1].0, base);
                        paint.fill(std::iter::once(first).chain(seg.line.iter().copied()).chain(std::iter::once(last)), &alpha(col, if hc { 0.22 } else { 0.14 }));
                    }
                    let width = (if def.dashed { 1.25 } else { 1.75 }) * if hc { 1.4 } else { 1.0 };
                    let dash: Option<&[f32]> = if def.dashed { Some(&[4.0, 4.0]) } else { None };
                    if seg.line.len() == 1 {
                        // Ett enkelt punkt: en kort strek, så det synes.
                        let (x, y) = seg.line[0];
                        paint.stroke([(x - 2.0, y), (x + 2.0, y)], width, !def.step, dash, &col);
                    } else {
                        paint.stroke(seg.line.iter().copied(), width, !def.step, dash, &col);
                    }
                }
            }
        }
        s.pop();

        // ---- Markøren ----
        let own = imp.hover_x.get().filter(|x| *x >= pad_l && *x <= pad_l + pw);
        let th = match own {
            Some(x) => Some(t0 + (((x - pad_l) / pw) as f64 * span as f64) as i64),
            None => inner.sync.as_ref().filter(|sy| (sy.on)()).and_then(|sy| sy.t.get()),
        };
        let Some(th) = th else { return };
        let best = math::nearest(&data.t, start, th, t_now, 3000.max(gap / 2).max(view.step_ms()));
        let Some(bi) = best else {
            // Over et hull: si hvorfor det er tomt.
            if own.is_some()
                && let Some((_, _, kind)) = holes.iter().find(|(a, b, _)| th >= *a && th <= *b)
            {
                let lines = match kind {
                    HoleKind::Gap => vec![tr!("Ingen data").to_string(), (inner.opts.gap_reason)()],
                    HoleKind::Tail => vec![stopped.clone().unwrap_or_else(|| tr!("Ingen data").into())],
                    HoleKind::Lead => match first {
                        Some(f) => vec![trf!("Historikken begynner {verdi}", verdi = math::when(f, real_now, off, false))],
                        None => vec![tr!("Ingen data ennå").into()],
                    },
                };
                self.draw_tip(s, c, own.unwrap_or(0.0), pad_t, pw + pad_l, &lines.into_iter().map(|l| (None, l)).collect::<Vec<_>>());
            }
            return;
        };
        let x = x_of(data.t[bi]).round();
        let mut yy = pad_t;
        while yy < pad_t + ph {
            s.append_color(&c.axis, &graphene::Rect::new(x, yy, 1.0, 2.0_f32.min(pad_t + ph - yy)));
            yy += 5.0;
        }
        for &k in &visible {
            if k >= data.s.len() {
                continue;
            }
            let def = &inner.series[k];
            let Some(v) = data.s[k].avg.get(bi).copied().flatten() else { continue };
            let p = gsk::PathBuilder::new();
            p.add_circle(&graphene::Point::new(x + 0.5, y_of((def.map)(v))), 3.0);
            s.append_fill(&p.to_path(), gsk::FillRule::Winding, &c.series(def.color));
        }
        if own.is_none() {
            return;
        }
        // Verktøytips (bare på grafen pekeren er over): tidspunkt og verdier.
        let unit = (inner.opts.unit)();
        let d = inner.opts.decimals;
        let fmt = |def: &SeriesDef, v: f64| match &def.fmt {
            Some(f) => f(v),
            None => F::with_unit(F::num(Some(v), d), &unit),
        };
        let mut lines: Vec<(Option<gdk::RGBA>, String)> = vec![(None, math::hover_head(data.t[bi], t_now, win, view.step_ms(), real_now, off))];
        for &k in &visible {
            if k >= data.s.len() {
                continue;
            }
            let def = &inner.series[k];
            let Some(raw) = data.s[k].avg.get(bi).copied().flatten() else { continue };
            let v = (def.map)(raw);
            let lo = if def.band { data.s[k].lo.get(bi).copied().flatten().map(|x| (def.map)(x)) } else { None };
            let hi = if def.band { data.s[k].hi.get(bi).copied().flatten().map(|x| (def.map)(x)) } else { None };
            let range = match (lo, hi) {
                (Some(a), Some(b)) if b - a > v.abs() * 0.005 => format!(" ({}–{})", F::num(Some(a), d), F::num(Some(b), d)),
                _ => String::new(),
            };
            lines.push((Some(c.series(def.color)), format!("{}: {}{range}", def.label, fmt(def, v))));
        }
        self.draw_tip(s, c, x, pad_t, pad_l + pw, &lines);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_background(&self, s: &gtk::Snapshot, c: &Colors, yr: &Scale, dec: usize, ticks: &[math::Tick], tick_xs: &[i32], now_label: &str, pad_l: f32, pad_t: f32, pw: f32, ph: f32, w: f32, h: f32, y_of: &dyn Fn(f64) -> f32) {
        let pad_r = w - pad_l - pw;
        for v in math::y_ticks(yr) {
            let y = y_of(v).round();
            s.append_color(&c.grid, &graphene::Rect::new(pad_l, y, pw, 1.0));
            let l = self.layout(&F::num(Some(v), dec), false);
            Self::put(s, &l, pad_l - 6.0, y + 0.5, 1.0, &c.axis);
        }
        let y_text = h - Self::text_size(&self.layout("0", false)).1 / 2.0 - 2.0;
        let nl = self.layout(now_label, false);
        Self::put(s, &nl, w - pad_r, y_text, 1.0, &c.axis);
        let reserve = Self::text_size(&nl).0 + 12.0;
        for (t, x) in ticks.iter().zip(tick_xs) {
            let x = *x as f32;
            if x < pad_l + 10.0 || x > pad_l + pw {
                continue;
            }
            s.append_color(&c.grid, &graphene::Rect::new(x, pad_t, 1.0, ph));
            let l = self.layout(&t.label, false);
            let tw = Self::text_size(&l).0;
            // Merket står ikke inntil «nå» og ikke utenfor venstre kant.
            if x + tw / 2.0 > w - pad_r - reserve || x - tw / 2.0 < 0.0 {
                continue;
            }
            Self::put(s, &l, x + 0.5, y_text, 0.5, &c.axis);
        }
    }

    /// Kritiske grenser (nedklokking, avstenging) som stiplede streker med
    /// navnet over til høyre, bare når de er innenfor aksen. En tekst som
    /// ville truffet en annen, skjules (streken står).
    #[allow(clippy::too_many_arguments)]
    fn draw_limits(&self, s: &gtk::Snapshot, c: &Colors, yr: &Scale, limits: &[LimitLine], pad_l: f32, pad_t: f32, pw: f32, ph: f32, y_of: &dyn Fn(f64) -> f32) {
        if limits.is_empty() {
            return;
        }
        let ys: Vec<Option<f32>> = limits.iter().map(|l| (l.v >= yr.lo && l.v <= yr.hi).then(|| y_of(l.v).round())).collect();
        let text_h = Self::text_size(&self.layout("0", false)).1;
        let show = math::limit_labels(&ys, text_h);
        for (i, l) in limits.iter().enumerate() {
            let Some(y) = ys[i] else { continue };
            let col = if l.level == "danger" { c.danger } else { c.warn };
            let line = alpha(col, if c.hc { 0.95 } else { 0.7 });
            let mut x = pad_l;
            while x < pad_l + pw {
                s.append_color(&line, &graphene::Rect::new(x, y, 5.0f32.min(pad_l + pw - x), 1.0));
                x += 9.0;
            }
            if !show[i] {
                continue;
            }
            let t = self.layout(&l.label, false);
            let (tw, th) = Self::text_size(&t);
            // Over streken, eller under når den står helt øverst.
            let ty = if y - th - 2.0 < pad_t { y + 2.0 + th / 2.0 } else { y - 1.0 - th / 2.0 };
            if ty + th / 2.0 > pad_t + ph || tw + 12.0 > pw {
                continue;
            }
            let bx = pad_l + pw - tw - 6.0;
            let p = gsk::PathBuilder::new();
            p.add_rounded_rect(&gsk::RoundedRect::from_rect(graphene::Rect::new(bx - 3.0, (ty - th / 2.0).round(), tw + 6.0, th), 3.0));
            s.append_fill(&p.to_path(), gsk::FillRule::Winding, &alpha(c.label_bg, 0.85));
            Self::put(s, &t, pad_l + pw - 6.0, ty, 1.0, if l.level == "danger" { &c.danger_text } else { &c.warn_text });
        }
    }

    fn draw_tip(&self, s: &gtk::Snapshot, c: &Colors, x: f32, top: f32, right: f32, lines: &[(Option<gdk::RGBA>, String)]) {
        if lines.is_empty() {
            return;
        }
        let ls: Vec<pango::Layout> = lines.iter().map(|l| self.layout(&l.1, true)).collect();
        let lh = ls.iter().map(|l| Self::text_size(l).1).fold(0.0, f32::max) + 4.0;
        let tw = ls.iter().zip(lines).map(|(l, ln)| Self::text_size(l).0 + if ln.0.is_some() { 22.0 } else { 9.0 }).fold(0.0, f32::max) + 9.0;
        let th = lines.len() as f32 * lh + 10.0;
        let left = self.imp().geom.get().pad_l + 2.0;
        let mut bx = x + 10.0;
        if bx + tw > right {
            bx = x - 10.0 - tw;
        }
        let bx = bx.max(left).round();
        let by = (top + 4.0).round();
        let p = gsk::PathBuilder::new();
        p.add_rounded_rect(&gsk::RoundedRect::from_rect(graphene::Rect::new(bx, by, tw, th), 6.0));
        s.append_fill(&p.to_path(), gsk::FillRule::Winding, &c.tip_bg);
        for (i, (l, (col, _))) in ls.iter().zip(lines).enumerate() {
            let yy = by + 5.0 + lh * i as f32 + lh / 2.0;
            let tx = if let Some(col) = col {
                s.append_color(col, &graphene::Rect::new(bx + 9.0, (yy - 1.0).round(), 8.0, 2.0));
                bx + 22.0
            } else {
                bx + 9.0
            };
            let fg = if i == 0 { alpha(c.tip_fg, 0.75) } else { c.tip_fg };
            Self::put(s, l, tx, yy, 0.0, &fg);
        }
    }
}

// ---------------------------------------------------------------- Forklaringen

mod mark_imp {
    use super::*;

    #[derive(Default)]
    pub struct Mark {
        pub color: Cell<Option<gdk::RGBA>>,
        pub dashed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Mark {
        const NAME: &'static str = "RiggLegendMark";
        type Type = super::LegendMark;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("lg");
            klass.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Mark {}

    impl WidgetImpl for Mark {
        fn measure(&self, o: gtk::Orientation, _f: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (12, 12, -1, -1),
                _ => (3, 3, -1, -1),
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let Some(c) = self.color.get() else { return };
            let w = self.obj();
            let y = (w.height() as f32 / 2.0 - 1.0).round();
            if self.dashed.get() {
                s.append_color(&c, &graphene::Rect::new(0.0, y, 4.0, 2.0));
                s.append_color(&c, &graphene::Rect::new(7.0, y, 4.0, 2.0));
            } else {
                s.append_color(&c, &graphene::Rect::new(0.0, y - 0.5, 12.0, 3.0));
            }
        }
    }
}

glib::wrapper! {
    /// Den korte streken i seriens farge (stiplet for grenser).
    pub struct LegendMark(ObjectSubclass<mark_imp::Mark>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LegendMark {
    pub fn new(dashed: bool) -> LegendMark {
        let m: LegendMark = glib::Object::new();
        m.imp().dashed.set(dashed);
        m.set_valign(gtk::Align::Center);
        m
    }

    pub fn set_color(&self, c: gdk::RGBA) {
        self.imp().color.set(Some(c));
        self.queue_draw();
    }
}

// ---------------------------------------------------------------- Panelet

/// Hva panelet viser i overskriften.
pub struct PanelOpts {
    pub title: String,
    pub series: Vec<SeriesDef>,
    pub window: i64,
    pub chart: ChartOpts,
    pub sync: Option<Rc<CursorSync>>,
    /// «min · maks» i vinduet for første serie.
    pub stats: bool,
    /// Siste verdi for første serie.
    pub now: bool,
    /// Ekstra i overskriften (tidsvindu og lukk på Sensorer).
    pub extra: Option<gtk::Widget>,
    /// Flatt panel uten ramme (under en rad på Sensorer).
    pub flat: bool,
}

/// Et graf-panel: tittel, forklaring (klikk skjuler en serie), «På pause ·
/// 14:32:10», min/maks, nå-verdi, pauseknapp og grafen.
pub struct ChartPanel {
    pub root: gtk::Box,
    pub chart: Chart,
    pub view: Rc<View>,
    pub title: String,
    marks: Vec<LegendMark>,
    stats: gtk::Label,
    now: gtk::Label,
    now_unit: gtk::Label,
    paused_tag: gtk::Label,
    pause_btn: gtk::Button,
    pause_icon: Icon,
    pause_text: gtk::Label,
    show_stats: bool,
    show_now: bool,
    /// Grafer: alle står på pause, og toppfeltet sier det. Da holder ikonet.
    quiet_pause: Cell<bool>,
    on_pause: crate::widgets::Slot<dyn Fn(bool)>,
}

impl ChartPanel {
    pub fn new(app: &Rc<App>, o: PanelOpts) -> Rc<ChartPanel> {
        let keys: Vec<String> = o.series.iter().map(|s| s.key.clone()).collect();
        let view = View::new(app, keys, o.window, 800);
        let chart = Chart::new(app, view.clone(), o.series.clone(), o.chart.clone(), o.sync.clone());
        let root = crate::widgets::vbox(0, &["group", "chart-card"]);
        if o.flat {
            root.add_css_class("flat-chart");
        }
        let head = hbox(12, &["group-head"]);
        let title = label(&o.title, &["group-title"]);
        title.set_accessible_role(gtk::AccessibleRole::Heading);
        title.set_ellipsize(pango::EllipsizeMode::End);
        head.append(&title);
        let legend = hbox(8, &["legend"]);
        let colors = Colors::from_app(app);
        let mut marks = Vec::new();
        let mut toggles: Vec<gtk::ToggleButton> = Vec::new();
        if o.series.len() > 1 {
            for s in &o.series {
                let b = gtk::ToggleButton::new();
                b.set_active(true);
                b.add_css_class("lg-btn");
                b.add_css_class("flat");
                let bx = hbox(5, &[]);
                let m = LegendMark::new(s.dashed);
                m.set_color(colors.series(s.color));
                bx.append(&m);
                // Med få serier kuttes ikke navnene: hele forklaringen
                // forsvinner heller når hodet er for smalt (AllOrNothing
                // under), så tittelen står. Med mange (diskene) kuttes de.
                let l = gtk::Label::new(Some(&s.label));
                if o.series.len() > 5 {
                    l.set_ellipsize(pango::EllipsizeMode::End);
                }
                bx.append(&l);
                b.set_child(Some(&bx));
                b.set_tooltip_text(Some(&trf!("Skjul {verdi}", verdi = lower_name(&s.label))));
                legend.append(&b);
                marks.push(m);
                toggles.push(b);
            }
        }
        if o.series.len() > 5 {
            head.append(&legend);
        } else {
            head.append(&crate::widgets::fixed::AllOrNothing::new(&legend));
        }
        let aside = hbox(8, &["aside"]);
        aside.set_hexpand(true);
        aside.set_halign(gtk::Align::End);
        let paused_tag = label("", &["tag", "paused-tag"]);
        paused_tag.set_visible(false);
        aside.append(&paused_tag);
        // «min · maks» helt eller ikke: i smale grafer forsvinner den i stedet
        // for å bli «…0,0».
        let stats = label("", &["stats", "num"]);
        stats.set_visible(o.stats);
        aside.append(&crate::widgets::fixed::AllOrNothing::new(&stats));
        let nowb = hbox(2, &["now"]);
        let now = label("–", &["now-v", "num"]);
        let now_unit = label("", &["unit"]);
        now.set_valign(gtk::Align::Baseline);
        now_unit.set_valign(gtk::Align::Baseline);
        nowb.append(&now);
        nowb.append(&now_unit);
        nowb.set_visible(o.now);
        aside.append(&nowb);
        if let Some(e) = &o.extra {
            aside.append(e);
        }
        let pause_btn = gtk::Button::new();
        pause_btn.add_css_class("flat");
        pause_btn.add_css_class("sm");
        pause_btn.add_css_class("pause-btn");
        let pbx = hbox(4, &[]);
        let pause_icon = Icon::new("pause", 14);
        let pause_text = label(tr!("Fortsett"), &[]);
        pause_text.set_visible(false);
        pbx.append(&pause_icon);
        pbx.append(&pause_text);
        pause_btn.set_child(Some(&pbx));
        crate::widgets::tip(&pause_btn, tr!("Pause"), Some(tr!("Mellomrom")));
        pause_btn.update_property(&[gtk::accessible::Property::Label(tr!("Pause"))]);
        aside.append(&pause_btn);
        head.append(&aside);
        root.append(&head);
        let body = crate::widgets::vbox(0, &["group-body", "chart-body"]);
        body.set_vexpand(true);
        body.append(&chart);
        root.append(&body);
        root.set_vexpand(true);
        root.update_property(&[gtk::accessible::Property::Label(&o.title)]);
        let p = Rc::new(ChartPanel {
            root,
            chart,
            view,
            title: o.title.clone(),
            marks,
            stats,
            now,
            now_unit,
            paused_tag,
            pause_btn,
            pause_icon,
            pause_text,
            show_stats: o.stats,
            show_now: o.now,
            quiet_pause: Cell::new(false),
            on_pause: RefCell::new(None),
        });
        p.set_aria();
        // Forklaringen: minst én serie skal synes.
        for (i, b) in toggles.iter().enumerate() {
            let w = Rc::downgrade(&p);
            let all = toggles.clone();
            let name = lower_name(&o.series[i].label);
            b.connect_toggled(move |b| {
                let Some(p) = w.upgrade() else { return };
                if !b.is_active() && all.iter().filter(|x| x.is_active()).count() == 0 {
                    b.set_active(true);
                    return;
                }
                p.chart.set_hidden(i, !b.is_active());
                let tip = if b.is_active() { trf!("Skjul {verdi}", verdi = name) } else { trf!("Vis {verdi}", verdi = name) };
                b.set_tooltip_text(Some(&tip));
                p.render_numbers();
            });
        }
        let w = Rc::downgrade(&p);
        p.pause_btn.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                let next = !p.chart.paused();
                p.set_paused(next);
            }
        });
        let keys = gtk::EventControllerKey::new();
        let w = Rc::downgrade(&p);
        keys.connect_key_pressed(move |_, k, _, _| {
            if k == gdk::Key::space
                && let Some(p) = w.upgrade()
            {
                let next = !p.chart.paused();
                p.set_paused(next);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        p.chart.add_controller(keys);
        let w = Rc::downgrade(&p);
        p.chart.connect_points(move |n| {
            if let Some(p) = w.upgrade() {
                p.view.set_points(n);
            }
        });
        let w = Rc::downgrade(&p);
        *p.view.on_change.borrow_mut() = Some(Box::new(move || {
            if let Some(p) = w.upgrade() {
                p.chart.data_changed();
            }
        }));
        p.render_numbers();
        p
    }

    fn set_aria(&self) {
        let t = trf!("Graf: {navn}, {status}. Mellomrom setter på pause.", navn = self.title, status = window_label(self.view.window()));
        self.chart.update_property(&[gtk::accessible::Property::Label(&t)]);
    }

    /// Enheten og desimalene grafen viser (eksporten bruker de samme).
    pub fn unit(&self) -> String {
        self.chart.with(|i| (i.opts.unit)()).unwrap_or_default()
    }

    pub fn decimals(&self) -> usize {
        self.chart.with(|i| i.opts.decimals).unwrap_or(0)
    }

    /// Kalles når pausen endres (Grafer holder knappen «Pause alle» i takt).
    pub fn connect_pause(&self, f: impl Fn(bool) + 'static) {
        *self.on_pause.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_paused(&self, p: bool) {
        if self.chart.paused() == p {
            return;
        }
        self.chart.set_paused(p);
        self.render_pause();
        if let Some(f) = self.on_pause.borrow().as_ref() {
            f(p);
        }
        self.render_numbers();
    }

    /// Når hele siden står på pause (Grafer), sier toppfeltet det: da viser
    /// panelet bare ikonet, så overskriften ikke blir trang.
    pub fn set_quiet_pause(&self, q: bool) {
        if self.quiet_pause.replace(q) != q {
            self.render_pause();
        }
    }

    fn render_pause(&self) {
        let p = self.chart.paused();
        let loud = p && !self.quiet_pause.get();
        self.pause_icon.set_icon(if p { "play" } else { "pause" });
        self.pause_text.set_visible(loud);
        crate::widgets::class(&self.pause_btn, "on", p);
        crate::widgets::tip(&self.pause_btn, if p { tr!("Fortsett og hopp til nå") } else { tr!("Pause") }, Some(tr!("Mellomrom")));
        self.pause_btn.update_property(&[gtk::accessible::Property::Label(if p { tr!("Fortsett") } else { tr!("Pause") })]);
        self.paused_tag.set_visible(loud);
        if p {
            let at = self.chart.paused_at();
            set_text(&self.paused_tag, &trf!("På pause · {verdi}", verdi = F::time(F::local(at, utc_offset(at)), true)));
        }
    }

    pub fn set_window(self: &Rc<Self>, sec: i64) {
        if self.chart.paused() {
            self.set_paused(false);
        }
        self.view.set_window(sec);
        self.set_aria();
        self.chart.queue_draw();
    }

    pub fn set_colors(&self, c: &Colors) {
        for (m, s) in self.marks.iter().zip(self.chart.series()) {
            m.set_color(c.series(s.color));
        }
        self.chart.set_colors(c.clone());
    }

    /// Min og maks i vinduet (fra båndet når det finnes) og siste verdi, for
    /// første synlige serie.
    pub fn render_numbers(&self) {
        if !self.show_stats && !self.show_now {
            return;
        }
        let series = self.chart.series();
        let Some(k) = (0..series.len()).find(|i| !self.chart.is_hidden(*i)) else { return };
        let def = &series[k];
        let paused = self.chart.paused();
        let snap = self.chart.imp().snap.borrow();
        let live = self.view.data.borrow();
        let data: &Data = if paused { snap.as_ref().unwrap_or(&live) } else { &live };
        let Some(col) = data.s.get(k) else { return };
        let (d, unit) = self.chart.with(|i| (i.opts.decimals, (i.opts.unit)())).unwrap_or((0, String::new()));
        let t_now = if paused { self.chart.paused_at() } else { rigg_core::now_ms() };
        let t0 = t_now - self.view.window() * 1000;
        if self.show_stats {
            let sv = SeriesView { avg: &col.avg, lo: Some(&col.lo), hi: Some(&col.hi), step: def.step, band: def.band };
            let mut range = math::window_range(&data.t, &sv, t0, t_now, self.view.gap_ms(), &*def.map);
            if !paused
                && let Some(v) = self.view.latest_value(&def.key, t_now).map(|v| (def.map)(v)).filter(|v| v.is_finite())
            {
                range = Some(range.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
            }
            let text = match range {
                Some((lo, hi)) => trf!("min {min} · maks {maks}", min = F::num(Some(lo), d), maks = F::num(Some(hi), d)),
                None => String::new(),
            };
            set_text(&self.stats, &text);
        }
        if self.show_now {
            // Verdien nå (ikke snittet i den åpne bøtta), eller det siste punktet på pause.
            let live = if paused { None } else { self.view.app.upgrade().and_then(|a| a.value(&def.key)) };
            let last = live.or_else(|| col.avg.last().copied().flatten()).map(|v| (def.map)(v));
            let text = match (last, &def.fmt) {
                (Some(v), Some(f)) => f(v),
                (Some(v), None) => F::num(Some(v), d),
                (None, _) => F::DASH.into(),
            };
            match text.rsplit_once(F::NNBSP) {
                Some((n, u)) if def.fmt.is_some() => {
                    set_text(&self.now, n);
                    set_text(&self.now_unit, &format!("{}{u}", F::NNBSP));
                }
                _ => {
                    set_text(&self.now, &text);
                    set_text(&self.now_unit, if last.is_some() && !unit.is_empty() { &unit } else { "" });
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Settet

/// Grafene på en side. Tar imot prøvene, temaet, innstillingene og ny
/// tilkobling én gang, og sender videre til panelene som lever.
pub struct ChartSet {
    app: Weak<App>,
    panels: RefCell<Vec<Rc<ChartPanel>>>,
    visible: Cell<bool>,
    gpu: RefCell<Option<String>>,
    on_gpu: RefCell<Option<Box<dyn Fn()>>>,
}

impl ChartSet {
    pub fn new(app: &Rc<App>) -> Rc<ChartSet> {
        let set = Rc::new(ChartSet { app: Rc::downgrade(app), panels: RefCell::new(vec![]), visible: Cell::new(false), gpu: RefCell::new(app.gpu_id()), on_gpu: RefCell::new(None) });
        let w = Rc::downgrade(&set);
        app.on_telemetry.connect(move |_| {
            let Some(s) = w.upgrade() else { return };
            let Some(app) = s.app.upgrade() else { return };
            for p in s.panels() {
                p.view.live(&app);
            }
            if s.visible.get() {
                for p in s.panels() {
                    if !p.chart.paused() {
                        p.render_numbers();
                    }
                }
            }
        });
        let w = Rc::downgrade(&set);
        app.on_theme.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.recolor();
            }
        });
        let w = Rc::downgrade(&set);
        app.on_settings.connect(move |keys| {
            let Some(s) = w.upgrade() else { return };
            if keys.iter().any(|k| matches!(k.as_str(), "unit" | "chartScale" | "chartFill" | "chartSync" | "netUnit" | "decimals" | "clock" | "motion" | "highContrast")) {
                for p in s.panels() {
                    p.chart.imp().bg.replace(None);
                    p.chart.schedule();
                    p.chart.queue_draw();
                    p.render_numbers();
                }
            }
            if keys.iter().any(|k| k == "highContrast") {
                s.recolor();
            }
        });
        let w = Rc::downgrade(&set);
        app.on_conn.connect(move |c| {
            if *c == Conn::Connected
                && let Some(s) = w.upgrade()
            {
                for p in s.panels() {
                    p.view.load();
                }
            }
        });
        let w = Rc::downgrade(&set);
        app.on_devices.connect(move |_| {
            let Some(s) = w.upgrade() else { return };
            let Some(app) = s.app.upgrade() else { return };
            let id = app.gpu_id();
            if *s.gpu.borrow() != id {
                *s.gpu.borrow_mut() = id;
                if let Some(f) = s.on_gpu.borrow().as_ref() {
                    f();
                }
            }
        });
        set
    }

    fn panels(&self) -> Vec<Rc<ChartPanel>> {
        self.panels.borrow().clone()
    }

    fn recolor(&self) {
        let Some(app) = self.app.upgrade() else { return };
        let c = Colors::from_app(&app);
        for p in self.panels() {
            p.set_colors(&c);
        }
    }

    pub fn add(&self, p: &Rc<ChartPanel>) {
        self.panels.borrow_mut().push(p.clone());
    }

    pub fn clear(&self) {
        self.panels.borrow_mut().clear();
    }

    pub fn remove(&self, p: &Rc<ChartPanel>) {
        self.panels.borrow_mut().retain(|x| !Rc::ptr_eq(x, p));
    }

    pub fn list(&self) -> Vec<Rc<ChartPanel>> {
        self.panels()
    }

    /// Siden vises eller skjules (tallene i overskriftene oppdateres bare når den vises).
    pub fn set_visible(&self, v: bool) {
        self.visible.set(v);
        if v {
            self.recolor();
            for p in self.panels() {
                p.render_numbers();
            }
        }
    }

    /// Kalles når et annet skjermkort er valgt (grafene bygges på nytt).
    pub fn connect_gpu(&self, f: impl Fn() + 'static) {
        *self.on_gpu.borrow_mut() = Some(Box::new(f));
    }
}

// ---------------------------------------------------------------- Grafene som finnes

/// En graf fra lista (demoens `CHART_DEFS`). `fields`: (nøkkel eller felt
/// på kortet, navn, farge, slag).
pub struct ChartDef {
    pub id: &'static str,
    pub title: &'static str,
}

impl ChartDef {
    pub fn localized_title(&self) -> &'static str {
        match self.id {
            "gpuTemp" => tr!("GPU-temperatur"),
            "gpuLoad" => tr!("GPU-last"),
            "coreClock" => tr!("Kjerneklokke"),
            "memClock" => tr!("Minneklokke"),
            "power" => tr!("GPU-effekt"),
            "fan" => tr!("GPU-vifte"),
            "vram" => tr!("Videominne"),
            "cpuLoad" => tr!("CPU-last"),
            "cpuTemp" => tr!("CPU-temperatur"),
            "mem" => tr!("Minne brukt"),
            "net" => tr!("Nett"),
            "diskTemp" => tr!("Disktemperatur"),
            "diskIo" => tr!("Disk-I/O"),
            _ => self.title,
        }
    }
}

/// Rekkefølgen på Grafer (og i Innstillinger).
pub const DEFS: [ChartDef; 13] = [
    ChartDef { id: "gpuTemp", title: "GPU-temperatur" },
    ChartDef { id: "gpuLoad", title: "GPU-last" },
    ChartDef { id: "coreClock", title: "Kjerneklokke" },
    ChartDef { id: "memClock", title: "Minneklokke" },
    ChartDef { id: "power", title: "GPU-effekt" },
    ChartDef { id: "fan", title: "GPU-vifte" },
    ChartDef { id: "vram", title: "Videominne" },
    ChartDef { id: "cpuLoad", title: "CPU-last" },
    ChartDef { id: "cpuTemp", title: "CPU-temperatur" },
    ChartDef { id: "mem", title: "Minne brukt" },
    ChartDef { id: "net", title: "Nett" },
    ChartDef { id: "diskTemp", title: "Disktemperatur" },
    ChartDef { id: "diskIo", title: "Disk-I/O" },
];

/// Diskene som har temperatur i siste prøve: (nøkkel, navn). Disktemperatur
/// får én serie per disk (runde 6A); Grafer bygger grafen på nytt når lista endres.
pub fn disk_temp_series(app: &App) -> Vec<(String, String)> {
    let Some(t) = app.telemetry() else { return vec![] };
    t.sys.disks.iter().filter(|d| d.temp.is_some()).map(|d| (format!("disk/{}/temp", d.id), d.name.clone())).collect()
}

/// Øvre grense for en MB/s-graf: høyeste målte verdi for nøklene (min og
/// maks fra tjenesten, eller siste minutt), med litt luft, minst `floor` MB/s.
pub fn mbps_top(app: &App, keys: &[String], floor: f64) -> f64 {
    let mut top: f64 = 0.0;
    {
        let d = app.data.borrow();
        for k in keys {
            top = top.max(d.stats["stats"][k.as_str()]["max"].as_f64().unwrap_or(0.0));
        }
    }
    for k in keys {
        top = top.max(app.recent(k).iter().map(|p| p.1).fold(0.0, f64::max));
    }
    (top / 1e6 * 1.15).max(floor)
}

/// Seriefargene til disktemperaturene, i rekkefølge.
const DISK_COLORS: [&str; 6] = ["--c-temp", "--c-clock", "--c-mem", "--c-power", "--c-memctl", "--c-load"];

pub fn def_title(id: &str) -> &'static str {
    DEFS.iter().find(|d| d.id == id).map(ChartDef::localized_title).unwrap_or_else(|| tr!("Graf"))
}

pub struct DefOpts {
    pub window: i64,
    pub sync: Option<Rc<CursorSync>>,
    pub title: Option<String>,
    pub stats: bool,
    pub now: bool,
}

impl Default for DefOpts {
    fn default() -> Self {
        DefOpts { window: 60, sync: None, title: None, stats: true, now: true }
    }
}

/// Området temperaturgrafene viser (20–100 °C). Brukes med
/// [`ChartOpts::temp`]: taket er et minste, og verdier utenfor utvider aksen.
pub fn temp_range() -> (f64, f64) {
    if F::fahrenheit() { (68.0, 212.0) } else { (20.0, 100.0) }
}

/// Nett i valgt enhet: MB/s eller Mbit/s.
pub fn net_scaled(bps: f64) -> f64 {
    if F::format().net_unit == "bit" { bps * 8.0 / 1e6 } else { bps / 1e6 }
}

pub fn net_unit() -> String {
    if F::format().net_unit == "bit" { "Mbit/s".into() } else { "MB/s".into() }
}

/// Nedklokking og avstenging for kortet som vises (fra NVML), som streker i
/// temperaturgrafene, i enheten som vises.
/// `prefix`: «GPU-» når grafen også har andre temperaturer (Oversikt).
pub fn gpu_temp_limits(app: &App, prefix: &str) -> Vec<LimitLine> {
    let Some((slow, shut)) = app.with_statics(|s| (s.temp_slowdown, s.temp_shutdown)) else { return vec![] };
    let mut out = Vec::new();
    let (slow_name, shut_name) = if prefix == "GPU-" {
        (tr!("GPU-nedklokking").to_string(), tr!("GPU-avstenging").to_string())
    } else if prefix.is_empty() {
        (tr!("Nedklokking").to_string(), tr!("Avstenging").to_string())
    } else {
        (format!("{prefix}{}", tr!("Nedklokking").to_lowercase()), format!("{prefix}{}", tr!("Avstenging").to_lowercase()))
    };
    if let Some(v) = slow {
        out.push(LimitLine { v: F::temp_out(Some(v)).unwrap_or(v), label: format!("{slow_name} {}", F::temp(Some(v), 0)), level: "warn" });
    }
    if let Some(v) = shut {
        out.push(LimitLine { v: F::temp_out(Some(v)).unwrap_or(v), label: format!("{shut_name} {}", F::temp(Some(v), 0)), level: "danger" });
    }
    out
}

/// Prosent som ikke kan gå over 100 (last, minne).
pub const HARD_PCT: Option<(f64, f64)> = Some((0.0, 100.0));
/// Verdier som ikke kan bli negative (W, MHz, MiB, MB/s).
pub const HARD_POS: Option<(f64, f64)> = Some((0.0, f64::INFINITY));

/// Området, utvidet oppover når kortet oppgir en høyere grense.
fn with_cap(r: (f64, f64), cap: Option<f64>) -> (f64, f64) {
    match cap {
        Some(c) if c.is_finite() && c > r.1 => (r.0, c * 1.05),
        _ => r,
    }
}

/// Et ferdig panel for en graf i [`DEFS`] (brukes av Oversikt, Grafer og Kontroller).
pub fn def_panel(app: &Rc<App>, id: &str, o: DefOpts) -> Rc<ChartPanel> {
    let gk = |f: &str| app.gpu_key(f).unwrap_or_else(|| format!("gpu:-/{f}"));
    let wa = Rc::downgrade(app);
    // Ett felt fra de faste dataene, uten å kopiere dem (spørres ved hver tegning).
    let st = move |f: fn(&rigg_core::model::GpuStatic) -> Option<f64>| wa.upgrade().and_then(|a| a.with_statics(f)).flatten();
    let temp = |v: f64| F::temp_out(Some(v)).unwrap_or(v);
    let (series, unit, mut opts, fixed_only): (Vec<SeriesDef>, Rc<dyn Fn() -> String>, ChartOpts, bool) = match id {
        "gpuTemp" => (vec![SeriesDef::new(&gk("temp"), tr!("Temperatur"), "--c-temp").fill().map(temp)], Rc::new(|| F::temp_label().into()), ChartOpts::temp(app, temp_range), false),
        "gpuLoad" => (
            vec![SeriesDef::new(&gk("gpuUtil"), "GPU", "--c-load").fill(), SeriesDef::new(&gk("memUtil"), tr!("Minnekontroller"), "--c-memctl")],
            Rc::new(|| "%".into()),
            ChartOpts::new(app, || (0.0, 100.0)),
            true,
        ),
        "coreClock" => {
            let s = st.clone();
            (vec![SeriesDef::new(&gk("coreClock"), tr!("Kjerne"), "--c-clock").fill()], Rc::new(|| "MHz".into()), ChartOpts::new(app, move || with_cap((0.0, 2500.0), s(|x| x.core_max))), false)
        }
        "memClock" => {
            let s = st.clone();
            (vec![SeriesDef::new(&gk("memClock"), tr!("Minne"), "--c-mem").fill()], Rc::new(|| "MHz".into()), ChartOpts::new(app, move || with_cap((0.0, 8000.0), s(|x| x.mem_max))), false)
        }
        "power" => {
            let s = st.clone();
            (
                vec![SeriesDef::new(&gk("power"), tr!("Trekk"), "--c-power").fill(), SeriesDef::new(&gk("powerLimit"), tr!("Grense"), "--c-limit").limit()],
                Rc::new(|| "W".into()),
                ChartOpts::new(app, move || with_cap((0.0, 300.0), s(|x| x.power_max))),
                false,
            )
        }
        "fan" => (vec![SeriesDef::new(&gk("fan"), tr!("Vifte"), "--c-clock").fill()], Rc::new(|| "%".into()), ChartOpts::new(app, || (0.0, 100.0)), true),
        "vram" => {
            let wa = Rc::downgrade(app);
            (
                vec![SeriesDef::new(&gk("vramUsed"), tr!("Brukt"), "--c-mem").fill()],
                Rc::new(|| "MiB".into()),
                ChartOpts::new(app, move || (0.0, wa.upgrade().and_then(|a| a.gpu_sample()).and_then(|s| s.vram_total).unwrap_or(8192.0))),
                false,
            )
        }
        "cpuLoad" => (vec![SeriesDef::new("cpu/load", tr!("Total"), "--c-load").fill()], Rc::new(|| "%".into()), ChartOpts::new(app, || (0.0, 100.0)), true),
        "cpuTemp" => (vec![SeriesDef::new("cpu/temp", tr!("Temperatur (Tdie)"), "--c-temp").fill().map(temp)], Rc::new(|| F::temp_label().into()), ChartOpts::temp(app, temp_range), false),
        "mem" => (vec![SeriesDef::new("mem/usedPct", tr!("Brukt"), "--c-mem").fill()], Rc::new(|| "%".into()), ChartOpts::new(app, || (0.0, 100.0)), true),
        "diskTemp" => {
            let list = disk_temp_series(app);
            let series: Vec<SeriesDef> = if list.is_empty() {
                vec![SeriesDef::new("disk/-/temp", tr!("Ingen disk med temperatur"), "--c-temp")]
            } else {
                list.iter().enumerate().map(|(i, (k, n))| SeriesDef::new(k, n, DISK_COLORS[i % DISK_COLORS.len()]).map(temp)).collect()
            };
            (series, Rc::new(|| F::temp_label().into()), ChartOpts::temp(app, temp_range), false)
        }
        "diskIo" => (
            vec![SeriesDef::new("disk/read", tr!("Lesing"), "--c-load").fill().map(|b| b / 1e6), SeriesDef::new("disk/write", tr!("Skriving"), "--c-memctl").map(|b| b / 1e6)],
            Rc::new(|| "MB/s".into()),
            {
                let wa = Rc::downgrade(app);
                ChartOpts::new(app, move || (0.0, wa.upgrade().map(|a| mbps_top(&a, &["disk/read".into(), "disk/write".into()], 50.0)).unwrap_or(100.0)))
            },
            false,
        ),
        _ => (
            vec![SeriesDef::new("net/rx", tr!("Mottatt"), "--c-load").fill().map(net_scaled), SeriesDef::new("net/tx", tr!("Sendt"), "--c-memctl").map(net_scaled)],
            Rc::new(net_unit),
            // Foretrukket 0–1 MB/s (8 Mbit/s): mer utvides aksen av seg selv
            // (8A). Før var det 0–1000, og vanlig nett ble en strek ved null.
            ChartOpts::new(app, || if F::format().net_unit == "bit" { (0.0, 8.0) } else { (0.0, 1.0) }),
            false,
        ),
    };
    opts.unit = unit;
    opts.decimals = match id {
        "cpuLoad" | "cpuTemp" | "mem" | "diskIo" => 1,
        "net" => 2,
        _ => 0,
    };
    opts.min_span = match id {
        "diskIo" => Rc::new(|| 5.0),
        "net" => Rc::new(|| if F::format().net_unit == "bit" { 0.8 } else { 0.1 }),
        "coreClock" => Rc::new(|| 400.0),
        "memClock" => Rc::new(|| 1000.0),
        "vram" => Rc::new(|| 1024.0),
        "power" => {
            // En fjerdedel av effektgrensen (minst 40 W), så ±10 W støy ved 230 W ikke fyller grafen.
            let wa = Rc::downgrade(app);
            Rc::new(move || {
                let a = wa.upgrade();
                let lim = a.as_ref().and_then(|a| a.gpu_sample()).and_then(|s| s.power_limit).or_else(|| a.as_ref().and_then(|a| a.with_statics(|s| s.power_default)).flatten());
                (lim.unwrap_or(0.0) * 0.25).max(40.0)
            })
        }
        _ => Rc::new(|| 0.0),
    };
    if fixed_only {
        opts.auto = Rc::new(|| false);
    }
    // Ingenting kuttes (runde 8A): det faste området er det foretrukne, og
    // bare last og minne i prosent er en hard grense. Vifta har ingen, fordi
    // NVML kan melde over 100 %.
    opts.hard = match id {
        "gpuLoad" | "cpuLoad" | "mem" => HARD_PCT,
        "gpuTemp" | "cpuTemp" | "diskTemp" | "fan" => None,
        _ => HARD_POS,
    };
    if id == "gpuTemp" {
        let wa = Rc::downgrade(app);
        opts.limits = Rc::new(move || wa.upgrade().map(|a| gpu_temp_limits(&a, "")).unwrap_or_default());
    }
    if series.iter().any(|d| d.key.starts_with("gpu:")) {
        let wa = Rc::downgrade(app);
        opts.stopped = Rc::new(move || wa.upgrade().and_then(|a| gpu_stopped(&a)));
    }
    if !id.starts_with("gpu") && !matches!(id, "coreClock" | "memClock" | "power" | "fan" | "vram") {
        let wa = Rc::downgrade(app);
        opts.lag_ms = Rc::new(move || wa.upgrade().map(|a| a.settings().interval_ms as i64 + 150).unwrap_or(1150));
    }
    ChartPanel::new(app, PanelOpts { title: o.title.unwrap_or_else(|| def_title(id).into()), series, window: o.window, chart: opts, sync: o.sync, stats: o.stats, now: o.now, extra: None, flat: false })
}

/// Liten nedtrekksliste for tidsvindu (Oversikt og under en rad på Sensorer).
pub fn window_dropdown(value: i64, label_text: &str, on_pick: impl Fn(i64) + 'static) -> gtk::DropDown {
    let names: Vec<&str> = window_choices().map(|(_, title)| title).collect();
    let dd = gtk::DropDown::from_strings(&names);
    dd.add_css_class("win-select");
    // Den korte, faste listen trenger ikke et ekstra søkefelt. Behold
    // GTKs native valg/check og tastaturnavigasjon i menyen.
    dd.set_enable_search(false);
    dd.set_valign(gtk::Align::Center);
    dd.set_selected(WINDOWS.iter().position(|w| w.0 == value).unwrap_or(0) as u32);
    dd.update_property(&[gtk::accessible::Property::Label(label_text)]);
    dd.set_tooltip_text(Some(label_text));
    dd.connect_selected_notify(move |d| {
        if let Some(w) = WINDOWS.get(d.selected() as usize) {
            on_pick(w.0);
        }
    });
    dd
}

/// Setter valget i en nedtrekksliste fra [`window_dropdown`] uten å lagre på nytt.
pub fn set_window_dropdown(dd: &gtk::DropDown, value: i64) {
    let i = WINDOWS.iter().position(|w| w.0 == value).unwrap_or(0) as u32;
    if dd.selected() != i {
        dd.set_selected(i);
    }
}

// ---------------------------------------------------------------- Spaltene

mod cols_imp {
    use super::*;

    pub struct Cols {
        pub weights: RefCell<Vec<f32>>,
        pub gap: Cell<i32>,
        /// Under denne bredden står alt i én spalte.
        pub stack_below: Cell<i32>,
        pub max_cols: Cell<usize>,
        /// Minste høyde per rad.
        pub min_row: Cell<i32>,
        /// Ledig høyde fordeles på radene (grafene fyller siden).
        pub fill: Cell<bool>,
        /// Bruddpunkter: (under bredde, spalter). Det minste antallet som
        /// gjelder for bredden, vinner.
        pub breaks: RefCell<Vec<(i32, usize)>>,
        /// Siste spalte og rad får 1 px mer enn plassen, så kantlinja til
        /// høyre og under (flisene) klippes bort av beholderen.
        pub bleed: Cell<bool>,
        /// Bare hele rader: antall spalter går opp i antall barn (4 × 1,
        /// 2 × 2 eller 1 × 4 for fire kort, aldri 3 + 1).
        pub whole_rows: Cell<bool>,
        /// Avstand mellom radene når den er en annen enn mellom spaltene.
        pub row_gap: Cell<Option<i32>>,
    }

    impl Default for Cols {
        fn default() -> Self {
            Cols {
                weights: RefCell::new(vec![]),
                gap: Cell::new(8),
                stack_below: Cell::new(0),
                max_cols: Cell::new(3),
                min_row: Cell::new(0),
                fill: Cell::new(false),
                breaks: RefCell::new(vec![]),
                bleed: Cell::new(false),
                whole_rows: Cell::new(false),
                row_gap: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Cols {
        const NAME: &'static str = "RiggCols";
        type Type = super::Cols;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("cols");
        }
    }

    impl ObjectImpl for Cols {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl Cols {
        pub fn kids(&self) -> Vec<gtk::Widget> {
            let mut v = Vec::new();
            let mut c = self.obj().first_child();
            while let Some(w) = c {
                if w.is_visible() {
                    v.push(w.clone());
                }
                c = w.next_sibling();
            }
            v
        }

        /// Bruddpunktene gir det høyeste antallet spalter; det går ned til
        /// hvert barn får minst sin minste bredde (`fit.rs`), så et bredt
        /// barn aldri legger seg over naboen.
        pub fn n_cols(&self, width: i32, kids: &[gtk::Widget]) -> usize {
            let k = kids.len();
            if k == 0 {
                return 1;
            }
            if width < self.stack_below.get() {
                return 1;
            }
            let mut n = self.max_cols.get();
            for &(below, cols) in self.breaks.borrow().iter() {
                if width < below {
                    n = n.min(cols);
                }
            }
            let want = n.clamp(1, k);
            if want == 1 {
                return 1;
            }
            let mins: Vec<i32> = kids.iter().map(|w| w.measure(gtk::Orientation::Horizontal, -1).0).collect();
            let (whole, whole_rows) = (self.whole_rows.get(), crate::widgets::fit::whole_rows(k));
            crate::widgets::fit::fit_cols_where(want, &mins, |n| self.columns(width, n), |n| !whole || whole_rows(n))
        }

        pub fn row_gap(&self) -> i32 {
            self.row_gap.get().unwrap_or_else(|| self.gap.get())
        }

        /// (x, bredde) per spalte.
        pub fn columns(&self, width: i32, n: usize) -> Vec<(i32, i32)> {
            let ws = self.weights.borrow();
            let weights: Vec<f32> = if ws.len() == n { ws.clone() } else { vec![1.0; n] };
            crate::widgets::fit::columns(width, self.gap.get(), &weights)
        }

        /// Høyden per rad (naturlig, minst `min_row`).
        pub fn rows(&self, kids: &[gtk::Widget], cols: &[(i32, i32)]) -> Vec<i32> {
            let n = cols.len();
            kids.chunks(n)
                .map(|row| row.iter().enumerate().map(|(i, k)| k.measure(gtk::Orientation::Vertical, cols[i].1).1).max().unwrap_or(0).max(self.min_row.get()))
                .collect()
        }
    }

    impl WidgetImpl for Cols {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let kids = self.kids();
            match o {
                gtk::Orientation::Horizontal => {
                    let min = kids.iter().map(|k| k.measure(o, -1).0).max().unwrap_or(0);
                    (min, min, -1, -1)
                }
                _ => {
                    let w = if for_size > 0 { for_size } else { 800 };
                    let cols = self.columns(w, self.n_cols(w, &kids));
                    let rows = self.rows(&kids, &cols);
                    let h = rows.iter().sum::<i32>() + self.row_gap() * (rows.len() as i32 - 1).max(0);
                    (h, h, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _b: i32) {
            let kids = self.kids();
            let n = self.n_cols(width, &kids);
            let cols = self.columns(width, n);
            let mut rows = self.rows(&kids, &cols);
            let gap = self.row_gap();
            let used = rows.iter().sum::<i32>() + gap * (rows.len() as i32 - 1).max(0);
            if self.fill.get() && height > used && !rows.is_empty() {
                let extra = (height - used) / rows.len() as i32;
                for r in &mut rows {
                    *r += extra;
                }
            }
            let mut y = 0;
            let bleed = self.bleed.get() as i32;
            let nrows = rows.len();
            for (ri, row) in kids.chunks(n).enumerate() {
                let rh = rows[ri] + if ri + 1 == nrows { bleed } else { 0 };
                for (i, k) in row.iter().enumerate() {
                    let cw = cols[i].1 + if i + 1 == n { bleed } else { 0 };
                    k.size_allocate(&gtk::Allocation::new(cols[i].0, y, cw, rh), -1);
                }
                y += rows[ri] + gap;
            }
        }
    }
}

glib::wrapper! {
    /// Spalter med vekter (Oversikt 1 : 1,25 : 1) og like høye rader, som
    /// stables i én spalte under et bruddpunkt. Med `fill` fordeles ledig
    /// høyde på radene, så grafene fyller siden.
    pub struct Cols(ObjectSubclass<cols_imp::Cols>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Cols {
    pub fn new(weights: &[f32], gap: i32, stack_below: i32) -> Cols {
        let c: Cols = glib::Object::new();
        *c.imp().weights.borrow_mut() = weights.to_vec();
        c.imp().max_cols.set(weights.len().max(1));
        c.imp().gap.set(gap);
        c.imp().stack_below.set(stack_below);
        c
    }

    pub fn append(&self, w: &impl IsA<gtk::Widget>) {
        w.set_parent(self);
        self.queue_resize();
    }

    pub fn clear(&self) {
        while let Some(c) = self.first_child() {
            c.unparent();
        }
    }

    pub fn set_max_cols(&self, n: usize) {
        self.imp().max_cols.set(n.max(1));
        self.queue_resize();
    }

    /// Færre spalter under bredder: `&[(900, 3), (640, 2)]`.
    pub fn set_breaks(&self, breaks: &[(i32, usize)]) {
        *self.imp().breaks.borrow_mut() = breaks.to_vec();
        self.queue_resize();
    }

    /// Egen avstand mellom radene (standard: som mellom spaltene).
    pub fn set_row_gap(&self, gap: i32) {
        self.imp().row_gap.set(Some(gap));
        self.queue_resize();
    }

    /// Bare hele rader (se `whole_rows` i `cols_imp::Cols`).
    pub fn set_whole_rows(&self) {
        self.imp().whole_rows.set(true);
        self.queue_resize();
    }

    /// Flisene: kantlinja til høyre og under klippes bort i ytterkanten.
    pub fn set_bleed(&self) {
        self.imp().bleed.set(true);
        self.set_overflow(gtk::Overflow::Hidden);
        self.queue_resize();
    }

    pub fn set_fill(&self, min_row: i32) {
        self.imp().fill.set(true);
        self.imp().min_row.set(min_row);
        self.set_vexpand(true);
        self.queue_resize();
    }
}

// ---------------------------------------------------------------- Bred eller smal

mod adaptive_imp {
    use super::*;

    #[derive(Default)]
    pub struct Adaptive {
        pub narrow: Cell<bool>,
        /// Venstrejustert (Innstillinger); ellers høyrejustert (toppfeltet).
        pub start: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Adaptive {
        const NAME: &'static str = "RiggAdaptive";
        type Type = super::Adaptive;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Adaptive {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl Adaptive {
        fn kids(&self) -> Option<(gtk::Widget, gtk::Widget)> {
            let a = self.obj().first_child()?;
            let b = a.next_sibling()?;
            Some((a, b))
        }
    }

    impl WidgetImpl for Adaptive {
        fn measure(&self, o: gtk::Orientation, f: i32) -> (i32, i32, i32, i32) {
            let Some((wide, narrow)) = self.kids() else { return (0, 0, -1, -1) };
            let (wm, wn, _, _) = wide.measure(o, f);
            let (nm, nn, _, _) = narrow.measure(o, f);
            match o {
                gtk::Orientation::Horizontal => (nm, wn.max(nn), -1, -1),
                _ => (wm.max(nm), wn.max(nn), -1, -1),
            }
        }

        fn size_allocate(&self, w: i32, h: i32, _b: i32) {
            let Some((wide, narrow)) = self.kids() else { return };
            let need = wide.measure(gtk::Orientation::Horizontal, -1).1;
            let use_narrow = w < need;
            if self.narrow.replace(use_narrow) != use_narrow {
                // Den som ikke vises, kan ikke nås med Tab eller pekeren.
                wide.set_sensitive(!use_narrow);
                narrow.set_sensitive(use_narrow);
                wide.set_can_target(!use_narrow);
                narrow.set_can_target(use_narrow);
            }
            let (show, hide) = if use_narrow { (&narrow, &wide) } else { (&wide, &narrow) };
            let hw = hide.measure(gtk::Orientation::Horizontal, -1).0;
            hide.size_allocate(&gtk::Allocation::new(w + 100, 0, hw.max(1), h), -1);
            let (smin, snat, _, _) = show.measure(gtk::Orientation::Horizontal, -1);
            let sw = snat.min(w).max(smin);
            let sh = show.measure(gtk::Orientation::Vertical, sw).1.min(h).max(1);
            let x = if self.start.get() { 0 } else { w - sw };
            show.size_allocate(&gtk::Allocation::new(x, (h - sh) / 2, sw, sh), -1);
        }
    }
}

glib::wrapper! {
    /// Viser `wide` når det er plass, ellers `narrow` (tidsvinduet på Grafer:
    /// segmentknapp i brede vinduer, nedtrekksliste ved 640 px).
    pub struct Adaptive(ObjectSubclass<adaptive_imp::Adaptive>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Adaptive {
    pub fn new(wide: &impl IsA<gtk::Widget>, narrow: &impl IsA<gtk::Widget>) -> Adaptive {
        let a: Adaptive = glib::Object::new();
        a.set_overflow(gtk::Overflow::Hidden);
        wide.set_parent(&a);
        narrow.set_parent(&a);
        narrow.set_sensitive(false);
        narrow.set_can_target(false);
        a
    }

    pub fn start(self) -> Self {
        self.imp().start.set(true);
        self
    }
}
