//! `ui.json` (eies av vinduet). Leses synkront før første bilde, så tema,
//! aksent og tetthet aldri blinker (som demoens `theme-boot.js`). Skrives
//! atomisk 500 ms etter siste endring, så lukking aldri venter på disk.
//! En ødelagt fil kopieres til `ui.json.ødelagt-<tid>`.

use crate::state::App;
use rigg_core::settings::{Settings, lenient};
use rigg_core::trf;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub fn path(app: &App) -> PathBuf {
    rigg_core::paths::config_dir(app.profile).join("ui.json")
}

/// Leser innstillingene. Gir en melding når fila ikke kunne leses.
pub fn load(profile: rigg_core::paths::Profile) -> (Settings, Option<String>) {
    let p = rigg_core::paths::config_dir(profile).join("ui.json");
    let Ok(text) = std::fs::read_to_string(&p) else { return (Settings::default(), None) };
    if text.trim().is_empty() {
        return (Settings::default(), None);
    }
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) if v.is_object() => (lenient(&v), None),
        _ => {
            let stamp = rigg_core::now_ms();
            let mut bad = p.as_os_str().to_owned();
            bad.push(format!(".ødelagt-{stamp}"));
            let _ = std::fs::copy(&p, &bad);
            (
                Settings::default(),
                Some(trf!(
                    "ui.json kunne ikke leses. Kopien heter {fil}. Standardverdiene brukes.",
                    fil = PathBuf::from(bad).file_name().unwrap_or_default().to_string_lossy(),
                )),
            )
        }
    }
}

pub fn save_now(app: &App) {
    if let Err(e) = try_save_now(app) {
        eprintln!("Kunne ikke lagre ui.json: {e}");
    }
}

pub fn try_save_now(app: &App) -> std::io::Result<()> {
    if let Some(t) = app.save_timer.borrow_mut().take() {
        t.remove();
    }
    let text = serde_json::to_string_pretty(&app.settings().to_value())?;
    if *app.saved_text.borrow() == text {
        return Ok(());
    }
    rigg_core::write_atomic(&path(app), text.as_bytes())?;
    *app.saved_text.borrow_mut() = text;
    Ok(())
}

/// Lagrer 500 ms etter siste endring.
pub fn schedule_save(app: &Rc<App>) {
    if let Some(t) = app.save_timer.borrow_mut().take() {
        t.remove();
    }
    let a = app.clone();
    let id = gtk::glib::timeout_add_local_once(Duration::from_millis(500), move || {
        a.save_timer.borrow_mut().take();
        save_now(&a);
    });
    *app.save_timer.borrow_mut() = Some(id);
}
