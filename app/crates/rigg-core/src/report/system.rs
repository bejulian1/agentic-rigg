//! Maskinen som seksjoner: prosessor, cache, instruksjonssett og sårbarheter
//! (Prosessor-siden, [`PROSESSOR`]), hovedkort og BIOS, minne, operativsystem
//! og kontrollere (System-siden, [`SYSTEM`]), og diskene (rapporten). Rapporten
//! tar med alt. Port av
//! `web/js/pages/system.js`, med rettingene fra maskinvare-revisjon kap. 7 og 11
//! (CPU-sensorens temperatur, `Shmem` skilt ut, SPD som et valg som krever
//! godkjenning, disker).
//!
//! Dataene er `get_system` (fra `rigg-hw/src/sys/{cpuinfo,dmi,os,pci}.rs`) og
//! siste `SysSample`.

use super::{Item, Kv, Section, Table};
use crate::{tr, trf};
use crate::fmt as F;
use crate::model::SysSample;
use serde_json::Value;

pub struct SysInput<'a> {
    pub system: &'a Value,
    pub sys: Option<&'a SysSample>,
    /// Vis vertsnavnet (siden viser det; rapporten bare når serienumre tas med).
    pub show_hostname: bool,
    /// Skrivebord og økttype fra vinduets miljø (tjenesten kan mangle dem).
    pub desktop: Option<String>,
    pub session: Option<String>,
    /// Rapporten: ta med klokke og last per tråd som tabell.
    pub report: bool,
    /// Høyeste målte trådklokke i MHz (min og maks fra tjenesten), så
    /// klokkeområdet kan si hvor høyt boost faktisk går.
    pub max_thread_mhz: Option<f64>,
}

/// Seksjonene på Prosessor-siden (1.1.0).
pub const PROSESSOR: &[&str] = &["prosessor", "cache", "flagg", "sarbarheter"];
/// Seksjonene på System-siden (1.1.0).
pub const SYSTEM: &[&str] = &["hovedkort", "minne", "os", "kontrollere"];

/// En sårbarhet fra sysfs på norsk: «Not affected» → «Ikke berørt»,
/// «Mitigation: Retpolines; …» → «Tiltak: Retpolines; …».
pub fn vuln_text(s: &str) -> String {
    let s = s.trim();
    if s == "Not affected" {
        return "Ikke berørt".into();
    }
    if let Some(r) = s.strip_prefix("Mitigation: ") {
        return format!("Tiltak: {r}");
    }
    if let Some(r) = s.strip_prefix("Vulnerable") {
        return format!("Sårbar{r}");
    }
    s.to_string()
}

/// Navnet på en sårbarhet: de kjente med sine egne navn og forkortelser
/// («l1tf» → «L1TF», «spec_store_bypass» → «Speculative Store Bypass»),
/// ellers «tsx_new_thing» → «Tsx new thing».
fn vuln_name(n: &str) -> String {
    let known = match n {
        "gather_data_sampling" => "Gather Data Sampling",
        "ghostwrite" => "GhostWrite",
        "indirect_target_selection" => "Indirect Target Selection",
        "itlb_multihit" => "iTLB Multihit",
        "l1tf" => "L1TF",
        "mds" => "MDS",
        "meltdown" => "Meltdown",
        "mmio_stale_data" => "MMIO Stale Data",
        "old_microcode" => "Gammel mikrokode",
        "reg_file_data_sampling" => "Register File Data Sampling",
        "retbleed" => "Retbleed",
        "spec_rstack_overflow" => "Return Stack Overflow (SRSO)",
        "spec_store_bypass" => "Speculative Store Bypass",
        "spectre_v1" => "Spectre v1",
        "spectre_v2" => "Spectre v2",
        "srbds" => "SRBDS",
        "tsa" => "TSA",
        "tsx_async_abort" => "TSX Async Abort",
        "vmscape" => "VMScape",
        _ => "",
    };
    if !known.is_empty() {
        return known.into();
    }
    let mut t = n.replace('_', " ");
    if let Some(c) = t.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    t
}

