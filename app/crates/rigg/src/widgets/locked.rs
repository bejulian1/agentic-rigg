//! Låst-tilstanden (produktspesifikasjonen 6.2): banneret «Endringer er
//! låst» (Lys), popoveren «Hva skal til?» med trinnene fra
//! `capabilities.write`, og porten som avgjør om en skriveoperasjon i det
//! hele tatt sendes.
//!
//! Her ligger også det Kontroller, Viftekurve og Lys lagrer i appen
//! (`profiler.json`: profilene, utkastet, lysvalget og 3D-visningen), og
//! to små byggeklosser de tre sidene deler: [`Columns`] (hovedflate og
//! sidekolonne, under hverandre i smale vinduer) og [`ConfirmBar`]
//! (Behold eller Rull tilbake, bare mot den simulerte backenden).
//!
//! Runde 6C: [`LockStrip`] (én rolig linje øverst på Kontroller og
//! Viftekurve, i stedet for banneret), [`edit_draft`] og angre/gjør om for
//! utkastet ([`UndoStack`], Ctrl+Z og Ctrl+Shift+Z).
//!
//! **Eies av 6C (Styring).** Låsemerket i toppfeltet settes med
//! `Topbar::set_locked(side, true)`.

use crate::state::App;
use crate::widgets::{Icon, button, hbox, label, set_text, vbox};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;
use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use rigg_core::txn::{self, Gate};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// ================================================================ porten

/// Kan vinduet skrive nå? Bare `dev` mot den simulerte backenden.
pub fn gate(app: &App) -> Gate {
    txn::write_gate(app.data.borrow().hello.as_ref(), app.profile.is_dev())
}

/// Sender en skriveoperasjon **bare** når porten er åpen. Ellers får `cb`
/// `write_locked` med en gang, og ingenting går til tjenesten. Tjenesten
/// svarer uansett `write_locked` med det ekte kortet (to lås, ikke én).
pub fn send_write(app: &Rc<App>, op: &str, args: Value, cb: impl FnOnce(OpResult) + 'static) {
    if !rigg_core::proto::WRITE_OPS.contains(&op) || gate(app) != Gate::Sim {
        cb(Err(ProtoError::new(ErrorCode::WriteLocked)));
        return;
    }
    app.client.request(op, args, cb);
}

/// Merket «Låst» i toppfeltet for en side (skallet finnes kanskje ikke ennå
/// når siden bygges første gang).
pub fn mark_topbar(app: &Rc<App>, page: &'static str) {
    let a = app.clone();
    glib::idle_add_local_once(move || {
        if let Some(s) = a.shell.get() {
            s.topbar.set_locked(page, true);
        }
    });
}

/// Merket «Låst» i toppfeltet bare når siden faktisk er låst: mot den
/// simulerte backenden er Bruk på, og da ville merket si det motsatte.
pub fn sync_topbar(app: &Rc<App>, page: &'static str) {
    if let Some(s) = app.shell.get() {
        s.topbar.set_locked(page, gate(app) != Gate::Sim);
    }
}

// ================================================================ root-hjelperen (7A)

/// Tilstanden til root-hjelperen fra tjenesten (`capabilities.write.helper` i
/// `hello`, oppdatert med `get_helper`). `Null` uten tjeneste.
pub fn helper(app: &App) -> Value {
    app.data.borrow().hello.as_ref().map(|h| h["capabilities"]["write"]["helper"].clone()).unwrap_or(Value::Null)
}

/// «off», «on», «error» eller «outdated» (og «off» uten svar).
pub fn helper_state(app: &App) -> String {
    helper(app)["state"].as_str().unwrap_or("off").to_string()
}

/// De godkjente trinnene som navn («Effekt», «Vifter» …).
pub fn helper_steps(app: &App) -> Vec<&'static str> {
    let h = helper(app);
    h["steps"].as_array().map(|l| l.iter().filter_map(|s| s.as_str().and_then(rigg_core::hjelper::Step::parse)).map(|s| s.name()).collect()).unwrap_or_default()
}

/// Ny tilstand fra `get_helper`, lagt inn i `hello` så alle ser den.
pub fn set_helper(app: &App, v: Value) {
    if let Some(h) = app.data.borrow_mut().hello.as_mut() {
        h["capabilities"]["write"]["helper"] = v;
    }
}

