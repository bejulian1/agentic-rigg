# Agentic Rigg 1.1.0 · bygg 17 · kritikk og finpuss (runde 6D)

*Agent 6D, 9. november 2026. Eneste agent etter 6A, 6B og 6C. Funnene er rettet i samme runde; det som står igjen, er merket «Ikke rettet».*

## Sammendrag

1.1.0 er en stor runde: to nye sider (Prosessor og Disker), et nytt skall (maskinhode, vindusknapper, nye navn på Styring) og to redesignede styringssider. Hver del er gjort grundig, men de tre agentene har ikke sett hverandres sider ferdige. Det som skilte, var i sømmene:

- RGB-lys hadde fortsatt det store banneret fra 1.0.0, mens de to andre Styring-sidene har den rolige låst-linja.
- Grafhoder kuttet tittelen («Eff… ▬ Tr… ‐‐ G…») i standardvinduet (1440 px) på Klokker og effekt.
- Statusblokken kalte alle disker «Disktemp».
- Temperaturmålet regnet i °C med °F valgt.
- Gamle sidenavn sto igjen i tekster («Se Viftekurve», påskeeggene, rapporten, `--help`).
- Skjermbildeplanene for 6A–6C var ikke med i `bilder.sh --plan alle`, og `3a-sim` klikket en knapp som ikke finnes lenger.

**Telling:** 8 Må · 14 Bør · 7 Kan (29 funn). **Rettet:** alle 8 Må og 14 Bør, og 4 av 7 Kan.

**P0-toppene** fra 6C kom ikke fra tjenesten. Se punkt 1.

## Slik er det gjort

- Lest `REGLER.md`, brukerens tilbakemelding, LOGG 6A–6C, `maalinger.md` 6A–6C og `kritikk-1.0.0.md`.
- Gått gjennom 6A-, 6B- og 6C-bildene i `docs/skjermbilder/1.1.0/`, og en egen forserie med release-bygget (planene `3c`, `6a`, `6c-ekte` og `2c`, privat økt, cairo) før rettingene. Den endelige serien etter rettingene ligger i `docs/skjermbilder/1.1.0/6d/`.
- Øynene til en Afterburner-, HWiNFO-, CrystalDiskInfo- og CPU-Z-bruker: samme ordlyd, enheter, tusenskille og avrunding på tvers av sidene, ingenting kuttet ved 640 og 1440 px, og ingen tekst som sier noe annet enn det som skjer.
- Kortet er bare lest (NVML-get og `nvidia-smi --query-gpu`). Ingen I2C, ingen `/dev/sd*`, ingen av brukerens prosesser rørt.

Merking: **Må** = feil eller noe brukeren ba om · **Bør** = synlig uferdig eller inkonsekvent · **Kan** = finpuss.

---

## Funn

### Tjenesten og målingene

1. **Må · P0-topper uten klient (fra 6C).** *Rettet: årsaken var ikke tjenesten.*
   - **Hva 6C så:** kortet i P0 i 4–5 s omtrent 54–63 s og 80–83 s etter at `agentic-riggd` startet, uten vindu.
   - **Undersøkt alene** (ingen andre agenter, `nvidia-smi --query-gpu=pstate,clocks.gr -lms 1000`, rådata i `maal/runde6d-p0-*.csv`, skriptet `verktoy/runde6d/p0-forsok.sh`):

     | Forsøk | Varighet | P0 |
     |---|---|---|
     | Grunnlinje, ingenting av meg kjører | 240 s | 0 |
     | A: tjenesten alene, ekte NVML, uten Shell og øktbuss | 200 s (223 prøver) | 0 |
     | C: bare den private økta (hodeløs Shell), uten tjeneste | 220 s | 3 s, idet Shell starter |
     | B: privat økt, tjenesten startet 20 s etter Shell, uten vindu, som 6C | 200 s (243 prøver) | 4 s, idet Shell starter; ingen etter 54–63 s eller 80–83 s |
     | 6D-målingene: tjenesten alene 2 × 180 s, og vinduet på alle ti sidene | se `maalinger.md` | se `maalinger.md` |

   - **Årsak:** hver hodeløse `gnome-shell` vekker kortet i 3–4 s når den starter (kjent fra 4A). 6A, 6B og 6C kjørte private økter samtidig, så toppene i 6Cs logg var de andre agentenes Shell-starter. Tjenesten gjør ingenting hvert minutt som rører kortet: `every60` brukes ikke av NVML-backenden, `malloc_trim` og lagringen av historikken hvert 60. s er bare minne og disk, og UDisks2 spørres ikke i bakgrunnen (diskvarselet er av).
   - **Gjort:** ingen kodeendring trengs. Forsøkene og skriptet ligger der målingene ligger, og `maal6d.sh` logger P-tilstanden ved hvert steg, så det kan sjekkes igjen.

