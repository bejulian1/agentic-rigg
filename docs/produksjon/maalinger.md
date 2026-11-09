# Agentic Rigg 1.0 · målinger

*Hver runde fører tallene sine her, med dato og bygg. Kravene står i `produktspesifikasjonen.md` kap. 9. Rådataene ligger i `maal/`.*

## Runde 1 · grunnmuren (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** `app/scripts/maal.sh` med release-bygget (`lto`, `codegen-units = 1`, `strip`). Alt kjører i en privat, hodeløs GNOME-økt (`app/scripts/okt.sh`: egen `gnome-shell --headless` 50.1 med virtuell skjerm 1920 × 1080, egen HOME, egne XDG-mapper, egen økt- og systembuss), så brukerens skrivebord ikke er rørt. Bare prosesser skriptet startet selv, er målt. Tjenesten leser det ekte kortet (RTX 2080, driver 610.43.02, NVML, bare lesing). Minne er fra `/proc/<pid>/smaps_rollup`, CPU er `utime + stime` fra `/proc/<pid>/stat` over 60 s.

**Viktig forbehold.** Den hodeløse Shell tegner selv på skjermkortet, så kortet står tidvis i P0/P2 under målingene, ikke i P8 som på et skrivebord i ro. NVML-kallene er da billigere enn i P8 (maskinvare-revisjon kap. 1), men vindusflatene går gjennom en annen kompositor enn brukerens. Tallene for første bilde bør måles på nytt i brukerens ekte økt etter installasjon (runde 5).

### Kald start til første bilde (tjenesten kjører)

Fra `exec` til første `after-paint` på frame clock (`RIGG_TIMING=1`). Første start i hver serie er med kald diskbuffer og er holdt utenfor medianen.

| Tegner | Median | Verste (varm) | Første (kald) | Privat minne etter 10 s | Krav |
|---|---|---|---|---|---|
| **Vulkan** (GTK 4.22 velger den selv) | **410 ms** | 430 ms | 1 220 ms | 48–62 MB (PSS 76–88, RSS 187–199) | ≤ 250 ms, ≤ 70 MB |
| GL (`GSK_RENDERER=ngl`) | 280 ms | 280 ms | 1 070 ms | 40 MB (PSS 57, RSS 157) | |
| Programvare (`GSK_RENDERER=cairo`) | **160 ms** | 170 ms | 160 ms | **10 MB** (PSS 17, RSS 53) | |

Tiden i `main()` er 20–30 ms kortere enn fra `exec` (dynamisk lenking og GResource). Skallet og Oversikt er bygd på under 30 ms; resten er tegneren som starter (Vulkan- eller GL-kontekst på NVIDIA), slik arkitektur.md kap. 2.2 forutså (117 ms for en EGL-kontekst).

**Konklusjon for 3A:** med standardtegneren (Vulkan) er kravet på 250 ms ikke nådd. Cairo gir 160 ms og en sjettedel av minnet, GL 280 ms. Valget av tegner er 3A sin oppgave (arkitektur.md 3.3, punkt 6); tallene her peker mot `cairo` som standard for vinduet, med GL bare for 3D-kortet på Lys (GtkGLArea virker uavhengig av GSK-tegneren). Det må måles at grafene (2A) tegnes billig nok med cairo.

### Minne

| Prosess | Rss | Pss | Privat (Private_Dirty) | Krav |
|---|---|---|---|---|
| Tjenesten alene (NVML åpen, ingen historikk fra før) | 31 MB | 25 MB | **20 MB** | ≤ 45 MB |
| Tjenesten med vindu tilkoblet | 32 MB | 26 MB | 21 MB | |
| Tjenesten etter 4 min | 32 MB | 27 MB | 21 MB | |
| Vinduet på Oversikt, 10 s (Vulkan) | 187–199 MB | 76–88 MB | 48–62 MB | ≤ 70 MB |

Tjenestens historikk var tom ved start. Full historikk (48 t, 96 serier) legger til høyst 6,8 MB (beregnet, `rigg-core/src/history.rs`).

### CPU i ro

| Tilstand | CPU (% av én kjerne) | Krav |
|---|---|---|
| Tjenesten alene, «Fortsett i bakgrunnen» på, 2 s-takt | **1,35 %** | ≤ 1,2 % (mål 1 %) |
| Tjenesten med vinduet åpent (1 s-takt) | **2,22 %** | ≤ 2,5 % |
| Tjenesten etter at vinduet er lukket (2 s igjen) | 1,33 % | ≤ 1,2 % |
| Vinduet på Oversikt (ingen animasjon) | **0,78 %** | ≤ 2 % (mål 1 %) |

**Bakgrunnen er 0,15 prosentpoeng over kravet.** Med 2 s-takt er hver runde også en «hvert 2. s»-runde, så vifte 1, turtallene, SM- og videoklokke og koder/dekoder leses hver gang (prøveplanen i maskinvare-revisjon kap. 12). 3A kan la bakgrunnstakten lese bare den slanke runden og ta de trege verdiene hvert 4.–10. s når ingen vindu er tilkoblet. En slank runde koster ca. 4,5 ms i P0 og 15 ms i P8.

### Andre tall

| Hva | Tall |
|---|---|
| NVML: `nvmlInit_v2` + oppregning (`rigg-hw/tests/ekte.rs`, bare lesing) | 28 ms |
| Første runde med alle «hvert 2. s»-verdiene | 40 ms (P8) |
| Slank runde | 13 ms (P8) |
| Systemprøve (`/proc` og `/sys`, `pread`) | 0,54 ms |
| Effekt fra energitelleren i ro | 11,7–11,9 W (P8) |
| Binærene | `agentic-rigg` 1,8 MB, `agentic-riggd` 1,2 MB |
| Testene (`cargo test --workspace`) | 65 tester, kontrakttestene 3,3 s |

## Runde 2B · logging, rapport og varsler (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Release-bygget av `agentic-riggd` mot det ekte kortet (NVML, bare lesing) i en privat, hodeløs økt (`app/scripts/okt.sh`) med «Fortsett i bakgrunnen» på og 2 s-takt, som i runde 1. Fire tilstander à 60 s etter hverandre i samme prosess: A og B uten klient, C og D med en klient som abonnerer på telemetri hvert 1. s (som vinduet). Loggen er «alle sensorer», 1 s, standard CSV, til en mappe i økta. CPU er `utime + stime` fra `/proc/<pid>/stat`, minne fra `smaps_rollup`. Skriptet og rådataene ligger i `maal/runde2b-maal.sh` og `maal/runde2b-logging.txt`.

| Tilstand | CPU (% av én kjerne) | Privat minne (Private_Dirty) | Pss |
|---|---|---|---|
| A · bakgrunn 2 s, ingen logg | 1,48 % | 21,1 MB | 26,8 MB |
| B · bakgrunn 2 s, logg på | 1,33 % | 21,3 MB | 24,9 MB |
| C · klient 1 s, logg på | 1,98 % | 21,5 MB | 25,0 MB |
| D · klient 1 s, ingen logg | 1,92 % | 21,5 MB | 25,0 MB |

- **Loggen koster ikke målbart.** Forskjellen mellom med og uten logg er innenfor støyen mellom to minutter (± 0,15 prosentpoeng); en rad med 68 kolonner er én formatering og én kopi inn i en buffer på 64 KiB som tømmes hvert 5. s. Minnet øker med 0,2–0,4 MB (bufferen og kolonnene). Bakgrunnstallet er fortsatt over kravet på 1,2 % av samme grunn som i runde 1 (de trege NVML-verdiene hvert 2. s), ikke på grunn av loggen.
- **Fila:** 68 kolonner og ca. 345 byte per rad med alle sensorene. Med 1 s blir det ca. 1,2 MB i timen og 30 MB i døgnet, så ny fil (`…-del2.csv`) kommer etter ca. 3,5 døgn.
- **Takten:** uten vindu går tjenesten i bakgrunnstakten (2 s), så en logg med 1 s får en rad hvert 2. s når vinduet er lukket (32 rader første minutt i B, deretter 1 per sekund i C). Det er et ønske til hovedøkta i `LOGG.md`.
- **Varslene:** testet i den private økta mot den simulerte backenden og ekte CPU-temperatur (rød grense satt til 41 °C): `Notify` gikk til `org.freedesktop.Notifications` etter 10 s med «CPU-temperaturen er 44 °C» / «Over den røde grensen på 41 °C siden 12:36.», og `app.vis-side("sensorer")` over D-Bus åpnet vinduet på Sensorer.
- **Testene:** `cargo test --workspace` passerer; 2B la til 6 tester i `rigg-core/tests/rapport.rs`, 5 i `riggd/tests/logg.rs` (tar ca. 20 s, fordi scenarioet «borte» først mister kortet etter 15 s), og enhetstester i `csv.rs`, `specs.rs`, `report/*.rs`, `logg.rs`, `varsler.rs`, `logg_ui.rs` og lesekodene i `rigg-hw` (cpuinfo, dmi, os, pci, nvml_info).

## Runde 2C · Lys i 3D, Kontroller og Viftekurve (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Release-bygget (`lto`, `codegen-units = 1`) i en privat, hodeløs GNOME-økt (`scripts/okt.sh` med egen mappe `RIGG_OKT_DIR=~/.cache/rigg-build/okt-2c`, 1920 × 1080, vinduet 1440 × 900, mørkt tema), mot `agentic-riggd --backend sim --syntetisk-historikk`. Planen ligger i `crates/rigg/src/pages/styring_bilder.rs` (`RIGG_BILDER_2C=<mappe> RIGG_BARE_MAAL=1`, bare med `RIGG_PROFIL=dev`). Bilder per sekund og CPU-tid i `render` telles i appen; vinduets CPU er `utime + stime` fra `/proc/self/stat` over målevinduet; GPU-bruk er `nvidia-smi pmon -s u` for vinduets PID (bare spørring), og kortets effekt fra `nvidia-smi --query-gpu` hvert sekund. Skjermen i økta er 60 Hz.

### 3D-kortet (GtkGLArea, GLES 3.2 på RTX 2080, 4 × MSAA)

| Tilstand | Vulkan (GTKs standard) | GL (`ngl`) | Programvare (`cairo`) |
|---|---|---|---|
| Regnbue og vifter (10 s) | **59,6 bilder/s**, vinduet 48,1 % CPU | **59,7 bilder/s**, 15,4 % | **59,3 bilder/s**, 62,9 % |
| Dreier kameraet hele tiden (10 s) | 60,0 bilder/s, 46,1 % | 59,5 bilder/s, 15,6 % | 58,9 bilder/s, 67,4 % |
| Statisk effekt, viftene snurrer (5 s) | 56,2 bilder/s, 44,4 % | 58,2 bilder/s, 14,6 % | 57,4 bilder/s, 61,2 % |
| I ro (redusert bevegelse, 5 s) | 1 bilde, 0,8 % | 1 bilde, 0,6 % | 1 bilde, 1,4 % |
| Kontroller i ro (Lys skjult, 5 s) | **0 bilder**, 0,8 % | 0 bilder, 0,8 % | 0 bilder, 1,0 % |
| GPU-bruk for vinduet ved 60 bilder/s (`pmon` sm) | 36–37 % | 22–24 % | 30–32 % |
| Kortets effekt ved 60 bilder/s (i ro: 11,5–12 W) | 17–18 W, P8 | 16–17 W, P8 | 16–17 W, P8 |

