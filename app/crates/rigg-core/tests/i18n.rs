//! Egen testprosess slik at bokmål er aktivt i de øvrige testene.

use rigg_core::disk::{self, Level, SmartAttr};
use rigg_core::{fmt, i18n};
use serde_json::json;

#[test]
fn oversettelser_og_sprakvalg() {
    let empty = rigg_core::settings::lenient(&json!({}));
    assert_eq!(empty.language, "system");
    let edited = rigg_core::settings::merge(&empty, &json!({"language": "pl"})).unwrap();
    assert_eq!(edited.to_value()["language"], "pl");

    let mut old_json = empty.to_value();
    old_json.as_object_mut().unwrap().remove("language");
    let old: rigg_core::settings::Settings = serde_json::from_value(old_json).unwrap();
    assert_eq!(old.language, "system");
    assert!(rigg_core::settings::merge(&empty, &json!({"language": "xx"})).is_err());

    i18n::init(&edited.language);
    assert_eq!(Level::Warn.text(), "Ostrzeżenie");
    for (pretty, text) in [(1, "1 sektor"), (2, "2 sektory"), (12, "12 sektorów")] {
        let attr = SmartAttr { pretty, unit: 3, ..Default::default() };
        assert_eq!(disk::attr_text(&attr, "", None), text);
    }
    assert_eq!(fmt::watt(Some(210.3), 1), "210,3 W");
}
