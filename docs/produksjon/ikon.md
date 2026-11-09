# Appikonet til Agentic Rigg

Skrevet 7. november 2026 av ikonagenten. Gjelder den ekte appen (`io.github.AgenticRigg`). Demoens ikon i `web/icons/` er ikke endret.

## 1. Kort fortalt

- **Valgt motiv: «Vifte i måler».** En skjermkortvifte med fem blad står i en måleskala på 270°, der tre firedeler er fylt med aksentfargen. Gullkontakten (PCIe) stikker ut under flisen. Viften og kontakten sier «skjermkort», og skalaen sier «måler og overvåker».
- Samme flis som Agentic Klipp: 104 × 96 med 4 px underkant, 20 px hjørner, 12 px marg og samme lyse kant. Klipp er blå med hvite hjørner, Rigg er grafitt med blå måler. De hører sammen uten å kunne forveksles.
- Det er ingen regnbue, ingen glød og ingen bokstaver. Den eneste gradienten er den svake toningen i flaten, slik GNOME-ikonene og Klipp har.
- Hovedikonet er håndtegnet SVG på 2 kB. I tillegg finnes egne SVG-er tegnet på pikselrutenettet for 16, 24, 32 og 48 px. GTK velger dem selv, og det er målt (kap. 4).
- Det er ti varianter: Standard, én per aksentfarge (Blå, Blågrønn, Grønn, Lilla, Rosa, Skifer), Mørk, Lys og Monokrom. Alle lages fra én mal per størrelse med `lag-varianter.py`.
- **Bytte uten å logge ut virker, men ikke slik det var foreslått.** Å skrive over ikonfila og touche `hicolor/` gjør ingenting i GNOME Shell 50, heller ikke etter 12 sekunder. Det som virker, er at hver variant har sitt eget ikonnavn, og at appen skriver om `Icon=` i `.desktop`-fila. Da bytter dokken og resten av Shell ikonet etter ca. 5 sekunder. Se kapittel 7.

## 2. Skisser og valg

Fire skisser ble tegnet og sett i 16, 24, 32, 48, 64 og 128 px på lys og mørk bakgrunn, og 16 og 24 px forstørret ×6 (`ikon/skisser.png`, kildene ligger i `ikon/skisser/`).

| Skisse | Hva | Vurdering |
|---|---|---|
| A Vifte | Flis med én stor vifte (sju blad) og kontakten under. Videreføring av v0.0.6. | Tydelig ved 48 px og over. Ved 16–24 px blir sju hvite blad en grå klatt, og den sier «vifte» mer enn «overvåking». |
| B Skjermkort | Kortet sett forfra i bredformat, to vifter, brakett og kontakt. | Lettest å kjenne igjen som GPU ved 64 og 128 px. Det brede formatet gjør det lite ved 16–32 px, og to vifter flyter sammen. Det er også det mest «gamer»-aktige. |
| C Vifte i måler | Flis med en måleskala rundt en vifte. | Den blå buen er det som leses best av alle elementene ved 16–32 px. Den sier både maskinvare og overvåking og er ulik alle andre ikoner i appnettet. **Valgt.** |
| D Brikke | En brikke med bein og en pulskurve. | Generisk «CPU/systemovervåker». Beina blir uskarpe ved små størrelser. |

Runde 2 (bare C) prøvde 7, 6 og 5 blad, med og uten kontakt. Fem brede blad var klarest ved 24–32 px. Kontakten kom med igjen fordi den gjør flisen til et skjermkort også ved 16 px, der viften bare er en prikk. Flisen ble 4 enheter lavere, så kontakten holder seg innenfor 12 px-margen (bunn på 116).

Etter runde 2 ble stripene på kontakten fjernet. De lå bak underkanten og viste seg bare som prikker ved 512 px.

## 3. Hovedikonet

`app/data/icons/hicolor/scalable/apps/io.github.AgenticRigg.svg` (laget av `varianter/maler/128.svg`):

| Del | Mål (128-rutenett) | Farge (Standard) |
|---|---|---|
| Flis | x 12–116, y 12–108, r 20 | `#55555b` → `#424247` (svak toning), lys kant 2 px med 14 % hvitt |
| Underkant | 4 px, bunn på 112 | `#2a2a2e` |
| Kontakt | x 44–84, y 102–116, r 3 | `#e5a50a` og `#c88800` (Adwaita gul) |
| Måler | sentrum (64, 60), r 35, strek 11, rund ende, fra −135° til +135° | spor `#2e2e32`, fyll til +55° i `#2a75cf` (`--accent-bg`) |
| Vifte | brønn r 25, fem blad fra r 7 til r 23 | brønn `#2e2e32`, blad `#f6f5f4` |
| Nav | r 8 og r 3 | `#2a75cf` og `#559cf7` (`--a-d`) |

