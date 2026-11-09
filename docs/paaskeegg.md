# Påskeegg · fasit (v0.0.12 · bygg 12)

> **Røper alt.** Denne fila er fasiten. PATCHLOG sier bare at det finnes små overraskelser.

Agentic Rigg har ti små, skjulte detaljer. De skal gjøre appen litt morsommere å bruke uten å bryte det nøkterne uttrykket. I vanlig bruk ser du ingen av dem.

Alle eggene ligger i én fil, `web/js/easter.js`. Lista `EGGS` øverst i fila har de samme ti, i samme rekkefølge som her. Skjermbilder ligger i `docs/skjermbilder/v0.0.12/`, med samme nummer som her.

## Reglene alle eggene følger

- **Ufarlige.**
  - Eggene endrer aldri et målt tall og later aldri som noe er ekte.
  - De sender ingenting til skjermkortet, bakgrunnstjenesten eller serveren.
  - De rører ikke utkastet, bekreftelsesflyten, kontrollene eller innstillingene.
  - Nordlys er det eneste som legger seg over noe. Det gjelder bare forhåndsvisningen, og det går bort av seg selv.
  - Røyktesten sjekker at innstillinger, utkast, det som er brukt, lysvalget, profilene og ventende endringer er like før og etter.
- **Stille.**
  - Ingen lyder, dialoger eller popuper som tar fokus.
  - To av eggene bruker det vanlige varselet nederst til høyre, med et lite egg i stedet for haken. Varselet forsvinner av seg selv etter 5 s.
- **Ingen advarselsfarger.**
  - Gul, oransje og rød betyr advarsel og fare (`docs/grafer-og-malere-v0.0.11.md`), så eggene bruker bare aksentfargen og dempet tekst.
  - Nordlys bruker grønt, blågrønt og fiolett.
- **Bevegelse.**
  - Bare to egg beveger seg: appikonet og Nordlys.
  - Appikonet går via `animate()` i `util.js`, så det står stille ved «Redusert» og «Av».
  - Nordlys tegnes i den samme løkka som de andre RGB-effektene. Ved «Redusert» og «Av» er det et stillbilde, slik Regnbue er.
  - Ingenting blinker. Nordlyset blir aldri svakere enn 15 %, og bølgene er langsomme (perioder på 7–21 s).
- **Tilgjengelighet.**
  - Alt som vises, har tekst for skjermlesere: et verktøytips, en `aria-label`, en skjult tekst, en `aria-live`-linje eller `announce()`.
  - Ingen egg legger til tab-stopp eller endrer fokus.
  - Juksekoden, filteret, kurven (piltastene), Nordlys og konsollen utløses med tastaturet. 42 °C, bursdagen og natta skjer av seg selv. Bare appikonet og versjonen må klikkes.
  - Fargene er tokens, så høy kontrast og alle fargetemaer virker uten noe ekstra.
- **Én bryter.**
  - «Små overraskelser» under Innstillinger › Generelt (gruppen Annet) slår av alt med en gang, også det som vises nå.
  - Valget er `settings.surprises` (på som standard) og valideres i `server/store.js`.
  - Undertittelen sier hvor mange egg du har funnet («Du har funnet 3 av 10»), men ikke hvilke.
- **Funnet.**
  - Hva du har funnet, huskes bare i nettleseren (`localStorage` `ag-eggs`), ikke i innstillingene eller på serveren.

## Eggene

### 1 · Juksekoden

- **Utløses:** ↑ ↑ ↓ ↓ ← → ← → B A hvor som helst, når fokus ikke står i et tekstfelt. Modifikatortaster teller ikke.
- **Skjer:** Et varsel sier «Juksekode godtatt» og «+30 liv. Klokkene, effekten og viften er som før – kortet får aldri vite om dette.»
- **Hvorfor:**
  - Konami-koden er det klassiske påskeegget, og et overklokkingsverktøy er nettopp stedet der man ønsker seg «gratis» ytelse.
  - Svaret sier rett ut at ingenting har skjedd, så ingen tror at klokkene er endret.
