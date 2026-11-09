# Status

**alpha v0.0.15 · bygg 15** – 6. november 2026: Innstillinger dekker hele vinduet. Sidefeltet skjules, og Innstillinger har sin egen navigasjon med tilbake, søk i alle delene og de sju delene. Panelene står i to eller tre spalter. Tilbake (←, Esc, Ctrl+, eller nettleserens tilbake) gir siden du kom fra, med samme rulling og fokus. Valg og begrunnelse i `docs/innstillinger-v0.0.15.md`. Bygg 14: sidefeltet kan alltid utvides igjen. Valget gjelder i nettleseren straks, også mot en server som ikke kjenner det (brukerens server på 4318 er v0.0.8 og bør startes på nytt). Tallene i det sammenlagte sidefeltet har tydelig rytme og et lite kildemerke (GPU, CPU, RAM), og ikonlista ruller aldri (`docs/PATCHLOG.md`). Bygg 13: Lys viser skjermkortet i 3D. Du kan vri det, ta av lag til bare kretskortet er igjen, og skille lagene. Forhåndsvisningen fyller siden, og glidere og fargeknapper er slankere i hele appen. Det flate kortet er reserven uten WebGL, ved programvaregrafikk og når 3D tegner for tregt. Valg og målinger står i `docs/lys-3d-v0.0.13.md`. Bygg 12: noen små overraskelser (påskeegg) rundt om i appen. De endrer aldri tall, innstillinger eller kortet, og de slås av med «Små overraskelser» under Innstillinger › Generelt (fasit i `docs/paaskeegg.md`). Bygg 11: grafer fra 1 min til 48 t med historikk på serveren, pause per graf, markør på alle eller én graf, faste seriefarger, seks aksentfarger og nye målere for temperatur og effekt (valg og funn i `docs/grafer-og-malere-v0.0.11.md`). Bygg 10: sammenleggbart sidefelt uten hovedmeny (valg og funn i `docs/sidefelt-v0.0.10.md`). Innstillinger, Tastatursnarveier og knappen som legger sammen sidefeltet til en ikonlist (Ctrl+B, huskes) står nederst i sidefeltet, Om er en del av Innstillinger, og kortvelgeren er én tynn rad. Bygg 9: tettere og mer profesjonelt (plan og funn i `docs/design-pro-v0.0.9.md`). Paneler med tynn ramme og lav overskriftsstripe i stedet for svevende kort, 26 px rader, verdiene rett ved etikettene, og Sensorer som én tabell gruppert etter enhet med en større graf under valgt rad. Alle funksjoner, temaer og animasjonsnivåer som før. Bygg 8: finpuss etter daemon-flyten (funn i `docs/finpuss-v0.0.8.md`). Én ventende endring om gangen på tvers av faner, rene trappetrinn for grensen i grafene, ingen tekst i viftekurven som krysses av linjer, og fokus som går tilbake dit det kom fra. Bygg 7 gjorde appen klar for ekte bakgrunnstjeneste: alle skrivende handlinger går nå til en bakgrunnstjeneste over en Unix-socket (`docs/daemon-api.md`). Tjenesten er foreløpig en simulering (`daemon-mock/`) som kjører som vanlig bruker og startes av `npm start`. Bruk gir en midlertidig endring som må beholdes innen fristen, ellers ruller tjenesten tilbake. Grensene kortet melder, klemte verdier og nødregelen (full vifte over 85 °C, vifte til auto når tjenesten stopper) vises i Kontroller og Viftekurve. Endringer i `docs/PATCHLOG.md`. `npm test` kjører 165 tester (ca. 90 s). Før det: kritikkrunde 2 og ønskeliste (v0.0.6), redesign, kanttilfeller og fargetemaer (`docs/edge-cases.md`) og bevegelse (`docs/bevegelse.md`).

## Virker live

### Skjermkort (nvidia-smi)

Lest med én fast `nvidia-smi --query-gpu=… -lms 500`-prosess, bare mens en nettleser er koblet til. Den stopper 10 s etter at siste fane er lukket. I tillegg kjøres én fast `--query-gpu`-lesing og én `nvidia-smi -q` ved oppstart.

