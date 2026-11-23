
## 2026-11-07 · Appikon og varianter (ikonagenten)

- Nytt appikon «Vifte i måler»: skjermkortvifte i en måleskala med PCIe-kontakt, på samme flis som Klipp. Valgt blant fire skisser (vifte, skjermkort, vifte i måler, brikke) og en runde til med antall blad. Håndtegnede utgaver for 16, 24, 32 og 48 px, og symbolsk ikon. Filer i `app/data/icons/`, valg og mål i `ikon.md`.
- Ti varianter (Standard, de seks aksentfargene med eksakte `--accent-bg`, Mørk, Lys, Monokrom) fra én mal per størrelse med `lag-varianter.py` (`--sjekk` til tester, `--forhandsvis` til PNG). Forhåndsvisninger i `ikon/`.
- Testet i en egen hodeløs GNOME Shell 50.1 med Ubuntu-dokken: å skrive over ikonfila og touche `hicolor/` bytter **ikke** ikonet. Å skrive om `Icon=` i `.desktop` til et annet ikonnavn bytter det etter ca. 5 s uten utlogging, hvis temaet kjenner navnet (installert på forhånd, eller `hicolor/` touchet). GTK 4.22 målt i brukerens ekte `~/.local` med et midlertidig navn, som er fjernet igjen.
- Anbefaling: installer alle variantene med egne navn, bytt med `Icon=`, og bruk `touch` i stedet for `gtk-update-icon-cache` i `~/.local`. En gyldig buffer gjør ikoner som andre apper legger til senere, usynlige. `arkitektur.md` kap. 12 punkt 3 og 6 bør justeres (se `ikon.md` kap. 10).
- Ingen sudo og ingenting installert. Klipps ikoner, andre apper og brukerens Shell er ikke rørt. Den eneste varige endringen utenfor prosjektet er mtime på `~/.local/share/icons/hicolor/`. Den hodeløse Shell meldte seg til GDM ved oppstart (idempotent kall, to oppstarter).

## 2026-11-07 · Maskinvarerevisjon (lese-agent)

- Skrev `maskinvare-revisjon.md`: hver verdi på alle sidene sjekket mot kort og driver, med kilde, støtte, målt verdi, kost, intervall, demostatus og hva produksjon skal vise. Kapitlene 10 og 12 dekker det som mangler uten root og forslaget til polling.
- La tre lese-verktøy i `verktoy/`: `nvml_probe.py` (92 NVML-get-funksjoner), `nvml_timing.c` (oppstart, RSS, pollekost, om pollingen vekker kortet) og `sys_probe.py` (/proc og /sys). Ingenting er skrevet til skjermkortet, og ingen I2C-enhet er åpnet.
- Viktigste tall: `nvmlInit_v2` 12–21 ms, handle 6–8 ms, NVML +24 MB RSS. Demorunden på 19 kall tar 54–62 ms i P8 og 10–11 ms i P0, og det er ren CPU-tid. Klokkehendelser alene koster 25 ms i P8. Tett polling løfter kortet til P0 (55–69 W). Ved 1–2 Hz blir det i P8.
- Funn: viftens turtall og to vifter virker via NVML. Demoens CPU-temp er Tctl (+10 °C). SPD-filene (`ee1004`) er lesbare for alle, men ikke lest. **USB-C-kontrolleren ligger nå på `i2c-10`, ikke `i2c-8`, så regelen i REGLER.md peker på feil buss.**

## 2026-11-07 · Produkt og arkitektur

- Skrev `produktspesifikasjon.md` (hva ekte verktøy gjør, krav merket MÅ/BØR/SENERE, hva hvert demotrekk blir i produksjon, den låste tilstanden for Kontroller, Viftekurve og Lys, påskeeggene, ytelsesmål og åpne spørsmål til brukeren), `arkitektur.md` (prosessmodell, crates, tema, grafer, 3D, innstillinger, protokoll, sikkerhet, tester, installasjon, versjon) og `byggeplan.md` (runde 1–5 med eierskap per fil).
- Valg: tjeneste `agentic-riggd` (systemd `--user`, GLib-løkke og én prøvetråd, NVML bare der) og vindu `agentic-rigg` (rent GTK4, ikke libadwaita). Lukk = vinduets prosess avslutter. Opptak i bakgrunnen er et valg (1, 2 eller 5 s), av til brukeren har bestemt seg.
- Målt (bare lesing, egne prosesser): `nvmlInit_v2` 12 ms og +20 MB privat minne. En EGL-kontekst på NVIDIA tar 117 ms og gir +18 MB privat minne, som ikke frigjøres. `Adw.init()` tar 118–121 ms og trekker inn 16 ekstra bibliotek. Klipp i ro bruker 26 MB privat minne (78 MB RSS). Tallene står i `arkitektur.md` kap. 2. Prøveplanen følger `maskinvare-revisjon.md` kap. 12.
- Ingenting er bygd, ingen tjenester er startet, og ingenting er skrevet til kortet. Testprosessene var Python-skript og en egen `gtk4-broadwayd` (stoppet etterpå).

## 2026-11-07 · Runde 1 · Grunnmur

- **Arbeidsområdet** `app/`: `rigg-core`, `rigg-hw`, `riggd` (`agentic-riggd`) og `rigg` (`agentic-rigg`), `version.json` (1.0.0 · bygg 16, lest av `rigg-core/build.rs`), `clippy.toml`, release-profil som Klipp uten `panic = "abort"`. `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer (65 tester + 2 `#[ignore]` mot det ekte kortet, som også er kjørt).
- **rigg-core:** historikk (port av `history.js` med testtilfellene, binær `historikk.bin`, takt per tidsrom), min/maks/snitt, innstillinger (port av `mergeSettings`/`lenientSettings`, import av en ekte eksport fra demoen i `tests/demo-eksport.json`), `fmt`, sensornavn (Tdie som CPU-temp), sensornøkler, protokolltypene, påskeeggenes logikk, og tokens med kontrasttesten for alle 96 kombinasjonene (leser `data/theme/tokens.css`; alle par består).
- **rigg-hw:** egen lesebinding til NVML med `dlopen` i stedet for `nvml-wrapper` (avvik: færre avhengigheter, og `NvmlLib::sym` avviser alt som ikke er `nvmlDeviceGet*`/`nvmlSystemGet*`, så skrivefunksjoner ikke kan slås opp). Prøveplanen fra revisjonen (energiteller, `MemoryInfo_v2`, PCIe fra sysfs, klokkehendelser bare utenfor P8, NOT_SUPPORTED huskes, faste data fordelt over de første rundene), tilstandene `driver_updated`/`no_driver`/`lost`, sim-backenden med seks scenarier, `/proc` og `/sys` med `pread`, fixtures fra maskinen, og `tests/bare_lesing.rs`.
- **riggd:** GLib-løkke, én prøvetråd (panikk fanges), hele leseprotokollen (`docs/produksjon/protokoll.md`), takt etter abonnement og fokus (aldri over 2 Hz, ingen etterslep), historikk og min/maks lagret, 60 s frist, `Type=notify`, `SO_PEERCRED`, `write_locked`, dvale via logind. Kontrakttestene i `riggd/tests/kontrakt.rs` (9).
- **rigg:** skallet med sidefelt (kortvelger, tre seksjoner med lås på Styring, statusblokk, fot), ikonlist (Ctrl+B og under 860 px), toppfelt med GNOMEs vinduskontroller, linjene under toppfeltet, varsler, tastatursnarveier (egen dialog), temaene som klasser på vinduet, bevegelsesnivåene, klienten (start via `StartUnit`, reserve som egen prosess, dev-profilen rører aldri systemd), Innstillinger i hele vinduet (navigasjon, søk med markering, tilbake på tre måter) med Generelt, Utseende (med appikonet etter `ikon.md`), Enheter og format, Sidefelt, Data og Om, og en enkel Oversikt. Alle åtte sidene finnes; stubbene sier «… kommer i neste runde».
- **Skript:** `install.sh`/`uninstall.sh` (testet bare med midlertidig HOME og `RIGG_INSTALL_NO_SYSTEMD=1`), `scripts/okt.sh` (privat hodeløs GNOME-økt med egen økt- og systembuss, så Shell ikke melder seg til GDM), `scripts/bilder.sh`, `scripts/maal.sh`, `scripts/ikoner.py`, `scripts/hent-fixtures.sh`.
- **Målt** (`maalinger.md`): første bilde 410 ms med Vulkan (GTKs standard), 280 ms med GL og 160 ms med cairo. Vinduet 48–62 MB privat (Vulkan), 10 MB med cairo. Tjenesten 20–21 MB privat, 1,35 % CPU med 2 s-takt i bakgrunnen (kravet er 1,2 %), 2,2 % med vinduet åpent. Vinduet 0,8 % på Oversikt. To krav er ikke nådd og står som oppgaver for 3A.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde1/` (19 bilder, ekte data fra kortet).
- Ingenting er skrevet til skjermkortet, ingen I2C er åpnet, ingen systemd-enhet er rørt, ingen av brukerens prosesser er stoppet, og demoen på 4318 er ikke rørt. Alle prøveøkter kjørte med egen HOME under `~/.cache/rigg-build/okt` og er slettet.
- **Ønsker til hovedøkta:** (1) 3A bør vurdere `cairo` som standardtegner (tallene over). (2) Bakgrunnstakten bør lese de trege verdiene sjeldnere enn hvert 2. s når ingen vindu er tilkoblet. (3) Appikonraden bruker `min-children-per-line 5` som `ikon.md` sier; vinduet når likevel 640 px.

## 2026-11-07 · Runde 2B · Maskinvare og rapport

- **Grafikkort (GPU-Z):** alt fra NVML og sysfs i seksjoner (Fra driveren, Minne og buss, Klokker med stolper, P-tilstander med klokkeområde, Grenser med offset og område, Vifter og kjøling med fart, turtall og mål per vifte, Ikke støttet, Prosesser som bruker skjermkortet, Spesifikasjon «Referanse, ikke målt»). UUID maskert med øye og kopiknapp. 7 nye steg i `gpu/nvml_info.rs` (bare `NvmlLib`-metoder, strukturene kontrollert mot `verktoy/nvml_probe.py`), fordelt over de første rundene. Tilstandene `driver_updated`, `no_driver`, `lost` og dvale står øverst på siden, testet mot sim-scenariene.
- **System (CPU-Z):** prosessor med kodenavn (oppslag), CCX, P-tilstander, Tdie (Tctl i verktøytipset), cache, instruksjonssett som brikker med søk, hovedkort og BIOS med brikkesett fra `pci.ids`, minne med stolper og `Shmem` skilt ut, operativsystem, disker (ny) og klokke og last per tråd. Root-verdiene sier «Krever root» med grunn. **SPD-filene er ikke lest**; bare navnene på `ee1004`-enhetene telles («Krever godkjenning»).
- **Rapport:** `rigg-core::report` bygger sidene og rapporten fra de samme seksjonene. «Kopier som tekst / Markdown» på begge sidene, «Lagre rapport …» (toppfeltet og Om) med valget «Ta med serienumre og UUID» (av). Vertsnavnet er heller ikke med som standard.
- **Logging:** `riggd/src/logg.rs` skriver CSV i begge formatene (komma/punktum og semikolon/komma), låste kolonner, tomt felt når en verdi mangler, buffer tømt hvert 5. s, `…-del2.csv` ved 100 MB, stopp under 500 MB ledig. Vinduet: knappen på Sensorer (Ctrl+L overalt), prikken og tiden i sidefeltet, varsler med «Vis i Filer», og Innstillinger › Logging og varsler.
- **Varsler:** `riggd/src/varsler.rs` sender skrivebordsvarsel etter 10 s over rød grense, med 3 °C hysterese og høyst ett per verdi per 15 min, og ved overgangene for kortet. Av som standard. Klikk åpner appen på Sensorer via appens handling `vis-side` (lagt inn i `logg_ui::attach`).
- **Målt:** tjenesten 1,33–1,98 % CPU og 21,1–21,5 MB privat minne med og uten logg; loggen koster ikke målbart (`maalinger.md`, runde 2B).
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde2b/` (28 bilder og en ekte rapport som `.txt` og `.md`), tatt i den private økta med `logg_ui/bilder.rs` (bare debug, `RIGG_SKJERMBILDER_2B`).
- **Avvik:** én linje i `riggd/tests/kontrakt.rs` (grunnmuren sin) er endret: den ventet `not_implemented` fra `log_start`, og med ekte logging ville den ha skrevet til `~/Dokumenter`. Nå starter og stopper den loggen i en midlertidig mappe.
- Ingenting er skrevet til kortet, ingen I2C og ingen SPD er lest, ingen av brukerens prosesser er rørt, og alle prøveøkter kjørte med egen HOME under `~/.cache/rigg-build/okt-2b`.
- **Ønsker til hovedøkta:**
  1. `riggd/src/schedule.rs`: mens loggen går, bør takten være minst så rask som loggintervallet (aldri under 0,5 s). Nå får en 1 s-logg en rad hvert 2. s når vinduet er lukket. `daemon.rs` bør også kalle `check_idle()` etter en `log`-hendelse fra `Logger::tick` (loggen kan stoppe av seg selv ved full disk).
  2. `rigg/src/state.rs` (`ingest`): ikke nullstill valgt kort når det mangler i prøven. Når kortet er `lost`, blir `app.gpu_id()` og `app.statics()` `None`; Grafikkort går rundt det, men andre sider (2A, 2C) får samme problem.
  3. `rigg/src/widgets/ikoner_data.rs`: Lucide `circle-dot` mangler (ikonet spesifikasjonen gir loggknappen). `logg_ui.rs` tegner det selv til da.
  4. `rigg-hw/src/gpu/sim.rs`: fyll `GpuStatic.extra` med eksempeldata, så Grafikkort viser de samme radene mot sim som mot kortet (nå «–» for brand, GSP, P-tilstander osv.).
  5. En operasjon `get_processes {gpu}` i `ops.rs`/`sampler.rs`, så prosesslista kan oppdateres mens Grafikkort vises (høyst hvert 5. s). Leseren finnes: `nvml_info::processes(lib, dev)`. I dag er lista et øyeblikksbilde fra de faste dataene, og siden sier det.
  6. `rigg/src/skjermbilder.rs`: 2B-planen ligger i `logg_ui/bilder.rs` og kan flyttes inn dit.

## 2026-11-07 · Runde 2C · Styring (låst) og Lys

- **Låst-tilstanden** (`rigg/src/widgets/locked.rs`): banneret «Endringer er låst» (kan legges sammen til én linje, huskes i `dismissed`), popoveren «Hva skal til?» med de seks trinnene fra `capabilities.write`, merket «Låst» i toppfeltet, og lesevisninger med område-spor (markør, standard, grensene under, tjenestens policy som mørkere felt, kilde og «Endret utenfor appen» i verktøytipset, kort aksentmarkering ved endring). Ingen grå glidere. Porten `send_write` sender en skriveoperasjon bare når `txn::write_gate` er `Sim` (dev-profil + `backend: sim` + `write.state: sim`); ellers får kalleren `write_locked` uten at noe går til tjenesten. Ctrl+Z og Ctrl+Enter gjør ingenting i lesevisningen.
- **Kontroller:** effektgrense (håndhevet, 105–275 W, standard), kjerne- og minneoffset, temperaturmål med nedklokking og avstenging, vifte (fart og o/min per vifte, mål, minste), sikkerhetsreglene beskrevet, grafer for effekt (med grensen som stiplet trapp) og kjerneklokke for siste minutt, «Nå», klokkebegrensninger som brikker med tidspunkt, de fem profilene (Hent, lagre over, gi nytt navn, tøm med Angre) og «Støttes ikke på dette kortet». **Utkast** (BØR): «Lag utkast» gjør radene om til glidere og tallfelt med «Utkast · ikke sendt til kortet», endringsliste, «Forkast utkast» med Angre og «Bruk» låst med «Skriving er ikke slått på». Utkastet deles med Viftekurve og lagres i `profiler.json`.
- **Viftekurve:** «Slik styrer driveren viften»: punktsky av viftefart mot GPU-temperatur fra historikken (1 t / 6 t / 24 t / 48 t, standard 48 t), median per grad, ring for «nå», markør som leser av medianen, sone under 35 %. Til høyre: vifte nå, siste døgn («Viften har gått på 35 % i 86 % av tiden siste døgn») og «Når skriving blir slått på» (ikke aktive). Utkast med egen kurve oppå punktskyen (dra, dobbeltklikk, [ ] og piltaster, Delete), forhåndsvalgene Stille, Balansert og Ytelse, tallfelt per punkt og **påskeegg 8** (Jetmotor/Bibliotek).
- **Bekreft eller rull tilbake** (`rigg-core/src/txn.rs`): port av `daemon-mock/card.js` uten tidtakere: én ventende endring, bare eieren kan beholde, frist 10–60 s, frakobling og fristen ruller tilbake, `reset` bekreftes straks, klemming etter kort og policy med kilde, kurven kontrolleres og løftes. `Book::handle` svarer `write_locked` for alt annet enn `backend == "sim"`. 14 tester. Vinduet har Behold/Rull tilbake-linja (`ConfirmBar`) klar, men tjenesten svarer fortsatt `not_implemented` med sim (ønske 1).
- **Lys:** 3D-kortet portet fra `web/js/lys3d/` til `GtkGLArea` med GLES 3.0 via `glow` (funksjonspekere fra libepoxy): skyggeleggerne nesten uendret (`app/data/lys3d/*.glsl`, bare ASCII), modellen, oppsettet og teksturene (tegnet med cairo i en egen tråd), skyggekart, gulv, glorier, bakgrunnslys, 4 × MSAA, plukking på én piksel. Vinkler, lagtrinnene Hel/Uten deksel/Uten kjøler/Bare kretskort, «Skill lagene», navn på delen under pekeren, dellisten, klikk på lyssonene, vifter som følger viftefarten, piltaster/+/−/0, treghet. **RGB-motoren** portet (`rgb.rs`, 6 tester). **Vinkel, zoom, lagtrinn, «Skill lagene», 2D/3D og sone huskes mellom økter** (`profiler.json`). **Reserven** (`lys3d/flat.rs`): det flate kortet med `snapshot()`, viftene og kortet bufret som teksturer; brukes når GL feiler (med varsel), når du velger 2D, og hvis grafikken er for treg. **Høyre kolonne i 2D** er fylt med «Soner» (farge, antall lysdioder, hex, klikk velger sone) og «Lyskontrolleren» (hva som er kjent, at den ikke er lest, og hvorfor). Banneret sier rett ut at lyskontrolleren ikke er lest. «Lagre i appen» med «Lysvalget er lagret i appen. Lyskontrolleren er ikke koblet til.» **Nordlys** (egg 10) virker i 3D og 2D, Esc avslutter.
- **GL under cairo:** GtkGLArea virker med `GSK_RENDERER=cairo` i GTK 4.22, men bildet lastes ned hvert bilde: 61–67 % av én kjerne ved 60 bilder/s mot 15 % med `ngl` og 44–49 % med Vulkan. Tallene og anbefalingen til runde 3 står i `maalinger.md` (runde 2C).
- **Målt** (release, privat økt): 3D 59,6–60 bilder/s med regnbue og når kameraet dreies (Vulkan), 59,5–59,7 (ngl), 58,9–59,3 (cairo); vår tegning 0,3–0,45 ms CPU per bilde, helt bilde på GPU-en 1,8–2,0 ms; GPU for vinduet 22–37 % i P8 (kortet 16–18 W mot 11,5 W i ro); 0 bilder i ro med redusert bevegelse og når Lys er skjult; 2D med snurrende vifter 5,4–6 % CPU (54 % før viftene ble teksturer).
- **Tester:** `rigg/tests/locked.rs` (5): ingen Bruk aktiv med NVML eller uten `hello`, tjenesten med nvml tilbyr ingen skriveoperasjoner, boka svarer `write_locked`, ingen fil kaller en skriveoperasjon utenom porten, og Bruk kan bare bli aktiv i grenen for `Gate::Sim`. Pluss enhetstester i `txn.rs`, `rgb.rs`, `lys3d/*` og `viftekurve.rs`. `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde2c/` (23 bilder): Kontroller, Viftekurve og Lys i lyst og mørkt, «Hva skal til?», utkast, banneret lagt sammen, alle lagtrinnene, skill lagene, bakfra, temperatur, 2D, og alle tre ved 640 px. Planen er `rigg/src/pages/styring_bilder.rs` (`RIGG_BILDER_2C`, startes med `--side kontroller`).
- Ingenting er skrevet til kortet, ingen I2C er åpnet, ingen av brukerens prosesser er rørt, og alle prøveøkter kjørte mot `--backend sim` med egen HOME (til slutt i `~/.cache/rigg-build/okt-2c`).
- **Ønsker til hovedøkta:**
  1. `riggd/src/ops.rs` og `daemon.rs`: koble inn bekreftelsesflyten for sim. Legg `txn: rigg_core::txn::Book` i `Daemon`; erstatt grenen for `proto::WRITE_OPS` med `self.txn.handle(&self.opts.backend, op, a, &format!("s{sid}"), rigg_core::now_ms())` (den svarer selv `write_locked` for nvml/ingen); kall `self.txn.tick(now)` hver runde og `self.txn.disconnect(&format!("s{sid}"))` når en økt lukkes; send `take_events()` som hendelsen `txn` (`Event::to_json`). Sett `confirm_s` fra `tjeneste.json` (`confirmSec`). `get_state` kan bruke `book.state_view(now)` med sim.
  2. `rigg-hw/src/gpu/nvml_info.rs` (2B) eller `nvml.rs`, og `sim.rs`: fyll `GpuStatic.extra.styring` = `{"coreOffsetMHz": {value, min, max}, "memOffsetMHz": {value, min, max} (effektiv MHz = NVML/2), "tempTarget": {value, min, max} (THRESHOLD ACOUSTIC_CURR/MIN/MAX), "fanPolicy": [0, 0], "fanTarget": [35, 35]}` med `nvmlDeviceGetClockOffsets`, `…FanControlPolicy_v2` og `…TargetFanSpeed`. Kontroller og Viftekurve leser dette allerede og viser «Leses ikke ennå» til det finnes. Viftemodus og mål-fart bør helst følge prøven (hvert 5. s), ikke bare de faste dataene.
  3. `scripts/okt.sh`: flere agenter som kjører økter samtidig deler `~/.cache/rigg-build/okt`, og skriptet sletter mappa ved start. 2A og 2C kjørte samtidig. Standarden bør være en mappe per kjøring (f.eks. `okt-$$`), eller sett `RIGG_OKT_DIR` i `bilder.sh`/`maal.sh`.
  4. `rigg/src/skjermbilder.rs`: steget `stubb-kontroller-lys` kan tas bort (siden er ikke stubb lenger); 2C-planen ligger i `pages/styring_bilder.rs`.
  5. `widgets/chart.rs` (2A) fantes ikke da Kontroller ble laget; Kontroller har en liten egen graf (`MiniChart` i `kontroller.rs`) som runde 3 kan bytte mot `RiggChart`.
  6. Runde 3: velg tegner med Lys i tankene (se GL under cairo over). `GSK_RENDERER=ngl` gir 3D for 15 % CPU; cairo gir 61–67 % mens kortet beveger seg.
  7. Kjent advarsel: «Trying to snapshot GtkScrolledWindow … without a current allocation» én gang når 3D-visningen lages; ufarlig, ikke funnet årsaken.

## 2026-11-07 · Runde 2A · Data: Oversikt, Sensorer og Grafer