/// Spør tjenesten på nytt (etter bryteren) og sier fra til alle.
pub fn refresh_helper(app: &Rc<App>, then: impl FnOnce() + 'static) {
    let a = app.clone();
    app.client.request("get_helper", json!({"refresh": true}), move |r| {
        if let Ok(v) = r {
            set_helper(&a, v);
        }
        a.on_service.emit(&());
        then();
    });
}

/// Hjelperen kjører, men denne versjonen skriver ikke til det ekte kortet.
pub fn real_card_locked(app: &App) -> bool {
    let h = helper(app);
    helper_state(app) == "on" && h["backend"] != "sim" && !h["realWrites"].as_bool().unwrap_or(false)
}

/// Trinnene kan ikke godkjennes i denne versjonen: det ekte kortet er
/// sperret (1.2.0). Bare det simulerte kortet i en utviklerforekomst kan
/// prøve dem. Med hjelperen av vet vinduet ikke mer enn det.
pub fn steps_later(app: &App) -> bool {
    real_card_locked(app) || (helper_state(app) != "on" && gate(app) != Gate::Sim)
}

/// Hvor et trinn står, med samme ord i «Hva skal til?» og i Innstillinger ›
/// Styring: «Godkjent», «Ikke godkjent», «Kommer senere» eller «Ukjent».
pub fn step_status(app: &App, step: rigg_core::hjelper::Step) -> (&'static str, bool) {
    let on = helper(app)["steps"].as_array().is_some_and(|l| l.iter().any(|s| s == step.id()));
    if matches!(helper_state(app).as_str(), "error" | "outdated") {
        // Hjelperen svarer ikke: hva som er godkjent, er ukjent.
        ("Ukjent", false)
    } else if on {
        ("Godkjent", true)
    } else if step == rigg_core::hjelper::Step::Lys || steps_later(app) {
        ("Kommer senere", false)
    } else {
        ("Ikke godkjent", false)
    }
}

/// Merket ved Styring i sidefeltet: tekst, ikon og verktøytips.
pub fn badge(app: &App) -> (String, &'static str, String) {
    let steps = helper_steps(app);
    let list = if steps.is_empty() { "ingen trinn".to_string() } else { steps.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>().join(", ") };
    match helper_state(app).as_str() {
        "on" if real_card_locked(app) => (
            "Endring på · låst".into(),
            "shield",
            "Hjelperen for endring kjører, men denne versjonen endrer ikke det ekte kortet ennå. Alt står som før. Se Innstillinger › Skjermkortstyring.".into(),
        ),
        "on" => (
            format!("Endring på · {list}"),
            "shield",
            format!("Hjelperen for endring kjører. Godkjent: {list}. Hvert trinn godkjennes for seg i Innstillinger › Skjermkortstyring, og hver endring må beholdes innen fristen."),
        ),
        "error" => ("Endring: feil".into(), "triangle-alert", "Hjelperen for endring svarer ikke, så alt er låst. Se Innstillinger › Skjermkortstyring.".into()),
        "outdated" => ("Endring: slå på igjen".into(), "circle-alert", "Hjelperen er fra en annen versjon av appen. Slå «Tillat endring av skjermkortet» av og på igjen i Innstillinger › Skjermkortstyring.".into()),
        _ => ("Bare visning".into(), "lock", HELPER_OFF_TIP.into()),
    }
}

/// Verktøytipset når endring ikke er slått på.
pub const HELPER_OFF_TIP: &str = "Agentic Rigg leser bare. Endringer på skjermkortet er låst til du slår dem på selv i Innstillinger › Skjermkortstyring, ett trinn om gangen.";

/// Teksten i låst-linja når hjelperen er aktivert (ellers sidens egen tekst).
pub fn helper_line(app: &App) -> Option<String> {
    let h = helper(app);
    let steps = helper_steps(app);
    match helper_state(app).as_str() {
        "on" if h["backend"] != "sim" && !h["realWrites"].as_bool().unwrap_or(false) => {
            Some("Endring er slått på, men denne versjonen endrer ikke det ekte kortet ennå. Det du endrer her, blir et utkast som lagres i appen.".into())
        }
        "on" if steps.is_empty() => Some("Endring er slått på, men ingen trinn er godkjent ennå. Godkjenn et trinn i Innstillinger › Skjermkortstyring.".into()),
        "on" => None,
        "error" => Some("Hjelperen for endring svarer ikke, så alt er låst. Se Innstillinger › Skjermkortstyring.".into()),
        "outdated" => Some("Hjelperen er fra en annen versjon, så alt er låst. Slå bryteren i Innstillinger › Skjermkortstyring av og på igjen.".into()),
        _ => None,
    }
}

