//! «Styrer»-linja (runde 8A): kortet Styring-sidene gjelder, funnet av seg
//! selv. Står øverst på Klokker og effekt, Vifter og RGB-lys, rolig og uten
//! ramme: GPU-ikon, «Styrer», navnet på kortet, og produsent, PCI-adresse,
//! VBIOS og driver. Med flere kort kan kortet byttes her (som i
//! maskinhodet), og produsenten kan settes for hånd når kortet ikke melder
//! den (`Settings::gpu_makers`, `rigg_core::vendor`).
//!
//! Kortet borte, ingen driver eller ny driver: linja sier det i klartekst, og
//! at styringen ikke er tilgjengelig.

use crate::state::App;
use crate::widgets::{Icon, class, hbox, label, set_text, vbox};
use gtk::prelude::*;
use rigg_core::model::{DeviceState, GpuStatic};
use rigg_core::{tr, trf};
use rigg_core::vendor;
use serde_json::json;
use std::rc::Rc;

/// Nøkkelen produsenten lagres under: UUID, ellers PCI-adressen.
pub fn maker_key(st: &GpuStatic) -> String {
    st.uuid.clone().filter(|u| !u.is_empty()).unwrap_or_else(|| st.id.clone())
}

/// Produsenten for kortet: det brukeren har satt, ellers det kortet melder.
/// (tekst, «bruker»/«kort»/«»).
pub fn maker(app: &App, st: &GpuStatic) -> (String, &'static str) {
    let user = app.settings().gpu_makers.get(&maker_key(st)).cloned();
    let (name, source) = vendor::maker_text(user.as_deref(), st.pci_subsystem_id);
    let name = if source == "kort" {
        match vendor::board_maker(st.pci_subsystem_id) {
            vendor::Maker::Reference => tr!("NVIDIA-referansekort").into(),
            vendor::Maker::Other(id) => trf!("PCI-leverandør {id}", id = format!("{id:04x}")),
            _ => name,
        }
    } else {
        name
    };
    (name, source)
}

/// Produsenten når den er kjent (satt av brukeren eller en kjent
/// kortprodusent), til maskinhodet. Ikke «NVIDIA-referansekort».
pub fn maker_short(app: &App, st: &GpuStatic) -> Option<String> {
    let (t, src) = maker(app, st);
    (src == "bruker" || matches!(vendor::board_maker(st.pci_subsystem_id), vendor::Maker::Known(_))).then_some(t).filter(|t| !t.is_empty())
}

/// Hvorfor produsenten står som den gjør (verktøytipset).
fn maker_tip(st: &GpuStatic, src: &str) -> String {
    let sub = st.pci_subsystem_id.map(vendor::subsystem_text);
    match (src, vendor::board_maker(st.pci_subsystem_id), sub) {
        ("bruker", vendor::Maker::Reference, Some(s)) => trf!("Produsenten er satt av deg. Kortet melder NVIDIAs referanse i PCI-subsystemet ({s}).", s = s),
        ("bruker", _, _) => tr!("Produsenten er satt av deg.").into(),
        (_, vendor::Maker::Reference, Some(s)) => trf!("Kortet melder NVIDIAs referanse i PCI-subsystemet ({s}). Det er et referansekort, eller en produsent som ikke setter sin egen ID. Du kan sette produsenten selv.", s = s),
        (_, vendor::Maker::Known(_), Some(s)) => trf!("Fra PCI-subsystemet ({s}).", s = s),
        _ => tr!("Produsenten er ikke lest.").into(),
    }
}

/// Linja øverst på Styring-sidene.
pub struct TargetLine {
    pub root: gtk::Box,
    icon: Icon,
    name: gtk::Label,
    meta: gtk::Label,
    maker_btn: gtk::Button,
    pick: gtk::Button,
    pick_label: gtk::Label,
    app: Rc<App>,
}

