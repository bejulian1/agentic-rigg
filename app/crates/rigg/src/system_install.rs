//! Systemkomponentene (rothjelperne for GPU-styring og RGB): kontroll av
//! den betrodde installasjonen, og installasjon med ett trykk.
//!
//! Knappen kjører `data/hjelper/system-hent.py` som root via pkexec
//! (polkits generelle passordvindu for `python3`). Skriptteksten og
//! argumentene kommer fra denne appen under ~/.local, så root-handlingen er
//! ikke sikrere enn brukerkontoen som starter den. Skriptet laster ned
//! utgivelsen med appens egen versjon og bygg fra GitHub til en ny rot-eid
//! mappe, kontrollerer SHA-256-summen GitHub oppgir og arkivets identitet,
//! og kjører først da installasjonsprogrammet fra den kontrollerte kopien.
//! Root leser ingen filer fra hjemmemappa. Er appen installert fra et
//! utgivelsesarkiv, sendes også summene for systemfilene i det arkivet
//! (`SYSTEMFILER.sha256`), så en senere utskiftet utgivelse avvises. Et
//! kildebygg har ingen slike summer. Uten nett finnes terminalkommandoen med
//! arkiv og sum oppgitt for hånd.
use crate::state::App;
use crate::toast::Toast;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use rigg_core::{tr, trf};
use std::cell::{Cell, RefCell};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const MARK: &str = "/usr/local/libexec/agentic-rigg-system/revision";
/// Bygget RGB-delen faktisk kom fra. GPU-oppdateringen lar en aktiv eller
/// uavklart RGB-del stå urørt (`--keep-rgb`), så den kan være eldre enn `MARK`.
const RGB_MARK: &str = "/usr/local/libexec/agentic-rigg-system/rgb-revision";
const COMMAND: &str = include_str!("../../../data/hjelper/system-bootstrap.command");
const FETCH: &str = include_str!("../../../data/hjelper/system-hent.py");
const VERSION_JSON: &[u8] = include_bytes!("../../../version.json");
const PKEXEC: &str = "/usr/bin/pkexec";
/// Linjen system-hent.py skriver når en gammel RGB-økt er gjenopprettet og
/// brukeren må se på kortet (samme som `CONFIRM_PREFIX` i skriptet).
const RECOVERY_PREFIX: &str = "AGENTIC-RIGG-RGB-AVKLAR ";
/// Linjen system-hent.py skriver når RGB-delen ble stående urørt under en
/// GPU-oppdatering (samme som `RGB_KEPT_PREFIX` i skriptet).
const RGB_KEPT_PREFIX: &str = "AGENTIC-RIGG-RGB-BEHOLDT ";
const MAX_MANIFEST: u64 = 16 * 1024;

thread_local! {
    /// Installasjonen pågår (passordvinduet, nedlastingen eller installasjonen).
    static BUSY: Cell<bool> = const { Cell::new(false) };
    /// Sider som vil vite når systemkomponentene er installert.
    static LISTENERS: RefCell<Vec<Box<dyn Fn() -> bool>>> = const { RefCell::new(Vec::new()) };
}

pub fn trusted(path: &Path) -> bool {
    path.ancestors().enumerate().all(|(index, part)| {
        std::fs::symlink_metadata(part).is_ok_and(|info| {
            info.uid() == 0
                && info.mode() & 0o022 == 0
                && if index == 0 {
                    info.is_file()
                } else {
                    info.is_dir()
                }
        })
    })
}

/// Hva den installerte revisjonen må dekke.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Need {
    /// Slå av, avklare eller gjenopprette: også eldre installasjoner.
    Cleanup,
    /// Aktivere GPU-styring (NVML-hjelperen).
    Nvml,
    /// Aktivere kortlys (RGB-hjelperen).
    Rgb,
}

pub fn ready(need: Need) -> bool {
    [
        MARK,
        "/usr/local/libexec/agentic-rigg-system/system-install",
        "/usr/local/libexec/agentic-rigg-rgb-v2/rgb-admin.py",
    ]
    .iter()
    .all(|path| trusted(Path::new(path)))
        && std::fs::read_to_string(MARK).is_ok_and(|value| compatible_revision(&value, need))
        && (need != Need::Rgb || rgb_part_ready())
}

