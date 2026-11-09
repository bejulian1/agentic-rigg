//! Operativsystemet: distribusjon, kjerne, arkitektur, vertsnavn, økttype og
//! skrivebord (port av `readOs` i `server/system.js`). Diskene står i
//! `disk.rs` (1.1.0); her er bare den enkle lista til rapporten.

use serde_json::{Value, json};
use std::path::Path;

fn txt(p: &Path) -> Option<String> {
    crate::read_text(p).filter(|s| !s.is_empty())
}

/// `KEY="verdi"`-linjene i `/etc/os-release`.
pub fn os_release(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').trim_matches('\'').to_string()).filter(|v| !v.is_empty())
    })
}

/// Maskinarkitekturen (`uname -m`).
fn machine() -> Option<String> {
    // SAFETY: uname fyller bare strukturen vi eier.
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return None;
    }
    // SAFETY: feltet er nullavsluttet av kjernen.
    let s = unsafe { std::ffi::CStr::from_ptr(u.machine.as_ptr()) };
    Some(s.to_string_lossy().into_owned())
}

/// Økttypen ut fra miljøet: (tekst, variabelen den kom fra).
pub fn session_from_env(get: impl Fn(&str) -> Option<String>) -> (Option<String>, Option<&'static str>) {
    if let Some(t) = get("XDG_SESSION_TYPE").filter(|s| !s.is_empty()) {
        let nice = match t.as_str() {
            "wayland" => "Wayland".to_string(),
            "x11" => "X11".to_string(),
            "tty" => "Tekstkonsoll".to_string(),
            other => other.to_string(),
        };
        return (Some(nice), Some("XDG_SESSION_TYPE"));
    }
    if get("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty()) {
        return (Some("Wayland".into()), Some("WAYLAND_DISPLAY"));
    }
    if get("DISPLAY").is_some_and(|s| !s.is_empty()) {
        return (Some("X11".into()), Some("DISPLAY"));
    }
    (None, None)
}

pub fn read(root: &Path) -> Value {
    let rel = std::fs::read_to_string(root.join("etc/os-release")).or_else(|_| std::fs::read_to_string(root.join("usr/lib/os-release"))).unwrap_or_default();
    let env = |k: &str| std::env::var(k).ok();
    let (session, source) = session_from_env(env);
    json!({
        "distro": os_release(&rel, "PRETTY_NAME").or_else(|| os_release(&rel, "NAME")).unwrap_or_else(|| "Linux".into()),
        "distroId": os_release(&rel, "ID"),
        "version": os_release(&rel, "VERSION"),
        "codename": os_release(&rel, "VERSION_CODENAME"),
        "kernel": txt(&root.join("proc/sys/kernel/osrelease")),
        "kernelBuild": txt(&root.join("proc/sys/kernel/version")),
        "arch": machine(),
        "hostname": txt(&root.join("proc/sys/kernel/hostname")),
        "session": session,
        "sessionSource": source,
        "desktop": env("XDG_CURRENT_DESKTOP"),
        "disks": super::disk::basic_json(root),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_og_økttype() {
        let t = "PRETTY_NAME=\"Ubuntu 26.04.1 LTS\"\nNAME=\"Ubuntu\"\nVERSION_CODENAME=resolute\n";
        assert_eq!(os_release(t, "PRETTY_NAME").as_deref(), Some("Ubuntu 26.04.1 LTS"));
        assert_eq!(os_release(t, "VERSION_CODENAME").as_deref(), Some("resolute"));
        assert_eq!(os_release(t, "ID"), None);
        let env = |k: &str| (k == "WAYLAND_DISPLAY").then(|| "wayland-0".to_string());
        assert_eq!(session_from_env(env), (Some("Wayland".into()), Some("WAYLAND_DISPLAY")));
        let env = |k: &str| (k == "XDG_SESSION_TYPE").then(|| "x11".to_string());
        assert_eq!(session_from_env(env).0.as_deref(), Some("X11"));
    }

    #[test]
    fn denne_maskinen() {
        let v = read(Path::new("/"));
        assert!(v["kernel"].as_str().is_some_and(|k| !k.is_empty()));
    }
}
