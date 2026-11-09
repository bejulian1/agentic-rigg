# Agentic Rigg 1.3 · produktspesifikasjon

*Produkt- og arkitekturagenten, 7. november 2026. Gjelder den ekte desktop-appen i Rust og GTK4, første versjon 1.0.0 · bygg 16. Oppdatert 9. november 2026 for 1.1.0 · bygg 17 (runde 6D): ti sider, nye navn på Styring, maskinhodet i sidefeltet og de redesignede styringssidene. Oppdatert igjen samme dag for 1.2.0 · bygg 18 (runde 7C): bryteren for endring og root-hjelperen i Innstillinger › Styring (6.7), og viftemotoren på Vifter (6.5). Oppdatert for 1.3.0 · bygg 19 (runde 8A): Oversikt over hele maskinen (3.11), akser som aldri kutter (3.12), Disker uten tomrom, og Skjermkortstyring med «Styrer»-linja (6.8).*

Dette dokumentet sier **hva** appen skal gjøre og **hvordan den skal oppføre seg**. `arkitektur.md` sier hvordan den bygges, og `byggeplan.md` sier hvem som bygger hva. Demoen (`web/`, `server/`, `daemon-mock/`) er fortsatt fasiten for design, ordlyd og oppførsel der dette dokumentet ikke sier noe annet.

**Merking**

- **MÅ**: med i 1.0.0.
- **BØR**: med i 1.0.0 hvis byggerundene rekker det, ellers i 1.0.x. Ingenting i BØR skal blokkere 1.0.0.
- **SENERE**: ikke i 1.0. Står her så arkitekturen ikke stenger for det.
- **[Kilde]** står i dokumentasjon. **[Kjent]** er godt kjent oppførsel i verktøyet, men ikke slått opp på nytt i dag. **[Målt]** er målt på denne maskinen 7. november 2026 (tallene står i `arkitektur.md`, kapittel 2).

---

## 1. Kortversjon

1. **To prosesser.** En liten tjeneste, `agentic-riggd` (systemd `--user`), leser kortet og maskinen, holder historikken i 48 t og skriver logger. Vinduet, `agentic-rigg`, kobler seg til og tegner. **Lukk betyr lukk**: vinduet avslutter prosessen sin og frigjør minnet. Tjenesten fortsetter bare hvis du har slått på opptak i bakgrunnen eller logger til fil.
2. **Ingen simulering i brukerflaten.** Alt som står i appen, er lest fra maskinen. Ordet «simulert» forsvinner fra appen. Den simulerte backenden finnes bare for tester og skjermbilder.
3. **Klokker og effekt, Vifter og RGB-lys (Styring) er låst.** De viser ekte verdier fra kortet: effektgrense med grenser, klokke-offset, temperaturmål, viftemodus og viftefart. Endringer er låst med en rolig forklaring til du slår på endring i Innstillinger › Styring og godkjenner ett trinn om gangen (kapittel 6). I 1.2.0 finnes bryteren og hjelperen, men det ekte kortet er fortsatt sperret (6.7).
4. **Det et ekte verktøy trenger:** logging til CSV, eksport av historikk, min/maks/snitt som lever i tjenesten, rapport som tekst eller Markdown, flere skjermkort, og ærlige tilstander når driveren er oppdatert eller kortet forsvinner.
5. **Lett og rask.** Vinduet skal vise første bilde på under 250 ms (mål 150 ms), og være borte på under 50 ms når du lukker det. Tjenesten skal bruke rundt 1 % av én kjerne i bakgrunnen, og aldri spørre kortet oftere enn 2 ganger i sekundet (ellers våkner det fra P8, se `maskinvare-revisjon.md`). Målene står i kapittel 9.

---

## 2. Hva ekte verktøy gjør

Tabellen er grunnlaget for valgene i kapittel 3. Den viser hva verktøyene gjør, ikke hva vi skal kopiere.

| Område | MSI Afterburner | HWiNFO | GPU-Z | CPU-Z | LACT | GreenWithEnvy | CoreCtrl | Mission Center |
|---|---|---|---|---|---|---|---|---|
| **Prosesser** | Én app, RivaTuner (RTSS) for overlegg [Kjent] | Én app med egen sensormodus [Kjent] | Én app [Kjent] | Én app [Kjent] | Root-tjeneste `lactd` i systemd og GTK-vindu som kobler til over Unix-socket, også uten grafisk økt [Kilde: LACT README] | Én Python/GTK-app [Kjent] | GUI og D-Bus-hjelper med root og polkit [Kilde: research kap. 3] | Vindu og egen innsamlingsprosess, `missioncenter-gatherer` [Kilde: pakkeliste i nixpkgs] |
| **Start og lukking** | Start med Windows, start minimert, lukk til systemfeltet, legg på overklokking ved oppstart [Kjent] | Autostart, start minimert, bare sensorer, minimer i stedet for å lukke [Kilde: HWiNFO-forum/endringslogg] | Minimer til systemfeltet, fortsett å oppdatere i bakgrunnen [Kjent] | Lukk = avslutt [Kjent] | Vinduet lukkes, tjenesten fortsetter og legger på innstillingene ved oppstart og etter dvale [Kilde] | Start ved innlogging, start skjult [Kjent] | Start minimert i systemfeltet, lukk til systemfeltet [Kjent] | Lukk = avslutt, ingen bakgrunn [Kjent] |
| **Logging** | Overvåkingshistorikk til fil (`.hml`) [Kjent] | CSV med valgt intervall og utvalgte sensorer, start/stopp-knapp [Kilde: guider, uenige om menyene] | «Log to file» på Sensors-fanen [Kjent] | Ingen | Historikkgrafer for effekt, temperatur og klokke, eksport som CSV [Kilde] | Ingen | Ingen | Ingen |
| **Profiler** | 5 plasser, hurtigtast per profil, 2D/3D-profil [Kjent] | Ingen (bare visning) | Ingen | Ingen | Profiler som byttes automatisk etter prosess eller gamemode, også fra CLI [Kilde] | Profiler for vifte og OC, legg på ved oppstart [Kjent] | Global profil og profil per program som slår inn når programmet kjører [Kjent] | Ingen |
| **Hurtigtaster og OSD** | Globale taster for profiler, OSD og opptak via RTSS [Kjent] | Ingen OSD selv, men tallene kan vises i systemfeltet, én verdi per ikon [Kilde: HWiNFO-forum] | Ingen | Ingen | Ingen | Ingen | Ingen | Ingen |
| **Alarmer** | Alarm per graf med terskel [Kjent] | Alarm per sensor: pip, logg, kjør program, ballongvarsel [Kilde: guide] | Ingen | Ingen | Ingen | Ingen | Ingen | Ingen |
| **Min/maks/snitt** | Min/maks i grafene [Kjent] | Nå, Min, Maks, Snitt og nullstilling (klokkeknappen) [Kjent] | Klikk på verdien for å bytte mellom nå, min, maks og snitt [Kjent] | Ingen | Ingen | Ingen | Ingen | Ingen |
| **Rapport og «validering»** | Ingen | Rapport som tekst, HTML og CSV [Kjent] | Validering på nett (TechPowerUp), Lookup, lagre VBIOS [Kjent] | Lagre rapport som TXT/HTML, validering på nett [Kjent] | Feilsøkingsarkiv med sysfs-data fra menyen [Kilde] | Ingen | Ingen | Ingen |
| **Flere skjermkort** | Kortvelger, «synkroniser like kort» [Kjent] | Alle i samme tre [Kjent] | Nedtrekksliste nederst [Kjent] | Graphics-fanen velger kort [Kjent] | Kortvelger, `--gpu-id`, kort kan kobles fra og til [Kilde] | Ett kort [Kjent] | Flere [Kjent] | Alle i lista [Kjent] |
| **Driveroppdatering, kort borte** | Viser ingenting til omstart [Kjent] | Sensoren blir grå [Kjent] | Feilmelding ved oppstart [Kjent] | Uendret | Logger i journalen, ellers lite hjelp [Kilde: README peker på `journalctl` og `dmesg`] | Krasjer eller viser N/A [Kjent] | Uendret | Kortet forsvinner fra lista [Kjent] |

