//! Fargetokens fra `app/data/theme/tokens.css`: oppslag for det som tegnes i
//! Rust (miniatyrene i Innstillinger, grafer, målere) og
//! kontrasttesten for alle 96 kombinasjonene (port av `scripts/contrast.js`).
//!
//! Fila leses ved bygging (`include_str!`), så CSS og test aldri kan gå i
//! utakt. Selektorene er `window.rigg` med klasser (`.dark`, `.pal-sand`,
//! `.hc`, `.acc-teal`) og `:not(.klasse)`. Som i demoen vinner høyere
//! spesifisitet (antall klasser), og ved lik spesifisitet den som står sist.

use std::collections::HashMap;

pub const TOKENS_CSS: &str = include_str!("../../../data/theme/tokens.css");

pub const PALETTES: [&str; 4] = ["grafitt", "sand", "fjord", "midnatt"];
pub const ACCENTS: [&str; 6] = ["blue", "teal", "green", "purple", "pink", "slate"];

/// Én kombinasjon av valgene.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Env {
    pub dark: bool,
    pub palette: String,
    pub high_contrast: bool,
    pub accent: String,
}

impl Env {
    pub fn new(dark: bool, palette: &str, high_contrast: bool, accent: &str) -> Env {
        Env { dark, palette: palette.into(), high_contrast, accent: accent.into() }
    }

    /// Klassene vinduet får (uten `rigg`).
    pub fn classes(&self) -> Vec<String> {
        let mut c = Vec::new();
        if self.dark {
            c.push("dark".to_string());
        }
        if self.palette != "grafitt" {
            c.push(format!("pal-{}", self.palette));
        }
        if self.high_contrast {
            c.push("hc".to_string());
        }
        if self.accent != "blue" {
            c.push(format!("acc-{}", self.accent));
        }
        c
    }
}

struct Rule {
    yes: Vec<String>,
    no: Vec<String>,
    decls: Vec<(String, String)>,
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn parse_selector(sel: &str) -> Option<(Vec<String>, Vec<String>)> {
    let rest = sel.trim().strip_prefix("window.rigg")?;
    let (mut yes, mut no) = (Vec::new(), Vec::new());
    let mut r = rest;
    while !r.is_empty() {
        if let Some(x) = r.strip_prefix(":not(.") {
            let end = x.find(')')?;
            no.push(x[..end].to_string());
            r = &x[end + 1..];
        } else if let Some(x) = r.strip_prefix('.') {
            let end = x.find(['.', ':']).unwrap_or(x.len());
            yes.push(x[..end].to_string());
            r = &x[end..];
        } else {
            return None;
        }
    }
    Some((yes, no))
}

fn rules() -> &'static [Rule] {
    use std::sync::OnceLock;
    static R: OnceLock<Vec<Rule>> = OnceLock::new();
    R.get_or_init(|| {
        let css = strip_comments(TOKENS_CSS);
        let mut out = Vec::new();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let sels = &rest[..open];
            let Some(close) = rest[open..].find('}') else { break };
            let body = &rest[open + 1..open + close];
            rest = &rest[open + close + 1..];
            let decls: Vec<(String, String)> = body
                .split(';')
                .filter_map(|d| {
                    let (k, v) = d.split_once(':')?;
                    let k = k.trim();
                    k.starts_with("--").then(|| (k.to_string(), v.trim().to_string()))
                })
                .collect();
            for sel in sels.split(',') {
                if let Some((yes, no)) = parse_selector(sel) {
                    out.push(Rule { yes, no, decls: decls.clone() });
                }
            }
        }
        out
    })
}

