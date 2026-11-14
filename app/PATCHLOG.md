# Patchlog

## 1.4.0 · bygg 23 – 14. november 2026: kortbundet GPU-styring og brukerflate

- **Én aktivering:** Innstillinger › Skjermkortstyring bruker én administratorgodkjenning for valgt GPU. De fire separate godkjenningene er fjernet. Eldre `helper.json` v1 blir ikke automatisk full autorisasjon; brukeren må aktivere det nye oppsettet. Installasjonen under `~/.local` aktiverer fortsatt ingenting.
- **Faktiske funksjoner per felt:** effektgrense, kjerneoffset, minneoffset, manuell vifte, viftekurve og temperaturmål rapporterer egen tilgjengelighet, område, grunn og mulighet for tilbakelesing. Manglende temperaturmål eller minneoffset sperrer ikke fungerende effektstyring. Oppdagelse bruker gettere og tilgjengelige setter-symboler uten å prøve en endring; driveren kan fortsatt avvise selve skrivingen.
- **Riktig kort og riktige verdier:** hjelperen bindes til PCI-adresse og UUID og åpner aldri ubundet GPU-indeks 0. Skriveoperasjoner krever samme identitet i vindu, tjeneste og hjelper. Bare endrede felt sendes; grensene kommer fra den aktuelle backenden fremfor en fast RTX 2080-policy. `get_state` og skrivesvar gir flate aktive og fangede grunnverdier til brukerflaten.
- **Fanget utgangspunkt:** kortets innstillinger fanges og lagres atomisk i en beskyttet grunnverdifil før skriving. Opptaket binds også til driver og VBIOS og beholdes ved vanlig oppstart/recovery. Ren ny administratoraktivering arkiverer det gamle først etter bekreftet gjenoppretting og uten dirty-markør, så dagens utgangspunkt fanges på nytt. «Standard» og stopp gjenoppretter dette opptaket, også opprinnelige effektgrenser og klokkeoffset. Nytt opptak krever Auto-vifte; et eksisterende Auto-opptak kan brukes til recovery mens våre manuelle verdier er aktive.
- **Ærlige feil:** delvis mislykket endring forsøkes gjenopprettet til bekreftet oppsett og deretter fanget utgangspunkt. En dirty-markør lagres før første endring. Den fjernes bare når grunnverdiene er bekreftet; ukjent eller mislykket tilbakelesing gir feilstilstand og sperrer videre endring. Avinstallering av hjelperen og utskifting av en gammel hjelper avbrytes hvis gjenoppretting ikke kan bekreftes. Klokkegettere kan vise gamle offset; dette merkes som uverifisert, ikke som en fysisk måling.
- **Frist og viftesikkerhet:** Bruk/Behold, eierskap og tilbakerulling er beholdt. Frakobling, manglende livssignal, hvilegap og stopp utløser gjenoppretting etter egne endringer. Nødvifte krever faktisk viftestøtte; temperaturbortfall under Manuell går til gjenoppretting. Feilet tilbakerulling meldes som feil, ikke som vellykket rollback.
- **Hjelpertjenesten:** root-eid oppsett v2, privat uid-kontrollert socket, beskyttet tilstandsmappe, CAP_SYS_ADMIN for NVML og CAP_CHOWN for socketen. Enhetsadgang avgrenses til NVIDIA-kontrollnoden og det valgte kortets `/dev/nvidiaN`. Klarstatus krever en tilgjengelig backend; en lyttende socket alene er utilstrekkelig. Ingen I2C-tilgang eller generelle kommandoer i protokollen.
- **RGB:** lyskontrolleren er fortsatt ikke identifisert. Styringssiden kan lagre og vise lysvalg, men sender dem ikke til kortet, selv om NVML-styring ellers er klar.
- **PCIe-modell:** 82 kontaktflater per side (11 + 71) på x16-kanten, åpent nøkkelspor, faset innføringskant og delte kontaktmål for 2D, mesh og tekstur. Dette er en illustrasjon, ikke produsentens CAD eller en identifikasjon av kortvarianten.
- **Fokus og menyer:** sensorfilteret slipper fokus ved klikk utenfor og ved sideskifte; Esc tømmer/avslutter søket og tømming virker straks. Nedtrekksmenyer får sammenhengende farger for valgt rad, hover og tastaturfokus. Låst-/klarstatus følger valgt GPU og tjenestens oppdateringer.
- **Endelig brukerflatekontroll:** Den endelige native UI-planen bestod, inkludert klokkesiden med den nye statusvakten. Den oppdaterte installerte appen startet på Klokker og effekt; klar hjelper og samsvarende kortidentitet er bekreftet.
- **Kontrollstatus:** siste fullførte workspace-kjøring gir 388 bestått, 4 ignorert og 0 feilet; workspace-clippy på alle mål er ren. Den native GTK-planens assertions bestod, blant annet fokus/nedtrekksmenyer, én aktivering i testoppsettet, Bruk/Behold, status og PCIe 2D/3D. Egne sim-/mocktester dekker blant annet feil identitet, manglende støtte, delvis skriving, resetfeil og uverifisert gjenoppretting.
- **Aktivering uten hengende lås:** en eksisterende appforekomst håndterer nå `--innstillinger DEL`. Hjelperstatus poller uavhengig av klarstatus, slik at aktivering utenfor vinduet blir oppdaget. Bare meningsfulle statusendringer utløser ny layout. Watcher-testen og testisoleringen inngår i den fullførte workspace-kjøringen.
- **Fysisk prøve og installasjon:** Brukeren har fullført ekte polkit-aktivering, og begrenset skriving mot den herdede NVML-hjelperen er verifisert på RTX 2080: 245 → 240 W, begge vifter Auto (35 %) → Manuell (50 %), kjerneoffset 0 → −15 MHz og minneoffset 0 → −10 MHz. Hvert felt har fersk NVML-tilbakelesing og revert; avsluttende reset bekreftet 245 W, offset 0/0 MHz, temperaturmål 83 °C og Auto på begge vifter. En senere prøve verifiserte flat 50 %-viftekurve på begge vifter i 4 s og temperaturmål 83 → 82 °C, med revert/reset og bekreftet stock true ved avslutning. Installerte 1.4.0-binærer og uendrede innstillinger er kontrollert, og historikken er sikret. Gjennom installert NVML-daemon er kortbinding, apply 240 W + confirm/Behold, fersk 240 W og reset til 245 W verifisert. Ny apply 240 W med 10 s frist uten Behold ga rolled_back/timeout og fersk 245 W før noe ekstra reset-kall; avsluttende reset ga stock true. Alle grenser, positiv overklokking, stresstest, drivertap og strømbrudd gjenstår. Fysisk Wayland-pekerbruk er ikke verifisert i denne runden; de interne GTK capture-/pick-testene er en egen avgrensning. Ingen nye ytelsestall påstås for 1.4.0. Se `docs/produksjon/kritikk-1.4.0.md` for lukkede reviewfunn.

