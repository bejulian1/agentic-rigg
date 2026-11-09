//! Kontroller (produktspesifikasjonen 6.4), redesignet i runde 6C som en
//! overklokkingsflate i appens rolige stil:
//!
//! - **Øverst** de store tallene: kjerneklokke, minneklokke, temperatur,
//!   effekt (W og % av grensen), last, vifte, P-tilstand og hva som
//!   begrenser kortet nå (fra NVMLs klokkebegrensninger).
//! - **Klokker og effekt:** én rad per kontroll med verdi, enhet, område,
//!   standardmerke, Tilbakestill, en kort forklaring, nå-linje og (i).
//!   «Tilbakestill alt til standard» står i panelhodet.
//! - **Til høyre:** utkastet, profilene, begrensningene som liste (som
//!   HWiNFO) og det kortet ikke kan.
//! - **Under:** kjerneklokke, temperatur og effekt for de siste minuttene.
//!
//! Endringer er låst mot det ekte kortet. Glidere og tallfelt lager et
//! utkast som lagres i appen og ikke sendes. Mot den simulerte backenden i
//! en utviklerforekomst virker Bruk, Behold og Rull tilbake.
//!
//! **Eies av 6C (Styring).**

use super::{Key, Page};
use crate::state::App;
use crate::widgets::chart::{self, ChartSet, Cols, DefOpts};
use crate::widgets::locked::{self, Columns, ConfirmBar, LockStrip, signed};
use crate::widgets::oc::{OcRow, Stat, StatStrip, info_button, info_content};
use crate::widgets::rows::{KvList, tag};
use crate::widgets::{Icon, Panel, Seg, button, class, hbox, label, set_text, tip, vbox};
use gtk::prelude::*;
use gtk::{gdk, glib};
use rigg_core::fmt as F;
use rigg_core::txn::{self, FanMode, Gate, Values};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Skjermbilder for 6C: bare med `RIGG_PROFIL=dev` og `RIGG_BILDER_2C`.
#[path = "styring_bilder.rs"]
pub mod bilder;

// ================================================================ det som er lest

/// Alt Kontroller og Viftekurve viser, lest fra prøven, de faste dataene og
/// `extra.styring` (offset, temperaturmålets område, viftemodus og mål-fart).
#[derive(Clone, Debug, Default)]
pub struct Read {
    pub t: i64,
    pub power: Option<f64>,
    pub limit: Option<f64>,
    pub p_min: Option<f64>,
    pub p_max: Option<f64>,
    pub p_def: Option<f64>,
    pub core_off: Option<f64>,
    pub core_rng: Option<(f64, f64)>,
    pub mem_off: Option<f64>,
    pub mem_rng: Option<(f64, f64)>,
    pub temp_target: Option<f64>,
    pub temp_rng: Option<(f64, f64)>,
    pub slowdown: Option<f64>,
    pub shutdown: Option<f64>,
    pub fan_policy: Option<Vec<u64>>,
    pub fans: Vec<Option<f64>>,
    pub rpm: Vec<Option<f64>>,
    pub fan_target: Vec<f64>,
    pub fan_min: Option<f64>,
    pub temp: Option<f64>,
    pub core: Option<f64>,
    pub mem: Option<f64>,
    pub core_max: Option<f64>,
    pub mem_max: Option<f64>,
    pub util: Option<f64>,
    pub mem_util: Option<f64>,
    pub pstate: Option<String>,
    pub throttle: Option<u64>,
    pub inferred: bool,
    /// Kortet svarer ikke (`lost`): det som leses fra kortet, står som «–».
    pub lost: bool,
}

fn range_of(v: &Value) -> Option<(f64, f64)> {
    Some((v["min"].as_f64()?, v["max"].as_f64()?))
}

pub fn read(app: &App) -> Read {
    let s = app.gpu_sample().unwrap_or_default();
    let st = app.statics().unwrap_or_default();
    let lost = app.gpu_info().is_some_and(|i| i.state == rigg_core::model::DeviceState::Lost);
    // Når kortet svarer ikke, er verdiene fra de faste dataene gamle: bare
    // grensene (område, standard) står igjen.
    let none = Value::Null;
    let sx = &st.extra["styring"];
    let x = if lost { &none } else { sx };
    Read {
        t: app.telemetry().map(|t| t.t).unwrap_or(0),
        power: s.power,
        limit: s.power_limit,
        p_min: st.power_min,
        p_max: st.power_max,
        p_def: st.power_default,
        core_off: x["coreOffsetMHz"]["value"].as_f64(),
        core_rng: range_of(&sx["coreOffsetMHz"]),
        mem_off: x["memOffsetMHz"]["value"].as_f64(),
        mem_rng: range_of(&sx["memOffsetMHz"]),
        temp_target: x["tempTarget"]["value"].as_f64().or(st.temp_target.filter(|_| !lost)),
        temp_rng: range_of(&sx["tempTarget"]),
        slowdown: st.temp_slowdown,
        shutdown: st.temp_shutdown,
        // Prøven (hvert 5. s) går foran de faste dataene, som bare leses ved oppstart.
        fan_policy: if s.fan_policy.iter().any(Option::is_some) {
            Some(s.fan_policy.iter().flatten().map(|p| u64::from(*p)).collect())
        } else {
            x["fanPolicy"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect())
        },
        fans: s.fans.clone(),
        rpm: s.fan_rpm.clone(),
        fan_target: if s.fan_target.iter().any(Option::is_some) {
            s.fan_target.iter().flatten().copied().collect()
        } else {
            x["fanTarget"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
        },
        fan_min: st.fan_min,
        temp: s.temp,
        core: s.core_clock,
        mem: s.mem_clock,
        core_max: st.core_max,
        mem_max: st.mem_max,
        util: s.gpu_util,
        mem_util: s.mem_util,
        pstate: s.pstate.clone(),
        throttle: s.throttle,
        inferred: s.throttle_inferred,
        lost,
    }
}

impl Read {
    /// Verdiene som et utkast starter fra.
    pub fn values(&self) -> Values {
        let d = Values::default();
        let auto = self.fan_policy.as_ref().is_none_or(|p| p.iter().all(|x| *x == 0));
        Values {
            power_limit_w: self.limit.or(self.p_def).unwrap_or(d.power_limit_w).round(),
            core_offset_mhz: self.core_off.unwrap_or(0.0),
            mem_offset_mhz: self.mem_off.unwrap_or(0.0),
            temp_target_c: self.temp_target.unwrap_or(d.temp_target_c),
            fan_mode: if auto { FanMode::Auto } else { FanMode::Manual },
            fan_percent: self.fan_max_now().unwrap_or(50.0).round().max(self.floor()),
            fan_curve: txn::DEFAULT_CURVE.to_vec(),
            // Hysterese, rampe og resten av viftemotoren (7B): standard.
            ..d
        }
    }

    /// Minste viftefart (VBIOS): 35 % på dette kortet.
    pub fn floor(&self) -> f64 {
        self.fan_min.unwrap_or(txn::HW_FAN.min)
    }

    /// «Auto · driveren styrer», som modusvelgeren i demoen.
    pub fn fan_mode_text(&self) -> String {
        match &self.fan_policy {
            None => "–".into(),
            Some(p) if p.iter().all(|x| *x == 0) => "Auto · driveren styrer".into(),
            Some(_) => "Manuell · satt utenfor appen".into(),
        }
    }

    pub fn fan_max_now(&self) -> Option<f64> {
        self.fans.iter().flatten().cloned().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))))
    }

    /// «35 % · 812 o/min»
    pub fn fan_text(&self, i: usize) -> String {
        let pct = F::pct(self.fans.get(i).cloned().flatten(), 0);
        match self.rpm.get(i).cloned().flatten() {
            Some(r) => format!("{pct} · {}", F::rpm(Some(r))),
            None => pct,
        }
    }

    /// «Vifte 1 35 % · 802 o/min · vifte 2 35 % · 798 o/min», uten brudd i «o/min».
    pub fn fans_line(&self) -> String {
        let s = (0..self.fans.len()).map(|i| format!("vifte {} {}", i + 1, self.fan_text(i))).collect::<Vec<_>>().join(", ");
        capitalize(&s).replace("o/min", "o\u{2060}/\u{2060}min")
    }
}

/// Kortets standard: effektgrensen kortet melder (245 W), ingen offset,
/// 83 °C, driveren styrer viften og Balansert-kurven.
pub fn stock(r: &Read) -> Values {
    let d = Values::default();
    Values { power_limit_w: r.p_def.unwrap_or(d.power_limit_w).round(), ..d }
}

/// Det som kan tilbakestilles hver for seg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Power,
    Core,
    Mem,
    Temp,
    Fan,
    Curve,
}

