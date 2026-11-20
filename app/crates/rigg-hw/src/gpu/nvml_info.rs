//! Faste data fra NVML til Grafikkort-siden (GPU-Z-delen): arkitektur,
//! CUDA-kjerner, minnebuss, BAR1, klokkeområde per P-tilstand, GSP, InfoROM,
//! kjølerinfo og resten av `maskinvare-revisjon.md` kap. 6.
//!
//! Grunnmuren kaller [`read_step`] for `step` = 0 .. [`STEPS`] (ett steg per
//! runde, etter de grunnleggende dataene), så lesingen fordeles over de første
//! sekundene og ikke vekker kortet. Resultatet legges i `GpuStatic::extra`
//! (fritt JSON) og kommer til vinduet med `get_static`.
//!
//! **Bare lesing.** Alle kall går gjennom `NvmlLib`, som bare slår opp
//! `nvmlDeviceGet*`/`nvmlSystemGet*`. Strukturene og versjonsfeltene følger
//! `nvml.h` og er kontrollert mot `docs/produksjon/verktoy/nvml_probe.py`,
//! som kjørte de samme kallene på dette kortet.
//!
//! Det kortet ikke støtter (`NOT_SUPPORTED`), står i `extra.na` med koden, så
//! siden kan si «Ikke støttet» med grunnen i stedet for å gjette.

use super::nvml::{Dev, NvmlLib, SUCCESS, code_name};
use rigg_core::tr;
use serde_json::{Map, Value, json};
use std::ffi::{CStr, c_char, c_int, c_uint, c_ulonglong};

/// Antall steg. Tunge steg (klokkeområder per P-tilstand) er delt i to.
pub const STEPS: usize = 7;

const MIB: f64 = 1024.0 * 1024.0;
/// `NVML_PSTATE_UNKNOWN`.
const PSTATE_UNKNOWN: c_int = 32;
/// `NVML_VALUE_NOT_AVAILABLE` for `usedGpuMemory`.
const VALUE_NA: u64 = u64::MAX;

/// Versjonsfeltet NVML bruker: størrelsen på strukturen og versjonen øverst.
fn ver<T>(v: u32) -> c_uint {
    (std::mem::size_of::<T>() as c_uint) | (v << 24)
}

fn obj(extra: &mut Value) -> &mut Map<String, Value> {
    if !extra.is_object() {
        *extra = Value::Object(Map::new());
    }
    extra.as_object_mut().unwrap()
}

/// Lagrer en verdi, eller koden i `na` når kallet feilet.
fn put<T: Into<Value>>(extra: &mut Value, key: &str, r: Result<T, c_int>) {
    match r {
        Ok(v) => {
            obj(extra).insert(key.into(), v.into());
        }
        Err(code) => {
            let o = obj(extra);
            let na = o.entry("na").or_insert_with(|| Value::Object(Map::new()));
            if let Some(m) = na.as_object_mut() {
                m.insert(key.into(), Value::String(code_name(code).into()));
            }
        }
    }
}

pub fn brand_name(b: u32) -> &'static str {
    match b {
        1 => "Quadro",
        2 => "Tesla",
        3 => "NVS",
        4 => "GRID",
        5 => "GeForce",
        6 => "Titan",
        7..=11 => tr!("NVIDIA virtuell GPU"),
        12 => "Quadro RTX",
        13 => "NVIDIA RTX",
        14 => "NVIDIA",
        15 => "GeForce RTX",
        16 => "Titan RTX",
        _ => tr!("Ukjent"),
    }
}

pub fn arch_name(a: u32) -> Option<&'static str> {
    Some(match a {
        2 => "Kepler",
        3 => "Maxwell",
        4 => "Pascal",
        5 => "Volta",
        6 => "Turing",
        7 => "Ampere",
        8 => "Ada Lovelace",
        9 => "Hopper",
        10 => "Blackwell",
        _ => return None,
    })
}

// ---------------------------------------------------------------- Kall med egne strukturer

fn bar1(lib: &NvmlLib, d: Dev) -> Result<(u64, u64), c_int> {
    #[repr(C)]
    struct Bar1 {
        total: c_ulonglong,
        free: c_ulonglong,
        used: c_ulonglong,
    }
    let mut b = Bar1 { total: 0, free: 0, used: 0 };
    // SAFETY: `nvmlDeviceGetBAR1MemoryInfo(nvmlDevice_t, nvmlBAR1Memory_t*)`.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut Bar1) -> c_int>("nvmlDeviceGetBAR1MemoryInfo").ok_or(13)?;
        let r = f(d, &mut b);
        if r == SUCCESS { Ok((b.total, b.used)) } else { Err(r) }
    }
}