Kantene på flisen ligger på hele piksler ved 32, 64, 96 og 128 px. Ved 48 px havner de på halve piksler, og derfor har 48 px egen fil (kap. 4).

Fargen i Standard er appens egen blå (`--accent-bg`), ikke GNOME-blå. Grafitt er det samme som standardtemaet i appen.

## 4. Små størrelser (finpuss)

GNOME skalerer SVG-en ned, og da blir kantene uskarpe der rutenettet ikke går opp. Derfor er 16, 24, 32 og 48 px tegnet på nytt på sitt eget pikselrutenett (`varianter/maler/{16,24,32,48}.svg`):

- **16 px:** flis 14 × 12 med 1 px underkant og 1 px gullstripe. Måler med strek 2. I midten en skive på 4 px med navet i aksentfargen (bladene får ikke plass). Lys variant har mørk skive, ellers forsvinner den mot den lyse flisen.
- **24 px:** flis 20 × 18, måler med strek 2, vifte med fem blad (r 1,2–3,8) og nav på 3 px.
- **32 px:** flis 26 × 24, lys kant som én hel piksellinje, måler r 8,5 med strek 3, så buen går fra hel piksel til hel piksel (5–26).
- **48 px** (standardstørrelsen i Ubuntu-dokken): flis 40 × 36 på hele piksler (fra x 4, ikke 4,5), måler r 13 med strek 4, brønn r 9 med to piksler luft til buen.

Sammenligning skalert mot håndtegnet: `ikon/finpuss-16-48.png`. Forskjellen synes best ved 16, 24 og 48 px.

Målt med GTK 4.22 (`Gtk.IconTheme.lookup_icon` mot `app/data/icons`): 16 → `16x16`, 24 → `24x24`, 32 → `32x32`, 48 → `48x48`, 64, 96 og 128 → `scalable`, og 16 px ved skala 2 → `32x32`. Mapperekkefølgen i `hicolor/index.theme` setter faste størrelser foran `scalable`, og GNOME Shell bruker den samme regelen.

## 5. Symbolsk ikon

`hicolor/symbolic/apps/io.github.AgenticRigg-symbolic.svg`, 16 × 16, én farge (`#2e3436`, som GTK farger om). Det er bare fylte flater og ingen streker. Måleren er en ring fra 5 til 7 px med åpning nederst. Sporet har 35 % dekning, og den fylte delen går til +55° som i hovedikonet. Viften har tre blad og nav, fordi fire blad ble for tett ved 16 px (`ikon/symbolsk.png`).

## 6. Varianter

Alle lages av `app/data/icons/lag-varianter.py` fra malene i `varianter/maler/`. En variant er bare et sett farger, så formen kan ikke gli fra hverandre.

| id | Navn | Ikonnavn | Flis | Måler |
|---|---|---|---|---|
| `standard` | Standard | `io.github.AgenticRigg` | grafitt | `#2a75cf` |
| `blue` | Blå | `io.github.AgenticRigg-blue` | `#2a75cf` | hvit |
| `teal` | Blågrønn | `…-teal` | `#138093` | hvit |
| `green` | Grønn | `…-green` | `#308540` | hvit |
| `purple` | Lilla | `…-purple` | `#9141ac` | hvit |
| `pink` | Rosa | `…-pink` | `#b95083` | hvit |
| `slate` | Skifer | `…-slate` | `#5f7184` | hvit |
| `dark` | Mørk | `…-dark` | `#1c1c1f` | `#559cf7` (aksent i mørkt) |
| `light` | Lys | `…-light` | `#f0eff1` med mørk kantlinje | `#2169be` (aksent i lyst) |
| `mono` | Monokrom | `…-mono` | grafitt | `#deddda`, grå kontakt |

Aksentfargene er nøyaktig `--accent-bg` fra `web/styles.css` (seksjon 2). Toppen av toningen er den samme fargen blandet med 14 % hvitt, og underkanten med 40 % svart. Navet bruker `--a-d`. Id-ene er de samme som `accent` i innstillingene, så «Som aksentfargen» kan legges til senere uten nye navn.

