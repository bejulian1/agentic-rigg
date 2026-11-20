//! Grafikkort-siden (GPU-Z) som seksjoner: det driveren og sysfs vet om kortet,
//! klokker, P-tilstander, grenser, vifter, det som ikke støttes, prosessene og
//! referansetabellen. Port av `web/js/pages/gpu.js` og `gpufmt.js`, med
//! tilleggene fra maskinvare-revisjon kap. 6.
//!
//! Dataene er `GpuStatic` (med `extra` fra `rigg-hw/src/gpu/nvml_info.rs`) og
//! siste `GpuSample`. Feltnavnene i `extra` står i `nvml_info.rs`.

use super::{Item, Kv, Section, Table};
use crate::fmt as F;
use crate::model::{GpuInfo, GpuSample, GpuStatic};
use crate::{tr, trf, trn};
use serde_json::Value;

pub struct GpuInput<'a> {
    pub info: Option<&'a GpuInfo>,
    pub st: &'a GpuStatic,
    pub s: Option<&'a GpuSample>,
    /// Vis hele UUID-en (øyet på siden, eller «Ta med serienumre» i rapporten).
    pub show_uuid: bool,
    /// Påskeegg 5: («8 år i dag», verktøytips) på raden «Lansert».
    pub egg: Option<(String, String)>,
    pub utc_offset_s: i64,
}

const NB: char = '\u{a0}';

/// PCI-leverandører som kan stå i subsystemet.
pub fn vendor_name(v: &str) -> Option<&'static str> {
    Some(match v.to_ascii_lowercase().as_str() {
        "10de" => "NVIDIA",
        "1002" => "AMD",
        "8086" => "Intel",
        "1569" => "Palit",
        "1462" => "MSI",
        "1043" => "ASUS",
        "3842" => "EVGA",
        "1458" => "Gigabyte",
        "19da" => "Zotac",
        "196e" => "PNY",
        "10b0" => "Gainward",
        "1da2" => "Sapphire",
        _ => return None,
    })
}

/// «GPU-eaf0••••-••••-…»: de fire første tegnene, resten som prikker.
pub fn mask_uuid(u: &str) -> String {
    let (p, rest) = u.strip_prefix("GPU-").map(|r| ("GPU-", r)).unwrap_or(("", u));
    let mut out = String::from(p);
    for (i, c) in rest.chars().enumerate() {
        out.push(if i < 4 || c == '-' { c } else { '•' });
    }
    out
}

/// «Gen 3».
pub fn gen_text(g: Option<f64>) -> String {
    match g {
        Some(g) => format!("Gen{NB}{}", g.round() as i64),
        None => F::DASH.into(),
    }
}

/// «x16 Gen 1 · maks Gen 3», «x8 Gen 3 · maks x16» eller «x16 Gen 3» (som demoen).
pub fn pcie_text(max_w: Option<f64>, max_g: Option<f64>, cur_w: Option<f64>, cur_g: Option<f64>) -> Option<String> {
    let link = |w: Option<f64>, g: Option<f64>| format!("x{} {}", w.map(|w| (w.round() as i64).to_string()).unwrap_or_else(|| "?".into()), gen_text(g));
    if cur_w.is_none() && cur_g.is_none() {
        return (max_w.is_some() || max_g.is_some()).then(|| link(max_w, max_g));
    }
    let now = link(cur_w.or(max_w), cur_g.or(max_g));
    let low_g = matches!((max_g, cur_g), (Some(m), Some(c)) if c < m);
    let low_w = matches!((max_w, cur_w), (Some(m), Some(c)) if c < m);
    if !low_g && !low_w {
        return Some(now);
    }
    let mut tail = String::new();
    if low_w {
        tail.push_str(&format!("x{}", max_w.unwrap_or(0.0).round() as i64));
    }
    if low_w && low_g {
        tail.push(' ');
    }
    if low_g {
        tail.push_str(&gen_text(max_g));
    }
    Some(trf!("{na} · maks {maks}", na = now, maks = tail))
}

fn gt(g: i64) -> Option<f64> {
    Some(match g {
        1 => 2.5,
        2 => 5.0,
        3 => 8.0,
        4 => 16.0,
        5 => 32.0,
        6 => 64.0,
        _ => return None,
    })
}

/// Forklaringen til verktøytipset, med overføringsrate per lane.
pub fn pcie_explain(max_g: Option<f64>, cur_g: Option<f64>) -> Option<String> {
    let (m, c) = (max_g?.round() as i64, cur_g?.round() as i64);
    let rate = |g: i64| match gt(g) {
        Some(r) => trf!(
            "Gen {gen} = {rate} per lane",
            gen = g,
            rate = F::with_unit(F::num(Some(r), if r.fract() == 0.0 { 0 } else { 1 }), "GT/s")
        ),
        None => format!("Gen {g}"),
    };
    Some(if c < m {
        trf!(
            "Lenken går på Gen {na} nå fordi kortet sparer strøm i tomgang, og går opp til Gen {maks} under last. {rate_na}, {rate_maks}.",
            na = c,
            maks = m,
            rate_na = rate(c),
            rate_maks = rate(m)
        )
    } else {
        trf!("Lenken går på full hastighet. {rate}.", rate = rate(c))
    })
}

