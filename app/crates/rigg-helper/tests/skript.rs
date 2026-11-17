//! Aktiver- og deaktiver-skriptene, `trinn.sh`, `install.sh` og
//! `uninstall.sh` mot en falsk rot og et falskt HOME. Aldri mot `/`, aldri
//! root, aldri systemctl (skriptene hopper over det med `--root`).
#![allow(clippy::disallowed_methods)] // testene kjører skriptene

use std::os::unix::fs::{MetadataExt, PermissionsExt};
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

/// Kildemappa slik `install.sh` legger den: skriptene, enheten, policyen og binæren.
fn kilde(dir: &Path) -> PathBuf {
    let k = dir.join("kilde");
    std::fs::create_dir_all(&k).unwrap();
    for e in std::fs::read_dir(app_dir().join("data/hjelper"))
        .unwrap()
        .flatten()
    {
        std::fs::copy(e.path(), k.join(e.file_name())).unwrap();
    }
    std::fs::copy(
        env!("CARGO_BIN_EXE_agentic-rigg-helper"),
        k.join("agentic-rigg-helper"),
    )
    .unwrap();
    k
}

fn run(script: &Path, args: &[&str], env: &[(&str, String)]) -> Output {
    let mut c = Command::new("bash");
    c.arg(script).args(args).env_remove("PKEXEC_UID");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

/// Alle filer under `root`, med modus, sortert («usr/local/libexec/… 755»).
fn tree(root: &Path) -> Vec<String> {
    fn walk(base: &Path, p: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(p).unwrap().flatten() {
            let path = e.path();
            let m = std::fs::symlink_metadata(&path).unwrap();
            if m.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(format!(
                    "{} {:o}",
                    path.strip_prefix(base).unwrap().display(),
                    m.mode() & 0o777
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn uid() -> String {
    unsafe { libc::geteuid() }.to_string()
}

#[test]
fn aktiver_og_deaktiver_gir_riktig_filtre() {
    let dir = tmpdir("tre");
    let k = kilde(&dir);
    let root = dir.join("rot");
    std::fs::create_dir_all(&root).unwrap();
    let r = root.to_str().unwrap();
    let env = [("PKEXEC_UID", uid())];

    let out = run(&k.join("aktiver.sh"), &["--root", r], &env);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hopper over systemctl"), "{stdout}");
    assert_eq!(
        tree(&root),
        vec![
            "etc/agentic-rigg/helper.json 644",
            "etc/systemd/system/agentic-rigg-helper.service 644",
            "etc/systemd/system/agentic-rigg-helper.service.d/gpu.conf 644",
            "usr/local/libexec/agentic-rigg-helper 755",
            "usr/local/libexec/agentic-rigg/aktiver.sh 755",
            "usr/local/libexec/agentic-rigg/deaktiver.sh 755",
            "usr/local/libexec/agentic-rigg/trinn.sh 755",
            "usr/share/polkit-1/actions/io.github.AgenticRigg.helper.policy 644",
        ]
    );
    let cfg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("etc/agentic-rigg/helper.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cfg["uid"].to_string(), uid());
    assert_eq!(cfg["versjon"], 2);
    assert_eq!(cfg["enabled"], true);
    assert!(cfg.get("trinn").is_none());
    assert!(cfg["bygg"].as_u64().unwrap() >= 17);
    assert_eq!(
        std::fs::metadata(root.join("etc/agentic-rigg"))
            .unwrap()
            .mode()
            & 0o777,
        0o755
    );

    // Eldre trinnkommandoer gir ingen nye tillatelser.
    let trinn = root.join("usr/local/libexec/agentic-rigg/trinn.sh");
    assert!(
        !run(&trinn, &["effekt", "på", "--root", r], &[])
            .status
            .success()
    );
    let before = tree(&root);
    assert!(
        run(&k.join("aktiver.sh"), &["--root", r], &env)
            .status
            .success()
    );
    assert_eq!(tree(&root), before);
    let text = std::fs::read_to_string(root.join("etc/agentic-rigg/helper.json")).unwrap();
    let config: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(config["enabled"], true);

    // Deaktiver fra kopien bare root kan endre: ingen filer igjen.
    let out = run(
        &root.join("usr/local/libexec/agentic-rigg/deaktiver.sh"),
        &["--root", r],
        &[],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(tree(&root).is_empty(), "igjen: {:?}", tree(&root));
    assert!(!root.join("etc/agentic-rigg").exists());
    assert!(!root.join("usr/local/libexec/agentic-rigg").exists());
    // Idempotent.
    let out = run(&k.join("deaktiver.sh"), &["--root", r], &[]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("allerede slått av"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ny_ren_aktivering_arkiverer_opptaket_men_dirty_beholder_alt() {
    let dir = tmpdir("grunnopptak");
    let k = kilde(&dir);
    let root = dir.join("rot");
    let state = root.join("var/lib/agentic-rigg");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let baseline = state.join("baseline.json");
    let original = r#"{"captured":{"powerLimitW":200,"clockOffsets":{"coreMhz":100}}}"#;
    std::fs::write(&baseline, original).unwrap();
    std::fs::set_permissions(&baseline, std::fs::Permissions::from_mode(0o600)).unwrap();
    let r = root.to_str().unwrap();
    let env = [("PKEXEC_UID", uid())];
    let dirty = state.join("dirty.json");
    std::fs::write(&dirty, "uavklart").unwrap();
    let before = tree(&root);
    let out = run(&k.join("aktiver.sh"), &["--root", r], &env);
    assert!(!out.status.success());
    assert_eq!(tree(&root), before);
    assert_eq!(std::fs::read_to_string(&baseline).unwrap(), original);
    // Også en dangling symlink er uavklart, ikke fravær av endringer.
    std::fs::remove_file(&dirty).unwrap();
    std::os::unix::fs::symlink("mangler", &dirty).unwrap();
    assert!(
        !run(&k.join("aktiver.sh"), &["--root", r], &env)
            .status
            .success()
    );
    assert_eq!(std::fs::read_to_string(&baseline).unwrap(), original);
    std::fs::remove_file(&dirty).unwrap();
    let out = run(&k.join("aktiver.sh"), &["--root", r], &env);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !baseline.exists(),
        "Ny helper skal fange nye verdier ved oppstart"
    );
    let archives: Vec<_> = std::fs::read_dir(&state).unwrap().flatten().collect();
    assert_eq!(archives.len(), 1);
    assert!(
        archives[0]
            .file_name()
            .to_string_lossy()
            .starts_with("baseline-forrige-")
    );
    assert_eq!(
        std::fs::read_to_string(archives[0].path()).unwrap(),
        original
    );
    assert_eq!(archives[0].metadata().unwrap().mode() & 0o777, 0o600);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn skriptene_gir_tydelige_feil() {
    let dir = tmpdir("feil");
    let k = kilde(&dir);
    let root = dir.join("rot");
    std::fs::create_dir_all(&root).unwrap();
    let r = root.to_str().unwrap();
    // Uten uid (ikke via pkexec): feil, ingenting lagt inn.
    let out = run(&k.join("aktiver.sh"), &["--root", r], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Kjør via pkexec"));
    assert!(tree(&root).is_empty());
    // uid 0 får aldri være klient.
    let out = run(&k.join("aktiver.sh"), &["--root", r, "--uid", "0"], &[]);
    assert_eq!(out.status.code(), Some(2));
    // Ukjent argument og manglende fil.
    assert_eq!(
        run(&k.join("aktiver.sh"), &["--tull"], &[]).status.code(),
        Some(2)
    );
    std::fs::remove_file(k.join("trinn.sh")).unwrap();
    let out = run(&k.join("aktiver.sh"), &["--root", r, "--uid", &uid()], &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Mangler"));
    assert!(tree(&root).is_empty());
    // Uten --root og uten root: nekter (rører aldri /).
    let out = run(&k.join("deaktiver.sh"), &[], &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Må kjøres som root"));
    let out = run(&k.join("aktiver.sh"), &["--uid", &uid()], &[]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("Må kjøres som root"));
    // Den eldre trinnkommandoen avviser alltid; ingen funksjonsgodkjenning.
    let t = app_dir().join("data/hjelper/trinn.sh");
    assert_eq!(
        run(&t, &["alt", "på", "--root", r], &[]).status.code(),
        Some(2)
    );
    let before = tree(&root);
    let out = run(&t, &["effekt", "på", "--root", r], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Separate godkjenningstrinn er fjernet"));
    assert_eq!(tree(&root), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn polkit_og_enheten_er_som_avtalt() {
    let p =
        std::fs::read_to_string(app_dir().join("data/hjelper/io.github.AgenticRigg.helper.policy"))
            .unwrap();
    for id in ["aktiver", "deaktiver"] {
        assert!(
            p.contains(&format!(
                "<action id=\"io.github.AgenticRigg.helper.{id}\">"
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
        1
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
    assert_eq!(
        tree(&h),
        vec![
            "agentic-rigg-helper 755",
            "agentic-rigg-helper.service 644",
            "agentic-rigg-rgb.service 644",
            "aktiver.sh 755",
            "deaktiver.sh 755",
            "io.github.AgenticRigg.helper.policy 644",
            "io.github.AgenticRigg.rgb.policy 644",
            "rgb-admin.py 755",
            "rgb-aktiver.sh 755",
            "rgb-avklar.sh 755",
            "rgb-deaktiver.sh 755",
            "rgb-gjenopprett.sh 755",
            "trinn.sh 755"
        ]
    );
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
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("fortsatt aktiv") && s.contains("deaktiver.sh"),
        "{s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
