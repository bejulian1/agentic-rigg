//! Systemkomponentene (rothjelperne for GPU-styring og RGB): kontroll av
//! den betrodde installasjonen, og installasjon fra appen uten terminal.
//!
//! Appen kjører det samme bootstrap-skriptet som terminalkommandoen, bare
//! gjennom pkexec (polkits passordvindu) i stedet for sudo. Skriptet kopierer
//! arkivet til en ny rot-eid mappe, kontrollerer det mot SHA-256 som brukeren
//! limer inn fra utgivelsessiden, og kjører først da installasjonsprogrammet
//! fra den kontrollerte kopien. Ingenting fra hjemmemappa kjøres som root.
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{button, hbox, label, vbox};
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
const PKEXEC: &str = "/usr/bin/pkexec";
/// `/bin` og `/usr/bin/env` er lenker på Ubuntu 26.04; bash er en vanlig fil.
const BASH: &str = "/usr/bin/bash";
/// Som `env -i PATH=/usr/bin:/bin` i terminalkommandoen: pkexec slipper
/// gjennom språk-, skjerm- og terminalvariabler, så de fjernes her.
const CLEAN_ENV: &str = "unset LANG LANGUAGE DISPLAY XAUTHORITY SHELL TERM; export PATH=/usr/bin:/bin LC_ALL=C;";
const RELEASES: &str = "https://github.com/bejulian1/agentic-rigg/releases";

thread_local! {
    /// Installasjonen venter på pkexec (passordvinduet er oppe).
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

/// Skriptet inne i terminalkommandoen (`bash -ceu '…'`), uten skallsitering.
/// Én kilde: appen og terminalkommandoen kjører nøyaktig det samme.
fn bootstrap_script() -> Option<String> {
    let start = COMMAND.find("-ceu '")? + "-ceu '".len();
    let end = start + COMMAND[start..].find("' rigg \"")?;
    Some(COMMAND[start..end].replace(r"'\''", "'"))
}

fn valid_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Summen slik GitHub viser den (`sha256:…`) eller bare heksadesimalt.
fn normalize_digest(text: &str) -> String {
    let text = text.trim();
    text.strip_prefix("sha256:").unwrap_or(text).trim().to_ascii_lowercase()
}

/// Navnet på utgivelsesarkivet for denne appversjonen, uten plattformdelen.
fn archive_prefix() -> String {
    format!("agentic-rigg-{}-build{}-", rigg_core::version::VERSION, rigg_core::version::BUILD_STR)
}

/// Arkivet for denne versjonen i Nedlastinger, hvis det ligger der.
fn downloaded_archive() -> Option<PathBuf> {
    let dir = glib::user_special_dir(glib::UserDirectory::Downloads)?;
    let prefix = archive_prefix();
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.path())
        .filter(|path| path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".tar.gz")))
        .collect();
    found.sort();
    found.pop()
}

/// Kan appen selv starte installasjonen? Krever betrodd pkexec og bash.
pub fn can_install() -> bool {
    bootstrap_script().is_some()
        && [PKEXEC, BASH].iter().all(|path| trusted(Path::new(path)))
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

/// Melding og knapp når systemkomponentene mangler eller er utdaterte.
/// Knappen åpner [`show_dialog`]; terminalkommandoen ligger under «Bruk terminal».
pub fn prompt(app: &Rc<App>) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let text = gtk::Label::new(Some(
        tr!("Systemkomponentene mangler eller må oppdateres. Installer dem fra utgivelsesarkivet. Du blir spurt om administratorpassordet."),
    ));
    text.set_wrap(true);
    text.set_xalign(0.0);
    box_.append(&text);
    let install = button(Some("download"), Some(tr!("Installer systemkomponenter …")), &["suggested"]);
    install.set_halign(gtk::Align::Start);
    install.set_sensitive(can_install() && !BUSY.with(Cell::get));
    if !can_install() {
        install.set_tooltip_text(Some(tr!("Fant ikke pkexec. Bruk terminalkommandoen.")));
    }
    {
        let a = app.clone();
        install.connect_clicked(move |_| {
            let b = a.clone();
            show_dialog(&a, move |ok| if ok {
                b.toast(Toast::new(tr!("Systemkomponentene er installert")).body(tr!("Slå på GPU-styring eller kortlys når du vil bruke dem.")).ms(5000));
            });
        });
    }
    box_.append(&install);

    let terminal = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let hint = gtk::Label::new(Some(tr!("Kjør kommandoen i en administratorterminal. Oppgi arkivstien og SHA-256 fra utgivelsessiden.")));
    hint.set_wrap(true);
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
    let expander = gtk::Expander::new(Some(tr!("Bruk terminal i stedet")));
    expander.set_child(Some(&terminal));
    box_.append(&expander);
    box_
}