// ================================================================ profiler.json

/// En lagret profil (de fem plassene, produktspesifikasjonen 3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub values: txn::Values,
    pub saved_at: i64,
}

impl Profile {
    fn to_json(&self) -> Value {
        json!({"name": self.name, "values": serde_json::to_value(&self.values).unwrap_or(Value::Null), "savedAt": self.saved_at})
    }
    fn from_json(v: &Value) -> Option<Profile> {
        let name: String = v["name"].as_str()?.chars().take(32).collect();
        let values = serde_json::from_value(v["values"].clone()).ok()?;
        Some(Profile { name, values, saved_at: v["savedAt"].as_i64().unwrap_or(0) })
    }
}

pub const SLOTS: usize = 5;

/// Innholdet i `profiler.json`. Eies av vinduet; bare disse tre sidene skriver.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stored {
    pub profiles: Vec<Option<Profile>>,
    /// Utkastet på Kontroller og Viftekurve (`None`: lesevisning).
    pub utkast: Option<txn::Values>,
    /// Lysvalget («Lagre i appen»).
    pub rgb: Value,
    /// 3D-visningen på Lys: vinkel, zoom, lagtrinn, skill lagene, 2D/3D, sone.
    pub lys: Value,
}

impl Stored {
    fn to_json(&self) -> Value {
        json!({
            "version": 1,
            "profiles": self.profiles.iter().map(|p| p.as_ref().map_or(Value::Null, Profile::to_json)).collect::<Vec<_>>(),
            "utkast": self.utkast.as_ref().map_or(Value::Null, |u| serde_json::to_value(u).unwrap_or(Value::Null)),
            "rgb": self.rgb,
            "lys": self.lys,
        })
    }
    /// Ett dårlig felt nullstiller ikke resten.
    fn from_json(v: &Value) -> Stored {
        let mut s = Stored {
            profiles: v["profiles"].as_array().map(|a| a.iter().map(Profile::from_json).collect()).unwrap_or_default(),
            utkast: serde_json::from_value(v["utkast"].clone()).ok(),
            rgb: if v["rgb"].is_object() { v["rgb"].clone() } else { Value::Null },
            lys: if v["lys"].is_object() { v["lys"].clone() } else { Value::Null },
        };
        s.profiles.resize(SLOTS, None);
        s.profiles.truncate(SLOTS);
        s
    }
}

thread_local! {
    static UNDO: RefCell<UndoStack> = RefCell::new(UndoStack::default());
    static UNDOING: Cell<bool> = const { Cell::new(false) };
    static STORE: RefCell<Option<Stored>> = const { RefCell::new(None) };
    static LISTENERS: crate::widgets::Listeners<dyn Fn(&str)> = const { RefCell::new(Vec::new()) };
    static SAVE: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

fn store_path(app: &App) -> std::path::PathBuf {
    rigg_core::paths::config_dir(app.profile).join("profiler.json")
}

fn load(app: &App) -> Stored {
    let v: Value = std::fs::read_to_string(store_path(app)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    Stored::from_json(&v)
}

/// Leser det som er lagret (fra disk første gang).
pub fn stored(app: &App) -> Stored {
    STORE.with(|st| st.borrow_mut().get_or_insert_with(|| load(app)).clone())
}

/// Endrer det som er lagret, sier fra til sidene (`what`: «utkast»,
/// «profiler», «rgb», «lys») og lagrer 500 ms etter siste endring.
pub fn update(app: &Rc<App>, what: &str, f: impl FnOnce(&mut Stored)) {
    let (changed, draft_before) = STORE.with(|st| {
        let mut b = st.borrow_mut();
        let s = b.get_or_insert_with(|| load(app));
        let before = s.clone();
        f(s);
        (*s != before, (s.utkast != before.utkast).then_some(before.utkast))
    });
    if !changed {
        return;
    }
    // Ctrl+Z: hver endring i utkastet huskes (ikke når den kommer fra angre).
    if let Some(prev) = draft_before
        && !UNDOING.with(Cell::get)
    {
        UNDO.with(|u| u.borrow_mut().record(&prev, rigg_core::now_ms()));
    }
    let ls = LISTENERS.with(|l| l.borrow().clone());
    for l in ls {
        l(what);
    }
    SAVE.with(|t| {
        if let Some(id) = t.borrow_mut().take() {
            id.remove();
        }
    });
    let a = app.clone();
    let id = glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        SAVE.with(|t| t.borrow_mut().take());
        save_now(&a);
    });
    SAVE.with(|t| *t.borrow_mut() = Some(id));
}

