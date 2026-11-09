# Designplan v0.0.9 · bygg 9: tettere og mer profesjonelt

Brukeren vil at appen skal se mer profesjonell ut, i retning av sammendragsvinduet og sensorvinduet i HWiNFO (`docs/inspirasjon/hwinfo-oppsummering.webp`). Det er retningen som gjelder, ikke utseendet: ingen navn, logoer, ikoner eller farger hentes derfra. Ikke «gamery», ingen gradienter og ingen pynt som ikke formidler noe. Grafene skal være der. Alle funksjoner, temaer, høy kontrast, animasjonsnivåer, fokusringer og tastaturbruk skal virke som før.

Skjermbilder før og etter ligger i `docs/skjermbilder/v0.0.9/` (`for/` og `etter/`).

## Hva vi ser i v0.0.8

- Kortene svever: radius 10 px, skygge og 12 px luft mellom dem. Hver gruppe ser ut som et eget kort på en flate, ikke som en del av et verktøy.
- Radene er 34 px høye. Grafikkort viser 17 rader på en skjerm der HWiNFO viser over 30.
- I etikett–verdi-listene står etiketten helt til venstre og verdien helt til høyre. I et kort som er 550 px bredt, må øyet hoppe over 300 px tomrom for hver rad.
- Overskriftene i kortene er like store som innholdet og har ingen flate eller linje under seg, så det er uklart hvor en gruppe begynner.
- Sensorer er fem separate kort, hvert med sin egen tabelloverskrift (Sensor, Nå, Min …) gjentatt fem ganger. Ved 360 px står bare «Temper…» igjen av navnet.
- Merkene er piller (radius 10 px), og trådene på System er 16 små kort.

## Prinsipper

**Paneler, ikke kort.** En gruppe er et panel: 1 px ramme, radius 6 px (`--r-lg`), ingen skygge. Overskriften står i en egen, lav stripe (`--panel-head`, en tone fra kortflaten) med 1 px linje under. Panelene står 8 px fra hverandre. Popovere, menyer og dialoger beholder skyggen, fordi de faktisk ligger over noe.

**Avstander.** Fortsatt 4-punkts skala. Standard tetthet:

| Token | v0.0.8 | v0.0.9 | Kompakt | Luftig |
|---|---|---|---|---|
| `--gap` (mellom paneler) | 12 | 8 | 6 | 12 |
| `--pad` (inne i paneler) | 16 | 12 | 8 | 16 |
| `--content-pad` | 16 | 12 | 8 | 16 |
| `--h-row` (rad i lister og tabeller) | 34 | 26 | 22 | 32 |
| `--h-panel` (panelstripe, ny) | – | 30 | 26 | 34 |
| `--h-md` (knapper, felt) | 32 | 28 | 26 | 32 |
| `--h-sm` (små knapper) | 24 | 22 | 20 | 26 |
| `--head-h` (toppfeltet) | 47 | 41 | – | – |

Tastatur- og pekermål er fortsatt minst 22 px (WCAG 2.5.8 krever 24 px eller avstand; de små knappene har luft rundt seg).

**Radius.** `--r-sm` 3 px (merker, felt i tabeller), `--r` 4 px (knapper, felt, segmenter), `--r-lg` 6 px (paneler). Merkene er rektangler med 3 px radius i stedet for piller.

**Rammer.** Alltid 1 px. `--border` rundt paneler og under panelstripen, `--border-soft` mellom rader. Ingen dobbeltlinjer: et panels siste rad har ingen linje.

**Skrift.** Samme familie (skrivebordets). Brødtekst 13 px. Panelstripe og tabelloverskrifter 12 px halvfet (`--fs-sm`). Sidetittel 14 px (var 15). Store tall på Oversikt 20 px (var 24). Ingen tekst under 11 px.

**Tall.** Alltid `tabular-nums`. I tabeller er tallkolonnene høyrejustert, så sifrene står over hverandre. Enheten står etter et smalt hardt mellomrom (U+202F) og er dempet (`.unit`). I etikett–verdi-rutenett står verdien rett ved etiketten (venstrejustert ved en fast etikettbredde), så avstanden mellom etikett og verdi er den samme på alle bredder. Tall som skal sammenlignes loddrett, står i tabeller, ikke i rutenett.

**Grafer.** Beholdes overalt: Grafer-siden, sparklinjene i statusblokken og på Oversikt, Oversikt- og Kontroller-grafene, og nå også ved hver sensor (60 s-sparklinje i raden, og en større graf når du åpner raden). Samme farger og tegnemåte som før; bare luften rundt er mindre.

