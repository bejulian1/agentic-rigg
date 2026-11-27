//! `agentic-rigg-helper`: den lille root-hjelperen som endrer skjermkortet
//! for `agentic-riggd`. Se `docs/produksjon/hjelper-sikkerhet.md`.
//!
//! ```text
//! agentic-rigg-helper [--backend nvml|sim] [--socket STI] [--config STI]
//! agentic-rigg-helper --tilbakestill [--backend nvml|sim] [--config STI]
//! agentic-rigg-helper --rgb-identifiser --pci PCI --uuid UUID
//! agentic-rigg-helper --rgb-server
//! ```
//!
//! - Kjører som systemd-systemtjeneste (`agentic-rigg-helper.service`) etter
//!   at brukeren har slått på bryteren i Innstillinger › Styring (pkexec).
//! - Lytter på `/run/agentic-rigg/helper.sock` (`0600`, eies av brukeren) og
//!   godtar bare uid-en i `/etc/agentic-rigg/helper.json` (`SO_PEERCRED`).
//! - Faste operasjoner: `status`, `apply`, `confirm`, `revert`, `reset`
//!   (og `rgb_get`/`rgb_set`, som ikke er laget). Ingen kommandoer, stier
//!   eller adresser kan sendes inn.
//! - `--backend sim` er et simulert kort. `nvml` krever global v2-autorisasjon,
//!   PCI/UUID-binding og varig grunnopptak. Oppdagelsen utfører ingen skriving.
//! - `--tilbakestill` (systemd `ExecStopPost`): gjenopprett fangede grunnverdier
//!   etter egne endringer, også etter et krasj. Uten dirty-markør åpnes ikke GPU.

mod journal;
mod motor;
mod rgb_activation;
mod rgb_backend;
mod rgb_probe;
mod rgb_runtime;
mod rgb_service;
mod rgb_store;
mod server;

use rigg_core::hjelper;
use rigg_hw::gpu::skriv::{self, SimSkriver, Skriver};
use std::path::PathBuf;

const HELP: &str =
    "Bruk: agentic-rigg-helper [--backend nvml|sim] [--socket STI] [--config STI] [--tilbakestill]
      agentic-rigg-helper --rgb-identifiser --pci PCI --uuid UUID
      agentic-rigg-helper --rgb-server
      agentic-rigg-helper --rgb-forbered --pci PCI --uuid UUID --uid UID --static-confirmed
      agentic-rigg-helper --rgb-avklaringsmal --pci PCI --uuid UUID --uid UID
      agentic-rigg-helper --rgb-avklar --pci PCI --uuid UUID --uid UID --static-confirmed --recovery-id ID
      agentic-rigg-helper --rgb-gjenopprett --pci PCI --uuid UUID --uid UID --restore-original
      agentic-rigg-helper --rgb-adapter --pci PCI --uuid UUID --uid UID

Root-hjelperen til Agentic HW Monitor. Startes av systemd etter at du har slått på
«Tillat endring av skjermkortet» i Innstillinger › Styring, og fjernes når du
slår den av. Tar bare imot faste operasjoner fra tjenesten din, og bare for
trinnene du har godkjent.

--rgb-identifiser er en separat, lesende administratorprøve for ett valgt
skjermkort. Den gir JSON uten å starte tjenesten eller endre oppsettet.
--rgb-server starter en separat RGB-tjeneste med egen socket og journal
under den felles korttillatelsen.";

struct Args {
    backend: String,
    socket: PathBuf,
    config: PathBuf,
    reset_only: bool,
    baseline: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let p = hjelper::Paths::new("/");
    let mut a = Args {
        backend: "nvml".into(),
        socket: p.socket(),
        config: p.config(),
        reset_only: false,
        baseline: "/var/lib/agentic-rigg/baseline.json".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!(
                    "Agentic HW Monitor-hjelperen {} · bygg {}",
                    rigg_core::version::VERSION,
                    rigg_core::version::BUILD_STR
                );
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
            "--baseline" => a.baseline = it.next().ok_or("--baseline trenger en sti")?.into(),
            other => return Err(format!("Ukjent argument: {other}")),
        }
    }
    if !matches!(a.backend.as_str(), "nvml" | "sim") {
        return Err(format!("Ukjent backend: {}", a.backend));
    }
    Ok(a)
}