pub fn save_now(app: &App) {
    let Some(s) = STORE.with(|st| st.borrow().clone()) else { return };
    let text = serde_json::to_string_pretty(&s.to_json()).unwrap_or_default();
    if let Err(e) = rigg_core::write_atomic(&store_path(app), text.as_bytes()) {
        eprintln!("Kunne ikke lagre profiler.json: {e}");
    }
}

/// Endrer utkastet. Uten utkast begynner det fra `base` (det kortet har),
/// og blir ikke laget når endringen ikke gjør det forskjellig fra kortet.
pub fn edit_draft(app: &Rc<App>, base: txn::Values, f: impl FnOnce(&mut txn::Values)) {
    update(app, "utkast", |s| {
        let mut d = s.utkast.clone().unwrap_or_else(|| base.clone());
        f(&mut d);
        if s.utkast.is_none() && d == base {
            return;
        }
        s.utkast = Some(d);
    });
}

// ================================================================ angre (Ctrl+Z)

/// Historikken for utkastet på Kontroller og Viftekurve: Ctrl+Z angrer,
/// Ctrl+Shift+Z gjør om. Endringer tettere enn [`UndoStack::MERGE_MS`] (en
/// glider eller et punkt som dras) blir ett steg.
#[derive(Debug, Default)]
pub struct UndoStack {
    past: Vec<Option<txn::Values>>,
    future: Vec<Option<txn::Values>>,
    last: i64,
}

impl UndoStack {
    pub const MERGE_MS: i64 = 700;
    pub const MAX: usize = 100;

    /// `before`: utkastet før endringen som nettopp ble gjort.
    pub fn record(&mut self, before: &Option<txn::Values>, now: i64) {
        if self.past.is_empty() || now - self.last > Self::MERGE_MS {
            self.past.push(before.clone());
            if self.past.len() > Self::MAX {
                self.past.remove(0);
            }
        }
        self.last = now;
        self.future.clear();
    }

    /// Utkastet før siste steg, eller `None` når det ikke er mer å angre.
    pub fn undo(&mut self, cur: &Option<txn::Values>) -> Option<Option<txn::Values>> {
        let p = self.past.pop()?;
        self.future.push(cur.clone());
        self.last = 0;
        Some(p)
    }

    pub fn redo(&mut self, cur: &Option<txn::Values>) -> Option<Option<txn::Values>> {
        let n = self.future.pop()?;
        self.past.push(cur.clone());
        self.last = 0;
        Some(n)
    }
}

/// Ctrl+Z (`redo` = Ctrl+Shift+Z). `false` når det ikke var noe å angre.
pub fn undo_draft(app: &Rc<App>, redo: bool) -> bool {
    let cur = stored(app).utkast;
    let next = UNDO.with(|u| if redo { u.borrow_mut().redo(&cur) } else { u.borrow_mut().undo(&cur) });
    let Some(next) = next else { return false };
    UNDOING.with(|u| u.set(true));
    update(app, "utkast", |s| s.utkast = next);
    UNDOING.with(|u| u.set(false));
    true
}

// ================================================================ låst-linja

/// Linja øverst på Klokker og effekt, Vifter og RGB-lys (runde 6C, RGB-lys
/// fra 6D): én rolig linje som sier om siden er låst, et utkast, simulert
/// eller uten kort, og hvorfor. Erstatter det store banneret fra 1.0.0.
pub struct LockStrip {
    pub root: gtk::Box,
    icon: Icon,
    title: gtk::Label,
    text: gtk::Label,
    what: gtk::Button,
    base: String,
}

impl LockStrip {
    /// `base`: teksten i lesevisningen (hva siden viser).
    pub fn new(app: &Rc<App>, base: &str) -> Rc<LockStrip> {
        let root = hbox(8, &["lock-strip"]);
        root.set_accessible_role(gtk::AccessibleRole::Note);
        let icon = Icon::new("lock", 14);
        icon.set_valign(gtk::Align::Center);
        icon.add_css_class("ls-icon");
        let title = label("Låst", &["ls-title"]);
        title.set_valign(gtk::Align::Center);
        let text = label(base, &["ls-text"]);
        text.set_wrap(true);
        text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let what = button(None, Some("Hva skal til?"), &["sm", "flat", "ls-what"]);
        what.set_valign(gtk::Align::Center);
        crate::widgets::tip(&what, "Trinnene som må godkjennes før appen kan endre kortet", None);
        let a = app.clone();
        what.connect_clicked(move |b| what_it_takes(&a, b));
        root.append(&icon);
        root.append(&title);
        root.append(&text);
        root.append(&what);
        Rc::new(LockStrip { root, icon, title, text, what, base: base.to_string() })
    }