/// RGB-delen er fra et RGB-kompatibelt bygg. Uten egen RGB-revisjon (eldre
/// installasjoner) gjelder den felles revisjonen alene.
fn rgb_part_ready() -> bool {
    let path = Path::new(RGB_MARK);
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        _ => trusted(path) && std::fs::read_to_string(path).is_ok_and(|value| compatible_revision(&value, Need::Rgb)),
    }
}

/// NVML og RGB har hver sin kompatibilitetsgrense, så en ny RGB-protokoll
/// gjør ikke en fungerende GPU-styring «utdatert».
fn compatible_revision(value: &str, need: Need) -> bool {
    if value == "1\n" {
        return need == Need::Cleanup;
    }
    revision_build(value).is_some_and(|build| {
        build > 0 && build <= rigg_core::version::build() && match need {
            Need::Cleanup => true,
            Need::Nvml => rigg_core::hjelper::compatible(rigg_core::hjelper::PROTOCOL, build),
            Need::Rgb => rigg_core::rgb_auth::compatible_build(build),
        }
    })
}

fn revision_build(value: &str) -> Option<u32> {
    value.strip_prefix("2:")?.trim_end().parse().ok()
}

fn newer_revision(value: &str) -> bool {
    revision_build(value).is_some_and(|build| build > rigg_core::version::build())
}

pub fn newer_installed() -> bool {
    trusted(Path::new(MARK)) && std::fs::read_to_string(MARK).is_ok_and(|value| newer_revision(&value))
}

/// Pythons faktiske fil (`/usr/bin/python3` er en lenke), bare når den og
/// alle mappene over er eid av root og ikke skrivbare for andre.
fn python() -> Option<PathBuf> {
    std::fs::canonicalize("/usr/bin/python3").ok().filter(|path| path.starts_with("/usr/bin/") && trusted(path))
}

/// Kan appen selv starte installasjonen? Krever betrodd pkexec og python, uten nedgradering.
pub fn can_install() -> bool {
    !newer_installed() && trusted(Path::new(PKEXEC)) && python().is_some()
}

/// Installasjonen pågår.
pub fn busy() -> bool {
    BUSY.with(Cell::get)
}

