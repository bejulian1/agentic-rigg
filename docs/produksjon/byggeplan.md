# Agentic Rigg 1.0 · byggeplan

*Produkt- og arkitekturagenten, 7. november 2026. Bygger på `produktspesifikasjon.md` og `arkitektur.md`. Hovedøkta deler ut rundene herfra.*

## Regler for alle rundene

- Les `REGLER.md`, `produktspesifikasjon.md` og `arkitektur.md` først. Demoen er fasiten for design og ordlyd.
- **Eierskap.** En fil har én eier per runde. Du skriver bare i filene runden din eier. Trenger du noe i en annens fil, skriver du det som et ønske i `LOGG.md`, og hovedøkta tar det videre. Grunnmuren lager alle filene i tabellen som stubber med riktig signatur og registrering (moduler, sider, operasjoner), så de parallelle agentene aldri trenger å røre felles filer som `mod.rs`, `ops.rs` eller `Cargo.toml`.
- **Bygging.** `CARGO_BUILD_JOBS=6`, `CARGO_TARGET_DIR=$HOME/.cache/rigg-build`. Sjekk `MemAvailable` før hvert bygg (minst 6 GB). Cargo låser mappa, så parallelle agenter venter på hverandre. Bruk `cargo check -p <din crate>` mens du jobber, og fullt bygg bare når du trenger å kjøre.
- **Utviklingsforekomst.** Kjør alltid med `RIGG_PROFIL=dev` og egne mapper (`arkitektur.md` 4.8). Ikke koble til, stopp eller overskriv en installert forekomst eller demoen på port 4318. Ikke kjør `install.sh` uten at hovedøkta har sagt ja.
- **Ingen skriving til kortet.** Bruk `--backend sim` for alt som har med Bruk, Behold og Rull tilbake å gjøre.
- **Ferdig betyr:** `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer, tallene du påstår er målt og står i `docs/produksjon/maalinger.md`, skjermbilder av det du har laget ligger i `docs/skjermbilder/1.0.0/<runde>/`, og du har en oppføring i `LOGG.md`.
- **Finpuss.** Hver runde ender med en egen gjennomgang av detaljer: hover, fokus, tomtilstander, lastetilstander, tall som ikke danser, avstander, tastatur. «Detaljer er magien.»

## Oversikt

| Runde | Agenter | Parallelt? | Leverer |
|---|---|---|---|
| 0 | Produkt og arkitektur, maskinvarerevisjon, ikon | ja (ferdig) | Dokumentene, `maskinvare-revisjon.md`, ikonene |
| 1 | **Grunnmur** (1 agent) | – | Arbeidsområdet, `rigg-core`, `rigg-hw`, tjenesten, skallet, temaene, Innstillinger-rammen, install-skriptene, måleskriptet. Alle sider finnes som stubber |
| 2 | **2A Data**, **2B Maskinvare og rapport**, **2C Styring (låst) og Lys** | ja, 3 agenter | Alle ni sidene, logging, eksport, rapport, tilstander, 3D |
| 3 | **3A Optimalisering og integrasjon**, **3B Bakgrunn** (valgfri) | ja, 2 agenter | Målene i produktspesifikasjonen kap. 9, tegnervalg, skjermbildeserie, systemfelt og autostart |
| 4 | **4A Kritikk** så **4B Finpuss** | etter hverandre | Designkritikk med friske øyne, rettinger, README, STATUS og PATCHLOG for 1.0.0 |
| 5 | Installasjon | hovedøkta | `install.sh` på ekte, med brukerens ja |

---

## Runde 1 · Grunnmur (én agent)

**Mål:** et vindu som åpner seg på under 250 ms med ekte tall fra tjenesten på en enkel Oversikt, alle temaene og Innstillinger-rammen, og en tjeneste som oppfyller kontrakten for lesing. Etter runden kan tre agenter jobbe samtidig uten å røre de samme filene.

**Eier (alt i `app/` som ikke er nevnt under runde 2 og 3, og spesielt):**

| Område | Filer |
|---|---|
| Arbeidsområdet | `app/Cargo.toml`, `app/version.json`, `app/clippy.toml`, `app/install.sh`, `app/uninstall.sh`, `app/scripts/maal.sh`, `app/scripts/bilder.sh`, `app/data/rigg.gresource.xml`, `app/data/io.github.AgenticRigg.desktop.in`, `app/data/systemd/agentic-riggd.service` |
| `rigg-core` | `lib.rs`, `model.rs`, `sensor_key.rs`, `proto.rs`, `history.rs`, `stats.rs`, `settings.rs`, `fmt.rs`, `sensormeta.rs`, `downsample.rs`, `easter.rs`, `tokens.rs`, og **stubber** for `specs.rs`, `report.rs`, `csv.rs` (2B) og `txn.rs` (2C) |
| `rigg-hw` | `lib.rs`, `gpu/mod.rs` (trait og tilstander), `gpu/nvml.rs` (init, oppregning, prøver, feil), `gpu/sim.rs` (scenariene), `sys/mod.rs`, `sys/cpu.rs`, `sys/mem.rs`, `sys/net.rs`, `sys/hwmon.rs`, `tests/bare_lesing.rs`, `tests/fixtures/`, og **stubber** for `gpu/nvml_info.rs`, `sys/dmi.rs`, `sys/cpuinfo.rs`, `sys/os.rs`, `sys/pci.rs` (2B) |
| `riggd` | `main.rs`, `server.rs`, `session.rs`, `ops.rs` (alle operasjonene registrert; logg- og alarmoperasjonene kaller funksjoner i 2B-filene), `sampler.rs`, `schedule.rs`, `store.rs`, `lifecycle.rs` (grace, signaler, `READY=1`, dvale), `devices.rs`, `tests/kontrakt.rs`, og **stubber** for `logg.rs`, `varsler.rs` (2B) og `tray.rs` (3B) |
| `rigg` (skallet) | `main.rs`, `app.rs`, `window.rs`, `sidebar.rs` (kortvelger, sider, statusblokk, legg sammen), `topbar.rs`, `client.rs`, `state.rs`, `theme.rs`, `prefs.rs`, `shortcuts.rs` (og snarveisdialogen), `toast.rs`, `motion.rs`, `easter.rs` (krokene for alle ti eggene), `infobar.rs` (linjene under toppfeltet), `widgets/mod.rs`, `widgets/panel.rs`, `widgets/seg.rs`, `widgets/switchrow.rs`, `widgets/rows.rs`, `widgets/spark.rs`, `widgets/popover.rs`, `pages/mod.rs` (registeret med alle åtte sidene), og **stubber** for alle sidefilene, `widgets/chart.rs`, `widgets/gauge.rs`, `widgets/bar.rs` (2A), `rapport_ui.rs`, `logg_ui.rs` (2B), `widgets/locked.rs`, `rgb.rs`, `lys3d/` (2C) |
| Innstillinger | `rigg/src/settings/mod.rs`, `search.rs`, `generelt.rs`, `utseende.rs` (med appikon), `format.rs`, `sidefelt.rs`, `data.rs`, `om.rs`, og **stubber** for `grafer.rs` (2A) og `logging.rs` (2B) |
| Tema | `app/data/theme/tokens.css`, `base.css`, `skall.css`, `innstillinger.css`, og **tomme** sidefiler `oversikt.css`, `sensorer.css`, `grafer.css` (2A), `grafikkort.css`, `system.css` (2B), `kontroller.css`, `viftekurve.css`, `lys.css` (2C) |
| Dokumenter | `docs/produksjon/protokoll.md`, `docs/produksjon/maalinger.md` (første tall), `LOGG.md` |

**Oppgaver:**

1. Arbeidsområdet med de fire cratene, versjon fra `version.json`, `clippy.toml` med de forbudte NVML-metodene, release-profilen.
2. `rigg-core`: porter `history.js` (med alle testtilfellene fra `test/history.test.js`), statistikk, `mergeSettings`/`lenientSettings` (testene fra `test/store.test.js` som gjelder innstillinger, og import av en ekte eksport fra demoen), `fmt.js`, protokolltypene.
3. `rigg-hw`: NVML-backenden med prøveplanen og tilstandene, `/proc` og `/sys` med åpne filer og `pread`, fixtures fra denne maskinen, sim-backenden med scenariene. Bruk tallene i `maskinvare-revisjon.md`.
4. `riggd`: hele leseprotokollen, takt etter abonnement og fokus, historikk med lagring, min/maks/snitt, grace-stopp, `Type=notify`, `SO_PEERCRED`, `write_locked`. Kontrakttestene.
5. `rigg`: GtkApplication med én forekomst, ui.json før første bilde, skallet (sidefelt med alle tilstandene fra demoen, ikonlist, Ctrl+B), oppstart av tjenesten via systemd og reserven, klienten, tema-klassene med alle 96 kombinasjonene og kontrasttesten, bevegelsesnivåene, varsler, Innstillinger-rammen (hele vinduet, navigasjon, søk, tilbake på tre måter) med delene Generelt, Utseende, Enheter og format, Sidefelt, Data og Om. En enkel Oversikt med de seks nøkkeltallene, så kjeden kan måles.
6. `install.sh` og `uninstall.sh` (kjøres ikke på ekte, bare med `HOME` satt til en midlertidig mappe for å teste dem).
7. `maal.sh` og første målinger av kald start, minne og CPU.
8. Påskeeggenes logikk og kroker, så sidene bare kaller én funksjon.

**Ferdig når:** vinduet åpner med ekte GPU- og CPU-tall, alle åtte sidene finnes i sidefeltet (stubbene sier «Kommer i neste runde»), alle temaene og Innstillinger-delene virker, kontrakttestene og kontrasttesten passerer, og første bilde er målt.

---

## Runde 2 · tre agenter samtidig

Alle tre bygger på grunnmuren og rører bare filene sine. Grensesnittene mellom dem er funksjonene grunnmuren har laget som stubber.

### 2A · Data: Oversikt, Sensorer og Grafer

**Mål:** de tre målesidene som i demoen, med grafer som tegnes billig, historikk 1 min–48 t fra tjenesten og CSV-eksport.

| Eier | |
|---|---|
| `rigg/src/pages/oversikt.rs`, `sensorer.rs`, `grafer.rs` | Sidene |
| `rigg/src/widgets/chart.rs`, `gauge.rs`, `bar.rs` | Graf, målere, stolper (`arkitektur.md` 6.4) |
| `rigg/src/settings/grafer.rs` | Delen «Grafer og sensorer» |
| `app/data/theme/oversikt.css`, `sensorer.css`, `grafer.css` | Stil |
| `rigg/tests/chart.rs` | Tester for nedsampling, akser og hull |

**Oppgaver:** Oversikt (seks nøkkeltall med sparklinje, kortet, målere, klokker, tre grafer med eget tidsvindu). Sensorer (én tabell gruppert etter enhet med Nå/Min/Maks/Snitt/60 s, filter, festing, større graf under valgt rad, piltaster, ↺ med angre, høyreklikkmeny med «Ta med i loggen» og «Nullstill», knappen fra `logg_ui.rs` i toppfeltet). Grafer (valgfrie grafer, bånd, markør på alle eller én, pause, forklaring, hull med forklaring, «Eksporter CSV …»). Påskeegg 4 og 7. Målt CPU for tegningen.

**Rører ikke:** `widgets/spark.rs` (grunnmuren), `logg_ui.rs` (2B), `rigg-core`.

### 2B · Maskinvare og rapport: Grafikkort, System, logging, varsler, tilstander

**Mål:** GPU-Z- og CPU-Z-delen med alt fra NVML og sysfs, rapporten, loggingen og hvordan appen oppfører seg når kortet eller driveren svikter.

| Eier | |
|---|---|
| `rigg/src/pages/grafikkort.rs`, `system.rs` | Sidene |
| `rigg/src/rapport_ui.rs`, `logg_ui.rs` | Kopier, Lagre rapport, loggknappen og loggstatusen i sidefeltet |
| `rigg/src/settings/logging.rs` | Delen «Logging og varsler» |
| `rigg-core/src/specs.rs`, `report.rs`, `csv.rs` | Spesifikasjonstabellen, rapporten, CSV-skriveren |
| `rigg-hw/src/gpu/nvml_info.rs`, `sys/dmi.rs`, `sys/cpuinfo.rs`, `sys/os.rs`, `sys/pci.rs` | Statiske data |
| `riggd/src/logg.rs`, `varsler.rs` | Loggen og skrivebordsvarslene |
| `riggd/tests/logg.rs`, `rigg-core/tests/rapport.rs` | Tester |
| `app/data/theme/grafikkort.css`, `system.css` | Stil |

**Oppgaver:** Grafikkort og System som i demoen, fra NVML og sysfs (også vifter og RPM), «Referanse, ikke målt», Kopier som tekst/Markdown, Lagre rapport (uten serienumre som standard). Logging til CSV i begge formatene med alle grensene i produktspesifikasjonen 3.2. Varsler ved rød grense (BØR). Visningen av tilstandene `driver_updated`, `no_driver`, `lost` i kortvelgeren og på Grafikkort, testet mot sim-scenariene. Kortvelgeren med to simulerte kort. Påskeegg 5.

**Rører ikke:** `sidebar.rs` (grunnmuren; loggstatusen settes inn med funksjonen grunnmuren har laget), `ops.rs`.

### 2C · Styring (låst) og Lys

**Mål:** Kontroller, Viftekurve og Lys med ekte verdier og den rolige låst-tilstanden, og 3D-kortet i GtkGLArea.

| Eier | |
|---|---|
| `rigg/src/pages/kontroller.rs`, `viftekurve.rs`, `lys.rs` | Sidene |
| `rigg/src/widgets/locked.rs` | Låst-banneret, «Hva skal til?», lesevisningen av verdier med område-spor |
| `rigg/src/rgb.rs`, `rigg/src/lys3d/` (`mod.rs`, `renderer.rs`, `mesh.rs`, `model.rs`, `layout.rs`, `textures.rs`, `flat.rs`), `app/data/lys3d/*.glsl` | RGB-motoren, 3D og det flate kortet |
| `rigg-core/src/txn.rs` | Bekreft-eller-rull-tilbake for sim-backenden |
| `app/data/theme/kontroller.css`, `viftekurve.css`, `lys.css` | Stil |
| `rigg/tests/locked.rs` | At ingen Bruk er aktiv med NVML-backenden |

**Oppgaver:** Låst-tilstanden i produktspesifikasjonen kap. 6, med trinnene fra `capabilities.write`. Kontroller med alle verdiene i 6.4. Viftekurve med punktskyen fra historikken (6.5). Lys med 3D (port av `web/js/lys3d/`), lag, eksplodert visning, effektene, «Lagre i appen», reserven og Nordlys (egg 10). Utkast (BØR) med egg 8. Bekreftelsesflyten mot sim (BØR), skjult bak `RIGG_PROFIL=dev`.

**Rører ikke:** `widgets/chart.rs` (2A; bruk den for grafene på Kontroller), `topbar.rs` (låsemerket settes med funksjonen grunnmuren har laget).

---

## Runde 3 · Optimalisering og bakgrunn

### 3A · Optimalisering og integrasjon (én agent, alle filer unntatt 3B sine)

- Mål alt i produktspesifikasjonen kap. 9 med `maal.sh`. Finn og rett det som er over kravene.
- Velg tegner (`ngl`, `vulkan`, `cairo`) etter målinger, og vurder om «rask gjenåpning» trengs (`arkitektur.md` 3.4).
- Mål prøveplanen med kortet i P8 og P0, og sjekk med `verktoy/nvml_timing.c watch` at tjenesten aldri vekker kortet fra P8.
- Kjør tjenesten i 1 t med vinduet av og på, og se etter lekkasjer (privat minne skal være flatt).
- Full skjermbildeserie: alle sider × lyst/mørkt × fire temaer, høy kontrast, 640 px og 1920 px, med `bilder.sh`.
- Gjennomgang av at ingen tekst sier «simulert», at alle demoens taster virker, og at alle ti påskeeggene virker.

### 3B · Bakgrunn (valgfri, kan gå samtidig med 3A)

| Eier | |
|---|---|
| `riggd/src/tray.rs` | Systemfeltet (BØR) |
| Gruppa «Bakgrunn» i `rigg/src/settings/generelt.rs` | Bare den gruppa; 3A rører ikke fila i denne runden |

Systemfeltet med ikon, verktøytips og meny, bare når tjenesten går i bakgrunnen. «Start tjenesten ved innlogging» med `EnableUnitFiles`/`DisableUnitFiles` over D-Bus, prøvd bare i en privat økt. Ingenting slås på i brukerens ekte økt.

---

## Runde 4 · Kritikk og finpuss

- **4A Kritikk** (bare lesing, ny agent): går gjennom appen med friske øyne mot demoen og produktspesifikasjonen, og skriver `docs/produksjon/kritikk-1.0.0.md` med funn, skjermbilder og prioritet.
- **4B Finpuss** (alle filer): retter funnene, gjør en siste runde med detaljer, og skriver `app/README.md` og første del av STATUS og PATCHLOG for 1.0.0 · bygg 16.

## Runde 5 · Installasjon

Hovedøkta kjører `install.sh` når brukeren har sagt ja, og sjekker appen i «Vis programmer», ikonet i panelet, kald og varm start, og lukking. Spørsmålene i produktspesifikasjonen kap. 10 bør være besvart før dette.

## Senere, med brukerens godkjenning

Root-hjelperen og skrivetrinnene (effektgrense først), profiler som legges på, profiler per program, globale hurtigtaster, MangoHud-oppsett, AMD og Intel, feilsøkingspakke.