    /// `lost`: kortet svarer ikke. `drafting`: det finnes et utkast.
    pub fn sync(&self, app: &App, lost: bool, drafting: bool) {
        let sim = gate(app) == Gate::Sim;
        let (icon, title, text, kind) = if lost {
            ("unplug", "Kortet svarer ikke", "Verdiene vises igjen når kortet er tilbake. Utkastet beholdes.".to_string(), "lost")
        } else if sim {
            ("circle-dot", "Simulert kort", "Bruk sender utkastet til den simulerte backenden, med Behold eller Rull tilbake. Det ekte kortet røres ikke.".to_string(), "sim")
        } else if drafting {
            let why = if helper_state(app) == "on" { "Denne versjonen endrer ikke det ekte kortet ennå" } else { "Endring er ikke slått på" };
            ("pencil", "Utkast · låst", format!("Utkastet lagres i appen og sendes ikke til kortet. {why}, så Bruk er låst."), "draft")
        } else {
            ("lock", "Låst", helper_line(app).unwrap_or_else(|| self.base.clone()), "")
        };
        if self.icon.icon_name() != icon {
            self.icon.set_icon(icon);
        }
        set_text(&self.title, title);
        set_text(&self.text, &text);
        self.what.set_visible(!sim);
        for k in ["lost", "sim", "draft"] {
            crate::widgets::class(&self.root, k, k == kind);
        }
    }
}

/// Lytter på endringer i det lagrede (`what` som i [`update`]).
pub fn on_store(f: impl Fn(&str) + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Rc::new(f)));
}

/// «Hva skal til?»: bryteren og de fire trinnene, med de samme navnene og
/// ordene som i Innstillinger › Skjermkortstyring.
pub fn what_it_takes(app: &Rc<App>, anchor: &gtk::Button) {
    let bx = vbox(0, &["steps-pop"]);
    bx.append(&label("Hva skal til?", &["pop-title"]));
    let lead = label("Først slår du på endring i Innstillinger › Skjermkortstyring. Så godkjennes ett trinn om gangen, i denne rekkefølgen:", &["pop-sub"]);
    lead.set_wrap(true);
    lead.set_max_width_chars(46);
    lead.set_xalign(0.0);
    bx.append(&lead);
    let grid = gtk::Grid::new();
    grid.add_css_class("steps");
    grid.set_column_spacing(12);
    let state = match helper_state(app).as_str() {
        "on" => "På",
        "error" => "Feil",
        "outdated" => "Må slås på igjen",
        _ => "Av",
    };
    let mut rows: Vec<(String, String, String)> = vec![("".into(), "Endring av skjermkortet".into(), state.into())];
    for (i, step) in rigg_core::hjelper::STEPS.iter().copied().enumerate() {
        rows.push((format!("{}.", i + 1), step.name().into(), step_status(app, step).0.into()));
    }
    for (i, (n, name, status)) in rows.iter().enumerate() {
        let n = label(n, &["muted", "num"]);
        n.set_xalign(1.0);
        let nl = label(name, &[]);
        nl.set_xalign(0.0);
        let st = label(status, &["muted", "sm"]);
        st.set_xalign(0.0);
        grid.attach(&n, 0, i as i32, 1, 1);
        grid.attach(&nl, 1, i as i32, 1, 1);
        grid.attach(&st, 2, i as i32, 1, 1);
    }
    bx.append(&grid);
    let foot_text = if steps_later(app) {
        "I denne versjonen er hjelperen bare prøvd mot et simulert kort, så det ekte kortet kan ikke endres ennå. Hver endring skal senere bekreftes med Behold, ellers rulles den tilbake."
    } else {
        "Hvert trinn godkjennes med passordet ditt, og hver endring må bekreftes med Behold innen fristen, ellers rulles den tilbake."
    };
    let foot = label(foot_text, &["pop-foot"]);
    foot.set_wrap(true);
    foot.set_max_width_chars(46);
    foot.set_xalign(0.0);
    bx.append(&foot);
    let open = button(Some("settings"), Some("Åpne Innstillinger › Skjermkortstyring"), &["sm", "flat", "pop-open"]);
    open.set_halign(gtk::Align::Start);
    bx.append(&open);
    let p = crate::widgets::popover::popover(anchor, &bx);
    p.add_css_class("pop-wide");
    {
        let (a, p) = (app.clone(), p.clone());
        open.connect_clicked(move |_| {
            p.popdown();
            a.open_settings("styring");
        });
    }
    p.popup();
}

