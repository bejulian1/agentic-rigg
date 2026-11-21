use super::{Page, lys_kort::CardLight};
use crate::state::App;
use crate::widgets::vbox;
use gtk::prelude::*;
use std::rc::Rc;

struct Lys {
    root: gtk::Box,
    card: Rc<CardLight>,
}

impl Page for Lys {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn show(&self, _query: Option<&str>) {
        self.card.show();
    }

    fn hide(&self) {
        self.card.hide();
    }
}

pub fn build(app: &Rc<App>) -> Box<dyn Page> {
    let root = vbox(12, &["page", "styring", "lys"]);
    let card = CardLight::new(app);
    root.append(&card.root);
    Box::new(Lys { root, card })
}
