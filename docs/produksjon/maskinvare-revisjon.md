# Maskinvarerevisjon – hva som faktisk virker på denne maskinen

*Målt 7. november 2026 for produksjonsbygget (Rust/GTK4). Palit RTX 2080 Rev. A (TU104, `10de:1e87`/`10de:1e87`), driver 610.43.02 (NVIDIA Open Kernel Module, GSP-fastvare på), NVML 13.610.43.02, CUDA 13.3. AMD Ryzen 7 2700X, MSI B450 Gaming Pro Carbon AC (MS-7B85), BIOS 1.60, 48 GB RAM, Ubuntu 26.04.1 LTS, kjerne 7.0.0-31, GNOME på Wayland.*

**Metode.** Alt er lest uten root og uten å skrive noe til skjermkortet. NVML er kalt direkte med bare `nvmlInit_v2`, `nvmlDeviceGetHandleByIndex_v2`, `nvmlSystemGet*`, `nvmlDeviceGet*` og `nvmlShutdown`. Ingen I2C-enhet er åpnet. SPD-filene (`ee1004`) er ikke lest, bare listet med `stat`. Verktøyene ligger i `verktoy/` (se siste kapittel), og alle tall under kan gjentas med dem.

**Tid per kall** står som «i ro / under last»: første tall er målt med kortet i P8 (300 MHz, ca. 12 W), andre med kortet i P0 (2010 MHz). Tidene er veggtid i ett kall, median av mange.

---

## 0. Kortversjon

1. **NVML gir mer enn demoen viser.** Viftens turtall (≈ 800 o/min ved 35 %) og to separate vifter virker, det samme gjør målfart, viftepolicy, energiteller, klokkeoffset-områder, klokkeområde per P-tilstand, «bussbelastning» (dynamiske P-tilstander), bruddtellere for nedklokking og prosesslista med VRAM per prosess. Demoen sier i dag at turtall «ikke finnes» og at antall vifter «ikke oppgis».
2. **Ikke støttet på kortet:** hotspot-temperatur, minnetemperatur, T.Limit/margin, kjernespenning, gjennomsnittseffekt fra driveren, applications clocks (avviklet), serienummer, board part number (tomt), ECC, NVLink (ingen lenker), auto boost-flagget.
3. **NVML er dyrt i ro.** Når kortet står i P8, koster et kall typisk 2–5 ms, og `nvmlDeviceGetCurrentClocksEventReasons` koster **25 ms**. Hele demorunden (19 kall) tar **54–62 ms i P8** og **10–11 ms i P0**. Tiden er ren CPU-tid i kjernen (`sys`), ikke venting.
4. **Tett polling vekker kortet.** 50 runder rett etter hverandre løfter kortet fra P8/12 W til **P0/2010 MHz/55–69 W** på under ett sekund. Polling hvert sekund og hvert halve sekund holder kortet i P8. Ved 10 Hz kryper det opp mot P5 etter ca. 9 s. Appen må aldri polle tett.
5. **Oppstart:** `dlopen` 0,4–0,8 ms, `nvmlInit_v2` **12–21 ms**, handle **5,7–7,6 ms**, faste opplysninger ca. **50 ms** i P8, første pollerunde 17–60 ms. NVML må derfor startes på en egen tråd etter at vinduet er vist. NVML øker RSS med **ca. 24 MB** (20 MB anonymt minne og `libcuda.so`, som NVML laster selv).
6. **CPU- og systemkildene er nesten gratis:** alt som trengs per sekund fra `/proc` og `/sys` koster under 0,3 ms til sammen med åpne filer og `pread`.
7. **Feil i demoen:** CPU-temperaturen i sidefeltet, på Oversikt og i Grafer er **Tctl**, som ligger nøyaktig **10,0 °C over Tdie** på 2700X (målt 47,375 mot 37,375 °C). «Buffer og cache» tar med 4,4 GiB `Shmem` uten å si at den delen ikke kan frigjøres. Merknaden om at SPD bare kan leses av root, er feil (se kapittel 10).
8. **Sikkerhetsfunn: I2C-numrene har flyttet seg siden researchen.** USB-C-kontrolleren (`ccgx-ucsi`, adresse 0x08) ligger nå på **`i2c-10`**, ikke `i2c-8`. `i2c-8` er nå hovedkortets SMBus (PIIX4 port 2). Regelen «åpne aldri `i2c-8`/0x08» i `REGLER.md` treffer derfor feil buss. Bussen må alltid finnes ut fra navn og sysfs-sti, aldri ut fra nummer (kapittel 9).

---

## 1. Måletall for NVML

| Måling | Resultat | Verktøy |
|---|---|---|
| `dlopen("libnvidia-ml.so.1")` | 0,41–0,75 ms | `nvml_timing` |
| `nvmlInit_v2` | 12,0–20,6 ms (C, 8 kjøringer), 11,9–13,0 ms (Python) | `nvml_timing`, `nvml_probe.py` |
| `nvmlDeviceGetHandleByIndex_v2` | 5,7–7,6 ms | samme |
| `nvmlShutdown` | 1,2–1,5 ms | `nvml_timing` |
| RSS før/etter init | 1,6 MB → 26,4 MB (C). Python: RssAnon +20,5 MB, RssFile +3,1 MB, `libcuda.so` 4,1 MB | `nvml_timing`, smaps |
| Full demorunde, 19 kall, i ro (P8) | 54–62 ms, median ≈ 58 ms | `nvml_timing watch 1000 45` |
| Full demorunde under last (P0) | 9,6–13,5 ms, median 10,3–12,0 ms | `nvml_timing` |
| CPU-tid, full runde hvert sekund i 10 s | `sys` 0,59 s = **5,9 % av én kjerne** | `/usr/bin/time` |
| CPU-tid, slank runde (10 kall, nær forslaget i kap. 12) hvert sekund | 17 ms per runde, `sys` 0,19 s / 10 s = **1,9 %** | `nvml_timing lean 1000 10` |
| Demoens `nvidia-smi -lms 500` i 12 s | `sys` 1,14 s = **9,5 % av én kjerne**, RSS 28 MB, bare 22 av 24 linjer (hver prøve tar > 500 ms) | `/usr/bin/time` |
| Én `nvidia-smi --query-gpu` | 40–46 ms veggtid, 30 ms `sys` | samme |
| `nvidia-smi -q` | 100 ms | samme |

**Pollingen påvirker kortet** (`nvml_timing watch MS SEK`):

| Intervall | P-tilstand | Kjerneklokke | Effekt |
|---|---|---|---|
| Tett (50 runder etter hverandre) | P0 innen 0,5 s | 2010 MHz | 55–69 W |
| 100 ms (runden tar 55 ms) | P8, så P5 etter 9 s | 300 → 600 → 690 MHz | 12–13 W |
| 500 ms | P8 hele tiden | 300 MHz | 12,0–12,9 W |
| 1000 ms | P8 hele tiden | 300 MHz (420 én gang) | 12,6–13,2 W |

Forklaringen er sannsynligvis at driveren på Turing med GSP-fastvare sender hver spørring som et RPC-kall til GSP på kortet, og at kortet ser dette som aktivitet [antakelse, men tallene stemmer med det]. Konsekvens: **appen skal polle høyst 2 Hz og aldri løse ut mange runder etter hverandre**, heller ikke ved oppstart.

**Kostnadsklasser for NVML-kall** (P8 / P0):

| Klasse | Kall | Kost |
|---|---|---|
| Hurtigbufret i biblioteket | navn, UUID, VBIOS, InfoROM, PCI-info, grenseområde, standardgrense, akustisk min/maks, nedklokkingsterskel, antall vifter, kjølerinfo, compute capability, minneinfo, BAR1, støttede klokkehendelser, GSP-versjon, PCIe-bredde, Encoder-økter | 0,01–0,1 ms |
| Lett RPC | temperatur, P-tilstand, effektgrense, håndhevet grense, viftepolicy, klokkeoffset, PCIe-generasjon, persistence, prosesslister | 0,3–0,9 ms / 0,1–0,2 ms |
| Tung RPC | effekt, energi, klokker, last, encoder/decoder, viftefart, målfart, turtall, maks klokker, klokkeområde per P-tilstand, bruddtellere, dynamiske P-tilstander | 1,5–6 ms / 0,4–2 ms |
| Svært tung | klokkehendelser 24–27 ms / 0,7 ms · Display mode 11–14 ms · PCIe-gjennomstrømning 20–21 ms (fast måleperiode) · ytelsesmodi 9 ms · feltverdier (9 felt) 3–10 ms · last per prosess 1–4 ms | – |

