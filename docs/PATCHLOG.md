# Patchlog

## alpha v0.0.15 · bygg 15 – 6. november 2026: Innstillinger i hele vinduet

Innstillinger dekker nå hele vinduet, med egen navigasjon der sidefeltet står, søk i alle delene og panelene i to eller tre spalter. Valgene og begrunnelsen står i `docs/innstillinger-v0.0.15.md`. Sikkerhetskopi av `web/`, `test/`, `version.json`, `package.json`, `README.md` og dokumentene som er endret, ligger i `.backup-v0.0.14/`.

**Hele vinduet**
- **Egen visning, ikke et lag.** `#/innstillinger` setter `data-view="settings"` på `<html>`. Sidefeltet og toppfeltet skjules, og Innstillinger fyller vinduet. Ruten, omlasting og nettleserens tilbake og fram virker som for alle sider, og Tab kan ikke komme til det som er skjult. Begrunnelsen står i designnotatet.
- **Ingen blink ved omlasting.** `theme-boot.js` setter `data-view` før første tegning.
- **Linjene øverst står fortsatt over alt:** Behold med nedtelling, Frakoblet og Bakgrunnstjeneste ikke tilkoblet.
- **Fire veier tilbake:** knappen ← øverst til venstre («Tilbake til Sensorer · Esc»), Esc, Ctrl+, igjen og nettleserens tilbake. Du kommer til siden du kom fra, med samme rulling. Fokuset går tilbake dit det var, ellers til tannhjulet.
- **Historikken.** Oppføringen merkes med siden du kom fra (`history.state.agFrom`), så tilbake bruker `history.back()`, også etter omlasting. Åpnet direkte byttes oppføringen ut med sist brukte side, uten nye oppføringer.
- **Esc** lukker først en popover eller dialog og tømmer søket, og går så tilbake. Ctrl+B gjør ingenting mens sidefeltet er skjult.
- **Overgangen:** opasitet og 6 px løft over 250 ms inn og ut. Ved «Redusert» er det en kort toning, og ved «Av» ingenting. Rammen står stille, fordi navigasjonen er like bred som sidefeltet og har den samme toppstripen.

**Navigasjonen**
- Tilbake og tittelen øverst, så søkefeltet (med / som hint), så de sju delene med ikon, og nederst «Endringer lagres straks».
- Loddrett faneliste: piltaster, Home og End. Fokus går til den valgte delen når Innstillinger åpnes.
- Mellom 700 og 860 px er kolonnen 220 px. Under 700 px står tilbake, tittel og søk på én linje øverst og delene som sju ikoner under, med navnet i verktøytipset og for skjermlesere.

**Innholdet**
- **Panelene** er de samme som ellers i appen: tynn ramme, lav overskriftsstripe og rader rett inni. Etiketten og en kort forklaring står til venstre, kontrollen til høyre, og radene er 40 px. Lengre forklaringer står som merknad nederst i panelet.
- **Stripen øverst** viser navnet på delen og en linje om hva som står der.
- **Spalter etter bredden:**
  - én under ca. 1140 px
  - to fra ca. 1140 px (1440 × 900)
  - tre fra ca. 1760 px (1920 × 1080) når delen har fire paneler eller flere
- **Ingen innstillinger er tatt bort.** Disse er flyttet eller har fått nytt navn:
  - «Start med simulert GPU» (før «Bruk simulert ved oppstart») står under Datakilde i «Skjermkortdata».
  - Varselfarger er flyttet fra Sidefelt til Grafer og sensorer, med én rad per verdi og Gul og Rød i hver sin kolonne.
  - Sparklinjer i statusblokken sto i et panel uten tittel og står nå i Sidefelt › Visning.
  - Ny rad Sidefelt › Visning › **Bredde** (Full eller Ikonlist). Det er det samme valget som Ctrl+B, og det gjelder straks, også mot en eldre server.
  - «Status nederst» heter «Statusblokken».
  - Om er et panel ved siden av Feilsøking i stedet for en sentrert plakat.
- **Kortere forklaringer**, og samme ordbruk overalt.
- **Tilgjengelighet.** Forklaringen er koblet til kontrollen (`aria-describedby`). Feltene for varselfarger har navn («GPU-temperatur, gul grense»); før het de `gpuTempWarn`.

**Søk**
- **/ eller Ctrl+F** inne i Innstillinger flytter fokus til søket.
- **Treffene er vanlige paneler** med kontrollene i behold. De kan endres rett fra treffet.
- **Hvor treffet står:** «Utseende ›» foran tittelen. Det er en knapp til panelet i delen.
- **Navigasjonen** viser antall treff per del og demper delene uten treff.
- **Ordene er markert** i titlene. Alle ordene i søket må passe.
- **Det søkes ikke i valgene i en nedtrekksliste.**
- **Ingen treff:** en tydelig tomtilstand.
- **Taster:** Enter går til første kontroll. Esc tømmer søket, og Esc igjen går tilbake.
- **Målt:** 3–18 ms per søk med layout (én bokstav gir 40 treff og er det tregeste).

**Finpuss med friske øyne:** ni funn er rettet. Lista står i designnotatet. Blant dem:
- «graf» fant Startside.
- Markeringen delte ord i avkrysningslistene.
- Fokusringen ble stående på feil del.
- «0–100» ble delt over to linjer.
- Søkefeltet var 104 px ved 360 px.
- Den gamle sentrerte Om-regelen styrte fortsatt.

**Kontrast**
- `npm run contrast` sjekker også teksten på markerte søketreff (`--text` på `--accent-soft-strong` over `--card`) og dempede deler i navigasjonen.
- Alle 96 kombinasjoner består. Dårligste par er uendret: 4,51:1 for tekst og 3,01:1 for UI-grenser.

**Tester** (165 tester, ca. 90 s)
- **Ny fil** `ui/settings.test.js` med tre tester:
  - **Hele vinduet:** sidefeltet og toppfeltet skjules, og navigasjonen står ved venstre kant. Tilbake virker med Esc, knappen, Ctrl+, to ganger og nettleserens tilbake og fram. Rullingen og fokuset på tannhjulet (med synlig ring) kommer tilbake. Esc i en popover lukker bare popoveren. Ctrl+B gjør ingenting. Bredde-valget virker. Ved 360 px står navigasjonen øverst, og ingenting ruller vannrett i noen del.
  - **Omlasting** på `#/innstillinger` uten historikk (tilbake til sist brukte side, uten ny oppføring) og etter at Innstillinger er åpnet fra en side.
  - **Søk:** / og Ctrl+F, treff, delen, markering, antall per del, en bryter som virker i treffet, flere ord, nedtrekkslister, tomtilstand, Enter, Esc og knappen til delen.
- **Én eksisterende test er utvidet**, ikke endret. Tastaturtesten i `ui/smoke.test.js` åpner Innstillinger med Ctrl+, og bruker så kortvelgeren i sidefeltet. Sidefeltet er nå skjult i Innstillinger, så testen trykker Ctrl+, én gang til og sjekker at den kommer tilbake til Oversikt.
- **Ingen påstander er endret.**

**Serveren er ikke endret.** Innstillinger bruker ingen nye nøkler, så alt virker også mot brukerens server på 4318 (v0.0.8). Der vises versjonen fra serveren til den startes på nytt.

**Skjermbilder** ligger i `docs/skjermbilder/v0.0.15/` (`for/` og `etter/`):
- alle sju delene i mørkt og lyst ved 1440 × 900
- 1920 × 1080, 1100 × 800, 360 px og 200 % zoom (720 × 450)
- etter: alle fire fargetemaer og seks aksentfarger, høy kontrast lyst og mørkt, søk med og uten treff, fokusring, verktøytipset på tilbake, Behold-linja over Innstillinger, kompakt tetthet og skrift 116 %

Alt er tatt mot egne testservere. Port 4318 er ikke rørt.

## alpha v0.0.14 · bygg 14 – 6. november 2026: sidefeltet kan alltid utvides, og tallene i ikonlista

En liten rettelse av det sammenlagte sidefeltet (ikonlista). Sikkerhetskopi av `web/`, `test/`, `version.json`, `package.json` og dokumentene som er endret, ligger i `.backup-v0.0.13/`.

**Feil: sidefeltet ble stående som ikonlist**
- **Slik så det ut.** Knappen nederst sa «Utvid sidefeltet», men et klikk (eller Ctrl+B) la det sammen igjen. Ingen feil i konsollen.
- **Årsaken** var to feil i `web/js/app.js` som slo til sammen mot en server som ikke kjenner `sideCollapsed`. Brukerens server på 4318 kjører fortsatt v0.0.8 (startet 5. november, før v0.0.10).
  1. Den gamle serveren ignorerer nøkler den ikke kjenner, og svarer med innstillinger uten `sideCollapsed`. `applySettings()` regnet bare ut endringer for nøklene i det *nye* svaret. En nøkkel som forsvant, ble ikke sett som en endring, så `applySide()` ble aldri kalt. Ikonlista ble stående, mens `sideCollapsed` i appen var blitt `undefined`.
  2. `toggleSide()` regnet neste tilstand som `!sideCollapsed`. Det er `!undefined`, altså `true`. Hvert klikk la derfor sammen igjen, og svaret fra serveren tok bort nøkkelen på nytt. Sidefeltet var fanget.
- **Rettet:**
  - **Valget gjelder lokalt først.** Sidefeltets bredde er nå et valg for nettleseren først (`LOCAL_FIRST` i `app.js`). Det tas i bruk straks og huskes i utdraget i `localStorage`, som `theme-boot.js` allerede leste. Det rulles aldri tilbake.
  - **Serveren får valget etterpå.** Valget sendes til serveren som før. Det regnes som lagret bare når serveren svarer med samme verdi.
  - **Uten svar fra serveren** brukes det lokale valget. Det gjelder når serveren ikke kjenner nøkkelen, avviser den (400) eller ikke svarer.
  - **Ingen feilmelding** vises for et valg som uansett virker.
  - **Endringer sammenlignes nå begge veier.** `applySettings()` ser på nøklene i både det gamle og det nye, så en nøkkel som forsvinner, regnes også som en endring.
  - **Knappen og Ctrl+B bytter det du ser.** Neste tilstand regnes fra bredden på skjermen (`data-side`). Etterpå kalles `applySide()` direkte.
  - **Tilbakestill og Importer** under Innstillinger glemmer lokale valg som ikke er bekreftet (`applySettings(…, { replace: true })`).
- **Start serveren på 4318 på nytt** for at valget også skal lagres der, og for at versjonen skal vise 0.0.14. Appen virker uten omstart: last fanen på nytt.

**Tallene i ikonlista**
- **Mer luft og en tydelig rytme.** Tallet og enheten står som én tett gruppe med 2 px mellom. Neste verdi står 8 px unna. Før var avstanden lik begge steder, og «W» og «%» så ut til å klistre seg til tallet under.
- **Kilden står over hver gruppe.** Et lite, sperret og dempet merke (GPU, CPU, RAM, NETT, ANDRE) står over første verdi hver gang kilden skifter. «64 °C» under «CPU» er dermed prosessoren.
  - Merket er 9,5 px, vekt 500, i `--dim`, som består kontrastsjekken på sidefeltet.
  - Merket er skjult i full bredde og for skjermlesere. Der står navnet allerede («GPU-temp 38 °C»), og verktøytipset har det fulle navnet («GPU-temperatur · vis på Sensorer»).
  - Ikonlista er like smal som før (56 px).
- **Ikonlista ruller aldri.** Foten med Innstillinger skal alltid synes.
  - Får ikke alle verdiene plass, vises så mange som får plass, i brukerens rekkefølge. Et merke som ikke har noen verdier under seg, tas bort.
  - Dette gjelder også et problem som fantes fra før: mellom 600 og ca. 700 px høyde rullet ikonlista allerede med fem verdier.
  - Under 600 px er blokken skjult, som før.
- **Sett i** lyst og mørkt, alle fire fargetemaer, alle seks aksentfarger og høy kontrast (begge). Også med 1–3 siffer, 100 %, gult og rødt nivå, nettrate (176,8 kB/s), en festet sensor (ANDRE), åtte verdier, skriftstørrelse 116 %, og høydene 900, 800, 700, 640 og 600 px.

**Tester** (162 tester, ca. 90 s)
- **Ny test** i `ui/smoke.test.js`. Den legger sammen, utvider og laster på nytt, med knappen og med Ctrl+B. Hvert bytte sjekkes straks og etter at serveren har svart, sammen med `aria-expanded` og knappens navn.
  - Testen kjøres mot fire servere: en ny server, og en server som ignorerer, avviser eller ikke svarer på `sideCollapsed`. De tre siste etterlignes i nettleseren.
  - Den sjekker også kildemerkene, avstanden mellom enhet og neste tall, at ikonlista ikke ruller ved 900, 700 og 640 px, og at full bredde er urørt.
  - Testen feiler på koden fra v0.0.13 («ignore: lagt sammen med knappen: valget i appen»).
- **Ingen eksisterende tester er endret.**

**Skjermbilder** ligger i `docs/skjermbilder/v0.0.14/` (`for/` og `etter/`):
- ikonlista i mørkt og lyst ved 1440 × 900
- nærbilde av tallene i skala 3
- etter: alle temaer side om side (skala 2), verktøytipset med hover og 360 px

