//! Låst-tilstanden (produktspesifikasjonen 6.2): popoveren «Hva skal til?» med støtte fra
//! `capabilities.write`, og porten som avgjør om en skriveoperasjon i det
//! hele tatt sendes.
//!
//! Her ligger også det Kontroller og Viftekurve lagrer i appen
//! (`profiler.json`: profilene og utkastet), og
//! to små byggeklosser de deler: [`Columns`] (hovedflate og
//! sidekolonne, under hverandre i smale vinduer) og [`ConfirmBar`]
//! (Behold eller Rull tilbake, med autoritativ status fra tjenesten).
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
use rigg_core::hjelper::{display_diagnostic, display_error};
use rigg_core::{tr, trf};
use rigg_core::proto::{ErrorCode, OpResult, ProtoError};
use rigg_core::txn::{self, Gate};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// ================================================================ porten

/// Porten krever autoritativ hjelperstatus og det samme kortet som vises.
pub fn gate(app: &App) -> Gate {
    let g = txn::write_gate(app.data.borrow().hello.as_ref(), app.profile.is_dev());
    if g != Gate::Real { return g; }
    if !app.client.is_connected() || app.gpu_info().is_none_or(|i| i.state != rigg_core::model::DeviceState::Ok) {
        return Gate::Locked(tr!("Skjermkortet eller tjenesten svarer ikke. Endringer er låst."));
    }
    if !helper_targets_selected(app) {
        return Gate::Locked(tr!("Hjelperen er aktivert for et annet kort. Aktiver styring for kortet som vises."));
    }
    Gate::Real
}

/// Bare ekte, aktivert og skriveklar styring skjuler lesevisningsmerket.
pub fn control_ready(app: &App) -> bool { gate(app) == Gate::Real }

pub fn helper(app: &App) -> Value {
    app.data.borrow().hello.as_ref().map(|h| h["capabilities"]["write"]["helper"].clone()).unwrap_or(Value::Null)
}

pub fn helper_state(app: &App) -> String {
    helper(app)["state"].as_str().unwrap_or("off").to_string()
}

/// Funksjonsgrenser tilhører ett konkret kort, også før skriveporten åpnes.
pub fn helper_targets_selected(app: &App) -> bool {
    let uuid = app.statics().and_then(|st| st.uuid);
    target_matches(&helper(app), app.gpu_id().as_deref(), uuid.as_deref())
}

fn target_matches(helper: &Value, pci: Option<&str>, uuid: Option<&str>) -> bool {
    match (pci, uuid, helper["identity"]["pci"].as_str(), helper["identity"]["uuid"].as_str()) {
        (Some(p), Some(u), Some(hp), Some(hu)) => p == hp && u == hu,
        _ => false,
    }
}

pub fn capabilities(app: &App) -> Value {
    let h = helper(app);
    if h["capabilities"].is_object() && (gate(app) == Gate::Sim || helper_targets_selected(app)) { return h["capabilities"].clone(); }
    if gate(app) == Gate::Sim {
        return app.data.borrow().hello.as_ref().map(|v| v["capabilities"]["write"]["fields"].clone()).unwrap_or(Value::Null);
    }
    Value::Null
}

pub fn supports(app: &App, field: &str) -> bool {
    gate(app) == Gate::Sim || capabilities(app)[field]["available"].as_bool().unwrap_or(false)
}

pub fn capability_reason(app: &App, field: &str) -> String {
    let caps = capabilities(app);
    caps[field]["reason"].as_str().map(display_diagnostic).map_or_else(|| tr!("Funksjonen er ikke tilgjengelig på dette kortet.").into(), |text| text.into_owned())
}

pub fn capability_range(app: &App, field: &str) -> Option<(f64, f64)> {
    let c = capabilities(app);
    Some((c[field]["min"].as_f64()?, c[field]["max"].as_f64()?))
}

/// Ny status skal også oppdatere skriveporten uten å vente på ny hello.
pub fn set_helper(app: &App, v: Value) {
    let before = helper(app);
    if ["state", "enabled", "ready", "build", "identity"].iter().any(|k| before[*k] != v[*k]) {
        CONTROL.with(|c| { c.borrow_mut().take(); });
        CONTROL_AT.with(|t| t.set(0));
        CONTROL_GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
    }
    if let Some(h) = app.data.borrow_mut().hello.as_mut() {
        h["capabilities"]["write"]["fields"] = v["capabilities"].clone();
        if h["daemon"]["backend"] == "nvml" {
            h["capabilities"]["write"]["state"] = json!(if v["enabled"] == true && v["ready"] == true && v["realWrites"] == true && v["backend"] == "nvml" { "ready" } else { "locked" });
        }
        h["capabilities"]["write"]["helper"] = v;
    }
}

pub fn refresh_helper(app: &Rc<App>, then: impl FnOnce() + 'static) {
    let a = app.clone();
    app.client.request("get_helper", json!({"refresh": true}), move |r| {
        if let Ok(v) = r { set_helper(&a, v); }
        a.on_service.emit(&());
        refresh_control(&a, true);
        then();
    });
}

/// Oppdag også aktivering utenfor det åpne vinduet, mens porten er låst.
/// Kun meningsfulle statusendringer utløser ny layout; sensorverdier gjør ikke det.
pub fn poll_helper(app: &Rc<App>) {
    if !app.client.is_connected() || HELPER_BUSY.with(Cell::get) { return; }
    let now = rigg_core::now_ms();
    if now - HELPER_AT.with(Cell::get) < 3000 { return; }
    HELPER_AT.with(|t| t.set(now));
    HELPER_BUSY.with(|b| b.set(true));
    let a = app.clone();
    app.client.request("get_helper", json!({"refresh": true}), move |r| {
        HELPER_BUSY.with(|b| b.set(false));
        if let Ok(v) = r {
            let before = helper(&a);
            let changed = helper_status_changed(&before, &v);
            set_helper(&a, v);
            if changed {
                a.on_service.emit(&());
                crate::settings::request_render();
                refresh_control(&a, true);
            }
        }
    });
}

