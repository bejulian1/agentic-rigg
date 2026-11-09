# Lys i 3D – prototype

> **Flyttet inn i appen i v0.0.13.** 3D-koden ligger nå i `web/js/lys3d/` og effektmotoren i `web/js/rgb.js` (`RgbEngine`). Hva som er endret underveis, står i `docs/lys-3d-v0.0.13.md`. Den viktigste endringen er at 3D-visningen går i appens felles løkke og frigjøres når Lys ikke vises. Mappa her er beholdt som den var, så prototypen og kontrollen (`verktoy/kontroll.mjs`) fortsatt kan kjøres for seg.

En frittstående prototype av forhåndsvisningen på Lys-siden. Skjermkortet vises i 3D, og du kan vri det med mus, berøring og tastatur. Du kan ta av dekselet med viftene, kjøleren og bakplaten til bare kretskortet er igjen, eller skille lagene fra hverandre i en eksplodert visning. Lyssonene `bar`, `fan1` og `fan2` får fargene fra den samme effektmotoren som `web/js/rgb.js`. Alt er simulert, og ingenting sendes til kortet.

Kortet er generisk, i RTX 2080-størrelse: TU104-klasse GPU (545 mm²), åtte GDDR6-brikker, 8 + 2 strømfaser, 8- og 6-pins strøm, PCIe x16 med gullkontakter, 3 × DisplayPort, HDMI og USB-C, to vifter på 90 mm og lyslist. Det har ingen produsentlogoer eller varemerker.

Ingen filer i appen er endret. Prototypen har ingen avhengigheter og laster ingenting ned.

## Åpne

```sh
cd prototyper/lys-3d
python3 -m http.server 8765 --bind 127.0.0.1    # eller: node verktoy/server.mjs
```

Åpne <http://127.0.0.1:8765/>. Prototypen kan ikke åpnes direkte som fil, fordi nettlesere ikke laster JS-moduler fra `file://`.

| Gjør | Mus og berøring | Tastatur (når 3D-visningen har fokus) |
|---|---|---|
| Vri | Dra | ← → (15°), ↑ ↓ (10°), med Shift 45° og 30° |
| Zoome | Rulle, klype | + og − |
| Tilbakestille | Dobbeltklikk, knappen Tilbakestill | 0 eller Home |
| Velge sone | Klikk på lyslisten eller en lysring | Sone i Farge-panelet |
| Se hva en del er | Pek på den | Fokus i dellisten fremhever delen |

Alle kontroller er vanlige knapper og radiogrupper med piltaster, som i appen. Visningen har `role="application"`, en beskrivelse som følger med på det som vises (vinkel, lag, lys og vifter), og en tydelig fokusring. Dellisten er en tekstlig versjon av modellen.

URL-parametre (brukes av skjermbildene og er nyttige for å dele en tilstand): `lag=0–3`, `skill=0–100`, `vinkel=forfra|skraatt|ovenfra|bakfra`, `effekt=static|breathe|rainbow|cycle|temperature|off`, `farge=3a7bf0`, `lysstyrke`, `temp`, `bakgrunnslys=på`, `tema=light|dark`, `bevegelse=full|reduced|off`, `skall=av` (uten kulissen av sidefeltet) og `kvalitet=fast` (slår av vernet mot treg grafikk).

## Teknologi: håndskrevet WebGL 2

Jeg valgte ren WebGL 2 uten bibliotek, fordi det ga klart best resultat for innsatsen:

- **CSS 3D** (`preserve-3d` med flater) holder til et deksel og to vifter, men ikke til et kretskort med flere hundre deler. Det gir ingen ekte dybdesortering (flater som krysser, flimrer), ingen skygger og ingen lys som faller på delene. Hver flate er et DOM-element, så 60 000 hjørner er uaktuelt.
- **Canvas 2D med egen projeksjon** måtte sortert og fylt alle trekanter i JavaScript. Det er tregt og gir ingen skygger.
- **WebGL 2** har dybdebuffer, skyggekart, kantutjevning (MSAA), mipmaps og instansiering. Modellen er ca. 59 000 hjørner i 52 tegnekall, og et bilde koster 0,2–0,35 ms JavaScript.

