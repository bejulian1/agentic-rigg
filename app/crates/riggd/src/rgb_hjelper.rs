//! Egen, avgrenset IPC-tråd for RGB. Ingen NVML-klient, I2C eller oppstartsvalg.
//! Hovedløkka legger bare arbeid i kø og behandler ferdige svar. Socketen har
//! totalfrister, og klienttap kan avbryte den uten å vente på root-workerens I2C.

use rigg_core::proto::{self, ErrorCode, Incoming, OpResult, ProtoError, Request};
use rigg_core::rgb::{Binding, Observed, Request as ColorRequest};
use rigg_core::rgb_auth::{self, Paths};
use rigg_core::rgb_effect::Effect;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const MAX_PENDING: usize = 4;
const MAX_REPLY: usize = 65_536;
const RPC_TIMEOUT: Duration = Duration::from_secs(3);
// GTK-klienten utløper forespørsler etter 5 s. Kø + hele IPC-runden må
// utløpe tidligere, slik at en sen køjobb ikke starter etter UI-feilmeldingen.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(4);
const HEARTBEAT: Duration = Duration::from_secs(2);

fn boottime_ms() -> Result<u64, String> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: rent klokkeoppslag til en gyldig timespec; ingen maskinvare-I/O.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return Err(format!(
            "RGB-klokken kunne ikke leses: {}",
            std::io::Error::last_os_error()
        ));
    }
    let seconds = u64::try_from(time.tv_sec).map_err(|_| "Ugyldig RGB-klokke.".to_string())?;
    let nanos = u64::try_from(time.tv_nsec).map_err(|_| "Ugyldig RGB-klokke.".to_string())?;
    if nanos >= 1_000_000_000 {
        return Err("Ugyldig RGB-klokke.".into());
    }
    seconds
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add(nanos / 1_000_000))
        .ok_or_else(|| "RGB-klokken er utenfor gyldig område.".into())
}

#[derive(Clone, Copy)]
struct Deadline {
    poll: Instant,
    queued_boot_ms: u64,
}

impl Deadline {
    fn check(self) -> Result<Duration, String> {
        self.check_at(boottime_ms())
    }

