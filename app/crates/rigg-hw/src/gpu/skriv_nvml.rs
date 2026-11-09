//! NVML-skriving for root-hjelperen. **Kompileres bare med cargo-funksjonen
//! `ekte-skriving`, som er av, og brukes bare når `skriv::EKTE_SKRIVING` er
//! `true`.** I 1.2.0 er begge av, og ingen prosess har kjørt denne koden.
//!
//! Egen `dlopen` med egen liste over navn ([`allowed_write`]): bare de kallene
//! hjelperen trenger, ett kort (indeks 0), og ingen andre skrivefunksjoner.
//! Kallene og grensene er de i `docs/daemon-api.md` kap. 5 og
//! `maskinvare-revisjon.md` kap. 8. Før trinn 2 skal hvert kall prøves for seg
//! mot kortet med brukeren til stede (se `hjelper-sikkerhet.md`).

use super::skriv::{Limits, Skriver, StockValues};
use rigg_core::txn::{FanMode, Range, Values};
use std::ffi::{CString, c_int, c_uint, c_void};

type Ret = c_int;
type Dev = *mut c_void;

const NVML_CLOCK_GRAPHICS: c_int = 0;
const NVML_CLOCK_MEM: c_int = 2;
const NVML_PSTATE_0: c_int = 0;
const NVML_TEMPERATURE_GPU: c_int = 0;
const THRESHOLD_ACOUSTIC_MIN: c_int = 4;
const THRESHOLD_ACOUSTIC_CURR: c_int = 5;
const THRESHOLD_ACOUSTIC_MAX: c_int = 6;

/// `nvmlClockOffset_v1_t`.
#[repr(C)]
#[derive(Default)]
struct ClockOffset {
    version: c_uint,
    kind: c_int,
    pstate: c_int,
    offset: c_int,
    min: c_int,
    max: c_int,
}

const CLOCK_OFFSET_V1: c_uint = (std::mem::size_of::<ClockOffset>() as c_uint) | (1 << 24);

/// Navnene skrivedelen kan slå opp. Alt annet avvises før `dlsym`.
pub fn allowed_write(name: &str) -> bool {
    matches!(
        name,
        "nvmlInit_v2"
            | "nvmlShutdown"
            | "nvmlDeviceGetHandleByIndex_v2"
            | "nvmlDeviceGetTemperature"
            | "nvmlDeviceGetNumFans"
            | "nvmlDeviceGetMinMaxFanSpeed"
            | "nvmlDeviceGetFanSpeed_v2"
            | "nvmlDeviceGetPowerManagementLimitConstraints"
            | "nvmlDeviceGetPowerManagementDefaultLimit"
            | "nvmlDeviceGetClockOffsets"
            | "nvmlDeviceGetTemperatureThreshold"
            | "nvmlDeviceSetPowerManagementLimit"
            | "nvmlDeviceSetClockOffsets"
            | "nvmlDeviceSetTemperatureThreshold"
            | "nvmlDeviceSetFanSpeed_v2"
            | "nvmlDeviceSetDefaultFanSpeed_v2"
    )
}

pub struct NvmlSkriver {
    handle: *mut c_void,
    dev: Dev,
    fans: u32,
    limits: Limits,
    applied: Values,
    fan_now: Option<f64>,
}

impl Drop for NvmlSkriver {
    fn drop(&mut self) {
        // SAFETY: `nvmlShutdown(void)` på et initialisert bibliotek.
        unsafe {
            if let Some(f) = self.sym::<unsafe extern "C" fn() -> Ret>("nvmlShutdown") {
                f();
            }
            libc::dlclose(self.handle);
        }
    }
}

macro_rules! call {
    ($s:expr, $name:literal, $ty:ty, $($arg:expr),*) => {{
        // SAFETY: signaturen `$ty` er NVML-signaturen for `$name`.
        let f = unsafe { $s.sym::<$ty>($name) }.ok_or(concat!($name, " finnes ikke i driveren"))?;
        let r = unsafe { f($($arg),*) };
        if r != 0 { Err(format!("{} feilet ({})", $name, r)) } else { Ok(()) }
    }};
}

impl NvmlSkriver {
    /// # Safety
    /// `F` må være funksjonspekeren som stemmer med NVML-signaturen.
    unsafe fn sym<F: Copy>(&self, name: &'static str) -> Option<F> {
        assert!(allowed_write(name), "{name} er ikke på lista til skrivedelen");
        let c = CString::new(name).ok()?;
        // SAFETY: dlsym på et åpent bibliotek med gyldig navn.
        let p = unsafe { libc::dlsym(self.handle, c.as_ptr()) } as usize;
        if p == 0 {
            return None;
        }
        // SAFETY: ikke-null peker fra dlsym; kalleren lover typen.
        Some(unsafe { std::mem::transmute_copy::<usize, F>(&p) })
    }