`nvmlDeviceGetFieldValues` samler ikke kallene billigere. Ni felt i ett kall tok 3,0 ms i P0 og 9,6 ms i P8, det samme som enkeltkallene til sammen.

---

## 2. Sidefeltet (statusblokken)

Statusblokken viser faste verdier (`STATUS_DEFS` i `web/js/defs.js`) og festede rader fra Sensorer.

| Verdi | Kilde | Støttet | Målt eksempel | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| GPU-temp | `nvmlDeviceGetTemperature(NVML_TEMPERATURE_GPU)` | Ja, hele °C | 31 °C | 0,4–1,1 ms / 0,14 ms | 1 s | Live (`temperature.gpu`) | Som i dag |
| GPU-last | `nvmlDeviceGetUtilizationRates().gpu` | Ja, hele % (driverens måleperiode 1/6–1 s) | 2–6 % i ro | 1,6–4,5 ms / 0,52 ms | 1 s | Live | Som i dag |
| Effekt | `nvmlDeviceGetPowerUsage` (mW, øyeblikksverdi på Turing) eller Δ`nvmlDeviceGetTotalEnergyConsumption` (mJ) / Δt | Ja. Gjennomsnitt fra driveren (felt 185) er «Ikke støttet» | 12,2–12,9 W i ro; 54–69 W når kortet er vekket | 4,5–9 ms / 1,6–2 ms (hver) | 1 s | Live (`power.draw`) | Snitt fra energitelleren (kap. 12) |
| VRAM | `nvmlDeviceGetMemoryInfo_v2().used` | Ja, byte | 674 MiB av 8192 (+399 MiB reservert) | 0,02–0,07 ms | 1 s | Live (`memory.used`) | **Bruk `_v2`.** v1 regner det reserverte som brukt (1074 MiB) og stemmer ikke med nvidia-smi |
| Vifte | `nvmlDeviceGetFanSpeed_v2(fan)` | Ja, per vifte, hele % | 35 % / 35 % | 1,2–2,6 ms / 0,41 ms per vifte | 1 s (vifte 0), 2 s (vifte 1) | Live, én samlet verdi | Vis høyeste av de to, og begge i verktøytipset |
| CPU-temp | `/sys/class/hwmon/hwmon0/temp2_input` (k10temp Tdie) | Ja, 1/8 °C | 37,375 °C (Tctl 47,375) | 2,3 µs (pread) | 1 s | **Feil: viser Tctl**, 10 °C for høyt | Tdie som hovedverdi, Tctl i verktøytips (kap. 7) |
| CPU-last | `/proc/stat` (rad `cpu`) | Ja, jiffies (10 ms) | – | 34 µs | 1 s | Live | Som i dag |
| Minne | `/proc/meminfo` MemTotal − MemAvailable | Ja, kB | 18,1 GiB av 47,0 GiB brukt | 7 µs | 1 s | Live | Som i dag |
| Nett ned/opp | `/sys/class/net/*/statistics/{rx,tx}_bytes` | Ja, byte | wlo1 7,79 GB mottatt totalt | 1,4 µs per fil | 1 s | Live | Som i dag. `lo` og virtuelle kort utelates (som i dag) |
| Kildemerke / modus | – | – | – | – | – | «nvidia-smi» eller «Simulert» | «NVML». Ved feil: «Skjermkortet svarer ikke» med årsak fra `nvmlErrorString` |

---

## 3. Oversikt

| Verdi | Kilde | Støttet | Målt eksempel | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Fliser: temperatur, GPU-last, effekt, VRAM, vifte | se kapittel 2 | Ja | se over | se over | 1 s | Live | Som i dag |
| Flis: kjerneklokke | `nvmlDeviceGetClockInfo(NVML_CLOCK_GRAPHICS)` | Ja, MHz | 300 (P8), 2010 (P0) | 2,4–4,2 ms / 0,7–1,1 ms | 1 s | Live (`clocks.gr`) | Som i dag |
| «av 245 W» under effekt | `nvmlDeviceGetEnforcedPowerLimit` | Ja, mW | 245 000 | 0,6–0,7 ms / 0,09 ms | 5 s (endres bare ved set) | Live | Les ved oppstart og etter hver bruk i Kontroller, ellers hvert 5. s |
| PCIe (nå/maks) | `/sys/bus/pci/devices/0000:1c:00.0/current_link_{speed,width}` eller `nvmlDeviceGetCurrPcieLinkGeneration/Width` | Ja. 2,5/5/8 GT/s = gen 1/2/3 | Gen 1–3 (endres med last), x16 | sysfs 2,3 µs; NVML 0,5–0,9 ms / 0,13 ms | 1 s | Live (`pcie.link.*.current`) | **Bruk sysfs** (300 ganger billigere, samme svar) |
| P-tilstand | `nvmlDeviceGetPerformanceState` | Ja | P8 i ro, P5/P0 under last | 0,3–0,7 ms / 0,09 ms | 1 s | Live | Som i dag |
| Viftestyring | `nvmlDeviceGetFanControlPolicy_v2(fan)` | Ja. 0 = driveren styrer (`TEMPERATURE_CONTINOUS_SW`), 1 = manuell | 0 / 0 | 0,8 ms / 0,08 ms | 5 s | **Simulert** (fra demoens lagrede tilstand) | Les policy fra kortet. «Kurve» kan bare vises når vår egen tjeneste styrer viften |
| Video (enc/dek) | `nvmlDeviceGetEncoderUtilization`, `…DecoderUtilization` | Ja, %, driverens periode 1 s | 0 / 0 | 1,3–4,8 ms / 0,53 ms hver | 2 s | Live | 2 s er nok, siden driveren selv bare oppdaterer hvert sekund |
| Klokker-stolper (grafikk, minne, video) | `nvmlDeviceGetClockInfo` + `nvmlDeviceGetMaxClockInfo` | Ja | nå 300/405/540, maks 2190/7000/2040 MHz | nå: 2–3,6 ms; maks: 1,5–2,5 ms (én gang) | nå 1 s (grafikk, minne), 2 s (video) | Live | Som i dag. Maks leses én gang |
| Klokkehendelser (hva begrenser klokken) | `nvmlDeviceGetCurrentClocksEventReasons` (maske), støttet maske 0x1ff | Ja | 0x1 (tomgang) i P8, 0x0 i P0 | **24–27 ms i P8** / 0,7 ms | 2 s, men bare når P-tilstanden ikke er P8 | Live (`clocks_event_reasons.active`) | I P8 vises «Tomgang» uten kallet. Under last: les hvert 2. s |
| Temperaturmåler med grenser | `nvmlDeviceGetTemperatureThreshold`: SHUTDOWN, SLOWDOWN, GPU_MAX, ACOUSTIC_CURR | Ja | 100 / 97 / 88 / 83 °C | 0,5–3 ms (én gang) | Ved oppstart, ACOUSTIC_CURR også etter set | Live (fra `nvidia-smi -q`) | Som i dag |
| Effektmåler med grenser | `nvmlDeviceGetPowerManagementLimitConstraints`, `…DefaultLimit`, `…Limit` | Ja | 105–275 W, standard 245 W | < 0,03 ms (hurtigbufret) | Ved oppstart | Live | Som i dag |
| Tre små grafer (last, temperatur, effekt) | samme kilder | Ja | – | – | 1 s | Live, 500 ms | 1 s (passer historikkens 1 s-bøtter) |

---

## 4. Sensorer

GPU-radene (`GPU_ROWS` i `web/js/pages/sensors.js`), CPU, minne, nett og «Andre sensorer» (hwmon).