/// Klokkebegrensningene i masken fra NVML (som `throttleList` i demoen).
pub fn throttle_list(mask: u64) -> Vec<&'static str> {
    type Reason = (u64, fn() -> &'static str);
    let reasons: [Reason; 9] = [
        (0x1, || tr!("Tomgang")),
        (0x2, || tr!("App-klokker")),
        (0x4, || tr!("Effektgrense")),
        (0x8, || tr!("HW-nedklokking")),
        (0x10, || tr!("Sync boost")),
        (0x20, || tr!("Termisk (SW)")),
        (0x40, || tr!("Termisk (HW)")),
        (0x80, || tr!("Strømbrems (HW)")),
        (0x100, || tr!("Skjermklokke")),
    ];
    reasons.iter().filter(|(b, _)| mask & b != 0).map(|(_, t)| t()).collect()
}

fn s(v: Option<&str>) -> Option<String> {
    v.map(str::to_string).filter(|x| !x.is_empty())
}

fn brand_text(brand: &str) -> String {
    match brand {
        "NVIDIA virtuell GPU" => tr!("NVIDIA virtuell GPU").into(),
        "Ukjent" => tr!("Ukjent").into(),
        other => other.to_string(),
    }
}

fn kernel_module_text(km: &str) -> &str {
    match km {
        "Lukket kjernemodul" => tr!("Lukket kjernemodul"),
        other => other,
    }
}

fn proc_kind_text(kind: &str) -> &str {
    match kind {
        "grafikk" => tr!("grafikk"),
        "compute" => tr!("compute"),
        "grafikk og compute" => tr!("grafikk og compute"),
        other => other,
    }
}

fn ex<'a>(st: &'a GpuStatic, k: &str) -> &'a Value {
    st.extra.get(k).unwrap_or(&Value::Null)
}

/// Kode fra `extra.na`, når kallet ga en feil.
fn na_code<'a>(st: &'a GpuStatic, k: &str) -> Option<&'a str> {
    st.extra.get("na")?.get(k)?.as_str()
}

fn yes_no(b: Option<bool>, yes: &str, no: &str) -> Option<String> {
    b.map(|b| if b { yes.to_string() } else { no.to_string() })
}

fn temp_c(v: Option<f64>) -> Option<String> {
    v.map(|v| F::temp(Some(v), 0))
}

/// «Ikke støttet» med koden i forklaringen, eller `None` når feltet bare ikke er lest ennå.
fn unsupported(st: &GpuStatic, k: &str, why: &str) -> (Option<String>, String) {
    match na_code(st, k) {
        Some("NVML_ERROR_NOT_SUPPORTED") => (Some(tr!("Ikke støttet").into()), why.into()),
        Some(code) => (Some(tr!("Kunne ikke leses").into()), format!("{why} ({code})")),
        None => (None, why.into()),
    }
}