## 1.3.3 · bygg 22 – 14. november 2026: CPU-lanseringsdato og spesifikasjonsrevisjon

- Prosessor og rapport får lanseringsdato fra et lokalt modelloppslag med kilde. Ryzen 7 2700X viser 19. april 2018 fra AMD. Datoen bestemmes ikke av CPUID-familien og arves ikke av ukjente CPU-modeller.
- Komponentenes viste, skjulte og manglende opplysninger er kartlagt i `docs/produksjon/spesifikasjoner-revisjon-2026-11-14.md`. RAM-moduler, skjerm og nettverksdetaljer har størst behov for utvidelse. Øvrige funn er ikke implementert i denne versjonen.
- Ingen endring av maskinvaretilgang eller GPU-styring.

## 1.3.2 · bygg 21 – 14. november 2026: adaptive måleakser og gjennomgang

- Ferske ytterverdier tas med før midtpunktet i en åpen langtidsbøtte er synlig. Interpolerte linjer og bånd ved vinduets kanter tas med i akseberegningen. Tegningen bruker lineær verdiavbildning og klipper først på tegneflaten, slik at en topp som forlater vinduet ikke endrer kurvens helning.
- Viftekurvens målte punkter og «Nå»-punkt får plass også ved 110 °C, under 20 °C og vifteverdier over 100 %. Tegning, rutenett og peker bruker samme visningsskala. Redigerbare punkter beholder de lovlige grensene, og aksen fryses under dragging før ventende ytterverdier tas med etter slipp.
- Forklaringen til «Tilpass / Fast» sier at begge utvider området når verdiene går utenfor. Bare last og brukt minne i prosent har hard grense 0–100.
- Nye regresjonstester dekker 110 °C, kjerneklokke 2500 MHz mot oppgitt maks 2190, minneklokke 10000 MHz, CPU-klokke 6000 MHz, effekt 400 W, disk på 90 °C og høye disk-/nettrater, med «Tilpass» og «Fast».
- Kodegjennomgang og UI/UX-runde er dokumentert i `docs/produksjon/kritikk-1.3.2.md`. Åpne reviewfunn er skilt fra grafrettingene. Ingen installasjon eller endring av ekte maskinvare.

## 1.3.1 · bygg 20 – 13. november 2026: kortmeny, sidefelt og søkefokus

