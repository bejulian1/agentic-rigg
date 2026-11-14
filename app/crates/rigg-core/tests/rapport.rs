//! Rapporten og CSV-formatene (produktspesifikasjonen 3.2 og 3.6): det som
//! står i rapporten, at serienumre og UUID ikke er med som standard, at de
//! to CSV-variantene kan leses tilbake, og at sidene og rapporten bruker de
//! samme seksjonene.

use rigg_core::csv::{self, CsvFormat};
use rigg_core::model::*;
use rigg_core::report::{self, ReportFormat, ReportInput};
use serde_json::{Value, json};

const ID: &str = "0000:1c:00.0";
const UUID: &str = "GPU-eaf05f06-1111-2222-3333-444455556666";

fn statics() -> GpuStatic {
    GpuStatic {
        id: ID.into(),
        name: "NVIDIA GeForce RTX 2080".into(),
        uuid: Some(UUID.into()),
        driver: Some("610.43.02".into()),
        nvml_version: Some("13.610.43.02".into()),
        cuda_version: Some("13.3".into()),
        vbios: Some("90.04.0B.40.8C".into()),
        pci_device_id: Some(0x1e87_10de),
        pci_subsystem_id: Some(0x1e87_10de),
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
        extra: json!({
            "brand": "GeForce", "architecture": "Turing", "computeCap": "7.5", "cudaCores": 2944, "busWidth": 256,
            "gspFirmware": "610.43.02", "gspMode": {"enabled": true, "default": true}, "inforomImage": "G001.0000.02.04",
            "bar1": {"totalMiB": 256, "usedMiB": 17}, "kernelModule": "Open Kernel Module",
            "pstates": ["P0", "P2", "P8"],
            "pstateClocks": [{"p": "P0", "gfx": [300, 2190], "mem": [7000, 7000]}, {"p": "P8", "gfx": [300, 645], "mem": [405, 405]}],
            "acousticMin": 65, "acousticMax": 88,
            "offsetGfx": {"now": 0, "min": -1000, "max": 1000}, "offsetMem": {"now": 0, "min": -2000, "max": 6000},
            "fans": [{"policy": 0, "target": 35, "signal": 2, "coolerTarget": 14}, {"policy": 0, "target": 35, "signal": 2, "coolerTarget": 14}],
            "thermal": [{"controller": 1, "min": 0, "max": 127, "cur": 31, "target": 1}],
            "processes": {"t": 1_794_058_330_000i64, "list": [{"pid": 1234, "name": "gnome-shell", "vramMiB": 95, "kind": "grafikk"}]},
            "pci": {"revision": "a1", "driver": "nvidia", "bars": [{"bar": 1, "size": 268_435_456u64}], "bar1Sizes": [64, 128, 256], "aerCorrectable": 0},
            "pcieReplay": 0,
            "na": {"serial": "NVML_ERROR_NOT_SUPPORTED", "memMax": "NVML_ERROR_NOT_SUPPORTED", "margin": "NVML_ERROR_NOT_SUPPORTED",
                   "defaultAppClockGfx": "NVML_ERROR_NOT_SUPPORTED", "partNumber": "NVML_ERROR_NOT_SUPPORTED"}
        }),
    }
}

fn telemetry() -> Telemetry {
    let mut t = Telemetry { t: 1_794_058_330_000, ..Default::default() };
    t.gpus.insert(
        ID.into(),
        GpuSample {
            temp: Some(31.0),
            power: Some(12.3),
            power_limit: Some(245.0),
            fans: vec![Some(35.0), Some(35.0)],
            fan_rpm: vec![Some(801.0), Some(798.0)],
            pcie_gen: Some(1.0),
            pcie_width: Some(16.0),
            core_clock: Some(300.0),
            pstate: Some("P8".into()),
            ..Default::default()
        },
    );
    t.sys.cpu.load = Some(4.0);
    t.sys.cpu.temp = Some(37.375);
    t.sys.cpu.tctl = Some(47.375);
    t.sys.cpu.core_load = vec![Some(3.0), Some(5.0)];
    t.sys.cpu.core_mhz = vec![Some(2200.0), Some(4300.0)];
    t.sys.mem.total_kb = Some(49_270_320.0);
    t.sys.mem.used_kb = Some(18_000_000.0);
    t
}