const CACHE_TYPE: [(&str, &str); 3] = [("Data", "Data"), ("Instruction", "Instruksjon"), ("Unified", "Felles")];
const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn st(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

/// DMI-dato «03/06/2019» (MM/DD/ÅÅÅÅ) → «6. mars 2019».
pub fn dmi_date(s: &str) -> Option<String> {
    let p: Vec<&str> = s.trim().split('/').collect();
    if p.len() != 3 {
        return None;
    }
    let (m, d, y): (usize, u32, i32) = (p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?);
    (1..=12).contains(&m).then(|| F::date(y, m as u32, d, false))
}

/// «#31-Ubuntu SMP PREEMPT_DYNAMIC Sat Aug  1 04:26:38 UTC 2026» → «Bygg #31 · 1. aug. 2026 · PREEMPT_DYNAMIC».
pub fn kernel_build(s: &str) -> String {
    let mut parts = Vec::new();
    if let Some(n) = s.strip_prefix('#').and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).filter(|n| !n.is_empty()) {
        parts.push(trf!("Bygg #{nummer}", nummer = n));
    }
    let w: Vec<&str> = s.split_whitespace().collect();
    for i in 0..w.len() {
        if let Some(m) = MON.iter().position(|x| *x == w[i])
            && let (Some(d), Some(y)) = (w.get(i + 1).and_then(|x| x.parse::<u32>().ok()), w.last().and_then(|x| x.parse::<i32>().ok()))
        {
            parts.push(F::date(y, m as u32 + 1, d, true));
            break;
        }
    }
    if s.contains("PREEMPT_DYNAMIC") {
        parts.push("PREEMPT_DYNAMIC".into());
    }
    if parts.is_empty() { s.to_string() } else { parts.join(" · ") }
}

/// XDG_CURRENT_DESKTOP «ubuntu:GNOME» → «GNOME (Ubuntu-økt)».
pub fn desktop_text(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split(':').filter(|p| !p.is_empty()).collect();
    let main = *parts.last()?;
    if parts.len() > 1 {
        let v = parts[0].to_lowercase();
        let nice = match v.as_str() {
            "ubuntu" => tr!("Ubuntu-økt").to_string(),
            "pop" => tr!("Pop-økt").to_string(),
            "zorin" => tr!("Zorin-økt").to_string(),
            _ => trf!("{navn}-økt", navn = parts[0]),
        };
        return Some(format!("{main} ({nice})"));
    }
    Some(main.to_string())
}

/// «0-3,8-11» → «0–3, 8–11».
fn cpu_list(s: &str) -> String {
    s.split(',').map(|p| p.replace('-', "–")).collect::<Vec<_>>().join(", ")
}

/// Linux-CPU-ID-ene som er online. En eksplisitt tom liste betyr ingen tråder;
/// bare eldre tjenestesvar uten feltet bruker det sammenhengende trådtallet.
pub fn cpu_thread_ids(cpu: &Value) -> Vec<usize> {
    match cpu.get("threadIds") {
        Some(ids) => ids.as_array().map(|ids| ids.iter().filter_map(|v| v.as_u64().and_then(|v| usize::try_from(v).ok())).collect()).unwrap_or_default(),
        None => (0..cpu["threads"].as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(0)).collect(),
    }
}

