# Neste systemopplysninger: RAM, skjermer og nettverk

Vurdert 17. november 2026 mot gjeldende kildekode og `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`. Dette er en avgrenset plan, ikke implementerte funksjoner. RGB er fortsatt hovedarbeidet. Denne gjennomgangen har ikke kjørt maskinvareprøver, lest SPD/EDID/rå SMBIOS, åpnet enheter, endret nettverk eller startet bygg. Ingen nye vertsverdier påstås.

## Hva appen allerede gjør

| Område | Kontrollert kode | Faktisk dekning |
|---|---|---|
| RAM | `rigg-hw/src/sys/mem.rs`, `rigg-core/src/model.rs`, `report/system.rs` | Brukbar total, tilgjengelig/brukt/fritt, cache/buffer/Shmem/frigjørbar slab og swap. Innsamlingen skjer i eksisterende systemprøve. |
| Modulopplysninger | `rigg-hw/src/sys/dmi.rs` | Leser tillatte DMI-tekstfelt og teller navn på allerede bundne SPD-endepunkter. Ingen modulinnhold eller SPD-parser. |
| Nettverk | `rigg-hw/src/sys/net.rs`, `rigg-core/src/sensormeta.rs` | Byte/s, operativ tilstand og Wi-Fi-markør per navn; oppdager grensesnitt hvert tiende sekund. Modell og driver finnes separat i PCI-inventaret. |
| Skjermer | `rigg/src/pages/system.rs`, `rigg-core/src/report/system.rs` | Ingen skjermseksjon eller monitorinnsamling. GTK brukes allerede i vinduet. |
| Deling av data | `riggd/src/sampler.rs`, `rigg-hw/src/sys/mod.rs` | Én eksisterende prøvetråd; systemdata inngår i samme `SysSample`. `get_system` har separat cache for CPU/DMI/OS/PCI. |

Tidligere funn er kontrollert på nytt: System-sidens F5 sender fortsatt `get_system {}`, mens daemonen bygger de faste feltene bare når cachen mangler. Teksten «Faste data er lest på nytt» lover derfor mer enn operasjonen gjør. Nettverkets «fysisk»-utvalg bygger fortsatt på navneprefikser. CPU-topologifunnene fra den gamle revisjonen er ikke gjentatt som nye funn; denne gjennomgangen gjelder de tre områdene over.

## Prioritet og minste nyttige levering

1. **Korrekthet og nettverksdetaljer:** rett friskhetsmerking, nettverksidentitet og rater; koble grensesnittet til modell/driver og vis MTU samt tilgjengelig lenkeinformasjon. Dette gjenbruker dagens poller og PCI-oppslag.
2. **Skjermer i den åpne GTK-økten:** legg til en liten, hendelsesdrevet GDK-leser i vinduet. Rapporten bruker samme snapshot. Daemonen skal fortsatt fungere uten skjermøkt.
3. **RAM-bruk og minnepress:** presiser eksisterende kilde/ukjent-status og legg eventuelt til et lite utvalg OS-målinger. Full modulidentifikasjon og aktive timinger er en egen datakildeoppgave; de løses ikke ved å legge flere rader rundt dagens total.

### Nettverk

Foreslått datamodell: behold trafikk i `NetSample`, og legg et avgrenset `NetInfo`-inventar ved siden av med `ifindex`, navn, kildemerket enhetssti, PCI-adresse der den finnes, driver, modell, type og forhold til under-/overliggende grensesnitt. Navnet er en etikett. Bruk `ifindex` sammen med en lokal oppdagelsesgenerasjon for levetiden; behold enhetssti/PCI-/USB-identitet som korrelasjonsdata. Ikke lov at et navn, indeks eller en MAC-adresse er en varig identitet på tvers av omstart eller bytte av adapter.

Les metadata ved oppdagelse og oppdatering, med samme ti-sekunders intervall som dagens `scan`. Slå opp modell ved å koble `device`-lenken til eksisterende `pci::read_board` når forelderen faktisk er PCI. USB- og andre grensesnitt må få sin egen kilde eller ukjent modell, ikke feil PCI-navn. Behold rater i den vanlige prøven; ikke start nye shell-prosesser per intervall.