pub fn sections(g: &GpuInput) -> Vec<Section> {
    let st = g.st;
    let smp = g.s.cloned().unwrap_or_default();
    let spec = st.pci_device_id.and_then(|d| crate::specs::lookup(d, st.pci_subsystem_id));
    let pci = ex(st, "pci");
    let ids = st.pci_device_id.map(|d| (format!("{:04x}", d & 0xffff), format!("{:04x}", d >> 16)));
    let sub = st.pci_subsystem_id.map(|d| (format!("{:04x}", d & 0xffff), format!("{:04x}", d >> 16)));
    let hex = |p: &Option<(String, String)>| p.as_ref().map(|(v, d)| format!("{}:{}", v.to_uppercase(), d.to_uppercase()));
    let mut out = Vec::new();

    // ---- Fra driveren ----
    let mut d = Section::new("driver", tr!("Fra driveren"));
    d.kv(Kv::new("navn", tr!("Navn"), s(Some(&st.name))));
    d.kv(Kv::new("merke", tr!("Merke"), s(ex(st, "brand").as_str()).map(|v| brand_text(&v))));
    d.kv(match ex(st, "architecture").as_str() {
        Some(a) => Kv::new("arkitektur", tr!("Arkitektur"), Some(a.into())),
        None => Kv::new("arkitektur", tr!("Arkitektur"), spec.as_ref().map(|s| s.architecture.to_string())).info_opt(spec.as_ref().map(|_| tr!("Fra referansetabellen.").to_string())),
    });
    d.kv(Kv::new("brikke", tr!("GPU-brikke"), spec.as_ref().map(|s| format!("{} · {}", s.chip, s.variant))).info(tr!("Slått opp på PCI-ID. NVML oppgir ikke delenummeret.")));
    let dev_hex = hex(&ids);
    d.kv(Kv::new("pci-enhet", tr!("PCI-ID, enhet"), dev_hex.as_ref().map(|h| format!("{h} · {}", ids.as_ref().and_then(|(v, _)| vendor_name(v)).unwrap_or(tr!("ukjent leverandør"))))));
    let same = ids.as_ref().zip(sub.as_ref()).is_some_and(|(a, b)| a.0 == b.0);
    d.kv(
        Kv::new(
            "pci-sub",
            tr!("PCI-ID, subsystem"),
            hex(&sub).map(|h| format!("{h} · {}", if same { tr!("samme som enheten") } else { sub.as_ref().and_then(|(v, _)| vendor_name(v)).unwrap_or(tr!("ukjent produsent")) })),
        )
        .info_opt(same.then(|| tr!("Subsystemet melder NVIDIAs egen ID, ikke kortprodusentens. Derfor kan ikke appen lese hvem som har laget kortet.").into())),
    );
    d.kv(Kv::new("bus", tr!("Bus-ID"), s(Some(&st.id))).info_opt(pci["driver"].as_str().map(|k| trf!("Kjernedriver: {driver}", driver = k))));
    d.kv(Kv::new("revisjon", tr!("Revisjon"), pci["revision"].as_str().map(|r| format!("0x{}", r.trim_start_matches("0x").trim_start_matches("0X").to_uppercase()))));
    d.kv(Kv::new("vbios", "VBIOS", st.vbios.clone()));
    let km = ex(st, "kernelModule").as_str().map(kernel_module_text);
    d.kv(Kv::new("driver", tr!("Driver"), st.driver.clone().map(|v| match km {
        Some(k) => format!("{v} · {k}"),
        None => v,
    })));
    d.kv(Kv::new("nvml", "NVML", st.nvml_version.clone()));
    d.kv(Kv::new("cuda", tr!("CUDA (driver)"), st.cuda_version.clone()));
    d.kv(Kv::new("cc", tr!("Compute capability"), s(ex(st, "computeCap").as_str())));
    let gsp = ex(st, "gspFirmware").as_str().map(|v| {
        let on = ex(st, "gspMode")["enabled"].as_bool();
        match on {
            Some(true) => trf!("{versjon} · på", versjon = v),
            Some(false) => trf!("{versjon} · av", versjon = v),
            None => v.to_string(),
        }
    });
    d.kv(Kv::new("gsp", tr!("GSP-fastvare"), gsp).info(tr!("Fastvaren på kortet som driveren snakker med. «På» er standard med åpen kjernemodul.")));
    let inforom = ex(st, "inforomImage").as_str().map(|i| match ex(st, "inforomOem").as_str() {
        Some(o) => format!("{i} · OEM {o}"),
        None => i.to_string(),
    });
    d.kv(Kv::new("inforom", "InfoROM", inforom));
    match &st.uuid {
        Some(u) => d.kv(Kv::new("uuid", "UUID", Some(if g.show_uuid { u.clone() } else { mask_uuid(u) }))),
        None => d.kv(Kv::new("uuid", "UUID", None)),
    };
    let (na, why) = unsupported(st, "serial", tr!("NVML svarer «ikke støttet». Det er vanlig for GeForce-kort."));
    let serial = ex(st, "serial").as_str().map(|v| if g.show_uuid { v.to_string() } else { tr!("Skjult").into() });
    d.kv(Kv { id: "serie", k: tr!("Serienummer").into(), v: serial, na: na.map(|_| tr!("Ikke oppgitt av kortet").into()), info: Some(why) });
    d.kv(Kv::new("persistence", tr!("Persistence-modus"), yes_no(ex(st, "persistence").as_bool(), tr!("På"), tr!("Av"))).info(tr!("Holder driveren lastet når ingen program bruker kortet.")));
    d.kv(
        Kv::new("skjerm", tr!("Skjerm aktiv (driverens flagg)"), yes_no(ex(st, "displayActive").as_bool(), tr!("Ja"), tr!("Nei")))
            .info(tr!("NVML displayActive. Med den åpne kjernemodulen på Wayland står det ofte «Nei» selv når en skjerm er koblet til.")),
    );
    out.push(d);

    // ---- Minne og buss ----
    let mut b = Section::new("buss", tr!("Minne og buss"));
    b.kv(Kv::new("vram", tr!("Videominne"), st.vram_total.or(smp.vram_total).map(|v| F::mib(Some(v), None, None))).info_opt(smp.vram_reserved.map(|r| trf!("{minne} er reservert av driveren og regnes ikke som brukt (MemoryInfo_v2).", minne = F::mib(Some(r), None, None)))));
    b.kv(Kv::new("minnebuss", tr!("Minnebuss"), ex(st, "busWidth").as_u64().map(|w| F::with_unit(w.to_string(), "bit"))).info(tr!("Lest fra driveren, ikke fra referansetabellen.")));
    b.kv(Kv::new("cuda-kjerner", tr!("CUDA-kjerner"), ex(st, "cudaCores").as_f64().map(|c| F::num(Some(c), 0))).info(tr!("Lest fra driveren.")));
    let pcie = pcie_text(st.pcie_width_max, st.pcie_gen_max, smp.pcie_width, smp.pcie_gen);
    let mut tip = vec![];
    if let Some(e) = pcie_explain(st.pcie_gen_max, smp.pcie_gen) {
        tip.push(e);
    }
    if let (Some(a), Some(b2), Some(c), Some(d2)) = (pci["linkSpeed"].as_str(), pci["linkWidth"].as_u64(), pci["linkSpeedMax"].as_str(), pci["linkWidthMax"].as_u64()) {
        tip.push(trf!("sysfs: {hastighet} x{bredde}, maks {maks_hastighet} x{maks_bredde}", hastighet = a, bredde = b2, maks_hastighet = c, maks_bredde = d2));
    }
    b.kv(Kv::new("pcie", "PCIe", pcie).info(tip.join("\n")));
    let (gm, sm) = (ex(st, "pcieGenMaxGpu").as_f64(), ex(st, "pcieGenMaxSys").as_f64());
    b.kv(Kv::new(
        "pcie-maks",
        tr!("PCIe maks"),
        match (gm, sm) {
            (Some(a), Some(c)) => Some(trf!("Kortet {kort} · hovedkortet {hovedkort}", kort = gen_text(Some(a)), hovedkort = gen_text(Some(c)))),
            (Some(a), None) => Some(trf!("Kortet {kort}", kort = gen_text(Some(a)))),
            _ => None,
        },
    ));
    let bar1 = ex(st, "bar1");
    b.kv(Kv::new(
        "bar1",
        "BAR1",
        bar1["totalMiB"].as_f64().map(|t| match bar1["usedMiB"].as_f64() {
            Some(u) => trf!("{totalt} · {brukt} i bruk", totalt = F::mib(Some(t), None, None), brukt = F::mib(Some(u), None, None)),
            None => F::mib(Some(t), None, None),
        }),
    ));
    b.kv(Kv::new("rebar", "Resizable BAR", rebar_text(pci, st.vram_total, bar1["totalMiB"].as_f64())).info(tr!("Lest fra sysfs (resource1 og resource1_resize).")));
    let replay = ex(st, "pcieReplay").as_u64();
    let aer = pci["aerCorrectable"].as_u64();
    let nb = crate::i18n::active() == crate::i18n::Language::Nb;
    b.kv(Kv::new(
        "pcie-feil",
        tr!("PCIe-feil"),
        match (replay, aer) {
            (None, None) => None,
            _ => Some(
                [
                    replay.map(|r| if nb {
                        format!("{r} gjentatte sendinger")
                    } else {
                        trn!(r, "{antall} gjentatt sending", "{antall} gjentatte sendinger", antall = F::num(Some(r as f64), 0))
                    }),
                    aer.map(|a| if nb {
                        format!("{a} korrigerte")
                    } else {
                        trn!(a, "{antall} korrigert", "{antall} korrigerte", antall = F::num(Some(a as f64), 0))
                    }),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            ),
        },
    )
    .info(tr!("Telleren for gjentatte sendinger (replay) fra NVML og korrigerte AER-feil fra sysfs, siden oppstart. Null er normalt.")));
    out.push(b);

    // ---- Klokker ----
    let mut c = Section::new("klokker", tr!("Klokker"));
    c.aside = Some("MHz".into());
    let rows = [(tr!("Kjerne"), smp.core_clock, st.core_max), ("SM", smp.sm_clock, st.sm_max), (tr!("Minne"), smp.mem_clock, st.mem_max), (tr!("Video"), smp.video_clock, st.video_max)];
    c.items.push(Item::Table(Table {
        id: "klokker",
        head: vec![String::new(), tr!("Nå").into(), tr!("Maks (driver)").into()],
        end: vec![false, true, true],
        rows: rows.iter().map(|(n, now, max)| vec![n.to_string(), F::num(*now, 0), F::num(*max, 0)]).collect(),
    }));
    let mut note = vec![];
    if na_code(st, "defaultAppClockGfx").is_some() || na_code(st, "appClockGfx").is_some() {
        note.push(tr!("Driveren oppgir ikke standardklokker (applications clocks er avviklet).").to_string());
    }
    if let Some(sp) = &spec {
        note.push(trf!("Boost etter spesifikasjonen er {klokke} (referanse, ikke målt).", klokke = F::mhz(Some(sp.boost_mhz as f64))));
    }
    if let Some(m) = smp.throttle {
        let l = throttle_list(m);
        let what = if l.is_empty() {
            tr!("ingen").to_string()
        } else if crate::i18n::active() == crate::i18n::Language::Nb {
            l.join(", ").to_lowercase()
        } else {
            l.join(", ")
        };
        note.push(if smp.throttle_inferred {
            trf!("Begrenser nå: {arsaker} (i P8 leses ikke årsaken; kortet er i tomgang).", arsaker = what)
        } else {
            trf!("Begrenser nå: {arsaker}.", arsaker = what)
        });
    }
    c.items.push(Item::Note(note.join(" ")));
    out.push(c);

    // ---- P-tilstander ----
    let pst = ex(st, "pstateClocks").as_array();
    if let Some(list) = pst.filter(|l| !l.is_empty()) {
        let mut p = Section::new("pstates", tr!("P-tilstander"));
        p.aside = Some("MHz".into());
        let range = |v: &Value| match (v.get(0).and_then(Value::as_f64), v.get(1).and_then(Value::as_f64)) {
            (Some(a), Some(b)) if a == b => F::num(Some(a), 0),
            (Some(a), Some(b)) => format!("{}–{}", F::num(Some(a), 0), F::num(Some(b), 0)),
            _ => F::DASH.into(),
        };
        let now = smp.pstate.clone();
        p.items.push(Item::Table(Table {
            id: "pstates",
            head: vec![String::new(), tr!("Kjerne").into(), tr!("Minne").into()],
            end: vec![false, true, true],
            rows: list
                .iter()
                .map(|r| {
                    let name = r["p"].as_str().unwrap_or("?").to_string();
                    let label = if now.as_deref() == Some(name.as_str()) { trf!("{navn} · nå", navn = name) } else { name };
                    vec![label, range(&r["gfx"]), range(&r["mem"])]
                })
                .collect(),
        }));
        let mut n = vec![tr!("Område per P-tilstand fra driveren. P0 er full ytelse, P8 er tomgang.").to_string()];
        if let Some(mc) = ex(st, "memClocks").as_array().filter(|a| !a.is_empty()) {
            n.push(trf!("Støttede minneklokker: {klokker} MHz.", klokker = mc.iter().filter_map(Value::as_f64).map(|v| F::num(Some(v), 0)).collect::<Vec<_>>().join(", ")));
        }
        p.items.push(Item::Note(n.join(" ")));
        out.push(p);
    }

    // ---- Grenser ----
    let mut l = Section::new("grenser", tr!("Grenser"));
    let plimit = smp.power_limit.or_else(|| ex(st, "powerLimit").as_f64());
    l.kv(Kv::new("effektgrense", tr!("Effektgrense nå"), plimit.map(|w| F::watt(Some(w), 0))).info(tr!("Håndhevet grense fra driveren.")));
    l.kv(Kv::new(
        "effektomrade",
        tr!("Standard / område"),
        match (st.power_default, st.power_min, st.power_max) {
            (Some(d), Some(a), Some(b)) => Some(format!("{} · {}–{}", F::watt(Some(d), 0), F::num(Some(a), 0), F::watt(Some(b), 0))),
            _ => None,
        },
    ));
    let (amin, amax) = (ex(st, "acousticMin").as_f64(), ex(st, "acousticMax").as_f64());
    l.kv(
        Kv::new(
            "tempmal",
            tr!("Temperaturmål"),
            st.temp_target.map(|t| match (amin, amax) {
                (Some(a), Some(b)) => trf!("{mal} · område {min}–{maks}", mal = F::temp(Some(t), 0), min = F::temp_num(Some(a), 0), maks = F::temp(Some(b), 0)),
                _ => F::temp(Some(t), 0),
            }),
        )
        .info(tr!("Akustisk mål: kortet senker klokken for å holde seg her.")),
    );
    l.kv(Kv::new("tempmaks", tr!("Maks driftstemperatur"), temp_c(st.temp_max)));
    l.kv(Kv::new("nedklokking", tr!("Nedklokking"), temp_c(st.temp_slowdown)));
    l.kv(Kv::new("avstenging", tr!("Avstenging"), temp_c(st.temp_shutdown)));
    l.kv(
        Kv::new("avstand", tr!("Avstand til nedklokking"), st.temp_slowdown.zip(smp.temp).map(|(a, t)| F::temp_delta(a - t, 0)))
            .info(tr!("Utregnet: nedklokkingsgrensen minus temperaturen nå. Dette er ikke en direkte måling av T.Limit.")),
    );
    let off = |k: &str, div: f64| -> Option<String> {
        let o = ex(st, k);
        let (n, a, b) = (o["now"].as_f64()? / div, o["min"].as_f64()? / div, o["max"].as_f64()? / div);
        Some(trf!("{na} · område {min}…{maks}", na = F::with_unit(F::signed(n, 0), "MHz"), min = F::signed(a, 0), maks = F::with_unit(F::signed(b, 0), "MHz")))
    };
    l.kv(Kv::new("offset-kjerne", tr!("Klokkeoffset, kjerne"), off("offsetGfx", 1.0)).info(tr!("Lest med NVML. Klokker og effekt er låst; appen endrer ikke offset.")));
    l.kv(
        Kv::new("offset-minne", tr!("Klokkeoffset, minne"), off("offsetMem", 2.0))
            .info(tr!("I effektiv MHz, som Klokker og effekt bruker. NVML oppgir minneoffset i dobbel dataklokke (dobbelt så store tall).")),
    );
    out.push(l);

    // ---- Vifter og kjøling ----
    let mut f = Section::new("vifter", tr!("Vifter og kjøling"));
    let fans = ex(st, "fans").as_array().cloned().unwrap_or_default();
    let count = st.fan_count.map(|n| n as usize).or((!smp.fans.is_empty()).then_some(smp.fans.len()));
    let signal = fans.first().and_then(|x| x["signal"].as_i64());
    let target = fans.first().and_then(|x| x["coolerTarget"].as_i64());
    let cooler_tip = match (signal, target) {
        (Some(sg), Some(t)) => {
            let sig = match sg {
                1 => tr!("av/på"),
                2 => tr!("variabel fart"),
                _ => tr!("ukjent styring"),
            };
            let mut parts = vec![];
            for (bit, n) in [(2, "GPU"), (4, tr!("minne")), (8, tr!("strømforsyning"))] {
                if t & bit != 0 {
                    parts.push(n);
                }
            }
            Some(trf!("Kjølerinfo fra driveren: {signal}, kjøler {mal}.", signal = sig, mal = if parts.is_empty() { tr!("ukjent").into() } else { parts.join(", ") }))
        }
        _ => None,
    };
    f.kv(
        Kv::new(
            "antall",
            tr!("Vifter"),
            count.map(|n| match (st.fan_min, st.fan_max) {
                (Some(a), Some(b)) => format!("{n} · {}–{}", F::num(Some(a), 0), F::pct(Some(b), 0)),
                _ => n.to_string(),
            }),
        )
        .info_opt(cooler_tip),
    );
    for i in 0..count.unwrap_or(0).min(8) {
        let pct = smp.fans.get(i).copied().flatten();
        let rpm = smp.fan_rpm.get(i).copied().flatten();
        let tgt = fans.get(i).and_then(|x| x["target"].as_f64());
        let mut parts = vec![F::pct(pct, 0)];
        if rpm.is_some() {
            parts.push(F::rpm(rpm));
        } else if smp.na.iter().any(|n| n == "fanRpm") {
            parts.push(tr!("turtall ikke støttet").into());
        }
        if let Some(t) = tgt {
            parts.push(trf!("mål {prosent}", prosent = F::pct(Some(t), 0)));
        }
        let id: &'static str = ["vifte-1", "vifte-2", "vifte-3", "vifte-4", "vifte-5", "vifte-6", "vifte-7", "vifte-8"][i];
        f.kv(Kv::new(id, &trf!("Vifte {nr}", nr = i + 1), pct.map(|_| parts.join(" · "))));
    }
    let policies: Vec<i64> = fans.iter().filter_map(|x| x["policy"].as_i64()).collect();
    f.kv(
        Kv::new(
            "styring",
            tr!("Styring"),
            (!policies.is_empty()).then(|| if policies.iter().all(|p| *p == 0) { tr!("Auto · driveren styrer").into() } else { tr!("Manuell · rapportert av driveren").into() }),
        )
        .info(tr!("Lest med NVML (viftepolicy). Viser driverens rapporterte modus; angir ikke hvilket program som satte den.")),
    );
    let th = ex(st, "thermal").as_array().cloned().unwrap_or_default();
    f.kv(Kv::new(
        "sensorer",
        tr!("Termiske sensorer"),
        (!th.is_empty()).then(|| {
            let first = &th[0];
            let what = if first["controller"].as_i64() == Some(1) {
                tr!("intern GPU-kontroller").to_string()
            } else {
                trf!("kontroller {id}", id = first["controller"])
            };
            format!(
                "{} · {what}, {}–{}",
                th.len(),
                F::temp_num(first["min"].as_f64(), 0),
                F::temp(first["max"].as_f64(), 0)
            )
        }),
    ));
    out.push(f);

    // ---- Ikke støttet ----
    let mut n = Section::new("ikke-stottet", tr!("Datatilgjengelighet"));
    n.aside = Some(tr!("ikke innsamlet eller utilgjengelig").into());
    n.kv(Kv::new("hotspot", tr!("Hotspot-temperatur"), None).na(tr!("Ikke innsamlet")).info(tr!("Appen samler ikke inn en egen hotspot-temperatur. De termiske sensorene som driveren rapporterer, vises over; dette avklarer ikke alene om kortet har en hotspot-sensor.")));
    let (mna, mwhy) = unsupported(st, "memMax", tr!("Appen har ingen tilgjengelig minnetemperatur fra NVML for denne prøven. Tilgjengelighet avhenger av kortet og driveren."));
    n.kv(Kv { id: "minnetemp", k: tr!("Minnetemperatur").into(), v: None, na: Some(mna.unwrap_or_else(|| tr!("Ikke verifisert").into())), info: Some(mwhy) });
    let (tna, twhy) = unsupported(st, "margin", tr!("Appen har ingen tilgjengelig T.Limit-margin fra NVML for denne prøven. Avstanden til nedklokking under Grenser er regnet ut av appen."));
    n.kv(Kv { id: "tlimit", k: tr!("T.Limit (margin)").into(), v: None, na: Some(tna.unwrap_or_else(|| tr!("Ikke verifisert").into())), info: Some(twhy) });
    n.kv(Kv::new("spenning", tr!("Kjernespenning"), None).na(tr!("Ikke innsamlet")).info(tr!("Appen samler ikke inn GPU-kjernespenning. Tilgang for dette kortet og denne driveren er ikke verifisert.")));
    n.kv(Kv::new("snitteffekt", tr!("Gjennomsnittseffekt fra driveren"), None).na(tr!("Ikke innsamlet")).info(tr!("Appen samler ikke inn driverens eget effektgjennomsnitt. Effekten beregnes fra energitelleren når den finnes, ellers brukes den rapporterte øyeblikksverdien.")));
    if na_code(st, "defaultAppClockGfx").is_some() || na_code(st, "customerBoostGfx").is_some() {
        n.kv(Kv::new("appklokker", tr!("Standardklokker (applications clocks)"), None).na(tr!("Avviklet")).info(tr!("Avviklet i driveren for dette kortet.")));
    }
    if na_code(st, "partNumber").is_some() {
        n.kv(Kv::new("delenummer", tr!("Delenummer (board part number)"), None).na(tr!("Ikke oppgitt")).info(tr!("Driveren svarer, men feltet er tomt på dette kortet.")));
    }
    // Verdier i prøven som kortet ikke støtter (huskes fra første forsøk).
    for f in &smp.na {
        let (label, why) = match f.as_str() {
            "fanRpm" => (tr!("Viftens turtall"), tr!("NVML gir «ikke støttet» for turtallet.")),
            "encUtil" => (tr!("Videokoder"), tr!("NVML gir «ikke støttet» for koderen.")),
            "decUtil" => (tr!("Videodekoder"), tr!("NVML gir «ikke støttet» for dekoderen.")),
            "throttle" => (tr!("Klokkebegrensninger"), tr!("NVML gir «ikke støttet» for årsakene.")),
            "power" => (tr!("Effekttrekk"), tr!("Verken energitelleren eller effekten kan leses.")),
            "energy" => (tr!("Energiteller"), tr!("Effekten leses som øyeblikksverdi i stedet.")),
            "temp" => (tr!("Temperatur"), tr!("NVML gir «ikke støttet» for temperaturen.")),
            _ => continue,
        };
        let id: &'static str = match f.as_str() {
            "fanRpm" => "na-fanrpm",
            "encUtil" => "na-enc",
            "decUtil" => "na-dec",
            "throttle" => "na-throttle",
            "power" => "na-power",
            "energy" => "na-energy",
            _ => "na-temp",
        };
        n.kv(Kv::new(id, label, None).na(tr!("Ikke støttet")).info(why));
    }
    out.push(n);

    // ---- Prosesser ----
    let procs = ex(st, "processes");
    let mut p = Section::new("prosesser", tr!("Prosesser som bruker skjermkortet"));
    let list = procs["list"].as_array().cloned().unwrap_or_default();
    if procs.is_object() {
        p.items.push(Item::Table(Table {
            id: "prosesser",
            head: vec![tr!("Prosess").into(), "PID".into(), tr!("Type").into(), tr!("Videominne").into()],
            end: vec![false, true, false, true],
            rows: list
                .iter()
                .map(|r| vec![r["name"].as_str().unwrap_or("–").to_string(), r["pid"].to_string(), proc_kind_text(r["kind"].as_str().unwrap_or("")).to_string(), F::mib(r["vramMiB"].as_f64(), None, None)])
                .collect(),
        }));
        let when = procs["t"].as_i64().map(|t| F::time(F::local(t, g.utc_offset_s), true)).unwrap_or_else(|| "–".into());
        let live = procs["live"] == true;
        p.items.push(Item::Note(match (list.is_empty(), live) {
            (true, false) => trf!("Ingen prosesser brukte skjermkortet kl. {tid}.", tid = when),
            (true, true) => trf!("Ingen prosesser bruker skjermkortet (lest kl. {tid}). Lista oppdateres hvert 5. s mens siden vises.", tid = when),
            (false, false) => trf!("Øyeblikksbilde fra kl. {tid}, da tjenesten leste de faste dataene. Videominne per prosess fra driveren.", tid = when),
            (false, true) => trf!("Lest kl. {tid}. Lista oppdateres hvert 5. s mens siden vises. Videominne per prosess fra driveren.", tid = when),
        }));
    } else {
        p.items.push(Item::Note(tr!("Lista leses sammen med de faste dataene.").into()));
    }
    out.push(p);

    // ---- Spesifikasjon ----
    let mut r = Section::new("spesifikasjon", tr!("Spesifikasjon"));
    r.aside = Some(tr!("Referanse, ikke målt").into());
    match &spec {
        None => r.items.push(Item::Note(trf!("Ingen oppføring for PCI-ID {id} i den innebygde tabellen.", id = dev_hex.unwrap_or_else(|| tr!("ukjent").into())))),
        Some(sp) => {
            let u = |v: String, unit: &str| Some(F::with_unit(v, unit));
            let dv = |x: crate::specs::Derived| Kv::new("avledet", "", u(F::num(Some(x.value), if x.value < 100.0 { 1 } else { 0 }), x.unit)).info(x.formula);
            r.items.push(Item::Sub(tr!("Brikke").into()));
            r.kv(Kv::new("modell", tr!("Modell"), Some(sp.name.into())));
            r.kv(Kv::new("brikke", tr!("Brikke"), Some(format!("{} · {}", sp.chip, sp.variant))));
            r.kv(Kv::new("arkitektur", tr!("Arkitektur"), Some(sp.architecture.into())));
            r.kv(Kv::new("prosess", tr!("produksjonsprosess", "Prosess"), Some(sp.process.into())));
            r.kv(Kv::new("storrelse", tr!("Brikkestørrelse"), u(F::num(Some(sp.die_mm2 as f64), 0), "mm²")));
            r.kv(Kv::new("transistorer", tr!("Transistorer"), Some(trf!("{antall} mrd.", antall = F::num(Some(sp.transistors_bn), 1)))));
            r.kv(Kv::new("cuda", tr!("CUDA-kjerner"), Some(F::num(Some(sp.cuda as f64), 0))));
            r.kv(Kv::new("sm", tr!("SM-er"), Some(F::num(Some(sp.sm as f64), 0))));
            r.kv(Kv::new("tensor", "Tensor / RT", Some(format!("{} / {}", sp.tensor, sp.rt))));
            r.kv(Kv::new("tmu", "TMU / ROP", Some(format!("{} / {}", sp.tmu, sp.rop))));
            r.kv(Kv::new("l2", tr!("L2-cache"), u(F::num(Some(sp.l2_kb as f64 / 1024.0), 0), "MiB")));
            r.kv(Kv::new("grunnklokke", tr!("Grunnklokke"), Some(F::mhz(Some(sp.base_mhz as f64)))));
            r.kv(Kv::new("boost", tr!("Boostklokke"), Some(F::mhz(Some(sp.boost_mhz as f64)))));
            r.items.push(Item::Sub(tr!("Minne").into()));
            r.kv(Kv::new("type", tr!("Type"), Some(sp.mem_type.into())));
            r.kv(Kv::new("buss", tr!("Buss"), u(sp.bus_width.to_string(), "bit")));
            r.kv(Kv::new("mengde", tr!("Mengde"), u(sp.mem_gb.to_string(), "GB")));
            r.kv(Kv::new("hastighet", tr!("Hastighet"), Some(trf!("{verdi} effektivt", verdi = F::with_unit(F::num(Some(sp.mem_gbps), if sp.mem_gbps.fract() == 0.0 { 0 } else { 1 }), "Gbps")))));
            r.kv(Kv { k: tr!("Båndbredde").into(), ..dv(sp.bandwidth()) });
            r.kv(Kv::new("leverandor", tr!("Leverandør"), None).na(tr!("Ukjent på Linux")).info(sp.mem_vendor_note_text()));
            r.items.push(Item::Sub(tr!("Ytelse og kort").into()));
            r.kv(Kv { k: tr!("Pikselfyllrate").into(), ..dv(sp.pixel_fill()) });
            r.kv(Kv { k: tr!("Teksturfyllrate").into(), ..dv(sp.texture_fill()) });
            r.kv(Kv { k: "FP32".into(), ..dv(sp.fp32()) });
            r.kv(Kv::new("tdp", tr!("TDP (referansekort)"), Some(F::watt(Some(sp.tdp_w as f64), 0))).info(tr!("Kortprodusenten kan sette en annen grense. Driveren oppgir standardgrensen under Grenser.")));
            r.kv(Kv::new("pcie", "PCIe", pcie_spec(sp.bus)).info(sp.bus));
            r.kv(Kv::new("api", "DirectX / Vulkan / OpenGL", Some(format!("{} / {} / {}", sp.directx, sp.vulkan, sp.opengl))));
            let launched_date = crate::specs::launch_date(sp.launched);
            let launched = match &g.egg {
                Some((t, _)) => format!("{launched_date} · {t}"),
                None => launched_date,
            };
            r.kv(Kv::new("lansert", tr!("Lansert"), Some(launched)).info_opt(g.egg.as_ref().map(|e| e.1.clone())));
        }
    }
    out.push(r);
    out
}

