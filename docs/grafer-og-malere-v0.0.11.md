# Grafer og målere v0.0.11 · bygg 11

Brukeren ville at appen skulle bli mer avansert, men fortsatt profesjonell og gjennomtenkt. Ønskene i denne runden var:

1. grafer med valgfri lengde, helt opp til 48 timer
2. tydeligere målere, og farger som betyr én ting
3. pause på en graf
4. markøren på alle grafene, eller bare på grafen du holder over
5. faste farger per serie
6. seks farger å velge mellom for grensesnittet

Designprinsippene fra `docs/design-pro-v0.0.9.md` gjelder fortsatt. Ingenting skrives til skjermkortet eller maskinen; historikken lagres bare i datamappa.

Skjermbilder før og etter ligger i `docs/skjermbilder/v0.0.11/`. Etter-bildene er tatt med en testserver som er fylt med syntetiske 48 timer (`AG_TEST_HISTORY=1`, se under). De viser derfor en jevn døgnrytme med topper omtrent hver tredje time og et hull på 40 minutter for sju timer siden, «da serveren var av». Ekte data ser annerledes ut.

## 1. Historikk på serveren (`server/history.js`)

**Lag.** Hver prøve legges i en bøtte i tre lag samtidig. Per serie lagres min, snitt og maks for hver bøtte. Derfor forsvinner ikke toppene når dataene blir grovere: maks i en 60 s-bøtte er den høyeste prøven i hele minuttet.

| Lag | Bøtte | Tar vare på | Høyst rader |
|---|---|---|---|
| 0 | 1 s | 15 min | 901 |
| 1 | 10 s | 6 t | 2 161 |
| 2 | 60 s | 48 t | 2 881 |

**Grenser.**
- Høyst 96 serier. Seriene over grensen telles, men lagres ikke.
- Radene lagres som `Float32Array`, så minnet er høyst ca. 6,8 MB.
- Fila er høyst 16 MiB. Det verste tilfellet er alle 96 serier og alle lag fulle, og testen måler det til 14,6 MiB. Med de ca. 50 seriene på denne maskinen er fila omtrent halvparten. Blir fila likevel for stor, kuttes de eldste radene før den skrives.

**Lagring.**
- `history.json` i `AG_DATA_DIR` skrives hvert 5. minutt og ved avslutning (SIGINT, SIGTERM, SIGHUP).
- Skrivingen er atomisk, med temp-fil og rename, slik `store.js` gjør det.
- Ved oppstart leses fila inn igjen. Rader som er for gamle for laget sitt, eller som ligger i framtida, hoppes over.
- En ødelagt fil (ugyldig JSON, ukjent format, ugyldige serier eller verdier, eller for stor) flyttes til `history.json.bad`. Serveren logger det, og historikken begynner på nytt.

**Seriene.** Navnene er de samme i nettleseren og på serveren:
- `gpu.temp`, `gpu.power` og de andre GPU-feltene. I simulert modus lagres de som `gsim.*`, så simulerte tall aldri blandes med ekte. API-et viser alltid kilden som er i bruk.
- `cpu.load`, `cpu.temp` (samme sensor som nettleseren viser), `cpu.t0` …
- `mem.usedPct`, `mem.used` …
- `net.rx` og `net.tx` (summen over fysiske kort, samme regel som i nettleseren), `net.<kort>.rx/tx`
- `hw.<id>` for alle hwmon-sensorer

**Når det samles inn.** Serveren samler bare når kilden kjører, det vil si mens minst én fane er åpen (pluss 10 s). Det er den samme regelen som før, så `nvidia-smi` kjører ikke i bakgrunnen uten at noen ser på. Der ingen fane var åpen, eller serveren var av, finnes det ingen bøtter, og grafene viser et hull. Se «Latt være».

**API.** `GET /api/history?window=3600&keys=gpu.temp,gpu.power&points=800`
- `window` må være en av 60, 300, 900, 3600, 21600, 86400 og 172800.
- `keys` er 1–16 navn (`[A-Za-z0-9_.:/-]`, høyst 80 tegn), uten doble.
- `points` er 10–4000 (standard 1000).
- Alt annet gir 400 med norsk melding, også ukjente parametre.
- Svaret har én kolonne for tid og én for hver av min, snitt og maks per serie, med `null` der serien manglet. Det har aldri flere punkter enn `points`.
- Ruten leser bare. `nvidia-smi`, `/proc` og `/sys` leses som før.

**Syntetisk historikk.** `AG_TEST_HISTORY=1` fyller historikken med 48 timer ved oppstart. Den brukes bare av testene og skjermbildene.

## 2. Grafene (`web/js/charts.js`, `history.js`, `chartpanel.js`)

