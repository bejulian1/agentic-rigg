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
    Some(format!("{now} · maks {tail}"))
}

fn gt(g: i64) -> Option<&'static str> {
    Some(match g {
        1 => "2,5",
        2 => "5",
        3 => "8",
        4 => "16",
        5 => "32",
        6 => "64",
        _ => return None,
    })
}

/// Forklaringen til verktøytipset, med overføringsrate per lane.
pub fn pcie_explain(max_g: Option<f64>, cur_g: Option<f64>) -> Option<String> {
    let (m, c) = (max_g?.round() as i64, cur_g?.round() as i64);
    let rate = |g: i64| match gt(g) {
        Some(r) => format!("Gen {g} = {r}{}GT/s per lane", F::NNBSP),
        None => format!("Gen {g}"),
    };
    Some(if c < m {
        format!("Lenken går på Gen {c} nå fordi kortet sparer strøm i tomgang, og går opp til Gen {m} under last. {}, {}.", rate(c), rate(m))
    } else {
        format!("Lenken går på full hastighet. {}.", rate(c))
    })
}

/// Klokkebegrensningene i masken fra NVML (som `throttleList` i demoen).
pub fn throttle_list(mask: u64) -> Vec<&'static str> {
    const R: [(u64, &str); 9] = [
        (0x1, "Tomgang"),
        (0x2, "App-klokker"),
        (0x4, "Effektgrense"),
        (0x8, "HW-nedklokking"),
        (0x10, "Sync boost"),
        (0x20, "Termisk (SW)"),
        (0x40, "Termisk (HW)"),
        (0x80, "Strømbrems (HW)"),
        (0x100, "Skjermklokke"),
    ];
    R.iter().filter(|(b, _)| mask & b != 0).map(|(_, t)| *t).collect()
}

