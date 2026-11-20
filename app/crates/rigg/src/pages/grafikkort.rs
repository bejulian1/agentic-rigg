//! Grafikkort (GPU-Z): alt driveren og sysfs vet om kortet, klokker med
//! stolper, P-tilstander, grenser, vifter, det kortet ikke støtter, prosessene
//! og referansetabellen («Referanse, ikke målt»). Port av `web/js/pages/gpu.js`.
//!
//! Innholdet kommer fra `rigg_core::report::gpu::sections`, så «Kopier» og
//! «Lagre rapport …» gir det samme som står her. Tilstanden til kortet
//! (driveren er oppdatert, ingen driver, kortet svarer ikke, dvale) står
//! øverst, og verdiene som ikke kan leses nå, viser «–».
//!
//! Siden oppdateres bare mens den vises, og bare tekstene som er nye.
//! Prosesslista spørres etter hvert 5. s (`get_processes`) bare mens siden
//! vises; tjenesten leser den bare så lenge noen spør.

use super::{Key, Page};
use crate::rapport_ui::{SectionPanel, Sections};
use crate::state::App;
use crate::widgets::bar::Bar;
use crate::widgets::{Panel, hbox, label, set_text, vbox};
use gtk::prelude::*;
use rigg_core::fmt as F;
use rigg_core::model::{DeviceState, GpuInfo, GpuStatic};
use rigg_core::report::{self, Item, Kv, ReportFormat, Section};
use rigg_core::{tr, trf};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Hvor ofte prosesslista hentes mens siden vises (ms).
const PROCESSES_MS: u64 = 5_000;

struct Clocks {
    panel: Panel,
    rows: Vec<(Bar, gtk::Label, gtk::Label)>,
    note: gtk::Label,
}

struct State {
    app: Rc<App>,
    visible: Cell<bool>,
    show_uuid: Cell<bool>,
    view: Rc<Sections>,
    clocks: Clocks,
    content: gtk::Stack,
    empty: gtk::Box,
    empty_title: gtk::Label,
    empty_text: gtk::Label,
    /// Siste prosessliste fra `get_processes`: (kort, lista).
    procs: RefCell<Option<(String, Value)>>,
    proc_timer: RefCell<Option<gtk::glib::SourceId>>,
}

pub struct Grafikkort {
    root: gtk::Box,
    actions: gtk::Box,
    st: Rc<State>,
}

impl Page for Grafikkort {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        self.st.visible.set(true);
        self.st.refresh();
        State::start_processes(&self.st);
    }
    fn hide(&self) {
        self.st.visible.set(false);
        if let Some(t) = self.st.proc_timer.borrow_mut().take() {
            t.remove();
        }
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.key == gtk::gdk::Key::F5 && !k.ctrl && !k.alt {
            self.st.reload();
            return true;
        }
        false
    }
}

/// Tilstanden til kortet øverst på siden (produktspesifikasjonen 3.8).
/// Feilkodene står bare i verktøytipset.
fn state_section(i: &GpuInfo) -> Option<Section> {
    let since = F::since(crate::app::local_time(i.since), crate::app::local_now());
    let mut s = Section::new("tilstand", tr!("Tilstand"));
    match i.state {
        DeviceState::Ok => return None,
        DeviceState::DriverUpdated => {
            s.kv(Kv::new("tilstand", tr!("Tilstand"), Some(tr!("Driveren er oppdatert").into())).info_opt(i.detail.clone()));
            s.kv(Kv::new("kjorer", tr!("Kjører"), i.driver.clone()));
            s.kv(Kv::new("installert", tr!("Installert"), i.driver_installed.clone()));
            s.kv(Kv::new("gjor", tr!("Hva du kan gjøre"), Some(tr!("Start maskinen på nytt. Da lastes den nye driveren, og kortet leses som før.").into())));
            s.items.push(Item::Note(tr!("Tjenesten sjekker driveren hvert 60. s. Verdiene under er de siste som ble lest.").into()));
        }
        DeviceState::Lost => {
            s.kv(Kv::new("tilstand", tr!("Tilstand"), Some(tr!("Skjermkortet svarer ikke").into())).info_opt(i.detail.clone()));
            s.kv(Kv::new("siden", tr!("Siden"), Some(since)));
            s.kv(Kv::new("hvorfor", tr!("Hvorfor"), Some(tr!("Det skjer når kortet faller av bussen (Xid 79) eller trekkes ut.").into())));
            s.items.push(Item::Note(tr!("Tjenesten prøver å finne kortet igjen hvert 10. s. Tallene før det er tatt vare på, og historikken får et hull. Verdiene som leses nå, står som «–».").into()));
        }
        DeviceState::Suspended => {
            s.kv(Kv::new("tilstand", tr!("Tilstand"), Some(tr!("Maskinen sover").into())));
            s.items.push(Item::Note(tr!("Prøvene står stille og starter igjen 2 s etter at maskinen våkner.").into()));
        }
        DeviceState::NoDriver => {
            s.kv(Kv::new("tilstand", tr!("Tilstand"), Some(tr!("Fant ingen NVIDIA-driver").into())).info_opt(i.detail.clone()));
        }
    }
    Some(s)
}

