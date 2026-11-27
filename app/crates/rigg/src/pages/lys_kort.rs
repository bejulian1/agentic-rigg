//! Fysisk kortlys, atskilt fra NVML-styring.
//! Ingen redigering, profilinnlasting eller sideåpning sender en lysendring.

use crate::state::App;
use crate::toast::Toast;
use crate::widgets::{Panel, Seg, button, class, hbox, label, set_text, vbox};
use gtk::glib;
use gtk::prelude::*;
use rigg_core::proto::{ErrorCode, ProtoError};
use rigg_core::{fmt, tr, trf};
use rigg_core::rgb::{Binding, Brightness, Request};
use rigg_core::rgb_auth;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VisualState {
    pub bound: bool,
    pub pending: bool,
    /// Valgt modus i kontrollene: static, off, breathing, rainbow, cycle.
    pub selected_mode: String,
    pub selected_color: Option<[u8; 3]>,
    pub selected_brightness: u8,
    /// Aktiv (beholdt) programvareeffekt fra hjelperen, hvis noen kjører.
    pub active_effect: Option<String>,
    /// Sist leste eller lagrede farge og lysstyrke fra hjelperen.
    pub observed_color: Option<[u8; 3]>,
    pub observed_brightness: Option<u8>,
    pub observation_source: String,
}

pub(super) struct CardLight {
    pub root: gtk::Box,
    pub on_visual: crate::widgets::Slot<dyn Fn(&VisualState)>,
    last_visual: RefCell<Option<VisualState>>,
    app: Rc<App>,
    helper: RefCell<Value>,
    visible: Cell<bool>,
    reading: Cell<bool>,
    busy: Cell<bool>,
    fixture: Cell<bool>,
    poll_at: Cell<Option<Instant>>,
    timer: RefCell<Option<glib::SourceId>>,
    message: RefCell<Option<String>>,
    badge: gtk::Label,
    summary: gtk::Label,
    identity: gtk::Label,
    observation: gtk::Label,
    confirmed: gtk::Label,
    recovery: gtk::Label,
    effect_info: gtk::Label,
    runtime_error: gtk::Label,
    /// Kortlyset på/av, atskilt fra modus og effekt.
    power: gtk::Switch,
    mode: Seg,
    color: gtk::Entry,
    color_pick: gtk::Button,
    swatch: gtk::DrawingArea,
    swatch_color: Rc<Cell<Option<[u8; 3]>>>,
    swatches_box: gtk::Box,
    controls: gtk::Grid,
    brightness_box: gtk::Box,
    details: gtk::Expander,
    /// Neste steg etter et vellykket kall (lagre som varig, start «hold»).
    followups: RefCell<std::collections::VecDeque<Followup>>,
    /// Neste `rgb_apply` er «Av» som skal holdes mens appen er åpen.
    hold_intent: Cell<bool>,
    /// Skrivemåten for «Av» (`rgbAv`), under detaljene.
    av_method: gtk::DropDown,
    /// Brukeren har endret valgene siden siste synkronisering fra kortet.
    edited: Cell<bool>,
    syncing: Cell<bool>,
    last_synced: RefCell<Value>,
    varig: gtk::CheckButton,
    varig_info: gtk::Label,
    pub action_box: gtk::Box,
    action_title: gtk::Label,
    action_text: gtk::Label,
    factory: gtk::Button,
    last_state: RefCell<String>,
    support: gtk::Label,
    readback_note: gtk::Label,
    brightness: gtk::SpinButton,
    apply: gtk::Button,
    apply_hint: gtk::Label,
    settings: gtk::Button,
    keep_observed: gtk::Button,
    refresh: gtk::Button,
    reset: gtk::Button,
    release: gtk::Button,
    ack: gtk::Button,
    pending: gtk::Box,
    deadline: gtk::Label,
    pending_hint: gtk::Label,
    keep: gtk::Button,
    revert: gtk::Button,
}

thread_local! {
    static CURRENT: RefCell<std::rc::Weak<CardLight>> = const { RefCell::new(std::rc::Weak::new()) };
    static FLOW_INITIAL: RefCell<Value> = const { RefCell::new(Value::Null) };
    static FLOW_EFFECT_START: RefCell<Value> = const { RefCell::new(Value::Null) };
}

const FLOW_MARKER: &str = "agentic-rigg-rgb-fake-v1";
type EffectLabel = (&'static str, fn() -> &'static str);

const EFFECTS: [EffectLabel; 3] = [
    ("breathing", || tr!("Pusting")),
    ("rainbow", || tr!("Regnbue")),
    ("cycle", || tr!("Fargesyklus")),
];

fn effect_name(id: &str) -> Option<&'static str> {
    EFFECTS
        .iter()
        .find_map(|(key, name)| (*key == id).then(name))
}

fn supports_effect(status: &Value, effect: &str) -> bool {
    effect_name(effect).is_some()
        && status["softwareEffects"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(effect)))
}

fn operation_error(error: &ProtoError) -> String {
    match error.code {
        ErrorCode::Busy | ErrorCode::NotOwner => tr!("Lysstyringen er opptatt eller tilhører et annet vindu.").into(),
        ErrorCode::NoPending | ErrorCode::TxnMismatch => tr!("Lysprøven finnes ikke lenger eller fristen er ute.").into(),
        ErrorCode::WriteLocked => tr!("Nye lysendringer er sperret. Kontroller status og avklar eventuell gjenoppretting.").into(),
        ErrorCode::PermissionDenied => tr!("RGB-tillatelsen for dette kortet kunne ikke bekreftes.").into(),
        ErrorCode::UnknownGpu => tr!("Det valgte kortet er ikke tilgjengelig.").into(),
        ErrorCode::NotSupported => tr!("RGB-hjelperen støtter ikke dette lysvalget.").into(),
        ErrorCode::InvalidArgument => tr!("RGB-hjelperen avviste forespørselen. Les status før du prøver igjen.").into(),
        // Ukjent årsak: behold tjenestens egen melding synlig, ikke bare i et verktøytips.
        _ => with_diagnostic(
            tr!("RGB-forespørselen mislyktes. Sjekk status og se på kortet før du prøver igjen."),
            Some(&rigg_core::hjelper::display_diagnostic(&error.message)),
        ),
    }
}

/// Oversatt forklaring først, deretter den opprinnelige diagnosen uendret.
/// Skriptene og tjenesten sier hva som må avklares; det skal ikke bare ligge
/// i et verktøytips som krever pekeren.
fn with_diagnostic(text: &str, diagnostic: Option<&str>) -> String {
    match diagnostic.map(str::trim).filter(|detail| !detail.is_empty()) {
        Some(detail) => format!("{text}\n{detail}"),
        None => text.to_string(),
    }
}

/// Teksten som forklarer en gjenoppretting. En lagret feil går foran
/// «samsvarer»: hjelperen avviser brukerbekreftelsen så lenge feilen står,
/// så teksten skal ikke be brukeren bekrefte.
fn recovery_text(state: &Value) -> String {
    if state["readError"].is_string() {
        return tr!("Avlesning av kortlyset mislyktes. Nye lysendringer er sperret.").into();
    }
    if state["recoveryConfirmationSource"] == "userVisual" {
        return tr!("Gjenopprettingen er bekreftet av deg. Effektmodusen er fortsatt ikke maskinelt avlest.").into();
    }
    let failed = state["error"].is_string();
    match state["recovery"].as_str() {
        Some("readable_fields_match") if !failed => tr!("Farge og lysstyrke samsvarer etter tilbakeføring. Se på kortet før du bekrefter gjenopprettingen.").into(),
        Some("mismatch") => tr!("Tilbakeføringen samsvarer ikke med avlesningen. Nye fargeendringer er sperret.").into(),
        Some("unknown") => tr!("Gjenopprettingen er uavklart. Nye fargeendringer er sperret.").into(),
        _ if failed => tr!("Gjenopprettingen er uavklart. Nye fargeendringer er sperret.").into(),
        _ => String::new(),
    }
}

fn is_blocked(state: &Value) -> bool {
    let visually_confirmed = state["recoveryConfirmationSource"] == "userVisual";
    state["error"].is_string()
        || state["readError"].is_string()
        || matches!(state["recovery"].as_str(), Some("unknown" | "mismatch"))
        || (state["recovery"] == "readable_fields_match" && !visually_confirmed)
}

fn can_apply(status: &Value, chosen_effect: Option<&str>) -> bool {
    let state = &status["state"];
    status["review"].is_null() && state["pending"].is_null()
        && !is_blocked(state)
        && chosen_effect.is_none_or(|effect| supports_effect(status, effect))
}

fn flow_requested(app: &App) -> bool {
    app.profile.is_dev() && std::env::var_os("RIGG_RGB_FLOW").is_some()
}

/// Skjermbildeprøven må være isolert både i appen og i svaret fra arbeideren.
/// En privat env-rot alene beviser ikke hvilken socket daemonen brukte.
fn flow_environment(app: &App, helper: &Value) -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if !app.profile.is_dev()
        || std::env::var("RIGG_RGB_FLOW").as_deref() != Ok("sim")
        || !app.data.borrow().backend.starts_with("sim")
        || std::env::var_os("RIGG_SKJERMBILDER").is_none()
    {
        return Err("RGB-funksjonsprøven krever dev, sim og en privat skjermbildeøkt.".into());
    }
    let root = std::env::var_os("RIGG_HJELPER_ROT")
        .map(PathBuf::from)
        .ok_or("RGB-funksjonsprøven mangler privat rot.")?;
    let canonical = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
    let info = std::fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
    // SAFETY: bare prosessens eksisterende identitet, ingen privilegieendring.
    let uid = unsafe { libc::geteuid() };
    if root != canonical
        || !canonical.starts_with("/tmp")
        || canonical == Path::new("/tmp")
        || !info.is_dir()
        || info.uid() != uid
        || info.mode() & 0o7777 != 0o700
    {
        return Err("RGB-funksjonsprøven trenger en egen 0700-mappe under /tmp.".into());
    }
    let marker = root.join("rgb-flow.fake");
    let info = std::fs::symlink_metadata(&marker).map_err(|e| e.to_string())?;
    if !info.is_file()
        || info.uid() != uid
        || info.mode() & 0o7777 != 0o600
        || info.nlink() != 1
        || info.len() > 64
        || std::fs::read_to_string(marker).map_err(|e| e.to_string())? != format!("{FLOW_MARKER}\n")
    {
        return Err("RGB-funksjonsprøven mangler riktig privat markørfil.".into());
    }
    let socket = rgb_auth::Paths::new(&root).socket();
    if std::fs::canonicalize(&socket).map_err(|e| e.to_string())? != socket
        || !std::fs::symlink_metadata(&socket)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_socket()
        || helper["status"]["testMarker"] != FLOW_MARKER
        || !bound_status(helper, selected_identity(app).as_ref())
    {
        return Err(
            "RGB-funksjonsprøven har ikke fått svar fra den merkede fake-arbeideren.".into(),
        );
    }
    Ok(())
}

/// Teksten i en knapp laget med ikon og tekst (`widgets::button`).
fn set_text_button(button: &gtk::Button, text: &str) {
    let mut child = button.child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            set_text(label, text);
            return;
        }
        child = widget.first_child().or_else(|| widget.next_sibling());
    }
}

fn paragraph(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label(text, classes);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l.set_hexpand(true);
    l.set_xalign(0.0);
    l
}

fn flow() -> gtk::FlowBox {
    let row = gtk::FlowBox::new();
    row.set_halign(gtk::Align::Start);
    row.set_selection_mode(gtk::SelectionMode::None);
    row.set_min_children_per_line(1);
    row.set_max_children_per_line(5);
    row.set_column_spacing(6);
    row.set_row_spacing(6);
    row.add_css_class("rgb-card-flow");
    row
}

fn selected_identity(app: &App) -> Option<(String, String)> {
    Some((app.gpu_id()?, app.statics()?.uuid?))
}

fn card_present(app: &App) -> bool {
    app.gpu_info()
        .is_some_and(|gpu| matches!(gpu.state, rigg_core::model::DeviceState::Ok))
}

fn hex_color(text: &str) -> Option<[u8; 3]> {
    let value = text.strip_prefix('#').unwrap_or(text);
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

fn color_text(value: &Value) -> Option<String> {
    let rgb: [u8; 3] = serde_json::from_value(value.clone()).ok()?;
    Some(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
}



/// Steg som følger etter et vellykket «Bruk» eller «Behold».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Followup {
    /// Lagre det beholdte valget som varig utgangspunkt.
    Persist,
    /// Start «hold» for «Av»: hjelperen skriver det kortet skal vise på nytt
    /// hvert 2. sekund, uten en ny prøve (allerede under prøven, så kortet
    /// ikke går tilbake til fabrikklyset mens fristen løper).
    Hold,
}

/// Skrivemåtene for «Av» (`rgbAv`), i rekkefølgen de vises.
fn av_methods() -> [(&'static str, &'static str); 5] {
    [
        ("hold", tr!("Svart med 0 %, holdt mens appen er åpen")),
        ("svart-100", tr!("Svart med 100 %")),
        ("svart-0", tr!("Svart med 0 %")),
        ("farge-0", tr!("Valgt farge med 0 %")),
        ("nesten-svart", tr!("Nesten svart (#010101) med 1 %")),
    ]
}

/// «Av» slik innstillingen `rgbAv` sier: forespørselen, og om den må skrives
/// på nytt mens appen er åpen (keepalive). Ingen av variantene bruker andre
/// registre enn appens faste farge (60, 6C–6F).
fn off_request(method: &str, color: [u8; 3]) -> (Request, bool) {
    let level = |v: u8| Brightness::try_from(v).expect("0–100");
    match method {
        "svart-100" => (Request::Off {}, false),
        "svart-0" => (Request::Static { color: [0, 0, 0], brightness: level(0) }, false),
        "farge-0" => (Request::Static { color, brightness: level(0) }, false),
        "nesten-svart" => (Request::Static { color: [1, 1, 1], brightness: level(1) }, false),
        _ => (Request::Static { color: [0, 0, 0], brightness: level(0) }, true),
    }
}

/// Lyser i praksis ikke: «Av», svart, lysstyrke 0 eller nesten svart med 1 %.
fn looks_off(value: &Value) -> bool {
    match serde_json::from_value::<Request>(value.clone()) {
        Ok(Request::Off {}) => true,
        Ok(Request::Static { color, brightness }) => color == [0, 0, 0] || brightness.get() == 0 || (color == [1, 1, 1] && brightness.get() <= 1),
        Err(_) => false,
    }
}

/// Et «Bruk» fra før aktiveringen kan sendes: innen fristen og uendret.
fn request_text(value: &Value) -> String {
    if looks_off(value) {
        return tr!("Av").into();
    }
    match serde_json::from_value::<Request>(value.clone()) {
        Ok(Request::Off {}) => tr!("Av").into(),
        // Står inne i setninger («… går tilbake til fast farge #3A7BF0 · 80 %»).
        Ok(Request::Static { color, brightness }) => trf!("fast farge {farge} · {styrke}", farge = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]), styrke = fmt::pct(Some(f64::from(brightness.get())), 0)),
        Err(_) => tr!("Ingen kjent innstilling").into(),
    }
}