fn system() -> Value {
    json!({
        "cpu": {"name": "AMD Ryzen 7 2700X Eight-Core Processor", "vendorName": "AMD", "vendor": "AuthenticAMD", "family": 23, "model": 8, "stepping": 2,
                "cores": 8, "threads": 16, "caches": [{"level": 1, "type": "Data", "size": 32768, "instances": 8, "total": 262_144, "ways": 8}],
                "flags": ["sse", "avx2"], "important": [{"flag": "avx2", "label": "AVX2"}], "freq": {"minKHz": 2_200_000, "maxKHz": 3_700_000, "driver": "acpi-cpufreq"},
                "codename": {"name": "Pinnacle Ridge (Zen+)", "process": "12 nm", "socket": "AM4", "revision": "PiR-B2"}, "l3Groups": ["0-3,8-11", "4-7,12-15"]},
        "board": {"board_vendor": "Micro-Star International Co., Ltd.", "board_name": "B450 GAMING PRO CARBON AC (MS-7B85)", "bios_date": "03/06/2019", "spd": {"chips": 4, "driver": "ee1004"}},
        "os": {"distro": "Ubuntu 26.04.1 LTS", "kernel": "7.0.0-31-generic", "hostname": "hemmelig-vert", "disks": [{"name": "sdb", "model": "Samsung SSD 870", "kind": "SSD", "bus": "SATA", "bytes": 1_000_204_886_016u64, "system": true}]},
        "pci": {"chipset": "AMD 400 Series Chipset"}
    })
}

fn input(serials: bool) -> ReportInput {
    let tele = telemetry();
    ReportInput {
        gpus: json!([{"id": ID, "index": 0, "name": "NVIDIA GeForce RTX 2080", "state": "ok", "since": 0, "driver": "610.43.02"}]),
        statics: serde_json::to_value(vec![statics()]).unwrap(),
        system: system(),
        stats: json!({"since": 1_794_050_000_000i64, "avgSince": 1_794_050_000_000i64, "stats": {"gpu:0000:1c:00.0/temp": {"min": 30.0, "max": 52.0, "avg": 33.5, "n": 100}}}),
        telemetry: serde_json::to_value(&tele).unwrap(),
        service: json!({"name": "agentic-riggd", "version": "1.0.0", "build": 16, "backend": "nvml"}),
        env: vec![("GTK".into(), "4.22.4".into())],
        desktop: Some("ubuntu:GNOME".into()),
        session: Some("Wayland".into()),
        include_serials: serials,
        created: "7. nov. 2026 14:32".into(),
        utc_offset_s: 3600,
    }
}

#[test]
fn rapporten_har_alt_spesifikasjonen_krever() {
    let t = report::render(&input(false), ReportFormat::Text);
    for want in [
        "Agentic Rigg · rapport",
        "Laget: 7. nov. 2026 14:32",
        "agentic-riggd 1.0.0 · bygg 16 · NVML, bare lesing",
        "Operativsystem: Ubuntu 26.04.1 LTS",
        "Kjerne: 7.0.0-31-generic",
        "NVIDIA-driver: 610.43.02",
        "NVML: 13.610.43.02",
        "Skjermkort · RTX 2080",
        "Tilstand: Leses med NVML",
        "[Fra driveren]",
        "[Spesifikasjon (referanse, ikke målt)]",
        "CUDA-kjerner: 2\u{202F}944",
        "[Datatilgjengelighet (ikke innsamlet eller utilgjengelig)]",
        "Hotspot-temperatur: Ikke innsamlet",
        "Prosessor",
        "Pinnacle Ridge (Zen+) · 12 nm · AM4 · PiR-B2",
        "Brikkesett: AMD 400 Series Chipset",
        "BIOS-dato: 6. mars 2019",
        "Moduler og timinger: Krever godkjenning",
        "Skrivebord: GNOME (Ubuntu-økt) · Wayland",
        "[Disker]",
        "[Sensorer]",
        "Min og maks siden 2026-11-07T12:13:20+01:00.",
    ] {
        assert!(t.contains(want), "mangler «{want}» i rapporten:\n{t}");
    }
    // Nå/Min/Maks/Snitt for GPU-temperaturen.
    let row = t.lines().find(|l| l.starts_with("GPU · Temperatur")).expect("sensorraden");
    assert!(row.contains("31\u{202F}°C") && row.contains("30\u{202F}°C") && row.contains("52\u{202F}°C") && row.contains("34\u{202F}°C"), "{row}");
}

