//! Faste prosessordata til System-siden (CPU-Z): navn, familie, modell,
//! stepping, mikrokode, kjerner og tråder, CCX, cache, flagg og cpufreq-område.
//!
//! Port av `readCpu` i `server/system.js`, med tilleggene fra
//! maskinvare-revisjon kap. 7: kodenavn som oppslag (merket som oppslag),
//! CCX-inndeling fra L3-cachen, mikrokode fra sysfs og sårbarhetene.
//! Alt leses én gang (`get_system`), uten root. Multiplikator, bussklokke,
//! spenning og pakkeeffekt krever MSR eller RAPL (root) og leses ikke.

use serde_json::{Value, json};
use std::collections::BTreeSet;
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
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x01, 0x01), name: "Summit Ridge (Zen)", process: "14 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x08, 0x08), name: "Pinnacle Ridge (Zen+)", process: "12 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x11, 0x11), name: "Raven Ridge (Zen)", process: "14 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x18, 0x18), name: "Picasso (Zen+)", process: "12 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x60, 0x60), name: "Renoir (Zen 2)", process: "7 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x17, models: (0x71, 0x71), name: "Matisse (Zen 2)", process: "7 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x19, models: (0x21, 0x21), name: "Vermeer (Zen 3)", process: "7 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x19, models: (0x50, 0x50), name: "Cezanne (Zen 3)", process: "7 nm", socket: "AM4" },
    Codename { vendor: "AuthenticAMD", family: 0x19, models: (0x61, 0x61), name: "Raphael (Zen 4)", process: "5 nm", socket: "AM5" },
    Codename { vendor: "AuthenticAMD", family: 0x1a, models: (0x44, 0x44), name: "Granite Ridge (Zen 5)", process: "4 nm", socket: "AM5" },
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
                    out.extend(a..=b.min(a + 4096));
                }
            }
            None => {
                if let Ok(a) = part.trim().parse() {
                    out.push(a);
                }
            }
        }
    }
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
        let kv: Block =
            block.lines().filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
        let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
        if !kv.iter().any(|(k, _)| k == "processor") {
            continue;
        }
        cores.insert((get("physical id").unwrap_or_else(|| "0".into()), get("core id").or_else(|| get("processor")).unwrap_or_default()));
        if n == 0 {
            first = kv;
        }
        n += 1;
    }
    (first, n, cores)
}

