//! Bevegelse (docs/bevegelse.md): Automatisk følger GTKs
//! `gtk-interface-reduced-motion` og `gtk-enable-animations`. Varighetene er
//! demoens tokens. Med «Av» er alle 0, og ingen tick-callbacks registreres.

use crate::state::App;
use gtk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Full,
    Reduced,
    Off,
}

pub fn level(app: &App) -> Level {
    match app.settings().motion.as_str() {
        "full" => Level::Full,
        "reduced" => Level::Reduced,
        "off" => Level::Off,
        _ => system(),
    }
}

/// Systemets valg (GNOME › Tilgjengelighet › Reduser animasjon).
pub fn system() -> Level {
    let Some(s) = gtk::Settings::default() else { return Level::Full };
    if s.gtk_interface_reduced_motion() == gtk::ReducedMotion::Reduce {
        return Level::Reduced;
    }
    if !s.is_gtk_enable_animations() {
        return Level::Reduced;
    }
    Level::Full
}

/// Varighet for trinn 1–4 (80, 150, 200, 250 ms), justert for nivået.
pub fn dur(app: &App, step: u8) -> u32 {
    let full = [80, 150, 200, 250];
    let reduced = [80, 0, 100, 0];
    let i = (step.clamp(1, 4) - 1) as usize;
    match level(app) {
        Level::Full => full[i],
        Level::Reduced => reduced[i],
        Level::Off => 0,
    }
}

fn ease_out(t: f64) -> f64 {
    // cubic-bezier(0.2, 0, 0, 1) tilnærmet.
    1.0 - (1.0 - t).powi(3)
}

/// Toner en flate inn (sidebytte, sidefeltet etter Ctrl+B). Bare opasitet.
pub fn fade_in(app: &App, w: &impl IsA<gtk::Widget>, step: u8, from: f64) {
    let ms = dur(app, step) as f64;
    let w = w.clone().upcast::<gtk::Widget>();
    if ms <= 0.0 {
        w.set_opacity(1.0);
        return;
    }
    w.set_opacity(from);
    let start = std::time::Instant::now();
    w.add_tick_callback(move |w, _| {
        let t = (start.elapsed().as_secs_f64() * 1000.0 / ms).min(1.0);
        w.set_opacity(from + (1.0 - from) * ease_out(t));
        if t >= 1.0 { gtk::glib::ControlFlow::Break } else { gtk::glib::ControlFlow::Continue }
    });
}

/// Varighet for GTKs egne overganger (revealer, stack).
pub fn transition_ms(app: &App, step: u8) -> u32 {
    dur(app, step)
}

