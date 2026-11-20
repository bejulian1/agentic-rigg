//! Krokene for alle ti påskeeggene (produktspesifikasjonen kap. 7). Tekst og
//! logikk står i `rigg_core::easter`; her er det som rører vinduet. Sidene
//! kaller én funksjon hver:
//!
//! | Egg | Krok | Hvem kaller |
//! |---|---|---|
//! | 1 Juksekoden | [`on_key`] | `shortcuts.rs` |
//! | 2 Støvtørking | [`icon_click`] | `sidebar.rs` (appikonet) |
//! | 3 Utviklermodus | [`about_version`] | `settings/om.rs` |
//! | 4 Svaret på alt | [`mark42`] | `pages/oversikt.rs` (2A) |
//! | 5 Bursdag | [`launch_day`] | `pages/grafikkort.rs` (2B) |
//! | 6 Nattevakt | [`night`] | `sidebar.rs` (hvert minutt) |
//! | 7 Kaffe | [`filter_quip`] | `pages/sensorer.rs` (2A) |
//! | 8 Kallenavn | [`curve_nickname`] | `pages/viftekurve.rs` (2C) |
//! | 9 Hilsen | [`greet`] | `app.rs` ved oppstart, og `--hint` |
//!
//! Alle sjekker bryteren «Små overraskelser» selv, og husker funnet i
//! `ui.json` (`eggsFound`).

use crate::state::App;
use crate::toast::Toast;
use gtk::prelude::*;
use rigg_core::easter as E;
use rigg_core::{tr, trf};
use std::cell::RefCell;
use std::rc::Rc;

pub fn enabled(app: &App) -> bool {
    app.settings().surprises
}

pub fn disabled_hint() -> &'static str {
    tr!("Små overraskelser er slått av (Innstillinger › Generelt).")
}

fn say(app: &App, title: &str, body: &str) {
    app.toast(Toast::new(title).body(body).icon("egg").ms(5000));
}

thread_local! {
    static KONAMI: RefCell<E::Konami> = RefCell::new(E::Konami::default());
    static CLICKS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
}

/// 1 · Juksekoden. Kalles for hvert tastetrykk utenfor tekstfelt.
pub fn on_key(app: &Rc<App>, key: gtk::gdk::Key) {
    let Some(name) = key.name() else { return };
    let hit = KONAMI.with(|k| k.borrow_mut().key(&name));
    if hit && enabled(app) {
        app.egg_found("konami");
        say(app, E::konami_title(), E::konami_body());
    }
}

/// 2 · Støvtørking: fem raske klikk på appikonet. Snurrer to runder og
/// bremser opp, bare ved full bevegelse.
pub fn icon_click(app: &Rc<App>, spin: &crate::widgets::icon::Spin) {
    if !enabled(app) {
        return;
    }
    let now = rigg_core::now_ms();
    if !CLICKS.with(|c| E::dust_click(&mut c.borrow_mut(), now)) {
        return;
    }
    app.egg_found("stov");
    say(app, E::dust_title(), E::dust_body());
    if crate::motion::level(app) != crate::motion::Level::Full {
        return;
    }
    let start = std::time::Instant::now();
    spin.add_tick_callback(move |w, _| {
        let t = (start.elapsed().as_secs_f64() / 1.4).min(1.0);
        // cubic-bezier(0.15, 0.6, 0.25, 1) tilnærmet: rask start, myk stopp.
        let e = 1.0 - (1.0 - t).powf(3.2);
        if let Some(s) = w.downcast_ref::<crate::widgets::icon::Spin>() {
            s.set_angle(if t >= 1.0 { 0.0 } else { (e * 720.0) as f32 });
        }
        if t >= 1.0 { gtk::glib::ControlFlow::Break } else { gtk::glib::ControlFlow::Continue }
    });
}

