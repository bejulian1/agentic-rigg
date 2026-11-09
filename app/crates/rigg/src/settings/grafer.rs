//! Grafer og sensorer (demoens del med samme navn): tidsvindu på Grafer,
//! markøren, y-aksen, fylt flate, hvilke grafer som vises, sparklinjene på
//! Sensorer, «Husk min og maks mellom økter» (som nå gjelder tjenesten) og
//! varselfargene.
//!
//! **Eies av 2A (Data).**

use super::{Group, GroupOpts, custom_row, group, request_render, row, row_wrap, save, seg, switch_row};
use crate::state::App;
use crate::widgets::chart::{DEFS, WINDOWS};
use crate::widgets::{hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::fmt as F;
use serde_json::json;
use std::rc::Rc;

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    let win_opts: Vec<(String, &str)> = WINDOWS.iter().map(|w| (w.0.to_string(), w.1)).collect();
    let win_refs: Vec<(&str, &str)> = win_opts.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    let window = seg(app, &win_refs, &s.chart_window.to_string(), "chartWindow", "Tidsvindu på Grafer", true);
    // Nedtrekksliste når delen er smal (Innstillinger ved 640 px).
    let win_dd = {
        let a = app.clone();
        crate::widgets::chart::window_dropdown(s.chart_window as i64, "Tidsvindu på Grafer", move |sec| {
            save(&a, json!({"chartWindow": sec}));
        })
    };
    win_dd.set_halign(gtk::Align::Start);
    let window_ctrl = crate::widgets::chart::Adaptive::new(&window.root, &win_dd).start();
    window_ctrl.set_halign(gtk::Align::Fill);
    window_ctrl.set_hexpand(true);
    let scale = seg(app, &[("auto", "Tilpass"), ("fixed", "Fast")], &s.chart_scale, "chartScale", "Y-akse", false);
    let (a1, a2, a3, a4) = (app.clone(), app.clone(), app.clone(), app.clone());

    // Grafer som vises: avkrysning, minst én.
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(4);
    flow.set_min_children_per_line(2);
    flow.set_column_spacing(16);
    flow.set_row_spacing(2);
    flow.set_homogeneous(true);
    flow.add_css_class("check-list");
    for d in DEFS.iter() {
        let on = s.charts.iter().any(|c| c == d.id);
        let a = app.clone();
        let id = d.id;
        let c = crate::widgets::switchrow::check(d.title, on, move |c| {
            let mut set = a.settings().charts.clone();
            if c.is_active() {
                if !set.iter().any(|x| x == id) {
                    set.push(id.into());
                }
            } else {
                set.retain(|x| x != id);
            }
            if set.is_empty() {
                c.set_active(true);
                a.toast(crate::toast::Toast::new("Minst én graf må vises").error());
                return;
            }
            let ordered: Vec<&str> = DEFS.iter().map(|d| d.id).filter(|x| set.iter().any(|s| s == x)).collect();
            save(&a, json!({"charts": ordered}));
        });
        flow.insert(&c, -1);
    }
    let pick = vbox(0, &["set-row", "wrap", "check-row"]);
    pick.append(&flow);
    let titles: Vec<&str> = DEFS.iter().map(|d| d.title).collect();

    vec![
        group(
            "Grafer",
            GroupOpts { note: Some("Tjenesten tar vare på 48 timer med historikk, også over en omstart. Med «Fortsett i bakgrunnen» av (Generelt) blir det hull i grafene der appen var lukket."), ..Default::default() },
            vec![
                row_wrap("Tidsvindu på Grafer", Some("Hvor langt tilbake siden Grafer viser. Oversikt og Sensorer har eget valg ved grafene."), &window_ctrl, "historikk minutter timer"),
                switch_row("Markør på alle grafene", Some("Når du holder over én graf, vises tidspunktet på de andre også. Av: bare på grafen du holder over."), s.chart_sync, "synkron tidspunkt sikte", move |on| {
                    save(&a1, json!({"chartSync": on}));
                }),
                row("Y-akse", Some("Tilpass følger dataene i vinduet. Prosent har alltid fast skala 0–100."), &scale.root, "skala akse"),
                switch_row("Fylt flate", Some("Svak farge under linja. Lange vinduer viser i stedet et bånd fra minste til største verdi."), s.chart_fill, "fyll", move |on| {
                    save(&a2, json!({"chartFill": on}));
                }),
            ],
        ),
        group("Grafer som vises", GroupOpts { desc: Some("På siden Grafer. Kan også velges i toppfeltet der."), k: "velg skjul", whole: true, ..Default::default() }, vec![custom_row(&pick, &titles.join(" "))]),
        group(
            "Sensorer",
            GroupOpts::default(),
            vec![
                switch_row("Sparklinjer", Some("Liten graf for siste 60 sekunder ved hver sensor."), s.sensor_sparks, "graf", move |on| {
                    save(&a3, json!({"sensorSparks": on}));
                }),
                switch_row(
                    "Husk min og maks mellom økter",
                    Some("Tjenesten tar vare på min og maks, også når appen er lukket. Nullstilles bare når du trykker ↺ på Sensorer. Snitt gjelder siden tjenesten startet."),
                    s.sensor_keep_min_max,
                    "minimum maksimum",
                    move |on| {
                        save(&a4, json!({"sensorKeepMinMax": on}));
                    },
                ),
            ],
        ),
        thresholds(app),
    ]
}

