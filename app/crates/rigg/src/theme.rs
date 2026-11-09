//! Temaene i GTK-CSS (arkitektur.md 6.3). Valgene settes som klasser på
//! vinduet, slik demoen setter `data-*` på `<html>`: `.dark`, `.pal-sand`,
//! `.hc`, `.acc-teal`, `.tett-kompakt`, `.skrift-108`. Alle 96 kombinasjonene
//! sjekkes for kontrast i `rigg-core` (tokens.rs).
//!
//! «System» følger `gtk-interface-color-scheme` og bytter med en gang.
//! Verktøytipsene er egne vinduer uten vinduets variabler; de får en liten
//! stil med ferdig utregnede farger for temaet som vises.

use crate::state::App;
use gtk::prelude::*;
use rigg_core::tokens::{Env, color_or};
use std::cell::RefCell;
use std::rc::Rc;

/// Innstillingene som endrer tema eller størrelse.
pub const THEME_KEYS: &[&str] = &["theme", "accent", "palette", "highContrast", "density", "fontScale", "motion"];

const SHEETS: &[&str] = &[
    "tokens", "base", "skall", "innstillinger", "oversikt", "sensorer", "grafer", "grafikkort", "system", "prosessor", "disker", "kontroller", "viftekurve", "lys",
];

/// Skallets regler som må vinne over sidenes egne ark. Arkene har samme
/// prioritet, og da vinner det som er lastet sist, uansett spesifisitet; disse
/// står derfor i den dynamiske stilen (prioritet APPLICATION + 1).
///
/// Tidsvinduet («1 min») i toppfeltet var 22 px med liten skrift mot 28 px og
/// vanlig skrift på knappene ved siden av (runde 6B).
const OVER_PAGES: &str = "window.rigg .head-actions dropdown.win-select > button { min-height: var(--h-md); padding: 0 var(--sp-2) 0 var(--sp-3); font-size: var(--fs-body); }";

