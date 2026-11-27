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
use rigg_core::proto::{self, ErrorCode, ProtoError};
use std::collections::{BTreeMap, VecDeque};
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
/// All venting på svar i én runde til sammen, så takten (frist og
/// nødregel) aldri skyves mer enn dette av trege klienter.
const TURN_REPLY_BUDGET: Duration = Duration::from_millis(500);
/// Byte lest fra én forbindelse per runde, også tomme linjer og mellomrom.
const MAX_BYTES_PER_TURN: usize = 64 * 1024;
/// Linjeskift fra én forbindelse per runde: forespørsler og tomme linjer.
/// Det som er lest utover, venter i forbindelsens kø til neste runde.
const MAX_LINES_PER_TURN: usize = 64;
/// Lukket, feilet eller halvlukket (lesesiden hos oss er ferdig) forbindelse.
const HANGUP: libc::c_short = libc::POLLHUP | libc::POLLERR | libc::POLLNVAL | libc::POLLRDHUP;

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
pub fn load_config(path: &Path) -> Result<Config, String> {
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
    Config::parse(&text)
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
    /// Byte etter siste linjeskift (en linje som ikke er hel ennå).
    partial: Vec<u8>,
    /// Hele linjer i ankomstrekkefølge som ikke er behandlet ennå. `None`:
    /// tom linje eller bare mellomrom (teller mot rundens grense).
    queue: VecDeque<Option<Vec<u8>>>,
}

/// Deler mottatte byte i linjer i ankomstrekkefølge: hvert linjeskift gir én
/// oppføring i `queue`, `None` for tomme linjer og mellomrom, ellers linjen
/// uten `\n` og `\r`. `Err`: en linje er lengre enn `MAX_LINE`.
fn frame(
    partial: &mut Vec<u8>,
    data: &[u8],
    queue: &mut VecDeque<Option<Vec<u8>>>,
) -> Result<(), ()> {
    partial.extend_from_slice(data);
    let mut start = 0;
    while let Some(pos) = partial[start..].iter().position(|&b| b == b'\n') {
        let end = start + pos;
        let mut line = &partial[start..end];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        if line.len() > hjelper::MAX_LINE {
            return Err(());
        }
        queue.push_back((!line.iter().all(u8::is_ascii_whitespace)).then(|| line.to_vec()));
        start = end + 1;
    }
    partial.drain(..start);
    if partial.len() > hjelper::MAX_LINE {
        return Err(());
    }
    Ok(())
}

/// Har motparten lukket eller halvlukket? Ingen lesing, null ventetid.
fn peer_closed(stream: &UnixStream) -> bool {
    let mut fd = libc::pollfd {
        fd: stream.as_raw_fd(),
        events: libc::POLLRDHUP,
        revents: 0,
    };
    // SAFETY: én gyldig pollfd; null ventetid.
    let r = unsafe { libc::poll(&mut fd, 1, 0) };
    r < 0 || r > 0 && fd.revents & HANGUP != 0
}

/// `(dev, inode)` for socketen på stien, hvis stien er en socket.
pub(crate) fn socket_identity(path: &Path) -> Option<(u64, u64)> {
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
    conns: BTreeMap<u64, Conn>,
    seq: u64,
    /// Socketen vi bandt (`dev`, `inode`), så stopp ikke fjerner en annens.
    socket_id: Option<(u64, u64)>,
    /// Råklokka (`CLOCK_BOOTTIME`); testene kan bytte den ut.
    clock: fn() -> Option<i64>,
    line: Tidslinje,
    last_tick: Option<i64>,
    /// Felles svarbudsjett for runden (se `TURN_REPLY_BUDGET`).
    reply_until: Option<Instant>,
    /// Klokka feilet midt i runden: ingen flere forespørsler, og løkka stopper.
    clock_lost: bool,
}