impl TargetLine {
    pub fn new(app: &Rc<App>) -> Rc<TargetLine> {
        let root = hbox(8, &["target-line"]);
        root.set_accessible_role(gtk::AccessibleRole::Note);
        let icon = Icon::new("gpu", 16);
        icon.add_css_class("tl-icon");
        icon.set_valign(gtk::Align::Start);
        icon.set_margin_top(3);
        root.append(&icon);
        let lead = label(tr!("Styrer"), &["tl-lead"]);
        lead.set_valign(gtk::Align::Baseline);
        root.append(&lead);
        let text = hbox(8, &[]);
        root.set_baseline_position(gtk::BaselinePosition::Top);
        text.set_baseline_position(gtk::BaselinePosition::Top);
        text.set_valign(gtk::Align::Baseline);
        text.set_hexpand(true);
        let name = label("", &["tl-name"]);
        name.set_valign(gtk::Align::Baseline);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let meta = label("", &["tl-meta", "num"]);
        meta.set_valign(gtk::Align::Baseline);
        // Brytes heller enn å kuttes i smale vinduer (640 px).
        meta.set_wrap(true);
        meta.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        meta.set_hexpand(true);
        meta.set_xalign(0.0);
        text.append(&name);
        text.append(&meta);
        root.append(&text);
        let maker_btn = crate::widgets::button(Some("pencil"), None, &["flat", "sm", "icon-only", "tl-btn"]);
        maker_btn.set_valign(gtk::Align::Center);
        crate::widgets::tip(&maker_btn, tr!("Sett produsenten"), None);
        maker_btn.update_property(&[gtk::accessible::Property::Label(tr!("Sett produsenten"))]);
        root.append(&maker_btn);
        let pick = gtk::Button::new();
        pick.add_css_class("flat");
        pick.add_css_class("sm");
        pick.add_css_class("tl-btn");
        pick.set_valign(gtk::Align::Center);
        let pb = hbox(4, &[]);
        let pick_label = label("", &["num"]);
        pb.append(&pick_label);
        pb.append(&Icon::new("chevron-down", 14));
        pick.set_child(Some(&pb));
        crate::widgets::tip(&pick, tr!("Velg kortet som styres"), None);
        pick.set_visible(false);
        root.append(&pick);
        let t = Rc::new(TargetLine { root, icon, name, meta, maker_btn, pick, pick_label, app: app.clone() });
        let w = Rc::downgrade(&t);
        t.maker_btn.connect_clicked(move |b| {
            if let Some(t) = w.upgrade() {
                t.edit_maker(b);
            }
        });
        let w = Rc::downgrade(&t);
        t.pick.connect_clicked(move |b| {
            if let Some(t) = w.upgrade() {
                t.show_cards(b);
            }
        });
        let w = Rc::downgrade(&t);
        app.on_devices.connect(move |_| {
            if let Some(t) = w.upgrade() {
                t.render();
            }
        });
        let w = Rc::downgrade(&t);
        app.on_conn.connect(move |_| {
            if let Some(t) = w.upgrade() {
                t.render();
            }
        });
        let w = Rc::downgrade(&t);
        app.on_settings.connect(move |k| {
            if k.iter().any(|k| k == "gpuMakers")
                && let Some(t) = w.upgrade()
            {
                t.render();
            }
        });
        t.render();
        // Linja lever like lenge som flaten (hubene holder bare svake referanser).
        let keep = t.clone();
        t.root.connect_destroy(move |_| {
            let _ = &keep;
        });
        t
    }

    fn statics(&self) -> Option<GpuStatic> {
        self.app.statics().or_else(|| {
            let id = self.app.gpu_info()?.id;
            self.app.data.borrow().statics.get(&id).map(|s| s.0.clone())
        })
    }

