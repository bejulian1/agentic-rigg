//! Privat GTK-kontroll av Oppgaver. En eventuell signalprøve bruker bare
//! sleep-barnet som den eksterne prøverunneren opprettet for denne økta.
use super::Step;
use crate::state::App;
use gtk::prelude::*;
use serde_json::json;
use std::rc::Rc;

fn widget(app: &App, class: &str) -> gtk::Widget {
    super::find_class(app.window().unwrap().upcast_ref(), class).unwrap_or_else(|| panic!("Mangler {class}"))
}

fn list(app: &App) -> gtk::ListBox {
    widget(app, "task-rows").downcast().unwrap()
}

fn search(app: &App, text: &str) {
    widget(app, "task-search").downcast::<gtk::SearchEntry>().unwrap().set_text(text);
}

fn rows(app: &App) -> Vec<gtk::ListBoxRow> {
    let mut out = Vec::new();
    let mut child = list(app).first_child();
    while let Some(c) = child {
        if let Some(r) = c.downcast_ref::<gtk::ListBoxRow>().filter(|r| r.is_visible()) {
            out.push(r.clone());
        }
        child = c.next_sibling();
    }
    out
}

fn button(app: &App, class: &str) -> gtk::Button {
    widget(app, class).downcast().unwrap()
}

fn assert_layout(app: &Rc<App>) {
    let win = app.window().unwrap();
    for class in ["task-summaries", "task-controls", "task-pager", "task-footer", "task-status"] {
        let w = widget(app, class);
        let r = w.compute_bounds(&win).unwrap();
        assert!(
            w.is_mapped() && r.y() >= 0.0 && r.y() + r.height() <= win.height() as f32 + 1.0,
            "{class} skal være synlig uten siderulling"
        );
    }
    assert!(widget(app, "task-table").height() > 100, "Tabellen må få tilgjengelig høyde");
    assert!(!rows(app).is_empty(), "Minst én ekte prosess skal være synlig");
    assert!(rows(app).len() <= 100, "Høyst 100 gjenbrukte rader");
}

fn dialog() -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.is_visible() && w.title().as_deref() == Some("Avslutt oppgave?"))
        .expect("Bekreftelsesdialog")
}

fn click_dialog(text: &str) {
    let win = dialog();
    let b = super::find_button(win.upcast_ref(), text).unwrap();
    assert!(b.is_sensitive());
    b.emit_clicked();
}

fn own_pid() -> u32 {
    std::env::var("RIGG_TASK_TEST_PID").expect("Runnerens sleep-PID").parse().unwrap()
}

fn assert_owned_test_process() {
    let parent = |text: &str| {
        text.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|pid| pid.parse::<u32>().ok())
            .unwrap()
    };
    let own = std::fs::read_to_string("/proc/self/stat").unwrap();
    let target = std::fs::read_to_string(format!("/proc/{}/stat", own_pid())).unwrap();
    assert_eq!(parent(&own), parent(&target), "Signalprøven krever et søsken startet av samme private runner");
    let exe = std::fs::read_link(format!("/proc/{}/exe", own_pid())).unwrap();
    let sleep = std::fs::canonicalize("/usr/bin/sleep").unwrap();
    assert_eq!(exe, sleep, "Signalprøven tillater bare runnerens sleep-binær, også når Ubuntu bruker en symlink");
    let name = target.split_once('(').unwrap().1.rsplit_once(')').unwrap().0;
    assert_eq!(name, "sleep", "Bare runnerens sleep-kommando kan brukes i signalprøven");
}

