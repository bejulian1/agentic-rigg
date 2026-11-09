# Kanttilfeller – alpha v0.0.4 · bygg 4

Gjennomgang av vindu, data, feiltilstander, lagring, interaksjon, enheter og grafer. Alt er testet mot en egen server på port 4323 med `AG_DATA_DIR=/tmp/claude-1000/agentic-rigg-edge`, i headless Chrome (ingen synlige vinduer).

## Slik er det testet

| Verktøy | Hva |
|---|---|
| `npm run check` | `node --check` på alle JS-filer |
| `npm test` (`scripts/test-store.js`, fra v0.0.5 `test/store.test.js`) | 15 tester av `server/store.js`: ødelagt, tom og feil type `state.json`, filer fra v0.0.1–v0.0.3, import av ugyldig/ondsinnet JSON, eksport → import, profilnavn |
| `npm run contrast` (`scripts/contrast.js`) | WCAG-kontrast for alle 144 kombinasjoner av fargetema, lys/mørk, aksent og høy kontrast |
| `scripts/fixtures/fake-smi/nvidia-smi` | Falsk `nvidia-smi` lagt først i `PATH` bare for testserveren. Modus leses fra en fil, så den kan byttes mens serveren går: `normal`, `na`, `allna`, `extreme`, `zero`, `longname`, `die`, `hang` |
| Headless-skript (egne, i scratch) | Skjermbilder, gjennomgang av alle sider i 12 vindusoppsett med sjekk av vannrett rulling og konsollfeil, Tab-gjennomgang med sjekk av synlig fokus, to faner samtidig, stopp/start av serveren med siden åpen, blokkert `localStorage`, og syntetiske systemprøver (1 og 128 tråder, 30 hwmon-enheter med lange navn, ingen nettverkskort) |
| `curl` | Importkallet med for stor, ugyldig og fremmed forespørsel |

## Vindu og visning

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| 360 px bredt: Sensorer | Tabellen var bredere enn vinduet. Navnekolonnen forsvant, og siden rullet sidelengs | Under 560 px innholdsbredde vises bare Nå og Maks. Navnet brytes over to linjer, notene om sensorer som mangler står i én kolonne | Skjermbilde `smalt-360-sensorer`, sveip uten vannrett rulling |
| 360 px: Kontroller | Glideren ble klemt til null bredde, og tallfeltet og Auto/Manuell overlappet teksten | Raden stables: tekst, glider i full bredde, så felt og ↺ | `smalt-360-kontroller` |
| 360 px: toppfeltet | Sidetittelen ble kuttet til «Kontr…», og Grafer-handlingene gikk utenfor | Under 620 px skjules «Ingen endringer»-teksten (prikken blir), «Siden 21:30», tidsvinduet på Grafer (finnes i Innstillinger) og teksten i statusmerket | Sveip: `main-head` uten overflyt på alle sider |
| 360 px: Innstillinger | Undertekstene ble tre ord brede ved siden av kontrollene | Radene brytes, så kontrollen legger seg under teksten når det trengs | `smalt-360-innstillinger`, `smalt-360-innstillinger-utseende` |
| Smalt: kortoverskrifter med forklaring (Viftekurve, Grafer) | Forklaringen presset kortet utover (Viftekurve 700 px med største skrift og luftig tetthet) | Kortoverskriften brytes, forklaringen går til egen linje. Min/maks-teksten på Grafer skjules under 560 px | Sveip 700×600 med skrift 116 % og luftig |
| Smalt: cache-, klokke- og punkttabeller | Tabellen var bredere enn kortet | Tabellen ruller inni kortet, ikke siden | Sveip 360/480 px |
| 600 px høyt, 2560×1440, 3840×2160 | Ingen feil. Sidene som fyller høyden bytter til rulling under 1000 px innholdsbredde | – | Sveip |
| Zoom 50 %, 150 %, 200 % (emulert med enhetspikselforhold) | Ingen feil etter rettingene over. 200 % på 1280×800 tilsvarer et 640×400-vindu | – | `zoom200-oversikt`, `zoom200-kontroller`, `zoom50-oversikt`, sveip |
| Største skrift + kompakt | Ingen overflyt | – | `storst-skrift-kompakt-*` |
| Klokker-kortet på Oversikt | Tomrom under kortene på 1920×1080 (og under Effekt) | Kortene i detaljraden fyller radens høyde: radene i klokketabellen fordeles, og noten i Temperatur og Effekt legger seg nederst | `oversikt-1920x1080-klokker-og-temperaturgraf` |