Alt er tatt mot egne testservere. Port 4318 er ikke rørt; den ble bare lest med GET for å finne versjonen.

## alpha v0.0.13 · bygg 13 – 5. november 2026: Lys i 3D og slankere kontroller

Lys viser skjermkortet i 3D. Du kan vri det, ta av lag til bare kretskortet er igjen, og skille lagene. Forhåndsvisningen fyller siden i full høyde, og kontrollene står i en smal kolonne. Glidere og fargeknapper er slankere. Valg, målinger og funn står i `docs/lys-3d-v0.0.13.md`, og prototypen i `prototyper/lys-3d/`. Ingenting skrives til skjermkortet eller maskinen; GPU-en brukes bare til å tegne. Sikkerhetskopi av `web/` og `test/` fra v0.0.12 ligger i `.backup-v0.0.12/`.

**Lys**
- **Nytt oppsett.** Forhåndsvisningen fyller venstre del i full høyde. Til høyre står en kolonne på 300 px med Effekt, Farge og (i 3D) Deler.
  - Infolinja er erstattet av merket «Simulert» med (?) i overskriften.
  - Sone er flyttet til Farge-panelet.
- **3D-kortet** (WebGL 2 uten avhengigheter, `web/js/lys3d/`). Det kan:
  - vris med mus, berøring og tastatur, og vises fra fire vinkler med tilbakestilling
  - vise lagene Hel, Uten deksel, Uten kjøler og Bare kretskort, og skille lagene
  - vise navnet på delen under pekeren, og fremheve delen fra en delliste
  - la deg klikke på lyssonene for å velge dem
  - snurre viftene etter viftefarten fra kilden
- **3D | 2D** i overskriften. Det flate kortet er reserven:
  - uten WebGL 2
  - ved programvaregrafikk
  - når 3D tegner for tregt
  - når du velger 2D

  Valget huskes i nettleseren (`ag-rgb-view`).
- **Lagd ved behov.** 3D lastes og lages først når Lys åpnes, stopper når du går til en annen side, og frigjøres etter 20 s. Det tegnes bare når noe endrer seg, aldri i skjult fane, og ikke i ro med «Redusert» eller «Av».
- **Alt annet virker som før:** effektene, sonene, Bruk og Forkast, Ctrl+Enter og Ctrl+Z, bakgrunnslys, lagring og Nordlys.

**Kode**
- **`rgb.js` er delt i to.**
  - `RgbEngine` regner fargene, med samme matematikk som før.
  - Målene får fargene gjennom `LightTarget`: `SvgCard` er det flate kortet, og `CardView` er 3D.
  - `RgbPreview` finnes fortsatt som motor + SVG.
  - Bakgrunnslyset følger nå pusten (før ble opasiteten bare skrevet når fargene endret seg).
- **`lights.js`** velger 2D eller 3D, bygger og river tegnerne, og kjører motoren og 3D i appens felles løkke.
- **`colorpicker.js`:** fargeknappene står på én rad, og metning/lyshet og fargetone ligger i en popover bak «Egendefinert …».
- **`ui.js`:** `fillRange` lar fyllet slutte midt under knotten.

**Utseende**
- **Glidere i hele appen:** 3 px spor og 14 px knott (før 4 og 18). Elementet er 24 px høyt (før 20), så treffflaten er større. Begrunnelsen står i dokumentet.
- **Fargeknapper:** 20 px farge i en treffflate på 24 px. Raden er ett tabulatorstopp med piltaster.
- **Scenen har temaets tone:**
  - Grafitt `#16181b`
  - Sand `#1b1815`
  - Fjord `#12181e`
  - Midnatt `#0c0d12` lyst og `#050506` mørkt
  - høy kontrast `#000`

  Scenen har egne tokens for tekst, merker og fokusring: `--stage-text`, `--stage-dim`, `--stage-chip` og `--stage-focus`.

**Kontrast**
- `npm run contrast` sjekker også tekst på scenen og på merkene over den, også over et lyst bilde, og fokusringen mot scenen. Alle 96 kombinasjoner består. Dårligste par er uendret: 4,51:1 for tekst og 3,01:1 for UI-grenser.

**Tester** (161 tester, ca. 65 s)
- **Ny test** i `ui/smoke.test.js`: «Lys i 3D». Den sjekker:
  - automatisk 2D ved programvaregrafikk
  - valg av 3D: lerretet fyller scenen, og valget huskes
  - piltaster, + og 0, og fokusringen
  - alle lagtrinn med færre deler for hvert trinn, og beskrivelsen
  - ingen bilder i ro med «Redusert» og «Av», høyst ca. 60 bilder/s, og ingen bilder på en annen side
  - nordlys på 3D-kortet
  - ingen vannrett rulling ved 1440 og 360 px
  - frigjøring og ny oppretting
  - 2D når WebGL er slått av
  - ingen konsollfeil

  Vernet mot treg grafikk slås av i testen, fordi headless Chrome tegner med programvare.
- **Ingen eksisterende tester er endret.** De ser Lys i 2D (programvaregrafikk i headless Chrome), så F29-testen og påskeeggtestene, som leser fargene fra SVG-en, prøver fortsatt det samme.

**Serveren må startes på nytt** for at versjonen skal vise 0.0.13. Lys virker i en fane som lastes på nytt, også uten omstart. Serveren er ikke endret.

**Skjermbilder** ligger i `docs/skjermbilder/v0.0.13/` (før og etter):
- Lys i lyst og mørkt, og alle lagtrinn
- Sand, Fjord og Midnatt med andre aksenter
- høy kontrast, også med fokus
- 360 px
- «Egendefinert …», 2D valgt, og reserven ved programvaregrafikk og uten WebGL
- Kontroller og Viftekurve med de slanke gliderne

3D-bildene er tatt i headless Chrome som tegner på maskinens skjermkort, mot egne servere på tilfeldige porter. Port 4318 er ikke rørt.

## alpha v0.0.12 · bygg 12 – 5. november 2026: små overraskelser

Appen har fått noen små overraskelser. Hva de er og hvor de gjemmer seg, står ikke her. Fasiten ligger i `docs/paaskeegg.md` for den som vil vite det. Ingenting skrives til skjermkortet eller maskinen. Sikkerhetskopi av `web/` og `server/` fra v0.0.11 ligger i `.backup-v0.0.11/`.

**Det som er lovet**
- De endrer aldri et målt tall og later aldri som noe er ekte.
- De sender ingenting til skjermkortet, bakgrunnstjenesten eller serveren.
- De rører ikke kontrollene, bekreftelsesflyten eller innstillingene dine.
- Ingen lyder, ingen dialoger og ingen blinking.
- De følger animasjonsnivået (ingenting beveger seg med «Av»), høy kontrast, tastatur og skjermlesere.
- De bruker aldri gul, oransje eller rød.

**Innstillinger**
- Ny bryter **Små overraskelser** under Innstillinger › Generelt (gruppen Annet). Den er på som standard, og av slår av alt med en gang.
- Lagres som `surprises` (sann/usann) i `server/store.js`, validert som de andre.
- Hvor mange du har funnet, huskes bare i nettleseren.

**Kode**
- Alt ligger i én fil, `web/js/easter.js`, med en liste øverst. Krokene i sidene er korte og merket «Påskeegg».
- `ui.js`: et varsel kan ha et eget ikon (`toast(…, { icon })`).
- `rgb.js` importerer fortsatt bare `util.js`.

**Rettet**
- Grafikkort › Lansert sa «September 2018» med stor forbokstav. Nå står det «september 2018» (`server/specs.js`).
- README hadde «{N}» der det skulle stå et smalt mellomrom (U+202F) i «100 %», «85 °C» og «2 s» (fire steder).

**Tester** (160 tester, ca. 45 s)
- `store.test.js` (1 ny): `surprises` er på som standard, beholdes ved andre endringer, eldre filer får standard, og alt annet enn sann/usann avvises.
- Ny `ui/easter.test.js` (4 tester) i headless Chrome. Hva de sjekker, står i fasiten:
  - alt kan utløses ved 1440 og 360 px, uten konsollfeil og vannrett rulling, og uten å endre innstillinger, utkast, brukt, lys, profiler eller ventende endring
  - bryteren slår av alt
  - ingenting animeres med «Av»
  - vanlig bruk utløser ingenting ved et uhell
- Ingen eksisterende tester er endret.

**Serveren må startes på nytt** for at bryteren skal huskes, for at versjonen skal vise 0.0.12 og for «september 2018». Uten omstart virker overraskelsene, men bryteren hopper tilbake til på. Den gamle serveren kjenner ikke `surprises` og sender innstillingene tilbake uten den.

Skjermbilder ligger i `docs/skjermbilder/v0.0.12/`. De er tatt i headless Chrome mot egne servere på tilfeldige porter. Port 4318 og brukerens socket er ikke rørt. Filnavnene røper hva bildene viser.

## alpha v0.0.11 · bygg 11 – 5. november 2026: grafer opptil 48 t, tydelige målere og faste farger

Grafer med valgfritt tidsvindu fra 1 min til 48 t og historikk på serveren, pause per graf, markør på alle grafene eller bare én, faste seriefarger, seks aksentfarger med navn, og nye målere for temperatur og effekt. Alle steder med oransje og rødt er gått gjennom. Valg og funn står i `docs/grafer-og-malere-v0.0.11.md`. Ingenting skrives til skjermkortet eller maskinen; historikken lagres i datamappa. Sikkerhetskopi av `web/` og `server/` fra v0.0.10 ligger i `.backup-v0.0.10/`.

**Historikk (server)**
- Ny `server/history.js`:
  - 1 s-bøtter i 15 min, 10 s i 6 t og 60 s i 48 t, med min, snitt og maks per bøtte, så topper ikke forsvinner.
  - Høyst 96 serier og høyst `keep/step` rader per lag (ca. 6,8 MB minne i verste fall).
  - `history.json` i `AG_DATA_DIR` skrives hvert 5. minutt og ved avslutning, atomisk med temp-fil og rename. Den er høyst 16 MiB; verste tilfelle målt til 14,6 MiB.
  - Ved oppstart leses fila inn igjen. Gamle rader ryddes bort. En ødelagt fil flyttes til `history.json.bad`.
- Seriene får prøvene fra den samme lesingen som før (`nvidia-smi`, `/proc`, `/sys`). Simulerte GPU-tall lagres for seg (`gsim.*`). Det samles inn mens kilden kjører, det vil si mens en fane er åpen, som før.
- `GET /api/history?window=&keys=&points=`:
  - window er en av sju verdier, keys er 1–16 navn uten doble, og points er 10–4000.
  - Ukjente parametre og alt annet ugyldig gir 400.
  - Ruten leser bare.
- `AG_TEST_HISTORY=1` fyller historikken med syntetiske 48 t (for tester og skjermbilder).

**Grafer (nettleser)**
- `web/js/history.js` henter vinduet og fletter inn de levende prøvene. `web/js/chartpanel.js` er ett felles graf-panel for Oversikt, Grafer, Kontroller og grafen under en rad på Sensorer.
- `charts.js` er skrevet om:
  - tidsakse med sekunder, klokkeslett og dato der døgnet skifter
  - min/maks-bånd i lange vinduer
  - aldri flere punkter enn piksler
  - hull der data mangler, og «Data fra i dag 16:43»
  - tidspunkt og verdi (med min–maks) når du holder over
- **Tidsvindu:** 1 min, 5 min, 15 min, 1 t, 6 t, 24 t og 48 t.
  - Grafer har det i toppfeltet (`chartWindow`; 2 min fra eldre filer blir 5 min).
  - Oversikt og Kontroller har en nedtrekksliste i toppfeltet, og grafen på Sensorer har den i overskriften (`windows`).
- **Pause per graf:** knapp, eller mellomrom når grafen har fokus. Mens den står stille, står det «På pause · 14:32:10», og knappen heter Fortsett. Dataene samles videre, og Fortsett hopper til nå. Pause for alle grafene på Grafer som før.
- **Markør:** på alle grafene eller bare den du holder over (`chartSync`). Valget er en knapp i toppfeltet på Grafer og en bryter under Innstillinger › Grafer og sensorer.
- **Forklaringen:** klikk på en serie skjuler eller viser den.

**Farger**
- **Seriepalett:** sju faste farger (`--pal-*`, med betydningene i `--c-*`) i lyst, mørkt og høy kontrast, uten gul, oransje og rød, og uavhengige av aksenten. De brukes i alle grafer, sparklinjer og forklaringer. `npm run contrast` sjekker hele paletten.
- **Seks aksentfarger:** blå, blågrønn, grønn, lilla, rosa og skifer, som fargeprøver med navn. Gul, oransje og rød er fjernet fordi de betyr advarsel og fare; lagrede valg med dem blir blå.
- **Gjennomgang av gult og rødt:**
  - Temperaturskalaens soner er borte.
  - «Temperatur nå» på viftekurven har temperaturfargen.
  - Endringsprikkene har aksentfarge.
  - Simulert GPU er nøytral med grå prikk.
  - Verdier med varselfarge sier i verktøytipset hva fargen betyr.

