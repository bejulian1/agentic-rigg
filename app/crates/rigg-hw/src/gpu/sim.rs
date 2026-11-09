//! Den simulerte backenden. **Bare for tester og skjermbilder**
//! (`agentic-riggd --backend sim[:scenario]`, `RIGG_BACKEND=sim`). Ingenting
//! herfra vises som ekte i appen, og den kan ikke velges når NVML er aktiv.
//!
//! Scenarier (produktspesifikasjonen 3.8):
//! - `normal`: ett RTX 2080 med verdiene fra maskinvarerevisjonen.
//! - `na`: turtall, koder/dekoder og klokkebegrensninger er «ikke støttet».
//! - `to-kort`: to kort (kortvelgeren).
//! - `borte`: kortet faller av bussen etter 15 s, og kommer tilbake etter 45 s.
//! - `driver-oppdatert`: driveren er oppdatert uten omstart.
//! - `ingen-driver`: ingen NVIDIA-driver.
//! - `spill`: som `normal`, men med spill-last (runde 7B): 90 s med 80–95 %
//!   last og 60 s nesten i ro, om igjen. Da synes viftekurven, hysteresen
//!   og rampen i grafene.
//! - `hete` (runde 8A): en syklus på 240 s fra under 20 °C via 95 °C til
//!   110 °C og ned igjen, med effekt over taket (300 W). Tjenesten legger på
//!   prosessoren og diskene ([`hete`]), så skjermbildene viser at aksene
//!   ikke kutter. Fysisk umulig på et ekte kort; bare for skjermbildene.
//!
//! Temperaturen er en funksjon av effekten og viftefarten ([`Termo`], runde
//! 7B). Tjenesten kjører modellen i viftesløyfen (`rigg-hw/src/gpu/sim_vifter.rs`),
//! fordi det er der viftefarten bestemmes.

use super::{GpuBackend, Plan, Round};
use rigg_core::model::{DeviceState, GpuInfo, GpuSample, GpuStatic};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Normal,
    Na,
    TwoCards,
    Lost,
    DriverUpdated,
    NoDriver,
    Game,
    Heat,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        Some(match s {
            "" | "normal" => Scenario::Normal,
            "na" => Scenario::Na,
            "to-kort" => Scenario::TwoCards,
            "borte" => Scenario::Lost,
            "driver-oppdatert" => Scenario::DriverUpdated,
            "ingen-driver" => Scenario::NoDriver,
            "spill" => Scenario::Game,
            "hete" => Scenario::Heat,
            _ => return None,
        })
    }
}

struct SimCard {
    info: GpuInfo,
    statics: GpuStatic,
    phase: f64,
    load_scale: f64,
}

pub struct Sim {
    scenario: Scenario,
    start: i64,
    cards: Vec<SimCard>,
}

const DRIVER: &str = "610.43.02";

fn card(id: &str, index: u32, name: &str, phase: f64, load_scale: f64, now: i64) -> SimCard {
    SimCard {
        info: GpuInfo { id: id.into(), index, name: name.into(), state: DeviceState::Ok, since: now, driver: Some(DRIVER.into()), ..Default::default() },
        statics: GpuStatic {
            id: id.into(),
            name: name.into(),
            uuid: Some(format!("GPU-sim0000-{index:04}")),
            driver: Some(DRIVER.into()),
            nvml_version: Some("13.610.43.02".into()),
            cuda_version: Some("13.3".into()),
            vbios: Some("90.04.0B.40.8C".into()),
            pci_device_id: Some(0x1e8710de),
            pci_subsystem_id: Some(0x1e8710de),
            vram_total: Some(8192.0),
            power_min: Some(105.0),
            power_max: Some(275.0),
            power_default: Some(245.0),
            core_max: Some(2190.0),
            mem_max: Some(7000.0),
            sm_max: Some(2190.0),
            video_max: Some(2040.0),
            fan_count: Some(2),
            fan_min: Some(35.0),
            fan_max: Some(100.0),
            temp_shutdown: Some(100.0),
            temp_slowdown: Some(97.0),
            temp_max: Some(88.0),
            temp_target: Some(83.0),
            pcie_gen_max: Some(3.0),
            pcie_width_max: Some(16.0),
            extra: sim_extra(index),
        },
        phase,
        load_scale,
    }
}