## Data

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| `[N/A]` i alle felt unntatt temperatur | Tallene ble «–» som de skal, men «Video (enc / dek)» viste «0 % / 0 %», og Effekt viste en tom stolpe og en løs «–» | Video viser «–». Effekt sier «Driveren oppgir ikke effekt for dette kortet.» | Fixtur `na`, `data-na-oversikt` |
| `[N/A]` i alle felt | Prøven forkastes, vakthunden starter `nvidia-smi` på nytt, og etter tre feil går appen til simulert med grunnen «ingen data på 5 s» | Allerede riktig | Fixtur `allna`/`hang` med SSE-lytter |
| Ekstreme verdier (100 °C, 100 %, 999 W, 0 MiB VRAM) | Klokkebegrensningen listet ni årsaker over seks linjer og blåste opp kortet. Effektgrafen klippet alt over 300 W | Kort form med alvorligste årsak først og «+8», hele lista i verktøytipset (også på Sensorer). Grafområdet følger kortets maks effekt og klokker | Fixtur `extreme`, `data-extreme-oversikt` |
| Alt 0, VRAM-total 0 | «av … GiB» skjules, ingen deling på null | Allerede riktig | Fixtur `zero` |
| Svært langt GPU-navn og driver | Ellipsis i enhetskortet, fullt navn i verktøytipset | Allerede riktig | Fixtur `longname`, `data-longname-*` |
| Mange hwmon-enheter og lange sensornavn | Ingen overflyt; navnet kuttes med ellipsis og står i verktøytipset | – | Syntetiske prøver (30 enheter × 6 sensorer) |
| Nettverkskort som kommer og går (Docker) | Radene for borte grensesnitt ble liggende i minnet og ble oppdatert usynlig | Rader for enheter som er borte, slettes når oppsettet endres | Kodegjennomgang og syntetiske prøver |
| Ingen nettverkskort | Nett-gruppen skjules. Nett-grafen viste «–MB/s» | Enheten skjules når verdien mangler | Syntetiske prøver |
| 1 og 128 tråder | Fungerer: én flis, eller 16 rader med fliser på System og «Per tråd (128)» på Sensorer | – | `f-128-*` (arbeidsbilder) |

## Feiltilstander

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| `nvidia-smi` dør midt i kjøringen | Startes på nytt, og etter tre feil på 30 s går appen til simulert med feilmeldingen som grunn | Allerede riktig | Fixtur `die`: tre omstarter, så `hello` med «Falt tilbake til simulert …» |
| Serveren stopper og starter igjen | Lokal historikk ble slettet hvis den nye serveren ikke hadde data ennå. Grafene trakk en rett strek over hullet | Historikken beholdes ved samme kilde. Opphold over 5 s gir brudd i linjene (grafer og sparklinjer) | `frakoblet-grafer`, `etter-omstart-brudd-i-grafen`. Etter tre omstarter: samme antall lyttere (8/4/1/4), én EventSource, ingen vekst i heap |
| Serveren svarer ikke på `/api/state` ved oppstart | Siden ble stående med skjelett og én feilmelding, uten nytt forsøk | Frakoblet-linja vises med nedtelling, «Prøv nå» virker, og appen starter når serveren svarer | Kodegjennomgang; stopp/start-test |
| Serveren er nede når HTML-en lastes | Nettleserens egen feilside | Ikke rettet: appen finnes ikke før HTML-en er lastet | Stopp/start-test |
| Treg eller manglende første melding (`hello`) | Strømmen kunne stå åpen uten data, og «Kobler til …» ble stående | Uten `hello` på 10 s regnes strømmen som brutt og kobles på nytt. «Tilkoblet» settes først når `hello` er kommet | Kodegjennomgang |
| Simulert ↔ live midt i kjøringen | Historikk byttes, min/maks/snitt for GPU nullstilles, varsel vises | Allerede riktig | Bytte via `/api/mode` med fixturen |
| Nettverksfeil i `fetch` | Varslene viste nettleserens engelske «Failed to fetch» | «Ingen kontakt med serveren.» eller «Serveren svarte ikke i tide.» (tidsgrense 10 s) | Kodegjennomgang |
| For stor forespørsel (> 64 kB) | Serveren brøt forbindelsen, så nettleseren fikk bare en nettverksfeil | 413 med «For stor forespørsel (maks 64 kB)» | `curl` med 200 kB |

