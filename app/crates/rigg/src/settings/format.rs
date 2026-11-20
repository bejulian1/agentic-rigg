//! Enheter og format: temperatur, minne, nettrate, desimaler og klokkeslett
//! (uendret fra demoen).

use super::{Group, GroupOpts, group, kw, row, seg};
use crate::state::App;
use rigg_core::fmt as F;
use rigg_core::{tr, trf};
use std::rc::Rc;

pub fn build(app: &Rc<App>) -> Vec<Group> {
    let s = app.settings().clone();
    let now = F::time(crate::app::local_now(), false);
    vec![
        group(
            tr!("Enheter"),
            GroupOpts::default(),
            vec![
                row(
                    tr!("Temperatur"),
                    Some(tr!("Alle sider, grafer, temperaturmålet og viftekurven.")),
                    &seg(app, &[("C", "°C"), ("F", "°F")], &s.unit, "unit", tr!("Temperaturenhet"), false).root,
                    &kw("celsius fahrenheit", tr!("søkeord", "celsius fahrenheit")),
                ),
                row(
                    tr!("Minne"),
                    Some(tr!("GiB som Linux-verktøy, eller GB som produsentene.")),
                    &seg(app, &[("bin", "GiB"), ("dec", "GB")], &s.mem_unit, "memUnit", tr!("Minneenhet"), false).root,
                    &kw("binær desimal gigabyte", tr!("søkeord", "binær desimal gigabyte")),
                ),
                row(
                    tr!("Nettrate"),
                    Some(tr!("Byte per sekund eller bit per sekund.")),
                    &seg(app, &[("byte", "MB/s"), ("bit", "Mbit/s")], &s.net_unit, "netUnit", tr!("Nettrate"), false).root,
                    &kw("nett hastighet", tr!("søkeord", "nett hastighet")),
                ),
            ],
        ),
        group(
            tr!("Visning"),
            GroupOpts::default(),
            vec![
                row(
                    tr!("Desimaler"),
                    Some(tr!("Temperatur, effekt og prosent i tabeller og sidefeltet. Automatisk velger etter stedet.")),
                    &seg(app, &[("auto", tr!("Automatisk")), ("0", "0"), ("1", "1")], &s.decimals, "decimals", tr!("Desimaler"), false).root,
                    &kw("desimal komma", tr!("søkeord", "desimal komma")),
                ),
                row(
                    tr!("Klokkeslett"),
                    Some(&trf!("Min/maks på Sensorer, profiler og grafer. Nå: {na}.", na = now)),
                    &seg(app, &[("system", tr!("System")), ("24", tr!("24 t")), ("12", tr!("12 t"))], &s.clock, "clock", tr!("Klokkeslett"), false).root,
                    &kw("tid klokke am pm", tr!("søkeord", "tid klokke am pm")),
                ),
            ],
        ),
    ]
}
