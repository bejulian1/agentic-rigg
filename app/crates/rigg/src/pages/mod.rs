//! Registeret med alle ti sidene, i rekkefølgen i sidefeltet (også tastene
//! 1–9 og 0). Sidene bygges første gang de vises, og blir liggende til vinduet
//! lukkes (arkitektur.md 3.3, punkt 3).
//!
//! **Slik lager en side seg (runde 2):** fila eksporterer
//! `pub fn build(app: &Rc<App>) -> Box<dyn Page>`. Registeret under er
//! allerede koblet til alle åtte; ingen andre filer trenger å endres.
//! `Page::widget()` er innholdet (skallet legger det i en rullbar flate når
//! `scrolls()` er sann), og `Page::actions()` er knappene i toppfeltet.

pub mod disker;
pub mod grafer;
pub mod grafikkort;
pub mod kontroller;
pub mod lys;
pub mod oversikt;
pub mod prosessor;
pub mod sensorer;
pub mod system;
pub mod viftekurve;

use crate::state::App;
use std::rc::Rc;

/// Tastetrykk som skallet sender videre til siden som vises.
pub struct Key {
    pub key: gtk::gdk::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Fokus står i et tekstfelt.
    pub typing: bool,
}

pub trait Page {
    /// Innholdet.
    fn widget(&self) -> gtk::Widget;
    /// Knappene i toppfeltet (høyre side), eller ingenting.
    fn actions(&self) -> Option<gtk::Widget> {
        None
    }
    /// Skal skallet legge innholdet i en rullbar flate? (Sider som fyller
    /// høyden selv, som Grafer og Lys, svarer `false`.)
    fn scrolls(&self) -> bool {
        true
    }
    /// Siden vises (også etter Innstillinger). `query` er det som sto etter
    /// `?` i lenken, f.eks. `vis=gpu/temp` fra statusblokken.
    fn show(&self, _query: Option<&str>) {}
    /// Siden skjules: stopp tidtakere og tick-callbacks.
    fn hide(&self) {}
    /// Tastetrykk. `true` betyr at siden brukte det.
    fn on_key(&self, _k: &Key) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Monitor,
    Hardware,
    Control,
}

pub struct PageDef {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub section: Section,
    pub build: fn(&Rc<App>) -> Box<dyn Page>,
}

pub const PAGES: [PageDef; 10] = [
    PageDef { id: "oversikt", title: "Oversikt", icon: "layout-dashboard", section: Section::Monitor, build: oversikt::build },
    PageDef { id: "sensorer", title: "Sensorer", icon: "thermometer", section: Section::Monitor, build: sensorer::build },
    PageDef { id: "grafer", title: "Grafer", icon: "chart-line", section: Section::Monitor, build: grafer::build },
    PageDef { id: "grafikkort", title: "Grafikkort", icon: "gpu", section: Section::Hardware, build: grafikkort::build },
    PageDef { id: "prosessor", title: "Prosessor", icon: "cpu", section: Section::Hardware, build: prosessor::build },
    PageDef { id: "disker", title: "Disker", icon: "hard-drive", section: Section::Hardware, build: disker::build },
    PageDef { id: "system", title: "System", icon: "monitor", section: Section::Hardware, build: system::build },
    PageDef { id: "kontroller", title: "Klokker og effekt", icon: "sliders-horizontal", section: Section::Control, build: kontroller::build },
    PageDef { id: "viftekurve", title: "Vifter", icon: "fan", section: Section::Control, build: viftekurve::build },
    PageDef { id: "lys", title: "RGB-lys", icon: "palette", section: Section::Control, build: lys::build },
];

pub fn def(id: &str) -> Option<&'static PageDef> {
    PAGES.iter().find(|p| p.id == id)
}

pub fn section_title(s: Section) -> &'static str {
    match s {
        Section::Monitor => "Overvåking",
        Section::Hardware => "Maskinvare",
        Section::Control => "Skjermkortstyring",
    }
}

/// Det som står foran sidetittelen i toppfeltet (runde 8A): Styring-sidene
/// gjelder skjermkortet, så toppfeltet sier «Skjermkort › Vifter».
pub fn crumb(id: &str) -> Option<&'static str> {
    def(id).filter(|d| d.section == Section::Control).map(|_| "Skjermkort")
}

/// Hele navnet på en side der den står alene (vindustittelen, Startside og
/// skjermleseren): «Skjermkort › Vifter».
pub fn full_title(id: &str) -> String {
    let Some(d) = def(id) else { return id.to_string() };
    match crumb(id) {
        Some(c) => format!("{c} › {}", d.title),
        None => d.title.to_string(),
    }
}
