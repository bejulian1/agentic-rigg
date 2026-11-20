//! Logging og varsler: loggens sensorer, intervall, mappe og format, og
//! skrivebordsvarsler (produktspesifikasjonen 3.2 og 3.5).
//!
//! Valgene ligger i `tjeneste.json` (tjenesten kjører loggen og varslene,
//! også når vinduet er lukket) og endres med `set_settings`.

use super::{Group, GroupOpts, custom_row, group, kw, request_render, row, switch_row};
use crate::state::App;
use crate::widgets::{Seg, hbox, label, vbox};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::fmt as F;
use rigg_core::settings::ServiceSettings;
use rigg_core::{tr, trf};
use serde_json::json;
use std::rc::Rc;

thread_local! {
    static WATCH: std::cell::RefCell<Option<(glib::WeakRef<gtk::Label>, glib::WeakRef<gtk::Button>)>> = const { std::cell::RefCell::new(None) };
}

fn svc(app: &App) -> ServiceSettings {
    rigg_core::settings::lenient_service(&app.data.borrow().service)
}

fn log_dir(app: &App) -> std::path::PathBuf {
    svc(app).log.dir.map(std::path::PathBuf::from).unwrap_or_else(|| glib::user_special_dir(glib::UserDirectory::Documents).unwrap_or_else(|| glib::home_dir().join("Dokumenter")).join("Agentic Rigg"))
}