/// Installasjonsvinduet: velg arkivet, lim inn SHA-256 og installer.
/// `done(true)` etter vellykket installasjon, `done(false)` ellers.
pub fn show_dialog(app: &Rc<App>, done: impl FnOnce(bool) + 'static) {
    if BUSY.with(Cell::get) || !can_install() {
        done(false);
        return;
    }
    let Some((win, root)) = crate::shortcuts::dialog(app, tr!("Installer systemkomponenter"), 560, 460) else {
        done(false);
        return;
    };
    type Done = Rc<RefCell<Option<Box<dyn FnOnce(bool)>>>>;
    let done: Done = Rc::new(RefCell::new(Some(Box::new(done))));
    let finish = {
        let done = done.clone();
        move |ok: bool| {
            if let Some(f) = done.borrow_mut().take() {
                f(ok);
            }
        }
    };
    {
        let finish = finish.clone();
        win.connect_close_request(move |_| {
            if BUSY.with(Cell::get) {
                return glib::Propagation::Stop;
            }
            finish(false);
            glib::Propagation::Proceed
        });
    }

    let body = vbox(12, &["dialog-body"]);
    let wrap = |text: &str, classes: &[&str]| {
        let l = label(text, classes);
        l.set_wrap(true);
        l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        l.set_xalign(0.0);
        l
    };
    body.append(&wrap(tr!("Rothjelperne for GPU-styring og RGB-lys installeres fra utgivelsesarkivet. Arkivet kopieres til en rot-eid mappe og kontrolleres mot SHA-256 fra utgivelsessiden før noe kjøres som administrator."), &[]));
    body.append(&wrap(tr!("Installasjonen gir ingen tilgang i seg selv. Du slår på GPU-styring eller kortlys etterpå, med eget passord."), &["muted"]));

    // ---- Arkivet ----
    let chosen: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(downloaded_archive()));
    body.append(&wrap(tr!("Utgivelsesarkiv"), &["strong"]));
    let file_row = hbox(8, &[]);
    let file_label = label("", &[]);
    file_label.set_hexpand(true);
    file_label.set_xalign(0.0);
    let show_file = {
        let file_label = file_label.clone();
        let chosen = chosen.clone();
        move || match chosen.borrow().as_ref() {
            Some(path) => {
                file_label.add_css_class("mono");
                file_label.set_wrap(false);
                file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                file_label.set_text(&rigg_core::paths::tilde(path));
                file_label.set_tooltip_text(Some(&path.display().to_string()));
            }
            None => {
                file_label.remove_css_class("mono");
                file_label.set_ellipsize(gtk::pango::EllipsizeMode::None);
                file_label.set_wrap(true);
                file_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                file_label.set_text(&trf!("Ikke valgt. Last ned {navn}….tar.gz fra utgivelsessiden.", navn = archive_prefix()));
                file_label.set_tooltip_text(None);
            }
        }
    };
    show_file();
    file_row.append(&file_label);
    let choose = gtk::Button::with_label(tr!("Velg …"));
    choose.add_css_class("sm");
    file_row.append(&choose);
    body.append(&file_row);

    // ---- Kontrollsummen ----
    body.append(&wrap(tr!("SHA-256 fra utgivelsessiden"), &["strong"]));
    let digest = gtk::Entry::new();
    digest.set_placeholder_text(Some(tr!("64 heksadesimale tegn")));
    digest.add_css_class("mono");
    body.append(&digest);
    body.append(&wrap(tr!("Kopier summen fra GitHub, ikke fra en fil ved siden av nedlastingen. Den står ved arkivet under Assets."), &["muted", "sm"]));
    let page = button(Some("arrow-right"), Some(tr!("Åpne utgivelsessiden")), &["sm"]);
    page.set_halign(gtk::Align::Start);
    page.connect_clicked(|_| {
        let uri = format!("{RELEASES}/tag/v{}", rigg_core::version::VERSION);
        let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
    });
    body.append(&page);

    let status = wrap("", &["muted"]);
    status.set_visible(false);
    body.append(&status);

    // ---- Knappene ----
    let buttons = hbox(8, &[]);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label(tr!("Avbryt"));
    let install = gtk::Button::with_label(tr!("Installer"));
    install.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&install);
    body.append(&buttons);
    root.append(&body);

    let update = {
        let install = install.clone();
        let digest = digest.clone();
        let chosen = chosen.clone();
        move || {
            install.set_sensitive(!BUSY.with(Cell::get) && chosen.borrow().is_some() && valid_digest(&normalize_digest(&digest.text())));
        }
    };
    update();
    {
        let update = update.clone();
        digest.connect_changed(move |_| update());
    }
    {
        let chosen = chosen.clone();
        let update = update.clone();
        let show_file = show_file.clone();
        let parent = win.clone();
        choose.connect_clicked(move |_| {
            let d = gtk::FileDialog::new();
            d.set_title(tr!("Velg utgivelsesarkivet"));
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(tr!("Agentic Rigg-arkiv (.tar.gz)")));
            filter.add_pattern("agentic-rigg-*.tar.gz");
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            d.set_filters(Some(&filters));
            if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Downloads) {
                d.set_initial_folder(Some(&gio::File::for_path(dir)));
            }
            let chosen = chosen.clone();
            let update = update.clone();
            let show_file = show_file.clone();
            d.open(Some(&parent), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res && let Some(path) = file.path() {
                    *chosen.borrow_mut() = Some(path);
                    show_file();
                    update();
                }
            });
        });
    }
    {
        let w = win.clone();
        cancel.connect_clicked(move |_| { w.close(); });
    }
    {
        let a = app.clone();
        let w = win.clone();
        let cancel = cancel.clone();
        let choose = choose.clone();
        let digest_entry = digest.clone();
        let status = status.clone();
        let update = update.clone();
        install.connect_clicked(move |button| {
            let Some(archive) = chosen.borrow().clone() else { return };
            let sum = normalize_digest(&digest_entry.text());
            if !valid_digest(&sum) { return; }
            BUSY.with(|b| b.set(true));
            for widget in [button.upcast_ref::<gtk::Widget>(), cancel.upcast_ref(), choose.upcast_ref(), digest_entry.upcast_ref()] {
                widget.set_sensitive(false);
            }
            status.remove_css_class("error");
            status.set_text(tr!("Venter på administratorgodkjenning …"));
            status.set_visible(true);
            let a = a.clone();
            let w = w.clone();
            let cancel = cancel.clone();
            let choose = choose.clone();
            let digest_entry = digest_entry.clone();
            let status = status.clone();
            let update = update.clone();
            let finish = finish.clone();
            run(archive, sum, move |code, err| {
                BUSY.with(|b| b.set(false));
                if code == 0 {
                    installed();
                    crate::widgets::locked::refresh_helper(&a, crate::settings::request_render);
                    finish(true);
                    w.close();
                    return;
                }
                for widget in [cancel.upcast_ref::<gtk::Widget>(), choose.upcast_ref(), digest_entry.upcast_ref()] {
                    widget.set_sensitive(true);
                }
                update();
                status.set_text(&failure_text(code, &err));
                if code != 126 {
                    status.add_css_class("error");
                }
            });
        });
    }
    win.present();
    digest.grab_focus();
}