/// `nvmlDeviceGetGspFirmwareVersion(nvmlDevice_t, char*)`: bufferen er 64 byte i nvml.h.
fn gsp_version(lib: &NvmlLib, d: Dev) -> Result<String, c_int> {
    let mut buf = [0 as c_char; 128];
    // SAFETY: bufferen er dobbelt så stor som NVML_GSP_FIRMWARE_VERSION_BUF_SIZE.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut c_char) -> c_int>("nvmlDeviceGetGspFirmwareVersion").ok_or(13)?;
        let r = f(d, buf.as_mut_ptr());
        buf[127] = 0;
        if r == SUCCESS { Ok(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()) } else { Err(r) }
    }
}

/// `nvmlDeviceGetInforomVersion(nvmlDevice_t, nvmlInforomObject_t, char*, unsigned)`.
fn inforom_object(lib: &NvmlLib, d: Dev, object: c_uint) -> Result<String, c_int> {
    let mut buf = [0 as c_char; 64];
    // SAFETY: lengden sendes med.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, c_uint, *mut c_char, c_uint) -> c_int>("nvmlDeviceGetInforomVersion").ok_or(13)?;
        let r = f(d, object, buf.as_mut_ptr(), buf.len() as c_uint);
        if r == SUCCESS { Ok(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()) } else { Err(r) }
    }
}

fn supported_pstates(lib: &NvmlLib, d: Dev) -> Result<Vec<u32>, c_int> {
    let mut arr = [PSTATE_UNKNOWN; 16];
    // SAFETY: `(nvmlDevice_t, nvmlPstates_t*, unsigned size)`; size er antall plasser.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut c_int, c_uint) -> c_int>("nvmlDeviceGetSupportedPerformanceStates").ok_or(13)?;
        let r = f(d, arr.as_mut_ptr(), arr.len() as c_uint);
        if r != SUCCESS {
            return Err(r);
        }
    }
    let mut v: Vec<u32> = arr.iter().filter(|p| (0..16).contains(*p)).map(|p| *p as u32).collect();
    v.sort();
    v.dedup();
    Ok(v)
}

fn minmax_of_pstate(lib: &NvmlLib, d: Dev, clock: c_uint, pstate: c_uint) -> Result<(u32, u32), c_int> {
    let (mut lo, mut hi): (c_uint, c_uint) = (0, 0);
    // SAFETY: `(nvmlDevice_t, nvmlClockType_t, nvmlPstates_t, unsigned*, unsigned*)`.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, c_uint, c_uint, *mut c_uint, *mut c_uint) -> c_int>("nvmlDeviceGetMinMaxClockOfPState").ok_or(13)?;
        let r = f(d, clock, pstate, &mut lo, &mut hi);
        if r == SUCCESS { Ok((lo, hi)) } else { Err(r) }
    }
}

/// Klokkeoffset nå og kortets område (`nvmlClockOffset_t` v1).
fn clock_offsets(lib: &NvmlLib, d: Dev, clock: c_int, pstate: c_int) -> Result<(i32, i32, i32), c_int> {
    #[repr(C)]
    struct Off {
        version: c_uint,
        kind: c_int,
        pstate: c_int,
        now: c_int,
        min: c_int,
        max: c_int,
    }
    let mut o = Off { version: ver::<Off>(1), kind: clock, pstate, now: 0, min: 0, max: 0 };
    // SAFETY: strukturen og versjonsfeltet følger nvml.h.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut Off) -> c_int>("nvmlDeviceGetClockOffsets").ok_or(13)?;
        let r = f(d, &mut o);
        if r == SUCCESS { Ok((o.now, o.min, o.max)) } else { Err(r) }
    }
}

fn margin_temperature(lib: &NvmlLib, d: Dev) -> Result<i32, c_int> {
    #[repr(C)]
    struct Margin {
        version: c_uint,
        margin: c_int,
    }
    let mut m = Margin { version: ver::<Margin>(1), margin: 0 };
    // SAFETY: `nvmlDeviceGetMarginTemperature(nvmlDevice_t, nvmlMarginTemperature_t*)`.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut Margin) -> c_int>("nvmlDeviceGetMarginTemperature").ok_or(13)?;
        let r = f(d, &mut m);
        if r == SUCCESS { Ok(m.margin) } else { Err(r) }
    }
}

