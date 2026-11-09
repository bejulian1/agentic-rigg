# Status for designkritikken

To runder står her: først kritikken av v0.0.5 (`docs/design-kritikk-v0.0.5.md`), rettet i v0.0.6 · bygg 6 («Kritikkrunde 2»), og under den kritikken av v0.0.3, rettet i bygg 4.

Tegnforklaring: **✓** gjort · **–** ikke gjort (med grunn) · **→ anim** overlatt til animasjonsagenten · **≈** gjort på en annen måte enn foreslått (grunnen står).

# Kritikken av v0.0.5 → v0.0.6 · bygg 6

Skjermbilder før og etter (lyst og mørkt, 1440×900, pluss 1100, 1920 og 360 px der funnet gjelder) ligger i scratchpad-mappa `v006-del1/` for økten (tmp, forsvinner ved omstart). Ønskelisten i kapittel 5 er laget i del 2 (nederst her og i `PATCHLOG.md`).

## Høy

- ✓ F29 Lys-forhåndsvisningen var 0 × 0 under ca. 1240 px (og i lave vinduer, f.eks. 1440×560). Årsaken var `cqh` i en `container-type: size`-scene som fikk høyden fra `min-height`. Nå måles scenen med en `ResizeObserver` i `lights.js`, og kortet får bredde og høyde i px (660 × 280). Uten fast sidehøyde (enkolonne) får scenen høyden fra bredden (`aspect-ratio: 660 / 330`). Fanges av røyktesten (se under).
- ✓ F22 Kontroller: høyre kolonne er ikke lenger tom. Til høyre: Endringer (én linje når ingenting er endret), Profiler og spennings- og klokkekurven, som fyller resten. Til venstre: Justeringer og to grafer for siste minutt, Effekt (trekk og grense) og Kjerneklokke, så du ser hva modellen gjør med det du bruker. ≈ Kritikken foreslo at Justeringer skulle ta hele venstre høyde; det ville gitt ca. 360 px tomt inne i kortet ved 1440×900, så resten av høyden går til grafene i stedet.

## Middels

- ✓ F1 Frakoblet: Bruk er låst med verktøytipset «Venter på serveren» på Kontroller, Viftekurve og Lys, og statusblokken dempes som innholdet. ≈ Forkast er fortsatt aktiv, fordi den bare forkaster lokalt og virker uten server. «2 s» hadde allerede U+202F (det smale mellomrommet er bare smalt i Ubuntu Sans).
- ✓ F2 Én PCIe-formatering (`pcieText` i `gpufmt.js`) overalt: «x16 Gen 1 · maks Gen 3», og «x16 Gen 3» når lenken går fullt. Brukt på Oversikt, Grafikkort, enhetspopoveren og Spesifikasjon («PCIe 3.0 x16» → «x16 Gen 3», raden heter nå PCIe). Verktøytipset gir overføringsraten («Gen 1 = 2,5 GT/s per lane»). Grafikkort leser den løpende verdien.
- ✓ F8 Klokker strekkes ikke lenger: fast radhøyde. Skjermkort har fem rader og Klokker overskrift og fire rader, så kortene er like høye av seg selv (230 px ved 1440×900), og Temperatur og Effekt fyller samme høyde. ≈ Ingen ekstra rad trengtes.
- ✓ F9 Minste spenn for effekt ved «Tilpass» er en fjerdedel av effektgrensen (minst 40 W), på Oversikt, Grafer og Kontroller. Temperatur hadde allerede 20 °C.
- ✓ F11 Sensorer: «Min/maks siden 23:37» står ved ↺ (tidspunktet også i verktøytipset), og «Fold sammen alle» står først, skilt fra søkefeltet med en strek. Under 620 px skjules fold-knappen.
- ✓ F12 Ved 360 px brytes ikke navnene midt i ordet lenger: én linje med ellipse og hele navnet i verktøytipset, og litt smalere tallkolonner. (`hyphens: auto` virker ikke for norsk i Chrome på Linux, prøvd.)
- ✓ F20 System: kolonnene er like høye, og Instruksjonssett fyller resten. Først de 19 utvalgte, så så mange av de andre settene som får plass i hele rader (`contain: size`, så de aldri gjør kolonnen høyere). Overskriften viser «76 av 110». Se også «Vis alle» under kapittel 3.
- ✓ F23 Live-modus: Vifte-raden sier «Driveren styrer viften. Den simulerte kurven ville gitt 38 %.»
- ✓ F24 Spennings- og klokkekurven har fast y-akse over hele det mulige området (standard + −200 til +300 MHz), så kurven løftes i stedet for at aksen flytter seg.
- ✓ F26 Viftekurven kan ikke falle: dra, piltaster og dobbeltklikk holder punktet mellom naboenes viftefart, og tallfeltene i Punkter har grenser og rød kant ved ugyldig tall (verdien legges innenfor, med et kort varsel). Serveren løfter også punkter som ligger lavere enn forrige (gjelder gamle filer og profiler), med egen test.
- ✓ F30 Lys ved 360 px: merkene er borte (F31), og sonevalget bruker hele bredden og krymper i stedet for å gå ut av kortet.
- ✓ F32 Datakilde: «Simulert skjermkort (låst ved oppstart av serveren). CPU, minne og nett er alltid ekte.» Ingen miljøvariabel og ingen dobbel setning.
- ✓ F33 Datamappe: én rad med etikett, stien med ellipse i midten (de to siste mappene står alltid) og kopier-knappen alltid synlig. Hele stien i verktøytipset.

