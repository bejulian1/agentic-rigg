//! NVML-backenden. **Bare lesing.**
//!
//! `libnvidia-ml.so.1` lastes med `dlopen` i prøvetråden i tjenesten, og hver
//! funksjon slås opp med [`NvmlLib::sym`], som bare godtar `nvmlInit_v2`,
//! `nvmlShutdown`, `nvmlErrorString` og navn som begynner med `nvmlDeviceGet`
//! eller `nvmlSystemGet`. Skrivende funksjoner (`nvmlDeviceSet*`,
//! `nvmlDeviceReset*`, `nvmlDeviceClear*` …) kan derfor ikke kalles herfra,
//! heller ikke ved en feil. Valget av egen binding i stedet for `nvml-wrapper`
//! står i `docs/produksjon/LOGG.md` (færre avhengigheter, og ingen
//! skrivemetoder finnes i det hele tatt).
//!
//! Tallene for kostnad per kall og prøveplanen står i `maskinvare-revisjon.md`.

use super::{GpuBackend, PcieLink, Plan, Round};
use rigg_core::model::{DeviceState, GpuInfo, GpuSample, GpuStatic};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulonglong, c_void};
use std::path::PathBuf;

type Ret = c_int;

/// Et kort (`nvmlDevice_t`). Ugjennomsiktig for oss: bare NVML leser pekeren.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Dev(*mut c_void);

impl Dev {
    pub fn null() -> Dev {
        Dev(std::ptr::null_mut())
    }
}

pub const SUCCESS: Ret = 0;
pub const ERROR_UNINITIALIZED: Ret = 1;
pub const ERROR_NOT_SUPPORTED: Ret = 3;
pub const ERROR_NO_PERMISSION: Ret = 4;
pub const ERROR_DRIVER_NOT_LOADED: Ret = 9;
pub const ERROR_LIBRARY_NOT_FOUND: Ret = 12;
pub const ERROR_GPU_IS_LOST: Ret = 15;
pub const ERROR_LIB_RM_VERSION_MISMATCH: Ret = 18;
pub const ERROR_UNKNOWN: Ret = 999;

/// Navnene som kan slås opp. Alt annet avvises før `dlsym`.
pub fn allowed(name: &str) -> bool {
    name.starts_with("nvmlDeviceGet") || name.starts_with("nvmlSystemGet") || matches!(name, "nvmlInit_v2" | "nvmlShutdown" | "nvmlErrorString")
}

/// Kode → navn til feilsøking («NVML_ERROR_GPU_IS_LOST»).
pub fn code_name(r: Ret) -> &'static str {
    match r {
        0 => "NVML_SUCCESS",
        1 => "NVML_ERROR_UNINITIALIZED",
        2 => "NVML_ERROR_INVALID_ARGUMENT",
        3 => "NVML_ERROR_NOT_SUPPORTED",
        4 => "NVML_ERROR_NO_PERMISSION",
        7 => "NVML_ERROR_INSUFFICIENT_SIZE",
        9 => "NVML_ERROR_DRIVER_NOT_LOADED",
        12 => "NVML_ERROR_LIBRARY_NOT_FOUND",
        13 => "NVML_ERROR_FUNCTION_NOT_FOUND",
        15 => "NVML_ERROR_GPU_IS_LOST",
        18 => "NVML_ERROR_LIB_RM_VERSION_MISMATCH",
        _ => "NVML_ERROR_UNKNOWN",
    }
}

/// Det åpne biblioteket.
pub struct NvmlLib {
    handle: *mut c_void,
    syms: RefCell<HashMap<&'static str, usize>>,
}

// Biblioteket brukes bare fra prøvetråden; pekeren flyttes dit én gang.
unsafe impl Send for NvmlLib {}

impl NvmlLib {
    /// `dlopen("libnvidia-ml.so.1")`. Gir en feiltekst når det mangler.
    pub fn load() -> Result<NvmlLib, String> {
        let name = CString::new("libnvidia-ml.so.1").unwrap();
        // SAFETY: dlopen med et gyldig, nullavsluttet navn.
        let h = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if h.is_null() {
            return Err("Fant ikke NVIDIA-driveren (libnvidia-ml.so.1)".into());
        }
        Ok(NvmlLib { handle: h, syms: RefCell::new(HashMap::new()) })
    }

