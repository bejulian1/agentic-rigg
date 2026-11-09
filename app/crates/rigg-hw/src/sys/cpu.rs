//! CPU-last fra `/proc/stat` og klokke per tråd fra `cpuinfo_avg_freq`
//! (APERF/MPERF-snitt, kjerne 6.13+), ellers `scaling_cur_freq`.

use crate::PreadFile;
use rigg_core::model::CpuSample;
use std::path::Path;

pub struct Cpu {
    stat: Option<PreadFile>,
    prev: Vec<(u64, u64)>,
    threads: usize,
    freq: Vec<Option<PreadFile>>,
    governor: Option<PreadFile>,
    boost: Option<PreadFile>,
    gov_cache: Option<String>,
    boost_cache: Option<bool>,
}

/// (idle, total) per rad: «cpu» først, så «cpu0» … «cpuN».
pub fn parse_stat(text: &str) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.starts_with("cpu") {
            break;
        }
        let v: Vec<u64> = line.split_whitespace().skip(1).take(8).filter_map(|x| x.parse().ok()).collect();
        if v.len() < 5 {
            continue;
        }
        let idle = v[3] + v[4];
        out.push((idle, v.iter().sum()));
    }
    out
}

fn load(a: (u64, u64), b: (u64, u64)) -> Option<f64> {
    let dt = b.1.checked_sub(a.1)?;
    let di = b.0.checked_sub(a.0)?;
    if dt == 0 {
        return Some(0.0);
    }
    let v = (1.0 - di as f64 / dt as f64) * 100.0;
    Some((v.clamp(0.0, 100.0) * 10.0).round() / 10.0)
}

impl Cpu {
    pub fn new(root: &Path) -> Cpu {
        let mut stat = PreadFile::open(&root.join("proc/stat")).ok();
        let threads = stat.as_mut().and_then(|f| f.read(1 << 16).ok().map(|t| parse_stat(t).len().saturating_sub(1))).unwrap_or(0);
        let cpu_dir = root.join("sys/devices/system/cpu");
        let freq = (0..threads)
            .map(|i| {
                let d = cpu_dir.join(format!("cpu{i}/cpufreq"));
                PreadFile::open(&d.join("cpuinfo_avg_freq")).or_else(|_| PreadFile::open(&d.join("scaling_cur_freq"))).ok()
            })
            .collect();
        Cpu {
            stat,
            prev: vec![],
            threads,
            freq,
            governor: PreadFile::open(&cpu_dir.join("cpu0/cpufreq/scaling_governor")).ok(),
            boost: PreadFile::open(&cpu_dir.join("cpufreq/boost")).ok(),
            gov_cache: None,
            boost_cache: None,
        }
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn sample(&mut self, every5: bool) -> CpuSample {
        let cur = self.stat.as_mut().and_then(|f| f.read(1 << 16).ok().map(parse_stat)).unwrap_or_default();
        let (total, cores) = if self.prev.len() == cur.len() && !cur.is_empty() {
            let t = load(self.prev[0], cur[0]);
            let c = (1..cur.len()).map(|i| load(self.prev[i], cur[i])).collect();
            (t, c)
        } else {
            (None, vec![None; cur.len().saturating_sub(1)])
        };
        self.prev = cur;
        let core_mhz = self.freq.iter_mut().map(|f| f.as_mut().and_then(|f| f.read_f64()).map(|khz| (khz / 1000.0).round())).collect();
        if every5 {
            self.gov_cache = self.governor.as_mut().and_then(|f| f.read(64).ok().map(|s| s.trim().to_string()));
            self.boost_cache = self.boost.as_mut().and_then(|f| f.read(8).ok().map(|s| s.trim() == "1"));
        }
        CpuSample { load: total, core_load: cores, core_mhz, temp: None, tctl: None, governor: self.gov_cache.clone(), boost: self.boost_cache }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_fra_to_avlesninger() {
        let a = parse_stat("cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0\nintr 1 2 3\n");
        let b = parse_stat("cpu  200 0 200 1400 0 0 0 0 0 0\ncpu0 50 0 50 500 0 0 0 0\n");
        assert_eq!(a.len(), 2);
        assert_eq!(load(a[0], b[0]), Some(25.0));
        assert_eq!(load(a[1], b[1]), Some(0.0));
    }
}