## Lav

- ✓ F3 Utviklerspråk: serveren sier «Valgt ved oppstart av serveren» i stedet for «Tvunget med AG_SIMULATE=1» (popover: «Simulert (valgt ved oppstart av serveren)»), Lys-tipset sier «Kortet har trolig en Palit-lyskontroller …», og Om viser filstiene bare under Feilsøking. (API-feilen fra `/api/mode` nevner fortsatt `AG_SIMULATE=1`; den vises ikke i appen, fordi Live-knappen er låst.)
- ≈ F4 Forkast og Bruk står fortsatt i toppfeltet, men deaktivert Bruk er nøytral grå i stedet for halvgjennomsiktig blå. Å skjule knappene ville flyttet teksten i toppfeltet ved første endring, og det hoppet var verre enn to rolige grå knapper.
- ✓ F5 Små sparklinjer (statusblokken, flisene og Sensorer) har en tynn stiplet grunnlinje der det mangler data, ikke en grå flate. De store grafene beholder den skraverte flaten.
- ✓ F6 «Tilbakestill» uten ellipse.
- ✓ F7 Ikonknapper uten tekst får `aria-label` fra verktøytipset (også hovedmenyen), Tastatursnarveier viser én tast per knapp ([Ctrl] [,]), og Om skriver versjonen «alpha v0.0.6 · bygg 6» som loggen.
- ✓ F10 Skjermkort på Oversikt gjentar ikke flisene: GPU-last og minnekontroller er byttet med Videominne (stolpe, «1,4 / 8,0 GiB»), og P-tilstand står sammen med begrensningen («P8 · tomgang», gul når den er alvorlig).
- ✓ F13 Nå-kolonnen på Sensorer får gul og rød etter tersklene (GPU- og CPU-temperatur, GPU-, CPU- og trådlast), og teksten under Varselfarger sier det.
- ✓ F14 «Ikke tilgjengelig» filtreres også, og «Ingen sensorer passer filteret» vises bare når ingenting passer. «Ikke tilgjengelig –» er fjernet fra forklaringen, og navnene skrives «Gruppe · sensor».
- ✓ F15 Én kolonne opp til 1560 px innholdsbredde, med navnekolonnen på 260 px og 60 s-sparklinjen i resten av bredden. ≈ Grensen for to kolonner ble hevet (fra 1300 px), ikke senket: med to kolonner ved 1300 px ble navnekolonnen bare ca. 80 px.
- ✓ F16 Pause: merket «Pauset 00:18» i toppfeltet og «pauset» i stedet for «nå» på tidsaksen.
- ✓ F17 Én enhet per tidsvindu: «−90 s · −60 s · −30 s» for 1 og 2 min, minutter for 5 og 15 min.
- ≈ F18 Klokker på Grafikkort har nå samme form som på Oversikt (stolpe, Nå, Maks), og boostklokken står i noten under. Ingen egen Spesifikasjon-kolonne; den står allerede i kortet ved siden av.
- ✓ F19 Simulerte verdier (Bus-ID, Driver, CUDA) er «–» i dempet farge med verktøytipset «Simulert kilde». Enhetskortet skriver bare «Simulert», ikke «driver simulert».
- ✓ F21 Verktøytipset om moduler og timinger bruker bare serverens setning, uten «(dmidecode)». Systemrader uten verdi sier «Ikke oppgitt», ikke «Ikke oppgitt av driveren» (Skrivebord).
- ✓ F25 Profiler: «Lagre som ny profil», merket «1 av 5», ingen «plass N». Er verdiene lik en profil, er knappen av og det står «Lik «Profil 1»». Hver rad har Hent og en ⋯-meny (lagre over, gi nytt navn, tøm). «Skjemaet» er borte fra tekstene.
- ✓ F27 Nå-kortet på Viftekurve er borte. Grafens etikett sier «33 °C → 37 % · vifte 36 %» (kortere i smale vinduer), og Oppførsel får plass uten rulling ved 1440×900.
- ✓ F28 «Kurve · Egendefinert», endringsprikk på punkter som ikke er brukt, og kortere sonetekst («Under 35 % går ikke») eller ingen i smale plott.
- ✓ F31 Bare unntakene er merket: et lite øye for effekter som bare er forhåndsvisning, med forklaring i kortets overskrift.
- ✓ F34 Avkrysningslistene bruker `repeat(auto-fill, minmax(190px, 1fr))` (tre kolonner), og Sider er nå avkrysningsbokser i samme rutenett.
- ✓ F35 «Brukt i min/maks-tidspunktet på Sensorer, i profiler og i grafer.»