2. **Må · Ustabile tester under høy last.** *Rettet.*
   - `riggd/tests/kontrakt.rs::kortet_borte_står_i_lista` leste `list_gpus` én gang rett etter `hello`. Før første runde er lista tom, så `gpus[0].state` var `null`.
   - `riggd/tests/logg.rs::tilstandene_fra_sim_scenariene` gjorde det samme, og `all()` på en tom liste er sann, så `list[0]` feilet etterpå.
   - Begge venter nå på tilstanden (sjekk hvert 50. ms, frist 14–20 s), og `wait_event("telemetry")` etter at kortet er borte har 15 s frist. Kallene venter på svar i høyst 30 s i stedet for å feile ved lesefristen på 5 s, og en halv linje fra før fristen tas vare på. Testene blir ikke tregere når maskinen er i ro: de går videre så snart tilstanden er der.

### Skall og sidefelt

3. **Må · Statusblokken kaller alle disker «Disktemp».** *Rettet.*
   - Festede disknøkler (`disk/<serie>/temp|read|write|busy`) hadde bare korte navn. Med fem disker sto det fem like rader.
   - Nå: «sda temp», «nvme0n1 les», «sdb skriv», «sda aktiv», med kildemerket «DISK» i ikonlista, og verktøytipset og Innstillinger › Sidefelt sier «Samsung SSD 870 QVO 1TB (sda) · temperatur». Navnet kommer fra disklista i siste prøve, også når den kommer etter at blokken er bygget. Kjernenavnet er valgt fordi like disker har samme modell, og fordi Disker viser det samme navnet. Test: `disknøklene_har_disken_i_navnet`.

4. **Må · Tastatursnarveier mangler Styring.** *Rettet.*
   - Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y, Ctrl+Enter, piltastene, PgUp/PgDn og Home/End på gliderne, og [ ], piltastene, + og Delete på kurvepunktene sto ikke i oversikten.
   - Ny gruppe «Styring (Klokker og effekt, Vifter)». Dialogen har to spalter som GNOMEs snarveivindu (860 px bred når vinduet er minst 900 px), så alt får plass uten å rulle; under 900 px er den én spalte som før. Test: `hver_gruppe_står_i_én_spalte_og_styring_er_med`.

5. **Bør · `agentic-rigg --help` mangler Prosessor og Disker.** *Rettet.* Lista har alle ti sidene, og de nye navnene står ved `kontroller`, `viftekurve` og `lys`.

6. **Bør · Gamle sidenavn i tekster.** *Rettet.*
   - «Se Viftekurve.» i verktøytipset for viftestyring på Oversikt → «Se Vifter.»
   - Påskeeggene: «på Viftekurve» → «på Vifter», «på Lys» → «på RGB-lys».
   - Rapporten: «Kontroller er låst» og «som Kontroller bruker» → «Klokker og effekt».
   - Utseende: «3D-kortet på Lys» → «3D-kortet på RGB-lys».
   - «Hva skal til?», trinn 6: «Lys (Palit-kontrolleren på I2C)» → «RGB-lys (…)».

### Styring