- **Kode:** `onKonami()` i `easter.js`. Den lytter i fangstfasen og stopper ikke tastene, så piltastene virker som før (sidefeltet, viftekurven, Sensorer).

### 2 · Støvtørking

- **Utløses:** Fem klikk på appikonet øverst i sidefeltet, med under 2 s mellom hvert.
- **Skjer:**
  - Ikonet, som er et skjermkort med vifte, snurrer to runder og bremser opp som en vifte som går ned i fart.
  - Varselet sier «Pff! Støvet er blåst av viften. Bare på ikonet – den ekte viften går som før.»
  - Ved «Redusert» og «Av» står ikonet stille, og bare varselet kommer.
- **Hvorfor:** Alle som har et skjermkort, har blåst støv ut av det. Klikk på et ikon er den eldste utløseren som finnes.
- **Kode:** `onIconClick()` i `easter.js`. Animasjonen er `transform: rotate` (kompositoren) i 1,4 s.

### 3 · Utviklermodus

- **Utløses:** Sju klikk på versjonsmerket i Innstillinger › Om.
- **Skjer:**
  - Fra det tredje klikket står det under versjonen «Fire klikk til, så er du utvikler.», så «Tre …» og så videre.
  - På det sjuende står det «Du er nå utvikler. Det var du nok fra før.» Feilsøkingslista får raden «Små overraskelser: 7 av 10 funnet».
  - Etterpå sier tre klikk «Du er allerede utvikler.», og raden står der hver gang Om åpnes.
  - Nedtellingen forsvinner etter 4 s uten klikk. De to første klikkene sier ingenting, så et tilfeldig klikk ikke gir tekst.
- **Hvorfor:**
  - Det er en hilsen til Android, der man blir utvikler ved å trykke sju ganger på byggnummeret.
  - Den som leter i feilsøkingsradene, er utvikler, og får se hvor mange egg som er igjen.
- **Kode:** `aboutEgg()` i `easter.js`, kalt fra `om()` i `pages/settings.js`. Linja under versjonen er `aria-live="polite"` og tar ingen plass når den er tom.

### 4 · Svaret på alt

- **Utløses:** GPU-temperaturen på Oversikt viser nøyaktig 42 °C, i hele grader. Med °F skjer det ikke, for kortet blir aldri 42 °F.
- **Skjer:**
  - Tallet får en prikket understrek i aksentfargen.
  - Linja får verktøytipset «42 °C – svaret på livet, universet og alt. Målingen er ekte; det er bare tallet som er morsomt.»
  - Skjermlesere får det samme som skjult tekst etter tallet.
  - Tallet er det målte, og det går bort så snart temperaturen er 41 eller 43.
- **Hvorfor:**
  - Det er en hilsen til *Haikerens guide til galaksen*, og 42 °C er en helt vanlig temperatur for et kort på skrivebordet.
  - Egget dukker opp av seg selv når man minst venter det, og det er like stille som resten av siden.
- **Kode:**
  - `mark42()` i `easter.js`, kalt fra `pages/overview.js`.
  - Verktøytipset ligger på linja (`#ov-temp-margin`), som står fast. Tallet lages på nytt for hver prøve, og et tips på tallet ville forsvunnet hvert halve sekund.

### 5 · Bursdag

- **Utløses:** 20. september, på Grafikkort, når kortet er et GeForce RTX 2080 (også det simulerte). Det gjelder ikke Ti- og Super-utgavene.
- **Skjer:**
  - Raden «Lansert» får et lite kakeikon og «8 år i dag» (antall år regnes ut) i aksentfargen.
  - Verktøytipset sier «Gratulerer med dagen! GeForce RTX 2080 kom i butikkene 20. september 2018.»
  - «Kopier som tekst» tar bare med «september 2018».
- **Hvorfor:** Kortet i denne maskinen har bursdag. Spesifikasjonstabellen vet bare måneden, så dagen står i `easter.js`.
- **Kode:**
  - `launchDay()` og `LAUNCH_DAYS` i `easter.js`, kalt fra `pages/gpu.js`.
  - Teksten står inne i verdien, så den brytes mellom ord i smale vinduer (sett ved 360 px).
  - Kakeikonet er Lucide `cake`, lagt til i ikonspriten i `index.html`.