## Kapittel 3: de kjente punktene

- ✓ Lys: Justering er slått inn i Effekt-kortet (effektlista, en skillelinje, så Lysstyrke og ev. Hastighet, Retning og Område). Raden under scenen er to omtrent like høye kort, Effekt og Farge.
- ≈ Oversikt: tomrommet mellom stolpe og bunntekst er borte. Kortene er innholdshøye og like høye som naboene, og forklaringen står rett under stolpen som en skala-legende («▏mål 83 °C ▏nedklokking 97 °C … avstenging 100 °C»). Merkelapper plassert ved strekene ble prøvd på papiret, men 97 og 100 °C ligger 13 px fra hverandre ved 1440 px, så de ville kollidert.
- ✓ System: «Vis alle (N)» er erstattet av en fast ikonknapp (utvid/fold), og tallet står i overskriften («76 av 110», «110 i alt», «12 av 110» ved søk). Ingenting endrer bredde.

## Kapittel 5: ønskelisten (del 2)

- ✓ 5.1 Start og stopp: raden øverst i Oppførsel (med bryter, av som standard), «Hysterese 7 °C. Under 48 °C holder viften 35 %.», stiplede linjer og skravert bånd i grafen som kan dras og flyttes med tastaturet, flat linje på minste fart til venstre for stopp, og «Følger viftekurven fra 55 °C · nå 35 % (under start)» på Kontroller. Merkelappene heter «start» og «stopp», og feltene «Start ved» og «Stopp under» i stedet for «slipper under»: kortere, og det samme ordet som i grafen. At «stopp» er minste fart, står i undertittelen og verktøytipset.
- ✓ 5.2 Min og maks huskes: valget og teksten som foreslått, dato i «Min/maks siden …», «Snitt for denne økten» på Snitt. På som standard (begrunnet i `PATCHLOG.md`).
- ✓ 5.3 Appikon: flis med vifte og PCIe-kontakt, fullfarge og symbolsk. Varianten «skjermkort skrått forfra» ble tegnet, men er for bred og utydelig ved 16–32 px.
- ✓ 5.4 Feste og rekkefølge: nål ved hover/fokus, fylt på festede rader, høyreklikk, ordnet liste med håndtak, dra, Alt+↑/↓, × og «Legg til verdi …» med søk, meny på statusradene (Flytt opp/ned, Løsne, Vis på Sensorer). Ingen «Festet»-gruppe. Høyst 8 verdier. ⋯-knapp på statusradene er ikke laget (høyreklikk og menytasten gir menyen), fordi raden er for smal til en knapp ved siden av sparklinja.

