//! Spalter som faktisk får plass. Spalteoppsettene (`Masonry`, `Cols`,
//! `Columns`) velger antall spalter ut fra bruddpunkter i vindusbredden. Et
//! bruddpunkt vet ikke hvor bredt innholdet er: med lengre ord (tysk), større
//! skrift eller et bredt rutenett ble et panel bredere enn spalten sin og la
//! seg over naboen (Innstillinger › Utseende i 1.6.0). Her er regelen de
//! deler: bruddpunktet gir det *høyeste* antallet, og antallet går ned til
//! hvert barn får minst sin egen minste bredde.
//!
//! Ren Rust uten GTK, så regelen testes uten skjerm.

/// (x, bredde) per spalte når `width` deles etter `weights` med `gap` imellom.
/// Siste spalte tar resten, så summen alltid er `width`.
pub fn columns(width: i32, gap: i32, weights: &[f32]) -> Vec<(i32, i32)> {
    let n = weights.len().max(1);
    let avail = (width - gap * (n as i32 - 1)).max(n as i32);
    let sum: f32 = weights.iter().sum();
    let mut out = Vec::with_capacity(n);
    let mut x = 0;
    for i in 0..n {
        let wgt = weights.get(i).copied().unwrap_or(1.0);
        let cw = if i + 1 == n { width - x } else if sum > 0.0 { (avail as f32 * wgt / sum).floor() as i32 } else { avail / n as i32 };
        out.push((x, cw.max(1)));
        x += cw + gap;
    }
    out
}

/// Like brede spalter.
pub fn even(width: i32, gap: i32, n: usize) -> Vec<(i32, i32)> {
    columns(width, gap, &vec![1.0; n.max(1)])
}

/// Det høyeste antallet spalter, høyst `want`, der hvert barn får minst sin
/// minste bredde. `mins[i]` er minste bredde for barn `i`, som står i spalte
/// `i % n`. `cols_for(n)` gir spaltene for `n` spalter. Én spalte godtas
/// alltid: da er det beholderens egen minste bredde som gjelder.
pub fn fit_cols(want: usize, mins: &[i32], cols_for: impl Fn(usize) -> Vec<(i32, i32)>) -> usize {
    fit_cols_where(want, mins, cols_for, |_| true)
}

/// Som [`fit_cols`], men bare antall som `allowed` godtar (én spalte godtas
/// alltid). Brukes der radene skal være hele: fire kort står 4 × 1, 2 × 2
/// eller 1 × 4, aldri 3 + 1.
pub fn fit_cols_where(want: usize, mins: &[i32], cols_for: impl Fn(usize) -> Vec<(i32, i32)>, allowed: impl Fn(usize) -> bool) -> usize {
    let mut n = want.max(1);
    while n > 1 {
        if allowed(n) {
            let cols = cols_for(n);
            if cols.len() == n && mins.iter().enumerate().all(|(i, m)| *m <= cols[i % n].1) {
                break;
            }
        }
        n -= 1;
    }
    n
}

/// Hele rader: antallet går opp i antall barn.
pub fn whole_rows(kids: usize) -> impl Fn(usize) -> bool {
    move |n| n > 0 && kids.is_multiple_of(n)
}

/// Som [`fit_cols`] når barna fylles spalte for spalte (Masonry): alle
/// spaltene er like brede, så det bredeste barnet avgjør.
pub fn fit_even(want: usize, width: i32, gap: i32, widest_min: i32) -> usize {
    fit_cols(want, &[widest_min], |n| even(width, gap, n))
}

/// Skal to flater stå side ved side? Begge må få minst sin minste bredde.
pub fn fits_pair(first_w: i32, first_min: i32, second_w: i32, second_min: i32) -> bool {
    first_w >= first_min && second_w >= second_min
}