/// Én verdi tilbake til standard.
pub fn reset_field(v: &mut Values, f: Field, st: &Values) {
    match f {
        Field::Power => v.power_limit_w = st.power_limit_w,
        Field::Core => v.core_offset_mhz = st.core_offset_mhz,
        Field::Mem => v.mem_offset_mhz = st.mem_offset_mhz,
        Field::Temp => v.temp_target_c = st.temp_target_c,
        Field::Fan => v.fan_mode = st.fan_mode,
        Field::Curve => v.fan_curve = st.fan_curve.clone(),
    }
}

/// Alt til standard. Den manuelle farten huskes (den brukes ikke i Auto).
pub fn reset_all(v: &Values, st: &Values) -> Values {
    Values { fan_percent: v.fan_percent, ..st.clone() }
}

/// «245 W · kjerne +0 MHz · minne +0 MHz · mål 83 °C · vifte auto»
pub fn summary(v: &Values) -> String {
    let fan = match v.fan_mode {
        FanMode::Auto => "vifte auto".to_string(),
        FanMode::Curve => "vifte kurve".to_string(),
        FanMode::Manual => format!("vifte {}", F::pct(Some(v.fan_percent), 0)),
    };
    format!("{} · kjerne {} · minne {} · mål {} · {fan}", F::watt(Some(v.power_limit_w), 0), signed(v.core_offset_mhz, "MHz"), signed(v.mem_offset_mhz, "MHz"), F::temp(Some(v.temp_target_c), 0))
}

/// Navn og verdi som tekst for endringslista.
fn field_text(v: &Values, field: &str) -> (&'static str, String) {
    match field {
        "powerLimitW" => ("Effektgrense", F::watt(Some(v.power_limit_w), 0)),
        "clockOffsets.coreMhz" => ("Kjerneklokke-offset", signed(v.core_offset_mhz, "MHz")),
        "clockOffsets.memMhz" => ("Minneklokke-offset", signed(v.mem_offset_mhz, "MHz")),
        "tempTargetC" => ("Temperaturmål", F::temp(Some(v.temp_target_c), 0)),
        "fan.mode" => ("Viftemodus", mode_name(v.fan_mode).into()),
        "fan.manualPercent" => ("Viftefart (manuell)", F::pct(Some(v.fan_percent), 0)),
        // Viftemotoren (runde 7B).
        "fan.perFan" => ("Viftekurver", if v.fan_per_fan { "per vifte" } else { "felles" }.into()),
        "fan.curves" => ("Kurvene per vifte", v.fan_curves.iter().map(|c| format!("{} punkter", c.len())).collect::<Vec<_>>().join(" · ")),
        "fan.hysteresisC" => ("Hysterese", F::temp_delta(v.fan_hysteresis_c, 0)),
        "fan.rampUpPctS" => ("Rampe opp", format!("{}{}%/s", v.fan_ramp_up_pct_s, F::NNBSP)),
        "fan.rampDownPctS" => ("Rampe ned", format!("{}{}%/s", v.fan_ramp_down_pct_s, F::NNBSP)),
        "fan.smoothingS" => ("Utjevning", if v.fan_smoothing_s > 0.0 { format!("{}{}s", v.fan_smoothing_s, F::NNBSP) } else { "av".into() }),
        "fan.quietUntilC" => ("Hold minimum til", v.fan_quiet_until_c.map(|t| F::temp(Some(t), 0)).unwrap_or_else(|| "av".into())),
        _ => ("Viftekurve", format!("{} punkter", v.fan_curve.len())),
    }
}

pub fn mode_name(m: FanMode) -> &'static str {
    match m {
        FanMode::Auto => "Auto",
        FanMode::Curve => "Kurve",
        FanMode::Manual => "Manuell",
    }
}

/// Endringene i utkastet mot det kortet har nå (manuell fart bare i Manuell).
pub fn draft_diff(read: &Values, draft: &Values) -> Vec<String> {
    // Kurven og viftemotoren (7B) betyr bare noe i modus Kurve.
    let curve_field = |f: &str| f.starts_with("fan.") && f != "fan.mode" && f != "fan.manualPercent";
    let hidden = |f: &str| (f == "fan.manualPercent" && draft.fan_mode != FanMode::Manual) || (curve_field(f) && draft.fan_mode != FanMode::Curve && read.fan_mode != FanMode::Curve);
    read.diff(draft).into_iter().filter(|f| !hidden(f)).collect()
}

/// Klokkebegrensningene (`nvmlClocksEventReason*`): bit, navn og forklaring.
pub const THROTTLE: [(u64, &str, &str); 9] = [
    (0x1, "Tomgang", "Kortet har ingenting å gjøre og holder klokkene nede."),
    (0x2, "Programklokker", "Klokkene er låst av et program (applications clocks)."),
    (0x4, "Effektgrense", "Kortet trekker så mye som effektgrensen tillater."),
    (0x8, "Maskinvare-nedklokking", "Kortet klokker ned av seg selv (strøm eller temperatur)."),
    (0x10, "Synkronisert boost", "Klokkene følger et annet kort."),
    (0x20, "Temperaturmål", "Kortet har nådd temperaturmålet og senker klokkene."),
    (0x40, "Termisk nedklokking", "Maskinvaren klokker ned fordi kortet er for varmt."),
    (0x80, "Effektbrems", "Strømforsyningen har bedt kortet bremse."),
    (0x100, "Skjermklokke", "Klokkene er satt for skjermen."),
];

/// Det som begrenser kortet nå, som én flis: navn, linje under, tone og tips.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub name: &'static str,
    pub sub: String,
    /// «», «accent» eller «warn».
    pub tone: &'static str,
    pub tip: String,
}

/// Last over dette uten noen begrensning fra NVML: kortet står på toppen av
/// klokkekurven, altså spenningsgrensen (utledet, ikke lest).
const VOLTAGE_LOAD: f64 = 60.0;

/// Den viktigste begrensningen nå. Rekkefølgen: maskinvaren (temperatur,
/// effektbrems, nedklokking), temperaturmålet, effektgrensen, resten, tomgang.
/// Uten begrensning under last: spenningsgrensen (utledet).
pub fn limiter(r: &Read) -> Limit {
    let l = |name, sub: String, tone, tip: &str| Limit { name, sub, tone, tip: tip.to_string() };
    if r.lost {
        return l("–", "Kortet svarer ikke".into(), "", "");
    }
    if r.inferred {
        // P-tilstanden ved siden av sier allerede «P8 · tomgang» (Kan 14 i
        // kritikk-1.1.0): her står bare at ingenting holder kortet igjen.
        return l("Ingenting", "Kortet er i ro".into(), "", "Kortet er i dyp tomgang (P8). Da spør tjenesten ikke om årsakene: kallet koster ca. 25 ms og vekker kortet.");
    }
    let Some(mask) = r.throttle else {
        return l("–", "Kortet melder ikke årsaker".into(), "", "nvmlDeviceGetCurrentClocksEventReasons svarer ikke på dette kortet.");
    };
    let more = |bit: u64| {
        let n = THROTTLE.iter().filter(|(b, _, _)| mask & b != 0 && *b != bit && *b != 0x1).count();
        if n > 0 { format!(" · +{n}") } else { String::new() }
    };
    let order: [(u64, &str, &str, &str); 8] = [
        (0x40, "Temperatur", "Maskinvaren klokker ned", "warn"),
        (0x80, "Effekt", "Strømforsyningen bremser", "warn"),
        (0x8, "Maskinvare", "Kortet klokker ned selv", "warn"),
        (0x20, "Temperatur", "Temperaturmålet er nådd", "accent"),
        (0x4, "Effekt", "Effektgrensen er nådd", "accent"),
        (0x2, "Programklokker", "Låst av et program", ""),
        (0x100, "Skjermklokke", "Satt for skjermen", ""),
        (0x10, "Synkronisert", "Følger et annet kort", ""),
    ];
    for (bit, name, sub, tone) in order {
        if mask & bit != 0 {
            let why = THROTTLE.iter().find(|t| t.0 == bit).map(|t| t.2).unwrap_or("");
            let sub = match bit {
                0x4 => match r.limit {
                    Some(w) => format!("Grensen {} er nådd", F::watt(Some(w), 0)),
                    None => sub.to_string(),
                },
                0x20 => match r.temp_target {
                    Some(t) => format!("Målet {} er nådd", F::temp(Some(t), 0)),
                    None => sub.to_string(),
                },
                _ => sub.to_string(),
            };
            return l(name, sub + &more(bit), tone, why);
        }
    }
    if mask & 0x1 != 0 {
        return l("Ingenting", "Kortet er i ro".into(), "", "Kortet har ingenting å gjøre og holder klokkene nede (tomgang).");
    }
    if r.util.is_some_and(|u| u >= VOLTAGE_LOAD) {
        return l(
            "Spenning",
            "Toppen av klokkekurven".into(),
            "",
            "NVML melder ingen begrensning, og kortet har last. Da går det så høyt spenningen tillater (det GPU-Z kaller VRel/VOp). Utledet, ikke lest fra kortet.",
        );
    }
    l("Ingen", "Klokkene kan gå fritt".into(), "", "NVML melder ingen begrensning.")
}

