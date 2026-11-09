# Agentic Rigg 1.0.0 · bygg 16 · kritikk (runde 4A)

*Kritikeren i runde 4A, 7. november 2026. Ingen kode er endret. Runde 4B retter funnene.*

## Sammendrag

Appen er nær et ferdig produkt. Den starter raskt, tallene er ekte og stort sett riktige, og alle åtte sidene og Innstillinger følger demoen tett i alle fire temaer, lyst og mørkt, og høy kontrast. Clippy er ren, og `cargo test --workspace` gir 177 grønne tester (2 ignorert mot det ekte kortet).

Det som fortsatt ser ut som en demo, er små ting du ser hver dag, og feiltilstander der teksten ikke stemmer med det som skjer:

- avkuttet tekst i sidefeltet og på Oversikt ved 640 px,
- «standard»-merker som står på feil sted på Kontroller,
- to forskjellige antall målinger på Viftekurve,
- «Lest fra kortet» mens kortet er borte.

To MÅ-krav fra spesifikasjonen mangler: «Stopp tjenesten nå» og lisensene. Med de 7 Må-punktene rettet kan 1.0.0 installeres. Bør-punktene er finpussen som skiller appen fra HWiNFO-kvalitet.

**Telling:** 7 Må · 20 Bør · 16 Kan (43 funn).

**P0/P8:** det er appen som vekker kortet, ikke skjermbildetakingen. Første gang Lys bygger 3D-kortet går kortet til P0 (1 515 MHz, 36–39 W) i ca. 4 s. Les mer under «Ytelse og målinger».

## Slik er det gjort

- Jeg har lest `REGLER.md`, produktspesifikasjonen, `arkitektur.md` (deler), `maalinger.md`, `LOGG.md` og `runde2-api.md`, og koden i demoen (`web/`) der den er sammenlignet.
- Jeg har gått gjennom de 228 skjermbildene i `docs/skjermbilder/1.0.0/runde3c/`. Stiene under er relative til `docs/skjermbilder/1.0.0/`.
- Egne bilder og måledata ligger i `runde4a/`:
  - planen `3c-640` (alle sidene startet i 640 px), med release-bygget i en privat økt,
  - `pstate-oversikt.txt` og `pstate-lys.txt`.
- Kortet er bare lest (`nvidia-smi --query-gpu`). Ingen I2C er åpnet, og ingen av brukerens prosesser er rørt. Øktene mine er avsluttet og mappene slettet.
- **Ikke dekket:** tastatur og fokus er ikke prøvd for hånd. Jeg har ikke noe verktøy som sender taster til den private økta, utenom planene. Bare koden er lest (`shortcuts.rs`). 4B bør ta en runde med bare tastatur, se punkt 43.

---

## Funn

Merking: **Må** = før installasjon · **Bør** = i 1.0.0 hvis det rekkes · **Kan** = senere eller ved anledning.

### Sidefelt og skall

1. **Må · Statusblokken kutter navn og gjentar enheten.**
   - **Hva er galt:** Festet minneklokke vises som «VRAM-klo…». Festet trådklokke heter «Tråd 3 MHz» og har i tillegg verdien «2 200 MHz», så enheten står to ganger.
   - **Hvor:** `rigg/src/pages/sensorer.rs:343` og `:353` (`short_name`).
   - **Bilde:** `runde3c/3a/sidefelt-festede-nokler-lys.png`.
   - **Retting:** Bruk «Minnekl.», i samme mønster som «Minnekontr.», og «Tråd 3» uten «MHz». Legg inn en test som sjekker at alle korte navn får plass i bredden navnet har, med standard skrift og «Stor».

2. **Bør · Sparklinjene i statusblokken forstørrer støy.**
   - **Hva er galt:** «Turtall 1» svinger mellom 797 og 803 o/min, men linja ser ut som store hopp.
   - **Bilde:** `runde3c/3a/sidefelt-festede-nokler-lys.png`.
   - **Retting:** Gi hver sensor et minste spenn (`minSpan`), slik demoen gjør for klokkene. Forslag: turtall 200 o/min, temperatur 5 °C, effekt 20 W, prosent 10.