**Målerne på Oversikt**
- Temperatur og effekt har samme måler (`web/js/gauge.js`): fyll til verdien nå (ingen prikk), tynne streker for grensene med tallet under, og en forklaring som navngir alle tallene.
- Tallene krasjer aldri: skalaens sluttall viker først.
- Gul og rød bare over varselgrensene, alltid med ord ved siden av.

**Tester** (155 tester, ca. 45 s)
- Ny `history.test.js` (9 tester):
  - bøtter og topper som overlever nedsampling
  - aldri flere punkter enn bedt om, og riktig lag per vindu
  - hull
  - grenser for serier og rader
  - lagring og innlesing (samme svar etter omstart, gamle rader utløper)
  - verste tilfelle under filgrensen
  - ødelagte filer (fire varianter)
  - nøklene fra prøvene
  - at lesing ikke endrer noe
- `http.test.js` (4 nye):
  - gyldig `/api/history`
  - 12 ugyldige varianter som gir 400
  - ukjent serie og POST
  - historikken overlever en omstart av serveren
- `ui/smoke.test.js`, ny røyktest: tidsvindu 48 t, pause med knapp og mellomrom, forklaring som skjuler serier, faste seriefarger når aksenten endres, markøren av og på, tidsvindu på Oversikt, målerne uten krasj ved 1440 og 360 px, og seks aksentfarger med navn.
- **Endret, med vilje:**
  - `ui/smoke.test.js` (Sensorer): tittelen i grafen under raden er nå panelets overskrift (`.group-title`), ikke et `<b>`. Det er samme sjekk av samme tekst.
  - `ui/smoke.test.js` (tastatur og fokus): Tab-gangen på Kontroller går nå 44 stopp i stedet for 30. Grafene har fått forklaring, pauseknapp og fokuserbar graf, så profilmenyen kommer senere. Rekkefølgen og fokusringen sjekkes som før, for alle stoppene.
- `scripts/contrast.js`: seks aksenter (96 kombinasjoner) og seriepaletten mot kortet.

**Serveren må startes på nytt** for at historikken, `/api/history` og de nye innstillingene (`windows`, `chartSync`, nye tidsvinduer) skal virke. Uten omstart henter grafene ingen historikk og viser bare de levende prøvene fra fanen ble åpnet. Den gamle serveren avviser også tidsvinduene 1 t–48 t og glemmer tidsvindu per side og markørvalget.

Skjermbilder før og etter ligger i `docs/skjermbilder/v0.0.11/`. Etter-bildene er tatt med syntetiske 48 t (`AG_TEST_HISTORY=1`) mot egne servere på tilfeldige porter. Port 4318 og brukerens socket er ikke rørt.

## alpha v0.0.10 · bygg 10 – 5. november 2026: sammenleggbart sidefelt uten hovedmeny

Brukeren valgte skisse B: et rent sidefelt uten hamburgermeny, som kan legges sammen til en smal ikonlist. Innholdet er det samme som før. Valg og funn står i `docs/sidefelt-v0.0.10.md`. Ingenting her skriver til skjermkortet eller maskinen. Sikkerhetskopi av `web/` fra v0.0.9 ligger i `.backup-v0.0.9-web/`.

**Sidefeltet**
- Hovedmenyen (≡, `#menu-btn` og `#main-menu`) er fjernet. Toppen er appikon, navn og alpha-merket.
- Ny fot under statusblokken, skilt med en tynn linje: Innstillinger (lenke med tannhjul, Ctrl+,), Tastatursnarveier (?-knapp, Ctrl+?) og knappen som legger sammen sidefeltet (Ctrl+B).
- Kortvelgeren er én rad på 28 px: ikon, navn og statusprikk. Live/Simulert og driveren står i verktøytipset, for skjermlesere og i popoveren, som virker som før.
- Ikonlist (56 px) med `data-side="rail"` på `<html>`: ikoner med verktøytips til høyre, kortvelgeren som ikon med prikk, statusverdiene som tall med enheten under (etiketten i verktøytipset og for skjermlesere) og foten loddrett. Samme regler brukes under 860 px, der sidefeltet alltid er en ikonlist. Der er knappen skjult, og Ctrl+B gjør ingenting. Statusblokken vises nå også i smale vinduer, men viker i vinduer under 600 px høyde (også 200 % zoom), så foten alltid synes.
- Bredden byttes straks; innholdet i sidefeltet toner inn og følger `data-motion` (ingen animasjon med «Av»).

**Om og snarveier**
- Om er en egen del av Innstillinger (siste i lista): versjon, beskrivelse, knapp til Tastatursnarveier og feilsøkingsradene. Om-dialogen og Om-knappen under Data er fjernet.
- Tastatursnarveier har Ctrl+B og en lenke nederst til Innstillinger › Om.

**Lagring**
- Ny innstilling `sideCollapsed` (sann/usann, standard usann) i `server/store.js` (`defaultSettings` og `mergeSettings`). Ellers er serverkoden urørt. Et utdrag lagres i `localStorage`, så `theme-boot.js` setter riktig bredde før første tegning.

**Tester** (141 tester, ca. 42 s)
- `ui/smoke.test.js`, «tastatur og fokus»: testen åpnet hovedmenyen med Enter og sjekket at fokus gikk tilbake til menyknappen etter Esc. Knappen finnes ikke lenger, så den delen er byttet ut med den nye veien: ?-knappen åpner Tastatursnarveier, og fokus går tilbake til den etter Esc. Ctrl+? åpner dialogen, og Ctrl+B står i den. Lenken i dialogen åpner Innstillinger › Om med versjonen. Innstillinger åpnes fra sidefeltet med Enter og med Ctrl+, fra en annen side. Tab-rekkefølgen sjekker nå Innstillinger, Tastatursnarveier og Legg sammen sidefeltet der Hovedmeny sto, med synlig fokusring på alle.
- `ui/smoke.test.js`, «Av»: klikket på menyknappen for å åpne en popover. Den finnes ikke lenger, så testen åpner kortvelgeren i stedet, og legger i tillegg sammen og utvider sidefeltet. Fortsatt 0 animasjoner.
- Ny røyktest for sidefeltet: knappen gir ikonlist (bredde, skjulte navn, statusverdien innenfor), valget lagres på serveren, alle ni sider uten vannrett rulling som ikonlist ved 1440 og 1100 px, valget huskes etter omlasting, Ctrl+B utvider igjen, og ved 360 px er knappen skjult, Ctrl+B gjør ingenting og det er ingen vannrett rulling.
- `store.test.js`: `sideCollapsed` er av som standard, beholdes ved andre endringer og avviser alt annet enn sann/usann.

Skjermbilder før og etter (lyst og mørkt, utvidet og sammenlagt ved 1440 × 900, kortvelgeren, snarveiene, Innstillinger › Om, 360 px og 200 % zoom) ligger i `docs/skjermbilder/v0.0.10/`. Tatt i headless Chrome mot egne servere på tilfeldige porter. Port 4318 og brukerens socket er ikke rørt.

## alpha v0.0.9 · bygg 9 – 5. november 2026: tettere og mer profesjonelt

Designrunde i retning av sammendrags- og sensorvinduet i HWiNFO: mer informasjon per skjerm, paneler i stedet for svevende kort, tabeller og etikett–verdi-rutenett, og grafene beholdt. Ingen navn, logoer, ikoner eller farger er hentet derfra. Ingen gradienter eller glød. Prinsipper og side-for-side-plan står i `docs/design-pro-v0.0.9.md`, med funnene fra etter-bildene og finpussrunden. Ingen serverkode er endret, og ingenting her skriver til skjermkortet eller maskinen. Sikkerhetskopi av `web/` fra v0.0.8 ligger i `.backup-v0.0.8-web/`.

**Tokens (`web/styles.css`, seksjon 1 og 2)**
- Standard tetthet: rader 26 px (var 34), knapper og felt 28 px (var 32), små knapper 22 px, 8 px mellom paneler (var 12) og 12 px luft inni (var 16). Toppfeltet 41 px (var 47). Kompakt og luftig er skalert tilsvarende (22/32 px rader).
- Ny `--h-panel` (30 px) for panelstripen og ny farge `--panel-head` per fargetema og lys/mørk. Den er lagt til flatene `npm run contrast` sjekker.
- Radius 3/4/6 px (var 4/6/10). Merker er rektangler med 3 px radius, ikke piller. Dialoger 8 px (var 14).
- `--shadow-card` er `none` i alle temaer. Popovere, menyer og dialoger beholder skyggen.
- Sidetittel 14 px (var 15) og store tall på Oversikt 20 px (var 24).

**Komponenter**
- Panel (`.group`): 1 px ramme, ingen skygge, overskriften i en lav stripe med linje under. Panelets tittel er 12 px halvfet.
- Etikett–verdi (`.kv`): verdien står rett ved etiketten. Én etikettkolonne per panel med `subgrid`, så bred som den lengste etiketten (høyst 45 %), i stedet for etikett helt til venstre og verdi helt til høyre.
- Tabeller: tabelloverskrift med sterkere linje, 26 px rader. Underoverskrifter i Spesifikasjon med linje under.
- Bokslister (Innstillinger) med 40 px rader (var 52), infolinjer 30 px.

**Side for side**
- **Sensorer** (brukerønske): én tett tabell i ett panel i stedet for fem kort. Overskriftsraden står én gang og blir stående øverst når du ruller. Gruppert etter enhet (Skjermkort, Prosessor, Minne, Andre sensorer, Nett) med sammenleggbare gruppeknapper (navn, kilde og P-tilstand/begrensning). 24 px rader, tall høyrejustert med dempet enhet, 60 s-sparklinje i hver rad. Klikk på navnet (eller Enter) åpner en større graf for siste 60 s rett under raden, med akser, hårkors og kilde; én om gangen, Esc lukker. Piltaster mellom radene og mellom nål og navn. Feste, høyreklikk, nullstilling, min/maks som huskes, filter, «Fold sammen alle» og «Vis på Sensorer» virker som før. Ved 360 px: Sensor, Nå og Maks, og navnet har fått plass.
- **Oversikt**: «Temperatur og effekt» er ett panel med to deler (var to paneler), så detaljraden er like høy i alle tre kolonner uten tomrom. «Høyeste 5 min» heter «maks 5 min», med forklaring i verktøytipset.
- **Grafikkort og System**: verdiene ved etikettene, 26 px rader. Trådene på System er et rutenett av felt med felles kanter (var 16 små kort).
- **Kontroller, Viftekurve, Lys, Grafer, Innstillinger**: paneler og tettere rader. Endringer uten endringer er bare stripen. Effektlista på Lys har litt høyere rader, så panelene slutter likt.

**Rettet underveis**
- To tema- eller fargebytter rett etter hverandre ga en konsollfeil («AbortError: Transition was skipped») fra View Transitions. Fantes også i v0.0.8. Fanges nå i `viewTransition()`.
- Tastatursnarveier: tastene brøt over to linjer.

**Tester** (139 tester, ca. 38 s)
- `ui/smoke.test.js`: ny test for Sensorer: én tabell med én overskriftsrad som klistrer seg, gruppene i rekkefølge, høyrejusterte tall, sammenlegging (rader skjult og `aria-expanded`), grafen under valgt rad (størrelse, flytting til en annen rad, lukking), piltaster med én tab-stopp per gruppe, Enter og Esc med fokus tilbake. Ingen eksisterende test er endret eller svekket; feste, nullstilling og «Snitt»-verktøytipset bruker de samme selektorene som før.
- `scripts/contrast.js`: `--panel-head` er med blant flatene (all tekst må ha 4,5:1 på stripen). Dårligste tekst 4,51:1 og dårligste UI-grense 3,01:1, som før.

Skjermbilder før og etter av alle ni sider i lyst og mørkt ved 1440 × 900, fem sider ved 360 px og noen tilstander (graf på Sensorer, rullet tabell, 200 % zoom, bekreftelse med klemt verdi, høy kontrast) ligger i `docs/skjermbilder/v0.0.9/for/` og `etter/`. Tatt i headless Chrome mot egne servere på tilfeldige porter. Port 4318 og brukerens socket er ikke rørt.

## alpha v0.0.8 · bygg 8 – 5. november 2026: finpuss etter daemon-flyten

Detaljrunde etter v0.0.7, med vekt på bekreftelseslinja, grenser, klemming, frakoblet tjeneste og de tre viftemodiene. Funn for funn står i `docs/finpuss-v0.0.8.md`. Ingen nye funksjoner, og ingenting her skriver til skjermkortet eller maskinen.

**Veikartpunktene**
- Viftekurve: «Under 35 % er ikke mulig på kortet» er en merkelapp nederst i sonen (samme form som start og stopp), alltid til venstre for nødregelsonen og tegnet over linjene, så ingen stiplet linje krysser den. Etiketten for temperatur nå viker for «Nødregel 100 %».
- Viftekurve: kortene i sidekolonnen har sin egen høyde, så Oppførsel ikke har tomrom nederst når start/stopp er av (var 47 px ved 1440 × 900 og 227 px ved 1920 × 1080). Bare Oppførsel vokser når feltene glir fram.
- Kontroller og Viftekurve: Bruk får ingen hake når endringen venter på Behold, bare den vanlige deaktiverte formen med verktøytipset «Endringen venter på Behold».
- Effekt-grafen (Kontroller, Oversikt, Grafer): grensen tegnes som rene trappetrinn (`step: true` i `charts.js`), uten interpolasjon og på hele piksler.
- Flere faner: én ventende endring om gangen. Alle faner viser den og kan beholde eller rulle tilbake. Bare fanen som trykket Bruk, kan bruke mer før den er avgjort; de andre ser «i en annen fane», og Bruk er låst. Serveren avviser et konkurrerende Bruk med 409 (`pending_elsewhere`), også to i samme øyeblikk (et Bruk på vei teller). Den ventende endringen har feltet `tab` (fanen som eier den). Beholdt i en annen fane gir et varsel; lukket eierfane gir en egen grunn i de andre.
- Tastatur og fokus: fokus går tilbake til menyknappen etter Esc i en dialog åpnet fra hovedmenyen (havnet på «Til innhold»).

