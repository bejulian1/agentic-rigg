//! Privat native GTK-regresjon for 1.4.0. Daemon og helper kjører bare sim.
//! Pointerprøvene bruker GTKs levende pick-tre og capture-controllerens signal;
//! de erstatter ikke en ekstern Wayland-test av fysisk input/propagering.
//! NVML/ready nedenfor er bare en tydelig logget UI-statusfixture. Ingen
//! Bruk-/Behold-/reset-handler kalles mens denne fixturen er aktiv.

use super::*;
use crate::widgets::locked;
use std::cell::Cell;

thread_local! {
    static SENSOR_ROWS: Cell<usize> = const { Cell::new(0) };
    static MOCK_STATUS: Cell<i8> = const { Cell::new(-1) };
    static MOCK_BUSY: Cell<bool> = const { Cell::new(false) };
}

fn mapped(w: &gtk::Widget, predicate: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if !w.is_mapped() { return None; }
    if predicate(w) { return Some(w.clone()); }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(hit) = mapped(&c, predicate) { return Some(hit); }
        child = c.next_sibling();
    }
    None
}

fn class(app: &App, name: &str) -> gtk::Widget {
    mapped(app.window().unwrap().upcast_ref(), &|w| w.has_css_class(name)).unwrap_or_else(|| panic!("1.4.0-ui: mangler synlig {name}"))
}

fn search(app: &App) -> gtk::SearchEntry { class(app, "sensor-filter").downcast().unwrap() }

fn row_count(w: &gtk::Widget) -> usize {
    if !w.is_mapped() { return 0; }
    let mut n = usize::from(w.has_css_class("sn-row"));
    let mut child = w.first_child();
    while let Some(c) = child { n += row_count(&c); child = c.next_sibling(); }
    n
}

fn assert_filter_cleared(app: &Rc<App>) {
    assert!(search(app).text().is_empty(), "råfilter kom tilbake");
    assert!(!search(app).state_flags().contains(gtk::StateFlags::FOCUS_WITHIN));
    assert_eq!(row_count(app.window().unwrap().upcast_ref()), SENSOR_ROWS.with(Cell::get), "forsinket filter skjuler rader");
}

fn sensor_escape(app: &Rc<App>) {
    assert!(app.shell().page_key(&crate::pages::Key { key: gtk::gdk::Key::Escape, ctrl: false, shift: false, alt: false, typing: true }));
}

/// Samme capture-handler som en pointerhendelse, med faktisk GTK-pick.
fn capture_press(app: &App, target: &gtk::Widget) {
    let win = app.window().unwrap();
    let rect = target.compute_bounds(&win).unwrap();
    let (x, y) = ((rect.x() + rect.width() / 2.0) as f64, (rect.y() + rect.height() / 2.0) as f64);
    let controllers = win.observe_controllers();
    let mut count = 0;
    for i in 0..controllers.n_items() {
        if let Some(g) = controllers.item(i).and_then(|c| c.downcast::<gtk::GestureClick>().ok())
            && g.propagation_phase() == gtk::PropagationPhase::Capture && g.button() == gtk::gdk::BUTTON_PRIMARY
        {
            g.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
            count += 1;
        }
    }
    assert!(count > 0, "Sensorer mangler capture-controller");
}

fn time_dropdown(app: &App) -> gtk::DropDown { class(app, "win-select").downcast().unwrap() }
fn time_popup(app: &App) -> gtk::Popover {
    mapped(time_dropdown(app).upcast_ref(), &|w| w.is::<gtk::Popover>()).unwrap().downcast().unwrap()
}

fn menu_shot(app: &Rc<App>, name: &str) {
    let pop = time_popup(app);
    let snap = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(&pop)).snapshot(&snap, pop.width() as f64, pop.height() as f64);
    let tex = pop.renderer().unwrap().render_texture(snap.to_node().unwrap(), None);
    let dir = PathBuf::from(std::env::var_os("RIGG_SKJERMBILDER").unwrap());
    tex.save_to_png(dir.join(format!("{name}.png"))).unwrap();
    eprintln!("RIGG_SKJERMBILDE {name}: native dropdown {}×{}", pop.width(), pop.height());
}

fn mock_status(app: &Rc<App>, ready: bool) {
    MOCK_STATUS.with(|s| s.set(i8::from(ready)));
    apply_mock_status(app);
    app.on_service.emit(&());
}

