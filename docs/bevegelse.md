# Bevegelse og detaljer

Dette er språket for bevegelse i Agentic Rigg (alpha v0.0.4 · bygg 4). Følg det når du legger til noe nytt, så appen fortsetter å føles som én ting. Kort sagt skal bevegelse være kort, jevn og meningsfull. Den skal vise hvor noe kom fra, hvor det gikk eller at noe har skjedd, og den skal aldri være pynt. Brukeren har en 240 Hz-skjerm og merker hakking.

## Grunnregler

1. **Bare `transform` og `opacity` animeres.** Unntaket er sammenlegging, som bruker `grid-template-rows` (0fr ↔ 1fr). Ikke animer `width`, `height`, `top`, `left` eller `margin`, for det gir layout i hvert bilde.
2. **Ut går raskere enn inn.** Det som kommer, bruker `--ease-out`, og det som går, bruker `--ease-in` og kortere tid.
3. **Ingen forsinkelse på innhold.** Ting starter straks. Den eneste ventetiden er 500 ms før det første verktøytipset.
4. **Tall tweenes aldri.** Verdier bytter direkte med tabulære sifre. Stolper og markører glir kort, og fargeskifte (gul/rød) toner.
5. **Én rAF-løkke** (`onFrame` i `web/js/util.js`). Bare siden som vises, registrerer seg. En oppgave returnerer `false` når ingenting beveger seg, og da sover løkka helt. `wake()` starter den igjen, og `app.js` vekker den ved nye data, input og hendelser.
6. **Tegn bare det som er endret.** Canvas tegnes bare når noe er nytt, og grafene ruller bare når tidsaksen har flyttet seg minst ⅓ fysisk piksel. RGB-viftene roteres av kompositoren.
7. **`data-motion` styrer alt.** Ingen animasjon får overstyre nivået.

## Tokens (`web/styles.css`, seksjon 1)

| Token | Full | Redusert | Av | Brukes til |
|---|---|---|---|---|
| `--dur-1` | 80 ms | 80 ms | 0 | hover og trykk (bare farge) |
| `--dur-2` | 150 ms | 0 | 0 | små tilstander med bevegelse: bryter, segmentmarkør, chevron, prikker |
| `--dur-3` | 200 ms | 100 ms | 0 | inn/ut: varsel, sammenlegging, banner, sidebytte, nivåfarger |
| `--dur-4` | 250 ms | 0 | 0 | større endringer: kurvemorf, LED-kryssfade |
| `--dur-in` | 140 ms | 100 ms | 0 | popover, meny, verktøytips og dialog inn |
| `--dur-out` | 90 ms | 80 ms | 0 | de samme ut |
| `--dur-vt` | 200 ms | 100 ms | – | kryssfade ved temabytte (View Transitions) |
| `--move` | 1 | 0 | 0 | ganges med all forflytning og skalering |
| `--ease-out` | `cubic-bezier(0.2, 0, 0, 1)` | | | inn i ro |
| `--ease-in` | `cubic-bezier(0.4, 0, 1, 1)` | | | ut |
| `--ease-std` | `cubic-bezier(0.25, 0.1, 0.25, 1)` | | | farger |

Skriv forflytning slik at den forsvinner av seg selv ved redusert bevegelse, f.eks. `translateY(calc(8px * var(--move)))` eller `transition: transform calc(var(--dur-3) * var(--move))`. I JS bruker du `dur('3')`, `EASE.out` og `animate(el, frames, opts, reducedFrames)` fra `util.js`. `animate()` gjør ingenting ved «av», spiller `reducedFrames` (typisk bare opasitet, maks 100 ms) ved «redusert» og `frames` ved «full».

## Nivåene

`data-motion` på `<html>` er alltid `full`, `reduced` eller `off`. Innstillingen «Automatisk» følger `prefers-reduced-motion` live.

- **Full:** alt i tabellen under.
- **Redusert:** bare overganger som viser en tilstand, og da som kort opasitet (≤ 100 ms). Det er ingen forflytning eller skalering, ingen sidebytte-animasjon og ingen segmentglid. Høyden hopper ved sammenlegging, men innholdet toner. Grafene tegnes i trinn, stolpene hopper, RGB er et stillbilde med «Animasjon av» og viftene står.
- **Av:** ingen overganger og ingen animasjoner (`document.getAnimations().length` er 0 under bruk). Regelen ligger nederst i `styles.css`, seksjon 8. Tilstander som ellers vises med en kort animasjon (f.eks. raden som lyser opp på Sensorer), vises statisk.