/// Åpning gjør bare oppdagelse og getterkall, aldri en skriveprøve.
fn open_backend(
    name: &str,
    cfg: &hjelper::Config,
    restore_only: bool,
) -> (Option<Box<dyn Skriver>>, &'static str, Option<String>) {
    match name {
        "sim" => (Some(Box::new(SimSkriver::new())), "sim", None),
        _ if cfg.legacy || (!restore_only && (!cfg.enabled || cfg.generation.is_none())) => (
            None,
            "nvml",
            Some("Skjermkortstyring må aktiveres på nytt med global autorisasjon.".into()),
        ),
        _ => match skriv::open_nvml_bound(
            cfg.pci.as_deref().unwrap_or(""),
            cfg.uuid.as_deref().unwrap_or(""),
        ) {
            Ok(h) => (Some(h), "nvml", None),
            Err(e) => (None, "nvml", Some(e)),
        },
    }
}

/// Midlertidig fil for grunnopptaket med tilfeldig navn: en gammel fil fra
/// en tidligere prosess med samme PID skal ikke stoppe et nytt opptak.
/// `create_new` (`O_EXCL`) følger aldri en symlenke og overskriver ingenting.
fn create_staged(parent: &std::path::Path) -> Result<(PathBuf, std::fs::File), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut last = String::new();
    for _ in 0..8 {
        let mut bytes = [0u8; 8];
        // SAFETY: bytes er en gyldig buffer på 8 byte.
        let n = unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) };
        if n != bytes.len() as isize {
            return Err(format!(
                "Tilfeldig filnavn kunne ikke lages: {}",
                std::io::Error::last_os_error()
            ));
        }
        let staged = parent.join(format!(
            ".baseline-{}-{:016x}.tmp",
            std::process::id(),
            u64::from_ne_bytes(bytes)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(&staged)
        {
            Ok(file) => return Ok((staged, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = e.to_string(),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!("Ingen ledig midlertidig grunnverdifil: {last}"))
}

/// Grunnverdiene overlever helperkrasj og er bundet til kort/driver/VBIOS.
/// Et eksisterende, avvikende opptak erstattes aldri automatisk.
fn prepare_baseline(
    h: &mut dyn Skriver,
    path: &std::path::Path,
    create: bool,
) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let baseline = match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let euid = unsafe { libc::geteuid() };
            let parent = path.parent().ok_or("Grunnverdifila mangler mappe")?;
            let dir = std::fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
            if !meta.is_file()
                || meta.uid() != euid
                || meta.mode() & 0o077 != 0
                || meta.len() > 65_536
                || !dir.is_dir()
                || dir.uid() != euid
                || dir.mode() & 0o022 != 0
            {
                return Err(
                    "Grunnverdifila eller mappa har utrygg eier, modus eller filtype.".into(),
                );
            }
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| format!("Ugyldig grunnverdifil: {e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
            let baseline = h.baseline();
            // Vi kan foreløpig bare gjenopprette en fanget, automatisk viftepolicy.
            // Recovery må derimot kunne åpne kortet mens vår egen manuelle
            // innstilling fremdeles er aktiv, så kravet gjelder bare nytt opptak.
            if baseline["captured"]["fan"]["mode"] != "auto" {
                return Err("Nytt grunnverdiopptak krever automatisk viftestyring. Ingen innstillinger er endret.".into());
            }
            let parent = path.parent().ok_or("Grunnverdifila mangler mappe")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let dir = std::fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
            if !dir.is_dir() || dir.uid() != unsafe { libc::geteuid() } || dir.mode() & 0o022 != 0 {
                return Err("Grunnverdimappa har utrygg eier eller modus.".into());
            }
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            // Atomisk, varig publisering uten å overskrive et eksisterende opptak.
            let (staged, mut file) = create_staged(parent)?;
            use std::io::Write;
            let result = (|| -> Result<(), String> {
                file.write_all(
                    serde_json::to_string_pretty(&baseline)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                std::fs::hard_link(&staged, path).map_err(|e| e.to_string())?;
                std::fs::File::open(parent)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())
            })();
            let _ = std::fs::remove_file(&staged);
            result?;
            baseline
        }
        Err(e) => return Err(format!("Grunnverdifila kan ikke leses: {e}")),
    };
    h.load_baseline(&baseline)
}

/// Hvor mange vifter gjenopprettingen må finne igjen. En eldre merkør uten
/// tallet sier ikke hvor mange vifter som kan være endret, og grunnopptaket
/// lagrer ikke antallet: `None`, og merkøren beholdes (feil lukket).
fn marker_fans(marker: Option<&serde_json::Value>) -> Option<usize> {
    marker?["fans"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
}

/// `--tilbakestill`: gjenopprett og bekreft grunnverdiene. Merkøren fjernes
/// bare etter bekreftet standard, også for viftene en tidligere økt kunne
/// endre; ellers beholdes den.
fn recover(
    h: &mut dyn Skriver,
    marker: Option<&serde_json::Value>,
    dirty: &std::path::Path,
) -> Result<(), String> {
    let fans = marker_fans(marker);
    match fans {
        Some(n) if n > h.fan_count() => journal::err(&format!(
            "Kortet melder {} styrbare vifter, men {n} kan være endret. \
             Viftene kan ikke bekreftes på Auto.",
            h.fan_count()
        )),
        Some(_) => {}
        None => journal::err(
            "Merkøren er fra en eldre hjelper uten viftetall. Resten gjenopprettes, \
             men viftene kan ikke bekreftes på Auto.",
        ),
    }
    // Ukjent antall: ingen viftetelling kan bekrefte Auto. Resten skrives.
    h.require_fan_count(fans.unwrap_or(usize::MAX));
    match h.reset() {
        Ok(()) if fans.is_none() => {
            return Err(
                "Eldre gjenopprettingsmerkør uten viftetall: viftene kan ikke bekreftes. \
                 Merkøren beholdes."
                    .into(),
            );
        }
        Ok(()) if h.snapshot()["stock"] == true => {}
        Ok(()) => {
            return Err(
                "Gjenoppretting kunne ikke bekreftes ved tilbakelesing. Merkøren beholdes.".into(),
            );
        }
        Err(e) => return Err(format!("Gjenoppretting feilet: {e}. Merkøren beholdes.")),
    }
    std::fs::remove_file(dirty)
        .and_then(|_| {
            std::fs::File::open(dirty.parent().unwrap_or(std::path::Path::new("/")))
                .and_then(|f| f.sync_all())
        })
        .map_err(|e| format!("Kortet er gjenopprettet, men merkøren kunne ikke fjernes: {e}"))
}

fn sd_notify(state: &str) {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() else {
        return;
    };
    let p = path.to_string_lossy().into_owned();
    let _ = if let Some(abs) = p.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        std::os::unix::net::SocketAddr::from_abstract_name(abs.as_bytes())
            .and_then(|a| sock.send_to_addr(state.as_bytes(), &a))
    } else {
        sock.send_to(state.as_bytes(), &p)
    };
}

fn main() {
    // Den avgrensede RGB-prøven må avsluttes før vanlig tjeneroppsett,
    // NVML-åpning, grunnverdifiler, socket eller recovery kan bli berørt.
    if let Some(code) = rgb_probe::dispatch() {
        std::process::exit(code);
    }
    if let Some(code) = rgb_activation::dispatch() {
        std::process::exit(code);
    }
    if let Some(code) = rgb_service::dispatch() {
        std::process::exit(code);
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    // Samme monotone tidslinje som løkka (CLOCK_BOOTTIME), aldri veggklokka.
    let now = server::boot_ms().unwrap_or(0);
    let dirty = args.baseline.with_file_name("dirty.json");
    if args.reset_only {
        match std::fs::symlink_metadata(&dirty) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                journal::info(
                    "Ingen endringer fra hjelperen er registrert. Ingen GPU-kall utføres.",
                );
                return;
            }
            Err(e) => {
                journal::err(&format!("Gjenopprettingsmerkøren kan ikke undersøkes: {e}"));
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
    let cfg = match server::load_config(&args.config) {
        Ok(c) => c,
        Err(e) => {
            journal::err(&format!("Starter ikke: {e}"));
            std::process::exit(1);
        }
    };
    let (mut hw, backend, mut locked) = open_backend(&args.backend, &cfg, args.reset_only);
    if backend == "nvml"
        && let Some(h) = hw.as_mut()
        && let Err(e) = prepare_baseline(h.as_mut(), &args.baseline, !args.reset_only)
    {
        locked = Some(e);
    }
    if args.reset_only {
        // Et opptak fra et annet kort/driver skal aldri brukes til recovery.
        use std::os::unix::fs::MetadataExt;
        let marker = std::fs::symlink_metadata(&dirty).ok();
        let marker_value = std::fs::read_to_string(&dirty)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        let identity = hw
            .as_ref()
            .map(|h| h.identity())
            .unwrap_or(serde_json::Value::Null);
        if marker.is_none_or(|m| {
            !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0
        }) || marker_value
            .as_ref()
            .is_none_or(|v| v["identity"] != identity)
        {
            journal::err("Gjenopprettingsmerkøren har feil identitet, eier eller modus.");
            std::process::exit(1);
        }
        if let Some(e) = locked {
            journal::err(&format!("Gjenoppretting kan ikke bekreftes: {e}"));
            std::process::exit(1);
        }
        let Some(mut h) = hw else {
            journal::err("Gjenoppretting mangler en identifisert backend.");
            std::process::exit(1);
        };
        match recover(h.as_mut(), marker_value.as_ref(), &dirty) {
            Ok(()) => {
                journal::notice("Kortets grunnverdier er bekreftet gjenopprettet etter stopp.")
            }
            Err(e) => {
                journal::err(&e);
                std::process::exit(1);
            }
        }
        return;
    }
    if dirty.exists() {
        locked = Some("Forrige økt ble ikke gjenopprettet. Kjør en kontrollert gjenoppretting før ny styring.".into());
    }
    let mut m = motor::Motor::new(hw, backend, locked.clone());
    if backend == "nvml" {
        m.set_dirty_marker(dirty);
    }
    m.set_config(Ok(cfg.clone()), now);
    server::install_signals();
    let listener = match server::bind(&args.socket, Some(cfg.uid)) {
        Ok(l) => l,
        Err(e) => {
            journal::err(&format!("Starter ikke: {e}"));
            std::process::exit(1);
        }
    };
    let socket_id = server::socket_identity(&args.socket);
    let enabled = cfg.enabled;
    journal::notice(&format!(
        "agentic-rigg-helper {} (bygg {}) lytter på {} for uid {}. Backend {backend}{}. Global autorisasjon: {}.",
        rigg_core::version::VERSION,
        rigg_core::version::BUILD_STR,
        args.socket.display(),
        cfg.uid,
        locked
            .map(|l| format!(" (sperret: {l})"))
            .unwrap_or_default(),
        if enabled { "på" } else { "av / eldre oppsett" }
    ));
    // En lyttende socket alene er ikke bevis på at NVML er klar.
    if m.hw.is_some() && m.locked.is_none() {
        sd_notify("READY=1");
    } else {
        journal::err("Hjelperen er ikke klar; oppdagelsen har ikke gitt en tilgjengelig backend.");
        if std::env::var_os("NOTIFY_SOCKET").is_some() {
            server::remove_own_socket(&args.socket, socket_id);
            std::process::exit(1);
        }
    }
    let mut srv = server::Server::new(
        listener,
        args.socket.clone(),
        args.config.clone(),
    );
    srv.run(&mut m);
    sd_notify("STOPPING=1");
    if m.hardware_error.is_some() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;
    use rigg_hw::gpu::skriv::SimSkriver;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn vanlig_oppstart_og_recovery_overskriver_aldri_grunnopptaket() {
        let dir = std::env::temp_dir().join(format!("rigg-baseline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("baseline.json");
        let mut first = SimSkriver::new();
        first.applied.power_limit_w = 200.0;
        prepare_baseline(&mut first, &path, true).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let mut restart = SimSkriver::new();
        restart.applied.power_limit_w = 230.0;
        restart.applied.fan_mode = rigg_core::txn::FanMode::Manual;
        prepare_baseline(&mut restart, &path, true).unwrap();
        prepare_baseline(&mut restart, &path, false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        let v: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(v["captured"]["powerLimitW"], 200.0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Et kort som mister viftesøket i recovery, men ellers kan nullstilles.
    struct LostFans {
        fans: usize,
        required: usize,
    }
    impl Skriver for LostFans {
        fn backend(&self) -> &'static str {
            "nvml"
        }
        fn limits(&self) -> skriv::Limits {
            skriv::Limits::rtx2080()
        }
        fn temperature(&mut self) -> Option<f64> {
            Some(45.0)
        }
        fn apply(&mut self, _: &rigg_core::txn::Values) -> Result<(), String> {
            Err("ikke i recovery".into())
        }
        fn set_fan_percent(&mut self, _: f64) -> Result<(), String> {
            Err("ikke i recovery".into())
        }
        fn fan_count(&self) -> usize {
            self.fans
        }
        fn require_fan_count(&mut self, n: usize) {
            self.required = n;
        }
        fn reset(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn snapshot(&self) -> serde_json::Value {
            serde_json::json!({"stock": (self.required <= self.fans).then_some(true)})
        }
    }

    #[test]
    fn recovery_beholder_merkoren_naar_endrede_vifter_ikke_gjenfinnes() {
        let dir = std::env::temp_dir().join(format!("rigg-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dirty = dir.join("dirty.json");
        let marker = |v: serde_json::Value| {
            std::fs::write(&dirty, v.to_string()).unwrap();
            v
        };
        for (value, fans, ok) in [
            // To vifter kunne være endret, ingen ble funnet: behold.
            (serde_json::json!({"version": 1, "fans": 2}), 0, false),
            // Eldre merkør uten tall: ukjent antall, merkøren beholdes alltid,
            // også når ett eller begge av kortets to vifter blir funnet.
            (serde_json::json!({"version": 1}), 0, false),
            (serde_json::json!({"version": 1}), 1, false),
            (serde_json::json!({"version": 1}), 2, false),
            (serde_json::json!({"version": 1, "fans": 2}), 1, false),
            (serde_json::json!({"version": 1, "fans": "2"}), 2, false),
            (serde_json::json!({"version": 1, "fans": 2}), 2, true),
            // Kortet hadde ingen styrbare vifter da endringene startet.
            (serde_json::json!({"version": 1, "fans": 0}), 0, true),
        ] {
            let v = marker(value);
            let mut h = LostFans { fans, required: 0 };
            assert_eq!(recover(&mut h, Some(&v), &dirty).is_ok(), ok, "{v} med {fans} vifter");
            assert_eq!(dirty.exists(), !ok, "{v} med {fans} vifter");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn midlertidig_grunnfil_kolliderer_ikke_med_gammel_pid_fil() {
        let dir = std::env::temp_dir().join(format!("rigg-staged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        // Rest fra en tidligere prosess med samme PID (gammelt navneskjema).
        let stale = dir.join(format!(".baseline-{}.tmp", std::process::id()));
        std::fs::write(&stale, b"gammel").unwrap();
        let path = dir.join("baseline.json");
        prepare_baseline(&mut SimSkriver::new(), &path, true).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&stale).unwrap(), b"gammel", "rest røres ikke");
        let (a, _) = create_staged(&dir).unwrap();
        let (b, _) = create_staged(&dir).unwrap();
        assert_ne!(a, b);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nytt_manuelt_grunnopptak_lagres_ikke() {
        let dir = std::env::temp_dir().join(format!("rigg-manual-baseline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("baseline.json");
        let mut writer = SimSkriver::new();
        writer.applied.fan_mode = rigg_core::txn::FanMode::Manual;
        assert!(prepare_baseline(&mut writer, &path, true).is_err());
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