## Lagring

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| `state.json` som inneholder `null` | **Serveren krasjet ved oppstart** (`null.applied`) | Alt som ikke er et JSON-objekt, behandles som ødelagt | `npm test` |
| Ødelagt `state.json` (ugyldig JSON, tall, liste) | Standardverdier, og fila ble overskrevet ved neste lagring | Fila kopieres først til `state.json.ødelagt-<tid>`. Serveren logger det, og første fane som åpnes får et varsel med stien | `npm test` |
| Tom `state.json` | Standardverdier | Allerede riktig, ingen kopi | `npm test` |
| `state.json` fra v0.0.1–v0.0.3 | Lest, men ett ugyldig felt i `applied` eller `rgb` (f.eks. en kurve med ett punkt, eller et temperaturområde under 5 °C) nullstilte hele blokken | Alle blokker leses felt for felt. Nye felt (fargetema, høy kontrast, soner) får standardverdi | `npm test` med ekte filer fra v0.0.1 og v0.0.2 |
| Import: ugyldig JSON, feil typer, ukjente verdier | Avvises med norsk melding, ingenting endres | Allerede riktig | `npm test`, `curl` |
| Import: prototype pollution (`__proto__`, `constructor`), ekstra felt | Ignoreres; bare kjente felt leses | Allerede riktig, nå med test | `npm test` |
| Import: for stor fil | Klienten avviser over 64 kB, serveren svarer 413 | Serverdelen rettet (over) | `curl` |
| Eksport → import | Gir de samme innstillingene tilbake, også fargetema og høy kontrast | – | `npm test` |
| `localStorage` blokkert | Alt virker; temaet settes da fra serveren etter lasting i stedet for før første tegning | Ugyldig skriftskala i lokal hurtigbuffer ignoreres nå | Headless med `localStorage` som kaster |
| To faner endrer innstillinger | Den andre fanen viste gamle verdier og kunne overskrive den lokale hurtigbufferen med dem | Fanene varsler hverandre (`BroadcastChannel`) og henter innstillingene på nytt, også når en fane blir synlig igjen | To faner: A endrer aksent, B endrer enhet; begge viser begge |
| Raske klikk på aksentfarger | Svar som kom i feil rekkefølge, eller en feil på et tidlig kall, kunne rulle tilbake et nyere valg | Bare svaret på siste lagring tas i bruk | Fire lagringer samtidig: siste vinner i begge faner |

## Interaksjon

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| Bare tastatur | Alle fokuserbare elementer har synlig fokus (ring, feltkant eller ring rundt glideknotten) | Fokusringen er nå hel aksentfarge (3:1 mot alle flater), 3 px i høy kontrast | Tab-gjennomgang av Innstillinger, Kontroller, Oversikt og Lys |
| Tab ut av en popover/meny | Popoveren ble hengende åpen bak fokus | Lukkes når fokus forlater den | Headless |
| Piltaster i meny med deaktivert valg | Fokus stoppet på det deaktiverte valget | Deaktiverte valg hoppes over. Home/End virker | Kodegjennomgang |
| Escape og klikk utenfor | Lukker og gir fokus tilbake til knappen | Allerede riktig | Headless |
| Snarveier mens man skriver | Tall bytter ikke side og Ctrl+Z angrer i feltet, ikke skjemaet | Allerede riktig | Headless |
| Dobbeltklikk på Bruk / Ctrl+Enter to ganger | Knappen ble deaktivert, men Ctrl+Enter to ganger rakk å sende to lagringer | Én lagring om gangen på Kontroller, Viftekurve og Lys | Kodegjennomgang |
| Dra viftekurvepunkt utenfor området | Pekeren fanges, verdien klemmes til området og naboene | Allerede riktig | – |
| Viftekurve med 2 eller 10 punkter | Kan ikke slette under 2 eller legge til over 10 (knapp, tabell, dobbeltklikk og Delete) | Allerede riktig | – |
| Profiler med tomt navn | Blir «Profil n» | Allerede riktig | `npm test` |
| Profiler med samme navn | To profiler kunne hete det samme | Navnet får «(2)», «(3)» … (uten hensyn til store/små bokstaver) | `npm test` |
| RGB-hex med ugyldig input | Feltet markeres og forklarer formatet, fargen beholdes | Allerede riktig | – |