- **CPU-tid i vår egen tegning:** 0,29–0,45 ms per bilde (rammedata og GL-kall). Resten av vinduets CPU er GTK som setter sammen bildet. **Helt bilde ventet ferdig på GPU-en** (60 bilder, `glFinish` etter hvert): **1,8–2,0 ms**.
- **Ingen bilder i ro.** Lys tegner bare ved input, når lag eller kamera beveger seg, når effekter går og når viftene snurrer (som demoen). Ved «Redusert» og «Av» står vifter og effekter stille, og løkka sover (det ene bildet er bryteren for bevegelse). Når Lys er skjult, tegnes ingenting; GL-ressursene frigjøres etter 30 s.
- **Høyst ca. 60 bilder/s** for løpende animasjon, også på 240 Hz-skjermen (minst 14 ms mellom bildene); input tegnes straks. Grensen var først 15,5 ms som i demoen, og ga 52 bilder/s med Vulkan fordi frame clock på 60 Hz går litt ujevnt; med 14 ms ble det 56–60. Vulkan-kolonnen er målt med 14 ms, de to andre med 15,5 ms. Når bare viftene går (statisk effekt), kommer 56–58 bilder/s; tegningen tar 0,3 ms og GPU-en 2 ms, så det er ikke tegningen som er for treg.
- **Oppstart av 3D:** geometri og teksturer lages i en egen tråd (0,5–0,8 s første gang, 0,2–0,25 s når Lys åpnes igjen), opplasting 65–180 ms, første 3D-bilde 0,3–0,9 s etter at visningen ble laget. Vinduet fryser ikke imens («Bygger 3D-kortet …»).
- **GL under `GSK_RENDERER=cairo` virker i GTK 4.22.** GtkGLArea lager sin egen GL-kontekst fra flaten uansett hvilken tegner vinduet har, og cairo-tegneren laster bildet ned fra GPU-en hvert bilde. Det koster: vinduet bruker 61–67 % av én kjerne ved 60 bilder/s mot 15 % med `ngl`. Vulkan importerer GL-bildet, også det dyrere enn `ngl` (44–49 %). **Til runde 3:** cairo som standard for vinduet og GL bare for 3D-kortet er mulig, men Lys i 3D blir da dyrt mens det snurrer. Alternativer: `ngl` for hele vinduet, eller bytte tegner bare mens Lys vises (krever nytt vindu i GTK 4, så det er neppe verdt det).

### Det flate kortet (2D)

| Viftene snurrer, 60 bilder/s | Vulkan | GL (`ngl`) | cairo |
|---|---|---|---|
| Vinduets CPU | **5,4 %** | **6,0 %** | 40,5 % |
| Før optimalisering (viftehjulene tegnet som stier hvert bilde) | 54,3 % | 40,0 % | 53,4 % |

Kortet og viftehjulene tegnes nå én gang med cairo til teksturer per størrelse, og hvert bilde dreier bare teksturene og fargelegger de 56 lysdiodene. Med cairo-tegneren må hele scenen fortsatt rastreres på CPU hvert bilde.

### Kontroller og Viftekurve

- **Kontroller i ro:** 0 bilder fra 3D, vinduet 0,8–1,0 % CPU med 1 s-takt (to grafer for siste minutt og verdiradene).
- **Viftekurve:** punktskyen for 48 t er 1 400–1 450 punkter (`get_history`, 2 000 punkter, 48 t) tegnet som én sti; den hentes når siden vises og hvert 60. s mens den vises, ikke ved hver prøve.

## Runde 2A · Oversikt, Sensorer og Grafer (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Release-bygget i en privat, hodeløs GNOME-økt (`scripts/okt.sh` med egen `RIGG_OKT_DIR`, 1920 × 1080). Tjenesten leste det ekte kortet (NVML, bare lesing, 1 s-takt) og hadde gått i 70 s, så sparklinjene og 1 min-grafene var fulle. Vinduet startet på siden med `startPage` i `ui.json`, sto 10 s, og CPU er `utime + stime` fra `/proc/<pid>/stat` over 60 s uten at noe ble rørt. Bevegelse «System» (full i økta). En annen agent kjørte sin egen økt samtidig, så tallene kan variere med ± 0,3 prosentpoeng.

| Side og tidsvindu | Cairo (`GSK_RENDERER=cairo`) | Standard (Vulkan) | Krav |
|---|---|---|---|
| Oversikt, grafene på 1 min | **2,28 %** | **1,80 %** | ≤ 2 % (mål 1 %) |
| Oversikt, grafene på 24 t | 1,52 % | 0,80 % | |
| Sensorer (tabellen åpen, 60 s-sparklinjer) | 2,13 % | 1,15 % | |
| Grafer, 6 grafer på 1 min | 1,40 % | 2,10 % | |
| Grafer, 6 grafer på 48 t | 0,60 % | 0,60 % | |
| Privat minne (Private_Dirty), samme sider | 11–14 MB | 53–89 MB | ≤ 70 MB med GL |

Tjenesten brukte 1,9–2,2 % med vinduet åpent i de samme målingene (kravet er ≤ 2,5 %).

**Hva tegningen koster** (`RIGG_2A_PROFIL=1` skriver det til stderr hvert 10. s, cairo):

| | Før | Etter |
|---|---|---|
| `snapshot()` for én graf | 0,05–0,07 ms | uendret |
| Grafer 1 min: tegninger per sekund (6 grafer) | 133 (hver piksel, 22 per graf) | 15, i 4 bilder per sekund |
| Oversikt 1 min: bilder per sekund | 5,5 (grafene i hvert sitt bilde) | 3,5 |
| Bilde med rulling (Oversikt, cairo) | 3,9 ms | 5,6 ms (tre grafer i samme bilde) |
| Bildet hvert sekund på Oversikt (tall, fliser, målere) | 12,5 ms | 13,2 ms |
| Grafer 48 t | 1,57 % CPU (grafene tegnet på nytt hvert sekund) | 0,60 % (høyst hvert 5. s, og når tidsaksen går en piksel) |

**Det som er gjort:** (1) Grafene vekkes på et felles rutenett i tid, høyst hvert 400. ms, og står alltid på hele piksler; på 1 min går de 2–3 piksler om gangen i stedet for å tegne 20 ganger i sekundet. Grafene på siden tegnes i samme bilde. Med «Redusert» og «Av» flytter de seg bare med nye prøver. (2) Lange vinduer tegnes på nytt høyst hvert 5. s for nye prøver (en feil tegnet dem hvert sekund). (3) Stolpene og målerne glir bare ved store endringer (over 3 %); klokka som går 300 → 315 MHz hopper rett dit, i stedet for ti bilder. (4) Tidtakeren står når siden er skjult (`unmap`), på pause og når vinduet er minimert.

**Det som gjenstår (3A):** `snapshot()` er billig; kostnaden er GTK som tegner om. Med cairo koster bildet hvert sekund på Oversikt ca. 13 ms (tallene, flisene, målerne og statusblokken skifter, og en stor del av vinduet tegnes om), og det er hovedgrunnen til at Oversikt med cairo er 0,3 prosentpoeng over kravet. Med Vulkan er Oversikt under kravet. Verdt å sjekke med `GTK_DEBUG=…`/sysprof hvilke flater som skades hvert sekund før tegneren velges.

## Runde 3B · Bakgrunn og systemfeltet (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Release-bygget av `agentic-riggd` i en privat, hodeløs GNOME-økt: en kopi av `scripts/okt.sh` med `--mode=ubuntu`, så Ubuntu AppIndicator-utvidelsen, Yaru og dokken var på. Økta hadde egen HOME, egne XDG-mapper, egen sesjonsbuss og tom systembuss. Kortet var det ekte (NVML, bare lesing) og sto i P8 (29–30 °C, ca. 12 W). «Fortsett i bakgrunnen» var på, og ingen vinduer var åpne. Hver måling startet en ny tjeneste, ventet 20 s og målte `utime + stime` fra `/proc/<pid>/stat` over 120 s og `smaps_rollup` til slutt. Det ble målt på tre måter:

- **A:** systemfeltet koblet inn (`tray::attach`, én linje i en kopi av `main.rs`), med verten (ikonet vises).
- **A uten vert:** samme bygg, men på en egen, tom sesjonsbuss uten `StatusNotifierWatcher`.
- **B:** bygget uten systemfeltet, slik `main.rs` er nå.

Koden er arbeidsområdet slik det var kl. 14.06, altså med det 3A hadde gjort til da. Andre agenter kan ha bygd samtidig, så regn med ± 0,05 prosentpoeng. Skriptene og råtallene ligger i `docs/produksjon/verktoy/runde3b/` (`maal.sh`, `maal-resultat-*.txt`).

**Hva systemfeltet koster** (2 s-takt, to målinger hver):

| Bygg | CPU (av én kjerne) | Rss | Pss | Private_Dirty | Fildeskriptorer | Tråder |
|---|---|---|---|---|---|---|
| B, uten systemfelt | 1,40 / 1,37 % | 33,2 / 33,6 MB | 27,4 / 27,7 MB | 21,7 / 21,9 MB | 53 | 6 |
| A uten vert | 1,42 / 1,46 % | 33,2 / 33,5 MB | 27,4 / 27,7 MB | 21,6 / 22,0 MB | 54 | 6 |
| A, ikonet vises, første versjon | 1,46 / 1,43 % | 33,0 / 33,6 MB | 27,2 / 27,7 MB | 21,5 / 22,0 MB | 55 | 6 |
| **A, ikonet vises, endelig versjon** | **1,30 / 1,33 %** | 33,7 / 33,8 MB | 28,0 / 28,0 MB | 22,2 / 22,2 MB | 55 | 6 |

- **CPU:** forskjellen ligger innenfor støyen. Systemfeltet sjekker tilstanden hvert 2. s, og ellers skjer det bare noe når noe endrer seg.
- **Minne:** ikke målbart, under 0,1 MB. Minnet vokser litt i løpet av økta fordi historikken fylles, og den veksten er større enn forskjellen mellom byggene.
- **Fildeskriptorer og tråder:** to fildeskriptorer til, én for sesjonsbussen og én for ikonets egen forbindelse. Ingen ny tråd: GDBus-tråden finnes fra før, for systembussen.
- **D-Bus-trafikk mens ikonet vises** (`dbus-monitor` i 130 s): 12 `NewToolTip`, altså ca. ett hvert 10. s, og ingen andre meldinger før menyen åpnes. AppIndicator henter ikke verktøytipset.
  - Den første versjonen sendte `NewToolTip` og `ItemsPropertiesUpdated` ved hver prøve, fordi lasten endrer seg hele tiden.
  - Nå meldes verktøytipset høyst hvert 10. s, eller straks når loggen starter eller stopper. Statuslinja i menyen oppdateres når menyen åpnes (`AboutToShow`).
- **Uten vert:** journalen har bare oppstartslinja. Ingen feil, ingen navn på bussen, og ingen forbindelse utover å følge navnet `org.kde.StatusNotifierWatcher`.

**Takt i bakgrunnen** (B, uten systemfelt; A i parentes):

| Takt | CPU (av én kjerne) |
|---|---|
| 1 s | 2,08 % (A 2,12 %) |
| 2 s | 1,37–1,40 % (A 1,30–1,46 %) |
| 5 s | 0,69 % (A 0,66 %) |