/// Kjølerinfo for én vifte (`nvmlCoolerInfo_t` v1): signaltype og mål (bitmaske).
fn cooler_info(lib: &NvmlLib, d: Dev, index: c_uint) -> Result<(i32, i32), c_int> {
    #[repr(C)]
    struct Cooler {
        version: c_uint,
        index: c_uint,
        signal: c_int,
        target: c_int,
    }
    let mut c = Cooler { version: ver::<Cooler>(1), index, signal: 0, target: 0 };
    // SAFETY: strukturen og versjonsfeltet følger nvml.h.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut Cooler) -> c_int>("nvmlDeviceGetCoolerInfo").ok_or(13)?;
        let r = f(d, &mut c);
        if r == SUCCESS { Ok((c.signal, c.target)) } else { Err(r) }
    }
}

fn thermal_settings(lib: &NvmlLib, d: Dev) -> Result<Vec<Value>, c_int> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Sensor {
        controller: c_int,
        min: c_int,
        max: c_int,
        cur: c_int,
        target: c_int,
    }
    #[repr(C)]
    struct Settings {
        count: c_uint,
        sensor: [Sensor; 3],
    }
    let z = Sensor { controller: 0, min: 0, max: 0, cur: 0, target: 0 };
    let mut s = Settings { count: 0, sensor: [z; 3] };
    // SAFETY: `(nvmlDevice_t, unsigned sensorIndex, nvmlGpuThermalSettings_t*)`; 15 = NVML_THERMAL_TARGET_ALL.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, c_uint, *mut Settings) -> c_int>("nvmlDeviceGetThermalSettings").ok_or(13)?;
        let r = f(d, 15, &mut s);
        if r != SUCCESS {
            return Err(r);
        }
    }
    Ok(s.sensor.iter().take(s.count.min(3) as usize).map(|x| json!({"controller": x.controller, "min": x.min, "max": x.max, "cur": x.cur, "target": x.target})).collect())
}

fn supported_mem_clocks(lib: &NvmlLib, d: Dev) -> Result<Vec<u32>, c_int> {
    let mut n: c_uint = 32;
    let mut arr = [0 as c_uint; 32];
    // SAFETY: `(nvmlDevice_t, unsigned* count, unsigned* clocks)`; count er plassene inn og antallet ut.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> c_int>("nvmlDeviceGetSupportedMemoryClocks").ok_or(13)?;
        let r = f(d, &mut n, arr.as_mut_ptr());
        if r != SUCCESS {
            return Err(r);
        }
    }
    Ok(arr[..(n as usize).min(32)].to_vec())
}

/// Prosessene som bruker kortet: PID og videominne (`nvmlProcessInfo_t`, v3-kallene).
fn running(lib: &NvmlLib, d: Dev, name: &'static str) -> Result<Vec<(u32, Option<u64>)>, c_int> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Proc {
        pid: c_uint,
        used: c_ulonglong,
        gi: c_uint,
        ci: c_uint,
    }
    let mut n: c_uint = 64;
    let mut arr = [Proc { pid: 0, used: 0, gi: 0, ci: 0 }; 64];
    // SAFETY: `(nvmlDevice_t, unsigned* infoCount, nvmlProcessInfo_t* infos)`; count er plassene inn.
    unsafe {
        let f = lib.sym::<unsafe extern "C" fn(Dev, *mut c_uint, *mut Proc) -> c_int>(name).ok_or(13)?;
        let r = f(d, &mut n, arr.as_mut_ptr());
        if r != SUCCESS {
            return Err(r);
        }
    }
    Ok(arr[..(n as usize).min(64)].iter().map(|p| (p.pid, (p.used != VALUE_NA && p.used < (1 << 62)).then_some(p.used))).collect())
}

