//! Oppgavebehandling. Bare den synlige siden ber om prosesser; høyst 100
//! gjenbrukte rader og én forespørsel om gangen. Systemgrafene deler App-data.
//!
//! Minnetallene (se `rigg_core::process`): kolonnen «Minne» er privat minne
//! i RAM (`RssAnon`), som kan summeres og leses for alle prosesser. RAM-kortet
//! deler brukt minne i prosesser (`AnonPages`), delt minne/tmpfs (`Shmem`),
//! kjernen og resten, og viser buffer/cache og swap for seg.
use rigg_core::{tr, trf, trn};
use super::{Key, Page};
use crate::state::{App, Conn};
use crate::widgets::icon::Icon;
use crate::widgets::seg::Seg;
use crate::widgets::spark::SparkOpts;
use crate::widgets::{Spark, class, hbox, label, set_text, vbox};
use gtk::{glib, prelude::*};
use rigg_core::fmt as F;
use rigg_core::process::{ProcessDetail, ProcessIdentity, ProcessRow, ProcessTotals, Restricted};
use rigg_core::proto::ErrorCode;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

const LIMIT: usize = 100;
/// Høyst så mange utvidede grupper (samme grense som tjenesten).
const MAX_EXPANDED: usize = rigg_core::process::MAX_EXPANDED;
fn columns() -> [(&'static str, &'static str, i32); 8] {
    [
    ("name", tr!("Navn"), 190),
    ("pid", "PID", 70),
    ("user", tr!("Bruker"), 80),
    ("cpu", "CPU", 70),
    ("memory", tr!("Minne"), 90),
    ("swap", "Swap", 80),
    ("gpuMemory", tr!("GPU-minne"), 90),
    ("state", tr!("Status"), 70),
    ]
}

/// Den valgte raden: en prosess, eller en gruppe (etter gruppenøkkelen).
#[derive(Clone, PartialEq, Eq)]
enum Selected {
    Process(ProcessIdentity),
    Group(String),
}

impl Selected {
    fn of(row: &ProcessRow) -> Selected {
        match (&row.group_key, row.group_count) {
            (Some(key), n) if n > 0 => Selected::Group(key.clone()),
            _ => Selected::Process(row.identity),
        }
    }
}

struct RowView {
    row: gtk::ListBoxRow,
    expander: gtk::Button,
    arrow: Icon,
    cells: Vec<gtk::Label>,
}
struct Summary {
    value: gtk::Label,
    spark: Spark,
    key: &'static str,
    lines: Vec<gtk::Label>,
}

/// Fordelingen av RAM (andeler av totalt): prosesser, delt/tmpfs, kjerne,
/// annet brukt og buffer/cache. Resten er ledig.
#[derive(Clone, Copy, Default)]
struct MemSplit([f64; 5]);

