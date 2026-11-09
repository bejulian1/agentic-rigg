# Designkritikk – Agentic Rigg alpha v0.0.3 · bygg 3

4. november 2026. Kritikken bygger på skjermbilder fra headless Chrome (egen server på port 4321, live-data fra RTX 2080) av alle ni sider i 1920×1080 og 1280×800, lyst og mørkt tema, pluss 840×800 (ikonsidefelt), nærbilder og fokustest. Jeg har også lest `web/index.html`, `web/styles.css` og sidene i `web/js/pages/`. Skjermbildene ligger i
`/tmp/claude-1000/-home-julian--claude-spares-spare-6B1YYE/b1cbda93-d303-42c5-84d3-fb3430d92402/scratchpad/kritikk-v003/`.

**Merking.** **[LAT]** betyr at noe avslører en lat eller uferdig implementering: inkonsekvente avstander, uferdige tilstander, ting som hopper, manglende hover eller fokus, tekst som brekker, tall som danser, rå verdier eller begrensninger som ikke forklares. Brukeren sa at nettopp slike detaljer skiller god programvare fra dårlig. Fiks dem selv om de er små.

Prioritet: **P1** må fikses, **P2** bør fikses, **P3** er fint å ha.

---

## 1. Helhetsinntrykk

Appen har riktig retning. Gradientene er borte, fargene er dempet og informasjonen er ærlig. Det som gjør at den fortsatt føles «vibe coded»:

1. **Bokser i bokser i bokser.** Hver gruppe har ramme. Inni står hver lesbare verdi i et felt med egen ramme, og grafene har en tredje ramme. Skrivebeskyttede verdier ser ut som felt du kan skrive i. Det er hovedgrunnen til at sidene virker travle uten å vise mer.
2. **Skallet er satt sammen og ikke designet.** Ett toppfelt går over både sidefelt og innhold. Sidetittelen står to ganger (toppfelt og h1). GPU-navn og driver står midt i toppfeltet på alle sider, også System. Det er ca. 550 px tomrom mellom «Lys (RGB)» og «Innstillinger».
3. **Død plass der det ikke trengs, trengsel der det trengs.** På 1920×1080 er nederste 350–450 px tomme på Oversikt, Grafer, Kontroller, Viftekurve og Lys, og halve bredden er tom på Innstillinger. Samtidig er Sensorer 2155 px høy.
4. **Gjentakelser.** «Simulert» står tre–fire ganger på samme skjerm. Datakilden står fire ganger på Innstillinger. Versjonen står tre ganger.
5. **Rå verdier og uforklarte tall.** For eksempel `ucsi_source_psy_8_00081 · curr1`, `ubuntu:GNOME`, `0/0 MHz`, «Mot nedklokking 37 %» (temperaturen delt på 97 °C fra 0 °C) og y-akser med 1 875 og 6 375.
6. **Ikoner som ikke skiller seg.** Innstillinger og Lys har nesten samme sol-ikon. Grafer og Viftekurve er begge «akse med strek». I ikonsidefeltet (under 860 px) kan du ikke se forskjell, og det finnes ingen verktøytips.
7. **Ingen tydelig typografisk skala.** Appen bruker 8 skriftstørrelser (11, 11,5, 12, 12,5, 13, 16, 18 og 22 px), 3 vekter og 6 forskjellige komponenthøyder (18–32 px).

---

## 2. Globalt

### 2.1 Typografi

| # | Pri | Hva | Hvor | Forslag |
|---|---|---|---|---|
| T1 | P1 | `system-ui` blir Noto Sans her (`fc-match system-ui`), ikke skrivebordets skrift. Appen ser derfor ikke ut som resten av GNOME | `:root --font` | `--font: 'Ubuntu Sans', 'Adwaita Sans', 'Cantarell', 'Inter', system-ui, sans-serif;`. Ubuntu 26.04 bruker Ubuntu Sans i grensesnittet. Mono: `'Ubuntu Sans Mono', 'Adwaita Mono', ui-monospace, monospace` |
| T2 | P1 | Det er 8 størrelser uten system | hele `styles.css` | Bruk en fast skala med fem trinn: `--fs-cap: 11px` (aksetall og fotnoter), `--fs-sm: 12px` (sekundærtekst og etiketter i tabeller), `--fs-body: 13px`, `--fs-title: 15px` (gruppetitler og sidetittel i toppfeltet), `--fs-num: 24px` (store tall). Fjern 11,5 og 12,5. Siden h1-ene går bort (se Skall), trengs ikke 18 px |
| T3 | P2 | Vektene 400, 500, 600 og 700 brukes om hverandre. `.nav-sec`, `.group-title` og `.sensors tr.grp button` bruker 700, og knapper bruker 500 | | Bruk bare 400 og 600. GNOME bruker «heading» (600) og brødtekst (400) |
| T4 | P2 | Store bokstaver med sperring (`.nav-sec`, `.stat-k`, `.sensors tr.sub`) gir et web-dashboard-preg | | Bruk vanlig skrift i `--fs-sm` med vekt 600 og fargen `--muted`, slik Adwaita gjør med `.heading` og `.dim-label`. Behold store bokstaver bare på underrader i Sensorer, eller fjern dem der også |
| T5 | P1 **[LAT]** | Mellomrommet mellom tall og enhet er et vanlig mellomrom (`${v} °C`, `${v} W`), så linja kan brekke mellom «36» og «°C». Tusenskilletegnet er riktig nok smalt mellomrom (U+202F i `fmt0`/`fmtN`) | `util.js`, `pages/sensors.js` (`F`), `overview.js`, `app.js` (`renderSide`) | Lag én hjelper `withUnit(v, unit)` som setter U+202F (smalt hardt mellomrom) mellom tall og enhet, og bruk den overalt. Prosent med samme mellomrom («38 %»), slik norsk standard sier |
| T6 | P1 **[LAT]** | Grader skrives på to måter: «20°C» på viftekurveaksen og i verktøytipset («37°C → 37 %»), men «37 °C» ellers | `fancurve.js` | Bruk alltid «37 °C» med U+202F. Aksetall kan stå uten enhet når aksen har tittelen «°C» |
| T7 | P2 **[LAT]** | Tall er venstrejustert i lesefeltene for klokker («975 MHz» under «1 335 MHz»), men høyrejustert i prosentfeltene. Når antall sifre endres, flytter enheten seg | `.rows-3 .val` på Oversikt, `#gc-*`, `#sy-*` | Regel: målte tall står alltid høyrejustert med tabulære sifre. Tekst står venstrejustert |
| T8 | P3 | Kursiv brukes for «Ikke oppgitt av driveren» | `.val.na`, `.grid-table td.na` | GNOME bruker ikke kursiv i grensesnittet. Bruk vanlig skrift i fargen `--dim` |

### 2.2 Avstander og rutenett

- **P1 – Innfør en 4-punkts skala** og bruk bare den: `--sp-1: 4px`, `--sp-2: 8px`, `--sp-3: 12px`, `--sp-4: 16px`, `--sp-6: 24px`. I dag forekommer 1, 2, 3, 5, 6, 7, 10, 14, 18, 22 og 30 px. **[LAT]**
- **P1 – Komponenthøyder:** i dag er det 18 (badge), 22 (chip/source), 24 (val, btn.small), 26 (seg, pill), 28 (btn, numfield) og 32 (nav). Bruk tre høyder: `--h-sm: 24px` (kompakt), `--h-md: 32px` (knapper, felt og segmenter; Adwaita bruker 34) og `--h-row: 36px` (rader i lister og innstillinger). Tettheten (se Innstillinger) skalerer disse.
- **P1 – Én innholdsbredde og et 12-kolonners rutenett** med container queries i stedet for `@media (max-width: 1280px)`. Bruddpunktene regner i dag med vindusbredden, men innholdet er 196 px smalere. Derfor går Grafer til én kolonne allerede ved 1280 px, selv om det er 1084 px til rådighet. Bruk `container-type: inline-size` på `.content` og `@container (min-width: …)`.
- **P2 – Innholdsmarg:** `.content { padding: 14px 18px 28px }` er skjev. Bruk 16 px eller 24 px likt på alle sider.
- **P2 – Gruppeavstand:** grupper står 12 px fra hverandre, men gruppetittelen har 6 px luft over og 10 px under. Bruk 12 px mellom grupper og 8 px inne i titler, slik at rytmen går opp.

### 2.3 Farger og kontrast

Målt kontrast:

| Par | Lyst | Mørkt | Vurdering |
|---|---|---|---|
| `--dim` på sidefeltet (kbd-tall, versjon, aksetall 11 px) | **3,18:1** | 4,02:1 | Under 4,5:1 for liten tekst. P1 |
| `--muted` på kort | 6,4:1 | 6,4:1 | OK |
| Kort mot bakgrunn | 1,07:1 | 1,12:1 | Kortet skilles bare av rammen. Greit i Adwaita-stil, men da må rammene være gode |
| `--field-border` mot kort | 1,58:1 | 1,48:1 | For svakt til å bære felt, men for sterkt når alt har ramme (se «bokser i bokser») |
| `--track` mot kort (tomme stolper) | 1,27:1 | – | Tom stolpe forsvinner nesten i lyst tema |
| Gliderknott mot kort, mørkt tema | – | **1,19:1** | Knotten er nesten usynlig. P1 **[LAT]** |