# Kritikken av v0.0.3 → v0.0.4 · bygg 4

Kopi av funnlista i `docs/design-kritikk-v0.0.3.md`, krysset av etter redesignet i bygg 4 og etter runden med bevegelse og detaljer (regler i `docs/bevegelse.md`).

## 2.1 Typografi

- ✓ T1 Fontstakk `Ubuntu Sans, Adwaita Sans, Cantarell, Inter, system-ui`, mono `Ubuntu Sans Mono …`.
- ✓ T2 Fem størrelser (`--fs-cap/sm/body/title/num`) som skaleres med skriftstørrelse-valget.
- ✓ T3 Bare vekt 400 og 600.
- ✓ T4 Ingen store bokstaver med sperring; seksjonsetiketter i `--fs-sm`/600/`--muted`.
- ✓ T5 [LAT] U+202F mellom tall og enhet overalt (`fmt.js` `withUnit`), også «38 %».
- ✓ T6 [LAT] Viftekurven: tall på aksene, enheten i aksetittelen, «37 °C → 37 %» med U+202F.
- ✓ T7 [LAT] Målte tall høyrejustert med tabulære sifre (kv-verdier, tabeller, stolper).
- ✓ T8 Ingen kursiv; manglende verdier i `--dim`.

## 2.2 Avstander og rutenett

- ✓ 4-punkts skala `--sp-1…6`. [LAT]
- ✓ Komponenthøyder `--h-sm/md/row`, skalert av tettheten.
- ✓ Container queries på `.content` i stedet for vindusbredde. ≈ Ikke et 12-kolonners rutenett; hver side har eget enkelt grid, som var nok.
- ✓ Lik innholdsmarg (`--content-pad`).
- ✓ Gruppeavstand `--gap` og titler som går opp i rytmen.

## 2.3 Farger og kontrast

- ✓ P1 `--dim` ≥ 4,7:1 i begge temaer (målt).
- ✓ P1 [LAT] Lys knott med skygge på glidere og brytere.
- ✓ Tydeligere rammer og svak skygge på kort; feltrammer bare på felt.
- ✓ Serie 2 velges ut fra aksenten (oransje eller blå), så den alltid skiller seg.
- ✓ «Simulert» og «Referanse» er nøytrale merker/infolinjer; gult brukes bare for advarsel og statusmerket «Simulert GPU».
- ✓ P3 Kaldere grå i lyst tema (Adwaita-nære flater).

## 2.4 / 7 Ikoner

- ✓ Lucide (ISC) som inline sprite i `index.html`, strek 1,75, `currentColor`, ingen opacity-triks. Lisens i `docs/LISENSER.md`.
- ✓ Unike ikoner per side. ≈ Grafikkort bruker Lucides eget `gpu`-ikon (finnes nå i settet), så det trengtes ikke et egentegnet.
- ✓ Alle foreslåtte handlingsikoner (kopier, pause/spill, nullstill, angre, fold, søppel, lagre, øye, info/varsel, lås, strek over sirkel, pluss, søk).
- – P3 Nytt appikon: ikke laget. Det trenger en egen runde med GNOME-retningslinjene og bør ikke gjøres halvveis.

## 2.5 Komponenter

- ✓ K1 `.kv`-rader uten feltramme, verdier markerbare; felt bare der du skriver.
- ✓ K2 [LAT] Segment i AdwToggleGroup-stil, synlig fokus på valgt segment.
- ✓ K3 [LAT] `.btn`, `.flat`, `.suggested`, `.destructive`, `.icon-only`, hover/trykk/deaktivert med grunn i verktøytips.
- ✓ K4 Tallfelt og glider på én rad med ↺ som bare vises ved avvik fra standard.
- ✓ K5 [LAT] Tabeller med `data-align` (tekst venstre, tall høyre).
- ✓ K6 Lys knott på bryter. Knotten glir og sporet toner over `--dur-2` (150 ms).
- ✓ K7 `.tag` og `.chip` erstatter pill/badge/source/curve-tag.
- ✓ K8 Stolper på 8 px uten ramme, bedre skinne.
- ✓ K9 Fliser med stort tall, dempet enhet og tynn skillelinje.
- ✓ K10 «Rediger kurve →» som flat knapp.

