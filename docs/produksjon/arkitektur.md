# Agentic Rigg 1.0 · arkitektur

*Produkt- og arkitekturagenten, 7. november 2026. Hører sammen med `produktspesifikasjon.md` (hva) og `byggeplan.md` (hvem og når).*

## 1. Valgene på én side

| Spørsmål | Valg | Hovedgrunn |
|---|---|---|
| Prosessmodell | **To prosesser:** tjenesten `agentic-riggd` (systemd `--user`) og vinduet `agentic-rigg`. Senere en tredje, en root-hjelper bak polkit, bare for skriving | Lukket vindu koster 0 MB. Historikk og logging overlever at vinduet lukkes eller krasjer. GTK og grafikkdriveren er aldri i minnet når ingen ser på |
| Hva «lukk» betyr | Vinduets prosess **avslutter**. Tjenesten lever videre bare med opptak i bakgrunnen eller logging | Et skjult GTK-vindu holder NVIDIA-driverens GL-del i minnet (målt nedenfor) |
| libadwaita | **Nei.** Rent GTK4 med egen CSS | Demoens eget, tette uttrykk; `Adw.init()` målt til 118 ms og 16 ekstra delte bibliotek; GTK 4.22 har selv det vi trengte libadwaita til |
| Tråder i vinduet | Én (GTK-hovedtråden). Socketen leses asynkront med GIO | Ingen låser, ingenting å synkronisere |
| Kjøretid i tjenesten | GLib/GIO-hovedløkke og én prøvetråd for NVML og `/proc`/`/sys`. Ikke tokio, ikke zbus | GLib er alt lastet i alle skrivebordsprosesser (delt minne), og gir socket, D-Bus (varsler, logind, systemd) og tidtakere i én løkke |
| NVML | Bare i tjenesten, via `nvml-wrapper` (laster `libnvidia-ml.so.1` med `dlopen`) | NVML koster 23 MB privat minne [Målt]. Vinduet skal aldri betale det |
| Grafer | Egen widget med `snapshot()`, `GskPath` og bufrede bakgrunnsnoder. Tegnes bare når data eller størrelse endres | 1–2 Hz data trenger ikke en løkke på 240 Hz |
| 3D på Lys | `GtkGLArea` med GLES 3.0, shaderne fra `web/js/lys3d/` nesten uendret | WebGL 2 er GLES 3.0 |
| Innstillinger | JSON i `~/.config/agentic-rigg/`: `ui.json` (eies av vinduet) og `tjeneste.json` (eies av tjenesten) | Samme nøkler og format som demoens eksport, én skriver per fil, ingen schema-kompilering |
| Historikk | Samme tre lag som `server/history.js` (1 s i 15 min, 10 s i 6 t, 60 s i 48 t), i tjenesten, lagret binært i `~/.local/state/agentic-rigg/historikk.bin` | Høyst 6,8 MB i minnet og på disk, mot 16 MB JSON i demoen |
| Installasjon | `~/.local` etter mønsteret fra Klipp, pluss `~/.config/systemd/user/agentic-riggd.service`. Ingenting aktiveres ved innlogging uten at brukeren slår det på | REGLER § Sikkerhet 2 |
| Versjon | 1.0.0 · bygg 16, kanal «stable». Ingen alpha-merke. Én kilde: `app/version.json` | Om og `--version` |

---

## 2. Målinger som valgene bygger på

Målt 7. november 2026 på maskinen i REGLER (RTX 2080, driver 610.43.02 med GSP, Ryzen 7 2700X, GNOME 50.1 på Wayland, Ubuntu 26.04.1). Bare lesekall. Skriptene ligger i scratchpad-mappa til denne økta og er gjengitt i kortversjon her, så de kan gjentas. `docs/produksjon/maskinvare-revisjon.md` (maskinvareagenten) har flere NVML-tall når den er ferdig; der tallene er uenige, gjelder de nyeste.

### 2.1 NVML (Python `ctypes`, bare `nvmlInit_v2`, `Get`-kall og `nvmlShutdown`)

| Hva | Tall |
|---|---|
| `dlopen("libnvidia-ml.so.1")` | 0,4 ms |
| `nvmlInit_v2` | 12–13 ms (to kjøringer) |
| Minne etter `nvmlInit_v2` | RSS 11 → 34 MB, **privat (Private_Dirty) 3 → 23 MB**, én ekstra tråd |
| `nvmlShutdown` | 1,2 ms |

CPU-tid per kall (median av 15, to kjøringer; tallene varierer omtrent 2× med CPU-klokka). Kallene går gjennom GSP-fastvaren og koster mer enn man skulle tro:

| Kall | ms |
|---|---|
| `GetCurrentClocksEventReasons` (klokkebegrensninger) | **11–18** |
| `GetPcieThroughput` | 20 ms veggtid (venter), 0,3 ms CPU |
| `GetPowerUsage` | 1,6–3,5 |
| `GetTotalEnergyConsumption` | 3,8 |
| `GetClockInfo` (per klokke) | 0,8–2,0 |
| `GetUtilizationRates` | 0,6–1,3 |
| `GetFanSpeed_v2` (per vifte) | 0,5–1,1 |
| `GetEncoderUtilization`, `GetDecoderUtilization` | 0,5–0,7 hver |
| `GetTemperature`, `GetPerformanceState`, `GetEnforcedPowerLimit`, `GetCurrPcieLinkGeneration` | 0,1–0,2 |
| `GetMemoryInfo`, `GetCurrPcieLinkWidth` | 0,01 |

Tallene over er tatt mens kortet var i P0 (skrivebordet var i bruk). `maskinvare-revisjon.md` har målt det samme i ro (P8), og der er alt dyrere:

| Fra maskinvarerevisjonen | P8 (i ro) | P0 |
|---|---|---|
| `GetCurrentClocksEventReasons` | 24–27 ms | 0,7 ms |
| Hele demorunden (19 kall) | 54–62 ms | 10–11 ms |
| Slank runde (8–10 kall) hvert sekund | ca. 15–17 ms = **1,9 % av én kjerne** | ca. 4,5 ms |
| Demoens `nvidia-smi -lms 500` | 9,5 % av én kjerne | – |
| Tett polling (runder rett etter hverandre) | **vekker kortet** til P0 og 55–69 W på under 1 s | – |
| `nvmlDeviceGetFieldValues` (9 felt) | 9,6 ms, ikke billigere enn enkeltkallene | 3,0 ms |

Konsekvens: høyst 2 Hz, aldri runder rett etter hverandre, og en prøveplan med rask og treg takt (kapittel 4.3).

### 2.2 GTK og grafikkdriveren