Det vi tar med oss:

- **Tjeneste og vindu hver for seg** er normalen på Linux (LACT, CoreCtrl, Mission Center). Det gjør at vinduet kan lukkes uten at historikk og logging stopper.
- **HWiNFO-funksjonene** (CSV-logging, min/maks/snitt med nullstilling og alarm per sensor) er det som skiller et verktøy fra en demo. Demoen har allerede tabellen og tersklene.
- **Afterburner-funksjonene** (profiler, hurtigtaster, OSD og «legg på ved oppstart») henger sammen med skriving til kortet. De venter til skriving er godkjent.
- **Validering på nett** (GPU-Z, CPU-Z) passer ikke. Appen skal ikke sende noe ut av maskinen. Rapport til fil og utklippstavle dekker behovet.

---

## 3. Krav

### 3.1 Start, stopp, bakgrunn, minimer og lukk

| Krav | Merke |
|---|---|
| Appen ligger i «Vis programmer» med eget ikon, navn «Agentic Rigg», og søkeordene gpu, skjermkort, sensor, temperatur, vifte, nvidia. Varianten av ikonet velges i Innstillinger › Utseende | MÅ |
| **Én forekomst.** Starter du appen når vinduet er åpent, kommer det samme vinduet fram (GtkApplication) | MÅ |
| **Kald start:** ui-innstillingene leses før første bilde, så tema, aksent og tetthet aldri blinker (som `theme-boot.js`). Første bilde viser skallet og startsiden med «–» der tallene kommer. Tallene kommer innen én prøve, og historikken fylles inn etterpå. Ingen spinnere. «Kobler til tjenesten …» vises bare hvis det tar mer enn 300 ms | MÅ |
| Vinduet starter tjenesten selv hvis den ikke kjører, via systemd `--user` (reserve: start den som egen prosess). Brukeren skal aldri måtte starte noe for hånd | MÅ |
| **Lukk** (×, Ctrl+W, Ctrl+Q): vinduet forsvinner straks, og prosessen avslutter. Det finnes ikke noe «lukk til bakgrunnen» i vinduet. Det som skal fortsette, gjør det i tjenesten | MÅ |
| **Minimer:** vanlig GNOME-minimering. Når vinduet ikke synes (minimert eller dekket), stopper all tegning og animasjon, og vinduet ber tjenesten om sakte prøver. CPU i vinduet skal være nær 0 | MÅ |
| **Tjenesten når vinduet er lukket:** med «Fortsett i bakgrunnen» av stopper den 60 s etter at siste vindu ble lukket (så en rask gjenåpning ikke mister noe), og historikken er lagret på disk. Med valget på fortsetter den å ta opp historikk, min/maks og alarmer til du logger ut, med «Takt i bakgrunnen» (1, 2 eller 5 s, standard 2 s). Logging til fil holder alltid tjenesten i live til loggingen stoppes | MÅ |
| **Standard for «Fortsett i bakgrunnen»:** av (brukerens valg 7. november 2026, kapittel 10). Valget står øverst i Innstillinger › Generelt › Bakgrunn og sier med ord hva av og på betyr: av = ingenting kjører når appen er lukket, og grafene får et hull for den tiden; på = historikken fortsetter for ca. 21–28 MB og 0,85 % av én kjerne med 2 s-takt, og ikonet vises i systemfeltet. Når det er av, sier Grafer og Oversikt «Hull i grafen: appen var lukket» i verktøytipset på hullet | MÅ |
| **Start tjenesten ved innlogging** (Innstillinger › Generelt › Bakgrunn): `systemctl --user enable`. Av som standard (brukerens valg, kapittel 10). Agentene slår det aldri på selv | BØR |
| **Stopp tjenesten nå** (samme sted): stopper opptak og logging, og sier hva som stoppes («Stopper loggen til fil og opptaket i bakgrunnen …»). Angre i 5 s. Tjenesten avslutter straks siste vindu er lukket (ikke etter 60 s), og vinduet starter den igjen neste gang det åpnes | MÅ |
| **Systemfelt** (StatusNotifierItem) eid av tjenesten, bare mens den går i bakgrunnen (bakgrunnsopptak på eller en logg går, og ingen vinduer åpne). Verktøytips med GPU-temperatur, effekt og last («GPU 52 °C · 12 W · 8 % last»). Meny: «Åpne Agentic Rigg», «Start logging» eller «Stopp logging» (alltid synlig), «Stopp bakgrunnsopptak» (slår av «Fortsett i bakgrunnen» og stopper tjenesten hvis ingen logg går). Ubuntu har AppIndicator-utvidelsen på som standard (den er på her [Målt]). I ren GNOME vises det ikke, og appen virker likt uten | BØR |
| Tall i selve panelet (f.eks. «52°» ved ikonet), slik HWiNFO gjør i systemfeltet | SENERE |
| Vindusstørrelse og om sidefeltet er sammenlagt, huskes. Posisjon kan ikke huskes på Wayland og prøves ikke | MÅ |
| Minste vindusstørrelse 640 × 480. Bruddpunktene fra demoen beholdes: ikonlist under 860 px, Innstillinger med ikoner øverst under 700 px | MÅ |

### 3.2 Logging og eksport

| Krav | Merke |
|---|---|
| **Logg til fil (CSV)**, kjørt av tjenesten, så den fortsetter når vinduet lukkes. Start og stopp med en knapp i toppfeltet på Sensorer (ikon `circle-dot`, «Logg til fil», Ctrl+L) og i Innstillinger › Logging og varsler | MÅ |
| Valg: alle sensorer eller utvalgte (høyreklikk på en rad på Sensorer › «Ta med i loggen», eller en liste i Innstillinger), intervall 0,5 / 1 / 2 / 5 / 10 s (standard 1 s), mappe (standard `~/Dokumenter/Agentic Rigg/`), filnavn `rigg-2026-11-07_14-32-10.csv` | MÅ |
| Format: UTF-8, én overskriftsrad «Navn [enhet]», første kolonner `Tid` (ISO 8601 med sone) og `Sekunder` (fra start). Tomt felt når en verdi mangler. To varianter: **Standard CSV** (komma, punktum som desimaltegn, standard) og **Norsk regneark** (semikolon, komma som desimaltegn) | MÅ |
| Kolonnene ligger fast mens loggen går. Forsvinner en sensor, står feltet tomt. Ny sensor tas med i neste logg | MÅ |
| Tjenesten skriver med buffer og tømmer bufferen hvert 5. s og ved stopp. Ny fil (`…-del2.csv`) ved 100 MB. Stopper med en melding hvis disken har under 500 MB ledig | MÅ |
| Mens loggen går: en liten prikk og tiden ved appnavnet øverst i sidefeltet, og i systemfeltet. Verktøytipset har filnavn, antall rader og størrelse. «Vis i Filer» åpner mappa | MÅ |
| **Eksporter historikk** på Grafer: «Eksporter CSV …» lagrer tidsvinduet og seriene du ser, med `min`, `snitt` og `maks` per serie, fra historikken i tjenesten | MÅ |
| Global hurtigtast for å starte og stoppe loggen (portalen GlobalShortcuts) | SENERE |
| Logge til journalen eller OpenTelemetry (LACT har det) | SENERE |