    /// Slår opp en lesefunksjon. Panikk hvis navnet ikke er en lesefunksjon:
    /// det er en programmeringsfeil, og den skal stoppe før noe kalles.
    ///
    /// # Safety
    /// `F` må være funksjonspekertypen som stemmer med NVML-signaturen.
    pub unsafe fn sym<F: Copy>(&self, name: &'static str) -> Option<F> {
        assert!(allowed(name), "{name} er ikke en lesefunksjon og kan ikke slås opp");
        assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<usize>());
        let p = {
            let mut cache = self.syms.borrow_mut();
            *cache.entry(name).or_insert_with(|| {
                let c = CString::new(name).unwrap();
                // SAFETY: dlsym på et åpent bibliotek med gyldig navn.
                unsafe { libc::dlsym(self.handle, c.as_ptr()) as usize }
            })
        };
        if p == 0 {
            return None;
        }
        // SAFETY: p er en ikke-null funksjonspeker fra dlsym; kalleren lover typen.
        Some(unsafe { std::mem::transmute_copy::<usize, F>(&p) })
    }

    pub fn init(&self) -> Ret {
        // SAFETY: signaturen er `nvmlReturn_t nvmlInit_v2(void)`.
        unsafe {
            match self.sym::<unsafe extern "C" fn() -> Ret>("nvmlInit_v2") {
                Some(f) => f(),
                None => 13,
            }
        }
    }

    pub fn shutdown(&self) {
        // SAFETY: `nvmlReturn_t nvmlShutdown(void)`.
        unsafe {
            if let Some(f) = self.sym::<unsafe extern "C" fn() -> Ret>("nvmlShutdown") {
                f();
            }
        }
    }

    pub fn error_string(&self, r: Ret) -> String {
        // SAFETY: `const char* nvmlErrorString(nvmlReturn_t)`, statisk tekst.
        unsafe {
            match self.sym::<unsafe extern "C" fn(Ret) -> *const c_char>("nvmlErrorString") {
                Some(f) => {
                    let p = f(r);
                    if p.is_null() { code_name(r).into() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
                }
                None => code_name(r).into(),
            }
        }
    }

    /// `nvmlSystemGet…(char*, unsigned)`.
    pub fn system_string(&self, name: &'static str) -> Option<String> {
        let mut buf = [0 as c_char; 96];
        // SAFETY: bufferen er stor nok (NVML bruker høyst 80 tegn), og lengden sendes med.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(*mut c_char, c_uint) -> Ret>(name)?;
            (f(buf.as_mut_ptr(), buf.len() as c_uint) == SUCCESS).then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
        }
    }

    /// CUDA-versjonen som «13.3».
    pub fn cuda_version(&self) -> Option<String> {
        let mut v: c_int = 0;
        // SAFETY: `nvmlSystemGetCudaDriverVersion_v2(int*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(*mut c_int) -> Ret>("nvmlSystemGetCudaDriverVersion_v2")?;
            (f(&mut v) == SUCCESS).then(|| format!("{}.{}", v / 1000, v % 1000 / 10))
        }
    }

    pub fn count(&self) -> Result<u32, Ret> {
        let mut n: c_uint = 0;
        // SAFETY: `nvmlDeviceGetCount_v2(unsigned*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(*mut c_uint) -> Ret>("nvmlDeviceGetCount_v2").ok_or(13)?;
            let r = f(&mut n);
            if r == SUCCESS { Ok(n) } else { Err(r) }
        }
    }

    pub fn handle(&self, index: u32) -> Result<Dev, Ret> {
        let mut d: *mut c_void = std::ptr::null_mut();
        // SAFETY: `nvmlDeviceGetHandleByIndex_v2(unsigned, nvmlDevice_t*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(c_uint, *mut *mut c_void) -> Ret>("nvmlDeviceGetHandleByIndex_v2").ok_or(13)?;
            let r = f(index, &mut d);
            if r == SUCCESS { Ok(Dev(d)) } else { Err(r) }
        }
    }

    pub fn handle_by_pci(&self, bus_id: &str) -> Result<Dev, Ret> {
        let c = CString::new(bus_id).map_err(|_| 2)?;
        let mut d: *mut c_void = std::ptr::null_mut();
        // SAFETY: `nvmlDeviceGetHandleByPciBusId_v2(const char*, nvmlDevice_t*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Ret>("nvmlDeviceGetHandleByPciBusId_v2").ok_or(13)?;
            let r = f(c.as_ptr(), &mut d);
            if r == SUCCESS { Ok(Dev(d)) } else { Err(r) }
        }
    }

    // ---- Kall per kort. Alle gir Err(kode) ved feil. ----

    pub fn dev_string(&self, d: Dev, name: &'static str) -> Result<String, Ret> {
        let mut buf = [0 as c_char; 128];
        // SAFETY: `(nvmlDevice_t, char*, unsigned)`; NVML-tekstene er under 96 tegn.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut c_char, c_uint) -> Ret>(name).ok_or(13)?;
            let r = f(d, buf.as_mut_ptr(), buf.len() as c_uint);
            if r == SUCCESS { Ok(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()) } else { Err(r) }
        }
    }

    pub fn dev_u32(&self, d: Dev, name: &'static str) -> Result<u32, Ret> {
        let mut v: c_uint = 0;
        // SAFETY: `(nvmlDevice_t, unsigned*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut c_uint) -> Ret>(name).ok_or(13)?;
            let r = f(d, &mut v);
            if r == SUCCESS { Ok(v) } else { Err(r) }
        }
    }

    /// `(nvmlDevice_t, unsigned arg, unsigned*)`: temperatur, klokke, vifte, terskel.
    pub fn dev_u32_arg(&self, d: Dev, name: &'static str, arg: u32) -> Result<u32, Ret> {
        let mut v: c_uint = 0;
        // SAFETY: enum-argumentene i NVML er int-store; signaturen er (dev, enum/unsigned, unsigned*).
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, c_uint, *mut c_uint) -> Ret>(name).ok_or(13)?;
            let r = f(d, arg, &mut v);
            if r == SUCCESS { Ok(v) } else { Err(r) }
        }
    }

    pub fn dev_u64(&self, d: Dev, name: &'static str) -> Result<u64, Ret> {
        let mut v: c_ulonglong = 0;
        // SAFETY: `(nvmlDevice_t, unsigned long long*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut c_ulonglong) -> Ret>(name).ok_or(13)?;
            let r = f(d, &mut v);
            if r == SUCCESS { Ok(v) } else { Err(r) }
        }
    }

    pub fn dev_i32(&self, d: Dev, name: &'static str) -> Result<i32, Ret> {
        let mut v: c_int = 0;
        // SAFETY: `(nvmlDevice_t, int*)` (P-tilstand er en enum).
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut c_int) -> Ret>(name).ok_or(13)?;
            let r = f(d, &mut v);
            if r == SUCCESS { Ok(v) } else { Err(r) }
        }
    }

    /// `(nvmlDevice_t, unsigned*, unsigned*)`: to verdier (grenser, last med periode).
    pub fn dev_u32_pair(&self, d: Dev, name: &'static str) -> Result<(u32, u32), Ret> {
        let (mut a, mut b): (c_uint, c_uint) = (0, 0);
        // SAFETY: `(nvmlDevice_t, unsigned*, unsigned*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret>(name).ok_or(13)?;
            let r = f(d, &mut a, &mut b);
            if r == SUCCESS { Ok((a, b)) } else { Err(r) }
        }
    }

    pub fn utilization(&self, d: Dev) -> Result<(u32, u32), Ret> {
        #[repr(C)]
        struct Util {
            gpu: c_uint,
            memory: c_uint,
        }
        let mut u = Util { gpu: 0, memory: 0 };
        // SAFETY: `nvmlDeviceGetUtilizationRates(nvmlDevice_t, nvmlUtilization_t*)`.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut Util) -> Ret>("nvmlDeviceGetUtilizationRates").ok_or(13)?;
            let r = f(d, &mut u);
            if r == SUCCESS { Ok((u.gpu, u.memory)) } else { Err(r) }
        }
    }

    /// `nvmlDeviceGetMemoryInfo_v2` (ikke v1, som regner det reserverte som brukt). Byte: (total, reservert, brukt).
    pub fn memory_v2(&self, d: Dev) -> Result<(u64, u64, u64), Ret> {
        #[repr(C)]
        struct MemV2 {
            version: c_uint,
            total: c_ulonglong,
            reserved: c_ulonglong,
            free: c_ulonglong,
            used: c_ulonglong,
        }
        let mut m = MemV2 { version: (std::mem::size_of::<MemV2>() as c_uint) | (2 << 24), total: 0, reserved: 0, free: 0, used: 0 };
        // SAFETY: strukturen og versjonsfeltet følger nvml.h (nvmlMemory_v2_t).
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut MemV2) -> Ret>("nvmlDeviceGetMemoryInfo_v2").ok_or(13)?;
            let r = f(d, &mut m);
            if r == SUCCESS { Ok((m.total, m.reserved, m.used)) } else { Err(r) }
        }
    }

    /// Turtall for én vifte (`nvmlFanSpeedInfo_t` v1).
    pub fn fan_rpm(&self, d: Dev, fan: u32) -> Result<u32, Ret> {
        #[repr(C)]
        struct FanInfo {
            version: c_uint,
            fan: c_uint,
            speed: c_uint,
        }
        let mut s = FanInfo { version: (std::mem::size_of::<FanInfo>() as c_uint) | (1 << 24), fan, speed: 0 };
        // SAFETY: strukturen og versjonsfeltet følger nvml.h.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut FanInfo) -> Ret>("nvmlDeviceGetFanSpeedRPM").ok_or(13)?;
            let r = f(d, &mut s);
            if r == SUCCESS { Ok(s.speed) } else { Err(r) }
        }
    }

    /// PCI-info: (bus-ID som NVML skriver den, enhets-ID, subsystem-ID).
    pub fn pci(&self, d: Dev) -> Result<(String, u32, u32), Ret> {
        #[repr(C)]
        struct Pci {
            bus_id_legacy: [c_char; 16],
            domain: c_uint,
            bus: c_uint,
            device: c_uint,
            pci_device_id: c_uint,
            pci_sub_system_id: c_uint,
            bus_id: [c_char; 32],
        }
        let mut p = Pci { bus_id_legacy: [0; 16], domain: 0, bus: 0, device: 0, pci_device_id: 0, pci_sub_system_id: 0, bus_id: [0; 32] };
        // SAFETY: `nvmlDeviceGetPciInfo_v3(nvmlDevice_t, nvmlPciInfo_t*)`, struktur fra nvml.h.
        unsafe {
            let f = self.sym::<unsafe extern "C" fn(Dev, *mut Pci) -> Ret>("nvmlDeviceGetPciInfo_v3").ok_or(13)?;
            let r = f(d, &mut p);
            if r != SUCCESS {
                return Err(r);
            }
            Ok((CStr::from_ptr(p.bus_id.as_ptr()).to_string_lossy().into_owned(), p.pci_device_id, p.pci_sub_system_id))
        }
    }
}

