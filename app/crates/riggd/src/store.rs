//! Filene tjenesten eier (arkitektur.md kap. 7):
//! - `~/.config/agentic-rigg/tjeneste.json` (innstillingene)
//! - `~/.local/state/agentic-rigg/historikk.bin` (hvert 60. s og ved stopp)
//! - `~/.local/state/agentic-rigg/minmaks.json` (høyst hvert 30. s, når «Husk min og maks» er på)
//! - `~/.local/state/agentic-rigg/statisk-<uuid>-<driver>.json` (faste data, så
//!   `get_static` kan svare straks ved neste oppstart)
//!
//! Alt skrives atomisk (temp-fil og `rename`).

use rigg_core::model::GpuStatic;
use rigg_core::settings::{ServiceSettings, lenient_service};
use std::path::{Path, PathBuf};

pub struct Store {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Store {
    pub fn new(config_dir: PathBuf, state_dir: PathBuf) -> Store {
        Store { config_dir, state_dir }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.config_dir.join("tjeneste.json")
    }

    pub fn history_path(&self) -> PathBuf {
        self.state_dir.join("historikk.bin")
    }

    pub fn minmax_path(&self) -> PathBuf {
        self.state_dir.join("minmaks.json")
    }

    /// Leser `tjeneste.json` felt for felt. En ødelagt fil kopieres til
    /// `tjeneste.json.ødelagt-<tid>`, og standardverdiene brukes.
    pub fn load_settings(&self, now: i64) -> (ServiceSettings, Option<String>) {
        let p = self.settings_path();
        let Ok(text) = std::fs::read_to_string(&p) else { return (ServiceSettings::default(), None) };
        if text.trim().is_empty() {
            return (ServiceSettings::default(), None);
        }
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) if v.is_object() => (lenient_service(&v), None),
            _ => {
                let mut bad = p.as_os_str().to_owned();
                bad.push(format!(".ødelagt-{now}"));
                let _ = std::fs::copy(&p, &bad);
                (ServiceSettings::default(), Some(format!("tjeneste.json kunne ikke leses. Kopi: {}", Path::new(&bad).display())))
            }
        }
    }

    pub fn save_settings(&self, s: &ServiceSettings) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(&s.to_value()).unwrap_or_default();
        rigg_core::write_atomic(&self.settings_path(), text.as_bytes())
    }

    fn static_path(&self, st: &GpuStatic) -> Option<PathBuf> {
        let uuid = st.uuid.as_deref()?;
        let drv = st.driver.as_deref()?;
        let safe = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.').collect::<String>();
        Some(self.state_dir.join(format!("statisk-{}-{}.json", safe(uuid), safe(drv))))
    }

    pub fn save_static(&self, st: &GpuStatic) {
        if let Some(p) = self.static_path(st)
            && let Ok(text) = serde_json::to_string(st)
        {
            let _ = rigg_core::write_atomic(&p, text.as_bytes());
        }
    }

    /// Faste data fra forrige kjøring for et kort med samme ID og driver.
    pub fn cached_statics(&self) -> Vec<GpuStatic> {
        let Ok(rd) = std::fs::read_dir(&self.state_dir) else { return vec![] };
        rd.flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("statisk-"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| serde_json::from_str::<GpuStatic>(&t).ok())
            .collect()
    }
}

/// Lager mappa med 0700 og sjekker at den er vår og ikke en symlenke.
pub fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    std::fs::create_dir_all(dir).map_err(|e| format!("Kunne ikke lage {}: {e}", dir.display()))?;
    let m = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if m.file_type().is_symlink() || !m.is_dir() {
        return Err(format!("{} er ikke en mappe", dir.display()));
    }
    // SAFETY: getuid kan ikke feile.
    let uid = unsafe { libc::getuid() };
    if m.uid() != uid {
        return Err(format!("{} eies av en annen bruker", dir.display()));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