## 2.6 Tom- og lastetilstander

- ✓ P1 [LAT] Plassholdere i samme høyde som innholdet: enhetskort, fliser, Skjermkort/Temperatur/Effekt på Oversikt, og alle lister og tabeller på Grafikkort og System (`skelKv`, `skelTable`). Grupper uten innhold («Ikke tilgjengelig», statusblokken) vises først når de har innhold. Layoutskift ved lasting er nær 0 på alle sider.
- ✓ P1 [LAT] «Samler data …» med skravert flate i grafer og sparklinjer.
- ✓ P1 [LAT] Sparklinjer mot tid (se Sensorer).
- ✓ P2 Tomme profiler som én rad.
- ✓ P2 Frakoblet: infolinje med nedtelling, «Prøv nå» og dempede verdier.

## 3 Skall

- ✓ S1 Delt toppfelt med loddrett skillelinje.
- ✓ S2 Ingen dobbel sidetittel (`.page-head` fjernet).
- ✓ S3 GPU-navn og driver flyttet til enhetskortet.
- ✓ S4 Sidehandlinger som ikonknapper i toppfeltet.
- ✓ S5 Tomrommet i sidefeltet er borte (Innstillinger i menyen, statusblokk nederst).
- ✓ S6 [LAT] Verktøytips i ikonsidefeltet, unike ikoner.
- ✓ S7 Tastetall skjult, i verktøytips og Tastatursnarveier; kan slås på.
- ✓ S8 Statusblokk med én verdi per rad, valgfrie verdier, terskelfarger og klikk til sensor.
- ✓ S9 Versjonen bare i Om.
- ≈ S10 Appikonet i full farge 20 px i sidefeltets toppfelt (ikonet selv er uendret).
- ✓ S11 «Kontroller · simulert» i vanlig skrift.
- ✓ S12 [LAT] `scrollbar-gutter: stable`.
- ✓ 3.2 Hovedmeny (≡) med Innstillinger (Ctrl+,), Tastatursnarveier (Ctrl+?) og Om; statusmerke bare ved unormalt; enhetskort med popover og Live/Simulert; ikonsidefelt med statusprikk.

## 4.1 Oversikt

- ✓ P1 Fyller høyden; nøkkelrad, detaljer og tre grafer. Ingen rulling på 1280×800.
- ✓ P1 [LAT] «Mot nedklokking» erstattet av temperaturskala og «58 °C under nedklokking».
- ✓ P1 °C og % deler aldri akse (tre separate grafer).
- ✓ P1 [LAT] Pillen «Simulert oppsett … 0/0 MHz» fjernet.
- ✓ P2 Navn/driver/VBIOS bare i enhetskortet og Grafikkort.
- ✓ P2 [LAT] Effekt som én stolpe med grensemerke og én dempet linje; «håndhevet» bare ved avvik.
- ✓ P2 Klokker som tabell med Nå/Maks og stolper.
- ✓ P2 «Høyeste 5 min» i stedet for «høyeste i økten».
- ✓ P2 Container queries.
- ✓ P3 Encoder/decoder slått sammen til én rad.

## 4.2 Sensorer

