//! Om: ikon, navn, «Versjon 1.1.0 · bygg 17» og én setning (ingen
//! alpha-merke), hjelp, og feilsøking: tjenesten (versjon, bygg, PID,
//! oppetid, minne), driver, NVML, CUDA, økttype, GTK og tegner, socket og
//! mappene. Sju klikk på versjonen gir påskeegg 3.
//!
//! Nederst i Om-kortet står signaturen «Code Pro», navnet på byggeserien,
//! som i Klipp og Agentic Desktop (runde 6B): sentrert under versjonen, med
//! en tynn linje over.

use super::{Group, GroupOpts, custom_row, group, live, row};
use crate::state::App;
use crate::widgets::rows::KvList;
use crate::widgets::{hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::fmt as F;
use std::rc::Rc;

/// Navnet på byggeserien appene hører til, likt i Klipp og Agentic Desktop.
pub const SIGNATURE: &str = "Code Pro";
/// Teksten under signaturen (samme ordlyd som i Klipp).
pub const SIGNATURE_NOTE: &str = "Navnet på byggeserien denne versjonen hører til";

/// Signaturen: navnet og en liten linje under, sentrert.
fn signature() -> gtk::Box {
    let b = vbox(2, &["about-signature"]);
    b.set_halign(gtk::Align::Fill);
    let name = label(SIGNATURE, &["about-signature-name"]);
    name.set_xalign(0.5);
    name.set_selectable(false);
    b.append(&name);
    let note = label(SIGNATURE_NOTE, &["about-signature-note"]);
    note.set_xalign(0.5);
    note.set_wrap(true);
    note.set_justify(gtk::Justification::Center);
    b.append(&note);
    b.update_property(&[gtk::accessible::Property::Label(&format!("{SIGNATURE}. {SIGNATURE_NOTE}."))]);
    b
}

fn service_line(app: &App) -> String {
    if !app.client.is_connected() {
        return "Svarer ikke".into();
    }
    let d = app.data.borrow();
    let h = d.hello.as_ref();
    let name = h.and_then(|h| h["daemon"]["name"].as_str()).unwrap_or("agentic-riggd");
    let ver = h.and_then(|h| h["daemon"]["version"].as_str()).unwrap_or("");
    let build = h.and_then(|h| h["daemon"]["build"].as_u64()).map(|b| format!(" · bygg {b}")).unwrap_or_default();
    let mut s = format!("{name} {ver}{build}");
    if let Some(p) = d.ping.as_ref() {
        if let Some(pid) = p["pid"].as_u64() {
            s.push_str(&format!(" · PID {pid}"));
        }
        if let Some(t) = p["startedAt"].as_i64() {
            s.push_str(&format!(" · oppe {}", F::uptime(Some(((rigg_core::now_ms() - t) / 1000) as f64))));
        }
        if let Some(kb) = p["privateKB"].as_f64() {
            s.push_str(&format!(" · {} MB privat minne", F::num(Some(kb / 1024.0), 0)));
        }
    }
    s
}

fn source_line(app: &App) -> String {
    let d = app.data.borrow();
    let backend = d.backend.clone();
    let st = d.gpu.as_ref().and_then(|g| d.statics.get(g)).map(|s| s.0.clone());
    match (backend.as_str(), st) {
        ("nvml", Some(s)) => format!("NVML {} · driver {}", s.nvml_version.unwrap_or_default(), s.driver.unwrap_or_default()),
        ("nvml", None) => "NVML".into(),
        ("sim", _) => "Testdata (agentic-riggd --backend sim)".into(),
        ("", _) => "–".into(),
        (b, _) => b.into(),
    }
}

fn renderer(app: &App) -> String {
    let r = app.window().and_then(|w| w.renderer()).map(|r| r.type_().name().to_string()).unwrap_or_default();
    let kind = match r.as_str() {
        "GskNglRenderer" | "GskGLRenderer" => "GL",
        "GskVulkanRenderer" => "Vulkan",
        "GskCairoRenderer" => "programvare (Cairo)",
        "" => "–",
        other => other,
    };
    format!("GTK {}.{}.{} · tegner {kind}", gtk::major_version(), gtk::minor_version(), gtk::micro_version())
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    // ---- Om-kortet ----
    let card = hbox(16, &["about-card"]);
    let pic = gtk::Picture::for_resource(&format!("/io/github/AgenticRigg/icons/varianter/{}/48.svg", app.settings().app_icon));
    pic.set_can_shrink(false);
    pic.set_valign(gtk::Align::Start);
    card.append(&pic);
    let text = vbox(4, &["about-text"]);
    let name = label(rigg_core::version::NAME, &["about-name"]);
    name.set_accessible_role(gtk::AccessibleRole::Heading);
    text.append(&name);
    let ver = gtk::Button::new();
    ver.add_css_class("plain");
    ver.set_halign(gtk::Align::Start);
    let vtag = crate::widgets::rows::tag(&rigg_core::version::about(), None, "quiet");
    vtag.add_css_class("clickable");
    ver.set_child(Some(&vtag));
    ver.set_tooltip_text(Some(&rigg_core::version::long()));
    text.append(&ver);
    let whisper = label("", &["egg-whisper"]);
    whisper.set_visible(false);
    text.append(&whisper);
    let line = label("Overvåking av skjermkort og maskin for Linux. Appen leser bare, og ingenting sendes til skjermkortet.", &["about-line"]);
    line.set_wrap(true);
    text.append(&line);
    card.append(&text);
    let hero = group(rigg_core::version::NAME, GroupOpts { whole: true, k: "versjon bygg om code pro signatur byggeserie", ..Default::default() }, vec![]);
    hero.head.set_visible(false);
    hero.widget.append(&card);
    hero.widget.append(&signature());

    // ---- Hjelp ----
    let keys = crate::widgets::button(Some("circle-help"), Some("Tastatursnarveier …"), &[]);
    {
        let a = app.clone();
        keys.connect_clicked(move |_| crate::shortcuts::show_dialog(&a));
    }
    let lic = crate::widgets::button(None, Some("Lisenser …"), &[]);
    {
        let a = app.clone();
        lic.connect_clicked(move |_| show_licenses(&a));
    }
    let mut help_rows = vec![
        row("Tastatursnarveier", Some("Alle hurtigtastene samlet. Også Ctrl+? og ?-knappen i sidefeltet."), &keys, "taster hurtigtast snarvei"),
        row("Lisenser", Some("Ikonene (Lucide, ISC) og Rust-bibliotekene (MIT). Også i ~/.local/share/doc/agentic-rigg/LISENSER.md."), &lic, "lisens isc mit lucide opphavsrett"),
    ];
    if let Some(r) = crate::rapport_ui::about_row(app) {
        help_rows.push(r);
    }
    let help = group("Hjelp", GroupOpts::default(), help_rows);

    // ---- Feilsøking ----
    let kv = KvList::new();
    kv.grid.add_css_class("about-debug");
    kv.add("Versjon", &rigg_core::version::about().replacen("Versjon ", "", 1));
    let svc = kv.add("Tjenesten", &service_line(app));
    live(&svc, service_line);
    let src = kv.add("Kilde", &source_line(app));
    live(&src, source_line);
    let st = app.statics().unwrap_or_default();
    kv.add("CUDA", st.cuda_version.as_deref().unwrap_or("–"));
    let session = format!(
        "{} · {}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "–".into()),
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "–".into())
    );
    kv.add("Økt", &session);
    let gtkline = kv.add("Vinduet", &renderer(app));
    live(&gtkline, renderer);
    let sock = kv.add("Socket", &rigg_core::paths::tilde(app.client.socket()));
    sock.add_css_class("mono");
    let dirs = kv.add("Mapper", &format!("{}\n{}", rigg_core::paths::tilde(&rigg_core::paths::config_dir(app.profile)), rigg_core::paths::tilde(&rigg_core::paths::state_dir(app.profile))));
    dirs.add_css_class("mono");
    if app.profile.is_dev() {
        kv.add("Profil", "Utvikling (RIGG_PROFIL=dev)");
    }
    let eggs_box = vbox(0, &[]);
    if crate::easter::enabled(app) && app.settings().eggs_found.iter().any(|e| e == "utvikler") {
        let l = kv.add("Små overraskelser", &crate::easter::found_text(app));
        live(&l, crate::easter::found_text);
    }
    {
        let (a, kv2) = (app.clone(), kv.clone());
        crate::easter::about_version(app, &ver, &whisper, move || {
            let l = kv2.add("Små overraskelser", &crate::easter::found_text(&a));
            live(&l, crate::easter::found_text);
        });
    }
    let debug_box = vbox(0, &[]);
    debug_box.append(&kv.grid);
    debug_box.append(&eggs_box);
    let mut dbg = group(
        "Feilsøking",
        GroupOpts { whole: true, k: "debug versjon tjeneste pid", ..Default::default() },
        vec![],
    );
    dbg.widget.insert_child_after(&debug_box, dbg.widget.first_child().as_ref());
    dbg.rows.push(custom_row(&gtk::Box::new(gtk::Orientation::Horizontal, 0), &format!("{} {}", service_line(app), source_line(app))));
    vec![hero, help, dbg]
}

