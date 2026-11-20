//! Oppgavebehandling. Bare den synlige siden ber om prosesser; høyst 100
//! gjenbrukte rader og én forespørsel om gangen. Systemgrafene deler App-data.
use super::{Key, Page};
use crate::state::{App, Conn};
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Spark, class, hbox, label, set_text, vbox};
use gtk::{glib, prelude::*};
use rigg_core::fmt as F;
use rigg_core::process::{ProcessIdentity, ProcessRow};
use rigg_core::proto::ErrorCode;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

const LIMIT: usize = 100;
const COLS: [(&str, &str, i32); 7] = [
    ("name", "Navn", 170),
    ("pid", "PID", 64),
    ("user", "Bruker", 80),
    ("cpu", "CPU", 82),
    ("memory", "RAM", 100),
    ("gpuMemory", "GPU-minne", 100),
    ("state", "Status", 70),
];

struct RowView {
    row: gtk::ListBoxRow,
    cells: Vec<gtk::Label>,
}
struct Summary {
    value: gtk::Label,
    spark: Spark,
    key: &'static str,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Load {
    Live,
    Displayed(usize),
    Fresh,
}

struct State {
    app: Rc<App>,
    visible: Cell<bool>,
    suspended: Cell<bool>,
    paused: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    refresh_retry: RefCell<Option<glib::SourceId>>,
    refresh_until: Cell<Option<Instant>>,
    pending: Cell<Option<Load>>,
    busy: Cell<bool>,
    generation: Cell<u64>,
    snapshot: Cell<Option<u64>>,
    snapshot_lost: Cell<bool>,
    search: gtk::SearchEntry,
    tree: gtk::ToggleButton,
    pause: gtk::ToggleButton,
    list: gtk::ListBox,
    rows: Vec<RowView>,
    data: RefCell<Vec<ProcessRow>>,
    selected: RefCell<Option<ProcessIdentity>>,
    updating: Cell<bool>,
    sort: RefCell<String>,
    descending: Cell<bool>,
    headers: Vec<gtk::Button>,
    offset: Cell<usize>,
    next: Cell<Option<usize>>,
    previous_offsets: RefCell<Vec<usize>>,
    previous: gtk::Button,
    following: gtk::Button,
    count: gtk::Label,
    status: gtk::Label,
    selection: gtk::Label,
    stop: gtk::Button,
    empty: gtk::Label,
    stale: Cell<bool>,
    summaries: Vec<Summary>,
}