- Kortet under Skjermkortstyring åpner detaljene ved raden du trykker på. Menyen følger sidefeltets bredde, med kort navn, tydelige grupper og tekst som brytes ved behov. Driver/CUDA står side om side; VBIOS, PCIe, oppdatering og tjeneste har egne felt. Med flere kort er det valgte merket.
- Klikk på skjermkortet øverst under appnavnet åpner Grafikkort direkte, også i ikonlista. Pil og verktøytips følger handlingen.
- Ikonene i sammenlagt sidefelt er sentrert. Innstillinger, hjelp og utviding har samme bredde og plassering som navigasjonsknappene; maskinhodet og appikonet følger samme midtlinje.
- «Mer om kortet» og kortvalg lukker menyen før handlingen. Skjermbildeplanen `kortmeny` kontrollerer menybredde, stor skrift, lang drivertekst, temaer, kortvalg, navigasjon og sentrering av Innstillinger.
- Innstillinger: klikk utenfor søket slipper fokus og den blå rammen. Første Esc avslutter søket og beholder Innstillinger; neste Esc går tilbake. Kryssknappen oppdaterer treffene straks. Et raskt seksjonsbytte tømmer også tekst som ennå ikke er behandlet av søkeforsinkelsen, slik at gamle treff ikke dukker opp igjen. Ctrl+F, / og tastaturfokus er bevart.
- Ingen endring i maskinvaretilgang. Det ekte kortet er fortsatt sperret. Webdemoen har sin egen versjon og er uendret.

## 1.3.0 · bygg 19 – 9. november 2026: Oversikt over hele maskinen, grafer som ikke kutter

Etter brukerens ønsker: grafene skal stige naturlig når temperaturen går mot 80, 95 eller 110 °C, Oversikt skal vise hele systemet som GNOME Resources, Disker skal ikke ha tomrom, og Styring skal tydelig handle om skjermkortet. Runde 8A gjorde alt (`docs/produksjon/LOGG.md`). Det ekte kortet er fortsatt sperret: ingenting skrives dit i 1.3.0.

**Ingen graf kutter verdier**
- **Før:** «Tilpass» fulgte dataene, men innenfor et fast ytre område per graf (GPU- og CPU-temperatur 20–100 °C, disk 20–60 °C, effekt, klokker og VRAM med egne tak), og «Fast» viste bare det området. 110 °C ble en flat strek i toppen, og under 20 °C forsvant.
- **Nå** er området et *foretrukket* område, ikke en grense (`chart::math::Axis`, `y_axis`). Går dataene over eller under, utvides aksen til en pen skala med litt luft, både med «Tilpass» og «Fast». GPU-temperatur 40–110 °C gir 25–125 °C (Tilpass) eller 20–120 °C (Fast), 12–18 °C gir 5–30 °C (Tilpass) eller 0–100 °C (Fast), 330 W gir 0–400 W, og en disk på 65 °C får plass.
- **Hard grense** bare der verdien ikke kan gå over: last og minne i prosent (0–100). Viftene har ingen, fordi NVML kan melde over 100 %.
- **Aksen pumper ikke** (`AxisHold`): den utvides straks når kurven trenger plass, men krymper først når den har ønsket seg mindre i 15 s, og da til det største den har ønsket seg i den tiden. Et bilde uten data (historikken hentes på nytt) drar den ikke tilbake til hele området.
- **Grenselinjer:** nedklokking (97 °C) og avstenging (100 °C) står som stiplede streker med navn i GPU-temperaturgrafene når de er innenfor aksen, og tas med i aksen når kurven er nær. Effektgrensen står som før.
- **Seks merker** når det gir en tettere akse (8–117 °C blir 0–125 i steg på 25, ikke 0–150 i steg på 50).
- **Gjelder alle grafene:** Oversikt, Grafer, grafen under en rad på Sensorer, Disker, Klokker og effekt, og Vifter (også «Siste 5 minutter», der % og °C deler akse: den utvides i steg på 25 over 100 °C). Sparklinjene (statusblokken, flisene, Sensorer, Prosessor, Disker) holder 0–100 % som foretrukket område og går over når en vifte melder 105 %.
- **Aksetallene** har så mange desimaler som steget trenger: steg på 0,25 gir «0,25 0,50 0,75» (før «0,2 0,5 0,8»), og 2,5 gir «2,5 5,0 7,5» (før «3 5 8»).
- **Nettgrafen** har 0–1 MB/s (8 Mbit/s) som foretrukket område, før 0–1000 MB/s. Vanlig trafikk var da en strek ved null; nå synes den, og mer trafikk utvider aksen.
- Kurveredigereren på Vifter beholder 20–100 °C og 0–100 %: det er et redigeringsområde.