// ================================================================ spaltene

mod cols_imp {
    use super::*;

    /// Lytter for smal/bred: (smal, under hverandre).
    pub type NarrowFn = Box<dyn Fn(bool, bool)>;

    pub struct Columns {
        pub side_w: Cell<i32>,
        pub bp: Cell<i32>,
        pub gap: Cell<i32>,
        pub narrow: Cell<Option<(bool, bool)>>,
        pub row_bp: Cell<i32>,
        pub fill: Cell<bool>,
        /// Side ved side: hver spalte får sin naturlige høyde (ikke den høyeste).
        pub natural: Cell<bool>,
        pub on_narrow: RefCell<Vec<NarrowFn>>,
    }

    impl Default for Columns {
        fn default() -> Self {
            Columns { side_w: Cell::new(320), bp: Cell::new(980), gap: Cell::new(8), narrow: Cell::new(None), row_bp: Cell::new(640), fill: Cell::new(false), natural: Cell::new(false), on_narrow: RefCell::new(Vec::new()) }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Columns {
        const NAME: &'static str = "RiggColumns";
        type Type = super::Columns;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("columns");
        }
    }

    impl ObjectImpl for Columns {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl Columns {
        fn kids(&self) -> (Option<gtk::Widget>, Option<gtk::Widget>) {
            let a = self.obj().first_child();
            let b = a.as_ref().and_then(|a| a.next_sibling());
            (a, b)
        }
        fn side_width(&self, width: i32) -> i32 {
            self.side_w.get().min(width / 2)
        }
    }

