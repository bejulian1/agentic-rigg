//! Grafer (demoens `monitor.js`): linjegrafer fra 1 min til 48 t, med
//! historikken fra tjenesten og de levende prøvene flettet inn. Hvilke grafer
//! som vises, velges i toppfeltet (og under Innstillinger). Rutenettet fyller
//! høyden: én spalte med tre grafer eller færre, ellers to.
//!
//! Toppfeltet: tidsvindu (`chartWindow`), «På pause · 14:32:10», markøren på
//! alle grafene eller bare den du holder over (`chartSync`), pause for alle
//! (mellomrom utenfor en graf), velg grafer, og «Eksporter CSV …» med
//! tidsvinduet og seriene du ser, med min, snitt og maks per serie, fra
//! historikken i tjenesten (produktspesifikasjonen 3.2).
//!
//! **Eies av 2A (Data).**

use rigg_core::{tr, trf, trn};
use super::{Key, Page};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::chart::{self, ChartPanel, ChartSet, Cols, CursorSync, DEFS, DefOpts, math};
use crate::widgets::{Icon, Seg, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::fmt as F;
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Inner {
    app: Rc<App>,
    root: gtk::Box,
    actions: gtk::Box,
    grid: Cols,
    set: Rc<ChartSet>,
    sync: Rc<CursorSync>,
    seg: Seg,
    win_dd: gtk::DropDown,
    paused_tag: gtk::Label,
    sync_btn: gtk::ToggleButton,
    pause_btn: gtk::Button,
    pause_icon: Icon,
    paused: Cell<bool>,
    visible: Cell<bool>,
    quiet: Cell<bool>,
    pick: RefCell<Option<gtk::Popover>>,
    /// Diskene i Disktemperatur (grafen bygges på nytt når lista endres, runde 6A).
    disk_sig: RefCell<String>,
}

pub struct Grafer {
    inner: Rc<Inner>,
}

impl Page for Grafer {
    fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }

    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.inner.actions.clone().upcast())
    }

    fn show(&self, _q: Option<&str>) {
        self.inner.visible.set(true);
        self.inner.set.set_visible(true);
    }

    fn hide(&self) {
        self.inner.visible.set(false);
        self.inner.set.set_visible(false);
    }

    fn on_key(&self, k: &Key) -> bool {
        if k.key != gtk::gdk::Key::space || k.typing || k.ctrl || k.alt {
            return false;
        }
        // Mellomrom på en knapp, en graf eller et valg gjør det den gjør der.
        let focus = self.inner.app.window().and_then(|w| gtk::prelude::GtkWindowExt::focus(&w));
        if let Some(f) = focus {
            let mut w = Some(f);
            while let Some(c) = w {
                if c.is::<gtk::Button>() || c.is::<chart::Chart>() || c.is::<gtk::DropDown>() || c.is::<gtk::CheckButton>() {
                    return false;
                }
                w = c.parent();
            }
        }
        let p = !self.inner.paused.get();
        self.inner.set_paused(p);
        true
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(0, &["page", "chart-page"]);
    root.set_vexpand(true);
    let grid = Cols::new(&[1.0, 1.0], 8, 700);
    grid.set_fill(180);
    grid.add_css_class("chart-grid");
    root.append(&grid);

    // ---- Toppfeltet ----
    let actions = hbox(4, &[]);
    let opts: Vec<(String, &str)> = chart::window_choices().map(|(sec, title)| (sec.to_string(), title)).collect();
    let opt_refs: Vec<(&str, &str)> = opts.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    let seg = Seg::new(&opt_refs, &app.settings().chart_window.to_string(), tr!("Tidsvindu"));
    seg.root.add_css_class("win-seg");
    {
        let a = app.clone();
        seg.connect(move |v| {
            if let Ok(n) = v.parse::<u64>() {
                a.set(json!({"chartWindow": n}));
            }
        });
    }
    let win_dd = {
        let a = app.clone();
        chart::window_dropdown(app.settings().chart_window as i64, tr!("Tidsvindu"), move |sec| {
            a.set(json!({"chartWindow": sec}));
        })
    };
    actions.append(&chart::Adaptive::new(&seg.root, &win_dd));
    let sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sep.add_css_class("sep");
    sep.set_valign(gtk::Align::Center);
    actions.append(&sep);
    let paused_tag = label("", &["tag", "tag-lg", "paused-tag"]);
    paused_tag.set_visible(false);
    actions.append(&paused_tag);
    let sync_btn = gtk::ToggleButton::new();
    sync_btn.add_css_class("flat");
    sync_btn.add_css_class("icon-only");
    sync_btn.set_child(Some(&Icon::new("crosshair", 16)));
    sync_btn.set_active(app.settings().chart_sync);
    actions.append(&sync_btn);
    let pause_btn = gtk::Button::new();
    pause_btn.add_css_class("flat");
    pause_btn.add_css_class("icon-only");
    let pause_icon = Icon::new("pause", 16);
    pause_btn.set_child(Some(&pause_icon));
    tip(&pause_btn, tr!("Pause alle"), Some("Mellomrom"));
    pause_btn.update_property(&[gtk::accessible::Property::Label(tr!("Pause alle grafene"))]);
    actions.append(&pause_btn);
    let pick_btn = gtk::Button::new();
    pick_btn.add_css_class("flat");
    pick_btn.add_css_class("icon-only");
    pick_btn.set_child(Some(&Icon::new("sliders-horizontal", 16)));
    tip(&pick_btn, tr!("Velg grafer"), None);
    pick_btn.update_property(&[gtk::accessible::Property::Label(tr!("Velg grafer"))]);
    actions.append(&pick_btn);
    let export_btn = gtk::Button::new();
    export_btn.add_css_class("flat");
    export_btn.add_css_class("icon-only");
    export_btn.set_child(Some(&Icon::new("download", 16)));
    tip(&export_btn, tr!("Eksporter CSV …"), None);
    export_btn.update_property(&[gtk::accessible::Property::Label(tr!("Eksporter CSV …"))]);
    actions.append(&export_btn);

    let sync = {
        let a = Rc::downgrade(app);
        CursorSync::new(move || a.upgrade().is_none_or(|a| a.settings().chart_sync))
    };
    let inner = Rc::new(Inner {
        app: app.clone(),
        root,
        actions,
        grid,
        set: ChartSet::new(app),
        sync,
        seg,
        win_dd,
        paused_tag,
        sync_btn: sync_btn.clone(),
        pause_btn: pause_btn.clone(),
        pause_icon,
        paused: Cell::new(false),
        visible: Cell::new(false),
        quiet: Cell::new(false),
        pick: RefCell::new(None),
        disk_sig: RefCell::new(String::new()),
    });
    inner.build_charts();
    inner.render_sync();
    {
        let w = Rc::downgrade(&inner);
        pause_btn.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                let p = !s.paused.get();
                s.set_paused(p);
            }
        });
        let a = app.clone();
        sync_btn.connect_toggled(move |b| {
            if a.settings().chart_sync != b.is_active() {
                a.set(json!({"chartSync": b.is_active()}));
            }
        });
        let w = Rc::downgrade(&inner);
        pick_btn.connect_clicked(move |b| {
            if let Some(s) = w.upgrade() {
                s.show_pick(b);
            }
        });
        let w = Rc::downgrade(&inner);
        export_btn.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.export();
            }
        });
        let w = Rc::downgrade(&inner);
        app.on_settings.connect(move |k| {
            let Some(s) = w.upgrade() else { return };
            if k.iter().any(|k| k == "charts") {
                s.build_charts();
            }
            if k.iter().any(|k| k == "chartWindow") {
                let win = s.app.settings().chart_window;
                s.seg.set(&win.to_string());
                chart::set_window_dropdown(&s.win_dd, win as i64);
                for p in s.set.list() {
                    p.set_window(win as i64);
                }
                s.sync_pause();
            }
            if k.iter().any(|k| k == "chartSync") {
                s.render_sync();
            }
        });
        // Disktemperatur har én serie per disk: ny graf når diskene kommer eller går.
        let w = Rc::downgrade(&inner);
        app.on_telemetry.connect(move |_| {
            let Some(s) = w.upgrade() else { return };
            if !s.app.settings().charts.iter().any(|c| c == "diskTemp") {
                return;
            }
            let sig: String = chart::disk_temp_series(&s.app).into_iter().map(|(k, _)| k).collect::<Vec<_>>().join(",");
            if *s.disk_sig.borrow() != sig {
                let first = s.disk_sig.borrow().is_empty() && sig.is_empty();
                *s.disk_sig.borrow_mut() = sig;
                if !first {
                    s.build_charts();
                }
            }
        });
        let w = Rc::downgrade(&inner);
        inner.set.connect_gpu(move || {
            if let Some(s) = w.upgrade() {
                s.build_charts();
            }
        });
    }
    #[cfg(debug_assertions)]
    {
        use super::oversikt::bilder::hook;
        let w = Rc::downgrade(&inner);
        hook("grafer.pause", move |a| {
            if let Some(s) = w.upgrade() {
                s.set_paused(a == "1");
            }
        });
        let w = Rc::downgrade(&inner);
        hook("grafer.markor", move |a| {
            if let Some(s) = w.upgrade() {
                let list = s.set.list();
                let f = a.parse::<f32>().ok();
                if let Some(p) = list.get(1).or(list.first()) {
                    p.chart.set_hover_frac(f);
                }
            }
        });
    }
    Box::new(Grafer { inner })
}

