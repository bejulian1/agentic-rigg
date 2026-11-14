# Komponentenes spesifikasjoner – gjennomgang 14. november 2026

## Vurdering

Appen viser mye om GPU, CPU og disker, men **ikke alle tilgjengelige spesifikasjoner**. RAM-moduler, skjerm og nettverkskort har størst forbedringspotensial. Flere fakta samles allerede inn uten å vises. Det finnes også tilfeller der en verdi feilaktig fremstilles som utilgjengelig eller fersk.

Gjennomgangen gjelder produksjonsappen i Rust/GTK. Tre Sol-agenter kartla GPU, CPU/RAM/hovedkort og disker/nett/skjerm; hovedøkta kontrollerte sentrale kodefunn og enkelte lesbare sysfs-verdier. Ingen maskinvareinnstillinger er endret. Ingen SPD-/I2C-enheter eller rå SMBIOS-tabeller er åpnet.

Etter brukerens tillegg er **CPU-lanseringsdato** lagt til for den konkrete, kildebekreftede modellen Ryzen 7 2700X i 1.3.3 · bygg 22. Resten nedenfor er funn og forslag, ikke ferdig implementerte utvidelser.

## Dekning og mangler

| Komponent | Allerede vist | Viktigste hull |
|---|---|---|
| GPU | Driver/VBIOS/NVML/CUDA, arkitektur, compute capability, CUDA-kjerner, minnebuss/VRAM, PCIe/BAR1/ReBAR, klokker og P-tilstander, grenser, vifter, prosesser og separat referansetabell | Skjulte innsamlede felt, feilstyrt «ikke støttet», foreldede felt og begrenset modelltabell |
| CPU | Modell/familie/stepping, mikrokode, kjerner/tråder, cache, instruksjonssett, sårbarheter, cpufreq, last/klokke/temperatur og AMD-kodenavn som oppslag | Lanseringsdato; allerede innsamlet cachelinjestørrelse/cache-deling og prosessorpakker; topologibegrensninger på andre maskiner |
| RAM | Brukbar total, brukt/tilgjengelig, cache/buffer og swap; antall oppdagede SPD-endepunkter | Ingen modulspesifikasjoner, DDR-type, kapasitet per modul, produsent/delenummer, ranks, profiler eller aktiv frekvens/kanaler/timinger |
| Hovedkort/BIOS | Produsent, modell/revisjon, BIOS-versjon/dato, systemmodell, kabinettype og PCI-avledet brikkesett | Noen innsamlede produkt-/kabinettfelt skjules; PCI-navnet identifiserer ikke nødvendigvis eksakt brikkesett |
| Disker | Modell, produsent, serienummer, fastvare, WWN, kapasitet, type, o/min, sektorstørrelser, scheduler, SMART/NVMe-helse, driftstid/slitasje/feil, partisjoner og I/O | Bedre skille mellom gjeldende og maksimal lenke, særlig NVMe/USB. Støtte varierer med disk og USB-bro |
| Nettverk | Grensesnitt, trafikkrater, opp/ned-status og Wi-Fi-markør; kontroller og driver finnes separat i PCI-tabellen | Kobling mellom grensesnitt og modell/driver; forhandlet lenkehastighet, duplex, MTU og relevante Wi-Fi-opplysninger |
| Skjerm | Ingen egen spesifikasjonsvisning | Modell, tilkobling, gjeldende modus/Hz, skalering og størrelse der økten oppgir det |
| Lyd/USB/andre kontrollere | Type, kontrollerens navn og driver | PCI-adresse og ID-er er innsamlet, men skjult. Kontrolleroversikten beskriver ikke nødvendigvis tilkoblet utstyr |

Dette er en vurdering av feltene og datakildene, ikke et løfte om at alle støttede disker eller GPU-er leverer alle feltene.

## Opplysninger som allerede finnes i datagrunnlaget

