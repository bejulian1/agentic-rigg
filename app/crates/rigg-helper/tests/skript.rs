//! Aktiver- og deaktiver-skriptene, `trinn.sh`, `install.sh` og
//! `uninstall.sh` mot en falsk rot og et falskt HOME. Aldri mot `/`, aldri
//! root, aldri systemctl (skriptene hopper over det med `--root`).
#![allow(clippy::disallowed_methods)] // testene kjører skriptene

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn app_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmpdir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "hjelper-skript-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const PCI: &str = "0000:01:00.0";
const UUID: &str = "GPU-01234567-1234-1234-1234-0123456789ab";

fn fixture(tag: &str) -> PathBuf {
    let root = tmpdir(tag);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let lib = root.join("usr/local/libexec/agentic-rigg");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(&lib)
        .unwrap();
    std::fs::copy(
        app_dir().join("data/hjelper/agentic-rigg-helper.service"),
        lib.join("agentic-rigg-helper.service"),
    )
    .unwrap();
    std::fs::write(
        lib.parent().unwrap().join("agentic-rigg-helper"),
        b"skal aldri kjores",
    )
    .unwrap();
    for path in [
        lib.join("agentic-rigg-helper.service"),
        lib.parent().unwrap().join("agentic-rigg-helper"),
    ] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    root
}