| Rad | Kilde | Støttet | Målt eksempel | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Temperatur | se kap. 2 | Ja | 31 °C | 0,4–1,1 ms | 1 s | Live | Som i dag |
| GPU-last / Minnekontroller-last | `nvmlDeviceGetUtilizationRates` (ett kall gir begge) | Ja | 4 % / 11 % | 1,6–4,5 ms | 1 s | Live | Som i dag |
| Videokoder / -dekoder | se kap. 3 | Ja | 0 % / 0 % | 1,3–4,8 ms hver | 2 s | Live | Som i dag |
| Grafikk-, SM-, minne-, videoklokke | `nvmlDeviceGetClockInfo` (GRAPHICS, SM, MEM, VIDEO) | Ja, MHz | 300 / 300 / 405 / 540 | 2–4 ms hver i P8 | grafikk og minne 1 s, SM og video 2 s | Live | SM = grafikk på dette kortet (begge 2010 under last). Kan slås sammen eller leses sjeldnere |
| Effekttrekk | se kap. 2 | Ja | 12,8 W | 4,5–9 ms | 1 s | Live | Snitt fra energitelleren |
| Effektgrense | `nvmlDeviceGetEnforcedPowerLimit` | Ja | 245 W | 0,6 ms | 5 s | Live | 5 s |
| Viftefart | `nvmlDeviceGetFanSpeed_v2` per vifte | Ja, 2 vifter | 35 % / 35 % | 1,2–2,6 ms per vifte | 1–2 s | Live, én rad | **To rader**: «Vifte 1» og «Vifte 2» |
| **Ny: Viftens turtall** | `nvmlDeviceGetFanSpeedRPM` (struct `nvmlFanSpeedInfo_t` v1) | **Ja** | 801 / 798 o/min | 2,4–2,5 ms / 0,4 ms per vifte | 2 s | **Mangler, og demoen sier feilaktig at det ikke finnes** | To rader i o/min. Ta bort merknaden «Viftens turtall (RPM)» |
| **Ny: Viftens målfart** | `nvmlDeviceGetTargetFanSpeed` | Ja | 35 % | 1,6–3,2 ms | 5 s | Mangler | Verktøytips på viftefart («mål 35 %») |
| Videominne brukt | `nvmlDeviceGetMemoryInfo_v2` | Ja | 674 MiB | 0,07 ms | 1 s | Live | `_v2` (se kap. 2) |
| PCIe-generasjon | sysfs `current_link_speed` | Ja | 2,5 GT/s = gen 1 | 2,3 µs | 1 s | Live | sysfs |
| **Ny: PCIe-bredde** | sysfs `current_link_width` | Ja | 16 | 2,3 µs | 5 s | Mangler som rad | Valgfri rad |
| **Ny: Bussbelastning** | `nvmlDeviceGetDynamicPstatesInfo().utilization[3]` (BUS); [0] GPU, [1] FB, [2] VID | Ja | 0 % (GPU 4 %, FB 10 %) | 2,6 ms / 0,52 ms | 2 s | Mangler | GPU-Z kaller dette «Bus Interface Load». Billig erstatning for PCIe-gjennomstrømning |
| **Ny: PCIe-gjennomstrømning** | `nvmlDeviceGetPcieThroughput` (TX, RX), KB/s | Ja | TX 683, RX 1513 KB/s | **20–21 ms per retning** (driveren måler i 20 ms) | Bare når raden er synlig, høyst hvert 5. s | Mangler | Kun på forespørsel |
| **Ny: Energi** | `nvmlDeviceGetTotalEnergyConsumption`, mJ siden driveren ble lastet, oppdateres hvert ~100 ms | Ja | 1 017 977 031 mJ (≈ 283 Wh) | 3,8–5 ms / 2 ms | 1 s (brukes til effekt) | Mangler | Grunnlag for effekt og for «energi siden start» |
| Prosessor · Temperatur (Tctl) / (Tdie) | k10temp `temp1_input` (Tctl), `temp2_input` (Tdie) | Ja, 1/8 °C | 47,375 / 37,375 °C | 2,3 µs hver | 1 s | Live, begge | Som i dag, men Tdie er «CPU-temp» (kap. 7) |
| Prosessor · Total last, Tråd 0–15 | `/proc/stat` | Ja | – | 34 µs for alle | 1 s | Live | Som i dag |
| **Ny: Klokke per kjerne** | `/sys/devices/system/cpu/cpuN/cpufreq/cpuinfo_avg_freq` (kHz, APERF/MPERF-snitt) | Ja | 2,2–4,35 GHz | 19 µs for alle 16 | 1 s | Bare på System, ikke på Sensorer | Valgfri gruppe. Se kap. 7 om tråder og kjerner |
| Minne · Brukt, Brukt av totalt, Tilgjengelig, Swap brukt | `/proc/meminfo` | Ja | se kap. 7 | 7 µs | 1 s | Live | Som i dag |
| Nett · ned/opp per kort | `/sys/class/net/*/statistics/*_bytes` | Ja | wlo1 oppe, enp24s0 nede | 1,4 µs per fil | 1 s | Live | Som i dag |
| **Ny: Wi-Fi-signal** | `/proc/net/wireless` (link, level dBm) | Ja | link 70, −33 dBm | 73–92 µs | 5 s | Mangler | Valgfritt |
| Andre sensorer · iwlwifi_1 temp1 | hwmon1 | Ja, men dyr | 49 °C | **61–70 µs** (spør Wi-Fi-fastvaren) | 5 s | Live 1 s | 5 s |
| Andre sensorer · ucsi_source_psy (in0, curr1) | hwmon2, USB-C-kontrolleren på skjermkortet | Ja, men alltid 0 | 0 V, 0 A | 1,3 µs | 5 s | Live | Skjul når begge er 0 og max er 0 (ingen USB-C-enhet tilkoblet) |
| Andre sensorer · hidpp_battery_0 | hwmon3, Logitech-mottaker | Ingen målinger | – | – | – | Merknad «ingen verdier» | Utelat uten merknad |
| **Ny: Disk-I/O** | `/proc/diskstats` (felt 6 og 10 = sektorer lest/skrevet, 512 B) eller `/sys/block/sdX/stat` | Ja | sdb 53 M sektorer lest | 69 µs (diskstats, 70 linjer med loop) / 3 µs per `stat`-fil | 1 s | Mangler | Les `/sys/block/sd*/stat` for fysiske disker, ikke hele diskstats |

---

## 5. Grafer

Grafene bruker de samme kildene som Sensorer. Demoens serier: GPU-temperatur, GPU-last (med minnekontroller), kjerneklokke, minneklokke, GPU-effekt (trekk og grense), GPU-vifte, videominne (brukt og total), CPU-last, CPU-temperatur, minne brukt, nett.

| Serie | Kilde | Støttet | Demo i dag | Produksjon |
|---|---|---|---|---|
| GPU-temperatur, -last, klokker, effekt, vifte, VRAM | NVML (kap. 2–4) | Ja | Live, prøve hvert 500. ms | Prøve hvert sekund. Historikken lagrer uansett 1 s-bøtter med min/snitt/maks |
| Effekt: grense | `nvmlDeviceGetEnforcedPowerLimit` | Ja | Live | Hvert 5. s, tegnes som trapp |
| CPU-temperatur | k10temp | Ja | **Tctl** (10 °C for høyt) | Tdie |
| CPU-last, minne, nett | `/proc`, `/sys` | Ja | Live, 1 s | Som i dag |
| Nye serier som kan legges til | viftens turtall, bussbelastning, energi, klokke per kjerne, disk-I/O | Ja | – | Valgfritt |

Med 1 s polling blir grafene ikke grovere enn i dag, fordi historikklaget med finest oppløsning allerede er 1 s (`server/history.js`).

---

## 6. Grafikkort (GPU-Z)

