//! Kortbundet NVML-backend for root-hjelperen. Åpning gjør bare get-kall.
//! Set-kall krever kompilert støtte og at hjelperen har persistet baseline.
//! Ingen fallback til kortindeks, I2C, RGB eller automatisk overklokking.

use super::skriv::{Limits, Skriver, valid_binding};
use rigg_core::txn::{FanMode, Range, Values};
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};

type Ret = c_int;
type Dev = *mut c_void;
const GRAPHICS: c_int = 0;
const MEMORY: c_int = 2;

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

pub fn allowed_write(name: &str) -> bool {
    matches!(
        name,
        "nvmlInit_v2"
            | "nvmlShutdown"
            | "nvmlDeviceGetHandleByPciBusId_v2"
            | "nvmlDeviceGetUUID"
            | "nvmlDeviceGetVbiosVersion"
            | "nvmlSystemGetDriverVersion"
            | "nvmlDeviceGetTemperature"
            | "nvmlDeviceGetNumFans"
            | "nvmlDeviceGetMinMaxFanSpeed"
            | "nvmlDeviceGetFanSpeed_v2"
            | "nvmlDeviceGetTargetFanSpeed"
            | "nvmlDeviceGetFanControlPolicy_v2"
            | "nvmlDeviceGetPowerManagementLimitConstraints"
            | "nvmlDeviceGetPowerManagementDefaultLimit"
            | "nvmlDeviceGetPowerManagementLimit"
            | "nvmlDeviceGetClockOffsets"
            | "nvmlDeviceGetTemperatureThreshold"
            | "nvmlDeviceSetPowerManagementLimit"
            | "nvmlDeviceSetClockOffsets"
            | "nvmlDeviceSetTemperatureThreshold"
            | "nvmlDeviceSetFanSpeed_v2"
            | "nvmlDeviceSetDefaultFanSpeed_v2"
            | "nvmlDeviceSetFanControlPolicy"
    )
}

fn nvml_result(name: &str, code: Ret) -> Result<(), String> {
    if code == 0 {
        return Ok(());
    }
    let kind = match code {
        2 => "invalid_argument",
        3 | 13 => "not_supported",
        4 => "no_permission",
        15 => "gpu_lost",
        _ => "nvml_error",
    };
    Err(format!("{kind}: {name} feilet (NVML {code})"))
}

/// Det smale testgrensesnittet inneholder ingen filstier eller frie kall.
trait Api {
    fn identity(&self) -> Value;
    fn supports(&self, name: &'static str) -> bool;
    fn power(&mut self) -> Result<(Range, f64, f64), String>;
    fn offset(&mut self, kind: c_int) -> Result<(f64, Range), String>;
    fn temp_limits(&mut self) -> Result<(f64, Range), String>;
    fn fans(&mut self) -> Result<(usize, Range), String>;
    fn temperature(&mut self) -> Result<f64, String>;
    fn fan_policy(&mut self, i: usize) -> Result<u32, String>;
    fn fan_target(&mut self, i: usize) -> Result<f64, String>;
    fn fan_speed(&mut self, i: usize) -> Result<f64, String>;
    fn set_power(&mut self, w: f64) -> Result<(), String>;
    fn set_offset(&mut self, kind: c_int, raw_mhz: f64) -> Result<(), String>;
    fn set_temp(&mut self, c: f64) -> Result<(), String>;
    fn set_fan(&mut self, i: usize, pct: f64) -> Result<(), String>;
    fn auto_fan(&mut self, i: usize) -> Result<(), String>;
}

struct DeviceApi {
    handle: *mut c_void,
    dev: Dev,
    initialized: bool,
    identity: Value,
}

impl DeviceApi {
    /// # Safety
    /// F må være den eksakte signaturen til det hvitelistede NVML-symbolet.
    unsafe fn sym<F: Copy>(&self, name: &'static str) -> Option<F> {
        assert!(allowed_write(name), "{name} er ikke tillatt");
        assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<usize>());
        let name = CString::new(name).ok()?;
        // SAFETY: åpent bibliotek og nullterminert symbolnavn.
        let p = unsafe { libc::dlsym(self.handle, name.as_ptr()) } as usize;
        if p == 0 {
            None
        } else {
            Some(unsafe { std::mem::transmute_copy::<usize, F>(&p) })
        }
    }