/// Varselfargene: gul og rød grense for temperatur, gul for last.
fn thresholds(app: &Rc<App>) -> Group {
    let t = app.settings().thresholds.clone();
    let c = F::temp_label();
    let field = |key: &'static str, min: f64, max: f64, temp: bool, value: f64, name: &str| -> gtk::Widget {
        let (lo, hi, v) = if temp { (F::temp_out(Some(min)).unwrap_or(min).round(), F::temp_out(Some(max)).unwrap_or(max).round(), F::temp_out(Some(value)).unwrap_or(value).round()) } else { (min, max, value) };
        let sp = gtk::SpinButton::with_range(lo, hi, 1.0);
        sp.set_value(v);
        sp.set_digits(0);
        sp.set_numeric(true);
        sp.add_css_class("thr-field");
        sp.set_valign(gtk::Align::Center);
        sp.update_property(&[gtk::accessible::Property::Label(name)]);
        let a = app.clone();
        sp.connect_value_changed(move |sp| {
            let v = sp.value();
            let c = if temp { F::temp_in(v).round() } else { v };
            if !super::save(&a, json!({"thresholds": {key: c}})) {
                request_render();
            }
        });
        let b = hbox(4, &[]);
        b.append(&sp);
        b.append(&label(if temp { c } else { "%" }, &["muted"]));
        b.upcast()
    };
    let none = || -> gtk::Widget {
        let l = label("–", &["dim", "thr-none"]);
        l.set_tooltip_text(Some("Last har bare gul grense"));
        l.set_xalign(0.5);
        l.upcast()
    };
    let head = hbox(16, &["set-row", "thr-head"]);
    let sp = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sp.set_hexpand(true);
    head.append(&sp);
    for (cls, name) in [("warn", "Gul"), ("danger", "Rød")] {
        let h = hbox(6, &["thr-h"]);
        h.append(&crate::widgets::rows::dot(cls));
        h.append(&label(name, &[]));
        head.append(&h);
    }
    head.set_accessible_role(gtk::AccessibleRole::Presentation);
    let thr = |title: &str, a: gtk::Widget, b: gtk::Widget, k: &str| {
        let ctrl = hbox(16, &["thr-ctrl"]);
        a.add_css_class("thr-cell");
        b.add_css_class("thr-cell");
        ctrl.append(&a);
        ctrl.append(&b);
        row(title, None, &ctrl, k)
    };
    group(
        "Varselfarger",
        GroupOpts { desc: Some("Verdien blir gul eller rød over grensen: i sidefeltet, på Oversikt, i Nå-kolonnen på Sensorer og i stolpene."), whole: true, k: "grense terskel advarsel gul rød", ..Default::default() },
        vec![
            custom_row(&head, ""),
            thr("GPU-temperatur", field("gpuTempWarn", 40.0, 100.0, true, t.gpu_temp_warn, "GPU-temperatur, gul grense"), field("gpuTempCrit", 40.0, 110.0, true, t.gpu_temp_crit, "GPU-temperatur, rød grense"), "gpu"),
            thr("CPU-temperatur", field("cpuTempWarn", 40.0, 100.0, true, t.cpu_temp_warn, "CPU-temperatur, gul grense"), field("cpuTempCrit", 40.0, 110.0, true, t.cpu_temp_crit, "CPU-temperatur, rød grense"), "cpu prosessor"),
            thr("Last (GPU og CPU)", field("loadWarn", 50.0, 100.0, false, t.load_warn, "Last, gul grense"), none(), "last"),
        ],
    )
}