Det som er laget:

- `js/gl/math.js`: matriser og vektorer. `js/gl/mesh.js`: geometribygger (bokser med avrundede kanter og glatte normaler, sylindre, rør, rørbøyer, buestriper og flater for vifteblader).
- `js/gl/renderer.js`: skyggeleggere og pass. Hovedlys med skyggekart (2048², PCF), fyll- og kantlys, halvkuleomgivelse og en enkel studio-refleksjon for metall, ACES-tonekurve. LED-sonene lyser på omgivelsene: lyslisten som linjelys og ringene som punktlys. LED-ene er selvlysende flater med glorie (instansierte, additive). Gulvet tar imot en myk skygge. Plukking (hvilken del som er under pekeren) tegner bare én piksel.
- `js/textures.js`: alle teksturer tegnes med canvas ved oppstart. Det gjelder loddemaske, ledere, gjennomføringer, puter og silketrykk på begge sider av kortet, etiketter på brikkene, og braketten og bakplaten med utsparinger i alfakanalen. Det finnes ingen bildefiler.
- `js/layout.js`: én kilde for alle plasseringer i millimeter, så teksturen og geometrien alltid stemmer.
- `js/card-model.js`: delene, materialene, lagene og LED-plasseringene. `js/view.js`: kamera, lag, input, løkka og lysgrensesnittet. `js/engine.js`: effektmotoren. `js/main.js`: siden.

## Ytelse

Målt med `node verktoy/kontroll.mjs --gpu` i headless Chrome 153 ved 1440 × 900, med visningen på ca. 1100 × 760 piksler, regnbue og vifter som går. Tallene varierer litt mellom kjøringer, så tabellen viser spennet fra flere kjøringer.

| | Maskinens GPU (RTX 2080, ANGLE/OpenGL) | Programvare-GPU (SwiftShader) |
|---|---|---|
| Bilder per sekund under animasjon | 59,6–60,0 (taket) | 2–5, så slår vernet inn |
| JavaScript per bilde | 0,23–0,36 ms (p95 0,4–0,5 ms) | 0,4–0,5 ms |
| Hele bildet, ventet ferdig på GPU-en | 1,3–2,5 ms, også med nytt skyggekart | 110–240 ms |
| I ro (statisk lys, vifte stoppet) | 0 bilder | 0 bilder |
| Skjult fane, redusert bevegelse | 0 bilder | 0 bilder |
| Oppstart, første gang i en ny profil | Teksturer 0,24 s, skyggeleggere og opplasting 1,0 s | 0,2 s og 0,4 s |

Slik holdes belastningen nede:

- Løkka tegner bare når noe endrer seg, og sover ellers. Den stopper i skjult fane og når visningen er utenfor skjermen (`IntersectionObserver`).
- Løpende animasjoner (vifter og effekter) tegnes høyst ca. 60 ganger i sekundet, også på skjermer med høyere frekvens. Input tegnes straks.
- Skyggekartet tegnes bare på nytt når lag flytter seg eller lyset dreier med kameraet. Viftehjulene kaster ikke skygge, så snurrende vifter alene tegner det ikke på nytt.
- Kjøleribben bytter til en stripet blokk med mipmaps når lamellene blir mindre enn ca. 9 piksler. Det fjerner moaré og sparer over 100 bokser.
- Skyggeleggerne kompileres parallelt (`KHR_parallel_shader_compile`), så siden ikke fryser mens driveren jobber.
- **Vern mot treg grafikk:** tar bildene over 50 ms under animasjon, senkes oppløsningen i trinn til 50 %. Er det fortsatt for tregt, blir vifter og effekter stillbilder, og visningen sier det («Stillbilde: grafikken tegner for tregt»). Det slo inn i SwiftShader og aldri på RTX 2080.