pub struct Oppgaver {
    root: gtk::Box,
    st: Rc<State>,
}
impl Page for Oppgaver {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
    fn scrolls(&self) -> bool {
        false
    }
    fn show(&self, _: Option<&str>) {
        self.st.visible.set(true);
        self.st.refresh_summary();
        self.st.start();
    }
    fn hide(&self) {
        self.st.visible.set(false);
        self.st.cancel_timer();
    }
    fn window_hidden(&self, hidden: bool) {
        if self.st.suspended.replace(hidden) == hidden {
            return;
        }
        if hidden {
            self.st.cancel_timer();
        } else {
            self.st.start();
        }
    }
    fn on_key(&self, k: &Key) -> bool {
        if k.ctrl && !k.alt && matches!(k.key, gtk::gdk::Key::f | gtk::gdk::Key::F) {
            self.st.search.grab_focus();
            return true;
        }
        if !k.ctrl && !k.alt && k.key == gtk::gdk::Key::F5 {
            self.st.refresh_now();
            return true;
        }
        false
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(t) = self.timer.get_mut().take() {
            t.remove();
        }
        if let Some(t) = self.refresh_retry.get_mut().take() {
            t.remove();
        }
    }
}
impl State {
    fn active(&self) -> bool {
        self.visible.get() && !self.suspended.get() && !self.paused.get()
    }
    fn cancel_refresh(&self) {
        self.refresh_until.set(None);
        if let Some(t) = self.refresh_retry.borrow_mut().take() {
            t.remove();
        }
    }
    fn cancel_timer(&self) {
        self.cancel_refresh();
        self.pending.set(None);
        self.generation.set(self.generation.get().wrapping_add(1));
        if let Some(t) = self.timer.borrow_mut().take() {
            t.remove();
        }
    }
    fn start(self: &Rc<Self>) {
        if !self.active() || self.app.conn.get() != Conn::Connected {
            return;
        }
        self.load(Load::Live);
        if self.timer.borrow().is_some() {
            return;
        }
        let w = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local(2, move || {
            let Some(s) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            s.load(Load::Live);
            glib::ControlFlow::Continue
        });
        *self.timer.borrow_mut() = Some(id);
    }
    fn request_page(self: &Rc<Self>, load: Load) {
        if !self.visible.get() || self.suspended.get() || self.app.conn.get() != Conn::Connected {
            return;
        }
        self.cancel_refresh();
        self.generation.set(self.generation.get().wrapping_add(1));
        self.stale.set(true);
        self.update_selection();
        self.refresh_until.set(Some(Instant::now() + Duration::from_secs(5)));
        self.load(load);
    }
    fn refresh_now(self: &Rc<Self>) {
        self.request_page(Load::Fresh);
    }
    fn retry_refresh(self: &Rc<Self>, load: Load) {
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let Some(s) = weak.upgrade() else { return };
            s.refresh_retry.borrow_mut().take();
            s.load(load);
        });
        *self.refresh_retry.borrow_mut() = Some(id);
    }
    fn changed(self: &Rc<Self>) {
        self.previous_offsets.borrow_mut().clear();
        self.request_page(Load::Displayed(0));
    }
    fn load(self: &Rc<Self>, load: Load) {
        if !self.visible.get()
            || self.suspended.get()
            || self.app.conn.get() != Conn::Connected
            || (load == Load::Live && (self.paused.get() || self.refresh_until.get().is_some()))
        {
            return;
        }
        if self.busy.replace(true) {
            if load != Load::Live {
                self.pending.set(Some(load));
            }
            return;
        }
        let generation = self.generation.get();
        let (offset, snapshot, newer_than) = match load {
            Load::Live => (self.offset.get(), None, None),
            Load::Displayed(offset) => (offset, self.snapshot.get(), None),
            Load::Fresh => (self.offset.get(), None, self.snapshot.get()),
        };
        let lost = matches!(load, Load::Displayed(_)) && self.snapshot_lost.get();
        let args = json!({"filter": self.search.text().to_string(), "sort": &*self.sort.borrow(),
            "descending": self.descending.get(), "tree": self.tree.is_active(),
            "offset": if lost { 0 } else { offset }, "limit": LIMIT,
            "snapshotId": snapshot, "newerThan": newer_than});
        let w = Rc::downgrade(self);
        self.app.client.request("get_tasks", args, move |result| {
            let Some(s) = w.upgrade() else { return };
            s.busy.set(false);
            if generation != s.generation.get() {
                if let Some(pending) = s.pending.take() {
                    s.load(pending);
                }
                return;
            }
            match result {
                Ok(v) if v["loading"] == true => {
                    if s.snapshot.get().is_none() && !s.snapshot_lost.get() {
                        s.apply(&v);
                    } else {
                        s.stale.set(true);
                        s.update_selection();
                        set_text(&s.status, "Henter ny prøve …");
                    }
                    if load != Load::Live {
                        if s.refresh_until.get().is_some_and(|until| Instant::now() < until) {
                            s.retry_refresh(load);
                        } else {
                            s.cancel_refresh();
                            set_text(&s.status, "Tjenesten bruker lengre tid. Prøv igjen med F5.");
                        }
                    }
                }
                Ok(v) => {
                    s.cancel_refresh();
                    let replaced = lost || v["snapshotChanged"] == true;
                    let mut previous = s.previous_offsets.borrow_mut();
                    if replaced || v["offset"].as_u64() != Some(offset as u64) {
                        previous.clear();
                    } else if matches!(load, Load::Displayed(_)) {
                        if offset > s.offset.get() {
                            previous.push(s.offset.get());
                        } else if previous.last() == Some(&offset) {
                            previous.pop();
                        }
                    }
                    drop(previous);
                    s.apply(&v);
                    if replaced {
                        s.app.toast(crate::toast::Toast::new(
                            "Prosessbildet er ikke lenger tilgjengelig",
                        ).body("Viser første side i et nytt prosessbilde."));
                    }
                }
                Err(e) => {
                    s.cancel_refresh();
                    s.stale.set(true);
                    if matches!(
                        e.code,
                        ErrorCode::UnknownOp | ErrorCode::NotSupported | ErrorCode::NotImplemented
                    ) {
                        s.cancel_timer();
                        set_text(
                            &s.status,
                            "Oppgavebehandling støttes ikke av denne tjenesteversjonen.",
                        );
                        set_text(
                            &s.empty,
                            "Oppdater tjenesten for å vise oppgaver. Prøv igjen med F5.",
                        );
                        s.empty.set_visible(s.data.borrow().is_empty());
                        s.update_selection();
                        return;
                    }
                    set_text(
                        &s.status,
                        &format!("Kunne ikke hente oppgaver · {}", e.message),
                    );
                    if s.data.borrow().is_empty() {
                        set_text(
                            &s.empty,
                            "Oppgaver er ikke tilgjengelige. Prøv igjen med F5.",
                        );
                        s.empty.set_visible(true);
                    }
                    s.update_selection();
                }
            }
        });
    }
    fn apply(&self, v: &Value) {
        let parsed = serde_json::from_value::<Vec<ProcessRow>>(v["rows"].clone());
        let Ok(mut data) = parsed else {
            self.stale.set(true);
            set_text(
                &self.status,
                "Tjenesten returnerte ugyldige oppgavedata. Prøv igjen med F5.",
            );
            self.update_selection();
            return;
        };
        data.truncate(LIMIT);
        self.stale.set(
            v["stale"].as_bool().unwrap_or(false)
                || v["loading"].as_bool().unwrap_or(false)
                || v["error"].as_str().is_some_and(|s| !s.is_empty()),
        );
        if v["loading"] != true {
            self.snapshot.set(v["snapshotId"].as_u64());
            self.snapshot_lost.set(false);
        }
        let selected = *self.selected.borrow();
        self.updating.set(true);
        self.list.unselect_all();
        let mut restored = None;
        for (i, view) in self.rows.iter().enumerate() {
            let Some(row) = data.get(i) else {
                view.row.set_visible(false);
                continue;
            };
            view.row.set_visible(true);
            let name = if self.tree.is_active() && row.depth > 0 {
                format!(
                    "{}↳ {}",
                    "  ".repeat(usize::from(row.depth.min(8))),
                    row.name
                )
            } else {
                row.name.clone()
            };
            view.row.set_widget_name(&format!(
                "task-{}-{}",
                row.identity.pid, row.identity.start_ticks
            ));
            let user = row
                .user
                .clone()
                .or_else(|| row.uid.map(|uid| uid.to_string()))
                .unwrap_or_else(|| "–".into());
            let values = [
                name,
                row.identity.pid.to_string(),
                user,
                F::pct(row.cpu_percent, F::dec(1)),
                bytes(row.rss_bytes),
                bytes(row.gpu_vram_bytes),
                state_name(&row.state).into(),
            ];
            for (cell, text) in view.cells.iter().zip(values) {
                set_text(cell, &text);
            }
            view.cells[0].set_tooltip_text(Some(&format!(
                "{}\nPID {} · forelder {}",
                row.name, row.identity.pid, row.ppid
            )));
            view.cells[3].set_tooltip_text(Some("100 % tilsvarer én logisk CPU. Flertrådede oppgaver kan overstige 100 %. Første prøve vises som –."));
            view.cells[4].set_tooltip_text(Some("Resident minne (RSS), inkludert delte sider. Radene kan ikke summeres til systemets minnebruk."));
            view.cells[5].set_tooltip_text(Some("Videominne rapportert per prosess av NVIDIA-driveren. – betyr at målingen ikke er tilgjengelig, ikke null."));
            if selected.as_ref().is_some_and(|id| {
                id.pid == row.identity.pid && id.start_ticks == row.identity.start_ticks
            }) {
                restored = Some(row.identity);
                self.list.select_row(Some(&view.row));
            }
        }
        *self.data.borrow_mut() = data;
        *self.selected.borrow_mut() = restored;
        self.updating.set(false);
        self.offset.set(v["offset"].as_u64().unwrap_or(0) as usize);
        self.next.set(v["nextOffset"].as_u64().map(|n| n as usize));
        self.previous.set_sensitive(self.offset.get() > 0);
        self.following.set_sensitive(self.next.get().is_some());
        let matched = v["matched"].as_u64().unwrap_or(0);
        let total = v["total"].as_u64().unwrap_or(matched);
        let n = self.data.borrow().len();
        let range = if n == 0 {
            "0".into()
        } else {
            format!("{}–{}", self.offset.get() + 1, self.offset.get() + n)
        };
        set_text(
            &self.count,
            &format!("{range} av {matched} · {total} oppgaver"),
        );
        self.empty.set_visible(n == 0);
        set_text(
            &self.empty,
            if v["loading"].as_bool() == Some(true) {
                "Leser oppgaver … CPU blir tilgjengelig etter neste prøve."
            } else {
                "Ingen oppgaver passer søket."
            },
        );
        let mut message = if v["loading"].as_bool() == Some(true) {
            "Henter ny prøve …".into()
        } else if self.paused.get() {
            "Oppdatering pauset · F5 oppdaterer én gang".into()
        } else if self.stale.get() {
            format!(
                "Eldre prøve · {} s",
                v["ageMs"].as_u64().unwrap_or(0) / 1000
            )
        } else {
            format!(
                "Oppdateres hvert 2. sekund · prøve {} s gammel",
                v["ageMs"].as_u64().unwrap_or(0) / 1000
            )
        };
        if let Some(error) = v["error"].as_str().filter(|s| !s.is_empty()) {
            message = format!("{message} · {error}");
        }
        let unreadable = v["unreadable"].as_u64().unwrap_or(0);
        if unreadable > 0 {
            message.push_str(&format!(" · {unreadable} utilgjengelige"));
        }
        if let Some(note) = v["gpuNote"].as_str().filter(|n| !n.is_empty()) {
            self.headers[5].set_tooltip_text(Some(&format!("Sorter etter GPU-minne. {note}")));
        }
        self.status.set_tooltip_text(Some(&message));
        set_text(&self.status, &message);
        self.update_selection();
    }
    fn selected_row(&self) -> Option<ProcessRow> {
        let id = self.selected.borrow();
        self.data
            .borrow()
            .iter()
            .find(|r| {
                id.as_ref().is_some_and(|id| {
                    id.pid == r.identity.pid && id.start_ticks == r.identity.start_ticks
                })
            })
            .cloned()
    }
    fn update_selection(&self) {
        let row = self.selected_row();
        let allowed = row.as_ref().is_some_and(|r| r.can_terminate)
            && !self.stale.get()
            && self.app.client.is_connected();
        self.stop.set_sensitive(allowed);
        let text = row
            .as_ref()
            .map(|r| format!("{} · PID {}", r.name, r.identity.pid))
            .unwrap_or_else(|| "Velg en oppgave for å avslutte den".into());
        self.selection.set_tooltip_text(
            row.as_ref()
                .and_then(|r| r.protected_reason.as_deref())
                .or(Some(&text)),
        );
        set_text(&self.selection, &text);
        let reason = row
            .as_ref()
            .and_then(|r| r.protected_reason.as_deref())
            .unwrap_or(if self.stale.get() {
                "Oppdater oppgavelisten før du avslutter en oppgave."
            } else {
                "Send normal avslutning (SIGTERM) til den valgte oppgaven."
            });
        self.stop.set_tooltip_text(Some(reason));
    }
    fn confirm(self: &Rc<Self>) {
        let Some(row) = self
            .selected_row()
            .filter(|r| r.can_terminate && !self.stale.get())
        else {
            return;
        };
        let Some((window, root)) =
            crate::shortcuts::dialog(&self.app, "Avslutt oppgave?", 440, 250)
        else {
            return;
        };
        let body = vbox(12, &["dialog-body"]);
        let name = label(
            &format!("{} · PID {}", row.name, row.identity.pid),
            &["bold"],
        );
        name.set_wrap(true);
        body.append(&name);
        let detail = label(
            "Oppgaven får en vanlig forespørsel om å avslutte (SIGTERM). Ulagret arbeid kan gå tapt.",
            &[],
        );
        detail.set_wrap(true);
        body.append(&detail);
        root.append(&body);
        let buttons = hbox(8, &["dialog-foot"]);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Avbryt");
        let yes = gtk::Button::with_label("Avslutt oppgave");
        yes.add_css_class("destructive");
        buttons.append(&cancel);
        buttons.append(&yes);
        root.append(&buttons);
        let win = window.clone();
        cancel.connect_clicked(move |_| win.close());
        let w = Rc::downgrade(self);
        let win = window.clone();
        yes.connect_clicked(move |b| {
            b.set_sensitive(false); win.close();
            let Some(s) = w.upgrade() else { return };
            let a = s.app.clone(); let weak = Rc::downgrade(&s);
            s.app.client.request("terminate_task", json!({"pid": row.identity.pid, "expectedStartTicks": row.identity.start_ticks, "confirmed": true}), move |res| {
                match res {
                    Ok(_) => a.toast(crate::toast::Toast::new("Avslutning sendt").body("Oppgaven har mottatt SIGTERM. Den kan bruke litt tid på å avslutte.")),
                    Err(e) => a.toast(crate::toast::Toast::new("Oppgaven ble ikke avsluttet").body(&e.message).error()),
                }
                if let Some(s) = weak.upgrade() {
                    s.request_page(Load::Displayed(s.offset.get()));
                }
            });
        });
        window.present();
        cancel.grab_focus();
    }
    fn refresh_summary(&self) {
        if !self.visible.get() || self.suspended.get() {
            return;
        }
        let now = rigg_core::now_ms();
        for summary in &self.summaries {
            let key = if summary.key == "gpu" {
                self.app.gpu_key("gpuUtil")
            } else {
                Some(summary.key.into())
            };
            let value = key.as_ref().and_then(|k| self.app.value(k));
            set_text(&summary.value, &F::pct(value, F::dec(0)));
            let points = key.as_ref().map(|k| self.app.recent(k)).unwrap_or_default();
            summary.spark.set_points(
                &points,
                now,
                SparkOpts {
                    span_ms: 60_000,
                    fixed: Some((0.0, 100.0)),
                    min_span: 0.0,
                },
            );
        }
    }
}
fn bytes(v: Option<u64>) -> String {
    F::bytes(v.map(|n| n as f64), F::dec(1))
}
fn state_name(state: &str) -> &str {
    match state {
        "R" => "Kjører",
        "S" => "Sover",
        "D" => "Venter",
        "T" | "t" => "Stoppet",
        "Z" => "Zombie",
        "I" => "Inaktiv",
        other => other,
    }
}
fn cell(text: &str, width: i32, first: bool) -> gtk::Label {
    let l = label(text, &["task-cell", "num"]);
    l.set_width_request(width);
    l.set_hexpand(first);
    l.set_xalign(if first { 0.0 } else { 1.0 });
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "task-page"]);
    root.set_vexpand(true);
    let summary_row = hbox(8, &["task-summaries"]);
    summary_row.set_homogeneous(true);
    let mut summaries = Vec::new();
    for (title, key, serie, tooltip) in [
        (
            "CPU · hele systemet",
            "cpu/load",
            "load",
            "Samlet CPU-last, 0–100 %. Oppgavenes CPU-prosent bruker én logisk CPU som 100 %.",
        ),
        (
            "RAM · hele systemet",
            "mem/usedPct",
            "mem",
            "Systemets brukte minne. Inkluderer mer enn oppgavenes RSS; delte sider gjør RSS ikke-additivt.",
        ),
        (
            "GPU · valgt kort",
            "gpu",
            "memctl",
            "Samlet GPU-last for kortet valgt i sidefeltet. Per-prosess GPU-prosent er ikke tilgjengelig.",
        ),
    ] {
        let panel = vbox(5, &["task-summary"]);
        panel.set_tooltip_text(Some(tooltip));
        panel.append(&label(title, &["muted", "sm"]));
        let line = hbox(12, &[]);
        let value = label("–", &["task-total", "num"]);
        let spark = Spark::new(serie, 60);
        spark.set_hexpand(true);
        line.append(&value);
        line.append(&spark);
        panel.append(&line);
        summary_row.append(&panel);
        summaries.push(Summary { value, spark, key });
    }
    root.append(&summary_row);
    let controls = hbox(8, &["task-controls"]);
    let search = gtk::SearchEntry::new();
    search.add_css_class("task-search");
    search.set_placeholder_text(Some("Søk etter navn, PID eller bruker"));
    search.set_hexpand(true);
    search.set_search_delay(250);
    search.update_property(&[gtk::accessible::Property::Label("Søk i oppgaver")]);
    let tree = gtk::ToggleButton::with_label("Prosesstre");
    tree.add_css_class("task-tree");
    tree.set_tooltip_text(Some(
        "Vis foreldre og barn. Sortering gjelder innen hver familie; ingen summering av barna.",
    ));
    let pause = gtk::ToggleButton::with_label("Pause");
    pause.add_css_class("task-pause");
    pause.set_tooltip_text(Some(
        "Pause oppgavelisten. Systemgrafene fortsetter å vise felles målinger.",
    ));
    controls.append(&search);
    controls.append(&tree);
    controls.append(&pause);
    root.append(&controls);
    let table = vbox(0, &["task-table"]);
    table.set_vexpand(true);
    let heads = hbox(0, &["task-header"]);
    let mut headers = Vec::new();
    for (i, (_, title, width)) in COLS.iter().enumerate() {
        let b = gtk::Button::new();
        b.add_css_class("task-sort");
        let l = cell(title, *width, i == 0);
        b.set_child(Some(&l));
        b.set_hexpand(i == 0);
        heads.append(&b);
        headers.push(b);
    }
    // Hele tabellen deler én horisontal rulleflate, så kolonnene følger overskriftene.
    let list = gtk::ListBox::new();
    list.add_css_class("task-rows");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let mut rows = Vec::with_capacity(LIMIT);
    for _ in 0..LIMIT {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("task-row");
        let line = hbox(0, &[]);
        let mut cells = Vec::new();
        for (i, (_, _, width)) in COLS.iter().enumerate() {
            let l = cell("", *width, i == 0);
            line.append(&l);
            cells.push(l);
        }
        row.set_child(Some(&line));
        row.set_visible(false);
        list.append(&row);
        rows.push(RowView { row, cells });
    }
    let vertical = gtk::ScrolledWindow::new();
    vertical.set_vexpand(true);
    vertical.set_hexpand(true);
    vertical.set_hscrollbar_policy(gtk::PolicyType::Never);
    vertical.set_child(Some(&list));
    vertical.set_min_content_height(0);
    let inner = vbox(0, &[]);
    inner.append(&heads);
    inner.append(&vertical);
    inner.set_vexpand(true);
    let horizontal = gtk::ScrolledWindow::new();
    horizontal.set_vexpand(true);
    horizontal.set_vscrollbar_policy(gtk::PolicyType::Never);
    horizontal.set_child(Some(&inner));
    let overlay = gtk::Overlay::new();
    overlay.set_vexpand(true);
    overlay.set_child(Some(&horizontal));
    let empty = label("Leser oppgaver …", &["muted", "task-empty"]);
    empty.set_halign(gtk::Align::Center);
    empty.set_valign(gtk::Align::Center);
    empty.set_wrap(true);
    empty.set_can_target(false);
    overlay.add_overlay(&empty);
    table.append(&overlay);
    root.append(&table);
    let pager = hbox(8, &["task-pager"]);
    let count = label("Leser oppgaver …", &["muted", "num"]);
    count.set_hexpand(true);
    let previous = gtk::Button::with_label("Forrige");
    let following = gtk::Button::with_label("Neste");
    previous.set_sensitive(false);
    following.set_sensitive(false);
    pager.append(&count);
    pager.append(&previous);
    pager.append(&following);
    root.append(&pager);
    let footer = hbox(8, &["task-footer"]);
    let selection = label("Velg en oppgave for å avslutte den", &["muted"]);
    selection.set_hexpand(true);
    selection.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let stop = gtk::Button::with_label("Avslutt oppgave …");
    stop.add_css_class("task-stop");
    stop.set_sensitive(false);
    footer.append(&selection);
    footer.append(&stop);
    root.append(&footer);
    let note = label(
        "CPU: 100 % = én logisk CPU · RAM: RSS med delte sider · GPU-minne: – = ikke tilgjengelig",
        &["muted", "sm", "task-note"],
    );
    note.set_ellipsize(gtk::pango::EllipsizeMode::End);
    note.set_tooltip_text(Some("CPU kan overstige 100 %. RSS kan ikke summeres til systemets RAM-bruk. GPU-prosent per oppgave er ikke tilgjengelig; GPU-minne vises bare når driveren leverer det. Prosesstreet summerer ikke foreldre og barn."));
    root.append(&note);
    let status = label(
        "Kobler til oppgavetjenesten …",
        &["muted", "sm", "task-status"],
    );
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    root.append(&status);
    let st = Rc::new(State {
        app: app.clone(),
        visible: Cell::new(false),
        suspended: Cell::new(false),
        paused: Cell::new(false),
        timer: RefCell::new(None),
        refresh_retry: RefCell::new(None),
        refresh_until: Cell::new(None),
        pending: Cell::new(None),
        busy: Cell::new(false),
        generation: Cell::new(0),
        snapshot: Cell::new(None),
        snapshot_lost: Cell::new(false),
        search,
        tree,
        pause,
        list,
        rows,
        data: RefCell::new(Vec::new()),
        selected: RefCell::new(None),
        updating: Cell::new(false),
        sort: RefCell::new("cpu".into()),
        descending: Cell::new(true),
        headers,
        offset: Cell::new(0),
        next: Cell::new(None),
        previous_offsets: RefCell::new(Vec::new()),
        previous,
        following,
        count,
        status,
        selection,
        stop,
        empty,
        stale: Cell::new(true),
        summaries,
    });
    for (i, (key, title, _)) in COLS.iter().enumerate() {
        let w = Rc::downgrade(&st);
        let key = *key;
        let title = *title;
        let b = &st.headers[i];
        b.set_tooltip_text(Some(&format!("Sorter etter {title}")));
        b.set_widget_name(&format!("task-sort-{key}"));
        if key == "cpu" {
            b.add_css_class("sorted");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                set_text(&l, "CPU ↓");
            }
        }
        b.connect_clicked(move |_| {
            let Some(s) = w.upgrade() else { return };
            let same = s.sort.borrow().as_str() == key;
            s.descending.set(if same {
                !s.descending.get()
            } else {
                matches!(key, "cpu" | "memory" | "gpuMemory")
            });
            *s.sort.borrow_mut() = key.into();
            for (j, (_, t, _)) in COLS.iter().enumerate() {
                let active = j == i;
                class(&s.headers[j], "sorted", active);
                if let Some(l) = s.headers[j].child().and_downcast::<gtk::Label>() {
                    set_text(
                        &l,
                        &if active {
                            format!("{t} {}", if s.descending.get() { "↓" } else { "↑" })
                        } else {
                            t.to_string()
                        },
                    );
                }
            }
            s.changed();
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.search.connect_search_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.changed();
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.tree.connect_toggled(move |_| {
            if let Some(s) = w.upgrade() {
                s.changed();
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.pause.connect_toggled(move |b| {
            if let Some(s) = w.upgrade() {
                s.paused.set(b.is_active());
                b.set_label(if b.is_active() { "Fortsett" } else { "Pause" });
                if b.is_active() {
                    s.cancel_timer();
                    set_text(&s.status, "Oppdatering pauset · F5 oppdaterer én gang");
                } else {
                    s.start();
                }
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.list.connect_row_selected(move |_, row| {
            let Some(s) = w.upgrade() else { return };
            if s.updating.get() {
                return;
            }
            *s.selected.borrow_mut() =
                row.and_then(|r| s.data.borrow().get(r.index() as usize).map(|r| r.identity));
            s.update_selection();
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.stop.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.confirm();
            }
        });
    }
    for forward in [false, true] {
        let w = Rc::downgrade(&st);
        let b = if forward { &st.following } else { &st.previous };
        b.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                if s.refresh_until.get().is_some() {
                    return;
                }
                let offset = s.offset.get();
                let next = if forward {
                    let Some(next) = s.next.get() else { return };
                    next
                } else {
                    s.previous_offsets.borrow().last().copied()
                        .unwrap_or_else(|| offset.saturating_sub(LIMIT))
                };
                s.request_page(Load::Displayed(next));
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        app.on_telemetry.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.refresh_summary();
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        app.on_conn.connect(move |conn| {
            if let Some(s) = w.upgrade() {
                s.stale.set(true);
                s.update_selection();
                if *conn == Conn::Connected {
                    s.start();
                } else {
                    s.cancel_timer();
                    if s.snapshot.take().is_some() {
                        s.snapshot_lost.set(true);
                    }
                }
            }
        });
    }
    Box::new(Oppgaver { root, st })
}
