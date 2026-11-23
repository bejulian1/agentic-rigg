//! Svensk i en egen testprosess: språket velges én gang per prosess.

use rigg_core::disk::{self, Level, SmartAttr};
use rigg_core::{fmt, i18n, tr, trf, trn};
use serde_json::json;

#[test]
fn svensk_oversettelse_flertall_og_format() {
    let empty = rigg_core::settings::lenient(&json!({}));
    let edited = rigg_core::settings::merge(&empty, &json!({"language": "sv"})).unwrap();
    assert_eq!(edited.to_value()["language"], "sv");
    assert_eq!(i18n::Language::from_code("sv").map(i18n::Language::native_name), Some("Svenska"));
    assert_eq!(i18n::resolve("sv"), i18n::Language::Sv);

    i18n::init(&edited.language);
    assert_eq!(i18n::active(), i18n::Language::Sv);
    assert_eq!(Level::Warn.text(), "Varning");
    // Entall bare for nøyaktig 1; 0 og 21 er flertall.
    for (pretty, text) in [(0, "0 sektorer"), (1, "1 sektor"), (2, "2 sektorer"), (21, "21 sektorer")] {
        let attr = SmartAttr { pretty, unit: 3, ..Default::default() };
        assert_eq!(disk::attr_text(&attr, "", None), text);
    }
    // Desimalkomma, smalt hardt mellomrom som tusenskille og foran enheten.
    assert_eq!(fmt::watt(Some(210.3), 1), "210,3\u{202F}W");
    assert_eq!(fmt::num(Some(12_345.6), 1), "12\u{202F}345,6");
    assert_eq!(fmt::pct(Some(38.0), 0), "38\u{202F}%");
    assert_eq!(fmt::date(2026, 11, 7, true), "2026-11-07");
    assert_eq!(fmt::month_day(11, 7), "7/11");
    assert_eq!(fmt::uptime(Some(90_061.0)), "1\u{202F}d 1\u{202F}h 1\u{202F}min");
    // Tekst som ikke finnes i katalogen, faller tilbake til bokmål.
    assert_eq!(tr!("Finnes ikke i noen katalog"), "Finnes ikke i noen katalog");
    assert_eq!(trf!("Ukjent {verdi}", verdi = 3), "Ukjent 3");
    assert_eq!(trn!(2, "ukjent ting", "ukjente ting"), "ukjente ting");
}