**Ny Oversikt over hele maskinen**
- **Seks fliser øverst:** Prosessor (last, temperatur, snittklokke), Skjermkort (last, temperatur, effekt av grensen, P-tilstand), Minne (brukt av totalt, GiB og %), VRAM, Disker (samlet MB/s, varmeste disk, SMART-helsa) og Nettverk (ned og opp). Hver med ikon, sparklinje for siste minutt og verktøytips. Klikk åpner Prosessor, Grafikkort, System, Disker eller Sensorer. Seks spalter, tre under 900 px og to under 560 px.
- **Tilstand-linja** står bare når noe er verdt å se, det alvorligste først: kortet svarer ikke, ny driver, ingen driver, nedklokking (termisk eller maskinvare, ikke effektgrensen under last), GPU- eller CPU-temperatur over varselgrensene eller ved nedklokkingsgrensen, minnet nesten fullt, en disk med advarsel eller feil. Klikk åpner siden for delen.
- **Grafene** (tre spalter på brede skjermer, to, og én under 700 px): Last (CPU og GPU), Temperaturer (GPU, CPU og de to varmeste diskene, med grenselinjene), GPU-effekt, Minne og VRAM (i %, GiB i verktøytipset), Disk-I/O og Nettverk. Tidsvalget i toppfeltet styrer alle.
- **Skjermkortet nederst:** Temperatur og effekt (målerne som før) og Skjermkort (P-tilstand og begrensning, kjerne- og minneklokke mot kortets maks, vifter med turtall, viftestyring, PCIe, video), med lenke til Grafikkort. Klokker-panelet og VRAM-raden er flyttet: de står på Grafikkort og i flisene.
- **Rolig i bruk:** grafene på Oversikt flytter seg med prøvene (hvert sekund, i samme bilde som flisene), ikke hver piksel mellom dem, og målerne og klokkestolpene hopper rett til ny verdi. Under last, når kortet står i P0 og alle tallene endrer seg hvert sekund, ga glidingen og den jevne rullingen 4–5 bilder i sekundet. Kortets tilstand står ikke lenger i en linje øverst på Oversikt og Styring-sidene, fordi tilstand-linja og «Styrer»-linja sier det der.
- **Uten driver** viser Skjermkort-flisa det, VRAM og skjermkortdelen skjules, og grafene viser prosessor, minne og disker. Uten UDisks2 står «helse ukjent», uten disktemperatur «uten temperatur».
- Ingen nye data fra tjenesten: alt fantes. Topp-prosesser er ikke tatt med (krever en ny lesing av `/proc/*/stat` hvert sekund; ikke billig nok for startsiden).

**Disker uten tomrom**
- Disk og SMART står side om side med like høye kort, og radene fordeler høyden i det laveste kortet. Partisjoner står i hele bredden under, så tabellen får plass. Under 720 px står alt under hverandre.

**Styring handler om skjermkortet**
- Seksjonen i sidefeltet heter **Skjermkortstyring**, og under tittelen står kortet som styres («RTX 2080», eller «Palit RTX 2080» når produsenten er kjent). Klikk åpner detaljene og kortvalget.
- Toppfeltet sier **Skjermkort › Klokker og effekt**, **Skjermkort › Vifter** og **Skjermkort › RGB-lys**, og det samme står i vindustittelen og Startside.
- **«Styrer»-linja** øverst på de tre sidene: GPU-ikon, navnet, produsenten, PCI-adressen, VBIOS og driveren, funnet av seg selv fra NVML og sysfs. Med flere kort står «Kort 1 av 2» med valget. Kortet borte, ingen driver eller ny driver: linja sier det, og at styringen er utilgjengelig.
- **Produsenten** leses fra PCI-subsystemet når kortprodusenten har satt sin egen ID (1569 Palit, 10b0 Gainward, 1462 MSI, 1043 ASUS og flere, `rigg_core::vendor`). NVIDIAs egen (10de, som på dette Palit-kortet) gir «NVIDIA-referansekort», og forklaringen sier hvorfor. Blyanten i linja setter «Produsent (valgfritt)», lagret per kort (UUID) i `ui.json` (`gpuMakers`), og vist i linja og maskinhodet.
- Panelhodene: «Kortets viftekurve», «Hvem styrer kortets vifter», «Kortets vifter», «Lyskontrolleren på kortet». Innstillinger › Skjermkortstyring, tastatursnarveiene og verktøytipsene i sidefeltet bruker de samme ordene.

**Ytelse**
- Grafene leser kortets faste data uten å kopiere dem (`App::with_statics`). Før kopierte grafene for klokker og effekt dem ved hver tegning.
- Stolpene og målerne melder verdien til skjermleseren i hele prosent, og bare når den endres.
- **Hjelperen:** avviste hjelperen tjenesten (feil bruker) og lukket forbindelsen før tjenesten rakk å skrive, ble grunnen «Broken pipe». Nå leser tjenesten avvisningen som ligger der, og grunnen er «Bare brukeren som aktiverte hjelperen, får koble til». Sett som en test som feilet under last.

**Sim og skjermbilder**
- Scenarioet `sim:hete`: under 20 °C, 95 °C og 110 °C med 300 W, prosessoren med, én disk på 65 °C og én på 14 °C.
- Planene `8a` (Oversikt i lyst og mørkt, 1920/1440/900/640 px, høy kontrast, Styrer-linja med og uten produsent) og `8a-hete` (aksene med Tilpass, Fast og °F på Oversikt, Grafer, Klokker og effekt, Vifter og Disker). `6b-tilstand` viser også Oversikt og Styrer-linja med to kort, kortet borte, ingen driver og ny driver.