- ✓ P1 [LAT] Sparklinje 196 px × 18 px, grupper i to kolonner på brede vinduer.
- ✓ P1 [LAT] Tegnet mot tid, felles startkant, riktig vindu ved alle intervaller.
- ✓ P1 Én rad per tråd, i lukket «Per tråd»-undergruppe, klokken ved navnet.
- ✓ P1 [LAT] Menneskelige navn, rå navn i verktøytips.
- ✓ P2 [LAT] Fast nettrate-enhet per rad (byttes bare oppover), MB/s eller Mbit/s.
- ✓ P2 P-tilstand og begrensning som merker i gruppeoverskriften.
- ✓ P2 PCIe «Gen 3», snitt «—».
- ✓ P2 Kolonneoverskrift «60 s» over fast kolonne.
- ✓ P2 [LAT] Fold alle som ikonknapp med fast bredde.
- ✓ P2 «Ikke tilgjengelig» med lås-/strek-ikon og to kolonner.
- ✓ P3 Bredere tallkolonner, min/maks i `--muted`.
- ✓ P3 Filter (Ctrl+F).
- – P3 Festing av sensorer: ikke laget. Statusblokken dekker det viktigste; festing trenger en egen modell for vilkårlige sensorer i sidefeltet.

## 4.3 Grafer

- ✓ P1 [LAT] Pene aksemerker (1-2-2,5-5).
- ✓ P1 «Tilpass til data» / «Fast» (prosent alltid fast).
- ✓ P1 Rutenettet fyller høyden.
- ✓ P1 To kolonner også på 1280.
- ✓ P2 Tittel, verdi og min/maks på én linje.
- ✓ P2 CPU-last, CPU-temp, minne, nett og VRAM, valgbare.
- ✓ P2 Tidsvindu 1/2/5/15 min (historikken samles i nettleseren).
- ✓ P2 Tidsakse «−1 min», venstre merke droppes.
- ✓ P3 Synkronisert hårkors.
- ✓ P3 Pause som ikonknapp, mellomrom slår av/på.

## 4.4 Grafikkort

- ✓ P1 `.kv`-rader; spesifikasjonen i to kolonner.
- ✓ P1 [LAT] Død «Standard»-kolonne fjernet, forklaring i fotnote.
- ✓ P2 Forklaringer bak (i).
- ✓ P2 Ingen gjentatt arkitektur; venstre «fra driveren», høyre «referanse».
- ✓ P2 Grenser under Spesifikasjon til høyre.
- ✓ P2 UUID med øye og kopier.
- ✓ P3 Kopier som ikonknapp; toast med antall linjer.
- ≈ P3 Eksport: «Kopier som tekst» og «Kopier som Markdown». «Lagre som JSON» er ikke laget.

## 4.5 System

- ✓ P1 Toppfeltet uten GPU-navn.
- ✓ P1 [LAT] «GNOME (Ubuntu-økt)», «Bygg #31 · 1. aug. 2026», «6. mars 2019», «Mikrokode-revisjon»; rå strenger i verktøytips.
- ✓ P2 Node.js flyttet til Om.
- ✓ P2 8 × 2 fliser, stolpe = last med legende.
- ✓ P2 Cache-tabell justert.
- ✓ P2 Swap «Full» med forklaring.
- ≈ P3 Søk når alle instruksjonssett vises; utvalget fremheves som før.

## 4.6 Kontroller

- ✓ P1 [LAT] Én infolinje (kan lukkes), ingen pille; toast «Lagret (simulert)».
- ✓ P1 Gruppen heter «Justeringer».
- ✓ P1 Én rad per kontroll.
- ✓ P1 [LAT] «Hent» i stedet for «Last».
- ✓ P1 [LAT] Tøm profil med Angre (6 s).
- ✓ P2 Fylte profiler som kort, tomme som én rad, nytt navn med blyant/dobbeltklikk.
- ✓ P2 «Endringer» viser bare forskjeller.
- ≈ P2 Bunnlinja: Forkast/Bruk står i toppfeltet i stedet for en klebrig bunnlinje (samme mønster på Kontroller, Viftekurve og Lys, og den ruller aldri bort).
- ✓ P2 [LAT] Auto: glideren skjules og raden viser kurvens verdi nå.
- ✓ P2 Endringsprikk i stedet for «· endret».
- ≈ P3 Spennings-/klokkekurve som illustrasjon (ikke styrbar). Strømgrense i %, «bruk ved oppstart» og Ctrl+1–5 for profiler er ikke laget.

## 4.7 Viftekurve

