//! Egen RGB-prosess og socket. Denne løkka deler ikke NVML-motor eller lease.
//! Vanlig helperstart, profilvalg og status åpner aldri I2C.

use crate::rgb_backend::NativeBackend;
use crate::rgb_runtime::Runtime;
use crate::{rgb_store, server};
use rigg_core::hjelper::{MAX_CLIENTS, MAX_LINE};
use rigg_core::proto::{self, ErrorCode, LineBuffer, LineEvent, OpResult, ProtoError};
use rigg_core::rgb_auth::{self, Authorization};
use rigg_core::rgb_txn::Backend;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const SOCKET: &str = "/run/agentic-rigg-rgb/helper.sock";
const MAX_FRAMES_PER_TURN: usize = 8;
const MAX_BYTES_PER_TURN: usize = 16_384;

fn mode(args: &[String]) -> Option<Result<(), String>> {
    args.iter().any(|s| s == "--rgb-server").then(|| {
        if args == ["--rgb-server"] {
            Ok(())
        } else {
            Err("--rgb-server kan ikke kombineres med andre argumenter.".into())
        }
    })
}

fn dispatch_with(
    args: &[String],
    euid: u32,
    run: impl FnOnce() -> Result<(), String>,
) -> Option<(Value, i32)> {
    let result = mode(args)?;
    let result = result.and_then(|()| {
        if euid != 0 {
            Err("RGB-tjenesten krever en separat administratoraktivering.".into())
        } else {
            run()
        }
    });
    Some(match result {
        Ok(()) => (json!({"ok": true, "operation": "rgb_server"}), 0),
        Err(message) => (
            json!({"ok": false, "operation": "rgb_server", "error": message}),
            if euid == 0 { 2 } else { 1 },
        ),
    })
}

pub fn dispatch() -> Option<i32> {
    let raw: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if !raw.iter().any(|arg| arg == "--rgb-server") {
        return None;
    }
    let args: Vec<String> = match raw.into_iter().map(|arg| arg.into_string()).collect() {
        Ok(args) => args,
        Err(_) => {
            eprintln!(
                "{}",
                json!({"ok": false, "operation": "rgb_server",
                "error": "RGB-argumentene må være gyldig UTF-8."})
            );
            return Some(2);
        }
    };
    // SAFETY: geteuid leser bare prosessens eksisterende rettigheter.
    let (result, code) = dispatch_with(&args, unsafe { libc::geteuid() }, run_production)?;
    if code != 0 {
        eprintln!("{result}");
    }
    Some(code)
}

/// Monoton tid som også inkluderer suspend. Ingen reserve til veggklokke.
fn now_ms() -> Result<u64, String> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: time er en gyldig timespec; clock_gettime endrer ingen HW.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return Err(format!(
            "RGB-klokken er utilgjengelig: {}",
            std::io::Error::last_os_error()
        ));
    }
    let seconds = u64::try_from(time.tv_sec).map_err(|_| "Ugyldig RGB-klokke.")?;
    let nanos = u64::try_from(time.tv_nsec).map_err(|_| "Ugyldig RGB-klokke.")?;
    if nanos >= 1_000_000_000 {
        return Err("Ugyldig RGB-klokke.".into());
    }
    seconds
        .checked_mul(1000)
        .and_then(|v| v.checked_add(nanos / 1_000_000))
        .ok_or_else(|| "RGB-klokken er for stor.".into())
}

fn trusted_directory(path: &Path, owner: u32) -> Result<(), String> {
    let m = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_dir() || m.file_type().is_symlink() || m.uid() != owner || m.mode() & 0o022 != 0 {
        return Err(format!("{} er ikke en trygg RGB-mappe.", path.display()));
    }
    Ok(())
}

fn prepare_socket_directory(path: &Path, owner: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("RGB-mappen mangler forelder.")?;
    trusted_directory(parent, owner)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o755)
                .create(path)
                .map_err(|e| e.to_string())?;
        }
        Err(e) => return Err(e.to_string()),
    }
    trusted_directory(path, owner)?;
    let m = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if m.mode() & 0o001 == 0 {
        return Err("RGB-mappen gir ikke klienten tilgang til socketen.".into());
    }
    Ok(())
}

