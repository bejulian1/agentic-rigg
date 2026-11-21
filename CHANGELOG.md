# Versjonslogg

Gjeldende versjon: **1.5.1 · bygg 26**. Nedlastinger og utgivelsesnotater ligger under [Releases](https://github.com/bejulian1/agentic-rigg/releases).

## 1.5.1 · bygg 26 – 21. november 2026

- **Systemkomponenter uten terminal:** «Installer systemkomponenter …» i Innstillinger › Skjermkortstyring og på RGB-siden åpner et vindu der du velger utgivelsesarkivet (forhåndsvalgt fra Nedlastinger når det ligger der) og limer inn SHA-256 fra utgivelsessiden (`sha256:`-prefikset fra GitHub godtas). Appen kjører det samme kontrollerte bootstrap-skriptet som terminalkommandoen, via `pkexec` og polkits passordvindu: arkivet kopieres til en ny root-eid mappe og kontrolleres før noe kjøres som root. Terminalkommandoen ligger fortsatt under «Bruk terminal i stedet».
- **Én bryter:** Når komponentene mangler, åpner «Tillat endring av skjermkortet» installasjonen først og fortsetter med aktiveringen etterpå (to passordspørsmål: installasjon og aktivering).
- Hjelperne og protokollene er uendret. Systemkomponenter fra 1.5.0 (bygg 25) kan beholdes; ingen ny aktivering kreves for dem.

## 1.5.0 · bygg 25 – 21. november 2026

Første utgave som åpen kildekode (MIT), med offentlig repo, engelsk README og retningslinjer for bidrag.

- **Ti språk:** Hele brukerflaten, varsler, systemfeltet og rapporter finnes på norsk bokmål, engelsk, tysk, fransk, spansk, portugisisk (Brasil), italiensk, polsk, forenklet kinesisk og japansk. «Følg systemet» er standard; valget ligger i Innstillinger › Generelt › Språk og tas i bruk etter omstart av vinduet. Tall, datoer og klokkeslett følger språket. Bakgrunnstjenesten leser språket ved oppstart.
- **Oppgaver:** Ny side med Linux-prosesser: CPU, RSS-minne, tilgjengelig GPU-minne, PID, bruker og status, søk, sortering og prosesstre. Én bekreftet prosess kan få SIGTERM via pidfd; system- og øktprosesser, appen og klientene er vernet. Pause, søk og sidebytte bruker samme frosne prosessbilde (#7). Ctrl+Shift+Esc åpner siden.
- **Lys på kortet:** Kortbundet RGB-styring for Palit GeForce RTX 2080 Super JetStream med fast farge, av, lysstyrke og de programvaredrevne effektene pusting, regnbue og fargesyklus. Bruk/Behold, automatisk tilbakeføring, lagret utgangspunkt og visuell bekreftelse av sluttilstanden. Hvert gjenopprettingsforsøk har egen identitet (#4), og aktivering, gjenoppretting og avinstallering tåler avbrudd og omstart (#6).
- **Ingen demo i appen:** Den lokale 2D/3D-lysmodellen, valget «3D-kortet», lokal lagring av lysvalg og påskeegget «nordlys» er fjernet. RGB-siden viser bare det kortet faktisk kan styre, ellers en tydelig tomtilstand. Localhost-demoen er tatt ut av repoet.
- **Sikrere systemkomponenter:** GPU- og RGB-hjelperen installeres med én kontrollert systeminstallasjon som kopierer arkivet til en root-eid mappe og kontrollerer SHA-256 fra utgivelsessiden før noe kjøres (#5). Appen bruker bare faste, root-eide polkit-innganger uten lagret godkjenning. Administrative I2C-trinn kjører i samme systemd-sandkasse som arbeideren. Hjelperne avviser dupliserte felt. NVML-protokoll 2 beholdes, men laveste kompatible hjelperbygg er 25, så **GPU-styring må aktiveres på nytt én gang**. RGB bruker egen protokoll 2.
- **CPU-hotplug:** Sensorer, Prosessor og sidefeltet følger endrede aktive CPU-ID-er, også når antall tråder er uendret (#3).
- **Begrensninger:** RGB støtter bare Palit 100-kontrolleren på RTX 2080 Super JetStream. AMD-/Intel-GPU-backender er ikke implementert. Oversettelsene er laget og kryssjekket av AI-modeller.

## 1.4.1 · bygg 24 – 14. november 2026

- Vifter får Enkel/Avansert visning: vanlig valg av Auto/Manuell først, kurver og detaljert oppførsel under Avansert. En aktiv kurve fortsetter i Enkel visning og forklares der.
- Behold/Rull tilbake ligger fast synlig på Vifter og Klokker og effekt mens innholdet rulles.
- Klokker viser GPU-/minneklokke nå med NVIDIA-driverens rapporterte maksimum, mens GPU-justering og Minnejustering forklares som offset der 0 betyr ingen justering. Effektgrensen vises i W og prosent av kortets standard; temperaturmål, vifter og grensestøtte samles under Avansert. Hjelp følger faktisk styringsstatus.
- Oppdateringen kan bruke allerede autorisert GPU-hjelper fra bygg 23 med protokoll 2. Eldre, ukjent nyere eller annen protokoll avvises. Sikkerhetsmotoren endres ikke.
- Vifteminimum omtales som driverens manuelle område, ikke et bevis på fysisk umulig viftestopp. Automatisk Zero RPM er fortsatt ubekreftet; presiseringen innfører ingen ny styringsgrense eller Zero RPM-funksjon.

- **CPU på den faktiske maskinen:** Linux-CPU-ID-er beholdes ved hull i nummerrekken og hotplug. Prosessor, Sensorer og rapport bruker de aktive ID-ene; flere sokler og ulike cachegrupper behandles separat. Cachevarianter og antall bygger på sysfs-identitet og deling, og ukjent antall vises som ukjent. Temperatur bruker sensorens navn; L3-grupper kalles ikke automatisk CCX.
- **GPU etter funksjonsstøtte:** Klokker, effekt og vifter bruker valgt korts tilgjengelige felt og rapporterte grenser. Viftevisningen skiller ukjent antall, null vifter og én til flere vifter, uten å finne på et kort med to vifter. Kontroller og kurvevalg følger det rapporterte antallet og støtten.
- **Versjonslogg og bidrag:** Innstillinger og Om åpner en innebygd logg med de ni dokumenterte desktoputgavene, nyeste først. Om krediterer Julian, Anthropic Claude Opus 5.5 (Claude Code) og OpenAI GPT 6.1 Sol (Codex).

- **Sluttkontroll:** 410 tester bestått, 4 ignorert og ingen feil; ren workspace-clippy. Private GTK-planer bestod for versjonsloggens to innganger og rekkefølge, fast Behold etter rulling, ukjent/0/1/3 vifter og smal visning, samt Prosessor, System og Disker i lyst/mørkt. Manuell 55 % forble bekreftet etter 71 s i simulering. Produksjonsbinærene oppgir 1.4.1 bygg 24. Ingen nye GPU-skriveprøver er utført; HW-sikkerhetsmotoren er uendret.

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
