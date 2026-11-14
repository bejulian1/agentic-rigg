//! Hjelperen som vanlig bruker, ende til ende: `agentic-rigg-helper
//! --backend sim --socket <tmp> --config <tmp>/helper.json` som barneprosess.
//! Ingen root, ingen systemd og ingen NVML (sim eller avvist legacy-oppsett).
#![allow(clippy::disallowed_methods)] // testene starter hjelperen som barneprosess

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn tmpdir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "hjelper-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    d
}

fn my_uid() -> u32 {
    unsafe { libc::geteuid() }
}

pub fn write_config(dir: &Path, uid: u32, steps: &[&str]) {
    let etc = dir.join("etc/agentic-rigg");
    std::fs::create_dir_all(&etc).unwrap();
    std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tmp = etc.join(".helper.json.tmp");
    std::fs::write(
        &tmp,
        json!({"versjon": 1, "uid": uid, "trinn": steps, "bygg": 17, "aktivert": "test"})
            .to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::rename(&tmp, etc.join("helper.json")).unwrap();
}

struct Helper {
    child: Child,
    dir: PathBuf,
    socket: PathBuf,
}

impl Helper {
    fn start(tag: &str, uid: u32, steps: &[&str], backend: &str) -> Helper {
        let dir = tmpdir(tag);
        write_config(&dir, uid, steps);
        Helper::start_in(dir, backend)
    }

    fn start_in(dir: PathBuf, backend: &str) -> Helper {
        let socket = dir.join("run/agentic-rigg/helper.sock");
        let child = Command::new(env!("CARGO_BIN_EXE_agentic-rigg-helper"))
            .args(["--backend", backend, "--socket"])
            .arg(&socket)
            .arg("--config")
            .arg(dir.join("etc/agentic-rigg/helper.json"))
            .arg("--baseline")
            .arg(dir.join("var/lib/agentic-rigg/baseline.json"))
            .env_remove("NOTIFY_SOCKET")
            .env_remove("JOURNAL_STREAM")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start agentic-rigg-helper");
        let t0 = Instant::now();
        while !socket.exists() {
            assert!(t0.elapsed() < Duration::from_secs(5), "socketen kom ikke");
            std::thread::sleep(Duration::from_millis(10));
        }
        Helper { child, dir, socket }
    }

    fn connect(&self) -> Conn {
        Conn::new(&self.socket)
    }

    fn term(&mut self) -> bool {
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(5) {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    next: u64,
}

impl Conn {
    fn new(p: &Path) -> Conn {
        let s = UnixStream::connect(p).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Conn {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
            next: 1,
        }
    }

    fn call(&mut self, op: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.w
            .write_all(format!("{}\n", json!({"id": id, "op": op, "args": args})).as_bytes())
            .unwrap();
        let mut line = String::new();
        self.r.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or(Value::Null)
    }

    fn ok(&mut self, op: &str, args: Value) -> Value {
        let v = self.call(op, args);
        assert_eq!(v["ok"], json!(true), "{op}: {v}");
        v["result"].clone()
    }

    fn err(&mut self, op: &str, args: Value) -> Value {
        let v = self.call(op, args);
        assert_eq!(v["ok"], json!(false), "{op} skulle feile: {v}");
        v["error"].clone()
    }

    fn hw(&mut self) -> Value {
        self.ok("status", json!({}))["hardware"].clone()
    }
}

#[test]
fn trinn_som_ikke_er_godkjent_avvises() {
    let h = Helper::start("trinn", my_uid(), &[], "sim");
    let mut c = h.connect();
    let s = c.ok("status", json!({}));
    assert_eq!(s["steps"], json!([]));
    assert_eq!(s["uid"], my_uid());
    for p in [
        json!({"powerLimitW": 200}),
        json!({"fan": {"mode": "manual", "manualPercent": 60}}),
        json!({"clockOffsets": {"coreMhz": 50}}),
    ] {
        let e = c.err("apply", json!({"settings": p}));
        assert_eq!(e["code"], "step_not_enabled", "{p}");
    }
    assert_eq!(
        c.err("rgb_set", json!({"rgb": {}}))["code"],
        "step_not_enabled"
    );
    assert_eq!(c.hw()["stock"], true);
    assert_eq!(c.hw()["writes"], 0, "ingenting er skrevet");
}

#[test]
fn godkjent_trinn_gir_bruk_og_behold() {
    let h = Helper::start("behold", my_uid(), &["effekt"], "sim");
    let mut c = h.connect();
    let r = c.ok(
        "apply",
        json!({"settings": {"powerLimitW": 200, "tempTargetC": 80}}),
    );
    let txn = r["txn"].as_str().unwrap().to_string();
    assert_eq!(c.hw()["applied"]["powerLimitW"], 200.0);
    c.ok("confirm", json!({"txn": txn}));
    let s = c.ok("status", json!({}));
    assert_eq!(s["state"]["confirmed"]["powerLimitW"], 200.0);
    assert!(s["state"]["pending"].is_null());
    assert!(
        s["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["status"] == "confirmed")
    );
    // Et trinn som ikke er godkjent, kommer ikke med selv sammen med et som er det.
    assert_eq!(
        c.err(
            "apply",
            json!({"settings": {"powerLimitW": 210, "fan": {"mode": "manual"}}})
        )["code"],
        "step_not_enabled"
    );
    // Tilbakestill alt: standard med en gang.
    c.ok("reset", json!({}));
    assert_eq!(c.hw()["stock"], true);
}

#[test]
fn fristen_ruller_tilbake() {
    let h = Helper::start("frist", my_uid(), &["vifter"], "sim");
    let mut c = h.connect();
    c.ok("apply", json!({"settings": {"fan": {"mode": "manual", "manualPercent": 70}}, "confirmTimeoutS": 10}));
    assert_eq!(c.hw()["fanNow"], 70.0);
    let t0 = Instant::now();
    loop {
        let s = c.ok("status", json!({}));
        if s["state"]["pending"].is_null() {
            assert!(
                s["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["reason"] == "timeout"),
                "{s}"
            );
            assert!(
                s["hardware"]["fanNow"].is_null(),
                "viftene tilbake til Auto"
            );
            break;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(14),
            "fristen gikk ikke ut"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(t0.elapsed() >= Duration::from_secs(9));
}

#[test]
fn feil_uid_avvises() {
    let h = Helper::start("uid", my_uid() + 1, &["effekt"], "sim");
    let mut c = h.connect();
    let v = c.call("status", json!({}));
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "permission_denied");
    // Forbindelsen er lukket etterpå.
    let mut line = String::new();
    assert_eq!(c.r.read_line(&mut line).unwrap_or(0), 0);
}

#[test]
fn verdier_utenfor_kortets_omrade_avvises() {
    let h = Helper::start("omrade", my_uid(), &["effekt", "vifter", "klokker"], "sim");
    let mut c = h.connect();
    for p in [
        json!({"powerLimitW": 400}),
        json!({"powerLimitW": 10}),
        json!({"tempTargetC": 100}),
        json!({"clockOffsets": {"coreMhz": 1200}}),
        json!({"clockOffsets": {"memMhz": -1500}}),
        json!({"fan": {"manualPercent": 10}}),
        json!({"fan": {"curve": [[30, 0], [80, 100]]}}),
        json!({"fan": {"curve": [[30, 40], [140, 100]]}}),
    ] {
        let e = c.err("apply", json!({"settings": p}));
        assert_eq!(e["code"], "invalid_argument", "{p}");
    }
    let e = c.err("apply", json!({"settings": {"powerLimitW": 400}}));
    assert_eq!(e["details"]["reason"], "out_of_range");
    assert_eq!(e["details"]["max"], 275.0);
    assert_eq!(c.hw()["writes"], 0);
    // Ingen vilkårlige operasjoner, stier eller felt.
    assert_eq!(
        c.err("exec", json!({"cmd": "/bin/sh"}))["code"],
        "unknown_op"
    );
    assert_eq!(
        c.err(
            "apply",
            json!({"settings": {"i2c": {"bus": 3, "addr": 73}}})
        )["code"],
        "invalid_argument"
    );
}

#[test]
fn klienten_borte_gir_standard() {
    let h = Helper::start("borte", my_uid(), &["effekt", "vifter"], "sim");
    let mut a = h.connect();
    let r = a.ok(
        "apply",
        json!({"settings": {"powerLimitW": 220, "fan": {"mode": "manual", "manualPercent": 65}}}),
    );
    a.ok("confirm", json!({"txn": r["txn"]}));
    let mut b = h.connect();
    assert_eq!(b.hw()["stock"], false);
    drop(a);
    let t0 = Instant::now();
    while b.hw()["stock"] != true {
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "kortet gikk ikke til standard"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(b.hw()["fanNow"].is_null());
}

#[test]
fn nodregelen_over_85_grader() {
    let h = Helper::start("nod", my_uid(), &["vifter"], "sim");
    let mut c = h.connect();
    c.ok(
        "apply",
        json!({"settings": {"fan": {"mode": "manual", "manualPercent": 40}}}),
    );
    let r = c.ok("sim_temp", json!({"celsius": 87}));
    assert_eq!(r["emergency"], true);
    let s = c.ok("status", json!({}));
    assert_eq!(s["hardware"]["fanNow"], 100.0);
    assert!(
        s["state"]["pending"].is_null(),
        "ventende endring rullet tilbake"
    );
    c.ok("sim_temp", json!({"celsius": 70}));
    assert!(c.hw()["fanNow"].is_null());
}

#[test]
fn trinn_leses_pa_nytt_og_kan_trekkes_tilbake() {
    let mut h = Helper::start("reload", my_uid(), &[], "sim");
    let mut c = h.connect();
    assert_eq!(
        c.err("apply", json!({"settings": {"powerLimitW": 200}}))["code"],
        "step_not_enabled"
    );
    write_config(&h.dir, my_uid(), &["effekt"]);
    // Skrivende operasjoner leser helper.json før de svarer.
    let r = c.ok("apply", json!({"settings": {"powerLimitW": 200}}));
    c.ok("confirm", json!({"txn": r["txn"]}));
    write_config(&h.dir, my_uid(), &[]);
    let t0 = Instant::now();
    while c.hw()["stock"] != true {
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "trinnet ble ikke trukket tilbake"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // SIGTERM: socketen fjernes.
    assert!(h.term());
    assert!(!h.socket.exists());
}

#[test]
fn socketen_er_privat_og_meldinger_har_grense() {
    let h = Helper::start("sock", my_uid(), &[], "sim");
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(&h.socket).unwrap();
    assert_eq!(m.mode() & 0o777, 0o600);
    let mut c = h.connect();
    c.w.write_all(&vec![b'x'; 20_000]).unwrap();
    c.w.write_all(b"\n").unwrap();
    let mut line = String::new();
    c.r.read_line(&mut line).unwrap();
    assert!(line.contains("message_too_large"), "{line}");
    let mut d = h.connect();
    assert_eq!(d.call("status", json!(3))["error"]["code"], "bad_request");
}

#[test]
fn oppsett_andre_kan_skrive_i_avvises() {
    let dir = tmpdir("modus");
    write_config(&dir, my_uid(), &["effekt"]);
    std::fs::set_permissions(
        dir.join("etc/agentic-rigg/helper.json"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_agentic-rigg-helper"))
        .args(["--backend", "sim", "--socket"])
        .arg(dir.join("run/agentic-rigg/helper.sock"))
        .arg("--config")
        .arg(dir.join("etc/agentic-rigg/helper.json"))
        .arg("--baseline")
        .arg(dir.join("var/lib/agentic-rigg/baseline.json"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("kan skrives av andre"));
    // Ukjent trinn i fila: starter ikke.
    std::fs::set_permissions(
        dir.join("etc/agentic-rigg/helper.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::fs::write(
        dir.join("etc/agentic-rigg/helper.json"),
        r#"{"versjon":1,"uid":1000,"trinn":["alt"]}"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_agentic-rigg-helper"))
        .args(["--backend", "sim", "--config"])
        .arg(dir.join("etc/agentic-rigg/helper.json"))
        .arg("--baseline")
        .arg(dir.join("var/lib/agentic-rigg/baseline.json"))
        .arg("--socket")
        .arg(dir.join("s.sock"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Eldre v1-oppsett kan ikke gi full GPU-autorisasjon. Hjelperen svarer på
/// `status`, men åpner ikke NVML og avviser skriving.
#[test]
fn legacyoppsett_gir_ikke_ekte_skrivetilgang() {
    let h = Helper::start("nvml", my_uid(), &["effekt", "vifter", "klokker"], "nvml");
    let mut c = h.connect();
    let s = c.ok("status", json!({}));
    assert_eq!(s["helper"]["backend"], "nvml");
    assert_eq!(s["helper"]["realWrites"], false);
    assert!(s["hardware"].is_null());
    let e = c.err("apply", json!({"settings": {"powerLimitW": 200}}));
    assert_eq!(e["code"], "write_locked");
    assert_eq!(e["details"]["reason"], "real_writes_off");
    assert_eq!(
        c.err("sim_temp", json!({"celsius": 90}))["code"],
        "unknown_op"
    );
    // Prosessen har ikke lastet NVML.
    let maps = std::fs::read_to_string(format!("/proc/{}/maps", h.child.id())).unwrap();
    assert!(!maps.contains("libnvidia-ml"), "NVML er lastet");
}

#[test]
fn tilbakestill_gjor_ingenting_uten_registrerte_endringer() {
    let dir = tmpdir("tilb");
    write_config(&dir, my_uid(), &[]);
    let out = Command::new(env!("CARGO_BIN_EXE_agentic-rigg-helper"))
        .args(["--backend", "nvml", "--tilbakestill", "--config"])
        .arg(dir.join("etc/agentic-rigg/helper.json"))
        .arg("--baseline")
        .arg(dir.join("baseline.json"))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Ingen GPU-kall utføres"));
    assert!(!dir.join("baseline.json").exists());
    assert!(!dir.join("dirty.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
