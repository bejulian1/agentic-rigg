//! Hurtigtastene (produktspesifikasjonen 3.4) og dialogen
//! «Tastatursnarveier» (egen, fordi `GtkShortcutsWindow` er utfaset i 4.18).
//!
//! Rekkefølgen: dialoger først, så skallets taster (Ctrl+, Ctrl+? Ctrl+B
//! Ctrl+L Ctrl+W Ctrl+Q Ctrl +/−/0), så siden som vises (Ctrl+F, Ctrl+R,
//! mellomrom, F5 …), og til slutt 1–9 og 0 for de ti sidene når fokus ikke
//! står i et tekstfelt (0 er den tiende, som på et tastatur).

use rigg_core::tr;
use crate::pages::{Key, PAGES};
use crate::state::App;
use crate::widgets::{hbox, label, vbox};
use gtk::gdk::{Key as K, ModifierType};
use gtk::prelude::*;
use std::rc::Rc;

/// Står fokus i et tekstfelt?
pub fn typing(w: &gtk::Window) -> bool {
    gtk::prelude::GtkWindowExt::focus(w).is_some_and(|f| f.is::<gtk::Text>() || f.is::<gtk::Entry>() || f.is::<gtk::SearchEntry>() || f.is::<gtk::TextView>())
}

/// Kort beskrivelse av en flate til fokusloggen: type, tilgjengelig navn
/// eller tekst, og om fokusringen synes.
pub fn describe(w: &gtk::Widget) -> String {
    fn text(w: &gtk::Widget) -> Option<String> {
        if let Some(l) = w.downcast_ref::<gtk::Label>() {
            return Some(l.text().to_string()).filter(|t| !t.is_empty());
        }
        if let Some(b) = w.downcast_ref::<gtk::Button>()
            && let Some(l) = b.label()
        {
            return Some(l.to_string());
        }
        let mut c = w.first_child();
        while let Some(ch) = c {
            if let Some(t) = text(&ch) {
                return Some(t);
            }
            c = ch.next_sibling();
        }
        None
    }
    let name = w.tooltip_text().map(|t| t.to_string()).or_else(|| text(w)).unwrap_or_default();
    let name: String = name.lines().next().unwrap_or("").chars().take(60).collect();
    let ring = w.root().and_then(|r| r.downcast::<gtk::Window>().ok()).is_some_and(|win| win.gets_focus_visible());
    format!("{} «{name}»{}", w.type_().name(), if ring { "" } else { " (uten ring)" })
}