| Hva | Tall |
|---|---|
| EGL-kontekst på NVIDIA (pbuffer, det GTKs GL-tegner gjør ved første bilde) | **117 ms** første gang. RSS +97 MB, PSS +31 MB, **privat +18 MB**. Etter `eglTerminate` er privat minne fortsatt +17 MB (bibliotekene blir liggende) |
| `Adw.init()` (Python, på Wayland, uten vindu) | **118–121 ms** (tre kjøringer). Noe av dette er fontconfig og ikontema som GTK også gjør ved første tegning, så nettokostnaden er lavere, men ikke null |
| Ekstra delte bibliotek libadwaita 1.9 trekker inn (`ldd`) | 16, blant annet `libappstream`, `libcurl-gnutls`, `libldap`, `libssl`, `libxmlb` |
| GTK-vindu med 200 rader (Python, broadway) | ca. 80 ms fra import til vinduet er bygd; med libadwaita ca. 175 ms |
| Klipp (Rust, GTK4 + libadwaita, i ro uten vist vindu) | RSS 78 MB, PSS 36 MB, privat 26 MB, 9 tråder. Ingen NVIDIA-GL-bibliotek lastet, så det er gulvet for en GTK-prosess |
| Demoen (Node-serveren) | ca. 160 MB RSS (brukerens måling) |

Ubuntu har AppIndicator-utvidelsen på (`ubuntu-appindicators@ubuntu.com` er aktiv). GTK 4.22.4 har `gtk-interface-color-scheme`, `gtk-interface-contrast`, `gtk-interface-reduced-motion` og CSS-mediespørringene `prefers-color-scheme` og `prefers-contrast` (sett i `libgtk-4.so.1`).

---

## 3. Prosessmodellen

### 3.1 Kandidatene

**A. Tjeneste og vindu.** `agentic-riggd` kjører som systemd `--user`-tjeneste, holder NVML åpent, tar prøver, holder historikken og serverer data over en Unix-socket (kontrakten i `docs/daemon-api.md`, utvidet i kapittel 5). `agentic-rigg` er GTK-vinduet som kobler til og tegner. Senere: `agentic-rigg-helper`, en system-tjeneste med root bak polkit, bare for skriving (LACT-mønsteret).

**B. Én prosess.** En GtkApplication som blir liggende med skjult vindu (`hold()`, `--gapplication-service`), og som selv leser NVML og holder historikken.

### 3.2 Sammenligning (estimert fra målingene i kapittel 2)

| | A: tjeneste + vindu | B: én prosess |
|---|---|---|
| **Vinduet lukket, privat minne** | Tjenesten: NVML 23 + GLib og egen kode ca. 3 + historikk opptil 7 ≈ **30–35 MB**. Med bakgrunn av: 0 etter 60 s | GTK ca. 15–25 + GL-driveren 17 + NVML 23 + historikk 7 ≈ **65–75 MB**, hele tiden |
| **Vinduet lukket, CPU** | Slank runde hvert 2. s: ca. 1 % av én kjerne i P8 (revisjonen: 1,9 % ved 1 s) | Det samme, pluss GTK-hovedløkka |
| **Vinduet lukket, RSS** | ca. 40–45 MB | ca. 170–190 MB (delte bibliotek fra GTK og NVIDIA-GL) |
| **Vinduet åpent, privat minne totalt** | 30–35 + vinduet 45–60 ≈ 75–95 MB | 70–90 MB |
| **Kald start til første bilde** | GTK-oppstart og CSS ca. 30–60 ms (Rust), GL-kontekst ca. 117 ms, siden ca. 10–30 ms ≈ **150–250 ms**. Tjenesten starter parallelt (prosess + 12 ms NVML) og er ikke på den kritiske stien | Første gang som A. Etterpå: |
| **Gjenåpning** | Som kald start: **150–250 ms** | Vise skjult vindu: **20–50 ms** |
| **Lukk** | Vinduet skjules og prosessen avslutter. Alt frigjøres | Vinduet skjules. Minnet blir |
| **Krasj i GTK-koden** | Historikk og logging fortsetter | Historikk og logging stopper |
| **Synlig for brukeren i bakgrunnen** | `systemctl --user status agentic-riggd`, Innstillinger › Generelt, systemfeltet (BØR) | Ingenting i ren GNOME. Bakgrunnsportalen (`org.freedesktop.portal.Background`) viser bare apper i sandkasse (Flatpak) under «Bakgrunnsapper» [Antakelse, godt underbygd: portalen kjenner ikke app-ID for vertsprosesser] |
| **Senere skriving** | Root-hjelperen er en tredje prosess uansett. Vinduet snakker fortsatt bare med `riggd` | Må uansett splittes |
| **Kompleksitet** | Protokoll, to binærer, systemd-enhet | Enklere å bygge |

### 3.3 Valg: A

Modell A vinner på det brukeren har bedt om. Lukket vindu betyr null minne for GTK og grafikkdriveren, og 30–35 MB for tjenesten bare når du har valgt opptak i bakgrunnen. Den er også robust: et krasj i vinduet stopper ikke loggingen. Kontrakten finnes alt (`docs/daemon-api.md`), demoen har bevist flyten, og LACT, CoreCtrl og Mission Center har samme deling. Prisen er gjenåpningen, som er 150–250 ms i stedet for 20–50 ms. Den holdes nede med:

1. **Ikke libadwaita** (sparer opptil ca. 100 ms og 16 bibliotek, kapittel 6.1).
2. **Ingen blokkerende arbeid før første bilde.** `ui.json` (noen kB) leses synkront. Alt annet, som tilkobling, statiske data og historikk, kommer etter første bilde.
3. **Lat bygging av sider.** Bare startsiden og skallet bygges ved oppstart. De andre sidene bygges første gang de vises, og blir liggende til vinduet lukkes.
4. **CSS og ikoner i GResource**, kompilert inn i binæren. Ingen filer å lete etter.
5. **Release-profil** som Klipp (`lto`, `codegen-units = 1`, `strip`), se kapittel 9.
6. **Måling av tegnerne.** Optimaliseringsrunden måler `GSK_RENDERER=ngl`, `vulkan` og `cairo` for første bilde, minne og CPU, og velger standard. Valget kan settes i Om › Feilsøking (BØR). Med `cairo` lastes ikke GL-driveren i det hele tatt, men 3D på Lys trenger GL (reserven er det flate kortet).
   **Valgt i runde 3C: `cairo`** (`rigg/src/main.rs`, `DEFAULT_RENDERER`; `GSK_RENDERER` i miljøet vinner). Første bilde ca. 150 ms mot ca. 290 ms med GL og 460 ms med Vulkan, og 12–15 MB privat minne mot 46–106 MB med GL. Grafene og sparklinjene tegnes med cairo direkte (`widgets::Painter`), og Lys går med høyst 30 bilder/s. Tallene står i `maalinger.md`, runde 3C.

### 3.4 Reserveplan

Hvis kald start fortsatt er over 250 ms etter runde 3: «rask gjenåpning». Vinduet skjules ved lukking og avslutter først etter 5 min uten ny aktivering. Dette er en ren endring i `rigg/src/app.rs` og påvirker ikke resten.

**Runde 3C:** trengs ikke. Med cairo er kald start ca. 150 ms, og lukking avslutter prosessen på under 25 ms.