- **GPU:** `rigg-hw/src/gpu/nvml_info.rs` leser `boardId`, `computeMode` og `partNumber`. Gyldig delenummer vises ikke; bare feilstien håndteres i `rigg-core/src/report/gpu.rs`. `rigg-hw/src/sys/pci.rs` samler også PCI-strømtilstand, boot-VGA, BAR-størrelser og flere AER-feiltellere enn rapporten viser.
- **CPU:** `rigg-hw/src/sys/cpuinfo.rs` samler `lineSize`, `sharedBy`, `packages` og listen over governors. Cachetabellen i `rigg-core/src/report/system.rs` viser ikke alle disse detaljene.
- **Hovedkort:** `rigg-hw/src/sys/dmi.rs` samler produktversjon/-familie/SKU og kabinettprodusent/-versjon som ikke alle presenteres. Vis bare meningsfulle verdier; produsentens plassholdere er allerede filtrert.
- **Kontrollere:** `rigg-hw/src/sys/pci.rs::read_board` har `addr` og `ids`, mens `report/system.rs` viser type, navn og driver. En utvidbar detaljrad eller kopiering er passende for disse tekniske verdiene.

## Korrekthetsfunn før flere felt legges til

1. **F5 leser cache, ikke nødvendigvis maskinvaren på nytt.** `riggd/src/ops.rs::op_get_system` bygger CPU/DMI/OS/PCI én gang. `get_static` returnerer GPU-cache. UI sier likevel «Faste data er lest på nytt». Trenger en eksplisitt oppfriskingsoperasjon eller en mer presis tekst. Tidsavhengige snapshot-felt må ikke fremstilles som nye.
2. **GPU-målfart og viftepolicy kan være gamle.** `report/gpu.rs` bruker `extra.fans` selv om ferske `GpuSample.fan_target`/`fan_policy` finnes. Bruk den ferske prøven og statisk reserve ved behov.
3. **«Ikke støttet» er delvis hardkodet for Turing.** En vellykket `margin` blir ikke vist som verdi. Gyldige applications-/default-/customer-boost-klokker og delenummer mangler en tilsvarende suksessvisning. Utfallet på ett RTX 2080 må ikke brukes som fasit for alle GPU-er. Minne-maks-terskel er heller ikke en målt minnetemperatur.
4. **CPU-topologi er ikke generell nok.** `cpuinfo.rs` går gjennom `0..antall online` og estimerer cacheantall ut fra CPU0. Det kan bli feil ved hull i CPU-ID-er, hybride kjerner eller flere sokler. Flere L3-grupper kalles dessuten «CCX» uten å avgrense til relevante AMD-modeller. Dagens homogene 2700X med 0–15 er ikke bevis for andre oppsett.
5. **«Fysiske nettverkskort» avgjøres med navneprefikser.** `sensormeta.rs::is_physical_net` kan slippe gjennom broer/bond med andre navn og dobbelttelle trafikk. Fysisk/virtuell type og topologi bør komme fra innsamlingen.

Funnene er kildebaserte med konkrete kodebaner kontrollert. Det er ikke gjennomført nye feilinjeksjoner eller generell UI-test i denne revisjonen.

## Hva som kan utvides uten root

**Nettverk:** Lokal lesing bekrefter driver `igb` for Ethernet og `iwlwifi` for Wi-Fi, samt MTU 1500. Kablet grensesnitt var nede og rapporterte hastighet −1/duplex ukjent. Wi-Fi hadde ingen verdi i sysfs-filene `speed`/`duplex`. Disse skal vises som utilgjengelige, ikke som 0 Mbit/s. [Kjernens nettverks-ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-net) dokumenterer MTU, hastighet og duplex; Wi-Fi trenger en egnet egen kilde for lenkeegenskaper.