```
cd app/data/icons
./lag-varianter.py                 # skriv varianter/, varianter.json og hicolor/ på nytt
./lag-varianter.py --sjekk         # endrer ingenting, avslutter med 1 hvis noe er utdatert (til tester)
./lag-varianter.py --forhandsvis ../../../docs/produksjon/ikon
```

Utdata: `varianter/<id>/{128,48,32,24,16}.svg`, `varianter/varianter.json` (id, navn, beskrivelse, ikonnavn og hvilken hicolor-mappe hver størrelse hører til) og standardvarianten i `hicolor/`. Skriptet trenger bare Python 3. Forhåndsvisningene trenger i tillegg `python3-gi` med GdkPixbuf (samme SVG-tegner som GNOME, librsvg 2.61) og Pillow. Alle 61 SVG-filene er sjekket som gyldig XML. Til sammen er variantene 86 kB.

## 7. Bytte ikon mens appen kjører

### 7.1 Slik ble det testet

GNOME Shell kan ikke observeres fra et skript i brukerens egen økt (Eval og skjermbilde er stengt). Derfor ble testen kjørt i en **egen, hodeløs GNOME Shell 50.1** (samme binær som brukerens, `--headless --mode=ubuntu`, med Ubuntu-dokken). Den hadde egen D-Bus-økt og egne `HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME` og `XDG_RUNTIME_DIR`. En midlertidig utvidelse i den økta tok skjermbilder og logget `icon-theme-changed`. Testappene het `io.github.AgenticRigg.ikontest*`. Alt lå i arbeidsmappa til agenten og er stoppet. Brukerens Shell, Klipp og andre apper er ikke rørt.

Én bieffekt skal nevnes: den hodeløse Shell meldte seg til GDM ved oppstart («Registering session with GDM»), to ganger. Kallet er idempotent, og ingenting i brukerens økt endret seg, men det er grunnen til at testen ikke ble startet flere ganger.

GTK-siden ble testet både i den egne økta og i brukerens ekte `~/.local` med navnet `io.github.AgenticRigg.ikontest`. Den fila er fjernet igjen. Den eneste varige endringen er at mtime på `~/.local/share/icons/hicolor/` er satt til 7.11. kl. 10:08 (innholdet er uendret).

### 7.2 Resultater

Bildebevis: `ikon/gnome-test.png`.

| Forsøk | Hva | Resultat i Shell |
|---|---|---|
| A | Samme navn. Fila overskrevet atomisk, og `hicolor/` touchet. Appnettet åpnet. | **Uendret** etter 12 s. Ingen `icon-theme-changed`. |
| A' | Som A, pluss `touch` på `.desktop`-fila, eller fila skrevet om med samme innhold eller med en ny `X-`-nøkkel | Uendret etter 8 s |
| B1 | `Icon=` skrevet om til et navn som temaet allerede kjenner | **Byttet etter ca. 5 s** (mellom 5 og 6 s) |
| B2 | `Icon=` skrevet om til en fil som ble lagt til etter at temaet ble lest, uten touch | **Reserveikon** (grått tannhjul) |
| B3 | Ny fil, `hicolor/` touchet, så `Icon=` skrevet om | Temaet lastet på nytt med en gang. Riktig ikon etter ca. 5 s, og A-ikonet fra forsøk A ble også riktig. |

GTK 4.22, i egne prosesser (både i testøkta og i brukerens ekte økt):

| Tilstand | `has_icon` for ny fil |
|---|---|
| Ingen buffer, `hicolor/` ikke touchet, prosessen kjører | Nei, heller ikke etter 7 s |
| Ingen buffer, `hicolor/` touchet, prosessen kjører | Ja innen 7 s |
| Gyldig `icon-theme.cache`, ny fil, ikke touchet | Nei, heller ikke i ny prosess |
| Gyldig buffer, `hicolor/` touchet i et senere sekund | Ja. Bufferen regnes som utdatert og overses. |
| `gtk-update-icon-cache --ignore-theme-index --force ~/.local/share/icons/hicolor` | Virker uten root (exit 0) |

### 7.3 Hvorfor