### 3.3 Profiler og profiler per program

Profiler endrer kortet, og skriving er ikke godkjent. Derfor:

| Krav | Merke |
|---|---|
| De fem profilplassene fra demoen beholdes som **lagrede oppsett**. «Lagre det kortet bruker nå» lagrer de leste verdiene (effektgrense, offset, temperaturmål og viftemodus) med navn og tid | BØR |
| «Hent» legger profilen i utkastet (kapittel 6.3), ikke på kortet | BØR |
| Legge på en profil, hurtigtast per profil, legge på ved oppstart | SENERE, krever skriving |
| Profil per program (som CoreCtrl og LACT: når `steam_app_…` eller en valgt prosess kjører, eller GameMode er aktiv). Tjenesten ser på `/proc` hvert 2. s. Krever godkjent skriving | SENERE |

### 3.4 Hurtigtaster og OSD

| Krav | Merke |
|---|---|
| Alle tastene fra demoen i vinduet: 1–9 og 0 for de ti sidene (1.1.0), Ctrl+B, Ctrl+, , Ctrl+F og /, Ctrl+?/?, Esc, mellomrom for pause på Grafer, Alt+↑/↓ i lister, piltaster i tabeller og kurven. På Klokker og effekt og Vifter: Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y) angrer og gjør om i utkastet, Ctrl+Enter er Bruk (bare mot den simulerte backenden), og piltastene, PgUp/PgDn og Home/End flytter glidere og kurvepunkter. Alt står i dialogen Tastatursnarveier | MÅ |
| Ctrl+L logg av/på, Ctrl+R nullstill min/maks på Sensorer (med angre i 5 s), Ctrl+Q avslutt, Ctrl+W lukk vindu, F5 hent statiske data på nytt (Grafikkort og System) | MÅ |
| Ctrl + / Ctrl − / Ctrl 0 endrer skriftstørrelse i trinnene fra Innstillinger (erstatter nettleserens zoom) | BØR |
| Egen dialog for tastatursnarveier (som demoen). `GtkShortcutsWindow` er utfaset i GTK 4.18, så den lages selv | MÅ |
| Globale hurtigtaster (vis vindu, logg av/på) via portalen GlobalShortcuts. GNOME spør brukeren første gang | SENERE |
| **OSD i spill** via MangoHud: «Lag MangoHud-oppsett» skriver `~/.config/MangoHud/MangoHud.conf` med verdiene du har festet (research kap. 5). Et eget overlegg over andre vinduer går ikke på GNOME Wayland (ingen layer-shell, ingen «alltid øverst») | SENERE |

### 3.5 Alarmer, skrivebordsvarsler og min/maks/snitt

| Krav | Merke |
|---|---|
| **Min, maks og snitt lever i tjenesten**, så de dekker tiden vinduet var lukket når tjenesten går i bakgrunnen. Min og maks lagres mellom omstarter (som `sensorKeepMinMax` i demoen). Snitt gjelder siden siste nullstilling eller siden tjenesten startet, og raden sier hvilket | MÅ |
| Nullstill alle (↺ på Sensorer, Ctrl+R) med angre i 5 s. Nullstill én sensor fra høyreklikkmenyen | MÅ (alle), BØR (én) |
| «Min/maks siden 14:32 i dag» som i demoen | MÅ |
| Varselfargene (gul og rød) fra demoen beholdes for sidefeltet, Oversikt, Sensorer og stolpene | MÅ |
| **Skrivebordsvarsel** når en verdi har vært over den røde grensen i 10 s, sendt av tjenesten (virker også når vinduet er lukket). Tittel «GPU-temperaturen er 86 °C», tekst «Over den røde grensen på 85 °C siden 14:31.» Hysterese på 3 °C eller 3 prosentpoeng, høyst ett varsel per verdi per 15 min. Klikk på varselet åpner appen på Sensorer. Av som standard | BØR |
| Varsel når skjermkortet forsvinner eller driveren ikke kan leses (kapittel 3.8) | BØR |
| Egen alarm per sensor (terskel over/under, varighet), fra høyreklikkmenyen på Sensorer | SENERE |
| Lyd, kjør program eller slå av maskinen ved alarm (HWiNFO) | Latt være: passer ikke den rolige tonen |

### 3.6 Rapport og «validering»

| Krav | Merke |
|---|---|
| **Kopier som tekst / Markdown** på Grafikkort og System, som i demoen | MÅ |
| **Lagre rapport …** (Innstillinger › Om og knapp på Grafikkort og System): én fil, `.txt` eller `.md`, med app og versjon, OS, kjerne, økttype, driver og NVML-versjon, alle skjermkort (fra driveren og «Referanse, ikke målt» hver for seg), prosessor, hovedkort/BIOS, minne, alle sensorer med Nå/Min/Maks/Snitt og tidspunkt, og tilstanden til hvert kort | MÅ |
| Serienumre, UUID og vertsnavn er **ikke** med som standard. «Ta med serienumre og UUID» er et valg i lagringsdialogen | MÅ |
| Feilsøkingspakke (som LACT): rapporten, utdrag fra tjenestens logg og hvilke sysfs-filer som ble lest, i ett arkiv | SENERE |
| Validering på nett, Lookup og opplasting | Latt være: appen sender ingenting ut av maskinen |
| Lagre VBIOS (GPU-Z): krever root på Linux | SENERE, krever godkjenning |
| Minnemoduler fra SPD (CPU-Z «SPD»): `ee1004` har bundet SPD-EEPROM-ene, og filene kan leses uten root, men lesingen er I2C-trafikk på hovedkortets SMBus (`maskinvare-revisjon.md` kap. 10). Faktisk minnehastighet og timinger krever root | SENERE, krever godkjenning |

### 3.7 Flere skjermkort

| Krav | Merke |
|---|---|
| Datamodellen, protokollen, historikken og loggen bruker en stabil ID per kort (fra PCI-bussen), aldri bare indeksen. Sensornøkler som `gpu:0000:1c:00.0/temp` | MÅ |
| Kortvelgeren øverst i sidefeltet lister alle NVIDIA-kort. Sidene følger kortet som er valgt. Sensorer har én gruppe per kort. Med ett kort ser alt ut som i demoen | MÅ (modell), BØR (UI testet med to simulerte kort) |
| AMD og Intel som bare lesing via sysfs og hwmon (`amdgpu`, `i915`, `xe`) | SENERE (kan ikke testes på maskinen) |

### 3.8 Når driveren oppdateres, kortet forsvinner eller maskinen sover

Tjenesten skiller mellom disse tilstandene og sender hendelsen `device` når en av dem endres. Vinduet viser dem i kortvelgeren (prikken og verktøytipset), som en linje under toppfeltet på GPU-sidene, og på Grafikkort. CPU, minne, nett og sensorer går som før i alle tilstandene.