    fn dev_string(&self, name: &'static str) -> Result<String, String> {
        let mut b = [0 as c_char; 128];
        // SAFETY: NVMLs dev/string/len-signatur, bufferlengden sendes med.
        let f = unsafe { self.sym::<unsafe extern "C" fn(Dev, *mut c_char, c_uint) -> Ret>(name) }
            .ok_or_else(|| format!("not_supported: {name}"))?;
        nvml_result(name, unsafe {
            f(self.dev, b.as_mut_ptr(), b.len() as c_uint)
        })?;
        b[127] = 0;
        Ok(unsafe { CStr::from_ptr(b.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    fn number(&self, name: &'static str) -> Result<u32, String> {
        let mut n = 0;
        // SAFETY: NVML dev/unsigned-output-signatur.
        let f = unsafe { self.sym::<unsafe extern "C" fn(Dev, *mut c_uint) -> Ret>(name) }
            .ok_or_else(|| format!("not_supported: {name}"))?;
        nvml_result(name, unsafe { f(self.dev, &mut n) })?;
        Ok(n)
    }

    fn arg_number(&self, name: &'static str, arg: u32) -> Result<u32, String> {
        let mut n = 0;
        // SAFETY: NVML dev/enum-or-index/unsigned-output-signatur.
        let f = unsafe { self.sym::<unsafe extern "C" fn(Dev, c_uint, *mut c_uint) -> Ret>(name) }
            .ok_or_else(|| format!("not_supported: {name}"))?;
        nvml_result(name, unsafe { f(self.dev, arg, &mut n) })?;
        Ok(n)
    }

    fn pair(&self, name: &'static str) -> Result<Range, String> {
        let (mut a, mut b) = (0, 0);
        // SAFETY: NVML dev/two-unsigned-outputs-signatur.
        let f =
            unsafe { self.sym::<unsafe extern "C" fn(Dev, *mut c_uint, *mut c_uint) -> Ret>(name) }
                .ok_or_else(|| format!("not_supported: {name}"))?;
        nvml_result(name, unsafe { f(self.dev, &mut a, &mut b) })?;
        Ok(Range::new(a as f64, b as f64))
    }
}

impl Drop for DeviceApi {
    fn drop(&mut self) {
        // SAFETY: dette objektet eier dlopen-håndtaket og init-referansen.
        unsafe {
            if self.initialized
                && let Some(f) = self.sym::<unsafe extern "C" fn() -> Ret>("nvmlShutdown")
            {
                f();
            }
            libc::dlclose(self.handle);
        }
    }
}

impl Api for DeviceApi {
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn supports(&self, name: &'static str) -> bool {
        unsafe { self.sym::<unsafe extern "C" fn()>(name) }.is_some()
    }
    fn power(&mut self) -> Result<(Range, f64, f64), String> {
        let r = self.pair("nvmlDeviceGetPowerManagementLimitConstraints")?;
        Ok((
            Range::new(r.min / 1000.0, r.max / 1000.0),
            self.number("nvmlDeviceGetPowerManagementDefaultLimit")? as f64 / 1000.0,
            self.number("nvmlDeviceGetPowerManagementLimit")? as f64 / 1000.0,
        ))
    }
    fn offset(&mut self, kind: c_int) -> Result<(f64, Range), String> {
        let mut o = ClockOffset {
            version: std::mem::size_of::<ClockOffset>() as u32 | (1 << 24),
            kind,
            pstate: 0,
            ..Default::default()
        };
        // SAFETY: nvmlClockOffset_t v1 og korrekt dev/output-signatur.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, *mut ClockOffset) -> Ret>(
                "nvmlDeviceGetClockOffsets",
            )
        }
        .ok_or("not_supported: clock offsets")?;
        nvml_result("nvmlDeviceGetClockOffsets", unsafe { f(self.dev, &mut o) })?;
        Ok((o.offset as f64, Range::new(o.min as f64, o.max as f64)))
    }
    fn temp_limits(&mut self) -> Result<(f64, Range), String> {
        Ok((
            self.arg_number("nvmlDeviceGetTemperatureThreshold", 5)? as f64,
            Range::new(
                self.arg_number("nvmlDeviceGetTemperatureThreshold", 4)? as f64,
                self.arg_number("nvmlDeviceGetTemperatureThreshold", 6)? as f64,
            ),
        ))
    }
    fn fans(&mut self) -> Result<(usize, Range), String> {
        let n = self.number("nvmlDeviceGetNumFans")? as usize;
        if n == 0 || n > 8 {
            return Err("not_supported: ugyldig antall vifter".into());
        }
        Ok((n, self.pair("nvmlDeviceGetMinMaxFanSpeed")?))
    }
    fn temperature(&mut self) -> Result<f64, String> {
        self.arg_number("nvmlDeviceGetTemperature", 0)
            .map(f64::from)
    }
    fn fan_policy(&mut self, i: usize) -> Result<u32, String> {
        self.arg_number("nvmlDeviceGetFanControlPolicy_v2", i as u32)
    }
    fn fan_target(&mut self, i: usize) -> Result<f64, String> {
        self.arg_number("nvmlDeviceGetTargetFanSpeed", i as u32)
            .map(f64::from)
    }
    fn fan_speed(&mut self, i: usize) -> Result<f64, String> {
        self.arg_number("nvmlDeviceGetFanSpeed_v2", i as u32)
            .map(f64::from)
    }
    fn set_power(&mut self, w: f64) -> Result<(), String> {
        // SAFETY: dev/milliwatt-signatur.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, c_uint) -> Ret>(
                "nvmlDeviceSetPowerManagementLimit",
            )
        }
        .ok_or("not_supported: power")?;
        nvml_result("nvmlDeviceSetPowerManagementLimit", unsafe {
            f(self.dev, (w * 1000.0).round() as u32)
        })
    }
    fn set_offset(&mut self, kind: c_int, raw_mhz: f64) -> Result<(), String> {
        let mut o = ClockOffset {
            version: std::mem::size_of::<ClockOffset>() as u32 | (1 << 24),
            kind,
            pstate: 0,
            offset: raw_mhz.round() as c_int,
            ..Default::default()
        };
        // SAFETY: nvmlClockOffset_t v1 med P0, ingen andre domener eller P-stater.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, *mut ClockOffset) -> Ret>(
                "nvmlDeviceSetClockOffsets",
            )
        }
        .ok_or("not_supported: clock offsets")?;
        nvml_result("nvmlDeviceSetClockOffsets", unsafe { f(self.dev, &mut o) })
    }
    fn set_temp(&mut self, c: f64) -> Result<(), String> {
        let mut c = c.round() as c_int;
        // SAFETY: dev/threshold/int-pointer-signatur, bare ACOUSTIC_CURR.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, c_int, *mut c_int) -> Ret>(
                "nvmlDeviceSetTemperatureThreshold",
            )
        }
        .ok_or("not_supported: temperature target")?;
        nvml_result("nvmlDeviceSetTemperatureThreshold", unsafe {
            f(self.dev, 5, &mut c)
        })
    }
    fn set_fan(&mut self, i: usize, pct: f64) -> Result<(), String> {
        // SAFETY: dev/fan-index/percent-signatur.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, c_uint, c_uint) -> Ret>("nvmlDeviceSetFanSpeed_v2")
        }
        .ok_or("not_supported: fan")?;
        nvml_result("nvmlDeviceSetFanSpeed_v2", unsafe {
            f(self.dev, i as u32, pct.round() as u32)
        })
    }
    fn auto_fan(&mut self, i: usize) -> Result<(), String> {
        // SAFETY: dev/fan-index-signatur.
        let f = unsafe {
            self.sym::<unsafe extern "C" fn(Dev, c_uint) -> Ret>("nvmlDeviceSetDefaultFanSpeed_v2")
        }
        .ok_or("not_supported: fan auto")?;
        nvml_result("nvmlDeviceSetDefaultFanSpeed_v2", unsafe {
            f(self.dev, i as u32)
        })?;
        if self.fan_policy(i)? != 0 {
            // Reserven brukes bare når tilbakelesingen faktisk viser manuell policy.
            let f = unsafe {
                self.sym::<unsafe extern "C" fn(Dev, c_uint, c_uint) -> Ret>(
                    "nvmlDeviceSetFanControlPolicy",
                )
            }
            .ok_or("readback_mismatch: vifte ble ikke automatisk")?;
            nvml_result("nvmlDeviceSetFanControlPolicy", unsafe {
                f(self.dev, i as u32, 0)
            })?;
        }
        if self.fan_policy(i)? != 0 {
            return Err("readback_mismatch: vifte ble ikke automatisk".into());
        }
        Ok(())
    }
}

pub struct NvmlSkriver {
    api: Box<dyn Api>,
    identity: Value,
    caps: Value,
    limits: Limits,
    applied: Values,
    restore: Values,
    baseline: Value,
    baseline_ready: bool,
    observed: Value,
    fans: usize,
    /// Vifter som kan ha blitt endret av en tidligere økt (recovery).
    required_fans: usize,
    fan_now: Option<f64>,
    faults: Vec<String>,
    offsets_verified: [bool; 2],
}

fn capability(r: Range, readback: bool) -> Value {
    json!({"available": true, "min": r.min, "max": r.max, "readback": readback})
}
fn unavailable(reason: String) -> Value {
    json!({"available": false, "reason": reason, "readback": false})
}
fn valid_range(r: Range) -> bool {
    r.min.is_finite() && r.max.is_finite() && r.min <= r.max
}