Forslag:
- **P1:** `--dim` lyst `#77746f` (≈4,5:1 på `--sidebar`), mørkt `#9a9792`.
- **P1:** Knotten på glideren skal alltid være lys med skygge, slik Adwaita gjør: lyst tema `#fff`, mørkt tema `#e8e8e8`, med `box-shadow: 0 1px 2px rgba(0,0,0,.3)`. Det gjelder `input[type=range]::-webkit-slider-thumb`.
- **P2:** Når feltrammene rundt skrivebeskyttede verdier er borte (se 2.5), kan `--border` på grupper bli litt tydeligere (lyst `#d0cdc8`), og kortet kan skilles fra bakgrunnen med en svak skygge (`0 1px 3px rgba(0,0,0,.07)`), slik `.card` i Adwaita gjør.
- **P2:** Seriefargene er `--series-1 = --accent`. Når aksentfargen kan velges (se Innstillinger), må serie 2 (oransje) fortsatt skille seg fra alle aksentfargene. Velg serie 2 ut fra aksenten, eller la serie 1 være fast blå i grafer.
- **P2:** Det er `--warn-soft`-bakgrunn på tre ting med ulik betydning: infolinja «simulert», merket «Referanse – ikke målt» og RGB-pillen. Gul skal bety advarsel. «Simulert» og «Referanse» er informasjon og bør få et nøytralt merke med ikon.
- **P3:** I det lyse temaet er bakgrunnen varmgrå (`#f4f3f2`). Adwaita bruker `#fafafb` for vinduet og `#ebebed` for sidefeltet. Vurder å gå litt kaldere, så appen ikke ser «beige» ut ved siden av GNOME-apper.

### 2.4 Ikoner

Se egen seksjon 7. Det globale problemet er at ikonene er tegnet for hånd med ulik optisk vekt (strek 1,3 i sidefeltet, 1,5 i infolinja, 2 i toast og chevron), og at to par er nesten like.

### 2.5 Komponentkonsistens

| # | Pri | Komponent | Problem | Forslag |
|---|---|---|---|---|
| K1 | **P1** | Skrivebeskyttede verdier (`.val`) | Hver verdi står i en hvit boks med ramme og ser redigerbar ut. Det gir støy på Oversikt, Grafikkort, System og Om | Lag `.kv`-rader i Adwaita-stil: etikett i `--muted` til venstre (fast kolonne), verdien som ren tekst og skillelinje `--border-soft` mellom radene. Behold feltutseendet bare der du kan skrive. Verdiene skal kunne markeres og kopieres (se mikrodetaljer). Dette alene fjerner kanskje 40 % av den visuelle støyen |
| K2 | P1 | Segmentkontroll (`.seg`) | Valgt segment fylles helt med aksentfarge. Innstillinger har tre blå blokker under hverandre. Fokusringen (2 px aksent) **forsvinner** på et valgt segment (sjekket med Tab: fokus på «0,5 s» kan ikke sees) **[LAT]** | Bruk stilen til AdwToggleGroup: en grå skinne (`--track`) der det valgte segmentet er en hevet «knapp» (`--card`, skygge og vekt 600). Aksentfargen brukes ikke. Fokus: `outline: 2px solid color-mix(in srgb, var(--accent) 60%, transparent); outline-offset: 2px` på hele segmentet |
| K3 | P1 | Knapper | `.btn:hover` (`--card` → `--card-head`) kan nesten ikke sees i lyst tema. Ingen tydelig trykk-tilstand. `.btn.ghost` brukes i `controls.js`, men **finnes ikke i CSS** **[LAT]** | Definer `.btn`, `.btn.flat` (ghost), `.btn.suggested`, `.btn.destructive` og `.btn.icon`. Hover: `--hover`-overlegg. Trykk: `--active`. Deaktivert: 50 % gjennomsiktighet og et verktøytips som sier hvorfor |
| K4 | P1 | Tallfelt (`.numfield`) og glider | Feltet står øverst til høyre, opptil 980 px fra glideren. Tallet har vekt 600, enheten 12 px | Én rad: etikett og beskrivelse (fast 280 px), glider (fleks), tallfelt (96 px) og en tilbakestill-knapp (↺) som bare vises når verdien avviker fra standard |
| K5 | P2 | Tabeller (`.grid-table`) | Alle kolonner unntatt den første høyrejusteres, også tekstkolonner. Cache-tabellen har «Data / Instruksjon / Felles» høyrejustert **[LAT]** | `th` og `td` får `data-align`. Tekst står til venstre og tall til høyre. Overskriften følger kolonnen |
| K6 | P2 | Bryter (`.switch`) | Kommer greit ut. Knotten er `--field`, som er nesten svart i mørkt tema | Knotten skal alltid være lys. Bruk 150 ms overgang (se Bevegelse) |
| K7 | P2 | Piller, merker og brikker | Det finnes fire nesten like varianter: `.pill`, `.badge`, `.source`, `.curve-tag` (og `.chip`) | Slå sammen til `.tag` (18 px, nøytral, aksent eller advarsel) og `.chip` (22 px, for instruksjonssett) |
| K8 | P2 | Stolper (`.bar`) | 16 px høye med ramme, og tom skinne nesten usynlig. De virker tunge ved siden av tekst på 13 px | 8 px høye, radius 4, uten ramme, skinne `--track` med bedre kontrast. Etiketten med prosent står til høyre uten boks |
| K9 | P2 | Statistikkfliser (`.stat`) | Etikett med store bokstaver over tallet og ramme rundt hver flis | Tall i 24 px med enheten i `--muted`, etiketten under i `--fs-sm` og ingen ramme. Skill flisene med en tynn loddrett linje |
| K10 | P3 | Lenker | «Rediger kurven» er en understreket web-lenke midt i en beskrivelse | Bruk en flat knapp med pil (`Rediger kurve →`) eller la hele raden lede til Viftekurve |

### 2.6 Tomtilstander og lastetilstander

- **P1 [LAT]:** Før data kommer står det «–» i hvert felt, og toppfeltet sier «Kobler til …». Når den første prøven kommer, hopper alt på plass. Vis en rolig plassholder (grå blokk med samme bredde som en typisk verdi, uten skimmer) i maks 1 s, og deretter verdiene.
- **P1 [LAT]:** Grafer og «Siste minutt» er tomme fra venstre de første sekundene. Det ser ødelagt ut. Tegn et svakt skravert felt med teksten «Samler data …» der det ikke finnes historikk ennå, eller vis bare tidsrommet som finnes (aksen vokser til 60/120 s).
- **P1 [LAT]:** Sparklinjene på Sensorer ligger helt ute til høyre (ca. 90 px av 950) rett etter lasting, og GPU-gruppen og CPU-gruppen begynner på ulike x-posisjoner. Se Sensorer.
- **P2:** Profiler: fem identiske «Tom plass»-kort med deaktivert «Last» og «×» er tomtilstand × 5. Se Kontroller.
- **P2:** Hvis serveren dør, står det bare «Frakoblet – prøver igjen …» i rød tekst i toppfeltet. Bruk en infolinje under toppfeltet med «Prøver igjen om 3 s» og en «Prøv nå»-knapp, og demp verdiene (opacity .5) så det er tydelig at de er gamle.

---

## 3. Skall: toppfelt og sidefelt

### 3.1 Hva som er galt i dag

| # | Pri | Hva | Hvor |
|---|---|---|---|
| S1 | P1 | Ett toppfelt går over hele bredden, og kantlinja krysser sidefeltet. GNOME-apper med sidefelt (Innstillinger, Nautilus, Disker) har ett toppfelt for sidefeltet og ett for innholdet, med en loddrett skillelinje hele veien opp | `.app` grid-areas `'top top'`, `.headerbar` |
| S2 | P1 | Sidetittelen står to ganger: «Oversikt» midt i toppfeltet og «Oversikt» som h1 rett under | `#hb-title` og `.page-head h1` |
| S3 | P1 | «NVIDIA GeForce RTX 2080 · driver 610.43.02» står midt i toppfeltet på **alle** sider, også System, Innstillinger og Lys. Det er feil sted for kontekst, og det tar den plassen der en tittel skal stå | `#gpu-name` |
| S4 | P1 | Høyre del av toppfeltet er tom (bare frakoblet-varselet). Sidehandlinger som «Kopier som tekst», «Pause», «60 s / 120 s» og «Nullstill» står i stedet i en egen rad under (`.head-tools`), på linje med en undertittel | `.page-head` |
| S5 | P1 | Sidefeltet: ca. 550 px tomrom mellom «Lys (RGB)» og «Innstillinger» på 1080p | `.nav-spacer` |
| S6 | P1 **[LAT]** | Ikonsidefeltet (under 860 px) har ingen verktøytips, og «Lys» og «Innstillinger» har samme ikon | `.nav-item` mangler `title`/tooltip |
| S7 | P2 | Tastene 1–9 står ved hver menylinje. Det er støy som sjelden brukes, og det trekker blikket bort fra navnene | `.nav-item kbd` |
| S8 | P2 | Ministatistikken «GPU 37 °C · 23 %» blander to størrelser i én verdi. «Strøm» er GPU-ens effekt, men står som egen linje som om den gjaldt hele maskinen. Den kan ikke klikkes og kan ikke velges | `.side-live`, `renderSide()` |
| S9 | P2 | Versjonen «alpha v0.0.3 · bygg 3» står i 11,5 px `--dim` (3,2:1) nederst i sidefeltet og gjentas to ganger på Innstillinger | `.side-ver` |
| S10 | P2 | Logoen er 20 px, tegnet med strek i aksentfarge. Den virker spinkel ved siden av appnavnet i 600 | `.brand-mark` |
| S11 | P2 | Seksjonen «KONTROLLER simulert» blander store bokstaver og små i samme etikett | `.nav-sec .sim-word` |
| S12 | P2 **[LAT]** | Sidebytte flytter innholdet sidelengs: noen sider ruller og får vanlig rullefelt (15 px), andre ikke. Bredden endres derfor når du bytter side | `.content` mangler `scrollbar-gutter: stable` |