/// Eksempeldata til `GpuStatic.extra`, i samme form som `nvml_info.rs` gir
/// fra RTX 2080 (rapport-eksempel i `docs/skjermbilder/1.0.0/runde2b/`),
/// så Grafikkort, Kontroller og Viftekurve viser de samme radene mot sim.
fn sim_extra(index: u32) -> serde_json::Value {
    let mut x = serde_json::json!({
        "brandId": 5, "brand": "GeForce", "archId": 6, "architecture": "Turing",
        "computeCap": "7.5", "cudaCores": 2944, "busWidth": 256, "boardId": format!("0x{:x}", 0x1c00 + index),
        "na": {"serial": "NVML_ERROR_NOT_SUPPORTED", "margin": "NVML_ERROR_NOT_SUPPORTED", "memMax": "NVML_ERROR_NOT_SUPPORTED",
               "appClockGfx": "NVML_ERROR_NOT_SUPPORTED", "defaultAppClockGfx": "NVML_ERROR_NOT_SUPPORTED", "partNumber": "NVML_ERROR_NOT_SUPPORTED"},
        "gspFirmware": DRIVER, "gspMode": {"enabled": true, "default": true},
        "inforomImage": "G001.0000.02.04", "inforomOem": "1.1",
        "bar1": {"totalMiB": 256.0, "usedMiB": 19.0}, "kernelModule": "Open Kernel Module",
        "powerLimit": 245.0, "acousticMin": 65, "acousticCurr": 83, "acousticMax": 88, "gpsCurr": 0,
        "offsetGfx": {"now": 0, "min": -1000, "max": 1000},
        "offsetMem": {"now": 0, "min": -2000, "max": 6000},
        "customerBoostGfx": 2190,
        "fans": [{"policy": 0, "target": 35, "signal": 2, "coolerTarget": 7}, {"policy": 0, "target": 35, "signal": 2, "coolerTarget": 7}],
        "persistence": false, "displayActive": index == 0, "computeMode": 0,
        "thermal": [{"controller": 1, "min": 0, "max": 127, "cur": 40, "target": 1}],
        "pcieReplay": 0, "pcieGenMaxGpu": 3, "pcieGenMaxSys": 3, "pcieWidthMax": 16,
    });
    x["pstates"] = serde_json::json!(["P0", "P2", "P3", "P5", "P8"]);
    x["pstateClocks"] = serde_json::json!([
        {"p": "P0", "gfx": [300, 2190], "mem": [7000, 7000]},
        {"p": "P2", "gfx": [300, 2190], "mem": [6800, 6800]},
        {"p": "P3", "gfx": [300, 2190], "mem": [5000, 5000]},
        {"p": "P5", "gfx": [300, 2100], "mem": [810, 810]},
        {"p": "P8", "gfx": [300, 645], "mem": [405, 405]},
    ]);
    x["memClocks"] = serde_json::json!([7000, 6800, 5000, 810, 405]);
    x["processes"] = sim_processes(index, 0);
    x["pci"] = serde_json::json!({
        "revision": "0xa1", "linkSpeed": "2.5 GT/s PCIe", "linkSpeedMax": "8.0 GT/s PCIe", "linkWidth": 16, "linkWidthMax": 16,
        "aerCorrectable": 0, "aerFatal": 0, "aerNonfatal": 0, "bootVga": index == 0, "powerState": "D0",
        "bars": [{"bar": 0, "size": 16_777_216}, {"bar": 1, "size": 268_435_456}, {"bar": 3, "size": 33_554_432}],
        "bar1Sizes": [64, 128, 256],
    });
    let st = super::nvml_info::styring(&x);
    x["styring"] = st;
    x
}

