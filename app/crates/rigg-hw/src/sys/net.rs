//! Nett: `rx_bytes`/`tx_bytes` per kort i `/sys/class/net`, som rater i byte/s.
//! `lo` tas ikke med. Virtuelle kort er med, men telles ikke i summen
//! (`sensormeta::net_totals`).

use crate::PreadFile;
use rigg_core::model::NetSample;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct Iface {
    rx: Option<PreadFile>,
    tx: Option<PreadFile>,
    oper: Option<PreadFile>,
    wireless: bool,
    prev: Option<(f64, f64, i64)>,
}

pub struct Net {
    dir: PathBuf,
    ifaces: BTreeMap<String, Iface>,
}

impl Net {
    pub fn new(root: &Path) -> Net {
        Net { dir: root.join("sys/class/net"), ifaces: BTreeMap::new() }
    }

    /// Finner nye og fjerner borte kort (hvert 10. s).
    pub fn scan(&mut self) {
        let names: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n != "lo").collect())
            .unwrap_or_default();
        self.ifaces.retain(|k, _| names.contains(k));
        for n in names {
            if self.ifaces.contains_key(&n) {
                continue;
            }
            let d = self.dir.join(&n);
            let open = |f: &str| PreadFile::open(&d.join(f)).ok();
            self.ifaces.insert(
                n,
                Iface {
                    rx: open("statistics/rx_bytes"),
                    tx: open("statistics/tx_bytes"),
                    oper: open("operstate"),
                    wireless: d.join("wireless").is_dir() || d.join("phy80211").exists(),
                    prev: None,
                },
            );
        }
    }

    pub fn sample(&mut self, now_ms: i64) -> BTreeMap<String, NetSample> {
        let mut out = BTreeMap::new();
        for (name, i) in &mut self.ifaces {
            let rx = i.rx.as_mut().and_then(|f| f.read_f64());
            let tx = i.tx.as_mut().and_then(|f| f.read_f64());
            let state = i.oper.as_mut().and_then(|f| f.read(32).ok().map(|s| s.trim().to_string()));
            let (mut rrx, mut rtx) = (None, None);
            if let (Some(r), Some(t)) = (rx, tx) {
                if let Some((pr, pt, pt0)) = i.prev
                    && now_ms > pt0
                {
                    let dt = (now_ms - pt0) as f64 / 1000.0;
                    rrx = Some(((r - pr) / dt).max(0.0));
                    rtx = Some(((t - pt) / dt).max(0.0));
                }
                i.prev = Some((r, t, now_ms));
            }
            out.insert(name.clone(), NetSample { rx: rrx, tx: rtx, state, wireless: i.wireless });
        }
        out
    }
}
