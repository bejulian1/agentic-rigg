# Innstillinger v0.0.15 · bygg 15: hele vinduet, egen navigasjon og søk

Brukeren så Innstillinger i Chrome ved 1568 × 685 og syntes det så smalt og tomt ut. Det var en vanlig side inne i appen. Sidefeltet sto til venstre, menyen for delene fløt som en smal kolonne midt på, og innholdet var en kolonne på ca. 350 px med mye luft på begge sider. Ønsket var at Innstillinger skulle dekke hele vinduet, ha sin egen navigasjon der sidefeltet står, og bruke bredden.

Skjermbilder før og etter ligger i `docs/skjermbilder/v0.0.15/` (`for/` og `etter/`).

## Valget: egen visning, ikke et lag over appen

Innstillinger er fortsatt ruten `#/innstillinger` i det samme skallet. Når den vises, settes `data-view="settings"` på `<html>`. CSS skjuler da sidefeltet, toppen av sidefeltet og toppfeltet, og Innstillinger fyller vinduet. Det er ingen ny flate over appen.

Et lag over appen (overlay) ble vurdert og valgt bort:

- **Ruting og omlasting.** Med et lag måtte appen vite hvilken side som skulle ligge under ved omlasting på `#/innstillinger`, og hashen ville sagt én ting mens skjermen viste to. Som visning er ruten den samme som før. Omlasting, lenker og nettleserens tilbake og fram virker som for alle andre sider. `theme-boot.js` setter `data-view` før første tegning, så sidefeltet ikke blinker forbi ved omlasting.
- **Fokusfelle.** Et lag trenger `inert` på appen under, en egen fokusfelle og å holde dem i takt. Når sidefeltet og siden under er skjult (`display: none`), kan Tab ikke komme dit. Feller blir da unødvendige.
- **Linjene øverst.** «Behold endringene?», «Frakoblet» og «Bakgrunnstjeneste: ikke tilkoblet» ligger i skallet. De står fortsatt over Innstillinger, over hele bredden. Under et lag ville nedtellingen for Behold ha vært skjult.
- **Ytelse.** Siden under tegnes ikke mens Innstillinger vises, akkurat som når du bytter side. Med et lag ville grafene på Oversikt ha tegnet bak.
- **360 px.** Visningen bruker det samme rutenettet som resten av appen. Et lag måtte hatt sin egen smale variant med rulling inni.

Det et lag gir gratis, er at siden under beholder rullingen og fokuset. Det er laget for hånd i `route()` (`app.js`). Når Innstillinger åpnes, huskes siden, hvor langt den var rullet og hva som hadde fokus. Tilbake gir den samme siden med den samme rullingen. Fokuset går til elementet som hadde det. Er det borte, eller sto fokuset på ingenting (Ctrl+, uten fokus), går det til tannhjulet.

### Tilbake

Fire veier tilbake, med samme resultat:

- knappen øverst til venstre (←). Verktøytipset sier hvor den går: «Tilbake til Sensorer · Esc».
- Esc. Esc lukker først en åpen popover eller dialog, og tømmer søket hvis det står noe der.
- Ctrl+, igjen. Hurtigtasten åpner og lukker.
- nettleserens tilbake

Når Innstillinger åpnes fra en side, merkes oppføringen i historikken med siden (`history.state.agFrom`). Tilbake bruker da `history.back()`, så nettleserens fram åpner Innstillinger igjen, og ingen ekstra oppføringer samles opp. Merket overlever omlasting. Er Innstillinger åpnet direkte (ny fane eller bokmerke), byttes oppføringen ut med sist brukte side, eller startsiden. Det gir ingen oppføring som tilbake-knappen i nettleseren går i ring i.

Ctrl+B gjør ingenting mens Innstillinger vises, fordi sidefeltet ikke synes. Bredden kan i stedet velges under Sidefelt › Visning.

### Bevegelse

Bytte inn og ut av Innstillinger er en litt lengre og roligere overgang enn et vanlig sidebytte: opasitet og 6 px løft over `--dur-4` (250 ms), med `--ease-out`. Ved «Redusert» er det en kort toning (100 ms) i stedet for et hopp. Ved «Av» skjer ingenting. «Automatisk» følger `prefers-reduced-motion` som før. Bredden animeres ikke. Navigasjonen i Innstillinger er like bred som sidefeltet (236 px) og har den samme toppstripen (41 px). Rammen står derfor stille når visningen byttes, og bare innholdet toner.