- GNOME Shell har en teksturbuffer per ikonnavn og størrelse. Så lenge navnet er det samme, tegner den det den har, uansett hva som ligger på disk.
- Ikontemaet (St, en kopi av GTK3s) sjekker mtime på temamappene (`…/icons/hicolor`) bare når den slår opp et navn som ikke er i bufferen, og høyst hvert 5. sekund. Å legge en fil i `scalable/apps/` endrer ikke mtime på `hicolor/`. Derfor må `hicolor/` touches når en ny fil legges til.
- Når temaet lastes på nytt, tømmes teksturbufferen, og da blir også overskrevne filer riktige (B3). Men bare `touch` utløser ingen ny lesing, fordi ingenting slår opp et nytt navn (A).
- Et nytt navn i `.desktop` gjør at Shell (via `GAppInfoMonitor`, ca. 4–5 s forsinkelse) lager ikonet på nytt med et navn som ikke er i bufferen. Hvis innholdet i fila er likt eller `Icon=` er uendret, skjer ingenting (A').
- `mtime` sammenlignes på hele sekunder. Bufferfila må være eldre enn mappa med minst ett sekund for å regnes som utdatert.

### 7.4 Anbefalt løsning

1. **`install.sh` installerer alle variantene**, hver med sitt eget navn (`varianter.json` → `ikonnavn`) i `scalable`, `48x48`, `32x32`, `24x24` og `16x16`, pluss det symbolske ikonet. Det er 51 filer og ca. 90 kB. Til slutt `touch ~/.local/share/icons/hicolor`.
2. **`.desktop`-fila skrives med `Icon=<ikonnavnet til valgt variant>`.** Standard er `io.github.AgenticRigg`. Ved ny installasjon beholdes valget (leses fra innstillingene).
3. **Når brukeren velger en variant**, gjør appen dette (rekkefølgen er viktig):
   1. Sjekk at variantens fem filer finnes. Mangler noen, skriv dem atomisk fra GResource og `touch` `hicolor/` etterpå.
   2. Skriv `~/.local/share/applications/io.github.AgenticRigg.desktop` atomisk (ny fil og `rename`) med ny `Icon=`. Resten av fila er lik.
   3. `gtk::Window::set_default_icon_name(ikonnavn)` og lagre valget i innstillingene.
   4. Vis eventuelt et varsel: «Appikonet byttes i dokken og Vis programmer om noen sekunder.»
4. **Ikke kjør `gtk-update-icon-cache` i `~/.local`.** Uten buffer leser GTK og Shell mappene direkte, og det tar ingen merkbar tid for noen titalls filer. En gyldig buffer i `~/.local/share/icons/hicolor` gjør derimot at ikoner som andre apper legger til senere uten å oppdatere bufferen, blir usynlige (målt: «gyldig buffer, ny fil» over). Brukerens buffer er i dag utdatert (fra 6.11. kl. 16:02, mappa er nyere) og blir derfor oversett. Det er greit. `update-desktop-database` er ufarlig og kan bli.
5. **Vinduesikonet:** På Wayland finner GNOME Shell appen fra app-id-en (`io.github.AgenticRigg`), som må være lik navnet på `.desktop`-fila. Ikonet i dokken, Alt+Tab og Vis programmer kommer fra `Icon=` der. `set_default_icon_name` er likevel riktig for egne dialoger og X11.

Den foreslåtte metoden, altså å skrive valgt variant over `io.github.AgenticRigg.svg` og touche mappa, virker bare etter ny innlogging, eller tilfeldig når Shell slår opp et annet nytt ikon minst 5 s senere. Den er derfor ikke valgt.

### 7.5 Avinstallering

- `uninstall.sh` fjerner `.desktop`-fila, alle `io.github.AgenticRigg*.svg` under `hicolor/*/apps/` og det symbolske ikonet, og touch-er `hicolor/`. Appen forsvinner fra Vis programmer og dokken etter ca. 5 s (`GAppInfoMonitor`). Er den festet i dokken, forsvinner den derfra også.
- Rør ikke `icon-theme.cache` eller andre apps filer. Lag ikke en ny buffer (se 7.4).
- Valget av variant ligger i innstillingene (`~/.config/agentic-rigg/`) og blir stående, slik resten av innstillingene gjør. Neste installasjon bruker det igjen.
- Blir ikonfilene borte mens `.desktop` står igjen, viser Shell reserveikonet etter neste lesing av temaet (B2).

### 7.6 Begrensninger

- Bytte krever at appen eier `.desktop`-fila i `~/.local/share/applications`. Som Flatpak går det ikke, fordi den eksporterte fila er skrivebeskyttet. Ikonnavnene (`io.github.AgenticRigg-*`) er likevel gyldige for Flatpak-eksport.
- Endres utseendet på et ikon i en ny versjon under samme navn, viser Shell det gamle til neste innlogging eller neste bytte av variant. `install.sh` kan løse det ved å skrive `.desktop` med et annet navn og så det riktige, men det er ikke nødvendig nå.
- Selve Vis programmer ble ikke fotografert med testikonet på første side. Det bruker de samme ikonene og den samme teksturbufferen som dokken, og dokken og et eget `St.Icon` ble fotografert.

## 8. Innstillingsraden

Plassering: **Innstillinger › Utseende › Farger**, rett under Aksentfarge. Skisse: `ikon/innstillinger.png` (lyst og mørkt, laget med demoens `styles.css` i hodeløs Chrome).

- Rad med tittel **«Appikon»** og undertekst «Ikonet i Vis programmer, dokken og Alt+Tab. Byttet vises etter noen sekunder, uten å logge ut.» Kontrollen ligger under teksten (`row wrap`), som Fargetema.
- Rutenett av kort i samme mønster som Fargetema (`.pal-cards`/`.pal-card`/`.pv-frame`/`.pv-check`/`.pal-name`): kolonner på minst 72 px, mellomrom 8 px loddrett og 12 px vannrett. Ti kort gir én rad ved 860 px og to rader i smalere vinduer.
- Hvert kort: en flate på 64 px høyde (`--card-alt` med 1 px `--border-soft`, radius `--r`) med ikonet i **48 px** midt i, og navnet under (`--fs-sm`, midtstilt). Valgt kort har 2 px ring i aksentfargen, hake øverst til høyre og navnet i halvfet, akkurat som Fargetema. Pekeren over gir 1 px `--border`.
- Forhåndsvisningen tegnes fra GResource (`varianter/<id>/48.svg`), ikke fra ikontemaet. Da vises den riktig også før appen er installert. 48 px er den håndtegnede versjonen.
- GTK4: `GtkFlowBox` (`selection-mode: single`, `homogeneous`, `min-children-per-line 5`, `max-children-per-line 10`, `activate-on-single-click`). Hvert barn har tilgjengelig navn «Blå: Flis i blå». Selve boksen har navnet «Appikon». Piltastene flytter, og Enter og mellomrom velger. Søkeord til innstillingssøket: «ikon appikon dokk vis programmer farge».
- Haken ligger 5 px utenfor kortet. Rutenettet trenger derfor 6 px luft over seg, ellers klippes haken av raden (synes i skissen i lyst tema).

## 9. Filer

| Sti | Hva |
|---|---|
| `app/data/icons/hicolor/scalable/apps/io.github.AgenticRigg.svg` | hovedikonet (Standard) |
| `app/data/icons/hicolor/{16x16,24x24,32x32,48x48}/apps/io.github.AgenticRigg.svg` | pikseltegnede små størrelser |
| `app/data/icons/hicolor/symbolic/apps/io.github.AgenticRigg-symbolic.svg` | symbolsk ikon |
| `app/data/icons/varianter/maler/{128,48,32,24,16}.svg` | malene med `{{plassholdere}}` |
| `app/data/icons/varianter/<id>/…` og `varianter.json` | de ti variantene |
| `app/data/icons/lag-varianter.py` | generatoren |
| `docs/produksjon/ikon/skisser.png` og `skisser/` | de fire skissene |
| `docs/produksjon/ikon/storrelser.png` | 16–512 px på lyst og mørkt |
| `docs/produksjon/ikon/varianter.png` | alle variantene, 128 og 48 px, lyst og mørkt |
| `docs/produksjon/ikon/bakgrunner.png` | lyst og mørkt vindu, mørk oversikt og grå dokk, ved siden av Klipp |
| `docs/produksjon/ikon/finpuss-16-48.png` | skalert mot håndtegnet, forstørret ×6 |
| `docs/produksjon/ikon/symbolsk.png` | symbolsk ikon, tre og fire blad |
| `docs/produksjon/ikon/gnome-test.png` | testen i GNOME Shell (kap. 7) |
| `docs/produksjon/ikon/innstillinger.png` | innstillingsraden |

## 10. Til de andre agentene

`arkitektur.md` kap. 12 bør justeres etter dette:

- Punkt 3: størrelsene er `scalable`, `48x48`, `32x32`, `24x24` og `16x16` (48 px er lagt til for dokken).
- Punkt 6 og avinstallering punkt 2: `touch ~/.local/share/icons/hicolor` i stedet for `gtk-update-icon-cache` (se 7.4 punkt 4).
- Byttet virker bare når `Icon=` får et *annet* navn. Det må ikke byttes ved å skrive over fila (7.4 punkt 3).

`lag-varianter.py --sjekk` kan legges inn i `cargo test` eller CI, så ingen endrer en variant for hånd.
