//! Byggeklossene vinduet deler. Alle sidene bruker disse, så uttrykket er
//! likt overalt (demoens komponenter i `web/styles.css` seksjon 5).
//!
//! | Fil | Hva | Eier |
//! |---|---|---|
//! | `icon.rs` | Lucide-ikoner med GskPath, `Spin` | grunnmuren |
//! | `panel.rs` | panel med overskriftsstripe | grunnmuren |
//! | `seg.rs` | segmentknapp | grunnmuren |
//! | `switchrow.rs` | bryter og avkrysning | grunnmuren |
//! | `rows.rs` | etikett/verdi-rader, merker, prikker, tomtilstand, infolinje | grunnmuren |
//! | `spark.rs` | sparklinje | grunnmuren |
//! | `popover.rs` | meny og popover | grunnmuren |
//! | `masonry.rs` | spalter | grunnmuren |
//! | `fit.rs` | antall spalter som får plass (felles for spalteoppsettene) | 1.6.1 |
//! | `chart.rs`, `gauge.rs`, `bar.rs` | graf, målere, stolper | 2A |
//! | `locked.rs` | låst-banneret og lesevisningene | 2C |
//! | `oc.rs` | glideren, tallflisene og (i) på Kontroller og Viftekurve | 6C |

pub mod bar;
pub mod chart;
pub mod charthead;
pub mod fit;
pub mod fixed;
pub mod gauge;
pub mod icon;
pub mod leadrow;
pub mod ikoner_data;
pub mod locked;
pub mod masonry;
pub mod oc;
pub mod panel;
pub mod popover;
pub mod rows;
pub mod seg;
pub mod setrow;
pub mod spark;
pub mod styrer;
pub mod switchrow;
pub mod trader;
pub mod version_log;
pub mod wrap;

/// Tegner vinduet med cairo (`GSK_RENDERER=cairo`, standard fra runde 3C)?
/// Da tegnes sparklinjene med cairo direkte i stedet for som GSK-stier.
pub fn cairo_renderer() -> bool {
    thread_local! {
        static CAIRO: bool = std::env::var("GSK_RENDERER").is_ok_and(|r| r == "cairo");
    }
    CAIRO.with(|c| *c)
}

/// En plass til ett tilbakekall: `Slot<dyn Fn(i32, i32)>`.
pub type Slot<F> = std::cell::RefCell<Option<Box<F>>>;
/// Flere lyttere som kan kalles mens lista endres: `Listeners<dyn Fn(&str)>`.
pub type Listeners<F> = std::cell::RefCell<Vec<std::rc::Rc<F>>>;
/// Plasseringen av barna i en spalteoppsett: (barn, x, y, bredde, høyde) og høyden.
pub type Placed = (Vec<(gtk::Widget, i32, i32, i32, i32)>, i32);

pub use icon::Icon;
pub use panel::Panel;
pub use seg::Seg;
pub use spark::Spark;

use gtk::prelude::*;