## Tid og enheter

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| °F i temperaturområdet under Lys | Feltene hadde grenser i °C (20–95), så 104 °F ble klemt til 95 °F = 35 °C | Grensene følger enheten | Kodegjennomgang |
| °F i hysterese (Viftekurve) | Grensen ble satt ved oppstart; byttet du til °F etterpå, ble 18 °F klemt til 10 °F | Grensene oppdateres ved enhetsbytte, og tallfelt klemmer mot feltets gjeldende grenser | Kodegjennomgang |
| °F i viftekurve, terskler, grafakser og temperaturmål | Riktig | – | `fahrenheit-viftekurve`, `fahrenheit-grafer` |
| Desimaler, 12/24 t | Riktig | – | – |
| Lange økter | Historikk i nettleseren er tidsbegrenset (16 min), sensorhistorikk 63 s, toppverdi 5 min. Lyttere og DOM-noder er stabile | Rader for borte enheter slettes (over) | Lyttere/heap/noder målt før og etter omstarter |

## Grafer

| Tilfelle | Hva skjedde | Rettet | Testet |
|---|---|---|---|
| Temperaturgrafen ved «Tilpass» | 38–40 °C ble skalert til 36–44 °C, så en flat kurve så dramatisk ut | Minste spenn ved tilpasset akse: 20 °C (36 °F) for temperatur, 40 W effekt, 400 MHz kjerneklokke, 1000 MHz minneklokke, 1 GiB VRAM | `tema-*`, `oversikt-1920x1080-…` |
| Fylt flate under en tilpasset akse | Flaten gikk til aksens bunn (f.eks. 36 °C) og overdrev endringen | Flaten tegnes bare når aksen starter på områdets bunn (0 % / 0 W, eller 20 °C) | Samme |
| Sparklinjer | Samme overdrivelse (39–41 °C fylte hele høyden) | Samme minste spenn i fliser, statusblokk og Sensorer | Skjermbilder |
| Hull i dataene | Rett strek over hullet | Brudd i linjen etter 5 s uten data | `etter-omstart-brudd-i-grafen` |

## Ikke rettet, og hvorfor

- **Serveren nede når siden lastes:** nettleseren viser sin egen feilside; appen har ikke startet ennå.
- **GNOME-aksentfargen:** droppet. Den ligger i dconf (`org.gnome.desktop.interface accent-color`). Å lese den krever `gsettings` (et nytt program), en egen parser for dconf-databasens binærformat (GVariant) eller en D-Bus-klient mot innstillingsportalen. Ingen av delene er rene nok, og nettleseren kan ikke lese den selv.
- **Bryteren av, glidere og stolper i vanlig kontrast:** sporet (`--track`) er pynt og skal ikke ha 3:1. Bryteren har i stedet en kant med 3:1, og fyllet i glidere og stolper har 3:1 mot sporet. I høy kontrast får alle tre kant.
- **Minimumsspenn for nettrate:** ikke satt. Små og store rater er like viktige, og et fast minimum ville skjult lav trafikk.
- **Endringer gjort mens Bruk venter på svar** forkastes når svaret kommer. Kallet er lokalt og tar millisekunder, så det er ikke verdt mer kode.
- **`prefers-contrast: more`** fra systemet slår ikke på høy kontrast av seg selv. Bryteren er et eget valg, så det blir ikke to kilder for samme innstilling.