/// Eksempelprosesser. Videominnet går litt opp og ned, så det synes at lista lever.
fn sim_processes(index: u32, now: i64) -> serde_json::Value {
    let wobble = ((now / 5000) % 7) as f64;
    let list = if index == 0 {
        serde_json::json!([
            {"pid": 2198, "name": "gnome-shell", "vramMiB": 109.0, "kind": "grafikk"},
            {"pid": 5972, "name": "firefox", "vramMiB": 80.0 + wobble * 4.0, "kind": "grafikk og compute"},
            {"pid": 5977, "name": "Xwayland", "vramMiB": 3.0, "kind": "grafikk"},
        ])
    } else {
        serde_json::json!([{"pid": 7311, "name": "blender", "vramMiB": 412.0 + wobble, "kind": "compute"}])
    };
    serde_json::json!({"t": now, "list": list, "error": null})
}

impl Sim {
    pub fn new(scenario: Scenario, now: i64) -> Sim {
        let mut cards = Vec::new();
        if scenario != Scenario::NoDriver {
            cards.push(card("0000:1c:00.0", 0, "NVIDIA GeForce RTX 2080", 0.0, 1.0, now));
        }
        if scenario == Scenario::TwoCards {
            cards.push(card("0000:2d:00.0", 1, "NVIDIA GeForce RTX 2070 SUPER", 1.7, 0.6, now));
        }
        if scenario == Scenario::DriverUpdated {
            for c in &mut cards {
                c.info.state = DeviceState::DriverUpdated;
                c.info.driver_installed = Some("610.51".into());
                c.info.detail = Some("Kjernemodulen er 610.43.02, biblioteket er 610.51".into());
            }
        }
        Sim { scenario, start: now, cards }
    }

    /// `since_s`: sekunder siden starten (hete-syklusen begynner kaldt).
    fn value(c: &SimCard, t: f64, scenario: Scenario, since_s: f64) -> GpuSample {
        let s = t / 1000.0 + c.phase * 100.0;
        let wave = (s / 23.0).sin() * 0.5 + (s / 7.0).sin() * 0.25;
        let load = if scenario == Scenario::Game {
            // 150 s-syklus: 90 s spill (80–95 %, med små fall som i en ekte
            // scene), 60 s meny eller skrivebord (3–6 %).
            let k = s.rem_euclid(150.0);
            let dip = if (k % 37.0) < 3.0 { 25.0 } else { 0.0 };
            if k < 90.0 { 88.0 + 6.0 * wave - dip } else { 4.5 + wave }
        } else {
            let burst = if (s % 90.0) < 20.0 { 1.0 } else { 0.0 };
            4.0 + 30.0 * burst + 6.0 * wave
        };
        let heat = (scenario == Scenario::Heat).then(|| hete(since_s));
        let load = heat.map(|h| h.last).unwrap_or(load);
        let load = (load * c.load_scale).clamp(0.0, 100.0);
        let temp = heat.map(|h| h.gpu).unwrap_or(34.0 + load * 0.45 + wave);
        let p0 = load > 15.0;
        let fan = (35.0 + (temp - 55.0).max(0.0) * 2.0).clamp(35.0, 100.0);
        let na = scenario == Scenario::Na;
        GpuSample {
            temp: Some(temp.round()),
            gpu_util: Some(load.round()),
            mem_util: Some((load * 0.6).round()),
            enc_util: (!na).then_some(0.0),
            dec_util: (!na).then_some(0.0),
            core_clock: Some(if p0 { 1650.0 + load * 3.0 } else { 300.0 }.round()),
            sm_clock: Some(if p0 { 1650.0 + load * 3.0 } else { 300.0 }.round()),
            mem_clock: Some(if p0 { 7000.0 } else { 405.0 }),
            video_clock: Some(if p0 { 1500.0 } else { 540.0 }),
            power: Some(heat.map(|h| h.effekt).unwrap_or(12.4 + load * 2.0 + wave * 0.4)),
            power_limit: Some(245.0),
            fan: Some(fan.round()),
            fans: vec![Some(fan.round()), Some(fan.round())],
            fan_rpm: if na { vec![] } else { vec![Some((fan * 22.9).round()), Some((fan * 22.8).round())] },
            fan_policy: vec![Some(0), Some(0)],
            fan_target: vec![Some(fan.round()), Some(fan.round())],
            vram_used: Some(674.0 + load * 30.0),
            vram_total: Some(8192.0),
            vram_reserved: Some(399.0),
            pstate: Some(if p0 { "P2" } else { "P8" }.into()),
            pcie_gen: Some(if p0 { 3.0 } else { 1.0 }),
            pcie_width: Some(16.0),
            throttle: if na { None } else { Some(if p0 { 0 } else { 1 }) },
            throttle_inferred: !p0,
            energy_mj: Some(1_017_977_031.0 + t),
            na: if na { vec!["decUtil".into(), "encUtil".into(), "fanRpm".into(), "throttle".into()] } else { vec![] },
        }
    }
}

