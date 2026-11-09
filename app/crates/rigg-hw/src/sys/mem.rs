//! Minne fra `/proc/meminfo`. Brukt = MemTotal − MemAvailable (som `free`).
//! `Shmem` står for seg, fordi den ligger i `Cached` men ikke kan frigjøres.

use crate::PreadFile;
use rigg_core::model::MemSample;
use std::path::Path;

pub struct Mem {
    file: Option<PreadFile>,
}

pub fn parse(text: &str) -> MemSample {
    let get = |k: &str| -> Option<f64> {
        text.lines().find_map(|l| {
            let (key, rest) = l.split_once(':')?;
            (key == k).then(|| rest.split_whitespace().next()?.parse().ok()).flatten()
        })
    };
    let total = get("MemTotal");
    let avail = get("MemAvailable");
    let swap_t = get("SwapTotal");
    let swap_f = get("SwapFree");
    MemSample {
        total_kb: total,
        available_kb: avail,
        used_kb: total.zip(avail).map(|(t, a)| t - a),
        free_kb: get("MemFree"),
        buffers_kb: get("Buffers"),
        cached_kb: get("Cached"),
        shmem_kb: get("Shmem"),
        sreclaimable_kb: get("SReclaimable"),
        swap_total_kb: swap_t,
        swap_used_kb: swap_t.zip(swap_f).map(|(t, f)| t - f),
    }
}

impl Mem {
    pub fn new(root: &Path) -> Mem {
        Mem { file: PreadFile::open(&root.join("proc/meminfo")).ok() }
    }

    pub fn sample(&mut self) -> MemSample {
        self.file.as_mut().and_then(|f| f.read(1 << 14).ok().map(parse)).unwrap_or_default()
    }
}