fn admin(root: &Path, action: &str, uid: &str, args: &[&str]) -> Output {
    Command::new("/usr/bin/python3")
        .args(["-I", "-S"])
        .arg(app_dir().join("data/hjelper/nvml-admin.py"))
        .arg(action)
        .arg("--root")
        .arg(root)
        .args(["--uid", uid])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn aktivering_binder_kort_og_deaktivering_krever_eier_ikke_kort() {
    let root = fixture("binding");
    let args = ["--pci", PCI, "--uuid", UUID];
    let out = admin(&root, "activate", "1000", &args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = root.join("etc/agentic-rigg/helper.json");
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        (config["versjon"].as_u64(), config["uid"].as_u64()),
        (Some(2), Some(1000))
    );
    assert_eq!(config["pci"], PCI);
    assert_eq!(config["uuid"], UUID);
    assert_eq!(config["enabled"], true);
    assert!(!admin(&root, "deactivate", "1001", &[]).status.success());
    assert!(path.exists());
    assert_eq!(
        std::fs::read_to_string(
            root.join("etc/systemd/system/agentic-rigg-helper.service.d/gpu.conf")
        )
        .unwrap(),
        "[Service]\nDeviceAllow=\nDeviceAllow=/dev/nvidiactl rw\nDeviceAllow=/dev/nvidia0 rw\n"
    );
    assert!(admin(&root, "deactivate", "1000", &[]).status.success());
    assert!(!path.exists());
    assert!(root.join("usr/local/libexec/agentic-rigg-helper").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn uavklart_nvml_beholdes_og_ren_aktivering_arkiverer_grunnverdiene() {
    let root = fixture("grunnopptak");
    let state = root.join("var/lib/agentic-rigg");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&state)
        .unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let baseline = state.join("baseline.json");
    std::fs::write(&baseline, "opptak").unwrap();
    std::fs::set_permissions(&baseline, std::fs::Permissions::from_mode(0o600)).unwrap();
    let dirty = state.join("dirty.json");
    let args = ["--pci", PCI, "--uuid", UUID];
    std::fs::write(&dirty, "uavklart").unwrap();
    assert!(!admin(&root, "activate", "1000", &args).status.success());
    assert!(!admin(&root, "deactivate", "1000", &[]).status.success());
    assert!(baseline.exists() && dirty.exists());
    std::fs::remove_file(&dirty).unwrap();
    std::os::unix::fs::symlink("mangler", &dirty).unwrap();
    assert!(!admin(&root, "activate", "1000", &args).status.success());
    std::fs::remove_file(dirty).unwrap();
    assert!(admin(&root, "activate", "1000", &args).status.success());
    let archive = std::fs::read_dir(state).unwrap().next().unwrap().unwrap();
    assert!(
        archive
            .file_name()
            .to_string_lossy()
            .starts_with("baseline-forrige-")
    );
    assert_eq!(std::fs::read_to_string(archive.path()).unwrap(), "opptak");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn faste_nvml_innganger_og_avviste_argumenter() {
    let root = fixture("argument");
    for args in [
        &["--pci", PCI, "--uuid", UUID, "--pci", PCI][..],
        &["--kilde", "x"],
        &["--systemctl", "x"],
    ] {
        assert!(!admin(&root, "activate", "1000", args).status.success());
    }
    for (wrapper, action) in [("aktiver.sh", "activate"), ("deaktiver.sh", "deactivate")] {
        let text = std::fs::read_to_string(app_dir().join("data/hjelper").join(wrapper)).unwrap();
        assert_eq!(
            text,
            format!(
                "#!/bin/sh\nexec /usr/bin/python3 -I -S /usr/local/libexec/agentic-rigg/nvml-admin.py {action} \"$@\"\n"
            )
        );
    }
    assert!(
        !admin(&root, "activate", "0", &["--pci", PCI, "--uuid", UUID])
            .status
            .success()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn polkit_og_enheten_er_som_avtalt() {
    let p =
        std::fs::read_to_string(app_dir().join("data/hjelper/io.github.AgenticRigg.helper.policy"))
            .unwrap();
    for id in ["aktiver", "deaktiver"] {
        assert!(
            p.contains(&format!(
                "<action id=\"io.github.AgenticRigg.helper.v2.{id}\">"
            )),
            "{id}"
        );
    }
    assert!(
        !p.contains("<allow_any>yes") && !p.contains("<allow_active>yes"),
        "ingen handling uten passord"
    );
    assert_eq!(
        p.matches("<allow_active>auth_admin</allow_active>").count(),
        2
    );
    let u = std::fs::read_to_string(app_dir().join("data/hjelper/agentic-rigg-helper.service"))
        .unwrap();
    for must in [
        "ProtectSystem=strict",
        "NoNewPrivileges=yes",
        "CapabilityBoundingSet=CAP_CHOWN CAP_SYS_ADMIN\n",
        "StateDirectory=agentic-rigg",
        "StateDirectoryMode=0700",
        "Restart=no",
        "DevicePolicy=closed",
        "ProtectHome=yes",
        "PrivateNetwork=yes",
        "RestrictAddressFamilies=AF_UNIX",
        "ExecStopPost=/usr/local/libexec/agentic-rigg-helper --backend nvml --tilbakestill",
    ] {
        assert!(u.contains(must), "{must}");
    }
    let devices: Vec<&str> = u
        .lines()
        .filter(|l| l.starts_with("DeviceAllow="))
        .collect();
    assert!(
        devices == vec!["DeviceAllow=/dev/nvidiactl rw"],
        "{devices:?}"
    );
    assert!(!u.contains("DeviceAllow=char-i2c"));
    assert!(!p.contains("helper.trinn."));
    assert!(!u.contains("ProcSubset=pid"));
}

#[test]
fn install_kopierer_bare_filene_og_uninstall_sier_fra() {
    let dir = tmpdir("inst");
    let home = dir.join("home");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for b in ["agentic-rigg", "agentic-riggd", "agentic-rigg-helper"] {
        std::fs::write(bin.join(b), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(bin.join(b), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let env = |c: &mut Command| {
        c.env("HOME", &home)
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env("RIGG_INSTALL_NO_SYSTEMD", "1");
    };
    let mut c = Command::new("bash");
    c.arg(app_dir().join("install.sh"))
        .arg("--bin-dir")
        .arg(&bin);
    env(&mut c);
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let h = home.join(".local/share/agentic-rigg/hjelper");
    assert!(!h.exists());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ikke aktivert"));

    // Uten aktiv hjelper: alt fjernes, kode 0.
    let fake_root = dir.join("rot");
    std::fs::create_dir_all(&fake_root).unwrap();
    let mut c = Command::new("bash");
    c.arg(app_dir().join("uninstall.sh"))
        .env("RIGG_HJELPER_ROT", &fake_root);
    env(&mut c);
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!h.exists());

    // Med aktiv hjelper (i den falske roten): sier fra og gir kode 3.
    let mut c = Command::new("bash");
    c.arg(app_dir().join("install.sh"))
        .arg("--bin-dir")
        .arg(&bin);
    env(&mut c);
    assert!(c.output().unwrap().status.success());
    std::fs::create_dir_all(fake_root.join("etc/agentic-rigg")).unwrap();
    std::fs::write(fake_root.join("etc/agentic-rigg/helper.json"), "{}").unwrap();
    let mut c = Command::new("bash");
    c.arg(app_dir().join("uninstall.sh"))
        .env("RIGG_HJELPER_ROT", &fake_root);
    env(&mut c);
    let out = c.output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(
        s.contains("fortsatt aktiv") && home.join(".local/bin/agentic-rigg").exists(),
        "{s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