**Over kravet.** Produktspesifikasjonen kap. 9 krever ≤ 1,2 % med 2 s og kortet i P8 (målet er 1 %). Målt er 1,3–1,5 %, og B er like høy, så det er prøveplanen og ikke systemfeltet. Det er sendt videre til 3A. Tallene står i Innstillinger › Generelt › Bakgrunn og må oppdateres hvis prøveplanen endres.

**Minnet i brukerflaten.** Statuslinja viser Pss (ca. 28 MB), og teksten sier «ca. 30 MB». Pss er eget minne pluss en rettferdig del av de delte bibliotekene. Private_Dirty (22 MB) er bare det tjenesten har for seg selv. Rss (34 MB) teller de delte bibliotekene fullt.

## Runde 3C · Optimalisering (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Release-bygget (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`), bygd på nytt før målingene. «Før» er bygget fra starten av runden (koden etter 3A og 3B), «etter» er koden slik den er nå. Alt kjører i private, hodeløse økter (`app/scripts/okt.sh`, ny mappe per kjøring, 1920 × 1080), med det ekte kortet (NVML, bare lesing) og tjenesten med «Fortsett i bakgrunnen» på. Hver måling er gjort minst fem ganger, og tallet er medianen. Skriptene ligger i `docs/produksjon/verktoy/runde3c/` (`kjor.sh` + ett skript per måling), og rådataene i `docs/produksjon/maal/runde3c-*.txt`.

- **Tid:** måleskriptet tar veggklokka rett før `exec`, og vinduet skriver merker med veggklokka (`RIGG_TIMING=1`, `rigg/src/timing.rs`): `main`, `command_line` (GTK er startet), `prefs`, `css`, `shell`, `page`, `realize`, `layout`, `paint`, `first_frame`, `hello`, `telemetry`, og ved lukking `close`, `hidden`, `unmap`, `exit`. `RIGG_TIMING_LUKK=MS` lukker vinduet samme vei som ×-knappen (`gtk_window_close`). Første start i hver serie (kald diskbuffer og tom NVIDIA-shaderbuffer i den nye økta) er holdt utenfor medianen.
- **CPU:** `utime + stime` fra `/proc/<pid>/stat`. Vinduet: 70 s etter at tjenesten startet (sparklinjer og 1 min-grafer fulle), 10 s etter at siden ble åpnet, så 5 × 20 s. Tjenesten alene: 20 s etter start, så 5 × 60 s.
- **Minne:** `Private_Dirty`, `Pss` og `Rss` fra `smaps_rollup`.
- **P-tilstanden** er fra `nvml_timing watch 2000` (to ganger i sekundet ville vært mer enn tjenesten selv) i egne kjøringer, fordi `watch` gjør 19 NVML-kall hvert 2. s og ville forstyrret CPU-tallet. Uten `watch` er P-tilstanden lest med `nvidia-smi` etter hver måling.
- `perf` og `sysprof` virker ikke uten root her (`perf_event_paranoid = 4`). Tida er funnet med merkene over, `strace -f -tt` og en teller for bilder (`RIGG_RAMMER=1`: bilder og tid fra «before-paint» til «after-paint» hvert 10. s).

### Hvor tida går fra exec til første bilde (før)

Median av fem varme starter, ms fra `exec` (`maal/runde3c-start-foer.txt`). Kortet sto i P8 før og etter hver serie.

| Steg | Vulkan (standard før) | GL (`ngl`) | cairo |
|---|---|---|---|
| `exec` → `main()` (lenking av 113 bibliotek, konstruktører) | 26 | 25 | 25 |
| → GTK startet (`command_line`: D-Bus, Wayland, xkb, Yaru-temaet) | 70 | 79 | 69 |
| → skall og startside bygd (`page`) | 100 | 110 | 98 |
| → vinduet realisert (tegneren startet) | 365 | 259 | 120 |
| → første layout (stil, skrift) | 404 | 296 | 155 |
| → **første bilde** | **457** | **329** | **173** |

- **Tegneren er nesten alt.** Vulkan bruker 265 ms og GL 150 ms på å realisere vinduet (laste NVIDIA-driveren, `eglInitialize`, kontekster); cairo 22 ms. Første bilde koster i tillegg 50 ms med Vulkan og 33 ms med GL (shaderne), mot 17 ms med cairo.
- **Yaru-temaet ble lest to ganger.** `theme::apply` satte `gtk-application-prefer-dark-theme` hver gang, og hver gang laster GTK hele systemtemaet på nytt (ca. 14 ms). Nå settes det bare når verdien endres. Skallet bygges på 3 ms i stedet for 17.
- **Forsøkt og forkastet:** å varme opp GL-driveren i en egen tråd (`eglInitialize` og en kontekst på EGL-enhetsplattformen mens hovedtråden starter GTK). En egen C-test viste at 77 av driverens ca. 80 ms er engangsoppsett per prosess, men i appen ble første bilde 10–30 ms *senere*: tråden og GTK slåss om lasterlåsen (`dlopen`), og første bilde ble tregere. Uten Yaru (`GTK_THEME=Empty`) sparer GL 24 ms, men er fortsatt 268 ms, og appen trenger temaet for brytere, rullefelt og avkrysning.

### Valg av tegner: cairo

| | cairo | GL (`gl`) | Vulkan |
|---|---|---|---|
| Første bilde, etter (median, verste) | **162 ms** (186) | 293 ms (301) | 446 ms (475) |
| Første start i økta (tom shaderbuffer) | 164 ms | 1 052 ms | 1 263 ms |
| Lukk → prosessen avsluttet | 10 ms | 46 ms | 77 ms |
| Privat minne, Oversikt etter 10 s | **12,6 MB** | 48,6 MB (100 MB første gang i økta) | (48–89 MB, 2A) |
| Privat minne per side i ro (etter 10 + 100 s) | 11–15 MB | 46–106 MB | – |
| CPU i ro, verste side utenom Lys | 1,65 % (Sensorer) | 1,75 % (Grafer, før) | – |
| Lys i 3D mens det animerer | 28–36 % (høyst 30 bilder/s) | 15 % (60 bilder/s) | 44–49 % (2C) |
| Lys i 2D, viftene snurrer | 19 % | 6 % | 5 % (2C) |

**Begrunnelse.** Bare cairo når kravet til første bilde (≤ 250 ms), og cairo bruker en fjerdedel til en åttendedel av minnet: GL-driveren fra NVIDIA koster 35–90 MB privat minne i prosessen så snart den er lastet, og det gis ikke tilbake før prosessen avslutter. Med de rettingene som er gjort, er CPU i ro under 2 % på alle sidene med cairo (tabellen under). Prisen er Lys mens det animerer: GL-bildet til 3D-kortet må lastes ned fra GPU-en hvert bilde med cairo, og 2D-kortet rastreres på CPU. Derfor går Lys med høyst 30 bilder/s med cairo, og koster da halvparten av før. GL tegner Lys billigere, men koster 130 ms ved hver start og minnet over hele tida. Rask gjenåpning (arkitektur.md 3.4) trengs ikke. `GSK_RENDERER=gl` (eller `vulkan`) i miljøet overstyrer valget, og Om viser tegneren.

**Forsøkt uten effekt:** `GtkGraphicsOffload` rundt GLArea-en på Lys (så GL-bildet kunne gått rett til kompositoren som egen flate) endret ingenting: 62,9 % før og etter. NVIDIA-driveren her gir ikke dmabuf fra GLArea-en. Den er beholdt, fordi den ikke koster noe og virker med andre drivere.

### Kravene i produktspesifikasjonen kap. 9, før og etter

«Før» er bygget fra starten av runden med standardtegneren den gang (Vulkan); for vinduet står også cairo og GL. «Etter» er cairo. Kortet sto i P8 i alle målingene som er oppgitt med P-tilstand.

| Krav | Krav | Før | Etter | |
|---|---|---|---|---|
| Kald start til første bilde (tjenesten kjører) | ≤ 250 ms (mål 150) | 457 ms (Vulkan) · 329 ms (GL) · 173 ms (cairo) | **162 ms** (verste 186) | ✓ |
| Kald start når tjenesten også må startes | ≤ 350 ms, tall innen 1 s | – | første bilde **164 ms**, tall etter **245 ms** | ✓ |
| Varm start (vinduet finnes) | ≤ 50 ms | – | **35 ms** (33–63; verst når Sensorer vises) | ✓ |
| Lukk: vinduet borte | ≤ 50 ms (oppgaven: < 100 ms) | 8–9 ms median, opptil 79 ms (`fsync` av `ui.json` før vinduet ble skjult) | **0,6 ms** (verste 0,7) | ✓ |
| Lukk: prosessen avsluttet | ≤ 300 ms | 86 ms (Vulkan) · 55 ms (GL) · 19 ms (cairo) | **10 ms** (verste 11) | ✓ |
| Vinduet, privat minne, alle sider besøkt | ≤ 70 MB (mål 50) | 48–89 MB (Vulkan, 2A), 144 MB med Lys i 3D (GL) | **13–15 MB** uten 3D · **136 MB** med Lys i 3D | ✗ med 3D |
| Tjenesten, privat minne med full historikk | ≤ 45 MB (mål 35) | 21 MB tom historikk | **21,3 MB** tom, + 1,3 MB med syntetisk 48 t (sim); beregnet høyst + 6,8 MB | ✓ |
| Tjenesten, CPU i bakgrunnen (2 s, P8) | ≤ 1,2 % (mål 1 %) | **1,38 %** (1,37–1,43) | **0,85 %** (0,85–0,97) | ✓ |
| Tjenesten, CPU med vinduet åpent (1 s) | ≤ 2,5 % | 1,80–2,65 % (Lys: 2,50 med cairo, 2,65 med GL) | 1,95–2,35 % (Lys: 2,55 %) | ✓ (Lys ✗ med 0,05) |
| Polling, kortet i P8 i ro | aldri vekke kortet | – | **P8 i alle prøvene**: bakgrunn 62 av 62, vinduet på hver side 15 av 15. Rettet i 4A: første gang Lys bygger 3D-kortet, går kortet til P0 i ca. 4 s (se under) | ✓ (✗ første 3D-oppbygging) |
| Vinduet, CPU i ro på Oversikt | ≤ 2 % (mål 1 %) | 1,60 % (GL) · 2,50 % (cairo) | **1,35 %** | ✓ |
| Vinduet, CPU i ro på hver side | ≤ 2 % | se under (cairo: Oversikt 2,50, Sensorer 2,30, System 4,80) | alle ≤ 1,65 % | ✓ |

### Vinduet i ro per side (5 × 20 s, median, % av én kjerne)

Tjenesten hadde gått i 70 s (fulle sparklinjer og 1 min-grafer). Vinduet 1440 × 900 på 1920 × 1080, bevegelse «System» (full), Oversikt og Kontroller med grafene på 1 min. Rådata: `maal/runde3c-cpu-foer.txt` og `-etter.txt`.