## 1.2.0 · bygg 18 – 9. november 2026: bryteren for endring og Afterburner-vifter

Etter brukerens ønsker til 1.2.0 (`docs/produksjon/tilbakemelding/1.2.0.md`): en enkel måte å slå på og av det som trengs for å endre klokker, lys og vifter, og viftekurver like fleksible som i Afterburner. Runde 7A og 7B laget det nye, og 7C kritiserte og finpusset (`docs/produksjon/kritikk-1.2.0.md`, 28 funn). Det ekte kortet er fortsatt sperret: ingenting skrives dit i 1.2.0.

**Bryteren og hjelperen (7A)**
- **Innstillinger › Styring** (ny del): bryteren «Tillat endring av skjermkortet», status (Av, På, Feil, eller «Slå på igjen» etter en ny versjon), trinnlisten og «Hva bryteren gjør».
- **Root-hjelperen** `agentic-rigg-helper` (ny crate `rigg-helper`): fem faste operasjoner over en privat socket, bare for brukeren din. Den har grensesjekk mot kortets egne områder, frist med Behold eller tilbakerulling, nødregel over 85 °C og standardverdier når tjenesten eller hjelperen stopper. Ingen kommandoer, filstier eller I2C.
- **Bryteren** kjører `pkexec aktiver.sh` eller `deaktiver.sh`. Det er brukerens egen handling med passord. Skriptene legger inn og fjerner hjelperen, polkit-regelen og en herdet systemtjeneste.
- **Trinnene** godkjennes hvert for seg med et eget spørsmål og passordet: Effekt, Vifter, Klokker og RGB-lys. Alle er av ved aktivering, og mot det ekte kortet kan de ikke godkjennes i 1.2.0 («Kommer senere»).
- **Tjenesten** sender skrivingen videre til hjelperen når den er på (`get_helper`, `step_not_enabled`, `helper_off`, `backend_mismatch`). Vinduet snakker aldri med hjelperen.
- **Sidefeltet:** merket ved Styring viser tilstanden: «Bare visning», «Endring på · låst», «Endring på · effekt», «Endring: feil» eller «Endring: slå på igjen».
- **Det ekte kortet er sperret to ganger:** cargo-funksjonen `ekte-skriving` er av, og `EKTE_SKRIVING = false`. Hjelperen med det ekte kortet laster ikke engang NVML. Sikkerhetsgjennomgangen står i `docs/produksjon/hjelper-sikkerhet.md`.
- **`install.sh`** legger filene i `~/.local/share/agentic-rigg/hjelper/` og aktiverer ingenting. **`uninstall.sh`** fjerner dem og sier fra hvis hjelperen fortsatt er på.

**Vifter (7B)**
- **Viftemotoren** (`rigg_core::vifte`, ren logikk) er den samme i tjenesten (sim) og i hjelperen. Den har nødregel, utjevning, hysterese, kurve per vifte med stille flate, minste fart og rampe.
- **Felles kurve · Vifte 1 · Vifte 2:** egen kurve per vifte. Kurven til den andre viften står svakt bak.
- **Oppførsel:**
  - hysterese 0–10 °C, tegnet som et bånd i grafen
  - rampe opp og ned i %/s
  - utjevning (av, 3, 5 eller 10 s)
  - «Hold minimum til __ °C»

  Under hver står en forklaring i klartekst, for eksempel «Fra 35 % til 100 % på 7 s».
- **Viftestopp (0 %)** er ikke mulig på RTX 2080. Fastvaren holder minst 35 %, også i Afterburner, og appen sier det.
- **Siste 5 minutter:** motoren kjørt over historikken, ved siden av slik viftene faktisk gikk, med snitt, høyeste og antall endringer.
- **Forhåndsvalgene** Stille, Balansert og Ytelse setter både kurve og oppførsel.
- **Sim:** varmemodell og scenarioet `sim:spill`, så kurven, hysteresen og rampen synes i grafene.

**Kritikk og finpuss (7C)**
- **Klartekst:**
  - Låst-linja, toasten og «Utkast · låst» ber ikke lenger brukeren godkjenne et trinn som ikke kan godkjennes mot det ekte kortet.
  - Feilstatusen viser en forståelig melding, ikke en rå systemfeil. Det tekniske står i verktøytipset.
  - Teksten i Innstillinger › Styring er uten sjargong («administratorrettigheter», ikke «root-tilgang», «polkit» eller «systemtjenesten»), og «Hvem»-raden lover ikke mer vern enn det finnes.
  - «Endring» brukes overalt i stedet for «skriving».
