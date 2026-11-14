# RGB via OpenRGB SDK · lesende tillegg 14. november 2026

**Konklusjon:** En ren SDK-klient kan snakke med en allerede kjørende OpenRGB-server uten egen bussåpning eller skanning. Det gir en konkret backendvei, men løser ikke manglende identifikasjon/støtte for dette Palit-kortet. Ingen lokal RGB-kontroller eller fungerende SDK-server er bekreftet i denne runden. RGB må derfor fortsatt være utilgjengelig som fysisk styring i Agentic Rigg.

Dette supplerer `utredning-styring-2026-11-14.md` med ny lokal inventering og gjennomgått SDK-kode. Ingen OpenRGB-binær, detektor, registersonde eller SDK-kommando er kjørt; ingen pakke/tjeneste/innstilling er endret. Bare dokumentasjon er endret.

**Ny brukerbekreftelse:** Brukeren opplyser at lyset på dette faktiske kortet normalt styres med Palit ThunderMaster i Windows. Det bekrefter fungerende lysstyring gjennom produsentens Windows-program på eksemplaret, ikke en avlest kontrolleridentitet, bestemt protokollversjon eller OpenRGB/Linux-støtte. Ingen ny Windows-/HW-prøve er utført av agenten.

**Foreløpig modellkandidat fra brukeren:** «Mener er Palit Geforce RTX 2080 Super JetStream». Ordlyden er usikker. Dette er ikke innført som automatisk modell-/produsentidentifikasjon. Se modelltillegget nederst; kandidatens produktkode må fortsatt bekreftes på eksemplaret.

## Nye lokale observasjoner

| Lesing | Resultat og avgrensing |
|---|---|
| PATH og dpkg | Ingen `openrgb`/`OpenRGB` i PATH; ingen OpenRGB-pakke funnet i dpkg. Dette utelukker ikke en binær utenfor de undersøkte plasseringene. |
| Installasjons-/oppstartsoppføringer | Ingen treff i undersøkte system-/brukertjenester, autostart, programmeny, `/opt`, lokale binærmapper eller vanlige AppImage-/nedlastingsmapper under `/home/julian`. Ingen OpenRGB-konfigurasjonsmappe funnet der. |
| Prosess-/nettstatus | Først var sandkassens PID-/netlinkvisning utilstrekkelig. Hovedøkten kontrollerte deretter verten med `pgrep`, `ss` og system-/brukertjenestelister: ingen OpenRGB-prosess, ingen lytter på TCP 6742 og ingen OpenRGB-tjeneste funnet. Det utelukker ikke andre porter eller programnavn. Ingen serverforbindelse er forsøkt. |
| GPU-identitet fra sysfs | `0000:1c:00.0`, `10de:1e87`, subsystem `10de:1e87`, driver `nvidia`. Dette identifiserer PCI-funksjonen, ikke lyskontrolleren. |
| Adapter-navn og stier | «NVIDIA i2c adapter 1 at 1c:00.0» ligger nå på `i2c-0`, under samme GPU-PCI-sti. Portnummeret i navnet og Linux-bussnummeret er forskjellige størrelser. Ingen adapter er åpnet. |
| Forbudte/andre enheter | `ccgx-ucsi` ligger nå under GPU-funksjon `1c:00.3` på `i2c-7`; SPD-navnene `ee1004` ligger på SMBus `i2c-8`. Bare navn, driverlenker og stier er lest. Ingen register-, EEPROM- eller enhetslesing. |
| USB-identiteter | Blant eksisterende USB-enheter finnes `1462:7b85`, «MSI PRO CARBON 12». Det er ikke en Palit-identifikasjon eller et bevis for GPU-lys. Ingen USB-enhet er åpnet. |

Adapterplasseringen er et nytt øyeblikksbilde, ikke et bussnummer som kan hardkodes. Sysfs-navnene gir ingen «PALIT 150»-signatur.

## Hva SDK-kildene faktisk viser

