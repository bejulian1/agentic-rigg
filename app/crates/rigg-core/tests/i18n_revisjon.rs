//! Språkrevisjonen (`app/scripts/i18n-revisjon.py --check`) skal være uten
//! funn: ingen synlig tekst utenfor `tr!`, samme sidenavn i sidefeltet og
//! Innstillinger, og hvert språk i nedtrekkslista fullstendig oversatt uten
//! tegn fra et annet språk. Revisjonen endrer ingen filer.
#![allow(clippy::disallowed_methods)] // testen kjører revisjonsskriptet i kontrollmodus

use std::path::Path;

#[test]
fn språkrevisjonen_har_ingen_funn() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/i18n-revisjon.py");
    let out = std::process::Command::new("python3").arg("-I").arg(&script).arg("--check").output().expect("python3");
    assert!(out.status.success(), "i18n-revisjon.py --check:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