fn apply_mock_status(app: &Rc<App>) {
    let mode = MOCK_STATUS.with(Cell::get);
    if mode < 0 { return; }
    let pci = app.gpu_id().unwrap();
    let uuid = app.statics().and_then(|s| s.uuid).unwrap();
    let ready = mode == 1;
    {
        let mut data = app.data.borrow_mut();
        data.hello.as_mut().unwrap()["daemon"]["backend"] = json!("nvml");
    }
    let mut caps = json!({});
    for key in ["power", "core_clock", "mem_clock", "fan_manual", "fan_curve", "temp_target", "rgb"] {
        let available = !matches!(key, "temp_target" | "rgb");
        caps[key] = json!({"available": available, "reason": if available { "Simulert UI-støtte" } else { "SIMULERT UI-STATUS: funksjonen er utilgjengelig i denne testen." }});
    }
    locked::set_helper(app, json!({"state": if ready {"on"} else {"off"}, "enabled": ready, "ready": ready, "realWrites": ready,
        "backend": "nvml", "identity": {"pci": pci, "uuid": uuid}, "capabilities": caps,
        "message": "SIMULERT UI-STATUS. Daemon/helper kjører sim; ingen maskinvare endres."}));
}

fn install_mock_hooks(app: &Rc<App>) {
    assert!(app.profile.is_dev());
    assert_eq!(app.data.borrow().hello.as_ref().unwrap()["daemon"]["backend"], "sim");
    let a = Rc::downgrade(app);
    app.on_service.connect(move |_| {
        if MOCK_STATUS.with(Cell::get) < 0 || MOCK_BUSY.with(|b| b.replace(true)) { return; }
        if let Some(app) = a.upgrade() { apply_mock_status(&app); app.on_service.emit(&()); }
        MOCK_BUSY.with(|b| b.set(false));
    });
    let a = Rc::downgrade(app);
    app.on_telemetry.connect(move |_| {
        if MOCK_STATUS.with(Cell::get) < 0 { return; }
        if let Some(app) = a.upgrade() { apply_mock_status(&app); app.on_service.emit(&()); }
    });
    eprintln!("RIGG_SKJERMBILDER 1.4.0-ui: SIMULERT statusfixture, ingen ekte writes");
}

fn stop_mock(app: &Rc<App>) {
    MOCK_STATUS.with(|s| s.set(-1));
    app.data.borrow_mut().hello.as_mut().unwrap()["daemon"]["backend"] = json!("sim");
    app.data.borrow_mut().hello.as_mut().unwrap()["capabilities"]["write"]["state"] = json!("sim");
    locked::refresh_helper(app, crate::settings::request_render);
}