7. **Må · Temperaturmålet regner i °C når enheten er °F.** *Rettet.*
   - Tallfeltet viste 83 og enheten «°C», også med °F valgt. Glideren og merkene brukte allerede `F::temp`, men tallfeltet og enheten gjorde det ikke. Det samme gjaldt temperaturen i punkttabellen på Vifter.
   - Nytt `oc::temp_spin`: verdien i feltet er fortsatt °C (det NVML og utkastet bruker), men teksten vises og leses i enheten fra Innstillinger, og skrives på nytt når enheten byttes. Enhetsetiketten følger med. Punkttabellen har «Temperatur (°F)» og «Viftefart (%)» i overskriften, og verdiene rundes til hele grader °C, som i grafen.

8. **Må · Vifter ved 640 px kutter kurvenavnet og «Data for 1 min …».** *Rettet.*
   - Tidsvinduet og dekningen står under hverandre til høyre for forklaringen, så «Data for 2 min av 48 t» alltid får plass.
   - Under 720 px på kurvekortet er «Tilbakestill kurven» bare ikonet (teksten står i verktøytipset), så navnet på kurven («Balansert · standard») får plass i hodet.

9. **Må · Grafhodene kutter tittelen.** *Rettet.*
   - På Klokker og effekt i 1440 px (standardvinduet) sto «Eff… ▬ Tr… ‐‐ G…» over effektgrafen. Tittelen og forklaringen ble kuttet samtidig.
   - Ny `fixed::AllOrNothing`: forklaringen i et grafhode (grafer med opptil tre serier) står helt eller ikke i det hele tatt, så tittelen aldri kuttes. «min · maks» i hodet gjør det samme, i stedet for «…0,0» i smale grafer (Disker ved 640 px). Grafer med mange serier (disktemperaturen) kutter fortsatt navnene, så de kan skjules.

10. **Bør · RGB-lys har et annet låst-banner enn de to andre Styring-sidene.** *Rettet.* RGB-lys bruker nå samme låst-linje: «Låst · Lyskontrolleren er ikke lest: det krever I2C på skjermkortet …» og «Hva skal til?». Det gamle banneret (`Banner`) og CSS-en for det er fjernet.

11. **Bør · «Låst» i toppfeltet ved siden av en aktiv «Bruk».** *Rettet.* Mot den simulerte backenden (skjermbildene, testene) er Bruk på, men merket «Låst» sto der likevel. Merket følger nå porten (`locked::sync_topbar`).

12. **Bør · «Mål-fart 35 %» med «35 % · 35 %» under.** *Rettet.* Like mål: «begge viftene». Ulike: «vifte 1 40 % · vifte 2 45 %».

13. **Kan · «Tomgang: Ja · siden 01:13» i aksentfarge under Begrensninger.** *Rettet.* Tomgang er den vanlige tilstanden og står nøytralt; aksenten er for begrensninger som faktisk holder kortet igjen.

14. **Kan · «P-tilstand: P8 / tomgang» og «Begrenset av: Tomgang / P8 · ingenting å gjøre» side om side.** *Ikke rettet.* Det står to ganger, men begge flisene er riktige og har hver sin jobb når kortet jobber.

15. **Kan · Merket «Utkast · ikke sendt til kortet» står etter Bruk → Behold mot sim.** *Ikke rettet.* Bare synlig mot den simulerte backenden, og utkastpanelet sier «likt det kortet har nå».

16. **Bør · `3a-sim` klikker «Lag utkast», som ikke finnes.** *Rettet.* Planen bruker `locked::edit_draft`, som `draft_kontroller` i `styring_bilder.rs`.

### Maskinvare

17. **Bør · Sårbarhetene heter «L1tf», «Mds», «Itlb multihit», «Srbds» og «Tsa».** *Rettet.* Kjente navn står som de skrives: L1TF, MDS, iTLB Multihit, SRBDS, TSA, MMIO Stale Data, Speculative Store Bypass, Return Stack Overflow (SRSO), TSX Async Abort, GhostWrite, VMScape, «Gammel mikrokode». Ukjente får fortsatt «Ny sårbarhet»-formen.

18. **Bør · «boost på · last 37,42» på Prosessor.** *Rettet.* Ved siden av flisa «Last 98,7 %» så lastsnittet ut som en annen last. Nå «lastsnitt 37,42», som verktøytipset.

