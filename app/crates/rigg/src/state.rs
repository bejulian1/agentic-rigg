//! `App`: den ene felles tilstanden i vinduet (arkitektur.md 6.2), delt som
//! `Rc<App>`. Siste prøve, statiske data, statistikk, innstillinger og
//! tilstanden til tjenesten. Sidene abonnerer på endringer med enkle
//! callbacks ([`Hub`]). Ingen rammeverk, ingen tråder.
//!
//! **API for sidene (runde 2):**
//! - `app.on_telemetry.connect(|_| …)`: ny prøve (1 s). Les med
//!   `app.gpu_sample()`, `app.sys()`, `app.value(key)` og `app.recent(key)`.
//! - `app.on_settings.connect(|keys| …)`: innstillinger endret (nøklene som
//!   endret seg, f.eks. `"unit"`, `"thresholds"`).
//! - `app.on_theme.connect(|_| …)`: farger eller skrift endret (tegn på nytt).
//! - `app.on_devices`, `app.on_stats`, `app.on_conn`, `app.on_log`,
//!   `app.on_alarm`, `app.on_txn`, `app.on_service`.
//! - `app.set(json!({"unit": "F"}))`: endre innstillinger (lagres etter 500 ms).
//! - `app.client.request(op, args, |res| …)`: snakk med tjenesten.
//! - `app.toast(Toast::new("Tittel").body("…"))`, `app.go("sensorer")`,
//!   `app.open_settings("sidefelt")`.

use rigg_core::tr;
use crate::client::Client;
use rigg_core::model::{GpuInfo, GpuSample, GpuStatic, SysSample, Telemetry};
use rigg_core::paths::Profile;
use rigg_core::settings::Settings;
use serde_json::Value;
use std::cell::{OnceCell, Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// En enkel signal-hub: lyttere kalles i rekkefølge. Nye lyttere kan legges
/// til mens en hendelse sendes.
pub struct Hub<T> {
    subs: crate::widgets::Listeners<dyn Fn(&T)>,
}

impl<T> Default for Hub<T> {
    fn default() -> Self {
        Hub { subs: RefCell::new(Vec::new()) }
    }
}

impl<T> Hub<T> {
    pub fn connect(&self, f: impl Fn(&T) + 'static) {
        self.subs.borrow_mut().push(Rc::new(f));
    }

    pub fn emit(&self, v: &T) {
        let subs = self.subs.borrow().clone();
        for s in subs {
            s(v);
        }
    }
}

/// Forbindelsen til tjenesten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    /// Første forsøk (ingen linje før 300 ms).
    Connecting,
    Connected,
    /// Brutt eller ikke funnet (ms da det skjedde).
    Lost(i64),
}

/// Hvor lenge sparklinjene ser tilbake (ms).
pub const RECENT_MS: i64 = 62_000;

#[derive(Default)]
pub struct Data {
    pub telemetry: Option<Rc<Telemetry>>,
    pub values: HashMap<String, f64>,
    pub recent: HashMap<String, VecDeque<(i64, f64)>>,
    pub gpus: Vec<GpuInfo>,
    /// Kortet som er valgt i kortvelgeren.
    pub gpu: Option<String>,
    pub statics: HashMap<String, (GpuStatic, bool)>,
    pub stats: Value,
    pub hello: Option<Value>,
    pub ping: Option<Value>,
    pub service: Value,
    pub log: Value,
    pub backend: String,
}

/// Kortet som skal være valgt etter en prøve. Et kort som mangler i prøven,
/// er ikke borte fra valget: `lost` er en egen tilstand som sidene viser, og
/// kortet står fortsatt i lista fra tjenesten. Bare når valget ikke finnes i
/// lista heller (eller ingenting er valgt), velges første kort med prøve,
/// ellers første kort i lista.
pub fn pick_gpu<'a>(current: Option<&str>, sampled: impl Iterator<Item = &'a str> + Clone, listed: &[&str]) -> Option<String> {
    if let Some(c) = current
        && (sampled.clone().any(|g| g == c) || listed.contains(&c))
    {
        return Some(c.to_string());
    }
    sampled.clone().next().or_else(|| listed.iter().copied().find(|g| !g.is_empty())).map(str::to_string)
}

