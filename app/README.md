# Agentic Rigg

Agentic Rigg er et overvåkingsverktøy for skjermkort og maskin på Ubuntu, skrevet i Rust med GTK 4. Det samler det du ellers bruker MSI Afterburner, HWiNFO, GPU-Z og CPU-Z til på Windows:

- **Oversikt** over hele maskinen, i stil med GNOME Resources: fliser for prosessor, skjermkort, minne, VRAM, disker og nettverk, en tilstand-linje når noe er verdt å se (nedklokking, varm GPU, disk med advarsel, kortet borte), grafer for last, temperaturer, effekt, minne, disk og nett fra siste minutt til 48 timer, og skjermkortets målere nederst.
- **Sensorer**: alle verdier i én tabell, med min, maks og snitt, sparklinjer og logging til CSV.
- **Grafer**: opptil elleve grafer med felles markør, pause og CSV-eksport.
- **Grafikkort**, **Prosessor**, **Disker** og **System**: alt NVML, sysfs, `/proc` og UDisks2 kan si om kortet, prosessoren (klokke og last per tråd, cache, sårbarheter), diskene (SMART-helse, temperatur, MB/s, partisjoner, som CrystalDiskInfo) og maskinen, og rapport som tekst eller Markdown.
- **Skjermkortstyring:** **Klokker og effekt** (effektgrense, klokke-offset, temperaturmål, vifte), **Vifter** (viftekurve som i Afterburner, med egen kurve per vifte, hysterese, rampe, utjevning og «hold minimum til») og **RGB-lys**. Klokker og Vifter viser målte GPU-data; RGB er en forhåndsvisning. Endringer blir utkast og profiler i appen og sendes ikke til kortet i denne versjonen (se «Endring av skjermkortet» under).

Sidefeltet har et maskinhode med skjermkortet og prosessoren, og tastene 1–9 og 0 går til de ti sidene. Klikk på skjermkortet øverst åpner Grafikkort direkte. Kortet under Skjermkortstyring åpner en smal detaljmeny i sidefeltets bredde, med kortvalg når det finnes flere kort. Ikonene står sentrert også når sidefeltet er lagt sammen. Versjon og byggnummer står i Innstillinger › Om, sammen med signaturen Code Pro. Grafene kutter aldri en verdi: aksen utvides når temperaturen går over 100 °C eller under 20 °C, og krymper først når kurven har roet seg. Denne fila gjelder 1.3.3 · bygg 22.

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

Agentic Rigg leser bare, til du selv slår på endring. I 1.3.0 endrer appen fortsatt ingenting på det ekte kortet.

**Hvilket kort styres?** Skjermkortstyring i sidefeltet viser kortet under tittelen, og øverst på Klokker og effekt, Vifter og RGB-lys står en «Styrer»-linje med navnet, produsenten, PCI-adressen, VBIOS og driveren, funnet av seg selv. Produsenten leses fra PCI-subsystemet når kortprodusenten har satt sin egen ID (for eksempel 1569 Palit, 1462 MSI, 1043 ASUS). Står NVIDIAs egen (10de), som på Palit RTX 2080 Rev. A, står det «NVIDIA-referansekort», og du kan sette produsenten selv med blyanten («Produsent (valgfritt)»). Den lagres per kort og vises bare i appen. Med flere kort velger du kortet i linja. Er kortet borte eller driveren mangler, sier linja det, og styringen er utilgjengelig.

**Hva er hjelperen?** For å endre effektgrense, vifter eller klokker trenger en Linux-app administratorrettigheter (root). Agentic Rigg ber ikke om det selv. Den har i stedet en liten hjelper (`agentic-rigg-helper`) som kjører i bakgrunnen med de rettighetene, og som bare kan gjøre fem faste ting med tall: lese status, legge på en endring, beholde den, rulle den tilbake og gå til standard. Den tar ikke imot kommandoer eller filstier, og bare brukeren din kan snakke med den.

**Den er av til du slår den på.** `install.sh` legger bare filene i `~/.local/share/agentic-rigg/hjelper/`. Ingenting kjører, og ingenting er installert utenfor hjemmemappa, før du slår på bryteren.

**Slik slår du den på og av:**