- **«Hva skal til?»** viser bryteren og de samme fire trinnene som Innstillinger › Styring, med en knapp dit.
- **Hjelperen bruker viftemotoren som tjenesten:**
  - start fra farten viftene har (`med_start`)
  - nytt oppsett bare når det er endret
  - ett steg per runde, også under nødregelen, så viftene rampes ned etterpå
  - test: hjelperen og tjenestens sim-sløyfe gir samme fart hvert sekund i 8 minutter. Sim-sløyfen ligger nå i `rigg-hw/src/gpu/sim_vifter.rs`.
- **RGB-lys** sier at lys kommer i en senere versjon, ikke at I2C «ikke er slått på».
- **«Siste 5 minutter»** forklarer begge linjene med egne kurver per vifte. «Nødregel 100 % over 85 °C» står ikke lenger der kurven går.
- **Tre Kan-punkter fra 6D:**
  - «P8 / tomgang» står bare én gang på Klokker og effekt («Begrenset av: Ingenting · Kortet er i ro»).
  - Utkast-merket forsvinner når utkastet er likt kortet.
  - Modellnavnet på Disker kuttes ikke ved 640 px.
- **Innstillinger ved 640 px:** vindusknappene står i høyre kant.
- **Vifter ved 640 px:** flisene øverst står 3 + 3, ikke 5 + 1.
- **`--version`** har samme form for alle tre binærene: «Agentic Rigg 1.2.0 · bygg 18» og «Agentic Rigg-hjelperen 1.2.0 · bygg 18».
- **`bilder.sh --plan alle`** har planene for 7A og 7B, og standardmappa er `docs/skjermbilder/1.2.0/`.

## 1.1.0 · bygg 17 – 9. november 2026: Disker, Prosessor, nytt skall og ny Styring

Etter brukerens tilbakemelding på 1.0.0 (`docs/produksjon/tilbakemelding/1.1.0.md`). Runde 6A, 6B og 6C laget det nye, og 6D kritiserte og finpusset (`docs/produksjon/kritikk-1.1.0.md`, 29 funn).

**Nye sider (6A)**
- **Disker:** én rad per disk med helse (OK / Advarsel / Feil / Ukjent), temperatur, MB/s og aktivitet. Utfoldet: modell, serie, fastvare, o/min, SMART-helse med grunner, driftstid, skrevet og lest, levetid, partisjoner med ledig plass, MB/s-graf og hele SMART-tabellen. UDisks2 over D-Bus uten root; aldri selvtest, aldri `/dev/sd*`.
- **Prosessor:** Tdie/Tctl, last, klokker, governor og boost, klokke og last per tråd gruppert etter CCX og kjerne, cache, instruksjonssett og sårbarheter (det CPU-Z-delen på System var).
- **System** har hovedkort/BIOS, minne, OS og kontrollere (nett, lyd, USB, SATA, NVMe).
- Disktemperatur og -hastighet i Sensorer og Grafer, og varsel for diskhelse (av som standard).

**Skallet (6B)**
- **Vindusknappene** som i GNOME/libadwaita: usynlig treffområde, sirkel på 24 px, riktig avstand, og plassering etter `gtk-decoration-layout`.
- **Maskinhodet** øverst i sidefeltet: «RTX 2080 / 8 GB · driver 610.43.02» og «Ryzen 7 2700X / 8 kjerner · 16 tråder». Ingen grønn prikk; feil står i tekst.
- **Styring** med nye navn og forklaring: **Klokker og effekt**, **Vifter** og **RGB-lys**, og merket «Bare visning». `id` og `--side` er uendret.
- Tastene **1–9 og 0** for de ti sidene.
- **Code Pro** nederst i Innstillinger › Om.

**Styring redesignet (6C)**
- Store tall øverst (klokker, temperatur, effekt, last, vifte, P-tilstand og «Begrenset av»), glidere med standardmerke, tallfelt og «Tilbakestill» per rad, «Tilbakestill alt til standard», og (i) med hva hver kontroll gjør.
- Vifter: kurveredigerer med faste akser, minste fart og nødregel tegnet inn, driverens faktiske kurve i bakgrunnen, forhåndsvalg og tastatur.
- Ctrl+Z / Ctrl+Shift+Z i utkastet. Alt er fortsatt låst mot det ekte kortet.

