# Agentic Rigg · localhost-demoen

**alpha v0.0.15 · bygg 15** – lokal demo av et maskinvareverktøy for Linux.

Agentic Rigg viser hva skjermkortet og resten av maskinen gjør akkurat nå. Den har en oversikt, en sensortabell med min/maks/snitt, grafer, en side med alt driveren vet om skjermkortet (i stil med GPU-Z) og en side om prosessor, hovedkort, minne og operativsystem (i stil med CPU-Z). I tillegg kan du klikke rundt i kontroller av samme type som et overklokkingsverktøy: effektgrense, klokke-offset, temperaturmål, vifte og viftekurve, profiler og RGB-lys.

Overvåkingen er ekte. Skjermkortet leses med `nvidia-smi`, og resten leses fra `/proc` og `/sys`. **Alle kontrollene er simulert.** Ingenting sendes til kortet eller endres på maskinen.

Fra v0.0.7 går alle skrivende handlinger til en **bakgrunnstjeneste** over en Unix-socket, slik den ekte appen skal gjøre det: tjenesten klemmer verdier til kortets grenser, ruller tilbake hvis du ikke trykker Behold innen fristen, setter viften til 100 % over 85 °C og til auto når den stopper. Tjenesten er foreløpig en simulering (`daemon-mock/`) som kjører som vanlig bruker. Kontrakten den ekte Rust-daemonen skal følge, står i `docs/daemon-api.md`.

Laget for maskinen med Palit GeForce RTX 2080 (TU104) og NVIDIA-driver 610, men virker hvor som helst. Uten `nvidia-smi` går appen over til et simulert skjermkort av seg selv. Systemdataene er ekte også da.

Appnavnet står bare ett sted, i `version.json` (`"name"`). Tittel, sidefeltet, Innstillinger › Om og loggen henter navnet derfra. Versjonen vises bare i Innstillinger › Om.

## Start

Krever Node 22 eller nyere. Ingen avhengigheter, så du trenger ikke `npm install`.

```sh
npm start            # bakgrunnstjenesten (simulert) og serveren; live GPU-tall hvis nvidia-smi finnes
npm run start:sim    # det samme, men tving simulert GPU
npm run start:server # bare serveren (Kontroller og Viftekurve sier da «ikke tilkoblet»)
npm run start:daemon # bare bakgrunnstjenesten (daemon-mock)
npm run check        # node --check på alle JS-filer (også test/)
npm test             # alle tester: server, kontrakt og UI (node --test test/), ca. 90 s
npm run test:server  # bare servertestene (tolking, sikkerhet, lagring, HTTP, SSE)
npm run test:ui      # bare røyktestene av UI i headless Chrome (smoke og påskeegg)
npm run contrast     # WCAG-kontrast for alle temaer, aksenter og høy kontrast
```

Åpne <http://127.0.0.1:4318/>. Stopp med Ctrl+C. Da stopper også bakgrunnstjenesten og `nvidia-smi`-prosessen.

`npm start` kjører `scripts/dev.js`. Den starter `daemon-mock/index.js` først, venter til socketen finnes, og starter så `server/index.js` med samme socket og datamappe. Krasjer tjenesten, startes den igjen etter 2 s (høyst fem ganger i minuttet). Stoppes den rent (f.eks. `kill <pid>`), blir den liggende, så du kan se hvordan appen oppfører seg uten tjeneste. Serveren starter selv aldri andre programmer enn `nvidia-smi`.

## Arkitektur

```
nettleser ──HTTP/SSE──► server/ (Node, 127.0.0.1:4318) ──NDJSON over Unix-socket──► daemon-mock/ (Node, simulert kort)
                         ├─ daemon-client.js  hello, abonnement, gjenoppkobling, «ikke tilkoblet»
                         └─ nvidia.js         live-tall fra nvidia-smi, bare lesing
```