- Hvert 500. ms: temperatur, GPU-last, minnekontroller-last, encoder/decoder-last, grafikk-, SM-, minne- og videoklokke, strømtrekk, effektgrense og håndhevet grense, viftefart, VRAM, P-tilstand, PCIe-generasjon og -bredde nå, og aktive klokkebegrensninger (tomgang, effektgrense, termisk osv.).
- Én gang: navn, driver, VBIOS, PCI-buss, PCI-ID for enhet og subsystem, UUID, compute capability, PCIe maks, maks klokker, effektgrenser.
- Fra `nvidia-smi -q` (bare utvalgte felt tas vare på): merke og arkitektur, delenummer, CUDA-versjon, BAR1, temperaturmål, maks driftstemperatur, nedklokking (slowdown) og avstenging (shutdown), maks videoklokke, GSP-fastvare og InfoROM.
- Fra sysfs (`/sys/bus/pci/devices/…`): PCI-ID-er, link-hastighet, BAR-størrelser og hvilke BAR1-størrelser kortet støtter. Det er slik Grafikkort-siden vet at kortet ikke støtter Resizable BAR (største BAR1 er 256 MiB).
- Hvis `nvidia-smi` dør, henger eller bare svarer N/A, startes den på nytt. Etter tre feil på 30 s går appen over til simulert GPU og sier hvorfor. Testet med en falsk `nvidia-smi` (`scripts/fixtures/fake-smi/`) som bare legges i `PATH` for en testserver.

### Resten av maskinen (/proc og /sys)

Leses av `server/system.js` uten å starte noe program. Faste data leses én gang ved oppstart. Levende data leses hvert sekund, bare mens en nettleser lytter, og sendes på den samme SSE-strømmen som GPU-dataene.

- **CPU:** modell, produsent, familie/modell/stepping, mikrokode, kjerner og tråder, cache-nivåer fra `/sys/devices/system/cpu/cpu0/cache/` (per instans, antall og totalt), alle instruksjonssett med et utvalg som brikker, cpufreq min/maks, driver, governor og boost. Live: total last og last per tråd fra `/proc/stat`, klokke per tråd fra `scaling_cur_freq`.
- **Hovedkort og BIOS:** fra `/sys/class/dmi/id/`. Serienummerfilene er root-only og leses ikke.
- **Minne:** totalt, brukt, tilgjengelig, buffer/cache og swap fra `/proc/meminfo`.
- **OS:** distribusjon, kjerne, arkitektur, økttype (fra `XDG_SESSION_TYPE` i serverens miljø), skrivebord, vertsnavn, oppetid og last.
- **Sensorer:** alle `temp*`, `fan*`, `in*`, `power*` (og `curr*`) i `/sys/class/hwmon`, med etiketter. På denne maskinen er det `k10temp` (Tctl og Tdie), `iwlwifi_1`, `ucsi_source_psy_8_00081` og `hidpp_battery_0` (som ikke har noen verdier). Nett: ned/opp per grensesnitt fra `/sys/class/net/*/statistics`.

### Utseende og tilpassing

