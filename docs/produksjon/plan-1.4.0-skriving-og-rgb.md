# Plan for 1.4.0 · ekte skriving og RGB

> **Oppdatert føring 14. november 2026:** Les [den nye utredningen](utredning-styring-2026-11-14.md) først. Brukeren ønsker én aktivering uten fire godkjenningstrinn. Tillegget skiller bekreftede getter-resultater fra uprøvde skrivekall og korrigerer sikkerheten i RGB-påstandene nedenfor. Denne eldre planen er historisk forskningsgrunnlag, ikke bekreftet støtte eller instruks om å aktivere ekte skriving. Ingen I2C-lesing eller skriving er autorisert av dokumentet.

*Research-agent 9R, 9. november 2026. Gjelder Palit RTX 2080 Rev. A (TU104, PCI `10de:1e87`, subsystem `10de:1e87`), VBIOS 90.04.0B.40.8C, driver 610.43.02 (NVIDIA Open Kernel Module med GSP), GNOME 50.1 på Wayland, systemd 259. Ingen kode er endret, og ingenting er skrevet til kortet. Ingen `/dev/i2c-*` er åpnet (se «Metode» nederst).*

**Merking i dokumentet**

- **[Kilde]**: står i dokumentasjon, kildekode eller et opptak. Lenken står i kapittel 8.
- **[Målt]**: lest på denne maskinen i dag, bare med lesing.
- **[Antakelse]**: mitt eget resonnement. Må prøves før det brukes.

---

## 0. Sammendrag for deg

### Hva som er mulig

| Funksjon | Mulig? | Hvor sikker |
|---|---|---|
| Effektgrense 105–275 W | **Ja**, via NVML | Høy. LACT gjør det samme på GeForce |
| Vifter: fast fart, egen kurve, tilbake til Auto | **Ja**, via NVML, også på Wayland og uten Coolbits | Høy. Minste fart er 35 %, og det er en grense i kortets BIOS |
| Klokke-offset for kjerne og minne | **Ja**, via NVML (`SetClockOffsets`) | Middels til høy. Driveren har hatt feil i hva den rapporterer tilbake, så vi må måle klokkene under last |
| Temperaturmål (65–88 °C) | **Kanskje.** Kallet finnes og LACT bruker det, men flere GeForce-kort svarer «ikke støttet» | Lav. Må prøves |
| RGB-lyset | **Sannsynligvis ja**, med egen kode på I2C | Middels til høy (se under) |
| Viftestopp (0 o/min) | **Nei.** Kortet har ikke viftestopp, og 35 % er en BIOS-grense | Høy |
| «Lagre lyset på kortet» | **Nei.** Vi kjenner ingen kommando for det på dette kortet | Høy |
| OC Scan | **Ikke i 1.4.0.** Det finnes ingen NVIDIA-skanner på Linux. En egen skanner er mulig, men risikabel og dyr | Høy |

### RGB: hva vi vet nå (nytt i denne runden)