**Kritikk og finpuss (6D)**
- **P0-toppene** 6C så etter ca. 1 min, kom fra andre agenters hodeløse Shell, ikke fra tjenesten (målt alene, `maalinger.md` runde 6D).
- **Statusblokken** gir diskene navn («sda temp», «nvme0n1 les»), med modellen i verktøytipset.
- **°F:** temperaturmålet og kurvepunktene vises og leses i °F når det er valgt.
- **Tastatursnarveier** har Styring-tastene og to spalter.
- **Grafhodene** kutter aldri tittelen: forklaringen og «min · maks» står helt eller ikke.
- **Vifter ved 640 px:** kurvenavnet og «Data for …» får plass.
- **RGB-lys** har samme låst-linje som de to andre Styring-sidene. «Låst» i toppfeltet vises ikke når Bruk er på (testdata).
- Sårbarhetene med riktige navn (L1TF, MDS, iTLB Multihit …), «lastsnitt» i stedet for «last», diskradene på Sensorer med kjernenavnet først, tall og enhet brytes ikke fra hverandre, og gamle sidenavn er borte fra tekstene og `--help`.
- To tester som kunne feile under høy last, venter nå på tilstand.
- `bilder.sh --plan alle` har planene for 6A, 6B og 6C.

## 1.0.0 · bygg 16 – 7. november 2026: runde 4B, finpuss før installasjon

Rettingene etter kritikken i `docs/produksjon/kritikk-1.0.0.md` (43 funn), brukerens svar på produktspørsmålene og en egen tastaturrunde. Nummeret i parentes er funnet i kritikken.

**Brukerens valg (spesifikasjonen kap. 10)**
- **Lys åpner med 2D-kortet.** Trykker du 3D, huskes valget til neste gang (`lys.picked` i `profiler.json`). Ny innstilling Utseende › Lys › «3D-kortet: Auto / Alltid 3D / Alltid 2D» (`lys3d`, standard Auto = 2D til du har valgt 3D én gang). Første oppbygging av 3D vekker kortet (P0 i ca. 4 s) og koster ca. 120 MB i vinduet, så det skjer bare når du ber om det (30).
- **«Fortsett i bakgrunnen» er av som standard** og står nå øverst i Innstillinger › Generelt. Forklaringen sier med ord hva av og på betyr, med de målte tallene (21–28 MB, 0,85 % med 2 s).
- **Start ved innlogging** er av, og **skrivebordsvarsler** er av som standard. Begge kan slås på.
- **Systemfeltet** (40): menyen har «Åpne Agentic Rigg», «Start logging» eller «Stopp logging» (alltid synlig) og «Stopp bakgrunnsopptak», som slår av valget og stopper tjenesten hvis ingen logg går. Verktøytipset har temperatur, effekt og last: «GPU 52 °C · 12 W · 8 % last».
- **«Kjerneklokke» overalt** (5): Sensorer («Grafikklokke»), statusblokken («GPU-klokke»), CSV-kolonnen og klokketabellene («Grafikk» → «Kjerne»). Minneklokka heter «Minneklokke».
- **Merket «Lest fra kortet» er fjernet** fra Kontroller (20).
- **Oversikt uten driver** (10): bare GPU-delen blir en tom tilstand («Fant ingen NVIDIA-driver»). Grafene viser CPU-last, CPU-temperatur og minne.

**Må før installasjon**
- **Statusblokken** (1): navnene får plassen tallet og sparklinja ikke bruker, og kuttes ikke («Kjerneklokke», «Minneklokke»). «Tråd 3» uten «MHz»; enheten står ved tallet. Ny test sjekker lengden og at ingen navn gjentar enheten, og skjermbildeplanen sjekker det samme med «Stor» skrift.
- **Flisene på Oversikt** (7) står i 6, 3 eller 2 spalter etter bredden, som demoen. Skillelinjene følger rutenettet.
- **«standard»-merket på Kontroller** (19) står under standardverdien på sporet (245 W ved 245, 0 ved 0), og flyttes inn hvis det ville havnet oppå endene.
- **Kontroller når kortet er borte** (20): banneret sier «Kortet svarer ikke. Verdiene vises igjen når det er tilbake.» «Lag utkast» begynner fra siste lesing.
- **Viftekurve** (25, 26): dekningen vises som tid, fra samme kilde begge steder («Data for 3 min av 48 t», «1 min av 24 t»). Under ett døgn sier linja «siden 18:11 (3 min)», ikke «siste døgn».
- **«Stopp tjenesten nå»** (33) i Generelt › Bakgrunn, med tekst om hva som stoppes (loggen nevnes når den går) og angre i 5 s. Tjenesten avslutter når siste vindu er lukket, uten de vanlige 60 s.
- **Lisensene** (34): `data/LISENSER.md` (ikonene, Rust-bibliotekene og MIT) er bygget inn og vises fra Om › «Lisenser …». `install.sh` legger den i `~/.local/share/doc/agentic-rigg/`.