- **Lesing** av live-tall går som før direkte fra `server/nvidia.js` og `/proc`/`/sys`.
- **Skriving** (Bruk, Behold, Rull tilbake, Tilbakestill) går til tjenesten. Hver Bruk gir en midlertidig endring med frist. Linja under toppfeltet spør «Behold endringene?» og teller ned. Uten svar ruller tjenesten tilbake, og det samme skjer når fanen lukkes eller lastes på nytt, eller forbindelsen brytes.
- **Simulert kort**: med tjenesten tilkoblet kommer de simulerte prøvene fra den (der viftekurven og nødregelen kjører). Uten tjeneste går den lokale modellen videre med siste kjente verdier.
- Socketen ligger i `$XDG_RUNTIME_DIR/agentic-rigg/` (mappe 0700, socket 0600), med et filnavn som bygger på datamappa. Den ekte daemonen er planlagt som en Rust-tjeneste med root, polkit og systemd (krever din godkjenning, se `docs/VEIKART.md`).

## Sider

Sidefeltet har kortvelgeren øverst (én rad med navn og statusprikk; klikk for driver, PCIe og Live/Simulert) og en statusblokk nederst med opptil 8 verdier i den rekkefølgen du velger. Fest en sensor med nålen på Sensorer, og endre rekkefølgen under Innstillinger › Sidefelt (dra eller Alt+↑/↓) eller med høyreklikk på raden. Helt nederst står Innstillinger (Ctrl+,), Tastatursnarveier (?, Ctrl+?) og knappen som legger sammen sidefeltet til en smal ikonlist (Ctrl+B). Valget huskes. Under 860 px er sidefeltet alltid en ikonlist. Versjon og feilsøking står under Innstillinger › Om.

| Tast | Side | Hva |
|---|---|---|
| 1 | Oversikt | Seks nøkkeltall med sparklinje, skjermkort, målere for temperatur og effekt, klokker og tre grafer med eget tidsvindu (1 min–48 t) |
| 2 | Sensorer | Én tabell gruppert etter enhet (skjermkort, prosessor, minne, andre sensorer, nett) med Nå, Min, Maks, Snitt og 60 s. Klikk på et navn for en større graf under raden. Min og maks huskes mellom økter. Filter (Ctrl+F). Forklaring på det som ikke kan leses |
| 3 | Grafer | Valgfrie grafer for 1 min til 48 t med historikk fra serveren (min/maks-bånd ved lange vinduer), markør på alle grafene eller bare én, pause per graf eller alle (mellomrom), forklaring der serier kan skjules |
| 4 | Grafikkort | Fra driveren (nvidia-smi og sysfs) ved siden av en innebygd spesifikasjon merket «Referanse, ikke målt». Kopier som tekst/Markdown |
| 5 | System | Prosessor, cache, instruksjonssett, tråder, hovedkort/BIOS, minne og OS. Kopier som tekst/Markdown |
| 6–8 | Kontroller, Viftekurve, Lys | Simulert. Forkast og Bruk står i toppfeltet (Ctrl+Z / Ctrl+Enter). Lys viser kortet i 3D: vri det, ta av lag til bare kretskortet er igjen, og skill lagene. Uten WebGL eller med programvaregrafikk brukes det flate kortet (3D \| 2D i overskriften) |
| Ctrl+, | Innstillinger | Dekker hele vinduet, med egen navigasjon til venstre: Generelt, Utseende, Enheter og format, Sidefelt, Grafer og sensorer, Data (eksport/import/tilbakestill) og Om (versjon og feilsøking). Søk i alle delene med / eller Ctrl+F. Tilbake med knappen øverst, Esc eller Ctrl+, igjen, til siden du kom fra |

## Tilpassing

Alle valg lagres i `state.json` i datamappa og trer i kraft straks: lyst/mørkt (System/Lys/Mørk), fargetema (Grafitt, Sand, Fjord, Midnatt), høy kontrast, aksentfarge (seks, med navn), tetthet, skriftstørrelse, animasjoner (Automatisk/Full/Redusert/Av), startside, oppdateringsintervall, °C/°F, GiB/GB, MB/s eller Mbit/s, desimaler, 12/24 t, statusverdier i sidefeltet og varselterskler, sammenlagt sidefelt, små overraskelser (av/på), synlige sider, tidsvindu for grafene (per side), markør på alle grafene, y-akse, fylt flate, hvilke grafer som vises og sparklinjer. Innstillingene kan eksporteres og importeres som JSON.