/// [`request_text`] først på en linje.
fn request_heading(value: &Value) -> String {
    let text = request_text(value);
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Det kortet sist ble lest til, som tekst (også rå lysstyrke over 100 %).
fn observed_text(observed: &Value) -> String {
    match (color_text(&observed["color"]), observed["brightnessRaw"].as_u64()) {
        (Some(_), Some(level)) if level <= 100 => {
            request_text(&json!({"mode": "static", "color": observed["color"], "brightness": level}))
        }
        (Some(color), Some(level)) => trf!("{farge} · rå lysstyrke {styrke}, utenfor støttet område", farge = color, styrke = fmt::num(Some(level as f64), 0)),
        _ => tr!("et lys som ikke kunne leses").into(),
    }
}

fn bound_status(helper: &Value, selected: Option<&(String, String)>) -> bool {
    let Some((pci, uuid)) = selected else {
        return false;
    };
    let Ok(binding) = serde_json::from_value::<Binding>(helper["status"]["identity"].clone())
    else {
        return false;
    };
    helper["state"] == "on"
        && helper["status"]["authorized"] == true
        && binding.validate().is_ok()
        && &binding.pci == pci
        && &binding.uuid == uuid
        && helper["status"]["helper"]["name"] == rgb_auth::HELPER_NAME
        && helper["status"]["helper"]["domain"] == rgb_auth::DOMAIN
        && helper["status"]["helper"]["strictRequests"] == true
        && helper["status"]["helper"]["protocol"] == rgb_auth::PROTOCOL
        && helper["status"]["helper"]["build"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .is_some_and(rgb_auth::compatible_build)
}

/// RGB-arbeideren mangler funksjonene som kreves av kontrollsiden.
fn rgb_stale(_app: &App, helper: &Value) -> bool {
    if FIXTURE_STALE.with(Cell::get) {
        return true;
    }
    lacks_current_features(helper)
}

fn lacks_current_features(helper: &Value) -> bool {
    helper["state"] == "on"
        && helper["status"].is_object()
        && !(helper["status"]["persist"] == true && helper["status"]["hold"] == true)
}

thread_local! {
    /// «Hold» ved oppstart er forsøkt for denne forbindelsen til tjenesten.
    static AUTO_HOLD_QUERY: Cell<bool> = const { Cell::new(false) };
    static AUTO_HOLD_INSTANCE: RefCell<Option<String>> = const { RefCell::new(None) };
    static STOPPED_HOLD: RefCell<std::collections::BTreeSet<(String,String)>> = const { RefCell::new(std::collections::BTreeSet::new()) };
}

/// Etter hver ny forbindelse til tjenesten (appstart, eller tjenesten startet
/// på nytt): er «Av» lagret som kortets utgangspunkt og skrivemåten «hold»,
/// startes «hold» igjen, én gang. Bare når kortet allerede er lest likt
/// utgangspunktet av hjelperen; et avvik vises på siden RGB-lys i stedet.
pub(crate) fn auto_hold(app: &Rc<App>) {
    AUTO_HOLD_INSTANCE.with(|t| t.borrow_mut().take());
    try_auto_hold(app);
    thread_local! { static WIRED: Cell<bool> = const { Cell::new(false) }; }
    if !WIRED.with(|w| w.replace(true)) {
        let weak = Rc::downgrade(app);
        glib::timeout_add_seconds_local(5, move || {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            if crate::widgets::locked::helper_state(&app) == "on" { try_auto_hold(&app); }
            glib::ControlFlow::Continue
        });
        let a = Rc::downgrade(app);
        app.on_devices.connect(move |_| {
            if let Some(app) = a.upgrade() {
                try_auto_hold(&app);
            }
        });
    }
}

fn try_auto_hold(app: &Rc<App>) {
    if app.settings().rgb_av != "hold"
        || !app.client.is_connected()
        || !card_present(app)
        || selected_identity(app).is_none_or(|card| STOPPED_HOLD.with(|s| s.borrow().contains(&card)))
    {
        return;
    }
    if AUTO_HOLD_QUERY.with(|v| v.replace(true)) { return; }
    let a = app.clone();
    app.client.request("get_rgb_helper", json!({"refresh": true}), move |result| {
        let eligible = result.ok().filter(|helper| auto_hold_wanted(&a, helper));
        let Some((pci, uuid)) = selected_identity(&a).filter(|card| !STOPPED_HOLD.with(|s| s.borrow().contains(card))) else {
            AUTO_HOLD_QUERY.with(|v| v.set(false));
            return;
        };
        let Some(instance) = eligible.and_then(|h| h["status"]["instanceId"].as_str().map(str::to_owned)) else {
            AUTO_HOLD_QUERY.with(|v| v.set(false));
            return;
        };
        if AUTO_HOLD_INSTANCE.with(|v| v.borrow().as_ref() == Some(&instance)) {
            AUTO_HOLD_QUERY.with(|v| v.set(false));
            return;
        }
        let args = json!({"gpu": pci, "uuid": uuid, "kilde": "hold-oppstart"});
        a.client.request("rgb_hold", args, move |result| {
            AUTO_HOLD_QUERY.with(|v| v.set(false));
            if result.is_ok() { AUTO_HOLD_INSTANCE.with(|v| v.replace(Some(instance))); }
            if let Err(error) = &result {
                eprintln!("RGB: «hold» ved oppstart ble ikke startet: {}", error.message);
            }
            if let Some(p) = CURRENT.with(|c| c.borrow().upgrade()) {
                p.refresh();
            }
        });
    });
}

/// «Av» er lagret som utgangspunkt, økten er ren og ingen holder lyset.
fn auto_hold_wanted(app: &App, helper: &Value) -> bool {
    bound_status(helper, selected_identity(app).as_ref())
        && !rgb_stale(app, helper)
        && helper["busy"] != true
        && hold_status_ready(&helper["status"])
}

/// Hjelperens del av [`auto_hold_wanted`].
fn hold_status_ready(status: &Value) -> bool {
    let state = &status["state"];
    status["review"].is_null() && status["hold"] == true
        && status["keepalive"] != true
        && status["holding"].is_null()
        && status["effect"].is_null()
        && status["drift"] != true
        && state["dirty"] == false
        && state["pending"].is_null()
        && !is_blocked(state)
        && looks_off(&status["baselineRequest"])
        && state["confirmed"] == status["baselineRequest"]
}

thread_local! {
    /// Bare skjermbildefixturen «stale_unsettled»: RGB-delen må oppdateres.
    static FIXTURE_STALE: Cell<bool> = const { Cell::new(false) };
}

/// Bundet og klar til bruk: som [`bound_status`], men ikke når RGB-delen må oppdateres.
fn usable(app: &App, helper: &Value, selected: Option<&(String, String)>) -> bool {
    bound_status(helper, selected) && !rgb_stale(app, helper)
}

fn should_read_clean(helper: &Value, selected: Option<&(String, String)>) -> bool {
    bound_status(helper, selected)
        && helper["busy"] != true
        && helper["status"]["state"]["dirty"] == false
}

fn boottime_ms() -> Option<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: rent klokkeoppslag. Samme monotone klokke som RGB-hjelperen,
    // inkludert suspend; dette brukes bare til tekst og knappetilstand.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 {
        return None;
    }
    let sec = u64::try_from(time.tv_sec).ok()?;
    let ns = u64::try_from(time.tv_nsec)
        .ok()
        .filter(|n| *n < 1_000_000_000)?;
    sec.checked_mul(1000)?.checked_add(ns / 1_000_000)
}

impl CardLight {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let panel = Panel::new(tr!("Lys på kortet"));
        panel.root.add_css_class("rgb-card-control");
        // Én linje: et merke som kan brytes, ble brutt i to («Ikke / aktivert»)
        // så snart panelhodet var høyt nok, fordi GTK da spør om bredden for
        // den høyden. Er det for trangt, kuttes teksten med «…».
        let badge = label(tr!("Ikke aktivert"), &["tag", "quiet"]);
        badge.set_ellipsize(gtk::pango::EllipsizeMode::End);
        badge.set_max_width_chars(48);
        // Merket følger teksthøyden, ikke panelhodets høyde (ble en firkantet boks).
        badge.set_valign(gtk::Align::Center);
        badge.set_has_tooltip(true);
        badge.connect_query_tooltip(|l, _, _, _, tip| {
            let cut = l.layout().is_ellipsized();
            if cut {
                tip.set_text(Some(&l.text()));
            }
            cut
        });
        panel.aside.append(&badge);
        let summary = paragraph("", &["muted"]);
        panel.body.append(&summary);

        // Valgene står alltid fremme. Første «Bruk» aktiverer (eller
        // oppdaterer) kortlyset med ett passord og bruker deretter valget.
        let controls = gtk::Grid::new();
        controls.add_css_class("rgb-card-grid");
        controls.set_row_spacing(10);
        controls.set_column_spacing(16);
        let heading = |text: &str| {
            let l = label(text, &["rgb-card-heading"]);
            l.set_valign(gtk::Align::Center);
            l
        };
        // Kortlyset på/av står for seg: «Av» er ikke en modus ved siden av
        // effektene, og forrige valg kommer tilbake når lyset slås på.
        let power = gtk::Switch::new();
        power.set_active(true);
        power.set_valign(gtk::Align::Center);
        power.set_halign(gtk::Align::Start);
        power.add_css_class("rgb-card-power");
        power.update_property(&[gtk::accessible::Property::Label(tr!("Kortlys på eller av"))]);
        controls.attach(&heading(tr!("Kortlys")), 0, 0, 1, 1);
        controls.attach(&power, 1, 0, 1, 1);
        let mode = Seg::new(
            &[
                ("static", tr!("Fast farge")),
                ("breathing", tr!("Pusting")),
                ("rainbow", tr!("Regnbue")),
                ("cycle", tr!("Fargesyklus")),
            ],
            "static",
            tr!("Fysisk lysmodus"),
        )
        .small();
        mode.root.set_halign(gtk::Align::Start);
        controls.attach(&heading(tr!("Modus")), 0, 1, 1, 1);
        controls.attach(&mode.root, 1, 1, 1, 1);

        let swatch = gtk::DrawingArea::new();
        swatch.set_content_width(20);
        swatch.set_content_height(20);
        let swatch_color = Rc::new(Cell::new(None::<[u8; 3]>));
        let fill = swatch_color.clone();
        swatch.set_draw_func(move |_, context, width, height| {
            if let Some([red, green, blue]) = fill.get() {
                context.set_source_rgb(f64::from(red) / 255.0, f64::from(green) / 255.0, f64::from(blue) / 255.0);
                context.arc(f64::from(width) / 2.0, f64::from(height) / 2.0, 8.0, 0.0, std::f64::consts::TAU);
                let _ = context.fill();
            }
        });
        let color = gtk::Entry::new();
        color.set_text("#3A7BF0");
        color.set_max_length(7);
        color.set_width_chars(8);
        color.set_max_width_chars(8);
        color.add_css_class("mono");
        color.update_property(&[gtk::accessible::Property::Label(
            tr!("Farge på kortet, seks heksadesimale siffer"),
        )]);
        // Fargeruten åpner en fargevelger med hele spekteret, hex og forhåndsvisning.
        let color_pick = gtk::Button::new();
        color_pick.set_child(Some(&swatch));
        color_pick.add_css_class("flat");
        color_pick.add_css_class("rgb-color-pick");
        color_pick.set_tooltip_text(Some(tr!("Velg farge …")));
        color_pick.update_property(&[gtk::accessible::Property::Label(tr!("Velg farge …"))]);
        let color_box = hbox(6, &[]);
        color_box.append(&color_pick);
        color_box.append(&color);
        let swatches_box = hbox(4, &["swatches"]);
        swatches_box.set_valign(gtk::Align::Center);
        swatches_box.update_property(&[gtk::accessible::Property::Label(tr!("Hurtigfarger"))]);
        for (hex, name_fn) in crate::rgb::SWATCHES {
            let b = gtk::Button::new();
            b.add_css_class("swatch");
            b.add_css_class("flat");
            let area = gtk::DrawingArea::new();
            area.set_content_width(18);
            area.set_content_height(18);
            let rgb = hex_color(hex).unwrap_or([58, 123, 240]);
            area.set_draw_func(move |_, ctx, w, h| {
                let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
                ctx.set_source_rgb(f64::from(rgb[0]) / 255.0, f64::from(rgb[1]) / 255.0, f64::from(rgb[2]) / 255.0);
                ctx.arc(cx, cy, 7.0, 0.0, std::f64::consts::TAU);
                let _ = ctx.fill();
                ctx.set_source_rgba(0.5, 0.5, 0.55, 0.35);
                ctx.set_line_width(1.0);
                ctx.arc(cx, cy, 7.0, 0.0, std::f64::consts::TAU);
                let _ = ctx.stroke();
            });
            b.set_child(Some(&area));
            let title = name_fn();
            b.set_tooltip_text(Some(title));
            b.update_property(&[gtk::accessible::Property::Label(title)]);
            // Bare fargen endres. Hurtigfargen slår ikke på lyset eller bytter
            // modus i det skjulte (det ga en uventet prøve med lysstyrke 0 i 1.6).
            let entry = color.clone();
            b.connect_clicked(move |_| entry.set_text(hex));
            swatches_box.append(&b);
        }
        let color_row = flow();
        color_row.insert(&color_box, -1);
        color_row.insert(&swatches_box, -1);
        controls.attach(&heading(tr!("Farge")), 0, 2, 1, 1);
        controls.attach(&color_row, 1, 2, 1, 1);

