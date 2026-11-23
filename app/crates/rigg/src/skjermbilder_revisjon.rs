//! Layoutrevisjon: hver side og hver del av Innstillinger i flere
//! vindusbredder, tettheter og skriftstørrelser, mot `--backend sim` i en
//! privat økt. Språket velges med `RIGG_BILDER_SPRAK` (bilder.sh).
//!
//! Hvert bilde følges av en maskinell kontroll av det levende widgettreet
//! ([`findings`]): barn som stikker ut av forelderen, og søsken som
//! overlapper. Funnene skrives til `funn.txt` ved siden av bildene. Kontrollen
//! erstatter ikke å se på bildene; den peker ut hvor man skal se først.
//! `KLIPT` er innhold som stikker ut av en flate som klipper, `VINDU` er
//! innhold som holder vinduet bredere enn bredden planen ba om, og `BRUTT`
//! er en kort tekst som står på flere linjer selv om den får plass på én.
//!
//! - `RIGG_REVISJON_BREDDER`: `640x760,1024x700,1280x820,1920x1080` (standard)
//! - `RIGG_REVISJON_VARIANTER`: `standard:100,compact:100,comfortable:100,standard:116`
//! - `RIGG_REVISJON_SIDER`: kommaliste med sider og `innstillinger-<del>` (standard: alle)
//! - `RIGG_REVISJON_TEMA`: `dark` (standard) eller `light`

use super::*;

fn list(var: &str, default: &str) -> Vec<String> {
    std::env::var(var).unwrap_or_else(|_| default.into()).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn plan() -> Vec<Step> {
    use Step::*;
    let sizes: Vec<(i32, i32)> = list("RIGG_REVISJON_BREDDER", "1920x1080,1280x820,1024x700,640x760").iter().filter_map(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))).collect();
    let variants: Vec<(String, u32)> =
        list("RIGG_REVISJON_VARIANTER", "standard:100,compact:100,comfortable:100,standard:116").iter().filter_map(|s| s.split_once(':').and_then(|(d, f)| Some((d.to_string(), f.parse().ok()?)))).collect();
    let only = list("RIGG_REVISJON_SIDER", "");
    let theme = std::env::var("RIGG_REVISJON_TEMA").unwrap_or_else(|_| "dark".into());
    let wanted = |id: &str| only.is_empty() || only.iter().any(|o| o == id);
    let mut v = vec![Size(1920, 1080), Set(base()), Set(json!({"theme": theme}))];
    let mut first = true;
    for (density, font) in &variants {
        v.push(Set(json!({"density": density, "fontScale": font})));
        for (w, h) in &sizes {
            // To ganger: sidefeltet blir ikonlist først under 860 px, og først
            // da kan vinduet bli smalere.
            v.extend([Size(*w, *h), Wait(500), Size(*w, *h), Wait(500)]);
            let tag = format!("{w}-{density}-{font}");
            for p in PAGES {
                if !wanted(p) {
                    continue;
                }
                v.push(Go(p));
                v.push(Wait(if first { if p == "lys" { 2500 } else { 1500 } } else { 500 }));
                v.push(Shot(name(format!("{p}-{tag}"))));
                v.extend([Scroll(1.0), Wait(250), ShotScrolled(name(format!("{p}-{tag}-bunn"))), Scroll(0.0)]);
            }
            first = false;
            let mut open = false;
            for s in crate::settings::SECTIONS.iter() {
                let id = format!("innstillinger-{}", s.id);
                if !wanted(&id) {
                    continue;
                }
                open = true;
                v.push(Settings(s.id));
                v.push(Wait(450));
                v.push(Shot(name(format!("{id}-{tag}"))));
                v.extend([Scroll(1.0), Wait(250), ShotScrolled(name(format!("{id}-{tag}-bunn"))), Scroll(0.0)]);
            }
            if open {
                v.push(CloseSettings);
                v.push(Wait(200));
            }
        }
    }
    v.push(Set(base()));
    v
}

fn drawable(w: &gtk::Widget) -> bool {
    w.is_visible() && w.is_mapped() && w.is_child_visible()
}

/// Første tekst inni flaten, så funnet kan kjennes igjen.
fn first_text(w: &gtk::Widget) -> Option<String> {
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        let t = l.text();
        if !t.trim().is_empty() {
            return Some(t.chars().take(28).collect());
        }
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        if drawable(&ch)
            && let Some(t) = first_text(&ch)
        {
            return Some(t);
        }
        c = ch.next_sibling();
    }
    None
}

fn describe(w: &gtk::Widget) -> String {
    let classes = w.css_classes().iter().map(|c| c.to_string()).collect::<Vec<_>>().join(".");
    let text = first_text(w).map(|t| format!(" «{t}»")).unwrap_or_default();
    format!("{}{}{}{text}", w.css_name(), if classes.is_empty() { "" } else { "." }, classes)
}

/// Foreldre som med vilje legger barn oppå hverandre eller utenfor seg selv,
/// og GTKs egne innmat (glidere, brytere, rullefelt).
fn layered(w: &gtk::Widget) -> bool {
    w.is::<gtk::Overlay>()
        || w.is::<gtk::Stack>()
        || w.is::<gtk::ScrolledWindow>()
        || w.is::<gtk::Viewport>()
        || w.is::<gtk::Popover>()
        || w.is::<gtk::Window>()
        || w.is::<gtk::Revealer>()
        || w.is::<gtk::Range>()
        || w.is::<gtk::Switch>()
        || w.is::<gtk::Editable>()
        || w.is::<gtk::ProgressBar>()
        || w.is::<gtk::LevelBar>()
        || w.is::<gtk::Scrollbar>()
        || matches!(w.css_name().as_str(), "trough" | "slider" | "highlight" | "fill" | "text")
}