## Oppsett

**Navigasjonen** står der sidefeltet står, med samme flate (`--sidebar`) og linje. Øverst er tilbake og tittelen, så søkefeltet, så de sju delene med ikon, og nederst «Endringer lagres straks». Det siste svarer på spørsmålet om hvor Lagre-knappen er. Delene er en loddrett faneliste: piltaster, Home og End bytter del, og Tab går videre til innholdet. Mellom 700 og 860 px er kolonnen 220 px.

**Innholdet** har en stripe øverst som toppfeltet på de andre sidene: navnet på delen og én linje om hva som står der. Panelene er de samme som ellers i appen, med tynn ramme, lav overskriftsstripe og rader rett inni. Etiketten og forklaringen står til venstre, og kontrollen til høyre. En rad er 40 px (følger tettheten), og forklaringen er kort nok til én eller to linjer. Lengre forklaringer står som en merknad nederst i panelet. Rutenett (Gul/Rød), lister og fargevalg går over hele panelbredden.

**Bredden.** Panelene flyter i spalter (CSS `column-count`), så rekkefølgen og Tab går nedover i hver spalte, og høye paneler ikke lager hull:

| Vindu | Spalter |
|---|---|
| under ca. 1140 px | 1 |
| fra ca. 1140 px (også 1440 × 900) | 2 |
| fra ca. 1760 px (også 1920 × 1080) | 3 når delen har fire paneler eller flere, ellers 2 brede |

Tre smale spalter med en tom til høyre så dårligere ut enn to brede. Ved 1440 px er det ikke lenger død plass på sidene, og ved 1920 px fyller delene med mest innhold tre spalter.

**Smalt vindu (under 700 px, også telefon).** Tilbake, tittelen og søket står på én linje øverst. Delene står under som en rad med sju ikoner, med navnet i verktøytipset, for skjermlesere og i overskriften under. Under søk står antall treff som et lite tall på ikonet. Panelene er én spalte, og kontrollen går under teksten når panelet er smalere enn 460 px.

## Søk

Søkefeltet står øverst i navigasjonen. «/» eller Ctrl+F flytter fokus dit fra hvor som helst i Innstillinger. Søket leter i alle delene og viser treffene som vanlige paneler med kontrollene i behold, så de kan endres rett fra treffet, som i VS Code.

- **Hva det søkes i:** delens navn, panelets tittel og forklaring, radens etikett og forklaring, teksten på knappene (°F, Mørk, Ikonlist) og noen søkeord som ikke står i teksten (f.eks. «påskeegg» for Små overraskelser og «fahrenheit» for Temperatur). Valgene i en nedtrekksliste er ikke med, ellers ville «graf» funnet Startside.
- **Flere ord** må alle passe: «graf fylt» gir bare Fylt flate.
- **Lister og rutenett** (statusblokken, Varselfarger, Feilsøking) vises hele eller ikke.
- **Hvor treffet står:** «Utseende ›» foran panelets tittel. Den er en knapp som åpner delen, tømmer søket, ruller panelet fram, lyser det kort opp og gir fokus til første kontroll.
- **Markering:** ordet er markert i titlene (`--accent-soft-strong`, sjekket i `npm run contrast`).
- **Navigasjonen** viser antall treff per del. Deler uten treff dempes, og ingen del er valgt mens søket står.
- **Antall treff** står til høyre i stripen («5 treff») og leses opp av skjermlesere (`role="status"`).
- **Ingen treff:** «Ingen innstillinger passer «xyzzy».» og et forslag til ord.
- **Taster:** Enter eller pil ned går til første kontroll i treffene. Esc tømmer søket, og Esc igjen går tilbake.
- **Ytelse:** søket bygger alle delene og tar bort radene som ikke passer. Målt i headless Chrome med layout (median av åtte): 3–4 ms for «kontrast» og ingen treff, 9 ms for «graf» (16 treff) og 15–18 ms for én bokstav (40 treff). Tastetrykk samles til ett søk per bilde.

## Innholdet

Ingen innstillinger eller funksjoner er tatt bort. Id-ene som testene og `easter.js` bruker (`#set-body`, `#set-nav`, `#about-version`, `.about-debug`, `.order-row`, `.accent-swatches`, `label.row` for bryterne, seksjonsnavnene i `openSettings()`), er de samme.