### 6 · Nattevakt

- **Utløses:** Klokka er mellom 02:00 og 04:59.
- **Skjer:**
  - En liten, dempet måne står ved alpha-merket øverst i sidefeltet.
  - Verktøytipset og `aria-label` sier «Sent oppe? Skjermkortet er våkent det også.»
  - Månen går bort av seg selv klokka fem. Den vises ikke i ikonlista, der det ikke er plass.
- **Hvorfor:** Den som sitter med et overvåkingsverktøy klokka tre om natta, holder på med noe. En liten nikk er nok.
- **Kode:** `renderNight()` i `easter.js`. Klokka sjekkes én gang i minuttet (`setInterval`), uten animasjon.

### 7 · Kaffe

- **Utløses:** «kaffe» (også «kaffi», «kaffen», «coffee») eller «hei» (også «hallo», «heisann», «hei hei», «hoi») i filteret på Sensorer. Bare når filteret ikke har treff.
- **Skjer:**
  - I stedet for «Ingen sensorer passer filteret.» står det «Ingen sensor måler kaffe ennå. Det nærmeste er CPU-temperaturen.» eller «Hei! Ingen sensor heter det, men takk for at du spør.»
  - Alle andre søk uten treff gir den vanlige teksten.
- **Hvorfor:** Søkefelt er der folk skriver ting de ikke burde, og en tomtilstand som svarer, er en klassisk liten detalj.
- **Kode:** `filterQuip()` og `FILTER_QUIPS` i `easter.js`, kalt fra `applyFilter()` i `pages/sensors.js`.

### 8 · Kallenavn på kurven

- **Utløses:** Alle punktene på viftekurven står på 100 %, eller alle står på 35 % (det laveste kortet tillater). Det gjelder både når du drar og når du skriver i tabellen.
- **Skjer:**
  - Tittelen sier «Kurve · Jetmotor» eller «Kurve · Bibliotek» i stedet for «Kurve · Egendefinert», på samme måte som den sier «Kurve · Stille» for forhåndsvalget.
  - Verktøytipset på tittelen sier «Alle punktene står på 100 %. Naboene hilser.» eller «Alle punktene står på 35 %, så lavt kortet tillater. Hysj.»
  - Kurven, utkastet og Bruk er urørt.
- **Hvorfor:** Den som tegner kurven rett på taket eller rett på gulvet, vet hva de gjør. Kallenavnet bruker det samme stedet og den samme formen som forhåndsvalgene.
- **Kode:** `curveNickname()` i `easter.js`, kalt fra `renderSide()` i `pages/fancurve-page.js`.

### 9 · Hilsen i konsollen

- **Utløses:** Åpne utviklerverktøyene i nettleseren (F12).
- **Skjer:**
  - Konsollen viser «Agentic Rigg alpha v0.0.12 · bygg 12» i halvfet skrift og en kort hilsen i dempet grå:
    - at alt leser bare,
    - at `window.__app` har tilstanden,
    - at det finnes ti små overraskelser.
  - `rigg.hint()` gir et hint om et egg du ikke har funnet ennå, og hvor mange du har. Når alle er funnet, sier den hvor fasiten står.
- **Hvorfor:**
  - Mange nettsider hilser på nysgjerrige utviklere i konsollen, og denne appen er laget for folk som åpner panelet.
  - Hintene gjør det mulig å finne resten uten å lese fasiten.
- **Kode:**
  - `greet()` og `hint()` i `easter.js`.
  - Hilsenen er `console.info` (ikke en feil), og den kommer ikke når bryteren er av.
  - Headless Chrome har ingen synlig konsoll, så skjermbildet er teksten i `09-konsoll.txt`.

### 10 · Nordlys