        let brightness = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
        brightness.set_value(80.0);
        brightness.set_digits(0);
        brightness.set_width_chars(3);
        // Samme tette tallfelt som grensefeltene på Grafer.
        brightness.add_css_class("thr-field");
        brightness.update_property(&[gtk::accessible::Property::Label(
            tr!("Kortlysets lysstyrke i prosent"),
        )]);
        let slider = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&brightness.adjustment()));
        slider.set_draw_value(false);
        // Samme tette glider som resten av styringen (ellers GTKs oransje standard).
        slider.add_css_class("ctl-scale");
        // Ønsker 260 px, men kan krympe: en fast bredde holdt vinduet bredere
        // enn 640 px med luftig tetthet.
        slider.set_size_request(120, -1);
        slider.update_property(&[gtk::accessible::Property::Label(
            tr!("Kortlysets lysstyrke i prosent"),
        )]);
        let brightness_box = hbox(8, &[]);
        brightness_box.append(&crate::widgets::fixed::Preferred::new(&slider, 260));
        brightness_box.append(&brightness);
        brightness_box.append(&label("%", &["muted"]));
        controls.attach(&heading(tr!("Lysstyrke")), 0, 3, 1, 1);
        controls.attach(&brightness_box, 1, 3, 1, 1);
        panel.body.append(&controls);
        let varig = gtk::CheckButton::with_label(tr!("Behold også etter at appen lukkes"));
        varig.set_active(app.settings().rgb_varig);
        varig.add_css_class("rgb-card-varig");
        panel.body.append(&varig);
        let varig_info = paragraph("", &["sm", "muted"]);
        panel.body.append(&varig_info);

        let effect_info = paragraph("", &["sm", "muted"]);
        panel.body.append(&effect_info);
        let runtime_error = paragraph("", &["sm", "warn"]);
        panel.body.append(&runtime_error);
        let recovery = paragraph("", &["sm"]);
        panel.body.append(&recovery);

        // Én tydelig handlingsboks når lyset må avklares, i stedet for fire knapper.
        let action_box = vbox(6, &["rgb-action-box"]);
        let action_title = label("", &["bold"]);
        action_title.set_xalign(0.0);
        action_title.set_wrap(true);
        let action_text = paragraph("", &["sm"]);
        let action_buttons = flow();
        action_box.append(&action_title);
        action_box.append(&action_text);
        action_box.append(&action_buttons);
        action_box.set_visible(false);


        let actions = flow();
        let apply = button(Some("check"), Some(tr!("Bruk")), &["suggested"]);
        let reset = button(
            Some("rotate-ccw"),
            Some(tr!("Gjenopprett utgangspunkt")),
            &["sm", "flat"],
        );
        let release = button(None, Some(tr!("Avslutt lysstyring")), &["sm"]);
        let ack = button(Some("check"), Some(tr!("Bekreft lyset …")), &["sm", "suggested"]);
        for b in [&apply, &release] {
            actions.insert(b, -1);
        }
        action_buttons.insert(&ack, -1);
        action_buttons.insert(&reset, -1);
        panel.body.append(&actions);
        let apply_hint = paragraph("", &["muted", "sm"]);
        panel.body.append(&apply_hint);

        let pending = vbox(6, &["rgb-card-pending"]);
        let deadline = paragraph("", &["bold"]);
        pending.append(&deadline);
        let pending_hint = paragraph("", &["sm"]);
        pending.append(&pending_hint);
        let confirmation = flow();
        let keep = button(Some("check"), Some(tr!("Behold")), &["sm", "suggested"]);
        let revert = button(Some("undo-2"), Some(tr!("Rull tilbake")), &["sm"]);
        confirmation.insert(&keep, -1);
        confirmation.insert(&revert, -1);
        pending.append(&confirmation);
        panel.body.append(&pending);

        // Sjeldne ting: avlesning, kortidentitet og administratorhandlinger.
        let details_box = vbox(6, &["rgb-card-details"]);
        let identity = paragraph("", &["muted", "sm"]);
        let observation = paragraph(
            tr!("Avlesning: ukjent. Kortet er ikke undersøkt av denne visningen."),
            &["sm"],
        );
        let confirmed = paragraph("", &["muted", "sm"]);
        let readback_note = paragraph(tr!("Farge og lysstyrke kan leses tilbake. Effektmodusen kan ikke leses; en visuell bekreftelse er din observasjon."), &["muted", "sm"]);
        let support = paragraph(tr!("Agentic HW Monitor støtter Palit GeForce RTX 2080 Super JetStream via den separate RGB-hjelperen."), &["muted", "sm"]);
        for l in [&identity, &observation, &confirmed, &readback_note, &support] {
            details_box.append(l);
        }
        let methods = av_methods();
        let av_method = gtk::DropDown::from_strings(&methods.map(|(_, text)| text));
        let current = app.settings().rgb_av.clone();
        av_method.set_selected(methods.iter().position(|(id, _)| *id == current).unwrap_or(0) as u32);
        av_method.set_halign(gtk::Align::Start);
        av_method.update_property(&[gtk::accessible::Property::Label(tr!("Skrivemåte for «Av»"))]);
        let av_row = hbox(8, &[]);
        av_row.append(&label(tr!("Skrivemåte for «Av»"), &["sm"]));
        av_row.append(&av_method);
        details_box.append(&av_row);
        let details_actions = flow();
        let refresh = button(Some("refresh-cw"), Some(tr!("Sjekk igjen")), &["sm", "flat"]);
        let settings = button(Some("settings"), Some(tr!("Åpne Innstillinger › Skjermkortstyring")), &["sm"]);
        let keep_observed = button(Some("check"), None, &["sm"]);
        action_buttons.insert(&settings, -1);
        action_buttons.insert(&keep_observed, -1);
        let factory = button(Some("rotate-ccw"), Some(tr!("Tilbake til fabrikklyset")), &["sm", "flat"]);
        action_buttons.insert(&refresh, -1);
        details_actions.insert(&factory, -1);
        details_box.append(&details_actions);
        let details = gtk::Expander::new(Some(tr!("Detaljer og feilsøking")));
        details.add_css_class("rgb-card-details-expander");
        details.set_child(Some(&details_box));
        panel.body.append(&details);

        let p = Rc::new(Self {
            root: panel.root,
            on_visual: RefCell::new(None),
            last_visual: RefCell::new(None),
            app: app.clone(),
            helper: RefCell::new(Value::Null),
            visible: Cell::new(false),
            reading: Cell::new(false),
            busy: Cell::new(false),
            fixture: Cell::new(false),
            poll_at: Cell::new(None),
            timer: RefCell::new(None),
            message: RefCell::new(None),
            badge,
            summary,
            identity,
            observation,
            confirmed,
            recovery,
            effect_info,
            runtime_error,
            power,
            mode,
            color,
            color_pick,
            swatch,
            swatch_color,
            swatches_box,
            controls,
            brightness_box,
            details,
            followups: RefCell::new(Default::default()),
            hold_intent: Cell::new(false),
            av_method,
            edited: Cell::new(false),
            syncing: Cell::new(false),
            last_synced: RefCell::new(Value::Null),
            varig,
            varig_info,
            action_box,
            action_title,
            action_text,
            factory,
            last_state: RefCell::new(String::new()),
            support,
            readback_note,
            brightness,
            apply,
            apply_hint,
            settings,
            keep_observed,
            refresh,
            reset,
            release,
            ack,
            pending,
            deadline,
            pending_hint,
            keep,
            revert,
        });
        CURRENT.with(|c| *c.borrow_mut() = Rc::downgrade(&p));
        let weak = Rc::downgrade(&p);
        crate::system_install::on_installed(move || weak.upgrade().map(|p| p.refresh()).is_some());
        p.wire();
        p.render();
        p
    }

    /// Brukeren endret et valg: et ventende «Bruk» fra før aktiveringen
    /// gjelder ikke lenger, og kortets tilstand skal ikke overskrive valget.
    fn user_edit(&self) {
        if self.syncing.get() {
            return;
        }
        self.edited.set(true);
    }

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.mode.connect(move |_| {
            if let Some(p) = weak.upgrade() {
                p.user_edit();
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.power.connect_active_notify(move |_| {
            if let Some(p) = weak.upgrade() {
                p.user_edit();
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.color.connect_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.user_edit();
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.brightness.connect_value_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.user_edit();
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.color_pick.connect_clicked(move |b| {
            let Some(p) = weak.upgrade() else { return };
            let dialog = gtk::ColorDialog::new();
            dialog.set_title(tr!("Velg farge"));
            dialog.set_with_alpha(false);
            dialog.set_modal(true);
            let [r, g, bl] = hex_color(&p.color.text()).unwrap_or([58, 123, 240]);
            let initial = gtk::gdk::RGBA::new(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(bl) / 255.0, 1.0);
            let window = b.root().and_downcast::<gtk::Window>();
            let entry = p.color.clone();
            dialog.choose_rgba(window.as_ref(), Some(&initial), gtk::gio::Cancellable::NONE, move |result| {
                if let Ok(rgba) = result {
                    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    entry.set_text(&format!("#{:02X}{:02X}{:02X}", channel(rgba.red()), channel(rgba.green()), channel(rgba.blue())));
                }
            });
        });
        let weak = Rc::downgrade(self);
        self.varig.connect_toggled(move |b| {
            if let Some(p) = weak.upgrade() {
                p.app.set(json!({"rgbVarig": b.is_active()}));
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.av_method.connect_selected_notify(move |d| {
            if let Some(p) = weak.upgrade()
                && let Some((id, _)) = av_methods().get(d.selected() as usize)
                && p.app.settings().rgb_av != *id
            {
                p.app.set(json!({"rgbAv": id}));
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.factory.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let factory = p.helper.borrow()["status"]["factory"].clone();
            // Fabrikklyset prøves som et vanlig valg: «Behold» gjør det varig igjen.
            if let Ok(Request::Static { color, brightness }) = serde_json::from_value::<Request>(factory) {
                p.syncing.set(true);
                p.power.set_active(true);
                p.mode.set("static");
                p.color.set_text(&format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]));
                p.brightness.set_value(f64::from(brightness.get()));
                p.syncing.set(false);
                p.edited.set(true);
                p.apply_with("fabrikklys");
            }
        });
        let weak = Rc::downgrade(self);
        self.apply.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.apply();
            }
        });
        let weak = Rc::downgrade(self);
        self.refresh.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let clean =
                    should_read_clean(&p.helper.borrow(), selected_identity(&p.app).as_ref());
                if clean {
                    p.operation("rgb_read", json!({}));
                } else {
                    p.refresh();
                }
            }
        });
        for (button, op) in [
            (&self.keep, "rgb_confirm"),
            (&self.revert, "rgb_revert"),
            (&self.reset, "rgb_reset"),
            (&self.release, "rgb_release"),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.operation(op, json!({}));
                }
            });
        }
        let a = self.app.clone();
        self.settings.connect_clicked(move |_| a.open_settings("styring"));
        let weak = Rc::downgrade(self);
        self.keep_observed.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let review = p.helper.borrow()["status"]["review"].clone();
            if review["expected"].is_object() && review["id"].is_string() {
                p.operation("rgb_keep_observed", json!({"confirmedStatic":true,"expected":review["expected"],"recoveryId":review["id"]}));
            }
        });
        let weak = Rc::downgrade(self);
        self.ack.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let status = p.helper.borrow()["status"].clone();
            let expected = status["state"]["confirmed"].clone();
            let Some(recovery_id) = status["recoveryId"].as_str().map(str::to_owned) else {
                return;
            };
            let selected = selected_identity(&p.app);
            let check = if looks_off(&expected) {
                tr!("Jeg ser at lyset på kortet er slukket.")
            } else { tr!("Jeg ser at lyset viser den forventede faste fargen.") };
            let w = Rc::downgrade(&p);
            confirm(&p.app, tr!("Bekreft lyset på kortet"), &trf!("Forventet lys: {lys}.\n\nSe på det fysiske kortet. Samsvarende fargeverdier alene er ikke nok. Hjelperen leser verdiene på nytt før observasjonen din lagres.", lys = request_text(&expected)), check, tr!("Bekreft min observasjon"), move || {
                let Some(p) = w.upgrade() else { return };
                if selected_identity(&p.app) != selected
                    || p.helper.borrow()["status"]["state"]["confirmed"] != expected
                    || p.helper.borrow()["status"]["recoveryId"] != recovery_id
                {
                    p.app.toast(Toast::new(tr!("Lystilstanden er endret")).body(tr!("Kontroller det nye forventede lyset og åpne bekreftelsen på nytt.")));
                    p.refresh();
                    return;
                }
                p.operation("rgb_ack_recovery", json!({"confirmedStatic": true, "expected": expected, "recoveryId": recovery_id}));
            });
        });
        let weak = Rc::downgrade(self);
        self.app.on_conn.connect(move |_| {
            if let Some(p) = weak.upgrade() {
                p.render();
                if p.visible.get() {
                    p.refresh();
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.app.on_devices.connect(move |_| {
            if let Some(p) = weak.upgrade() {
                p.render();
                if p.visible.get() {
                    p.refresh();
                }
            }
        });
    }

    pub fn show(self: &Rc<Self>) {
        self.visible.set(true);
        self.refresh();
        if self.timer.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        *self.timer.borrow_mut() = Some(glib::timeout_add_local(
            Duration::from_millis(500),
            move || {
                let Some(p) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if !p.visible.get() {
                    return glib::ControlFlow::Break;
                }
                p.render();
                let h = p.helper.borrow();
                let quick = h["owned"] == true || h["status"]["state"]["pending"].is_object();
                drop(h);
                let delay = if quick { 500 } else { 2000 };
                if p.poll_at
                    .get()
                    .is_none_or(|t| t.elapsed() >= Duration::from_millis(delay))
                {
                    p.refresh();
                }
                glib::ControlFlow::Continue
            },
        ));
    }

    pub fn hide(&self) {
        self.visible.set(false);
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        // Valget beholdes mens vinduet lever. Daemonen eier heartbeat og
        // håndterer vindustap, også når Lys-siden er skjult.
    }

    fn refresh(self: &Rc<Self>) {
        if self.fixture.get()
            || self.reading.get()
            || self.busy.get()
            || !self.app.client.is_connected()
        {
            return;
        }
        self.reading.set(true);
        self.poll_at.set(Some(Instant::now()));
        let weak = Rc::downgrade(self);
        self.app
            .client
            .request("get_rgb_helper", json!({"refresh": true}), move |result| {
                let Some(p) = weak.upgrade() else { return };
                p.reading.set(false);
                if p.fixture.get() {
                    return;
                }
                *p.helper.borrow_mut() = match result {
                    Ok(value) => value,
                    Err(error) => {
                        json!({"state": "error", "message": error.message, "status": null})
                    }
                };
                p.render();
            });
    }

    /// Valget i kontrollene som `rgb_apply`-argumenter, og om det er «Av» som
    /// skal holdes. «Av» skrives slik innstillingen `rgbAv` sier; med «hold»
    /// startes keepalive så snart prøven er skrevet (se [`Followup::Hold`]).
    fn request_args(&self) -> Option<(Value, bool)> {
        let color = hex_color(&self.color.text());
        if !self.power.is_active() {
            let method = self.app.settings().rgb_av.clone();
            let (request, hold) = off_request(&method, color.unwrap_or([58, 123, 240]));
            return Some((json!({"request": request, "confirmTimeoutS": 15}), hold));
        }
        let mode = self.mode.value().unwrap_or_else(|| "static".into());
        let brightness = u8::try_from(self.brightness.value_as_int()).ok().and_then(|v| Brightness::try_from(v).ok())?;
        let mut args = json!({"request": Request::Static { color: color?, brightness }, "confirmTimeoutS": 15});
        if effect_name(&mode).is_some() {
            args["effect"] = json!(mode);
        }
        Some((args, false))
    }

    fn apply(self: &Rc<Self>) {
        self.apply_with("bruk-knapp");
    }

    fn apply_with(self: &Rc<Self>, source: &'static str) {
        if !card_present(&self.app) {
            return;
        }
        if !usable(&self.app, &self.helper.borrow(), selected_identity(&self.app).as_ref()) {
            return;
        }
        let Some((mut args, hold)) = self.request_args() else { return };
        let effect = args["effect"].as_str().map(str::to_owned);
        if !can_apply(&self.helper.borrow()["status"], effect.as_deref()) {
            return;
        }
        args["kilde"] = json!(source);
        // Står kortet allerede på dette «Av» (lagret, eller etter «Slutt å
        // holde av»), gir prøven «uendret» og ingen skriving; «hold» startes da
        // direkte som oppfølging.
        self.hold_intent.set(hold);
        self.operation("rgb_apply", args);
    }

    /// Sender ett RGB-kall. `false`: kallet ble ikke sendt (opptatt, ikke
    /// eier, frakoblet …); et oppfølgingssteg melder da fra selv.
    fn operation(self: &Rc<Self>, op: &'static str, mut args: Value) -> bool {
        // Gjelder bare dette kallet, også når det ikke blir sendt.
        let hold_intent = op == "rgb_apply" && self.hold_intent.replace(false);
        if self.fixture.get() || self.busy.get() || !self.app.client.is_connected() {
            return false;
        }
        let h = self.helper.borrow().clone();
        if flow_requested(&self.app)
            && let Err(error) = flow_environment(&self.app, &h)
        {
            *self.message.borrow_mut() = Some(error);
            self.render();
            return false;
        }
        let selected = selected_identity(&self.app);
        if !usable(&self.app, &h, selected.as_ref()) || h["busy"] == true {
            return false;
        }
        let Some((pci, uuid)) = selected else { return false };
        if matches!(op, "rgb_confirm" | "rgb_revert") {
            if h["owned"] != true {
                return false;
            }
            let Some(txn) = h["status"]["state"]["pending"]["txn"].as_u64() else {
                return false;
            };
            args["txn"] = json!(txn);
        }
        if op == "rgb_release" && h["owned"] != true {
            return false;
        }
        args["gpu"] = json!(pci);
        args["uuid"] = json!(uuid);
        // Hvor kallet kom fra, til tjenestens logg (`RGB: … fra …`).
        if args.get("kilde").is_none() {
            args["kilde"] = json!(format!("{}-knapp", op.trim_start_matches("rgb_").replace('_', "-")));
        }
        // Det som beholdes nå, for oppfølgingen (varig, «hold»).
        let kept = (op == "rgb_confirm").then(|| (h["status"]["state"]["pending"]["requested"].clone(), h["status"]["effect"].as_str().map(str::to_owned)));
        if !matches!(op, "rgb_persist" | "rgb_hold") {
            // Et nytt brukervalg erstatter oppfølginger fra forrige.
            self.followups.borrow_mut().clear();
        }
        self.busy.set(true);
        self.message.borrow_mut().take();
        self.render();
        let weak = Rc::downgrade(self);
        self.app.client.request(op, args, move |result| {
            let Some(p) = weak.upgrade() else { return };
            p.busy.set(false);
            match result {
                Ok(value) => {
                    if value["status"].is_object() {
                        p.helper.borrow_mut()["status"] = value["status"].clone();
                    }
                    // Eierskapet etter kallet (fra tjenesten), så neste steg
                    // ikke avvises av en eldre status (1.7.0-kontrollen, B1).
                    if let Some(owned) = value["owned"].as_bool() {
                        p.helper.borrow_mut()["owned"] = json!(owned);
                    }
                    let outcome = value["outcome"]["status"].as_str();
                    if hold_intent
                        && matches!(outcome, Some("pending" | "unchanged"))
                        && value["status"]["hold"] == true
                        && value["status"]["keepalive"] != true
                    {
                        p.followups.borrow_mut().push_back(Followup::Hold);
                    }
                    if op == "rgb_release" && let Some(card) = selected_identity(&p.app) {
                        STOPPED_HOLD.with(|s| s.borrow_mut().insert(card));
                    }
                    if hold_intent && let Some(card) = selected_identity(&p.app) {
                        STOPPED_HOLD.with(|s| s.borrow_mut().remove(&card));
                    }
                    if matches!(op, "rgb_ack_recovery" | "rgb_keep_observed") {
                        AUTO_HOLD_INSTANCE.with(|v| v.borrow_mut().take());
                        try_auto_hold(&p.app);
                    }
                    if op == "rgb_apply" && outcome == Some("unchanged") && p.varig.is_active() && value["status"]["effect"].is_null() {
                        p.followups.borrow_mut().push_front(Followup::Persist);
                    }
                    if let Some((request, effect)) = kept {
                        p.plan_followups(&request, effect.as_deref());
                        p.edited.set(false);
                    }
                    if let Some(next) = p.followups.borrow_mut().pop_front() {
                        glib::idle_add_local_once({
                            let p = p.clone();
                            move || p.run_followup(next)
                        });
                    }
                }
                Err(error) => {
                    p.followups.borrow_mut().clear();
                    if let Some(details) = &error.details
                        && details["status"].is_object()
                    {
                        p.helper.borrow_mut()["status"] = details["status"].clone();
                    }
                    let drift = op == "rgb_hold" && p.helper.borrow()["status"]["drift"] == true;
                    let message = if drift {
                        tr!("Kortet viser ikke det lagrede utgangspunktet. Se boksen på siden: «Gjenopprett utgangspunktet» setter det tilbake.").to_string()
                    } else {
                        operation_error(&error)
                    };
                    *p.message.borrow_mut() = Some(message.clone());
                    p.summary.set_tooltip_text(Some(&error.message));
                    let title = match op {
                        "rgb_read" => tr!("Kunne ikke lese kortlyset"),
                        "rgb_persist" => tr!("Valget ble ikke lagret som varig"),
                        "rgb_hold" => tr!("«Av» holdes ikke"),
                        _ => tr!("Lysendringen ble ikke fullført"),
                    };
                    p.app.toast(Toast::new(title).body(&message).error());
                }
            }
            p.render();
            p.refresh();
        });
        true
    }

    /// Etter «Behold» på et fast valg eller «Av»: lagre som varig når brukeren
    /// vil det. «Hold» startet allerede under prøven og fortsetter; startet det
    /// ikke da, prøves det igjen her.
    fn plan_followups(&self, kept: &Value, effect: Option<&str>) {
        let mut queue = self.followups.borrow_mut();
        queue.clear();
        if effect.is_some() {
            return;
        }
        let status = &self.helper.borrow()["status"];
        if self.varig.is_active() && status["persist"] == true {
            queue.push_back(Followup::Persist);
        }
        let (_, hold) = off_request(&self.app.settings().rgb_av, [0, 0, 0]);
        if hold && status["hold"] == true && status["keepalive"] != true && looks_off(kept) && !self.power.is_active() {
            queue.push_back(Followup::Hold);
        }
    }

    /// Ett oppfølgingssteg. Blir det ikke sendt, sier siden fra: et steg som
    /// stille faller bort, var feilen bak «Av» som ikke holdt (1.7.0-kontrollen).
    fn run_followup(self: &Rc<Self>, step: Followup) {
        let sent = match step {
            Followup::Persist => self.operation("rgb_persist", json!({"kilde": "varig-etter-behold"})),
            Followup::Hold => self.operation("rgb_hold", json!({"kilde": "hold-start"})),
        };
        if sent {
            return;
        }
        self.followups.borrow_mut().clear();
        let (title, body) = match step {
            Followup::Persist => (
                tr!("Valget ble ikke lagret som varig"),
                tr!("Lyset er beholdt, men ikke lagret som kortets utgangspunkt. Trykk «Bruk» og «Behold» igjen for å prøve på nytt."),
            ),
            Followup::Hold => (
                tr!("«Av» holdes ikke"),
                tr!("«Av» er satt, men skrives ikke på nytt mens appen er åpen. Trykk «Bruk» igjen for å starte det."),
            ),
        };
        *self.message.borrow_mut() = Some(body.to_string());
        self.app.toast(Toast::new(title).body(body).error());
        self.render();
    }

    /// Viser kortets beholdte valg i kontrollene, så lenge brukeren ikke har
    /// endret noe selv (aldri midt i en prøve, og aldri en sending).
    fn sync_from_helper(&self) {
        let target = {
            let helper = self.helper.borrow();
            let state = &helper["status"]["state"];
            let selected = selected_identity(&self.app);
            if self.edited.get() || self.busy.get() || self.fixture.get() || !state["pending"].is_null()
                || !bound_status(&helper, selected.as_ref())
            {
                return;
            }
            let target = json!({"confirmed": state["confirmed"], "effect": helper["status"]["effect"]});
            if *self.last_synced.borrow() == target {
                return;
            }
            *self.last_synced.borrow_mut() = target.clone();
            target
        };
        self.syncing.set(true);
        let effect = target["effect"].as_str();
        if effect.is_none() && looks_off(&target["confirmed"]) {
            self.power.set_active(false);
        } else {
            self.power.set_active(true);
            if let Ok(Request::Static { color, brightness }) = serde_json::from_value::<Request>(target["confirmed"].clone()) {
                self.color.set_text(&format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]));
                self.brightness.set_value(f64::from(brightness.get()));
            }
            self.mode.set(effect.filter(|e| effect_name(e).is_some()).unwrap_or("static"));
        }
        self.syncing.set(false);
    }

    pub fn render(&self) {
        self.sync_from_helper();
        let helper = self.helper.borrow();
        let status = &helper["status"];
        let state = &status["state"];
        let selected = selected_identity(&self.app);
        let reported = &status["identity"];
        let same_card = selected.as_ref().is_some_and(|(pci, uuid)| reported["pci"].as_str() == Some(pci) && reported["uuid"].as_str() == Some(uuid));
        let unsupported = same_card && helper["state"] == "unsupported";
        let stale = rgb_stale(&self.app, &helper);
        let bound = bound_status(&helper, selected.as_ref()) && !stale;
        let connected = self.app.client.is_connected();
        let present = card_present(&self.app);
        let owned = helper["owned"] == true;
        let other = helper["busy"] == true && !owned;
        let pending = state["pending"].is_object();
        // «Hold» er ikke en effekt for brukeren: det er «Av» som holdes mens appen er åpen.
        let holding_off = status["keepalive"] == true;
        let worker_effect = status["effect"].as_str();
        let worker_effect_name = worker_effect.and_then(effect_name);
        for (id, _) in EFFECTS {
            if let Some(button) = self.mode.button(id) {
                // Før aktivering vises alle valg; etterpå bare det hjelperen støtter.
                button.set_visible(!bound || supports_effect(status, id));
            }
        }
        if let Some(selected_mode) = self.mode.value()
            && effect_name(&selected_mode).is_some()
            && bound
            && !supports_effect(status, &selected_mode)
        {
            self.mode.set("static");
        }
        let selected_mode = self.mode.value().unwrap_or_else(|| "static".into());
        // Med kortlyset slått av er modusen bare et lagret valg, ikke det som sendes.
        let chosen_effect = effect_name(&selected_mode).filter(|_| self.power.is_active());
        let blocked = is_blocked(state);
        // Kortet har selv endret seg i en ren økt (ingen sperre; se boksen).
        let drift = bound && !pending && status["drift"] == true && state["dirty"] == false;
        let available = connected && bound && !self.busy.get() && helper["busy"] != true;
        let admin_ui = helper["systemAdmin"] == true;
        let newer_installed = admin_ui && !self.fixture.get() && !self.app.profile.is_dev()
            && crate::system_install::newer_installed();
        let badge = if self.busy.get() {
            tr!("Venter …")
        } else if !connected {
            tr!("Frakoblet")
        } else if !present {
            tr!("Kortet er utilgjengelig")
        } else if !bound && (newer_installed || helper["state"] == "error") {
            tr!("Ikke klar")
        } else if !bound && (helper["state"] == "outdated" || stale) {
            tr!("Må oppdateres")
        } else if unsupported {
            tr!("Ikke støttet")
        } else if same_card && helper["state"] == "waiting" {
            tr!("Venter på enhet")
        } else if !bound {
            tr!("Ikke aktivert")
        } else if pending {
            tr!("Prøve pågår")
        } else if blocked {
            tr!("Må avklares")
        } else if drift {
            tr!("Avviker")
        } else if worker_effect.is_some() {
            tr!("Effekt aktiv")
        } else if holding_off || looks_off(&state["confirmed"]) {
            tr!("Lyset er av")
        } else {
            tr!("Klar")
        };
        set_text(
            &self.badge,
            &if self.fixture.get()
                || (flow_requested(&self.app) && status["testMarker"] == FLOW_MARKER)
            {
                trf!("Simulerte RGB-data · {status}", status = badge)
            } else {
                badge.to_string()
            },
        );
        class(
            &self.badge,
            "warn",
            connected && bound && (pending || blocked || drift),
        );
        let message = if self.busy.get() {
            tr!("Venter på svar. Ingen ny endring kan sendes nå.").into()
        } else if !connected {
            tr!("Forbindelsen til tjenesten er brutt. Hjelperen håndterer eventuell tilbakeføring.")
                .into()
        } else if let Some(message) = self.message.borrow().clone() {
            message
        } else if !present {
            tr!("Kortet er ikke tilgjengelig. Nye lysendringer og aktivering er sperret.").into()
        } else if other {
            tr!("Et annet vindu styrer kortlyset. Her vises bare status.").into()
        } else if bound {
            tr!("Hele kortlyset er én sone. Ingenting sendes før du trykker «Bruk».").into()
        } else if unsupported {
            tr!("Ikke støttet").into()
        } else if same_card && helper["state"] == "waiting" {
            with_diagnostic(tr!("Venter på enhet"), helper["message"].as_str())
        } else if helper["state"] == "error" || stale || newer_installed || helper["state"] == "outdated" {
            with_diagnostic(tr!("RGB-hjelperen er ikke klar."), helper["message"].as_str())
        } else {
            tr!("Skjermkortstyring er slått av.").into()
        };
        set_text(&self.summary, &message);
        // Må avklares eller oppdateres: varselfarge, så det ikke forsvinner i resten.
        class(&self.summary, "warn", stale && self.message.borrow().is_none());
        if newer_installed {
            self.summary.set_tooltip_text(Some(&message));
        } else if self.message.borrow().is_none() {
            self.summary.set_tooltip_text(helper["message"].as_str());
        }
        set_text(
            &self.identity,
            &selected.as_ref().map_or_else(
                || tr!("Venter på kortets PCI-adresse og UUID.").into(),
                |(pci, _)| trf!("Valgt kort: {pci}", pci = pci),
            ),
        );
        let effect_info = if let Some(name) = worker_effect_name {
            if pending {
                trf!(
                    "{effekt} er valgt. Nå vises bare startfargen; animasjonen starter først etter «Behold».", effekt = name
                )
            } else {
                trf!(
                    "{effekt} er aktiv på hele kortlyset. Velg en annen modus eller farge og trykk «Bruk» for å bytte, eller «Stopp effekten» for å gå tilbake til utgangspunktet.", effekt = name
                )
            }
        } else if chosen_effect.is_some() {
            let detail = match selected_mode.as_str() {
                "breathing" => tr!("Pusting varierer lysstyrken i fargen du velger."),
                "rainbow" => {
                    tr!("Regnbue går gjennom hele fargespekteret fra fargetonen du velger. Gråtoner starter på rødt.")
                }
                _ => {
                    tr!("Fargesyklus veksler mellom seks tydelige fargetoner fra fargetonen du velger. Gråtoner starter på rødt.")
                }
            };
            trf!(
                "{detalj} Først prøves startfargen; «Behold» starter animasjonen på én lyssone.", detalj = detail
            )
        } else {
            String::new()
        };
        set_text(&self.effect_info, &effect_info);
        self.effect_info.set_visible(!effect_info.is_empty());
        let runtime_error = status["runtimeError"].as_str().unwrap_or("");
        set_text(&self.runtime_error, tr!("RGB-hjelperen meldte en driftsfeil. Kontroller lyset før du fortsetter."));
        self.runtime_error.set_tooltip_text(status["runtimeError"].as_str());
        self.runtime_error
            .set_visible(bound && !runtime_error.is_empty());
        let source = status["observationSource"].as_str().unwrap_or("unknown");
        let observed = &state["observed"];
        let values = color_text(&observed["color"])
            .zip(observed["brightnessRaw"].as_u64())
            .map(|(c, b)| {
                if b <= 100 {
                    format!("{c} · {}", fmt::pct(Some(b as f64), 0))
                } else {
                    trf!("{farge} · rå lysstyrke {styrke}, utenfor støttet område", farge = c, styrke = fmt::num(Some(b as f64), 0))
                }
            });
        let observation = match (source, values) {
            ("stored_baseline", Some(v)) => {
                trf!("Lagret utgangspunkt: {lys}. Dette er ikke en ny avlesning.", lys = v)
            }
            ("operation_readback", Some(v)) => {
                trf!("Sist lest fra kortet: {lys}. Effektmodus: ukjent.", lys = v)
            }
            _ => tr!("Avlesning: ukjent. Effektmodusen er ikke bekreftet.").into(),
        };
        set_text(&self.observation, &observation);
        self.observation.set_visible(bound);
        set_text(
            &self.confirmed,
            &trf!("Sist bekreftede valg: {lys}. Valget er ikke bevis på avlest effektmodus.", lys = request_text(&state["confirmed"])),
        );
        self.confirmed.set_visible(bound);
        let recovery = recovery_text(state);
        set_text(&self.recovery, &recovery);
        self.recovery.set_tooltip_text(state["readError"].as_str().or_else(|| state["error"].as_str()));
        // Teksten står i handlingsboksen; avsnittet brukes ikke lenger alene.
        self.recovery.set_visible(false);
        self.pending.set_visible(pending && bound);
        set_text(
            &self.pending_hint,
            if worker_effect.is_some() {
                tr!("Kontroller startfargen på kortet. «Behold» starter animasjonen; «Rull tilbake» eller fristutløp går tilbake til utgangspunktet.")
            } else {
                tr!("Se på det fysiske kortet før du beholder endringen. Uten bekreftelse forsøker hjelperen å rulle tilbake.")
            },
        );
        let remaining = state["pending"]["deadlineMs"]
            .as_u64()
            .zip(boottime_ms())
            .map(|(end, now)| end.saturating_sub(now).div_ceil(1000));
        set_text(
            &self.deadline,
            &format!(
                "{} · {}",
                request_heading(&state["pending"]["requested"]),
                remaining.map_or_else(
                    || tr!("fristen styres av hjelperen").into(),
                    |s| if s == 0 {
                        tr!("fristen er ute; venter på status").into()
                    } else {
                        trf!("bekreft innen {tid}", tid = fmt::with_unit(fmt::num(Some(s as f64), 0), "s"))
                    }
                )
            ),
        );
        self.keep
            .set_sensitive(available && owned && remaining != Some(0));
        self.revert.set_sensitive(available && owned);
        let off = !self.power.is_active();
        let valid_color = hex_color(&self.color.text()).is_some();
        class(&self.color, "error", !valid_color && !off);
        let color = (!off).then(|| hex_color(&self.color.text())).flatten();
        if self.swatch_color.replace(color) != color {
            self.swatch.queue_draw();
        }
        self.support.set_visible(!bound);
        self.controls.set_visible(present && bound && status["review"].is_null());
        self.apply.set_visible(present);
        self.readback_note.set_visible(bound);
        self.color.set_sensitive(!off && !self.busy.get());
        self.color_pick.set_sensitive(!off && !self.busy.get());
        self.swatches_box.set_sensitive(!off && !self.busy.get());
        self.power.set_sensitive(!self.busy.get());
        self.brightness_box.set_sensitive(!off && !self.busy.get());
        self.mode.root.set_sensitive(!off && !self.busy.get());
        let chosen_id = chosen_effect.map(|_| selected_mode.as_str());
        let apply_ok = available && present && (off || valid_color) && can_apply(status, chosen_id);
        self.apply.set_sensitive(apply_ok);
        let apply_hint = if apply_ok || self.busy.get() || !connected || !present || other {
            ""
        } else if !off && !valid_color {
            tr!("Oppgi en gyldig fargekode på seks tegn, for eksempel #3A7BF0.")
        } else if !bound {
            ""
        } else if pending {
            tr!("Bekreft eller rull tilbake prøven som pågår før du bruker en ny innstilling.")
        } else if blocked {
            ""
        } else if chosen_id.is_some_and(|effect| !supports_effect(status, effect)) {
            tr!("RGB-hjelperen støtter ikke dette lysvalget.")
        } else {
            ""
        };
        set_text(&self.apply_hint, apply_hint);
        // Under en prøve står «Behold»/«Rull tilbake» i egen boks; «Bruk» er skjult.
        self.apply_hint.set_visible(!(apply_hint.is_empty() || pending && bound));
        self.refresh
            .set_sensitive(connected && !self.reading.get() && !self.busy.get());
        let needs_ack = bound && status["recoveryId"].is_string() && !pending;
        let runtime_failed = bound && status["runtimeError"].is_string();
        let must_resolve = bound && !pending && (blocked || runtime_failed) && !needs_ack;
        let action = needs_ack || must_resolve || drift;
        let review = &status["review"];
        let reviewing = bound && review.is_object();
        let action = action || reviewing || !bound;
        self.action_box.set_visible(action || self.message.borrow().is_some());
        self.refresh.set_visible(reviewing || must_resolve || (!bound && !unsupported));
        self.settings.set_visible(!bound && !unsupported);
        self.keep_observed.set_visible(reviewing);
        self.keep_observed.set_sensitive(available && review["expected"].is_object());
        self.keep_observed.set_label(if review["kind"] == "first" { tr!("Jeg ser én fast farge") } else { tr!("Behold lyset jeg ser") });
        if reviewing {
            set_text(&self.action_title, if review["expected"].is_null() { tr!("Avlesning av kortlyset mislyktes. Nye lysendringer er sperret.") } else { tr!("Jeg ser én fast farge") });
            let fields = trf!("Avlest lys: {lest}. Lagret lys: {lagret}. Fabrikklys: {fabrikk}.", lest = observed_text(observed), lagret = request_text(&status["baselineRequest"]), fabrikk = request_text(&status["factory"]));
            set_text(&self.action_text, &with_diagnostic(&fields, state["readError"].as_str().or(self.message.borrow().as_deref())));
        } else if !bound || (!action && self.message.borrow().is_some()) {
            set_text(&self.action_title, badge);
            set_text(&self.action_text, &message);
        } else if needs_ack {
            set_text(&self.action_title, tr!("Se på kortet og bekreft lyset"));
            set_text(&self.action_text, &trf!("Kortet er satt tilbake til {lys}. Hjelperen kan bare lese fargeverdiene, ikke hva kortet faktisk lyser. Bekreft når kortet viser dette, eller gjenopprett på nytt.", lys = request_text(&state["confirmed"])));
        } else if must_resolve {
            set_text(&self.action_title, tr!("Lyset må avklares"));
            let text = if recovery.is_empty() {
                tr!("RGB-hjelperen meldte en driftsfeil. Gjenopprett utgangspunktet og kontroller lyset før du fortsetter.").to_string()
            } else {
                recovery.clone()
            };
            let base = if status["baselineRequest"].is_object() { &status["baselineRequest"] } else { &state["confirmed"] };
            set_text(&self.action_text, &format!("{text} {}", trf!("Utgangspunktet er {lys}.", lys = request_text(base))));
        } else if drift {
            set_text(&self.action_title, tr!("Kortet viser ikke det lagrede utgangspunktet"));
            let now = observed_text(observed);
            set_text(&self.action_text, &trf!("Kortet viser nå {lest}, men det lagrede utgangspunktet er {lys}. Det er ingen endring fra appen: kortet kan ha gått tilbake til fabrikklyset av seg selv, for eksempel etter en kaldstart. «Gjenopprett utgangspunktet» skriver {lys} på nytt, og så bekrefter du det du ser på kortet. Det krever ikke passord.", lest = now, lys = request_text(&state["confirmed"])));
        }
        self.ack.set_visible(needs_ack);
        self.ack.set_sensitive(available);
        self.reset.set_visible(bound && action && review["kind"] != "first");
        self.reset.set_sensitive(available);
        set_text_button(&self.reset, if needs_ack { tr!("Gjenopprett på nytt") } else if looks_off(&status["baselineRequest"]) { tr!("Gjenopprett lagret Av") } else { tr!("Gjenopprett lagret lys") });
        self.apply.set_visible(present && !action && !(pending && bound));
        // Under en prøve står «Behold» og «Rull tilbake» i egen boks.
        self.release.set_visible(bound && owned && !action && !pending);
        self.release.set_sensitive(available);
        self.release.set_label(if worker_effect.is_some() {
            tr!("Stopp effekten")
        } else if holding_off {
            tr!("Slutt å holde av")
        } else {
            tr!("Avslutt lysstyring")
        });
        self.release.set_tooltip_text(Some(tr!("Kortet går tilbake til det lagrede utgangspunktet, og appen slipper lyset.")));
        // «Behold også etter at appen lukkes»: bare for fast farge og «Av».
        let persist_supported = bound && status["persist"] == true;
        let chosen_effect_now = chosen_effect.is_some();
        self.varig.set_visible(persist_supported && !action && !chosen_effect_now);
        self.varig.set_sensitive(!self.busy.get());
        self.av_method.set_sensitive(!self.busy.get());
        let resting = if status["baselineRequest"].is_object() { request_text(&status["baselineRequest"]) } else { tr!("det lagrede utgangspunktet").to_string() };
        let (_, hold) = off_request(&self.app.settings().rgb_av, [0, 0, 0]);
        let varig_text = if !bound || action {
            String::new()
        } else if chosen_effect_now || worker_effect.is_some() {
            trf!("Effekter kjører bare mens appen er åpen. Når appen lukkes, stopper effekten og kortet går tilbake til {lys}.", lys = resting)
        } else if off && hold && !pending && looks_off(&status["baselineRequest"]) && state["confirmed"] == status["baselineRequest"] {
            // «Av» er allerede lagret som utgangspunkt.
            if holding_off {
                tr!("«Av» er kortets utgangspunkt og skrives ikke tilbake når appen lukkes. Mens appen er åpen, skrives det på nytt hvert 2. sekund, fordi kortet ellers kan gå tilbake til fabrikklyset av seg selv. Det kan også skje etter at appen er lukket.").to_string()
            } else {
                tr!("«Av» er kortets utgangspunkt, men skrives ikke på nytt nå. Trykk «Bruk» for å holde det mens appen er åpen.").to_string()
            }
        } else if off && hold && persist_supported && self.varig.is_active() {
            tr!("«Av» blir kortets utgangspunkt etter «Behold» og skrives ikke tilbake når appen lukkes. Mens appen er åpen, skrives det på nytt hvert 2. sekund, fordi kortet ellers kan gå tilbake til fabrikklyset av seg selv. Det kan også skje etter at appen er lukket.").to_string()
        } else if off && hold {
            trf!("«Av» skrives på nytt hvert 2. sekund mens appen er åpen, fordi kortet ellers kan gå tilbake til fabrikklyset av seg selv. Når appen lukkes, går kortet tilbake til {lys}.", lys = resting)
        } else if persist_supported && self.varig.is_active() {
            tr!("Etter «Behold» blir valget kortets utgangspunkt og skrives ikke tilbake når appen lukkes. Om kortet selv beholder lyset etter omstart, er ikke verifisert.").to_string()
        } else {
            trf!("Når appen lukkes, går kortet tilbake til {lys}.", lys = resting)
        };
        set_text(&self.varig_info, &varig_text);
        self.varig_info.set_visible(!varig_text.is_empty());
        let factory = &status["factory"];
        self.factory.set_visible(
            bound && factory.is_object() && *factory != status["baselineRequest"] && !pending && !action,
        );
        self.factory.set_sensitive(available && can_apply(status, None));
        self.factory.set_tooltip_text(factory.is_object().then(|| trf!("Fabrikklyset: {lys}. Prøves som et vanlig valg; «Behold» gjør det til utgangspunktet igjen.", lys = request_text(factory))).as_deref());
        let state_name = helper["state"].as_str().unwrap_or("").to_owned();
        if state_name == "error" && *self.last_state.borrow() != "error" {
            self.details.set_expanded(true);
        }
        *self.last_state.borrow_mut() = state_name;
        let observed_color: Option<[u8; 3]> = serde_json::from_value(observed["color"].clone()).ok();
        let observed_brightness = observed["brightnessRaw"].as_u64().and_then(|v| u8::try_from(v).ok());
        let visual = VisualState {
            bound,
            pending: pending && bound,
            selected_mode: if off { "off".into() } else { selected_mode },
            selected_color: hex_color(&self.color.text()),
            selected_brightness: self.brightness.value_as_int().clamp(0, 100) as u8,
            active_effect: (bound && !pending)
                .then(|| worker_effect.map(str::to_owned))
                .flatten(),
            observed_color: bound.then_some(observed_color).flatten(),
            observed_brightness: bound.then_some(observed_brightness).flatten(),
            observation_source: source.to_string(),
        };
        let emit = self.last_visual.borrow().as_ref() != Some(&visual);
        if emit {
            *self.last_visual.borrow_mut() = Some(visual.clone());
            if let Some(cb) = self.on_visual.borrow().as_ref() {
                cb(&visual);
            }
        }
    }

    pub fn visual_state(&self) -> VisualState {
        self.last_visual.borrow().clone().unwrap_or(VisualState {
            bound: false,
            pending: false,
            selected_mode: "static".into(),
            selected_color: hex_color(&self.color.text()),
            selected_brightness: self.brightness.value_as_int().clamp(0, 100) as u8,
            active_effect: None,
            observed_color: None,
            observed_brightness: None,
            observation_source: "unknown".into(),
        })
    }

}