/// «P8 · tomgang», som Sensorer.
pub fn pstate_text(p: &str) -> &'static str {
    match p {
        "P0" => "full ytelse",
        "P1" | "P2" => "høy ytelse",
        "P3" | "P4" | "P5" => "middels",
        "P8" => "tomgang",
        "P10" | "P12" => "laveste",
        _ => "",
    }
}

// ================================================================ forklaringene

/// Kort forklaring under tittelen, og (i)-innholdet per kontroll.
struct Doc {
    desc: &'static str,
    paras: &'static [&'static str],
}

const DOC_POWER: Doc = Doc {
    desc: "Øker grensen for hvor mye strøm kortet kan trekke. Høyere gir mer ytelse og varme.",
    paras: &[
        "Kortet holder seg under grensen ved å senke klokken. Under tung last er det oftest effekten som begrenser et RTX 2080, så en høyere grense gir høyere boost.",
        "Prisen er mer varme, mer viftestøy og høyere strømforbruk. En lavere grense gir et kjøligere og stillere kort med litt lavere ytelse.",
    ],
};
const DOC_CORE: Doc = Doc {
    desc: "Flytter hele klokkekurven opp eller ned. Pluss gir høyere klokke ved samme spenning, men kan gjøre kortet ustabilt.",
    paras: &[
        "Offset legges på hvert punkt på klokkekurven (spenning mot klokke). Kortet booster fortsatt selv, men lander høyere. Spenningen endres ikke.",
        "For høy offset gir krasj, feil i bildet eller at driveren starter på nytt. Prøv i steg på 15–25 MHz og test under last.",
    ],
};
const DOC_MEM: Doc = Doc {
    desc: "Øker minneklokken. Gir mer minnebåndbredde; for høyt gir feil eller lavere ytelse.",
    paras: &[
        "Oppgitt i effektiv MHz, som i Afterburner: +1\u{a0}000 her er +500 i NVML.",
        "GDDR6 retter feil selv. For høy offset kan derfor gi lavere ytelse før det synes i bildet. Mål ytelsen, ikke bare om det krasjer.",
    ],
};
const DOC_TEMP: Doc = Doc {
    desc: "Temperaturen kortet holder seg under. Lavere gir et kjøligere kort og lavere klokke.",
    paras: &[
        "Når temperaturen når målet, senker kortet klokken (begrensningen «Temperaturmål»). Viftene styres fortsatt av viftemodusen.",
        "Nedklokking og avstenging lenger opp er faste og kan ikke endres. NVIDIA har ikke bekreftet at målet virker på GeForce-kort.",
    ],
};
const DOC_FAN: Doc = Doc {
    desc: "Hvem som styrer viftene: driveren, viftekurven eller en fast fart.",
    paras: &[
        "Auto: driveren styrer etter temperaturen, som uten appen. Kurve: tjenesten setter farten fra viftekurven hvert sekund. Manuell: fast fart, uansett temperatur.",
        "Viftene går aldri under 35 % på dette kortet (grensen ligger i VBIOS), så viftestopp i tomgang er ikke mulig. Lukkes Agentic Rigg helt, går viftene tilbake til Auto.",
    ],
};

// ================================================================ siden

/// Kontrollene i rekkefølgen på siden.
const ROWS: [Field; 5] = [Field::Power, Field::Core, Field::Mem, Field::Temp, Field::Fan];

pub struct Kontroller {
    app: Rc<App>,
    root: gtk::Box,
    actions: gtk::Box,
    strip: Rc<LockStrip>,
    stats: Vec<Rc<Stat>>,
    rows: Vec<Rc<OcRow>>,
    fan_seg: Seg,
    fan_edit: gtk::Button,
    draft_tag: gtk::Box,
    reset_all: gtk::Button,
    /// Siste verdier lest mens kortet svarte (utkast når kortet er borte).
    last_good: RefCell<Option<Values>>,
    diff_panel: Panel,
    diff_kv: gtk::Box,
    dirty: gtk::Label,
    apply: gtk::Button,
    discard: gtk::Button,
    confirm: Rc<ConfirmBar>,
    limit_kv: Vec<gtk::Label>,
    limit_note: gtk::Label,
    since: RefCell<HashMap<u64, i64>>,
    slots: gtk::Box,
    slots_count: gtk::Label,
    charts: Rc<ChartSet>,
    safety: gtk::Label,
    visible: Cell<bool>,
    /// Spaltene står under hverandre (smalt vindu): teksten i toppfeltet skjules.
    narrow: Cell<bool>,
    renaming: Cell<Option<usize>>,
}

impl Kontroller {
    fn key(self: &Rc<Self>, k: &Key) -> bool {
        // Ctrl+Enter: Bruk. Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y): angre og gjør om i utkastet.
        if k.ctrl && matches!(k.key, gdk::Key::Return | gdk::Key::KP_Enter) {
            if self.draft().is_some() && self.apply.is_sensitive() {
                self.do_apply();
            }
            return true;
        }
        if k.ctrl && !k.typing && matches!(k.key, gdk::Key::z | gdk::Key::Z | gdk::Key::y | gdk::Key::Y) {
            let redo = k.shift || matches!(k.key, gdk::Key::y | gdk::Key::Y);
            locked::undo_draft(&self.app, redo);
            return true;
        }
        false
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(8, &["page", "styring", "kontroller"]);
    root.append(&crate::widgets::styrer::TargetLine::new(app).root);
    let strip = LockStrip::new(app, "Viser det kortet har nå. Det du endrer, blir et utkast som lagres i appen og ikke sendes til kortet.");
    root.append(&strip.root);
    let confirm = ConfirmBar::new(app);
    root.append(&confirm.root);

    // ---- Tallene øverst ----
    let strip_stats = StatStrip::new(8);
    let names = ["Kjerneklokke", "Minneklokke", "Temperatur", "Effekt", "Last", "Vifte", "P-tilstand", "Begrenset av"];
    let stats: Vec<Rc<Stat>> = names.iter().map(|n| Stat::new(app, n)).collect();
    for s in &stats {
        strip_stats.add(s);
    }
    // P-tilstand og begrensningen har ingen måler.
    stats[6].meter.set_visible(false);
    stats[7].meter.set_visible(false);
    let live = vbox(0, &["group", "oc-live"]);
    live.append(&strip_stats.root);
    root.append(&live);

    // ---- Klokker og effekt ----
    let main = vbox(8, &[]);
    let setup = Panel::new("Innstillinger for kortet");
    let draft_tag = tag("Utkast · ikke sendt til kortet", Some("pencil"), "accent");
    draft_tag.set_visible(false);
    let reset_all = button(Some("rotate-ccw"), Some("Tilbakestill alt til standard"), &["sm"]);
    setup.aside.append(&draft_tag);
    setup.aside.append(&reset_all);
    let rows_box = vbox(0, &["oc-rows"]);
    setup.body.append(&rows_box);
    let mk = |title: &str, unit: &str, doc: &'static Doc, f: Field, signed: bool| {
        let a = app.clone();
        let t = title.to_string();
        let info = info_button(&title.to_lowercase(), move || info_for(&a, &t, doc, f));
        OcRow::new(app, title, unit, info, signed)
    };
    let rows: Vec<Rc<OcRow>> = vec![
        mk("Effektgrense", "W", &DOC_POWER, Field::Power, false),
        mk("Kjerneklokke-offset", "MHz", &DOC_CORE, Field::Core, true),
        mk("Minneklokke-offset", "MHz", &DOC_MEM, Field::Mem, true),
        mk("Temperaturmål", F::temp_label(), &DOC_TEMP, Field::Temp, false),
        mk("Vifte", "%", &DOC_FAN, Field::Fan, false),
    ];
    for (r, doc) in rows.iter().zip([&DOC_POWER, &DOC_CORE, &DOC_MEM, &DOC_TEMP, &DOC_FAN]) {
        set_text(&r.desc, doc.desc);
        rows_box.append(&r.root);
    }
    rows[0].slider.set_fmt(|v| F::watt(Some(v), 0));
    rows[1].slider.set_fmt(|v| signed(v, "MHz"));
    rows[2].slider.set_fmt(|v| signed(v, "MHz"));
    rows[3].slider.set_fmt(|v| F::temp(Some(v), 0));
    // Lagres i °C (NVML), vises i enheten fra Innstillinger.
    crate::widgets::oc::temp_spin(&rows[3].spin);
    rows[4].slider.set_fmt(|v| F::pct(Some(v), 0));
    // Vifte: modus over glideren, og en lenke til kurven.
    let fan_seg = Seg::new(&[("auto", "Auto"), ("curve", "Kurve"), ("manual", "Manuell")], "auto", "Viftemodus").small();
    let fan_edit = button(Some("arrow-right"), Some("Rediger kurven"), &["sm", "flat"]);
    tip(&fan_edit, "Gå til Vifter, der kurven redigeres", None);
    rows[4].top.append(&fan_seg.root);
    fan_edit.set_halign(gtk::Align::Start);
    rows[4].mid.append(&fan_edit);
    rows[4].top.set_visible(true);
    let safety_row = hbox(8, &["safety-note"]);
    safety_row.append(&Icon::new("shield", 14));
    let safety = label("", &["muted", "sm"]);
    safety.set_wrap(true);
    safety.set_hexpand(true);
    safety_row.append(&safety);
    setup.root.append(&safety_row);
    main.append(&setup.root);

