//! Det flate kortet (port av `SvgCard` i `web/js/rgb.js`), tegnet med
//! `snapshot()`. Reserven når GL mangler eller ikke kan startes, og når du
//! velger 2D. Virker med alle tegnerne, også `GSK_RENDERER=cairo`.
//!
//! Kortet (660 × 280) skaleres inn i scenen med luft rundt. Viftene snurrer
//! etter viftefarten bare ved full bevegelse; lysdiodene får fargene fra
//! RGB-motoren, med pust, sonedemping og bakgrunnslys.

use super::layout;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use std::cell::{Cell, RefCell};
use std::f32::consts::{PI, TAU};

const VW: f32 = 660.0;
const VH: f32 = 280.0;

mod imp {
    use super::*;

    pub struct Flat {
        /// Fargen som vises per lysdiode (0–255), med pust og sonedemping.
        pub leds: RefCell<[f32; 168]>,
        pub fan_angle: Cell<[f32; 2]>,
        pub ambient: Cell<([f32; 3], f32, bool)>,
        pub label: RefCell<String>,
        /// Viftehjulet tegnet én gang (cairo), bare dreid per bilde.
        pub rotor: RefCell<Option<(i32, gdk::Texture)>>,
        /// Kortet uten lys og viftehjul, tegnet én gang per størrelse.
        pub card: RefCell<Option<(i32, gdk::Texture)>>,
        /// Buene i lysringene (samme stier hvert bilde, bare fargen endres).
        pub arcs: RefCell<Vec<gsk::Path>>,
    }