| Tilstand | Hvordan tjenesten ser det | Hva vinduet sier | Hva tjenesten gjør |
|---|---|---|---|
| `ok` | NVML svarer | Ingenting | Leser |
| `field_na` | Én verdi gir `NOT_SUPPORTED` eller `N/A` | «–» og forklaringen fra demoen («Kan ikke leses på dette kortet») | Spør ikke etter verdien igjen før neste oppstart (`NOT_SUPPORTED`), eller prøver igjen om 30 s (andre feil) |
| `driver_updated` | `nvmlInit` gir `LIB_RM_VERSION_MISMATCH`, eller `/sys/module/nvidia/version` ikke stemmer med det NVML melder | **Driveren er oppdatert.** «Ny NVIDIA-driver (610.51) er installert, men den gamle (610.43.02) kjører fortsatt. Start maskinen på nytt for å lese skjermkortet.» | Sjekker hvert 60. s. Ingen gjentatte feil i journalen |
| `no_driver` | `libnvidia-ml.so.1` mangler, eller `DRIVER_NOT_LOADED` | «Fant ingen NVIDIA-driver. Prosessor, minne og sensorer vises som vanlig.» GPU-sidene viser en tom tilstand, ikke et simulert kort. På Oversikt (1.3.0) sier Skjermkort-flisa «Ingen driver», tilstand-linja forklarer, VRAM og skjermkortdelen skjules, og grafene viser prosessor, minne og disker | Prøver igjen hvert 60. s |
| `lost` | `GPU_IS_LOST` eller kortet er borte fra PCI (Xid 79, eGPU koblet fra) | «Skjermkortet svarer ikke siden 14:32. Det skjer når kortet faller av bussen eller trekkes ut. Tallene før det er tatt vare på.» Festede verdier viser «–» | Prøver å finne kortet igjen hvert 10. s. Historikken får et hull |
| `suspended` | logind `PrepareForSleep` | Ingenting (vinduet er ikke synlig) | Stopper prøvene, lagrer historikken, starter igjen etter oppvåkning med 2 s pause og nye forsøk ved feil |
| Tjenesten borte | Socketen svarer ikke | Linja «Tjenesten svarer ikke» med knappen «Start på nytt» (samme rolige stil som demoens «Frakoblet») | – |
| Tjenesten er eldre | `hello` melder et annet bygg enn vinduet | «Tjenesten er fra bygg 16, vinduet fra bygg 17. Start tjenesten på nytt.» med knapp | – |

### 3.9 Sidene i produksjon

Ti sider i 1.1.0 (ni i 1.0.0), i denne rekkefølgen og med tastene 1–9 og 0. `id` er uendret fra 1.0.0 (`kontroller`, `viftekurve`, `lys`), så lenker, innstillinger og `--side` virker som før.

| Gruppe | Side (`id`) | Hva den viser |
|---|---|---|
| Overvåking | **Oversikt** (`oversikt`) | Hele maskinen (1.3.0, se 3.11): fliser, tilstand-linja, seks grafer og skjermkortets målere. Tidsvinduet 1 min–48 t henter fra tjenesten |
| | **Sensorer** (`sensorer`) | Som demoen. Min/maks/snitt fra tjenesten. «Logg til fil» i toppfeltet, «Ta med i loggen» og «Nullstill» i høyreklikkmenyen. Gruppa Disker (1.1.0): temperatur, lesing, skriving og aktivitet per disk |
| | **Grafer** (`grafer`) | Som demoen. «Eksporter CSV …». Hull når appen var lukket, med forklaring. «Disktemperatur» og «Disk-I/O» (1.1.0) |
| Maskinvare | **Grafikkort** (`grafikkort`) | Alt fra NVML og sysfs i stedet for `nvidia-smi`. Spesifikasjonstabellen er fortsatt «Referanse, ikke målt». «Lagre rapport …», to vifter med fart og turtall hver for seg, VRAM med `MemoryInfo_v2`, og effekt som snitt fra energitelleren. Det kortet ikke støtter, står som «Ikke støttet av kortet og driveren» |
| | **Prosessor** (`prosessor`, ny i 1.1.0) | CPU-Z-delen som lå på System: Nå (Tdie/Tctl, last, klokker, governor og boost), klokke og last per tråd gruppert etter CCX og kjerne, Prosessor, Cache, Instruksjonssett og Sårbarheter |
| | **Disker** (`disker`, ny i 1.1.0) | Som CrystalDiskInfo, uten root: UDisks2 over D-Bus (modell, serie, fastvare, o/min, SMART-helse, temperatur, driftstid, hele SMART-tabellen, selvtest) og `/proc/diskstats` (MB/s, aktivitet), ledig plass per partisjon. Aldri `SmartUpdate` eller selvtest, aldri `/dev/sd*` |
| | **System** (`system`) | Hovedkort/BIOS, minne, operativsystem og kontrollere (nett, lyd, USB, SATA, NVMe). CPU-temperaturen er Tdie (Tctl i verktøytipset), og «buffer og cache» skiller ut `Shmem`. «Lagre rapport …» |
| Skjermkortstyring (merket «Bare visning», kortet under tittelen) | **Klokker og effekt** (`kontroller`) | Effektgrense, klokke-offset, temperaturmål og vifte. Låst mot kortet (kapittel 6) |
| | **Vifter** (`viftekurve`) | Viftekurve og turtall. Låst. Viser driverens vifte slik den faktisk går (kapittel 6) |
| | **RGB-lys** (`lys`) | Effekter og farger på kortet som forhåndsvisning. Lyskontrolleren leses ikke (kapittel 6). Siden åpner med 2D-kortet; se 6.6 |

**Disker** (1.3.0): en utfoldet disk har Disk og SMART side om side med like høye kort (radene fordeler høyden i det laveste), og Partisjoner i hele bredden under. Under 720 px står de under hverandre. Ingen tomrom under Disk.

**Sidefeltet** (1.1.0) har et maskinhode øverst: skjermkortet («RTX 2080 / 8 GB · driver 610.43.02») og prosessoren («Ryzen 7 2700X / 8 kjerner · 16 tråder») i ett kort. Skjermkortet åpner detaljene og kortvalget, prosessoren går til Prosessor. Feil står i rødt eller gult i linja under navnet, ikke som en farget prikk. Vindusknappene følger GNOME (`gtk-decoration-layout`) og ser ut som i libadwaita/Yaru.

| **Innstillinger** | Som v0.0.15 (hele vinduet, egen navigasjon, søk). Endringene står i 3.10 |

### 3.10 Innstillinger

Navnene på valgene (nøklene) er de samme som i demoen, så en eksportert fil fra demoen kan importeres (MÅ).