| Side | Før, GL | Før, cairo | **Etter, cairo** | Bilder per 10 s etter | Privat minne etter |
|---|---|---|---|---|---|
| Oversikt | 1,60 | 2,50 | **1,35** | 36 à 2,9 ms | 12,8 MB |
| Sensorer | 0,95 | 2,30 | **1,65** | 11 à 14,1 ms | 14,5 MB |
| Grafer (6 grafer, 1 min) | 1,75 | 1,75 | **0,95** | 26 à 3,1 ms | 12,4 MB |
| Grafikkort | 0,45 | 0,65 | **0,55** | 14 à 3,2 ms | 13,6 MB |
| System | 1,30 | 4,80 | **0,70** | 11 à 5,9 ms | 14,9 MB |
| Kontroller (to grafer) | 1,30 | 1,05 | **0,90** | 45 à 1,7 ms | 13,0 MB |
| Viftekurve | 0,30 | 0,40 | **0,30** | 10 à 2,4 ms | 11,4 MB |
| Lys (3D, regnbue og vifter, animerer) | 14,25 | 48,0 | 25,4 | ca. 30 bilder/s | 195 MB |

Det som er gjort:

- **System: 4,8 → 0,7 %.** Telleren viste 240 bilder per 10 s hele tiden: de 16 trådstolpene gled hver for seg mot ny verdi (ca. 0,3 s per glidning), så noen gled alltid. `Bar::set_still(true)` på trådstolpene: de hopper rett til ny verdi, som i HWiNFO. Én tegning i sekundet.
- **Grafer og sparklinjer med cairo direkte: Oversikt 2,2 → 1,25 %, Sensorer 2,15 → 1,65 %.** GSK-stiene (`append_fill`/`append_stroke`) ble tegnet som klippemasker av cairo-tegneren. `widgets::Painter` lager én cairo-node per graf eller sparklinje med vanlig `cairo_fill` og `cairo_stroke` når tegneren er cairo, og GSK-stier ellers. Samme utseende. Bildet hvert sekund på Oversikt gikk fra 5,7 til 2,9 ms i snitt, og på Sensorer fra 19,6 til 14,1 ms.
- **Sensorer oppdaterer bare radene som synes** i rullefeltet (resten når de rulles fram, når en gruppe åpnes eller vinduet blir større). I 1440 × 900 synes nesten alle radene, så det sparer lite her, men mye i et lite vindu med alle trådene åpne.

### Lys

| Mens Lys animerer | Før, cairo (60 bilder/s) | Etter, cairo (høyst 30 bilder/s) | GL (60 bilder/s) |
|---|---|---|---|
| 3D, statisk effekt, viftene snurrer | 62,6 % | **28,2 %** | 14,6 % |
| 3D, regnbue og vifter | 62,9 % | **36,2 %** | 14,5 % |
| 3D, kameraet dreier | 67,6 % | **32,3 %** | 15,0 % |
| 3D, redusert bevegelse (i ro) | 1,6 % | 1,4 % | 0,6 % |
| 2D, viftene snurrer | 38,4 % | **18,6 %** | 5,9 % |
| Kontroller i ro (Lys skjult) | 1,4 % | 1,0 % | 1,8 % |

- **30 bilder/s med cairo** (`pages/lys.rs`, `step`): løkka hopper over tikk som kommer under 30 ms etter forrige. Input (dra, klikk, rull) tegnes fortsatt straks.
- **Vakthunden** som sender 3D til 2D når GL er for tregt, kunne slå ut av ett enkelt hikk med 30 bilder/s (snittet over 50 ms etter én lang pause). Nå kreves tolv trege bilder på rad (`lys3d/mod.rs`, `watch_speed`).
- **Minnet:** etter opplasting slippes CPU-kopiene av teksturene (ca. 22 MB), og `malloc_trim` gir det byggetråden lot ligge igjen tilbake (`widgets::trim_memory`), også når 3D frigjøres 30 s etter at du gikk fra Lys. Heapen faller fra 70 til 46 MB. Det meste av minnet er likevel NVIDIA-driveren (`/dev/nvidiactl` 34 MB, anonymt 51 MB), og det blir liggende til vinduet lukkes: 135–145 MB etter at Lys er vist, med begge tegnerne.
- **Oppstart av 3D:** geometri og teksturer i egen tråd 0,2 s, opplasting 66–150 ms, første 3D-bilde 0,3–0,45 s etter at visningen ble laget. Det skjer bare når Lys åpnes.

### Tjenesten

| Takt i bakgrunnen | Før (3B) | Etter | Krav |
|---|---|---|---|
| 1 s | 2,08 % | **1,50 %** (1,47–1,53) | – |
| 2 s | **1,38 %** (1,37–1,43, målt nå) | **0,85 %** (0,85–0,97) | ≤ 1,2 % |
| 5 s | 0,69 % | **0,42 %** (0,40–0,47) | – |

Privat minne 21,2–21,4 MB, Pss 27 MB, Rss 33 MB, 54 fildeskriptorer, 6 tråder, likt før og etter.

- **Det trege i bakgrunnen leses sjeldnere** (`schedule::detailed`, `sampler.rs`, `Plan::watched`). Vifte 2, turtall, SM- og videoklokke, koder og dekoder leses hvert 2. s bare når et vindu som synes abonnerer på prøvene, eller en logg går. Ellers hvert 10. s. Effektgrensen og PCIe-bredden hvert 30. s i stedet for 5. Viftestyring og målfart (målfarten koster 1,6–3,2 ms per vifte i P8) leses bare med vindu. Når vinduet kommer fram, leses alt i neste runde, så tallene er ferske. Historikken får de trege seriene hvert 10. s i bakgrunnen; 48 t-grafene viser uansett minst 86 s per punkt.
- **Ingen JSON uten mottaker:** prøven og statistikken gjøres om til JSON bare når noen abonnerer (før: hver runde).
- **`malloc_trim` hvert 60. s** (etter lagringen av historikken): store svar på `get_history` når et vindu kobler til, la igjen 0,5–1,3 MB ledig heap hver gang, som aldri ble gitt tilbake (se lekkasjetesten). Kostnaden er ikke målbar: 0,85 % (0,83–0,90) med 2 s-takt, som uten.
- **Ingen allokering per runde for det som står stille:** NOT_SUPPORTED-lista og P-tilstanden som tekst lages bare når de endres. `/proc` og `/sys` ble allerede lest med `pread` på åpne filer.
- **P-tilstanden:** `nvml_timing watch 2000` ved siden av tjenesten i bakgrunnen i 125 s: P8 i alle 62 prøvene, grafikkklokke 300 MHz (315 MHz to ganger), 11–12 W (`maal/runde3c-pstate-bakgrunn.txt`). Med vinduet åpent på hver av de åtte sidene i 30 s: P8 i alle 15 prøvene per side (`maal/runde3c-pstate-sider.txt`). **Rettet i runde 4A:** prøvene ble tatt fra 10 s etter at siden var åpnet, og fikk ikke med seg at **første oppbygging av 3D-kortet på Lys vekker kortet til P0 i ca. 4 s** (1 515 MHz, 36–39 W, `docs/skjermbilder/1.0.0/runde4a/pstate-lys.txt`). Deretter P8 (315 MHz, 14 W) med 15–19 % GPU-last mens 3D animerer. Den hodeløse Shell vekker også kortet i ca. 4 s når økta starter. Fra runde 4B åpner Lys med 2D-kortet, så dette skjer bare når brukeren velger 3D.

### Lekkasjetest (1 t)

Tjenesten med det ekte kortet og «Fortsett i bakgrunnen» på (2 s-takt) i 60 min. Hvert 5. min ble vinduet åpnet i 90 s (11 ganger), på en ny side hver gang og med tre sider til mens det var åpent, og lukket samme vei som ×. Hvert 30. s: `Private_Dirty`, `Pss`, `Rss`, fildeskriptorer, tråder og P-tilstand. Kurvene er `maal/runde3c-lekkasje-1t-trim.csv` og `.svg` (endelig), `runde3c-lekkasje-1t.*` (før `malloc_trim`) og `runde3c-lekkasje-sim-full-historikk.*`, laget med `verktoy/runde3c/lekkasje.sh` og `lekkasje_svg.py`.

| Privat minne (`Private_Dirty`) | Før `malloc_trim` | **Etter** |
|---|---|---|
| Etter 5 min | 21,3 MB | **21,2 MB** |
| Etter 15 min | 22,6 MB | **22,2 MB** |
| Etter 60 min | 23,8 MB | **22,9 MB** |
| Stigning fra 20 til 60 min | 1,2 MB/t | **0,8 MB/t** |
| Fildeskriptorer og tråder | 53–54 og 6 hele tiden | 53–54 og 6 hele tiden |
| P-tilstand (`nvidia-smi` hvert 30. s) | P8, med P0/P5 mens skjermbildeserien gikk i en annen økt | **P8 i alle 120 prøvene** |

**Ingen lekkasje.** Det som vokser, er historikken, slik den skal: nivå 1 (1 s i 15 min) er fullt etter 15 min (900 rader à ca. 1,2 KB med 96 serier, ca. 1 MB), og nivå 2 (10 s i 6 t) og 3 (60 s i 48 t) vokser med 7 rader i minuttet, ca. 0,5 MB/t, til de er fulle etter 6 og 48 t (høyst 6,8 MB i alt, beregnet). De 0,8 MB/t etter 20 min er dette pluss malloc. Fildeskriptorer og tråder står stille, også når vinduet kobler til og fra.

**Funnet underveis:** uten `malloc_trim` steg minnet i trinn hver gang et vindu koblet til. Med full syntetisk historikk (sim, så historikken ikke vokser) var kurven flat når vinduet var lukket (4,02 → 4,04 MB på 4 min) og steg med 0,1–1,3 MB når det var åpent: svarene på `get_history` for 48 t-grafene lot ledig heap ligge igjen. Nå gis det tilbake hvert 60. s.

### Release-profilen

| | `opt-level = 3` (beholdt) | `opt-level = "s"` |
|---|---|---|
| `agentic-rigg` | 4,23 MB | 3,19 MB |
| `agentic-riggd` | 1,73 MB | 1,35 MB |
| Første bilde (cairo, median av 7) | 173 ms (162 ms i hovedserien) | 162 ms |

Forskjellen i første bilde er innenfor det seriene varierer med (± 10 ms). `"s"` ville spart 1 MB på disk, men optimaliserer for størrelse og ikke fart i tegningen og JSON-lesingen (ikke målt hver for seg), så `opt-level = 3` er beholdt.

- `lto = true` (fat) og `codegen-units = 1` er beholdt: minst og raskest kode, prisen er byggetid (1,5 min for vinduet).
- `strip = true` er beholdt.
- **`panic = "abort"` er ikke trygt:** prøvetråden i tjenesten fanger panikk med `catch_unwind` og starter på nytt etter 1 s, så historikk og logg ikke stopper. `panic` kan ikke settes per pakke i Cargo, så det gjelder begge binærene.
- **Avhengighetene** er allerede de minste: `gtk4` (uten libadwaita), `glow` og `glam` (3D), `serde`/`serde_json`, `libc`, `glib`/`gio`/`glib-unix`, `async-channel`. Ingen kan fjernes uten å skrive koden selv.
- `exec` → `main()` tar 25–28 ms uansett: lenkeren bruker 2–3 ms (`LD_DEBUG=statistics`), resten er å laste 113 delte bibliotek fra GTK (blant annet `libgnutls` og `libcups`) og konstruktørene deres.

### Det som fortsatt er over kravet