## Hva som beveger seg

| Sted | Hvordan | Varighet | Redusert |
|---|---|---|---|
| Hover på knapper, rader, menylinjer | bakgrunnsfarge | `--dur-1` `--ease-std` | samme |
| Trykk (`:active`) | bakgrunn `--active`, 0 ms inn | 0 / `--dur-1` ut | samme |
| Fokusring | vises straks | 0 | 0 |
| Sidebytte | opasitet 0 → 1 og 4 px løft (Web Animations i `route()`), ikke ved første side | `--dur-3` `--ease-out` | ingen |
| Sammenlegging (Sensorer) | `grid-template-rows` 0fr ↔ 1fr, innholdet toner, chevron roterer | `--dur-3` / chevron `--dur-2` | høyden hopper, innholdet toner |
| «Per tråd» | radene toner inn | `--dur-3` | opasitet |
| Popover og meny | opasitet og skala 0,96 → 1 fra knappen (`transform-origin` settes i `placePop`) | `--dur-in` inn, `--dur-out` ut | bare opasitet |
| Verktøytips | 500 ms ventetid første gang, opasitet og 2 px glid fra ankeret. Naboer i en varm gruppe får tipset straks og uten overgang (`data-instant`) | `--dur-in` / `--dur-out` | bare opasitet |
| Dialog | opasitet og skala 0,97 → 1, bakteppet toner (`@starting-style`, `allow-discrete`) | `--dur-in` / `--dur-out` | bare opasitet |
| Varsel (toast) | inn: opp 8 px og 98 % → 100 %. Ut: ned 4 px. Stabelen glir på plass med FLIP (`flip()` i `ui.js`) | `--dur-3` / 0,75 × `--dur-3` | bare opasitet |
| Segmentkontroll | markøren (`.seg-ind`) glir med FLIP: ny plass settes én gang, og overgangen er bare `translateX` + `scaleX` | `--dur-2` `--ease-out` | hopp |
| Bryter | knotten glir, sporet skifter farge | `--dur-2` | hopp |
| Avkrysning, prikker for endring | haken og prikken toner og vokser litt inn | `--dur-2` | tone |
| Stolper | glattes i rAF-løkka, `translateX` (ikke `scaleX`, så den runde enden beholder formen) | ca. 0,2 s | hopp |
| Temperaturskala | markøren flyttes med `translateX` | `--dur-3` | hopp |
| Nivåfarger (gul/rød) | farge toner | `--dur-3` | samme |
| Temabytte | kryssfade av hele vinduet med View Transitions. Elementenes egne fargeoverganger er av imens (`data-switching`). Aldri ved første lasting | `--dur-vt` | 100 ms |
| Grafer | tidsaksen ruller jevnt. Y-aksen glir til nytt område ved «Tilpass» (merkene følger med). Hårkors med verdier ved hover | rulling etter piksel | trinnvis, aksen hopper |
| Viftekurve | punktet følger pekeren uten easing. Punktet vokser 1 → 1,3 ved hover og grep, og det vises hjelpelinjer til aksene mens du drar (tettere prikker med Shift-smekk). Forhåndsvalg og Forkast morfer kurven | vekst `--dur-1`, morf `--dur-4` | ingen vekst, ingen morf |
| RGB | viftehjulene er egne lag som roteres av kompositoren (fart = `playbackRate`). LED-ene ligger i en egen SVG, og bare endrede farger skrives. Regnbue og fargesyklus flytter fargen i trinn på 1,5° (v0.0.8), så bilder mellom trinnene ikke skriver noe. Bytte av effekt eller farge kryssfades. Effektprøven i lista viser effekten ved hover | morf `--dur-4` | stillbilde |
| Frakoblet-linja | glir ned og toner inn, og ut igjen før raden lukkes (`display` `allow-discrete`). Skjermlesere får én melding ved brudd og én ved retur | `--dur-3` | bare opasitet |
| «Bakgrunnstjeneste: ikke tilkoblet» (v0.0.7) | som frakoblet-linja, bare på Kontroller og Viftekurve. Sekundene i «Prøver igjen om 3 s» byttes uten tween | `--dur-3` | bare opasitet |
| Bekreftelsen etter Bruk (v0.0.7) | linja glir inn som frakoblet-linja. Den tynne aksentlinja nederst krymper med `scaleX` ett sekund om gangen (lineært, så den møter neste tall), og sekundene byttes uten tween. Etter tilbakerulling blir flaten nøytral. Skjermlesere får én melding ved start og én ved slutt, aldri per sekund | inn `--dur-3`, linja 1 s × `--move` | linja hopper hvert sekund; ved «Av» ingen overgang |
| Kopier og Bruk | hake på knappen i 1,2 s (`confirmDone()` i `ui.js`). Knappen beholder bredden. Venter endringen på Behold (v0.0.8), får Bruk ingen hake; bekreftelseslinja viser tilstanden | `--dur-3` | tone |
| «Vis på Sensorer» | raden lyser opp og toner ut | 1,6 s | samme (statisk ved «av») |
| Start og stopp (Viftekurve) | feltene glir fram under raden (sammenlegging). Linjene og båndet følger pekeren uten easing, og merkelappen fylles med aksent når linja er valgt | `--dur-3` | høyden hopper, innholdet toner |
| Nål på Sensorer | vises ved hover og fokus (opasitet) | `--dur-1` | samme |
| Rekkefølge (Innstillinger › Sidefelt) | raden følger pekeren, naboene glir unna med `translateY`. Etter flytting lyser raden kort opp på ny plass | `--dur-2`, opplysning `--dur-4` × 3 | ingen glid, ingen opplysning |

