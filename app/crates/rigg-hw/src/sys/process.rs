//! Behovsstyrt lesing av /proc, uten egen tråd eller privilegert hjelper.
//!
//! Felt og enheter: https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html
//! UID/RSS: https://docs.kernel.org/filesystems/proc.html
//! Sikker prosessreferanse: https://man7.org/linux/man-pages/man2/pidfd_open.2.html
//! Kun SIGTERM: https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html
use rigg_core::process::{ProcessIdentity, ProcessRow, ProcessSnapshot};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_PROC_FILE: u64 = 64 * 1024;
const PF_KTHREAD: u64 = 0x0020_0000;

#[derive(Debug)]
struct Stat {
    identity: ProcessIdentity,
    name: String,
    state: String,
    ppid: u32,
    ticks: u64,
    rss_pages: i64,
    flags: u64,
}

/// `comm` kan selv inneholde mellomrom, parenteser og linjeskift. De faste
/// numeriske feltene følger siste avsluttende parentes; split_whitespace på
/// hele stat-filen ville forskjøvet utime/stime/starttime/RSS.
fn parse_stat(text: &str) -> Option<Stat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open {
        return None;
    }
    let pid = text[..open].trim().parse::<u32>().ok()?;
    let fields: Vec<_> = text[close + 1..].split_whitespace().take(22).collect();
    if fields.len() < 22 || fields[0].len() != 1 {
        return None;
    }
    let number = |i: usize| fields[i].parse::<u64>().ok();
    Some(Stat {
        identity: ProcessIdentity {
            pid,
            start_ticks: number(19)?,
        },
        name: text[open + 1..close]
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(128)
            .collect(),
        state: fields[0].into(),
        ppid: fields[1].parse().ok()?,
        flags: number(6)?,
        ticks: number(11)?.checked_add(number(12)?)?,
        rss_pages: fields[21].parse().ok()?,
    })
}

#[derive(Debug)]
struct Status {
    uids: [u32; 4],
    kernel: bool,
    privileged: bool,
}

fn parse_status(text: &str) -> Option<Status> {
    let line = text.lines().find_map(|l| l.strip_prefix("Uid:"))?;
    let values: Vec<u32> = line
        .split_whitespace()
        .take(4)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let uids: [u32; 4] = values.try_into().ok()?;
    let kernel = text
        .lines()
        .any(|l| l.strip_prefix("Kthread:").is_some_and(|v| v.trim() == "1"));
    // En ufullstendig/malformet statusfil gir aldri lov til å signalisere.
    let privileged = ["CapEff:", "CapPrm:", "CapAmb:"].iter().any(|key| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
            != Some(0)
    });
    Some(Status {
        uids,
        kernel,
        privileged,
    })
}