Det finnes et OpenRGB-innlegg ([#1420](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1420)) for **akkurat ditt kort**: «Palit GamingPro OC RTX 2080» med `VEN_10DE&DEV_1E87&SUBSYS_1E8710DE`. Det har ThunderMaster-opptak fra Windows. Opptakene viser at:

- lyskontrolleren sitter på **I2C-adresse 0x49**
- den melder seg med **«PALIT 150»** i registrene `0xF6–0xFE` (ikke «PALIT 100», som 2080 Ti-kortet gjør)
- farge og lysstyrke ligger i `0x6C–0x6F`, og modus i `0x60`

Protokollen er den samme som OpenRGB kaller «PalitGPUv2». En testversjon av OpenRGB for søsterkortet (2080 Ti, samme ID-mønster) ble prøvd av en bruker: fast farge, av/på og lysstyrke virket. Den ble aldri tatt inn i OpenRGB. **OpenRGB støtter derfor ikke kortet ditt i dag.** Ingen detektor matcher `10de:1e87`/`10de:1e87`.

Jeg er ganske sikker (anslagsvis 75–80 %) på at lyset kan styres. Det som gjenstår, er:

1. at ditt eksemplar faktisk har lyset (en GamingPro, ikke en variant uten RGB)
2. at Linux-adapteren «NVIDIA i2c adapter 1» er den samme porten som ThunderMaster bruker på Windows
3. at signaturen leses riktig på din maskin

Alle tre avgjøres av **én lesing som du kjører selv** (kapittel 2.6).

### Hva som er risikabelt

1. **I2C mot feil buss eller adresse.** Det er det eneste her som i verste fall kan skade maskinvare. Bussnumrene flytter seg, og samme maskin har USB-C-kontrolleren og SPD-brikkene på andre I2C-busser. Planen låser derfor til én adapter, funnet ut fra navn og PCI-sti, og til én adresse. Den skriver bare til hvitelistede registre etter at signaturen er sett.
2. **Klokke-offset** kan gi bildefeil, krasj i spill eller at hele Wayland-økta faller (skjermkortet driver skrivebordet også). Det skader ikke kortet innenfor NVMLs områder, fordi spenningen er låst på Turing og effektgrensen gjelder. Du kan likevel miste ulagret arbeid. Alt er tilbake til standard etter omstart.
3. **Vifter i manuell modus** blir stående på siste fart hvis hjelperen dør. Det er dekket: `ExecStopPost`, nødregelen over 85 °C, og minimum 35 %.

### Det jeg anbefaler

**Rekkefølge for 1.4.0:**

1. **B0: forarbeid uten skriving.** Rett sandkassen (se 3.2), gi hjelperen `CAP_SYS_ADMIN` og ingenting mer, og la hjelperen åpne NVML for **lesing** i den ekte tjenesten.
2. **B1: effektgrense**, med deg til stede.
3. **B2: vifter.**
4. **B3: temperaturmål.** Bare hvis kortet svarer, ellers merkes det «ikke støttet».
5. **B4: klokke-offset.** Kjerne først, så minne.
6. **A: lys.** Først signaturlesingen du kjører selv, så lesing i hjelperen, så skriving.

RGB kan gå parallelt med B2–B4, men skal ikke slippes sammen med første NVML-skriving. Én ny risiko om gangen.

**Ikke gjør:** viftestopp, «lagre på kortet», modusene `0xE0–0xE6` (regnbue og blink i maskinvaren), automatisk OC Scan i 1.4.0, eller noen form for skanning av I2C-busser.

### Hva du må gjøre selv

1. Se på kortet: lyser logoen, og i hvilken farge eller effekt? Les produktkoden på etiketten (for eksempel `NE62080S20P2-180A` = GamingPro OC).
2. Installer `i2c-tools` selv hvis du vil kjøre signaturlesingen (`sudo apt install i2c-tools`). Kjør de to kommandoene i kapittel 2.6, og lim inn svaret.
3. Godkjenn hvert trinn med passord i appen, og vær til stede ved første prøve av hvert trinn (sjekklistene i kapittel 3.7).
4. Ha et spill eller en benchmark klar som last (til B1, B2 og B4).

---

## 1. Maskinen nå [Målt]

### 1.1 PCI

`lspci -nnv -s 1c:00.0`: `TU104 [GeForce RTX 2080 Rev. A] [10de:1e87] (rev a1)`, `Subsystem: NVIDIA Corporation … [10de:1e87]`, driver `nvidia`. sysfs sier det samme: `vendor=0x10de device=0x1e87 subsystem_vendor=0x10de subsystem_device=0x1e87 class=0x030000`. **Kortet melder NVIDIAs subsystem-ID, ikke Palits (`1569`) eller Gainwards (`10b0`).** Det er bekreftet.

Funksjonene på kortet:

| Funksjon | ID | Driver |
|---|---|---|
| `1c:00.0` | `10de:1e87` | `nvidia` (grafikk) |
| `1c:00.1` | `10de:10f8` | `snd_hda_intel` |
| `1c:00.2` | `10de:1ad8` | `xhci_hcd` (USB-C) |
| `1c:00.3` | `10de:1ad9` | `nvidia-gpu` (USB-C/PD-kontroller over I2C) |

### 1.2 I2C, bare navn og stier

| Nr. nå | Navn (`/sys/bus/i2c/devices/i2c-N/name`) | Forelder i sysfs | Rolle |
|---|---|---|---|
| `i2c-0` | NVIDIA i2c adapter 1 at 1c:00.0 | `0000:1c:00.0` | **Port 1, der Palit-kontrolleren skal være** |
| `i2c-1`…`i2c-6` | NVIDIA i2c adapter 2, 3, 5, 6, 7, 8 at 1c:00.0 | `0000:1c:00.0` | Andre GPU-busser (DDC og annet). Port 4 finnes ikke |
| `i2c-7` | SMBus PIIX4 adapter port 0 at 0b00 | `0000:00:14.0` | Hovedkortet: `7-0050…0053` = `ee1004` (SPD), `7-0036/0037` = `dummy` (sidevalg) |
| `i2c-8` | SMBus PIIX4 adapter port 2 at 0b00 | `0000:00:14.0` | Hovedkortet |
| `i2c-9` | SMBus PIIX4 adapter port 1 at 0b20 | `0000:00:14.0` | Hovedkortet |
| `i2c-10` | NVIDIA GPU I2C adapter | `0000:1c:00.3` | **USB-C: `10-0008` = `ccgx-ucsi`. Aldri røres** |

- Nummereringen er den samme som 7. november, men det er tilfeldig. Den avhenger av rekkefølgen modulene lastes i.
- Det er **ingen** kjerneklient på adresse 0x49 på `i2c-0` (ingen `0-0049` i `/sys/bus/i2c/devices`).
- **`i2c-dev` er bygget inn i kjernen** (`CONFIG_I2C_CHARDEV=y`, og `i2c-dev.ko` står i `modules.builtin`). Den trenger derfor ikke lastes, og kan ikke lastes ut. `/dev/i2c-0…10` finnes og er `crw------- root root`. Det finnes ingen `i2c`-gruppe og ingen udev-regel som åpner dem for brukere. Det er riktig for oss: bare root-hjelperen skal åpne én av dem.
- `/sys/class/i2c-adapter/` finnes ikke på denne kjernen. Bruk `/sys/bus/i2c/devices/i2c-N` eller `/sys/class/i2c-dev/i2c-N` (begge har `name`).
- udev ser enheten slik (`udevadm info -a -n /dev/i2c-0`, bare lesing): `SUBSYSTEM=="i2c-dev"`, `ATTR{name}=="NVIDIA i2c adapter 1 at 1c:00.0"`, og forelderen `KERNELS=="0000:1c:00.0"`, `DRIVERS=="nvidia"`, `ATTRS{vendor}=="0x10de"`, `ATTRS{device}=="0x1e87"`, `ATTRS{subsystem_vendor}=="0x10de"`, `ATTRS{subsystem_device}=="0x1e87"`.

### 1.3 Utgangspunktet for NVML-skriving (før-verdier)

| Verdi | Nå | Kilde |
|---|---|---|
| Effektgrense / standard / område | 245 W / 245 W / 105–275 W | `nvidia-smi -q -d POWER`, `nvml_probe.py` |
| Temperaturmål (ACOUSTIC_CURR) / område | 83 °C / 65–88 °C | `nvml_probe.py` («Akustisk mål») |
| Avstenging / nedklokking / GPU maks | 100 / 97 / 88 °C | samme |
| Klokke-offset P0 og P8 | 0 MHz (kjerne −1000…+1000, minne −2000…+6000 NVML-enheter) | `nvml_probe.py` |
| Vifter | 2 stk., 35 %, policy 0 (driveren styrer) | samme |
| Maks klokker | grafikk 2190, minne 7000 MHz | `nvidia-smi -q -d CLOCK` |
| Persistence mode | Av | `nvidia-smi -q` |
| P-tilstander | P0, P2, P3, P5, P8 | `nvml_probe.py` |
| Hvilemodus | `nvidia-suspend`, `nvidia-resume` og `nvidia-hibernate` er på, `PreserveVideoMemoryAllocations=1` | `systemctl list-unit-files`, `/proc/driver/nvidia/params` |

---

## 2. A · RGB på Palit RTX 2080

### 2.1 Bevisene

| # | Hva | Hvor sterkt |
|---|---|---|
| 1 | **OpenRGB #1420**: «Palit GamingPro OC RTX 2080», `PCI\VEN_10DE&DEV_1E87&SUBSYS_1E8710DE&REV_A1`. Samme ID-er som ditt kort. ThunderMaster-opptak med `nvapispy` | **Sterkt.** Samme modell og samme ID-er |
| 2 | Opptaket `nvapi_21928562.txt` (#1420, oppstart → blå/rød pusting → lukk): alle kall går til **0x49** med 1 byte registeradresse. Lesing `0xF0–0xFF` gir `00 00 00 00 00 00 50 41 4C 49 54 20 31 35 30 00` = **«PALIT 150»** | **Sterkt** |
| 3 | Samme opptak: ThunderMaster skriver `0x60=01, 0x6C=45, 0x6D=C8, 0x6E=00, 0x6F=64` (lagret farge ved oppstart), og leser `0x6C` med 4 byte som kontroll | Sterkt |
| 4 | Pusting: `0x60=22`, `0x61=01 62=FF 63=00 64=FF 65=00 66=64 67=00 68=64 69=00 6A=00 6B=00`, farge 1 i `0x6C–0x6F` (`00 00 FF 64`), farge 2 i `0x70–0x75` (`FF 00 00 64 00 00`) | Sterkt |
| 5 | Regnbue (`rainbow.txt`, 772 linjer): ThunderMaster lager den **i programvare**. 129 ganger `0x60=01` + RGB + lysstyrke, og hver gang en 4-byte kontrollesing av `0x6C`. Ingen bruk av `0xE0` | Sterkt |
| 6 | Kommentar i #1420 fra OpenRGB-utvikleren TheRogueZeta: veldig likt EVGA v2, men uten lås/opplås-kommando | Middels |
| 7 | **OpenRGB #1938** (Palit 2080 Ti GamingPro OC, `1E07`/`1E0710DE`, «PALIT 100»): en testversjon med detektor på port 1, adresse 0x49 ble prøvd. Brukeren skrev at farge, av/på og lysstyrke virket, men ikke «cycle» og «strobe». En annen bruker bygde selv med én detektorlinje og fikk det til å virke | Middels. Søsterkort, Windows |
| 8 | OpenRGB `master` (`b9d903b`, 2026-11-08): `PalitGPUv2Controller` har nøyaktig disse registrene. Detektorer med NVIDIA-subsystem-ID på 0x49 finnes for RTX 2060 SUPER, **RTX 2080 SUPER** (`1E81`/`10DE:1E81`) og RTX 2070 SUPER Jetstream. **Ingen for `1E87`/`10DE:1E87`** | Sterkt for protokollen. Negativt for «støttet» |

**Konklusjon:** Kortet har med stor sannsynlighet en Palit-mikrokontroller på 0x49 som snakker PalitGPUv2 med signaturen «PALIT 150». **OpenRGB støtter det ikke** (#1420 står fortsatt i «Step1 – Missing Info»). Vi må bruke vår egen implementasjon av protokollfaktaene. OpenRGB er GPL, så vi kopierer ingen kode.

### 2.2 Hvordan OpenRGB identifiserer v2-kort, og hvorfor vi gjør det strengere

- OpenRGB krever `bus->info.port_id == 1`. På Linux leses port-ID fra adapternavnet med `sscanf("NVIDIA i2c adapter %hu at")` (`i2c_smbus_linux.cpp`). Det krever også PCI-ID-ene fra sysfs (vendor, device, subsystem).
- Selve testen for v2 (`TestForPalitGPUv2Controller`): `read_byte_data(0x49, 0x00) == 0x00` **og** `read_byte_data(0x49, 0xE0) < 0x05`. Det er en svak test. Mange brikker kan gi 0 i register 0.
- v1-testen leser «PALIT» i `0x07–0x0B`, og faller ellers tilbake til en **quick write mot 0x08**. Den skal vi aldri etterligne: 0x08 er USB-C-kontrollerens adresse på `i2c-10`.
- Andre produsenter bruker også 0x49 på NVIDIA-busser (EVGA Turing, Zotac Turing, PNY). Det er derfor **innholdet** som avgjør, ikke adressen.

**Vår test:** bare ThunderMasters egen lesing, `0xF0–0xFF`, én byte om gangen (byte data). Den er brukt hver gang ThunderMaster starter på akkurat denne modellen. Vi leser **ikke** `0x00` eller `0xE0`, fordi de ikke trengs når vi har den sterkere signaturen.

### 2.3 Protokollen (PalitGPUv2, slik den brukes på RTX 2080)

| Register | Betydning | Kilde | Vi bruker |
|---|---|---|---|
| `0xF0–0xF5` | `00` | opptak | Leses (signatur) |
| `0xF6–0xFE` | ASCII «PALIT 150» | opptak #1420 | Leses (signatur) |
| `0xFF` | `00` | opptak | Leses (signatur) |
| `0x60` | Kontroll: `0x01` = programvare/fast farge, `0x22` = pusting i maskinvaren | opptak, OpenRGB `.h` | Skrives (hviteliste) |
| `0x61` | `0x01` ved pusting | opptak | Skrives bare ved pusting |
| `0x62–0x65` | Pustefart (lav/høy byte, to sett) | OpenRGB `.h`, opptak | Skrives bare ved pusting |
| `0x66–0x6B` | Pusteparametere (`64 00 64 00 00 00` i opptaket) | opptak | Skrives bare med opptakets verdier |
| `0x6C/6D/6E` | R/G/B, farge 1 | opptak, OpenRGB | Leses og skrives |
| `0x6F` | Lysstyrke 0–100 (`0x64` = 100) | opptak | Leses og skrives |
| `0x70–0x72` | R/G/B, farge 2 (pusting) | opptak, OpenRGB | Skrives bare ved pusting |
| `0x73` | Lysstyrke for farge 2 | opptak | Skrives bare ved pusting |
| `0x74–0x75` | `00` | opptak | Skrives bare som `00` |
| `0xE0–0xE2`, `0xE4–0xE6` | Maskinvaremodus (cycle, strobe), retning, fart, farge 3 | OpenRGB `.h` | **Aldri.** Ikke i noe opptak fra RTX 20, og rapportert at det ikke virker på 2080 Ti |
| Lagre til flash | **Ingen kjent kommando** | – | **Aldri** |

**Lagre på kortet:** Ingen av de tre opptakene fra ditt kort har en lagre-kommando. Det OpenRGB har for v2 har heller ingen. ThunderMaster skriver den lagrede fargen på nytt hver gang den starter (opptak 3). Det tyder på at *programmet* husker fargen, ikke kortet. Palits egen ThunderMaster-side nevner «Save RGB Control To The Board» for nyere kort (RTX 30/40/50), og skriver at noen modeller trenger en BIOS-oppdatering for det. **Vi skriver aldri noe ukjent register for å prøve.** Erstatningen er at tjenesten legger lyset på igjen når du logger inn (kapittel 4).

**Avgrensning:** Hvert opptak har én fargetrippel. Logoen er altså **én sone**, ikke adresserbar per LED [Antakelse, godt underbygd].

### 2.4 Finne riktig buss: aldri ut fra nummer

Hjelperen velger adapteren slik, og avbryter ved første avvik:

1. Gå gjennom `/sys/bus/i2c/devices/i2c-*`. Behold bare de der `name` er **nøyaktig** `NVIDIA i2c adapter 1 at <bb>:<dd>.<f>` (etterfølgende linjeskift fjernes).
2. `readlink -f` på adapteren. Forelderen skal være en PCI-enhet (`…/0000:1c:00.0/i2c-N`) med `vendor=0x10de`, `device=0x1e87`, `subsystem_vendor=0x10de`, `subsystem_device=0x1e87`, `class=0x030000`, driver `nvidia`. Bussadressen i navnet skal være den samme som forelderens PCI-adresse.
3. Det skal finnes **nøyaktig én** slik adapter. Null eller flere gir stopp.
4. Svarteliste, uansett nummer: adaptere med navn `NVIDIA GPU I2C adapter` (USB-C, funksjon `.3`), `SMBus PIIX4*`, `SMBus I801*`, og enhver adapter som har barn med navn `ccgx-ucsi`, `ee1004`, `spd5118` eller `dummy`.
5. Ingen kjerneklient på 0x49 (`/sys/bus/i2c/devices/N-0049` finnes ikke). Ellers stopp.
6. Åpne `/dev/i2c-N`, kjør `fstat`, og sjekk at `st_rdev` er major 89, minor N. Sett adressen med `ioctl(I2C_SLAVE, 0x49)`, **aldri `I2C_SLAVE_FORCE`**. Da får vi `EBUSY` hvis en kjernedriver eier adressen.
7. Bruk bare `I2C_SMBUS` med `I2C_SMBUS_BYTE_DATA` (les og skriv én byte). **Ingen quick, word, block eller `I2C_RDWR`.** NVIDIA-driveren har hatt feil i word- og blokkoverføringer som ga gale farger ([open-gpu-kernel-modules #41](https://github.com/NVIDIA/open-gpu-kernel-modules/issues/41), rettet i 565.77). Byte data virket hele tiden.

### 2.5 Plan for «lys»-trinnet i hjelperen

#### 2.5.1 Identifisering, bare lesing

- Én adapter (2.4), én adresse (0x49), 16 lesinger `0xF0…0xFF` med byte data.
- Krav: `F0–F5 = 00`, `F6–FE = 50 41 4C 49 54 20 31 35 30` («PALIT 150»), `FF = 00`.
- Ved første feil (NACK/`EIO`/`ENXIO`/`ETIMEDOUT`): stopp med en gang. Ikke prøv igjen i samme økt.
- Andre «PALIT 1x0»-versjoner (for eksempel «PALIT 100») gir «ukjent versjon av lyskontrolleren». Status vises, men vi skriver ingenting. Det kan åpnes senere etter et nytt opptak.
- Er signaturen riktig, leses tilstanden: `0x6C, 0x6D, 0x6E, 0x6F`. Det er de samme fire bytene ThunderMaster leser. **`0x60` leses ikke av ThunderMaster eller OpenRGB**, og kan derfor ikke regnes som trygg å lese. Vi lar være, og regner med at kortet er i den modusen du ser med øynene (trinn 0).
- Resultatet lagres i minnet som `opprinnelig` og logges i journalen.

#### 2.5.2 Hviteliste for skriving

Hjelperen tar bare imot semantiske kall. Det finnes ingen generell «skriv register»-operasjon i protokollen.

| Kall | Skrivinger i denne rekkefølgen | Grenser |
|---|---|---|
| `rgb_set {mode:"static", r,g,b, brightness}` | `60=01`, `6C=r`, `6D=g`, `6E=b`, `6F=brightness` | r/g/b 0–255, lysstyrke 0–100 |
| `rgb_set {mode:"off"}` | `60=01`, `6C=00`, `6D=00`, `6E=00`, `6F=00` | Som ThunderMaster: farge 0, ikke `0x60=00` |
| `rgb_set {mode:"breathe", c1, c2, speed}` | `60=22`, `61=01`, `62=lo`, `63=hi`, `64=lo`, `65=hi`, `66=64`, `67=00`, `68=64`, `69=00`, `6A=00`, `6B=00`, `6C–6F=c1`, `70–72=c2`, `73=lys2`, `74=00`, `75=00` | Fart 0x000A–0x1324 (OpenRGB-området). Resten fast som i opptaket |
| Programvareeffekter (regnbue, temperaturfarge) | Gjentatt `static` | Høyst **10 oppdateringer per sekund** (50 byte), og bare fra `riggd` med åpen forbindelse |

**Faste regler:**

- **Registerhvitelisten** er `0x60–0x75` med verdiene over. Alt annet avvises før `ioctl` (testet mot en falsk buss).
- Etter hver `static` leses `0x6C–0x6F` tilbake (fire byte-lesinger). Ved avvik stopper `lys` resten av økta, og status viser «Lyset svarte ikke som ventet». Ingen nye forsøk.
- **Én skriving om gangen:** en mutex rundt hele sekvensen.
- Ingen skriving før signaturen er sett i **samme prosess**, og etter at adapteren er funnet på nytt (2.4) ved hver start av hjelperen.

#### 2.5.3 Tilbake til fabrikkoppsettet

1. **Det vanlige:** «Tilbakestill lyset» skriver `opprinnelig` (fra 2.5.1) med `static`. Det gjør også `reset`, frakobling, stopp av hjelperen (`ExecStopPost`) og når `lys` trekkes tilbake.
2. **Hvis kortet var i en effekt** som ikke er fast farge før første skriving, kan vi ikke gjenskape effekten, siden `0x60` ikke leses. [Antakelse] Mikrokontrolleren går til fastvarens standard når strømmen er helt borte, og ThunderMaster legger sin lagrede farge på hver gang den starter. Det bekreftes i prøve L4 (2.7).
3. **Siste utvei:** Slå av maskinen helt, og bryt strømmen i 30 s. Lyset er ikke lagret på kortet (2.3), så det overlever ikke et strømbrudd [Antakelse, prøves i L4].

#### 2.5.4 Når signaturen ikke stemmer

- Hjelperen svarer `rgb_*` med `not_supported` og `details.reason = "signature_mismatch"` (eller `"no_adapter"`, `"busy"`, `"io_error"`). Bytene som ble lest, står i journalen.
- **Ingen skriving.** Ingen nye forsøk i samme økt. Ingen andre adresser eller busser. Ingen skanning.
- Lyssiden viser «Fant ikke Palit-lyskontrolleren på dette kortet» med de leste bytene i detaljene. Lysdelen står som «Ikke støttet» til du godkjenner trinnet på nytt.
- Alternativet NVIDIA Illumination (NvAPI) gjelder bare RTX 2080 FE (`10DE:12A6`) i OpenRGB, ikke ditt kort. Det prøves ikke.

### 2.6 Kommandoer du kan kjøre selv for å bekrefte signaturen

Kjøres bare etter trinn 0 (se på kortet). Du kjører dem, og limer inn svaret. Hovedøkta kjører dem ikke.

**1. Finn bussen ut fra navn og PCI-sti (ingen root, ingen I2C):**

```bash
for d in /sys/bus/i2c/devices/i2c-*; do
  [ "$(cat "$d/name")" = "NVIDIA i2c adapter 1 at 1c:00.0" ] || continue
  p=$(readlink -f "$d/..")
  [ "$(basename "$p")" = "0000:1c:00.0" ] || continue
  [ "$(cat "$p/subsystem_vendor"):$(cat "$p/subsystem_device")" = "0x10de:0x1e87" ] || continue
  basename "$d"
done
```

Skal gi **nøyaktig én** linje (i dag `i2c-0`). Gir den null eller to linjer: stopp og lim inn svaret.

**2. Les signaturen: én buss, én adresse, 16 byte-lesinger, stopp ved første feil.** Krever `i2c-tools`. `-y` hopper over spørsmålet, så det er linje 1 som gjør valget. Bytt `N` med tallet fra linje 1:

```bash
N=0   # fra kommando 1
for r in $(seq 0xF0 0xFF); do
  printf '0x%02X ' "$r"
  sudo i2cget -y "$N" 0x49 "$r" b || break
done
```

Forventet svar: `0xF0`–`0xF5` gir `0x00`, så `0x50 0x41 0x4c 0x49 0x54 0x20 0x31 0x35 0x30`, og `0xFF` gir `0x00`.

**3. Bare hvis 2 stemte: les fargen** (de fire registrene ThunderMaster leser):

```bash
for r in 0x6C 0x6D 0x6E 0x6F; do printf '%s ' $r; sudo i2cget -y "$N" 0x49 $r b || break; done
```

**Ikke kjør** `i2cdetect`, `i2cdump`, `i2cset`, `i2ctransfer` eller `i2cget -f` mot noen buss. Ikke kjør noe mot andre adresser.

### 2.7 Prøveplan for lys, med deg til stede

| Prøve | Hva | Hva vi ser etter | Ved feil |
|---|---|---|---|
| **L0** | Se på kortet. Skriv ned farge, effekt og produktkode | – | Lyser ingenting: lys = «ikke støttet», stopp |
| **L1** | Kommando 1–3 i 2.6 (du kjører) | «PALIT 150» og fire fargebyte | Stopp. Lim inn svaret |
| **L2** | Hjelperen med `lys` godkjent, men skriving sperret: `rgb_get` gir signatur og farge | Samme bytes som L1 | Stopp |
| **L3** | Første skriving **uten endring**: `static` med verdiene fra L2. Les tilbake | Uendret lys, lik tilbakelesing | Stopp. Strømbrudd hvis lyset endret seg |
| **L4** | Rød, så av, så pusting, så «Tilbakestill lyset». Til slutt kald start (strømbrudd 30 s) | Fargene stemmer. Etter kald start: notér hva kortet viser uten appen | Tilbakestill. Ellers kald start |
| **L5** | Regnbue i programvare i 5 min ved 10 Hz | Ingen feil i journalen og ingen `NVRM`-linjer i `journalctl -k` | Stopp effekten |

### 2.8 Endringer i koden og enheten (for 1.4.0)

- **Ny modul** `rigg-hw/src/gpu/lys.rs` med et trait `Lys` (`identify`, `read_color`, `apply(LysValg)`, `restore`):
  - `SimLys`: en falsk Palit-brikke i minnet med registerkart og feilinjeksjon (NACK, feil signatur, «PALIT 100», tilbakelesing som avviker).
  - `PalitLys`: den ekte, bak en egen cargo-funksjon `ekte-lys` og en egen konstant `EKTE_LYS`. Den skal ikke slås på samtidig med `ekte-skriving`.
- **Bussvalget** (2.4) som en ren funksjon over en sysfs-rot (`fn finn_adapter(root: &Path) -> Result<Adapter, Grunn>`). Testes mot falske sysfs-trær:
  - dagens maskin
  - numrene byttet om
  - to NVIDIA-kort
  - USB-C-adapteren med navn som ligner
  - en `0-0049`-klient
  - feil subsystem
- **Protokoll:** `rgb_get` og `rgb_set` med `mode`, farger, lysstyrke og fart. Ingen registeradresser fra klienten. `check_ranges` utvides.
- **Nødregel for lys:** ingen. Lys påvirker ikke temperaturen, men regnbuetakten stoppes når forbindelsen lukkes.
- **Enheten:** I dag har den `DevicePolicy=closed` og bare NVIDIA-enheter. Forslag [Antakelse, må prøves]:
  - `aktiver.sh` legger inn en udev-regel `/etc/udev/rules.d/70-agentic-rigg-i2c.rules`:
    `SUBSYSTEM=="i2c-dev", ATTR{name}=="NVIDIA i2c adapter 1 at *", KERNELS=="0000:*", ATTRS{vendor}=="0x10de", ATTRS{device}=="0x1e87", ATTRS{subsystem_vendor}=="0x10de", ATTRS{subsystem_device}=="0x1e87", SYMLINK+="agentic-rigg/gpu-i2c-port1"`
  - Enheten får `DeviceAllow=/dev/agentic-rigg/gpu-i2c-port1 rw`.
  - Da slipper cgroup-filteret bare den ene minor-en til, uansett nummer. systemd slår opp stien når tjenesten starter. Det må prøves at en symlenke virker der. Virker det ikke, er reserven `DeviceAllow=char-i2c rw` pluss kodesjekkene i 2.4. Den er svakere, fordi den åpner for alle I2C-busser i cgroup-filteret.
  - Regelen fjernes av `deaktiver.sh`. Den gir **ikke** brukere tilgang (ingen `MODE`/`GROUP`/`uaccess`).
- **Tester mot sim:**
  - signatur riktig/feil/«PALIT 100»
  - NACK midt i
  - hvitelisten (alle 256 registre prøvd, bare de godkjente slipper gjennom)
  - takten (11. oppdatering i samme sekund avvises)
  - tilbakelesing som avviker låser lys
  - `reset` og frakobling gir `opprinnelig`
  - ikke-godkjent `lys` gir `step_not_enabled`
  - kildesøket i `bare_lesing.rs` utvides (I2C-ioctl bare i `lys.rs`, bare hjelperen bruker den)

### 2.9 Kjente problemer og advarsler

- **OpenRGB-README:** OpenRGB bruker reverserte protokoller. Tidligere versjoner har skadet eller «bricket» maskinvare, og koden bak er siden fjernet eller rettet. Jeg fant ingen dokumentert sak der et **skjermkort** ble ødelagt av en I2C-lesing. Det jeg fant, gjaldt RAM og hovedkort på SMBus, og var for det meste udokumentert.
- **NVIDIA-driverens I2C:** word- og blokkoverføringer var feil fram til 565.77 ([#41](https://github.com/NVIDIA/open-gpu-kernel-modules/issues/41)). Det er rapportert igjen på enkelte kort med 572.x, uavklart. Vi bruker bare byte data.
- **`i2c-dev`:** innebygd her, så det er ingenting å laste. På andre maskiner krever OpenRGB `modprobe i2c-dev` og udev-regler for gruppen `i2c`. Det trenger vi ikke, fordi hjelperen er root.
- **Moduler som lastes senere** (for eksempel `i2c-nct6775`) kan endre nummereringen. En Gigabyte-bruker mistet RGB-styringen etter at en slik modul ble lastet. Det er grunnen til at adapteren alltid finnes på nytt ved start.
- **Feil buss** kan treffe DDC (EDID på 0x50, DDC/CI på 0x37), spenningsregulatoren eller USB-C-kontrolleren. Vi rører bare port 1 og bare 0x49, og bare etter signaturen.

---

## 3. B · Ekte NVML-skriving

### 3.1 Kallene

Funksjonsnavnene er sjekket mot `nvml.h` i go-nvml (`NVML_API_VERSION_STR "13"`).

| Formål | Kall | Rettighet ifølge `nvml.h` | Turing / Linux / 610 | Merknad |
|---|---|---|---|---|
| Effektgrense | `nvmlDeviceSetPowerManagementLimit(dev, mW)` (eller `…Limit_v2` med `nvmlPowerValue_v2_t`) | «Requires root/admin permissions» | **Ja** (Kepler+). Området 105–275 W er målt | «Not persistent across reboots or driver unloads». v1 er ikke merket utfaset, men v2 «replaces» den |
| Standardgrense | `nvmlDeviceGetPowerManagementDefaultLimit` | Lesing | Ja, 245 W | Dette er fasiten for «standard», ikke det vi leste ved oppstart |
| Fast viftefart | `nvmlDeviceSetFanSpeed_v2(dev, fan, %)` | Gruppen krever root | **Ja** (Maxwell+). LACT bruker den på GeForce, også RTX 2080 Super | Setter policy til manuell («you can burn your GPU!»). Under VBIOS-minimum gir det `INVALID_ARGUMENT` |
| Vifte tilbake til Auto | `nvmlDeviceSetDefaultFanSpeed_v2(dev, fan)` | Gruppen krever root | Ja | Det LACT bruker. En forumtråd fra 2022 sier at den ikke alltid slipper kurven igjen. Må verifiseres med policy og fart under last |
| Viftepolicy | `nvmlDeviceSetFanControlPolicy(dev, fan, policy)`; `0 = TEMPERATURE_CONTINOUS_SW`, `1 = MANUAL` | «Requires privileged user» | Maxwell+ | **Reserve** hvis `SetDefaultFanSpeed_v2` ikke gir policy 0. Ikke i `allowed_write` i dag |
| Klokke-offset | `nvmlDeviceSetClockOffsets(dev, nvmlClockOffset_t*)` med `type` og `pstate` | «Requires privileged user» | **Ja** (Maxwell+, NVML 555+) | `nvmlDeviceSetGpcClkVfOffset`/`…MemClkVfOffset` er `DEPRECATED(13.0)`, så de brukes ikke |
| Temperaturmål | `nvmlDeviceSetTemperatureThreshold(dev, ACOUSTIC_CURR=5, int*)` | Ingen merknad i `nvml.h`, men gruppen krever root | **Usikkert.** LACT har det (`reset_target_temp`), men et MX130 fikk «unsupported», og NVIDIA svarte at det ikke finnes API for maksgrensen | Må prøves. `NOT_SUPPORTED` gir «Ikke støttet på dette kortet» |
| Temperatur | `nvmlDeviceGetTemperature` | Lesing | Virker | `DEPRECATED(13.0)`. Bytt til `nvmlDeviceGetTemperatureV` |

**Fire funn som gjelder akkurat oss:**

1. **Offset rapporteres ikke alltid tilbake.** LACT har en egen løsning («On some driver versions, the applied offset values are not reported»): den husker siste verdi. En NVIDIA-forumtråd (GTX 1650 Super og 4060 Laptop, 565.77) sier det samme. Den sier også at offset for én P-tilstand slår inn for alle. Tilbakelesing av offset er derfor **ikke** bevis. Bevis er `Max Clocks`/klokke under last.
2. **Offset per P-tilstand:** `skriv_nvml.rs` skriver bare P0. LACT skriver hver P-tilstand for seg. Vi beholder P0, leser P0/P2/P5/P8 etterpå og noterer resultatet (prøve K1).
3. **Minne-offset i NVML er dobbel MHz.** `skriv_nvml.rs` ganger med 2 og deler grensene på 2. Det er riktig.
4. **Viftekurve** finnes ikke i fastvaren. Hjelperen kjører den (7B), og det er det samme LACT gjør.

### 3.2 Rettigheter og herding

**Driveren krever `CAP_SYS_ADMIN`.** I NVIDIAs åpne kjernemodul er `os_is_administrator()` lik `NV_IS_SUSER()`, som er lik `capable(CAP_SYS_ADMIN)` (`kernel-open/common/inc/nv-linux.h` og `kernel-open/nvidia/os-interface.c`). Hjelperen kjører som uid 0, men `CapabilityBoundingSet=CAP_CHOWN` tar fra den alt annet. Da vil alle set-kall gi `NVML_ERROR_NO_PERMISSION` (4). Det gjør det ubesvarte punktet 3 i `hjelper-sikkerhet.md` kap. 6 til en kjent sak.

**Enheten for 1.4.0** (endringene fra 1.2.0):

```ini
CapabilityBoundingSet=CAP_CHOWN CAP_SYS_ADMIN
AmbientCapabilities=
NoNewPrivileges=yes
# NVML leser /proc/driver/nvidia/params, /proc/devices, /proc/cpuinfo og
# /proc/meminfo (strings i libnvidia-ml.so.610.43.02). ProcSubset=pid
# skjuler dem, så den må bort. ProtectProc=invisible beholdes.
#ProcSubset=pid
StateDirectory=agentic-rigg
StateDirectoryMode=0700
```

Det andre beholdes: `ProtectSystem=strict`, `ProtectHome`, `PrivateNetwork`, `DevicePolicy=closed` med bare `char-nvidia`/`char-nvidiactl`/`char-nvidia-uvm`, `SystemCallFilter`, `MemoryDenyWriteExecute` og `RestrictAddressFamilies=AF_UNIX`.

`CAP_SYS_ADMIN` er et bredt privilegium. Det demmes inn av:

- **Ingen filstier, ingen exec, ingen nett:** `ProtectSystem=strict`, `PrivateNetwork`, `RestrictNamespaces`, `SystemCallFilter=~@mount @module @raw-io @privileged`
- **Bare skjermkortets enheter** (`DevicePolicy=closed`)
- **Ingen nye prosesser med rettigheter** (`NoNewPrivileges`)
- **Protokollen,** som bare tar tall

`systemd-analyze security` vil gi en dårligere poengsum (anslagsvis 1,2 → ca. 2,5). Det skal skrives inn i `hjelper-sikkerhet.md`.

**Ting som må prøves i den ekte enheten før første skriving (B0)** [Antakelse, ikke målt i sandkassen ennå]:

- `nvmlInit_v2` med `ProcSubset=pid` fjernet (og helst først med den på, for å bekrefte at den er problemet)
- `TasksMax=4`: `libnvidia-ml` kan starte tråder (`pthread_create` finnes i biblioteket). Øk til 8 hvis init feiler
- `MemoryMax=64M`: NVML legger til ca. 24 MB RSS (målt i revisjonen). Det bør holde
- `MemoryDenyWriteExecute=yes` og `SystemCallFilter`: følg med på `journalctl -u agentic-rigg-helper` etter `SIGSYS`/`EPERM`
- `/dev/nvidia-caps` trengs ikke (bare MIG og profilering). `nvidia-modprobe` skal ikke trengs, fordi nodene finnes

### 3.3 Wayland, X11 og Coolbits

- **NVML trenger verken X11 eller Coolbits.** Det snakker med `/dev/nvidia*` direkte. LACT sier at den virker uten grafisk økt, og fungerer på Wayland. På en RTX 2080 Super (Colorful, Ubuntu 24.04, 550) feilet `nvidia-settings` med «Unknown Error», mens LACT styrte vifte og effekt.
- **Coolbits** er en X-konfig for NV-CONTROL. Den virker ikke på Wayland, og installert `nvidia-settings` (510.47.03) er eldre enn NVML-overgangen i 570. GreenWithEnvy bruker NV-CONTROL og er arkivert. Ingen av dem er aktuelle.

### 3.4 Standardverdier: lese og gjenopprette

| Verdi | Fasit | Gjenopprett med |
|---|---|---|
| Effekt | `GetPowerManagementDefaultLimit` (245 W), alltid fra driveren | `SetPowerManagementLimit(default)` |
| Vifter | Policy 0 (driveren) | `SetDefaultFanSpeed_v2(i)` per vifte, sjekk `GetFanControlPolicy_v2(i) == 0`. Ellers `SetFanControlPolicy(i, 0)` |
| Offset | 0 for kjerne og minne | `SetClockOffsets(…, 0)` for P0 (og P2/P5/P8 hvis K1 viser at de ble endret) |
| Temperaturmål | **Problemet:** NVML har ingen «standard» for ACOUSTIC_CURR | Første verdi lest av hjelperen etter aktivering lagres i `/var/lib/agentic-rigg/standard.json` (0600 root) **før første skriving**, sammen med driverversjon og VBIOS. Gyldig bare med samme VBIOS. Er den ulik 83 °C ved første lesing: varsel i status («temperaturmålet var allerede endret»). LACT gjør det samme (husker første lesing) |

Fila skrives bare én gang, og leses ved hver oppstart. `deaktiver.sh` fjerner den. Skrivingen er atomisk: `tmp` + `rename` + `fsync`.

### 3.5 Driverkrasj, GPU tapt og hvilemodus

- **Ikke varig:** effektgrense, offset og viftepolicy forsvinner ved omstart eller når driveren lastes ut [Kilde: `nvml.h`]. Persistence mode er av. Så lenge skrivebordet bruker kortet, blir driveren lastet. **Omstart er alltid en full tilbakerulling.**
- **GPU tapt (Xid 79 og lignende):** kallene gir `NVML_ERROR_GPU_IS_LOST` (15). Hjelperen skal da:
  - gå i tilstanden «kortet svarer ikke»
  - slutte å skrive og avvise `apply`
  - forsøke `nvmlShutdown`
  - avslutte med kode ≠ 0, så systemd starter den på nytt (`Restart=on-failure`)
  - ved ny start åpne NVML på nytt og bare lese
  - **aldri** legge noe på igjen automatisk
- **Etter en uren stans** (hjelperen fant `/run/agentic-rigg/aktiv` fra forrige gang) skal `riggd` heller ikke legge profiler på igjen automatisk. Du må trykke selv.
- **Hvilemodus:** Hjelperen har ingen D-Bus. Den kan oppdage at maskinen har sovet ved å sammenligne `CLOCK_BOOTTIME − CLOCK_MONOTONIC` i hver runde. Et hopp over 2 s betyr at den har sovet. Da:
  - leses alt tilbake
  - går alt som ikke er bekreftet, til standard
  - legges bekreftede verdier på igjen bare mens forbindelsen som eier dem, er åpen
  - logges «Etter hvile» i journalen
  - Om driveren selv nullstiller grensen etter resume (`nvidia-resume.service`), vet vi ikke [ikke målt]. Det prøves i B1-sjekklisten.
- **Krasj i hjelperen:** `ExecStopPost=--tilbakestill` setter alt til standard, slik det er i dag.

### 3.6 Funn i `skriv_nvml.rs` som bør rettes før trinn B1

1. **Halvveis `apply`:** feiler for eksempel offset etter at effekten er satt, blir `self.applied` ikke oppdatert. Hjelperen tror da at kortet har gamle verdier. Det bør oppdateres felt for felt, eller rulles tilbake det som rakk å bli satt, med en test i sim (feilinjeksjon i `SimSkriver`).
2. **Feilkoder:** `call!` gir bare tallet. Map 2/3/4/15 til `invalid_argument`, `not_supported`, `no_permission` og `gpu_lost`, så vinduet kan si hva som skjedde.
3. **`allowed_write`:**
   - Legg til `nvmlDeviceGetFanControlPolicy_v2`, `nvmlDeviceSetFanControlPolicy` (reserven), `nvmlDeviceGetPowerManagementLimit` (tilbakelesing) og `nvmlDeviceGetTemperatureV`.
   - Fjern `nvmlDeviceGetTemperature` når V-varianten er i bruk.
4. **Standard for temperaturmålet** fra `standard.json`, ikke fra ACOUSTIC_CURR ved hver oppstart (3.4).
5. **Sperren per kall.** I dag åpner `EKTE_SKRIVING` alt på én gang. Foreslått:
   - `pub const EKTE_TRINN: &[Step] = &[Step::Effekt];` i 1.4.0-bygget for B1, utvidet runde for runde
   - kallene utenfor listen gir `not_supported` med `reason = "not_yet_enabled"`
   - testen `sperren_er_av_i_denne_versjonen` byttes ut med en test av listen
6. **Lesemodus:** `open_nvml_les()` åpner NVML og leser grenser og nåverdier uten noen set-kall. Den brukes i B0, og alltid før skrivedelen tas i bruk.
7. **Tilbakelesing etter hver skriving:**
   - effekt: `GetPowerManagementLimit == satt`
   - vifter: policy og målfart
   - temperaturmål: ACOUSTIC_CURR
   - offset: bare «uten feilkode» (se 3.1, punkt 1)
   - Avvik gir `revert` og en tydelig feil.

### 3.7 Trinnplan for 1.4.0

Felles for alle trinn:

- Én ny ting per bygg.
- Ingen trinn uten at det forrige er grønt med deg til stede.
- Alle tester mot sim kjøres med `cargo test --workspace` og `cargo clippy -D warnings` (`CARGO_BUILD_JOBS=6`, `CARGO_TARGET_DIR=$HOME/.cache/rigg-build`, minst 6 GB ledig).

**Avlesninger før og etter** (bare lesing, kjøres i et eget terminalvindu):

```bash
nvidia-smi -q -d POWER,CLOCK,TEMPERATURE,PERFORMANCE > for.txt      # før
nvidia-smi -q -d POWER,CLOCK,TEMPERATURE,PERFORMANCE > etter.txt    # etter
diff for.txt etter.txt
python3 -I docs/produksjon/verktoy/nvml_probe.py | grep -E "Offset|policy|Akustisk|Effektgrense|Håndhevet|Viftefart"
nvidia-smi --query-gpu=power.limit,enforced.power.limit,fan.speed,clocks.gr,clocks.mem,temperature.gpu,pstate,clocks_event_reasons.sw_power_cap --format=csv -l 1
journalctl -u agentic-rigg-helper -n 50
journalctl -k -g NVRM --since "-10 min"     # Xid-er (du er i gruppen adm)
```

Felt å sammenligne i `nvidia-smi -q`:

- **POWER:** `Current Power Limit`, `Requested Power Limit`, `Default Power Limit`, `Min/Max Power Limit`, `Instantaneous Power Draw`
- **CLOCK:** `Graphics`, `Memory`, `Max Clocks Graphics/Memory`
- **TEMPERATURE:** `GPU Current Temp`, `GPU Target Temperature Specification`, `GPU Max Operating Temp`, `Slowdown`, `Shutdown`
- **PERFORMANCE:** `Performance State`, `Clocks Event Reasons` (særlig `SW Power Cap`, `SW Thermal Slowdown`), `Clocks Event Reasons Counters`

`GPU Target Temperature Specification` er kanskje spesifikasjonen, ikke ACOUSTIC_CURR. For temperaturmålet er det `nvml_probe.py` («Akustisk mål») som gjelder.

**Tilbakerulling, i denne rekkefølgen:**

1. **I appen:** «Rull tilbake» før fristen, eller la fristen gå ut (10–60 s).
2. **I appen:** «Tilbakestill alt» (`reset`) gir standard og Auto.
3. **I appen:** trekk tilbake trinnet (passord). Kortet går til standard innen 1 s.
4. **Slå av bryteren** (`deaktiver.sh`). Tjenesten stopper, og `ExecStopPost` setter standard.
5. **Fra terminal hvis appen ikke svarer** (du kjører selv): `sudo systemctl stop agentic-rigg-helper`. Det samme som 4, uten å fjerne filene.
6. **Bare effekt, hvis hjelperen ikke kan startes:** `sudo nvidia-smi -pl 245`.
7. **Alt annet:** start maskinen på nytt. Ingenting i NVML er varig.

#### B0 · Forarbeid (ingen skriving)

- **Kode:**
  - enheten (3.2)
  - `StateDirectory` og `standard.json` (3.4)
  - `open_nvml_les` (3.6.6)
  - feilkodene (3.6.2)
  - `apply` felt for felt (3.6.1)
  - `EKTE_TRINN = &[]` (3.6.5)
  - `ekte-skriving` slås på for hjelperen i release-bygget, men listen er tom
  - status viser grensene lest fra kortet (`status.limits`, `status.stock`, `status.policy`)
- **Tester mot sim:**
  - halvveis `apply` med feilinjeksjon
  - feilkode → riktig `ErrorCode`
  - `standard.json` skrives én gang, endres ikke ved ny start, avvises med feil VBIOS, og `deaktiver.sh` fjerner den (i `skript.rs`)
  - enheten har `CAP_SYS_ADMIN` og ikke `ProcSubset=pid` (`polkit_og_enheten_er_som_avtalt`)
  - `EKTE_TRINN` tom gir `not_yet_enabled` for alle felt
  - kildesøket: set-kall bare i `skriv_nvml.rs`
- **Første ekte prøve, med deg:**
  - [ ] Du slår på bryteren med passord. Status: «På · trinn: ingen».
  - [ ] `journalctl -u agentic-rigg-helper`: NVML åpnet, ingen `SIGSYS`/`EPERM`, grensene 105–275 W, 35–100 %, ±1000 / −1000…+3000, 65–88 °C.
  - [ ] `/var/lib/agentic-rigg/standard.json` har 245 W og 83 °C.
  - [ ] Før og etter (`nvidia-smi -q …`) er like, bortsett fra tid og forbruk.
  - [ ] `systemd-analyze security agentic-rigg-helper` er notert.
  - Ved feil: slå av bryteren. Ingenting er skrevet.

#### B1 · Effektgrense

- **Kode:** `EKTE_TRINN = &[Step::Effekt]`, men bare feltet `power_limit_w`. Temperaturmålet venter til B3, selv om det hører til samme polkit-trinn.
- **Tester mot sim:**
  - 105 og 275 slipper gjennom
  - 104,9 og 275,1 avvises
  - fristen ruller tilbake til 245
  - frakobling gir 245
  - tilbakelesing som avviker gir `revert`
  - etter hvile (falsk `BOOTTIME`-hopp) gir standard for ubekreftet verdi
- **Første ekte prøve, med deg:**
  - [ ] Før-avlesning.
  - [ ] Godkjenn `effekt` (passord).
  - [ ] 245 → 225 W, **la fristen gå ut**. `power.limit` viser 225, så 245 innen fristen.
  - [ ] 245 → 225 W, **Behold**. `Current/Requested Power Limit` er 225 W.
  - [ ] Under last (spill eller benchmark): `Instantaneous Power Draw` ≤ ca. 225 W, og `SW Power Cap: Active` i perioder.
  - [ ] «Tilbakestill alt»: 245 W.
  - [ ] **Er `NO_PERMISSION` tilbake?** Da mangler `CAP_SYS_ADMIN` (3.2). Stopp og meld fra.
  - [ ] Hvile: sett 225 W og bekreft, la maskinen sove i 30 s, vekk den, og noter hva `power.limit` er. Sammenlign med det hjelperen sier.
  - [ ] Slå av bryteren: 245 W.

#### B2 · Vifter

- **Kode:** `EKTE_TRINN += Vifter`. Reserven `SetFanControlPolicy(i, 0)` hvis policy ≠ 0 etter `SetDefaultFanSpeed_v2`. Nødregelen (85 °C inn, 80 °C ut) er uendret.
- **Tester mot sim:**
  - 35 slipper gjennom
  - 34 avvises
  - policy blir stående manuell etter `SetDefault` (feilinjeksjon), og reserven gir policy 0
  - nødregelen gir 100 %
  - kurven er lik `sim_vifter` (finnes)
  - SIGKILL av `riggd` gir Auto innen 3 s
- **Første ekte prøve, med deg:**
  - [ ] Før: fart 35 %, policy 0/0, ca. 800 o/min.
  - [ ] Fast 60 %, la fristen gå ut. 60 % / policy 1, så Auto / policy 0.
  - [ ] Fast 60 %, Behold. Begge vifter ca. 60 %, o/min stiger. Tilbakestill.
  - [ ] Kurve under last i 5 min: farten følger kurven, ingen hopp.
  - [ ] Auto under last: farten stiger av seg selv. **Viktig, på grunn av forumfeilen med `SetDefaultFanSpeed_v2`.**
  - [ ] `sudo systemctl kill -s KILL agentic-rigg-helper` med fast 60 %: systemd starter den på nytt, `ExecStopPost` setter Auto. Notér tiden.
  - [ ] Slå av bryteren: policy 0.

#### B3 · Temperaturmål

- **Kode:** `EKTE_TRINN` får feltet `temp_target_c`. `NOT_SUPPORTED` gir `capabilities.write.steps.temp_target = "unsupported"`, og vinduet skjuler kontrollen med «Ikke støttet av kortet».
- **Tester mot sim:**
  - 65/88 slipper gjennom, 64/89 avvises
  - `not_supported` gjør at status merker feltet
  - standard fra `standard.json`
- **Første ekte prøve, med deg:**
  - [ ] 83 → 78 °C, la fristen gå ut. «Akustisk mål» viser 78, så 83.
  - [ ] Hvis `NOT_SUPPORTED` eller `NO_PERMISSION`: notér det. Trinnet blir «ikke støttet», og vi går videre.
  - [ ] Hvis det virker: last i 10 min med Auto-vifter, se om temperaturen legger seg nær 78 og om viftene jobber hardere.

#### B4 · Klokke-offset

- **Kode:**
  - `EKTE_TRINN += Klokker`, kjerne og minne hver for seg i UI-et
  - tjenestens strengere område (−200…+300 kjerne, −1000…+1500 effektiv MHz minne) beholdes
  - fristen for klokker er minst 30 s
  - etter uren stans legges klokker **aldri** på automatisk
- **Tester mot sim:**
  - grensene
  - minne ×2 til NVML
  - fristen
  - tilbakelesing ignoreres for offset (bare feilkode teller)
  - etter uren stans gir `riggd` ingen automatisk `apply`
- **Første ekte prøve, med deg** (lagre alt arbeid først, siden Wayland-økta kan falle):
  - [ ] Kjerne +50 MHz, la fristen gå ut. Ingen feil, ingen `NVRM` i `journalctl -k`.
  - [ ] Kjerne +50, Behold. Under last er `clocks.gr` ca. 50 MHz høyere ved samme temperatur og effekt (sammenlign med før-målingen under samme last). `Max Clocks Graphics` endres kanskje.
  - [ ] Notér offset P0/P2/P5/P8 fra `nvml_probe.py` (rapporteres de, og gjelder de alle?).
  - [ ] Minne +200 effektiv MHz (NVML +400). `clocks.mem` under last.
  - [ ] Tilbakestill: offset 0, klokkene som før.
  - **Ved krasj eller frys:** start på nytt. Alt er borte, siden ingenting er varig, og `riggd` legger ikke på klokker etter uren stans.

---

## 4. ThunderMaster-paritet

ThunderMaster (Windows) oppgir i dag støtte for RTX 30/40/50 på Palits side. Opptakene i #1420 viser at en eldre versjon styrte lyset på ditt kort.

| Funksjon i ThunderMaster | Mulig på Linux med dette kortet? | Hvordan, og hva det koster | Risiko | Anbefaling |
|---|---|---|---|---|
| **Manuell kjerne- og minneklokke** | Ja (`SetClockOffsets`) | B4. Lite arbeid, skrivekoden finnes | Middels: krasj, ingen skade | **Nå (1.4.0)** |
| **OC Scan** | **Ingen NVIDIA-skanner på Linux.** NVIDIA Scanner går gjennom NVAPI på Windows. LACT har ingen skanner for NVIDIA, bare manuell VF-kurve | En egen skanner: offset +15 MHz per steg under last, se etter Xid i `journalctl -k`, feil i en regnelast med kjent svar, og bildefeil (kan ikke oppdages). Trenger egen last (Vulkan compute i appen, eller eksterne `memtest_vulkan`/`gpu-burn`), 20–60 min per kjøring og en egen modus som tåler at økta krasjer. **Stort arbeid** | **Høy for økta:** Wayland-skrivebordet går på samme kort, og en Xid 79 krever omstart. Liten for kortet: spenningen er låst, effektgrensen gjelder, og ingenting er varig | **Senere, og bare som «veiledet test»**: appen foreslår neste steg, og du kjører last og ser etter feil. Ingen automatisk skanning i 1.4.0 |
| **Auto-kurve** (driverens) | Ja: policy 0 / `SetDefaultFanSpeed_v2` | Finnes (Auto) | Lav | **Nå** |
| **Fast fart** | Ja, 35–100 % | B2 | Lav | **Nå** |
| **Egendefinert kurve** | Ja, i hjelperen (7B) | B2 | Lav, med nødregel | **Nå** |
| **«Triple-fan mode»** (to vifter sammen, den tredje for seg) | Kortet har **2 vifter**. NVML gir dem hver sin indeks med egen o/min (ca. 801/798) | Kurve per vifte finnes i motoren (7B). Vis «Vifte 1 / Vifte 2 hver for seg» | Lav | **Nå,** som «vifter hver for seg» |
| **Viftestopp (Fan Stop)** | **Nei.** NVML-minimum er 35 % fra VBIOS (`GetMinMaxFanSpeed`), og `SetFanSpeed_v2` under minimum gir `INVALID_ARGUMENT`. TechPowerUps test av Palit RTX 2080 GamingPro OC sier at kortet ikke har viftestopp i tomgang (2080 *Super* GamingPro OC har det) | Den eneste veien er en annen VBIOS. Turing-VBIOS er signert, og en kryssflash kan gjøre kortet ubrukelig | Høy | **Ikke.** Vis «Ikke støttet på dette kortet: minste fart er 35 % (kortets BIOS)» |
| **Sanntidsovervåking: last, temperatur, klokker** | Ja, finnes i dag (NVML-lesing) | – | Ingen | Finnes |
| **FPS** | Ja, men bare gjennom **MangoHud** i spillet. GNOME Mutter har ingen API for bildefrekvens per vindu, og vi skal ikke hekte oss inn i spill | MangoHud skriver CSV og kaller `flush()` etter hver rad (`logging.cpp`). Appen lager `MangoHud.conf` med `output_folder=~/.local/state/agentic-rigg/fps`, `autostart_log=1` og `log_interval=500`, og leser siste linje i nyeste fil med inotify (fps, frametime). Kontrollsocketen (`control=`) kan bare slå HUD/logging/fcat av og på. Den sender ikke FPS. Med gamescope trengs `mangoapp`. `gamescope --stats-path` er ikke bekreftet. MangoHud er ikke installert (`sudo apt install mangohud`), og du starter spill med `mangohud %command%` | Ingen for kortet. Ikke sett `MANGOHUD=1` globalt, for da får også Rigg-vinduet (GTK4/Vulkan) overlegget | **Senere (1.5):** FPS-fliser fra MangoHud-loggen. I 1.4.0 er det nok å lage konfigen |
| **FPS-tak («Frame Rate Target»)** | Ja: `fps_limit=` i MangoHud | Konfiggenerator | Ingen | Senere, sammen med FPS |
| **Tilpassbart infopanel** | Ja | Finnes langt på vei | Ingen | Finnes |
| **6 profiler** | Ja, uten fast tak | `~/.config/agentic-rigg/profiler/*.json`. Profiler legges på gjennom hjelperen med fristen. En profil du allerede har bekreftet med de samme verdiene, kan `riggd` bekrefte selv [designvalg for hovedøkta] | Lav | **Nå:** 6 plasser som i ThunderMaster, eller fritt antall |
| **Hurtigtaster** (ThunderMaster-siden nevner ingen) | Ja. **GlobalShortcuts-portalen** finnes fra GNOME 48 (her GNOME Shell 50.1, `xdg-desktop-portal-gnome` 50.0). Den virker mens vinduet kjører. Uten vindu: en GNOME-snarvei som kjører `agentic-rigg --profil N` | Portal: moderat (D-Bus i vinduet). Snarvei: du legger den inn selv i Innstillinger › Tastatur. Appen skal ikke skrive i `gsettings` uten å spørre | Lav. Klokkeprofiler fra hurtigtast bør bare være bekreftede profiler | **Senere (1.5)** |
| **RGB-effekter** | Fast farge, av, lysstyrke og pusting i maskinvaren. Regnbue og «GPU-temperatur»-farge i programvare (ThunderMaster gjør det samme, og de kan ikke lagres på kortet der heller) | Kapittel 2 | Lav etter signaturen. Middels før | **Nå (1.4.0), etter L0–L5** |
| **Adresserbare LED-er** | **Nei.** Logoen er én sone (én RGB-trippel i alle opptak) | – | – | **Ikke** |
| **«Lagre på kortet» / standard ved oppstart** | **Nei.** Ingen kjent kommando for dette kortet (2.3). Palit skriver selv at noen modeller trenger ny BIOS for funksjonen | Erstatning: `riggd` legger på lyset når du logger inn (lys er lav risiko og trenger ingen frist) | Å prøve ukjente registre: **høy** | **Ikke** på kortet. Gjør det i programvare ved innlogging |

---

## 5. Anbefalt rekkefølge (oppsummert)

1. **B0:** enheten (`CAP_SYS_ADMIN`, uten `ProcSubset=pid`, `StateDirectory`), lesemodus, feilkoder, `apply` felt for felt og `EKTE_TRINN`. Prøv i den ekte enheten **uten** skriving.
2. **B1:** effektgrense.
3. **B2:** vifter (med Auto-prøven under last).
4. **B3:** temperaturmål (kan ende som «ikke støttet»).
5. **B4:** klokke-offset (kjerne, så minne).
6. **A:**
   - L0–L1 kan gjøres når som helst, fordi det bare er du som leser
   - lys-koden (sim) kan bygges parallelt med B1–B4
   - ekte `lys` (L2–L5) slippes som eget bygg, ikke samtidig med første ekte NVML-skriving
7. **ThunderMaster-paritet** i 1.4.0:
   - viftene hver for seg
   - profilplasser
   - MangoHud-konfig
   - teksten «ikke støttet» for viftestopp og «lagre på kortet»
   - FPS-fliser, hurtigtaster og veiledet OC kommer i 1.5

## 6. Hva du må gjøre selv

| Når | Hva |
|---|---|
| Nå | Se på kortet: lys, farge eller effekt, og produktkode (L0) |
| Når du vil | Eventuelt `sudo apt install i2c-tools`, så kommandoene i 2.6, og lim inn svaret (L1) |
| B0 | Slå på bryteren med passord. Se på journalen sammen med hovedøkta |
| B1–B4 og L2–L5 | Godkjenn hvert trinn med passord, vær til stede, og ha en last klar (spill eller benchmark). Lagre arbeid før B4 |
| Ved problemer | Tilbakerulling 1–7 i kapittel 3.7. Bare punkt 5–6 krever terminal og `sudo`, og de kjører du selv |
| Senere, valgfritt | `sudo apt install mangohud` for FPS. Legg inn en GNOME-snarvei for profiler selv |

---

## 7. Hva denne planen ikke gjør

- Ingen VF-kurve og ingen «voltage boost» via udokumentert NvAPI.
- Ingen låst kjerneklokke. Det er mulig senere (`SetGpuLockedClocks`, Volta+).
- Ingen PowerMizer-modus.
- Ingen skanning av I2C, ingen bruk av `0xE0–0xE6`, ingen «lagre på kortet» og ingen VBIOS-endring.
- Ingen udev-regel som gir brukere tilgang til `/dev/i2c-*`.

---

## 8. Kilder

**OpenRGB** (GitLab `CalcProgrammer1/OpenRGB`, commit `b9d903b544a4`, 2026-11-08)

- [`PalitGPUControllerDetect.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/PalitGPUController/PalitGPUControllerDetect.cpp): v1- og v2-testene, `port_id == 1`, 0x49, listen over kort
- [`PalitGPUv2Controller.h`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/PalitGPUController/PalitGPUv2Controller.h) og [`.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/PalitGPUController/PalitGPUv2Controller.cpp): registrene og skrivesekvensene
- [`pci_ids.h`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/pci_ids/pci_ids.h): `NVIDIA_RTX2080_A_DEV = 0x1E87`, `NVIDIA_RTX2080_DEV = 0x1E82`, `PALIT_SUB_VEN = 0x1569`, `GAINWARD_SUB_VEN = 0x10B0`
- [`i2c_smbus_linux.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/i2c_smbus/Linux/i2c_smbus_linux.cpp): port-ID fra «NVIDIA i2c adapter %hu at», PCI-ID-er fra sysfs
- [`NVIDIAIlluminationControllerDetect_Windows_Linux.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/NVIDIAIlluminationController/NVIDIAIlluminationControllerDetect_Windows_Linux.cpp): bare RTX 2080 FE (`10DE:12A6`)
- [`Documentation/SMBusAccess.md`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Documentation/SMBusAccess.md) og [README](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/README.md): `i2c-dev`, udev og advarselen om at maskinvare er blitt skadet
- [Issue #1420, Palit GamingPro OC RTX 2080](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1420), med opptakene [`nvapi_21928562.txt`](https://gitlab.com/CalcProgrammer1/OpenRGB/uploads/6202470b9b87cad3261f917c2c4e373f/nvapi_21928562.txt), [`blue_and_red_alternation.txt`](https://gitlab.com/CalcProgrammer1/OpenRGB/uploads/81f2597323304a508be232ce4562a4ee/blue_and_red_alternation.txt) og [`rainbow.txt`](https://gitlab.com/CalcProgrammer1/OpenRGB/uploads/dea549138ffee5d9269224f5b6a87052/rainbow.txt)
- [Issue #1938, Palit RTX 2080 Ti GamingPro OC](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1938), med opptaket [`nvapi_65640.txt`](https://gitlab.com/CalcProgrammer1/OpenRGB/uploads/d7766911e7d2fd1eea1df887732bac64/nvapi_65640.txt) og testversjonen
- [Issue #5856, Palit RTX 2080 Ti Dual](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/5856): nytt ønske fra 2026-09, ikke løst

**NVIDIA**

- [open-gpu-kernel-modules `nv-linux.h`](https://github.com/NVIDIA/open-gpu-kernel-modules/blob/main/kernel-open/common/inc/nv-linux.h) (`NV_IS_SUSER() = capable(CAP_SYS_ADMIN)`), [`os-interface.c`](https://github.com/NVIDIA/open-gpu-kernel-modules/blob/main/kernel-open/nvidia/os-interface.c) (`os_is_administrator`) og [`nv-i2c.c`](https://github.com/NVIDIA/open-gpu-kernel-modules/blob/main/kernel-open/nvidia/nv-i2c.c) (adapternavn og SMBus-funksjoner)
- [open-gpu-kernel-modules #41](https://github.com/NVIDIA/open-gpu-kernel-modules/issues/41): I2C-feil med OpenRGB, rettet i 565.77
- [`nvml.h` (go-nvml)](https://github.com/NVIDIA/go-nvml/blob/main/gen/nvml/nvml.h): signaturer, rettigheter, `DEPRECATED(13.0)`, «not persistent»
- [NVML Device Commands](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceCommands.html)
- [Forum: SetClockOffsets/GetMinMaxClockOfPState](https://forums.developer.nvidia.com/t/nvmldevicegetminmaxclockofpstate-nvmldevicesetclockoffsets-issues/318332)
- [Forum: SetDefaultFanSpeed_v2 slipper ikke kurven](https://forums.developer.nvidia.com/t/nvmldevicesetdefaultfanspeed-v2-does-not-resume-fan-speed-algorithm-please-fix/214430)
- [Forum: API for temperaturgrense](https://forums.developer.nvidia.com/t/is-there-an-api-to-change-the-current-maximum-gpu-temperature-limit/287426)
- [Forum: RTX 2080 Super, nvidia-settings feiler og LACT virker](https://forums.developer.nvidia.com/t/rtx-2080-super-cannot-control-fan-speed-on-colorful-igame-vulcan-error-assigning-gputargetfanspeed/373056)
- [Driver 565.77](https://www.nvidia.com/en-us/drivers/details/237587/)

**LACT** (MIT, v0.10.1)

- [`nvidia.rs`](https://github.com/ilya-zlobintsev/LACT/blob/master/lact-daemon/src/server/gpu_controller/nvidia.rs): viftekurve, `set_default_fan_speed`, offset per P-tilstand, `last_applied_offsets` og `reset_target_temp`
- [`suspend.rs`](https://github.com/ilya-zlobintsev/LACT/blob/master/lact-daemon/src/suspend.rs): ny oppstart etter hvile via logind
- [`lactd.service`](https://github.com/ilya-zlobintsev/LACT/blob/master/res/lactd.service): full root, uten herding
- [README](https://github.com/ilya-zlobintsev/LACT)

**Annet**

- [Palit ThunderMaster](https://www.palit.com/palit/thundermaster.php?lang=en): funksjoner, «Save RGB Control To The Board», 6 profiler, Fan Stop og OC Scan
- TechPowerUp, [Palit RTX 2080 GamingPro OC](https://www.techpowerup.com/review/palit-geforce-rtx-2080-gaming-pro-oc/): ingen viftestopp i tomgang. Siden ga 403 til verktøyet, så setningen er lest fra et søkeresultat. Til sammenligning: [2080 Super GamingPro OC](https://www.techpowerup.com/review/palit-geforce-rtx-2080-super-gaming-pro-oc/32.html) har viftestopp
- MangoHud: [README](https://github.com/flightlessmango/MangoHud) (`output_folder`, `autostart_log`, `log_interval`, `control=`, `fps_limit`), [`logging.cpp`](https://github.com/flightlessmango/MangoHud/blob/master/src/logging.cpp) (flush per rad) og [`control.cpp`](https://github.com/flightlessmango/MangoHud/blob/master/src/control.cpp) (bare hud/logging/fcat)
- [GNOME 48 for utviklere](https://release.gnome.org/48/developers/): GlobalShortcuts-portalen
- Tidligere arbeid i prosjektet: `research/linux-gpu-kontroll.md` kap. 4, `maskinvare-revisjon.md` kap. 8–9 og `hjelper-sikkerhet.md` kap. 6

## Metode og sikkerhet i denne runden

- **Kjørt på maskinen, bare lesing:**
  - `lspci -nnv -s 1c:00.0`
  - lesing av `/sys/bus/i2c/devices/*/name`, `/sys/class/i2c-dev/*/name`, `readlink -f` på adapterne, og `vendor`/`device`/`subsystem_*`/`class`/`driver` under `/sys/bus/pci/devices/0000:1c:00.*`
  - `ls -l /dev/i2c-*` (bare `stat`, ingen `open`)
  - `/proc/modules`, `modules.builtin`, `/boot/config-*`, `/proc/devices`, `/proc/driver/nvidia/params`
  - `udevadm info -a -n /dev/i2c-0` (leser udev-databasen og sysfs, åpner ikke enheten)
  - `nvidia-smi -q` og `-q -d POWER,CLOCK,TEMPERATURE,PERFORMANCE`
  - `docs/produksjon/verktoy/nvml_probe.py` (bare Get)
  - `strings` på `libnvidia-ml.so.610.43.02`
  - `systemctl list-unit-files`, `gnome-shell --version`, `dpkg-query`, `sysctl kernel.dmesg_restrict`
- **Ikke gjort:** Ingen `/dev/i2c-*` er åpnet. Ingen `i2c*`-verktøy (de er ikke installert). Ingen NVML-set, ingen `nvidia-smi` med set-flagg, ingen `pkexec`/`sudo`/`systemctl start|stop`, ingen moduler lastet, og ingen prosesser rørt.
- **Kilder fra nettet:** lest til stdout med `curl` og analysert i minnet. Ingenting er lagret i prosjektet, og ingenting er bygget eller kjørt. OpenRGB-kode er bare lest for protokollfakta. Ingen GPL-kode er kopiert.
