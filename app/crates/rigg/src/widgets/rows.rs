//! Etikett/verdi-rader (demoens `.kv`), merker, prikker, tomtilstand og
//! infolinjer i innholdet.

use super::{Icon, hbox, label};
use gtk::prelude::*;

/// Liste med etikett og verdi. Etikettkolonnen er så bred som den lengste
/// etiketten (demoens subgrid), og verdiene står rett ved etikettene.
#[derive(Clone)]
pub struct KvList {
    pub grid: gtk::Grid,
    rows: std::rc::Rc<std::cell::Cell<i32>>,
}

impl Default for KvList {
    fn default() -> Self {
        Self::new()
    }
}

impl KvList {
    pub fn new() -> KvList {
        let grid = gtk::Grid::new();
        grid.set_column_spacing(0);
        grid.add_css_class("kv");
        KvList { grid, rows: Default::default() }
    }

    /// Legger til en rad og gir verdi-etiketten.
    pub fn add(&self, k: &str, v: &str) -> gtk::Label {
        let val = label(v, &["v"]);
        // Kan merkes og kopieres med musa, men er ikke et stopp for Tab
        // (tastaturrunden i 4B: ellers sto Tab på hver verdi i panelene).
        val.set_selectable(true);
        val.set_focusable(false);
        val.set_wrap(true);
        val.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        val.set_hexpand(true);
        self.add_widget(k, &val);
        val
    }

    pub fn add_widget(&self, k: &str, v: &impl IsA<gtk::Widget>) {
        let r = self.rows.get();
        let kl = label(k, &["k", "kv-row"]);
        kl.set_valign(gtk::Align::Fill);
        kl.set_yalign(0.5);
        let vb = hbox(6, &["kv-row"]);
        vb.append(v);
        self.grid.attach(&kl, 0, r, 1, 1);
        self.grid.attach(&vb, 1, r, 1, 1);
        // Forrige rad var siste; nå er den ikke lenger det.
        if r > 0 {
            for c in 0..2 {
                if let Some(w) = self.grid.child_at(c, r - 1) {
                    w.remove_css_class("last");
                }
            }
        }
        kl.add_css_class("last");
        vb.add_css_class("last");
        self.rows.set(r + 1);
    }
}

/// Merke (`.tag`) med valgfritt ikon. `kind`: «», «quiet», «accent», «ok», «warn», «danger».
pub fn tag(text: &str, icon: Option<&str>, kind: &str) -> gtk::Box {
    let b = hbox(5, &["tag"]);
    if !kind.is_empty() {
        b.add_css_class(kind);
    }
    b.set_valign(gtk::Align::Center);
    if let Some(i) = icon {
        b.append(&Icon::new(i, 12));
    }
    let l = gtk::Label::new(Some(text));
    b.append(&l);
    b
}

/// Statusprikk. `kind`: «ok», «warn», «danger», «accent» eller «» (grå).
pub fn dot(kind: &str) -> gtk::Box {
    let d = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    d.add_css_class("dot");
    if !kind.is_empty() {
        d.add_css_class(kind);
    }
    d.set_valign(gtk::Align::Center);
    d.set_halign(gtk::Align::Center);
    d
}

pub fn set_dot(d: &gtk::Box, kind: &str) {
    for k in ["ok", "warn", "danger", "accent"] {
        super::class(d, k, k == kind);
    }
}