pub fn attach(app: &Rc<App>, window: &gtk::ApplicationWindow) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let a = app.clone();
    let win = window.clone();
    keys.connect_key_pressed(move |_, key, _, state| {
        let ctrl = state.contains(ModifierType::CONTROL_MASK);
        let shift = state.contains(ModifierType::SHIFT_MASK);
        let alt = state.contains(ModifierType::ALT_MASK);
        let typing = typing(win.upcast_ref());
        let shell = a.shell().clone();
        if !typing && !ctrl && !alt {
            crate::easter::on_key(&a, key);
        }
        // ---- Tab: rundt til starten igjen ----
        // GTK slipper ikke fokus videre fra siste flate i de egne beholderne
        // (sidene sto fast på siste graf; tastaturrunden i 4B). Samme regel
        // som GtkWindow: prøv videre, og begynn forfra når det er slutt.
        if !ctrl && !alt && matches!(key, K::Tab | K::ISO_Left_Tab | K::KP_Tab) {
            let dir = if shift || key == K::ISO_Left_Tab { gtk::DirectionType::TabBackward } else { gtk::DirectionType::TabForward };
            win.set_focus_visible(true);
            let before = gtk::prelude::GtkWindowExt::focus(&win);
            // Noen beholdere svarer «flyttet» uten å flytte fokus; da er vi på slutten.
            let moved = win.child_focus(dir) && gtk::prelude::GtkWindowExt::focus(&win) != before;
            if !moved {
                gtk::prelude::GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
                win.child_focus(dir);
            }
            if std::env::var_os("RIGG_FOKUS").is_some() {
                let f = gtk::prelude::GtkWindowExt::focus(&win);
                eprintln!("RIGG_TAB moved={moved} focus={} mapped={}", f.as_ref().map(describe).unwrap_or_default(), f.as_ref().is_some_and(|f| f.is_mapped()));
            }
            return gtk::glib::Propagation::Stop;
        }
        // ---- Skallet ----
        if ctrl && !alt {
            match key {
                K::Escape if shift => {
                    shell.go("oppgaver");
                    return gtk::glib::Propagation::Stop;
                }
                K::comma => {
                    shell.toggle_settings();
                    return gtk::glib::Propagation::Stop;
                }
                K::question | K::slash if shift || key == K::question => {
                    show_dialog(&a);
                    return gtk::glib::Propagation::Stop;
                }
                // Logg til fil: på alle sidene og i Innstillinger (logg_ui eier den).
                K::l | K::L if !shift => {
                    crate::logg_ui::toggle(&a);
                    return gtk::glib::Propagation::Stop;
                }
                K::b | K::B if !shift => {
                    if !shell.in_settings() {
                        shell.sidebar.ctrl_b();
                    }
                    return gtk::glib::Propagation::Stop;
                }
                K::w | K::W => {
                    win.close();
                    return gtk::glib::Propagation::Stop;
                }
                K::q | K::Q => {
                    win.close();
                    a.gtk.quit();
                    return gtk::glib::Propagation::Stop;
                }
                K::plus | K::equal | K::KP_Add => {
                    font_step(&a, 1);
                    return gtk::glib::Propagation::Stop;
                }
                K::minus | K::KP_Subtract => {
                    font_step(&a, -1);
                    return gtk::glib::Propagation::Stop;
                }
                K::_0 | K::KP_0 => {
                    a.set(serde_json::json!({"fontScale": 100}));
                    return gtk::glib::Propagation::Stop;
                }
                _ => {}
            }
        }
        if !ctrl && !alt && !typing && key == K::question {
            show_dialog(&a);
            return gtk::glib::Propagation::Stop;
        }
        // ---- Siden ----
        if shell.page_key(&Key { key, ctrl, shift, alt, typing }) {
            return gtk::glib::Propagation::Stop;
        }
        // ---- 1–9 og 0 ----
        if !ctrl && !alt && !typing
            && let Some(i) = key.to_unicode().and_then(|c| c.to_digit(10)).and_then(page_index)
        {
            shell.go(PAGES[i].id);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(keys);
}

/// Siden for et siffer: 1–9 er side 1–9, og 0 er den tiende.
pub fn page_index(digit: u32) -> Option<usize> {
    let i = match digit {
        0 => 9,
        1..=9 => digit as usize - 1,
        _ => return None,
    };
    (i < PAGES.len()).then_some(i)
}

fn font_step(app: &Rc<App>, dir: i32) {
    let steps = rigg_core::settings::FONT_SCALES;
    let cur = app.settings().font_scale;
    let i = steps.iter().position(|s| *s == cur).unwrap_or(1) as i32;
    let j = (i + dir).clamp(0, steps.len() as i32 - 1) as usize;
    if steps[j] != cur {
        app.set(serde_json::json!({"fontScale": steps[j]}));
    }
}

/// Tastene som (gruppe, [(taster, hva)]).
type KeyGroup = (&'static str, Vec<(&'static str, &'static str)>);

fn keys() -> [KeyGroup; 4] {
    [
    (
        tr!("Navigasjon"),
        vec![
            ("1–9 0", tr!("Åpne de ti opprinnelige sidene; 0 åpner RGB-lys")),
            ("Ctrl+Shift+Esc", tr!("Åpne Oppgaver")),
            ("↑ ↓", tr!("Flytt i sidefeltet")),
            ("Ctrl+,", tr!("Åpne og lukk Innstillinger")),
            ("Ctrl+?", tr!("Denne oversikten")),
            ("Ctrl+B", tr!("Legg sammen eller utvid sidefeltet")),
            ("Esc", tr!("Lukk meny, popover, dialog eller Innstillinger")),
        ],
    ),
    (
        tr!("Sider"),
        vec![
            ("Ctrl+F", tr!("Søk i Oppgaver, Sensorer eller Innstillinger")),
            ("/", tr!("Søk i Innstillinger")),
            ("Alt+↑ / Alt+↓", tr!("Flytt en verdi i statusblokken (Innstillinger › Sidefelt)")),
            (tr!("Mellomrom"), tr!("Pause og fortsett (Grafer)")),
            ("Ctrl+L", tr!("Logg til fil av og på")),
            ("Ctrl+R", tr!("Nullstill min og maks (Sensorer)")),
            ("F5", tr!("Oppdater nå (Oppgaver) eller les faste data på nytt (Grafikkort, Prosessor og System)")),
        ],
    ),
    (
        tr!("Vinduet"),
        vec![
            ("Ctrl++ / Ctrl+−", tr!("Større eller mindre skrift")),
            ("Ctrl+0", tr!("Vanlig skriftstørrelse")),
            ("Ctrl+W", tr!("Lukk vinduet")),
            ("Ctrl+Q", tr!("Avslutt")),
        ],
    ),
    (
        tr!("Skjermkortstyring (klokker og effekt, vifter)"),
        vec![
            ("Ctrl+Z", tr!("Angre i utkastet")),
            ("Ctrl+Shift+Z / Ctrl+Y", tr!("Gjør om")),
            ("Ctrl+Enter", tr!("Bruk utkastet")),
            ("← →", tr!("Glideren: ett steg ned eller opp (Shift: stort steg)")),
            ("PgUp PgDn", tr!("Glideren: stort steg opp eller ned")),
            ("Home End", tr!("Glideren: laveste eller høyeste verdi")),
            ("[ ]", tr!("Kurven: forrige eller neste punkt (også PgUp og PgDn)")),
            ("← → ↑ ↓", tr!("Kurven: flytt punktet 1 °C eller 1 % (Shift: 5)")),
            ("+ Delete", tr!("Kurven: legg til eller fjern et punkt")),
        ],
    ),
    ]
}

/// Gruppene i hver spalte i oversikten (indekser i [`keys`]).
const COLUMNS: [&[usize]; 2] = [&[0, 1], &[2, 3]];

/// Én tast per knapp, som i GNOME: [Ctrl] [,] og [↑] [↓].
fn caps(k: &str) -> Vec<String> {
    k.split(' ')
        .flat_map(|x| {
            if x.chars().count() > 1 && x.contains('+') && x != "+" {
                let mut parts: Vec<String> = Vec::new();
                let mut rest = x;
                while let Some(i) = rest.find('+') {
                    if i == 0 {
                        break;
                    }
                    parts.push(rest[..i].to_string());
                    rest = &rest[i + 1..];
                }
                parts.push(if rest.is_empty() { "+".into() } else { rest.to_string() });
                parts
            } else {
                vec![x.to_string()]
            }
        })
        .collect()
}

/// Et lite modalt vindu med tittel, lukkeknapp og Esc (snarveiene, lisensene).
/// Høyden er `h`, men aldri høyere enn hovedvinduet. Gir vinduet og
/// innholdsboksen under tittelen.
pub fn dialog(app: &Rc<App>, title: &str, w: i32, h: i32) -> Option<(gtk::Window, gtk::Box)> {
    let parent = app.window()?;
    let win = gtk::Window::new();
    win.set_transient_for(Some(&parent));
    win.set_modal(true);
    let ph = parent.height();
    win.set_default_size(w, if ph > 200 { h.min(ph - 24) } else { h });
    win.set_title(Some(title));
    win.add_css_class("dialog");
    crate::theme::apply_to(app, &win);
    let empty = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    empty.set_visible(false);
    win.set_titlebar(Some(&empty));
    let root = vbox(0, &[]);
    // Tittelen midt på, og GNOMEs lukkeknapp der layouten vil ha den (en
    // modal dialog får bare «lukk», som i libadwaita).
    let head = gtk::CenterBox::new();
    head.add_css_class("dialog-head");
    let t = label(title, &["dlg-title"]);
    t.set_accessible_role(gtk::AccessibleRole::Heading);
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let yes = || Rc::new(std::cell::Cell::new(true));
    head.set_start_widget(Some(&crate::topbar::controls(gtk::PackType::Start, yes())));
    head.set_center_widget(Some(&t));
    head.set_end_widget(Some(&crate::topbar::controls(gtk::PackType::End, yes())));
    let handle = gtk::WindowHandle::new();
    handle.set_child(Some(&head));
    root.append(&handle);
    win.set_child(Some(&root));
    let esc = gtk::EventControllerKey::new();
    {
        let w = win.clone();
        esc.connect_key_pressed(move |_, key, _, _| {
            if key == K::Escape {
                w.close();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
    }
    win.add_controller(esc);
    Some((win, root))
}

/// Fokus på lukkeknappen i toppen av en dialog fra [`dialog`].
pub fn focus_close(root: &gtk::Box) {
    fn first_button(w: &gtk::Widget) -> Option<gtk::Widget> {
        if w.is::<gtk::Button>() && w.is_visible() {
            return Some(w.clone());
        }
        let mut c = w.first_child();
        while let Some(ch) = c {
            if let Some(b) = first_button(&ch) {
                return Some(b);
            }
            c = ch.next_sibling();
        }
        None
    }
    if let Some(b) = root.first_child().and_then(|h| first_button(&h)) {
        b.set_focusable(true);
        b.grab_focus();
    }
}

pub fn show_dialog(app: &Rc<App>) {
    // Høy nok til alle radene med standard skrift (ca. 760 px), så ingenting
    // står under kanten; ellers ruller lista over foten, som har egen bakgrunn.
    // To spalter, som GNOMEs snarveivindu: alt får plass uten å rulle med
    // standard skrift. I smale vinduer (under 900 px) står spaltene under
    // hverandre og lista ruller.
    let wide = app.window().is_some_and(|p| p.width() >= 900);
    let Some((w, root)) = dialog(app, tr!("Tastatursnarveier"), if wide { 860 } else { 480 }, if wide { 660 } else { 780 }) else { return };
    let body = if wide { hbox(32, &["dialog-body"]) } else { vbox(0, &["dialog-body"]) };
    body.set_homogeneous(wide);
    let keys = keys();
    for groups in COLUMNS {
        let col = vbox(0, &["keys-col"]);
        col.set_hexpand(true);
        body.append(&col);
        for &gi in groups {
            let (group, list) = &keys[gi];
            col.append(&label(group, &["keys-h"]));
            for (k, what) in list.iter() {
            let row = hbox(12, &["keys-row"]);
            let l = label(what, &[]);
            l.set_hexpand(true);
            l.set_wrap(true);
            row.append(&l);
            let c = hbox(4, &["caps"]);
            for cap in caps(k) {
                // «/» skiller to taster som gjør det samme: ikke en tast.
                let kc = if cap == "/" && k.contains(" / ") { label(tr!("eller"), &["keys-or"]) } else { label(&cap, &["keycap"]) };
                kc.set_xalign(0.5);
                c.append(&kc);
            }
            row.append(&c);
            col.append(&row);
            }
        }
    }
    let sc = gtk::ScrolledWindow::new();
    sc.set_hscrollbar_policy(gtk::PolicyType::Never);
    sc.set_vexpand(true);
    sc.set_child(Some(&body));
    root.append(&sc);
    let foot_box = hbox(0, &["dialog-foot"]);
    let foot = gtk::Button::with_label(tr!("Versjon og feilsøking står under Innstillinger › Om."));
    foot.add_css_class("link");
    {
        let (a, w) = (app.clone(), w.clone());
        foot.connect_clicked(move |_| {
            w.close();
            a.open_settings("om");
        });
    }
    foot_box.append(&foot);
    root.append(&foot_box);
    w.present();
}

#[cfg(test)]
mod tests {
    #[test]
    fn sifrene_gir_alle_ti_sidene() {
        use super::page_index;
        assert_eq!(crate::pages::PAGES.len(), 12);
        assert_eq!(page_index(1), Some(0));
        assert_eq!(page_index(9), Some(8));
        assert_eq!(page_index(0), Some(9));
        let all: Vec<usize> = (0..=9).filter_map(page_index).collect();
        assert_eq!(all.len(), 10, "hver side har sin tast");
        assert_eq!(crate::pages::PAGES[page_index(0).unwrap()].id, "lys");
        assert_eq!(super::caps("1–9 0"), vec!["1–9", "0"]);
        // Sidefeltet viser den samme tasten som virker.
        for d in 0..=9 {
            let i = page_index(d).unwrap();
            assert_eq!(crate::sidebar::page_key(i), Some(d.to_string()));
        }
    }

    #[test]
    fn taster_deles_som_i_gnome() {
        assert_eq!(super::caps("Ctrl+,"), vec!["Ctrl", ","]);
        assert_eq!(super::caps("↑ ↓"), vec!["↑", "↓"]);
        assert_eq!(super::caps("Ctrl++"), vec!["Ctrl", "+"]);
        assert_eq!(super::caps("Alt+↑ / Alt+↓"), vec!["Alt", "↑", "/", "Alt", "↓"]);
        assert_eq!(super::caps("Ctrl+Shift+Z / Ctrl+Y"), vec!["Ctrl", "Shift", "Z", "/", "Ctrl", "Y"]);
        assert_eq!(super::caps("+ Delete"), vec!["+", "Delete"]);
    }

    #[test]
    fn hver_gruppe_står_i_én_spalte_og_styring_er_med() {
        let mut seen: Vec<usize> = super::COLUMNS.iter().flat_map(|c| c.iter().copied()).collect();
        seen.sort();
        let keys = super::keys();
        assert_eq!(seen, (0..keys.len()).collect::<Vec<_>>());
        let styring = keys.iter().find(|(g, _)| g.starts_with("Skjermkortstyring")).expect("Styring-gruppa");
        for k in ["Ctrl+Z", "Ctrl+Shift+Z / Ctrl+Y", "← →", "[ ]"] {
            assert!(styring.1.iter().any(|(x, _)| *x == k), "{k} mangler");
        }
    }
}