impl Drop for NvmlLib {
    fn drop(&mut self) {
        // Biblioteket lukkes ikke med dlclose: libcuda, som NVML laster selv,
        // tåler ikke å bli lastet ut igjen. nvmlShutdown kalles av backenden.
    }
}

// NVML-konstanter.
pub const TEMPERATURE_GPU: u32 = 0;
pub const CLOCK_GRAPHICS: u32 = 0;
pub const CLOCK_SM: u32 = 1;
pub const CLOCK_MEM: u32 = 2;
pub const CLOCK_VIDEO: u32 = 3;
pub const THRESHOLD_SHUTDOWN: u32 = 0;
pub const THRESHOLD_SLOWDOWN: u32 = 1;
pub const THRESHOLD_GPU_MAX: u32 = 3;
pub const THRESHOLD_ACOUSTIC_CURR: u32 = 5;
/// Klokkebegrensning «tomgang» (`nvmlClocksEventReasonGpuIdle`).
pub const EVENT_GPU_IDLE: u64 = 0x1;

const MIB: f64 = 1024.0 * 1024.0;

struct Card {
    info: GpuInfo,
    dev: usize,
    nvml_bus: String,
    link: PcieLink,
    na: HashSet<&'static str>,
    last: GpuSample,
    energy: Option<(u64, i64)>,
    fans: u32,
    /// P-tilstanden som tall (teksten i `last.pstate` lages bare når den endres).
    pstate_n: Option<i32>,
    statics: GpuStatic,
    static_step: usize,
    lost_retry_at: i64,
}

