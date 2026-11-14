//! Tjenesten mot root-hjelperen, ende til ende som vanlig bruker (runde 7A):
//! `agentic-rigg-helper --backend sim --socket <tmp> --config <tmp>/helper.json`
//! og `agentic-riggd --backend sim --hjelper-rot <tmp>` som barneprosesser.
//! Ingen root, ingen systemd, ingenting mot det ekte kortet.
#![allow(clippy::disallowed_methods)] // testene starter prosessene selv

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
        "riggd-hj-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    d
}

fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

/// Hjelperen ligger ved siden av tjenesten i målmappa. Den bygges (eller
/// sjekkes) her én gang, så testen aldri kjører en gammel binær.
fn helper_bin() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let p = Path::new(env!("CARGO_BIN_EXE_agentic-riggd")).with_file_name("agentic-rigg-helper");
    BUILT.call_once(|| {
        let target = p.parent().unwrap().parent().unwrap();
        let ok = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "-p", "rigg-helper", "--target-dir"])
            .arg(target)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .unwrap();
        assert!(ok.success(), "kunne ikke bygge rigg-helper");
    });
    p
}

fn write_config(root: &Path, uid: u32, steps: &[&str]) {
    let etc = root.join("etc/agentic-rigg");
    std::fs::create_dir_all(&etc).unwrap();
    std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tmp = etc.join(".tmp");
    std::fs::write(
        &tmp,
        json!({"versjon": 1, "uid": uid, "trinn": steps, "bygg": 17}).to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::rename(&tmp, etc.join("helper.json")).unwrap();
}

struct Proc(Child);

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(p: &Path) {
    let t0 = Instant::now();
    while !p.exists() {
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "{} kom ikke",
            p.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn start_helper(root: &Path, backend: &str) -> Proc {
    let sock = root.join("run/agentic-rigg/helper.sock");
    let c = Command::new(helper_bin())
        .args(["--backend", backend, "--socket"])
        .arg(&sock)
        .arg("--config")
        .arg(root.join("etc/agentic-rigg/helper.json"))
        .arg("--baseline")
        .arg(root.join("var/lib/agentic-rigg/baseline.json"))
        .env_remove("JOURNAL_STREAM")
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&sock);
    Proc(c)
}

fn start_riggd(dir: &Path, backend: &str) -> (Proc, PathBuf) {
    let sock = dir.join("riggd/riggd.sock");
    let c = Command::new(env!("CARGO_BIN_EXE_agentic-riggd"))
        .args(["--backend", backend, "--grace-ms", "60000", "--socket"])
        .arg(&sock)
        .arg("--hjelper-rot")
        .arg(dir.join("rot"))
        .env("RIGG_CONFIG_DIR", dir.join("config"))
        .env("RIGG_STATE_DIR", dir.join("state"))
        .env_remove("NOTIFY_SOCKET")
        .env_remove("RIGG_PROFIL")
        .env_remove("RIGG_HJELPER_ROT")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&sock);
    (Proc(c), sock)
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    next: u64,
    events: Vec<Value>,
}

impl Conn {
    fn new(p: &Path) -> Conn {
        let s = UnixStream::connect(p).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        let mut c = Conn {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
            next: 1,
            events: vec![],
        };
        c.ok("hello", json!({"protocol": [1], "client": "hjelper-test"}));
        c
    }

    fn line(&mut self) -> Value {
        let mut l = String::new();
        self.r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap_or(Value::Null)
    }

    fn call(&mut self, op: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.w
            .write_all(format!("{}\n", json!({"id": id, "op": op, "args": args})).as_bytes())
            .unwrap();
        loop {
            let v = self.line();
            if v.get("event").is_some() {
                self.events.push(v);
                continue;
            }
            assert_eq!(v["id"], json!(id), "{v}");
            return v;
        }
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

    fn wait_txn(&mut self, reason: &str, max: Duration) -> bool {
        let t0 = Instant::now();
        while t0.elapsed() < max {
            if self
                .events
                .iter()
                .any(|e| e["event"] == "txn" && e["data"]["reason"] == reason)
            {
                return true;
            }
            let v = self.line();
            if v.get("event").is_some() {
                self.events.push(v);
            }
        }
        false
    }
}

/// Direkte mot hjelperen (som riggd, men for å se kortet).
fn helper_status(root: &Path) -> Value {
    let s = UnixStream::connect(root.join("run/agentic-rigg/helper.sock")).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut w = s.try_clone().unwrap();
    w.write_all(b"{\"id\":1,\"op\":\"status\"}\n").unwrap();
    let mut l = String::new();
    BufReader::new(s).read_line(&mut l).unwrap();
    serde_json::from_str::<Value>(&l).unwrap()["result"].clone()
}

#[test]
fn trinn_ikke_godkjent_avvises_og_godkjent_gir_bruk_og_behold() {
    let dir = tmpdir("trinn");
    let root = dir.join("rot");
    write_config(&root, uid(), &[]);
    let _h = start_helper(&root, "sim");
    let (_d, sock) = start_riggd(&dir, "sim");
    let mut c = Conn::new(&sock);
    let h = c.ok("hello", json!({"protocol": [1]}));
    assert_eq!(h["capabilities"]["write"]["helper"]["state"], "on");
    assert_eq!(h["capabilities"]["write"]["helper"]["backend"], "sim");
    let e = c.err("set_power_limit", json!({"watts": 200}));
    assert_eq!(e["code"], "step_not_enabled");
    assert_eq!(e["details"]["steps"], json!(["effekt"]));
    assert_eq!(helper_status(&root)["hardware"]["writes"], 0);

    // Eldre sim-fixture gir ett felt lov. Produksjonen bruker globalt v2-oppsett.
    write_config(&root, uid(), &["effekt"]);
    let v = c.ok("get_helper", json!({"refresh": true}));
    assert_eq!(v["steps"], json!(["effekt"]));
    let h = c.ok("hello", json!({"protocol": [1]}));
    let steps = h["capabilities"]["write"]["steps"].as_array().unwrap();
    // Støtte og autorisasjon er uavhengig: sim-backenden støtter begge.
    assert_eq!(
        steps.iter().find(|s| s["id"] == "power").unwrap()["status"],
        "supported"
    );
    assert_eq!(
        steps.iter().find(|s| s["id"] == "clocks").unwrap()["status"],
        "supported"
    );
    assert_eq!(
        c.err("set_clock_offsets", json!({"coreMhz":50}))["code"],
        "step_not_enabled"
    );
    let r = c.ok("set_power_limit", json!({"watts": 200}));
    let txn = r["txn"].as_str().unwrap().to_string();
    assert_eq!(
        helper_status(&root)["hardware"]["applied"]["powerLimitW"],
        200.0
    );
    // En annen økt kan ikke beholde eller endre.
    let mut other = Conn::new(&sock);
    assert_eq!(
        other.err("confirm", json!({"txn": txn}))["code"],
        "not_owner"
    );
    assert_eq!(
        other.err("set_power_limit", json!({"watts": 210}))["code"],
        "busy"
    );
    c.ok("confirm", json!({"txn": txn}));
    let st = c.ok("get_state", json!({}));
    assert_eq!(st["confirmed"]["powerLimitW"], 200.0);
    assert!(st["pending"].is_null());
    // Verdi utenfor kortets område: avvist av hjelperen, gjennom tjenesten.
    let e = c.err("set_power_limit", json!({"watts": 500}));
    assert_eq!(e["code"], "invalid_argument");
    assert_eq!(e["details"]["reason"], "out_of_range");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fristen_ute_ruller_tilbake_gjennom_tjenesten() {
    let dir = tmpdir("frist");
    let root = dir.join("rot");
    write_config(&root, uid(), &["vifter"]);
    let _h = start_helper(&root, "sim");
    let (_d, sock) = start_riggd(&dir, "sim");
    let mut c = Conn::new(&sock);
    c.ok("subscribe", json!({"topics": ["txn"]}));
    c.ok(
        "set_fan",
        json!({"mode": "manual", "manualPercent": 60, "confirmTimeoutS": 10}),
    );
    assert_eq!(helper_status(&root)["hardware"]["fanNow"], 60.0);
    assert!(
        c.wait_txn("timeout", Duration::from_secs(16)),
        "ingen txn-hendelse for fristen: {:?}",
        c.events
    );
    assert!(helper_status(&root)["hardware"]["fanNow"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tjenesten_borte_gir_standard() {
    let dir = tmpdir("borte");
    let root = dir.join("rot");
    write_config(&root, uid(), &["effekt", "vifter"]);
    let _h = start_helper(&root, "sim");
    let (mut d, sock) = start_riggd(&dir, "sim");
    let mut c = Conn::new(&sock);
    let r = c.ok(
        "apply",
        json!({"settings": {"powerLimitW": 230, "fan": {"mode": "manual", "manualPercent": 55}}}),
    );
    c.ok("confirm", json!({"txn": r["txn"]}));
    assert_eq!(helper_status(&root)["hardware"]["stock"], false);
    // Tjenesten krasjer (SIGKILL): hjelperen merker at forbindelsen er borte.
    d.0.kill().unwrap();
    d.0.wait().unwrap();
    let t0 = Instant::now();
    loop {
        let s = helper_status(&root);
        if s["hardware"]["stock"] == true {
            assert!(s["hardware"]["fanNow"].is_null(), "viftene på Auto");
            break;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(3),
            "kortet gikk ikke til standard: {s}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn feil_uid_gir_feil_og_las() {
    let dir = tmpdir("uid");
    let root = dir.join("rot");
    write_config(&root, uid() + 1, &["effekt"]);
    let _h = start_helper(&root, "sim");
    let (_d, sock) = start_riggd(&dir, "sim");
    let mut c = Conn::new(&sock);
    let v = c.ok("get_helper", json!({"refresh": true}));
    assert_eq!(v["state"], "error");
    assert!(
        v["message"].as_str().unwrap().contains("annen bruker"),
        "{v}"
    );
    let e = c.err("set_power_limit", json!({"watts": 200}));
    assert_eq!(e["code"], "write_locked");
    assert_eq!(e["details"]["reason"], "helper_error");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ulike_kort_sendes_ikke_videre() {
    // Hjelperen har det ekte kortet (sperret), tjenesten det simulerte.
    let dir = tmpdir("ulik");
    let root = dir.join("rot");
    write_config(&root, uid(), &["effekt"]);
    let _h = start_helper(&root, "nvml");
    let (_d, sock) = start_riggd(&dir, "sim");
    let mut c = Conn::new(&sock);
    let e = c.err("set_power_limit", json!({"watts": 200}));
    assert_eq!(e["code"], "write_locked");
    assert_eq!(e["details"]["reason"], "backend_mismatch");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn uten_hjelper_som_for_men_med_grunn() {
    let dir = tmpdir("av");
    let (_d, sock) = start_riggd(&dir, "ingen");
    let mut c = Conn::new(&sock);
    let h = c.ok("hello", json!({"protocol": [1]}));
    assert_eq!(h["capabilities"]["write"]["helper"]["state"], "off");
    assert_eq!(h["capabilities"]["write"]["state"], "locked");
    let e = c.err("apply", json!({"settings": {"powerLimitW": 200}}));
    assert_eq!(e["code"], "write_locked");
    assert_eq!(e["details"]["reason"], "helper_off");
    assert!(e["message"].as_str().unwrap().contains("låst"));
    // Med sim og uten hjelper: den simulerte boka som før.
    let dir2 = tmpdir("av-sim");
    let (_d2, sock2) = start_riggd(&dir2, "sim");
    let mut c2 = Conn::new(&sock2);
    let r = c2.ok("set_power_limit", json!({"watts": 200}));
    assert!(r["txn"].is_string());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}
