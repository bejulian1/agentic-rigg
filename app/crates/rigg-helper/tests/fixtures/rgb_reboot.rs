// The build-33 incident through archive migration, boot discovery, worker RPC
// and revocation. Only private files, simulated systemd and a typed fake card.
use super::*;
use crate::rgb_runtime::Runtime;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::process::Command;

const PCI: &str = "0000:1c:00.0";
const UUID: &str = "GPU-12345678-abcd-ef01-2345-6789abcdef01";

fn put(path: &Path, data: &[u8]) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(path.parent().unwrap())
        .unwrap();
    fs::write(path, data).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn link(from: &Path, to: &Path) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(to.parent().unwrap())
        .unwrap();
    symlink(from, to).unwrap();
}

fn boot_metadata(root: &Path) -> Value {
    let gpu = root.join("sys/devices/pci0000:00").join(PCI);
    let adapter = gpu.join("i2c-37");
    let driver = root.join("sys/bus/pci/drivers/nvidia");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(&driver)
        .unwrap();
    for (key, value) in [
        ("vendor", "0x10de"),
        ("device", "0x1e87"),
        ("subsystem_vendor", "0x10de"),
        ("subsystem_device", "0x1e87"),
    ] {
        put(&gpu.join(key), value.as_bytes());
    }
    put(&adapter.join("name"), b"NVIDIA i2c adapter 1 at 1c:00.0");
    put(&adapter.join("i2c-dev/i2c-37/dev"), b"89:37");
    link(&driver, &gpu.join("driver"));
    link(&gpu, &root.join("sys/bus/pci/devices").join(PCI));
    link(&adapter, &root.join("sys/bus/i2c/devices/i2c-37"));
    link(
        &adapter.join("i2c-dev/i2c-37"),
        &root.join("sys/class/i2c-dev/i2c-37"),
    );
    for (bus, name) in [("i2c-8", "SMBus"), ("i2c-10", "ccgx-ucsi")] {
        put(
            &root.join("sys/bus/i2c/devices").join(bus).join("name"),
            name.as_bytes(),
        );
    }
    put(
        &root
            .join("proc/driver/nvidia/gpus")
            .join(PCI)
            .join("information"),
        format!("GPU UUID: {UUID}\nBus Location: {PCI}\nDevice Minor: 13\n").as_bytes(),
    );
    let mut metadata = palit_rgb::metadata_preview(root, PCI, UUID)
        .unwrap()
        .to_json();
    assert_eq!(
        metadata["devicePath"],
        root.join("dev/i2c-37").to_str().unwrap()
    );
    // The report is passed across the fixture's virtual filesystem boundary.
    metadata["devicePath"] = json!("/dev/i2c-37");
    metadata
}

