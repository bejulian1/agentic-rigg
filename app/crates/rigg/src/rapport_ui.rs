//! «Kopier som tekst / Markdown» og «Lagre rapport …» (produktspesifikasjonen
//! 3.6) på Grafikkort, System og i Innstillinger › Om, og visningen av
//! seksjonene fra `rigg_core::report` som paneler.
//!
//! Grunnmuren kaller [`about_row`] fra `settings/om.rs`. Grafikkort og System
//! bygger panelene med [`SectionPanel`], så det som står på siden er det samme
//! som kopieres og lagres.

use crate::settings::Row;
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{Icon, Panel, hbox, label, set_text, vbox};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::report::{self, Item, ReportFormat, ReportInput, Section};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Merket i overskriften på spesifikasjonstabellen.
pub const REFERENCE: &str = "Referanse, ikke målt";
const REFERENCE_TIP: &str = "Tallene kommer fra en innebygd tabell slått opp på PCI-ID. De gjelder brikken og referansekortet og er ikke målt på denne maskinen.";

/// Tidssonen akkurat nå (sekunder fra UTC).
pub fn utc_offset(ms: i64) -> i64 {
    let t: libc::time_t = (ms / 1000) as libc::time_t;
    // SAFETY: localtime_r skriver bare i `tm`, som vi eier.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff
}

/// «7. nov. 2026 14:32».
pub fn stamp_long(ms: i64) -> String {
    let t = crate::app::local_time(ms);
    format!("{}. {} {} {}", t.day, rigg_core::fmt::MON_SHORT[(t.month - 1) as usize], t.year, rigg_core::fmt::time(t, false))
}

// ---------------------------------------------------------------- Seksjoner som paneler

/// Rad i et panel: etiketten, verdien og boksen rundt verdien (sidene legger
/// til knapper der, som øyet ved UUID).
#[derive(Clone)]
pub struct KvRefs {
    pub value: gtk::Label,
    pub value_box: gtk::Box,
    info: Icon,
}

/// Et panel bygd fra en seksjon. Oppdateres i stedet for å bygges på nytt så
/// lenge radene er de samme ([`signature`]).
pub struct SectionPanel {
    pub panel: Panel,
    pub sig: String,
    pub kv: HashMap<String, KvRefs>,
    tables: Vec<Vec<Vec<gtk::Label>>>,
    notes: Vec<gtk::Label>,
}

/// Hvilke rader og tabeller en seksjon har. Er den lik, holder det å oppdatere tekstene.
pub fn signature(s: &Section) -> String {
    let mut out = format!("{}|{}|{:?}", s.id, s.title, s.aside);
    for i in &s.items {
        match i {
            Item::Kv(k) => out.push_str(&format!("|k:{}:{}", k.id, k.k)),
            Item::Sub(t) => out.push_str(&format!("|s:{t}")),
            Item::Table(t) => out.push_str(&format!("|t:{}:{}x{}:{}", t.id, t.rows.len(), t.head.len(), t.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>().join(","))),
            Item::Note(_) => out.push_str("|n"),
            Item::Text(_) => out.push_str("|x"),
        }
    }
    out
}

fn kv_id(k: &report::Kv) -> String {
    // Avledede rader i spesifikasjonen deler ID; etiketten skiller dem.
    format!("{}:{}", k.id, k.k)
}

fn set_value(r: &KvRefs, k: &report::Kv) {
    set_text(&r.value, &k.text());
    crate::widgets::class(&r.value, "na", k.missing());
    let tip = k.info.as_deref();
    if r.value.tooltip_text().as_deref() != tip {
        r.value.set_tooltip_text(tip);
        r.info.set_tooltip_text(tip);
    }
    // Ikonet står der det finnes en forklaring på verdien eller på hvorfor den
    // mangler; bak «–» som ikke er lest ennå, står bare verktøytipset.
    r.info.set_visible(tip.is_some() && (k.v.is_some() || k.na.is_some()));
}