For utviklere: fargene er CSS-tokens i `web/styles.css` (seksjon 2: `:root`, `:root[data-resolved='dark']`, `[data-palette=…]`, `[data-contrast='high']`, `:root[data-accent=…]`). Brukervalgene legges på `<html>` av `web/js/prefs.js` (`data-palette`, `data-contrast`, `data-accent`, `data-density`, `data-motion`, `--fs-scale` osv.), og serveren validerer dem i `server/store.js` (`mergeSettings`). Et nytt fargetema er én blokk for lyst og én for mørkt i seksjon 2, en linje i `PALETTES` i `store.js` og `settings.js`, og `npm run contrast` må gå gjennom.

## Tester

Testene bruker bare Nodes innebygde `node:test` og `node:assert`, og ligger i `test/`:

| Fil | Hva |
|---|---|
| `nvidia-parse.test.js` | Tolking av `--query-gpu`-linjer og `nvidia-smi -q` (ekte utdata fra RTX 2080 i `test/fixtures/`), med `[N/A]`, `[Not Supported]`, tomme felt og avkortede linjer |
| `nvidia-safety.test.js` | At `assertReadOnly` avviser set-flagg (`-pl`, `-lgc`, `-ac` …), at argumentlistene er frosne, at ingen annen fil i `server/` starter programmer, at `daemon-mock/` ikke starter programmer, laster NVML eller rører `/dev`, og at `scripts/dev.js` bare starter de to Node-filene |
| `daemon-contract.test.js` | Kontrakten mot en ekte mock over socket: hello og versjon, hver operasjon, feilkoder, klemming, bekreftelse innen fristen, tilbakerulling ved frist og frakobling, nødregel over 85 °C, vifte til auto ved avslutning (også etter SIGKILL), ugyldig JSON, for store meldinger og socket-rettigheter |
| `daemon-integration.test.js` | Server ↔ mock: Bruk, Behold og Rull tilbake via HTTP, frist, fane som lukkes, nødregel i prøvene, tjenesten som stopper og kommer tilbake, server uten tjeneste og server som dør midt i fristen |
| `history.test.js` | Historikken for grafene: bøtter med min/snitt/maks, nedsampling, hull, grenser for serier, rader og filstørrelse, lagring og innlesing, og ødelagt fil |
| `store.test.js` | Lagring, migrering fra v0.0.1–v0.0.3, import og validering i `server/store.js`, start/stopp for viften, statusblokken og min/maks-fila |
| `simulator.test.js` | Den simulerte viften med start og stopp: kurve over start, minste fart under stopp, tilstanden beholdes mellom |
| `http.test.js` | Serveren som barneprosess: Host, Origin, sti-traversering, alle API-kall, ugyldig og for stor kropp, `index.html` med utseendet i `<html>` |
| `sse.test.js` | `/api/stream`: hello, prøver, flere lyttere, og at kilden stopper når siste lytter lukker |
| `fake-smi.test.js` | Live-modus med den falske `nvidia-smi` først i `PATH` |
| `ui/easter.test.js` | Headless Chrome: de små overraskelsene (v0.0.12) ved 1440 og 360 px uten konsollfeil, vannrett rulling eller endret tilstand, bryteren som slår av alt, «Av» uten animasjoner, og ingen utløsning ved et uhell. Fasit i `docs/paaskeegg.md` |
| `ui/settings.test.js` | Headless Chrome: Innstillinger i hele vinduet (v0.0.15). Sidefeltet skjules, Esc, tilbakeknappen, Ctrl+, og nettleserens tilbake går til siden du kom fra med samme rulling og fokus, omlasting på `#/innstillinger` (med og uten historikk), 360 px, og søket (/, Ctrl+F, treff, markering, kontroller i treffene, tomtilstand og Esc) |
| `ui/smoke.test.js` | Headless Chrome: alle ni sider i lyst og mørkt ved 1440, 1100 og 360 px, uten konsollfeil og vannrett rulling, animasjonsnivå «Av» uten animasjoner (også med bekreftelsen), ønskelisten fra v0.0.6, bekreft-eller-rull-tilbake (Behold, Rull tilbake nå, frist, klemt verdi, omlasting, tjenesten borte), Sensorer som én tabell (grupper, sammenlegging, graf for valgt sensor, piltaster), og sidefeltet som legges sammen (knapp, Ctrl+B, huskes, 360 px). Fra v0.0.13: Lys i 3D (tastatur, lagtrinn, ingen bilder i ro, nordlys, frigjøring og 2D uten WebGL). Fra v0.0.14: sidefeltet legges sammen, utvides og lastes på nytt også mot en server som ikke kjenner valget, og tallene i ikonlista |