1. Gå til Innstillinger › Skjermkortstyring og slå på **«Tillat endring av skjermkortet»**.
2. Skriv passordet ditt i vinduet som kommer opp (polkit). Hjelperen legges da i `/usr/local/libexec/`, oppsettet i `/etc/agentic-rigg/helper.json`, og den startes som systemtjenesten `agentic-rigg-helper`. Statusen ved bryteren sier «På».
3. Slår du bryteren av (passordet igjen), stoppes hjelperen og fjernes helt, og kortet går tilbake til standard. Skal du avinstallere appen, slår du bryteren av først.

**Trinnene er av.** Bryteren gir ingen lov til å endre noe. Hvert trinn godkjennes for seg under bryteren, med et eget spørsmål og passordet ditt, i denne rekkefølgen: **Effekt** (effektgrense og temperaturmål), **Vifter** (modus, fast fart og viftekurve), **Klokker** (kjerne- og minneklokke-offset) og **RGB-lys**. Alle fire er av når bryteren slås på, og et trinn kan trekkes tilbake når som helst.

**Det ekte kortet er fortsatt sperret i 1.3.3.** Hjelperen er bygget og testet mot et simulert kort. Mot det ekte kortet kan trinnene ikke godkjennes ennå (de står som «Kommer senere»), og skrivekoden for kortet er ikke bygget inn i binæren. Klokker og effekt og Vifter viser målte GPU-data; RGB-lys er en forhåndsvisning. Det du endrer, blir et utkast i appen med «Tilbakestill» og Ctrl+Z / Ctrl+Shift+Z. Du kan lagre profiler, men ingenting sendes til kortet. Ekte styring krever en senere, separat implementasjon og kontroll på maskinvaren.

**Vernet som er prøvd i simulering:** Hver endring må bekreftes med **Behold** innen fristen (15 s som standard), ellers rulles den tilbake. Nødregelen setter viftene til 100 % over 85 °C, til kortet er under 80 °C. Frakobling og stopp skal gjenopprette standard med Auto. Dette må bekreftes på ekte maskinvare før styring aktiveres. Dette RTX 2080-kortet rapporterer 35–100 % som vifteområde; fysisk viftestopp er ikke prøvd her.

Detaljene og sikkerhetsgjennomgangen står i `docs/produksjon/hjelper-sikkerhet.md`.

Appen åpner aldri `/dev/i2c-*` eller `/dev/sd*`. Lyskontrolleren på kortet leses derfor ikke, og RGB-lys er en forhåndsvisning. Diskene leses bare gjennom UDisks2 (uten root) og `/proc/diskstats`, og appen starter aldri en SMART-selvtest.

## Kjente begrensninger

- **Bare NVIDIA.** Kortet leses med NVML. AMD og Intel er ikke støttet ennå.
- **Det NVML ikke kan på Turing:** hotspot- og minnetemperatur, kjernespenning, spennings- og klokkekurve og årsakene til nedklokking i P8 vises som «Ikke støttet» med forklaring.
- **Hovedkortsensorer** (`nct6775` o.l.) vises bare hvis kjernemodulen er lastet. Appen laster ingen moduler.
- **Det som krever root** (multiplikator, pakkeeffekt, serienumre i DMI, SPD på minnet) leses ikke.
- **RGB-lys i 3D** bruker GL: første gang vekker det skjermkortet i ca. 4 s, og vinduet bruker ca. 120 MB mer minne. RGB-lys åpner derfor med 2D-kortet. Velger du 3D, huskes det (Innstillinger › Utseende › Lys › «3D-kortet»).
- **Systemfeltet** krever AppIndicator-utvidelsen, som Ubuntu har på som standard. I ren GNOME vises ikke ikonet; appen virker likt uten.
- **Wayland:** vindusposisjonen huskes ikke, bare størrelsen.
- **Ett kort om gangen** vises i sidene; flere kort velges i kortvelgeren øverst i sidefeltet eller i «Styrer»-linja.

## Lisens

Agentic Rigg er lisensiert under MIT. Ikonene er fra Lucide (ISC). Lisensene for ikonene og Rust-bibliotekene står i `data/LISENSER.md`, i Innstillinger › Om › Lisenser og i `~/.local/share/doc/agentic-rigg/LISENSER.md` etter installasjon.