| Del | Endring fra demoen |
|---|---|
| **Generelt** | «Datakilde (Live/Simulert)» og «Start med simulert GPU» fjernes. «Oppdateringsintervall» (0,5 / 1 / 2 s) blir, men **standard er 1 s** (demoen hadde 0,5 s). Forklaringen sier at 0,5 s koster dobbelt så mye CPU. Ny gruppe **Bakgrunn**: status for tjenesten («Kjører siden 09:12 · 31 MB»), «Fortsett i bakgrunnen», «Takt i bakgrunnen» (1 / 2 / 5 s, standard 2 s), «Start tjenesten ved innlogging» (BØR), «Stopp tjenesten nå». «Frist for å beholde endringer» skjules til skriving er godkjent |
| **Utseende** | Som før: System/Lys/Mørk, Grafitt/Sand/Fjord/Midnatt, høy kontrast, seks aksentfarger, tetthet, skrift, animasjoner. Nytt: **Appikon** (variantene i `app/data/icons/varianter/`). Nytt (BØR): aksent «System» som følger GNOME-aksenten, og høy kontrast «Automatisk» som følger GNOME. Demoen kunne ikke lese aksenten; GTK og portalen kan |
| **Enheter og format** | Uendret |
| **Sidefelt** | Uendret |
| **Grafer og sensorer** | Uendret, pluss «Husk min og maks mellom økter» som nå gjelder tjenesten |
| **Logging og varsler** (ny) | Loggens sensorer, intervall, mappe og format. Skrivebordsvarsler av/på (BØR) og hvilke verdier som varsler |
| **Data og tilbakestilling** | «Datamappe» viser `~/.config/agentic-rigg/` og `~/.local/state/agentic-rigg/` med «Vis i Filer». Eksport og import som før. Nytt: «Slett historikken» (med angre i 5 s, tjenesten sletter først etter fristen). Teksten «Profiler, lys og simulerte verdier blir» blir «Profiler og lysvalg blir» |
| **Styring** (ny i 1.2.0) | Bryteren «Tillat endring av skjermkortet», status, trinnlisten og «Hva bryteren gjør» (6.7) |
| **Om** | Ikon, navn, «Versjon 1.2.0 · bygg 18», én setning, og signaturen «Code Pro» nederst (1.1.0). Ingen alpha-merke. Feilsøking: tjenesten (versjon, bygg, PID, oppetid, minne), driver, NVML-versjon, CUDA-versjon, økttype, GTK-versjon og tegner (GL/Vulkan/programvare), socket og mappene. «Lagre rapport …». Lisenser |

---

### 3.11 Oversikt over hele maskinen (1.3.0)

Brukeren: «overvåking delen bør også ha mulighet til å se mer av systemet enn bare GPU … nesten som Resources-appen». Oversikt er startsiden og viser hele maskinen, i appens rolige stil, det viktigste øverst:

| Del | Innhold | Klikk |
|---|---|---|
| **Fliser** (seks, tre under 900 px, to under 560 px) | Prosessor: last, temperatur (Tdie), snittklokke. Skjermkort: last, temperatur, effekt av grensen, P-tilstand. Minne: brukt av totalt i GiB og %. VRAM: brukt av totalt. Disker: samlet MB/s, varmeste disk, SMART-helse («alle OK», «1 advarsel»). Nettverk: ned og opp. Hver med sparklinje for siste minutt | Prosessor, Grafikkort, System, Grafikkort, Disker, Sensorer (nett) |
| **Tilstand-linja** | Bare når noe er verdt å se, det alvorligste først, høyst tre og «+N»: kortet svarer ikke, ny driver, ingen driver, nedklokking (termisk eller maskinvare; effektgrensen under last er vanlig og står ikke), GPU- eller CPU-temperatur over gul/rød grense eller ved nedklokkingsgrensen, minnet over 90/95 %, disk med SMART-advarsel eller -feil | Siden for delen |
| **Grafer** (tre spalter fra ca. 1240 px innhold, to, én under 700 px) | Last (GPU og CPU), Temperaturer (GPU, CPU og de to varmeste diskene, med nedklokking og avstenging), GPU-effekt (trekk og grense), Minne og VRAM (%), Disk-I/O, Nettverk. Felles markør, tidsvalget i toppfeltet | – |
| **Skjermkortet** | Temperatur og effekt (målerne), Skjermkort (P-tilstand og begrensning, kjerne- og minneklokke mot maks, vifter, viftestyring, PCIe, video) med lenke til Grafikkort | Grafikkort |

Ingen nye data fra tjenesten: alt fantes i prøvene, historikken og `get_disks` (hentes hvert 30. s mens siden vises; tjenesten har lista). Topp-prosesser er ikke med: det krever en ny lesing av `/proc/*/stat` hvert sekund. Krav: vinduet i ro ≤ 2 %, kald start ≤ 250 ms (Oversikt er startsiden).

### 3.12 Akser som ikke kutter (1.3.0)

Hver graf har et **foretrukket område** (GPU- og CPU-temperatur 20–100 °C, disk 20–60 °C, effekt 0 til kortets maks, klokker 0 til kortets maks). Det er ikke en grense:

- **«Fast»** viser det foretrukne området, og **«Tilpass»** følger dataene innenfor det, som før.
- Går dataene **over eller under**, utvides aksen til en pen skala (1-2-2,5-5 × 10ⁿ, fem eller seks merker) med litt luft, i begge modusene. 110 °C, 15 °C, 330 W og en disk på 65 °C kuttes aldri.
- **Hard grense** bare der verdien ikke kan gå over: last og minne i prosent. Viftene har ingen (NVML kan melde over 100 %).
- **Aksen pumper ikke:** den utvides straks, men krymper først når den har ønsket seg mindre i 15 s.
- **Grenselinjer:** nedklokking og avstenging (fra NVML) som stiplede streker med navn når de er innenfor aksen; en grense like over kurven (15 % av spennet) tas med i aksen.
- Sparklinjene følger dataene; 0–100 % er foretrukket, ikke en grense.
- Kurveredigereren på Vifter (20–100 °C, 0–100 %) er et redigeringsområde og beholdes.

## 4. Demoens semantikk og hva den blir

| I demoen | I produksjon |
|---|---|
| Merket «Simulert» (status øverst, kortvelgeren, Grafikkort, Sensorer-gruppa «· simulert», Lys-merket) | Borte. Kortvelgeren viser kortets tilstand (kapittel 3.8). Lys-merket heter «Forhåndsvisning» |
| «Bruk simulert ved oppstart» / «Start med simulert GPU» | Fjernet fra brukerflaten. Den simulerte backenden velges bare med `agentic-riggd --backend sim` eller `RIGG_BACKEND=sim`, i tester og for skjermbilder |
| Datakilde Live/Simulert (segment i Generelt og kortvelgeren, `POST /api/mode`, `AG_SIMULATE`) | Fjernet. Kilden står som en linje i kortvelgeren og i Om › Feilsøking: «NVML · driver 610.43.02» |
| Automatisk overgang til simulert kort når `nvidia-smi` feiler | Fjernet. Feil gir en ærlig tilstand (kapittel 3.8), aldri falske tall |
| Bakgrunnstjenesten: «Tilkoblet (simulert)», «Startes sammen med appen (npm start)» | Gruppa Bakgrunn i Generelt: «Kjører siden 09:12 · 31 MB · opptak i bakgrunnen av». Linja under toppfeltet bare ved feil |
| «Behold endringene?» med frist (10–60 s) | Vises ikke, fordi ingenting skrives. Koden og valget `confirmSec` beholdes og testes mot den simulerte backenden, klare for første godkjente trinn |
| Forkast og Bruk i toppfeltet (Ctrl+Z / Ctrl+Enter) | Merket «Låst» i toppfeltet på Klokker og effekt og Vifter. Endringer blir et utkast i appen, med «Forkast utkast» og Ctrl+Z / Ctrl+Shift+Z; «Bruk» er låst mot det ekte kortet. På RGB-lys heter knappen «Lagre i appen» |
| Toastene «Tilbakestilt til standard (simulert)», «Lys lagret (simulert)», «Endringene er beholdt … (simulert)» | Borte, eller med ny tekst: «Lysvalget er lagret i appen. Lyskontrolleren er ikke koblet til.» |
| Kontroller-banneret «Alt her er simulert …» | Låst-banneret i kapittel 6.2 |
| Viftekurve: «Kurven kjøres av en simulert bakgrunnstjeneste …» | «Driveren styrer viften. Kurven under viser hvordan den har gått de siste 48 timene.» |
| Spennings- og klokkekurven («illustrasjon, ikke lest fra kortet») | Tas bort. Den kan ikke leses med NVML på Turing, og en illustrasjon hører ikke hjemme i et måleverktøy |
| Nav-seksjonen «Kontroller · simulert» og verktøytipsene «Kontroller (simulert)» | «Styring» med merket «Bare visning» (lås) ved seksjonen, og verktøytips som sier hva hver side styrer (1.1.0). Med bryteren på (1.2.0): «Endring på · låst» mot det ekte kortet, «Endring på · effekt» mot sim, «Endring: feil» eller «Endring: slå på igjen» |
| HTTP-serveren på 127.0.0.1:4318, SSE, Host/Origin-sjekk, `/api/*` | Borte. Ingen nettverksport i det hele tatt. Vinduet snakker med tjenesten over Unix-socket (`docs/daemon-api.md`, utvidet i `arkitektur.md` kap. 5) |
| `nvidia-smi`-prosesser | Borte. NVML leses direkte. Appen starter ingen andre programmer (bortsett fra `systemctl --user` via D-Bus) |
| `state.json`, `sensor-minmax.json`, `history.json` i `AG_DATA_DIR` | `~/.config/agentic-rigg/ui.json` og `tjeneste.json`, `~/.local/state/agentic-rigg/historikk.bin` og `minmaks.json` |
| `localStorage` (funne påskeegg, valgt del i Innstillinger, sidefeltet først lokalt) | `ui.json` |
| Flere faner, `pending_elsewhere`, `/api/tab/close` | Én forekomst. Lukket vindu er en klient som kobler fra, og tjenesten ruller tilbake det som ikke er beholdt (kontraktens regel 5), når skriving kommer |
| Simulatoren (`server/simulator.js`, `daemon-mock/card.js`) | Den simulerte backenden i `rigg-hw` (bare tester og skjermbilder) |
| «Referanse, ikke målt» | Beholdes. Det er ærlig |
| `prefers-reduced-motion`, `prefers-color-scheme` | GTKs `gtk-interface-reduced-motion`, `gtk-interface-color-scheme` og `gtk-interface-contrast` (finnes i GTK 4.22 [Målt]) |
| «alpha v0.0.15 · bygg 15» | «Versjon 1.2.0 · bygg 18», bare i Innstillinger › Om, `--version` og rapporten |
| Påskeeggene | Alle ti beholdes (kapittel 7) |