Servertestene starter egne servere med `AG_SIMULATE=1` (eller den falske `nvidia-smi`), en ledig tilfeldig port, en midlertidig datamappe og en egen daemon-mock med socket i en midlertidig mappe. De rører aldri port 4318, det ekte kortet eller `.data/`. UI-testen bruker `/usr/bin/google-chrome --headless=new` med egen profil i en midlertidig mappe (en annen sti kan settes med `AG_CHROME`), og hoppes over med en melding hvis Chrome mangler. Alle prosesser testene starter, drepes etterpå, også ved feil.

Kanttilfeller og hvordan de er testet står i `docs/edge-cases.md`. For å teste feil fra skjermkortet uten å røre maskinen finnes en falsk `nvidia-smi` i `scripts/fixtures/fake-smi/`. Den legges bare først i `PATH` for en egen testserver (se kommentaren øverst i fila).

## Miljøvariabler

| Variabel | Standard | Betydning |
|---|---|---|
| `AG_PORT` | `4318` | Porten. Serveren binder bare til `127.0.0.1`. |
| `AG_DATA_DIR` | `agentic-rigg/.data/` | Her lagres `state.json` med simulerte innstillinger, profiler, lys og app-innstillinger, `sensor-minmax.json` med min og maks for Sensorer, og `history.json` med historikken for grafene (høyst 16 MiB). Mappa ligger i `.gitignore`. |
| `AG_SIMULATE` | av | `1` tvinger simulert skjermkort. Da kan du heller ikke bytte til live fra Innstillinger. CPU, minne, nett og sensorer er fortsatt ekte. |
| `AG_DAEMON_SOCKET` | `$XDG_RUNTIME_DIR/agentic-rigg/mock-<id>.sock` | Socketen til bakgrunnstjenesten. Settes av `npm start` for begge prosessene. Høyst 107 byte. |
| `AG_NO_DAEMON` | av | `1`: `npm start` starter bare serveren. |
| `AG_MOCK_CONFIRM_MIN` | `10` | Bare mocken: korteste frist i sekunder (testene bruker 1). |
| `AG_TEST_HISTORY` | av | `1` fyller historikken med syntetiske 48 timer ved oppstart (for tester og skjermbilder). |
| `AG_MOCK_TEST_OPS` | av | Bare mocken: `1` slår på `mock.set_temperature` (holder temperaturen fast, for å prøve nødregelen). |

## Sikkerhetsregel

Demoen skal **aldri** endre noe på det ekte skjermkortet eller maskinen.

- `server/nvidia.js` er den eneste fila som starter et program, og den starter bare `nvidia-smi` med tre faste argumentlister:
  1. `--query-gpu=… --format=csv,noheader,nounits --id=0` én gang ved oppstart (faste egenskaper),
  2. `-q --id=0` én gang ved oppstart (temperaturgrenser, CUDA-versjon, BAR1 og lignende; bare utvalgte felt tas vare på),
  3. `--query-gpu=… -lms 500` som én prosess mens en nettleser lytter.

  Listene er frosne konstanter. Prosessene startes med `execFile`/`spawn` uten shell, og ingenting fra nettleseren havner i argumentene. Ved oppstart sjekkes hvert argument mot en liste med tillatte flagg (`--query-gpu`, `--format`, `--id`, `-lms`, `-q`).