**Detaljrunden**
- × på bekreftelseslinja var rød etter tilbakerulling. Bare ikonet foran teksten har linjas farge nå.
- Tre endrede verdier skrives ut («Effektgrense, kjerneklokke-offset og viftemodus»), ikke «og 1 til».
- Frakoblet midt i fristen: varselet gjentar ikke det linja sier. «Bruk igjen» er låst med grunn mens serveren eller tjenesten er borte.
- Kontroller › Vifte: modusvelgeren står på samme sted i Auto, Kurve og Manuell, og raden får ikke ny høyde. Glider og tallfelt for Manuell står i midten.
- Oversikt › Viftestyring med samme ord som velgeren: «Auto · driveren styrer», «Kurve · følger viftekurven», «Kurve · hviler på 35 %», «Manuell · 60 %», «Nødregel · 100 %».
- Viftekurve sier «Ingen endringer» i toppfeltet (ikke «Kurven er brukt» ved siden av «Brukes ikke · Auto»).
- «Standard 245 W» og «ved 40 °C» brytes ikke midt i.
- Grafene har plass til «0 000» på y-aksen, så plottene i samme kolonne starter likt.
- 360 px: hele sidetittelen i toppfeltet (også med endringer), og forklaringen over viftekurven brytes som én rad.

**Regnbue og fargesyklus** (veikartpunkt fra v0.0.6)
- Fasen rundes til 1,5°. Mellom to trinn skrives ingen LED og beregnes ingen stil. Ved fart 5 skrives regnbuen 100 ganger i sekundet, uansett skjermfrekvens.
- Målt ved ubegrenset bildetakt i headless Chrome: skript per bilde 0,059 → 0,012 ms og stil per bilde 0,143 → 0,035 ms (regnbue), 0,029 → 0,008 ms og 0,067 → 0,030 ms (fargesyklus). Regnet om til 240 Hz: stil ca. 34 → 18 ms/s og skript ca. 14 → 7 ms/s for regnbuen. Ved 60 Hz er det uendret (0,13–0,15 ms skript per bilde, ca. 25 ms/s stil), fordi trinnene da kommer oftere enn bildene.

**Tester** (138 tester, ca. 38 s)
- `daemon-contract.test.js`: to klienter skriver i samme øyeblikk; én får txn, den andre `busy`, bare den første kan bekrefte, og den andre kan skrive etterpå.
- `daemon-integration.test.js`: to faner trykker Bruk samtidig mot serveren; én 200 og én 409 (`pending_elsewhere`), samme ventende endring med riktig eierfane i begge SSE-strømmene (aldri feil eier underveis), eierfanen kan bruke mer i samme txn, Behold fra den andre virker, og den andre kan bruke sitt etterpå.
- `ui/smoke.test.js`: tastatur og fokus (Tab-rekkefølge på Kontroller og Viftekurve, synlig fokusring på alt som får fokus, seks Tab-stopp på kurven, Esc lukker hovedmeny, dialog, kortvelger og profilmeny med fokus tilbake). To faner i samme nettleser som trykker Bruk samtidig: samme txn, nøyaktig én eier, Bruk uten hake, låst med grunn i den andre, 409 fra serveren, Behold fra den andre fanen og lukket eierfane.
- `test/helpers/chrome.js`: `newPage()` for en fane til, og `key()` sender tegnet for Enter og mellomrom, så knapper aktiveres som med et ekte tastatur.