    let charts_row = Cols::new(&[1.0, 1.0, 1.0], 8, 760);
    charts_row.set_fill(170);
    charts_row.add_css_class("ctl-charts");
    let charts = ChartSet::new(app);
    let build_charts = {
        let (app, set, row) = (app.clone(), charts.clone(), charts_row.clone());
        move || {
            row.clear();
            set.clear();
            for (id, title) in [("coreClock", "Kjerneklokke"), ("gpuTemp", "Temperatur"), ("power", "Effekt")] {
                let p = chart::def_panel(&app, id, DefOpts { window: 300, sync: None, title: Some(title.into()), stats: false, now: false });
                row.append(&p.root);
                set.add(&p);
            }
        }
    };
    build_charts();
    charts.connect_gpu(build_charts);
    main.append(&charts_row);

    // ---- Sidekolonnen ----
    let side = vbox(8, &[]);
    let diff_panel = Panel::new("Utkast");
    let diff_kv = vbox(0, &["diff-list"]);
    diff_panel.body.append(&diff_kv);
    diff_panel.root.set_visible(false);
    side.append(&diff_panel.root);

    let prof_p = Panel::new("Profiler");
    let slots_count = label("–", &["tag", "quiet"]);
    slots_count.set_valign(gtk::Align::Center);
    tip(&slots_count, "Hent fyller utkastet med verdiene fra profilen. Ingenting sendes til kortet.", None);
    prof_p.aside.append(&slots_count);
    let slots = vbox(0, &["slots"]);
    prof_p.body.append(&slots);
    side.append(&prof_p.root);

    let lim_p = Panel::new("Begrensninger");
    let ll = KvList::new();
    lim_p.body.append(&ll.grid);
    let limit_kv: Vec<gtk::Label> = LIMIT_ROWS.iter().map(|(k, _, _)| ll.add(k, "–")).collect();
    for (l, (_, _, why)) in limit_kv.iter().zip(LIMIT_ROWS) {
        l.set_tooltip_text(Some(why));
    }
    let limit_note = label("", &["muted", "sm", "lim-note"]);
    limit_note.set_wrap(true);
    lim_p.body.append(&limit_note);
    side.append(&lim_p.root);

    let nos = Panel::new("Ikke mulig på dette kortet");
    let nl = KvList::new();
    nos.body.append(&nl.grid);
    for (k, v) in [
        ("Spenningsoffset", "NVIDIA har ingen spenningsoffset på Linux."),
        ("Klokkekurven (V/F)", "Kan ikke leses eller redigeres med NVML på Turing. Offset flytter hele kurven."),
        ("Låst minneklokke", "Krever Ampere eller nyere."),
        ("Viftestopp (0 %)", "Minste fart er 35 % (VBIOS)."),
    ] {
        nl.add(k, v).add_css_class("muted");
    }
    side.append(&nos.root);

    let cols = Columns::new(&main, &side, 340, 1040);
    cols.set_row_breakpoint(700);
    root.append(&cols);

    // ---- Toppfeltet (bare i utkast) ----
    let actions = hbox(6, &["styring-actions"]);
    actions.set_valign(gtk::Align::Center);
    let dirty = label("", &["muted", "sm", "dirty-text"]);
    let discard = button(None, Some("Forkast utkast"), &["sm", "flat"]);
    tip(&discard, "Tilbake til det kortet har. Utkastet slettes (Ctrl+Z angrer).", None);
    let apply = button(None, Some("Bruk"), &["sm", "suggested"]);
    actions.append(&dirty);
    actions.append(&discard);
    actions.append(&apply);
    locked::mark_topbar(app, "kontroller");

    let page = Rc::new(Kontroller {
        app: app.clone(),
        root,
        actions,
        strip,
        stats,
        rows,
        fan_seg,
        fan_edit,
        draft_tag,
        reset_all,
        last_good: RefCell::new(None),
        diff_panel,
        diff_kv,
        dirty,
        apply,
        discard,
        confirm,
        limit_kv,
        limit_note,
        since: RefCell::new(HashMap::new()),
        slots,
        slots_count,
        charts,
        safety,
        visible: Cell::new(false),
        narrow: Cell::new(false),
        renaming: Cell::new(None),
    });
    page.wire(&cols);
    page.refresh();
    page.render_slots();
    bilder::maybe_start(app);
    Box::new(Wrap(page))
}

/// Begrensningene i sidekolonnen (som HWiNFOs «Performance Limit»): navn,
/// bitene og forklaringen. Spenning er utledet (NVML har ingen bit for den).
const LIMIT_ROWS: [(&str, u64, &str); 6] = [
    ("Effekt", 0x4 | 0x80, "Effektgrensen er nådd, eller strømforsyningen bremser."),
    ("Temperatur", 0x20 | 0x40, "Temperaturmålet er nådd, eller maskinvaren klokker ned fordi kortet er for varmt."),
    ("Spenning", 0, "Utledet: ingen begrensning fra NVML mens kortet har last. Da står det på toppen av klokkekurven."),
    ("Maskinvare", 0x8, "Kortet klokker ned av seg selv (strøm eller temperatur)."),
    ("Tomgang", 0x1, "Kortet har ingenting å gjøre."),
    ("Annet", 0x2 | 0x10 | 0x100, "Programklokker, synkronisert boost eller skjermklokke."),
];

/// (i)-innholdet for en kontroll, med tallene fra kortet nå.
fn info_for(app: &Rc<App>, title: &str, doc: &Doc, f: Field) -> gtk::Box {
    let r = read(app);
    let st = stock(&r);
    let paras: Vec<String> = doc.paras.iter().map(|s| s.to_string()).collect();
    let fx = |k: &str, v: String| (k.to_string(), v);
    let pol = |p: txn::Range| format!("{} til {}", signed(p.min, "MHz"), signed(p.max, "MHz"));
    let facts: Vec<(String, String)> = match f {
        Field::Power => vec![
            fx("Standard", F::watt(Some(st.power_limit_w), 0)),
            fx("Område", format!("{} til {} (fra kortet)", F::watt(r.p_min, 0), F::watt(r.p_max, 0))),
            fx("Nå", format!("grense {}, trekker {}", F::watt(r.limit, 0), F::watt(r.power, 0))),
            fx("Påvirker", "Kjerneklokken under last, temperatur, viftestøy og strømforbruk".into()),
            fx("NVML", "nvmlDeviceGetEnforcedPowerLimit (leses). Skriving er låst.".into()),
        ],
        Field::Core => vec![
            fx("Standard", signed(0.0, "MHz")),
            fx("Kortet tillater", r.core_rng.map(|(a, b)| format!("{} til {}", signed(a, "MHz"), signed(b, "MHz"))).unwrap_or_else(|| "leses ikke".into())),
            fx("Tjenesten tillater", pol(txn::POLICY_CORE)),
            fx("Nå", format!("kjerneklokke {}, høyest {}", F::mhz(r.core), F::mhz(r.core_max))),
            fx("Påvirker", "Kjerneklokken på hele kurven og stabiliteten. Ikke spenningen.".into()),
            fx("NVML", "nvmlDeviceGetClockOffsets (leses). Skriving er låst.".into()),
        ],
        Field::Mem => vec![
            fx("Standard", signed(0.0, "MHz")),
            fx("Kortet tillater", r.mem_rng.map(|(a, b)| format!("{} til {}", signed(a, "MHz"), signed(b, "MHz"))).unwrap_or_else(|| "leses ikke".into())),
            fx("Tjenesten tillater", pol(txn::POLICY_MEM)),
            fx("Nå", format!("minneklokke {}, høyest {}", F::mhz(r.mem), F::mhz(r.mem_max))),
            fx("Påvirker", "Minnebåndbredden. Ikke kjerneklokken.".into()),
            fx("NVML", "nvmlDeviceGetClockOffsets (leses). Skriving er låst.".into()),
        ],
        Field::Temp => vec![
            fx("Standard", F::temp(Some(st.temp_target_c), 0)),
            fx("Område", r.temp_rng.map(|(a, b)| format!("{} til {} (fra kortet)", F::temp(Some(a), 0), F::temp(Some(b), 0))).unwrap_or_else(|| "leses ikke".into())),
            fx("Faste grenser", format!("nedklokking ved {}, avstenging ved {}", F::temp(r.slowdown, 0), F::temp(r.shutdown, 0))),
            fx("Nå", F::temp(r.temp, 0)),
            fx("NVML", "nvmlDeviceGetTemperatureThreshold, ACOUSTIC_CURR (leses). Skriving er låst.".into()),
        ],
        Field::Fan | Field::Curve => vec![
            fx("Standard", "Auto (driveren styrer)".into()),
            fx("Minste fart", format!("{} (VBIOS)", F::pct(Some(r.floor()), 0))),
            fx("Nå", r.fans_line().replace('\u{2060}', "")),
            fx("Modus nå", r.fan_mode_text()),
            fx("NVML", "nvmlDeviceGetFanControlPolicy_v2 (leses). Skriving er låst.".into()),
        ],
    };
    info_content(title, &paras, &facts)
}

