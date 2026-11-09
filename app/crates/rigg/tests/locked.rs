//! Låsen i vinduet (produktspesifikasjonen 6.1): med NVML-backenden er ingen
//! Bruk aktiv, og ingen skriveoperasjon kan sendes. Det gjelder i hele
//! kjeden: porten i vinduet (`rigg_core::txn::write_gate`), at alle
//! skriveoperasjonene går gjennom `widgets::locked::send_write`, og at boka
//! svarer `write_locked` for alt annet enn den simulerte backenden.
//!
//! **Eies av 2C (Styring og Lys).**

use rigg_core::proto::{self, ErrorCode};
use rigg_core::txn::{Book, Gate, write_gate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn hello(backend: &str) -> Value {
    json!({
        "daemon": {"backend": backend, "build": 16},
        "capabilities": {"ops": proto::ops_for(backend), "write": proto::write_capabilities(backend)},
    })
}

#[test]
fn ingen_bruk_er_aktiv_med_nvml() {
    for dev in [false, true] {
        assert!(matches!(write_gate(Some(&hello("nvml")), dev), Gate::Locked(_)), "nvml, dev={dev}");
        assert!(matches!(write_gate(Some(&hello("ingen")), dev), Gate::Locked(_)), "ingen, dev={dev}");
    }
    // Før tjenesten har svart, er alt låst.
    assert!(matches!(write_gate(None, true), Gate::Locked(_)));
    // Bare en utviklerforekomst mot den simulerte backenden kan prøve flyten.
    assert_eq!(write_gate(Some(&hello("sim")), true), Gate::Sim);
    assert!(matches!(write_gate(Some(&hello("sim")), false), Gate::Locked(_)));
    // Låst-teksten er den produktspesifikasjonen 6.3 gir.
    let Gate::Locked(t) = write_gate(Some(&hello("nvml")), true) else { panic!() };
    assert_eq!(t, "Bruk er låst: denne versjonen endrer ikke det ekte kortet");
}

#[test]
fn tjenesten_med_nvml_tilbyr_ingen_skriveoperasjoner() {
    let h = hello("nvml");
    let ops: Vec<&str> = h["capabilities"]["ops"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    for op in proto::WRITE_OPS {
        assert!(!ops.contains(op), "{op} tilbys med nvml");
    }
    assert_eq!(h["capabilities"]["write"]["state"], "locked");
    // «Hva skal til?»: alle seks trinnene er ikke godkjent.
    let steps = rigg_core::txn::steps(Some(&h));
    assert_eq!(steps.len(), 6);
    assert!(steps.iter().all(|s| s.2.starts_with("Ikke godkjent")));
}

#[test]
fn boka_svarer_write_locked_for_alt_unntatt_sim() {
    for backend in ["nvml", "ingen", ""] {
        let mut b = Book::new();
        for op in proto::WRITE_OPS {
            let e = b.handle(backend, op, &json!({"settings": {"powerLimitW": 150}, "watts": 150, "txn": "t1"}), "s1", 0).unwrap_err();
            assert_eq!(e.code, ErrorCode::WriteLocked, "{backend}/{op}");
        }
        assert!(b.pending().is_none());
    }
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Koden uten kommentarer (som `rigg-hw/tests/bare_lesing.rs`).
fn code(p: &Path) -> String {
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|l| match l.find("//") {
            Some(i) if l[..i].matches('"').count() % 2 == 0 => &l[..i],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ingen fil sender en skriveoperasjon rett til tjenesten. Alt går gjennom
/// `locked::send_write`, som sjekker porten først.
#[test]
fn skriveoperasjonene_gaar_gjennom_porten() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(files.len() > 30);
    let mut hits = Vec::new();
    for f in &files {
        let c = code(f).replace(char::is_whitespace, "");
        for op in proto::WRITE_OPS {
            if c.contains(&format!("request(\"{op}\"")) {
                hits.push(format!("{}: request(\"{op}\")", f.display()));
            }
        }
    }
    assert!(hits.is_empty(), "skriveoperasjoner utenom porten:\n{}", hits.join("\n"));
    // Porten selv: sjekker operasjonen og at porten er åpen før noe sendes.
    let locked = code(&src_dir().join("widgets/locked.rs")).replace(char::is_whitespace, "");
    let body = locked.split("pubfnsend_write").nth(1).expect("send_write finnes");
    let body = &body[..body.find("\n}").unwrap_or(body.len()).min(400)];
    assert!(body.contains("WRITE_OPS.contains(&op)") && body.contains("gate(app)!=Gate::Sim"), "send_write må sjekke porten: {body}");
    assert!(body.find("gate(app)").unwrap() < body.find("client.request").unwrap(), "porten sjekkes før forespørselen");
}

/// Bare sidene under Styring bruker `send_write`, og de setter Bruk aktiv
/// bare når porten er `Gate::Sim`.
#[test]
fn bruk_er_bare_aktiv_med_aapen_port() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let c = code(f);
        if c.contains("send_write(") {
            assert!(["kontroller.rs", "viftekurve.rs", "locked.rs"].contains(&name.as_str()), "{name} bruker send_write");
        }
        if ["kontroller.rs", "viftekurve.rs"].contains(&name.as_str()) {
            // Hver gang Bruk kan bli aktiv, er det i grenen for Gate::Sim.
            let compact = c.replace(char::is_whitespace, "");
            let mut rest = compact.as_str();
            let mut n = 0;
            while let Some(i) = rest.find("self.apply.set_sensitive(") {
                let before = &rest[..i];
                let arm = before.rfind("Gate::Sim=>").unwrap_or(0);
                let locked = before.rfind("Gate::Locked").unwrap_or(0);
                let after = &rest[i + "self.apply.set_sensitive(".len()..];
                if !after.starts_with("false") {
                    assert!(arm > locked && arm > 0, "{name}: Bruk kan bli aktiv utenfor Gate::Sim");
                }
                n += 1;
                rest = after;
            }
            assert!(n >= 2, "{name}: fant ikke Bruk-knappen ({n})");
        }
    }
}
