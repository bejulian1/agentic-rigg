# Agentic Rigg

Agentic Rigg er et overvåkingsverktøy for skjermkort og maskin på Ubuntu, skrevet i Rust med GTK 4. Det samler det du ellers bruker MSI Afterburner, HWiNFO, GPU-Z og CPU-Z til på Windows:

- **Oversikt** over hele maskinen, i stil med GNOME Resources: fliser for prosessor, skjermkort, minne, VRAM, disker og nettverk, en tilstand-linje når noe er verdt å se (nedklokking, varm GPU, disk med advarsel, kortet borte), grafer for last, temperaturer, effekt, minne, disk og nett fra siste minutt til 48 timer, og skjermkortets målere nederst.
- **Sensorer**: alle verdier i én tabell, med min, maks og snitt, sparklinjer og logging til CSV.
- **Grafer**: opptil elleve grafer med felles markør, pause og CSV-eksport.
- **Grafikkort**, **Prosessor**, **Disker** og **System**: alt NVML, sysfs, `/proc` og UDisks2 kan si om kortet, prosessoren (klokke og last per tråd, cache, sårbarheter), diskene (SMART-helse, temperatur, MB/s, partisjoner, som CrystalDiskInfo) og maskinen, og rapport som tekst eller Markdown.
- **Skjermkortstyring:** **Klokker og effekt** (effektgrense, klokke-offset, temperaturmål, vifte), **Vifter** (viftekurve som i Afterburner, med egen kurve per vifte, hysterese, rampe, utjevning og «hold minimum til») og **RGB-lys**. Klokker og Vifter viser målte GPU-data; RGB er en forhåndsvisning. NVML-styring er implementert for funksjonene kortet støtter, etter én administratoraktivering (se «Endring av skjermkortet» under). Begrenset fysisk NVML-skriving og gjenoppretting er verifisert mot hjelperen på RTX 2080; RGB sendes ikke til kortet.

En allerede åpen app følger nå `--innstillinger DEL`. Hjelperstatus sjekkes også mens styring er låst, slik at ny administratoraktivering oppdages uten å starte vinduet på nytt.

Sidefeltet har et maskinhode med skjermkortet og prosessoren, og tastene 1–9 og 0 går til de ti sidene. Klikk på skjermkortet øverst åpner Grafikkort direkte. Kortet under Skjermkortstyring åpner en smal detaljmeny i sidefeltets bredde, med kortvalg når det finnes flere kort. Ikonene står sentrert også når sidefeltet er lagt sammen. Versjon og byggnummer står i Innstillinger › Om, sammen med signaturen Code Pro. Grafene kutter aldri en verdi: aksen utvides når temperaturen går over 100 °C eller under 20 °C, og krymper først når kurven har roet seg. Denne fila gjelder 1.4.0 · bygg 23. Sensorfilteret slipper fokus ved klikk utenfor og ved sideskifte; Esc tømmer/avslutter søket. Nedtrekksmenyene har tydelige tilstander for valgt rad og tastaturfokus. PCIe-kanten i kortillustrasjonen deler riktige x16-kontaktmål mellom 2D og 3D; modellen er fortsatt en illustrasjon.

Prosessor viser også lanseringsdato når den konkrete modellen er i det lokale oppslaget. Ryzen 7 2700X viser 19. april 2018, med AMD som kilde. Ukjente modeller får ingen antatt dato. Gjennomgangen av øvrige spesifikasjoner og mangler står i `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`.

Måleaksene utvides også for 110 °C og klokker over kortets oppgitte maksimum. Ferske topper i langtidsvinduer og viftekurvens målte punkter tas med; lovlige redigeringsgrenser og 0–100 % for last og brukt minne beholdes.

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
- **På:** historikk, min og maks og varsler fortsetter til du logger ut, med «Takt i bakgrunnen» 1, 2 eller 5 s. Med 2 s koster det ca. 21–28 MB minne og 0,85 % av én kjerne. Et ikon i systemfeltet viser at tjenesten går, med «Åpne Agentic Rigg», «Start logging» / «Stopp logging» og «Stopp bakgrunnsopptak».
- En logg til fil holder alltid tjenesten i gang til loggen stoppes.
- **«Stopp tjenesten nå»** stopper loggen og opptaket; tjenesten avslutter når vinduet lukkes.

Målt med kortet i ro (RTX 2080, P8) i 1.2.0: kald start til første bilde 182 ms, vinduet 12–17 MB uten 3D og under 2 % av én kjerne i ro på alle sidene utenom animasjonen på RGB-lys. I 1.3.0 er det målt mens andre programmer holdt kortet i P0: kald start 236 ms, og Oversikt 2,3–2,5 % (1.2.0 i samme forhold: 5–8,5 %). Tallene og hvordan de er målt står i `docs/produksjon/maalinger.md`.

## Personvern

Alt blir på maskinen. Appen har ingen nettverkskode, laster ikke ned noe og sender ingenting noe sted: ikke tall, ikke rapporter, ikke feilmeldinger. Rapporter og logger lagres bare der du ber om det. Serienumre og UUID er skjult i rapporten til du slår dem på selv.

## Endring av skjermkortet