/// 3 · Utviklermodus: sju klikk på versjonen i Om. `whisper` står rett
/// under versjonen; `on_dev` legger til raden med antall egg.
pub fn about_version(app: &Rc<App>, version: &gtk::Button, whisper: &gtk::Label, on_dev: impl Fn() + 'static) {
    let count = Rc::new(std::cell::Cell::new(0u32));
    let timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
    let app = app.clone();
    let whisper = whisper.clone();
    version.connect_clicked(move |_| {
        if !enabled(&app) {
            return;
        }
        if let Some(t) = timer.borrow_mut().take() {
            t.remove();
        }
        let (c2, w2, t2) = (count.clone(), whisper.clone(), timer.clone());
        *timer.borrow_mut() = Some(gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
            t2.borrow_mut().take();
            c2.set(0);
            w2.set_text("");
            w2.set_visible(false);
        }));
        let n = count.get() + 1;
        count.set(n);
        let already = app.settings().eggs_found.iter().any(|e| e == "utvikler");
        if let Some(text) = E::dev_whisper(n, already) {
            whisper.set_text(&text);
            whisper.set_visible(true);
        }
        if n >= E::DEV_CLICKS && !already {
            app.egg_found("utvikler");
            on_dev();
        }
    });
}

/// «3 av 10 funnet» (raden i Om › Feilsøking etter egg 3).
pub fn found_text(app: &App) -> String {
    trf!("{funnet} av {totalt} funnet", funnet = app.settings().eggs_found.len(), totalt = E::EGGS.len())
}

/// 4 · Svaret på alt: temperaturen som vises er nøyaktig 42 °C. Bare
/// understreken og verktøytipset er lagt til; tallet er det målte.
pub fn mark42(app: &Rc<App>, value: &gtk::Label, temp_c: Option<f64>, host: &impl IsA<gtk::Widget>) {
    let unit_c = app.settings().unit == "C";
    let on = enabled(app) && E::is_42(temp_c.map(|t| t.round() as i64), unit_c);
    let had = value.has_css_class("egg-42");
    if on == had {
        return;
    }
    if on {
        value.add_css_class("egg-42");
        value.set_markup("<u>42</u>");
        host.set_tooltip_text(Some(E::answer_tip()));
        app.egg_found("svaret");
    } else {
        value.remove_css_class("egg-42");
        host.set_tooltip_text(None);
    }
}

/// 5 · Bursdag: («8 år i dag», verktøytips) på lanseringsdagen, ellers `None`.
pub fn launch_day(app: &Rc<App>, card_name: &str) -> Option<(String, String)> {
    if !enabled(app) {
        return None;
    }
    let t = crate::app::local_now();
    let hit = E::launch_day(card_name, t.year, t.month, t.day)?;
    app.egg_found("bursdag");
    Some(hit)
}

/// 6 · Nattevakt: skal månen stå ved appnavnet nå?
pub fn night(app: &Rc<App>) -> bool {
    let on = enabled(app) && E::is_night(crate::app::local_now().hour);
    if on {
        app.egg_found("natt");
    }
    on
}

/// 7 · Kaffe: setningen i tomtilstanden på Sensorer, eller `None`.
pub fn filter_quip(app: &Rc<App>, filter: &str) -> Option<&'static str> {
    if !enabled(app) {
        return None;
    }
    let q = E::filter_quip(filter)?;
    app.egg_found("kaffe");
    Some(q)
}

/// 8 · Kallenavn på kurven i utkastet: (navn, verktøytips) eller `None`.
pub fn curve_nickname(app: &Rc<App>, points: &[(f64, f64)]) -> Option<(&'static str, &'static str)> {
    if !enabled(app) {
        return None;
    }
    let n = E::curve_nickname(points)?;
    app.egg_found("kallenavn");
    Some(n)
}

/// 9 · Hilsen på standard ut når appen startes fra en terminal.
pub fn greet(app: &Rc<App>) {
    // SAFETY: isatty på fd 1 kan ikke feile på en måte som skader.
    let tty = unsafe { libc::isatty(1) } == 1;
    if tty && enabled(app) {
        println!("{}", E::greeting());
        app.egg_found("konsoll");
    }
}