3. **Bør · Kortvelgeren viser en feilkode og markerer tekst av seg selv.**
   - **Hva er galt:**
     - Når kortet er borte, er undertittelen «temp: NVML_ERROR_GPU_IS_LOST». Spesifikasjonen (kap. 5) sier at feilkoder bare skal stå i verktøytipset.
     - Verdien «agentic-riggd 1.0.0 · bygg 16» er markert med blått når popoveren åpnes, fordi den valgbare etiketten får fokus.
     - PCIe skrives «Gen 1 x16 (maks gen 3 x16)», men «x16 Gen 1 · maks Gen 3» på Oversikt.
   - **Hvor:** `rigg/src/sidebar.rs:565-583` og `widgets/rows.rs:32`.
   - **Bilder:** `runde3c/2b-borte/tilstand-borte-kortvelger.png` og `runde3c/2b-to-kort/to-kort-kortvelger.png`.
   - **Retting:**
     - Undertittel «Svarer ikke siden 18:03», med koden i verktøytipset.
     - `set_selectable(false)`, eller flytt fokus til lukkeknappen.
     - Samme PCIe-format som Oversikt.

4. **Bør · Tastatursnarveier: bunnlenken ligger over siste rad.**
   - **Hva er galt:** «Versjon og feilsøking står under Innstillinger › Om.» dekker «Vanlig skriftstørrelse». Ctrl+W og Ctrl+Q ligger under kanten, og ingenting viser at lista kan rulles.
   - **Hvor:** `rigg/src/shortcuts.rs:171` (480 × 640) og `:229`.
   - **Bilde:** `runde3c/grunnmur/tastatursnarveier-mork.png`.
   - **Retting:** Gjør dialogen høy nok til alle radene (ca. 760 px), eller gi bunnfeltet egen bakgrunn og en skillelinje over.

5. **Kan · Samme verdi har ulike navn og formater.** Det meste er arvet fra demoen.
   - **Klokka:** «Kjerneklokke» på Oversikt, Grafer og Kontroller, «Grafikklokke» på Sensorer, «GPU-klokke» i statusblokken og «Grafikk» i Klokker.
   - **Viftestyring:** «Auto · driveren styrer» (Oversikt, Viftekurve), «Driveren styrer (automatisk)» (Grafikkort) og «Auto · 35 %» (Kontroller).
   - **VRAM:** «1,0 GiB» (Oversikt), «1 031 MiB» (Sensorer), «8 192 MiB» (Grafikkort) og «8 GB» (Spesifikasjon).
   - **Retting:** Lag én navneliste i `rigg-core/sensormeta.rs` og bruk den overalt. Se produktspørsmål 6.

6. **Kan · Tidsaksen skriver «−45s» uten mellomrom.**
   - **Hva er galt:** Oversikt og Kontroller skriver «−45s», mens Grafer skriver «−4 min» med mellomrom.
   - **Bilde:** `runde3c/3c/oversikt-grafitt-lys.png`.
   - **Retting:** Smalt mellomrom (U+202F), som ellers i appen: «−45 s».

### Oversikt

7. **Må · Flistitlene kuttes ved 640 px.**
   - **Hva er galt:** «Tempera…», «Eff… av …», «Kjernekl…», «VR… av …» og «Vifte 2 vi…». Bekreftet.
   - **Hvor:** `rigg/src/pages/oversikt.rs:109-110` (`hbox` med `set_homogeneous(true)` og seks fliser uansett bredde). Demoen bytter til 3 kolonner under 900 px og 2 under 640 px (`web/styles.css:5220-5268`).
   - **Bilder:** `runde3c/3c/oversikt-640-lys.png` og `runde4a/oversikt-640-fra-start-lys.png`.
   - **Retting:** Legg flisene i et rutenett med 6, 3 eller 2 kolonner etter bredden, med samme bruddpunkter og skillelinjer som demoen.

8. **Bør · Temperaturstanga er tom i tomgang.**
   - **Hva er galt:** Skalaen begynner på 30 °C. Med kortet på 30 °C, som er tomgang her, er stanga helt tom og ser ødelagt ut. Ved 31 °C vises bare en prikk.
   - **Hvor:** `oversikt.rs:562`. Demoen har det samme.
   - **Bilder:** `runde3c/3c/oversikt-grafitt-lys.png` og `oversikt-grafitt-mork.png`.
   - **Retting:** Begynn skalaen på 20 °C, og rett verktøytipset.