## Grensesnittet mot rgb.js

Modellen tar imot lys gjennom et lite grensesnitt, `LightTarget`, som `CardView` implementerer. Alle farger er sRGB 0–255, slik `rgb.js` regner dem.

```js
setZoneColor(zone, [r, g, b])   // hele sonen i én farge: 'bar' | 'fan1' | 'fan2'
setLedColors(zone, rgbArray)    // én farge per LED (3 tall per LED, i rgb.js-rekkefølge)
setLayerOpacity(k)              // pust: styrken på alle LED-ene (0–1)
setDimmedZones(zone)            // 'all' eller valgt sone (de andre tones til 30 %)
setAmbient([r, g, b] | null, a) // bakgrunnslys bak kortet
commit()                        // ber om et nytt bilde
```

LED-ene står i samme rekkefølge og med samme `u` som i `rgb.js`: 20 i lyslisten, så 18 rundt vifte 1 og 18 rundt vifte 2, med klokka fra toppen. `js/engine.js` er en ordrett kopi av fargematematikken i `RgbPreview.tick()` uten SVG. Den finnes bare fordi prototypen skal kunne åpnes alene. `verktoy/kontroll.mjs` laster `web/js/rgb.js` og `engine.js` i samme side og kjører 1260 bilder per animasjonsnivå gjennom alle effektene. Det største avviket er 0,000000.

## Plan for å flytte den inn

**Filer i appen som må endres:**