impl SectionPanel {
    pub fn new(s: &Section) -> SectionPanel {
        let panel = Panel::new(&s.title);
        panel.root.add_css_class("sec");
        panel.root.add_css_class(&format!("sec-{}", s.id));
        if let Some(a) = &s.aside {
            if a == REFERENCE {
                let t = crate::widgets::rows::tag(REFERENCE, Some("info"), "quiet");
                t.set_tooltip_text(Some(REFERENCE_TIP));
                panel.aside.append(&t);
            } else {
                panel.aside.append(&label(a, &[]));
            }
        }
        let mut me = SectionPanel { panel, sig: signature(s), kv: HashMap::new(), tables: vec![], notes: vec![] };
        let mut grid: Option<(gtk::Grid, i32)> = None;
        // Den siste raden før en tabell, merknad eller slutten har ingen linje under.
        let mut last: Option<(gtk::Label, gtk::Box)> = None;
        let end_grid = |last: &mut Option<(gtk::Label, gtk::Box)>| {
            if let Some((k, v)) = last.take() {
                k.add_css_class("last");
                v.add_css_class("last");
            }
        };
        for item in &s.items {
            match item {
                Item::Kv(k) => {
                    let (g, r) = grid.get_or_insert_with(|| {
                        let g = gtk::Grid::new();
                        g.add_css_class("kv");
                        g.add_css_class("sec-kv");
                        me.panel.body.append(&g);
                        (g, 0)
                    });
                    let kl = label(&k.k, &["k", "kv-row"]);
                    kl.set_valign(gtk::Align::Fill);
                    kl.set_yalign(0.5);
                    let vb = hbox(4, &["kv-row"]);
                    vb.set_hexpand(true);
                    let vl = label("", &["v"]);
                    vl.set_selectable(true);
                    vl.set_focusable(false);
                    vl.set_wrap(true);
                    vl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    vl.set_xalign(0.0);
                    vb.append(&vl);
                    let info = Icon::new("info", 12);
                    info.add_css_class("info-mark");
                    info.set_valign(gtk::Align::Center);
                    vb.append(&info);
                    g.attach(&kl, 0, *r, 1, 1);
                    g.attach(&vb, 1, *r, 1, 1);
                    *r += 1;
                    last = Some((kl.clone(), vb.clone()));
                    let refs = KvRefs { value: vl, value_box: vb, info };
                    set_value(&refs, k);
                    me.kv.insert(kv_id(k), refs);
                }
                Item::Sub(t) => {
                    let (g, r) = grid.get_or_insert_with(|| {
                        let g = gtk::Grid::new();
                        g.add_css_class("kv");
                        g.add_css_class("sec-kv");
                        me.panel.body.append(&g);
                        (g, 0)
                    });
                    let l = label(t, &["kv-sub"]);
                    if *r == 0 {
                        l.add_css_class("first");
                    }
                    g.attach(&l, 0, *r, 2, 1);
                    *r += 1;
                    last = None;
                }
                Item::Table(t) => {
                    end_grid(&mut last);
                    grid = None;
                    let g = gtk::Grid::new();
                    g.add_css_class("rtable");
                    g.set_column_spacing(16);
                    let mut cells = Vec::new();
                    // Tekstkolonnen med det lengste innholdet tar plassen som er igjen.
                    let wide = (0..t.head.len())
                        .filter(|c| !t.end.get(*c).copied().unwrap_or(false))
                        .max_by_key(|c| t.rows.iter().map(|r| r.get(*c).map(|x| x.chars().count()).unwrap_or(0)).max().unwrap_or(0))
                        .unwrap_or(0);
                    for (c, h) in t.head.iter().enumerate() {
                        let l = label(h, &["th"]);
                        if t.end.get(c).copied().unwrap_or(false) {
                            l.set_xalign(1.0);
                        }
                        l.set_hexpand(c == wide);
                        g.attach(&l, c as i32, 0, 1, 1);
                    }
                    for (ri, row) in t.rows.iter().enumerate() {
                        let mut rl = Vec::new();
                        for (c, v) in row.iter().enumerate() {
                            let l = label(v, &["td", "num"]);
                            if t.end.get(c).copied().unwrap_or(false) {
                                l.set_xalign(1.0);
                            }
                            if ri + 1 == t.rows.len() {
                                l.add_css_class("last");
                            }
                            l.set_selectable(c == 0 && t.id == "prosesser");
                            g.attach(&l, c as i32, ri as i32 + 1, 1, 1);
                            rl.push(l);
                        }
                        cells.push(rl);
                    }
                    me.panel.body.append(&g);
                    me.tables.push(cells);
                }
                Item::Note(n) => {
                    end_grid(&mut last);
                    grid = None;
                    let l = label(n, &["group-note"]);
                    l.set_wrap(true);
                    l.set_visible(!n.is_empty());
                    me.panel.body.append(&l);
                    me.notes.push(l);
                }
                Item::Text(_) => {}
            }
        }
        end_grid(&mut last);
        me
    }