/// Én linje i detaljfeltet: navnet og verdien.
struct DetailItem {
    key: &'static str,
    item: gtk::Box,
    value: gtk::Label,
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
    view: Seg,
    expanded: RefCell<Vec<String>>,
    pause: gtk::ToggleButton,
    list: gtk::ListBox,
    rows: Vec<RowView>,
    data: RefCell<Vec<ProcessRow>>,
    selected: RefCell<Option<Selected>>,
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
    reason: gtk::Label,
    stop: gtk::Button,
    empty: gtk::Label,
    stale: Cell<bool>,
    summaries: Vec<Summary>,
    totals: RefCell<ProcessTotals>,
    mem_bar: gtk::DrawingArea,
    mem_split: Rc<Cell<MemSplit>>,
    detail_scroll: gtk::ScrolledWindow,
    detail_items: Vec<DetailItem>,
    detail: RefCell<Option<ProcessDetail>>,
    detail_gpu: Cell<Option<u64>>,
    detail_busy: Cell<bool>,
    detail_generation: Cell<u64>,
    /// Forrige I/O-avlesning for den valgte prosessen (tid, lest, skrevet).
    io_previous: Cell<Option<(ProcessIdentity, i64, u64, u64)>>,
    io_rate: Cell<Option<(f64, f64)>>,
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
        let view = self.view_mode();
        let args = json!({"filter": self.search.text().to_string(), "sort": &*self.sort.borrow(),
            "descending": self.descending.get(), "tree": view == "tree", "group": view == "group",
            "expanded": &*self.expanded.borrow(),
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
                        set_text(&s.status, tr!("Henter ny prøve …"));
                    }
                    if load != Load::Live {
                        if s.refresh_until.get().is_some_and(|until| Instant::now() < until) {
                            s.retry_refresh(load);
                        } else {
                            s.cancel_refresh();
                            set_text(&s.status, tr!("Tjenesten bruker lengre tid. Prøv igjen med F5."));
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
                            tr!("Prosessbildet er ikke lenger tilgjengelig"),
                        ).body(tr!("Viser første side i et nytt prosessbilde.")));
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
                            tr!("Oppgavebehandling støttes ikke av denne tjenesteversjonen."),
                        );
                        set_text(
                            &s.empty,
                            tr!("Oppdater tjenesten for å vise oppgaver. Prøv igjen med F5."),
                        );
                        s.empty.set_visible(s.data.borrow().is_empty());
                        s.update_selection();
                        return;
                    }
                    set_text(
                        &s.status,
                        &trf!("Kunne ikke hente oppgaver · {verdi}", verdi = e.message),
                    );
                    if s.data.borrow().is_empty() {
                        set_text(
                            &s.empty,
                            tr!("Oppgaver er ikke tilgjengelige. Prøv igjen med F5."),
                        );
                        s.empty.set_visible(true);
                    }
                    s.update_selection();
                }
            }
        });
    }
    fn apply(self: &Rc<Self>, v: &Value) {
        let parsed = serde_json::from_value::<Vec<ProcessRow>>(v["rows"].clone());
        let Ok(mut data) = parsed else {
            self.stale.set(true);
            set_text(
                &self.status,
                tr!("Tjenesten returnerte ugyldige oppgavedata. Prøv igjen med F5."),
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
        let selected = self.selected.borrow().clone();
        let view = self.view_mode();
        self.updating.set(true);
        self.list.unselect_all();
        let mut restored = None;
        for (i, view_row) in self.rows.iter().enumerate() {
            let Some(row) = data.get(i) else {
                view_row.row.set_visible(false);
                continue;
            };
            view_row.row.set_visible(true);
            let group = row.group_count > 0;
            // Gruppevisning: pilen på grupperader, tom plass ellers, så navnene står på linje.
            view_row.expander.set_visible(view == "group");
            view_row.expander.set_opacity(if group { 1.0 } else { 0.0 });
            view_row.expander.set_can_target(group);
            view_row.expander.set_focusable(group);
            if group {
                view_row.arrow.set_icon(if row.expanded { "chevron-down" } else { "chevron-right" });
                view_row.expander.set_tooltip_text(Some(if row.expanded { tr!("Fold sammen gruppen") } else { tr!("Fold ut gruppen") }));
                view_row.expander.update_property(&[gtk::accessible::Property::Label(&trf!("{navn}: {antall} prosesser", navn = row.name, antall = row.group_count))]);
            }
            view_row.expander.set_margin_start(if view == "group" { i32::from(row.depth.min(4)) * 16 } else { 0 });
            let name = if view == "tree" && row.depth > 0 {
                format!(
                    "{}↳ {}",
                    "  ".repeat(usize::from(row.depth.min(8))),
                    row.name
                )
            } else if group {
                format!("{} ({})", row.name, row.group_count)
            } else {
                row.name.clone()
            };
            view_row.row.set_widget_name(&match (&row.group_key, group) {
                (Some(key), true) => format!("task-group-{key}"),
                _ => format!("task-{}-{}", row.identity.pid, row.identity.start_ticks),
            });
            class(&view_row.row, "task-group", group);
            class(&view_row.row, "task-kernel", row.kernel);
            let user = row
                .user
                .clone()
                .or_else(|| row.uid.map(|uid| uid.to_string()))
                .unwrap_or_else(|| "–".into());
            let values = [
                name,
                if group { String::new() } else { row.identity.pid.to_string() },
                user,
                F::pct(row.cpu_percent, F::dec(1)),
                bytes(row.private_bytes),
                bytes(row.swap_bytes),
                bytes(row.gpu_vram_bytes),
                state_name(&row.state).into(),
            ];
            for (cell, text) in view_row.cells.iter().zip(values) {
                set_text(cell, &text);
            }
            view_row.cells[0].set_tooltip_text(Some(&if group {
                trf!("{navn}: {antall} prosesser. Tallene er summen for gruppen.", navn = row.name, antall = row.group_count)
            } else if row.kernel {
                trf!("{navn}\nKjernetråd · PID {pid}", navn = row.name, pid = row.identity.pid)
            } else {
                trf!("{navn}\nPID {pid} · forelder {forelder}", navn = row.name, pid = row.identity.pid, forelder = row.ppid)
            }));
            view_row.cells[2].set_tooltip_text(Some(&view_row.cells[2].text()));
            view_row.cells[3].set_tooltip_text(Some(tr!("100 % tilsvarer én logisk CPU. Flertrådede oppgaver kan overstige 100 %. Første prøve vises som –.")));
            view_row.cells[4].set_tooltip_text(Some(&if row.kernel {
                tr!("Kjernetråder har ikke eget minne i brukerrommet.").to_string()
            } else {
                trf!("Privat minne i RAM (uten delte sider). RSS med delte sider: {rss}.", rss = bytes(row.rss_bytes))
            }));
            view_row.cells[5].set_tooltip_text(Some(tr!("Minne prosessen har fått flyttet til swap på disk.")));
            view_row.cells[6].set_tooltip_text(Some(tr!("Videominne rapportert per prosess av NVIDIA-driveren. – betyr at målingen ikke er tilgjengelig, ikke null.")));
            if selected.as_ref() == Some(&Selected::of(row)) {
                restored = selected.clone();
                self.list.select_row(Some(&view_row.row));
            }
        }
        *self.data.borrow_mut() = data;
        // En prosess som har forsvunnet fra siden, beholdes ikke som valgt.
        let lost = restored.is_none() && selected.is_some();
        *self.selected.borrow_mut() = restored;
        self.updating.set(false);
        if let Ok(totals) = serde_json::from_value::<ProcessTotals>(v["totals"].clone()) {
            *self.totals.borrow_mut() = totals;
        }
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
        // Gruppevisningen teller rader (grupper og utfoldede prosesser), ikke prosesser.
        let grouped = view == "group";
        let count = if grouped {
            trn!(total, "{range} av {matched} rader · {total} oppgaver", "{range} av {matched} rader · {total} oppgaver", range = range, matched = matched, total = total)
        } else {
            trn!(total, "{range} av {matched} · {total} oppgaver", "{range} av {matched} · {total} oppgaver", range = range, matched = matched, total = total)
        };
        set_text(&self.count, &count);
        self.empty.set_visible(n == 0);
        set_text(
            &self.empty,
            if v["loading"].as_bool() == Some(true) {
                tr!("Leser oppgaver … CPU blir tilgjengelig etter neste prøve.")
            } else {
                tr!("Ingen oppgaver passer søket.")
            },
        );
        let mut message = if v["loading"].as_bool() == Some(true) {
            tr!("Henter ny prøve …").into()
        } else if self.paused.get() {
            tr!("Oppdatering pauset · F5 oppdaterer én gang").into()
        } else if self.stale.get() {
            trf!("Eldre prøve · {verdi} s", verdi = v["ageMs"].as_u64().unwrap_or(0) / 1000)
        } else {
            trf!("Oppdateres hvert 2. sekund · prøve {verdi} s gammel", verdi = v["ageMs"].as_u64().unwrap_or(0) / 1000)
        };
        if let Some(error) = v["error"].as_str().filter(|s| !s.is_empty()) {
            message = format!("{message} · {error}");
        }
        let unreadable = v["unreadable"].as_u64().unwrap_or(0);
        if unreadable > 0 {
            message.push_str(&trf!(" · {unreadable} utilgjengelige", unreadable = unreadable));
        }
        if let Some(note) = v["gpuNote"].as_str().filter(|n| !n.is_empty()) {
            self.headers[5].set_tooltip_text(Some(&trf!("Sorter etter GPU-minne. {note}", note = note)));
        }
        self.status.set_tooltip_text(Some(&message));
        set_text(&self.status, &message);
        if lost {
            self.clear_detail();
        }
        self.update_selection();
        self.refresh_summary();
        self.load_detail();
    }
    fn view_mode(&self) -> String {
        self.view.value().unwrap_or_else(|| "group".into())
    }
    fn selected_row(&self) -> Option<ProcessRow> {
        let selected = self.selected.borrow();
        let selected = selected.as_ref()?;
        self.data.borrow().iter().find(|r| Selected::of(r) == *selected).cloned()
    }
    /// Fold en gruppe ut eller sammen og hent siden på nytt.
    fn toggle_group(self: &Rc<Self>, key: &str) {
        {
            let mut expanded = self.expanded.borrow_mut();
            if let Some(i) = expanded.iter().position(|k| k == key) {
                expanded.remove(i);
            } else {
                if expanded.len() >= MAX_EXPANDED {
                    expanded.remove(0);
                }
                expanded.push(key.to_string());
            }
        }
        self.request_page(Load::Displayed(self.offset.get()));
    }
    fn update_selection(&self) {
        let row = self.selected_row();
        let allowed = row.as_ref().is_some_and(|r| r.can_terminate)
            && !self.stale.get()
            && self.app.client.is_connected();
        self.stop.set_sensitive(allowed);
        let text = match &row {
            Some(r) if r.group_count > 0 => trf!("{navn} · {antall} prosesser", navn = r.name, antall = r.group_count),
            Some(r) if r.kernel => trf!("{navn} · PID {pid} · kjernetråd", navn = r.name, pid = r.identity.pid),
            Some(r) => format!("{} · PID {}", r.name, r.identity.pid),
            None => tr!("Ingen oppgave valgt").into(),
        };
        self.selection.set_tooltip_text(Some(&text));
        set_text(&self.selection, &text);
        class(&self.selection, "muted", row.is_none());
        // Hvorfor oppgaven ikke kan avsluttes, står synlig (ikke bare i verktøytipset).
        let reason = match &row {
            None => tr!("Velg en rad for å se kommando, minne, disk-I/O og starttid. Grupper foldes ut med pilen eller Enter.").to_string(),
            Some(r) if r.group_count > 0 => r.protected_reason.clone().unwrap_or_default(),
            Some(_) if self.stale.get() => tr!("Oppdater oppgavelisten før du avslutter en oppgave.").into(),
            Some(r) => match r.protected_reason.as_deref() {
                Some(reason) => trf!("Kan ikke avsluttes: {grunn}", grunn = reason),
                None => tr!("Avslutt sender en vanlig forespørsel om å avslutte (SIGTERM).").into(),
            },
        };
        set_text(&self.reason, &reason);
        self.reason.set_tooltip_text(Some(&reason));
        self.stop.set_tooltip_text(Some(&reason));
        self.render_detail(row.as_ref());
    }
    fn clear_detail(&self) {
        self.detail.borrow_mut().take();
        self.detail_gpu.set(None);
        self.io_previous.set(None);
        self.io_rate.set(None);
        self.detail_generation.set(self.detail_generation.get().wrapping_add(1));
    }
    /// Henter detaljene for den valgte prosessen (ikke for grupper). Kalles ved
    /// valg og etter hver ny side, så tallene følger listen hvert 2. sekund.
    fn load_detail(self: &Rc<Self>) {
        let Some(Selected::Process(identity)) = self.selected.borrow().clone() else {
            return;
        };
        if self.detail_busy.replace(true) || !self.visible.get() || self.app.conn.get() != Conn::Connected {
            if !self.visible.get() || self.app.conn.get() != Conn::Connected {
                self.detail_busy.set(false);
            }
            return;
        }
        let generation = self.detail_generation.get();
        let w = Rc::downgrade(self);
        self.app.client.request("get_task_detail", json!({"pid": identity.pid, "startTicks": identity.start_ticks}), move |result| {
            let Some(s) = w.upgrade() else { return };
            s.detail_busy.set(false);
            if generation != s.detail_generation.get() || s.selected.borrow().as_ref() != Some(&Selected::Process(identity)) {
                return;
            }
            match result.ok().and_then(|v| {
                let gpu = v["gpuVramBytes"].as_u64();
                serde_json::from_value::<ProcessDetail>(v).ok().map(|d| (d, gpu))
            }) {
                Some((detail, gpu)) if detail.identity == identity => {
                    if let (Some(read), Some(written)) = (detail.io_read_bytes, detail.io_write_bytes) {
                        if let Some((id, t, r, w)) = s.io_previous.get().filter(|p| p.0 == identity) {
                            let dt = (detail.sampled_at_ms - t) as f64 / 1000.0;
                            if id == identity && dt > 0.2 && read >= r && written >= w {
                                s.io_rate.set(Some(((read - r) as f64 / dt, (written - w) as f64 / dt)));
                            }
                        }
                        s.io_previous.set(Some((identity, detail.sampled_at_ms, read, written)));
                    }
                    s.detail_gpu.set(gpu);
                    *s.detail.borrow_mut() = Some(detail);
                }
                // Prosessen er borte eller kan ikke leses nå: vis det listen vet.
                _ => {
                    s.detail.borrow_mut().take();
                }
            }
            s.render_detail(s.selected_row().as_ref());
        });
    }
    fn set_item(&self, key: &str, value: &str, tip: Option<&str>) {
        if let Some(item) = self.detail_items.iter().find(|i| i.key == key) {
            show_item(&item.item, true);
            set_text(&item.value, value);
            item.value.set_tooltip_text(Some(tip.unwrap_or(value)));
            class(&item.value, "muted", value == "–" || value == tr!("ingen tilgang"));
        }
    }
    /// Fyller detaljfeltet for raden (prosess eller gruppe). Felt kjernen ikke
    /// lar appen lese, står som – med forklaring.
    fn render_detail(&self, row: Option<&ProcessRow>) {
        for item in &self.detail_items {
            show_item(&item.item, false);
        }
        let Some(row) = row else {
            self.detail_scroll.set_visible(false);
            return;
        };
        self.detail_scroll.set_visible(true);
        let na = tr!("Ikke tilgjengelig for denne prosessen.");
        let restricted_tip = tr!("Krever at prosessen tilhører deg, eller root. Kjernen gir ikke andre brukere tilgang.");
        if row.group_count > 0 {
            self.set_item("count", &F::num(Some(f64::from(row.group_count)), 0), None);
            self.set_item("user", row.user.as_deref().unwrap_or("–"), None);
            self.set_item("cpu", &F::pct(row.cpu_percent, F::dec(1)), None);
            self.set_item("threads", &F::num(row.threads.map(f64::from), 0), None);
            self.set_item("private", &bytes(row.private_bytes), Some(tr!("Summen av gruppens private minne i RAM.")));
            self.set_item("swap", &bytes(row.swap_bytes), None);
            self.set_item("gpu", &bytes(row.gpu_vram_bytes), None);
            return;
        }
        let detail = self.detail.borrow();
        let d = detail.as_ref().filter(|d| d.identity == row.identity);
        let restricted = |r: Restricted| d.is_some_and(|d| d.restricted.contains(&r));
        // Felt kjernen nekter, merkes synlig (verktøytipset forklarer hvorfor).
        let no_access = tr!("ingen tilgang");
        let or_denied = |text: String, r: Restricted| if text == "–" && restricted(r) { no_access.to_string() } else { text };
        let cmd = d.and_then(|d| d.cmdline.clone().or_else(|| d.exe.clone()));
        let cmd_text = match (&cmd, row.kernel) {
            (Some(cmd), _) => cmd.clone(),
            (None, true) => tr!("Kjernetråd uten kommandolinje").into(),
            (None, false) => "–".into(),
        };
        self.set_item("command", &cmd_text, d.and_then(|d| d.exe.as_deref()).map(|exe| format!("{cmd_text}\n{exe}")).as_deref());
        if d.is_some_and(|d| d.restricted.contains(&Restricted::Exe) && d.cmdline.is_none()) {
            self.set_item("command", no_access, Some(restricted_tip));
        }
        self.set_item("user", &row.user.clone().or_else(|| row.uid.map(|u| u.to_string())).unwrap_or_else(|| "–".into()), None);
        let started = d.and_then(|d| d.started_at_ms).map(|ms| {
            F::since(crate::app::local_time(ms), crate::app::local_time(rigg_core::now_ms()))
        });
        self.set_item("started", started.as_deref().unwrap_or("–"), None);
        let cpu_time = d.and_then(|d| d.cpu_time_s).map(duration);
        self.set_item("cpuTime", cpu_time.as_deref().unwrap_or("–"), Some(tr!("Samlet CPU-tid siden start (bruker + kjerne).")));
        self.set_item("threads", &F::num(d.and_then(|d| d.threads).or(row.threads).map(f64::from), 0), None);
        self.set_item("nice", &d.and_then(|d| d.nice).map_or_else(|| "–".to_string(), |n| n.to_string()), Some(tr!("Nice-verdi: −20 er høyest prioritet, 19 lavest.")));
        self.set_item("private", &bytes(d.and_then(|d| d.private_bytes).or(row.private_bytes)), Some(tr!("Privat minne i RAM (RssAnon): heap, stakk og andre anonyme sider. Dette er kolonnen Minne.")));
        let pss_tip = if restricted(Restricted::Smaps) { restricted_tip } else { tr!("PSS: privat minne pluss en rettferdig andel av delte sider. Summen over alle prosesser gir brukt minne uten dobbeltelling.") };
        self.set_item("pss", &or_denied(bytes(d.and_then(|d| d.pss_bytes)), Restricted::Smaps), Some(pss_tip));
        let uss_tip = if restricted(Restricted::Smaps) { restricted_tip } else { tr!("USS: sider bare denne prosessen bruker. Frigjøres når prosessen avsluttes.") };
        self.set_item("uss", &or_denied(bytes(d.and_then(|d| d.uss_bytes)), Restricted::Smaps), Some(uss_tip));
        self.set_item("shared", &bytes(d.and_then(|d| d.shared_bytes).or(row.shared_bytes)), Some(tr!("Delte sider i RAM: biblioteker, filer og delt minne. Telles hos alle prosessene som bruker dem.")));
        self.set_item("rss", &bytes(d.and_then(|d| d.rss_bytes).or(row.rss_bytes)), Some(tr!("RSS: privat minne pluss delte sider. Kan ikke summeres over prosesser.")));
        self.set_item("swap", &bytes(d.and_then(|d| d.swap_bytes).or(row.swap_bytes)), None);
        self.set_item("gpu", &bytes(self.detail_gpu.get().or(row.gpu_vram_bytes)), Some(tr!("Videominne rapportert per prosess av NVIDIA-driveren. – betyr at målingen ikke er tilgjengelig, ikke null.")));
        let rate = self.io_rate.get();
        let io = |total: Option<u64>, rate: Option<f64>| match (total, rate) {
            (Some(total), Some(rate)) => format!("{} · {}/s", bytes(Some(total)), F::bytes(Some(rate), 1)),
            (Some(total), None) => bytes(Some(total)),
            _ => "–".into(),
        };
        let io_tip = if restricted(Restricted::Io) { restricted_tip } else { tr!("Lest fra og skrevet til lagringsenheter siden start, og raten siden forrige prøve.") };
        self.set_item("read", &or_denied(io(d.and_then(|d| d.io_read_bytes), rate.map(|r| r.0)), Restricted::Io), Some(io_tip));
        self.set_item("write", &or_denied(io(d.and_then(|d| d.io_write_bytes), rate.map(|r| r.1)), Restricted::Io), Some(io_tip));
        let parent = d.map(|d| match &d.parent_name {
            Some(name) => format!("{name} · {}", d.ppid),
            None => d.ppid.to_string(),
        });
        self.set_item("parent", parent.as_deref().unwrap_or("–"), None);
        self.set_item("unit", d.and_then(|d| d.unit.as_deref()).unwrap_or("–"), Some(tr!("Systemd-enheten prosessen kjører i (cgroup).")));
        if d.is_none() && !row.kernel {
            for item in self.detail_items.iter().filter(|i| matches!(i.key, "command" | "started" | "pss" | "uss" | "read" | "write")) {
                item.value.set_tooltip_text(Some(na));
            }
        }
    }
    fn confirm(self: &Rc<Self>) {
        let Some(row) = self
            .selected_row()
            .filter(|r| r.can_terminate && !self.stale.get())
        else {
            return;
        };
        let Some((window, root)) =
            crate::shortcuts::dialog(&self.app, tr!("Avslutt oppgave?"), 440, 250)
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
            tr!("Oppgaven får en vanlig forespørsel om å avslutte (SIGTERM). Ulagret arbeid kan gå tapt."),
            &[],
        );
        detail.set_wrap(true);
        body.append(&detail);
        root.append(&body);
        let buttons = hbox(8, &["dialog-foot"]);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label(tr!("Avbryt"));
        let yes = gtk::Button::with_label(tr!("Avslutt oppgave"));
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
                    Ok(_) => a.toast(crate::toast::Toast::new(tr!("Avslutning sendt")).body(tr!("Oppgaven har mottatt SIGTERM. Den kan bruke litt tid på å avslutte."))),
                    Err(e) => a.toast(crate::toast::Toast::new(tr!("Oppgaven ble ikke avsluttet")).body(&e.message).error()),
                }
                if let Some(s) = weak.upgrade() {
                    s.refresh_now();
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
            let lines = match summary.key {
                "cpu/load" => self.cpu_lines(),
                "mem/usedPct" => self.mem_lines(),
                _ => self.gpu_lines(),
            };
            for (label, (text, tip)) in summary.lines.iter().zip(lines) {
                set_text(label, &text);
                label.set_tooltip_text(Some(tip.as_deref().unwrap_or(&text)));
            }
        }
    }
    fn cpu_lines(&self) -> Vec<(String, Option<String>)> {
        let sys = self.app.sys();
        let cpu = sys.as_ref().map(|s| &s.cpu);
        let mhz: Vec<f64> = cpu.map(|c| c.core_mhz.iter().flatten().copied().collect()).unwrap_or_default();
        let avg = (!mhz.is_empty()).then(|| mhz.iter().sum::<f64>() / mhz.len() as f64);
        let max = mhz.iter().copied().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        let load = sys.as_ref().map(|s| s.loadavg.iter().take(3).map(|v| F::num(Some(*v), 2)).collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty());
        let t = self.totals.borrow();
        vec![
            (
                trf!("{klokke} · {temperatur}", klokke = F::mhz(avg), temperatur = F::temp(cpu.and_then(|c| c.temp), 0)),
                Some(trf!("Snittklokke over alle tråder: {snitt}. Høyeste: {maks}. Temperatur (Tdie): {temperatur}.", snitt = F::mhz(avg), maks = F::mhz(max), temperatur = F::temp(cpu.and_then(|c| c.temp), 1))),
            ),
            (
                trf!("Last {verdier}", verdier = load.as_deref().unwrap_or("–")),
                Some(tr!("Gjennomsnittlig belastning (load average) siste 1, 5 og 15 minutter: prosesser som kjører eller venter. 16 tilsvarer full last på 16 tråder.").to_string()),
            ),
            (
                if t.processes == 0 {
                    "–".into()
                } else {
                    trf!("{prosesser} prosesser · {traader} tråder", prosesser = F::num(Some(f64::from(t.processes)), 0), traader = F::num(Some(t.threads as f64), 0))
                },
                Some(trf!("{prosesser} prosesser og {kjerne} kjernetråder. Trådene er summen for prosessene som kan leses, pluss kjernetrådene.", prosesser = t.processes, kjerne = t.kernel_threads)),
            ),
        ]
    }
    fn mem_lines(&self) -> Vec<(String, Option<String>)> {
        let Some(m) = self.app.sys().map(|s| s.mem) else {
            self.mem_split.set(MemSplit::default());
            self.mem_bar.queue_draw();
            return vec![("–".into(), None), ("–".into(), None), ("–".into(), None)];
        };
        let kb = |v: Option<f64>| F::kb(v, 1);
        let cache = m.cache_kb();
        let other = m.used_kb.map(|u| (u - m.anon_kb.unwrap_or(0.0) - m.shmem_kb.unwrap_or(0.0) - m.kernel_kb.unwrap_or(0.0)).max(0.0));
        if let Some(total) = m.total_kb.filter(|t| *t > 0.0) {
            let f = |v: Option<f64>| v.unwrap_or(0.0).max(0.0) / total;
            self.mem_split.set(MemSplit([f(m.anon_kb), f(m.shmem_kb), f(m.kernel_kb), f(other), f(cache)]));
        }
        self.mem_bar.queue_draw();
        let processes = self.totals.borrow().swap_bytes as f64 / 1024.0;
        let swap_tip = trf!(
            "Swap brukt: {brukt} av {totalt}. Prosessene har {prosesser} i swap (kolonnen Swap); resten er for det meste delt minne og tmpfs som er flyttet ut.",
            brukt = kb(m.swap_used_kb), totalt = kb(m.swap_total_kb), prosesser = F::kb(Some(processes), 1)
        );
        vec![
            (
                trf!("{brukt} av {totalt} · {ledig} tilgjengelig", brukt = kb(m.used_kb), totalt = kb(m.total_kb), ledig = kb(m.available_kb)),
                Some(tr!("Brukt = totalt − tilgjengelig (MemAvailable), som free og GNOME Ressurser.").to_string()),
            ),
            (
                trf!("Prosesser {prosesser} · Delt/tmpfs {delt} · Kjerne {kjerne}", prosesser = kb(m.anon_kb), delt = kb(m.shmem_kb), kjerne = kb(m.kernel_kb)),
                Some(trf!(
                    "Prosesser: anonymt minne (AnonPages), omtrent summen av kolonnen Minne. Delt/tmpfs (Shmem): filer i tmpfs som /tmp og /dev/shm, delt minne og grafikkbuffere; tilhører ingen enkelt prosess. Kjerne: slab som ikke kan frigjøres, kjernestakker og sidetabeller. Annet brukt: {annet} (blant annet reserver kjernen holder av).",
                    annet = kb(other)
                )),
            ),
            (
                trf!("Cache {cache} · Swap {brukt} av {totalt}", cache = kb(cache), brukt = kb(m.swap_used_kb), totalt = kb(m.swap_total_kb)),
                Some(format!("{}\n{swap_tip}", tr!("Buffer og cache (uten Shmem) kan frigjøres ved behov og regnes som tilgjengelig."))),
            ),
        ]
    }
    fn gpu_lines(&self) -> Vec<(String, Option<String>)> {
        let Some(sample) = self.app.gpu_sample() else {
            return vec![(tr!("Ingen skjermkort valgt").into(), None), (String::new(), None)];
        };
        let total = sample.vram_total.or_else(|| self.app.statics().and_then(|s| s.vram_total));
        vec![
            (
                {
                    // Samme enhet på begge sider: «1,5 GiB av 8,0 GiB».
                    let unit = total.filter(|t| *t >= 1024.0).map(|_| if F::format().mem_unit == "dec" { "GB" } else { "GiB" });
                    trf!("VRAM {brukt} av {totalt}", brukt = F::mib(sample.vram_used, Some(1), unit), totalt = F::mib(total, Some(1), unit))
                },
                Some(tr!("Videominne i bruk på kortet, fra driveren.").to_string()),
            ),
            (
                trf!("{temperatur} · {effekt}", temperatur = F::temp(sample.temp, 0), effekt = F::watt(sample.power, 0)),
                Some(tr!("Kortets temperatur og effektforbruk.").to_string()),
            ),
        ]
    }
}
/// Viser eller skjuler en detaljlinje. I rutenettet skjules hele cellen, så
/// det ikke blir hull etter felt som ikke gjelder raden.
fn show_item(item: &gtk::Box, on: bool) {
    item.set_visible(on);
    if let Some(child) = item.parent().and_downcast::<gtk::FlowBoxChild>() {
        child.set_visible(on);
    }
}

