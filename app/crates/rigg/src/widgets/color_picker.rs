//! Fargevelger for lokale utkast. Uavrundet HSV bevares under draing.
use super::{class, hbox, label, vbox};
use crate::state::App;
use gtk::cairo::{Context, LinearGradient};
use gtk::prelude::*;
use gtk::{gdk, glib};
use rigg_core::tr;
use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

fn hue(h: f64) -> f64 {
    if h.is_finite() {
        h.rem_euclid(360.0) % 360.0
    } else {
        0.0
    }
}

pub fn rgb_to_hsv(rgb: [u8; 3], fallback_hue: f64) -> Hsv {
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.0);
    let v = r.max(g).max(b);
    let delta = v - r.min(g).min(b);
    if delta == 0.0 {
        return Hsv {
            h: hue(fallback_hue),
            s: 0.0,
            v,
        };
    }
    let sector = if v == r {
        (g - b) / delta
    } else if v == g {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    Hsv {
        h: hue(sector * 60.0),
        s: delta / v,
        v,
    }
}

pub fn hue_to_rgb_f64(h: f64) -> (f64, f64, f64) {
    let sector = hue(h) / 60.0;
    let f = sector.fract();
    match sector as u8 {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    }
}

pub fn hsv_to_rgb(hsv: Hsv) -> [u8; 3] {
    let (r, g, b) = hue_to_rgb_f64(hsv.h);
    let unit = |x: f64| if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
    let (s, v) = (unit(hsv.s), unit(hsv.v));
    [r, g, b].map(|c| (255.0 * v * (1.0 - s + s * c)).round() as u8)
}

pub fn format_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let value = text.strip_prefix('#').unwrap_or(text);
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let [_, r, g, b] = u32::from_str_radix(value, 16).ok()?.to_be_bytes();
    Some([r, g, b])
}

fn rounded(ctx: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    ctx.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    ctx.arc(x + r, y + r, r, PI, PI + FRAC_PI_2);
    ctx.close_path();
}

pub fn draw_swatch(ctx: &Context, w: i32, h: i32, rgb: Option<[u8; 3]>) {
    rounded(ctx, 0.5, 0.5, f64::from(w - 1), f64::from(h - 1), 3.0);
    if let Some([r, g, b]) = rgb {
        ctx.set_source_rgb(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        );
        let _ = ctx.fill_preserve();
    }
    ctx.set_source_rgba(0.502, 0.502, 0.533, 0.45);
    ctx.set_line_width(1.0);
    let _ = ctx.stroke();
}

#[derive(Clone, Copy)]
enum Plane {
    Sv,
    Hue,
}

impl Plane {
    fn area(self) -> gtk::DrawingArea {
        let (height, name, cursor) = match self {
            Self::Sv => (156, tr!("Metning og lyshet"), "crosshair"),
            Self::Hue => (18, tr!("Fargetone"), "ew-resize"),
        };
        let area = gtk::DrawingArea::new();
        area.set_content_width(232);
        area.set_content_height(height);
        area.set_focusable(true);
        area.set_cursor_from_name(Some(cursor));
        area.add_css_class("rgb-spectrum");
        area.set_accessible_role(gtk::AccessibleRole::Slider);
        area.update_property(&[
            gtk::accessible::Property::Label(name),
            gtk::accessible::Property::ValueMin(0.0),
            gtk::accessible::Property::ValueMax(if matches!(self, Self::Hue) {
                360.0
            } else {
                100.0
            }),
        ]);
        area
    }

