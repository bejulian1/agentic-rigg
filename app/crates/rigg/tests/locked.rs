//! Skriveporten krever enten utviklersimulering eller en aktivert, klar
//! NVML-hjelper. Kildesjekkene dekker vinduets ekstra kortbinding og alle
//! skrivekall uten å opprette GTK-vinduer eller bruke ekte maskinvare.

use rigg_core::proto::{self, ErrorCode};
use rigg_core::txn::{Book, Gate, write_gate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn hello(backend: &str) -> Value {
    json!({
        "daemon": {"backend": backend},
        "capabilities": {"ops": proto::ops_for(backend), "write": proto::write_capabilities(backend)},
    })
}

#[test]
fn uaktivert_nvml_og_ukjent_backend_er_laast() {
    for dev in [false, true] {
        assert!(
            matches!(write_gate(Some(&hello("nvml")), dev), Gate::Locked(_)),
            "nvml, dev={dev}"
        );
        assert!(
            matches!(write_gate(Some(&hello("ingen")), dev), Gate::Locked(_)),
            "ingen, dev={dev}"
        );
    }
    // Før tjenesten har svart, er alt låst.
    assert!(matches!(write_gate(None, true), Gate::Locked(_)));
    // Simuleringen krever fortsatt en utviklerforekomst.
    assert_eq!(write_gate(Some(&hello("sim")), true), Gate::Sim);
    assert!(matches!(
        write_gate(Some(&hello("sim")), false),
        Gate::Locked(_)
    ));
}

#[test]
fn ekte_port_krever_alle_tillatelsene_ogsaa_i_utviklermodus() {
    let mut ready = hello("nvml");
    ready["capabilities"]["write"]["state"] = json!("ready");
    ready["capabilities"]["write"]["helper"] = json!({
        "enabled": true, "ready": true, "realWrites": true,
        "identity": {"pci": "0000:01:00.0", "uuid": "GPU-test"},
    });
    for dev in [false, true] {
        assert_eq!(write_gate(Some(&ready), dev), Gate::Real);
        for field in ["enabled", "ready", "realWrites"] {
            for value in [json!(false), Value::Null, json!("true")] {
                let mut denied = ready.clone();
                denied["capabilities"]["write"]["helper"][field] = value;
                assert!(
                    matches!(write_gate(Some(&denied), dev), Gate::Locked(_)),
                    "{field}, dev={dev}: {denied}"
                );
            }
        }
        let mut denied = ready.clone();
        denied["capabilities"]["write"]["state"] = json!("locked");
        assert!(matches!(write_gate(Some(&denied), dev), Gate::Locked(_)));
        for backend in ["sim", "ingen", ""] {
            denied = ready.clone();
            denied["daemon"]["backend"] = json!(backend);
            assert!(
                matches!(write_gate(Some(&denied), dev), Gate::Locked(_)),
                "{backend}"
            );
        }
    }
}

#[test]
fn standard_nvml_tilbyr_ingen_skriving_uten_hjelperstatus() {
    let h = hello("nvml");
    let ops: Vec<&str> = h["capabilities"]["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for op in proto::WRITE_OPS {
        assert!(!ops.contains(op), "{op} tilbys med nvml");
    }
    assert_eq!(h["capabilities"]["write"]["state"], "locked");
    // Funksjonene annonseres utilgjengelige inntil hjelperen har probet dem.
    let steps = rigg_core::txn::steps(Some(&h));
    assert_eq!(steps.len(), 6);
    assert!(steps.iter().all(|s| s.2.starts_with("Ikke tilgjengelig")));
    // Manglende status må heller ikke gi en tilgjengelig funksjon.
    assert!(
        rigg_core::txn::steps(None)
            .iter()
            .all(|s| s.2.starts_with("Ikke godkjent"))
    );
    let mut mixed = h;
    mixed["capabilities"]["write"]["steps"] = json!([
        {"id": "power", "status": "supported"},
        {"id": "rgb", "status": "unavailable"},
    ]);
    let steps = rigg_core::txn::steps(Some(&mixed));
    assert_eq!(
        steps.iter().find(|s| s.0 == "power").unwrap().2,
        "Tilgjengelig"
    );
    assert!(
        steps
            .iter()
            .find(|s| s.0 == "rgb")
            .unwrap()
            .2
            .starts_with("Ikke tilgjengelig")
    );
}

#[test]
fn boka_svarer_write_locked_for_alt_unntatt_sim() {
    for backend in ["nvml", "ingen", ""] {
        let mut b = Book::new();
        for op in proto::WRITE_OPS {
            let e = b
                .handle(
                    backend,
                    op,
                    &json!({"settings": {"powerLimitW": 150}, "watts": 150, "txn": "t1"}),
                    "s1",
                    0,
                )
                .unwrap_err();
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

/// Finn hele funksjonskroppen, også etter nestede blokker og tekst med klammer.
fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("mangler {signature}"));
    let open = start + source[start..].find('{').unwrap();
    let (mut depth, mut quoted, mut escaped) = (0, false, false);
    for (i, ch) in source[open..].char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("uavsluttet {signature}")
}

#[test]
fn ekte_ui_port_krever_tilkobling_og_valgt_pci_og_uuid() {
    let source = code(&src_dir().join("widgets/locked.rs"));
    let gate = function_body(&source, "pub fn gate(").replace(char::is_whitespace, "");
    for guard in [
        "txn::write_gate(",
        "ifg!=Gate::Real{returng;}",
        "!app.client.is_connected()",
        "app.gpu_info().is_none_or(|i|i.state!=rigg_core::model::DeviceState::Ok)",
        "app.gpu_id().as_deref()!=h[\"identity\"][\"pci\"].as_str()",
        "st.uuid.as_deref().is_none()",
        "st.uuid.as_deref()!=h[\"identity\"][\"uuid\"].as_str()",
    ] {
        assert!(gate.contains(guard), "UI-porten mangler {guard}");
    }
    assert_eq!(gate.matches("returnGate::Locked(").count(), 2);
    assert!(gate.ends_with("Gate::Real"));
}

/// Vanlige skriver går gjennom porten. Det eneste direkte unntaket er
/// tilbakerulling av en allerede sendt transaksjon mot dens opprinnelige kort.
#[test]
fn skriveoperasjonene_gaar_gjennom_porten() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(files.len() > 30);
    let locked_path = src_dir().join("widgets/locked.rs");
    let mut hits = Vec::new();
    for f in &files {
        let c = code(f).replace(char::is_whitespace, "");
        for op in proto::WRITE_OPS {
            if c.contains(&format!("request(\"{op}\"")) {
                if *op == "revert" && f == &locked_path {
                    assert_eq!(c.matches("request(\"revert\"").count(), 1);
                    continue;
                }
                hits.push(format!("{}: request(\"{op}\")", f.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "skriveoperasjoner utenom porten:\n{}",
        hits.join("\n")
    );
    // Porten selv: sjekker operasjonen og at porten er åpen før noe sendes.
    let locked = code(&locked_path);
    let body = function_body(&locked, "pub fn send_write(").replace(char::is_whitespace, "");
    let before_request = &body[..body
        .find("app.client.request(op,args,")
        .expect("porten sender operasjonen")];
    assert!(before_request.contains("letg=gate(app);"));
    assert!(before_request.contains("if!rigg_core::proto::WRITE_OPS.contains(&op)||matches!(g,Gate::Locked(_)){cb(Err(ProtoError::new(ErrorCode::WriteLocked)));return;}"));
    assert!(before_request.contains("ifg==Gate::Real{"));
    assert!(before_request.contains("args[\"gpu\"]=json!(app.gpu_id());"));
    assert!(before_request.contains("args[\"uuid\"]=json!(app.statics().and_then(|s|s.uuid));"));
    // RGB skal ikke bli aktivert av at effekt-/klokkestyring er klar.
    assert!(before_request.contains("ifmatches!(op,\"rgb_get\"|\"rgb_set\")&&!supports(app,\"rgb\"){cb(Err(ProtoError::msg(ErrorCode::NotSupported,capability_reason(app,\"rgb\"))));return;}"));
    let changed_card = body.find("iforigin.key!=control_key(&a){").unwrap();
    let rollback = body
        .find("a.client.request(\"revert\",origin.args(json!({\"txn\":id}))")
        .unwrap();
    assert!(changed_card < rollback);
    assert!(body[changed_card..rollback].contains("v[\"pending\"].is_object()"));
    let origin = function_body(&locked, "fn args(&self,").replace(char::is_whitespace, "");
    assert!(origin.contains("ifself.real{"));
    assert!(origin.contains("args[\"gpu\"]=json!(self.gpu);"));
    assert!(origin.contains("args[\"uuid\"]=json!(self.uuid);"));
}

/// Bare sidene under Styring bruker `send_write`, og de setter Bruk aktiv
/// bare når porten er `Gate::Sim` eller `Gate::Real`, med gyldig utkast og
/// tilgjengelige felt. En låst port kan aldri aktivere Bruk.
#[test]
fn bruk_er_bare_aktiv_med_aapen_port() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let c = code(f);
        if c.contains("send_write(") {
            assert!(
                ["kontroller.rs", "viftekurve.rs", "locked.rs"].contains(&name.as_str()),
                "{name} bruker send_write"
            );
        }
        if ["kontroller.rs", "viftekurve.rs"].contains(&name.as_str()) {
            let compact = c.replace(char::is_whitespace, "");
            let mut n = 0;
            for (i, _) in compact.match_indices("self.apply.set_sensitive(") {
                let before = &compact[..i];
                let arm = before.rfind("Gate::Sim|Gate::Real=>{").unwrap_or(0);
                let locked = before.rfind("Gate::Locked").unwrap_or(0);
                let after = &compact[i + "self.apply.set_sensitive(".len()..];
                if !after.starts_with("false") {
                    assert!(
                        arm > locked && arm > 0,
                        "{name}: Bruk kan bli aktiv utenfor åpen port"
                    );
                    let checks = &before[arm..];
                    assert!(
                        checks.contains("locked::control_values(&self.app).is_some()"),
                        "{name}: mangler faktisk hjelpertilstand"
                    );
                    assert!(
                        checks.contains("locked::validated_draft(") && checks.contains(".is_ok()"),
                        "{name}: mangler utkastvalidering"
                    );
                    assert!(
                        checks.contains("locked::patch_error(") && checks.contains(".is_none()"),
                        "{name}: mangler funksjonssjekk"
                    );
                }
                n += 1;
            }
            assert!(compact.contains("Gate::Locked(t)=>{self.apply.set_sensitive(false);"));
            assert!(n >= 2, "{name}: fant ikke Bruk-knappen ({n})");
        }
    }
}
