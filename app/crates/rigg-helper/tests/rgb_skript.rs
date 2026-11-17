//! RGB-installasjon mot en uttrykkelig privat filrot. Ingen hjelper/systemctl
//! kjøres, også når testen selv er root. Rust-adapteren testes separat.

use rigg_core::rgb::{AdapterIdentity, Baseline, Binding, Observed, PciIdentity, Signature};
use rigg_core::rgb_auth::Authorization;
use serde_json::{Value, json};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const PCI: &str = "0000:1c:00.0";
const UUID: &str = "GPU-11111111-2222-3333-4444-555555555555";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    source: PathBuf,
    report: PathBuf,
}

fn app() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "rigg-rgb-admin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        let root = base.join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let source = base.join("source");
        fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
        for name in [
            "rgb-admin.py",
            "rgb-aktiver.sh",
            "rgb-deaktiver.sh",
            "rgb-avklar.sh",
            "rgb-gjenopprett.sh",
            "agentic-rigg-rgb.service",
            "io.github.AgenticRigg.rgb.policy",
        ] {
            fs::copy(app().join("data/hjelper").join(name), source.join(name)).unwrap();
            fs::set_permissions(source.join(name), fs::Permissions::from_mode(0o644)).unwrap();
        }
        // Denne kilden må aldri kjøres av --root, heller ikke for --version.
        fs::write(source.join("agentic-rigg-helper"), b"#!/bin/sh\nexit 91\n").unwrap();
        fs::set_permissions(
            source.join("agentic-rigg-helper"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let report = base.join("report.json");
        let binding = Binding {
            pci: PCI.into(),
            uuid: UUID.into(),
            pci_ids: PciIdentity {
                vendor: 0x10de,
                device: 0x1e87,
                subsystem_vendor: 0x10de,
                subsystem_device: 0x1e87,
            },
            adapter: AdapterIdentity {
                port: 1,
                name: "NVIDIA i2c adapter 1 at 1c:00.0".into(),
                parent_pci: PCI.into(),
                driver: "nvidia".into(),
            },
            address: 0x49,
            signature: Signature::Palit100,
        };
        let baseline = Baseline::user_confirmed_static(
            binding.clone(),
            Observed {
                color: Some([20, 40, 60]),
                brightness_raw: Some(80),
                mode: None,
            },
        )
        .unwrap();
        let auth = Authorization::new(1000, binding, rigg_core::version::build()).unwrap();
        fs::write(
            &report,
            json!({
                "ok": true,
                "report": {
                    "authorization": auth, "baseline": baseline,
                    "metadata": {
                        "pci": PCI, "uuid": UUID, "adapter": {"busNumber": 17},
                        "devicePath": "/dev/i2c-17", "deviceNumber": {"major": 89, "minor": 17}
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        Self {
            base,
            root,
            source,
            report,
        }
    }

    fn run(&self, action: &str, extra: &[&str]) -> Output {
        self.run_at_root(&self.root, action, extra)
    }

    fn run_at_root(&self, root: &Path, action: &str, extra: &[&str]) -> Output {
        // Kun denne private filfixturen; --root kjører verken helper,
        // systemctl eller noen fysisk backend, også når Cargo er root.
        #[allow(clippy::disallowed_methods)]
        let mut cmd = Command::new("/usr/bin/python3");
        cmd.arg("-I")
            .arg(self.source.join("rgb-admin.py"))
            .arg(action)
            .arg("--root")
            .arg(root)
            .arg("--uid")
            .arg("1000")
            .arg("--kilde")
            .arg(&self.source)
            .arg("--report")
            .arg(&self.report)
            .args(["--pci", PCI, "--uuid", UUID])
            .args(extra);
        cmd.output().unwrap()
    }

    fn activate(&self) {
        let out = self.run("activate", &["--static-confirmed"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn filfixture_installerer_bare_rgb_og_en_fersk_avgrenset_adapter() {
    let f = Fixture::new();
    for (relative, value) in [
        ("etc/agentic-rigg/helper.json", "behold-nvml-konfig"),
        ("usr/local/libexec/agentic-rigg-helper", "behold-nvml-bin"),
        (
            "etc/systemd/system/agentic-rigg-helper.service",
            "behold-nvml-enhet",
        ),
    ] {
        let path = f.at(relative);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(path.parent().unwrap())
            .unwrap();
        fs::write(path, value).unwrap();
    }
    f.activate();
    for (relative, value) in [
        ("etc/agentic-rigg/helper.json", "behold-nvml-konfig"),
        ("usr/local/libexec/agentic-rigg-helper", "behold-nvml-bin"),
        (
            "etc/systemd/system/agentic-rigg-helper.service",
            "behold-nvml-enhet",
        ),
    ] {
        assert_eq!(fs::read_to_string(f.at(relative)).unwrap(), value);
    }
    let dropin =
        fs::read_to_string(f.at("etc/systemd/system/agentic-rigg-rgb.service.d/card.conf"))
            .unwrap();
    assert_eq!(
        dropin,
        "[Service]\nDeviceAllow=\nDeviceAllow=/dev/i2c-17 rw\n"
    );
    for (path, mode) in [
        ("etc/agentic-rigg/rgb.json", 0o600),
        ("var/lib/agentic-rigg-rgb/baseline.json", 0o600),
        ("usr/local/libexec/agentic-rigg-rgb-helper", 0o755),
    ] {
        assert_eq!(
            fs::metadata(f.at(path)).unwrap().permissions().mode() & 0o7777,
            mode
        );
    }
    assert_eq!(
        fs::metadata(f.at("var/lib/agentic-rigg-rgb"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );
    let unit = fs::read_to_string(f.at("etc/systemd/system/agentic-rigg-rgb.service")).unwrap();
    assert!(!unit.contains("ExecStopPost=") && !unit.contains("WantedBy="));
    assert!(unit.contains("DevicePolicy=closed") && !unit.contains("char-i2c"));
}

#[test]
fn journal_og_manglende_bekreftelse_stanser_installasjon_eller_fjerning() {
    let f = Fixture::new();
    assert!(!f.run("activate", &[]).status.success());
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
    f.activate();
    let journal = f.at("var/lib/agentic-rigg-rgb/journal.json");
    fs::write(&journal, b"uavklart").unwrap();
    let baseline = fs::read(f.at("var/lib/agentic-rigg-rgb/baseline.json")).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    assert!(!f.run("deactivate", &[]).status.success());
    assert_eq!(fs::read(&journal).unwrap(), b"uavklart");
    assert_eq!(
        fs::read(f.at("var/lib/agentic-rigg-rgb/baseline.json")).unwrap(),
        baseline
    );
    assert!(f.at("etc/agentic-rigg/rgb.json").exists());
}

#[test]
fn avklart_deaktivering_beholder_originalen_og_ukjente_filer() {
    let f = Fixture::new();
    f.activate();
    let baseline = fs::read(f.at("var/lib/agentic-rigg-rgb/baseline.json")).unwrap();
    let own = f.at("usr/local/libexec/agentic-rigg-rgb/egen-merknad");
    fs::write(&own, b"behold").unwrap();
    let out = f.run("deactivate", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
    assert!(!f.at("usr/local/libexec/agentic-rigg-rgb-helper").exists());
    assert_eq!(
        fs::read(f.at("var/lib/agentic-rigg-rgb/baseline.json")).unwrap(),
        baseline
    );
    assert_eq!(fs::read(own).unwrap(), b"behold");
}

#[test]
fn feil_busrapport_og_symlink_target_skriver_ikke_enhetsregel() {
    let f = Fixture::new();
    let mut report: Value = serde_json::from_slice(&fs::read(&f.report).unwrap()).unwrap();
    report["report"]["metadata"]["adapter"]["busNumber"] = json!("17\nDeviceAllow=char-i2c rw");
    fs::write(&f.report, report.to_string()).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    assert!(
        !f.at("etc/systemd/system/agentic-rigg-rgb.service.d/card.conf")
            .exists()
    );

    let f = Fixture::new();
    let outside = f.base.join("utenfor");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, f.root.join("etc")).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    assert!(fs::read_dir(outside).unwrap().next().is_none());
}

#[test]
fn duplikater_forkortelser_og_root_alias_avvises_uten_host_handling() {
    let f = Fixture::new();
    for extra in [
        vec!["--static-confirmed", "--pci", PCI],
        vec!["--static-confirmed", "--roo", "/"],
        vec!["--static-confirmed", "--restore-original"],
        vec!["--static-confirmed", "--root", "/"],
    ] {
        assert!(!f.run("activate", &extra).status.success());
    }
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
}

#[test]
fn root_alias_og_skrivbare_fixtureforeldre_avvises() {
    let f = Fixture::new();
    let alias = f.base.join("root-alias");
    symlink("/", &alias).unwrap();
    let out = f.run_at_root(&alias, "activate", &["--static-confirmed"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Testrot"));
    fs::set_permissions(&f.base, fs::Permissions::from_mode(0o777)).unwrap();
    let out = f.run("activate", &["--static-confirmed"]);
    fs::set_permissions(&f.base, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!out.status.success());
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
}

#[test]
fn levende_workerlas_beholder_tillatelse_og_binaer() {
    let f = Fixture::new();
    f.activate();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(f.at("var/lib/agentic-rigg-rgb/worker.lock"))
        .unwrap();
    // SAFETY: flock gjelder bare filen i denne uttrykkelig private fixturen.
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let out = f.run("deactivate", &[]);
    assert!(!out.status.success());
    assert!(f.at("etc/agentic-rigg/rgb.json").exists());
    assert!(f.at("usr/local/libexec/agentic-rigg-rgb-helper").exists());
    drop(lock);
    assert!(f.run("deactivate", &[]).status.success());
}

#[test]
fn duplisert_eller_ikke_regulaer_rapport_er_ikke_installasjonsbevis() {
    let f = Fixture::new();
    fs::write(&f.report, br#"{"ok":true,"ok":false,"report":{}}"#).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    fs::remove_file(&f.report).unwrap();
    fs::create_dir(&f.report).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
}
