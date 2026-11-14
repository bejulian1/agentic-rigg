//! Sikkerhet (arkitektur.md kap. 8, som `test/nvidia-safety.test.js` i demoen):
//! discovery/prøvetaking forblir rent lesende. Maskinvareskriving finnes kun
//! i hjelperens featurestyrte, kortbundne backend med bekreftet baseline.
//! Ingen test her åpner en GPU eller gjør en maskinvareskriving.
//!
//! Kommentarer og testmoduler (alt etter `#[cfg(test)]`) søkes ikke i, fordi
//! de nevner de forbudte navnene for å forklare eller teste forbudet.
//!
//! **Unntakene fra runde 7A (root-hjelperen), og bare de:**
//! - `rigg-hw/src/gpu/skriv_nvml.rs` har NVML-skrivekallene. Fila kompileres
//!   bare med cargo-funksjonen `ekte-skriving`, som bare hjelperen slår på.
//!   Ubundet åpning er avvist; hjelperen krever også global tillatelse,
//!   kortidentitet og vedvarende baseline før første skrivekall.
//! - `rigg-hw/src/gpu/skriv.rs` og `rigg-helper/` har `.set_fan…`/`.set_power…`
//!   mot det avgrensede skrivegrensesnittet.
//! - `rigg/src/settings/styring.rs` starter `pkexec` når brukeren selv slår
//!   på bryteren (passordvinduet er polkit sitt).

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "fixtures" || n == "target")
            {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Koden uten kommentarer og uten testmodulen nederst.
fn code(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let body = match text.find("#[cfg(test)]") {
        Some(i) => &text[..i],
        None => &text[..],
    };
    body.lines()
        .map(|l| match l.find("//") {
            // Ikke kutt «//» inne i en tekst (f.eks. URL-er).
            Some(i) if l[..i].matches('"').count() % 2 == 0 => &l[..i],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rel(p: &Path) -> String {
    p.strip_prefix(crates_dir())
        .unwrap_or(p)
        .display()
        .to_string()
}

#[test]
fn ingen_skriving_til_kortet_i_koden() {
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    assert!(files.len() > 20, "fant bare {} filer", files.len());
    const FORBIDDEN: [&str; 10] = [
        "nvmlDeviceSet",
        "nvmlDeviceReset",
        "nvmlDeviceClear",
        "nvmlUnitSet",
        "/dev/i2c",
        "nvidia-smi",
        "sudo ",
        "pkexec",
        ".set_power",
        ".set_fan",
    ];
    // Bare disse modulene kaller skrivegrensesnittet. Rå NVML-settere
    // har en separat, snevrere grense nedenfor.
    let writer_call_sites = |f: &str| {
        f.ends_with("gpu/sim.rs")
            || f.ends_with("rigg-core/src/txn.rs")
            || f.ends_with("gpu/skriv.rs")
            || f.ends_with("gpu/skriv_nvml.rs")
            || f.starts_with("rigg-helper/")
    };
    // NVML-skrivekallene: bare den sperrede fila. pkexec: bare bryteren.
    let gated = |f: &str, pat: &str| {
        (f.ends_with("gpu/skriv_nvml.rs") && pat.starts_with("nvmlDevice")) || (f.ends_with("rigg/src/settings/styring.rs") && pat == "pkexec")
        // Skripttestene nevner PKEXEC_UID (som pkexec setter); de kjører aldri pkexec.
        || (f.starts_with("rigg-helper/tests/") && pat == "pkexec")
    };
    let mut hits = Vec::new();
    for f in &files {
        let name = rel(f);
        if name.ends_with("tests/bare_lesing.rs") {
            continue;
        }
        let c = code(f);
        for pat in FORBIDDEN {
            // Kun den eksakte strengliteralen med testscriptets navn er
            // et harmløst treff, ikke andre pkexec-referanser i fixturefila.
            let hit = if name == "rigg/src/skjermbilder_ui140.rs" && pat == "pkexec" {
                c.replace("\"/falsk-pkexec.sh\"", "").contains(pat)
            } else {
                c.contains(pat)
            };
            if hit && !(writer_call_sites(&name) && pat.starts_with(".set_")) && !gated(&name, pat)
            {
                hits.push(format!("{name}: {pat}"));
            }
        }
    }
    assert!(hits.is_empty(), "forbudt i koden:\n{}", hits.join("\n"));
}

#[test]
fn ingen_programmer_startes() {
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    let mut hits = Vec::new();
    for f in &files {
        let name = rel(f);
        // Testene starter tjenesten som barneprosess, og reserven i vinduet
        // starter tjenesten når systemd --user mangler (arkitektur.md kap. 8).
        if name.contains("/tests/") || name.ends_with("rigg/src/client.rs") {
            continue;
        }
        let c = code(f);
        // Bryteren i Innstillinger › Styring starter pkexec (brukerens handling, 7A).
        if name.ends_with("rigg/src/settings/styring.rs") {
            assert!(
                c.contains("/usr/bin/pkexec") && !c.contains("Command::new"),
                "styring.rs skal bare starte pkexec med gio::Subprocess"
            );
            continue;
        }
        if c.contains("Command::new")
            || c.contains("process::Command")
            || c.contains("libc::exec")
            || c.contains("libc::fork")
            || c.contains("Subprocess")
        {
            hits.push(name);
        }
    }
    assert!(hits.is_empty(), "starter programmer: {hits:?}");
}

#[test]
fn alle_nvml_navn_er_lesefunksjoner() {
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    let mut names = Vec::new();
    for f in &files {
        let c = code(f);
        let mut rest = c.as_str();
        while let Some(i) = rest.find("\"nvml") {
            let s = &rest[i + 1..];
            let end = s
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or(s.len());
            // Funksjonsnavn: «nvml» og en stor bokstav («nvml» alene er backend-navnet).
            if s.as_bytes()[..end]
                .get(4)
                .is_some_and(u8::is_ascii_uppercase)
            {
                names.push((rel(f), s[..end].to_string()));
            }
            rest = &s[end..];
        }
    }
    assert!(names.len() > 20, "fant bare {} NVML-navn", names.len());
    // Den sperrede skrivefila har sin egen, lukkede liste (7A).
    const SKRIV: [&str; 6] = [
        "nvmlDeviceSetPowerManagementLimit",
        "nvmlDeviceSetClockOffsets",
        "nvmlDeviceSetTemperatureThreshold",
        "nvmlDeviceSetFanSpeed_v2",
        "nvmlDeviceSetDefaultFanSpeed_v2",
        "nvmlDeviceSetFanControlPolicy",
    ];
    let ok = |f: &str, n: &str| {
        rigg_hw::gpu::nvml::allowed(n) || (f.ends_with("gpu/skriv_nvml.rs") && SKRIV.contains(&n))
    };
    let bad: Vec<_> = names.iter().filter(|(f, n)| !ok(f, n)).collect();
    assert!(bad.is_empty(), "ikke lesefunksjoner: {bad:?}");
}

/// Prøvetakingens symbolfilter avviser også symbolene helperen nå støtter.
#[test]
fn discovery_kan_ikke_bruke_skrivebackenden() {
    for name in [
        "nvmlDeviceSetPowerManagementLimit",
        "nvmlDeviceSetClockOffsets",
        "nvmlDeviceSetFanSpeed_v2",
        "nvmlDeviceSetDefaultFanSpeed_v2",
        "nvmlDeviceSetFanControlPolicy",
        "nvmlDeviceSetTemperatureThreshold",
        "nvmlDeviceResetGpuLockedClocks",
        "nvmlDeviceClearEccErrorCounts",
    ] {
        assert!(
            !rigg_hw::gpu::nvml::allowed(name),
            "discovery tillater {name}"
        );
    }
    for name in ["gpu/nvml.rs", "gpu/nvml_info.rs"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(name);
        let c = code(&path);
        for forbidden in [
            "skriv_nvml::",
            "gpu::skriv",
            "open_nvml_bound",
            "SimSkriver",
            "dyn Skriver",
            ".set_power",
            ".set_fan",
            ".set_offset",
            ".set_temp",
            ".auto_fan",
        ] {
            assert!(!c.contains(forbidden), "{name} bruker {forbidden}");
        }
    }
}

#[test]
fn nvml_writer_krever_bundet_baseline_foer_skriveinngangene() {
    let c = code(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gpu/skriv_nvml.rs"));
    assert!(
        c.contains("baseline_ready: false"),
        "åpning må starte uten skriveklar baseline"
    );
    assert!(c.contains("if self.baseline_ready") && c.contains("baseline_required:"));
    let load = c
        .split("fn load_baseline(")
        .nth(1)
        .unwrap()
        .split("\n    fn ")
        .next()
        .unwrap();
    assert!(
        load.contains("baseline[\"identity\"] != self.identity"),
        "baseline må høre til kort, driver og VBIOS"
    );
    assert!(load.contains("self.baseline_ready = true"));
    assert!(
        !load.contains("self.api.set_") && !load.contains("self.api.auto_fan"),
        "baselinegodkjenning skal ikke skrive"
    );
    for method in ["apply", "set_fan_percents", "restore_to"] {
        let marker = format!("fn {method}(");
        let body = c
            .split(&marker)
            .nth(1)
            .unwrap()
            .split("\n    fn ")
            .next()
            .unwrap();
        assert!(
            body.split_once('{')
                .unwrap()
                .1
                .trim_start()
                .starts_with("self.guard()?;"),
            "{method} mangler baselineport før handlinger"
        );
    }
    let open = c
        .split("pub fn open(")
        .nth(1)
        .unwrap()
        .split("\nimpl ")
        .next()
        .unwrap();
    assert!(
        open.find("valid_binding(pci, uuid)").unwrap() < open.find("libc::dlopen").unwrap(),
        "kortbinding skal avvises før NVML åpnes"
    );
    assert!(
        !open.contains("self.api.set_") && !open.contains("self.api.auto_fan"),
        "åpning/discovery skal ikke skrive"
    );
}

/// Bare hjelperen aktiverer skrivedelen; ubundet åpning er alltid avvist.
#[test]
fn ekte_skriving_er_kortbundet_og_bare_for_hjelperen() {
    assert_eq!(
        rigg_hw::gpu::skriv::real_writes_available(),
        cfg!(feature = "ekte-skriving")
    );
    assert!(rigg_hw::gpu::skriv::open_nvml().is_err());
    assert!(rigg_hw::gpu::skriv::open_nvml_bound("", "").is_err());
    let m = std::fs::read_to_string(crates_dir().join("rigg-hw/src/gpu/mod.rs")).unwrap();
    assert!(
        m.contains("#[cfg(feature = \"ekte-skriving\")]\npub mod skriv_nvml;"),
        "skriv_nvml.rs må stå bak cargo-funksjonen"
    );
    // Bare rigg-helper slår på funksjonen (rigg-hw definerer den).
    let mut tomls = vec![crates_dir().join("../Cargo.toml")];
    for e in std::fs::read_dir(crates_dir()).unwrap().flatten() {
        tomls.push(e.path().join("Cargo.toml"));
    }
    for t in tomls {
        let Ok(text) = std::fs::read_to_string(&t) else {
            continue;
        };
        let uses = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter(|l| l.contains("ekte-skriving"))
            .count();
        let allowed = if t.ends_with("rigg-hw/Cargo.toml") || t.ends_with("rigg-helper/Cargo.toml")
        {
            1
        } else {
            0
        };
        assert_eq!(uses, allowed, "{} nevner ekte-skriving", t.display());
    }
    // Bare hjelperen (og rigg-hw selv) bruker skrivemodulen.
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    for f in &files {
        let name = rel(f);
        if name.starts_with("rigg-helper/") || name.starts_with("rigg-hw/") {
            continue;
        }
        let c = code(f);
        assert!(
            !c.contains("gpu::skriv") && !c.contains("open_nvml") && !c.contains("SimSkriver"),
            "{name} bruker skrivemodulen"
        );
    }
}

#[test]
fn i2c_bussene_åpnes_ikke() {
    // Å lese navnene i /sys/bus/i2c/devices/*/name er trygt, men koden gjør
    // ikke det heller i 1.0. Ingen kode skal nevne i2c-dev eller ioctl.
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    for f in &files {
        let c = code(f);
        assert!(
            !c.contains("i2c-dev") && !c.contains("I2C_SLAVE") && !c.contains("libc::ioctl"),
            "{}",
            rel(f)
        );
    }
}