    fn draw(self, ctx: &Context, w: f64, h: f64, hsv: Hsv) {
        rounded(ctx, 0.0, 0.0, w, h, 4.0);
        ctx.clip();
        let gradient = LinearGradient::new(0.0, 0.0, w, 0.0);
        match self {
            Self::Sv => {
                let (r, g, b) = hue_to_rgb_f64(hsv.h);
                gradient.add_color_stop_rgb(0.0, 1.0, 1.0, 1.0);
                gradient.add_color_stop_rgb(1.0, r, g, b);
                let _ = ctx.set_source(&gradient);
                let _ = ctx.paint();
                let shade = LinearGradient::new(0.0, 0.0, 0.0, h);
                shade.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
                shade.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 1.0);
                let _ = ctx.set_source(&shade);
                let _ = ctx.paint();
                ctx.arc(hsv.s * w, (1.0 - hsv.v) * h, 5.5, 0.0, TAU);
            }
            Self::Hue => {
                for stop in 0..=6 {
                    let (r, g, b) = hue_to_rgb_f64(f64::from(stop) * 60.0);
                    gradient.add_color_stop_rgb(f64::from(stop) / 6.0, r, g, b);
                }
                let _ = ctx.set_source(&gradient);
                let _ = ctx.paint();
                let x = (w - 7.0).max(0.0) * hsv.h / 360.0;
                rounded(ctx, x + 1.5, 1.5, 4.0, h - 3.0, 2.0);
            }
        }
        ctx.set_source_rgb(0.12, 0.12, 0.12);
        ctx.set_line_width(3.5);
        let _ = ctx.stroke_preserve();
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        ctx.set_line_width(1.5);
        let _ = ctx.stroke();
    }

    fn point(self, hsv: &mut Hsv, x: f64, y: f64) {
        match self {
            Self::Sv => {
                hsv.s = x;
                hsv.v = 1.0 - y;
            }
            Self::Hue => hsv.h = (360.0 * x).min(360.0_f64.next_down()),
        }
    }

    fn key(self, hsv: &mut Hsv, key: gdk::Key, shift: bool) -> bool {
        let step = if shift { 0.10 } else { 0.02 };
        let deg = if shift { 15.0 } else { 3.0 };
        use gdk::Key;
        match (self, key) {
            (Self::Sv, Key::Left) => hsv.s = (hsv.s - step).max(0.0),
            (Self::Sv, Key::Right) => hsv.s = (hsv.s + step).min(1.0),
            (Self::Sv, Key::Up) => hsv.v = (hsv.v + step).min(1.0),
            (Self::Sv, Key::Down) => hsv.v = (hsv.v - step).max(0.0),
            (Self::Sv, Key::Home) => {
                hsv.s = 0.0;
                hsv.v = 0.0;
            }
            (Self::Sv, Key::End) => {
                hsv.s = 1.0;
                hsv.v = 1.0;
            }
            (Self::Hue, Key::Left | Key::Down) => hsv.h = hue(hsv.h - deg),
            (Self::Hue, Key::Right | Key::Up) => hsv.h = hue(hsv.h + deg),
            (Self::Hue, Key::Home) => hsv.h = 0.0,
            (Self::Hue, Key::End) => hsv.h = 360.0_f64.next_down(),
            _ => return false,
        }
        true
    }
}

pub struct ColorPicker {
    root: gtk::Box,
    sv: gtk::DrawingArea,
    hue: gtk::DrawingArea,
    preview: gtk::DrawingArea,
    hex: gtk::Entry,
    channels: [gtk::SpinButton; 3],
    hsv: Cell<Hsv>,
    internal: Cell<bool>,
    on_change: Box<dyn Fn([u8; 3])>,
    pop: glib::WeakRef<gtk::Popover>,
}