/// Prosesslista med navn fra `/proc/<pid>/comm`. Kost 0,7 ms i P8 for begge kallene.
pub fn processes(lib: &NvmlLib, d: Dev) -> Value {
    let mut list: Vec<Value> = Vec::new();
    let mut err = None;
    for (name, kind) in [("nvmlDeviceGetGraphicsRunningProcesses_v3", tr!("grafikk")), ("nvmlDeviceGetComputeRunningProcesses_v3", tr!("compute"))] {
        match running(lib, d, name) {
            Ok(v) => {
                for (pid, used) in v {
                    if let Some(x) = list.iter_mut().find(|x| x["pid"] == pid) {
                        // Samme prosess i begge listene: «grafikk og compute».
                        x["kind"] = json!(tr!("grafikk og compute"));
                        continue;
                    }
                    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string());
                    list.push(json!({"pid": pid, "name": comm, "vramMiB": used.map(|u| (u as f64 / MIB).round()), "kind": kind}));
                }
            }
            Err(c) => err = Some(code_name(c)),
        }
    }
    list.sort_by(|a, b| b["vramMiB"].as_f64().unwrap_or(-1.0).total_cmp(&a["vramMiB"].as_f64().unwrap_or(-1.0)));
    json!({"t": rigg_core::now_ms(), "list": list, "error": err})
}

/// `extra.styring` til Kontroller og Viftekurve, bygd av det steg 4 og 5 har
/// lest: klokkeoffset med kortets område (minnet i effektiv MHz, NVML oppgir
/// dobbel verdi), temperaturmålet med det akustiske området, og viftestyring
/// og målfart per vifte. Felt som ikke kunne leses, er utelatt.
pub fn styring(extra: &Value) -> Value {
    let mut o = Map::new();
    let off = |k: &str, div: f64| -> Option<Value> {
        let v = extra.get(k)?;
        Some(json!({"value": v["now"].as_f64()? / div, "min": v["min"].as_f64()? / div, "max": v["max"].as_f64()? / div}))
    };
    if let Some(v) = off("offsetGfx", 1.0) {
        o.insert("coreOffsetMHz".into(), v);
    }
    if let Some(v) = off("offsetMem", 2.0) {
        o.insert("memOffsetMHz".into(), v);
    }
    if let (Some(cur), Some(lo), Some(hi)) = (extra["acousticCurr"].as_f64(), extra["acousticMin"].as_f64(), extra["acousticMax"].as_f64()) {
        o.insert("tempTarget".into(), json!({"value": cur, "min": lo, "max": hi}));
    }
    if let Some(fans) = extra["fans"].as_array() {
        let policy: Vec<Value> = fans.iter().map(|f| f["policy"].clone()).collect();
        let target: Vec<Value> = fans.iter().map(|f| f["target"].clone()).collect();
        if policy.iter().any(Value::is_u64) {
            o.insert("fanPolicy".into(), json!(policy));
        }
        if target.iter().any(Value::is_number) {
            o.insert("fanTarget".into(), json!(target));
        }
    }
    Value::Object(o)
}

/// «Open Kernel Module» eller den lukkede modulen, fra `/proc/driver/nvidia/version`.
fn kernel_module() -> Option<String> {
    let t = std::fs::read_to_string("/proc/driver/nvidia/version").ok()?;
    let first = t.lines().next()?;
    Some(if first.contains("Open Kernel Module") { "Open Kernel Module".into() } else { tr!("Lukket kjernemodul").into() })
}

