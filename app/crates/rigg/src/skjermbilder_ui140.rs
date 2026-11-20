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
    static ORIGINAL_FAN_COUNT: Cell<Option<u32>> = const { Cell::new(None) };
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

fn page_scroll(app: &App, page_shell: &str) -> gtk::ScrolledWindow {
    mapped(&class(app, page_shell), &|w| w.is::<gtk::ScrolledWindow>()).unwrap().downcast().unwrap()
}

fn assert_at_bottom(app: &App, page_shell: &str) {
    let adj = page_scroll(app, page_shell).vadjustment();
    assert!(adj.value() > 50.0, "prøven må faktisk ha rullet ned");
    assert!((adj.value() - (adj.upper() - adj.page_size())).abs() < 2.0);
}

fn assert_sticky_keep(app: &Rc<App>, page_shell: &str) {
    let win = app.window().unwrap();
    let keep = find_button(win.upcast_ref(), "Behold*").expect("Behold må finnes etter Bruk");
    assert!(keep.is_mapped() && keep.is_sensitive());
    let bounds = keep.compute_bounds(&win).unwrap();
    let viewport = page_scroll(app, page_shell).compute_bounds(&win).unwrap();
    assert!(bounds.x() >= 0.0 && bounds.y() >= 0.0);
    assert!(bounds.x() + bounds.width() <= win.width() as f32 + 1.0);
    assert!(bounds.y() + bounds.height() <= viewport.y() + 1.0, "Behold ligger inne i eller under rulleområdet");
    assert!(bounds.y() + bounds.height() <= win.height() as f32);
    eprintln!("RIGG_SKJERMBILDER sticky Behold {page_shell}: x={} y={} w={} h={}, viewport-y={}", bounds.x(), bounds.y(), bounds.width(), bounds.height(), viewport.y());
}

fn change_fan_view(app: &Rc<App>, advanced: bool) {
    let before = locked::stored(app);
    let mode = crate::pages::viftekurve::card_values(app).fan_mode;
    find_button(app.window().unwrap().upcast_ref(), if advanced { "Avansert" } else { "Enkel" }).unwrap().emit_clicked();
    let after = locked::stored(app);
    assert_eq!(after.fan_advanced, advanced);
    assert_eq!(before.utkast, after.utkast, "visningsbytte endret utkast");
    assert_eq!(before.utkast_base, after.utkast_base);
    assert_eq!(before.utkast_gpu, after.utkast_gpu);
    assert_eq!(crate::pages::viftekurve::card_values(app).fan_mode, mode, "visningsbytte endret viftemodus");
}

fn assert_manual_kept(app: &Rc<App>) {
    assert_eq!(locked::gate(app), rigg_core::txn::Gate::Sim);
    app.client.request("get_state", json!({"gpu": app.gpu_id()}), |r| {
        let state = r.expect("simtilstand etter Behold");
        assert!(state["pending"].is_null());
        if state["activeValues"].is_object() {
            assert_eq!(state["activeValues"]["fanMode"], "manual");
            assert_eq!(state["activeValues"]["fanPercent"], 55.0);
        } else {
            assert_eq!(state["active"]["fan"]["mode"], "manual");
            assert_eq!(state["active"]["fan"]["manualPercent"], 55.0);
        }
        eprintln!("RIGG_SKJERMBILDER Manuell 55 % beholdt etter >70s, pending=null");
    });
}

/// Endrer bare privat sim-UI-metadata, aldri kort, utkast eller helperconfig.
fn fan_count_fixture(app: &Rc<App>, count: Option<u32>) {
    assert_eq!(locked::gate(app), rigg_core::txn::Gate::Sim);
    let before = locked::stored(app);
    let gpu = app.gpu_id().unwrap();
    app.data.borrow_mut().statics.get_mut(&gpu).unwrap().0.fan_count = count;
    app.on_devices.emit(&());
    let after = locked::stored(app);
    assert_eq!(before.utkast, after.utkast, "antallsbytte endret utkastet");
    assert_eq!(before.utkast_base, after.utkast_base);
    assert_eq!(before.utkast_gpu, after.utkast_gpu);
    eprintln!("RIGG_SKJERMBILDER SIMULERT UI-vifteantall: {count:?}");
}

