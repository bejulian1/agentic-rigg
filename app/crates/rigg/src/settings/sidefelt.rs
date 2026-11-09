//! Sidefelt: statusblokken (rekkefølge, fjern, legg til, Alt+↑/↓), bredden,
//! sparklinjene og hvilke sider som står i sidefeltet.

use super::{Group, GroupOpts, custom_row, group, request_render, row, save, switch_row};
use crate::sidebar::status_label;
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{Icon, hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::settings::{PAGES, STATUS_ITEMS, STATUS_MAX};
use serde_json::json;
use std::rc::Rc;

fn set_items(app: &Rc<App>, items: Vec<String>, focus: Option<String>) {
    save(app, json!({"statusItems": items}));
    request_render();
    if let Some(f) = focus {
        FOCUS.with(|x| *x.borrow_mut() = Some(f));
    }
}

thread_local! {
    static FOCUS: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn status_group(app: &Rc<App>) -> Group {
    let items = app.settings().status_items.clone();
    let list = vbox(0, &[]);
    list.set_accessible_role(gtk::AccessibleRole::List);
    list.update_property(&[gtk::accessible::Property::Label("Verdier i statusblokken, i rekkefølge")]);
    if items.is_empty() {
        let r = hbox(12, &["order-row"]);
        r.append(&Icon::new("pin", 16));
        let t = vbox(0, &[]);
        t.append(&label("Statusblokken er tom", &["title"]));
        let s = label("Legg til verdier her, eller fest sensorer med nålen på Sensorer.", &["subtitle"]);
        s.set_wrap(true);
        t.append(&s);
        r.append(&t);
        list.append(&r);
    }
    let focus = FOCUS.with(|f| f.borrow_mut().take());
    let n = items.len();
    let disks = app.sys().map(|s| s.disks).unwrap_or_default();
    for (i, id) in items.iter().enumerate() {
        let mut def = status_label(id);
        if let Some((short, long)) = crate::pages::sensorer::disk_name(id, &disks) {
            (def.label, def.long) = (short, long);
        }
        let r = hbox(12, &["order-row"]);
        r.set_focusable(true);
        r.set_accessible_role(gtk::AccessibleRole::ListItem);
        r.update_property(&[gtk::accessible::Property::Label(&format!("{} av {n}: {}. Alt+pil opp eller ned flytter.", i + 1, def.long))]);
        let t = vbox(0, &[]);
        t.set_hexpand(true);
        t.append(&label(&def.long, &["title"]));
        let src = match def.src {
            "gpu" => "Skjermkort",
            "cpu" => "Prosessor",
            "mem" => "Minne",
            "net" => "Nett (alle kort)",
            "disk" => "Disker",
            _ => "Andre sensorer",
        };
        t.append(&label(&format!("{src} · i sidefeltet: {}", def.label), &["subtitle"]));
        r.append(&t);
        let pos = label(&(i + 1).to_string(), &["pos", "num"]);
        r.append(&pos);
        let del = crate::widgets::button(Some("x"), None, &["flat", "sm", "icon-only"]);
        del.set_tooltip_text(Some("Fjern fra statusblokken"));
        del.update_property(&[gtk::accessible::Property::Label(&format!("Fjern {}", def.long))]);
        del.set_valign(gtk::Align::Center);
        {
            let (a, id) = (app.clone(), id.clone());
            del.connect_clicked(move |_| {
                let cur: Vec<String> = a.settings().status_items.iter().filter(|x| **x != id).cloned().collect();
                let next = cur.get(i.min(cur.len().saturating_sub(1))).cloned();
                set_items(&a, cur, next);
            });
        }
        r.append(&del);
        let keys = gtk::EventControllerKey::new();
        {
            let (a, id, rw) = (app.clone(), id.clone(), r.clone());
            keys.connect_key_pressed(move |_, key, _, st| {
                use gtk::gdk::Key as K;
                let alt = st.contains(gtk::gdk::ModifierType::ALT_MASK);
                match key {
                    K::Up | K::Down if alt => {
                        let mut cur = a.settings().status_items.clone();
                        let Some(i) = cur.iter().position(|x| *x == id) else { return gtk::glib::Propagation::Stop };
                        let j = if key == K::Up { i as i32 - 1 } else { i as i32 + 1 };
                        if j >= 0 && (j as usize) < cur.len() {
                            let x = cur.remove(i);
                            cur.insert(j as usize, x);
                            set_items(&a, cur, Some(id.clone()));
                        }
                        gtk::glib::Propagation::Stop
                    }
                    K::Up => {
                        if let Some(p) = rw.prev_sibling() {
                            p.grab_focus();
                        }
                        gtk::glib::Propagation::Stop
                    }
                    K::Down => {
                        if let Some(nx) = rw.next_sibling() {
                            nx.grab_focus();
                        }
                        gtk::glib::Propagation::Stop
                    }
                    K::Delete | K::BackSpace => {
                        let cur: Vec<String> = a.settings().status_items.iter().filter(|x| **x != id).cloned().collect();
                        set_items(&a, cur, None);
                        gtk::glib::Propagation::Stop
                    }
                    _ => gtk::glib::Propagation::Proceed,
                }
            });
        }
        r.add_controller(keys);
        list.append(&r);
        if focus.as_deref() == Some(id.as_str()) {
            let rw = r.clone();
            gtk::glib::idle_add_local_once(move || {
                rw.grab_focus();
                rw.add_css_class("flash");
                let r2 = rw.clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || r2.remove_css_class("flash"));
            });
        }
    }
    // «+ Legg til verdi …» nederst, med søk i en popover.
    let full = items.len() >= STATUS_MAX;
    let add = crate::widgets::button(Some("plus"), Some("Legg til verdi …"), &["flat"]);
    add.set_halign(gtk::Align::Start);
    add.set_sensitive(!full);
    if full {
        add.set_tooltip_text(Some(&format!("Statusblokken er full ({STATUS_MAX} av {STATUS_MAX}). Fjern en verdi først.")));
    }
    {
        let a = app.clone();
        add.connect_clicked(move |b| picker(&a, b));
    }
    let foot = hbox(0, &["order-foot"]);
    foot.append(&add);
    list.append(&foot);
    let count = label(&format!("{} av {STATUS_MAX}", items.len()), &[]);
    let mut g = group(
        "Statusblokken",
        GroupOpts {
            desc: Some("Verdiene nederst i sidefeltet, i denne rekkefølgen. Velg en rad og bruk Alt+↑/↓ for å flytte. Fest flere fra Sensorer med nålen."),
            aside: Some(count.upcast()),
            whole: true,
            k: "status nederst rekkefølge fest sensor verdier",
            ..Default::default()
        },
        vec![],
    );
    g.widget.insert_child_after(&list, g.widget.first_child().and_then(|h| h.next_sibling()).as_ref());
    g.rows.push(custom_row(&gtk::Box::new(gtk::Orientation::Horizontal, 0), &items.join(" ")));
    g
}

/// Popover med søk: de faste verdiene (sensorene kommer med Sensorer i runde 2).
fn picker(app: &Rc<App>, anchor: &gtk::Button) {
    let body = vbox(4, &[]);
    let q = gtk::SearchEntry::new();
    q.set_placeholder_text(Some("Søk etter verdi"));
    body.append(&q);
    let results = vbox(0, &["pick-list"]);
    let sc = gtk::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk::PolicyType::Never);
    sc.set_max_content_height(320);
    sc.set_propagate_natural_height(true);
    sc.set_child(Some(&results));
    body.append(&sc);
    let pop = crate::widgets::popover::popover(anchor, &body);
    pop.add_css_class("pop-wide");
    let draw = {
        let (a, results, pop) = (app.clone(), results.clone(), pop.clone());
        move |f: &str| {
            crate::widgets::clear(&results);
            let cur = a.settings().status_items.clone();
            results.append(&label("Faste verdier", &["menu-label"]));
            let mut any = false;
            for id in STATUS_ITEMS {
                let def = status_label(id);
                if !f.is_empty() && !format!("{} {}", def.long, def.label).to_lowercase().contains(&f.to_lowercase()) {
                    continue;
                }
                any = true;
                let on = cur.iter().any(|x| x == id);
                let b = gtk::Button::new();
                b.add_css_class("flat");
                b.add_css_class("menu-item");
                b.add_css_class("pick");
                let bx = hbox(12, &[]);
                let n = label(&def.long, &[]);
                n.set_hexpand(true);
                bx.append(&n);
                bx.append(&label(if on { "Lagt til" } else { "" }, &["ps"]));
                b.set_child(Some(&bx));
                b.set_sensitive(!on);
                let (a2, p2) = (a.clone(), pop.clone());
                b.connect_clicked(move |_| {
                    let mut cur = a2.settings().status_items.clone();
                    if cur.len() >= STATUS_MAX {
                        return;
                    }
                    cur.push(id.to_string());
                    p2.popdown();
                    set_items(&a2, cur, Some(id.to_string()));
                });
                results.append(&b);
            }
            if !any {
                results.append(&label(&format!("Ingen verdi heter «{f}»"), &["pop-sub"]));
            }
        }
    };
    let draw = Rc::new(draw);
    draw("");
    {
        let d = draw.clone();
        q.connect_search_changed(move |e| d(e.text().trim()));
    }
    pop.popup();
    q.grab_focus();
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    let width = crate::widgets::Seg::new(&[("full", "Full"), ("rail", "Ikonlist")], if s.side_collapsed { "rail" } else { "full" }, "Bredde på sidefeltet");
    {
        let a = app.clone();
        width.connect(move |v| {
            save(&a, json!({"sideCollapsed": v == "rail"}));
        });
    }
    let narrow = app.shell.get().is_some_and(|sh| sh.window.width() < 860 && sh.window.width() > 0);
    let wsub = if narrow {
        "Vinduet er smalt, så sidefeltet er en ikonlist nå. Valget gjelder i bredere vinduer."
    } else {
        "Ikonlista viser bare ikoner og korte tall. Også Ctrl+B og knappen nederst i sidefeltet."
    };
    let checks = gtk::Grid::new();
    checks.add_css_class("check-list");
    checks.set_column_spacing(16);
    checks.set_column_homogeneous(true);
    let hidden = s.hidden_pages.clone();
    for (i, (id, name)) in PAGES.iter().enumerate() {
        let a = app.clone();
        let cb = crate::widgets::switchrow::check(name, !hidden.iter().any(|h| h == id), move |cb| {
            let mut next: Vec<String> = a.settings().hidden_pages.clone();
            if cb.is_active() {
                next.retain(|x| x != id);
            } else if !next.iter().any(|x| x == id) {
                next.push(id.to_string());
            }
            if next.len() >= PAGES.len() {
                cb.set_active(true);
                a.toast(Toast::new("Minst én side må vises").error());
                return;
            }
            let ordered: Vec<&str> = PAGES.iter().map(|p| p.0).filter(|p| next.iter().any(|n| n == p)).collect();
            save(&a, json!({"hiddenPages": ordered}));
        });
        checks.attach(&cb, (i % 3) as i32, (i / 3) as i32, 1, 1);
    }
    let pages_row = vbox(0, &["set-row", "wrap"]);
    pages_row.append(&checks);
    let names: String = PAGES.iter().map(|p| p.1).collect::<Vec<_>>().join(" ");
    let (a1, a2) = (app.clone(), app.clone());
    vec![
        status_group(app),
        group(
            "Visning",
            GroupOpts::default(),
            vec![
                row("Bredde", Some(wsub), &width.root, "ikonlist legg sammen utvid smal"),
                switch_row("Sparklinjer", Some("Liten graf for siste minutt ved hver verdi i statusblokken."), s.status_sparks, "statusblokk graf", move |on| {
                    save(&a1, json!({"statusSparks": on}));
                }),
            ],
        ),
        group(
            "Sider",
            GroupOpts { desc: Some("Sidene i sidefeltet. Skjulte sider kan fortsatt åpnes med hurtigtasten."), k: "skjul vis navigasjon", ..Default::default() },
            vec![
                custom_row(&pages_row, &names),
                switch_row("Vis hurtigtaster", Some("Tallene 1–8 ved sidene. Ellers står de i verktøytipset og under Tastatursnarveier."), s.show_kbd, "taster tall snarvei", move |on| {
                    save(&a2, json!({"showKbd": on}));
                }),
            ],
        ),
    ]
}