#[allow(clippy::disallowed_methods)]
fn script(script: &Path, args: &[&str], code: i32) {
    let out = Command::new("/usr/bin/python3")
        .args(["-I", "-S"])
        .arg(script)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.code() == Some(code),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bygg33_kaldstart_med_journal_begge_avklaringsvalg() {
    for keep in [true, false] {
        let fixture = Fixture::new();
        let root = &fixture.path;
        let mut card = binding();
        card.uuid = UUID.into();
        card.signature = if keep {
            Signature::Palit150
        } else {
            Signature::Palit100
        };
        let off = Observed {
            mode: None,
            color: Some([0, 0, 0]),
            brightness_raw: Some(0),
        };
        let saved = Baseline::user_confirmed_static(card.clone(), off.clone()).unwrap();
        let factory = Baseline::user_confirmed_static(card.clone(), observed()).unwrap();
        let old = Authorization::new(1000, card.clone(), 33).unwrap();
        let state = root.join("var/lib/agentic-rigg-rgb");
        let mut journal = fixtures::record(&saved);
        journal.operation = Operation::Recover;
        journal.phase = Phase::Attempted;
        journal.target = Book::new(saved.clone()).unwrap().baseline_request().clone();
        put(
            &state.join("baseline.json"),
            &serde_json::to_vec(&saved).unwrap(),
        );
        put(
            &state.join("journal.json"),
            &serde_json::to_vec(&journal).unwrap(),
        );
        put(
            &state.join("fabrikk-baseline.json"),
            &serde_json::to_vec(&factory).unwrap(),
        );
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        put(
            &root.join("etc/agentic-rigg/helper.json"),
            &serde_json::to_vec(
                &json!({"versjon":2,"enabled":true,"uid":1000,"pci":PCI,"uuid":UUID,"bygg":33}),
            )
            .unwrap(),
        );
        put(
            &root.join("etc/agentic-rigg/rgb.json"),
            &serde_json::to_vec(&old).unwrap(),
        );
        put(
            &root.join("run/systemd/system/agentic-rigg-rgb.service.d/50-agentic-rigg-card.conf"),
            b"[Service]\nDeviceAllow=/dev/i2c-8 rw\n",
        );
        fs::remove_dir_all(root.join("run")).unwrap();
        let metadata = boot_metadata(root);
        put(
            &root.join("fixture-rgb.json"),
            &serde_json::to_vec(&json!({"ok":true,"report":{"metadata":metadata}})).unwrap(),
        );
        let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        script(
            &app.join("scripts/test-system-install.py"),
            &["--migrate-fixture", root.to_str().unwrap()],
            0,
        );
        if keep {
            let manager = root.join("usr/local/libexec/agentic-rigg/nvml-admin.py");
            let boot = ["boot", "--root", root.to_str().unwrap(), "--uid", "1000"];
            let worker_state = root.join("fixture-systemd.json");
            put(
                &worker_state,
                br#"{"ActiveState":"inactive","fail":{"start":"before"}}"#,
            );
            script(&manager, &boot, 75);
            let status_file = root.join("run/agentic-rigg-card/status.json");
            let report =
                || serde_json::from_slice::<Value>(&fs::read(&status_file).unwrap()).unwrap();
            assert_eq!(report()["rgb"], "waiting");
            assert_eq!(report()["nvml"], "ready");
            put(&worker_state, b"{}");
            script(&manager, &boot, 0);
            assert_eq!(report()["rgb"], "ready");
        }
        let rules = root.join("run/systemd/system");
        assert_eq!(
            fs::read_to_string(rules.join("agentic-rigg-helper.service.d/gpu.conf")).unwrap(),
            "[Service]\nDeviceAllow=\nDeviceAllow=/dev/nvidiactl rw\nDeviceAllow=/dev/nvidia13 rw\n"
        );
        assert_eq!(
            fs::read_to_string(rules.join("agentic-rigg-rgb.service.d/50-agentic-rigg-card.conf"))
                .unwrap(),
            "[Service]\nDeviceAllow=\nDeviceAllow=/dev/i2c-37 rw\n"
        );
        let read = |path: &str| fs::read_to_string(root.join(path)).unwrap();
        let grant =
            rigg_core::hjelper::Config::parse(&read("etc/agentic-rigg/helper.json")).unwrap();
        let auth = Authorization::parse(&read("etc/agentic-rigg/rgb.json")).unwrap();
        assert!(auth.matches_card(&grant));
        let gpu = Box::new(rigg_hw::gpu::skriv::SimSkriver::new());
        let mut nvml = crate::motor::Motor::new(Some(gpu), "sim", None);
        nvml.set_config(Ok(grant), 0);
        let change = json!({"gpu":PCI,"uuid":UUID,"settings":{"powerLimitW":200}});
        let trial = nvml.handle(1, "apply", &change, 1).unwrap();
        nvml.handle(
            1,
            "confirm",
            &json!({"gpu":PCI,"uuid":UUID,"txn":trial["txn"]}),
            2,
        )
        .unwrap();
        assert_eq!(nvml.status(3)["state"]["confirmed"]["powerLimitW"], 200.0);
        let (hw, physical) = hardware();
        physical.lock().unwrap().binding = card;
        let native = NativeBackend::from_parts(
            auth.clone(),
            saved.clone(),
            PrivateStore::open_fixture(&state).unwrap(),
            hw,
            false,
        )
        .unwrap();
        assert_eq!(auth.binding().signature, old.binding().signature);
        let mut runtime = Runtime::new(auth.clone(), saved.clone(), native).unwrap();
        let id = json!({"gpu":PCI,"uuid":UUID});

        let mut rpc = |op: &str, mut args: Value| {
            let clock = crate::rgb_service::now_ms().unwrap();
            args["gpu"] = json!(PCI);
            args["uuid"] = json!(UUID);
            runtime.handle(1000, 7, op, &args, clock)
        };
        let status = rpc("rgb_read", id.clone()).unwrap()["status"].clone();
        assert_eq!(status["review"]["kind"], "journal");
        for op in ["rgb_hold", "rgb_persist"] {
            assert!(rpc(op, id.clone()).is_err());
        }
        let apply = json!({"request":status["baselineRequest"],"confirmTimeoutS":15});
        assert!(rpc("rgb_apply", apply.clone()).is_err());
        assert!(
            physical
                .lock()
                .unwrap()
                .actions
                .iter()
                .all(|op| *op == "read")
        );
        physical.lock().unwrap().result = Ok(off);
        if keep {
            let ack = json!({"confirmedStatic":true,"recoveryId":status["review"]["id"],"expected":status["review"]["expected"]});
            rpc("rgb_keep_observed", ack).unwrap();
            assert!(!state.join("journal.json").exists());
            assert!(fs::read_dir(&state).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("journal-uavklart-")
            }));
            let pending = rpc("rgb_apply", apply).unwrap();
            let txn = json!({"txn":pending["outcome"]["txn"]});
            rpc("rgb_confirm", txn.clone()).unwrap();
            rpc("rgb_confirm", txn).unwrap(); // lost response
            rpc("rgb_persist", id.clone()).unwrap();
            rpc("rgb_persist", id.clone()).unwrap();
        } else {
            let stale = json!({"recoveryId":"obsolete","expected":status["baselineRequest"]});
            assert!(rpc("rgb_reset", stale).is_err());
            assert!(
                physical
                    .lock()
                    .unwrap()
                    .actions
                    .iter()
                    .all(|a| *a == "read")
            );
            let restored = rpc(
                "rgb_reset",
                json!({"recoveryId":status["review"]["id"],"expected":status["baselineRequest"]}),
            )
            .unwrap();
            let ack = json!({"confirmedStatic":true,"recoveryId":restored["status"]["recoveryId"],"expected":status["baselineRequest"]});
            rpc("rgb_ack_recovery", ack).unwrap();
        }
        let hold_start = crate::rgb_service::now_ms().unwrap();
        assert_eq!(
            rpc("rgb_hold", id.clone()).unwrap()["status"]["keepalive"],
            true
        );
        let socket = root.join("hold.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let mut service = crate::rgb_service::Service::new(listener, socket).unwrap();
        let writes = || {
            physical
                .lock()
                .unwrap()
                .actions
                .iter()
                .filter(|op| **op == "apply")
                .count()
        };
        let before = writes();
        let start = std::time::Instant::now();
        let result = service.run(
            &mut runtime,
            &std::sync::atomic::AtomicBool::new(false),
            &auth,
            || {
                if start.elapsed().as_secs() >= 5 || writes() > before {
                    Err("tilbakekalt".into())
                } else {
                    Ok(auth.clone())
                }
            },
        );
        assert!(result.is_err() && crate::rgb_service::now_ms().unwrap() - hold_start >= 2000);
        assert_eq!(writes(), before + 1);
        assert!(!runtime.keepalive_active());
        assert!(
            !runtime
                .animate(crate::rgb_service::now_ms().unwrap() + 3000)
                .unwrap()
        );
        assert_eq!(writes(), before + 1);
        drop(runtime);
        // Even an unresolved journal cannot keep either permission active.
        put(
            &state.join("journal.json"),
            &serde_json::to_vec(&journal).unwrap(),
        );
        script(
            &root.join("usr/local/libexec/agentic-rigg/nvml-admin.py"),
            &[
                "deactivate",
                "--root",
                root.to_str().unwrap(),
                "--uid",
                "1000",
            ],
            0,
        );
        nvml.set_config(Err("Tilgang trukket tilbake.".into()), 4);
        assert!(nvml.handle(1, "apply", &change, 5).is_err());
        assert!(state.join("journal.json").is_file() && state.join("baseline.json").is_file());
        for path in [
            "etc/agentic-rigg/helper.json",
            "etc/agentic-rigg/rgb.json",
            "etc/systemd/system/agentic-rigg-card.service",
        ] {
            assert!(!root.join(path).exists());
        }
    }
}