### 3.5 Hva GNOME tillater, og hva det betyr

- **Ingen systemfelt i ren GNOME.** Ubuntu har AppIndicator-utvidelsen på som standard. Systemfeltet er derfor BØR, eid av tjenesten (`ksni`, som Klipp bruker, eller `gio::DBusConnection` direkte), og appen må virke likt uten.
- **Bakgrunnsportalen** gir ikke noe for en app utenfor sandkasse. Vi bruker systemd `--user` i stedet. Det er synlig, kan stoppes, logger til journalen og starter på nytt ved krasj.
- **Autostart** er `systemctl --user enable agentic-riggd.service` (`WantedBy=default.target`), ikke en `.desktop`-fil i `~/.config/autostart`. Bare brukeren slår det på, fra Innstillinger.
- **Wayland:** ingen posisjon for vinduet, ingen «alltid øverst» og ingen overlegg over andre vinduer. Globale hurtigtaster går via portalen GlobalShortcuts (Klipp bruker den med `ashpd`).
- **Skrivebordsvarsler** sendes av tjenesten med `org.freedesktop.Notifications` over D-Bus-økta (tjenesten har `DBUS_SESSION_BUS_ADDRESS` fra systemd-brukerøkta). Handlingen «Åpne» aktiverer appen med `gio::DesktopAppInfo::launch`.

### 3.6 Livsløpet

```
Bruker starter Agentic Rigg
  └─ agentic-rigg (GtkApplication, app-ID io.github.AgenticRigg)
       ├─ finnes en forekomst? → den viser vinduet, ferdig (≤ 50 ms)
       ├─ les ui.json → bygg skall + startside → første bilde
       └─ (asynkront) koble til $XDG_RUNTIME_DIR/agentic-rigg/riggd.sock
            ├─ ingen socket → StartUnit("agentic-riggd.service") over D-Bus til systemd --user
            │                  (reserve uten systemd: start agentic-riggd selv, frakoblet med setsid)
            │                  prøv igjen hvert 25. ms i opptil 2 s
            └─ hello → get_static → subscribe(intervalMs) → get_history for synlige grafer

Bruker lukker vinduet
  └─ vinduet skjules straks, lagrer vindusstørrelsen (noen byte), avslutter
       └─ riggd ser at klienten kobler fra
            ├─ logging aktiv → fortsetter
            ├─ «Fortsett i bakgrunnen» på → fortsetter med takten i bakgrunnen (standard 2 s)
            └─ ellers → stopper etter 60 s (ny klient innen da avbryter)
                 └─ ved stopp: lagre historikk og min/maks, nvmlShutdown, slett socketen
```

Tjenesten avslutter også rent ved SIGTERM (systemd stop, utlogging): lagrer, lukker loggen, `nvmlShutdown`.

---

## 4. Tjenesten `agentic-riggd`

### 4.1 Oppbygning

- **Hovedtråden** kjører en `glib::MainLoop`. Den eier socket-tjenesten (`gio::SocketListener` på en `UnixSocketAddress`), klientøktene, historikken, statistikken (min/maks/snitt), loggfila, alarmene og D-Bus (varsler, logind `PrepareForSleep`, senere systemfeltet).
- **Prøvetråden** eier NVML og de åpne filene i `/proc` og `/sys`. Den tar én prøve per tikk og sender den til hovedtråden over en kanal (`async-channel`, som glib kan vente på). NVML-kallene blokkerer i 10–20 ms og skal aldri gå på hovedtråden.
- Ingen andre tråder. Ingen tokio.
- Tilgang: socketmappa `0700`, socketen `0600`, og `SO_PEERCRED` sjekker at klienten har samme UID (mocken kunne ikke det). Høyst 16 klienter og 64 KiB per linje, som kontrakten.

### 4.2 Lesing fra `/proc` og `/sys`

Filene åpnes én gang og leses med `pread` fra start (ingen `open`/`close` per prøve; revisjonen målte 1,1–1,3 µs mot 9 µs): `/proc/stat`, `/proc/meminfo`, `cpuinfo_avg_freq` for hver tråd, `temp*_input`, `fan*_input`, `in*_input`, `power*_input` i `/sys/class/hwmon/*`, og `/sys/class/net/*/statistics/{rx,tx}_bytes`. Nye hwmon-enheter og nettkort oppdages hvert 10. s. Samme regler som `server/system.js`: root-only-filer åpnes ikke, `EACCES` blir «krever root». Rettingene fra revisjonen kap. 11 gjelder: CPU-temperaturen er **Tdie** (Tctl ligger 10 °C over på 2700X og står i verktøytipset), og «buffer og cache» skiller ut `Shmem`.

### 4.3 Prøveplanen

Planen følger `maskinvare-revisjon.md` kap. 12, som er fasit for hvilke kall som går i hvilken takt. Kort fortalt:

| Takt | Hva | Kostnad per runde [Målt, revisjonen] |
|---|---|---|
| **Rask, slank runde.** 1 s mens et vindu abonnerer (0,5 s hvis brukeren velger det; aldri raskere). **I bakgrunnen etter valget «Takt i bakgrunnen»: 1, 2 (standard) eller 5 s** (1–0,2 Hz) | NVML (8 kall): temperatur, energiteller (effekt = ΔmJ/Δms, snitt for perioden), grafikk- og minneklokke, `UtilizationRates`, `MemoryInfo_v2` (ikke v1, som regner det reserverte som brukt), P-tilstand, vifte 0. PCIe-generasjon fra sysfs (`current_link_speed`), ikke NVML. CPU, minne, nett, hwmon (Tdie og Tctl) | ca. 15 ms i P8, ca. 4,5 ms i P0 + under 0,3 ms `/proc`/`/sys` |
| **Hvert 2. s** | Vifte 1, turtall, SM- og videoklokke, encoder/decoder, dynamiske P-tilstander. Klokkehendelser **bare når kortet ikke er i P8** (ellers «Tomgang», og kallet ville kostet 25 ms) | – |
| **Hvert 5. s** | Håndhevet effektgrense, viftepolicy, målfart, PCIe-bredde (sysfs `current_link_width`), governor og boost, `loadavg` | – |
| **Hvert 10.–60. s** | BAR1 i bruk, bruddtellere for nedklokking, PCIe-replay | – |
| **Ved behov** (bare mens panelet synes, høyst hvert 5. s) | PCIe-gjennomstrømning (20 ms per retning), last per prosess, ytelsesmodi | – |
| **Én gang** (oppstart og etter `driver_updated`/`lost`) | Statiske data (revisjonen kap. 12). **Fordelt over de første sekundene**, ikke i én blokk, så kortet ikke vekkes. Bufres på disk med UUID og driverversjon som nøkkel, så `get_static` kan svare straks ved neste oppstart | ca. 50 ms i P8 til sammen |

Regler som ikke kan brytes:

- **Aldri raskere enn 2 Hz**, og **aldri ta igjen** tapte runder. Fast takt (`glib::timeout` på absolutt tid i prøvetråden), og en runde som kommer for sent, hoppes over.
- `NOT_SUPPORTED` huskes per verdi etter første forsøk.
- Takten følger det klientene ber om i `subscribe` (raskeste vinner, men aldri under 500 ms) og hvilke sider som synes (`set_focus`, kapittel 5).
- `nvmlDeviceGetFieldValues` brukes ikke. Revisjonen målte at det ikke er billigere.

Historikken får en rad per sekund uansett takt (1 s-bøtter med min, snitt og maks). En 2 s-takt i bakgrunnen gir en prøve i annenhver bøtte, og 5 s en i hver femte. Grafene behandler et hull som sammenhengende når det er kortere enn tre ganger takten som gjaldt (tjenesten lagrer takten per tidsrom i historikken), så bakgrunnsopptak ikke ser ut som brudd.

NVML kalles aldri på GTK-hovedtråden i noen prosess. Vinduet laster ikke NVML i det hele tatt (det koster ca. 24 MB fordi NVML laster `libcuda.so` selv).

### 4.4 Historikk, statistikk og lagring

- **Historikk:** samme lag, grenser og spørring som `server/history.js` (`TIERS`, `WINDOWS`, `MAX_SERIES = 96`, `MAX_KEYS = 16`, 10–4000 punkter, min/snitt/maks per bøtte, hull der det ikke kom prøver). Lagres som `f32` i et enkelt binærformat med versjonsnummer, nøkkelliste og radene per lag. Skrives atomisk (temp-fil og `rename`) hvert 60. s og ved stopp. Ødelagt fil gir ny, tom historikk og en melding i journalen. Største fil: 5 940 rader × 96 serier × 12 byte ≈ 6,8 MB.
- **Min/maks/snitt** per sensor siden siste nullstilling. Min og maks lagres i `minmaks.json` (høyst hvert 30. s, som demoen) når «Husk min og maks» er på. Snitt er sum og antall, og gjelder siden nullstilling eller siden tjenesten startet.
- **Plassering:** `$XDG_STATE_HOME/agentic-rigg/` (standard `~/.local/state/agentic-rigg/`). Det er på systemdisken, ikke Transcend-disken.

### 4.5 Logging og alarmer

- Loggen skrives av hovedtråden med en `BufWriter` som tømmes hvert 5. s. Kolonnene låses ved start. Formatene og grensene står i produktspesifikasjonen 3.2.
- Alarmene vurderes på hver prøve mot tersklene i `tjeneste.json` (som vinduet holder i takt med `ui.json`), med varighet, hysterese og grense for hvor ofte (produktspesifikasjonen 3.5).

### 4.6 Tilstander for kortet

`rigg-hw` oversetter NVML-feil til tilstandene i produktspesifikasjonen 3.8 (`ok`, `driver_updated`, `no_driver`, `lost`, `suspended`). Driverversjonen sammenlignes med `/sys/module/nvidia/version` hvert 60. s, så en oppdatert driver oppdages også mens den gamle fortsatt svarer. Tjenesten krasjer aldri på en NVML-feil. Panikk i prøvetråden fanges, prøvetråden startes på nytt, og hovedtråden fortsetter.

### 4.7 Systemd-enheten

`~/.config/systemd/user/agentic-riggd.service` (skrevet av `install.sh`):

```ini
[Unit]
Description=Agentic Rigg – sensorer og historikk
Documentation=file://%h/.local/share/doc/agentic-rigg/README.md

[Service]
Type=notify
ExecStart=%h/.local/bin/agentic-riggd
Restart=on-failure
RestartSec=2
# Bare lesing. Ingen root, ingen skriving til kortet.
NoNewPrivileges=yes
LockPersonality=yes
RestrictRealtime=yes
MemoryHigh=96M
MemoryMax=128M
Nice=5

[Install]
WantedBy=default.target
```

- `Type=notify`: tjenesten sier `READY=1` (en linje til `$NOTIFY_SOCKET`, uten egen crate) når socketen er bundet og første prøve er tatt.
- `MemoryMax=128M` er et sikkerhetsnett mot lekkasjer, godt over målet på 45 MB.
- Ikke `ProtectHome`: tjenesten skal skrive logger i en mappe brukeren velger.
- Enheten **aktiveres ikke** av `install.sh`. Vinduet starter den ved behov (`StartUnit`). «Start tjenesten ved innlogging» i Innstillinger kjører `EnableUnitFiles` over D-Bus.

### 4.8 Utviklingsforekomster

Agentene må aldri koble seg til, stoppe eller overskrive en installert forekomst. Derfor:

- `RIGG_PROFIL=dev` (eller debug-bygg) gir app-ID `io.github.AgenticRigg.Devel`, socket `riggd-dev.sock`, `~/.config/agentic-rigg-dev/` og `~/.local/state/agentic-rigg-dev/`. Testene bruker midlertidige mapper (`RIGG_SOCKET`, `RIGG_CONFIG_DIR`, `RIGG_STATE_DIR`).
- Testene starter `agentic-riggd` som egen barneprosess, ikke via systemd.
- Socketen ligger ikke der demoens mock har sin (`mock-<id>.sock`), så demoen på port 4318 merker ingenting.

---

## 5. Protokollen

Rammen er uendret: NDJSON over Unix-socket, `hello` først, `id` på forespørsler, hendelser uten `id`, feilkodene i `docs/daemon-api.md` kap. 7. Protokollen er fortsatt **1**: alt under er nye operasjoner, felt og hendelser, som kontrakten sier ikke er brudd. Typene står ett sted, i `rigg-core/src/proto.rs`, og brukes av begge sider. Grunnmuragenten skriver den fullstendige beskrivelsen i `docs/produksjon/protokoll.md`.

**Endringer i `hello`:**

```json
"daemon": {"name": "agentic-riggd", "version": "1.0.0", "build": 16, "mock": false, "pid": 4242, "backend": "nvml"},
"capabilities": {
  "gpus": ["0000:1c:00.0"],
  "ops": ["hello", "ping", "list_gpus", "get_static", "get_system", "get_limits", "get_state", "get_telemetry",
          "subscribe", "unsubscribe", "set_focus", "get_history", "get_stats", "reset_stats",
          "log_start", "log_stop", "log_status", "get_settings", "set_settings", "shutdown_when_idle"],
  "events": ["telemetry", "device", "stats", "log", "alarm", "settings", "shutdown"],
  "write": {"state": "locked", "steps": [{"id": "power", "status": "not_approved"}, {"id": "clocks", "status": "not_approved"},
            {"id": "fan_manual", "status": "not_approved"}, {"id": "fan_curve", "status": "not_approved"},
            {"id": "temp_target", "status": "not_approved"}, {"id": "rgb", "status": "not_approved"}]}
}
```