/// `Page` for `Rc<Kontroller>` (siden deler seg selv med tilbakekallene).
struct Wrap(Rc<Kontroller>);

impl Page for Wrap {
    fn widget(&self) -> gtk::Widget {
        self.0.root.clone().upcast()
    }
    fn actions(&self) -> Option<gtk::Widget> {
        Some(self.0.actions.clone().upcast())
    }
    fn show(&self, _q: Option<&str>) {
        self.0.visible.set(true);
        self.0.charts.set_visible(true);
        self.0.refresh();
    }
    fn hide(&self) {
        self.0.visible.set(false);
        self.0.charts.set_visible(false);
    }
    fn on_key(&self, k: &Key) -> bool {
        self.0.key(k)
    }
}

impl Kontroller {
    fn draft(&self) -> Option<Values> {
        locked::stored(&self.app).utkast
    }

    /// Det et nytt utkast begynner fra: kortet nå, eller siste lesing når kortet er borte.
    fn base(&self) -> Values {
        let r = read(&self.app);
        if r.lost { self.last_good.borrow().clone().unwrap_or_else(|| r.values()) } else { r.values() }
    }

    fn edit(&self, f: impl FnOnce(&mut Values)) {
        locked::edit_draft(&self.app, self.base(), f);
    }

    fn wire(self: &Rc<Self>, cols: &Columns) {
        let w = Rc::downgrade(self);
        cols.connect_narrow(move |stacked, rows| {
            if let Some(p) = w.upgrade() {
                for r in &p.rows {
                    r.set_narrow(rows);
                }
                p.narrow.set(stacked);
                p.refresh();
            }
        });
        // Glidere og tallfelt.
        for (i, f) in ROWS.iter().enumerate() {
            let f = *f;
            let w = Rc::downgrade(self);
            self.rows[i].connect_change(move |v| {
                let Some(p) = w.upgrade() else { return };
                p.edit(|d| match f {
                    Field::Power => d.power_limit_w = v.round(),
                    Field::Core => d.core_offset_mhz = v.round(),
                    Field::Mem => d.mem_offset_mhz = v.round(),
                    Field::Temp => d.temp_target_c = v.round(),
                    _ => d.fan_percent = v.round(),
                });
            });
            let w = Rc::downgrade(self);
            self.rows[i].connect_reset(move || {
                let Some(p) = w.upgrade() else { return };
                let st = stock(&read(&p.app));
                p.edit(|d| reset_field(d, f, &st));
            });
        }
        let w = Rc::downgrade(self);
        self.fan_seg.connect(move |v| {
            if let Some(p) = w.upgrade() {
                let m = FanMode::parse(v).unwrap_or_default();
                p.edit(|d| d.fan_mode = m);
            }
        });
        let w = Rc::downgrade(self);
        self.fan_edit.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.app.go("viftekurve");
            }
        });
        let w = Rc::downgrade(self);
        self.reset_all.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.do_reset_all();
            }
        });
        let w = Rc::downgrade(self);
        self.discard.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                locked::update(&p.app, "utkast", |s| s.utkast = None);
                let a = p.app.clone();
                p.app.toast(crate::toast::Toast::new("Utkastet er forkastet").action("Angre", move || {
                    locked::undo_draft(&a, false);
                }));
            }
        });
        let w = Rc::downgrade(self);
        self.apply.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.do_apply();
            }
        });
        let w = Rc::downgrade(self);
        locked::on_store(move |what| {
            if let Some(p) = w.upgrade() {
                if what == "profiler" || what == "utkast" {
                    p.render_slots();
                }
                p.refresh();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_telemetry.connect(move |_| {
            if let Some(p) = w.upgrade() {
                p.track_throttle();
                if p.visible.get() {
                    p.refresh();
                }
            }
        });
        for hub in [&self.app.on_devices, &self.app.on_theme] {
            let w = Rc::downgrade(self);
            hub.connect(move |_| {
                if let Some(p) = w.upgrade() {
                    p.refresh();
                }
            });
        }
        let w = Rc::downgrade(self);
        self.app.on_conn.connect(move |_| {
            if let Some(p) = w.upgrade() {
                p.refresh();
            }
        });
        let w = Rc::downgrade(self);
        self.app.on_settings.connect(move |k| {
            if let Some(p) = w.upgrade()
                && k.iter().any(|k| matches!(k.as_str(), "unit" | "confirmSec" | "decimals" | "clock"))
            {
                for r in &p.rows {
                    r.slider.queue_draw();
                }
                p.refresh();
                p.render_slots();
            }
        });
        let w = Rc::downgrade(self);
        *self.confirm.on_done.borrow_mut() = Some(Box::new(move |how| {
            if let Some(p) = w.upgrade() {
                if how == "confirm" {
                    locked::update(&p.app, "utkast", |s| s.utkast = None);
                }
                p.refresh();
            }
        }));
    }

    fn do_reset_all(self: &Rc<Self>) {
        let r = read(&self.app);
        let st = stock(&r);
        let cur = self.draft().unwrap_or_else(|| self.base());
        let next = reset_all(&cur, &st);
        if self.draft().is_none() && draft_diff(&self.base(), &next).is_empty() {
            self.app.toast(crate::toast::Toast::new("Kortet står allerede på standard"));
            return;
        }
        locked::update(&self.app, "utkast", |s| s.utkast = Some(next));
        let a = self.app.clone();
        self.app.toast(crate::toast::Toast::new("Utkastet har standardverdiene").body("Ingenting er sendt til kortet.").action("Angre", move || {
            locked::undo_draft(&a, false);
        }));
    }

    /// Når begynte hver begrensning? (Tiden står i verktøytipset.)
    fn track_throttle(&self) {
        let r = read(&self.app);
        let mask = if r.inferred { 0x1 } else { r.throttle.unwrap_or(0) };
        let mut since = self.since.borrow_mut();
        since.retain(|bit, _| mask & bit != 0);
        for (bit, _, _) in THROTTLE {
            if mask & bit != 0 {
                since.entry(bit).or_insert(r.t);
            }
        }
    }

    fn do_apply(self: &Rc<Self>) {
        let Some(d) = self.draft() else { return };
        if locked::gate(&self.app) != Gate::Sim || self.confirm.active() {
            return;
        }
        let secs = self.app.settings().confirm_sec;
        let w = Rc::downgrade(self);
        locked::send_write(&self.app, "apply", json!({"settings": d.to_json(), "confirmTimeoutS": secs}), move |res| {
            let Some(p) = w.upgrade() else { return };
            match res {
                Ok(v) if v["txn"].is_string() => p.confirm.start(&p.app, &v),
                Ok(_) => p.app.toast(crate::toast::Toast::new("Ingen endringer å bruke")),
                Err(e) => p.app.toast(crate::toast::Toast::new("Kunne ikke bruke utkastet").body(&e.message).error()),
            }
            p.refresh();
        });
    }

    fn refresh(&self) {
        let r = read(&self.app);
        let draft = self.draft();
        let editing = draft.is_some();
        if !r.lost && r.limit.is_some() {
            *self.last_good.borrow_mut() = Some(r.values());
        }
        // Merket «Utkast · ikke sendt til kortet» bare når utkastet er ulikt
        // kortet (etter Behold mot sim er det likt; Kan 15 i kritikk-1.1.0).
        let differs = draft.as_ref().is_some_and(|d| r.lost || !draft_diff(&r.values(), d).is_empty());
        self.strip.sync(&self.app, r.lost, differs);
        locked::sync_topbar(&self.app, "kontroller");
        self.draft_tag.set_visible(differs);
        class(&self.root, "drafting", editing);
        self.render_stats(&r);
        self.render_rows(&r, draft.as_ref());
        self.render_diff(&r, draft.as_ref());
        self.render_limits(&r);
        set_text(
            &self.safety,
            &format!(
                "Når endring er slått på: hver endring må beholdes innen {}{}s, ellers rulles den tilbake. Over {} går viftene alltid til {}. Lukkes Agentic Rigg helt, går kortet tilbake til standard og viftene til Auto.",
                self.app.settings().confirm_sec,
                F::NNBSP,
                F::temp(Some(txn::EMERGENCY_FAN_C), 0),
                F::pct(Some(100.0), 0),
            ),
        );
    }

    fn render_stats(&self, r: &Read) {
        let s = &self.stats;
        let frac = |v: Option<f64>, m: Option<f64>| match (v, m) {
            (Some(v), Some(m)) if m > 0.0 => Some(v / m),
            _ => None,
        };
        // Kjerneklokke og minneklokke: nå, og høyeste kortet kan.
        s[0].set(&F::num(r.core, 0), "MHz", &r.core_max.map(|m| format!("høyest {}", F::mhz(Some(m)))).unwrap_or_default());
        s[0].meter.set(frac(r.core, r.core_max), None, 0);
        s[0].set_tip("Kjerneklokken nå. Måleren: andel av det høyeste kortet kan.");
        s[1].set(&F::num(r.mem, 0), "MHz", &r.mem_max.map(|m| format!("høyest {}", F::mhz(Some(m)))).unwrap_or_default());
        s[1].meter.set(frac(r.mem, r.mem_max), None, 0);
        s[1].set_tip("Minneklokken nå. NVML oppgir den uten dobbel dataklokke.");
        // Temperatur mot målet (merket) og nedklokkingen.
        let top = r.slowdown.unwrap_or(97.0);
        let t_tone = match (r.temp, r.temp_target) {
            (Some(t), _) if t >= top - 2.0 => 2,
            (Some(t), Some(g)) if t >= g - 1.0 => 1,
            _ => 0,
        };
        s[2].set(&F::temp_num(r.temp, 0), F::temp_label(), &r.temp_target.map(|g| format!("mål {}", F::temp(Some(g), 0))).unwrap_or_default());
        s[2].meter.set(r.temp.map(|t| (t - 20.0) / (top - 20.0)), r.temp_target.map(|g| (g - 20.0) / (top - 20.0)), t_tone);
        s[2].set_tone(["", "warn", "danger"][t_tone as usize]);
        s[2].set_tip(&format!("GPU-temperaturen. Merket på måleren er temperaturmålet; måleren går til nedklokkingen ved {}.", F::temp(Some(top), 0)));
        // Effekt: W og andel av grensen.
        let share = frac(r.power, r.limit);
        s[3].set(&F::num(r.power, 0), "W", &match (share, r.limit) {
            (Some(f), Some(l)) => format!("{} av {}", F::pct(Some(f * 100.0), 0), F::watt(Some(l), 0)),
            _ => String::new(),
        });
        let p_tone = if share.is_some_and(|f| f >= 0.97) { 1 } else { 0 };
        s[3].meter.set(share, None, p_tone);
        s[3].set_tone(if p_tone == 1 { "warn" } else { "" });
        s[3].set_tip("Strømtrekket nå, og hvor mye av effektgrensen det er.");
        s[4].set(&F::num(r.util, 0), "%", &r.mem_util.map(|m| format!("minnekontroller {}", F::pct(Some(m), 0))).unwrap_or_default());
        s[4].meter.set(r.util.map(|u| u / 100.0), None, 0);
        s[4].set_tip("GPU-last nå.");
        let rpm = r.rpm.iter().flatten().cloned().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        s[5].set(&F::num(r.fan_max_now(), 0), "%", &rpm.map(|x| F::rpm(Some(x))).unwrap_or_default());
        s[5].meter.set(r.fan_max_now().map(|f| f / 100.0), Some(r.floor() / 100.0), 0);
        s[5].set_tip(&format!("Den raskeste viften nå. Merket er minste fart ({}).", F::pct(Some(r.floor()), 0)));
        let p = r.pstate.clone().unwrap_or_else(|| F::DASH.into());
        s[6].set(&p, "", pstate_text(&p));
        s[6].set_tip("P-tilstanden. P0 er full ytelse, P8 er dyp tomgang.");
        let lim = limiter(r);
        s[7].set(lim.name, "", &lim.sub);
        s[7].set_tone(lim.tone);
        s[7].set_tip(&lim.tip);
    }

    fn render_rows(&self, r: &Read, draft: Option<&Values>) {
        let st = stock(r);
        let base = r.values();
        let shown = draft.cloned().unwrap_or_else(|| base.clone());
        let diff = draft.map(|d| draft_diff(&base, d)).unwrap_or_default();
        let changed = |k: &str| diff.iter().any(|x| x == k);
        let na = r.lost;

        // Effektgrense
        let row = &self.rows[0];
        let (lo, hi) = (r.p_min.unwrap_or(txn::HW_POWER.min), r.p_max.unwrap_or(txn::HW_POWER.max));
        row.set_range(lo, hi, (lo, hi), 1.0, 5.0);
        row.show(if na && draft.is_none() { None } else { Some(shown.power_limit_w) }, r.limit, Some(st.power_limit_w));
        set_text(&row.now, &match (r.power, r.limit) {
            (Some(p), Some(l)) => format!("Trekker nå {} ({} av grensen)", F::watt(Some(p), 0), F::pct(Some(p / l * 100.0), 0)),
            _ => format!("Trekker nå {}", F::watt(r.power, 0)),
        });
        row.set_changed(changed("powerLimitW"));
        row.set_reset_tip(&format!("Tilbake til standard ({})", F::watt(Some(st.power_limit_w), 0)));

        // Klokke-offset: hele området kortet tillater tegnes; glideren holder seg
        // innenfor det tjenesten tillater.
        for (i, off, rng, pol, now, max, key, step, big) in [
            (1, r.core_off, r.core_rng, txn::POLICY_CORE, r.core, r.core_max, "clockOffsets.coreMhz", 5.0, 25.0),
            (2, r.mem_off, r.mem_rng, txn::POLICY_MEM, r.mem, r.mem_max, "clockOffsets.memMhz", 10.0, 100.0),
        ] {
            let row = &self.rows[i];
            let (lo, hi) = rng.unwrap_or((pol.min, pol.max));
            row.set_range(lo.min(pol.min), hi.max(pol.max), (pol.min.max(lo), pol.max.min(hi)), step, big);
            let v = if i == 1 { shown.core_offset_mhz } else { shown.mem_offset_mhz };
            // Leses ikke (og ikke i utkast): «–», ikke en gjettet 0.
            row.show(if (off.is_none() || na) && draft.is_none() { None } else { Some(v) }, off, Some(0.0));
            let what = if i == 1 { "Kjerneklokke" } else { "Minneklokke" };
            set_text(&row.now, &format!("{what} nå {}, høyest {}{}", F::mhz(now), F::mhz(max), if off.is_none() && !na { " · offset leses ikke ennå" } else { "" }));
            row.set_changed(changed(key));
            row.set_reset_tip(&format!("Tilbake til standard ({})", signed(0.0, "MHz")));
            let t = format!("Tjenesten tillater {} til {} når endring er slått på. Kortet tillater {} til {}.", signed(pol.min, "MHz"), signed(pol.max, "MHz"), signed(lo, "MHz"), signed(hi, "MHz"));
            if row.slider.tooltip_text().as_deref() != Some(t.as_str()) {
                row.slider.set_tooltip_text(Some(&t));
            }
        }

        // Temperaturmål
        let row = &self.rows[3];
        let (lo, hi) = r.temp_rng.unwrap_or((txn::HW_TEMP.min, txn::HW_TEMP.max));
        row.set_range(lo, hi, (lo, hi), 1.0, 5.0);
        row.show(if na && draft.is_none() { None } else { Some(shown.temp_target_c) }, r.temp_target, Some(st.temp_target_c));
        set_text(&row.now, &match (r.slowdown, r.shutdown) {
            (Some(a), Some(b)) => format!("GPU-temperatur nå {} · nedklokking ved {}, avstenging ved {}", F::temp(r.temp, 0), F::temp(Some(a), 0), F::temp(Some(b), 0)),
            _ => format!("GPU-temperatur nå {}", F::temp(r.temp, 0)),
        });
        set_text(&row.unit, F::temp_label());
        crate::widgets::oc::temp_spin_refresh(&row.spin);
        row.set_changed(changed("tempTargetC"));
        row.set_reset_tip(&format!("Tilbake til standard ({})", F::temp(Some(st.temp_target_c), 0)));

        // Vifte: modus, og fast fart i Manuell.
        let row = &self.rows[4];
        let mode = shown.fan_mode;
        self.fan_seg.set(mode.as_str());
        let floor = r.floor();
        row.set_range(floor, 100.0, (floor, 100.0), 1.0, 5.0);
        let manual = mode == FanMode::Manual;
        row.slider.set_visible(manual);
        self.fan_edit.set_visible(mode == FanMode::Curve);
        row.show(if manual { Some(shown.fan_percent) } else { None }, r.fan_max_now().filter(|_| base.fan_mode == FanMode::Manual), None);
        row.unit.set_visible(manual);
        set_text(&row.value, mode_name(mode));
        row.reset.set_sensitive(mode != FanMode::Auto);
        row.set_reset_tip("Tilbake til standard (Auto)");
        set_text(&row.desc, match mode {
            FanMode::Auto => "Driveren styrer viftene etter temperaturen, som uten appen.",
            FanMode::Curve => "Agentic Rigg setter farten etter viftekurven hvert sekund.",
            FanMode::Manual => "Viftene går med fast fart, uansett temperatur.",
        });
        set_text(&row.now, &if r.fans.is_empty() { F::DASH.to_string() } else { r.fans_line() });
        row.set_changed(diff.iter().any(|x| x.starts_with("fan.")));
    }

    fn render_limits(&self, r: &Read) {
        let since = self.since.borrow();
        let mask = if r.inferred { 0x1 } else { r.throttle.unwrap_or(0) };
        let known = !r.lost && (r.throttle.is_some() || r.inferred);
        let volt = limiter(r).name == "Spenning";
        for (l, (_, bits, why)) in self.limit_kv.iter().zip(LIMIT_ROWS) {
            let on = if bits == 0 { volt } else { mask & bits != 0 };
            let text = if !known || (r.inferred && bits != 0x1) {
                F::DASH.to_string()
            } else if on {
                let first = THROTTLE.iter().filter(|(b, _, _)| bits & b != 0).filter_map(|(b, _, _)| since.get(b)).min();
                match first {
                    Some(t) => format!("Ja · siden {}", locked::clock(*t, false)),
                    None => "Ja".into(),
                }
            } else {
                "Nei".into()
            };
            set_text(l, &text);
            // Tomgang er den vanlige tilstanden, ikke en begrensning å legge merke til.
            class(l, "lim-on", on && known && bits != 0x1);
            class(l, "na", text == F::DASH);
            let names: Vec<&str> = THROTTLE.iter().filter(|(b, _, _)| bits & b != 0 && mask & b != 0).map(|(_, n, _)| *n).collect();
            let t = if names.is_empty() { why.to_string() } else { format!("{why}\nNå: {}.", names.join(", ")) };
            if l.tooltip_text().as_deref() != Some(t.as_str()) {
                l.set_tooltip_text(Some(&t));
            }
        }
        set_text(
            &self.limit_note,
            &if r.lost {
                "Kortet svarer ikke. Begrensningene vises igjen når det er tilbake.".to_string()
            } else if r.inferred {
                "Kortet er i tomgang (P8). Årsakene leses når kortet jobber.".to_string()
            } else if r.throttle.is_none() {
                "Kortet melder ikke klokkebegrensninger.".to_string()
            } else {
                String::new()
            },
        );
        self.limit_note.set_visible(!self.limit_note.text().is_empty());
        let ntip = r.inferred.then_some("I P8 spør tjenesten ikke om årsakene: kallet koster ca. 25 ms og vekker kortet.");
        if self.limit_note.tooltip_text().as_deref() != ntip {
            self.limit_note.set_tooltip_text(ntip);
        }
    }

    fn render_diff(&self, r: &Read, draft: Option<&Values>) {
        let Some(d) = draft else {
            self.diff_panel.root.set_visible(false);
            for w in [self.dirty.clone().upcast::<gtk::Widget>(), self.discard.clone().upcast(), self.apply.clone().upcast()] {
                w.set_visible(false);
            }
            return;
        };
        self.diff_panel.root.set_visible(true);
        crate::widgets::clear(&self.diff_kv);
        let base = r.values();
        let diff = draft_diff(&base, d);
        if diff.is_empty() {
            let l = label("Utkastet er likt det kortet har nå.", &["muted", "sm"]);
            l.set_wrap(true);
            self.diff_kv.append(&l);
        }
        for f in &diff {
            let (name, to) = field_text(d, f);
            let (_, mut from) = field_text(&base, f);
            // Det tjenesten ikke leser ennå, er ukjent, ikke 0.
            if (f == "clockOffsets.coreMhz" && r.core_off.is_none()) || (f == "clockOffsets.memMhz" && r.mem_off.is_none()) || (f == "fan.mode" && r.fan_policy.is_none()) {
                from = "ukjent".into();
            }
            let row = hbox(6, &["diff-row"]);
            let k = label(name, &["k"]);
            k.set_hexpand(true);
            row.append(&k);
            row.append(&label(&from, &["muted", "num"]));
            row.append(&Icon::new("arrow-right", 12));
            row.append(&label(&to, &["bold", "num"]));
            self.diff_kv.append(&row);
        }
        let note = label("Lagres i appen og sendes ikke til kortet. Ctrl+Z angrer.", &["muted", "sm", "diff-note"]);
        note.set_wrap(true);
        self.diff_kv.append(&note);
        let n = diff.len();
        set_text(&self.dirty, &match n {
            0 => "Ingen endringer".to_string(),
            1 => "1 endring i utkastet".to_string(),
            n => format!("{n} endringer i utkastet"),
        });
        // Smalt vindu: toppfeltet får ikke plass til teksten (utkast-merket sier det).
        self.dirty.set_visible(!self.narrow.get());
        self.discard.set_visible(true);
        self.apply.set_visible(true);
        match locked::gate(&self.app) {
            Gate::Sim => {
                let ok = n > 0 && !self.confirm.active();
                self.apply.set_sensitive(ok);
                tip(&self.apply, if self.confirm.active() { "Endringen venter på Behold" } else if ok { "Bruk utkastet" } else { "Ingen endringer å bruke" }, Some("Ctrl+Enter"));
            }
            Gate::Locked(t) => {
                self.apply.set_sensitive(false);
                self.apply.set_tooltip_text(Some(t));
            }
        }
    }

    fn values_on_page(&self) -> Values {
        self.draft().unwrap_or_else(|| read(&self.app).values())
    }

    /// De fem profilplassene.
    fn render_slots(self: &Rc<Self>) {
        crate::widgets::clear(&self.slots);
        let st = locked::stored(&self.app);
        let here = self.values_on_page();
        let twin = st.profiles.iter().flatten().find(|p| p.values == here).map(|p| p.name.clone());
        let used = st.profiles.iter().flatten().count();
        set_text(&self.slots_count, &format!("{used} av {}", locked::SLOTS));
        if used == 0 {
            let l = label("Ingen profiler ennå. En profil husker alle verdiene på siden, også viftekurven.", &["muted", "sm"]);
            l.set_wrap(true);
            self.slots.append(&l);
        }
        for (i, p) in st.profiles.iter().enumerate() {
            let Some(p) = p else { continue };
            let row = gtk::Grid::new();
            row.add_css_class("slot");
            row.set_column_spacing(6);
            if twin.as_deref() == Some(p.name.as_str()) {
                row.add_css_class("current");
            }
            let name: gtk::Widget = if self.renaming.get() == Some(i) {
                let e = gtk::Entry::new();
                e.set_text(&p.name);
                e.set_max_length(32);
                e.set_hexpand(true);
                e.update_property(&[gtk::accessible::Property::Label(&format!("Nytt navn på {}", p.name))]);
                let w = Rc::downgrade(self);
                e.connect_activate(move |e| {
                    if let Some(pg) = w.upgrade() {
                        pg.rename(i, &e.text());
                    }
                });
                let keys = gtk::EventControllerKey::new();
                let w = Rc::downgrade(self);
                keys.connect_key_pressed(move |_, k, _, _| {
                    if k == gdk::Key::Escape
                        && let Some(pg) = w.upgrade()
                    {
                        pg.renaming.set(None);
                        let pg2 = pg.clone();
                        glib::idle_add_local_once(move || pg2.render_slots());
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                });
                e.add_controller(keys);
                let e2 = e.clone();
                glib::idle_add_local_once(move || {
                    e2.grab_focus();
                });
                e.upcast()
            } else {
                let nb = hbox(6, &[]);
                nb.set_hexpand(true);
                let l = label(&p.name, &["s-name", "bold"]);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                l.set_tooltip_text(Some("Dobbeltklikk for å gi nytt navn"));
                let g = gtk::GestureClick::new();
                let w = Rc::downgrade(self);
                g.connect_pressed(move |_, n, _, _| {
                    if n == 2
                        && let Some(pg) = w.upgrade()
                    {
                        pg.renaming.set(Some(i));
                        let pg2 = pg.clone();
                        glib::idle_add_local_once(move || pg2.render_slots());
                    }
                });
                l.add_controller(g);
                nb.append(&l);
                if twin.as_deref() == Some(p.name.as_str()) {
                    let t = tag("på siden", None, "quiet");
                    t.set_tooltip_text(Some("Verdiene på siden er de samme som i profilen"));
                    nb.append(&t);
                }
                nb.upcast()
            };
            row.attach(&name, 0, 0, 1, 1);
            let load = button(None, Some("Hent"), &["sm"]);
            tip(&load, "Fyll utkastet med verdiene fra profilen", None);
            let w = Rc::downgrade(self);
            let pv = p.clone();
            load.connect_clicked(move |_| {
                if let Some(pg) = w.upgrade() {
                    let v = pv.values.clone();
                    locked::update(&pg.app, "utkast", |s| s.utkast = Some(v));
                    pg.app.toast(crate::toast::Toast::new(&format!("Hentet «{}»", pv.name)).body("Utkastet har verdiene fra profilen. Ingenting er sendt til kortet."));
                }
            });
            row.attach(&load, 1, 0, 1, 1);
            let more = button(Some("ellipsis-vertical"), None, &["sm", "flat"]);
            more.update_property(&[gtk::accessible::Property::Label(&format!("Flere valg for {}", p.name))]);
            let w = Rc::downgrade(self);
            more.connect_clicked(move |b| {
                let Some(pg) = w.upgrade() else { return };
                let m = crate::widgets::popover::Menu::new(b);
                let (a, b2, c) = (pg.clone(), pg.clone(), pg.clone());
                m.item(Some("save"), "Lagre over med verdiene på siden", None, move || a.save_slot(i, None));
                m.item(Some("pencil"), "Gi nytt navn", None, move || {
                    b2.renaming.set(Some(i));
                    b2.render_slots();
                });
                m.item(Some("trash-2"), "Tøm plassen", None, move || c.clear_slot(i));
                m.show();
            });
            row.attach(&more, 2, 0, 1, 1);
            let sum = label(&summary(&p.values), &["muted", "sm", "s-sum"]);
            sum.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let custom = p.values.fan_curve != txn::DEFAULT_CURVE.to_vec();
            sum.set_tooltip_text(Some(&format!(
                "{}\nViftekurve: {}\nLagret {}",
                summary(&p.values),
                if custom { format!("egen, {} punkter", p.values.fan_curve.len()) } else { "standard".into() },
                locked::clock(p.saved_at, false)
            )));
            row.attach(&sum, 0, 1, 3, 1);
            self.slots.append(&row);
        }
        if let Some(free) = st.profiles.iter().position(Option::is_none) {
            let add_row = hbox(8, &["slot-add"]);
            let add = button(Some("plus"), Some("Lagre som ny profil"), &["sm"]);
            let hint = label("", &["muted", "sm"]);
            match &twin {
                Some(t) => {
                    add.set_sensitive(false);
                    add.set_tooltip_text(Some(&format!("Verdiene på siden er de samme som i «{t}»")));
                    set_text(&hint, &format!("Lik «{t}»"));
                }
                None => add.set_tooltip_text(Some(if self.draft().is_some() { "Lagrer utkastet som en ny profil" } else { "Lagrer verdiene kortet har nå som en ny profil" })),
            }
            let w = Rc::downgrade(self);
            add.connect_clicked(move |_| {
                if let Some(pg) = w.upgrade() {
                    pg.save_slot(free, Some(""));
                }
            });
            add_row.append(&add);
            add_row.append(&hint);
            self.slots.append(&add_row);
        } else {
            let l = label("Alle fem plassene er i bruk. Lagre over en profil fra ⋯-menyen.", &["muted", "sm"]);
            l.set_wrap(true);
            self.slots.append(&l);
        }
    }

    fn save_slot(self: &Rc<Self>, i: usize, name: Option<&str>) {
        let values = self.values_on_page();
        let st = locked::stored(&self.app);
        let name = match name {
            Some(n) if !n.is_empty() => n.to_string(),
            Some(_) => {
                let used: Vec<String> = st.profiles.iter().flatten().map(|p| p.name.clone()).collect();
                (1..).map(|n| format!("Profil {n}")).find(|n| !used.contains(n)).unwrap_or_default()
            }
            None => st.profiles[i].as_ref().map(|p| p.name.clone()).unwrap_or_else(|| format!("Profil {}", i + 1)),
        };
        let p = locked::Profile { name: name.clone(), values: values.clone(), saved_at: rigg_core::now_ms() };
        locked::update(&self.app, "profiler", |s| s.profiles[i] = Some(p));
        self.app.toast(crate::toast::Toast::new(&format!("Lagret i «{name}»")).body(&summary(&values)));
    }

    fn rename(self: &Rc<Self>, i: usize, name: &str) {
        self.renaming.set(None);
        let n: String = name.trim().chars().take(32).collect();
        if !n.is_empty() {
            locked::update(&self.app, "profiler", |s| {
                if let Some(p) = s.profiles[i].as_mut() {
                    p.name = n;
                }
            });
        }
        self.render_slots();
    }

    fn clear_slot(self: &Rc<Self>, i: usize) {
        let before = locked::stored(&self.app).profiles[i].clone();
        locked::update(&self.app, "profiler", |s| s.profiles[i] = None);
        let name = before.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let a = self.app.clone();
        self.app.toast(crate::toast::Toast::new(&format!("«{name}» er tømt")).action("Angre", move || {
            let b = before.clone();
            locked::update(&a, "profiler", |s| s.profiles[i] = b);
        }));
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Read {
        Read {
            limit: Some(245.0),
            p_def: Some(245.0),
            core_off: Some(0.0),
            mem_off: Some(0.0),
            temp_target: Some(83.0),
            fan_policy: Some(vec![0, 0]),
            fans: vec![Some(35.0), Some(36.0)],
            fan_min: Some(35.0),
            ..Default::default()
        }
    }

    #[test]
    fn standard_er_kortets_verdier() {
        let st = stock(&card());
        assert_eq!(st.power_limit_w, 245.0);
        assert_eq!((st.core_offset_mhz, st.mem_offset_mhz, st.temp_target_c), (0.0, 0.0, 83.0));
        assert_eq!(st.fan_mode, FanMode::Auto);
        assert_eq!(st.fan_curve, txn::DEFAULT_CURVE.to_vec());
        // Et kort som melder en annen standard, får den.
        let r = Read { p_def: Some(250.4), ..card() };
        assert_eq!(stock(&r).power_limit_w, 250.0);
        // Kortet slik det står er standard: ingen forskjell.
        assert!(draft_diff(&card().values(), &st).is_empty());
    }

    #[test]
    fn tilbakestill_en_og_alle() {
        let r = card();
        let st = stock(&r);
        let mut d = Values { power_limit_w: 200.0, core_offset_mhz: 75.0, mem_offset_mhz: 500.0, temp_target_c: 70.0, fan_mode: FanMode::Manual, fan_percent: 60.0, fan_curve: vec![(40.0, 40.0), (80.0, 100.0)], ..Default::default() };
        reset_field(&mut d, Field::Core, &st);
        assert_eq!(d.core_offset_mhz, 0.0);
        assert_eq!(d.power_limit_w, 200.0, "bare den ene verdien");
        reset_field(&mut d, Field::Fan, &st);
        assert_eq!(d.fan_mode, FanMode::Auto);
        assert_eq!(d.fan_percent, 60.0, "den manuelle farten huskes");
        reset_field(&mut d, Field::Curve, &st);
        assert_eq!(d.fan_curve, txn::DEFAULT_CURVE.to_vec());
        let all = reset_all(&d, &st);
        assert_eq!(draft_diff(&r.values(), &all), Vec::<String>::new());
        assert_eq!(all.fan_percent, 60.0);
    }

    #[test]
    fn utkastet_viser_bare_det_som_gjelder() {
        let base = card().values();
        let d = Values { fan_percent: 80.0, ..base.clone() };
        // Manuell fart i Auto er ikke en endring.
        assert!(draft_diff(&base, &d).is_empty());
        let d = Values { fan_mode: FanMode::Manual, ..d };
        assert_eq!(draft_diff(&base, &d), vec!["fan.mode".to_string(), "fan.manualPercent".to_string()]);
    }

    #[test]
    fn begrensningen_naa() {
        // P8: tomgang (utledet, uten å spørre kortet).
        let r = Read { inferred: true, throttle: Some(1), pstate: Some("P8".into()), ..card() };
        assert_eq!(limiter(&r).name, "Ingenting");
        assert!(!limiter(&r).sub.contains("P8"), "P-tilstanden står i flisen ved siden av");
        // Effektgrensen.
        let r = Read { throttle: Some(0x4), util: Some(99.0), ..card() };
        let l = limiter(&r);
        assert_eq!((l.name, l.sub.as_str()), ("Effekt", "Grensen 245\u{202f}W er nådd"));
        // Maskinvaren går foran, og resten telles.
        let r = Read { throttle: Some(0x4 | 0x40), ..card() };
        let l = limiter(&r);
        assert_eq!(l.name, "Temperatur");
        assert!(l.sub.ends_with("+1"), "{}", l.sub);
        assert_eq!(l.tone, "warn");
        // Ingen begrensning under last: spenningsgrensen (utledet).
        let r = Read { throttle: Some(0), util: Some(97.0), ..card() };
        assert_eq!(limiter(&r).name, "Spenning");
        // Ingen begrensning uten last.
        let r = Read { throttle: Some(0), util: Some(10.0), ..card() };
        assert_eq!(limiter(&r).name, "Ingen");
        // Kortet borte.
        assert_eq!(limiter(&Read { lost: true, ..card() }).name, "–");
    }

    #[test]
    fn viftelinja_brytes_ikke_i_o_per_min() {
        let r = Read { rpm: vec![Some(802.0), Some(798.0)], ..card() };
        let s = r.fans_line();
        assert!(s.starts_with("Vifte 1 35"), "{s}");
        assert!(s.contains("o\u{2060}/\u{2060}min"));
    }
}