impl State {
    /// Henter prosesslista nå og hvert 5. s til siden skjules.
    fn start_processes(this: &Rc<State>) {
        if this.proc_timer.borrow().is_some() {
            return;
        }
        this.fetch_processes();
        let w = Rc::downgrade(this);
        let id = gtk::glib::timeout_add_local(std::time::Duration::from_millis(PROCESSES_MS), move || {
            let Some(s) = w.upgrade() else { return gtk::glib::ControlFlow::Break };
            s.fetch_processes();
            gtk::glib::ControlFlow::Continue
        });
        *this.proc_timer.borrow_mut() = Some(id);
    }

    fn fetch_processes(self: &Rc<Self>) {
        let Some(id) = self.app.gpu_id() else { return };
        if !self.app.client.is_connected() || self.app.gpu_info().is_some_and(|i| i.state == DeviceState::Lost) {
            return;
        }
        let w = Rc::downgrade(self);
        self.app.client.request("get_processes", json!({"gpu": id}), move |res| {
            let Some(s) = w.upgrade() else { return };
            let Ok(v) = res else { return };
            if v["live"] != true || !v["processes"].is_object() {
                return;
            }
            let mut p = v["processes"].clone();
            p["live"] = json!(true);
            let gpu = v["gpu"].as_str().unwrap_or_default().to_string();
            let changed = s.procs.borrow().as_ref().is_none_or(|(g, old)| g != &gpu || old["list"] != p["list"] || old["t"] != p["t"]);
            if changed {
                *s.procs.borrow_mut() = Some((gpu, p));
                s.refresh();
            }
        });
    }

    fn sections(&self, show_uuid: bool) -> Option<(Vec<Section>, GpuStatic)> {
        let app = &self.app;
        let info = app.gpu_info();
        let mut st = card_statics(app).or_else(|| info.as_ref().filter(|i| !i.id.is_empty()).map(|i| GpuStatic { id: i.id.clone(), name: i.name.clone(), driver: i.driver.clone(), ..Default::default() }))?;
        // Den levende prosesslista erstatter øyeblikksbildet fra de faste dataene.
        if let Some((g, p)) = self.procs.borrow().as_ref()
            && *g == st.id
            && st.extra.is_object()
        {
            st.extra["processes"] = p.clone();
        }
        let sample = if info.as_ref().is_some_and(|i| i.state == DeviceState::Lost) { None } else { app.gpu_sample() };
        let egg = spec_name(&st).and_then(|n| crate::easter::launch_day(app, &n));
        let now = rigg_core::now_ms();
        let input = report::gpu::GpuInput { info: info.as_ref(), st: &st, s: sample.as_ref(), show_uuid, egg, utc_offset_s: crate::rapport_ui::utc_offset(now) };
        let mut secs = report::gpu::sections(&input);
        // På siden står referansetabellen øverst i høyre spalte (som demoen); i
        // rapporten står den sist.
        if let Some(i) = secs.iter().position(|s| s.id == "spesifikasjon") {
            let spec = secs.remove(i);
            let at = secs.iter().position(|s| s.id == "vifter").unwrap_or(secs.len());
            secs.insert(at, spec);
        }
        if let Some(s) = info.as_ref().and_then(state_section) {
            secs.insert(0, s);
        }
        Some((secs, st))
    }

