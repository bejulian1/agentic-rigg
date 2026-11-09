//! Klienten mot tjenesten (arkitektur.md 6.7): Unix-socket med NDJSON, lest
//! ikke-blokkerende på GTK-hovedløkka (`unix_fd_add_local`), uten tråder.
//! Svar matches på `id`.
//!
//! Finnes ikke socketen, startes tjenesten: med `StartUnit` over D-Bus til
//! systemd `--user` i produksjon, og som egen prosess når enheten ikke finnes
//! (eller med `RIGG_PROFIL=dev`, som aldri rører systemd). Så prøves det igjen
//! hvert 25. ms i opptil 2 s, og etter det med økende pause (0,5–5 s).

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use rigg_core::paths::{Profile, UNIT};
use rigg_core::proto::{self, ErrorCode, Incoming, LineBuffer, LineEvent, OpResult, ProtoError};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Emnene vinduet abonnerer på.
pub const TOPICS: &[&str] = &["telemetry", "device", "stats", "log", "alarm", "settings", "txn"];

type Callback = Box<dyn FnOnce(OpResult)>;
type EventFn = Rc<dyn Fn(&str, &Value)>;
type ConnFn = Rc<dyn Fn(bool)>;

#[derive(Default)]
struct Inner {
    stream: Option<UnixStream>,
    in_watch: Option<glib::SourceId>,
    out_watch: Option<glib::SourceId>,
    lines: LineBuffer,
    out: Vec<u8>,
    next_id: u64,
    pending: HashMap<u64, (Instant, Callback)>,
    retry: Option<glib::SourceId>,
    backoff_ms: u64,
    fast_until: Option<Instant>,
    started_service: bool,
}

pub struct Client {
    path: PathBuf,
    profile: Profile,
    dbus: RefCell<Option<gio::DBusConnection>>,
    inner: RefCell<Inner>,
    on_event: RefCell<Option<EventFn>>,
    on_conn: RefCell<Option<ConnFn>>,
    connected: Cell<bool>,
    /// Hvordan tjenesten ble startet: «systemd», «prosess» eller «» (kjørte fra før).
    pub started_by: RefCell<String>,
}

impl Client {
    pub fn new(path: PathBuf, profile: Profile) -> Rc<Client> {
        Rc::new(Client {
            path,
            profile,
            dbus: RefCell::new(None),
            inner: RefCell::new(Inner { next_id: 1, backoff_ms: 500, ..Default::default() }),
            on_event: RefCell::new(None),
            on_conn: RefCell::new(None),
            connected: Cell::new(false),
            started_by: RefCell::new(String::new()),
        })
    }

    pub fn socket(&self) -> &Path {
        &self.path
    }

    pub fn is_connected(&self) -> bool {
        self.connected.get()
    }

    pub fn set_handlers(&self, on_event: impl Fn(&str, &Value) + 'static, on_conn: impl Fn(bool) + 'static) {
        *self.on_event.borrow_mut() = Some(Rc::new(on_event));
        *self.on_conn.borrow_mut() = Some(Rc::new(on_conn));
    }