    impl Default for Flat {
        fn default() -> Self {
            Flat { leds: RefCell::new([0.0; 168]), fan_angle: Cell::new([0.3, 1.9]), ambient: Cell::new(([0.0; 3], 0.0, false)), label: RefCell::new("AGENTIC".into()), rotor: RefCell::new(None), card: RefCell::new(None), arcs: RefCell::new(Vec::new()) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Flat {
        const NAME: &'static str = "RiggFlatCard";
        type Type = super::FlatCard;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("flatcard");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Flat {}

    impl WidgetImpl for Flat {
        fn measure(&self, o: gtk::Orientation, _for: i32) -> (i32, i32, i32, i32) {
            match o {
                gtk::Orientation::Horizontal => (240, 660, -1, -1),
                _ => (140, 300, -1, -1),
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let w = self.obj();
            let (ww, hh) = (w.width() as f32, w.height() as f32);
            // Bakgrunnslyset: en myk flekk bak kortet (indre 60 % av scenen).
            let (amb, a, on) = self.ambient.get();
            if on && a > 0.004 {
                let c = gdk::RGBA::new(amb[0], amb[1], amb[2], (a * 0.35).min(1.0));
                let clear = gdk::RGBA::new(amb[0], amb[1], amb[2], 0.0);
                let r = graphene::Rect::new(0.0, 0.0, ww, hh);
                s.append_radial_gradient(&r, &graphene::Point::new(ww / 2.0, hh / 2.0), ww * 0.42, hh * 0.48, 0.0, 1.0, &[gsk::ColorStop::new(0.0, c), gsk::ColorStop::new(0.55, gdk::RGBA::new(amb[0], amb[1], amb[2], a * 0.18)), gsk::ColorStop::new(1.0, clear)]);
            }
            let cw = (ww - 48.0).min((hh - 32.0) * VW / VH).max(0.0);
            if cw < 40.0 {
                return;
            }
            let k = cw / VW;
            let ch = VH * k;
            s.save();
            s.translate(&graphene::Point::new(((ww - cw) / 2.0).round(), ((hh - ch) / 2.0).round()));
            s.scale(k, k);
            let px = (144.0 * k * w.scale_factor() as f32).ceil() as i32;
            let card_px = (VW * k * w.scale_factor() as f32).ceil() as i32;
            self.draw_card(s, px, card_px);
            s.restore();
        }
    }

    impl Flat {
        fn led(&self, i: usize) -> gdk::RGBA {
            let l = self.leds.borrow();
            gdk::RGBA::new(l[i * 3] / 255.0, l[i * 3 + 1] / 255.0, l[i * 3 + 2] / 255.0, 1.0)
        }

        /// Viftehjulet (11 blader, nav og pil) tegnet med cairo i `px` × `px`.
        /// Å dreie en ferdig tekstur er langt billigere enn å tegne stiene
        /// på nytt hvert bilde (målt: se maalinger.md).
        fn rotor(&self, px: i32) -> Option<gdk::Texture> {
            if let Some((p, t)) = self.rotor.borrow().as_ref()
                && *p == px
            {
                return Some(t.clone());
            }
            use gtk::cairo;
            let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, px, px).ok()?;
            {
                let cr = cairo::Context::new(&surf).ok()?;
                let sc = px as f64 / 144.0;
                cr.scale(sc, sc);
                cr.translate(72.0, 72.0);
                let col = |cr: &cairo::Context, h: u32| cr.set_source_rgb(((h >> 16) & 255) as f64 / 255.0, ((h >> 8) & 255) as f64 / 255.0, (h & 255) as f64 / 255.0);
                for i in 0..11 {
                    let _ = cr.save();
                    cr.rotate(i as f64 / 11.0 * std::f64::consts::TAU);
                    cr.move_to(0.0, -18.0);
                    cr.curve_to(22.0, -30.0, 30.0, -54.0, 14.0, -68.0);
                    cr.curve_to(2.0, -58.0, -6.0, -40.0, 0.0, -18.0);
                    cr.close_path();
                    col(&cr, 0x1c2129);
                    let _ = cr.fill_preserve();
                    col(&cr, 0x2a313a);
                    cr.set_line_width(0.8);
                    let _ = cr.stroke();
                    let _ = cr.restore();
                }
                cr.new_sub_path();
                cr.arc(0.0, 0.0, 20.0, 0.0, std::f64::consts::TAU);
                col(&cr, 0x262c34);
                let _ = cr.fill_preserve();
                col(&cr, 0x2f3740);
                cr.set_line_width(1.0);
                let _ = cr.stroke();
                cr.move_to(-7.0, 6.0);
                cr.line_to(0.0, -8.0);
                cr.line_to(7.0, 6.0);
                cr.close_path();
                col(&cr, 0x56606b);
                cr.set_line_width(1.6);
                cr.set_line_join(cairo::LineJoin::Round);
                let _ = cr.stroke();
            }
            surf.flush();
            let stride = surf.stride() as usize;
            let data = surf.take_data().ok()?.to_vec();
            let bytes = glib::Bytes::from_owned(data);
            let tex: gdk::Texture = gdk::MemoryTexture::new(px, px, gdk::MemoryFormat::B8g8r8a8Premultiplied, &bytes, stride).upcast();
            *self.rotor.borrow_mut() = Some((px, tex.clone()));
            Some(tex)
        }

        /// Kortet uten lys og viftehjul (brakett, kretskort, kontakter,
        /// deksel, navnet og viftehusene), tegnet med cairo én gang per størrelse.
        fn card(&self, px_w: i32) -> Option<gdk::Texture> {
            if let Some((p, t)) = self.card.borrow().as_ref()
                && *p == px_w
            {
                return Some(t.clone());
            }
            use gtk::cairo;
            let px_h = (px_w as f32 * VH / VW).ceil() as i32;
            let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, px_w, px_h).ok()?;
            {
                let cr = cairo::Context::new(&surf).ok()?;
                let sc = px_w as f64 / VW as f64;
                cr.scale(sc, sc);
                let col = |h: u32| cr.set_source_rgb(((h >> 16) & 255) as f64 / 255.0, ((h >> 8) & 255) as f64 / 255.0, (h & 255) as f64 / 255.0);
                let rr = |x: f64, y: f64, w: f64, h: f64, r: f64| {
                    cr.new_sub_path();
                    cr.arc(x + w - r, y + r, r, -PI as f64 / 2.0, 0.0);
                    cr.arc(x + w - r, y + h - r, r, 0.0, PI as f64 / 2.0);
                    cr.arc(x + r, y + h - r, r, PI as f64 / 2.0, PI as f64);
                    cr.arc(x + r, y + r, r, PI as f64, 1.5 * PI as f64);
                    cr.close_path();
                };
                let fill = |h: u32| {
                    col(h);
                    let _ = cr.fill();
                };
                // Brakett
                rr(10.0, 22.0, 20.0, 226.0, 3.0);
                fill(0x353c45);
                for (y, h) in [(40.0, 36.0), (86.0, 36.0), (132.0, 20.0)] {
                    rr(15.0, y, 10.0, h, 2.0);
                    fill(0x0b0d10);
                }
                for i in 0..7 {
                    rr(16.0, 162.0 + i as f64 * 10.0, 8.0, 5.0, 1.0);
                    fill(0x1a1f25);
                }
                // Kretskortkant og kontaktfingre
                rr(34.0, 214.0, 580.0, 16.0, 2.0);
                fill(0x0d1a14);
                let sx = 580.0 / layout::PCB.1;
                let edge = layout::FINGERS;
                for (x0, x1) in [edge.tab_a, edge.tab_b] {
                    rr(34.0 + x0 * sx, 228.0, (x1 - x0) * sx, 2.0 - edge.y0 * sx, 0.8);
                    fill(0x0d1a14);
                }
                for x in layout::finger_xs() {
                    let [x0, y0, x1, y1] = layout::finger_rect(x);
                    rr(34.0 + x0 * sx, 230.0 - y1 * sx, (x1 - x0) * sx, (y1 - y0) * sx, 0.25);
                    fill(0xb08a3e);
                }
                // Strømkontakter
                for (x, w) in [(512.0, 34.0), (552.0, 44.0)] {
                    rr(x, 14.0, w, 14.0, 2.0);
                    col(0x15191e);
                    let _ = cr.fill_preserve();
                    col(0x2b323a);
                    cr.set_line_width(1.0);
                    let _ = cr.stroke();
                }
                // Deksel
                cr.move_to(34.0, 34.0);
                for (x, y) in [(600.0, 34.0), (630.0, 62.0), (630.0, 196.0), (612.0, 216.0), (58.0, 216.0), (34.0, 196.0)] {
                    cr.line_to(x, y);
                }
                cr.close_path();
                col(0x1b2026);
                let _ = cr.fill_preserve();
                col(0x2c333c);
                cr.set_line_width(1.5);
                let _ = cr.stroke();
                for pts in [&[(318.0, 60.0), (352.0, 60.0), (340.0, 196.0), (306.0, 196.0)][..], &[(604.0, 70.0), (622.0, 86.0), (622.0, 178.0), (604.0, 194.0)][..], &[(44.0, 70.0), (58.0, 58.0), (58.0, 196.0), (44.0, 186.0)][..]] {
                    cr.move_to(pts[0].0, pts[0].1);
                    for p in &pts[1..] {
                        cr.line_to(p.0, p.1);
                    }
                    cr.close_path();
                    fill(0x1d2229);
                }
                // Navnet på dekselet (ingen logo), sperret som i SVG-en.
                let label = self.label.borrow().clone();
                cr.select_font_face("Ubuntu Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_font_size(9.0);
                let total: f64 = label.chars().map(|c| cr.text_extents(&c.to_string()).map(|e| e.x_advance()).unwrap_or(6.0) + 4.0).sum::<f64>() - 4.0;
                let mut x = 330.0 - total / 2.0;
                col(0x3b444f);
                for c in label.chars() {
                    let t = c.to_string();
                    cr.move_to(x, 210.0);
                    let _ = cr.show_text(&t);
                    x += cr.text_extents(&t).map(|e| e.x_advance()).unwrap_or(6.0) + 4.0;
                }
                // Viftehusene
                for cx in [186.0, 476.0] {
                    cr.new_sub_path();
                    cr.arc(cx, 128.0, 82.0, 0.0, TAU as f64);
                    col(0x0c0e11);
                    let _ = cr.fill_preserve();
                    col(0x252b33);
                    cr.set_line_width(2.0);
                    let _ = cr.stroke();
                    cr.new_sub_path();
                    cr.arc(cx, 128.0, 72.0, 0.0, TAU as f64);
                    fill(0x0b0d10);
                }
            }
            surf.flush();
            let stride = surf.stride() as usize;
            let data = surf.take_data().ok()?.to_vec();
            let tex: gdk::Texture = gdk::MemoryTexture::new(px_w, px_h, gdk::MemoryFormat::B8g8r8a8Premultiplied, &glib::Bytes::from_owned(data), stride).upcast();
            *self.card.borrow_mut() = Some((px_w, tex.clone()));
            Some(tex)
        }

        fn draw_card(&self, s: &gtk::Snapshot, px: i32, card_px: i32) {
            if let Some(t) = self.card(card_px.max(64)) {
                s.append_texture(&t, &graphene::Rect::new(0.0, 0.0, VW, VH));
            }
            // Viftehjulene: ferdige teksturer, bare dreid.
            let angles = self.fan_angle.get();
            for (fi, cx) in [186.0f32, 476.0].iter().enumerate() {
                if let Some(tex) = self.rotor(px.max(16)) {
                    s.save();
                    s.translate(&graphene::Point::new(*cx, 128.0));
                    s.rotate(angles[fi].to_degrees());
                    s.append_texture(&tex, &graphene::Rect::new(-72.0, -72.0, 144.0, 144.0));
                    s.restore();
                }
            }
            // Lysringene: 2 × 18 buer (stiene lages én gang).
            if self.arcs.borrow().is_empty() {
                let mut v = Vec::with_capacity(36);
                for cx in [186.0f32, 476.0] {
                    let (cy, r) = (128.0, 77.0);
                    for i in 0..18 {
                        let a0 = i as f32 / 18.0 * TAU - PI / 2.0 + 0.03;
                        let a1 = (i + 1) as f32 / 18.0 * TAU - PI / 2.0 - 0.03;
                        let b = gsk::PathBuilder::new();
                        b.move_to(cx + a0.cos() * r, cy + a0.sin() * r);
                        for k in 1..=4 {
                            let a = a0 + (a1 - a0) * k as f32 / 4.0;
                            b.line_to(cx + a.cos() * r, cy + a.sin() * r);
                        }
                        v.push(b.to_path());
                    }
                }
                *self.arcs.borrow_mut() = v;
            }
            let st = gsk::Stroke::new(3.5);
            st.set_line_cap(gsk::LineCap::Round);
            for (i, p) in self.arcs.borrow().iter().enumerate() {
                s.append_stroke(p, &st, &self.led(20 + i));
            }
            // Lyslisten langs toppen.
            let (x0, x1) = (70.0, 586.0);
            let seg = (x1 - x0) / 20.0;
            for i in 0..20 {
                let r = graphene::Rect::new(x0 + i as f32 * seg + 1.2, 40.0, seg - 2.4, 4.5);
                s.push_rounded_clip(&gsk::RoundedRect::from_rect(r, 2.0));
                s.append_color(&self.led(i), &r);
                s.pop();
            }
        }
    }
}

glib::wrapper! {
    pub struct FlatCard(ObjectSubclass<imp::Flat>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl FlatCard {
    pub fn new(label: &str) -> FlatCard {
        let f: FlatCard = glib::Object::new();
        f.set_hexpand(true);
        f.set_vexpand(true);
        *f.imp().label.borrow_mut() = label.to_uppercase();
        f.update_property(&[gtk::accessible::Property::Label("Forhåndsvisning av skjermkortets lys, sett forfra")]);
        f
    }

    /// Fargene fra motoren, med pust og sonedemping.
    pub fn set_lights(&self, e: &crate::rgb::Engine, ambient_on: bool) {
        let mut l = [0.0f32; 168];
        for i in 0..56 {
            let c = e.shown(i);
            l[i * 3..i * 3 + 3].copy_from_slice(&c);
        }
        let amb = (e.ambient.unwrap_or([0.0; 3]), e.ambient_a, ambient_on);
        let changed = *self.imp().leds.borrow() != l || self.imp().ambient.get() != amb;
        if changed {
            *self.imp().leds.borrow_mut() = l;
            self.imp().ambient.set(amb);
            self.queue_draw();
        }
    }

    /// Viftene dreies `dt` sekunder ved farten `pct` (omdreininger som demoen).
    pub fn spin(&self, dt: f32, pct: f64) {
        let rate = (60.0 + (pct.clamp(0.0, 100.0) as f32 / 100.0) * 520.0) / 360.0;
        let mut a = self.imp().fan_angle.get();
        a[0] += rate * TAU * dt;
        a[1] += rate * TAU * dt * 0.985;
        a[0] %= TAU;
        a[1] %= TAU;
        self.imp().fan_angle.set(a);
        self.queue_draw();
    }
}