fn helper_status_changed(before: &Value, after: &Value) -> bool {
    ["state", "enabled", "ready", "realWrites", "backend", "build", "identity", "capabilities", "message", "hardwareError"]
        .iter().any(|key| before[*key] != after[*key])
}

pub fn watch_helper(app: &Rc<App>) {
    let weak = Rc::downgrade(app);
    app.on_telemetry.connect(move |_| {
        if let Some(app) = weak.upgrade() { poll_helper(&app); }
    });
}

thread_local! {
    static HELPER_BUSY: Cell<bool> = const { Cell::new(false) };
    static HELPER_AT: Cell<i64> = const { Cell::new(0) };
    static CONTROL: RefCell<Option<(String, Value)>> = const { RefCell::new(None) };
    static CONTROL_BUSY: Cell<bool> = const { Cell::new(false) };
    static CONTROL_AT: Cell<i64> = const { Cell::new(0) };
    static CONTROL_GENERATION: Cell<u64> = const { Cell::new(0) };
}

fn control_key(app: &App) -> String {
    format!("{}/{}", app.gpu_id().unwrap_or_default(), app.statics().and_then(|s| s.uuid).unwrap_or_default())
}

#[derive(Clone)]
struct ControlOrigin {
    key: String,
    gpu: Option<String>,
    uuid: Option<String>,
    real: bool,
}

impl ControlOrigin {
    fn capture(app: &App) -> Self {
        Self { key: control_key(app), gpu: app.gpu_id(), uuid: app.statics().and_then(|s| s.uuid), real: gate(app) == Gate::Real }
    }

    fn args(&self, mut args: Value) -> Value {
        if self.real {
            args["gpu"] = json!(self.gpu);
            args["uuid"] = json!(self.uuid);
        }
        args
    }
}

fn remember_control(app: &App, v: &Value) {
    if !v["activeValues"].is_object() { return; }
    CONTROL.with(|c| *c.borrow_mut() = Some((control_key(app), v.clone())));
}

fn control_view(app: &App) -> Option<Value> {
    if !control_ready(app) { return None; }
    let key = control_key(app);
    CONTROL.with(|c| c.borrow().as_ref().filter(|(k, _)| k == &key).map(|(_, v)| v.clone()))
}

pub fn control_values(app: &App) -> Option<txn::Values> {
    serde_json::from_value(control_view(app)?["activeValues"].clone()).ok()
}

/// Bare verdier som NVML faktisk har kunnet lese tilbake.
pub fn observed_control(app: &App) -> Option<Value> {
    let v = control_view(app)?;
    if v["hardwareKnown"] != true { return None; }
    let mut observed = v["hardware"]["observed"].clone();
    if v["hardware"]["offsetsVerified"][0] == false { observed["coreOffsetMhz"] = Value::Null; }
    if v["hardware"]["offsetsVerified"][1] == false { observed["memOffsetMhz"] = Value::Null; }
    Some(observed)
}

pub fn stock_values(app: &App) -> Option<txn::Values> {
    serde_json::from_value(control_view(app)?["stockValues"].clone()).ok()
}

/// Lesing av faktisk hjelpertilstand, høyst hvert femte sekund i vanlig visning.
pub fn refresh_control(app: &Rc<App>, force: bool) {
    if !control_ready(app) || CONTROL_BUSY.with(Cell::get) { return; }
    let now = rigg_core::now_ms();
    if !force && now - CONTROL_AT.with(Cell::get) < 5000 { return; }
    CONTROL_BUSY.with(|b| b.set(true));
    CONTROL_AT.with(|t| t.set(now));
    let a = app.clone();
    let key = control_key(app);
    let generation = CONTROL_GENERATION.with(Cell::get);
    let args = json!({"gpu": app.gpu_id(), "uuid": app.statics().and_then(|s| s.uuid)});
    app.client.request("get_state", args, move |r| {
        CONTROL_BUSY.with(|b| b.set(false));
        if key != control_key(&a) { return; }
        if generation != CONTROL_GENERATION.with(Cell::get) { refresh_control(&a, true); return; }
        match r {
            Ok(v) => {
                if v["write"]["helper"].is_object() { set_helper(&a, v["write"]["helper"].clone()); }
                remember_control(&a, &v);
            },
            Err(_) => CONTROL.with(|c| { c.borrow_mut().take(); }),
        }
        a.on_service.emit(&());
    });
}

/// Send bare endrede felt. Ukjente/ustøttede felt utelates aldri i stillhet.
pub fn changed_patch(before: &txn::Values, after: &txn::Values) -> Value {
    fn diff(a: &Value, b: &Value) -> Value {
        let mut out = serde_json::Map::new();
        if let Some(obj) = b.as_object() {
            for (k, v) in obj {
                if a.get(k) == Some(v) { continue; }
                let v = if v.is_object() { diff(&a[k], v) } else { v.clone() };
                out.insert(k.clone(), v);
            }
        }
        Value::Object(out)
    }
    diff(&before.to_json(), &after.to_json())
}