- Lyst, mørkt og System (følger `prefers-color-scheme` live). Fire fargetemaer (Grafitt, Sand, Fjord, Midnatt) valgt med miniatyrkort, høy kontrast som egen bryter, seks aksentfarger (med navn), tre tettheter, fire skriftstørrelser og animasjonsnivå Automatisk/Full/Redusert/Av. Alt lagres i `state.json` og settes på `<html>` før første tegning (ingen blink).
- Alle farger er tokens med én kilde i `web/styles.css`, seksjon 2. `npm run contrast` regner ut kontrasten for alle 96 kombinasjoner (fra v0.0.11; var 144 med ni aksenter), også seriepaletten for grafene, (tema × lys/mørk × aksent × høy kontrast): ingen tekst under 4,5:1 og ingen UI-grense under 3:1.
- Enheter (°C/°F, GiB/GB, MB/s/Mbit/s), desimaler, 12/24 t, startside, synlige sider, statusverdier, varselterskler, graftidsvindu og y-akse, fylt flate, valg av grafer og sparklinjer. Eksport/import som JSON og tilbakestilling med angre.
- Sidefeltet kan legges sammen til en ikonlist (knapp nederst eller Ctrl+B, huskes); under 860 px er det alltid en ikonlist.
- Innstillinger dekker hele vinduet (v0.0.15): egen navigasjon der sidefeltet står, søk i alle delene (/ eller Ctrl+F) med kontrollene i treffene, og panelene i én til tre spalter etter bredden. Tilbake med ←, Esc, Ctrl+, eller nettleserens tilbake til siden du kom fra, med rulling og fokus. Under 700 px står delene som ikoner øverst.
- Statusblokken: opptil 8 verdier i egen rekkefølge, faste verdier eller sensorer festet fra Sensorer (nål ved hover/fokus, høyreklikk). Rekkefølgen endres i Innstillinger › Sidefelt med dra eller Alt+↑/↓, eller med høyreklikk i sidefeltet. Følger med til andre faner.
- Min og maks på Sensorer huskes mellom økter (kan slås av), med dato i «Min/maks siden …». Snitt gjelder økten. Lagres i `AG_DATA_DIR/sensor-minmax.json`, skrives høyst hvert 30. s.
- Eget appikon (skjermkort med vifte) som favicon, i sidefeltet og i Om. Filer i `web/icons/` og `assets/icons/`.
- Små overraskelser (v0.0.12): ti skjulte detaljer, samlet i `web/js/easter.js`. De følger animasjonsnivået, høy kontrast, tastatur og skjermlesere, bruker aldri gul, oransje eller rød, og slås av med én bryter under Innstillinger › Generelt (`surprises`, på som standard).
- Bevegelse etter `docs/bevegelse.md`: sidebytte, sammenlegging, popover/meny, dialog, verktøytips, varsler med stabel, segmentmarkør, brytere, temabytte som kryssfade, stolper, y-akse som glir, viftekurve (vekst, hjelpelinjer, morf), RGB-kryssfade og frakoblet-linja. Bare `transform` og opasitet (pluss `grid-template-rows` ved sammenlegging). Med «Redusert» er det bare korte tonninger, og med «Av» ingen animasjoner.
- Én felles `requestAnimationFrame`-løkke. Bare siden du ser på tegnes, og løkka sover helt når ingenting beveger seg. Grafene tegnes når rullingen har flyttet seg ⅓ piksel, ikke hvert bilde. RGB-viftene roteres av kompositoren. Med redusert eller ingen bevegelse tegnes grafene i trinn, stolpene hopper og RGB-forhåndsvisningen er et stillbilde.

## Simulert