---

## 5. Tekst og ordlyd

- «Simulert» står aldri i appen. Det som ikke kan leses, sier hvorfor («Kan ikke leses på dette kortet», «Krever root»), som i demoen.
- Det som er lest fra kortet, merkes ikke. Det er normalen. Bare unntakene merkes: «Referanse, ikke målt», «Utkast · ikke sendt til kortet», «Forhåndsvisning».
- Låst-tekstene er rolige og konkrete: hva som er låst, hvorfor, og hva som skal til. Aldri gul eller rød for låsen, fordi den ikke er en feil.
- Feiltilstandene sier hva som skjedde, når, og hva du kan gjøre. Ingen feilkoder i hovedteksten, bare i verktøytipset og i Om › Feilsøking.
- Alt er norsk bokmål, med samme ordvalg som demoen (Bruk, Behold, Forkast, Tilbakestill, Nullstill, Fest).

---

## 6. Låst skriving: Klokker og effekt, Vifter og RGB-lys

### 6.1 Prinsipp

Sidene skal være nyttige **nå**, ikke ventesider. De viser alt kortet faktisk gjør og tillater, lest med NVML, og de forklarer kort hva som skal til for å endre det. Låsen gjelder i hele kjeden: vinduet har ingen aktive Bruk-knapper mot det ekte kortet, tjenesten skriver aldri selv (se `arkitektur.md` kap. 8), og root-hjelperen fra 1.2.0 (6.7) har skrivekoden for det ekte kortet sperret.

### 6.2 Felles låst-tilstand

**1.1.0 (runde 6C):** på Klokker og effekt og Vifter er banneret erstattet av **låst-linja**: én rolig linje som sier «Låst», «Utkast · låst», «Simulert kort» eller «Kortet svarer ikke», med hvorfor og «Hva skal til?». RGB-lys har fortsatt banneret. Verdiradene er glidere med tallfelt (6.3), ikke lesevisninger. Resten av dette avsnittet beskriver 1.0.0, og trinnene og «Hva skal til?» gjelder fortsatt.

**Banner øverst på siden** (samme flate som demoens infolinjer, nøytral farge, ikon `lock`):

> **Endringer er låst**
> Verdiene under er lest fra kortet akkurat nå. Agentic Rigg endrer ikke skjermkortet før du har slått på skriving, ett trinn om gangen.
> [Hva skal til?]

Banneret kan legges sammen til én linje («Låst · lest fra kortet»), og valget huskes (`dismissed`, som demoen). Det kan ikke fjernes helt mens siden er låst.

Når kortet ikke svarer (`lost`), sier banneret: «Kortet svarer ikke. Verdiene vises igjen når det er tilbake.» Utkastet kan fortsatt endres, og det begynner fra siste lesing.

**1.2.0 (runde 7C):** «Hva skal til?» viser bryteren og de fire trinnene fra Innstillinger › Styring (6.7), med samme navn og ord («Godkjent», «Ikke godkjent», «Kommer senere», «Ukjent») og en knapp til Innstillinger › Styring. Tabellen under er 1.0.0. Med bryteren på sier låst-linja «Endring er slått på, men denne versjonen endrer ikke det ekte kortet ennå. Det du endrer her, blir et utkast som lagres i appen.»

**«Hva skal til?»** (1.0.0) åpner en popover med trinnene, i den rekkefølgen VEIKART har valgt:

| Trinn | Status |
|---|---|
| 1. Effektgrense | Ikke godkjent |
| 2. Klokke-offset (kjerne og minne) | Ikke godkjent |
| 3. Vifte, manuell fart | Ikke godkjent |
| 4. Viftekurve | Ikke godkjent |
| 5. Temperaturmål | Ikke godkjent (ikke verifisert på GeForce) |
| 6. Lys (Palit-kontrolleren på I2C) | Ikke godkjent (kontrolleren er ikke bekreftet) |

Under trinnene: «Skriving krever en liten hjelper med root-tilgang, som spør om passordet ditt (polkit). Den er ikke installert. Hvert trinn slås på for seg, og alltid med Behold eller Rull tilbake.» Statusen kommer fra tjenesten (`capabilities.write`), så teksten endres av seg selv når et trinn senere blir godkjent.

**Toppfeltet** har merket «Låst» (ikon og tekst, dempet) der Forkast og Bruk sto. Verktøytipset: «Endringer er låst. Verdiene er lest fra kortet.» Ctrl+Z og Ctrl+Enter gjør ingenting.

**Verdirader** er lesevisninger, ikke deaktiverte glidere. En grå glider ser ødelagt ut. Hver rad har:

- etikett og verdi med enhet («Effektgrense 245 W»),
- et tynt område-spor med markør der verdien står, og merker for standard og grensene («105 W · standard 245 W · 275 W»),
- kilde i verktøytipset («Lest med NVML 14:32:10. Grensene kommer fra kortet.»),
- en hendelse når verdien endres utenfra (f.eks. av `nvidia-smi -pl` eller et annet verktøy): en kort aksentmarkering og «Endret utenfor appen 14:33» i verktøytipset.

### 6.3 Utkast (BØR)

