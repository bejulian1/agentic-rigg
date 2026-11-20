//! GtkApplication med én forekomst (arkitektur.md 6.2). Starter du appen når
//! vinduet er åpent, kommer det samme vinduet fram. `ui.json` leses før
//! første bilde; tilkoblingen til tjenesten, de faste dataene og historikken
//! kommer etter.
//!
//! Med `RIGG_TIMING=1` skriver vinduet `RIGG_TIMING …` til stderr ved første
//! bilde (`scripts/maal.sh`).

use crate::state::{App, Conn};
use gtk::prelude::*;
use gtk::{gio, glib};
use rigg_core::model::{GpuInfo, GpuStatic, Telemetry};
use serde_json::{Value, json};
use std::rc::Rc;

/// Valg fra kommandolinja som gjelder vinduet.
#[derive(Clone, Debug, Default)]
pub struct Cli {
    pub page: Option<String>,
    pub settings: Option<String>,
}

/// Ikonnavnet for varianten som er valgt (docs/produksjon/ikon.md kap. 6).
pub fn icon_name(app: &App) -> String {
    let id = app.settings().app_icon.clone();
    if id == "standard" { rigg_core::paths::APP_ID.to_string() } else { format!("{}-{id}", rigg_core::paths::APP_ID) }
}

/// Lokal tid nå.
pub fn local_now() -> rigg_core::fmt::LocalTime {
    local_time(rigg_core::now_ms())
}

/// Lokal tid for et tidspunkt (ms), med tidssonen fra systemet.
pub fn local_time(ms: i64) -> rigg_core::fmt::LocalTime {
    let t: libc::time_t = (ms / 1000) as libc::time_t;
    // SAFETY: localtime_r skriver bare i `tm`, som vi eier.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    rigg_core::fmt::local(ms, tm.tm_gmtoff)
}

pub fn run(cli: Cli) -> glib::ExitCode {
    let profile = rigg_core::paths::Profile::current();
    let gapp = gtk::Application::builder().application_id(profile.app_id()).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();
    let state: Rc<std::cell::RefCell<Option<Rc<App>>>> = Rc::new(std::cell::RefCell::new(None));

    {
        let state = state.clone();
        gapp.connect_command_line(move |gapp, cmd| {
            // Andre gang: kommandolinja kommer hit i den første forekomsten.
            let args: Vec<String> = cmd.arguments().iter().map(|a| a.to_string_lossy().into_owned()).collect();
            let cli = crate::parse_window_args(&args[1..]);
            crate::timing::mark("command_line");
            activate(gapp, &state, &cli);
            glib::ExitCode::SUCCESS
        });
    }
    let _ = cli;
    gapp.run_with_args(&std::env::args().collect::<Vec<_>>())
}

