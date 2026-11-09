# Finpuss v0.0.8 · bygg 8

Detaljrunde med friske øyne etter daemon-flyten i v0.0.7. Alle ni sider er sett i headless Chrome i lyst og mørkt tema ved 1440 × 900, 1100 × 800, 1920 × 1080 og 360 px, i høy kontrast og ved 200 % zoom (720 × 450 CSS-piksler med skala 2). Hvert funn er sett før og etter. Ingen nye funksjoner.

Format: **funn** → hva som er gjort.

## Fra veikartet

1. **Viftekurve: «Under 35 % er ikke mulig på kortet» ble krysset av de stiplede linjene** (nødregelen ved 85 °C og temperatur nå) → Teksten er en rolig merkelapp nederst i sonen, i samme form og på samme linje som merkelappene for start og stopp. Den står alltid til venstre for nødregelsonen og tegnes over linjene, så ingen linje krysser teksten. Viker for start og stopp (kortere tekst, så venstre hjørne, ellers ingen).
2. **Viftekurve: etiketten for temperatur nå la seg oppå «Nødregel 100 %»** (sett ved 76–90 °C) → Etiketten bytter side når den ville truffet teksten, og flyttes ned under den når ingen side går (over 90 °C).
3. **Viftekurve: tomrom nederst i Oppførsel når start/stopp er av** (47 px ved 1440 × 900, 227 px ved 1920 × 1080) → Kortene i sidekolonnen har sin egen høyde. Bare Oppførsel vokser når feltene glir fram, og ingenting under den flytter seg. Tomrommet er nå kortets vanlige luft (13 px) i alle størrelser.
4. **Kontroller og Viftekurve: Bruk viste bare en hake mens endringen ventet på Behold** → Når endringen venter, får Bruk ingen hake. Knappen er en vanlig deaktivert, nøytral «Bruk» med verktøytipset «Endringen venter på Behold». Ventetilstanden står i toppfeltet («Venter på Behold») og i bekreftelseslinja. Haken brukes fortsatt når noe lagres uten bekreftelse.
5. **Effekt-grafen: grenselinja ble et lite krøll når grensen endres** (Kontroller, Oversikt, Grafer) → Grensen tegnes som et rent trappetrinn: vannrett, loddrett, vannrett. Den interpoleres ikke mellom prøvene eller fram til «nå», og de vannrette trinnene ligger på hele piksler, så den stiplede linja er skarp.
6. **Flere faner som trykker Bruk samtidig var ikke definert** (de havnet i samme txn, og eierskapet hoppet til fanen som kom sist) → Én ventende endring om gangen. Alle faner viser den samme linja og kan trykke Behold eller Rull tilbake nå. Bare fanen som trykket Bruk, kan bruke mer før endringen er avgjort. De andre ser «… er tatt i bruk i en annen fane.», og Bruk er låst med «Venter på Behold i en annen fane». Serveren svarer 409 (`pending_elsewhere`) på et konkurrerende Bruk, også når to kommer i samme øyeblikk. Beholdes endringen i en annen fane, får fanen som trykket Bruk et varsel («Det ble gjort i en annen fane») i stedet for at linja bare forsvinner. Lukkes eierfanen, står det «Fanen som trykket Bruk, ble lukket eller lastet på nytt.» i de andre.
7. **Fokus ble borte etter Esc i en dialog åpnet fra hovedmenyen** (det havnet på «Til innhold» øverst) → Menyen gir fokus til menyknappen før dialogen åpnes, og dialogen gir fokus tilbake dit når den lukkes, men bare hvis fokus faktisk er borte.

## Detaljrunden

8. **× på bekreftelseslinja var rød etter tilbakerulling** (`.banner .icon` farget alle ikoner, også i knappene) → Bare ikonet foran teksten får linjas farge.
9. **«Effektgrense, kjerneklokke-offset og 1 til er tatt i bruk.»** → Tre navn skrives ut. Fra fire står det «og 2 andre».
10. **Frakoblet midt i fristen sa det samme to ganger** (linja «Bakgrunnstjenesten stoppet. Verdiene fra før er i bruk igjen.» og varselet «… Endringene som ventet, ble rullet tilbake.») → Varselet sier bare at viften gikk til auto når linja allerede forteller om tilbakerullingen.
11. **«Bruk igjen» kunne trykkes mens tjenesten var borte**, og ga en feilmelding → Låst med samme grunn som Bruk («Venter på bakgrunnstjenesten» / «Venter på serveren») til den er tilbake.
12. **Kontroller › Vifte: modusvelgeren hoppet til venstre i Manuell**, og forklaringen brøt seg over fire linjer (raden ble høyere) → Glideren og tallfeltet står i midten, der Auto og Kurve har teksten sin. Velgeren står på samme sted i alle tre modi, og raden har samme høyde. Glideren er uten tall i endene her; 35–100 % står i verktøytipset.
13. **Viftemodus forklart med ulike ord på Oversikt** («Auto (driveren, simulert)», «Kurve (simulert)», og «Driveren (auto)» i live) → Samme mønster som modusvelgeren: «Auto · driveren styrer», «Kurve · følger viftekurven», «Kurve · hviler på 35 %», «Manuell · 60 %», «Nødregel · 100 %». Kontroller og Viftekurve brukte allerede Auto/Kurve/Manuell.
14. **Viftekurve sa «Kurven er brukt» ved siden av merket «Brukes ikke · Auto»** → Toppfeltet sier «Ingen endringer», som på Kontroller.
15. **«Standard» og verdien ble skilt ved linjeskift** («… Standard / 245 W.») og «ved / 40 °C» på Vifte-raden → Hardt mellomrom, så de alltid står sammen.
16. **Grafer: plottene i samme kolonne startet ulike steder** (y-aksen var smalere for 85 °C enn for 2 500 MHz) → Y-aksen har alltid minst plass til «0 000», så plottene står over hverandre (også på Oversikt og Kontroller).
17. **360 px: «Kontrol…» i toppfeltet** → Den tomme endringsindikatoren tar ikke plass når det ikke er endringer, og toppfeltet har litt tettere luft og knapper under 620 px. Tittelen står helt også med endringer.
18. **360 px: forklaringen over viftekurven brøt slik at «Viftefart nå» og tastaturikonet sto alene på hver sin linje** → Forklaringen og ikonet brytes som én rad.

