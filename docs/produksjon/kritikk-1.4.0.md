# Kodegjennomgang og brukerflatekontroll · 1.4.0, bygg 23

14. november 2026. Gjennomgangen gjelder den nye NVML-skrivestien, hjelperens sikkerhet og styringssidene. Funnene under er rettet i koden og kontrollert med simulerte/injiserte feil. Brukeren har deretter fullført ekte polkit-aktivering, og begrenset fysisk NVML-skriving/revert/reset er verifisert mot hjelperen. 1.4.0 er installert med kontrollert binærhash, uendrede innstillinger og sikret historikk. En etterfølgende prøve gjennom installert NVML-daemon verifiserer også effektstyring, confirm/Behold og tidsfrist mot ekte GPU. Denne oppføringen bekrefter ingen publisering.

## Lukkede reviewfunn

1. **[P1] Gjenoppretting brukte fabrikkantakelser eller et foreldet opptak.** «Standard» bruker nå fanget opprinnelig effekt, temperaturmål og klokkeoffset, ikke antatt 245 W/offset 0. Vanlig restart/recovery bevarer opptaket. En ren ny administratoraktivering krever først bekreftet gammel gjenoppretting og fravær av dirty, og arkiverer deretter det gamle opptaket før nye verdier fanges. Nytt grunnopptak med manuell vifte avvises. Se `app/crates/rigg-hw/src/gpu/skriv_nvml.rs`, `app/crates/rigg-helper/src/main.rs::prepare_baseline` og `app/data/hjelper/aktiver.sh`.

2. **[P1] Akseptert klokkesetting kunne feilaktig regnes som tilbakelest.** Både avvik og getter-feil gir nå `readback_mismatch`, også for negative offset. Skrivecachen beholder hva som kan ha blitt endret, så recovery prøver feltet igjen. `offsetsVerified` og `stock: null` skiller ukjent tilstand fra bekreftet gjenoppretting. Uverifiserte offset og temperaturmål presenteres ikke som kjente aktive HW-verdier. Se `skriv_nvml.rs::offset` og `app/crates/rigg/src/widgets/locked.rs::observed_control`.

3. **[P1] Delvis feil eller uverifisert reset kunne gi falsk suksess.** Recovery skriver alle aktuelle felt uten å stole på den gamle cachen og forsøker alle vifter selv om et forsøk feiler. Dirty lagres før skriving og ryddes først ved bekreftet `stock == true`; Ok alene eller null er utilstrekkelig. Feilet gjenoppretting sperrer videre endring og fjerner ikke hjelperen. Transaksjonshendelser samles etter HW-utfallet, slik at mislykket rollback ikke meldes som vellykket. Se `skriv_nvml.rs::restore_to`, `motor.rs::push_hw`/`restore_stock` og `main.rs`.

4. **[P1] Et forberedt utkast kunne sende gamle standardverdier sammen med ønsket endring.** Utkast lagrer opprinnelig grunnlag og GPU-identitet. Før Bruk flettes bare brukerens endringer inn i hjelperens ferske aktive verdier, og bare differansen sendes. Eldre utkast uten grunnlag beholdes som referanse, men kan ikke sendes automatisk. En effektendring skal dermed ikke nullstille eksisterende klokkeoffset. Se `locked.rs::rebase_draft`, `validate_draft_binding` og `set_draft`.

5. **[P1] Kortbytte under et asynkront kall kunne bruke svaret på feil GPU.** Operasjonen fanger PCI/UUID og opprinnelig kontekst. Et forsinket apply-svar for kort A oppdaterer ikke kort B; en ventende transaksjon forsøkes rullet tilbake mot A. Bekreftelseslinja bruker også opprinnelig identitet og sperrer Behold ved kortbytte. Se `locked.rs::ControlOrigin`, `send_write` og `ConfirmBar::answer`.

6. **[P1] Utløpt nedtelling eller transportfeil kunne skjule en uavklart endring.** Bekreftelseslinja står nå på «venter på bekreftet tilbakestilling» ved null. Den avsluttes først etter transaksjonshendelse eller bekreftet tjenestetilstand. Transportfeil bevarer ventestatus; recovery-feil vises med sperret Behold og mulighet for nytt rollback-forsøk. Se `locked.rs::ConfirmBar::tick`/`poll`/`show_failure`.

7. **[P2] Eksisterende vindu kunne bli stående på gammel side og låst hjelperstatus.** En eksisterende appforekomst håndterer nå `--innstillinger DEL`, og en uavhengig statuspoll oppdager administratoraktivering også før styringen er klar. Watcher-testen skiller meningsfulle autorisasjons-/identitetsendringer fra løpende sensorverdier, så dataoppdateringer ikke bygger hele brukerflaten på nytt. Se `app/crates/rigg/src/app.rs::activate` og `locked.rs::poll_helper`/`watch_helper`.

