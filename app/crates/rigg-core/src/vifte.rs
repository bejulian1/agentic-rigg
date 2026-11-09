//! Viftemotoren (runde 7B): fra temperatur til viftefart, som ren logikk uten
//! I/O. Tjenesten bruker den mot det simulerte kortet (`riggd`), og
//! root-hjelperen (7A) skal bruke den samme motoren mot det ekte kortet
//! senere. Tiden sendes inn (`naa_ms`), så alt kan testes uten å vente.
//!
//! Hver runde, i denne rekkefølgen:
//! 1. **Nødregelen** ser på den rå temperaturen: fra [`NOD_C`] (85 °C) går
//!    alle viftene straks til 100 %, og de blir der til temperaturen er under
//!    [`NOD_SLIPP_C`] (80 °C). Den kan ikke slås av, og den venter ikke på
//!    utjevning eller rampe.
//! 2. **Utjevning:** snittet av temperaturene de siste `utjevning_s`
//!    sekundene (0 = av), så korte topper ikke gir viftesus.
//! 3. **Hysterese:** opp går kurven straks; ned først når temperaturen har
//!    falt `hysterese_c` under punktet der viftene gikk opp. Det samme som å
//!    følge kurven forskjøvet `hysterese_c` mot venstre på vei ned.
//! 4. **Kurven** for hver vifte (felles eller egen), med **stille flate**:
//!    under `stille_til_c` holdes minste fart.
//! 5. **Minimum:** alt under kortets minste fart (35 % på RTX 2080, VBIOS)
//!    klemmes opp. Viftestopp (0 %) er ikke mulig på dette kortet.
//! 6. **Rampe:** farten flyttes mot målet med høyst `rampe_opp_pct_s` opp og
//!    `rampe_ned_pct_s` ned per sekund.
//!
//! Ukjent temperatur (lesefeil): farten holdes i [`UKJENT_FRIST_MS`], deretter
//! går viftene til 100 % til temperaturen kan leses igjen.

use crate::txn::{self, DEFAULT_CURVE, Values};
use std::collections::VecDeque;

/// Én kurve: (temperatur °C, fart %), stigende temperatur.
pub type Kurve = Vec<(f64, f64)>;

/// Nødregelen: full fart fra 85 °C, tilbake til kurven under 80 °C.
pub const NOD_C: f64 = txn::EMERGENCY_FAN_C;
pub const NOD_SLIPP_C: f64 = txn::EMERGENCY_RELEASE_C;
/// Hvor lenge farten holdes når temperaturen ikke kan leses, før full fart.
pub const UKJENT_FRIST_MS: i64 = 5_000;
/// Flest vifter motoren styrer (og flest kurver i `fan.curves`).
pub const MAKS_VIFTER: usize = 8;

/// Hysterese i °C: standard og område.
pub const HYSTERESE_STANDARD_C: f64 = 3.0;
pub const HYSTERESE: txn::Range = txn::Range::new(0.0, 10.0);
/// Rampefart i % per sekund: standard opp og ned, og område.
pub const RAMPE_OPP_STANDARD: f64 = 10.0;
pub const RAMPE_NED_STANDARD: f64 = 3.0;
pub const RAMPE: txn::Range = txn::Range::new(1.0, 100.0);
/// Utjevning av temperaturen i sekunder (0 = av), og valgene i vinduet.
pub const UTJEVNING: txn::Range = txn::Range::new(0.0, 10.0);
pub const UTJEVNING_VALG: [f64; 4] = [0.0, 3.0, 5.0, 10.0];
/// «Hold minimum til» (°C).
pub const STILLE: txn::Range = txn::Range::new(30.0, 80.0);

/// Det motoren trenger. Lages fra [`Values`] med [`Oppsett::fra_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct Oppsett {
    /// Én kurve per vifte, eller én for alle (felles kurve).
    pub kurver: Vec<Kurve>,
    /// Antall vifter (2 på RTX 2080).
    pub vifter: usize,
    pub hysterese_c: f64,
    pub rampe_opp_pct_s: f64,
    pub rampe_ned_pct_s: f64,
    pub utjevning_s: f64,
    /// Minste fart under denne temperaturen (`None`: av).
    pub stille_til_c: Option<f64>,
    /// Kortets minste fart (`nvmlDeviceGetMinMaxFanSpeed`, 35 %).
    pub min_pct: f64,
}

impl Default for Oppsett {
    fn default() -> Self {
        Oppsett::fra_values(&Values::default(), 2, txn::HW_FAN.min)
    }
}

impl Oppsett {
    /// Oppsettet for `vifter` vifter med minste fart `min_pct`. Med
    /// `fan_per_fan` får hver vifte sin kurve fra `fan_curves` (eller
    /// `fan_curve` når den mangler); ellers én felles kurve.
    pub fn fra_values(v: &Values, vifter: usize, min_pct: f64) -> Oppsett {
        let vifter = vifter.clamp(1, MAKS_VIFTER);
        let kurver = if v.fan_per_fan { (0..vifter).map(|i| v.fan_curves.get(i).filter(|c| !c.is_empty()).unwrap_or(&v.fan_curve).clone()).collect() } else { vec![v.fan_curve.clone()] };
        Oppsett { kurver, vifter, hysterese_c: v.fan_hysteresis_c, rampe_opp_pct_s: v.fan_ramp_up_pct_s, rampe_ned_pct_s: v.fan_ramp_down_pct_s, utjevning_s: v.fan_smoothing_s, stille_til_c: v.fan_quiet_until_c, min_pct }.renset()
    }

