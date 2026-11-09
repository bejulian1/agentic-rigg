//! `agentic-rigg-helper`: den lille root-hjelperen som endrer skjermkortet
//! for `agentic-riggd` (runde 7A, 1.2.0). Se `docs/produksjon/hjelper-sikkerhet.md`.
//!
//! ```text
//! agentic-rigg-helper [--backend nvml|sim] [--socket STI] [--config STI]
//! agentic-rigg-helper --tilbakestill [--backend nvml|sim] [--config STI]
//! ```
//!
//! - Kjører som systemd-systemtjeneste (`agentic-rigg-helper.service`) etter
//!   at brukeren har slått på bryteren i Innstillinger › Styring (pkexec).
//! - Lytter på `/run/agentic-rigg/helper.sock` (`0600`, eies av brukeren) og
//!   godtar bare uid-en i `/etc/agentic-rigg/helper.json` (`SO_PEERCRED`).
//! - Faste operasjoner: `status`, `apply`, `confirm`, `revert`, `reset`
//!   (og `rgb_get`/`rgb_set`, som ikke er laget). Ingen kommandoer, stier
//!   eller adresser kan sendes inn.
//! - `--backend sim` er et simulert kort (testene). Med `nvml` er det ekte
//!   kortet sperret i denne versjonen: hjelperen svarer på `status`, men all
//!   skriving gir `write_locked` (`real_writes_off`), og NVML lastes ikke.
//! - `--tilbakestill` (systemd `ExecStopPost`): kortet til standard og ut,
//!   også etter et krasj. Gjør ingenting når ingen trinn er godkjent.

mod journal;
mod motor;
mod server;

use rigg_core::hjelper;
use rigg_hw::gpu::skriv::{self, SimSkriver, Skriver};
use std::path::PathBuf;

const HELP: &str = "Bruk: agentic-rigg-helper [--backend nvml|sim] [--socket STI] [--config STI] [--tilbakestill]

Root-hjelperen til Agentic Rigg. Startes av systemd etter at du har slått på
«Tillat endring av skjermkortet» i Innstillinger › Styring, og fjernes når du
slår den av. Tar bare imot faste operasjoner fra tjenesten din, og bare for
trinnene du har godkjent.";

struct Args {
    backend: String,
    socket: PathBuf,
    config: PathBuf,
    reset_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let p = hjelper::Paths::new("/");
    let mut a = Args { backend: "nvml".into(), socket: p.socket(), config: p.config(), reset_only: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("Agentic Rigg-hjelperen {} · bygg {}", rigg_core::version::VERSION, rigg_core::version::BUILD_STR);
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            "--backend" => a.backend = it.next().ok_or("--backend trenger nvml eller sim")?,
            "--socket" => a.socket = it.next().ok_or("--socket trenger en sti")?.into(),
            "--config" => a.config = it.next().ok_or("--config trenger en sti")?.into(),
            "--tilbakestill" => a.reset_only = true,
            other => return Err(format!("Ukjent argument: {other}")),
        }
    }
    if !matches!(a.backend.as_str(), "nvml" | "sim") {
        return Err(format!("Ukjent backend: {}", a.backend));
    }
    Ok(a)
}

/// Kortet. Med `nvml` er det sperret i 1.2.0 (`skriv::open_nvml` gir feil).
fn open_backend(name: &str) -> (Option<Box<dyn Skriver>>, &'static str, Option<String>) {
    match name {
        "sim" => (Some(Box::new(SimSkriver::new())), "sim", None),
        _ => match skriv::open_nvml() {
            Ok(h) => (Some(h), "nvml", None),
            Err(e) => (None, "nvml", Some(e)),
        },
    }
}

fn sd_notify(state: &str) {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() else { return };
    let p = path.to_string_lossy().into_owned();
    let _ = if let Some(abs) = p.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        std::os::unix::net::SocketAddr::from_abstract_name(abs.as_bytes()).and_then(|a| sock.send_to_addr(state.as_bytes(), &a))
    } else {
        sock.send_to(state.as_bytes(), &p)
    };
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    let now = rigg_core::now_ms();
    let cfg = server::load_config(&args.config);
    if args.reset_only {
        // Bare når noe kan ha vært endret (et trinn godkjent).
        let any = cfg.as_ref().is_ok_and(|(c, _)| !c.steps.is_empty());
        let (hw, _, locked) = open_backend(&args.backend);
        match (any, hw) {
            (true, Some(mut h)) => match h.reset() {
                Ok(()) => journal::notice("Kortet er tilbake på standard (tilbakestill etter stopp)."),
                Err(e) => {
                    journal::err(&format!("Tilbakestill feilet: {e}"));
                    std::process::exit(1);
                }
            },
            (_, None) => journal::info(&format!("Ingenting å tilbakestille: {}", locked.unwrap_or_default())),
            (false, _) => journal::info("Ingenting å tilbakestille: ingen trinn er godkjent."),
        }
        return;
    }
    let (cfg, mtime) = match cfg {
        Ok((c, m)) => (c, m),
        Err(e) => {
            journal::err(&format!("Starter ikke: {e}"));
            std::process::exit(1);
        }
    };
    let (hw, backend, locked) = open_backend(&args.backend);
    let mut m = motor::Motor::new(hw, backend, locked.clone());
    m.set_config(Ok(cfg.clone()), now);
    server::install_signals();
    let listener = match server::bind(&args.socket, Some(cfg.uid)) {
        Ok(l) => l,
        Err(e) => {
            journal::err(&format!("Starter ikke: {e}"));
            std::process::exit(1);
        }
    };
    let steps: Vec<&str> = cfg.steps.iter().map(|s| s.id()).collect();
    journal::notice(&format!(
        "agentic-rigg-helper {} (bygg {}) lytter på {} for uid {}. Backend {backend}{}. Godkjente trinn: {}.",
        rigg_core::version::VERSION,
        rigg_core::version::BUILD_STR,
        args.socket.display(),
        cfg.uid,
        locked.map(|l| format!(" (sperret: {l})")).unwrap_or_default(),
        if steps.is_empty() { "ingen".into() } else { steps.join(", ") }
    ));
    sd_notify("READY=1");
    let mut srv = server::Server::new(listener, args.socket.clone(), args.config.clone(), Some(mtime));
    srv.run(&mut m);
    sd_notify("STOPPING=1");
}