impl Card {
    fn dev(&self) -> Dev {
        Dev(self.dev as *mut c_void)
    }
}

/// Backenden. Eies av prøvetråden.
pub struct Nvml {
    root: PathBuf,
    lib: Option<NvmlLib>,
    initialized: bool,
    cards: Vec<Card>,
    /// Tilstand og forklaring når ingen kort kan leses (ingen driver, oppdatert driver).
    pub driver_state: DeviceState,
    pub driver_detail: Option<String>,
    driver: Option<String>,
    nvml_version: Option<String>,
    cuda_version: Option<String>,
    next_init_at: i64,
}

/// Antall steg for de faste dataene før 2B sine (se [`Nvml::static_step`]).
const BASE_STATIC_STEPS: usize = 5;

impl Nvml {
    /// Laster og starter NVML. Feiler aldri: uten driver blir tilstanden `no_driver`.
    pub fn new(root: PathBuf, now_ms: i64) -> Nvml {
        let mut n = Nvml {
            root,
            lib: None,
            initialized: false,
            cards: vec![],
            driver_state: DeviceState::Ok,
            driver_detail: None,
            driver: None,
            nvml_version: None,
            cuda_version: None,
            next_init_at: 0,
        };
        n.try_init(now_ms);
        n
    }

    fn try_init(&mut self, now_ms: i64) {
        self.next_init_at = now_ms + 60_000;
        if self.lib.is_none() {
            match NvmlLib::load() {
                Ok(l) => self.lib = Some(l),
                Err(e) => {
                    self.driver_state = DeviceState::NoDriver;
                    self.driver_detail = Some(e);
                    return;
                }
            }
        }
        let lib = self.lib.as_ref().unwrap();
        let r = lib.init();
        if r != SUCCESS {
            self.driver_state = match r {
                ERROR_LIB_RM_VERSION_MISMATCH => DeviceState::DriverUpdated,
                _ => DeviceState::NoDriver,
            };
            self.driver_detail = Some(format!("nvmlInit_v2: {} ({})", lib.error_string(r), code_name(r)));
            return;
        }
        self.initialized = true;
        self.driver = lib.system_string("nvmlSystemGetDriverVersion");
        self.nvml_version = lib.system_string("nvmlSystemGetNVMLVersion");
        self.cuda_version = lib.cuda_version();
        let count = lib.count().unwrap_or(0);
        let mut cards = Vec::new();
        for i in 0..count {
            let Ok(dev) = lib.handle(i) else { continue };
            let Ok((bus, dev_id, sub_id)) = lib.pci(dev) else { continue };
            let id = super::sysfs_addr(&bus);
            let name = lib.dev_string(dev, "nvmlDeviceGetName").unwrap_or_else(|_| "NVIDIA-skjermkort".into());
            let link = PcieLink::open(&self.root, &id);
            let statics = GpuStatic {
                id: id.clone(),
                name: name.clone(),
                driver: self.driver.clone(),
                nvml_version: self.nvml_version.clone(),
                cuda_version: self.cuda_version.clone(),
                pci_device_id: Some(dev_id),
                pci_subsystem_id: Some(sub_id),
                pcie_gen_max: link.gen_max(),
                pcie_width_max: link.max_width,
                extra: serde_json::Value::Object(Default::default()),
                ..Default::default()
            };
            cards.push(Card {
                info: GpuInfo { id, index: i, name, state: DeviceState::Ok, since: now_ms, driver: self.driver.clone(), driver_installed: None, detail: None },
                dev: dev.0 as usize,
                nvml_bus: bus,
                link,
                na: HashSet::new(),
                last: GpuSample::default(),
                energy: None,
                fans: 0,
                pstate_n: None,
                statics,
                static_step: 0,
                lost_retry_at: 0,
            });
        }
        self.cards = cards;
        self.driver_state = DeviceState::Ok;
        self.driver_detail = None;
        self.check_driver(now_ms);
    }