    pub fn render(&self) {
        let app = &self.app;
        let info = app.gpu_info();
        let st = self.statics();
        let cards: Vec<_> = app.data.borrow().gpus.iter().filter(|g| !g.id.is_empty()).cloned().collect();
        let full_name = st.as_ref().map(|s| s.name.clone()).filter(|n| !n.is_empty()).or_else(|| info.as_ref().map(|i| i.name.clone()).filter(|n| !n.is_empty()));
        let (kind, name, meta, tip) = match (app.client.is_connected(), info.as_ref().map(|i| i.state)) {
            (false, _) => ("", tr!("Venter på tjenesten").to_string(), tr!("kortet vises når tjenesten svarer").to_string(), String::new()),
            (true, None) => ("", tr!("Leser skjermkortet …").to_string(), String::new(), String::new()),
            (true, Some(DeviceState::NoDriver)) => ("warn", tr!("Fant ingen NVIDIA-driver").to_string(), tr!("ingen skjermkort å styre").to_string(), info.as_ref().and_then(|i| i.detail.clone()).unwrap_or_default()),
            (true, Some(DeviceState::DriverUpdated)) => (
                "warn",
                full_name.clone().unwrap_or_else(|| tr!("Skjermkortet").into()),
                tr!("driveren er oppdatert · start maskinen på nytt for å styre kortet").to_string(),
                info.as_ref().and_then(|i| i.detail.clone()).unwrap_or_default(),
            ),
            (true, Some(DeviceState::Lost)) => {
                let since = info.as_ref().map(|i| rigg_core::fmt::since(crate::app::local_time(i.since), crate::app::local_now())).unwrap_or_default();
                (
                    "danger",
                    full_name.clone().unwrap_or_else(|| tr!("Skjermkortet").into()),
                    trf!("svarer ikke siden {since} · styringen er utilgjengelig", since = since),
                    info.as_ref().and_then(|i| i.detail.clone()).unwrap_or_default(),
                )
            }
            (true, Some(_)) => {
                let st = st.clone().unwrap_or_default();
                let mut parts: Vec<String> = Vec::new();
                let (m, src) = maker(app, &st);
                if !m.is_empty() {
                    parts.push(m);
                }
                parts.push(st.id.clone());
                if let Some(v) = st.vbios.as_ref() {
                    parts.push(format!("VBIOS {v}"));
                }
                if let Some(d) = st.driver.clone().or_else(|| info.as_ref().and_then(|i| i.driver.clone())) {
                    parts.push(trf!("driver {d}", d = d));
                }
                let tip = trf!(
                    "{kort}\nProdusent: {produsent}\nPCI-adresse: {pci}\nVBIOS: {vbios}\n{detalj}",
                    kort = full_name.clone().unwrap_or_default(),
                    produsent = maker(app, &st).0,
                    pci = st.id,
                    vbios = st.vbios.clone().unwrap_or_else(|| "–".into()),
                    detalj = maker_tip(&st, src),
                );
                ("", full_name.clone().unwrap_or_else(|| tr!("Skjermkort").into()), parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · "), tip)
            }
        };
        set_text(&self.name, &name);
        set_text(&self.meta, &meta);
        for k in ["warn", "danger"] {
            class(&self.root, k, k == kind);
        }
        self.icon.set_icon(if kind.is_empty() { "gpu" } else { "triangle-alert" });
        if self.root.tooltip_text().map(|t| t.to_string()).unwrap_or_default() != tip {
            self.root.set_tooltip_text((!tip.is_empty()).then_some(tip.as_str()));
        }
        let ok = kind.is_empty() && st.is_some() && app.client.is_connected();
        self.maker_btn.set_visible(ok);
        // Flere kort: «Kort 1 av 2» med valget.
        let many = cards.len() > 1;
        self.pick.set_visible(many);
        if many {
            let i = info.as_ref().map(|i| i.index as usize + 1).unwrap_or(1);
            set_text(&self.pick_label, &trf!("Kort {i} av {antall}", i = i, antall = cards.len()));
        }
        self.root.update_property(&[gtk::accessible::Property::Label(&trf!("Styrer: {name}. {meta}", name = name, meta = meta))]);
    }

    /// Kortene, med det som styres merket. Som valget i maskinhodet.
    fn show_cards(&self, anchor: &gtk::Button) {
        let body = vbox(0, &[]);
        body.append(&label(tr!("Kortet som styres"), &["menu-label"]));
        let cur = self.app.gpu_id();
        let cards: Vec<_> = self.app.data.borrow().gpus.iter().filter(|g| !g.id.is_empty()).cloned().collect();
        for g in cards {
            let b = gtk::Button::new();
            b.add_css_class("flat");
            b.add_css_class("menu-item");
            let bx = hbox(10, &[]);
            let ic = Icon::new(if Some(&g.id) == cur.as_ref() { "check" } else { "gpu" }, 14);
            bx.append(&ic);
            let t = vbox(0, &[]);
            let n = label(&rigg_core::model::short_gpu_name(&g.name), &[]);
            n.set_xalign(0.0);
            let s = label(&g.id, &["muted", "sm", "num"]);
            s.set_xalign(0.0);
            t.append(&n);
            t.append(&s);
            bx.append(&t);
            b.set_child(Some(&bx));
            let (a, id) = (self.app.clone(), g.id.clone());
            b.connect_clicked(move |b| {
                a.select_gpu(&id);
                if let Some(p) = b.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>() {
                    p.popdown();
                }
            });
            body.append(&b);
        }
        let pop = crate::widgets::popover::popover(anchor, &body);
        pop.add_css_class("pop-wide");
        pop.popup();
    }

    /// «Produsent (valgfritt)»: lagres per kort, vises her og i maskinhodet.
    fn edit_maker(&self, anchor: &gtk::Button) {
        let Some(st) = self.statics() else { return };
        let key = maker_key(&st);
        let current = self.app.settings().gpu_makers.get(&key).cloned().unwrap_or_default();
        let detected = vendor::maker_text(None, st.pci_subsystem_id).0;
        let body = vbox(6, &["tl-maker"]);
        body.append(&label(tr!("Produsent (valgfritt)"), &["pop-title"]));
        let why = label(&maker_tip(&st, if current.is_empty() { "kort" } else { "bruker" }), &["pop-sub"]);
        why.set_wrap(true);
        why.set_max_width_chars(42);
        why.set_xalign(0.0);
        body.append(&why);
        let entry = gtk::Entry::new();
        entry.set_text(&current);
        entry.set_placeholder_text(Some(if matches!(vendor::board_maker(st.pci_subsystem_id), vendor::Maker::Known(_)) { &detected } else { "Palit, MSI, ASUS …" }));
        entry.set_max_length(40);
        entry.set_margin_start(8);
        entry.set_margin_end(8);
        entry.update_property(&[gtk::accessible::Property::Label(tr!("Produsent"))]);
        body.append(&entry);
        let note = label(tr!("Vises bare i appen, i Styrer-linja og i maskinhodet. Kortet endres ikke."), &["pop-sub"]);
        note.set_wrap(true);
        note.set_max_width_chars(42);
        note.set_xalign(0.0);
        body.append(&note);
        let row = hbox(6, &[]);
        row.set_halign(gtk::Align::End);
        row.set_margin_end(8);
        row.set_margin_bottom(4);
        let clear = crate::widgets::button(None, Some(tr!("Fjern")), &["sm", "flat"]);
        clear.set_sensitive(!current.is_empty());
        let save = crate::widgets::button(None, Some(tr!("Lagre")), &["sm", "suggested"]);
        row.append(&clear);
        row.append(&save);
        body.append(&row);
        let pop = crate::widgets::popover::popover(anchor, &body);
        pop.add_css_class("pop-wide");
        let store = {
            let (a, key) = (self.app.clone(), key.clone());
            move |name: &str| {
                let mut m = a.settings().gpu_makers.clone();
                if name.trim().is_empty() {
                    m.remove(&key);
                } else {
                    m.insert(key.clone(), name.trim().to_string());
                }
                a.set(json!({ "gpuMakers": m }));
            }
        };
        {
            let (e, p, store) = (entry.clone(), pop.clone(), store.clone());
            save.connect_clicked(move |_| {
                store(&e.text());
                p.popdown();
            });
        }
        {
            let (p, store) = (pop.clone(), store.clone());
            clear.connect_clicked(move |_| {
                store("");
                p.popdown();
            });
        }
        {
            let p = pop.clone();
            entry.connect_activate(move |e| {
                store(&e.text());
                p.popdown();
            });
        }
        pop.popup();
        entry.grab_focus();
    }
}