impl NvmlSkriver {
    fn from_api(mut api: Box<dyn Api>) -> Self {
        let identity = api.identity();
        let mut l = Limits::rtx2080();
        let mut current = Values::default();
        let mut caps = json!({});
        let mut observed = json!({});
        let power = api.power().and_then(|(r, d, n)| {
            if valid_range(r)
                && n.is_finite()
                && d >= r.min
                && d <= r.max
                && api.supports("nvmlDeviceSetPowerManagementLimit")
            {
                Ok((r, d, n))
            } else {
                Err("not_supported: effektgrenser eller setter mangler".into())
            }
        });
        match power {
            Ok((r, d, n)) => {
                l.power_w = r;
                l.default.power_w = d;
                current.power_limit_w = n;
                caps["power"] = capability(r, true);
                observed["powerLimitW"] = json!(n);
            }
            Err(e) => {
                l.power_w = Range::new(0.0, 0.0);
                caps["power"] = unavailable(e);
            }
        }
        for (k, kind, div) in [("core_clock", GRAPHICS, 1.0), ("mem_clock", MEMORY, 2.0)] {
            let result = api.offset(kind).and_then(|(n, r)| {
                if valid_range(r) && n.is_finite() && api.supports("nvmlDeviceSetClockOffsets") {
                    Ok((n / div, Range::new(r.min / div, r.max / div)))
                } else {
                    Err("not_supported: offsetområde eller setter mangler".into())
                }
            });
            match result {
                Ok((n, r)) => {
                    caps[k] = capability(r, false);
                    if kind == GRAPHICS {
                        l.core_mhz = r;
                        current.core_offset_mhz = n;
                        observed["coreOffsetMhz"] = json!(n);
                    } else {
                        l.mem_mhz = r;
                        current.mem_offset_mhz = n;
                        observed["memOffsetMhz"] = json!(n);
                    }
                }
                Err(e) => {
                    caps[k] = unavailable(e);
                    if kind == GRAPHICS {
                        l.core_mhz = Range::new(0.0, 0.0);
                    } else {
                        l.mem_mhz = Range::new(0.0, 0.0);
                    }
                }
            }
        }
        match api.temp_limits().and_then(|(n, r)| {
            if valid_range(r) && n.is_finite() && api.supports("nvmlDeviceSetTemperatureThreshold")
            {
                Ok((n, r))
            } else {
                Err("not_supported: temperaturmål mangler".into())
            }
        }) {
            Ok((n, r)) => {
                l.temp_target_c = r;
                l.default.temp_target_c = n;
                current.temp_target_c = n;
                caps["temp_target"] = capability(r, true);
                observed["tempTargetC"] = json!(n);
            }
            Err(e) => {
                l.temp_target_c = Range::new(0.0, 0.0);
                caps["temp_target"] = unavailable(e);
            }
        }
        let fan = api.fans().and_then(|(n, r)| {
            if !valid_range(r)
                || r.min < 0.0
                || r.max > 100.0
                || !api.supports("nvmlDeviceSetFanSpeed_v2")
                || !api.supports("nvmlDeviceSetDefaultFanSpeed_v2")
            {
                return Err("not_supported: viftegrenser eller setter mangler".into());
            }
            // Manuell styring krever at Auto og mål kan leses for alle viftene.
            let mut policies = Vec::new();
            let mut targets = Vec::new();
            for i in 0..n {
                policies.push(api.fan_policy(i)?);
                targets.push(api.fan_target(i)?);
            }
            api.temperature()?;
            Ok((n, r, policies, targets))
        });
        let fans = match fan {
            Ok((n, r, p, t)) => {
                l.fan_pct = r;
                current.fan_mode = if p.iter().all(|p| *p == 0) {
                    FanMode::Auto
                } else {
                    FanMode::Manual
                };
                current.fan_percent = t.iter().copied().fold(r.min, f64::max);
                observed["fanPolicy"] = json!(p);
                observed["fanTarget"] = json!(t);
                caps["fan_manual"] = capability(r, true);
                caps["fan_curve"] = capability(r, true);
                n
            }
            Err(e) => {
                l.fan_pct = Range::new(0.0, 0.0);
                caps["fan_manual"] = unavailable(e.clone());
                caps["fan_curve"] = unavailable(e);
                0
            }
        };
        caps["rgb"] = unavailable("not_implemented: ingen RGB-backend".into());
        // Dette er gjenopprettingspunktet før Agentic HW Monitor endret noe, ikke
        // antatte fabrikkverdier. Bevar også eksternt satt effekt og offset.
        l.default.power_w = current.power_limit_w;
        l.default.temp_target_c = current.temp_target_c;
        l.default.core_offset_mhz = current.core_offset_mhz;
        l.default.mem_offset_mhz = current.mem_offset_mhz;
        l.default.fan_percent = current.fan_percent;
        let restore = current.clone();
        let baseline = json!({"version": 1, "identity": identity, "captured": current.to_json(), "restore": restore.to_json()});
        Self {
            api,
            identity,
            caps,
            limits: l,
            applied: current,
            restore,
            baseline,
            baseline_ready: false,
            observed,
            fans,
            required_fans: 0,
            fan_now: None,
            faults: vec![],
            offsets_verified: [true, true],
        }
    }