fn confirm(
    app: &Rc<App>,
    title: &str,
    text: &str,
    check: &str,
    action: &str,
    done: impl FnOnce() + 'static,
) {
    confirm_or_cancel(app, title, text, check, action, move |ok| {
        if ok {
            done();
        }
    });
}

type Answer = Box<dyn FnOnce(bool)>;

/// Som [`confirm`], men `done(false)` når dialogen lukkes uten bekreftelse
/// (eller ikke kan vises), så en ventende administratorhandling kan avbrytes.
fn confirm_or_cancel(
    app: &Rc<App>,
    title: &str,
    text: &str,
    check: &str,
    action: &str,
    done: impl FnOnce(bool) + 'static,
) {
    let Some((window, root)) = crate::shortcuts::dialog(app, title, 500, 320) else {
        done(false);
        return;
    };
    let content = vbox(12, &["rgb-card-dialog"]);
    content.append(&paragraph(text, &[]));
    let checked = gtk::CheckButton::new();
    checked.set_child(Some(&paragraph(check, &[])));
    content.append(&checked);
    let buttons = flow();
    let cancel = button(None, Some(tr!("Avbryt")), &[]);
    let accept = button(None, Some(action), &["suggested"]);
    if let Some(label) = accept.child().and_downcast::<gtk::Label>() {
        label.set_wrap(true);
    }
    accept.set_sensitive(false);
    buttons.insert(&cancel, -1);
    buttons.insert(&accept, -1);
    content.append(&buttons);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&content));
    root.append(&scroll);
    let b = accept.clone();
    checked.connect_toggled(move |c| b.set_sensitive(c.is_active()));
    let w = window.clone();
    cancel.connect_clicked(move |_| w.close());
    let once: Rc<RefCell<Option<Answer>>> = Rc::new(RefCell::new(Some(Box::new(done))));
    let pending = once.clone();
    window.connect_close_request(move |_| {
        let done = pending.borrow_mut().take();
        if let Some(done) = done {
            done(false);
        }
        glib::Propagation::Proceed
    });
    let w = window.clone();
    accept.connect_clicked(move |_| {
        if !checked.is_active() {
            return;
        }
        let done = once.borrow_mut().take();
        w.close();
        if let Some(done) = done {
            done(true);
        }
    });
    window.present();
    cancel.grab_focus();
}

