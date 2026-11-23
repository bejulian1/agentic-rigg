//! Egen daemon og blokkert, falsk RGB-socket under privat rot. Ingen root,
//! vertshjelper, I2C, NVML-backend eller installasjon brukes av prøven.
#![allow(clippy::disallowed_methods)] // starter bare testens egen daemon

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    child: Option<Child>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn send(stream: &mut UnixStream, id: u64, op: &str, args: Value) {
    writeln!(stream, "{}", json!({"id":id,"op":op,"args":args})).unwrap();
}

fn receive(reader: &mut BufReader<UnixStream>) -> Value {
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        let response: Value = serde_json::from_str(&line).unwrap();
        if response.get("id").is_some() {
            return response;
        }
    }
}

#[test]
fn ventende_rgb_rpc_sperrer_ikke_daemonens_ping_eller_hello_gate() {
    let root = rigg_core::paths::short_tmp().join(format!("riggd-rgb-isolert-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut fixture = Fixture {
        root: root.clone(),
        child: None,
    };
    let helper_root = root.join("helper");
    std::fs::create_dir(&helper_root).unwrap();
    std::fs::set_permissions(&helper_root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let helper_path = rigg_core::rgb_auth::Paths::new(&helper_root).socket();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(helper_path.parent().unwrap())
        .unwrap();
    let helper = UnixListener::bind(&helper_path).unwrap();
    std::fs::set_permissions(&helper_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (arrived, wait_arrival) = mpsc::sync_channel(1);
    let (release, wait_release) = mpsc::sync_channel(1);
    let fake = std::thread::spawn(move || {
        let (stream, _) = helper.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["op"], "rgb_status");
        arrived.send(()).unwrap();
        let _ = wait_release.recv_timeout(Duration::from_secs(5));
    });
    let socket = root.join("daemon.sock");
    let empty_root = root.join("empty-root");
    std::fs::create_dir(&empty_root).unwrap();
    fixture.child = Some(
        Command::new(env!("CARGO_BIN_EXE_agentic-riggd"))
            .args(["--backend", "sim", "--grace-ms", "60000", "--socket"])
            .arg(&socket)
            .arg("--root")
            .arg(&empty_root)
            .arg("--hjelper-rot")
            .arg(&helper_root)
            .env("RIGG_CONFIG_DIR", root.join("config"))
            .env("RIGG_STATE_DIR", root.join("state"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", root.join("no-bus").display()),
            )
            .env_remove("RIGG_PROFIL")
            .env_remove("RIGG_HJELPER_ROT")
            .env_remove("NOTIFY_SOCKET")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "testdaemonen startet ikke");
        assert!(
            fixture
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut stream = UnixStream::connect(&socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    send(&mut stream, 1, "get_rgb_helper", json!({"refresh":true}));
    assert_eq!(receive(&mut reader)["error"]["code"], "hello_required");
    send(
        &mut stream,
        2,
        "hello",
        json!({"protocol":[1],"client":"rgb-private-test"}),
    );
    assert_eq!(receive(&mut reader)["ok"], true);
    send(&mut stream, 3, "get_rgb_helper", json!({"refresh":true}));
    wait_arrival.recv_timeout(Duration::from_secs(2)).unwrap();
    // Root-fixturen holder fortsatt RGB-svaret tilbake. Ping på samme UI-
    // socket skal likevel svare, med riktig id og uten å forlenge RGB-lease.
    send(&mut stream, 4, "ping", json!({}));
    let ping = receive(&mut reader);
    assert_eq!(ping["id"], 4);
    assert_eq!(ping["ok"], true);
    drop(reader);
    drop(stream);
    release.send(()).unwrap();
    fake.join().unwrap();
}
