//! `get_network` (Nettverk-siden): grensesnittene, rutene, DNS og socketene.
//!
//! Lista lages i en egen kortlivet tråd, så NetworkManager over
//! systembussen og gjennomgangen av `/proc/*/fd` aldri holder hovedløkka eller
//! prøvetråden. Svaret er den forrige lista; er den eldre enn [`FRESH_MS`],
//! startes en ny lesing, og den hentes ved neste kall (`pending` når ingen
//! finnes ennå). Ingenting leses uten at et vindu ber om det.
//!
//! Med `--backend sim` er nettet simulert (`RIGG_NETT=ekte` gir det ekte,
//! `RIGG_NETT=sim` gir det simulerte med andre backender), og prøvene får
//! de simulerte ratene ([`NetCache::overlay`]), så grafene og lista stemmer.

use rigg_core::net::NetInfo;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc;

/// Så gammel kan lista være før en ny lesing startes (ms).
pub const FRESH_MS: i64 = 2_500;

pub struct NetCache {
    root: PathBuf,
    sim: bool,
    started: i64,
    latest: Option<NetInfo>,
    pending: Option<mpsc::Receiver<NetInfo>>,
    /// Leseren (med navnene fra pci.ids) mellom lesingene. Lånes av tråden.
    reader: Option<rigg_hw::sys::netinfo::NetReader>,
    back: Option<mpsc::Receiver<rigg_hw::sys::netinfo::NetReader>>,
}

/// Simulert nett med `--backend sim` (eller `RIGG_NETT=sim`), ellers det ekte.
pub fn sim_wanted(backend: &str, env: Option<&str>) -> bool {
    match env {
        Some("sim") => true,
        Some("ekte") => false,
        _ => backend == "sim",
    }
}

impl NetCache {
    pub fn new(root: PathBuf, backend: &str, now: i64) -> NetCache {
        let sim = sim_wanted(backend, std::env::var("RIGG_NETT").ok().as_deref());
        let reader = Some(rigg_hw::sys::netinfo::NetReader::new(&root));
        NetCache { root, sim, started: now, latest: None, pending: None, reader, back: None }
    }

    /// De simulerte ratene i prøven.
    pub fn overlay(&self, net: &mut std::collections::BTreeMap<String, rigg_core::model::NetSample>, now: i64) {
        if self.sim {
            rigg_hw::sys::net_sim::overlay(net, self.started, now);
        }
    }

    fn collect(&mut self) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(info) => {
                    self.latest = Some(info);
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.back
            && let Ok(r) = rx.try_recv()
        {
            self.reader = Some(r);
            self.back = None;
        }
    }

    fn start(&mut self, now: i64) {
        if self.sim {
            self.latest = Some(rigg_hw::sys::net_sim::info(self.started, now));
            return;
        }
        // Leseren er fortsatt ute hos forrige tråd: vent på den.
        let Some(mut reader) = self.reader.take() else { return };
        let (tx, rx) = mpsc::channel();
        let (btx, brx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("rigg-nett".into()).spawn(move || {
            let info = reader.read(rigg_core::now_ms());
            let _ = tx.send(info);
            let _ = btx.send(reader);
        });
        match spawned {
            Ok(_) => {
                self.pending = Some(rx);
                self.back = Some(brx);
            }
            Err(_) => self.reader = Some(rigg_hw::sys::netinfo::NetReader::new(&self.root)),
        }
    }

    /// Svaret på `get_network`.
    pub fn get(&mut self, now: i64) -> Value {
        self.collect();
        let stale = self.latest.as_ref().is_none_or(|l| now - l.t > FRESH_MS);
        if stale && self.pending.is_none() {
            self.start(now);
        }
        match &self.latest {
            Some(l) => {
                let mut v = serde_json::to_value(l).unwrap_or(Value::Null);
                v["pending"] = json!(false);
                v
            }
            None => json!({"t": 0, "source": "", "ifaces": [], "pending": true}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valg_av_kilde() {
        assert!(sim_wanted("sim", None));
        assert!(!sim_wanted("nvml", None));
        assert!(!sim_wanted("sim", Some("ekte")));
        assert!(sim_wanted("ingen", Some("sim")));
    }

    #[test]
    fn simulert_svar_med_en_gang() {
        let mut c = NetCache { root: "/".into(), sim: true, started: 0, latest: None, pending: None, reader: None, back: None };
        let v = c.get(5_000);
        assert_eq!(v["pending"], false);
        assert_eq!(v["source"], "sim");
        assert_eq!(v["ifaces"][0]["name"], "enp5s0");
        let first = v["t"].clone();
        assert_eq!(c.get(6_000)["t"], first, "fersk liste gjenbrukes");
        assert_ne!(c.get(9_000)["t"], first);
    }

    #[test]
    fn ekte_lesing_i_egen_traad_fra_fixtures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rigg-hw/tests/fixtures/nett");
        let mut c = NetCache { root: root.clone(), sim: false, started: 0, latest: None, pending: None, reader: Some(rigg_hw::sys::netinfo::NetReader::new(&root)), back: None };
        assert_eq!(c.get(1_000)["pending"], true);
        let mut v = Value::Null;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            v = c.get(1_100);
            if v["pending"] == false {
                break;
            }
        }
        assert_eq!(v["source"], "sysfs");
        assert_eq!(v["hostname"], "rigg-test");
        assert_eq!(v["ifaces"][0]["name"], "wlo1");
    }
}
