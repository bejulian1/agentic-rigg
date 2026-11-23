//! Raden i et innstillingspanel: tekst (tittel og forklaring) og en kontroll.
//!
//! Før var raden en vannrett `GtkBox`. Den kan ikke bryte: når kontrollen var
//! bred (fire segmenter, tysk tekst, skrift «Større»), ble teksten presset
//! ned til sitt lengste ord og brutt i mange korte linjer, eller raden ble
//! bredere enn panelet og stakk ut i nabospalten. Her legger kontrollen seg
//! under teksten når det blir trangt (`fit::row_fit`), som i et skjema på en
//! smal skjerm. `wrap`-rader står alltid under hverandre og fyller bredden
//! (miniatyrer, fargeprøver, lister).
//!
//! Barna er alltid tekstboksen og kontrollboksen, i den rekkefølgen, så kode
//! som henter forklaringen eller bryteren med `first_child`/`last_child`,
//! virker som før.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::Cell;

/// Avstanden mellom tekst og kontroll side ved side (som `--sp-4`).
const GAP: i32 = 16;
/// Avstanden når kontrollen har lagt seg under teksten (som `--sp-2`).
const STACK_GAP: i32 = 8;
/// Teksten skal ha minst så mye (px ved 100 % skrift) ved siden av kontrollen.
const TEXT_COMFORT: f64 = 180.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SetRow {
        /// Alltid under hverandre, og kontrollen fyller bredden.
        pub wrap: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SetRow {
        const NAME: &'static str = "RiggSetRow";
        type Type = super::SetRow;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("setrow");
        }
    }

    impl ObjectImpl for SetRow {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl SetRow {
        fn kids(&self) -> Option<(gtk::Widget, gtk::Widget)> {
            let t = self.obj().first_child()?;
            let c = t.next_sibling()?;
            Some((t, c))
        }

        /// `Some((tekstbredde, kontrollbredde))` side ved side, ellers `None`.
        fn side(&self, width: i32, text: &gtk::Widget, ctrl: &gtk::Widget) -> Option<(i32, i32)> {
            if self.wrap.get() || !ctrl.is_visible() {
                return None;
            }
            let (tmin, tnat, _, _) = text.measure(gtk::Orientation::Horizontal, -1);
            let (cmin, cnat, _, _) = ctrl.measure(gtk::Orientation::Horizontal, -1);
            let comfort = (TEXT_COMFORT * crate::theme::font_scale().max(0.5)).round() as i32;
            crate::widgets::fit::row_fit(width, GAP, (tmin, tnat), (cmin, cnat), comfort)
        }

        /// Kontrollens bredde når den står under teksten.
        fn stacked_ctrl_width(&self, width: i32, ctrl: &gtk::Widget) -> i32 {
            let (cmin, cnat, _, _) = ctrl.measure(gtk::Orientation::Horizontal, -1);
            if self.wrap.get() { width.max(cmin) } else { cnat.min(width).max(cmin) }
        }

        fn stack_gap(&self) -> i32 {
            // `wrap`-radene har avstanden som marg i CSS (.set-row.wrap .ctrl).
            if self.wrap.get() { 0 } else { STACK_GAP }
        }
    }

    impl WidgetImpl for SetRow {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some((text, ctrl)) = self.kids() else { return (0, 0, -1, -1) };
            let shown = ctrl.is_visible();
            match o {
                gtk::Orientation::Horizontal => {
                    let (tmin, tnat, _, _) = text.measure(o, -1);
                    if !shown {
                        return (tmin, tnat, -1, -1);
                    }
                    let (cmin, cnat, _, _) = ctrl.measure(o, -1);
                    let nat = if self.wrap.get() { tnat.max(cnat) } else { tnat + GAP + cnat };
                    (tmin.max(cmin), nat.max(tmin.max(cmin)), -1, -1)
                }
                _ => {
                    let width = if for_size > 0 { for_size } else { self.measure(gtk::Orientation::Horizontal, -1).1 };
                    if !shown {
                        let (a, b, _, _) = text.measure(o, width);
                        return (a, b, -1, -1);
                    }
                    match self.side(width, &text, &ctrl) {
                        Some((tw, cw)) => {
                            let (t0, t1, _, _) = text.measure(o, tw);
                            let (c0, c1, _, _) = ctrl.measure(o, cw);
                            (t0.max(c0), t1.max(c1), -1, -1)
                        }
                        None => {
                            let (t0, t1, _, _) = text.measure(o, width);
                            let (c0, c1, _, _) = ctrl.measure(o, self.stacked_ctrl_width(width, &ctrl));
                            let gap = self.stack_gap();
                            (t0 + gap + c0, t1 + gap + c1, -1, -1)
                        }
                    }
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let Some((text, ctrl)) = self.kids() else { return };
            if !ctrl.is_visible() {
                text.size_allocate(&gtk::Allocation::new(0, 0, width, height), -1);
                return;
            }
            let rtl = self.obj().direction() == gtk::TextDirection::Rtl;
            match self.side(width, &text, &ctrl) {
                Some((tw, cw)) => {
                    let (tx, cx) = if rtl { (cw + GAP, 0) } else { (0, width - cw) };
                    text.size_allocate(&gtk::Allocation::new(tx, 0, tw, height), -1);
                    ctrl.size_allocate(&gtk::Allocation::new(cx, 0, cw, height), -1);
                }
                None => {
                    let th = text.measure(gtk::Orientation::Vertical, width).1;
                    let cw = self.stacked_ctrl_width(width, &ctrl);
                    let y = th + self.stack_gap();
                    let ch = ctrl.measure(gtk::Orientation::Vertical, cw).1.max(height - y);
                    text.size_allocate(&gtk::Allocation::new(0, 0, width, th), -1);
                    ctrl.size_allocate(&gtk::Allocation::new(if rtl { width - cw } else { 0 }, y, cw, ch), -1);
                }
            }
        }
    }
}

glib::wrapper! {
    /// Tekst og kontroll side ved side, eller under hverandre når det er trangt.
    pub struct SetRow(ObjectSubclass<imp::SetRow>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SetRow {
    /// `wrap`: kontrollen står alltid under teksten og fyller bredden.
    pub fn new(text: &impl IsA<gtk::Widget>, ctrl: &impl IsA<gtk::Widget>, wrap: bool) -> SetRow {
        let r: SetRow = glib::Object::new();
        r.imp().wrap.set(wrap);
        text.set_parent(&r);
        ctrl.set_parent(&r);
        r
    }
}