/// Prosessoren som JSON (samme felt som demoens `/api/system` › `cpu`).
pub fn read(root: &Path) -> Value {
    let text = std::fs::read_to_string(root.join("proc/cpuinfo")).unwrap_or_default();
    let (first, blocks, core_ids) = cpuinfo_blocks(&text);
    let get = |k: &str| first.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    let cpu_dir = root.join("sys/devices/system/cpu");
    // Tråder: sysfs først (cpuN), ellers antall blokker i cpuinfo.
    let sys_threads = txt(&cpu_dir.join("online")).map(|s| parse_cpu_list(&s).len()).unwrap_or(0);
    let threads = if sys_threads > 0 { sys_threads } else { blocks };
    // Kjerner: topologien i sysfs (core_id per pakke), ellers cpuinfo.
    let mut topo = BTreeSet::new();
    // Kjernen hver tråd hører til (Prosessor-siden grupperer trådene, runde 6A).
    let mut core_of: Vec<Value> = Vec::with_capacity(threads);
    for i in 0..threads {
        let t = cpu_dir.join(format!("cpu{i}/topology"));
        let c = txt(&t.join("core_id"));
        core_of.push(json!(c.as_deref().and_then(|c| c.parse::<u32>().ok())));
        if let (Some(p), Some(c)) = (txt(&t.join("physical_package_id")), c) {
            topo.insert((p, c));
        }
    }
    let cores = if !topo.is_empty() { topo.len() } else { core_ids.len() };
    let packages: BTreeSet<String> = topo.iter().map(|(p, _)| p.clone()).collect();

    let flags: Vec<String> = get("flags").unwrap_or_default().split_whitespace().map(str::to_string).collect();
    let fset: BTreeSet<&str> = flags.iter().map(String::as_str).collect();
    let important: Vec<Value> = IMPORTANT_FLAGS.iter().filter(|(f, _)| fset.contains(f)).map(|(f, l)| json!({"flag": f, "label": l})).collect();

    // Cache fra cpu0. Antall instanser = tråder / tråder som deler cachen.
    let mut caches = Vec::new();
    let mut l3_groups: Vec<String> = Vec::new();
    let cache_dir = cpu_dir.join("cpu0/cache");
    let mut idx: Vec<String> = std::fs::read_dir(&cache_dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("index")).collect())
        .unwrap_or_default();
    idx.sort();
    for name in &idx {
        let d = cache_dir.join(name);
        let size = txt(&d.join("size")).and_then(|s| parse_size(&s));
        let shared = txt(&d.join("shared_cpu_list")).map(|s| parse_cpu_list(&s).len()).unwrap_or(1).max(1);
        let instances = ((threads as f64 / shared as f64).round() as u64).max(1);
        let level = int(&d.join("level"));
        caches.push(json!({
            "level": level,
            "type": txt(&d.join("type")),
            "size": size,
            "instances": instances,
            "total": size.map(|s| s * instances),
            "ways": int(&d.join("ways_of_associativity")),
            "lineSize": int(&d.join("coherency_line_size")),
            "sharedBy": shared,
        }));
    }
    // CCX-er: én L3 per gruppe av tråder (Zen deler L3 per CCX).
    if let Some(l3) = idx.iter().find(|n| int(&cache_dir.join(n).join("level")) == Some(3)) {
        let mut seen = BTreeSet::new();
        for i in 0..threads {
            if let Some(list) = txt(&cpu_dir.join(format!("cpu{i}/cache/{l3}/shared_cpu_list")))
                && seen.insert(list.clone())
            {
                l3_groups.push(list);
            }
        }
    }

    let fd = cpu_dir.join("cpu0/cpufreq");
    let driver = txt(&fd.join("scaling_driver"));
    let freq = json!({
        "minKHz": int(&fd.join("cpuinfo_min_freq")),
        "maxKHz": int(&fd.join("cpuinfo_max_freq")),
        "driver": driver,
        "governors": txt(&fd.join("scaling_available_governors")).map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default(),
        "available": driver.is_some(),
        "pstatesKHz": txt(&fd.join("scaling_available_frequencies")).map(|s| s.split_whitespace().filter_map(|x| x.parse::<u64>().ok()).collect::<Vec<_>>()).unwrap_or_default(),
    });

    let vendor = get("vendor_id");
    let num = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
    let (family, model, stepping) = (num("cpu family"), num("model"), num("stepping"));
    let codename = match (vendor.as_deref(), family, model) {
        (Some(v), Some(f), Some(m)) => CODENAMES.iter().find(|c| c.vendor == v && c.family == f && (c.models.0..=c.models.1).contains(&m)),
        _ => None,
    };
    let revision = match (codename.map(|c| c.name), stepping) {
        (Some(n), Some(2)) if n.starts_with("Pinnacle Ridge") => Some("PiR-B2"),
        _ => None,
    };
    let microcode = txt(&cpu_dir.join("cpu0/microcode/version")).or_else(|| get("microcode"));

    // Sårbarhetene (lesbare for alle): navn → tekst.
    let mut vulns = serde_json::Map::new();
    if let Ok(rd) = std::fs::read_dir(cpu_dir.join("vulnerabilities")) {
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
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
        "packages": packages.len().max(1),
        "caches": caches,
        "l3Groups": l3_groups,
        "coreOf": core_of,
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
        let root = std::env::temp_dir().join(format!("rigg-cpuinfo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let block = |n: u32, core: u32| {
            format!("processor\t: {n}\nvendor_id\t: AuthenticAMD\ncpu family\t: 23\nmodel\t\t: 8\nmodel name\t: AMD Ryzen 7 2700X Eight-Core Processor\nstepping\t: 2\nmicrocode\t: 0x800820d\nphysical id\t: 0\ncore id\t\t: {core}\nflags\t\t: fpu sse sse2 avx avx2 lm svm\naddress sizes\t: 43 bits physical, 48 bits virtual\n\n")
        };
        let text: String = (0..4).map(|i| block(i, i % 2)).collect();
        put(&root, "proc/cpuinfo", &text);
        put(&root, "sys/devices/system/cpu/online", "0-3\n");
        for i in 0..4 {
            put(&root, &format!("sys/devices/system/cpu/cpu{i}/topology/core_id"), &format!("{}\n", i % 2));
            put(&root, &format!("sys/devices/system/cpu/cpu{i}/topology/physical_package_id"), "0\n");
            let l3 = if i % 2 == 0 { "0,2" } else { "1,3" };
            put(&root, &format!("sys/devices/system/cpu/cpu{i}/cache/index3/shared_cpu_list"), l3);
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
        put(&root, "sys/devices/system/cpu/cpu0/cpufreq/scaling_driver", "acpi-cpufreq");
        put(&root, "sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq", "3700000");
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
        let imp: Vec<&str> = v["important"].as_array().unwrap().iter().map(|x| x["label"].as_str().unwrap()).collect();
        assert_eq!(imp, vec!["SSE", "SSE2", "AVX", "AVX2", "x86-64", "AMD-V"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn denne_maskinen() {
        let v = read(Path::new("/"));
        assert!(v["threads"].as_u64().unwrap() >= 1);
        assert!(v["name"].as_str().is_some_and(|n| !n.is_empty()));
    }
}
