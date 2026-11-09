# Agentic Rigg

**1.3.0 · bygg 19** · Code Pro

Agentic Rigg er et raskt og lett verktøy for skjermkort og maskin på Ubuntu, skrevet i Rust og GTK 4. Det samler det du ellers bruker MSI Afterburner, HWiNFO, GPU-Z, CPU-Z og CrystalDiskInfo til på Windows, i én rolig og tett app i HWiNFO-stil.

Appen åpner på under 0,25 s, lukker på under 0,01 s og bruker rundt 15 MB minne og under 2 % av én kjerne når den står i ro. Den vekker ikke skjermkortet fra hvilemodus.

## Hva den viser

| Side | Innhold |
|---|---|
| **Oversikt** | Hele maskinen på ett sted: prosessor, skjermkort, minne, VRAM, disker og nettverk som fliser, en tilstandslinje når noe er verdt å se, og grafer fra siste minutt til 48 timer |
| **Sensorer** | Alle verdier i én tabell med min, maks og snitt, sparklinjer og logging til CSV |
| **Grafer** | Opptil elleve grafer med felles markør, pause og eksport |
| **Grafikkort** | Alt NVML vet om kortet (som GPU-Z): klokker, effekt, PCIe, VBIOS, prosesser |
| **Prosessor** | Klokke og last per tråd og kjerne, cache, instruksjonssett og sårbarheter (som CPU-Z) |
| **Disker** | SMART-helse, temperatur, driftstid, slitasje, MB/s og partisjoner (som CrystalDiskInfo), uten root |
| **System** | Hovedkort, BIOS, minne, operativsystem og kontrollere |
| **Skjermkortstyring** | Klokker og effekt, Vifter (viftekurve som i Afterburner, med hysterese, rampe og egen kurve per vifte) og RGB-lys |

Grafene kutter aldri en verdi: aksen utvides når temperaturen går over 100 °C eller under 20 °C.

## Installasjon

Klon repoet og kjør installasjonsskriptet:

```bash
git clone https://github.com/bejulian1/agentic-rigg.git
cd agentic-rigg
./install.sh
```

Skriptet sjekker at pakkene finnes, bygger appen og installerer den for deg under `~/.local`. Mangler noe, skriver det ut én `sudo apt install …`-linje du kan kjøre. Etterpå ligger Agentic Rigg i «Vis programmer».

Det som trengs for å bygge:

- Rust 1.92 eller nyere (`rustup`)
- `libgtk-4-dev` (GTK 4.22), `pkgconf` og `libglib2.0-dev-bin`
- NVIDIA-driveren for skjermkortdelen (ikke for å bygge). Uten den viser appen fortsatt prosessor, minne, disker og sensorer.

Skriptet bruker aldri `sudo` og slår ingenting på ved innlogging.

### Oppdatering

```bash
cd agentic-rigg
git pull
./install.sh
```

### Avinstallering

```bash
./uninstall.sh
```

Innstillinger og historikk blir liggende. Skriptet sier hvor de er.

## Endring av skjermkortet

Agentic Rigg leser bare, til du selv slår på endring i Innstillinger › Skjermkortstyring. Da legges en liten hjelper med root-rettigheter inn bak Ubuntus passorddialog, og hvert trinn (effekt, vifter, klokker, lys) må godkjennes for seg. I 1.3.0 er det ekte kortet fortsatt sperret: endringer blir et utkast i appen. Første trinn mot kortet (effektgrense) kommer i 1.4.0.

Hver endring rulles tilbake av seg selv hvis du ikke trykker Behold, viftene går til 100 % over 85 °C, og en omstart gir alltid kortets egne standardverdier tilbake. Detaljene står i [`app/README.md`](app/README.md) og [`docs/produksjon/hjelper-sikkerhet.md`](docs/produksjon/hjelper-sikkerhet.md).

## Personvern

Alt blir på maskinen. Appen har ingen nettverkskode og sender ingenting noe sted.

## Laget for

Utviklet og målt på Ubuntu med GNOME (Wayland), Palit GeForce RTX 2080, NVIDIA-driver 610 og AMD Ryzen 7 2700X. Den virker med andre NVIDIA-kort. AMD- og Intel-skjermkort er ikke støttet ennå.

## Mappene

| Mappe | Innhold |
|---|---|
| `app/` | Selve appen: Rust-arbeidsområdet (`crates/`), ikoner, temaer, hjelperfiler og skript |
| `app/crates/rigg` | Vinduet (GTK 4) |
| `app/crates/riggd` | Tjenesten som leser kortet og maskinen og holder historikken |
| `app/crates/rigg-helper` | Hjelperen for endring av kortet (av som standard) |
| `app/crates/rigg-core`, `rigg-hw` | Felles logikk, protokoll og maskinvarelesing |
| `docs/produksjon/` | Arkitektur, protokoll, målinger, sikkerhet, kritikkrunder og planer |
| `web/`, `server/`, `daemon-mock/` | Den første localhost-demoen (se [`docs/DEMO.md`](docs/DEMO.md)) |

Endringer per versjon står i [`app/PATCHLOG.md`](app/PATCHLOG.md), og hva som virker nå i [`app/STATUS.md`](app/STATUS.md).

## Utvikling

```bash
cd app
CARGO_TARGET_DIR=~/.cache/rigg-build cargo build --release --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Lisens

MIT. Ikonene er fra Lucide (ISC). Se [`app/data/LISENSER.md`](app/data/LISENSER.md).