- ✓ P1 Kurven fyller høyden.
- ✓ P1 [LAT] Enhet bare i overskrift/aksetittel.
- ✓ P2 Forhåndsvalg som knapper, navnet i tittelen.
- ✓ P2 Redigerbar punkttabell med legg til/slett.
- ✓ P2 Brukt kurve som stiplet spøkelse.
- ✓ P2 Modus som merke i tittelen.
- ✓ P2 «Oppførsel»: hysterese, reaksjonstid opp/ned, minste fart, temperaturkilde (simulatoren bruker dem).
- ✓ P3 Tastaturhint bak (?)-knapp.
- ✓ P3 Shift smekker til 5 °C/5 %, koordinater ved punktet mens du drar.

## 5 Innstillinger

- ✓ P1 Sentrert kolonne (maks 720 px) med underliste; blir faner på smale vinduer.
- ✓ P1 [LAT] Datakilden står én gang (rad med status og segment) pluss enhetskortet.
- ✓ P1 Om er en dialog fra hovedmenyen, uten dobbel versjon.
- ✓ P1 Mange valg som virker og lagres (se PATCHLOG).
- ✓ P2 Temperaturenhet under «Enheter og format».
- ✓ P2 «Bruk simulert ved oppstart» skilt fra «Datakilde» (nå vs. neste oppstart).
- ✓ P2 [LAT] Ingen toast ved vanlige valg; toast bare for «gjelder neste gang serveren starter».
- ✓ 5.2 Seksjonene Generelt, Utseende, Enheter og format, Sidefelt, Grafer og sensorer, Data (datamappe, eksport/import, tilbakestill med angre).
- ≈ Skriftstørrelse og 12/24-timers klokke er laget selv om kritikken kalte dem støy, fordi brukeren ba om dem. Skriftstørrelsen skalerer hele den typografiske skalaen i fire trinn.
- – «Husk min/maks mellom økter» og «grupper lukket ved start»: ikke laget (krever lagring på serveren; lukkede grupper huskes i stedet lokalt).
- – Rekkefølge på statusverdier med dra: ikke laget (fast rekkefølge).
- – «Følg systemets aksentfarge»: nettleseren kan ikke lese den (står i undertittelen).
- – Støy som ikke er laget, som kritikken anbefalte: fri aksentfarge, skriftvalg, rekkefølge på sider, linjetykkelse, egne seriefarger, desimaler per sensor, egne animasjonsvarigheter, lydvarsler.

## 6 Lys (RGB)

- ✓ P1 Forhåndsvisning i full bredde på mørk scene i begge temaer. ≈ Den fyller ledig høyde i stedet for maks 300 px, så siden ikke får tomrom nederst.
- ✓ P1 [LAT] Redigerbart hex-felt med validering, egen fargevelger.
- ✓ P1 Dempede LED-fargeprøver; standard er blå, ikke neon-mint.
- ✓ P1 [LAT] «Lyskontroller: ikke koblet til (simulert)», research-detaljen i (?).
- ✓ P2 [LAT] Hastighet og retning skjules for effekter som ikke bruker dem.
- ✓ P2 Effektmerknaden erstattet av «Støttet»/«Bare forhåndsvisning» per effekt.
- ✓ P2 [LAT] Samme glider + tallfelt som ellers.
- ✓ P2 Soner (Alle/Lyslist/Vifte 1/Vifte 2), valgt sone fremheves.
- ✓ 6.2 Temperatur-effekt, nylig brukte farger, bakgrunnslys (av som standard), Forkast/Bruk i toppfeltet.
- – Pipette: ikke laget (nettleserens EyeDropper finnes ikke overalt og gir lite her).
- ✓ Kryssfade ved bytte av effekt og farge (250 ms). ≈ Effektprøven animeres bare mens pekeren er over raden (eller den har fokus); seks prøver som gikk hele tiden, ville vært pynt.
- ✓ Viftene roteres av kompositoren, lysdiodene ligger i eget lag, pust endrer bare lagets opasitet, og ingenting kjører mens siden ikke vises.

## 8 Bevegelse og mikrodetaljer