- **Grafen** (`rigg/src/widgets/chart.rs`, regnestykkene i `widgets/chart/math.rs`): port av `charts.js`, `history.js` og `chartpanel.js`. Historikk 1 min–48 t fra tjenesten (`get_history`) med de levende prøvene flettet inn (bøtter i lange vinduer), linjer og bånd som `GskPath`, rutenett og tekst på hele piksler (skarp med `GSK_RENDERER=cairo`), bufret bakgrunnsnode, høyst ett punkt per pikselkolonne (snitt for linja, laveste min og høyeste maks for båndet). Hull skraveres, og musa over hullet sier hvorfor («Hull i grafen: appen var lukket» når «Fortsett i bakgrunnen» er av). Takten i bakgrunnen (2/5 s) gir ikke brudd. Pause per graf (knapp og mellomrom), markøren på alle grafene eller én (`chartSync`), forklaring som skjuler serier, faste seriefarger, min/maks i overskriften og snitt (min–maks) i verktøytipset.
- **Tegner bare når det trengs:** «nå» rundes ned til hele piksler, og grafene vekkes på et felles rutenett i tid når tidsaksen har gått minst én piksel, høyst hvert 400. ms (1 min: 2–3 piksler om gangen; 48 t: hvert 3. min), så grafene på en side tegnes i samme bilde. Lange vinduer tegnes høyst hvert 5. s for nye prøver. Tidtakeren lever bare mens grafen er kartlagt (`map`/`unmap`), står stille på pause, og venter når vinduet er minimert. «Redusert»/«Av» bevegelse: bare ved nye prøver.
- **Rettet fra demoen:** (1) Båndet for minneklokka ved 48 t ble tunge blå blokker. Nå tegnes båndet i stykker med styrke etter lokal høyde (`math::band_runs`): et bånd over hele plottet får en firedel av styrken, et smalt bånd full styrke. (2) Flisene på Oversikt viser alltid siste minutt, fylt fra historikken ved tilkobling, uansett tidsvinduet for grafene.
- **Oversikt:** seks fliser med sparklinje og varselfarge, Skjermkort (PCIe med forklaring, P-tilstand med klokkebegrensning, viftestyring, VRAM-stolpe, video), «Temperatur og effekt» med målerne (`widgets/gauge.rs`: fyll til verdien, merker for mål, nedklokking, grense, tallene krasjer aldri), Klokker med stolper (`widgets/bar.rs`), og tre grafer med eget tidsvindu (`windows.oversikt`). Det kortet ikke støtter, står som «Ikke støttet» med forklaring. Påskeegg 4 (42 °C).
- **Sensorer:** én tabell i HWiNFO-stil (Skjermkort per kort, Prosessor med Tdie/Tctl og per tråd med MHz, Minne, Andre sensorer, Nett), Nå/Min/Maks/Snitt fra tjenestens statistikk, 60 s-sparklinje (fylles fra historikken), overskrift som klistrer seg øverst, filter (Ctrl+F) med påskeegg 7, sammenlegging, festing i statusblokken, graf under raden med eget tidsvindu (`windows.sensorer`), piltaster med én Tab-stopp per gruppe, ↺/Ctrl+R med Angre i 5 s (tjenesten får `reset_stats` først etter fristen), høyreklikk med «Ta med i loggen», «Nullstill min og maks» og rekkefølgen i statusblokken, knappen fra `logg_ui.rs` i toppfeltet. «Ikke tilgjengelig» sier hvorfor hotspot, minnetemperatur, spenning og hovedkortet mangler. NOT_SUPPORTED-felt står som «Ikke støttet» i raden.
- **Grafer:** tidsvindu (segmentknapp, nedtrekksliste når toppfeltet er smalt), «Pause alle» og mellomrom, markør, velg grafer, «Eksporter CSV …» (vinduet og seriene du ser, min/snitt/maks per serie, `Tid` i ISO 8601 med sone og `Sekunder`, formatet fra loggvalget).
- **Innstillinger › Grafer og sensorer:** tidsvindu, markør, y-akse, fylt flate, grafer som vises, sparklinjer, «Husk min og maks», varselfargene.
- **Tester:** `rigg/tests/chart.rs` (15): nedsampling og topper, båndet, hull og takt, akser, tidsaksen, markøren, tekstene, måleren og CSV.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde2a/` fra private, hodeløse økter (`okt.sh` med egen `RIGG_OKT_DIR`): ekte kort (NVML, bare lesing) for 1 min, og `--backend sim --syntetisk-historikk` for 24/48 t, pause og markør. Stegene ligger i `pages/oversikt/bilder.rs` (`RIGG_SKJERMBILDER_2A`, bare i debug-bygget).
- **Målt** (`maalinger.md`, runde 2A): vinduet i ro på Oversikt (1 min) 2,28 % med cairo og 1,80 % med Vulkan; Sensorer 2,13/1,15 %; Grafer 1 min 1,40/2,10 %; Grafer 48 t 0,60 %. Grafene tegnes på et felles rutenett, høyst hvert 400. ms, alltid på hele piksler. Med cairo er Oversikt 0,3 prosentpoeng over kravet på 2 %; resten ligger i bildet som tegnes hvert sekund (ca. 13 ms med cairo), se oppgaven for 3A der.
- `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer. Ingenting er skrevet til kortet; alle økter var private og egne prosesser.
- **Ønsker til filer andre eier:**
  1. `sidebar.rs` (grunnmuren): klikk på en rad i statusblokken bør gå til `sensorer?vis=<id>` (Sensorer åpner gruppa, ruller dit og lyser opp raden; både faste ID-er som `gpuTemp` og nøkler virker). Festede nøkler bør få navn fra `crate::pages::sensorer::short_name(key)` («Vifte 1 o/min», ikke «fanRpm0»).
  2. `logg_ui.rs` (2B): Ctrl+L er ikke koblet på Sensorer. Foreslår at `logg_ui` eier tasten (f.eks. `logg_ui::toggle(app)` som Sensorer kaller fra `on_key`). «Logg til fil» med tekst gjør toppfeltet trangt ved 640 px; ikon alene der ville hjulpet.
  3. `skjermbilder.rs` (grunnmuren/3A): stegene for 2A ligger i `pages/oversikt/bilder.rs` (`RIGG_SKJERMBILDER_2A`, `RIGG_2A_DEL=historikk` for sim med syntetisk 48 t). Kan flyttes inn i den felles planen. Merk: `okt.sh` bruker én fast mappe som standard; parallelle agenter bør sette `RIGG_OKT_DIR`.
  4. `window.rs` (grunnmuren): sidestakken og den ytre stakken (app/Innstillinger) er homogene, så den bredeste siden (også Innstillinger) bestemmer vinduets minste bredde. Mine sider holder seg under 640 px, men `set_hhomogeneous(false)` ville gjort det robust.
  5. `rigg-core/model.rs` og `rigg-hw` (hovedøkta): Oversikt viser «Auto · driveren styrer» fordi viftestyringen (`nvmlDeviceGetFanControlPolicy_v2`) ikke er med i prøven. Et felt som `fanPolicy` ville gjort raden ekte.

## 2026-11-07 · Runde 3A · Integrasjon

Ønskene fra 2A, 2B og 2C er koblet inn. Alle filer unntatt `riggd/src/tray.rs` og gruppa «Bakgrunn» i `settings/generelt.rs` (3B).

- [x] **Skrivestien mot sim** (2C ønske 1). `Daemon` har `txn: rigg_core::txn::Book` (`confirm_s` fra `confirmSec`). Grenen for `WRITE_OPS` i `ops.rs` svarer selv `write_locked` for alt annet enn `sim` (to lås: der og i `Book::handle`), ellers `self.txn.handle(backend, op, args, "s{sid}", now)`. `tick(now)` hver runde, `disconnect("s{sid}")` når en økt lukkes, `take_events()` som hendelsen `txn` (ny i `EVENTS`/`TOPICS`). `get_state` gir boka med sim. Prøven fra sim viser det som er i bruk (effektgrense, manuell fart eller kurven via `txn::curve_at`), og `extra.styring` følger offset og temperaturmål (vinduet henter de faste dataene igjen etter `device`). `ConfirmBar` lytter på `app.on_txn` og lukker seg når tjenesten ruller tilbake. Kontrakttester: `skriving_er_låst_mot_det_ekte_kortet` (`--backend nvml` og `ingen`, alle ti skriveoperasjonene med gyldige argumenter gir `write_locked`, ingenting venter, ingen `txn`) og `bruk_behold_og_rull_tilbake_mot_sim` (Bruk → Behold, Bruk → Rull tilbake, `busy`/`not_owner`/`txn_mismatch`/`no_pending`, klemming med kilde, frakobling og fristen). `rigg/tests/locked.rs` og `bare_lesing.rs` passerer uendret. Ende til ende i vinduet: `sim-kontroller-*.png`.
- [x] **`extra.styring`** (2C ønske 2, 2A ønske 5). `nvml_info.rs` steg 4 leser også ACOUSTIC_CURR; etter steg 5 bygger `nvml_info::styring()` `{coreOffsetMHz, memOffsetMHz (effektiv MHz = NVML/2), tempTarget, fanPolicy, fanTarget}` av det som er lest (bare `NvmlLib`-get). `GpuSample` har nye `fanPolicy` og `fanTarget` (hvert 5. s, `nvmlDeviceGetFanControlPolicy_v2`/`…TargetFanSpeed`), og `model::fan_control()`. Oversikt («Viftestyring» med målfart i verktøytipset), Kontroller og Viftekurve bruker prøven først, de faste dataene ellers. «Leses ikke ennå» er borte mot det ekte kortet (`kontroller-lys.png`: 0 MHz · −1000…+1000, 0 MHz · −1000…+3000, 83 °C · 65–88).
- [x] **Takten mens loggen går** (2B ønske 1). `schedule::interval(…, log_ms)`: minst så rask som loggintervallet, aldri under 500 ms. `reschedule()` etter `log_start`/`log_stop` og etter en `log`-hendelse fra `tick`, og `check_idle()` etter den. Test i `schedule.rs` og kontrakttesten `loggen_bestemmer_takten`.
- [x] **`state.rs` `ingest`** (2B ønske 2). `state::pick_gpu()`: et kort som mangler i prøven, men står i lista, beholder valget. Test `kortet_som_er_borte_beholder_valget`, kontrakttest `kortet_borte_står_i_lista` (sim `borte`). Gått gjennom sidene med `sim:borte` (`borte-*.png`): Oversikt tømmer målere og stolper i stedet for å vise gamle tall; Sensorer beholder gruppa Skjermkort med «–» og merket «Svarer ikke»; Kontroller viser «–» for det som leses fra kortet (områdene står) og «Kortet svarer ikke» under Begrensninger; Viftekurve og Lys virker; Grafikkort som i 2B. Kortet kommer tilbake (`tilbake-oversikt-mork.png`).
- [x] **`get_processes {gpu}`** (2B ønske 5). Ny leseoperasjon; prøvetråden leser lista (`GpuBackend::processes`, `nvml_info::processes`) hvert 5. s i 15 s etter siste forespørsel, ellers aldri. Grafikkort spør hvert 5. s bare mens siden vises; merknaden sier «Lest kl. … Lista oppdateres hvert 5. s mens siden vises.» (rapporten beholder øyeblikksbildet). Tester: `prosesslista_leses_mens_den_er_bedt_om`, `prosesslista_sier_om_den_lever`, `faste_data_og_prosesser_som_fra_kortet`.
- [x] **Sim fyller `extra`** (2B ønske 4): samme felt som RTX 2080 gir (rapport-eksempelet), med `styring` og prosesser.
- [x] **Sidefeltet** (2A ønske 1). Klikk og «Vis på Sensorer» går til `sensorer?vis=<id>`. Festede nøkler får navnet fra `pages::sensorer::short_name` («Vifte 1 o/min» blir «Turtall 1» i blokken, fullt navn i verktøytipset). Loggindikatoren var koblet (2B).
- [x] **Ctrl+L** (2A ønske 2). Skallets taster (`shortcuts.rs`, fangefasen) kaller `logg_ui::toggle(app)` på alle sider og i Innstillinger; den gamle lytteren i boblefasen er fjernet. Loggknappen viser bare ikonet under 900 px (`logg_ui::set_compact`). Lucide `circle-dot` er i `ikoner_data.rs` via `scripts/ikoner.py` (`EKSTRA`, demoen er ikke endret).
- [x] **`window.rs`** (2A ønske 4): `set_hhomogeneous(false)`/`set_vhomogeneous(false)` på sidestakken og den ytre stakken. Vinduet virker ved 640 px (`*-640-*.png`).
- [x] **Kontroller-grafene** (2C ønske 5): `MiniChart` er fjernet; effekt (med grensen) og kjerneklokke er `chart::def_panel` i en `ChartSet`, i `Cols` som legger dem under hverandre under 660 px.
- [x] **Skript** (2C ønske 3/4, 2A ønske 3, 2B ønske 6). `okt.sh` lager en ny mappe per kjøring (`~/.cache/rigg-build/okt.XXXXXX`), sletter bare sin egen, og tar med alle `RIGG_*`, `GSK_*` og `RUST_BACKTRACE`. `bilder.sh --plan …` er én runner for alle planene (grunnmur, 3a, 3a-sim, 3a-borte, 3a-egg, 2a, 2a-historikk, 2b-kort, 2b-<tilstand>, 2c, eller `alle`), hver i egen økt med riktig backend og egen undermappe; planene fra runde 2 ligger der de er. Steget `stubb-kontroller-lys` er fjernet, og stubbsiden (`pages/stub.rs`) med CSS-en.
- [x] **Død kode** (runde 1). `#![allow(dead_code)]` og `#![allow(clippy::type_complexity)]` i `rigg/src/main.rs` er fjernet: ubrukte funksjoner og felt er slettet, tilbakekall bruker `widgets::Slot<dyn Fn…>`/`Listeners`, og lange typer har egne navn (ingen `type_complexity`-unntak trengtes). `absurd_extreme_comparisons` i `nvml.rs` og `unused_imports` i `easter.rs` er borte. Igjen: `#[allow(dead_code)] mod tray;` i `riggd/src/main.rs` (3B kobler inn systemfeltet), og `#[allow(dead_code)]` på `math`-modulen i `rigg/tests/chart.rs` (fila tas inn med `#[path]`).
- [x] **GTK-advarselen på Lys** (2C ønske 7). Årsak: `CardView::on_ready` kalles fra GLAreas tegning (midt i snapshot), og Lys bygde dellisten i sidekolonnen der, så `GtkScrolledWindow` hadde mistet allokeringen da `Columns` tegnet den. Nå går det via `idle_add_local_once`. Funnet med en midlertidig `log_set_writer_func` med tilbakesporing (fjernet). Ingen GTK-advarsler i siste kjøring av planene.
- **Feil funnet underveis:** `ReadRow::read` (`widgets/locked.rs`) fjernet en `timeout_add_local_once` som allerede hadde gått, og glib fikk panikk (appen stoppet med SIGABRT) så snart en verdi på Kontroller endret seg to ganger. Tidtakeren tømmes nå når den går. Viftekurve sa «1 målinger».
- **Gjennomgang** (byggeplanen):
  - Ingen tekst i brukerflaten sier «simulert». Med `--backend sim` står «testdata» (Om, Kopier på Grafikkort, rapporten) og bare der.
  - Tastene i snarveisdialogen har alle en handler: 1–8, ↑ ↓ i sidefeltet, Ctrl+, Ctrl+? Ctrl+B, Esc, Ctrl+F og / (Innstillinger), Ctrl+F (Sensorer), Alt+↑ ↓ (Sidefelt), mellomrom (Grafer), Ctrl+L (nå overalt), Ctrl+R, F5 (Grafikkort og System), Ctrl++ Ctrl+− Ctrl+0, Ctrl+W, Ctrl+Q.
  - Påskeeggene: alle ti er koblet til en krok (`egg_found` med `konami`, `stov`, `utvikler`, `svaret`, `bursdag`, `natt`, `kaffe`, `kallenavn`, `konsoll`, `nordlys`). 1, 7 og 10 er prøvd i vinduet (`egg-*.png`, planen `3a-egg`, tastene sendt gjennom skallets egen tastevei). 4, 5, 6 og 8 avhenger av temperatur, dato, klokke og utkast og har tester i `rigg-core/src/easter.rs` og bilder fra runde 2. 9 skrives bare når standard ut er en terminal, og det er den ikke i den private økta.
- **Tester:** `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer: 176 tester (153 før), pluss 2 `#[ignore]` mot det ekte kortet. Nye: 6 kontrakttester, `schedule`, `pick_gpu`, `styring`, sim-dataene, `curve_at`, modellen, rapporten.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde3a/` (26 bilder): ekte kort (NVML, bare lesing) for Oversikt, Kontroller, Viftekurve, Grafikkort med levende prosessliste, festede nøkler og 640 px; sim for Bruk/Behold/Rull tilbake, kort borte og påskeeggene.
- Ingenting er skrevet til kortet, ingen I2C er åpnet, ingen av brukerens prosesser er rørt. Alle økter var private (`okt.sh`) med egne mapper, og bare mine egne mapper er slettet.
- **Til optimaliseringsagenten:**
  1. Nye NVML-kall i prøven hvert 5. s: `FanControlPolicy_v2` (0,08 ms etter første) og `TargetFanSpeed` (1,6–3,2 ms per vifte i P8 ifølge revisjonen), altså opptil ca. 6 ms hvert 5. s. Mål om det løfter tjenesten over 1,2 % i bakgrunnen, og sjekk med `nvml_timing.c watch` at det ikke vekker kortet. Målfarten kan eventuelt leses bare når et vindu er koblet til.
  2. `get_processes` koster 0,7 ms per lesing og skjer bare mens Grafikkort vises.
  3. Release-binærene i `~/.cache/rigg-build/release` er fra runde 2; `bilder.sh` tar release først. Bygg release før målinger og skjermbildeserien, eller sett `RIGG_BIN`.
  4. Kontroller har nå to `RiggChart` (1 min) i stedet for `MiniChart`; mål CPU på Kontroller med cairo.

## 2026-11-07 · Runde 3B · Bakgrunn og systemfeltet

- **Systemfeltet** (`riggd/src/tray.rs`): StatusNotifierItem og `com.canonical.dbusmenu`, skrevet for hånd over GIO D-Bus. Ingen ny crate og ingen ny tråd.
  - **Når det vises:** ikonet finnes bare mens tjenesten går i bakgrunnen, det vil si at «Fortsett i bakgrunnen» er på eller en logg går, og ingen vinduer er koblet til (`hello` med `agentic-rigg/…`). Det forsvinner når et vindu åpnes eller tjenesten stopper, og kommer tilbake ca. 2 s etter at vinduet er lukket.
  - **Ikonet:** `io.github.AgenticRigg-symbolic`.
  - **Verktøytipset:** «GPU 29 °C · 3 % last», med «· Logger» og antall rader mens loggen går. AppIndicator viser ikke verktøytips, så den samme statuslinja står også øverst i menyen, og den kan ikke klikkes.
  - **Menyen:** «Åpne Agentic Rigg», «Stopp logging» (bare mens loggen går) og «Avslutt bakgrunnstjenesten». Dobbeltklikk på ikonet åpner appen (`Activate`).
  - **Slik virker det:** én tidtaker hvert 2. s leser tilstanden. Når ikonet er ønsket, følger tjenesten navnet `org.kde.StatusNotifierWatcher`. Først når verten finnes, åpnes en egen forbindelse til sesjonsbussen, med navnet `org.kde.StatusNotifierItem-<pid>-1`, objektene og registreringen. Når ikonet ikke lenger er ønsket, lukkes forbindelsen, og verten fjerner ikonet.
  - **Uten vert** skjer ingenting, og ingenting skrives i journalen.
- **«Start tjenesten ved innlogging»** (Innstillinger › Generelt › Bakgrunn): tjenesten kaller `GetUnitFileState`, `EnableUnitFiles`/`DisableUnitFiles` og `Reload` til systemd --user over D-Bus, som `systemctl --user enable` gjør. Valget er av som standard.
  - Bryteren viser alltid det systemd svarer. Ved feil kommer et varsel, og bryteren går tilbake. Er enheten ikke installert, er raden låst og sier hvorfor.
  - Utviklingsprofilen bruker enheten `agentic-riggd-dev.service`. Den finnes ikke i en vanlig installasjon, så et utviklingsbygg kan aldri slå på den installerte tjenesten.
- **Gruppa «Bakgrunn»:**
  - **Status:** «Kjører siden 14:48 (3 min) · PID 2382810 · 28 MB minne · opptak i bakgrunnen på», med «· logger til fil» når en logg går. Tallene kommer fra `ping` hvert 5. s mens raden synes.
  - **Valgene:** «Fortsett i bakgrunnen», «Takt i bakgrunnen» (1/2/5 s, med målt CPU) og «Start tjenesten ved innlogging».
  - **Merknaden:** hva det koster.
  - **Rettet:** bryteren og takten fulgte ikke tjenesten når gruppa ble bygd før tjenesten svarte. Bryteren sto av selv om bakgrunn var på, og takten var grå. Nå følger de tjenesten, uten å sende noe tilbake.
  - Søket finner radene, for eksempel på «innlogging», «autostart», «systemfelt» og «pid».
- **Målt** (`maalinger.md`, runde 3B):
  - **Systemfeltet:** ingen målbar CPU (1,30–1,33 % mot 1,37–1,40 % uten, ± 0,05), under 0,1 MB minne, to fildeskriptorer og ingen ny tråd.
  - **D-Bus-trafikk:** ca. ett signal hvert 10. s mens ikonet vises. Den første versjonen sendte to signaler hvert 2. s, og det er rettet.
  - **Tjenesten i bakgrunnen:** 2,1 % med 1 s-takt, 1,4 % med 2 s og 0,7 % med 5 s, og ca. 28 MB Pss.
- **Tester:**
  - `tray.rs` har 8 tester: når ikonet vises, vinduer, menyen med og uten logg, faste ID-er, synlighet som egenskaper, verktøytipset og hvor ofte det meldes, oppsettet som `(u(ia{sv}av))`, og XML-en.
  - Én av dem kjører mot en privat `dbus-daemon` med en falsk vert. Den sjekker navnet, registreringen, `GetLayout`, klikk som blir handlinger (skjulte rader og «hovered» gjør ingenting), `ItemsPropertiesUpdated` når loggen starter, og at navnet forsvinner.
  - `generelt.rs` har to tester: enhetsnavnet per profil, tilstandene og feilteksten.
  - `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer.
- **Prøvd i en privat økt** (`docs/produksjon/verktoy/runde3b/`): en kopi av `okt.sh` med `--mode=ubuntu`, en liten testutvidelse (`rigg-3b@agentic`) som tar skjermbilder og klikker med en virtuell mus, og en attrapp av `org.freedesktop.systemd1`. Attrappen gjør det samme med filene som systemd, og nekter å starte på en buss der systemd finnes eller med den ekte `~/.config`.
  - **Systemfeltet:** ikonet og menyen i Yaru. «Stopp logging» stoppet loggen. «Åpne Agentic Rigg» åpnet vinduet, og da forsvant ikonet. Ikonet kom tilbake når vinduet ble lukket. «Avslutt bakgrunnstjenesten» stoppet tjenesten rent, og socketen ble slettet. Med bakgrunn av og logg vises ikonet; uten logg vises det ikke.
  - **Start ved innlogging:** på og av med klikk i vinduet. `systemctl --user --root=/ is-enabled` (frakoblet, leser bare filer) med den private `XDG_CONFIG_HOME` bekreftet «enabled» og «disabled». Feilvarselet ble prøvd med en skrivebeskyttet `default.target.wants`.
- **Ingenting er slått på i den ekte økta.** `~/.config/systemd/user` er uendret (lista med tidsstempler før og etter er lik), og `systemctl --user is-enabled agentic-riggd.service` sier fortsatt `not-found`. Brukerens GNOME-utvidelser og innstillinger er ikke rørt. Ingenting er skrevet til kortet, og ingen av brukerens prosesser er rørt.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde3b/` (12 bilder): ikonet i panelet, menyen i lyst og mørkt, med og uten logg, skrivebordet med vinduet åpent (uten ikon) og lukket (med ikon), gruppa i lyst og mørkt, med innlogging på, når enheten ikke er installert, og feilvarselet.
- **Ønsker til hovedøkta:**
  1. **`riggd/src/main.rs`**: koble inn systemfeltet. Prøvd i en kopi, og clippy passerer med den:
     ```diff
     -// Systemfeltet kobles inn av 3B (BØR).
     -#[allow(dead_code)]
      mod tray;
     …
     +    let _tray = tray::attach(&d);
          eprintln!("{} lytter på {} ({}).", rigg_core::version::long(), socket.display(), kind.name());
     ```
  2. **3A (optimalisering):** tjenesten bruker 1,3–1,5 % av én kjerne med 2 s-takt i P8, mot kravet ≤ 1,2 %. Det er likt med og uten systemfeltet. Tallene i «Takt i bakgrunnen» og i merknaden må oppdateres hvis prøveplanen endres.
  3. **`scripts/okt.sh`** (valgfritt): `OKT_MODE=ubuntu` (gir `--mode=ubuntu`) og `OKT_FORBERED` (et skript før Shell starter) ville latt alle teste med AppIndicator og Yaru. Se `verktoy/runde3b/okt3b.sh`, som har tre linjer mer enn `okt.sh`.
  4. **Produktspesifikasjonen 3.1** sier at menyen skal ha «Start/stopp logging» og «Stopp bakgrunnsopptak». Oppgaven for 3B sa «Stopp logging» (bare mens loggen går) og «Avslutt bakgrunnstjenesten», og det er det som er laget. «Stopp tjenesten nå» fra kap. 3.10 er ikke laget; «Avslutt bakgrunnstjenesten» i systemfeltet dekker det mens tjenesten går i bakgrunnen.

## 2026-11-07 · Runde 3C · Optimalisering

Alle filer. Målet var at vinduet åpner og lukker seg øyeblikkelig og bruker lite minne, og at tallene er målt. Tallene og hvordan de er målt står i `maalinger.md`, runde 3C.

- **Tegneren er nå cairo** (`rigg/src/main.rs`: `DEFAULT_RENDERER`, settes som `GSK_RENDERER` før GTK starter hvis den ikke er satt, så popovere og verktøytips får samme tegner). Begrunnelse: eneste tegner under 250 ms til første bilde (162 ms mot 293 ms med GL og 446 ms med Vulkan), og 12–15 MB privat minne mot 46–106 MB med GL. CPU i ro er nå 0,3–1,65 % på alle sidene med cairo. Prisen er Lys mens det animerer (se under). Rask gjenåpning trengs ikke.
- **Før → etter** (detaljer i `maalinger.md`): første bilde 457 → 162 ms · lukk til vinduet er borte opptil 79 → 0,6 ms, til prosessen er avsluttet 86 → 10 ms · tjenesten i bakgrunnen (2 s, P8) 1,38 → 0,85 % · Oversikt i ro 1,60 (GL) / 2,50 (cairo) → 1,35 % · System 4,80 → 0,70 % (cairo) · vinduet uten 3D 48–106 → 13–15 MB. P8 i alle prøvene, i bakgrunnen og med vinduet på hver side (rettet i 4A: første oppbygging av 3D-kortet på Lys gir P0 i ca. 4 s).
- **Over kravet:** vinduet med Lys i 3D er 136 MB (NVIDIA-driveren, gis ikke tilbake før vinduet lukkes), Lys koster 25–36 % mens det animerer, og tjenesten 2,55 % mens Lys animerer. Forslag til hovedøkta: «auto» viser 2D med cairo, og 3D lastes først når du velger det.
- **Oppstart:**
  - `rigg/src/timing.rs`: merker med veggklokka fra `main` til første bilde og ved lukking (`RIGG_TIMING=1`), og `RIGG_TIMING_LUKK=MS` som lukker vinduet samme vei som ×-knappen. `RIGG_RAMMER=1` skriver bilder og tid per bilde hvert 10. s.
  - `theme.rs`: `gtk-application-prefer-dark-theme` settes bare når verdien endres. Før lastet GTK hele Yaru-temaet på nytt ved hver start (ca. 14 ms).
  - Forsøkt og forkastet: GL-driveren varmet opp i en egen tråd (`glwarm.rs`, fjernet), GL uten systemtema.
- **Lukking:** `window.rs` skjuler vinduet før `ui.json` lagres (lagringen har `fsync` og kunne holde vinduet på skjermen i 50–80 ms), og `prefs::save_now` skriver ikke når ingenting er endret (`App::saved_text`).
- **CPU i ro med cairo:**
  - `widgets::Painter` (`widgets/mod.rs`): grafene (`chart.rs`) og sparklinjene (`spark.rs`) tegnes som én cairo-node med vanlig fyll og strek når tegneren er cairo, og som GSK-stier ellers. Oversikt gikk fra 5,7 til 2,9 ms per bilde.
  - System tegnet 24 bilder i sekundet hele tiden: de 16 trådstolpene gled etter hver sin endring. `Bar::set_still` lar dem hoppe rett til ny verdi (`pages/system.rs`).
  - Sensorer oppdaterer bare radene som synes i rullefeltet, og resten når de rulles fram eller vinduet blir større.
