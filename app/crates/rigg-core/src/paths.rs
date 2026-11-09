//! Profiler og stier (arkitektur.md 4.8 og kap. 7).
//!
//! Produksjon: app-ID `io.github.AgenticRigg`, socket
//! `$XDG_RUNTIME_DIR/agentic-rigg/riggd.sock`, `~/.config/agentic-rigg/` og
//! `~/.local/state/agentic-rigg/`.
//!
//! Utvikling (`RIGG_PROFIL=dev`): app-ID `io.github.AgenticRigg.Devel`,
//! `riggd-dev.sock`, `~/.config/agentic-rigg-dev/` og
//! `~/.local/state/agentic-rigg-dev/`, så en dev-forekomst aldri kobler seg til,
//! stopper eller overskriver en installert forekomst. Testene setter
//! `RIGG_SOCKET`, `RIGG_CONFIG_DIR` og `RIGG_STATE_DIR` til midlertidige mapper.
//!
//! Debug-bygg er ikke automatisk dev: profilen bestemmes bare av `RIGG_PROFIL`,
//! så et debug-bygg aldri overrasker ved å bytte mappe.

use std::env;
use std::path::PathBuf;

pub const APP_ID: &str = "io.github.AgenticRigg";
pub const APP_ID_DEV: &str = "io.github.AgenticRigg.Devel";
pub const UNIT: &str = "agentic-riggd.service";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Prod,
    Dev,
}

impl Profile {
    pub fn current() -> Profile {
        match env::var("RIGG_PROFIL").as_deref() {
            Ok("dev") | Ok("utvikling") => Profile::Dev,
            _ => Profile::Prod,
        }
    }

    pub fn is_dev(self) -> bool {
        self == Profile::Dev
    }

    pub fn app_id(self) -> &'static str {
        match self {
            Profile::Prod => APP_ID,
            Profile::Dev => APP_ID_DEV,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Profile::Prod => "",
            Profile::Dev => "-dev",
        }
    }
}

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if !v.is_empty() && PathBuf::from(&v).is_absolute() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// Mappa for socketen: `$XDG_RUNTIME_DIR/agentic-rigg/` (0700).
pub fn runtime_dir() -> PathBuf {
    let base = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/agentic-rigg-{}", unsafe_uid())));
    base.join("agentic-rigg")
}

fn unsafe_uid() -> u32 {
    // Bare til stinavnet når XDG_RUNTIME_DIR mangler; ingen libc i kjernen.
    std::fs::metadata("/proc/self").map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap_or(0)
}

/// Socketen tjenesten lytter på. `RIGG_SOCKET` overstyrer.
pub fn socket_path(p: Profile) -> PathBuf {
    if let Some(s) = env::var_os("RIGG_SOCKET").filter(|s| !s.is_empty()) {
        return PathBuf::from(s);
    }
    runtime_dir().join(format!("riggd{}.sock", p.suffix()))
}

/// `~/.config/agentic-rigg/` (ui.json, tjeneste.json, profiler.json). `RIGG_CONFIG_DIR` overstyrer.
pub fn config_dir(p: Profile) -> PathBuf {
    if let Some(s) = env::var_os("RIGG_CONFIG_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(s);
    }
    xdg("XDG_CONFIG_HOME", ".config").join(format!("agentic-rigg{}", p.suffix()))
}

/// `~/.local/state/agentic-rigg/` (historikk.bin, minmaks.json). `RIGG_STATE_DIR` overstyrer.
pub fn state_dir(p: Profile) -> PathBuf {
    if let Some(s) = env::var_os("RIGG_STATE_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(s);
    }
    xdg("XDG_STATE_HOME", ".local/state").join(format!("agentic-rigg{}", p.suffix()))
}

/// `~/.local/share/applications/<app-id>.desktop` (appikonet skrives om her).
pub fn desktop_file(p: Profile) -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("applications").join(format!("{}.desktop", p.app_id()))
}

/// `~/.local/share/icons/hicolor`.
pub fn hicolor_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("icons").join("hicolor")
}

/// `~` foran en sti erstatter hjemmemappa, til visning.
pub fn tilde(path: &std::path::Path) -> String {
    let h = home();
    match path.strip_prefix(&h) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
