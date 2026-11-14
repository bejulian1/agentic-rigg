//! CPU-last fra `/proc/stat` og klokke per tråd fra `cpuinfo_avg_freq`
//! (APERF/MPERF-snitt, kjerne 6.13+), ellers `scaling_cur_freq`.

use crate::PreadFile;
use rigg_core::model::CpuSample;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Times = (u64, u64);

#[derive(Default)]
struct Counters {
    total: Option<Times>,
    cpus: BTreeMap<u32, Times>,
}

pub struct Cpu {
    stat: Option<PreadFile>,
    root: PathBuf,
    prev: Counters,
    threads: usize,
    freq: BTreeMap<u32, Option<PreadFile>>,
    governor: Option<PreadFile>,
    governor_id: Option<u32>,
    boost: Option<PreadFile>,
    gov_cache: Option<String>,
    boost_cache: Option<bool>,
}

/// (idle, total) per rad: «cpu» først, så «cpu0» … «cpuN».
pub fn parse_stat(text: &str) -> Vec<(u64, u64)> {
    let counters = parse_counters(text);
    counters
        .total
        .into_iter()
        .chain(counters.cpus.into_values())
        .collect()
}

/// Bevar Linux-ID-en: cpu8 må aldri få deltaen eller klokka til cpu4 når
/// noen CPU-er er frakoblet. Aggregertraden «cpu» er en egen teller.
fn parse_counters(text: &str) -> Counters {
    let mut out = Counters::default();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let Some(name) = words.next() else { continue };
        let id = if name == "cpu" {
            None
        } else {
            let Some(id) = name.strip_prefix("cpu").and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            Some(id)
        };
        let Some(v) = words
            .take(8)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
        else {
            continue;
        };
        if v.len() < 5 {
            continue;
        }
        let idle = v[3] + v[4];
        let times = (idle, v.iter().sum());
        if let Some(id) = id {
            out.cpus.insert(id, times);
        } else {
            out.total = Some(times);
        }
    }
    out
}