## Sett og funnet i orden

- Bekreftelseslinja i alle tilstander: venter (med og uten klemt verdi, én, to og tre endringer), nedtelling, beholdt (varsel), rullet tilbake (fristen gikk ut, du valgte å rulle tilbake, siden lastet på nytt, fanen som trykket Bruk ble lukket, tjenesten stoppet) og frakoblet midt i. «Forbindelsen brøt» og «Alt ble tilbakestilt» er ikke fremprovosert i denne runden. Også i høy kontrast og ved 360 px.
- «Klemt av tjenesten: 300 W → 275 W» på raden og i linja. «Bakgrunnstjeneste: ikke tilkoblet» med nedtelling og Prøv nå, bare på Kontroller og Viftekurve.
- Fokusring på alt som nås med Tab på Kontroller og Viftekurve (glidere har ringen på knotten, tallfelt på feltet, brytere på sporet). Rekkefølgen er hopp til innhold, sidefelt, sidens handlinger, linjene under toppfeltet og innhold. Esc lukker hovedmenyen, kortvelgeren, profilmenyen og dialogene, og fokus går tilbake til knappen.
- Ingen vannrett rulling og ingen avkuttet tittel ved 200 % zoom. Den eneste avkortede teksten er sensornavnet «USB-C-kontroller (skjermkort)» på Sensorer, som har hele navnet i verktøytipset.
- Kontrast: `npm run contrast` passerer (dårligste tekst 4,51:1, dårligste UI-grense 3,01:1).

## Latt være

- **Sidekolonnen på Viftekurve slutter ikke i samme høyde som kurvekortet** ved 1440 × 900 når start/stopp er av (34 px kortere). Det alternativet var et kort med tomrom inni, som var verre ved større vinduer. Med start/stopp på slutter de likt.
- **Lys sier «Brukt: Statisk»** i toppfeltet i stedet for «Ingen endringer». Lys har ingen bakgrunnstjeneste og ingen bekreftelse, og effekten er nyttig å se der.
- **Tastaturbrukere får ikke verktøytipset på merkene** «Grenser fra kortet» og «Brukes ikke · Auto» (de er ikke fokuserbare). Teksten står i merket; tipset er tillegg.
- **«8 000» på y-aksen for minneklokke** ser ut som «8000» i lerretet ved liten skrift. Tallet har tusenskille (U+202F); det er glyfene som står tett.

## Regnbue og fargesyklus (veikartpunkt fra v0.0.6)

Fasen rundes nå til hele 1,5° (under det øyet skiller). I bildene mellom to trinn er alle fargene like, så ingen LED skrives og ingen stil beregnes. Ved fart 5 skrives regnbuen 100 ganger i sekundet og fargesyklusen 73, uansett skjermfrekvens. Målt med rAF-innpakning og `Performance.getMetrics` over 4 s på Lys:

| | Før | Etter |
|---|---|---|
| 60 Hz (headless): skript per bilde, regnbue | 0,13 ms | 0,15 ms (uendret, 60 < 100 trinn/s) |
| 60 Hz: stil per sekund, regnbue | 24,5 ms | 25,5 ms (uendret) |
| Ubegrenset takt: bilder per sekund, regnbue | 838 | 1 723 |
| Ubegrenset: skript per bilde, regnbue | 0,059 ms | 0,012 ms |
| Ubegrenset: stil per bilde, regnbue | 0,143 ms | 0,035 ms |
| Ubegrenset: skript per bilde, fargesyklus | 0,029 ms | 0,008 ms |
| Ubegrenset: stil per bilde, fargesyklus | 0,067 ms | 0,030 ms |

Regnet om til 240 Hz (brukerens skjerm) blir stilberegningen for regnbuen ca. 34 → 18 ms/s og skriptet ca. 14 → 7 ms/s. Ved 60 Hz er det ingen gevinst, fordi trinnene da kommer oftere enn bildene.