/// Privat funksjonsprøve mot en eksplisitt merket fake-arbeider. Bruker de
/// vanlige GTK-signalene og RPC-kallene, aldri render-fixturen nedenfor.
pub(crate) fn debug_flow(step: &str) {
    CURRENT.with(|current| {
        let p = current
            .borrow()
            .upgrade()
            .expect("RGB-panelet må være åpnet");
        assert!(!p.fixture.get(), "RGB-flow må ikke bruke visningsfixture");
        let helper = p.helper.borrow().clone();
        flow_environment(&p.app, &helper).expect("RGB-flow isolasjon");
        let state = &helper["status"]["state"];
        let click = |button: &gtk::Button| {
            assert!(
                button.is_sensitive() && button.is_visible(),
                "RGB-flow knapp er ikke tilgjengelig"
            );
            button.emit_clicked();
        };
        eprintln!("RIGG_RGB_FLOW {step}");
        match step {
            "ready" => {
                assert_eq!(state["dirty"], false);
                assert!(state["pending"].is_null());
                assert_ne!(state["confirmed"]["mode"], "off");
                FLOW_INITIAL.with(|v| *v.borrow_mut() = state.clone());
            }
            "edit" => {
                p.color.set_text("#32B487");
                p.brightness.set_value(65.0);
            }
            "unchanged" => {
                FLOW_INITIAL
                    .with(|v| assert_eq!(state, &*v.borrow(), "Redigering endret RGB-status"));
            }
            "apply" => click(&p.apply),
            "pending" => {
                assert_eq!(helper["owned"], true);
                assert_eq!(
                    state["pending"]["requested"],
                    json!({"mode":"static","color":[50,180,135],"brightness":65})
                );
                assert!(p.keep.is_sensitive() && p.revert.is_sensitive());
            }
            "keep" => click(&p.keep),
            // «Behold også etter at appen lukkes» er standard: valget er
            // lagret som varig, økten er ren og appen eier ikke lyset lenger.
            "kept" => {
                let kept = json!({"mode":"static","color":[50,180,135],"brightness":65});
                assert!(state["pending"].is_null());
                assert_eq!(state["confirmed"], kept);
                assert_eq!(helper["status"]["baselineRequest"], kept);
                assert_eq!(state["dirty"], false);
                assert_eq!(helper["owned"], false);
                assert_eq!(helper["status"]["keepalive"], false, "bare «Av» holdes");
            }
            "effect_from_kept" => {
                let button = p.mode.button("breathing").expect("Pusting mangler");
                assert!(button.is_visible() && button.is_sensitive());
                button.set_active(true);
                assert!(
                    p.apply.is_sensitive(),
                    "Bruk skal være aktiv for å starte effekt fra beholdt fast farge"
                );
            }
            "effect_breathing" | "effect_rainbow" | "effect_cycle" => {
                let effect = step.strip_prefix("effect_").unwrap();
                assert!(supports_effect(&helper["status"], effect));
                let button = p.mode.button(effect).expect("Effektknapp mangler");
                assert!(button.is_visible() && button.is_sensitive());
                button.set_active(true);
                p.color.set_text("#32B487");
                p.brightness.set_value(65.0);
            }
            "effect_edit_unchanged" => {
                assert!(p.apply.is_sensitive());
            }
            "effect_pending" => {
                assert_eq!(
                    helper["status"]["effect"].as_str(),
                    p.mode.value().as_deref()
                );
                assert!(state["pending"].is_object());
                assert_eq!(
                    state["observed"]["color"],
                    state["pending"]["requested"]["color"]
                );
                assert_eq!(state["observed"]["brightnessRaw"], 65);
                assert!(state["observed"]["mode"].is_null());
                assert!(p.keep.is_sensitive());
                assert!(!p.apply.is_sensitive());
                assert!(!p.apply.is_visible() && !p.apply_hint.is_visible(), "Under prøven står bare Behold og Rull tilbake");
                assert!(p.effect_info.is_visible() && !p.effect_info.text().is_empty());
                FLOW_EFFECT_START.with(|v| *v.borrow_mut() = state["observed"].clone());
            }
            "effect_not_started" => {
                assert!(state["pending"].is_object());
                FLOW_EFFECT_START.with(|v| {
                    assert_eq!(state["observed"], *v.borrow(), "Effekt animerte før Behold")
                });
            }
            "effect_active" => {
                assert_eq!(
                    helper["status"]["effect"].as_str(),
                    p.mode.value().as_deref()
                );
                assert!(state["pending"].is_null());
                assert_eq!(helper["owned"], true);
                assert!(state["observed"]["mode"].is_null());
                assert_eq!(state["observed"]["brightnessRaw"], 65);
                assert!(p.effect_info.is_visible() && !p.effect_info.text().is_empty());
                assert_eq!(p.release.label().as_deref(), Some(tr!("Stopp effekten")));
                assert!(p.apply.is_sensitive(), "Bruk skal være aktiv under kjørende effekt slik at brukeren kan bytte modus");
                assert!(!p.reset.is_visible());
                FLOW_EFFECT_START.with(|v| {
                    assert_ne!(
                        state["observed"],
                        *v.borrow(),
                        "Aktiv effekt ga ingen nye rammer"
                    );
                    assert_eq!(state["confirmed"]["color"], v.borrow()["color"]);
                });
            }
            "reset" => click(&p.reset),
            "recovery" => {
                assert_eq!(state["recoveryNeedsConfirmation"], true);
                assert!(!p.apply.is_sensitive());
                // Én handlingsboks med neste steg i stedet for fire knapper.
                assert!(p.action_box.is_visible() && p.ack.is_visible() && p.reset.is_visible());
                assert!(!p.apply.is_visible() && !p.apply_hint.is_visible());
                assert!(state["observed"]["mode"].is_null());
                FLOW_INITIAL.with(|v| assert_eq!(state["confirmed"], v.borrow()["confirmed"]));
            }
            // Prøver videresending av et gammelt/feil forventet mål. Dette
            // velger aldri et skrivemål og er bare tilgjengelig i fake-flow.
            "stale_ack" => {
                assert!(p.operation(
                    "rgb_ack_recovery",
                    json!({"confirmedStatic":true,"expected":{"mode":"off"},
                        "recoveryId":helper["status"]["recoveryId"]}),
                ));
            }
            "stale_rejected" => {
                assert!(
                    p.message.borrow().is_some(),
                    "Utdatert expected ble ikke avvist"
                );
                assert_eq!(state["dirty"], true);
                assert_eq!(state["recoveryNeedsConfirmation"], true);
                assert!(state["recoveryConfirmationSource"].is_null());
            }
            "ack" => {
                click(&p.ack);
                let dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Window>().ok())
                    .find(|w| {
                        w.is_visible() && w.title().as_deref() == Some(tr!("Bekreft lyset på kortet"))
                    })
                    .expect("Den vanlige RGB-bekreftelsesdialogen mangler");
                fn find<T: IsA<gtk::Widget> + glib::object::ObjectType>(
                    widget: &gtk::Widget,
                ) -> Option<T> {
                    if let Ok(found) = widget.clone().downcast::<T>() {
                        return Some(found);
                    }
                    let mut child = widget.first_child();
                    while let Some(w) = child {
                        if let Some(found) = find::<T>(&w) {
                            return Some(found);
                        }
                        child = w.next_sibling();
                    }
                    None
                }
                let check = find::<gtk::CheckButton>(dialog.upcast_ref())
                    .expect("Visuell avkrysning mangler");
                let accept = crate::skjermbilder::find_button(
                    dialog.upcast_ref(),
                    tr!("Bekreft min observasjon"),
                )
                .expect("Visuell bekreftelsesknapp mangler");
                assert!(!check.is_active() && !accept.is_sensitive());
                check.set_active(true);
                assert!(accept.is_sensitive());
                accept.emit_clicked();
            }
            "acknowledged" => {
                assert_eq!(state["dirty"], false);
                assert_eq!(state["recoveryConfirmationSource"], "userVisual");
                assert!(state["observed"]["mode"].is_null());
                assert_eq!(helper["owned"], false);
                assert!(looks_off(&state["confirmed"]), "tilbake på det varige «Av»");
            }
            // «Av» er en egen bryter; standardmåten er svart med 0 %, holdt.
            "off" => {
                assert_eq!(p.app.settings().rgb_av, "hold");
                assert!(p.varig.is_active(), "«Behold også etter at appen lukkes» er standard");
                p.power.set_active(false);
            }
            // «Hold» starter allerede under prøven (før «Behold»), uten effekt.
            "off_pending" => {
                assert!(looks_off(&state["pending"]["requested"]), "{}", state["pending"]["requested"]);
                assert_eq!(helper["status"]["keepalive"], true, "«hold» må gå mens fristen løper");
                assert!(helper["status"]["effect"].is_null());
                assert_eq!(helper["owned"], true);
            }
            // Etter «Behold»: lagret som varig, ren økt, «hold» fortsetter.
            "off_kept" => {
                assert!(state["pending"].is_null());
                assert_eq!(state["dirty"], false, "varig «Av» gir ingen uavklart økt");
                assert!(looks_off(&helper["status"]["baselineRequest"]));
                assert_eq!(state["confirmed"], helper["status"]["baselineRequest"]);
                assert_eq!(helper["status"]["keepalive"], true);
                assert_eq!(helper["owned"], true);
                assert!(!p.power.is_active());
                assert_eq!(p.release.label().as_deref(), Some(tr!("Slutt å holde av")));
                FLOW_EFFECT_START.with(|v| *v.borrow_mut() = json!({
                    "writes": helper["status"]["holdWrites"], "outcome": helper["status"]["lastOutcome"],
                }));
            }
            // Minst 10 s senere: fortsatt «hold», ingen ny prøve, flere skrivinger.
            "off_held" => {
                assert!(state["pending"].is_null(), "ingen ny prøve");
                assert!(!p.pending.is_visible());
                assert_eq!(helper["status"]["keepalive"], true);
                assert_eq!(state["dirty"], false);
                FLOW_EFFECT_START.with(|v| {
                    let v = v.borrow();
                    assert_eq!(helper["status"]["lastOutcome"], v["outcome"], "ingen ny transaksjon");
                    let before = v["writes"].as_u64().unwrap_or(0);
                    let now = helper["status"]["holdWrites"].as_u64().unwrap_or(0);
                    assert!(now >= before + 4, "«hold» skrev bare {} ganger på ≥ 10 s", now - before);
                });
            }
            // Som en omstart av appen: tjenesten mister forbindelsen og «hold» stopper.
            "restart" => p.app.client.debug_reconnect(),
            "off_restarted" => {
                assert!(p.app.client.is_connected());
                assert_eq!(helper["status"]["keepalive"], true, "«hold» startet ikke igjen etter omstart");
                assert_eq!(helper["owned"], true);
                assert_eq!(state["dirty"], false);
                assert!(state["pending"].is_null());
            }
            "release" => click(&p.release),
            "hold_stopped" => {
                assert_eq!(helper["status"]["keepalive"], false);
                assert_eq!(helper["owned"], false);
                assert_eq!(state["dirty"], false);
                assert!(looks_off(&state["confirmed"]));
            }
            // Ny farge, så «Rull tilbake»: utgangspunktet («Av») må bekreftes.
            "on" => {
                p.power.set_active(true);
                p.mode.set("static");
            }
            "on_pending" => {
                assert_eq!(state["pending"]["requested"], json!({"mode":"static","color":[50,180,135],"brightness":65}));
                assert_eq!(helper["status"]["keepalive"], false);
                assert!(!p.release.is_visible(), "under prøven står bare Behold og Rull tilbake");
            }
            "revert" => click(&p.revert),
            "reverted" => {
                assert!(helper["status"]["effect"].is_null());
                assert!(state["pending"].is_null());
                assert_eq!(state["recoveryNeedsConfirmation"], true);
                assert!(looks_off(&state["confirmed"]));
                assert!(p.action_box.is_visible() && p.ack.is_visible());
            }
            _ => panic!("Ukjent RGB-flow-steg: {step}"),
        }
    });
}