9. **Bør · Grafene sier «Samler data …» når kortet er borte.**
   - **Hva er galt:**
     - Når kortet ikke svarer, viser grafene «Samler data …» foran dataene, og ingenting i hullet etter at dataene stopper.
     - Raden PCIe på Oversikt står igjen med «x16 Gen 3», mens resten er «–».
   - **Hvor:** `rigg/src/widgets/chart.rs:1025-1040` (`HoleKind`).
   - **Bilder:** `runde3c/3a-borte/borte-oversikt-lys.png` og `borte-kontroller-lys.png`.
   - **Retting:**
     - Hullet til høyre skal si «Kortet svarer ikke siden 18:03».
     - PCIe skal vise «–», som de andre radene.

10. **Bør · Uten driver viser Oversikt et tomt GPU-oppsett.**
    - **Hva er galt:** Med `no_driver` viser Oversikt hele GPU-oppsettet med «–» og tre grafer med «Samler data …» som aldri fylles. Spesifikasjonen (3.8) sier at GPU-sidene skal ha en tom tilstand. Grafikkort har det, Oversikt har det ikke.
    - **Bilde:** `runde3c/2b-ingen-driver/tilstand-ingen-driver-oversikt.png`.
    - **Retting:** Bytt flisene, panelene og grafene for GPU ut med samme tomme tilstand som på Grafikkort. Se produktspørsmål 4.

### Sensorer

11. **Kan · Swap har to desimaler i MiB.**
    - **Hva er galt:** «Swap brukt 123,50 MiB». Det er for presist.
    - **Bilde:** `runde3c/3c/sensorer-grafitt-lys.png`.
    - **Retting:** 0 desimaler for MiB og 2 for GiB.

12. **Kan · Nett-raden kan leses som «nede».**
    - **Hva er galt:** «Ethernet (enp24s0) · ned» med «ikke tilkoblet» ved siden av leses som «nede / ikke tilkoblet», men «ned» betyr nedlasting. Demoen gjør det samme.
    - **Hvor:** `sensorer.rs:629` og `short_name` (`net`).
    - **Retting:** «mottatt» og «sendt», eller ↓ og ↑.

### Grafer

Ingen egne funn utover punkt 6 og 9. Toppen i Kjerneklokke ved −1 min i `runde3c/3c/grafer-*` er ekte: det er den hodeløse Shell som vekker kortet når økta starter (se under «Ytelse og målinger»).

### Grafikkort

13. **Kan · «Skjerm tilkoblet: Nei» kan være misvisende.**
    - **Hva er galt:** Verdien kommer fra NVML `displayActive`. Den er «Disabled» på Wayland med den åpne kjernemodulen, også når en skjerm er koblet til. Nå sier `/sys/class/drm/card1-*/status` også «disconnected» for alle utgangene, så skjermen kan ha vært av. Det er ikke avklart.
    - **Hvor:** `rigg-core/src/report/gpu.rs:224`.
    - **Retting:** Kall raden «Skjerm aktiv (driverens flagg)», eller les utgangene i `/sys/class/drm`.

14. **Kan · Revisjon «0x0XA1» i bildene fra testbackenden.**
    - **Hva er galt:** Den simulerte backenden gir `"0xa1"`, og `gpu.rs:192` legger til «0x» foran.
    - **Hvor:** `rigg-hw/src/gpu/sim.rs:124`.
    - **Bilder:** `runde3c/2b-driver-oppdatert/*` og `runde3c/2b-dvale/*`.
    - **Retting:** `"a1"` i sim, eller fjern et «0x» som allerede står der.

### System

15. **Bør · Minnet summerer ikke.**
    - **Hva er galt:** Brukt 16,4 GiB og Buffer og cache 31,4 GiB er til sammen mer enn Totalt 47,0 GiB, før den ledige delen er regnet med. Delt minne (Shmem, nå 4,4 GiB) telles både i «brukt» (totalt minus tilgjengelig) og i cachen. Spesifikasjonen (3.9) sier at «buffer og cache» skal skille ut Shmem. I dag står det bare i verktøytipset.
    - **Hvor:** `rigg-core/src/report/system.rs:235-241`.
    - **Bilde:** `runde3c/3c/system-grafitt-lys.png`.
    - **Retting:** Cache = Buffers + Cached + SReclaimable − Shmem. Ellers kan du bruke modellen til `free`, der brukt, cache og ledig blir totalt.

