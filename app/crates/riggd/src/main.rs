//! `agentic-riggd`: tjenesten som leser skjermkortet og maskinen, holder
//! historikken og min/maks, og serverer data over en Unix-socket.
//!
//! Kjører som systemd `--user`-tjeneste (`Type=notify`), eller startes av
//! vinduet når systemd mangler. Bare lesing: ingenting skrives til kortet.
//!
//! ```text
//! agentic-riggd [--socket STI] [--version]
//! ```
//! Skjulte valg for tester og skjermbilder: `--backend nvml|sim[:scenario]|ingen`
//! (også `RIGG_BACKEND`), `--grace-ms N`, `--root STI`, `--syntetisk-historikk`,
//! `--hjelper-rot STI` (falsk rot for root-hjelperen, runde 7A).

mod daemon;
mod devices;
mod hjelper;
mod lifecycle;
mod logg;
mod ops;
mod sampler;
mod schedule;
mod server;
mod session;
mod store;
mod tray;
mod varsler;

use rigg_core::paths::{self, Profile};
use rigg_core::proto::GRACE_MS;
use sampler::BackendKind;
use std::path::PathBuf;

const HELP: &str = "Bruk: agentic-riggd [--socket STI] [--version]

Tjenesten til Agentic Rigg. Leser skjermkortet (NVML) og maskinen (/proc og
/sys), holder historikken i 48 timer og serverer dataene over en Unix-socket.
Den startes av appen eller av systemd --user, og skriver aldri til kortet.";

struct Args {
    socket: Option<PathBuf>,
    backend: String,
    grace_ms: u32,
    root: PathBuf,
    synthetic: bool,
    hjelper_rot: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        socket: None,
        backend: std::env::var("RIGG_BACKEND").unwrap_or_else(|_| "nvml".into()),
        grace_ms: GRACE_MS,
        root: PathBuf::from("/"),
        synthetic: false,
        hjelper_rot: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("{}", rigg_core::version::long());
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            "--socket" => a.socket = Some(it.next().ok_or("--socket trenger en sti")?.into()),
            "--backend" => a.backend = it.next().ok_or("--backend trenger et navn")?,
            "--grace-ms" => a.grace_ms = it.next().and_then(|v| v.parse().ok()).ok_or("--grace-ms trenger et tall")?,
            "--root" => a.root = it.next().ok_or("--root trenger en sti")?.into(),
            "--syntetisk-historikk" => a.synthetic = true,
            "--hjelper-rot" => a.hjelper_rot = Some(it.next().ok_or("--hjelper-rot trenger en sti")?.into()),
            other => return Err(format!("Ukjent argument: {other}")),
        }
    }
    Ok(a)
}

fn backend_kind(name: &str) -> Result<BackendKind, String> {
    let (b, scen) = name.split_once(':').unwrap_or((name, ""));
    match b {
        "nvml" => Ok(BackendKind::Nvml),
        "ingen" => Ok(BackendKind::None),
        "sim" => rigg_hw::gpu::sim::Scenario::parse(scen).map(BackendKind::Sim).ok_or_else(|| format!("Ukjent scenario: {scen}")),
        _ => Err(format!("Ukjent backend: {b}")),
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    let kind = match backend_kind(&args.backend) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let profile = Profile::current();
    let socket = args.socket.clone().unwrap_or_else(|| paths::socket_path(profile));
    let config_dir = paths::config_dir(profile);
    let state_dir = paths::state_dir(profile);
    if let Some(dir) = socket.parent()
        && let Err(e) = store::private_dir(dir)
    {
        eprintln!("{e}");
        std::process::exit(1);
    }
    for d in [&config_dir, &state_dir] {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("Kunne ikke lage {}: {e}", d.display());
        }
    }
    let listener = match server::bind(&socket) {
        server::Bind::Ok(l) => l,
        server::Bind::Busy => {
            eprintln!("Tjenesten kjører allerede ({}).", socket.display());
            std::process::exit(0);
        }
        server::Bind::Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let now = rigg_core::now_ms();
    let (tx, rx) = async_channel::unbounded::<sampler::Round>();
    let store = store::Store::new(config_dir, state_dir);
    // Takten før første klient: «Takt i bakgrunnen» (standard 2 s).
    let (svc, _) = store.load_settings(now);
    let interval = svc.background_interval_ms;
    let sampler = sampler::Sampler::start(kind.clone(), args.root.clone(), interval, tx);
    let main_loop = glib::MainLoop::new(None, false);
    let hjelper = hjelper::Hjelper::paths_from(args.hjelper_rot.clone(), profile.is_dev());
    let hete = matches!(kind, sampler::BackendKind::Sim(rigg_hw::gpu::sim::Scenario::Heat));
    let opts = daemon::Options { backend: kind.name().to_string(), root: args.root.clone(), grace_ms: args.grace_ms, hjelper, hete };
    let srv = server::Server::new(socket.clone(), listener);
    let d = daemon::Daemon::new(opts, store, srv, sampler, interval, main_loop.clone(), now);
    if args.synthetic && kind.name() == "sim" {
        rigg_core::history::fill_synthetic(&mut d.borrow_mut().history, now, 48, "0000:1c:00.0");
    }

    // Lytteren.
    {
        let fd = std::os::fd::AsRawFd::as_raw_fd(&d.borrow().server.listener);
        let weak = std::rc::Rc::downgrade(&d);
        let w = glib_unix::unix_fd_add_local(fd, glib::IOCondition::IN, move |_, _| {
            if let Some(d) = weak.upgrade() {
                d.borrow_mut().on_accept();
            }
            glib::ControlFlow::Continue
        });
        d.borrow_mut().server.listen_watch = Some(w);
    }
    // Prøvene fra prøvetråden.
    {
        let weak = std::rc::Rc::downgrade(&d);
        glib::MainContext::default().spawn_local(async move {
            while let Ok(r) = rx.recv().await {
                let Some(d) = weak.upgrade() else { break };
                d.borrow_mut().on_round(r);
            }
        });
    }
    // Signaler: lagre og stopp rent.
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let weak = std::rc::Rc::downgrade(&d);
        glib_unix::unix_signal_add_local(sig, move || {
            if let Some(d) = weak.upgrade() {
                d.borrow_mut().stop("signal");
            }
            glib::ControlFlow::Break
        });
    }
    // Dvale.
    let weak = std::rc::Rc::downgrade(&d);
    let _sleep = lifecycle::watch_sleep(move |going| {
        if let Some(d) = weak.upgrade() {
            d.borrow_mut().on_sleep(going);
        }
    });
    // Systemfeltet: vises bare når tjenesten går i bakgrunnen uten vindu.
    let _tray = tray::attach(&d);
    eprintln!("{} lytter på {} ({}).", rigg_core::version::long(), socket.display(), kind.name());
    main_loop.run();
    eprintln!("Stoppet.");
}