**Bør**
- **Sparklinjene i statusblokken** har minste spenn (2): turtall 200 o/min, temperatur 5 °C, effekt 20 W, prosent 10, klokker 400 MHz.
- **Kortvelgeren** (3): «Svarer ikke siden 18:03» med NVML-koden i verktøytipset, fokus på «Mer om kortet» (verdien ble markert), og PCIe som på Oversikt («x16 Gen 1 · maks Gen 3»).
- **Tastatursnarveiene** (4): dialogen er høy nok til alle radene, og foten har egen bakgrunn og en skillelinje.
- **Temperaturstanga** på Oversikt (8) begynner på 20 °C.
- **Grafene når kortet er borte** (9): hullet til høyre kommer etter 3 s og sier «Kortet svarer ikke siden 18:03» (eller «Svarer ikke» når det er smalt), og flaten før dataene sier «Data fra 18:01», ikke «Samler data …». PCIe viser «–».
- **Minnet summerer** (15): «Buffer og cache» er uten delt minne (Shmem), som allerede er med i «Brukt».
- **Klokkeområdet på System** (16): «2 200–3 700 MHz · boost til 4,35 GHz (målt)» fra tjenestens høyeste trådklokke.
- **Det mørke båndet på offset-sporene** (21) forklares under raden: «Mørkt felt: tillatt når skriving slås på (−200…+300 MHz).»
- **«o/min»** brytes ikke lenger over to linjer (22).
- **Begrensninger og Profiler** (23): brikkene er de samme som på Sensorer («P8», «Tomgang»), teksten er «Kortet er i tomgang (P8). Årsakene leses når kortet jobber.» med 25 ms-forklaringen i verktøytipset, og «0 av 5» strekkes ikke.
- **Aksetekstene på Viftekurve** (27) kolliderer ikke i hjørnet, og det siste tallet går ikke utenfor grafen.
- **Viftekurve når kortet er borte** (28): «–» med «Kortet svarer ikke», ikke «Leses ikke ennå».
- **Ingen omtale av demoen** (35) i Innstillinger › Data.
- **Tastaturrunden** (43): se under.

**Kan**
- Viftestyring heter «Auto · driveren styrer» og «Manuell · satt utenfor appen» overalt (5).
- Swap og andre MiB-tall uten desimaler (11). Nett heter «mottatt» og «sendt» (12).
- «Skjerm aktiv (driverens flagg)» med forklaring (13). Revisjonen er «0xA1», ikke «0x0XA1» (14).
- «PREEMPT_DYNAMIC» og «17h (23) · 8 · 2» på System (17). Instruksjonssettet med store bokstaver og en linje om hva uthevet betyr (18).
- «Ikke dokumentert» på Lys har spørsmålstegn, ikke øye (32).
- Stiene i Data får plass (36). Versjonen skrives «1.0.0 · bygg 16» overalt, også i `--version` (37).
- «Takt i bakgrunnen» er dempet når den ikke gjelder (38).
- Med «Driveren er oppdatert» viser statusblokken «–» for GPU-verdiene (41).
- Legendene sier «Skjul GPU», ikke «Skjul gpu».
- Tallkolonnene på Sensorer vokser med skriftstørrelsen («10,93 MB/s» ble kuttet med «Stor»), og navnene i «Leses ikke» brytes over to linjer i smale vinduer i stedet for «…».
- Forklaringene i Generelt › Bakgrunn er kortere, og «2 s-takt» brytes ikke ved bindestreken.
- Skjermbildeplanene skriver alle etiketter med «…» til `kuttet.txt`, så avkutting blir funnet uten å se på hvert bilde. Planen `3c` tar også statusblokken med festede nøkler i standard og «Stor» skrift, Om, Lisenser, snarveiene og Lys før og etter 3D.

**Tastaturet (43)**
- Prøvd med ekte tastetrykk: et virtuelt tastatur i den hodeløse Shell (`docs/produksjon/verktoy/runde4b/`), skjermbilder fra Shell og vinduets fokuslogg (`RIGG_FOKUS=1`, bare utviklingsprofilen). Bildene og stegene ligger i `docs/skjermbilder/1.0.0/runde4b/tastatur/`.
- **Fokusringen sto rundt alle foreldrene** til flaten med fokus (flisraden, innholdet, toppfeltet), fordi GTK setter `:focus-visible` på hele kjeden. Nå `:focus:focus-visible`, og tekstfeltene viser fokus bare med rammen.
- **Tab sto fast** på siste flate på en side (grafen på Kontroller, punktskyen på Viftekurve). Nå går Tab rundt til starten, som i GTK ellers.
- **Verdiene i panelene var stopp for Tab** (hver verdi i Nå, Siste døgn, Feilsøking). De kan fortsatt merkes og kopieres med musa, men Tab går bare mellom ting som kan brukes.
- **Esc i et tomt filter** på Sensorer flytter fokus ut av feltet; første Esc tømmer det.
- Ctrl+, Ctrl+F og søk i Innstillinger, Esc, Ctrl+B, Ctrl+?, Ctrl++ og Ctrl+0, Ctrl+W og Ctrl+Q er prøvd og virker.

**Ytelse:** uendret eller bedre. Tallene står i `docs/produksjon/maalinger.md` (runde 4B) og i `STATUS.md`.