    /// Nye tekster. Gir `false` når radene ikke er de samme (bygg panelet på nytt).
    pub fn update(&self, s: &Section) -> bool {
        if signature(s) != self.sig {
            return false;
        }
        let (mut ti, mut ni) = (0, 0);
        for item in &s.items {
            match item {
                Item::Kv(k) => {
                    if let Some(r) = self.kv.get(&kv_id(k)) {
                        set_value(r, k);
                    }
                }
                Item::Table(t) => {
                    if let Some(cells) = self.tables.get(ti) {
                        for (row, labels) in t.rows.iter().zip(cells) {
                            for (v, l) in row.iter().zip(labels) {
                                set_text(l, v);
                            }
                        }
                    }
                    ti += 1;
                }
                Item::Note(n) => {
                    if let Some(l) = self.notes.get(ni) {
                        set_text(l, n);
                        l.set_visible(!n.is_empty());
                    }
                    ni += 1;
                }
                _ => {}
            }
        }
        true
    }

    pub fn row(&self, id: &str) -> Option<&KvRefs> {
        self.kv.iter().find(|(k, _)| k.split(':').next() == Some(id)).map(|(_, v)| v)
    }
}

/// Kalles når en seksjon er bygd (Grafikkort legger til UUID-knappene).
pub type BuildFn = dyn Fn(&'static str, &SectionPanel);

/// Panelene på en side i spalter, med de faste panelene (som sidene tegner
/// selv) på plassen sin.
pub struct Sections {
    pub masonry: crate::widgets::masonry::Masonry,
    panels: RefCell<HashMap<&'static str, SectionPanel>>,
    order: RefCell<Vec<&'static str>>,
    /// Paneler siden tegner selv, etter seksjons-ID.
    pub custom: RefCell<HashMap<&'static str, gtk::Widget>>,
    /// Kalles når et panel er bygd på nytt (sidene legger til knapper).
    pub on_build: crate::widgets::Slot<BuildFn>,
}

impl Sections {
    pub fn new() -> Rc<Sections> {
        let masonry = crate::widgets::masonry::Masonry::new();
        masonry.set_gap(8);
        masonry.set_breakpoints(760, 1560, 6);
        masonry.set_hexpand(true);
        Rc::new(Sections {
            masonry,
            panels: RefCell::new(HashMap::new()),
            order: RefCell::new(vec![]),
            custom: RefCell::new(HashMap::new()),
            on_build: RefCell::new(None),
        })
    }

    /// Viser seksjonene. Seksjoner med et eget panel i `custom` tegnes ikke her.
    pub fn show(&self, secs: &[Section]) {
        let order: Vec<&'static str> = secs.iter().map(|s| s.id).collect();
        let mut rebuilt = false;
        {
            let mut panels = self.panels.borrow_mut();
            let custom = self.custom.borrow();
            for s in secs {
                if custom.contains_key(s.id) {
                    continue;
                }
                let ok = panels.get(s.id).is_some_and(|p| p.update(s));
                if !ok {
                    let p = SectionPanel::new(s);
                    if let Some(f) = self.on_build.borrow().as_ref() {
                        f(s.id, &p);
                    }
                    panels.insert(s.id, p);
                    rebuilt = true;
                }
            }
            panels.retain(|k, _| order.contains(k));
        }
        if rebuilt || *self.order.borrow() != order {
            self.masonry.clear();
            let panels = self.panels.borrow();
            let custom = self.custom.borrow();
            for id in &order {
                if let Some(w) = custom.get(id) {
                    self.masonry.append(w);
                } else if let Some(p) = panels.get(id) {
                    self.masonry.append(&p.panel.root);
                }
            }
            *self.order.borrow_mut() = order;
        }
    }

}

// ---------------------------------------------------------------- Kopier

/// Hake på knappen i 1,2 s etter kopier eller lagre (demoens `confirmDone`).
pub fn confirm_done(b: &gtk::Button, icon: &str, tip: &str) {
    let before = b.tooltip_text().map(|t| t.to_string());
    b.set_child(Some(&Icon::new("check", 16)));
    b.add_css_class("is-done");
    b.set_tooltip_text(Some(tip));
    b.announce(tip, gtk::AccessibleAnnouncementPriority::Medium);
    let (b, icon) = (b.clone(), icon.to_string());
    glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
        b.set_child(Some(&Icon::new(&icon, 16)));
        b.remove_css_class("is-done");
        b.set_tooltip_text(before.as_deref());
    });
}