/// Spesifikasjonens «PCIe 3.0 x16» i samme form som resten: «x16 Gen 3».
pub fn pcie_spec(s: &str) -> Option<String> {
    let rest = s.trim().strip_prefix("PCIe")?.trim();
    let (ver, width) = rest.split_once(" x")?;
    let gen_n: f64 = ver.split('.').next()?.parse().ok()?;
    let w: f64 = width.trim().parse().ok()?;
    pcie_text(Some(w), Some(gen_n), None, None)
}

/// Resizable BAR som demoen: på, av eller ikke støttet ut fra BAR1 og videominnet.
fn rebar_text(pci: &Value, vram: Option<f64>, bar1_mib: Option<f64>) -> Option<String> {
    if !pci.is_object() {
        return None;
    }
    let bar1 = pci["bars"].as_array().and_then(|b| b.iter().find(|x| x["bar"] == 1)).and_then(|x| x["size"].as_f64()).map(|s| s / 1024.0 / 1024.0).or(bar1_mib);
    if let (Some(b), Some(v)) = (bar1, vram)
        && b >= v
    {
        return Some(tr!("På · BAR1 dekker hele videominnet").into());
    }
    if let Some(sizes) = pci["bar1Sizes"].as_array().filter(|a| !a.is_empty()) {
        let max = sizes.iter().filter_map(Value::as_f64).fold(0.0, f64::max);
        if vram.is_some_and(|v| max < v) {
            return Some(trf!("Ikke støttet · største BAR1 er {storrelse}", storrelse = F::mib(Some(max), None, None)));
        }
        return Some(trf!("Av · kortet støtter opptil {storrelse}", storrelse = F::mib(Some(max), None, None)));
    }
    bar1.map(|b| trf!("Av · BAR1 er {storrelse}", storrelse = F::mib(Some(b), None, None)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcie_som_demoen() {
        assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(16.0), Some(1.0)).unwrap(), "x16 Gen\u{a0}1 · maks Gen\u{a0}3");
        assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(16.0), Some(3.0)).unwrap(), "x16 Gen\u{a0}3");
        assert_eq!(pcie_text(Some(16.0), Some(3.0), Some(8.0), Some(3.0)).unwrap(), "x8 Gen\u{a0}3 · maks x16");
        assert_eq!(pcie_spec("PCIe 3.0 x16").unwrap(), "x16 Gen\u{a0}3");
        assert!(pcie_explain(Some(3.0), Some(1.0)).unwrap().contains("sparer strøm"));
    }

    #[test]
    fn uuid_maskeres() {
        assert_eq!(mask_uuid("GPU-eaf05f06-1234-abcd"), "GPU-eaf0••••-••••-••••");
        assert_eq!(throttle_list(0x5), vec!["Tomgang", "Effektgrense"]);
    }
}