**Ikke lag:** tall som teller, sprett og elastikk, skimmer på plassholdere, parallakse, glød som pulserer, ikoner som roterer ved hover, sidefelt som glir, eller animasjon som starter av seg selv uten at noe har skjedd.

## Mikrodetaljer (sjekkliste for nye ting)

- `:focus-visible` med `--focus-w` og `--focus`. Ingen `:focus`-ring som synes ved klikk. Tab-rekkefølgen skal følge skjermen: sidefelt, sidens handlinger og så innhold.
- `cursor: default` på knapper og rader, `pointer` bare på ekte lenker i tekst, `crosshair` på grafer, `grab`/`grabbing` på kurvepunkter og `help` bare på (i).
- Grensesnitt er `user-select: none`. Verdier, tabellceller og kode kan markeres. Korte ID-er har `user-select: all`.
- Verktøytips har forklaring og eventuelt hurtigtast (`data-tip`, `data-tip-key`). Deaktiverte knapper beholder pekerhendelser, så tipset kan si hvorfor. Avkortet tekst (ellipsis) får hele teksten i tipset automatisk.
- Plassholdere (`.skel`, `skelKv`, `skelTable` i `rows.js`) har samme høyde som innholdet. Ingenting skal hoppe når data kommer, og grupper uten innhold vises ikke før de har innhold.
- Tall og enhet skilles med U+202F (`F.withUnit`), og negative tall bruker U+2212 (`F.num`). Manglende verdi er «–», og intervaller skrives med «–» uten mellomrom. Entall og flertall går gjennom `F.plural`.
- Kolonner med levende tall har fast bredde, så stolper og overskrifter ikke flytter seg når et tall får et siffer til.
- Meldinger til skjermleser går gjennom `announce()` (én høflig region), og bare for viktige endringer.
- Faviconet har egen flate og fungerer i begge temaer. Det følger ikke aksenten med vilje, fordi appikonet skal være likt overalt.
- Utseendet skrives inn i `<html>` av serveren (`data-prefs="server"`), så første tegning har riktig tema også uten lokal lagring.

## Slik måles det

`npm run check`, `npm test` og `npm run contrast`, og i tillegg headless Chrome via CDP. Rapporten fra bygg 4 står i `docs/PATCHLOG.md`.

- **Tid per bilde:** pakk `requestAnimationFrame` inn (`Page.addScriptToEvaluateOnNewDocument`) og summer tiden i tilbakekallene per bilde. Rapporter snitt og 95-persentil.
- **Layout:** `Performance.getMetrics` (`LayoutCount`, `RecalcStyleCount`) før og etter en animasjon, og `PerformanceObserver` for `layout-shift`.
- **Midt i overganger:** `Animation.setPlaybackRate` 0,1 og skjermbilde.
- **«Av»:** `document.getAnimations().length` under en runde med interaksjon skal være 0.
