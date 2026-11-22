//! Socketen og løkka: `poll` på lytteren og høyst fire forbindelser, ett
//! sekunds takt for fristen og nødregelen. Én tråd, ingen GLib.
//!
//! Hver forbindelse sjekkes med `SO_PEERCRED`: bare uid-en i `helper.json`
//! slipper inn. Socketen er `0600` og eies av den uid-en (som root:
//! `chown`, det eneste privilegiet tjenesten har i tillegg).
//!
//! All planlegging i motoren (takt, livssignal, hvileoppdagelse, Behold-frist
//! og viftekurve) bruker `CLOCK_BOOTTIME`: monoton og inkluderer hviletid,
//! slik at et hopp i veggklokka verken fryser tilbakerullingen eller gir en
//! falsk hviletilbakestilling.

use crate::journal;
use crate::motor::Motor;
use rigg_core::hjelper::{self, Config};
use rigg_core::proto::{self, ErrorCode, LineBuffer, LineEvent, ProtoError};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub static STOP: AtomicBool = AtomicBool::new(false);

/// Hvor lenge et svar til en godkjent klient kan vente på plass i
/// socketbufferen før forbindelsen regnes som død.
const REPLY_BUDGET: Duration = Duration::from_millis(500);

/// Monoton tid i millisekunder som også teller hvile (`CLOCK_BOOTTIME`).
/// Ingen reserve til veggklokka: `None` får tjenesten til å stoppe trygt.
pub fn boot_ms() -> Option<i64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: time er en gyldig timespec; clock_gettime endrer ingen HW.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return None;
    }
    if !(0..1_000_000_000).contains(&time.tv_nsec) || time.tv_sec < 0 {
        return None;
    }
    time.tv_sec
        .checked_mul(1000)?
        .checked_add(time.tv_nsec / 1_000_000)
}

/// Tidslinjen motoren får. Råklokka er monoton, men et avvik bakover skal
/// verken gi negative deltaer eller fryse takten til klokka har tatt igjen
/// forrige verdi: tidslinjen står stille i det øyeblikket og går så videre
/// i råklokkas tempo.
#[derive(Debug, Default)]
pub(crate) struct Tidslinje {
    last: Option<i64>,
    offset: i64,
}

impl Tidslinje {
    pub(crate) fn les(&mut self, raw: i64) -> i64 {
        let mut t = raw.saturating_add(self.offset);
        if let Some(last) = self.last
            && t < last
        {
            self.offset = last.saturating_sub(raw);
            t = last;
        }
        self.last = Some(t);
        t
    }

    pub(crate) fn siste(&self) -> Option<i64> {
        self.last
    }
}

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