    fn available(&self, key: &str) -> bool {
        self.caps[key]["available"] == true
    }
    fn guard(&self) -> Result<(), String> {
        if self.baseline_ready {
            Ok(())
        } else {
            Err("baseline_required: grunnverdiene må lagres før første endring".into())
        }
    }
    fn check(&self, key: &str, v: f64, range: Range) -> Result<(), String> {
        if !self.available(key) {
            return Err(format!("not_supported: {key}"));
        }
        if !v.is_finite() || v < range.min || v > range.max {
            return Err(format!("invalid_argument: {key} er utenfor kortets område"));
        }
        Ok(())
    }
    fn power(&mut self, value: f64) -> Result<(), String> {
        if let Err(e) = self.api.set_power(value) {
            self.disable_unsupported("power", &e);
            return Err(e);
        }
        self.applied.power_limit_w = value;
        let (_, _, now) = self.api.power()?;
        self.observed["powerLimitW"] = json!(now);
        if (now - value).abs() > 0.001 {
            return Err("readback_mismatch: effektgrensen stemmer ikke".into());
        }
        Ok(())
    }
    fn offset(&mut self, kind: c_int, value: f64) -> Result<(), String> {
        let div = if kind == MEMORY { 2.0 } else { 1.0 };
        if let Err(e) = self.api.set_offset(kind, value * div) {
            self.disable_unsupported(
                if kind == GRAPHICS {
                    "core_clock"
                } else {
                    "mem_clock"
                },
                &e,
            );
            return Err(e);
        }
        let i = usize::from(kind == MEMORY);
        if kind == GRAPHICS {
            self.applied.core_offset_mhz = value;
        } else {
            self.applied.mem_offset_mhz = value;
        }
        self.offsets_verified[i] = false;
        let readback = self.api.offset(kind);
        if let Ok((now, _)) = &readback {
            self.observed[if kind == GRAPHICS {
                "coreOffsetMhz"
            } else {
                "memOffsetMhz"
            }] = json!(*now / div);
            self.offsets_verified[i] = (*now / div - value).abs() < 0.001;
        }
        // Setterens aksept er ikke bevis på endring. Behold ønsket applied-verdi
        // så recovery må skrive tilbake også når getter mangler eller melder 0.
        if !self.offsets_verified[i] {
            return Err(match readback {
                Err(e) => format!("readback_mismatch: klokkeoffset kunne ikke leses tilbake: {e}"),
                Ok(_) => "readback_mismatch: klokkeoffset stemmer ikke".into(),
            });
        }
        Ok(())
    }
    fn temp(&mut self, value: f64) -> Result<(), String> {
        if let Err(e) = self.api.set_temp(value) {
            self.disable_unsupported("temp_target", &e);
            return Err(e);
        }
        self.applied.temp_target_c = value;
        let (now, _) = self.api.temp_limits()?;
        self.observed["tempTargetC"] = json!(now);
        if (now - value).abs() > 0.001 {
            return Err("readback_mismatch: temperaturmålet stemmer ikke".into());
        }
        Ok(())
    }
    fn fans_auto(&mut self) -> Result<(), String> {
        let mut errors = vec![];
        for i in 0..self.fans {
            if let Err(e) = self.api.auto_fan(i) {
                errors.push(format!("vifte {i}: {e}"));
            }
        }
        if errors.is_empty() {
            self.applied.fan_mode = FanMode::Auto;
            self.fan_now = None;
            self.observed["fanPolicy"] = json!(vec![0; self.fans]);
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
    fn record(&mut self, result: Result<(), String>) -> Result<(), String> {
        if let Err(e) = &result {
            self.faults.push(e.clone());
        }
        result
    }
    fn disable_unsupported(&mut self, key: &str, error: &str) {
        if error.starts_with("not_supported:") {
            self.caps[key] = unavailable(error.to_string());
        }
    }
    fn restore_to(&mut self, v: &Values) -> Result<(), String> {
        self.guard()?;
        // Gjenoppretting skriver også felt der setter feilet før cachen kunne
        // oppdateres. Forsøk resten selv om én vifte eller ett felt feiler.
        let mut errors = vec![];
        if self.required_fans > self.fans {
            // Viftesøket fant færre vifter enn en tidligere økt kunne endre.
            // Da kan Auto verken skrives eller bekreftes for de manglende.
            errors.push(format!(
                "fan_unverified: {} av {} vifter ble funnet; Auto kan ikke bekreftes",
                self.fans, self.required_fans
            ));
        }
        if self.fans > 0 {
            let r = match v.fan_mode {
                FanMode::Auto => self.fans_auto(),
                FanMode::Manual => self.set_fan_percent(v.fan_percent),
                FanMode::Curve => self.set_fan_percent(self.limits.fan_pct.max),
            };
            if let Err(e) = r {
                errors.push(e);
            }
        }
        for (key, value, range) in [
            ("power", v.power_limit_w, self.limits.power_w),
            ("core_clock", v.core_offset_mhz, self.limits.core_mhz),
            ("mem_clock", v.mem_offset_mhz, self.limits.mem_mhz),
            ("temp_target", v.temp_target_c, self.limits.temp_target_c),
        ] {
            if !self.available(key) {
                continue;
            }
            let r = self.check(key, value, range).and_then(|()| match key {
                "power" => self.power(value),
                "core_clock" => self.offset(GRAPHICS, value),
                "mem_clock" => self.offset(MEMORY, value),
                _ => self.temp(value),
            });
            if let Err(e) = r {
                errors.push(e);
            }
        }
        if errors.is_empty() && self.fans > 0 {
            self.applied.fan_mode = v.fan_mode;
            self.applied.fan_percent = v.fan_percent;
            self.applied.fan_curve = v.fan_curve.clone();
            self.applied.fan_per_fan = v.fan_per_fan;
            self.applied.fan_curves = v.fan_curves.clone();
            self.applied.fan_hysteresis_c = v.fan_hysteresis_c;
            self.applied.fan_ramp_up_pct_s = v.fan_ramp_up_pct_s;
            self.applied.fan_ramp_down_pct_s = v.fan_ramp_down_pct_s;
            self.applied.fan_smoothing_s = v.fan_smoothing_s;
            self.applied.fan_quiet_until_c = v.fan_quiet_until_c;
        }
        self.faults = errors;
        if self.faults.is_empty() {
            Ok(())
        } else {
            Err(self.faults.join("; "))
        }
    }
    fn is_stock(&self) -> Option<bool> {
        if !self.faults.is_empty() || self.required_fans > self.fans {
            return None;
        }
        let same = (!self.available("power")
            || self.applied.power_limit_w == self.restore.power_limit_w)
            && (!self.available("core_clock")
                || self.applied.core_offset_mhz == self.restore.core_offset_mhz)
            && (!self.available("mem_clock")
                || self.applied.mem_offset_mhz == self.restore.mem_offset_mhz)
            && (!self.available("temp_target")
                || self.applied.temp_target_c == self.restore.temp_target_c)
            && (self.fans == 0 || self.applied.fan_mode == FanMode::Auto && self.fan_now.is_none());
        if !same {
            Some(false)
        } else if self.offsets_verified.iter().all(|v| *v) {
            Some(true)
        } else {
            None
        }
    }
}

pub fn open(pci: &str, uuid: &str) -> Result<NvmlSkriver, String> {
    if !valid_binding(pci, uuid) {
        return Err("identity_required: PCI og UUID kreves".into());
    }
    let name = CString::new("libnvidia-ml.so.1").unwrap();
    // SAFETY: fast biblioteknavn. Verken navn eller symboler kommer fra IPC.
    let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return Err("not_supported: fant ikke libnvidia-ml.so.1".into());
    }
    let mut api = DeviceApi {
        handle,
        dev: std::ptr::null_mut(),
        initialized: false,
        identity: Value::Null,
    };
    let init = unsafe { api.sym::<unsafe extern "C" fn() -> Ret>("nvmlInit_v2") }
        .ok_or("not_supported: nvmlInit_v2")?;
    nvml_result("nvmlInit_v2", unsafe { init() })?;
    api.initialized = true;
    let bus = CString::new(pci).map_err(|_| "identity_required: ugyldig PCI")?;
    let by_pci = unsafe {
        api.sym::<unsafe extern "C" fn(*const c_char, *mut Dev) -> Ret>(
            "nvmlDeviceGetHandleByPciBusId_v2",
        )
    }
    .ok_or("not_supported: PCI-binding")?;
    nvml_result("nvmlDeviceGetHandleByPciBusId_v2", unsafe {
        by_pci(bus.as_ptr(), &mut api.dev)
    })?;
    let actual = api.dev_string("nvmlDeviceGetUUID")?;
    if actual != uuid {
        return Err("identity_mismatch: UUID stemmer ikke med PCI-adressen".into());
    }
    let vbios = api.dev_string("nvmlDeviceGetVbiosVersion")?;
    if vbios.trim().is_empty() {
        return Err("identity_mismatch: VBIOS kunne ikke identifiseres".into());
    }
    let mut driver = [0 as c_char; 96];
    let get_driver = unsafe {
        api.sym::<unsafe extern "C" fn(*mut c_char, c_uint) -> Ret>("nvmlSystemGetDriverVersion")
    }
    .ok_or("not_supported: driveridentitet")?;
    nvml_result("nvmlSystemGetDriverVersion", unsafe {
        get_driver(driver.as_mut_ptr(), driver.len() as u32)
    })?;
    driver[95] = 0;
    let driver = unsafe { CStr::from_ptr(driver.as_ptr()) }.to_string_lossy();
    if driver.trim().is_empty() {
        return Err("identity_mismatch: driveren kunne ikke identifiseres".into());
    }
    api.identity =
        json!({"pci": pci.to_ascii_lowercase(), "uuid": actual, "driver": driver, "vbios": vbios});
    Ok(NvmlSkriver::from_api(Box::new(api)))
}

impl Skriver for NvmlSkriver {
    fn backend(&self) -> &'static str {
        "nvml"
    }
    fn limits(&self) -> Limits {
        self.limits
    }
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn capabilities(&self) -> Value {
        self.caps.clone()
    }
    fn current_values(&self) -> Values {
        self.applied.clone()
    }
    fn baseline(&self) -> Value {
        self.baseline.clone()
    }
    fn load_baseline(&mut self, baseline: &Value) -> Result<(), String> {
        if baseline["version"] != 1 || baseline["identity"] != self.identity {
            return Err(
                "identity_mismatch: grunnverdier tilhører et annet kort, VBIOS eller driver".into(),
            );
        }
        let v = &baseline["captured"];
        let num = |v: &Value| {
            v.as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| "invalid_argument: ugyldige grunnverdier".to_string())
        };
        let mut restore = self.limits.stock();
        restore.power_limit_w = num(&v["powerLimitW"])?;
        restore.temp_target_c = num(&v["tempTargetC"])?;
        restore.core_offset_mhz = num(&v["clockOffsets"]["coreMhz"])?;
        restore.mem_offset_mhz = num(&v["clockOffsets"]["memMhz"])?;
        restore.fan_percent = num(&v["fan"]["manualPercent"])?;
        if v["fan"]["mode"] != "auto" {
            return Err("invalid_argument: grunnverdiene må bruke automatisk vifte".into());
        }
        for (key, value, range) in [
            ("power", restore.power_limit_w, self.limits.power_w),
            ("core_clock", restore.core_offset_mhz, self.limits.core_mhz),
            ("mem_clock", restore.mem_offset_mhz, self.limits.mem_mhz),
            (
                "temp_target",
                restore.temp_target_c,
                self.limits.temp_target_c,
            ),
        ] {
            if self.available(key) {
                self.check(key, value, range)?;
            }
        }
        self.limits.default.temp_target_c = restore.temp_target_c;
        self.limits.default.power_w = restore.power_limit_w;
        self.limits.default.core_offset_mhz = restore.core_offset_mhz;
        self.limits.default.mem_offset_mhz = restore.mem_offset_mhz;
        self.limits.default.fan_percent = restore.fan_percent;
        self.restore = restore;
        self.baseline = baseline.clone();
        self.baseline_ready = true;
        Ok(())
    }
    fn temperature(&mut self) -> Option<f64> {
        self.api.temperature().ok()
    }
    fn apply(&mut self, v: &Values) -> Result<(), String> {
        self.guard()?;
        if !self.faults.is_empty() {
            return Err("hardware_fault: gjenoppretting må lykkes før nye endringer".into());
        }
        let old = self.applied.clone();
        for (key, value, before, range) in [
            (
                "power",
                v.power_limit_w,
                old.power_limit_w,
                self.limits.power_w,
            ),
            (
                "core_clock",
                v.core_offset_mhz,
                old.core_offset_mhz,
                self.limits.core_mhz,
            ),
            (
                "mem_clock",
                v.mem_offset_mhz,
                old.mem_offset_mhz,
                self.limits.mem_mhz,
            ),
            (
                "temp_target",
                v.temp_target_c,
                old.temp_target_c,
                self.limits.temp_target_c,
            ),
        ] {
            if value != before {
                self.check(key, value, range)?;
            }
        }
        if v.fan_mode != old.fan_mode
            || v.fan_mode == FanMode::Manual && v.fan_percent != old.fan_percent
        {
            self.check(
                if v.fan_mode == FanMode::Curve {
                    "fan_curve"
                } else {
                    "fan_manual"
                },
                v.fan_percent,
                self.limits.fan_pct,
            )?;
        }
        let result = (|| {
            if v.power_limit_w != old.power_limit_w {
                self.power(v.power_limit_w)?;
            }
            if v.core_offset_mhz != old.core_offset_mhz {
                self.offset(GRAPHICS, v.core_offset_mhz)?;
            }
            if v.mem_offset_mhz != old.mem_offset_mhz {
                self.offset(MEMORY, v.mem_offset_mhz)?;
            }
            if v.temp_target_c != old.temp_target_c {
                self.temp(v.temp_target_c)?;
            }
            match v.fan_mode {
                FanMode::Auto if old.fan_mode != FanMode::Auto => self.fans_auto()?,
                FanMode::Manual => self.set_fan_percent(v.fan_percent)?,
                _ => {}
            }
            self.applied = v.clone();
            Ok(())
        })();
        self.record(result)
    }
    fn set_fan_percent(&mut self, pct: f64) -> Result<(), String> {
        self.set_fan_percents(&vec![pct; self.fans])
    }
    fn restore_values(&mut self, v: &Values) -> Result<(), String> {
        self.restore_to(v)
    }
    fn set_fan_percents(&mut self, pct: &[f64]) -> Result<(), String> {
        self.guard()?;
        if pct.len() != self.fans || pct.is_empty() {
            return Err("invalid_argument: én fart per vifte kreves".into());
        }
        for p in pct {
            self.check("fan_manual", *p, self.limits.fan_pct)?;
        }
        let mut errors = vec![];
        let mut targets = vec![None; self.fans];
        let mut policies = vec![None; self.fans];
        for (i, p) in pct.iter().enumerate() {
            let r = (|| {
                if let Err(e) = self.api.set_fan(i, *p) {
                    self.disable_unsupported("fan_manual", &e);
                    self.disable_unsupported("fan_curve", &e);
                    return Err(e);
                }
                let target = self.api.fan_target(i)?;
                let policy = self.api.fan_policy(i)?;
                targets[i] = Some(target);
                policies[i] = Some(policy);
                if policy != 1 || (target - p.round()).abs() > 0.001 {
                    return Err("readback_mismatch: viftepolicy eller målfart stemmer ikke".into());
                }
                Ok(())
            })();
            if let Err(e) = r {
                errors.push(format!("vifte {i}: {e}"));
            }
        }
        self.observed["fanTarget"] = json!(targets);
        self.observed["fanPolicy"] = json!(policies);
        if errors.is_empty() {
            self.fan_now = pct.iter().copied().reduce(f64::max);
            Ok(())
        } else {
            self.record(Err(errors.join("; ")))
        }
    }
    fn fan_count(&self) -> usize {
        self.fans
    }
    fn require_fan_count(&mut self, n: usize) {
        self.required_fans = n;
    }
    fn fan_percents(&mut self) -> Vec<f64> {
        (0..self.fans)
            .map(|i| self.api.fan_speed(i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default()
    }
    fn snapshot(&self) -> Value {
        json!({"applied": self.applied.to_json(), "observed": self.observed, "stock": self.is_stock(), "faults": self.faults, "identity": self.identity, "capabilities": self.caps, "baselineReady": self.baseline_ready, "offsetsVerified": self.offsets_verified, "fanNow": self.fan_now})
    }
    fn reset(&mut self) -> Result<(), String> {
        let restore = self.restore.clone();
        self.restore_to(&restore)?;
        if self.is_stock() != Some(true) {
            let e =
                "readback_mismatch: grunnverdiene kunne ikke verifiseres etter reset".to_string();
            self.faults.push(e.clone());
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct State {
        power: f64,
        offsets: [f64; 2],
        policies: [u32; 2],
        targets: [f64; 2],
        temp_target: f64,
        calls: Vec<String>,
        fail: Vec<String>,
        unsupported_temp: bool,
        unsupported_memory: bool,
        mismatch_power: bool,
        stale_offsets: bool,
        offset_read_errors: bool,
        unsupported_setters: Vec<String>,
        frozen_offsets: Option<[f64; 2]>,
        fan_read_errors: bool,
    }
    impl Default for State {
        fn default() -> Self {
            Self {
                power: 245.0,
                offsets: [0.0; 2],
                policies: [0; 2],
                targets: [35.0; 2],
                temp_target: 83.0,
                calls: vec![],
                fail: vec![],
                unsupported_temp: false,
                unsupported_memory: false,
                mismatch_power: false,
                stale_offsets: false,
                offset_read_errors: false,
                unsupported_setters: vec![],
                frozen_offsets: None,
                fan_read_errors: false,
            }
        }
    }
    struct Mock(Rc<RefCell<State>>);
    impl Mock {
        fn write(&self, call: String) -> Result<(), String> {
            let mut s = self.0.borrow_mut();
            s.calls.push(call.clone());
            if s.unsupported_setters.contains(&call) {
                return Err(format!("not_supported: {call}"));
            }
            if s.fail.contains(&call) {
                Err(format!("no_permission: {call}"))
            } else {
                Ok(())
            }
        }
    }
    impl Api for Mock {
        fn identity(&self) -> Value {
            json!({"pci":"0000:1c:00.0", "uuid":"GPU-1234-abcd", "driver":"610", "vbios":"90.04"})
        }
        fn supports(&self, _: &'static str) -> bool {
            true
        }
        fn power(&mut self) -> Result<(Range, f64, f64), String> {
            let s = self.0.borrow();
            Ok((
                Range::new(105.0, 275.0),
                245.0,
                s.power + if s.mismatch_power { 1.0 } else { 0.0 },
            ))
        }
        fn offset(&mut self, kind: c_int) -> Result<(f64, Range), String> {
            let s = self.0.borrow();
            if s.offset_read_errors {
                return Err("not_supported: offset readback".into());
            }
            let mem = kind == MEMORY;
            if mem && s.unsupported_memory {
                return Err("not_supported: memory".into());
            }
            Ok((
                if let Some(values) = s.frozen_offsets {
                    values[usize::from(mem)]
                } else if s.stale_offsets {
                    0.0
                } else {
                    s.offsets[usize::from(mem)]
                },
                if mem {
                    Range::new(-2000.0, 6000.0)
                } else {
                    Range::new(-1000.0, 1000.0)
                },
            ))
        }
        fn temp_limits(&mut self) -> Result<(f64, Range), String> {
            let s = self.0.borrow();
            if s.unsupported_temp {
                Err("not_supported: temperature target".into())
            } else {
                Ok((s.temp_target, Range::new(65.0, 88.0)))
            }
        }
        fn fans(&mut self) -> Result<(usize, Range), String> {
            Ok((2, Range::new(35.0, 100.0)))
        }
        fn temperature(&mut self) -> Result<f64, String> {
            Ok(45.0)
        }
        fn fan_policy(&mut self, i: usize) -> Result<u32, String> {
            if self.0.borrow().fan_read_errors {
                return Err("unknown: viftepolicy kunne ikke leses".into());
            }
            Ok(self.0.borrow().policies[i])
        }
        fn fan_target(&mut self, i: usize) -> Result<f64, String> {
            Ok(self.0.borrow().targets[i])
        }
        fn fan_speed(&mut self, i: usize) -> Result<f64, String> {
            Ok(self.0.borrow().targets[i])
        }
        fn set_power(&mut self, v: f64) -> Result<(), String> {
            self.write("power".into())?;
            self.0.borrow_mut().power = v;
            Ok(())
        }
        fn set_offset(&mut self, kind: c_int, v: f64) -> Result<(), String> {
            self.write(format!("offset{kind}"))?;
            self.0.borrow_mut().offsets[usize::from(kind == MEMORY)] = v;
            Ok(())
        }
        fn set_temp(&mut self, v: f64) -> Result<(), String> {
            self.write("temp".into())?;
            self.0.borrow_mut().temp_target = v;
            Ok(())
        }
        fn set_fan(&mut self, i: usize, p: f64) -> Result<(), String> {
            self.write(format!("fan{i}"))?;
            let mut s = self.0.borrow_mut();
            s.policies[i] = 1;
            s.targets[i] = p;
            Ok(())
        }
        fn auto_fan(&mut self, i: usize) -> Result<(), String> {
            self.write(format!("auto{i}"))?;
            self.0.borrow_mut().policies[i] = 0;
            Ok(())
        }
    }
    fn backend(state: State) -> (NvmlSkriver, Rc<RefCell<State>>) {
        let state = Rc::new(RefCell::new(state));
        (NvmlSkriver::from_api(Box::new(Mock(state.clone()))), state)
    }
    fn ready(hw: &mut NvmlSkriver) {
        hw.load_baseline(&hw.baseline()).unwrap();
    }

    #[test]
    fn aapning_og_baseline_bekreftelse_skriver_ikke() {
        let (mut h, s) = backend(State::default());
        let v = Values {
            power_limit_w: 200.0,
            ..h.current_values()
        };
        assert!(h.apply(&v).unwrap_err().starts_with("baseline_required:"));
        assert!(s.borrow().calls.is_empty());
        let mut bad = h.baseline();
        bad["identity"]["uuid"] = json!("GPU-9999");
        assert!(
            h.load_baseline(&bad)
                .unwrap_err()
                .starts_with("identity_mismatch:")
        );
        bad = h.baseline();
        bad["identity"]["driver"] = json!("611");
        assert!(h.load_baseline(&bad).is_err());
        ready(&mut h);
        assert!(s.borrow().calls.is_empty());
    }

    #[test]
    fn unsupported_temperatur_og_minne_blokkerer_ikke_effekt_og_kjerne() {
        let (mut h, s) = backend(State {
            unsupported_temp: true,
            unsupported_memory: true,
            ..Default::default()
        });
        ready(&mut h);
        assert_eq!(h.capabilities()["temp_target"]["available"], false);
        assert_eq!(h.capabilities()["mem_clock"]["available"], false);
        let v = Values {
            power_limit_w: 200.0,
            core_offset_mhz: 50.0,
            ..h.current_values()
        };
        h.apply(&v).unwrap();
        assert_eq!(s.borrow().power, 200.0);
        assert_eq!(s.borrow().offsets[0], 50.0);
        let bad = Values {
            power_limit_w: 210.0,
            mem_offset_mhz: 100.0,
            ..v
        };
        let before = s.borrow().calls.len();
        assert!(h.apply(&bad).unwrap_err().starts_with("not_supported:"));
        assert_eq!(
            s.borrow().calls.len(),
            before,
            "avvis hele oppsettet før delvis skriving"
        );
        h.reset().unwrap();
        assert!(
            !s.borrow()
                .calls
                .iter()
                .any(|c| c == "temp" || c == "offset2")
        );
        assert_eq!(h.snapshot()["stock"], true);
    }

    #[test]
    fn delvis_apply_kan_gjenopprettes_uten_aa_stole_paa_cache() {
        let (mut h, s) = backend(State::default());
        ready(&mut h);
        let previous = h.current_values();
        s.borrow_mut().fail.push("offset0".into());
        assert!(
            h.apply(&Values {
                power_limit_w: 200.0,
                core_offset_mhz: 50.0,
                ..previous.clone()
            })
            .is_err()
        );
        assert_eq!(s.borrow().power, 200.0);
        assert_eq!(h.snapshot()["applied"]["powerLimitW"], 200.0);
        assert!(h.snapshot()["stock"].is_null());
        s.borrow_mut().fail.clear();
        s.borrow_mut().calls.clear();
        h.restore_values(&previous).unwrap();
        assert_eq!(s.borrow().power, 245.0);
        assert!(
            s.borrow().calls.contains(&"offset0".into()),
            "skriv også feltet som feilet før cacheoppdatering"
        );
        assert_eq!(h.snapshot()["stock"], true);
    }

    #[test]
    fn reset_proever_alle_vifter_og_felt_selv_ved_feil() {
        let (mut h, s) = backend(State::default());
        ready(&mut h);
        h.apply(&Values {
            power_limit_w: 200.0,
            fan_mode: FanMode::Manual,
            fan_percent: 70.0,
            ..h.current_values()
        })
        .unwrap();
        s.borrow_mut().calls.clear();
        s.borrow_mut().fail = vec!["auto0".into(), "power".into()];
        let error = h.reset().unwrap_err();
        assert!(error.contains("auto0") && error.contains("power"));
        for call in ["auto0", "auto1", "power", "offset0", "offset2", "temp"] {
            assert!(s.borrow().calls.iter().any(|c| c == call), "mangler {call}");
        }
        assert_eq!(s.borrow().policies, [1, 0]);
        assert!(h.snapshot()["stock"].is_null());
        s.borrow_mut().fail.clear();
        h.reset().unwrap();
        assert_eq!(s.borrow().policies, [0, 0]);
        assert_eq!(h.snapshot()["stock"], true);
    }

    #[test]
    fn readback_avvik_og_viftefeil_skjules_ikke() {
        let (mut h, s) = backend(State::default());
        ready(&mut h);
        s.borrow_mut().mismatch_power = true;
        assert!(
            h.apply(&Values {
                power_limit_w: 200.0,
                ..h.current_values()
            })
            .unwrap_err()
            .starts_with("readback_mismatch:")
        );
        assert_eq!(h.snapshot()["observed"]["powerLimitW"], 201.0);
        assert!(h.snapshot()["stock"].is_null());
        s.borrow_mut().mismatch_power = false;
        h.reset().unwrap();
        s.borrow_mut().fail.push("fan0".into());
        assert!(h.set_fan_percents(&[60.0, 70.0]).is_err());
        assert!(h.snapshot()["fanNow"].is_null());
        assert_eq!(h.snapshot()["observed"]["fanTarget"], json!([null, 70.0]));
        assert!(s.borrow().calls.contains(&"fan1".into()));
    }

    #[test]
    fn direkte_viftefart_er_ikke_standard_selv_naar_boka_staar_auto() {
        let (mut h, _) = backend(State::default());
        ready(&mut h);
        h.set_fan_percent(100.0).unwrap();
        assert_eq!(h.current_values().fan_mode, FanMode::Auto);
        assert_eq!(
            h.snapshot()["stock"],
            false,
            "nødregelens direkte viftekommando er også en maskinvareendring"
        );
        h.reset().unwrap();
        assert_eq!(h.snapshot()["stock"], true);
    }

    #[test]
    fn recovery_uten_gjenfunne_vifter_er_ikke_standard() {
        // Økt 1: manuell vifte, og merkøren registrerer to styrbare vifter.
        let (mut first, _) = backend(State::default());
        ready(&mut first);
        first
            .apply(&Values {
                fan_mode: FanMode::Manual,
                fan_percent: 70.0,
                ..first.current_values()
            })
            .unwrap();
        let baseline = first.baseline();
        let recorded = first.fan_count();
        assert_eq!(recorded, 2);
        // Økt 2 (ExecStopPost etter krasj): viftene står fortsatt manuelt,
        // men viftepolicyen kan ikke leses, så oppdagelsen finner 0 vifter.
        let lost = || State {
            policies: [1, 1],
            targets: [70.0; 2],
            fan_read_errors: true,
            ..Default::default()
        };
        let (mut recovery, state) = backend(lost());
        assert_eq!(recovery.fan_count(), 0);
        recovery.load_baseline(&baseline).unwrap();
        recovery.require_fan_count(recorded);
        let e = recovery.reset().unwrap_err();
        assert!(e.contains("fan_unverified"), "{e}");
        assert_ne!(recovery.snapshot()["stock"], true);
        assert_eq!(state.borrow().policies, [1, 1], "viftene er fortsatt manuelle");
        // Uten kravet ville samme kort feilaktig blitt meldt som standard.
        let (mut naive, _) = backend(lost());
        naive.load_baseline(&baseline).unwrap();
        naive.reset().unwrap();
        assert_eq!(naive.snapshot()["stock"], true);
        // Når viftene er tilbake, gjenopprettes og bekreftes Auto.
        let (mut again, state) = backend(State {
            policies: [1, 1],
            targets: [70.0; 2],
            ..Default::default()
        });
        again.load_baseline(&baseline).unwrap();
        again.require_fan_count(recorded);
        again.reset().unwrap();
        assert_eq!(state.borrow().policies, [0, 0]);
        assert_eq!(again.snapshot()["stock"], true);
    }

    #[test]
    fn persistbaseline_beholder_temperaturmaal_over_hjelperrestart() {
        let (first, _) = backend(State::default());
        let baseline = first.baseline();
        let (mut restarted, state) = backend(State {
            temp_target: 86.0,
            ..Default::default()
        });
        assert_eq!(restarted.current_values().temp_target_c, 86.0);
        restarted.load_baseline(&baseline).unwrap();
        assert_eq!(restarted.limits().stock().temp_target_c, 83.0);
        assert!(
            state.borrow().calls.is_empty(),
            "load_baseline er bare lesing av en verdi"
        );
        restarted.reset().unwrap();
        assert_eq!(state.borrow().temp_target, 83.0);
    }

    #[test]
    fn bare_effektendring_bevarer_fanget_ekstern_effekt_og_offset_ved_reset() {
        let (mut h, state) = backend(State {
            power: 200.0,
            offsets: [100.0, 400.0],
            ..Default::default()
        });
        ready(&mut h);
        assert_eq!(h.limits().stock().power_limit_w, 200.0);
        assert_eq!(h.limits().stock().core_offset_mhz, 100.0);
        assert_eq!(h.limits().stock().mem_offset_mhz, 200.0);
        let baseline = h.baseline();
        h.apply(&Values {
            power_limit_w: 220.0,
            ..h.current_values()
        })
        .unwrap();
        assert_eq!(state.borrow().calls, vec!["power"]);
        h.reset().unwrap();
        assert_eq!(state.borrow().power, 200.0);
        assert_eq!(state.borrow().offsets, [100.0, 400.0]);
        assert_eq!(state.borrow().policies, [0, 0]);
        assert_eq!(h.snapshot()["stock"], true);
        // Ny ExecStopPost-prosess ser 220 W/manuell vifte, men bruker den
        // opprinnelige vedvarende tilstanden, ikke et nytt opptak.
        let (mut recovery, state) = backend(State {
            power: 220.0,
            offsets: [150.0, 600.0],
            policies: [1, 1],
            ..Default::default()
        });
        recovery.load_baseline(&baseline).unwrap();
        recovery.reset().unwrap();
        assert_eq!(state.borrow().power, 200.0);
        assert_eq!(state.borrow().offsets, [100.0, 400.0]);
        assert_eq!(state.borrow().policies, [0, 0]);
        assert_eq!(recovery.snapshot()["stock"], true);
    }

    #[test]
    fn ny_baseline_med_ekstern_manuell_vifte_avvises_uten_skriving() {
        let (mut h, state) = backend(State {
            policies: [1, 0],
            targets: [65.0, 35.0],
            ..Default::default()
        });
        assert!(h.load_baseline(&h.baseline()).is_err());
        assert!(state.borrow().calls.is_empty());
    }

    #[test]
    fn reset_er_ikke_vellykket_naar_offset_tilbakelesingen_er_ukjent() {
        let (mut h, state) = backend(State::default());
        ready(&mut h);
        h.apply(&Values {
            core_offset_mhz: 50.0,
            ..h.current_values()
        })
        .unwrap();
        state.borrow_mut().frozen_offsets = Some([50.0, 0.0]);
        assert!(h.reset().unwrap_err().starts_with("readback_mismatch:"));
        assert!(h.snapshot()["stock"].is_null());
        assert!(!h.snapshot()["faults"].as_array().unwrap().is_empty());
        state.borrow_mut().frozen_offsets = None;
        h.reset().unwrap();
        assert_eq!(h.snapshot()["stock"], true);
    }

    #[test]
    fn minneoffset_omregnes_og_offset_ack_er_ikke_falsk_readback() {
        let (mut h, s) = backend(State::default());
        ready(&mut h);
        s.borrow_mut().stale_offsets = true;
        assert!(
            h.apply(&Values {
                mem_offset_mhz: 200.0,
                ..h.current_values()
            })
            .unwrap_err()
            .starts_with("readback_mismatch:")
        );
        assert_eq!(s.borrow().offsets[1], 400.0);
        assert_eq!(h.snapshot()["applied"]["clockOffsets"]["memMhz"], 200.0);
        assert_eq!(h.snapshot()["observed"]["memOffsetMhz"], 0.0);
        assert_eq!(h.snapshot()["offsetsVerified"], json!([true, false]));
        assert!(h.snapshot()["stock"].is_null());
        assert!(
            h.snapshot()["faults"][0]
                .as_str()
                .unwrap()
                .starts_with("readback_mismatch:")
        );
        h.reset().unwrap();
        assert_eq!(s.borrow().offsets, [0.0, 0.0]);
        assert_eq!(h.snapshot()["offsetsVerified"], json!([true, true]));
        assert_eq!(h.snapshot()["stock"], true);
        assert_eq!(
            s.borrow().calls.iter().filter(|c| *c == "offset2").count(),
            2
        );
    }

    #[test]
    fn manglende_offset_readback_avviser_ogsaa_negative_verdier_og_recovery_proever_paa_nytt() {
        for (kind, value) in [(GRAPHICS, -75.0), (MEMORY, -200.0)] {
            let (mut h, state) = backend(State::default());
            ready(&mut h);
            state.borrow_mut().offset_read_errors = true;
            let mut wanted = h.current_values();
            if kind == GRAPHICS {
                wanted.core_offset_mhz = value;
            } else {
                wanted.mem_offset_mhz = value;
            }
            assert!(
                h.apply(&wanted)
                    .unwrap_err()
                    .starts_with("readback_mismatch:")
            );
            assert_eq!(h.current_values(), wanted);
            let i = usize::from(kind == MEMORY);
            assert!(!h.offsets_verified[i]);
            assert_eq!(
                state.borrow().offsets[i],
                value * if kind == MEMORY { 2.0 } else { 1.0 }
            );
            assert!(h.snapshot()["stock"].is_null());
            assert!(!h.snapshot()["faults"].as_array().unwrap().is_empty());
            // Selv ved en ny getterfeil forsøkes tilbakeføring; tilstanden
            // forblir ukjent helt til en senere recovery kan bekrefte den.
            assert!(h.reset().unwrap_err().starts_with("readback_mismatch:"));
            assert_eq!(state.borrow().offsets, [0.0, 0.0]);
            assert!(h.snapshot()["stock"].is_null());
            state.borrow_mut().offset_read_errors = false;
            h.reset().unwrap();
            assert_eq!(h.snapshot()["stock"], true);
            assert_eq!(h.snapshot()["offsetsVerified"], json!([true, true]));
            assert!(h.snapshot()["faults"].as_array().unwrap().is_empty());
            let call = format!("offset{kind}");
            assert_eq!(
                state.borrow().calls.iter().filter(|c| *c == &call).count(),
                3
            );
        }
    }

    #[test]
    fn unsupported_setter_fjerner_bare_den_aktuelle_funksjonen() {
        let (mut h, s) = backend(State::default());
        ready(&mut h);
        s.borrow_mut().unsupported_setters.push("temp".into());
        assert!(
            h.apply(&Values {
                temp_target_c: 84.0,
                ..h.current_values()
            })
            .unwrap_err()
            .starts_with("not_supported:")
        );
        assert_eq!(h.capabilities()["temp_target"]["available"], false);
        h.reset().unwrap();
        h.apply(&Values {
            power_limit_w: 200.0,
            ..h.current_values()
        })
        .unwrap();
        assert_eq!(s.borrow().power, 200.0);
        assert_eq!(
            s.borrow().calls.iter().filter(|c| *c == "temp").count(),
            1,
            "reset skal ikke prøve en setter som meldte NOT_SUPPORTED igjen"
        );
    }
}
