//! Viftesløyfen mot det **simulerte** kortet (runde 7B). Bare `--backend
//! sim` kommer hit (`riggd`: `Daemon::overlay_sim`); med NVML skriver
//! tjenesten aldri til kortet, og denne fila brukes ikke. Ren logikk uten I/O.
//! Lå i `riggd/src/vifter.rs` til runde 7C; flyttet hit så root-hjelperen kan
//! prøves mot den samme sløyfen (`rigg-helper/src/motor.rs`, testen
//! `kurven_er_lik_tjenestens_sim_sloyfe`).
//!
//! Hver runde, per kort:
//! 1. Temperaturen fra varmemodellen ([`Termo`]): effekten
//!    fra prøven og farten viftene hadde forrige runde.
//! 2. Farten etter modusen i boka (`txn.active`):
//!    - **Auto:** driverens egen kurve, slik sim alltid har vist den.
//!    - **Manuell:** fast fart.
//!    - **Kurve:** viftemotoren ([`rigg_core::vifte::Motor`]) med hysterese,
//!      rampe, utjevning, stille flate, per-vifte-kurver og nødregelen.
//! 3. Prøven får temperaturen, farten, målfarten, turtallet og viftestyringen.
//!
//! Motoren lages når modus Kurve slås på, og får nytt oppsett (uten å miste
//! farten) når kurven eller innstillingene endres.

use rigg_core::model::GpuSample;
use rigg_core::txn::{FanMode, Values};
use rigg_core::vifte::{Motor, Oppsett};
use super::sim::Termo;
use std::collections::BTreeMap;

/// Tilstanden for ett simulert kort.
struct Kort {
    termo: Termo,
    /// Farten viftene har nå (%), én per vifte.
    fart: Vec<f64>,
    motor: Option<Motor>,
    sist_ms: i64,
}

/// Viftesløyfen for alle de simulerte kortene.
#[derive(Default)]
pub struct SimVifter {
    kort: BTreeMap<String, Kort>,
    /// Nødregelen gjelder for minst ett kort (til `get_state`).
    pub nod: bool,
}

/// Driverens egen kurve i sim (Auto), som `rigg-hw/src/gpu/sim.rs` alltid har brukt.
pub fn driverkurve(t: f64) -> f64 {
    (35.0 + (t - 55.0).max(0.0) * 2.0).clamp(35.0, 100.0)
}

/// Driveren flytter viftene mykt også i Auto (ca. 5 %/s).
const DRIVER_RAMPE: f64 = 5.0;

