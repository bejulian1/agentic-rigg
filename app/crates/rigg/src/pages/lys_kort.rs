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
    mode: Seg,
    color: gtk::Entry,
    swatch: gtk::DrawingArea,
    swatch_color: Rc<Cell<Option<[u8; 3]>>>,
    swatches_box: gtk::Box,
    controls: gtk::Grid,
    brightness_box: gtk::Box,
    details: gtk::Expander,
    /// «Bruk» startet aktiveringen; valget brukes når hjelperen er klar.
    queued: Cell<bool>,
    last_state: RefCell<String>,
    support: gtk::Label,
    readback_note: gtk::Label,
    brightness: gtk::SpinButton,
    apply: gtk::Button,
    apply_hint: gtk::Label,
    install_prompt: gtk::Expander,
    activate: gtk::Button,
    deactivate: gtk::Button,
    resolve: gtk::Button,
    repair: gtk::Button,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdminAction {
    Activate,
    Deactivate,
    Inspect,
    Resolve,
    Restore,
}

impl AdminAction {
    fn script(self) -> &'static str {
        match self {
            Self::Activate => "rgb-aktiver.sh",
            Self::Deactivate => "rgb-deaktiver.sh",
            Self::Inspect | Self::Resolve => "rgb-avklar.sh",
            Self::Restore => "rgb-gjenopprett.sh",
        }
    }
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
    state["pending"].is_null()
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

fn request_text(value: &Value) -> String {
    match serde_json::from_value::<Request>(value.clone()) {
        Ok(Request::Off {}) => tr!("Av").into(),
        Ok(Request::Static { color, brightness }) => trf!("Fast farge {farge} · {styrke}", farge = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]), styrke = fmt::pct(Some(f64::from(brightness.get())), 0)),
        Err(_) => tr!("Ingen kjent innstilling").into(),
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