pub fn install_signals() {
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: handleren setter bare et atomisk flagg.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
    // SAFETY: å ignorere SIGPIPE er alltid trygt; skrivefeil gir EPIPE i stedet.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}

/// Leser `helper.json` og sjekker hvem som eier den: som root må fila og
/// mappa eies av root og ikke kunne skrives av andre. Ellers (testene, den
/// falske roten) må de eies av brukeren selv.
pub fn load_config(path: &Path) -> Result<(Config, i64), String> {
    let euid = unsafe { libc::geteuid() };
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.file_type().is_file() {
        return Err(format!("{} er ikke en vanlig fil", path.display()));
    }
    for (what, m) in [
        ("fila", meta.clone()),
        (
            "mappa",
            std::fs::metadata(path.parent().unwrap_or(Path::new("/")))
                .map_err(|e| e.to_string())?,
        ),
    ] {
        if m.uid() != euid {
            return Err(format!(
                "{} {what} eies av uid {}, ikke {euid}",
                path.display(),
                m.uid()
            ));
        }
        if m.mode() & 0o022 != 0 {
            return Err(format!(
                "{} {what} kan skrives av andre (modus {:o})",
                path.display(),
                m.mode() & 0o777
            ));
        }
    }
    if meta.len() > 4096 {
        return Err("helper.json er for stor".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok((
        Config::parse(&text)?,
        meta.mtime() * 1_000_000_000 + meta.mtime_nsec(),
    ))
}

fn mtime(path: &Path) -> Option<i64> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| m.mtime() * 1_000_000_000 + m.mtime_nsec())
}

/// Binder socketen. En gammel socket fjernes; alt annet på stien er en feil.
pub fn bind(path: &Path, uid: Option<u32>) -> Result<UnixListener, String> {
    if let Some(dir) = path.parent()
        && !dir.exists()
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
    }
    if let Ok(m) = std::fs::symlink_metadata(path) {
        if !m.file_type().is_socket() {
            return Err(format!("{} finnes og er ikke en socket", path.display()));
        }
        if UnixStream::connect(path).is_ok() {
            return Err(format!("Hjelperen kjører allerede ({})", path.display()));
        }
        let _ = std::fs::remove_file(path);
    }
    // Socketen lages med 0600 fra start (umask), ikke bare etterpå.
    let old = unsafe { libc::umask(0o177) };
    let l = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()));
    unsafe { libc::umask(old) };
    let l = l?;
    if let Some(uid) = uid
        && unsafe { libc::geteuid() } == 0
    {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|e| e.to_string())?;
        // SAFETY: lchown på en sti vi nettopp har laget.
        if unsafe { libc::lchown(c.as_ptr(), uid, u32::MAX) } != 0 {
            return Err(format!(
                "chown {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
    }
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    Ok(l)
}

pub(crate) fn peer_uid(s: &UnixStream) -> Option<(u32, i32)> {
    let mut c = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt med riktig størrelse på ucred.
    let r = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut c as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    (r == 0).then_some((c.uid, c.pid))
}

struct Conn {
    uid: u32,
    stream: UnixStream,
    buf: LineBuffer,
}

/// `(dev, inode)` for socketen på stien, hvis stien er en socket.
fn socket_identity(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| m.file_type().is_socket())
        .map(|m| (m.dev(), m.ino()))
}

/// Fjerner socketen bare når stien fortsatt er den vi bandt, aldri en
/// erstatning en annen prosess har lagt der.
pub(crate) fn remove_own_socket(path: &Path, identity: Option<(u64, u64)>) -> bool {
    identity.is_some()
        && socket_identity(path) == identity
        && std::fs::remove_file(path).is_ok()
}