16. **Bør · Klokkeområdet stemmer ikke med målt klokke.**
    - **Hva er galt:** «Klokke (cpufreq) 2 200–3 700 MHz» og «Boost: På», mens «Klokke og last per tråd» viser 4 336 MHz. En CPU-Z-bruker ser at tallene ikke stemmer.
    - **Bilde:** `runde3c/2b-kort/system-lys-nederst.png`.
    - **Retting:** «2 200–3 700 MHz, boost til 4,35 GHz (målt)». Høyeste målte trådklokke ligger allerede i tjenesten. Alternativt: kall raden «Nominell klokke».

17. **Kan · To uvante uttrykk på System.**
    - **Hva er galt:**
      - «Kjernebygg … dynamisk forkjøpping» er en uvant oversettelse av PREEMPT_DYNAMIC.
      - «Familie · modell · rev. 23 · 8 · 2» er desimalt. CPU-Z-brukere kjenner 17h · 8 · 2.
    - **Retting:**
      - «PREEMPT_DYNAMIC», med forklaring i verktøytipset.
      - «17h (23) · 8 · 2».

18. **Kan · Instruksjonssettet blander store og små bokstaver og to farger uten forklaring.**
    - **Hva er galt:** Blå brikker med store bokstaver og grå med små («fpu», «vme»).
    - **Bilde:** `runde3c/2b-kort/system-lys-nederst.png`.
    - **Retting:** Store bokstaver overalt, og en liten forklaring av fargene i panelhodet («uthevet = viktige for spill og koding»).

### Kontroller

19. **Må · «standard»-merket står midt på sporet, ikke ved standardverdien.**
    - **Hva er galt:**
      - Effektgrense: 245 av 105–275 W, men «standard 245 W» står midt på.
      - Minneklokke-offset: 0 av −1000…+3000, men «standard 0» står ved +1000.
      - Temperaturmål: 83 av 65–88 °C, men «standard 83 °C» står ved 76.
      - Bare kjerneklokke-offset (symmetrisk) blir riktig.
    - **Hvor:** `rigg/src/widgets/locked.rs:491-499` (`CenterBox`).
    - **Bilder:** `runde3c/3c/kontroller-grafitt-lys.png` og `runde4a/kontroller-640-fra-start-lys.png`.
    - **Retting:** Plasser merket og et lite strek-merke ved standardverdien (`Fixed` eller overlegg). Flytt merket inn hvis det havner for nær kantene.

20. **Må · «Lest fra kortet» vises mens kortet er borte.**
    - **Hva er galt:** Bekreftet:
      - Merket «Lest fra kortet» står der, og banneret sier «Verdiene under er lest fra kortet akkurat nå», mens alle radene viser «–» og den røde linja sier at kortet ikke svarer.
      - «Lag utkast» er fortsatt aktiv.
      - Spesifikasjonen kap. 5 sier at det som er lest fra kortet, ikke merkes, så merket bryter med den også når kortet er der.
    - **Hvor:** `rigg/src/pages/kontroller.rs:274` (banneret), `:288` og `:702` (merket).
    - **Bilde:** `runde3c/3a-borte/borte-kontroller-lys.png`.
    - **Retting:**
      - Fjern merket, eller vis det bare når kortet svarer (se produktspørsmål 3).
      - Banneret får en egen tekst for `lost`: «Kortet svarer ikke. Verdiene vises igjen når det er tilbake.»
      - «Lag utkast» kan stå, men verdiene i utkastet hentes fra siste lesing.

21. **Bør · Det mørke båndet på klokke-offset-sporene er uforklart.**
    - **Hva er galt:** På begge offset-radene ligger et mørkere stykke av sporet (tjenestens strengere policy) uten etikett eller forklaring. Det ser ut som en valgt verdi.
    - **Hvor:** `locked.rs:362`.
    - **Retting:** Verktøytips og en liten forklaring under raden: «Mørkt felt: tillatt når skriving blir slått på (−200…+150 MHz)».