thread_local! {
    static DYNAMIC: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static LAST: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Laster stilarkene fra GResource (én gang per prosess).
pub fn load(display: &gtk::gdk::Display) {
    for name in SHEETS {
        let p = gtk::CssProvider::new();
        let n = name.to_string();
        p.connect_parsing_error(move |_, section, err| {
            eprintln!("CSS ({n}.css {}): {err}", section.to_str());
        });
        p.load_from_resource(&format!("/io/github/AgenticRigg/theme/{name}.css"));
        gtk::style_context_add_provider_for_display(display, &p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let dynamic = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(display, &dynamic, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
    DYNAMIC.with(|d| *d.borrow_mut() = Some(dynamic));
    let theme = gtk::IconTheme::for_display(display);
    theme.add_resource_path("/io/github/AgenticRigg/icons");
}

/// Er systemet i mørkt?
pub fn system_dark() -> bool {
    gtk::Settings::default().is_some_and(|s| s.gtk_interface_color_scheme() == gtk::InterfaceColorScheme::Dark)
}

pub fn resolved_dark(app: &App) -> bool {
    match app.settings().theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    }
}

/// Kombinasjonen som vises nå.
pub fn env(app: &App) -> Env {
    let s = app.settings();
    Env::new(resolved_dark(app), &s.palette, s.high_contrast, &s.accent)
}

thread_local! {
    static FONT_SCALE: std::cell::Cell<u32> = const { std::cell::Cell::new(100) };
}

/// Skriftstørrelsen i prosent (100 = standard), for faste kolonnebredder.
pub fn font_scale() -> f64 {
    FONT_SCALE.with(|f| f.get()) as f64 / 100.0
}

/// Klassene for vinduet (uten «rigg»).
pub fn classes(app: &App) -> Vec<String> {
    let mut c = env(app).classes();
    let s = app.settings();
    FONT_SCALE.with(|f| f.set(s.font_scale));
    match s.density.as_str() {
        "compact" => c.push("tett-kompakt".into()),
        "comfortable" => c.push("tett-luftig".into()),
        _ => {}
    }
    if s.font_scale != 100 {
        c.push(format!("skrift-{}", s.font_scale));
    }
    c
}

const OURS: &[&str] = &["dark", "hc", "tett-kompakt", "tett-luftig", "skrift-92", "skrift-108", "skrift-116"];

fn is_ours(c: &str) -> bool {
    OURS.contains(&c) || c.starts_with("pal-") || c.starts_with("acc-")
}

/// Setter klassene på et vindu (hovedvinduet og egne dialoger).
pub fn apply_to(app: &App, w: &impl IsA<gtk::Widget>) {
    let want = classes(app);
    w.add_css_class("rigg");
    for c in w.css_classes() {
        if is_ours(c.as_str()) && !want.iter().any(|x| x == c.as_str()) {
            w.remove_css_class(c.as_str());
        }
    }
    for c in &want {
        w.add_css_class(c);
    }
}

fn hex(c: [f64; 4]) -> String {
    if c[3] >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", c[0].round() as u8, c[1].round() as u8, c[2].round() as u8)
    } else {
        format!("rgba({}, {}, {}, {:.3})", c[0].round(), c[1].round(), c[2].round(), c[3])
    }
}

/// Fargen til et token for temaet som vises (til tegning i Rust).
pub fn rgba(app: &App, token: &str) -> gtk::gdk::RGBA {
    let c = color_or(&env(app), token);
    gtk::gdk::RGBA::new((c[0] / 255.0) as f32, (c[1] / 255.0) as f32, (c[2] / 255.0) as f32, c[3] as f32)
}

/// Tar i bruk innstillingene: klassene på vinduet, verktøytipsene,
/// GTKs egne lyst/mørkt og animasjoner, og sier fra til sidene.
pub fn apply(app: &Rc<App>) {
    if let Some(w) = app.window() {
        apply_to(app, &w);
    }
    for w in gtk::Window::list_toplevels() {
        if let Ok(w) = w.downcast::<gtk::Window>()
            && w.has_css_class("rigg")
        {
            apply_to(app, &w);
        }
    }
    let e = env(app);
    let fs = (app.settings().font_scale as f64 / 100.0) * 12.0;
    let css = format!(
        "tooltip {{ background-color: {}; color: {}; border-radius: 4px; padding: 5px 8px; font-size: {fs:.2}px; box-shadow: none; border: none; text-shadow: none; }} tooltip label {{ color: {}; }} {OVER_PAGES}",
        hex(color_or(&e, "--tip-bg")),
        hex(color_or(&e, "--tip-fg")),
        hex(color_or(&e, "--tip-fg")),
    );
    let changed = LAST.with(|l| {
        if *l.borrow() == css {
            false
        } else {
            *l.borrow_mut() = css.clone();
            true
        }
    });
    if changed {
        DYNAMIC.with(|d| {
            if let Some(p) = d.borrow().as_ref() {
                p.load_from_string(&css);
            }
        });
    }
    if let Some(s) = gtk::Settings::default() {
        // GTKs eget tema (det som ikke er stilt her) følger lyst/mørkt.
        // Bare når verdien endres: hver gang den settes, laster GTK hele
        // systemtemaet (Yaru) på nytt, og det koster ca. 15 ms (runde 3C).
        #[allow(deprecated)]
        if s.is_gtk_application_prefer_dark_theme() != e.dark {
            s.set_gtk_application_prefer_dark_theme(e.dark);
        }
    }
    app.on_theme.emit(&());
}

/// Lytter på systemets lyst/mørkt og redusert bevegelse.
pub fn watch_system(app: &Rc<App>) {
    let Some(s) = gtk::Settings::default() else { return };
    let a = app.clone();
    s.connect_gtk_interface_color_scheme_notify(move |_| {
        if a.settings().theme == "system" {
            apply(&a);
        }
    });
    let a = app.clone();
    s.connect_gtk_interface_reduced_motion_notify(move |_| {
        if a.settings().motion == "system" {
            a.on_settings.emit(&vec!["motion".to_string()]);
        }
    });
}