| Verdi | Kilde | Støttet | Målt eksempel | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Navn | `nvmlDeviceGetName` | Ja | NVIDIA GeForce RTX 2080 | < 0,05 ms | Oppstart | Live | Som i dag |
| Merke | `nvmlDeviceGetBrand` | Ja, enum 5 | GeForce | < 0,05 ms (1,9 ms første gang) | Oppstart | Live (`-q`) | Som i dag |
| Arkitektur | `nvmlDeviceGetArchitecture` | Ja, enum 6 | Turing | < 0,05 ms | Oppstart | Referansetabell | Fra NVML |
| GPU-brikke / delenummer | `nvidia-smi -q` «GPU Part Number». NVML har ikke dette kallet | Bare via nvidia-smi | 1E87-400-A1 | 100 ms (`-q`) | Én gang, kan bufres | Live (`-q`) | Slå opp brikken (TU104-400A) fra PCI-ID som i dag. Delenummeret kan leses én gang med `nvidia-smi -q` i bakgrunnen og bufres, eller utelates |
| PCI-ID enhet / subsystem | `nvmlDeviceGetPciInfo_v3` eller sysfs `vendor`, `device`, `subsystem_*` | Ja | 10de:1e87 / 10de:1e87 | 0,02 ms / 1,2 µs | Oppstart | Live (sysfs) | Som i dag, med samme forklaring om NVIDIA-subsystemet |
| Bus-ID | `nvmlDeviceGetPciInfo_v3().busId` | Ja | 00000000:1C:00.0 | 0,02 ms | Oppstart | Live | Som i dag |
| Revisjon | sysfs `revision` | Ja | 0xa1 | 1,2 µs | Oppstart | Live | Som i dag |
| VBIOS | `nvmlDeviceGetVbiosVersion` | Ja | 90.04.0B.40.8C | 0,05 ms | Oppstart | Live | Som i dag |
| Driver | `nvmlSystemGetDriverVersion` | Ja | 610.43.02 | < 0,07 ms | Oppstart | Live | Som i dag, gjerne med «Open Kernel Module» fra `/proc/driver/nvidia/version` |
| CUDA | `nvmlSystemGetCudaDriverVersion_v2` | Ja | 13.3 | 0,04 ms | Oppstart | Live (`-q`) | Fra NVML |
| Compute capability | `nvmlDeviceGetCudaComputeCapability` | Ja | 7.5 | < 0,02 ms | Oppstart | Live | Som i dag |
| PCIe (maks og nå) | `nvmlDeviceGetMaxPcieLinkGeneration` (system) og `…GpuMaxPcieLinkGeneration` (kort), `…MaxPcieLinkWidth`; nå fra sysfs | Ja | Gen 3 x16 maks, nå gen 1–3 | maks < 1,3 ms én gang | Maks ved oppstart, nå 1 s | Live | Som i dag |
| **Ny: PCIe-feil** | `nvmlDeviceGetPcieReplayCounter`; sysfs `aer_dev_correctable` | Ja | 0 replays, TOTAL_ERR_COR 0 | 0,75 ms / 2,4 µs | 60 s | Mangler | Valgfritt, fint for feilsøking |
| Videominne | `nvmlDeviceGetMemoryInfo_v2().total` | Ja | 8192 MiB | 0,07 ms | Oppstart | Live | Som i dag |
| **Ny: Minnebuss** | `nvmlDeviceGetMemoryBusWidth` | Ja | 256 bit | 0,3–0,8 ms | Oppstart | Bare i referansetabellen | Fra NVML (målt, ikke referanse) |
| **Ny: CUDA-kjerner** | `nvmlDeviceGetNumGpuCores` | Ja | 2944 | 0,2–0,7 ms | Oppstart | Bare i referansetabellen | Fra NVML |
| BAR1 | `nvmlDeviceGetBAR1MemoryInfo` | Ja | 256 MiB, 17 MiB i bruk | 0,07 ms | 10 s for «i bruk» | Live (`-q`, én gang) | Som i dag |
| Resizable BAR | sysfs `resource1` (størrelse) og `resource1_resize` (bitmaske) | Ja | `0x1c0` = 64, 128, 256 MiB. Største er 256 MiB < 8192 MiB | 4,5 µs | Oppstart | Live | «Ikke støttet · største BAR1 er 256 MiB». Demoens tekst er riktig |
| Vifter | `nvmlDeviceGetNumFans`, `…MinMaxFanSpeed` | Ja | 2, område 35–100 % | < 0,1 ms / 2,7 ms | Oppstart | **Feil: «Ikke oppgitt»** (nvidia-smi gir én samlet verdi) | «2 · 35–100 %» |
| **Ny: Kjølerinfo** | `nvmlDeviceGetCoolerInfo` | Ja | signal 2 (variabel), mål 14 (GPU, minne, strømforsyning) | 0,06 ms | Oppstart | Mangler | Verktøytips på Vifter |
| UUID | `nvmlDeviceGetUUID` | Ja | GPU-eaf05f06-… | 0,07 ms | Oppstart | Live, maskert | Som i dag |
| GSP-fastvare | `nvmlDeviceGetGspFirmwareVersion`, `…GspFirmwareMode` | Ja | 610.43.02, på (standard på) | 0,06 ms | Oppstart | Live (`-q`) | Fra NVML |
| InfoROM | `nvmlDeviceGetInforomImageVersion`; objekt OEM 1.1; ECC og POWER «Ikke støttet» | Ja | G001.0000.02.04 | 0,08 ms | Oppstart | Live (`-q`) | Som i dag |
| Serienummer | `nvmlDeviceGetSerial` | **Ikke støttet** | NOT_SUPPORTED | – | – | Vises ikke | «Ikke oppgitt av kortet» (vanlig for GeForce-kort) |
| Board part number | `nvmlDeviceGetBoardPartNumber` | Svarer OK, men tom tekst | «» | – | – | Vises ikke | Utelat |
| Board ID | `nvmlDeviceGetBoardId` | Ja | 0x1c00 | 0,06 ms | Oppstart | Vises ikke | Valgfritt |
| Klokker nå (grafikk, SM, minne, video) | `nvmlDeviceGetClockInfo` | Ja | 300 / 300 / 405 / 540 MHz | 2–4 ms hver i P8 | 1–2 s (bare når siden er synlig) | Live | Som i dag |
| Klokker maks (driver) | `nvmlDeviceGetMaxClockInfo` | Ja | 2190 / 2190 / 7000 / 2040 MHz | 1,5–2,5 ms | Oppstart | Live | Som i dag |
| Standard grafikkklokke / applications clocks | `nvmlDeviceGetApplicationsClock`, `…DefaultApplicationsClock`, `…MaxCustomerBoostClock` | **Ikke støttet** (avviklet) | NOT_SUPPORTED | – | – | Merknad «avviklet» | Som i dag |
| **Ny: Klokkeområde per P-tilstand** | `nvmlDeviceGetMinMaxClockOfPState` | Ja | P0 300–2190 / minne 7000; P2 300–2190 / 6800; P5 300–2100 / 810; P8 300–645 / 405 MHz | 1,3–1,7 ms per kall | Oppstart | Mangler | Liten tabell, nyttig ved siden av Kontroller |
| **Ny: Støttede P-tilstander** | `nvmlDeviceGetSupportedPerformanceStates` | Ja | P0, P2, P3, P5, P8 | 1,4 ms | Oppstart | Mangler | Valgfritt |
| **Ny: Støttede minneklokker** | `nvmlDeviceGetSupportedMemoryClocks` | Ja | 7000, 6800, 5000, 810, 405 MHz | 2,5–3,2 ms | Oppstart | Mangler | Valgfritt |
| Effektgrense nå | `nvmlDeviceGetPowerManagementLimit` | Ja | 245 W | 0,7 ms | 5 s | Live | Som i dag |
| Standard / område | `…DefaultLimit`, `…LimitConstraints` | Ja | 245 W · 105–275 W | < 0,03 ms | Oppstart | Live | Som i dag |
| Temperaturmål | `nvmlDeviceGetTemperatureThreshold(ACOUSTIC_CURR)`, område ACOUSTIC_MIN/MAX | Ja | 83 °C (65–88) | 0,9 ms | Oppstart og etter set | Live (`-q`) | Vis også området |
| Maks drift / nedklokking / avstenging | THRESHOLD GPU_MAX, SLOWDOWN, SHUTDOWN | Ja | 88 / 97 / 100 °C | 0,5–3 ms | Oppstart | Live (`-q`) | Som i dag |
| Minne maks / GPS / margin (T.Limit) | THRESHOLD MEM_MAX, GPS_CURR; `nvmlDeviceGetMarginTemperature` | **Ikke støttet** | NOT_SUPPORTED | – | – | Vises ikke | «Ikke støttet av kortet» om raden tas med |
| **Ny: Termiske sensorer** | `nvmlDeviceGetThermalSettings(NVML_THERMAL_TARGET_ALL)` | Ja, men bare én | 1 sensor: intern GPU-kontroller, 0–127 °C, nå 31 | 0,8 ms | – | – | Bekrefter at kortet ikke har hotspot- eller minnesensor via NVML |
| Display aktiv / modus, persistence, compute mode, adressering | `nvmlDeviceGetDisplayActive` (1,8 ms), `…DisplayMode` (**11–14 ms**), `…PersistenceMode` (0 = av), `…ComputeMode` (0 = standard) | Ja | aktiv, persistence av, standard | se til venstre | Oppstart (Display mode kan utelates) | Live (`-q`), noe av det | Bare «Display aktiv» og «Persistence» trengs |
| ECC, NVLink, MIG, virtualisering, auto boost | `nvmlDeviceGetEccMode` (NOT_SUPPORTED), `…NvLinkState` (OK, 0 = av), `…VirtualizationMode` (0 = ingen), `…AutoBoostedClocksEnabled` (NOT_SUPPORTED) | Ikke relevant | – | – | – | Vises ikke | Utelat. NVLink svarer «av», ikke «ikke støttet» |
| **Ny: Prosesser på kortet** | `nvmlDeviceGetGraphicsRunningProcesses_v3`, `…ComputeRunningProcesses_v3` (PID, VRAM); navn fra `/proc/PID/comm` | Ja | gnome-shell 94–96 MiB, Chrome 64 MiB, Claude 201 MiB, Obsidian 47 MiB, Xwayland 3 MiB, ChatGPT 124 MiB | 0,66–0,72 ms / 0,15–0,22 ms | 5 s når lista er synlig | Mangler | Ny tabell «Prosesser som bruker skjermkortet» |
| **Ny: Last per prosess** | `nvmlDeviceGetProcessUtilization` (trenger buffer for mange prøver; 64 ga INSUFFICIENT_SIZE, 1024 virket) | Ja | Claude: SM 6 %, minne 10 % | 1–4 ms | 5 s når synlig | Mangler | Valgfri kolonne |
| Referansetabellen (brikke, CUDA-kjerner, ROP, båndbredde …) | `server/specs.js` | – | – | – | – | Referanse, merket «ikke målt» | Behold merkingen. CUDA-kjerner og buss kan nå vises som målt |