**1.1.0:** det finnes ingen «Lag utkast»-knapp lenger. Gliderne, tallfeltene og kurven er alltid kontroller; utkastet lages først når en endring faktisk gjør det forskjellig fra kortet (`locked::edit_draft`), med merket «Utkast · ikke sendt til kortet» og aksentkant på endrede rader. Hver rad har «Tilbakestill» til standard, og panelet har «Tilbakestill alt til standard». Ctrl+Z / Ctrl+Shift+Z angrer og gjør om (høyst 100 steg). Utkastet lagres i appen og kan lagres som profil. «Bruk» står der, men er låst med verktøytipset «Bruk er låst: denne versjonen endrer ikke det ekte kortet» (1.2.0). «Forkast utkast» går tilbake til det kortet har. Det gjør profilene nyttige før skriving er godkjent, og det er der påskeegg 8 (kallenavn på kurven) lever.

### 6.4 Klokker og effekt (`kontroller`)

| Del | Lest fra kortet (NVML-get) |
|---|---|
| Effekt | Gjeldende grense og håndhevet grense, standard og område (105–275 W [Målt]), strømtrekk nå. Graf for siste minutt med effekt og grense, som demoen |
| Klokker | Kjerne- og minneoffset nå (`nvmlDeviceGetClockOffsets`, minne i effektiv MHz), kortets grenser (kjerne −1000…+1000, minne −1000…+3000 effektiv [Målt]), og tjenestens strengere policy for senere skriving. Klokke nå og maks per P-tilstand. Graf for kjerneklokke |
| Temperaturmål | Akustisk mål nå (83 °C) og område (65–88 °C) [Målt], nedklokking og avstenging |
| Vifte | Modus (driverens auto, policy 0 [Målt]), fart per vifte i % og RPM, mål-fart, minste fart (35 %) |
| Begrensninger | Aktive klokkebegrensninger (effekt, termisk, tomgang …) som brikker, med tid siden de slo inn |
| Profiler | De fem plassene som lagrede oppsett (3.3) |

Det kortet ikke kan (låst minneklokke, spenningsoffset, 0 %), står som i demoen under «Støttes ikke på dette kortet», med grunnen.

### 6.5 Vifter (`viftekurve`)

- **Hovedgrafen** er driverens faktiske kurve: punktsky av viftefart mot GPU-temperatur fra historikken (valgfritt vindu, standard 48 t), med en tynn linje for median per grad. Overskriften: «Slik styrer driveren viften». Det er ekte data, og det er det du trenger for å lage en bedre kurve senere.
- Til høyre: modus, fart nå per vifte, minste fart, og «Viften har gått på 35 % i 92 % av tiden siste døgn».
- Utkast av egen kurve (BØR) tegnes oppå punktskyen, så du ser forskjellen.
- Nødregelen (full vifte over 85 °C) og «vifte til auto når Agentic Rigg lukkes» beskrives under «Når endring er slått på», ikke som aktive regler.
- **1.2.0 (runde 7B):** Viftemotoren (`rigg_core::vifte`) er den samme i tjenesten (sim) og i hjelperen: nødregel → utjevning → hysterese → kurve per vifte med stille flate → minste fart → rampe. Vifter har «Felles kurve · Vifte 1 · Vifte 2», panelet **Oppførsel** (hysterese 0–10 °C, rampe opp og ned i %/s, utjevning av/3/5/10 s, «Hold minimum til __ °C») med klartekst under hver, og **Siste 5 minutter** (motoren kjørt over historikken ved siden av slik viftene gikk). Viftestopp (0 %) er ikke mulig: fastvaren holder minst 35 %. Feltene står i `protokoll.md` kap. 11.

### 6.6 RGB-lys (`lys`)

- Lyskontrolleren kan ikke leses uten I2C (adresse 0x49 på bussen som heter «NVIDIA i2c adapter 1», nå `/dev/i2c-0`; numrene flytter seg mellom oppstarter, se `maskinvare-revisjon.md` kap. 9), som ikke er godkjent. Låst-linja sier det rett ut (1.2.0): «Lyskontrolleren leses ikke ennå: lys er siste trinn og kommer i en senere versjon.»
- 3D-kortet, lagene, eksplodert visning og effektene beholdes som **forhåndsvisning**. Valget lagres i appen («Lagre i appen»), så det er klart den dagen lys blir godkjent.
- **2D eller 3D** (brukerens valg 7. november 2026): siden åpner med 2D-kortet, som koster lite. Trykker du 3D, huskes valget til neste gang. Innstillinger › Utseende › Lys › «3D-kortet: Auto / Alltid 3D / Alltid 2D», med Auto som standard: 2D til du har valgt 3D én gang, deretter det sist valgte. Grunnen er målt: første oppbygging av 3D-kortet vekker skjermkortet til P0 i ca. 4 s, og vinduet bruker ca. 136 MB mot 13–15 MB uten 3D (`maalinger.md`, runde 3C og 4B).
- Effekten «Temperatur» bruker den ekte GPU-temperaturen i forhåndsvisningen.
- Nordlys (påskeegg 10) virker som før.

### 6.7 Endring av skjermkortet: bryteren og hjelperen (1.2.0)

Brukerens ønske (`tilbakemelding/1.2.0.md`): «enkelt å skru av og på root eller det trenger for å endre GPU klokker og lys / viftehastighet». Bygget i trinn, og hvert trinn mot det ekte kortet krever brukerens eget ja, hver gang.

- **Innstillinger › Styring** har bryteren **«Tillat endring av skjermkortet»**. Den kjører `pkexec aktiver.sh` eller `deaktiver.sh` fra `~/.local/share/agentic-rigg/hjelper/` (lagt der av `install.sh`, ikke aktivert). Passordvinduet kommer fra polkit. Bryteren flytter seg først når skriptet er ferdig; imens står «Venter på passordet …».
- **Status** ved bryteren: et merke (Av, På, Feil, Slå på igjen) og én setning i klartekst om hva det betyr. Det tekniske (versjon, bygg, kort, feilmelding) står i verktøytipset. «Sjekk igjen» ved feil.
- **Trinnene:** 1. Effekt (effektgrense og temperaturmål), 2. Vifter (modus, fast fart, kurve), 3. Klokker (kjerne- og minneklokke-offset), 4. RGB-lys. Alle er av ved aktivering. Hvert godkjennes med et eget spørsmål (risikoen i én setning, Behold og fristen, passordet) og så passordet, og kan trekkes tilbake. Merket per trinn: «Godkjent», «Ikke godkjent», «Kommer senere» (RGB-lys, og alle mot det ekte kortet i 1.2.0) eller «Ukjent» (hjelperen svarer ikke).
- **«Hva bryteren gjør»:** hjelperen, passordet, hvem som slipper til (bare brukeren din; andre brukere på maskinen slipper ikke til), vernet (Behold innen fristen, 100 % vifte over 85 °C, standard når Agentic Rigg lukkes) og filene.
- **I 1.2.0 er det ekte kortet sperret:** trinnknappene er av mot det, skrivekoden er ikke bygget inn (`ekte-skriving` er av), og vinduet sender aldri skriving dit (`write_gate`). Mot den simulerte backenden i en utviklerforekomst kan hele flyten prøves.
- Protokollen står i `protokoll.md` kap. 12, sikkerhetsgjennomgangen i `hjelper-sikkerhet.md`.

---

### 6.8 Skjermkortstyring og «Styrer»-linja (1.3.0)

Brukeren: «Styring-delen bør egentlig hinte enda mer om at den tilhører GPU / skjermkort … kan se hva / hvilken komponent som blir styrt, kan legge inn sånn auto detect av GPU».

