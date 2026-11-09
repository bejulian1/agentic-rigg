//! Søket i Innstillinger: hver del bygges som vanlig, og radene som ikke
//! passer, skjules. Et panel med `whole` (lister og rutenett) vises helt
//! eller ikke. Teksten det søkes i, er delens navn, panelets tittel og
//! forklaring, radens tekst og søkeordene (`k`). Alle ordene må passe.
//! Kontrollene virker i treffene, og ordene markeres i titlene.

use super::{SECTIONS, SettingsView};
use crate::widgets::{Icon, hbox, label, vbox};
use gtk::prelude::*;
use std::rc::Rc;

pub fn norm(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn fits(hay: &str, terms: &[String]) -> bool {
    let h = norm(hay);
    terms.iter().all(|t| h.contains(t.as_str()))
}

fn escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}

/// Markerer ordene i en etikett (bare tekst, ingen markup fra søket).
pub fn mark(l: &gtk::Label, terms: &[String], bg: &str) {
    let text = l.text().to_string();
    let low = text.to_lowercase();
    // Finn alle treff (byte-posisjoner i den lille teksten), slå sammen overlapp.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for t in terms {
        if t.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(i) = low[from..].find(t.as_str()) {
            let a = from + i;
            spans.push((a, a + t.len()));
            from = a + t.len().max(1);
        }
    }
    if spans.is_empty() || low.len() != text.len() {
        return;
    }
    spans.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(m) if s.0 <= m.1 => m.1 = m.1.max(s.1),
            _ => merged.push(s),
        }
    }
    let mut out = String::new();
    let mut i = 0;
    for (a, b) in merged {
        if !text.is_char_boundary(a) || !text.is_char_boundary(b) {
            return;
        }
        out.push_str(&escape(&text[i..a]));
        out.push_str(&format!("<span background=\"{bg}\">{}</span>", escape(&text[a..b])));
        i = b;
    }
    out.push_str(&escape(&text[i..]));
    l.set_markup(&out);
}

pub fn render(v: &Rc<SettingsView>, q: &str) {
    let terms: Vec<String> = norm(q).split(' ').filter(|t| !t.is_empty()).map(String::from).collect();
    let c = crate::theme::rgba(&v.app, "--accent-soft-strong");
    let bg = format!("#{:02x}{:02x}{:02x}{:02x}", (c.red() * 255.0) as u8, (c.green() * 255.0) as u8, (c.blue() * 255.0) as u8, (c.alpha() * 255.0) as u8);
    let mut total = 0;
    let mut any = false;
    for (si, s) in SECTIONS.iter().enumerate() {
        let mut hits = 0;
        for g in (s.build)(&v.app) {
            let base = format!("{} {}", s.name, g.text);
            let matched = if g.whole || g.rows.is_empty() {
                let mut all = vec![base.clone()];
                all.extend(g.rows.iter().map(|r| r.text.clone()));
                if fits(&all.join(" "), &terms) {
                    hits += 1;
                    true
                } else {
                    false
                }
            } else {
                let mut n = 0;
                for r in &g.rows {
                    if fits(&format!("{base} {}", r.text), &terms) {
                        n += 1;
                        for t in &r.titles {
                            mark(t, &terms, &bg);
                        }
                    } else {
                        r.widget.set_visible(false);
                    }
                }
                hits += n;
                n > 0
            };
            if !matched {
                continue;
            }
            mark(&g.title_label, &terms, &bg);
            // Hvor treffet står: «Utseende ›» foran panelets tittel, som en knapp dit.
            let crumb = gtk::Button::with_label(s.name);
            crumb.add_css_class("set-crumb");
            crumb.set_tooltip_text(Some(&format!("Gå til {}", s.name)));
            let sep = label("›", &["set-crumb-sep"]);
            g.head.prepend(&sep);
            g.head.prepend(&crumb);
            let me = Rc::downgrade(v);
            let id = s.id;
            let title = g.title.clone();
            crumb.connect_clicked(move |_| {
                if let Some(v) = me.upgrade() {
                    v.select(id, Some(&title));
                }
            });
            v.masonry.append(&g.widget);
            any = true;
        }
        let (_, b, _, n) = &v.tabs[si];
        n.set_text(&if hits > 0 { hits.to_string() } else { String::new() });
        n.set_visible(hits > 0);
        crate::widgets::class(b, "no-hits", hits == 0);
        total += hits;
    }
    v.heading.set_text("Søk");
    v.lead.set_text(&format!("«{q}» i alle delene"));
    v.count.set_text(&if total > 0 { format!("{total} treff") } else { "Ingen treff".into() });
    if !any {
        v.masonry.set_visible(false);
        crate::widgets::clear(&v.empty);
        v.empty.append(&Icon::new("search", 20));
        let t = vbox(4, &[]);
        t.append(&label(&format!("Ingen innstillinger passer «{q}»."), &["b"]));
        let s = label("Prøv et ord fra navnet eller forklaringen, for eksempel tema, enhet eller graf.", &[]);
        s.set_wrap(true);
        t.append(&s);
        v.empty.append(&t);
        v.empty.set_visible(true);
    }
    let _ = hbox;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alle_ordene_må_passe() {
        let t = vec!["mørk".to_string(), "tema".to_string()];
        assert!(fits("Lyst eller mørkt  System følger GNOME tema", &t));
        assert!(!fits("Lyst eller lyst", &t));
        assert_eq!(norm("  Søk   I  Innstillinger "), "søk i innstillinger");
    }
}