/// En rad med tekst til venstre og en kontroll til høyre (Innstillinger).
/// `Some((tekstbredde, kontrollbredde))` når de står side ved side, `None`
/// når kontrollen skal ned under teksten.
///
/// Side ved side krever at teksten får minst `comfort` px (eller hele sin
/// naturlige bredde når den er kortere), og aldri mindre enn sitt lengste
/// ord. Ellers ble forklaringen brutt i tre–fire korte linjer ved siden av
/// en bred segmentknapp, eller raden ble bredere enn panelet. Kontrollen kan
/// krympe fra naturlig til minste bredde før raden brytes.
pub fn row_fit(width: i32, gap: i32, text: (i32, i32), ctrl: (i32, i32), comfort: i32) -> Option<(i32, i32)> {
    let (text_min, text_nat) = text;
    let (ctrl_min, ctrl_nat) = (ctrl.0, ctrl.1.max(ctrl.0));
    let need = text_min.max(text_nat.min(comfort));
    if width - gap - ctrl_min < need {
        return None;
    }
    let ctrl_w = ctrl_nat.min(width - gap - need).max(ctrl_min);
    Some((width - gap - ctrl_w, ctrl_w))
}

/// Plasseringen i et grafhode (`charthead.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadFit {
    /// Bredden tittelen får.
    pub title_w: i32,
    /// Forklaringen står på tittellinja (ellers på egen linje under).
    pub legend_inline: bool,
    /// «min · maks» får plass på tittellinja.
    pub stats: bool,
}

/// Hodet på en graf: tittel, forklaring (serienavnene), «min · maks» og
/// halen (verdien nå og pauseknappen), i den rekkefølgen de får plass.
///
/// 1. Halen står alltid. Tittelen får resten opp til sin naturlige bredde,
///    og kuttes bare når den alene ikke får plass.
/// 2. Forklaringen står etter tittelen hvis hele får plass. Ellers går den
///    ned på egen linje. Den forsvinner aldri.
/// 3. «min · maks» vises hvis hele får plass i det som er igjen.
///
/// Før sto alt i én `GtkBox` der valgfrie deler ba om naturlig bredde og
/// fikk en del av den: plassen ble delt ut til deler som likevel ikke ble
/// vist, og tittelen ble kuttet («GPU-tempera…») ved siden av et tomrom.
pub fn head_fit(width: i32, gap: i32, title: (i32, i32), tail_w: i32, legend_w: Option<i32>, stats_w: Option<i32>, stats_gap: i32) -> HeadFit {
    let (title_min, title_nat) = title;
    let tail = if tail_w > 0 { tail_w + gap } else { 0 };
    let title_w = title_nat.min(width - tail).max(title_min).max(0);
    let mut rest = width - title_w - tail;
    let legend_inline = legend_w.is_some_and(|lw| rest >= lw + gap);
    if legend_inline {
        rest -= legend_w.unwrap_or(0) + gap;
    }
    let stats = stats_w.is_some_and(|sw| sw > 0 && rest >= sw + stats_gap);
    HeadFit { title_w, legend_inline, stats }
}