1. **Vinduets minne med Lys i 3D: 136 MB** (kravet er ≤ 70 MB med alle sider besøkt). Uten 3D er vinduet 13–15 MB. 3D-kortet trenger GL, og NVIDIA-driveren bruker 35–90 MB privat minne i prosessen bare ved å være lastet, pluss buffere for 3D-kortet. Det gis ikke tilbake før vinduet lukkes, med noen av tegnerne. Det som kan gis tilbake, gis tilbake: CPU-kopiene av teksturene og malloc etter opplasting og når 3D frigjøres. Det som gjenstår, er et valg for hovedøkta: la «auto» vise 2D-kortet når tegneren er cairo, og laste 3D først når du trykker 3D (og huske det). Da holder vinduet seg under 15 MB for de fleste, og 3D koster bare for dem som velger det.
2. **Lys mens det animerer: 25–36 % av én kjerne i 3D og 19 % i 2D** med cairo. Det er ikke «i ro», men det er dyrt. GL tegner Lys for 15 og 6 %, men koster 130 ms ved hver start og 35–90 MB hele tida. Med redusert bevegelse (eller «Av») står Lys stille og koster 1,4 %.
3. **Tjenesten med Lys åpent: 2,55 %** (kravet ≤ 2,5 % med vinduet åpent). Med de andre sidene 1,95–2,35 %. Mens GPU-en tegner 3D-kortet, tar NVML-kallene lenger tid, selv om kortet blir i P8.

### Skjermbildene

`scripts/bilder.sh --plan alle docs/skjermbilder/1.0.0/runde3c` med cairo som tegner: alle planene fra runde 1–3, hver i sin egen økt og undermappe. Release-bygget for alle planene unntatt 2a og 2b, som bare finnes i debug-bygget (`bilder.sh` velger det nå selv; før ble vinduet stående og vente). Den nye planen `3c` (`skjermbilder.rs`, `RIGG_PLAN=3c`, det ekte kortet) har alle åtte sidene i lyst og mørkt med de fire temaene (64), høy kontrast i lyst og mørkt (16), alle sidene i 640 og 1920 px (16) og Innstillinger › Generelt med de nye tallene. Grafene, sparklinjene, målerne og 3D-kortet ser ut som med GL. Verdt å se på for 4A: i 640 px blir titlene i flisene på Oversikt kuttet («Tempera…», «Eff… av…»).

## Runde 4B · Finpuss (7. november 2026, 1.0.0 · bygg 16)

**Slik er det målt.** Som i 3C, med de samme skriptene (`verktoy/runde3c/kjor.sh`) og release-bygget etter alle rettingene i 4B, i en privat, hodeløs økt med det ekte kortet (NVML, bare lesing) og cairo som tegner. Rådata: `maal/runde4b-start.txt`, `-cpu.txt` og `-tjeneste.txt`. **Forbehold:** brukerens egne programmer holdt skjermkortet på 19 % last (420 MHz, P8) og maskinen på last 1,3–1,7 under målingene. I 3C sto kortet i ro. NVML-kallene tar lenger tid når kortet tegner noe annet, så tjenestens tall er litt høyere enn i 3C uten at koden der er endret.

| Krav | Krav | 3C | **4B** | |
|---|---|---|---|---|
| Kald start til første bilde (tjenesten kjører) | ≤ 250 ms | 162 ms (verste 186) | **170 ms** (164–180, 11 starter) | ✓ |
| Lukk: vinduet borte | < 100 ms | 0,6 ms | **0,6 ms** (0,5–0,9) | ✓ |
| Lukk: prosessen avsluttet | ≤ 300 ms | 10 ms | **10 ms** (8–12) | ✓ |
| Vinduet, CPU i ro på Oversikt | ≤ 2 % | 1,35 % | **1,35 %** (1,25–1,75) | ✓ |
| Vinduet, CPU i ro på hver side | ≤ 2 % | ≤ 1,65 % | **≤ 1,50 %** (Lys i 2D animerer, se under) | ✓ |
| Tjenesten i bakgrunnen (2 s, P8) | ≤ 1,2 % | 0,85 % | **1,05 %** (1,05–1,10), med kortet på 19 % last fra andre programmer | ✓ |
| Tjenesten med vinduet åpent (1 s) | ≤ 2,5 % | 1,95–2,35 % | 2,60–2,85 %, samme forbehold | ✗ (målt med kortet i bruk) |

**Vinduet i ro per side** (5 × 20 s, median, % av én kjerne; privat minne i parentes):

| Side | 3C | **4B** |
|---|---|---|
| Oversikt | 1,35 | **1,35** (14,3 MB) |
| Sensorer | 1,65 | **1,50** (14,5 MB) |
| Grafer | 0,95 | **1,10** (12,4 MB) |
| Grafikkort | 0,55 | **0,55** (13,3 MB) |
| System | 0,70 | **0,85** (13,5 MB) |
| Kontroller | 0,90 | **1,10** (13,3 MB) |
| Viftekurve | 0,30 | **0,35** (11,5 MB) |
| Lys | 25,4 (3D, animerer) | **16,75** (2D, viftene går rundt; 14,2 MB) |

- **Lys åpner nå i 2D.** Vinduet er 14 MB på Lys mot 136 MB med 3D, og kortet vekkes ikke. Viftene i 2D-kortet roterer med full bevegelse, så siden er ikke «i ro»; med «Redusert» eller «Av» står den stille (1,4 % i 3C).
- **Kald start +8 ms** (median 170 mot 162 ms): `layout` til første bilde tar 6–8 ms mer (153 mot 146 ms fra exec), resten er likt. Oversikt har nå rutenettet for flisene og den tomme tilstanden for GPU-delen; maskinen var også mer belastet. Fortsatt godt under 250 ms.
- **P-tilstanden:** P8 før og etter startserien, og i alle vindusmålingene. Første oppbygging av 3D (P0 i ca. 4 s, runde 4A) skjer nå bare når brukeren velger 3D.
- **Tjenesten med vinduet åpent** ligger 0,3–0,5 prosentpoeng over 3C på alle sidene likt, også på Viftekurve og Grafikkort der vinduet ikke er endret. Det er kortets last fra andre programmer, ikke vinduet; bør måles på nytt med kortet i ro (runde 5, i brukerens økt).

## Runde 5 · Installert i brukerens økt (7. november 2026, 1.0.0 · bygg 16)

Installert med `app/install.sh` i `~/.local` etter brukerens ja. Målt i den ekte GNOME-økta (Wayland, 1920×1080 på 240 Hz) med `RIGG_TIMING=1`, release-bygget og cairo.

| Måling | Krav | Resultat |
|---|---|---|
| Kald start, tjenesten startes av vinduet | ≤ 350 ms | 188 / 191 ms fra `main()` til første bilde (220 ms fra exec) |
| Tall fra kortet i vinduet | ≤ 1 s | 293 ms |
| Lukking → vinduet borte | < 100 ms | 1,3 ms |
| Lukking → prosessen avsluttet | ≤ 300 ms | 4,1 ms |
| Tjenesten stopper uten bakgrunnsopptak | 60 s | stoppet ca. 58 s etter lukking |
| Vinduet i ro, Oversikt (urørt, 3 × 10 s) | ≤ 2 % | 0–0,2 % |
| Tjenesten med vinduet åpent | ≤ 2,5 % | 0,8–0,9 % |
| Privat minne | | vinduet 16 MB, tjenesten 27 MB |
| P-tilstand | P8 | P8 i alle prøver utenom én (P0 med 2 % last, mens brukeren klikket rundt) |

- Tjenesten med vinduet åpent ligger nå godt under kravet. De 2,6–2,85 % i 4B skyldtes kortets last fra andre programmer.
- Vinduet i ro er lavere enn i de private øktene, sannsynligvis fordi GNOME ikke tegner vinduer som er dekket.
- Mens brukeren klikket mellom Grafikkort og System var vinduet på 14–26 %. Det er bruk, ikke ro.
- Systemfeltmenyen er ikke sett i panelet ennå, fordi «Fortsett i bakgrunnen» er av.

## Runde 6B · Skallet (8. november 2026, 1.1.0 · bygg 17 under arbeid)

**Slik er det målt.** `verktoy/runde6b/start.sh` i en privat, hodeløs økt (`scripts/okt.sh`), release-bygget og cairo, tjenesten fra det nye bygget mot det ekte kortet (bare lesing). Før (1.0.0 · bygg 16, kopi av den installerte binæren) og etter startes **annenhver gang** i samme økt, 15 ganger hver, så begge får den samme lasten. **Forbehold:** maskinen hadde last 14–17 under hele målingen (brukerens egne Java-prosesser og de andre agentenes økter), mot ca. 1,5 i 4B. Tallene er derfor høyere og mer spredt enn i 4B, men før og etter kan sammenlignes. Rådata: `maal/runde6b-start.txt`.

| Måling | Krav | Før (1.0.0) | **Etter (6B)** |
|---|---|---|---|
| Kald start til første bilde, median | ≤ 250 ms | 217 ms | **199 ms** ✓ |
| Kald start, raskeste | | 162 ms | 166 ms |
| Kald start, tregeste (lastetopp) | | 389 ms | 534 ms |

- Skallet koster ikke merkbart ved start: maskinhodet bygges med plassholdere, og prosessornavnet hentes med `get_system` først når tjenesten svarer, etter første bilde. Vinduskontrollene er bare CSS.
- Et første forsøk med `scripts/maal.sh` (én binær per økt, etter først) ga median 388 mot 210 ms, men lasten steg fra ca. 5 til 21 mellom de to øktene (andre agenter bygde). Derfor annenhver gang.
- Laveste vindushøyde med utvidet sidefelt: 602 px med ti sider (før: 622 px med åtte), fordi statusblokken ikke lenger ber om høyde.

## Runde 6C · Klokker og effekt og Vifter (8. november 2026, 1.1.0 · bygg 17 under arbeid)

Målt med release-bygget og cairo i en privat, hodeløs økt (`verktoy/runde6c/bilder6c.sh`, planen `ekte` i `pages/styring_bilder.rs`). Det ekte kortet leses med NVML; ingenting skrives. Vinduet er 1440×900, med «Bevegelse: full». CPU-tida er fra `/proc/self/stat`, og antall bilder er fra vinduets `GdkFrameClock`. Rådata: `maal/runde6c-vindu-i-ro.txt`, `maal/runde6c-pstate.csv` og `maal/runde6c-steg.txt` (kjøring 2, med klokkeslett for hvert steg i planen), `maal/runde6c-2-*` (kjøring 3) og siste kjøring i `docs/skjermbilder/1.1.0/6c/ekte/`.

### Vinduet i ro (krav ≤ 2 %)

| Side | 4B (1.0.0) | 6C kjøring 1 (30 s) | kjøring 2 (60 s) | kjøring 3 (60 s) | siste bygg (60 s) | Bilder tegnet |
|---|---|---|---|---|---|---|
| Klokker og effekt (Kontroller) | 1,10 % | 0,87 % | 0,62 % | 0,60 % | **0,60 %** | 1,7 per s |
| Vifter (Viftekurve) | 0,35 % | 0,53 % | 0,73 % | 0,98 % | **0,68 %** | 2,2 per s |
| Klokker og effekt med utkast (20 s) | – | 0,95 % | 0,85 % | 0,90 % | **1,20 %** | 2,8 per s |

