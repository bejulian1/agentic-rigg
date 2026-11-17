//! Agentic Rigg · felles kjerne uten GTK og uten NVML.
//!
//! Brukes av tjenesten (`riggd`) og vinduet (`rigg`). Alt her er ren Rust og
//! kan testes på under ett sekund (`cargo test -p rigg-core`).

pub mod cpu_specs;
pub mod csv;
pub mod disk;
pub mod downsample;
pub mod easter;
pub mod fmt;
pub mod hjelper;
pub mod history;
pub mod model;
pub mod paths;
pub mod process;
pub mod proto;
pub mod report;
pub mod sensor_key;
pub mod sensormeta;
pub mod settings;
pub mod specs;
pub mod stats;
pub mod tokens;
pub mod txn;
pub mod vendor;
pub mod vifte;

/// Navn, versjon og bygg fra `app/version.json` (lest av build.rs).
pub mod version {
    pub const NAME: &str = env!("RIGG_NAME");
    pub const VERSION: &str = env!("RIGG_VERSION");
    pub const CHANNEL: &str = env!("RIGG_CHANNEL");
    /// Byggnummeret som tekst; [`build`] gir det som tall.
    pub const BUILD_STR: &str = env!("RIGG_BUILD");

    pub fn build() -> u32 {
        BUILD_STR.parse().unwrap_or(0)
    }

    /// «Agentic Rigg 1.2.0 · bygg 18», som `--version` og Om skriver.
    pub fn long() -> String {
        format!("{NAME} {VERSION} · bygg {BUILD_STR}")
    }

    /// «Versjon 1.2.0 · bygg 18», som Innstillinger › Om viser.
    pub fn about() -> String {
        format!("Versjon {VERSION} · bygg {BUILD_STR}")
    }
}

/// Millisekunder siden 1970 (veggklokke).
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Skriver en fil atomisk: midlertidig fil i samme mappe, `fsync` og `rename`.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("fil");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn versjonen_er_lik_i_cargo_og_version_json() {
        assert_eq!(env!("CARGO_PKG_VERSION"), super::version::VERSION);
        assert_eq!(super::version::NAME, "Agentic Rigg");
        assert!(super::version::build() >= 16);
        assert_eq!(super::version::long(), format!("Agentic Rigg {} · bygg {}", super::version::VERSION, super::version::BUILD_STR));
    }
}