pub fn patch_error(app: &App, before: &txn::Values, after: &txn::Values) -> Option<String> {
    if gate(app) == Gate::Sim { return None; }
    let p = changed_patch(before, after);
    let mut needs = Vec::new();
    if p.get("powerLimitW").is_some() { needs.push("power"); }
    if p.get("tempTargetC").is_some() { needs.push("temp_target"); }
    if p["clockOffsets"].get("coreMhz").is_some() { needs.push("core_clock"); }
    if p["clockOffsets"].get("memMhz").is_some() { needs.push("mem_clock"); }
    if p.get("fan").is_some() {
        needs.push(if after.fan_mode == txn::FanMode::Curve { "fan_curve" } else { "fan_manual" });
    }
    needs.into_iter().find(|f| !supports(app, f)).map(|f| capability_reason(app, f))
}

/// Alle ekte operasjoner bindes til PCI + UUID. Hjelperen validerer på nytt.
pub fn send_write(app: &Rc<App>, op: &str, mut args: Value, cb: impl FnOnce(OpResult) + 'static) {
    let g = gate(app);
    if !rigg_core::proto::WRITE_OPS.contains(&op) || matches!(g, Gate::Locked(_)) {
        cb(Err(ProtoError::new(ErrorCode::WriteLocked)));
        return;
    }
    if g == Gate::Real {
        if matches!(op, "rgb_get" | "rgb_set") && !supports(app, "rgb") {
            cb(Err(ProtoError::msg(ErrorCode::NotSupported, capability_reason(app, "rgb"))));
            return;
        }
        args["gpu"] = json!(app.gpu_id());
        args["uuid"] = json!(app.statics().and_then(|s| s.uuid));
    }
    let origin = ControlOrigin::capture(app);
    let a = app.clone();
    app.client.request(op, args, move |r| {
        if origin.key != control_key(&a) {
            // apply kan allerede ha endret A mens brukeren nå viser B.
            // Rull bare denne transaksjonen tilbake mot dens opprinnelige kort.
            if let Ok(v) = &r
                && let Some(id) = v["txn"].as_str()
                && v["pending"].is_object()
            {
                let a2 = a.clone();
                a.client.request("revert", origin.args(json!({"txn": id})), move |reverted| {
                    let message = match reverted {
                        Ok(v) if v["hardwareError"].is_null() && v["hardwareKnown"] != false => tr!("Valgt kort ble endret. Endringen på det opprinnelige kortet er rullet tilbake.").to_string(),
                        Ok(_) => tr!("Valgt kort ble endret. Tilbakestilling på det opprinnelige kortet er ikke bekreftet. Se status i Innstillinger › Skjermkortstyring.").to_string(),
                        Err(e) => trf!("Valgt kort ble endret. Tilbakestilling på det opprinnelige kortet kunne ikke bekreftes: {feil}", feil = display_error(&e)),
                    };
                    cb(Err(ProtoError::msg(ErrorCode::WriteLocked, message)));
                    refresh_helper(&a2, || {});
                });
            } else {
                cb(Err(ProtoError::msg(ErrorCode::WriteLocked, tr!("Valgt kort ble endret mens forespørselen ventet. Svaret brukes ikke på det nye kortet."))));
            }
            return;
        }
        if let Ok(v) = &r {
            if v["write"]["helper"].is_object() { set_helper(&a, v["write"]["helper"].clone()); }
            remember_control(&a, v);
        } else { refresh_helper(&a, || {}); }
        cb(r);
        a.on_service.emit(&());
        refresh_control(&a, true);
    });
}

pub fn mark_topbar(app: &Rc<App>, page: &'static str) {
    let a = app.clone();
    glib::idle_add_local_once(move || sync_topbar(&a, page));
}

pub fn sync_topbar(app: &Rc<App>, page: &'static str) {
    if let Some(s) = app.shell.get() {
        // Kortlyset har egen arbeider og viser status ved sine kontroller.
        let open = !matches!(gate(app), Gate::Locked(_));
        s.topbar.set_locked(page, !open);
    }
}

pub fn badge(app: &App) -> (String, &'static str, String) {
    if gate(app) == Gate::Sim { return (tr!("Simulert kort").into(), "circle-dot", tr!("Endringer gjelder bare simuleringen.").into()); }
    match helper_state(app).as_str() {
        "on" if control_ready(app) => (tr!("Styring klar").into(), "shield", tr!("Støttede endringer sendes til det valgte kortet når du trykker Bruk.").into()),
        "on" if gate(app) == Gate::Sim => (tr!("Simulert styring").into(), "circle-dot", tr!("Endringer gjelder bare det simulerte kortet.").into()),
        "on" => (tr!("Styring utilgjengelig").into(), "triangle-alert", helper_line(app).unwrap_or_default()),
        "waiting" => (tr!("Venter på enhet").into(), "timer", helper_line(app).unwrap_or_default()),
        "error" => (tr!("Styring: feil").into(), "triangle-alert", helper_line(app).unwrap_or_default()),
        "outdated" => (tr!("Oppdater hjelper").into(), "circle-alert", helper_line(app).unwrap_or_default()),
        _ => (tr!("Bare visning").into(), "lock", tr!("Endringer på skjermkortet er slått av. Aktiver én gang med administratorpassord i Innstillinger › Skjermkortstyring.").into()),
    }
}