    /// Kobler til (og starter tjenesten om nødvendig). Kalles etter første bilde.
    pub fn start(self: &Rc<Self>, dbus: Option<gio::DBusConnection>) {
        *self.dbus.borrow_mut() = dbus;
        if !self.try_connect() {
            self.start_service();
            self.inner.borrow_mut().fast_until = Some(Instant::now() + Duration::from_secs(2));
            self.schedule_retry(25);
        }
        // Svar som ikke kommer innen 5 s, gir en feil (tjenesten henger).
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(c) = weak.upgrade() else { return glib::ControlFlow::Break };
            c.expire(Duration::from_secs(5));
            glib::ControlFlow::Continue
        });
    }

    fn expire(&self, max: Duration) {
        let now = Instant::now();
        let late: Vec<u64> = self.inner.borrow().pending.iter().filter(|(_, (t, _))| now.duration_since(*t) > max).map(|(id, _)| *id).collect();
        for id in late {
            let cb = self.inner.borrow_mut().pending.remove(&id);
            if let Some((_, cb)) = cb {
                cb(Err(ProtoError::msg(ErrorCode::Unavailable, "Tjenesten svarte ikke på 5 s.")));
            }
        }
    }

    fn try_connect(self: &Rc<Self>) -> bool {
        let Ok(s) = UnixStream::connect(&self.path) else { return false };
        if s.set_nonblocking(true).is_err() {
            return false;
        }
        let fd = s.as_raw_fd();
        let weak = Rc::downgrade(self);
        let w = glib_unix_fd_add(fd, glib::IOCondition::IN | glib::IOCondition::HUP | glib::IOCondition::ERR, move || {
            let Some(c) = weak.upgrade() else { return glib::ControlFlow::Break };
            if c.on_readable() { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
        });
        {
            let mut i = self.inner.borrow_mut();
            i.stream = Some(s);
            i.in_watch = Some(w);
            i.lines = LineBuffer::new();
            i.out.clear();
            i.backoff_ms = 500;
            i.fast_until = None;
            if let Some(r) = i.retry.take() {
                r.remove();
            }
        }
        self.connected.set(true);
        let f = self.on_conn.borrow().clone();
        if let Some(f) = f {
            f(true);
        }
        true
    }

    fn schedule_retry(self: &Rc<Self>, ms: u64) {
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            let Some(c) = weak.upgrade() else { return };
            c.inner.borrow_mut().retry = None;
            if c.connected.get() || c.try_connect() {
                return;
            }
            let fast = c.inner.borrow().fast_until.is_some_and(|t| Instant::now() < t);
            let next = if fast {
                25
            } else {
                let mut i = c.inner.borrow_mut();
                let b = i.backoff_ms;
                i.backoff_ms = (b * 2).min(5000);
                b
            };
            c.schedule_retry(next);
        });
        if let Some(old) = self.inner.borrow_mut().retry.replace(id) {
            old.remove();
        }
    }

    /// Prøv igjen nå («Start på nytt» i linja «Tjenesten svarer ikke»).
    pub fn reconnect_now(self: &Rc<Self>) {
        if self.connected.get() || self.try_connect() {
            return;
        }
        self.inner.borrow_mut().started_service = false;
        self.start_service();
        self.inner.borrow_mut().fast_until = Some(Instant::now() + Duration::from_secs(2));
        self.schedule_retry(25);
    }

    fn start_service(self: &Rc<Self>) {
        if std::mem::replace(&mut self.inner.borrow_mut().started_service, true) {
            return;
        }
        let dbus = self.dbus.borrow().clone();
        match (self.profile, dbus) {
            (Profile::Prod, Some(conn)) => {
                let weak = Rc::downgrade(self);
                conn.call(
                    Some("org.freedesktop.systemd1"),
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    "StartUnit",
                    Some(&(UNIT, "replace").to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    3000,
                    None::<&gio::Cancellable>,
                    move |res| {
                        let Some(c) = weak.upgrade() else { return };
                        match res {
                            Ok(_) => *c.started_by.borrow_mut() = "systemd".into(),
                            // Enheten finnes ikke (ikke installert), eller ingen systemd --user.
                            Err(_) => c.spawn_service(),
                        }
                    },
                );
            }
            _ => self.spawn_service(),
        }
    }

    /// Reserven: start `agentic-riggd` fra samme mappe som vinduet, som egen
    /// prosessgruppe, så den lever videre når vinduet lukkes.
    fn spawn_service(&self) {
        let Some(bin) = std::env::current_exe().ok().map(|p| p.with_file_name("agentic-riggd")).filter(|p| p.exists()) else {
            eprintln!("Fant ikke agentic-riggd ved siden av vinduet.");
            return;
        };
        use std::os::unix::process::CommandExt;
        #[allow(clippy::disallowed_methods)] // Eneste unntak: reserven når systemd --user mangler (arkitektur.md kap. 8).
        let child = std::process::Command::new(&bin)
            .arg("--socket")
            .arg(&self.path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .process_group(0)
            .spawn();
        match child {
            Ok(mut c) => {
                *self.started_by.borrow_mut() = "prosess".into();
                // Rydd opp når den stopper (ingen zombie mens vinduet lever).
                std::thread::spawn(move || {
                    let _ = c.wait();
                });
            }
            Err(e) => eprintln!("Kunne ikke starte {}: {e}", bin.display()),
        }
    }

    fn on_readable(self: &Rc<Self>) -> bool {
        let mut buf = Vec::new();
        let mut closed = false;
        {
            let mut i = self.inner.borrow_mut();
            let Some(s) = i.stream.as_mut() else { return false };
            let mut chunk = [0u8; 32 * 1024];
            loop {
                match s.read(&mut chunk) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                }
            }
        }
        if !buf.is_empty() {
            let events = self.inner.borrow_mut().lines.push(&buf, 8 * 1024 * 1024);
            for e in events {
                match e {
                    LineEvent::Line(l) => self.dispatch(&l),
                    LineEvent::TooLarge => closed = true,
                }
            }
        }
        if closed {
            self.lost(true);
            return false;
        }
        true
    }

    fn dispatch(&self, line: &[u8]) {
        match proto::parse_incoming(line) {
            Some(Incoming::Event { event, data }) => {
                let f = self.on_event.borrow().clone();
                if let Some(f) = f {
                    f(&event, &data);
                }
            }
            Some(Incoming::Response { id, result }) => {
                let cb = id.as_u64().and_then(|id| self.inner.borrow_mut().pending.remove(&id));
                if let Some((_, cb)) = cb {
                    cb(result);
                }
            }
            None => {}
        }
    }

    /// Forbindelsen er brutt: alle som venter, får en feil, og vi prøver igjen.
    fn lost(self: &Rc<Self>, from_in_watch: bool) {
        let pending: Vec<Callback> = {
            let mut i = self.inner.borrow_mut();
            i.stream = None;
            if let Some(w) = i.out_watch.take() {
                w.remove();
            }
            if let Some(w) = i.in_watch.take()
                && !from_in_watch
            {
                w.remove();
            }
            i.pending.drain().map(|(_, (_, cb))| cb).collect()
        };
        for cb in pending {
            cb(Err(ProtoError::new(ErrorCode::Unavailable)));
        }
        if self.connected.replace(false) {
            let f = self.on_conn.borrow().clone();
            if let Some(f) = f {
                f(false);
            }
        }
        self.inner.borrow_mut().started_service = false;
        self.schedule_retry(500);
    }

    /// Sender en forespørsel. Uten forbindelse får `cb` en feil med en gang.
    pub fn request(self: &Rc<Self>, op: &str, args: Value, cb: impl FnOnce(OpResult) + 'static) {
        if !self.connected.get() {
            cb(Err(ProtoError::msg(ErrorCode::Unavailable, "Tjenesten svarer ikke.")));
            return;
        }
        let id = {
            let mut i = self.inner.borrow_mut();
            let id = i.next_id;
            i.next_id += 1;
            i.pending.insert(id, (Instant::now(), Box::new(cb)));
            let mut line = serde_json::to_vec(&json!({"id": id, "op": op, "args": args})).unwrap_or_default();
            line.push(b'\n');
            i.out.extend_from_slice(&line);
            id
        };
        let _ = id;
        self.flush();
    }

    fn flush(self: &Rc<Self>) {
        let mut failed = false;
        let mut need_watch = false;
        {
            let mut i = self.inner.borrow_mut();
            let Inner { stream, out, out_watch, .. } = &mut *i;
            let Some(s) = stream.as_mut() else { return };
            while !out.is_empty() {
                match s.write(out) {
                    Ok(0) => {
                        failed = true;
                        break;
                    }
                    Ok(n) => {
                        out.drain(..n);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        need_watch = out_watch.is_none();
                        break;
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
        }
        if failed {
            self.lost(false);
            return;
        }
        if need_watch {
            let fd = self.inner.borrow().stream.as_ref().map(|s| s.as_raw_fd());
            let Some(fd) = fd else { return };
            let weak = Rc::downgrade(self);
            let w = glib_unix_fd_add(fd, glib::IOCondition::OUT, move || {
                let Some(c) = weak.upgrade() else { return glib::ControlFlow::Break };
                c.inner.borrow_mut().out_watch = None;
                c.flush();
                glib::ControlFlow::Break
            });
            self.inner.borrow_mut().out_watch = Some(w);
        }
    }
}

fn glib_unix_fd_add(fd: i32, cond: glib::IOCondition, mut f: impl FnMut() -> glib::ControlFlow + 'static) -> glib::SourceId {
    glib_unix::unix_fd_add_local(fd, cond, move |_, _| f())
}