### 3.2 Forslag til nytt skall

```
┌─ sidefelt 232 px ─────────────────┐┌─ innholdets toppfelt 47 px ───────────────────────────────────┐
│ [■] Agentic Rigg  alpha     [≡]   ││  Sensorer                      [⟲ Nullstill] [⌄] [⧉]  ● Simulert │
├───────────────────────────────────┤├────────────────────────────────────────────────────────────────┤
│ ┌───────────────────────────────┐ ││                                                                │
│ │ ▣ GeForce RTX 2080            │ ││   (innhold, ingen h1/undertittel)                              │
│ │   ● Live · driver 610.43.02   │ ││                                                                │
│ └───────────────────────────────┘ ││                                                                │
│ Overvåking                        ││                                                                │
│  ▦ Oversikt                       ││                                                                │
│  🌡 Sensorer                      ││                                                                │
│  ∿ Grafer                         ││                                                                │
│ Maskinvare                        ││                                                                │
│  ▣ Grafikkort                     ││                                                                │
│  ▢ System                         ││                                                                │
│ Kontroller · simulert             ││                                                                │
│  ☰ Kontroller                     ││                                                                │
│  ✣ Viftekurve                     ││                                                                │
│  ◐ Lys                            ││                                                                │
│                                   ││                                                                │
│ ── Status ─────────────────────── ││                                                                │
│  GPU      37 °C  ▁▂▂▃   23 %      ││                                                                │
│  Effekt   36 W   ▁▁▂▁             ││                                                                │
│  CPU      63 °C  ▂▃▂▂   22 %      ││                                                                │
│  Minne    27,1 / 47 GiB           ││                                                                │
└───────────────────────────────────┘└────────────────────────────────────────────────────────────────┘
```

Konkret:

1. **Del toppfeltet i to** (P1). `.app` får `grid-template-areas: 'sidehead mainhead' 'side main'`. Sidefeltets toppfelt har sidefeltfargen, innholdets toppfelt har vindusfargen, og en loddrett linje går fra topp til bunn. Høyde 47 px, som i Adwaita.
2. **Sidefeltets toppfelt:** appikonet i full farge (`icon.svg`, 20 px, radius 5), «Agentic Rigg» i 600, et lite `alpha`-merke (11 px, `--muted`, ramme) og helt til høyre en hovedmenyknapp (≡) med **Innstillinger (Ctrl+,)**, **Tastatursnarveier (Ctrl+?)** og **Om Agentic Rigg**. Versjonen flyttes ut av sidefeltet og står bare i Om-dialogen. Den kan også stå i verktøytipset på appnavnet.
3. **Innholdets toppfelt:** sidetittelen til **venstre** i `--fs-title`/600. Sidehandlingene står til høyre som ikonknapper med verktøytips: Kopier (⧉), Pause/Spill (⏸), Tidsvindu (segment 60 s / 120 s / 5 min), Nullstill (⟲), Fold sammen alle (⌄). Ytterst til høyre står et statusmerke **bare når noe er unormalt**: «● Simulert GPU» (gult), «● Frakoblet» (rødt). Når alt er live og normalt, står det ingenting.
4. **Fjern `.page-head` (h1 + lead)** på alle sider (P1). Det gir 60–70 px mer høyde per side. Undertekster som er nyttige, flyttes: «Min, maks og snitt regnes fra …» blir verktøytips på «Statistikk siden 20:23:31». Simulert-advarselen blir én infolinje (se Kontroller). Resten slettes.
5. **Enhetskort øverst i sidefeltet** (P1, svaret på «driver o.l. øverst i midten»). Kortet viser GPU-navnet uten «NVIDIA» (`GeForce RTX 2080`), og under det i `--fs-sm` «● Live · driver 610.43.02». Prikken er grønn når kilden er live og gul når den er simulert. Klikk åpner en liten popover med VBIOS, CUDA, PCIe nå/maks, datakilde, intervall, en Live/Simulert-veksler og lenken «Vis alt om kortet →» til Grafikkort. Dette er stedet for driver, GPU-navn og datakilde. Fjern dem fra toppfeltet. Når flere GPU-er støttes senere, blir kortet en velger.
6. **Statusblokk nederst i sidefeltet** (P2), erstatter `.side-live`:
   - én størrelse per rad: etikett i `--muted` (venstre, fast 56 px), verdi høyrejustert med tabulære sifre og en 40×12 px sparklinje for siste 60 s
   - temperatur og last står i to faste kolonner, slik at tallene ligger rett under hverandre
   - raden kan klikkes og går til riktig sensor på Sensorer
   - hvilke 2–5 verdier som vises, velges i Innstillinger (GPU-temp, GPU-last, GPU-effekt, VRAM, CPU-temp, CPU-last, minne, nett ned/opp)
   - verdien blir gul og rød ved terskler (samme terskler som stolpene)
7. **Innstillinger ut av navigasjonslista** og inn i hovedmenyen og Ctrl+, (P1). Da forsvinner det store tomrommet, og statusblokken får bunnen. Hvis brukeren vil ha Innstillinger synlig, kan den heller stå som en flat rad med tannhjul *under* statusblokken, uten nummer.
8. **Tallene 1–8 skjules** (P2). Vis dem i verktøytipset («Sensorer – 2») og i Tastatursnarveier-vinduet. La en innstilling slå dem på.
9. **Seksjonsetiketter** med vanlig skrift: «Overvåking», «Maskinvare», «Kontroller · simulert» i `--fs-sm`/600/`--muted` (P2).
10. **Ikonsidefelt under 860 px** (P1): verktøytips med navn og snarvei. Enhetskortet blir bare et ikon med statusprikk. Statusblokken skjules.
11. **`.content { scrollbar-gutter: stable; }`** (P1 **[LAT]**), så innholdet ikke flytter seg sidelengs ved sidebytte.

---

## 4. Side for side

### 4.1 Oversikt (`#page-oversikt`, `pages/overview.js`)

Målt på 1920×1080: venstre kolonne slutter ved y=733, høyre ved y=680, og vinduet er 1080. **Ca. 350–400 px er tomme nederst.** På 1280×800 er siden 39 px for høy, så du må rulle for å se bunnen av «Siste minutt». Den nesten-passer, og det er verst.

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Død plass nederst. Grafen «Siste minutt» er 190 px fast | `.chart-box-ov { height: 190px }` | Gjør Oversikt til et rutenett som fyller høyden: øverst en **nøkkelrad** med 6 fliser (Temperatur, GPU-last, Effekt, Kjerneklokke, VRAM, Vifte), hver med stort tall og en liten sparklinje. Under den står to kolonner med detaljer, og grafen tar resten av høyden (`flex: 1; min-height: 200px`). På 1280×800 skal alt få plass uten rulling |
| P1 | «Mot nedklokking 37 %» regnes som temp/97 °C fra 0 °C. Det betyr ingenting for brukeren **[LAT]** | `onSample`: `s.temp / sd` | Vis en temperaturskala fra 30 °C til avstenging (100 °C) med markører for Mål (83) og Nedklokking (97), og en prikk for nå. Eller skriv rett ut: «60 °C under nedklokking» |
| P1 | Grafen blander °C og % på samme akse 0–100 uten enhet | `#pulse-canvas` | To små grafer side om side, eller to y-akser med enhet («%» til venstre, «°C» til høyre). Enklere: bare GPU-last og effekt i %, og temperatur i en egen graf |
| P1 | Pillen «Simulert oppsett: 245 W · 0/0 MHz · 83 °C · vifte auto» står på en overvåkingsside og er kryptisk («0/0 MHz») **[LAT]** | `.head-tools .pill`, `renderApplied()` | Fjern den fra Oversikt. Simulerte verdier hører hjemme på Kontroller. Hvis den skal være med: én linje i en «Kontroller (simulert)»-gruppe med etiketter («Kjerne +0 MHz, Minne +0 MHz») |
| P2 | «Navn» gjentar GPU-navnet som allerede står i enhetskortet. Driver, VBIOS og PCIe står både her og på Grafikkort | Skjermkort-gruppen | Behold bare det som endrer seg: PCIe nå (x16 3.0, som endrer seg), P-tilstand, VRAM og laster. Navn, driver og VBIOS flyttes til enhetskortet og Grafikkort |
| P2 | Strøm har 6 felt (Trekk, Grense, Standard, Min, Håndhevet, Maks) i en tre-kolonners rute der etikettene har ulik bredde, så feltkantene ikke ligger på linje (synlig i 840 px) **[LAT]** | `.grid-3 .cell` | Én stolpe «36 W av 245 W» med grense-markør. Min/Standard/Maks står i én dempet linje under («Område 105–275 W · standard 245 W»). «Håndhevet» vises bare når den avviker fra Grense |
| P2 | Klokker «nå / maks» har to kolonner uten kolonneoverskrift. «Maks» er en fast verdi som gjentas | `.rows-3` | Tabell med overskriftene «Nå» og «Maks», tall høyrejustert. Enda bedre: stolper «1 080 av 2 190 MHz» |
| P2 | «Høyeste i økten» er lik «Nå» ved oppstart og forteller lite | `#ov-temp-peak` | Gjør flisen til «Høyeste · 5 min» eller vis maks i grafen |
| P2 | `stats-4` blir 2×2 på ≤1280 px, og Temperatur-gruppen blir høy | `@media (max-width:1280px) .stats-4` | Container query. Med nøkkelraden over trengs ikke flisene |
| P3 | Encoder/Decoder 0 % hele tiden tar to rader | | Slå dem sammen til «Video (enc/dec) 0 % / 0 %», eller skjul dem når de er 0 med en «Vis alle»-lenke |