fn assert_fan_target_choices(app: &Rc<App>, count: usize) {
    // Ved én vifte er velgeren med hensikt skjult. Søk derfor også i
    // skjulte barn, men bare i kurvevelgeren, aldri i historikklegendene.
    fn selector(w: &gtk::Widget) -> Option<gtk::Widget> {
        if w.has_css_class("target-seg") { return Some(w.clone()); }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(found) = selector(&c) { return Some(found); }
            child = c.next_sibling();
        }
        None
    }
    let root = selector(&class(app, "viftekurve")).expect("kurvevelgeren skal være bygd");
    let mut labels = Vec::new();
    let mut child = root.first_child();
    while let Some(c) = child {
        if let Some(button) = c.downcast_ref::<gtk::ToggleButton>() {
            labels.push(button.label().unwrap().to_string());
            if count > 1 { assert!(button.is_sensitive() && button.is_mapped()); }
        }
        child = c.next_sibling();
    }
    let mut expected = vec!["Felles kurve".to_string()];
    expected.extend((1..=count).map(|i| format!("Vifte {i}")));
    assert_eq!(labels, expected, "valg må følge faktisk vifteantall");
    assert_eq!(root.is_mapped(), count > 1);
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

fn version_log_window(app: &App) -> gtk::Window {
    let windows: Vec<_> = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<gtk::Window>().ok())
        .filter(|w| w.is_mapped() && w.title().as_deref() == Some("Versjonslogg")).collect();
    assert_eq!(windows.len(), 1, "én native Versjonslogg skal være åpen");
    let window = windows.into_iter().next().unwrap();
    assert!(window.is_modal(), "Versjonslogg skal være modal");
    assert_eq!(window.transient_for(), app.window().map(|w| w.upcast::<gtk::Window>()), "Versjonslogg skal tilhøre hovedvinduet");
    window
}

fn version_headings(w: &gtk::Widget, out: &mut Vec<gtk::Label>) {
    if let Some(label) = w.downcast_ref::<gtk::Label>()
        && label.has_css_class("group-title") && label.text().contains(" · bygg ")
    { out.push(label.clone()); }
    let mut child = w.first_child();
    while let Some(c) = child { version_headings(&c, out); child = c.next_sibling(); }
}

fn assert_version_log(app: &Rc<App>, bottom: bool) {
    let win = version_log_window(app);
    let mut headers = Vec::new();
    version_headings(win.upcast_ref(), &mut headers);
    assert!(headers.len() >= 9, "hele desktophistorikken skal være med");
    assert!(headers.first().unwrap().text().starts_with("1.5.0 · bygg "), "nyeste utgave skal være øverst");
    assert!(headers.last().unwrap().text().starts_with("1.0.0 · bygg "), "første utgave skal være nederst");
    let visible = if bottom { headers.last().unwrap() } else { headers.first().unwrap() };
    let rect = visible.compute_bounds(&win).unwrap();
    assert!(visible.is_mapped() && rect.y() >= 0.0 && rect.y() + rect.height() <= win.height() as f32, "riktig ende av historikken skal være i modalens viewport");
}