---

## 7. System (CPU-Z)

### Prosessor

| Verdi | Kilde | Støttet | Målt eksempel | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Navn | `/proc/cpuinfo` «model name» | Ja | AMD Ryzen 7 2700X Eight-Core Processor | 47 µs for hele fila | Oppstart | Live | Som i dag |
| Kodenavn, prosess, sokkel, revisjon | Ingen kilde i kjernen. Slås opp fra familie/modell/stepping | Oppslag | 17h/08h/2 → Pinnacle Ridge, 12 nm, AM4, PiR-B2 | – | Oppstart | Mangler | Liten innebygd tabell, merket som oppslag (som GPU-referansen) |
| Produsent | `vendor_id` | Ja | AuthenticAMD | – | Oppstart | Live | Som i dag |
| Familie · modell · stepping | `/proc/cpuinfo` | Ja | 23 · 8 · 2 (17h · 08h · 2h) | – | Oppstart | Live | Som i dag |
| Mikrokode | `/sys/devices/system/cpu/cpu0/microcode/version` (eller cpuinfo) | Ja | 0x800820d | 1,2 µs | Oppstart | Live (cpuinfo) | Som i dag |
| Kjerner og tråder | `topology/thread_siblings_list`, `core_id` | Ja | 8 kjerner, 16 tråder. Søsken: cpuN og cpuN+8 (cpu0 = 0,8) | 1,1–1,5 µs per fil | Oppstart | Live | Som i dag |
| **Ny: CCX-inndeling** | `cache/index3/shared_cpu_list` | Ja | CCX 0: 0–3, 8–11; CCX 1: 4–7, 12–15 | 1,3 µs | Oppstart | Mangler | Kan gruppere kjerneflisene i to CCX-er |
| Klokke (cpufreq) min–maks | `cpuinfo_min_freq`, `cpuinfo_max_freq` | Ja | 2200–3700 MHz. P-tilstander 3700, 3200, 2200. Boost (CPB) går til ca. 4350 MHz | 1,1 µs | Oppstart | Live | Som i dag |
| Governor og driver | `scaling_governor`, `scaling_driver` | Ja | schedutil · acpi-cpufreq (amd-pstate krever CPPC, som Zen+ ikke har) | 1,1 µs | 5 s | Live, 1 s | 5 s |
| Boost | `/sys/devices/system/cpu/cpufreq/boost` (eller `cpu0/cpufreq/cpb`) | Ja | 1 = på | 1,1 µs | 5 s | Live | Som i dag |
| **Ny: Tid i hver P-tilstand** | `cpufreq/stats/time_in_state` | Ja | 3700: 12 166 s, 3200: 6 180 s, 2200: 113 583 s (10 ms-enheter) | 1,7 µs | 10 s | Mangler | Valgfritt |
| Klokke per tråd | `cpuN/cpufreq/cpuinfo_avg_freq` (kjerne 6.13+), ellers `scaling_cur_freq`, ellers «cpu MHz» i `/proc/cpuinfo` | Ja, kHz, APERF/MPERF-snitt | 2132–4354 MHz | 1,3 µs per fil, 19 µs for 16 | 1 s | Live (`scaling_cur_freq`) | `cpuinfo_avg_freq` først. `cpuinfo_cur_freq` krever root |
| Multiplikator og bussklokke | Krever MSR (`/dev/cpu/*/msr`, root) | **Nei uten root** | – | – | – | Mangler | Utelat, eller «≈ klokke / 100 MHz» merket som anslag |
| Kjernespenning (VID/SVI2) | Krever MSR eller `zenpower` (ikke i kjernen) | **Nei** | – | – | – | Mangler | «Ikke tilgjengelig uten root» |
| Pakkeeffekt (RAPL) | `/sys/class/powercap/intel-rapl:0/energy_uj` (0400 root); perf `power/energy-pkg` krever `perf_event_paranoid` ≤ 0 (er 4) | **Nei uten root** | EACCES | – | – | Merknad, riktig | Via root-tjenesten senere |
| Last per tråd | `/proc/stat` | Ja | – | 34 µs | 1 s | Live | Som i dag |
| Adresser | cpuinfo «address sizes» | Ja | 43 bit fysisk, 48 bit virtuelt | – | Oppstart | Live | Som i dag |

### Cache

| Verdi | Kilde | Støttet | Målt eksempel | Demo i dag | Produksjon |
|---|---|---|---|---|---|
| L1 data | `cpu0/cache/index0/{size,ways_of_associativity,coherency_line_size,shared_cpu_list}` | Ja | 32 KiB × 8, 8-veis, 64 B | Live | Som i dag |
| L1 instruksjon | index1 | Ja | 64 KiB × 8, 4-veis | Live | Som i dag |
| L2 | index2 | Ja | 512 KiB × 8, 8-veis | Live | Som i dag |
| L3 | index3 | Ja | 8 MiB × 2 (én per CCX), 16-veis | Live | Som i dag |

Alle cache-filene koster 1–2 µs og leses én gang. Demoens utregning (`tråder / tråder som deler cachen`) gir riktig antall instanser.

### Instruksjonssett

`flags` i `/proc/cpuinfo`: 112 flagg. Av de utvalgte finnes SSE–SSE4.2, SSE4A, AVX, AVX2, FMA3, F16C, AES, SHA, CLMUL, BMI1/2, ADX, RDRAND/RDSEED, x86-64, AMD-V. AVX-512 finnes ikke. Demoen er riktig. Sårbarheter: 19 filer i `/sys/devices/system/cpu/vulnerabilities/` (lesbare, valgfritt).

### Temperatur (k10temp)