/// Ett punkt i hete-syklusen (`sim:hete`, runde 8A): GPU, prosessor og to
/// disker i °C, og GPU-last (%) og -effekt (W).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hete {
    pub gpu: f64,
    pub cpu: f64,
    pub disk_varm: f64,
    pub disk_kald: f64,
    pub last: f64,
    pub effekt: f64,
}

/// Lengden på hete-syklusen (s).
pub const HETE_SYKLUS_S: f64 = 240.0;

/// Hete-syklusen ved `t_s` sekunder etter start: 30 s kaldt (16–18 °C),
/// opp til 95 °C og 40 s der, opp til 110 °C og 40 s der med 300 W, så
/// avkjøling mot 17 °C. Prosessoren følger med (14–98 °C), én disk står på
/// 65 °C og en på 14 °C.
pub fn hete(t_s: f64) -> Hete {
    let k = t_s.rem_euclid(HETE_SYKLUS_S);
    let wave = (t_s / 5.0).sin();
    let ramp = |a: f64, b: f64, k0: f64, k1: f64| a + (b - a) * ((k - k0) / (k1 - k0)).clamp(0.0, 1.0);
    let (gpu, last) = if k < 30.0 {
        (17.0 + wave, 2.0)
    } else if k < 60.0 {
        (ramp(17.0, 95.0, 30.0, 60.0), 100.0)
    } else if k < 100.0 {
        (94.0 + wave, 100.0)
    } else if k < 120.0 {
        (ramp(95.0, 110.0, 100.0, 120.0), 100.0)
    } else if k < 160.0 {
        (109.0 + wave, 100.0)
    } else {
        (17.0 + 92.0 * (-(k - 160.0) / 22.0).exp(), 3.0)
    };
    let effekt = if (100.0..160.0).contains(&k) { 300.0 + 4.0 * wave } else if last > 50.0 { 240.0 + 3.0 * wave } else { 14.0 + wave };
    let cpu = 14.0 + (gpu - 17.0) * 0.9 + wave * 0.5;
    Hete { gpu: gpu.round(), cpu: (cpu * 10.0).round() / 10.0, disk_varm: 65.0 + wave * 0.4, disk_kald: 14.0, last, effekt }
}

/// Varmemodellen for det simulerte kortet (runde 7B): temperaturen går mot
/// likevekten `omgivelse + effekt × R(vifte)` med tidskonstanten [`Termo::TAU_S`].
/// Varmemotstanden synker med viftefarten (`R ∝ (35 / fart)^0,6`), så mer
/// vifte gir lavere temperatur, og temperaturen henger etter lasten slik et
/// ekte kjøleribbe gjør. Tallene er tilpasset RTX 2080 i maskinvarerevisjonen:
/// 36 °C i ro (20 W), 49 °C ved 80 W og ca. 74 °C ved 190 W, alt med 35 %.
#[derive(Clone, Debug)]
pub struct Termo {
    t: f64,
    sist_ms: Option<i64>,
}

