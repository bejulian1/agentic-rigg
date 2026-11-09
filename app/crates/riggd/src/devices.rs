//! Tilstanden til hvert kort (produktspesifikasjonen 3.8). Hendelsen `device`
//! sendes når et kort kommer til, forsvinner eller bytter tilstand.

use rigg_core::model::{DeviceState, GpuInfo};

#[derive(Default)]
pub struct Devices {
    pub list: Vec<GpuInfo>,
    pub suspended: bool,
}

impl Devices {
    /// Tar inn listen fra prøvetråden. Gir kortene som er endret.
    pub fn update(&mut self, infos: Vec<GpuInfo>) -> Vec<GpuInfo> {
        let mut changed = Vec::new();
        for g in &infos {
            let old = self.list.iter().find(|o| o.id == g.id);
            if old.is_none_or(|o| o.state != g.state || o.detail != g.detail) {
                changed.push(g.clone());
            }
        }
        for o in &self.list {
            if !infos.iter().any(|g| g.id == o.id) {
                let mut gone = o.clone();
                gone.state = DeviceState::Lost;
                changed.push(gone);
            }
        }
        self.list = infos;
        changed
    }

    /// Listen slik klientene ser den (med `suspended` under dvale).
    pub fn view(&self) -> Vec<GpuInfo> {
        self.list
            .iter()
            .map(|g| {
                let mut g = g.clone();
                if self.suspended && g.state == DeviceState::Ok {
                    g.state = DeviceState::Suspended;
                }
                g
            })
            .collect()
    }

    pub fn first_id(&self) -> Option<String> {
        self.list.iter().find(|g| !g.id.is_empty()).map(|g| g.id.clone())
    }

    pub fn known(&self, id: &str) -> bool {
        !id.is_empty() && self.list.iter().any(|g| g.id == id)
    }
}