/// Knappen «Kopier» med menyen «Kopier som tekst» og «Kopier som Markdown».
/// `make` gir teksten og en merknad til bekreftelsen («UUID maskert»).
pub fn copy_button(app: &Rc<App>, make: impl Fn(ReportFormat) -> Option<(String, String)> + 'static) -> gtk::Button {
    let b = crate::widgets::button(Some("copy"), None, &["flat"]);
    crate::widgets::tip(&b, "Kopier", None);
    b.update_property(&[gtk::accessible::Property::Label("Kopier"), gtk::accessible::Property::HasPopup(true)]);
    let make = Rc::new(make);
    let a = app.clone();
    b.connect_clicked(move |btn| {
        let m = crate::widgets::popover::Menu::new(btn);
        m.popover.set_halign(gtk::Align::End);
        for (fmt, text) in [(ReportFormat::Text, "Kopier som tekst"), (ReportFormat::Markdown, "Kopier som Markdown")] {
            let (a, make, btn) = (a.clone(), make.clone(), btn.clone());
            m.item(Some("copy"), text, None, move || {
                let Some((s, note)) = make(fmt) else {
                    a.toast(Toast::new("Ingenting å kopiere ennå").body("Dataene er ikke lest ferdig."));
                    return;
                };
                let Some(d) = gtk::gdk::Display::default() else {
                    a.toast(Toast::new("Kunne ikke kopiere").body("Utklippstavlen er ikke tilgjengelig.").error());
                    return;
                };
                d.clipboard().set_text(&s);
                let lines = s.lines().count();
                let what = if fmt == ReportFormat::Markdown { "Markdown" } else { "tekst" };
                let tip = if note.is_empty() { format!("Kopiert som {what} ({lines} linjer)") } else { format!("Kopiert som {what} ({lines} linjer, {note})") };
                confirm_done(&btn, "copy", &tip);
            });
        }
        m.show();
    });
    b
}

// ---------------------------------------------------------------- Rapporten

fn renderer_name(app: &App) -> Option<String> {
    let r = app.window()?.renderer()?;
    let n = r.type_().name().to_string();
    Some(
        if n.contains("Vulkan") {
            "Vulkan"
        } else if n.contains("Ngl") || n.contains("GL") {
            "GL"
        } else if n.contains("Cairo") {
            "programvare (cairo)"
        } else {
            return Some(n);
        }
        .to_string(),
    )
}

