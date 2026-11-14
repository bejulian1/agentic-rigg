//! Kort, innebygd desktophistorikk. Kilden oppdateres ved hver utgivelse;
//! løpende testtall og utviklingslogger hører ikke hjemme i denne dialogen.

use super::{label, vbox};
use crate::state::App;
use gtk::prelude::*;
use serde_json::Value;
use std::rc::Rc;

const RESOURCE: &str = "/io/github/AgenticRigg/version-log.json";

struct Entry {
    version: String,
    build: u32,
    date: String,
    title: String,
    changes: Vec<String>,
}

fn entries(text: &str) -> Result<Vec<Entry>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let rows = value.as_array().ok_or("Versjonsloggen må være en liste")?;
    rows.iter()
        .map(|row| {
            let text = |key: &str| row[key].as_str().filter(|s| !s.trim().is_empty()).map(str::to_string).ok_or_else(|| format!("Mangler {key}"));
            let changes = row["changes"]
                .as_array()
                .ok_or("Mangler endringsliste")?
                .iter()
                .map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(str::to_string).ok_or_else(|| "Tom endringsbeskrivelse".to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            if changes.is_empty() {
                return Err("Tom endringsliste".into());
            }
            Ok(Entry { version: text("version")?, build: row["build"].as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("Ugyldig byggnummer")?, date: text("date")?, title: text("title")?, changes })
        })
        .collect()
}

fn date_label(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3
        && let (Ok(month), Ok(day)) = (parts[1].parse::<usize>(), parts[2].parse::<u32>())
        && (1..=12).contains(&month)
    {
        return format!("{day}. {} {}", rigg_core::fmt::MONTHS[month - 1], parts[0]);
    }
    date.to_string()
}

fn wrap(text: &str, classes: &[&str]) -> gtk::Label {
    let text = label(text, classes);
    text.set_wrap(true);
    text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    text.set_max_width_chars(64);
    text.set_hexpand(true);
    text
}

pub fn show(app: &Rc<App>) {
    let Some((window, root)) = crate::shortcuts::dialog(app, "Versjonslogg", 640, 720) else { return };
    let body = vbox(12, &["lic-body"]);
    body.append(&wrap("Desktoputgavene, fra nyeste til eldste. Åpne en versjon for å se endringene.", &["muted"]));
    let data = gtk::gio::resources_lookup_data(RESOURCE, gtk::gio::ResourceLookupFlags::NONE).map_err(|e| e.to_string()).and_then(|bytes| std::str::from_utf8(&bytes).map_err(|e| e.to_string()).and_then(entries));
    match data {
        Ok(entries) => {
            for (index, entry) in entries.into_iter().enumerate() {
                let expander = gtk::Expander::new(None);
                expander.set_hexpand(true);
                expander.set_expanded(index == 0);
                let heading = vbox(2, &[]);
                heading.append(&wrap(&format!("{} · bygg {} · {}", entry.version, entry.build, date_label(&entry.date)), &["group-title"]));
                heading.append(&wrap(&entry.title, &["muted"]));
                expander.set_label_widget(Some(&heading));
                let changes = vbox(8, &[]);
                changes.set_margin_start(20);
                changes.set_margin_top(10);
                changes.set_margin_bottom(8);
                for change in &entry.changes {
                    changes.append(&wrap(&format!("• {change}"), &[]));
                }
                expander.set_child(Some(&changes));
                body.append(&expander);
            }
        }
        Err(_) => body.append(&wrap("Versjonsloggen kunne ikke leses fra appen.", &[])),
    }
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));
    root.append(&scroll);
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn innebygd_logg_folger_appversjonen_og_er_sortert() {
        let log = entries(include_str!("../../../../data/version-log.json")).unwrap();
        let current: Value = serde_json::from_str(include_str!("../../../../version.json")).unwrap();
        assert_eq!(log[0].version, current["version"].as_str().unwrap());
        assert_eq!(u64::from(log[0].build), current["build"].as_u64().unwrap());
        let semver = |v: &str| v.split('.').map(|n| n.parse::<u32>().unwrap()).collect::<Vec<_>>();
        assert!(log.windows(2).all(|pair| semver(&pair[0].version) > semver(&pair[1].version) && pair[0].build > pair[1].build && pair[0].date >= pair[1].date));
        assert_eq!(log.last().unwrap().version, "1.0.0");
        assert_eq!(log.last().unwrap().build, 16);
        assert!(log.len() >= 9, "Behold de ni dokumenterte desktoputgavene ved senere utvidelser");
        assert_eq!(log[0].version, rigg_core::version::VERSION);
        assert_eq!(log[0].build, rigg_core::version::build());
        assert_eq!(date_label("2026-11-14"), "14. november 2026");
        let resource = include_str!("../../../../data/rigg.gresource.xml");
        assert!(resource.contains("<file>version-log.json</file>"));
    }
}