impl Inner {
    fn build_charts(self: &Rc<Self>) {
        self.grid.clear();
        self.set.clear();
        let ids: Vec<&str> = DEFS.iter().map(|d| d.id).filter(|id| self.app.settings().charts.iter().any(|c| c == id)).collect();
        self.grid.set_max_cols(if ids.len() <= 3 { 1 } else { 2 });
        let win = self.app.settings().chart_window as i64;
        for id in ids {
            let p = chart::def_panel(&self.app, id, DefOpts { window: win, sync: Some(self.sync.clone()), ..Default::default() });
            let w = Rc::downgrade(self);
            p.connect_pause(move |_| {
                if let Some(s) = w.upgrade()
                    && !s.quiet.get()
                {
                    s.sync_pause();
                }
            });
            if self.paused.get() {
                p.set_paused(true);
            }
            self.grid.append(&p.root);
            self.set.add(&p);
        }
        if self.visible.get() {
            self.set.set_visible(true);
        }
        self.sync_pause();
    }

    fn set_paused(&self, p: bool) {
        self.quiet.set(true);
        for x in self.set.list() {
            x.set_paused(p);
        }
        self.quiet.set(false);
        self.sync_pause();
    }

    /// Knappen og merket følger grafene: «Fortsett alle» når alle står stille.
    fn sync_pause(&self) {
        let list = self.set.list();
        let all = !list.is_empty() && list.iter().all(|p| p.chart.paused());
        self.paused.set(all);
        for p in &list {
            p.set_quiet_pause(all);
        }
        self.pause_icon.set_icon(if all { "play" } else { "pause" });
        tip(&self.pause_btn, if all { tr!("Fortsett alle") } else { tr!("Pause alle") }, Some("Mellomrom"));
        self.pause_btn.update_property(&[gtk::accessible::Property::Label(if all { tr!("Fortsett alle grafene") } else { tr!("Pause alle grafene") })]);
        crate::widgets::class(&self.pause_btn, "on", all);
        self.paused_tag.set_visible(all);
        if all {
            let at = list.iter().map(|p| p.chart.paused_at()).max().unwrap_or_else(rigg_core::now_ms);
            set_text(&self.paused_tag, &trf!("På pause · {verdi}", verdi = F::time(F::local(at, chart::utc_offset(at)), true)));
        }
    }

