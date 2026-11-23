//! Private skjermbilder av Nettverk (`RIGG_PLAN=nettverk`): sim gir
//! simulerte grensesnitt med dokumentasjonsadresser; `--backend ingen` med
//! `RIGG_NETT=ekte` viser maskinens egne data uten NetworkManager (økta har
//! en tom systembuss). Tre bredder, lyst og mørkt, og den sammenfoldede
//! gruppen med virtuelle grensesnitt åpnet.
use super::Step;
use crate::state::App;
use gtk::prelude::*;
use std::rc::Rc;

fn toggle_virtual(app: &Rc<App>) {
    let win = app.window().expect("vindu");
    match super::find_class(win.upcast_ref(), "nw-virt-toggle").and_then(|w| w.downcast::<gtk::Button>().ok()) {
        Some(b) => b.emit_clicked(),
        None => eprintln!("RIGG_SKJERMBILDER fant ikke knappen for virtuelle grensesnitt"),
    }
}

pub fn plan() -> Vec<Step> {
    use Step::*;
    let mut v = vec![Set(super::base()), Size(1920, 1080), Wait(800), Size(1920, 1080), Go("nettverk"), Wait(4000)];
    for (w, h, tag) in [(1920, 1080, "1920"), (1280, 820, "1280"), (1024, 700, "1024")] {
        v.extend([Size(w, h), Wait(1200), Size(w, h), Wait(1200), Scroll(0.0), Wait(300)]);
        v.push(Shot(match tag {
            "1920" => "nettverk-1920-lys",
            "1280" => "nettverk-1280-lys",
            _ => "nettverk-1024-lys",
        }));
        v.extend([Scroll(1.0), Wait(400)]);
        v.push(Shot(match tag {
            "1920" => "nettverk-1920-lys-bunn",
            "1280" => "nettverk-1280-lys-bunn",
            _ => "nettverk-1024-lys-bunn",
        }));
    }
    v.extend([
        Size(1280, 820),
        Wait(1000),
        Eval(toggle_virtual),
        Wait(600),
        Scroll(1.0),
        Wait(400),
        Shot("nettverk-1280-virtuelle-lys"),
        Set(serde_json::json!({"theme": "dark"})),
        Wait(800),
        Scroll(0.0),
        Wait(300),
        Shot("nettverk-1280-mork"),
        Scroll(1.0),
        Wait(300),
        Shot("nettverk-1280-mork-bunn"),
    ]);
    v
}