pub fn sections(i: &SysInput) -> Vec<Section> {
    let cpu = &i.system["cpu"];
    let board = &i.system["board"];
    let os = &i.system["os"];
    let pci = &i.system["pci"];
    let sys = i.sys.cloned().unwrap_or_default();
    let mut out = Vec::new();

    // ---- Prosessor ----
    let mut c = Section::new("prosessor", "Prosessor");
    c.kv(Kv::new("navn", "Navn", st(&cpu["name"])));
    let spec = cpu["vendor"].as_str().zip(cpu["name"].as_str()).and_then(|(vendor, name)| crate::cpu_specs::lookup(vendor, name));
    let launch_source = spec.map(|s| format!("Modelloppslag, ikke målt. Kilde: AMD, {}: {}", s.name, s.source_url));
    c.kv(
        Kv::new("lansert", "Lansert", spec.map(|s| s.launched.to_string()))
            .na("Ikke i modelloppslaget")
            .info(launch_source.clone().unwrap_or_else(|| "Lanseringsdato krever et kjent modellnavn og riktig leverandør. CPUID-familie og kodenavn brukes ikke som erstatning.".into())),
    );
    let cn = &cpu["codename"];
    c.kv(
        Kv::new(
            "kodenavn",
            "Kodenavn",
            st(&cn["name"]).map(|n| [Some(n), st(&cn["process"]), st(&cn["socket"]), st(&cn["revision"])].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
        )
        .na("Ikke i oppslagstabellen")
        .info("Slått opp fra produsent, familie og modell. Kjernen oppgir ikke kodenavn, prosess og sokkel."),
    );
    c.kv(Kv::new("produsent", "Produsent", st(&cpu["vendorName"])).info_opt(st(&cpu["vendor"])));
    let (fa, mo, se) = (cpu["family"].as_u64(), cpu["model"].as_u64(), cpu["stepping"].as_u64());
    c.kv(
        Kv::new("fms", "Familie · modell · rev.", match (fa, mo, se) {
            // Som CPU-Z: «17h (23) · 8 · 2».
            (Some(a), Some(b), Some(d)) => Some(format!("{a:X}h ({a}) · {b} · {d}")),
            _ => None,
        })
        .info_opt(match (fa, mo, se) {
            (Some(_), Some(b), Some(d)) => Some(format!("Familie, modell og stepping (revisjon). Familien står heksadesimalt og desimalt; modell {b:X}h, stepping {d:X}h.")),
            _ => None,
        }),
    );
    c.kv(Kv::new("mikrokode", "Mikrokode-revisjon", st(&cpu["microcode"])));
    let (cores, threads) = (cpu["cores"].as_i64(), cpu["threads"].as_i64());
    c.kv(Kv::new(
        "kjerner",
        "Kjerner og tråder",
        match (cores, threads) {
            (Some(k), Some(t)) => Some(format!("{} · {}", F::plural(k, "kjerne", "kjerner"), F::plural(t, "tråd", "tråder"))),
            (None, Some(t)) => Some(F::plural(t, "tråd", "tråder")),
            _ => None,
        },
    ));
    if let Some(g) = cpu["l3Groups"].as_array().filter(|g| g.len() > 1) {
        let list: Vec<String> = g.iter().filter_map(Value::as_str).map(cpu_list).collect();
        c.kv(Kv::new("ccx", "L3-grupper", Some(format!("{} · tråd {}", g.len(), list.join(" og ")))).info("Grupper av tråder som deler L3-cache, slik kjernen rapporterer dem. Gruppene er ikke nødvendigvis CCX-er."));
    }
    let fq = &cpu["freq"];
    c.kv(
        Kv::new(
            "klokke",
            "Klokke (cpufreq)",
            match (fq["minKHz"].as_f64(), fq["maxKHz"].as_f64()) {
                // «2 200–3 700 MHz · boost til 4,35 GHz (målt)», når boost er målt over maks.
                (Some(a), Some(b)) => Some(match i.max_thread_mhz.filter(|m| *m > b / 1000.0 + 25.0) {
                    Some(m) => format!("{}–{} · boost til {} (målt)", F::num(Some(a / 1000.0), 0), F::mhz(Some(b / 1000.0)), F::with_unit(F::num(Some(m / 1000.0), 2), "GHz")),
                    None => format!("{}–{}", F::num(Some(a / 1000.0), 0), F::mhz(Some(b / 1000.0))),
                }),
                _ => None,
            },
        )
        .na("cpufreq er ikke tilgjengelig")
        .info("Nominelt område fra cpufreq. Med boost på går kjernene over maks; «målt» er den høyeste trådklokken tjenesten har sett."),
    );
    if let Some(p) = fq["pstatesKHz"].as_array().filter(|a| a.len() > 1) {
        c.kv(Kv::new("pstates", "P-tilstander", Some(F::with_unit(p.iter().filter_map(Value::as_f64).map(|k| F::num(Some(k / 1000.0), 0)).collect::<Vec<_>>().join(", "), "MHz"))));
    }
    c.kv(Kv::new(
        "governor",
        "Governor og driver",
        Some([sys.cpu.governor.clone(), st(&fq["driver"])].into_iter().flatten().collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty()),
    ));
    c.kv(Kv::new("boost", "Boost", sys.cpu.boost.map(|b| if b { "På".into() } else { "Av".into() })).na("Ikke oppgitt"));
    let temp_source = crate::sensormeta::cpu_temp_sensor(&sys).filter(|s| s.value.is_some() && !s.label.is_empty() && s.label != s.key).map(|s| s.label.as_str());
    c.kv(
        Kv::new("temp", "CPU-temperatur", crate::sensormeta::cpu_temp(&sys).map(|t| match temp_source {
            Some(source) => format!("{} ({source})", F::temp(Some(t), 1)),
            None => F::temp(Some(t), 1),
        }))
        .info_opt(sys.cpu.tctl.map(|t| format!("Tctl rapporteres separat: {}. Sensorbetydningen avhenger av CPU-modellen og driveren.", F::temp(Some(t), 1)))),
    );
    c.kv(Kv::new(
        "adresser",
        "Adresser",
        st(&cpu["addressSizes"]).map(|a| a.replace(" bits physical", &format!("{}bit fysisk", F::NNBSP)).replace(" bits virtual", &format!("{}bit virtuelt", F::NNBSP)))));
    c.kv(Kv::new("spenning", "Kjernespenning", None).na("Ikke innsamlet").info("Appen samler ikke inn CPU-kjernespenning. Tilgang og målekilde avhenger av CPU, hovedkort og driver; SVI2-støtte er ikke verifisert her."));
    c.kv(Kv::new("effekt", "Pakkeeffekt", None).na("Ikke innsamlet").info("Appen samler ikke inn CPU-pakkeeffekt. Tilgjengelige energitellere, eventuell RAPL-støtte og lesetilgang avhenger av maskinen og kjernen."));
    c.kv(Kv::new("multiplikator", "Multiplikator og bussklokke", None).na("Ikke innsamlet").info("Appen samler ikke inn multiplikator og bussklokke. Trådklokken leses separat fra cpufreq; målekilden avhenger av driveren."));
    if i.report
        && let Some(source) = launch_source
    {
        c.items.push(Item::Note(format!("Lanseringsdato: {source}")));
    }
    out.push(c);

    // ---- Cache ----
    let mut k = Section::new("cache", "Cache");
    let caches = cpu["caches"].as_array().cloned().unwrap_or_default();
    k.items.push(Item::Table(Table {
        id: "cache",
        head: ["Nivå", "Type", "Per instans", "Antall", "Totalt", "Assosiativitet"].map(String::from).to_vec(),
        end: vec![false, false, true, true, true, true],
        rows: caches
            .iter()
            .map(|x| {
                let ty = x["type"].as_str().unwrap_or("");
                vec![
                    format!("L{}", x["level"].as_u64().unwrap_or(0)),
                    CACHE_TYPE.iter().find(|(a, _)| *a == ty).map(|(_, b)| b.to_string()).unwrap_or_else(|| ty.to_string()),
                    F::bytes(x["size"].as_f64(), 0),
                    x["instances"].as_u64().map(|n| format!("×{}{n}", F::NNBSP)).unwrap_or_else(|| F::DASH.into()),
                    F::bytes(x["total"].as_f64(), 0),
                    x["ways"].as_u64().map(|w| format!("{w}-veis")).unwrap_or_else(|| F::DASH.into()),
                ]
            })
            .collect(),
    }));
    out.push(k);

    // ---- Instruksjonssett ----
    let mut fl = Section::new("flagg", "Instruksjonssett");
    let imp: Vec<String> = cpu["important"].as_array().map(|a| a.iter().filter_map(|x| x["label"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    let all: Vec<String> = cpu["flags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    fl.kv(Kv::new("utvalgte", "Utvalgte", (!imp.is_empty()).then(|| imp.join(", "))));
    fl.kv(Kv::new("antall", "Alle flagg", (!all.is_empty()).then(|| format!("{} i alt", all.len()))));
    if !all.is_empty() {
        fl.items.push(Item::Text(all.join(" ")));
    }
    out.push(fl);

    // ---- Sårbarheter (sysfs, lesbart for alle) ----
    if let Some(v) = cpu["vulnerabilities"].as_object().filter(|v| !v.is_empty()) {
        let mut sv = Section::new("sarbarheter", "Sårbarheter");
        let affected = v.values().filter_map(Value::as_str).filter(|t| t.starts_with("Vulnerable")).count();
        sv.aside = Some(if affected == 0 { "ingen åpne".to_string() } else { F::plural(affected as i64, "åpen", "åpne") });
        for (k, t) in v {
            let text = t.as_str().unwrap_or("");
            sv.kv(Kv::new("vuln", &vuln_name(k), Some(vuln_text(text))));
        }
        out.push(sv);
    }

    // ---- Hovedkort og BIOS ----
    let mut b = Section::new("hovedkort", tr!("Hovedkort og BIOS"));
    let dm = |k: &str| st(&board[k]);
    b.kv(Kv::new("produsent", tr!("Produsent"), dm("board_vendor")).na(tr!("Ikke oppgitt")));
    b.kv(Kv::new("modell", tr!("Modell"), dm("board_name")).na(tr!("Ikke oppgitt")));
    b.kv(Kv::new("versjon", tr!("Versjon"), dm("board_version")).na(tr!("Ikke oppgitt")));
    b.kv(
        Kv::new("brikkesett", tr!("Brikkesett"), st(&pci["chipset"]))
            .na(tr!("Ikke oppgitt"))
            .info_opt(pci["chipsetIds"].as_array().filter(|a| !a.is_empty()).map(|a| trf!("Slått opp i pci.ids fra {id}.", id = a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")))),
    );
    b.kv(Kv::new("system", tr!("System"), Some([dm("sys_vendor"), dm("product_name")].into_iter().flatten().collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty())).na(tr!("Ikke oppgitt")));
    b.kv(Kv::new("bios", "BIOS", Some([dm("bios_vendor"), dm("bios_version")].into_iter().flatten().collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty())).na(tr!("Ikke oppgitt")).info_opt(dm("bios_release").map(|r| trf!("Utgave {versjon}", versjon = r))));
    b.kv(Kv::new("bios-dato", tr!("BIOS-dato"), dm("bios_date").map(|d| dmi_date(&d).unwrap_or(d))).na(tr!("Ikke oppgitt")).info_opt(dm("bios_date").map(|d| trf!("Rå verdi: {dato}", dato = d))));
    b.kv(Kv::new("kabinett", tr!("Kabinett"), st(&board["chassis"])).na(tr!("Ikke oppgitt")));
    b.kv(Kv::new("serienumre", tr!("Serienumre"), None).na(tr!("Leses ikke (krever root)")).info(tr!("Serienummerfilene i /sys/class/dmi/id er bare lesbare for root. Appen åpner dem ikke.")));
    out.push(b);

    // ---- Minne ----
    let mut m = Section::new("minne", tr!("Minne"));
    let mem = &sys.mem;
    let total = mem.total_kb.or_else(|| i.system["memTotalKB"].as_f64());
    m.kv(Kv::new("totalt", tr!("Totalt"), total.map(|t| F::kb(Some(t), 1))).info(tr!("Det kjernen kan bruke. Resten av modulene er reservert av fastvaren og kjernen.")));
    m.kv(Kv::new(
        "brukt",
        tr!("Brukt"),
        mem.used_kb.zip(total).map(|(u, t)| format!("{} · {}", F::kb(Some(u), 1), F::pct(Some(u / t * 100.0), 0))),
    ));
    m.kv(Kv::new("tilgjengelig", tr!("Tilgjengelig"), mem.available_kb.map(|a| F::kb(Some(a), 1))));
    // Delt minne (Shmem) står i Cached, men regnes også i «brukt» (totalt
    // minus tilgjengelig). Trekkes det fra her, går tallene opp i totalen.
    let cache = match (mem.buffers_kb, mem.cached_kb) {
        (Some(b), Some(c)) => Some((b + c + mem.sreclaimable_kb.unwrap_or(0.0) - mem.shmem_kb.unwrap_or(0.0)).max(0.0)),
        _ => None,
    };
    m.kv(
        Kv::new("cache", tr!("Buffer og cache"), cache.map(|c| F::kb(Some(c), 1))).info_opt(mem.shmem_kb.map(|s| trf!("Buffer, cache og frigjørbar slab, uten delt minne (Shmem, tmpfs: {minne}). Delt minne kan ikke frigjøres og er regnet med i Brukt.", minne = F::kb(Some(s), 1)))));
    m.kv(Kv::new(
        "swap",
        tr!("Swap"),
        match (mem.swap_used_kb, mem.swap_total_kb) {
            (_, Some(t)) if t <= 0.0 => Some(tr!("Ingen swap").into()),
            (Some(u), Some(t)) => Some(trf!("{brukt} av {totalt}", brukt = F::kb(Some(u), 1), totalt = F::kb(Some(t), 1))),
            _ => None,
        },
    ));
    let spd = &board["spd"];
    let chips = spd["chips"].as_u64().unwrap_or(0);
    let spd_tip = if chips > 0 {
        trf!(
            "{antall} SPD-brikker er funnet ({driver}). Produsent, delenummer og timingene modulene støtter, kan leses derfra uten root, men lesingen er I2C-trafikk på hovedkortets SMBus, og den er ikke slått på. Hastigheten minnet faktisk går på, og timingene BIOS har satt, krever root.",
            antall = chips,
            driver = spd["driver"].as_str().unwrap_or("ee1004")
        )
    } else {
        tr!("Moduler, hastighet og timinger ligger i SMBIOS-tabellen og i registrene i minnekontrolleren, som bare root kan lese.").to_string()
    };
    m.kv(Kv::new("moduler", tr!("Moduler og timinger"), None).na(if chips > 0 { tr!("Krever godkjenning") } else { tr!("Krever root") }).info(spd_tip));
    out.push(m);

    // ---- Operativsystem ----
    let mut o = Section::new("os", tr!("Operativsystem"));
    o.kv(Kv::new("distro", tr!("Distribusjon"), st(&os["distro"])));
    o.kv(Kv::new("kjerne", tr!("Kjerne"), st(&os["kernel"])));
    o.kv(Kv::new("kjernebygg", tr!("Kjernebygg"), st(&os["kernelBuild"]).map(|k| kernel_build(&k))).info_opt(st(&os["kernelBuild"]).map(|k| if k.contains("PREEMPT_DYNAMIC") { trf!("{bygg}\nPREEMPT_DYNAMIC: hvor ofte kjernen kan avbryte seg selv, velges ved oppstart.", bygg = k) } else { k })));
    o.kv(Kv::new("arkitektur", tr!("Arkitektur"), st(&os["arch"])));
    let desk = i.desktop.clone().or_else(|| st(&os["desktop"]));
    let sess = i.session.clone().or_else(|| st(&os["session"]));
    o.kv(Kv::new(
        "skrivebord",
        tr!("Skrivebord"),
        Some([desk.as_deref().and_then(desktop_text), sess].into_iter().flatten().collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty()),
    ).info_opt(desk.map(|d| format!("XDG_CURRENT_DESKTOP={d}"))));
    if i.show_hostname {
        o.kv(Kv::new("vert", tr!("Vertsnavn"), st(&os["hostname"])));
    } else if i.report {
        o.kv(Kv::new("vert", tr!("Vertsnavn"), None).na(tr!("Ikke tatt med")));
    }
    o.kv(Kv::new("oppetid", tr!("Oppetid"), sys.uptime.map(|u| F::uptime(Some(u)))));
    o.kv(
        Kv::new("last", tr!("Last (1 / 5 / 15 min)"), (!sys.loadavg.is_empty()).then(|| sys.loadavg.iter().map(|v| F::num(Some(*v), 2)).collect::<Vec<_>>().join(" / ")))
            .info(tr!("Gjennomsnittlig antall prosesser som kjører eller venter. Over antall tråder betyr kø.")),
    );
    out.push(o);

    // ---- Kontrollere (PCI, fra pci.ids) ----
    if let Some(list) = pci["controllers"].as_array().filter(|l| !l.is_empty()) {
        let mut k = Section::new("kontrollere", tr!("Kontrollere"));
        k.items.push(Item::Table(Table {
            id: "kontrollere",
            head: [tr!("Type"), tr!("Enhet"), tr!("Driver")].map(String::from).to_vec(),
            end: vec![false, false, false],
            rows: list
                .iter()
                .map(|x| {
                    let name = [st(&x["vendor"]), st(&x["name"])].into_iter().flatten().collect::<Vec<_>>().join(" ");
                    vec![
                        match x["kind"].as_str().unwrap_or_default() {
                            "Trådløst nett" => tr!("Trådløst nett"),
                            "Lyd" => tr!("Lyd"),
                            kind => kind,
                        }.into(),
                        if name.is_empty() { st(&x["ids"]).unwrap_or_else(|| F::DASH.into()) } else { name },
                        st(&x["driver"]).unwrap_or_else(|| tr!("ingen").into()),
                    ]
                })
                .collect(),
        }));
        out.push(k);
    }

    // ---- Disker (bare i rapporten; siden heter Disker) ----
    if i.report {
        out.push(disks_section(i.system));
    }

    // ---- Klokke og last per tråd (bare i rapporten; siden har rutenettet) ----
    let thread_ids = cpu_thread_ids(cpu);
    if i.report && !thread_ids.is_empty() {
        let mut t = Section::new("trader", "Klokke og last per tråd");
        t.items.push(Item::Table(Table {
            id: "trader",
            head: ["Tråd", "Klokke", "Last"].map(String::from).to_vec(),
            end: vec![false, true, true],
            rows: thread_ids.iter()
                .map(|&n| vec![format!("T{n}"), F::mhz(sys.cpu.core_mhz.get(n).copied().flatten()), F::pct(sys.cpu.core_load.get(n).copied().flatten(), 0)])
                .collect(),
        }));
        out.push(t);
    }
    out
}

/// Diskene med helse og temperatur fra `get_system` › `disks` (tjenestens
/// disklista, runde 6A), ellers den enkle lista fra sysfs.
fn disks_section(system: &Value) -> Section {
    let mut d = Section::new("disker", "Disker");
    let list: Option<crate::disk::DiskList> = serde_json::from_value(system["disks"].clone()).ok().filter(|l: &crate::disk::DiskList| !l.disks.is_empty());
    match list {
        Some(l) => {
            d.items.push(Item::Table(Table {
                id: "disker",
                head: ["Disk", "Modell", "Type", "Størrelse", "Helse", "Temperatur"].map(String::from).to_vec(),
                end: vec![false, false, false, true, false, true],
                rows: l
                    .disks
                    .iter()
                    .map(|x| {
                        vec![
                            format!("{}{}", x.name, if x.system { " · system" } else { "" }),
                            [x.vendor.clone(), Some(x.model.clone())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "),
                            [x.kind.clone(), x.bus.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                            x.size.map(crate::disk::disk_size_label).unwrap_or_else(|| F::DASH.into()),
                            x.health.level.text().to_string(),
                            F::temp(x.smart.temp, 0),
                        ]
                    })
                    .collect(),
            }));
            let reasons: Vec<String> = l.disks.iter().filter(|x| !x.health.reasons.is_empty()).map(|x| format!("{}: {}", x.name, x.health.reasons.join(", "))).collect();
            if !reasons.is_empty() {
                d.items.push(Item::Note(reasons.join(". ")));
            }
            if l.source == "sysfs" {
                d.items.push(Item::Note("SMART mangler: UDisks2 svarte ikke.".into()));
            }
        }
        None => {
            let disks = system["os"]["disks"].as_array().cloned().unwrap_or_default();
            d.items.push(Item::Table(Table {
                id: "disker",
                head: ["Disk", "Modell", "Type", "Størrelse"].map(String::from).to_vec(),
                end: vec![false, false, false, true],
                rows: disks
                    .iter()
                    .map(|x| {
                        let name = x["name"].as_str().unwrap_or("?");
                        let sysd = if x["system"].as_bool() == Some(true) { " · system" } else { "" };
                        vec![
                            format!("{name}{sysd}"),
                            [st(&x["vendor"]), st(&x["model"])].into_iter().flatten().collect::<Vec<_>>().join(" "),
                            [st(&x["kind"]), st(&x["bus"])].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                            x["bytes"].as_f64().map(crate::disk::disk_size_label).unwrap_or_else(|| F::DASH.into()),
                        ]
                    })
                    .collect(),
            }));
            d.items.push(Item::Note("Helse og temperatur står på Disker-siden når tjenesten har lest diskene.".into()));
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_metadata_beholder_sensor_og_faktiske_trad_ider() {
        let mut system = serde_json::json!({"cpu": {"threads": 4, "threadIds": [2, 3, 8, 11], "l3Groups": ["2-3", "8,11"]}});
        let mut sys: SysSample = serde_json::from_value(serde_json::json!({
            "cpu": {"temp": 42.0, "coreLoad": [null, null, 10.0, 20.0, null, null, null, null, 30.0, null, null, 40.0]},
            "hwmon": [{"name": "coretemp", "sensors": [{"kind": "temp", "key": "temp1", "label": "Package id 0", "value": 42.0}]}]
        }))
        .unwrap();
        let render = |system: &Value, sys: &SysSample| sections(&SysInput { system, sys: Some(sys), show_hostname: false, desktop: None, session: None, report: true, max_thread_mhz: None });
        let sections = render(&system, &sys);
        let text = crate::report::render_sections(&sections, crate::report::ReportFormat::Text, 2);
        assert!(text.contains("(Package id 0)") && text.contains("L3-grupper"), "{text}");
        assert!(!text.contains("Tdie") && !text.contains("CCX-er:"), "{text}");
        let rows = sections
            .iter()
            .find(|s| s.id == "trader")
            .unwrap()
            .items
            .iter()
            .find_map(|item| match item {
                Item::Table(t) => Some(&t.rows),
                _ => None,
            })
            .unwrap();
        assert_eq!(rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(), vec!["T2", "T3", "T8", "T11"]);
        system["cpu"]["threadIds"] = serde_json::json!([]);
        assert!(cpu_thread_ids(&system["cpu"]).is_empty());
        assert!(!render(&system, &sys).iter().any(|s| s.id == "trader"));
        system["cpu"].as_object_mut().unwrap().remove("threadIds");
        assert_eq!(cpu_thread_ids(&system["cpu"]), vec![0, 1, 2, 3]);
        sys.hwmon.clear();
        let text = crate::report::render_sections(&render(&system, &sys), crate::report::ReportFormat::Text, 2);
        assert!(text.contains("CPU-temperatur:") && !text.contains("Tdie"), "{text}");
    }

    #[test]
    fn datoer_og_tekster() {
        assert_eq!(dmi_date("03/06/2019").as_deref(), Some("6. mars 2019"));
        assert_eq!(dmi_date("tull"), None);
        assert_eq!(kernel_build("#31-Ubuntu SMP PREEMPT_DYNAMIC Sat Aug  1 04:26:38 UTC 2026"), "Bygg #31 · 1. aug. 2026 · PREEMPT_DYNAMIC");
        assert_eq!(desktop_text("ubuntu:GNOME").as_deref(), Some("GNOME (Ubuntu-økt)"));
        assert_eq!(desktop_text("KDE").as_deref(), Some("KDE"));
        assert_eq!(cpu_list("0-3,8-11"), "0–3, 8–11");
        assert_eq!(vuln_text("Not affected"), "Ikke berørt");
        assert_eq!(vuln_text("Mitigation: Retpolines; IBPB: conditional"), "Tiltak: Retpolines; IBPB: conditional");
        assert_eq!(vuln_text("Vulnerable: SMT vulnerable"), "Sårbar: SMT vulnerable");
        assert_eq!(vuln_name("spec_store_bypass"), "Speculative Store Bypass");
        assert_eq!(vuln_name("l1tf"), "L1TF");
        assert_eq!(vuln_name("ny_sarbarhet"), "Ny sarbarhet");
    }

    #[test]
    fn diskene_i_rapporten_med_helse() {
        let sys = serde_json::json!({"disks": {"t": 1, "source": "udisks", "disks": [{"name": "sda", "model": "ST4000DM000-2AE166", "kind": "HDD", "bus": "SATA", "size": 4.0e12,
            "smart": {"temp": 38.0}, "health": {"level": "warn", "reasons": ["10 dårlige sektorer"]}}]}});
        let d = disks_section(&sys);
        let t = crate::report::render_sections(&[d], crate::report::ReportFormat::Text, 2);
        assert!(t.contains("Advarsel") && t.contains("38") && t.contains("sda: 10 dårlige sektorer"), "{t}");
    }
}