    fn refresh(&self) {
        if !self.visible.get() {
            return;
        }
        let info = self.app.gpu_info();
        let no_card = info.as_ref().is_none_or(|i| i.id.is_empty()) && card_statics(&self.app).is_none();
        if no_card {
            let (title, text) = match info.as_ref().map(|i| i.state) {
                Some(DeviceState::NoDriver) => (
                    tr!("Fant ingen NVIDIA-driver"),
                    tr!("Prosessor, minne og sensorer vises som vanlig. Når driveren er installert og lastet, leses kortet av seg selv. Tjenesten prøver hvert 60. s."),
                ),
                Some(DeviceState::DriverUpdated) => (
                    tr!("Driveren er oppdatert"),
                    tr!("En ny NVIDIA-driver er installert, men den gamle kjører fortsatt. Start maskinen på nytt for å lese skjermkortet."),
                ),
                _ if !self.app.client.is_connected() => (tr!("Venter på tjenesten"), tr!("Skjermkortet vises når tjenesten svarer.")),
                _ => (tr!("Leser skjermkortet …"), tr!("De faste dataene leses de første sekundene.")),
            };
            set_text(&self.empty_title, title);
            set_text(&self.empty_text, text);
            self.empty.set_tooltip_text(info.as_ref().and_then(|i| i.detail.as_deref()));
            self.content.set_visible_child_name("tom");
            return;
        }
        self.content.set_visible_child_name("kort");
        let Some((secs, st)) = self.sections(self.show_uuid.get()) else { return };
        if let Some(c) = secs.iter().find(|s| s.id == "klokker") {
            self.update_clocks(c, &st);
        }
        self.view.show(&secs);
    }

    fn update_clocks(&self, sec: &Section, st: &GpuStatic) {
        let s = self.app.gpu_sample().unwrap_or_default();
        let lost = self.app.gpu_info().is_some_and(|i| i.state == DeviceState::Lost);
        let vals = [(s.core_clock, st.core_max), (s.sm_clock, st.sm_max), (s.mem_clock, st.mem_max), (s.video_clock, st.video_max)];
        for ((now, max), (bar, nl, ml)) in vals.iter().zip(&self.clocks.rows) {
            let now = if lost { None } else { *now };
            set_text(nl, &F::num(now, 0));
            set_text(ml, &F::num(*max, 0));
            crate::widgets::class(ml, "na", max.is_none());
            bar.set(Some(now.zip(*max).map(|(n, m)| n / m).unwrap_or(0.0)), "");
        }
        if let Some(Item::Note(n)) = sec.items.iter().find(|i| matches!(i, Item::Note(_))) {
            set_text(&self.clocks.note, n);
        }
    }

    /// F5: henter de faste dataene på nytt fra tjenesten.
    fn reload(self: &Rc<Self>) {
        let Some(id) = self.app.gpu_info().map(|i| i.id).filter(|i| !i.is_empty()) else { return };
        let a = self.app.clone();
        self.app.client.request("get_static", json!({"gpu": id}), move |res| {
            let Ok(v) = res else { return };
            if let Ok(st) = serde_json::from_value::<GpuStatic>(v["static"].clone()) {
                let complete = v["complete"].as_bool().unwrap_or(false);
                a.data.borrow_mut().statics.insert(st.id.clone(), (st, complete));
                a.on_devices.emit(&());
                a.toast(crate::toast::Toast::new(tr!("Faste data er lest på nytt")).ms(2500));
            }
        });
    }
}