- **Utløses:** Skriv «nordlys» på Lys, når fokus ikke står i et tekstfelt.
- **Skjer:**
  - Forhåndsvisningen viser et nordlys som glir langsomt over lyslista og viftene: grønt, blågrønt og av og til fiolett, med gardiner som blir sterkere og svakere.
  - Bakgrunnslyset følger med. Merket øverst til venstre sier «Nordlys · lagres ikke · Esc avslutter», og skjermlesere får det samme.
  - Utkastet er urørt, så «Ingen endringer»-tilstanden, Forkast og Bruk er som før.
  - Nordlyset går bort når du trykker Esc, velger en effekt, farge, sone eller bakgrunnslys, trykker Forkast eller Bruk, går til en annen side eller slår av bryteren.
  - Lysstyrken følger valget ditt, men aldri under 40 %. Med effekten Av lyser nordlyset med 85 %.
- **Hvorfor:**
  - Det er en norsk effekt i et norsk program, og nordlys er det naturlige RGB-lyset.
  - Fargene ligger utenfor advarselsfargene.
- **Kode:**
  - `aurora()` i `easter.js` gir fargen per lysdiode.
  - `RgbPreview.setEgg()` i `rgb.js` viser den over valgt effekt, med samme kryssfade som et effektbytte.
  - `setEgg()` i `pages/lights.js` styrer merket og når det slutter.
  - `rgb.js` importerer fortsatt bare `util.js`, så prototypen i `prototyper/lys-3d/` kan laste den som før.
  - Tiden rundes til 1/20 s, så nordlyset skriver høyst 20 ganger i sekundet, og bare de lysdiodene som har endret seg.

## Slik endrer eller fjerner du et egg

1. Finn egget i `EGGS` og i seksjonen med samme nummer i `easter.js`.
2. For å fjerne det:
   - Ta det bort fra `EGGS` og `HINTS`.
   - Slett funksjonen og kroken i fila som kaller den. Krokene er merket «Påskeegg» (lista øverst i `easter.js`).
   - Ta det bort fra `ALL` i `test/ui/easter.test.js`.
3. Et nytt egg må følge reglene over. Sjekk det med `enabled()` og meld det som funnet med `mark('id')`. Det som vises, merkes med `data-egg-node`, så bryteren kan ta det bort.

## Tester

Fila `test/ui/easter.test.js` har fire tester i headless Chrome, mot en egen testserver:

1. **Alle ti utløses.**
   - Det skjer ved 1440 og 360 px, uten konsollfeil og uten vannrett rulling.
   - Innstillinger, utkast, brukt, lys, profiler og ventende endring er like før og etter, både i nettleseren og på serveren.
   - 42 °C kommer fra den simulerte tjenesten (`mock.set_temperature`, bare i tester). Datoene stilles med `__easter.setNow()`.
2. **Bryteren slår av alt.** Månen og nordlyset forsvinner med en gang. Ingen av de ti kan utløses etterpå, og konsollen hilser ikke ved neste lasting.
3. **«Av» animerer ingenting.** Ingen animasjoner i noe bilde mens eggene utløses, og nordlyset står stille. Ved «Redusert» snurrer ikke ikonet heller.
4. **Ingen egg ved et uhell.**
   - Testen bruker appen vanlig: tastene 1–8, piltastene i sidefeltet, to klikk på ikonet og tre på versjonen.
   - Juksekoden i filterfeltet og «nordlys» i et tekstfelt teller ikke, og «nordlys» på en annen side heller ikke.
   - Bare 42 °C kan dukke opp av seg selv, fordi den simulerte temperaturen varierer.

## Latt være

- **Nyttår.** Et merke ved nyttår ville vært et tredje datoegg, og to holder. Grafenes datomerke ved midnatt var et fristende sted, men `charts.js` er stor, og grafene skal ikke ha pynt.
- **Lyder, konfetti og noe som dekker skjermen.** Det passer ikke uttrykket, og brukeren ba om stille detaljer.
- **Mer enn ett egg på samme sted,** og egg i bekreftelseslinja, Kontroller eller noe som har med Bruk og Behold å gjøre. Der skal ingenting være uventet.
- **Lagring på serveren av hva som er funnet.** Det er en liten ting for én nettleser, ikke en innstilling.
- **Egen snarvei til Om-egget.** Versjonsmerket er ikke en knapp, og det skulle ikke bli en tab-stopp bare for et egg. Juksekoden og Nordlys dekker tastaturet.
