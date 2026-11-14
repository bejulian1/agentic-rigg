//! Prosessor (1.1.0, runde 6A): CPU-Z-delen som lå på System, gjort om til en
//! CPU-Z/HWiNFO-side. Øverst de levende tallene (CPU-temperatur, total last,
//! snitt- og toppklokke, governor og boost), så prosessoren, cachen,
//! instruksjonssettene og sårbarhetene, og nederst klokke og last per tråd
//! gruppert etter L3-gruppe og kjerne (trådene som deler en kjerne, står sammen).
//!
//! De faste dataene hentes én gang (`get_system`); de levende tallene kommer
//! fra prøvene og oppdateres bare mens siden vises. Rekkefølgen: Nå, klokke
//! og last per tråd, så de faste dataene.

use super::{Key, Page};
use crate::rapport_ui::Sections;
use crate::state::App;
use crate::widgets::spark::SparkOpts;
use crate::widgets::trader::{GroupDef, ThreadGrid};
use crate::widgets::{Icon, Panel, Spark, class, hbox, label, set_text, vbox};
use gtk::glib;
use gtk::prelude::*;
use rigg_core::fmt as F;
use rigg_core::report::{self, ReportFormat, Section};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// ---------------------------------------------------------------- Brikker som brytes

/// Brikker i naturlig bredde som brytes til neste linje (CSS `flex-wrap`).
/// `GtkFlowBox` legger barna i like spalter, som gir ujevne mellomrom.
mod wrap_imp {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct Wrap;

    #[glib::object_subclass]
    impl ObjectSubclass for Wrap {
        const NAME: &'static str = "RiggWrap";
        type Type = super::Wrap;
        type ParentType = gtk::Widget;
        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("wrap");
        }
    }

    impl ObjectImpl for Wrap {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    pub const GAP: i32 = 4;

    impl Wrap {
        /// (barn, x, y, bredde, høyde) og total høyde for en bredde.
        pub fn layout(&self, width: i32) -> crate::widgets::Placed {
            let mut out = Vec::new();
            let (mut x, mut y, mut line_h) = (0, 0, 0);
            let mut c = self.obj().first_child();
            while let Some(w) = c {
                c = w.next_sibling();
                if !w.is_visible() {
                    continue;
                }
                let cw = w.measure(gtk::Orientation::Horizontal, -1).1.min(width.max(1));
                let ch = w.measure(gtk::Orientation::Vertical, cw).1;
                if x > 0 && x + cw > width {
                    x = 0;
                    y += line_h + GAP;
                    line_h = 0;
                }
                out.push((w, x, y, cw, ch));
                x += cw + GAP;
                line_h = line_h.max(ch);
            }
            (out, if line_h > 0 { y + line_h } else { 0 })
        }
    }

    impl WidgetImpl for Wrap {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }
        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => {
                    let mut min = 0;
                    let mut c = self.obj().first_child();
                    while let Some(w) = c {
                        min = min.max(w.measure(gtk::Orientation::Horizontal, -1).0);
                        c = w.next_sibling();
                    }
                    (min, min.max(200), -1, -1)
                }
                _ => {
                    let (_, h) = self.layout(if for_size > 0 { for_size } else { 400 });
                    (h, h, -1, -1)
                }
            }
        }
        fn size_allocate(&self, width: i32, _h: i32, _b: i32) {
            for (w, x, y, cw, ch) in self.layout(width).0 {
                w.size_allocate(&gtk::Allocation::new(x, y, cw, ch), -1);
            }
        }
    }
}

glib::wrapper! {
    pub struct Wrap(ObjectSubclass<wrap_imp::Wrap>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Wrap {
    fn default() -> Self {
        Self::new()
    }
}

impl Wrap {
    pub fn new() -> Wrap {
        glib::Object::new()
    }
    pub fn append(&self, w: &impl IsA<gtk::Widget>) {
        w.set_parent(self);
    }
    pub fn clear(&self) {
        while let Some(c) = self.first_child() {
            c.unparent();
        }
    }
}

// ---------------------------------------------------------------- Trådene

/// «0-3,8-11» → [0, 1, 2, 3, 8, 9, 10, 11].
pub fn cpu_list(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for p in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match p.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    out.extend(a..=b);
                }
            }
            None => {
                if let Ok(a) = p.parse() {
                    out.push(a);
                }
            }
        }
    }
    out
}