impl Termo {
    /// Luften i kabinettet (°C).
    pub const OMGIVELSE_C: f64 = 31.6;
    /// Varmemotstand ved minste fart (°C per W).
    pub const R_MIN: f64 = 0.222;
    /// Tidskonstanten (s).
    pub const TAU_S: f64 = 20.0;

    pub fn new(t0: f64) -> Termo {
        Termo { t: t0, sist_ms: None }
    }

    /// Temperaturen kortet ville stabilisert seg på.
    pub fn likevekt(effekt_w: f64, vifte_pct: f64) -> f64 {
        let r = Self::R_MIN * (35.0 / vifte_pct.clamp(35.0, 100.0)).powf(0.6);
        Self::OMGIVELSE_C + effekt_w.max(0.0) * r
    }

    /// Ett steg: temperaturen nå (°C, med desimaler).
    pub fn steg(&mut self, effekt_w: f64, vifte_pct: f64, naa_ms: i64) -> f64 {
        let mal = Self::likevekt(effekt_w, vifte_pct);
        match self.sist_ms {
            None => self.t = mal,
            Some(s) => {
                let dt = (naa_ms - s).clamp(0, 60_000) as f64 / 1000.0;
                self.t += (mal - self.t) * (1.0 - (-dt / Self::TAU_S).exp());
            }
        }
        self.sist_ms = Some(naa_ms);
        self.t
    }

    pub fn temp(&self) -> f64 {
        self.t
    }
}