Skriveoperasjonene (`apply`, `set_*`, `confirm`, `revert`, `reset`, `rgb_*`) står **ikke** i `ops` med `backend: "nvml"`. Kalles de likevel, svarer tjenesten med den nye feilkoden `write_locked` («Endringer er låst. Skriving til skjermkortet er ikke slått på.»). Med `--backend sim` finnes de, og oppfører seg som mocken, så bekreftelsesflyten kan testes.

**Nye operasjoner:**

| op | args | result |
|---|---|---|
| `get_static` | `gpu` | Alt Grafikkort-siden trenger fra driveren og sysfs (som `/api/gpuinfo`, uten spesifikasjonstabellen, som ligger i `rigg-core`) |
| `get_system` | – | Faste systemdata (som `/api/system`) |
| `subscribe` | `topics`, `intervalMs?` (500–2000) | `{topics, intervalMs}`. Den raskeste klienten bestemmer takten |
| `set_focus` | `pages: ["kontroller"]` | Hvilke sider som synes, for den trege takten (4.3). `[]` når vinduet er minimert |
| `get_history` | `window`, `keys`, `points` | Som `/api/history`: `{window, step, from, to, oldest, t, series: {key: {min, avg, max}}}` |
| `get_stats` / `reset_stats` | `keys?` (alle når utelatt) | `{since, stats: {key: {min, max, avg, n}}}` |
| `log_start` / `log_stop` / `log_status` | `keys?`, `intervalMs`, `dir`, `format` | `{active, file, rows, bytes, startedAt}` |
| `get_settings` / `set_settings` | delvis `tjeneste.json` | hele `tjeneste.json`, validert |
| `shutdown_when_idle` | – | Stopper etter frist når siste klient er borte, også med bakgrunn på («Stopp tjenesten nå» bruker vanlig `StopUnit`) |

**Hendelser:** `telemetry` har `{t, gpus: {"<id>": Sample}, sys: SysSample}` med feltnavnene fra demoens `sample` og `sys`, så sidene kan porteres rett. `device` sendes når tilstanden til et kort endres (3.8 i produktspesifikasjonen). `stats` sendes hvert 5. s til klienter som viser Sensorer. `log` og `alarm` ved overganger.

**Sensornøkler:** `gpu:<pci-buss>/<felt>` (`gpu:0000:1c:00.0/temp`), `cpu/load`, `cpu/thread/3/load`, `mem/usedPct`, `net/<grensesnitt>/rx`, `hwmon/<navn>/<fil>` (`hwmon/k10temp/temp1`). Samme mønster som demoens `KEY_RE`. Nøklene brukes likt i historikk, statistikk, logg, festede sensorer og alarmer.

---

## 6. Vinduet `agentic-rigg`

### 6.1 libadwaita eller rent GTK4

**Valg: rent GTK4 med egen CSS.**

For libadwaita:

- `AdwStyleManager` følger GNOMEs lyst/mørkt, kontrast og aksent.
- Ferdige deler: `AdwToastOverlay`, `AdwNavigationSplitView`, `AdwBreakpoint`, `AdwAboutDialog`, preferanseradene.
- Klipp bruker det allerede.

Mot libadwaita:

- **Uttrykket.** Demoen har sitt eget tette uttrykk i HWiNFO-stil: 26 px rader, tynne paneler og fire fargetemaer. Adwaita-arket har høye rader (en `AdwActionRow` er ca. 50 px), runde kort og egne farger, og vi måtte overstyrt nesten alt. De ferdige delene passer heller ikke: preferanseradene er for høye, `AdwAboutDialog` ser annerledes ut enn Om, og toastene står nederst i midten.
- **Tiden.** `Adw.init()` er målt til ca. 120 ms, og libadwaita trekker inn 16 bibliotek (appstream, curl, ldap, ssl). Det går rett inn i kald start.
- **GTK 4.22 dekker behovet selv.** Den har `gtk-interface-color-scheme`, `gtk-interface-contrast` og `gtk-interface-reduced-motion`, mediespørringer i CSS og CSS-variabler.
- **Aksenten** leses fra portalen (`org.freedesktop.appearance` `accent-color`) med ett D-Bus-kall, når vi vil følge den (BØR).

Det vi lager selv er smått, og demoen har dem alt: varsel nederst til høyre (en `GtkRevealer` i en `GtkOverlay`), segmentknapp, bryterrad, panel med stripe, bruddpunkter (lytting på bredden i `size_allocate`), snarveisdialog og Om.

### 6.2 Oppbygning

- `GtkApplication` med `HANDLES_COMMAND_LINE` (`--side oversikt`, `--hint`, `--version`).
- Ett `GtkApplicationWindow` med egen tittellinje (`GtkHeaderBar` med egen tittel-widget, så vindusknappene er GNOMEs).
- Skallet: sidefelt (kortvelger, sider, statusblokk, Innstillinger, snarveier, legg sammen) og en `GtkStack` for sidene. Innstillinger er en egen visning i en ytre `GtkStack` («app» / «innstillinger»), slik v0.0.15 gjør det med `data-view`.
- Én `AppState` (i `Rc`) med siste prøve, statistikk, statiske data, innstillinger og tilstand for tjenesten. Sidene abonnerer på endringer med enkle callbacks. Ingen rammeverk som relm4.
- **Tekst som endres** settes bare når teksten faktisk er ny (`set_label` med sammenligning), så GTK ikke må måle på nytt. Tall bruker tabulære siffer (`font-feature-settings: "tnum"`), så de ikke danser.
- **Sensorer** bruker `GtkColumnView` med `GtkTreeListModel` (gruppene) og gjenbrukte rader. Hver rad oppdaterer bare sine egne etiketter.
- **Når vinduet ikke synes** (`GdkToplevelState::MINIMIZED` eller `SUSPENDED`): ingen tick-callbacks, ingen tegning, og `set_focus([])`, så tjenesten går over til sakte takt.

### 6.3 Temaer i GTK-CSS

Demoens tokens (`web/styles.css`, seksjon 2) flyttes til `app/data/theme/tokens.css` som CSS-variabler. GTK 4.22 støtter egendefinerte egenskaper (`--navn`) og `var()`. Valgene settes som CSS-klasser på vinduet, slik demoen setter `data-*` på `<html>`:

| Demoen | GTK |
|---|---|
| `:root`, `:root[data-resolved='dark']` | `window.rigg { … }`, `window.rigg.dark { … }` |
| `[data-palette=sand]` osv. | `window.rigg.pal-sand`, `.pal-sand.dark` |
| `[data-contrast='high']` | `window.rigg.hc` (og `.hc.dark`) |
| `:root[data-accent=teal]` | `window.rigg.acc-teal` (seks blokker, pluss `.dark`-varianter der demoen har det) |
| `data-density`, `--fs-scale` | `.tett-kompakt`, `.tett-luftig` og fire klasser for skrift (`.skrift-92` … `.skrift-116`) |
| `data-motion` | Bevegelsen styres i Rust (se 6.6), ikke i CSS |