fn s(v: Option<&str>) -> Option<String> {
    v.map(str::to_string).filter(|x| !x.is_empty())
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
        Some("NVML_ERROR_NOT_SUPPORTED") => (Some("Ikke støttet".into()), why.into()),
        Some(code) => (Some("Kunne ikke leses".into()), format!("{why} ({code})")),
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
    let mut d = Section::new("driver", "Fra driveren");
    d.kv(Kv::new("navn", "Navn", s(Some(&st.name))));
    d.kv(Kv::new("merke", "Merke", s(ex(st, "brand").as_str())));
    d.kv(match ex(st, "architecture").as_str() {
        Some(a) => Kv::new("arkitektur", "Arkitektur", Some(a.into())),
        None => Kv::new("arkitektur", "Arkitektur", spec.as_ref().map(|s| s.architecture.to_string())).info_opt(spec.as_ref().map(|_| "Fra referansetabellen.".to_string())),
    });
    d.kv(Kv::new("brikke", "GPU-brikke", spec.as_ref().map(|s| format!("{} · {}", s.chip, s.variant))).info("Slått opp på PCI-ID. NVML oppgir ikke delenummeret."));
    let dev_hex = hex(&ids);
    d.kv(Kv::new("pci-enhet", "PCI-ID, enhet", dev_hex.as_ref().map(|h| format!("{h} · {}", ids.as_ref().and_then(|(v, _)| vendor_name(v)).unwrap_or("ukjent leverandør")))));
    let same = ids.as_ref().zip(sub.as_ref()).is_some_and(|(a, b)| a.0 == b.0);
    d.kv(
        Kv::new(
            "pci-sub",
            "PCI-ID, subsystem",
            hex(&sub).map(|h| format!("{h} · {}", if same { "samme som enheten" } else { sub.as_ref().and_then(|(v, _)| vendor_name(v)).unwrap_or("ukjent produsent") })),
        )
        .info_opt(same.then(|| "Subsystemet melder NVIDIAs egen ID, ikke kortprodusentens. Derfor kan ikke appen lese hvem som har laget kortet.".into())),
    );
    d.kv(Kv::new("bus", "Bus-ID", s(Some(&st.id))).info_opt(pci["driver"].as_str().map(|k| format!("Kjernedriver: {k}"))));
    d.kv(Kv::new("revisjon", "Revisjon", pci["revision"].as_str().map(|r| format!("0x{}", r.trim_start_matches("0x").trim_start_matches("0X").to_uppercase()))));
    d.kv(Kv::new("vbios", "VBIOS", st.vbios.clone()));
    let km = ex(st, "kernelModule").as_str();
    d.kv(Kv::new("driver", "Driver", st.driver.clone().map(|v| match km {
        Some(k) => format!("{v} · {k}"),
        None => v,
    })));
    d.kv(Kv::new("nvml", "NVML", st.nvml_version.clone()));
    d.kv(Kv::new("cuda", "CUDA (driver)", st.cuda_version.clone()));
    d.kv(Kv::new("cc", "Compute capability", s(ex(st, "computeCap").as_str())));
    let gsp = ex(st, "gspFirmware").as_str().map(|v| {
        let on = ex(st, "gspMode")["enabled"].as_bool();
        match on {
            Some(true) => format!("{v} · på"),
            Some(false) => format!("{v} · av"),
            None => v.to_string(),
        }
    });
    d.kv(Kv::new("gsp", "GSP-fastvare", gsp).info("Fastvaren på kortet som driveren snakker med. «På» er standard med åpen kjernemodul."));
    let inforom = ex(st, "inforomImage").as_str().map(|i| match ex(st, "inforomOem").as_str() {
        Some(o) => format!("{i} · OEM {o}"),
        None => i.to_string(),
    });
    d.kv(Kv::new("inforom", "InfoROM", inforom));
    match &st.uuid {
        Some(u) => d.kv(Kv::new("uuid", "UUID", Some(if g.show_uuid { u.clone() } else { mask_uuid(u) }))),
        None => d.kv(Kv::new("uuid", "UUID", None)),
    };
    let (na, why) = unsupported(st, "serial", "NVML svarer «ikke støttet». Det er vanlig for GeForce-kort.");
    let serial = ex(st, "serial").as_str().map(|v| if g.show_uuid { v.to_string() } else { "Skjult".into() });
    d.kv(Kv { id: "serie", k: "Serienummer".into(), v: serial, na: na.map(|_| "Ikke oppgitt av kortet".into()), info: Some(why) });
    d.kv(Kv::new("persistence", "Persistence-modus", yes_no(ex(st, "persistence").as_bool(), "På", "Av")).info("Holder driveren lastet når ingen program bruker kortet."));
    d.kv(
        Kv::new("skjerm", "Skjerm aktiv (driverens flagg)", yes_no(ex(st, "displayActive").as_bool(), "Ja", "Nei"))
            .info("NVML displayActive. Med den åpne kjernemodulen på Wayland står det ofte «Nei» selv når en skjerm er koblet til."),
    );
    out.push(d);

    // ---- Minne og buss ----
    let mut b = Section::new("buss", "Minne og buss");
    b.kv(Kv::new("vram", "Videominne", st.vram_total.or(smp.vram_total).map(|v| F::mib(Some(v), None, None))).info_opt(smp.vram_reserved.map(|r| format!("{} er reservert av driveren og regnes ikke som brukt (MemoryInfo_v2).", F::mib(Some(r), None, None)))));
    b.kv(Kv::new("minnebuss", "Minnebuss", ex(st, "busWidth").as_u64().map(|w| F::with_unit(w.to_string(), "bit"))).info("Lest fra driveren, ikke fra referansetabellen."));
    b.kv(Kv::new("cuda-kjerner", "CUDA-kjerner", ex(st, "cudaCores").as_f64().map(|c| F::num(Some(c), 0))).info("Lest fra driveren."));
    let pcie = pcie_text(st.pcie_width_max, st.pcie_gen_max, smp.pcie_width, smp.pcie_gen);
    let mut tip = vec![];
    if let Some(e) = pcie_explain(st.pcie_gen_max, smp.pcie_gen) {
        tip.push(e);
    }
    if let (Some(a), Some(b2), Some(c), Some(d2)) = (pci["linkSpeed"].as_str(), pci["linkWidth"].as_u64(), pci["linkSpeedMax"].as_str(), pci["linkWidthMax"].as_u64()) {
        tip.push(format!("sysfs: {a} x{b2}, maks {c} x{d2}"));
    }
    b.kv(Kv::new("pcie", "PCIe", pcie).info(tip.join("\n")));
    let (gm, sm) = (ex(st, "pcieGenMaxGpu").as_f64(), ex(st, "pcieGenMaxSys").as_f64());
    b.kv(Kv::new(
        "pcie-maks",
        "PCIe maks",
        match (gm, sm) {
            (Some(a), Some(c)) => Some(format!("Kortet {} · hovedkortet {}", gen_text(Some(a)), gen_text(Some(c)))),
            (Some(a), None) => Some(format!("Kortet {}", gen_text(Some(a)))),
            _ => None,
        },
    ));
    let bar1 = ex(st, "bar1");
    b.kv(Kv::new(
        "bar1",
        "BAR1",
        bar1["totalMiB"].as_f64().map(|t| match bar1["usedMiB"].as_f64() {
            Some(u) => format!("{} · {} i bruk", F::mib(Some(t), None, None), F::mib(Some(u), None, None)),
            None => F::mib(Some(t), None, None),
        }),
    ));
    b.kv(Kv::new("rebar", "Resizable BAR", rebar_text(pci, st.vram_total, bar1["totalMiB"].as_f64())).info("Lest fra sysfs (resource1 og resource1_resize)."));
    let replay = ex(st, "pcieReplay").as_u64();
    let aer = pci["aerCorrectable"].as_u64();
    b.kv(Kv::new(
        "pcie-feil",
        "PCIe-feil",
        match (replay, aer) {
            (None, None) => None,
            _ => Some(
                [replay.map(|r| format!("{r} gjentatte sendinger")), aer.map(|a| format!("{a} korrigerte"))]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
        },
    )
    .info("Telleren for gjentatte sendinger (replay) fra NVML og korrigerte AER-feil fra sysfs, siden oppstart. Null er normalt."));
    out.push(b);

    // ---- Klokker ----
    let mut c = Section::new("klokker", "Klokker");
    c.aside = Some("MHz".into());
    let rows = [("Kjerne", smp.core_clock, st.core_max), ("SM", smp.sm_clock, st.sm_max), ("Minne", smp.mem_clock, st.mem_max), ("Video", smp.video_clock, st.video_max)];
    c.items.push(Item::Table(Table {
        id: "klokker",
        head: vec![String::new(), "Nå".into(), "Maks (driver)".into()],
        end: vec![false, true, true],
        rows: rows.iter().map(|(n, now, max)| vec![n.to_string(), F::num(*now, 0), F::num(*max, 0)]).collect(),
    }));
    let mut note = vec![];
    if na_code(st, "defaultAppClockGfx").is_some() || na_code(st, "appClockGfx").is_some() {
        note.push("Driveren oppgir ikke standardklokker (applications clocks er avviklet).".to_string());
    }
    if let Some(sp) = &spec {
        note.push(format!("Boost etter spesifikasjonen er {} (referanse, ikke målt).", F::mhz(Some(sp.boost_mhz as f64))));
    }
    if let Some(m) = smp.throttle {
        let l = throttle_list(m);
        let what = if l.is_empty() { "ingen".to_string() } else { l.join(", ").to_lowercase() };
        note.push(format!("Begrenser nå: {what}{}.", if smp.throttle_inferred { " (i P8 leses ikke årsaken; kortet er i tomgang)" } else { "" }));
    }
    c.items.push(Item::Note(note.join(" ")));
    out.push(c);

    // ---- P-tilstander ----
    let pst = ex(st, "pstateClocks").as_array();
    if let Some(list) = pst.filter(|l| !l.is_empty()) {
        let mut p = Section::new("pstates", "P-tilstander");
        p.aside = Some("MHz".into());
        let range = |v: &Value| match (v.get(0).and_then(Value::as_f64), v.get(1).and_then(Value::as_f64)) {
            (Some(a), Some(b)) if a == b => F::num(Some(a), 0),
            (Some(a), Some(b)) => format!("{}–{}", F::num(Some(a), 0), F::num(Some(b), 0)),
            _ => F::DASH.into(),
        };
        let now = smp.pstate.clone();
        p.items.push(Item::Table(Table {
            id: "pstates",
            head: vec![String::new(), "Kjerne".into(), "Minne".into()],
            end: vec![false, true, true],
            rows: list
                .iter()
                .map(|r| {
                    let name = r["p"].as_str().unwrap_or("?").to_string();
                    let label = if now.as_deref() == Some(name.as_str()) { format!("{name} · nå") } else { name };
                    vec![label, range(&r["gfx"]), range(&r["mem"])]
                })
                .collect(),
        }));
        let mut n = vec!["Område per P-tilstand fra driveren. P0 er full ytelse, P8 er tomgang.".to_string()];
        if let Some(mc) = ex(st, "memClocks").as_array().filter(|a| !a.is_empty()) {
            n.push(format!("Støttede minneklokker: {} MHz.", mc.iter().filter_map(Value::as_f64).map(|v| F::num(Some(v), 0)).collect::<Vec<_>>().join(", ")));
        }
        p.items.push(Item::Note(n.join(" ")));
        out.push(p);
    }

    // ---- Grenser ----
    let mut l = Section::new("grenser", "Grenser");
    let plimit = smp.power_limit.or_else(|| ex(st, "powerLimit").as_f64());
    l.kv(Kv::new("effektgrense", "Effektgrense nå", plimit.map(|w| F::watt(Some(w), 0))).info("Håndhevet grense fra driveren."));
    l.kv(Kv::new(
        "effektomrade",
        "Standard / område",
        match (st.power_default, st.power_min, st.power_max) {
            (Some(d), Some(a), Some(b)) => Some(format!("{} · {}–{}", F::watt(Some(d), 0), F::num(Some(a), 0), F::watt(Some(b), 0))),
            _ => None,
        },
    ));
    let (amin, amax) = (ex(st, "acousticMin").as_f64(), ex(st, "acousticMax").as_f64());
    l.kv(
        Kv::new(
            "tempmal",
            "Temperaturmål",
            st.temp_target.map(|t| match (amin, amax) {
                (Some(a), Some(b)) => format!("{} · område {}–{}", F::temp(Some(t), 0), F::temp_num(Some(a), 0), F::temp(Some(b), 0)),
                _ => F::temp(Some(t), 0),
            }),
        )
        .info("Akustisk mål: kortet senker klokken for å holde seg her."),
    );
    l.kv(Kv::new("tempmaks", "Maks driftstemperatur", temp_c(st.temp_max)));
    l.kv(Kv::new("nedklokking", "Nedklokking", temp_c(st.temp_slowdown)));
    l.kv(Kv::new("avstenging", "Avstenging", temp_c(st.temp_shutdown)));
    l.kv(
        Kv::new("avstand", "Avstand til nedklokking", st.temp_slowdown.zip(smp.temp).map(|(a, t)| F::temp_delta(a - t, 0)))
            .info("Utregnet: nedklokkingsgrensen minus temperaturen nå. Dette er ikke en direkte måling av T.Limit."));
    let off = |k: &str, div: f64| -> Option<String> {
        let o = ex(st, k);
        let (n, a, b) = (o["now"].as_f64()? / div, o["min"].as_f64()? / div, o["max"].as_f64()? / div);
        Some(format!("{} · område {}…{}", F::with_unit(F::signed(n, 0), "MHz"), F::signed(a, 0), F::with_unit(F::signed(b, 0), "MHz")))
    };
    l.kv(Kv::new("offset-kjerne", "Klokkeoffset, kjerne", off("offsetGfx", 1.0)).info("Lest med NVML. Klokker og effekt er låst; appen endrer ikke offset."));
    l.kv(
        Kv::new("offset-minne", "Klokkeoffset, minne", off("offsetMem", 2.0))
            .info("I effektiv MHz, som Klokker og effekt bruker. NVML oppgir minneoffset i dobbel dataklokke (dobbelt så store tall)."),
    );
    out.push(l);

    // ---- Vifter og kjøling ----
    let mut f = Section::new("vifter", "Vifter og kjøling");
    let fans = ex(st, "fans").as_array().cloned().unwrap_or_default();
    let count = st.fan_count.map(|n| n as usize).or((!smp.fans.is_empty()).then_some(smp.fans.len()));
    let signal = fans.first().and_then(|x| x["signal"].as_i64());
    let target = fans.first().and_then(|x| x["coolerTarget"].as_i64());
    let cooler_tip = match (signal, target) {
        (Some(sg), Some(t)) => {
            let sig = match sg {
                1 => "av/på",
                2 => "variabel fart",
                _ => "ukjent styring",
            };
            let mut parts = vec![];
            for (bit, n) in [(2, "GPU"), (4, "minne"), (8, "strømforsyning")] {
                if t & bit != 0 {
                    parts.push(n);
                }
            }
            Some(format!("Kjølerinfo fra driveren: {sig}, kjøler {}.", if parts.is_empty() { "ukjent".into() } else { parts.join(", ") }))
        }
        _ => None,
    };
    f.kv(
        Kv::new(
            "antall",
            "Vifter",
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
            parts.push("turtall ikke støttet".into());
        }
        if let Some(t) = tgt {
            parts.push(format!("mål {}", F::pct(Some(t), 0)));
        }
        let id: &'static str = ["vifte-1", "vifte-2", "vifte-3", "vifte-4", "vifte-5", "vifte-6", "vifte-7", "vifte-8"][i];
        f.kv(Kv::new(id, &format!("Vifte {}", i + 1), pct.map(|_| parts.join(" · "))));
    }
    let policies: Vec<i64> = fans.iter().filter_map(|x| x["policy"].as_i64()).collect();
    f.kv(
        Kv::new(
            "styring",
            "Styring",
            (!policies.is_empty()).then(|| if policies.iter().all(|p| *p == 0) { "Auto · driveren styrer".into() } else { "Manuell · rapportert av driveren".into() }))
        .info("Lest med NVML (viftepolicy). Viser driverens rapporterte modus; angir ikke hvilket program som satte den."));
    let th = ex(st, "thermal").as_array().cloned().unwrap_or_default();
    f.kv(Kv::new(
        "sensorer",
        "Termiske sensorer",
        (!th.is_empty()).then(|| {
            let first = &th[0];
            let what = if first["controller"].as_i64() == Some(1) { "intern GPU-kontroller".to_string() } else { format!("kontroller {}", first["controller"]) };
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
    let mut n = Section::new("ikke-stottet", "Datatilgjengelighet");
    n.aside = Some("ikke innsamlet eller utilgjengelig".into());
    n.kv(Kv::new("hotspot", "Hotspot-temperatur", None).na("Ikke innsamlet").info("Appen samler ikke inn en egen hotspot-temperatur. De termiske sensorene som driveren rapporterer, vises over; dette avklarer ikke alene om kortet har en hotspot-sensor."));
    let (mna, mwhy) = unsupported(st, "memMax", "Appen har ingen tilgjengelig minnetemperatur fra NVML for denne prøven. Tilgjengelighet avhenger av kortet og driveren.");
    n.kv(Kv { id: "minnetemp", k: "Minnetemperatur".into(), v: None, na: Some(mna.unwrap_or_else(|| "Ikke verifisert".into())), info: Some(mwhy) });
    let (tna, twhy) = unsupported(st, "margin", "Appen har ingen tilgjengelig T.Limit-margin fra NVML for denne prøven. Avstanden til nedklokking under Grenser er regnet ut av appen.");
    n.kv(Kv { id: "tlimit", k: "T.Limit (margin)".into(), v: None, na: Some(tna.unwrap_or_else(|| "Ikke verifisert".into())), info: Some(twhy) });
    n.kv(Kv::new("spenning", "Kjernespenning", None).na("Ikke innsamlet").info("Appen samler ikke inn GPU-kjernespenning. Tilgang for dette kortet og denne driveren er ikke verifisert."));
    n.kv(Kv::new("snitteffekt", "Gjennomsnittseffekt fra driveren", None).na("Ikke innsamlet").info("Appen samler ikke inn driverens eget effektgjennomsnitt. Effekten beregnes fra energitelleren når den finnes, ellers brukes den rapporterte øyeblikksverdien."));
    if na_code(st, "defaultAppClockGfx").is_some() || na_code(st, "customerBoostGfx").is_some() {
        n.kv(Kv::new("appklokker", "Standardklokker (applications clocks)", None).na("Avviklet").info("Avviklet i driveren for dette kortet."));
    }
    if na_code(st, "partNumber").is_some() {
        n.kv(Kv::new("delenummer", "Delenummer (board part number)", None).na("Ikke oppgitt").info("Driveren svarer, men feltet er tomt på dette kortet."));
    }
    // Verdier i prøven som kortet ikke støtter (huskes fra første forsøk).
    for f in &smp.na {
        let (label, why) = match f.as_str() {
            "fanRpm" => ("Viftens turtall", "NVML gir «ikke støttet» for turtallet."),
            "encUtil" => ("Videokoder", "NVML gir «ikke støttet» for koderen."),
            "decUtil" => ("Videodekoder", "NVML gir «ikke støttet» for dekoderen."),
            "throttle" => ("Klokkebegrensninger", "NVML gir «ikke støttet» for årsakene."),
            "power" => ("Effekttrekk", "Verken energitelleren eller effekten kan leses."),
            "energy" => ("Energiteller", "Effekten leses som øyeblikksverdi i stedet."),
            "temp" => ("Temperatur", "NVML gir «ikke støttet» for temperaturen."),
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
        n.kv(Kv::new(id, label, None).na("Ikke støttet").info(why));
    }
    out.push(n);

    // ---- Prosesser ----
    let procs = ex(st, "processes");
    let mut p = Section::new("prosesser", "Prosesser som bruker skjermkortet");
    let list = procs["list"].as_array().cloned().unwrap_or_default();
    if procs.is_object() {
        p.items.push(Item::Table(Table {
            id: "prosesser",
            head: vec!["Prosess".into(), "PID".into(), "Type".into(), "Videominne".into()],
            end: vec![false, true, false, true],
            rows: list
                .iter()
                .map(|r| vec![r["name"].as_str().unwrap_or("–").to_string(), r["pid"].to_string(), r["kind"].as_str().unwrap_or("").to_string(), F::mib(r["vramMiB"].as_f64(), None, None)])
                .collect(),
        }));
        let when = procs["t"].as_i64().map(|t| F::time(F::local(t, g.utc_offset_s), true)).unwrap_or_else(|| "–".into());
        let live = procs["live"] == true;
        p.items.push(Item::Note(match (list.is_empty(), live) {
            (true, false) => format!("Ingen prosesser brukte skjermkortet kl. {when}."),
            (true, true) => format!("Ingen prosesser bruker skjermkortet (lest kl. {when}). Lista oppdateres hvert 5. s mens siden vises."),
            (false, false) => format!("Øyeblikksbilde fra kl. {when}, da tjenesten leste de faste dataene. Videominne per prosess fra driveren."),
            (false, true) => format!("Lest kl. {when}. Lista oppdateres hvert 5. s mens siden vises. Videominne per prosess fra driveren."),
        }));
    } else {
        p.items.push(Item::Note("Lista leses sammen med de faste dataene.".into()));
    }
    out.push(p);

    // ---- Spesifikasjon ----
    let mut r = Section::new("spesifikasjon", "Spesifikasjon");
    r.aside = Some("Referanse, ikke målt".into());
    match &spec {
        None => r.items.push(Item::Note(format!("Ingen oppføring for PCI-ID {} i den innebygde tabellen.", dev_hex.unwrap_or_else(|| "ukjent".into())))),
        Some(sp) => {
            let u = |v: String, unit: &str| Some(F::with_unit(v, unit));
            let dv = |x: crate::specs::Derived| Kv::new("avledet", "", u(F::num(Some(x.value), if x.value < 100.0 { 1 } else { 0 }), x.unit)).info(x.formula);
            r.items.push(Item::Sub("Brikke".into()));
            r.kv(Kv::new("modell", "Modell", Some(sp.name.into())));
            r.kv(Kv::new("brikke", "Brikke", Some(format!("{} · {}", sp.chip, sp.variant))));
            r.kv(Kv::new("arkitektur", "Arkitektur", Some(sp.architecture.into())));
            r.kv(Kv::new("prosess", "Prosess", Some(sp.process.into())));
            r.kv(Kv::new("storrelse", "Brikkestørrelse", u(F::num(Some(sp.die_mm2 as f64), 0), "mm²")));
            r.kv(Kv::new("transistorer", "Transistorer", Some(format!("{} mrd.", F::num(Some(sp.transistors_bn), 1)))));
            r.kv(Kv::new("cuda", "CUDA-kjerner", Some(F::num(Some(sp.cuda as f64), 0))));
            r.kv(Kv::new("sm", "SM-er", Some(F::num(Some(sp.sm as f64), 0))));
            r.kv(Kv::new("tensor", "Tensor / RT", Some(format!("{} / {}", sp.tensor, sp.rt))));
            r.kv(Kv::new("tmu", "TMU / ROP", Some(format!("{} / {}", sp.tmu, sp.rop))));
            r.kv(Kv::new("l2", "L2-cache", u(F::num(Some(sp.l2_kb as f64 / 1024.0), 0), "MiB")));
            r.kv(Kv::new("grunnklokke", "Grunnklokke", Some(F::mhz(Some(sp.base_mhz as f64)))));
            r.kv(Kv::new("boost", "Boostklokke", Some(F::mhz(Some(sp.boost_mhz as f64)))));
            r.items.push(Item::Sub("Minne".into()));
            r.kv(Kv::new("type", "Type", Some(sp.mem_type.into())));
            r.kv(Kv::new("buss", "Buss", u(sp.bus_width.to_string(), "bit")));
            r.kv(Kv::new("mengde", "Mengde", u(sp.mem_gb.to_string(), "GB")));
            r.kv(Kv::new("hastighet", "Hastighet", Some(format!("{} effektivt", F::with_unit(F::num(Some(sp.mem_gbps), if sp.mem_gbps.fract() == 0.0 { 0 } else { 1 }), "Gbps")))));
            r.kv(Kv { k: "Båndbredde".into(), ..dv(sp.bandwidth()) });
            r.kv(Kv::new("leverandor", "Leverandør", None).na("Ukjent på Linux").info(sp.mem_vendor_note));
            r.items.push(Item::Sub("Ytelse og kort".into()));
            r.kv(Kv { k: "Pikselfyllrate".into(), ..dv(sp.pixel_fill()) });
            r.kv(Kv { k: "Teksturfyllrate".into(), ..dv(sp.texture_fill()) });
            r.kv(Kv { k: "FP32".into(), ..dv(sp.fp32()) });
            r.kv(Kv::new("tdp", "TDP (referansekort)", Some(F::watt(Some(sp.tdp_w as f64), 0))).info("Kortprodusenten kan sette en annen grense. Driveren oppgir standardgrensen under Grenser."));
            r.kv(Kv::new("pcie", "PCIe", pcie_spec(sp.bus)).info(sp.bus));
            r.kv(Kv::new("api", "DirectX / Vulkan / OpenGL", Some(format!("{} / {} / {}", sp.directx, sp.vulkan, sp.opengl))));
            let launched = match &g.egg {
                Some((t, _)) => format!("{} · {t}", sp.launched),
                None => sp.launched.to_string(),
            };
            r.kv(Kv::new("lansert", "Lansert", Some(launched)).info_opt(g.egg.as_ref().map(|e| e.1.clone())));
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
        return Some("På · BAR1 dekker hele videominnet".into());
    }
    if let Some(sizes) = pci["bar1Sizes"].as_array().filter(|a| !a.is_empty()) {
        let max = sizes.iter().filter_map(Value::as_f64).fold(0.0, f64::max);
        if vram.is_some_and(|v| max < v) {
            return Some(format!("Ikke støttet · største BAR1 er {}", F::mib(Some(max), None, None)));
        }
        return Some(format!("Av · kortet støtter opptil {}", F::mib(Some(max), None, None)));
    }
    bar1.map(|b| format!("Av · BAR1 er {}", F::mib(Some(b), None, None)))
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