Agentic Rigg overvåker uten administratorrettigheter. I 1.4.0 er NVML-styring implementert for effektgrense, kjerne-/minneoffset og GPU-vifter, med egen støttekontroll for temperaturmål. **Ekte polkit-aktivering og begrenset fysisk skriving gjennom hjelperen er nå verifisert på RTX 2080.** Installasjon og oppdagelse prøver ingen endringer automatisk.

**Hvilket kort styres?** Skjermkortstyring i sidefeltet viser kortet under tittelen. «Styrer»-linja viser navn, produsent, PCI-adresse, VBIOS og driver. Hjelperen bindes til det valgte kortets PCI-adresse og NVML-UUID; hvert skrivekall kontrollerer samme identitet. Den åpner aldri ubundet kortindeks 0. Et annet valgt kort får ikke bruke bindingen. Et produsentnavn du setter selv er bare en etikett og påvirker ikke bindingen eller funksjonsstøtten.

**Én aktivering:** Gå til Innstillinger › Skjermkortstyring og slå på **«Tillat endring av skjermkortet»**. Brukeren skriver administratorpassord i polkit-dialogen. Hjelperen installeres i `/usr/local/libexec/`, oppsettet i `/etc/agentic-rigg/helper.json`, og en herdet systemtjeneste startes. Fire separate godkjenningstrinn brukes ikke lenger. Et eldre trinnoppsett gir ikke automatisk global tillatelse og må aktiveres på nytt. `install.sh` legger bare hjelpefilene under `~/.local/share/agentic-rigg/hjelper/` og aktiverer dem ikke.

**Støtte per funksjon:** Innstillinger viser effekt, kjerneoffset, minneoffset, manuell vifte, viftekurve, temperaturmål og RGB separat. Manglende temperaturmål sperrer for eksempel ikke effektgrensen. Områdene hentes fra kortet; tall utenfor avvises fremfor å klemmes. Tilgjengelighet betyr at nødvendige gettere og setter-symboler finnes, ikke at driveren har godkjent et skriveforsøk. «Klar» krever at hjelperens backend og kortidentitet er klare; en socket alene er ikke nok.

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

**Sluttkontroll:** 388 bestått, 4 ignorert og 0 feilet; workspace-clippy på alle mål er ren. Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.

**Viftevern:** Nødregelen over 85 °C bruker bare faktisk tilgjengelig viftestyring og slipper under 80 °C. Temperaturbortfall under Manuell utløser gjenoppretting. Kurvemotoren har utjevning, hysterese, rampe og feilhåndtering; feil ved skriving meldes videre og behandles som gjenopprettingsbehov. Dette er implementert og dekket av sim-/mocktester, men ikke bekreftet på fysisk GPU ennå.

**RGB forblir forhåndsvisning:** Lyskontrolleren er ikke identifisert. Lysvalg kan lagres og vises, men sendes ikke til kortet selv om NVML-styring er klar. Ingen kontrollerstøtte utledes bare fra Palit-navnet eller PCI-ID.

Detaljene, gjenværende begrensninger og den historiske sikkerhetsgjennomgangen står i `docs/produksjon/hjelper-sikkerhet.md`.

Appen åpner aldri `/dev/i2c-*` eller `/dev/sd*`. Lyskontrolleren på kortet leses derfor ikke, og RGB-lys er en forhåndsvisning. Diskene leses bare gjennom UDisks2 (uten root) og `/proc/diskstats`, og appen starter aldri en SMART-selvtest.

## Kjente begrensninger

- **Bare NVIDIA.** Kortet leses med NVML. AMD og Intel er ikke støttet ennå.
- **Det NVML ikke kan på Turing:** hotspot- og minnetemperatur, kjernespenning, spennings- og klokkekurve og årsakene til nedklokking i P8 vises som «Ikke støttet» med forklaring.
- **Hovedkortsensorer** (`nct6775` o.l.) vises bare hvis kjernemodulen er lastet. Appen laster ingen moduler.
- **Systemdata som krever root** (multiplikator, pakkeeffekt, serienumre i DMI, SPD på minnet) leses ikke. GPU-hjelperen utvider ikke tilgangen til CPU, RAM eller hovedkort.
- **RGB-lys i 3D** bruker GL: første gang vekker det skjermkortet i ca. 4 s, og vinduet bruker ca. 120 MB mer minne. RGB-lys åpner derfor med 2D-kortet. Velger du 3D, huskes det (Innstillinger › Utseende › Lys › «3D-kortet»).
- **Systemfeltet** krever AppIndicator-utvidelsen, som Ubuntu har på som standard. I ren GNOME vises ikke ikonet; appen virker likt uten.
- **Wayland:** vindusposisjonen huskes ikke, bare størrelsen.
- **Ett kort om gangen** vises i sidene; flere kort velges i kortvelgeren øverst i sidefeltet eller i «Styrer»-linja.

## Lisens

Agentic Rigg er lisensiert under MIT. Ikonene er fra Lucide (ISC). Lisensene for ikonene og Rust-bibliotekene står i `data/LISENSER.md`, i Innstillinger › Om › Lisenser og i `~/.local/share/doc/agentic-rigg/LISENSER.md` etter installasjon.