fn activate(gapp: &gtk::Application, state: &Rc<std::cell::RefCell<Option<Rc<App>>>>, cli: &Cli) {
    if let Some(app) = state.borrow().as_ref() {
        // Forekomsten finnes: vis vinduet, og gå til siden som ble bedt om.
        if let Some(p) = &cli.page {
            app.go(p);
        }
        if let Some(section) = &cli.settings {
            app.open_settings(section);
        }
        crate::widgets::locked::poll_helper(app);
        if let Some(w) = app.window() {
            w.present();
        }
        return;
    }
    let profile = rigg_core::paths::Profile::current();
    let (settings, note) = crate::prefs::load(profile);
    rigg_core::fmt::set_format(rigg_core::fmt::Format::from_settings(&settings));
    crate::timing::mark("prefs");
    let display = gtk::gdk::Display::default().expect("ingen skjerm");
    crate::theme::load(&display);
    crate::timing::mark("css");
    let client = crate::client::Client::new(rigg_core::paths::socket_path(profile), profile);
    let app = App::new(gapp.clone(), profile, settings, client);
    // Det som står på disk nå: lukking uten endringer skriver ingenting.
    *app.saved_text.borrow_mut() = serde_json::to_string_pretty(&app.settings().to_value()).unwrap_or_default();
    if let Some(n) = note {
        app.notes.borrow_mut().push(n);
    }
    *state.borrow_mut() = Some(app.clone());
    gtk::Window::set_default_icon_name(&icon_name(&app));

    let shell = crate::window::Shell::new(&app);
    let _ = app.shell.set(shell.clone());
    crate::theme::apply(&app);
    crate::theme::watch_system(&app);
    crate::timing::mark("shell");
    let start = cli.page.clone().unwrap_or_else(|| app.settings().start());
    shell.go(&start);
    if let Some(sec) = &cli.settings {
        shell.open_settings(Some(sec));
    }
    crate::timing::mark("page");
    if crate::timing::on() {
        report_first_frame(&shell.window);
    }
    if std::env::var_os("RIGG_RAMMER").is_some() {
        frame_stats(&shell.window);
    }
    shell.window.present();
    crate::timing::mark("present");
    crate::logg_ui::attach(&app);
    crate::widgets::locked::watch_helper(&app);
    crate::skjermbilder::start(&app);

    // Etter første bilde: tilkobling, hilsen og meldinger fra oppstarten.
    let a = app.clone();
    glib::idle_add_local_once(move || {
        connect(&a);
        crate::easter::greet(&a);
        for n in a.notes.borrow_mut().drain(..) {
            a.toast(crate::toast::Toast::new("Innstillingene kunne ikke leses").body(&n).error().ms(12_000));
        }
    });
}

/// Første `after-paint` på frame clock: tid fra `exec` og fra `main()`.
/// Kobles til før `present()`, så realize ikke er passert.
fn report_first_frame(w: &gtk::ApplicationWindow) {
    let hook = move |w: &gtk::ApplicationWindow| {
        crate::timing::mark("realize");
        let Some(clock) = w.frame_clock() else { return };
        let done = std::rc::Rc::new(std::cell::Cell::new(0u8));
        let d1 = done.clone();
        clock.connect_layout(move |_| {
            if d1.get() == 0 {
                d1.set(1);
                crate::timing::mark("layout");
            }
        });
        let d2 = done.clone();
        clock.connect_paint(move |_| {
            if d2.get() == 1 {
                d2.set(2);
                crate::timing::mark("paint");
            }
        });
        let w3 = w.clone();
        clock.connect_after_paint(move |_| {
            if done.get() >= 3 {
                return;
            }
            done.set(3);
            crate::timing::mark("first_frame");
            let main_ms = crate::timing::since_main_ms().unwrap_or(f64::NAN);
            let r = w3.renderer().map(|r| r.type_().name().to_string()).unwrap_or_default();
            eprintln!("RIGG_TIMING main_to_first_frame_ms={main_ms:.1} exec_to_first_frame_ms={:.1} renderer={r}", since_exec_ms().unwrap_or(f64::NAN));
            // Måling av lukking: samme vei som ×-knappen (`gtk_window_close`).
            if let Some(ms) = std::env::var("RIGG_TIMING_LUKK").ok().and_then(|v| v.parse::<u64>().ok()) {
                let w4 = w3.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    crate::timing::mark("close");
                    w4.close();
                });
            }
        });
        w.connect_unmap(|_| crate::timing::mark("unmap"));
        w.connect_destroy(|_| crate::timing::mark("destroy"));
    };
    if w.is_realized() {
        hook(w);
    } else {
        w.connect_realize(move |w| hook(w));
    }
}