| Fil | Endring |
|---|---|
| `web/js/lys3d/` (ny) | `gl/math.js`, `gl/mesh.js`, `gl/renderer.js`, `layout.js`, `textures.js`, `card-model.js`, `view.js` flyttes hit uendret. `engine.js` og `css/tokens.css` flyttes ikke. |
| `web/js/rgb.js` | Del `RgbPreview` i to: `RgbEngine` (fargene, som i dag) og en tegner. SVG-tegneren beholdes som reserve uten WebGL. Motoren får et mål (`LightTarget`) og kaller `setZoneColor`/`setLedColors`/`setLayerOpacity`/`setAmbient` der den i dag skriver SVG-attributter. Viftefarten flyttes fra `syncSpin()` til `CardView.setFanSpeed()`. |
| `web/js/pages/lights.js` | Lag `CardView` på `#rgb-stage`, med `motion` fra `util.js`. Bruk appens `onFrame` i stedet for egen `requestAnimationFrame`: `CardView.frame()` deles i `tick(dt)` (gir «opptatt» tilbake) og `draw()`. `show()`/`hide()` kaller `setRunning`. Visningen opprettes først når Lys åpnes første gang, så oppstarten på ca. 1 s ikke treffer appstart. Uten WebGL brukes SVG-tegneren. |
| `web/index.html` | Ny markup for Lys (under). |
| `web/styles.css` | Seksjon 6, «Lys»: nytt rutenett, scenen, fokusringen som eget lag og teksten på scenen. Seksjon 5: de slankere gliderne og fargeknappene (neste runde). |
| `test/ui/smoke.test.js` | Lys: 3D-visningen finnes og er ikke 0 × 0, ingen konsollfeil, reserve uten WebGL, piltaster og 0, lagvalget og at løkka sover ved «Av». Sammenligningen med rgb.js fra `kontroll.mjs` blir en egen test. |
| `scripts/contrast.js` | Teksten på scenen (#c9ccd1 og #9a9ea6 mot `--stage`) tas med. |
| `docs/bevegelse.md`, `docs/STATUS.md`, `docs/PATCHLOG.md`, `version.json` | Nye animasjoner (lag som løftes av, kamera som glir, vifter med treghet), status og versjon. |

**Layout uten død plass.** I dag har siden en infolinje, en scene i full bredde med mye luft rundt kortet, en sonelinje og to brede kort under (Effekt og en stor fargevelger). Forslaget, som prototypen viser:

- Forhåndsvisningen fyller venstre del i full høyde. Til høyre står en smal kolonne på 300 px med Effekt (liste og justering), Farge (sone og farger), Simulert kort og Deler. Kolonnen ruller selv, og siden ruller aldri på 1440 × 900 eller 1100 × 760.
- Kameraet rammer inn det som er synlig fra den valgte vinkelen (projiserte hjørner, ikke en omsluttende kule), så kortet fyller bredden.
- Infolinjen «Lyskontroller: ikke koblet til» blir til «Simulert. Ingenting sendes til kortet.» i panelets overskrift, med (?) for detaljene. Det sparer en rad.
- Sone flyttes til Farge-panelet, fordi sonen bestemmer hva fargen gjelder. Linja under scenen har bare lag, «Skill lagene» og bakgrunnslys.
- Under 1100 px forsvinner sidefeltet (som i appen). Under 900 px går kolonnen under scenen, og scenen får 58 % av høyden.

**Slankere glidere og fargeknapper (neste runde, prøvd her):**

- Glider: 3 px spor og 14 px knott (i dag 4 og 18), men fortsatt 20 px høy treffflate. Tallfelt 64 px (i dag 76) og rader på 30 px. Etikett, glider og tall står i ett rutenett med faste kolonner, så sporene står likt.
- Fargeknapper: 20 px (i dag 26) i én rad med de ni fargene, valgt med ring. Hex-feltet er 88 px. Den store metning/lyshet-flaten (ca. 200 px høy) flyttes bak en knapp, «Egendefinert …», som åpner den i en popover. «Nylig brukt» blir en andre rad med samme størrelse.
- Segmentkontrollene bruker `sm` overalt på Lys.

## Kontroll og skjermbilder

```sh
node prototyper/lys-3d/verktoy/kontroll.mjs          # fra agentic-rigg/
node prototyper/lys-3d/verktoy/kontroll.mjs --gpu    # også måling på maskinens GPU
```

Skriptet bruker `test/helpers/chrome.js` uendret og en liten statisk server på en tilfeldig port (`verktoy/server.mjs`, bare 127.0.0.1). Det sjekker motoren mot `rgb.js`, mus, berøring, tastatur, Tilbakestill, lagvalg med piltaster, beskrivelsen, fokusringen, klikk på en lyssone, at delens navn vises ved peking, at løkka sover (i ro, skjult fane, redusert bevegelse), bildetakten og at konsollen er uten feil. Til slutt lagrer det 19 skjermbilder i `skjermbilder/`. Med `--gpu` startes en egen headless Chrome som tegner på maskinens skjermkort, bare for å måle. Ingenting skrives til kortet.

## Det jeg lot være

- Selve flyttingen inn i appen, og de slankere kontrollene på den ekte Lys-siden. De er bare prøvd i prototypen.
- Lagring av valgene (lagtrinn, vinkel) og Bruk/Forkast. Prototypen har bare lokal tilstand. Sone, tema og animasjonsnivå huskes i `localStorage`.
- Fargetemaene Sand, Fjord og Midnatt, høy kontrast og andre aksenter. Bare Grafitt og blå er kopiert inn.
- Detaljer i modellen: NVLink, den skjeve formen på DisplayPort, skruehull i brakettens flens, utsparinger i lamellene rundt varmerørene, kabler og vifteledninger, og tekst og logo på dekselet.
- Ekte bloom og ambient occlusion. LED-glorien er sprites, og dybden i kretskortet kommer fra skyggekartet.
- Teksturene tegnes på hovedtråden (ca. 0,2 s). De kunne vært tegnet i en worker med `OffscreenCanvas`.
- Viftefarten følger samme formel som `rgb.js` (0,17–1,6 omdreininger per sekund), ikke ekte turtall, fordi ekte turtall ville gitt stroboskopeffekt. 0 % betyr at viftene står stille.
- Testet bare i Chrome (headless, med programvare-GPU og med RTX 2080). Firefox, WebKit og ekte berøringsskjerm er ikke prøvd.