impl GpuBackend for Sim {
    fn kind(&self) -> &'static str {
        "sim"
    }

    fn gpus(&self) -> Vec<GpuInfo> {
        if self.scenario == Scenario::NoDriver {
            return vec![GpuInfo {
                id: String::new(),
                state: DeviceState::NoDriver,
                detail: Some("Fant ikke NVIDIA-driveren (libnvidia-ml.so.1)".into()),
                ..Default::default()
            }];
        }
        self.cards.iter().map(|c| c.info.clone()).collect()
    }

    fn sample(&mut self, plan: &Plan) -> Round {
        let mut out = Round::default();
        for c in &self.cards {
            if c.info.state == DeviceState::Lost {
                continue;
            }
            out.samples.insert(c.info.id.clone(), Self::value(c, plan.now_ms as f64, self.scenario, (plan.now_ms - self.start) as f64 / 1000.0));
        }
        out
    }

    fn static_info(&self, id: &str) -> Option<(GpuStatic, bool)> {
        self.cards.iter().find(|c| c.info.id == id).map(|c| (c.statics.clone(), true))
    }

    fn processes(&mut self, id: &str) -> Option<serde_json::Value> {
        let c = self.cards.iter().find(|c| c.info.id == id && c.info.state != DeviceState::Lost)?;
        Some(sim_processes(c.info.index, rigg_core::now_ms()))
    }

    fn check(&mut self, now_ms: i64) {
        if self.scenario != Scenario::Lost {
            return;
        }
        let age = now_ms - self.start;
        let lost = (15_000..45_000).contains(&age);
        for c in &mut self.cards {
            let want = if lost { DeviceState::Lost } else { DeviceState::Ok };
            if c.info.state != want {
                c.info.state = want;
                c.info.since = now_ms;
                c.info.detail = lost.then(|| "temp: NVML_ERROR_GPU_IS_LOST".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenariene() {
        let mut s = Sim::new(Scenario::TwoCards, 0);
        assert_eq!(s.gpus().len(), 2);
        let r = s.sample(&Plan { now_ms: 1000, ..Default::default() });
        assert_eq!(r.samples.len(), 2);
        let g = &r.samples["0000:1c:00.0"];
        assert!(g.temp.unwrap() > 30.0 && g.fans.len() == 2);
        let mut l = Sim::new(Scenario::Lost, 0);
        l.check(20_000);
        assert_eq!(l.gpus()[0].state, DeviceState::Lost);
        assert!(l.sample(&Plan { now_ms: 20_000, ..Default::default() }).samples.is_empty());
        l.check(50_000);
        assert_eq!(l.gpus()[0].state, DeviceState::Ok);
        assert_eq!(Sim::new(Scenario::NoDriver, 0).gpus()[0].state, DeviceState::NoDriver);
        assert_eq!(Sim::new(Scenario::DriverUpdated, 0).gpus()[0].state, DeviceState::DriverUpdated);
        assert!(Scenario::parse("tull").is_none());
        assert_eq!(Scenario::parse("spill"), Some(Scenario::Game));
    }

    #[test]
    fn spill_gir_hoy_last_og_pauser() {
        let mut s = Sim::new(Scenario::Game, 0);
        let last = |s: &mut Sim, ms: i64| s.sample(&Plan { now_ms: ms, ..Default::default() }).samples["0000:1c:00.0"].gpu_util.unwrap();
        assert!(last(&mut s, 10_000) > 75.0);
        assert!(last(&mut s, 120_000) < 12.0);
    }

    #[test]
    fn hete_går_fra_under_20_til_110_grader() {
        assert_eq!(Scenario::parse("hete"), Some(Scenario::Heat));
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        let mut over_taket = false;
        for s in 0..HETE_SYKLUS_S as i64 {
            let h = hete(s as f64);
            lo = lo.min(h.gpu);
            hi = hi.max(h.gpu);
            over_taket |= h.effekt > 288.75;
            assert!(h.disk_varm > 64.0 && h.disk_kald < 20.0);
        }
        assert!(lo < 20.0 && hi >= 110.0, "{lo}–{hi}");
        assert!(over_taket, "effekten går over grafens foretrukne tak");
        assert!((60..100).any(|s| (93.0..=96.0).contains(&hete(s as f64).gpu)), "en stund rundt 95 °C");
    }

    #[test]
    fn varmemodellen() {
        // Likevekten: mer effekt gir varmere, mer vifte gir kaldere.
        assert!((Termo::likevekt(20.0, 35.0) - 36.0).abs() < 0.1);
        assert!((Termo::likevekt(80.0, 35.0) - 49.4).abs() < 0.2);
        assert!(Termo::likevekt(190.0, 35.0) > Termo::likevekt(190.0, 60.0) + 8.0);
        assert_eq!(Termo::likevekt(100.0, 10.0), Termo::likevekt(100.0, 35.0), "under 35 % gjelder ikke");
        // Første steg er likevekten; så henger temperaturen etter.
        let mut t = Termo::new(40.0);
        assert!((t.steg(20.0, 35.0, 0) - 36.0).abs() < 0.1);
        let etter_1s = t.steg(190.0, 35.0, 1000);
        assert!(etter_1s > 36.5 && etter_1s < 40.0, "{etter_1s}");
        let mut siste = etter_1s;
        for i in 2..200 {
            siste = t.steg(190.0, 35.0, i * 1000);
        }
        assert!((siste - Termo::likevekt(190.0, 35.0)).abs() < 0.1);
    }

    #[test]
    fn faste_data_og_prosesser_som_fra_kortet() {
        let mut s = Sim::new(Scenario::Normal, 0);
        let (st, done) = s.static_info("0000:1c:00.0").unwrap();
        assert!(done);
        assert_eq!(st.extra["architecture"], "Turing");
        assert_eq!(st.extra["cudaCores"], 2944);
        let sty = &st.extra["styring"];
        assert_eq!(sty["memOffsetMHz"]["max"], 3000.0, "effektiv MHz");
        assert_eq!(sty["tempTarget"]["value"], 83.0);
        assert_eq!(sty["fanPolicy"], serde_json::json!([0, 0]));
        let p = s.processes("0000:1c:00.0").unwrap();
        assert_eq!(p["list"].as_array().unwrap().len(), 3);
        assert!(s.processes("0000:99:00.0").is_none());
        let r = s.sample(&Plan { now_ms: 1000, ..Default::default() });
        assert_eq!(r.samples["0000:1c:00.0"].fan_policy, vec![Some(0), Some(0)]);
    }
}
