# Fysisk prøve av «Av» på kortlyset

På brukerens Palit RTX 2080 ble kortet mørkt i ca. 5 sekunder etter «Av» (`60=01`, RGB `000000`, lysstyrke 100) og gikk så av seg selv tilbake til fabrikklyset (fast grønt `#45C800`). Ingen transaksjon skrev grønt. Hjelperen leste fortsatt svart/100 i `6C–6F` mens kortet lyste grønt, så tilbakelesingen beviser ikke hva kortet viser. Denne prøven finner ut hvilken skrivemåte som faktisk holder seg mørk. Den bruker bare de samme registrene som appens faste farge, og svaret er brukerens øyne.

## Forutsetninger

- Appen er installert, og kortlyset er aktivert. RGB-siden viser «Klar», og kortet står på en fast farge.
- Appen eier ikke lyset: lukk appen, eller trykk «Avslutt lysstyring» eller «Slutt å holde av».
- Kjøres som vanlig bruker. Ingen sudo og ingen passord.

## Kommandoer

```sh
python3 ~/.local/share/doc/agentic-rigg/rgb-av-prove.py --liste
python3 ~/.local/share/doc/agentic-rigg/rgb-av-prove.py --kandidat 1
python3 ~/.local/share/doc/agentic-rigg/rgb-av-prove.py --bekreft
```

Hvis `XDG_DATA_HOME` er satt, ligger skriptet i `$XDG_DATA_HOME/doc/agentic-rigg/` i stedet for `~/.local/share/doc/agentic-rigg/`.

| Kandidat | Skrives | Forventning før prøven |
|---|---|---|
| 1 `svart-100` | `60=01`, `000000`, lysstyrke 100 (dagens «Av») | Kommer tilbake etter ca. 5 s |
| 2 `svart-0` | `60=01`, `000000`, lysstyrke 0 | Ukjent |
| 3 `farge-0` | `60=01`, utgangsfargen, lysstyrke 0 | Ukjent |
| 4 `nesten-svart` | `60=01`, `010101`, lysstyrke 1 | Ukjent; nesten usynlig hvis den holder |

## Forventet tid

Hver kandidat tar ca. 20 sekunder: tilkobling, skriving, 15 sekunder hold og tilbakerulling. Alle fire tar 2–3 minutter med svarene.

## Slik kjører dirigenten prøven sammen med brukeren

For hver kandidat N fra 1 til 4:

1. Be brukeren se på kortet.
2. Kjør `--kandidat N`. Skriptet skriver kandidaten, holder den i 15 s og ruller tilbake til utgangslyset.
3. Skriptet avslutter med ett spørsmål. Still det til brukeren: «Ble kortlyset slukket, og holdt det seg slukket hele tiden til det kom tilbake nå? Svar «holdt», «kom tilbake etter ca. N sekunder» eller «ble ikke slukket», og om kortet nå viser utgangslyset igjen.»
4. Når brukeren bekrefter at utgangslyset er tilbake, kjører du `--bekreft`. Først da kan neste kandidat prøves.

Uten svar ruller hjelperen selv tilbake etter 30 sekunder, også om skriptet avbrytes. Etter tilbakerulling står kortet på utgangslyset og venter på bekreftelse. Ingenting lagres som varig. Brukeren kan også kjøre alle fire alene med spørsmål i terminalen: `--alle`.

## Etter prøven

Velg skrivemåten for «Av» etter resultatet i innstillingen `rgbAv` (`svart-0`, `farge-0`, `nesten-svart` eller `svart-100`).

- **En kandidat holdt.** Bruk den. «Av» kan da også lagres som varig utgangspunkt og holder seg uten at appen er åpen. Om kortet beholder den etter omstart, er fortsatt en egen prøve.
- **Ingen kandidat holdt.** Behold standarden `hold`: svart med 0 %, skrevet på nytt hvert 2. sekund mens appen eier lyset. RGB-siden sier ærlig at kortet kan lyse igjen når appen lukkes.