    fn check_driver(&mut self, now_ms: i64) {
        let running = super::running_driver(&self.root);
        let installed = super::installed_driver(&self.root);
        let st = super::driver_state(running.as_deref(), installed.as_deref());
        for c in &mut self.cards {
            if c.info.state == DeviceState::Lost {
                continue;
            }
            if c.info.state != st {
                c.info.state = st;
                c.info.since = now_ms;
            }
            c.info.driver_installed = (st == DeviceState::DriverUpdated).then(|| installed.clone()).flatten();
            c.info.detail = (st == DeviceState::DriverUpdated)
                .then(|| format!("Kjernemodulen er {}, biblioteket er {}", running.clone().unwrap_or_default(), installed.clone().unwrap_or_default()));
        }
    }

    /// Ett felt: husker NOT_SUPPORTED, merker kortet som borte ved GPU_IS_LOST.
    fn take<T>(card: &mut Card, field: &'static str, r: Result<T, Ret>, now_ms: i64) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(ERROR_NOT_SUPPORTED) => {
                card.na.insert(field);
                None
            }
            Err(ERROR_GPU_IS_LOST) => {
                if card.info.state != DeviceState::Lost {
                    card.info.state = DeviceState::Lost;
                    card.info.since = now_ms;
                    card.info.detail = Some(format!("{field}: NVML_ERROR_GPU_IS_LOST"));
                    card.lost_retry_at = now_ms + 10_000;
                }
                None
            }
            Err(_) => None,
        }
    }

    fn read_card(lib: &NvmlLib, c: &mut Card, plan: &Plan) {
        let d = c.dev();
        let now = plan.now_ms;
        macro_rules! get {
            ($field:literal, $call:expr) => {
                if c.na.contains($field) { None } else { Self::take(c, $field, $call, now) }
            };
        }
        // ---- hver runde ----
        let temp = get!("temp", lib.dev_u32_arg(d, "nvmlDeviceGetTemperature", TEMPERATURE_GPU));
        c.last.temp = temp.map(f64::from);
        if c.info.state == DeviceState::Lost {
            return;
        }
        let energy = get!("energy", lib.dev_u64(d, "nvmlDeviceGetTotalEnergyConsumption"));
        c.last.energy_mj = energy.map(|e| e as f64);
        let mut power = None;
        if let Some(e) = energy {
            if let Some((e0, t0)) = c.energy
                && now > t0
                && e >= e0
            {
                power = Some((e - e0) as f64 / (now - t0) as f64);
            }
            c.energy = Some((e, now));
        }
        if power.is_none() && (energy.is_none() || plan.first) {
            power = get!("power", lib.dev_u32(d, "nvmlDeviceGetPowerUsage")).map(|mw| mw as f64 / 1000.0);
        }
        if power.is_some() || energy.is_none() {
            c.last.power = power;
        }
        c.last.core_clock = get!("coreClock", lib.dev_u32_arg(d, "nvmlDeviceGetClockInfo", CLOCK_GRAPHICS)).map(f64::from);
        c.last.mem_clock = get!("memClock", lib.dev_u32_arg(d, "nvmlDeviceGetClockInfo", CLOCK_MEM)).map(f64::from);
        if let Some((g, m)) = get!("gpuUtil", lib.utilization(d)) {
            c.last.gpu_util = Some(g as f64);
            c.last.mem_util = Some(m as f64);
        }
        if let Some((total, reserved, used)) = get!("vramUsed", lib.memory_v2(d)) {
            c.last.vram_total = Some(total as f64 / MIB);
            c.last.vram_reserved = Some(reserved as f64 / MIB);
            c.last.vram_used = Some(used as f64 / MIB);
            c.statics.vram_total = c.last.vram_total;
        }
        let pstate = get!("pstate", lib.dev_i32(d, "nvmlDeviceGetPerformanceState"));
        let pstate = pstate.filter(|p| (0..16).contains(p));
        if pstate != c.pstate_n {
            c.pstate_n = pstate;
            c.last.pstate = pstate.map(|p| format!("P{p}"));
        }
        if c.fans == 0 && !c.na.contains("fanCount") {
            c.fans = get!("fanCount", lib.dev_u32(d, "nvmlDeviceGetNumFans")).unwrap_or(0).min(8);
        }
        let nfans = c.fans.max(1) as usize;
        c.last.fans.resize(nfans, None);
        c.last.fan_rpm.resize(c.fans as usize, None);
        c.last.fans[0] = get!("fan0", lib.dev_u32_arg(d, "nvmlDeviceGetFanSpeed_v2", 0)).map(f64::from);
        c.last.pcie_gen = c.link.gen_now();
        // ---- hvert 2. s ----
        if plan.every2 || plan.first {
            for i in 1..c.fans {
                let v = if c.na.contains("fanN") { None } else { Self::take(c, "fanN", lib.dev_u32_arg(d, "nvmlDeviceGetFanSpeed_v2", i), now) };
                c.last.fans[i as usize] = v.map(f64::from);
            }
            for i in 0..c.fans {
                let v = if c.na.contains("fanRpm") { None } else { Self::take(c, "fanRpm", lib.fan_rpm(d, i), now) };
                c.last.fan_rpm[i as usize] = v.map(f64::from);
            }
            c.last.sm_clock = get!("smClock", lib.dev_u32_arg(d, "nvmlDeviceGetClockInfo", CLOCK_SM)).map(f64::from);
            c.last.video_clock = get!("videoClock", lib.dev_u32_arg(d, "nvmlDeviceGetClockInfo", CLOCK_VIDEO)).map(f64::from);
            c.last.enc_util = get!("encUtil", lib.dev_u32_pair(d, "nvmlDeviceGetEncoderUtilization")).map(|(u, _)| u as f64);
            c.last.dec_util = get!("decUtil", lib.dev_u32_pair(d, "nvmlDeviceGetDecoderUtilization")).map(|(u, _)| u as f64);
            // Klokkebegrensninger koster 25 ms i P8; der er svaret «tomgang» uansett.
            if c.last.pstate.as_deref() == Some("P8") {
                c.last.throttle = Some(EVENT_GPU_IDLE);
                c.last.throttle_inferred = true;
            } else {
                c.last.throttle = get!("throttle", lib.dev_u64(d, "nvmlDeviceGetCurrentClocksEventReasons"));
                c.last.throttle_inferred = false;
            }
        }
        // ---- hvert 5. s ----
        if plan.every5 || plan.first {
            c.last.power_limit = get!("powerLimit", lib.dev_u32(d, "nvmlDeviceGetEnforcedPowerLimit")).map(|mw| mw as f64 / 1000.0);
            c.last.pcie_width = c.link.width_now();
        }
        // Viftestyring og målfart (maskinvare-revisjon kap. 4: 5 s). Bare get-kall.
        // Bare vinduet viser dem, og målfarten koster 1,6–3,2 ms per vifte i P8.
        if (plan.every5 && plan.watched) || plan.first {
            c.last.fan_policy.resize(c.fans as usize, None);
            c.last.fan_target.resize(c.fans as usize, None);
            for i in 0..c.fans {
                let p = if c.na.contains("fanPolicy") { None } else { Self::take(c, "fanPolicy", lib.dev_u32_arg(d, "nvmlDeviceGetFanControlPolicy_v2", i), now) };
                c.last.fan_policy[i as usize] = p;
                let t = if c.na.contains("fanTarget") { None } else { Self::take(c, "fanTarget", lib.dev_u32_arg(d, "nvmlDeviceGetTargetFanSpeed", i), now) };
                c.last.fan_target[i as usize] = t.map(f64::from);
            }
            if c.last.fan_policy.iter().all(Option::is_none) {
                c.last.fan_policy.clear();
            }
            if c.last.fan_target.iter().all(Option::is_none) {
                c.last.fan_target.clear();
            }
        }
        c.last.fan = c.last.fans.iter().flatten().cloned().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        // Lista over det som ikke støttes, vokser bare: bygg den bare når den endres.
        if c.last.na.len() != c.na.len() {
            let mut na: Vec<String> = c.na.iter().map(|x| x.to_string()).collect();
            na.sort();
            c.last.na = na;
        }
    }

    /// Ett steg av de faste dataene (ett per runde de første sekundene).
    fn static_step(lib: &NvmlLib, c: &mut Card) {
        let d = c.dev();
        let st = &mut c.statics;
        match c.static_step {
            0 => {
                st.uuid = lib.dev_string(d, "nvmlDeviceGetUUID").ok();
                st.vbios = lib.dev_string(d, "nvmlDeviceGetVbiosVersion").ok();
            }
            1 => {
                if let Ok((lo, hi)) = lib.dev_u32_pair(d, "nvmlDeviceGetPowerManagementLimitConstraints") {
                    st.power_min = Some(lo as f64 / 1000.0);
                    st.power_max = Some(hi as f64 / 1000.0);
                }
                st.power_default = lib.dev_u32(d, "nvmlDeviceGetPowerManagementDefaultLimit").ok().map(|mw| mw as f64 / 1000.0);
            }
            2 => {
                let mx = |t| lib.dev_u32_arg(d, "nvmlDeviceGetMaxClockInfo", t).ok().map(f64::from);
                st.core_max = mx(CLOCK_GRAPHICS);
                st.sm_max = mx(CLOCK_SM);
                st.mem_max = mx(CLOCK_MEM);
                st.video_max = mx(CLOCK_VIDEO);
            }
            3 => {
                let th = |t| lib.dev_u32_arg(d, "nvmlDeviceGetTemperatureThreshold", t).ok().map(f64::from);
                st.temp_shutdown = th(THRESHOLD_SHUTDOWN);
                st.temp_slowdown = th(THRESHOLD_SLOWDOWN);
                st.temp_max = th(THRESHOLD_GPU_MAX);
                st.temp_target = th(THRESHOLD_ACOUSTIC_CURR);
            }
            4 => {
                st.fan_count = lib.dev_u32(d, "nvmlDeviceGetNumFans").ok();
                if let Ok((lo, hi)) = lib.dev_u32_pair(d, "nvmlDeviceGetMinMaxFanSpeed") {
                    st.fan_min = Some(lo as f64);
                    st.fan_max = Some(hi as f64);
                }
            }
            n => {
                let ext = n - BASE_STATIC_STEPS;
                if ext < super::nvml_info::STEPS {
                    super::nvml_info::read_step(lib, d, ext, &mut st.extra);
                }
            }
        }
        c.static_step += 1;
    }
}