**Skjerm:** GTK/GDK tilbyr [monitoridentitet, kontakt og størrelse](https://docs.gtk.org/gdk4/class.Monitor.html) og [oppdateringsfrekvens når tilgjengelig](https://docs.gtk.org/gdk4/method.Monitor.get_refresh_rate.html). Sistnevnte er i milli-Hz; null betyr manglende verdi. [Geometrien er logiske applikasjonspiksler](https://docs.gtk.org/gdk4/method.Monitor.get_geometry.html), ikke fysisk modusoppløsning. Disse må ikke blandes ved skalering. Aktiv modus og annonserte støttede moduser må også skilles. Informasjonen bør hentes fra vinduets økt; en hodeløs tjeneste skal fortsatt fungere uten skjermtilgang.

Lokal sysfs-lesing fant DP-2 tilkoblet og 1920×1080 i moduslisten. Det er ikke en måling av gjeldende Hz eller bevis for at den første oppføringen er aktiv. Ingen EDID-binærfil eller DDC-enhet ble lest her.

**GPU:** Først vis allerede innsamlede felt riktig. Ytterligere getter-kandidater fra eksisterende lesesonde er støttede klokkebegrensninger, prosesslast, busstype, IRQ og strømkilde. Prioriter statiske/oppslagsbaserte opplysninger ved åpning, fremfor flere tunge NVML-kall hvert sekund. Tidligere målinger i `maskinvare-revisjon.md` er historiske og er ikke nye ytelsesmålinger her.

## RAM: et eget datainnsamlingsarbeid

RAM-delen er en bruksoversikt, ikke en full CPU-Z-lignende minnevisning. Dagens `dmi.rs` teller kun bundne SPD-enheter. Det finnes ingen SPD-parser eller fungerende godkjennings-/leseflyt i appen bak teksten «Krever godkjenning».

Ved denne gjennomgangen er fire 512-byte EEPROM-endepunkter synlige i filmetadata. De er lesbare for vanlige brukere på denne maskinen, men innholdet er ikke lest: dette ville være SMBus/I2C-trafikk. Bussnumrene har endret seg siden den gamle revisjonen. Rå SMBIOS-tabell er root-beskyttet og er heller ikke åpnet.

En senere løsning må skille modulens produsent/delenummer/kapasitet/organisering og støttede JEDEC/XMP-profiler, fastvarens rapporterte konfigurasjon og aktiv DRAM-frekvens/kanaler/timinger. SPD alene beviser ikke de aktive verdiene, og SMBIOS garanterer ikke komplette timingdata. Ikke utled dual-channel fra totalen 48 GB eller fire endepunkter. Det er heller ikke korrekt å hevde generelt at all SPD-lesing krever root. Inntil en datakilde faktisk er implementert, bruk «Ikke lest» og en presis forklaring.

## CPU-lanseringsdato

AMD oppgir **19. april 2018** for Ryzen 7 2700X, både i [produktopplysningene](https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-2000-series/amd-ryzen-7-2700x.html) og [lanseringsmeldingen](https://ir.amd.com/news-events/press-releases/detail/827/2nd-generation-amd-ryzen-desktop-processors-deliver-best-in-class-compute-performance-and-even-faster-gaming-framerates-than-previous-generation). Datoen vises i den felles prosessorseksjonen, slik at side og rapport er enige.

Oppslaget må gjelde eksakt modell og produsent, med håndtering av vanlige navnevarianter. CPUID-familie eller «Pinnacle Ridge» er ikke tilstrekkelig; 2700X-datoen skal ikke arves av en annen SKU. Oppslaget er lokalt, merket som produsentopplysning og krever ingen nettsøk ved kjøring. Ukjent modell gir ukjent dato.

## Anbefalt prioritering videre

1. Rett foreldede/feilmerkede verdier og oppfrisking; vis relevante fakta som allerede er innsamlet.
2. Legg til skjerm og nettverkskort med presise enheter og manglende-verdi-håndtering.
3. Utvid RAM med en egen, avgrenset datakilde og tydelig skille mellom støttede og aktive innstillinger.
4. Utvid referansetabeller med kilde per modell. Dagens GPU-tabell har bare fire TU104-varianter; tabellverdier og beregnet FP32/fillrate/båndbredde er ikke målt ytelse.

Flere opplysninger bør ligge under «Detaljer» når de ikke hjelper den vanlige oversikten. Målet er mer korrekt og nyttig informasjon, med kilde og forklaring ved mangler.

## Kontroll av datoendringen

Workspace-clippy på alle mål er ren; 350 tester bestått, 4 ignorert, 0 feilet. Nye kontroller dekker eksakt modell og aliaser, feil SKU/leverandør og dato/kilde i tekst og Markdown. En egen lesende kodegjennomgang fant ingen konkrete feil i tilleggene. Alle tre debug-binærene viser 1.3.3 · bygg 22.

Eksisterende skjermbildeplan `6a` er kjørt i en privat GNOME-økt med simulert GPU og ekte lesbare CPU-data. Bilder av prosessorsiden i lyst/mørkt ved 1920 og 640 px er kontrollert; Lansert-raden vises uten avkutting. Bildene ligger i `docs/skjermbilder/1.3.3/spesifikasjoner/`, og localhost-visningen på port 4320 er oppdatert. Økta er avsluttet. Dette er ingen ny fulltest av alle brukerhandlinger eller måling av ytelse. Den installerte appen er ikke erstattet.
