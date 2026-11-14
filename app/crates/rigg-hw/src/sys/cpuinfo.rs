//! Faste prosessordata til System-siden (CPU-Z): navn, familie, modell,
//! stepping, mikrokode, kjerner og tråder, CCX, cache, flagg og cpufreq-område.
//!
//! Port av `readCpu` i `server/system.js`, med tilleggene fra
//! maskinvare-revisjon kap. 7: kodenavn som oppslag (merket som oppslag),
//! CCX-inndeling fra L3-cachen, mikrokode fra sysfs og sårbarhetene.
//! Alt leses én gang (`get_system`), uten root. Multiplikator, bussklokke,
//! spenning og pakkeeffekt krever MSR eller RAPL (root) og leses ikke.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Flaggene CPU-Z og demoen viser først (flagg, navn).
pub const IMPORTANT_FLAGS: &[(&str, &str)] = &[
    ("sse", "SSE"),
    ("sse2", "SSE2"),
    ("pni", "SSE3"),
    ("ssse3", "SSSE3"),
    ("sse4_1", "SSE4.1"),
    ("sse4_2", "SSE4.2"),
    ("sse4a", "SSE4A"),
    ("avx", "AVX"),
    ("avx2", "AVX2"),
    ("avx512f", "AVX-512F"),
    ("fma", "FMA3"),
    ("f16c", "F16C"),
    ("aes", "AES-NI"),
    ("sha_ni", "SHA"),
    ("pclmulqdq", "CLMUL"),
    ("bmi1", "BMI1"),
    ("bmi2", "BMI2"),
    ("popcnt", "POPCNT"),
    ("abm", "ABM"),
    ("adx", "ADX"),
    ("rdrand", "RDRAND"),
    ("rdseed", "RDSEED"),
    ("lm", "x86-64"),
    ("svm", "AMD-V"),
    ("vmx", "VT-x"),
    ("nx", "NX"),
];

/// Kodenavn, prosess og sokkel slått opp fra produsent, familie og modell.
/// Kjernen oppgir ikke dette; det er et oppslag og merkes som det.
struct Codename {
    vendor: &'static str,
    family: u32,
    models: (u32, u32),
    name: &'static str,
    process: &'static str,
    socket: &'static str,
}

const CODENAMES: &[Codename] = &[
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x01, 0x01),
        name: "Summit Ridge (Zen)",
        process: "14 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x08, 0x08),
        name: "Pinnacle Ridge (Zen+)",
        process: "12 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x11, 0x11),
        name: "Raven Ridge (Zen)",
        process: "14 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x18, 0x18),
        name: "Picasso (Zen+)",
        process: "12 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x60, 0x60),
        name: "Renoir (Zen 2)",
        process: "7 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x17,
        models: (0x71, 0x71),
        name: "Matisse (Zen 2)",
        process: "7 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x19,
        models: (0x21, 0x21),
        name: "Vermeer (Zen 3)",
        process: "7 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x19,
        models: (0x50, 0x50),
        name: "Cezanne (Zen 3)",
        process: "7 nm",
        socket: "AM4",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x19,
        models: (0x61, 0x61),
        name: "Raphael (Zen 4)",
        process: "5 nm",
        socket: "AM5",
    },
    Codename {
        vendor: "AuthenticAMD",
        family: 0x1a,
        models: (0x44, 0x44),
        name: "Granite Ridge (Zen 5)",
        process: "4 nm",
        socket: "AM5",
    },
];

fn txt(p: &Path) -> Option<String> {
    crate::read_text(p).filter(|s| !s.is_empty())
}

fn int(p: &Path) -> Option<u64> {
    txt(p)?.parse().ok()
}