22. **Bør · «o/min» brytes over to linjer.**
    - **Hva er galt:** I beskrivelsen av Vifte står «… vifte 2 35 % · 801 o/» og «min.» på hver sin linje. Pango bryter etter «/».
    - **Hvor:** `rigg-core/src/fmt.rs:149` (`rpm`) og `with_unit`.
    - **Bilde:** `runde3c/3c/kontroller-grafitt-lys.png`.
    - **Retting:** «o\u{2060}/\u{2060}min» (ordskjøt rundt skråstreken), eller bytt hele beskrivelsen til ikke-brytende mellomrom mellom tall og enhet.

23. **Bør · Begrensninger og Profiler ser uferdige ut.**
    - **Hva er galt:**
      - Brikken «Tomgang» er en bred, venstrejustert ramme som ser ut som et tomt tekstfelt.
      - Teksten «I P8 spør tjenesten ikke om begrensningene (kallet koster 25 ms og vekker kortet). Kortet er i tomgang.» er utviklerspråk.
      - Merket «0 av 5» i Profiler strekkes til full høyde i panelhodet.
    - **Hvor:** `kontroller.rs:887`.
    - **Bilde:** `runde3c/3c/kontroller-grafitt-lys.png`.
    - **Retting:**
      - Samme brikke som på Sensorer («P8», «Tomgang»).
      - Teksten «Kortet er i tomgang (P8). Årsakene leses når kortet jobber.», med 25 ms-forklaringen i verktøytipset.
      - `valign(Center)` på merket.

24. **Kan · Minneklokke-offset er uklar.**
    - **Hva er galt:** Beskrivelsen «Effektiv minnefrekvens. NVML bruker dobbel verdi.» er uklar. «−1 000» mangler enhet, mens «+1 000 MHz» har den.
    - **Retting:** «Oppgitt i effektiv MHz, som i Afterburner (+1 000 her er +500 i NVML).» og MHz på begge ender, eller på ingen.

### Viftekurve

25. **Må · «2 målinger» og «Målinger 3».**
    - **Hva er galt:** Bekreftet: «2 målinger» øverst og «Målinger 3» i Siste døgn, samtidig. Det er to forskjellige `get_history`-spørringer (48 t / 2 000 punkter og 24 t / 1 440 punkter), og begge teller nedsamplede punkter, ikke målinger.
    - **Hvor:** `rigg/src/pages/viftekurve.rs:886-905` og `:917`.
    - **Bilder:** `runde3c/3c/viftekurve-grafitt-lys.png` og `viftekurve-sand-mork.png`.
    - **Retting:** Tell ekte prøver, eller vis dekningen som tid («Data for 3 min» / «Data for 31 t av 48 t»). Bruk samme kilde begge steder.

26. **Bør · «Siste døgn» lover mer enn dataene holder.**
    - **Hva er galt:** «Viften har gått på 35 % i 100 % av tiden siste døgn» står med 3 min data.
    - **Hvor:** `viftekurve.rs:921`.
    - **Retting:** Med mindre enn 24 t data: «siden 18:11 (3 min)» i stedet for «siste døgn».

27. **Bør · Aksetekstene kolliderer i hjørnet.**
    - **Hva er galt:** «0 %» på y-aksen og «20 °C» på x-aksen ligger oppå hverandre nede til venstre.
    - **Bilde:** `runde3c/3c/viftekurve-grafitt-lys.png` (alle temaene).
    - **Retting:** Flytt «0 %» opp en halv linje, eller la x-aksen begynne til høyre for y-etikettene.

28. **Bør · Kortet borte vises som «Leses ikke ennå».**
    - **Hva er galt:** Modus og Mål-fart sier «Leses ikke ennå» når kortet er borte. Det er ikke «ennå».
    - **Hvor:** `viftekurve.rs:951` og `:957`.
    - **Bilde:** `runde3c/3a-borte/borte-viftekurve-lys.png`.
    - **Retting:** «–», med «Kortet svarer ikke» i verktøytipset, når tilstanden er `lost`.

29. **Kan · «Mål-fart 35 % / 35 %» sier ikke hvilken vifte som er hvilken.**
    - **Retting:** «35 % · 35 % (vifte 1 · 2)», eller én rad per vifte, slik Vifte 1 og Vifte 2 står over.

### Lys