/// Skrivebord og økttype fra vinduets miljø: («ubuntu:GNOME», «Wayland»).
pub fn desktop_env() -> (Option<String>, Option<String>) {
    let desk = std::env::var("XDG_CURRENT_DESKTOP").ok().filter(|s| !s.is_empty());
    let sess = match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("wayland") => Some("Wayland".to_string()),
        Some("x11") => Some("X11".to_string()),
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ if std::env::var_os("WAYLAND_DISPLAY").is_some() => Some("Wayland".to_string()),
        _ if std::env::var_os("DISPLAY").is_some() => Some("X11".to_string()),
        _ => None,
    };
    (desk, sess)
}

/// Samler det rapporten trenger: det vinduet har, og `get_system` og
/// `get_stats` fra tjenesten.
pub fn gather(app: &Rc<App>, include_serials: bool, done: impl FnOnce(ReportInput) + 'static) {
    let a = app.clone();
    app.client.request("get_system", json!({}), move |sys| {
        let system = sys.unwrap_or(Value::Null);
        let b = a.clone();
        a.client.request("get_stats", json!({}), move |stats| {
            let app = b;
            let now = rigg_core::now_ms();
            let (desktop, session) = desktop_env();
            let d = app.data.borrow();
            let mut statics: Vec<_> = d.statics.values().map(|s| s.0.clone()).collect();
            statics.sort_by(|x, y| x.id.cmp(&y.id));
            let mut env = vec![(
                "Vindu".to_string(),
                format!(
                    "GTK {}.{}.{}{}",
                    gtk::major_version(),
                    gtk::minor_version(),
                    gtk::micro_version(),
                    renderer_name(&app).map(|r| format!(" · tegner {r}")).unwrap_or_default()
                ),
            )];
            if let Some(s) = &session {
                env.push(("Økttype".into(), s.clone()));
            }
            let input = ReportInput {
                gpus: serde_json::to_value(&d.gpus).unwrap_or(Value::Null),
                statics: serde_json::to_value(statics).unwrap_or(Value::Null),
                system,
                stats: stats.unwrap_or_else(|_| d.stats.clone()),
                telemetry: d.telemetry.as_ref().map(|t| serde_json::to_value(t.as_ref()).unwrap_or(Value::Null)).unwrap_or(Value::Null),
                service: d.hello.as_ref().map(|h| h["daemon"].clone()).unwrap_or(Value::Null),
                env,
                desktop,
                session,
                include_serials,
                created: stamp_long(now),
                utc_offset_s: utc_offset(now),
            };
            drop(d);
            done(input);
        });
    });
}

/// Åpner mappa med fila markert (Filer), eller bare mappa.
pub fn show_in_files(app: &App, path: &std::path::Path) {
    let uri = gio::File::for_path(path).uri();
    let ok = app.gtk.dbus_connection().is_some_and(|c| {
        let params = (vec![uri.to_string()], "").to_variant();
        c.call(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            None::<&gio::Cancellable>,
            |_| {},
        );
        true
    });
    if !ok {
        let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
        if let Err(e) = gio::AppInfo::launch_default_for_uri(&gio::File::for_path(&dir).uri(), None::<&gio::AppLaunchContext>) {
            app.toast(Toast::new("Kunne ikke åpne mappa").body(&e.to_string()).error());
        }
    }
}

