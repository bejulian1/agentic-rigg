# Sidefelt v0.0.10 · bygg 10: sammenleggbart, uten hovedmeny

Brukeren syntes hamburgermenyen og sidefeltet så klumpete ut, fikk tre skisser (A: rent sidefelt, B: som A, men kan legges sammen til en ikonlist, C: faner øverst) og valgte B. Innholdet er det samme som før. Designprinsippene fra `docs/design-pro-v0.0.9.md` gjelder fortsatt (paneler, 1 px linjer, små radier, tetthet).

Skjermbilder før og etter ligger i `docs/skjermbilder/v0.0.10/` (`for/` og `etter/`).

## Valg

**Toppen av sidefeltet** er bare appikon, navn og alpha-merket. Hovedmenyen (≡) hadde tre punkter som nå har sin egen plass.

**Foten av sidefeltet**, under statusblokken og skilt med en tynn linje: Innstillinger (tannhjul, lenke), en ?-knapp for Tastatursnarveier og knappen som legger sammen sidefeltet (panel med pil). Alle tre har verktøytips med hurtigtast (Ctrl+, · Ctrl+? · Ctrl+B).

**Om** er en egen del av Innstillinger (siste i lista): ikon, navn, versjon, kort beskrivelse, en knapp til Tastatursnarveier og feilsøkingsradene som før lå i en dialog. Snarveisdialogen har en lenke nederst til Innstillinger › Om. Det er renere enn en ny liten meny ved ?-knappen, som bare ville vært hamburgermenyen på et nytt sted. «Om»-knappen under Data er fjernet, fordi Om nå er en egen del.

**Kortvelgeren** er én rad på 28 px: ikon, navn og statusprikk (grønn = live, gul = simulert, rød = frakoblet) foran pilen. Live/Simulert og driveren står i verktøytipset, for skjermlesere (skjult tekst i knappen) og i popoveren, som virker som før.

**Ikonlista** er 56 px bred, den samme som det smale vinduet hadde fra før, og samme regler brukes nå begge steder:
- Navigasjonen viser bare ikoner, med navn og tast i verktøytipset til høyre. Seksjonene er tynne streker.
- Kortvelgeren er ikonet med prikken på hjørnet.
- Statusblokken viser korte tall med enheten under (32 / °C), uten etikett. Etiketten står i verktøytipset («GPU-temperatur · vis på Sensorer») og er skjult bare for øyet, så skjermlesere leser «GPU-temp 32 °C». Varselfargene står som før. Ingen verdi kuttes: nettrate med fire sifre og enhet (852,6 / kB/s) får plass.
- Foten står loddrett: tannhjul, ?, utvid.
- I lave vinduer (under 600 px, også 200 % zoom) viker statusverdiene, så foten alltid synes uten at sidefeltet må rulles.

**Smale vinduer** (under 860 px) er alltid ikonlist. Knappen som legger sammen er skjult der, og Ctrl+B gjør ingenting, fordi det ikke er noe å utvide til. Å åpne sidefeltet over innholdet ville gitt et nytt lag (bakgrunn, fokusfelle, Esc) for noe som ikke trengs: alle sidene, Innstillinger og snarveier nås fra ikonene. Valget brukeren har gjort, beholdes og gjelder igjen når vinduet blir bredt.

**Huskes**: `sideCollapsed` i appens innstillinger (`saveSettings`, validert i `server/store.js` som de andre). Et utdrag lagres også i `localStorage`, så `theme-boot.js` setter riktig bredde før første tegning (ingen blink ved omlasting). Ellers er serverkoden urørt.

**Bevegelse**: bredden byttes straks. Å animere bredden ville tegnet hele innholdet og alle grafene på nytt hvert bilde. I stedet toner innholdet i sidefeltet inn (opasitet, `--dur-3`). Med «Redusert» er det en kort toning, med «Av» ingenting (røyktesten for «Av» klikker nå også på knappen).

## Funn → hva som er gjort

1. **Knappen som legger sammen så trykket inn ut** i full bredde: `aria-expanded="true"` ga den den samme grå bakgrunnen som en åpen meny → Bakgrunnen er bare ved hover og trykk. Ikonet viser retningen: panel med pil inn (legg sammen) eller pil ut (utvid).
2. **200 % zoom (720 × 450 CSS-piksler)**: ikonlista med statusverdier er ca. 590 px høy, så foten med Innstillinger ville havnet under kanten og måttet rulles fram → Statusverdiene viker i vinduer under 600 px høyde (de står fortsatt på Oversikt og Sensorer).
3. **Kortvelgeren viste navnet i verktøytipset to ganger** (et tips på knappen og et på navnet, fra før) → Ett tips på knappen: «GeForce RTX 2080 · Live · driver 610.43.02».
4. **Snarveisdialogen** manglet Ctrl+B og en vei til Om → Ctrl+B står under Navigasjon, og nederst står «Versjon og feilsøking står under Innstillinger › Om» som lenke.
5. **Tab-rekkefølgen**: foten kommer etter statusblokken og før toppfeltets handlinger, som på skjermen. Røyktesten sjekker rekkefølgen og synlig fokusring på alle tre.

### Sett og funnet i orden

- Full bredde og ikonlist i lyst og mørkt ved 1440 × 900 og 1100 × 800, alle ni sider uten vannrett rulling. 360 px, 200 % zoom, høy kontrast og 1440 × 700.
- Kortvelgerens popover i begge bredder, snarveisdialogen med lenken, Innstillinger › Om, Ctrl+, og Ctrl+?.
- `npm run contrast`: ingen nye farger, og dårligste tekst og UI-grense er som før (4,51:1 og 3,01:1).

### Latt være

- **Serveren skriver ikke `data-side` inn i `<html>`**, slik den gjør med tema og tetthet. Oppdraget var å la serverkoden være utenom `store.js`. Utdraget i `localStorage` dekker omlasting i samme nettleser; en helt ny nettleser viser full bredde et øyeblikk før innstillingene er lest.
- **Ingen egen tast for Om.** Innstillinger › Om er to tastetrykk unna (Ctrl+, og så Om), og snarveisdialogen lenker dit.