30. **Bør · 3D som standard koster mye** (se produktspørsmål 1).
    - **Hva er galt:** «Auto» viser 3D, og det koster:
      - kortet går til P0 i ca. 4 s første gang (målt, se under),
      - vinduet bruker 136 MB mot 13–15 MB uten 3D, og det gis ikke tilbake før vinduet lukkes,
      - 25–36 % av én kjerne mens det animerer, og 15–19 % GPU-last.
    - **Hvor:** `rigg/src/pages/lys.rs:672`.
    - **Anbefaling:** «Auto» bør vise 2D når tegneren er cairo, og 3D-knappen husker valget («Vis i 3D»).

31. **Bør (sjekk) · Lysdiodene er slukket når kortet er borte.**
    - **Hva er galt:** Med kortet borte er lysdiodene i 3D-forhåndsvisningen slukket, selv om effekten er Statisk. Forhåndsvisningen skal ikke avhenge av kortet.
    - **Bilde:** `runde3c/3a-borte/borte-lys-lys.png`. Sammenlign med `3c/lys-grafitt-lys.png`.
    - **Retting:** Finn årsaken i `lys.rs` og `lys3d/`. Hvis det skyldes at lysstyrken eller tempen fra prøven er `None`, bruk en standardverdi.

32. **Kan · Øye-ikonet betyr to ting.**
    - **Hva er galt:** I panelhodet betyr det «bare forhåndsvisning» for hele panelet. På Regnbue, Fargesyklus og Temperatur betyr det «ikke dokumentert for kontrolleren».
    - **Hvor:** `lys.rs:472` og `:497`.
    - **Retting:** Et annet ikon for «ikke dokumentert», for eksempel `circle-help`.

### Innstillinger

33. **Må · «Stopp tjenesten nå» mangler.**
    - **Hva er galt:** Spesifikasjonen 3.1 (MÅ) og 3.10 krever valget i Generelt › Bakgrunn, med tekst om hva som stoppes. 3B laget det ikke («Avslutt bakgrunnstjenesten» finnes bare i systemfeltet, og bare når vinduet er lukket).
    - **Bilde:** `runde3c/3c/innstillinger-generelt-lys.png`.
    - **Retting:** En knapp i gruppa Bakgrunn med teksten «Stopper opptak og logging. Vinduet starter tjenesten igjen neste gang det åpnes.» og angre i 5 s. Logger tjenesten, skal teksten nevne loggen.

34. **Må · Lisensene finnes ikke etter installasjon.**
    - **Hva er galt:** Om sier «Ikoner: Lucide (ISC). Lisensene står i docs/LISENSER.md.» Men `install.sh` installerer ikke `docs/LISENSER.md`. Den legger bare inn `app/README.md` hvis den finnes, og den gjør ikke det. ISC-lisensen krever at merknaden følger med, og spesifikasjonen 3.10 krever «Lisenser» i Om.
    - **Hvor:** `rigg/src/settings/om.rs:147` og `app/install.sh:127-129`.
    - **Retting:** Legg lisensteksten i GResource og vis den i en egen dialog fra Om («Lisenser …»). Installer også en kopi til `~/.local/share/doc/agentic-rigg/`.

35. **Bør · Brukerflaten nevner «demoen».**
    - **Hva er galt:** «Lagrer innstillingene som JSON, i samme format som demoen.» og «Leser en eksportert fil, også fra demoen.»
    - **Hvor:** `rigg/src/settings/data.rs:208-209`.
    - **Bilde:** `runde3c/grunnmur/innstillinger-data-varsel-mork.png`.
    - **Retting:** «… også fra nettversjonen (0.0.x)», eller bare «Leser en eksportert fil.»

36. **Kan · Stiene kuttes midt i.**
    - **Hva er galt:** «/home/jul…-rigg-dev».
    - **Retting:** Vis `~/.config/agentic-rigg/` og `~/.local/state/agentic-rigg/` med `~`, som spesifikasjonen sier. Da får de plass.

37. **Kan · Versjonen skrives på to måter i Om.**
    - **Hva er galt:** «Versjon 1.0.0 · bygg 16» øverst og «Agentic Rigg 1.0.0 (bygg 16)» i Feilsøking.
    - **Retting:** Samme form begge steder.

38. **Kan · «Takt i bakgrunnen» virker aktiv når bakgrunnen er av.**
    - **Hva er galt:** Valget kan trykkes på når «Fortsett i bakgrunnen» er av. Da betyr det bare noe mens en logg går.
    - **Retting:** Demp raden og si «Gjelder når «Fortsett i bakgrunnen» er på eller en logg går.»