pub(crate) fn stat_cpu_ids(text: &str) -> Vec<u32> {
    parse_counters(text).cpus.into_keys().collect()
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
        let initial = stat
            .as_mut()
            .and_then(|f| f.read(1 << 20).ok().map(parse_counters))
            .unwrap_or_default();
        let cpu_dir = root.join("sys/devices/system/cpu");
        let freq = initial
            .cpus
            .keys()
            .map(|i| {
                let d = cpu_dir.join(format!("cpu{i}/cpufreq"));
                (
                    *i,
                    PreadFile::open(&d.join("cpuinfo_avg_freq"))
                        .or_else(|_| PreadFile::open(&d.join("scaling_cur_freq")))
                        .ok(),
                )
            })
            .collect();
        let governor_id = initial.cpus.keys().next().copied();
        Cpu {
            stat,
            root: root.to_path_buf(),
            prev: Counters::default(),
            threads: initial.cpus.len(),
            freq,
            governor: governor_id.and_then(|id| {
                PreadFile::open(&cpu_dir.join(format!("cpu{id}/cpufreq/scaling_governor"))).ok()
            }),
            governor_id,
            boost: PreadFile::open(&cpu_dir.join("cpufreq/boost")).ok(),
            gov_cache: None,
            boost_cache: None,
        }
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn sample(&mut self, every5: bool) -> CpuSample {
        let cur = self
            .stat
            .as_mut()
            .and_then(|f| f.read(1 << 20).ok().map(parse_counters))
            .unwrap_or_default();
        self.threads = cur.cpus.len();
        self.freq.retain(|id, _| cur.cpus.contains_key(id));
        let cpu_dir = self.root.join("sys/devices/system/cpu");
        for id in cur.cpus.keys() {
            self.freq.entry(*id).or_insert_with(|| {
                let d = cpu_dir.join(format!("cpu{id}/cpufreq"));
                PreadFile::open(&d.join("cpuinfo_avg_freq"))
                    .or_else(|_| PreadFile::open(&d.join("scaling_cur_freq")))
                    .ok()
            });
        }
        let governor_id = cur.cpus.keys().next().copied();
        let governor_changed = governor_id != self.governor_id;
        if governor_changed {
            self.governor = governor_id.and_then(|id| {
                PreadFile::open(&cpu_dir.join(format!("cpu{id}/cpufreq/scaling_governor"))).ok()
            });
            self.governor_id = governor_id;
        }
        let total = self.prev.total.zip(cur.total).and_then(|(a, b)| load(a, b));
        // Indeksene beholder betydningen sin i telemetri- og historikknøklene.
        // Manglende CPU-er får None, ikke målingen til en annen CPU.
        let slots = cur.cpus.keys().next_back().map_or(0, |id| *id as usize + 1);
        let mut cores = vec![None; slots];
        let mut core_mhz = vec![None; slots];
        for (id, times) in &cur.cpus {
            cores[*id as usize] = self
                .prev
                .cpus
                .get(id)
                .and_then(|before| load(*before, *times));
            core_mhz[*id as usize] = self
                .freq
                .get_mut(id)
                .and_then(Option::as_mut)
                .and_then(|f| f.read_f64())
                .map(|khz| (khz / 1000.0).round());
        }
        self.prev = cur;
        if every5 || governor_changed {
            self.gov_cache = self
                .governor
                .as_mut()
                .and_then(|f| f.read(64).ok().map(|s| s.trim().to_string()));
            self.boost_cache = self
                .boost
                .as_mut()
                .and_then(|f| f.read(8).ok().map(|s| s.trim() == "1"));
        }
        CpuSample {
            load: total,
            core_load: cores,
            core_mhz,
            temp: None,
            tctl: None,
            governor: self.gov_cache.clone(),
            boost: self.boost_cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("rigg-cpu-{name}-{}-{stamp}", std::process::id()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
        fn put(&self, rel: &str, text: &str) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        fn stat(&self, rows: &[(u32, u64, u64)]) {
            let (user, idle) = rows
                .iter()
                .fold((0, 0), |(u, i), row| (u + row.1, i + row.2));
            let mut text = format!("cpu {user} 0 0 {idle} 0 0 0 0\n");
            for (id, user, idle) in rows {
                text.push_str(&format!("cpu{id} {user} 0 0 {idle} 0 0 0 0\n"));
            }
            self.put("proc/stat", &text);
        }
        fn frequency(&self, id: u32, mhz: u32) {
            self.put(
                &format!("sys/devices/system/cpu/cpu{id}/cpufreq/scaling_cur_freq"),
                &format!("{}\n", mhz * 1000),
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn last_fra_to_avlesninger() {
        let a =
            parse_stat("cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0\nintr 1 2 3\n");
        let b = parse_stat("cpu  200 0 200 1400 0 0 0 0 0 0\ncpu0 50 0 50 500 0 0 0 0\n");
        assert_eq!(a.len(), 2);
        assert_eq!(load(a[0], b[0]), Some(25.0));
        assert_eq!(load(a[1], b[1]), Some(0.0));
    }

    #[test]
    fn sparse_cpu_id_beholder_last_og_klokke_paa_riktig_traad() {
        let f = Fixture::new("sparse");
        let ids = [0, 1, 2, 3, 8, 9, 10, 11];
        f.stat(&ids.map(|id| (id, 10, 90)));
        for id in 0..12 {
            f.frequency(
                id,
                if ids.contains(&id) {
                    2000 + id * 100
                } else {
                    6000
                },
            );
        }
        let mut cpu = Cpu::new(&f.0);
        let first = cpu.sample(true);
        assert_eq!(cpu.threads(), 8);
        assert_eq!(first.core_mhz.len(), 12);
        assert_eq!(first.core_mhz[8], Some(2800.0));
        assert!(first.core_load.iter().all(Option::is_none));
        assert!(
            first.core_mhz[4..8].iter().all(Option::is_none),
            "offline CPU4–7 må ikke få klokker"
        );
        f.stat(&ids.map(|id| (id, if id == 8 { 40 } else { 20 }, 100)));
        let second = cpu.sample(false);
        assert_eq!(second.core_load[8], Some(75.0));
        assert_eq!(second.core_load[3], Some(50.0));
        assert!(second.core_load[4..8].iter().all(Option::is_none));
        assert_eq!(second.core_mhz[11], Some(3100.0));
    }

    #[test]
    fn ny_cpu_arver_ikke_forrige_cpu_sin_delta_naar_antallet_er_likt() {
        let f = Fixture::new("hotplug");
        f.stat(&[(0, 10, 90), (8, 10, 90)]);
        f.frequency(0, 2000);
        f.frequency(8, 2800);
        f.frequency(9, 3900);
        f.put(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
            "powersave\n",
        );
        f.put(
            "sys/devices/system/cpu/cpu9/cpufreq/scaling_governor",
            "performance\n",
        );
        let mut cpu = Cpu::new(&f.0);
        assert_eq!(cpu.sample(true).governor.as_deref(), Some("powersave"));
        f.stat(&[(9, 500, 500), (0, 20, 100)]); // Ny ID og annen rekkefølge, samme antall.
        let second = cpu.sample(false);
        assert_eq!(second.core_load[0], Some(50.0));
        assert_eq!(
            second.core_load[9], None,
            "CPU9 har ennå ingen tidligere teller"
        );
        assert_eq!(second.core_mhz[9], Some(3900.0));
        assert_eq!(second.core_mhz[8], None);
        f.stat(&[(9, 510, 530)]);
        let third = cpu.sample(false);
        assert_eq!(third.core_load[9], Some(25.0));
        assert_eq!(third.core_mhz[0], None);
        assert_eq!(
            third.governor.as_deref(),
            Some("performance"),
            "representativ aktiv CPU, ikke hardkodet CPU0"
        );
        assert_eq!(cpu.threads(), 1);
    }
}