### 4.2 Sensorer (`#page-sensorer`, `pages/sensors.js`, `.sensors`)

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Sparklinjekolonnen er ca. 950 px bred × 18 px høy på 1920. Linjene blir flate, og øyet må vandre 1 000 px fra navn til graf. Rett etter lasting tegnes de bare i de siste ca. 90 px, så midten av tabellen er tom **[LAT]** | `col.c-spark { width:auto }`, `drawSpark` (`x0 = W − (len−1)·step`) | Gi sparklinja fast bredde (200–240 px) og 20 px høyde. Sett `max-width` på tabellen (ca. 1 000 px) **eller** bruk to tabeller i kolonner på brede vinduer (GPU + Minne + Nett til venstre, CPU + hwmon til høyre). Tegn de første sekundene fra venstre med en svak «samler»-flate, ikke ute til høyre |
| P1 | GPU-sparklinjene begynner ved x≈935, CPU-linjene ved x≈870. Ulik historikklengde gir ulik startkant **[LAT]** | `cap: 120` (GPU, 0,5 s) og `cap: 60` (sys, 1 s) | Tegn mot tid, ikke mot antall prøver: x = (t − (nå − 60 s)) / 60 s. Da står alle linjer på samme tidsakse, også når intervallet endres. I dag betyr «Siste 60 s» 120 s eller 240 s når intervallet er 1 s eller 2 s **[LAT]** |
| P1 | CPU-gruppen har 16 rader «CPU 0–15» to ganger (last, deretter klokke), altså 32 rader med samme navn | `order.push … 'Last per tråd' / 'Klokke per tråd'` | Én rad per tråd: «Tråd 0 · 18,9 % · 2 087 MHz». Kolonnene Nå/Min/Maks/Snitt gjelder last, og klokken vises i en ekstra kolonne. Eller la undergruppen «Per tråd» være lukket som standard. Det halverer høyden (2155 → ca. 1 400 px) |
| P1 | Rå navn: `ucsi_source_psy_8_00081 · curr1`, `iwlwifi_1 · temp1`, `enp24s0 ned (nede)`, `wlo1 opp`, `Tctl (k10temp)` **[LAT]** | `sensors.js` (hwmon/net) | Lag en navnetabell: «USB-C-kontroller (skjermkort) · strøm», «Wi-Fi-kort · temperatur», «Ethernet (enp24s0) · ned – ikke tilkoblet», «Wi-Fi (wlo1) · ned», «CPU Tctl». Det rå navnet står dempet i en tredje linje eller i verktøytipset |
| P2 | Nett-enhetene hopper mellom B/s, KiB/s og MiB/s i samme kolonne, så tallene danser **[LAT]** | `fmtRate` | Fast enhet per rad, valgt ut fra maks (eller fra Innstillinger: «Mbit/s» / «MB/s»). Høyrejustert med tre gjeldende sifre |
| P2 | P-tilstand og Klokkebegrensning er tekstrader med tomme Min/Maks/Snitt-celler | | Flytt dem til en liten «Tilstand»-linje i gruppeoverskriften («P5 · Tomgang»), eller vis «tid i tilstand» (f.eks. P0 12 %, P5 88 %) |
| P2 | Snitt av PCIe-generasjon («2.0») gir ingen mening. Desimalpunktum her, men komma ellers | `F.gen` | Vis «—» i Snitt for diskrete verdier. Skriv generasjonen «Gen 3» |
| P2 | Overskriften «Siste 60 s» står til venstre med 22 px innrykk, men linjene starter langt til høyre | `.sensors thead th:last-child` | Når kolonnen har fast bredde, rettes dette av seg selv. Skriv «60 s» |
| P2 | «Fold sammen alle» bytter tekst og bredde («Fold ut alle»), så knappene ved siden av hopper **[LAT]** | `#sn-toggle` | Ikonknapp i toppfeltet (⌄/⌃) med verktøytips og fast bredde |
| P2 | «Ikke tilgjengelig»: tre-kolonners liste der midtkolonnen (260 px) brekker «Moduler, hastighet, timinger og temperatur» over to linjer | `.notes li` | To kolonner: venstre «Minne – moduler, hastighet og timinger» (600) og høyre forklaringen. Legg et lite ikon foran («krever root» = lås, «ikke støttet» = strek over sirkel) |
| P3 | Raden «Nå» har vekt 600 og resten 400. Det fungerer, men tallene står veldig tett (92 px kolonner) | `col.c-num` | 96–104 px. Vis Min og Maks i `--muted` |
| P3 | Ingen søk eller filter i en tabell med 70+ rader | | Søkefelt i toppfeltet (Ctrl+F), som filtrerer rader live |
| P3 | Ingen festing | | Stjerne på en rad fester den øverst («Festet»), og festede rader kan vises i sidefeltets statusblokk |

### 4.3 Grafer (`#page-overvaking`, `pages/monitor.js`, `charts.js`)

Målt: på 1920×1080 er ca. 170 px tomme nederst. På 1280×800 går siden til én kolonne og blir 1 694 px høy, selv om det er plass til to kolonner.

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Y-aksen deles i fire like deler av et fast område: 0–2 500 gir 625/1 250/1 875, og 0–8 500 gir 2 125/4 250/6 375 **[LAT]** | `charts.js draw()`: `min + (max−min)·i/yTicks` | Lag «pene» aksemerker (1-2-2,5-5 × 10ⁿ): 0/500/1 000/1 500/2 000/2 500 MHz og 0/2 000/4 000/6 000/8 000 |
| P1 | Faste områder gjør at temperaturen (37 °C på 20–100) og effekten (40 W på 0–300) blir flate streker langs bunnen | `defs[].range` | Valg per graf eller globalt: «Fast skala» eller «Tilpass til data» (område = min/maks i vinduet ± 10 %, avrundet til pene tall). Standard: tilpass for temperatur, klokke og effekt, fast for prosent |
| P1 | Grafene har fast høyde (170 px) og fyller ikke vinduet | `.chart-box { height:170px }` | Rutenettet fyller høyden: 3 rader × 2 kolonner, hver graf `flex:1`, med minimum 160 px |
| P1 | Bruddpunktet 1280 px gir én kolonne på 1280-vinduet | `@media (max-width:1280px) .chart-grid` | Container query: to kolonner fra 900 px innholdsbredde |
| P2 | Gruppetittelen (Temperatur), stort tall og «min · maks» står på to linjer. Det er dobbel overskrift | `.chart-head` | Én linje i gruppetittelen: «Temperatur», til høyre «37 °C» (stor) og «min 36 · maks 37» (dempet). Det sparer 30 px per graf |
| P2 | Bare GPU-grafer, selv om appen nå er «Rigg» | | Legg til CPU-last (total), CPU-temp (Tctl), Minne brukt og Nett. Vis grafene som en valgbar liste: «Legg til graf» med avkrysning, rekkefølgen huskes |
| P2 | Tidsvindu 60/120 s | `#mon-window` | 1 min / 2 min / 5 min / 15 min (krever lengre historikk på serveren, 15 min ved 1 s er bare 900 punkter) |
| P2 | Tidsaksen viser «−120 s» helt til venstre, klemt mot y-aksen | `clamp(x, padL + 14, …)` | Vis «−2 min», «−1 min» og «nå». Det første merket kan droppes når det kolliderer |
| P3 | Hover viser verdier (bra), men det er ingen felles hårkors over alle grafer | | Synkronisert hårkors: når du holder over én graf, vises samme tidspunkt i alle |
| P3 | «Pause» bytter tekst til «Fortsett», og bredden hopper | `#mon-pause` | Ikonknapp ⏸/▶ med fast bredde. Mellomrom-tasten slår av og på |