Skjermbilder før og etter per funn, og alle ni sider etterpå i lyst og mørkt, er tatt i headless Chrome mot egne servere og sockets (port 4391 og tilfeldige porter i testene, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## alpha v0.0.7 · bygg 7 – 5. november 2026: klar for ekte bakgrunnstjeneste

Alle skrivende handlinger går nå til en bakgrunnstjeneste over en Unix-socket, slik den ekte appen skal gjøre det. Tjenesten er foreløpig en simulering (`daemon-mock/`) som kjører som vanlig bruker. Kontrakten den ekte Rust-daemonen skal følge, står i `docs/daemon-api.md`. Ingenting her skriver til skjermkortet eller maskinen.

**Kontrakten (`docs/daemon-api.md`)**
- Linjeavgrenset JSON (NDJSON) over Unix-socket, høyst 64 KiB per linje. Valgt framfor lengdeprefiks fordi JSON aldri har rå linjeskift, LACT gjør det samme, det er enkelt i både Node og Rust (`LinesCodec` med maks lengde), og det kan feilsøkes med `ncat -U`. For lange linjer gir `message_too_large`, og forbindelsen lukkes.
- `hello` med protokollversjon (1), evner (`ops`, `events`, RGB som «planlagt»), frist og forrige avslutning (`lastExit`). Forespørsel/svar med `id`, hendelser uten (`telemetry`, `state`, `txn`, `safety`, `shutdown`), og 18 feilkoder.
- Lesing (`list_gpus`, `get_limits`, `get_state`, `get_telemetry`, `subscribe`) og skriving (`apply`, `set_power_limit`, `set_clock_offsets`, `set_temp_target`, `set_fan`, `reset`, `confirm`, `revert`). Hver skriveoperasjon er koblet til NVML-funksjonen den ekte daemonen skal bruke (`nvmlDeviceSetPowerManagementLimit_v2`, `nvmlDeviceSetClockOffsets` med minne × 2, `nvmlDeviceSetTemperatureThreshold(ACOUSTIC_CURR)`, `nvmlDeviceSetFanSpeed_v2`/`SetDefaultFanSpeed_v2`/`SetFanControlPolicy`). `rgb_get`/`rgb_set` er plassholdere som alltid svarer `not_implemented`.
- Bekreft eller rull tilbake: hver skriving gir en txn med frist (standard 15 s, 10–60). Uten `confirm` ruller daemonen tilbake selv. Det samme skjer straks når klienten som skrev, kobler fra, og når daemonen stopper. Ubekreftede verdier lagres aldri.
- Sikkerhet i daemonen: klemming til kortets grenser (eller strengere policy for offset), viftekurve som aldri går under 35 % eller faller, full vifte over 85 °C uansett modus (slippes under 80 °C), og vifte til auto ved avslutning og krasj. For SIGKILL beskriver kontrakten `ExecStopPost=` og `WatchdogSec=` i systemd.
- Tilgang beskrevet, ikke laget: socket `0660 root:agentic-rigg`, `SO_PEERCRED`, polkit-handlingene `…tune`, `…advanced` og `…rgb`, og systemd-herding.

**Bakgrunnstjenesten (`daemon-mock/`)**
- `card.js`: simulert kort med grenser, klemming, txn med frist, nødregel og lagring av det bekreftede i `AG_DATA_DIR/daemon-mock.json`. Bruker modellen i `server/simulator.js`. Første oppstart tar med verdiene fra `state.json`, så ingenting forsvinner ved oppgraderingen.
- `daemon.js`: socket i en privat mappe (0700) med 0600, sjekk av eier og symlenke, stien høyst 107 byte, nekter å starte hvis en annen daemon svarer på socketen, høyst 16 klienter, og ingen telemetri til klienter som henger etter (over 1 MiB i kø).
- `index.js`: SIGTERM/SIGINT/SIGHUP og krasj ruller tilbake, setter viften til auto, sier fra til klientene og fjerner socketen. Neste oppstart vet om forrige endte rent.
- Ingen barneprosesser, ingen NVML, ingen `/dev`, ingen root. Sjekkes av `nvidia-safety.test.js`.

**Serveren**
- `server/daemon-client.js` (`node:net`): hello, abonnement, grenser og tilstand ved tilkobling, og gjenoppkobling med 0,5 → 5 s ventetid. `server/daemon-protocol.js` er felles for klient og mock (versjon, feilkoder, socket, oversetting mellom appens felt og kontraktens).
- `/api/apply`, `/api/reset` og de nye `/api/confirm`, `/api/revert`, `/api/tab/close`, `/api/daemon` og `/api/daemon/retry` går gjennom tjenesten. Uten tjeneste svarer de 503 med «Bakgrunnstjenesten er ikke tilkoblet. Ingenting er endret.» Feilkodene fra tjenesten blir HTTP 400/409/501/503/504 med norsk melding.
- SSE-hendelsen `daemon`: tilkoblet, ventende endring (med det som er endret, klemt og verdiene fra før), grenser fra kortet, nødregel og siste utfall.
- Fanen som trykket Bruk, følges med en fane-ID. Lukkes eller lastes den på nytt midt i fristen, ruller serveren tilbake straks, slik tjenesten gjør når en klient forsvinner. Siden sier fra selv ved `pagehide` (fetch med `keepalive`), fordi Chrome kan holde SSE-strømmen åpen i tilbake/fram-hurtigbufferen.
- Simulerte prøver kommer fra tjenesten når den er tilkoblet (der kurven og nødregelen kjører). Ellers tar den lokale modellen over med siste kjente verdier, uten hopp.
- `npm start` kjører `scripts/dev.js`: starter mocken, venter på socketen, starter serveren med samme socket og datamappe, starter mocken igjen etter krasj (høyst fem ganger i minuttet) og stopper begge sammen. `.claude/launch.json` virker uendret. Ingen barneprosesser i `server/` utenfor `nvidia.js`.

**Viftemodus med tre valg**
- Kontrakten skiller driverens auto, kurve og manuell, og «vifte til auto» må bety driveren. Derfor har Vifte nå Auto · Kurve · Manuell. Før betydde «Auto» kurven. Filer fra før v0.0.7 (`state.json` uten `schema`) leses slik: «auto» med egen kurve eller start/stopp på blir «Kurve», ellers driverens auto (som i simuleringen oppfører seg likt).
- Viftekurve: merket ved tittelen sier «I bruk» eller «Brukes ikke · Auto», og «Bruk kurven» setter modusen i utkastet. Bruk her tar modusen med.

**Bekreft eller rull tilbake i UI** (Kontroller, Viftekurve og vifte-modus)
- Etter Bruk står en rolig linje under toppfeltet: «Behold endringene? Effektgrense og viftemodus er tatt i bruk.», «Ruller tilbake om 12 s», Rull tilbake nå og Behold. En tynn aksentlinje nederst krymper mot null (transform, ett sekund om gangen).
- Toppfeltet sier «Venter på Behold» med blå prikk, og Endringer-kortet det samme.
- Fokus flyttes ikke, bortsett fra når Bruk-knappen du trykket på, ble låst: da går fokus til Behold. Ctrl+Shift+Enter er Behold på alle sider (står i Tastatursnarveier).
- Skjermlesere får én melding når nedtellingen starter og én når den er ferdig. Sekundene er `aria-hidden`, og linja er en region med navn.
- Rullet tilbake: nøytral linje med grunnen («Fristen gikk ut.», «Du valgte å rulle tilbake.», «Siden ble lukket eller lastet på nytt før du trykket Behold.», «Forbindelsen til bakgrunnstjenesten brøt.»), «Bruk igjen» og ×. Utkastet følger med tilbake hvis du ikke har endret noe siden. Blir stående i 20 s (lenger mens pekeren eller fokus er i den).
- Fristen velges under Innstillinger › Generelt › Bakgrunnstjeneste: 10, 15, 30 eller 60 s. Der står også status for tjenesten.
- Med «Av» kjører ingen animasjon (linja hopper hvert sekund). Med «Redusert» toner linja bare inn.
- I 360 px bredde skjules ikonet, og knappene legger seg under teksten.

**Grenser og sikkerhet i UI**
- Glidernes ender er grensene kortet melder gjennom tjenesten, med verktøytips («Kortets maks, meldt av bakgrunnstjenesten» eller «Tjenestens maks. Kortet melder −1000 til +1000 MHz …»). Merket «Grenser fra kortet» i Justeringer.
- Tallfeltene lar deg skrive utenfor. Raden sier «Over kortets maks. Klemmes til 275 W.» før Bruk, linja sier hva tjenesten klemte, og raden viser «Klemt av tjenesten: 300 W → 275 W» så lenge verdien står.
- Nederst i Justeringer, uten varselfarge: «Over 85 °C går viften alltid 100 %. Stopper bakgrunnstjenesten, går viften til auto. Uten Behold rulles endringer tilbake etter 15 s.»
- Viftekurve: nødregelen er en rolig sone fra 85 °C med «Nødregel 100 %», gul «Nødregel aktiv» mens den gjelder. Vifte-raden på Kontroller og Oversikt sier det samme.
- «Bakgrunnstjeneste: ikke tilkoblet. Endringer kan ikke brukes nå. Prøver igjen om 3 s.» med Prøv nå, bare på Kontroller og Viftekurve. Bruk er låst med «Venter på bakgrunnstjenesten». Stopper tjenesten, sier et varsel at viften gikk tilbake til auto.

**Rettet underveis**
- Chrome holdt SSE-strømmen åpen etter at siden var forlatt (tilbake/fram-hurtigbufferen). Etter seks slike sider fikk nye kall ingen ledig forbindelse. Strømmen lukkes nå ved `pagehide` og åpnes igjen ved `pageshow`. Funnet med skjermbildeskriptet.
- Svaret på Bruk og SSE-hendelsen kan komme i begge rekkefølger; samme txn tegnes én gang, og fokus går fortsatt til Behold.

**Tester** (134 tester, ca. 36 s)
- Ny `daemon-contract.test.js` (13): hello og versjon, socket-rettigheter og én daemon per socket, lesing, hver skriveoperasjon og `confirm`, klemming (kort, policy, kurve), feilkoder, tilbakerulling ved frist og ved frakobling, nødregel med hysterese, testoperasjonen bare med `AG_MOCK_TEST_OPS`, avslutning (shutdown, vifte til auto, ubekreftet ikke lagret, SIGKILL oppdages), ugyldig JSON og for store meldinger, og import fra `state.json`.
- Ny `daemon-integration.test.js` (10): `/api/state` med tjenesten, Bruk → Behold, Rull tilbake nå og frist, fane som lukkes (SSE) og `pagehide`, prøver og nødregel fra tjenesten, 400 med norsk melding, tjenesten som stopper og kommer tilbake, server uten tjeneste, og server som dør midt i fristen.
- `ui/smoke.test.js`: ny test av hele flyten (klemt verdi, fokus, én skjermlesermelding, Ctrl+Shift+Enter, Rull tilbake nå, frist med Bruk igjen, omlasting, tjenesten borte og tilbake). «Av»-testen viser nå også bekreftelsen og tilbakerulling, fortsatt 0 animasjoner.
- `nvidia-safety.test.js`: `daemon-mock/` starter ingen programmer, laster ikke NVML og rører ikke `/dev`, og `scripts/dev.js` starter bare de to Node-filene. `store.test.js`: viftemodus fra eldre filer og `confirmSec`. `simulator.test.js`: driverens auto og nødregelen. `http.test.js`: Origin og feil type for de nye kallene.
- Testhjelperen starter en egen mock for hver testserver, med socket i en midlertidig mappe.

Skjermbilder (lyst og mørkt, nedtelling, tilbakerullet, klemt verdi, tjenesten frakoblet, nødregel, 360 px og alle ni sider) er tatt i headless Chrome mot egne servere på tilfeldige porter.

## alpha v0.0.6 · bygg 6 – 5. november 2026

Bygg 6 har to deler: «Kritikkrunde 2» (funnene i `docs/design-kritikk-v0.0.5.md`) og «Ønskeliste» (kapittel 5 i samme dokument). Status per funn står i `docs/design-kritikk-status.md`.

### Kritikkrunde 2

Alle høye og middels funn er rettet, de tre kjente punktene i kapittel 3 også, og alle lave funn er tatt (fire på en annen måte enn foreslått, med grunn i statuslisten). Hvert funn er sett i headless Chrome før og etter, i lyst og mørkt tema, ved 1440×900, 1100×800, 1920×1080 og 360 px.

**Lys**
- Forhåndsvisningen var tom (0 × 0) under ca. 1240 px og i lave vinduer. Scenen måles nå med en `ResizeObserver`, og kortet får størrelsen i px. I enkolonne får scenen høyden fra bredden.
- Justering er slått inn i Effekt-kortet, så raden under scenen er to like høye kort: Effekt og Farge.
- «Støttet» er borte. Bare effektene som er ren forhåndsvisning, har et lite øye, forklart i overskriften. Ved 360 px krymper sonevalget i stedet for å gå ut av kortet.

**Oversikt**
- Skjermkort: GPU-last og minnekontroller (som står i flisene og Last-grafen) er byttet med Videominne med stolpe, og P-tilstand står sammen med begrensningen («P8 · tomgang»).
- Kortene i detaljraden er like høye av seg selv. Klokker har fast radhøyde, og Temperatur og Effekt har forklaringen rett under stolpen, uten tomrom.
- Effektgrafen har minste spenn lik en fjerdedel av effektgrensen ved «Tilpass» (også Grafer og Kontroller).
- Temperaturskalaen ga vannrett rulling når kortet sto ytterst til høyre (1100 og 360 px): markørlaget telte med i rullebredden. Klippet med `overflow-x: clip` og 8 px margin. Funnet av den utvidede røyktesten.

**Kontroller**
- Ny fordeling uten tom kolonne: Justeringer og grafene Effekt og Kjerneklokke (siste minutt) til venstre, Endringer, Profiler og spennings- og klokkekurven til høyre.
- Endringer er én linje når ingenting er endret.
- Spennings- og klokkekurven har fast y-akse (1 000–2 500 MHz), så offset løfter kurven.
- Profiler: «Lagre som ny profil», merket «1 av 5», Hent og en ⋯-meny per profil, og «Lik «Profil 1»» i stedet for å lagre en kopi.
- I live-modus sier Vifte-raden at driveren styrer viften.

**Viftekurve**
- Kurven kan ikke falle (dra, piltaster, dobbeltklikk og tallfelt). Serveren løfter punkter som ligger lavere enn forrige.
- Nå-kortet er borte. Grafens etikett viser temperatur, kurvens verdi og viftefart nå. Oppførsel får plass ved 1440×900.
- «Hysterese» heter «Senk først etter» (Oppførsel og Endringer), så ordet kan brukes om start/stopp-terskelen i ønskelisten.
- «Kurve · Egendefinert», endringsprikk i Punkter og kortere sonetekst i smale vinduer.

**Sensorer**
- Toppfeltet: fold-knappen først og skilt fra søket, «Min/maks siden 23:37» ved ↺.
- Nå-kolonnen får gul og rød etter tersklene.
- «Ikke tilgjengelig» filtreres med søket.
- Én kolonne opp til 1560 px innholdsbredde, med navnet på 260 px og sparklinjen i resten. Ved 360 px står navnene på én linje med ellipse.

**System**
- Kolonnene er like høye. Instruksjonssett fyller resten med flere sett i hele rader, tallet står i overskriften («76 av 110»), og «Vis alle (N)» er en fast ikonknapp, så ingenting endrer bredde.
- «Ikke oppgitt» i stedet for «Ikke oppgitt av driveren» for systemdata, og ett verktøytips om moduler og timinger.

**Grafikkort og felles**
- PCIe skrives likt overalt: «x16 Gen 1 · maks Gen 3».
- Klokker på Grafikkort har stolper som på Oversikt. Simulerte verdier er «–» med verktøytipset «Simulert kilde».
- Frakoblet: Bruk er låst («Venter på serveren»), og statusblokken dempes.
- Små sparklinjer har en stiplet grunnlinje i stedet for en grå flate før det finnes data.
- Grafer: merket «Pauset 00:18» og «pauset» på tidsaksen, og én tidsenhet per vindu.
- Deaktivert Bruk er nøytral grå (i mørkt tema så den aktiv ut).
- Tekstvask: ingen `AG_SIMULATE=1`, «Research fant» eller filstier utenfor Feilsøking, én setning for Datakilde, «Tilbakestill» uten ellipse, riktig etikett i Klokkeslett-teksten, versjonen skrevet likt overalt, én tast per knapp i Tastatursnarveier og `aria-label` på alle ikonknapper.
- Innstillinger: Datamappe på én rad med ellipse i midten og kopier-knappen synlig, og avkrysningslistene (også Sider) i tre kolonner.

**Server** (bare tekst og validering, ingen nye kall og ingen nye programmer)
- `telemetry.js`: grunnen ved tvunget simulering er «Valgt ved oppstart av serveren».
- `index.js`: forklaringen om lyskontrolleren uten «Research fant».
- `system.js`: forklaringer uten «Ikke tilgjengelig –» og «(dmidecode)».
- `store.js`: viftekurven normaliseres så den aldri faller.

**Tester** (83 tester, ca. 30 s)
- Røyktesten kjører nå ved 1440, 1100 og 360 px og feiler hvis en synlig hovedflate (kort, graf, lerret, Lys-scenen eller -kortet) er 0 × 0.
- Ny røyktest for Lys-forhåndsvisningen ved 1440×900, 1100×800, 1000×800, 360×780 og 1440×560: kortet har størrelse, holder 660 × 280 og fyller minst halve scenen. Begge testene feiler mot v0.0.5.
- `store.test.js`: viftekurve som faller, løftes (fil fra v0.0.1 og nye kall).
- `http.test.js`: tekstene fra serveren er uten utviklerspråk.

### Ønskeliste

De fire ønskene fra veikartet, laget etter forslagene i kapittel 5 i designkritikken. Alt er fortsatt simulert: ingenting her styrer viften eller noe annet på maskinen. Sett i headless Chrome i lyst og mørkt ved 1440×900, 1100×800 og 360 px, og alle ni sider etterpå.

**Viftekurve: start og stopp med hysterese** (brukerens eget ønske)
- Ny rad øverst i Oppførsel: «Start og stopp» med bryter (av som standard). Når den er på, glir to felt fram under raden: «Start ved 55 °C» og «Stopp under 48 °C», og undertittelen sier «Hysterese 7 °C. Under 48 °C holder viften 35 %.»
- Ærlig om kortet: RTX 2080 tillater ikke under 35 % og har ikke viftestopp, så «stopp» betyr at viften hviler på minste fart. Det står i undertittelen («Viften hviler på 35 % når det er kjølig»), i verktøytipset på raden og i grafen («Minste fart 35 %»). Ingen setning om 0 %.
- I grafen: to stiplede loddrette linjer med merkelapper ved x-aksen («stopp 48», «start 55») og et skravert bånd mellom dem. Linjene og båndet kan dras (båndet flytter begge og beholder avstanden, Shift smekker til 5 °C). Tab går videre fra punktene til start og stopp, og piltastene flytter dem. Til venstre for stopp er kurven dempet og stiplet, og viften hviler på en flat linje (stiplet i båndet, der den gjelder på vei opp). Etiketten for temperatur nå sier «(hvile)» når viften hviler.
- Start ligger alltid minst 3 °C over stopp. Skriver du en verdi som bryter det, flyttes den andre, og et varsel sier hvor («Stopp er flyttet til 87 °C»).
- Kontroller › Vifte: «Følger viftekurven fra 55 °C · nå 35 % (under start)» når viften hviler. Endringene vises i Endringer på Kontroller («Start og stopp: Av → På»).
- «Senk først etter» (hysterese per trinn) er uendret og heter fortsatt det, så de to begrepene ikke blandes.
- Undertitlene i Oppførsel er kortet ned («Temperaturfall før viften går ned», «Hotspot oppgis ikke av driveren», hele teksten i verktøytipset), så Oppførsel fortsatt får plass ved 1440×900 med den nye raden.
- Server: `fanStartStop`, `fanStartTemp` (30–90 °C) og `fanStopTemp` (25–87 °C) i `applied` og i profiler, validert i `store.js` (start minst 3 °C over stopp, også når valget er av). Gamle filer får standardverdiene, og et ugyldig par i fila gir standardparet uten å nullstille resten. `simulator.js` har en egen funksjon `restState()`: i hvile til temperaturen når start, på kurven til den er under stopp, og tilstanden beholdes mellom de to. Simulerte prøver har `fanRest`.

**Min og maks huskes mellom økter**
- På som standard. Begrunnelse: en ny lasting, en ny fane eller en omstart av serveren nullstilte tidligere toppene uten at du ba om det, og det var nettopp det ønsket gjaldt. Nullstilling er fortsatt ett trykk (↺) med Angre, og tidspunktet viser hvor gamle tallene er.
- Valget står under Innstillinger › Grafer og sensorer › Sensorer: «Husk min og maks mellom økter», med undertittelen «Nullstilles bare når du trykker ↺ på Sensorer. Snitt gjelder alltid denne økten.» Slås det av, glemmer serveren det som er lagret.
- «Min/maks siden 2. nov. 20:07» får dato når den er en annen dag (og år når det er et annet år). Verktøytipset på Snitt-overskriften er «Snitt for denne økten», og Angre-varselet sier hvor gamle tallene var.
- Lagres på serveren i `AG_DATA_DIR/sensor-minmax.json` (egen fil, ikke i `state.json`, så den ikke er med i eksport og tilbakestilling). Fanen sender høyst hvert 10. s når noe er endret, og ved lukking. Serveren fletter (laveste min, høyeste maks) og skriver fila høyst hvert 30. s, og straks når den stoppes. En ødelagt fil gir et tomt sett og en kopi for feilsøking. GPU-radene gjelder bare samme kilde (live eller simulert).
- Nullstilling og Angre i én fane når de andre straks (BroadcastChannel). En fane som ikke har fått med seg en nullstilling, kan ikke skrive gamle tall tilbake.
- Nytt API: `GET /api/minmax`, `POST /api/minmax` (flett, eller `replace: true` ved nullstilling og angre) og `DELETE /api/minmax`.

**Eget appikon**
- Et skjermkort sett forfra: mørk flis i Adwaita-grått (samme form og underkant som ikonet til Agentic Klipp), én tydelig vifte med hvite blad og blått nav, og PCIe-kontakten i gull under kortet. Ingen bokstav og ingen regnbue.
- Tre varianter ble tegnet og sett ved 16, 20, 32, 64 og 128 px på lys og mørk bakgrunn: skjermkort skrått forfra med brakett (pent ved 128 px, men for bredt og grumsete ved 16–32 px), flis med vifte (leselig i alle størrelser) og flis med viftekurve (leselig, men ser ut som et hvilket som helst overvåkingsverktøy). Valgt: flis med vifte, med PCIe-kontakten fra den første varianten, så flisen leses som et skjermkort også ved 16 px.
- Filer: `web/icons/agentic-rigg.svg` (fullfarge) og `web/icons/agentic-rigg-symbolic.svg` (16 px, én farge), og for senere `.desktop`/Electron `assets/icons/hicolor/…/io.github.AgenticRigg.svg`, `…-symbolic.svg` og PNG i 64–512 px i `assets/icons/png/`.
- Brukt som favicon (har egen flate, så den virker på lys og mørk fanelinje), i toppen av sidefeltet (20 px) og i Om (80 px). Det gamle sekskantikonet (`web/icon.svg`) er fjernet.

**Feste sensorer og rekkefølgen i statusblokken**
- Sensorer: et feste-ikon (nål) til venstre for navnet vises når pekeren er over raden eller raden har fokus. Festede rader har nålen fylt hele tiden, dempet. Klikk fester eller løsner, og høyreklikk på raden gir samme valg. Ingen ny kolonne. Med tastatur er nålene én tab-stopp per gruppe, og piltastene flytter mellom dem.
- En rad som er det samme som en fast verdi (f.eks. GPU-temperatur), festes som den faste verdien, så navn og format blir like.
- Statusblokken viser opptil 8 verdier (ca. 8 rader får plass ved 1440×900 uten rulling) i rekkefølgen du har valgt. Festede sensorer får korte navn («CPU-tråd 3», «CPU Tdie», «VRAM-klokke», «wlo1 ned») med hele navnet i verktøytipset, og hele grader og prosent som de faste verdiene. Er blokken full, sier et varsel fra («Statusblokken er full (8 av 8)») med snarvei til Innstillinger.
- Høyreklikk (eller menytasten) på en statusrad: Flytt opp, Flytt ned, Løsne, Vis på Sensorer og «Rekkefølge og utvalg …».
- Innstillinger › Sidefelt: avkrysningsrutenettet er byttet med en ordnet liste, «Status nederst 5 av 8». Hver rad har håndtak, navn, kilde og navnet i sidefeltet, plassnummer og ×. Dra i håndtaket (raden følger pekeren og naboene glir unna, bare transform), eller velg raden og bruk Alt+↑/↓ (fokus blir på raden). «Legg til verdi …» åpner en popover med søk i faste verdier og alle sensorene, med verdien nå til høyre.
- Tomtilstand: lista sier «Statusblokken er tom», og sidefeltet viser «Fest sensorer fra Sensorer» (lenke dit) i stedet for å skjule blokken.
- Endringer når andre faner gjennom innstillingene (BroadcastChannel, som før).
- Server: `statusItems` er en ordnet liste med høyst 8 verdier: faste ID-er eller rad-ID-er fra Sensorer (`gpu/…`, `cpu/…`, `mem/…`, `net/…`, `hw/…`, ingen `.` eller `..` som del). Gamle lister (høyst fem, fast rekkefølge) leses som før.

**Rettet underveis**
- Innstillinger tegnet seg ikke på nytt når en annen endring kom mens en lagring ventet på svar (flagget `selfChange` gjaldt hele ventetiden). Nå gjelder det bare selve endringen. Funnet av den nye røyktesten.
- `.picker` var allerede navnet på fargevelgeren på Lys. Søkepopoveren heter `.add-picker`, ellers fikk Lys vannrett rulling ved 360 px. Funnet av røyktesten.

**Ikke gjort: regnbue og fargesyklus skriver fortsatt 56 LED-farger per bilde.** Et `hue-rotate`-filter på LED-laget ville gitt andre farger enn i dag (filteret er en tilnærming som bleker mettede farger), og animerte CSS-egenskaper per LED gir like mange stilberegninger. Ingen av delene er målt i dette bygget, og jeg fant ingen løsning som tydelig ville vært billigere, så punktet står i veikartet.

**Tester** (105 tester, ca. 35 s)
- `store.test.js` (31): start/stopp (standard, grenser, minst 3 °C, par i fila, profiler, filer fra før v0.0.6), statusblokken (rekkefølge, festede ID-er, høyst 8, ugyldige ID-er, gamle lister), «Husk min og maks», min/maks-fila (validering, fletting, nullstilling i annen fane, angre, ny kilde, sjelden skriving, flush, ødelagt fil, dårlige rader).
- Ny `simulator.test.js` (4): `restState()` og den simulerte viften: opp forbi start gir kurven, ned under stopp gir minste fart, mellom beholdes tilstanden, minste fart over 35 %, av og manuell gir ingen hvile.
- `http.test.js` (32): `/api/apply` med start/stopp (og 400 ved for liten avstand), `/api/minmax` (hent, flett, slå av, 409, 400), Origin-sjekk og feil type for de nye kallene, og at ikonfilene serveres som `image/svg+xml`.
- `ui/smoke.test.js` (5): ny test for ønskelisten: bryter og felt for start/stopp, Bruk, `fanRest` i prøvene og teksten på Kontroller, feste fra Sensorer (også som fast verdi), full blokk, Alt+↓ i Innstillinger med fokus, sidefeltet følger, tomtilstand, nullstilling av min/maks på serveren, verktøytipset på Snitt og appikonet.

## alpha v0.0.5 · bygg 5 – 4. november 2026: sikkerhetsnett

Automatiske tester, så senere runder ikke ødelegger det som virker. Bare Nodes innebygde `node:test` og `node:assert`, ingen nye pakker. `npm test` kjører alt på ca. 20 s.

### Tester

- Ny mappe `test/` med sju testfiler og 80 tester. `npm test` kjører `node --test test/`, `npm run test:server` bare servertestene og `npm run test:ui` bare UI-testen. `npm run check` sjekker også filene i `test/` (46 filer).
- **Tolking av nvidia-smi** (`nvidia-parse.test.js`, 16 tester): linjer fra `--query-gpu` for faste egenskaper og for løkka, med `[N/A]`, `[Not Supported]`, `N/A`, tomme felt, tekst i tallfelt, `Infinity`/`NaN`, CR, ekstra felt og avkortede linjer (alle lengder). `nvidia-smi -q` tolkes fra ekte utdata fra RTX 2080 med driver 610 (`test/fixtures/nvidia-smi-q-rtx2080.txt`, UUID og PDI nullet ut), fra eldre format uten «CUDA UMD Version», fra tom og avkortet tekst og med CRLF.
- **Sikkerhet i nvidia.js** (`nvidia-safety.test.js`, 7 tester): `assertReadOnly` avviser alle set-flagg fra `nvidia-smi -h` (`-pl`, `-lgc`, `-lmc`, `-pm`, `-rgc`, `-ac`, `--applications-clocks`, `-r`, `-e`, `-c`, `-mig` …), også bak lovlige argumenter. De tre argumentlistene er frosne (skriving kaster) og har forventet innhold. `nvidia-smi` startes uten shell og bare med de frosne listene, ingen annen fil i `server/` bruker `child_process`, og ingen kode åpner `/dev/i2c-*`, `/dev/mem` eller `/dev/port`.
- **Lagring** (`store.test.js`, 15 tester): de 15 tilfellene fra `scripts/test-store.js`, nå i `node --test`-format. Den gamle fila er fjernet.
- **HTTP** (`http.test.js`, 29 tester): serveren startes som barneprosess med `AG_SIMULATE=1`, ledig tilfeldig port og midlertidig `AG_DATA_DIR`. Feil eller manglende Host (også `localhost:4318` og DNS-rebinding-navn) avvises, fremmed Origin på alle skrivende kall gir 403 uten at noe endres, og kropp uten `application/json` gir 415. 20 varianter av sti-traversering (`/../`, `%2e%2e`, `%2f`, dobbel koding, nullbyte, ugyldig prosentkoding, `//etc/passwd`, `.data/state.json`) gir 4xx og lekker ingenting. Ukjent API gir 404. Alle GET-kall, og at `/api/apply`, `/api/reset`, `/api/rgb`, `/api/profiles/:n` (POST, PATCH, DELETE, PUT), `/api/settings` med import og tilbakestilling svarer `simulated: true`. Ugyldig JSON, gyldig JSON av feil type og for stor kropp gir 4xx, og serveren lever videre uten interne feil i loggen. `index.html` får utseendet skrevet inn i `<html>` og appnavnet satt inn.
- **SSE** (`sse.test.js`, 5 tester): `/api/stream` sender `hello`, `sample` hvert 500. ms og `sys`. Med to lyttere får den ene fortsatt data når den andre lukker, og når begge er borte stopper kilden etter 10 s (ikke før). En ny lytter starter kilden igjen og får historikken.
- **Live med falsk nvidia-smi** (`fake-smi.test.js`, 5 tester): serveren startes uten `AG_SIMULATE`, med `scripts/fixtures/fake-smi` først i `PATH`. Hele kjeden fra prosess til SSE prøves i vanlig modus og N/A-modus, og bytte mellom live og simulert. Etterpå er prosessgruppen (server og falsk `nvidia-smi`) borte.
- **UI-røyktest** (`ui/smoke.test.js`, 3 tester): headless Chrome (`--headless=new`) mot en egen simulert testserver. Alle ni sider i lyst og mørkt tema ved 1440 og 360 px: riktig sidetittel og overskrift, siden har innhold, ingen vannrett rulling i dokument, `body` eller `#content`, og ingen konsollfeil, unntak eller loggfeil. Animasjonsnivå «Av» gir `document.getAnimations().length === 0`, målt i hvert bilde under sidebytte, varsel, meny, temabytte og regnbue på Lys. Som kontroll ser den samme målingen animasjoner med «Full». Testen hoppes over med en melding hvis Chrome mangler.
- Hjelpere i `test/helpers/`: `server.js` starter testservere og har en HTTP-klient der Host og rå stier kan styres. `chrome.js` er en liten CDP-klient med Nodes innebygde `WebSocket`. Chrome får egen profil i en midlertidig mappe, debug-port valgt av Chrome selv (lest fra `DevToolsActivePort`) og en død proxy for alt som ikke er localhost, så ingenting går ut av maskinen. Server og Chrome startes i egne prosessgrupper og drepes i `after()`, ved `exit` og ved SIGINT/SIGTERM.

### Feil funnet av testene

- `POST /api/mode` med JSON-kroppen `null` ga 500 «Intern feil» og et `TypeError`-stakkspor i serverloggen. Nå gir den 400 «mode må være live eller sim». Fanges av «gyldig JSON av feil type gir 400, ikke 500» i `http.test.js`, som prøver `null`, liste, streng, tall og `true` mot alle skrivende kall.

### Annet

- `server/nvidia.js` eksporterer `assertReadOnly` og de frosne listene `STATIC_ARGS`, `QUERY_ARGS` og `LOOP_ARGS` for testene. Listen over tillatte flagg og argumentlistene er uendret, og ingen ny kode starter programmer.
- Merk: det finnes ikke noe `GET /api/settings`. Innstillingene leses fra `GET /api/state` (`settings`), og testene sjekker dem der.

## alpha v0.0.4 · bygg 4 – 4. november 2026

Redesign etter `docs/design-kritikk-v0.0.3.md`. Status per funn står i `docs/design-kritikk-status.md`. Bevegelse og mikrodetaljer er gjort i en egen runde, beskrevet til slutt i dette bygget («Bevegelse og detaljer»), med regler i `docs/bevegelse.md`.

### Skall

- Delt toppfelt i GNOME-stil: sidefeltet har eget toppfelt (appikon, navn, `alpha`-merke og hovedmeny ≡), innholdet har sitt (sidetittel til venstre, sidens handlinger til høyre). Ingen dobbel sidetittel og ingen undertittel under den.
- Hovedmenyen har Innstillinger (Ctrl+,), Tastatursnarveier (Ctrl+?) og Om. Innstillinger er ute av sidelista. Versjonen står bare i Om-dialogen.
- Enhetskort øverst i sidefeltet: «GeForce RTX 2080», «● Live · driver 610.43.02». Klikk gir en popover med driver, VBIOS, CUDA, PCIe, intervall, Live/Simulert-veksler og «Vis alt om kortet». GPU-navn og driver står ikke lenger midt i toppfeltet.
- Statusblokk nederst i sidefeltet med 0–5 verdier du velger selv (GPU-temp, GPU-last, effekt, VRAM, vifte, CPU-temp, CPU-last, minne, nett ned/opp), sparklinje for siste minutt og gul/rød farge etter tersklene i Innstillinger. Klikk går til riktig sensor på Sensorer.
- Statusmerke i toppfeltet bare når noe er unormalt: «Simulert GPU» eller «Frakoblet». Ved frakobling vises en infolinje med nedtelling og «Prøv nå», og verdiene dempes.
- Smalt vindu (under 860 px): bare ikoner, med egne verktøytips. Sidefeltet er én tab-stopp med piltaster. Hopp-lenke «Til innhold». `scrollbar-gutter: stable`.
- Vindustittelen får «(simulert)» når kilden er simulert. Ruten for Grafer heter nå `#/grafer` (`#/overvaking` sendes videre).

### Grunnmur

- Alle farger er semantiske CSS-tokens med én kilde: `:root` (lyst), `:root[data-resolved='dark']` (mørkt) og `:root[data-accent=…]` (aksent). `data-resolved` settes av `theme-boot.js`/`theme.js`.
- Fontstakk `Ubuntu Sans, Adwaita Sans, Cantarell, Inter, system-ui`. Fem skriftstørrelser, to vekter, 4-punkts avstander og tre komponenthøyder. Ingen store bokstaver med sperring.
- Kontrast etter WCAG AA i begge temaer og for alle ni aksentfarger (dårligste tekstpar 4,5:1). Lys knott med skygge på glidere og brytere.
- Ikoner fra Lucide (ISC) som inline SVG-sprite, ingen CDN. Unike ikoner per side; Grafikkort bruker Lucides `gpu`. Lisens i `docs/LISENSER.md`.
- Komponenter: etikett/verdi-rader uten feltramme (felt bare der du kan skrive), knapper (`flat`, `suggested`, `destructive`, `icon-only`), segmentkontroll i AdwToggleGroup-stil, brytere, glidere med standardmerke, tabeller med tekst venstre og tall høyre, merker (`.tag`), stolper på 8 px, egne verktøytips (500 ms, med hurtigtast), popover/meny, dialoger, varsler med «Angre», tomtilstand og plassholdere.
- Synlige fokusringer overalt, også på valgt segment, aksentknapper og canvas.
- Tall og enheter skilles med smalt hardt mellomrom (U+202F), også «38 %». Ekte minustegn.

### Sider

- **Oversikt:** nøkkelrad med seks fliser (stort tall, enhet dempet, sparklinje). Skjermkort (PCIe, P-tilstand, begrensning, laster), temperatur som skala fra 30 °C til avstenging med merker for mål og nedklokking og teksten «58 °C under nedklokking» (erstatter den gale «Mot nedklokking 37 %»), effekt som én stolpe «38 W av 245 W» med grensemerke, klokketabell med Nå/Maks. Tre grafer (Last i %, Temperatur, Effekt) fyller resten av høyden, så °C og % aldri deler akse. Pillen «Simulert oppsett … 0/0 MHz» er fjernet. Passer uten rulling på 1280×800.
- **Sensorer:** grupper som kort i to kolonner, sparklinjer på 196 px tegnet mot tid (alle starter likt, riktig vindu ved alle intervaller, «samler»-flate de første sekundene). Én rad per tråd i en lukket «Per tråd»-undergruppe med klokke ved siden av navnet. Menneskelige navn (f.eks. «USB-C-kontroller (skjermkort) · strøm», «Wi-Fi (wlo1) · ned») med det rå navnet i verktøytipset. Fast nettrate-enhet per rad. P-tilstand og begrensning som merker i gruppeoverskriften. PCIe som «Gen 3», snitt «—». Filter (Ctrl+F), Fold alle og Nullstill (med Angre) som ikonknapper. «Ikke tilgjengelig» med lås-/strek-ikon.
- **Grafer:** pene aksemerker, y-akse «Tilpass» eller «Fast», tidsvindu 1/2/5/15 min, tidsakse i «−1 min», rutenett som fyller høyden, synkronisert hårkors, pause som ikon (mellomrom). Grafene kan velges (også CPU-last, CPU-temp, minne, nett og VRAM).
- **Grafikkort:** etikett/verdi-rader, forklaringer bak (i). Klokketabellen uten den døde «Standard»-kolonnen. Spesifikasjonen i to kolonner med nøytralt «Referanse, ikke målt»-merke. UUID med øye og kopier. Kopier som tekst eller Markdown.
- **System:** «GNOME (Ubuntu-økt) · Wayland», «Bygg #31 · 1. aug. 2026», «6. mars 2019», «Mikrokode-revisjon». Cache-tabellen justert riktig. Tråder i 8 × 2 fliser med stolpe = last. Swap «Full» med forklaring. Søk i instruksjonssett. Node-versjonen er flyttet til Om.
- **Kontroller (simulert):** én infolinje (kan lukkes), hver kontroll på én rad med glider, standardmerke, tallfelt og ↺. Endringsprikk i stedet for «· endret». Viften viser kurvens verdi i Auto i stedet for en død glider. Forkast/Bruk i toppfeltet med grunn i verktøytipset når de er av. «Endringer» viser bare forskjeller. Profiler: fylte som kort, tomme som én rad, «Hent» i stedet for «Last», nytt navn med blyant/dobbeltklikk, tømming med Angre. Ny illustrasjon av spennings- og klokkekurven (merket «Illustrasjon»).
- **Viftekurve (simulert):** kurven fyller høyden, aksetitler, brukt kurve stiplet mens du redigerer, Shift smekker til 5 °C/5 %. Punkttabellen kan redigeres, med «Legg til punkt» og slett. Forhåndsvalg som knapper, navnet vises i tittelen. Ny gruppe «Oppførsel»: minste fart, hysterese, reaksjonstid opp/ned og temperaturkilde (GPU/CPU). Simulatoren bruker dem.
- **Lys (simulert):** forhåndsvisning i full bredde på mørk scene, soner (Alle/Lyslist/Vifte 1/Vifte 2) og valgfritt bakgrunnslys. Effekter som liste med «Støttet»/«Bare forhåndsvisning». Ny effekt **Temperatur** (kald → varm etter GPU-temperaturen i et område du velger). Egen fargevelger med flate, fargetone, hex-felt (godtar `3a7bf0`, `#3A7BF0`, `#38e`, `rgb(…)`), ni dempede LED-farger og nylig brukte. Hastighet og retning vises bare for effekter som bruker dem. Statusen er ærlig: «Lyskontroller: ikke koblet til (simulert)», research-detaljen i (?).
- **Innstillinger:** GNOME-oppsett med seks seksjoner: Generelt (startside, intervall, datakilde, simulert ved oppstart), Utseende (tema som tre forhåndsvisninger, ni aksentfarger, tetthet, skriftstørrelse, animasjoner Automatisk/Full/Redusert/Av), Enheter og format (°C/°F, GiB/GB, MB/s/Mbit/s, desimaler, 12/24 t), Sidefelt (statusverdier, sparklinjer, varselterskler, vis/skjul sider, hurtigtaster), Grafer og sensorer (tidsvindu, y-akse, fylt flate, hvilke grafer, sparklinjer) og Data (datamappe, eksport/import som JSON, vis skjulte meldinger, tilbakestill alt med Angre). Alt trer i kraft straks og lagres i `state.json`.

### Animasjonskrok

- `data-motion` på `<html>` er alltid `full`, `reduced` eller `off`. Valget «Automatisk» følger `prefers-reduced-motion` live. `--dur-1…4` og `--ease-*` er definert og nulles for redusert/av. Den gamle regelen som fjernet alle overganger er borte. Stolper, viftekurvens markør og RGB-forhåndsvisningen hopper eller står stille ved redusert/av, og grafene tegnes da i trinn.

### Server

- `server/store.js` validerer alle nye innstillinger (lister med lovlige verdier, klemming av tall, terskler der rød må være over gul) og leser gamle `state.json` felt for felt. Nye felt i `applied` (minste viftefart, hysterese, reaksjonstid, temperaturkilde) og `rgb` (soner, retning, temperaturfarger og -område, bakgrunnslys, nylig brukte).
- Nye kall: `POST /api/settings/reset`, `POST /api/settings/import`, `PUT /api/profiles/n` (angre tømming), `PATCH /api/profiles/n` (nytt navn). `/api/state` gir også standardverdier og datamappa.
- Uendret sikkerhet: bare de frosne, lesende `nvidia-smi`-kallene i `server/nvidia.js`, ingen nye programmer, alle «Bruk» simulert.

### Kanttilfeller og temaer

Egen runde for robusthet og temapersonalisering. Alle funn med test og status står i `docs/edge-cases.md`.

**Temaer**

- Fire fargetemaer med egne navn, hvert i lyst og mørkt: **Grafitt** (nøytral grå, standard), **Sand** (varm, som papir), **Fjord** (kjølig blågrå) og **Midnatt** (dypt mørkt, rent hvitt i lyst tema). De endrer bare flater, rammer og tekst, så aksentfargen og statusfargene virker likt i alle.
- **Høy kontrast** som egen bryter, over alle temaer: svart/hvit tekst, mørkere dempet tekst, rammer og feltkanter med 3:1, sterkere rutenett og grafserier, tykkere linjer i grafene, kant på stolper, glidere og brytere og 3 px fokusring.
- Innstillinger → Utseende: «Lyst eller mørkt» er nå en segmentkontroll, og de tre gamle temakortene (med rå fargeverdier) er erstattet av fire miniatyrkort for fargetemaet. Miniatyrene får temaets egne tokens via `.pv[data-palette]`, så de viser nøyaktig det du får, i lyst eller mørkt etter det som vises nå. Valget trer i kraft straks.
- Fokusringen er hel aksentfarge (før 70 % gjennomsiktig, under 3:1 med skifer-aksent). Feltkanter har 3:1. Bryteren har kant når den er av. Nytt token `--accent-fill` for stolper og glidere (3:1 mot sporet).
- `npm run contrast` (`scripts/contrast.js`) leser seksjon 2 i `styles.css` og regner ut kontrasten for alle 144 kombinasjoner (4 temaer × lys/mørk × 9 aksenter × høy kontrast av/på): 4,5:1 for tekst og 3:1 for UI-grenser. Dårligste tekstpar er 4,51:1, dårligste UI-grense 3,01:1.
- Canvas-grafer leser fargene på nytt ved bytte av tema, fargetema, aksent og kontrast. Sparklinjene følger av seg selv.
- GNOME-aksentfargen leses ikke: det krever `gsettings`, en dconf-parser eller D-Bus (se `docs/edge-cases.md`).

**Grafer**

- Minste spenn på y-aksen ved «Tilpass»: 20 °C (36 °F), 40 W, 400/1000 MHz og 1 GiB. En nesten flat temperatur ser flat ut. Gjelder også sparklinjer.
- Fylt flate bare når aksen starter på områdets bunn.
- Brudd i linjen der det mangler data i mer enn 5 s (frakoblet, omstart), i grafer og sparklinjer.
- Grafområdet for effekt og klokker utvides hvis kortet oppgir høyere maks.
- Oversikt: kortene i detaljraden fyller høyden, så Klokker og Effekt ikke har tomrom under seg.

**Robusthet**

- Server: `state.json` med `null` krasjet serveren ved oppstart. Ødelagt fil kopieres nå til `state.json.ødelagt-<tid>` før den overskrives, og første fane får et varsel. Alle blokker leses felt for felt (filer fra v0.0.1–v0.0.3). For store forespørsler gir 413 med melding i stedet for brutt forbindelse. Like profilnavn får «(2)».
- Klient: lokal historikk beholdes når serveren starter på nytt. Uten `hello` på 10 s kobles det til på nytt. `/api/state` prøves igjen med nedtelling hvis serveren ikke svarer ved oppstart. Nettverksfeil vises på norsk. Faner holder innstillingene i takt (`BroadcastChannel`), og bare svaret på siste lagring tas i bruk.
- Smale vinduer ned til 360 px og zoom 200 % uten vannrett rulling: Sensorer viser Nå/Maks, Kontroller stables, Innstillinger-rader brytes, toppfeltet viser bare det viktigste, brede tabeller ruller inni kortet og kortoverskrifter brytes.
- Data: «Video (enc / dek)» og Effekt håndterer N/A, og klokkebegrensning vises kort med alvorligste årsak først og resten i verktøytipset.
- °F: temperaturområdet under Lys og hysteresen under Viftekurve ble klemt med grenser i °C. Tallfelt klemmer nå mot feltets gjeldende grenser.
- Popovere lukkes når fokus forlater dem, og menyer hopper over deaktiverte valg. Bruk kan ikke sendes to ganger. Rader for nettverkskort som er borte, slettes.
- Testfixtur `scripts/fixtures/fake-smi/nvidia-smi` (bare for testservere via `PATH`) og `npm test` for lagringen.

### Bevegelse og detaljer

Siste runde i bygget: animasjoner der de hjelper, og finpuss av små ting. Varigheter, easing og regler per nivå står i `docs/bevegelse.md`. Bare `transform` og `opasitet` animeres (pluss `grid-template-rows` ved sammenlegging), og alt følger `data-motion`. «Redusert» beholder bare korte tonninger, og «Av» har ingen animasjoner.

**Bevegelse**

- Nye tokens `--dur-in`, `--dur-out`, `--dur-vt` og `--move`, som ganges med all forflytning. Dermed bruker «redusert» de samme reglene, bare uten bevegelse. `animate()`, `dur()` og `EASE` i `util.js` gjør det samme fra JS.
- **Sidebytte:** innholdet toner inn og løftes 4 px over 200 ms. Det skjer uten forsinkelse og uten layout, og ikke ved første side etter oppstart.
- **Sammenlegging** på Sensorer: høyden glir (0fr ↔ 1fr) mens innholdet toner. Chevronen roterer, og «Per tråd»-radene toner inn.
- **Popover og meny** skaleres fra knappen de hører til (opphavspunkt regnes ut ved plassering), 140 ms inn og 90 ms ut. **Dialoger** toner og skaleres inn, og bakteppet toner, også når de lukkes.
- **Verktøytips:** 500 ms ventetid første gang og 2 px glid fra ankeret. Naboer i en varm gruppe får tipset straks og uten overgang.
- **Varsler** kommer inn nedenfra og går ut raskere, og stabelen glir på plass (FLIP) når et varsel kommer eller går. Rettet: grensen på fire varsler kunne gå i evig løkke når lukkingen var animert.
- **Segmentkontroller:** markøren glir til nytt valg (FLIP med bare `transform`), og valgt knapp har fast bredde, så fet tekst ikke dytter naboene. **Brytere:** knotten og sporet glir og toner.
- **Hover, trykk og valgt:** samme korte fargeovergang overalt. Trykk slår inn med en gang, og valgte rader reagerer også på pekeren.
- **Temabytte** (lyst/mørkt, fargetema, aksent, kontrast, også når systemet bytter) kryssfader hele vinduet med View Transitions på 200 ms. Elementenes egne overganger er av imens, så ingenting toner to ganger. Dette skjer aldri ved første lasting, og serveren skriver nå utseendet inn i `<html>`, så første tegning har riktig tema også uten lokal lagring.
- **Verdier:** stolpene skyves med `translateX` (den runde enden beholder formen), og markøren på temperaturskalaen flyttes med `transform`, ikke `left`. Nivåfarger toner. Tall tweenes ikke.
- **Grafer:** y-aksen glir til nytt område ved «Tilpass» i stedet for å hoppe, og merkene følger med. Rullingen tegnes bare når tidsaksen har flyttet seg minst ⅓ fysisk piksel, så 240 Hz ikke gir fire ganger så mye arbeid.
- **Viftekurve:** punktene vokser ved hover og grep, og det vises hjelpelinjer til aksene mens du drar (tettere prikker når Shift smekker). Forhåndsvalg og Forkast morfer kurven over 250 ms. Canvaset tegnes bare ved endring.
- **Lys:** viftehjulene er egne lag som roteres av kompositoren (farten endres med `playbackRate`). Lysdiodene ligger i et eget lag, og bare endrede farger skrives. Pust endrer bare lagets opasitet og starter på full styrke. Bytte av effekt eller farge kryssfades over 250 ms, og valgt sone fremheves ved at de andre toner ned. Bakgrunnslyset er ett element der bare opasiteten skrives per bilde. Effektprøven i lista viser effekten ved hover, og ingenting kjører mens siden ikke vises.
- **Frakoblet:** linja glir inn og ut, og enhetskortet viser «Frakoblet» med rød prikk i stedet for «Live». Skjermlesere får én melding ved brudd og én ved retur, i stedet for en nedtelling som ble lest opp hvert sekund.
- **Kopier og Bruk:** en hake på knappen i 1,2 s, uten at knappen endrer bredde. Vellykket kopiering gir ikke lenger varsel (antall linjer står i verktøytipset og leses opp). Feil gir fortsatt varsel.
- Raden som lyser opp ved «Vis på Sensorer» og den nylagrede profilen toner ut i stedet for å bli stående farget.
- **Én rAF-løkke som sover:** når ingenting beveger seg (statisk lys, Sensorer, Viftekurve, Kontroller, System), ber løkka ikke om bilder i det hele tatt. Den vekkes av nye data, input, hendelser og ny størrelse. Grafene bestiller selv neste vekking når rullingen har kommet ⅓ piksel.

**Mikrodetaljer**

- Deaktiverte knapper får igjen pekerhendelser, så verktøytipset som sier hvorfor, faktisk vises (før: `pointer-events: none`). Hover gjelder bare aktive knapper.
- Tab-rekkefølgen følger skjermen: sidefelt, sidens handlinger og så innhold. Tab inn i sidefeltet lander på siden du er på. Fokusringen synes på alle tab-stopp, kontrollert på alle ni sider.
- Avkortet tekst (ellipsis) får hele teksten i verktøytipset automatisk, også der elementet ikke hadde tips (f.eks. gruppenotater på Sensorer).
- Plassholdere i samme høyde som innholdet på Oversikt, Grafikkort og System. «Ikke tilgjengelig» på Sensorer og statusblokken vises først når de har innhold. Layoutskift ved lasting: Grafikkort 0,088 → 0,0001, System 0,28 → 0, Sensorer 0,028 → 0.
- Tall som ikke skal danse: klokketabellen på Oversikt og tallet i graftitlene har fast bredde.
- Glidere: min/maks har faste kolonner, så sporene står likt i alle rader, og standardmerket ligger under knotten.
- `user-select: none` på alle knapper, menyer, merker, legender og glidere. Verdier og feilmeldinger kan fortsatt markeres.
- Nedtrekkslister bruker samme chevron som resten av appen. Avkrysning og radio har hover, og haken toner inn. Fargeprøver har en tynn ring ved hover.
- U+202F mellom tall og enhet også i statisk tekst («1 min», «35 %», «3 s», «5 °C / 5 %»). «–» for manglende verdi overalt (før også «—»). Riktig entall og flertall («1 kjerne», `F.plural`).
- Sidetittelen er tom til ruten er kjent, så «Oversikt» ikke blinker før en annen startside.
- Faviconet har egen flate og fungerer i begge temaer. Det følger ikke aksenten, med vilje.

**Målt** (headless Chrome 153, 1920 × 1080, egen server på port 4324, live RTX 2080)

- Skriptid per bilde i rAF-løkka, snitt / 95-persentil: Oversikt 0,45 / 2,2 ms, Grafer (seks grafer) 0,66 / 1,1 ms, Lys regnbue 0,18 / 0,3 ms og Lys pust 0,06 / 0,1 ms. Lys statisk sover (2,8 bilder/s). Per sekund er det mindre enn før: Oversikt 12,5 ms/s (før 26,6), Grafer 29 ms/s (før 45). Oversikt tegnes nå ca. 28 ganger i sekundet og Grafer ca. 44, uansett skjermfrekvens.
- Lys: fra 60 til ca. 2 layout per sekund. Hovedtråden bruker 7 ms/s i stedet for ca. 110 ms/s når effekten står stille, og ca. 70 ms/s i stedet for 174 ms/s ved pust.
- Layout under animasjon (0,1× fart, ca. 2,6 s per steg): 5–12 layout for sidebytte, meny, varsel, segment, tema og dialog (tilstandsskiftene og nye data, ikke per bilde). Sammenlegging har layout per bilde med vilje (`grid-template-rows`).
- Layoutskift (CLS) mens sidene står: 0 på alle ni sider.
- `document.getAnimations().length` under en runde med sidebytte, sammenlegging, meny, varsel, segment, lys, viftekurve, temabytte og dialog: 0 med «Av», ingen transform-animasjoner med «Redusert».
- Headless Chrome er låst til 60 Hz, og programvarekompositering gjør hovedtråden dyrere enn med GPU. Tallene er derfor øvre grenser, men 240 Hz er ikke målt direkte.

## alpha v0.0.3 · bygg 3 – 4. november 2026

- Nytt navn: **Agentic Rigg** (før Agentic GPU). Prosjektmappa heter nå `agentic-rigg/`, og navnet kommer fortsatt bare fra `version.json`.
- Sensorer: mindre død plass i tabellen. Navnekolonnen er 250 px, tallkolonnene står rett ved siden av, og sparklinja for siste 60 s fyller resten av bredden.
- Sensorer: snittet for PCIe-generasjon vises som en generasjon (f.eks. «2.0») i stedet for et rått desimaltall.
- Merket «Live (nvidia-smi)» / «Simulert GPU» er flyttet fra toppfeltet til Innstillinger, ved «Datakilde nå». Toppfeltet viser fortsatt varsel hvis forbindelsen til serveren brytes.

## alpha v0.0.2 · bygg 2 – 4. november 2026

### Utseende

- Nytt, nøkternt utseende i GNOME-ånd: grupper med overskrift og ramme, etikett til venstre og verdien i et lite lesefelt, enkle prosentstolper, tydelige tallfelt, flate knapper og systemfont (`system-ui`, Cantarell/Inter som reserve) med tabulære tall. Glød, gradienter og neon er fjernet. Én dempet blå aksentfarge.
- Lyst og mørkt tema, og System som standard. System følger `prefers-color-scheme` live. Valget ligger under Innstillinger og lagres. Alle farger er CSS-variabler, og grafene bytter farger sammen med temaet.
- Oversikt er bygd om: de runde målerne er erstattet av tallfelt og stolper for skjermkort, strøm, klokker (nå/maks), temperatur (nå, høyeste, mål, nedklokking), vifte og klokkebegrensning, pluss en enkel graf for siste minutt.
- Grafene er enkle linjer med flat fylt flate og rutenett.
- Kortere overganger: stolpene når riktig verdi på ca. 0,2 s, og første verdi settes direkte. Tidligere brukte målerne flere sekunder ved oppstart.
- Toppfeltet er et enkelt GNOME-aktig felt med sidetittel og skjermkortnavn. Sidefeltet er delt i Overvåking, Maskinvare og Kontroller (simulert), og viser versjon og bygg nederst.
- Smalere vinduer får én kolonne. Under 860 px viser sidefeltet bare ikoner.

### Nye sider

- **Grafikkort** (i stil med GPU-Z): navn, merke og arkitektur, GPU-brikke, PCI-ID for enhet og subsystem (med forklaring på at subsystemet melder NVIDIAs ID), bus-ID, VBIOS, driver, CUDA, compute capability, PCIe nå/maks («x16 3.0 @ x16 1.1») med forklaring på strømsparing, videominne, BAR1, Resizable BAR, UUID (maskert, med Vis-knapp), klokketabell, effekt- og temperaturgrenser. Ved siden av en innebygd spesifikasjonstabell merket «Referanse – ikke målt», med beregnet fyllrate og FP32 og formelen i verktøytipset. «Kopier som tekst».
- **System** (i stil med CPU-Z): prosessor, cache-tabell, instruksjonssett som brikker med «Vis alle», klokke og last per tråd (live), hovedkort/BIOS, minne (moduler, hastighet og timinger: «Krever bakgrunnstjeneste (root)») og operativsystem. «Kopier som tekst».
- **Sensorer** (i stil med HWiNFO): én tabell med sammenleggbare grupper for GPU, CPU, Minne, Nett og Andre hwmon. Kolonnene Nå, Min, Maks, Snitt og en sparklinje for siste 60 s. Nullstill-knapp. Sensorer som mangler forklares nederst i stedet for å vises som feil.
- Overvåking heter nå **Grafer**. Snarveiene er 1–9 i rekkefølgen i sidefeltet.

### Server

- `server/system.js`: leser CPU, cache, DMI, minne, OS, nett, hwmon og PCI fra `/proc` og `/sys`. Starter ingen programmer. Faste data leses én gang. Levende data leses hvert sekund, bare mens en nettleser lytter, og sendes som `sys`-hendelser på den eksisterende SSE-strømmen.
- `server/specs.js`: spesifikasjonstabell slått opp på PCI-ID (TU104 / RTX 2080).
- `server/nvidia.js`: feltene er nå navngitt i stedet for å telles etter plass. Flere lesende felt i `--query-gpu`. Én fast `nvidia-smi -q --id=0` ved oppstart, og `-q` er lagt til i listen over tillatte flagg. Fortsatt ingen flagg som setter noe.
- Nye API-kall: `GET /api/gpuinfo`, `GET /api/system`, `GET /api/sensors`. `POST /api/settings` tar også `theme`.
- Appnavnet kommer fra `version.json` (`"name"`) og settes inn i HTML-en av serveren. Et navnebytte er én linje.

### Rettet

- Temperaturmålet under Kontroller følger nå °C/°F (felt, skala, beskrivelse og «Gjeldende»).
- Profillisten viser om profilen har egen viftekurve (med antall punkter) eller standardkurven. Kurvepunktene står i verktøytipset.

## alpha v0.0.1 · bygg 1 – 4. november 2026

Første klikkbare demo.

- Lokal Node-server uten avhengigheter, bare på `127.0.0.1`, med `Host`- og `Origin`-sjekk.
- Live overvåking med én fast `nvidia-smi --query-gpu … -lms 500`-prosess som bare kjører mens en nettleser lytter. Automatisk fallback til simulert kort.
- Sider: Oversikt (runde målere, VRAM og minigraf), Kontroller, Viftekurve, Overvåking, Lys (RGB) og Innstillinger.
- Simulerte kontroller: effektgrense, kjerne- og minneklokke-offset, temperaturmål, vifte Auto/Manuell, redigerbar viftekurve og fem profiler. Alt lagres i `state.json`, og serveren svarer `simulated: true`.
- RGB-forhåndsvisning med Statisk, Pust, Regnbue, Fargesyklus og Av.
- Mørkt tema med neon-aksent.