/// Én kjerne og trådene den har.
pub type CoreThreads = (Option<u32>, Vec<usize>);

/// Gruppene på siden: (L3-gruppenavn, [(kjerne, [tråder])]). Trådene som deler en
/// kjerne (pakke + `core_id`), står sammen; uten L3-gruppene er det én gruppe.
pub fn groups(thread_ids: &[usize], l3: &[String], core_of: &[Option<u32>], package_of: &[Option<u32>]) -> Vec<(String, Vec<CoreThreads>)> {
    if thread_ids.is_empty() {
        return Vec::new();
    }
    let mut sets: Vec<Vec<usize>> = l3.iter().map(|g| cpu_list(g).into_iter().filter(|t| thread_ids.contains(t)).collect::<Vec<_>>()).filter(|g| !g.is_empty()).collect();
    let mut covered: Vec<usize> = sets.iter().flatten().copied().collect();
    covered.sort_unstable();
    let mut expected = thread_ids.to_vec();
    expected.sort_unstable();
    if sets.len() < 2 || covered != expected {
        sets = vec![thread_ids.to_vec()];
    }
    let many = sets.len() > 1;
    sets.into_iter()
        .enumerate()
        .map(|(i, set)| {
            let mut cores: Vec<CoreThreads> = Vec::new();
            for t in set {
                let c = core_of.get(t).copied().flatten();
                let package = package_of.get(t).copied().flatten();
                match cores.iter_mut().find(|(k, ts)| c.is_some() && package.is_some() && *k == c && package_of.get(ts[0]).copied().flatten() == package) {
                    Some((_, v)) => v.push(t),
                    None => cores.push((c, vec![t])),
                }
            }
            cores.sort_by_key(|(c, v)| (package_of.get(v[0]).copied().flatten().unwrap_or(u32::MAX), c.unwrap_or(u32::MAX), v[0]));
            (if many { format!("L3-gruppe {i}") } else { "Alle kjerner".to_string() }, cores)
        })
        .collect()
}

fn core_label(core: Option<u32>, thread: usize, package_of: &[Option<u32>], many_packages: bool) -> String {
    match (core, package_of.get(thread).copied().flatten()) {
        (Some(c), Some(p)) if many_packages => format!("Pakke {p} · kjerne {c}"),
        (Some(c), Some(_)) => format!("Kjerne {c}"),
        _ => format!("T{thread}"),
    }
}

struct Flags {
    panel: Panel,
    chips: Wrap,
    count: gtk::Label,
    toggle: gtk::Button,
    search: gtk::SearchEntry,
    all: Cell<bool>,
    filter: RefCell<String>,
    shown_for: RefCell<String>,
}

/// En levende flis øverst: navn, tall, linje under og sparklinje.
struct Tile {
    value: gtk::Label,
    sub: gtk::Label,
    spark: Option<Spark>,
}

fn tile(parent: &gtk::Grid, name: &str, tip: &str, serie: Option<&str>) -> Tile {
    let b = vbox(2, &["pr-tile"]);
    b.set_hexpand(true);
    let k = label(name, &["pr-k"]);
    b.append(&k);
    let row = hbox(8, &[]);
    let value = label(F::DASH, &["pr-v", "num"]);
    value.set_hexpand(true);
    row.append(&value);
    let spark = serie.map(|s| {
        let sp = Spark::new(s, 72);
        sp.set_valign(gtk::Align::Center);
        row.append(&sp);
        sp
    });
    b.append(&row);
    let sub = label("", &["pr-sub", "num"]);
    sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    b.append(&sub);
    b.set_tooltip_text(Some(tip));
    let n = parent.observe_children().n_items() as i32;
    parent.attach(&b, n, 0, 1, 1);
    Tile { value, sub, spark }
}

struct Live {
    temp: Tile,
    load: Tile,
    clock: Tile,
    gov: Tile,
}

