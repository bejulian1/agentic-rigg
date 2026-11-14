# Versjonslogg

Nyeste utgave: **1.3.3 · bygg 22**. Nedlastinger og utgivelsesnotater ligger under [Releases](https://github.com/bejulian1/agentic-rigg/releases).

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

Ekte endring av GPU-innstillinger og fysisk RGB er ikke aktivert. Utkast/profiler og RGB-forhåndsvisning er tilgjengelige. Støtte for sensorer og spesifikasjoner avhenger av maskinvare og driver. Detaljert utviklingslogg står i [app/PATCHLOG.md](app/PATCHLOG.md).