pub fn helper_line(app: &App) -> Option<String> {
    let h = helper(app);
    match helper_state(app).as_str() {
        "on" if control_ready(app) || gate(app) == Gate::Sim => None,
        "on" => Some(h["message"].as_str().map(display_diagnostic).map_or_else(|| tr!("Hjelperen er ikke skriveklar for kortet som vises.").into(), |text| text.into_owned())),
        "waiting" => Some(h["message"].as_str().map(display_diagnostic).map_or_else(|| tr!("Venter på enhet").into(), |text| text.into_owned())),
        "error" => Some(h["message"].as_str().map(display_diagnostic).map_or_else(|| tr!("Hjelperen svarer ikke. Endringer er låst.").into(), |text| text.into_owned())),
        "outdated" => Some(tr!("Hjelperen må oppdateres. Oppdater systemkomponentene fra Innstillinger › Skjermkortstyring.").into()),
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

/// Innholdet i `profiler.json`. Eies av vinduet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stored {
    pub profiles: Vec<Option<Profile>>,
    /// Utkastet på Kontroller og Viftekurve (`None`: lesevisning).
    pub utkast: Option<txn::Values>,
    /// Opprinnelig grunnlag og kortidentitet; mangler for eldre utkast.
    pub utkast_base: Option<txn::Values>,
    pub utkast_gpu: Option<String>,
    /// Visningsnivå på Vifter. Endrer aldri kortet eller utkastet.
    pub fan_advanced: bool,
    /// Visningsvalg for 2D/3D-kortet på Lys (vinkel, zoom, lagtrinn, 2D/3D).
    pub lys: Value,
}

impl Stored {
    fn to_json(&self) -> Value {
        json!({
            "version": 2,
            "profiles": self.profiles.iter().map(|p| p.as_ref().map_or(Value::Null, Profile::to_json)).collect::<Vec<_>>(),
            "utkast": self.utkast.as_ref().map_or(Value::Null, |u| serde_json::to_value(u).unwrap_or(Value::Null)),
            "utkastBase": self.utkast_base,
            "utkastGpu": self.utkast_gpu,
            "fanAdvanced": self.fan_advanced,
            "lys": self.lys,
        })
    }
    /// Ett dårlig felt nullstiller ikke resten.
    fn from_json(v: &Value) -> Stored {
        let mut s = Stored {
            profiles: v["profiles"].as_array().map(|a| a.iter().map(Profile::from_json).collect()).unwrap_or_default(),
            utkast: serde_json::from_value(v["utkast"].clone()).ok(),
            utkast_base: serde_json::from_value(v["utkastBase"].clone()).ok(),
            utkast_gpu: v["utkastGpu"].as_str().map(str::to_string),
            fan_advanced: v["fanAdvanced"].as_bool().unwrap_or(false),
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
    static BINDING_DRAFT: Cell<bool> = const { Cell::new(false) };
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

pub fn fan_advanced(app: &App) -> bool { stored(app).fan_advanced }

/// Bare lokal visning; bruker den vanlige lagringen uten endring i utkast.
pub fn set_fan_advanced(app: &Rc<App>, advanced: bool) {
    update(app, "fan-view", |s| s.fan_advanced = advanced);
}

/// Endrer det som er lagret, sier fra til sidene (`what`: «utkast»,
/// «profiler») og lagrer 500 ms etter siste endring.
pub fn update(app: &Rc<App>, what: &str, f: impl FnOnce(&mut Stored)) {
    let (changed, draft_before, binding_changed) = STORE.with(|st| {
        let mut b = st.borrow_mut();
        let s = b.get_or_insert_with(|| load(app));
        let before = s.clone();
        f(s);
        // Direkte profil-/legacy-innlasting må aldri arve bindingen til et
        // tidligere utkast. set_draft er den eksplisitte, bundne inngangen.
        if s.utkast.is_some() && s.utkast != before.utkast && !BINDING_DRAFT.with(Cell::get) && !UNDOING.with(Cell::get) {
            s.utkast_base = None;
            s.utkast_gpu = None;
        }
        let binding_changed = s.utkast_base != before.utkast_base || s.utkast_gpu != before.utkast_gpu;
        (*s != before, (s.utkast != before.utkast).then_some(before.utkast), binding_changed)
    });
    if !changed {
        return;
    }
    // Ctrl+Z: hver endring i utkastet huskes (ikke når den kommer fra angre).
    if binding_changed {
        UNDO.with(|u| *u.borrow_mut() = UndoStack::default());
        if draft_before == Some(None) {
            UNDO.with(|u| u.borrow_mut().record(&None, rigg_core::now_ms()));
        }
    } else if let Some(prev) = draft_before
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
    let mut d = match validated_draft(app, &base) {
        Ok(d) => d.unwrap_or_else(|| base.clone()),
        Err(why) => {
            app.toast(crate::toast::Toast::new(tr!("Utkastet må oppdateres")).body(&why).error());
            return;
        }
    };
    f(&mut d);
    if stored(app).utkast.is_none() && d == base { return; }
    set_draft(app, base, d);
}

/// Bevarer bare feltene brukeren endret, også når hjelperen leser nye verdier.
fn rebase_draft(original: &txn::Values, draft: &txn::Values, current: &txn::Values) -> txn::Values {
    let before = serde_json::to_value(original).expect("Values kan serialiseres");
    let wanted = serde_json::to_value(draft).expect("Values kan serialiseres");
    let mut rebased = serde_json::to_value(current).expect("Values kan serialiseres");
    for (field, value) in wanted.as_object().expect("Values er et objekt") {
        if before.get(field) != Some(value) { rebased[field] = value.clone(); }
    }
    serde_json::from_value(rebased).expect("bare felt fra Values er erstattet")
}

/// Legacyutkast og utkast fra et annet kort beholdes, men kan ikke sendes.
pub fn validated_draft(app: &App, current: &txn::Values) -> Result<Option<txn::Values>, String> {
    validate_draft_binding(stored(app), &control_key(app), current)
}

fn validate_draft_binding(s: Stored, key: &str, current: &txn::Values) -> Result<Option<txn::Values>, String> {
    let Some(d) = s.utkast else { return Ok(None) };
    let Some(original) = s.utkast_base else {
        return Err(tr!("Dette eldre utkastet mangler opprinnelig grunnlag. Behold det som referanse, eller forkast det og lag et nytt utkast fra kortets verdier.").into());
    };
    if s.utkast_gpu.as_deref() != Some(key) {
        return Err(tr!("Utkastet tilhører en annen kortidentitet. Velg det opprinnelige kortet, eller forkast utkastet før du lager et nytt.").into());
    }
    Ok(Some(rebase_draft(&original, &d, current)))
}

pub fn draft_for_apply(app: &App, current: &txn::Values) -> Result<txn::Values, String> {
    validated_draft(app, current)?.ok_or_else(|| tr!("Det finnes ikke noe utkast å bruke.").into())
}

/// Profilinnlasting og eksplisitt reset er en ny intensjon på valgt kort.
pub fn set_draft(app: &Rc<App>, base: txn::Values, next: txn::Values) {
    let key = control_key(app);
    BINDING_DRAFT.with(|b| b.set(true));
    update(app, "utkast", |s| {
        s.utkast = Some(next);
        s.utkast_base = Some(base);
        s.utkast_gpu = Some(key);
    });
    BINDING_DRAFT.with(|b| b.set(false));
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
    required: Option<&'static str>,
}

impl LockStrip {
    /// `base`: teksten i lesevisningen (hva siden viser).
    pub fn new(app: &Rc<App>, base: &str) -> Rc<LockStrip> {
        Self::new_for(app, base, None)
    }

    pub fn new_for(app: &Rc<App>, base: &str, required: Option<&'static str>) -> Rc<LockStrip> {
        let root = hbox(8, &["lock-strip"]);
        root.set_accessible_role(gtk::AccessibleRole::Note);
        let icon = Icon::new("lock", 14);
        icon.set_valign(gtk::Align::Center);
        icon.add_css_class("ls-icon");
        let title = label(tr!("Låst"), &["ls-title"]);
        title.set_valign(gtk::Align::Center);
        let text = label(base, &["ls-text"]);
        text.set_wrap(true);
        text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let what = button(None, Some(tr!("Hva skal til?")), &["sm", "flat", "ls-what"]);
        what.set_valign(gtk::Align::Center);
        crate::widgets::tip(&what, tr!("Aktivering og støtte for kortet"), None);
        let a = app.clone();
        what.connect_clicked(move |b| what_it_takes(&a, b));
        root.append(&icon);
        root.append(&title);
        root.append(&text);
        root.append(&what);
        Rc::new(LockStrip { root, icon, title, text, what, base: base.to_string(), required })
    }

    /// `lost`: kortet svarer ikke. `drafting`: det finnes et utkast.
    pub fn sync(&self, app: &App, lost: bool, drafting: bool) {
        let sim = gate(app) == Gate::Sim;
        let (icon, title, text, kind) = if lost {
            ("unplug", tr!("Kortet svarer ikke"), tr!("Verdiene vises igjen når kortet er tilbake. Utkastet beholdes.").to_string(), "lost")
        } else if sim {
            ("circle-dot", tr!("Simulert kort"), tr!("Bruk sender utkastet til den simulerte backenden, med Behold eller Rull tilbake. Det ekte kortet røres ikke.").to_string(), "sim")
        } else if control_ready(app) && self.required.is_some_and(|f| !supports(app, f)) {
            ("lock", tr!("Utilgjengelig"), capability_reason(app, self.required.unwrap()), "")
        } else if control_ready(app) {
            ("shield", if drafting { tr!("Utkast") } else { tr!("Styring klar") }, if drafting { tr!("Endringene sendes til kortet når du trykker Bruk. Behold dem innen fristen, ellers rulles de tilbake.").into() } else { tr!("Støttede funksjoner kan endres med Bruk og Behold.").into() }, "")
        } else if drafting {
            ("pencil", tr!("Utkast · låst"), tr!("Utkastet lagres i appen og sendes ikke til kortet. Styring er ikke tilgjengelig for kortet som vises, så Bruk er låst.").into(), "draft")
        } else {
            ("lock", tr!("Låst"), helper_line(app).unwrap_or_else(|| self.base.clone()), "")
        };
        if self.icon.icon_name() != icon {
            self.icon.set_icon(icon);
        }
        set_text(&self.title, title);
        set_text(&self.text, &text);
        self.what.set_visible(!sim && !control_ready(app));
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
    let bx = vbox(8, &["steps-pop"]);
    bx.append(&label(tr!("Skjermkortstyring"), &["pop-title"]));
    let lead = label(tr!("Aktiver styring én gang med administratorpassord i Innstillinger. Funksjonene under blir tilgjengelige der kortet og driveren støtter dem."), &["pop-sub"]);
    lead.set_wrap(true);
    lead.set_max_width_chars(46);
    bx.append(&lead);
    for (key, name) in [("power", tr!("Effektgrense")), ("fan_manual", tr!("Vifter")), ("fan_curve", tr!("Viftekurve")), ("core_clock", tr!("Kjerneklokke")), ("mem_clock", tr!("Minneklokke")), ("temp_target", tr!("Temperaturmål"))] {
        let row = hbox(12, &[]);
        let title = label(name, &[]);
        title.set_hexpand(true);
        row.append(&title);
        let available = supports(app, key);
        row.append(&label(if available { tr!("Tilgjengelig") } else if matches!(helper_state(app).as_str(), "off" | "waiting") { tr!("Ikke undersøkt") } else { tr!("Utilgjengelig") }, &["muted", "sm"]));
        row.set_tooltip_text((!available).then(|| capability_reason(app, key)).as_deref());
        bx.append(&row);
    }
    let foot = label(tr!("Hver endring må beholdes innen fristen. Du kan alltid velge Rull tilbake."), &["pop-foot"]);
    foot.set_wrap(true);
    foot.set_max_width_chars(46);
    bx.append(&foot);
    let open = button(Some("settings"), Some(tr!("Åpne Innstillinger › Skjermkortstyring")), &["sm", "flat", "pop-open"]);
    bx.append(&open);
    let p = crate::widgets::popover::popover(anchor, &bx);
    p.add_css_class("pop-wide");
    let (a, pop) = (app.clone(), p.clone());
    open.connect_clicked(move |_| { pop.popdown(); a.open_settings("styring"); });
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
        /// Under hverandre: under bruddpunktet, eller når en av flatene ikke
        /// får sin minste bredde side ved side (lange ord, større skrift).
        fn stacked(&self, width: i32, main: &gtk::Widget, side: &gtk::Widget) -> bool {
            if width < self.bp.get() {
                return true;
            }
            let sw = self.side_width(width);
            let mw = width - sw - self.gap.get();
            !crate::widgets::fit::fits_pair(mw, main.measure(gtk::Orientation::Horizontal, -1).0, sw, side.measure(gtk::Orientation::Horizontal, -1).0)
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
                    if !self.stacked(w, &main, &side) {
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
            let narrow = self.stacked(width, &main, &side);
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

/// En bekreftelse tilhører kortet som mottok endringen, også etter kortbytte.
#[derive(Clone)]
struct PendingConfirmation {
    id: String,
    deadline: i64,
    origin: ControlOrigin,
    before: Value,
    wanted: Value,
}

/// Venter på autoritativ bekreftelse/rollback. Lokal frist er bare en visning.
pub struct ConfirmBar {
    pub root: gtk::Revealer,
    text: gtk::Label,
    keep: gtk::Button,
    revert: gtk::Button,
    txn: RefCell<Option<PendingConfirmation>>,
    timer: RefCell<Option<glib::SourceId>>,
    in_tick: Cell<bool>,
    failed: Cell<bool>,
    answering: Cell<bool>,
    reverting_context: Cell<bool>,
    poll_busy: Cell<bool>,
    poll_at: Cell<i64>,
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
        let revert = button(None, Some(tr!("Rull tilbake")), &["sm"]);
        let keep = button(None, Some(tr!("Behold")), &["sm", "suggested"]);
        bx.append(&revert);
        bx.append(&keep);
        root.set_child(Some(&bx));
        root.set_visible(false);
        let cb = Rc::new(ConfirmBar {
            root, text, keep: keep.clone(), revert: revert.clone(), txn: RefCell::new(None), timer: RefCell::new(None),
            in_tick: Cell::new(false), failed: Cell::new(false), answering: Cell::new(false), reverting_context: Cell::new(false),
            poll_busy: Cell::new(false), poll_at: Cell::new(0), on_done: RefCell::new(None),
        });
        let (w, a) = (Rc::downgrade(&cb), Rc::downgrade(app));
        keep.connect_clicked(move |_| { if let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) { c.answer(&a, "confirm"); } });
        let (w, a) = (Rc::downgrade(&cb), Rc::downgrade(app));
        revert.connect_clicked(move |_| { if let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) { c.answer(&a, "revert"); } });
        let (w, a) = (Rc::downgrade(&cb), Rc::downgrade(app));
        app.on_txn.connect(move |ev| {
            let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) else { return };
            let mine = c.txn.borrow().as_ref().is_some_and(|p| ev["txn"].as_str() == Some(p.id.as_str()));
            if !mine { return; }
            if ev["status"] == "failed" || ev["reason"] == "recovery_failed" {
                c.show_failure(tr!("Tilbakestilling kunne ikke bekreftes. Kortets tilstand er usikker. Prøv Rull tilbake, og se status i Innstillinger › Skjermkortstyring."));
            } else if ev["status"] == "rolled_back" {
                let why = ev["reason"].as_str().unwrap_or("revert").to_string();
                c.finish(&why);
                a.toast(crate::toast::Toast::new(tr!("Endringene er rullet tilbake")));
            }
        });
        for hub in [&app.on_devices, &app.on_service] {
            let (w, a) = (Rc::downgrade(&cb), Rc::downgrade(app));
            hub.connect(move |_| {
                if let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) {
                    if c.failed.get() { c.poll(&a); } else { c.tick(&a); }
                }
            });
        }
        cb
    }

    pub fn start(self: &Rc<Self>, app: &Rc<App>, res: &Value) {
        let Some(id) = res["txn"].as_str() else { return };
        let before = state_values(res, "confirmed");
        let wanted = state_values(res, "active");
        *self.txn.borrow_mut() = Some(PendingConfirmation {
            id: id.to_string(), deadline: rigg_core::now_ms() + res["pending"]["remainingMs"].as_i64().unwrap_or(15_000),
            origin: ControlOrigin::capture(app), before, wanted,
        });
        self.failed.set(false);
        self.answering.set(false);
        self.reverting_context.set(false);
        self.poll_at.set(0);
        self.root.remove_css_class("err");
        self.root.set_visible(true);
        self.root.set_reveal_child(true);
        self.keep.set_sensitive(true);
        self.revert.set_sensitive(true);
        self.keep.grab_focus();
        self.ensure_timer(app);
        self.tick(app);
    }

    fn ensure_timer(self: &Rc<Self>, app: &Rc<App>) {
        if self.timer.borrow().is_some() { return; }
        let (w, a) = (Rc::downgrade(self), Rc::downgrade(app));
        *self.timer.borrow_mut() = Some(glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) else { return glib::ControlFlow::Break };
            c.in_tick.set(true);
            c.tick(&a);
            c.in_tick.set(false);
            if c.txn.borrow().is_some() && !c.failed.get() { glib::ControlFlow::Continue }
            else { c.timer.borrow_mut().take(); glib::ControlFlow::Break }
        }));
    }

    fn tick(self: &Rc<Self>, app: &Rc<App>) {
        let Some(p) = self.txn.borrow().clone() else { return };
        if self.failed.get() { return; }
        let same_card = p.origin.key == control_key(app);
        if !same_card && !self.reverting_context.replace(true) {
            self.answer(app, "revert");
        }
        if self.failed.get() || self.txn.borrow().is_none() { return; }
        let left = ((p.deadline - rigg_core::now_ms()) as f64 / 1000.0).ceil().max(0.0) as i64;
        self.keep.set_sensitive(same_card && left > 0 && !self.answering.get());
        self.revert.set_sensitive(!self.answering.get());
        self.keep.set_label(&trf!("Behold ({sek})", sek = rigg_core::fmt::num(Some(left as f64), 0)));
        let message = if !same_card { tr!("Kortet er byttet. Venter på bekreftet tilbakestilling på det opprinnelige kortet …").to_string() }
            else if left == 0 { tr!("Fristen er ute. Venter på bekreftet tilbakestilling fra tjenesten …").to_string() }
            else { trf!("Behold endringene? Uten svar rulles de tilbake om {tid}.", tid = rigg_core::fmt::with_unit(rigg_core::fmt::num(Some(left as f64), 0), "s")) };
        set_text(&self.text, &message);
        self.poll(app);
    }

    fn poll(self: &Rc<Self>, app: &Rc<App>) {
        let now = rigg_core::now_ms();
        if self.poll_busy.get() || self.answering.get() || now - self.poll_at.get() < 1000 { return; }
        let Some(p) = self.txn.borrow().clone() else { return };
        self.poll_at.set(now);
        self.poll_busy.set(true);
        let (w, a) = (Rc::downgrade(self), Rc::downgrade(app));
        app.client.request("get_state", p.origin.args(json!({})), move |res| {
            let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) else { return };
            c.poll_busy.set(false);
            if !c.txn.borrow().as_ref().is_some_and(|t| t.id == p.id && t.origin.key == p.origin.key) { return; }
            let Ok(v) = res else { return }; // Behold ventestatus ved transportfeil.
            if !v["hardwareError"].is_null() || v["hardwareKnown"] == false {
                c.show_failure(tr!("Tjenesten kan ikke bekrefte kortets tilstand eller tilbakestilling. Prøv Rull tilbake, og se status i Innstillinger › Skjermkortstyring."));
                return;
            }
            if v["pending"].is_object() { return; }
            let active = state_values(&v, "active");
            if active.is_object() && active == p.before {
                c.finish("revert");
                a.toast(crate::toast::Toast::new(tr!("Endringene er rullet tilbake")));
            } else if active.is_object() && active == p.wanted && state_values(&v, "confirmed") == p.wanted {
                c.finish(if p.origin.key == control_key(&a) { "confirm" } else { "context_changed" });
            } else if c.failed.get() && active.is_object() && active == state_values(&v, "confirmed") {
                c.finish("resolved");
                a.toast(crate::toast::Toast::new(tr!("Kortets tilstand er bekreftet")).body(tr!("Ingen ubekreftet endring venter.")));
            }
        });
    }

    fn show_failure(&self, why: &str) {
        self.failed.set(true);
        self.root.add_css_class("err");
        self.root.set_visible(true);
        self.root.set_reveal_child(true);
        self.keep.set_sensitive(false);
        self.revert.set_sensitive(!self.answering.get());
        set_text(&self.text, why);
        self.text.announce(why, gtk::AccessibleAnnouncementPriority::High);
        self.stop_timer();
    }

    fn stop_timer(&self) {
        if let Some(t) = self.timer.borrow_mut().take() && !self.in_tick.get() { t.remove(); }
    }

    fn finish(&self, how: &str) {
        self.txn.borrow_mut().take();
        self.stop_timer();
        self.root.set_reveal_child(false);
        self.root.set_visible(false);
        if let Some(f) = self.on_done.borrow().as_ref() { f(how); }
    }

    fn answer(self: &Rc<Self>, app: &Rc<App>, op: &'static str) {
        if self.answering.get() { return; }
        let Some(p) = self.txn.borrow().clone() else { return };
        if op == "confirm" && (p.origin.key != control_key(app) || matches!(gate(app), Gate::Locked(_)) || rigg_core::now_ms() >= p.deadline) {
            return;
        }
        self.answering.set(true);
        self.keep.set_sensitive(false);
        self.revert.set_sensitive(false);
        let (w, a) = (Rc::downgrade(self), Rc::downgrade(app));
        // Rull tilbake bruker den opprinnelige identiteten selv om valgt kort
        // eller skriveporten har endret seg. Hjelperen validerer transaksjonen.
        app.client.request(op, p.origin.args(json!({"txn": p.id})), move |res| {
            let (Some(c), Some(a)) = (w.upgrade(), a.upgrade()) else { return };
            c.answering.set(false);
            if !c.txn.borrow().as_ref().is_some_and(|t| t.id == p.id && t.origin.key == p.origin.key) { return; }
            match res {
                Ok(v) if v["hardwareError"].is_null() && v["hardwareKnown"] != false => {
                    if p.origin.key == control_key(&a) { remember_control(&a, &v); }
                    c.finish(if op == "confirm" && p.origin.key != control_key(&a) { "context_changed" } else { op });
                    a.toast(crate::toast::Toast::new(if op == "confirm" { tr!("Endringene er beholdt") } else { tr!("Endringene er rullet tilbake") }));
                    a.on_service.emit(&());
                    refresh_control(&a, true);
                }
                Ok(_) => c.show_failure(tr!("Tjenesten kan ikke bekrefte tilbakestillingen. Kortets tilstand er usikker. Prøv Rull tilbake.")),
                Err(e) => c.show_failure(&trf!("Kunne ikke bekrefte svaret: {feil}. Prøv Rull tilbake. Endringen regnes ikke som tilbakestilt før tjenesten bekrefter det.", feil = display_error(&e))),
            }
        });
    }

    pub fn active(&self) -> bool { self.txn.borrow().is_some() }
}

