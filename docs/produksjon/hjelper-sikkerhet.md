# Root-hjelperen · sikkerhetsgjennomgang (runde 7A, 1.2.0)

*Agent 7A, 9. november 2026. Gjelder `agentic-rigg-helper` (crate `app/crates/rigg-helper`), aktiveringen i `app/data/hjelper/`, klienten i `riggd/src/hjelper.rs` og bryteren i Innstillinger › Styring. Ingenting her er kjørt som root eller mot det ekte kortet: alt er testet som vanlig bruker mot det simulerte kortet og en falsk rot.*

## 1. Hva som er laget

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

## 2. Sikkerhetsmodellen i fem linjer

1. Hjelperen tar bare imot fem faste operasjoner med tall; ingen kommandoer, stier, I2C-adresser eller fritekst finnes i protokollen.
2. Bare uid-en i den root-eide `helper.json` slipper inn (`SO_PEERCRED`, og socketen er `0600` og eies av den uid-en).
3. Hvert felt hører til et trinn, og et trinn som ikke er godkjent i `helper.json` (tom ved aktivering), gir `step_not_enabled`; bare root via polkit med administratorpassord kan godkjenne.
4. Tall utenfor områdene kortet melder, avvises; alt som legges på, rulles tilbake uten `confirm` innen 10–60 s, når `riggd` forsvinner, når hjelperen stopper, og over 85 °C går viftene til 100 %.
5. Det ekte kortet er i tillegg sperret i 1.2.0: NVML-skrivekoden er ikke kompilert inn, og sperren i `rigg-hw` er av, så hjelperen med `--backend nvml` laster ikke engang NVML.

## 3. Låsene, og hvordan de henger sammen

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

## 4. Trusselbildet

Vi antar en Ubuntu-maskin med én menneskelig bruker, GNOME og polkit, og at hjelperen er aktivert. Det vi verner er **maskinvaren** (skjermkortet skal ikke bli ødelagt, overopphetet eller gjort ustabilt uten brukerens ja) og **root** (hjelperen skal ikke bli en vei til root).

| Angriper | Eksempel |
|---|---|
| A. Ondsinnet lokal prosess som **samme bruker** | Et skadelig skript i nettleseren, en npm-pakke, en utvidelse |
| B. Lokal prosess som **en annen bruker** (også systembrukere som `www-data`) | En tjeneste som er tatt over |
| C. Nettverket | – |
| D. Den som kan skrive i brukerens filer **før aktivering** | Samme som A, før brukeren slår på bryteren |

### 4.1 Hva en ondsinnet prosess som samme bruker (A) kan

- **Koble til hjelperen** og sende de samme operasjonene som `riggd`. uid-sjekken skiller ikke mellom prosesser som samme bruker. Det kan ikke gjøres sikkert uten en egen bruker for `riggd` (se kap. 6).
- **Endre det de godkjente trinnene tillater, innenfor kortets områder.** Med `effekt` godkjent: sette effektgrensen et sted i 105–275 W. Med `vifter`: sette viftene til minste fart (35 %, kortets eget minimum). Med `klokker`: offset i det tjenesten tillater (−200…+300 MHz kjerne, −1000…+1500 MHz minne).
- **Holde en endring i live** ved å sende `confirm` selv, og holde forbindelsen åpen.
- **Gjøre maskinen ustabil** med klokke-offset (bildefeil, krasj) når `klokker` er godkjent.
- **Lure brukeren** til å godkjenne et trinn (falsk dialog), som den kan med alt annet som krever passord.

### 4.2 Hva den ikke kan, og hvorfor

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

### 4.3 Andre brukere (B) og nettverket (C)

- **Andre brukere** kommer ikke inn: socketen er `0600` og eies av brukeren i `helper.json`, og `SO_PEERCRED` sjekkes i tillegg ved hver forbindelse (`feil_uid_avvises`, `feil_uid_gir_feil_og_las`). root (uid 0) får aldri være klient (`aktiver.sh` og `Config::parse` avviser uid 0).
- **Nettverket** er stengt: `PrivateNetwork=yes`, `IPAddressDeny=any`, `RestrictAddressFamilies=AF_UNIX`.

### 4.4 Før aktivering (D): den svakeste lenken

Bryteren kjører `pkexec ~/.local/share/agentic-rigg/hjelper/aktiver.sh`, og skriptet kopierer binæren fra samme mappe. Begge kan skrives av brukeren. En prosess som allerede kjører som brukeren, kan bytte dem ut **før** brukeren slår på bryteren, og få det brukeren godkjenner med passordet, kjørt som root.

- Det er samme risiko som `sudo make install` eller `pkexec` på et skript i hjemmemappa, og polkit-vinduet viser stien (`org.freedesktop.policykit.exec`, administratorpassord). En prosess som allerede er brukeren, kan uansett vise en falsk passorddialog.
- **Etter** aktivering kjører alt fra `/usr/local/libexec/` (eid av root): `deaktiver.sh` og `trinn.sh` startes derfra, og polkit-handlingene peker på de stiene (`exec.path`, og `exec.argv1` for trinnene). En ny versjon (status «Krever ny versjon») går gjennom `aktiver.sh` på nytt, og da gjelder det samme som første gang.
- Tiltak senere: en distribusjonspakke (`.deb`) legger hjelperen og policyen i `/usr` ved installasjon, så ingen brukerskrivbar fil kjøres som root.

## 5. Hva som er testet (som vanlig bruker)

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

## 6. Kjente svakheter og hva som gjenstår før trinn 2

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