### 4.4 Grafikkort (`#page-grafikkort`, `pages/gpu.js`)

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Hver verdi står i et 680 px bredt felt, også «46», «7.5» og «64» | `.rows .val` | Bruk `.kv` (K1). Verdiene trenger ikke full bredde. Spesifikasjonen kan stå i **to** kv-kolonner innenfor gruppen («Brikke» ved siden av «Minne»), så høyre gruppe går fra 1 070 til ca. 560 px |
| P1 | Klokketabellen har kolonnen «Standard» der alle fire verdiene er «ikke oppgitt». Det er en helt død kolonne **[LAT]** | `#gc-clocks` | Fjern kolonnen og behold forklaringen i en fotnote. «Boost (ref.)» står bare for Grafikk og skal merkes som referanse (samme merke som spesifikasjonen) |
| P2 | Forklaringer (`.val-explain`) står som brødtekst under feltene og bryter rytmen. Noen går over to linjer | PCI-ID subsystem, PCIe, Vifter, Minneleverandør, TDP | Lite (i)-ikon etter verdien med verktøytips/popover, eller nummererte fotnoter nederst i gruppen. Den viktigste (PCIe 2.0 i tomgang) kan stå som dempet sekundærtekst i samme rad |
| P2 | «Merke og arkitektur: GeForce · Turing» og «Arkitektur: Turing» står i begge kolonner. Navnet står både som «NVIDIA GeForce RTX 2080» og «GeForce RTX 2080 Rev. A» | | Gjør forskjellen tydelig: venstre «Målt/fra driveren», høyre «Referanse». Ikke gjenta Arkitektur |
| P2 | Siden blir 1 247 px høy på 1080p fordi Grenser ligger under Klokker til venstre, mens høyre kolonne slutter ved 1 068 | | Flytt Grenser til høyre under Spesifikasjon når den blir kortere, eller bruk tre kolonner på ≥1700 px innholdsbredde |
| P2 | UUID er maskert med punkter og har en «Vis»-knapp inne i feltet | | Bra idé. Bruk øye-ikonet (eye/eye-off) og legg til en kopier-knapp |
| P3 | «Kopier som tekst» står alene i en egen rad | | Ikonknapp i toppfeltet. Toasten sier «Kopiert, 42 linjer» |
| P3 | Ingen eksport | | Meny: «Kopier som tekst», «Kopier som Markdown», «Lagre som JSON» |

### 4.5 System (`#page-system`, `pages/system.js`)

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Toppfeltet viser GPU-navn og driver på System-siden | `#gpu-name` | Løses av skallet |
| P1 | Rå verdier: «ubuntu:GNOME», «#31-Ubuntu SMP PREEMPT_DYNAMIC Sat Aug 1 04:26:38 UTC 2026», «6. mars 2019 (03/06/2019)», «0x800820d» **[LAT]** | `#sy-os`, `#sy-board`, `#sy-cpu` | «GNOME (Ubuntu-økt)», «Bygg #31 · 1. aug. 2026» med hele strengen i verktøytipset, bare «6. mars 2019», og mikrokoden kan stå, men med etiketten «Mikrokode-revisjon» |
| P2 | «Node.js (server)» står under Operativsystem | `#sy-os` | Flytt til Om |
| P2 | Klokke- og lastflisene: 16 fliser i rader på 6 (6+6+4), så siste rad er hakkete. Stolpen viser last, men det står ikke | `.core-grid` `minmax(118px,1fr)` | 8 kolonner × 2 rader (`repeat(8, 1fr)`). Hver flis: «T0» (`--muted`), klokke i 600, stolpe = last med prosent til høyre. Gi stolpen en legende («stolpe = last»). Farge etter last |
| P2 | Cache-tabellen høyrejusterer tekstkolonner | `.grid-table` | Se K5 |
| P2 | Swap 8,0 av 8 GiB vises som full gul stolpe uten forklaring | `#sy-mem` | Vis «Full» med verktøytips («Swap er helt brukt. Systemet kan bli tregt.»). Gult er riktig |
| P3 | Instruksjonssett: «Vis alle (110)» er bra. Brikkene i mono når alle vises | | Søkefelt når alle vises. Fremhev de 10 viktigste |

### 4.6 Kontroller (simulert) (`#page-kontroller`, `pages/controls.js`)

Målt: ca. 260 px tomme nederst på 1920×1080. På 1280 blir siden 1 426 px høy.

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Simulert-meldingen står tre ganger på én skjerm: pillen «Simulert – ingenting sendes til kortet», infolinja «Bruk lagrer verdiene i appen …» og sidefeltet «KONTROLLER simulert». I tillegg sier hver toast det **[LAT]** | `.pill-sim`, `#ctl-banner` | Behold én infolinje øverst (nøytral med (i)-ikon, kan lukkes, husket). Behold sidefeltets «· simulert». Fjern pillen. Toasten sier bare «Lagret (simulert)» |
| P1 | Gruppen heter «Innstillinger», akkurat som siden Innstillinger | `.ctl-layout .group-title` | «Skjermkort» eller «Justeringer» |
| P1 | Hver kontroll bruker 105 px høyde: tittel og beskrivelse, så et felt helt til høyre, så en glider i full bredde (980 px for 170 W) og en skala under | `.ctl`, `.ctl-top`, `.slider-wrap` | Én rad à ca. 56 px: venstre kolonne 280 px med tittel og beskrivelse, midten glider (maks 520 px) med standardmerke, høyre tallfelt 96 px og ↺. Skalaen (min/maks) står i verktøytipset og som små tall ved glideren. Fem kontroller blir ca. 300 px i stedet for 525 |
| P1 | «Last» betyr «last inn», men «last» betyr belastning overalt ellers i appen («GPU-last») **[LAT]** | `controls.js:194` | «Hent» eller «Bruk profil». Og «Lagre» → «Lagre her» |
| P1 | Tøm profil (×) skjer straks, uten angre **[LAT]** | `controls.js:196` | Toast «Profil 2 tømt · Angre» i 6 s |
| P2 | Fem tomme profilkort med deaktivert «Last» og «×» | `.slots` | Fylte profiler som kort. Tomme plasser som én kompakt rad: «+ Lagre nåværende som profil 3». Profilnavnet redigeres ved dobbeltklikk eller med blyant-ikon, ikke som et usynlig felt |
| P2 | «Gjeldende» gjentar verdiene fra skjemaet. Når de er like, er gruppen ren støy | `#applied-list` | Vis bare forskjeller: «Effektgrense 245 W → 260 W». Uten endringer: «Ingen ulagrede endringer» |
| P2 | Bunnlinja (Forkast/Tilbakestill/Bruk) ruller bort på 1280 | `.ctl-foot` | Gjør den klebrig nederst i innholdet (`position: sticky; bottom: 0`) med en skillelinje, og vis den bare når det er endringer. Fram til da: «Ingen endringer» (dempet) |
| P2 | Viftegliderens deaktivering i Auto forklares ikke ved glideren **[LAT]** | `.ctl-fan` | Tekst ved glideren: «Velg Manuell for å sette fast fart». Feltet og glideren skjules helt i Auto, eller viser kurvens verdi nå |
| P2 | «· endret» settes etter tittelen med CSS `::after`, og linja hopper | `.ctl.changed h3::after` | En prikk (6 px, `--warn`) foran tittelen og ↺ ved feltet. Ikke tekst som endrer bredden |
| P3 | Mer å vri på (simulert): spennings-/frekvenskurve (V/F) som egen visning, «Strømgrense i %» som alternativ til W, «Bruk ved oppstart» per profil, hurtigtaster for profiler (Ctrl+1–5) | | Bare hvis det passer i demoen. V/F-kurven er det mest «pro» |

### 4.7 Viftekurve (`#page-viftekurve`, `fancurve.js`, `pages/fancurve-page.js`)

Målt: ca. 360 px tomme nederst på 1920×1080 fordi kurven er `min(58vh, 520px)`.

| Pri | Hva | Hvor | Forslag |
|---|---|---|---|
| P1 | Kurven fyller ikke høyden | `.curve-wrap { height: min(58vh,520px) }` | Fleks til bunnen av vinduet (minst 320 px). Siden skal ikke rulle på 1080p eller 800p |
| P1 | «20°C» på aksen og «37°C → 37 %» i verktøytipset står uten mellomrom (se T6). Punkttabellen har «Temp (°C)» i overskriften og «30 °C» i hver celle **[LAT]** | `fancurve.js`, `#fc-points` | Enheten står bare i overskriften, og cellene viser bare tall. Aksetittel «Temperatur (°C)» og «Vifte (%)» |
| P2 | Forhåndsvalg vises som segmentkontroll, men er handlinger og ikke en tilstand (ingen er valgt) | `#fc-presets.seg` | Tre vanlige knapper, eller en nedtrekksliste «Last forhåndsvalg …». Hvis kurven er lik et forhåndsvalg, vis navnet i gruppetittelen («Kurve · Balansert») |
| P2 | Punkttabellen kan bare leses | `#fc-points` | Tallfelt i tabellen (temperatur og %), slik at du kan skrive verdiene. Legg til «+ Legg til punkt» og slett-knapp per rad. Bra for tastatur og presisjon |
| P2 | Ingen sammenligning med kurven som er brukt | | Mens du redigerer, vises den brukte kurven som en stiplet grå «spøkelseskurve», slik GWE gjør |
| P2 | Pillen «Vifte: Auto – kurven er aktiv · simulert, styrer ikke den ekte viften» er lang og gjentar simulert | `#fan-mode-chip` | Dette står i infolinja. Behold «Auto · kurve aktiv» som merke i gruppetittelen |
| P2 | Mer å vri på (simulert): **hysterese** (°C), **reaksjonstid** opp/ned (s), **minste fart** og **temperaturkilde** (GPU / Hotspot / CPU når det finnes) | Ny gruppe «Oppførsel» | Disse finnes i ekte viftestyrere (FanControl, CoolerControl), gir mening og er lette å simulere |
| P3 | Hurtigtastteksten «Tab velger punkt · piltaster flytter …» står alltid synlig | `.kbd-hint` | Vis den bare når kurven har fokus, eller i et (?)-verktøytips |
| P3 | Punktene kan ikke smekkes til rutenettet | | Hold Shift ved dra for å smekke til 5 °C / 5 %. Vis koordinatene ved markøren mens du drar |

### 4.8 Lys (RGB). Se egen seksjon 6.

