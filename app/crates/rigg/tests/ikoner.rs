//! Ikonvariantene er laget av malene (docs/produksjon/ikon.md kap. 6):
//! `lag-varianter.py --sjekk` endrer ingenting og feiler hvis en variant er
//! redigert for hånd eller ikke laget på nytt. Og alle filene GResource og
//! install.sh trenger, finnes.
#![allow(clippy::disallowed_methods)] // testen kjører generatoren i sjekkmodus

use std::path::Path;

fn icons() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/icons")
}

#[test]
fn variantene_er_oppdatert() {
    let script = icons().join("lag-varianter.py");
    let out = std::process::Command::new("python3").arg("-I").arg(&script).arg("--sjekk").output().expect("python3");
    assert!(out.status.success(), "lag-varianter.py --sjekk: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn alle_filene_finnes() {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(icons().join("varianter/varianter.json")).unwrap()).unwrap();
    let list = v["varianter"].as_array().unwrap();
    assert_eq!(list.len(), 10);
    for x in list {
        let id = x["id"].as_str().unwrap();
        for size in ["128", "48", "32", "24", "16"] {
            assert!(icons().join(format!("varianter/{id}/{size}.svg")).exists(), "{id}/{size}");
        }
        let gres = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/rigg.gresource.xml")).unwrap();
        assert!(gres.contains(&format!("icons/varianter/{id}/48.svg")), "GResource mangler {id}");
    }
    for size in ["scalable", "48x48", "32x32", "24x24", "16x16"] {
        assert!(icons().join(format!("hicolor/{size}/apps/io.github.AgenticRigg.svg")).exists());
    }
    assert!(icons().join("hicolor/symbolic/apps/io.github.AgenticRigg-symbolic.svg").exists());
}