- Ingenting tegnes hele tiden. Bildene i ro er grafene, som flytter seg én gang per prøve (hvert sekund). Glidere, målere og kurven tegnes bare når en verdi endres; `Meter` runder til 1/400, så støy under en piksel ikke gir et nytt bilde.
- Utkastet koster litt mer fordi utkastlista, toppfeltet og profilene bygges på nytt når noe endres; i ro er det bare grafene.
- Vifter har nå to grafer for de siste 5 minuttene (temperatur og viftene) i tillegg til kurven, derfor er den høyere enn i 4B. Klokker og effekt har tre grafer (kjerneklokke, temperatur og effekt), men er lavere enn i 4B.

### P-tilstanden (kortet vekkes ikke av sidene)

`nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu -l 1` (bare lesing) gikk fra 20 s før økta til 10 s etter. Kjøring 2: 362 prøver. Kjøring 3: 252 prøver.

- **Mens vinduet var åpent: P8 i alle prøvene**, 179 i kjøring 2 (23:39:18–23:42:19, høyeste kjerneklokke 420 MHz), 189 i kjøring 3 (23:52:36–23:55:37) og 187 med det siste bygget (00:35:12–00:38:14, `docs/skjermbilder/1.1.0/6c/ekte/pstate.csv`). I det tidsrommet åpnet planen begge sidene, byttet lyst og mørkt, endret vindusstørrelsen til 1920, 640 og 1440 px, åpnet (i) og «Hva skal til?», lagde og tilbakestilte utkast, tok 20 skjermbilder og sto i ro i 60 s på hver side.
- **P0 i 4–5 s før vinduet startet:** i kjøring 3 bare når Shell startet (23:51:54), med det siste bygget når Shell startet og én gang mens bare tjenesten gikk. I kjøring 2 tre ganger: 23:36:45 (den hodeløse Shell starter, kjent fra 4A), og 23:37:44 og 23:39:04, mens bare tjenesten gikk i økta. De to siste skyldes ikke sidene.
- **Mulig funn for tjenesten (ikke 6Cs område):** P0-toppene uten vindu kom 54–63 s etter at tjenesten startet i alle tre kjøringene med lang nok oppvarming (23:26:32, 23:37:44 og 00:35:06), og deretter igjen etter 80–83 s. Med vinduet koblet til kom ingen. Det kan være noe tjenesten gjør omtrent hvert minutt uten klienter, eller andre agenters hodeløse økter (hver ny Shell vekker kortet i ca. 4 s, 4A). Bør sjekkes med `agentic-riggd` alene, uten andre økter.
- I den første kjøringen var det også P0 i ca. 4 s idet vinduet startet (23:27:55) og etter en endring av vindusstørrelsen (23:28:27). Det kom ikke igjen i den andre kjøringen, og kom på samme tid som andre økter startet. Bør sjekkes med kortet alene, i brukerens økt.

### Skjermbildene

`docs/skjermbilder/1.1.0/6c/`: `ekte/` (låst, lyst og mørkt, 1920 og 640 px, hele siden, (i) og «Hva skal til?»), `sim/` (utkast, Bruk → Behold, Bruk → Rull tilbake, Tilbakestill alt, Manuell) og `for/` (1.0.0 · bygg 16 fra runde 4B, til sammenlikning).

## Runde 6A · Disker, Prosessor og System (8.–9. november 2026, 1.1.0 · bygg 17)

Release-bygget, cairo. Maskinen var tungt lastet av andre agenter under alle målingene (lastsnitt 15–24 på 16 tråder), så tallene er heller for høye enn for lave. Rådata: `maal/runde6a-tjeneste.txt`, `maal/runde6a-vindu.txt`, `maal/runde6a-vindu-trader.txt` og `maal/runde6a-rammer.txt`. Skriptene: `verktoy/runde6a/tjeneste.sh` (med `klient.py`) og `verktoy/runde3c/kjor.sh cpu.sh` / `rammer.sh`.

### Tjenesten (ekte kort, ekte disker og ekte UDisks2, 90 s per måling)

Kjørt utenfor den private økta, fordi UDisks2 bare finnes på den ekte systembussen: egne XDG-mapper og socket, `RIGG_PROFIL=dev`, ingen øktbuss. `RIGG_DISKER=ingen` slår diskene av til sammenligning.

| Tilstand | Uten disker | Med disker | Krav |
|---|---|---|---|
| Bakgrunn, 2 s-takt, ingen klient | 0,88 % | **0,90 %** | ≤ 1,2 % |
| «Vindu» på Disker (prøver hvert 1 s, `get_disks` hvert 15. s, UDisks2 hvert 60. s) | 1,93 % | **2,04 %** | ≤ 2,5 % |

Privat minne 21,0 → 21,1 MB i bakgrunnen og 21,5 → 22,9 MB med vindu (disklista med SMART-tabellene ligger i minnet). Seks tråder, som før (GDBus-tråden for systembussen fantes alt for logind).

- **Hva diskene koster:** `/proc/diskstats` leses med én `pread` per runde (ca. 1,5 kB med 32 loop-enheter). UDisks2 spørres ikke i bakgrunnen, med mindre diskvarselet er på (da hvert 10. min). Med vindu: én `GetManagedObjects` (64 kB svar) hvert 60. s, og `SmartGetAttributes` bare for disker der `SmartUpdated` har endret seg (UDisks2 leser selv omtrent hvert 10. min). Målt mot de fem diskene: 20 ms veggtid første gang med alle tabellene, 10 ms deretter (debug-bygg).
- Historikken har 112 serier (96 før): fire per disk og summen. Fila er høyst ca. 8 MB (testen `verste_tilfelle_under_grensen`).

### Vinduet i ro per side (3 × 20 s, % av én kjerne)

| Side | Vinduet | Privat minne | Bilder per 10 s |
|---|---|---|---|
| Disker (én disk foldet ut med graf) | **1,10 / 1,35 / 1,55** | 14,1 MB | 37–38 à 2,3–3,5 ms |
| Prosessor (trådrutenettet tegnet i én widget) | **0,95 / 1,00 / 1,05** | 13,3 MB | 11 à 9,1 ms |
| Prosessor før (48 etiketter og 16 stolper) | 2,65 / 3,05 / 3,65 | 14,1 MB | 10–17 à 19–23 ms |
| System | **0,40 / 0,55 / 0,45** | 12,0 MB | 11 à 2,0 ms |
| Oversikt (til sammenligning) | 1,20 / 1,20 / 1,20 | 13,1 MB | – |

- **Prosessor: 5,25 → 3,05 → 1,00 %.** Første versjon lå på 5,25–5,95 %. Først ble de levende radene (temperatur, governor, boost) tatt ut av panelene i spaltene, så de ikke måles om hvert sekund (de står i Nå), og flisene satt i faste rutenett i stedet for FlowBox: 3,05 %. Så ble klokke og last per tråd tegnet i én widget (`widgets/trader.rs`, `RiggThreadGrid`): tekst med pango og stolper og kanter som fargede flater, uten layout når tallene endres, og ingen tegning når det som vises står stille. Klokkene rundes til 25 MHz. Samme utseende, 640 px (to kjerner per rad), tilgjengelig beskrivelse med alle verdiene og verktøytips per tråd. Målt med lastsnitt 20–36 (`maal/runde6a-vindu-trader.txt`, `maal/runde6a-rammer.txt`).
- Disker og System er under kravet. P8 på kortet i alle målingene.

## Runde 6D · Kritikk og finpuss (9. november 2026, 1.1.0 · bygg 17)

**Slik er det målt.** `verktoy/runde6d/maal6d.sh`: én privat, hodeløs økt (`scripts/okt.sh`, 1920×1080), release-bygget og cairo, tjenesten mot det ekte kortet (bare lesing) med «Fortsett i bakgrunnen» på og 2 s-takt. `nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu -lms 1000` gikk hele tiden, med en linje for hvert steg. CPU er fra `/proc/<pid>/stat`, minne fra `smaps_rollup`. Ingen andre agenter kjørte. **Lastsnitt 2,0–3,8** under målingen (brukerens egne programmer), mye lavere enn i 6A–6C (15–36). Rådata: `maal/runde6d-maal.txt` og `maal/runde6d-pstate.csv`.

Et første forsøk 01:41 ble stoppet etter tjenesten alene, de kalde startene og 12 min på Oversikt, fordi maskinen gikk tom for minne (andre prosjekter, swap nesten full). Det forsøket hadde P8 i alle 244 prøvene utenom 3 s idet Shell startet, også de 12 minuttene med Oversikt. Alt under er fra den hele kjøringen 09:05–09:24.

### P0-toppene fra 6C: ikke tjenesten

6C så kortet i P0 i 4–5 s omtrent 54–63 s og 80–83 s etter at `agentic-riggd` startet, uten klient. Undersøkt med meg som eneste agent (`verktoy/runde6d/p0-forsok.sh`, rådata `maal/runde6d-p0-*.csv`):

| Forsøk | Varighet | Prøver | P0 |
|---|---|---|---|
| Grunnlinje: ingenting fra meg | 240 s | 240 | ingen |
| A: tjenesten alene, ekte NVML, uten Shell og øktbuss | 200 s | 223 | ingen |
| C: bare den private økta (hodeløs Shell) | 220 s | 247 | 3 s, idet Shell startet |
| B: privat økt, tjenesten startet 20 s etter Shell, uten vindu (som 6C) | 200 s | 247 | 4 s, idet Shell startet; ingen 54–63 s eller 80–83 s etter tjenesten |
| 6D-målingen: tjenesten alene 2 × 180 s, 15 kalde starter, vinduet på alle ti sidene | 18,5 min | 1 120 | 3 s, idet Shell startet (før tjenesten) |

**Konklusjon:** tjenesten vekker ikke kortet. Hver hodeløse `gnome-shell` går til P0 (1 515 MHz, 35 W) i 3–4 s når den starter (kjent fra 4A). 6A, 6B og 6C kjørte private økter samtidig, så toppene i 6Cs logg var de andre agentenes Shell-starter. I koden er det ingenting som hvert minutt spør kortet: `every60` brukes ikke av NVML-backenden, `malloc_trim` og lagringen av historikken hvert 60. s er bare minne og disk, og UDisks2 spørres ikke i bakgrunnen med diskvarselet av. Ingen kodeendring.

### Tallene (krav fra produktspesifikasjonen kap. 9 og oppdraget)

| Måling | Krav | 6D |
|---|---|---|
| Kald start til første bilde, median av 15 | ≤ 250 ms | **176 ms** (163–195) ✓ |
| Tall fra kortet i vinduet | ≤ 1 s | 173–221 ms ✓ |
| Lukking: vinduet borte | < 100 ms | **0,6–0,9 ms** ✓ |
| Lukking: prosessen avsluttet | ≤ 300 ms | 10,2–12,5 ms ✓ |
| Tjenesten i bakgrunnen (2 s, ingen vindu), før og etter vinduene | ≤ 1,2 % | **0,72 % / 0,72 %** ✓ |
| Tjenesten med vinduet åpent (1 s) | ≤ 2,5 % | **1,80–2,50 %** ✓ (høyest med Sensorer og Grafer, som abonnerer på flest verdier) |
| Vinduet i ro, alle ti sidene | ≤ 2 % | **0,40–1,85 %** ✓ (RGB-lys i 2D for seg, under) |
| Tjenesten, privat minne | ≤ 45 MB | 21,2 → 22,5 MB etter alle sidene |
| Vinduet, privat minne | ≤ 70 MB | 12,0–16,0 MB |

### Vinduet i ro per side (3 × 20 s etter 10 s, % av én kjerne)

