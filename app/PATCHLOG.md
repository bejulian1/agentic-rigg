# Patchlog

## 1.7.0 · bygg 33 – 24. november 2026: nytt navn, svensk, Nettverk, Oppgaver, layout og RGB «Av» som holder

- Samler PR #32 (visningsnavn og svensk), #34 (layoutrevisjon, `math::temp_axis`, Om), #37 (GPU-aktivering med `--keep-rgb`, Oppgaver med `RssAnon`, gruppering og `get_task_detail`), #38 (`get_network`, Nettverk-siden), #39 (README og bilder), #40 (`widgets/charthead.rs`, `StatStrip` som `Cols`, `limit_label_tops`, `leadrow`), #43 (RGB-oppgradering med ett passord, `--keep-nvml`, `nvml_admin_files`, `TaskRow`), #45 (`rgb_persist`, `rgbAv`, fargevelger, `rgb-av-prove.py`) og #47 (`rgb_hold` som egen operasjon, `owned` i svarene, funksjonssjekk for eldre RGB-hjelpere).
- `widgets/oc.rs`: tallene under glideren viser alltid min, standard og maks. `place_ticks` flytter «standard …» inn mellom endene i stedet for å skjule en ende; `TICK_LEVELS` har tre former (enhet på alle, enhet bare ved standard, standard som bare verdien), og `share_ticks` lar radene på Klokker og effekt bruke samme form ut fra den smaleste synlige glideren. Minstebredden er uendret (100 px), så vinduet kan fortsatt bli 640 px. Tester: `tallene_under_glideren_viser_min_standard_og_maks`, `tall_uten_enhet`.
- `rigg_core::hjelper::MIN_COMPATIBLE_BUILD` står på 32 og protokoll 2 er uendret: NVML-delen av `rigg-helper` er ikke endret siden 1.6.0 (bare navnet i `--version`). `rgb_auth::MIN_COMPATIBLE_BUILD` står på 29; RGB-hjelpere uten `persist`/`hold` i statusen avgjøres som «må oppdateres» etter egenskaper (PR #47), ikke bygg. Ingen kompatibilitetstest er endret.
- `skjermbilder_ui140.rs`: nyeste utgave i versjonsloggen er 1.7.0. `docs/images/clocks-power.png` er tatt på nytt (simulerte data).
- Ikke prøvd på ekte maskinvare: «hold», varig «Av», gjenoppretting etter krasj og kaldstart. Ekte `pkexec` og nedlasting fra GitHub er bare dekket av kjedede tester med falske systemkall. Issue #19 (signert `SHA256SUMS`) står åpen.

## 1.6.0 · bygg 32 – 23. november 2026: sikkerhetsrettinger i NVML-hjelperen og systeminstallasjonen

- `rigg_core::hjelper::MIN_COMPATIBLE_BUILD` = 32 (protokoll 2 uendret, RGB 29 uendret). Kompatibilitetstestene i `rigg-core`, `riggd` og `system_install.rs` avviser 25–31 og godtar 32. `utgaven_med_nvml_rettingene_avviser_eldre_hjelpere` vokter grensen.
- PR #23 (`rigg-helper`): `server::boot_ms()` (`CLOCK_BOOTTIME`) og `Tidslinje` gir motoren monoton tid. Klokka leses før hver forespørsel, og tapt klokke stopper tjenesten. Nødregelen bruker `restore_stock` med bekreftet Auto når 100 % feiler. `dirty.json` har `fans`, og `NvmlSkriver::require_fan_count` hindrer falsk `stock` i `--tilbakestill`. `mark_dirty()` setter `marker_unverified` ved enhver feil, `apply`/`reset` går via `marker_failed()`, og `clear_dirty()` kontrollerer merkøren før den fjernes. Porten avviser skrivinger med ubekreftet merkør. `thermal_guard()` (issue #20) forsøker `Skriver::reset` ved ≥ 85 °C eller ukjent temperatur i feiltilstand, med 5–60 s mellomrom, bare på et kort prosessen har skrevet til og som ikke er sperret. Status: `state.thermalGuard`. IPC: accept-, linje- og bytegrenser per runde, kø i ankomstrekkefølge, ingen utføring etter EOF/halvlukking, POLLOUT-budsjett, socket fjernes bare ved samme dev/inode, tilfeldig midlertidig grunnverdifil.
- PR #24: `system-hent.py` sender avklaringsmålet på stdout etter `--restore-original` og `--inspect` og venter i høyst 15 minutter på `recoveryId` på stdin (`system_install.rs` svarer etter `confirm_restored`) (#17). Tillatelsene leses før og etter nedlastingen, og `/run/agentic-rigg-system.lock` arves via `AGENTIC_RIGG_SYSTEM_LOCK_FD` med kontroll av enhet/inode (#18). `pakk-utgivelse.py` legger `SYSTEMFILER.sha256` i arkivet, `install.sh` og `uninstall.sh` legger og fjerner den, og root krever nøyaktig de systemfilene. Strengere arkividentitet, høyst 256 MiB utpakket (#19, delvis). `rigg_core::paths::short_tmp()` for socketfixturer, og `nvml-admin.py` beholder høyst fem `baseline-forrige-*.json` (#21).
- PR #22: tray-stopp bruker `keep_alive()`. RGB-aktiveringen reparerer manglende betrodde filer, binder bekreftelsen til handlingen dialogen viste og avbryter ved endret behov. `system-install` avviser nedgradering under lås, kontrollerer GPU-oversikten etter UUID og avgrenser pakkekilden (vanlig fil, høyst 64 MiB, `O_NOFOLLOW`). Adminskriptene skiller eldre tillatelser fra feil eier og avviser tvetydige installerere.
- Ikke gjort (issue #19): signert `SHA256SUMS` og en fast root-eid hentehjelper med egen polkit-handling.

## 1.5.6 · bygg 31 – 21. november 2026: uavklart RGB-journal ved oppdatering

- `system-hent.py`: `settle_rgb()` kjører installerte `rgb-gjenopprett.sh --restore-original`, `rgb-avklar.sh --inspect` (JSON fra stdout, `recoveryId` må være 32 heks) og `rgb-avklar.sh --static-confirmed --recovery-id` etter kontrollert arkiv og før `rgb-deaktiver.sh`. Det skjer bare når `RGB_JOURNAL` finnes og forespurt aktivering er `rgb` for samme PCI/UUID som `rgb.json`; ellers stopper skriptet før nettet. Fem nye Python-prøver (18 totalt), blant annet rekkefølge etter kontrollert arkiv, feil sum med journal og stopp ved hvert feilende trinn. Feil etter gjenopprettingen gir egen melding på stderr, og kortmismatch nevner det bundne kortet.
- `lys_kort.rs`: aktiveringsdialogen forteller om gjenopprettingen når systemkomponentene skal installeres eller oppdateres.

## 1.5.5 · bygg 30 – 21. november 2026: lysvalgene alltid fremme

- `lys_kort.rs`: kontrollene er en `gtk::Grid` med overskriftene Modus/Farge/Lysstyrke og en `gtk::Scale` som deler justering med tallfeltet. Kontrollene, effektknappene og «Bruk» vises også uten bundet hjelper. Etterpå filtreres effektene på `softwareEffects` som før. Uten binding åpner «Bruk» `admin_dialog(Activate, then_apply = true)` (tilstand `off`/`outdated`). `queued` settes først etter den visuelle bekreftelsen, nullstilles ved avbrudd, feil og `hide()`, og utløser én `apply()` når hjelperen er bundet og `can_apply` tillater det.
- Avlesning, identitet, `offline_fallback` og administratorknappene er flyttet til et `gtk::Expander` («Detaljer og feilsøking»), som åpnes ved overgang til tilstanden `error`. `lys.rs`: uten binding forhåndsviser 2D/3D-kortet valgt modus, farge og lysstyrke (før: av). Ny visningsfixture-kontroll: `off` krever synlige kontroller, effekter og «Bruk».

## 1.5.4 · bygg 29 – 21. november 2026: 2D/3D-kortet og effektbytte

- `lys3d/` (2D og 3D), shaderne i `data/lys3d/`, valget `lys3d` og `--stage*`-tokenene er gjeninnført fra før `c4d5745`. `rgb.rs` er nå en forhåndsvisning med én sone (56 LED-er) som følger `CardLight`; demosonene og den lokale lagringen av lysvalg kommer ikke tilbake. `glow`/`glam` er tilbake i `LISENSER.md`.
- RGB-protokoll 3: `Book::apply_after_animation` leser kortet og sammenligner med siste effektramme før bytte. Avvik sperrer automatisk gjenoppretting uten skriving; `before` i journalen er rammen, `baseline` er uendret. Runtime tillater eier å bytte fra aktiv effekt og starte effekt fra beholdt fast farge/av. Lesefeil under bytte stopper effekten og frigir som ved feilet ramme; feil før lesing lar effekten gå videre. `lys_kort.rs` bruker `can_apply` og viser årsaken under «Bruk».
- `rgb_auth::MIN_COMPATIBLE_BUILD` = 29. `system_install::ready` tar `Need::{Cleanup, Nvml, Rgb}`, så RGB-grensen ikke gjør NVML utdatert. «Aktiver kortlys» er aktiv når RGB-hjelperen er utdatert og kan installeres.
- `system-hent.py`: leser aktive tillatelser (root-eid, samme `PKEXEC_UID`) før nettet brukes. Etter kontrollert arkiv kjøres installert `rgb-deaktiver.sh --pci --uuid` for en aktiv RGB-tilgang; etter installasjonen kjøres `aktiver.sh` for den forrige GPU-bindingen når brukeren ikke selv ba om GPU-styring. Tre nye Python-prøver.

## 1.5.3 · bygg 28 – 21. november 2026: én bryter, ett passord

- `system-hent.py` tar `VERSJON BYGG [nvml|rgb PCI UUID]`. Argumentene og `PKEXEC_UID` kontrolleres før nettet brukes. Etter vellykket installasjon kjøres `/usr/local/libexec/agentic-rigg/aktiver.sh` eller `…/agentic-rigg-rgb-v2/rgb-aktiver.sh --static-confirmed` med fast miljø og `PKEXEC_UID`. Feiler installasjonen, aktiveres ingenting.
- Skjermkortstyring: bryteren installerer og aktiverer i én `pkexec` når komponentene mangler eller hjelperen er utdatert. Den utdaterte tilstanden vises som av. Kortlys: «Aktiver kortlys» gjør det samme etter den visuelle bekreftelsen. `system_install::prompt` er erstattet av `offline_fallback` (bare terminalkommandoen).
- Python-prøvene dekker installasjon pluss aktivering, at aktivering bare skjer etter vellykket installasjon, og at ugyldig domene, PCI, UUID eller UID stopper før nettet brukes.

## 1.5.2 · bygg 27 – 21. november 2026: systemkomponenter med bare passordet

- Ny `data/hjelper/system-hent.py`, kjørt som `pkexec <python3> -I -S -c <skript> VERSJON BYGG`. Python-stien kanoniseres (`/usr/bin/python3` er en lenke) og må være en root-eid fil under `/usr/bin`. Skriptet henter `releases/tags/vVERSJON` fra GitHub-API-et, krever nøyaktig ett arkiv med forventet navn og `digest` `sha256:…`, laster ned med størrelsesgrense og tidsavbrudd til en ny `0700`-mappe under `/var/tmp`, sammenligner summen før tar åpnes, og kjører `system-install` med fast miljø.
- `system_install.rs`: installasjonsvinduet er erstattet av knappen «Installer systemkomponenter», som starter installasjonen direkte. Feilene for GitHub, manglende utgivelse og sumavvik oversettes. Bryteren installerer og aktiverer; ved utdatert hjelper aktiverer knappen styringen igjen etterpå.
- `scripts/test-system-hent.py` (falske nettsvar, ingen root) og `test-system-install.py` kjøres nå fra `cargo test` (`rigg-helper/tests/skript.rs`). Skjermbildeplanen `installer` er fjernet sammen med vinduet.

## 1.5.1 · bygg 26 – 21. november 2026: systemkomponenter uten terminal

- `system_install.rs`: installasjonsvindu med filvalg, SHA-256-felt og lenke til utgivelsessiden. Kjører `pkexec /usr/bin/bash -ceu 'PATH=/usr/bin:/bin; export PATH; <skript>' rigg ARKIV SUM`, der skriptet hentes fra `system-bootstrap.command` (én kilde; testen låser at appen og terminalkommandoen kjører det samme). `pkexec` og `bash` må være root-eide vanlige filer; `/bin` og `/usr/bin/env` er lenker på Ubuntu 26.04 og brukes derfor ikke.
- Skjermkortstyring: bryteren er aktiv når komponentene mangler og `pkexec` finnes, åpner installasjonen og kaller aktiveringen etter vellykket installasjon. RGB-siden oppdateres når installasjonen er ferdig.
- `bare_lesing.rs` og `clippy.toml` tillater `pkexec` i `system_install.rs`. Ny skjermbildeplan `installer` (trykker aldri Installer). Hjelperne, polkit-handlingene og `MIN_COMPATIBLE_BUILD` (25) er uendret.

## 1.5.0 · bygg 25 – 21. november 2026: ti språk, Oppgaver og lys på kortet

### Åpen kildekode
- Repoet er offentlig under MIT. Engelsk README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, maler for saker og PR-er og privat sårbarhetsrapportering. Forfatteradressen i første commit er byttet til GitHubs noreply-adresse; ellers er historikken bevart. Localhost-demoen og demoens `version.json` er tatt ut av sporingen og ligger bare lokalt.

### Språk
- `rigg_core::i18n` med `tr!`, `trf!` og `trn!`. Nøkkelen er den norske kildeteksten, og gettext-katalogene ligger per språk og modul i `app/data/i18n/`. Byggskriptet validerer og bygger dem inn, så pakken trenger ingen katalogfiler. Statiske oppslag returnerer `&'static str`.
- Språkvalg med «Følg systemet» og ti språk i Innstillinger › Generelt. «Start på nytt nå» starter vinduet på nytt. `riggd` leser språket ved oppstart. Meldinger fra hjelperne oversettes ved visning, ut fra stabile identifikatorer; protokollene er uendret.
- Tall-, dato- og klokkeformat per språk. Turtallsenheten følger språket. `i18n-trekk-ut.py --check` kontrollerer kataloger, plassholdere og flertall. `i18n-revisjon.py --check` finner bokmålstekst utenfor makroene.

### Demo fjernet
- 2D/3D-lysmodellen (`rgb.rs`, `lys3d/`, `data/lys3d/`), lokal lagring av lysvalg, innstillingen «3D-kortet», påskeegget «nordlys» og tekster om forhåndsvisning er fjernet. Eldre `ui.json` og `profiler.json` med disse nøklene leses fortsatt.

### Sikkerhet i systemkomponentene
- Felles `system-install` for NVML og RGB. Kjøres fra en root-eid kopi av arkivet etter SHA-256-kontroll, med `--uninstall` og `--revoke nvml|rgb`. NVML-aktivering bruker faste innganger under `/usr/local/libexec/agentic-rigg/` med `auth_admin` uten lagring. Testflagg er utilgjengelige i installert kode. Gammel aktivering fra `~/.local` tilbakekalles ved migrering.
- Administrative RGB-trinn og NVML-gjenoppretting kjører via `systemd-run` med lukket enhetspolicy. Flyktig RGB-kortregel ligger under `/run`. `python3 -I -S`, `ProtectProc=invisible` og avvisning av dupliserte JSON-felt.
- `MIN_COMPATIBLE_BUILD` er 25 for NVML og RGB, og `strictRequests: true` kreves. GPU-styring må aktiveres på nytt én gang.

### Palit-RGB

- Pusting, regnbue og fargesyklus i «Lys på kortet» bruker den egne RGB-hjelperen. Bruk viser startfargen, Behold starter animasjonen, og Stopp gjenoppretter lagret utgangspunkt. Fysisk verifikasjon av animasjonene på PALIT 100 gjenstår.

- Eget kildegrunnlag fra ThunderMaster-opptak og installert NVIDIA-driver; eksakte Palit 100/150-signaturer, faktisk fargeprotokoll og begrensninger ved ukjent effektmodus.
- Typet RGB-identitet, observasjon og baseline skiller registerverdier, brukerbekreftelse og faktisk avlest modus.
- Egen, streng RGB-tillatelsesmodell uten klientvalgte enhetsstier. Eksisterende v2-GPU-tillatelse gir ikke RGB; eldre eksplisitte simulerte trinn beholdes.
- Smal statisk/av-skrivetransport med fersk kort-/signatur-/fd-kontroll, fem faste registerskrivinger, avvisning av endrede fargefelt før skriving og separat gjenopprettingssti. Feil etter et forsøkt skrivekall behandles som mulig delvis endring.
- Selvstendig RGB-transaksjon med klientbinding, Behold, frist og tilbakerulling. Samsvarende farge/lysstyrke oppgraderes aldri til avlest effektmodus eller full gjenopprettingsbekreftelse.
- Separat `--rgb-server` med egen socket, peer-UID, RGB-kompatibilitet, 15-sekunders lease og privat journal før setterkall. Oppstart/status åpner ikke I2C; eksisterende journal sperrer oppstart uten automatisk fargeendring. Avgrenset socketbehandling og stopp/klienttap er testet mot fakes. [Protokoll og begrensninger](../docs/produksjon/rgb-hjelper.md).
- Egen administratoraktivering med brukerbekreftet statisk utgangspunkt, separate root-filer og systemd-tjeneste med kun ferskt identifisert I2C-enhet i DeviceAllow. Ingen automatisk oppstartsaktivering. Avklaring, eksplisitt tilbakeføring og deaktivering beholder journal ved uavklart tilstand; eksisterende NVML-hjelper endres ikke.
- Asynkron, avgrenset RGB-klient i daemonen med egen eier, kø, tidsfrist inkludert hviletid og heartbeat. GUI-tråden blokkeres ikke av RGB-socketen. Bare en levende eierøkt fornyer leasen.
- «Lys på kortet» har én fysisk sone, fast farge/av, lysstyrke, Bruk/Behold og gjenoppretting. Visuell bekreftelse krever aktiv avkryssing og gjelder det bestemte gjenopprettingsmålet fra dialogåpning; den gir ikke en avlest effektmodus.
- Separat, lesende `--rgb-identifiser --pci PCI --uuid UUID` i hjelperen. Streng GPU-/adapterbinding, én adresse og et fast lesesett; ingen registerskriving, skanning, konfigurasjonsendring eller omstart av GPU-hjelperen.
- Nye isolerte modell-/transport-/argumenttester, private administrasjonsskriptprøver og GTK-betjening mot en eksplisitt merket falsk RGB-arbeider. På brukerens Palit RTX 2080 Super JetStream er fast blått og automatisk tilbakeføring til grønt fysisk bekreftet ved 100 % lysstyrke. [Status og kilder](../docs/produksjon/palit-rgb-2026-11-17.md).

### Oppgaver

- Egen side under Overvåking, med søk etter navn/PID/bruker, kolonnesortering, prosesstre og avgrensede sider med gjenbrukte rader. Ctrl+Shift+Esc åpner siden; de ti opprinnelige tallsnarveiene beholdes.
- Den eksisterende prøvetråden samler prosessdata høyst hvert andre sekund ved behov. Behovet utløper etter fem sekunder uten forespørsler. Ingen ekstra overvåkingstråd eller eksterne prosesskommandoer; systemgrafene deler eksisterende App-data.
- CPU-delta følger PID og starttid med monoton klokke. Én logisk CPU er 100 %. RAM er RSS og inneholder delte sider; systemets minnebruk vises separat. Utilgjengelige målinger blir ukjente.
- GPU-minne gjenbruker NVML-prosessleseren og krever ferske data og uendret prosessidentitet. Manglende kortdata eller ukjent delbidrag gir ingen ufullstendig sum. GPU-prosent per prosess er foreløpig ikke implementert.
- Avslutning gjelder én bekreftet prosess og bare SIGTERM via pidfd. Ny kontroll av identitet, brukerrettigheter, kjernetråder og beskyttede prosesser, inkludert tilkoblede klienters foreldre. Ingen GPU-hjelper, administratoropptrapping eller automatisk eskalering.
- Pause/F5, gamle tjenester, frakobling og foreldede prøver har egne tilstander. Søk og sortering sender ikke et nytt signalmål ved radflytting.
- [Gjennomføring, kilder og verifikasjon](../docs/produksjon/oppgavebehandling.md).

## 1.4.1 · bygg 24 – 14. november 2026: enklere viftestyring og synlig bekreftelse

- **Versjonslogg og bidrag:** Innstillinger har en fast Versjonslogg-knapp, og Om har Vis versjonslogg. Dialogen viser de ni dokumenterte desktoputgavene fra 1.4.1 bygg 24 til 1.0.0 bygg 16, nyeste først, med korte endringer og uten løpende testtall. Historikken ligger som en egen innebygd ressurs. Om krediterer Julian, Anthropic Claude Opus 5.5 (Claude Code) og OpenAI GPT 6.1 Sol (Codex), og den gamle påstanden om at appen bare leser er erstattet med presis tekst om overvåking og støttet GPU-styring.

- **Enkel/Avansert Vifter:** vanlige modusvalg og manuell fart får førsteprioritet. Kurve, historikk og detaljert oppførsel vises i Avansert. Bytte av visning endrer ikke kortet; en allerede aktiv kurve fortsetter og får en tydelig forklaring også i Enkel.
- **Felles utkast i Enkel:** endringer fra Klokker og effekt og skjulte kurvefelt oppsummeres også på Vifter. Bruk gjelder hele utkastet; inaktive kurvevalg presenteres som slike og åpner ikke Bruk alene.
- **Lagret utgangspunkt:** reset-etiketter beskriver det fangede utgangspunktet, ikke en antatt fabrikkstandard. Det endrer ingen lagrede grunnverdier eller reset-kode.
- **Synlig Behold:** bekreftelseslinja ligger utenfor den rullende delen på både Vifter og Klokker og effekt, så nedtelling, Behold og Rull tilbake er tilgjengelige etter Bruk nederst på siden.
- **Klokker og effekt:** GPU-/minneklokke nå og NVIDIA-driverens rapporterte maksimum står i målefelt øverst. Gliderne heter GPU-justering og Minnejustering: 0 MHz betyr ingen justering. De er offset, ikke faste klokkefrekvenser. Effektgrensen viser både W og prosent av kortets standard. Temperaturmål og vifter ligger i Avansert, med Åpne Vifter som direkte lenke; kortets grenser og utilgjengelige funksjoner ligger i egne avanserte detaljer. Det beregnes ingen forventet sluttklokke ved å legge offset til rapportert maksimum.
- **Hjelp:** statisk tekst om at skriving er låst er erstattet med faktisk styringsstatus. En misvisende forklaring om NVML/minneomregning er fjernet fra brukerhjelpen; selve omregningen og HW-implementasjonen er uendret.
- **Kompatibel autorisasjon:** `MIN_COMPATIBLE_BUILD = 23` og uendret helperprotokoll 2 lar app/daemon bygg 24 bruke allerede autorisert hjelperbygg 23. Både første oppdagelse og løpende status bruker samme kontroll. Bygg 22, annen protokoll, manglende/overflytende metadata og hjelpere nyere enn appen avvises. Ingen ny administratorgodkjenning kreves bare for denne oppdateringen.
- **Avgrensing:** HW-skrivere, baseline, readback, frister og recovery er uendret. Senere sikkerhetsrettinger må øke minimumsbygget og ved behov protokollen; dette er innført som fast regel i AGENTS.md. RGB er fortsatt forhåndsvisning.
- **Presisering av viftestopp:** «Minst 35 % (driver)» og funksjonsstøtten beskriver driverens manuelle område 35–100 %. Tidligere påstand om at VBIOS fysisk umuliggjør 0 % var for bastant: `nvmlDeviceGetMinMaxFanSpeed` oppgir området brukeren kan sette, ikke støtte for automatisk viftestopp. Automatisk Zero RPM på dette kortet er ubekreftet. Ingen backend-/grenseendring, 0 %-prøve eller ny Zero RPM-funksjon er gjort. Kilde: [NVIDIAs NVML-dokumentasjon](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html). [Brukerrapporter på NVIDIA-forumet](https://forums.developer.nvidia.com/t/any-workaround-to-the-30-fan-speed-limitation-on-newer-nvidia-gpus/291486) beskriver ulike erfaringer med drivere og kort, men bekrefter ikke dette kortets Auto-støtte.

- **CPU på den faktiske maskinen:** Linux-CPU-ID-er beholdes ved hull i nummerrekken og hotplug. Prosessor, Sensorer og rapport bruker de aktive ID-ene; flere sokler og ulike cachegrupper behandles separat. Cachevarianter og antall bygger på sysfs-identitet og deling, og ukjent antall vises som ukjent. Temperatur bruker sensorens navn; L3-grupper kalles ikke automatisk CCX.
- **GPU etter funksjonsstøtte:** Klokker, effekt og vifter bruker valgt korts tilgjengelige felt og rapporterte grenser. Viftevisningen skiller ukjent antall, null vifter og én til flere vifter, uten å finne på et kort med to vifter. Kontroller og kurvevalg følger det rapporterte antallet og støtten.

- **Sluttkontroll:** 410 tester bestått, 4 ignorert og ingen feil; ren workspace-clippy. Private GTK-planer bestod for versjonsloggens to innganger og rekkefølge, fast Behold etter rulling, ukjent/0/1/3 vifter og smal visning, samt Prosessor, System og Disker i lyst/mørkt. Manuell 55 % forble bekreftet etter 71 s i simulering. Produksjonsbinærene oppgir 1.4.1 bygg 24. Ingen nye GPU-skriveprøver er utført; HW-sikkerhetsmotoren er uendret.

## 1.4.0 · bygg 23 – 14. november 2026: kortbundet GPU-styring og brukerflate

- **Én aktivering:** Innstillinger › Skjermkortstyring bruker én administratorgodkjenning for valgt GPU. De fire separate godkjenningene er fjernet. Eldre `helper.json` v1 blir ikke automatisk full autorisasjon; brukeren må aktivere det nye oppsettet. Installasjonen under `~/.local` aktiverer fortsatt ingenting.
- **Faktiske funksjoner per felt:** effektgrense, kjerneoffset, minneoffset, manuell vifte, viftekurve og temperaturmål rapporterer egen tilgjengelighet, område, grunn og mulighet for tilbakelesing. Manglende temperaturmål eller minneoffset sperrer ikke fungerende effektstyring. Oppdagelse bruker gettere og tilgjengelige setter-symboler uten å prøve en endring; driveren kan fortsatt avvise selve skrivingen.
- **Riktig kort og riktige verdier:** hjelperen bindes til PCI-adresse og UUID og åpner aldri ubundet GPU-indeks 0. Skriveoperasjoner krever samme identitet i vindu, tjeneste og hjelper. Bare endrede felt sendes; grensene kommer fra den aktuelle backenden fremfor en fast RTX 2080-policy. `get_state` og skrivesvar gir flate aktive og fangede grunnverdier til brukerflaten.
- **Fanget utgangspunkt:** kortets innstillinger fanges og lagres atomisk i en beskyttet grunnverdifil før skriving. Opptaket binds også til driver og VBIOS og beholdes ved vanlig oppstart/recovery. Ren ny administratoraktivering arkiverer det gamle først etter bekreftet gjenoppretting og uten dirty-markør, så dagens utgangspunkt fanges på nytt. «Standard» og stopp gjenoppretter dette opptaket, også opprinnelige effektgrenser og klokkeoffset. Nytt opptak krever Auto-vifte; et eksisterende Auto-opptak kan brukes til recovery mens våre manuelle verdier er aktive.
- **Ærlige feil:** delvis mislykket endring forsøkes gjenopprettet til bekreftet oppsett og deretter fanget utgangspunkt. En dirty-markør lagres før første endring. Den fjernes bare når grunnverdiene er bekreftet; ukjent eller mislykket tilbakelesing gir feilstilstand og sperrer videre endring. Avinstallering av hjelperen og utskifting av en gammel hjelper avbrytes hvis gjenoppretting ikke kan bekreftes. Klokkegettere kan vise gamle offset; dette merkes som uverifisert, ikke som en fysisk måling.
- **Frist og viftesikkerhet:** Bruk/Behold, eierskap og tilbakerulling er beholdt. Frakobling, manglende livssignal, hvilegap og stopp utløser gjenoppretting etter egne endringer. Nødvifte krever faktisk viftestøtte; temperaturbortfall under Manuell går til gjenoppretting. Feilet tilbakerulling meldes som feil, ikke som vellykket rollback.
- **Hjelpertjenesten:** root-eid oppsett v2, privat uid-kontrollert socket, beskyttet tilstandsmappe, CAP_SYS_ADMIN for NVML og CAP_CHOWN for socketen. Enhetsadgang avgrenses til NVIDIA-kontrollnoden og det valgte kortets `/dev/nvidiaN`. Klarstatus krever en tilgjengelig backend; en lyttende socket alene er utilstrekkelig. Ingen I2C-tilgang eller generelle kommandoer i protokollen.
- **RGB:** lyskontrolleren er fortsatt ikke identifisert. Styringssiden kan lagre og vise lysvalg, men sender dem ikke til kortet, selv om NVML-styring ellers er klar.
- **PCIe-modell:** 82 kontaktflater per side (11 + 71) på x16-kanten, åpent nøkkelspor, faset innføringskant og delte kontaktmål for 2D, mesh og tekstur. Dette er en illustrasjon, ikke produsentens CAD eller en identifikasjon av kortvarianten.
- **Fokus og menyer:** sensorfilteret slipper fokus ved klikk utenfor og ved sideskifte; Esc tømmer/avslutter søket og tømming virker straks. Nedtrekksmenyer får sammenhengende farger for valgt rad, hover og tastaturfokus. Låst-/klarstatus følger valgt GPU og tjenestens oppdateringer.
- **Endelig brukerflatekontroll:** Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.
- **Kontrollstatus:** siste fullførte workspace-kjøring gir 388 bestått, 4 ignorert og 0 feilet; workspace-clippy på alle mål er ren. Den native GTK-planens assertions bestod, blant annet fokus/nedtrekksmenyer, én aktivering i testoppsettet, Bruk/Behold, status og PCIe 2D/3D. Egne sim-/mocktester dekker blant annet feil identitet, manglende støtte, delvis skriving, resetfeil og uverifisert gjenoppretting.
- **Aktivering uten hengende lås:** en eksisterende appforekomst håndterer nå `--innstillinger DEL`. Hjelperstatus poller uavhengig av klarstatus, slik at aktivering utenfor vinduet blir oppdaget. Bare meningsfulle statusendringer utløser ny layout. Watcher-testen og testisoleringen inngår i den fullførte workspace-kjøringen.
- **Fysisk prøve og installasjon:** Brukeren har fullført ekte polkit-aktivering, og begrenset skriving mot den herdede NVML-hjelperen er verifisert på RTX 2080: 245 → 240 W, begge vifter Auto (35 %) → Manuell (50 %), kjerneoffset 0 → −15 MHz og minneoffset 0 → −10 MHz. Hvert felt har fersk NVML-tilbakelesing og revert; avsluttende reset bekreftet 245 W, offset 0/0 MHz, temperaturmål 83 °C og Auto på begge vifter. En senere prøve verifiserte flat 50 %-viftekurve på begge vifter i 4 s og temperaturmål 83 → 82 °C, med revert/reset og bekreftet stock true ved avslutning. Installerte 1.4.0-binærer og uendrede innstillinger er kontrollert, og historikken er sikret. Gjennom installert NVML-daemon er kortbinding, apply 240 W + confirm/Behold, fersk 240 W og reset til 245 W verifisert. Ny apply 240 W med 10 s frist uten Behold ga rolled_back/timeout og fersk 245 W før noe ekstra reset-kall; avsluttende reset ga stock true. Alle grenser, positiv overklokking, stresstest, drivertap og strømbrudd gjenstår. Fysisk Wayland-pekerbruk er ikke verifisert i denne runden; de interne GTK capture-/pick-testene er en egen avgrensning. Ingen nye ytelsestall påstås for 1.4.0. Se `docs/produksjon/kritikk-1.4.0.md` for lukkede reviewfunn.

## 1.3.3 · bygg 22 – 14. november 2026: CPU-lanseringsdato og spesifikasjonsrevisjon

- Prosessor og rapport får lanseringsdato fra et lokalt modelloppslag med kilde. Ryzen 7 2700X viser 19. april 2018 fra AMD. Datoen bestemmes ikke av CPUID-familien og arves ikke av ukjente CPU-modeller.
- Komponentenes viste, skjulte og manglende opplysninger er kartlagt i `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`. RAM-moduler, skjerm og nettverksdetaljer har størst behov for utvidelse. Øvrige funn er ikke implementert i denne versjonen.
- Ingen endring av maskinvaretilgang eller GPU-styring.

## 1.3.2 · bygg 21 – 14. november 2026: adaptive måleakser og gjennomgang

- Ferske ytterverdier tas med før midtpunktet i en åpen langtidsbøtte er synlig. Interpolerte linjer og bånd ved vinduets kanter tas med i akseberegningen. Tegningen bruker lineær verdiavbildning og klipper først på tegneflaten, slik at en topp som forlater vinduet ikke endrer kurvens helning.
- Viftekurvens målte punkter og «Nå»-punkt får plass også ved 110 °C, under 20 °C og vifteverdier over 100 %. Tegning, rutenett og peker bruker samme visningsskala. Redigerbare punkter beholder de lovlige grensene, og aksen fryses under dragging før ventende ytterverdier tas med etter slipp.
- Forklaringen til «Tilpass / Fast» sier at begge utvider området når verdiene går utenfor. Bare last og brukt minne i prosent har hard grense 0–100.
- Nye regresjonstester dekker 110 °C, kjerneklokke 2500 MHz mot oppgitt maks 2190, minneklokke 10000 MHz, CPU-klokke 6000 MHz, effekt 400 W, disk på 90 °C og høye disk-/nettrater, med «Tilpass» og «Fast».
- Kodegjennomgang og UI/UX-runde er dokumentert i `docs/produksjon/kritikk-1.3.2.md`. Åpne reviewfunn er skilt fra grafrettingene. Ingen installasjon eller endring av ekte maskinvare.

## 1.3.1 · bygg 20 – 13. november 2026: kortmeny, sidefelt og søkefokus

- Kortet under Skjermkortstyring åpner detaljene ved raden du trykker på. Menyen følger sidefeltets bredde, med kort navn, tydelige grupper og tekst som brytes ved behov. Driver/CUDA står side om side; VBIOS, PCIe, oppdatering og tjeneste har egne felt. Med flere kort er det valgte merket.
- Klikk på skjermkortet øverst under appnavnet åpner Grafikkort direkte, også i ikonlista. Pil og verktøytips følger handlingen.
- Ikonene i sammenlagt sidefelt er sentrert. Innstillinger, hjelp og utviding har samme bredde og plassering som navigasjonsknappene; maskinhodet og appikonet følger samme midtlinje.
- «Mer om kortet» og kortvalg lukker menyen før handlingen. Skjermbildeplanen `kortmeny` kontrollerer menybredde, stor skrift, lang drivertekst, temaer, kortvalg, navigasjon og sentrering av Innstillinger.
- Innstillinger: klikk utenfor søket slipper fokus og den blå rammen. Første Esc avslutter søket og beholder Innstillinger; neste Esc går tilbake. Kryssknappen oppdaterer treffene straks. Et raskt seksjonsbytte tømmer også tekst som ennå ikke er behandlet av søkeforsinkelsen, slik at gamle treff ikke dukker opp igjen. Ctrl+F, / og tastaturfokus er bevart.
- Ingen endring i maskinvaretilgang. Det ekte kortet er fortsatt sperret. Webdemoen har sin egen versjon og er uendret.

## 1.3.0 · bygg 19 – 9. november 2026: Oversikt over hele maskinen, grafer som ikke kutter

Etter brukerens ønsker: grafene skal stige naturlig når temperaturen går mot 80, 95 eller 110 °C, Oversikt skal vise hele systemet som GNOME Resources, Disker skal ikke ha tomrom, og Styring skal tydelig handle om skjermkortet. Runde 8A gjorde alt (`docs/produksjon/LOGG.md`). Det ekte kortet er fortsatt sperret: ingenting skrives dit i 1.3.0.

**Ingen graf kutter verdier**
- **Før:** «Tilpass» fulgte dataene, men innenfor et fast ytre område per graf (GPU- og CPU-temperatur 20–100 °C, disk 20–60 °C, effekt, klokker og VRAM med egne tak), og «Fast» viste bare det området. 110 °C ble en flat strek i toppen, og under 20 °C forsvant.
- **Nå** er området et *foretrukket* område, ikke en grense (`chart::math::Axis`, `y_axis`). Går dataene over eller under, utvides aksen til en pen skala med litt luft, både med «Tilpass» og «Fast». GPU-temperatur 40–110 °C gir 25–125 °C (Tilpass) eller 20–120 °C (Fast), 12–18 °C gir 5–30 °C (Tilpass) eller 0–100 °C (Fast), 330 W gir 0–400 W, og en disk på 65 °C får plass.
- **Hard grense** bare der verdien ikke kan gå over: last og minne i prosent (0–100). Viftene har ingen, fordi NVML kan melde over 100 %.
- **Aksen pumper ikke** (`AxisHold`): den utvides straks når kurven trenger plass, men krymper først når den har ønsket seg mindre i 15 s, og da til det største den har ønsket seg i den tiden. Et bilde uten data (historikken hentes på nytt) drar den ikke tilbake til hele området.
- **Grenselinjer:** nedklokking (97 °C) og avstenging (100 °C) står som stiplede streker med navn i GPU-temperaturgrafene når de er innenfor aksen, og tas med i aksen når kurven er nær. Effektgrensen står som før.
- **Seks merker** når det gir en tettere akse (8–117 °C blir 0–125 i steg på 25, ikke 0–150 i steg på 50).
- **Gjelder alle grafene:** Oversikt, Grafer, grafen under en rad på Sensorer, Disker, Klokker og effekt, og Vifter (også «Siste 5 minutter», der % og °C deler akse: den utvides i steg på 25 over 100 °C). Sparklinjene (statusblokken, flisene, Sensorer, Prosessor, Disker) holder 0–100 % som foretrukket område og går over når en vifte melder 105 %.
- **Aksetallene** har så mange desimaler som steget trenger: steg på 0,25 gir «0,25 0,50 0,75» (før «0,2 0,5 0,8»), og 2,5 gir «2,5 5,0 7,5» (før «3 5 8»).
- **Nettgrafen** har 0–1 MB/s (8 Mbit/s) som foretrukket område, før 0–1000 MB/s. Vanlig trafikk var da en strek ved null; nå synes den, og mer trafikk utvider aksen.
- Kurveredigereren på Vifter beholder 20–100 °C og 0–100 %: det er et redigeringsområde.

**Ny Oversikt over hele maskinen**
- **Seks fliser øverst:** Prosessor (last, temperatur, snittklokke), Skjermkort (last, temperatur, effekt av grensen, P-tilstand), Minne (brukt av totalt, GiB og %), VRAM, Disker (samlet MB/s, varmeste disk, SMART-helsa) og Nettverk (ned og opp). Hver med ikon, sparklinje for siste minutt og verktøytips. Klikk åpner Prosessor, Grafikkort, System, Disker eller Sensorer. Seks spalter, tre under 900 px og to under 560 px.
- **Tilstand-linja** står bare når noe er verdt å se, det alvorligste først: kortet svarer ikke, ny driver, ingen driver, nedklokking (termisk eller maskinvare, ikke effektgrensen under last), GPU- eller CPU-temperatur over varselgrensene eller ved nedklokkingsgrensen, minnet nesten fullt, en disk med advarsel eller feil. Klikk åpner siden for delen.
- **Grafene** (tre spalter på brede skjermer, to, og én under 700 px): Last (CPU og GPU), Temperaturer (GPU, CPU og de to varmeste diskene, med grenselinjene), GPU-effekt, Minne og VRAM (i %, GiB i verktøytipset), Disk-I/O og Nettverk. Tidsvalget i toppfeltet styrer alle.
- **Skjermkortet nederst:** Temperatur og effekt (målerne som før) og Skjermkort (P-tilstand og begrensning, kjerne- og minneklokke mot kortets maks, vifter med turtall, viftestyring, PCIe, video), med lenke til Grafikkort. Klokker-panelet og VRAM-raden er flyttet: de står på Grafikkort og i flisene.
- **Rolig i bruk:** grafene på Oversikt flytter seg med prøvene (hvert sekund, i samme bilde som flisene), ikke hver piksel mellom dem, og målerne og klokkestolpene hopper rett til ny verdi. Under last, når kortet står i P0 og alle tallene endrer seg hvert sekund, ga glidingen og den jevne rullingen 4–5 bilder i sekundet. Kortets tilstand står ikke lenger i en linje øverst på Oversikt og Styring-sidene, fordi tilstand-linja og «Styrer»-linja sier det der.
- **Uten driver** viser Skjermkort-flisa det, VRAM og skjermkortdelen skjules, og grafene viser prosessor, minne og disker. Uten UDisks2 står «helse ukjent», uten disktemperatur «uten temperatur».
- Ingen nye data fra tjenesten: alt fantes. Topp-prosesser er ikke tatt med (krever en ny lesing av `/proc/*/stat` hvert sekund; ikke billig nok for startsiden).

**Disker uten tomrom**
- Disk og SMART står side om side med like høye kort, og radene fordeler høyden i det laveste kortet. Partisjoner står i hele bredden under, så tabellen får plass. Under 720 px står alt under hverandre.

**Styring handler om skjermkortet**
- Seksjonen i sidefeltet heter **Skjermkortstyring**, og under tittelen står kortet som styres («RTX 2080», eller «Palit RTX 2080» når produsenten er kjent). Klikk åpner detaljene og kortvalget.
- Toppfeltet sier **Skjermkort › Klokker og effekt**, **Skjermkort › Vifter** og **Skjermkort › RGB-lys**, og det samme står i vindustittelen og Startside.
- **«Styrer»-linja** øverst på de tre sidene: GPU-ikon, navnet, produsenten, PCI-adressen, VBIOS og driveren, funnet av seg selv fra NVML og sysfs. Med flere kort står «Kort 1 av 2» med valget. Kortet borte, ingen driver eller ny driver: linja sier det, og at styringen er utilgjengelig.
- **Produsenten** leses fra PCI-subsystemet når kortprodusenten har satt sin egen ID (1569 Palit, 10b0 Gainward, 1462 MSI, 1043 ASUS og flere, `rigg_core::vendor`). NVIDIAs egen (10de, som på dette Palit-kortet) gir «NVIDIA-referansekort», og forklaringen sier hvorfor. Blyanten i linja setter «Produsent (valgfritt)», lagret per kort (UUID) i `ui.json` (`gpuMakers`), og vist i linja og maskinhodet.
- Panelhodene: «Kortets viftekurve», «Hvem styrer kortets vifter», «Kortets vifter», «Lyskontrolleren på kortet». Innstillinger › Skjermkortstyring, tastatursnarveiene og verktøytipsene i sidefeltet bruker de samme ordene.

**Ytelse**
- Grafene leser kortets faste data uten å kopiere dem (`App::with_statics`). Før kopierte grafene for klokker og effekt dem ved hver tegning.
- Stolpene og målerne melder verdien til skjermleseren i hele prosent, og bare når den endres.
- **Hjelperen:** avviste hjelperen tjenesten (feil bruker) og lukket forbindelsen før tjenesten rakk å skrive, ble grunnen «Broken pipe». Nå leser tjenesten avvisningen som ligger der, og grunnen er «Bare brukeren som aktiverte hjelperen, får koble til». Sett som en test som feilet under last.

**Sim og skjermbilder**
- Scenarioet `sim:hete`: under 20 °C, 95 °C og 110 °C med 300 W, prosessoren med, én disk på 65 °C og én på 14 °C.
- Planene `8a` (Oversikt i lyst og mørkt, 1920/1440/900/640 px, høy kontrast, Styrer-linja med og uten produsent) og `8a-hete` (aksene med Tilpass, Fast og °F på Oversikt, Grafer, Klokker og effekt, Vifter og Disker). `6b-tilstand` viser også Oversikt og Styrer-linja med to kort, kortet borte, ingen driver og ny driver.

## 1.2.0 · bygg 18 – 9. november 2026: bryteren for endring og Afterburner-vifter

Etter brukerens ønsker til 1.2.0 (`docs/produksjon/tilbakemelding/1.2.0.md`): en enkel måte å slå på og av det som trengs for å endre klokker, lys og vifter, og viftekurver like fleksible som i Afterburner. Runde 7A og 7B laget det nye, og 7C kritiserte og finpusset (`docs/produksjon/kritikk-1.2.0.md`, 28 funn). Det ekte kortet er fortsatt sperret: ingenting skrives dit i 1.2.0.

**Bryteren og hjelperen (7A)**
- **Innstillinger › Styring** (ny del): bryteren «Tillat endring av skjermkortet», status (Av, På, Feil, eller «Slå på igjen» etter en ny versjon), trinnlisten og «Hva bryteren gjør».
- **Root-hjelperen** `agentic-rigg-helper` (ny crate `rigg-helper`): fem faste operasjoner over en privat socket, bare for brukeren din. Den har grensesjekk mot kortets egne områder, frist med Behold eller tilbakerulling, nødregel over 85 °C og standardverdier når tjenesten eller hjelperen stopper. Ingen kommandoer, filstier eller I2C.
- **Bryteren** kjører `pkexec aktiver.sh` eller `deaktiver.sh`. Det er brukerens egen handling med passord. Skriptene legger inn og fjerner hjelperen, polkit-regelen og en herdet systemtjeneste.
- **Trinnene** godkjennes hvert for seg med et eget spørsmål og passordet: Effekt, Vifter, Klokker og RGB-lys. Alle er av ved aktivering, og mot det ekte kortet kan de ikke godkjennes i 1.2.0 («Kommer senere»).
- **Tjenesten** sender skrivingen videre til hjelperen når den er på (`get_helper`, `step_not_enabled`, `helper_off`, `backend_mismatch`). Vinduet snakker aldri med hjelperen.
- **Sidefeltet:** merket ved Styring viser tilstanden: «Bare visning», «Endring på · låst», «Endring på · effekt», «Endring: feil» eller «Endring: slå på igjen».
- **Det ekte kortet er sperret to ganger:** cargo-funksjonen `ekte-skriving` er av, og `EKTE_SKRIVING = false`. Hjelperen med det ekte kortet laster ikke engang NVML. Sikkerhetsgjennomgangen står i `docs/produksjon/hjelper-sikkerhet.md`.
- **`install.sh`** legger filene i `~/.local/share/agentic-rigg/hjelper/` og aktiverer ingenting. **`uninstall.sh`** fjerner dem og sier fra hvis hjelperen fortsatt er på.

**Vifter (7B)**
- **Viftemotoren** (`rigg_core::vifte`, ren logikk) er den samme i tjenesten (sim) og i hjelperen. Den har nødregel, utjevning, hysterese, kurve per vifte med stille flate, minste fart og rampe.
- **Felles kurve · Vifte 1 · Vifte 2:** egen kurve per vifte. Kurven til den andre viften står svakt bak.
- **Oppførsel:**
  - hysterese 0–10 °C, tegnet som et bånd i grafen
  - rampe opp og ned i %/s
  - utjevning (av, 3, 5 eller 10 s)
  - «Hold minimum til __ °C»

  Under hver står en forklaring i klartekst, for eksempel «Fra 35 % til 100 % på 7 s».
- **Viftestopp (0 %)** er ikke mulig på RTX 2080. Fastvaren holder minst 35 %, også i Afterburner, og appen sier det.
- **Siste 5 minutter:** motoren kjørt over historikken, ved siden av slik viftene faktisk gikk, med snitt, høyeste og antall endringer.
- **Forhåndsvalgene** Stille, Balansert og Ytelse setter både kurve og oppførsel.
- **Sim:** varmemodell og scenarioet `sim:spill`, så kurven, hysteresen og rampen synes i grafene.

**Kritikk og finpuss (7C)**
- **Klartekst:**
  - Låst-linja, toasten og «Utkast · låst» ber ikke lenger brukeren godkjenne et trinn som ikke kan godkjennes mot det ekte kortet.
  - Feilstatusen viser en forståelig melding, ikke en rå systemfeil. Det tekniske står i verktøytipset.
  - Teksten i Innstillinger › Styring er uten sjargong («administratorrettigheter», ikke «root-tilgang», «polkit» eller «systemtjenesten»), og «Hvem»-raden lover ikke mer vern enn det finnes.
  - «Endring» brukes overalt i stedet for «skriving».
- **«Hva skal til?»** viser bryteren og de samme fire trinnene som Innstillinger › Styring, med en knapp dit.
- **Hjelperen bruker viftemotoren som tjenesten:**
  - start fra farten viftene har (`med_start`)
  - nytt oppsett bare når det er endret
  - ett steg per runde, også under nødregelen, så viftene rampes ned etterpå
  - test: hjelperen og tjenestens sim-sløyfe gir samme fart hvert sekund i 8 minutter. Sim-sløyfen ligger nå i `rigg-hw/src/gpu/sim_vifter.rs`.
- **RGB-lys** sier at lys kommer i en senere versjon, ikke at I2C «ikke er slått på».
- **«Siste 5 minutter»** forklarer begge linjene med egne kurver per vifte. «Nødregel 100 % over 85 °C» står ikke lenger der kurven går.
- **Tre Kan-punkter fra 6D:**
  - «P8 / tomgang» står bare én gang på Klokker og effekt («Begrenset av: Ingenting · Kortet er i ro»).
  - Utkast-merket forsvinner når utkastet er likt kortet.
  - Modellnavnet på Disker kuttes ikke ved 640 px.
- **Innstillinger ved 640 px:** vindusknappene står i høyre kant.
- **Vifter ved 640 px:** flisene øverst står 3 + 3, ikke 5 + 1.
- **`--version`** har samme form for alle tre binærene: «Agentic Rigg 1.2.0 · bygg 18» og «Agentic Rigg-hjelperen 1.2.0 · bygg 18».
- **`bilder.sh --plan alle`** har planene for 7A og 7B, og standardmappa er `docs/skjermbilder/1.2.0/`.

## 1.1.0 · bygg 17 – 9. november 2026: Disker, Prosessor, nytt skall og ny Styring

Etter brukerens tilbakemelding på 1.0.0 (`docs/produksjon/tilbakemelding/1.1.0.md`). Runde 6A, 6B og 6C laget det nye, og 6D kritiserte og finpusset (`docs/produksjon/kritikk-1.1.0.md`, 29 funn).

**Nye sider (6A)**
- **Disker:** én rad per disk med helse (OK / Advarsel / Feil / Ukjent), temperatur, MB/s og aktivitet. Utfoldet: modell, serie, fastvare, o/min, SMART-helse med grunner, driftstid, skrevet og lest, levetid, partisjoner med ledig plass, MB/s-graf og hele SMART-tabellen. UDisks2 over D-Bus uten root; aldri selvtest, aldri `/dev/sd*`.
- **Prosessor:** Tdie/Tctl, last, klokker, governor og boost, klokke og last per tråd gruppert etter CCX og kjerne, cache, instruksjonssett og sårbarheter (det CPU-Z-delen på System var).
- **System** har hovedkort/BIOS, minne, OS og kontrollere (nett, lyd, USB, SATA, NVMe).
- Disktemperatur og -hastighet i Sensorer og Grafer, og varsel for diskhelse (av som standard).

**Skallet (6B)**
- **Vindusknappene** som i GNOME/libadwaita: usynlig treffområde, sirkel på 24 px, riktig avstand, og plassering etter `gtk-decoration-layout`.
- **Maskinhodet** øverst i sidefeltet: «RTX 2080 / 8 GB · driver 610.43.02» og «Ryzen 7 2700X / 8 kjerner · 16 tråder». Ingen grønn prikk; feil står i tekst.
- **Styring** med nye navn og forklaring: **Klokker og effekt**, **Vifter** og **RGB-lys**, og merket «Bare visning». `id` og `--side` er uendret.
- Tastene **1–9 og 0** for de ti sidene.
- **Code Pro** nederst i Innstillinger › Om.

**Styring redesignet (6C)**
- Store tall øverst (klokker, temperatur, effekt, last, vifte, P-tilstand og «Begrenset av»), glidere med standardmerke, tallfelt og «Tilbakestill» per rad, «Tilbakestill alt til standard», og (i) med hva hver kontroll gjør.
- Vifter: kurveredigerer med faste akser, minste fart og nødregel tegnet inn, driverens faktiske kurve i bakgrunnen, forhåndsvalg og tastatur.
- Ctrl+Z / Ctrl+Shift+Z i utkastet. Alt er fortsatt låst mot det ekte kortet.

**Kritikk og finpuss (6D)**
- **P0-toppene** 6C så etter ca. 1 min, kom fra andre agenters hodeløse Shell, ikke fra tjenesten (målt alene, `maalinger.md` runde 6D).
- **Statusblokken** gir diskene navn («sda temp», «nvme0n1 les»), med modellen i verktøytipset.
- **°F:** temperaturmålet og kurvepunktene vises og leses i °F når det er valgt.
- **Tastatursnarveier** har Styring-tastene og to spalter.
- **Grafhodene** kutter aldri tittelen: forklaringen og «min · maks» står helt eller ikke.
- **Vifter ved 640 px:** kurvenavnet og «Data for …» får plass.
- **RGB-lys** har samme låst-linje som de to andre Styring-sidene. «Låst» i toppfeltet vises ikke når Bruk er på (testdata).
- Sårbarhetene med riktige navn (L1TF, MDS, iTLB Multihit …), «lastsnitt» i stedet for «last», diskradene på Sensorer med kjernenavnet først, tall og enhet brytes ikke fra hverandre, og gamle sidenavn er borte fra tekstene og `--help`.
- To tester som kunne feile under høy last, venter nå på tilstand.
- `bilder.sh --plan alle` har planene for 6A, 6B og 6C.

## 1.0.0 · bygg 16 – 7. november 2026: runde 4B, finpuss før installasjon

Rettingene etter kritikken i `docs/produksjon/kritikk-1.0.0.md` (43 funn), brukerens svar på produktspørsmålene og en egen tastaturrunde. Nummeret i parentes er funnet i kritikken.

**Brukerens valg (spesifikasjonen kap. 10)**
- **Lys åpner med 2D-kortet.** Trykker du 3D, huskes valget til neste gang (`lys.picked` i `profiler.json`). Ny innstilling Utseende › Lys › «3D-kortet: Auto / Alltid 3D / Alltid 2D» (`lys3d`, standard Auto = 2D til du har valgt 3D én gang). Første oppbygging av 3D vekker kortet (P0 i ca. 4 s) og koster ca. 120 MB i vinduet, så det skjer bare når du ber om det (30).
- **«Fortsett i bakgrunnen» er av som standard** og står nå øverst i Innstillinger › Generelt. Forklaringen sier med ord hva av og på betyr, med de målte tallene (21–28 MB, 0,85 % med 2 s).
- **Start ved innlogging** er av, og **skrivebordsvarsler** er av som standard. Begge kan slås på.
- **Systemfeltet** (40): menyen har «Åpne Agentic Rigg», «Start logging» eller «Stopp logging» (alltid synlig) og «Stopp bakgrunnsopptak», som slår av valget og stopper tjenesten hvis ingen logg går. Verktøytipset har temperatur, effekt og last: «GPU 52 °C · 12 W · 8 % last».
- **«Kjerneklokke» overalt** (5): Sensorer («Grafikklokke»), statusblokken («GPU-klokke»), CSV-kolonnen og klokketabellene («Grafikk» → «Kjerne»). Minneklokka heter «Minneklokke».
- **Merket «Lest fra kortet» er fjernet** fra Kontroller (20).
- **Oversikt uten driver** (10): bare GPU-delen blir en tom tilstand («Fant ingen NVIDIA-driver»). Grafene viser CPU-last, CPU-temperatur og minne.

**Må før installasjon**
- **Statusblokken** (1): navnene får plassen tallet og sparklinja ikke bruker, og kuttes ikke («Kjerneklokke», «Minneklokke»). «Tråd 3» uten «MHz»; enheten står ved tallet. Ny test sjekker lengden og at ingen navn gjentar enheten, og skjermbildeplanen sjekker det samme med «Stor» skrift.
- **Flisene på Oversikt** (7) står i 6, 3 eller 2 spalter etter bredden, som demoen. Skillelinjene følger rutenettet.
- **«standard»-merket på Kontroller** (19) står under standardverdien på sporet (245 W ved 245, 0 ved 0), og flyttes inn hvis det ville havnet oppå endene.
- **Kontroller når kortet er borte** (20): banneret sier «Kortet svarer ikke. Verdiene vises igjen når det er tilbake.» «Lag utkast» begynner fra siste lesing.
- **Viftekurve** (25, 26): dekningen vises som tid, fra samme kilde begge steder («Data for 3 min av 48 t», «1 min av 24 t»). Under ett døgn sier linja «siden 18:11 (3 min)», ikke «siste døgn».
- **«Stopp tjenesten nå»** (33) i Generelt › Bakgrunn, med tekst om hva som stoppes (loggen nevnes når den går) og angre i 5 s. Tjenesten avslutter når siste vindu er lukket, uten de vanlige 60 s.
- **Lisensene** (34): `data/LISENSER.md` (ikonene, Rust-bibliotekene og MIT) er bygget inn og vises fra Om › «Lisenser …». `install.sh` legger den i `~/.local/share/doc/agentic-rigg/`.

**Bør**
- **Sparklinjene i statusblokken** har minste spenn (2): turtall 200 o/min, temperatur 5 °C, effekt 20 W, prosent 10, klokker 400 MHz.
- **Kortvelgeren** (3): «Svarer ikke siden 18:03» med NVML-koden i verktøytipset, fokus på «Mer om kortet» (verdien ble markert), og PCIe som på Oversikt («x16 Gen 1 · maks Gen 3»).
- **Tastatursnarveiene** (4): dialogen er høy nok til alle radene, og foten har egen bakgrunn og en skillelinje.
- **Temperaturstanga** på Oversikt (8) begynner på 20 °C.
- **Grafene når kortet er borte** (9): hullet til høyre kommer etter 3 s og sier «Kortet svarer ikke siden 18:03» (eller «Svarer ikke» når det er smalt), og flaten før dataene sier «Data fra 18:01», ikke «Samler data …». PCIe viser «–».
- **Minnet summerer** (15): «Buffer og cache» er uten delt minne (Shmem), som allerede er med i «Brukt».
- **Klokkeområdet på System** (16): «2 200–3 700 MHz · boost til 4,35 GHz (målt)» fra tjenestens høyeste trådklokke.
- **Det mørke båndet på offset-sporene** (21) forklares under raden: «Mørkt felt: tillatt når skriving slås på (−200…+300 MHz).»
- **«o/min»** brytes ikke lenger over to linjer (22).
- **Begrensninger og Profiler** (23): brikkene er de samme som på Sensorer («P8», «Tomgang»), teksten er «Kortet er i tomgang (P8). Årsakene leses når kortet jobber.» med 25 ms-forklaringen i verktøytipset, og «0 av 5» strekkes ikke.
- **Aksetekstene på Viftekurve** (27) kolliderer ikke i hjørnet, og det siste tallet går ikke utenfor grafen.
- **Viftekurve når kortet er borte** (28): «–» med «Kortet svarer ikke», ikke «Leses ikke ennå».
- **Ingen omtale av demoen** (35) i Innstillinger › Data.
- **Tastaturrunden** (43): se under.

**Kan**
- Viftestyring heter «Auto · driveren styrer» og «Manuell · satt utenfor appen» overalt (5).
- Swap og andre MiB-tall uten desimaler (11). Nett heter «mottatt» og «sendt» (12).
- «Skjerm aktiv (driverens flagg)» med forklaring (13). Revisjonen er «0xA1», ikke «0x0XA1» (14).
- «PREEMPT_DYNAMIC» og «17h (23) · 8 · 2» på System (17). Instruksjonssettet med store bokstaver og en linje om hva uthevet betyr (18).
- «Ikke dokumentert» på Lys har spørsmålstegn, ikke øye (32).
- Stiene i Data får plass (36). Versjonen skrives «1.0.0 · bygg 16» overalt, også i `--version` (37).
- «Takt i bakgrunnen» er dempet når den ikke gjelder (38).
- Med «Driveren er oppdatert» viser statusblokken «–» for GPU-verdiene (41).
- Legendene sier «Skjul GPU», ikke «Skjul gpu».
- Tallkolonnene på Sensorer vokser med skriftstørrelsen («10,93 MB/s» ble kuttet med «Stor»), og navnene i «Leses ikke» brytes over to linjer i smale vinduer i stedet for «…».
- Forklaringene i Generelt › Bakgrunn er kortere, og «2 s-takt» brytes ikke ved bindestreken.
- Skjermbildeplanene skriver alle etiketter med «…» til `kuttet.txt`, så avkutting blir funnet uten å se på hvert bilde. Planen `3c` tar også statusblokken med festede nøkler i standard og «Stor» skrift, Om, Lisenser, snarveiene og Lys før og etter 3D.

**Tastaturet (43)**
- Prøvd med ekte tastetrykk: et virtuelt tastatur i den hodeløse Shell (`docs/produksjon/verktoy/runde4b/`), skjermbilder fra Shell og vinduets fokuslogg (`RIGG_FOKUS=1`, bare utviklingsprofilen). Bildene og stegene ligger i `docs/skjermbilder/1.0.0/runde4b/tastatur/`.
- **Fokusringen sto rundt alle foreldrene** til flaten med fokus (flisraden, innholdet, toppfeltet), fordi GTK setter `:focus-visible` på hele kjeden. Nå `:focus:focus-visible`, og tekstfeltene viser fokus bare med rammen.
- **Tab sto fast** på siste flate på en side (grafen på Kontroller, punktskyen på Viftekurve). Nå går Tab rundt til starten, som i GTK ellers.
- **Verdiene i panelene var stopp for Tab** (hver verdi i Nå, Siste døgn, Feilsøking). De kan fortsatt merkes og kopieres med musa, men Tab går bare mellom ting som kan brukes.
- **Esc i et tomt filter** på Sensorer flytter fokus ut av feltet; første Esc tømmer det.
- Ctrl+, Ctrl+F og søk i Innstillinger, Esc, Ctrl+B, Ctrl+?, Ctrl++ og Ctrl+0, Ctrl+W og Ctrl+Q er prøvd og virker.

**Ytelse:** uendret eller bedre. Tallene står i `docs/produksjon/maalinger.md` (runde 4B) og i `STATUS.md`.
