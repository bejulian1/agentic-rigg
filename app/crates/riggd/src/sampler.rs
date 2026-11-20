//! Prøvetråden: eier NVML og de åpne filene i `/proc` og `/sys`, tar én
//! prøve per tikk og sender den til hovedtråden.
//!
//! Regler som ikke kan brytes (arkitektur.md 4.3):
//! - **Aldri raskere enn 2 Hz**, og **aldri ta igjen** tapte runder. Fast takt
//!   på absolutt tid; en runde som kommer for sent, hoppes over.
//! - NVML kalles bare her, aldri på hovedtråden.
//! - Panikk i tråden fanges; backenden lages på nytt etter 1 s, og hovedtråden
//!   fortsetter (arkitektur.md 4.6).

use rigg_core::tr;
use rigg_core::model::{GpuInfo, GpuSample, GpuStatic, SysSample};
use rigg_core::proto::MIN_INTERVAL_MS;
use rigg_hw::gpu::{GpuBackend, Plan};
use rigg_hw::sys::SysReader;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Hvilken backend tråden skal bruke.
#[derive(Clone, Debug)]
pub enum BackendKind {
    Nvml,
    Sim(rigg_hw::gpu::sim::Scenario),
    None,
}

impl BackendKind {
    pub fn name(&self) -> &'static str {
        match self {
            BackendKind::Nvml => "nvml",
            BackendKind::Sim(_) => "sim",
            BackendKind::None => "ingen",
        }
    }
}

/// Fra hovedtråden til prøvetråden.
pub enum Cmd {
    Interval(u32),
    /// Dvale eller ingen grunn til å ta prøver.
    Pause(bool),
    /// Prosesslista (`get_processes`): les den hvert 5. s så lenge den er på.
    Processes(bool),
    /// Oppgaver leses bare mens en synlig side fornyer fristen.
    Tasks {
        generation: u64,
        until: Instant,
    },
    /// Et vindu viser verdiene eller loggen går ([`crate::schedule::detailed`]):
    /// de trege verdiene hvert 2. og 5. s. Ellers hvert 10. og 30. s.
    Detailed(bool),
    /// Lag disklista (UDisks2, partisjoner) i neste runde (`get_disks`).
    Disks,
    /// Diskvarselet er på: les SMART hvert 10. min også uten vindu.
    DiskAlarm(bool),
    Stop,
}

/// Fra prøvetråden til hovedtråden.
pub struct Round {
    pub t: i64,
    pub interval_ms: u32,
    pub gpus: BTreeMap<String, GpuSample>,
    pub sys: SysSample,
    pub infos: Vec<GpuInfo>,
    /// Faste data som er nye eller endret siden forrige runde.
    pub statics: Vec<(GpuStatic, bool)>,
    /// Prosesslista per kort, når den er bedt om og lest denne runden.
    pub processes: Vec<(String, serde_json::Value)>,
    pub tasks: Option<(u64, crate::tasks::TaskSnapshot)>,
    /// Disklista (`get_disks`), når den er laget denne runden.
    pub disks: Option<rigg_core::disk::DiskList>,
    /// Tid brukt på runden (feilsøking, `maal.sh`).
    pub cost_us: u64,
}