| Side | Vinduet | Tjenesten samtidig | Vinduet, privat minne |
|---|---|---|---|
| Oversikt | 1,35 / 1,45 / 1,25 | 1,95 | 13,2 MB |
| Sensorer | 1,75 / 1,65 / 1,85 | 2,40 | 16,0 MB |
| Grafer | 0,95 / 1,00 / 0,85 | 2,35–2,50 | 12,7 MB |
| Grafikkort | 0,50 / 0,50 / 0,60 | 1,80–1,85 | 13,7 MB |
| Prosessor | 1,15 / 1,20 / 1,25 | 2,15–2,25 | 13,4 MB |
| Disker | 1,05 / 1,25 / 0,95 | 1,85–1,90 | 14,1 MB |
| System | 0,40 / 0,45 / 0,50 | 1,90 | 12,0 MB |
| Klokker og effekt | 0,75 / 0,75 / 0,80 | 2,00–2,05 | 14,1 MB |
| Vifter | 0,60 / 0,60 / 0,65 | 1,85–1,95 | 15,8 MB |
| RGB-lys (2D, viftene animerer) | 16,45 / 16,70 / 16,85 | 1,90–2,00 | 14,6 MB |

- **RGB-lys** animerer viftene i 2D-kortet («Bevegelse» står på system i den private økta), som i 4B (16,75 %). Med «Bevegelse: av» viser siden et stillbilde med merket «Animasjon av» (ikke målt her). Det er en animasjon, ikke ro, og noteres for seg.
- Kortet var i **P8 i alle prøvene** mens vinduet var åpent på hver av de ti sidene (09:09–09:21).
- Skjermbildeplanen `6c-ekte` målte i forserien Klokker og effekt 0,68 %, Vifter 0,73 % og med utkast 0,90 % (60/60/20 s), som 6C.

## Runde 7B · Vifter med viftemotoren (9. november 2026, 1.2.0 under arbeid)

Målt med release-bygget og cairo i private, hodeløse økter (`verktoy/runde7b/bilder7b.sh`, planene `7b-ekte` og `7b-sim` i `pages/styring_bilder.rs`), én økt om gangen. Det ekte kortet leses med NVML; ingenting skrives. Vinduet er 1440×900 med «Bevegelse: full». Vinduets CPU er fra `/proc/self/stat` og bildene fra `GdkFrameClock`; tjenestens CPU er fra `/proc/<pid>/stat` i skriptet. Rådata: `docs/skjermbilder/1.2.0/7b/7b-ekte/` og `7b-sim/` (`maal.txt`, `tjeneste.txt`, `steg.txt`, `pstate.csv`). Lastsnitt 2–4; 7A arbeidet samtidig, men uten egne økter mens målingene gikk.

### Vinduet i ro på Vifter (60 s, krav ≤ 2 %)

| Tilstand | 6C (1.1.0) | 7B | Bilder tegnet |
|---|---|---|---|
| Ekte kort, låst, ingen utkast | 0,68 % | **0,67 %** | 2,3 per s |
| Ekte kort, utkast med egen kurve per vifte | – | **0,70 %** | 2,3 per s |
| `sim:spill`, kurven i bruk (viftene endrer seg hvert sekund) | – | **0,63 %** | 1,2 per s |

- Den nye forhåndsvisningen henter fem minutter historikk hvert 10. s og regner motoren over 300 prøver bare når utkastet eller historikken er ny. Gliderne og grafen tegnes bare når en verdi endres. Siden koster det samme som i 6C.
- Første kjøring med debug-bygget: 0,93–0,97 %.

### Tjenesten

| Måling | 7B | Krav / sammenligning |
|---|---|---|
| Ekte kort (NVML), alene uten vindu, 30 s | **0,67 %** | 6D: 0,72 % i bakgrunnen (krav 1,2 %) |
| Ekte kort, med vinduet på Vifter, hele planen (153 s, med 640/1920 px og temabytter) | **1,96 %**, 33 MB RSS, 21 MB privat | 6D: 1,80–2,50 % (krav 2,5 %) |
| `sim:spill` med viftesløyfen, alene 30 s | **0,03 %** | – |
| `sim:spill` med vinduet og kurven i bruk, hele planen (327 s) | **0,09 %**, 10 MB RSS | – |

Viftesløyfen (varmemodellen og motoren per kort) koster ikke målbart: hele sim-tjenesten med kurven i bruk, Bruk/Behold to ganger og vinduet koblet til bruker under 0,1 % av én kjerne. Med NVML kjører ikke viftesløyfen i det hele tatt.

### P-tilstanden (kortet vekkes ikke av siden)

`nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu -l 1` (bare lesing) fra 20 s før økta til 10 s etter: 361 prøver i P8 og 3 i P0. De tre P0-prøvene (11:56:13–15) er idet den hodeløse Shell starter, som i 4A og 6D; vinduet åpnet Vifter 11:59:18. **P8 i alle prøvene mens vinduet var åpent** (bilder i lyst og mørkt, per vifte, hysterese, °F, høy kontrast, 640 og 1920 px, og 2 × 60 s i ro). Første kjøring (11:28–11:33) ga det samme.

## Runde 7A · Hjelperen av (9. november 2026, 1.2.0 under arbeid, målt av 7C)

7A rakk ikke å måle selv. Målt av 7C 12:19–12:24 med release-bygget fra 7A og 7B, før 7Cs rettinger. `--version` sa fortsatt «1.1.0 · bygg 17», for versjonen ble satt opp senere i 7C.

**Slik er det målt:**
- Skriptet er `verktoy/runde7c/maal7c.sh` med `RUNDE=7a PAGES= BG_SECS=120 RUNS=15`. Det er 6Ds `maal6d.sh` med valgbart rundenavn og valgbare sider.
- Én privat, hodeløs økt med cairo. Tjenesten leser det ekte kortet (bare lesing), med «Fortsett i bakgrunnen» på og 2 s-takt.
- Hjelperen er **av**: ingen `/etc/agentic-rigg/helper.json`. Da ser tjenesten etter hjelperen med én `stat` hvert 60. s.
- `nvidia-smi --query-gpu … -lms 1000` gikk ved siden av.
- Lastsnittet var 0,7–3,0. Ingen andre agenter kjørte.
- Rådata: `maal/runde7a-maal.txt` og `maal/runde7a-pstate.csv`.

| Måling | Krav | 6D (1.1.0) | 7A (hjelperen av) |
|---|---|---|---|
| Kald start til første bilde, median av 15 | ≤ 250 ms | 176 ms (163–195) | **163 ms** (158–178) ✓ |
| Tall fra kortet i vinduet | ≤ 1 s | 173–221 ms | 160–190 ms ✓ |
| Lukking: vinduet borte | < 100 ms | 0,6–0,9 ms | **0,7–1,2 ms** ✓ |
| Lukking: prosessen avsluttet | ≤ 300 ms | 10,2–12,5 ms | 8,5–11,9 ms ✓ |
| Tjenesten i bakgrunnen (2 s, uten vindu), 120 s før og etter startene | ≤ 1,2 % | 0,72 % / 0,72 % | **0,78 % / 0,71 %** ✓ |
| Tjenestens minne (RSS / PSS / privat) | ≤ 45 MB privat | 21,2 MB privat | 33,1 / 25,3 / 21,1 MB, og 33,5 / 25,7 / 21,5 MB etter ✓ |

- **Klienten mot hjelperen** (`riggd/src/hjelper.rs`) koster ikke målbart når hjelperen er av. Den gjør én `stat` av `helper.json` og socketen hvert 60. s. Tjenesten har seks tråder, som i 6D.
- **P-tilstanden:** 292 prøver i P8 og 3 i P0. P0-prøvene er de 3 s idet den hodeløse Shell starter (12:19:08–10), før tjenesten. Kortet var i P8 under alle de kalde startene og i bakgrunnen.

## Runde 7C · Sluttrunde (9. november 2026, 1.2.0 · bygg 18)

**Slik er det målt:**
- Skriptet er `verktoy/runde7c/maal7c.sh` (`RUNDE=7c`), bygd på 6Ds `maal6d.sh`, i én privat, hodeløs økt (1920×1080) med release-bygget for 1.2.0 · bygg 18 og cairo.
- Tjenesten leser det ekte kortet (bare lesing), med «Fortsett i bakgrunnen» på og 2 s-takt. Hjelperen er av.
- Rekkefølgen:
  1. tjenesten alene i 180 s
  2. 15 kalde starter med lukking
  3. hver side i 10 s pluss 3 × 20 s
  4. tjenesten alene i 180 s igjen
- `nvidia-smi --query-gpu … -lms 1000` gikk hele tiden, med en linje per steg.
- CPU er fra `/proc/<pid>/stat`, og minnet fra `smaps_rollup`.
- Ingen andre agenter kjørte.
- **Lastsnittet var 1,9–3,7** under sidene. Ved start var det 5,2, rett etter mitt eget release-bygg, og ved slutten 9,6 (brukerens egne programmer).
- Rådata: `maal/runde7c-maal.txt` og `maal/runde7c-pstate.csv`.
- Kodeendringene etter målingen er tekst (verktøytipset på Bruk) og radene med flisene på Vifter. Ingen av dem er i de målte løkkene.

### Tallene (krav fra oppdraget og produktspesifikasjonen kap. 9)

| Måling | Krav | 6D (1.1.0) | 7C (1.2.0) |
|---|---|---|---|
| Kald start til første bilde, median av 15 | ≤ 250 ms | 176 ms | **182 ms** (162–190 for start 3–15) ✓ |
| Tall fra kortet i vinduet | ≤ 1 s | 173–221 ms | 164–411 ms ✓ |
| Lukking: vinduet borte | < 100 ms | 0,6–0,9 ms | **0,6–1,0 ms** ✓ |
| Lukking: prosessen avsluttet | ≤ 300 ms | 10,2–12,5 ms | 9,3–13,7 ms (første start 25,3 ms) ✓ |
| Tjenesten i bakgrunnen (2 s, uten vindu), før og etter vinduene | ≤ 1,2 % | 0,72 % / 0,72 % | **0,76 % / 0,71 %** ✓ |
| Tjenesten med vinduet åpent (1 s) | ≤ 2,5 % | 1,80–2,50 % | **1,80–2,45 %** ✓ |
| Vinduet i ro, alle ti sidene | ≤ 2 % | 0,40–1,85 % | **0,40–1,75 %** ✓ (RGB-lys i 2D for seg) |
| Tjenesten, minne (privat / PSS / RSS) | ≤ 45 MB privat | 21,2–22,5 MB privat | 21,2 → 22,5 MB privat, PSS 25,4–26,7 MB, RSS 33,1–34,4 MB |
| Vinduet, minne (privat / PSS / RSS) | ≤ 70 MB | 12,0–16,0 MB privat | 12,0–16,7 MB privat, PSS 24,1–29,4 MB, RSS 56–63 MB |
| P-tilstand med vinduet åpent | P8 | P8 | **P8 i alle prøvene** ✓ |

**De to første kalde startene** tok 367 og 404 ms. De kom 15 s etter at release-bygget var ferdig, med lastsnitt 5. Binæren var ny og ikke i sidebufferen ennå, og den første lukkingen tok 25 ms av samme grunn. Fra tredje start var det 162–190 ms, som i 6D (163–195). Medianen av alle 15 er 182 ms. Brukerens første start etter installasjonen kan få det samme ene tregere første bildet.