**Farger.** Ingen nye farger utenom `--panel-head` (én per tema og lys/mørk). Den legges til flatene `npm run contrast` sjekker, så all tekst på stripen også må ha 4,5:1.

## Side for side

**Skall.** Toppfeltet 41 px. Sidefeltets menypunkter 28 px, statusblokken med 22 px rader. GPU-kortet øverst får 4 px radius.

**Oversikt.** Seks nøkkeltall i ett panel med felles kanter (som før, men tettere: 20 px tall, sparklinje under). Skjermkort, «Temperatur og effekt» (ett panel med to deler) og Klokker som paneler med stripe. Grafene står som før, med lavere stripe og mindre luft.

**Sensorer** (tillegg fra brukeren: «sensorvinduet i HWiNFO er en tabell»)
- Én tett tabell i ett panel, med kolonnene Sensor, Nå, Min, Maks, Snitt og 60 s. Overskriftsraden står bare én gang og blir stående øverst når du ruller (`position: sticky`).
- Gruppert etter enhet: Skjermkort, Prosessor, Minne, Andre sensorer og Nett, i den rekkefølgen. Hver gruppe har en sammenleggbar overskriftsrad (knapp med chevron, navn, kilde og antall, og P-tilstand/begrensning for skjermkortet), med samme stripefarge som panelene. «Per tråd» er en undergruppe som før.
- Radene er 24 px (standard), tallene høyrejustert, Nå halvfet, Min og Maks dempet. Varselfargene (gul/rød) står som før.
- Grafene: 60 s-sparklinje i hver rad. Klikk på navnet (eller Enter) åpner en større graf rett under raden: siste 60 s med akser og hårkors, kilde og Fest-knapp. Én åpen om gangen; Esc eller nytt klikk lukker.
- Feste, nullstilling av min/maks, at min/maks huskes, filteret (Ctrl+F), «Fold sammen alle», høyreklikkmenyen og «Vis på Sensorer» virker som før.
- Tastatur: gruppeknappen og én Tab-stopp i radene per gruppe. Pil opp/ned flytter mellom radene, pil venstre/høyre mellom nålen og navnet, Home/End til første og siste, Enter åpner grafen og Esc lukker den.
- Ved 360 px: Sensor, Nå og Maks. Navnet får plass før tallene krymper (ikke «Temper…»).
- «Ikke tilgjengelig» står under, som et panel med tette rader.

**Grafer.** Paneler med stripe (navn, forklaring, min/maks og nå). 8 px mellom grafene.

**Grafikkort og System.** Etikett–verdi-rutenett med 26 px rader og én etikettkolonne per panel (så bred som den lengste etiketten, høyst 45 %). Verdiene står ved etiketten. Underoverskriftene i Spesifikasjon står i en lav stripe, som panelene. Trådene på System er et rutenett av felt som deler kant (ingen 16 små kort). Instruksjonssett som tette merker med 3 px radius.

**Kontroller.** Justeringene med 8 px luft per rad, knapper og felt 28 px. Endringer, Profiler og kurven som paneler. Grafene under som før.

**Viftekurve.** Kurven i et panel. Punktene som en tett tabell med 22 px felt. Oppførsel med 26 px rader.

**Lys.** Forhåndsvisningen i et panel med 4 px radius på scenen. Effekter og Farge som paneler.

**Innstillinger.** Bokslistene får 6 px radius og 40 px rader (var 52). Ingen skygge.

## Funn → hva som er gjort

Etter-bildene og finpussrunden er tatt i headless Chrome mot egne servere på tilfeldige porter, i lyst og mørkt tema ved 1440 × 900, 1100 × 800 og 360 px, ved 200 % zoom (720 × 450 med skala 2), i høy kontrast, med kompakt og luftig tetthet, Fjord og Sand og største skrift.

### Første runde (etter-bildene)