#[test]
fn cpu_lansering_krever_eksakt_modell_og_har_kilde_i_visning_og_rapport() {
    const SOURCE: &str = "https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-2000-series/amd-ryzen-7-2700x.html";
    let mut data = input(false);
    let sections = report::system::sections(&report::system::SysInput {
        system: &data.system,
        sys: None,
        show_hostname: false,
        desktop: None,
        session: None,
        report: false,
        max_thread_mhz: None,
    });
    let launch = sections.iter().find(|s| s.id == "prosessor").unwrap().find("lansert").unwrap();
    assert_eq!(launch.k, "Lansert");
    assert_eq!(launch.v.as_deref(), Some("19. april 2018"));
    let info = launch.info.as_deref().expect("kilden står i sidens verktøytips");
    assert!(info.contains("Modelloppslag") && info.contains(SOURCE));
    for fmt in [ReportFormat::Text, ReportFormat::Markdown] {
        let text = report::render(&data, fmt);
        assert!(text.contains("19. april 2018") && text.contains(SOURCE), "dato og kilde i {fmt:?}");
    }
    // Samme CPUID/kodenavn er ikke nok: nabomodeller og feil leverandør
    // skal ikke arve 2700X-datoen i hverken tekst- eller Markdown-rapporten.
    for (vendor, name) in [
        ("AuthenticAMD", "AMD Ryzen 7 2700 Eight-Core Processor"),
        ("AuthenticAMD", "AMD Ryzen 7 2700U"),
        ("AuthenticAMD", "AMD Ryzen 7 2700X PRO"),
        ("AuthenticAMD", "AMD Ryzen 7 PRO 2700X"),
        ("AuthenticAMD", "Ukjent Pinnacle Ridge"),
        ("GenuineIntel", "AMD Ryzen 7 2700X Eight-Core Processor"),
        ("", "AMD Ryzen 7 2700X"),
    ] {
        data.system["cpu"]["vendor"] = json!(vendor);
        data.system["cpu"]["name"] = json!(name);
        for fmt in [ReportFormat::Text, ReportFormat::Markdown] {
            let text = report::render(&data, fmt);
            assert!(!text.contains("19. april 2018") && !text.contains(SOURCE), "falskt treff for {vendor}: {name}");
            assert!(text.contains("Ikke i modelloppslaget"));
        }
    }
}

#[test]
fn serienumre_uuid_og_vertsnavn_er_ikke_med_som_standard() {
    let t = report::render(&input(false), ReportFormat::Markdown);
    assert!(!t.contains("eaf05f06-1111"), "UUID-en skal ikke stå i rapporten");
    assert!(!t.contains("hemmelig-vert"), "vertsnavnet skal ikke stå i rapporten");
    assert!(t.contains("| Vertsnavn | Ikke tatt med |"));
    assert!(t.contains("Ikke tatt med (velg «Ta med serienumre og UUID» når du lagrer)"));
    let with = report::render(&input(true), ReportFormat::Markdown);
    assert!(with.contains(UUID));
    assert!(with.contains("hemmelig-vert"));
}

#[test]
fn markdown_har_overskrifter_og_tabeller() {
    let m = report::render(&input(false), ReportFormat::Markdown);
    assert!(m.starts_with("# Agentic Rigg · rapport\n\n| Felt | Verdi |\n|---|---|\n| App | Agentic Rigg "));
    assert!(m.contains("\n## Skjermkort · RTX 2080\n"));
    assert!(m.contains("\n### Klokker (MHz)\n"));
    assert!(m.contains("\n### Spesifikasjon (referanse, ikke målt)\n"));
    assert!(m.contains("| Sensor | Nå | Min | Maks | Snitt |\n|---|---:|---:|---:|---:|\n"));
    // Ingen tabellrad brytes av en «|» i en verdi.
    for l in m.lines().filter(|l| l.starts_with("| ")) {
        assert!(l.ends_with(" |") || l.ends_with("|"), "{l}");
    }
}

#[test]
fn uten_driver_sier_rapporten_det() {
    let mut i = input(false);
    i.gpus = json!([{"id": "", "index": 0, "name": "", "state": "no_driver", "since": 0, "detail": "Fant ikke NVIDIA-driveren (libnvidia-ml.so.1)"}]);
    i.statics = json!([]);
    let t = report::render(&i, ReportFormat::Text);
    assert!(t.contains("[Skjermkort]\nTilstand: Fant ingen NVIDIA-driver"), "{t}");
    assert!(t.contains("[Prosessor]"), "prosessoren skal stå der likevel");
}