- **Generelt.** «Data» heter nå «Skjermkortdata». «Bruk simulert ved oppstart» heter nå «Start med simulert GPU» og står rett under Datakilde, fordi det er det samme valget for neste oppstart. Bakgrunnstjenesten har en kort statuslinje, og forklaringen står som merknad under fristen.
- **Utseende.** Kortere forklaringer. At GNOME-aksenten ikke kan leses, står som merknad nederst i Farger, ikke i forklaringen til Aksentfarge. Forskjellen på Redusert og Av står som merknad under Animasjoner.
- **Enheter og format.** Kortere forklaringer, ellers som før.
- **Sidefelt.**
  - «Status nederst» heter «Statusblokken», med «5 av 8» i stripen.
  - Sparklinjer sto i et panel uten tittel. De står nå i «Visning», sammen med en ny rad: **Bredde** (Full eller Ikonlist). Det er det samme valget som Ctrl+B og knappen nederst i sidefeltet (`sideCollapsed`), og det gjelder i nettleseren straks, også mot en eldre server (`LOCAL_FIRST` fra v0.0.14). Under 860 px sier raden at sidefeltet uansett er en ikonlist.
  - Sider og Vis hurtigtaster er som før.
- **Grafer og sensorer.** Varselfarger er flyttet hit fra Sidefelt. Grensene gjelder sidefeltet, Oversikt, Sensorer og stolpene, ikke bare sidefeltet. Panelet har nå én rad per verdi, med Gul og Rød i hver sin kolonne under et lite fargemerke. Feltene har navn for skjermlesere («GPU-temperatur, gul grense»); før het de `gpuTempWarn`.
- **Data og tilbakestilling.** En merknad sier hva som lagres i datamappa. Tilbakestill sier «Alt i Innstillinger» i stedet for «alt på denne siden».
- **Om.** Ikon, navn, versjon og én setning står i et panel ved siden av Feilsøking, ikke sentrert som en plakat. Sju klikk på versjonen gir fortsatt utviklermodus. Lisensene står som merknad under Feilsøking.
- **Tilgjengelighet.** Forklaringen i hver rad er koblet til kontrollen med `aria-describedby`, så skjermlesere leser den sammen med navnet.

## Funn i finpussrunden

Sett med friske øyne etter at alt virket:

1. **Søket fant Startside for «graf».** Valgene i nedtrekkslista (Grafer, Grafikkort) var med i teksten det ble søkt i. Nå søkes det i etikett, forklaring, knappetekst og søkeord.
2. **Markeringen delte ord i avkrysningslistene** («Graf er»), fordi etiketten er en flex-boks og markeringen ble et eget element med mellomrom. Teksten står nå i et eget `span`.
3. **Fokusringen sto igjen på Generelt** når en annen del ble åpnet med `openSettings()` mens Innstillinger var åpne. Står fokuset i navigasjonen, følger det nå delen som er valgt.
4. **Om-panelet fikk en tom tittel i søketreffene.** Panelet har ingen stripe; i treffene får det en med bare «Om».
5. **«0–100» ble delt over to linjer** ved stor skrift. Det står nå med ordskjøter.
6. **«Endringer lagres med en gang.»** brøt til to linjer ved skrift 116 % og er nå «Endringer lagres straks.»
7. **Søkefeltet ved 360 px** var bare 104 px bredt (regelen for søkefelt i smale toppfelt), med «Søk i inn…». Det fyller nå linja og sier bare «Søk».
8. **Under søk var Generelt fortsatt valgt** i navigasjonen, selv om treffene kom fra alle delene. Ingen del er valgt mens søket står.
9. **Den gamle `.about-hero`-regelen** (sentrert, 64 px ikon fra dialogen i v0.0.9) styrte fortsatt Om. Den er tatt bort.

## Latt være

- **Delen i hashen** (`#/innstillinger?del=utseende`). Delen huskes i `localStorage` som før. En del i hashen ville gitt én historikkoppføring per del, og nettleserens tilbake ville gått gjennom delene i stedet for ut av Innstillinger.
- **Egen tast for hver del** (som 1–8 for sidene). 1–8 virker fortsatt og fører til sidene, så Innstillinger lukkes.
- **Lukkeknapp (×) i tillegg til ←.** To knapper med samme oppgave ville gitt to steder å lete. ← står der man forventer «tilbake» i GNOME og JetBrains, og Esc står i verktøytipset.
