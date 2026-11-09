//! Socket-tjenesten: Unix-socket med NDJSON (docs/daemon-api.md kap. 1).
//!
//! Lytteren og klientene er ikke-blokkerende og drives av GLib-hovedløkka
//! (`unix_fd_add_local`), uten tråder og uten tokio. Tilgang: mappa er
//! `0700`, socketen `0600`, og `SO_PEERCRED` sjekker at klienten har samme
//! UID. Høyst 16 klienter og 64 KiB per linje. Klienter som ikke leser
//! (over 4 MiB i kø), kobles fra.

use rigg_core::proto::LineBuffer;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// Mest vi holder i kø til én klient før den kobles fra.
pub const MAX_QUEUE: usize = 4 * 1024 * 1024;

pub struct Client {
    pub stream: UnixStream,
    pub lines: LineBuffer,
    pub out: Vec<u8>,
    pub in_watch: Option<glib::SourceId>,
    pub out_watch: Option<glib::SourceId>,
    pub uid: Option<u32>,
    pub pid: Option<i32>,
}

pub struct Server {
    pub path: PathBuf,
    pub listener: UnixListener,
    pub listen_watch: Option<glib::SourceId>,
    pub clients: BTreeMap<u64, Client>,
    next_id: u64,
}

/// Klientens UID og PID fra `SO_PEERCRED`.
pub fn peer_cred(s: &UnixStream) -> Option<(u32, i32)> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: gyldig fd, og bufferen er en ucred med riktig lengde.
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len) };
    (r == 0).then_some((cred.uid, cred.pid))
}

pub fn my_uid() -> u32 {
    // SAFETY: getuid kan ikke feile.
    unsafe { libc::getuid() }
}

pub enum Bind {
    Ok(UnixListener),
    /// En annen tjeneste svarer allerede på socketen.
    Busy,
    Err(String),
}

/// Binder socketen. En gammel socket som ingen svarer på, fjernes først.
pub fn bind(path: &Path) -> Bind {
    use std::os::unix::fs::PermissionsExt;
    if path.as_os_str().len() > 107 {
        return Bind::Err(format!("Stien til socketen er for lang (høyst 107 byte): {}", path.display()));
    }
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Bind::Busy;
        }
        let _ = std::fs::remove_file(path);
    }
    // Socketen lages med 0600 fra start (umask), så ingen andre rekker å koble til.
    // SAFETY: umask kan ikke feile; den gamle verdien settes tilbake straks.
    let old = unsafe { libc::umask(0o177) };
    let res = UnixListener::bind(path);
    // SAFETY: som over.
    unsafe { libc::umask(old) };
    match res {
        Ok(l) => {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            if let Err(e) = l.set_nonblocking(true) {
                return Bind::Err(e.to_string());
            }
            Bind::Ok(l)
        }
        Err(e) => Bind::Err(format!("Kunne ikke lytte på {}: {e}", path.display())),
    }
}

impl Server {
    pub fn new(path: PathBuf, listener: UnixListener) -> Server {
        Server { path, listener, listen_watch: None, clients: BTreeMap::new(), next_id: 1 }
    }

    /// Godtar nye klienter. Gir (id, avvist-melding) for hver.
    pub fn accept(&mut self) -> Vec<u64> {
        let mut out = Vec::new();
        loop {
            match self.listener.accept() {
                Ok((s, _)) => {
                    let _ = s.set_nonblocking(true);
                    let cred = peer_cred(&s);
                    let id = self.next_id;
                    self.next_id += 1;
                    self.clients.insert(
                        id,
                        Client { stream: s, lines: LineBuffer::new(), out: Vec::new(), in_watch: None, out_watch: None, uid: cred.map(|c| c.0), pid: cred.map(|c| c.1) },
                    );
                    out.push(id);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        out
    }

    /// Leser det som ligger klart. `Ok(true)` betyr at klienten har lukket.
    pub fn read(&mut self, id: u64, buf: &mut Vec<u8>) -> std::io::Result<bool> {
        let Some(c) = self.clients.get_mut(&id) else { return Ok(true) };
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match c.stream.read(&mut chunk) {
                Ok(0) => return Ok(true),
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > 256 * 1024 {
                        return Ok(false);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(false),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Legger bytes i køen og skriver det som går nå. Gir `false` når
    /// klienten må kobles fra (feil, eller for mye i kø).
    pub fn queue(&mut self, id: u64, bytes: &[u8]) -> bool {
        let Some(c) = self.clients.get_mut(&id) else { return false };
        c.out.extend_from_slice(bytes);
        if c.out.len() > MAX_QUEUE {
            return false;
        }
        Self::flush_client(c)
    }

    pub fn flush(&mut self, id: u64) -> bool {
        match self.clients.get_mut(&id) {
            Some(c) => Self::flush_client(c),
            None => false,
        }
    }

    fn flush_client(c: &mut Client) -> bool {
        while !c.out.is_empty() {
            match c.stream.write(&c.out) {
                Ok(0) => return false,
                Ok(n) => {
                    c.out.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return true,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return false,
            }
        }
        true
    }

    pub fn pending(&self, id: u64) -> bool {
        self.clients.get(&id).is_some_and(|c| !c.out.is_empty())
    }
}