impl Server {
    pub fn new(
        listener: UnixListener,
        socket: PathBuf,
        config: PathBuf,
    ) -> Server {
        Server {
            listener,
            socket_id: socket_identity(&socket),
            socket,
            config,
            conns: BTreeMap::new(),
            seq: 0,
            clock: boot_ms,
            line: Tidslinje::default(),
            last_tick: None,
            reply_until: None,
            clock_lost: false,
        }
    }

    /// Tiden motoren får: monoton, også over hvile og veggklokkehopp.
    fn now(&mut self) -> Option<i64> {
        let t = (self.clock)().map(|raw| self.line.les(raw));
        if t.is_none() {
            self.clock_lost = true;
            journal::err("Den monotone klokka (CLOCK_BOOTTIME) kan ikke leses. Stopper.");
        }
        t
    }

    /// Ventetid for neste svar: høyst `REPLY_BUDGET`, og aldri mer enn det
    /// som er igjen av rundens felles budsjett.
    fn reply_budget(&self) -> Duration {
        self.reply_until
            .map_or(REPLY_BUDGET, |u| u.saturating_duration_since(Instant::now()))
            .min(REPLY_BUDGET)
    }

    /// Les innholdet på nytt før hver forespørsel og hver periodiske takt.
    fn reload(&mut self, m: &mut Motor, now: i64) {
        m.set_config(load_config(&self.config), now);
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
    /// Klokka leses på nytt før hver forespørsel og før takten, så svar som
    /// har ventet aldri gir en foreldet tid. `false`: klokka kan ikke leses,
    /// og tjenesten stopper trygt.
    fn turn(&mut self, m: &mut Motor) -> bool {
        let Some(start) = self.now() else {
            return false;
        };
        // Linjer som venter i en kø, behandles neste runde uten å vente.
        let queued = self.conns.values().any(|c| !c.queue.is_empty());
        let wait = if queued {
            0
        } else {
            self.last_tick
                .map_or(0, |l| (1000 - start.saturating_sub(l)).clamp(0, 1000)) as i32
        };
        let mut fds = vec![libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        let ids: Vec<u64> = self.conns.keys().copied().collect();
        for id in &ids {
            fds.push(libc::pollfd {
                fd: self.conns[id].stream.as_raw_fd(),
                events: libc::POLLIN | libc::POLLRDHUP,
                revents: 0,
            });
        }
        // SAFETY: fds er en gyldig liste med pollfd.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait) };
        self.reply_until = Some(Instant::now() + TURN_REPLY_BUDGET);
        if n > 0 && fds[0].revents & libc::POLLIN != 0 {
            let Some(now) = self.now() else {
                return false;
            };
            self.accept(m, now);
        }
        for (i, id) in ids.iter().enumerate() {
            let ev = if n > 0 { fds[i + 1].revents } else { 0 };
            let queued = self.conns.get(id).is_some_and(|c| !c.queue.is_empty());
            if (ev != 0 || queued) && !self.read(*id, m, ev) {
                self.conns.remove(id);
                let Some(now) = self.now() else {
                    return false;
                };
                m.disconnect(*id, now);
            }
            if self.clock_lost {
                return false;
            }
        }
        let Some(now) = self.now() else {
            return false;
        };
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
                    partial: Vec::new(),
                    queue: VecDeque::new(),
                },
            );
        }
    }

    /// Leser det som er kommet og svarer. `false`: forbindelsen er lukket.
    /// `ev` er poll-hendelsene for forbindelsen i denne runden.
    fn read(&mut self, id: u64, m: &mut Motor, ev: libc::c_short) -> bool {
        // Høyst `MAX_BYTES_PER_TURN` byte per runde, og det leses bare så
        // lenge køen har færre enn `MAX_LINES_PER_TURN` linjer. Deretter tas
        // høyst så mange linjer fra fronten av køen, tomme linjer medregnet:
        // eldre forespørsler går alltid før nyere tomme linjer.
        let mut closed = ev & HANGUP != 0;
        let lines: Vec<Option<Vec<u8>>> = {
            let Some(c) = self.conns.get_mut(&id) else {
                return false;
            };
            let mut chunk = [0u8; 4096];
            let mut total = 0usize;
            while !closed && total < MAX_BYTES_PER_TURN && c.queue.len() < MAX_LINES_PER_TURN {
                let room = chunk.len().min(MAX_BYTES_PER_TURN - total);
                match c.stream.read(&mut chunk[..room]) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => {
                        total += n;
                        if frame(&mut c.partial, &chunk[..n], &mut c.queue).is_err() {
                            let _ = write_bounded(
                                &mut c.stream,
                                &proto::response_line(
                                    &serde_json::Value::Null,
                                    &Err(ProtoError::new(ErrorCode::MessageTooLarge)),
                                ),
                                Duration::ZERO,
                            );
                            return false;
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
            let take = MAX_LINES_PER_TURN.min(c.queue.len());
            c.queue.drain(..take).collect()
        };
        // En klient som allerede har lukket, får ikke utført skriving fra
        // data som lå igjen i mottakskøen (ingen kan bekrefte eller se den).
        // Også ved grensen for runden: lukking sjekkes før hver forespørsel.
        if closed {
            return false;
        }
        for line in lines.into_iter().flatten() {
            if self.conns.get(&id).is_none_or(|c| peer_closed(&c.stream)) {
                return false;
            }
            // Fersk tid for hver forespørsel: et tidligere svar kan ha ventet.
            let Some(now) = self.now() else {
                return true;
            };
            let (rid, r) = match proto::parse_request(&line) {
                Ok(req) => {
                    // Tillatelsen kan være erstattet med samme endringstid.
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
            let budget = self.reply_budget();
            let Some(c) = self.conns.get_mut(&id) else {
                return false;
            };
            if !Self::reply(&mut c.stream, &rid, &r, budget) {
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
            // Kort rot: socketstien må holde seg under SUN_LEN også med lang TMPDIR.
            let dir = rigg_core::paths::short_tmp().join(format!(
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
            // Testrøttens 0700 beskytter socketen fra første øyeblikk.
            // Produksjonsbind endrer umask prosessvidt og prøves i egen prosess.
            let listener = UnixListener::bind(&path).unwrap();
            listener.set_nonblocking(true).unwrap();
            let config = self.0.join("helper.json");
            let uid = unsafe { libc::geteuid() };
            std::fs::write(
                &config,
                format!(r#"{{"versjon":1,"uid":{uid},"trinn":["effekt","vifter"]}}"#),
            ).unwrap();
            let mut m = Motor::new(Some(Box::new(SimSkriver::new())), "sim", None);
            std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
            m.set_config(Ok(load_config(&config).unwrap()), 0);
            let srv = Server::new(listener, path, config);
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
        /// Så mange ms går råklokka fram for hver lesing.
        static STEP: Cell<i64> = const { Cell::new(0) };
    }
    fn fake_clock() -> Option<i64> {
        let t = FAKE.with(Cell::get);
        FAKE.with(|c| c.set(t + STEP.with(Cell::get)));
        Some(t)
    }
    fn set_fake(ms: i64) {
        FAKE.with(|c| c.set(ms));
    }

    fn send(c: &UnixStream, v: Value) {
        (&*c).write_all(format!("{v}\n").as_bytes()).unwrap();
    }

    #[test]
    fn hver_forespoersel_faar_fersk_tid_ikke_rundens_gamle() {
        let fx = Fixture::new("fersk");
        let (mut srv, mut m) = fx.server();
        srv.clock = fake_clock;
        STEP.with(|c| c.set(0));
        set_fake(2_000_000);
        let c = UnixStream::connect(fx.socket()).unwrap();
        assert!(srv.turn(&mut m));
        let id = *srv.conns.keys().next().unwrap();
        let r = m
            .handle(
                id,
                "apply",
                &json!({"settings": {"powerLimitW": 200}, "confirmTimeoutS": 10}),
                2_000_000,
            )
            .unwrap();
        let deadline = 2_010_000;
        // To forespørsler i samme runde. Runden starter 2 ms før fristen,
        // og klokka går 1 ms per lesing: «status» kommer før fristen, Behold
        // etter. Med én felles tid for runden ville Behold blitt godtatt.
        send(&c, json!({"id": 1, "op": "status"}));
        send(&c, json!({"id": 2, "op": "confirm", "args": {"txn": r["txn"]}}));
        STEP.with(|c| c.set(1));
        set_fake(deadline - 2);
        assert!(srv.turn(&mut m));
        STEP.with(|c| c.set(0));
        let mut r = std::io::BufReader::new(c.try_clone().unwrap());
        c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["ok"], true);
        line.clear();
        r.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["ok"], false, "foreldet tid ville godtatt Behold: {v}");
        assert_eq!(v["error"]["code"], "no_pending", "{v}");
        assert_eq!(m.status(0)["hardware"]["stock"], true);
    }

    #[test]
    fn tomme_linjer_er_begrenset_per_runde() {
        let fx = Fixture::new("tomme");
        let (mut srv, mut m) = fx.server();
        let c = UnixStream::connect(fx.socket()).unwrap();
        assert!(srv.turn(&mut m));
        c.set_nonblocking(true).unwrap();
        let blank = [b'\n'; 4096];
        let mut written = 0;
        while written < 3 * MAX_BYTES_PER_TURN {
            match (&c).write(&blank) {
                Ok(n) => written += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        assert!(written > MAX_BYTES_PER_TURN + 4096, "{written}");
        let id = *srv.conns.keys().next().unwrap();
        assert!(srv.read(id, &mut m, libc::POLLIN));
        // Resten ligger igjen til neste runde; takten er ikke holdt igjen.
        let fd = srv.conns[&id].stream.as_raw_fd();
        let mut b = [0u8; 1];
        // SAFETY: gyldig fd og buffer; bare titting, ingen venting.
        let left = unsafe {
            libc::recv(
                fd,
                b.as_mut_ptr().cast(),
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        assert_eq!(left, 1);
        drop(c);
    }

    /// Svarlinjer som er kommet fram nå, uten å vente på flere.
    fn replies_now(r: &mut std::io::BufReader<UnixStream>) -> Vec<Value> {
        r.get_ref()
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut out = Vec::new();
        loop {
            let mut line = String::new();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => return out,
                Ok(_) => out.push(serde_json::from_str(&line).unwrap()),
            }
        }
    }

    #[test]
    fn hoyst_64_linjer_per_runde_og_resten_neste_runde() {
        let fx = Fixture::new("linjer");
        let (mut srv, mut m) = fx.server();
        let c = UnixStream::connect(fx.socket()).unwrap();
        assert!(srv.turn(&mut m));
        let id = *srv.conns.keys().next().unwrap();
        let mut r = std::io::BufReader::new(c.try_clone().unwrap());
        // 100 korte forespørsler i én skriving (godt under én 4 KiB-bit).
        let mut batch = String::new();
        for i in 0..100 {
            batch.push_str(&format!("{{\"id\":{i},\"op\":\"nop\"}}\n"));
        }
        assert!(batch.len() < 4096);
        (&c).write_all(batch.as_bytes()).unwrap();
        assert!(srv.read(id, &mut m, libc::POLLIN));
        let first = replies_now(&mut r);
        assert_eq!(first.len(), MAX_LINES_PER_TURN);
        assert_eq!(first[63]["id"], 63);
        assert_eq!(srv.conns[&id].queue.len(), 100 - MAX_LINES_PER_TURN);
        // Neste runde venter ikke på poll når køen har linjer, og tar resten.
        let t = Instant::now();
        assert!(srv.turn(&mut m));
        assert!(t.elapsed() < Duration::from_millis(500), "{:?}", t.elapsed());
        let rest = replies_now(&mut r);
        assert_eq!(rest.len(), 100 - MAX_LINES_PER_TURN);
        assert_eq!(rest[0]["id"], 64);
        assert!(srv.conns[&id].queue.is_empty());

        // Tomme linjer teller med: 60 tomme gir plass til bare 4 forespørsler.
        let mut batch = "\n".repeat(60);
        for i in 0..10 {
            batch.push_str(&format!("{{\"id\":{},\"op\":\"nop\"}}\n", 100 + i));
        }
        (&c).write_all(batch.as_bytes()).unwrap();
        assert!(srv.read(id, &mut m, libc::POLLIN));
        assert_eq!(replies_now(&mut r).len(), 4);
        assert!(srv.turn(&mut m));
        let rest = replies_now(&mut r);
        assert_eq!(rest.len(), 6);
        assert_eq!(rest[5]["id"], 109);
    }

    #[test]
    fn tomme_linjer_tar_64_per_runde_og_eldre_forespoersler_gaar_foerst() {
        let fx = Fixture::new("rekkefolge");
        let (mut srv, mut m) = fx.server();
        let c = UnixStream::connect(fx.socket()).unwrap();
        assert!(srv.turn(&mut m));
        let id = *srv.conns.keys().next().unwrap();
        let mut r = std::io::BufReader::new(c.try_clone().unwrap());
        // (a) 200 tomme linjer i én skriving: 64 per runde.
        (&c).write_all("\n".repeat(200).as_bytes()).unwrap();
        for left in [136, 72, 8, 0] {
            assert!(srv.read(id, &mut m, libc::POLLIN));
            assert_eq!(srv.conns[&id].queue.len(), left);
        }
        assert!(replies_now(&mut r).is_empty());
        // (b) 100 forespørsler, så 64 tomme linjer og én forespørsel til:
        // de 36 eldre i køen behandles før de nye tomme linjene.
        let req = |i: usize| format!("{{\"id\":{i},\"op\":\"nop\"}}\n");
        (&c).write_all((0..100).map(req).collect::<String>().as_bytes()).unwrap();
        assert!(srv.read(id, &mut m, libc::POLLIN));
        assert_eq!(replies_now(&mut r).len(), 64);
        let mut later = "\n".repeat(64);
        later.push_str(&req(500));
        (&c).write_all(later.as_bytes()).unwrap();
        assert!(srv.read(id, &mut m, libc::POLLIN));
        let ids: Vec<i64> = replies_now(&mut r)
            .iter()
            .map(|v| v["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, (64..100).collect::<Vec<i64>>());
        assert!(srv.read(id, &mut m, 0));
        let last = replies_now(&mut r);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0]["id"], 500);
        assert!(srv.conns[&id].queue.is_empty());
    }

    #[test]
    fn eof_ved_grensen_for_runden_utforer_ingen_skriving() {
        let fx = Fixture::new("eof-grense");
        let (mut srv, mut m) = fx.server();
        let c = UnixStream::connect(fx.socket()).unwrap();
        assert!(srv.turn(&mut m));
        // Flere forespørsler enn én runde tar, så lukking av skrivesiden.
        let mut batch = String::new();
        for i in 0..MAX_LINES_PER_TURN + 6 {
            batch.push_str(&format!(
                "{}\n",
                json!({"id": i, "op": "apply", "args": {"settings": {"powerLimitW": 200}}})
            ));
        }
        (&c).write_all(batch.as_bytes()).unwrap();
        c.shutdown(std::net::Shutdown::Write).unwrap();
        let id = *srv.conns.keys().next().unwrap();
        // Lesing direkte, uten poll-hendelsen: lukkingen må likevel oppdages.
        assert!(!srv.read(id, &mut m, libc::POLLIN));
        let s = m.status(0);
        assert_eq!(s["hardware"]["writes"], 0, "{s}");
        assert!(m.book.pending().is_none());
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
        let first = UnixListener::bind(&path).unwrap();
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