/// `RIGG_RAMMER=1`: bilder og tid fra «before-paint» til «after-paint» hvert 10. s
/// (feilsøking av CPU i ro, runde 3C).
fn frame_stats(w: &gtk::ApplicationWindow) {
    w.connect_realize(|w| {
        let Some(clock) = w.frame_clock() else { return };
        let st = Rc::new(std::cell::Cell::new((None::<std::time::Instant>, 0u32, 0.0f64, 0.0f64, std::time::Instant::now())));
        let s1 = st.clone();
        clock.connect_before_paint(move |_| {
            let mut v = s1.get();
            v.0 = Some(std::time::Instant::now());
            s1.set(v);
        });
        clock.connect_after_paint(move |_| {
            let mut v = st.get();
            if let Some(t) = v.0.take() {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                v.1 += 1;
                v.2 += ms;
                v.3 = v.3.max(ms);
            }
            if v.4.elapsed().as_secs() >= 10 {
                eprintln!("RIGG_RAMMER {} bilder på 10 s, {:.1} ms i alt, snitt {:.2} ms, verste {:.2} ms", v.1, v.2, if v.1 > 0 { v.2 / v.1 as f64 } else { 0.0 }, v.3);
                v = (None, 0, 0.0, 0.0, std::time::Instant::now());
            }
            st.set(v);
        });
    });
}

/// Millisekunder siden prosessen startet (`/proc/self/stat`, felt 22).
pub fn since_exec_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let start_ticks: f64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    // SAFETY: sysconf kan ikke feile på en måte som skader.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    Some((uptime - start_ticks / hz) * 1000.0)
}

// ---------------------------------------------------------------- Tilkoblingen

fn connect(app: &Rc<App>) {
    let a = app.clone();
    let b = app.clone();
    app.client.set_handlers(move |ev, data| on_event(&a, ev, data), move |up| on_conn(&b, up));
    app.client.start(app.gtk.dbus_connection());
}

fn on_conn(app: &Rc<App>, up: bool) {
    if !up {
        let c = Conn::Lost(rigg_core::now_ms());
        app.conn.set(c);
        app.on_conn.emit(&c);
        return;
    }
    let a = app.clone();
    app.client.request("hello", json!({"protocol": [1], "client": format!("agentic-rigg/{}", rigg_core::version::VERSION)}), move |res| {
        let Ok(h) = res else { return };
        a.data.borrow_mut().backend = h["daemon"]["backend"].as_str().unwrap_or("").to_string();
        a.data.borrow_mut().hello = Some(h);
        crate::timing::mark_once("hello");
        if let Some(s) = a.shell.get() {
            s.send_focus();
        }
        a.conn.set(Conn::Connected);
        a.on_conn.emit(&Conn::Connected);
        after_hello(&a);
    });
}

fn after_hello(app: &Rc<App>) {
    crate::widgets::locked::poll_helper(app);
    let iv = app.settings().interval_ms;
    app.client.request("subscribe", json!({"topics": crate::client::TOPICS, "intervalMs": iv}), |_| {});
    let a = app.clone();
    app.client.request("get_telemetry", json!({}), move |res| {
        if let Ok(v) = res
            && let Ok(t) = serde_json::from_value::<Telemetry>(v)
            && t.t > 0
        {
            crate::timing::mark_once("telemetry");
            a.ingest(t);
        }
    });
    let a = app.clone();
    app.client.request("list_gpus", json!({}), move |res| {
        if let Ok(v) = res {
            set_gpus(&a, &v["gpus"]);
            fetch_statics(&a);
            prefill(&a);
        }
    });
    let a = app.clone();
    app.client.request("get_settings", json!({}), move |res| {
        if let Ok(v) = res {
            a.data.borrow_mut().service = v;
            a.on_service.emit(&());
        }
    });
    let a = app.clone();
    app.client.request("get_stats", json!({}), move |res| {
        if let Ok(v) = res {
            a.data.borrow_mut().stats = v;
            a.on_stats.emit(&());
        }
    });
    let a = app.clone();
    app.client.request("log_status", json!({}), move |res| {
        if let Ok(v) = res {
            a.data.borrow_mut().log = v.clone();
            a.on_log.emit(&v);
        }
    });
    ping(app);
}