fn read_small(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_PROC_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PROC_FILE as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "For stor /proc-fil",
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_stat(root: &Path, pid: u32) -> io::Result<Stat> {
    let text = read_small(&root.join(format!("proc/{pid}/stat")))?;
    parse_stat(&text)
        .filter(|s| s.identity.pid == pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Ugyldig prosessinformasjon"))
}

pub fn read_identity(root: &Path, pid: u32) -> Option<ProcessIdentity> {
    read_stat(root, pid).ok().map(|s| s.identity)
}

fn critical_name(name: &str) -> bool {
    matches!(
        name,
        "init"
            | "systemd"
            | "systemd-logind"
            | "systemd-udevd"
            | "systemd-journal"
            | "dbus-daemon"
            | "dbus-broker"
            | "dbus-broker-lau"
            | "gnome-shell"
            | "gnome-session"
            | "gnome-session-b"
            | "gnome-session-binary"
            | "gdm"
            | "gdm3"
            | "gdm-session-wor"
            | "gdm-wayland-ses"
            | "Xorg"
            | "Xwayland"
            | "sshd"
            | "sshd-session"
            | "login"
            | "polkitd"
            | "pipewire"
            | "pipewire-pulse"
            | "wireplumber"
            | "pulseaudio"
            | "rigg"
            | "riggd"
            | "rigg-hjelper"
            | "rigg-helper"
    ) || [
        "gsd-",
        "agentic-rigg",
        "gnome-session",
        "gdm-",
        "dbus-broker",
        "systemd-",
        "gnome-keyring",
        "xdg-desktop-por",
        "xdg-document-po",
        "xdg-permission-",
        "ibus-",
        "at-spi-",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// Appen, tilkoblede klienter og alle deres foreldre er beskyttet. Hele
/// foreldrekjeden må kunne kontrolleres; bortfalt/ulest klient gir feil.
/// Bruk samme funksjon for knappenes status og umiddelbart før SIGTERM.
pub fn protected_ancestors(root: &Path, client_pids: &[u32]) -> Result<HashSet<u32>, String> {
    let mut result = HashSet::new();
    for first in std::iter::once(std::process::id()).chain(client_pids.iter().copied()) {
        let mut pid = first;
        let mut chain = HashSet::new();
        let mut completed = false;
        for _ in 0..1024 {
            if pid <= 1 {
                completed = true;
                break;
            }
            if !chain.insert(pid) {
                return Err("Ugyldig foreldrekjede for appen eller en tilkoblet klient".into());
            }
            if result.contains(&pid) {
                completed = true;
                break;
            }
            result.insert(pid);
            pid = read_stat(root, pid)
                .map_err(|_| {
                    "Kunne ikke kontrollere foreldreprosesser for appen eller en tilkoblet klient"
                        .to_string()
                })?
                .ppid;
        }
        if !completed {
            return Err("Foreldrekjeden er for lang".into());
        }
    }
    Ok(result)
}

fn ancestors(root: &Path) -> Result<HashSet<u32>, String> {
    protected_ancestors(root, &[])
}

fn denial(
    stat: &Stat,
    status: Option<&Status>,
    uid: u32,
    euid: u32,
    protected: &HashSet<u32>,
) -> Option<String> {
    let message = if stat.identity.pid <= 1 {
        Some("Systemets init-prosess er beskyttet")
    } else if protected.contains(&stat.identity.pid) {
        Some("Appen eller en foreldreprosess er beskyttet")
    } else if critical_name(&stat.name) {
        Some("Viktig system- eller øktprosess er beskyttet")
    } else if matches!(stat.state.as_str(), "Z" | "X" | "x") {
        Some("Prosessen er allerede avsluttet")
    } else if stat.flags & PF_KTHREAD != 0 || status.is_some_and(|s| s.kernel) {
        Some("Kjerneprosesser er beskyttet")
    } else if uid == 0 || euid != uid {
        Some("Avslutning krever en vanlig brukerøkt")
    } else if let Some(status) = status {
        if status.uids.contains(&0) {
            Some("Root-prosesser er beskyttet")
        } else if status.uids.iter().any(|u| *u != uid) {
            Some("Prosessen tilhører en annen bruker eller har andre rettigheter")
        } else if status.privileged {
            Some("Prosessen har særskilte systemrettigheter")
        } else {
            None
        }
    } else {
        Some("Kan ikke kontrollere prosessens brukerrettigheter")
    };
    message.map(str::to_string)
}

/// CPU-målingen er knyttet til identitet + monoton tid. Ingen wall-clock-
/// differanse, barnas CPU-tid eller normalisering mot antall logiske CPU-er.
fn cpu_percent(
    old: Option<(u64, Instant)>,
    ticks: u64,
    now: Instant,
    ticks_per_second: Option<u64>,
) -> Option<f64> {
    let (previous, then) = old?;
    let elapsed = now.checked_duration_since(then)?.as_secs_f64();
    let delta = ticks.checked_sub(previous)?;
    let hz = ticks_per_second?;
    if elapsed <= 0.0 || hz == 0 {
        return None;
    }
    let percent = delta as f64 / hz as f64 / elapsed * 100.0;
    percent.is_finite().then_some(percent)
}

pub struct ProcessReader {
    root: PathBuf,
    previous: HashMap<ProcessIdentity, (u64, Instant)>,
    page_size: Option<u64>,
    ticks_per_second: Option<u64>,
    users: HashMap<u32, String>,
    users_at: Option<Instant>,
}

impl ProcessReader {
    pub fn new(root: &Path) -> Self {
        // SAFETY: sysconf har ingen pekarargument eller sideeffekter.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        Self {
            root: root.into(),
            previous: HashMap::new(),
            page_size: u64::try_from(page).ok().filter(|n| *n > 0),
            ticks_per_second: u64::try_from(ticks).ok().filter(|n| *n > 0),
            users: HashMap::new(),
            users_at: None,
        }
    }

    /// Kalles når siden slutter å bli brukt; neste prøve har ukjent CPU-rate.
    pub fn reset(&mut self) {
        self.previous.clear();
    }

    pub fn sample(&mut self, now_ms: i64) -> ProcessSnapshot {
        self.sample_at(now_ms, Instant::now())
    }

    fn sample_at(&mut self, now_ms: i64, now: Instant) -> ProcessSnapshot {
        if self
            .users_at
            .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(30))
        {
            self.users.clear();
            if let Ok(text) = read_small(&self.root.join("etc/passwd")) {
                for line in text.lines() {
                    let mut f = line.split(':');
                    if let (Some(name), Some(_), Some(uid)) = (f.next(), f.next(), f.next())
                        && let Ok(uid) = uid.parse()
                    {
                        self.users.entry(uid).or_insert_with(|| {
                            name.chars().filter(|c| !c.is_control()).take(64).collect()
                        });
                    }
                }
            }
            self.users_at = Some(now);
        }
        let mut result = ProcessSnapshot {
            sampled_at_ms: now_ms,
            ..Default::default()
        };
        let entries = match fs::read_dir(self.root.join("proc")) {
            Ok(entries) => entries,
            Err(_) => {
                self.reset();
                result.error = Some("Prosesslisten er utilgjengelig".into());
                return result;
            }
        };
        let uid = unsafe { libc::getuid() };
        let euid = unsafe { libc::geteuid() };
        let protected = ancestors(&self.root).ok();
        let mut previous = HashMap::new();
        for entry in entries {
            let Ok(entry) = entry else {
                result.unreadable = result.unreadable.saturating_add(1);
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name
                .to_str()
                .filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
            else {
                continue;
            };
            let Ok(pid) = name.parse::<u32>() else {
                continue;
            };
            let stat = match read_stat(&self.root, pid) {
                Ok(stat) => stat,
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        result.unreadable = result.unreadable.saturating_add(1);
                    }
                    continue;
                }
            };
            let status = read_small(&entry.path().join("status"))
                .ok()
                .and_then(|s| parse_status(&s));
            if status.is_none() {
                result.unreadable = result.unreadable.saturating_add(1);
            }
            // status kan ha blitt lest etter en PID-utskifting. Bland aldri UID
            // fra én instans med stat/CPU/minne fra en annen.
            if read_identity(&self.root, pid) != Some(stat.identity) {
                continue;
            }
            let reason = if self.root != Path::new("/") {
                Some("Avslutning er sperret for testdata".into())
            } else if let Some(ref protected) = protected {
                denial(&stat, status.as_ref(), uid, euid, protected)
            } else {
                Some("Kunne ikke kontrollere egne foreldreprosesser".into())
            };
            let cpu = cpu_percent(
                self.previous.get(&stat.identity).copied(),
                stat.ticks,
                now,
                self.ticks_per_second,
            );
            previous.insert(stat.identity, (stat.ticks, now));
            let real_uid = status.as_ref().map(|s| s.uids[0]);
            result.rows.push(ProcessRow {
                identity: stat.identity,
                ppid: stat.ppid,
                name: stat.name,
                state: stat.state,
                uid: real_uid,
                user: real_uid.and_then(|u| self.users.get(&u).cloned()),
                effective_uid: status.as_ref().map(|s| s.uids[1]),
                rss_bytes: u64::try_from(stat.rss_pages)
                    .ok()
                    .and_then(|pages| pages.checked_mul(self.page_size?)),
                cpu_percent: cpu,
                gpu_vram_bytes: None,
                can_terminate: reason.is_none(),
                protected_reason: reason,
                depth: 0,
            });
        }
        self.previous = previous;
        result.rows.sort_by_key(|r| r.identity.pid);
        result
    }
}

/// Sender én vanlig SIGTERM. Resultatet betyr «signal sendt», ikke at
/// prosessen faktisk er avsluttet. Ingen SIGKILL, prosessgruppe eller fallback.
pub fn terminate(root: &Path, identity: ProcessIdentity) -> Result<(), String> {
    terminate_with_protected(root, identity, &[])
}

pub fn terminate_with_protected(
    root: &Path,
    identity: ProcessIdentity,
    protected_pids: &[u32],
) -> Result<(), String> {
    // Eksakt rot, uten canonicalize: en fixture/symlink skal aldri bli en
    // signalkanal til vertsmaskinen. Testroten må alltid avvises før pidfd_open.
    if root != Path::new("/") {
        return Err("Avslutning er sperret for testdata".into());
    }
    if identity.pid <= 1 || identity.pid > i32::MAX as u32 {
        return Err("Ugyldig eller beskyttet prosess".into());
    }
    let uid = unsafe { libc::getuid() };
    let euid = unsafe { libc::geteuid() };
    if uid == 0 || uid != euid {
        return Err("Avslutning krever en vanlig brukerøkt".into());
    }
    // Før kontroll av /proc åpnes en stabil referanse. Dersom PID-en gjenbrukes
    // etter dette, vil signalet aldri bli sendt til den nye prosessen.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, identity.pid as libc::pid_t, 0u32) };
    if fd < 0 {
        return Err(format!(
            "Kan ikke åpne sikker prosessreferanse: {}",
            io::Error::last_os_error()
        ));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    let protected = protected_ancestors(root, protected_pids)?;
    let stat = read_stat(root, identity.pid)
        .map_err(|_| "Prosessen finnes ikke lenger eller kan ikke leses".to_string())?;
    if stat.identity != identity {
        return Err("Prosessen er erstattet. Oppdater listen og velg på nytt".into());
    }
    let status = read_small(&root.join(format!("proc/{}/status", identity.pid)))
        .ok()
        .and_then(|s| parse_status(&s));
    if let Some(reason) = denial(&stat, status.as_ref(), uid, euid, &protected) {
        return Err(reason);
    }
    if read_identity(root, identity.pid) != Some(identity) {
        return Err("Prosessen er erstattet eller avsluttet".into());
    }
    let sent = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            libc::SIGTERM,
            std::ptr::null::<libc::siginfo_t>(),
            0u32,
        )
    };
    if sent < 0 {
        return Err(format!(
            "Kunne ikke sende SIGTERM: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let n = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("rigg-process-{}-{n}", std::process::id()));
            fs::create_dir_all(dir.join("proc")).unwrap();
            fs::create_dir_all(dir.join("etc")).unwrap();
            fs::write(
                dir.join("etc/passwd"),
                "julian:x:1000:1000:Test:/tmp:/bin/false\n",
            )
            .unwrap();
            Self(dir)
        }
        fn write(&self, pid: u32, start: u64, user_ticks: u64, system_ticks: u64) {
            let dir = self.0.join(format!("proc/{pid}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("stat"),
                stat_text(pid, "arbeid (med) rom)", start, user_ticks, system_ticks),
            )
            .unwrap();
            fs::write(dir.join("status"), "Uid:\t1000\t1000\t1000\t1000\nKthread:\t0\nCapEff:\t0000000000000000\nCapPrm:\t0000000000000000\nCapAmb:\t0000000000000000\n").unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn stat_text(pid: u32, name: &str, start: u64, user: u64, system: u64) -> String {
        // Felt 3..24; cutime/cstime er bevisst store og må ikke medregnes.
        format!(
            "{pid} ({name}) S 1 0 0 0 0 0 0 0 0 0 {user} {system} 999999 888888 20 0 1 0 {start} 409600 7\n"
        )
    }
    #[test]
    fn stat_names_parentheses_and_precise_fields() {
        let stat = parse_stat(&stat_text(42, "a (b) c)\n", 777, 10, 20)).unwrap();
        assert_eq!(
            stat.identity,
            ProcessIdentity {
                pid: 42,
                start_ticks: 777
            }
        );
        assert_eq!(stat.name, "a (b) c) ");
        assert_eq!(stat.ticks, 30);
        assert_eq!(stat.rss_pages, 7);
        assert_eq!(stat.ppid, 1);
        assert!(parse_stat("42 (ab) S 0").is_none());
        assert!(parse_stat(&stat_text(42, "ok", 7, u64::MAX, 1)).is_none());
    }
    #[test]
    fn cpu_monotonic_missing_regression_and_multiple_cores() {
        let t = Instant::now();
        assert_eq!(cpu_percent(None, 10, t, Some(100)), None);
        assert_eq!(
            cpu_percent(Some((10, t)), 210, t + Duration::from_secs(1), Some(100)),
            Some(200.0)
        );
        assert_eq!(
            cpu_percent(Some((10, t)), 5, t + Duration::from_secs(1), Some(100)),
            None
        );
        assert_eq!(cpu_percent(Some((10, t)), 10, t, Some(100)), None);
        assert_eq!(
            cpu_percent(Some((10, t)), 10, t + Duration::from_secs(1), None),
            None
        );
    }
    #[test]
    fn fixture_sampling_reuse_gaps_unknown_uid_and_no_host_fallback() {
        let fixture = Fixture::new();
        fixture.write(42, 10, 20, 30);
        fs::create_dir_all(fixture.0.join("proc/not-a-pid")).unwrap();
        // Et numerisk PID uten stat simulerer at en prosess forsvinner.
        fs::create_dir_all(fixture.0.join("proc/999")).unwrap();
        let mut reader = ProcessReader::new(&fixture.0);
        reader.page_size = Some(4096);
        reader.ticks_per_second = Some(100);
        let t = Instant::now();
        let a = reader.sample_at(1000, t);
        assert_eq!(a.rows.len(), 1);
        assert_eq!(a.rows[0].cpu_percent, None);
        assert_eq!(a.rows[0].rss_bytes, Some(7 * 4096));
        assert_eq!(a.rows[0].user.as_deref(), Some("julian"));
        assert!(!a.rows[0].can_terminate);
        fixture.write(42, 10, 120, 130);
        let b = reader.sample_at(-1000, t + Duration::from_secs(1));
        assert_eq!(
            b.rows[0].cpu_percent,
            Some(200.0),
            "wall-clock reversal does not change CPU rate"
        );
        fixture.write(42, 11, 5, 5);
        assert_eq!(
            reader.sample_at(2000, t + Duration::from_secs(2)).rows[0].cpu_percent,
            None,
            "reused PID"
        );
        fs::remove_file(fixture.0.join("proc/42/status")).unwrap();
        let no_uid = reader.sample_at(3000, t + Duration::from_secs(3));
        assert_eq!(no_uid.rows[0].uid, None);
        assert!(!no_uid.rows[0].can_terminate);
        assert_eq!(no_uid.unreadable, 1);
        fs::remove_dir_all(fixture.0.join("proc/42")).unwrap();
        assert!(
            reader
                .sample_at(4000, t + Duration::from_secs(4))
                .rows
                .is_empty()
        );
        fixture.write(42, 11, 10, 10);
        assert_eq!(
            reader.sample_at(5000, t + Duration::from_secs(5)).rows[0].cpu_percent,
            None,
            "missing sample clears baseline"
        );
        reader.reset();
        assert_eq!(
            reader.sample_at(6000, t + Duration::from_secs(6)).rows[0].cpu_percent,
            None
        );
        fs::remove_dir_all(fixture.0.join("proc")).unwrap();
        assert!(
            reader
                .sample_at(7000, t + Duration::from_secs(7))
                .error
                .is_some()
        );
    }
    #[test]
    fn permission_denied_status_keeps_row_unknown_and_cannot_signal() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        fixture.write(42, 10, 20, 30);
        let status = fixture.0.join("proc/42/status");
        fs::set_permissions(&status, fs::Permissions::from_mode(0o000)).unwrap();
        let mut reader = ProcessReader::new(&fixture.0);
        let sample = reader.sample(1000);
        fs::set_permissions(&status, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(sample.rows.len(), 1);
        assert_eq!(sample.rows[0].uid, None);
        assert_eq!(sample.unreadable, 1);
        assert!(!sample.rows[0].can_terminate);
    }
    #[test]
    fn malformed_or_missing_capabilities_fail_closed() {
        for caps in [
            "",
            "CapEff: invalid\nCapPrm: 0\nCapAmb: 0\n",
            "CapEff: 1\nCapPrm: 0\nCapAmb: 0\n",
        ] {
            let status = parse_status(&format!("Uid: 1000 1000 1000 1000\n{caps}")).unwrap();
            assert!(status.privileged);
        }
    }
    #[test]
    fn fixture_termination_always_denied_before_host_lookup() {
        let fixture = Fixture::new();
        fixture.write(std::process::id(), 1, 0, 0);
        assert!(
            terminate(
                &fixture.0,
                ProcessIdentity {
                    pid: std::process::id(),
                    start_ticks: 1
                }
            )
            .is_err()
        );
    }
    #[test]
    fn protection_rejects_privilege_kernel_zombie_session_and_ancestors() {
        let mut stat = parse_stat(&stat_text(42, "normal", 7, 0, 0)).unwrap();
        let mut status =
            parse_status("Uid: 1000 1000 1000 1000\nKthread: 0\nCapEff: 0\nCapPrm: 0\nCapAmb: 0\n")
                .unwrap();
        let mut protected = HashSet::new();
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_none());
        assert!(denial(&stat, Some(&status), 0, 0, &protected).is_some());
        assert!(denial(&stat, Some(&status), 1000, 0, &protected).is_some());
        assert!(denial(&stat, None, 1000, 1000, &protected).is_some());
        for slot in 0..4 {
            status.uids[slot] = 1001;
            assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
            status.uids[slot] = 1000;
        }
        status.uids[0] = 0;
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
        status.uids[0] = 1000;
        status.privileged = true;
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
        status.privileged = false;
        protected.insert(42);
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
        protected.clear();
        for name in [
            "gnome-shell",
            "systemd",
            "rigg",
            "riggd",
            "Xwayland",
            "gsd-media-keys",
            "agentic-rigg",
            "agentic-riggd",
            "agentic-rigg-he",
            "dbus-broker-lau",
        ] {
            stat.name = name.into();
            assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
        }
        stat.name = "normal".into();
        stat.flags = PF_KTHREAD;
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
        stat.flags = 0;
        stat.state = "Z".into();
        assert!(denial(&stat, Some(&status), 1000, 1000, &protected).is_some());
    }
    struct TestChild(Child);
    impl Drop for TestChild {
        fn drop(&mut self) {
            // Kun prosessen denne testen selv startet; sikrer ingen lekket prøve.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    #[allow(clippy::disallowed_methods)] // Isolert /bin/sleep startet og ryddet av denne testen; ingen maskinvareverktøy.
    fn pidfd_sigterm_own_child_only_rejects_stale_and_protected_identity() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let mut child = TestChild(Command::new("/bin/sleep").arg("20").spawn().unwrap());
        let identity = read_identity(Path::new("/"), child.0.id()).unwrap();
        let protected = protected_ancestors(Path::new("/"), &[identity.pid]).unwrap();
        assert!(protected.contains(&identity.pid));
        assert!(protected.contains(&std::process::id()));
        let stale = ProcessIdentity {
            start_ticks: identity.start_ticks.saturating_add(1),
            ..identity
        };
        assert!(terminate(Path::new("/"), stale).is_err());
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(terminate_with_protected(Path::new("/"), identity, &[identity.pid]).is_err());
        assert!(child.0.try_wait().unwrap().is_none());
        let own = read_identity(Path::new("/"), std::process::id()).unwrap();
        assert!(terminate(Path::new("/"), own).is_err());
        terminate(Path::new("/"), identity).expect("pidfd SIGTERM for isolated test child");
        for _ in 0..100 {
            if let Some(status) = child.0.try_wait().unwrap() {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(status.signal(), Some(libc::SIGTERM));
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("isolated test child did not exit on SIGTERM");
    }
}