/// Lisensene (`data/LISENSER.md`, kompilert inn) i et eget vindu: avsnitt
/// som tekst, kodeblokker og tabeller med fast bredde.
pub fn show_licenses(app: &Rc<App>) {
    let Some((w, root)) = crate::shortcuts::dialog(app, "Lisenser", 640, 720) else { return };
    let text = gtk::gio::resources_lookup_data("/io/github/AgenticRigg/LISENSER.md", gtk::gio::ResourceLookupFlags::NONE).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let body = vbox(0, &["lic-body"]);
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 1 {
            let l = label(part.trim_matches('\n'), &["mono", "lic-code"]);
            l.set_selectable(true);
            l.set_focusable(false);
            l.set_wrap(true);
            l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            body.append(&l);
            continue;
        }
        let mut table = String::new();
        for para in part.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            if para.starts_with('|') {
                for line in para.lines().filter(|l| !l.starts_with("|---")) {
                    let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
                    table.push_str(&format!("{:<22} {:<10} {}\n", cells.first().unwrap_or(&""), cells.get(1).unwrap_or(&""), cells.get(2).unwrap_or(&"")));
                }
                let l = label(table.trim_end(), &["mono", "lic-table"]);
                l.set_selectable(true);
                l.set_focusable(false);
                body.append(&l);
                table.clear();
                continue;
            }
            let (cls, t) = match para.strip_prefix("# ").or_else(|| para.strip_prefix("## ")) {
                Some(h) => ("lic-h", h.to_string()),
                None => ("", para.replace('`', "").replace(['[', ']'], "").replace("(https://lucide.dev)", "")),
            };
            let classes = [cls];
            let l = label(&t, if cls.is_empty() { &[] } else { &classes });
            l.set_wrap(true);
            l.set_selectable(true);
            l.set_focusable(false);
            if !cls.is_empty() {
                l.set_accessible_role(gtk::AccessibleRole::Heading);
            }
            body.append(&l);
        }
    }
    let sc = gtk::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk::PolicyType::Never);
    sc.set_vexpand(true);
    sc.set_child(Some(&body));
    root.append(&sc);
    w.present();
    // Fokus på lukkeknappen, ikke på første valgbare avsnitt (det ble markert).
    crate::shortcuts::focus_close(&root);
}