/// De faste dataene for kortet som vises. Når kortet er borte, har prøvene
/// det ikke, og da står ikke kortet som valgt; dataene finnes likevel.
fn card_statics(app: &App) -> Option<GpuStatic> {
    app.statics().or_else(|| {
        let id = app.gpu_info()?.id;
        app.data.borrow().statics.get(&id).map(|s| s.0.clone())
    })
}

/// Navnet i referansetabellen (påskeegg 5 ser etter «RTX 2080»).
fn spec_name(st: &GpuStatic) -> Option<String> {
    st.pci_device_id.and_then(|d| rigg_core::specs::lookup(d, st.pci_subsystem_id)).map(|s| s.name.to_string()).or_else(|| Some(st.name.clone()))
}

fn clocks_panel(app: &Rc<App>) -> Clocks {
    let panel = Panel::new(tr!("Klokker"));
    panel.root.add_css_class("sec");
    panel.aside.append(&label("MHz", &[]));
    let g = gtk::Grid::new();
    g.add_css_class("rtable");
    g.add_css_class("clock-table");
    g.set_column_spacing(16);
    for (c, h) in ["", "", tr!("Nå"), tr!("Maks (driver)")].iter().enumerate() {
        let l = label(h, &["th"]);
        if c >= 2 {
            l.set_xalign(1.0);
        }
        g.attach(&l, c as i32, 0, 1, 1);
    }
    let mut rows = Vec::new();
    for (r, name) in [tr!("Kjerne"), "SM", tr!("Minne"), tr!("Video")].iter().enumerate() {
        let cls: &[&str] = if r == 3 { &["td", "last"] } else { &["td"] };
        let n = label(name, cls);
        let bar = Bar::new(app, true);
        // Hopper rett til ny verdi: under last (P0) skifter klokkene hvert
        // sekund, og fire stolper som glir ga mange bilder i sekundet (8A).
        bar.set_still(true);
        bar.set_hexpand(true);
        bar.set_valign(gtk::Align::Center);
        let bb = hbox(0, cls);
        bb.add_css_class("bar-cell");
        bb.set_hexpand(true);
        bb.append(&bar);
        let now = label("–", cls);
        now.add_css_class("num");
        now.set_xalign(1.0);
        now.set_width_chars(6);
        let max = label("–", cls);
        max.add_css_class("num");
        max.set_xalign(1.0);
        g.attach(&n, 0, r as i32 + 1, 1, 1);
        g.attach(&bb, 1, r as i32 + 1, 1, 1);
        g.attach(&now, 2, r as i32 + 1, 1, 1);
        g.attach(&max, 3, r as i32 + 1, 1, 1);
        rows.push((bar, now, max));
    }
    panel.body.append(&g);
    let note = label("", &["group-note"]);
    note.set_wrap(true);
    panel.body.append(&note);
    Clocks { panel, rows, note }
}

