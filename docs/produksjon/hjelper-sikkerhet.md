# Root-hjelperen · sikkerhet og begrensninger

## Gjeldende installasjon på denne featuregrenen

GPU- og RGB-styring er separate tillatelser. Den 20. november 2026 ble fast blått og tilbakeføring til grønt fysisk bekreftet på ett Palit RTX 2080 Super JetStream; testtilgangen ble deretter deaktivert. Den nye installasjonsflyten og de transiente systemd-enhetene er ikke prøvd på fysisk maskinvare. [Systeminstallasjon, bygg fra kilde, arkivering og avinstallasjon](rgb-hjelper.md#betrodd-installasjon-og-administratoraktivering) gjelder **begge** hjelperne. Versjonshistorikken nedenfor beskriver tidligere utgaver, ikke gjeldende installasjonsinstrukser.

- Fra appen (1.5.2) kjøres `system-hent.py` som root via `pkexec python3 -I -S -c`: utgivelsen med appens versjon lastes ned fra GitHub til en ny root-eid `0700`-mappe og kontrolleres mot `digest` fra GitHub-API-et før tar åpnes. Tillitsankeret er GitHub over TLS, det samme stedet summen ellers kopieres fra. Fra 1.5.3 tar skriptet også `nvml|rgb PCI UUID` og kjører det faste, nettopp installerte aktiveringsskriptet med `PKEXEC_UID` etter vellykket installasjon, så installasjon og aktivering er én godkjenning. RGB får `--static-confirmed` bare etter appens visuelle bekreftelse. Terminalkommandoen med arkiv og sum oppgitt for hånd er reserven uten nett. Etter issue #17–#19 kontrolleres også arkivets identitet og, når appen er installert fra et utgivelsesarkiv, de festede summene i `SYSTEMFILER.sha256`; én felles systemlås holdes gjennom installasjon og aktivering, og en gammel RGB-økt avklares først etter at brukeren har sett det gjenopprettede lyset. Se [Ett-klikks installasjon fra appen](rgb-hjelper.md#ett-klikks-installasjon-fra-appen) for den presise tillitsmodellen og hva som ikke er innført.
- Systeminstallereren kopierer og verifiserer pakken før noen kode fra den kjøres. Brukerinstallasjonen fjerner gamle kjente brukerkopier og starter aldri root-kode. Det finnes ingen `--kilde` i den installerte NVML-aktiveringen. Testrot, simulert systemd og UID-overstyring er bare tilgjengelige i kildefixturen med eksplisitt privat rot.
- NVML bruker faste root-eide skript, nye polkit-handlinger uten `keep`, faktisk `PKEXEC_UID` og kortets PCI/UUID. Installereren stopper eldre tjeneste. Hvis `dirty.json` står igjen, prøver den den tidligere installerte binæren én gang til i en transient, tidsavgrenset systemd-sandkasse. Den kontrollerer den lagrede PCI/UUID-bindingen og gir bare tilgang til kontrollnoden og kortets `/dev/nvidiaN`. Også deaktivering bruker denne sandkassen. Manglende fil, avvikende identitet eller fortsatt journal blokkerer utskifting; først etter bekreftet ren tilstand fjernes gammel autorisasjon. **Én ny NVML-aktivering etter systeminstallasjonen er påkrevd.** Valg av kort eller profil kan ikke fornye tillatelsen.
- Gyldige protokoll-2-meldinger er uendret. Dupliserte JSON-felt avvises før dispatch, også i underobjekter. Minimumsbygg for NVML er **32** fra 1.6.0 (sikkerhetsrettingene nedenfor) og uendret i 1.7.0 (bygg 33), der NVML-delen av hjelperen ikke er endret; for RGB fortsatt **29**, men en RGB-hjelper uten `persist`/`hold` (bygg 29–32) regnes som «må oppdateres» etter egenskapene; NVML-hjelpere fra bygg 25–31 er utdatert. Status må i tillegg oppgi `strictRequests: true`. Både første og løpende status bruker samme kontroll; en gammel 24-binær kommer ikke rundt sperren.
- NVML-deaktivering krever riktig UID, ikke et kort som fortsatt finnes i appen. Tjenesten stoppes; uavklarte egne endringer må gjenopprettes for det opprinnelige kortet. Ved permanent fjernet kort kan bare administrator arkivere den stoppede tilstanden, uten å bekrefte fysisk gjenoppretting. Når aktiverende UID er borte, kan root tilbakekalle en stoppet, ren tillatelse med `system-install --revoke nvml` eller `--revoke rgb`. Installert programkode og polkit fjernes separat med `system-install --uninstall`.

## Sikkerhetsrettinger i NVML-hjelperen · 1.6.0, bygg 32

1.6.0 retter sikkerhetsmotoren i NVML-hjelperen (PR #23, issue #20): monoton `CLOCK_BOOTTIME`-tid for frister, livssignal og hvile, nødregel som aldri godtar lav manuell vifte som gjenoppretting, viftebevisst `--tilbakestill`, gjenopprettingsmerkøren som aldri slettes ubekreftet, sperre for nye skrivinger med ubekreftet merkør, overtemperaturvern i feiltilstand og herdet socketløkke. Eldre hjelpere mangler dette, så `MIN_COMPATIBLE_BUILD = 32`; bygg 25–31 vises som utdatert. Protokoll 2 er uendret. **Én ny administratoraktivering kreves:** brukeren oppdaterer systemkomponentene fra Innstillinger › Skjermkortstyring med administratorpassordet, og GPU-styringen aktiveres igjen i samme godkjenning. RGB-minimumet 29 er uendret. Ingen fysisk prøve av rettingene er gjort.

## UI-kompatibilitet · 1.4.1, bygg 24

1.4.1 endrer brukerflaten og lar app/daemon bygg 24 bruke brukerens allerede autoriserte hjelper fra bygg 23. HW-sikkerhetsmotor, baseline, frister og recovery er uendret. `MIN_COMPATIBLE_BUILD = 23` og eksakt protokoll 2 kontrolleres både ved oppdagelse og løpende status. Bare bygg fra minimum til appens eget bygg godtas; ukjent nyere hjelper, eldre bygg, feil protokoll og ugyldige metadata avvises. Kortbinding, uid og faktisk klarstatus kreves fortsatt.

Ved sikkerhetsrettinger som eldre hjelpere ikke må få omgå, skal minimumsbygget økes; inkompatibel protokoll eller endrede sikkerhetsforutsetninger krever ved behov protokolløkning. Dette følger nå av AGENTS.md. Denne snevre kompatibiliteten gjelder ikke automatisk alle senere oppdateringer. Kompatibilitetskontrollen inngår i en full workspace-kjøring med 396 bestått, 4 ignorert og 0 feil samt ren clippy. En lesende kontroll med daemon 24 og eksisterende root-hjelper 23 bekreftet On/ready uten HW-skriving eller ny aktivering. 1.4.1 er ikke bekreftet installert/publisert. Detaljer står i `kritikk-1.4.1.md`; maskinvareprøvene nedenfor gjelder 1.4.0.

**Presisering om viftestopp i 1.4.1:** Det rapporterte manuelle området 35–100 % avgjør ikke om automatisk styring kan stoppe viftene. [NVIDIAs beskrivelse av `nvmlDeviceGetMinMaxFanSpeed`](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html) gjelder hva brukeren kan sette. Automatisk Zero RPM er ubekreftet på dette kortet. Eldre historikk nedenfor som tilskriver 35 %-grensen en fysisk VBIOS-begrensning er derfor for bastant og skal ikke brukes som gjeldende konklusjon. Teksten er korrigert uten backend-/grenseendring, nye skrivekall eller 0 %-prøve.

## Historisk modell · 1.4.0, bygg 23 · 14. november 2026

Dette avsnittet erstatter den tidligere modellens fire godkjenningstrinn og absolutte NVML-skrivelås. Historikken fra 1.2.0 er bevart lenger ned. Brukeren har autorisert implementasjon av kortbundet GPU-styring gjennom én administratoraktivering. RGB/I2C, CPU, hovedkortvifter, systempakker og kjernemoduler inngår ikke i denne styringen.

**Kontrollstatus:** siste fullførte workspace-kjøring har 388 bestått, 4 ignorert og 0 feilet; workspace-clippy på alle mål er ren. Native GTK-assertions bestod. Sluttkjøringen inkluderer CLI-/statusrettingene og isolerte testhjelper-røtter. Brukeren har fullført ekte polkit-aktivering; begrenset fysisk skriving, fersk tilbakelesing, revert og reset er verifisert mot den herdede NVML-hjelperen på RTX 2080. En etterfølgende prøve gjennom installert NVML-daemon verifiserer også effektgrensens confirm/Behold og 10 s-frist mot ekte GPU. Simulering og feilinjeksjon gir ikke bevis for at driveren tillater et bestemt kall. Det tidligere systemd-sikkerhetstallet, binærstørrelsen og ytelsestallene nedenfor gjelder 1.2.0 og er ikke nye målinger av 1.4.0.

### Autorisasjon, identitet og funksjonsstøtte

- `install.sh` installerer under brukerens hjemmemappe og aktiverer ingen GPU-styring. Bryteren i Innstillinger › Skjermkortstyring starter `pkexec aktiver.sh --pci PCI --uuid UUID`. Administratorpassordet skrives i polkit-dialogen.
- Root-eid `helper.json` v2 inneholder uid, `enabled` og én PCI/UUID-binding. Eldre v1-trinn oppgraderes ikke automatisk til global tillatelse. `trinn.sh` gir ingen tillatelser; det er beholdt som en avvisende kompatibilitetskommando. Polkit har handlinger for aktivering og deaktivering, ikke fire funksjonsgodkjenninger.
- Unix-socketen er `0600`, eid av den autoriserte brukeren. `SO_PEERCRED` kontrollerer uid; en åpen forbindelse kontrolleres også mot en endret konfigurasjon. Samme uid betyr fortsatt samme rettigheter: andre prosesser som brukeren kan sende de tillatte operasjonene.
- Vindu, `riggd` og hjelper kontrollerer valgt PCI/UUID. NVML åpnes med den bundne PCI-adressen og sammenligner UUID; ubundet åpning av GPU-indeks 0 avvises. Grunnopptaket er dessuten bundet til driver og VBIOS.
- Feltene `power`, `core_clock`, `mem_clock`, `fan_manual`, `fan_curve`, `temp_target` og `rgb` får separate egenskaper: tilgjengelighet, område, grunn og tilbakelesing. Oppdagelse gjør bare lesing og symbolsjekk, aldri en testskriving. Et tilgjengelig symbol beviser ikke at driveren vil tillate selve endringen. Manglende støtte på ett felt sperrer ikke uavhengige felt.
- Bare endrede felt sendes til `apply`. Tall utenfor backendens oppgitte områder avvises uten klemming til en annen GPU-modells policy. Ukjente felt og operasjoner avvises. RGB-operasjonene returnerer utilgjengelig/ikke støttet; lyskontrolleren er ikke identifisert.

### Grunnopptak, frister og feil

| Tilstand | Implementert handling |
|---|---|
| Første åpning etter ren administratoraktivering | Fang gjeldende innstillinger uten skriving. Ny fangst krever Auto-vifte. Lagre atomisk som `baseline.json` under `/var/lib/agentic-rigg/` før backend får skrive. Filen har modus `0600` i beskyttet mappe `0700`. |
| Vanlig oppstart eller recovery | Last det eksisterende opptaket og kontroller full identitet. Ikke erstatt det med verdier som kan stamme fra en avbrutt endring. En eksisterende Auto-baseline kan lastes selv om våre nåværende vifteverdier er manuelle. |
| Ny, ren administratoraktivering | Stopp gammel hjelper og krev bekreftet gjenoppretting. Kontroller at `dirty.json` ikke finnes, heller ikke som symlink. Arkiver tidligere grunnopptak som `baseline-forrige-*.json`; ny backend fanger deretter dagens utgangspunkt. Uavklarte endringer stanser aktiveringen før installasjonen erstattes. |
| Før endring | Lagre og synkroniser en beskyttet `dirty.json` bundet til identiteten før setter-kall. Et uendret Bruk oppretter ingen slik markør eller transaksjon. |
| Bruk uten Behold | Hjelperens frist er 10–60 s (standard 15 s). Bare eieren kan bekrefte/rulle tilbake transaksjonen. Fristutløp forsøker å gjenopprette det bekreftede oppsettet. |
| Delvis skrivefeil | Rapporter feilen. Forsøk tvungen gjenoppretting av bekreftet oppsett og deretter fangede grunnverdier. Gjenoppretting må ikke stole på en skrivecache etter en delvis feil. |
| Uverifisert gjenoppretting | Behold dirty-markøren og feilstatusen, sperr videre vanlige endringer og meld transaksjonen som feil. `reset()` som returnerer Ok er alene utilstrekkelig: snapshot må bekrefte `stock == true`. `null` er ukjent, ikke suksess. |
| Avvist eller ubekreftet merkør | En `dirty.json` med feil identitet, eier eller modus, eller en merkør som ikke kunne lagres, overskrives og slettes aldri av samme prosess. Det gjelder også senere stopp, endret oppsett og `reset`, og merkøren kontrolleres på nytt før den fjernes etter bekreftet standard. Er kortet skrevet til, går det straks til grunnverdiene med Auto-vifter og feilstatus. Er det uberørt, røres det ikke. I begge tilfeller avviser hjelperen nye endringer til omstart; `reset` tillates bare som gjenoppretting etter egne skrivinger. |
| Standard, frakobling eller stopp etter egne endringer | Gjenopprett fanget utgangspunkt, ikke en antatt fabrikkverdi. Opprinnelig effektgrense og klokkeoffset beholdes i grunnverdiene. Uberørt aktivering/stopp gjør ingen reset-kall. |
| Manglende livssignal eller hvilegap | Forbindelsen som eier endringene må vedlikeholdes av `riggd`. Manglende livssignal over 65 s eller et tick-gap over 15 s utløser gjenoppretting etter egne endringer. |
| Deaktivering eller utskifting av hjelper | Uavhengig `--tilbakestill` må lykkes før hjelpefiler fjernes eller gammel binær erstattes. Feil eller uverifisert tilstand beholder installasjonen/opptaket. Grunnverdifila og arkiver beholdes etter vellykket deaktivering. |

Klokkegettere kan returnere gamle offset. Backend rapporterer dette som manglende fysisk tilbakelesing (`readback: false` / `offsetsVerified`) og må ikke likestille en akseptert setter med en måling. Konservativ ukjent status kan derfor hindre deaktivering selv om setter-kallene returnerte suksess. En endret driver/VBIOS kan også kreve særskilt recovery; grunnopptaket brukes ikke automatisk på en annen identitet.

### Viftevern

Nødregelen over 85 °C gjelder bare dersom faktisk viftestyring er tilgjengelig og hjelperen eier aktive innstillinger. Ventende endring rulles tilbake før nødviften brukes; regelen slipper under 80 °C. Manglende fan-støtte skal ikke ødelegge en uavhengig effektgrense. Ukjent temperatur under Manuell utløser gjenoppretting. Kurvemotoren bruker temperaturkontroll, hysterese, rampe og utjevning; feil ved viftedrift behandles som skrivefeil og oppdaterer ikke en falsk vellykket cache. Dette erstatter ikke driverens og fastvarens eget vern og er ikke fysisk verifisert ennå.

I feiltilstand (`hardware_error`) gjelder ikke nødregelen, fordi hjelperen da ikke eier aktive innstillinger. Kortet kan likevel stå igjen med lav manuell vifte. Temperaturen leses derfor fortsatt hver runde. Ved 85 °C eller mer, eller når temperaturen ikke kan leses, forsøker overtemperaturvernet grunnverdiene med Auto-vifter. Det samme skriver stopp og `reset`. Vernet forsøker straks og så med 5, 10, 20, 40 og deretter 60 s mellomrom til standard er bekreftet ved tilbakelesing, og slipper under 80 °C. Det skriver aldri andre verdier, griper ikke inn på et uberørt eller sperret kort og åpner ikke for nye endringer. Feiltilstanden står til brukeren selv har tilbakestilt. Status viser vernet som `state.thermalGuard` (`attempts`, `stockVerified`).

### Systemtjenestens avgrensing

Tjenesten kjører som root med `CAP_CHOWN` for socketen og `CAP_SYS_ADMIN` for NVML. Systemet er skrivebeskyttet utenom tjenestens runtime- og tilstandsmapper. `ProtectHome`, `NoNewPrivileges`, privat nett, Unix-adressefamilie og filtrerte systemkall er beholdt. Enhetsadgang begrenses til `/dev/nvidiactl` og ett `/dev/nvidiaN`; aktiveringsskriptet leser Device Minor for den bundne PCI-adressen fra driverens eksisterende proc-opplysning. Ingen `/dev/i2c-*` tillates. Ressursgrensene er 128 MiB og 32 oppgaver. `Restart=no` unngår automatisk ny start med gamle aktive verdier; `ExecStopPost` forsøker uavhengig recovery når dirty finnes.

Protokollen har ingen generell shell-/programkjøring eller klientvalgte filstier. Den har fremdeles høyest fire forbindelser og 16 KiB per linje. `READY=1` krever at backend er åpnet og grunnopptaket er klart; en lyttende socket alene er ikke tilstrekkelig.

Første aktivering og oppdatering bruker fortsatt et skript og en binær fra brukerens skrivbare installasjonsmappe. En prosess med samme uid kan bytte dem før brukeren godkjenner med passord. Dette er en eksisterende tillitsgrense; hardening av den kjørende tjenesten løser ikke den. Etter installasjon ligger tjenestens filer root-eid i `/usr/local/libexec/`. Ingen ny systemd-sikkerhetsskår er målt for 1.4.0.

Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.

### Gjennomført fysisk prøve · 14. november 2026

Etter brukerens polkit-godkjenning meldte den herdede root-hjelperen klar NVML-backend. Enkeltprøver mot hjelperen verifiserte effekt 245 → 240 W, begge vifter Auto (35 %) → Manuell (50 %), kjerneoffset 0 → −15 MHz og minneoffset 0 → −10 MHz. Hvert felt ble kontrollert med fersk NVML-getter og revert. Avsluttende reset bekreftet 245 W, kjerne-/minneoffset 0/0 MHz, temperaturmål 83 °C og Auto på begge vifter. Alle tilbakelesinger stemte. En senere prøve verifiserte også flat 50 %-kurve med begge vifter på målet i 4 s og temperaturmål 83 → 82 °C med revert/reset. Avsluttende stock true bekreftet igjen 245 W, offset 0/0 MHz, 83 °C og Auto på begge vifter.

1.4.0 · bygg 23 er installert med kontrollert binærhash, uendrede innstillinger og sikret historikk. Prøven verifiserer den faktiske hjelperen under tjenestens sandbox. Deretter bekreftet hello/get_helper/get_state gjennom installert NVML-daemon kortbindingen. Apply 240 W + confirm/Behold ga fersk 240 W og reset til 245 W. Ny apply 240 W uten Behold ga rolled_back/timeout etter 10 s og fersk 245 W før noe ekstra reset-kall; avsluttende reset bekreftet stock true. Dette er en daemonprøve av effektgrensen, ikke en fysisk gjennomgang av alle GUI-handlinger. Den verifiserer heller ikke alle grenser, positiv overklokking, stresstest, drivertap eller strømbrudd.

### Tester og gjenstående kontroll

- `rigg-helper/src/motor.rs`: rene sim-/mocktester for global tillatelse, feil identitet, uavhengig støtte per felt, andre GPU-områder, delvis feil, reset-/recoveryfeil, uverifisert stock, dirty, frist/nødregel, ukjent temperatur og livssignal.
- `rigg-helper/src/main.rs` og `tests/skript.rs`: nye tester for bevaring av opptak ved oppstart/recovery, avvist ny Manuell-fangst, ren ny aktivering som arkiverer og dirty/symlink som stanser utskifting. Skripttestene bruker bare en falsk rot.
- `rigg-hw/src/gpu/skriv_nvml.rs`: injisert API dekker faktisk setter-rekkefølge, funksjoner som mangler, delfeil, Auto-forsøk, gamle offsetgettere og gjenoppretting av fangede verdier uten NVML eller enheter.
- `rigg-hw/tests/bare_lesing.rs`: skrivemodulen tilhører bare hjelperen; den vanlige datainnsamlingen skal fortsette å være lesende.
- Lukkede reviewfunn og UI-avgrensninger står i `kritikk-1.4.0.md`. Fysisk Wayland-pekerbruk er ikke kontrollert i denne runden; GTK-planen bruker interne capture-/pick-tester. Installasjon og den begrensede fysiske prøven er bekreftet over; publisering er ikke bekreftet her. Alle GUI-handlinger, øvrige grenser og strøm-/driverfeil må fortsatt prøves separat. Ingen strømbruddsgaranti eller fysisk RGB-reset påstås.

---

## Historisk gjennomgang · 1.2.0, bygg 18

Alt nedenfor er bevart som historikk fra 9. november. Versjonslåser, separate trinn, privilegier, testresultater, målinger og gjenstående plan gjelder den gamle implementasjonen; de beskriver ikke dagens 1.4.0-modell.

## Root-hjelperen · sikkerhetsgjennomgang (runde 7A, 1.2.0)

*Agent 7A, 9. november 2026. Gjelder `agentic-rigg-helper` (crate `app/crates/rigg-helper`), aktiveringen i `app/data/hjelper/`, klienten i `riggd/src/hjelper.rs` og bryteren i Innstillinger › Styring. Ingenting her er kjørt som root eller mot det ekte kortet: alt er testet som vanlig bruker mot det simulerte kortet og en falsk rot.*

### 1. Hva som er laget

| Del | Hvor | Rolle |
|---|---|---|
| `agentic-rigg-helper` | `/usr/local/libexec/agentic-rigg-helper` (systemtjeneste, root) | Tar imot fem faste operasjoner fra `riggd` over `/run/agentic-rigg/helper.sock` og legger verdiene på kortet med frist, bekreft eller rull tilbake |
| `helper.json` | `/etc/agentic-rigg/helper.json` (`0644 root:root`) | uid-en som aktiverte, og de godkjente trinnene (tom ved aktivering) |
| Systemd-enheten | `/etc/systemd/system/agentic-rigg-helper.service` | Herdet: ett privilegium, skrivebeskyttet system, bare skjermkortets enheter |
| Polkit-policyen | `/usr/share/polkit-1/actions/io.github.AgenticRigg.helper.policy` | Én handling for aktivering, én for deaktivering og én per trinn |
| `aktiver.sh`, `deaktiver.sh`, `trinn.sh` | Kopiert til `/usr/local/libexec/agentic-rigg/` | Kjøres av brukeren via `pkexec` fra bryteren |
| Klienten | `riggd/src/hjelper.rs` | Finner hjelperen, spør om `status`, sender `apply`/`confirm`/`revert`/`reset` videre |
| Bryteren | `rigg/src/settings/styring.rs` | `pkexec …/aktiver.sh` eller `deaktiver.sh`, status, trinnlisten |

Protokollen mellom `riggd` og hjelperen er NDJSON som mellom vinduet og `riggd` (`docs/produksjon/protokoll.md` kap. 12), høyst 16 KiB per linje og høyst fire forbindelser.

### 2. Sikkerhetsmodellen i fem linjer

1. Hjelperen tar bare imot fem faste operasjoner med tall; ingen kommandoer, stier, I2C-adresser eller fritekst finnes i protokollen.
2. Bare uid-en i den root-eide `helper.json` slipper inn (`SO_PEERCRED`, og socketen er `0600` og eies av den uid-en).
3. Hvert felt hører til et trinn, og et trinn som ikke er godkjent i `helper.json` (tom ved aktivering), gir `step_not_enabled`; bare root via polkit med administratorpassord kan godkjenne.
4. Tall utenfor områdene kortet melder, avvises; alt som legges på, rulles tilbake uten `confirm` innen 10–60 s, når `riggd` forsvinner, når hjelperen stopper, og over 85 °C går viftene til 100 %.
5. Det ekte kortet er i tillegg sperret i 1.2.0: NVML-skrivekoden er ikke kompilert inn, og sperren i `rigg-hw` er av, så hjelperen med `--backend nvml` laster ikke engang NVML.

### 3. Låsene, og hvordan de henger sammen

En skriveoperasjon fra vinduet må forbi alle disse, i denne rekkefølgen:

| # | Lås | Hvor | I 1.2.0 |
|---|---|---|---|
| 1 | `write_gate`: Bruk er bare aktiv mot den simulerte backenden i en utviklerforekomst | `rigg_core::txn::write_gate`, `widgets/locked.rs::send_write` | Lukket mot det ekte kortet. Vinduet sender aldri skriving dit |
| 2 | `riggd` ruter: hjelperen må være aktivert, svare, være fra samme bygg (`helper_outdated`), ha samme backend som tjenesten (`backend_mismatch`), og trinnet må stå i `status.steps` (`step_not_enabled`). Uten hjelper: `write_locked` med `details.reason = "helper_off"` (og sim-boka som før med `--backend sim`) | `riggd/src/ops.rs`, `hjelper.rs::via_hjelper` | Virker; testet ende til ende |
| 3 | Hjelperen: uid (`SO_PEERCRED`), trinnet i `helper.json`, grensesjekken, nødregelen, frist | `rigg-helper/src/server.rs`, `motor.rs` | Virker; testet ende til ende |
| 4 | Sperren i `rigg-hw`: `gpu::skriv::EKTE_SKRIVING = false` **og** cargo-funksjonen `ekte-skriving` er av (ingen Cargo.toml slår den på) | `rigg-hw/src/gpu/skriv.rs`, `skriv_nvml.rs` | Av. `--backend nvml` gir `write_locked` med `real_writes_off` |
| 5 | Kildesøket: NVML-skrivekall bare i `skriv_nvml.rs`, skrivemodulen brukes bare av hjelperen, `pkexec` bare i `styring.rs` | `rigg-hw/tests/bare_lesing.rs` | Grønn |

**Trinnlisten** er det brukeren styrer: `effekt` (effektgrense og temperaturmål), `vifter` (modus, fast fart, kurve), `klokker` (kjerne- og minne-offset) og `lys` (RGB, bare `not_implemented`). I `capabilities.write.steps` (vinduets «Hva skal til?») blir `effekt` til `power` og `temp_target`, `vifter` til `fan_manual` og `fan_curve`, `klokker` til `clocks`, og `lys` til `rgb`.

**Trinn 2 (effektgrense mot det ekte kortet)** krever at lås 1 og 4 åpnes, og at brukeren godkjenner `effekt` (lås 3). Lås 2, 3 og 5 blir stående.

### 4. Trusselbildet

Vi antar en Ubuntu-maskin med én menneskelig bruker, GNOME og polkit, og at hjelperen er aktivert. Det vi verner er **maskinvaren** (skjermkortet skal ikke bli ødelagt, overopphetet eller gjort ustabilt uten brukerens ja) og **root** (hjelperen skal ikke bli en vei til root).

| Angriper | Eksempel |
|---|---|
| A. Ondsinnet lokal prosess som **samme bruker** | Et skadelig skript i nettleseren, en npm-pakke, en utvidelse |
| B. Lokal prosess som **en annen bruker** (også systembrukere som `www-data`) | En tjeneste som er tatt over |
| C. Nettverket | – |
| D. Den som kan skrive i brukerens filer **før aktivering** | Samme som A, før brukeren slår på bryteren |

#### 4.1 Hva en ondsinnet prosess som samme bruker (A) kan

- **Koble til hjelperen** og sende de samme operasjonene som `riggd`. uid-sjekken skiller ikke mellom prosesser som samme bruker. Det kan ikke gjøres sikkert uten en egen bruker for `riggd` (se kap. 6).
- **Endre det de godkjente trinnene tillater, innenfor kortets områder.** Med `effekt` godkjent: sette effektgrensen et sted i 105–275 W. Med `vifter`: sette viftene til minste fart (35 %, kortets eget minimum). Med `klokker`: offset i det tjenesten tillater (−200…+300 MHz kjerne, −1000…+1500 MHz minne).
- **Holde en endring i live** ved å sende `confirm` selv, og holde forbindelsen åpen.
- **Gjøre maskinen ustabil** med klokke-offset (bildefeil, krasj) når `klokker` er godkjent.
- **Lure brukeren** til å godkjenne et trinn (falsk dialog), som den kan med alt annet som krever passord.

#### 4.2 Hva den ikke kan, og hvorfor

| Kan ikke | Hvorfor |
|---|---|
| Endre noe i et trinn som ikke er godkjent | Hjelperen sjekker feltene mot `helper.json` før noe annet. Fila eies av root (`0644`, mappa `0755`), og hjelperen nekter å starte hvis fila eller mappa kan skrives av andre. `trinn.sh` kan bare kjøres som root via polkit (`auth_admin`, passord hver gang) |
| Godkjenne et trinn selv | Polkit-handlingene `…trinn.*` er `auth_admin` for aktiv økt og `no` ellers; ingen `allow_any`. `trinn.sh` ligger der bare root kan skrive |
| Kjøre kommandoer, lese eller skrive filer, eller nå I2C gjennom hjelperen | Protokollen har bare `status`, `apply`, `confirm`, `revert`, `reset` (og `rgb_*` = `not_implemented`). Ukjente operasjoner gir `unknown_op`, ukjente felt `invalid_argument`. Hjelperen har ingen `exec`, ingen filstier fra klienten, og åpner ingen `/dev/i2c-*` (og enheten slipper ikke til andre enheter enn `/dev/nvidia*`) |
| Gå utenfor kortets områder | `check_ranges` avviser (ikke klemmer) tall utenfor det kortet melder (`nvmlDeviceGetPowerManagementLimitConstraints`, `GetMinMaxFanSpeed`, `GetClockOffsets`, terskelene). Tjenestens strengere policy for klokker klemmes i tillegg av `txn::merge` |
| Stoppe viftene eller koke kortet | Viftene kan ikke gå under 35 % (VBIOS). Over 85 °C går viftene til 100 % og ventende endring rulles tilbake, uansett hva klienten sender, til kortet er under 80 °C. Driveren og VBIOS har i tillegg sin egen nedklokking (slowdown 91 °C) og avstengning (96 °C) |
| Bli root gjennom hjelperen | Ingen operasjon tar en sti, et program eller en streng som brukes til noe annet enn et tall. Tjenesten har bare `CAP_CHOWN` (til socketen), `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, ingen nett, `SystemCallFilter=@system-service`, `MemoryDenyWriteExecute`. `systemd-analyze security --offline` gir **1,2 «OK»** (0 er best, 10 verst) |
| Overbelaste hjelperen | Høyst fire forbindelser og 16 KiB per linje (`message_too_large` og lukking), ett sekunds takt, `MemoryMax=64M`, `TasksMax=4` |
| Gjøre endringen varig uten at noen er der | Hjelperen lagrer ingenting. Den bekreftede tilstanden lever bare så lenge forbindelsen som skrev, er åpen; lukkes den, går kortet til standard med viftene på Auto. Stopper hjelperen (eller krasjer), setter `ExecStopPost` kortet til standard |

#### 4.3 Andre brukere (B) og nettverket (C)

- **Andre brukere** kommer ikke inn: socketen er `0600` og eies av brukeren i `helper.json`, og `SO_PEERCRED` sjekkes i tillegg ved hver forbindelse (`feil_uid_avvises`, `feil_uid_gir_feil_og_las`). root (uid 0) får aldri være klient (`aktiver.sh` og `Config::parse` avviser uid 0).
- **Nettverket** er stengt: `PrivateNetwork=yes`, `IPAddressDeny=any`, `RestrictAddressFamilies=AF_UNIX`.

#### 4.4 Før aktivering (D): den svakeste lenken

Bryteren kjører `pkexec ~/.local/share/agentic-rigg/hjelper/aktiver.sh`, og skriptet kopierer binæren fra samme mappe. Begge kan skrives av brukeren. En prosess som allerede kjører som brukeren, kan bytte dem ut **før** brukeren slår på bryteren, og få det brukeren godkjenner med passordet, kjørt som root.

- Det er samme risiko som `sudo make install` eller `pkexec` på et skript i hjemmemappa, og polkit-vinduet viser stien (`org.freedesktop.policykit.exec`, administratorpassord). En prosess som allerede er brukeren, kan uansett vise en falsk passorddialog.
- **Etter** aktivering kjører alt fra `/usr/local/libexec/` (eid av root): `deaktiver.sh` og `trinn.sh` startes derfra, og polkit-handlingene peker på de stiene (`exec.path`, og `exec.argv1` for trinnene). En ny versjon (status «Krever ny versjon») går gjennom `aktiver.sh` på nytt, og da gjelder det samme som første gang.
- Tiltak senere: en distribusjonspakke (`.deb`) legger hjelperen og policyen i `/usr` ved installasjon, så ingen brukerskrivbar fil kjøres som root.

### 5. Hva som er testet (som vanlig bruker)

| Test | Fil | Hva |
|---|---|---|
| `trinn_som_ikke_er_godkjent_avvises` | `rigg-helper/tests/ende_til_ende.rs` | Tom liste: alle felt og `rgb_set` gir `step_not_enabled`, null skrivinger |
| `godkjent_trinn_gir_bruk_og_behold` | samme | `apply` → `confirm`, hendelsen, et ikke-godkjent felt i samme `apply` avvises helt, `reset` |
| `fristen_ruller_tilbake` | samme | 10 s uten `confirm`: `timeout`, viftene tilbake til Auto |
| `feil_uid_avvises` | samme | Annen uid i `helper.json`: `permission_denied` og lukket forbindelse |
| `verdier_utenfor_kortets_omrade_avvises` | samme | Åtte tall utenfor: `invalid_argument` med `out_of_range`; `exec` og et I2C-felt avvises |
| `klienten_borte_gir_standard` | samme | Forbindelsen som skrev, lukkes: standard og Auto |
| `nodregelen_over_85_grader` | samme | 87 °C: 100 % og tilbakerulling; 70 °C: Auto igjen |
| `trinn_leses_pa_nytt_og_kan_trekkes_tilbake` | samme | Ny `helper.json` gjelder straks; trukket trinn gir standard; SIGTERM fjerner socketen |
| `socketen_er_privat_og_meldinger_har_grense` | samme | `0600`, 20 KB-linje gir `message_too_large` |
| `oppsett_andre_kan_skrive_i_avvises` | samme | `helper.json` med `0666` eller ukjent trinn: starter ikke |
| `ekte_kort_er_sperret_i_denne_versjonen` | samme | `--backend nvml`: `write_locked`/`real_writes_off`, og `libnvidia-ml` er ikke i `/proc/<pid>/maps` |
| `tilbakestill_gjor_ingenting_uten_trinn` | samme | `--tilbakestill` (ExecStopPost) rører ingenting uten godkjente trinn |
| `trinn_ikke_godkjent_avvises_og_godkjent_gir_bruk_og_behold` | `riggd/tests/hjelper.rs` | Gjennom `riggd`: `step_not_enabled`, godkjent trinn, `not_owner`/`busy` for en annen økt, `out_of_range` |
| `fristen_ute_ruller_tilbake_gjennom_tjenesten` | samme | `txn`-hendelsen `timeout` når fram til vinduet |
| `tjenesten_borte_gir_standard` | samme | `riggd` drept med SIGKILL: kortet til standard innen 3 s |
| `feil_uid_gir_feil_og_las`, `ulike_kort_sendes_ikke_videre`, `uten_hjelper_som_for_men_med_grunn` | samme | `helper_error`, `backend_mismatch`, `helper_off` |
| `aktiver_og_deaktiver_gir_riktig_filtre` | `rigg-helper/tests/skript.rs` | Nøyaktig sju filer med riktige moduser under `--root`, tom trinnliste, idempotent, deaktiver fjerner alt |
| `skriptene_gir_tydelige_feil`, `polkit_og_enheten_er_som_avtalt`, `install_kopierer_bare_filene_og_uninstall_sier_fra` | samme | Feilmeldinger, at `/` aldri røres uten root, policy og herding, `install.sh`/`uninstall.sh` |
| Motoren | `rigg-helper/src/motor.rs` | Ni enhetstester, også viftekurven med 7Bs viftemotor. `kurven_er_lik_tjenestens_sim_sloyfe` (7C): 480 s med egne kurver per vifte, hysterese, rampe, utjevning, stille flate og nødregelen gir samme fart hvert sekund som tjenestens sim-sløyfe (`rigg_hw::gpu::sim_vifter`) |
| `ekte_skriving_er_sperret_og_bare_for_hjelperen` | `rigg-hw/tests/bare_lesing.rs` | Sperren, cargo-funksjonen, og at bare hjelperen bruker skrivemodulen |

Release-binæren `agentic-rigg-helper` er 668 KB, lenker bare mot `libc` og `libgcc_s` (GLib/GIO fra `rigg-hw` er fjernet av LTO), og inneholder ingen `nvmlDeviceSet*`-navn (`strings`).

### 6. Kjente svakheter og hva som gjenstår før trinn 2

1. **Samme bruker = samme rettigheter.** Alle prosesser som brukeren kan snakke med hjelperen. Et eget systembrukernavn for `riggd` ville skille dem, men `riggd` er en `--user`-tjeneste. Det er et bevisst valg (som LACT med gruppa `wheel`/`lact`): trinnene og områdene er vernet, ikke hvem som bruker dem.
2. **Polkit per skriving er ikke brukt.** `docs/daemon-api.md` kap. 9 foreslo `CheckAuthorization` for hver skriving. `riggd` kjører som systemd `--user`-tjeneste uten innloggingsøkt, så polkit ville sett den som «ikke aktiv» og avvist (eller krevd `allow_any`, som ikke verner noe). Samtykket ligger i stedet i trinnlisten, som bare kan endres med administratorpassord, ett trinn om gangen.
3. **Rettigheten for NVML-skriving er ikke målt.** Enheten har bare `CAP_CHOWN`. Trinn 2 må prøve `nvmlDeviceSetPowerManagementLimit` med brukeren til stede og se om driveren krever `CAP_SYS_ADMIN`; i så fall legges den til i `CapabilityBoundingSet`, og ingenting annet.
4. **Standardverdiene for det ekte kortet** leses av `skriv_nvml.rs` (`GetPowerManagementDefaultLimit`, `ACOUSTIC_CURR` ved oppstart). Er temperaturmålet allerede endret av et annet verktøy, blir det standarden. Trinn 2 bør lagre verdiene fra første oppstart i `/var/lib/agentic-rigg/` (`StateDirectory=`).
5. **`riggd` holdes i live** mens hjelperen har andre verdier enn standard (`keep_alive`), så å lukke vinduet ikke tilbakestiller kortet etter 60 s. Med «Fortsett i bakgrunnen» av og verdier på kortet vil tjenesten derfor kjøre videre til kortet er på standard igjen. Vinduet bør si det tydelig i trinn 2.
6. **Første aktivering kjører brukerskrivbare filer som root** (kap. 4.4).

**Før trinn 2 (effektgrense mot det ekte kortet)**, i denne rekkefølgen og med brukeren til stede:

1. Brukeren aktiverer selv med bryteren (passord), og ser at status er «På · trinn: ingen».
2. En egen runde slår på `ekte-skriving` og `EKTE_SKRIVING` **bare for effekt** (de andre kallene i `skriv_nvml.rs` returnerer `not_supported` til sine trinn er klare), og åpner `write_gate` for `write.helper.state == "on"` med trinnet godkjent.
3. Lese grensene og standarden med `skriv_nvml::open` uten å skrive (`status.limits`), og sammenligne med `maskinvare-revisjon.md` kap. 8.
4. Brukeren godkjenner `effekt` (passord), og prøver én endring (for eksempel 245 → 225 W) med Behold og en med Rull tilbake, med `nvidia-smi --query-gpu=power.limit` ved siden av.
5. Avklare `CAP_SYS_ADMIN` (punkt 3 over) og standardverdiene (punkt 4).
6. Skru av bryteren og se at kortet står på 245 W (standard) igjen.