/// Vinduskontrollene har negativ marg med vilje (base.css).
fn skipped(w: &gtk::Widget) -> bool {
    w.css_name() == "windowcontrols"
}

/// Følger det bredeste barnet nedover: flaten som faktisk krever bredden.
pub fn culprit(w: &gtk::Widget) -> String {
    let mut cur = w.clone();
    loop {
        let own = cur.measure(gtk::Orientation::Horizontal, -1).0;
        let mut best: Option<(gtk::Widget, i32)> = None;
        let mut c = cur.first_child();
        while let Some(ch) = c {
            c = ch.next_sibling();
            if !drawable(&ch) {
                continue;
            }
            let m = ch.measure(gtk::Orientation::Horizontal, -1).0;
            if best.as_ref().is_none_or(|b| m > b.1) {
                best = Some((ch, m));
            }
        }
        match best {
            // Barnet står for det meste av bredden: gå videre ned.
            Some((ch, m)) if m * 10 >= own * 8 && m > 0 => cur = ch,
            _ => return format!("{} (min {own} px)", describe(&cur)),
        }
    }
}

/// Barn som stikker ut av forelderen, og søsken som overlapper (over 1 px).
pub fn findings(w: &gtk::Widget, out: &mut Vec<String>) {
    if !drawable(w) || w.is::<gtk::Popover>() {
        return;
    }
    let mut kids: Vec<(gtk::Widget, gtk::graphene::Rect)> = Vec::new();
    let mut c = w.first_child();
    while let Some(ch) = c {
        c = ch.next_sibling();
        if !drawable(&ch) || ch.is::<gtk::Popover>() || skipped(&ch) {
            continue;
        }
        if let Some(b) = ch.compute_bounds(w)
            && b.width() > 0.0
            && b.height() > 0.0
        {
            kids.push((ch.clone(), b));
        }
    }
    if !layered(w) {
        let (pw, ph) = (w.width() as f32, w.height() as f32);
        // Foreldre som klipper, parkerer skjulte barn helt utenfor seg selv
        // (Adaptive, kolonnene i sensortabellen). Det er ikke et funn.
        let clips = w.overflow() == gtk::Overflow::Hidden;
        kids.retain(|(_, b)| !(clips && (b.x() >= pw || b.y() >= ph || b.x() + b.width() <= 0.0 || b.y() + b.height() <= 0.0)));
        for (k, b) in &kids {
            let (dx, dy) = ((b.x() + b.width() - pw).max(-b.x()), (b.y() + b.height() - ph).max(-b.y()));
            if dx > 1.0 || dy > 1.0 {
                let kind = if clips { "KLIPT" } else { "UT" };
                let why = if dx > 1.0 { format!("; krever bredden: {}", culprit(k)) } else { String::new() };
                out.push(format!("{kind} {} stikker {:.0}×{:.0} px ut av {} ({}×{}){why}", describe(k), dx.max(0.0), dy.max(0.0), describe(w), pw, ph));
            }
        }
        if kids.len() <= 80 {
            for (i, (a, ra)) in kids.iter().enumerate() {
                for (b, rb) in kids.iter().skip(i + 1) {
                    let ox = (ra.x() + ra.width()).min(rb.x() + rb.width()) - ra.x().max(rb.x());
                    let oy = (ra.y() + ra.height()).min(rb.y() + rb.height()) - ra.y().max(rb.y());
                    if ox > 1.0 && oy > 1.0 {
                        out.push(format!("OVERLAPP {} og {} overlapper {:.0}×{:.0} px i {}", describe(a), describe(b), ox, oy, describe(w)));
                    }
                }
            }
        }
    }
    // En kort tekst brutt over flere linjer selv om den hadde fått plass på
    // én: en etikett som kan brytes, i en vannrett rad som er høy nok til to
    // linjer, får bredden for den høyden («Ikke / aktivert»).
    if let Some(bx) = w.downcast_ref::<gtk::Box>()
        && bx.orientation() == gtk::Orientation::Horizontal
    {
        let used: f32 = kids.iter().map(|(_, b)| b.width()).sum();
        let free = w.width() as f32 - used - bx.spacing() as f32 * kids.len().saturating_sub(1) as f32;
        for (k, b) in &kids {
            // Etiketten selv, eller etiketten alene i en beholder (Steady).
            let only = k.first_child().filter(|c| c.next_sibling().is_none());
            if let Some(l) = k.downcast_ref::<gtk::Label>().or_else(|| only.as_ref().and_then(|c| c.downcast_ref::<gtk::Label>()))
                && l.wraps()
                && l.layout().line_count() > 1
                && !l.text().contains('\n')
            {
                let nat = l.measure(gtk::Orientation::Horizontal, -1).1 as f32;
                if nat <= b.width() + free + 0.5 && l.text().chars().count() <= 40 {
                    out.push(format!("BRUTT {} står på {} linjer, men får plass på én ({:.0} px ledig) i {}", describe(l.upcast_ref()), l.layout().line_count(), free, describe(w)));
                }
            }
        }
    }
    for (k, _) in &kids {
        findings(k, out);
    }
}
