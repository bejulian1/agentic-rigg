# Lys i 3D (v0.0.13 · bygg 13)

Lys-siden viser nå skjermkortet i 3D. Du kan vri det, ta av dekselet med viftene og kjøleren til bare kretskortet er igjen, og skille lagene fra hverandre. Forhåndsvisningen fyller siden i full høyde, og kontrollene står i en smal kolonne til høyre. Glidere og fargeknapper er slankere. Prototypen som dette bygger på, ligger urørt i `prototyper/lys-3d/` (README der).

Ingenting skrives til skjermkortet eller maskinen. GPU-en brukes bare til å tegne.

## Hva som er nytt på siden

- **Forhåndsvisningen i full høyde.** Venstre del er én scene uten luft rundt. Til høyre står en kolonne på 300 px med Effekt (liste og justering), Farge (sone, fargeknapper og hex) og, i 3D, Deler. Kolonnen ruller selv, og siden ruller ikke ved 1440 × 900.
- **Infolinja er borte.** «Lyskontroller: ikke koblet til (simulert)» er nå merket «Simulert» med (?) i panelets overskrift. Det sparer en rad, og teksten er den samme i verktøytipset.
- **Sone står i Farge-panelet**, fordi sonen bestemmer hva fargen gjelder. Linja under scenen har bare lagvalget, «Skill lagene» og bakgrunnslys.
- **3D-kortet** (WebGL 2, skrevet for hånd, ingen avhengigheter). Det har:
  - fire vinkler (Forfra, Skrått, Ovenfra, Bakfra) og tilbakestilling
  - lagvalget Hel, Uten deksel, Uten kjøler og Bare kretskort, der lagene løftes av etter tur
  - «Skill lagene», en eksplodert visning
  - navnet på delen under pekeren, og en delliste som fremhever delen ved peking eller fokus
  - lyssonene, som du kan klikke på for å velge dem
  - vifter som snurrer etter viftefarten fra kilden, med treghet

  Kortet er det samme generiske RTX 2080-kortet som i prototypen. Det har ingen logoer.
- **2D og 3D.** En liten bryter (3D | 2D) i overskriften. Valget huskes i nettleseren (`ag-rgb-view`).
- **Slankere glidere i hele appen**, og fargeknapper på én rad med den store fargeflaten bak «Egendefinert …».

## 2D som reserve

Det flate SVG-kortet fra før er beholdt og brukes:

| Når | Hva skjer |
|---|---|
| Nettleseren har ikke WebGL 2 | 2D. 3D-knappen er låst, og verktøytipset sier hvorfor. |
| Programvaregrafikk (SwiftShader, llvmpipe o.l.) | 2D automatisk. Du kan likevel velge 3D. |
| 3D tegner for tregt (automatisk valg) | Oppløsningen senkes først (til 50 %). Er det fortsatt for tregt, bytter Lys til 2D og sier fra med et varsel. |
| 3D tegner for tregt (du valgte 3D selv) | Oppløsningen senkes, og deretter står effekter og vifter stille. Scenen sier det («Stillbilde: grafikken tegner for tregt»). |
| Du velger 2D | 2D. |

«For tregt» betyr at det tar mer enn 50 ms mellom bildene i over 12 bilder under løpende animasjon, etter 2,5 s oppvarming. Hvis WebGL-konteksten ikke kan lages, faller siden tilbake til 2D uten feil i konsollen.

## Én motor, to tegnere

`web/js/rgb.js` er delt i to deler:

- **`RgbEngine`** regner ut fargen på de 56 lysdiodene, pusten og bakgrunnslyset. Matematikken er den samme som før, også for Nordlys.
- **Mål** som motoren sender til, gjennom et lite grensesnitt (`LightTarget`): `setZoneColor`, `setLedColors`, `setLayerOpacity`, `setDimmedZones`, `setAmbient` og `commit`. Det er to mål: **`SvgCard`** (det flate kortet) og **`CardView`** (3D, `web/js/lys3d/view.js`).

Effekter, soner, kryssfade, bakgrunnslys, temperatur og påskeegget virker derfor likt i begge. `RgbPreview` finnes fortsatt, som motor + `SvgCard`, så prototypens kontroll mot `rgb.js` fortsatt virker.

## Når det tegnes

- **Lagd og frigjort ved behov.** 3D-koden lastes (`import()`) og lages først når Lys åpnes. Når du går til en annen side, stopper tegningen med en gang. Etter 20 s frigjøres WebGL-konteksten, lerretet, lytterne og teksturene. Vinkel, zoom og lagtrinn huskes til neste gang.
- **Appens felles løkke.** 3D-visningen kjører i appens løkke (`onFrame`), sammen med effektmotoren. Ingen egen `requestAnimationFrame`.
- **Bare når noe endrer seg.** Det tegnes bare ved input, når lag eller kamera beveger seg, når effekter beveger seg, og når vifter snurrer. Nye temperaturprøver gir ikke nytt bilde når fargene er de samme.
- **Ingen tegning i bakgrunnen.** Ingenting tegnes i skjult fane eller når scenen er utenfor skjermen.
- **«Redusert» og «Av».** Vifter og effekter står stille, lagene tones i stedet for å gli, og kameraet hopper. Løkka sover helt.
- **Høyst ca. 60 bilder/s** for løpende animasjon, også på skjermer med høyere frekvens. Input tegnes straks.

Målt i headless Chrome 153 på RTX 2080 (ANGLE/OpenGL), 1440 × 900, regnbue og vifter:

| Måling | Resultat |
|---|---|
| Bilder per sekund | 59,6 |
| JavaScript per bilde | 0,37 ms (p95 0,50 ms) |
| Helt bilde, ventet ferdig på GPU-en | 2,05 ms |
| Bilder i ro ved «Redusert» | 0 |
| Fra Lys åpnes til første 3D-bilde, ny profil | 1,04 s (teksturer 0,23 s, skyggeleggere og opplasting 0,92 s) |

Skyggeleggerne kompileres parallelt (`KHR_parallel_shader_compile`), så siden fryser ikke. Med Chromes skyggeleggerbuffer går det fortere neste gang.

## Temaer

- **Scenen er mørk i alle temaer.** Lysdioder synes ikke mot lyst, og et mørkt «fotostudio» i et lyst grensesnitt er vanlig i produktvisninger. Tonen følger likevel temaet: Grafitt `#16181b`, Sand `#1b1815` (varm), Fjord `#12181e` (kjølig), Midnatt `#0c0d12` lyst og `#050506` mørkt, og høy kontrast `#000`.
- **Egne tokens for scenen** (seksjon 2 i `styles.css`): `--stage-text`, `--stage-dim`, `--stage-chip` (bak små merker) og `--stage-focus` (fokusringen, temaets lyse aksent `--a-d`). Når tema, kontrast eller aksent endres, oppdateres bakgrunnen og fremhevingen i 3D med en gang.
- **`npm run contrast` sjekker de nye parene** i alle 96 kombinasjonene:
  - tekst på scenen
  - tekst på merkene over scenen, også når merket ligger over en lys del av bildet (`#808080`)
  - fokusringen mot scenen

## Slankere kontroller

- **Glidere i hele appen:** 3 px spor og 14 px knott (før 4 og 18). Selve elementet er 24 px høyt (før 20), så treffflaten er større enn før, og hele sporet kan klikkes. Fokus vises som en ring på 3 px rundt knotten. Fyllet slutter nå midt under knotten (`fillRange` tar hensyn til knottens bredde). Standardmerket på Kontroller er flyttet etter.
  - **Hvorfor i hele appen:** Én glider overalt ser roligere og mer gjennomtenkt ut enn to størrelser. Den store knotten var det som gjorde Kontroller og Lys tunge. Treffflaten blir ikke mindre, og kontrasten (fyll mot spor 3:1) er uendret.
- **Fargeknapper:** 20 px farge i en treffflate på 24 px, ni på én rad, valgt med ring. Raden er ett tabulatorstopp med piltaster. Under står fargen nå, hex-feltet og «Egendefinert …», som åpner metning/lyshet og fargetone i en popover (Esc lukker). Velgeren tar ca. 110 px i høyden i stedet for ca. 300.
- **Segmentkontrollene** på Lys er `sm`. Lagvalget får korte navn under 560 px (Deksel av, Kjøler av, Kretskort), og hele navnet er knappens etikett.

## Tilgjengelighet

- **Visningen** har `role="application"`, merkelappen «Skjermkortet i 3D» og en beskrivelse som følger med (vinkel, lag, lys og vifter). Tastene står synlig på scenen.
- **Taster:** piltaster vrir (15° og 10°, med Shift 45° og 30°), + og − zoomer, og 0 eller Home tilbakestiller. Fokusringen er et eget lag over lerretet, i `--stage-focus`.
- **Lagvalget** er en radiogruppe med piltaster, og skjermlesere får lagtrinnet opplest. Dellisten er ett tabulatorstopp med piltaster, Home og End.

## Tester

- **Ny test i `test/ui/smoke.test.js`:** «Lys i 3D». Den sjekker:
  - automatisk 2D ved programvaregrafikk
  - valg av 3D: lerretet fyller scenen, og valget huskes
  - piltaster, + og 0 (som ikke bytter side), og at fokusringen vises
  - alle lagtrinn med færre deler for hvert trinn, og beskrivelsen
  - ingen nye bilder i ro med «Redusert» og «Av», høyst ca. 60 bilder/s med regnbuen, og ingen bilder på en annen side
  - nordlys på 3D-kortet, som beveger seg og ikke kan brukes
  - ingen vannrett rulling ved 1440 og 360 px
  - frigjøring og ny oppretting
  - 2D som reserve når WebGL er slått av: 3D låst, 2D merket, lagvalget skjult og effektene virker
  - ingen konsollfeil
- **Vernet mot treg grafikk** slås av i testen. Headless Chrome tegner med programvare, og effektene ville ellers blitt stillbilder. Vernet er kontrollert i prototypens `verktoy/kontroll.mjs`.
- **De andre UI-testene er uendret og består.** De ser Lys i 2D, fordi headless Chrome bruker programvaregrafikk. Det gjelder også F29-testen av det flate kortet og påskeeggtestene, som leser fargene fra SVG-en.

## Det jeg lot være

- **3D i de andre testene.** UI-testene kjører med programvaregrafikk. 3D-kortet er testet i én test (over), og ytelsen er målt på RTX 2080 med et eget skript.
- **Lagring av vinkel, lagtrinn og «Skill lagene» mellom økter.** De huskes bare mens appen er åpen.
- **Animert bytte mellom 2D og 3D.** Det bytter med en gang.
- **Kontrastsjekk for lysdiodenes farger.** De er brukerens valg og ikke tekst eller kontroller.
- **Firefox og WebKit.** Testet bare i Chrome (headless, med programvaregrafikk og med RTX 2080).
- **Det som står under «Det jeg lot være» i `prototyper/lys-3d/README.md`**, for eksempel NVLink, bloom og ambient occlusion, gjelder fortsatt.