- **Bakgrunnstjenesten** (`daemon-mock/`). Snakker kontrakten i `docs/daemon-api.md` (NDJSON over Unix-socket, protokoll 1) i en privat mappe under `$XDG_RUNTIME_DIR/agentic-rigg/` (0700/0600). Simulerer kortet, klemmer til grensene NVML målte (effekt 105–275 W, temperaturmål 65–88 °C, vifte 35–100 %) og en strengere policy for offset (kjerne −200…+300, minne −1000…+1500 MHz), gir hver skriving en frist (10–60 s, valgt under Innstillinger), ruller tilbake ved frist, frakobling, lukket fane og stopp, setter viften til 100 % over 85 °C og til auto når den stopper. Det bekreftede lagres i `AG_DATA_DIR/daemon-mock.json`. Uten tjeneste er Bruk låst, og den lokale modellen går videre med siste kjente verdier.
- **Alle kontroller.** Effektgrense (105–275 W), kjerneklokke-offset (−200 til +300 MHz), minneklokke-offset (−1000 til +1500 MHz), temperaturmål (65–88 °C, vises i °F når det er valgt), vifte Auto (driveren)/Kurve/Manuell (35–100 %), viftekurve som aldri faller, med minste fart, start og stopp med hysterese (under stopp hviler viften på minste fart, kurven tar over igjen ved start; ekte 0 % går ikke på kortet), «senk først etter» (hysterese per trinn), reaksjonstid og temperaturkilde, og fem profiler (lagre som ny, hent, lagre over, gi nytt navn, tøm med angre; like verdier lagres ikke to ganger). Spennings- og klokkekurven er en illustrasjon, ikke lest fra kortet. Kontroller viser effekt og kjerneklokke for siste minutt, så du ser hva modellen gjør. Går gjennom bakgrunnstjenesten (over), og serveren svarer `simulated: true`. Profiler lagres i `AG_DATA_DIR/state.json`.
- **Lys (RGB).** Kortet vises i 3D (WebGL 2): vinkler, lagene Hel, Uten deksel, Uten kjøler og Bare kretskort, eksplodert visning og delliste. Uten WebGL, ved programvaregrafikk eller når 3D tegner for tregt brukes det flate kortet. Soner, farge per sone, lysstyrke, hastighet, retning og effektene Statisk, Pust, Regnbue, Fargesyklus, Temperatur (farge etter GPU-temperaturen) og Av. Statisk, Pust og Av er dokumentert for kontrolleren (ikke testet på kortet); de andre er merket med et øye som bare forhåndsvisning. Ingenting sendes til lyskontrolleren.
- **Simulert skjermkort.** Brukes med `AG_SIMULATE=1`, med «Start med simulert GPU» (Innstillinger › Generelt), hvis du bytter under Innstillinger, eller hvis `nvidia-smi` mangler eller feiler. Grafikkort-siden viser da simulatorens verdier (merket «Simulert») og spesifikasjonstabellen. CPU, minne, nett og sensorer er fortsatt ekte.

## Referanse, ikke målt

Grafikkort-siden har en innebygd spesifikasjonstabell (`server/specs.js`) slått opp på PCI-ID. For `10de:1e87` (RTX 2080 Rev. A, TU104): 12 nm TSMC, 545 mm², 13,6 mrd. transistorer, 2944 CUDA-kjerner, 184 TMU, 64 ROP, 46 SM, 368 tensorkjerner, 46 RT-kjerner, 4 MiB L2, GDDR6 256 bit, 8 GB, 14 Gbps, 448 GB/s, grunnklokke 1515 MHz, boost 1710 MHz, lansert september 2018. Piksel- og teksturfyllrate og FP32 regnes fra tabellen med referanse-boost, og formelen står i verktøytipset. Minneleverandøren vises som «Ukjent på Linux».

## Verifisert i bygg 6 (del 2, ønskeliste)

