# Agentic Rigg

**1.4.1 · bygg 24** · Rust og GTK 4 · Ubuntu · Code Pro

**Pågående RGB-arbeid på `feature/palit-rgb`:** Vi undersøker faktisk fargestyring for Palit GeForce RTX 2080 Super JetStream. [Kildegrunnlag og verifikasjon](docs/produksjon/palit-rgb-2026-11-17.md) beskriver kortbundet transport og en separat RGB-hjelper med privat journal, testet mot falske enheter. Aktivering fra appen, brukerflate og fysisk verifikasjon er ennå ikke ferdig eller utgitt.

Et lett maskinvareverktøy for Ubuntu: sensorer, grafer, CPU- og GPU-spesifikasjoner og diskhelse i én norsk desktop-app. Inspirert av MSI Afterburner, HWiNFO, GPU-Z, CPU-Z og CrystalDiskInfo.

[Last ned siste utgave](https://github.com/bejulian1/agentic-rigg/releases/latest) · [Versjonslogg](CHANGELOG.md) · [Bidragsytere](CONTRIBUTORS.md)

<table>
  <tr><th>Vifter</th><th>Klokker og effekt</th><th>RGB</th></tr>
  <tr>
    <td><a href="docs/images/vifter.png"><img src="docs/images/vifter.png" alt="Vifter med Enkel og Avansert visning" width="320"></a></td>
    <td><a href="docs/images/klokker.png"><img src="docs/images/klokker.png" alt="Klokker og effekt med målinger og GPU-justering" width="320"></a></td>
    <td><a href="docs/images/rgb.png"><img src="docs/images/rgb.png" alt="RGB-forhåndsvisning med PCIe x16-modell" width="320"></a></td>
  </tr>
</table>

*Vifter, Klokker og effekt og RGB fra private testøkter med simulerte data i GTK-appen 1.4.1. Klikk for full størrelse.*

## Nytt i 1.4.1

- Enkel/Avansert Vifter viser vanlige modusvalg først og samler kurver og fininnstilling under Avansert. En aktiv kurve fortsetter i Enkel visning.
- Behold/Rull tilbake er synlig også når siden er rullet ned, på både Vifter og Klokker og effekt.
- Klokker viser GPU-/minneklokke nå og rapportert maksimum, med egne GPU-/minnejusteringer der 0 betyr ingen justering. Effektgrensen vises i W og prosent; temperaturmål, vifter og grenser ligger under Avansert.
- Eksisterende GPU-autorisasjon fra hjelperbygg 23 beholdes med protokoll 2; denne UI-patchen trenger ikke ny aktivering.

**Sluttkontroll for 1.4.1:** 410 tester bestått, 4 ignorert og ingen feil; ren workspace-clippy. Private GTK-planer bestod for versjonsloggens to innganger og rekkefølge, fast Behold etter rulling, ukjent/0/1/3 vifter og smal visning, samt Prosessor, System og Disker i lyst/mørkt. Manuell 55 % forble bekreftet etter 71 s i simulering. Produksjonsbinærene oppgir 1.4.1 bygg 24. Ingen nye GPU-skriveprøver er utført; HW-sikkerhetsmotoren er uendret.

- **CPU på den faktiske maskinen:** Linux-CPU-ID-er beholdes ved hull i nummerrekken og hotplug. Prosessor, Sensorer og rapport bruker de aktive ID-ene; flere sokler og ulike cachegrupper behandles separat. Cachevarianter og antall bygger på sysfs-identitet og deling, og ukjent antall vises som ukjent. Temperatur bruker sensorens navn; L3-grupper kalles ikke automatisk CCX.
- **GPU etter funksjonsstøtte:** Klokker, effekt og vifter bruker valgt korts tilgjengelige felt og rapporterte grenser. Viftevisningen skiller ukjent antall, null vifter og én til flere vifter, uten å finne på et kort med to vifter. Kontroller og kurvevalg følger det rapporterte antallet og støtten.
- **Versjonslogg og bidrag:** Innstillinger og Om åpner en innebygd logg med de ni dokumenterte desktoputgavene, nyeste først. Om krediterer Julian, Anthropic Claude Opus 5.5 (Claude Code) og OpenAI GPT 6.1 Sol (Codex).

GPU-backenden er NVIDIA NVML; AMD-/Intel-GPU-backender er ikke implementert. CPU-, RAM-, hovedkort- og diskdata kommer fra tilgjengelige `/proc`-, sysfs/DMI-, hwmon- og UDisks2-kilder. Manglende data og funksjoner erstattes ikke med spesifikasjoner fra utviklingsmaskinen. Dette er generell håndtering av maskinvaredata, ikke fysisk verifikasjon av alle komponenter og kort.

Installasjon av 1.4.1 bygg 24 på utviklingsmaskinen er verifisert med samsvarende binærhash og uendrede innstillinger under installasjonen. Historikken er sikkerhetskopiert. Installert daemon bygg 24 leser den allerede godkjente hjelperen fra bygg 23 som klar, med riktig kortbinding og uten maskinvarefeil; oppstart ved innlogging er fortsatt av.

## Verifisert i 1.4.0

- Implementert NVML-styring av effekt, klokkeoffset og GPU-vifter med én administratoraktivering og separat støtte per felt.
- Kortbinding med PCI/UUID, lagret utgangspunkt, tilbakelesing og tilbakerulling som rapporterer feil ærlig.
- Korrigert PCIe-modell, bedre sensorfilter/fokus og tydeligere nedtrekksmenyer.

**Polkit-aktivering og begrenset GPU-skriving er verifisert på RTX 2080:** effekt 245 → 240 W, begge vifter til 50 %, kjerneoffset −15 MHz og minneoffset −10 MHz, med fersk NVML-tilbakelesing og gjenoppretting. Gjennom installert daemon er også 240 W + Behold, reset til 245 W og automatisk tilbakerulling etter 10 s uten Behold verifisert med fersk NVML-getter. En flat 50 %-viftekurve holdt begge vifter på målet i 4 s, og temperaturmål 83 → 82 °C med revert/reset er også verifisert. Alle grenser, positiv overklokking og stress-/feiltilfeller er ikke prøvd. Overvåking krever ingen administratoraktivering. Styring blir først tilgjengelig når den kortbundne hjelperen er klar; endringer krever Bruk og Behold. RGB er fortsatt en forhåndsvisning, og lyskontrolleren er ikke identifisert.

Sluttkontroll for 1.4.0: **388 bestått, 4 ignorert, 0 feil**, ren workspace-clippy og bestått native UI-plan. Installert app er kontrollert med klar hjelper og samsvarende kortidentitet.

## Hva appen viser

| Side | Innhold |
|---|---|
| Oversikt | CPU, GPU, RAM, VRAM, disker og nettverk; tilstand og grafer fra ett minutt til 48 timer |
| Sensorer | Måleverdier med min, maks og snitt, sparklinjer og CSV-logging |
| Grafer | Felles markør, pause, eksport og adaptive akser |
| Grafikkort | NVML-data: klokker, effekt, PCIe, VBIOS, vifter, grenser og prosesser; separat referansetabell |
| Prosessor | Klokke og last per tråd/kjerne, cache, instruksjonssett, sårbarheter og lanseringsdato for kjente modeller |
| Disker | SMART/NVMe-helse, temperatur, driftstid, slitasje, I/O og partisjoner |
| System | Hovedkort, BIOS, minnebruk, operativsystem og kontrollere |
| Skjermkortstyring | NVML-styring av støttede klokker, effekt og vifter etter aktivering; utkast/profiler og RGB-forhåndsvisning |

Appen leser maskinens tilgjengelige data fra `/proc`, sysfs/DMI og hwmon; diskhelse kommer fra UDisks2, og NVIDIA-GPU-data fra NVML. CPU-, RAM- og hovedkortvisning er ikke et fast oppsett for RTX 2080-maskinen. Videre utvidelser skal følge faktisk maskinvare, kilder og funksjonsstøtte, med manglende opplysninger vist som ukjente eller utilgjengelige. GPU-styring må bruke valgt korts identitet, tilgjengelige felt og rapporterte grenser. NVML-backenden gjelder NVIDIA; GPU-backender for AMD og Intel er ikke implementert. Tilgjengelige felt avhenger av komponentene og driverne. Referanseopplysninger er merket og skal ikke forveksles med målinger. RAM-moduldetaljer, skjerminformasjon og nettverkskortets spesifikasjoner er områder for videre utvidelse.

## Last ned og installer

Den ferdigbygde pakken ligger under [Releases](https://github.com/bejulian1/agentic-rigg/releases). Den er laget for **Ubuntu 26.04, amd64/x86-64, GTK ≥ 4.22 og glibc ≥ 2.39**. Eldre Ubuntu-utgaver er ikke bekreftet støttet.

Last ned `agentic-rigg-1.4.1-build24-ubuntu26.04-amd64.tar.gz` og `SHA256SUMS` fra samme Release:

```bash
sha256sum --check SHA256SUMS
tar -xzf agentic-rigg-1.4.1-build24-ubuntu26.04-amd64.tar.gz
cd agentic-rigg-1.4.1
./install.sh --bin-dir "$PWD/bin"
~/.local/bin/agentic-rigg
```

Installereren bruker ikke sudo. Den installerer for den innloggede brukeren under `~/.local`, legger appen i programmenyen og beholder innstillinger og historikk. Kjør som vanlig bruker. Automatisk oppstart ved innlogging og GPU-hjelperen aktiveres ikke av installasjonen.

GitHub Packages er et register for bestemte pakkeformater og containere. Denne desktop-utgaven distribueres som en nedlastbar pakke under Releases.

### Bygg fra kildekoden

Det trengs Rust ≥ 1.92, GTK-utviklingspakker ≥ 4.22, `pkgconf` og `libglib2.0-dev-bin`. NVIDIA-driveren trengs for GPU-data, men ikke for å bygge appen. Uten den fungerer CPU, minne og øvrige tilgjengelige systemdata. SMART bruker UDisks2.

```bash
git clone https://github.com/bejulian1/agentic-rigg.git
cd agentic-rigg
./install.sh
```

Mer om bygging og innstillinger står i [app/README.md](app/README.md).

### Oppdatering og avinstallering

Lukk appen før oppdatering. Pakk ut neste Release og kjør installereren på nytt. Fra en kildeklone: `git pull` og `./install.sh`.

Slå av en aktivert GPU-hjelper i Innstillinger › Skjermkortstyring før avinstallasjon. Egne GPU-endringer gjenopprettes til det lagrede utgangspunktet; ved uverifisert gjenoppretting beholdes hjelpefilene og feilen vises. Kjør deretter `./uninstall.sh` fra pakken eller repoet. Innstillinger og historikk beholdes.

## Personvern og støtte

Sensorer og historikk behandles lokalt. Appen sender ingen telemetri. GPU-overvåking bruker NVIDIA NVML; AMD- og Intel-GPU-er har ikke egen backend ennå. Utviklet på Ubuntu/GNOME/Wayland med RTX 2080 og Ryzen 7 2700X. Andre NVIDIA-kort kan ha andre tilgjengelige felt.

## Contributors

Utviklet av [Julian / bejulian1](https://github.com/bejulian1), med AI-assistanse fra **Anthropic Claude Opus 5.5 (Claude Code)** og **OpenAI GPT 6.1 Sol (Codex)**. Se [bidragsoversikten](CONTRIBUTORS.md) for roller og kreditering.

## Lisens

[MIT](LICENSE). Ikonene er fra Lucide (ISC), med enkelte Feather-elementer (MIT). Fullstendige merknader følger i [app/data/LISENSER.md](app/data/LISENSER.md) og i nedlastingspakken.