/// `f` kalles etter en vellykket installasjon. Returnerer `f` `false`
/// (siden finnes ikke lenger), fjernes den.
pub fn on_installed(f: impl Fn() -> bool + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

fn installed() {
    let listeners = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    let kept: Vec<_> = listeners.into_iter().filter(|f| f()).collect();
    LISTENERS.with(|l| {
        let mut l = l.borrow_mut();
        let newer = std::mem::take(&mut *l);
        *l = kept;
        l.extend(newer);
    });
}

/// Reserven uten nett: terminalkommandoen der administratoren oppgir arkivet
/// og summen selv. Vanlig installasjon skjer når brukeren slår på
/// skjermkortstyring eller kortlys ([`run`]).
pub fn offline_fallback() -> gtk::Expander {
    let terminal = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let hint = gtk::Label::new(Some(tr!("Kjør kommandoen i en administratorterminal. Oppgi arkivstien og SHA-256 fra utgivelsessiden.")));
    hint.set_wrap(true);
    hint.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    hint.set_xalign(0.0);
    let command = gtk::Entry::new();
    command.set_editable(false);
    command.set_text(COMMAND.trim());
    let copy = gtk::Button::with_label(tr!("Kopier installasjonskommando"));
    copy.set_halign(gtk::Align::Start);
    copy.connect_clicked(|button| button.clipboard().set_text(COMMAND.trim()));
    terminal.append(&hint);
    terminal.append(&command);
    terminal.append(&copy);
    let expander = gtk::Expander::new(Some(tr!("Uten nett: bruk terminal")));
    expander.set_child(Some(&terminal));
    expander
}

/// Et gammelt RGB-utgangspunkt er skrevet tilbake under oppdateringen.
/// Brukeren må se på kortet før journalen avklares.
pub struct Recovery {
    pub pci: String,
    pub uuid: String,
    pub expected: serde_json::Value,
    pub recovery_id: String,
}

/// Viser avklaringsmålet for brukeren og svarer `true` bare når brukeren har
/// sett og bekreftet nøyaktig det gjenopprettede lyset.
pub type ConfirmRecovery = Box<dyn FnOnce(Recovery, Box<dyn FnOnce(bool)>)>;

/// Hvilken tilgang som slås på rett etter installasjonen, for kortet
/// (PCI-adresse, GPU-UUID). RGB krever at brukeren allerede har bekreftet
/// det faste lyset på kortet, og en eventuell gammel økt avklares først
/// etter en ny bekreftelse av det gjenopprettede lyset.
pub enum Activate {
    Nvml(String, String),
    Rgb(String, String, ConfirmRecovery),
}

/// `version.json`, `bin/NAVN` eller `data/hjelper/NAVN` (som `SYSTEM_PATH`
/// i system-hent.py).
fn system_path(path: &str) -> bool {
    let name = |rest: &str| {
        (1..=64).contains(&rest.len())
            && rest.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    path == "version.json"
        || path.strip_prefix("bin/").is_some_and(name)
        || path.strip_prefix("data/hjelper/").is_some_and(name)
}

/// Kontrollerer `SYSTEMFILER.sha256` fra utgivelsesarkivet og at den hører
/// til nettopp denne appens `version.json`.
fn checked_manifest(text: &str, version_json: &[u8]) -> Result<String, String> {
    let invalid = || tr!("Summene fra utgivelsesarkivet appen ble installert fra, er ugyldige eller hører til en annen versjon. Installer appen på nytt fra utgivelsesarkivet, eller bruk terminalkommandoen.").to_string();
    let body = text.strip_suffix('\n').ok_or_else(invalid)?;
    let mut seen = std::collections::BTreeMap::new();
    for line in body.split('\n') {
        let (digest, path) = line.split_once("  ").ok_or_else(invalid)?;
        let hex = digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex || !system_path(path) || seen.insert(path, digest).is_some() || seen.len() > 64 {
            return Err(invalid());
        }
    }
    let own = glib::compute_checksum_for_data(glib::ChecksumType::Sha256, version_json).ok_or_else(invalid)?;
    if seen.get("version.json") != Some(&own.as_str())
        || !seen.contains_key("bin/agentic-rigg-helper")
        || !seen.contains_key("data/hjelper/system-install")
    {
        return Err(invalid());
    }
    Ok(text.to_owned())
}

/// Summene systeminstallasjonen skal kreve (`-` for et kildebygg uten dem).
fn pinned_manifest() -> Result<String, String> {
    use std::io::Read;
    let path = rigg_core::paths::release_manifest();
    // Ingen symlenke eller FIFO: fila åpnes uten å følge lenker og uten å blokkere.
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path);
    let mut text = String::new();
    match opened {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("-".into()),
        Ok(file) if file.metadata().is_ok_and(|info| info.is_file() && info.len() <= MAX_MANIFEST) => {
            // Ugyldig UTF-8 eller en lesefeil gir tom tekst, som avvises under.
            let _ = file.take(MAX_MANIFEST + 1).read_to_string(&mut text);
        }
        _ => {}
    }
    checked_manifest(&text, VERSION_JSON)
}

/// RGB-delen ble stående urørt under GPU-oppdateringen: kortet den gjelder
/// (hvis kjent) og om en tidligere RGB-økt fortsatt må avklares.
#[derive(Debug, PartialEq, Eq)]
struct RgbKept {
    pci: Option<String>,
    unsettled: bool,
}

fn rgb_kept(line: &str) -> Option<RgbKept> {
    let value: serde_json::Value = serde_json::from_str(line.strip_prefix(RGB_KEPT_PREFIX)?).ok()?;
    let pci = match &value["pci"] {
        serde_json::Value::Null => None,
        serde_json::Value::String(pci) if pci.len() == 12 && pci.bytes().all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.')) => Some(pci.clone()),
        _ => return None,
    };
    Some(RgbKept { pci, unsettled: value["unsettled"].as_bool()? })
}

/// Meldingen etter en GPU-oppdatering som lot RGB-delen stå.
fn rgb_kept_toast(kept: &RgbKept) -> Option<Toast> {
    if !kept.unsettled {
        return None;
    }
    let body = match &kept.pci {
        Some(pci) => trf!("GPU-styringen er oppdatert uten å røre kortlyset. En tidligere RGB-økt for kortet {kort} er fortsatt ikke avsluttet. Velg det kortet og åpne RGB-lys for å avklare den.", kort = pci),
        None => tr!("GPU-styringen er oppdatert uten å røre kortlyset. En tidligere RGB-økt er fortsatt ikke avsluttet. Åpne RGB-lys for å avklare den.").to_string(),
    };
    Some(Toast::new(tr!("RGB-økten må avklares separat")).body(&body).ms(9000))
}

/// Avklaringsmålet fra en linje på stdout, bare for kortet som oppdateres.
fn recovery_request(line: &str, (pci, uuid): (&str, &str)) -> Option<Recovery> {
    let value: serde_json::Value = serde_json::from_str(line.strip_prefix(RECOVERY_PREFIX)?).ok()?;
    let id = value["recoveryId"].as_str()?;
    let valid_id = id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    (valid_id && value["pci"] == pci && value["uuid"] == uuid && value["expected"].is_object()).then(|| Recovery {
        pci: pci.into(),
        uuid: uuid.into(),
        expected: value["expected"].clone(),
        recovery_id: id.into(),
    })
}

fn failure_text(code: i32, err: &str) -> String {
    match code {
        126 => tr!("Passordet ble ikke gitt. Ingenting er endret.").into(),
        127 if err.is_empty() => tr!("Fant ikke pkexec, eller ingen kan spørre om passordet (polkit).").into(),
        _ if err.starts_with("Fikk ikke kontakt med GitHub") || err.starts_with("Nedlastingen fra GitHub feilet") =>
            tr!("Fikk ikke kontakt med GitHub. Sjekk nettet, eller bruk terminalkommandoen.").into(),
        _ if err.starts_with("Utgivelsen v") => tr!("Fant ikke utgivelsen for denne versjonen på GitHub.").into(),
        _ if err.starts_with("Kontrollsummen stemmer ikke") =>
            tr!("Nedlastingen stemmer ikke med summen GitHub oppgir. Ingen systemfiler er endret.").into(),
        _ if err.is_empty() => tr!("Installasjonen feilet. Ingen nye tilganger er gitt.").into(),
        _ => trf!("Installasjonen feilet: {feil}", feil = rigg_core::hjelper::display_diagnostic(err)),
    }
}

/// Installerer systemkomponentene og slår på `activate` med ett passord:
/// `pkexec PYTHON -I -S -c HENT VERSJON BYGG DOMENE PCI UUID`, uten å
/// blokkere vinduet. Feil vises som melding. `done(true)` når både
/// installasjonen og aktiveringen lyktes og sidene er varslet.
pub fn run(app: &Rc<App>, activate: Activate, done: impl FnOnce(bool) + 'static) {
    // Én administratorhandling om gangen: ikke mens GPU-styring eller RGB
    // aktiveres, slås av eller gjenopprettes.
    let other = crate::settings::styring::admin_busy() || crate::pages::lys_kort::admin_busy();
    let Some(python) = python().filter(|_| !newer_installed() && trusted(Path::new(PKEXEC)) && !busy() && !other) else {
        if other {
            app.toast(Toast::new(tr!("Vent til den andre administratorhandlingen er ferdig")).ms(3500));
        }
        done(false);
        return;
    };
    let pins = match pinned_manifest() {
        Ok(pins) => pins,
        Err(error) => {
            app.toast(Toast::new(tr!("Installasjonen ble ikke fullført")).body(&error).error());
            done(false);
            return;
        }
    };
    let (domain, pci, uuid, confirm) = match activate {
        Activate::Nvml(pci, uuid) => ("nvml", pci, uuid, None),
        Activate::Rgb(pci, uuid, confirm) => ("rgb", pci, uuid, Some(confirm)),
    };
    let argv: Vec<std::ffi::OsString> = vec![
        PKEXEC.into(), python.into_os_string(), "-I".into(), "-S".into(), "-c".into(), FETCH.into(),
        rigg_core::version::VERSION.into(), rigg_core::version::BUILD_STR.into(), pins.into(),
        domain.into(), pci.clone().into(), uuid.clone().into(),
    ];
    let refs: Vec<&std::ffi::OsStr> = argv.iter().map(|a| a.as_os_str()).collect();
    // Unntaket fra «ingen programmer» (clippy.toml): brukeren slo selv på
    // styringen eller kortlyset, og pkexec viser passordvinduet fra polkit.
    // Stdin og stdout bærer bare avklaringen av en gammel RGB-økt.
    #[allow(clippy::disallowed_methods)]
    let started = gio::Subprocess::newv(
        &refs,
        gio::SubprocessFlags::STDIN_PIPE | gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    );
    let proc = match started {
        Ok(p) => p,
        Err(e) => {
            app.toast(Toast::new(tr!("Installasjonen ble ikke fullført")).body(&failure_text(127, &e.to_string())).error());
            done(false);
            return;
        }
    };
    BUSY.with(|b| b.set(true));
    crate::settings::request_render();
    let a = app.clone();
    let context = glib::MainContext::default();
    let stderr = proc.stderr_pipe().map(|stream| context.spawn_local(read_tail(stream)));
    glib::MainContext::default().spawn_local(async move {
        let mut stdin = proc.stdin_pipe();
        let mut confirm = confirm;
        let mut kept = None;
        if let Some(stdout) = proc.stdout_pipe() {
            let lines = gio::DataInputStream::new(&stdout);
            while let Ok(Some(line)) = lines.read_line_future(glib::Priority::DEFAULT).await {
                let line = String::from_utf8_lossy(&line);
                if line.starts_with(RGB_KEPT_PREFIX) {
                    kept = rgb_kept(line.trim_end());
                    continue;
                }
                if !line.starts_with(RECOVERY_PREFIX) {
                    continue;
                }
                // Et ugyldig mål eller et annet kort: lukk stdin, så skriptet
                // beholder journalen i stedet for å vente.
                let Some(request) = recovery_request(line.trim_end(), (&pci, &uuid)) else {
                    answer(stdin.take(), None);
                    continue;
                };
                let id = request.recovery_id.clone();
                let pipe = stdin.take();
                let reply: Box<dyn FnOnce(bool)> = Box::new(move |ok| answer(pipe, ok.then_some(id)));
                match confirm.take() {
                    Some(confirm) => confirm(request, reply),
                    None => reply(false),
                }
            }
        }
        drop(stdin);
        let _ = proc.wait_future().await;
        let code = if proc.has_exited() { proc.exit_status() } else { 1 };
        let err = match stderr {
            Some(task) => task.await.unwrap_or_default(),
            None => String::new(),
        };
        let err = err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        BUSY.with(|b| b.set(false));
        if code != 0 {
            let toast = Toast::new(tr!("Installasjonen ble ikke fullført")).body(&failure_text(code, &err));
            a.toast(if code == 126 { toast.ms(3500) } else { toast.error() });
            crate::settings::request_render();
            done(false);
            return;
        }
        installed();
        crate::widgets::locked::refresh_helper(&a, crate::settings::request_render);
        if let Some(toast) = kept.as_ref().and_then(rgb_kept_toast) {
            let b = a.clone();
            a.toast(toast.action(tr!("Åpne RGB-lys"), move || {
                b.shell().close_settings();
                b.go("lys");
            }));
        }
        done(true);
    });
}

/// Svar system-hent.py: gjenopprettingsidentiteten brukeren bekreftet, eller
/// bare lukket stdin (da beholdes journalen og ingenting byttes).
fn answer(stdin: Option<gio::OutputStream>, id: Option<String>) {
    let Some(stdin) = stdin else { return };
    glib::MainContext::default().spawn_local(async move {
        if let Some(id) = id {
            let _ = stdin.write_all_future(format!("{id}\n").into_bytes(), glib::Priority::DEFAULT).await;
        }
        let _ = stdin.close_future(glib::Priority::DEFAULT).await;
    });
}

/// Hele stderr leses (så skriptet aldri blokkerer), og de siste 64 KiB beholdes.
async fn read_tail(stream: gio::InputStream) -> String {
    let mut tail = Vec::new();
    while let Ok(bytes) = stream.read_bytes_future(16 * 1024, glib::Priority::DEFAULT).await {
        if bytes.is_empty() {
            break;
        }
        tail.extend_from_slice(&bytes);
        if tail.len() > 64 * 1024 {
            tail.drain(..tail.len() - 64 * 1024);
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nyere_revisjon_er_ikke_manglende_eller_utdatert() {
        let app_build = rigg_core::version::build();
        let newer = format!("2:{}\n", app_build + 1);
        assert!(newer_revision(&newer));
        for need in [Need::Cleanup, Need::Nvml, Need::Rgb] {
            assert!(!compatible_revision(&newer, need));
        }
        for other in ["".into(), "1\n".into(), "2:24\n".into(), format!("2:{app_build}\n")] {
            assert!(!newer_revision(&other));
        }
    }

    #[test]
    fn gammel_installasjon_tillater_bare_opprydding() {
        for old in ["1\n", "2:24\n"] {
            assert!(!compatible_revision(old, Need::Nvml));
            assert!(!compatible_revision(old, Need::Rgb));
            assert!(compatible_revision(old, Need::Cleanup));
        }
        // NVML-sikkerhetsrettingene i 1.6.0 (bygg 32) gjør 25–31 utdatert for
        // GPU-styring; RGB-protokoll 3 (bygg 29) gjelder bare RGB-delen.
        for old_nvml in ["2:25\n", "2:28\n"] {
            assert!(!compatible_revision(old_nvml, Need::Nvml));
            assert!(!compatible_revision(old_nvml, Need::Rgb));
            assert!(compatible_revision(old_nvml, Need::Cleanup));
        }
        for rgb_only in ["2:29\n", "2:31\n"] {
            assert!(compatible_revision(rgb_only, Need::Rgb));
            assert!(!compatible_revision(rgb_only, Need::Nvml));
        }
        assert!(compatible_revision("2:32\n", Need::Rgb));
        assert!(compatible_revision("2:32\n", Need::Nvml));
        for unknown in ["", "3:25\n", "2:0\n", "2:4294967295\n"] {
            assert!(!compatible_revision(unknown, Need::Cleanup));
        }
    }

    fn manifest(version_json: &[u8]) -> String {
        let own = glib::compute_checksum_for_data(glib::ChecksumType::Sha256, version_json).unwrap();
        format!(
            "{a}  bin/agentic-rigg\n{a}  bin/agentic-rigg-helper\n{a}  data/hjelper/system-install\n{own}  version.json\n",
            a = "a".repeat(64)
        )
    }

    #[test]
    fn festede_summer_maa_hore_til_denne_appen() {
        let good = manifest(VERSION_JSON);
        assert_eq!(checked_manifest(&good, VERSION_JSON).as_deref(), Ok(good.as_str()));
        // En annen versjon, manglende systemfiler, feil format eller farlige stier.
        let other = manifest(b"{\"name\":\"Agentic HW Monitor\",\"version\":\"0.0.1\",\"build\":1}");
        let missing = good.replace("data/hjelper/system-install", "data/hjelper/aktiver.sh");
        let doubled = format!("{good}{}", good.lines().next().unwrap().to_owned() + "\n");
        let upper = good.replacen(&"a".repeat(64), &"A".repeat(64), 1);
        for bad in [
            other.as_str(), missing.as_str(), doubled.as_str(), upper.as_str(), "", "-\n",
            &good.replace("  bin/agentic-rigg\n", "  bin/../agentic-rigg\n"),
            &good.replace("  bin/agentic-rigg\n", " bin/agentic-rigg\n"),
            &good.replace("  bin/agentic-rigg\n", "  data/icons/x.svg\n"),
            good.trim_end(),
        ] {
            assert!(checked_manifest(bad, VERSION_JSON).is_err(), "{bad:?}");
        }
        assert!(system_path("data/hjelper/rgb-admin.py") && system_path("version.json"));
        for path in ["bin/", "bin/a/b", "data/hjelper/../x", "data/x", "/bin/x", "version.json.bak"] {
            assert!(!system_path(path), "{path}");
        }
    }

    #[test]
    fn avklaringsmaal_gjelder_bare_kortet_som_oppdateres() {
        let card = ("0000:1c:00.0", "GPU-12345678-abcd-ef01-2345-6789abcdef01");
        let line = |pci: &str, id: &str, expected: &str| {
            format!("{RECOVERY_PREFIX}{{\"pci\":\"{pci}\",\"uuid\":\"{}\",\"expected\":{expected},\"recoveryId\":\"{id}\"}}", card.1)
        };
        let id = "0123456789abcdef0123456789abcdef";
        let ok = recovery_request(&line(card.0, id, r#"{"mode":"static"}"#), card).unwrap();
        assert_eq!((ok.pci.as_str(), ok.uuid.as_str(), ok.recovery_id.as_str()), (card.0, card.1, id));
        assert_eq!(ok.expected["mode"], "static");
        for bad in [
            line("0000:01:00.0", id, "{}"),
            line(card.0, "../x", "{}"),
            line(card.0, &id.to_uppercase(), "{}"),
            line(card.0, id, "[]"),
            line(card.0, id, "{}").replace(RECOVERY_PREFIX, ""),
            format!("{RECOVERY_PREFIX}ikke json"),
        ] {
            assert!(recovery_request(&bad, card).is_none(), "{bad}");
        }
        // Samme prefiks og samme stimønster som skriptet og pakkeskriptet.
        assert!(FETCH.contains(&format!("CONFIRM_PREFIX = '{RECOVERY_PREFIX}'")));
        assert!(FETCH.contains(&format!("RGB_KEPT_PREFIX = '{RGB_KEPT_PREFIX}'")));
        let pattern = r"bin/[A-Za-z0-9._-]{1,64}|version\.json|data/hjelper/[A-Za-z0-9._-]{1,64}";
        assert!(FETCH.contains(&format!("SYSTEM_PATH = r'{pattern}'")));
        assert!(include_str!("../../../scripts/pakk-utgivelse.py").contains(&format!("re.compile(r\"{pattern}\"")));
    }

    #[test]
    fn beholdt_rgb_del_gir_egen_avklaringsmelding() {
        let line = |json: &str| format!("{RGB_KEPT_PREFIX}{json}");
        assert_eq!(
            rgb_kept(&line(r#"{"pci":"0000:1c:00.0","unsettled":true}"#)),
            Some(RgbKept { pci: Some("0000:1c:00.0".into()), unsettled: true })
        );
        assert_eq!(rgb_kept(&line(r#"{"pci":null,"unsettled":false}"#)), Some(RgbKept { pci: None, unsettled: false }));
        for bad in [r#"{"pci":"../x","unsettled":true}"#, r#"{"pci":"0000:1c:00.0"}"#, "[]", "ikke json"] {
            assert!(rgb_kept(&line(bad)).is_none(), "{bad}");
        }
        assert!(rgb_kept(r#"{"pci":null,"unsettled":true}"#).is_none());
        // Bare en uavklart økt gir egen melding; en avklart RGB-del er bare urørt.
        assert!(rgb_kept_toast(&RgbKept { pci: None, unsettled: false }).is_none());
        assert!(rgb_kept_toast(&RgbKept { pci: Some("0000:1c:00.0".into()), unsettled: true }).is_some());
    }

    /// Argumentene appen sender passer skriptets strenge kontroll helt frem
    /// til root-kravet. Kjøres aldri som root, og aldri via pkexec.
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn hentingen_godtar_appens_argumenter_og_krever_root() {
        if unsafe { libc::geteuid() } == 0 || python().is_none() {
            return;
        }
        for pins in ["-".to_string(), manifest(VERSION_JSON)] {
            let out = std::process::Command::new(python().unwrap())
                .args(["-I", "-S", "-c", FETCH, rigg_core::version::VERSION, rigg_core::version::BUILD_STR, &pins])
                .args(["rgb", "0000:1c:00.0", "GPU-12345678-abcd-ef01-2345-6789abcdef01"])
                .env_clear()
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "Systeminstallasjon krever administrator.");
            assert!(out.stdout.is_empty());
        }
    }

    /// Hentingen ber om nøyaktig arkivet pakkeskriptet lager for denne
    /// versjonen, og versjonen passer skriptets strenge argumentkontroll.
    #[test]
    fn hentingen_passer_pakken_og_versjonen() {
        let pack = include_str!("../../../scripts/pakk-utgivelse.py");
        assert!(pack.contains("ubuntu26.04-amd64.tar.gz"));
        assert!(FETCH.contains("ASSET = 'agentic-rigg-{version}-build{build}-ubuntu26.04-amd64.tar.gz'"));
        assert!(FETCH.contains("REPO = 'bejulian1/agentic-rigg'"));
        let parts: Vec<_> = rigg_core::version::VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit())));
        assert!(rigg_core::version::build() > 0);
        assert!(FETCH.contains("if __name__ == '__main__':"));
    }
}