#[test]
fn første_grunnopptak_krever_fersk_visuell_bekreftelse() {
    let fixture = Fixture::new();
    let first = Baseline::awaiting_visual(binding(), observed()).unwrap();
    let start = || {
        let store = PrivateStore::open_fixture(&fixture.path).unwrap();
        let (hw, physical) = hardware();
        let backend =
            NativeBackend::from_parts(authorization(), first.clone(), store, hw, false).unwrap();
        (
            Runtime::new(authorization(), first.clone(), backend).unwrap(),
            physical,
        )
    };
    let (first_worker, _) = start();
    let old_id = first_worker.status()["review"]["id"].clone();
    drop(first_worker);
    let (mut worker, physical) = start();
    let review = worker.status()["review"].clone();
    assert_eq!(review["kind"], "first");
    let mut ack = json!({"gpu":binding().pci, "uuid":binding().uuid, "confirmedStatic":true, "expected":review["expected"], "recoveryId":old_id});
    assert!(
        worker
            .handle(1000, 1, "rgb_keep_observed", &ack, 1)
            .is_err()
    );
    ack["recoveryId"] = review["id"].clone();
    physical.lock().unwrap().read_observed.color = Some([45, 50, 60]);
    assert!(
        worker
            .handle(1000, 1, "rgb_keep_observed", &ack, 2)
            .is_err()
    );
    assert!(!fixture.path.join("baseline.json").exists());
    let fresh = worker.status()["review"].clone();
    ack["expected"] = fresh["expected"].clone();
    ack["recoveryId"] = fresh["id"].clone();
    worker
        .handle(1000, 1, "rgb_keep_observed", &ack, 3)
        .unwrap();
    assert!(fixture.path.join("baseline.json").exists());
    assert!(worker.status()["review"].is_null());
    assert!(
        physical
            .lock()
            .unwrap()
            .actions
            .iter()
            .all(|a| *a == "read")
    );
}