    fn render_sync(&self) {
        let on = self.app.settings().chart_sync;
        if self.sync_btn.is_active() != on {
            self.sync_btn.set_active(on);
        }
        let t = if on { tr!("Markøren vises på alle grafene. Klikk for bare grafen du holder over.") } else { tr!("Markøren vises bare på grafen du holder over. Klikk for alle grafene.") };
        tip(&self.sync_btn, t, None);
        self.sync_btn.update_property(&[gtk::accessible::Property::Label(tr!("Markør på alle grafene"))]);
        if !on {
            self.sync.set(None);
        }
        for p in self.set.list() {
            p.chart.queue_draw();
        }
    }

    /// «Grafer som vises»: avkrysning som blir stående åpen. Minst én graf.
    fn show_pick(self: &Rc<Self>, anchor: &gtk::Button) {
        let list = vbox(2, &["pick-list"]);
        list.append(&label(tr!("Grafer som vises"), &["menu-label"]));
        let mut checks: Vec<gtk::CheckButton> = Vec::new();
        for d in DEFS.iter() {
            let on = self.app.settings().charts.iter().any(|c| c == d.id);
            let a = self.app.clone();
            let id = d.id;
            let c = crate::widgets::switchrow::check(d.localized_title(), on, move |c| {
                let mut set: Vec<String> = a.settings().charts.clone();
                if c.is_active() {
                    if !set.iter().any(|x| x == id) {
                        set.push(id.to_string());
                    }
                } else {
                    set.retain(|x| x != id);
                }
                if set.is_empty() {
                    c.set_active(true);
                    return;
                }
                let ordered: Vec<&str> = DEFS.iter().map(|d| d.id).filter(|x| set.iter().any(|s| s == x)).collect();
                a.set(json!({"charts": ordered}));
            });
            c.add_css_class("pick-item");
            list.append(&c);
            checks.push(c);
        }
        // Den siste som er på, kan ikke slås av.
        let update = {
            let checks = checks.clone();
            move || {
                let n = checks.iter().filter(|c| c.is_active()).count();
                for c in &checks {
                    c.set_sensitive(!(n == 1 && c.is_active()));
                    c.set_tooltip_text(if n == 1 && c.is_active() { Some(tr!("Minst én graf må vises")) } else { None });
                }
            }
        };
        update();
        for c in &checks {
            let u = update.clone();
            c.connect_toggled(move |_| u());
        }
        let p = crate::widgets::popover::popover(anchor, &list);
        p.set_halign(gtk::Align::End);
        *self.pick.borrow_mut() = Some(p.clone());
        p.popup();
    }