1. **Sensorer ved 360 px: navnene forsvant helt.** Min, Snitt og 60 s ble skjult med `display: none` på cellene, og da skjøv Maks seg inn i Min-kolonnen mens kolonnebreddene sto igjen → Kolonnene får bredde 0 og cellene `visibility: hidden`, så hver celle blir i sin kolonne. Navnet får nå ca. 110 px («Temperatur» står helt; lange navn får ellipse og hele navnet i verktøytipset).
2. **Sensorer: overskriftsraden klistret seg 12 px under toppen**, så radene skimtet over den når siden rullet → Den klistrer seg til selve kanten (`top: calc(-1 * var(--content-pad))`).
3. **Oversikt: Temperatur og Effekt var to paneler stablet**, og to panelstriper gjorde midtkolonnen 44 px høyere enn Skjermkort og Klokker, med tomrom under radene der → Ett panel, «Temperatur og effekt», med to deler skilt av en hårfin linje. Hver del har en lav linje med navn, verdi og tillegg. Midtkolonnen er litt bredere (1 : 1,25 : 1), og ledig høyde i Skjermkort og Klokker fordeles på radene.
4. **Oversikt: «34 °C · 63 °C under ned…» ble avkortet** fordi «Høyeste 5 min: 34 °C» sto på samme linje → «maks 5 min: 34 °C» (samme form som «min · maks» på Grafer), med hele forklaringen i verktøytipset. Linja brytes heller enn å avkortes i smale vinduer.
5. **Etikett–verdi: fast etikettbredde (168 px) ga «x16 Gen 1 · maks / Gen 3» på to linjer** i Oversikt › Skjermkort → Én etikettkolonne per panel med `subgrid`: så bred som den lengste etiketten i panelet, høyst 45 % (40 % i smale paneler). Verdiene i et panel starter fortsatt på samme linje.
6. **Oversikt ved 360 px: alle radene i Skjermkort var like høye som den høyeste** (`grid-auto-rows: 1fr`) → Ledig høyde fordeles med vanlig strekking av rader, uten å gjøre dem like.
7. **Kontroller › Endringer uten endringer: dobbel linje nederst** (linja under stripen og panelets ramme) → Stripen er hele panelet: ingen linje under og runde hjørner nederst.
8. **Lys: Effekt-panelet fikk tomrom nederst** da radene ble lavere (Farge er høyere) → Effektradene er 6 px høyere enn tabellrader. De er valg du klikker på, og panelene slutter nå likt.

### Finpussrunden (friske øyne)

9. **Enhetene i sensortabellen var like sterke som tallene** («34,0 °C» i halvfet) → Tall og enhet er delt, og enheten er dempet og ikke halvfet (som på Oversikt). Skrives bare når teksten endres.
10. **Tastatursnarveier: «Ctrl Shift Enter» brøt over to linjer**, og dialogen hadde 14 px radius mot 6 px ellers → Tastene brytes ikke, radene er 26 px, og dialogene har 8 px radius.
11. **Konsollfeil ved raske tema- eller fargebytter**: to View Transitions rett etter hverandre ga «AbortError: Transition was skipped» som et uhåndtert unntak (fantes også i v0.0.8) → Den hoppede overgangen fanges i `viewTransition()` (`util.js`).
12. **Sensorer: raden med åpen graf var ikke merket** → Raden har samme bakgrunn som valgt rad, og navneknappen har `aria-expanded`.
13. **Sensorer: «Ikke tilgjengelig» hadde 35 px rader under en tabell med 24 px rader** → 26 px, som resten.

### Sett og funnet i orden

- Alle ni sider i lyst og mørkt ved 1440 × 900 (også hele siden for Sensorer, Grafikkort, System, Kontroller og Innstillinger) og fem sider ved 360 px. Ingen vannrett rulling, ingen konsollfeil.
- Høy kontrast (stripene, tabellen og rammene), kompakt tetthet med Fjord, luftig tetthet med Sand og største skrift, og 200 % zoom.
- Bekreftelseslinja med klemt verdi på Kontroller, enhetskortets popover og Tastatursnarveier.
- Tastatur på Sensorer med ekte tastetrykk: Tab til gruppeknappen og inn i radene, piltaster, synlig fokusring på nål og navn, Enter åpner grafen, Esc lukker og fokus blir på raden.
- `npm run contrast` med `--panel-head` som ny flate: dårligste tekst 4,51:1, dårligste UI-grense 3,01:1 (uendret).

### Latt være

- **Viftekurve: sidekolonnen slutter fortsatt før kurvepanelet** (ca. 170 px ved 1440 × 900 med start/stopp av). Alternativet er et panel med tomrom inni, som v0.0.8 fjernet med vilje.
- **Lange sensornavn ved 360 px får ellipse** («Minnekontroller-last»). Navnet står i verktøytipset og i grafen når raden åpnes. Å bryte navnene ville gitt ulike radhøyder i en tabell som skal være jevn.
- **Disk og hovedkortsensorer** står ikke på Sensorer. Disk krever ny lesing i serveren (`/proc/diskstats`), og hovedkortet krever en kjernedriver (nct6775). Begge er funksjoner, ikke design, og hører hjemme i en egen runde.
- **Etikett–verdi stables ikke i smale paneler** (etikett over verdi). Det ville doblet høyden på korte par som «Merke GeForce».