Den [offisielle SDK-siden](https://openrgb.org/sdk.html) beskriver integrasjon med enheter som OpenRGB støtter. [Protokollen](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Documentation/OpenRGBSDK.md) har separate kommandoer for protokollversjon, kontrollerliste og kontrollerdata. Rescan er en egen kommando, ID 140. [Serverkoden](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/NetworkServer.cpp) returnerer eksisterende kontrollerbeskrivelser i dataforespørselen; den kontrollerte stien kaller ikke deteksjon.

SDK-data omfatter navn, leverandør, serienummer, location, soner og modi. Protokoll 0–5 bruker listeindekser som må forkastes når listen endres; protokoll 6 har egne kontroller-ID-er. Verken indeks eller navn alene er en varig GPU-binding. [Nettverkshodet](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/NetworkProtocol.h) oppgir localhost og TCP 6742 som standard.

**Viktig skille:** OpenRGBs egen oppstart/deteksjon kan åpne busser. [Palit-detektoren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUControllerDetect.cpp) gjør faktiske I2C-lesinger for V2 og har en skrivebasert fallback i en V1-gren. Det er ikke tillatt å starte slik deteksjon automatisk som del av en «lesende» SDK-oppdagelse. SDK flytter eierskapet til HW-tilgangen; det fjerner ikke identifikasjonskravet.

## Kortstøtte: fortsatt ingen ny lokal bekreftelse

[#1420](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1420) er nå tilgjengelig og oppgir GamingPro OC RTX 2080 med samme PCI/subsystem. Dette bekrefter at saken er relevant, ikke at vårt eksemplar har samme kontroller. Den gjennomgåtte Palit-detektorfilen har fortsatt ingen full match for `10de:1e87 / 10de:1e87`. Gainward 2080-oppføringen har andre øvrige ID-er. Den separate [NVIDIA Illumination-detektoren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/NVIDIAIlluminationController/NVIDIAIlluminationControllerDetect_Windows_Linux.cpp) krever en annen subsystem-ID for 2080 FE.

De offisielle støttesidene for [1.1](https://openrgb.org/devices_1.1.html) og [pipeline](https://openrgb.org/devices_pipeline.html) ble åpnet, men selve enhetstabellen var ikke tilgjengelig i tekstvisningen. Detektorvurderingen over bygger på kildekoden, ikke en antatt komplett tabell. Ingen lokal kontrollersignatur, produktkode eller ThunderMaster-capture er innhentet nå.

## Konkret neste backend

1. Lag en avgrenset SDK-klient med simulert server først. Bruk bare en eksisterende, uttrykkelig valgt localhost-server; ikke start OpenRGB, last driver, åpne I2C eller be om rescan. Manglende server betyr utilgjengelig RGB.
2. Les protokoll og eksisterende kontrollerdata. Bind bare en identifisert GPU-kontroller til valgt PCI/UUID. [Palit V2-location](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUv2Controller.cpp) beskriver bussnavn/adresse; lokal sysfs-korrelasjon kan hjelpe, men er ikke alene protokollbevis. Avvis tvetydig kortvalg, tom identitet og endret liste.
3. Vis bare kontrollerens faktiske soner/modi. Ingen automatisk profil, oppstartsrestore, resize, SaveMode, firmware eller RGB-skriving ved tilkobling. Senere eksplisitt fargeendring og feil/frakobling må prøves separat på en verifisert kontroller.

Dette kan bygges uten ny busskanning i Agentic Rigg. Det som mangler før fysisk RGB kan åpnes, er en faktisk støttet/identifisert Palit-kontroller i en gjennomgått OpenRGB-instans. Å bare legge til samme PCI-ID i en detektor eller starte bred skanning er ikke en bekreftelse og er ikke gjort.

## Avgrenset ThunderMaster-oppfølging

[Palits egen ThunderMaster-side](https://www.palit.com/thundermaster/index.php?lang=en) beskriver RGB-styring og modellavhengig lagring på kortet. Den publiserer ingen SDK-/registerbeskrivelse. Dette er ikke bevis for at et privat SDK ikke finnes, eller for at akkurat RTX 2080 kan lagre farge permanent. Den gamle produktsidelenken for GamingPro OC fra #1420 videresender nå til forsiden og gir ingen ny modellbekreftelse.

**V1 versus V2:** Den kontrollerte [OpenRGB-detektoren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUControllerDetect.cpp) leter etter «PALIT» ved register 0x07 i V1-grenen; registrerte V1-kort bruker 0x08. V2-grenen gjelder 0x49 og andre registerkontroller. [V2-headeren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUv2Controller.h) har kontroll 0x60 og RGB/lysstyrkefelter. Dette er forskjellige protokollspor, ikke noe vi kan velge bare fordi ThunderMaster virker. «PALIT 150» i den eldre planen er fortsatt en referanse til et annet korts opptak; vedlegget fra #1420 lot seg ikke hente på nytt her. Ingen lokal signatur eller fast kobling mellom den strengen og vårt eksemplar er bekreftet.

**Neste konkrete forskningspunkt ved en senere Windows-økt:** dokumenter fysisk produktkode, ThunderMaster-versjon og hvilke lysvalg som faktisk vises. Ved separat avtalt opptaksrunde: registrer bare produsentprogrammets eksisterende kall rundt én kjent lysendring om gangen, med synlig resultat, og gå tilbake til utgangspunktet. Skill oppstart/avslutning fra farge, av/på og lysstyrke. Sammenlign transport, adresser, registerrekkefølge og eventuelle signaturdata med #1420/V2-sporet. Ikke legg til registerlesinger, skanning, firmware, BIOS eller «set as default» for å få et opptak.

Målet er å avgjøre hvilken Linux-/OpenRGB-backend som faktisk kan støtte eksemplaret. Fungerende ThunderMaster er et bedre utgangspunkt enn PCI-likhet alene, men gir ikke i seg selv Linux-protokollen eller en SDK-enhet. Ingen opptaksverktøy, driver eller binær er lastet ned eller kjørt i denne undersøkelsen.

## Modelltillegg · RTX 2080 Super JetStream er en kandidat

Her er **Super JetStream produktnavnet**, mens GPU-en i produktarket er **GeForce RTX 2080**, ikke RTX 2080 SUPER. Det passer som kandidat med tidligere avlest RTX 2080/PCI 1e87 og minneklokke 7000 MHz, men flere kort kan ha disse opplysningene. Ingen ny HW-/sysfs-lesing er gjort i modellrunden.

[Produktarket distribuert via Inet](https://cdn.inet.se/pdf/5406816_0.pdf), side 1, oppgir `NE62080U20P2-1040J`, TU104, 2944 kjerner, 8 GB GDDR6, 14 Gbps og Super JetStream som OC-versjon av JetStream-serien. [OWCs egen kompatibilitetstabell](https://eshop.macsales.com/tech_center/manuals/helios-fx-compatibility-guide-owcsgdhfxgpucmp.pdf), side 36, knytter samme kode til «GeForce RTX 2080 Super JetStream» og fører RTX 2080 SUPER separat. Den foreslåtte `NE62080S20P2-1040J` er ikke bekreftet; kildene som ble funnet bruker **U**, ikke S. Ingen av kodene er avlest på brukerens kort. Palits gamle modellside [id 3048](https://www.palit.com/palit/vgapro.php?id=3048&lang=en) videresender til forsiden og gir ikke nå en lesbar modellbekreftelse.

**RGB-konsekvens:** Det er ikke funnet en eksakt 2080 Super JetStream-oppføring/full-ID-match i den gjennomgåtte OpenRGB-detektoren. V1-grenen for de registrerte eldre kortene og V2-grenen på 0x49 er forskjellige spor. «PALIT 150» i GamingPro-opptaket identifiserer verken JetStream-kortet eller hvilken klasse som kan brukes. Produktnavn, samme GPU og fungerende ThunderMaster erstatter ikke kontrolleridentifikasjon.

**Zero RPM:** Opplysninger om Dual BIOS/0-dB på BIOS 2 finnes hos forhandlere og testere, men dette ble ikke verifisert i tilgjengelig produsenttekst i denne avgrensede runden. PDF-bildene lot seg ikke hente for kontroll. Automatisk viftestopp på brukerens eksemplar er derfor fortsatt ubekreftet, og BIOS-posisjon skal ikke antas eller endres.

**Neste konkrete avklaring:** produktkode fra eske/kvittering eller tilgjengelig etikett, uten demontering, før modellen fastsettes. Deretter kan en separat avtalt Windows-fangst fra dette eksemplarets ThunderMaster sammenlignes med kjent protokoll. Ingen GPU-identitet, produsent, registerliste eller RGB-støtte endres automatisk på grunnlag av den usikre kandidaten.