/// Kun isolerte skjermbildeplaner: setter visningsdata, aldri daemon/HW.
pub(crate) fn debug_fixture(kind: &str) {
    CURRENT.with(|current| {
        if let Some(p) = current.borrow().upgrade()
            && p.app.profile.is_dev()
            && p.app.data.borrow().backend == "sim"
            && std::env::var_os("RIGG_SKJERMBILDER").is_some()
        {
            p.hide();
            p.fixture.set(true);
            let Some((pci, uuid)) = selected_identity(&p.app) else { return };
            let Some(bus) = pci.get(5..7).and_then(|v| u8::from_str_radix(v, 16).ok()) else { return };
            let Some(slot) = pci.get(8..) else { return };
            let adapter_name = format!("NVIDIA i2c adapter 1 at {bus:x}:{slot}");
            let mut value = json!({
                "state":"on", "owned":false, "busy":false, "message":null,
                "status":{
                    "helper":{"name":rgb_auth::HELPER_NAME,"domain":rgb_auth::DOMAIN,"protocol":rgb_auth::PROTOCOL,"strictRequests":true,"build":rigg_core::version::build()},
                    "authorized":true,"observationSource":"operation_readback",
                    "softwareEffects":["breathing","rainbow","cycle"],
                    "persist":true, "hold":true, "keepalive":false, "drift":false,
                    "baselineRequest":{"mode":"static","color":[58,123,240],"brightness":80},
                    "factory":{"mode":"static","color":[69,200,0],"brightness":100},
                    "identity":{"pci":pci,"uuid":uuid,"pciIds":{"vendor":0x10de,"device":0x1e87,"subsystemVendor":0x10de,"subsystemDevice":0x1e87},"adapter":{"port":1,"name":adapter_name,"parentPci":pci,"driver":"nvidia"},"address":73,"signature":"palit_100"},
                    "state":{"baselineSource":"user_confirmed_static","confirmed":{"mode":"static","color":[58,123,240],"brightness":80},"observed":{"color":[58,123,240],"brightnessRaw":80,"mode":null},"pending":null,"dirty":false,"recovery":null,"recoveryNeedsConfirmation":false,"recoveryConfirmationSource":null,"error":null}
                }
            });
            FIXTURE_STALE.with(|s| s.set(kind == "stale_unsettled"));
            // Kontrollene som brukeren ville sett dem i tilstanden.
            p.syncing.set(true);
            p.power.set_active(kind != "lys_av");
            p.varig.set_active(true);
            p.mode.set(if kind == "effekt" { "breathing" } else { "static" });
            p.syncing.set(false);
            match kind {
                // «Av» er lagret som varig utgangspunkt og holdes mens appen er
                // åpen: ren økt, ingen journal, lease for «hold».
                "lys_av" => {
                    let off = json!({"mode":"static","color":[0,0,0],"brightness":0});
                    value["owned"] = json!(true);
                    value["status"]["keepalive"] = json!(true);
                    value["status"]["baselineRequest"] = off.clone();
                    value["status"]["holding"] = json!({"owner":12,"deadlineMs":boottime_ms().unwrap_or(0)+15_000,"leaseMs":15_000});
                    value["status"]["state"]["confirmed"] = off;
                    value["status"]["state"]["observed"] = json!({"color":[0,0,0],"brightnessRaw":0,"mode":null});
                }
                // Ren økt, men kortet har selv gått tilbake til fabrikklyset.
                "avvik" => {
                    value["status"]["drift"] = json!(true);
                    value["status"]["state"]["observed"] = json!({"color":[69,200,0],"brightnessRaw":100,"mode":null});
                }
                // Bundet, men gjenopprettingen er uavklart (feil i tilbakelesing).
                "blocked" => {
                    value["status"]["state"]["dirty"] = json!(true);
                    value["status"]["state"]["recovery"] = json!("unknown");
                    value["status"]["state"]["error"] = json!("Gjenoppretting samsvarer ikke med fersk tilbakelesing.");
                }
                "effekt" => {
                    value["owned"] = json!(true);
                    value["status"]["effect"] = json!("breathing");
                    value["status"]["holding"] = json!({"owner":12,"deadlineMs":boottime_ms().unwrap_or(0)+15_000,"leaseMs":15_000});
                    value["status"]["state"]["owner"] = json!(12);
                    value["status"]["state"]["dirty"] = json!(true);
                    value["status"]["state"]["mayHaveWritten"] = json!(true);
                }
                "off" => value = json!({"state":"off","owned":false,"busy":false,"message":null,"status":null}),
                // GPU-oppdateringen lot RGB-delen stå på bygg 31 med en uavklart økt.
                "first" | "journal" => {
                    value["status"]["review"] = json!({"kind":kind,"id":"simulated-attempt","expected":value["status"]["factory"]});
                    value["status"]["state"]["observed"] = json!({"color":[69,200,0],"brightnessRaw":100,"mode":null});
                    value["status"]["baselineRequest"] = if kind == "first" { Value::Null } else { json!({"mode":"static","color":[0,0,0],"brightness":0}) };
                }
                "stale_unsettled" => {
                    // Bygg 31 kjenner verken varig valg eller «hold».
                    value["status"]["helper"]["build"] = json!(31);
                    for key in ["persist", "hold", "keepalive", "drift", "baselineRequest", "factory"] {
                        value["status"][key] = Value::Null;
                    }
                    value["status"]["state"]["dirty"] = json!(true);
                    value["status"]["state"]["recovery"] = json!("readable_fields_match");
                    value["status"]["state"]["recoveryNeedsConfirmation"] = json!(true);
                    value["status"]["recoveryId"] = json!("rgb-visningsfixture-1");
                }
                "stored" => value["status"]["observationSource"] = json!("stored_baseline"),
                "pending" | "pending_effects" => {
                    if kind == "pending_effects" {
                        value["status"]["softwareEffects"] = json!(["breathing","rainbow","cycle"]);
                    }
                    value["owned"] = json!(true);
                    value["status"]["state"]["dirty"] = json!(true);
                    value["status"]["state"]["pending"] = json!({"txn":7,"owner":12,"deadlineMs":boottime_ms().unwrap_or(0)+15_000,"requested":{"mode":"static","color":[58,123,240],"brightness":80}});
                }
                "recovery" | "confirmed" => {
                    value["status"]["state"]["dirty"] = json!(kind == "recovery");
                    value["status"]["state"]["recovery"] = json!("readable_fields_match");
                    value["status"]["state"]["recoveryNeedsConfirmation"] = json!(kind == "recovery");
                    if kind == "confirmed" { value["status"]["state"]["recoveryConfirmationSource"] = json!("userVisual"); }
                    else { value["status"]["recoveryId"] = json!("rgb-visningsfixture-1"); }
                }
                "error" => value = json!({"state":"error","owned":false,"busy":false,"message":"RecoveryRequired: En tidligere RGB-økt må avklares. Ingen automatisk endring er utført.","status":null}),
                "effect_error" => {
                    value["status"]["softwareEffects"] = json!(["breathing","rainbow","cycle"]);
                    value["status"]["effect"] = Value::Null;
                    value["status"]["runtimeError"] = json!("Den simulerte effekten stoppet etter en I/O-feil. Kontroller gjenopprettingen.");
                    value["status"]["state"]["dirty"] = json!(true);
                    value["status"]["state"]["recovery"] = json!("readable_fields_match");
                    value["status"]["state"]["recoveryNeedsConfirmation"] = json!(true);
                    value["status"]["recoveryId"] = json!("rgb-visningsfixture-1");
                }
                _ => {}
            }
            *p.helper.borrow_mut() = value;
            p.render();
            if kind == "effect_error" {
                assert!(p.runtime_error.is_visible() && p.ack.is_visible());
                assert!(p.helper.borrow()["status"]["state"]["error"].is_null());
            } else if kind == "off" {
                // Uten aktiv hjelper skal valgene og «Bruk» likevel stå fremme:
                // første «Bruk» aktiverer kortlyset (1.5.5).
                assert!(p.settings.is_visible() && !p.apply.is_sensitive());
                for (id, _) in EFFECTS {
                    assert!(p.mode.button(id).unwrap().is_visible(), "Effekter er skjult før aktivering");
                }
            } else if kind == "pending_effects" {
                for (id, _) in EFFECTS {
                    assert!(p.mode.button(id).unwrap().is_visible(), "Støttet effekt er skjult");
                }
            } else if usable(&p.app, &p.helper.borrow(), selected_identity(&p.app).as_ref()) {
                let status = p.helper.borrow()["status"].clone();
                for (id, _) in EFFECTS {
                    assert_eq!(p.mode.button(id).unwrap().is_visible(), supports_effect(&status, id), "Effekten {id} følger ikke hjelperens støtte");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av_skrives_slik_innstillingen_sier_og_bare_med_faste_registre() {
        let base = [69, 200, 0];
        let level = |v| Brightness::try_from(v).unwrap();
        assert_eq!(off_request("svart-100", base), (Request::Off {}, false));
        assert_eq!(off_request("svart-0", base), (Request::Static { color: [0, 0, 0], brightness: level(0) }, false));
        assert_eq!(off_request("farge-0", base), (Request::Static { color: base, brightness: level(0) }, false));
        assert_eq!(off_request("nesten-svart", base), (Request::Static { color: [1, 1, 1], brightness: level(1) }, false));
        // Standard («hold»): svart med 0 %, holdt mens appen er åpen.
        assert_eq!(off_request("hold", base), (Request::Static { color: [0, 0, 0], brightness: level(0) }, true));
        assert!(off_request("ukjent", base).1);
        for method in rigg_core::settings::RGB_AV {
            let (request, _) = off_request(method, base);
            assert!(looks_off(&serde_json::to_value(request).unwrap()), "{method}");
        }
    }

    #[test]
    fn svart_eller_null_lysstyrke_vises_som_av() {
        for off in [json!({"mode":"off"}), json!({"mode":"static","color":[0,0,0],"brightness":100}),
                    json!({"mode":"static","color":[58,123,240],"brightness":0})] {
            assert!(looks_off(&off), "{off}");
            assert_eq!(request_text(&off), "Av");
        }
        let on = json!({"mode":"static","color":[58,123,240],"brightness":80});
        assert!(!looks_off(&on));
        assert!(request_text(&on).contains("#3A7BF0"));
    }

    #[test]
    fn hjelper_uten_varig_valg_og_hold_maa_oppdateres() {
        // Bygg 31 (1.6.0) og main før kontrollen (bare «persist»).
        let old = json!({"state": "on", "status": {"helper": {"build": 31}}});
        assert!(lacks_current_features(&old));
        let partial = json!({"state": "on", "status": {"persist": true, "softwareEffects": ["hold"]}});
        assert!(lacks_current_features(&partial));
        let current = json!({"state": "on", "status": {"persist": true, "hold": true}});
        assert!(!lacks_current_features(&current));
        // Bare en kjørende hjelper vurderes; «av» og «outdated» har egne veier.
        assert!(!lacks_current_features(&json!({"state": "off", "status": null})));
    }

    #[test]
    fn hold_ved_oppstart_bare_for_lagret_av_i_ren_okt() {
        let off = json!({"mode": "static", "color": [0, 0, 0], "brightness": 0});
        let ready = json!({
            "hold": true, "keepalive": false, "holding": null, "effect": null, "drift": false,
            "baselineRequest": off,
            "state": {"dirty": false, "pending": null, "confirmed": off, "recovery": null, "error": null},
        });
        assert!(hold_status_ready(&ready));
        let mut colour = ready.clone();
        colour["baselineRequest"] = json!({"mode": "static", "color": [58, 123, 240], "brightness": 80});
        colour["state"]["confirmed"] = colour["baselineRequest"].clone();
        assert!(!hold_status_ready(&colour), "bare «Av» holdes");
        for (key, value) in [("keepalive", json!(true)), ("drift", json!(true)), ("hold", Value::Null)] {
            let mut status = ready.clone();
            status[key] = value;
            assert!(!hold_status_ready(&status), "{key}");
        }
        let mut dirty = ready.clone();
        dirty["state"]["dirty"] = json!(true);
        assert!(!hold_status_ready(&dirty), "aldri over en uavklart økt");
        let mut held = ready;
        held["holding"] = json!({"owner": 3});
        assert!(!hold_status_ready(&held), "noen andre holder lyset");
    }

    #[test]
    fn tekst_for_lys_inne_i_setninger_og_avvik() {
        let blue = json!({"mode": "static", "color": [58, 123, 240], "brightness": 80});
        assert!(request_text(&blue).starts_with("fast farge"));
        assert!(request_heading(&blue).starts_with("Fast farge"));
        assert_eq!(observed_text(&json!({"color": [69, 200, 0], "brightnessRaw": 100})), request_text(&json!({"mode": "static", "color": [69, 200, 0], "brightness": 100})));
        assert!(observed_text(&json!({"color": [1, 2, 3], "brightnessRaw": 250})).contains("250"));
        assert!(!observed_text(&json!({"color": null})).is_empty());
    }

    #[test]
    fn kortfarge_er_streng_og_valgene_er_semantiske() {
        assert_eq!(hex_color("#00aAFF"), Some([0, 170, 255]));
        for bad in ["", "#abc", "#gg0000", "#1234567", "１２３４５６", "#12 456"] {
            assert_eq!(hex_color(bad), None);
        }
        assert_eq!(request_text(&json!({"mode": "off"})), "Av");
        assert_eq!(
            request_text(&json!({"mode": "breathe"})),
            "Ingen kjent innstilling"
        );
    }

    #[test]
    fn lagret_feil_ber_aldri_om_brukerbekreftelse() {
        let confirm = recovery_text(&json!({"recovery": "readable_fields_match"}));
        assert!(confirm.contains("før du bekrefter"));
        // Hjelperen avviser bekreftelsen mens `error` står.
        let failed = recovery_text(&json!({
            "recovery": "readable_fields_match",
            "error": "Brukerbekreftelsen kunne ikke lagres: I/O",
        }));
        assert_eq!(failed, "Gjenopprettingen er uavklart. Nye fargeendringer er sperret.");
        assert_eq!(recovery_text(&json!({"error": "Gjenoppretting feilet: I/O"})), failed);
        assert_eq!(recovery_text(&json!({"recovery": "unknown", "error": "I/O"})), failed);
        assert!(recovery_text(&json!({"recovery": "mismatch", "error": "I/O"})).contains("samsvarer ikke"));
        assert!(recovery_text(&json!({"recovery": "readable_fields_match", "readError": "I/O"})).contains("Avlesning"));
        assert!(recovery_text(&json!({"recovery": "readable_fields_match", "recoveryConfirmationSource": "userVisual"})).contains("bekreftet av deg"));
        assert_eq!(recovery_text(&json!({"recovery": "verified"})), "");
        assert_eq!(recovery_text(&Value::Null), "");
    }

    #[test]
    fn ukjent_diagnose_vises_uendret_etter_forklaringen() {
        assert_eq!(with_diagnostic("Forklaring.", None), "Forklaring.");
        assert_eq!(with_diagnostic("Forklaring.", Some("  \n")), "Forklaring.");
        assert_eq!(
            with_diagnostic("Forklaring.", Some("RGB: Uavklart RGB-journal for 0000:1c:00.0 GPU-x. Velg «Gjenopprett lagret lys».\n")),
            "Forklaring.\nRGB: Uavklart RGB-journal for 0000:1c:00.0 GPU-x. Velg «Gjenopprett lagret lys»."
        );
        let known = operation_error(&ProtoError::new(ErrorCode::Busy));
        assert_eq!(known, "Lysstyringen er opptatt eller tilhører et annet vindu.");
        let unknown = operation_error(&ProtoError::msg(ErrorCode::Unavailable, "RGB-økta er avsluttet."));
        assert!(unknown.starts_with("RGB-forespørselen mislyktes."));
        assert!(unknown.ends_with("\nRGB-økta er avsluttet."));
    }

    #[test]
    fn effekter_krever_eksplisitt_kapasitet_og_ublokkert_tilstand() {
        let mut status = json!({
            "softwareEffects":["breathing","rainbow","cycle","unknown"],
            "effect":null,
            "state":{"dirty":false,"mayHaveWritten":false,"owner":null,"pending":null,"error":null,"readError":null,"recovery":null}
        });
        for (id, _) in EFFECTS {
            assert!(supports_effect(&status, id));
            assert!(!supports_effect(&json!({}), id));
            assert!(can_apply(&status, Some(id)));
        }
        assert!(!supports_effect(&status, "unknown"));
        assert!(!can_apply(&status, Some("unknown")));
        // Beholdte valg (dirty/mayHaveWritten/owner) og aktiv effekt tillater bytte.
        for (key, value) in [
            ("dirty", json!(true)),
            ("mayHaveWritten", json!(true)),
            ("owner", json!(7)),
        ] {
            let mut changed = status.clone();
            changed["state"][key] = value;
            assert!(can_apply(&changed, Some("breathing")));
            assert!(can_apply(&changed, None));
        }
        let mut active_effect = status.clone();
        active_effect["effect"] = json!("rainbow");
        assert!(can_apply(&active_effect, Some("breathing")));
        assert!(can_apply(&active_effect, None));

        // Ventende prøve, feil og uavklart gjenoppretting sperrer fortsatt Bruk.
        for (key, value) in [
            ("pending", json!({"txn":1})),
            ("error", json!("I/O")),
            ("readError", json!("I/O")),
            ("recovery", json!("unknown")),
            ("recovery", json!("mismatch")),
            ("recovery", json!("readable_fields_match")),
        ] {
            let mut changed = status.clone();
            changed["state"][key] = value.clone();
            assert!(!can_apply(&changed, Some("breathing")), "{key}={value}");
            assert!(!can_apply(&changed, None), "{key}={value}");
        }
        // Visuelt bekreftet gjenoppretting åpner Bruk igjen.
        status["state"]["recovery"] = json!("readable_fields_match");
        status["state"]["recoveryConfirmationSource"] = json!("userVisual");
        assert!(can_apply(&status, Some("breathing")));
        assert!(can_apply(&status, None));
    }

    #[test]
    fn nvml_tillatelse_eller_feil_kort_gir_aldri_rgb_klar() {
        let selected = ("0000:1c:00.0".into(), "GPU-fixture".into());
        assert!(!bound_status(
            &json!({"state": "on", "enabled": true, "backend": "nvml"}),
            Some(&selected)
        ));
        assert!(!bound_status(
            &json!({"state": "on", "status": {"authorized": true}}),
            Some(&selected)
        ));
        assert!(!bound_status(&Value::Null, None));
    }

    #[test]
    fn rgb_status_krever_rett_kort_domene_og_kompatibelt_bygg() {
        let selected = ("0000:1c:00.0".into(), "GPU-fixture".into());
        let value = json!({
            "state":"on", "status":{
                "authorized":true,
                "helper":{"name":rgb_auth::HELPER_NAME,"domain":rgb_auth::DOMAIN,"protocol":rgb_auth::PROTOCOL,"strictRequests":true,"build":rigg_core::version::build()},
                "identity":{"pci":selected.0,"uuid":selected.1,"pciIds":{"vendor":0x10de,"device":0x1e87,"subsystemVendor":0x10de,"subsystemDevice":0x1e87},"adapter":{"port":1,"name":"NVIDIA i2c adapter 1 at 1c:00.0","parentPci":selected.0,"driver":"nvidia"},"address":73,"signature":"palit_100"}
            }
        });
        assert!(bound_status(&value, Some(&selected)));
        let wrong_gpu = (selected.0.clone(), "GPU-another".into());
        assert!(!bound_status(&value, Some(&wrong_gpu)));
        for (field, replacement) in [
            ("domain", json!("nvml")),
            ("protocol", json!(rgb_auth::PROTOCOL + 1)),
            ("strictRequests", Value::Null),
            ("build", json!(rigg_core::version::build() + 1)),
        ] {
            let mut changed = value.clone();
            changed["status"]["helper"][field] = replacement;
            assert!(!bound_status(&changed, Some(&selected)));
        }
        let mut wrong_port = value;
        wrong_port["status"]["identity"]["adapter"]["port"] = json!(0);
        assert!(!bound_status(&wrong_port, Some(&selected)));
    }

    #[test]
    fn sjekk_igjen_leser_ren_tilstand_bare_nar_arbeideren_er_ledig() {
        let selected = ("0000:1c:00.0".into(), "GPU-fixture".into());
        let mut value = json!({
            "state": "on",
            "busy": false,
            "status": {
                "authorized": true,
                "helper": {
                    "name": rgb_auth::HELPER_NAME,
                    "domain": rgb_auth::DOMAIN,
                    "protocol": rgb_auth::PROTOCOL,
                    "strictRequests": true,
                    "build": rigg_core::version::build()
                },
                "identity": {
                    "pci": selected.0,
                    "uuid": selected.1,
                    "pciIds": {
                        "vendor": 0x10de,
                        "device": 0x1e87,
                        "subsystemVendor": 0x10de,
                        "subsystemDevice": 0x1e87
                    },
                    "adapter": {
                        "port": 1,
                        "name": "NVIDIA i2c adapter 1 at 1c:00.0",
                        "parentPci": selected.0,
                        "driver": "nvidia"
                    },
                    "address": 73,
                    "signature": "palit_100"
                },
                "state": {"dirty": false}
            }
        });
        assert!(should_read_clean(&value, Some(&selected)));
        value["busy"] = json!(true);
        assert!(!should_read_clean(&value, Some(&selected)));
        value["busy"] = json!(false);
        value["status"]["state"]["dirty"] = json!(true);
        assert!(!should_read_clean(&value, Some(&selected)));
    }
}