/// RGB-delen er eldre enn GPU-delen (GPU-oppdateringen lot den stå, kanskje
/// med en uavklart økt). Den kjørende RGB-hjelperen regnes da som «må
/// oppdateres»: «Bruk» oppdaterer og avklarer med ett passord.
fn rgb_stale(app: &App, helper: &Value) -> bool {
    if FIXTURE_STALE.with(Cell::get) {
        return true;
    }
    !app.profile.is_dev() && helper["systemAdmin"] == true && crate::system_install::rgb_older_than_system()
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
        let mode = Seg::new(
            &[
                ("static", tr!("Fast farge")),
                ("off", tr!("Av")),
                ("breathing", tr!("Pusting")),
                ("rainbow", tr!("Regnbue")),
                ("cycle", tr!("Fargesyklus")),
            ],
            "static",
            tr!("Fysisk lysmodus"),
        )
        .small();
        mode.root.set_halign(gtk::Align::Start);
        controls.attach(&heading(tr!("Modus")), 0, 0, 1, 1);
        controls.attach(&mode.root, 1, 0, 1, 1);

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
        let color_box = hbox(6, &[]);
        color_box.append(&swatch);
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
            let entry = color.clone();
            let mode_ref = mode.clone();
            b.connect_clicked(move |_| {
                if mode_ref.value().as_deref() == Some("off") {
                    mode_ref.set("static");
                }
                entry.set_text(hex);
            });
            swatches_box.append(&b);
        }
        let color_row = flow();
        color_row.insert(&color_box, -1);
        color_row.insert(&swatches_box, -1);
        controls.attach(&heading(tr!("Farge")), 0, 1, 1, 1);
        controls.attach(&color_row, 1, 1, 1, 1);

        let brightness = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
        brightness.set_value(80.0);
        brightness.set_digits(0);
        brightness.set_width_chars(3);
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
        controls.attach(&heading(tr!("Lysstyrke")), 0, 2, 1, 1);
        controls.attach(&brightness_box, 1, 2, 1, 1);
        panel.body.append(&controls);

        let effect_info = paragraph("", &["sm", "muted"]);
        panel.body.append(&effect_info);
        let runtime_error = paragraph("", &["sm", "warn"]);
        panel.body.append(&runtime_error);
        let recovery = paragraph("", &["sm"]);
        panel.body.append(&recovery);

        let actions = flow();
        let apply = button(Some("check"), Some(tr!("Bruk")), &["suggested"]);
        let reset = button(
            Some("rotate-ccw"),
            Some(tr!("Gjenopprett utgangspunkt")),
            &["sm", "flat"],
        );
        let release = button(None, Some(tr!("Avslutt lysstyring")), &["sm"]);
        let ack = button(Some("check"), Some(tr!("Bekreft gjenoppretting …")), &["sm"]);
        for b in [&apply, &release, &reset, &ack] {
            actions.insert(b, -1);
        }
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
        let install_prompt = crate::system_install::offline_fallback();
        details_box.append(&install_prompt);
        let details_actions = flow();
        let refresh = button(Some("refresh-cw"), Some(tr!("Sjekk igjen")), &["sm", "flat"]);
        let activate = button(Some("shield"), Some(tr!("Aktiver kortlys …")), &["sm", "flat"]);
        let repair = button(None, Some(tr!("Gjenopprett lagret lys …")), &["sm", "flat"]);
        let resolve = button(None, Some(tr!("Avklar tidligere økt …")), &["sm", "flat"]);
        let deactivate = button(None, Some(tr!("Slå av RGB-tilgang …")), &["sm", "flat"]);
        for b in [&refresh, &activate, &repair, &resolve, &deactivate] {
            details_actions.insert(b, -1);
        }
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
            mode,
            color,
            swatch,
            swatch_color,
            swatches_box,
            controls,
            brightness_box,
            details,
            queued: Cell::new(false),
            last_state: RefCell::new(String::new()),
            support,
            readback_note,
            brightness,
            apply,
            apply_hint,
            activate,
            install_prompt,
            deactivate,
            resolve,
            repair,
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

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.mode.connect(move |_| {
            if let Some(p) = weak.upgrade() {
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.color.connect_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.render();
            }
        });
        let weak = Rc::downgrade(self);
        self.brightness.connect_value_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.render();
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
        for (button, action) in [
            (&self.activate, AdminAction::Activate),
            (&self.deactivate, AdminAction::Deactivate),
            (&self.resolve, AdminAction::Inspect),
            (&self.repair, AdminAction::Restore),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.admin_dialog(action, false);
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.ack.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let status = p.helper.borrow()["status"].clone();
            let expected = status["state"]["confirmed"].clone();
            let Some(recovery_id) = status["recoveryId"].as_str().map(str::to_owned) else {
                return;
            };
            let selected = selected_identity(&p.app);
            let check = if expected["mode"] == "off" {
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
        self.queued.set(false);
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

    fn apply(self: &Rc<Self>) {
        if !card_present(&self.app) {
            return;
        }
        if !usable(&self.app, &self.helper.borrow(), selected_identity(&self.app).as_ref()) {
            // Første «Bruk»: aktiver (og om nødvendig oppdater) kortlyset med
            // ett passord. Valget brukes som en vanlig prøve når hjelperen er
            // klar, med «Behold» og tilbakerulling som før.
            self.admin_dialog(AdminAction::Activate, true);
            return;
        }
        let mode = self.mode.value().unwrap_or_else(|| "static".into());
        let helper = self.helper.borrow();
        let status = &helper["status"];
        let chosen = effect_name(&mode).map(|_| mode.as_str());
        if !can_apply(status, chosen) {
            return;
        }
        drop(helper);
        let request = if mode == "off" {
            Request::Off {}
        } else {
            let Some(color) = hex_color(&self.color.text()) else {
                return;
            };
            let Some(brightness) = u8::try_from(self.brightness.value_as_int())
                .ok()
                .and_then(|v| Brightness::try_from(v).ok())
            else {
                return;
            };
            Request::Static { color, brightness }
        };
        let mut args = json!({"request": request, "confirmTimeoutS": 15});
        if effect_name(&mode).is_some() {
            args["effect"] = json!(mode);
        }
        self.operation("rgb_apply", args);
    }

    fn operation(self: &Rc<Self>, op: &'static str, mut args: Value) {
        if self.fixture.get() || self.busy.get() || !self.app.client.is_connected() {
            return;
        }
        let h = self.helper.borrow().clone();
        if flow_requested(&self.app)
            && let Err(error) = flow_environment(&self.app, &h)
        {
            *self.message.borrow_mut() = Some(error);
            self.render();
            return;
        }
        let selected = selected_identity(&self.app);
        if !usable(&self.app, &h, selected.as_ref()) || h["busy"] == true {
            return;
        }
        let Some((pci, uuid)) = selected else { return };
        if matches!(op, "rgb_confirm" | "rgb_revert") {
            if h["owned"] != true {
                return;
            }
            let Some(txn) = h["status"]["state"]["pending"]["txn"].as_u64() else {
                return;
            };
            args["txn"] = json!(txn);
        }
        if op == "rgb_release" && h["owned"] != true {
            return;
        }
        args["gpu"] = json!(pci);
        args["uuid"] = json!(uuid);
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
                }
                Err(error) => {
                    if let Some(details) = &error.details
                        && details["status"].is_object()
                    {
                        p.helper.borrow_mut()["status"] = details["status"].clone();
                    }
                    let message = operation_error(&error);
                    *p.message.borrow_mut() = Some(message.clone());
                    p.summary.set_tooltip_text(Some(&error.message));
                    let title = if op == "rgb_read" {
                        tr!("Kunne ikke lese kortlyset")
                    } else {
                        tr!("Lysendringen ble ikke fullført")
                    };
                    p.app.toast(Toast::new(title).body(&message).error());
                }
            }
            p.render();
            p.refresh();
        });
    }

    pub fn render(&self) {
        let helper = self.helper.borrow();
        let status = &helper["status"];
        let state = &status["state"];
        let selected = selected_identity(&self.app);
        let stale = rgb_stale(&self.app, &helper);
        let bound = bound_status(&helper, selected.as_ref()) && !stale;
        // En uavklart RGB-økt: fra den kjørende hjelperen, eller rapportert av
        // GPU-oppdateringen (som lot RGB-delen stå).
        let unsettled = stale
            && (state["dirty"] == true
                || state["recovery"].is_string()
                || crate::system_install::unsettled_rgb_reported()
                    .is_some_and(|pci| pci.is_empty() || selected.as_ref().is_some_and(|s| s.0 == pci)));
        let connected = self.app.client.is_connected();
        let present = card_present(&self.app);
        let owned = helper["owned"] == true;
        let other = helper["busy"] == true && !owned;
        let pending = state["pending"].is_object();
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
        let chosen_effect = effect_name(&selected_mode);
        let blocked = is_blocked(state);
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
        } else if !bound {
            tr!("Ikke aktivert")
        } else if pending {
            tr!("Prøve pågår")
        } else if worker_effect.is_some() {
            tr!("Effekt aktiv")
        } else if blocked {
            tr!("Må avklares")
        } else {
            tr!("Egen RGB-tillatelse")
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
            connected && bound && (pending || blocked),
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
        } else if unsettled {
            tr!("En tidligere RGB-økt er ikke avsluttet, og RGB-delen må oppdateres. Trykk «Bruk»: kortet settes tilbake til det lagrede lyset, du ser på kortet og bekrefter, og så oppdateres og aktiveres kortlyset med ett passord.").into()
        } else if stale {
            tr!("RGB-delen er eldre enn GPU-delen og må oppdateres. Trykk «Bruk» for å oppdatere og aktivere kortlyset med ett passord.").into()
        } else if helper["state"] == "on" {
            tr!("RGB-tillatelsen gjelder ikke kortet som er valgt. Ingen endringer kan sendes her.")
                .into()
        } else if newer_installed || helper["state"] == "error" {
            tr!("RGB-hjelperen er ikke klar. Se «Detaljer og feilsøking» nedenfor.").into()
        } else if helper["state"] == "outdated" {
            tr!("Velg lys og trykk «Bruk». RGB-hjelperen oppdateres først, og du skriver passordet én gang.").into()
        } else {
            tr!("Velg lys og trykk «Bruk». Første gang aktiveres kortlyset med passordet ditt.").into()
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
                    "{effekt} er aktiv på hele kortlyset. Velg en annen modus eller farge og trykk «Bruk» for å bytte, eller «Stopp» for å gjenopprette det lagrede utgangspunktet.", effekt = name
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
        self.recovery.set_visible(bound && !recovery.is_empty());
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
                request_text(&state["pending"]["requested"]),
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
        let off = self.mode.value().as_deref() == Some("off");
        let valid_color = hex_color(&self.color.text()).is_some();
        class(&self.color, "error", !valid_color && !off);
        let color = (!off).then(|| hex_color(&self.color.text())).flatten();
        if self.swatch_color.replace(color) != color {
            self.swatch.queue_draw();
        }
        self.support.set_visible(!bound);
        self.controls.set_visible(present);
        self.apply.set_visible(present);
        self.readback_note.set_visible(bound);
        self.color.set_sensitive(!off && !self.busy.get());
        self.swatches_box.set_sensitive(!self.busy.get());
        self.brightness_box.set_sensitive(!off && !self.busy.get());
        self.mode.root.set_sensitive(!self.busy.get());
        let chosen_id = chosen_effect.map(|_| selected_mode.as_str());
        let activation = admin_paths(&self.app, &helper, AdminAction::Activate);
        let installable = installable(&self.app, &helper);
        let admin_available = connected
            && present
            && !self.busy.get()
            && !other
            && selected_identity(&self.app).is_some();
        let activatable = admin_available
            && (helper["state"] != "outdated" || installable)
            && (activation.is_ok() || installable);
        let apply_ok = if bound {
            available && present && (off || valid_color) && can_apply(status, chosen_id)
        } else {
            activatable && (stale || matches!(helper["state"].as_str(), Some("off" | "outdated"))) && (off || valid_color)
        };
        self.apply.set_sensitive(apply_ok);
        let unbound_hint = activation.as_ref().err().filter(|_| !installable).cloned().unwrap_or_default();
        let apply_hint = if apply_ok || self.busy.get() || !connected || !present || other {
            ""
        } else if !off && !valid_color {
            tr!("Oppgi en gyldig fargekode på seks tegn, for eksempel #3A7BF0.")
        } else if !bound {
            unbound_hint.as_str()
        } else if pending {
            tr!("Bekreft eller rull tilbake prøven som pågår før du bruker en ny innstilling.")
        } else if blocked {
            tr!("Avklar gjenopprettingen eller feilen nedenfor før du bruker en ny innstilling.")
        } else if chosen_id.is_some_and(|effect| !supports_effect(status, effect)) {
            tr!("RGB-hjelperen støtter ikke dette lysvalget.")
        } else {
            ""
        };
        set_text(&self.apply_hint, apply_hint);
        self.apply_hint.set_visible(!apply_hint.is_empty());
        self.refresh
            .set_sensitive(connected && !self.reading.get() && !self.busy.get());
        self.activate.set_visible(!bound && present && admin_ui);
        let resolution = admin_paths(&self.app, &helper, AdminAction::Resolve);
        let restoration = admin_paths(&self.app, &helper, AdminAction::Restore);
        // Mangler systemkomponentene, installerer «Bruk» dem med samme
        // passord. Terminalkommandoen er reserven uten nett.
        self.install_prompt.set_visible(
            !self.app.profile.is_dev() && admin_ui && !newer_installed
                && (helper["state"] == "outdated" || activation.is_err()),
        );
        let deactivation = admin_paths(&self.app, &helper, AdminAction::Deactivate);
        self.activate.set_sensitive(activatable);
        self.activate.set_tooltip_text(activation.as_ref().err().filter(|_| !installable).map(String::as_str));
        self.resolve.set_visible(
            !bound && present && admin_ui && matches!(helper["state"].as_str(), Some("off" | "error" | "outdated")),
        );
        self.resolve
            .set_sensitive(admin_available && resolution.is_ok());
        self.resolve.set_tooltip_text(resolution.err().as_deref());
        self.repair.set_visible(
            !bound && present && admin_ui && matches!(helper["state"].as_str(), Some("off" | "error" | "outdated")),
        );
        self.repair
            .set_sensitive(admin_available && restoration.is_ok());
        self.repair.set_tooltip_text(restoration.err().as_deref());
        self.deactivate.set_visible(
            (bound && state["dirty"] == false && !pending) || (!bound && deactivation.is_ok()),
        );
        self.deactivate
            .set_sensitive(connected && !self.busy.get() && !other && deactivation.is_ok());
        self.deactivate
            .set_tooltip_text(deactivation.err().as_deref());
        self.reset.set_visible(
            bound
                && worker_effect.is_none()
                && (state["dirty"] == true || state["readError"].is_string()),
        );
        self.reset.set_sensitive(available && !pending);
        self.release.set_visible(bound && owned);
        self.release.set_sensitive(available);
        self.release.set_label(if worker_effect.is_some() {
            tr!("Stopp")
        } else {
            tr!("Avslutt lysstyring")
        });
        self.release.set_tooltip_text(Some(tr!("Gjenopprett det lagrede utgangspunktet og frigi lysstyringen. Se på kortet før du bekrefter gjenopprettingen.")));
        self.ack
            .set_visible(bound && status["recoveryId"].is_string());
        self.ack.set_sensitive(available && !pending);
        let state_name = helper["state"].as_str().unwrap_or("").to_owned();
        if state_name == "error" && *self.last_state.borrow() != "error" {
            self.details.set_expanded(true);
        }
        *self.last_state.borrow_mut() = state_name;
        if self.queued.get() && bound && !self.busy.get() {
            // Aktiveringen er ferdig: bruk valget som utløste den, én gang.
            self.queued.set(false);
            if apply_ok {
                glib::idle_add_local_once(|| {
                    if let Some(p) = CURRENT.with(|c| c.borrow().upgrade()) {
                        p.apply();
                    }
                });
            }
        }
        if let Some(shell) = self.app.shell.get() {
            shell.topbar.set_locked("lys", false);
        }
        let observed_color: Option<[u8; 3]> = serde_json::from_value(observed["color"].clone()).ok();
        let observed_brightness = observed["brightnessRaw"].as_u64().and_then(|v| u8::try_from(v).ok());
        let visual = VisualState {
            bound,
            pending: pending && bound,
            selected_mode,
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

    fn admin_dialog(self: &Rc<Self>, action: AdminAction, then_apply: bool) {
        if self.busy.get() || (action != AdminAction::Deactivate && !card_present(&self.app)) {
            return;
        }
        let mut identity = selected_identity(&self.app);
        if action == AdminAction::Deactivate
            && !bound_status(&self.helper.borrow(), identity.as_ref())
        {
            identity = None;
        }
        if identity.is_none() && action != AdminAction::Deactivate {
            return;
        }
        let install = action == AdminAction::Activate && installable(&self.app, &self.helper.borrow());
        if let Err(error) = admin_paths(&self.app, &self.helper.borrow(), action).map(|_| ()).or_else(|e| if install { Ok(()) } else { Err(e) }) {
            self.app.toast(
                Toast::new(tr!("RGB-oppsettet er ikke klart"))
                    .body(&error)
                    .error(),
            );
            return;
        }
        if action == AdminAction::Inspect {
            self.run_admin(action, identity, None, false);
            return;
        }
        let (title, text, check) = match action {
            AdminAction::Deactivate => (
                tr!("Slå av RGB-tilgang"),
                tr!("Dette stopper RGB-arbeideren og fjerner kortets RGB-tillatelse når gjenopprettingen er avklart. Det lagrede utgangspunktet beholdes."),
                tr!("Jeg vil slå av RGB-tilgangen for dette kortet."),
            ),
            AdminAction::Inspect | AdminAction::Resolve => unreachable!("egen avklaringsdialog"),
            AdminAction::Restore => (
                tr!("Gjenopprett lagret lys"),
                tr!("Administratorhjelperen stopper RGB-arbeideren hvis den kjører. Hvis det finnes en uavklart journal, forsøker den deretter å gjenopprette det lagrede utgangspunktet. Kontroller lyset og velg «Avklar tidligere økt» etterpå."),
                tr!("Jeg vil gjenopprette det lagrede utgangspunktet på dette kortet."),
            ),
            AdminAction::Activate => (
                tr!("Aktiver lys på kortet"),
                tr!("Se på det fysiske kortet før du fortsetter. Lyset må allerede vise én fast farge. Pust, fargeskift og et ukjent utgangspunkt kan ikke lagres som et sikkert utgangspunkt. Administratorgodkjenningen gjelder bare kortlyset; den endrer ikke GPU-tillatelsen."),
                tr!("Jeg ser kortet og bekrefter at det viser én fast farge."),
            ),
        };
        // En eventuell gammel RGB-økt avklares under oppdateringen, men først
        // etter at brukeren har sett det gjenopprettede lyset (system-hent.py).
        let text = if install {
            // GPU-delen fra samme utgivelse står (`--keep-nvml`). Ellers byttes
            // den også, og en aktiv GPU-hjelper startes på nytt.
            let gpu = if crate::system_install::gpu_part_current() {
                tr!("GPU-styringen røres ikke.")
            } else if crate::widgets::locked::helper_state(&self.app) == "on" {
                tr!("GPU-hjelperen oppdateres også og startes på nytt: effekt, vifter og klokker går tilbake til utgangspunktet og må brukes på nytt.")
            } else {
                ""
            };
            let restore = tr!("Har en tidligere RGB-økt ikke blitt avsluttet, settes kortet først tilbake til det lagrede lyset i samme godkjenning. Deretter ber appen deg se på kortet og bekrefte det gjenopprettede lyset før økten avsluttes og oppdateringen fortsetter.");
            [text, restore, gpu].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join("\n\n")
        } else {
            text.to_owned()
        };
        let weak = Rc::downgrade(self);
        confirm(
            &self.app,
            title,
            &trf!("Kort: {kort}\n\n{beskrivelse}", kort = identity.as_ref().map_or(tr!("Lagret binding"), |card| card.0.as_str()), beskrivelse = text),
            check,
            tr!("Fortsett til administratorgodkjenning"),
            move || {
                if let Some(p) = weak.upgrade()
                    && (identity.is_none() || selected_identity(&p.app) == identity)
                {
                    p.queued.set(then_apply);
                    p.run_admin(action, identity, None, install);
                }
            },
        );
    }

    fn confirm_offline(self: &Rc<Self>, identity: (String, String), expected: &Value, id: String) {
        let weak = Rc::downgrade(self);
        self.confirm_restored(identity.clone(), expected, tr!("Fortsett til administratorgodkjenning"), move |ok| {
            if let Some(p) = weak.upgrade().filter(|_| ok) {
                p.run_admin(AdminAction::Resolve, Some(identity), Some(id), false);
            }
        });
    }

    /// Brukeren ser på kortet og bekrefter det gjenopprettede, lagrede lyset.
    /// `done(true)` bare når kortet fortsatt er valgt og brukeren bekreftet.
    fn confirm_restored(
        self: &Rc<Self>,
        identity: (String, String),
        expected: &Value,
        action: &str,
        done: impl FnOnce(bool) + 'static,
    ) {
        let weak = Rc::downgrade(self);
        confirm_or_cancel(
            &self.app,
            tr!("Avklar tidligere RGB-økt"),
            &trf!("Lagret lys for {kort}: {lys}.\n\nSe på kortet. Fortsett bare hvis lyset viser dette utgangspunktet som én fast farge.", kort = identity.0, lys = request_text(expected)),
            tr!("Jeg ser at kortlyset viser det lagrede, faste utgangspunktet."),
            action,
            move |ok| {
                done(ok && weak.upgrade().is_some_and(|p| selected_identity(&p.app).as_ref() == Some(&identity)));
            },
        );
    }

    /// Ett passord: hent og installer systemkomponentene, og aktiver kortlyset
    /// for kortet brukeren nettopp bekreftet visuelt.
    fn install_and_activate(self: &Rc<Self>, identity: Option<(String, String)>) {
        let Some((pci, uuid)) = identity else {
            self.queued.set(false);
            return;
        };
        if self.busy.get() {
            self.queued.set(false);
            return;
        }
        self.message.borrow_mut().take();
        // En gammel økt er gjenopprettet: avklares bare når brukeren har sett
        // nøyaktig det lagrede lyset på det samme, fortsatt valgte kortet.
        let weak = Rc::downgrade(self);
        let confirm: crate::system_install::ConfirmRecovery = Box::new(move |recovery, reply| {
            let identity = (recovery.pci, recovery.uuid);
            match weak.upgrade() {
                Some(p)
                    if selected_identity(&p.app).as_ref() == Some(&identity)
                        && serde_json::from_value::<Request>(recovery.expected.clone()).is_ok() =>
                {
                    p.confirm_restored(identity, &recovery.expected, tr!("Bekreft og fortsett oppdateringen"), reply);
                }
                _ => reply(false),
            }
        });
        let weak = Rc::downgrade(self);
        // `run` avviser selv samtidige administratorhandlinger (også denne
        // sidens), så `busy` settes først når installasjonen faktisk er i gang.
        crate::system_install::run(&self.app, crate::system_install::Activate::Rgb(pci, uuid, confirm), move |ok| {
            let Some(p) = weak.upgrade() else { return };
            p.busy.set(false);
            p.queued.set(p.queued.get() && ok);
            if ok {
                let message = tr!("RGB-oppsettet er oppdatert. Kontroller status før du bruker en farge.").to_string();
                *p.message.borrow_mut() = Some(message.clone());
                p.app.toast(Toast::new(tr!("RGB-oppsett")).body(&message));
            }
            p.render();
            p.refresh();
        });
        self.busy.set(crate::system_install::busy());
        self.render();
    }

    fn run_admin(
        self: &Rc<Self>,
        action: AdminAction,
        identity: Option<(String, String)>,
        recovery_id: Option<String>,
        confirmed_install: bool,
    ) {
        if self.fixture.get() || (action != AdminAction::Deactivate && !card_present(&self.app)) {
            self.queued.set(false);
            return;
        }
        if action == AdminAction::Activate {
            let can_install = installable(&self.app, &self.helper.borrow());
            if confirmed_install != can_install {
                self.queued.set(false);
                self.message.borrow_mut().take();
                self.app.toast(Toast::new(tr!("RGB-oppsettet er ikke klart")));
                self.render();
                self.refresh();
                return;
            }
            if confirmed_install {
                self.install_and_activate(identity);
                return;
            }
        }
        let Ok((pkexec, script)) = admin_paths(&self.app, &self.helper.borrow(), action) else {
            self.queued.set(false);
            return;
        };
        // Aldri samtidig med installasjonen av systemkomponentene.
        if crate::system_install::busy() || self.busy.replace(true) {
            self.queued.set(false);
            return;
        }
        self.message.borrow_mut().take();
        self.render();
        let mut argv = vec![pkexec.into_os_string(), script.into_os_string()];
        if let Some((pci, uuid)) = &identity {
            argv.extend(["--pci".into(), pci.into(), "--uuid".into(), uuid.into()]);
        }
        match action {
            AdminAction::Restore => argv.push("--restore-original".into()),
            AdminAction::Activate => argv.push("--static-confirmed".into()),
            AdminAction::Inspect => argv.push("--inspect".into()),
            AdminAction::Resolve => {
                argv.extend([
                    "--static-confirmed".into(),
                    "--recovery-id".into(),
                    recovery_id
                        .expect("avklaringsdialogen har en identitet")
                        .into(),
                ]);
            }
            AdminAction::Deactivate => {}
        }
        let argv: Vec<&std::ffi::OsStr> = argv.iter().map(|s| s.as_os_str()).collect();
        // Unntak: bare et eksplisitt brukerklikk etter visuell bekreftelse
        // starter polkit. Ingen startup-/profil-/statusvei kaller dette.
        #[allow(clippy::disallowed_methods)]
        let result = gtk::gio::Subprocess::newv(
            &argv,
            gtk::gio::SubprocessFlags::STDOUT_PIPE | gtk::gio::SubprocessFlags::STDERR_PIPE,
        );
        let proc = match result {
            Ok(proc) => proc,
            Err(error) => {
                self.busy.set(false);
                self.queued.set(false);
                let diagnostic = error.to_string();
                *self.message.borrow_mut() = Some(with_diagnostic(tr!("RGB-oppsettet kunne ikke fullføres."), Some(&diagnostic)));
                self.summary.set_tooltip_text(Some(&diagnostic));
                self.render();
                return;
            }
        };
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let result = proc.communicate_utf8_future(None).await;
            let Some(p) = weak.upgrade() else { return };
            p.busy.set(false);
            let mut code = if result.is_ok() && proc.has_exited() { proc.exit_status() } else { 1 };
            let (output, error) = result.unwrap_or_default();
            let mut invalid_target = false;
            if code == 0 && action == AdminAction::Inspect {
                let report = output.and_then(|out| serde_json::from_str::<Value>(&out).ok());
                if let Some(report) = &report
                    && let Some(id) = report["recoveryId"].as_str()
                    && serde_json::from_value::<Request>(report["expected"].clone()).is_ok()
                    && let Some(identity) = identity
                    && selected_identity(&p.app).as_ref() == Some(&identity)
                {
                    p.render();
                    p.confirm_offline(identity, &report["expected"], id.to_owned());
                    return;
                }
                if !matches!(report, Some(Value::Object(fields)) if fields.is_empty()) {
                    code = 1;
                    invalid_target = true;
                }
            }
            let diagnostic = error
                .as_deref()
                .and_then(|text| text.lines().rfind(|line| !line.trim().is_empty()))
                .filter(|_| code != 0 && code != 126);
            if code != 0 || action != AdminAction::Activate {
                p.queued.set(false);
            }
            let message = match code {
                0 if action == AdminAction::Restore => tr!("RGB-arbeideren er stoppet. Kontroller lyset og velg «Avklar tidligere økt» hvis en gjenoppretting gjenstår.").to_string(),
                0 if action == AdminAction::Inspect => tr!("Ingen uavklart RGB-journal for kortet. Du kan slå av RGB-tilgangen.").to_string(),
                0 if action == AdminAction::Resolve => tr!("Gjenopprettingen er avklart. Du kan slå av RGB-tilgangen eller aktivere lysstyring på nytt.").to_string(),
                0 if action == AdminAction::Deactivate => tr!("RGB-tilgangen er slått av.").to_string(),
                0 => tr!("RGB-oppsettet er oppdatert. Kontroller status før du bruker en farge.").to_string(),
                126 => tr!("Administratorgodkjenningen ble avbrutt.").to_string(),
                _ if invalid_target => tr!("Avklaringsmålet er ugyldig eller gjelder et annet valgt kort.").to_string(),
                // Årsaken fra administratorskriptet sier hva som må avklares.
                _ => with_diagnostic(tr!("RGB-oppsettet kunne ikke fullføres."), diagnostic),
            };
            p.summary.set_tooltip_text(diagnostic);
            *p.message.borrow_mut() = Some(message.clone());
            p.app.toast(if code == 0 { Toast::new(tr!("RGB-oppsett")).body(&message) } else { Toast::new(tr!("RGB-oppsett ble ikke fullført")).body(&message).error() });
            p.render();
            p.refresh();
        });
    }
}