impl SimVifter {
    /// Fyller inn temperaturen og viftene i prøvene fra sim.
    pub fn runde(&mut self, gpus: &mut BTreeMap<String, GpuSample>, a: &Values, min_pct: f64, now: i64) {
        self.kort.retain(|id, _| gpus.contains_key(id));
        let mut nod = false;
        for (id, g) in gpus.iter_mut() {
            let n = g.fans.len().max(1);
            let k = self.kort.entry(id.clone()).or_insert_with(|| Kort { termo: Termo::new(g.temp.unwrap_or(40.0)), fart: vec![g.fan.unwrap_or(min_pct); n], motor: None, sist_ms: now });
            k.fart.resize(n, min_pct);
            // 1. Temperaturen fra effekten og farten viftene har.
            let snitt = k.fart.iter().sum::<f64>() / n as f64;
            let t = k.termo.steg(g.power.unwrap_or(20.0), snitt, now);
            // NVML melder hele grader.
            let temp = t.round();
            g.temp = Some(temp);
            let dt = ((now - k.sist_ms).max(0) as f64 / 1000.0).min(10.0);
            k.sist_ms = now;
            // 2. Farten etter modusen.
            let (fart, styrt) = match a.fan_mode {
                FanMode::Auto => {
                    k.motor = None;
                    let mal = driverkurve(temp);
                    let f: Vec<f64> = k.fart.iter().map(|c| if mal > *c { (c + DRIVER_RAMPE * dt).min(mal) } else { (c - DRIVER_RAMPE * dt).max(mal) }).collect();
                    (f, false)
                }
                FanMode::Manual => {
                    k.motor = None;
                    (vec![a.fan_percent.clamp(min_pct, 100.0); n], true)
                }
                FanMode::Curve => {
                    let o = Oppsett::fra_values(a, n, min_pct);
                    let m = k.motor.get_or_insert_with(|| Motor::new(o.clone()).med_start(&k.fart));
                    if m.oppsett() != &o {
                        m.sett_oppsett(o);
                    }
                    let s = m.steg(Some(temp), now);
                    nod |= s.nod;
                    (s.prosent, true)
                }
            };
            k.fart = fart;
            // 3. Prøven: fart i hele prosent, som NVML.
            let hel: Vec<Option<f64>> = k.fart.iter().map(|f| Some(f.round())).collect();
            g.fan = hel.iter().flatten().copied().reduce(f64::max);
            g.fans = hel.clone();
            g.fan_target = hel.clone();
            g.fan_policy = vec![Some(u32::from(styrt)); n];
            if !g.fan_rpm.is_empty() {
                g.fan_rpm = k.fart.iter().enumerate().map(|(i, f)| Some((f * if i == 0 { 22.9 } else { 22.8 }).round())).collect();
            }
        }
        self.nod = nod;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigg_core::txn::DEFAULT_CURVE;

    fn sample(power: f64) -> GpuSample {
        GpuSample { temp: Some(40.0), power: Some(power), fan: Some(35.0), fans: vec![Some(35.0), Some(35.0)], fan_rpm: vec![Some(800.0), Some(800.0)], fan_policy: vec![Some(0), Some(0)], ..Default::default() }
    }

    fn kjor(v: &mut SimVifter, a: &Values, power: f64, fra_s: i64, sek: i64) -> Vec<GpuSample> {
        (0..sek)
            .map(|i| {
                let mut g = BTreeMap::from([("k".to_string(), sample(power))]);
                v.runde(&mut g, a, 35.0, (fra_s + i) * 1000);
                g.remove("k").unwrap()
            })
            .collect()
    }

    #[test]
    fn auto_folger_driverkurven_og_styrer_ikke() {
        let mut v = SimVifter::default();
        let ut = kjor(&mut v, &Values::default(), 190.0, 0, 300);
        let siste = ut.last().unwrap();
        assert_eq!(siste.fan_policy, vec![Some(0), Some(0)]);
        let t = siste.temp.unwrap();
        assert!((60.0..75.0).contains(&t), "{t}");
        assert!((siste.fan.unwrap() - driverkurve(t)).abs() <= 2.0);
    }

    #[test]
    fn kurven_brukes_med_rampe_og_varmen_svarer() {
        let mut v = SimVifter::default();
        let a = Values { fan_mode: FanMode::Curve, ..Default::default() };
        // I ro først, så spill-last: farten går opp med høyst 10 %/s.
        kjor(&mut v, &a, 20.0, 0, 60);
        let ut = kjor(&mut v, &a, 190.0, 60, 240);
        for w in ut.windows(2) {
            assert!(w[1].fans[0].unwrap() - w[0].fans[0].unwrap() <= 10.5);
        }
        let siste = ut.last().unwrap();
        assert_eq!(siste.fan_policy, vec![Some(1), Some(1)]);
        let t = siste.temp.unwrap();
        // Likevekt mellom varme og vifte, under det Auto-kortet ville hatt med 35 %.
        assert!(t < Termo::likevekt(190.0, 35.0), "{t}");
        let mal = rigg_core::txn::curve_at(&DEFAULT_CURVE, t);
        assert!((siste.fans[0].unwrap() - mal).abs() <= 4.0, "{t} °C: {:?} mot {mal}", siste.fans);
        assert!(siste.fan_rpm[0].unwrap() > 900.0);
    }

    #[test]
    fn per_vifte_og_manuell() {
        let mut v = SimVifter::default();
        let a = Values { fan_mode: FanMode::Curve, fan_per_fan: true, fan_curves: vec![vec![(20.0, 40.0), (100.0, 40.0)], vec![(20.0, 70.0), (100.0, 70.0)]], ..Default::default() };
        let ut = kjor(&mut v, &a, 100.0, 0, 30);
        assert_eq!(ut.last().unwrap().fans, vec![Some(40.0), Some(70.0)]);
        assert_eq!(ut.last().unwrap().fan, Some(70.0));
        let m = Values { fan_mode: FanMode::Manual, fan_percent: 20.0, ..Default::default() };
        let ut = kjor(&mut v, &m, 100.0, 30, 2);
        assert_eq!(ut[1].fans, vec![Some(35.0), Some(35.0)], "klemt til minimum");
    }

    #[test]
    fn nodregelen_meldes() {
        let mut v = SimVifter::default();
        // Flat kurve på minimum og 245 W: kortet blir varmt, nødregelen slår inn.
        let a = Values { fan_mode: FanMode::Curve, fan_curve: vec![(20.0, 35.0), (100.0, 35.0)], ..Default::default() };
        let ut = kjor(&mut v, &a, 245.0, 0, 200);
        assert!(v.nod || ut.iter().any(|g| g.fans[0] == Some(100.0)));
        assert!(ut.iter().all(|g| g.temp.unwrap() < 90.0), "nødregelen holder temperaturen nede");
    }

    #[test]
    fn kort_som_forsvinner_glemmes() {
        let mut v = SimVifter::default();
        kjor(&mut v, &Values::default(), 50.0, 0, 2);
        assert_eq!(v.kort.len(), 1);
        v.runde(&mut BTreeMap::new(), &Values::default(), 35.0, 3000);
        assert!(v.kort.is_empty());
    }
}