39. **Kan · To BØR-valg fra spesifikasjonen mangler i Utseende.**
    - **Hva er galt:** Aksent «System» og høy kontrast «Automatisk» (3.10, BØR) finnes ikke. De blokkerer ikke 1.0.0.

### Systemfeltet

40. **Bør · Menyen og verktøytipset avviker fra spesifikasjonen** (se produktspørsmål 2).
    - **Hva er galt:** Bekreftet:

      | | Spesifikasjonen (3.1) | Laget |
      |---|---|---|
      | Meny | «Åpne Agentic Rigg», «Start/stopp logging», «Stopp bakgrunnsopptak» | «Åpne Agentic Rigg», «Stopp logging» (bare mens loggen går), «Avslutt bakgrunnstjenesten» |
      | Verktøytips | GPU-temperatur og effekt | «GPU 52 °C · 8 % last» |

    - **Hvor:** `riggd/src/tray.rs:6-7` og `:157-165`. 3B meldte avviket selv (`LOGG.md`, 3B ønske 4).
    - **Retting:** Venter på svaret på produktspørsmål 2. Uansett bør effekten med: «GPU 52 °C · 12 W · 8 % last».

### Feiltilstander ellers

41. **Kan (sjekk) · «Driveren er oppdatert» viser levende tall i sidefeltet.**
    - **Hva er galt:** Sidefeltet viser 49 °C og 79 W mens banneret sier at kortet ikke kan leses før omstart. Det kan være en rest fra testbackenden. Med `driver_updated` bør GPU-verdiene vise «–» eller siste kjente verdi i dempet farge.
    - **Bilde:** `runde3c/2b-driver-oppdatert/tilstand-driver-oppdatert-grafikkort.png`.

### Ytelse og målinger

42. **Bør · Påstanden om P8 i 3C stemmer ikke helt.**
    - **Hva er galt:** `maalinger.md` (3C) sier «P8 i alle prøvene … også Lys i 3D». Det er riktig etter de første sekundene, men første gang 3D bygges, går kortet til P0 i ca. 4 s.
    - **Retting:** Rett teksten i `maalinger.md` og `LOGG.md` (tall under).

**P0/P8, målt nå.**

- **Slik er det målt:** `nvidia-smi --query-gpu=pstate,clocks.gr,power.draw,utilization.gpu` hvert 2. s (bare lesing), utenfor økta. Release-byggene kjørte i `okt.sh` uten noen skjermbildeplan:
  - tjenesten alene i 15 s,
  - vinduet i 60 s (`--side oversikt` i én kjøring, `--side lys` i den andre),
  - tjenesten alene i 15 s til.
- **Rådata:** `runde4a/pstate-oversikt.txt` og `runde4a/pstate-lys.txt`.

| Fase | P-tilstand | Klokke | Effekt | GPU-last |
|---|---|---|---|---|
| Før økta | P8 | 300 MHz | 12 W | 0–3 % |
| **Den hodeløse Shell starter** | **P0 i ca. 4 s** | 1 515 MHz | 17–35 W | 0–2 % |
| Tjenesten starter (NVML) | P8 | 420 → 300 MHz | 12 W | 0 % |
| Vinduet på Oversikt, 60 s (30 prøver) | **P8 i alle** | 300 MHz | 12–13 W | 0–4 % |
| **Vinduet åpner Lys (3D bygges første gang)** | **P0 i ca. 4 s (2 prøver)** | 1 515 MHz | 36–39 W | 0–2 % |
| Lys i 3D animerer, resten av minuttet | P8 | 315 MHz | 14 W | **15–19 %** |
| Vinduet lukket | P8 | 300 MHz | 12 W | 0 % |

**Konklusjon:**

- Skjermbildene tas med cairo-tegneren (`skjermbilder.rs:543`, `render_texture`), og det vekker ikke kortet.
- Det er **appen** som vekker kortet: første gang Lys lager GL-konteksten og laster opp 3D-kortet.
- I serien `3c` er Lys siste side i første runde (grafitt, lys). Neste bilde, `oversikt-grafitt-mork.png`, ble tatt 1–3 s etterpå, mens kortet fortsatt var i P0 («P0 · ingen begrensning», 1 515 MHz, 36 W og en topp helt til høyre i grafene). Alle de andre Oversikt-bildene viser P8, fordi GL-ressursene da allerede fantes.
- Toppen ved −1 min i Grafer-bildene er Shell-oppstarten, ikke appen.
- 3C målte P-tilstanden 10 s etter at siden ble åpnet, og fikk derfor ikke med seg dette.
- Et vanlig vindu (Oversikt) vekker ikke kortet.