/// «0-3,8-11» → [0, 1, 2, 3, 8, 9, 10, 11].
pub fn parse_cpu_list(s: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                    out.extend(a..=b);
                }
            }
            None => {
                if let Ok(a) = part.trim().parse() {
                    out.push(a);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// «32K» → 32768.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: u64 = s[..end].parse().ok()?;
    let mul = match s[end..].trim().to_ascii_uppercase().as_str() {
        "" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    Some(n * mul)
}

/// Feltene i én blokk av `/proc/cpuinfo`.
type Block = Vec<(String, String)>;

/// Første blokk og antall blokker i `/proc/cpuinfo`, og (pakke, kjerne) per tråd.
fn cpuinfo_blocks(text: &str) -> (Block, usize, BTreeSet<(String, String)>) {
    let mut first = Vec::new();
    let mut n = 0;
    let mut cores = BTreeSet::new();
    for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let kv: Block = block
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
        if !kv.iter().any(|(k, _)| k == "processor") {
            continue;
        }
        cores.insert((
            get("physical id").unwrap_or_else(|| "0".into()),
            get("core id")
                .or_else(|| get("processor"))
                .unwrap_or_default(),
        ));
        if n == 0 {
            first = kv;
        }
        n += 1;
    }
    (first, n, cores)
}

/// De aktive Linux-ID-ene, aldri en oppdiktet sammenhengende 0..antall-liste.
fn thread_ids(cpu_dir: &Path, text: &str) -> Vec<u32> {
    let online = txt(&cpu_dir.join("online"))
        .map(|s| parse_cpu_list(&s))
        .unwrap_or_default();
    if !online.is_empty() {
        return online;
    }
    let mut ids: Vec<u32> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "processor")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn cpu_list(ids: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut start = 0;
    while start < ids.len() {
        let mut end = start;
        while end + 1 < ids.len() && ids[end].checked_add(1) == Some(ids[end + 1]) {
            end += 1;
        }
        parts.push(if start == end {
            ids[start].to_string()
        } else {
            format!("{}-{}", ids[start], ids[end])
        });
        start = end + 1;
    }
    parts.join(",")
}

/// Tell hver fysiske cache én gang og grupper bare like instanser. En hybrid-
/// CPU kan ha flere størrelser/assosiativiteter på samme nivå og av samme type.
fn cache_info(cpu_dir: &Path, ids: &[u32]) -> (Vec<Value>, Vec<String>) {
    let mut seen = BTreeSet::new();
    let mut groups = BTreeMap::new();
    let mut l3_groups = BTreeSet::new();
    for cpu in ids {
        let dir = cpu_dir.join(format!("cpu{cpu}/cache"));
        let mut indices: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| {
                        n.strip_prefix("index")
                            .is_some_and(|n| n.parse::<u32>().is_ok())
                    })
                    .collect()
            })
            .unwrap_or_default();
        indices.sort();
        for index in indices {
            let d = dir.join(&index);
            let (Some(level), Some(kind)) = (int(&d.join("level")), txt(&d.join("type"))) else {
                continue;
            };
            let id = int(&d.join("id"));
            let shared = txt(&d.join("shared_cpu_list"))
                .map(|s| parse_cpu_list(&s))
                .filter(|s| !s.is_empty());
            let scope = match &shared {
                Some(ids) => format!("cpus:{}", cpu_list(ids)),
                None if id.is_some() => format!(
                    "package:{:?}",
                    int(&cpu_dir.join(format!("cpu{cpu}/topology/physical_package_id")))
                ),
                // Manglende identitet gir ingen antakelse om andre CPU-ers cache.
                None => format!("cpu:{cpu}/{index}"),
            };
            if !seen.insert((level, kind.clone(), id, scope)) {
                continue;
            }
            if level == 3
                && let Some(ids) = &shared
            {
                l3_groups.insert(cpu_list(ids));
            }
            let size = txt(&d.join("size")).and_then(|s| parse_size(&s));
            let key = (
                level,
                kind,
                size,
                int(&d.join("ways_of_associativity")),
                int(&d.join("coherency_line_size")),
                shared.as_ref().map(Vec::len),
            );
            let group = groups.entry(key).or_insert((0u64, true));
            group.0 += 1;
            group.1 &= id.is_some() || shared.is_some();
        }
    }
    let caches = groups
        .into_iter()
        .map(
            |((level, kind, size, ways, line_size, shared_by), (count, identified))| {
                let instances = identified.then_some(count);
                json!({
                    "level": level, "type": kind, "size": size, "instances": instances,
                    "total": size.zip(instances).and_then(|(s, n)| s.checked_mul(n)), "ways": ways,
                    "lineSize": line_size, "sharedBy": shared_by,
                })
            },
        )
        .collect();
    (caches, l3_groups.into_iter().collect())
}