pub(super) fn plan() -> Vec<Step> {
    use Step::*;
    let mut steps = vec![
        Size(1440, 900),
        Set(super::base()),
        Go("oppgaver"),
        Wait(7000),
        Eval(assert_layout),
        Shot("oppgaver-lys-1440"),
        Eval(|a| widget(a, "task-tree").downcast::<gtk::ToggleButton>().unwrap().set_active(true)),
        Wait(600),
        Shot("oppgaver-prosesstre"),
        Eval(|a| super::find_button(a.window().unwrap().upcast_ref(), "PID").unwrap().emit_clicked()),
        Wait(600),
        Shot("oppgaver-sortering-pid"),
        Eval(|a| search(a, "finnes-ikke-agentic-rigg-xyz")),
        Wait(700),
        Eval(|a| {
            assert!(rows(a).is_empty());
            assert!(!button(a, "task-stop").is_sensitive());
        }),
        Shot("oppgaver-ingen-treff"),
        Eval(|a| {
            search(a, "");
            widget(a, "task-tree").downcast::<gtk::ToggleButton>().unwrap().set_active(false);
        }),
        Wait(700),
        Set(json!({"theme":"dark"})),
        Wait(600),
        Eval(assert_layout),
        Shot("oppgaver-mork-1440"),
        Size(640, 760),
        Wait(600),
        Size(640, 760),
        Wait(800),
        Eval(|a| {
            assert!(a.window().unwrap().width() <= 640);
            assert_layout(a);
        }),
        Shot("oppgaver-mork-640"),
        Set(json!({"theme":"light","fontScale":116})),
        Wait(700),
        Eval(assert_layout),
        Shot("oppgaver-lys-640-stor-skrift"),
        Set(super::base()),
        Size(1440, 900),
        Wait(700),
        Eval(|a| widget(a, "task-pause").downcast::<gtk::ToggleButton>().unwrap().set_active(true)),
        Wait(600),
        Shot("oppgaver-pause"),
        Wait(5500),
        Eval(|a| {
            assert!(a.shell().page_key(&crate::pages::Key {
                key: gtk::gdk::Key::F5,
                ctrl: false,
                shift: false,
                alt: false,
                typing: false
            }));
        }),
        Wait(3300),
        Eval(|a| {
            assert!(!rows(a).is_empty(), "F5 etter utløpt behov skal fullføre også under pause");
            let text = widget(a, "task-status").downcast::<gtk::Label>().unwrap().text();
            assert!(text.starts_with("Oppdatering pauset"), "Oppdateringen skal ha fullført og beholdt pause: {text}");
        }),
        Shot("oppgaver-pause-f5"),
        Eval(|a| widget(a, "task-pause").downcast::<gtk::ToggleButton>().unwrap().set_active(false)),
        Wait(1000),
    ];
    if std::env::var_os("RIGG_TASK_TEST_PID").is_some() {
        steps.extend([
            Eval(|a| {
                assert_owned_test_process();
                search(a, &own_pid().to_string());
            }),
            Wait(700),
            Eval(|a| {
                let prefix = format!("task-{}-", own_pid());
                let row = rows(a).into_iter().find(|r| r.widget_name().starts_with(&prefix)).expect("Runnerens sleep-prosess");
                list(a).select_row(Some(&row));
                assert!(button(a, "task-stop").is_sensitive(), "Eget ufarlig barn skal kunne avsluttes");
                button(a, "task-stop").emit_clicked();
            }),
            Wait(400),
            Eval(|_| {
                let win = dialog();
                let path = std::path::PathBuf::from(std::env::var_os("RIGG_SKJERMBILDER").unwrap()).join("oppgaver-avslutt-bekreftelse.png");
                super::shot(win.upcast_ref(), &path);
                click_dialog("Avbryt");
                assert!(std::path::Path::new(&format!("/proc/{}", own_pid())).exists(), "Avbryt skal ikke sende signal");
            }),
            Wait(400),
            Eval(|a| button(a, "task-stop").emit_clicked()),
            Wait(300),
            Eval(|_| click_dialog("Avslutt oppgave")),
            Wait(2800),
            Eval(|a| {
                let pid = own_pid();
                let gone = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                    Ok(s) => s.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')),
                    Err(_) => true,
                };
                assert!(gone, "SIGTERM til runnerens barn skal være mottatt");
                search(a, "");
            }),
            Wait(600),
            Shot("oppgaver-etter-avslutning"),
        ]);
    }
    steps.extend([Go("oversikt"), Wait(6000), Go("oppgaver"), Wait(4500), Eval(assert_layout), Shot("oppgaver-etter-sideskifte")]);
    steps
}