    // ------------------------------------------------------------ Eksport

    fn export(self: &Rc<Self>) {
        let panels: Vec<Rc<ChartPanel>> = self.set.list();
        let mut cols: Vec<(Rc<ChartPanel>, usize)> = Vec::new();
        for p in &panels {
            for i in 0..p.chart.series().len() {
                if !p.chart.is_hidden(i) {
                    cols.push((p.clone(), i));
                }
            }
        }
        cols.truncate(rigg_core::history::MAX_KEYS);
        if cols.is_empty() {
            return;
        }
        let window = self.app.settings().chart_window as i64;
        let keys: Vec<String> = cols.iter().map(|(p, i)| p.chart.series()[*i].key.clone()).collect();
        let points = (window as usize).clamp(rigg_core::history::MIN_POINTS, rigg_core::history::MAX_POINTS);
        let me = self.clone();
        self.app.client.request("get_history", json!({"window": window, "keys": keys, "points": points}), move |res| {
            let v = match res {
                Ok(v) => v,
                Err(e) => {
                    me.app.toast(Toast::new(tr!("Kunne ikke hente historikken")).body(&e.message).error());
                    return;
                }
            };
            let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
            let mut out: Vec<math::CsvCol> = Vec::new();
            for (p, i) in &cols {
                let s = &p.chart.series()[*i];
                let unit = p.unit();
                let name = if p.chart.series().len() > 1 { format!("{} · {}", p.title, s.label) } else { p.title.clone() };
                for (part, word) in [("min", "min"), ("avg", "snitt"), ("max", "maks")] {
                    let vals: Vec<Option<f64>> = serde_json::from_value::<Vec<Option<f64>>>(v["series"][&s.key][part].clone()).unwrap_or_default().into_iter().map(|x| x.map(|x| (s.map)(x))).collect();
                    out.push(math::CsvCol { head: format!("{name} {word} [{unit}]"), values: vals, decimals: (p.decimals() + 1).min(3) });
                }
            }
            let fmt = rigg_core::csv::CsvFormat::from_name(me.app.data.borrow().service["log"]["format"].as_str().unwrap_or("standard"));
            let off = chart::utc_offset(rigg_core::now_ms());
            let text = math::csv(&t, &out, off, fmt);
            me.save_dialog(text, t.len(), window);
        });
    }

    fn save_dialog(self: &Rc<Self>, text: String, rows: usize, window: i64) {
        let now = crate::app::local_now();
        let name = format!("rigg-historikk-{:04}-{:02}-{:02}_{:02}-{:02}.csv", now.year, now.month, now.day, now.hour, now.minute);
        let d = gtk::FileDialog::new();
        d.set_title(tr!("Eksporter historikk som CSV"));
        d.set_initial_name(Some(&name));
        d.set_accept_label(Some(tr!("Eksporter")));
        let home = glib::home_dir();
        let docs = glib::user_special_dir(glib::UserDirectory::Documents).unwrap_or(home.clone());
        let dir = docs.join("Agentic Rigg");
        let folder = if dir.is_dir() { dir } else { docs };
        d.set_initial_folder(Some(&gio::File::for_path(&folder)));
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("CSV"));
        filter.add_suffix("csv");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        d.set_filters(Some(&filters));
        let me = self.clone();
        let win = self.app.window();
        d.save(win.as_ref(), gio::Cancellable::NONE, move |res| {
            let Ok(file) = res else { return };
            let Some(path) = file.path() else { return };
            match rigg_core::write_atomic(&path, text.as_bytes()) {
                Ok(()) => {
                    let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let f2 = file.clone();
                    let w = me.app.window();
                    me.app.toast(
                        Toast::new(tr!("Historikken er eksportert"))
                            .body(&trn!(rows, "{fname} · {antall} rader, {periode}", "{fname} · {antall} rader, {periode}", fname = fname, antall = F::num(Some(rows as f64), 0), periode = math::window_label(window)))
                            .icon("download")
                            .action(tr!("Vis i Filer"), move || {
                                gtk::FileLauncher::new(Some(&f2)).open_containing_folder(w.as_ref(), gio::Cancellable::NONE, |_| {});
                            }),
                    );
                }
                Err(e) => me.app.toast(Toast::new(tr!("Kunne ikke lagre fila")).body(&e.to_string()).error()),
            }
        });
    }
}
