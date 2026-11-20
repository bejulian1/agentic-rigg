//! Språkvalget lagres i ui.json. En ny prosess leser det før første bilde.

use super::{Group, GroupOpts, group, row_wrap, save};
use crate::state::App;
use gtk::prelude::*;
use rigg_core::i18n::{self, Language};
use rigg_core::{tr, trf};
use serde_json::json;
use std::rc::Rc;

pub fn build(app: &Rc<App>) -> Group {
    let names: Vec<_> = std::iter::once(tr!("Følg systemet"))
        .chain(Language::ALL.map(Language::native_name))
        .collect();
    let codes: Vec<_> = std::iter::once(i18n::SYSTEM)
        .chain(Language::ALL.map(Language::code))
        .collect();
    let picker = gtk::DropDown::from_strings(&names);
    let saved = codes.iter().position(|code| *code == app.settings().language).unwrap_or(0);
    picker.set_selected(saved as u32);
    picker.set_valign(gtk::Align::Center);
    let language = row_wrap(tr!("Visningsspråk"), None, &picker, "");
    picker.update_relation(&[gtk::accessible::Relation::LabelledBy(&[language.titles[0].upcast_ref()])]);

    let restart = crate::widgets::button(Some("rotate-ccw"), Some(tr!("Start på nytt nå")), &["suggested"]);
    let action = row_wrap(tr!("Nytt språk etter omstart"), None, &restart, "");
    let pending = i18n::resolve(&app.settings().language) != i18n::active();
    action.widget.set_visible(pending);
    crate::widgets::class(&language.widget, "last", !pending);

    let a = app.clone();
    let prompt = action.widget.clone();
    let language_row = language.widget.downgrade();
    picker.connect_selected_notify(move |picker| {
        let Some(code) = codes.get(picker.selected() as usize) else { return };
        if save(&a, json!({"language": code})) {
            let pending = i18n::resolve(code) != i18n::active();
            prompt.set_visible(pending);
            if let Some(row) = language_row.upgrade() {
                crate::widgets::class(&row, "last", !pending);
            }
        }
    });
    let a = app.clone();
    restart.connect_clicked(move |_| {
        if let Err(error) = crate::app::restart(&a) {
            a.toast(crate::toast::Toast::new(tr!("Kunne ikke starte appen på nytt"))
                .body(&trf!("Omstarten ble avbrutt: {feil}", feil = error))
                .error());
        }
    });

    group(tr!("Språk"), GroupOpts::default(), vec![language, action])
}