19. **Bør · Diskradene på Sensorer er like ved 640 px.** *Rettet.* «ST2000DM008-2FR102 (sda) · lesing» ble «ST2000DM008-2FR1…» på alle fire radene. Nå står kjernenavnet og verdien først: «sda · lesing · ST2000DM008-2FR102». CSV-kolonnene er uendret.

20. **Bør · Serienummer og fastvare kuttes på Disker ved 640 px.** *Rettet.* Linja brytes i stedet for å kuttes.

21. **Kan · «7,9 GiB av 8 GiB» for swap på System.** *Rettet.* «7,9 GiB av 8,0 GiB», samme desimaler på begge sider.

22. **Kan · Helsemerket på Disker** («OK», «Advarsel») har ikon og tekst til venstre i et merke som er like bredt for alle. *Rettet.* Innholdet står midt i merket.

23. **Kan · Modellnavnet kuttes på Disker ved 640 px** («Samsung SSD 870 QVO 1TB» med merkene ved siden av). *Ikke rettet.* Hele navnet står i Disk-panelet og i verktøytipset.

### Tekst, enheter og avrunding

24. **Bør · Tall og enhet brytes over to linjer.** *Rettet.* «minst 35 / % (grensen ligger i VBIOS)» på Vifter. 21 lange tekster i sidene og Innstillinger hadde vanlig mellomrom mellom tall og enhet (%, °C, MHz, MB, W, ms, s, min, t). De har nå smalt hardt mellomrom (U+202F), som tallene fra `fmt` allerede har.

25. **Bør · Skjermbildeplanene for 1.1.0 er ikke med i `bilder.sh --plan alle`.** *Rettet.* `6a`, `6a-ekte`, `6b`, `6b-borte`, `6b-ingen-driver`, `6b-driver-oppdatert`, `6b-to-kort`, `6c-ekte` og `6c-lys` er nye planer i `bilder.sh` og med i `alle`. 6B-planene kunne bare kjøres med 6Bs eget skript. Standardmappa er `docs/skjermbilder/1.1.0/`.

26. **Kan · Snarveiene har ikke skilletegn mellom alternativer** («Ctrl Shift Z Ctrl Y», også «Ctrl + Ctrl −» og «Alt ↑ Alt ↓» fra 1.0.0). *Rettet.* Et dempet «eller» står mellom tastene som gjør det samme.

### Dokumentasjon

27. **Bør · Produktspesifikasjonen har ni sider og de gamle navnene.** *Rettet.* Kapittel 3.9 har de ti sidene med `id`, gruppene og maskinhodet; 3.4 har tastene 1–9/0 og Styring-tastene; kapittel 6 har de nye navnene, låst-linja og utkast uten «Lag utkast».

28. **Bør · README, STATUS og PATCHLOG gjelder 1.0.0.** *Rettet* (1.1.0 · bygg 17).

29. **Må · Versjonen er 1.0.0 · bygg 16.** *Rettet.* `app/version.json` og `Cargo.toml` er 1.1.0 · bygg 17, kanalen er uendret (`stable`). `--version` for begge binærene, Om og rapporten skriver «1.1.0 · bygg 17».

### Sjekket og i orden

30. **Oversikt, Sensorer og Grafer:** de nye navnene brukes overalt i sidefeltet, titlene og verktøytipsene. Sensorer har gruppa Disker med temperatur, lesing, skriving og aktivitet; Grafer har «Disktemperatur» og «Disk-I/O» i velgeren; Oversikt har ingen diskdel, som før (den er for skjermkortet).
31. **Enheter og avrunding på tvers:** MB/s for disker er desimale og runder likt på Disker, Sensorer og i grafene (`disker::mbps`). Temperatur har én desimal i tabellene (Sensorer, Prosessor) og ingen i fliser og statusblokken, som i 1.0.0. Tusenskille er U+202F overalt.
32. **Vindusknappene, maskinhodet og Code Pro** i Om ser ut som 6B målte dem, i lyst, mørkt og høy kontrast.

## Det som gjenstår

- Kan 14, 15 og 23 (over).
- Brukeren har ikke sett 1.1.0 installert. Hovedøkta installerer etter brukerens ja.
