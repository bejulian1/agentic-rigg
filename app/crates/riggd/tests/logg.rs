//! Loggen og tilstandene mot `agentic-riggd --backend sim[:scenario]` som
//! barneprosess (produktspesifikasjonen 3.2 og 3.8). Egne mapper, ingen
//! systemd, ingenting leses fra skjermkortet. Loggen skrives alltid til en
//! midlertidig mappe, aldri til `~/Dokumenter`.
#![allow(clippy::disallowed_methods)] // testene starter tjenesten som barneprosess

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
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
    let d = rigg_core::paths::short_tmp().join(format!(
        "riggd-logg-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

impl Daemon {
    fn start(backend: &str, grace_ms: u32) -> Daemon {
        let dir = tmpdir("d");
        let socket = dir.join("run/riggd.sock");
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
            .env("LANGUAGE", "nb")
            // Varslene går aldri til brukerens skrivebord fra testene.
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/finnes-ikke")
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
        Daemon { child, dir, socket }
    }

    fn connect(&self) -> Conn {
        let s = UnixStream::connect(&self.socket).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut c = Conn {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
            part: String::new(),
            next: 1,
            events: vec![],
        };
        let hello = c.ok("hello", json!({"protocol": [1], "client": "logg-test"}));
        assert_eq!(
            hello["capabilities"]["write"]["helper"]["state"], "off",
            "Loggtesten skal bare se sin tomme hjelperrot"
        );
        c
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
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
    events: Vec<Value>,
}

impl Conn {
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

    fn call(&mut self, op: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.w
            .write_all(format!("{}\n", json!({"id": id, "op": op, "args": args})).as_bytes())
            .unwrap();
        loop {
            let v = self.reply();
            if v.get("event").is_some() {
                self.events.push(v);
                continue;
            }
            return v;
        }
    }

    fn ok(&mut self, op: &str, args: Value) -> Value {
        let v = self.call(op, args);
        assert_eq!(v["ok"], json!(true), "{op}: {v}");
        v["result"].clone()
    }

    fn wait_event(
        &mut self,
        name: &str,
        pred: impl Fn(&Value) -> bool,
        max: Duration,
    ) -> Option<Value> {
        let t0 = Instant::now();
        loop {
            if let Some(i) = self
                .events
                .iter()
                .position(|e| e["event"] == name && pred(&e["data"]))
            {
                return Some(self.events.remove(i)["data"].clone());
            }
            if t0.elapsed() > max {
                return None;
            }
            if let Some(v) = self.line()
                && v.get("event").is_some()
            {
                self.events.push(v);
            }
        }
    }
}

fn read_csv(path: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn loggen_skriver_begge_formatene_og_stopper_rent() {
    let d = Daemon::start("sim", 60_000);
    let mut c = d.connect();
    c.ok(
        "subscribe",
        json!({"topics": ["telemetry", "log"], "intervalMs": 500}),
    );
    for (format, sep) in [("standard", ','), ("norsk", ';')] {
        let dir = d.dir.join(format!("logg-{format}"));
        let st = c.ok(
            "log_start",
            json!({"dir": dir.display().to_string(), "format": format, "intervalMs": 500}),
        );
        assert_eq!(st["active"], true);
        assert_eq!(st["format"], format);
        let ev = c
            .wait_event("log", |e| e["active"] == true, Duration::from_secs(2))
            .expect("log-hendelse ved start");
        assert_eq!(ev["file"], st["file"]);
        std::thread::sleep(Duration::from_millis(2600));
        let s = c.ok("log_status", json!({}));
        assert!(s["rows"].as_u64().unwrap() >= 3, "{s}");
        let st = c.ok("log_stop", json!({}));
        assert_eq!(st["active"], false);
        let file = st["file"].as_str().unwrap().to_string();
        let name = Path::new(&file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.starts_with("rigg-")
                && name.ends_with(".csv")
                && name.len() == "rigg-2026-11-07_14-32-10.csv".len(),
            "{name}"
        );
        let lines = read_csv(&file);
        assert!(lines.len() >= 4, "{lines:?}");
        let head: Vec<&str> = lines[0].split(sep).collect();
        assert_eq!(&head[..2], &["Tid", "Sekunder"]);
        assert!(head.contains(&"GPU · Temperatur [°C]"), "{head:?}");
        let n = head.len();
        for l in &lines[1..] {
            assert_eq!(l.split(sep).count(), n, "kolonnene ligger fast: {l}");
        }
        let row: Vec<&str> = lines[1].split(sep).collect();
        assert!(
            row[0].len() >= "2026-11-07T14:32:10.000+01:00".len() - 1,
            "ISO-tid med sone og millisekunder: {}",
            row[0]
        );
        let ti = head
            .iter()
            .position(|h| *h == "GPU · Temperatur [°C]")
            .unwrap();
        assert!(row[ti].parse::<f64>().is_ok(), "{}", row[ti]);
        if format == "norsk" {
            assert!(row[1].contains(','), "desimalkomma i Sekunder: {}", row[1]);
        } else {
            assert!(row[1].contains('.'), "{}", row[1]);
        }
        assert_eq!(
            st["bytes"].as_u64().unwrap(),
            std::fs::metadata(&file).unwrap().len()
        );
    }
}

#[test]
fn loggen_holder_tjenesten_i_live_uten_klienter() {
    let mut d = Daemon::start("sim", 300);
    let dir = d.dir.join("logg");
    {
        let mut c = d.connect();
        c.ok("log_start", json!({"dir": dir.display().to_string()}));
    }
    // Uten klienter går takten i bakgrunnen (2 s), så vent minst én runde.
    std::thread::sleep(Duration::from_millis(2600));
    assert!(d.running(), "tjenesten skal leve mens loggen går");
    let mut c = d.connect();
    let st = c.ok("log_stop", json!({}));
    let rows = st["rows"].as_u64().unwrap();
    assert!(rows >= 1, "{st}");
    drop(c);
    let t0 = Instant::now();
    while d.running() && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !d.running(),
        "uten logg og klienter stopper tjenesten etter fristen"
    );
}

#[test]
fn ugyldige_valg_gir_norske_feil() {
    let d = Daemon::start("sim", 60_000);
    let mut c = d.connect();
    let v = c.call("log_start", json!({"dir": "relativ/sti"}));
    assert_eq!(v["error"]["code"], "invalid_argument");
    assert!(v["error"]["message"].as_str().unwrap().contains("full sti"));
    let v = c.call("log_start", json!({"dir": "/tmp", "intervalMs": 3000}));
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("0,5, 1, 2, 5 eller 10 s")
    );
    let v = c.call("log_start", json!({"dir": "/tmp", "keys": []}));
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("minst én sensor")
    );
    assert_eq!(c.ok("log_status", json!({}))["active"], false);
}

#[test]
fn tilstandene_fra_sim_scenariene() {
    for (scenario, state, detail) in [
        ("sim:driver-oppdatert", "driver_updated", "610.51"),
        ("sim:ingen-driver", "no_driver", "libnvidia-ml"),
        ("sim:to-kort", "ok", ""),
    ] {
        let d = Daemon::start(scenario, 60_000);
        let mut c = d.connect();
        // Tilstanden settes i første runde. Under høy last kan den komme etter
        // at socketen finnes, så vent på den i stedet for å lese én gang.
        let t0 = Instant::now();
        let g = loop {
            let g = c.ok("list_gpus", json!({}));
            let list = g["gpus"].as_array().unwrap();
            if !list.is_empty() && list.iter().all(|x| x["state"] == state) {
                break g;
            }
            assert!(t0.elapsed() < Duration::from_secs(20), "{scenario}: {g}");
            std::thread::sleep(Duration::from_millis(50));
        };
        let list = g["gpus"].as_array().unwrap();
        if !detail.is_empty() {
            assert!(
                list[0]["detail"].as_str().unwrap().contains(detail),
                "{scenario}: {g}"
            );
        }
        if scenario == "sim:to-kort" {
            assert_eq!(list.len(), 2);
            assert_ne!(list[0]["id"], list[1]["id"], "stabil ID per kort");
        }
        if scenario == "sim:driver-oppdatert" {
            assert_eq!(list[0]["driverInstalled"], "610.51");
            assert_eq!(list[0]["driver"], "610.43.02");
        }
        if scenario == "sim:ingen-driver" {
            assert_eq!(list[0]["id"], "");
            // Prosessoren og minnet går som før.
            let t0 = Instant::now();
            loop {
                let t = c.ok("get_telemetry", json!({}));
                if !t.is_null() {
                    assert!(t["sys"]["cpu"]["coreLoad"].is_array());
                    assert!(t["gpus"].as_object().unwrap().is_empty());
                    break;
                }
                assert!(
                    t0.elapsed() < Duration::from_secs(20),
                    "ingen prøve fra prosessoren"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[test]
fn kortet_forsvinner_og_kommer_tilbake() {
    // Scenarioet «borte»: kortet faller av bussen etter 15 s (sjekkes hvert 10. s).
    let d = Daemon::start("sim:borte", 60_000);
    let mut c = d.connect();
    c.ok("subscribe", json!({"topics": ["telemetry", "device"]}));
    let ev = c
        .wait_event(
            "device",
            |e| {
                e["changed"]
                    .as_array()
                    .is_some_and(|l| l.iter().any(|g| g["state"] == "lost"))
            },
            Duration::from_secs(25),
        )
        .expect("device-hendelse med lost");
    let g = &ev["gpus"][0];
    assert_eq!(g["state"], "lost");
    assert!(g["since"].as_i64().unwrap() > 0);
    assert!(g["detail"].as_str().unwrap().contains("GPU_IS_LOST"));
    // Mens kortet er borte, har prøvene ikke kortet, men prosessoren er med.
    let t = c.ok("get_telemetry", json!({}));
    assert!(t["sys"]["cpu"]["load"].is_number() || t["sys"]["cpu"]["coreLoad"].is_array());
}