fn save_file(app: &Rc<App>, fmt: ReportFormat, serials: bool) {
    let Some(win) = app.window() else { return };
    let d = gtk::FileDialog::new();
    d.set_title("Lagre rapport");
    let t = crate::app::local_now();
    d.set_initial_name(Some(&format!("agentic-rigg-rapport-{:04}-{:02}-{:02}_{:02}-{:02}.{}", t.year, t.month, t.day, t.hour, t.minute, fmt.ext())));
    let f = gtk::FileFilter::new();
    f.set_name(Some(if fmt == ReportFormat::Markdown { "Markdown" } else { "Tekst" }));
    f.add_pattern(&format!("*.{}", fmt.ext()));
    let list = gio::ListStore::new::<gtk::FileFilter>();
    list.append(&f);
    d.set_filters(Some(&list));
    let a = app.clone();
    d.save(Some(&win), None::<&gio::Cancellable>, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        let b = a.clone();
        gather(&a, serials, move |input| {
            let text = report::render(&input, fmt);
            match rigg_core::write_atomic(&path, text.as_bytes()) {
                Ok(()) => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let (c, p) = (b.clone(), path.clone());
                    b.toast(
                        Toast::new("Rapporten er lagret")
                            .body(&format!("{name}{}", if serials { "" } else { " · uten serienumre og UUID" }))
                            .icon("save")
                            .action("Vis i Filer", move || show_in_files(&c, &p)),
                    );
                }
                Err(e) => b.toast(Toast::new("Kunne ikke lagre rapporten").body(&e.to_string()).error()),
            }
        });
    });
}

/// Valgene før lagring: format og «Ta med serienumre og UUID» (av).
pub fn save_popover(app: &Rc<App>, anchor: &impl IsA<gtk::Widget>) {
    let body = vbox(10, &["report-pop"]);
    body.append(&label("Lagre rapport", &["pop-title"]));
    let sub = label("Program og versjon, operativsystem, driver, skjermkort, prosessor, hovedkort, minne, disker og alle sensorer med nå, min, maks og snitt.", &["pop-sub"]);
    sub.set_wrap(true);
    sub.set_max_width_chars(44);
    body.append(&sub);
    let fmt = crate::widgets::Seg::new(&[("txt", "Tekst"), ("md", "Markdown")], "txt", "Format");
    let row = hbox(10, &[]);
    let fl = label("Format", &["muted"]);
    fl.set_hexpand(true);
    row.append(&fl);
    row.append(&fmt.root);
    body.append(&row);
    let serials = Rc::new(Cell::new(false));
    let s2 = serials.clone();
    let chk = crate::widgets::switchrow::check("Ta med serienumre og UUID", false, move |c| s2.set(c.is_active()));
    body.append(&chk);
    let note = label("Rapporten lagres bare på denne maskinen. Appen sender ingenting ut.", &["pop-sub", "dim"]);
    note.set_wrap(true);
    note.set_max_width_chars(44);
    body.append(&note);
    let go = crate::widgets::button(Some("save"), Some("Velg fil …"), &["suggested"]);
    go.set_halign(gtk::Align::End);
    body.append(&go);
    let p = crate::widgets::popover::popover(anchor, &body);
    p.add_css_class("pop-wide");
    let (a, p2) = (app.clone(), p.clone());
    go.connect_clicked(move |_| {
        let f = if fmt.value().as_deref() == Some("md") { ReportFormat::Markdown } else { ReportFormat::Text };
        p2.popdown();
        save_file(&a, f, serials.get());
    });
    p.popup();
}

/// Knappen «Lagre rapport …» i toppfeltet på Grafikkort og System.
pub fn save_button(app: &Rc<App>) -> gtk::Button {
    let b = crate::widgets::button(Some("save"), None, &["flat"]);
    crate::widgets::tip(&b, "Lagre rapport …", None);
    b.update_property(&[gtk::accessible::Property::Label("Lagre rapport …"), gtk::accessible::Property::HasPopup(true)]);
    let a = app.clone();
    b.connect_clicked(move |b| save_popover(&a, b));
    b
}

/// Raden «Lagre rapport …» i Om › Hjelp.
pub fn about_row(app: &Rc<App>) -> Option<Row> {
    let b = crate::widgets::button(Some("save"), Some("Lagre rapport …"), &[]);
    let a = app.clone();
    b.connect_clicked(move |b| save_popover(&a, b));
    Some(crate::settings::row(
        "Rapport",
        Some("Alt appen vet om maskinen i én fil, som tekst eller Markdown. Serienumre og UUID er ikke med, med mindre du velger det."),
        &b,
        "rapport lagre feilsøking tekst markdown serienummer",
    ))
}