| Verdi | Kilde | Målt | Merknad |
|---|---|---|---|
| Tctl | `hwmon0/temp1_input` (millgrader, steg på 125 = 1/8 °C) | 47,375 °C | Styringstemperatur. På 2700X legger AMD til **+10 °C** (på 1600X/1700X/1800X +20 °C), så viftene reagerer tidligere |
| Tdie | `hwmon0/temp2_input` | 37,375 °C | Ekte brikketemperatur. k10temp lager Tdie bare når forskyvningen er kjent |
| Forskjell | – | **10,000 °C** (to målinger: 47,375/37,375 og 59,5/49,375) | Bekrefter forskyvningen |
| Tccd1/Tccd2 | – | Finnes ikke | Bare Zen 2 og nyere |

**Produksjon:** «CPU-temp» i sidefeltet, på Oversikt og i Grafer skal være **Tdie** når den finnes. Tctl vises som egen rad på Sensorer med forklaring. Velg Tctl bare når Tdie mangler (da er forskyvningen 0). Demoen (`cpuTempSensor` i `web/js/sensormeta.js`) velger Tctl først og viser derfor 10 °C for mye.

### Hovedkort (`/sys/class/dmi/id`)

| Felt | Lesbar | Verdi |
|---|---|---|
| board_vendor / board_name / board_version | Ja | Micro-Star International Co., Ltd. / B450 GAMING PRO CARBON AC (MS-7B85) / 1.0 |
| sys_vendor / product_name / product_version | Ja | Micro-Star International Co., Ltd. / MS-7B85 / 1.0 |
| product_family / product_sku / board_asset_tag / chassis_asset_tag | Ja | «To be filled by O.E.M.» (plassholder, filtreres som i dag) |
| bios_vendor / bios_version / bios_date / bios_release | Ja | American Megatrends Inc. / 1.60 / 03/06/2019 / 5.14 |
| chassis_vendor / chassis_type / chassis_version | Ja | MSI / 3 (stasjonær) / 1.0 |
| board_serial / chassis_serial / product_serial / product_uuid | **Nei (0400 root)** | EACCES |

Hver fil koster 1,1 µs (pread) og leses én gang. **Ny: brikkesett** kan leses fra PCI uten root: `1022:43c8`/`43d5`/`43c6` = «AMD 400 Series Chipset» (B450). Navnet slås opp i `/usr/share/hwdata/pci.ids`.

### Minne

| Verdi | Kilde | Støttet | Målt | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Totalt | `/proc/meminfo` MemTotal | Ja | 49 270 320 kB (47,0 GiB; resten av 48 GB er reservert av fastvaren og kjernen) | 7 µs | Oppstart | Live | Som i dag |
| Brukt | MemTotal − MemAvailable | Ja | ≈ 18,1 GiB | 7 µs | 1 s | Live | Som i dag |
| Tilgjengelig | MemAvailable | Ja | 28,9 GiB | – | 1 s | Live | Som i dag |
| Buffer og cache | Buffers + Cached | Ja | 1,35 + 27,3 GiB, hvorav `Shmem` 4,4 GiB | – | 1 s | **Upresist:** `Cached` inneholder `Shmem` (tmpfs, delt minne), som ikke kan frigjøres | Som `free`: Buffers + Cached + SReclaimable, med verktøytips om hvor mye som er `Shmem`. «Kan frigjøres» er MemAvailable − MemFree |
| Swap | SwapTotal − SwapFree | Ja | 63 MiB av 8 GiB | – | 1 s | Live | Som i dag |
| **Ny: Minnepress** | `/proc/pressure/memory` | Ja | avg10 0,00 | 3,4 µs | 5 s | Mangler | Valgfritt |
| Moduler, type, hastighet, timinger, kanaler | SMBIOS type 17 (`/sys/firmware/dmi/tables/DMI`, 0400 root) eller SPD (`ee1004`) | Se kap. 10 | – | – | – | «Krever bakgrunnstjeneste (root)» | Se kap. 10 |

### Operativsystem

Distribusjon (`/etc/os-release`: Ubuntu 26.04.1 LTS), kjerne (`/proc/sys/kernel/osrelease`: 7.0.0-31-generic), kjernebygg, arkitektur, skrivebord (`XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`), vertsnavn, oppetid (`/proc/uptime`, 3,4 µs) og last (`/proc/loadavg`, 1,7 µs). Alt virker uten root og er live i demoen. Oppetid og last: 1 s holder for oppetid, last 5 s.

### Disk (ny seksjon, finnes ikke i demoen)

| Disk | Modell | Type | Temperatur uten root |
|---|---|---|---|
| sda | ST2000DM008-2FR102, 1,8 TB | HDD | Nei |
| sdb | Samsung SSD 870 QVO 1 TB | SATA SSD (systemdisk) | Nei |
| sdc | Transcend TS128GSSD370, 119 GB | SATA SSD | Nei |
| sdd | ST1000DM010-2EP102, 931 GB | HDD | Nei |
| sde | ST4000DM000-2AE166, 3,6 TB | HDD | Nei |

Modell (`/sys/block/sdX/device/model`), størrelse, rotasjon og I/O-tellere (`/sys/block/sdX/stat`) kan leses uten root. Ingen NVMe-disker. **Disktemperatur finnes ikke uten root:** `drivetemp`-modulen er ikke lastet (vi skal ikke laste moduler), og `smartctl` krever root (`/dev/sdX` er `root:disk 0660`, brukeren er ikke i `disk`). Tre av HDD-ene har nesten ingen lesinger siden oppstart (206 lesinger), så de står sannsynligvis i hvilemodus. SMART-lesing kan vekke dem. Det må tas med i vurderingen hvis root-tjenesten skal lese disktemperatur.

---

## 8. Kontroller og Viftekurve

Disse sidene skriver til kortet. Skrivestien er ikke testet og skal ikke testes nå. Alle **grensene og nåverdiene** kan derimot leses uten root:

| Verdi | Kilde | Støttet | Målt | Kost | Intervall | Demo i dag | Produksjon |
|---|---|---|---|---|---|---|---|
| Effektgrense, område og standard | `…PowerManagementLimit`, `…LimitConstraints`, `…DefaultLimit` | Ja | 245 W, 105–275 W, standard 245 W | < 0,7 ms | Oppstart og etter set | **Simulert** (fra daemon-mock eller appens egne grenser) | Grenser fra NVML. Skrivekontrollene låst |
| Kjerneklokke-offset | `nvmlDeviceGetClockOffsets` (v1, `type=GRAPHICS`, `pstate=P0`) | Ja | 0 MHz, område −1000…+1000 | 0,65–0,87 ms / 0,1 ms | Oppstart og etter set | Simulert | Nåverdi og område fra NVML |
| Minneklokke-offset | samme, `type=MEM` | Ja | 0, område −2000…+6000 (NVML-enheter, dobbel dataklokke) | samme | samme | Simulert | Vis i effektiv MHz og forklar enheten |
| Temperaturmål | THRESHOLD ACOUSTIC_CURR/MIN/MAX | Ja | 83 °C, 65–88 | 0,9 ms | Oppstart og etter set | Simulert | Fra NVML |
| Viftemodus | `nvmlDeviceGetFanControlPolicy_v2` | Ja | 0 = driveren styrer, begge vifter | 0,8 ms | 5 s | Simulert | Fra NVML |
| Viftefart og grenser | `…FanSpeed_v2`, `…TargetFanSpeed`, `…MinMaxFanSpeed` | Ja | 35 %, mål 35 %, 35–100 % | se kap. 4 | 1–2 s | Live fart, simulerte grenser | Fra NVML. Minimum 35 % gjelder også kurven |
| Viftens turtall | `…FanSpeedRPM` | Ja | ≈ 800 o/min ved 35 % | 2,5 ms | 2 s | Mangler | Vis ved siden av prosent på Viftekurve |
| Temperaturkilde for kurven | bare GPU-temperatur | Bare én sensor | 31 °C | – | – | «Hotspot oppgis ikke av driveren» | Riktig. NVML har bare én termisk sensor (kap. 6) |
| Start/stopp-vifte (0 %) | – | **Nei.** NVML-minimum er 35 % | – | – | – | Simulert | «Ikke støttet på dette kortet: minste fart er 35 %» |
| Profiler | appens egne data | – | – | – | – | Live i appen | Som i dag |

Skrivekall krever root (research kap. 2.2) og ligger til senere godkjenning.