pub struct Sampler {
    pub tx: mpsc::Sender<Cmd>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn make_backend(kind: &BackendKind, root: &std::path::Path) -> Box<dyn GpuBackend> {
    let now = rigg_core::now_ms();
    match kind {
        BackendKind::Nvml => Box::new(rigg_hw::gpu::nvml::Nvml::new(root.to_path_buf(), now)),
        BackendKind::Sim(s) => Box::new(rigg_hw::gpu::sim::Sim::new(*s, now)),
        BackendKind::None => Box::new(rigg_hw::gpu::NoGpu),
    }
}

impl Sampler {
    pub fn start(kind: BackendKind, root: PathBuf, interval_ms: u32, out: async_channel::Sender<Round>) -> Sampler {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let handle = std::thread::Builder::new()
            .name("rigg-prøver".into())
            .spawn(move || {
                let mut st = State {
                    interval: interval_ms.max(MIN_INTERVAL_MS),
                    paused: false,
                    processes: false,
                    tasks: None,
                    detailed: false,
                    disk_alarm: false,
                };
                loop {
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&kind, &root, &rx, &out, &mut st)));
                    match res {
                        Ok(()) => break,
                        Err(_) => {
                            eprintln!("{}", tr!("Prøvetråden fikk panikk. Starter den på nytt om 1 s."));
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
            })
            .expect(tr!("kunne ikke starte prøvetråden"));
        Sampler { tx, handle: Some(handle) }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    /// Stopper tråden og venter til NVML er lukket.
    pub fn stop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Det tråden husker over en panikk.
struct State {
    interval: u32,
    paused: bool,
    processes: bool,
    tasks: Option<(u64, Instant)>,
    detailed: bool,
    disk_alarm: bool,
}

/// Hvor ofte disklista (UDisks2-egenskapene, partisjonene) lages: hvert 60. s
/// med vindu eller logg, hvert 10. min i bakgrunnen når diskvarselet er på,
/// ellers ikke (runde 6A). UDisks2 leser selv SMART omtrent hvert 10. min.
pub fn disk_period(detailed: bool, alarm: bool) -> Option<i64> {
    if detailed {
        Some(60_000)
    } else if alarm {
        Some(600_000)
    } else {
        None
    }
}

/// Diskene: simulert med `--backend sim` (eller `RIGG_DISKER=sim`), ellers de
/// ekte. `RIGG_DISKER=ingen` slår dem av (målingene i runde 6A sammenligner).
fn make_disks(kind: &BackendKind, root: &std::path::Path) -> rigg_hw::sys::disk::DiskReader {
    let env = std::env::var("RIGG_DISKER").unwrap_or_default();
    let sim = env == "sim" || matches!(kind, BackendKind::Sim(_)) && env != "ekte" && env != "ingen";
    if sim {
        rigg_hw::sys::disk::DiskReader::sim(rigg_core::now_ms())
    } else if env == "ingen" {
        rigg_hw::sys::disk::DiskReader::off()
    } else {
        rigg_hw::sys::disk::DiskReader::new(root)
    }
}

/// Hvor ofte de trege verdiene leses (ms): (hvert «2.», hvert «5.») med og uten vindu.
pub fn slow_periods(detailed: bool) -> (i64, i64) {
    if detailed { (2_000, 5_000) } else { (10_000, 30_000) }
}

/// Prosesslista leses høyst så ofte (ms).
const PROCESSES_MS: i64 = 5_000;

fn run(kind: &BackendKind, root: &std::path::Path, rx: &mpsc::Receiver<Cmd>, out: &async_channel::Sender<Round>, st: &mut State) {
    let State {
        interval,
        paused,
        processes: want_proc,
        tasks: task_demand,
        detailed,
        disk_alarm,
    } = st;
    let mut last_proc = i64::MIN / 2;
    let mut last_task: Option<Instant> = None;
    let mut task_reader = rigg_hw::sys::process::ProcessReader::new(root);
    let mut backend = make_backend(kind, root);
    let mut sys = SysReader::new(root);
    let mut disks = make_disks(kind, root);
    let mut udisks = rigg_hw::sys::udisks::Udisks::new();
    let mut last_disks = i64::MIN / 2;
    let mut want_disks = false;
    let start = Instant::now();
    let mut next = start;
    let mut first = true;
    let (mut last2, mut last5, mut last60, mut last_check) = (i64::MIN / 2, i64::MIN / 2, i64::MIN / 2, i64::MIN / 2);
    let mut sent_static: BTreeMap<String, bool> = BTreeMap::new();
    loop {
        // Vent til neste tikk, men svar på kommandoer underveis.
        loop {
            let now = Instant::now();
            let wait = if *paused { Duration::from_secs(3600) } else { next.saturating_duration_since(now) };
            if wait.is_zero() {
                break;
            }
            match rx.recv_timeout(wait) {
                Ok(Cmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    backend.shutdown();
                    return;
                }
                Ok(Cmd::Interval(ms)) => {
                    let ms = ms.max(MIN_INTERVAL_MS);
                    if ms != *interval {
                        // Raskere takt: neste runde tidligst om den nye takten etter forrige.
                        let prev = next.checked_sub(Duration::from_millis(*interval as u64)).unwrap_or(next);
                        *interval = ms;
                        next = (prev + Duration::from_millis(ms as u64)).max(Instant::now());
                    }
                }
                Ok(Cmd::Processes(on)) => {
                    if on && !*want_proc {
                        // Første forespørsel: les lista i neste runde.
                        last_proc = i64::MIN / 2;
                    }
                    *want_proc = on;
                }
                Ok(Cmd::Tasks { generation, until }) => {
                    if task_demand.is_none_or(|(previous, deadline)| previous != generation || Instant::now() >= deadline) {
                        task_reader.reset();
                        last_task = None;
                    }
                    *task_demand = Some((generation, until));
                }
                Ok(Cmd::Detailed(on)) => {
                    if on && !*detailed {
                        // Vinduet kom fram: les alt i neste runde, så tallene er ferske.
                        last2 = i64::MIN / 2;
                        last5 = i64::MIN / 2;
                        last_disks = i64::MIN / 2;
                    }
                    *detailed = on;
                }
                Ok(Cmd::Disks) => want_disks = true,
                Ok(Cmd::DiskAlarm(on)) => *disk_alarm = on,
                Ok(Cmd::Pause(p)) => {
                    // Dvalen må ikke bli del av neste CPU-rate.
                    task_reader.reset();
                    last_task = None;
                    if *paused && !p {
                        // Etter oppvåkning: 2 s pause før første runde.
                        next = Instant::now() + Duration::from_secs(2);
                        first = true;
                    }
                    *paused = p;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        let t0 = Instant::now();
        if task_demand.is_some_and(|(_, until)| t0 >= until) {
            *task_demand = None;
            task_reader.reset();
            last_task = None;
        }
        let now = rigg_core::now_ms();
        if now - last_check >= 10_000 {
            last_check = now;
            backend.check(now);
        }
        let (p2, p5) = slow_periods(*detailed);
        let plan = Plan {
            now_ms: now,
            every2: now - last2 >= p2 - 50,
            every5: now - last5 >= p5 - 50,
            every60: now - last60 >= 59_950,
            first,
            watched: *detailed,
        };
        if plan.every2 {
            last2 = now;
        }
        if plan.every5 {
            last5 = now;
        }
        if plan.every60 {
            last60 = now;
        }
        let r = backend.sample(&plan);
        let mut sys_sample = sys.sample(now);
        // Disklista før prøven, så temperaturen i prøven er fersk.
        let mut disk_list = None;
        if want_disks || disk_period(*detailed, *disk_alarm).is_some_and(|p| now - last_disks >= p - 50) {
            last_disks = now;
            let table = *detailed || want_disks;
            want_disks = false;
            let drives = if disks.is_sim() || disks.is_off() { Ok(vec![]) } else { udisks.drives(table) };
            disk_list = Some(disks.list(now, drives.as_deref().map_err(String::as_str)));
        }
        sys_sample.disks = disks.sample(now);
        let infos = backend.gpus();
        let mut statics = Vec::new();
        for g in &infos {
            if g.id.is_empty() || sent_static.get(&g.id) == Some(&true) {
                continue;
            }
            if let Some((st, done)) = backend.static_info(&g.id) {
                sent_static.insert(g.id.clone(), done);
                statics.push((st, done));
            }
        }
        let tasks_due = task_demand.is_some() && last_task.is_none_or(|last| t0.saturating_duration_since(last) >= Duration::from_millis((crate::tasks::TASK_INTERVAL_MS - 50) as u64));
        // Les identiteten før NVML, og bekreft den igjen ved sammenkobling.
        let task_processes = tasks_due.then(|| task_reader.sample(now));
        let mut processes = Vec::new();
        if tasks_due || *want_proc && now - last_proc >= PROCESSES_MS - 50 {
            last_proc = now;
            for g in &infos {
                if let Some(p) = (!g.id.is_empty()).then(|| backend.processes(&g.id)).flatten() {
                    processes.push((g.id.clone(), p));
                }
            }
        }
        let tasks = task_processes.map(|snapshot| {
            last_task = Some(t0);
            let snapshot = crate::tasks::with_gpu(
                snapshot,
                &processes,
                matches!(kind, BackendKind::Nvml),
                infos.iter().filter(|gpu| !gpu.id.is_empty()).count(),
                root,
                t0,
                rigg_core::now_ms(),
            );
            (task_demand.as_ref().expect("etterspørsel kontrollert over").0, snapshot)
        });
        let cost_us = t0.elapsed().as_micros() as u64;
        let round = Round {
            t: now,
            interval_ms: *interval,
            gpus: r.samples,
            sys: sys_sample,
            infos,
            statics,
            processes,
            tasks,
            disks: disk_list,
            cost_us,
        };
        if out.send_blocking(round).is_err() {
            backend.shutdown();
            return;
        }
        first = false;
        // Fast takt. Kom vi for sent, hoppes de tapte rundene over (ingen etterslep).
        next += Duration::from_millis(*interval as u64);
        let after = Instant::now();
        if next <= after {
            let iv = Duration::from_millis(*interval as u64);
            while next <= after {
                next += iv;
            }
        }
    }
}