- **Navnene:** seksjonen i sidefeltet heter **Skjermkortstyring**, med kortet under tittelen («RTX 2080», «Palit RTX 2080» med kjent produsent). Toppfeltet sier **Skjermkort › Klokker og effekt / Vifter / RGB-lys**, og det samme står i vindustittelen, Startside, Innstillinger › Skjermkortstyring og Tastatursnarveier. Menypunktene i sidefeltet er korte, og «GPU» gjentas ikke i hver linje.
- **«Styrer»-linja** øverst på de tre sidene, rolig og uten ramme: GPU-ikon, «Styrer», navnet (NVML), og produsent · PCI-adresse · VBIOS · driver. Flere kort: «Kort 1 av 2» med valget (samme som i maskinhodet). Kortet borte, ingen driver eller ny driver: linja sier det i gult eller rødt, og at styringen er utilgjengelig.
- **Produsenten** fra PCI-subsystemets leverandør-ID når det er en kjent kortprodusent (`rigg_core::vendor`: 1569 Palit, 10b0 Gainward, 1462 MSI, 1043 ASUS, 3842 EVGA, 1458 Gigabyte, 19da Zotac …). 10de gir «NVIDIA-referansekort»: det er et referansekort, eller en produsent som ikke setter sin egen ID (som Palit RTX 2080 Rev. A i denne maskinen, 10de:1e87). Det sies ærlig, ikke gjettes.
- **«Produsent (valgfritt)»** (blyanten i linja): lagres per kort (UUID, ellers PCI-adressen) i `ui.json` (`gpuMakers`, høyst 40 tegn), vises i linja og maskinhodet, og endrer ikke kortet.

## 7. Påskeeggene i produksjon

Alle ti beholdes og følger reglene i `docs/paaskeegg.md` (ufarlige, stille, ingen advarselsfarger, én bryter, tilgjengelige). Det som må tilpasses:

| Egg | Endring |
|---|---|
| 1 Juksekoden | Uendret. Teksten «kortet får aldri vite om dette» er enda mer sann nå |
| 2 Støvtørking | Uendret (appikonet i sidefeltet, fem klikk). Animasjonen følger GTKs redusert bevegelse |
| 3 Utviklermodus | Uendret (sju klikk på versjonen i Om). Raden i Feilsøking som før |
| 4 Svaret på alt | Uendret. 42 °C kommer fra ekte målinger |
| 5 Bursdag | Uendret (20. september, RTX 2080) |
| 6 Nattevakt | Alpha-merket finnes ikke lenger. Månen står ved appnavnet øverst i sidefeltet |
| 7 Kaffe | Uendret (filteret på Sensorer) |
| 8 Kallenavn på kurven | Lever i utkastet på Viftekurve (6.3). Uten utkast vises det ikke |
| 9 Hilsen i konsollen | Det finnes ingen nettleserkonsoll. Hilsenen skrives til standard ut når appen startes fra en terminal (ikke i journalen). `agentic-rigg --hint` gir et hint om et egg du ikke har funnet, slik `rigg.hint()` gjorde |
| 10 Nordlys | Uendret på Lys, også i 3D (GtkGLArea) |

Hva du har funnet, lagres i `ui.json` (`eggsFound`), ikke i tjenesten.

---

## 8. Latt være

- **Lukk til bakgrunnen i vinduet.** Bakgrunnen er tjenesten. Et skjult GTK-vindu holder grafikkdriveren i minnet (målt +18 MB privat minne som ikke frigjøres, `arkitektur.md` kap. 2).
- **Validering på nett og automatiske oppdateringer.** Appen sender ingenting ut av maskinen.
- **Lyd og blinking ved alarm.**
- **Simulert kort i appen.** Ingen falske tall, heller ikke som reserve.
- **Spennings- og klokkekurven som illustrasjon.**

---

## 9. Ytelsesmål

Målt på denne maskinen med skriptet `app/scripts/maal.sh` (`arkitektur.md` kap. 10). Tallene skal stå i dokumentasjonen for hver runde.

| Hva | Krav | Mål |
|---|---|---|
| Kald start til første bilde (vinduet, tjenesten kjører) | ≤ 250 ms | 150 ms |
| Kald start når tjenesten også må startes | ≤ 350 ms til første bilde, tall innen 1 s | – |
| Varm start (vinduet finnes) | ≤ 50 ms | – |
| Lukk: vinduet borte / prosessen avsluttet | ≤ 50 ms / ≤ 300 ms | – |
| Vinduet, privat minne (Private_Dirty) med GL-tegner, alle sider besøkt | ≤ 70 MB | 50 MB |
| Tjenesten, privat minne med full historikk (48 t) | ≤ 45 MB | 35 MB (NVML alene er 23 MB [Målt]) |
| Tjenesten, CPU i bakgrunnen (2 s-takt, kortet i P8) | ≤ 1,2 % av én kjerne | 1 % (revisjonen: slank runde hvert sekund = 1,9 %) |
| Tjenesten, CPU med vinduet åpent (1 s-takt) | ≤ 2,5 % av én kjerne | – |
| Polling | aldri over 2 Hz, aldri runder etter hverandre; kortet skal bli i P8 i ro | – |
| Vinduet, CPU i ro på Oversikt (ingen animasjon) | ≤ 2 % av én kjerne | 1 % |
| Vinduet minimert | ≈ 0 % (ingen tegning, ingen tidtakere) | – |
| Historikk på disk | ≤ 8 MB, skrives høyst hvert 60. s | – |

Hvis kald start ikke når 250 ms etter optimaliseringsrunden, kan vinduet bli liggende skjult i 5 min etter lukking («rask gjenåpning»). Det er reserveplanen, ikke planen (`arkitektur.md` kap. 3.4).

---

## 10. Spørsmål til brukeren

Punkt 1–4 og Lys er besvart av brukeren 7. november 2026 (runde 4B). Svarene endrer standardverdier, ikke arkitekturen.

1. **Opptak i bakgrunnen: av som standard, valgfritt.** Brukeren vil velge selv. Standard er av, så tjenesten stopper 60 s etter at vinduet er lukket. Valget står øverst i Innstillinger › Generelt › Bakgrunn og forklarer begge: av = ingenting kjører når appen er lukket, og grafene får et hull for den tiden; på = historikken fortsetter, det koster ca. 21–28 MB og 0,85 % av én kjerne ved 2 s, og ikonet vises i systemfeltet. «Takt i bakgrunnen» 1, 2 (standard) eller 5 s.
2. **Start ved innlogging: av.** Kan slås på i Innstillinger › Generelt › Bakgrunn.
3. **Systemfelt: ja,** mens tjenesten går i bakgrunnen. Menyen og verktøytipset står i 3.1.
4. **Skrivebordsvarsler: av som standard,** kan slås på i Innstillinger › Logging og varsler.
5. **Lys: «2D, 3D ved én knapp».** Lys åpner med 2D; valget av 3D huskes. Se 6.6.
6. **GPU-klokka heter «Kjerneklokke» overalt** (Oversikt, Sensorer, Grafer, Kontroller, statusblokken; «Kjerne» i klokketabellene), som «Core clock» i Afterburner, og minneklokka «Minneklokke».
7. **Merket «Lest fra kortet»** på Kontroller er fjernet. Normale verdier merkes ikke (kapittel 5); banneret sier det.
8. **Første trinn med skriving** (effektgrense) kan forberedes mot den simulerte backenden nå, men slås ikke på uten ditt ja. (Åpent.)