---

## 9. Lys (RGB)

Ikke noe er lest fra eller skrevet til I2C i denne revisjonen. Bare `ls /dev/i2c-*` og bussnavnene i `/sys/bus/i2c/devices/*/name` er lest.

| Verdi | Kilde | Støttet | Demo i dag | Produksjon |
|---|---|---|---|---|
| Lyskontroller | Sannsynlig Palit «PalitGPUv2» på adresse **0x49** på «NVIDIA i2c adapter 1» (research kap. 4.2: ThunderMaster-opptak fra søsterkort, OpenRGB-kode) | **Ikke bekreftet** på dette kortet | Simulert, merket «ikke koblet til» | «Ikke koblet til» til trinnene i research kap. 4.5 er godkjent og gjort |
| Effekter (statisk, pust, av) | Registrene `0x60`, `0x6C–0x6F`, `0x61–0x6B` | Kjent fra opptaket, ikke testet | Simulert forhåndsvisning | Bare forhåndsvisning |
| Regnbue, fargesyklus, temperatur | Programvare (ThunderMaster gjør regnbue i programvare) | Ville kreve mange skriv per sekund | Simulert | Bare forhåndsvisning |
| Temperatur-effekten | GPU-temperatur fra NVML | Ja | Live temperatur i forhåndsvisningen | Som i dag |
| Tilgang | `/dev/i2c-*` er `crw------- root root` | Krever root | – | Root-tjenesten senere |

**Bussene nå** (lest 7. november, oppetid siden 5. november 06:21):

| Buss | Navn | Merknad |
|---|---|---|
| `i2c-0` | NVIDIA i2c adapter 1 at 1c:00.0 | Port 1, der Palit-kontrolleren sannsynligvis er |
| `i2c-1`…`i2c-6` | NVIDIA i2c adapter 2, 3, 5, 6, 7, 8 at 1c:00.0 | DDC og andre GPU-busser |
| `i2c-7` | SMBus PIIX4 adapter port 0 at 0b00 | **Hovedkortets SMBus.** SPD-EEPROM på 0x50–0x53 (`ee1004`), sidevalg 0x36/0x37 |
| `i2c-8` | SMBus PIIX4 adapter port 2 at 0b00 | Hovedkortets SMBus |
| `i2c-9` | SMBus PIIX4 adapter port 1 at 0b20 | Hovedkortets SMBus |
| `i2c-10` | NVIDIA GPU I2C adapter (funksjon 1c:00.3) | **USB-C-kontrolleren `ccgx-ucsi` på 0x08** |

Researchen (4. november) fant USB-C-kontrolleren på `i2c-8`. Nå er den `i2c-10`, fordi `i2c_piix4` og `ee1004` er lastet og har fått lavere nummer. **Regelen i `REGLER.md` («Åpne aldri `i2c-8`/0x08») peker derfor på hovedkortets SMBus nå, ikke på USB-C-kontrolleren.** Produksjonskoden må finne bussen ut fra navnet («NVIDIA i2c adapter 1 at 1c:00.0») og sysfs-stien under GPU-ens PCI-adresse, og svartelisten må gjelde «NVIDIA GPU I2C adapter» / `ccgx-ucsi` uansett nummer. Hovedsesjonen bør oppdatere regelen.

---

## 10. Mangler / ikke mulig uten root

| Verdi | Hvorfor | Hva vi mister | Hva produksjon viser |
|---|---|---|---|
| GPU hotspot- og minnetemperatur | NVML har én termisk sensor på dette kortet; `MEM_MAX`-terskel og felt 82 (minnetemperatur) gir NOT_SUPPORTED. LACT leser dette via udokumentert NvAPI på nyere arkitekturer (research 2.3) | Viktig for HWiNFO-likhet, men ikke tilgjengelig | «Ikke støttet av kortet og driveren» |
| GPU-kjernespenning | Ingen NVML-funksjon | Spenning i grafer og OC-bilde | «Ikke støttet: NVML oppgir ikke spenning» |
| GPU gjennomsnittseffekt fra driveren | Felt 185 NOT_SUPPORTED; `nvidia-smi` «Average Power Draw: N/A» | Ingen, vi regner snitt fra energitelleren | Snitt fra energitelleren |
| T.Limit (margin til nedklokking) | `nvmlDeviceGetMarginTemperature` NOT_SUPPORTED | – | Regn «avstand til nedklokking» selv: 97 °C − temperatur, merket som utregnet |
| Applications clocks, standard boost | Avviklet i driveren | – | «Avviklet i driveren» |
| GPU-serienummer, board part number | NOT_SUPPORTED / tom | – | Utelat |
| CPU-pakkeeffekt (RAPL) | `energy_uj` er 0400 root siden Platypus-fiksen (CVE-2020-8694); perf er sperret (`perf_event_paranoid` = 4) | CPU-effekt i W | «Krever bakgrunnstjeneste (root)» |
| CPU-spenning, multiplikator, bussklokke | MSR (`/dev/cpu/*/msr` 0600 root) eller `zenpower` (ikke i kjernen) | CPU-Z-feltene Core Voltage, Multiplier, Bus Speed | «Krever root» / anslag merket som anslag |
| `cpuinfo_cur_freq` | 0400 root | Ingenting, `cpuinfo_avg_freq` gir det samme | – |
| Hovedkortets vifter, spenninger og temperaturer | Krever `nct6775` (eller `nct6687`) for sensorbrikken. Ikke lastet, og vi laster ikke moduler | Hele HWiNFO-hovedkortdelen | Merknaden i demoen er riktig. Kan foreslå `sudo modprobe nct6775` som brukerens eget valg |
| DMI-serienumre og `product_uuid` | 0400 root | Ingenting viktig | «Leses ikke (krever root)» som i dag |
| `dmidecode` | Trenger `/sys/firmware/dmi/tables/DMI` (0400) eller `/dev/mem`. Uten root: «Permission denied» | Minnemoduler (type 17): produsent, delenummer, størrelse per spor, konfigurert hastighet, spenning; prosessor (type 4): ekstern klokke, nåværende hastighet; spor (type 9) | Vises ikke |
| `smartctl` / disktemperatur | `/dev/sdX` er `root:disk`; `drivetemp` ikke lastet | Disktemperatur, SMART-helse | «Krever root». Obs: SMART kan vekke HDD-er |
| `lshw` uten root | Gir ufullstendige data og advarer selv | – | Ikke bruk |
| Minnets faktiske hastighet, kanaler, timinger nå | Bare i UMC-registrene (SMN, root) eller SMBIOS (root). SPD gir bare det modulen *støtter* (JEDEC og XMP), ikke det BIOS har satt | DRAM-frekvens og timinger slik CPU-Z «Memory» viser dem | «Krever root» |

**DIMM-informasjon uten root – et funn.** `ee1004`-driveren er lastet og har bundet SPD-EEPROM-ene på `i2c-7`, adresse 0x50–0x53 (fire moduler). Filene `/sys/bus/i2c/devices/7-005{0,1,2,3}/eeprom` er **`-r--r--r--` (lesbare for alle)** og 512 byte store (DDR4-SPD). `decode-dimms` bruker akkurat disse filene. Det betyr at modulenes produsent, delenummer, størrelse, JEDEC-timinger og XMP-profiler *kan* leses uten root. Det er ikke gjort her, fordi lesingen går over hovedkortets SMBus, og `ee1004` skriver til sidevalgadressene 0x36/0x37 for å bytte side. Det er en vanlig og normalt trygg operasjon som kjernen gjør selv, men den er I2C-trafikk, og oppdraget sa ingen I2C. **Dette krever brukerens godkjenning før det brukes.** Uansett gir SPD ikke den hastigheten minnet faktisk kjører på. Demoens merknad «SMBIOS/SPD kan bare leses av root» er feil for SPD-delen.

`sensors` (lm-sensors 3.6.2) er installert. `sensors -j` koster 4,3 ms per kjøring og leser de samme hwmon-filene, så appen bør lese hwmon direkte.

---

## 11. Avvik i demoen som produksjonen bør rette