struct State {
    app: Rc<App>,
    /// Flisene i Nå: fire på rad, eller to og to i smale vinduer.
    tiles: gtk::Grid,
    tiles_cols: Cell<i32>,
    visible: Cell<bool>,
    system: RefCell<Value>,
    loading: Cell<bool>,
    view: Rc<Sections>,
    flags: Flags,
    live: Live,
    cores_panel: Panel,
    /// Klokke og last per tråd i én tegnet widget (`widgets/trader.rs`).
    grid: ThreadGrid,
    groups_sig: RefCell<String>,
}

pub struct Prosessor {
    root: gtk::Box,
    actions: gtk::Box,
    st: Rc<State>,
}

impl Page for Prosessor {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        self.st.visible.set(true);
        if self.st.system.borrow().is_null() {
            self.st.load();
        }
        self.st.refresh();
    }
    fn hide(&self) {
        self.st.visible.set(false);
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.key == gtk::gdk::Key::F5 && !k.ctrl && !k.alt {
            self.st.load();
            return true;
        }
        if k.ctrl && !k.alt && matches!(k.key, gtk::gdk::Key::f | gtk::gdk::Key::F) && self.st.flags.all.get() {
            self.st.flags.search.grab_focus();
            return true;
        }
        false
    }
}

impl State {
    fn load(self: &Rc<Self>) {
        if self.loading.replace(true) {
            return;
        }
        let w = Rc::downgrade(self);
        let first = self.system.borrow().is_null();
        self.app.client.request("get_system", json!({}), move |res| {
            let Some(s) = w.upgrade() else { return };
            s.loading.set(false);
            match res {
                Ok(v) => {
                    *s.system.borrow_mut() = v;
                    s.groups_sig.borrow_mut().clear();
                    s.refresh();
                    if !first {
                        s.app.toast(crate::toast::Toast::new("Faste data er lest på nytt").ms(2500));
                    }
                }
                Err(e) => {
                    if !first {
                        s.app.toast(crate::toast::Toast::new("Kunne ikke hente prosessordata").body(&e.message).error());
                    }
                }
            }
        });
    }

    fn sections(&self, report: bool) -> Vec<Section> {
        let sys = self.app.sys();
        let system = self.system.borrow();
        let (desktop, session) = crate::rapport_ui::desktop_env();
        let max_thread_mhz = self.max_thread_mhz();
        let mut v = report::system::sections(&report::system::SysInput { system: &system, sys: sys.as_ref(), show_hostname: false, desktop, session, report, max_thread_mhz });
        v.retain(|s| report::system::PROSESSOR.contains(&s.id) || report && s.id == "trader");
        if !report {
            // Det som endrer seg hvert sekund, står i Nå. Uten det bygges ikke
            // panelene i spaltene om hvert sekund (Sårbarheter har lange, brutte
            // linjer som er dyre å måle).
            for sec in v.iter_mut().filter(|s| s.id == "prosessor") {
                sec.items.retain(|i| !matches!(i, report::Item::Kv(k) if matches!(k.id, "temp" | "governor" | "boost")));
            }
        }
        v
    }

    /// Høyeste trådklokke tjenesten har sett (min og maks).
    fn max_thread_mhz(&self) -> Option<f64> {
        let d = self.app.data.borrow();
        d.stats["stats"].as_object().and_then(|o| o.iter().filter(|(k, _)| k.starts_with("cpu/thread/") && k.ends_with("/mhz")).filter_map(|(_, v)| v["max"].as_f64()).reduce(f64::max))
    }

    fn refresh(&self) {
        if !self.visible.get() {
            return;
        }
        let secs = self.sections(false);
        self.view.show(&secs);
        self.render_flags();
        self.render_live();
        self.render_cores();
    }