/// «2 t 05 min», «4 min 12 s» eller «12,3 s».
fn duration(seconds: f64) -> String {
    let s = seconds.max(0.0);
    if s < 60.0 {
        return trf!("{sekunder} s", sekunder = F::num(Some(s), 1));
    }
    let total = s.round() as u64;
    let (h, m, sec) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        trf!("{timer} t {minutter} min", timer = h, minutter = format!("{m:02}"))
    } else {
        trf!("{minutter} min {sekunder} s", minutter = m, sekunder = sec)
    }
}
/// Byte med én desimal fra GiB, hele MiB og hele KiB («984 KiB», ikke «984,0 KiB»).
fn bytes(v: Option<u64>) -> String {
    let n = v.map(|n| n as f64);
    F::bytes(n, if n.is_some_and(|n| n < 1024.0 * 1024.0) { 0 } else { F::dec(1) })
}
fn state_name(state: &str) -> &str {
    match state {
        "R" => tr!("Kjører"),
        "S" => tr!("Sover"),
        "D" => tr!("Venter"),
        "T" | "t" => tr!("Stoppet"),
        "Z" => "Zombie",
        "I" => tr!("Inaktiv"),
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

/// RAM-stolpen: prosesser, delt/tmpfs, kjerne, annet brukt og cache, i
/// fargene fra temaet. Resten av sporet er ledig minne.
fn draw_mem_bar(app: &Rc<App>, split: MemSplit, cr: &gtk::cairo::Context, w: f64, h: f64) {
    let rgba = |token: &str| crate::theme::rgba(app, token);
    let set = |c: gtk::gdk::RGBA, alpha: f64| cr.set_source_rgba(f64::from(c.red()), f64::from(c.green()), f64::from(c.blue()), f64::from(c.alpha()) * alpha);
    let r = h / 2.0;
    let rounded = |x: f64, width: f64| {
        cr.new_sub_path();
        cr.arc(x + width - r, r, r, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        cr.arc(x + r, r, r, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
        cr.close_path();
    };
    rounded(0.0, w);
    set(rgba("--track"), 1.0);
    let _ = cr.fill_preserve();
    cr.clip();
    let mut x = 0.0;
    for (i, (token, alpha)) in [("--c-mem", 1.0), ("--pal-purple", 1.0), ("--pal-gray", 1.0), ("--pal-gray", 0.55), ("--c-mem", 0.3)].into_iter().enumerate() {
        let width = (split.0[i].clamp(0.0, 1.0) * w).min(w - x);
        if width <= 0.0 {
            continue;
        }
        cr.rectangle(x, 0.0, width, h);
        set(rgba(token), alpha);
        let _ = cr.fill();
        x += width;
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "task-page"]);
    root.set_vexpand(true);
    let summary_row = hbox(8, &["task-summaries"]);
    summary_row.set_homogeneous(true);
    let mut summaries = Vec::new();
    let mem_split = Rc::new(Cell::new(MemSplit::default()));
    let mem_bar = gtk::DrawingArea::new();
    mem_bar.add_css_class("task-mem-bar");
    mem_bar.set_content_height(6);
    mem_bar.set_hexpand(true);
    {
        let a = app.clone();
        let split = mem_split.clone();
        mem_bar.set_draw_func(move |_, cr, w, h| draw_mem_bar(&a, split.get(), cr, f64::from(w), f64::from(h)));
    }
    mem_bar.set_tooltip_text(Some(tr!("Fra venstre: prosesser, delt minne/tmpfs, kjerne, annet brukt og buffer/cache. Resten av stolpen er ledig minne.")));
    for (title, key, serie, tooltip, lines) in [
        (
            tr!("CPU · hele systemet"),
            "cpu/load",
            "load",
            tr!("Samlet CPU-last, 0–100 %. Oppgavenes CPU-prosent bruker én logisk CPU som 100 %."),
            3,
        ),
        (
            tr!("RAM · hele systemet"),
            "mem/usedPct",
            "mem",
            tr!("Systemets brukte minne, fordelt på prosesser, delt minne/tmpfs og kjernen. Pek på en linje for forklaring."),
            3,
        ),
        (
            tr!("GPU · valgt kort"),
            "gpu",
            "memctl",
            tr!("Samlet GPU-last for kortet valgt i sidefeltet. Per-prosess GPU-prosent er ikke tilgjengelig."),
            2,
        ),
    ] {
        let panel = vbox(4, &["task-summary"]);
        let head = label(title, &["muted", "sm"]);
        head.set_xalign(0.0);
        head.set_tooltip_text(Some(tooltip));
        panel.append(&head);
        let line = hbox(12, &[]);
        let value = label("–", &["task-total", "num"]);
        value.set_tooltip_text(Some(tooltip));
        let spark = Spark::new(serie, 60);
        spark.set_hexpand(true);
        line.append(&value);
        line.append(&spark);
        panel.append(&line);
        if key == "mem/usedPct" {
            panel.append(&mem_bar);
        }
        let mut texts = Vec::new();
        for _ in 0..lines {
            let l = label("–", &["sm", "num", "task-summary-line"]);
            l.set_xalign(0.0);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            panel.append(&l);
            texts.push(l);
        }
        summary_row.append(&panel);
        summaries.push(Summary { value, spark, key, lines: texts });
    }
    root.append(&summary_row);
    let controls = hbox(8, &["task-controls"]);
    let search = gtk::SearchEntry::new();
    search.add_css_class("task-search");
    search.set_placeholder_text(Some(tr!("Søk etter navn, PID eller bruker")));
    search.set_hexpand(true);
    search.set_search_delay(250);
    search.update_property(&[gtk::accessible::Property::Label(tr!("Søk i oppgaver"))]);
    let view = Seg::new(
        &[("group", tr!("Gruppert")), ("list", tr!("Liste")), ("tree", tr!("Tre"))],
        "group",
        tr!("Visning av oppgavene"),
    );
    view.root.add_css_class("task-view");
    for (value, tip) in [
        ("group", tr!("Samle prosesser med samme navn (alle chrome-prosessene i én rad) og alle kjernetråder. Tallene er summen; fold ut for å se hver prosess.")),
        ("list", tr!("Alle prosesser og kjernetråder hver for seg.")),
        ("tree", tr!("Vis foreldre og barn. Sortering gjelder innen hver familie; ingen summering av barna.")),
    ] {
        if let Some(b) = view.button(value) {
            b.set_tooltip_text(Some(tip));
            b.add_css_class(&format!("task-view-{value}"));
        }
    }
    let pause = gtk::ToggleButton::with_label(tr!("Pause"));
    pause.add_css_class("task-pause");
    pause.set_tooltip_text(Some(
        tr!("Pause oppgavelisten. Systemgrafene fortsetter å vise felles målinger."),
    ));
    controls.append(&search);
    controls.append(&view.root);
    controls.append(&pause);
    root.append(&controls);
    let table = vbox(0, &["task-table"]);
    table.set_vexpand(true);
    let heads = hbox(0, &["task-header"]);
    let mut headers = Vec::new();
    let columns = columns();
    // Minste bredde i tegn per kolonne, så den følger skriftstørrelsen: den
    // oversatte overskriften med sorteringspil («GPU-Speicher ↓»), sjusifrede
    // PID-er, «1.012 KiB» og den lengste statusen («Schlafend»).
    let state_chars = ["R", "S", "D", "T", "Z", "I"].into_iter().map(|s| state_name(s).chars().count()).max().unwrap_or(6);
    let content_chars = [0, 8, 11, 7, 9, 9, 9, state_chars];
    let min_chars: Vec<i32> = columns
        .iter()
        .zip(content_chars)
        .enumerate()
        .map(|(i, ((_, title, _), content))| if i == 0 { -1 } else { (title.chars().count() + 2).max(content) as i32 })
        .collect();
    for (i, (_, title, width)) in columns.iter().enumerate() {
        let b = gtk::Button::new();
        b.add_css_class("task-sort");
        let l = cell(title, *width, i == 0);
        l.set_width_chars(min_chars[i]);
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
        let arrow = Icon::new("chevron-right", 14);
        let expander = gtk::Button::new();
        expander.set_child(Some(&arrow));
        expander.add_css_class("task-expander");
        expander.set_valign(gtk::Align::Center);
        expander.set_visible(false);
        line.append(&expander);
        let mut cells = Vec::new();
        for (i, (_, _, width)) in columns.iter().enumerate() {
            let l = cell("", *width, i == 0);
            l.set_width_chars(min_chars[i]);
            if i == 0 {
                l.add_css_class("task-name");
                // Pilen tar plass fra navnekolonnen, så de andre kolonnene står likt.
                l.set_width_request(*width - 24);
            }
            line.append(&l);
            cells.push(l);
        }
        row.set_child(Some(&line));
        row.set_visible(false);
        list.append(&row);
        rows.push(RowView { row, expander, arrow, cells });
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
    let empty = label(tr!("Leser oppgaver …"), &["muted", "task-empty"]);
    empty.set_halign(gtk::Align::Center);
    empty.set_valign(gtk::Align::Center);
    empty.set_wrap(true);
    empty.set_can_target(false);
    overlay.add_overlay(&empty);
    table.append(&overlay);
    root.append(&table);
    let pager = hbox(8, &["task-pager"]);
    let count = label(tr!("Leser oppgaver …"), &["muted", "num"]);
    count.set_hexpand(true);
    count.set_xalign(0.0);
    count.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let previous = gtk::Button::with_label(tr!("Forrige"));
    let following = gtk::Button::with_label(tr!("Neste"));
    previous.set_sensitive(false);
    following.set_sensitive(false);
    pager.append(&count);
    pager.append(&previous);
    pager.append(&following);
    root.append(&pager);

    // ---- Detaljfeltet for valgt oppgave ----
    let footer = vbox(6, &["task-footer", "task-detail"]);
    let head = hbox(8, &["task-detail-head"]);
    let titles = vbox(2, &[]);
    titles.set_hexpand(true);
    let selection = label(tr!("Ingen oppgave valgt"), &["bold", "task-detail-title"]);
    selection.set_xalign(0.0);
    selection.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let reason = label("", &["muted", "sm", "task-detail-reason"]);
    reason.set_xalign(0.0);
    // Høyst to linjer (lengre oversettelser), deretter …
    reason.set_wrap(true);
    reason.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    reason.set_lines(2);
    reason.set_ellipsize(gtk::pango::EllipsizeMode::End);
    titles.append(&selection);
    titles.append(&reason);
    let stop = gtk::Button::with_label(tr!("Avslutt oppgave …"));
    stop.add_css_class("task-stop");
    stop.set_valign(gtk::Align::Center);
    stop.set_sensitive(false);
    head.append(&titles);
    head.append(&stop);
    footer.append(&head);
    let detail_flow = gtk::FlowBox::new();
    detail_flow.add_css_class("task-detail-grid");
    detail_flow.set_selection_mode(gtk::SelectionMode::None);
    detail_flow.set_homogeneous(true);
    detail_flow.set_min_children_per_line(1);
    detail_flow.set_max_children_per_line(6);
    detail_flow.set_row_spacing(2);
    detail_flow.set_column_spacing(16);
    detail_flow.set_valign(gtk::Align::Start);
    let mut detail_items = Vec::new();
    // Fem per linje på brede vinduer: hvem og når, minnet, så swap, GPU og disk.
    for (key, title) in [
        ("command", tr!("Kommando")),
        ("count", tr!("Prosesser")),
        ("user", tr!("Bruker")),
        ("started", tr!("Startet")),
        ("cpu", "CPU"),
        ("cpuTime", tr!("CPU-tid")),
        ("threads", tr!("Tråder")),
        ("nice", tr!("Prioritet")),
        ("private", tr!("Privat minne")),
        ("pss", "PSS"),
        ("uss", "USS"),
        ("shared", tr!("Delt")),
        ("rss", "RSS"),
        ("swap", "Swap"),
        ("gpu", tr!("GPU-minne")),
        ("read", tr!("Disk lest")),
        ("write", tr!("Disk skrevet")),
        ("parent", tr!("Forelder")),
        ("unit", tr!("Enhet")),
    ] {
        let item = hbox(8, &["task-detail-item"]);
        let name = label(title, &["muted", "sm"]);
        name.set_xalign(0.0);
        name.set_width_request(86);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let value = label("–", &["sm", "num", "task-detail-value"]);
        value.set_xalign(0.0);
        value.set_hexpand(true);
        value.set_ellipsize(if key == "command" { gtk::pango::EllipsizeMode::Middle } else { gtk::pango::EllipsizeMode::End });
        value.set_selectable(key == "command");
        if key != "command" {
            // Naturlig bredde fra et fast antall tegn, så rutenettet får flere kolonner.
            value.set_max_width_chars(16);
            value.set_width_chars(8);
        }
        item.append(&name);
        item.append(&value);
        item.set_visible(false);
        if key == "command" {
            // Kommandolinjen får hele bredden over rutenettet.
            footer.append(&item);
        } else {
            item.set_width_request(200);
            detail_flow.insert(&item, -1);
        }
        detail_items.push(DetailItem { key, item, value });
    }
    let detail_scroll = gtk::ScrolledWindow::new();
    detail_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    detail_scroll.set_propagate_natural_height(true);
    detail_scroll.set_max_content_height(128);
    detail_scroll.set_child(Some(&detail_flow));
    detail_scroll.set_visible(false);
    footer.append(&detail_scroll);
    root.append(&footer);
    let bottom = hbox(12, &["task-bottom"]);
    let note = label(
        tr!("CPU: 100 % = én logisk CPU · Minne: privat minne i RAM, uten delte sider · – = ikke tilgjengelig"),
        &["muted", "sm", "task-note"],
    );
    note.set_ellipsize(gtk::pango::EllipsizeMode::End);
    note.set_hexpand(true);
    note.set_xalign(0.0);
    note.set_tooltip_text(Some(tr!("CPU kan overstige 100 %. Minne er privat minne i RAM (RssAnon) og kan summeres; RSS med delte sider står i detaljene. Gruppene summerer prosessene med samme navn. GPU-prosent per oppgave er ikke tilgjengelig; GPU-minne vises bare når driveren leverer det. Felt som krever at prosessen tilhører deg, vises som –.")));
    let status = label(
        tr!("Kobler til oppgavetjenesten …"),
        &["muted", "sm", "task-status"],
    );
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_xalign(1.0);
    bottom.append(&note);
    bottom.append(&status);
    root.append(&bottom);
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
        view,
        expanded: RefCell::new(Vec::new()),
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
        reason,
        stop,
        empty,
        stale: Cell::new(true),
        summaries,
        totals: RefCell::new(ProcessTotals::default()),
        mem_bar,
        mem_split,
        detail_scroll,
        detail_items,
        detail: RefCell::new(None),
        detail_gpu: Cell::new(None),
        detail_busy: Cell::new(false),
        detail_generation: Cell::new(0),
        io_previous: Cell::new(None),
        io_rate: Cell::new(None),
    });
    st.update_selection();
    for (i, (key, title, _)) in columns.iter().enumerate() {
        let w = Rc::downgrade(&st);
        let key = *key;
        let title = *title;
        let b = &st.headers[i];
        b.set_tooltip_text(Some(&trf!("Sorter etter {title}", title = title)));
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
                matches!(key, "cpu" | "memory" | "swap" | "gpuMemory")
            });
            *s.sort.borrow_mut() = key.into();
            for (j, (_, t, _)) in columns.iter().enumerate() {
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
    st.headers[4].set_tooltip_text(Some(tr!("Sorter etter privat minne i RAM (uten delte sider).")));
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
        st.view.connect(move |_| {
            if let Some(s) = w.upgrade() {
                s.expanded.borrow_mut().clear();
                s.changed();
            }
        });
    }
    for (i, view_row) in st.rows.iter().enumerate() {
        let w = Rc::downgrade(&st);
        view_row.expander.connect_clicked(move |_| {
            let Some(s) = w.upgrade() else { return };
            let key = s.data.borrow().get(i).filter(|r| r.group_count > 0).and_then(|r| r.group_key.clone());
            if let Some(key) = key {
                s.toggle_group(&key);
            }
        });
    }
    {
        // Enter eller dobbeltklikk på en grupperad folder den ut eller sammen.
        let w = Rc::downgrade(&st);
        st.list.connect_row_activated(move |_, row| {
            let Some(s) = w.upgrade() else { return };
            let key = s.data.borrow().get(row.index() as usize).filter(|r| r.group_count > 0).and_then(|r| r.group_key.clone());
            if let Some(key) = key {
                s.toggle_group(&key);
            }
        });
    }
    {
        let w = Rc::downgrade(&st);
        st.pause.connect_toggled(move |b| {
            if let Some(s) = w.upgrade() {
                s.paused.set(b.is_active());
                b.set_label(if b.is_active() { tr!("Fortsett") } else { tr!("Pause") });
                if b.is_active() {
                    s.cancel_timer();
                    set_text(&s.status, tr!("Oppdatering pauset · F5 oppdaterer én gang"));
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
            let next = row.and_then(|r| s.data.borrow().get(r.index() as usize).map(Selected::of));
            if *s.selected.borrow() != next {
                s.clear_detail();
            }
            *s.selected.borrow_mut() = next;
            s.update_selection();
            s.load_detail();
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
