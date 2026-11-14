# Status

**1.3.3 · bygg 22** – 14. november 2026: CPU-lanseringsdato fra et kildebelagt lokalt modelloppslag, delt av Prosessor og rapport. Ryzen 7 2700X viser 19. april 2018. Ingen dato gjettes fra familie eller kodenavn. Revisjonen av komponentenes spesifikasjoner står i `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`; øvrige utvidelser og korrekthetsfunn er fortsatt åpne. Produksjonsbygget er installert for brukeren 14. november etter uttrykkelig ønske.

Verifisert: workspace-clippy på alle mål er ren; 350 tester bestått, 4 ignorert, 0 feilet. Aliasoppslag, nabomodeller, feil leverandør og dato/kilde i tekst- og Markdown-rapport er dekket. Alle workspace-binærene er bygget lokalt.

Installasjonskontroll: alle tre installerte binærer viser 1.3.3 · bygg 22 og har samme SHA-256 som release-bygget. Kjørende NVML-tjeneste annonserer bygg 22 og låst skriving; appen er startet på Prosessor. Innstillingsfilene er uendret ved installasjon, og CPU-historikk fra før omstart er lest tilbake. Brukertjenesten er aktiv, men fortsatt ikke aktivert ved innlogging. Sikkerhetskopi av gammel installasjon, innstillinger og historikk er bevart lokalt.

**1.3.2 · bygg 21** – 14. november 2026: kontroll og retting av adaptive akser. Vanlige målegrafer utvides for temperatur, klokker, effekt og hastigheter utenfor normalområdet; også ferske langtidsverdier og synlig interpolasjon ved vinduskanter tas med. Viftekurvens målte punkter har adaptive visningsakser, mens lovlige redigeringsgrenser er beholdt.

Verifisert: ren workspace-clippy på alle mål, 347 beståtte tester, 4 ignorerte og 0 feil. Skjermbildeplanen `adaptive-akser` viser 110 °C, 2500/10000 MHz, 400 W, 108 % vifte og 90 °C disk, med «Fast», «Tilpass», °F og 48-timersvindu. Ekte klikk gjennom ti sider og ni innstillingsdeler samt smal visning, søk, Esc, simulert Bruk/Behold/Rull tilbake og RGB-forhåndsvisning er gjennomført. Gjennomgangen har åpne funn i `docs/produksjon/kritikk-1.3.2.md`; dette er en testutgave før installasjon, og det er ikke gjort nye ytelsesmålinger.

**1.3.1 · bygg 20** – 13. november 2026: kortmenyen under Skjermkortstyring følger sidefeltets bredde. Maskinhodets skjermkort åpner Grafikkort direkte. Sammenlagt sidefelt har sentrerte ikoner, også Innstillinger. Søket slipper fokus ved klikk utenfor, og Esc avslutter søket før Innstillinger lukkes. Se `PATCHLOG.md`. Dette er en testutgave før installasjon; funksjonsstatusen under gjelder fortsatt.

Verifisert: `cargo clippy --workspace --all-targets -- -D warnings` uten advarsler; `cargo test --workspace`: 338 bestått, 4 ignorert, 0 feilet. De nye skjermbildeplanene `kortmeny` (også med to kort) og `innstillinger-sok` er kjørt med layout- og funksjonskontroller. Ekte museklikk i en privat GNOME-økt bekrefter menyplassering, sentrerte ikoner og at søkefokusrammen forsvinner ved klikk utenfor. Ingen nye ytelsesmålinger er gjort.

**1.3.0 · bygg 19** – 9. november 2026: klar til å installeres når hovedøkta sier ja. Nytt siden 1.2.0:
- **Oversikt over hele maskinen:** fliser for prosessor, skjermkort, minne, VRAM, disker og nettverk, en tilstand-linje når noe er verdt å se, seks grafer, og skjermkortets målere nederst.
- **Ingen graf kutter verdier:** aksene utvides over og under det foretrukne området (110 °C, under 20 °C, 330 W, disk på 65 °C), uten å pumpe, med nedklokking og avstenging som streker.
- **Disker uten tomrom:** Disk og SMART side om side, Partisjoner i hele bredden.
- **Skjermkortstyring:** seksjonen, toppfeltet og panelene sier at det er skjermkortet som styres, og «Styrer»-linja viser kortet (navn, produsent, PCI-adresse, VBIOS, driver), funnet av seg selv, med produsent som kan settes for hånd.