- Ingen flagg som setter noe (`-pl`, `-lgc`, `-lmc`, `-pm`, `-rgc` osv.), ingen `nvidia-settings`, ingen `sudo`, ingen `dmidecode`, ingen `/dev/i2c-*` og ingen `/dev/mem`.
- `server/system.js` leser bare filer i `/proc`, `/sys` og `/etc/os-release`, og starter ingen programmer. Root-only-filer (serienumre, `product_uuid`, RAPL-energitelleren) åpnes ikke. Tilgang sjekkes med `access()` der det trengs, og en `EACCES` vises som «krever root» i stedet for en feil.
- Alle skrivende kall for kortet (`/api/apply`, `/api/confirm`, `/api/revert`, `/api/reset`) går til bakgrunnstjenesten, som i denne versjonen er `daemon-mock/`: ren simulering, kjører som vanlig bruker, starter ingen programmer, laster ikke NVML og rører ingen enheter. Svaret har `simulated: true`. `/api/rgb` og profiler lagres som før bare i `AG_DATA_DIR/state.json`. UI-et sier hver gang at ingenting ble sendt til kortet.
- `scripts/dev.js` (`npm start`) er det eneste stedet utenfor `server/nvidia.js` som starter prosesser, og den starter bare Node med `daemon-mock/index.js` og `server/index.js`.
- Serveren godtar bare `Host: 127.0.0.1:<port>` eller `localhost:<port>`, og avviser POST fra andre nettsteder (`Origin`).

## Oppbygning

```
version.json    navn, versjon, kanal og byggnummer (eneste sted appnavnet står)
server/
  index.js      HTTP-server, API, statiske filer, rydding ved stopp
  daemon-client.js  klient for bakgrunnstjenesten (node:net, gjenoppkobling)
  daemon-protocol.js felles for klient og mock: versjon, feilkoder, socket, felt
  nvidia.js     den eneste koden som starter nvidia-smi (bare lesing)
  system.js     /proc- og /sys-lesing: CPU, cache, DMI, minne, nett, hwmon, PCI
  specs.js      innebygd spesifikasjonstabell slått opp på PCI-ID
  telemetry.js  velger GPU-kilde, holder historikk, sender SSE
  simulator.js  simulert kort som reagerer på simulerte innstillinger
  store.js      validering og lagring i state.json og sensor-minmax.json
  history.js    historikk for grafene: 1 s/10 s/60 s-bøtter med min, snitt og maks, history.json
daemon-mock/    simulert bakgrunnstjeneste (docs/daemon-api.md)
  index.js      oppstart, signaler, vifte til auto ved stopp og krasj
  daemon.js     Unix-socket, NDJSON, økter, abonnement, grenser for meldinger
  card.js       simulert kort: grenser, klemming, txn med frist, nødregel, lagring
web/
  index.html    skall, sider, dialoger og Lucide-ikonsprite
  styles.css    tokens (farger, skrift, avstander, bevegelse), komponenter og sider
  js/theme-boot.js  setter tema, aksent, tetthet og data-motion før første tegning
  js/theme.js       System/Lys/Mørk og data-resolved
  js/prefs.js       legger brukervalgene på <html> og følger prefers-reduced-motion
  js/app.js         skall: navigasjon, enhetskort, statusblokk, meny, dialoger, taster
  js/defs.js        sidene og verdiene statusblokken kan vise
  js/pins.js        festede sensorer og rekkefølgen i statusblokken
  icons/            appikonet (fullfarge og symbolsk)
  js/api.js         fetch og SSE med historikk (15 min) og gjenoppkobling
  js/confirm.js     «Behold endringene?» med nedtelling, tilbakerullet, Bruk igjen
  js/fmt.js         tall, enheter (U+202F), temperatur, minne, nettrate, tid
  js/ui.js          varsler, segment, bryter, tallfelt, verktøytips, popover/meny, dialog
  js/charts.js      linjegrafer på canvas med pene aksemerker
  js/sparkline.js   SVG-sparklinjer mot tid
  js/sensormeta.js  menneskelige navn på hwmon og nett, CPU-temp, nett-sum
  js/colorpicker.js egen fargevelger til Lys (fargeknapper, hex, «Egendefinert …» i popover)
  js/fancurve.js    redigerbar viftekurve på canvas
  js/rgb.js         effektmotoren (RgbEngine) og det flate kortet (SvgCard)
  js/lys3d/         3D-kortet på Lys: view.js (kamera, lag, input), card-model.js, layout.js, textures.js, gl/ (WebGL 2)
  js/easter.js      små overraskelser (v0.0.12), alle på ett sted; fasit i docs/paaskeegg.md
  js/rows.js, copy.js, gpufmt.js, meters.js, util.js   felles småbiter
  js/pages/*.js     én fil per side
scripts/dev.js        npm start: starter daemon-mock og serveren
scripts/check.js      syntakssjekk
scripts/contrast.js   kontrast for alle fargekombinasjoner
scripts/fixtures/     falsk nvidia-smi for tester
test/*.test.js        servertester (node --test)
test/ui/              røyktest av UI i headless Chrome
test/helpers/         testserver og en liten CDP-klient for Chrome
test/fixtures/        utdata fra nvidia-smi -q (UUID fjernet)
assets/icons/      appikonet for senere .desktop/Electron (hicolor SVG og PNG)
docs/              STATUS, PATCHLOG, daemon-api (kontrakten), edge-cases, bevegelse, designkritikk med status, LISENSER, paaskeegg (fasit, røper alt), innstillinger-v0.0.15 (Innstillinger i hele vinduet)
```