pub struct App {
    pub gtk: gtk::Application,
    pub profile: Profile,
    pub settings: RefCell<Settings>,
    pub data: RefCell<Data>,
    pub client: Rc<Client>,
    pub conn: std::cell::Cell<Conn>,
    pub started: std::time::Instant,
    /// Siden som vises (eller «innstillinger»).
    pub page: RefCell<String>,
    pub on_telemetry: Hub<()>,
    pub on_settings: Hub<Vec<String>>,
    pub on_theme: Hub<()>,
    pub on_devices: Hub<()>,
    pub on_stats: Hub<()>,
    pub on_conn: Hub<Conn>,
    pub on_log: Hub<Value>,
    pub on_alarm: Hub<Value>,
    /// `txn` fra tjenesten (bare med den simulerte backenden): en endring er
    /// beholdt eller rullet tilbake (`{txn, status, reason}`).
    pub on_txn: Hub<Value>,
    /// tjeneste.json eller `ping` er endret (Innstillinger › Generelt og Om).
    pub on_service: Hub<()>,
    pub on_page: Hub<String>,
    pub shell: OnceCell<Rc<crate::window::Shell>>,
    pub(crate) save_timer: RefCell<Option<gtk::glib::SourceId>>,
    /// Det som sist ble skrevet til (eller lest fra) `ui.json`: lik tekst skrives ikke igjen.
    pub(crate) saved_text: RefCell<String>,
    pub(crate) notes: RefCell<Vec<String>>,
}

impl App {
    pub fn new(gtk: gtk::Application, profile: Profile, settings: Settings, client: Rc<Client>) -> Rc<App> {
        Rc::new(App {
            gtk,
            profile,
            settings: RefCell::new(settings),
            data: RefCell::new(Data::default()),
            client,
            conn: std::cell::Cell::new(Conn::Connecting),
            started: std::time::Instant::now(),
            page: RefCell::new(String::new()),
            on_telemetry: Hub::default(),
            on_settings: Hub::default(),
            on_theme: Hub::default(),
            on_devices: Hub::default(),
            on_stats: Hub::default(),
            on_conn: Hub::default(),
            on_log: Hub::default(),
            on_alarm: Hub::default(),
            on_txn: Hub::default(),
            on_service: Hub::default(),
            on_page: Hub::default(),
            shell: OnceCell::new(),
            save_timer: RefCell::new(None),
            saved_text: RefCell::new(String::new()),
            notes: RefCell::new(Vec::new()),
        })
    }