Runde 8A (`docs/produksjon/LOGG.md`). Endringene står i `PATCHLOG.md`. `cargo test --workspace` kjører 338 tester (4 ignorert), og clippy er ren. **Det ekte kortet er fortsatt sperret.**

## Virker

### Skjermkortet (NVML, bare lesing)

Tjenesten leser kortet med NVML hvert sekund mens et vindu som synes abonnerer, og sjeldnere i bakgrunnen. Ingen kall endrer kortet. Tjenesten alene holder kortet i P8 (målt i runde 6D).

- Temperatur, GPU- og minnekontroller-last, koder og dekoder, kjerne-, SM-, minne- og videoklokke, effekt (snitt fra energitelleren), effektgrense, vifter med fart og turtall hver for seg, målfart og viftestyring, VRAM (`MemoryInfo_v2`), P-tilstand, PCIe nå og maks, og klokkebegrensninger.
- Faste data: navn, driver, VBIOS, CUDA, PCI-ID og subsystem, UUID (skjult til du viser den), grensene for effekt, temperatur, klokke-offset og vifte, P-tilstandene og prosessene som bruker kortet.
- Kortet som faller av bussen, ny driver uten omstart, ingen driver og dvale vises med egne tekster.

### Prosessor, disker og maskinen

- **Prosessor:** Tdie/Tctl, last, snitt- og toppklokke, governor og boost, klokke og last per tråd gruppert etter CCX og kjerne, cache, instruksjonssett og sårbarheter.
- **Disker:** UDisks2 uten root (modell, serie, fastvare, o/min, SMART-helse som CrystalDiskInfo, temperatur, driftstid, hele SMART-tabellen, selvtest), `/proc/diskstats` (MB/s, aktivitet) og ledig plass per partisjon. Aldri selvtest, aldri `/dev/sd*`. Diskene står også i Sensorer, Grafer og statusblokken («sda temp»).
- **System:** hovedkort og BIOS fra DMI, minne som går opp i totalen, OS og økt, og kontrollerne (nett, lyd, USB, SATA, NVMe).
- Alle hwmon-sensorer med navn, og nett per grensesnitt.

### Tjenesten, historikken og bakgrunnen

- Historikk i 48 t på disk, min, maks og snitt, logging til CSV, varsler (også diskhelse) og rapport (tekst og Markdown).
- «Fortsett i bakgrunnen» (av), «Takt i bakgrunnen» 1, 2 eller 5 s, «Start tjenesten ved innlogging» (av) og «Stopp tjenesten nå».
- Systemfeltet mens tjenesten går i bakgrunnen.

### Vinduet

- Ti sider og Innstillinger, tastene 1–9 og 0, fire fargetemaer i lyst og mørkt, høy kontrast, seks aksentfarger, tre tettheter og fire skriftstørrelser. Fra 640 til 1920 px.
- Oversikt (1.3.0) over hele maskinen: seks fliser, tilstand-linja, grafene for last, temperaturer, effekt, minne og VRAM, disk og nett, og skjermkortets målere. Alle grafene utvider aksen i stedet for å kutte.
- Sidefeltet med maskinhodet (skjermkortet og prosessoren), Skjermkortstyring med kortet under tittelen og «Bare visning», og statusblokken med festede sensorer.
- Vindusknappene som i GNOME (libadwaita/Yaru), etter `gtk-decoration-layout`.
- Klokker og effekt og Vifter: store tall øverst, glidere med standardmerke og Tilbakestill, kurveredigerer, (i) med forklaring, angre og gjør om (Ctrl+Z / Ctrl+Shift+Z). Temperaturer i °C eller °F.
- Vifter (1.2.0): felles kurve eller én per vifte, hysterese, rampe opp og ned, utjevning, «Hold minimum til», «Siste 5 minutter» med kurven mot slik viftene gikk, og forhåndsvalgene Stille, Balansert og Ytelse.
- «Styrer»-linja (1.3.0) øverst på Klokker og effekt, Vifter og RGB-lys: kortet som styres, med produsent (fra PCI-subsystemet, eller satt for hånd), PCI-adresse, VBIOS og driver, og kortvalget når det er flere.
- Innstillinger › Skjermkortstyring (1.2.0): bryteren, status i klartekst, trinnlisten med eget spørsmål før passordet, og «Hva bryteren gjør». Merket ved Skjermkortstyring i sidefeltet følger tilstanden.
- Tastatursnarveier har alle tastene, også for gliderne og kurven.

