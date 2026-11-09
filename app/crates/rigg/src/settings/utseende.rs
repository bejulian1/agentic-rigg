//! Utseende: lyst eller mørkt, fargetema (miniatyrer tegnet med temaets
//! egne tokens), aksentfarge, appikon, høy kontrast, tetthet, skrift og
//! bevegelse.
//!
//! Appikonet følger `docs/produksjon/ikon.md` kap. 7–8: hver variant har sitt
//! eget ikonnavn, og byttet skriver om `Icon=` i den installerte `.desktop`-fila
//! atomisk. Ikonfila skrives aldri over (GNOME Shell bufrer per navn). Dokken
//! bytter etter ca. 5 s. Forhåndsvisningene leses fra GResource, så de vises
//! også før appen er installert.

use super::{Group, GroupOpts, group, row, row_wrap, save, seg, switch_row};
use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{Icon, hbox, label, vbox};
use gtk::prelude::*;
use rigg_core::tokens::{Env, color_or};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PALETTES: [(&str, &str, &str); 4] = [("grafitt", "Grafitt", "Nøytral grå"), ("sand", "Sand", "Varm, som papir"), ("fjord", "Fjord", "Kjølig blågrå"), ("midnatt", "Midnatt", "Dypt mørkt, skarpt lyst")];
const ACCENTS: [(&str, &str); 6] = [("blue", "Blå"), ("teal", "Blågrønn"), ("green", "Grønn"), ("purple", "Lilla"), ("pink", "Rosa"), ("slate", "Skifer")];

thread_local! {
    static PREVIEWS: RefCell<Vec<gtk::glib::WeakRef<gtk::DrawingArea>>> = const { RefCell::new(Vec::new()) };
    static HOOKED: Cell<bool> = const { Cell::new(false) };
}

/// Miniatyren av et fargetema: sidefelt, toppflate og et kort med tittel,
/// tekst og en stolpe i aksenten (demoens `.pv`).
fn preview(app: &Rc<App>, palette: &'static str) -> gtk::DrawingArea {
    let d = gtk::DrawingArea::new();
    d.set_content_height(68);
    d.set_hexpand(true);
    let a = app.clone();
    d.set_draw_func(move |_, cr, w, h| {
        let cur = crate::theme::env(&a);
        let env = Env::new(cur.dark, palette, cur.high_contrast, &cur.accent);
        let c = |n: &str| color_or(&env, n);
        let set = |cr: &gtk::cairo::Context, col: [f64; 4], alpha: f64| cr.set_source_rgba(col[0] / 255.0, col[1] / 255.0, col[2] / 255.0, col[3] * alpha);
        let rr = |cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64| {
            cr.new_sub_path();
            cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
            cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
            cr.close_path();
        };
        let (w, h) = (w as f64, h as f64);
        rr(cr, 0.5, 0.5, w - 1.0, h - 1.0, 4.0);
        cr.save().ok();
        cr.clip_preserve();
        set(cr, c("--bg"), 1.0);
        cr.fill().ok();
        let sw = (w * 0.3).round();
        set(cr, c("--sidebar"), 1.0);
        cr.rectangle(0.0, 0.0, sw, h);
        cr.fill().ok();
        set(cr, c("--border"), 1.0);
        cr.rectangle(sw, 0.0, 1.0, h);
        cr.fill().ok();
        for (i, (alpha, tok)) in [(0.85, "--text"), (0.55, "--muted"), (0.55, "--muted")].iter().enumerate() {
            set(cr, c(tok), *alpha);
            rr(cr, 7.0, 9.0 + i as f64 * 9.0, sw - 14.0, 4.0, 2.0);
            cr.fill().ok();
        }
        let (cx, cy, cw, ch) = (sw + 9.0, 9.0, w - sw - 18.0, h - 18.0);
        set(cr, c("--card"), 1.0);
        rr(cr, cx, cy, cw, ch, 4.0);
        cr.fill_preserve().ok();
        set(cr, c("--border"), 1.0);
        cr.set_line_width(1.0);
        cr.stroke().ok();
        set(cr, c("--text"), 1.0);
        rr(cr, cx + 7.0, cy + 7.0, cw * 0.55, 4.0, 2.0);
        cr.fill().ok();
        set(cr, c("--muted"), 0.7);
        rr(cr, cx + 7.0, cy + 16.0, cw * 0.75, 4.0, 2.0);
        cr.fill().ok();
        set(cr, c("--accent-fill"), 1.0);
        rr(cr, cx + 7.0, cy + ch - 11.0, cw * 0.4, 4.0, 2.0);
        cr.fill().ok();
        cr.restore().ok();
        rr(cr, 0.5, 0.5, w - 1.0, h - 1.0, 4.0);
        set(cr, c("--border"), 1.0);
        cr.set_line_width(1.0);
        cr.stroke().ok();
    });
    PREVIEWS.with(|p| p.borrow_mut().push(d.downgrade()));
    if !HOOKED.with(|h| h.replace(true)) {
        app.on_theme.connect(|_| {
            PREVIEWS.with(|p| {
                p.borrow_mut().retain(|w| match w.upgrade() {
                    Some(d) => {
                        d.queue_draw();
                        true
                    }
                    None => false,
                })
            });
        });
    }
    d
}

