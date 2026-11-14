# Versjonslogg

Nyeste utgave: **1.4.0 · bygg 23**. Nedlastinger og utgivelsesnotater ligger under [Releases](https://github.com/bejulian1/agentic-rigg/releases).

## 1.4.0 · bygg 23 – 14. november 2026

- Implementert kortbundet NVML-styring av effektgrense, kjerne-/minneoffset og GPU-vifter der driveren støtter funksjonen. Temperaturmål har egen støttekontroll.
- Én administratoraktivering erstatter fire godkjenningstrinn. Installering alene aktiverer ingen styring; endringer krever Bruk og Behold.
- PCI/UUID-binding, varig opptak av opprinnelige innstillinger, tilbakelesing og feilhåndtering ved delvis endring. Uverifisert gjenoppretting sperrer videre styring og beholder hjelpefilene.
- Korrigert PCIe x16-kant i 2D/3D-modellen, ryddigere nedtrekksmenyer og bedre fokus/tømming i sensorfilteret.
- En åpen app følger nå `--innstillinger DEL` og oppdager ny hjelperaktivering også mens styringen er låst.
- Brukeren har gjennomført ekte polkit-aktivering. Begrenset prøve mot NVML-hjelperen verifiserer effekt 245 → 240 W, begge vifter til 50 %, kjerneoffset −15 MHz og minneoffset −10 MHz, med fersk tilbakelesing, revert og reset til utgangspunktet. Gjennom installert daemon er også 240 W + Behold, reset til 245 W og automatisk tilbakerulling etter 10 s uten Behold verifisert med fersk NVML-getter. En flat 50 %-kurve styrte begge vifter i 4 s; temperaturmål 83 → 82 °C med revert/reset er også verifisert. Belastnings-/feilprøver gjenstår.
- RGB er fortsatt forhåndsvisning; lyskontrolleren er ikke identifisert.

- Sluttkontroll: 388 beståtte tester, 4 ignorerte, ingen feil og ren workspace-clippy. Endelig native UI-plan bestod; installert app viser klar, riktig kortbundet hjelper.

## 1.3.3 · bygg 22 – 14. november 2026

- Lanseringsdato i Prosessor og eksportert rapport, med kilde til produsenten.
- Ryzen 7 2700X viser 19. april 2018. Ukjente modeller får ingen antatt dato.
- Spesifikasjonsgjennomgang: RAM-moduler, skjerm og nettverksdetaljer har størst forbedringspotensial.
- Første nedlastbare GitHub Release, med ferdigbygde Ubuntu-binærer, sjekksummer og tre skjermbilder i README.

## 1.3.2 · bygg 21 – 14. november 2026

- Adaptive grafer tar med ferske topper og verdier ved tidsvinduets kanter.
- Riktig kurvehelning når en topp forlater vinduet.
- Viftekurvens målte punkter får plass også utenfor redigeringsområdet; aksen holdes rolig under dragging.
- Tilpass/Fast-forklaringen tydeliggjør at skalaene utvides ved behov.

## 1.3.1 · bygg 20 – 13. november 2026

- Kortmenyen under Skjermkortstyring følger sidefeltets bredde.
- Klikk på GPU-navnet øverst åpner Grafikkort direkte.
- Ikonene i sammenlagt sidefelt, inkludert Innstillinger, er sentrert.
- Innstillingssøk slipper fokus ved klikk utenfor; Esc og raske seksjonsbytter gir ikke gamle søketreff.

## 1.3.0 · bygg 19 – 9. november 2026

- Oversikt over hele maskinen med komponentfliser, tilstand og målegrafer.
- Utvidbare måleakser, diskvisning uten unødvendig tomrom og tydelig kortidentitet under styring.

## 1.2.0 · bygg 18 – 9. november 2026

- Forberedt hjelper og innstillingsflyt for skjermkortstyring; ekte skriving fortsatt låst.
- Viftekurver med hysterese, rampe, utjevning og mulighet for egen kurve per vifte.

## 1.1.0 · bygg 17 – 9. november 2026

- Disker med SMART, egen prosessorside, oppdatert vindusskall og styringsvisning.

## 1.0.0 · bygg 16 – 7. november 2026

- Første produksjonsutgave av Rust/GTK-appen: overvåking, sensorer, grafer, historikk, logging og rapport.

## Begrensninger i denne utgaven

Begrenset NVML-skriving og gjenoppretting er verifisert mot hjelperen på RTX 2080. Daemonvideresending, Behold og 10 s-frist er verifisert for effektgrensen. Alle grenser, positiv overklokking, stresstest, drivertap og strømbrudd er ikke verifisert. Tilgjengelighet av hvert felt avhenger av driver og kort, og oppdagelse utfører ingen testskriving. Fysisk RGB er utilgjengelig; utkast/profiler og RGB-forhåndsvisning er tilgjengelige. Støtte for sensorer og spesifikasjoner avhenger av maskinvare og driver. Detaljert utviklingslogg står i [app/PATCHLOG.md](app/PATCHLOG.md).