### 4.9 Innstillinger. Se egen seksjon 5.

---

## 5. Innstillinger

### 5.1 Problemer i dag

| Pri | Hva | Hvor |
|---|---|---|
| P1 | Kolonnen er venstrestilt og maks 860 px. På 1920 er **halve vinduet tomt** til høyre | `.settings { max-width: 860px }` |
| P1 | Datakilden står fire ganger: merket «● Live (nvidia-smi)», undertekst «Live: nvidia-smi.», segmentet Live/Simulert og raden «Datakilde» under Om **[LAT]** | `#source-badge`, `#set-source-reason`, `#about-source` |
| P1 | Om gjentar «Agentic Rigg / alpha v0.0.3 · bygg 3» i overskriften og deretter Navn, Versjon, Kanal og Bygg som felt | `#about-rows` |
| P1 | Bare fem innstillinger. Brukeren vil ha mer å vri på | |
| P2 | Temperaturenhet ligger under Utseende | |
| P2 | «Start i simulert modus» og «Datakilde nå» ligner hverandre, og forskjellen (nå vs. ved neste oppstart) er ikke tydelig | |
| P2 | Bytte av tema og enhet gir ingen bekreftelse, mens intervallbytte gir en toast. Det er inkonsekvent **[LAT]** | `settings.js` |

### 5.2 Ny struktur

Bruk oppsettet fra GNOME-innstillinger: én **sentrert** kolonne på maks 720 px med grupper (tittel over, «boxed list» med rader). Innstillingene har egne seksjoner i en **venstre underliste**, eller som faner i toppfeltet (AdwViewSwitcher-stil) når de blir mange. Med 6 seksjoner er en underliste til venstre i innholdet (180 px) best på 1920, og den blir faner på 1280.

1. **Generelt**
   - Startside: *Oversikt / Sensorer / Grafer / Sist brukte* (nedtrekk)
   - Oppdateringsintervall: 0,5 s / 1 s / 2 s (segment, AdwToggleGroup-stil)
   - Datakilde: én rad med «● Live · nvidia-smi» som undertekst og segmentet Live / Simulert. Raden «Bruk simulert ved oppstart» står rett under som bryter. Grunnen (`info.reason`) står bare når den sier noe nytt (f.eks. «nvidia-smi feilet 3 ganger»)
   - Vis simulerte kontroller i sidefeltet (bryter). Den skjuler hele gruppen Kontroller/Viftekurve/Lys for de som bare vil overvåke
2. **Utseende**
   - Tema: System / Lys / Mørk, vist som tre små forhåndsvisningskort, slik GNOME gjør. Det er en detalj som føles gjennomtenkt
   - Aksentfarge: 9 runde fargeprøver med GNOMEs aksentfarger (blå, blågrønn, grønn, gul, oransje, rød, rosa, lilla, skifer). Merk: nettleseren kan ikke lese systemets aksentfarge, så «Følg system» går ikke før Electron
   - Tetthet: Kompakt / Standard / Komfortabel (radhøyde 28 / 32 / 36 px, gruppeluft 8 / 12 / 16)
   - Animasjoner: Full / Redusert / Av (standard: følg `prefers-reduced-motion`)
   - Vis hurtigtaster i sidefeltet (bryter, av som standard)
3. **Enheter og format**
   - Temperatur: °C / °F
   - Desimaler: Auto / 0 / 1 (gjelder temperatur, effekt og prosent i tabeller)
   - Minne: binær (GiB) / desimal (GB)
   - Nettrate: byte/s (MB/s) / bit/s (Mbit/s)
   - Klokkeslett: følg systemet / 24 t / 12 t. Det brukes bare i «Statistikk siden …» og i grafverktøytips, så la det være «følg systemet» og ikke vis valget før det trengs
4. **Sidefelt**
   - Status nederst: velg 2–5 verdier (avkrysning, rekkefølge med dra)
   - Vis sparklinjer i status (bryter)
   - Varseltersker for farge: GPU-temp gul/rød (standard 75/85 °C), CPU-temp (80/90), last (90 %)
5. **Grafer og sensorer**
   - Standard tidsvindu: 1 / 2 / 5 / 15 min
   - Y-akse: tilpass til data / fast
   - Fylt flate under linja (bryter)
   - Sparklinjer i Sensorer (bryter)
   - Husk min/maks/snitt mellom økter (bryter, krever at serveren lagrer)
   - Grupper som er lukket ved start (avkrysning)
6. **Om og data** (eller Om som dialog fra hovedmenyen)
   - Om: appikon 64 px, navn, «alpha 0.0.3 (bygg 3)», én setning om hva appen er, lenker til Endringslogg (PATCHLOG) og Status. Node-versjon, datamappe og datakilde under «Feilsøking» (kan foldes ut)
   - Datamappe: stien vist med `--mono` og en kopier-knapp
   - Eksporter / importer innstillinger (JSON)
   - Tilbakestill alle innstillinger (destruktiv knapp med bekreftelse)

### 5.3 Verdt det, og hva som er støy

| Verdt det | Støy (ikke lag) |
|---|---|
| Aksentfarge (9 faste, testet i begge temaer) | Fritt fargevalg for aksent: gir dårlig kontrast |
| Tetthet | Egen skriftstørrelse: GNOME har tekstskalering, og Ctrl+/− (zoom) dekker resten. Støtt heller Ctrl+/−/0 |
| Startside | Skriftvalg |
| Statusverdier i sidefeltet | Rekkefølge på sider med dra: 8 sider, liten gevinst. Skjul simulerte sider er nok |
| Graftidsvindu og y-akse | Linjetykkelse og egne seriefarger per graf |
| Enheter (°C/°F, GiB/GB, MB/s/Mbit/s) | 12/24-timers klokke: appen viser nesten ikke klokkeslett, så følg systemet |
| Desimaler: Auto/0/1 | Desimaler per sensor |
| Animasjoner: Full/Redusert/Av | Egne animasjonsvarigheter |
| Varseltersker for statusfarger | Lydvarsler (ikke ennå) |
| Vis simulerte kontroller | «Vis tastaturtall» i tillegg til hurtigtastvinduet. Valgfritt, lav pri |

**Detaljer:** hver innstilling trer i kraft straks og lagres uten toast. Toast bare når noe skjer som ikke synes (f.eks. «Gjelder neste gang serveren starter»). Rader med segmenter og brytere skal ha samme høyre kant. Hele raden med bryter kan klikkes.

---

## 6. RGB-siden: nytt oppsett

### 6.1 Problemer i dag

| Pri | Hva | Hvor |
|---|---|---|
| P1 | Forhåndsvisningen er en hvit boks på 1 290 × 360 px med kortet i midten. Kontrollene er en smal kolonne på 360 px til høyre. **Ca. 500 px er tomme nederst** på 1080p | `.rgb-layout`, `.rgb-stage` |
| P1 | Hex-feltet «#3DF5C2» ser ut som et felt, men kan ikke redigeres (det er `.val`). Du må klikke på fargeprøven ved siden av, og det åpner nettleserens fargevelger **[LAT]** | `#rgb-hex`, `.color-pick` |
| P1 | Fargeprøvene er neonpaletten fra «gamery»-tida (`#3df5c2`, `#ff3df0`, `#ff3d5a`). Standardfargen er neon-mint | `lights.js SWATCHES` |
| P1 | Pillen «Sannsynlig Palit-kontroller funnet i research – ikke testet på kortet» er utviklerprat **[LAT]** | `#rgb-status` |
| P2 | Hastighet er nedtonet for Statisk uten forklaring **[LAT]** | `.mini-ctl.off` |
| P2 | Effektmerknaden («Statisk og Pust er dokumentert …») står som brødtekst og brekker over to linjer | `.group-note` |
| P2 | Lysstyrke og hastighet bruker en egen `mini-ctl`-komponent med verdien i fet skrift til høyre. Kontroller-siden bruker en annen glider med tallfelt. Ulik komponent for samme ting **[LAT]** | `.mini-ctl` vs `.ctl` |
| P2 | Ingen soner. Kortet har en lyslist og to vifteringer, men alt styres samlet | |
| P3 | Viftene snurrer etter live viftefart (fin detalj), men det sies bare i en fotnote | |

### 6.2 Forslag

```
┌ toppfelt: Lys                                        [Forkast] [Bruk]  ● Simulert ┐
├────────────────────────────────────────────────────────────────────────────────────┤
│ (i) Lyskontrolleren er ikke koblet til. Endringer vises bare i forhåndsvisningen.  │
│ ┌ Forhåndsvisning ─────────────────────────────────────────────────────────────┐   │
│ │            [ kortet, maks 300 px høyt, mørk scene i begge temaer ]           │   │
│ │  Sone:  (Alle)  (Lyslist)  (Vifte 1)  (Vifte 2)        ◐ vis bakgrunnslys    │   │
│ └──────────────────────────────────────────────────────────────────────────────┘   │
│ ┌ Effekt ──────────────┐ ┌ Farge ────────────────────┐ ┌ Justering ─────────────┐   │
│ │ ◉ Statisk     ✓ støt.│ │ [ metning/lyshet-flate ]  │ │ Lysstyrke ───●── 80 %  │   │
│ │ ○ Pust        ✓ støt.│ │ [ fargetone-glider     ]  │ │ Hastighet ──●─── 5     │   │
│ │ ○ Regnbue    forhånd.│ │ Hex [#3584E4]  [◧ pipette]│ │ Retning  ⟲ / ⟳         │   │
│ │ ○ Fargesyklus forhånd│ │ ● ● ● ● ● ● ● ● ●  (fast) │ │ (vises bare for        │   │
│ │ ○ Temperatur  forhånd│ │ ○ ○ ○ ○  (nylig brukte)   │ │  effekter som bruker   │   │
│ │ ○ Av                 │ │                           │ │  dem)                  │   │
│ └──────────────────────┘ └───────────────────────────┘ └────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────────────────┘
```