fn state_values(v: &Value, field: &str) -> Value {
    let typed = &v[format!("{field}Values")];
    if typed.is_object() { typed.clone() } else { v[field].clone() }
}

impl Drop for ConfirmBar {
    fn drop(&mut self) { self.stop_timer(); }
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
    fn gamle_lysvalg_ignoreres_uten_aa_miste_profiler() {
        let stored = Stored::from_json(&json!({
            "version": 2, "rgb": {"color": "#32B487"}, "lys": {"picked": "3d"},
            "profiles": [{"name": "Stille", "values": v(180.0)}], "utkast": v(200.0), "fanAdvanced": true,
        }));
        let saved = stored.to_json();
        assert_eq!(saved["profiles"][0]["name"], "Stille");
        assert_eq!(stored.utkast, v(200.0));
        assert!(stored.fan_advanced);
        assert!(saved.get("rgb").is_none());
        assert_eq!(saved["lys"]["picked"], "3d");
    }

    #[test]
    fn viftevisning_er_enkel_som_standard_og_lagres_uten_aa_endre_utkast() {
        let legacy = Stored::from_json(&json!({"utkast": v(200.0)}));
        assert!(!legacy.fan_advanced);
        let mut advanced = legacy.clone();
        advanced.fan_advanced = true;
        let saved = Stored::from_json(&advanced.to_json());
        assert!(saved.fan_advanced);
        assert_eq!(saved.utkast, legacy.utkast);
        assert_eq!(saved.utkast_base, legacy.utkast_base);
        assert_eq!(saved.utkast_gpu, legacy.utkast_gpu);
    }