`mtu` er byte. `speed` er Mbit/s og gjelder siste eller nåværende rapporterte lenkehastighet, hovedsakelig Ethernet. Vis den som aktiv bare med passende lenkestatus; `-1`, lesefeil og ukjent duplex er manglende opplysninger. Wi-Fi trenger en egen kilde for radioens lenkerate. `iflink` beskriver lenking mellom grensesnitt, og nåværende MAC-adresse kan være tilfeldig eller endret. Dette støttes av [kjernens nettverks-ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-net). Administrativt aktiv, carrier og operativt aktiv er forskjellige tilstander; ingen av dem beviser internettilgang. Se [Operational States](https://docs.kernel.org/networking/operstates.html).

Rett fire konkrete måleforhold samtidig:

- `net.rs` bruker nå veggklokken som rategrunnlag via `SysReader::sample(now)`. Send monoton måletid fra prøvetråden i tillegg; behold veggklokken kun som prøvens tidspunkt.
- Lagre akkumulerte tellere som `u64`, beregn differansen før konvertering til rate. Fallende teller eller ny identitet starter en ny måleserie med ukjent første rate; dagens klemming til null skjuler tellerreset.
- Ett manglende RX-/TX-felt skal ikke ødelegge den andre retningen. Manglende målinger skal heller ikke bli null eller en tilsynelatende komplett totalsum.
- Erstatt navneprefikslisten med innsamlet type/topologi. Broer, bond, VLAN og underliggende kort skal ikke dobbelttelles. Ved tvetydig topologi må totalsummen merkes delvis/ukjent eller avgrenses til eksplisitt viste grensesnitt.

For første utvidelse kan eksisterende to statistikkfiler beholdes. Hvis mange tellere legges til, bør én lesende rtnetlink-forespørsel vurderes: hver sysfs-statistikkfil henter internt en hel statistikkstruktur, og kjernen anbefaler rtnetlink for samlet statistikk. Dette krever ingen nettverksendring, men er et eget implementasjonsvalg. [Kernel: Interface statistics](https://docs.kernel.org/networking/statistics.html).

Første levering trenger ikke IP-adresser, SSID, signalstyrke, nettverkssøk eller nye NetworkManager-avhengigheter. Slike felt vurderes senere med presis kilde, oppdatering og rapportbehandling.

### Skjermer

Legg en liten `DisplaySnapshot`-modell i kjernen og en GDK-adapter i GTK-prosessen. Start med modell/beskrivelse/produsent der økten gir dem, kontakt, rapportert frekvens, logisk geometri, skalering og fysisk størrelse. `GdkDisplay::get_monitors` gir en liste som varsler ved endringer; monitorobjekter varsler også når de blir ugyldige. Bruk signaler og egenskapsvarsler, ikke en ny ettsekundspoller. [GDK Monitor](https://docs.gtk.org/gdk4/class.Monitor.html), [get_monitors](https://docs.gtk.org/gdk4/method.Display.get_monitors.html).

Skill feltene tydelig:

- **Logisk arbeidsflate:** geometrien er applikasjonspiksler. Den skal ikke uten videre få etiketten «aktiv skjermmodus». [get_geometry](https://docs.gtk.org/gdk4/method.Monitor.get_geometry.html).
- **Skalering:** bruk desimalverdien fra `get_scale` når tilgjengelig; API-et finnes fra GTK 4.14, og prosjektet bygger allerede med GTK 4.22-egenskaper. [get_scale](https://docs.gtk.org/gdk4/method.Monitor.get_scale.html).
- **Rapportert oppdateringsfrekvens:** GDK returnerer milli-Hz; 59940 blir 59,94 Hz, og null blir «Ikke oppgitt». Dette er ingen måling av øyeblikkelig VRR-frekvens. [get_refresh_rate](https://docs.gtk.org/gdk4/method.Monitor.get_refresh_rate.html).
- **Fysisk størrelse:** millimeter fra økten; avvis manglende/ikke-positive verdier. En beregnet diagonal skal merkes beregnet, ikke som bekreftet produktmodell. [get_width_mm](https://docs.gtk.org/gdk4/method.Monitor.get_width_mm.html), [get_height_mm](https://docs.gtk.org/gdk4/method.Monitor.get_height_mm.html).

Kontaktstrengen er nyttig for visning, men GDK advarer uttrykkelig mot å behandle den som en stabil skjerm-ID. Bruk en øktlokal identitet og ny generasjon ved frakobling. To like skjermer skal ikke slås sammen på modellnavn. [get_connector](https://docs.gtk.org/gdk4/method.Monitor.get_connector.html).

Uten en dokumentert kilde for selve signalmodusen beholdes fysisk aktiv oppløsning, HDR/VRR-status og støttede moduser som ukjent/ikke lest. Ikke sett første element i en sysfs-modusliste som aktivt, og ikke hev at logisk størrelse multiplisert med skalering alltid beviser signalmodusen. Ingen EDID-/DDC-lesing eller DRM-enhetsåpning er nødvendig for første levering. På Wayland/X11/fjernøkter kan feltdekningen variere. En hodeløs rapport sier at skjermøkt mangler; den starter ikke GTK for å hente data.

### RAM

Behold `MemTotal` som «Brukbart minne», ikke installert kapasitet per modul. `MemAvailable` er et estimat for minne som kan brukes uten swapping. Meminfo-felter overlapper; dagens kommentar om at cache-regnestykket får alt til å gå opp i totalen bør ikke videreføres som en generell garanti. Brukt = total minus tilgjengelig kan beholdes som dokumentert appdefinisjon. [Kernel: meminfo](https://docs.kernel.org/filesystems/proc.html#meminfo).

Parseren bør kontrollere heltall, enhet og konsistente grunnverdier. Meminfo bruker teksten `kB` for 1024-byte-enheter; behold wire-navnene om kompatibilitet krever det, men dokumenter KiB internt og konverter korrekt til brukerens GiB/GB-valg. [Kjernens meminfo-implementasjon](https://github.com/torvalds/linux/blob/master/fs/proc/meminfo.c).

En liten utvidelse kan hente `Dirty`, `Writeback`, `SUnreclaim` og `AnonPages` fra filen som allerede leses, med `Option` og kilde. Disse skal vises under Detaljer, ikke summeres med overlappende felt. Fravær gir ukjent, og gammel prøve skal merkes foreldet. Ingen påstand om DRAM-hastighet følger av disse målingene.

Minnepress kan leses hvert femte sekund fra `/proc/pressure/memory` i `SysReader`, uten triggere eller skriving. `some` og `full` har ulike betydninger; `avg10/60/300` er prosent tid, mens `total` er mikrosekunder. Manglende PSI er utilgjengelig, ikke 0 %. [Kernel: PSI](https://docs.kernel.org/accounting/psi.html).

«Moduler og timinger: Krever godkjenning/root» bør erstattes med «Ikke lest» og forklaring om hvilken datakilde som mangler. En tillatelse er ikke en implementert leseflyt. Ingen DDR-type, produsent, delenummer, ranks, XMP-profil, aktiv frekvens, kanaler eller timinger utledes fra 48 GB, hovedkortmodell eller antall endepunkter. Modulprofil, fastvarerapportert konfigurasjon og aktiv kontrollerinnstilling skal få separate felter/kilder når de en gang finnes.

Et mulig senere tillegg er allerede eksponerte EDAC-felt, dersom en eksisterende driver faktisk tilbyr dem. Behold forskjellen mellom `dimm*` og `rank*`; størrelse på en rank er ikke nødvendigvis modulens kapasitet. `dimm_label` kan være brukerdefinert og er ikke automatisk fysisk slotnavn. Det er ikke undersøkt om denne maskinen tilbyr EDAC-data, og ingen modul skal lastes for å skaffe dem. [Kjernens EDAC-ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-devices-edac).

## Filer, avgrensing og verifikasjon ved senere implementasjon

Nettverksarbeidet kan avgrenses til `rigg-hw/src/sys/net.rs`, `sys/mod.rs`, `riggd/src/sampler.rs`, typedata i `rigg-core`, `sensormeta.rs` og felles rapportseksjon. RAM gjenbruker `mem.rs` og samme prøvetråd. Skjermsamleren tilhører GTK-prosessen; kjernen og rapportfunksjonen mottar bare data. Legg ikke grafisk øktavhengighet inn i daemonen. Alle snapshots trenger kilde, tidspunkt og eksplisitt kjent/ukjent/foreldet status.

F5 må enten be eksisterende innsamling om en reell, samordnet oppfrisking eller beskrive at lagrede faste opplysninger vises. Diskers eksisterende mekanisme for forespurt oppdatering er et mønster å undersøke; unngå tung innsamling synkront i socketbehandleren. Tekst og Markdown skal bruke samme formateringsfunksjoner som siden.

Målrettede kontroller:

1. **Nettverk:** teller over 2^53 med liten delta; reset; null/negativ tidsdelta; tilbakehopp i veggklokke; navnebytte; ny enhet med samme navn; delvis RX/TX-feil; bro/bond/VLAN med vilkårlige navn; uavklarte over-/underlenker; `speed=-1`; Wi-Fi uten speed; ukjent duplex og carrier-feil. Verifiser ingen dobbelttelling og ingen falsk null.
2. **Skjermer:** manglende modell/kontakt/størrelse; null Hz; 59,94 Hz; 125/150 % skalering; to like modellnavn; rotert geometri; frakobling og ugyldig objekt; hodeløs rapport. En separat GTK-prøve sammenligner øktens rapporterte felt på Wayland og eventuelt X11, uten modusendring. Ingen bred plattformpåstand fra én GNOME-maskin.
3. **RAM:** manglende MemAvailable; feil enhet/tall; total null; tilgjengelig større enn total; manglende Shmem/slab; ingen swap; delvis PSI og ikke-endelige tall. Grunninformasjon må ikke bli til modul-/kanalpåstander. Rapporttester må skille brukerbart minne fra installert kapasitet.
4. **Felles:** midlertidige fixture-røtter uten fallback til verten, friske mot foreldede data, korrekt tekst/Markdown og eksisterende innstillinger bevart. Mål kostnaden før/etter i eksisterende poller når endringen faktisk implementeres; ingen nye ytelsestall påstås her.

Ingen av disse forslagene trenger å kobles til RGB-hjelperens privilegier. Første levering kan gi nyttige skjerm- og nettverksdetaljer og mer presis RAM-status uten SPD, rå DMI eller nye maskinvaretilganger.
