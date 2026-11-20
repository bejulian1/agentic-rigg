// Leser app/version.json (eneste kilde for navn, versjon og bygg, arkitektur.md
// kap. 13) og gjør verdiene tilgjengelige som RIGG_NAME, RIGG_VERSION og
// RIGG_BUILD. Fila er liten og flat, så den tolkes for hånd (ingen
// byggeavhengigheter).
use std::{env, fs, path::Path};

fn field<'a>(text: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\"");
    let i = text.find(&pat).unwrap_or_else(|| panic!("version.json mangler {key}"));
    let rest = text[i + pat.len()..].trim_start().trim_start_matches(':').trim_start();
    if let Some(r) = rest.strip_prefix('"') {
        &r[..r.find('"').expect("uavsluttet tekst i version.json")]
    } else {
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        &rest[..end]
    }
}

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = Path::new(&dir).join("../../version.json");
    let text = fs::read_to_string(&path).expect("fant ikke app/version.json");
    println!("cargo:rustc-env=RIGG_NAME={}", field(&text, "name"));
    println!("cargo:rustc-env=RIGG_VERSION={}", field(&text, "version"));
    println!("cargo:rustc-env=RIGG_CHANNEL={}", field(&text, "channel"));
    println!("cargo:rustc-env=RIGG_BUILD={}", field(&text, "build"));
    println!("cargo:rerun-if-changed={}", path.display());
    println!("cargo:rerun-if-changed=../../data/theme/tokens.css");

    let app = Path::new(&dir).join("../..");
    let script = app.join("scripts/i18n-trekk-ut.py");
    let out = Path::new(&env::var_os("OUT_DIR").expect("OUT_DIR")).join("i18n.rs");
    println!("cargo:rerun-if-changed={}", script.display());
    println!("cargo:rerun-if-changed={}", app.join("data/i18n").display());
    #[allow(clippy::disallowed_methods)] // Byggverktøyet validerer PO, utenfor appen.
    let status = std::process::Command::new("python3").arg(script).arg("--build").arg(out)
        .status().expect("trenger Python 3 til PO-katalogene");
    assert!(status.success(), "kunne ikke bygge inn PO-katalogene");
}