1. **CPU-temp = Tctl** (10 °C for høyt). Bruk Tdie (kap. 7).
2. **Vifter «Ikke oppgitt»** og **turtall «finnes ikke»**: NVML gir 2 vifter og o/min (kap. 4 og 6).
3. **VRAM**: med NVML må `nvmlDeviceGetMemoryInfo_v2` brukes. v1 gir 400 MiB for mye.
4. **Buffer og cache** tar med `Shmem` (kap. 7).
5. **Merknaden om SPD** («bare root») er feil (kap. 10).
6. **Viftestyring på Oversikt** er simulert. Les policy fra kortet.
7. **Grenser på Kontroller** er simulert eller fra daemon-mock. Les dem fra NVML.
8. **Polling hvert 500. ms med `nvidia-smi`** koster 9,5 % av én kjerne og rekker ikke 2 Hz i P8. Med NVML og en slank runde hvert sekund blir det under 2 %.

---

## 12. Anbefalt pollearkitektur

### Tråder

- **GTK-hovedtråden gjør aldri NVML-kall.** Et kall kan ta 25 ms, og en runde 60 ms.
- **Én NVML-tråd** (`std::thread`) eier biblioteket og handle-en. Den sender ferdige prøver til hovedtråden over en kanal (`async_channel` eller `glib::MainContext::channel`), og hovedtråden tegner.
- **System-tråden** (eller en `glib::timeout` på hovedtråden, siden alt koster under 0,3 ms) leser `/proc` og `/sys` med filbeskrivere som åpnes én gang og leses med `pread(fd, buf, 0)`. Det er 5–8 ganger raskere enn å åpne fila hver gang (målt: 9 µs mot 1,1–1,3 µs).
- NVML lastes dynamisk (`libloading`), så appen starter også uten NVIDIA-driver. Mangler biblioteket, vises «Fant ikke NVIDIA-driveren (libnvidia-ml.so.1)» og CPU/system virker fortsatt.

### Oppstart (målet er at vinduet er der med en gang)

1. Vis vinduet med plassholdere (demoen har dem allerede) og systemdata, som er klare på under 1 ms.
2. Start NVML-tråden: `dlopen` (0,5 ms), `nvmlInit_v2` (12–21 ms), handle (6–8 ms).
3. Første slanke pollerunde (17 ms i P8), og send den straks, så flisene fylles etter ca. 50 ms.
4. Les faste opplysninger (ca. 50 ms i P8) **fordelt over de første sekundene**, ikke i én tett blokk, så kortet ikke vekkes.
5. Valgfritt: buffre faste GPU-opplysninger på disk med UUID og driverversjon som nøkkel, så Grafikkort-siden kan vises fra bufferen ved neste oppstart og kontrolleres i bakgrunnen.

### Hvert sekund (1 Hz) – slank runde, ca. 15 ms i P8 og ca. 4,5 ms i P0

| Kilde | Kall |
|---|---|
| NVML (8 kall) | temperatur · energiteller (effekt = ΔmJ/Δms, snitt for sekundet) · klokke grafikk · klokke minne · `UtilizationRates` (GPU og minnekontroller) · `MemoryInfo_v2` · P-tilstand · vifte 0 (%) |
| sysfs GPU | `current_link_speed` (PCIe-generasjon) |
| CPU | `/proc/stat` · 16 × `cpuinfo_avg_freq` · k10temp Tctl og Tdie |
| System | `/proc/meminfo` · nett `rx_bytes`/`tx_bytes` per fysisk kort · `/proc/uptime` · `/sys/block/sd*/stat` |

Effekt: målt 12,34 W fra energitelleren over 2 s mot 12,93 W øyeblikksverdi. Energitelleren oppdateres omtrent hvert 100. ms, så 1 s-snittet er godt. Hvis `TotalEnergyConsumption` feiler, brukes `PowerUsage`. Ikke les begge hvert sekund (hver koster 4–5 ms i P8).

### Hvert 2. sekund

Vifte 1 (%), turtall for begge vifter, klokke SM og video, encoder og decoder, dynamiske P-tilstander (bussbelastning), klokkehendelser **bare når P-tilstanden ikke er P8** (i P8 vises «Tomgang», og kallet ville kostet 25 ms).

### Hvert 5. sekund

Håndhevet effektgrense, viftepolicy, viftens målfart, PCIe-bredde, governor og boost, `/proc/loadavg`, Wi-Fi-signal og iwlwifi-temperatur (60–90 µs, spør fastvaren), USB-C-sensorene, minnepress. Prosesslista bare når den er synlig.

### Hvert 10.–60. sekund

BAR1 i bruk (10 s), bruddtellere for nedklokking (`ViolationStatus`, 30 s), PCIe-replay og AER-teller (60 s), `time_in_state` (10 s).

### Én gang ved oppstart

GPU: navn, merke, arkitektur, UUID, VBIOS, InfoROM, GSP, PCI-info, driver, CUDA, compute capability, maks klokker, klokkeområde per P-tilstand, støttede P-tilstander, effektgrenser (område og standard), temperaturterskler, klokkeoffset med område, antall vifter, område og kjølerinfo, CUDA-kjerner, minnebuss, persistence og «display aktiv». Effektgrense, offset og temperaturmål leses også på nytt etter hver endring fra Kontroller.

CPU og system: cpuinfo (navn, familie, flagg, mikrokode), topologi, cache, cpufreq-område og -driver, DMI, OS, PCI-brikkesett, disker.

### Bare på forespørsel

PCIe-gjennomstrømning (20 ms per retning), last per prosess, ytelsesmodi (9 ms), støttede minneklokker, `nvidia-smi -q` (delenummer, 100 ms). Hentes bare når brukeren åpner det aktuelle panelet, og høyst hvert 5. s.

### Når vinduet er skjult

Demoen stopper kildene 10 s etter at siste lytter forsvant. For produksjon er dette en avveining for hovedsesjonen: skal historikken (Grafer, 48 t) fortsette når vinduet er lukket? I så fall bør den slanke runden gå videre hvert 5. s i bakgrunnen (ca. 0,4 % av én kjerne), og tilbake til 1 Hz når vinduet vises.

### Feil og grenser

- Hvert kall gir `nvmlReturn_t`. NOT_SUPPORTED huskes per verdi etter første forsøk, og verdien vises som «Ikke støttet» uten nye kall.
- GPU_IS_LOST, UNKNOWN eller tre feil på rad: vis «Skjermkortet svarer ikke», prøv `nvmlInit_v2` på nytt med økende pause (1, 2, 5, 10 s).
- Pollingen skal **aldri** gå raskere enn 2 Hz, og aldri ta igjen tapte runder etter hverandre (en kø med «etterslep» ville vekket kortet). Bruk fast takt og hopp over runder som kommer for sent.

---

## 13. Verktøyene

| Fil | Hva | Kjør |
|---|---|---|
| `verktoy/nvml_probe.py` | Kaller 92 NVML-get-funksjoner (135 målinger: 118 OK, 17 NOT_SUPPORTED), skriver støtte, verdi og median tid per kall. `--sakte`: ett kall om gangen med pause, så kortet blir i P8. `--json`: maskinlesbart | `python3 -I nvml_probe.py [--sakte] [--json]` |
| `verktoy/nvml_timing.c` | Oppstartstid, RSS, pollerunde. `watch MS SEK` viser om pollingen vekker kortet, `calls N` gir tid per kall i ro, `lean MS SEK` måler den slanke runden | `gcc -O2 -o ~/.cache/rigg-build/nvml_timing nvml_timing.c -ldl` |
| `verktoy/sys_probe.py` | Leser CPU-, minne-, disk-, nett-, hwmon-, DMI- og GPU-sysfs-kilder og måler «åpne+les» mot «pread». `--exec` måler også `lscpu`, `sensors` og `nvidia-smi` | `python3 -I sys_probe.py [--exec]` |

Alle tre bare leser. `nvml_probe.py` nekter å slå opp funksjonsnavn som ikke begynner med `nvmlDeviceGet` eller `nvmlSystemGet`. `sys_probe.py` nekter stier med `/dev/`, `eeprom` og `/sys/firmware/dmi/tables`. Standardmodusen i `nvml_timing` poller tett og løfter kortet til P0 i noen sekunder. Det er ufarlig, men gir P0-tall.