    fn read_limits(&self) -> Result<Limits, String> {
        let d = self.dev;
        let (mut pmin, mut pmax, mut pdef) = (0 as c_uint, 0 as c_uint, 0 as c_uint);
        call!(self, "nvmlDeviceGetPowerManagementLimitConstraints", unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret, d, &mut pmin, &mut pmax)?;
        call!(self, "nvmlDeviceGetPowerManagementDefaultLimit", unsafe extern "C" fn(Dev, *mut c_uint) -> Ret, d, &mut pdef)?;
        let (mut fmin, mut fmax) = (0 as c_uint, 0 as c_uint);
        call!(self, "nvmlDeviceGetMinMaxFanSpeed", unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret, d, &mut fmin, &mut fmax)?;
        let off = |kind| -> Result<(f64, f64), String> {
            let mut o = ClockOffset { version: CLOCK_OFFSET_V1, kind, pstate: NVML_PSTATE_0, ..Default::default() };
            call!(self, "nvmlDeviceGetClockOffsets", unsafe extern "C" fn(Dev, *mut ClockOffset) -> Ret, d, &mut o)?;
            Ok((o.min as f64, o.max as f64))
        };
        let core = off(NVML_CLOCK_GRAPHICS)?;
        let mem = off(NVML_CLOCK_MEM)?;
        let th = |t| -> Result<f64, String> {
            let mut v: c_uint = 0;
            call!(self, "nvmlDeviceGetTemperatureThreshold", unsafe extern "C" fn(Dev, c_int, *mut c_uint) -> Ret, d, t, &mut v)?;
            Ok(v as f64)
        };
        let (tmin, tcur, tmax) = (th(THRESHOLD_ACOUSTIC_MIN)?, th(THRESHOLD_ACOUSTIC_CURR)?, th(THRESHOLD_ACOUSTIC_MAX)?);
        Ok(Limits {
            power_w: Range::new(pmin as f64 / 1000.0, pmax as f64 / 1000.0),
            core_mhz: Range::new(core.0, core.1),
            mem_mhz: Range::new(mem.0 / 2.0, mem.1 / 2.0),
            temp_target_c: Range::new(tmin, tmax),
            fan_pct: Range::new(fmin as f64, fmax as f64),
            default: StockValues { power_w: pdef as f64 / 1000.0, temp_target_c: tcur },
        })
    }

    fn set_power(&self, w: f64) -> Result<(), String> {
        call!(self, "nvmlDeviceSetPowerManagementLimit", unsafe extern "C" fn(Dev, c_uint) -> Ret, self.dev, (w * 1000.0).round() as c_uint)
    }

    fn set_offset(&self, kind: c_int, mhz: f64) -> Result<(), String> {
        let mut o = ClockOffset { version: CLOCK_OFFSET_V1, kind, pstate: NVML_PSTATE_0, offset: mhz.round() as c_int, ..Default::default() };
        call!(self, "nvmlDeviceSetClockOffsets", unsafe extern "C" fn(Dev, *mut ClockOffset) -> Ret, self.dev, &mut o)
    }

    fn set_temp_target(&self, c: f64) -> Result<(), String> {
        let mut v: c_int = c.round() as c_int;
        call!(self, "nvmlDeviceSetTemperatureThreshold", unsafe extern "C" fn(Dev, c_int, *mut c_int) -> Ret, self.dev, THRESHOLD_ACOUSTIC_CURR, &mut v)
    }

    fn fans_auto(&mut self) -> Result<(), String> {
        for i in 0..self.fans {
            call!(self, "nvmlDeviceSetDefaultFanSpeed_v2", unsafe extern "C" fn(Dev, c_uint) -> Ret, self.dev, i)?;
        }
        self.fan_now = None;
        Ok(())
    }
}

pub fn open() -> Result<NvmlSkriver, String> {
    let name = CString::new("libnvidia-ml.so.1").map_err(|e| e.to_string())?;
    // SAFETY: dlopen med gyldig navn.
    let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return Err("Fant ikke NVIDIA-driveren (libnvidia-ml.so.1)".into());
    }
    let mut s = NvmlSkriver { handle, dev: std::ptr::null_mut(), fans: 0, limits: Limits::rtx2080(), applied: Values::default(), fan_now: None };
    call!(s, "nvmlInit_v2", unsafe extern "C" fn() -> Ret,)?;
    let mut dev: Dev = std::ptr::null_mut();
    call!(s, "nvmlDeviceGetHandleByIndex_v2", unsafe extern "C" fn(c_uint, *mut Dev) -> Ret, 0, &mut dev)?;
    s.dev = dev;
    let mut n: c_uint = 0;
    call!(s, "nvmlDeviceGetNumFans", unsafe extern "C" fn(Dev, *mut c_uint) -> Ret, dev, &mut n)?;
    s.fans = n.min(8);
    s.limits = s.read_limits()?;
    s.applied = s.limits.stock();
    Ok(s)
}