    fn render_live(&self) {
        // Fire fliser på rad, eller to og to når vinduet er smalt (aldri tre og én).
        let per = if self.app.window().is_some_and(|w| w.width() > 0 && w.width() < 1000) { 2 } else { 4 };
        if self.tiles_cols.get() != per {
            self.tiles_cols.set(per);
            let kids: Vec<gtk::Widget> = std::iter::successors(self.tiles.first_child(), |w| w.next_sibling()).collect();
            for k in &kids {
                self.tiles.remove(k);
            }
            for (i, k) in kids.iter().enumerate() {
                self.tiles.attach(k, i as i32 % per, i as i32 / per, 1, 1);
            }
        }
        let sys = self.app.sys().unwrap_or_default();
        let thread_ids = report::system::cpu_thread_ids(&self.system.borrow()["cpu"]);
        let now = self.app.telemetry().map(|t| t.t).unwrap_or_else(rigg_core::now_ms);
        let th = self.app.settings().thresholds.clone();
        // Temperatur med navnet fra den valgte CPU-sensoren.
        let t = rigg_core::sensormeta::cpu_temp(&sys);
        set_text(&self.live.temp.value, &F::temp(t, 1));
        let lvl = match t {
            Some(t) if t >= th.cpu_temp_crit => "danger",
            Some(t) if t >= th.cpu_temp_warn => "warn",
            _ => "",
        };
        for c in ["warn", "danger"] {
            class(&self.live.temp.value, c, c == lvl);
        }
        let source = rigg_core::sensormeta::cpu_temp_sensor(&sys).filter(|s| s.value.is_some() && !s.label.is_empty() && s.label != s.key).map(|s| s.label.clone()).unwrap_or_else(|| "CPU-temperatur".into());
        let detail = match sys.cpu.tctl.filter(|_| source != "Tctl") {
            Some(c) => format!("{source} · Tctl {}", F::temp(Some(c), 1)),
            None => source,
        };
        set_text(&self.live.temp.sub, &detail);
        if let Some(sp) = &self.live.temp.spark {
            sp.set_points(&self.app.recent("cpu/temp").iter().map(|(t, v)| (*t, F::temp_out(Some(*v)).unwrap_or(*v))).collect::<Vec<_>>(), now, SparkOpts { span_ms: 60_000, fixed: None, min_span: 10.0 });
            sp.set_level(lvl);
        }
        // Last: total, og den travleste tråden.
        set_text(&self.live.load.value, &F::pct(sys.cpu.load, 1));
        let busiest = thread_ids.iter().filter_map(|&i| sys.cpu.core_load.get(i).copied().flatten().map(|l| (i, l))).max_by(|a, b| a.1.total_cmp(&b.1));
        set_text(&self.live.load.sub, &busiest.map(|(i, l)| format!("travlest T{i} {}", F::pct(Some(l), 0))).unwrap_or_default());
        if let Some(sp) = &self.live.load.spark {
            sp.set_points(&self.app.recent("cpu/load"), now, SparkOpts { span_ms: 60_000, fixed: Some((0.0, 100.0)), min_span: 0.0 });
        }
        // Klokke: snitt og høyeste tråd nå, og høyeste som er målt.
        let mhz: Vec<(usize, f64)> = thread_ids.iter().filter_map(|&i| sys.cpu.core_mhz.get(i).copied().flatten().map(|m| (i, m))).collect();
        let avg = (!mhz.is_empty()).then(|| mhz.iter().map(|m| m.1).sum::<f64>() / mhz.len() as f64);
        set_text(&self.live.clock.value, &F::mhz(avg));
        let top = mhz.iter().max_by(|a, b| a.1.total_cmp(&b.1));
        let max_seen = self.max_thread_mhz();
        // «T14 3 950 · maks 4 125 MHz»: høyeste tråd nå, og høyeste målt.
        let sub = match (top, max_seen) {
            (Some((i, m)), Some(x)) => {
                format!("T{i} {} · maks {}", F::num(Some(*m), 0), F::mhz(Some(x)))
            }
            (Some((i, m)), None) => format!("T{i} {}", F::mhz(Some(*m))),
            _ => String::new(),
        };
        set_text(&self.live.clock.sub, &sub);
        self.live.clock.sub.set_tooltip_text(Some("Tråden med høyest klokke nå, og den høyeste trådklokken tjenesten har målt siden min og maks ble nullstilt."));
        if let Some(sp) = &self.live.clock.spark {
            // Snittet over trådene per prøve, fra sparklinjene til hver tråd.
            let series: Vec<Vec<(i64, f64)>> = thread_ids.iter().map(|i| self.app.recent(&format!("cpu/thread/{i}/mhz"))).collect();
            let pts: Vec<(i64, f64)> = series.first().map(|first| {
                first.iter().enumerate().filter_map(|(j, (t, _))| {
                    let vals: Vec<f64> = series.iter().filter_map(|s| s.get(j).filter(|p| p.0 == *t).map(|p| p.1)).collect();
                    (!vals.is_empty()).then(|| (*t, vals.iter().sum::<f64>() / vals.len() as f64))
                }).collect()
            }).unwrap_or_default();
            sp.set_points(&pts, now, SparkOpts { span_ms: 60_000, fixed: None, min_span: 400.0 });
        }
        // Governor, boost og lastsnitt.
        let gov = [sys.cpu.governor.clone(), self.system.borrow()["cpu"]["freq"]["driver"].as_str().map(str::to_string)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        set_text(&self.live.gov.value, if gov.is_empty() { F::DASH } else { &gov });
        let boost = match sys.cpu.boost {
            Some(true) => "boost på",
            Some(false) => "boost av",
            None => "boost ukjent",
        };
        let la = sys.loadavg.first().map(|v| format!("lastsnitt {}", F::num(Some(*v), 2)));
        set_text(&self.live.gov.sub, &[Some(boost.to_string()), la].into_iter().flatten().collect::<Vec<_>>().join(" · "));
        let all = (!sys.loadavg.is_empty()).then(|| sys.loadavg.iter().map(|v| F::num(Some(*v), 2)).collect::<Vec<_>>().join(" / "));
        self.live.gov.sub.set_tooltip_text(all.map(|a| format!("Lastsnitt 1 / 5 / 15 min: {a}. Over antall tråder betyr kø.")).as_deref());
    }

    fn render_flags(&self) {
        let f = &self.flags;
        let system = self.system.borrow();
        let cpu = &system["cpu"];
        let all: Vec<String> = cpu["flags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let imp: Vec<(String, String)> = cpu["important"].as_array().map(|a| a.iter().filter_map(|x| Some((x["flag"].as_str()?.to_string(), x["label"].as_str()?.to_string()))).collect()).unwrap_or_default();
        let filter = f.filter.borrow().clone();
        let key = format!("{}|{}|{}", all.len(), f.all.get(), filter);
        if *f.shown_for.borrow() == key {
            return;
        }
        *f.shown_for.borrow_mut() = key;
        f.chips.clear();
        // Store bokstaver overalt (som CPU-Z), med navnet fra /proc/cpuinfo i tipset.
        let chip = |text: &str, tip: Option<&str>, key: bool| {
            let l = label(&text.to_uppercase(), &["chip"]);
            if key {
                l.add_css_class("key");
            }
            if !f.all.get() && !key {
                l.add_css_class("muted");
            }
            if f.all.get() {
                l.add_css_class("mono");
            }
            l.set_tooltip_text(tip);
            l
        };
        if all.is_empty() {
            set_text(&f.count, "");
            return;
        }
        if f.all.get() {
            let list: Vec<&String> = all.iter().filter(|x| filter.is_empty() || x.contains(filter.as_str())).collect();
            for x in &list {
                let is_key = imp.iter().any(|(fl, _)| fl == *x);
                f.chips.append(&chip(x, Some(x.as_str()), is_key));
            }
            if list.is_empty() {
                f.chips.append(&label("Ingen treff.", &["muted"]));
            }
            set_text(&f.count, &if filter.is_empty() { format!("{} i alt", all.len()) } else { format!("{} av {}", list.len(), all.len()) });
        } else {
            for (fl, l) in &imp {
                f.chips.append(&chip(l, Some(fl), true));
            }
            // Noen av de andre, så kortet ikke står tomt; resten bak «Vis alle».
            let rest: Vec<&String> = all.iter().filter(|x| !imp.iter().any(|(fl, _)| fl == *x)).take(18).collect();
            for x in &rest {
                f.chips.append(&chip(x, Some(x.as_str()), false));
            }
            set_text(&f.count, &format!("{} av {}", imp.len() + rest.len(), all.len()));
            f.count.set_tooltip_text(Some(&format!("{} utvalgte først, så noen av de andre. Vis alle for å se og søke i alle {}.", imp.len(), all.len())));
        }
    }

    /// Grupperer trådene etter L3-gruppe og kjerne når tallet på tråder eller topologien endres.
    fn build_groups(&self, thread_ids: &[usize]) {
        let system = self.system.borrow();
        let cpu = &system["cpu"];
        let l3: Vec<String> = cpu["l3Groups"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let core_of: Vec<Option<u32>> = cpu["coreOf"].as_array().map(|a| a.iter().map(|x| x.as_u64().map(|v| v as u32)).collect()).unwrap_or_default();
        let package_of: Vec<Option<u32>> = cpu["packageOf"].as_array().map(|a| a.iter().map(|x| x.as_u64().and_then(|v| u32::try_from(v).ok())).collect()).unwrap_or_default();
        let many_packages = package_of.iter().flatten().collect::<std::collections::BTreeSet<_>>().len() > 1;
        let sig = format!("{thread_ids:?}|{l3:?}|{core_of:?}|{package_of:?}");
        if *self.groups_sig.borrow() == sig {
            return;
        }
        *self.groups_sig.borrow_mut() = sig;
        let l3_size = cpu["caches"].as_array().filter(|c| c.iter().filter(|x| x["level"].as_u64() == Some(3)).count() == 1).and_then(|c| c.iter().find(|x| x["level"].as_u64() == Some(3))).and_then(|x| x["size"].as_f64());
        let defs = groups(thread_ids, &l3, &core_of, &package_of)
            .into_iter()
            .map(|(name, cores)| {
                let known = cores.iter().all(|(core, ts)| core.is_some() && package_of.get(ts[0]).is_some_and(Option::is_some));
                let count = if known { F::plural(cores.len() as i64, "kjerne", "kjerner") } else { F::plural(cores.len() as i64, "trådgruppe", "trådgrupper") };
                let info = [Some(count), l3_size.filter(|_| name.starts_with("L3-gruppe ")).map(|s| format!("{} L3", F::bytes(Some(s), 0)))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                let cores = cores.into_iter().map(|(c, ts)| (core_label(c, ts[0], &package_of, many_packages), ts)).collect();
                GroupDef { name, info, cores }
            })
            .collect();
        self.grid.set_groups(defs);
    }

    fn render_cores(&self) {
        // Topologien (L3-grupper og kjerner) kommer med get_system; vent på den.
        if self.system.borrow().is_null() {
            return;
        }
        let sys = self.app.sys().unwrap_or_default();
        let n = sys.cpu.core_load.len().max(sys.cpu.core_mhz.len());
        let thread_ids = report::system::cpu_thread_ids(&self.system.borrow()["cpu"]);
        self.build_groups(&thread_ids);
        // Klokkene til nærmeste 25 MHz (trinnet P-tilstandene har), så tallene
        // står roligere; widgeten tegner bare når noe som vises, har endret seg.
        let vals = (0..n).map(|i| (sys.cpu.core_mhz.get(i).copied().flatten().map(|m| (m / 25.0).round() * 25.0), sys.cpu.core_load.get(i).copied().flatten())).collect();
        self.grid.set_values(vals, self.app.settings().thresholds.load_warn);
    }
}

fn flags_panel() -> Flags {
    let panel = Panel::new("Instruksjonssett");
    panel.root.add_css_class("sec");
    panel.root.add_css_class("flags-card");
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Søk"));
    search.add_css_class("flags-search");
    search.update_property(&[gtk::accessible::Property::Label("Søk i instruksjonssett")]);
    search.set_visible(false);
    search.set_max_width_chars(12);
    panel.aside.append(&search);
    let count = label("", &["num"]);
    panel.aside.append(&count);
    let toggle = crate::widgets::button(Some("chevron-down"), None, &["flat", "sm", "icon-only"]);
    toggle.set_tooltip_text(Some("Vis alle og søk"));
    toggle.update_property(&[gtk::accessible::Property::Label("Vis alle og søk")]);
    toggle.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
    panel.aside.append(&toggle);
    let chips = Wrap::new();
    chips.add_css_class("chips");
    panel.body.append(&chips);
    let note = label("Uthevet: viktige for spill og koding. Navnet i /proc/cpuinfo står i verktøytipset.", &["muted", "sm", "flags-note"]);
    note.set_wrap(true);
    panel.body.append(&note);
    Flags { panel, chips, count, toggle, search, all: Cell::new(false), filter: RefCell::new(String::new()), shown_for: RefCell::new(String::new()) }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "pr-page"]);
    // ---- Nå: de levende tallene ----
    let now_panel = Panel::new("Nå");
    now_panel.root.add_css_class("pr-now");
    // Fire fliser på rad, to og to i smale vinduer (render_live).
    let tiles = gtk::Grid::new();
    tiles.add_css_class("pr-tiles");
    tiles.set_column_homogeneous(true);
    let live = Live {
        temp: tile(&tiles, "CPU-temperatur", "Temperatur fra CPU-sensoren. Sensorens navn står under verdien når det er tilgjengelig; det kan for eksempel være Tdie, Tctl eller Package id 0.", Some("temp")),
        load: tile(&tiles, "Last", "Total last fra /proc/stat, siste minutt i linja.", Some("load")),
        clock: tile(&tiles, "Klokke (snitt)", "Snittet over alle trådene (APERF/MPERF fra cpufreq). Med boost går enkelttråder over maks.", Some("clock")),
        gov: tile(&tiles, "Governor", "Strategien kjernen bruker for klokken, og driveren (cpufreq).", None),
    };
    now_panel.body.append(&tiles);
    root.append(&now_panel.root);
    // ---- Klokke og last per tråd, gruppert etter L3-gruppe og kjerne ----
    let cores_panel = Panel::new("Klokke og last per tråd");
    cores_panel.root.add_css_class("sec");
    cores_panel.root.add_css_class("pr-cores");
    let legend = hbox(4, &["legend"]);
    let sw = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sw.add_css_class("lg");
    sw.set_valign(gtk::Align::Center);
    legend.append(&sw);
    legend.append(&label("stolpe = last", &[]));
    cores_panel.aside.append(&legend);
    let grid = ThreadGrid::new(app);
    grid.add_css_class("pr-groups");
    cores_panel.body.append(&grid);
    root.append(&cores_panel.root);
    // ---- Prosessor, cache, instruksjonssett og sårbarheter ----
    let view = Sections::new();
    let flags = flags_panel();
    view.custom.borrow_mut().insert("flagg", flags.panel.root.clone().upcast());
    root.append(&view.masonry);

    let st = Rc::new(State {
        app: app.clone(),
        tiles: tiles.clone(),
        tiles_cols: Cell::new(4),
        visible: Cell::new(false),
        system: RefCell::new(Value::Null),
        loading: Cell::new(false),
        view,
        flags,
        live,
        cores_panel,
        grid,
        groups_sig: RefCell::new(String::new()),
    });
    let _ = &st.cores_panel;
    {
        let w = Rc::downgrade(&st);
        st.flags.toggle.connect_clicked(move |b| {
            let Some(s) = w.upgrade() else { return };
            let all = !s.flags.all.get();
            s.flags.all.set(all);
            s.flags.search.set_visible(all);
            b.set_child(Some(&Icon::new(if all { "chevrons-down-up" } else { "chevron-down" }, 14)));
            let t = if all { "Vis utvalget" } else { "Vis alle og søk" };
            b.set_tooltip_text(Some(t));
            b.update_property(&[gtk::accessible::Property::Label(t)]);
            b.update_state(&[gtk::accessible::State::Expanded(Some(all))]);
            if all {
                s.flags.search.grab_focus();
            } else {
                s.flags.search.set_text("");
            }
            s.render_flags();
        });
        let w = Rc::downgrade(&st);
        st.flags.search.connect_search_changed(move |e| {
            if let Some(s) = w.upgrade() {
                *s.flags.filter.borrow_mut() = e.text().trim().to_lowercase();
                s.render_flags();
            }
        });
    }
    // Knappene i toppfeltet.
    let actions = hbox(2, &[]);
    actions.append(&crate::rapport_ui::save_button(app));
    let w = Rc::downgrade(&st);
    actions.append(&crate::rapport_ui::copy_button(app, move |fmt: ReportFormat| {
        let s = w.upgrade()?;
        if s.system.borrow().is_null() {
            return None;
        }
        let secs = s.sections(true);
        let title = format!("{} – Prosessor ({})", rigg_core::version::NAME, crate::rapport_ui::stamp_long(rigg_core::now_ms()));
        Some((report::render_page(&title, &secs, fmt), "med klokke og last per tråd".to_string()))
    }));

    let r = {
        let w = Rc::downgrade(&st);
        Rc::new(move || {
            if let Some(s) = w.upgrade() {
                s.refresh();
            }
        })
    };
    let r1 = r.clone();
    app.on_telemetry.connect(move |_| r1());
    let r2 = r.clone();
    app.on_settings.connect(move |_| r2());
    {
        let w = Rc::downgrade(&st);
        app.on_conn.connect(move |c| {
            if let Some(s) = w.upgrade()
                && *c == crate::state::Conn::Connected
                && s.visible.get()
                && s.system.borrow().is_null()
            {
                s.load();
            }
        });
    }
    Box::new(Prosessor { root, actions, st })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spredte_cpu_ider_beholdes_uten_oppfunne_trader() {
        let ids = [2, 3, 8, 11];
        let mut core_of = vec![None; 12];
        core_of[2] = Some(0);
        core_of[3] = Some(1);
        core_of[8] = Some(0);
        core_of[11] = Some(1);
        let package_of = vec![Some(0); 12];
        let g = groups(&ids, &["0-3,8".into(), "4-7,11".into()], &core_of, &package_of);
        assert_eq!(g[0].0, "L3-gruppe 0");
        assert_eq!(g.iter().flat_map(|g| &g.1).flat_map(|c| &c.1).copied().collect::<Vec<_>>(), vec![2, 8, 3, 11]);
        let g = groups(&ids, &["0-3".into()], &core_of, &package_of);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].1, vec![(Some(0), vec![2, 8]), (Some(1), vec![3, 11])]);
        assert!(groups(&[], &["0-3".into()], &core_of, &package_of).is_empty());
    }