Lyst/mørkt «System» følger `gtk-interface-color-scheme` (varsles med `notify`). Høy kontrast «Automatisk» (BØR) følger `gtk-interface-contrast`. GTKs egne widgeter (knapper, felt, rullefelt, verktøytips, popovere) får fargene ved at `tokens.css` også definerer fargene GTKs standardtema bruker (`@define-color window_bg_color`, `view_bg_color`, `accent_bg_color`, `accent_color`, `borders`, `popover_bg_color` osv.) per klasse. Grunnmuragenten verifiserer mot GTK 4.22 hvilke navn standardtemaet faktisk leser, og om `@define-color` kan bruke `var()`. Går ikke det, skrives verdiene ut for hver kombinasjon av et lite byggeskript (`build.rs`) fra én tabell.

**Kontrasten** sjekkes med en Rust-test som gjør det samme som `scripts/contrast.js`: alle 96 kombinasjoner (4 temaer × lys/mørk × 6 aksenter × høy kontrast), tekst ≥ 4,5:1, UI-grenser ≥ 3:1, også seriefargene. Testen leser verdiene fra `tokens.css`, så CSS og test aldri kan gå i utakt.

Farger som tegnes i Rust (grafer, målere, 3D-scenen) hentes fra stilen med `widget.color()` og egne klasser (`.serie-temp`, `.serie-last` …), eller fra en liten tabell i `rigg-core/src/tokens.rs` som testen over også sjekker.

### 6.4 Grafer

En egen widget, `RiggChart` (underklasse av `gtk::Widget`), og en enklere `RiggSpark` for sparklinjer:

- `snapshot()` bygger linja som en `gsk::Path` (`PathBuilder`) og tegner med `append_stroke` og `append_fill` (fylt flate). Bånd (min/maks) er en fylt sti.
- **Bakgrunnen** (rutenett, akser, etiketter) bygges som én `gsk::RenderNode` og gjenbrukes til størrelse, tema, tidsvindu eller y-skala endres. Etikettene er `pango::Layout`-er som bufres per tekst.
- **Data:** høyst to punkter per pikselkolonne (min og maks i kolonnen), regnet i `rigg-core` når nye data kommer, ikke i `snapshot()`.
- **Rulling:** grafen flytter seg bare når tidsaksen har flyttet seg minst én hel piksel. Ved 1 min og 1200 px er det 20 ganger i sekundet. Ved 1 t er det én gang i sekundet, og ved 48 t sjelden. Med «Redusert» eller «Av» flytter grafen seg bare når en ny prøve kommer. En tick-callback er bare aktiv mens grafen synes, og fjernes ellers.
- Markør, pause, forklaring og hull som i demoen. Hendelser med `EventControllerMotion`. Markøren på flere grafer samtidig går gjennom `AppState`.
- Målet er under 0,5 ms per graf per bilde i `snapshot()`. Det måles i runde 3 med `GTK_DEBUG=snapshot` og `sysprof`.
- **Runde 3C:** med cairo-tegneren lages linjene og flatene som én cairo-node med vanlig `cairo_fill`/`cairo_stroke` (`widgets::Painter`), ikke som GSK-stier. Med GL/Vulkan brukes stiene som før. Det halverte tida per bilde på Oversikt (5,7 → 2,9 ms).

Målerne (temperatur og effekt) og stolpene er også egne små widgeter med `snapshot()`.

### 6.5 3D-kortet på Lys

- `GtkGLArea` med `set_allowed_apis(GdkGLAPI::GLES)` og GLES 3.0. Shaderne i `web/js/lys3d/gl/renderer.js` er `#version 300 es`, så de kan brukes nesten uendret. `card-model.js`, `layout.js`, `textures.js`, `mesh.js` og `math.js` porteres til Rust (`glam` for matriser er nok).
- GL-kall med `glow`, med funksjonspekere fra libepoxy, som GTK alt har lastet (samme mønster som eksempelet `glium_gl_area` i gtk4-rs).
- Tegnes bare ved behov: `queue_render()` ved input, endret effekt eller mens en animasjon går (frame clock). I ro tegnes ingenting, som i demoen («ingen bilder i ro»).
- GL-ressursene lages første gang Lys vises, og frigjøres i `unrealize` og når du går fra siden etter 30 s.
- **Reserve:** det flate kortet tegnet med `snapshot()` når GL ikke er tilgjengelig (`GtkGLArea::error`), med `GSK_RENDERER=cairo`, eller når et bilde tar over 12 ms (som demoen). Valget «3D | 2D» i overskriften beholdes.
- RGB-motoren (`web/js/rgb.js`, `RgbEngine`) porteres til `rigg/src/rgb.rs` og gir fargen per LED for begge tegnerne.

### 6.6 Bevegelse

Demoens bevegelser (`docs/bevegelse.md`) lages med overgangene i `GtkRevealer` og `GtkStack` der de passer, og ellers med en liten egen tween på frame clock (bare `opacity` og `transform` i `snapshot`). Nivået Automatisk/Full/Redusert/Av styrer varighetene: Automatisk følger `gtk-interface-reduced-motion` og `gtk-enable-animations`. Med «Av» er alle varigheter 0, og ingen tick-callbacks registreres.

### 6.7 Klienten mot tjenesten

`rigg/src/client.rs`: `gio::SocketClient` mot `UnixSocketAddress`, og en `gio::DataInputStream` som leser linjer asynkront på GTK-hovedløkka (`read_line_future`). Svar matches på `id` med en `HashMap<id, oneshot>`. Ved brudd: prøv igjen etter 0,5 s, så opptil 5 s (som `daemon-client.js`), og vis linja «Tjenesten svarer ikke» etter 2 s. Ingen tråder.

---

## 7. Innstillinger og filer

| Fil | Eier (eneste skriver) | Innhold |
|---|---|---|
| `~/.config/agentic-rigg/ui.json` | vinduet | Alle app-innstillingene fra demoens `mergeSettings` unntatt `startSimulated`, pluss `appIcon`, `eggsFound`, `settingsSection`, `lastPage`, `window` (bredde, høyde, maksimert) |
| `~/.config/agentic-rigg/tjeneste.json` | tjenesten (vinduet endrer via `set_settings`) | `background`, `backgroundIntervalMs` (1000 / 2000 / 5000, standard 2000), `keepMinMax` (= `sensorKeepMinMax`), `log` (nøkler, intervall, mappe, format), `alarms` (på/av, verdier), `thresholds` (kopi fra `ui.json`), `confirmSec` |
| `~/.config/agentic-rigg/profiler.json` | vinduet | De fem profilplassene og lysvalget |
| `~/.local/state/agentic-rigg/historikk.bin` | tjenesten | 4.4 |
| `~/.local/state/agentic-rigg/minmaks.json` | tjenesten | 4.4 |