**Tidsvindu.** Brukeren kan velge 1 min, 5 min, 15 min, 1 t, 6 t, 24 t eller 48 t.
- På Grafer er valget en segmentvelger i toppfeltet. Valget lagres som `chartWindow`; 2 min fra eldre filer blir 5 min.
- På Oversikt og Kontroller er det en liten nedtrekksliste i toppfeltet.
- Under en rad på Sensorer står nedtrekkslista i grafens overskrift.
- Hver side husker sitt eget valg i `windows: { oversikt, kontroller, sensorer }`. Jeg valgte per side fordi Oversikt er et blikk på nå, mens Grafer er stedet for lange vinduer. Et felles valg ville gjort Oversikt til en 48-timersgraf etter ett klikk på Grafer.

**Historikk og levende data.**
- `HistoryView` henter vinduet fra serveren og fletter inn prøvene fra SSE-strømmen, så grafen oppdateres live som før.
- Korte vinduer får prøvene som de er.
- Lange vinduer samler dem i bøtter med samme bredde som serverens svar, så antallet punkter holder seg rundt det grafen har plass til.
- Etter en ny tilkobling hentes vinduet på nytt, så hull fylles.

**Tegning.**
- **Tidsaksen:** «−45 s» og «−4 min» opptil 5 min, klokkeslett (14:30) på hele tidspunkter for lengre vinduer, og dato («5. nov.») der døgnet skifter. Til høyre står «nå», eller «pauset 14:32».
- **Bånd:** Lange vinduer får et svakt bånd fra min til maks rundt snittlinja. Da tegnes ikke den fylte flaten, så båndet synes.
- **Punkter:** Det tegnes aldri flere punkter enn plottet er bredt i piksler. Er det flere, slås de sammen per pikselkolonne: snitt for linja, laveste min og høyeste maks for båndet.
- **Hull:** Der det mangler data, brytes linja, og flaten er skravert («Ingen data»).
- **Hvor langt tilbake:** Før første punkt står «Data fra i dag 16:43», eller «Samler data …» i korte vinduer.
- **Hold musa over:** tidspunkt (for eksempel «i dag 17:50:00 · snitt over 40 s») og verdien per serie, med min og maks i parentes når de skiller seg fra snittet.

**Pause per graf.**
- Pauseknappen står i overskriften, og mellomrom virker når grafen har fokus (grafen nås med Tab).
- Mens grafen står stille, står det «På pause · 14:32:10» i overskriften, og knappen heter «Fortsett».
- Dataene samles videre i bakgrunnen. Fortsett hopper til nå.
- På Grafer pauser knappen i toppfeltet (og mellomrom utenfor en graf) alle grafene.

**Markøren.** Den kan vises på alle grafene på siden, eller bare på grafen du holder over. Valget er en bryter under Innstillinger › Grafer og sensorer og en knapp (sikte) i toppfeltet på Grafer. Det huskes som `chartSync`.

**Forklaringen.** Hver serie er en knapp med seriens farge. Klikk skjuler eller viser serien; den siste synlige kan ikke skjules.

**Ellers.**
- Linjene er 1,75 px (stiplede 1,25 px), og tykkere i høy kontrast.
- Rutenettet og tallene på aksene er som før.
- Grafen under en rad på Sensorer har nå historikk som de andre (og eget tidsvindu), og y-aksen følger dataene i vinduet, ikke bare økta.

## 3. Seriepaletten

Seriene har faste farger som designtokens (`--pal-*` og `--c-*` i `styles.css`, seksjon 2), uavhengige av aksentfargen. Det er ingen gul, oransje eller rød: de betyr advarsel og fare.

| Farge | Token | Serier |
|---|---|---|
| Blå | `--c-load` | GPU-last, CPU-last, tråder, nett ned |
| Magenta | `--c-memctl` | Minnekontroller-last, video enc/dek, nett opp |
| Lilla | `--c-temp` | Alle temperaturer |
| Grønn | `--c-power` | Effekt |
| Blågrønn | `--c-clock` | Klokker og vifte |
| Cyan | `--c-mem` | VRAM, minne (RAM), minneklokke |
| Grå, stiplet | `--c-limit` | Grenser og referanser (effektgrense) |