    #[test]
    fn aktivering_rebaser_bare_brukerens_endring() {
        let original = txn::Values::default();
        let draft = txn::Values { power_limit_w: 200.0, ..original.clone() };
        let current = txn::Values { core_offset_mhz: 100.0, fan_percent: 65.0, fan_hysteresis_c: 7.0, ..original.clone() };
        let rebased = rebase_draft(&original, &draft, &current);
        assert_eq!(rebased.power_limit_w, 200.0);
        assert_eq!(rebased.core_offset_mhz, 100.0);
        assert_eq!(rebased.fan_percent, 65.0);
        assert_eq!(rebased.fan_hysteresis_c, 7.0);
        assert_eq!(changed_patch(&current, &rebased), json!({"powerLimitW": 200.0}));
    }

    #[test]
    fn eldre_utkast_og_annet_kort_beholdes_men_avvises() {
        let old = Stored::from_json(&json!({"version": 1, "utkast": v(200.0)}));
        assert_eq!(old.utkast, v(200.0));
        assert!(validate_draft_binding(old, "pci-a/uuid-a", &txn::Values::default()).unwrap_err().contains("eldre"));
        let bound = Stored { utkast: v(200.0), utkast_base: Some(txn::Values::default()), utkast_gpu: Some("pci-a/uuid-a".into()), ..Default::default() };
        let roundtrip = Stored::from_json(&bound.to_json());
        assert_eq!(roundtrip.utkast_gpu, bound.utkast_gpu);
        assert_eq!(roundtrip.utkast_base, bound.utkast_base);
        assert!(validate_draft_binding(roundtrip.clone(), "pci-b/uuid-b", &txn::Values::default()).is_err());
        assert_eq!(validate_draft_binding(roundtrip, "pci-a/uuid-a", &txn::Values::default()).unwrap(), v(200.0));
    }

