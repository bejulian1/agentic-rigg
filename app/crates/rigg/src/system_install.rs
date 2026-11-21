//! Systemkomponentene (rothjelperne for GPU-styring og RGB): kontroll av
//! den betrodde installasjonen, og installasjon med ett trykk.
//!
//! Knappen kjører `data/hjelper/system-hent.py` som root via pkexec
//! (polkits passordvindu). Skriptet laster ned utgivelsen med appens egen
//! versjon og bygg fra GitHub til en ny rot-eid mappe, kontrollerer den mot
//! SHA-256-summen GitHub publiserer for filen, og kjører først da
//! installasjonsprogrammet fra den kontrollerte kopien. Ingenting fra
//! hjemmemappa kjøres som root, og brukeren trenger ikke lime inn summen.
//! Uten nett finnes terminalkommandoen med arkiv og sum oppgitt for hånd.
use crate::state::App;
use crate::toast::Toast;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use rigg_core::{tr, trf};
use std::cell::{Cell, RefCell};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const MARK: &str = "/usr/local/libexec/agentic-rigg-system/revision";
const COMMAND: &str = include_str!("../../../data/hjelper/system-bootstrap.command");
const FETCH: &str = include_str!("../../../data/hjelper/system-hent.py");
const PKEXEC: &str = "/usr/bin/pkexec";

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

pub fn ready(activate: bool) -> bool {
    [
        MARK,
        "/usr/local/libexec/agentic-rigg-system/system-install",
        "/usr/local/libexec/agentic-rigg-rgb-v2/rgb-admin.py",
    ]
    .iter()
    .all(|path| trusted(Path::new(path)))
        && std::fs::read_to_string(MARK).is_ok_and(|value| compatible_revision(&value, activate))
}

fn compatible_revision(value: &str, activate: bool) -> bool {
    if value == "1\n" {
        return !activate;
    }
    value.strip_prefix("2:").and_then(|v| v.trim_end().parse::<u32>().ok()).is_some_and(|build| {
        build > 0 && build <= rigg_core::version::build() && (!activate ||
            (rigg_core::hjelper::compatible(rigg_core::hjelper::PROTOCOL, build)
             && rigg_core::rgb_auth::compatible_build(build)))
    })
}

/// Pythons faktiske fil (`/usr/bin/python3` er en lenke), bare når den og
/// alle mappene over er eid av root og ikke skrivbare for andre.
fn python() -> Option<PathBuf> {
    std::fs::canonicalize("/usr/bin/python3").ok().filter(|path| path.starts_with("/usr/bin/") && trusted(path))
}

/// Kan appen selv starte installasjonen? Krever betrodd pkexec og python.
pub fn can_install() -> bool {
    trusted(Path::new(PKEXEC)) && python().is_some()
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

/// Hvilken tilgang som slås på rett etter installasjonen, for kortet
/// (PCI-adresse, GPU-UUID). RGB krever at brukeren allerede har bekreftet
/// det faste lyset på kortet.
pub enum Activate {
    Nvml(String, String),
    Rgb(String, String),
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
    let Some(python) = python().filter(|_| trusted(Path::new(PKEXEC)) && !busy() && !other) else {
        if other {
            app.toast(Toast::new(tr!("Vent til den andre administratorhandlingen er ferdig")).ms(3500));
        }
        done(false);
        return;
    };
    let (domain, pci, uuid) = match activate {
        Activate::Nvml(pci, uuid) => ("nvml", pci, uuid),
        Activate::Rgb(pci, uuid) => ("rgb", pci, uuid),
    };
    let argv: Vec<std::ffi::OsString> = vec![
        PKEXEC.into(), python.into_os_string(), "-I".into(), "-S".into(), "-c".into(), FETCH.into(),
        rigg_core::version::VERSION.into(), rigg_core::version::BUILD_STR.into(),
        domain.into(), pci.into(), uuid.into(),
    ];
    let refs: Vec<&std::ffi::OsStr> = argv.iter().map(|a| a.as_os_str()).collect();
    // Unntaket fra «ingen programmer» (clippy.toml): brukeren slo selv på
    // styringen eller kortlyset, og pkexec viser passordvinduet fra polkit.
    #[allow(clippy::disallowed_methods)]
    let started = gio::Subprocess::newv(&refs, gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_PIPE);
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
    glib::MainContext::default().spawn_local(async move {
        let (_, err) = proc.communicate_utf8_future(None).await.unwrap_or((None, None));
        let code = if proc.has_exited() { proc.exit_status() } else { 1 };
        let err = err.map(|s| s.to_string()).unwrap_or_default();
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
        done(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gammel_installasjon_tillater_bare_opprydding() {
        for old in ["1\n", "2:24\n"] {
            assert!(!compatible_revision(old, true));
            assert!(compatible_revision(old, false));
        }
        assert!(compatible_revision("2:25\n", true));
        for unknown in ["", "3:25\n", "2:0\n", "2:4294967295\n"] {
            assert!(!compatible_revision(unknown, false));
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