fn validate_existing_socket(path: &Path, owner: u32, client: u32) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
        Ok(m)
            if m.file_type().is_socket()
                && (m.uid() == owner || m.uid() == client)
                && m.mode() & 0o777 == 0o600
                && m.nlink() == 1 =>
        {
            Ok(())
        }
        Ok(_) => Err("RGB-socketen finnes med utrygg type, eier eller modus.".into()),
    }
}

fn run_production() -> Result<(), String> {
    // Åpner bare tillatelse, lås og grunnopptak. En gammel journal gir feil
    // før socket/HW, og tolkes aldri som tillatelse til automatisk restore.
    let (auth, baseline, backend) = NativeBackend::open_production()?;
    let mut runtime = Runtime::new(auth.clone(), baseline, backend)?;
    let path = PathBuf::from(SOCKET);
    prepare_socket_directory(path.parent().ok_or("RGB-socketen mangler mappe.")?, 0)?;
    validate_existing_socket(&path, 0, auth.uid())?;
    let listener = server::bind(&path, Some(auth.uid()))?;
    let mut service = Service::new(listener, path)?;
    server::install_signals();
    service.run(&mut runtime, &server::STOP, &auth, || {
        rgb_store::load_authorization(Path::new(rgb_auth::CONFIG_PATH))
    })
}

struct Conn {
    uid: u32,
    stream: UnixStream,
    buffer: LineBuffer,
}

struct Service {
    listener: UnixListener,
    socket: PathBuf,
    socket_identity: (u64, u64),
    clients: BTreeMap<u64, Conn>,
    next_id: u64,
    peer: fn(&UnixStream) -> Option<(u32, i32)>,
}

impl Service {
    fn new(listener: UnixListener, socket: PathBuf) -> Result<Self, String> {
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let metadata = std::fs::symlink_metadata(&socket).map_err(|e| e.to_string())?;
        Ok(Self {
            listener,
            socket,
            socket_identity: (metadata.dev(), metadata.ino()),
            clients: BTreeMap::new(),
            next_id: 1,
            peer: server::peer_uid,
        })
    }

    fn check_auth<B: Backend>(
        runtime: &mut Runtime<B>,
        auth: &Authorization,
        load: &mut impl FnMut() -> Result<Authorization, String>,
    ) -> Result<(), String> {
        match load() {
            Ok(fresh) if fresh == *auth => Ok(()),
            _ => {
                runtime.revoke();
                Err("RGB-tillatelsen er fjernet, endret eller utrygg. Tjenesten stopper.".into())
            }
        }
    }

    fn run<B: Backend>(
        &mut self,
        runtime: &mut Runtime<B>,
        stop: &AtomicBool,
        auth: &Authorization,
        mut load: impl FnMut() -> Result<Authorization, String>,
    ) -> Result<(), String> {
        let result = self.run_loop(runtime, stop, auth, &mut load);
        // Livssyklus, aldri eksplisitt reset. Avvist forarbeid skal ikke
        // bli det første skrivekallet ved stopp eller tilbakekalt tillatelse.
        if let Some(outcome) = runtime.release() {
            crate::journal::notice(&format!(
                "RGB avsluttes med gjenopprettingstilstand {:?} ({:?}).",
                outcome.status, outcome.readback
            ));
        }
        self.clients.clear();
        result
    }