- **Lys:**
  - Med cairo går løkka med høyst ca. 30 bilder/s (input tegnes straks). 3D gikk fra 63 til 36 % og 2D fra 38 til 19 % av én kjerne mens det animerer.
  - Vakthunden som sender 3D til 2D når GL er tregt, krever nå tolv trege bilder på rad, så et enkelt hikk ikke slår den ut.
  - Etter opplasting slippes CPU-kopiene av teksturene, og `malloc_trim` gir minnet tilbake (også når 3D frigjøres etter 30 s). Teksturene bygges på nytt hvis GL-ressursene må lages igjen.
  - `GraphicsOffload` rundt GLArea-en er prøvd og beholdt: NVIDIA-driveren her gir ikke dmabuf, så det hjelper ikke ennå, men det er gratis og virker på andre drivere.
- **Tjenesten:**
  - `schedule::detailed`: de trege verdiene (vifte 2, turtall, SM- og videoklokke, koder og dekoder) leses hvert 2. s bare mens et vindu som synes abonnerer eller en logg går. Ellers hvert 10. s. Effektgrensen og PCIe-bredden hvert 30. s i stedet for 5. Viftestyring og målfart (1,6–3,2 ms per vifte i P8) leses bare med vindu.
  - Når vinduet kommer fram, leses alt i neste runde.
  - Ingen JSON lages for prøver eller statistikk når ingen abonnerer.
  - NOT_SUPPORTED-lista og P-tilstanden som tekst lages bare når de endres.
  - `malloc_trim` hvert 60. s, så ledig heap etter store `get_history`-svar gis tilbake (fant det med lekkasjetesten).