fn version_log_shot(app: &Rc<App>, name: &str) {
    let path = PathBuf::from(std::env::var_os("RIGG_SKJERMBILDER").unwrap()).join(format!("{name}.png"));
    super::shot(&version_log_window(app), &path);
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
        Size(1920, 1080), Set(base()), Settings("generelt"), Wait(500), Click("Versjonslogg"), Wait(400),
        Eval(|app| assert_version_log(app, false)), Eval(|app| version_log_shot(app, "versjonslogg-footer-nyeste-lys")),
        Eval(|app| {
            let win = version_log_window(app);
            let sc = mapped(win.upcast_ref(), &|w| w.is::<gtk::ScrolledWindow>()).unwrap().downcast::<gtk::ScrolledWindow>().unwrap();
            let adj = sc.vadjustment(); adj.set_value(adj.upper() - adj.page_size());
        }), Wait(300), Eval(|app| assert_version_log(app, true)), Eval(|app| version_log_shot(app, "versjonslogg-footer-eldste-lys")),
        Eval(|app| version_log_window(app).close()), Wait(200),
        Set(json!({"theme": "dark"})), Size(640, 760), Settings("om"), Wait(500), Click("Vis versjonslogg …"), Wait(400),
        Eval(|app| version_log_window(app).set_default_size(640, 640)), Wait(400),
        Eval(|app| assert_version_log(app, false)), Eval(|app| version_log_shot(app, "versjonslogg-om-mork-640")),
        Eval(|app| version_log_window(app).close()), Wait(200), CloseSettings,
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
        Go("kontroller"), Wait(700), Shot("klokker-enkel-lys"), Size(1440, 760), Wait(500),
        Eval(|app| {
            let advanced = mapped(&class(app, "kontroller"), &|w| w.downcast_ref::<gtk::Expander>().is_some_and(|e| e.label().is_some_and(|s| s.starts_with("Avansert · temperaturmål"))))
                .expect("klokkekontrollenes avanserte rader må finnes").downcast::<gtk::Expander>().unwrap();
            advanced.set_expanded(true);
        }), Wait(400),
        Eval(|app| {
            let base = crate::pages::kontroller::read(app).values();
            let mut draft = base.clone(); draft.power_limit_w = 220.0;
            locked::set_draft(app, base, draft);
            let saved = locked::stored(app);
            assert!(saved.utkast_base.is_some() && saved.utkast_gpu.is_some(), "utkastet må ha binding");
        }), Wait(300), Scroll(1.0), Wait(400), Eval(|app| assert_at_bottom(app, "control-page-shell")), Shot("kontroller-bunn-foer-bruk"),
        Click("Bruk"), Wait(600), Eval(|app| assert_sticky_keep(app, "control-page-shell")), Shot("sim-bruk-venter-behold"), Click("Behold*"), Wait(600),
        Eval(|app| assert!(locked::stored(app).utkast.is_none())),
        Size(1920, 1080), Go("viftekurve"), Wait(700), Shot("vifter-enkel-auto-lys"),
        Eval(|app| { assert!(!locked::fan_advanced(app)); assert!(mapped(app.window().unwrap().upcast_ref(), &|w| w.has_css_class("curve-card")).is_none()); }),
        Click("Fast fart"), Wait(300),
        Eval(|app| {
            let spin = mapped(&class(app, "manual-row"), &|w| w.is::<gtk::SpinButton>()).unwrap().downcast::<gtk::SpinButton>().unwrap();
            spin.set_value(55.0);
        }), Wait(300), Shot("vifter-enkel-fast-fart-utkast-lys"),
        Eval(|app| change_fan_view(app, true)), Wait(600), Shot("vifter-avansert-manuell-lys"),
        Scroll(1.0), Wait(400), Eval(|app| assert_at_bottom(app, "fan-page-shell")), Shot("vifter-bunn-foer-bruk"),
        Click("Bruk"), Wait(600), Eval(|app| assert_sticky_keep(app, "fan-page-shell")), Shot("vifter-bunn-sticky-behold-1920"), Click("Behold*"),
        Wait(71_000), Eval(assert_manual_kept), Scroll(0.0), Wait(400), Eval(|app| change_fan_view(app, false)), Wait(400), Shot("vifter-enkel-beholdt-etter-70s"),
        Size(640, 760), Wait(500), Shot("vifter-enkel-fast-fart-640-lys"),
        Eval(|app| change_fan_view(app, true)), Wait(500),
        Eval(|app| {
            let base = crate::pages::viftekurve::card_values(app);
            let mut draft = base.clone(); draft.fan_percent = 60.0; draft.fan_mode = rigg_core::txn::FanMode::Manual;
            locked::set_draft(app, base, draft);
        }), Wait(300), Scroll(1.0), Wait(400), Eval(|app| assert_at_bottom(app, "fan-page-shell")),
        Click("Bruk"), Wait(600), Eval(|app| assert_sticky_keep(app, "fan-page-shell")), Shot("vifter-bunn-sticky-behold-640"), Click("Rull tilbake"), Wait(600),
        Eval(|app| locked::update(app, "utkast", |s| s.utkast = None)), Scroll(0.0),
        Eval(|app| change_fan_view(app, false)), Size(1920, 1080), Wait(600), Shot("vifter-sim-aktivert-lys"),
        Eval(|app| { ORIGINAL_FAN_COUNT.with(|n| n.set(app.statics().and_then(|s| s.fan_count))); change_fan_view(app, true); fan_count_fixture(app, None); }),
        Wait(300), Eval(|app| assert!(!find_button(app.window().unwrap().upcast_ref(), "Auto*").unwrap().is_sensitive())), Shot("vifter-simulert-antall-ukjent"),
        Eval(|app| fan_count_fixture(app, Some(0))), Wait(300), Eval(|app| assert!(!find_button(app.window().unwrap().upcast_ref(), "Auto*").unwrap().is_sensitive())), Shot("vifter-simulert-null-vifter"),
        Eval(|app| fan_count_fixture(app, Some(1))), Wait(300), Eval(|app| assert_fan_target_choices(app, 1)), Shot("vifter-simulert-en-vifte"),
        Eval(|app| fan_count_fixture(app, Some(3))), Wait(300), Eval(|app| assert_fan_target_choices(app, 3)),
        Size(640, 760), Wait(400), Size(640, 760), Wait(400),
        Eval(|app| assert!(app.window().unwrap().width() <= 640, "tre-viftevisningen krever mer enn 640 px etter to størrelsesrunder: {}", app.window().unwrap().width())),
        Shot("vifter-simulert-tre-vifter-640"),
        Eval(|app| fan_count_fixture(app, ORIGINAL_FAN_COUNT.with(Cell::get))), Eval(|app| change_fan_view(app, false)), Size(1920, 1080),
        Go("lys"), Wait(700),
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