- Validering og migrering ligger i `rigg-core/src/settings.rs`, som en port av `mergeSettings` og `lenientSettings` i `server/store.js`: samme nøkler, grenser og meldinger. Ukjente nøkler ignoreres ved innlesing, ugyldige felt får standardverdi hver for seg (ett dårlig felt nullstiller ikke resten), og en ødelagt fil kopieres til `ui.json.ødelagt-<tid>`.
- **Eksport** lager demoens format (`{"settings": {…}}`) fra `ui.json` og `tjeneste.json` sammen. **Import** tar demoens eksport og deler den på de to filene.
- Skrives atomisk (temp-fil og `rename`), samlet i 500 ms etter siste endring, så lukking aldri venter på disk.
- **Hvorfor ikke GSettings:** schemaet må kompileres og installeres, import/eksport ville vært et eget format, tjenesten og testene måtte ha schemaet tilgjengelig, og vi får ikke validering med meldinger. JSON gir samme filformat som demoen og én valideringskode for begge prosessene.

---

## 8. Sikkerhet: ingen skriving

- Ingen kode i `rigg-hw`, `riggd` eller `rigg` kaller NVML-funksjoner som setter noe. `app/clippy.toml` lister alle `nvml_wrapper::Device::set_*`, `reset_*`, `clear_*` og `nvml_wrapper::Nvml::` sine skrivende metoder under `disallowed-methods`, og CI-sjekken (`cargo clippy -- -D warnings`) feiler hvis de brukes.
- En test (`rigg-hw/tests/bare_lesing.rs`) søker gjennom kildekoden etter `nvmlDeviceSet`, `set_power`, `set_fan`, `/dev/i2c`, `nvidia-smi`, `Command::new` og `sudo`, som `test/nvidia-safety.test.js` gjør for demoen. Unntak er bare den simulerte backenden.
- Den simulerte backenden har skrivestien (txn, frist, klemming, nødregel), så bekreftelsesflyten kan bygges og testes. Den kan ikke velges når NVML-backenden er aktiv, og `--backend sim` er skjult i hjelpeteksten.
- Appen starter ingen programmer. Unntaket er reserven når systemd mangler (tjenesten startes som barneprosess), og `gio::AppInfo::launch_default_for_uri` for «Vis i Filer».
- `/dev/i2c-*` åpnes aldri. Revisjonen fant at numrene har flyttet seg: USB-C-kontrolleren (`ccgx-ucsi`, 0x08) ligger nå på `i2c-10`, og `i2c-8` er hovedkortets SMBus. Den senere RGB-koden skal derfor finne bussen ut fra navn og sysfs-sti under GPU-ens PCI-adresse («NVIDIA i2c adapter 1 at 1c:00.0»), og svartelisten skal gjelde «NVIDIA GPU I2C adapter»/`ccgx-ucsi` uansett nummer. `REGLER.md` er oppdatert av hovedøkta.

Senere, med brukerens godkjenning: `agentic-rigg-helper`, en system-tjeneste med root (`/run/agentic-rigg/helper.sock`, `0660 root:agentic-rigg`, polkit-handlingene i `docs/daemon-api.md` kap. 9). `riggd` er dens eneste klient og videresender `apply`, `confirm` og `revert`. Txn med frist, nødregelen og viftekurven kjører i hjelperen, så de overlever at `riggd` dør.

---

## 9. Arbeidsområdet `app/`

```
app/
├── Cargo.toml              arbeidsområdet: medlemmer, felles avhengigheter, release-profil
├── version.json            {"name":"Agentic Rigg","version":"1.0.0","channel":"stable","build":16}
├── clippy.toml             disallowed-methods (kapittel 8)
├── install.sh, uninstall.sh
├── scripts/
│   ├── maal.sh             oppstartstid, minne og CPU (kapittel 10)
│   └── bilder.sh           skjermbilder i headless gnome-shell
├── data/
│   ├── icons/              appikonet og variantene (ikonagenten)
│   ├── theme/              tokens.css, base.css, skall.css, innstillinger.css, én fil per side
│   ├── rigg.gresource.xml
│   ├── io.github.AgenticRigg.desktop.in
│   ├── io.github.AgenticRigg.metainfo.xml   (BØR)
│   └── systemd/agentic-riggd.service
└── crates/
    ├── rigg-core/          ren Rust, ingen GTK, ingen NVML
    ├── rigg-hw/            NVML, /proc, /sys, simulert backend
    ├── riggd/              binæren agentic-riggd
    └── rigg/               binæren agentic-rigg (GTK4)
```

| Crate | Avhenger av | Innhold |
|---|---|---|
| `rigg-core` | `serde`, `serde_json` | Datamodell (`Sample`, `SysSample`, `SensorKey`, enheter), protokolltypene (`proto.rs`), historikk med lagene (`history.rs`), min/maks/snitt (`stats.rs`), innstillinger med validering og migrering (`settings.rs`), formatering (port av `fmt.js`: tall, U+202F, °C/°F, GiB/GB, MB/s), sensornavn (`sensormeta.js`), spesifikasjonstabellen (`specs.js`), rapport som tekst/Markdown, CSV-skriver, nedsampling for grafer, txn-logikk (bekreft eller rull tilbake) for den simulerte backenden, tekst og logikk for påskeeggene, fargetokens for kontrasttesten |
| `rigg-hw` | `rigg-core`, `nvml-wrapper`, `libc` | `GpuBackend`-trait med `Nvml` (bare lesing) og `Sim` (scenarier for tester: normal, N/A, to kort, kort borte, driver oppdatert). Lesing av CPU, cache, flagg, minne, nett, hwmon, DMI, PCI og OS fra `/proc`/`/sys` med rot-sti som kan byttes (for fixtures) |
| `riggd` | `rigg-core`, `rigg-hw`, `glib`, `gio`, `async-channel` | Socket og økter, prøvetråd og takt, historikk og lagring, statistikk, logg, alarmer og varsler, tilstander og dvale, systemd-varsling, senere systemfeltet |
| `rigg` | `rigg-core`, `gtk4` (0.11, `v4_22`), `glow`, `glam`, `async-channel` | Appen, vinduet, skallet, sidene, widgetene, temaene, Innstillinger, klienten, 3D og påskeeggene. **Ikke** `rigg-hw` (vinduet laster aldri NVML) |

`gtk4` 0.11, `glib`/`gio` 0.22 og `async-channel` 2.5 finnes alt i cargo-bufferen fra Klipp. `nvml-wrapper` må hentes (nyeste 0.11.x). Mangler den en lesefunksjon (f.eks. `nvmlDeviceGetClockOffsets`), brukes `nvml-wrapper-sys` direkte i `rigg-hw/src/gpu/nvml_ffi.rs`.