    /// Trygt uansett hva som kom inn: tallene innenfor områdene (NaN gir
    /// standard), kurvene uten ugyldige punkter og med stigende temperatur,
    /// og en tom kurve blir standardkurven.
    pub fn renset(mut self) -> Oppsett {
        let ok = |v: f64, r: txn::Range, std: f64| if v.is_finite() { r.clamp(v) } else { std };
        self.vifter = self.vifter.clamp(1, MAKS_VIFTER);
        self.min_pct = if self.min_pct.is_finite() { self.min_pct.clamp(0.0, 100.0) } else { txn::HW_FAN.min };
        self.hysterese_c = ok(self.hysterese_c, HYSTERESE, HYSTERESE_STANDARD_C);
        self.rampe_opp_pct_s = ok(self.rampe_opp_pct_s, RAMPE, RAMPE_OPP_STANDARD);
        self.rampe_ned_pct_s = ok(self.rampe_ned_pct_s, RAMPE, RAMPE_NED_STANDARD);
        self.utjevning_s = ok(self.utjevning_s, UTJEVNING, 0.0);
        self.stille_til_c = self.stille_til_c.filter(|t| t.is_finite()).map(|t| STILLE.clamp(t));
        self.kurver.truncate(MAKS_VIFTER);
        for k in &mut self.kurver {
            k.retain(|p| p.0.is_finite() && p.1.is_finite());
            k.sort_by(|a, b| a.0.total_cmp(&b.0));
            k.dedup_by(|b, a| a.0 == b.0);
            if k.is_empty() {
                *k = DEFAULT_CURVE.to_vec();
            }
        }
        if self.kurver.is_empty() {
            self.kurver.push(DEFAULT_CURVE.to_vec());
        }
        self
    }

    /// Er det én kurve for alle viftene?
    pub fn felles(&self) -> bool {
        self.kurver.len() == 1
    }

    /// Kurven for vifte `i` (den felles når viften ikke har sin egen).
    pub fn kurve(&self, i: usize) -> &[(f64, f64)] {
        self.kurver.get(i).or(self.kurver.first()).map_or(&DEFAULT_CURVE[..], |c| c.as_slice())
    }

    /// Farten vifte `i` skal ha ved kurvetemperaturen `t` (etter utjevning og
    /// hysterese): stille flate, kurven og minste fart. Ingen rampe, ingen
    /// nødregel.
    pub fn maal(&self, i: usize, t: f64) -> f64 {
        if self.stille_til_c.is_some_and(|q| t < q) {
            return self.min_pct;
        }
        txn::curve_at(self.kurve(i), t).clamp(self.min_pct, 100.0)
    }

    /// Farten på vei ned (temperaturen faller): kurven forskjøvet
    /// `hysterese_c` mot venstre. Grafen tegner båndet mellom de to.
    pub fn maal_ned(&self, i: usize, t: f64) -> f64 {
        self.maal(i, t + self.hysterese_c)
    }
}

/// Resultatet av én runde.
#[derive(Clone, Debug, PartialEq)]
pub struct Steg {
    /// Farten som skal settes, én per vifte, `min_pct..=100`.
    pub prosent: Vec<f64>,
    /// Det kurven vil ha (før rampen), én per vifte.
    pub maal: Vec<f64>,
    /// Temperaturen etter utjevning (`None`: ukjent).
    pub temp_c: Option<f64>,
    /// Temperaturen kurven leses ved, etter hysteresen.
    pub kurve_temp_c: Option<f64>,
    /// Nødregelen gjelder (eller temperaturen har vært ukjent for lenge).
    pub nod: bool,
    /// Temperaturen kunne ikke leses denne runden.
    pub ukjent: bool,
}

impl Steg {
    /// Hele prosent, slik `nvmlDeviceSetFanSpeed_v2` vil ha dem. Aldri under
    /// minste fart, også etter avrunding.
    pub fn heltall(&self, min_pct: f64) -> Vec<u32> {
        self.prosent.iter().map(|p| p.round().max(min_pct.ceil()).clamp(0.0, 100.0) as u32).collect()
    }

    /// Den høyeste farten av viftene.
    pub fn hoyest(&self) -> f64 {
        self.prosent.iter().copied().fold(0.0, f64::max)
    }
}

/// Motoren med tilstanden mellom rundene: temperaturene til utjevningen,
/// hysteresen, farten nå og nødregelen.
#[derive(Clone, Debug)]
pub struct Motor {
    o: Oppsett,
    temps: VecDeque<(i64, f64)>,
    kurve_t: Option<f64>,
    ut: Vec<Option<f64>>,
    maal: Vec<f64>,
    sist_ms: Option<i64>,
    start_ms: Option<i64>,
    sist_kjent_ms: Option<i64>,
    nod: bool,
}

/// Høyst så mange temperaturer i utjevningen (vern mot svært tette kall).
const MAKS_TEMPS: usize = 512;