- **Samme farge overalt:** en serie har samme farge på Oversikt, Grafer, Kontroller, i sparklinjene på Sensorer og i statusblokken.
- **Når farger gjentas:** bare for serier som aldri står i samme graf, og da for samme slags størrelse der det går (klokker og vifte er begge «fart»).
- **Par i samme graf:** blå og magenta (GPU og minnekontroller, nett ned og opp), og grønn og grå stiplet (effekt og grense). Blå og magenta skiller seg både i fargetone og lyshet, også med protanopi, deuteranopi og tritanopi. Grensen skilles i tillegg med strek og stipling, ikke bare farge.
- **Fire sett:** lyst, mørkt, høy kontrast lyst og høy kontrast mørkt. `npm run contrast` sjekker nå hele paletten mot kortet og den andre kortflaten i alle 96 kombinasjoner (minst 3:1).
- **Kontroller og illustrasjoner:** viftekurven (en kontroll) beholder aksentfargen. Spennings- og klokkekurven, som er en illustrasjon, har klokkefargen.

## 4. Seks farger for grensesnittet

- Aksentfargene er seks: blå, blågrønn, grønn, lilla, rosa og skifer. De vises som tydelige fargeprøver med navn under, i én rad, under Innstillinger › Utseende.
- Fargetemaene (Grafitt, Sand, Fjord og Midnatt) er uendret.
- **Hvorfor gul, oransje og rød er fjernet:** de samme fargene betyr advarsel og fare i appen. En rød aksent ville gjort valgt side og Bruk-knappen like røde som «Frakoblet», og en gul aksent like gul som en temperatur over grensen.
- Lagrede valg med disse fargene blir blå (også ved import), og ingenting annet endres.
- Grafene skifter ikke farge når aksenten endres; røyktesten sjekker det.

## 5. Målerne for temperatur og effekt (`web/js/gauge.js`)

Den gamle skalaen hadde en prikk for verdien nå og soner i gult og rødt fra målet og ut. Det var ikke tydelig hva sonene betydde, og prikken var vanskelig å lese. Den nye måleren gjør dette:

- **Fyll:** fyllet går fra skalaens start til verdien nå (som andre stolper i appen), så det er lett å se hvor verdien er.
- **Fargen på fyllet:** aksent normalt. Gul og rød bare over brukerens varselgrenser for temperatur (Innstillinger › Sidefelt), og gul når effekten er ved grensen. Det samme står i ord ved verdien («· over gul grense (75 °C)») og i verktøytipset.
- **Merker:** tynne streker for grensene, med tallet rett under (83 °C, 97 °C). Skalaen slutter ved avstenging (temperatur) eller kortets maks (effekt).
- **Forklaringen** under navngir alle tallene: «Mål 83 °C · Nedklokking 97 °C · Avstenging 100 °C» og «Grense (standard) 245 W · Kortets maks 275 W». Måleren kan dermed leses uten farger.
- **Ingen krasj:** tallene skal aldri overlappe. Står to for tett (høye verdier, smale vinduer), viker skalaens sluttall først, deretter merket lengst til høyre. Forklaringen har alltid alle. Røyktesten sjekker dette ved 1440 og 360 px.
- **Temperatur og effekt** bruker den samme måleren og ser like ut.

## 6. Hva fargene betyr

| Farge | Betyr | Hvor | I tekst |
|---|---|---|---|
| Gul (`--warn`) | Advarsel: over gul grense eller ved en grense | Verdier over gul varselgrense (fliser, statusblokk, Nå på Sensorer, måleren), effekt ved grensen, stolper over 85 % (VRAM, minne), bakgrunnstjenesten ikke tilkoblet, nødregelen aktiv, alvorlig klokkebegrensning | Verktøytips «over gul varselgrense …», teksten ved måleren, linja og merket sier hva det gjelder |
| Rød (`--danger`) | Fare eller feil | Verdier over rød grense, frakoblet fra serveren, feilmeldinger, ugyldige felt, Tilbakestill (kan ikke angres) | Verktøytips «over rød varselgrense …», «Frakoblet», meldingens tekst |
| Grønn (`--ok`) | I orden | Live-prikken, «Tilkoblet» | Tekst ved siden av |
| Grå prikk | Ikke ekte data | Simulert GPU | «Simulert» |
| Aksent | Valgt, endret, venter | Valgt side, Bruk, endringsprikker, bekreftelsen | Teksten i toppfeltet og linja |

## Funn → hva som er gjort

### Fargene (gjennomgang av alle steder med oransje og rødt)