    /// Separat injiserbar klokkekontroll: Instant alene teller ikke suspend.
    fn check_at(self, boot_now: Result<u64, String>) -> Result<Duration, String> {
        self.check_age_at(boot_now)?;
        self.poll
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                "RGB-hjelperen svarte ikke innen fristen. Tilstanden er uavklart.".into()
            })
    }

    fn check_age_at(self, boot_now: Result<u64, String>) -> Result<(), String> {
        let age = boot_now?
            .checked_sub(self.queued_boot_ms)
            .ok_or("RGB-klokken gikk bakover. Tilstanden er uavklart.")?;
        if u128::from(age) >= TOTAL_TIMEOUT.as_millis() {
            return Err(
                "RGB-fristen er ute, inkludert hviletid. Tilstanden er uavklart og må hentes på nytt.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct View {
    state: &'static str,
    message: Option<String>,
    status: Value,
}

impl View {
    fn off() -> Self {
        Self {
            state: "off",
            message: None,
            status: Value::Null,
        }
    }
    fn error(message: impl Into<String>) -> Self {
        Self {
            state: "error",
            message: Some(message.into()),
            status: Value::Null,
        }
    }
    fn checked(status: Value) -> Result<Self, Self> {
        let helper = &status["helper"];
        if helper["name"] != rgb_auth::HELPER_NAME || helper["domain"] != rgb_auth::DOMAIN {
            return Err(Self::error(
                "Socketen svarte ikke som den separate RGB-hjelperen.",
            ));
        }
        if helper["strictRequests"] != true
            || helper["protocol"].as_u64() != Some(u64::from(rgb_auth::PROTOCOL))
            || !helper["build"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .is_some_and(rgb_auth::compatible_build)
        {
            return Err(Self {
                state: "outdated",
                message: Some("RGB-hjelperen må oppdateres. Trykk «Aktiver kortlys» for å oppdatere og aktivere den med ett passord.".into()),
                status: Value::Null,
            });
        }
        let binding: Binding = serde_json::from_value(status["identity"].clone())
            .map_err(|_| Self::error("RGB-hjelperen mangler en gyldig kortbinding."))?;
        binding.validate().map_err(Self::error)?;
        if status["authorized"] != true {
            return Err(Self::error("Den separate RGB-tillatelsen er ikke aktiv."));
        }
        let state = &status["state"];
        let observed: Observed = serde_json::from_value(state["observed"].clone())
            .map_err(|_| Self::error("RGB-hjelperen mangler avlesningsstatus."))?;
        if serde_json::from_value::<ColorRequest>(state["confirmed"].clone()).is_err()
            || !state["dirty"].is_boolean()
            || !state["mayHaveWritten"].is_boolean()
            || !matches!(
                status["observationSource"].as_str(),
                Some("stored_baseline" | "operation_readback" | "unknown")
            )
            || observed.mode.is_some()
        {
            return Err(Self::error(
                "RGB-status har uventede verdier eller udokumentert effektmodus.",
            ));
        }
        let owner = state["owner"].as_u64().filter(|n| *n > 0);
        if (!state["owner"].is_null() && owner.is_none())
            || (owner.is_some() && status["holding"].is_null())
            || (!status["holding"].is_null()
                && (status["holding"]["owner"].as_u64() != owner
                    || owner.is_none()
                    || status["holding"]["deadlineMs"].as_u64().is_none()
                    || status["holding"]["leaseMs"] != 15_000))
        {
            return Err(Self::error("RGB-hjelperens eierstatus er ugyldig."));
        }
        if !state["pending"].is_null() {
            let pending = &state["pending"];
            if owner.is_none()
                || pending["owner"].as_u64() != owner
                || pending["txn"].as_u64().is_none_or(|n| n == 0)
                || pending["deadlineMs"].as_u64().is_none()
                || serde_json::from_value::<ColorRequest>(pending["requested"].clone()).is_err()
            {
                return Err(Self::error("RGB-hjelperens ventende endring er ugyldig."));
            }
        }
        let effects: Vec<Effect> = match status.get("softwareEffects") {
            None => Vec::new(),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|_| Self::error("RGB-hjelperen oppga ukjente programvareeffekter."))?,
        };
        // Pusting, regnbue, fargesyklus og «hold» (keepalive fra 1.7.0).
        if effects.len() > 4
            || effects
                .iter()
                .enumerate()
                .any(|(i, effect)| effects[..i].contains(effect))
        {
            return Err(Self::error("RGB-hjelperens effektliste er ugyldig."));
        }
        if !status["effect"].is_null() {
            let effect: Effect = serde_json::from_value(status["effect"].clone())
                .map_err(|_| Self::error("RGB-hjelperens valgte effekt er ukjent."))?;
            let static_target = if state["pending"].is_null() {
                state["confirmed"]["mode"] == "static"
            } else {
                state["pending"]["requested"]["mode"] == "static"
            };
            if !effects.contains(&effect)
                || owner.is_none()
                || state["dirty"] != true
                || state["mayHaveWritten"] != true
                || !static_target
            {
                return Err(Self::error(
                    "RGB-effekten mangler gyldig støtte eller eierskap.",
                ));
            }
        }
        // Varig utgangspunkt (fra 1.7.0): fast farge/av eller null.
        let request_or_null = |value: &Value| value.is_null() || serde_json::from_value::<ColorRequest>(value.clone()).is_ok();
        if !request_or_null(&status["baselineRequest"])
            || !request_or_null(&status["factory"])
            || !(status["persist"].is_null() || status["persist"].is_boolean())
        {
            return Err(Self::error("RGB-hjelperens varige utgangspunkt er ugyldig."));
        }
        Ok(Self {
            state: "on",
            message: None,
            status,
        })
    }
}

#[derive(Default)]
struct Control {
    stopped: AtomicBool,
    owner: AtomicU64,
    interrupt: Mutex<Option<UnixStream>>,
}

impl Control {
    fn close_socket(&self) {
        if let Ok(socket) = self.interrupt.lock()
            && let Some(socket) = socket.as_ref()
        {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
    fn abandon(&self, sid: u64) {
        if self
            .owner
            .compare_exchange(sid, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.close_socket();
        }
    }
}

struct Command {
    seq: u64,
    sid: u64,
    alive: Arc<AtomicBool>,
    op: String,
    args: Value,
    queued: Instant,
    queued_boot_ms: u64,
}

struct Pending {
    sid: u64,
    id: Value,
    alive: Arc<AtomicBool>,
    mutation: bool,
    refresh: bool,
}

pub struct Completion {
    seq: Option<u64>,
    sid: u64,
    view: View,
    result: OpResult,
    deadline: Option<Deadline>,
}

impl Completion {
    pub fn session(&self) -> u64 {
        self.sid
    }
}

pub struct RgbClient {
    commands: mpsc::SyncSender<Command>,
    control: Arc<Control>,
    pending: BTreeMap<u64, Pending>,
    next_seq: u64,
    view: View,
    thread: Option<std::thread::JoinHandle<()>>,
    system_admin: bool,
}

impl RgbClient {
    pub fn start(paths: Option<Paths>) -> (Self, async_channel::Receiver<Completion>) {
        let (commands, requests) = mpsc::sync_channel(MAX_PENDING);
        let (events, rx) = async_channel::bounded(MAX_PENDING + 2);
        let control = Arc::new(Control::default());
        let system_admin = paths
            .as_ref()
            .is_some_and(|paths| paths.root == Path::new("/"));
        let thread = paths.map(|paths| {
            let worker_control = control.clone();
            std::thread::Builder::new()
                .name("rigg-rgb-ipc".into())
                .spawn(move || worker(paths, worker_control, requests, events))
        });
        let (thread, view) = match thread {
            Some(Ok(thread)) => (Some(thread), View::off()),
            Some(Err(error)) => (
                None,
                View::error(format!("RGB-tråden kunne ikke starte: {error}")),
            ),
            None => (
                None,
                View {
                    state: "disabled",
                    message: Some(
                        "Simulering og utvikling krever en egen privat RGB-hjelperrot.".into(),
                    ),
                    status: Value::Null,
                },
            ),
        };
        (
            Self {
                commands,
                control,
                pending: BTreeMap::new(),
                next_seq: 1,
                view,
                thread,
                system_admin,
            },
            rx,
        )
    }

    pub fn is_operation(op: &str) -> bool {
        op == "get_rgb_helper" || rgb_auth::UI_OPS.contains(&op)
    }

    pub fn view(&self, sid: u64) -> Value {
        let owner = self.control.owner.load(Ordering::SeqCst);
        json!({"state": self.view.state, "message": self.view.message, "status": self.view.status,
            "systemAdmin": self.system_admin,
            "owned": owner == sid && sid != 0,
            "busy": (owner != 0 && owner != sid)
                || (owner == 0 && !self.view.status["holding"].is_null())
                || self.pending.values().any(|p| p.mutation)})
    }

    pub fn holds_values(&self) -> bool {
        self.control.owner.load(Ordering::SeqCst) != 0
    }

    #[cfg(test)]
    pub(crate) fn set_test_owner(&self, owner: u64) {
        self.control.owner.store(owner, Ordering::SeqCst);
    }

    /// `None` betyr at svaret kommer i completion-køen. Kalleren har allerede
    /// kontrollert at UI-økta finnes, har sagt hello og ikke stopper.
    pub fn enqueue(&mut self, sid: u64, request: &Request) -> Result<Option<Value>, ProtoError> {
        // Hvor i appen kallet kom fra («kilde»), bare til loggen: hjelperen får
        // aldri feltet. Hver endring logges, så en uventet transaksjon kan spores.
        let mut request = request.clone();
        let source = take_source(&mut request.args)?;
        validate_args(&request.op, &request.args)?;
        if request.op != "get_rgb_helper" {
            eprintln!("{}", log_line(sid, &request.op, source.as_deref(), &request.args));
        }
        let request = &request;
        let refresh = request.op == "get_rgb_helper";
        if self.view.state == "disabled" {
            return if refresh {
                Ok(Some(self.view(sid)))
            } else {
                Err(ProtoError::msg(
                    ErrorCode::WriteLocked,
                    self.view.message.clone().unwrap_or_default(),
                ))
            };
        }
        let owner = self.control.owner.load(Ordering::SeqCst);
        if refresh
            && (request.args["refresh"] != true
                || (owner != 0 && owner != sid)
                || self.pending.values().any(|p| p.mutation))
        {
            return Ok(Some(self.view(sid)));
        }
        if self.control.stopped.load(Ordering::SeqCst) {
            return Err(ProtoError::new(ErrorCode::ShuttingDown));
        }
        if sid == 0 || self.pending.len() >= MAX_PENDING {
            return Err(ProtoError::new(ErrorCode::Busy));
        }
        if !refresh {
            if owner != 0 && owner != sid {
                return Err(ProtoError::new(ErrorCode::NotOwner));
            }
            if self.pending.values().any(|p| p.mutation) {
                return Err(ProtoError::new(ErrorCode::Busy));
            }
            if owner == 0
                && matches!(
                    request.op.as_str(),
                    "rgb_confirm" | "rgb_revert" | "rgb_release"
                )
            {
                return Err(ProtoError::new(ErrorCode::NotOwner));
            }
        }
        let seq = self.next_seq;
        let queued_boot_ms = boottime_ms().map_err(|message| {
            if owner != 0 {
                self.control.abandon(owner);
            }
            self.view = View::error(&message);
            ProtoError::msg(ErrorCode::Unavailable, message)
        })?;
        self.next_seq = self
            .next_seq
            .checked_add(1)
            .ok_or_else(|| ProtoError::new(ErrorCode::Internal))?;
        let alive = Arc::new(AtomicBool::new(true));
        let command = Command {
            seq,
            sid,
            alive: alive.clone(),
            op: request.op.clone(),
            args: request.args.clone(),
            queued: Instant::now(),
            queued_boot_ms,
        };
        if !refresh {
            self.control.owner.store(sid, Ordering::SeqCst);
        }
        if self.commands.try_send(command).is_err() {
            if !refresh && owner == 0 {
                self.control.abandon(sid);
            }
            return Err(ProtoError::new(ErrorCode::Busy));
        }
        self.pending.insert(
            seq,
            Pending {
                sid,
                id: request.id.clone(),
                alive,
                mutation: !refresh,
                refresh,
            },
        );
        Ok(None)
    }

    pub fn complete(&mut self, event: Completion) -> Option<(u64, Value, OpResult)> {
        self.complete_at(event, boottime_ms())
    }

    fn complete_at(
        &mut self,
        mut event: Completion,
        boot_now: Result<u64, String>,
    ) -> Option<(u64, Value, OpResult)> {
        if let Some(seq) = event.seq {
            let pending = self.pending.remove(&seq)?;
            if pending.sid != event.sid || !pending.alive.load(Ordering::SeqCst) {
                return None;
            }
            self.expire_completion(&mut event, boot_now);
            self.view = event.view;
            let result = if pending.refresh {
                Ok(self.view(pending.sid))
            } else {
                event.result
            };
            Some((pending.sid, pending.id, result))
        } else {
            // Et gammelt heartbeat-svar må ikke vekke en avsluttet UI-økt.
            if [0, event.sid].contains(&self.control.owner.load(Ordering::SeqCst)) {
                self.expire_completion(&mut event, boot_now);
                self.view = event.view;
            }
            None
        }
    }

    fn expire_completion(&self, event: &mut Completion, boot_now: Result<u64, String>) {
        let check = event
            .deadline
            .ok_or_else(|| "RGB-klokken kunne ikke kontrolleres.".to_string())
            .and_then(|deadline| deadline.check_age_at(boot_now));
        if let Err(message) = check {
            self.control.abandon(event.sid);
            event.view = View::error(&message);
            event.result = Err(ProtoError::msg(ErrorCode::Unavailable, message));
        }
    }

    pub fn session_closed(&mut self, sid: u64) {
        self.pending.retain(|_, pending| {
            if pending.sid == sid {
                pending.alive.store(false, Ordering::SeqCst);
                false
            } else {
                true
            }
        });
        if self.control.owner.load(Ordering::SeqCst) == sid {
            self.control.abandon(sid);
            self.view =
                View::error("RGB-forbindelsen er lukket. Gjenopprettingsstatus må hentes på nytt.");
        }
    }

    pub fn stop(&mut self) {
        self.control.stopped.store(true, Ordering::SeqCst);
        self.control.owner.store(0, Ordering::SeqCst);
        for pending in self.pending.values() {
            pending.alive.store(false, Ordering::SeqCst);
        }
        self.pending.clear();
        self.control.close_socket();
        // Hovedløkken venter aldri på IPC. Tråden ser stopp senest i neste
        // 100 ms poll; ferdige tråder reapes, ellers avslutter de selv.
        if let Some(thread) = self.thread.take()
            && thread.is_finished()
        {
            let _ = thread.join();
        }
    }
}

impl Drop for RgbClient {
    fn drop(&mut self) {
        self.stop();
    }
}

fn invalid(message: &str) -> ProtoError {
    ProtoError::msg(ErrorCode::InvalidArgument, message)
}

/// Fjerner og kontrollerer `kilde` (kort, fast tekst fra appen, for eksempel
/// «bruk-knapp» eller «etter-aktivering»).
fn take_source(args: &mut Value) -> Result<Option<String>, ProtoError> {
    let Some(object) = args.as_object_mut() else { return Ok(None) };
    let Some(value) = object.remove("kilde") else { return Ok(None) };
    let source = value.as_str().filter(|s| (1..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
    source.map(|s| Some(s.to_string())).ok_or_else(|| invalid("Ugyldig kilde for RGB-kallet."))
}

/// Én linje i tjenestens logg for et RGB-kall: økt, operasjon, kilde og valget.
fn log_line(sid: u64, op: &str, source: Option<&str>, args: &Value) -> String {
    let what = match op {
        "rgb_apply" => format!(
            " valg={} effekt={}",
            serde_json::to_string(&args["request"]).unwrap_or_default(),
            args["effect"].as_str().unwrap_or("-")
        ),
        "rgb_confirm" | "rgb_revert" => format!(" txn={}", args["txn"]),
        _ => String::new(),
    };
    format!("RGB: økt {sid} {op} fra {}{what}", source.unwrap_or("ukjent"))
}

fn validate_args(op: &str, args: &Value) -> Result<(), ProtoError> {
    let object = args
        .as_object()
        .ok_or_else(|| invalid("RGB-argumentene må være et objekt."))?;
    let allowed: &[&str] = match op {
        "get_rgb_helper" => &["refresh"],
        "rgb_apply" => &["gpu", "uuid", "request", "confirmTimeoutS", "effect"],
        "rgb_confirm" | "rgb_revert" => &["gpu", "uuid", "txn"],
        "rgb_ack_recovery" => &["gpu", "uuid", "confirmedStatic", "expected", "recoveryId"],
        "rgb_reset" | "rgb_release" | "rgb_read" | "rgb_persist" => &["gpu", "uuid"],
        _ => return Err(ProtoError::new(ErrorCode::UnknownOp)),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("Ukjent RGB-argument."));
    }
    if op == "get_rgb_helper" {
        return if object.get("refresh").is_none_or(Value::is_boolean) {
            Ok(())
        } else {
            Err(invalid("refresh må være true eller false."))
        };
    }
    if !args["gpu"]
        .as_str()
        .is_some_and(rigg_core::hjelper::valid_pci)
        || !args["uuid"]
            .as_str()
            .is_some_and(rigg_core::hjelper::valid_uuid)
    {
        return Err(invalid("RGB trenger nøyaktig PCI-adresse og GPU-UUID."));
    }
    if op == "rgb_apply"
        && (!args["confirmTimeoutS"]
            .as_u64()
            .is_some_and(|n| (10..=60).contains(&n))
            || serde_json::from_value::<ColorRequest>(args["request"].clone()).is_err())
    {
        return Err(invalid(
            "RGB trenger statisk farge/av og en frist på 10–60 sekunder.",
        ));
    }
    if matches!(op, "rgb_confirm" | "rgb_revert") && args["txn"].as_u64().is_none_or(|n| n == 0) {
        return Err(invalid("RGB-transaksjonen mangler."));
    }
    if op == "rgb_apply"
        && let Some(effect) = args.get("effect")
        && (serde_json::from_value::<Effect>(effect.clone()).is_err()
            || args["request"]["mode"] != "static")
    {
        return Err(invalid(
            "En kjent programvareeffekt krever statisk startfarge.",
        ));
    }
    if op == "rgb_ack_recovery"
        && (args["confirmedStatic"] != true
            || !args["recoveryId"]
                .as_str()
                .is_some_and(|id| !id.is_empty() && id.len() <= 53)
            || serde_json::from_value::<ColorRequest>(args["expected"].clone()).is_err())
    {
        return Err(invalid(
            "Bekreft at den viste grunnfargen er riktig og lyser statisk.",
        ));
    }
    Ok(())
}

struct Connection {
    stream: UnixStream,
    sequence: u64,
    binding: Option<Binding>,
}

fn cancelled(control: &Control, alive: Option<&AtomicBool>) -> bool {
    control.stopped.load(Ordering::SeqCst) || alive.is_some_and(|a| !a.load(Ordering::SeqCst))
}

fn wait_io(
    stream: &UnixStream,
    events: i16,
    deadline: Deadline,
    control: &Control,
    alive: Option<&AtomicBool>,
) -> Result<(), String> {
    loop {
        if cancelled(control, alive) {
            return Err("RGB-forespørselen ble avbrutt.".into());
        }
        let remaining = deadline.check()?;
        let mut fd = libc::pollfd {
            fd: stream.as_raw_fd(),
            events,
            revents: 0,
        };
        // SAFETY: én gyldig socketdescriptor, og høyst 100 ms venting.
        let n = unsafe { libc::poll(&mut fd, 1, remaining.as_millis().clamp(1, 100) as i32) };
        if n > 0 {
            deadline.check()?;
            return Ok(());
        }
        if n < 0 && std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
}

/// Ingen klientstyrte stier. En falsk rot må være eksplisitt, kanonisk og
/// privat 0700; en symlink til vertens / kan aldri bli testmodus.
fn socket_metadata(paths: &Paths) -> Result<Option<(std::fs::Metadata, u32)>, String> {
    let uid = crate::server::my_uid();
    let root = &paths.root;
    let real = root == Path::new("/");
    if !real {
        let meta = match std::fs::symlink_metadata(root) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
            Ok(meta) => meta,
        };
        if !root.is_absolute()
            || std::fs::canonicalize(root).map_err(|e| e.to_string())? != *root
            || !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != uid
            || meta.mode() & 0o777 != 0o700
        {
            return Err(
                "RGB-testroten må være en kanonisk, privat mappe eid av denne brukeren.".into(),
            );
        }
    }
    let path = paths.socket();
    let expected = if real { 0 } else { uid };
    for directory in [
        root.join("run"),
        path.parent()
            .ok_or("RGB-socketen mangler mappe.")?
            .to_path_buf(),
    ] {
        let meta = match std::fs::symlink_metadata(&directory) {
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
            Ok(meta) => meta,
        };
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != expected
            || meta.mode() & 0o022 != 0
        {
            return Err("RGB-socketen ligger i en utrygg mappe.".into());
        }
    }
    let metadata = match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(meta) => meta,
    };
    if !metadata.file_type().is_socket()
        || ![expected, uid].contains(&metadata.uid())
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
    {
        return Err("RGB-socketens type, eier eller modus er ugyldig.".into());
    }
    Ok(Some((metadata, expected)))
}

impl Connection {
    fn open(
        paths: &Paths,
        control: &Control,
        alive: &AtomicBool,
        deadline: Deadline,
    ) -> Result<Option<Self>, String> {
        deadline.check()?;
        let Some((before, expected_uid)) = socket_metadata(paths)? else {
            return Ok(None);
        };
        let path = paths.socket();
        let bytes = path.as_os_str().as_encoded_bytes();
        // SAFETY: sockaddr_un består av heltall og bytefelt; null er gyldig.
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
            return Err("RGB-socketstien er ugyldig.".into());
        }
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (to, from) in address.sun_path.iter_mut().zip(bytes) {
            *to = *from as libc::c_char;
        }
        // SAFETY: vanlig ikke-blokkerende Unix-socket, ingen enhetsåpning.
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: fd er nyopprettet og overføres én gang til UnixStream.
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
        deadline.check()?;
        // SAFETY: address og length beskriver én gyldig, terminert Unix-sti.
        let result = unsafe {
            libc::connect(
                fd,
                &address as *const _ as *const libc::sockaddr,
                length as libc::socklen_t,
            )
        };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(error.to_string());
            }
            wait_io(&stream, libc::POLLOUT, deadline, control, Some(alive))?;
            if let Some(error) = stream.take_error().map_err(|e| e.to_string())? {
                return Err(error.to_string());
            }
        }
        deadline.check()?;
        if crate::server::peer_cred(&stream).map(|(uid, _)| uid) != Some(expected_uid) {
            return Err("RGB-socketen tilhører ikke den forventede hjelperbrukeren.".into());
        }
        let Some((after, after_uid)) = socket_metadata(paths)? else {
            return Err("RGB-socketen forsvant under tilkobling.".into());
        };
        if before.dev() != after.dev() || before.ino() != after.ino() || expected_uid != after_uid {
            return Err("RGB-socketen ble byttet under tilkobling.".into());
        }
        *control
            .interrupt
            .lock()
            .map_err(|_| "RGB-avbruddslåsen feilet.")? =
            Some(stream.try_clone().map_err(|e| e.to_string())?);
        Ok(Some(Self {
            stream,
            sequence: 0,
            binding: None,
        }))
    }

    fn checked_view(&mut self, status: Value) -> Result<View, View> {
        let view = View::checked(status)?;
        let binding: Binding = serde_json::from_value(view.status["identity"].clone())
            .map_err(|_| View::error("RGB-kortbindingen kunne ikke leses."))?;
        if self
            .binding
            .as_ref()
            .is_some_and(|previous| *previous != binding)
        {
            return Err(View::error(
                "RGB-kortbindingen ble endret på en åpen forbindelse.",
            ));
        }
        self.binding = Some(binding);
        Ok(view)
    }

    fn request(
        &mut self,
        op: &str,
        args: &Value,
        deadline: Deadline,
        control: &Control,
        alive: Option<&AtomicBool>,
    ) -> Result<OpResult, String> {
        deadline.check()?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("RGB-forespørselsnummeret er brukt opp.")?;
        let id = self.sequence;
        let mut line =
            serde_json::to_vec(&json!({"id":id,"op":op,"args":args})).map_err(|e| e.to_string())?;
        line.push(b'\n');
        let mut written = 0;
        while written < line.len() {
            wait_io(&self.stream, libc::POLLOUT, deadline, control, alive)?;
            deadline.check()?;
            match self.stream.write(&line[written..]) {
                Ok(0) => return Err("RGB-hjelperen lukket forbindelsen.".into()),
                Ok(n) => written += n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        let mut answer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            wait_io(&self.stream, libc::POLLIN, deadline, control, alive)?;
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err("RGB-hjelperen lukket forbindelsen.".into()),
                Ok(n) => answer.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            }
            deadline.check()?;
            if answer.len() > MAX_REPLY {
                return Err("RGB-hjelperens svar er for stort.".into());
            }
            if let Some(end) = answer.iter().position(|b| *b == b'\n') {
                if end + 1 != answer.len() {
                    return Err("RGB-hjelperen sendte uventede ekstra svar.".into());
                }
                deadline.check()?;
                return match proto::parse_incoming(&answer[..end]) {
                    Some(Incoming::Response {
                        id: received,
                        result,
                    }) if received == json!(id) => Ok(result),
                    _ => Err("RGB-hjelperens svar har feil format eller id.".into()),
                };
            }
        }
    }
}

fn drop_connection(connection: &mut Option<Connection>, control: &Control) {
    if let Some(connection) = connection.take() {
        let _ = connection.stream.shutdown(std::net::Shutdown::Both);
    }
    if let Ok(mut interrupt) = control.interrupt.lock() {
        *interrupt = None;
    }
}

fn worker(
    paths: Paths,
    control: Arc<Control>,
    commands: mpsc::Receiver<Command>,
    events: async_channel::Sender<Completion>,
) {
    let mut connection = None;
    let mut view = View::off();
    let mut held_sid = 0;
    let mut heartbeat = Instant::now();
    while !control.stopped.load(Ordering::SeqCst) {
        let owner = control.owner.load(Ordering::SeqCst);
        if held_sid != 0 && held_sid != owner {
            drop_connection(&mut connection, &control);
            held_sid = 0;
        }
        if held_sid != 0 && heartbeat.elapsed() >= HEARTBEAT {
            heartbeat = Instant::now();
            let sid = held_sid;
            let heartbeat_deadline = boottime_ms().map(|queued_boot_ms| Deadline {
                poll: Instant::now() + RPC_TIMEOUT,
                queued_boot_ms,
            });
            let args = json!({"gpu":view.status["identity"]["pci"],"uuid":view.status["identity"]["uuid"]});
            let reply = connection
                .as_mut()
                .ok_or_else(|| "RGB-forbindelsen mangler.".into())
                .and_then(|connection: &mut Connection| {
                    connection.request(
                        "rgb_heartbeat",
                        &args,
                        heartbeat_deadline.clone()?,
                        &control,
                        None,
                    )
                });
            match reply {
                Ok(Ok(result)) => match connection
                    .as_mut()
                    .expect("heartbeat har forbindelse")
                    .checked_view(result["status"].clone())
                {
                    Ok(next) => view = next,
                    Err(next) => {
                        view = next;
                        control.abandon(sid);
                    }
                },
                Ok(Err(error)) => {
                    view = View::error(error.message);
                    control.abandon(sid);
                }
                Err(error) => {
                    view = View::error(error);
                    control.abandon(sid);
                }
            }
            if control.owner.load(Ordering::SeqCst) != sid {
                drop_connection(&mut connection, &control);
                held_sid = 0;
            }
            let _ = events.try_send(Completion {
                seq: None,
                sid,
                view: view.clone(),
                result: Ok(Value::Null),
                deadline: heartbeat_deadline.ok(),
            });
        }
        let command = match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if cancelled(&control, Some(&command.alive)) {
            continue;
        }
        let now = Instant::now();
        let total_deadline = command.queued + TOTAL_TIMEOUT;
        let deadline = Deadline {
            poll: (now + RPC_TIMEOUT).min(total_deadline),
            queued_boot_ms: command.queued_boot_ms,
        };
        let mut result = match deadline.check() {
            Err(message) => {
                control.abandon(command.sid);
                drop_connection(&mut connection, &control);
                view = View::error(&message);
                Err(ProtoError::msg(ErrorCode::Unavailable, message))
            }
            Ok(_) => execute(
                &paths,
                &control,
                &command,
                &mut connection,
                &mut view,
                deadline,
                held_sid == command.sid,
            ),
        };
        // Suspend kan også skje etter at svaret er lest. Ikke behold eller
        // presenter en sen suksess som om forespørselen fortsatt var gyldig.
        if let Err(message) = deadline.check() {
            control.abandon(command.sid);
            drop_connection(&mut connection, &control);
            view = View::error(&message);
            result = Err(ProtoError::msg(ErrorCode::Unavailable, message));
        }
        if cancelled(&control, Some(&command.alive)) {
            drop_connection(&mut connection, &control);
            held_sid = 0;
            continue;
        }
        if command.op != "get_rgb_helper" {
            if view.state == "on"
                && !view.status["holding"].is_null()
                && !matches!(&result, Err(error) if error.code == ErrorCode::NotOwner || (error.code == ErrorCode::Busy && held_sid != command.sid))
            {
                // Op-svar er ikke heartbeat. Feil og ikke-fornyende
                // operasjoner må aldri skyve neste faktiske heartbeat ut.
                if held_sid != command.sid {
                    heartbeat = Instant::now();
                }
                held_sid = command.sid;
            } else {
                control
                    .owner
                    .compare_exchange(command.sid, 0, Ordering::SeqCst, Ordering::SeqCst)
                    .ok();
                held_sid = 0;
            }
        } else if held_sid == command.sid && view.status["holding"].is_null() {
            control
                .owner
                .compare_exchange(command.sid, 0, Ordering::SeqCst, Ordering::SeqCst)
                .ok();
            held_sid = 0;
        }
        let event = Completion {
            seq: Some(command.seq),
            sid: command.sid,
            view: view.clone(),
            result,
            deadline: Some(deadline),
        };
        // Fire pending RPC-er + ett heartbeat får plass. Ingen ubundet
        // produksjon av ferdige svar dersom GLib-hovedløkken stopper.
        if events.try_send(event).is_err() {
            control.abandon(command.sid);
            drop_connection(&mut connection, &control);
            break;
        }
    }
    control.stopped.store(true, Ordering::SeqCst);
    drop_connection(&mut connection, &control);
}

#[allow(clippy::too_many_arguments)]
fn execute(
    paths: &Paths,
    control: &Control,
    command: &Command,
    connection: &mut Option<Connection>,
    view: &mut View,
    deadline: Deadline,
    already_owned: bool,
) -> OpResult {
    let run = (|| -> Result<OpResult, String> {
        deadline.check()?;
        if connection.is_none() {
            *connection = Connection::open(paths, control, &command.alive, deadline)?;
        }
        let Some(connection) = connection.as_mut() else {
            *view = View::off();
            return Ok(Err(ProtoError::msg(
                ErrorCode::WriteLocked,
                "Den separate RGB-hjelperen er av.",
            )));
        };
        let status = connection
            .request(
                "rgb_status",
                &json!({}),
                deadline,
                control,
                Some(&command.alive),
            )?
            .map_err(|error| error.message)?;
        deadline.check()?;
        *view = connection.checked_view(status).map_err(|next| {
            let message = next.message.clone().unwrap_or_default();
            *view = next;
            message
        })?;
        if command.op == "get_rgb_helper" {
            return Ok(Ok(Value::Null));
        }
        if !already_owned && !view.status["holding"].is_null() {
            return Ok(Err(ProtoError::new(ErrorCode::Busy)));
        }
        if command.args["gpu"] != view.status["identity"]["pci"]
            || command.args["uuid"] != view.status["identity"]["uuid"]
        {
            return Ok(Err(ProtoError::new(ErrorCode::UnknownGpu)));
        }
        if let Some(effect) = command.args.get("effect")
            && view.status["softwareEffects"]
                .as_array()
                .is_none_or(|effects| !effects.contains(effect))
        {
            return Ok(Err(ProtoError::new(ErrorCode::NotSupported)));
        }
        // Sjekkes rett før utsending, også når statuskallet tok tid.
        if control.owner.load(Ordering::SeqCst) != command.sid
            || !command.alive.load(Ordering::SeqCst)
        {
            return Err("RGB-økta er avsluttet.".into());
        }
        deadline.check()?;
        let result = connection.request(
            &command.op,
            &command.args,
            deadline,
            control,
            Some(&command.alive),
        )?;
        deadline.check()?;
        let status = match &result {
            Ok(value) => Some(value["status"].clone()),
            Err(error) => error
                .details
                .as_ref()
                .and_then(|details| details.get("status"))
                .cloned(),
        };
        if let Some(status) = status {
            *view = connection.checked_view(status).map_err(|next| {
                let message = next.message.clone().unwrap_or_default();
                *view = next;
                message
            })?;
        } else {
            let status = connection
                .request(
                    "rgb_status",
                    &json!({}),
                    deadline,
                    control,
                    Some(&command.alive),
                )?
                .map_err(|error| error.message)?;
            *view = connection.checked_view(status).map_err(|next| {
                let message = next.message.clone().unwrap_or_default();
                *view = next;
                message
            })?;
        }
        deadline.check()?;
        Ok(result)
    })();
    match run {
        Ok(result) => result,
        Err(message) => {
            if view.state != "outdated" {
                *view = View::error(&message);
            }
            control.abandon(command.sid);
            drop_connection(connection, control);
            Err(ProtoError::msg(ErrorCode::Unavailable, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::rgb::{AdapterIdentity, PciIdentity, Signature};
    use std::io::{BufRead, BufReader};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = rigg_core::paths::short_tmp().join(format!(
                "rigg-rgb-ipc-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn paths(&self) -> Paths {
            Paths::new(self.0.clone())
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-rgb-ipc-private-fixture".into(),
            pci_ids: PciIdentity {
                vendor: 0x10de,
                device: 0x1e87,
                subsystem_vendor: 0x10de,
                subsystem_device: 0x1e87,
            },
            adapter: AdapterIdentity {
                port: 1,
                name: "NVIDIA i2c adapter 1 at 1c:00.0".into(),
                parent_pci: "0000:1c:00.0".into(),
                driver: "nvidia".into(),
            },
            address: 0x49,
            signature: Signature::Palit100,
        }
    }
    fn initial_status() -> Value {
        json!({"helper":{"name":rgb_auth::HELPER_NAME,"domain":rgb_auth::DOMAIN,"protocol":rgb_auth::PROTOCOL,"strictRequests":true,"build":rigg_core::version::build()},
            "identity":binding(),"authorized":true,"observationSource":"stored_baseline","holding":null,
            "state":{"confirmed":{"mode":"static","color":[1,2,3],"brightness":80},
                "observed":{"color":[1,2,3],"brightnessRaw":80,"mode":null},"owner":null,
                "pending":null,"dirty":false,"mayHaveWritten":false}})
    }
    fn identity() -> Value {
        json!({"gpu":binding().pci,"uuid":binding().uuid})
    }
    fn request(id: u64, op: &str) -> Request {
        let mut args = if op == "get_rgb_helper" {
            json!({"refresh":true})
        } else {
            identity()
        };
        match op {
            "rgb_apply" => {
                args["request"] = json!({"mode":"static","color":[9,8,7],"brightness":80});
                args["confirmTimeoutS"] = json!(10);
            }
            "rgb_confirm" | "rgb_revert" => args["txn"] = json!(1),
            "rgb_ack_recovery" => {
                args["confirmedStatic"] = json!(true);
                args["expected"] = initial_status()["state"]["confirmed"].clone();
                args["recoveryId"] = json!("private-fixture-1");
            }
            _ => {}
        }
        Request {
            id: json!(id),
            op: op.into(),
            args,
        }
    }

    struct Fake {
        stop: Arc<AtomicBool>,
        gate: Arc<AtomicBool>,
        log: Arc<Mutex<Vec<String>>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Fake {
        fn start(paths: &Paths, blocked: bool) -> Self {
            let socket = paths.socket();
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o755)
                .create(socket.parent().unwrap())
                .unwrap();
            let listener = UnixListener::bind(&socket).unwrap();
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let gate = Arc::new(AtomicBool::new(!blocked));
            let log = Arc::new(Mutex::new(Vec::new()));
            let (end, ready, calls) = (stop.clone(), gate.clone(), log.clone());
            let thread = std::thread::spawn(move || {
                while !end.load(Ordering::SeqCst) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(_) => break,
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_millis(100)))
                        .unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut status = initial_status();
                    while !end.load(Ordering::SeqCst) {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => break,
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    ErrorKind::WouldBlock | ErrorKind::TimedOut
                                ) =>
                            {
                                continue;
                            }
                            Err(_) => break,
                            _ => {}
                        }
                        let frame: Value = serde_json::from_str(&line).unwrap();
                        let op = frame["op"].as_str().unwrap();
                        calls.lock().unwrap().push(op.into());
                        while !ready.load(Ordering::SeqCst) && !end.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        if end.load(Ordering::SeqCst) {
                            break;
                        }
                        if op == "rgb_apply" {
                            status["state"]["owner"] = json!(77);
                            status["state"]["dirty"] = json!(true);
                            status["state"]["mayHaveWritten"] = json!(true);
                            status["state"]["pending"] = json!({"owner":77,"txn":1,"deadlineMs":10_000,"requested":frame["args"]["request"]});
                            status["state"]["observed"]["color"] =
                                frame["args"]["request"]["color"].clone();
                            status["holding"] =
                                json!({"owner":77,"deadlineMs":15_000,"leaseMs":15_000});
                            status["observationSource"] = json!("operation_readback");
                        } else if op == "rgb_confirm" {
                            status["state"]["confirmed"] =
                                status["state"]["pending"]["requested"].clone();
                            status["state"]["pending"] = Value::Null;
                        } else if matches!(op, "rgb_release" | "rgb_ack_recovery") {
                            status["holding"] = Value::Null;
                            status["state"]["owner"] = Value::Null;
                            status["state"]["pending"] = Value::Null;
                        }
                        let result = if op == "rgb_status" {
                            status.clone()
                        } else {
                            json!({"outcome":{"txn":1},"status":status})
                        };
                        let reply = proto::response_line(&frame["id"], &Ok(result));
                        if stream.write_all(&reply).is_err() {
                            break;
                        }
                    }
                    calls.lock().unwrap().push("closed".into());
                }
            });
            Self {
                stop,
                gate,
                log,
                thread: Some(thread),
            }
        }
        fn has(&self, op: &str) -> bool {
            self.log.lock().unwrap().iter().any(|seen| seen == op)
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.gate.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                thread.join().unwrap();
            }
        }
    }
    fn wait(until: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !until() {
            assert!(Instant::now() < deadline, "testfristen utløp");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn complete(client: &mut RgbClient, rx: &async_channel::Receiver<Completion>) -> OpResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(event) = rx.try_recv()
                && let Some((_, _, result)) = client.complete(event)
            {
                return result;
            }
            assert!(Instant::now() < deadline, "RGB-svaret mangler");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn status_krever_riktig_domene_tillatelse_bygg_og_hel_binding() {
        assert!(View::checked(initial_status()).is_ok());
        for (field, value) in [
            ("name", json!("agentic-rigg-helper")),
            ("domain", json!("nvml")),
            ("protocol", json!(1)),
            ("build", json!(rigg_core::version::build() + 1)),
        ] {
            let mut status = initial_status();
            status["helper"][field] = value;
            assert!(View::checked(status).is_err());
        }
        let mut status = initial_status();
        status["authorized"] = json!(false);
        assert!(View::checked(status).is_err());
        let mut status = initial_status();
        status["identity"]["address"] = json!(0x50);
        assert!(View::checked(status).is_err());
        let mut status = initial_status();
        status["state"]["observed"]["mode"] = json!("static");
        assert!(View::checked(status).is_err());
    }

    #[test]
    fn kilden_logges_og_fjernes_for_hjelperen() {
        let mut args = request(1, "rgb_apply").args;
        args["kilde"] = json!("etter-aktivering");
        assert_eq!(take_source(&mut args).unwrap().as_deref(), Some("etter-aktivering"));
        assert!(args.get("kilde").is_none(), "hjelperen får aldri kilden");
        assert!(validate_args("rgb_apply", &args).is_ok());
        let line = log_line(7, "rgb_apply", Some("etter-aktivering"), &args);
        assert!(line.contains("økt 7 rgb_apply fra etter-aktivering") && line.contains("\"brightness\":80"), "{line}");
        assert!(log_line(7, "rgb_persist", None, &args).ends_with("rgb_persist fra ukjent"));
        for bad in [json!(""), json!("Bruk knapp"), json!(3), json!("x".repeat(41))] {
            let mut args = request(1, "rgb_apply").args;
            args["kilde"] = bad;
            assert!(take_source(&mut args).is_err());
        }
        assert!(validate_args("rgb_persist", &identity()).is_ok());
        let mut extra = identity();
        extra["request"] = json!({"mode":"off"});
        assert!(validate_args("rgb_persist", &extra).is_err());
    }

    #[test]
    fn effekter_valideres_for_koling_og_krever_eksplisitt_hjelperstotte() {
        let mut args = request(1, "rgb_apply").args;
        args["request"] = json!({"mode":"static","color":[20,40,60],"brightness":80});
        for effect in ["breathing", "rainbow", "cycle"] {
            args["effect"] = json!(effect);
            assert!(validate_args("rgb_apply", &args).is_ok());
        }
        for effect in [Value::Null, json!("native"), json!(true)] {
            args["effect"] = effect;
            assert!(validate_args("rgb_apply", &args).is_err());
        }
        args["effect"] = json!("breathing");
        args["request"] = json!({"mode":"off"});
        assert!(validate_args("rgb_apply", &args).is_err());
        let mut status = initial_status();
        status["softwareEffects"] = json!(["breathing", "rainbow", "cycle"]);
        assert!(View::checked(status.clone()).is_ok());
        // 1.7.0: «hold» og varig utgangspunkt.
        let mut newer = status.clone();
        newer["softwareEffects"] = json!(["breathing", "rainbow", "cycle", "hold"]);
        newer["persist"] = json!(true);
        newer["baselineRequest"] = json!({"mode":"static","color":[0,0,0],"brightness":0});
        newer["factory"] = json!({"mode":"static","color":[69,200,0],"brightness":100});
        assert!(View::checked(newer.clone()).is_ok());
        newer["factory"] = json!({"mode":"blink"});
        assert!(View::checked(newer).is_err());
        status["effect"] = json!("breathing");
        assert!(
            View::checked(status.clone()).is_err(),
            "Ingen effekt uten eier"
        );
        status["effect"] = Value::Null;
        status["softwareEffects"] = json!(["native"]);
        assert!(View::checked(status).is_err());
    }

    #[test]
    fn falsk_rot_kan_ikke_peke_til_vert_eller_vaere_delt() {
        let root = Temp::new();
        let alias = root.0.join("host");
        std::os::unix::fs::symlink("/", &alias).unwrap();
        assert!(socket_metadata(&Paths::new(alias)).is_err());
        std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(socket_metadata(&root.paths()).is_err());
    }

    #[test]
    fn koen_er_begrenset_og_ukjente_argumenter_gir_ingen_rpc() {
        let root = Temp::new();
        let fake = Fake::start(&root.paths(), true);
        let (mut client, _rx) = RgbClient::start(Some(root.paths()));
        let mut invalid = request(1, "rgb_apply");
        invalid.args["path"] = json!("vilkaarlig");
        assert_eq!(
            client.enqueue(7, &invalid).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        let mut ack = request(2, "rgb_ack_recovery");
        assert!(validate_args(&ack.op, &ack.args).is_ok());
        ack.args.as_object_mut().unwrap().remove("expected");
        assert_eq!(
            client.enqueue(7, &ack).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        for id in 1..=MAX_PENDING {
            assert!(
                client
                    .enqueue(7, &request(id as u64, "get_rgb_helper"))
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(
            client
                .enqueue(7, &request(99, "get_rgb_helper"))
                .unwrap_err()
                .code,
            ErrorCode::Busy
        );
        assert!(!fake.has("rgb_apply"));
        client.stop();
    }

    #[test]
    fn behold_holder_egen_lease_og_klienttap_lukker_bare_rgb() {
        let root = Temp::new();
        let fake = Fake::start(&root.paths(), false);
        let (mut client, rx) = RgbClient::start(Some(root.paths()));
        client.enqueue(7, &request(1, "get_rgb_helper")).unwrap();
        let view = complete(&mut client, &rx).unwrap();
        assert_eq!(view["state"], "on", "RGB-fixturestatus: {view}");
        client.enqueue(7, &request(2, "rgb_apply")).unwrap();
        complete(&mut client, &rx).unwrap();
        assert_eq!(client.view(7)["owned"], true);
        assert_eq!(client.view(8)["busy"], true);
        assert_eq!(
            client
                .enqueue(8, &request(3, "rgb_confirm"))
                .unwrap_err()
                .code,
            ErrorCode::NotOwner
        );
        client.enqueue(7, &request(4, "rgb_confirm")).unwrap();
        complete(&mut client, &rx).unwrap();
        assert!(client.holds_values());
        let heartbeat_deadline = Instant::now() + Duration::from_secs(4);
        let mut id = 5;
        while !fake.has("rgb_heartbeat") && Instant::now() < heartbeat_deadline {
            let mut wrong = request(id, "rgb_apply");
            wrong.args["uuid"] = json!("GPU-other-private-fixture");
            client.enqueue(7, &wrong).unwrap();
            assert_eq!(
                complete(&mut client, &rx).unwrap_err().code,
                ErrorCode::UnknownGpu
            );
            id += 1;
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            fake.has("rgb_heartbeat"),
            "feilende kall må ikke utsette heartbeat"
        );
        client.session_closed(7);
        assert!(!client.holds_values());
        wait(|| fake.has("closed"));
        assert!(!fake.has("reset") && !fake.has("rgb_reset"));
        client.stop();
    }

    #[test]
    fn treg_status_blokkerer_ikke_cache_og_doed_oekt_kan_ikke_apply() {
        let root = Temp::new();
        let fake = Fake::start(&root.paths(), true);
        let (mut client, rx) = RgbClient::start(Some(root.paths()));
        let started = Instant::now();
        client.enqueue(7, &request(1, "rgb_apply")).unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        wait(|| fake.has("rgb_status"));
        let cached = Request {
            id: json!(2),
            op: "get_rgb_helper".into(),
            args: json!({}),
        };
        assert!(client.enqueue(7, &cached).unwrap().is_some());
        client.session_closed(7);
        fake.gate.store(true, Ordering::SeqCst);
        wait(|| fake.has("closed"));
        while let Ok(event) = rx.try_recv() {
            assert!(client.complete(event).is_none());
        }
        assert!(!fake.has("rgb_apply"));
        assert!(!client.holds_values());
        client.stop();
    }

    #[test]
    fn sene_svar_fra_doed_sid_gjenoppretter_ikke_eierskap() {
        let root = Temp::new();
        let (mut client, _rx) = RgbClient::start(Some(root.paths()));
        client.enqueue(7, &request(1, "rgb_apply")).unwrap();
        let seq = *client.pending.keys().next().unwrap();
        client.session_closed(7);
        let event = Completion {
            seq: Some(seq),
            sid: 7,
            view: View::checked(initial_status()).unwrap(),
            result: Ok(json!({})),
            deadline: Some(Deadline {
                poll: Instant::now() + RPC_TIMEOUT,
                queued_boot_ms: boottime_ms().unwrap(),
            }),
        };
        assert!(client.complete(event).is_none());
        assert_eq!(client.view(7)["owned"], false);
        assert_ne!(client.view(7)["state"], "on");
        client.stop();
    }

    #[test]
    fn utloept_koejobb_sender_ingen_status_eller_endring() {
        let root = Temp::new();
        let fake = Fake::start(&root.paths(), false);
        let (mut client, rx) = RgbClient::start(Some(root.paths()));
        let alive = Arc::new(AtomicBool::new(true));
        client.control.owner.store(7, Ordering::SeqCst);
        client.pending.insert(
            1,
            Pending {
                sid: 7,
                id: json!(1),
                alive: alive.clone(),
                mutation: true,
                refresh: false,
            },
        );
        assert!(
            client
                .commands
                .try_send(Command {
                    seq: 1,
                    sid: 7,
                    alive,
                    op: "rgb_apply".into(),
                    args: request(1, "rgb_apply").args,
                    queued: Instant::now() - TOTAL_TIMEOUT - Duration::from_millis(1),
                    queued_boot_ms: boottime_ms().unwrap(),
                })
                .is_ok()
        );
        assert_eq!(
            complete(&mut client, &rx).unwrap_err().code,
            ErrorCode::Unavailable
        );
        assert!(fake.log.lock().unwrap().is_empty());
        assert!(!client.holds_values());
        client.stop();
    }

    #[test]
    fn suspendgap_og_klokkefeil_sperrer_sen_suksess_uten_venting() {
        let deadline = Deadline {
            poll: Instant::now() + RPC_TIMEOUT,
            queued_boot_ms: 1000,
        };
        assert!(deadline.check_at(Ok(1001)).is_ok());
        // Instant er fortsatt innenfor fristen, men BOOTTIME inkluderer
        // det simulerte minuttet i suspend. Samme gate brukes før hver send.
        assert!(deadline.check_at(Ok(61_000)).is_err());
        assert!(deadline.check_at(Ok(999)).is_err());
        assert!(
            deadline
                .check_at(Err("injisert klokkefeil".into()))
                .is_err()
        );
        for clock in [Ok(61_000), Ok(999), Err("injisert klokkefeil".into())] {
            let (commands, _requests) = mpsc::sync_channel(MAX_PENDING);
            let control = Arc::new(Control::default());
            control.owner.store(7, Ordering::SeqCst);
            let mut client = RgbClient {
                commands,
                control,
                pending: BTreeMap::new(),
                next_seq: 2,
                view: View::off(),
                thread: None,
                system_admin: false,
            };
            client.pending.insert(
                1,
                Pending {
                    sid: 7,
                    id: json!(1),
                    alive: Arc::new(AtomicBool::new(true)),
                    mutation: true,
                    refresh: false,
                },
            );
            let mut status = initial_status();
            status["state"]["owner"] = json!(7);
            status["holding"] = json!({"owner":7,"deadlineMs":16_000,"leaseMs":15_000});
            let result = client
                .complete_at(
                    Completion {
                        seq: Some(1),
                        sid: 7,
                        view: View::checked(status).unwrap(),
                        result: Ok(json!({"outcome":{"status":"pending"}})),
                        deadline: Some(deadline),
                    },
                    clock,
                )
                .unwrap()
                .2;
            assert_eq!(result.unwrap_err().code, ErrorCode::Unavailable);
            assert!(!client.holds_values());
            assert_eq!(client.view(7)["state"], "error");
            assert!(client.view(7)["status"].is_null());
        }
    }

    #[test]
    fn ventende_effekt_etter_av_godtas_mens_aktiv_effekt_krever_statisk_bekreftet() {
        let mut status = initial_status();
        status["softwareEffects"] = json!(["breathing", "rainbow", "cycle"]);
        status["effect"] = json!("breathing");
        status["holding"] = json!({"owner": 7, "deadlineMs": 16_000, "leaseMs": 15_000});
        status["state"]["owner"] = json!(7);
        status["state"]["dirty"] = json!(true);
        status["state"]["mayHaveWritten"] = json!(true);
        status["state"]["confirmed"] = json!({"mode": "off"});
        status["state"]["pending"] = json!({
            "txn": 2,
            "owner": 7,
            "deadlineMs": 15_000,
            "requested": {"mode": "static", "color": [10, 20, 30], "brightness": 80}
        });
        assert!(View::checked(status.clone()).is_ok());
        // Når prøven ikke lenger venter, må bekreftet valg være statisk.
        status["state"]["pending"] = Value::Null;
        assert!(View::checked(status).is_err());
    }
}