8. **[P1] Integrasjonstester kunne finne vertens aktiverte GPU-hjelper.** Egne HOME-/socketmapper isolerte ikke helperens standardstier under `/etc` og `/run`. Etter ekte aktivering feilet fire kontrakttester med blant annet `backend_mismatch`, `helper_error` og en uventet liste over skriveoperasjoner. Kontrakttestenes skrivekall manglet UUID, som kreves før NVML-videresending; de observerte feilene dokumenterer ikke et vellykket HW-skrivekall. `kontrakt.rs` og `logg.rs` starter nå alltid med eksplisitt tom `--hjelper-rot` og fjernet arvet miljøvalg; hello kontrollerer at helper er av. Dedikerte helpertester beholder egen falsk rot, og alle helperprosesser får en lokal `--baseline`, slik at vertens dirty/opptak ikke leses. `okt.sh` isolerer også helper-roten for private GTK-/måleøkter og avviser `/`; `maal.sh` setter eksplisitt privat rot. Den ekte hjelperen stoppes eller endres ikke av rettingen. Den etterfølgende fulle workspace-kjøringen bestod med isoleringen på plass.

## Utført kontroll

Siste fullførte workspace-kjøring ga **388 bestått, 4 ignorert, 0 feilet**. `cargo clippy --workspace --all-targets -- -D warnings` er ren. Sluttkjøringen inkluderer CLI-/watcher-rettingene og isolerte testhjelper-røtter. Testene omfatter blant annet baseline-fangst/arkivering, identitet, støtte per felt, delvis skriving, getter-feil, resetfeil, dirty, temperaturbortfall og livssignaler. Den lange kurvesammenligningen gir samme resultat i hjelper og sim-tjeneste når begge får samme ferske temperatur før rundens steg.

Den native GTK-planen for 1.4.0 bestod sine assertions etter at testplanen ble rettet: sensorfilter/fokus, nedtrekksmenyer, én aktivering i testoppsettet, Bruk/Behold, styringsstatus og PCIe i 2D/3D. Interne GTK capture-/pick-tester kontrollerer treff og fokusflyt; de er ikke en fysisk Wayland-pekerprøve.

Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.

## Gjennomført fysisk prøve · 14. november

Den herdede root-hjelperen meldte NVML klar etter at brukeren fullførte polkit-dialogen. En begrenset prøve mot hjelperen på RTX 2080 ga:

| Felt | Endring | Kontroll |
|---|---|---|
| Effektgrense | 245 → 240 W | Fersk NVML-getter og revert |
| Begge vifter | Auto (35 %) → Manuell (50 %) | Ferske gettere og revert |
| Kjerneoffset | 0 → −15 MHz | Fersk NVML-getter og revert |
| Minneoffset | 0 → −10 MHz | Fersk NVML-getter og revert |
| Viftekurve | Flat 50 %-kurve | Begge vifter på målet i 4 s; reset |
| Temperaturmål | 83 → 82 °C | Fersk tilbakelesing, revert og reset |

Avsluttende reset bekreftet 245 W, kjerne-/minneoffset 0/0 MHz, temperaturmål 83 °C og Auto på begge vifter. Alle tilbakelesinger stemte. Temperaturmålet ble først bare lest; den etterfølgende prøven over verifiserte også endring til 82 °C. Etter kurve-/temperaturprøvene var utgangspunktet igjen bekreftet som 245 W, offset 0/0 MHz, temperaturmål 83 °C og Auto på begge vifter. Dette utvider sim-/mockbeviset med faktisk skriving og gjenoppretting i hjelperen.

En etterfølgende prøve gjennom installert `agentic-riggd` 1.4.0 · bygg 23 brukte ekte NVML. `hello`, `get_helper` og `get_state` bekreftet kortbindingen. Apply 240 W + confirm/Behold ga fersk 240 W, og reset ga 245 W. En ny apply 240 W med 10 s frist uten Behold ga en autoritativ `rolled_back`/`timeout`-hendelse; en fersk NVML-getter viste 245 W før noe ekstra reset-kall. Avsluttende reset bekreftet `stock: true`. Dette verifiserer videresending, bekreftelse og tidsstyrt tilbakerulling for effektgrensen gjennom daemonen.

## Gjenværende begrensninger

- RGB-kontrolleren er ikke identifisert. Lys er fortsatt forhåndsvisning; ingen fysisk RGB-/I2C-støtte er bekreftet.
- Daemonprøven dekker effektgrensens transaksjonsflyt; alle GUI-handlinger og vifte-/klokkeflyter gjennom vinduet er ikke verifisert på fysisk GPU. Prøvene dekker bare de oppgitte enkeltverdiene; alle grenser, positiv overklokking og stresstest er ikke kontrollert. Oppdagelse av gettere/setter-symboler alene er fortsatt ikke et skrivebevis.
- Fysisk mus/peker under Wayland er ikke verifisert i denne runden. Den interne GTK-planen erstatter ikke denne kontrollen.
- Ingen nye ytelsesmålinger eller kontroll av strømbrudd/ekte driverfeil er utført. Tidligere åpne funn fra andre komponenter i `kritikk-1.3.2.md` er ikke automatisk lukket av denne gjennomgangen.