    #[test]
    fn l3_grupper_og_kjerner() {
        // To L3-grupper; tråd i og i + 8 deler kjerne. Gruppenavnet antar ingen CPU-modell.
        let core_of: Vec<Option<u32>> = (0..16).map(|t| Some(t % 8)).collect();
        let package_of = vec![Some(0); 16];
        let g = groups(&(0..16).collect::<Vec<_>>(), &["0-3,8-11".into(), "4-7,12-15".into()], &core_of, &package_of);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "L3-gruppe 0");
        assert_eq!(g[0].1.len(), 4);
        assert_eq!(g[0].1[0], (Some(0), vec![0, 8]));
        assert_eq!(g[1].1[3], (Some(7), vec![7, 15]));
        // Uten L3-grupper: én gruppe.
        let g = groups(&[0, 1, 2, 3], &[], &[Some(0), Some(1), Some(0), Some(1)], &package_of);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "Alle kjerner");
        assert_eq!(g[0].1, vec![(Some(0), vec![0, 2]), (Some(1), vec![1, 3])]);
        // Uten topologi: én flis per tråd.
        let g = groups(&[0, 1], &[], &[], &[]);
        assert_eq!(g[0].1, vec![(None, vec![0]), (None, vec![1])]);
        assert_eq!(cpu_list("0-3,8-11"), vec![0, 1, 2, 3, 8, 9, 10, 11]);
    }

    #[test]
    fn to_pakker_med_likt_core_id_holdes_adskilt_uten_full_l3_topologi() {
        let ids = [0, 1, 8, 9];
        let mut core_of = vec![None; 10];
        let mut package_of = vec![None; 10];
        for id in ids {
            core_of[id] = Some(0);
            package_of[id] = Some(if id < 8 { 0 } else { 1 });
        }
        for l3 in [vec![], vec!["0-1".into()]] {
            let g = groups(&ids, &l3, &core_of, &package_of);
            assert_eq!(g.len(), 1);
            assert_eq!(g[0].1, vec![(Some(0), vec![0, 1]), (Some(0), vec![8, 9])]);
        }
        assert_eq!(core_label(Some(0), 0, &package_of, true), "Pakke 0 · kjerne 0");
        assert_eq!(core_label(Some(0), 8, &package_of, true), "Pakke 1 · kjerne 0");
        package_of[8] = None;
        package_of[9] = None;
        let g = groups(&ids, &[], &core_of, &package_of);
        assert_eq!(g[0].1.len(), 3, "ukjent pakke beviser ikke at to tråder deler kjerne");
        assert_eq!(core_label(Some(0), 8, &package_of, true), "T8");
    }
}