impl GpuBackend for Nvml {
    fn kind(&self) -> &'static str {
        "nvml"
    }

    fn gpus(&self) -> Vec<GpuInfo> {
        if self.cards.is_empty() && self.driver_state != DeviceState::Ok {
            return vec![GpuInfo { id: String::new(), name: String::new(), state: self.driver_state, detail: self.driver_detail.clone(), ..Default::default() }];
        }
        self.cards.iter().map(|c| c.info.clone()).collect()
    }

    fn sample(&mut self, plan: &Plan) -> Round {
        let mut out = Round::default();
        if !self.initialized {
            return out;
        }
        let Some(lib) = self.lib.as_ref() else { return out };
        for c in &mut self.cards {
            if c.info.state == DeviceState::Lost {
                continue;
            }
            Self::read_card(lib, c, plan);
            if c.info.state == DeviceState::Lost {
                continue;
            }
            // De faste dataene: ett steg per runde, etter prøven.
            if c.static_step < BASE_STATIC_STEPS + super::nvml_info::STEPS {
                Self::static_step(lib, c);
            }
            out.samples.insert(c.info.id.clone(), c.last.clone());
        }
        out
    }

    fn static_info(&self, id: &str) -> Option<(GpuStatic, bool)> {
        let c = self.cards.iter().find(|c| c.info.id == id)?;
        Some((c.statics.clone(), c.static_step >= BASE_STATIC_STEPS + super::nvml_info::STEPS))
    }

    fn check(&mut self, now_ms: i64) {
        if !self.initialized {
            if now_ms >= self.next_init_at {
                self.try_init(now_ms);
            }
            return;
        }
        self.check_driver(now_ms);
        // Kort som er borte: prøv å finne dem igjen hvert 10. s.
        let lib = self.lib.as_ref().unwrap();
        for c in &mut self.cards {
            if c.info.state != DeviceState::Lost || now_ms < c.lost_retry_at {
                continue;
            }
            c.lost_retry_at = now_ms + 10_000;
            if let Ok(dev) = lib.handle_by_pci(&c.nvml_bus)
                && lib.dev_u32_arg(dev, "nvmlDeviceGetTemperature", TEMPERATURE_GPU).is_ok()
            {
                c.dev = dev.0 as usize;
                c.info.state = DeviceState::Ok;
                c.info.since = now_ms;
                c.info.detail = None;
                c.energy = None;
            }
        }
    }

    fn processes(&mut self, id: &str) -> Option<serde_json::Value> {
        if !self.initialized {
            return None;
        }
        let lib = self.lib.as_ref()?;
        let c = self.cards.iter().find(|c| c.info.id == id && c.info.state != DeviceState::Lost)?;
        Some(super::nvml_info::processes(lib, c.dev()))
    }

    fn shutdown(&mut self) {
        if self.initialized
            && let Some(l) = &self.lib
        {
            l.shutdown();
        }
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_lesefunksjoner_kan_slås_opp() {
        for ok in ["nvmlInit_v2", "nvmlShutdown", "nvmlErrorString", "nvmlDeviceGetTemperature", "nvmlSystemGetDriverVersion"] {
            assert!(allowed(ok), "{ok}");
        }
        for bad in [
            "nvmlDeviceSetPowerManagementLimit_v2",
            "nvmlDeviceSetFanSpeed_v2",
            "nvmlDeviceSetDefaultFanSpeed_v2",
            "nvmlDeviceResetGpuLockedClocks",
            "nvmlDeviceClearEccErrorCounts",
            "nvmlDeviceSetClockOffsets",
            "nvmlInit",
            "nvmlUnitSetLedState",
            "nvmlDeviceSetTemperatureThreshold",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    #[should_panic(expected = "ikke en lesefunksjon")]
    fn skrivefunksjoner_gir_panikk_før_dlsym() {
        // Bruker et tomt «bibliotek»: sjekken skjer før oppslaget.
        let lib = NvmlLib { handle: std::ptr::null_mut(), syms: RefCell::new(HashMap::new()) };
        // SAFETY: kommer aldri til dlsym.
        let _ = unsafe { lib.sym::<unsafe extern "C" fn() -> Ret>("nvmlDeviceSetFanSpeed_v2") };
    }
}
