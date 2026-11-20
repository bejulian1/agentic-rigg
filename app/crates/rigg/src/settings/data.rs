//! Data og tilbakestilling: mappene (med «Vis i Filer»), eksport og import
//! i demoens format, skjulte meldinger, slett historikken og tilbakestill
//! alt (produktspesifikasjonen 3.10). Angre i 5 s der noe forsvinner.

use super::{Group, GroupOpts, group, kw, request_render, row, save};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{hbox, label};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::settings::{Settings, ServiceSettings, export, import};
use rigg_core::{tr, trn};
use serde_json::json;
use std::path::Path;
use std::rc::Rc;

fn open_folder(app: &App, dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let uri = gio::File::for_path(dir).uri();
    if let Err(e) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        app.toast(Toast::new(tr!("Kunne ikke åpne mappa")).body(&e.to_string()).error());
    }
}

fn copy(app: &App, text: &str) {
    if let Some(d) = gtk::gdk::Display::default() {
        d.clipboard().set_text(text);
        app.toast(Toast::new(tr!("Stien er kopiert")).ms(2000));
    }
}

fn path_row(app: &Rc<App>, title: &str, sub: &str, dir: &Path, k: &str) -> super::Row {
    let bx = hbox(8, &["path-box"]);
    let shown = rigg_core::paths::tilde(dir);
    let code = label(&shown, &["mono", "muted"]);
    code.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    code.set_max_width_chars(40);
    code.set_tooltip_text(Some(&dir.display().to_string()));
    code.set_selectable(true);
    bx.append(&code);
    let cp = crate::widgets::button(Some("copy"), None, &["flat", "sm", "icon-only"]);
    cp.set_tooltip_text(Some(tr!("Kopier stien")));
    cp.update_property(&[gtk::accessible::Property::Label(tr!("Kopier stien"))]);
    {
        let (a, d) = (app.clone(), dir.display().to_string());
        cp.connect_clicked(move |_| copy(&a, &d));
    }
    bx.append(&cp);
    let open = crate::widgets::button(Some("folder"), Some(tr!("Vis i Filer")), &["sm"]);
    {
        let (a, d) = (app.clone(), dir.to_path_buf());
        open.connect_clicked(move |_| open_folder(&a, &d));
    }
    bx.append(&open);
    row(title, Some(sub), &bx, k)
}

fn service_settings(app: &App) -> ServiceSettings {
    let v = app.data.borrow().service.clone();
    rigg_core::settings::lenient_service(&v)
}

fn export_dialog(app: &Rc<App>) {
    let Some(win) = app.window() else { return };
    let d = gtk::FileDialog::new();
    d.set_title(tr!("Eksporter innstillinger"));
    let t = crate::app::local_now();
    d.set_initial_name(Some(&format!("agentic-rigg-innstillinger-{:04}-{:02}-{:02}.json", t.year, t.month, t.day)));
    let a = app.clone();
    d.save(Some(&win), None::<&gio::Cancellable>, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        let now = crate::app::local_now();
        let stamp = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", now.year, now.month, now.day, now.hour, now.minute, now.second);
        let v = export(&a.settings(), &service_settings(&a), &stamp);
        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
        match rigg_core::write_atomic(&path, text.as_bytes()) {
            Ok(()) => a.toast(Toast::new(tr!("Innstillingene er eksportert")).body(&path.file_name().unwrap_or_default().to_string_lossy())),
            Err(e) => a.toast(Toast::new(tr!("Kunne ikke lagre fila")).body(&e.to_string()).error()),
        }
    });
}

fn restore(app: &Rc<App>, before: Settings, svc: ServiceSettings) {
    app.replace_settings(before);
    app.set_service(svc.to_value());
    request_render();
}