    pub fn settings(&self) -> Ref<'_, Settings> {
        self.settings.borrow()
    }

    pub fn shell(&self) -> &Rc<crate::window::Shell> {
        self.shell.get().expect("skallet er ikke bygd")
    }

    // ---------------------------------------------------------------- Prøvene

    /// Tar inn en `telemetry`-hendelse.
    pub fn ingest(&self, t: Telemetry) {
        let flat = rigg_core::sensor_key::flatten(&t);
        {
            let mut d = self.data.borrow_mut();
            d.values.clear();
            let cut = t.t - RECENT_MS;
            for (k, v) in &flat {
                d.values.insert(k.clone(), *v);
                let q = d.recent.entry(k.clone()).or_default();
                q.push_back((t.t, *v));
                while q.front().is_some_and(|p| p.0 < cut) {
                    q.pop_front();
                }
            }
            let listed: Vec<&str> = d.gpus.iter().map(|g| g.id.as_str()).collect();
            d.gpu = pick_gpu(d.gpu.as_deref(), t.gpus.keys().map(String::as_str), &listed);
            d.telemetry = Some(Rc::new(t));
        }
        self.on_telemetry.emit(&());
    }

    /// Fyller sparklinjene fra historikken (etter tilkobling).
    pub fn prefill(&self, key: &str, pts: Vec<(i64, f64)>) {
        let mut d = self.data.borrow_mut();
        let q = d.recent.entry(key.to_string()).or_default();
        let first = q.front().map(|p| p.0).unwrap_or(i64::MAX);
        let mut merged: VecDeque<(i64, f64)> = pts.into_iter().filter(|p| p.0 < first).collect();
        merged.extend(q.drain(..));
        *q = merged;
    }

    pub fn telemetry(&self) -> Option<Rc<Telemetry>> {
        self.data.borrow().telemetry.clone()
    }

    /// Prøven for kortet som er valgt.
    pub fn gpu_sample(&self) -> Option<GpuSample> {
        let d = self.data.borrow();
        let t = d.telemetry.as_ref()?;
        let id = d.gpu.as_ref()?;
        t.gpus.get(id).cloned()
    }

    pub fn sys(&self) -> Option<SysSample> {
        self.data.borrow().telemetry.as_ref().map(|t| t.sys.clone())
    }

    pub fn gpu_id(&self) -> Option<String> {
        self.data.borrow().gpu.clone()
    }

    /// Bytter kortet som vises og styres (maskinhodet og Styrer-linja).
    pub fn select_gpu(&self, id: &str) {
        self.data.borrow_mut().gpu = Some(id.to_string());
        self.on_devices.emit(&());
        self.on_telemetry.emit(&());
    }

    /// Sensornøkkel for et felt på valgt kort: `gpu:0000:1c:00.0/temp`.
    pub fn gpu_key(&self, field: &str) -> Option<String> {
        self.gpu_id().map(|id| rigg_core::sensor_key::gpu(&id, field))
    }

    /// Siste verdi for en sensornøkkel.
    pub fn value(&self, key: &str) -> Option<f64> {
        self.data.borrow().values.get(key).copied()
    }

    /// Siste minutt for en sensornøkkel, eldst først.
    pub fn recent(&self, key: &str) -> Vec<(i64, f64)> {
        self.data.borrow().recent.get(key).map(|q| q.iter().copied().collect()).unwrap_or_default()
    }

    /// Leser de faste dataene for kortet uten å kopiere dem (grafene spør
    /// ved hver tegning; `extra` kan være stor).
    pub fn with_statics<R>(&self, f: impl FnOnce(&GpuStatic) -> R) -> Option<R> {
        let d = self.data.borrow();
        let id = d.gpu.as_ref()?;
        d.statics.get(id).map(|s| f(&s.0))
    }

    pub fn statics(&self) -> Option<GpuStatic> {
        let d = self.data.borrow();
        let id = d.gpu.as_ref()?;
        d.statics.get(id).map(|s| s.0.clone())
    }

    pub fn gpu_info(&self) -> Option<GpuInfo> {
        let d = self.data.borrow();
        match &d.gpu {
            Some(id) => d.gpus.iter().find(|g| &g.id == id).cloned(),
            None => d.gpus.first().cloned(),
        }
    }

    // ---------------------------------------------------------------- Innstillinger

    /// Endrer innstillinger. Ugyldige verdier gir et varsel og ingen endring.
    pub fn set(self: &Rc<Self>, patch: Value) -> bool {
        let before = self.settings.borrow().clone();
        let next = match rigg_core::settings::merge(&before, &patch) {
            Ok(n) => n,
            Err(e) => {
                self.toast(crate::toast::Toast::new(tr!("Kunne ikke lagre innstillingen")).body(&e.0).error());
                return false;
            }
        };
        self.replace_settings(next);
        // Det tjenesten bruker, sendes dit også (tjeneste.json).
        let mut svc = serde_json::Map::new();
        if let Some(o) = patch.as_object() {
            for (k, v) in o {
                match k.as_str() {
                    "thresholds" => {
                        svc.insert("thresholds".into(), serde_json::to_value(&self.settings().thresholds).unwrap_or(Value::Null));
                    }
                    "sensorKeepMinMax" => {
                        svc.insert("keepMinMax".into(), v.clone());
                    }
                    "confirmSec" => {
                        svc.insert("confirmSec".into(), v.clone());
                    }
                    _ => {}
                }
            }
        }
        if !svc.is_empty() {
            self.set_service(Value::Object(svc));
        }
        true
    }

    /// Bytter alle innstillingene (import, tilbakestilling, angre).
    pub fn replace_settings(self: &Rc<Self>, next: Settings) {
        let before = self.settings.borrow().clone();
        if before == next {
            return;
        }
        let (bv, nv) = (before.to_value(), next.to_value());
        let keys: Vec<String> = nv.as_object().map(|o| o.keys().filter(|k| bv.get(k.as_str()) != o.get(k.as_str())).cloned().collect()).unwrap_or_default();
        *self.settings.borrow_mut() = next;
        rigg_core::fmt::set_format(rigg_core::fmt::Format::from_settings(&self.settings()));
        crate::prefs::schedule_save(self);
        if keys.iter().any(|k| crate::theme::THEME_KEYS.contains(&k.as_str())) {
            crate::theme::apply(self);
        }
        if keys.iter().any(|k| k == "intervalMs") {
            let iv = self.settings().interval_ms;
            self.client.request("subscribe", serde_json::json!({"topics": crate::client::TOPICS, "intervalMs": iv}), |_| {});
        }
        self.on_settings.emit(&keys);
    }

    /// Endrer `tjeneste.json` via tjenesten.
    pub fn set_service(self: &Rc<Self>, patch: Value) {
        let app = self.clone();
        self.client.request("set_settings", patch, move |res| match res {
            Ok(v) => {
                app.data.borrow_mut().service = v;
                app.on_service.emit(&());
            }
            Err(e) => app.toast(crate::toast::Toast::new(tr!("Tjenesten tok ikke imot endringen")).body(&e.message).error()),
        });
    }

    /// Et påskeegg er funnet (lagres i `eggsFound`). Gir `true` første gang.
    pub fn egg_found(self: &Rc<Self>, id: &str) -> bool {
        if self.settings().eggs_found.iter().any(|e| e == id) {
            return false;
        }
        let mut list = self.settings().eggs_found.clone();
        list.push(id.to_string());
        self.set(serde_json::json!({"eggsFound": list}));
        true
    }

    // ---------------------------------------------------------------- Skallet

    pub fn toast(&self, t: crate::toast::Toast) {
        if let Some(s) = self.shell.get() {
            s.toasts.show(t);
        }
    }

    /// Går til en side (`oversikt` … `lys`).
    pub fn go(&self, page: &str) {
        if let Some(s) = self.shell.get() {
            s.go(page);
        }
    }

    /// Åpner en del av Innstillinger.
    pub fn open_settings(&self, section: &str) {
        if let Some(s) = self.shell.get() {
            s.open_settings(Some(section));
        }
    }

    pub fn window(&self) -> Option<gtk::ApplicationWindow> {
        self.shell.get().map(|s| s.window.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::pick_gpu;

    const A: &str = "0000:1c:00.0";
    const B: &str = "0000:2d:00.0";

    #[test]
    fn kortet_som_er_borte_beholder_valget() {
        // Kortet faller av: det mangler i prøven, men står i lista som `lost`.
        assert_eq!(pick_gpu(Some(A), [].into_iter(), &[A]).as_deref(), Some(A));
        // To kort, det valgte er borte: valget står, det andre tas ikke.
        assert_eq!(pick_gpu(Some(B), [A].into_iter(), &[A, B]).as_deref(), Some(B));
        // Ingenting valgt: første med prøve, ellers første i lista.
        assert_eq!(pick_gpu(None, [B].into_iter(), &[A, B]).as_deref(), Some(B));
        assert_eq!(pick_gpu(None, [].into_iter(), &[A]).as_deref(), Some(A));
        // Uten driver er lista ett element med tom ID: ingenting å velge.
        assert_eq!(pick_gpu(None, [].into_iter(), &[""]), None);
        // Et kort som ikke finnes noe sted lenger (fjernet fra lista): første med prøve.
        assert_eq!(pick_gpu(Some(B), [A].into_iter(), &[A]).as_deref(), Some(A));
    }
}