pub(super) fn plan() -> Vec<Step> {
    use Step::*;
    vec![
        Eval(|app| {
            assert!(app.profile.is_dev());
            assert!(std::env::var("RIGG_PKEXEC").unwrap().ends_with("/falsk-pkexec.sh"));
            assert_ne!(std::env::var("RIGG_HJELPER_ROT").unwrap(), "/");
            assert_eq!(app.data.borrow().hello.as_ref().unwrap()["daemon"]["backend"], "sim");
        }),
        Size(1920, 1080), Set(base()), Go("sensorer"), Wait(900),
        Eval(|app| {
            SENSOR_ROWS.with(|n| n.set(row_count(app.window().unwrap().upcast_ref())));
            assert!(SENSOR_ROWS.with(Cell::get) > 0);
            let entry = search(app);
            entry.grab_focus();
            entry.set_text("ingen-sensortreff-140");
            // Ingen venting/mainloop mellom tekst og Esc: mindre enn 150 ms.
            sensor_escape(app);
            assert!(entry.text().is_empty());
        }),
        Wait(400), Eval(assert_filter_cleared), Shot("sensorer-esc-for-debounce-lys"),
        Eval(|app| { let s = search(app); s.grab_focus(); s.set_text("gpu"); }),
        Wait(400), Shot("sensorer-filter-aktivt-lys"),
        Eval(|app| {
            let entry = search(app);
            capture_press(app, entry.upcast_ref());
            assert!(entry.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN), "klikk i filteret mistet fokus");
            capture_press(app, &class(app, "main-title"));
            assert!(!entry.state_flags().contains(gtk::StateFlags::FOCUS_WITHIN), "tom topplinje slapp ikke fokus");
            assert_eq!(entry.text(), "gpu", "outsideclick skal beholde filterteksten");
        }),
        Shot("sensorer-klikk-utenfor-uten-fokus-lys"),
        Eval(|app| { sensor_escape(app); }), Wait(350), Eval(assert_filter_cleared),
        Eval(|app| {
            let entry = search(app); entry.grab_focus(); entry.set_text("ingen-sensortreff-140");
            entry.emit_by_name::<()>("stop-search", &[]);
        }), Wait(400), Eval(assert_filter_cleared),
        Go("grafer"), Wait(800),
        Eval(|app| { let dd = time_dropdown(app); assert!(!dd.enables_search()); assert_eq!(dd.model().unwrap().n_items(), 7); dd.emit_activate(); }),
        Wait(350), Eval(|app| menu_shot(app, "tidsmeny-peker-lys")), Shot("grafer-tidsmeny-vindu-lys"),
        Eval(|app| { let pop = time_popup(app); pop.child_focus(gtk::DirectionType::TabForward); }),
        Wait(200), Eval(|app| menu_shot(app, "tidsmeny-tabfokus-lys")),
        Eval(|app| { time_popup(app).popdown(); time_dropdown(app).set_selected(1); }), Wait(300),
        Eval(|app| { assert_eq!(time_dropdown(app).selected(), 1); time_dropdown(app).emit_activate(); }),
        Wait(300), Eval(|app| menu_shot(app, "tidsmeny-5min-valgt-lys")),
        Eval(|app| time_popup(app).popdown()), Set(json!({"theme": "dark"})), Wait(300),
        Eval(|app| time_dropdown(app).emit_activate()), Wait(300), Eval(|app| menu_shot(app, "tidsmeny-peker-mork")),
        Eval(|app| time_popup(app).popdown()), Set(base()),
        Settings("styring"), Wait(700), Shot("globalaktivering-av-sim-lys"),
        Eval(|app| crate::settings::styring::toggle(app, true)), Wait(5000),
        Eval(|app| { assert_eq!(locked::helper_state(app), "on"); assert_eq!(locked::gate(app), rigg_core::txn::Gate::Sim); }),
        Shot("globalaktivering-pa-sim-lys"), Set(json!({"theme": "dark"})), Wait(400), Shot("globalaktivering-pa-sim-mork"),
        Set(json!({"theme": "light"})), Wait(300), Scroll(1.0), Wait(300), Shot("funksjonsstotte-sim-lys"), Scroll(0.0), CloseSettings,
        Go("kontroller"), Wait(500),
        Eval(|app| {
            let base = crate::pages::kontroller::read(app).values();
            let mut draft = base.clone(); draft.power_limit_w = 220.0;
            locked::set_draft(app, base, draft);
            let saved = locked::stored(app);
            assert!(saved.utkast_base.is_some() && saved.utkast_gpu.is_some(), "utkastet må ha binding");
        }), Wait(300), Click("Bruk"), Wait(600), Shot("sim-bruk-venter-behold"), Click("Behold*"), Wait(600),
        Eval(|app| assert!(locked::stored(app).utkast.is_none())),
        Go("viftekurve"), Wait(700), Shot("vifter-sim-aktivert-lys"), Go("lys"), Wait(700),
        Eval(install_mock_hooks), Eval(|app| mock_status(app, true)), Wait(500),
        Eval(|app| { assert!(locked::control_ready(app)); assert!(!locked::supports(app, "rgb")); assert!(mapped(app.window().unwrap().upcast_ref(), &|w| w.has_css_class("nav-lock")).is_none()); }),
        Shot("rgb-utilgjengelig-simulert-klar-status"), Go("kontroller"), Wait(500), Shot("klokker-simulert-klar-status"),
        Go("viftekurve"), Wait(500), Shot("vifter-simulert-klar-status"),
        Eval(|app| mock_status(app, false)), Wait(350),
        Eval(|app| { assert!(!locked::control_ready(app)); assert!(mapped(app.window().unwrap().upcast_ref(), &|w| w.has_css_class("nav-lock")).is_some()); }),
        Shot("styring-av-umiddelbar-status"), Eval(stop_mock), Wait(600),
        Settings("styring"), Wait(400), Eval(|app| crate::settings::styring::toggle(app, false)), Wait(5000),
        Eval(|app| assert_eq!(locked::helper_state(app), "off")), Shot("globalaktivering-av-igjen-sim"), CloseSettings,
        Go("lys"), Click("2D"), Wait(700), Shot("rgb-pcie-2d-lys"),
        Click("3D"), Wait(6000),
        Eval(|_| { let info = crate::pages::lys::debug_info(); assert!(crate::pages::lys::debug_ready() && !info.starts_with("2D"), "3D krever GL-renderer, ikke cairo: {info}"); }),
        Shot("rgb-pcie-3d-hel-lys"), Click("Bare kretskort"), Click("Forfra"), Wait(600), Shot("rgb-pcie-3d-kretskort-forfra-lys"),
        Click("Bakfra"), Wait(600), Shot("rgb-pcie-3d-kretskort-bakfra-lys"), Set(json!({"theme": "dark"})), Wait(400),
        Shot("rgb-pcie-3d-kretskort-bakfra-mork"), Click("Hel"), Click("Skrått"), Wait(600), Shot("rgb-pcie-3d-hel-mork"),
        Click("2D"), Wait(500), Shot("rgb-pcie-2d-mork"),
        Eval(|_| eprintln!("RIGG_SKJERMBILDER 1.4.0-ui: alle assertions fullført; alle styringsverdier/statusbilder er simulerte")),
    ]
}