### Vinduet i ro per side (3 × 20 s etter 10 s, % av én kjerne)

| Side | Vinduet | Tjenesten samtidig | Vinduet, privat minne |
|---|---|---|---|
| Oversikt | 1,45 / 1,55 / 1,30 | 2,00–2,05 | 13,3 MB |
| Sensorer | 1,65 / 1,65 / 1,75 | 2,35–2,45 | 16,0 MB |
| Grafer | 0,85 / 1,00 / 0,90 | 2,40–2,45 | 12,7 MB |
| Grafikkort | 0,50 / 0,55 / 0,65 | 1,80–1,90 | 13,7 MB |
| Prosessor | 1,25 / 1,35 / 1,10 | 2,20 | 13,4 MB |
| Disker | 1,05 / 1,65 / 1,05 | 1,80–1,90 | 14,2 MB |
| System | 0,45 / 0,40 / 0,40 | 1,90 | 12,0 MB |
| Klokker og effekt | 0,80 / 0,75 / 0,80 | 2,00–2,10 | 14,1 MB |
| Vifter (med «Siste 5 minutter» og Oppførsel, nytt i 1.2.0) | 0,70 / 0,75 / 0,90 | 1,90–1,95 | 16,7 MB |
| RGB-lys (2D, viftene animerer) | 16,85 / 16,60 / 16,85 | 2,00–2,40 | 14,6 MB |

**Kommentarer:**
- **RGB-lys** animerer viftene i 2D-kortet («Bevegelse» står på «system» i den private økta), som i 4B og 6D (16,7 %). Det er en animasjon, ikke ro, og noteres for seg. Med «Bevegelse: av» står kortet stille.
- **Vifter** koster det samme som i 6C og 6D (0,60–0,65 %), med den nye forhåndsvisningen og de nye panelene. Den bruker 1 MB mer minne.
- **Tjenesten** er uendret fra 6D. Klienten mot hjelperen gjør én `stat` hvert 60. s når hjelperen er av.
- **P-tilstanden:** 1 116 prøver i P8 og 3 i P0. P0-prøvene er de 3 s idet den hodeløse Shell startet (12:44:05–07), før tjenesten. Kortet var i P8 gjennom alle startene, alle ti sidene og begge periodene med bare tjenesten.

## Installert 1.2.0 · bygg 18 i brukerens økt (9. november 2026, hovedøkta)

`install.sh` er kjørt over 1.0.0 · bygg 16 etter brukerens ja. Hjelperfilene ligger i `~/.local/share/agentic-rigg/hjelper/`. Hjelperen er ikke aktivert, og ingenting er lagt i `/usr/local`. Tjenesten er `disabled`.

| Måling | Resultat |
|---|---|
| Kald start, fra `main()` til første bilde | 178 ms (210 ms fra exec) |
| Tall fra kortet | 281 ms |
| Vinduet i ro på Oversikt (urørt, 2 × 10 s) | 0–0,1 % |
| Tjenesten med vinduet åpent | 2,2–2,4 % |
| Privat minne | vinduet 25 MB, tjenesten 29 MB (første minutt, med historikk fra disk) |
| P-tilstand | P8 |

## Runde 8A · Oversikt over hele maskinen (9. november 2026, 1.3.0 · bygg 19)

**Slik er det målt:** `verktoy/runde8a/maal8a.sh` (kopi av 7Cs `maal7c.sh`, `RUNDE=8a`), én privat, hodeløs økt (1920×1080), release-bygget og cairo, tjenesten mot det ekte kortet (bare lesing), «Fortsett i bakgrunnen» på og 2 s-takt, hjelperen av. Rekkefølgen som i 7C: tjenesten alene 180 s, 15 kalde starter med lukking, hver side 10 s pluss 3 × 20 s, tjenesten alene 180 s. `verktoy/runde8a/profil8a.sh` måler bildene (`RIGG_2A_PROFIL=1`) for én side.

**Forholdene var ikke rolige.** Andre programmer på maskinen (nettleseren og Node-prosesser fra andre prosjekter) ga lastsnitt 4,6–16,8 og holdt skjermkortet i **P0 i 908 av 1 123 prøver**, også før appen startet (170 av 183 prøver mens bare tjenesten gikk). Da endrer alle tallene seg hvert sekund, og sidene tegner mer enn i 7C, der kortet sto i P8. For å skille appen fra forholdene er 1.2.0 · bygg 18 (de installerte binærene, kopiert til `~/.cache/rigg-build/v120/`) målt på de samme sidene i samme time.

Rådata: `maal/runde8a-maal.txt` og `-pstate.csv` (hele serien), `runde8a-forste-*` (første forsøk, før rettingene under), `runde8a-sjekk-*` og `runde8a-grafikkort-*` (1.3.0 mot 1.2.0).

### Tallene (krav fra oppdraget og produktspesifikasjonen kap. 9)

| Måling | Krav | 7C (1.2.0, kortet i P8) | 8A (1.3.0, kortet i P0) |
|---|---|---|---|
| Kald start til første bilde, median av 15 | ≤ 250 ms | 182 ms | **236 ms** (211–267 utenom første, 309 ms) ✓ |
| Tall fra kortet i vinduet | ≤ 1 s | 164–411 ms | 213–328 ms ✓ |
| Lukking: vinduet borte | < 100 ms | 0,6–1,0 ms | **0,8–1,6 ms** ✓ |
| Lukking: prosessen avsluttet | ≤ 300 ms | 9,3–13,7 ms | 9,2–21,7 ms ✓ |
| Tjenesten i bakgrunnen (2 s), før og etter vinduene | ≤ 1,2 % | 0,76 / 0,71 % | **0,63 / 0,73 %** ✓ |
| Tjenesten med vinduet åpent (1 s) | ≤ 2,5 % | 1,80–2,45 % | **1,65–2,35 %** på åtte sider; enkeltmålinger på Grafikkort (2,80) og Disker (3,00) over |
| Vinduet i ro, Oversikt | ≤ 2 % | 1,30–1,55 % | **2,25–2,50 %** (1.2.0 samme time: 4,95–8,55 %) |
| Vinduet, minne (privat) | ≤ 70 MB | 12,0–16,7 MB | 12,1–17,1 MB ✓ |
| Tjenesten, minne (privat) | ≤ 45 MB | 21,2–22,5 MB | 21,2–22,5 MB ✓ |

### Vinduet i ro per side (3 × 20 s etter 10 s, % av én kjerne, kortet i P0)

| Side | Vinduet | Tjenesten samtidig | Vinduet, privat minne | 7C (P8) |
|---|---|---|---|---|
| Oversikt (ny) | 2,40 / 2,25 / 2,35 | 1,65–1,90 | 13,9 MB | 1,45 / 1,55 / 1,30 |
| Sensorer | 3,20 / 2,65 / 2,45 | 1,90–1,95 | 16,2 MB | 1,65 / 1,65 / 1,75 |
| Grafer | 2,40 / 2,25 / 2,05 | 1,75–1,80 | 12,9 MB | 0,85 / 1,00 / 0,90 |
| Grafikkort | 5,55 / 5,15 / 4,60, etter rettingen **1,15 / 1,00 / 2,50** | 1,85–2,80 | 14,3 MB | 0,50 / 0,55 / 0,65 |
| Prosessor | 2,20 / 2,50 / 3,00 | 1,90–2,30 | 13,6 MB | 1,25 / 1,35 / 1,10 |
| Disker | 1,30 / 1,50 / 1,35 | 2,00–3,00 | 14,2 MB | 1,05 / 1,65 / 1,05 |
| System | 0,50 / 0,50 / 0,50 | 1,80–2,25 | 12,1 MB | 0,45 / 0,40 / 0,40 |
| Klokker og effekt | 1,25 / 1,25 / 1,05 | 1,80–1,85 | 14,6 MB | 0,80 / 0,75 / 0,80 |
| Vifter | 1,40 / 0,95 / 0,90 | 1,70–1,85 | 17,1 MB | 0,70 / 0,75 / 0,90 |
| RGB-lys (2D, viftene animerer) | 17,55 / 17,70 / 17,80 | 1,75–1,80 | 15,0 MB | 16,85 / 16,60 / 16,85 |

### 1.3.0 mot 1.2.0 i samme forhold (kortet i P0)

| Side | 1.2.0 · bygg 18 | 1.3.0 · bygg 19 |
|---|---|---|
| Oversikt (sjekk 1) | 8,55 / 6,10 / 4,95 | 4,15 / 3,70 / 4,00 (før rettingene) |
| Grafer (sjekk 1) | 1,85 / 1,65 / 1,75 | 1,40 / 1,40 / 1,45 |
| Oversikt (sjekk 2) | 6,30 / 6,95 / 5,80 | **2,35 / 2,50 / 2,45** |
| Grafikkort (sjekk 2) | 7,00 / 5,75 / 5,90 | **1,15 / 1,00 / 2,50** |

### Hva Oversikt brukte tiden på, og hva som er rettet

Første forsøk ga Oversikt 4,60–5,85 %. Med `profil8a.sh`:

1. **Grafen for minne og VRAM** regnet VRAM om til prosent med kortets faste data, og kopierte dem (med hele `extra`) **for hvert punkt** i hver tegning. Nå leses totalen én gang (`App::with_statics`, uten kopi). Grafene for klokker og effekt kopierte de faste dataene én gang per tegning; de leser også uten kopi nå.
2. **Glidende målere og stolper.** Med kortet i P0 endrer effekten og klokkene seg mer enn 3 % nesten hvert sekund, og målerne og klokkestolpene gled i 0,2 s hver gang: 14–23 bilder i sekundet. På Oversikt og Grafikkort hopper de nå rett til ny verdi (`Gauge::set_still`, `Bar::set_still`). Det samme skjedde i 1.2.0 (målt over).
3. **Seks grafer som ruller hver for seg** ga 4–5 bilder i sekundet à 7 ms. På Oversikt flytter grafene seg nå med prøvene, i samme bilde som flisene (`Chart::set_calm`): 1 bilde i sekundet à ca. 17–21 ms. Grafer har fortsatt den jevne rullingen.
4. Mindre: flisa for diskene byttet verktøytips hvert sekund (MB/s), og stolpene og målerne meldte verdien til skjermleseren hvert sekund. Nå har verktøytipset ingen tall som endres, og skjermleseren får hele prosent bare når den endres.

**Det som står igjen:** ett bilde i sekundet der flisene og seks grafer tegnes med cairo, ca. 17–21 ms. Med kortet i P0 og lastsnitt 5–16 gir det 2,25–2,50 %, mot 4,95–8,55 % for 1.2.0 i samme time. I 7C-forhold (P8, lastsnitt 2–4) var Oversikt i 1.2.0 1,30–1,55 %; 1.3.0 er ikke målt i rolige forhold, fordi maskinen ikke ble rolig mens runden pågikk. Bør måles på nytt i brukerens økt etter installasjonen.

**Kald start:** 236 ms median (7C: 182 ms). Oversikt har nå seks grafer og seks fliser å bygge, og lasten var høyere. Første start (309 ms) kom rett etter release-bygget.

**P-tilstanden:** P0 kom fra andre programmer, ikke fra tjenesten eller vinduet: kortet sto i P0 i 170 av 183 prøver mens bare tjenesten gikk, før noe vindu var åpnet, og i 7C (samme kode for lesingen) var det P8 hele veien.
