# Agentic HW Monitor

Appen het «Agentic Rigg» til og med 1.6.0. Bare visningsnavnet er nytt. Kommandoene (`agentic-rigg`, `agentic-riggd`, `agentic-rigg-helper`), app-ID-en `io.github.AgenticRigg`, repoet `bejulian1/agentic-rigg` og alle mapper og systemstier heter det samme som før, så innstillinger, historikk, logger (`~/Dokumenter/Agentic Rigg/`) og en allerede aktivert GPU-hjelper følger med uten nytt passord. Se [REGLER.md](../docs/produksjon/REGLER.md#navn).

Appen har også **Oppgaver** under Overvåking (Ctrl+Shift+Esc), med prosess-CPU, RSS-minne, tilgjengelig GPU-minne, prosesstre, søk, sortering og bekreftet SIGTERM. Systemgrafene deler eksisterende telemetri. GPU-prosent per prosess er ikke implementert. Se [måledefinisjoner og begrensninger](../docs/produksjon/oppgavebehandling.md).

Agentic HW Monitor er et overvåkingsverktøy for skjermkort og maskin på Ubuntu, skrevet i Rust med GTK 4. Det samler det du ellers bruker MSI Afterburner, HWiNFO, GPU-Z og CPU-Z til på Windows:

- **Oversikt** over hele maskinen, i stil med GNOME Resources: fliser for prosessor, skjermkort, minne, VRAM, disker og nettverk, en tilstand-linje når noe er verdt å se (nedklokking, varm GPU, disk med advarsel, kortet borte), grafer for last, temperaturer, effekt, minne, disk og nett fra siste minutt til 48 timer, og skjermkortets målere nederst.
- **Sensorer**: alle verdier i én tabell, med min, maks og snitt, sparklinjer og logging til CSV.
- **Grafer**: opptil elleve grafer med felles markør, pause og CSV-eksport.
- **Grafikkort**, **Prosessor**, **Disker** og **System**: alt NVML, sysfs, `/proc` og UDisks2 kan si om kortet, prosessoren (klokke og last per tråd, cache, sårbarheter), diskene (SMART-helse, temperatur, MB/s, partisjoner, som CrystalDiskInfo) og maskinen, og rapport som tekst eller Markdown.
- **Skjermkortstyring:** **Klokker og effekt** (effektgrense, klokke-offset, temperaturmål, vifte), **Vifter** (viftekurve som i Afterburner, med egen kurve per vifte, hysterese, rampe, utjevning og «hold minimum til») og **RGB-lys**. Klokker og Vifter viser målte GPU-data; RGB-lys styrer kortlyset på Palit GeForce RTX 2080 Super JetStream via den separate RGB-hjelperen. NVML-styring er implementert for funksjonene kortet støtter, etter én administratoraktivering (se «Endring av skjermkortet» under). Begrenset fysisk NVML-skriving og gjenoppretting er verifisert mot hjelperen på RTX 2080; fast blått og tilbakeføring til grønt er verifisert på PALIT 100.

En allerede åpen app følger nå `--innstillinger DEL`. Hjelperstatus sjekkes også mens styring er låst, slik at ny administratoraktivering oppdages uten å starte vinduet på nytt.

Sidefeltet har et maskinhode med skjermkortet og prosessoren, og tastene 1–9 og 0 går til de ti sidene. Klikk på skjermkortet øverst åpner Grafikkort direkte. Kortet under Skjermkortstyring åpner en smal detaljmeny i sidefeltets bredde, med kortvalg når det finnes flere kort. Ikonene står sentrert også når sidefeltet er lagt sammen. Versjon og byggnummer står i Innstillinger › Om, sammen med signaturen Code Pro. Grafene kutter aldri en verdi: aksen utvides når temperaturen går over 100 °C eller under 20 °C, og krymper først når kurven har roet seg. Denne fila gjelder 1.6.0 · bygg 32. Sensorfilteret slipper fokus ved klikk utenfor og ved sideskifte; Esc tømmer/avslutter søket. Nedtrekksmenyene har tydelige tilstander for valgt rad og tastaturfokus.

Prosessor viser også lanseringsdato når den konkrete modellen er i det lokale oppslaget. Ryzen 7 2700X viser 19. april 2018, med AMD som kilde. Ukjente modeller får ingen antatt dato. Gjennomgangen av øvrige spesifikasjoner og mangler står i `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`.

Måleaksene utvides også for 110 °C og klokker over kortets oppgitte maksimum. Ferske topper i langtidsvinduer og viftekurvens målte punkter tas med; lovlige redigeringsgrenser og 0–100 % for last og brukt minne beholdes.

## Nytt i 1.6.0

- GPU-hjelperen er sikrere: fristene følger en monoton klokke som tåler hvile, nødregelen godtar aldri lav manuell vifte som gjenoppretting, og gjenopprettingsmerkøren slettes aldri når den ikke kan bekreftes. I feiltilstand leses temperaturen fortsatt, og ved 85 °C eller ukjent temperatur forsøker hjelperen å sette kortet tilbake til grunnverdiene med Auto-vifter. Hjelperen må oppdateres én gang med administratorpassordet.
- En uavklart RGB-økt avklares under oppdateringen først etter at du har sett det gjenopprettede lyset. Installasjonen holder én felles systemlås, kontrollerer tillatelsene på nytt etter nedlastingen og godtar bare systemfilene som hører til den installerte appen.
- Stopp av bakgrunnsopptak fra systemfeltet beholder holdte GPU- og RGB-verdier. Systeminstallasjonen avviser nedgradering og tilbys ikke over et nyere installert bygg.

## Nytt i 1.5.6

- Oppdateringen av RGB-hjelperen stopper ikke lenger på en tidligere RGB-økt som ikke er avsluttet. «Bruk» setter kortet tilbake til det lagrede lyset, avslutter økten, oppdaterer og aktiverer med ett passord.

## Nytt i 1.5.5

- RGB-lys viser Modus, Farge og Lysstyrke også før kortlyset er aktivert. Første «Bruk» aktiverer (eller oppdaterer) RGB-hjelperen med ett passord og bruker valget som en vanlig prøve. Hjelpetekst og sjeldne knapper ligger under «Detaljer og feilsøking».

## Nytt i 1.5.4

- RGB-lys viser skjermkortet i 2D eller 3D igjen, koblet til den ekte kortstyringen. Du kan bytte fritt mellom fast farge, av og effektene. RGB-hjelperen får ny protokoll; «Aktiver kortlys» oppdaterer den med ett passord og beholder GPU-styringen.

## Nytt i 1.5.3

- Én bryter, ett passord: «Tillat endring av skjermkortet» installerer det som mangler og slår på full skjermkortstyring i samme godkjenning. «Aktiver kortlys» gjør det samme for RGB.

## Nytt i 1.5.2

- Systemkomponentene installeres med bare administratorpassordet: «Installer systemkomponenter» (eller bryteren for skjermkortstyring) henter utgivelsen fra GitHub, kontrollerer den mot summen GitHub oppgir og installerer. Bryteren slår på styringen etterpå.

## Nytt i 1.5.0

- Ti språk i hele brukerflaten (Innstillinger › Generelt › Språk, standard «Følg systemet»): norsk bokmål, engelsk, tysk, fransk, spansk, portugisisk (Brasil), italiensk, polsk, forenklet kinesisk og japansk.
- Oppgaver under Overvåking, og Lys på kortet for Palit GeForce RTX 2080 Super JetStream med fast farge, av, lysstyrke, pusting, regnbue og fargesyklus.
- Den lokale lysmodellen og alt annet som bare var demo er fjernet.
- GPU- og RGB-hjelperen installeres med én kontrollert systeminstallasjon og bruker faste root-eide polkit-innganger. Laveste kompatible hjelperbygg er 25, så GPU-styring må aktiveres på nytt én gang. Se [RGB-hjelperen](../docs/produksjon/rgb-hjelper.md) og [hjelpersikkerhet](../docs/produksjon/hjelper-sikkerhet.md).

## Nytt i 1.4.1

- **CPU på den faktiske maskinen:** Linux-CPU-ID-er beholdes ved hull i nummerrekken og hotplug. Prosessor, Sensorer og rapport bruker de aktive ID-ene; flere sokler og ulike cachegrupper behandles separat. Cachevarianter og antall bygger på sysfs-identitet og deling, og ukjent antall vises som ukjent. Temperatur bruker sensorens navn; L3-grupper kalles ikke automatisk CCX.
- **GPU etter funksjonsstøtte:** Klokker, effekt og vifter bruker valgt korts tilgjengelige felt og rapporterte grenser. Viftevisningen skiller ukjent antall, null vifter og én til flere vifter, uten å finne på et kort med to vifter. Kontroller og kurvevalg følger det rapporterte antallet og støtten.
- **Versjonslogg og bidrag:** Innstillinger og Om åpner en innebygd logg med de ni dokumenterte desktoputgavene, nyeste først. Om krediterer Julian, Anthropic Claude Opus 5.5 (Claude Code) og OpenAI GPT 6.1 Sol (Codex).

GPU-backenden er NVIDIA NVML; AMD-/Intel-GPU-backender er ikke implementert. CPU-, RAM-, hovedkort- og diskdata kommer fra tilgjengelige `/proc`-, sysfs/DMI-, hwmon- og UDisks2-kilder. Manglende data og funksjoner erstattes ikke med spesifikasjoner fra utviklingsmaskinen. Dette er generell håndtering av maskinvaredata, ikke fysisk verifikasjon av alle komponenter og kort.

Utviklet av Julian / bejulian1, med AI-assistanse fra **Anthropic Claude Opus 5.5 (Claude Code)** og **OpenAI GPT 6.1 Sol (Codex)**.

Vifter har Enkel/Avansert visning: Auto og Manuell står først, mens kurve og fininnstillinger samles under Avansert. En aktiv kurve fortsetter uavhengig av visningsvalget. På Vifter og Klokker og effekt ligger Behold/Rull tilbake utenfor den rullende delen, så bekreftelsen fortsatt er synlig etter Bruk. Klokker og effekt følger samme funksjonsrekkefølge: målinger og vanlige justeringer først, avanserte valg under.

| Del | Betydning |
|---|---|
| GPU-klokke nå / Minneklokke nå | Faktisk målt klokke; maksimum ved siden av er rapportert av NVIDIA-driveren |
| GPU-justering / Minnejustering | Offset i MHz. 0 betyr ingen justering, ikke at klokken er 0 MHz |
| Effektgrense | Både W og prosent av kortets standard |
| Avansert · temperaturmål og vifter | Temperaturmål, viftevalg og lenken Åpne Vifter |
| Avansert · kortets grenser og funksjoner | Rapporterbare grenser og forklaringer på utilgjengelig støtte |

Rapportert maksimum og referanseverdier er separate fra klokken som måles nå. Boost, temperatur og effekt påvirker den faktiske hastigheten; appen beregner ingen fremtidig sluttklokke fra maksimum pluss justering. Minnejusteringen er et offset, ikke en fast minnefrekvens eller en båndbredde i Gb/s. Hjelp følger faktisk styringsstatus; endret forklaring i hjelp endrer ikke NVML-omregningen.

Minimumsbygget for GPU-hjelperen er eksplisitt; eldre bygg, ukjent nyere hjelper og annen protokoll avvises. Rene UI-utgaver kan beholde en allerede autorisert hjelper (som bygg 23 med app/daemon bygg 24). Ved sikkerhetsrettinger økes minimumsbygget og ved behov protokollen. 1.6.0 (bygg 32) retter sikkerhetsmotoren i hjelperen, så minimumsbygget er 32 med uendret protokoll 2: en installert hjelper fra bygg 25–31 vises som utdatert og oppdateres én gang med administratorpassordet fra Innstillinger › Skjermkortstyring.

**Viftestopp / Zero RPM:** På det undersøkte kortet rapporterer driveren 35–100 % som manuelt innstillingsområde. Det vises som «Minst 35 % (driver)». Dette avgjør ikke om automatisk styring kan stoppe viftene: NVML-funksjonen beskriver grensene brukeren kan sette. Automatisk Zero RPM er fortsatt ubekreftet, og appen har ikke fått en slik funksjon eller prøvd 0 %. Grenser og backend er uendret. Se [NVIDIAs NVML-referanse](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html). [Forumtråden om minimumshastighet](https://forums.developer.nvidia.com/t/any-workaround-to-the-30-fan-speed-limitation-on-newer-nvidia-gpus/291486) inneholder brukererfaringer med forskjellige kort/drivere; disse er ikke bevis for dette kortets støtte.

**Sluttkontroll for 1.4.1:** 410 tester bestått, 4 ignorert og ingen feil; ren workspace-clippy. Private GTK-planer bestod for versjonsloggens to innganger og rekkefølge, fast Behold etter rulling, ukjent/0/1/3 vifter og smal visning, samt Prosessor, System og Disker i lyst/mørkt. Manuell 55 % forble bekreftet etter 71 s i simulering. Produksjonsbinærene oppgir 1.4.1 bygg 24. Ingen nye GPU-skriveprøver er utført; HW-sikkerhetsmotoren er uendret.

## Data fra maskinen og støtteomfang

Appen samler CPU-identitet og last fra `/proc/cpuinfo` og `/proc/stat`, tilgjengelige klokke-/cacheopplysninger fra sysfs og sensorer fra hwmon. RAM-bruk kommer fra `/proc/meminfo`; hovedkort og BIOS fra lesbare DMI-felt. Disk-I/O kommer fra `/proc/diskstats`, mens tilgjengelig SMART-/NVMe-helse leses gjennom UDisks2. Disse kildene beskriver den aktuelle maskinen og er ikke et fast Ryzen-/RTX 2080-oppsett.

Dette gir ikke automatisk alle spesifikasjoner: RAM-modulenes EEPROM/timinger leses ikke, root-beskyttede DMI-felt åpnes ikke, og sensorer avhenger av drivere og hva kjernen eksponerer. Hovedkortets modellnavn betyr ikke at appen kan styre hovedkortvifter eller RGB. Kildemerkede referansetabeller må holdes atskilt fra avleste verdier; en ukjent modell skal ikke få RTX 2080-spesifikasjoner som erstatning.

GPU-overvåking og -styring har foreløpig NVIDIA NVML-backend. AMD-/Intel-GPU-backender er ikke implementert. Styringssider og kommende utvidelser skal følge valgt korts faktiske funksjoner, områder og enheter. Manglende støtte på ett felt skal forklares for det feltet. Det er ingen påstand om at alle NVIDIA-modeller, CPU-er, RAM-oppsett eller hovedkort er fysisk testet.

## Bygge

Det trengs Rust 1.92 eller nyere og utviklingspakkene for GTK 4.22 (`libgtk-4-dev`). NVIDIA-driveren gir `libnvidia-ml.so.1`, som tjenesten laster ved kjøring; den trengs ikke for å bygge.

```sh
cd app
CARGO_TARGET_DIR=~/.cache/rigg-build cargo build --release
```

Binærene havner i `~/.cache/rigg-build/release/`: `agentic-rigg` (vinduet), `agentic-riggd` (tjenesten) og `agentic-rigg-helper` (hjelperen for endring, se under).

Tester og lint:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Installere

```sh
./install.sh                    # bruker ~/.cache/rigg-build/release
./install.sh --bin-dir STI      # eller binærene et annet sted
```

Skriptet installerer bare for deg, under `~/.local`, og bruker aldri `sudo`:

| Hva | Hvor |
|---|---|
| Binærene | `~/.local/bin/agentic-rigg` og `~/.local/bin/agentic-riggd` |
| Programmet i «Vis programmer» | `~/.local/share/applications/io.github.AgenticRigg.desktop` |
| Ikonene (alle variantene) | `~/.local/share/icons/hicolor/…/apps/io.github.AgenticRigg*.svg` |
| Tjenesten (systemd --user) | `~/.config/systemd/user/agentic-riggd.service`, ikke slått på |
| Denne fila og lisensene | `~/.local/share/doc/agentic-rigg/` (`README.md`, `LISENSER.md`) |
| Filene til hjelperen for endring | `~/.local/share/agentic-rigg/hjelper/` (binæren, tre skript, systemd-enheten og polkit-regelen), ikke aktivert |

Tjenesten slås ikke på ved innlogging. Vinduet starter den selv når du åpner appen. «Start tjenesten ved innlogging» står i Innstillinger › Generelt › Bakgrunn.

Kjører du `install.sh` på nytt med nye binærer, startes en tjeneste som går, på nytt.

## Avinstallere

```sh
./uninstall.sh
```

Skriptet stopper og fjerner tjenesten, binærene, `.desktop`-fila, ikonene, dokumentasjonen og filene til hjelperen i `~/.local/share/agentic-rigg/hjelper/`. Har du slått på «Tillat endring av skjermkortet», slår du den av først i Innstillinger › Skjermkortstyring: hjelperen ligger da utenfor `~/.local`, og bare du kan fjerne den, med passordet ditt. Er den fortsatt på, sier `uninstall.sh` fra og viser hvordan den slås av.

Innstillingene og historikken blir liggende, og skriptet skriver ut hvor de ligger:

- `~/.config/agentic-rigg/`: `ui.json` (vinduet), `tjeneste.json` (tjenesten) og `profiler.json` (utkast, profiler og lysvalg).
- `~/.local/state/agentic-rigg/`: historikken for grafene (48 t) og min og maks.

Logger du til fil, havner CSV-filene i `~/Dokumenter/Agentic Rigg/` eller mappa du har valgt. De røres aldri av skriptene.

## Slik henger det sammen

Appen er to prosesser, og en tredje bare hvis du slår på endring:

- **Tjenesten** (`agentic-riggd`) leser kortet med NVML og maskinen fra `/proc` og `/sys`, og tar vare på historikken, min og maks, logging til fil og varsler. Den kjører som systemd `--user`-enhet (eller som vanlig prosess hvis systemd ikke svarer), uten root. Den snakker med vinduet over en Unix-socket i `$XDG_RUNTIME_DIR/agentic-rigg/` som bare du kan åpne.
- **Vinduet** (`agentic-rigg`) laster aldri NVML. Det spør tjenesten og tegner. Lukker du vinduet (×, Ctrl+W eller Ctrl+Q), avslutter prosessen straks.
- **Hjelperen** (`agentic-rigg-helper`) kjører bare når «Tillat endring av skjermkortet» er på. Tjenesten sender endringene dit; vinduet snakker aldri med den. Se «Endring av skjermkortet» under.

Hva som skjer når vinduet er lukket, bestemmer du i Innstillinger › Generelt › Bakgrunn:

- **«Fortsett i bakgrunnen» av** (standard): ingenting kjører når appen er lukket. Tjenesten stopper 60 s etter siste vindu, og grafene får et hull for den tiden.
- **På:** historikk, min og maks og varsler fortsetter til du logger ut, med «Takt i bakgrunnen» 1, 2 eller 5 s. Med 2 s koster det ca. 21–28 MB minne og 0,85 % av én kjerne. Et ikon i systemfeltet viser at tjenesten går, med «Åpne Agentic HW Monitor», «Start logging» / «Stopp logging» og «Stopp bakgrunnsopptak».
- En logg til fil holder alltid tjenesten i gang til loggen stoppes.
- **«Stopp tjenesten nå»** stopper loggen og opptaket; tjenesten avslutter når vinduet lukkes.

Målt med kortet i ro (RTX 2080, P8) i 1.2.0: kald start til første bilde 182 ms, vinduet 12–17 MB uten 3D og under 2 % av én kjerne i ro på alle sidene utenom animasjonen på RGB-lys. I 1.3.0 er det målt mens andre programmer holdt kortet i P0: kald start 236 ms, og Oversikt 2,3–2,5 % (1.2.0 i samme forhold: 5–8,5 %). Tallene og hvordan de er målt står i `docs/produksjon/maalinger.md`.

## Personvern

Alt blir på maskinen. Appen sender ingenting noe sted: ikke tall, ikke rapporter, ikke feilmeldinger. Den eneste nettkontakten er når du trykker «Installer systemkomponenter»: da laster administratorinstallasjonen ned denne versjonen fra GitHub. Rapporter og logger lagres bare der du ber om det. Serienumre og UUID er skjult i rapporten til du slår dem på selv.

## Endring av skjermkortet

Agentic HW Monitor overvåker uten administratorrettigheter. I 1.4.0 er NVML-styring implementert for effektgrense, kjerne-/minneoffset og GPU-vifter, med egen støttekontroll for temperaturmål. **Ekte polkit-aktivering og begrenset fysisk skriving gjennom hjelperen er nå verifisert på RTX 2080.** Installasjon og oppdagelse prøver ingen endringer automatisk.

**Hvilket kort styres?** Skjermkortstyring i sidefeltet viser kortet under tittelen. «Styrer»-linja viser navn, produsent, PCI-adresse, VBIOS og driver. Hjelperen bindes til det valgte kortets PCI-adresse og NVML-UUID; hvert skrivekall kontrollerer samme identitet. Den åpner aldri ubundet kortindeks 0. Et annet valgt kort får ikke bruke bindingen. Et produsentnavn du setter selv er bare en etikett og påvirker ikke bindingen eller funksjonsstøtten.

**Én aktivering:** Gå til Innstillinger › Skjermkortstyring og slå på **«Tillat endring av skjermkortet»**. Brukeren skriver administratorpassord i polkit-dialogen. Hjelperen installeres i `/usr/local/libexec/`, oppsettet i `/etc/agentic-rigg/helper.json`, og en herdet systemtjeneste startes. Fire separate godkjenningstrinn brukes ikke lenger. Et eldre trinnoppsett gir ikke automatisk global tillatelse og må aktiveres på nytt. `install.sh` legger bare hjelpefilene under `~/.local/share/agentic-rigg/hjelper/` og aktiverer dem ikke.

**Støtte per funksjon:** Innstillinger viser effekt, kjerneoffset, minneoffset, manuell vifte, viftekurve, og temperaturmål separat. RGB har egen tillatelse på siden RGB-lys. Manglende temperaturmål sperrer for eksempel ikke effektgrensen. Områdene hentes fra kortet; tall utenfor avvises fremfor å klemmes. Tilgjengelighet betyr at nødvendige gettere og setter-symboler finnes, ikke at driveren har godkjent et skriveforsøk. «Klar» krever at hjelperens backend og kortidentitet er klare; en socket alene er ikke nok.

**Bruk og Behold:** Et utkast blir sendt som bare de endrede feltene når du trykker Bruk. Behold innen fristen (15 s som standard), ellers forsøkes det bekreftede oppsettet gjenopprettet. Frist og eierskap håndteres i hjelperen. Tjenesten holdes i live mens den eier aktive GPU-innstillinger, også om vinduet lukkes. Manglende livssignal, frakobling, hvilegap og stopp utløser gjenoppretting etter egne endringer.

**Lagret utgangspunkt:** Før skriving lagres kortets fangede innstillinger atomisk under `/var/lib/agentic-rigg/`, bundet til PCI/UUID, driver og VBIOS. «Standard» gjenoppretter disse opprinnelige verdiene, også eventuelle eksisterende effektgrenser og klokkeoffset. Det er ikke en antatt fabrikkverdi. Ny fangst krever Auto-vifte; tidligere fangst kan brukes under recovery mens våre egne manuelle verdier er aktive. Vanlig oppstart og recovery bevarer opptaket. En ren ny administratoraktivering arkiverer først det gamle opptaket etter bekreftet gjenoppretting og fravær av uavklarte endringer, slik at dagens utgangspunkt fanges på nytt.

**Feil vises:** En delvis endring forsøkes først gjenopprettet til bekreftet oppsett og deretter til det fangede utgangspunktet. Endringsmerkøren beholdes og videre styring sperres når tilstanden er ukjent. Gjenoppretting godtas først når den kan bekreftes. Klokkegettere kan returnere gamle offset; et setter-svar regnes derfor ikke alene som fysisk tilbakelesing. Slår du bryteren av, fjernes ikke hjelperen hvis gjenoppretting feiler eller er uverifisert. Grunnverdifila beholdes også etter vellykket deaktivering.

**Gjennomført maskinvareprøve 14. november 2026:** Brukeren fullførte polkit-dialogen, og den herdede root-hjelperen meldte klar NVML-backend. Følgende ble prøvd enkeltvis mot hjelperen med fersk NVML-getter og revert etter hvert felt:

| Felt | Prøvd endring |
|---|---|
| Effektgrense | 245 → 240 W |
| Begge GPU-vifter | Auto (35 %) → Manuell (50 %) |
| Kjerneoffset | 0 → −15 MHz |
| Minneoffset | 0 → −10 MHz |
| Viftekurve | Flat 50 %-kurve, begge vifter på målet i 4 s |
| Temperaturmål | 83 → 82 °C |

Til slutt bekreftet reset 245 W, offset 0/0 MHz, temperaturmål 83 °C og Auto på begge viftene. Temperaturmålet ble først bare lest; en senere enkeltprøve verifiserte også 83 → 82 °C med revert og reset. En flat 50 %-kurve holdt begge vifter på målet i 4 s; avsluttende reset bekreftet igjen utgangspunktet. Appen 1.4.0 · bygg 23 er installert; binærhash og uendrede innstillinger er kontrollert, og historikken er sikret. En etterfølgende prøve gikk gjennom den installerte NVML-daemonen: `hello`, `get_helper` og `get_state` bekreftet kortbindingen; 240 W ble brukt og beholdt med `confirm`, lest tilbake og resatt til 245 W. En ny 240 W-endring med 10 s frist uten Behold ga en autoritativ `rolled_back`/`timeout`-hendelse, og fersk NVML-getter viste 245 W før noe ekstra reset-kall. Avsluttende reset bekreftet `stock: true`. Dette verifiserer daemonvideresending og transaksjonsflyt for effektgrensen, ikke alle GUI-handlinger mot fysisk GPU. Alle grenser, positiv overklokking, stresstest, drivertap og strømbrudd er heller ikke prøvd.

**Sluttkontroll for 1.4.0:** 388 bestått, 4 ignorert og 0 feilet; workspace-clippy på alle mål er ren. Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.

**Viftevern:** Nødregelen over 85 °C bruker bare faktisk tilgjengelig viftestyring og slipper under 80 °C. Temperaturbortfall under Manuell utløser gjenoppretting. Kurvemotoren har utjevning, hysterese, rampe og feilhåndtering; feil ved skriving meldes videre og behandles som gjenopprettingsbehov. Dette er implementert og dekket av sim-/mocktester, men ikke bekreftet på fysisk GPU ennå.

**RGB på kortet:** Panelet «Lys på kortet» har egne kontroller for én fysisk sone, fast farge/av, lysstyrke og programvaredrevne effekter når hjelperen støtter dem. Det krever separat administratoraktivering, identifisert Palit 100/150-kontroller og et visuelt bekreftet statisk utgangspunkt. Bruk sender en tidsbegrenset prøve. Behold før fristen hvis kortet viser det forventede lyset. NVML-tillatelsen gir ikke RGB-tilgang. Betjeningen er prøvd i GTK mot en falsk RGB-arbeider. På brukerens Palit RTX 2080 Super JetStream er fast blått og automatisk tilbakeføring til grønt fysisk bekreftet ved 100 % lysstyrke. [Status og begrensninger](../docs/produksjon/rgb-hjelper.md).

Detaljene, gjenværende begrensninger og den historiske sikkerhetsgjennomgangen står i `docs/produksjon/hjelper-sikkerhet.md`.

GUI og daemon åpner ikke `/dev/i2c-*` eller `/dev/sd*`. Den separate RGB-root-hjelperen kan etter egen aktivering åpne bare det valgte kortets ferskt identifiserte NVIDIA-port 1 og den gjennomgåtte Palit-adressen. Ingen skanning eller annen I2C-enhet inngår. Diskene leses bare gjennom UDisks2 (uten root) og `/proc/diskstats`, og appen starter aldri en SMART-selvtest.

## Kjente begrensninger

- **Bare NVIDIA.** Kortet leses med NVML. AMD og Intel er ikke støttet ennå.
- **Det NVML ikke kan på Turing:** hotspot- og minnetemperatur, kjernespenning, spennings- og klokkekurve og årsakene til nedklokking i P8 vises som «Ikke støttet» med forklaring.
- **Hovedkortsensorer** (`nct6775` o.l.) vises bare hvis kjernemodulen er lastet. Appen laster ingen moduler.
- **Systemdata som krever root** (multiplikator, pakkeeffekt, serienumre i DMI, SPD på minnet) leses ikke. GPU-hjelperen utvider ikke tilgangen til CPU, RAM eller hovedkort.
- **Systemfeltet** krever AppIndicator-utvidelsen, som Ubuntu har på som standard. I ren GNOME vises ikke ikonet; appen virker likt uten.
- **Wayland:** vindusposisjonen huskes ikke, bare størrelsen.
- **Ett kort om gangen** vises i sidene; flere kort velges i kortvelgeren øverst i sidefeltet eller i «Styrer»-linja.

## Lisens

Agentic HW Monitor er lisensiert under MIT. Ikonene er fra Lucide (ISC). Lisensene for ikonene og Rust-bibliotekene står i `data/LISENSER.md`, i Innstillinger › Om › Lisenser og i `~/.local/share/doc/agentic-rigg/LISENSER.md` etter installasjon.