- **Release-profilen** er beholdt (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`). `panic = "abort"` er ikke trygt: prøvetråden i tjenesten fanger panikk og starter på nytt, og `panic` kan ikke settes per pakke. Ingen avhengigheter kunne fjernes.
- **Skript:** `scripts/maal.sh` måler start og lukking med veggklokka. `bilder.sh`: viser `RIGG_2C_MAAL`, har planen `3c`, tar debug-bygget for 2a- og 2b-planene (de finnes bare der, og med release ble vinduet stående og vente) og gir vinduet høyst 15 min (`RIGG_PLAN_FRIST`). Måleskriptene for runden ligger i `docs/produksjon/verktoy/runde3c/` (`kjor.sh`, `lekkasje_svg.py`).
- **Tekst:** «Takt i bakgrunnen» (1,5 / 0,9 / 0,4 %) og merknaden i Innstillinger › Generelt har de nye tallene.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde3c/` med `bilder.sh --plan alle` (alle planene, cairo). Den nye planen `3c` tar alle sidene i lyst og mørkt med de fire temaene, høy kontrast i lyst og mørkt, og alle sidene i 640 og 1920 px (`3c/`, 98 bilder).
- **Tester:** `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` passerer: 177 tester (ny: `schedule::trege_verdier_i_full_takt_bare_med_vindu_som_synes_eller_logg`), pluss 2 `#[ignore]` mot det ekte kortet.
- **Lekkasjetesten:** 1 t med det ekte kortet og vinduet åpnet og lukket 11 ganger: privat minne 21,2 MB etter 5 min og 22,9 MB etter 60 min, 0,8 MB/t etter 20 min, som er historikken som fylles (beregnet ca. 0,5 MB/t til nivåene er fulle). Fildeskriptorer (53–54) og tråder (6) står stille, og kortet var i P8 i alle 120 prøvene. Ingen lekkasje. Kurven: `maal/runde3c-lekkasje-1t-trim.csv` og `.svg`.
- Ingenting er skrevet til kortet, ingen I2C er åpnet, og ingen av brukerens prosesser er rørt. Alle målinger kjørte i private, hodeløse økter med egne mapper; GL-forsøkene (`eglctx`) lagde bare en kontekst og leste ingenting fra eller til kortet utover det GTK gjør selv.

## 2026-11-07 · Runde 4A · Kritikk

- Skrev `kritikk-1.0.0.md`: 43 funn (7 Må, 20 Bør, 16 Kan) gruppert per side, og 6 produktspørsmål. Ingen kode er endret.
- **Må:**
  - «VRAM-klo…» og «Tråd 3 MHz» i statusblokken.
  - Flistitlene på Oversikt kuttes ved 640 px.
  - «standard»-merkene på Kontroller står midt på sporet.
  - «Lest fra kortet» mens kortet er borte.
  - «2 målinger» mot «Målinger 3» på Viftekurve.
  - «Stopp tjenesten nå» mangler.
  - Lisensene installeres ikke.
- **P0/P8:**
  - Målt med `nvidia-smi --query-gpu` hvert 2. s mens release-byggene kjørte i `okt.sh` uten skjermbildeplan. Rådata: `docs/skjermbilder/1.0.0/runde4a/pstate-*.txt`.
  - Oversikt i 60 s: P8 i alle prøvene.
  - Første gang Lys bygger 3D-kortet: P0, 1 515 MHz og 36–39 W i ca. 4 s. Deretter P8, 315 MHz, 14 W og 15–19 % GPU-last mens det animerer.
  - Den hodeløse Shell vekker også kortet i ca. 4 s når økta starter.
  - Skjermbildene i `3c/oversikt-grafitt-mork.png` er tatt rett etter første Lys. Selve skjermbildetakingen (cairo) vekker ikke kortet.
  - Påstanden i 3C om «P8 i alle prøvene også med Lys i 3D» bør rettes.
- **Anbefaling til hovedøkta:** Lys «auto» viser 2D med cairo, og 3D lastes først når du velger det (produktspørsmål 1).
- **Egne bilder:** `docs/skjermbilder/1.0.0/runde4a/` (planen `3c-640`, release, privat økt).
- **Tester:** `cargo clippy --workspace --all-targets -- -D warnings` er ren, og `cargo test --workspace` gir 177 grønne tester og 2 ignorerte.
- Ingenting er skrevet til kortet, ingen I2C er åpnet, og ingen av brukerens prosesser er rørt. Øktene mine (`okt-4a-p`, `okt-4a-b`) er avsluttet og mappene slettet.

## 2026-11-07 · Runde 4B · Finpuss før installasjon

Alle filer. Arbeidslista var `kritikk-1.0.0.md` (43 funn) og brukerens svar på produktspørsmålene. Endringene står i `app/PATCHLOG.md`, og status i `app/STATUS.md`. Ny `app/README.md`.

- **Rettet:** alle 7 Må og alle 20 Bør. Av Kan: 5 (klokka og viftestyringen; VRAM-formatene er ikke samkjørt), 11, 12, 13, 14, 17, 18, 24, 29, 32, 36, 37, 38 og 41. Ikke rettet: 6 (tidsaksen bruker allerede smalt mellomrom U+202F, som tall og enheter ellers i appen; det ser tett ut i liten skrift, men er riktig) og 39 (aksent «System» og høy kontrast «Automatisk», BØR i spesifikasjonen 3.10, krever å lese GNOME-innstillinger; ikke blokkerende). 31 (lysdiodene slukket når kortet er borte) er sjekket: i 2D, som Lys nå åpner med, lyser diodene også med kortet borte (`runde4b/3a-borte/borte-lys-lys.png`); effektmotoren (`rgb.rs`) bruker ikke prøven for Statisk. Det slukkede 3D-kortet i 4A er ikke gjenskapt.
- **Brukerens valg** (spesifikasjonen kap. 10 er oppdatert):
  - Lys åpner med 2D; 3D huskes når det velges. Innstillingen «3D-kortet: Auto / Alltid 3D / Alltid 2D» (`lys3d`) i Utseende › Lys, Auto som standard.
  - «Fortsett i bakgrunnen» av, start ved innlogging av, skrivebordsvarsler av. Bakgrunn står nå øverst i Generelt, med forklaring av av og på.
  - Systemfeltet: «Åpne Agentic Rigg», «Start logging»/«Stopp logging» (alltid synlig), «Stopp bakgrunnsopptak»; verktøytips med temperatur, effekt og last.
  - «Kjerneklokke» overalt, «Lest fra kortet» fjernet, Oversikt uten driver viser tom tilstand bare for GPU-delen.
- **Tastaturrunden** (funn 43) med ekte tastetrykk: en liten Shell-utvidelse med et virtuelt Clutter-tastatur i den private økta (`verktoy/runde4b/`: `okt4b.sh`, `forbered.sh`, `tast.sh`, `ext/rigg-4b@agentic`), skjermbilder fra Shell og vinduets fokuslogg (`RIGG_FOKUS=1`, bare utviklingsprofilen). Fant og rettet tre feil som ikke synes i skjermbildene fra planene:
  1. Fokusringen ble tegnet rundt alle foreldrene til flaten med fokus (GTK setter `:focus-visible` på hele kjeden). Nå `:focus:focus-visible` i alle CSS-filene.
  2. Tab sto fast på siste flate på Kontroller, Viftekurve og System. Vinduet går nå rundt til starten selv når en beholder svarer «flyttet» uten å flytte.
  3. Hver verdi i panelene (Nå, Siste døgn, Feilsøking, rapporten) var et stopp for Tab. Verdiene kan fortsatt merkes med musa.
  Også: Esc i tomt filter på Sensorer flytter fokus ut. Prøvd og virker: 1–8, Tab/Shift+Tab, Ctrl+, Ctrl+F og søk i Innstillinger, Esc, Ctrl+B, Ctrl+?, Ctrl++ og Ctrl+0, mellomrom (pause), Ctrl+Z i utkast, Ctrl+W og Ctrl+Q. Bilder og Tab-rekkefølge: `docs/skjermbilder/1.0.0/runde4b/tastatur/`.
- **Avkuttet tekst sjekkes automatisk:** hvert bilde i planene skriver etiketter med «…» til `kuttet.txt` i bildemappa. Statusblokken med festede nøkler er tatt med standard skrift og «Stor».
- **Lisensene:** `app/data/LISENSER.md` (Lucide ISC, Feather MIT, Rust-bibliotekene, MIT) er i GResource, vises fra Om › «Lisenser …» og installeres til `~/.local/share/doc/agentic-rigg/`. `install.sh` og `uninstall.sh` er prøvd mot en midlertidig HOME (`RIGG_INSTALL_NO_SYSTEMD=1`), ikke mot ekte `~/.local`.
- **`maalinger.md`:** påstanden om P8 også med Lys i 3D (3C) er rettet, og runde 4B har nye tall: kald start 170 ms (164–180; 3C 162), lukking 0,6 ms til vinduet er borte og 10 ms til prosessen er ute, vinduet i ro 1,35 % på Oversikt og 0,35–1,50 % på de andre sidene (Lys i 2D animerer viftene: 16,75 %), 11,5–14,5 MB privat minne på alle sidene, tjenesten i bakgrunnen 1,05 % med 2 s. Kortet hadde 19 % last fra brukerens egne programmer under målingene, så tjenestens tall er 0,2–0,5 prosentpoeng over 3C; med vinduet åpent 2,6–2,85 % (krav 2,5 %). Bør måles på nytt med kortet i ro.
- **Tester:** clippy ren, 180 tester grønne (177 før), pluss 2 `#[ignore]` mot det ekte kortet. Nye: statusnavnene (lengde, ingen enhet) og sparklinjenes minste spenn, `initial_view` for Lys, tester for systemfeltets nye meny og verktøytips (oppdatert), `bytes` uten desimaler for MB.
- **Skjermbilder:** `docs/skjermbilder/1.0.0/runde4b/` med `bilder.sh --plan alle` (release, cairo, 16 planer), pluss `tastatur/`. Planene `3c`, `3a-borte` og `grunnmur` og tastaturrunden er tatt på nytt etter de siste rettingene (tekstene i Bakgrunn, hullet etter at kortet stoppet, Kontroller og Viftekurve).
- Ingenting er skrevet til kortet, ingen I2C er åpnet, ingen av brukerens prosesser er rørt, og ingen systemd-enheter er slått på. Alle øktene var private med egne mapper, og bare mine egne mapper er slettet.
- **Til hovedøkta:**
  1. Installer med `app/install.sh` etter brukerens ja, og mål første bilde i den ekte økta.
  2. Systemfeltets nye meny bør ses i panelet én gang (testene går mot en privat D-Bus).

## Runde 5 · Installasjon (hovedøkta, 7. november 2026)

- `app/install.sh` er kjørt mot ekte `~/.local` etter brukerens ja. `.desktop`-fila er gyldig (`desktop-file-validate`), GNOME ser appen (`Gio.AppInfo`, `should_show`), og systemd-enheten er installert, men ikke slått på (`disabled`).
- Målt i brukerens økt: kald start 188–191 ms, lukking 1,3 ms til vinduet er borte og 4,1 ms til prosessen er ute, tjenesten stoppet ca. 58 s etter lukking, vinduet i ro 0–0,2 %, tjenesten 0,8–0,9 % med vinduet åpent, kortet i P8. Tallene står i `maalinger.md` (runde 5).

## 2026-11-08 · Runde 6B · Skall

**Navnene på Styring-sidene (til 6C og sidetitlene).** `id` er uendret (`kontroller`, `viftekurve`, `lys`), så lenker, innstillinger og `--side` virker som før. 6B endrer `title`/`icon` i `pages/mod.rs`:

| id | Ny tittel | Ikon | Undertekst (verktøytips i sidefeltet) |
|---|---|---|---|
| `kontroller` | **Klokker og effekt** | `sliders-horizontal` | Effektgrense, klokke-offset og temperaturmål |
| `viftekurve` | **Vifter** | `fan` | Viftekurve og turtall |
| `lys` | **RGB-lys** | `palette` | Effekter og farger på kortet |

- Seksjonen heter fortsatt «Styring» og får merket «Bare visning» (lås) med verktøytipset «Agentic Rigg leser bare. Endringer på skjermkortet er låst til du slår dem på selv, ett steg om gangen.» Hver side har ikke lenger sin egen hengelås.
- **Til 6A:** `rigg_core::settings::PAGES` (startsiden i Innstillinger › Generelt) bør få de samme navnene når Prosessor og Disker legges inn der.

**Vindusknappene.** Årsaken sto i `base.css`: selve knappen fikk bakgrunn og `border-radius: 50 %`. Knappen fyller toppfeltets høyde (ca. 34 × 40 px), så den ble en høy, mørk oval, og Yaru tegner i tillegg sin egen sirkel på ikonet inni. Ikonet havnet i en sirkel i en oval, med 10 px mellom knappene og 31 px fra kanten. Nå er det som i libadwaita/Yaru på Ubuntu (lest ut av `libadwaita-1.so` og Yaru-temaet): knappen er et usynlig treffområde på 34 px, sirkelen på 24 px er ikonet (16 + 2 × 4) med 10/15/30 % av tekstfargen i ro/hover/trykk, 3 px mellom, sirkelen 12 px fra kanten, og en tynn ring i høy kontrast. Målt med ekte peker i en privat økt: sentrene står 37 px fra hverandre og 25 px fra kanten, som i Ubuntus Innstillinger (37 og 24).
- Kontrollene følger `gtk-decoration-layout`: knappene til venstre står i sidefeltets toppfelt (i toppfeltet når sidefeltet er ikonlist), og tomme kontroller skjules (`topbar::controls`). Dialogene (Tastatursnarveier, Lisenser) bruker GNOMEs lukkeknapp med tittelen midt på.
- «1 min» i toppfeltet er 28 px med vanlig skrift, som knappene ved siden av (før 22 px og liten skrift). Regelen står i `theme.rs` (`OVER_PAGES`): ark med lik prioritet vinner etter lasterekkefølge, ikke spesifisitet, så `oversikt.css` overstyrte `skall.css`.
- Å dra fra en vindusknapp flytter vinduet, som i libadwaita (målt i begge).

**Sidefeltet.**
- Maskinhodet: «RTX 2080 / 8 GB · driver 610.43.02» og «Ryzen 7 2700X / 8 kjerner · 16 tråder» i ett kort. Skjermkortet åpner detaljene og kortvalget (med flere kort: «Kort 1 av 2 · 8 GB»), prosessoren går til Prosessor. Prosessoren hentes med `get_system` én gang per tilkobling. Ingen grønn prikk: feil står i rødt eller gult i linja under («Svarer ikke», «Ingen NVIDIA-driver», «Ny driver · start på nytt», «Tjenesten svarer ikke»), og som et lite merke i ikonlista. Under 760 px høyde vises bare navnene. Pila synes ved hover og fokus.
- Styring: navnene over, merket «Bare visning» ved seksjonen, og undertekst og forklaring i verktøytipsene og tilgjengelighetsbeskrivelsen. Hengelåsen per side er fjernet.
- Statusblokken ber ikke lenger om høyde (`FitBin`): den viser så mange verdier som får plass, også i utvidet sidefelt, og skjules når ingen får plass. Vinduet kan nå bli 602 px høyt med ti sider (før 622 med åtte).
- Tastene 1–9 og 0 (`shortcuts::page_index`), tallene i sidefeltet og hjelpen i Tastatursnarveier. `skjermbilder.rs` har én liste med ti sider, en test som sjekker den mot `pages::PAGES`, og planene `6b` og `6b-tilstand`.

**Om.** Signaturen «Code Pro» med «Navnet på byggeserien denne versjonen hører til» står sentrert nederst i Om-kortet, under versjonen, med en tynn linje over (som i Klipp og Agentic Desktop).

**Filer:** `sidebar.rs`, `topbar.rs` (skallets toppfelt; ingen andre eier den), `window.rs`, `shortcuts.rs`, `skjermbilder.rs`, `theme.rs`, `settings/om.rs`, `settings/mod.rs`, `pages/mod.rs` (Styring-titlene), `base.css`, `skall.css`, `innstillinger.css`. Verktøy: `verktoy/runde6b/` (`okt6b.sh`, `forbered.sh`, testutvidelsen med virtuell peker, `kjor.sh`, `finn_knapper.py`, `plan.sh`, `start.sh`).

**Skjermbilder:** `docs/skjermbilder/1.1.0/6b/`: `for/` og `etter/` (ekte peker i en privat økt: hele vinduet, toppfeltet, knappene i ro, hover og trykk, sidefeltet utvidet og sammenlagt, lavt vindu, Om), `naerbilde/` (4×, før, etter og Ubuntus Innstillinger), `ubuntu-innstillinger/` og `plan/` (planen `6b`: lyst, mørkt, høy kontrast lyst og mørkt, fokusring, ikonlist, 640 og 1920 px, høyde 560/700/800, dialogene, og maskinhodet i feiltilstandene). `plan/kuttet.txt` nevner ingen etiketter i skallet.

**Funnet hos andre (ikke rettet):**
- 6C: `rigg-hw/tests/bare_lesing.rs` feiler fordi `pages/kontroller.rs` nevner `nvmlDeviceSet…` i en forklaringstekst (linje 659–675).
- 6A: Prosessor gjør vinduet minst 786 px bredt (planen `6b`, `skall-prosessor-640-lys.png`), og `disker.css` linje 6 har `overflow`, som GTK ikke kjenner. `rigg_core::settings::PAGES` har fortsatt de gamle navnene og åtte sider.

**Kald start** (`maalinger.md`, runde 6B): median 199 ms etter mot 217 ms før, 15 starter hver annenhver gang i samme økt, med last 14–17 på maskinen. Kravet er 250 ms.

**Tester:** `cargo clippy --workspace --all-targets -- -D warnings` er ren. `cargo test --workspace`: alt grønt utenom 6Cs `bare_lesing` over. Nye tester: `sifrene_gir_alle_ti_sidene`, `prosessornavnet_blir_kort`, `prosessorlinja_har_kjerner_og_tråder`, `styring_sidene_sier_hva_de_styrer`, `planene_har_alle_sidene_i_samme_rekkefølge`.
- Ingenting er skrevet til kortet, ingen I2C er åpnet, ingen av brukerens prosesser er rørt. Alle øktene var private (`okt-6b-*`), og testvinduene brukte sim-backenden; bare startmålingen leste det ekte kortet.

## 2026-11-08 · Runde 6C · Styring: Klokker og effekt og Vifter

Redesign av Kontroller («Klokker og effekt») og Viftekurve («Vifter») etter brukerens ønske om en mer overklokkingsvennlig side, tilbakestilling til standard, lettere å se klokker og temperatur, og mer forklaring. Sidenavnene er 6Bs. Fortsatt låst mot det ekte kortet: alt blir et utkast som ikke sendes.

- **Felles (nye `widgets/oc.rs`, `widgets/locked.rs`):**
  - **Låst-linja** (`LockStrip`) erstatter det store banneret på de to sidene: én rolig linje som sier «Låst», «Utkast · låst», «Simulert kort» eller «Kortet svarer ikke», med hvorfor og «Hva skal til?». Lys (RGB-lys) har fortsatt `Banner`.
  - **Tallflisene** øverst (`Stat`, `StatStrip`, `Meter`): store tall med enhet, en linje under og en tynn måler med merke (målet, minste fart).
  - **Glideren** (`oc::Slider`, tegnet selv): område fra kortet, det tjenesten tillater som tykt spor, standardmerke med «standard 245 W», en hul ring for det kortet har når utkastet er noe annet, snap til steg og til standard (6 px). Tallene under kolliderer aldri (standard vinner, endene viker). Piltaster (Shift: stort steg), PgUp/PgDn, Home/End.
  - **Kontrollraden** (`OcRow`): tittel med (i), kort forklaring, nå-linje (for eksempel «Trekker nå 23 W (9 % av grensen)»), glider, tallfelt med fortegn, enhet og «Tilbakestill» (bare ikonet i smale vinduer). Endrede rader får aksentkant.
  - **(i)-popoveren:** hva den gjør, hva den påvirker, standard, område fra kortet, det tjenesten tillater og hvilket NVML-kall som leser verdien.
  - **Angre:** Ctrl+Z / Ctrl+Shift+Z (og Ctrl+Y) i utkastet på begge sidene (`UndoStack`, en glider som dras er ett steg, høyst 100 steg). «Forkast utkast» og «Tilbakestill alt» har «Angre» i varselet.
  - `locked::edit_draft`: utkastet lages først når en endring faktisk gjør det forskjellig fra kortet.
  - `Columns::set_natural`: side ved side får hver spalte sin naturlige høyde.
  - Fjernet: `ReadRow`, `RangeTrack`, `Ticks` og `Banner::set_lost` (brukes ikke lenger).
- **Klokker og effekt:**
  - Åtte fliser: kjerneklokke og minneklokke (nå og høyest), temperatur (mot målet), effekt (W og % av grensen), last, vifte, P-tilstand og «Begrenset av».
  - «Begrenset av» er lest fra NVMLs klokkebegrensninger (fantes allerede i telemetrien, `throttle`). Spenning er utledet: ingen begrensning mens kortet har minst 60 % last. I P8 står «Tomgang» uten å spørre kortet.
  - Panelet «Innstillinger for kortet» har én rad per kontroll og «Tilbakestill alt til standard» i hodet. Vifte-raden har Auto/Kurve/Manuell, fast fart i Manuell og «Rediger kurven» (til Vifter) i Kurve.
  - Til høyre: utkastet (fra → til), profilene (med «på siden» når verdiene er like en profil, og en tom tilstand), begrensningene som HWiNFO-liste (Ja · siden 14:32 / Nei), og det kortet ikke kan.
  - Under: kjerneklokke, temperatur og effekt for de siste 5 minuttene.
  - Ingen V/F-kurve: den kan ikke leses eller endres med NVML på Turing, og står under «Ikke mulig på dette kortet». Klokke mot temperatur er ikke laget.
- **Vifter:**
  - Seks fliser: temperatur, vifte 1 og 2 (% og o/min), mål-fart, modus, og hva kurven ville gitt nå.
  - Kurveredigereren har faste akser (20–100 °C, 0–100 %). Punktene dras og festes til hele grader og prosent (Shift: steg på 5), og holdes mellom naboene, over minste fart og aldri fallende. Dobbeltklikk legger til eller fjerner. Tastaturet: [ ] PgUp PgDn Home End, piltaster, +/Insert og Delete.
  - Live-punktet har hjelpelinjer til aksene og «Nå 37 °C · 45 %», og en markør der kurven ville vært.
  - Minste fart: skravert sone, stiplet linje og «Minst 35 % (VBIOS)». Nødregelen over 85 °C er markert.
  - Driverens faktiske kurve (punktsky og median) ligger i bakgrunnen. Uten utkast vises standardkurven stiplet.
  - Ingen tekst overlapper: hver tekst får en flate, og det som ville truffet noe annet, tegnes ikke. Første tall på x-aksen begynner ved aksen, så funnet fra 4A («0 %» og «20 °C») kan ikke komme tilbake.
  - Forhåndsvalg og «Tilbakestill kurven».
  - Til høyre:
    - «Hvem styrer viftene»: tre valg med forklaring, «kortet nå» og glider for fast fart.
    - Punktene som tall med fjern-knapp.
    - «Slik følges kurven»: hysterese (kurven har ingen, nødregelen 5 °C), start og stopp (ikke mulig, minst 35 % i VBIOS), mellom punktene, og fristen for Behold.
    - Siste døgn.
  - Under kurven: temperatur og begge viftene for de siste 5 minuttene.
- **Tester** (nye):
  - `oc::snap`.
  - Angre: sammenslåing, gjør om og grensen på 100.
  - `stock`, `reset_field` og `reset_all`, utkastets diff, `limiter` og viftelinja.
  - Kurvelogikken: snap, `move_point` (alltid gyldig for `check_curve`), `add_point`/`add_after`/`remove_point` og tilbakestill kurven.
  - `cargo clippy --workspace --all-targets -- -D warnings` er ren, og `cargo test --workspace --no-fail-fast` gir 215 bestått og 4 ignorert. `riggd/tests/logg.rs::tilstandene_fra_sim_scenariene` feilet én gang i en full kjøring, men besto i neste kjøring (ikke 6Cs fil).
- **Skjermbildeplanene** i `pages/styring_bilder.rs`:
  - `ekte` (låst) og `sim` (utkast, Bruk → Behold, Bruk → Rull tilbake, Tilbakestill alt, Manuell). Lys står i en egen plan, `lys`.
  - Planene skriver `steg.txt` med klokkeslett, `maal.txt` (CPU i ro) og `kuttet.txt`.
  - Kjøres med `verktoy/runde6c/bilder6c.sh`, som logger P-tilstanden mens `ekte` går.
  - Bildene ligger i `docs/skjermbilder/1.1.0/6c/` (`ekte/`, `sim/` og `for/` = 1.0.0).
- **Målt** (`maalinger.md`, runde 6C):
  - Vinduet i ro (60 s, tre kjøringer): Klokker og effekt 0,60–0,62 %, Vifter 0,68–0,98 %, med utkast 0,85–1,20 % (krav ≤ 2 %). Det er bare grafene som tegner, ca. 2 bilder/s.
  - P8 i alle prøvene mens vinduet var åpent (179, 189 og 187 prøver, 1 s mellom). P0-toppene i øktene kom før vinduet startet.
  - 640 px: radene og navnet på kurven har ingen fast minstebredde lenger. Før holdt de vinduet på 706–786 px (6Bs bilder `skall-kontroller-640` var 786 px); nå er alle bildene 640 px.
- **Endringer utenfor filene mine:**
  - `widgets/mod.rs`: `pub mod oc;` og en linje i tabellen.
  - Ny `docs/produksjon/verktoy/runde6c/bilder6c.sh`.
  - Ingenting i `rigg-core`, `rigg-hw` eller `riggd` (throttle reasons fantes allerede).
- **Til hovedøkta:**
  1. `skjermbilder.rs`, planen `3a-sim`, klikker «Lag utkast», men den knappen finnes ikke lenger, fordi gliderne lager utkastet selv. Bytt til `Eval` med `locked::edit_draft`, som i `styring_bilder.rs` (`draft_kontroller`).
  2. `shortcuts.rs` (6B): Ctrl+Z / Ctrl+Shift+Z i utkastet og piltastene på glidere og kurvepunkter står ikke i tastaturoversikten.
  3. `scripts/bilder.sh`, planen `2c`, kjører nå planen `sim` (valgt automatisk når porten er sim).
- Ingenting er skrevet til kortet, ingen I2C er åpnet, og ingen av brukerens prosesser er rørt. Øktene mine (`okt-6c-ekte`, `okt-6c-sim`) er private og slettet etter bruk.

## 2026-11-08/09 · Runde 6A · Maskinvare (Disker, Prosessor, System)

- **Disker** (`pages/disker.rs`, `disker.css`, ikon `hard-drive`): én oppføring per disk med helse (OK / Advarsel / Feil / Ukjent), temperatur, MB/s med sparklinjer og aktivitet. Utfoldet: Disk (modell, serie, fastvare, WWN, kapasitet, type og o/min, lenke, sektorer, planlegger), SMART-sammendrag (helse med grunner, temperatur, driftstid, sykluser, dårlige sektorer, levetid, skrevet/lest, selvtest, når UDisks2 leste), partisjoner med ledig plass, MB/s-graf med historikk og hele SMART-tabellen (kan foldes; røde/gule rader). Tomtilstander: venter på tjenesten, leser, ingen disker, UDisks2 svarer ikke (merknad øverst), disk uten SMART (USB). Smale vinduer: uten sparklinjer, ned til 640 px.
- **Tjenesten** (`rigg-hw/src/sys/disk.rs`, `udisks.rs`, `disk_sim.rs`; `rigg-core/src/disk.rs`): `/proc/diskstats` hver runde; UDisks2 (`GetManagedObjects`, `SmartGetAttributes` bare når `SmartUpdated` endres) hvert 60. s med vindu/logg, hvert 10. min i bakgrunnen bare når diskvarselet er på. Aldri `SmartUpdate`/selvtest, aldri `/dev/sd*`. Ny op `get_disks`, `sys.disks` i prøvene, `get_system.disks` til rapporten. Diskkoden er flyttet fra `sys/os.rs`. Helse som CrystalDiskInfo, og 241/242 tolket riktig (libatasmart regner 32 MiB; Samsung/Seagate teller LBA-er). Sim: fem disker. Protokollen: `protokoll.md` kap. 10.
- **Prøvd mot ekte UDisks2** (`cargo test -p rigg-hw -- --ignored ekte_udisks2`): alle fem diskene (870 QVO 24 °C, 98 % levetid, 8,48 TB skrevet; SSD370 uten temperatur (oppgir 0 °C), 100 %, 38,2 TB; tre Seagate 28–36 °C), alle OK, 20 ms første gang. `gdbus`-utskrift fra maskinen ligger som fixture (`rigg-hw/tests/fixtures/udisks/`).
- **Sensorer og Grafer:** gruppa Disker (temperatur, lesing, skriving, aktivitet per disk; MB/s desimalt); grafene «Disktemperatur» (én serie per disk) og «Disk-I/O» (sum). Nøkler `disk/<serie>/…` og `disk/read|write`; historikken har 112 serier (96 før), fila under 8 MB. CSV-kolonnene har navn.
- **Varsel for diskhelse** (`alarms.diskHealth`, under hovedbryteren som er av): SMART feiler, eller flere dårlige sektorer enn sist. Bryteren står i Innstillinger › Logging › Skrivebordsvarsler.
- **Prosessor** (`pages/prosessor.rs`, `prosessor.css`): Nå (Tdie/Tctl, last, snitt-/toppklokke, governor/boost, sparklinjer), klokke og last per tråd gruppert etter CCX og kjerne (T0+T8 osv.), Prosessor, Cache, Instruksjonssett og Sårbarheter (ny). **System** beholder hovedkort/BIOS, minne og OS, og har fått Kontrollere (nett, lyd, USB, SATA, NVMe fra pci.ids med driver). `rigg_core::settings::PAGES` har ti sider med 6B sine navn.
- **Tester:** clippy ren i mine filer; nye tester for diskstats, mountinfo, udev, SMART-tolkning, helse, UDisks2-fixturen, NVMe, sim, sensornøkler, CSV, varselet, CCX-gruppering, PCI-navn og kontrakttesten `diskene_med_helse_og_hastighet`. To kontrakttester som feilet under høy last (første runde etter hello, 25 s-frist) venter nå på kortet.
- **Skjermbilder:** `docs/skjermbilder/1.1.0/6a/` (planene `6a` med sim og `6a-ekte` uten UDisks2 i `bilder.sh`).
- **Målt** (`maalinger.md`, runde 6A): tjenesten i bakgrunnen 0,88 → 0,90 % med diskene (krav 1,2 %), med vindu på Disker 1,93 → 2,04 %. Vinduet i ro: Disker 1,35 %, System 0,45 %, Prosessor 1,00 % (5,25 % i første versjon; trådrutenettet tegnes nå i én widget, `widgets/trader.rs`).
- Ingenting er skrevet til kortet eller diskene, ingen I2C, ingen av brukerens prosesser rørt; tjenesten for målingene hadde egne mapper og socket.
- **Til andre:** `scripts/ikoner.py` har `hard-drive` i `EKSTRA`. `theme.rs` (CSS-lista) og `settings/logging.rs` (bryteren) er endret med én linje/én rad. `bare_lesing` feilet en stund på `nvmlDeviceSet` i `kontroller.rs` (6C).

## 2026-11-09 · Runde 6D · Kritikk og finpuss (1.1.0 · bygg 17)

Eneste agent etter 6A, 6B og 6C. Alle filer.

- **P0-toppene (fra 6C) kom ikke fra tjenesten.** Målt alene med `nvidia-smi --query-gpu … -lms 1000`: tjenesten med ekte NVML uten Shell i 200 s (P8 i alle 223 prøvene), bare den private økta i 220 s (P0 i 3 s idet Shell starter), og tjenesten i privat økt uten vindu i 200 s som 6C (P0 bare idet Shell startet, ingenting etter 54–63 eller 80–83 s). Hver hodeløse Shell vekker kortet i 3–4 s; 6A, 6B og 6C kjørte økter samtidig. Ingen kodeendring. Rådata `maal/runde6d-p0-*.csv`, skript `verktoy/runde6d/p0-forsok.sh`.
- **Kjente punkter:** Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y, Ctrl+Enter og tastene på glidere og kurvepunkter i Tastatursnarveier (ny gruppe, to spalter); `3a-sim` bruker `locked::edit_draft`; temperaturmålet og kurvepunktene i °F (`oc::temp_spin`); disknøklene i statusblokken heter «sda temp» osv. med modellen i verktøytipset; Vifter ved 640 px (tidsvinduet og dekningen under hverandre, «Tilbakestill kurven» som ikon); de to testene som feilet under høy last venter på tilstand, og testklienten tåler trege svar.
- **Kritikk:** `kritikk-1.1.0.md`, 29 funn (8 Må, 14 Bør, 7 Kan). Rettet: alle Må og Bør, og 4 Kan. Blant annet: grafhodene kutter aldri tittelen (`fixed::AllOrNothing`), RGB-lys har låst-linja, «Låst» skjules når Bruk er på (sim), sårbarhetene med riktige navn, «lastsnitt», diskradene på Sensorer med kjernenavnet først, tall og enhet brytes ikke fra hverandre (21 tekster), gamle sidenavn borte fra tekstene og `--help`, `bilder.sh --plan alle` har 6A–6C-planene.
- **Versjon:** `app/version.json` og `Cargo.toml` 1.1.0 · bygg 17 (kanal `stable`). `--version` for begge binærene, Om og rapporten viser det.
- **Dokumenter:** `app/README.md`, `STATUS.md`, `PATCHLOG.md`, produktspesifikasjonen (sidelista, navnene, kapittel 6), `maalinger.md` (runde 6D).
- **Målt** (runde 6D): lastsnitt 2–4, ingen andre agenter. Kald start median 176 ms (163–195), lukking 0,6–0,9 ms til vinduet er borte og 10–12,5 ms til prosessen er ute, vinduet i ro 0,40–1,85 % på alle sidene (RGB-lys i 2D animerer viftene: 16,7 %), tjenesten i bakgrunnen 0,72 % og med vinduet åpent 1,80–2,50 %, minne 12–16 MB (vinduet) og 21–22,5 MB (tjenesten), P8 i alle 1 117 prøvene utenom 3 s idet Shell startet. Første forsøk 01:41 stoppet da maskinen gikk tom for minne (andre prosjekter); kjørt på nytt 09:05–09:24 (`verktoy/runde6d/maal6d.sh`).
- **Skjermbilder:** `docs/skjermbilder/1.1.0/6d/` med `bilder.sh --plan alle` (release, cairo). 25 planer og 353 bilder, alle sidene i lyst og mørkt, fire temaer, høy kontrast, 640 og 1920 px, maskinhodet i feiltilstandene, Disker, Prosessor, System, og Klokker og effekt og Vifter mot det ekte kortet. `kuttet.txt` nevner bare diskradene på Sensorer ved 640 px (kjernenavnet og verdien står først), ett modellnavn på Disker ved 640 px, «· 60 °C under nedklokking» på Oversikt i lave vinduer og maskinlinja med «Stor» skrift. Planene `6c-ekte` og `3c` er tatt på nytt etter de siste rettingene på Vifter.
- **Tester:** clippy ren; `cargo test --workspace` to hele kjøringer, 217 bestått og 4 ignorert (215 før).
- **Avbrudd:** økta stoppet ca. 01:56 da maskinen gikk tom for minne (andre prosjekter). Min egen foreldreløse systembuss fra økta `okt-6d-maal` ble stoppet og mappa slettet; en `dbus-daemon` fra en annen agents økt 7. november (`okt.O1aAtc`) står fortsatt, ikke startet av meg. Etterpå: `CARGO_BUILD_JOBS=4`, én privat økt om gangen.
- Ingenting er skrevet til kortet, ingen I2C eller `/dev/sd*` åpnet, ingen av brukerens prosesser rørt, ingen systemd-enheter eller `~/.local` endret. Alle øktene var private og er slettet. Release-bygget ligger i `~/.cache/rigg-build/release/` til hovedøkta installerer.

## 2026-11-09 · Runde 7A · Hjelper og bryter (oppføringen skrevet av 7C)

7A gjorde arbeidet, men rakk ikke skjermbildene, målingene og denne oppføringen. 7C har gjort dem ferdig ut fra koden, `hjelper-sikkerhet.md` og `protokoll.md` kap. 12.

**Det nye (7As egne filer):**
- **`app/crates/rigg-helper` (ny crate, `agentic-rigg-helper`):** root-hjelperen. Den tar imot fem faste operasjoner (`status`, `apply`, `confirm`, `revert`, `reset`, pluss `rgb_*` = `not_implemented`) over `/run/agentic-rigg/helper.sock`, som NDJSON. Vernet:
  - `SO_PEERCRED` mot uid-en i `helper.json`
  - trinnene i `helper.json`
  - grensesjekk mot kortets egne områder (avviser, klemmer ikke)
  - frist med Behold
  - nødregel over 85 °C
  - standard når `riggd` kobler fra, når et trinn trekkes tilbake, og når hjelperen stopper (`--tilbakestill` i `ExecStopPost`)
  - journal for hver endring

  Filene er `main.rs`, `server.rs` (høyst fire forbindelser, 16 KiB per linje), `motor.rs` og `journal.rs`. `--backend sim` er det eneste som kjøres i 1.2.0. `--backend nvml` gir `write_locked` / `real_writes_off` og laster ikke NVML.
- **`app/data/hjelper/`:**
  - `aktiver.sh`, `deaktiver.sh` og `trinn.sh`. Brukeren kjører dem via `pkexec` fra bryteren. Alle tar `--root` for en falsk rot.
  - `agentic-rigg-helper.service`: systemtjeneste med ett privilegium, `ProtectSystem=strict`, ingen nett og bare `/dev/nvidia*`. `systemd-analyze security` gir 1,2.
  - `io.github.AgenticRigg.helper.policy`: `auth_admin` for aktivering, deaktivering og hvert trinn.
- **Klienten i tjenesten** (`riggd/src/hjelper.rs` og videresendingen i `ops.rs`):
  - Finner hjelperen: ved oppstart, hvert 60. s når den er av, og hvert 30. s når den er på.
  - Ruter skrivingen: `helper_off`, `helper_error`, `helper_outdated`, `backend_mismatch`, `step_not_enabled`, `busy` / `not_owner`. Fra `set_*` lages `apply` (`patch_for_op`).
  - Sender `revert` når økta som eier endringen, lukkes, og holder tjenesten i live mens kortet har andre verdier enn standard.
- **Innstillinger › Styring** (`rigg/src/settings/styring.rs`, ny): bryteren «Tillat endring av skjermkortet» (`pkexec` med `gio::Subprocess`), status, trinnlisten med et eget spørsmål før passordet, og «Hva bryteren gjør».
- **`app/scripts/falsk-pkexec.sh`:** for dev, testene og skjermbildene. Gir ingen root og legger alt i en falsk rot.
- **Sidefeltet** (`sidebar.rs`): merket ved Styring viser tilstanden til hjelperen, med teksten fra `widgets/locked.rs` (`badge`, `helper_line`).
- **`install.sh` og `uninstall.sh`:** filene til `~/.local/share/agentic-rigg/hjelper/` uten å aktivere noe. Er hjelperen fortsatt aktiv, sier `uninstall.sh` fra.

**Endringer utenfor filene 7A eide:**
- **`rigg-core/src/proto.rs`:** feilkoden `StepNotEnabled` (`step_not_enabled`, «Trinnet er ikke godkjent.») og operasjonen `get_helper` (`{refresh}` → `capabilities.write.helper`).
- **`rigg-core/src/hjelper.rs` (ny):** det felles for hjelperen, tjenesten og vinduet:
  - `Step` (`effekt`, `vifter`, `klokker`, `lys`), med navn og hva de dekker
  - `STEPS`, `Paths` (stiene under `/` eller en falsk rot), `Config` (`helper.json`) og `PROTOCOL`
  - `steps_for_patch`, `patch_for_op`, `step_not_enabled` og `locked`
  - tre tester
- **`riggd/src/daemon.rs`:** `Options.hjelper` (stiene), feltet `hjelper`, `probe` ved oppstart, `hjelper_session_closed`, `hjelper_tick` i runden og `holds_values()` i «skal tjenesten leve».
- **`riggd/src/main.rs`:** `mod hjelper;` og `--hjelper-rot STI` (og `RIGG_HJELPER_ROT` i dev) til `daemon::Options`.
- **`rigg/src/skjermbilder.rs`:** planene `7a` og `7a-ekte`, og `stop_fake_helper` for feiltilstanden.
- **`rigg-hw/tests/bare_lesing.rs`:** unntakene, og bare de:
  - NVML-skrivekallene i den sperrede `gpu/skriv_nvml.rs`, med egen lukket navneliste
  - `.set_…` i `gpu/skriv.rs` og `rigg-helper/`
  - `pkexec` bare i `settings/styring.rs`, med `gio::Subprocess` og aldri `Command::new`
  - `PKEXEC_UID` i skripttestene

  Testen `ekte_skriving_er_sperret_og_bare_for_hjelperen` sjekker sperren, cargo-funksjonen og at bare hjelperen bruker skrivemodulen.
- **`app/clippy.toml`:** `gio::Subprocess::newv` er forbudt (unntaket er bryteren, merket med `#[allow]`), og kommentaren nevner 7A.
- **`data/theme/innstillinger.css`:** blokken nederst, «Styring (runde 7A)» (`.helper-status-row`, `.helper-status`, `.helper-kv`, `.helper-confirm`, `.helper-warn` …).
- **`rigg/src/settings/mod.rs`:** `pub mod styring;` og delen «Styring» i `SECTIONS` (ikon `shield`), mellom «Logging og varsler» og «Data og tilbakestilling».
- **`rigg-hw`:**
  - `gpu/skriv.rs`: `Skriver`, `SimSkriver`, `Limits`, `EKTE_SKRIVING = false` og `open_nvml`
  - `gpu/skriv_nvml.rs`: bare med cargo-funksjonen `ekte-skriving`, som ingen Cargo.toml slår på
  - `Cargo.toml`: funksjonen

**Tester (7A):**
- 16 i `rigg-helper/tests/`: 12 i `ende_til_ende.rs` og 4 i `skript.rs`
- 8 enhetstester i `rigg-helper/src/motor.rs`
- 6 i `riggd/tests/hjelper.rs`
- 3 i `rigg-core/src/hjelper.rs`
- sperretestene i `rigg-hw`

Lista står i `hjelper-sikkerhet.md` kap. 5.

**Gjort ferdig av 7C:**
- **Skjermbilder:** `docs/skjermbilder/1.2.0/7a/` med `verktoy/runde7a/bilder7a.sh`, uten endring i skriptet. Planen `7a-ekte` er utvidet med Vifter, RGB-lys og Klokker og effekt etter at bryteren er slått av.
- **Målinger:** `maalinger.md`, runde 7A, med hjelperen av. Kald start 163 ms (median), tjenesten i bakgrunnen 0,78 % / 0,71 %, P8.
- **`cargo clippy -p rigg-hw --features ekte-skriving -- -D warnings`** (bare kompilert og lint-sjekket, ikke kjørt) var ren også med `--all-targets`.
- **Viftemotoren i hjelperen:** gjort lik 7Bs beskrivelse og tjenestens sim-sløyfe, med en ny test. Se 7C.

## 2026-11-09 · Runde 7B · Viftekurve

Navnene under er faste (skrevet tidlig, til 7A).

**Nye felt i `rigg_core::txn::Values`** (alle med serde-standard, så gamle utkast og profiler leses som før og får standardverdiene):

| Rust | JSON i `Values` (utkast, profiler) | `GpuSettings` (`apply`/`set_fan`) | Standard | Område |
|---|---|---|---|---|
| `fan_per_fan: bool` | `fanPerFan` | `fan.perFan` | `false` (felles kurve) | – |
| `fan_curves: Vec<Vec<(f64, f64)>>` | `fanCurves` | `fan.curves` | `[]` (= `fan_curve` for alle) | én kurve per vifte, hver som `fan.curve`; høyst 8 |
| `fan_hysteresis_c: f64` | `fanHysteresisC` | `fan.hysteresisC` | 3 | 0–10 °C |
| `fan_ramp_up_pct_s: f64` | `fanRampUpPctS` | `fan.rampUpPctS` | 10 | 1–100 %/s |
| `fan_ramp_down_pct_s: f64` | `fanRampDownPctS` | `fan.rampDownPctS` | 3 | 1–100 %/s |
| `fan_smoothing_s: f64` | `fanSmoothingS` | `fan.smoothingS` | 0 (av) | 0–10 s (vinduet tilbyr 0, 3, 5, 10) |
| `fan_quiet_until_c: Option<f64>` | `fanQuietUntilC` | `fan.quietUntilC` | `null` (av) | 30–80 °C |

`fan_curve` er uendret: felles kurve, og kurven for vifter som mangler i `fan_curves`.

**Viftemotoren** (`rigg_core::vifte`, ren logikk uten I/O):

```rust
let oppsett = vifte::Oppsett::fra_values(&values, vifter /* 2 */, min_pct /* 35 fra GetMinMaxFanSpeed */);
let mut motor = vifte::Motor::new(oppsett);           // .med_start(&[35.0, 35.0]) for å starte fra farten kortet har
let steg = motor.steg(Some(temp_c), naa_ms);          // hver runde; None = ukjent temperatur
steg.prosent                                          // Vec<f64>, én per vifte, alltid min_pct..=100
motor.sett_oppsett(nytt);                             // ny kurve uten å miste rampen og hysteresen
```

**Ferdig (runde 7B).** Det som er gjort, i tillegg til navnene over:

- **Viftemotoren** (`rigg-core/src/vifte.rs`, ny, ren logikk uten I/O): nødregel (85 °C → 100 % straks, tilbake under 80 °C, på rå temperatur, kan ikke slås av) → utjevning (snitt over 0–10 s) → hysterese (opp straks, ned først når temperaturen har falt hysteresen under punktet der viftene gikk opp; det samme som kurven forskjøvet mot venstre på vei ned) → kurve per vifte eller felles, med stille flate («hold minimum til») → minste fart (35 %, fra `GetMinMaxFanSpeed`) → rampe (% per sekund, opp og ned hver for seg). Ukjent temperatur: farten holdes i 5 s, så 100 %. `Oppsett::renset` gjør alt trygt (NaN, tomme kurver, tall utenfor områdene). `simuler` kjører motoren over historikk. 29 tester, fem av dem egenskapstester over tilfeldige oppsett (egen xorshift, ingen nye avhengigheter): alltid 35–100 %, rampen holdes, varmere gir aldri lavere fart, farten ender på målet, hysteresen holder farten innenfor båndet.
- **`Values` og valideringen** (`txn.rs`): de sju feltene over, `check_curves` (0–8 kurver, feilmeldingen sier hvilken vifte), `merge` og `set_fan` tar de nye feltene, `diff` har navnene, `to_json` sender dem, `Values::curve_for(i)`, og `Book.emergency` (satt av viftesløyfen, vises i `get_state`). Tester: et utkast fra 1.1.0 leses med standardene, gammel `GpuSettings` endrer ikke de nye feltene, klemming, ugyldige felt, `to_json` går rundt gjennom `merge`, `set_fan` per vifte.
- **Tjenesten (sim):** `riggd/src/vifter.rs` (ny): varmemodellen og motoren per kort. Auto følger driverens kurve i sim (policy 0), Manuell fast fart, Kurve motoren (policy 1). `extra.styring.fan` i de faste dataene har kurven og oppførselen som er i bruk (bare sim), så Vifter viser kurven kortet følger. `rigg-hw/src/gpu/sim.rs`: `Termo` (temperaturen går mot `31,6 °C + effekt × R(vifte)` med 20 s tidskonstant, R synker med farten) og scenarioet `sim:spill` (90 s med 80–95 % last, 60 s i ro). `daemon.rs`: `overlay_sim` bruker viftesløyfen; `main.rs`: `mod vifter;`.
- **Vifter** (`pages/viftekurve.rs`, `viftekurve.css`):
  - «Felles kurve · Vifte 1 · Vifte 2» over grafen. Med egne kurver vises den andre viftens kurve svakt prikket bak; tilbake til felles husker utkastet kurvene per vifte.
  - Hysteresen i grafen: skravert bånd mellom kurven og nedkurven (stiplet), «Ned · 3 °C hysterese», og «(ned 55 %)» i markøren.
  - «Hold minimum til» i grafen: flat på minste fart, loddrett sprang, «Minst 35 % til 55 °C», og punktene under flaten hule.
  - Nytt panel **Oppførsel**: glidere for hysterese (0–10 °C) og rampe opp og ned (1–50 %/s) med standardmerke, utjevning (Av, 3 s, 5 s, 10 s), «Hold minimum til __ °C» (bryter og tallfelt i °C/°F), og forklaring i klartekst under hver («Fra 35 % til 100 % på 7 s»). «Viftestopp (0 %) er ikke mulig på dette kortet: fastvaren holder minst 35 %. Det gjelder også Afterburner.»
  - Nytt panel **Siste 5 minutter med kurven/utkastet**: motoren kjørt over historikken (temperatur per sekund) ved siden av slik viftene faktisk gikk, med snitt, høyeste og antall endringer for begge. Hentes hvert 10. s mens siden vises; regnes bare på nytt når utkastet eller historikken er ny.
  - Forhåndsvalgene setter kurve og oppførsel (felles): Stille (minimum til 55 °C, 5 °C hysterese, 5/2 %/s, 5 s utjevning), Balansert (= standard), Ytelse (2 °C, 20/5 %/s). Navnet blir «Stille · endret» når bare oppførselen er endret. «Tilbakestill kurven» tar alt om viftekurven.
  - «Slik følges kurven»: nødregelen, rekkefølgen i motoren, mellom punktene, fristen for Behold. Teksten om «ingen hysterese» er borte.
  - Grafen, merkene på gliderne og nødregelteksten tegnes på nytt når enheten byttes (før sto aksene i °C også med °F).
  - Med sim og kurven i bruk: modus «Kurve · tjenesten styrer» og kurven heltrukket med «Viftene følger denne kurven nå».
- **Utenfor filene mine (små og nødvendige):** `pages/kontroller.rs` (6C): `..d` i `Read::values` og i én test, de nye feltene i endringslista (`field_text`) og skjult utenfor modus Kurve (`draft_diff`). `pages/styring_bilder.rs`: planene `7b-ekte` og `7b-sim`, og `draft_vifte` bruker `Preset::apply`. `docs/produksjon/verktoy/runde7b/bilder7b.sh` (ny).
- **Til 7A (hjelperen):** `Oppsett::fra_values(&values, vifter, min_pct)` med `min_pct` fra `nvmlDeviceGetMinMaxFanSpeed` og `vifter` fra `nvmlDeviceGetNumFans`. `Motor::new(o).med_start(&farten_kortet_har)` når Kurve slås på, `steg(Some(temp), nå_ms)` hver runde (`None` når temperaturen ikke kan leses), og `steg.heltall(min_pct)` til `nvmlDeviceSetFanSpeed_v2(vifte, %)` per vifte. `sett_oppsett` ved ny `confirm` beholder rampen. `steg.nod` → `Book.emergency`. Motoren gjør ingen I/O, kjenner ikke policy og skriver aldri; Auto (policy tilbake til 0) og ruller-tilbake er hjelperens ansvar. Nødregelen ligger i motoren også, så hjelperen trenger ikke en egen for viftene.
- **Tester:** `cargo clippy --workspace --all-targets -- -D warnings` ren; `cargo test --workspace --no-fail-fast` 299 bestått, 4 ignorert. Nye: 29 i `vifte.rs`, 6 i `txn.rs`, 5 i `riggd/src/vifter.rs`, 2 i `sim.rs`, 4 nye og én endret i `viftekurve.rs` (forhåndsvalg, felles og per vifte, linja som tegnes mot motoren, forhåndsvisningen, tilbakestill).
- **Skjermbilder:** `docs/skjermbilder/1.2.0/7b/` (29 bilder): `7b-ekte/` (låst: 1920 lyst og mørkt og hel side, utkast Stille, per vifte lyst og mørkt, hysterese 8 °C, °F, høy kontrast lyst og mørkt, 640 lyst og mørkt og hele siden) og `7b-sim/` (`sim:spill`: Auto, utkast, Behold, kurven i bruk lyst og mørkt og hel side, per vifte utkast og i bruk, hysterese, 640 og 640 høy kontrast mørkt). `kuttet.txt` finnes ikke: ingen etikett er kuttet.
- **Målt** (`maalinger.md`, runde 7B): vinduet i ro på Vifter 0,67 % (ekte, låst), 0,70 % (utkast per vifte), 0,63 % (sim med kurven i bruk); tjenesten med NVML 0,67 % alene og 1,96 % med vinduet; sim-tjenesten med viftesløyfen 0,03–0,09 %; P8 i alle prøvene mens vinduet var åpent.
- Ingenting er skrevet til kortet, ingen I2C er åpnet, ingen av brukerens prosesser er rørt, ingen systemd eller `~/.local`. Øktene (`okt-7b-*`) var private, én om gangen, og er slettet. Release-bygget i `~/.cache/rigg-build/release/` har 7Bs endringer.

## 2026-11-09 · Runde 7C · Sluttrunde (1.2.0 · bygg 18)

Eneste agent etter 7A og 7B. Alle filer.

**7A gjort ferdig:**
- **Skjermbilder:** `docs/skjermbilder/1.2.0/7a/` med `bilder7a.sh`, uten endring i skriptet:
  - av, venter, på, godkjenn-dialogen (Effekt og Klokker), trinn godkjent, feil og av igjen
  - sidefeltmerket i alle tilstandene
  - lyst og mørkt, 1440 og 640 px
  - `7a-ekte` med låst-linja på alle tre Styring-sidene
- **Målinger:** `maalinger.md`, runde 7A.
- **LOGG-oppføringen for 7A** (over).
- **`cargo clippy -p rigg-hw --features ekte-skriving -- -D warnings`:** ren, også `--all-targets`. Bare kompilert, ikke kjørt.
- **Hjelperen bruker nå 7Bs `vifte::Motor` slik 7B beskrev.** Lagt til:
  - `med_start(Skriver::fan_percents())`, en ny lesing, med `nvmlDeviceGetFanSpeed_v2` i den sperrede `skriv_nvml.rs` og på lista dens
  - `sett_oppsett` bare når oppsettet er nytt
  - ett `steg` per runde, også under nødregelen, så viftene rampes ned fra 100 % etterpå
  - `status.emergency` også når motoren har gått til 100 % fordi temperaturen er ukjent

  `SimSkriver` har fart per vifte. Sim-sløyfen er flyttet fra `riggd/src/vifter.rs` til `rigg-hw/src/gpu/sim_vifter.rs` (`riggd` bruker den derfra, og `mod vifter` er borte), så hjelperens nye test `kurven_er_lik_tjenestens_sim_sloyfe` kan kjøre den samme koden. Testen sammenligner 480 s med egne kurver per vifte, hysterese, rampe, utjevning, stille flate og nødregelen, sekund for sekund. Den feiler med den gamle koden.

**Kritikk:** `kritikk-1.2.0.md`, 28 funn (6 Må, 12 Bør, 10 Kan). Rettet: alle Må og Bør, og 8 Kan. Blant dem:
- **Låst-linja, toasten og «Utkast · låst»** ber ikke om et trinn som ikke kan godkjennes mot det ekte kortet.
- **Feilstatusen** er i klartekst, med det tekniske i verktøytipset.
- **«Hvem»-raden** lover ikke mer enn `hjelper-sikkerhet.md` 4.1.
- **«Hva skal til?»** har de fire trinnene, med knapp til Innstillinger › Styring.
- **Innstillinger › Styring** er uten sjargong, og statusen gjentar ikke merket.
- **Godkjenn-dialogen** har tydeligere tekst, og trinnene merkes «Kommer senere» eller «Ukjent».
- **Sidefeltmerket:** «Endring på · låst», «Endring: slå på igjen».
- **RGB-lys** har ikke lenger «I2C … ikke slått på».
- **«Endring» brukes overalt** i stedet for «skriving»: Bruk-tipset, «Når endring er slått på» og «Lukkes Agentic Rigg helt».
- **Vifter:**
  - «Siste 5 minutter» forklarer vifte 1 og vifte 2.
  - «Nødregel 100 %» står ikke der kurven går.
  - Flisene står 3 + 3 ved 640 px (`StatStrip::balance`).
- **Innstillinger ved 640 px:** vindusknappene står i høyre kant.
- **`--version`:** samme form for hjelperen, og `aktiver.sh` leser begge formene.
- **`bilder.sh`:** 7A- og 7B-planene er med i `alle`, og standardmappa er 1.2.0.
- **De tre Kan-punktene fra 6D:**
  - P8 står én gang; Begrenset av viser «Ingenting · Kortet er i ro»
  - utkast-merket står bare når utkastet er ulikt kortet
  - Disker ved 640 px er uten tilkoblingsbrikke

**Versjon:** `app/version.json` og `Cargo.toml` er 1.2.0 · bygg 18. Om, rapporten og `--version` for alle tre binærene viser det. Hjelperen skriver «Agentic Rigg-hjelperen 1.2.0 · bygg 18».

**Dokumenter:**
- `app/README.md` har den nye delen «Endring av skjermkortet»: hva hjelperen er, at den er av til brukeren slår den på, hvordan den slås på og av, at trinnene er av, og at det ekte kortet er sperret.
- `STATUS.md`, `PATCHLOG.md` (1.1.0 · bygg 17 og 1.2.0 · bygg 18), produktspesifikasjonen (6.7, 6.2, 6.5, 3.10 og versjonen), `protokoll.md` (sim-sløyfen og hjelperens viftemotor) og `hjelper-sikkerhet.md` (testene).

**Installasjonsprøven** mot en midlertidig HOME (`RIGG_INSTALL_NO_SYSTEMD=1`), over en kopi av den installerte 1.0.0 · bygg 16 uten hjelpermappe:
- `install.sh` gikk rent.
- Seks hjelperfiler havnet i `~/.local/share/agentic-rigg/hjelper/`, like kildene, med 755/644.
- `.desktop`-fila er gyldig (`desktop-file-validate`). Ikonene er på plass: 51, med alle fem størrelser og den symbolske.
- README ble med.
- En ny kjøring gikk rent.
- `uninstall.sh` (rc 0) fjernet alt utenom `ui.json` og tomme mapper.
- Ekte `~/.local` er ikke rørt.

**Målt** (runde 7C, lastsnitt 1,9–3,7):
- kald start median 182 ms (162–190 fra tredje start)
- lukking 0,6–1,0 ms
- vinduet i ro 0,40–1,75 % (RGB-lys 2D for seg: 16,6–16,9 %)
- tjenesten i bakgrunnen 0,76 / 0,71 % og 1,80–2,45 % med vinduet
- minne 12–16,7 MB (vinduet) og 21–22,5 MB (tjenesten)
- P8 i alle prøvene utenom 3 s idet Shell startet

**Skjermbilder:**
- **Hele serien:** `docs/skjermbilder/1.2.0/7c/` med `bilder.sh --plan alle` (release, cairo): 29 planer og 414 bilder. Det dekker alle ti sidene i lyst og mørkt, fire temaer, høy kontrast, 640 og 1920 px, feiltilstandene, 7A og 7B.
- **Tatt på nytt:** 6b, 7a og 7a-ekte, etter rettingen av vindusknappene i Innstillinger.
- **`kuttet.txt`** har bare det 6D godtok: diskradene på Sensorer ved 640 px (kjernenavnet først), «· 60 °C under nedklokking» i lave vinduer, lange stier fra den private økta og maskinlinja med «Stor» skrift. Modellnavnet på Disker er ikke lenger kuttet.
- **`docs/skjermbilder/1.2.0/7a/`** er tatt på nytt med det endelige bygget.

**Tester:**
- `cargo clippy --workspace --all-targets -- -D warnings` er ren.
- `cargo test --workspace --no-fail-fast` ga to hele kjøringer med **300 bestått, 4 ignorert**, mot 299 før. Den nye testen er `kurven_er_lik_tjenestens_sim_sloyfe`.

**Release:** de tre binærene ligger i `~/.cache/rigg-build/release/` og viser 1.2.0 · bygg 18.

**Opprydding og sikkerhet:**
- Ingenting er skrevet til kortet. Ingen I2C eller `/dev/sd*` er åpnet, ingen `pkexec`, `sudo` eller `systemctl`, og ingen av brukerens prosesser er rørt. Den gamle `dbus-daemon` (PID 2426879) står igjen.
- Økta som ble avbrutt da jeg stoppet den første serien, la igjen en privat systembuss (`dbus-daemon`, PID 1303009, startet av meg 13:12). Den er stoppet, og øktmappa er slettet.
- Alle øktene og de falske røttene er ryddet. Ekte `~/.local` og systemd er ikke rørt.

**Gjenstår:**
- Kan 19 og 28 i `kritikk-1.2.0.md`.
- Hovedøkta installerer 1.2.0.
- Trinn 2 (effektgrense mot det ekte kortet) følger `hjelper-sikkerhet.md` kap. 6, med brukeren til stede.

## 2026-11-09 · 9R · Research: ekte skriving og RGB (plan for 1.4.0)

- Skrev `plan-1.4.0-skriving-og-rgb.md`: sammendrag, RGB-plan (A), NVML-trinnplan B0–B4 med sjekklister og tilbakerulling, og kapittelet «ThunderMaster-paritet».
- **RGB:** OpenRGB-innlegget #1420 gjelder akkurat dette kortet (Palit GamingPro OC RTX 2080, `10DE:1E87`/`1E8710DE`). ThunderMaster-opptakene viser 0x49, signatur «PALIT 150» i `0xF6–0xFE` og PalitGPUv2-registrene `0x60–0x75`. OpenRGB støtter ikke kortet (ingen detektor, #1420 ikke løst). Ingen kjent «lagre på kortet»-kommando. `i2c-dev` er innebygd i kjernen (`CONFIG_I2C_CHARDEV=y`). Port 1 er `i2c-0` i dag, USB-C er `i2c-10`.
- **NVML:** Driveren krever `CAP_SYS_ADMIN` (`NV_IS_SUSER()` i den åpne modulen). `ProcSubset=pid` i enheten skjuler `/proc/driver/nvidia` og `/proc/devices`, som NVML leser. Effekt, vifter og offset virker på Turing via NVML på Wayland. Temperaturmålet er usikkert på GeForce. Viftestopp er ikke mulig (35 % i VBIOS, og kortet har ikke viftestopp).
- Bare lesing: sysfs-navn og -stier, `lspci`, `udevadm info -a`, `nvidia-smi -q`, `nvml_probe.py`. Ingen `/dev/i2c-*` åpnet, ingen set-kall, ingen sudo/pkexec, ingen kode endret.

## 2026-11-09 · Runde 8A · Oversikt over hele maskinen og akser som ikke kutter (1.3.0 · bygg 19)

Eneste agent. Alle filer. Brukerens ønsker: grafene skal stige naturlig mot 80, 95 og 110 °C, Oversikt skal vise hele maskinen som GNOME Resources, og (fra hovedøkta underveis) tomrommet på Disker og Styring som tydelig gjelder skjermkortet, med kortet funnet av seg selv.

**Aksene (`widgets/chart/math.rs`).**
- **Det som var feil:** `y_range` fulgte dataene med «Tilpass», men klemte resultatet til grafens faste område (`temp_range()` 20–100 °C, disk 20–60 °C, effekt, klokker og VRAM med egne tak), og «Fast» viste bare det området. Verdier over ble en flat strek i toppen, verdier under forsvant, og `y_of` klemte i tillegg hvert punkt. Sparklinjene med fast område (`downsample::range`) klemte på samme måte, og «Siste 5 minutter» på Vifter delte en akse på 0–100 for % og °C.
- **Nå:** `Axis { pref, hard, min_span, auto }` og `y_axis(data, &axis, marks)`. Det faste området er foretrukket: dataene utenfor utvider aksen med 6 % luft til en pen skala (fem eller seks merker, det tetteste), med «Tilpass» og «Fast». Innenfor er alt som før. `hard` bare for last og minne i prosent (`HARD_PCT`), og `HARD_POS` (ikke under 0) for W, MHz, MiB og MB/s. Viftene har ingen hard grense: NVML kan melde over 100 %.
- **Ingen pumping:** `AxisHold` i hver graf (nøkkel: modus, enhet, synlige serier, vindu). Utvides straks (unionen med den andre kanten holdt), krymper først etter 15 s med ønske om mindre, og da til det største ønsket i perioden. Et bilde uten data (historikken hentes på nytt etter ny bredde) lagrer ingenting; det dro først aksen tilbake til hele området (sett i de første skjermbildene).
- **Grenselinjer** (`LimitLine`, `ChartOpts.limits`, `gpu_temp_limits`): nedklokking og avstenging fra NVML stiplet i gult og rødt med navn til høyre på en flate; tekst som kolliderer, skjules (`limit_labels`). En grense innenfor 15 % over kurven tas med i aksen. Effektgrensen er fortsatt en serie.
- **Overalt:** `def_panel` (Oversikt, Grafer, Klokker og effekt, Vifter), Sensorer (raden), Disker (hastigheten), `fans_panel`, sparklinjene, og Vifter «Siste 5 minutter» (`preview_scale`, steg på 25, «%» bare 0–100). Målerne på Oversikt starter under 20 °C når kortet er kaldere, og slutter på verdien over avstenging («Nå»). Kurveredigereren er uendret.
- **Tester:** 13 i `math.rs` (`akse_tester`: 20–45, 80, 95, 110 °C, under 20 °C og under 0, °F, effekt over taket, disk 65 °C, prosent, grenselinjene, at aksen ikke pumper, uten data, unionen, og eksemplene i PATCHLOG), én i `tests/chart.rs`, `downsample::fast_område_kutter_ikke` og `siste_fem_minutter_kutter_ikke_varme`.

**Oversikt (`pages/oversikt.rs`, skrevet på nytt).**
- **Fliser** (`TILES`, `tile_views`): Prosessor, Skjermkort, Minne, VRAM, Disker, Nettverk, med ikon, status til høyre (P-tilstand, «alle OK» / «1 advarsel»), tall og enhet, to undertall med gul/rød (temperatur over grensene, effekt ved grensen) og sparklinje. Klikk går til siden for delen. `Cols` 6/3/2 spalter (900 og 560 px). Disk-sparklinja er lesing + skriving (`sum_series`); `disk/read` og `disk/write` fylles nå fra historikken ved tilkobling (`app.rs`).
- **Tilstand** (`state_items`, ren funksjon med tre tester): kortet borte, ny driver, ingen driver (info), nedklokking (bare termisk og maskinvare, ikke effektgrensen), GPU/CPU over grensene eller ved nedklokkingsgrensen, minne ≥ 90/95 %, disk med advarsel eller feil (fra `get_disks`, hvert 30. s mens siden vises). Høyst tre og «+N», alvorligst først, klikk åpner siden. Skjult ellers.
- **Grafene:** Last (GPU og CPU), Temperaturer (GPU, CPU og de to varmeste diskene, med grenselinjene), GPU-effekt, Minne og VRAM (%, GiB i verktøytipset), Disk-I/O, Nettverk. 3/2/1 spalter (1240 og 700 px), 150 px høye. Bygges på nytt bare når kortet kommer eller går, eller lista over disker med temperatur endres.
- **Skjermkortet nederst:** Temperatur og effekt (målerne, delene deler høyden) og Skjermkort (P-tilstand, kjerne- og minneklokke med stolpe, vifter med turtall, viftestyring, PCIe, video) med «Grafikkort ›». Klokker-panelet (fire klokker) står på Grafikkort fra før; VRAM står i flisa og grafen.
- **Tomt og feil:** ingen driver (flisa sier det, VRAM og skjermkortdelen skjules, grafene uten GPU), kortet borte (flisa «svarer ikke», tilstand-linja, grafenes hull «Kortet svarer ikke»), ingen UDisks2 («helse ukjent»), ingen disktemperatur («uten temperatur»).
- **Ikke laget:** topp-prosesser. Tjenesten har ingen prosessliste for hele maskinen; den måtte lest alle `/proc/*/stat` hvert sekund. Ikke billig nok for startsiden.
- Legenden i grafhodet bruker `AllOrNothing` opp til fem serier (før tre), så tittelen aldri kuttes.

**Disker:** Disk og SMART i `Cols` (1:1, stables under 720 px) med like høye kort og radene fordelt (`fill_rows`), Partisjoner i hele bredden under. Før: masonry med Partisjoner under SMART og et tomt felt under Disk.

**Skjermkortstyring.**
- Seksjonen heter «Skjermkortstyring» (`section_title`), med kortet under tittelen (`nav-target`: «RTX 2080», «Palit RTX 2080»; klikk åpner kortvalget i maskinhodet). Toppfeltet har «Skjermkort ›» foran tittelen (`pages::crumb`, `full_title` i vindustittelen og for skjermleseren). Startside: «Skjermkort › Vifter» osv. Innstillinger › Skjermkortstyring (var Styring), alle tekstene som viste dit, tastatursnarveiene og verktøytipsene i sidefeltet er endret. Panelhodene: «Kortets viftekurve», «Hvem styrer kortets vifter», «Kortets vifter», «Lyskontrolleren på kortet».
- **«Styrer»-linja** (`widgets/styrer.rs`, `TargetLine`) øverst på Klokker og effekt, Vifter og RGB-lys: navn, produsent, PCI-adresse, VBIOS og driver. Flere kort: «Kort 1 av 2» med valget (`App::select_gpu`, også brukt av maskinhodet). Borte, ingen driver, ny driver: gul eller rød tekst og «styringen er utilgjengelig».
- **Produsenten** (`rigg_core::vendor`, to tester): leverandør-ID i PCI-subsystemet (22 kjente kortprodusenter). 10de gir «NVIDIA-referansekort» med forklaring; dette kortet melder 10de:1e87. «Produsent (valgfritt)» lagres per kort i `Settings::gpu_makers` (`gpuMakers`, validert, høyst 40 tegn, én test).

**Sim:** `sim:hete` (`rigg_hw::gpu::sim::hete`, 240 s-syklus: 17 °C, 95 °C, 110 °C med 300 W, avkjøling), og tjenesten legger på prosessoren (14–98 °C) og to disker (65 og 14 °C) etter viftesløyfen (`overlay_hete`, `Options.hete`). Én test.

**Ytelse (funnet under målingen).** Første måling ga Oversikt 4,60–5,85 %. Rettet: VRAM-grafen kopierte kortets faste data per punkt (nå `App::with_statics`, også i `def_panel`); målerne og klokkestolpene gled hvert sekund med kortet i P0 (nå `set_still` på Oversikt og Grafikkort); seks grafer rullet hver for seg (nå `Chart::set_calm` på Oversikt: flytter seg med prøvene, ett bilde i sekundet); diskflisa byttet verktøytips hvert sekund; stolper og målere melder verdien til skjermleseren bare når hele prosenten endres. Linja om kortet øverst (`infobar.rs`) vises ikke lenger på Oversikt og Styring-sidene, der tilstand-linja og «Styrer»-linja sier det samme.

**Målt** (`maalinger.md`, runde 8A, `verktoy/runde8a/maal8a.sh` og `profil8a.sh`). Andre programmer holdt kortet i P0 (908 av 1 123 prøver, også før appen startet) og lastsnittet på 4,6–16,8, så tallene er høyere enn i 7C. Kald start 236 ms (median av 15), lukking 0,8–1,6 ms, tjenesten i bakgrunnen 0,63 / 0,73 %, med vindu 1,65–2,35 % (enkeltmålinger opptil 3,00 %), minne 12,1–17,1 MB og 21,2–22,5 MB. Vinduet i ro: Oversikt 2,25–2,50 % (1.2.0 i samme time: 4,95–8,55 %), Grafikkort 1,00–2,50 % etter rettingen (1.2.0: 5,75–7,00 %), System 0,50 %, Sensorer, Prosessor og Grafer 2,05–3,20 % (ikke endret i 8A utenom aksene; Grafer mot 1.2.0 i samme time: 1,40–1,45 mot 1,65–1,85 %). **Ikke målt i rolige forhold:** bør gjøres i brukerens økt etter installasjonen.

**Finpuss etter bildene:** «over» i stedet for «ved» nedklokkingsgrensen når kortet er over den, og bare det (ikke også «over rød grense»); målerne får avstenging og kortets maks som merker når verdien er over skalaen; feilen under Skjermkortstyring i sidefeltet er kortere («RTX 2080 · ny driver»); nettgrafen har 0–1 MB/s som foretrukket område (før 0–1000, en strek ved null); aksetallene har desimalene steget trenger (`axis_decimals`: 0,25 ga «0,2 0,5 0,8», 2,5 ga «3 5 8»).

**Versjon og dokumenter:** `app/version.json` og `Cargo.toml` 1.3.0 · bygg 19. `app/README.md` (Oversikt, aksene, «Hvilket kort styres?», Innstillinger › Skjermkortstyring), `STATUS.md`, `PATCHLOG.md` (1.3.0 · bygg 19, med Disker og Skjermkortstyring), produktspesifikasjonen (3.11, 3.12, 6.8, sidelista, Disker og `no_driver`), `maalinger.md` (runde 8A). `scripts/bilder.sh`: planene `8a` og `8a-hete`, standardmappa 1.3.0.

**Skjermbilder:** `docs/skjermbilder/1.3.0/8a/` med `bilder.sh --plan alle` (release, cairo): 31 planer og 470 bilder. `8a` og `8a-hete` er tatt på nytt med det endelige bygget, og `6b-driver-oppdatert` etter den siste rettingen i sidefeltet. `8a-hete` viser 110 °C, 95 °C og under 20 °C med Tilpass, Fast og °F på Oversikt, Grafer, Klokker og effekt, Vifter og Disker, og tilstand-linja med GPU over nedklokkingsgrensen og CPU over rød grense. Styrer-linja: ett kort (`8a`, uten og med «Palit»), to kort, kortet borte, ingen driver og ny driver (`6b-*`). `kuttet.txt` har bare det 6D og 7C godtok (diskradene på Sensorer ved 640 px, lange stier fra den private økta). Sett gjennom: Oversikt i alle bredder og temaer, hetebildene, Disker ved 640 og 1920 px (én partisjon; med flere blir bare tabellen i hele bredden høyere, Disk og SMART står uansett like høye), Styring-sidene i tilstandene. Disker ved 1440 px og en utfoldet disk med mange partisjoner er ikke tatt bilde av.

**Tester:** `cargo clippy --workspace --all-targets -- -D warnings` er ren. `cargo test --workspace --no-fail-fast`: 338 bestått, 4 ignorert, 0 feilet (300 før). Nye: 14 i `math.rs` (`akse_tester`, telles to ganger), `fast_område_kutter_ikke`, `siste_fem_minutter_kutter_ikke_varme`, fire i `oversikt.rs` (tilstand-linja og summene), to i `rigg_core::vendor`, `produsenten_per_kort` og `hete_går_fra_under_20_til_110_grader`. `akse_tester` i `math.rs` kjøres to ganger (i appen og i `tests/chart.rs`, som tar inn fila med `#[path]`). `riggd/tests/hjelper.rs::feil_uid_gir_feil_og_las` feilet i to hele kjøringer under last: hjelperen avviste og lukket forbindelsen før tjenesten skrev, og grunnen ble «Broken pipe» i stedet for «annen bruker». Rettet i `riggd/src/hjelper.rs` (`request_inner` leser avvisningen etter en feilet skriving); `cargo test -p riggd` og hele kjøringen er grønne etterpå. `riggd/tests/kontrakt.rs::leseoperasjonene_svarer` spurte etter kortlista før første runde under lastsnitt 12; testen venter nå på den (høyst 25 s), som de andre kontrakttestene.

**Opprydding og sikkerhet:** Ingenting er skrevet til kortet. Ingen I2C eller `/dev/sd*`, ingen `pkexec`, `sudo` eller system-`systemctl`, ingen av brukerens prosesser rørt (demoserveren, den installerte `agentic-rigg`/`agentic-riggd` og `dbus-daemon` 2426879 står). De installerte 1.2.0-binærene ble bare lest (kopiert til `~/.cache/rigg-build/v120/` for sammenligningen, slettet etterpå). Én privat økt om gangen; alle øktmappene er slettet. Ekte `~/.local` og systemd er ikke rørt. Release-bygget i `~/.cache/rigg-build/release/` viser 1.3.0 · bygg 19.

**Gjenstår:**
- Måle Oversikt og de andre sidene i rolige forhold (kortet i P8), helst i brukerens økt etter installasjonen. Under last med kortet i P0 er Oversikt 2,25–2,50 % (krav 2 %).
- Topp-prosesser på Oversikt (ikke laget, se over).
- Med fem grafer (uten driver) står én tom plass nederst til høyre.
- Hovedøkta installerer 1.3.0.

## 13. november 2026 – Codex: 1.3.1 · bygg 20, kortmeny, sidefelt og søk

Etter brukerens ønsker: kortdetaljene under Skjermkortstyring åpnes ved raden som klikkes, med samme bredde som raden og en roligere gruppering av feltene. Klikk på skjermkortet i maskinhodet åpner Grafikkort direkte. Ikoner og treffområder i sammenlagt sidefelt er sentrert, inkludert Innstillinger, hjelp og utviding. `Fixed` begrenser kortmenyen; lange verdier brytes, og med flere kort vises det valgte med et merke og tilgjengelig beskrivelse.

Søk i Innstillinger slipper fokus ved klikk utenfor uten å spise klikket. Første Esc tømmer og avslutter et aktivt søk; neste Esc går tilbake. `clear_search` tømmer tekst og forsinket søk synkront ved seksjonsbytte. Dermed dukker ikke gamle treff opp etter rask skriving og seksjonsbytte. Kryssknappen oppdaterer treffene straks, og skjulte innstillinger tegnes ikke av forsinkede søkesignaler.

Kontroll: clippy på alle workspace-mål er ren. Workspace-testene gir 338 bestått, 4 ignorert og 0 feilet. `kortmeny` er prøvd med ett og to simulerte kort, lyst/mørkt, høy kontrast, 116 % skrift, lang drivertekst og lavt vindu. Den synlige kortflaten er 219 px mot radens 219 px, og 244 mot 244 med stor skrift; skyggen måles separat. Innstillinger og navigasjonsikonene har samme midtlinje, x = 28 px. `innstillinger-sok` verifiserer seksjonsbytte før søkeforsinkelsen, Ctrl+F, tømming, Esc og stop-search. Pekerprøven bruker en privat GNOME-økt med den eksisterende 6B-testutvidelsen og bekrefter faktiske menyplasseringer og fokusrammen før/etter klikk utenfor.

Alle tre debug-binærene viser 1.3.1 · bygg 20. Versjonsfil, Cargo-manifest/lås, README, status og patchlog er oppdatert. Skjermbilder ligger i `docs/skjermbilder/1.3.1/`; en lokal skjermbildevisning er lagt på 127.0.0.1:4320. Ingen installasjon, omstart av brukerens tjeneste eller nye ytelsespåstander. Ingen endring av ekte maskinvare eller webdemo. To lesende Sol-gjennomganger av produksjonsendringene og en av søkefiksen fant ingen konkrete regresjoner etter finpussen.

## 14. november 2026 – Codex: 1.3.2 · bygg 21, akser og full gjennomgang

Brukeren ba om kontroll av alle temperatur-, klokke- og andre målegrafer, og deretter kodegjennomgang og UI/UX-test. Sol-agentene kartla alle 19 ChartOpts-opprettelser og de egne spark-/viftevisningene. Målegrafer hadde allerede ekspanderende normalområder, men to reelle hull ble rettet: ferske langtidsbøtter/interpolasjon ved vinduskanter og viftekurvens målte punkter utenfor redigeringsområdet. Reviewen fant også at pre-clamp av Y-verdier endret helningen når en topp forlot vinduet; avbildningen er nå lineær og selve tegneflaten klipper. Viftevisningen utvides for kulde, 110 °C og 108 % vifte, mens input fortsatt begrenses lovlig. Aksen fryses under dragging og ventende topper tas med etter slipp.

Kontroll: workspace-clippy på alle mål er ren; 347 tester bestått, 4 ignorert, 0 feilet. Ny tabelltest dekker temperatur, GPU-/CPU-klokker, minneklokke, effekt, disktemperatur og nett-/diskrate med Fast og Tilpass. Regresjonstestene dekker framtidsbøtter, hull, bånd, vinduskanter, korrekt helning og inputtransformasjoner. Skjermbildeplanen `adaptive-akser` viser 110 °C, 2500/10000 MHz, 400 W, 108 % vifte og 90 °C disk, også med °F og 48 t. Testverdiene injiseres bare i den private sim-visningen, aldri i maskinvaren.

UI/UX: faktisk peker klikket ti sider og ni innstillingsdeler ved 1440 × 900. Alle ti sider ble åpnet via snarveier ved 640 × 760. En ekstra runde klikket alle ni innstillingsdeler med faktisk vindusramme logget; Sidefelt økte minstebredden fra 640 til 683 px. Søk/Esc, direkte maskinhode-navigasjon og raske tastaturtilfeller er med. De eksisterende planene `3a-sim` og `6c-lys` kontrollerte Bruk/Behold/Rull tilbake og 2D/3D-forhåndsvisning mot simulering. Alle private økter er avsluttet.

Kodegjennomgangen står i `kritikk-1.3.2.md` med ti åpne funn, prioritet, trigger, konsekvens og forslag. To rigg-core-programprøver bekreftet tap av historikktopp etter gjeninnlesing av åpen bøtte (110 ble 40 °C) og tap av standardserier med 64 CPU-tråder (112 plasser fylt, 19 serier avvist, ingen minnehistorikk). Private UI-prøver bekreftet sensorfilter etter raskt Esc, ignorert innstillingsargument i eksisterende vindu, foreldet «Nå» og minstebreddeavvik. Øvrige funn er kildebaserte. Disse eldre feilene er rapportert og ikke automatisk rettet som del av grafoppgaven.

Alle tre testbinærene viser 1.3.2 · bygg 21. Kildeversjon, lås, dokumentasjon og skjermbildeplaner er oppdatert. Bildene ligger i `docs/skjermbilder/1.3.2/`, og localhost-visningen på 127.0.0.1:4320 er oppdatert. Den installerte appen, ekte tjenester og maskinvare er ikke endret. Ingen nye ytelsesmålinger eller påstander om fysisk styring.

## 14. november 2026 – Codex: utredning av ekte styring og én aktivering

På brukerens ønske er Palit RTX 2080-styring undersøkt som forarbeid til neste oppdatering. Tre Sol-agenter gjennomgikk backend/gjenoppretting, UI/protokoll og primærkilder for Palit/OpenRGB. `utredning-styring-2026-11-14.md` samler funn, filkart, én aktivering uten fire godkjenninger og nødvendige prøver. Den gamle 1.4.0-planen peker nå til tillegget; gamle bastante støtte-/RGB-påstander er ikke behandlet som lokal bekreftelse.

Kun lesing mot driveren: `nvidia-smi` bekrefter kort, PCI/subsystem, VBIOS, driver og effekt 105–275 W med standard/gjeldende 245 W. `verktoy/nvml_probe.py --json` bekrefter to vifter, rapportert område 35–100 %, automatiske vifter, offsetgrenser og temperaturmål. Sandbox-spørringen feilet; samme lesing utenfor sandkassen lyktes. Ingen driverendring var nødvendig. Get-resultater beviser ikke at set-kall fungerer. Ingen NVML-set-kall, I2C-åpning, tjenesteinstallasjon, systemfilendring eller endring av brukerens prosesser.

Viktige kodeblokkere: skrivekortet er hardkodet indeks 0, grenseavlesning er alt-eller-ingenting, transaksjoner bruker fortsatt faste grenser, faktisk tilstand mangler tilbakelesing, reset-/viftefeil kan skjules og livssignalets `stock`-kontrakt passer ikke ekte snapshot. RGB-backend mangler. Ingen kode er aktivert eller endret i denne runden; ekte-skriving-sperrene er intakte. Versjonen forblir 1.3.2 · bygg 21. Siste clippy/test-resultat er uendret fra kodeleveransen over; ikke kjørt på nytt for dokumentasjon.

## 14. november 2026 – Codex: spesifikasjonsrevisjon og 1.3.3 · bygg 22

Brukeren ba om kontroll av om komponentene viser så mange spesifikasjoner som mulig, og la til CPU-lanseringsdato. Tre Sol-agenter kartla GPU, CPU/RAM/hovedkort og disker/nett/skjerm. Samlet vurdering, skjulte innsamlede felt, korrekthetsfunn og prioriteringer står i `spesifikasjoner-revisjon-2026-11-14.md`. GPU/CPU/disk har god dekning; RAM-moduler, skjerm og nettverksdetaljer mangler mest. Disse utvidelsene og eksisterende feil i cache/merker/topologi er dokumentert, ikke implementert i denne endringen.

Ny `rigg-core/src/cpu_specs.rs` har eksakt modell-/vendoroppslag for Ryzen 7 2700X og kilden til 19. april 2018 fra AMD. Felles `report/system.rs` viser Lansert-raden på Prosessor og kilde i eksportert rapport; ingen CPUID-/kodenavnfallback eller nettsøk ved kjøring. Ukjente modeller får ingen antatt dato. En separat lesende Sol-review fant ingen konkrete feil.

Kontroll: clippy `--workspace --all-targets -- -D warnings` ren. `cargo test --workspace --no-fail-fast`: 350 bestått, 4 ignorert, 0 feilet. To lookup-tester og én rapporttest dekker fullt/kort normalisert modellnavn, 2700/2700U/PRO/XT, feil leverandør og dato/kilde i begge rapportformater. Alle tre lokalt bygde debug-binærene viser 1.3.3 · bygg 22. Versjon, Cargo-lås, README, STATUS, PATCHLOG og standard bildemappe er oppdatert.

Eksisterende `6a`-plan ga 16 skjermbilder i en privat GNOME-økt. Prosessorsiden er visuelt kontrollert i lyst/mørkt ved 1920 og 640 px; datoen vises uten avkutting. Bildene ligger i `docs/skjermbilder/1.3.3/spesifikasjoner/`. Økta er avsluttet. Skjermbildevisningen på 127.0.0.1:4320 er oppdatert og åpnet som leveranse i nettleseren; tidligere galleri er beholdt som `index-1.3.2.html`. Ingen ny full UI-klikkrunde eller ytelsesmåling.

Lokal lesing av `/proc/cpuinfo`, nettets driver/MTU/status og DRM-kontaktstatus/modusliste underbygger funnene. SPD-/SMBIOS-tilgjengelighet er bare kontrollert som filmetadata. Ingen EEPROM/DMI-tabell/I2C-åpning, GPU-skriving, systemendring eller installasjon. Brukerens prosesser og installerte app er urørt.

## 14. november 2026 – Codex: installert 1.3.3 · bygg 22 på brukerens PC

Brukeren ba uttrykkelig om å oppdatere den installerte Agentic Rigg med alt som er ferdig. Gammel app og tjeneste viste 1.3.0 · bygg 19. Produksjonsbygget er laget med `CARGO_BUILD_JOBS=6 CARGO_TARGET_DIR=~/.cache/rigg-build cargo build --release --locked --workspace`; ingen kildekode er endret i installasjonsrunden. Siste kodekontroll før installasjon var ren workspace-clippy og 350 beståtte tester, 4 ignorerte.

Sikkerhetskopi av binærer, appmenyoppføring, brukertjeneste, helper-filer, dokumentasjon og innstillinger ble tatt til `~/.cache/rigg-build/agentic-rigg-for-1.3.3-4rq_yzzr/`. Appen ble avsluttet med SIGTERM etter kontroll av PID og kjørbar fil. Sensortjenesten ble stoppet normalt med `systemctl --user stop`, og historikken ble også kopiert etter at tjenesten hadde lagret. CSV-logging var ikke aktiv. Dette er en målrettet omstart autorisert av oppdateringsønsket; andre brukerprosesser er ikke rørt.

`app/install.sh --bin-dir ~/.cache/rigg-build/release` installerte for brukeren under `~/.local`. Hjelperfilene er bare lagt klare; ingen root-tjeneste eller polkit-aktivering ble utført. Sensortjenesten ble startet igjen, og den oppdaterte appen åpnet på Prosessor.

Kontroll: alle tre installerte binærer viser 1.3.3 · bygg 22 og har samme SHA-256 som release-kildene. De eksisterende innstillingsfilenes SHA-256 er uendret under installasjon. Kjørende tjeneste annonserer 1.3.3/22, `backend=nvml`, ikke mock, finner `0000:1c:00.0` og gir telemetri og korrekt CPU-modell. `capabilities.write.state=locked`. CPU-historikk med gyldige datapunkter fra før ny tjenestestart er lest tilbake. Tjenesten er aktiv/running, og UnitFileState er fortsatt disabled. Appens oppstartslogg er tom og journalen har normal start uten nye feil. STATUS er oppdatert; ingen ny versjon utover bygg 22 eller nye ytelsespåstander.

## 14. november 2026 – Codex: GitHub-kilde og første nedlastbare Release

Brukeren ba om oppdatering av tilkoblet `bejulian1/agentic-rigg`, Releases, versjonslogg, kreditering av Claude/Codex og to–tre skjermbilder. Repoets private synlighet er beholdt. `gh` bruker eksisterende kontoautorisering; ingen ny tilgang, pakke-registry, systeminstallasjon eller maskinvareendring.

Publisert `main`-commit `5069b84b16f35364cc1c174c508754c68a20ae4b` og stabil Latest Release `v1.3.3`, begge 1.3.3 · bygg 22. Release: https://github.com/bejulian1/agentic-rigg/releases/tag/v1.3.3. Tittelen er «Agentic Rigg 1.3.3 · bygg 22». Taggen peker på riktig kildecommit. Ingen eksisterende historikk er skrevet om.

README er oppdatert med tre klikkbare bilder i én rad (Oversikt, Prosessor, Disker), presise støttekrav og installasjon fra ferdigbygd pakke. Root-CHANGELOG, CONTRIBUTORS, MIT-LICENSE og versjonerte utgivelsesnotater er lagt til. Eksisterende feilaktige løfter om aktiv styring og ukvalifiserte ytelsespåstander i README er erstattet med faktisk status. App-README skiller målte data fra RGB-forhåndsvisning og simuleringsprøvd vern. Ekte GPU-/RGB-skriving forblir sperret.

Claude og Codex er dokumentert som AI-assistanse med faktisk rollefordeling og medforfattertrailere som peker på verifiserte offisielle botkontoer. Eksisterende Claude-kreditering er beholdt. GitHubs Contributors-API viser etter publisering bare bejulian1; den automatiske boksen er ikke manuelt redigerbar. Det er ikke laget kunstige commits for statistikk.

Bildene i `docs/images/` er uendrede PNG-er fra 1.3.3. Prosessor/Disker er fra 6A; ny Oversikt er fra privat 8A-økt med cairo og 70 s oppvarming, slik at minuttgrafene er fylt. Alle tre er 1920 × 1048 og visuelt kontrollert uten personidentifikatorer. GPU-/diskdata er simulerte og merket som det. De private øktene er avsluttet. Bare disse tre publiseringsbildene er lastet opp, ikke rå bildeserier.

Ny `app/scripts/pakk-utgivelse.py` validerer versjon og ELF-arkitektur og pakker eksisterende release-binærer med installer/avinstaller, data, dokumentasjon og lisenser. Arkivet har 108 filer og ingen tester, fixtures eller logger. Filen `agentic-rigg-1.3.3-build22-ubuntu26.04-amd64.tar.gz` er 3 855 095 byte. SHA-256: `c684a80c89a69ea48ce8ca9db3f7058694cd7fee42a67bb477ed2650afe15fec`. Pakke og `SHA256SUMS` er vedlagt utgivelsen. Målstøtte: Ubuntu 26.04 amd64, GTK ≥ 4.22, glibc ≥ 2.39; systembiblioteker følger ikke med. Packages-feltet er ikke fylt med en kunstig container: GitHub Packages støtter ikke dette desktop-formatet, så pakken ligger under Releases.

Kontroll før publisering: arkivets sjekksum, versjon/bygg, kildecommit og filutvalg; isolert installasjon under en midlertidig brukerrot med systemd slått av; alle tre installerte prøvebinærer og nødvendige ikoner, appmenyoppføring og helper-filer. Deretter lastet Release-vedleggene ned fra GitHub og kontrollerte de samme sjekksummene/metadataene før utgivelsen ble publisert. GitHubs renderte README er kontrollert for tre bilder og dokumentlenker, og repoets README-blob er identisk med den lokale fila. Ingen nye Rust-kodeendringer etter de 350 beståtte testene/clippy-kontrollen.

35 eksplisitt valgte filer ble publisert. Eksisterende små kildekodetester ble beholdt/oppdatert for å holde repoet kontrollerbart; nye testresultater og store testdata ble ikke lastet opp. Brukeren fikk et valgfritt spørsmål om strengere testfiltrering og hadde ikke svart før publisering; anbefalt valg og avgrensning ble opplyst underveis. `System meldinger/` og utgivelsesutdata er lagt i gitignore. Denne lokale LOGG-oppføringen, installasjonsbackup og private samtaler er ikke publisert.

## 14. november 2026 – Codex: stående ønske om GitHub-utgivelser

Brukeren ba om at alle nye appversjoner fremover committes til GitHub og får versjonsendring også på Releases. Dette er lagret i rotens `AGENTS.md` som fast rutine med versjon/bygg, endringslogg, kontroller, kildecommit, nedlastingspakke/sjekksum og publisert Release. Instruksen viderefører avgrensning av lokale data, sannferdig kreditering, privat synlighet og eksisterende maskinvaresperrer. PC-installasjon skjer ved bestilling.

Dokumentasjonscommit `a4bdc86cfa0c02fefab9b0dd5cacc625b695ba29` er pushet til main. GitHubs AGENTS-blob samsvarer med den lokale fila. Ingen appkode, versjon eller utgivelsesartefakt er endret; 1.3.3 · bygg 22 er fortsatt gjeldende apputgave. Kun dokument-/lenke- og diffkontroll var nødvendig.

### Codex sluttkontroll · 1.4.0 bygg 23 · 14. november 2026

- 388 workspace-tester bestått, 4 ignorert, ingen feil. Clippy på alle mål er ren. Privat 1.4.0-ui-plan bestod etter siste statusretting; fysisk Wayland-input er ikke prøvd i denne runden.
- Installert produksjonsbygg; backup under `/home/julian/.cache/rigg-build/agentic-rigg-for-1.4.0-_bx745pw`. Innstillinger uendret ved installasjon, historikk sikkerhetskopiert. Alle installerte binærer, også root-hjelperen, har samme SHA-256 som release-bygget.
- Brukeren fullførte polkit. Den herdede NVML-hjelperen er klar for PCI 0000:1c:00.0 og riktig UUID. Fysiske prøver: effekt245→240W, begge vifter Auto35→Manuell50%, core0→−15MHz, minne0→−10MHz, flat viftekurve50% i4s og temperaturmål83→82°C. Ferske NVML-gettere, revert og reset bekreftet245W/0/0/83°C/Auto på begge vifter.
- Ekte daemonkjede: Bruk240W→Behold→reset245W; ny240W utenBehold ga rolled_back/timeout etter10s, fersk245W før ekstrareset. Private rapporter ligger kun i /tmp. Ingen positivOC, stress, I2C/RGB, driver-/strømbruddsprøver.
- Rettet automatisk UI-statusoppdagelse også før porten er klar og --innstillinger på eksisterende forekomst. Appvinduet er oppdatert og åpnet på Klokker og effekt.
- Oppdaget kontrakttestenes uisolerte standardhjelper etter aktivering; alle aktive Cargo-harnesser og private standardverktøy bruker nå egen rot/config/socket/baseline. Testene passerer med brukerens virkelige hjelper fortsatt aktiv.

- Publisert privat GitHub Latest v1.4.0 på kildecommit eb9fb0d3d57739eaf67a1efccd3a5a2e2d7969f9. Pakke108filer/3966934byte, SHA-256 5f3156446c2725d4e8fecd85f6fd5195aaface9d622074ecefcad8f0772f4982. Isolert installasjon, kilde-ID og nedlastede vedlegg er verifisert. Publisert URL: https://github.com/bejulian1/agentic-rigg/releases/tag/v1.4.0 . Kildecommit krediterer Codex-kontoens verifiserte noreply-adresse. Bare lokal LOGG er ucommittet.

## 2026-11-14 · CPU-ID-er og faktisk cachetopologi (Codex)

- CPU-leseren følger faktiske Linux-ID-er i `/proc/stat`. Lastdeltaer holdes per ID, frekvensfilene åpnes for samme CPU, og en ny CPU arver ikke en frakoblet CPU sin teller. Telemetrivektorene beholder indeks = Linux-ID; hull er ukjente verdier. Nye/frakoblede ID-er håndteres uten å flytte historikknøkler som `cpu:8`.
- Faste CPU-data får `threadIds`, sorterte aktive ID-er fra sysfs, `/proc/cpuinfo` eller `/proc/stat`. `threads` teller ID-ene, mens `coreOf` og `packageOf` beholder faktiske CPU-indekser. UI-et grupperer på pakke + rå kjerne-ID, også når L3-informasjon mangler eller er ufullstendig. Ukjent pakke blir ikke en oppdiktet pakke 0. Den minimale `get_system`-adapteren teller ikke lenger vektorhull som tråder. Nominelt cpufreq-område hentes fra de aktive CPU-ene; CPU0 er ikke nødvendig.
- Cachedata leses fra de aktive CPU-ene og dedupliseres etter nivå, type, cache-ID og delt CPU-liste. Ulike størrelser på samme nivå får egne rader. Antall og total regnes bare når cacheidentiteten kan bestemmes; ellers beholdes målt størrelse med ukjent antall/total. Rapporten viser da «–», ikke en oppdiktet enkeltinstans.
- Åtte nye syntetiske regresjonstester dekker `0-3,8-11`, hotplug med uendret trådantall, hybridcache og ulike indexN-navn, manglende CPU0, manglende cacheidentitet, fallback til faktiske `/proc/stat`-ID-er og to sokler med samme kjerne-ID. Flerpakkeprøven dekker både manglende og ufullstendige L3-grupper og ukjent pakke. Testene bruker unike private midlertidige mapper og ingen vertshjelper eller GPU. Rustfmt og diff-sjekk er utført; Cargo og samlet validering kjøres av hovedøkta.
- GPU-skriver, hjelperens sikkerhetsmotor og minimumskravet til autorisert hjelper er ikke endret av dette bidraget. Ingen maskinvarekall, bygg eller installasjon ble utført av denne agenten.

## 14. november 2026 · 1.4.1 sluttkontroll og lokal oppdatering

- 410 bestått, 4 ignorert, ingen feil; ren workspace-clippy. Native 1.4.0-ui-plan fullført med versjonslogg begge innganger og sortering, sticky Behold ved faktisk rulling, Manual55→Behold→71s, None/0/1/3vifter og faktisk640px etter dobbelresize. Systemplan6a fullført lys/mørk1920/640. Egen kodegjennomgang: identitetstest tilpasset felles helper, Fahrenheit readonly utenforområde rettet.
- Installert1.4.1b24, binærhash samsvarer og config uendret før restart. Backup: /home/julian/.cache/rigg-build/agentic-rigg-for-1.4.1-r7dhd0sv. Roothelper23uendret, daemon24/helper23Onreadykortbindingbekreftet, stocktrue, ingenHWfeil. Login fortsattdisabled. Ingen nye setters/I2C/RGB, ingen nye fysiske GPU-prøver.

- Publisert og kontrollert Latest: https://github.com/bejulian1/agentic-rigg/releases/tag/v1.4.1 · commit e269aceb14f2a3a605e29075cacef4670ab6576e. Pakke109filer uten tester, 4078383byte, SHA256 3e01c6f27ab7a0f949439b6f6b87d80e638feee0ff8274e925a1593365f60001. Isolert installasjon og ny nedlasting av draftvedlegg verifisert før publisering. Tagg/Latest/commit og privat repo bekreftet etter publisering. Kun lokalLOGG er ucommittet.

## 17. november 2026 – Codex: bli kjent med prosjektet

Lest AGENTS.md, produksjonsreglene, README-/statusdokumentasjon, arkitektur- og sikkerhetsoversikt samt sentrale innganger i de fem Rust-cratene. Sett på demoens struktur, utviklingsisolering, utgivelsesrutinen og README-bildet av Vifter. Lokal kildeversjon er 1.4.1 · bygg 24. Eksisterende loggendringer er bevart. Ingen appkode, innstillinger, tjenester eller maskinvare er endret; ingen bygg, tester, installasjon eller publisering er utført i denne orienteringsrunden.

## 17. november 2026 – Codex: Oppgaver på feature/task-manager

På brukerens bestilling ble `origin/main` hentet og `feature/task-manager` opprettet på e269aceb14f2a3a605e29075cacef4670ab6576e. Main beholdes urørt. Tre GPT 6.1 Sol-agenter med high arbeidet med henholdsvis Linux-research/kjerne/innsamling, daemon/IPC og GTK; hovedøkta integrerte, gjennomgikk, målte og kontrollerte. Eksisterende lokale loggendringer, brukerinnstillinger, historikk, installert app og hjelper er bevart.

Prosesslesing bruker eksisterende prøvetråd, 2 s behovsstyrt innsamling og 5 s behovsfrist. PID/starttid, monoton CPU-rate, UID/brukernavn, RSS, søk/sortering/iterativt tre og avgrensede sider. GTK viser maksimalt 100 gjenbrukte rader, delte systemminigrafer og bekreftet normal avslutning. SIGTERM er pidfd-bundet; nye identitets-/rettighetskontroller og vern av system-/øktprosesser, appen, klienter og deres foreldre. GPU-prosent er ikke implementert; VRAM gjenbruker NVML-listene og forkaster delvise/ukjente bidrag. Ingen GPU-/viftekontrollendring.

Samlet kontroll: ren workspace-clippy på alle mål; 424 tester bestått, 4 ignorert, 0 feil. Produksjonsbygg laget i felles cache med seks jobber; alle tre binærene oppgir uendret 1.4.1 bygg 24 fordi dette er et ikke utgitt PR-forslag. Privat GTK-plan bestod i lyst/mørkt og 640 px, med søk/sortering/tre/pause/F5/sideskifte og Avbryt/bekreftet SIGTERM til runnerens eget sleep-barn. Én PNG valgt til kildeforslaget; råbilder og målelogger er lokale. Målt svartid ca. 4,8 ms for 100 rader og ca. 2 MiB ekstra daemon-RSS med ~760 prosesser. Egen tom hjelperrot og separate mapper også i ekte, lesende NVML-prøve; faktisk VRAM levert uten GPU-settere. Kilder, definisjoner, måletall og avgrensninger står i oppgavebehandling.md. Minnevakt holder margin til 4 GiB; observerte minima var over 19 GiB. Ingen installasjon, systemendring eller Release i denne runden. Featurebranch og Draft PR publiseres for gjennomgang, uten merge.

Publisert featurecommit `13d2f5842df1719ca40315f66aae839afd2ab440` til `origin/feature/task-manager`, med 33 eksplisitt valgte filer og korrekt Codex-medforfatter. Draft PR #1: https://github.com/bejulian1/agentic-rigg/pull/1. PR-en er åpen, draft og ikke merget, med main/base e269aceb14f2a3a605e29075cacef4670ab6576e og riktig feature-head. Den eksakte commiten fikk ren clippy, 424 beståtte tester/4 ignorerte/0 feil og produksjonsbygg av alle tre binærer. Endelig testharness kontrollerer Ubuntus kanoniske sleep-binær, kommandonavn og felles forelder før egen signalprøve; privat GTK-økts 12 bilder og SIGTERM til eget barn bestod. Lokal og remote main er uendret. Kun lokal LOGG er fortsatt ucommittet; ingen Release, merge eller installasjon.

## 17. november 2026 – Codex/Astra: RGB-arbeid på separat branch

Aktivt langtidsmål: faktisk RGB på brukerens bekreftede Palit GeForce RTX 2080 Super JetStream. Opprettet feature/palit-rgb fra oppdatert origin/main e269ace, uten å blande inn Oppgaver-PR eller endre main. Tre GPT 6 Astra-agenter med xhigh undersøkte protokoll/captures, Linux-transport og appens sikkerhets-/tilstandsmodell. Gjenvant ThunderMaster-captures fra #1420/#1938/#1898 og testdiskusjoner; både PALIT100/150 er kildebelagte, men eget kort er ikke identifisert bare fra PCI-navn.

Implementert typet RGB-observasjon/baseline med ukjent effektmodus bevart, featurestyrt lesende Palit-transport med streng PCI/UUID/adapterbinding, og separat --rgb-identifiser-CLI før vanlig helperstart. Ingen registerskriving, busskanning eller automatisk utvidelse av gammel GPU-autorisasjon. Rettet testreglene snevert for denne eksplisitte lesestien. Clippy ren; 432 tester bestått, 4 ignorert, 0 feil, alle nye fixturer isolert også ved root. Felles cache/seks jobber/minnevakt; observerte tilgjengelige minnemengder over19GiB. Native NVAPI-gettere prøvd separat uten settere: ett COLOR_FIXED-felt, manuell/100%, ingen RGB-kanaler. Metadata bekrefter GPU-port1/i2c0, ccgx på .3/i2c7 og SPD påSMBus/i2c8; bare navn/stier lest. Gjennomgått polkit-probe for én valgt GPU er startet; videre fysisk fargekontroll og appintegrasjon er fortsatt mål, ikke erklært ferdig. Rapport/kilder i palit-rgb-2026-11-17.md. Eksisterende brukerinnstillinger, historikk og helper23 er bevart.

Første pågående RGB-fase er committet/pushet som `67f584ffbb4e7b84782734d0367f4082ba7b40fd` på feature/palit-rgb, 15 eksplisitt valgte filer. Draft PR #2: https://github.com/bejulian1/agentic-rigg/pull/2, åpen/draft/ikke merget, base main e269ace. PR-en lister faktisk fargekontroll, gjenoppretting, brukerflate og fysisk bekreftelse som gjenstående; den erstatter ikke det aktive RGB-målet med bare identifikasjon. Produksjonsbygg ferdig, alle tre binærer viser uendret 1.4.1 bygg24. Lokal LOGG/captures/diagnostikk er holdt utenfor.

Administratorprøven er fortsatt levende og venter (exec-session63865, hostpkexecPID1373681). Ikke start ny prøve eller overskriv debughelperen mens denne venter. Kommandoen er én lesende --rgb-identifiser for valgt PCI0000:1c:00.0 og faktiskGPUUUID. Debugbinary SHA256 a40be5e02eb567426a9ac4fb2f6d047da62632780a9a7c39a9353360d7d1ae07. Utdata: /tmp/rigg-rgb-palit-identification.json, stderr samme stamnavn.stderr; begge tomme ved siste kontroll. Ingen lesende I2C-resultat eller RGB-skriving er dermed bekreftet ennå. Aktivt mål forblir uferdig; brukerens opplysning om nåværende lys/effect er også etterspurt for trygg første fargeprøve. Ytterligere Astra-research kartlegger AMD/Intel og RAM/skjerm/nett uten kode- eller maskinvareendringer.

Videre forskningsnotater for AMD/Intel og RAM/skjerm/nett er gjennomgått og pushet som dokumentasjonscommit `ce3b2f2df9acc986f5a1eb96809e1e90892152be`, uten appkodeendring eller nye maskinvareprøver. DraftPR2 bekrefter riktig featurehead, mainbase e269ace, draft=true og merged=false;17endredefiler. De foreslåtte viderebackendene er ikke implementert. Polkit-prøve63865 er fortsatt levende og begge utdatafiler tomme; synlighet av administratorforespørselen er etterspurt i tillegg til dagens lys/effect. Dette er en verifisert venting på eksisterende prosess, ikke grunnlag for ny prøve eller erklært måloppnåelse.

Brukeren opplyste senere at han ikke er hjemme og ikke kan se lyset. Etter kontroll av egen ventende pkexec (PID1373681, fortsatt pkexec) ble bare denne prosessen avsluttet med TERM; session63865 endte med143. Ingen I2C-rapport foreligger. Ikke start ny administratorforespørsel eller fysisk fargeprøve før brukeren er tilgjengelig. Tidligere ventestatus ovenfor er dermed avsluttet. Offlinearbeidet fortsetter med separat RGB-tillatelse, avgrenset typed skriving og domenespesifikk transaksjon mot fakes; faktisk kortsignatur, synlig farge og gjenoppretting er uverifisert.

Hovedøkta har gjennomgått og integrert denne offlinefasen. Separat streng RGB-autorisasjonsmodell, globalGPUv2 ikkeRGB, transport med fem statiske/av-byte og guardedapply/separatrecover, og egen kjernetransaksjon er testet. To uavhengige gjennomganger rettet nullskriveavvisning som ellers kunne utløse recovery, også ved disconnect/shutdown, og feilaktige restored-statusnavn ved uavklart resultat. 14transaksjons-/12nye transport-/3auth-tester; faktisk journal I/O, authfil-leser, worker/socket/heartbeat og UI er uttrykkelig ikke koblet inn. Ingen ubrukt privilegert serverstub lagt til; videre integrasjon avventer fysisk signatur og grunnopptak.

Endelig workspaceclippy er ren. Workspace-testsettet bestod460/4ignorert/0feil; etter siste nye regresjonstest ble hele berørte core kjørt på nytt:145enhets+8integrasjon bestått. Kjøringene overlapper og summeres ikke. Minste observerte MemAvailable18,97GiB. Releasebygg bestod2m41s/min19,30GiB, tre binærer verifisert1.4.1bygg24. Ingen installasjon/nyRelease/fysiskRGBprøve. Hele endelige kodeendringen dokumenteres på DraftPR2 med konkrete gjenstående punkter; main, Oppgaverbranch og eksisterende GPUhelper23 uendret. LOGG beholdes lokal.

Commit `da1d0682788f554ac4cd3ed3fe07cb0aa95c658d` er pushet med10eksplisittefiler og korrekt Codex-medforfatter. DraftPR2 oppdatert og verifisert åpen/draft=true/merged=false, head da1d068, mainbase e269ace,3commits/20endredefiler samlet. KunLOGG er lokalendret. Rootprøve63865 er avsluttet143 og skalikke startespånytt mensbrukerborte. Neste avgjørende bevis er fysisk kortsignatur ogavklartstatiskutgangspunkt før videreprivilegertintegrasjon; målet erikke erklærtoppnådd.

## 17. november 2026 – videre offline RGB-integrasjon

Forrige målturn var reell fremdrift. Ny gjennomgang viste fortsatt meningsfullt arbeid uten fysisk prøve: varig journal, separat RGB-prosess og RPC kan implementeres og testes mot fakes mens brukeren er borte. Hovedøkta og de tre Astra-agentene bygget derfor rgb_store, rgb_backend, rgb_runtime og rgb_service. Ingen administratorforespørsel, installasjon eller fysisk I2C-prøve ble startet.

Separat --rgb-server krever root, egen RGB-tillatelse og eksisterende brukerbekreftet statisk baseline. Oppstart og status åpner ikke I2C. Filene kontrolleres strengt; eksklusiv lås og atomisk, fsync-et journal lagres før setter. Enhver journal ved restart krever avklaring uten auto-restore. NativeBackend har egne apply/recover-veier. Egen protokoll 1, kompatibilitetsminimum 24, faktisk peer-UID, tillatelsesreload og CLOCK_BOOTTIME-lease deler ingen NVML-tilstand. Eksisterende GPU-protokoll 2/minimum 23 er uendret. Status merker lagret baseline, tilbakelesing og ukjent kilde hver for seg.

Uavhengig review rettet nye RPC-er etter stopp og kødata fra lukket klient; begge har målrettede sockettester. Dokumentasjonen presiserer at pending-timeout og leasefrigivelse kan gi to avgrensede forsøk, og at unlink kan være utført før en mappe-fsync-feil. Menneskelig recoverybekreftelse, restartavklaring, aktivering/systemd, daemonklient og UI gjenstår. Disse kan fortsatt utvikles isolert; faktisk signatur, utgangspunkt og synlig fysisk retur krever brukeren hjemme. Målet er ikke oppnådd eller ved et fullstendig arbeidshinder ennå.

Endelig workspace-clippy er ren. Full testkjøring: 499 bestått, 4 ignorert, ingen feil; alle nye lager-/backend-/runtime-/sockettester er isolert også under root. Produksjonsbygg med seks jobber i felles cache bestod på 2 min 39 s; alle tre binærer og hjelperens nye help-oppføring er kontrollert, fortsatt 1.4.1 bygg 24. Minste observerte MemAvailable var 19,45 GiB under tester og 19,01 GiB under bygg. Ingen ny Release eller installasjon.

Commit 73a9c93aa2479cb53b3fda594e9e1f518eed48e7 er pushet med 15 eksplisitt valgte filer og korrekt Codex-medforfatter. Draft PR #2 er oppdatert og kontrollert åpen/draft/ikke merget, fire commits og 28 endrede filer samlet. Main og origin/main er fortsatt e269ace; Oppgaver-grenen er fortsatt 13d2f58. Kun lokal LOGG er ucommittet. Neste arbeid er separat appaktivering, klient/UI og tydelig menneskelig avklaring uten å forfalske avlest modus.


17. november 2026 – Codex/Astra: avgrenset statisk Palit-skrivetransport (ikke fysisk prøvd)

Bygget separat typed apply/recover i den featurestyrte Palit-modulen. Apply krever validert baseline med samme binding og fersk RGB-/lysstyrkeavlesning lik siste bekreftede verdi. Samme åpne fd, inode/enhetsnummer, sysfs/proc/UUID og kjent signatur kontrolleres før første byte. Bare fem gjennomgåtte byteverdier for statisk farge eller av; første skrivefeil stopper sekvensen og regnes som mulig delvis endring selv om første ioctl feiler. Tilbakelesing bekrefter bare RGB/lysstyrke og beholder ukjent effektmodus. Separat recover kan bruke hjelperens validerte journalmål når nåværende farge er ukjent; journalføring, autorisasjon og transaksjonslås er fortsatt kallerens ansvar. Identifikasjons-CLI er ikke endret.

Lagt til isolerte falske transportprøver for registerrekkefølge, alle fem feilposisjoner, drift før skriving, gjenoppretting med ukjent aktuell farge, signatur-/fd-endring, readback-feil og rå lysstyrke uten klipping. Scannerunntaket er begrenset til denne modulen og bevarer lesende identifikasjon. Rustfmt og diffkontroll utført; ingen bygg, Cargo-tester, root-/maskinvarekall, installasjon eller commits fra denne agenten. Hovedøkta kjører samlet verifikasjon.


## 17. november 2026 – Codex/Astra: isolert RGB-transaksjonsmodell

Lagt til `rigg-core::rgb_txn` med separat klient/eier, én ventende prøve, Behold/rollback, 10–60 s frist og injisert monoton tid. Opprinnelig grunnopptak, siste bekreftede appvalg og faktisk avlesning holdes atskilt; ukjent effektmodus fylles aldri inn fra skrivecache. Varig journal kreves før setter, med full binding/grunnopptak og før-/ønsket tilstand. Avlesningssamsvar uten kjent modus beholder dirty og sperrer nye endringer til særskilt avklaring.

Feil før første setter utløser ingen automatisk recovery; mulig delvis skriving prøver recovery én gang. Et eget flagg bevarer ansvar for tidligere egne endringer uten at en avvist skrivehensikt blir første maskinvareskriving ved klienttap/nedstenging. Falske «gjenopprettet»-statuser er fjernet; journalryddefeil og uavklart rollback forblir sperret. 14 isolerte fake-tester er lagt til for eierskap, journalrekkefølge, timeout, klokkefeil, drift mellom lesing/skriving, delvis feil, lifecycle og ærlig modus-/recovery-status. Rustfmt og diff-sjekk bestått; Cargo-kontroller overlates til hovedøkta. Ingen fysisk backend er koblet til modellen; disk/restart, hjelperprotokoll, brukerflate og administratorinstallasjon er ikke implementert her. Ingen bygg, nett-/maskinvaretilgang eller innstillingsendring utført i denne delrunden.


## 17. november 2026 – Codex/Astra: privat RGB-journal og native adapter (offline)

Lagt til separate `rgb_store.rs` og `rgb_backend.rs` i hjelperen. Produksjonsåpning krever root, fast RGB-tillatelse og et særskilt lagret UserConfirmedStatic-grunnopptak; den åpner ingen maskinvare og utfører ingen automatisk capture/recovery. Enhver eksisterende journal krever avklaring. Private mapper/filer kontrolleres med no-follow per stilledd, eier/modus/type/nlink, eksklusiv flock og bounded direkte JSON-parsing. Journalbytte bruker eksklusiv tempfil, fil-fsync, rename i samme mappe og mappe-fsync. Opprinnelig baseline kan ikke erstattes under en transaksjon.

NativeBackend binder fersk Discovery til hele autoriserte kortidentiteten, holder apply og recover atskilt, og journalfører Prepared/Attempted/RejectedBeforeWrite med bevart tidligere skriveansvar. Setterforsøk teller også når første kall feiler. Diskfeil etter setter gir fortsatt mulig fysisk endring og bevarer skrivehensikten; Prepared ved restart beviser aldri urørt kort. Kun full verifisering gjennom Book kan rydde journalen, mens dagens Palit-transport beholder ukjent modus og derfor ikke får slik kvittering. 19 isolerte fil-/fakeadaptertester er lagt til, inkludert rename/fsync-feil og før-/etter-setterfeil. Rustfmt og diffkontroll utført; hovedøkta eier Cargo-verifikasjonen. Ingen bygg, root-/maskinvarekall, administratorinstallasjon eller innstillingsendring fra denne agenten.


## 17. november 2026 – Codex/Astra: separat RGB-klient i daemonen

Lagt til egen avgrenset IPC-tråd for RGB, med kø-/svargrenser, totalfrister og ferdige svar tilbake til GLib. RGB-ruting krever en levende UI-økt med hello, holder egen eier etter Behold og sender separat heartbeat mens økta finnes. Klienttap kansellerer køarbeid og lukker bare RGB-socketen; sene svar kan ikke gjenopprette eierskap. Statuscache er umiddelbar og viser owned/busy per UI-økt. Root-peer, dedikert navn/domene/protokoll/bygg og full kortbinding valideres; private tester har kanonisk 0700-rot uten vertsfallback. Bekreftelse av recovery viderefører brukerflatens eksplisitte expected-mål.

Seks isolerte unit-tester og én ende-til-ende-prøve er skrevet, inkludert blokkert RGB-svar samtidig med vanlig daemon-ping. Rustfmt på nye filer og diffkontroll er gjennomført. Cargo-kontroller overlates til hovedøkta. Ingen maskinvare, administratoraktivering, installasjon eller innstillingsendring er utført av denne agenten.


## 17. november 2026 – Codex/Astra: eksplisitt visuell RGB-avklaring og separat restartflyt

Utvidet RGB-boka og runtime med `rgb_ack_recovery`: egen autorisert forbindelse, eksplisitt `confirmedStatic`, og obligatorisk typed `expected` som kun sammenlignes med hjelperens eget bekreftede mål. Et endret mål eller fristovergang krever ny brukerbekreftelse. Fersk kortbundet lesing må fortsatt gi komplette samsvarende felt uten tidligere recoveryfeil. Utfallet merkes UserConfirmed/userVisual; ReadableFieldsMatch og ukjent Observed.mode beholdes. Bekreftelse av originalen rydder ansvar/lease, mens bekreftelse av forrige appvalg beholder journal/ansvar og åpner videre avgrenset apply. Opprinnelig baseline erstattes aldri.

Den private adapteren lagrer én streng 0600-kvittering med fil- og mappe-fsync før eventuell journalrydding. Root-API for eksplisitt forberedelse lagrer statisk brukerbekreftet original før separat autorisasjon. Vanlig oppstart avviser fremdeles enhver journal. Egne root-CLI-API-er kan enten forsøke én journalført restore av lagret original, eller bare lese og visuelt avklare at originalen allerede er tilbake; ingen automatisk restartskriving eller falsk Verified-status. Samme eksklusive worker-lås gjelder begge. 15 nye isolerte tester dekker lease/CAS, eier, drift/I/O, kvitteringsdurabilitet og restart mot fakes. Rustfmt og diffkontroll bestått; samlet Cargo-verifikasjon eies av hovedøkta. Ingen fysisk tilgang, administratorhandling, installasjon eller prosessendring utført i denne delrunden.


## 17. november 2026 – Codex/Astra: administratorflyt gjennomgått

Gjennomgått separat RGB-CLI, fire administratorwrappere, Python-installasjon, systemd-enhet og polkit-policy. Rettet tre konkrete grenser i Python: produksjon krever bokstavelig `/` (testalias/symlink og utrygge /tmp-foreldre avvises), forkortede argparse-flagg kan ikke omgå duplikatsjekken, og deaktivering holder samme eksklusive worker.lock som Rust-backend gjennom siste journalsjekk og fjerning av autorisasjon. Bare RGB-tjenesten stoppes; journal, originalbaseline og visuell kvittering beholdes ved uavklart tilstand.

Wrapperne bruker isolert Python (-I). Pakken leses ferdig før root-staging/forberedelse, med regulær fil, fd-identitet, endringskontroll og størrelsesgrense; fixture-rapport/autorisasjon bruker avgrenset, symlinkfri lesing og avviser dupliserte JSON-felt/ugyldige tall. Dette er fortsatt eksplisitt administratorgodkjent pakkeinstallasjon, ikke kryptografisk signaturkontroll. Ingen konkrete ytterligere rettinger i Rust-CLI, unit eller policy. Python AST og diffkontroll bestått; hovedøkta eier fixturetester og bygg. Ingen faktisk administratorhandling, systemctl, installasjon eller maskinvaretilgang utført her.

### 17. november 2026 · Codex · RGB fase 4, samlet kontroll

536 workspace-tester bestod, 4 ignorert, 0 feil. Ren workspace-clippy etter finpuss. Produksjonsbygg med alle tre binærer 1.4.1 bygg 24. Tolv faktiske GTK-bilder og egen daemon/fake-flow bestod etter overskriftsretting: edit uten mutasjon, Bruk/Behold, reset, gammel ACK avvist uten endring, aktiv brukerbekreftelse, av/frigivelse; mode forble ukjent. Valgt RGB-bilde oppdaterer README med eksplisitt simulerings-/featuremerking. Ingen fysisk RGB-åpning, administratorforespørsel, installasjon, GPU-hjelperendring eller merge; bruker er borte, fysisk signatur/farge/tilbakeføring venter. Test/bygg brukte seks jobber, privat miljø og minnevakt (minst 19 GiB observert).

### 20. november 2026 · Codex · fysisk lesende RGB-identifikasjon

Brukeren er ved kortet. `--rgb-identifiser` via polkit returnerte `identified_read_only` for RTX 2080 på `0000:1c:00.0`, NVIDIA-port 1, adresse `0x49`, signatur `palit_100`, én `card_light`-sone. Ferske felt var RGB `[69,200,0]` (`#45C800`), rå lysstyrke 100 og `mode: null`. Bare identifikasjon utført. Visuell bekreftelse av statisk utgangspunkt gjenstår før fargeprøven.

20. november: Brukeren bekreftet konstant grønt før skriving. Egen RGB-tjeneste aktivert med polkit for valgt kort. Én prøve gjennom produksjonshjelperen (`txn=1`, 10 s): fersk lesing av blå `[0,0,255]` ved 100 %, deretter timeout og fersk lesing av original `[69,200,0]` ved 100 %. `recovery_needs_confirmation`, modus ukjent. Avventer brukerens observasjon etter prøve før visuell kvittering og deaktivering.

Den 20. november 2026 identifiserte den lesende proben én PALIT 100-sone på NVIDIA-port 1, adresse `0x49`. Brukeren bekreftet fast grønt (`#45C800`, 100 %) før aktivering med polkit. Produksjonshjelperen leste tilbake blått (`#0000FF`, 100 %) etter Bruk og den opprinnelige grønnfargen etter ti sekunders frist. Brukeren så blått og deretter grønt igjen. Hjelperen leste feltene på nytt, lagret bekreftelsen som `userVisual` og ryddet journalen. RGB-testtilgangen ble deaktivert. Prøven gikk gjennom hjelpersocketen. GTK-betjening er prøvd i simulering. Av-modus og endring av lysstyrke på kortet gjenstår.

20. november 2026: Programvaredrevet pusting, regnbue og fargesyklus prøvd i isolert GTK-økt på feature/palit-rgb. 562 tester bestod og fire var ignorert. Fysisk effektprøve gjenstår.

## 20. november 2026 – Claude Code: åpen kildekode, omskrevet første commit og demo ut av sporing

Brukeren ba om å gjøre repoet offentlig som åpen kildekode. Forfatteradressen i første commit var en privat e-postadresse. Etter brukerens valg ble den byttet til GitHubs noreply-adresse med `git filter-branch` på alle brancher og tagger. Trærne er identiske før og etter, og v1.3.3, v1.4.0 og v1.4.1 er flyttet til de nye commitene og force-pushet. Full sikkerhetskopi av historikken ligger lokalt i `~/.cache/agentic-rigg-backup/`. Gamle commit-ID-er i eldre issue-lenker og utgivelsesnotater viser til de opprinnelige commitene.

Localhost-demoen (`web/`, `server/`, `daemon-mock/`, `prototyper/`, `scripts/`, `package.json`) er tatt ut av git-sporingen og lagt i `.gitignore`. Mappene står urørt på disken. REGLER.md og AGENTS.md er oppdatert med språk for offentlige dokumenter, åpen kildekode og at appen ikke skal ha demofunksjoner.

## 20.–21. november 2026 – Claude Code (dirigent): 1.5.0 · bygg 25

Brukeren ba om åpen kildekode, offentlig repo, engelsk README, retningslinjer for bidrag, retting av alle åpne saker og PR-er med Gemini 4 Neon og GPT 6.2 Sol som underagenter, språkstøtte for ti språk og en ferdig app uten demo.

- Åpen kildekode: PR #8 med engelsk README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY og maler. Repoet er offentlig, privat sårbarhetsrapportering er slått på, og localhost-demoen er tatt ut av sporingen.
- Saker og PR-er: #3 (Gemini, gjennomgått av GPT, PR #9), #7 og PR #1 (GPT, gjennomgått av Gemini), og #4–#6 på PR #2 (GPT). Gemini gjennomgikk robustheten. Fable 5.2 gjorde to sikkerhetsgjennomganger, og GPT 6.2 Astra rettet funnene: felles `system-install` med SHA-256-kontroll fra root-eid kopi, faste polkit-innganger uten lagret godkjenning, sandkasse for administrativ I2C, avvisning av dupliserte felt og minimumsbygg 25.
- Språk: grunnmur i `rigg_core::i18n` og fire parallelle oversettelsesgrener, kryssjekket mellom Gemini og GPT. Gemini nådde kvotegrensen tre ganger. Arbeidet ble lagret som WIP og fullført av GPT, og Fable 5.2 tok de siste gjennomgangene. Oppslaget faller nå tilbake til foreldremodulen, så tekst i nestede moduler som `mod imp` blir oversatt.
- Demo fjernet: 2D/3D-lysmodellen, «3D-kortet», lokal lysvalglagring og «nordlys».
- README-bildene er tatt på nytt på engelsk med simulerte data (planene 2c og rgb-flow). Planene 1.4.0-ui og 2c finner knapper etter norsk tekst og må kjøres på bokmål for å klikke seg gjennom flyten.

## 21. november 2026 – Claude Code: 1.5.1 · bygg 26, systemkomponenter uten terminal

Brukeren ønsket at systeminstallasjonen kan startes fra skjermkortstyringen uten terminal. `system_install.rs` har nå et installasjonsvindu: filvalg (forhåndsvalgt fra Nedlastinger), SHA-256 fra utgivelsessiden (godtar `sha256:`-prefikset) og lenke til utgivelsessiden. Appen kjører skriptet fra `system-bootstrap.command` via `pkexec /usr/bin/bash` i stedet for `sudo`; en test låser at appen og terminalkommandoen kjører det samme. `/bin` og `/usr/bin/env` er lenker på Ubuntu 26.04 og stoppet den første versjonen i `trusted()`. Derfor brukes `/usr/bin/bash` direkte med fast `PATH` først i skriptet. Bryteren «Tillat endring av skjermkortet» åpner installasjonen når komponentene mangler og fortsetter med aktiveringen. Hjelperne, polkit-handlingene og minimumsbygget 25 er uendret. Ny privat skjermbildeplan `installer` (lyst/mørkt, bokmål/engelsk) trykker aldri Installer. Claude Fable 5.2 gjorde en uavhengig sikkerhetsgjennomgang.

## 21. november 2026 – Claude Code: 1.5.2 · bygg 27, systemkomponenter med bare passordet

Brukeren syntes det var utydelig å måtte finne og lime inn SHA-256, og ville at installasjonen skulle virke med bare passordet. Ny `system-hent.py` kjøres som root via `pkexec python3 -I -S -c`. Skriptet henter utgivelsen med appens versjon fra GitHub og kontrollerer den mot `digest` fra GitHub-API-et før `system-install` kjøres fra den kontrollerte kopien. Tillitsankeret er dermed GitHub over TLS, samme kilde som summen ellers ble kopiert fra. Nedlastingen og summen ble prøvd uten root mot den publiserte 1.5.1, og summene stemte. Installasjonsvinduet er fjernet. README og personvernteksten sier nå at denne nedlastingen er appens eneste nettkontakt. Claude Fable 5.2 gjorde sikkerhetsgjennomgangen.

## 21. november 2026 – Claude Code: 1.5.3 · bygg 28, én bryter og ett passord

Brukeren ville at bryteren i Skjermkortstyring skal slå full skjermkortstyring av og på med ett passord, som på Windows, og at fargestyringen skal virke uten SHA. `system-hent.py` aktiverer nå NVML eller RGB rett etter vellykket installasjon i samme `pkexec`. Bryteren og «Aktiver kortlys» bruker dette når komponentene mangler eller hjelperen er utdatert, og ellers den vanlige aktiveringen. Den egne installasjonsknappen er fjernet; bare terminalkommandoen står igjen som reserve uten nett. 604 tester bestod, 4 ignorert. Ingen fysisk prøve er gjort.

## 21. november 2026 – Claude Code (dirigent): 1.5.4 · bygg 29, kortet tilbake og effektbytte

Brukeren savnet 2D/3D-kortet på RGB-siden (fjernet som demo i 1.5.0) og meldte at «Bruk» ble grå ved bytte mellom effekter. Gemini 4 Neon gjeninnførte `lys3d/` koblet til den ekte `CardLight` (én sone, én kontrollrad) og laget RGB-protokoll 3 for bytte fra aktiv effekt og effektstart etter beholdt farge. Claude Fable 5.2 gjorde en uavhengig sikkerhetsgjennomgang: ingen fare for baseline eller kappløp, men oppgraderingen fra protokoll 2 var blokkert (installasjonen nekter mens RGB er aktiv og fjerner GPU-tillatelsen), og en lesefeil under bytte etterlot en låst tilstand. Dirigenten rettet begge: `system-hent.py` slår av gammel RGB etter kontrollert arkiv og aktiverer GPU-styringen igjen, `system_install::ready` skiller NVML og RGB, og lesefeil under bytte stopper effekten og frigir. Ingen fysisk prøve er gjort.

## 22. november 2026 – Codex: tray-stopp, RGB-aktivering og systeminstallasjon

Stopp av bakgrunnsopptak bevarer tjenesten mens en hjelper holder verdier. Utdatert RGB-hjelper kan oppdateres ved aktivering, men appen tilbyr ikke installasjon over et nyere systembygg. «Styring» beholdes som innstillingsseksjon. Systeminstallereren avviser nedgradering før inngrep, kontrollerer hele GPU-oversikten før arkivering og avgrenser pakkekilden. Administratorskriptene skiller eldre tillatelser fra feil eier, avviser tvetydige installerere og håndterer ugyldig NVML-JSON. Kontrollene bruker private testmapper; ingen fysisk prøve, versjonsendring eller installasjon.

Etter gjennomgang: GPU-oversikten må ha gyldig UUID for hvert kort, også når driveren oppgir «Unknown». Netthenting sjekker installert bygg før nedlasting og på nytt før RGB-handlinger. Terminalkommandoen avgrenser kopien før sjekksum og utpakking, og RGB-siden viser «Ikke klar» ved nyere systembygg i stedet for å be brukeren trykke «Bruk».

RGB-bekreftelsen gjelder nå bare handlingen dialogen viste. Endres installasjonsbehovet mens dialogen er åpen, avbrytes handlingen og må bekreftes på nytt. Ved nyere systembygg viser også verktøytipset gjeldende status fremfor å be om aktivering.

## 22. november 2026 – Claude Code: NVML-hjelperen, monoton klokke og IPC-herding (fix-PR, ingen utgivelse)

Fire verifiserte funn fra revisjonen er rettet uten versjonsbump. Hjelperens løkke gir motoren `CLOCK_BOOTTIME`-tid i stedet for veggklokka, så takt, livssignal, hvileoppdagelse, Behold-frist og viftekurve ikke fryser eller tilbakestiller ved klokkehopp. Klokka leses på nytt før hver forespørsel og før takten. Fristene håndheves monotont, mens `pending.deadline` på ledningen fortsatt er veggklokke-ms (`nå + remainingMs`). Nødregelen godtar ikke lenger lav manuell vifte som gjenoppretting når 100 % ikke kan skrives, men går til bekreftet Auto eller feiltilstand, og nødregelen meldes ikke som aktiv uten skriving. Feiler gjenopprettingsmerkøren før en skriving kortet venter på (nødregel, frist), går kortet til grunnverdiene med Auto-vifter og meldes som feil. Er ingenting skrevet ennå, røres ikke kortet. En slik ubekreftet merkør fjernes aldri av samme prosess, heller ikke ved stopp. `dirty.json` registrerer antall styrbare vifter, og `--tilbakestill` beholder merkøren når færre vifter gjenfinnes. En eldre merkør uten viftetall beholdes alltid (feil lukket), fordi grunnopptaket ikke lagrer antallet. Socketløkka begrenser accept, byte og linjeskift (også tomme) per runde og lar overskytende linjer vente i ankomstrekkefølge i en kø til neste runde, gir svarene et felles POLLOUT-budsjett per runde, utfører ikke forespørsler etter EOF eller halvlukking (også ved rundegrensen), fjerner bare sin egen socket ved stopp og ved mislykket oppstart, og grunnopptaket bruker tilfeldig midlertidig filnavn. Clippy og alle workspace-tester er grønne. Ingen fysisk prøve er gjort. `MIN_COMPATIBLE_BUILD` bør økes når dette utgis.

## 23. november 2026 – Claude Code: NVML-hjelperen, merkøren og overtemperaturvern i feiltilstand (PR #23, issue #20)

To funn fra siste kontrollrunde er rettet. En avvist eller ubekreftet `dirty.json` slettes aldri lenger av en senere stopp, endret oppsett eller `reset`. Feiler merkøren ved en ny endring eller tilbakestilling, går et kort som er skrevet til straks til grunnverdiene med Auto-vifter og feilstatus, mens et uberørt kort ikke røres. Merkøren kontrolleres også på nytt før den fjernes (SOL4-23-1). Forespørselsporten avviser nye skrivinger når merkøren er ubekreftet, også uten `hardware_error` (SOL5-23-1). For issue #20 leser hjelperen temperaturen også i feiltilstand. Ved 85 °C eller ukjent temperatur forsøker den grunnverdiene med Auto-vifter, med økende mellomrom (5–60 s), til standard er bekreftet. Den åpner ikke for nye endringer og rører ikke et uberørt eller sperret kort. En test i `rigg-core` krever at `MIN_COMPATIBLE_BUILD` heves til 32 når byggnummeret når 32, så eldre hjelpere uten rettingene avvises i utgaven. Protokoll 2 er uendret. Ingen fysisk prøve er gjort.

## 23. november 2026 – Claude Code: RGB-avklaring, systemlås, tillitsanker og testfixturer (issue #17, #18, #19, #21)

Fire funn fra kontrollrunden på 1.5.6 er rettet uten versjonsbump. #17: En uavklart RGB-økt avklares under ett-passords-oppdateringen først etter at brukeren har sett det gjenopprettede lyset. `system-hent.py` sender avklaringsmålet til appen på stdout etter `--restore-original` og `--inspect`, og venter på nøyaktig gjenopprettingsidentiteten på stdin. Appen viser den vanlige avklaringsdialogen; lukkes den, beholdes journalen og ingenting byttes. #18: Tillatelsene leses på nytt etter nedlastingen, og den felles systemlåsen holdes gjennom ny kontroll, installasjon, NVML-gjenaktivering og valgt aktivering. De nyinstallerte skriptene arver låsen som et kontrollert filhåndtak. Tilbakekalles GPU-tilgangen under nedlastingen, stopper alt og tilbakekallingen består. #19: Arkivet må ha én rotmappe og `version.json` med nøyaktig samme navn, versjon og bygg. Pakkeskriptet lager `SYSTEMFILER.sha256`, `install.sh` legger den hos brukeren, og appen krever at nedlastingen har nøyaktig de systemfilene (festet tillitsanker mot en senere utskiftet Release). Signert `SHA256SUMS` med innebygd nøkkel og en egen polkit-handling er dokumentert som gjenstående og krever brukerens beslutning. #21: Socket- og adminfixturer bruker en kort rot under `/tmp` når `TMPDIR` er lang eller ligger et annet sted, og `nvml-admin.py` beholder høyst fem tidligere grunnopptak (det opprinnelige og de fire nyeste) og legger ikke til et opptak som allerede er bevart. Hele suiten består også med en 112 tegn lang `TMPDIR` utenfor `/tmp`. Ingen GPU-/RGB-/I2C-skriving, installasjon eller Release.

## 23. november 2026 – Claude Code: 1.6.0 · bygg 32, sikkerhetsrettinger i hjelperen og systeminstallasjonen

Utgivelsen samler PR #22 (Codex: tray-stopp, RGB-aktivering og installasjonsvern), PR #23 (NVML-hjelperen: monoton klokke, nødregel, merkør, overtemperaturvern i feiltilstand og IPC-herding, issue #20) og PR #24 (RGB-avklaring etter observert lys, felles systemlås, festede systemfiler og testfixturer, issue #17, #18, #21 og delvis #19). `MIN_COMPATIBLE_BUILD` er hevet til 32 fordi rettingene ligger i hjelperen. Protokoll 2 og RGB-minimumet 29 er uendret. Én ny administratoraktivering kreves for GPU-styring. Issue #19 står åpen: signert `SHA256SUMS` og en fast root-eid hentehjelper krever brukerens beslutning. Ingen fysisk prøve er gjort, og ingenting er installert på brukerens maskin.

## 23. november 2026 – Claude Code (Fable 5.2): layoutrevisjon, temperaturakser og Om (issue #26–#31, ingen utgivelse)

Brukeren meldte at 1.6.0 var skjev «over hele applikasjonen»: fargetema-kortene på Innstillinger › Utseende gikk inn i nabospalten, og temperaturgrafene sto på 20–45 °C. 1.6.0 ble utgitt uten at noen så appen. Denne runden er derfor bygd rundt en ny skjermbildeplan, `scripts/bilder.sh --plan revisjon` (`skjermbilder_revisjon.rs`): hver side og hver del av Innstillinger i 640, 1024, 1280 og 1920 px, kompakt/standard/luftig og skrift «Større», mot `--backend sim` i en privat økt, med språket fra `RIGG_BILDER_SPRAK`. Hvert bilde følges av en maskinell kontroll av det levende widgettreet som skriver `funn.txt`: barn som stikker ut av forelderen (`UT`, `KLIPT`), søsken som overlapper (`OVERLAPP`), vindu som ikke lar seg gjøre så smalt som planen ba om (`VINDU`), og korte tekster brutt over flere linjer (`BRUTT`). Kontrollen peker ut hvor man skal se; bildene er fortsatt sett av den som retter.

Grunnårsaken til det meste var at spalteoppsettene (`Masonry`, `Cols`, `Columns`) valgte antall spalter bare ut fra bruddpunkter i bredden. Et panel som krevde mer enn spalten sin, fikk minste bredde likevel og ble tegnet over naboen. Regelen ligger nå ett sted, `widgets/fit.rs`: bruddpunktet gir det høyeste antallet, og antallet går ned til hvert barn får minst sin minste bredde. I tillegg: innstillingsraden er en egen flate (`widgets/setrow.rs`) der kontrollen legger seg under teksten når det blir trangt; fargetema, sidelisten og Prosessor-flisene flyter om i hele rader; sidefeltets seksjonsoverskrift og merke brytes i stedet for å gjøre sidefeltet bredere enn spalten (det var dette som la sidefeltet inn over innholdet med «Oppdater hjelper», tysk og større skrift), og `Fixed` klipper; tabellenes brede tekstkolonne kuttes med «…» og verktøytips; navnekolonnen på Sensorer går foran sparklinja; tallflisene på Vifter og Klokker og effekt står i hele rader; glideren på RGB-siden kan krympe, og merket der står på én linje.

Den siste kontrollen (`BRUTT`) fant en feil ingen hadde meldt: med tetthet «Luftig» ble nesten alle verdiene på Grafikkort, Prosessor og System brutt midt i ordet («GeF-/orce», «J-/a»), i alle bredder og språk. En etikett som kan brytes, i en vannrett rad, blir spurt om bredden for radens høyde; er raden høy nok til to linjer, får den bare bredden til to linjer. `Steady` (`widgets/fixed.rs`) spør uten høyde.

Baseline målt med samme plan før rettingene: 16 bilder per språk der vinduet ble 1065 px når planen ba om 1024 (Prosessor-flisene med større skrift), og vinduer på 641–1019 px når planen ba om 640. Resultatet etter rettingene står i PR-en.

Temperaturaksene har én regel, `math::temp_axis`, som alle temperaturgrafene bruker gjennom `ChartOpts::temp`: fra grafens nedre grense (20 °C) til minst 100 °C (212 °F), uansett «Tilpass»/«Fast». Verdier utenfor utvider aksen som før, og grenselinjene tas med. Det gjelder GPU-, CPU- og disktemperatur på Grafer, Oversikt › Temperaturer, Vifter, Klokker og effekt og grafen under en temperaturrad på Sensorer. Sparklinjene har ingen y-akse og er uendret. Forklaringen under Innstillinger › Grafer og sensorer › Y-akse sier det, på alle språk.

Om › Bidrag: raden «Gemini» heter «Antigravity» (verktøyet), med «Google Gemini 4 Neon» som verdi; `CONTRIBUTORS.md` og README følger samme inndeling.

Ikke prøvd: de øvrige sju språkene i vinduet, høy kontrast, ekte kort og aktivert hjelper (alt er kjørt mot `sim` med egen hjelperrot), og en fysisk skjerm med brukerens egne innstillinger. Kjent rest: Disker i bred visning krever 844–949 px før kompaktmodusen under 1000 px slår inn (vinduet kan gjøres smalere i to steg), og på tysk med skrift «Større» krever diskhodets tallrad 663 px, så vinduet ikke kan bli 640 px på den siden.

## 23. november 2026 – Claude Code: GPU-aktivering med uavklart RGB-økt og ny Oppgaver-side (fix-PR, ingen utgivelse)

Brukeren kunne ikke slå på GPU-styring etter 1.6.0: hjelperen fra bygg 31 skulle oppdateres, men `system-hent.py` stoppet fordi RGB-journalen for samme kort var uavklart. GPU-bryteren lar nå en aktiv eller uavklart RGB-del stå urørt (`system-install --keep-rgb`): bare NVML-delen og systeminstallasjonen byttes, RGB-delens bygg lagres i `rgb-revision`, og appen sier at RGB-økten avklares separat på RGB-lys. Journalen slettes eller bekreftes aldri. Under gjennomgangen viste det seg at `nvml-admin.py` lastet felles kode fra RGB-mappa; den gamle 1.5.6-koden kan ikke arve systemlåsen, så aktiveringen ville feilet. NVML-mappa har nå sin egen kopi. Oppgaver viser privat minne (`RssAnon`) og swap per prosess, grupperer etter navn med utfolding, har RAM-fordeling (prosesser, delt/tmpfs, kjerne, cache, swap), mer informasjon i CPU- og GPU-kortet og et detaljfelt for valgt prosess (kommando, start, CPU-tid, PSS/USS, disk-I/O, enhet). Deaktivert «Avslutt» ser deaktivert ut, og grunnen står synlig. Kontrollert visuelt i private økter med ekte `/proc` (backend `ingen` og simulert kort). Ingen GPU-/RGB-/I2C-skriving, installasjon eller Release.

## 23. november 2026 – Claude Code: Nettverk-siden (issue #33, ingen utgivelse)

Ny side under Maskinvare. Øverst står aktiv tilkobling, ned og opp nå med totalt siden kortene kom opp, og antall TCP-tilkoblinger. Deretter følger Ruting og DNS ved siden av Wi-Fi (SSID, signal i dBm og %, frekvens, bånd og kanal, bitrate, sikkerhet og BSSID). Hvert grensesnitt har en rad som på Disker. Raden folder ut adressene, maskinvaren (driver, PCI-/USB-navn fra `pci.ids`), tellerne for pakker, feil og droppede pakker og en graf for ned og opp. Grafen har tidsvindu i toppfeltet (`windows.nettverk`) og synkronisert markør. Virtuelle grensesnitt og loopback er samlet og sammenfoldet. Nederst vises porter som lytter, med prosessen når den kan leses uten root.

Tjenesten har fått leseoperasjonen `get_network` (`protokoll.md` kap. 14). Lesingen skjer i en kortlivet tråd og bare når siden spør. NetworkManager leses over D-Bus, og uten NetworkManager brukes `resolv.conf` og `/proc/net/wireless`. Ingen eksisterende operasjon eller sensornøkkel er endret. Med sim er nettet simulert med dokumentasjonsadresser. Tester for tolking og beregninger bruker fixturer i `rigg-hw/tests/fixtures/nett`. Skjermbildeplanene `nettverk` (sim) og `nettverk-ekte` (maskinens egne data uten NetworkManager) er laget og sett på i 1920, 1280 og 1024 px på bokmål, engelsk og tysk. Ingen GPU-, RGB- eller I2C-skriving og ingen versjonsendring.

## 23. november 2026 – Claude Code: nytt visningsnavn «Agentic HW Monitor» og svensk

Appen heter nå **Agentic HW Monitor** der brukeren ser navnet: `version.json` (`name`, som mater vindustittelen, sidefeltet, Om, rapportene, systemfeltet, varslene og `--version`), skrivebordsfilen, polkit-meldingene, systemd-beskrivelsene, installasjonsskriptene, pakkens README, språkkatalogene og dokumentasjonen rettet mot brukere. Systemfeltet og varslene henter navnet fra `rigg_core::version::NAME` i stedet for egne literaler. Tekniske identifikatorer er uendret (app-ID, binærnavn, crate-navn, alle stier, socketer og enheter, repoet og arkivnavnet), og standardmappa for logger heter fortsatt `~/Dokumenter/Agentic Rigg/`. Brukeren beholder derfor innstillinger, historikk og logger, og `MIN_COMPATIBLE_BUILD` er ikke rørt: en allerede aktivert hjelper trenger ikke nytt passord. Polkit-teksten og enhetsbeskrivelsene som allerede er installert under `/usr`, viser det gamle navnet til systemkomponentene oppdateres neste gang. `system-hent.py` godtar det gamle navnet i `version.json` bare for utgivelser til og med bygg 32 (`known_name`); senere bygg må oppgi det nye. Se [REGLER.md](REGLER.md#navn).

Svensk (`sv`) er nytt språk nummer elleve: `Language::Sv` med «Svenska» i nedtrekkslista, `sv`-lokaler (også `sv_FI`) velges av «Følg systemet», tall som `12 345,6`, dato `2026-11-07` og `7/11`, og 3053 oversatte oppføringer i 72 kataloger. Omfanget var først ni språk til, men ble endret til bare svensk underveis; ingenting fra de andre språkene ligger i grenen.

Revisjonen er strammet inn for alle språk. `i18n-revisjon.py --check` kontrollerer nå også katalogene (samme språkliste i Rust, uttrekksverktøyet, mappene og skrivebordsfilen, ingen hull, ingen tegn fra et annet språk, mellomrom og linjeskift som kilden, hardt mellomrom mellom tall og enhet, uendret appnavn), og kjøres av `cargo test`. Byggskriptet regner ut flertallsformene fra `Plural-Forms` i katalogene, og en test sammenligner dem med `Language::plural_index` for n = 0–999. Den nye regelen for tall og enhet fant 13 vanlige mellomrom i de eksisterende katalogene (en, de, es, it, pl, pt-BR, zh-CN); de er rettet til U+202F.

Sett i et ekte vindu: planen `sprak` (ny i `bilder.sh`) er kjørt for svensk, tysk og bokmål i isolerte økter med simulerte data, 46 bilder per språk (alle sidene og Innstillinger i 1440 px, tekstrike sider i 1100 og 640 px, mørkt tema). Svensk har de samme avkuttede tekstene som bokmål og ingen egne, etter at turtallsenheten ble `r/min` og én sensoretikett ble kortet ned. Tysk har egne avkuttinger som fantes fra før og hører til layoutrevisjonen: «Protokoll und Benachrichtigungen» i Innstillinger-navigasjonen i alle bredder, «Arbeitsspeicher und VRAM» ved 1100 px, og SMART-attributter og «Schlafend» ved 640 px. Skjermbildesteget prøver nå igjen når vinduet ikke har tegnet ennå; uten det manglet tilfeldige bilder.

Ikke prøvd: polkit-dialogen og systemfeltet på et ekte skrivebord, installasjon på brukerens maskin, og de tre README-bildene (de viser fortsatt «Agentic Rigg» i sidefeltet og bør tas på nytt i utgivelsessteget, etter layoutrevisjonen).

Etter at PR #34 (layoutrevisjonen), #37 (aktivering og Oppgaver) og #38 (Nettverk) var merget, er `main` tatt inn i grenen. De 221 nye og endrede tekstene derfra er oversatt til svensk, så alle elleve språk står på 3266 av 3266. Appnavnet i de nye katalogene (`tasks.po`, `nettverk.po`, `process.po`) er det nye, og hjelpeteksten for `--side` nevner `nettverk` i alle språk.

## 23. november 2026 – Claude Code: README, skjermbilder og bidragsytere før 1.7.0 (ingen utgivelse)

README.md er skrevet om med det nye navnet: hva appen er og for hvem, funksjonstabell med Nettverk, den nye Oppgaver-siden og elleve språk, egne avsnitt for installasjon og oppgradering (hjelpere fra før bygg 32 krever én ny administratoraktivering), en kort sikkerhetsmodell for GPU-styring og en innholdsliste. Påstandene er kontrollert mot koden (verdier utenfor kortets grenser avvises, de klemmes ikke; nødregelen ved 85 °C; nedgraderingsvernet).

Nye README-bilder i engelsk brukerflate, 1440 × 1000 px, fra private økter: `overview`, `overview-dark`, `fans`, `clocks-power`, `rgb-lighting`, `network` og `tasks` i `docs/images/`. Ny plan `readme` i `bilder.sh` (`skjermbilder_readme.rs`) kjører mot `sim:spill` og venter til den simulerte spill-lasten har vart over ett minutt, så Oversikt viser last. `readme-oppgaver` starter `scripts/readme-prosesser.py`, som lager en falsk rot under `~/.cache/rigg-build` (testfixturen `maskin` pluss oppdiktede prosesser, brukere, `/proc/stat` og `/proc/cpuinfo`, med CPU-tider som øker hvert sekund), og tjenesten leser den med `--root`. Maskinens egen prosessliste brukes ikke. RGB-fixturen har fått varianten `pending_effects` (prøve med effektene synlige). PNG-ene er kvantisert med libimagequant (64–86 KiB); fargene på hurtigvalgene er kontrollert mot originalen.

`vifter.png`, `klokker.png` og `rgb.png` er erstattet. `oppgaver-feature.png` viste maskinens egne prosesser og brukernavnet og er fjernet; `oppgavebehandling.md` viser nå `tasks.png`. Bildet finnes fortsatt i git-historikken. `CONTRIBUTORS.md` har fått rollene før 1.7.0. Repo-beskrivelse og topics er oppdatert med `gh repo edit`. Ingen GPU-, RGB- eller I2C-skriving, ingen sudo og ingen versjonsendring.

## 23. november 2026 – Claude Code: funn fra kontrollen før 1.7.0 (B1, B2 og BØR)

B1: Etter GPU-oppdateringen sto RGB-delen igjen på bygg 31 med uavklart journal, og appen kjørte `rgb-aktiver.sh` direkte, som avviste. Nå regnes en RGB-del som er eldre enn systemrevisjonen som «må oppdateres», og direkte aktivering krever RGB-delen fra appens bygg. «Bruk» tar installasjonsveien med ett passord (gjenopprett, én bekreftelse i appen, avklar, installer og aktiver), og `system-install --keep-nvml` lar en aktiv GPU-hjelper fra samme utgivelse gå. RGB-siden viser den uavklarte økten i varselfarge. En kjedet test i en privat rot teller to passord for hele oppgraderingen fra bygg 31 (GPU og RGB), med én bekreftelse i appen. B2: Av-bryteren krever ikke lenger den egne `rgb-admin.py`-kopien for komponenter fra bygg 32. Oppgaver har fast kolonneoppsett med skjuling i stedet for sidelengs rulling, PSS/USS leses utenfor riggds hovedløkke, og minnetallene forklares i detaljfeltet. RGB-lys: glideren har `ctl-scale`, og merkene i panelhodet er ikke lenger strukket. Ny skjermbildeplan `forste-mote` med en falsk GPU-hjelper fra bygg 31. `--keep-rgb` med ufullstendig RGB-del stopper ikke lenger GPU-oppdateringen, og en journal uten `rgb.json` gir en egen melding. Ingen GPU-/RGB-/I2C-skriving, installasjon eller Release.