impl Skriver for NvmlSkriver {
    fn backend(&self) -> &'static str {
        "nvml"
    }
    fn limits(&self) -> Limits {
        self.limits
    }
    fn temperature(&mut self) -> Option<f64> {
        let mut t: c_uint = 0;
        let r = (|| -> Result<(), String> { call!(self, "nvmlDeviceGetTemperature", unsafe extern "C" fn(Dev, c_int, *mut c_uint) -> Ret, self.dev, NVML_TEMPERATURE_GPU, &mut t) })();
        r.ok().map(|_| t as f64)
    }
    fn apply(&mut self, v: &Values) -> Result<(), String> {
        let old = self.applied.clone();
        if v.power_limit_w != old.power_limit_w {
            self.set_power(v.power_limit_w)?;
        }
        if v.core_offset_mhz != old.core_offset_mhz {
            self.set_offset(NVML_CLOCK_GRAPHICS, v.core_offset_mhz)?;
        }
        if v.mem_offset_mhz != old.mem_offset_mhz {
            self.set_offset(NVML_CLOCK_MEM, v.mem_offset_mhz * 2.0)?;
        }
        if v.temp_target_c != old.temp_target_c {
            self.set_temp_target(v.temp_target_c)?;
        }
        match v.fan_mode {
            FanMode::Auto if self.fan_now.is_some() => self.fans_auto()?,
            FanMode::Manual => self.set_fan_percent(v.fan_percent)?,
            _ => {}
        }
        self.applied = v.clone();
        Ok(())
    }
    fn set_fan_percent(&mut self, pct: f64) -> Result<(), String> {
        let p = self.limits.fan_pct.clamp(pct.round());
        if self.fan_now == Some(p) {
            return Ok(());
        }
        for i in 0..self.fans {
            call!(self, "nvmlDeviceSetFanSpeed_v2", unsafe extern "C" fn(Dev, c_uint, c_uint) -> Ret, self.dev, i, p as c_uint)?;
        }
        self.fan_now = Some(p);
        Ok(())
    }
    fn fan_count(&self) -> usize {
        self.fans as usize
    }
    /// Farten hver vifte har nå (lesing), så viftekurven starter der.
    fn fan_percents(&mut self) -> Vec<f64> {
        let mut out = Vec::new();
        for i in 0..self.fans {
            let mut p: c_uint = 0;
            let r = (|| -> Result<(), String> { call!(self, "nvmlDeviceGetFanSpeed_v2", unsafe extern "C" fn(Dev, c_uint, *mut c_uint) -> Ret, self.dev, i, &mut p) })();
            match r {
                Ok(()) => out.push(p as f64),
                Err(_) => return Vec::new(),
            }
        }
        out
    }
    fn set_fan_percents(&mut self, pct: &[f64]) -> Result<(), String> {
        for i in 0..self.fans {
            let p = self.limits.fan_pct.clamp(pct.get(i as usize).or(pct.last()).copied().unwrap_or(100.0).round());
            call!(self, "nvmlDeviceSetFanSpeed_v2", unsafe extern "C" fn(Dev, c_uint, c_uint) -> Ret, self.dev, i, p as c_uint)?;
        }
        self.fan_now = pct.iter().copied().reduce(f64::max);
        Ok(())
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"applied": self.applied.to_json(), "fanNow": self.fan_now})
    }
    fn reset(&mut self) -> Result<(), String> {
        let stock = self.limits.stock();
        // Viftene først (det tryggeste), så resten. Fortsett ved feil.
        let mut errs = Vec::new();
        if let Err(e) = self.fans_auto() {
            errs.push(e);
        }
        for r in [self.set_power(stock.power_limit_w), self.set_offset(NVML_CLOCK_GRAPHICS, 0.0), self.set_offset(NVML_CLOCK_MEM, 0.0), self.set_temp_target(stock.temp_target_c)] {
            if let Err(e) = r {
                errs.push(e);
            }
        }
        self.applied = stock;
        self.fan_now = None;
        if errs.is_empty() { Ok(()) } else { Err(errs.join("; ")) }
    }
}