/// Leser steg `step` for kortet `dev` og legger verdiene i `extra`.
pub fn read_step(lib: &NvmlLib, d: Dev, step: usize, extra: &mut Value) {
    match step {
        // Identitet (alle hurtigbufret i biblioteket, under 1 ms til sammen).
        0 => {
            put(extra, "brandId", lib.dev_u32(d, "nvmlDeviceGetBrand"));
            if let Some(b) = extra.get("brandId").and_then(Value::as_u64) {
                obj(extra).insert("brand".into(), json!(brand_name(b as u32)));
            }
            put(extra, "archId", lib.dev_u32(d, "nvmlDeviceGetArchitecture"));
            if let Some(a) = extra.get("archId").and_then(Value::as_u64).and_then(|a| arch_name(a as u32)) {
                obj(extra).insert("architecture".into(), json!(a));
            }
            put(extra, "computeCap", lib.dev_u32_pair(d, "nvmlDeviceGetCudaComputeCapability").map(|(a, b)| format!("{a}.{b}")));
            put(extra, "cudaCores", lib.dev_u32(d, "nvmlDeviceGetNumGpuCores"));
            put(extra, "busWidth", lib.dev_u32(d, "nvmlDeviceGetMemoryBusWidth"));
            put(extra, "boardId", lib.dev_u32(d, "nvmlDeviceGetBoardId").map(|b| format!("0x{b:x}")));
            put(extra, "serial", lib.dev_string(d, "nvmlDeviceGetSerial"));
            match lib.dev_string(d, "nvmlDeviceGetBoardPartNumber") {
                Ok(s) if !s.trim().is_empty() => put(extra, "partNumber", Ok::<_, c_int>(s)),
                Ok(_) => put::<String>(extra, "partNumber", Err(super::nvml::ERROR_NOT_SUPPORTED)),
                Err(e) => put::<String>(extra, "partNumber", Err(e)),
            }
        }
        // Fastvare og BAR1.
        1 => {
            put(extra, "gspFirmware", gsp_version(lib, d));
            put(extra, "gspMode", lib.dev_u32_pair(d, "nvmlDeviceGetGspFirmwareMode").map(|(on, def)| json!({"enabled": on == 1, "default": def == 1})));
            put(extra, "inforomImage", lib.dev_string(d, "nvmlDeviceGetInforomImageVersion"));
            put(extra, "inforomOem", inforom_object(lib, d, 0));
            put(extra, "bar1", bar1(lib, d).map(|(t, u)| json!({"totalMiB": (t as f64 / MIB).round(), "usedMiB": (u as f64 / MIB).round()})));
            if let Some(m) = kernel_module() {
                obj(extra).insert("kernelModule".into(), json!(m));
            }
        }
        // P-tilstandene og grafikkklokkens område i hver (1,3–1,7 ms per kall i P8).
        2 => {
            put(extra, "pstates", supported_pstates(lib, d).map(|v| json!(v.iter().map(|p| format!("P{p}")).collect::<Vec<_>>())));
            let ps: Vec<u32> = extra.get("pstates").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| p.as_str()?.strip_prefix('P')?.parse().ok()).collect()).unwrap_or_default();
            let rows: Vec<Value> = ps
                .iter()
                .map(|p| {
                    let g = minmax_of_pstate(lib, d, super::nvml::CLOCK_GRAPHICS, *p).ok();
                    json!({"p": format!("P{p}"), "gfx": g.map(|(a, b)| json!([a, b]))})
                })
                .collect();
            obj(extra).insert("pstateClocks".into(), json!(rows));
        }
        // Minneklokkens område per P-tilstand.
        3 => {
            if let Some(rows) = extra.get("pstateClocks").and_then(Value::as_array).cloned() {
                let rows: Vec<Value> = rows
                    .into_iter()
                    .map(|mut r| {
                        let p: Option<u32> = r["p"].as_str().and_then(|s| s.strip_prefix('P')?.parse().ok());
                        if let Some(p) = p
                            && let Ok((a, b)) = minmax_of_pstate(lib, d, super::nvml::CLOCK_MEM, p)
                        {
                            r["mem"] = json!([a, b]);
                        }
                        r
                    })
                    .collect();
                obj(extra).insert("pstateClocks".into(), json!(rows));
            }
            put(extra, "memClocks", supported_mem_clocks(lib, d));
        }
        // Grenser: effektgrense, akustisk område, offset med område, det som ikke støttes.
        4 => {
            put(extra, "powerLimit", lib.dev_u32(d, "nvmlDeviceGetPowerManagementLimit").map(|mw| mw as f64 / 1000.0));
            let th = |t| lib.dev_u32_arg(d, "nvmlDeviceGetTemperatureThreshold", t);
            put(extra, "acousticMin", th(4));
            put(extra, "acousticCurr", th(5));
            put(extra, "acousticMax", th(6));
            put(extra, "memMax", th(2));
            put(extra, "gpsCurr", th(7));
            put(extra, "margin", margin_temperature(lib, d));
            put(extra, "offsetGfx", clock_offsets(lib, d, 0, 0).map(|(n, a, b)| json!({"now": n, "min": a, "max": b})));
            put(extra, "offsetMem", clock_offsets(lib, d, 2, 0).map(|(n, a, b)| json!({"now": n, "min": a, "max": b})));
            put(extra, "appClockGfx", lib.dev_u32_arg(d, "nvmlDeviceGetApplicationsClock", 0));
            put(extra, "defaultAppClockGfx", lib.dev_u32_arg(d, "nvmlDeviceGetDefaultApplicationsClock", 0));
            put(extra, "customerBoostGfx", lib.dev_u32_arg(d, "nvmlDeviceGetMaxCustomerBoostClock", 0));
        }
        // Viftene og tilstandene: kjølerinfo, styring, målfart, persistence, skjerm.
        5 => {
            let n = lib.dev_u32(d, "nvmlDeviceGetNumFans").unwrap_or(0).min(8);
            let fans: Vec<Value> = (0..n)
                .map(|i| {
                    let co = cooler_info(lib, d, i).ok();
                    json!({
                        "policy": lib.dev_u32_arg(d, "nvmlDeviceGetFanControlPolicy_v2", i).ok(),
                        "target": lib.dev_u32_arg(d, "nvmlDeviceGetTargetFanSpeed", i).ok(),
                        "signal": co.map(|c| c.0),
                        "coolerTarget": co.map(|c| c.1),
                    })
                })
                .collect();
            obj(extra).insert("fans".into(), json!(fans));
            put(extra, "persistence", lib.dev_u32(d, "nvmlDeviceGetPersistenceMode").map(|v| v == 1));
            put(extra, "displayActive", lib.dev_u32(d, "nvmlDeviceGetDisplayActive").map(|v| v == 1));
            put(extra, "computeMode", lib.dev_u32(d, "nvmlDeviceGetComputeMode"));
            // Steg 4 og 5 er lest: det Kontroller og Viftekurve trenger, samlet.
            let st = styring(extra);
            obj(extra).insert("styring".into(), st);
        }
        // PCIe, termiske sensorer og prosessene (et øyeblikksbilde).
        6 => {
            put(extra, "thermal", thermal_settings(lib, d));
            put(extra, "pcieReplay", lib.dev_u32(d, "nvmlDeviceGetPcieReplayCounter"));
            put(extra, "pcieGenMaxGpu", lib.dev_u32(d, "nvmlDeviceGetGpuMaxPcieLinkGeneration"));
            put(extra, "pcieGenMaxSys", lib.dev_u32(d, "nvmlDeviceGetMaxPcieLinkGeneration"));
            put(extra, "pcieWidthMax", lib.dev_u32(d, "nvmlDeviceGetMaxPcieLinkWidth"));
            obj(extra).insert("processes".into(), processes(lib, d));
            if let Ok((bus, _, _)) = lib.pci(d) {
                let addr = super::sysfs_addr(&bus);
                obj(extra).insert("pci".into(), crate::sys::pci::read_gpu(std::path::Path::new("/"), &addr));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navn_og_na() {
        assert_eq!(brand_name(5), "GeForce");
        assert_eq!(arch_name(6), Some("Turing"));
        assert_eq!(arch_name(99), None);
        let mut e = Value::Null;
        put(&mut e, "cudaCores", Ok::<u32, c_int>(2944));
        put::<String>(&mut e, "serial", Err(3));
        assert_eq!(e["cudaCores"], 2944);
        assert_eq!(e["na"]["serial"], "NVML_ERROR_NOT_SUPPORTED");
        assert!(e.get("serial").is_none());
    }

    #[test]
    fn styring_fra_det_som_er_lest() {
        // Tallene fra RTX 2080 (maskinvare-revisjon kap. 8): minnet −2000…+6000 i NVML.
        let extra = json!({
            "offsetGfx": {"now": 0, "min": -1000, "max": 1000},
            "offsetMem": {"now": 400, "min": -2000, "max": 6000},
            "acousticMin": 65, "acousticCurr": 83, "acousticMax": 88,
            "fans": [{"policy": 0, "target": 35}, {"policy": 0, "target": 35}],
        });
        let s = styring(&extra);
        assert_eq!(s["coreOffsetMHz"], json!({"value": 0.0, "min": -1000.0, "max": 1000.0}));
        assert_eq!(s["memOffsetMHz"], json!({"value": 200.0, "min": -1000.0, "max": 3000.0}), "effektiv MHz");
        assert_eq!(s["tempTarget"], json!({"value": 83.0, "min": 65.0, "max": 88.0}));
        assert_eq!(s["fanPolicy"], json!([0, 0]));
        assert_eq!(s["fanTarget"], json!([35, 35]));
        // Det som ikke kunne leses, er utelatt (siden sier «Leses ikke ennå»).
        let s = styring(&json!({"fans": [{"policy": null, "target": null}], "na": {"offsetGfx": "NVML_ERROR_NOT_SUPPORTED"}}));
        assert_eq!(s, json!({}));
    }

    #[test]
    fn strukturene_har_riktig_størrelse() {
        // nvmlClockOffset_t v1 er 24 byte, nvmlCoolerInfo_t 16, nvmlMarginTemperature_t 8.
        assert_eq!(ver::<[u32; 6]>(1), 24 | (1 << 24));
        assert_eq!(ver::<[u32; 4]>(1), 16 | (1 << 24));
        assert_eq!(ver::<[u32; 2]>(1), 8 | (1 << 24));
    }
}