1. **Forhåndsvisningen øverst i full bredde, maks 300 px høy** (P1). Scenen er alltid mørk (`#16181b`) i begge temaer, fordi LED-er bare ser riktige ut på mørkt. Den hvite boksen i lyst tema får lysene til å se blasse ut. Valgfri «bakgrunnslys» (svak farget glød bak kortet, med bryter og av som standard) er stedet der litt «stemning» er lov.
2. **Soner** som brikker under forhåndsvisningen: Alle / Lyslist / Vifte 1 / Vifte 2. Den valgte sonen markeres i forhåndsvisningen (resten dempes til 30 %). Simulert, men det gir mening og er mer å vri på.
3. **Effekt som liste med radioknapper**, ikke segmenter. Hver rad viser navn, en liten animert prøve (16×16) og merket «Støttet» eller «Bare forhåndsvisning». Det erstatter merknaden i brødtekst. Ny effekt **Temperatur**: fargen går fra kald til varm etter GPU-temperaturen (to fargestopp og et område, f.eks. 40–80 °C). Den knytter RGB til det appen faktisk handler om, og er en «gjennomtenkt» detalj.
4. **Farge:** egen fargevelger (metning/lyshet-flate + fargetone-glider, inline og ikke nettleserens dialog), redigerbart hex-felt med validering (godtar «3584e4», «#3584E4» og «rgb(…)») og 9 faste fargeprøver. Bruk nøkterne, rene LED-farger: hvit, rød, oransje, gul, grønn, cyan, blå, lilla og rosa, mettet nok til LED men ikke neon-mint som standard. Under: 4–6 «nylig brukte» fargeprøver.
5. **Justering:** samme glider-med-tallfelt-komponent som Kontroller (K4). Kontroller som ikke gjelder effekten, **skjules** («Hastighet» vises ikke for Statisk) i stedet for å nedtones uten forklaring.
6. **Forkast og Bruk i toppfeltet** (eller i en klebrig bunnlinje som på Kontroller), med samme mønster for skitne endringer på alle sider: prikk og «Ikke brukt».
7. **Statusteksten:** «Lyskontroller: ikke koblet til (simulert)». Research-detaljen («Palit-kontroller, ikke testet») står i et (i)-verktøytips.
8. **På 1280:** forhåndsvisningen øverst (240 px), og de tre gruppene i to kolonner (Effekt | Farge, med Justering under Effekt).

---

## 7. Ikoner

**Anbefaling: Lucide (ISC-lisens), innebygd som inline SVG-sprite** i `web/icons.svg` (eller en `<svg><symbol>`-blokk i `index.html`), brukt med `<svg class="icon"><use href="#i-thermometer"/></svg>`. Strekene ligner det appen har i dag, og settet har nesten alt som trengs. Ingen CDN: kopier bare symbolene som brukes, og legg ISC-lisensteksten i `web/icons/LICENSE-lucide.txt`.

Felles regler: 24-rutenett vist i 16 px (sidefelt og knapper) eller 20 px (tomtilstander), `stroke-width: 1.75` (blir ca. 1,2 px ved 16 px), `stroke-linecap/linejoin: round`, `fill: none`, `color: currentColor`. Ingen opacity-triks (`.nav-item svg { opacity: .85 }` fjernes). Bruk heller `--muted` for inaktive og `--text` for aktive.

Alternativ: Tabler Icons (MIT) har samme stil og flere maskinvareikoner. Ikke bland settene.

Det finnes ikke noe godt skjermkortikon i settene (sjekk før dere velger). **Tegn ett eget** i Lucide-stil: et kort med to vifter (to sirkler i et avrundet rektangel og en brakett til venstre). Bruk det for Grafikkort og i enhetskortet.

| Sted | Ikon (Lucide-navn) |
|---|---|
| Oversikt | `layout-dashboard` (eller `gauge`) |
| Sensorer | `thermometer` |
| Grafer | `chart-line` (eller `activity`) |
| Grafikkort | **eget skjermkortikon** (se over) |
| System | `cpu` |
| Kontroller | `sliders-horizontal` |
| Viftekurve | `fan` |
| Lys (RGB) | `palette` (ikke sol, den er for lik tannhjul og lysstyrke) |
| Innstillinger | `settings` (tannhjul) |
| Hovedmeny | `menu` |
| Kopier | `copy` (kopiert: `check`) |
| Pause / spill | `pause` / `play` |
| Nullstill statistikk | `rotate-ccw` |
| Tilbakestill til standard | `undo-2` |
| Fold sammen/ut | `chevrons-down-up` / `chevrons-up-down`, chevron i grupper: `chevron-down` |
| Tøm profil | `trash-2` |
| Lagre profil | `save` |
| Vis/skjul UUID | `eye` / `eye-off` |
| Info, advarsel, feil | `info`, `triangle-alert`, `circle-alert` |
| Toast ok | `circle-check` |
| Live / simulert | fylt sirkel 8 px (ikke ikon) |
| Effekt (sidefelt-status) | `zap` |
| Minne | `memory-stick` |
| Nett | `arrow-down-up` |
| Krever root | `lock` |
| Ikke støttet | `circle-slash` |
| Legg til punkt | `plus` |
| Søk | `search` |

Appikonet (`icon.svg`): sekskanten med «A» er grei, men den ligner et generisk «AI»-merke. Vurder et ikon som sier maskinvare (et skjermkort eller en brikke med en pulslinje), tegnet etter GNOMEs retningslinjer for appikoner: 128 px, lett perspektiv, flate farger og én aksent. P3.

---

## 8. Bevegelse og mikrodetaljer (til animasjonsagenten)

### 8.1 Tokens

```css
:root {
  --dur-1: 80ms;   /* hover, trykk */
  --dur-2: 150ms;  /* små tilstander: bryter, segment, chevron, fokus */
  --dur-3: 200ms;  /* inn/ut av elementer: toast, popover, sammenlegging */
  --dur-4: 250ms;  /* større flater: sidebytte (bare opasitet) */
  --ease-out: cubic-bezier(0.2, 0, 0, 1);      /* inn i ro */
  --ease-in: cubic-bezier(0.4, 0, 1, 1);       /* ut */
  --ease-std: cubic-bezier(0.25, 0.1, 0.25, 1);
}
:root[data-motion='reduced'] { --dur-2: 0ms; --dur-3: 100ms; --dur-4: 0ms; }
:root[data-motion='off'] { --dur-1: 0ms; --dur-2: 0ms; --dur-3: 0ms; --dur-4: 0ms; }
@media (prefers-reduced-motion: reduce) { :root:not([data-motion='full']) { /* som 'reduced' */ } }
```

Den nåværende regelen `* { animation: none !important; transition: none !important }` er for brutal. Den fjerner også overganger som *hjelper* (opasitet på toast). Erstatt den med tokens. «Redusert» betyr: ingen forflytning eller skalering, bare korte opasitetsoverganger (maks 100 ms), og grafer som oppdateres i trinn uten jevn rulling.

### 8.2 Hvor animasjon hjelper

| Sted | Hva | Varighet og easing | Redusert |
|---|---|---|---|
| Hover på knapper, menylinjer og rader | bakgrunnsfarge | `--dur-1` `--ease-std` | uendret (bare farge) |
| Trykk | bakgrunn `--active`, ingen skalering | 0 ms inn, `--dur-1` ut | samme |
| Segment (toggle group) | den hevede «knappen» glir til nytt valg | `--dur-2` `--ease-out` (translateX og bredde) | hopp |
| Bryter | knotten glir, sporet skifter farge | `--dur-2` `--ease-out` | hopp |
| Fokusring | vises straks (ingen animasjon på fokus) | 0 | 0 |
| Sidebytte | bare kryssfade av innholdet. Toppfeltets tittel byttes straks | 120 ms ut-in, opasitet 0→1, **ingen** forflytning | 0 |
| Sammenlegging av grupper (Sensorer) | høyde via `grid-template-rows: 0fr → 1fr`, chevron roterer | `--dur-3` `--ease-out`. Chevron `--dur-2` | hopp, chevron hopper |
| Verdier som endres | **ingen** tweening av tall (tall skal ikke telle opp). Stolper glir som i dag (ca. 200 ms). Fargeskifte gul/rød over `--dur-3` | | stolper hopper |
| Toast | inn: opasitet + translateY(8px→0) + `scale(.98→1)`; ut: opasitet + translateY(4px). Stabelen glir opp når én forsvinner (FLIP) | inn `--dur-3` `--ease-out`, ut 150 ms `--ease-in` | bare opasitet 100 ms |
| Popover og verktøytips | opasitet + translateY(2px) | 120 ms inn, 80 ms ut. Verktøytips etter 500 ms ventetid, 0 ms ved neste element (gruppe-varme) | bare opasitet |
| Temabytte | ingen overgang på farger (gir sølete mellomfarger og lag i canvas). Ev. `document.startViewTransition` med kryssfade 200 ms | 200 ms | ingen |
| Viftekurve: dra | punktet følger pekeren uten easing. Hover: punktet vokser 1 → 1,3 i `--dur-1`. Under draging: `cursor: grabbing`, koordinatetikett ved punktet. Slipp: ingen «sprett» | | samme, uten vekst |
| Viftekurve: forhåndsvalg | kurven morfer fra gammel til ny (punktene interpoleres) | 250 ms `--ease-out` | hopp |
| RGB-forhåndsvisning | LED-ene oppdateres direkte. Bytte av effekt: kryssfade mellom gammel og ny farge på LED-ene | 250 ms | **statisk bilde**: vis effekten som stillbilde (regnbuen som gradient, pust på 60 %) og «Animasjon av» i hjørnet. Viftene står stille |
| Grafer | jevn rulling av tidsaksen er bra. På 240 Hz tegnes alle seks grafene hvert bilde | | trinnvis: tegn bare når ny prøve kommer |
| Skitne endringer | prikken og «Ikke brukt» toner inn | `--dur-2` | 0 |

**Hvor animasjon er støy (ikke lag):** tall som teller, sprett og elastikk, skimmer på skeletons, parallakse, animert gradient eller glød, ikoner som roterer ved hover, og sidefeltet som glir inn ved sidebytte.

**240 Hz:** test at hover og overganger ikke hakker, og at canvas-tegningen holder seg under ca. 1 ms per bilde. Vurder å begrense graftegningen til 120 fps når pekeren ikke er over en graf. P3.

### 8.3 Mikrodetaljer (sjekkliste)

- **Fokusringer (P1 [LAT]):** `:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; border-radius: inherit }` med `--focus: color-mix(in srgb, var(--accent) 65%, transparent)`. Må synes på valgt segment, på aksentknapper (ytre ring) og på canvas (viftekurven har det allerede). Fjern `outline: none` i `.slot-name:focus`.
- **Tastaturnavigasjon:** sidefeltet er én tab-stopp med piltaster opp/ned (roving tabindex). Hopp-lenke «Til innhold». Ctrl+, Innstillinger. Ctrl+? hurtigtaster. Ctrl+F søk i Sensorer. Mellomrom = pause på Grafer. Esc lukker popover. Ctrl+Enter = Bruk på Kontroller, Viftekurve og Lys. Ctrl+Z = forkast.
- **Verktøytips:** egen liten komponent (ikke `title`). Mørk boble, 12 px, maks 280 px bred, 500 ms ventetid. Påkrevd på alle ikonknapper, deaktiverte knapper (*hvorfor* de er deaktivert), avkortet tekst og ikonsidefeltet.
- **Tabulære tall:** `font-variant-numeric: tabular-nums` står på `body`, bra. Legg til `slashed-zero` bare i mono. Alle målte tall høyrejusteres (T7).
- **Enheter med smalt mellomrom:** U+202F mellom tall og enhet overalt (T5). Enheter i `--muted` og 0,9 em. Ikke hev enheten (`small` i `.stat-v` er greit, men skal stå på samme grunnlinje).
- **Minustegn:** `signed()` bruker riktig U+2212. Bruk det også på aksene («−60 s» står riktig) og i sensorverdier.
- **Ellipsis:** lange navn (sensornavn, GPU-navn i enhetskortet, profilnavn) får `text-overflow: ellipsis` og hele teksten i verktøytipset. Ingen tekst skal brekke midt i en tabellrad.
- **Tomtilstander:** én komponent `.empty` med 32 px ikon i `--dim`, én setning og eventuelt én knapp. Brukes for tomme profiler, «Ingen hwmon-sensorer», søk uten treff og graf uten data.
- **Lastetilstander og skeletons:** statiske grå blokker med `--track` og radius 4, i samme bredde som innholdet de erstatter. Ingen skimmer. Bytt til ekte innhold uten forflytning (samme høyde, ingen layout-hopp).
- **Rullefelt:** smale, Adwaita-lignende: `::-webkit-scrollbar { width: 10px }`, skinnen gjennomsiktig og tommelen `color-mix(in srgb, var(--text) 25%, transparent)` med radius 5 og 2 px innvendig kant (`border: 2px solid transparent; background-clip: content-box`). Ved hover 40 %. Pluss `scrollbar-gutter: stable` (S12).
- **Markering av tekst:** grensesnittet (`.sidebar`, `.headerbar`, etiketter, knapper og gruppetitler) får `user-select: none`. Verdier (`.kv` verdi, tabellceller, kodetekst) får `user-select: text`. `::selection { background: var(--accent-soft-strong); color: inherit }`. Dobbeltklikk på en verdi markerer hele verdien (`user-select: all` på korte ID-er som UUID, PCI-ID og VBIOS).
- **Kopier ved klikk:** høyreklikk eller et kopier-ikon ved hover på ID-er (UUID, PCI-ID, VBIOS, driver). Toast «Kopiert».
- **Cursor:** innebygde apper bruker vanlig pil på knapper. Bruk `cursor: default` på knapper, menylinjer og segmenter. `pointer` bare på ekte lenker. `grab`/`grabbing` på kurvepunkter, `crosshair` på grafer ved hover, `text` på felt og `help` bare på (i)-ikoner (ikke stiplede understreker i hele rader).
- **Deaktiverte kontroller:** alltid med verktøytips som sier hvorfor («Ingen endringer å forkaste», «Live er låst av AG_SIMULATE=1»).
- **Destruktive handlinger:** Tøm profil, Nullstill statistikk og Tilbakestill skal ha angre-toast (6 s) i stedet for bekreftelsesdialog.
- **Knappetekster som endres:** «Pause»/«Fortsett» og «Fold sammen alle»/«Fold ut alle» endrer bredden. Bruk ikoner med fast bredde eller `min-width` lik den lengste teksten **[LAT]**.
- **Klikkflater:** hele innstillingsraden kan klikkes for brytere. Hele sensorgruppeoverskriften kan klikkes (er det allerede).
- **Vindustittel:** `document.title` = «Sensorer · Agentic Rigg», bra. Legg til «(simulert)» når kilden er simulert.
- **Dokumentasjon [LAT]:** `README.md` sier fortsatt «alpha v0.0.2 · bygg 2». Ruten for Grafer heter fortsatt `#/overvaking`. Små ting, men de avslører også hastverk.

---

## 9. Prioritert gjennomføringsplan

Rekkefølgen er valgt slik at grunnmuren (tokens og komponenter) kommer før sidene, så sidene bare trenger å bygges om én gang.

1. **Tokens** (`styles.css :root`): skriftstabel (T1), skala (T2–T4), avstander, høyder og radier (2.2), fargeretting for kontrast (`--dim`, glideknott, `--track`) (2.3), `--focus` og bevegelsestokens (8.1). Erstatt alle løse px-verdier.
2. **Ikonsprite** (seksjon 7): Lucide-symboler og eget skjermkortikon, `.icon`-klasse, bytt ut alle inline-SVG-er. Lisensfil.
3. **Kjernekomponenter:** `.kv` for lesbare verdier (K1), knapper med `.flat`/`.suggested`/`.destructive`/`.icon` (K3), toggle group (K2), glider med tallfelt og ↺ (K4), tabelljustering (K5), `.tag` (K7), stolper på 8 px (K8), statistikkfliser (K9), `.empty`, skeleton og egen tooltip-komponent. Hjelperen `withUnit()` med U+202F (T5) og fast enhet per rad.
4. **Skall** (seksjon 3): delt toppfelt, sidefeltets toppfelt med hovedmeny, enhetskort, statusblokk, Innstillinger i menyen og Ctrl+, fjerning av `.page-head`, handlinger i toppfeltet, `scrollbar-gutter`, verktøytips i ikonsidefeltet, tastaturnavigasjon i sidefeltet.
5. **Innstillinger** (seksjon 5): ny struktur, og plumbing for de nye valgene i `server/store.js` (validering): startside, tetthet, aksentfarge, animasjonsnivå, enheter, desimaler, statusverdier, graftidsvindu og y-akse, vis simulerte sider. Om som dialog.
6. **Sider med mest død plass og rå verdier først:** Oversikt (4.1), Grafer (4.3: pene akser og fyll), Sensorer (4.2: sparklinje mot tid, én rad per tråd, vennlige navn), Kontroller (4.6: radoppsett, én simulert-melding, profiler og angre), Viftekurve (4.7), Lys (seksjon 6), Grafikkort og System (4.4, 4.5).
7. **Mikrodetaljer** (8.3): fokus, markering, cursor, rullefelt, deaktivert med grunn, knappebredder, lastetilstander og tomtilstander. Gå gjennom hver side med sjekklista og se etter **[LAT]**-punktene.
8. **Animasjoner** (animasjonsagenten, 8.1–8.2) på toppen av tokens. Test Full, Redusert og Av, og `prefers-reduced-motion`.
9. **Verifisering:** skjermbilder i 1920×1080, 1280×800 og 840×800 i begge temaer. Ingen side skal ha tomrom større enn ca. 120 px nederst på 1920×1080, og ingen side utenom Sensorer, Grafikkort og System skal rulle på 1280×800. Kontrast ≥4,5:1 for tekst under 14 px. Tab gjennom alle sider: fokus skal alltid kunne sees. Se etter tall som hopper (nettrate, klokker) og tekst som brekker. Oppdater `README.md`, `docs/STATUS.md` og `docs/PATCHLOG.md`.