/// `ping` gir tjenestens PID, oppetid og minne (Innstillinger › Generelt og Om).
pub fn ping(app: &Rc<App>) {
    let a = app.clone();
    app.client.request("ping", json!({}), move |res| {
        if let Ok(v) = res {
            a.data.borrow_mut().ping = Some(v);
            a.on_service.emit(&());
        }
    });
}

fn set_gpus(app: &Rc<App>, v: &Value) {
    let list: Vec<GpuInfo> = match serde_json::from_value(v.clone()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("list_gpus kunne ikke leses: {e}");
            vec![]
        }
    };
    {
        let mut d = app.data.borrow_mut();
        if d.gpu.as_ref().is_none_or(|g| !list.iter().any(|x| &x.id == g)) {
            d.gpu = list.iter().find(|g| !g.id.is_empty()).map(|g| g.id.clone());
        }
        d.gpus = list;
    }
    app.on_devices.emit(&());
}

/// De faste dataene: spør igjen til de er komplette (de leses gradvis).
fn fetch_statics(app: &Rc<App>) {
    let ids: Vec<String> = app.data.borrow().gpus.iter().filter(|g| !g.id.is_empty()).map(|g| g.id.clone()).collect();
    for id in ids {
        let a = app.clone();
        let id2 = id.clone();
        app.client.request("get_static", json!({"gpu": id}), move |res| {
            let Ok(v) = res else { return };
            let complete = v["complete"].as_bool().unwrap_or(false);
            if let Ok(st) = serde_json::from_value::<GpuStatic>(v["static"].clone()) {
                a.data.borrow_mut().statics.insert(id2.clone(), (st, complete));
                a.on_devices.emit(&());
            }
            if !complete {
                let a2 = a.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || fetch_statics(&a2));
            }
        });
    }
}

/// Sparklinjene i statusblokken og på Oversikt fylles fra historikken.
fn prefill(app: &Rc<App>) {
    let mut keys: Vec<String> = vec!["cpu/temp".into(), "cpu/load".into(), "mem/usedPct".into(), "net/rx".into(), "net/tx".into(), "disk/read".into(), "disk/write".into()];
    for f in ["temp", "gpuUtil", "power", "fan", "vramUsed", "coreClock"] {
        if let Some(k) = app.gpu_key(f) {
            keys.push(k);
        }
    }
    for chunk in keys.chunks(16) {
        let a = app.clone();
        app.client.request("get_history", json!({"window": 60, "keys": chunk, "points": 120}), move |res| {
            let Ok(v) = res else { return };
            let t: Vec<i64> = serde_json::from_value(v["t"].clone()).unwrap_or_default();
            if let Some(series) = v["series"].as_object() {
                for (k, s) in series {
                    let avg: Vec<Option<f64>> = serde_json::from_value(s["avg"].clone()).unwrap_or_default();
                    let pts: Vec<(i64, f64)> = t.iter().zip(avg).filter_map(|(t, v)| v.map(|v| (*t, v))).collect();
                    a.prefill(k, pts);
                }
            }
            a.on_telemetry.emit(&());
        });
    }
}

fn on_event(app: &Rc<App>, ev: &str, data: &Value) {
    match ev {
        "telemetry" => {
            if let Ok(t) = serde_json::from_value::<Telemetry>(data.clone()) {
                crate::timing::mark_once("telemetry");
                app.ingest(t);
            }
        }
        "device" => {
            set_gpus(app, &data["gpus"]);
            fetch_statics(app);
        }
        "stats" => {
            app.data.borrow_mut().stats = data.clone();
            app.on_stats.emit(&());
        }
        "log" => {
            app.data.borrow_mut().log = data.clone();
            app.on_log.emit(data);
        }
        "alarm" => app.on_alarm.emit(data),
        "txn" => app.on_txn.emit(data),
        "settings" => {
            app.data.borrow_mut().service = data.clone();
            app.on_service.emit(&());
        }
        "shutdown" => {}
        _ => {}
    }
}