- ✓ 8.1 Tokens `--dur-1…4`, `--ease-*`, `data-motion` (full/reduced/off) på `<html>`, «Automatisk» følger `prefers-reduced-motion`. Den brutale «ingen overganger»-regelen er fjernet.
- ✓ 8.2 Hover og trykk (0 ms inn), segment som glir (FLIP, bare transform), bryter, sammenlegging (grid-rader og tone), toast inn/ut med FLIP-stabel, popover og meny fra opphavspunktet, verktøytips med varm gruppe, dialog inn/ut, temabytte som kryssfade med View Transitions (200 ms), kurvemorf og vekst på punkter, RGB-kryssfade, frakoblet-linja, prikker for endring, stolper og temperaturmarkør med transform, y-akse som glir. «Redusert» har bare korte tonninger, og «Av» har ingen animasjoner (målt: 0).
- ≈ 8.2 Sidebytte: kort tone og 4 px løft (200 ms) i stedet for bare kryssfade, fordi brukeren ba om «lite løft». Ingen forsinkelse og ingen layout. Ved redusert bevegelse er det ingen animasjon.
- ✓ 8.2 240 Hz: rAF-løkka sover når ingenting beveger seg, og grafene tegnes når rullingen har flyttet seg ⅓ piksel, ikke hvert bilde (P3-forslaget om 120 fps er erstattet av dette).
- ✓ 8.3 Fokusringer overalt, også på valgt segment og canvas; `outline: none` på profilnavn er borte.
- ✓ 8.3 Tastatur: sidefeltet som én tab-stopp med piltaster, «Til innhold», Ctrl+, Ctrl+? Ctrl+F, mellomrom, Esc, Ctrl+Enter, Ctrl+Z.
- ✓ 8.3 Egen verktøytips-komponent (500 ms, varm gruppe, hurtigtast), på ikonknapper, deaktiverte knapper (hvorfor), avkortet tekst og ikonsidefeltet.
- ✓ 8.3 Tabulære tall, `slashed-zero` i mono, U+2212 minus.
- ✓ 8.3 Ellipsis med full tekst i verktøytips (enhetskort, sensornavn, profiler).
- ≈ 8.3 `.empty`-komponent brukt for søk uten treff; tomme profiler og «samler data» har egne former.
- ✓ 8.3 Skeletons: se 2.6.
- ✓ 8.3 Adwaita-lignende rullefelt.
- ✓ 8.3 Markering: grensesnitt `user-select: none`, verdier markerbare, `user-select: all` på korte ID-er.
- ≈ 8.3 Kopier: UUID og datamappe har kopier-knapp; andre ID-er markeres med ett klikk (ingen kopier-ved-hover).
- ✓ 8.3 `cursor: default` på knapper, `crosshair` på grafer, `grab/grabbing` på kurvepunkter, `help` bare på (i).
- ✓ 8.3 Deaktiverte kontroller med verktøytips om hvorfor. Rettet i bevegelsesrunden: knappene hadde `pointer-events: none`, så tipset ble aldri vist.
- ✓ 8.3 Tab-rekkefølge som følger skjermen (sidefelt, sidens handlinger, innhold), og Tab inn i sidefeltet lander på siden du er på.
- ✓ 8.3 Avkortet tekst får hele teksten i verktøytipset automatisk.
- ✓ 8.3 Kopier og Bruk viser en hake på knappen i 1,2 s. Vellykket kopiering gir ikke lenger varsel.
- ✓ 8.3 Én skjermleserregion (`announce()`) for frakobling, gjenoppkobling og kopiering. Nedtellingen i frakoblet-linja leses ikke lenger opp hvert sekund.
- ✓ 8.3 Angre-toast for tøm profil, nullstill statistikk, tilbakestill kontroller og tilbakestill innstillinger.
- ✓ 8.3 [LAT] Knapper som bytter innhold er ikonknapper med fast bredde.
- ✓ 8.3 Vindustittel med «(simulert)».
- ✓ 8.3 [LAT] README oppdatert, ruten heter `#/grafer`.
- – 240 Hz-test: ikke mulig i headless Chrome (låst til 60 Hz og uten GPU). Skriptid per bilde og arbeid per sekund er målt; se PATCHLOG.