/// Alle tokenene for en kombinasjon, med kaskaden som i demoen.
pub fn tokens_for(env: &Env) -> HashMap<String, String> {
    let classes = env.classes();
    let mut hit: Vec<(usize, usize, &Rule)> = rules()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.yes.iter().all(|c| classes.contains(c)) && !r.no.iter().any(|c| classes.contains(c)))
        .map(|(i, r)| (r.yes.len() + r.no.len(), i, r))
        .collect();
    hit.sort_by_key(|(spec, order, _)| (*spec, *order));
    let mut out = HashMap::new();
    for (_, _, r) in hit {
        for (k, v) in &r.decls {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// RGBA med kanaler 0–255 og alfa 0–1.
pub type Rgba = [f64; 4];

fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if s == "transparent" {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    if let Some(h) = s.strip_prefix('#') {
        let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
        if h.len() != 6 {
            return None;
        }
        let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
        return Some([p(0)?, p(2)?, p(4)?, 1.0]);
    }
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb("))?.strip_suffix(')')?;
    let p: Vec<f64> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    match p.len() {
        3 => Some([p[0], p[1], p[2], 1.0]),
        4 => Some([p[0], p[1], p[2], p[3]]),
        _ => None,
    }
}

fn eval(tok: &HashMap<String, String>, raw: &str, depth: u32) -> Result<Rgba, String> {
    if depth > 20 {
        return Err(format!("sirkel ved {raw}"));
    }
    let raw = raw.trim();
    if let Some(name) = raw.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) {
        let v = tok.get(name.trim()).ok_or_else(|| format!("mangler {name}"))?;
        return eval(tok, v, depth + 1);
    }
    if let Some(inner) = raw.strip_prefix("color-mix(in srgb,").and_then(|r| r.strip_suffix(')')) {
        // «<farge> <p>%, <farge>»
        let (a, b) = split_top(inner).ok_or_else(|| format!("kan ikke tolke {raw}"))?;
        let a = a.trim();
        let pi = a.rfind(' ').ok_or_else(|| format!("mangler prosent i {raw}"))?;
        let p: f64 = a[pi + 1..].trim_end_matches('%').parse::<f64>().map_err(|_| format!("prosent i {raw}"))? / 100.0;
        let c1 = eval(tok, &a[..pi], depth + 1)?;
        let c2 = eval(tok, b, depth + 1)?;
        let al = c1[3] * p + c2[3] * (1.0 - p);
        if al == 0.0 {
            return Ok([0.0; 4]);
        }
        let ch = |i: usize| (c1[i] * c1[3] * p + c2[i] * c2[3] * (1.0 - p)) / al;
        return Ok([ch(0), ch(1), ch(2), al]);
    }
    parse_color(raw).ok_or_else(|| format!("kan ikke tolke farge: {raw}"))
}

/// Deler på første komma som ikke er inne i parenteser.
fn split_top(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Fargen til et token for en kombinasjon.
pub fn color(env: &Env, name: &str) -> Result<Rgba, String> {
    let tok = tokens_for(env);
    let v = tok.get(name).ok_or_else(|| format!("mangler {name}"))?;
    eval(&tok, v, 0)
}

/// Som [`color`], men med en gjennomsiktig reserve så tegningen aldri feiler.
pub fn color_or(env: &Env, name: &str) -> Rgba {
    color(env, name).unwrap_or([128.0, 128.0, 128.0, 1.0])
}

/// Legger en (delvis gjennomsiktig) farge over en ugjennomsiktig bakgrunn.
pub fn over(fg: Rgba, bg: Rgba) -> Rgba {
    [fg[0] * fg[3] + bg[0] * (1.0 - fg[3]), fg[1] * fg[3] + bg[1] * (1.0 - fg[3]), fg[2] * fg[3] + bg[2] * (1.0 - fg[3]), 1.0]
}

fn lin(v: f64) -> f64 {
    let v = v / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

pub fn luminance(c: Rgba) -> f64 {
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG-kontrast for en forgrunn over en ugjennomsiktig bakgrunn.
pub fn ratio(fg: Rgba, bg: Rgba) -> f64 {
    let f = over(fg, bg);
    let (a, b) = (luminance(f), luminance(bg));
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Ett par som sjekkes: (navn, krav, målt).
pub type Check = (String, f64, f64);

/// Alle parene for en kombinasjon (samme som `scripts/contrast.js`).
pub fn checks(env: &Env) -> Result<Vec<Check>, String> {
    let tok = tokens_for(env);
    let c = |n: &str| -> Result<Rgba, String> { eval(&tok, tok.get(n).ok_or_else(|| format!("mangler {n}"))?, 0) };
    let solid = |n: &str, on: &str| -> Result<Rgba, String> { Ok(over(c(n)?, c(on)?)) };
    let mut list: Vec<Check> = Vec::new();
    let mut text = |l: String, fg: Rgba, bg: Rgba| list.push((format!("tekst: {l}"), 4.5, ratio(fg, bg)));
    const SURFACES: [&str; 7] = ["--bg", "--card", "--card-alt", "--panel-head", "--sidebar", "--head", "--field"];
    const TEXT_FG: [&str; 7] = ["--text", "--muted", "--dim", "--accent", "--ok", "--warn", "--danger"];
    for f in TEXT_FG {
        for s in SURFACES {
            text(format!("{f} på {s}"), c(f)?, c(s)?);
        }
    }
    text("--accent-fg på --accent-bg".into(), c("--accent-fg")?, c("--accent-bg")?);
    text("--tip-fg på --tip-bg".into(), c("--tip-fg")?, c("--tip-bg")?);
    text("--text på valgt rad".into(), c("--text")?, solid("--selected", "--sidebar")?);
    text("--text på --hover over --card".into(), c("--text")?, solid("--hover", "--card")?);
    text("--accent på --accent-soft over --card".into(), c("--accent")?, solid("--accent-soft", "--card")?);
    text("--text på søketreff".into(), c("--text")?, solid("--accent-soft-strong", "--card")?);
    text("--muted på --hover over --sidebar".into(), c("--muted")?, solid("--hover", "--sidebar")?);
    text("--warn på --warn-soft over --card".into(), c("--warn")?, solid("--warn-soft", "--card")?);
    text("--danger på --danger-soft over --card".into(), c("--danger")?, solid("--danger-soft", "--card")?);
    for f in ["--text", "--muted"] {
        text(format!("{f} på --accent-soft over --bg"), c(f)?, solid("--accent-soft", "--bg")?);
        text(format!("{f} på --warn-soft over --bg"), c(f)?, solid("--warn-soft", "--bg")?);
        text(format!("{f} på --info-soft over --bg"), c(f)?, solid("--info-soft", "--bg")?);
    }
    text("--text på --danger-soft over --bg".into(), c("--text")?, solid("--danger-soft", "--bg")?);
    let ok = c("--ok")?;
    text("--ok på 14 % --ok over --card".into(), ok, over([ok[0], ok[1], ok[2], 0.14], c("--card")?));
    let mut ui = |l: String, fg: Rgba, bg: Rgba| list.push((format!("ui: {l}"), 3.0, ratio(fg, bg)));
    for s in ["--field", "--card", "--bg"] {
        ui(format!("--field-border mot {s}"), c("--field-border")?, c(s)?);
    }
    for s in ["--bg", "--card", "--sidebar"] {
        ui(format!("--focus mot {s}"), c("--focus")?, c(s)?);
    }
    ui("--accent-fill mot --track".into(), c("--accent-fill")?, c("--track")?);
    ui("--accent-fill mot --card".into(), c("--accent-fill")?, c("--card")?);
    for n in ["--series-1", "--series-2", "--series-3", "--warn-fill", "--danger-fill", "--ok"] {
        ui(format!("{n} mot --card"), c(n)?, c("--card")?);
    }
    for n in ["--pal-blue", "--pal-magenta", "--pal-purple", "--pal-green", "--pal-teal", "--pal-cyan", "--pal-gray"] {
        for s in ["--card", "--card-alt"] {
            ui(format!("{n} mot {s}"), c(n)?, c(s)?);
        }
    }
    ui("hake (#fff) mot --accent-bg".into(), [255.0, 255.0, 255.0, 1.0], c("--accent-bg")?);
    if env.high_contrast {
        for s in ["--card", "--bg", "--sidebar"] {
            ui(format!("--border mot {s} (høy kontrast)"), c("--border")?, c(s)?);
        }
    }
    Ok(list)
}

/// Alle 96 kombinasjonene.
pub fn all_envs() -> Vec<Env> {
    let mut v = Vec::new();
    for p in PALETTES {
        for dark in [false, true] {
            for a in ACCENTS {
                for hc in [false, true] {
                    v.push(Env::new(dark, p, hc, a));
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kaskaden_som_i_demoen() {
        let e = Env::new(false, "grafitt", false, "blue");
        assert_eq!(color(&e, "--bg").unwrap(), [246.0, 246.0, 247.0, 1.0]);
        let e = Env::new(true, "sand", false, "teal");
        assert_eq!(color(&e, "--bg").unwrap(), [0x23 as f64, 0x20 as f64, 0x1c as f64, 1.0]);
        assert_eq!(color(&e, "--accent").unwrap(), [0x4d as f64, 0xa8 as f64, 0xbb as f64, 1.0], "mørkt bruker --a-d");
        let e = Env::new(true, "midnatt", true, "blue");
        assert_eq!(color(&e, "--text").unwrap(), [255.0, 255.0, 255.0, 1.0], "høy kontrast vinner over fargetemaet");
        let soft = color(&Env::new(false, "grafitt", false, "blue"), "--accent-soft").unwrap();
        assert!((soft[3] - 0.12).abs() < 1e-9);
    }

    #[test]
    fn alle_96_kombinasjonene_har_nok_kontrast() {
        let envs = all_envs();
        assert_eq!(envs.len(), 96);
        let mut fails = Vec::new();
        let mut pairs = 0;
        for e in &envs {
            for (label, min, r) in checks(e).unwrap() {
                pairs += 1;
                if r + 1e-9 < min {
                    fails.push(format!("{:?}: {label} = {r:.2} (krav {min})", e.classes()));
                }
            }
        }
        assert!(pairs > 96 * 90, "{pairs} par");
        assert!(fails.is_empty(), "{} par under kravet:\n{}", fails.len(), fails.join("\n"));
    }
}