## Låst

- **Endring av det ekte kortet.** Bryteren og hjelperen finnes, men mot det ekte kortet kan ingen trinn godkjennes i 1.3.0. Skrivekoden er ikke bygget inn (`ekte-skriving` av, `EKTE_SKRIVING = false`), og vinduet sender aldri skriving dit. Klokker og effekt, Vifter og RGB-lys viser de ekte verdiene, og endringer blir utkast og profiler i appen. Bare `--backend sim` (testdata, utviklerforekomsten) godtar Bruk, Behold og Rull tilbake, også gjennom hjelperen. Neste trinn (effektgrense) følger planen i `docs/produksjon/hjelper-sikkerhet.md` kap. 6, med brukeren til stede.
- **I2C.** Appen åpner aldri `/dev/i2c-*`. Lyskontrolleren leses derfor ikke, og RGB-lys er en forhåndsvisning.

## Referanse, ikke målt

Spesifikasjonstabellen på Grafikkort (RTX 2080 Rev. A, TU104) er slått opp på PCI-ID og merket «Referanse, ikke målt».

## Målt i bygg 19 (runde 8A)

Release-bygget i en privat økt med det ekte kortet og cairo, hjelperen av. **Forholdene var ikke rolige:** andre programmer holdt skjermkortet i P0 (908 av 1 123 prøver, også før appen startet) og lastsnittet på 4,6–16,8. Detaljer og sammenligningen med 1.2.0 i samme time står i `docs/produksjon/maalinger.md`, runde 8A.

| | Krav | Målt |
|---|---|---|
| Kald start til første bilde | ≤ 250 ms | 236 ms (median av 15; 211–267 utenom første) |
| Lukk: vinduet borte / prosessen avsluttet | < 100 ms | 0,8–1,6 ms / 9,2–21,7 ms |
| Vinduet i ro, Oversikt | ≤ 2 % | 2,25–2,50 % med kortet i P0 (1.2.0 i samme time: 4,95–8,55 %); ikke målt i rolige forhold |
| Vinduet i ro, de andre sidene | ≤ 2 % | 0,50–3,20 % med kortet i P0: System, Disker, Klokker og effekt og Vifter under 2 %, Grafikkort 1,00–2,50 % etter rettingen, Sensorer, Prosessor og Grafer 2,05–3,20 % (Grafer mot 1.2.0 i samme time: 1,40–1,45 mot 1,65–1,85 %); RGB-lys i 2D animerer: 17,6 % |
| Vinduets minne (privat) | ≤ 70 MB | 12,1–17,1 MB |
| Tjenesten i bakgrunnen, 2 s | ≤ 1,2 % | 0,63 % / 0,73 % |
| Tjenesten med vinduet åpent | ≤ 2,5 % | 1,65–2,35 % på åtte sider; enkeltmålinger opptil 2,80 % (Grafikkort) og 3,00 % (Disker) |
| Tjenestens minne (privat) | ≤ 45 MB | 21,2–22,5 MB |

## Verifisert i bygg 19 (runde 8A)

- `cargo clippy --workspace --all-targets -- -D warnings` er ren. `cargo test --workspace` gir 338 grønne tester (4 ignorert mot det ekte kortet og UDisks2).
- Hele skjermbildeserien (`scripts/bilder.sh --plan alle`, med de nye planene `8a` og `8a-hete`) ligger i `docs/skjermbilder/1.3.0/8a/`.

## Ikke dekket

- Installasjonen av 1.3.0 i brukerens ekte økt gjøres av hovedøkta. Første bilde bør måles der.
- Topp-prosesser på Oversikt er ikke laget: det krever en ny lesing av alle `/proc/*/stat` hvert sekund i tjenesten, og er ikke billig nok for startsiden.
- Bryteren er aldri slått på med ekte `pkexec` og polkit. Det gjør brukeren selv. Hjelperen er kjørt som vanlig bruker mot en falsk rot og det simulerte kortet.
- Diskene med SMART er prøvd mot den ekte UDisks2 i 6A, men skjermbildene fra den private økta har tom systembuss («Ukjent»-helse); de simulerte diskene viser SMART.
- Aksent «System» og høy kontrast «Automatisk» (spesifikasjonen 3.10, BØR) finnes ikke ennå.