/// Skriver hele svaret til en ikke-blokkerende socket. Ved `WouldBlock`
/// ventes det på `POLLOUT`, men aldri lenger enn `budget` til sammen.
fn write_bounded(stream: &mut UnixStream, mut data: &[u8], budget: Duration) -> bool {
    let start = Instant::now();
    while !data.is_empty() {
        match stream.write(data) {
            Ok(0) => return false,
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let left = budget.saturating_sub(start.elapsed());
                if left.is_zero() {
                    return false;
                }
                let mut fd = libc::pollfd {
                    fd: stream.as_raw_fd(),
                    events: libc::POLLOUT,
                    revents: 0,
                };
                let ms = left.as_millis().clamp(1, i32::MAX as u128) as i32;
                // SAFETY: én gyldig pollfd for denne forbindelsen.
                let r = unsafe { libc::poll(&mut fd, 1, ms) };
                if r < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    return false;
                }
                if r > 0 && fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

pub struct Server {
    pub listener: UnixListener,
    pub socket: PathBuf,
    pub config: PathBuf,
    cfg_mtime: Option<i64>,
    conns: BTreeMap<u64, Conn>,
    seq: u64,
    /// Socketen vi bandt (`dev`, `inode`), så stopp ikke fjerner en annens.
    socket_id: Option<(u64, u64)>,
    /// Råklokka (`CLOCK_BOOTTIME`); testene kan bytte den ut.
    clock: fn() -> Option<i64>,
    line: Tidslinje,
    last_tick: Option<i64>,
}

impl Server {
    pub fn new(
        listener: UnixListener,
        socket: PathBuf,
        config: PathBuf,
        cfg_mtime: Option<i64>,
    ) -> Server {
        Server {
            listener,
            socket_id: socket_identity(&socket),
            socket,
            config,
            cfg_mtime,
            conns: BTreeMap::new(),
            seq: 0,
            clock: boot_ms,
            line: Tidslinje::default(),
            last_tick: None,
        }
    }

    /// Tiden motoren får: monoton, også over hvile og veggklokkehopp.
    fn now(&mut self) -> Option<i64> {
        (self.clock)().map(|raw| self.line.les(raw))
    }

    /// `helper.json` på nytt når den er endret (trinn godkjent eller trukket).
    fn reload(&mut self, m: &mut Motor, now: i64) {
        let cur = mtime(&self.config);
        if cur == self.cfg_mtime {
            return;
        }
        self.cfg_mtime = cur;
        m.set_config(load_config(&self.config).map(|(c, _)| c), now);
    }

    fn reply(
        stream: &mut UnixStream,
        id: &serde_json::Value,
        r: &proto::OpResult,
        budget: Duration,
    ) -> bool {
        write_bounded(stream, &proto::response_line(id, r), budget)
    }

    pub fn run(&mut self, m: &mut Motor) {
        while !STOP.load(Ordering::SeqCst) {
            if !self.turn(m) {
                break;
            }
        }
        journal::notice("Stopper.");
        let now = self.now().or(self.line.siste()).unwrap_or(0);
        m.shutdown(now);
        self.conns.clear();
        if !remove_own_socket(&self.socket, self.socket_id) && self.socket.exists() {
            journal::warn(&format!(
                "{} er erstattet av en annen socket og fjernes ikke.",
                self.socket.display()
            ));
        }
    }

    /// Én runde: vent på trafikk (høyst til neste takt), svar, og kjør
    /// takten når det har gått ett sekund på den monotone tidslinjen.
    /// `false`: klokka kan ikke leses, og tjenesten stopper trygt.
    fn turn(&mut self, m: &mut Motor) -> bool {
        let Some(start) = self.now() else {
            journal::err("Den monotone klokka (CLOCK_BOOTTIME) kan ikke leses. Stopper.");
            return false;
        };
        let wait = self
            .last_tick
            .map_or(0, |l| (1000 - start.saturating_sub(l)).clamp(0, 1000)) as i32;
        let mut fds = vec![libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        let ids: Vec<u64> = self.conns.keys().copied().collect();
        for id in &ids {
            fds.push(libc::pollfd {
                fd: self.conns[id].stream.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        // SAFETY: fds er en gyldig liste med pollfd.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait) };
        let Some(now) = self.now() else {
            journal::err("Den monotone klokka (CLOCK_BOOTTIME) kan ikke leses. Stopper.");
            return false;
        };
        if n > 0 {
            if fds[0].revents & libc::POLLIN != 0 {
                self.accept(m, now);
            }
            for (i, id) in ids.iter().enumerate() {
                let ev = fds[i + 1].revents;
                if ev != 0 && !self.read(*id, m, now) {
                    self.conns.remove(id);
                    m.disconnect(*id, now);
                }
            }
        }
        if self.last_tick.is_none_or(|l| now.saturating_sub(l) >= 1000) {
            self.last_tick = Some(now);
            self.reload(m, now);
            m.tick(now);
        }
        true
    }

    fn accept(&mut self, m: &mut Motor, now: i64) {
        // Høyst MAX_CLIENTS forsøk per runde: en vedvarende kø skal ikke
        // holde takten (frist og nødregel) igjen. Resten tas neste runde.
        for _ in 0..hjelper::MAX_CLIENTS {
            let Ok((mut s, _)) = self.listener.accept() else {
                break;
            };
            // Ikke-blokkerende før første svar: en klient som aldri leser,
            // skal ikke kunne stanse løkka.
            if s.set_nonblocking(true).is_err() {
                continue;
            }
            self.reload(m, now);
            let allowed = m.cfg.as_ref().map(|c| c.uid);
            let peer = peer_uid(&s);
            let ok = matches!((peer, allowed), (Some((u, _)), Some(a)) if u == a);
            if !ok {
                journal::warn(&format!(
                    "Avviste en forbindelse fra uid {} (pid {}); bare uid {} er tillatt.",
                    peer.map_or(-1, |p| p.0 as i64),
                    peer.map_or(-1, |p| p.1),
                    allowed.map_or("ingen".into(), |a| a.to_string())
                ));
                let _ = Self::reply(
                    &mut s,
                    &serde_json::Value::Null,
                    &Err(ProtoError::msg(
                        ErrorCode::PermissionDenied,
                        "Bare brukeren som aktiverte hjelperen, får koble til.",
                    )),
                    Duration::ZERO,
                );
                continue;
            }
            if self.conns.len() >= hjelper::MAX_CLIENTS {
                let _ = Self::reply(
                    &mut s,
                    &serde_json::Value::Null,
                    &Err(ProtoError::new(ErrorCode::TooManyClients)),
                    Duration::ZERO,
                );
                continue;
            }
            self.seq += 1;
            self.conns.insert(
                self.seq,
                Conn {
                    uid: peer.map(|p| p.0).unwrap_or(u32::MAX),
                    stream: s,
                    buf: LineBuffer::new(),
                },
            );
        }
    }

    /// Leser det som er kommet og svarer. `false`: forbindelsen er lukket.
    fn read(&mut self, id: u64, m: &mut Motor, now: i64) -> bool {
        // Først alle hele linjer (lånet av forbindelsen slippes før svarene).
        let mut lines = Vec::new();
        let mut closed = false;
        {
            let Some(c) = self.conns.get_mut(&id) else {
                return false;
            };
            let mut chunk = [0u8; 4096];
            loop {
                match c.stream.read(&mut chunk) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => {
                        for ev in c.buf.push(&chunk[..n], hjelper::MAX_LINE) {
                            match ev {
                                LineEvent::Line(l) => lines.push(l),
                                LineEvent::TooLarge => {
                                    let _ = Self::reply(
                                        &mut c.stream,
                                        &serde_json::Value::Null,
                                        &Err(ProtoError::new(ErrorCode::MessageTooLarge)),
                                        REPLY_BUDGET,
                                    );
                                    return false;
                                }
                            }
                        }
                        if lines.len() > 64 {
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
        // En klient som allerede har lukket, får ikke utført skriving fra
        // data som lå igjen i mottakskøen (ingen kan bekrefte eller se den).
        if closed {
            return false;
        }
        for line in lines {
            let (rid, r) = match proto::parse_request(&line) {
                Ok(req) => {
                    // Trinnene kan være endret siden forrige runde (én stat).
                    self.reload(m, now);
                    let allowed = self
                        .conns
                        .get(&id)
                        .is_some_and(|c| m.cfg.as_ref().is_some_and(|cfg| cfg.uid == c.uid));
                    let result = if allowed {
                        m.handle(id, &req.op, &req.args, now)
                    } else {
                        Err(ProtoError::new(ErrorCode::PermissionDenied))
                    };
                    (req.id.clone(), result)
                }
                Err((rid, e)) => (rid, Err(e)),
            };
            let Some(c) = self.conns.get_mut(&id) else {
                return false;
            };
            if !Self::reply(&mut c.stream, &rid, &r, REPLY_BUDGET) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_hw::gpu::skriv::SimSkriver;
    use serde_json::{Value, json};
    use std::cell::Cell;
    use std::io::BufRead;

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "rigg-helper-server-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            Fixture(dir)
        }
        fn socket(&self) -> PathBuf {
            self.0.join("helper.sock")
        }
        /// En egen server med egen socket og en simulert motor. Ingen
        /// vertsstier, ingen ekte GPU.
        fn server(&self) -> (Server, Motor) {
            let path = self.socket();
            let listener = bind(&path, None).unwrap();
            // Ingen konfigfil: mtime er `None` og leses aldri på nytt.
            let srv = Server::new(listener, path, self.0.join("finnes-ikke.json"), None);
            let mut m = Motor::new(Some(Box::new(SimSkriver::new())), "sim", None);
            let uid = unsafe { libc::geteuid() };
            m.set_config(
                Config::parse(&format!(
                    r#"{{"versjon":1,"uid":{uid},"trinn":["effekt","vifter"]}}"#
                )),
                0,
            );
            (srv, m)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    thread_local! {
        static FAKE: Cell<i64> = const { Cell::new(0) };
    }
    fn fake_clock() -> Option<i64> {
        Some(FAKE.with(Cell::get))
    }
    fn set_fake(ms: i64) {
        FAKE.with(|c| c.set(ms));
    }

    #[test]
    fn tidslinjen_fryser_ikke_og_gaar_aldri_bakover() {
        let mut t = Tidslinje::default();
        assert_eq!(t.les(10_000), 10_000);
        // Et avvik bakover gir null delta, ikke en frossen takt i 9 s.
        assert_eq!(t.les(1_000), 10_000);
        assert_eq!(t.les(2_000), 11_000);
        assert_eq!(t.les(3_500), 12_500);
        assert_eq!(t.siste(), Some(12_500));
    }

    #[test]
    fn hjelperklokka_er_boottime_og_ikke_veggklokka() {
        let boot = boot_ms().expect("CLOCK_BOOTTIME");
        // /proc/uptime bygger på samme hviletellende klokke.
        let uptime = std::fs::read_to_string("/proc/uptime").unwrap();
        let secs: f64 = uptime.split_whitespace().next().unwrap().parse().unwrap();
        assert!(
            (boot - (secs * 1000.0) as i64).abs() < 5_000,
            "boot {boot} ms, uptime {secs} s"
        );
        assert!(
            (rigg_core::now_ms() - boot).abs() > 1_000_000,
            "motoren skal ikke få veggklokketid"
        );
    }

    #[test]
    fn takt_og_frist_folger_den_monotone_klokka_ogsaa_etter_avvik_bakover() {
        let fx = Fixture::new("klokke");
        let (mut srv, mut m) = fx.server();
        srv.clock = fake_clock;
        set_fake(5_000_000);
        assert!(srv.turn(&mut m));
        assert_eq!(srv.last_tick, Some(5_000_000));
        // Endring med 10 s frist på serverens tidslinje.
        let now = srv.now().unwrap();
        m.handle(
            1,
            "apply",
            &json!({"settings": {"powerLimitW": 200}, "confirmTimeoutS": 10}),
            now,
        )
        .unwrap();
        assert!(m.book.pending().is_some());
        // Råklokka går (unormalt) 60 s bakover: tidslinjen står, ingen frys.
        set_fake(4_940_000);
        assert!(srv.turn(&mut m));
        // Ett sekund senere på råklokka kommer neste takt, ikke etter 61 s.
        set_fake(4_941_000);
        assert!(srv.turn(&mut m));
        assert_eq!(srv.last_tick, Some(5_001_000));
        // Fristen går ut etter 10 s faktisk forløpt monoton tid.
        set_fake(4_950_000);
        assert!(srv.turn(&mut m));
        assert!(m.book.pending().is_none(), "fristen ruller tilbake");
        assert_eq!(m.status(0)["hardware"]["stock"], true);
        // Uten klokke stopper løkka i stedet for å gjette.
        srv.clock = || None;
        assert!(!srv.turn(&mut m));
    }

    #[test]
    fn eof_med_ventende_forespoersel_utforer_ingen_skriving() {
        let fx = Fixture::new("eof");
        let (mut srv, mut m) = fx.server();
        let mut c = UnixStream::connect(fx.socket()).unwrap();
        let req = json!({"id": 1, "op": "apply", "args": {"settings": {"powerLimitW": 200}}});
        c.write_all(format!("{req}\n").as_bytes()).unwrap();
        c.shutdown(std::net::Shutdown::Write).unwrap();
        // Runde 1 tar imot forbindelsen, runde 2 leser forespørsel + EOF.
        assert!(srv.turn(&mut m));
        assert_eq!(srv.conns.len(), 1);
        assert!(srv.turn(&mut m));
        assert!(srv.conns.is_empty(), "lukket klient fjernes");
        let s = m.status(0);
        assert_eq!(s["hardware"]["writes"], 0, "ingen skriving etter EOF: {s}");
        assert_eq!(s["hardware"]["stock"], true);
        assert!(s["events"].as_array().unwrap().is_empty(), "{s}");
        drop(c);
    }

    #[test]
    fn accept_er_begrenset_per_runde_og_godkjent_klient_faar_svar() {
        let fx = Fixture::new("svar");
        let (mut srv, mut m) = fx.server();
        // Flere ventende forbindelser enn MAX_CLIENTS: høyst fire per runde.
        let extra: Vec<UnixStream> = (0..hjelper::MAX_CLIENTS + 2)
            .map(|_| UnixStream::connect(fx.socket()).unwrap())
            .collect();
        assert!(srv.turn(&mut m));
        assert_eq!(srv.conns.len(), hjelper::MAX_CLIENTS);
        assert!(srv.turn(&mut m));
        // De to siste får «too_many_clients» uten å holde løkka igjen.
        assert_eq!(srv.conns.len(), hjelper::MAX_CLIENTS);
        let mut last = std::io::BufReader::new(extra.last().unwrap().try_clone().unwrap());
        extra
            .last()
            .unwrap()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut line = String::new();
        last.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["error"]["code"], "too_many_clients", "{v}");
        // Første klient får fortsatt svar på status.
        let mut first = extra[0].try_clone().unwrap();
        first
            .write_all(format!("{}\n", json!({"id": 7, "op": "status"})).as_bytes())
            .unwrap();
        assert!(srv.turn(&mut m));
        first
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut r = std::io::BufReader::new(first);
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn svar_venter_kort_paa_plass_i_bufferen_i_stedet_for_aa_droppe() {
        let (mut a, b) = UnixStream::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        // Fyll sendebufferen helt.
        let fill = vec![b'x'; 1 << 16];
        loop {
            match a.write(&fill) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        // Uten leser: avbrytes innen budsjettet.
        let t = Instant::now();
        assert!(!write_bounded(&mut a, b"{}\n", Duration::from_millis(50)));
        assert!(t.elapsed() < Duration::from_secs(1));
        // Med en leser som tømmer bufferen litt senere: svaret kommer fram.
        let reader = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let mut b = b;
            let mut sink = vec![0u8; 1 << 16];
            let mut total = 0usize;
            b.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            while let Ok(n) = b.read(&mut sink) {
                if n == 0 {
                    break;
                }
                total += n;
            }
            total
        });
        assert!(write_bounded(&mut a, b"ok\n", Duration::from_secs(2)));
        drop(a);
        assert!(reader.join().unwrap() > 0);
    }

    #[test]
    fn stopp_fjerner_bare_socketen_vi_bandt() {
        let fx = Fixture::new("inode");
        let path = fx.socket();
        let first = bind(&path, None).unwrap();
        let ours = socket_identity(&path);
        assert!(ours.is_some());
        // En annen prosess erstatter stien med sin egen socket.
        drop(first);
        std::fs::remove_file(&path).unwrap();
        let _other = UnixListener::bind(&path).unwrap();
        assert!(!remove_own_socket(&path, ours));
        assert!(path.exists(), "en annens socket står urørt");
        assert!(remove_own_socket(&path, socket_identity(&path)));
        assert!(!path.exists());
        // En vanlig fil er aldri «vår socket».
        std::fs::write(&path, b"").unwrap();
        assert!(!remove_own_socket(&path, ours));
        assert!(path.exists());
    }
}