impl ColorPicker {
    pub fn new(app: &App, rgb: [u8; 3], on_change: impl Fn([u8; 3]) + 'static) -> Rc<Self> {
        let root = vbox(8, &["rgb-picker"]);
        let sv = Plane::Sv.area();
        let hue = Plane::Hue.area();
        root.append(&sv);
        root.append(&hue);
        let row = hbox(6, &[]);
        let preview = gtk::DrawingArea::new();
        preview.set_content_width(26);
        preview.set_content_height(26);
        preview.set_valign(gtk::Align::Center);
        preview.add_css_class("rgb-picker-preview");
        let hex = gtk::Entry::new();
        hex.add_css_class("mono");
        hex.set_hexpand(true);
        hex.set_max_length(7);
        hex.update_property(&[gtk::accessible::Property::Label(tr!("Fargekode"))]);
        row.append(&preview);
        row.append(&hex);
        root.append(&row);
        let row = hbox(6, &[]);
        let channels = [
            ("R", tr!("Rød kanal")),
            ("G", tr!("Grønn kanal")),
            ("B", tr!("Blå kanal")),
        ]
        .map(|(short, name)| {
            let field = vbox(2, &[]);
            let spin = gtk::SpinButton::with_range(0.0, 255.0, 1.0);
            spin.set_width_chars(3);
            spin.set_max_width_chars(3);
            spin.set_numeric(true);
            spin.add_css_class("ctl-spin");
            spin.update_property(&[gtk::accessible::Property::Label(name)]);
            field.set_hexpand(true);
            field.append(&label(short, &["sm", "muted"]));
            field.append(&spin);
            row.append(&field);
            spin
        });
        root.append(&row);
        let foot = label(
            tr!("Forhåndsvises på kortet. Trykk «Bruk» for å sende."),
            &["sm", "muted"],
        );
        foot.set_wrap(true);
        foot.set_max_width_chars(28);
        root.append(&foot);
        let picker = Rc::new(Self {
            root,
            sv,
            hue,
            preview,
            hex,
            channels,
            hsv: Cell::new(rgb_to_hsv(rgb, 0.0)),
            internal: Cell::new(false),
            on_change: Box::new(on_change),
            pop: glib::WeakRef::new(),
        });
        picker.wire();
        picker.sync(picker.hsv.get(), false);
        let weak = Rc::downgrade(&picker);
        app.on_theme.connect(move |_| {
            if let Some(p) = weak.upgrade() {
                for area in [&p.sv, &p.hue, &p.preview] {
                    area.queue_draw();
                }
            }
        });
        picker
    }

    pub fn set_rgb(&self, rgb: [u8; 3]) {
        let hsv = self.hsv.get();
        if hsv_to_rgb(hsv) != rgb {
            self.sync(rgb_to_hsv(rgb, hsv.h), false);
        }
    }

