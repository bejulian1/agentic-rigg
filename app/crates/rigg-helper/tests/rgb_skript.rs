//! RGB-installasjon mot en uttrykkelig privat filrot. Ingen hjelper/systemctl
//! kjøres, også når testen selv er root. Rust-adapteren testes separat.

use rigg_core::rgb::{AdapterIdentity, Baseline, Binding, Observed, PciIdentity, Signature};
use rigg_core::rgb_auth::Authorization;
use serde_json::{Value, json};
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const TRUSTED: &str = "usr/local/libexec/agentic-rigg-rgb-v2";
const LEGACY_BIN: &str = "usr/local/libexec/agentic-rigg-rgb-helper";
const PERMISSION: &str = "etc/agentic-rigg/rgb.json";
const UNIT: &str = "etc/systemd/system/agentic-rigg-rgb.service";
const DROPIN: &str = "run/systemd/system/agentic-rigg-rgb.service.d/50-agentic-rigg-card.conf";
const JOURNAL: &str = "var/lib/agentic-rigg-rgb/journal.json";
const INACTIVE: &str = "var/lib/agentic-rigg-rgb.inactive";
const RECOVERY_ID: &str = "0123456789abcdef0123456789abcdef";
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
        // Adminskriptet godtar bare testrøtter under /tmp, uansett TMPDIR.
        let base = Path::new("/tmp").join(format!(
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
            "io.github.AgenticRigg.rgb.v2.policy",
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
        let trusted = root.join(TRUSTED);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(&trusted)
            .unwrap();
        for name in ["agentic-rigg-helper", "agentic-rigg-rgb.service"] {
            fs::copy(source.join(name), trusted.join(name)).unwrap();
        }
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
                    "expected": {"mode":"static","color":[20,40,60],"brightness":80},
                    "recoveryId": RECOVERY_ID,
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
            .arg("--report")
            .arg(&self.report);
        if action != "deactivate" || !extra.contains(&"uid-only") {
            cmd.args(["--pci", PCI, "--uuid", UUID]);
        }
        cmd.args(extra.iter().filter(|arg| **arg != "uid-only"));
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

    fn systemd(&self, state: Value) {
        self.write_fixture("fixture-systemd.json", state.to_string().as_bytes(), 0o600);
    }

    fn write_fixture(&self, relative: &str, data: &[u8], mode: u32) {
        let path = self.at(relative);
        fs::write(&path, data).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
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
    for dir in ["etc", "run"] {
        let stale = f.at(&format!("{dir}/systemd/system/agentic-rigg-rgb.service.d/card.conf"));
        fs::DirBuilder::new().recursive(true).mode(0o755).create(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "gammelt korttillegg").unwrap();
        fs::set_permissions(stale, fs::Permissions::from_mode(0o644)).unwrap();
    }
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
    for dir in ["etc", "run"] {
        assert!(!f.at(&format!("{dir}/systemd/system/agentic-rigg-rgb.service.d/card.conf")).exists());
    }
    let dropin = fs::read_to_string(f.at(DROPIN)).unwrap();
    assert_eq!(
        dropin,
        "[Service]\nDeviceAllow=\nDeviceAllow=/dev/i2c-17 rw\n"
    );
    for (path, mode) in [
        ("etc/agentic-rigg/rgb.json", 0o600),
        ("var/lib/agentic-rigg-rgb/baseline.json", 0o600),
        (
            "usr/local/libexec/agentic-rigg-rgb-v2/agentic-rigg-helper",
            0o755,
        ),
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
    f.write_fixture(JOURNAL, b"uavklart", 0o600);
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
fn uid_alene_krever_stoppet_ren_arbeider() {
    let f = Fixture::new();
    f.activate();
    let out = f.run("deactivate", &["uid-only"]);
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains(UUID));
    f.systemd(json!({"ActiveState":"inactive"}));
    f.write_fixture(JOURNAL, b"uavklart", 0o600);
    let out = f.run("deactivate", &["uid-only"]);
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains(PCI));
    fs::remove_file(f.at(JOURNAL)).unwrap();
    assert!(f.run("deactivate", &["uid-only"]).status.success());
    assert!(!f.at(PERMISSION).exists());
}

#[test]
fn avklart_deaktivering_beholder_originalen_og_ukjente_filer() {
    let f = Fixture::new();
    f.activate();
    let baseline = fs::read(f.at("var/lib/agentic-rigg-rgb/baseline.json")).unwrap();
    let own = f.at("usr/local/libexec/agentic-rigg-rgb/egen-merknad");
    fs::DirBuilder::new()
        .mode(0o755)
        .create(own.parent().unwrap())
        .unwrap();
    fs::write(&own, b"behold").unwrap();
    f.write_fixture(LEGACY_BIN, b"eldre", 0o755);
    let out = f.run("deactivate", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!f.at("etc/agentic-rigg/rgb.json").exists());
    assert!(!f.at(LEGACY_BIN).exists());
    assert!(f.at(TRUSTED).join("agentic-rigg-helper").is_file());
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
        !f.at(DROPIN).exists()
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
    assert!(f.at(TRUSTED).join("agentic-rigg-helper").is_file());
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

#[test]
fn policy_peker_pa_faste_innganger_og_aktivering_avviser_utrygg_kilde() {
    let f = Fixture::new();
    let policy = fs::read_to_string(f.source.join("io.github.AgenticRigg.rgb.v2.policy")).unwrap();
    for script in ["aktiver", "avklar", "gjenopprett", "deaktiver"] {
        let name = format!("rgb-{script}.sh");
        assert!(policy.contains(&format!("exec.path\">/{TRUSTED}/{name}</annotate>")));
        let entry = fs::read_to_string(f.source.join(name)).unwrap();
        assert!(entry.starts_with(&format!(
            "#!/bin/sh\nexec /usr/bin/python3 -I -S /{TRUSTED}/rgb-admin.py "
        )));
    }
    assert!(
        !f.run("activate", &["--static-confirmed", "--kilde", "/tmp"])
            .status
            .success()
    );
    let trusted_binary = f.at(TRUSTED).join("agentic-rigg-helper");
    fs::remove_file(&trusted_binary).unwrap();
    symlink(f.source.join("agentic-rigg-helper"), &trusted_binary).unwrap();
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    assert!(!f.at(PERMISSION).exists());
}

#[test]
fn aktivering_kan_gjentas_ved_tillatelse_for_enhet_og_feilstart() {
    let f = Fixture::new();
    f.activate();
    fs::remove_file(f.at(UNIT)).unwrap();
    fs::remove_file(f.at(DROPIN)).unwrap();
    f.systemd(json!({"fail":{"start":"before"}}));
    assert!(!f.run("activate", &["--static-confirmed"]).status.success());
    for path in [PERMISSION, UNIT, DROPIN] {
        assert!(f.at(path).exists());
    }
    f.systemd(json!({}));
    f.activate();
    f.activate();
    assert!(f.at(PERMISSION).exists());
}

#[test]
fn deaktivering_gjentas_etter_stopp_og_reloadfeil_eller_halv_installasjon() {
    let f = Fixture::new();
    f.activate();
    f.systemd(json!({"ActiveState":"active","fail":{"stop":"before"}}));
    assert!(!f.run("deactivate", &[]).status.success());
    assert!(f.at(PERMISSION).exists() && f.at(UNIT).exists() && f.at(DROPIN).exists());
    f.systemd(json!({"ActiveState":"active","fail":{"stop":"after","daemon-reload":"before"}}));
    assert!(!f.run("deactivate", &[]).status.success());
    assert!(f.at(PERMISSION).exists());
    assert!(!f.at(UNIT).exists() && !f.at(DROPIN).exists());
    assert!(!f.at(INACTIVE).exists());
    f.systemd(json!({"fail":{"stop":"before"}}));
    for _ in 0..2 {
        assert!(f.run("deactivate", &[]).status.success());
        assert!(f.at(INACTIVE).exists());
        assert!(!f.at(PERMISSION).exists());
    }

    // Rester fra en eldre opprydding som fjernet tillatelsen først.
    f.write_fixture(LEGACY_BIN, b"eldre", 0o755);
    assert!(f.run("deactivate", &[]).status.success());
    assert!(!f.at(LEGACY_BIN).exists());
}

#[test]
fn ren_ny_aktivering_med_endret_farge_bevarer_det_tidligere_grunnopptaket() {
    let f = Fixture::new();
    f.activate();
    let path = f.at("var/lib/agentic-rigg-rgb/baseline.json");
    let old = fs::read(&path).unwrap();
    assert!(f.run("deactivate", &[]).status.success());
    let mut report: Value = serde_json::from_slice(&fs::read(&f.report).unwrap()).unwrap();
    report["report"]["baseline"]["observed"]["color"] = json!([11, 22, 33]);
    fs::write(&f.report, report.to_string()).unwrap();
    f.activate();
    assert_ne!(fs::read(path).unwrap(), old);
    assert_eq!(
        fs::read(f.at("var/lib/agentic-rigg-rgb/baseline-1.json")).unwrap(),
        old
    );
    assert!(!f.at(INACTIVE).exists());
}

#[test]
fn stoppet_eller_eldre_arbeider_avklares_for_appen_fjernes() {
    let f = Fixture::new();
    f.activate();
    f.write_fixture(LEGACY_BIN, b"eldre", 0o755);
    f.write_fixture(JOURNAL, b"uavklart", 0o600);
    let home = f.base.join("home");
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["agentic-rigg", "agentic-riggd"] {
        fs::write(bin.join(name), b"behold til avklart").unwrap();
    }
    // Aldri stopp brukerens tjeneste eller les deres innstillinger.
    #[allow(clippy::disallowed_methods)]
    let uninstall = || {
        Command::new("bash")
            .arg(app().join("uninstall.sh"))
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("RIGG_HJELPER_ROT", &f.root)
            .env("RIGG_INSTALL_NO_SYSTEMD", "1")
            .output()
            .unwrap()
    };
    assert_eq!(uninstall().status.code(), Some(3));
    assert!(bin.join("agentic-rigg").exists() && bin.join("agentic-riggd").exists());
    assert!(!f.run("resolve", &["--inspect"]).status.success());
    assert!(!f.run("deactivate", &[]).status.success());
    assert!(f.run("restore", &["--restore-original"]).status.success());
    assert!(f.at(JOURNAL).exists());
    let report = f.run("resolve", &["--inspect"]);
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&report.stdout).unwrap()["recoveryId"],
        RECOVERY_ID
    );
    assert!(!f.run("resolve", &["--static-confirmed"]).status.success());
    for _ in 0..2 {
        assert!(
            f.run(
                "resolve",
                &["--static-confirmed", "--recovery-id", RECOVERY_ID]
            )
            .status
            .success()
        );
        assert!(!f.at(JOURNAL).exists());
    }
    assert!(f.run("deactivate", &[]).status.success());
    assert!(uninstall().status.success());
    assert!(!bin.join("agentic-rigg").exists() && !bin.join("agentic-riggd").exists());
}
