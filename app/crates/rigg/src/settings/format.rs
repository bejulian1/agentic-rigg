//! Enheter og format: temperatur, minne, nettrate, desimaler og klokkeslett
//! (uendret fra demoen).

use super::{Group, GroupOpts, group, row, seg};
use crate::state::App;
use rigg_core::fmt as F;
use std::rc::Rc;

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    let now = F::time(crate::app::local_now(), false);
    vec![
        group(
            "Enheter",
            GroupOpts::default(),
            vec![
                row("Temperatur", Some("Alle sider, grafer, temperaturmålet og viftekurven."), &seg(app, &[("C", "°C"), ("F", "°F")], &s.unit, "unit", "Temperaturenhet", false).root, "celsius fahrenheit"),
                row("Minne", Some("GiB som Linux-verktøy, eller GB som produsentene."), &seg(app, &[("bin", "GiB"), ("dec", "GB")], &s.mem_unit, "memUnit", "Minneenhet", false).root, "binær desimal gigabyte"),
                row("Nettrate", Some("Byte per sekund eller bit per sekund."), &seg(app, &[("byte", "MB/s"), ("bit", "Mbit/s")], &s.net_unit, "netUnit", "Nettrate", false).root, "nett hastighet"),
            ],
        ),
        group(
            "Visning",
            GroupOpts::default(),
            vec![
                row(
                    "Desimaler",
                    Some("Temperatur, effekt og prosent i tabeller og sidefeltet. Automatisk velger etter stedet."),
                    &seg(app, &[("auto", "Automatisk"), ("0", "0"), ("1", "1")], &s.decimals, "decimals", "Desimaler", false).root,
                    "desimal komma",
                ),
                row(
                    "Klokkeslett",
                    Some(&format!("Min/maks på Sensorer, profiler og grafer. Nå: {now}.")),
                    &seg(app, &[("system", "System"), ("24", "24 t"), ("12", "12 t")], &s.clock, "clock", "Klokkeslett", false).root,
                    "tid klokke am pm",
                ),
            ],
        ),
    ]
}
