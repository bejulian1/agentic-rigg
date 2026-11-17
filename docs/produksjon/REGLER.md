# Regler for produksjonsbygget av Agentic Rigg

Disse reglene gjelder alle agenter som jobber på den ekte desktop-appen. Les dem før du gjør noe.

## Mål

Agentic Rigg skal bli en ekte Ubuntu-desktop-app, skrevet i Rust med GTK4, ikke Electron. Den skal fungere som MSI Afterburner, HWiNFO, GPU-Z og CPU-Z, ikke som en demo.

- Appen skal åpne og lukke seg øyeblikkelig og bruke lite RAM.
- Den skal ligge i appmenyen («Vis programmer»).
- Alpha-merket skal fjernes, men versjon og byggnummer skal fortsatt stå i Innstillinger › Om.
- Localhost-demoen (`web/`, `server/`, `daemon-mock/`) er fasiten for design, ordlyd og oppførsel. Den skal ikke slettes eller flyttes, for brukeren kjører den fra disse stiene.

## Maskinen

| Del | Detaljer |
|---|---|
| Skjermkort | Palit RTX 2080 Rev. A (PCI-subsystemet melder NVIDIAs 10de:1e87). Driver 610.43.02, Wayland. |
| Prosessor | AMD Ryzen 7 2700X med 16 tråder (hwmon `k10temp`) |
| Minne og disk | 48 GB RAM. Ubuntu med GNOME. |
| Hovedkortsensorer | Ikke lastet (ingen `nct6775`). Ikke last kjernemoduler. |
| Skjerm | 1920×1080 på 240 Hz |

Biblioteket `libnvidia-ml.so.1` finnes. Dette er installert: Rust 1.93.1, GTK 4.22.4 og libadwaita 1.9.1.

## Sikkerhet

**Brukerbestilling 17. november 2026 – RGB:** Brukeren har nå bestilt undersøkelse og implementasjon av faktisk fargestyring for sitt **Palit GeForce RTX 2080 Super JetStream**, med høyere tilgang gjennom appen. Arbeidet gjøres på egen featurebranch og kan leveres som Draft PR, uten automatisk merge eller installasjon. Dette er den nye RGB-avklaringen som manglet i bestillingen 14. november: kortbundet RGB-implementasjon og nødvendige avgrensede prøver inngår, med eksplisitt administratoraktivering for privilegert tilgang. Det er ikke generell I2C-tilgang. Adapteren må identifiseres ved valgt GPU sin PCI/UUID, driver, portnavn og sysfs-sti; kontrollerprotokollen må identifiseres før skriving. Ingen bred skanning, alternative adresser ved feil, EEPROM-/BIOS-lagring, kjernemoduler, systempakker, USB-C/`ccgx-ucsi`, SPD eller andre enheter inngår. Lesende identifikasjon holdes atskilt fra fysisk fargeprøve. Utgangspunkt og gjenopprettingsmulighet må være avklart før første fargeendring; ukjent effektmodus eller utilstrekkelig tilbakelesing skal vises som ukjent. Den eksisterende GPU-autorisasjonen gir ikke automatisk en ny lagret RGB-tillatelse.

**Brukeravklaring 14. november 2026:** Brukeren har nå uttrykkelig bestilt ekte GPU-styring med én administratoraktivering: effektgrense, GPU-vifter og klokkeoffset. Dette erstatter den eldre arbeidsfasens absolutte skrivelås og fire separate brukertrinn nedenfor for nettopp denne leveransen. Implementasjon og kontrollerte prøver skal bruke den kortbundne hjelperen, persistente grunnverdier, funksjonsgrenser og bekreftet tilbakerulling. Administratorpassord skrives av brukeren i polkit-dialogen. Godkjenningen gjelder GPU-styring; CPU, hovedkortvifter, systempakker og kjernemoduler inngår ikke. I2C/RGB-skriving er fortsatt ikke godkjent av denne avklaringen. De øvrige reglene gjelder fortsatt. Punkt 1–3 nedenfor beskriver den tidligere fasens standardbegrensninger, som nå har dette uttrykkelige unntaket for Agentic Riggs GPU-hjelper og nødvendig appoppdatering/omstart.

Disse reglene kan ikke brytes.

1. **Skriv aldri til skjermkortet.** Det gjelder NVML-set-kall, `nvidia-smi` med set-flagg, I2C og alt annet som endrer klokker, effekt, vifte eller RGB. Skrivestien kan bygges og testes mot en simulert backend, men den skal være avslått og låst mot ekte maskinvare. Brukeren skal godkjenne hvert steg selv, senere. Ikke åpne noen `/dev/i2c-*` i det hele tatt. I2C-bussnumrene flytter seg mellom oppstarter: 7. november 2026 lå USB-C-kontrolleren (`ccgx-ucsi`, 0x08) på `i2c-10`, ikke på `i2c-8`, og `i2c-8` var hovedkortets SMBus. Kode som senere skal bruke I2C, må finne bussen ut fra navn og sysfs-sti, aldri ut fra nummer. Den må aldri røre `ccgx-ucsi` eller SPD-brikkene (`ee1004`). Å lese navnene i `/sys/bus/i2c/devices/*/name` er trygt.
2. **Ikke bruk sudo, ikke installer systempakker, ikke last kjernemoduler og ikke endre systemfiler.** Du kan installere for brukeren under `~/.local`, slik Klipp gjør. Systemd-tjenester kan bare være `--user`, og de skal ikke aktiveres for oppstart uten at hovedøkta sier fra.
3. **Ikke stopp eller start prosesser du ikke har startet selv.** Demoserveren på port 4318 (PID 33439/33453/33446), Klipp og alt annet brukeren kjører skal stå i fred.
4. **RAM.** Det skal alltid være minst 4 GB ledig (`MemAvailable` i `/proc/meminfo`). Sjekk før hver tung jobb, som cargo-bygg, headless Chrome eller tester. Er det under 6 GB, venter du og sjekker igjen; ikke start jobben.
   - Bygg med `CARGO_BUILD_JOBS=6` og den felles mappa `CARGO_TARGET_DIR=$HOME/.cache/rigg-build`.
   - Cargo låser mappa, så to agenter kan ikke bygge samtidig. Det er meningen.
5. **Ikke fyll disken.** Transcend-disken har bare cirka 28 GB ledig, så byggeutdata skal ligge i `~/.cache/rigg-build`, ikke i prosjektmappa.
6. **Lesing er trygt.** `nvidia-smi` med spørreflagg, NVML-get-kall, `/sys`, `/proc` og `lspci`/`lscpu`/`dmidecode` uten root er greit. Det samme gjelder `sensors`, hvis det finnes.

## Struktur

- Rust-arbeidsområdet ligger i `agentic-rigg/app/` (Cargo-workspace). Demoen ligger der den ligger.
- App-ID er `io.github.AgenticRigg`.
- All tekst i brukerflaten og alle dokumenter er på norsk bokmål.
- Dokumenter for produksjonsarbeidet ligger i `agentic-rigg/docs/produksjon/`.
- Hver agent oppdaterer `docs/produksjon/LOGG.md` med en kort oppføring om hva den har gjort.

## Kvalitet

- Brukeren vil ha profesjonell og gjennomtenkt brukerflate («detaljer er magien»). Den skal være tett og rolig, i HWiNFO-stil, uten gradienter og uten noe som ser «vibe coded» ut. Avslutt alltid med en egen finpussrunde.
- Mål det du påstår, for eksempel oppstartstid, RAM og CPU i ro, og skriv tallene i dokumentasjonen.
- Kjør `cargo clippy` og `cargo test` før du sier deg ferdig.
