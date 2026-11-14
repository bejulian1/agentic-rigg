//! Kontrakttestene (arkitektur.md kap. 11): `agentic-riggd --backend sim`
//! som barneprosess med egne mapper. Ingen systemd, ingen ekte socket, og
//! ingenting leses fra skjermkortet (sim). `/proc` og `/sys` leses som vanlig.
#![allow(clippy::disallowed_methods)] // testene starter tjenesten som barneprosess

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Daemon {
    child: Child,
    dir: PathBuf,
    socket: PathBuf,
}

fn tmpdir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "riggd-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

impl Daemon {
    fn start_in(dir: &Path, backend: &str, grace_ms: u32) -> Daemon {
        let socket = dir.join("run/riggd.sock");
        // En aktivert hjelper på verten må aldri bli del av kontrakttesten.
        let helper_root = dir.join("tom-hjelper-rot");
        std::fs::create_dir_all(&helper_root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_agentic-riggd"))
            .args(["--backend", backend, "--socket"])
            .arg(&socket)
            .arg("--hjelper-rot")
            .arg(&helper_root)
            .args(["--grace-ms", &grace_ms.to_string()])
            .env("RIGG_CONFIG_DIR", dir.join("config"))
            .env("RIGG_STATE_DIR", dir.join("state"))
            .env_remove("NOTIFY_SOCKET")
            .env_remove("RIGG_PROFIL")
            .env_remove("RIGG_HJELPER_ROT")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start agentic-riggd");
        let t0 = Instant::now();
        while !socket.exists() {
            assert!(t0.elapsed() < Duration::from_secs(5), "socketen kom ikke");
            std::thread::sleep(Duration::from_millis(10));
        }
        Daemon {
            child,
            dir: dir.to_path_buf(),
            socket,
        }
    }

    fn start(backend: &str) -> Daemon {
        Daemon::start_in(&tmpdir("k"), backend, 60_000)
    }

    fn connect(&self) -> Conn {
        Conn::new(&self.socket)
    }

    fn wait_exit(&mut self, max: Duration) -> bool {
        let t0 = Instant::now();
        while t0.elapsed() < max {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn term(&mut self) {
        // SAFETY: signal til vår egen barneprosess.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        assert!(
            self.wait_exit(Duration::from_secs(5)),
            "stoppet ikke på SIGTERM"
        );
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    part: String,
    next: u64,
    events: Vec<(Instant, Value)>,
}

impl Conn {
    fn new(p: &Path) -> Conn {
        let s = UnixStream::connect(p).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Conn {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
            part: String::new(),
            next: 1,
            events: vec![],
        }
    }

    /// Neste linje, eller `None` når linja er stille i 5 s (lesefristen).
    /// En halv linje fra før fristen tas vare på til neste gang.
    fn line(&mut self) -> Option<Value> {
        match self.r.read_line(&mut self.part) {
            Ok(0) => None,
            Ok(_) if self.part.ends_with('\n') => {
                let v = serde_json::from_str(&self.part).ok();
                self.part.clear();
                v
            }
            Ok(_) | Err(_) => None,
        }
    }

    /// Svaret på et kall: venter lenger enn lesefristen, så en treg runde
    /// under høy last ikke feiler testen (høyst 30 s).
    fn reply(&mut self) -> Value {
        let t0 = Instant::now();
        loop {
            if let Some(v) = self.line() {
                return v;
            }
            assert!(t0.elapsed() < Duration::from_secs(30), "svar fra tjenesten");
        }
    }

    fn raw(&mut self, bytes: &[u8]) -> Option<Value> {
        self.w.write_all(bytes).unwrap();
        loop {
            let v = self.line()?;
            if v.get("event").is_some() {
                self.events.push((Instant::now(), v));
                continue;
            }
            return Some(v);
        }
    }

    fn call(&mut self, op: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = format!("{}\n", json!({"id": id, "op": op, "args": args}));
        self.w.write_all(msg.as_bytes()).unwrap();
        loop {
            let v = self.reply();
            if v.get("event").is_some() {
                self.events.push((Instant::now(), v));
                continue;
            }
            assert_eq!(v["id"], json!(id));
            return v;
        }
    }

    fn ok(&mut self, op: &str, args: Value) -> Value {
        let v = self.call(op, args);
        assert_eq!(v["ok"], json!(true), "{op}: {v}");
        v["result"].clone()
    }

    fn err(&mut self, op: &str, args: Value) -> String {
        let v = self.call(op, args);
        assert_eq!(v["ok"], json!(false), "{op} skulle feile: {v}");
        v["error"]["code"].as_str().unwrap().to_string()
    }

    fn hello(&mut self) -> Value {
        let result = self.ok("hello", json!({"protocol": [1], "client": "kontrakt"}));
        assert_eq!(
            result["capabilities"]["write"]["helper"]["state"], "off",
            "Testen skal bare se sin tomme hjelperrot"
        );
        result
    }

    fn wait_event(&mut self, name: &str, max: Duration) -> Option<Value> {
        let t0 = Instant::now();
        while t0.elapsed() < max {
            if let Some(i) = self.events.iter().position(|(_, e)| e["event"] == name) {
                return Some(self.events.remove(i).1);
            }
            let v = self.line()?;
            if v.get("event").is_some() {
                self.events.push((Instant::now(), v));
            }
        }
        None
    }
}

#[test]
fn hello_kommer_først_og_sier_hva_tjenesten_kan() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    assert_eq!(c.err("ping", json!({})), "hello_required");
    assert_eq!(
        c.err("hello", json!({"protocol": [2]})),
        "unsupported_protocol"
    );
    let h = c.hello();
    assert_eq!(h["protocol"], 1);
    assert_eq!(h["daemon"]["name"], "agentic-riggd");
    assert_eq!(h["daemon"]["version"], rigg_core_version());
    assert_eq!(h["daemon"]["backend"], "sim");
    assert_eq!(h["daemon"]["mock"], true);
    let ops: Vec<&str> = h["capabilities"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for op in [
        "get_static",
        "get_history",
        "subscribe",
        "set_focus",
        "log_start",
        "shutdown_when_idle",
        "apply",
    ] {
        assert!(ops.contains(&op), "{op}");
    }
    assert_eq!(
        h["capabilities"]["write"]["steps"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(c.ok("ping", json!({}))["t"].as_i64().unwrap() > 0);
}

fn rigg_core_version() -> String {
    // Samme kilde som tjenesten: app/version.json.
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../version.json"))
            .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["version"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn uten_nvml_skrivetrinn_er_skriving_låst() {
    let d = Daemon::start("ingen");
    let mut c = d.connect();
    let h = c.hello();
    let ops = h["capabilities"]["ops"].as_array().unwrap();
    for w in [
        "apply",
        "set_power_limit",
        "set_fan",
        "confirm",
        "revert",
        "reset",
        "rgb_set",
    ] {
        assert!(
            !ops.iter().any(|o| o == w),
            "{w} skal ikke stå i ops uten sim"
        );
        assert_eq!(c.err(w, json!({"gpu": "0"})), "write_locked");
    }
    assert_eq!(h["capabilities"]["write"]["state"], "locked");
    let v = c.call("apply", json!({}));
    assert!(v["error"]["message"].as_str().unwrap().contains("låst"));
}

#[test]
fn leseoperasjonene_svarer() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    c.hello();
    // Kortlista fylles av første runde i prøvetråden; under høy last kan den
    // komme etter at vi spør (sett i runde 8A med lastsnitt 12).
    let t0 = Instant::now();
    let gpus = loop {
        let g = c.ok("list_gpus", json!({}));
        if g["gpus"][0]["id"].is_string() || t0.elapsed() > Duration::from_secs(25) {
            break g;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(gpus["gpus"][0]["id"], "0000:1c:00.0");
    // Første prøve kommer straks; telemetrien har demoens feltnavn.
    let t0 = Instant::now();
    let tele = loop {
        let t = c.ok("get_telemetry", json!({}));
        if !t.is_null() {
            break t;
        }
        assert!(t0.elapsed() < Duration::from_secs(3), "ingen prøve");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(tele["gpus"]["0000:1c:00.0"]["temp"].is_number());
    assert!(tele["sys"]["cpu"]["coreLoad"].is_array());
    let st = c.ok("get_static", json!({"gpu": "0000:1c:00.0"}));
    assert_eq!(st["static"]["name"], "NVIDIA GeForce RTX 2080");
    assert_eq!(
        c.err("get_static", json!({"gpu": "0000:99:00.0"})),
        "unknown_gpu"
    );
    assert!(c.ok("get_system", json!({})).is_object());
    assert_eq!(c.ok("get_limits", json!({}))["powerLimitW"]["max"], 275.0);
    assert!(c.ok("get_state", json!({}))["active"].is_object());
    let s = c.ok("get_settings", json!({}));
    assert_eq!(s["background"], false, "bakgrunn er av som standard");
    assert_eq!(s["backgroundIntervalMs"], 2000);
    assert_eq!(
        c.err("set_settings", json!({"backgroundIntervalMs": 3000})),
        "invalid_argument"
    );
    let s = c.ok("set_settings", json!({"backgroundIntervalMs": 5000}));
    assert_eq!(s["backgroundIntervalMs"], 5000);
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(d.dir.join("config/tjeneste.json")).unwrap())
            .unwrap();
    assert_eq!(saved["backgroundIntervalMs"], 5000);
    std::thread::sleep(Duration::from_millis(1200));
    let stats = c.ok("get_stats", json!({"keys": ["gpu:0000:1c:00.0/temp"]}));
    assert!(stats["stats"]["gpu:0000:1c:00.0/temp"]["max"].is_number());
    let r = c.ok("reset_stats", json!({}));
    assert!(r["stats"].as_object().unwrap().is_empty());
    let h = c.ok(
        "get_history",
        json!({"window": 60, "keys": ["gpu:0000:1c:00.0/temp", "cpu/load"], "points": 60}),
    );
    assert!(!h["t"].as_array().unwrap().is_empty());
    assert_eq!(
        h["series"]["gpu:0000:1c:00.0/temp"]["max"]
            .as_array()
            .unwrap()
            .len(),
        h["t"].as_array().unwrap().len()
    );
    assert_eq!(
        c.err("get_history", json!({"window": 61, "keys": ["cpu/load"]})),
        "invalid_argument"
    );
    assert_eq!(
        c.err("get_history", json!({"window": 60, "keys": ["a b"]})),
        "invalid_argument"
    );
    let many: Vec<String> = (0..17).map(|i| format!("k{i}")).collect();
    assert_eq!(
        c.err("get_history", json!({"window": 60, "keys": many})),
        "invalid_argument"
    );
    // Loggen (2B): alltid til en midlertidig mappe, aldri til ~/Dokumenter. Mer i tests/logg.rs.
    assert_eq!(
        c.ok(
            "log_start",
            json!({"dir": d.dir.join("logg").display().to_string()})
        )["active"],
        true
    );
    assert_eq!(c.ok("log_stop", json!({}))["active"], false);
    assert_eq!(c.ok("log_status", json!({}))["active"], false);
    assert_eq!(c.err("finnes_ikke", json!({})), "unknown_op");
    let p = c.ok("ping", json!({}));
    assert!(p["rssKB"].as_u64().unwrap() > 0);
}

#[test]
fn feilkoder_og_grenser() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    c.hello();
    let v = c.raw(b"{ikke json\n").unwrap();
    assert_eq!(v["error"]["code"], "bad_json");
    assert_eq!(v["id"], Value::Null);
    let v = c.raw(b"[1,2]\n").unwrap();
    assert_eq!(v["error"]["code"], "bad_request");
    let v = c.raw(b"{\"id\":5,\"op\":\"ping\",\"args\":7}\n").unwrap();
    assert_eq!(v["error"]["code"], "bad_request");
    assert_eq!(v["id"], 5);
    assert!(
        c.ok("ping", json!({}))["t"].is_number(),
        "forbindelsen lever etter feil"
    );
    // Over 64 KiB uten linjeskift: feil og lukket forbindelse.
    let big = vec![b'x'; 70_000];
    let v = c.raw(&big).unwrap();
    assert_eq!(v["error"]["code"], "message_too_large");
    assert!(c.line().is_none(), "forbindelsen er lukket");
}

#[test]
fn socketen_er_privat_og_høyst_16_klienter() {
    let d = Daemon::start("sim");
    let mode = std::fs::metadata(&d.socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let dmode = std::fs::metadata(d.socket.parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dmode, 0o700);
    let mut conns: Vec<Conn> = (0..16).map(|_| d.connect()).collect();
    for c in &mut conns {
        c.hello();
    }
    let mut extra = d.connect();
    let v = extra.line().expect("svar til klient nummer 17");
    assert_eq!(v["error"]["code"], "too_many_clients");
}

#[test]
fn abonnement_gir_prøver_men_aldri_over_2_hz() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    c.hello();
    let r = c.ok(
        "subscribe",
        json!({"topics": ["telemetry", "device"], "intervalMs": 100}),
    );
    assert_eq!(r["intervalMs"], 500, "klemt til 500 ms");
    let mut times = Vec::new();
    while times.len() < 6 {
        let e = c
            .wait_event("telemetry", Duration::from_secs(3))
            .expect("telemetri");
        times.push((Instant::now(), e["data"]["intervalMs"].as_u64().unwrap()));
    }
    // De første kan komme tett (prøven før abonnementet); se på de siste.
    for w in times[2..].windows(2) {
        let dt = w[1].0.duration_since(w[0].0);
        assert!(dt >= Duration::from_millis(400), "to prøver på {dt:?}");
    }
    assert_eq!(times.last().unwrap().1, 500);
    let f = c.ok("set_focus", json!({"pages": []}));
    assert_eq!(f["effectiveMs"], 2000, "minimert vindu gir sakte prøver");
    c.ok("unsubscribe", json!({"topics": ["telemetry"]}));
}

#[test]
fn stopper_etter_fristen_når_siste_klient_er_borte() {
    let dir = tmpdir("grace");
    let mut d = Daemon::start_in(&dir, "sim", 1500);
    {
        let mut c = d.connect();
        c.hello();
        std::thread::sleep(Duration::from_millis(1800));
        assert!(
            d.child.try_wait().unwrap().is_none(),
            "lever mens klienten er der"
        );
    }
    assert!(
        d.wait_exit(Duration::from_secs(5)),
        "stoppet ikke etter fristen"
    );
    assert!(!d.socket.exists(), "socketen er slettet");
}

#[test]
fn bakgrunn_på_holder_tjenesten_i_live() {
    let dir = tmpdir("bg");
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(
        dir.join("config/tjeneste.json"),
        r#"{"background": true, "backgroundIntervalMs": 1000}"#,
    )
    .unwrap();
    let mut d = Daemon::start_in(&dir, "sim", 800);
    std::thread::sleep(Duration::from_millis(2000));
    assert!(
        d.child.try_wait().unwrap().is_none(),
        "bakgrunn på: tjenesten stopper ikke"
    );
    let mut c = d.connect();
    c.hello();
    c.ok("shutdown_when_idle", json!({}));
    drop(c);
    assert!(
        d.wait_exit(Duration::from_secs(4)),
        "stopper når siste klient er borte etter shutdown_when_idle"
    );
}

#[test]
fn sigterm_lagrer_og_historikken_overlever_omstart() {
    let dir = tmpdir("term");
    let mut d = Daemon::start_in(&dir, "sim", 60_000);
    let mut c = d.connect();
    c.hello();
    std::thread::sleep(Duration::from_millis(2500));
    let before = c.ok(
        "get_history",
        json!({"window": 60, "keys": ["gpu:0000:1c:00.0/temp"], "points": 60}),
    );
    let n0 = before["t"].as_array().unwrap().len();
    assert!(n0 >= 1);
    c.ok("subscribe", json!({"topics": ["telemetry"]}));
    d.term();
    assert!(
        c.wait_event("shutdown", Duration::from_secs(2)).is_some(),
        "shutdown-hendelsen kom"
    );
    assert!(
        dir.join("state/historikk.bin").exists(),
        "historikken er lagret"
    );
    assert!(
        dir.join("state/minmaks.json").exists(),
        "min og maks er lagret"
    );
    assert!(!d.socket.exists());
    // Ny tjeneste i samme mapper: de gamle punktene er der.
    let d2 = Daemon::start_in(&dir, "sim", 60_000);
    std::mem::forget(d); // mappa eies nå av d2
    let mut c2 = d2.connect();
    c2.hello();
    let after = c2.ok(
        "get_history",
        json!({"window": 60, "keys": ["gpu:0000:1c:00.0/temp"], "points": 60}),
    );
    assert!(
        after["t"].as_array().unwrap().len() >= n0,
        "historikken overlevde omstart"
    );
}

// ---------------------------------------------------------------- runde 3A

/// Alle skriveoperasjonene med argumenter som ville vært gyldige mot sim.
fn write_calls() -> Vec<(&'static str, Value)> {
    vec![
        (
            "apply",
            json!({"settings": {"powerLimitW": 150, "fan": {"mode": "manual", "manualPercent": 80}}, "confirmTimeoutS": 10}),
        ),
        ("set_power_limit", json!({"watts": 150})),
        ("set_clock_offsets", json!({"coreMhz": 100, "memMhz": 200})),
        ("set_temp_target", json!({"celsius": 75})),
        ("set_fan", json!({"mode": "manual", "manualPercent": 80})),
        ("confirm", json!({"txn": "t1"})),
        ("revert", json!({"txn": "t1"})),
        ("reset", json!({})),
        ("rgb_get", json!({})),
        (
            "rgb_set",
            json!({"rgb": {"effect": "static", "color": "#ff0000"}}),
        ),
    ]
}

/// Mot det ekte kortet (`--backend nvml`, bare lesing) og uten kort svarer
/// **alle** skriveoperasjonene `write_locked`, også med gyldige argumenter,
/// og ingenting venter etterpå. Uten NVIDIA-driver blir nvml `no_driver`,
/// og svaret er det samme.
#[test]
fn skriving_er_låst_mot_det_ekte_kortet() {
    for backend in ["nvml", "ingen"] {
        let d = Daemon::start(backend);
        let mut c = d.connect();
        let h = c.hello();
        assert_eq!(h["capabilities"]["write"]["state"], "locked", "{backend}");
        let ops: Vec<&str> = h["capabilities"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(write_calls().len(), 10, "alle skriveoperasjonene er med");
        c.ok("subscribe", json!({"topics": ["txn", "device"]}));
        for (op, args) in write_calls() {
            assert!(!ops.contains(&op), "{backend}: {op} skal ikke stå i ops");
            assert_eq!(c.err(op, args), "write_locked", "{backend}/{op}");
        }
        // Uten kort (`ingen`, eller nvml uten driver) er svaret `unknown_gpu`.
        let st = c.call("get_state", json!({}));
        if st["ok"] == true {
            let st = &st["result"];
            assert!(st["pending"].is_null(), "{backend}: ingenting venter");
            assert!(
                st["active"]["powerLimitW"].as_f64() != Some(150.0),
                "{backend}: effektgrensen er ikke endret"
            );
        } else {
            assert_eq!(st["error"]["code"], "unknown_gpu", "{backend}");
        }
        assert!(
            c.wait_event("txn", Duration::from_millis(300)).is_none(),
            "{backend}: ingen txn-hendelse"
        );
    }
}

/// Bruk → Behold, Bruk → Rull tilbake, frakobling og fristen, ende til ende
/// mot `--backend sim`. Prøven viser det som er i bruk.
#[test]
fn bruk_behold_og_rull_tilbake_mot_sim() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    let h = c.hello();
    assert_eq!(h["capabilities"]["write"]["state"], "sim");
    c.ok("subscribe", json!({"topics": ["telemetry", "txn"]}));
    let gpu = "0000:1c:00.0";
    let limit = |c: &mut Conn| -> f64 {
        // Neste prøve etter endringen.
        c.events.clear();
        let e = c
            .wait_event("telemetry", Duration::from_secs(3))
            .expect("telemetri");
        e["data"]["gpus"][gpu]["powerLimit"].as_f64().unwrap()
    };
    assert_eq!(limit(&mut c), 245.0);

    // Bruk → Behold.
    let r = c.ok(
        "apply",
        json!({"settings": {"powerLimitW": 200}, "confirmTimeoutS": 30}),
    );
    let t1 = r["txn"].as_str().unwrap().to_string();
    assert_eq!(r["pending"]["timeoutS"], 30);
    let st = c.ok("get_state", json!({}));
    assert_eq!(st["pending"]["txn"], t1.as_str());
    assert_eq!(st["active"]["powerLimitW"], 200.0);
    assert_eq!(st["confirmed"]["powerLimitW"], 245.0);
    assert_eq!(limit(&mut c), 200.0, "prøven viser det som er i bruk");
    // En annen klient kan verken bruke eller bekrefte.
    let mut c2 = d.connect();
    c2.hello();
    c2.ok("subscribe", json!({"topics": ["txn"]}));
    assert_eq!(
        c2.err("apply", json!({"settings": {"powerLimitW": 190}})),
        "busy"
    );
    assert_eq!(c2.err("confirm", json!({"txn": t1})), "not_owner");
    assert_eq!(c.err("confirm", json!({"txn": "t999"})), "txn_mismatch");
    let st = c.ok("confirm", json!({"txn": t1}));
    assert!(st["pending"].is_null());
    assert_eq!(st["confirmed"]["powerLimitW"], 200.0);
    let ev = c2
        .wait_event("txn", Duration::from_secs(2))
        .expect("txn til alle");
    assert_eq!(
        ev["data"],
        json!({"txn": t1, "status": "confirmed", "reason": "confirm"})
    );

    // Bruk → Rull tilbake.
    let t2 = c.ok("apply", json!({"settings": {"powerLimitW": 180}}))["txn"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(t1, t2);
    assert_eq!(
        c.ok("revert", json!({"txn": t2}))["active"]["powerLimitW"],
        200.0
    );
    let ev = c2.wait_event("txn", Duration::from_secs(2)).unwrap();
    assert_eq!(ev["data"]["status"], "rolled_back");
    assert_eq!(ev["data"]["reason"], "revert");
    assert_eq!(limit(&mut c), 200.0);
    assert_eq!(c.err("confirm", json!({"txn": t2})), "no_pending");

    // Klemt til tjenestens policy, med kilde.
    let r = c.ok("set_clock_offsets", json!({"coreMhz": 900}));
    assert_eq!(r["clamped"][0]["source"], "policy");
    assert_eq!(r["active"]["clockOffsets"]["coreMhz"], 300.0);
    let t3 = r["txn"].as_str().unwrap().to_string();
    // Statiske data følger: Kontroller leser offset fra extra.styring.
    let s = c.ok("get_static", json!({"gpu": gpu}));
    assert_eq!(
        s["static"]["extra"]["styring"]["coreOffsetMHz"]["value"],
        300.0
    );

    // Klienten kobler fra: det den ikke har bekreftet, rulles tilbake straks.
    drop(c);
    let ev = c2
        .wait_event("txn", Duration::from_secs(2))
        .expect("tilbake ved frakobling");
    assert_eq!(
        ev["data"],
        json!({"txn": t3, "status": "rolled_back", "reason": "client_disconnected"})
    );
    assert_eq!(
        c2.ok("get_state", json!({}))["active"]["clockOffsets"]["coreMhz"],
        0.0
    );
    let s = c2.ok("get_static", json!({"gpu": gpu}));
    assert_eq!(
        s["static"]["extra"]["styring"]["coreOffsetMHz"]["value"],
        0.0
    );

    // Fristen (minst 10 s) går ut: tilbake til siste bekreftede.
    let t4 = c2.ok(
        "set_fan",
        json!({"mode": "manual", "manualPercent": 80, "confirmTimeoutS": 1}),
    )["txn"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        c2.ok("get_state", json!({}))["pending"]["timeoutS"],
        10,
        "fristen klemmes til 10 s"
    );
    // `wait_event` gir opp når linja er stille i 5 s; prøv til fristen er godt over.
    let t0 = Instant::now();
    let ev = loop {
        if let Some(e) = c2.wait_event("txn", Duration::from_secs(4)) {
            break e;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(14),
            "tilbake etter fristen"
        );
    };
    assert_eq!(
        ev["data"],
        json!({"txn": t4, "status": "rolled_back", "reason": "timeout"})
    );
    let st = c2.ok("get_state", json!({}));
    assert_eq!(st["active"]["fan"]["mode"], "auto");
    assert_eq!(st["active"]["powerLimitW"], 200.0, "det bekreftede står");

    // Tilbake til standard bekreftes straks.
    assert_eq!(c2.ok("reset", json!({}))["confirmed"]["powerLimitW"], 245.0);
}

/// Viftene i prøven følger den manuelle farten med sim, og viftestyringen
/// står i prøven (Oversikt, Kontroller og Viftekurve).
#[test]
fn viftestyringen_står_i_prøven() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    c.hello();
    c.ok("subscribe", json!({"topics": ["telemetry"]}));
    let gpu = "0000:1c:00.0";
    let e = c.wait_event("telemetry", Duration::from_secs(3)).unwrap();
    assert_eq!(e["data"]["gpus"][gpu]["fanPolicy"], json!([0, 0]));
    assert!(
        e["data"]["gpus"][gpu]["fanTarget"]
            .as_array()
            .is_some_and(|a| a.len() == 2)
    );
    c.ok("set_fan", json!({"mode": "manual", "manualPercent": 72}));
    c.events.clear();
    let e = c.wait_event("telemetry", Duration::from_secs(3)).unwrap();
    assert_eq!(e["data"]["gpus"][gpu]["fanPolicy"], json!([1, 1]));
    assert_eq!(e["data"]["gpus"][gpu]["fans"], json!([72.0, 72.0]));
    let st = c.ok("get_static", json!({"gpu": gpu}));
    let sty = &st["static"]["extra"]["styring"];
    assert_eq!(sty["fanPolicy"], json!([1, 1]));
    assert_eq!(sty["tempTarget"]["value"].as_f64(), Some(83.0));
    assert_eq!(
        (
            sty["tempTarget"]["min"].as_f64(),
            sty["tempTarget"]["max"].as_f64()
        ),
        (Some(65.0), Some(88.0))
    );
    assert_eq!(sty["memOffsetMHz"]["max"], 3000.0);
}

/// `get_processes`: øyeblikksbildet først, så lista prøvetråden leser.
#[test]
fn prosesslista_leses_mens_den_er_bedt_om() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    let h = c.hello();
    assert!(
        h["capabilities"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o == "get_processes")
    );
    let gpu = "0000:1c:00.0";
    // Under last (flere agenter bygger samtidig) kan første runde komme etter hello.
    let t0 = Instant::now();
    while c.ok("list_gpus", json!({}))["gpus"][0]["id"] != gpu {
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "kortet kom ikke i lista"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let first = c.ok("get_processes", json!({"gpu": gpu}));
    assert_eq!(first["gpu"], gpu);
    let t0 = Instant::now();
    let live = loop {
        let p = c.ok("get_processes", json!({"gpu": gpu}));
        if p["live"] == true {
            break p;
        }
        assert!(t0.elapsed() < Duration::from_secs(4), "lista kom ikke: {p}");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(live["processes"]["list"].as_array().unwrap().len(), 3);
    assert!(live["processes"]["t"].as_i64().unwrap() > 0);
    assert_eq!(
        c.err("get_processes", json!({"gpu": "0000:99:00.0"})),
        "unknown_gpu"
    );
}

/// Mens loggen går, er takten minst så rask som loggintervallet, også uten
/// vindu (ønske fra 2B). Etter stopp gjelder bakgrunnstakten igjen.
#[test]
fn loggen_bestemmer_takten() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    c.hello();
    assert_eq!(
        c.ok("ping", json!({}))["intervalMs"],
        2000,
        "ingen abonnent: bakgrunnstakten"
    );
    c.ok(
        "log_start",
        json!({"dir": d.dir.join("logg").display().to_string(), "intervalMs": 500}),
    );
    assert_eq!(c.ok("ping", json!({}))["intervalMs"], 500);
    c.ok("log_stop", json!({}));
    assert_eq!(c.ok("ping", json!({}))["intervalMs"], 2000);
}

/// Kortet faller av bussen (sim `borte`, etter 15 s): det står som `lost` i
/// lista og i `device`, prøvene har det ikke med, og de faste dataene finnes
/// fortsatt. Vinduet beholder valget (`state.rs`).
#[test]
fn kortet_borte_står_i_lista() {
    let d = Daemon::start("sim:borte");
    let mut c = d.connect();
    c.hello();
    let gpu = "0000:1c:00.0";
    // Kortet står i lista etter første runde (under høy last kan den komme
    // etter at socketen finnes). Kortet blir borte 15–25 s etter start.
    let t0 = Instant::now();
    loop {
        let g = c.ok("list_gpus", json!({}));
        if g["gpus"][0]["state"] == "ok" {
            break;
        }
        assert!(
            g["gpus"][0]["state"].is_null() && t0.elapsed() < Duration::from_secs(14),
            "kortet var ikke i lista: {g}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    c.ok("subscribe", json!({"topics": ["device", "telemetry"]}));
    let t0 = Instant::now();
    let ev = loop {
        if let Some(e) = c.wait_event("device", Duration::from_secs(4))
            && e["data"]["gpus"][0]["state"] == "lost"
        {
            break e;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "kortet ble ikke borte"
        );
    };
    assert_eq!(
        ev["data"]["gpus"][0]["id"], gpu,
        "samme ID, ikke fjernet fra lista"
    );
    assert!(
        ev["data"]["gpus"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("GPU_IS_LOST")
    );
    c.events.clear();
    // Neste prøve (hvert sekund, men tregere under høy last).
    let t = c
        .wait_event("telemetry", Duration::from_secs(15))
        .expect("telemetri etter at kortet ble borte");
    assert!(
        t["data"]["gpus"].get(gpu).is_none(),
        "prøven har ikke kortet"
    );
    assert!(
        c.ok("get_static", json!({"gpu": gpu}))["static"].is_object(),
        "de faste dataene står"
    );
}

/// `get_disks` (runde 6A): med sim fem disker med SMART-helse, og prøvene har
/// MB/s og temperatur per disk. Første svar kan være `pending` (lista lages i
/// neste runde).
#[test]
fn diskene_med_helse_og_hastighet() {
    let d = Daemon::start("sim");
    let mut c = d.connect();
    let h = c.hello();
    assert!(
        h["capabilities"]["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o == "get_disks")
    );
    let t0 = Instant::now();
    let list = loop {
        let v = c.ok("get_disks", json!({}));
        if v["pending"] == false {
            break v;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(8),
            "disklista kom ikke: {v}"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(list["source"], "sim");
    let disks = list["disks"].as_array().unwrap();
    assert_eq!(disks.len(), 5);
    let levels: Vec<&str> = disks
        .iter()
        .map(|x| x["health"]["level"].as_str().unwrap())
        .collect();
    assert_eq!(levels, ["ok", "ok", "ok", "warn", "unknown"]);
    assert!(
        disks[1]["smart"]["attributes"].as_array().unwrap().len() > 10,
        "hele SMART-tabellen"
    );
    c.ok("subscribe", json!({"topics": ["telemetry"]}));
    let t = c.wait_event("telemetry", Duration::from_secs(5)).unwrap();
    let sd = t["data"]["sys"]["disks"].as_array().unwrap();
    assert_eq!(sd.len(), 5);
    assert!(sd[0]["id"].as_str().is_some() && sd[0]["temp"].as_f64().is_some());
    // get_system har diskene til rapporten.
    assert_eq!(
        c.ok("get_system", json!({}))["disks"]["disks"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}