/// Den laveste tillatte spalten: det minste antallet fra og med `min_cols`
/// som `allowed` godtar, høyst `want`.
pub fn floor_cols(want: usize, min_cols: usize, allowed: impl Fn(usize) -> bool) -> usize {
    (min_cols.max(1)..=want.max(1)).find(|n| allowed(*n)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafhodet_gir_tittelen_plassen_først() {
        // Grafer i 1024 px på bokmål (1.6.0): kortet er 351 px, tittelen 104,
        // halen 104, «min · maks» 118. Før ble tittelen «GPU-tempera…».
        let f = head_fit(351, 12, (20, 104), 104, None, Some(118), 8);
        assert_eq!(f, HeadFit { title_w: 104, legend_inline: false, stats: true });
        // Seks piksler trangere: «min · maks» faller bort, tittelen står hel.
        let f = head_fit(351 - 6, 12, (20, 104), 104, None, Some(118), 8);
        assert_eq!(f, HeadFit { title_w: 104, legend_inline: false, stats: false });
        // Tittelen kuttes først når den alene ikke får plass ved siden av halen.
        let f = head_fit(180, 12, (20, 104), 104, None, Some(118), 8);
        assert_eq!(f.title_w, 64);
        assert!(!f.stats);
        // Aldri under tittelens minste bredde, og aldri negativ.
        assert_eq!(head_fit(60, 12, (20, 104), 104, None, None, 8).title_w, 20);
        assert_eq!(head_fit(0, 12, (0, 104), 104, None, None, 8).title_w, 0);
    }

    #[test]
    fn forklaringen_går_ned_på_egen_linje_og_forsvinner_ikke() {
        // Oversikt › Temperaturer i 1024 px: fire serier (300 px) får ikke
        // plass etter tittelen i et kort på 351 px. Før forsvant de.
        let f = head_fit(351, 12, (20, 90), 30, Some(300), None, 8);
        assert_eq!(f, HeadFit { title_w: 90, legend_inline: false, stats: false });
        // To serier (120 px) får plass på tittellinja.
        let f = head_fit(351, 12, (20, 40), 30, Some(120), None, 8);
        assert!(f.legend_inline);
        // Forklaringen går foran «min · maks» på tittellinja.
        let f = head_fit(400, 12, (20, 70), 104, Some(160), Some(118), 8);
        assert_eq!(f, HeadFit { title_w: 70, legend_inline: true, stats: false });
        // Med forklaringen på egen linje får «min · maks» plassen den ga fra seg.
        let f = head_fit(330, 12, (20, 70), 104, Some(160), Some(118), 8);
        assert_eq!(f, HeadFit { title_w: 70, legend_inline: false, stats: true });
        // Uten hale (ingen pauseknapp) brukes hele bredden.
        assert_eq!(head_fit(200, 12, (20, 80), 0, Some(108), None, 8), HeadFit { title_w: 80, legend_inline: true, stats: false });
    }

    #[test]
    fn laveste_tillatte_spalte() {
        // Seks fliser i hele rader, minst tre per rad: 3. Åtte, minst to: 2.
        assert_eq!(floor_cols(6, 3, whole_rows(6)), 3);
        assert_eq!(floor_cols(8, 2, whole_rows(8)), 2);
        assert_eq!(floor_cols(8, 3, whole_rows(8)), 4);
        assert_eq!(floor_cols(5, 2, whole_rows(5)), 5, "fem går bare opp i fem");
        assert_eq!(floor_cols(3, 9, |_| true), 1, "ingen tillatt: én spalte");
    }

    #[test]
    fn innstillingsraden_brytes_når_teksten_blir_for_trang() {
        // «Animasjoner» i 1.6.0: 459 px rad, segmentknapp på 290 px. Teksten
        // fikk 153 px og ble brutt i tre linjer.
        assert_eq!(row_fit(459, 16, (60, 330), (290, 290), 180), None);
        // «Tetthet»: knappen er 226 px, teksten får 217 px. Side ved side.
        assert_eq!(row_fit(459, 16, (60, 300), (226, 226), 180), Some((217, 226)));
        // En bryter er smal: alltid side ved side.
        assert_eq!(row_fit(459, 16, (60, 600), (44, 44), 180), Some((399, 44)));
        // Kort tekst trenger ikke 180 px: «Tema» ved siden av en bred knapp.
        assert_eq!(row_fit(459, 16, (40, 40), (380, 380), 180), Some((63, 380)));
        // Et langt ord (tysk) er minste bredde, også over komfortgrensen.
        assert_eq!(row_fit(459, 16, (220, 400), (226, 226), 180), None);
        assert_eq!(row_fit(470, 16, (220, 400), (226, 226), 180), Some((228, 226)));
    }

    #[test]
    fn kontrollen_krymper_før_raden_brytes() {
        // Sti med ellipse (min 80, naturlig 400) og en lang tekst.
        let fit = row_fit(459, 16, (60, 300), (80, 400), 180);
        assert_eq!(fit, Some((180, 263)), "kontrollen får det teksten ikke trenger");
        // Summen er alltid radens bredde.
        let (t, c) = fit.unwrap();
        assert_eq!(t + 16 + c, 459);
        // Naturlig bredde under minste bredde tåles.
        assert_eq!(row_fit(300, 16, (40, 40), (100, 50), 180), Some((184, 100)));
    }

    #[test]
    fn spaltene_fyller_bredden() {
        assert_eq!(even(1010, 12, 2), vec![(0, 499), (511, 499)]);
        let c = columns(1000, 8, &[1.25, 1.0]);
        assert_eq!(c[0].0, 0);
        assert_eq!(c[1].0 + c[1].1, 1000, "siste spalte tar resten");
        assert!(c[0].1 > c[1].1);
        // Aldri null eller negativ bredde.
        assert!(even(3, 12, 4).iter().all(|(_, w)| *w >= 1));
    }

    #[test]
    fn utseende_i_1_6_0_får_én_spalte_når_panelet_er_for_bredt() {
        // Feilen fra brukerens skjermbilde: spalten er 499 px, «Farger» krever 552.
        assert_eq!(fit_even(2, 1010, 12, 552), 1);
        // Når panelet får plass, står bruddpunktets antall.
        assert_eq!(fit_even(2, 1010, 12, 480), 2);
        assert_eq!(fit_even(2, 1010, 12, 499), 2, "akkurat plass");
        assert_eq!(fit_even(2, 1010, 12, 500), 1, "én piksel for lite");
    }

    #[test]
    fn tre_spalter_går_til_to_før_én() {
        // 1600 px: tre spalter på 525, to på 794.
        assert_eq!(fit_even(3, 1600, 12, 500), 3);
        assert_eq!(fit_even(3, 1600, 12, 600), 2);
        assert_eq!(fit_even(3, 1600, 12, 900), 1);
        // Bredere enn hele flaten: én spalte, aldri null.
        assert_eq!(fit_even(3, 1600, 12, 5000), 1);
        assert_eq!(fit_even(0, 1600, 12, 10), 1);
    }

    #[test]
    fn barna_står_i_hver_sin_spalte() {
        // Vekter 1,25 : 1 på 808 px gir 444 og 356. Barn 1 krever 400: for trangt.
        let cols = |n: usize| if n == 2 { columns(808, 8, &[1.25, 1.0]) } else { even(808, 8, n) };
        assert_eq!(fit_cols(2, &[300, 400], cols), 1);
        // Det brede barnet i den brede spalten går bra.
        assert_eq!(fit_cols(2, &[400, 300], cols), 2);
        // Barn 2 står i spalte 0 igjen (ny rad).
        assert_eq!(fit_cols(2, &[300, 300, 450], cols), 1);
        assert_eq!(fit_cols(2, &[300, 300, 440], cols), 2);
        // Uten barn står ønsket antall.
        assert_eq!(fit_cols(3, &[], |n| even(900, 8, n)), 3);
    }

    #[test]
    fn hele_rader_hopper_over_tre_pluss_én() {
        // Fire fargetemakort på 474 px: fire à 109 px er for trangt for 140 px.
        // Tre ville fått plass, men gir 3 + 1. Da blir det 2 × 2.
        let cols = |n: usize| even(474, 12, n);
        assert_eq!(fit_cols(4, &[140; 4], cols), 3, "uten regelen: 3 + 1");
        assert_eq!(fit_cols_where(4, &[140; 4], cols, whole_rows(4)), 2);
        assert_eq!(fit_cols_where(4, &[100; 4], cols, whole_rows(4)), 4);
        assert_eq!(fit_cols_where(4, &[300; 4], cols, whole_rows(4)), 1);
        // Ønsket antall som ikke går opp, hoppes også over.
        assert_eq!(fit_cols_where(3, &[10; 4], cols, whole_rows(4)), 2);
    }

    #[test]
    fn to_flater_side_ved_side() {
        assert!(fits_pair(652, 600, 320, 320));
        assert!(!fits_pair(652, 700, 320, 300), "hovedflaten er for trang");
        assert!(!fits_pair(652, 600, 320, 340), "sidekolonnen er for trang");
    }
}