fn check_badge() -> gtk::Box {
    let b = hbox(0, &["pv-check"]);
    let i = Icon::new("check", 12);
    i.set_stroke(2.75);
    b.append(&i);
    b.set_halign(gtk::Align::End);
    b.set_valign(gtk::Align::Start);
    b.set_can_target(false);
    b
}

fn palettes(app: &Rc<App>) -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    grid.set_column_homogeneous(true);
    grid.set_accessible_role(gtk::AccessibleRole::RadioGroup);
    grid.update_property(&[gtk::accessible::Property::Label("Fargetema")]);
    let cur = app.settings().palette.clone();
    let mut first: Option<gtk::ToggleButton> = None;
    for (i, (id, name, desc)) in PALETTES.iter().enumerate() {
        let b = gtk::ToggleButton::new();
        b.add_css_class("pal-card");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let col = vbox(2, &[]);
        let frame = gtk::Overlay::new();
        let pv = vbox(0, &["pv-frame"]);
        pv.set_margin_top(5);
        pv.set_margin_end(5);
        pv.append(&preview(app, id));
        frame.set_child(Some(&pv));
        let badge = check_badge();
        frame.add_overlay(&badge);
        col.append(&frame);
        let n = label(name, &["pal-name"]);
        col.append(&n);
        col.append(&label(desc, &["pal-desc"]));
        b.set_child(Some(&col));
        b.set_active(*id == cur);
        badge.set_visible(*id == cur);
        b.update_property(&[gtk::accessible::Property::Label(&format!("{name}: {desc}"))]);
        let a = app.clone();
        let idc = *id;
        b.connect_toggled(move |b| {
            badge.set_visible(b.is_active());
            if b.is_active() {
                save(&a, json!({"palette": idc}));
            }
        });
        grid.attach(&b, i as i32, 0, 1, 1);
    }
    grid
}

fn accents(app: &Rc<App>) -> gtk::Box {
    let bx = hbox(4, &[]);
    bx.set_homogeneous(true);
    bx.set_accessible_role(gtk::AccessibleRole::RadioGroup);
    bx.update_property(&[gtk::accessible::Property::Label("Aksentfarge")]);
    let cur = app.settings().accent.clone();
    let mut first: Option<gtk::ToggleButton> = None;
    for (id, name) in ACCENTS {
        let b = gtk::ToggleButton::new();
        b.add_css_class("swatch");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let col = vbox(4, &[]);
        let sw = hbox(0, &["sw", &format!("acc-{id}")]);
        sw.set_halign(gtk::Align::Center);
        let tick = Icon::new("check", 14);
        tick.set_stroke(2.5);
        tick.set_hexpand(true);
        tick.set_opacity(if id == cur { 1.0 } else { 0.0 });
        sw.append(&tick);
        col.append(&sw);
        let n = label(name, &["sw-name"]);
        n.set_xalign(0.5);
        col.append(&n);
        b.set_child(Some(&col));
        b.set_active(id == cur);
        b.update_property(&[gtk::accessible::Property::Label(name)]);
        let a = app.clone();
        b.connect_toggled(move |b| {
            tick.set_opacity(if b.is_active() { 1.0 } else { 0.0 });
            if b.is_active() {
                save(&a, json!({"accent": id}));
            }
        });
        bx.append(&b);
    }
    bx
}