impl Motor {
    pub fn new(o: Oppsett) -> Motor {
        let o = o.renset();
        let n = o.vifter;
        Motor { o, temps: VecDeque::new(), kurve_t: None, ut: vec![None; n], maal: vec![], sist_ms: None, start_ms: None, sist_kjent_ms: None, nod: false }
    }

    /// Starter fra farten viftene har nå (rampen begynner der i stedet for å
    /// hoppe til kurven). Første steg regnes som ett sekund.
    pub fn med_start(mut self, pct: &[f64]) -> Motor {
        for (i, u) in self.ut.iter_mut().enumerate() {
            *u = pct.get(i).or(pct.last()).filter(|p| p.is_finite()).map(|p| p.clamp(self.o.min_pct, 100.0));
        }
        self
    }

    pub fn oppsett(&self) -> &Oppsett {
        &self.o
    }

    /// Nytt oppsett (ny kurve, hysterese …) uten å miste farten nå, rampen,
    /// utjevningen eller nødregelen.
    pub fn sett_oppsett(&mut self, o: Oppsett) {
        let o = o.renset();
        let last = self.ut.last().copied().flatten();
        self.ut.resize(o.vifter, last);
        self.o = o;
    }

    /// Farten nå (det forrige steget ga), én per vifte.
    pub fn prosent(&self) -> Vec<Option<f64>> {
        self.ut.clone()
    }

    pub fn nod(&self) -> bool {
        self.nod
    }

    /// Én runde. `temp_c` er GPU-temperaturen (`None` eller NaN: kunne ikke
    /// leses), `naa_ms` er klokka i millisekunder.
    pub fn steg(&mut self, temp_c: Option<f64>, naa_ms: i64) -> Steg {
        let temp = temp_c.filter(|t| t.is_finite());
        let dt = match self.sist_ms {
            Some(s) => (naa_ms - s).max(0) as f64 / 1000.0,
            None => 1.0,
        };
        self.start_ms.get_or_insert(naa_ms);
        self.sist_ms = Some(naa_ms);
        let n = self.o.vifter;
        if self.ut.len() != n {
            let last = self.ut.last().copied().flatten();
            self.ut.resize(n, last);
        }

        // 1. Nødregelen på den rå temperaturen.
        let ukjent = temp.is_none();
        match temp {
            Some(t) => {
                self.sist_kjent_ms = Some(naa_ms);
                if t >= NOD_C {
                    self.nod = true;
                } else if self.nod && t < NOD_SLIPP_C {
                    self.nod = false;
                }
            }
            None => {
                // Uten noen temperatur ennå regnes fristen fra første runde.
                let fra = self.sist_kjent_ms.or(self.start_ms).unwrap_or(naa_ms);
                if naa_ms - fra > UKJENT_FRIST_MS {
                    self.nod = true;
                }
            }
        }

        // 2. Utjevning.
        if let Some(t) = temp {
            self.temps.push_back((naa_ms, t));
        }
        let vindu = (self.o.utjevning_s * 1000.0).round() as i64;
        while let Some(&(t0, _)) = self.temps.front() {
            let gammel = if vindu > 0 { t0 <= naa_ms - vindu } else { t0 < naa_ms };
            if gammel || self.temps.len() > MAKS_TEMPS {
                self.temps.pop_front();
            } else {
                break;
            }
        }
        let jevn = if ukjent || self.temps.is_empty() { None } else { Some(self.temps.iter().map(|p| p.1).sum::<f64>() / self.temps.len() as f64) };

        // 3. Hysteresen.
        if let Some(t) = jevn {
            let h = self.o.hysterese_c;
            let k = match self.kurve_t {
                // Opp (eller første gang): straks.
                Some(k) if t >= k => t,
                // Innenfor båndet under punktet: farten står.
                Some(k) if t >= k - h => k,
                // Falt mer enn hysteresen: kurven forskjøvet hysteresen mot venstre.
                Some(_) => t + h,
                None => t,
            };
            self.kurve_t = Some(k);
        }

        // 4–5. Målet per vifte. Ukjent temperatur: det forrige målet holdes.
        if let Some(k) = jevn.and(self.kurve_t) {
            self.maal = (0..n).map(|i| self.o.maal(i, k)).collect();
        } else if self.maal.len() != n {
            // Uten mål ennå: farten viftene har (med_start), ellers full fart.
            self.maal = (0..n).map(|i| self.maal.get(i).copied().or(self.ut[i]).unwrap_or(100.0)).collect();
        }

        // 6. Rampen (eller full fart i nød).
        let min = self.o.min_pct;
        let prosent: Vec<f64> = if self.nod {
            vec![100.0; n]
        } else {
            (0..n)
                .map(|i| {
                    let m = self.maal[i];
                    match self.ut[i] {
                        None => m,
                        Some(c) if m > c => (c + self.o.rampe_opp_pct_s * dt).min(m),
                        Some(c) => (c - self.o.rampe_ned_pct_s * dt).max(m),
                    }
                    .clamp(min, 100.0)
                })
                .collect()
        };
        for (u, p) in self.ut.iter_mut().zip(&prosent) {
            *u = Some(*p);
        }
        Steg { prosent, maal: self.maal.clone(), temp_c: jevn, kurve_temp_c: jevn.and(self.kurve_t), nod: self.nod, ukjent }
    }
}

