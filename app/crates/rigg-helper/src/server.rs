//! Socketen og løkka: `poll` på lytteren og høyst fire forbindelser, ett
//! sekunds takt for fristen og nødregelen. Én tråd, ingen GLib.
//!
//! Hver forbindelse sjekkes med `SO_PEERCRED`: bare uid-en i `helper.json`
//! slipper inn. Socketen er `0600` og eies av den uid-en (som root:
//! `chown`, det eneste privilegiet tjenesten har i tillegg).

use crate::journal;
use crate::motor::Motor;
use rigg_core::hjelper::{self, Config};
use rigg_core::proto::{self, ErrorCode, LineBuffer, LineEvent, ProtoError};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub static STOP: AtomicBool = AtomicBool::new(false);

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
        use std::os::unix::fs::FileTypeExt;
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

pub struct Server {
    pub listener: UnixListener,
    pub socket: PathBuf,
    pub config: PathBuf,
    cfg_mtime: Option<i64>,
    conns: BTreeMap<u64, Conn>,
    seq: u64,
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
            socket,
            config,
            cfg_mtime,
            conns: BTreeMap::new(),
            seq: 0,
        }
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

    fn reply(stream: &mut UnixStream, id: &serde_json::Value, r: &proto::OpResult) -> bool {
        stream.write_all(&proto::response_line(id, r)).is_ok()
    }

    pub fn run(&mut self, m: &mut Motor) {
        let mut last_tick = 0i64;
        while !STOP.load(Ordering::SeqCst) {
            let now = rigg_core::now_ms();
            let wait = (1000 - (now - last_tick)).clamp(0, 1000) as i32;
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
            let now = rigg_core::now_ms();
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
            if now - last_tick >= 1000 {
                last_tick = now;
                self.reload(m, now);
                m.tick(now);
            }
        }
        journal::notice("Stopper.");
        m.shutdown(rigg_core::now_ms());
        let _ = std::fs::remove_file(&self.socket);
    }

    fn accept(&mut self, m: &mut Motor, now: i64) {
        while let Ok((mut s, _)) = self.listener.accept() {
            self.reload(m, now);
            let allowed = m.cfg.as_ref().map(|c| c.uid);
            let peer = peer_uid(&s);
            let ok = matches!((peer, allowed), (Some((u, _)), Some(a)) if u == a);
            let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
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
                );
                continue;
            }
            if self.conns.len() >= hjelper::MAX_CLIENTS {
                let _ = Self::reply(
                    &mut s,
                    &serde_json::Value::Null,
                    &Err(ProtoError::new(ErrorCode::TooManyClients)),
                );
                continue;
            }
            if s.set_nonblocking(true).is_err() {
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
            if !Self::reply(&mut c.stream, &rid, &r) {
                return false;
            }
        }
        !closed
    }
}