## API

| Metode og sti | Hva |
|---|---|
| `GET /api/state` | App (navn, versjon, bygg), kilde, gjeldende innstillinger, bakgrunnstjenesten (`daemon`), grenser fra kortet, lys, profiler, app-innstillinger |
| `GET /api/stream?tab=<id>` | Server-Sent Events: `hello` (kilde, GPU-historikk, systemhistorikk), `sample` hvert 500. ms (GPU), `sys` hvert sekund (CPU, minne, nett, hwmon) og `daemon` (tilkoblet, ventende endring, grenser, nødregel, siste utfall) |
| `GET /api/gpuinfo` | Skjermkortinfo fra driveren og sysfs, pluss oppslag i spesifikasjonstabellen |
| `GET /api/system` | Faste systemdata (CPU, cache, flagg, DMI, OS) og én fersk prøve |
| `GET /api/history?window=…&keys=…&points=…` | Historikk for grafene: window 60–172800 s (sju verdier), 1–16 serier, 10–4000 punkter. Kolonner med tid, min, snitt og maks. Bare lesing |
| `GET /api/sensors` | Siste GPU-prøve, én systemprøve og forklaringer på sensorer som mangler |
| `POST /api/apply` | Delvis oppdatering via tjenesten → `{ applied, pending: { txn, timeoutS, remainingMs, changed, clamped }, clamped }`. 503 uten tjeneste |
| `POST /api/confirm` | `{"txn": "t3"}`: behold endringen som venter |
| `POST /api/revert` | `{"txn": "t3"}`: rull tilbake nå |
| `POST /api/reset` | Kortets standard via tjenesten (bekreftes straks) |
| `POST /api/tab/close` | `{"tab": "<id>"}`: siden forlates; serveren ruller tilbake fanens ventende endring |
| `GET /api/daemon`, `POST /api/daemon/retry` | Status for tjenesten, og koble til nå |
| `POST /api/rgb` | Farge, effekt, lysstyrke, hastighet → `simulated: true` |
| `GET/POST/PUT/PATCH/DELETE /api/profiles/1..5` | Hent, lagre, gjenopprett (angre), gi nytt navn og tøm profilplasser |
| `POST /api/settings` | Delvis oppdatering av app-innstillingene (alle valideres i `store.js`) |
| `POST /api/settings/reset` | Standard app-innstillinger |
| `POST /api/settings/import` | `{"settings": {…}}` erstatter alle app-innstillinger (ugyldig = 400, ingenting endres) |
| `POST /api/mode` | Bytt GPU-kilde nå: `{"mode":"live"}` eller `{"mode":"sim"}` |
| `GET/POST/DELETE /api/minmax` | Min og maks for Sensorer mellom økter: hent, flett inn (`replace: true` ved nullstilling og angre) og glem |

Se `docs/STATUS.md` for hva som virker og hva som mangler, og `docs/PATCHLOG.md` for endringer.