Release-profil som Klipp (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`), men **uten** `panic = "abort"`. Tjenesten må kunne fange panikk i prøvetråden (4.6), og `panic` kan ikke settes per pakke. Forskjellen i størrelse er liten. `install.sh` bygger ikke, men forventer begge binærene i samme mappe.

---

## 10. Måling

`app/scripts/maal.sh` (grunnmuragenten skriver det, optimaliseringsrunden bruker det):

- **Første bilde:** med `RIGG_TIMING=1` skriver vinduet `RIGG_TIMING start=… first_frame=…` til stderr, målt fra `main()` (og fra `exec` via `/proc/self/stat` starttid) til første `after-paint` på frame clock. Ti kjøringer, median og verste.
- **Minne:** `Rss`, `Pss` og `Private_Dirty` fra `/proc/<pid>/smaps_rollup` etter 10 s i ro og etter at alle sidene er besøkt.
- **CPU:** `utime + stime` fra `/proc/<pid>/stat` over 60 s, for vinduet og tjenesten hver for seg, i fire tilstander: tjenesten alene, vinduet på Oversikt, vinduet på Grafer ved 1 min, vinduet minimert.
- **Bare egne prosesser.** Skriptet starter dev-forekomster i en privat økt (`RIGG_PROFIL=dev`, egne mapper) og måler bare PID-er det har startet selv.

Tallene skrives i `docs/produksjon/maalinger.md` med dato og bygg.

---

## 11. Tester

| Nivå | Hva | Hvor |
|---|---|---|
| Enhet | Historikk (bøtter, nedsampling, hull, grenser, lagring, ødelagt fil: samme tilfeller som `test/history.test.js`), statistikk, innstillinger (validering, migrering, import av demoens eksport), formatering, CSV (begge formater, tomme felt), rapport, spesifikasjonsoppslag, txn-logikken, kontrast (96 kombinasjoner) | `rigg-core` (`cargo test -p rigg-core`, under 1 s) |
| Maskinvare mot fixtures | `/proc` og `/sys` fra denne maskinen kopiert til `rigg-hw/tests/fixtures/` (serienumre fjernet), med rot-sti. NVML-feil og tilstander mot `Sim` | `rigg-hw` |
| Sikkerhet | Kildesøket i kapittel 8, og at `ops` i `hello` ikke har skriveoperasjoner med `backend: "nvml"` | `rigg-hw/tests`, `riggd/tests` |
| Kontrakt | `agentic-riggd --backend sim --socket <tmp>` som barneprosess: hello, alle leseoperasjoner, feilkoder, `write_locked`, grenser for melding og klienter, `SO_PEERCRED`, socketrettigheter, abonnement og takt, frakobling, grace-stopp, SIGTERM med lagring, historikk som overlever omstart, logg til fil | `riggd/tests/kontrakt.rs` |
| Ekte maskinvare (bare lesing) | `#[ignore]`-tester som leser NVML på denne maskinen og sammenligner med `nvidia-smi --query-gpu` (spørreflagg) innenfor toleranse | `rigg-hw/tests/ekte.rs`, kjøres med `cargo test -- --ignored` |
| GTK-enhet | Widgeter uten skjerm: grafens nedsampling og akser, formatering i rader, søket i Innstillinger. Krever `gtk::init()`; kjøres med `GDK_BACKEND=broadway` mot en egen `gtk4-broadwayd` (virker på maskinen [Målt]) | `rigg/tests` |
| Skjermbilder | `agentic-rigg --skjermbilder <mappe>` (bare i debug-bygg) besøker hver side i hvert tema og lagrer PNG med `gtk::WidgetPaintable` og `gsk::Renderer::render_texture`, mot `agentic-riggd --backend sim` med syntetiske 48 t. `scripts/bilder.sh` kjører det i en privat `gnome-shell --headless --wayland --virtual-monitor 1920x1080` med egen `HOME` og egne XDG-mapper, så brukerens skrivebord aldri røres. Bildene sammenlignes for hånd, og røyktesten sjekker at ingen side har tomme flater (0 × 0) og at ingen advarsler kommer fra GTK | `scripts/bilder.sh`, `docs/skjermbilder/1.0.0/` |

`cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace` før hver runde sier seg ferdig (REGLER).

---

## 12. Installasjon (plan for `install.sh` og `uninstall.sh`)

Skrives i runde 1, etter `../klipp/install.sh`. Skriptene bygger ingenting, bruker ikke sudo og aktiverer ingenting ved innlogging.

**`install.sh [--bin-dir STI]`** (standard `${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}/release`):

1. Sjekk at `agentic-rigg` og `agentic-riggd` finnes og kan kjøres. Ellers: «Bygg først med …».
2. `install -Dm755` begge til `~/.local/bin/`.
3. Ikoner: `hicolor/{scalable,16x16,24x24,32x32}/apps/io.github.AgenticRigg.svg`, symbolsk ikon, og hver variant fra `data/icons/varianter/varianter.json` som `io.github.AgenticRigg-<variant>` i de samme størrelsene. Variantvalget i Utseende skriver om `Icon=` i `.desktop`-fila og setter ikonet på vinduet.
4. `~/.local/share/applications/io.github.AgenticRigg.desktop` med `Exec=` sitert som i Klipp, `Icon=io.github.AgenticRigg` (eller valgt variant, som beholdes ved ny installasjon), `Categories=System;Monitor;`, `Keywords=gpu;skjermkort;sensor;temperatur;vifte;nvidia;overvåking;`, `StartupWMClass=io.github.AgenticRigg`, `StartupNotify=true`, og handlingen «Sensorer» (`--side sensorer`).
5. `~/.config/systemd/user/agentic-riggd.service` (kapittel 4.7), så `systemctl --user daemon-reload`. Kjører tjenesten fra før (ny versjon), `systemctl --user try-restart agentic-riggd.service`. Ingen `enable`.
6. `gtk-update-icon-cache` og `update-desktop-database` som i Klipp.
7. Skriv ut hva som ble lagt hvor, og om `~/.local/bin` mangler i `PATH`.

**`uninstall.sh`:**

1. `systemctl --user stop agentic-riggd.service` og `disable` (bare vår enhet), fjern enhetsfila, `daemon-reload`.
2. Fjern binærene, `.desktop`-fila og alle ikonene våre (også variantene), og rydd ikonbufferen som Klipp gjør.
3. Rør ikke `~/.config/agentic-rigg/`, `~/.local/state/agentic-rigg/` eller loggene. Skriv ut stiene og hvordan de slettes for hånd.

---

## 13. Versjon

- `app/version.json` er eneste kilde for navn, versjon, kanal og bygg. Demoens `version.json` på rota røres ikke (demoen viser sin egen versjon).
- `build.rs` i `rigg` og `riggd` leser fila og setter `RIGG_NAME`, `RIGG_VERSION`, `RIGG_BUILD`. En test sjekker at `[workspace.package] version` i `Cargo.toml` er lik `version` i fila.
- Om viser «Versjon 1.0.0 · bygg 16». Kanalen «stable» vises ikke. `agentic-rigg --version` og `agentic-riggd --version` skriver «Agentic Rigg 1.0.0 (bygg 16)».
- Bygget øker med én for hver runde som leverer noe brukeren kan installere (17, 18 …). Versjonen følger semver: 1.0.x for rettelser, 1.1 når første skrivetrinn blir godkjent.
- `hello` har `build`, så vinduet kan si fra om tjenesten er fra et annet bygg (produktspesifikasjonen 3.8).