/// Variantene fra `varianter.json` (id, navn, beskrivelse, ikonnavn).
pub fn variants() -> Vec<(String, String, String, String)> {
    let Ok(bytes) = gtk::gio::resources_lookup_data("/io/github/AgenticRigg/icons/varianter/varianter.json", gtk::gio::ResourceLookupFlags::NONE) else { return vec![] };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return vec![] };
    v["varianter"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| {
                    let s = |k: &str| x[k].as_str().unwrap_or("").to_string();
                    (s("id"), s("navn"), s("beskrivelse"), s("ikonnavn"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn icon_grid(app: &Rc<App>) -> gtk::FlowBox {
    let fb = gtk::FlowBox::new();
    fb.add_css_class("icon-grid");
    fb.set_selection_mode(gtk::SelectionMode::Single);
    fb.set_homogeneous(true);
    fb.set_min_children_per_line(5);
    fb.set_max_children_per_line(10);
    fb.set_column_spacing(12);
    fb.set_row_spacing(8);
    fb.set_activate_on_single_click(true);
    fb.update_property(&[gtk::accessible::Property::Label("Appikon")]);
    let cur = app.settings().app_icon.clone();
    let mut sel: Option<gtk::FlowBoxChild> = None;
    for (id, name, desc, _icon) in variants() {
        let col = vbox(0, &[]);
        let frame = gtk::Overlay::new();
        let tile = hbox(0, &["ic-tile"]);
        tile.set_margin_top(5);
        tile.set_margin_end(5);
        let pic = gtk::Picture::for_resource(&format!("/io/github/AgenticRigg/icons/varianter/{id}/48.svg"));
        pic.set_can_shrink(false);
        pic.set_halign(gtk::Align::Center);
        pic.set_valign(gtk::Align::Center);
        pic.set_hexpand(true);
        tile.append(&pic);
        frame.set_child(Some(&tile));
        let badge = check_badge();
        badge.set_visible(id == cur);
        frame.add_overlay(&badge);
        col.append(&frame);
        let n = label(&name, &["ic-name"]);
        n.set_xalign(0.5);
        col.append(&n);
        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&col));
        child.set_widget_name(&id);
        child.set_tooltip_text(Some(&desc));
        child.update_property(&[gtk::accessible::Property::Label(&format!("{name}: {desc}"))]);
        fb.insert(&child, -1);
        if id == cur {
            sel = Some(child);
        }
    }
    if let Some(c) = sel {
        fb.select_child(&c);
    }
    let a = app.clone();
    fb.connect_selected_children_changed(move |fb| {
        let Some(child) = fb.selected_children().into_iter().next() else { return };
        let id = child.widget_name().to_string();
        // Haken følger valget.
        let mut c = fb.first_child();
        while let Some(w) = c {
            c = w.next_sibling();
            let on = w.widget_name() == id;
            if let Some(badge) = w.first_child().and_then(|col| col.first_child()).and_then(|ov| ov.last_child()) {
                badge.set_visible(on);
            }
        }
        if a.settings().app_icon != id && save(&a, json!({"appIcon": id})) {
            apply_app_icon(&a, &id);
        }
    });
    fb
}

/// Bytter appikonet: ikonfilene finnes (eller skrives fra GResource), så
/// skrives `Icon=` om i `.desktop`-fila atomisk, og vinduet får det nye navnet.
pub fn apply_app_icon(app: &Rc<App>, id: &str) {
    let Some((_, name, _, icon)) = variants().into_iter().find(|v| v.0 == id) else { return };
    gtk::Window::set_default_icon_name(&icon);
    if let Some(w) = app.window() {
        w.set_icon_name(Some(&icon));
    }
    let desktop = rigg_core::paths::desktop_file(app.profile);
    let Ok(text) = std::fs::read_to_string(&desktop) else {
        app.toast(Toast::new(&format!("Appikonet er «{name}»")).body("Appen er ikke installert, så bare vinduet har fått det nye ikonet. Valget brukes når appen installeres."));
        return;
    };
    // 1. Ikonfilene for varianten (skrives bare når de mangler).
    let hicolor = rigg_core::paths::hicolor_dir();
    let mut wrote = false;
    for (size, dir) in [("128", "scalable"), ("48", "48x48"), ("32", "32x32"), ("24", "24x24"), ("16", "16x16")] {
        let dest = hicolor.join(dir).join("apps").join(format!("{icon}.svg"));
        if dest.exists() {
            continue;
        }
        if let Ok(b) = gtk::gio::resources_lookup_data(&format!("/io/github/AgenticRigg/icons/varianter/{id}/{size}.svg"), gtk::gio::ResourceLookupFlags::NONE)
            && rigg_core::write_atomic(&dest, &b).is_ok()
        {
            wrote = true;
        }
    }
    if wrote {
        touch(&hicolor);
    }
    // 2. Ny Icon= i .desktop-fila, resten likt.
    let mut out = String::new();
    let mut in_entry = false;
    for line in text.lines() {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        }
        if in_entry && line.starts_with("Icon=") {
            out.push_str(&format!("Icon={icon}"));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    match rigg_core::write_atomic(&desktop, out.as_bytes()) {
        Ok(()) => app.toast(Toast::new(&format!("Appikonet er «{name}»")).body("Ikonet byttes i dokken og Vis programmer om noen sekunder.")),
        Err(e) => app.toast(Toast::new("Kunne ikke bytte appikonet").body(&e.to_string()).error()),
    }
}

/// Setter endringstiden på en mappe til nå (så GTK og Shell leser temaet på nytt).
fn touch(dir: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return };
    // SAFETY: utimensat med NULL setter tidene til nå; stien er en gyldig C-streng.
    unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), std::ptr::null(), 0) };
}

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    let sys = match crate::motion::system() {
        crate::motion::Level::Full => "full",
        _ => "redusert",
    };
    let a = app.clone();
    vec![
        group(
            "Farger",
            GroupOpts::default(),
            vec![
                row("Lyst eller mørkt", Some("System følger GNOME og bytter med en gang."), &seg(app, &[("system", "System"), ("light", "Lys"), ("dark", "Mørk")], &s.theme, "theme", "Lyst eller mørkt", false).root, "tema mørk modus lys dark light"),
                row_wrap("Fargetema", Some("Tonen i flater, rammer og tekst, i både lyst og mørkt."), &palettes(app), "palett grafitt sand fjord midnatt"),
                row_wrap("Aksentfarge", Some("Knapper, valgt side, stolper og glidere. Grafene har faste farger per serie."), &accents(app), "blå blågrønn grønn lilla rosa skifer"),
                row_wrap("Appikon", Some("Ikonet i Vis programmer, dokken og Alt+Tab. Byttet vises etter noen sekunder, uten å logge ut."), &icon_grid(app), "ikon appikon dokk vis programmer farge"),
                switch_row("Høy kontrast", Some("Sterkere tekst, rammer, fokusringer og grafer i alle fargetemaer."), s.high_contrast, "tilgjengelighet synlighet", move |on| {
                    save(&a, json!({"highContrast": on}));
                }),
            ],
        ),
        group(
            "Størrelse",
            GroupOpts::default(),
            vec![
                row("Tetthet", Some("Høyden på rader og luften mellom panelene."), &seg(app, &[("compact", "Kompakt"), ("standard", "Standard"), ("comfortable", "Luftig")], &s.density, "density", "Tetthet", false).root, "kompakt luftig tett"),
                row(
                    "Skriftstørrelse",
                    Some("Teksten i hele appen. Også Ctrl og + eller −, og Ctrl+0."),
                    &seg(app, &[("92", "Liten"), ("100", "Standard"), ("108", "Stor"), ("116", "Større")], &s.font_scale.to_string(), "fontScale", "Skriftstørrelse", true).root,
                    "tekst zoom skrift",
                ),
            ],
        ),
        group(
            "Lys",
            GroupOpts::default(),
            vec![row(
                "3D-kortet",
                Some("Auto viser 2D til du har valgt 3D én gang, deretter det du valgte sist. 3D bruker ca. 120 MB mer minne og vekker skjermkortet i noen sekunder første gang."),
                &seg(app, &[("auto", "Auto"), ("3d", "Alltid 3D"), ("2d", "Alltid 2D")], &s.lys3d, "lys3d", "3D-kortet på RGB-lys", false).root,
                "lys rgb 3d 2d forhåndsvisning kort",
            )],
        ),
        group(
            "Bevegelse",
            GroupOpts { note: Some("Redusert gir bare korte toninger, og grafene oppdateres i trinn. Av slår av all animasjon."), ..Default::default() },
            vec![row(
                "Animasjoner",
                Some(&format!("Automatisk følger systemets innstilling (nå: {sys}).")),
                &seg(app, &[("system", "Automatisk"), ("full", "Full"), ("reduced", "Redusert"), ("off", "Av")], &s.motion, "motion", "Animasjoner", false).root,
                "bevegelse animasjon redusert",
            )],
        ),
    ]
}