fn failure_text(code: i32, err: &str) -> String {
    match code {
        126 => tr!("Passordet ble ikke gitt. Ingenting er endret.").into(),
        127 if err.is_empty() => tr!("Fant ikke pkexec, eller ingen kan spørre om passordet (polkit).").into(),
        // Med LC_ALL=C: «sha256sum: WARNING: 1 computed checksum did NOT match».
        _ if err.starts_with("sha256sum:") || err.contains("Kontrollsummen") =>
            tr!("Kontrollsummen stemmer ikke med arkivet. Ingen systemfiler er endret. Sjekk at arkivet og summen gjelder samme fil.").into(),
        _ if err.is_empty() => tr!("Installasjonen feilet. Ingen nye tilganger er gitt.").into(),
        _ => trf!("Installasjonen feilet: {feil}", feil = rigg_core::hjelper::display_diagnostic(err)),
    }
}

/// `pkexec bash -ceu 'PATH=…; SKRIPT' rigg ARKIV SUM` uten å blokkere
/// vinduet. pkexec gir et rensket miljø; PATH settes som i terminalkommandoen
/// (`env -i PATH=/usr/bin:/bin`). `done(kode, siste feillinje)`.
fn run(archive: PathBuf, sum: String, done: impl FnOnce(i32, String) + 'static) {
    let Some(script) = bootstrap_script() else {
        done(1, String::new());
        return;
    };
    let script = format!("{CLEAN_ENV} {script}");
    let argv: Vec<std::ffi::OsString> = vec![
        PKEXEC.into(), BASH.into(), "-ceu".into(), script.into(), "rigg".into(), archive.into_os_string(), sum.into(),
    ];
    let refs: Vec<&std::ffi::OsStr> = argv.iter().map(|a| a.as_os_str()).collect();
    // Unntaket fra «ingen programmer» (clippy.toml): brukeren trykket selv
    // Installer, og pkexec viser passordvinduet fra polkit.
    #[allow(clippy::disallowed_methods)]
    let started = gio::Subprocess::newv(&refs, gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_PIPE);
    let proc = match started {
        Ok(p) => p,
        Err(e) => {
            done(127, e.to_string());
            return;
        }
    };
    glib::MainContext::default().spawn_local(async move {
        let (_, err) = proc.communicate_utf8_future(None).await.unwrap_or((None, None));
        let code = if proc.has_exited() { proc.exit_status() } else { 1 };
        let err = err.map(|s| s.to_string()).unwrap_or_default();
        done(code, err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").to_string());
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

    #[test]
    fn appen_kjorer_samme_skript_som_terminalkommandoen() {
        let script = bootstrap_script().expect("skriptet i system-bootstrap.command");
        assert!(script.starts_with("umask 077;"));
        assert!(script.ends_with(r#"/usr/bin/python3 -I -S installer release "$2""#));
        assert!(script.contains(r#"trap 'rm -rf -- "$tmp"' EXIT;"#));
        assert!(script.contains(r#"printf "%s  release\n" "$2" | sha256sum --check --strict;"#));
        // Tilbake til terminalformen gir nøyaktig kommandoen.
        let quoted = script.replace('\'', r"'\''");
        assert!(COMMAND.contains(&format!("-ceu '{quoted}' rigg \"")));
    }

    #[test]
    fn bare_gyldig_kontrollsum() {
        assert!(valid_digest(&"a".repeat(64)));
        assert!(valid_digest("93e3fad7fcf6116918bb2dda34d0e00edfa997a766b3d0bb9f5c05b5f023f058"));
        assert!(!valid_digest(&"a".repeat(63)));
        assert!(!valid_digest(&format!("{}g", "a".repeat(63))));
        assert!(!valid_digest(&format!(" {}", "a".repeat(63))));
        let sum = "93E3FAD7FCF6116918BB2DDA34D0E00EDFA997A766B3D0BB9F5C05B5F023F058";
        assert_eq!(normalize_digest(&format!(" sha256:{sum} ")), sum.to_ascii_lowercase());
        assert!(valid_digest(&normalize_digest(sum)));
    }
}