fn import_dialog(app: &Rc<App>) {
    let Some(win) = app.window() else { return };
    let d = gtk::FileDialog::new();
    d.set_title(tr!("Importer innstillinger"));
    let f = gtk::FileFilter::new();
    f.set_name(Some("JSON"));
    f.add_pattern("*.json");
    let list = gio::ListStore::new::<gtk::FileFilter>();
    list.append(&f);
    d.set_filters(Some(&list));
    let a = app.clone();
    d.open(Some(&win), None::<&gio::Cancellable>, move |res| {
        let Ok(file) = res else { return };
        let Some(path) = file.path() else { return };
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 64 * 1024 {
            a.toast(Toast::new(tr!("Fila er for stor")).body(tr!("En innstillingsfil er bare noen få kB.")).error());
            return;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            a.toast(Toast::new(tr!("Kunne ikke lese fila")).error());
            return;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            a.toast(Toast::new(tr!("Kunne ikke lese fila")).body(tr!("Den er ikke gyldig JSON.")).error());
            return;
        };
        let before = a.settings().clone();
        let svc_before = service_settings(&a);
        match import(&v, &before, &svc_before) {
            Ok((ui, svc)) => {
                a.replace_settings(ui);
                a.set_service(svc.to_value());
                request_render();
                let a2 = a.clone();
                a.toast(Toast::new(tr!("Innstillinger importert")).action(tr!("Angre"), move || restore(&a2, before.clone(), svc_before.clone())));
            }
            Err(e) => a.toast(Toast::new(tr!("Importen ble avvist")).body(&e.0).error()),
        }
    });
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let p = app.profile;
    let exp = crate::widgets::button(Some("download"), Some(tr!("Eksporter …")), &[]);
    {
        let a = app.clone();
        exp.connect_clicked(move |_| export_dialog(&a));
    }
    let imp = crate::widgets::button(Some("upload"), Some(tr!("Importer …")), &[]);
    {
        let a = app.clone();
        imp.connect_clicked(move |_| import_dialog(&a));
    }
    let dismissed = app.settings().dismissed.len();
    let show = gtk::Button::with_label(tr!("Vis igjen"));
    show.set_sensitive(dismissed > 0);
    if dismissed == 0 {
        show.set_tooltip_text(Some(tr!("Ingen meldinger er skjult")));
    }
    {
        let a = app.clone();
        show.connect_clicked(move |_| {
            save(&a, json!({"dismissed": []}));
            request_render();
        });
    }
    let clear = gtk::Button::with_label(tr!("Slett historikken"));
    {
        let a = app.clone();
        clear.connect_clicked(move |_| {
            // Tjenesten sletter først etter fristen, så Angre virker.
            let cancelled = Rc::new(std::cell::Cell::new(false));
            let c2 = cancelled.clone();
            a.toast(Toast::new(tr!("Historikken slettes")).body(tr!("Grafene begynner på nytt om 5 s.")).action(tr!("Angre"), move || c2.set(true)).ms(5000));
            let a2 = a.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(5200), move || {
                if cancelled.get() {
                    return;
                }
                let a3 = a2.clone();
                a2.client.request("clear_history", json!({}), move |r| {
                    if r.is_ok() {
                        a3.toast(Toast::new(tr!("Historikken er slettet")).ms(2500));
                    } else {
                        a3.toast(Toast::new(tr!("Kunne ikke slette historikken")).body(tr!("Tjenesten svarer ikke.")).error());
                    }
                });
            });
        });
    }
    let reset = gtk::Button::with_label(tr!("Tilbakestill"));
    reset.add_css_class("destructive");
    {
        let a = app.clone();
        reset.connect_clicked(move |_| {
            let before = a.settings().clone();
            let svc_before = service_settings(&a);
            let next = Settings { window: before.window.clone(), eggs_found: before.eggs_found.clone(), ..Settings::default() };
            a.replace_settings(next);
            a.set_service(ServiceSettings::default().to_value());
            request_render();
            let a2 = a.clone();
            a.toast(Toast::new(tr!("Innstillingene er tilbakestilt")).action(tr!("Angre"), move || restore(&a2, before.clone(), svc_before.clone())));
        });
    }
    let hidden_sub = if dismissed > 0 {
        trn!(dismissed, "{antall} infomelding er skjult.", "{antall} infomeldinger er skjult.", antall = dismissed)
    } else {
        tr!("Ingen infomeldinger er skjult.").into()
    };
    vec![
        group(
            tr!("Lagring"),
            GroupOpts { note: Some(tr!("Ingenting her leses eller sendes ut av maskinen.")), ..Default::default() },
            vec![
                path_row(app, tr!("Innstillinger"), tr!("ui.json (vinduet) og tjeneste.json (tjenesten)."), &rigg_core::paths::config_dir(p), &kw("mappe sti lagring config", tr!("søkeord", "mappe sti lagring config"))),
                path_row(app, tr!("Historikk og min/maks"), tr!("48 timer med historikk for grafene, og min og maks."), &rigg_core::paths::state_dir(p), &kw("mappe sti historikk state", tr!("søkeord", "mappe sti historikk state"))),
            ],
        ),
        group(
            tr!("Innstillinger som fil"),
            GroupOpts::default(),
            vec![
                row(tr!("Eksporter"), Some(tr!("Lagrer innstillingene som JSON, for sikkerhetskopi eller en annen maskin.")), &exp, &kw("lagre fil json sikkerhetskopi", tr!("søkeord", "lagre fil json sikkerhetskopi"))),
                row(tr!("Importer"), Some(tr!("Leser en eksportert fil. Er noe ugyldig, endres ingenting.")), &imp, &kw("åpne fil json", tr!("søkeord", "åpne fil json"))),
            ],
        ),
        group(
            tr!("Tilbakestilling"),
            GroupOpts::default(),
            vec![
                row(tr!("Skjulte meldinger"), Some(&hidden_sub), &show, &kw("infolinje melding", tr!("søkeord", "infolinje melding"))),
                row(tr!("Slett historikken"), Some(tr!("Grafene og 48 timer med historikk begynner på nytt. Min og maks blir. Kan angres i noen sekunder.")), &clear, &kw("historikk graf slett", tr!("søkeord", "historikk graf slett"))),
                row(tr!("Tilbakestill alle innstillinger"), Some(tr!("Alt i Innstillinger går tilbake til standard. Profiler og lysvalg blir. Kan angres i noen sekunder.")), &reset, &kw("standard nullstill fabrikk", tr!("søkeord", "standard nullstill fabrikk"))),
            ],
        ),
    ]
}
