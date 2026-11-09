//! Ekte maskinvare, bare lesing: `cargo test -p rigg-hw -- --ignored`.
//! Tre prøver ett sekund fra hverandre (1 Hz holder kortet i P8), og så
//! `nvmlShutdown`. Sjekker at verdiene er rimelige for et skjermkort i ro.

use rigg_hw::gpu::{GpuBackend, Plan, nvml::Nvml};
use rigg_hw::sys::SysReader;
use std::path::Path;

fn now() -> i64 {
    rigg_core::now_ms()
}

#[test]
#[ignore = "leser det ekte skjermkortet"]
fn nvml_på_denne_maskinen() {
    let t0 = std::time::Instant::now();
    let mut b = Nvml::new("/".into(), now());
    eprintln!("init: {:?}, tilstand {:?} {:?}", t0.elapsed(), b.driver_state, b.driver_detail);
    let gpus = b.gpus();
    assert!(!gpus.is_empty());
    eprintln!("kort: {gpus:?}");
    for i in 0..3 {
        let t = std::time::Instant::now();
        let r = b.sample(&Plan { now_ms: now(), every2: i % 2 == 0, every5: i == 0, every60: false, first: i == 0, watched: true });
        eprintln!("runde {i}: {:?}", t.elapsed());
        for (id, s) in &r.samples {
            eprintln!("  {id}: {}", serde_json::to_string(s).unwrap());
            assert!(s.temp.is_some_and(|t| (10.0..105.0).contains(&t)));
            assert!(s.vram_total.is_some_and(|v| v > 1000.0));
            assert!(!s.fans.is_empty());
            if i > 0 {
                assert!(s.power.is_some_and(|p| (1.0..400.0).contains(&p)), "effekt fra energitelleren");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
    let id = gpus[0].id.clone();
    let (st, done) = b.static_info(&id).unwrap();
    eprintln!("statisk ({done}): {}", serde_json::to_string(&st).unwrap());
    b.shutdown();
}

#[test]
#[ignore = "leser den ekte maskinen"]
fn sys_på_denne_maskinen() {
    let mut r = SysReader::new(Path::new("/"));
    r.sample(now());
    std::thread::sleep(std::time::Duration::from_millis(500));
    let t = std::time::Instant::now();
    let s = r.sample(now());
    eprintln!("sys-prøve: {:?}", t.elapsed());
    eprintln!("cpu: {:?}", s.cpu);
    assert!(s.cpu.load.is_some());
    assert!(s.cpu.temp.is_some());
}