    #[test]
    fn opprinnelig_identitet_beholdes_i_tilbakestillingsforespoerselen() {
        let origin = ControlOrigin { key: "pci-a/uuid-a".into(), gpu: Some("pci-a".into()), uuid: Some("uuid-a".into()), real: true };
        assert_eq!(origin.args(json!({"txn": "t7"})), json!({"txn": "t7", "gpu": "pci-a", "uuid": "uuid-a"}));
        assert!(state_values(&json!({"pending": null}), "active").is_null(), "manglende tilstand er ingen bekreftelse");
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

#[cfg(test)]
mod helper_watch_tests {
    use super::*;
    #[test]
    fn autorisasjon_oppdaterer_grensesnittet_men_ikke_sensorverdier() {
        let off = json!({"state":"off", "ready":false});
        let on = json!({"state":"on", "ready":true, "enabled":true});
        assert!(helper_status_changed(&off, &on));
        assert!(helper_status_changed(&on, &off));
        let mut sample = on.clone();
        sample["hardware"] = json!({"observed":{"powerLimitW":240}});
        assert!(!helper_status_changed(&on, &sample));
        sample["capabilities"] = json!({"rgb":{"available":false}});
        assert!(helper_status_changed(&on, &sample));
    }
}

#[cfg(test)]
mod selected_capability_tests {
    use super::*;
    #[test]
    fn grenser_fra_et_annet_kort_skal_aldri_brukes_som_valgt_kort() {
        let helper = json!({"identity":{"pci":"0000:01:00.0","uuid":"GPU-a"}});
        assert!(target_matches(&helper, Some("0000:01:00.0"), Some("GPU-a")));
        assert!(!target_matches(&helper, Some("0000:02:00.0"), Some("GPU-b")));
        assert!(!target_matches(&helper, Some("0000:01:00.0"), Some("GPU-b")));
        assert!(!target_matches(&helper, Some("0000:01:00.0"), None));
        assert!(!target_matches(&Value::Null, None, None));
    }
}