/// Systemkomponentene mangler (eller er utdaterte), og appen kan installere
/// dem selv sammen med RGB-aktiveringen.
fn installable(app: &App, helper: &Value) -> bool {
    !app.profile.is_dev() && helper["systemAdmin"] == true && crate::system_install::can_install()
        && (helper["state"] == "outdated" || admin_paths(app, helper, AdminAction::Activate).is_err())
}

/// En RGB-handling pågår (installasjonen venter da).
pub(crate) fn admin_busy() -> bool {
    CURRENT.with(|c| c.borrow().upgrade().is_some_and(|p| p.busy.get()))
}

fn admin_paths(
    app: &App,
    helper: &Value,
    action: AdminAction,
) -> Result<(PathBuf, PathBuf), String> {
    if app.profile.is_dev() || helper["systemAdmin"] != true {
        return Err(
            tr!("Administratorhandlinger for RGB krever appens vanlige maskinvareoppsett.").into(),
        );
    }
    let pkexec = PathBuf::from("/usr/bin/pkexec");
    let script = rgb_auth::Paths::new("/").libexec(action.script());
    let need = if action == AdminAction::Activate {
        crate::system_install::Need::Rgb
    } else {
        crate::system_install::Need::Cleanup
    };
    if !crate::system_install::ready(need)
        || [
            pkexec.as_path(),
            &script,
            Path::new(rgb_auth::POLICY_PATH),
            Path::new("/usr/local/libexec/agentic-rigg-rgb-v2/agentic-rigg-helper"),
            Path::new("/usr/local/libexec/agentic-rigg-rgb-v2/agentic-rigg-rgb.service"),
        ]
        .into_iter()
        .any(|path| !crate::system_install::trusted(path))
    {
        let message = if crate::system_install::newer_installed() {
            tr!("RGB-hjelperen er ikke klar. Se «Detaljer og feilsøking» nedenfor.")
        } else {
            tr!("Systemkomponentene mangler. Se «Uten nett: bruk terminal» under «Detaljer og feilsøking».")
        };
        return Err(message.into());
    }
    Ok((pkexec, script))
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
                assert!(admin_paths(&p.app, &helper, AdminAction::Activate).is_err());
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
            "kept" => {
                assert!(state["pending"].is_null());
                assert_eq!(state["dirty"], true);
                assert_eq!(helper["owned"], true);
                assert_eq!(
                    state["confirmed"],
                    json!({"mode":"static","color":[50,180,135],"brightness":65})
                );
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
                assert!(p.apply_hint.is_visible());
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
                assert_eq!(p.release.label().as_deref(), Some(tr!("Stopp")));
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
                assert!(p.apply_hint.is_visible());
                assert!(state["observed"]["mode"].is_null());
                FLOW_INITIAL.with(|v| assert_eq!(state["confirmed"], v.borrow()["confirmed"]));
            }
            // Prøver videresending av et gammelt/feil forventet mål. Dette
            // velger aldri et skrivemål og er bare tilgjengelig i fake-flow.
            "stale_ack" => p.operation(
                "rgb_ack_recovery",
                json!({"confirmedStatic":true,"expected":{"mode":"off"},
                    "recoveryId":helper["status"]["recoveryId"]}),
            ),
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
            }
            "off" => p
                .mode
                .button("off")
                .expect("Av-knappen mangler")
                .set_active(true),
            "off_pending" => assert_eq!(state["pending"]["requested"], json!({"mode":"off"})),
            "release" => click(&p.release),
            "released" => {
                assert!(helper["status"]["effect"].is_null());
                assert_eq!(helper["owned"], false);
                assert!(state["pending"].is_null());
                assert_eq!(state["recoveryNeedsConfirmation"], true);
                FLOW_INITIAL.with(|v| assert_eq!(state["confirmed"], v.borrow()["confirmed"]));
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
                    "identity":{"pci":pci,"uuid":uuid,"pciIds":{"vendor":0x10de,"device":0x1e87,"subsystemVendor":0x10de,"subsystemDevice":0x1e87},"adapter":{"port":1,"name":adapter_name,"parentPci":pci,"driver":"nvidia"},"address":73,"signature":"palit_150"},
                    "state":{"baselineSource":"user_confirmed_static","confirmed":{"mode":"static","color":[58,123,240],"brightness":80},"observed":{"color":[58,123,240],"brightnessRaw":80,"mode":null},"pending":null,"dirty":false,"recovery":null,"recoveryNeedsConfirmation":false,"recoveryConfirmationSource":null,"error":null}
                }
            });
            FIXTURE_STALE.with(|s| s.set(kind == "stale_unsettled"));
            match kind {
                "off" => value = json!({"state":"off","owned":false,"busy":false,"message":"Kortlyset trenger sin egen administratoraktivering.","status":null}),
                // GPU-oppdateringen lot RGB-delen stå på bygg 31 med en uavklart økt.
                "stale_unsettled" => {
                    value["status"]["helper"]["build"] = json!(31);
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
                assert!(p.controls.is_visible() && p.apply.is_visible(), "Lysvalgene er skjult før aktivering");
                for (id, _) in EFFECTS {
                    assert!(p.mode.button(id).unwrap().is_visible(), "Effekter er skjult før aktivering");
                }
            } else if kind == "pending_effects" {
                for (id, _) in EFFECTS {
                    assert!(p.mode.button(id).unwrap().is_visible(), "Støttet effekt er skjult");
                }
            } else if usable(&p.app, &p.helper.borrow(), selected_identity(&p.app).as_ref()) {
                for (id, _) in EFFECTS {
                    assert!(!p.mode.button(id).unwrap().is_visible(), "Ustøttet effekt er synlig");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
                "identity":{"pci":selected.0,"uuid":selected.1,"pciIds":{"vendor":0x10de,"device":0x1e87,"subsystemVendor":0x10de,"subsystemDevice":0x1e87},"adapter":{"port":1,"name":"NVIDIA i2c adapter 1 at 1c:00.0","parentPci":selected.0,"driver":"nvidia"},"address":73,"signature":"palit_150"}
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
                    "signature": "palit_150"
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