/// «Av» eller «Logger siden 14:32 · 1 234 rader · 2,1 MB».
fn status_text(app: &App) -> String {
    let st = crate::logg_ui::status(app);
    if !app.client.is_connected() {
        return tr!("Tjenesten svarer ikke").into();
    }
    if st["active"].as_bool() == Some(true) {
        let t = st["startedAt"].as_i64().map(|t| F::time(crate::app::local_time(t), false)).unwrap_or_default();
        return trf!("Logger siden {tid} · {rader}", tid = t, rader = crate::logg_ui::rows_text(&st));
    }
    match st["file"].as_str() {
        Some(f) => trf!("Av · siste: {fil}", fil = std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        None => tr!("Av").into(),
    }
}

/// Velg sensorer: avkrysning for alt i siste prøve, med søk.
fn pick_sensors(app: &Rc<App>, anchor: &gtk::Button) {
    let body = vbox(6, &["sensor-pick"]);
    body.append(&label(tr!("Sensorer i loggen"), &["pop-title"]));
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(tr!("Filtrer")));
    body.append(&search);
    let list = vbox(0, &[]);
    let sc = gtk::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk::PolicyType::Never);
    sc.set_min_content_height(320);
    sc.set_max_content_height(420);
    sc.set_propagate_natural_height(true);
    sc.set_child(Some(&list));
    body.append(&sc);
    let tele = app.telemetry();
    let flat = tele.as_deref().map(rigg_core::sensor_key::flatten).unwrap_or_default();
    let chosen = Rc::new(std::cell::RefCell::new(svc(app).log.keys));
    let mut rows: Vec<(String, gtk::CheckButton)> = Vec::new();
    for (k, _) in flat.iter().filter(|(k, _)| !k.contains("/thread/")) {
        let c = rigg_core::csv::column(k, tele.as_deref());
        let on = chosen.borrow().iter().any(|x| x == k);
        let (a, ch, key) = (app.clone(), chosen.clone(), k.clone());
        let cb = crate::widgets::switchrow::check(&c.header(), on, move |b| {
            let mut l = ch.borrow_mut();
            l.retain(|x| x != &key);
            if b.is_active() {
                l.push(key.clone());
            }
            a.set_service(json!({"log": {"keys": l.clone()}}));
        });
        cb.set_tooltip_text(Some(k));
        list.append(&cb);
        rows.push((c.header().to_lowercase(), cb));
    }
    if rows.is_empty() {
        list.append(&label(tr!("Sensorene vises når tjenesten har sendt første prøve."), &["muted"]));
    }
    let rows = Rc::new(rows);
    search.connect_search_changed(move |e| {
        let q = e.text().to_lowercase();
        for (t, cb) in rows.iter() {
            cb.set_visible(q.is_empty() || t.contains(q.as_str()));
        }
    });
    let note = label(tr!("Trådene (last og klokke per tråd) er med når du logger alle sensorene."), &["pop-sub", "dim"]);
    note.set_wrap(true);
    note.set_max_width_chars(40);
    body.append(&note);
    let p = crate::widgets::popover::popover(anchor, &body);
    p.add_css_class("pop-wide");
    p.connect_closed(|_| request_render());
    p.popup();
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = svc(app);
    let mut out = Vec::new();

    // ---- Logg til fil ----
    let status = label(&status_text(app), &["muted", "num"]);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_max_width_chars(36);
    let toggle = gtk::Button::with_label(if crate::logg_ui::is_active(app) { tr!("Stopp loggen") } else { tr!("Start loggen") });
    toggle.add_css_class("sm");
    {
        let a = app.clone();
        toggle.connect_clicked(move |_| crate::logg_ui::toggle(&a));
    }
    let open = crate::widgets::button(Some("folder"), Some(tr!("Vis i Filer")), &["sm"]);
    {
        let a = app.clone();
        open.connect_clicked(move |_| {
            let st = crate::logg_ui::status(&a);
            let p = st["file"].as_str().map(std::path::PathBuf::from).filter(|p| p.exists()).unwrap_or_else(|| {
                let d = log_dir(&a);
                let _ = std::fs::create_dir_all(&d);
                d
            });
            crate::rapport_ui::show_in_files(&a, &p);
        });
    }
    let sbox = hbox(8, &[]);
    sbox.append(&status);
    sbox.append(&toggle);
    sbox.append(&open);
    // Status og knapp følger loggen mens delen vises (én lytter, de siste etikettene).
    WATCH.with(|w| {
        let first = w.borrow().is_none();
        *w.borrow_mut() = Some((status.downgrade(), toggle.downgrade()));
        if first {
            let a = app.clone();
            app.on_log.connect(move |_| {
                WATCH.with(|w| {
                    if let Some((st, tg)) = w.borrow().as_ref()
                        && let (Some(st), Some(tg)) = (st.upgrade(), tg.upgrade())
                    {
                        crate::widgets::set_text(&st, &status_text(&a));
                        tg.set_label(if crate::logg_ui::is_active(&a) { tr!("Stopp loggen") } else { tr!("Start loggen") });
                    }
                });
            });
        }
    });
    super::live(&status, status_text);

    let which = Seg::new(&[("alle", tr!("Alle")), ("utvalgte", tr!("Utvalgte"))], if s.log.all { "alle" } else { "utvalgte" }, tr!("Sensorer i loggen"));
    {
        let a = app.clone();
        which.connect(move |v| {
            a.set_service(json!({"log": {"all": v == "alle"}}));
            request_render();
        });
    }
    let sens = hbox(8, &[]);
    sens.append(&which.root);
    if !s.log.all {
        let n = s.log.keys.len();
        let pick = gtk::Button::with_label(&if n == 0 { tr!("Velg …").to_string() } else { trf!("{antall} valgt …", antall = n) });
        pick.add_css_class("sm");
        let a = app.clone();
        pick.connect_clicked(move |b| pick_sensors(&a, b));
        sens.append(&pick);
    }

    let iv: Vec<(String, String)> = rigg_core::settings::LOG_INTERVALS.iter().map(|ms| (ms.to_string(), F::seconds(*ms))).collect();
    let iv_refs: Vec<(&str, &str)> = iv.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let interval = Seg::new(&iv_refs, &s.log.interval_ms.to_string(), tr!("Intervall"));
    {
        let a = app.clone();
        interval.connect(move |v| {
            if let Ok(n) = v.parse::<u64>() {
                a.set_service(json!({"log": {"intervalMs": n}}));
            }
        });
    }

    let dir = log_dir(app);
    let dbox = hbox(8, &["path-box"]);
    let code = label(&rigg_core::paths::tilde(&dir), &["mono", "muted"]);
    code.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    code.set_max_width_chars(28);
    code.set_tooltip_text(Some(&dir.display().to_string()));
    dbox.append(&code);
    let choose = gtk::Button::with_label(tr!("Velg …"));
    choose.add_css_class("sm");
    {
        let a = app.clone();
        choose.connect_clicked(move |_| {
            let Some(win) = a.window() else { return };
            let d = gtk::FileDialog::new();
            d.set_title(tr!("Mappe for loggen"));
            d.set_initial_folder(Some(&gio::File::for_path(log_dir(&a))));
            let b = a.clone();
            d.select_folder(Some(&win), None::<&gio::Cancellable>, move |res| {
                if let Ok(f) = res
                    && let Some(p) = f.path()
                {
                    b.set_service(json!({"log": {"dir": p.display().to_string()}}));
                    request_render();
                }
            });
        });
    }
    dbox.append(&choose);
    if s.log.dir.is_some() {
        let reset = crate::widgets::button(Some("rotate-ccw"), None, &["flat", "sm", "icon-only"]);
        reset.set_tooltip_text(Some(tr!("Tilbake til ~/Dokumenter/Agentic Rigg")));
        reset.update_property(&[gtk::accessible::Property::Label(tr!("Tilbake til standardmappa"))]);
        let a = app.clone();
        reset.connect_clicked(move |_| {
            a.set_service(json!({"log": {"dir": null}}));
            request_render();
        });
        dbox.append(&reset);
    }

    let format = Seg::new(&[("standard", tr!("Standard CSV")), ("norsk", tr!("Norsk regneark"))], &s.log.format, tr!("Format"));
    {
        let a = app.clone();
        format.connect(move |v| a.set_service(json!({"log": {"format": v}})));
    }

    out.push(group(
        tr!("Logg til fil"),
        GroupOpts {
            k: &kw("logg csv fil ctrl+l", tr!("søkeord", "logg csv fil ctrl+l")),
            desc: Some(tr!("Tjenesten skriver loggen, så den fortsetter når vinduet lukkes. Start og stopp også med knappen på Sensorer, eller Ctrl+L.")),
            note: Some(tr!("Ny fil ved 100 MB. Loggen stopper med en melding hvis disken har under 500 MB ledig. Kolonnene ligger fast mens loggen går; nye sensorer tas med i neste logg.")),
            ..Default::default()
        },
        vec![
            row(tr!("Status"), None, &sbox, &kw("logg status start stopp vis i filer", tr!("søkeord", "logg status start stopp vis i filer"))),
            row(tr!("Sensorer"), Some(tr!("Alle sensorene, eller bare de du velger (også med høyreklikk på Sensorer › «Ta med i loggen»).")), &sens, &kw("sensorer utvalgte alle", tr!("søkeord", "sensorer utvalgte alle"))),
            row(tr!("Intervall"), Some(tr!("Hvor ofte en rad skrives. Raskere enn takten tjenesten leser i, gir ikke flere rader.")), &interval.root, &kw("intervall sekunder", tr!("søkeord", "intervall sekunder"))),
            row(tr!("Mappe"), Some(tr!("Filnavn som rigg-2026-11-07_14-32-10.csv.")), &dbox, &kw("mappe dokumenter sti", tr!("søkeord", "mappe dokumenter sti"))),
            row(tr!("Format"), Some(tr!("Standard CSV bruker komma og desimalpunktum. Norsk regneark bruker semikolon og desimalkomma, og åpnes rett i LibreOffice og Excel med norsk oppsett.")), &format.root, &kw("format komma semikolon desimal regneark excel libreoffice", tr!("søkeord", "format komma semikolon desimal regneark excel libreoffice"))),
        ],
    ));

    // ---- Skrivebordsvarsler ----
    let th = &app.settings().thresholds;
    let (gpu_crit, cpu_crit) = (th.gpu_temp_crit, th.cpu_temp_crit);
    let a1 = app.clone();
    let main = switch_row(
        tr!("Varsle ved rød grense"),
        Some(tr!("Et varsel på skrivebordet når en verdi har vært over den røde grensen i 10 s. Høyst ett varsel per verdi per 15 min. Virker også når vinduet er lukket og tjenesten går i bakgrunnen.")),
        s.alarms.desktop,
        &kw("varsel skrivebord alarm rød grense", tr!("søkeord", "varsel skrivebord alarm rød grense")),
        move |on| {
            a1.set_service(json!({"alarms": {"desktop": on}}));
            request_render();
        },
    );
    let mut rows = vec![main];
    let on = s.alarms.desktop;
    let a2 = app.clone();
    let g = switch_row(
        tr!("GPU-temperatur"),
        Some(&trf!("Rød grense {grense}. Endres under Grafer og sensorer › Varselfarger.", grense = F::temp(Some(gpu_crit), 0))),
        s.alarms.gpu_temp,
        &kw("gpu temperatur varsel", tr!("søkeord", "gpu temperatur varsel")),
        move |v| a2.set_service(json!({"alarms": {"gpuTemp": v}})),
    );
    g.widget.set_sensitive(on);
    g.widget.add_css_class("avhengig");
    rows.push(g);
    let a3 = app.clone();
    let c = switch_row(
        tr!("CPU-temperatur"),
        Some(&trf!("Rød grense {grense} (Tdie).", grense = F::temp(Some(cpu_crit), 0))),
        s.alarms.cpu_temp,
        &kw("cpu temperatur varsel tdie", tr!("søkeord", "cpu temperatur varsel tdie")),
        move |v| a3.set_service(json!({"alarms": {"cpuTemp": v}})),
    );
    c.widget.set_sensitive(on);
    c.widget.add_css_class("avhengig");
    rows.push(c);
    let a4 = app.clone();
    let l = switch_row(
        tr!("Skjermkortet svarer ikke"),
        Some(tr!("Også når driveren er oppdatert og maskinen må startes på nytt, eller driveren ikke kan leses.")),
        s.alarms.gpu_lost,
        &kw("skjermkort borte driver varsel", tr!("søkeord", "skjermkort borte driver varsel")),
        move |v| a4.set_service(json!({"alarms": {"gpuLost": v}})),
    );
    l.widget.set_sensitive(on);
    l.widget.add_css_class("avhengig");
    rows.push(l);
    let a5 = app.clone();
    let dh = switch_row(
        tr!("Diskhelse"),
        Some(tr!("Når SMART melder at en disk feiler, eller antallet dårlige sektorer øker. Tjenesten ser på SMART hvert 10. minutt når vinduet er lukket.")),
        s.alarms.disk_health,
        &kw("disk helse smart sektorer varsel", tr!("søkeord", "disk helse smart sektorer varsel")),
        move |v| a5.set_service(json!({"alarms": {"diskHealth": v}})),
    );
    dh.widget.set_sensitive(on);
    dh.widget.add_css_class("avhengig");
    rows.push(dh);
    let hint = label(tr!("Klikk på et varsel for å åpne appen på riktig side. Varslene sendes av tjenesten, uten lyd."), &["set-note"]);
    hint.set_wrap(true);
    rows.push(custom_row(&hint, &kw("klikk varsel lyd", tr!("søkeord", "klikk varsel lyd"))));
    out.push(group(tr!("Skrivebordsvarsler"), GroupOpts { k: &kw("varsler notification", tr!("søkeord", "varsler notification")), ..Default::default() }, rows));
    out
}