/// Prosessoren som JSON (samme felt som demoens `/api/system` › `cpu`).
pub fn read(root: &Path) -> Value {
    let text = std::fs::read_to_string(root.join("proc/cpuinfo")).unwrap_or_default();
    let (first, _, core_ids) = cpuinfo_blocks(&text);
    let get = |k: &str| first.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    let cpu_dir = root.join("sys/devices/system/cpu");
    let mut ids = thread_ids(&cpu_dir, &text);
    if ids.is_empty() {
        ids = crate::read_text(&root.join("proc/stat"))
            .map(|s| super::cpu::stat_cpu_ids(&s))
            .unwrap_or_default();
    }
    let threads = ids.len();
    // Kjerner: topologien i sysfs (core_id per pakke), ellers cpuinfo.
    let mut topo = BTreeSet::new();
    // Kjernen hver tråd hører til (Prosessor-siden grupperer trådene, runde 6A).
    let mut core_of = vec![Value::Null; ids.last().map_or(0, |id| *id as usize + 1)];
    let mut package_of = vec![Value::Null; core_of.len()];
    for i in &ids {
        let t = cpu_dir.join(format!("cpu{i}/topology"));
        let c = txt(&t.join("core_id"));
        let p = txt(&t.join("physical_package_id"));
        core_of[*i as usize] = json!(c.as_deref().and_then(|c| c.parse::<u32>().ok()));
        package_of[*i as usize] = json!(p.as_deref().and_then(|p| p.parse::<u32>().ok()));
        if let (Some(p), Some(c)) = (p, c) {
            topo.insert((p, c));
        }
    }
    let cores = if !topo.is_empty() {
        topo.len()
    } else {
        core_ids.len()
    };
    let packages: BTreeSet<String> = topo.iter().map(|(p, _)| p.clone()).collect();

    let flags: Vec<String> = get("flags")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let fset: BTreeSet<&str> = flags.iter().map(String::as_str).collect();
    let important: Vec<Value> = IMPORTANT_FLAGS
        .iter()
        .filter(|(f, _)| fset.contains(f))
        .map(|(f, l)| json!({"flag": f, "label": l}))
        .collect();

    let (caches, l3_groups) = cache_info(&cpu_dir, &ids);
    let paths: Vec<_> = ids
        .iter()
        .map(|id| cpu_dir.join(format!("cpu{id}/cpufreq")))
        .collect();
    let drivers: BTreeSet<_> = paths
        .iter()
        .filter_map(|p| txt(&p.join("scaling_driver")))
        .collect();
    let minimum = paths
        .iter()
        .filter_map(|p| int(&p.join("cpuinfo_min_freq")))
        .min();
    let maximum = paths
        .iter()
        .filter_map(|p| int(&p.join("cpuinfo_max_freq")))
        .max();
    let mut governors = BTreeSet::new();
    let mut pstates = BTreeSet::new();
    for path in &paths {
        if let Some(s) = txt(&path.join("scaling_available_governors")) {
            governors.extend(s.split_whitespace().map(str::to_string));
        }
        if let Some(s) = txt(&path.join("scaling_available_frequencies")) {
            pstates.extend(s.split_whitespace().filter_map(|s| s.parse::<u64>().ok()));
        }
    }
    let freq = json!({
        "minKHz": minimum, "maxKHz": maximum,
        "driver": (!drivers.is_empty()).then(|| drivers.iter().cloned().collect::<Vec<_>>().join(" · ")),
        "governors": governors.into_iter().collect::<Vec<_>>(),
        "available": !drivers.is_empty() || minimum.is_some() || maximum.is_some(),
        "pstatesKHz": pstates.into_iter().collect::<Vec<_>>(),
    });

    let vendor = get("vendor_id");
    let num = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
    let (family, model, stepping) = (num("cpu family"), num("model"), num("stepping"));
    let codename = match (vendor.as_deref(), family, model) {
        (Some(v), Some(f), Some(m)) => CODENAMES
            .iter()
            .find(|c| c.vendor == v && c.family == f && (c.models.0..=c.models.1).contains(&m)),
        _ => None,
    };
    let revision = match (codename.map(|c| c.name), stepping) {
        (Some(n), Some(2)) if n.starts_with("Pinnacle Ridge") => Some("PiR-B2"),
        _ => None,
    };
    let microcode = ids
        .first()
        .and_then(|id| txt(&cpu_dir.join(format!("cpu{id}/microcode/version"))))
        .or_else(|| get("microcode"));

    // Sårbarhetene (lesbare for alle): navn → tekst.
    let mut vulns = serde_json::Map::new();
    if let Ok(rd) = std::fs::read_dir(cpu_dir.join("vulnerabilities")) {
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for n in names {
            if let Some(t) = txt(&cpu_dir.join("vulnerabilities").join(&n)) {
                vulns.insert(n, Value::String(t));
            }
        }
    }

    json!({
        "name": get("model name").map(|n| n.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_else(|| "Ukjent prosessor".into()),
        "vendor": vendor,
        "vendorName": match vendor.as_deref() { Some("AuthenticAMD") => Some("AMD"), Some("GenuineIntel") => Some("Intel"), _ => None }.map(str::to_string).or(vendor.clone()),
        "family": family,
        "model": model,
        "stepping": stepping,
        "microcode": microcode,
        "cores": (cores > 0).then_some(cores),
        "threads": threads,
        "threadIds": ids,
        "packages": packages.len().max(1),
        "caches": caches,
        "l3Groups": l3_groups,
        "coreOf": core_of,
        "packageOf": package_of,
        "flags": flags,
        "important": important,
        "freq": freq,
        "addressSizes": get("address sizes"),
        "bogomips": get("bogomips").and_then(|v| v.parse::<f64>().ok()),
        "codename": codename.map(|c| json!({"name": c.name, "process": c.process, "socket": c.socket, "revision": revision})),
        "vulnerabilities": vulns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "rigg-cpuinfo-{name}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn cache(
        root: &Path,
        cpu: u32,
        index: u32,
        id: u32,
        description: (u32, &str, &str),
        shared: &str,
    ) {
        let dir = format!("sys/devices/system/cpu/cpu{cpu}/cache/index{index}");
        for (name, value) in [
            ("level", description.0.to_string()),
            ("type", description.1.into()),
            ("size", description.2.into()),
            ("id", id.to_string()),
            ("shared_cpu_list", shared.into()),
            ("ways_of_associativity", "8".into()),
            ("coherency_line_size", "64".into()),
        ] {
            put(root, &format!("{dir}/{name}"), &value);
        }
    }

    #[test]
    fn lister_og_størrelser() {
        assert_eq!(parse_cpu_list("0-3,8-11"), vec![0, 1, 2, 3, 8, 9, 10, 11]);
        assert_eq!(parse_cpu_list("5"), vec![5]);
        assert_eq!(parse_size("32K"), Some(32 * 1024));
        assert_eq!(parse_size("8192K"), Some(8 * 1024 * 1024));
        assert_eq!(parse_size("x"), None);
    }

    #[test]
    fn ryzen_fra_en_falsk_rot() {
        let fixture = Fixture::new("ryzen");
        let root = fixture.0.clone();
        let block = |n: u32, core: u32| {
            format!(
                "processor\t: {n}\nvendor_id\t: AuthenticAMD\ncpu family\t: 23\nmodel\t\t: 8\nmodel name\t: AMD Ryzen 7 2700X Eight-Core Processor\nstepping\t: 2\nmicrocode\t: 0x800820d\nphysical id\t: 0\ncore id\t\t: {core}\nflags\t\t: fpu sse sse2 avx avx2 lm svm\naddress sizes\t: 43 bits physical, 48 bits virtual\n\n"
            )
        };
        let text: String = (0..4).map(|i| block(i, i % 2)).collect();
        put(&root, "proc/cpuinfo", &text);
        put(&root, "sys/devices/system/cpu/online", "0-3\n");
        for i in 0..4 {
            put(
                &root,
                &format!("sys/devices/system/cpu/cpu{i}/topology/core_id"),
                &format!("{}\n", i % 2),
            );
            put(
                &root,
                &format!("sys/devices/system/cpu/cpu{i}/topology/physical_package_id"),
                "0\n",
            );
            let l3 = if i % 2 == 0 { "0,2" } else { "1,3" };
            put(
                &root,
                &format!("sys/devices/system/cpu/cpu{i}/cache/index3/shared_cpu_list"),
                l3,
            );
            cache(&root, i, 0, i % 2, (1, "Data", "32K"), l3);
            cache(&root, i, 3, i % 2, (3, "Unified", "8192K"), l3);
        }
        let c = "sys/devices/system/cpu/cpu0/cache/";
        put(&root, &format!("{c}index0/level"), "1");
        put(&root, &format!("{c}index0/type"), "Data");
        put(&root, &format!("{c}index0/size"), "32K");
        put(&root, &format!("{c}index0/shared_cpu_list"), "0,2");
        put(&root, &format!("{c}index0/ways_of_associativity"), "8");
        put(&root, &format!("{c}index3/level"), "3");
        put(&root, &format!("{c}index3/type"), "Unified");
        put(&root, &format!("{c}index3/size"), "8192K");
        put(
            &root,
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_driver",
            "acpi-cpufreq",
        );
        put(
            &root,
            "sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq",
            "3700000",
        );
        let v = read(&root);
        assert_eq!(v["name"], "AMD Ryzen 7 2700X Eight-Core Processor");
        assert_eq!(v["vendorName"], "AMD");
        assert_eq!(v["threads"], 4);
        assert_eq!(v["cores"], 2);
        assert_eq!(v["microcode"], "0x800820d");
        assert_eq!(v["codename"]["name"], "Pinnacle Ridge (Zen+)");
        assert_eq!(v["codename"]["revision"], "PiR-B2");
        assert_eq!(v["caches"][0]["instances"], 2);
        assert_eq!(v["caches"][0]["total"], 65536);
        assert_eq!(v["l3Groups"].as_array().unwrap().len(), 2);
        assert_eq!(v["freq"]["maxKHz"], 3_700_000);
        let imp: Vec<&str> = v["important"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["label"].as_str().unwrap())
            .collect();
        assert_eq!(imp, vec!["SSE", "SSE2", "AVX", "AVX2", "x86-64", "AMD-V"]);
    }

    #[test]
    fn sparse_hybrid_teller_faktiske_cacheinstanser_og_beholder_cpu_id() {
        let f = Fixture::new("hybrid");
        let ids = [0, 1, 2, 3, 8, 9, 10, 11];
        put(&f.0, "sys/devices/system/cpu/online", "0-3,8-11\n");
        put(
            &f.0,
            "proc/cpuinfo",
            "processor: 0\nmodel name: Syntetisk hybridprosessor\nvendor_id: GenuineIntel\n\n",
        );
        for id in ids {
            let core = if id < 4 { id % 2 } else { id - 6 };
            put(
                &f.0,
                &format!("sys/devices/system/cpu/cpu{id}/topology/core_id"),
                &core.to_string(),
            );
            put(
                &f.0,
                &format!("sys/devices/system/cpu/cpu{id}/topology/physical_package_id"),
                "0",
            );
            let shared = if id < 4 {
                if id % 2 == 0 { "0,2" } else { "1,3" }.to_string()
            } else {
                id.to_string()
            };
            cache(
                &f.0,
                id,
                0,
                core,
                (1, "Data", if id < 4 { "64K" } else { "48K" }),
                &shared,
            );
            cache(
                &f.0,
                id,
                1,
                if id < 4 { id % 2 } else { 2 },
                (2, "Unified", if id < 4 { "2048K" } else { "4096K" }),
                if id < 4 { &shared } else { "8-11" },
            );
            // Samme fysiske L3 kan ha ulikt indexN-navn på ulike kjernetyper.
            cache(
                &f.0,
                id,
                if id < 4 { 3 } else { 2 },
                0,
                (3, "Unified", "12288K"),
                if id % 2 == 0 {
                    "0-3,8-11"
                } else {
                    "11,10,9,8,3,2,1,0"
                },
            );
            let base = format!("sys/devices/system/cpu/cpu{id}/cpufreq");
            put(
                &f.0,
                &format!("{base}/cpuinfo_min_freq"),
                if id < 4 { "800000" } else { "600000" },
            );
            put(
                &f.0,
                &format!("{base}/cpuinfo_max_freq"),
                if id < 4 { "5000000" } else { "3700000" },
            );
            put(&f.0, &format!("{base}/scaling_driver"), "intel_pstate");
        }
        let value = read(&f.0);
        assert_eq!(value["threadIds"], json!(ids));
        assert_eq!(value["threads"], 8);
        assert_eq!(value["cores"], 6);
        assert_eq!(value["coreOf"].as_array().unwrap().len(), 12);
        assert!(
            value["coreOf"].as_array().unwrap()[4..8]
                .iter()
                .all(Value::is_null)
        );
        assert_eq!(value["coreOf"][8], 2);
        assert_eq!(value["coreOf"][11], 5);
        assert_eq!(value["l3Groups"], json!(["0-3,8-11"]));
        assert_eq!(value["freq"]["minKHz"], 600_000);
        assert_eq!(value["freq"]["maxKHz"], 5_000_000);
        assert!(
            value["codename"].is_null(),
            "ukjent modell får ikke denne maskinens Ryzen-fakta"
        );
        let caches = value["caches"].as_array().unwrap();
        let instances = |level: u64, size: u64| {
            caches
                .iter()
                .find(|c| c["level"] == level && c["size"] == size)
                .unwrap()
        };
        assert_eq!(instances(1, 64 * 1024)["instances"], 2);
        assert_eq!(instances(1, 48 * 1024)["instances"], 4);
        assert_eq!(instances(2, 2 * 1024 * 1024)["instances"], 2);
        assert_eq!(instances(2, 4 * 1024 * 1024)["instances"], 1);
        assert_eq!(instances(3, 12 * 1024 * 1024)["instances"], 1);
        let l1_total: u64 = caches
            .iter()
            .filter(|c| c["level"] == 1)
            .map(|c| c["total"].as_u64().unwrap())
            .sum();
        assert_eq!(
            l1_total,
            320 * 1024,
            "CPU0s størrelse ganger antall tråder ville gitt feil total"
        );
    }

    #[test]
    fn uten_cpu0_brukes_faktiske_cpuinfo_ider_og_aktiv_cpufreq() {
        let f = Fixture::new("no-cpu0");
        put(
            &f.0,
            "proc/cpuinfo",
            "processor: 8\nmodel name: Ukjent CPU\n\nprocessor: 9\nmodel name: Ukjent CPU\n\n",
        );
        for id in [8, 9] {
            put(
                &f.0,
                &format!("sys/devices/system/cpu/cpu{id}/topology/core_id"),
                &(id - 4).to_string(),
            );
            put(
                &f.0,
                &format!("sys/devices/system/cpu/cpu{id}/topology/physical_package_id"),
                "1",
            );
            put(
                &f.0,
                &format!("sys/devices/system/cpu/cpu{id}/cpufreq/cpuinfo_max_freq"),
                "4500000",
            );
        }
        put(
            &f.0,
            "sys/devices/system/cpu/cpu8/microcode/version",
            "0x123",
        );
        let value = read(&f.0);
        assert_eq!(value["threadIds"], json!([8, 9]));
        assert_eq!(value["threads"], 2);
        assert_eq!(value["cores"], 2);
        assert!(value["coreOf"][0].is_null());
        assert_eq!(value["coreOf"][8], 4);
        assert_eq!(value["freq"]["maxKHz"], 4_500_000);
        assert_eq!(value["microcode"], "0x123");
    }

    #[test]
    fn ukjent_cacheidentitet_beholder_stoerrelsen_uten_aa_finne_paa_antall() {
        let f = Fixture::new("unknown-cache");
        put(&f.0, "sys/devices/system/cpu/online", "0-1");
        for cpu in [0, 1] {
            let dir = format!("sys/devices/system/cpu/cpu{cpu}/cache/index0");
            put(&f.0, &format!("{dir}/level"), "1");
            put(&f.0, &format!("{dir}/type"), "Data");
            put(&f.0, &format!("{dir}/size"), "32K");
        }
        let value = read(&f.0);
        let caches = value["caches"].as_array().unwrap();
        assert_eq!(caches.len(), 1);
        assert_eq!(caches[0]["size"], 32 * 1024);
        assert!(
            caches[0]["instances"].is_null(),
            "uten id/deltakerliste vet vi ikke fysisk antall"
        );
        assert!(caches[0]["total"].is_null());
        assert!(caches[0]["sharedBy"].is_null());
    }

    #[test]
    fn proc_stat_fallback_bruker_ider_naar_online_og_cpuinfo_mangler() {
        let f = Fixture::new("stat-only");
        put(
            &f.0,
            "proc/stat",
            "cpu 20 0 0 180 0\ncpu0 10 0 0 90 0\ncpu8 10 0 0 90 0\nintr 1\n",
        );
        let value = read(&f.0);
        assert_eq!(value["threadIds"], json!([0, 8]));
        assert_eq!(value["threads"], 2);
        assert_eq!(value["coreOf"].as_array().unwrap().len(), 9);
        assert!(
            value["cores"].is_null(),
            "logiske ID-er beviser ikke antall fysiske kjerner"
        );
        assert_eq!(value["name"], "Ukjent prosessor");
    }

    #[test]
    fn to_sokler_beholder_raa_kjerne_id_og_sparse_pakke_id() {
        let f = Fixture::new("two-packages");
        put(&f.0, "sys/devices/system/cpu/online", "0-1,8-9");
        for id in [0, 1, 8, 9] {
            let dir = format!("sys/devices/system/cpu/cpu{id}/topology");
            put(&f.0, &format!("{dir}/core_id"), "0");
            put(
                &f.0,
                &format!("{dir}/physical_package_id"),
                if id < 8 { "0" } else { "1" },
            );
        }
        for partial_l3 in [false, true] {
            if partial_l3 {
                for id in [0, 1] {
                    cache(&f.0, id, 3, 0, (3, "Unified", "8192K"), "0-1");
                }
            }
            let value = read(&f.0);
            assert_eq!(value["cores"], 2);
            assert_eq!(value["packages"], 2);
            assert_eq!(value["threads"], 4);
            assert_eq!(value["threadIds"], json!([0, 1, 8, 9]));
            assert_eq!(value["coreOf"][0], 0);
            assert_eq!(value["coreOf"][8], 0);
            assert_eq!(value["packageOf"][0], 0);
            assert_eq!(value["packageOf"][8], 1);
            assert!(
                value["packageOf"].as_array().unwrap()[2..8]
                    .iter()
                    .all(Value::is_null)
            );
            assert_eq!(
                value["l3Groups"].as_array().unwrap().len(),
                usize::from(partial_l3)
            );
        }
    }

    #[test]
    fn denne_maskinen() {
        let v = read(Path::new("/"));
        assert!(v["threads"].as_u64().unwrap() >= 1);
        assert!(v["name"].as_str().is_some_and(|n| !n.is_empty()));
    }
}