    fn run_loop<B: Backend>(
        &mut self,
        runtime: &mut Runtime<B>,
        stop: &AtomicBool,
        auth: &Authorization,
        load: &mut impl FnMut() -> Result<Authorization, String>,
    ) -> Result<(), String> {
        while !stop.load(Ordering::SeqCst) {
            Self::check_auth(runtime, auth, load)?;
            runtime
                .tick(now_ms()?)
                .map_err(|e| format!("RGB-frist: {e:?}"))?;
            let ids: Vec<u64> = self.clients.keys().copied().collect();
            let mut fds = vec![libc::pollfd {
                fd: self.listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            fds.extend(ids.iter().map(|id| libc::pollfd {
                fd: self.clients[id].stream.as_raw_fd(),
                events: libc::POLLIN | libc::POLLRDHUP,
                revents: 0,
            }));
            // SAFETY: fds inneholder gyldige descriptorer for denne runden.
            let poll_ms = if runtime.animation_active() {
                250
            } else {
                1000
            };
            let result =
                unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, poll_ms) };
            if result < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!(
                    "RGB-socketfeil: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Self::check_auth(runtime, auth, load)?;
            runtime
                .tick(now_ms()?)
                .map_err(|e| format!("RGB-frist: {e:?}"))?;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            if fds[0].revents & libc::POLLIN != 0 {
                self.accept(runtime);
            }
            for (index, id) in ids.iter().enumerate() {
                let events = fds[index + 1].revents;
                if events & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL | libc::POLLRDHUP) != 0
                {
                    self.clients.remove(id);
                    runtime.disconnect(*id);
                    continue;
                }
                if events != 0 {
                    match self.read(*id, runtime, auth, load, stop) {
                        Ok(true) => {}
                        Ok(false) => {
                            self.clients.remove(id);
                            runtime.disconnect(*id);
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            // Frister og klienttrafikk går alltid foran en programvareramme.
            // Sjekk hangup igjen: annen RPC/recovery kan ha tatt tid siden poll.
            if !runtime.animation_active() {
                continue;
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let incoming = self.disconnect_closed(runtime)?;
            Self::check_auth(runtime, auth, load)?;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            if incoming {
                continue;
            }
            if runtime
                .animate(now_ms()?)
                .map_err(|e| format!("RGB-effekt: {e:?}"))?
            {
                runtime
                    .frame_completed(now_ms()?)
                    .map_err(|e| format!("RGB-effektfrist: {e:?}"))?;
            }
        }
        Ok(())
    }

    fn disconnect_closed<B: Backend>(&mut self, runtime: &mut Runtime<B>) -> Result<bool, String> {
        let ids: Vec<u64> = self.clients.keys().copied().collect();
        let mut fds: Vec<libc::pollfd> = ids
            .iter()
            .map(|id| libc::pollfd {
                fd: self.clients[id].stream.as_raw_fd(),
                events: libc::POLLIN | libc::POLLRDHUP,
                revents: 0,
            })
            .collect();
        // SAFETY: gyldige descriptorer; null ventetid og ingen lesing/skriving.
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 0) };
        if result < 0 {
            return Err(format!(
                "Kan ikke kontrollere RGB-klienttap: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut incoming = false;
        for (id, fd) in ids.iter().zip(fds) {
            if fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL | libc::POLLRDHUP) != 0
            {
                self.clients.remove(id);
                runtime.disconnect(*id);
            } else if fd.revents & libc::POLLIN != 0 {
                incoming = true;
            }
        }
        Ok(incoming)
    }

    fn reply(stream: &mut UnixStream, id: &Value, result: &OpResult) -> bool {
        stream.write_all(&proto::response_line(id, result)).is_ok()
    }

    fn accept<B: Backend>(&mut self, runtime: &Runtime<B>) {
        // Høyst fire accept-forsøk per runde, også ved en vedvarende kø.
        for _ in 0..MAX_CLIENTS {
            let Ok((mut stream, _)) = self.listener.accept() else {
                break;
            };
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            let peer = (self.peer)(&stream);
            if peer.is_none_or(|(uid, _)| uid != runtime.authorized_uid()) {
                Self::reply(
                    &mut stream,
                    &Value::Null,
                    &Err(ProtoError::new(ErrorCode::PermissionDenied)),
                );
                continue;
            }
            if self.clients.len() >= MAX_CLIENTS || self.next_id == u64::MAX {
                Self::reply(
                    &mut stream,
                    &Value::Null,
                    &Err(ProtoError::new(ErrorCode::TooManyClients)),
                );
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.clients.insert(
                id,
                Conn {
                    uid: peer.expect("kontrollert peer").0,
                    stream,
                    buffer: LineBuffer::new(),
                },
            );
        }
    }

    fn read<B: Backend>(
        &mut self,
        id: u64,
        runtime: &mut Runtime<B>,
        auth: &Authorization,
        load: &mut impl FnMut() -> Result<Authorization, String>,
        stop: &AtomicBool,
    ) -> Result<bool, String> {
        let mut lines = Vec::new();
        let mut closed = false;
        {
            let Some(client) = self.clients.get_mut(&id) else {
                return Ok(false);
            };
            let mut chunk = [0u8; 4096];
            let mut total = 0;
            while total < MAX_BYTES_PER_TURN {
                match client.stream.read(&mut chunk) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(count) => {
                        total += count;
                        for event in client.buffer.push(&chunk[..count], MAX_LINE) {
                            match event {
                                LineEvent::Line(line) => lines.push(line),
                                LineEvent::TooLarge => {
                                    Self::reply(
                                        &mut client.stream,
                                        &Value::Null,
                                        &Err(ProtoError::new(ErrorCode::MessageTooLarge)),
                                    );
                                    return Ok(false);
                                }
                            }
                        }
                        if lines.len() > MAX_FRAMES_PER_TURN {
                            Self::reply(
                                &mut client.stream,
                                &Value::Null,
                                &Err(ProtoError::msg(
                                    ErrorCode::Busy,
                                    "For mange RGB-meldinger samtidig.",
                                )),
                            );
                            return Ok(false);
                        }
                        if lines.len() == MAX_FRAMES_PER_TURN {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                }
            }
        }
        // En klient som allerede har lukket, skal ikke få etablere en ny
        // skrivehensikt fra data som lå igjen i mottakskøen.
        if closed {
            return Ok(false);
        }
        for line in lines {
            if stop.load(Ordering::SeqCst) {
                return Ok(false);
            }
            Self::check_auth(runtime, auth, load)?;
            let now = now_ms()?;
            runtime.tick(now).map_err(|e| format!("RGB-frist: {e:?}"))?;
            let (request_id, result) = match proto::parse_request(&line) {
                Ok(request) => {
                    let uid = self.clients[&id].uid;
                    (
                        request.id,
                        runtime.handle(uid, id, &request.op, &request.args, now),
                    )
                }
                Err((id, error)) => (id, Err(error)),
            };
            let Some(client) = self.clients.get_mut(&id) else {
                return Ok(false);
            };
            if !Self::reply(&mut client.stream, &request_id, &result) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        // Ikke fjern en annen prosess sin erstattede socket.
        if let Ok(metadata) = std::fs::symlink_metadata(&self.socket)
            && metadata.file_type().is_socket()
            && (metadata.dev(), metadata.ino()) == self.socket_identity
        {
            let _ = std::fs::remove_file(&self.socket);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::rgb::{
        AdapterIdentity, Baseline, Binding, Observed, PciIdentity, Request, Signature,
    };
    use rigg_core::rgb_txn::{Journal, WriteError};
    use std::io::BufRead;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = rigg_core::paths::short_tmp().join(format!(
                "rigg-rgb-socket-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn binding() -> Binding {
        Binding {
            pci: "0000:1c:00.0".into(),
            uuid: "GPU-private-rgb-service-fixture".into(),
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

    struct Fake {
        observed: Observed,
        events: Arc<Mutex<Vec<&'static str>>>,
        stop_after_apply: Option<Arc<AtomicBool>>,
    }
    impl Backend for Fake {
        fn read(&mut self, _: &Binding) -> Result<Observed, String> {
            Ok(self.observed.clone())
        }
        fn mark_dirty(&mut self, journal: &Journal) -> Result<(), String> {
            journal.validate(&binding())?;
            self.events.lock().unwrap().push("journal");
            Ok(())
        }
        fn apply(
            &mut self,
            _: &Binding,
            request: &Request,
            _: &Observed,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.events.lock().unwrap().push("apply");
            self.observed = observation(request);
            if let Some(stop) = &self.stop_after_apply {
                stop.store(true, Ordering::SeqCst);
            }
            Ok(self.observed.clone())
        }
        fn recover(
            &mut self,
            _: &Binding,
            request: &Request,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.events.lock().unwrap().push("recover");
            self.observed = observation(request);
            Ok(self.observed.clone())
        }
        fn animation_frame(
            &mut self,
            _: &Binding,
            request: &Request,
            _: &Observed,
            _: &Baseline,
        ) -> Result<Observed, WriteError> {
            self.events.lock().unwrap().push("frame");
            self.observed = observation(request);
            Ok(self.observed.clone())
        }
        fn clear_dirty(&mut self, _: &Binding) -> Result<(), String> {
            panic!("Ukjent modus skal ikke rydde journalen som verifisert")
        }
    }
    fn observation(request: &Request) -> Observed {
        let (color, brightness) = match request {
            Request::Static { color, brightness } => (*color, brightness.get()),
            Request::Off {} => ([0, 0, 0], 100),
        };
        Observed {
            color: Some(color),
            brightness_raw: Some(brightness),
            mode: None,
        }
    }
    fn setup() -> (Authorization, Runtime<Fake>, Arc<Mutex<Vec<&'static str>>>) {
        setup_with_stop(None)
    }
    fn setup_with_stop(
        stop_after_apply: Option<Arc<AtomicBool>>,
    ) -> (Authorization, Runtime<Fake>, Arc<Mutex<Vec<&'static str>>>) {
        let auth = Authorization::new(1000, binding(), rigg_core::version::build()).unwrap();
        let observed = Observed {
            color: Some([1, 2, 3]),
            brightness_raw: Some(100),
            mode: None,
        };
        let baseline = Baseline::user_confirmed_static(binding(), observed.clone()).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let runtime = Runtime::new(
            auth.clone(),
            baseline,
            Fake {
                observed,
                events: events.clone(),
                stop_after_apply,
            },
        )
        .unwrap();
        (auth, runtime, events)
    }
    fn rpc(stream: &mut UnixStream, id: u64, op: &str, args: Value) -> Value {
        let mut line = serde_json::to_vec(&json!({"id": id, "op": op, "args": args})).unwrap();
        line.push(b'\n');
        stream.write_all(&line).unwrap();
        let mut response = String::new();
        std::io::BufReader::new(stream)
            .read_line(&mut response)
            .unwrap();
        serde_json::from_str(&response).unwrap()
    }

    #[test]
    fn cli_gater_root_og_blandede_argumenter_for_noe_startes() {
        assert!(dispatch_with(&["--version".into()], 0, || panic!()).is_none());
        for args in [
            vec!["--rgb-server".into()],
            vec![
                "--rgb-server".into(),
                "--socket".into(),
                "/run/agentic-rigg/helper.sock".into(),
            ],
            vec!["--backend".into(), "sim".into(), "--rgb-server".into()],
        ] {
            let (_, code) = dispatch_with(&args, 1000, || {
                panic!("Ingen start før root/argumentkontroll")
            })
            .unwrap();
            assert_ne!(code, 0);
        }
        let (_, code) = dispatch_with(
            &["--rgb-server".into(), "--root".into(), "/".into()],
            0,
            || panic!(),
        )
        .unwrap();
        assert_ne!(code, 0);
    }

    #[test]
    fn socketmappe_og_gammel_node_ma_vaere_trygg() {
        let fixture = Fixture::new();
        // SAFETY: rent lesende UID.
        let owner = unsafe { libc::geteuid() };
        let directory = fixture.0.join("runtime");
        prepare_socket_directory(&directory, owner).unwrap();
        let socket = directory.join("helper.sock");
        validate_existing_socket(&socket, owner, 1000).unwrap();
        std::fs::write(&socket, b"ikke en socket").unwrap();
        assert!(validate_existing_socket(&socket, owner, 1000).is_err());
        std::fs::remove_file(&socket).unwrap();
        symlink(fixture.0.join("utenfor"), &socket).unwrap();
        assert!(validate_existing_socket(&socket, owner, 1000).is_err());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(prepare_socket_directory(&directory, owner).is_err());
    }

    #[test]
    fn privat_socket_status_og_klienttap_bruker_bare_fake() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("rgb.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (auth, mut runtime, events) = setup();
        let mut service = Service::new(listener, socket.clone()).unwrap();
        // Bare denne private unit-fixturen injiserer UID, også når Cargo
        // kjøres som root. Produksjon bruker alltid faktisk SO_PEERCRED.
        service.peer = |_| Some((1000, 123));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            service
                .run(&mut runtime, &thread_stop, &auth, || Ok(auth.clone()))
                .unwrap();
            runtime.status()
        });
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let status = rpc(&mut stream, 1, "rgb_status", json!({}));
        assert_eq!(status["ok"], true, "{status}");
        assert!(events.lock().unwrap().is_empty());
        let response = rpc(
            &mut stream,
            2,
            "rgb_apply",
            json!({
                "gpu": binding().pci, "uuid": binding().uuid,
                "request": {"mode": "off"}, "confirmTimeoutS": 10
            }),
        );
        assert_eq!(response["ok"], true, "{response}");
        drop(stream);
        // Avslutt også løkka: shutdown skal bruke samme livssyklus, ikke
        // legge til et ekstra eksplisitt reset-forsøk.
        stop.store(true, Ordering::SeqCst);
        let status = worker.join().unwrap();
        assert_eq!(*events.lock().unwrap(), vec!["journal", "apply", "recover"]);
        assert_eq!(status["state"]["observed"]["mode"], Value::Null);
        assert_eq!(status["state"]["dirty"], true);
        assert!(!socket.exists());
    }

    #[test]
    fn faktisk_peer_leses_fra_den_private_socketen() {
        let (client, server) = UnixStream::pair().unwrap();
        // SAFETY: rent lesende UID.
        assert_eq!(crate::server::peer_uid(&client).unwrap().0, unsafe {
            libc::geteuid()
        });
        drop(server);
    }

    #[test]
    fn tilbakekalt_tillatelse_stopper_uten_i2c_eller_ny_rpc() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("rgb.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (auth, mut runtime, events) = setup();
        let mut service = Service::new(listener, socket).unwrap();
        let result = service.run(&mut runtime, &AtomicBool::new(false), &auth, || {
            Err("fjernet".into())
        });
        assert!(result.is_err());
        assert!(events.lock().unwrap().is_empty());
        assert_eq!(runtime.status()["authorized"], false);
    }

    #[test]
    fn lukket_klient_med_atte_rammesett_gir_ingen_skrivehensikt() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("rgb.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (auth, mut runtime, events) = setup();
        let mut service = Service::new(listener, socket).unwrap();
        let (mut sender, receiver) = UnixStream::pair().unwrap();
        receiver.set_nonblocking(true).unwrap();
        service.clients.insert(
            1,
            Conn {
                uid: 1000,
                stream: receiver,
                buffer: LineBuffer::new(),
            },
        );
        for id in 0..8 {
            let mut frame = serde_json::to_vec(&json!({
                "id": id, "op": "rgb_apply", "args": {
                    "gpu": binding().pci, "uuid": binding().uuid,
                    "request": {"mode": "off"}, "confirmTimeoutS": 10
                }
            }))
            .unwrap();
            frame.push(b'\n');
            sender.write_all(&frame).unwrap();
        }
        drop(sender);
        let stop = AtomicBool::new(false);
        let mut checks = 0;
        service
            .run(&mut runtime, &stop, &auth, || {
                checks += 1;
                // Første poll-runde må håndtere HUP før denne stopper neste.
                if checks == 3 {
                    stop.store(true, Ordering::SeqCst);
                }
                Ok(auth.clone())
            })
            .unwrap();
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn stopp_under_forste_operasjon_avviser_resten_av_koen() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("rgb.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (auth, mut runtime, events) = setup_with_stop(Some(stop.clone()));
        let mut service = Service::new(listener, socket).unwrap();
        let (mut sender, receiver) = UnixStream::pair().unwrap();
        receiver.set_nonblocking(true).unwrap();
        service.clients.insert(
            1,
            Conn {
                uid: 1000,
                stream: receiver,
                buffer: LineBuffer::new(),
            },
        );
        for (id, op, args) in [
            (
                1,
                "rgb_apply",
                json!({
                    "gpu": binding().pci, "uuid": binding().uuid,
                    "request": {"mode": "off"}, "confirmTimeoutS": 10
                }),
            ),
            (
                2,
                "rgb_reset",
                json!({"gpu": binding().pci, "uuid": binding().uuid}),
            ),
        ] {
            let mut frame = serde_json::to_vec(&json!({"id": id, "op": op, "args": args})).unwrap();
            frame.push(b'\n');
            sender.write_all(&frame).unwrap();
        }
        service
            .run(&mut runtime, &stop, &auth, || Ok(auth.clone()))
            .unwrap();
        assert_eq!(*events.lock().unwrap(), vec!["journal", "apply", "recover"]);
        drop(sender);
    }

    fn active_effect(runtime: &mut Runtime<Fake>) {
        let now = now_ms().unwrap() - 1000;
        let result = runtime
            .handle(
                1000,
                1,
                "rgb_apply",
                &json!({
                    "gpu": binding().pci, "uuid": binding().uuid,
                    "request": {"mode":"static", "color":[120,60,30], "brightness":80},
                    "confirmTimeoutS":10, "effect":"breathing"
                }),
                now,
            )
            .unwrap();
        runtime
            .handle(
                1000,
                1,
                "rgb_confirm",
                &json!({
                    "gpu":binding().pci, "uuid":binding().uuid, "txn":result["outcome"]["txn"]
                }),
                now,
            )
            .unwrap();
        assert!(runtime.animation_active());
    }

    #[test]
    fn lukket_eier_og_koet_reset_behandles_for_forfalt_frame() {
        for close in [true, false] {
            let fixture = Fixture::new();
            let socket = fixture.0.join("rgb.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let (auth, mut runtime, events) = setup();
            active_effect(&mut runtime);
            let mut service = Service::new(listener, socket).unwrap();
            let (mut sender, receiver) = UnixStream::pair().unwrap();
            receiver.set_nonblocking(true).unwrap();
            service.clients.insert(
                1,
                Conn {
                    uid: 1000,
                    stream: receiver,
                    buffer: LineBuffer::new(),
                },
            );
            if close {
                drop(sender);
                assert!(!service.disconnect_closed(&mut runtime).unwrap());
            } else {
                let mut line = serde_json::to_vec(&json!({"id":1,"op":"rgb_reset", "args":{
                    "gpu":binding().pci,"uuid":binding().uuid
                }}))
                .unwrap();
                line.push(b'\n');
                sender.write_all(&line).unwrap();
                assert!(service.disconnect_closed(&mut runtime).unwrap());
                service
                    .read(
                        1,
                        &mut runtime,
                        &auth,
                        &mut || Ok(auth.clone()),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
            }
            assert!(!runtime.animate(now_ms().unwrap()).unwrap());
            assert_eq!(*events.lock().unwrap(), vec!["journal", "apply", "recover"]);
        }
    }

    #[test]
    fn tilbakekalling_stopper_en_forfalt_effekt_for_frame() {
        let (auth, mut runtime, events) = setup();
        active_effect(&mut runtime);
        assert!(Service::check_auth(&mut runtime, &auth, &mut || Err("fjernet".into())).is_err());
        assert!(!runtime.animate(now_ms().unwrap()).unwrap());
        assert_eq!(*events.lock().unwrap(), vec!["journal", "apply", "recover"]);
    }
}