- `npm run check` (48 filer), `npm test` (105 tester, ca. 35 s) og `npm run contrast` (144 kombinasjoner, dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Nye tester for start/stopp i `store.js` og simulatoren, min/maks-fila, statusblokkens liste og en røyktest av ønskelisten i headless Chrome (start/stopp med Bruk, feste, full blokk, Alt+↓, tomtilstand, nullstilling, ikon).
- Sett i headless Chrome i lyst og mørkt ved 1440×900, 1100×800 og 360 px, og alle ni sider etterpå. Synk mellom to faner (feste og nullstilling av min/maks) prøvd med to forekomster av appen i samme nettleser.
- Ikke dekket automatisk: dra i start/stopp-linjene og i rekkefølgelista med ekte mus (prøvd med syntetiske pekerhendelser), og hvordan ikonet ser ut i GNOME-panelet (ingen `.desktop`-fil ennå).

## Verifisert i bygg 6 (del 1)

- `npm run check` (46 filer), `npm test` (83 tester, ca. 30 s) og `npm run contrast` (144 kombinasjoner, dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Røyktesten kjører alle ni sider i lyst og mørkt ved 1440, 1100 og 360 px og feiler på synlige flater som er 0 × 0. Egen test for Lys-forhåndsvisningen ved fem vindusstørrelser. Begge feiler mot v0.0.5 og passerer nå.
- Hvert funn er sett i headless Chrome før (kopi av v0.0.5) og etter, i lyst og mørkt ved 1440×900 og ved 1100×800, 1920×1080 og 360 px. Frakoblet er prøvd ved å stoppe testserveren med siden åpen, og live-modus med den falske `nvidia-smi`.
- Ikke dekket automatisk: utseende utover 0 × 0 og vannrett rulling, dra i viftekurven med mus (piltastene er prøvd), profilmenyen med tastatur.

## Verifisert i bygg 5

- `npm run check` (46 filer, også `test/`), `npm test` (80 tester i sju filer, ca. 20 s, kjørt to ganger på rad uten feil) og `npm run contrast` (144 kombinasjoner, dårligste tekst 4,51:1).
- Testene starter egne servere på ledige tilfeldige porter med `AG_SIMULATE=1` eller den falske `nvidia-smi`, og midlertidige datamapper. Port 4318, det ekte kortet og `.data/` røres ikke. Ingen prosesser blir hengende igjen etter kjøringen.
- Dekket: tolking av `--query-gpu` og `-q` (med N/A, tomme og avkortede linjer), avvisning av set-flagg og frosne argumentlister, lagring og migrering, Host/Origin/sti-traversering, alle API-kall og `simulated: true`, ugyldig og for stor kropp, utseendet i `<html>`, SSE med flere lyttere og stopp av kilden, live-modus via falsk `nvidia-smi`, og alle ni sider i lyst og mørkt ved 1440 og 360 px i headless Chrome (titler, innhold, ingen konsollfeil, ingen vannrett rulling, ingen animasjoner med «Av»).
- Ikke dekket automatisk: utseende og layout utover vannrett rulling (ingen skjermbildesammenligning), tastatur og fokus, hover og verktøytips, interaksjon med kontrollene (dra i viftekurven, fargevelger), frakobling og gjenoppkobling i nettleseren, og `server/system.js` mot ulike maskiner (testene leser bare denne maskinens `/proc` og `/sys`).

## Verifisert i bygg 4

- `npm run check`: alle JS-filer ok.
- Egen server på port 4322 med `AG_DATA_DIR=/tmp/claude-1000/agentic-rigg-design`, både live (RTX 2080, driver 610) og `AG_SIMULATE=1`.
- Headless Chrome: alle ni sider i 1920×1080 og 1280×800, lyst og mørkt, pluss kompakt tetthet med lilla aksent, luftig tetthet med grønn aksent og større skrift, smalt vindu (840×800), popover, hovedmeny, Om, Tastatursnarveier, fokusring etter Tab, Lys med Temperatur-effekt og redusert bevegelse, og Bruk/profil/tilbakestill på Kontroller i simulert modus. Ingen konsollfeil og ingen vannrett rulling.
- Oversikt, Grafer, Kontroller, Viftekurve og Lys fyller høyden uten rulling på både 1920×1080 og 1280×800. Sensorer, Grafikkort og System ruller på 1280×800, som kritikken tillater.
- Kontrast regnet ut for alle tekst-tokens og aksentfarger: dårligste par er 4,5:1.

Runden med kanttilfeller og temaer (egen server på port 4323, `AG_DATA_DIR=/tmp/claude-1000/agentic-rigg-edge`):

- `npm run check` (38 filer), `npm test` (15 tester av lagring, migrering og import) og `npm run contrast` (144 kombinasjoner, dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Alle ni sider i 360×800, 480×700, 700×600, 860×600, 1000×700, 1280×800 med zoom 50/150/200 %, største skrift med kompakt og luftig tetthet, 2560×1440 og 3840×2160: ingen vannrett rulling og ingen konsollfeil.
- Falsk `nvidia-smi` med N/A, ekstreme verdier, nuller, langt navn, prosess som dør og prosess som henger. Serveren stoppet og startet tre ganger med siden åpen: samme antall lyttere, én strøm, brudd i grafene der data mangler. To faner, blokkert `localStorage`, Tab-gjennomgang, popovere med Escape/klikk utenfor/fokus ut og 1/128 tråder.

Runden med bevegelse og detaljer (egen server på port 4324, `AG_DATA_DIR=/tmp/claude-1000/agentic-rigg-motion`):

- `npm run check` (38 filer), `npm test` (15 tester) og `npm run contrast` (144 kombinasjoner, dårligste tekst 4,51:1).
- Headless Chrome via CDP: alle ni sider i lyst og mørkt, bilder midt i overganger (avspilling 0,1×), skriptid per bilde, layout under hver animasjon, layoutskift ved lasting og i ro, Tab-gjennomgang med fokusring på hver side, hover på alle klikkbare elementer, frakobling med egen server som stoppes og startes, og `document.getAnimations()` per bevegelsesnivå (0 med «Av»). Tallene står i `docs/PATCHLOG.md`. Ingen konsollfeil og ingen vannrett rulling (360, 840, 1280, 1440 og 1920 px).

## Verifisert i bygg 15

- `npm run check` (73 filer), `npm test` (165 tester, ca. 90 s) og `npm run contrast` (96 kombinasjoner; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1, som før, nå også med søketreff).
- Ny røyktest for Innstillinger (`test/ui/settings.test.js`): skjult sidefelt, tilbake på fire måter med rulling og fokus, omlasting med og uten historikk, 360 px og søket.
- Sett i lyst og mørkt, alle fire fargetemaer og seks aksentfarger, høy kontrast, 1440 × 900, 1920 × 1080, 1100 × 800, 360 px, 200 % zoom, kompakt og skrift 116 %, med tastatur og fokusring (`docs/skjermbilder/v0.0.15/`).
- Bare egne testservere. Port 4318 er ikke rørt.

## Verifisert i bygg 14

- `npm run check` (72 filer), `npm test` (162 tester, ca. 90 s) og `npm run contrast` (96 kombinasjoner; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1, som før).
- Ny røyktest for sidefeltet mot ny server og mot en server som ignorerer, avviser eller ikke svarer på `sideCollapsed`. Den feiler på koden fra bygg 13.
- Tallene i ikonlista er sett i alle fargetemaer og aksenter, i høy kontrast, med 1–3 siffer og i lave vinduer (`docs/skjermbilder/v0.0.14/`).
- Bare egne testservere. Port 4318 er bare lest med GET.

## Verifisert i bygg 12

- `npm run check` (65 filer), `npm test` (160 tester, ca. 45 s) og `npm run contrast` (96 kombinasjoner; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1, som før).
- Ny røyktest (`test/ui/easter.test.js`): alle ti eggene ved 1440 og 360 px, bryteren, «Av» uten animasjoner og ingen utløsning ved et uhell. Tilstanden (innstillinger, utkast, brukt, lys, profiler, ventende endring) er lik før og etter, i nettleseren og på serveren.
- Skjermbilder i headless Chrome i lyst og mørkt ved 1440 × 900, pluss 360 px, redusert bevegelse og høy kontrast (`docs/skjermbilder/v0.0.12/`).
- Alle servere og sockets var egne (tilfeldige porter, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## Verifisert i bygg 11

- `npm run check` (63 filer), `npm test` (155 tester, ca. 45 s) og `npm run contrast` (96 kombinasjoner med seriepaletten; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Historikken: enhetstester for bøtter, nedsampling, grenser, lagring og ødelagt fil, og en integrasjonstest der `history.json` overlever en omstart av serveren. Verste tilfelle for fila er målt til 14,6 MiB (grensen er 16 MiB).
- Headless Chrome med syntetiske 48 t: Grafer ved 1 min, 6 t og 48 t, Oversikt ved 1 min og 24 t, Kontroller ved 6 t, grafen på Sensorer ved 48 t, pause og markør, høy kontrast og 360 px, før og etter.
- Alle servere og sockets var egne (tilfeldige porter, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## Verifisert i bygg 10

- `npm run check` (58 filer), `npm test` (141 tester, ca. 42 s) og `npm run contrast` (144 kombinasjoner; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Headless Chrome: full bredde og ikonlist i lyst og mørkt ved 1440 × 900 og 1100 × 800 (alle ni sider uten vannrett rulling), 360 px, 1440 × 700, 200 % zoom og høy kontrast. Kortvelgeren, snarveisdialogen, Innstillinger › Om, Ctrl+,, Ctrl+? og Ctrl+B.
- Alle servere og sockets var egne (tilfeldige porter, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## Verifisert i bygg 9

- `npm run check` (58 filer), `npm test` (139 tester, ca. 38 s) og `npm run contrast` (144 kombinasjoner, nå med panelstripen som flate; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Headless Chrome før og etter: alle ni sider i lyst og mørkt ved 1440 × 900, fem sider ved 360 px, og 1100 × 800, 200 % zoom, høy kontrast, kompakt og luftig tetthet, Fjord og Sand og største skrift. Bildene ligger i `docs/skjermbilder/v0.0.9/`.
- Ny røyktest for Sensorer (én tabell, grupper, sammenlegging, graf for valgt sensor, piltaster, Enter og Esc). Tastatur på Sensorer også prøvd med ekte tastetrykk og synlig fokusring.
- Alle servere og sockets var egne (tilfeldige porter, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## Verifisert i bygg 8

- `npm run check` (58 filer), `npm test` (138 tester, ca. 38 s) og `npm run contrast` (144 kombinasjoner; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Headless Chrome før og etter hvert funn i `docs/finpuss-v0.0.8.md`, i lyst og mørkt ved 1440 × 900, 1100 × 800, 1920 × 1080 og 360 px, i høy kontrast og ved 200 % zoom. Alle ni sider etterpå i lyst og mørkt.
- Røyktesten dekker nå tastatur og fokus (Tab-rekkefølge på Kontroller og Viftekurve, synlig fokusring, Esc og fokus tilbake) og to faner som trykker Bruk samtidig. Integrasjons- og kontrakttestene dekker samtidige Bruk fra to faner og to klienter.
- Regnbue og fargesyklus er målt før og etter ved 60 Hz og ubegrenset bildetakt (tallene i `PATCHLOG.md`).
- Alle servere, mocker og sockets var egne (port 4391 og tilfeldige porter, sockets under `/tmp`). Port 4318 og brukerens socket er ikke rørt.

## Verifisert i bygg 7

- `npm run check` (58 filer, også `daemon-mock/`), `npm test` (134 tester, ca. 36 s) og `npm run contrast` (144 kombinasjoner, nå også linjene under toppfeltet; dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).
- Kontrakttester mot en ekte mock over socket for hver operasjon og feilkode, frist, frakobling, nødregel, avslutning (også SIGKILL), ugyldig JSON, for store meldinger og rettighetene på mappe og socket. Integrasjonstester server ↔ mock, også med tjenesten borte og serveren som dør midt i fristen.
- Headless Chrome: bekreftelsen i lyst og mørkt, nedtelling, tilbakerullet, klemt verdi, tjenesten frakoblet, nødregel, 360 px og alle ni sider etterpå. Fokus, skjermlesermeldinger og Ctrl+Shift+Enter er sjekket i røyktesten, og «Av» gir 0 animasjoner også med bekreftelsen.
- Alle servere, mocker og sockets i testene og skjermbildene var egne (tilfeldige porter, sockets i `/tmp/ag-sock-*`). Port 4318 og brukerens datamappe er ikke rørt.
- Ikke dekket automatisk: ekte skjermleser (bare innholdet i `#live` er sjekket) og dra i kurven under nedtelling med ekte mus. Flere faner er dekket fra bygg 8.

## Kjente svakheter

- Bildefrekvensen er ikke målt på 240 Hz-skjermen. Headless Chrome er låst til 60 Hz og har ingen GPU, så målingene er øvre grenser for hovedtråden. Arbeidet per sekund er likevel gjort uavhengig av skjermfrekvensen (grafer etter piksel, løkka sover i ro, vifter på kompositoren).
- Regnbue og fargesyklus på Lys skriver fargene på 56 lysdioder hver gang fasen har flyttet seg 1,5° (100 ganger i sekundet ved fart 5). Ved 60 Hz er det fortsatt hvert bilde (ca. 0,15 ms skript); gevinsten kommer ved høyere skjermfrekvens.
- Driver 610 oppgir ikke standard-/boostklokker (applications clocks er avviklet). Boost vises derfor bare som referanseverdi fra tabellen, og fyllratene bruker referanse-boost, ikke kortets faktiske boost.
- nvidia-smi oppgir én samlet viftefart og ikke antall vifter eller RPM. Research med NVML målte to vifter, men appen leser ikke NVML direkte.
- Spesifikasjonstabellen har bare TU104/RTX 2080 (`10de:1e87` og `10de:1e82`). Andre kort får «ingen oppføring».
- Økttypen leses fra serverens miljø. Startes serveren fra en tjeneste uten `XDG_SESSION_TYPE`, står den som ukjent.
- `ucsi_source_psy_8_00081` er USB-C-kontrolleren på skjermkortet. Den leses bare gjennom kjernens hwmon-filer, aldri direkte over I2C.
- Serveren er én klient for bakgrunnstjenesten. Alle faner deler derfor samme ventende endring, og alle kan trykke Behold eller Rull tilbake nå. Serveren holder selv styr på hvilken fane som trykket Bruk, og bare den kan bruke mer før endringen er avgjort. Den ekte Electron-appen blir selv klienten.
- Fristen telles i nettleseren fra `remainingMs` i svaret. Henger fanen (f.eks. i bakgrunnen), kan tallet ligge et sekund etter; tjenesten ruller uansett tilbake til riktig tid.
- Klienten bruker mottakstid for GPU-prøvene. Tidsaksen kan hoppe litt hvis fanen har vært i bakgrunnen lenge.
- Snitt på Sensorer gjelder bare økten (min og maks huskes). Fanen sender min/maks høyst hvert 10. s, så de siste sekundene før en krasj i nettleseren kan mangle.
- Historikken til grafene (opptil 48 t) samles på serveren mens kilden kjører, det vil si mens en fane er åpen (pluss 10 s). Med appen lukket blir det hull i grafene. Opptak i bakgrunnen krever din godkjenning (se `docs/VEIKART.md`).
- Festede sensorer som forsvinner (f.eks. et nettkort som tas ut), står igjen i statusblokken med «–» til du fjerner dem.
- De automatiske testene (`npm test`) dekker server, API, en røyktest av sidene og tastatur og fokus på Kontroller og Viftekurve, men ikke utseende i detalj, hover eller dra-og-slipp. Det må fortsatt sjekkes med skjermbilder.
- Bryterens spor, glidernes spor og stolpenes bakgrunn er pynt i vanlig kontrast. Bryteren har kant og fyllet har 3:1 mot sporet; i høy kontrast får alle kant.
- GNOME-aksentfargen følges ikke (krever et nytt program eller D-Bus). Se `docs/edge-cases.md`.

## Ikke laget

- Ekte styring av noe som helst: effektgrense, klokker, vifte eller lys. Kontrakten og mocken er klare, men den ekte daemonen (Rust, root, NVML, polkit, systemd) krever din godkjenning, trinn for trinn (`docs/VEIKART.md`).
- Bakgrunnstjeneste med root for RAM-moduler, timinger, SPD-temperatur, RAPL-effekt og hovedkortsensorer (nct6775).
- Låst minneklokke og spenningsoffset. Det går ikke på Turing under Linux (research), så det vises ikke.
- Flere skjermkort. Bare kort 0 leses.
- Varsler (f.eks. ved høy temperatur), logging til fil, eksport utover «Kopier som tekst».
- Tilgangskontroll i mocken utover mappe- og filrettigheter (ingen `SO_PEERCRED` eller polkit; Node har ikke `SO_PEERCRED` uten tillegg).
- Electron/skrivebordsapp, autostart og systemstatusfelt.