1. **Temperaturskalaen på Oversikt hadde gule og røde soner fra målet og ut**, også når temperaturen var 33 °C. Fargen var pynt som så ut som en advarsel → Ny måler uten soner og prikk (punkt 5). Gult og rødt brukes bare når verdien faktisk er over en grense.
2. **«Temperatur nå» på viftekurven var en gul strek** (forklaringen sa «Temperatur nå» med gul strek). Gult betyr advarsel → Linja og punktet har temperaturens seriefarge (lilla). Nødregelsonen er fortsatt nøytral grå til den gjelder, og gul bare når nødregelen er aktiv («Nødregel aktiv»).
3. **Minnekontroller og nett opp var oransje** (andre serie fulgte aksenten og var oransje i lyst tema) → Magenta fra seriepaletten. Ingen serier er gule, oransje eller røde.
4. **Endringsprikker var gule** (Kontroller, Viftekurve, toppfeltet «2 endringer»), som om en ulagret endring var en advarsel → Aksentfarge. Teksten ved siden av sier hva det er.
5. **Simulert GPU var et gult merke med gul prikk** i toppfeltet og på kortvelgeren. Simulert er informasjon, ikke en advarsel → Nøytralt merke og grå prikk. Gul prikk betyr nå bare advarsel.
6. **Aksentfargene gul, oransje og rød** kolliderte med advarsel og fare → Fjernet, og lagrede valg blir blå (punkt 4).
7. **Gule og røde verdier sa ikke hva fargen betydde** (fliser, statusblokk, Nå på Sensorer, stolper) → Verktøytipset sier «over gul varselgrense» eller «over rød varselgrense (Innstillinger › Sidefelt)», og stolpene «Gult: over varselgrensen». VRAM-stolpen sier grensene (85 % og 95 %).
8. **Gjennomgått og beholdt:**
   - linja når bakgrunnstjenesten ikke er tilkoblet (gul: advarsel)
   - frakoblet fra serveren (rød: feil)
   - Tilbakestill (rød: kan ikke angres)
   - ugyldige felt og feilmeldinger (rød)
   - alvorlig klokkebegrensning (gult merke med forklaring i verktøytipset)
   - fargene i Lys-forhåndsvisningen, som er brukerens egne lysfarger

### Finpussrunden

9. **Grafen under en rad på Sensorer var tom:** lerretet var 0 px bredt (`flex: none` i en rad) → Tar hele bredden.
10. **Y-aksen i den samme grafen kuttet toppene i lange vinduer**, fordi den fulgte økta (33 °C) og ikke vinduet (75 °C) → Følger dataene i vinduet.
11. **Teksten ved temperaturen sa «… under nedklokkingnull»** når den ikke var over en grense → Rettet; den tomme delen tas ikke med.
12. **Tallene under måleren kunne gi vannrett rulling** et øyeblikk mens vinduet endret størrelse (de plasseres i px og flyttes ved neste måling) → Raden klipper, og røyktesten for 360 px fanger det.
13. **24 t viste bare to klokkeslett på aksen** i de smale grafene på Oversikt → Ett merke per ca. 64 px i stedet for 78 px, så 00:00, 06:00, 12:00 og 18:00 får plass.
14. **API-et kunne gi ett punkt mer enn bedt om** (bøttene er justert til hele steg) → Det eldste ekstra punktet tas bort. Enhetstesten fanget det.
15. **En ødelagt fil med en gammel rad ble ikke oppdaget**, fordi rader som var for gamle ble hoppet over før de ble sjekket → Alle rader valideres før alderen sjekkes.

### Sett og funnet i orden

- Grafer ved 1 min, 6 t og 48 t i lyst og mørkt, også med markør og verktøytips, pause og høy kontrast.
- Oversikt ved 1 min og 24 t, Kontroller ved 6 t og grafen på Sensorer ved 48 t.
- Innstillinger › Utseende og Grafer og sensorer, Viftekurve, og 360 px for Oversikt og Grafer.
- Tastatur:
  - Tab når forklaringen, pauseknappen og grafen.
  - Mellomrom pauser når grafen har fokus.
  - Esc lukker grafen på Sensorer når fokus er i grafen eller på knappene. Nedtrekkslista bruker Esc selv.
- `npm run contrast` med seriepaletten: alle seriefarger har minst 3:1 mot kortet. Dårligste tekst er fortsatt 4,51:1 og dårligste UI-grense 3,01:1.

## Latt være

- **Opptak når ingen fane er åpen.** Historikken samles mens kilden kjører, og kilden stopper 10 s etter at siste fane er lukket. Slik er det fordi `nvidia-smi` ikke skal kjøre i bakgrunnen uten at noen ser på. Med appen lukket blir det derfor hull. Å ta opp i bakgrunnen er én linje i `telemetry.js`, men det betyr at `nvidia-smi` alltid kjører. Det bør brukeren bestemme. Når appen blir en skrivebordsapp, er den uansett åpen mens den kjører.
- **Eksport av historikken** (CSV) og **zoom med mus** i grafene. Det er nyttig, men det er nye funksjoner til en egen runde.
- **Egne farger per serie** valgt av brukeren. Faste farger er poenget i denne runden.
- **Fylt flate i lange vinduer.** Båndet tar plassen; begge sammen ble uryddig.