/// Kjører motoren over en rekke målinger (ms, temperatur), for eksempel
/// historikken, og gir farten for hver. Vinduet bruker den til å vise hvordan
/// viftene ville gått med et utkast. `start`: farten viftene hadde først.
pub fn simuler(o: &Oppsett, prover: &[(i64, Option<f64>)], start: Option<&[f64]>) -> Vec<Steg> {
    let mut m = Motor::new(o.clone());
    if let Some(s) = start {
        m = m.med_start(s);
    }
    prover.iter().map(|(t, temp)| m.steg(*temp, *t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Enkel kurve: 35 % til 40 °C, 100 % ved 80 °C (rett linje imellom).
    fn lin() -> Kurve {
        vec![(40.0, 35.0), (80.0, 100.0)]
    }

    fn oppsett(h: f64, opp: f64, ned: f64) -> Oppsett {
        Oppsett { kurver: vec![lin()], vifter: 2, hysterese_c: h, rampe_opp_pct_s: opp, rampe_ned_pct_s: ned, utjevning_s: 0.0, stille_til_c: None, min_pct: 35.0 }
    }

    /// Kjører en temperatur hvert sekund og gir farten til vifte 1.
    fn kjor(m: &mut Motor, temps: &[f64], fra_s: i64) -> Vec<f64> {
        temps.iter().enumerate().map(|(i, t)| m.steg(Some(*t), (fra_s + i as i64) * 1000).prosent[0]).collect()
    }

    // ------------------------------------------------------------ kurven og minimum

    #[test]
    fn uten_hysterese_og_rampe_folges_kurven() {
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0));
        let ut = kjor(&mut m, &[30.0, 50.0, 60.0, 45.0, 90.0, 40.0], 0);
        let k = lin();
        let fasit: Vec<f64> = [30.0, 50.0, 60.0, 45.0, 90.0, 40.0].iter().map(|t| txn::curve_at(&k, *t).max(35.0)).collect();
        // 90 °C er over nødregelen: 100 %.
        assert_eq!(ut[..4], fasit[..4]);
        assert_eq!(ut[4], 100.0);
    }

    #[test]
    fn under_minimum_klemmes_til_minimum() {
        let o = Oppsett { kurver: vec![vec![(20.0, 0.0), (60.0, 10.0), (70.0, 20.0)]], ..oppsett(0.0, 100.0, 100.0) };
        let mut m = Motor::new(o);
        for t in [20.0, 50.0, 65.0, 75.0] {
            let s = m.steg(Some(t), 0);
            assert!(s.prosent.iter().all(|p| *p == 35.0), "{t}: {:?}", s.prosent);
            assert!(s.maal.iter().all(|p| *p == 35.0));
        }
        // Minste fart fra kortet gjelder, også et annet tall.
        let o = Oppsett { min_pct: 30.0, ..oppsett(0.0, 100.0, 100.0) };
        assert_eq!(Motor::new(o).steg(Some(20.0), 0).prosent, vec![35.0, 35.0], "kurven starter på 35");
        let o = Oppsett { min_pct: 45.0, ..oppsett(0.0, 100.0, 100.0) };
        assert_eq!(Motor::new(o).steg(Some(20.0), 0).prosent, vec![45.0, 45.0]);
    }

    #[test]
    fn heltall_gaar_aldri_under_minimum() {
        let s = Steg { prosent: vec![35.0, 99.6, 34.6], maal: vec![], temp_c: None, kurve_temp_c: None, nod: false, ukjent: false };
        assert_eq!(s.heltall(35.0), vec![35, 100, 35]);
        assert_eq!(s.hoyest(), 99.6);
    }

    // ------------------------------------------------------------ hysterese

    #[test]
    fn hysterese_opp_straks_ned_forst_etter_fallet() {
        let mut m = Motor::new(oppsett(3.0, 100.0, 100.0));
        // Opp til 60 °C: straks (rampen er 100 %/s).
        let opp = kjor(&mut m, &[50.0, 60.0], 0);
        assert_eq!(opp, vec![txn::curve_at(&lin(), 50.0), txn::curve_at(&lin(), 60.0)]);
        let ved60 = opp[1];
        // Ned 1, 2 og 3 °C: farten står (hysteresen er 3 °C).
        let ned = kjor(&mut m, &[59.0, 58.0, 57.0], 2);
        assert_eq!(ned, vec![ved60; 3]);
        // 4 °C under: ned, men bare til kurven 3 °C over temperaturen.
        let s = m.steg(Some(56.0), 5000);
        assert_eq!(s.kurve_temp_c, Some(59.0));
        assert_eq!(s.prosent[0], txn::curve_at(&lin(), 59.0));
        // Opp igjen 1 °C: står (fortsatt innenfor båndet).
        assert_eq!(m.steg(Some(57.0), 6000).prosent[0], txn::curve_at(&lin(), 59.0));
        // Over punktet: opp straks.
        assert_eq!(m.steg(Some(61.0), 7000).prosent[0], txn::curve_at(&lin(), 61.0));
    }

    #[test]
    fn hysterese_hindrer_pendling_rundt_en_temperatur() {
        // Temperaturen svinger ±1 °C rundt 60: med 3 °C hysterese står viftene.
        let mut m = Motor::new(oppsett(3.0, 100.0, 100.0));
        let ut = kjor(&mut m, &[61.0, 59.0, 61.0, 59.0, 60.0, 61.0, 59.0], 0);
        assert!(ut.iter().all(|p| *p == ut[0]), "{ut:?}");
        // Uten hysterese følger farten hver svingning.
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0));
        let ut = kjor(&mut m, &[61.0, 59.0, 61.0, 59.0], 0);
        assert!(ut[0] > ut[1] && ut[2] > ut[3]);
    }

    #[test]
    fn nedkurven_er_kurven_forskjovet() {
        let o = oppsett(3.0, 10.0, 3.0);
        assert_eq!(o.maal_ned(0, 57.0), o.maal(0, 60.0));
        assert!(o.maal_ned(0, 60.0) >= o.maal(0, 60.0));
    }

    // ------------------------------------------------------------ rampe

    #[test]
    fn rampe_opp_og_ned_hver_for_seg() {
        let mut m = Motor::new(oppsett(0.0, 10.0, 3.0)).med_start(&[35.0, 35.0]);
        // Hopp til 80 °C (100 %): høyst 10 %/s opp.
        let opp = kjor(&mut m, &[80.0, 80.0, 80.0, 80.0], 0);
        assert_eq!(opp, vec![45.0, 55.0, 65.0, 75.0]);
        let ut = kjor(&mut m, &[80.0; 4], 4);
        assert_eq!(ut, vec![85.0, 95.0, 100.0, 100.0]);
        // Ned til 40 °C (35 %): høyst 3 %/s.
        let ned = kjor(&mut m, &[40.0, 40.0, 40.0], 8);
        assert_eq!(ned, vec![97.0, 94.0, 91.0]);
    }

    #[test]
    fn rampen_regner_med_tiden_mellom_rundene() {
        let mut m = Motor::new(oppsett(0.0, 10.0, 3.0)).med_start(&[35.0]);
        assert_eq!(m.steg(Some(80.0), 0).prosent[0], 45.0, "første steg er ett sekund");
        assert_eq!(m.steg(Some(80.0), 500).prosent[0], 50.0, "et halvt sekund");
        assert_eq!(m.steg(Some(80.0), 2500).prosent[0], 70.0, "to sekunder");
        // Klokka går bakover: ingen bevegelse, ingen panikk.
        assert_eq!(m.steg(Some(80.0), 1000).prosent[0], 70.0);
    }

    #[test]
    fn uten_startfart_hopper_forste_steg_til_kurven() {
        let mut m = Motor::new(oppsett(0.0, 1.0, 1.0));
        assert_eq!(m.steg(Some(60.0), 0).prosent[0], txn::curve_at(&lin(), 60.0));
    }

    // ------------------------------------------------------------ utjevning

    #[test]
    fn utjevning_tar_bort_korte_topper() {
        let o = Oppsett { utjevning_s: 5.0, ..oppsett(0.0, 100.0, 100.0) };
        let mut m = Motor::new(o);
        // Fem sekunder på 50 °C, så én topp på 75 °C.
        let mut last = None;
        for (i, t) in [50.0, 50.0, 50.0, 50.0, 50.0, 75.0].iter().enumerate() {
            last = Some(m.steg(Some(*t), i as i64 * 1000));
        }
        let s = last.unwrap();
        assert_eq!(s.temp_c, Some(55.0), "snittet av 50, 50, 50, 50 og 75");
        assert!(s.prosent[0] < txn::curve_at(&lin(), 60.0));
        // Uten utjevning: rett til toppen.
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0));
        assert_eq!(m.steg(Some(75.0), 0).temp_c, Some(75.0));
    }

    #[test]
    fn utjevningen_glemmer_det_som_er_eldre_enn_vinduet() {
        let o = Oppsett { utjevning_s: 3.0, ..oppsett(0.0, 100.0, 100.0) };
        let mut m = Motor::new(o);
        m.steg(Some(80.0), 0);
        m.steg(Some(50.0), 1000);
        m.steg(Some(50.0), 2000);
        assert_eq!(m.steg(Some(50.0), 3000).temp_c, Some(50.0), "80 °C ved 0 s er ute av vinduet ved 3 s");
    }

    #[test]
    fn noden_venter_ikke_paa_utjevningen() {
        let o = Oppsett { utjevning_s: 10.0, ..oppsett(3.0, 5.0, 2.0) };
        let mut m = Motor::new(o).med_start(&[35.0, 35.0]);
        for i in 0..10 {
            m.steg(Some(50.0), i * 1000);
        }
        let s = m.steg(Some(86.0), 10_000);
        assert!(s.nod);
        assert_eq!(s.prosent, vec![100.0, 100.0]);
        assert!(s.temp_c.unwrap() < 60.0, "snittet er fortsatt lavt");
    }

    // ------------------------------------------------------------ stille flate

    #[test]
    fn stille_flate_holder_minimum_til_temperaturen() {
        let o = Oppsett { kurver: vec![vec![(30.0, 40.0), (60.0, 60.0), (80.0, 100.0)]], stille_til_c: Some(55.0), ..oppsett(0.0, 100.0, 100.0) };
        assert_eq!(o.maal(0, 30.0), 35.0);
        assert_eq!(o.maal(0, 54.0), 35.0);
        assert_eq!(o.maal(0, 55.0), txn::curve_at(&o.kurver[0], 55.0));
        let mut m = Motor::new(o);
        assert_eq!(kjor(&mut m, &[40.0, 54.0, 60.0], 0), vec![35.0, 35.0, 60.0]);
    }

    #[test]
    fn stille_flate_og_hysterese_sammen() {
        let o = Oppsett { kurver: vec![vec![(30.0, 40.0), (60.0, 60.0), (80.0, 100.0)]], stille_til_c: Some(50.0), ..oppsett(3.0, 100.0, 100.0) };
        let mut m = Motor::new(o);
        let ut = kjor(&mut m, &[51.0, 49.0, 48.0, 47.0, 46.0], 0);
        // Over flaten ved 51; ned til 48 holdes farten; ved 47 er kurvetemperaturen 50 (ikke under flaten),
        // ved 46 er den 49: minimum.
        assert!(ut[0] > 35.0);
        assert_eq!(ut[1], ut[0]);
        assert_eq!(ut[2], ut[0]);
        assert!(ut[3] > 35.0);
        assert_eq!(ut[4], 35.0);
    }

    // ------------------------------------------------------------ nødregelen

    #[test]
    fn nodregelen_full_fart_med_5_grader_hysterese() {
        let mut m = Motor::new(oppsett(0.0, 2.0, 1.0)).med_start(&[40.0, 40.0]);
        assert!(!m.steg(Some(84.0), 0).nod);
        let s = m.steg(Some(85.0), 1000);
        assert!(s.nod && s.prosent == vec![100.0, 100.0], "straks, uten rampe");
        for (i, t) in [84.0, 82.0, 80.5].iter().enumerate() {
            let s = m.steg(Some(*t), 2000 + i as i64 * 1000);
            assert!(s.nod, "{t}");
            assert_eq!(s.prosent, vec![100.0, 100.0]);
        }
        let s = m.steg(Some(79.0), 5000);
        assert!(!s.nod);
        // Tilbake til kurven med rampen ned fra 100 %.
        assert_eq!(s.prosent[0], 99.0);
    }

    #[test]
    fn nodregelen_kan_ikke_slaas_av() {
        // Selv en kurve som er flat på minimum og treg rampe gir 100 % over 85 °C.
        let o = Oppsett { kurver: vec![vec![(20.0, 35.0), (100.0, 35.0)]], stille_til_c: Some(80.0), ..oppsett(10.0, 1.0, 1.0) };
        let mut m = Motor::new(o).med_start(&[35.0, 35.0]);
        assert_eq!(m.steg(Some(90.0), 0).prosent, vec![100.0, 100.0]);
        // Fra Values: det finnes ikke noe felt som slår den av.
        let v: Values = serde_json::from_value(json!({"fanEmergency": false, "fanCurve": [[20, 35], [100, 35]]})).unwrap();
        let mut m = Motor::new(Oppsett::fra_values(&v, 2, 35.0));
        assert!(m.steg(Some(95.0), 0).nod);
    }

    #[test]
    fn ukjent_temperatur_holder_farten_og_gaar_til_full_etter_fristen() {
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0));
        let f = m.steg(Some(60.0), 0).prosent[0];
        let s = m.steg(None, 1000);
        assert!(s.ukjent && !s.nod);
        assert_eq!(s.prosent[0], f);
        assert_eq!(m.steg(Some(f64::NAN), 5000).prosent[0], f, "NaN er også ukjent");
        let s = m.steg(None, 5001);
        assert!(s.nod);
        assert_eq!(s.prosent, vec![100.0, 100.0]);
        // Lesbar igjen (under 80 °C): ut av nød.
        assert!(!m.steg(Some(60.0), 6000).nod);
        // Aldri lest: fristen regnes fra første runde, og farten er full.
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0));
        assert_eq!(m.steg(None, 0).prosent, vec![100.0, 100.0]);
        assert!(!m.steg(None, 5000).nod);
        assert!(m.steg(None, 5001).nod);
        // Med startfart holdes den i mellomtiden.
        let mut m = Motor::new(oppsett(0.0, 100.0, 100.0)).med_start(&[50.0, 40.0]);
        assert_eq!(m.steg(None, 0).prosent, vec![50.0, 40.0]);
    }

    // ------------------------------------------------------------ per vifte

    #[test]
    fn egen_kurve_per_vifte() {
        let v = Values { fan_per_fan: true, fan_curves: vec![vec![(30.0, 35.0), (70.0, 60.0)], vec![(30.0, 50.0), (70.0, 90.0)]], ..Default::default() };
        let o = Oppsett::fra_values(&v, 2, 35.0);
        assert!(!o.felles());
        let mut m = Motor::new(Oppsett { hysterese_c: 0.0, rampe_opp_pct_s: 100.0, rampe_ned_pct_s: 100.0, ..o });
        let s = m.steg(Some(50.0), 0);
        assert_eq!(s.prosent, vec![47.5, 70.0]);
    }

    #[test]
    fn felles_kurve_er_standard_og_mangler_faller_tilbake() {
        let v = Values::default();
        let o = Oppsett::fra_values(&v, 2, 35.0);
        assert!(o.felles());
        assert_eq!(o.kurve(1), &txn::DEFAULT_CURVE[..]);
        // Per vifte, men bare én kurve: vifte 2 får den felles.
        let v = Values { fan_per_fan: true, fan_curves: vec![vec![(30.0, 60.0), (80.0, 90.0)]], ..Default::default() };
        let o = Oppsett::fra_values(&v, 2, 35.0);
        assert_eq!(o.kurve(0), &[(30.0, 60.0), (80.0, 90.0)][..]);
        assert_eq!(o.kurve(1), &txn::DEFAULT_CURVE[..]);
        // Kurvene ignoreres når per vifte er av.
        let v = Values { fan_per_fan: false, fan_curves: vec![vec![(30.0, 60.0), (80.0, 90.0)]; 2], ..Default::default() };
        assert!(Oppsett::fra_values(&v, 2, 35.0).felles());
    }

    #[test]
    fn rampen_gjelder_hver_vifte() {
        let v = Values { fan_per_fan: true, fan_curves: vec![vec![(30.0, 35.0), (80.0, 40.0)], vec![(30.0, 35.0), (80.0, 100.0)]], ..Default::default() };
        let o = Oppsett { hysterese_c: 0.0, rampe_opp_pct_s: 10.0, ..Oppsett::fra_values(&v, 2, 35.0) };
        let mut m = Motor::new(o).med_start(&[35.0, 35.0]);
        let s = m.steg(Some(80.0), 0);
        assert_eq!(s.prosent, vec![40.0, 45.0]);
    }

    #[test]
    fn nytt_oppsett_beholder_farten() {
        let mut m = Motor::new(oppsett(0.0, 10.0, 3.0)).med_start(&[35.0, 35.0]);
        kjor(&mut m, &[80.0, 80.0], 0);
        assert_eq!(m.prosent(), vec![Some(55.0), Some(55.0)]);
        m.sett_oppsett(Oppsett { vifter: 3, ..oppsett(0.0, 10.0, 3.0) });
        assert_eq!(m.prosent().len(), 3);
        assert_eq!(m.steg(Some(80.0), 2000).prosent, vec![65.0; 3]);
    }

    // ------------------------------------------------------------ oppsettet er trygt

    #[test]
    fn rensing_av_oppsettet() {
        let o = Oppsett { kurver: vec![vec![], vec![(60.0, f64::NAN), (70.0, 50.0), (50.0, 40.0), (70.0, 60.0)]], vifter: 99, hysterese_c: f64::NAN, rampe_opp_pct_s: 0.0, rampe_ned_pct_s: 1e9, utjevning_s: -5.0, stille_til_c: Some(10.0), min_pct: f64::INFINITY }.renset();
        assert_eq!(o.vifter, MAKS_VIFTER);
        assert_eq!(o.kurver[0], DEFAULT_CURVE.to_vec());
        assert_eq!(o.kurver[1], vec![(50.0, 40.0), (70.0, 50.0)]);
        assert_eq!((o.hysterese_c, o.rampe_opp_pct_s, o.rampe_ned_pct_s, o.utjevning_s), (HYSTERESE_STANDARD_C, 1.0, 100.0, 0.0));
        assert_eq!(o.stille_til_c, Some(30.0));
        assert_eq!(o.min_pct, 35.0);
        assert_eq!(Oppsett { kurver: vec![], ..Oppsett::default() }.renset().kurver.len(), 1);
    }

    #[test]
    fn standardoppsettet_fra_values() {
        let o = Oppsett::default();
        assert_eq!((o.hysterese_c, o.rampe_opp_pct_s, o.rampe_ned_pct_s, o.utjevning_s, o.stille_til_c), (3.0, 10.0, 3.0, 0.0, None));
        assert_eq!(o.vifter, 2);
    }

    #[test]
    fn simuler_over_historikk() {
        let o = oppsett(0.0, 10.0, 3.0);
        let p: Vec<(i64, Option<f64>)> = (0..10).map(|i| (i * 1000, Some(if i < 5 { 40.0 } else { 80.0 }))).collect();
        let s = simuler(&o, &p, Some(&[35.0, 35.0]));
        assert_eq!(s.len(), 10);
        assert_eq!(s[4].prosent[0], 35.0);
        assert_eq!(s[5].prosent[0], 45.0);
        assert_eq!(s[9].prosent[0], 85.0);
    }

    // ------------------------------------------------------------ egenskaper (tilfeldige runder)

    /// Liten xorshift, så testene er like hver gang uten nye avhengigheter.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn range(&mut self, a: f64, b: f64) -> f64 {
            a + (b - a) * self.next()
        }
    }

    fn tilfeldig_oppsett(r: &mut Rng) -> Oppsett {
        let mut kurve = Vec::new();
        let n = 2 + (r.next() * 9.0) as usize;
        let mut t = r.range(20.0, 40.0);
        let mut f = r.range(0.0, 50.0);
        for _ in 0..n {
            kurve.push((t.round(), f.round()));
            t += r.range(1.0, 8.0);
            f = (f + r.range(0.0, 15.0)).min(100.0);
        }
        let mut k2 = kurve.clone();
        for p in &mut k2 {
            p.1 = (p.1 + 10.0).min(100.0);
        }
        Oppsett {
            kurver: if r.next() < 0.5 { vec![kurve] } else { vec![kurve, k2] },
            vifter: 2,
            hysterese_c: r.range(0.0, 10.0).round(),
            rampe_opp_pct_s: r.range(1.0, 100.0).round(),
            rampe_ned_pct_s: r.range(1.0, 100.0).round(),
            utjevning_s: [0.0, 3.0, 5.0, 10.0][(r.next() * 4.0) as usize % 4],
            stille_til_c: (r.next() < 0.4).then(|| r.range(30.0, 80.0).round()),
            min_pct: 35.0,
        }
    }

    #[test]
    fn egenskap_alltid_innenfor_minimum_og_full_fart() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..300 {
            let o = tilfeldig_oppsett(&mut r);
            let mut m = Motor::new(o.clone()).med_start(&[r.range(35.0, 100.0), r.range(35.0, 100.0)]);
            let mut t = r.range(30.0, 70.0);
            let mut now = 0i64;
            for _ in 0..120 {
                now += r.range(200.0, 2500.0) as i64;
                t = (t + r.range(-4.0, 4.0)).clamp(20.0, 100.0);
                let temp = if r.next() < 0.03 { None } else { Some(t) };
                let s = m.steg(temp, now);
                for p in &s.prosent {
                    assert!((35.0..=100.0).contains(p), "{p} med {o:?}");
                }
                if temp.is_some_and(|t| t >= NOD_C) {
                    assert!(s.nod && s.prosent.iter().all(|p| *p == 100.0));
                }
            }
        }
    }

    #[test]
    fn egenskap_rampen_holdes_utenom_nod() {
        let mut r = Rng(0x1234_5678_9abc_def1);
        for _ in 0..300 {
            let o = Oppsett { stille_til_c: None, ..tilfeldig_oppsett(&mut r) };
            let mut m = Motor::new(o.clone()).med_start(&[50.0, 50.0]);
            let mut forrige = vec![50.0, 50.0];
            let mut var_nod = false;
            let mut now = 0i64;
            let mut t = 50.0;
            for k in 0..100 {
                let dt = r.range(100.0, 2000.0) as i64;
                now += dt;
                t = (t + r.range(-6.0, 6.0)).clamp(20.0, 84.0);
                let s = m.steg(Some(t), now);
                // Første steg regnes som ett sekund (det finnes ingen forrige tid).
                if k > 0 && !s.nod && !var_nod {
                    for (p, f) in s.prosent.iter().zip(&forrige) {
                        let d = p - f;
                        let sek = dt as f64 / 1000.0;
                        assert!(d <= o.rampe_opp_pct_s * sek + 1e-9, "opp {d} på {sek} s, {o:?}");
                        assert!(-d <= o.rampe_ned_pct_s * sek + 1e-9, "ned {d} på {sek} s, {o:?}");
                    }
                }
                var_nod = s.nod;
                forrige = s.prosent.clone();
            }
        }
    }

    #[test]
    fn egenskap_varmere_gir_aldri_lavere_fart_i_ro() {
        // Uten rampe og hysterese er farten en ikke-fallende funksjon av temperaturen.
        let mut r = Rng(0xdead_beef_cafe_f00d);
        for _ in 0..200 {
            let o = Oppsett { hysterese_c: 0.0, rampe_opp_pct_s: 100.0, rampe_ned_pct_s: 100.0, utjevning_s: 0.0, ..tilfeldig_oppsett(&mut r) };
            let mut forrige = 0.0;
            for t in 20..100 {
                let mut m = Motor::new(o.clone());
                let p = m.steg(Some(t as f64), 0).prosent[0];
                assert!(p >= forrige, "{t} °C: {p} < {forrige}, {o:?}");
                forrige = p;
            }
        }
    }

    #[test]
    fn egenskap_farten_stopper_paa_maalet() {
        // Lik temperatur lenge nok: farten ender på målet, uansett start.
        let mut r = Rng(0x0bad_cafe_1234_5678);
        for _ in 0..200 {
            let o = tilfeldig_oppsett(&mut r);
            let t = r.range(25.0, 84.0).round();
            let mut m = Motor::new(o.clone()).med_start(&[r.range(35.0, 100.0), r.range(35.0, 100.0)]);
            let mut s = m.steg(Some(t), 0);
            for i in 1..200 {
                s = m.steg(Some(t), i * 1000);
            }
            for (i, p) in s.prosent.iter().enumerate() {
                assert_eq!(*p, o.maal(i, t), "{o:?} ved {t}");
            }
        }
    }

    #[test]
    fn egenskap_hysteresen_holder_farten_innenfor_baandet() {
        // Temperaturen går opp og ned innenfor hysteresen etter et toppunkt:
        // farten endres ikke (rampe 100 %/s, ingen utjevning).
        let mut r = Rng(0x5555_aaaa_3333_cccc);
        for _ in 0..200 {
            let h = r.range(1.0, 10.0).round();
            let o = Oppsett { hysterese_c: h, rampe_opp_pct_s: 100.0, rampe_ned_pct_s: 100.0, utjevning_s: 0.0, stille_til_c: None, ..tilfeldig_oppsett(&mut r) };
            let topp = r.range(40.0, 80.0).round();
            let mut m = Motor::new(o);
            let f = m.steg(Some(topp), 0).prosent.clone();
            for i in 1..30 {
                let t = topp - r.range(0.0, h);
                assert_eq!(m.steg(Some(t), i * 1000).prosent, f);
            }
        }
    }
}