/// Etikett med klasser, venstrejustert.
pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// Gir ledig minne i malloc tilbake til systemet (alle arenaene, også de
/// byggetråder har brukt). Kalles etter store, kortvarige jobber (3D-kortet).
pub fn trim_memory() {
    // SAFETY: malloc_trim har ingen forutsetninger.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Fyll og strek for grafene: GSK-stier med GL/Vulkan, eller én cairo-node
/// med vanlig cairo-fyll og -strek med cairo-tegneren. Med cairo tegnet GSK
/// stiene som klippemasker; direkte cairo halverte tida per bilde (runde 3C).
pub enum Painter<'a> {
    Gsk(&'a gtk::Snapshot),
    Cairo(gtk::cairo::Context),
}

impl<'a> Painter<'a> {
    /// `bounds`: flata alt tegnes innenfor (cairo-noden).
    pub fn new(s: &'a gtk::Snapshot, bounds: &gtk::graphene::Rect) -> Painter<'a> {
        if cairo_renderer() { Painter::Cairo(s.append_cairo(bounds)) } else { Painter::Gsk(s) }
    }

    fn rgba(cr: &gtk::cairo::Context, c: &gtk::gdk::RGBA) {
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
    }

    /// Lukket mangekant, fylt med vinding.
    pub fn fill(&self, pts: impl IntoIterator<Item = (f32, f32)>, color: &gtk::gdk::RGBA) {
        match self {
            Painter::Gsk(s) => {
                let p = gtk::gsk::PathBuilder::new();
                let mut first = true;
                for (x, y) in pts {
                    if first {
                        p.move_to(x, y);
                        first = false;
                    } else {
                        p.line_to(x, y);
                    }
                }
                p.close();
                s.append_fill(&p.to_path(), gtk::gsk::FillRule::Winding, color);
            }
            Painter::Cairo(cr) => {
                let mut first = true;
                for (x, y) in pts {
                    if first {
                        cr.move_to(x as f64, y as f64);
                        first = false;
                    } else {
                        cr.line_to(x as f64, y as f64);
                    }
                }
                cr.close_path();
                cr.set_fill_rule(gtk::cairo::FillRule::Winding);
                Self::rgba(cr, color);
                let _ = cr.fill();
            }
        }
    }

    /// Åpen linje. `round`: runde hjørner (ellers spisse), `dash`: stiplet.
    pub fn stroke(&self, pts: impl IntoIterator<Item = (f32, f32)>, width: f32, round: bool, dash: Option<&[f32]>, color: &gtk::gdk::RGBA) {
        match self {
            Painter::Gsk(s) => {
                let p = gtk::gsk::PathBuilder::new();
                let mut first = true;
                for (x, y) in pts {
                    if first {
                        p.move_to(x, y);
                        first = false;
                    } else {
                        p.line_to(x, y);
                    }
                }
                let st = gtk::gsk::Stroke::new(width);
                st.set_line_join(if round { gtk::gsk::LineJoin::Round } else { gtk::gsk::LineJoin::Miter });
                st.set_line_cap(gtk::gsk::LineCap::Butt);
                if let Some(d) = dash {
                    st.set_dash(d);
                }
                s.append_stroke(&p.to_path(), &st, color);
            }
            Painter::Cairo(cr) => {
                let mut first = true;
                for (x, y) in pts {
                    if first {
                        cr.move_to(x as f64, y as f64);
                        first = false;
                    } else {
                        cr.line_to(x as f64, y as f64);
                    }
                }
                cr.set_line_width(width as f64);
                cr.set_line_join(if round { gtk::cairo::LineJoin::Round } else { gtk::cairo::LineJoin::Miter });
                cr.set_line_cap(gtk::cairo::LineCap::Butt);
                match dash {
                    Some(d) => cr.set_dash(&d.iter().map(|v| *v as f64).collect::<Vec<_>>(), 0.0),
                    None => cr.set_dash(&[], 0.0),
                }
                Self::rgba(cr, color);
                let _ = cr.stroke();
            }
        }
    }
}

pub fn set_text(l: &gtk::Label, text: &str) {
    if l.text() != text {
        l.set_text(text);
    }
}

pub fn hbox(spacing: i32, classes: &[&str]) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
    for c in classes {
        b.add_css_class(c);
    }
    b
}

pub fn vbox(spacing: i32, classes: &[&str]) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, spacing);
    for c in classes {
        b.add_css_class(c);
    }
    b
}

/// Knapp med ikon og/eller tekst. `classes`: «flat», «suggested», «sm», «icon-only» …
pub fn button(icon: Option<&str>, text: Option<&str>, classes: &[&str]) -> gtk::Button {
    let b = gtk::Button::new();
    for c in classes {
        b.add_css_class(c);
    }
    let size = if classes.contains(&"sm") { 14 } else { 16 };
    match (icon, text) {
        (Some(i), Some(t)) => {
            let bx = hbox(6, &[]);
            bx.append(&Icon::new(i, size));
            bx.append(&gtk::Label::new(Some(t)));
            b.set_child(Some(&bx));
        }
        (Some(i), None) => {
            b.set_child(Some(&Icon::new(i, size)));
            if !classes.contains(&"icon-only") {
                b.add_css_class("icon-only");
            }
        }
        (None, Some(t)) => b.set_label(t),
        (None, None) => {}
    }
    b
}

/// Verktøytips med hurtigtast: «Legg sammen sidefeltet  Ctrl+B».
pub fn tip(w: &impl IsA<gtk::Widget>, text: &str, key: Option<&str>) {
    let t = match key {
        Some(k) => format!("{text}  ·  {k}"),
        None => text.to_string(),
    };
    w.set_tooltip_text(Some(&t));
}

/// Fjerner alle barn fra en boks.
pub fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// Legger til eller fjerner en klasse.
pub fn class(w: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    if on {
        w.add_css_class(name);
    } else {
        w.remove_css_class(name);
    }
}