    pub fn open(&self, anchor: &gtk::Button) {
        if self.root.parent().is_some() {
            return;
        }
        self.sync(self.hsv.get(), false);
        let pop = super::popover::popover(anchor, &self.root);
        pop.add_css_class("rgb-picker-pop");
        pop.set_autohide(true);
        pop.connect_closed(|p| p.set_child(gtk::Widget::NONE));
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let (weak, anchor) = (pop.downgrade(), anchor.downgrade());
        keys.connect_key_pressed(move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if let Some(pop) = weak.upgrade() {
                pop.popdown();
            }
            if let Some(anchor) = anchor.upgrade() {
                anchor.grab_focus();
            }
            glib::Propagation::Stop
        });
        pop.add_controller(keys);
        self.pop.set(Some(&pop));
        pop.popup();
        self.sv.grab_focus();
    }

    pub fn close(&self) {
        if let Some(pop) = self.pop.upgrade() {
            pop.popdown();
        }
    }

    fn sync(&self, hsv: Hsv, notify: bool) {
        let rgb = hsv_to_rgb(hsv);
        let changed = hsv_to_rgb(self.hsv.replace(hsv)) != rgb;
        self.internal.set(true);
        let hex = format_hex(rgb);
        if self.hex.text() != hex {
            self.hex.set_text(&hex);
        }
        class(&self.hex, "error", false);
        for (spin, channel) in self.channels.iter().zip(rgb) {
            if spin.value_as_int() != i32::from(channel) {
                spin.set_value(f64::from(channel));
            }
        }
        self.internal.set(false);
        self.hue
            .update_property(&[gtk::accessible::Property::ValueNow(hsv.h)]);
        let sv = format!("{:.0}% / {:.0}%", hsv.s * 100.0, hsv.v * 100.0);
        self.sv.update_property(&[
            gtk::accessible::Property::ValueNow(hsv.s * 100.0),
            gtk::accessible::Property::ValueText(&sv),
        ]);
        for area in [&self.sv, &self.hue, &self.preview] {
            area.queue_draw();
        }
        if changed && notify {
            (self.on_change)(rgb);
        }
    }

    fn wire(self: &Rc<Self>) {
        for (plane, area) in [(Plane::Sv, &self.sv), (Plane::Hue, &self.hue)] {
            let weak = Rc::downgrade(self);
            area.set_draw_func(move |_, ctx, w, h| {
                if let Some(p) = weak.upgrade() {
                    plane.draw(ctx, f64::from(w), f64::from(h), p.hsv.get());
                }
            });
            let drag = gtk::GestureDrag::new();
            let (weak, target) = (Rc::downgrade(self), area.downgrade());
            let point = move |x: f64, y: f64| {
                if let (Some(p), Some(area)) = (weak.upgrade(), target.upgrade()) {
                    let mut hsv = p.hsv.get();
                    let unit = |pos: f64, size: i32| (pos / f64::from(size.max(1))).clamp(0.0, 1.0);
                    plane.point(&mut hsv, unit(x, area.width()), unit(y, area.height()));
                    p.sync(hsv, true);
                }
            };
            let begin = point.clone();
            drag.connect_drag_begin(move |g, x, y| {
                if let Some(area) = g.widget() {
                    area.grab_focus();
                }
                begin(x, y);
            });
            drag.connect_drag_update(move |g, dx, dy| {
                if let Some((x, y)) = g.start_point() {
                    point(x + dx, y + dy);
                }
            });
            area.add_controller(drag);
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, mods| {
                if let Some(p) = weak.upgrade() {
                    let mut hsv = p.hsv.get();
                    if plane.key(&mut hsv, key, mods.contains(gdk::ModifierType::SHIFT_MASK)) {
                        p.sync(hsv, true);
                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            });
            area.add_controller(keys);
        }
        let weak = Rc::downgrade(self);
        self.preview.set_draw_func(move |_, ctx, w, h| {
            if let Some(p) = weak.upgrade() {
                draw_swatch(ctx, w, h, Some(hsv_to_rgb(p.hsv.get())));
            }
        });
        let weak = Rc::downgrade(self);
        self.hex.connect_changed(move |entry| {
            if let Some(p) = weak.upgrade()
                && !p.internal.get()
            {
                let rgb = parse_hex(&entry.text());
                class(entry, "error", rgb.is_none());
                if let Some(rgb) = rgb {
                    p.sync(rgb_to_hsv(rgb, p.hsv.get().h), true);
                }
            }
        });
        for spin in &self.channels {
            let weak = Rc::downgrade(self);
            spin.connect_value_changed(move |_| {
                if let Some(p) = weak.upgrade()
                    && !p.internal.get()
                {
                    let rgb = p.channels.each_ref().map(|s| s.value_as_int() as u8);
                    p.sync(rgb_to_hsv(rgb, p.hsv.get().h), true);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for (hex, _) in crate::rgb::SWATCHES {
            let rgb = parse_hex(hex).unwrap();
            assert_eq!(hsv_to_rgb(rgb_to_hsv(rgb, 97.0)), rgb);
        }
        for (i, rgb) in [
            [255, 0, 0],
            [255, 255, 0],
            [0, 255, 0],
            [0, 255, 255],
            [0, 0, 255],
            [255, 0, 255],
        ]
        .into_iter()
        .enumerate()
        {
            let hsv = rgb_to_hsv(rgb, 97.0);
            assert_eq!(hsv.h, i as f64 * 60.0);
            assert_eq!(hsv_to_rgb(hsv), rgb);
        }
        for shade in 0..=255 {
            let hsv = rgb_to_hsv([shade; 3], -30.0);
            assert_eq!(hsv.h, 330.0);
            assert_eq!(hsv_to_rgb(hsv), [shade; 3]);
        }
    }

    #[test]
    fn clamps_and_wraps() {
        let rgb = |h, s, v| hsv_to_rgb(Hsv { h, s, v });
        assert_eq!(rgb(360.0, 2.0, 1.1), [255, 0, 0]);
        assert_eq!(rgb(-120.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(rgb(90.0, -2.0, 1.0), [255; 3]);
        assert_eq!(rgb(90.0, 1.0, -1.0), [0; 3]);
        assert_eq!(rgb(f64::NAN, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(rgb(f64::INFINITY, f64::NAN, f64::NAN), [0; 3]);
    }

    #[test]
    fn hex_codes() {
        assert_eq!(format_hex([0, 15, 175]), "#000FAF");
        for text in ["000faf", "#000FAF"] {
            assert_eq!(parse_hex(text), Some([0, 15, 175]));
        }
        for text in ["", "#abc", "000faø", " 000faf", "#000FAF00", "##000fa"] {
            assert_eq!(parse_hex(text), None);
        }
    }
}