    impl WidgetImpl for Columns {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, o: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let (Some(main), Some(side)) = self.kids() else { return (0, 0, -1, -1) };
            let gap = self.gap.get();
            match o {
                gtk::Orientation::Horizontal => {
                    let (m0, m1, _, _) = main.measure(o, -1);
                    let (s0, s1, _, _) = side.measure(o, -1);
                    (m0.max(s0), m1 + gap + s1.max(self.side_w.get()), -1, -1)
                }
                _ => {
                    let w = if for_size < 0 { self.bp.get() } else { for_size };
                    if w >= self.bp.get() {
                        let sw = self.side_width(w);
                        let mw = (w - sw - gap).max(0);
                        let (m0, m1, _, _) = main.measure(o, mw);
                        let (s0, s1, _, _) = side.measure(o, sw);
                        (m0.max(s0), m1.max(s1), -1, -1)
                    } else {
                        let (m0, m1, _, _) = main.measure(o, w);
                        let (s0, s1, _, _) = side.measure(o, w);
                        (m0 + gap + s0, m1 + gap + s1, -1, -1)
                    }
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let (Some(main), Some(side)) = self.kids() else { return };
            let gap = self.gap.get();
            let narrow = width < self.bp.get();
            let main_w = if narrow { width } else { width - self.side_width(width) - gap };
            let state = (narrow, main_w < self.row_bp.get());
            if self.narrow.replace(Some(state)) != Some(state) {
                let obj = self.obj().clone();
                glib::idle_add_local_once(move || {
                    let (n, r) = obj.imp().narrow.get().unwrap_or((false, false));
                    for f in obj.imp().on_narrow.borrow().iter() {
                        f(n, r);
                    }
                });
            }
            if narrow {
                let (_, mh, _, _) = main.measure(gtk::Orientation::Vertical, width);
                let (_, sh, _, _) = side.measure(gtk::Orientation::Vertical, width);
                // In smale vinduer: hovedflaten får det den trenger, eller mer når det er plass.
                let mh = if self.fill.get() { mh.max(height - sh - gap) } else { mh };
                main.size_allocate(&gtk::Allocation::new(0, 0, width, mh), -1);
                side.size_allocate(&gtk::Allocation::new(0, mh + gap, width, sh.max(height - mh - gap)), -1);
            } else {
                let sw = self.side_width(width);
                let mw = width - sw - gap;
                let (mh, sh) = if self.natural.get() {
                    (main.measure(gtk::Orientation::Vertical, mw).1.min(height), side.measure(gtk::Orientation::Vertical, sw).1.min(height))
                } else {
                    (height, height)
                };
                main.size_allocate(&gtk::Allocation::new(0, 0, mw, mh), -1);
                side.size_allocate(&gtk::Allocation::new(mw + gap, 0, sw, sh), -1);
            }
        }
    }
}

glib::wrapper! {
    /// Hovedflate og sidekolonne side ved side, eller under hverandre når
    /// vinduet er smalere enn bruddpunktet.
    pub struct Columns(ObjectSubclass<cols_imp::Columns>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Columns {
    pub fn new(main: &impl IsA<gtk::Widget>, side: &impl IsA<gtk::Widget>, side_w: i32, breakpoint: i32) -> Columns {
        let c: Columns = glib::Object::new();
        c.imp().side_w.set(side_w);
        c.imp().bp.set(breakpoint);
        main.set_parent(&c);
        side.set_parent(&c);
        c
    }

    /// I smale vinduer fyller hovedflaten høyden som er igjen (Lys, Viftekurve).
    pub fn set_fill(&self, on: bool) {
        self.imp().fill.set(on);
    }

    /// Side ved side får hver spalte sin naturlige høyde (Viftekurve: kurven
    /// strekkes ikke til høyden på sidekolonnen).
    pub fn set_natural(&self, on: bool) {
        self.imp().natural.set(on);
    }

    /// `f(spaltene under hverandre, hovedflaten smalere enn radbruddpunktet)`.
    pub fn connect_narrow(&self, f: impl Fn(bool, bool) + 'static) {
        self.imp().on_narrow.borrow_mut().push(Box::new(f));
    }

    /// Under denne bredden på hovedflaten legger radene seg i to linjer.
    pub fn set_row_breakpoint(&self, px: i32) {
        self.imp().row_bp.set(px);
    }

    pub fn is_narrow(&self) -> bool {
        self.imp().narrow.get().is_some_and(|n| n.0)
    }
}

// ================================================================ Behold eller rull tilbake

/// Linja «Behold endringene?» etter Bruk (bare mot den simulerte backenden).
/// Spør tjenesten om tilstanden hvert sekund mens en endring venter.
pub struct ConfirmBar {
    pub root: gtk::Revealer,
    text: gtk::Label,
    keep: gtk::Button,
    txn: RefCell<Option<(String, i64)>>,
    timer: RefCell<Option<glib::SourceId>>,
    pub on_done: crate::widgets::Slot<dyn Fn(&str)>,
}

impl ConfirmBar {
    pub fn new(app: &Rc<App>) -> Rc<ConfirmBar> {
        let root = gtk::Revealer::new();
        root.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        root.set_transition_duration(crate::motion::transition_ms(app, 3));
        let bx = hbox(10, &["confirm-bar"]);
        bx.append(&Icon::new("timer", 16));
        let text = label("", &[]);
        text.set_hexpand(true);
        text.set_wrap(true);
        bx.append(&text);
        let revert = button(None, Some("Rull tilbake"), &["sm"]);
        let keep = button(None, Some("Behold"), &["sm", "suggested"]);
        bx.append(&revert);
        bx.append(&keep);
        root.set_child(Some(&bx));
        // Skjult helt (ingen avstand i siden) til en endring venter.
        root.set_visible(false);
        let cb = Rc::new(ConfirmBar { root, text, keep: keep.clone(), txn: RefCell::new(None), timer: RefCell::new(None), on_done: RefCell::new(None) });
        let (c, a) = (cb.clone(), app.clone());
        keep.connect_clicked(move |_| c.answer(&a, "confirm"));
        let (c, a) = (cb.clone(), app.clone());
        revert.connect_clicked(move |_| c.answer(&a, "revert"));
        // Tjenesten rullet tilbake selv (fristen, frakobling, en annen klient).
        let (w, a) = (Rc::downgrade(&cb), app.clone());
        app.on_txn.connect(move |ev| {
            let Some(c) = w.upgrade() else { return };
            let mine = c.txn.borrow().as_ref().is_some_and(|(id, _)| ev["txn"].as_str() == Some(id.as_str()));
            if mine && ev["status"] == "rolled_back" {
                let why = ev["reason"].as_str().unwrap_or("revert").to_string();
                c.finish(&why);
                if why == "timeout" {
                    a.toast(crate::toast::Toast::new("Endringene er rullet tilbake").body("Fristen gikk ut uten svar."));
                }
            }
        });
        cb
    }

    /// Svaret på `apply`: viser linja når en endring venter.
    pub fn start(self: &Rc<Self>, app: &Rc<App>, res: &Value) {
        let Some(id) = res["txn"].as_str() else { return };
        let deadline = rigg_core::now_ms() + res["pending"]["remainingMs"].as_i64().unwrap_or(15_000);
        *self.txn.borrow_mut() = Some((id.to_string(), deadline));
        self.root.set_visible(true);
        self.root.set_reveal_child(true);
        self.keep.grab_focus();
        self.tick(app);
        if self.timer.borrow().is_none() {
            let (c, a) = (self.clone(), app.clone());
            *self.timer.borrow_mut() = Some(glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                c.tick(&a);
                if c.txn.borrow().is_some() { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
            }));
        }
    }

    fn tick(self: &Rc<Self>, _app: &Rc<App>) {
        let Some((_, deadline)) = self.txn.borrow().clone() else { return };
        let left = ((deadline - rigg_core::now_ms()) as f64 / 1000.0).ceil().max(0.0) as i64;
        set_text(&self.text, &format!("Behold endringene? Uten svar rulles de tilbake om {left}{}s.", rigg_core::fmt::NNBSP));
        self.keep.set_label(&format!("Behold ({left})"));
        if left <= 0 {
            self.finish("timeout");
        }
    }

    fn finish(&self, how: &str) {
        self.txn.borrow_mut().take();
        if let Some(t) = self.timer.borrow_mut().take() {
            t.remove();
        }
        self.root.set_reveal_child(false);
        self.root.set_visible(false);
        if let Some(f) = self.on_done.borrow().as_ref() {
            f(how);
        }
    }

    fn answer(self: &Rc<Self>, app: &Rc<App>, op: &'static str) {
        let Some((id, _)) = self.txn.borrow().clone() else { return };
        let (c, a) = (self.clone(), app.clone());
        send_write(app, op, json!({"txn": id}), move |res| match res {
            Ok(_) => {
                c.finish(op);
                let t = if op == "confirm" { "Endringene er beholdt" } else { "Endringene er rullet tilbake" };
                a.toast(crate::toast::Toast::new(t));
            }
            Err(e) => {
                c.finish("error");
                a.toast(crate::toast::Toast::new("Kunne ikke svare").body(&e.message).error());
            }
        });
    }

    pub fn active(&self) -> bool {
        self.txn.borrow().is_some()
    }
}

/// Tekst for en verdi med enhet og fortegn: «+150 MHz».
pub fn signed(v: f64, unit: &str) -> String {
    rigg_core::fmt::with_unit(rigg_core::fmt::signed(v, 0), unit)
}

/// Klokkeslett i brukerens format: «14:32:10».
pub fn clock(ms: i64, seconds: bool) -> String {
    rigg_core::fmt::time(crate::app::local_time(ms), seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(w: f64) -> Option<txn::Values> {
        Some(txn::Values { power_limit_w: w, ..Default::default() })
    }

    #[test]
    fn angre_slaar_sammen_draing_og_gjor_om() {
        let mut u = UndoStack::default();
        // Første endring: fra ingen utkast.
        u.record(&None, 1_000);
        // En glider som dras: 200 ms mellom hver, blir ett steg.
        u.record(&v(240.0), 1_200);
        u.record(&v(230.0), 1_400);
        // Ny endring etter en pause.
        u.record(&v(220.0), 5_000);
        let cur = v(210.0);
        assert_eq!(u.undo(&cur), Some(v(220.0)));
        assert_eq!(u.undo(&v(220.0)), Some(None));
        assert_eq!(u.undo(&None), None);
        // Gjør om i motsatt rekkefølge.
        assert_eq!(u.redo(&None), Some(v(220.0)));
        assert_eq!(u.redo(&v(220.0)), Some(v(210.0)));
        assert_eq!(u.redo(&v(210.0)), None);
        // En ny endring tømmer «gjør om».
        u.undo(&v(210.0));
        u.record(&v(220.0), 9_000);
        assert_eq!(u.redo(&v(1.0)), None);
    }

    #[test]
    fn angre_husker_hoyst_hundre_steg() {
        let mut u = UndoStack::default();
        for i in 0..150 {
            u.record(&v(i as f64), i * 10_000);
        }
        let mut n = 0;
        let mut cur = v(0.0);
        while let Some(p) = u.undo(&cur) {
            cur = p;
            n += 1;
        }
        assert_eq!(n, UndoStack::MAX);
        assert_eq!(cur, v(50.0));
    }
}