/// Øyet og kopiknappen ved UUID-en (som demoen).
fn uuid_buttons(st: &Rc<State>, p: &SectionPanel) {
    let Some(r) = p.row("uuid") else { return };
    if card_statics(&st.app).and_then(|s| s.uuid).is_none() {
        return;
    }
    r.value.add_css_class("uuid");
    let shown = st.show_uuid.get();
    let eye = crate::widgets::button(Some(if shown { "eye-off" } else { "eye" }), None, &["flat", "sm", "icon-only"]);
    let t = if shown { tr!("Skjul UUID") } else { tr!("Vis UUID") };
    eye.set_tooltip_text(Some(t));
    eye.update_property(&[gtk::accessible::Property::Label(t)]);
    let s2 = Rc::downgrade(st);
    eye.connect_clicked(move |b| {
        if let Some(s) = s2.upgrade() {
            let shown = !s.show_uuid.get();
            s.show_uuid.set(shown);
            let t = if shown { tr!("Skjul UUID") } else { tr!("Vis UUID") };
            b.set_child(Some(&crate::widgets::Icon::new(if shown { "eye-off" } else { "eye" }, 14)));
            b.set_tooltip_text(Some(t));
            b.update_property(&[gtk::accessible::Property::Label(t)]);
            s.refresh();
        }
    });
    let cp = crate::widgets::button(Some("copy"), None, &["flat", "sm", "icon-only"]);
    cp.set_tooltip_text(Some(tr!("Kopier UUID")));
    cp.update_property(&[gtk::accessible::Property::Label(tr!("Kopier UUID"))]);
    let a = st.app.clone();
    cp.connect_clicked(move |b| {
        if let (Some(u), Some(d)) = (card_statics(&a).and_then(|s| s.uuid), gtk::gdk::Display::default()) {
            d.clipboard().set_text(&u);
            crate::rapport_ui::confirm_done(b, "copy", tr!("UUID kopiert"));
        }
    });
    r.value_box.append(&eye);
    r.value_box.append(&cp);
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "gc-page"]);
    let content = gtk::Stack::new();
    content.set_vhomogeneous(false);
    content.set_hhomogeneous(false);
    // Tomtilstanden (ingen driver, eller driveren er oppdatert uten data):
    // som `rows::empty`, men tittelen og teksten byttes etter tilstanden.
    let e = vbox(8, &["empty", "gc-empty"]);
    e.set_valign(gtk::Align::Center);
    e.set_halign(gtk::Align::Center);
    e.set_vexpand(true);
    e.append(&crate::widgets::Icon::new("gpu", 32));
    let et = label("", &["empty-title"]);
    et.set_xalign(0.5);
    e.append(&et);
    let ex = label("", &[]);
    ex.set_xalign(0.5);
    ex.set_justify(gtk::Justification::Center);
    ex.set_wrap(true);
    ex.set_max_width_chars(60);
    e.append(&ex);
    content.add_named(&e, Some("tom"));
    let view = Sections::new();
    let clocks = clocks_panel(app);
    view.custom.borrow_mut().insert("klokker", clocks.panel.root.clone().upcast());
    content.add_named(&view.masonry, Some("kort"));
    root.append(&content);
    let st = Rc::new(State { app: app.clone(), visible: Cell::new(false), show_uuid: Cell::new(false), view, clocks, content, empty: e, empty_title: et, empty_text: ex, procs: RefCell::new(None), proc_timer: RefCell::new(None) });
    {
        let w = Rc::downgrade(&st);
        *st.view.on_build.borrow_mut() = Some(Box::new(move |id, p| {
            if id == "driver"
                && let Some(s) = w.upgrade()
            {
                uuid_buttons(&s, p);
            }
        }));
    }
    // Knappene i toppfeltet: Lagre rapport og Kopier.
    let actions = hbox(2, &[]);
    actions.append(&crate::rapport_ui::save_button(app));
    let w = Rc::downgrade(&st);
    let copy = crate::rapport_ui::copy_button(app, move |fmt: ReportFormat| {
        let s = w.upgrade()?;
        let (secs, _) = s.sections(s.show_uuid.get())?;
        let src = if s.app.data.borrow().backend == "nvml" { tr!("NVML og sysfs") } else { tr!("testdata") };
        let title = trf!("{app} – Grafikkort ({kilde}, {dato})", app = rigg_core::version::NAME, kilde = src, dato = crate::rapport_ui::stamp_long(rigg_core::now_ms()));
        let note = if s.show_uuid.get() || card_statics(&s.app).and_then(|x| x.uuid).is_none() { String::new() } else { tr!("UUID maskert").to_string() };
        Some((report::render_page(&title, &secs, fmt), note))
    });
    actions.append(&copy);

    let refresh = {
        let w = Rc::downgrade(&st);
        move || {
            if let Some(s) = w.upgrade() {
                s.refresh();
            }
        }
    };
    let r = Rc::new(refresh);
    let r1 = r.clone();
    app.on_telemetry.connect(move |_| r1());
    let r2 = r.clone();
    app.on_devices.connect(move |_| r2());
    let r3 = r.clone();
    app.on_settings.connect(move |_| r3());
    let r4 = r.clone();
    app.on_conn.connect(move |_| r4());
    Box::new(Grafikkort { root, actions, st })
}