#[test]
fn sidene_og_rapporten_bruker_samme_seksjoner() {
    let st = statics();
    let tele = telemetry();
    let g = report::gpu::GpuInput { info: None, st: &st, s: tele.gpus.get(ID), show_uuid: false, egg: None, utc_offset_s: 3600 };
    let secs = report::gpu::sections(&g);
    let ids: Vec<&str> = secs.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec!["driver", "buss", "klokker", "pstates", "grenser", "vifter", "ikke-stottet", "prosesser", "spesifikasjon"]);
    let d = &secs[0];
    assert_eq!(d.find("uuid").unwrap().text(), "GPU-eaf0••••-••••-••••-••••-••••••••••••");
    assert_eq!(d.find("serie").unwrap().text(), "Ikke oppgitt av kortet");
    let b = &secs[1];
    assert_eq!(b.find("rebar").unwrap().text(), "Ikke støttet · største BAR1 er 256\u{202F}MiB");
    assert_eq!(b.find("pcie").unwrap().text(), "x16 Gen\u{a0}1 · maks Gen\u{a0}3");
    let l = &secs[4];
    assert_eq!(l.find("offset-minne").unwrap().text(), "0\u{202F}MHz · område \u{2212}1\u{202F}000…+3\u{202F}000\u{202F}MHz");
    assert_eq!(l.find("avstand").unwrap().text(), "66\u{202F}°C");
    let f = &secs[5];
    assert_eq!(f.find("vifte-2").unwrap().text(), "35\u{202F}% · 798\u{202F}o/min · mål 35\u{202F}%");
    assert!(f.find("antall").unwrap().info.as_deref().unwrap().contains("GPU, minne, strømforsyning"));
    let page = report::render_page("Agentic Rigg – Grafikkort", &secs, ReportFormat::Text);
    assert!(page.starts_with("Agentic Rigg – Grafikkort\n\n[Fra driveren]\nNavn: NVIDIA GeForce RTX 2080\n"));
}

#[test]
fn begge_csv_formatene_kan_leses_tilbake() {
    let t = telemetry();
    let keys = ["gpu:0000:1c:00.0/temp", "gpu:0000:1c:00.0/power", "gpu:0000:1c:00.0/encUtil", "cpu/temp"];
    let cols: Vec<csv::Column> = keys.iter().map(|k| csv::column(k, Some(&t))).collect();
    let values = [Some(31.0), Some(12.34), None, Some(37.375)];
    for (f, sep, dec) in [(CsvFormat::Standard, ',', '.'), (CsvFormat::Norsk, ';', ',')] {
        let mut text = csv::header_line(&cols, f);
        text += &csv::row_line(&csv::iso_time(t.t, 3600, false), 0.0, &cols, &values, f);
        text += &csv::row_line(&csv::iso_time(t.t + 1000, 3600, false), 1.0, &cols, &values, f);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        let head: Vec<&str> = lines[0].split(sep).collect();
        assert_eq!(head, vec!["Tid", "Sekunder", "GPU · Temperatur [°C]", "GPU · Effekttrekk [W]", "GPU · Videokoder [%]", "Prosessor · temperatur (Tdie) [°C]"]);
        let r: Vec<&str> = lines[2].split(sep).collect();
        assert_eq!(r.len(), head.len(), "fast antall kolonner");
        assert_eq!(r[0], "2026-11-07T14:32:11+01:00");
        assert_eq!(r[1], format!("1{dec}0"));
        assert_eq!(r[3], format!("12{dec}3"));
        assert_eq!(r[4], "", "tomt felt når verdien mangler");
        assert_eq!(r[5], format!("37{dec}4"));
        // Tilbake til tall.
        let back: f64 = r[3].replace(',', ".").parse().unwrap();
        assert!((back - 12.3).abs() < 1e-9);
    }
}


/// Prosesslista: øyeblikksbildet fra de faste dataene, eller den levende
/// lista fra `get_processes` mens Grafikkort vises (runde 3A).
#[test]
fn prosesslista_sier_om_den_lever() {
    use rigg_core::report::Item;
    let note = |st: &GpuStatic| -> String {
        let g = report::gpu::GpuInput { info: None, st, s: None, show_uuid: false, egg: None, utc_offset_s: 3600 };
        let secs = report::gpu::sections(&g);
        let p = secs.iter().find(|s| s.id == "prosesser").unwrap();
        p.items.iter().find_map(|i| if let Item::Note(n) = i { Some(n.clone()) } else { None }).unwrap()
    };
    let mut st = statics();
    assert!(note(&st).starts_with("Øyeblikksbilde fra kl. "), "{}", note(&st));
    st.extra["processes"]["live"] = json!(true);
    assert!(note(&st).starts_with("Lest kl. ") && note(&st).contains("mens siden vises"), "{}", note(&st));
    st.extra["processes"]["list"] = json!([]);
    assert!(note(&st).starts_with("Ingen prosesser bruker skjermkortet"), "{}", note(&st));
}