### Tastatur og tilgjengelighet

43. **Bør · En runde med bare tastatur i 4B.**
    - **Hva er gjort:** Koden har en handler for hver tast i snarveisdialogen (`shortcuts.rs`). 85 knapper med bare ikon har tilgjengelig navn (`Property::Label`), og temaet har 18 regler for fokus.
    - **Hva gjenstår:** Jeg har ikke kunnet trykke Tab gjennom sidene, så fokusrekkefølgen på Kontroller (glidere i utkastet), Esc i filteret og popoverne, og fokusringen med høy kontrast er ikke prøvd.
    - **Forslag:** En plan i `skjermbilder.rs` med `Step::Keys` som tar bilde av fokusringen etter 1, 5 og 10 trykk på Tab på hver side.

---

## Produktspørsmål til brukeren

Dette kan ikke 4B bestemme.

1. **Skal Lys «auto» vise 2D?**
   - **Hvorfor:** 3D som standard koster mye, mens 2D bruker lite (tallene fra 3C):

     | | 3D | 2D |
     |---|---|---|
     | Vinduets minne | 136 MB, gis ikke tilbake før vinduet lukkes | 13–15 MB |
     | CPU mens det animerer | 25–36 % av én kjerne | 19 % av én kjerne |
     | Kortet | P0 i ca. 4 s første gang, 15–19 % GPU-last | ikke målt |

   - **Anbefaling:** Ja. «Auto» viser 2D med cairo, og 3D lastes først når du trykker 3D. Valget huskes. Du ser 3D-kortet med ett klikk, og de som aldri trykker, betaler ingenting.

2. **Hva skal menyen i systemfeltet si?**
   - **Hvorfor:** Valget er mellom spesifikasjonen og det 3B laget (punkt 40):
     - Spesifikasjonen: «Start/stopp logging» og «Stopp bakgrunnsopptak».
     - 3B: «Stopp logging» bare mens loggen går, og «Avslutt bakgrunnstjenesten».
     - Forskjellen er at «Stopp bakgrunnsopptak» slår av valget, mens «Avslutt» bare stopper tjenesten til neste gang.
   - **Anbefaling:**
     - «Åpne Agentic Rigg».
     - «Start logging» / «Stopp logging», alltid synlig.
     - «Stopp bakgrunnsopptak», som slår av «Fortsett i bakgrunnen» og stopper tjenesten hvis ingen logg går.
     - Verktøytips med temperatur, effekt og last.

3. **Skal merket «Lest fra kortet» på Kontroller fjernes helt?**
   - **Hvorfor:** Spesifikasjonen kap. 5 sier at det som er lest fra kortet, ikke merkes. Kap. 6.2 bruker derimot «Låst · lest fra kortet» som sammenlagt banner.
   - **Anbefaling:** Fjern merket. Banneret sier det allerede.

4. **Hva skal Oversikt vise uten NVIDIA-driver?**
   - **Valgene:**
     - Hele siden blir en tom tilstand.
     - GPU-delen blir en tom tilstand, og CPU og minne vises i grafene.
   - **Anbefaling:** Det andre. Da er siden fortsatt nyttig.

5. **Standardene fra spesifikasjonen kap. 10 er fortsatt ubesvart.** Svarene bestemmer hva som installeres:
   - «Fortsett i bakgrunnen» av eller på (anbefalt på, 2 s),
   - «Start tjenesten ved innlogging»,
   - om du vil ha systemfeltet,
   - skrivebordsvarsler som standard.

6. **Navnet på GPU-klokka.**
   - **Valgene:** Demoen bruker «Kjerneklokke», «Grafikklokke», «GPU-klokke» og «Grafikk» om samme verdi. Skal appen bruke ett navn overalt, og avvike fra fasiten?
   - **Anbefaling:** «Kjerneklokke», som Afterburner («Core clock»), og «Minneklokke».
