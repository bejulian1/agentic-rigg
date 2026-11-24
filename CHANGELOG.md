# Versjonslogg

Gjeldende versjon: **1.7.0 · bygg 33** (forhåndsutgivelse til den er prøvd på kortet). Nedlastinger og utgivelsesnotater ligger under [Releases](https://github.com/bejulian1/agentic-rigg/releases).

## 1.7.0 · bygg 33 – 24. november 2026

- **Nytt navn:** Appen heter nå **Agentic HW Monitor**. Bare visningsnavnet er endret. Kommandoene, app-ID-en `io.github.AgenticRigg`, repoet og alle mapper og systemstier er de samme, så innstillinger, historikk, logger og en aktivert hjelper følger med (PR #32).
- **Svensk** er det ellevte språket, med svenske tall, datoer og søkeord. Oversettelseskontrollen sjekker nå også katalogene for alle språk (PR #32).
- **Nettverk:** Ny side under Maskinvare med aktiv tilkobling, ned og opp nå, ruting og DNS, Wi-Fi, ett utfoldbart kort per grensesnitt med egen graf, og porter som lytter. Siden bare leser (PR #38).
- **Oppgaver:** Alle prosesser vises, også root og andre brukere. Minnekolonnen viser privat minne (`RssAnon`), så summen stemmer med systemet, og RAM-kortet viser hva minnet brukes til (prosesser, delt/tmpfs, kjerne, cache og swap). Prosessene grupperes etter navn, og et detaljfelt viser kommando, bruker, minne, PSS/USS og disk-I/O for valgt prosess. Nye kort for CPU og GPU, og bunnfeltet forklarer hvorfor en prosess er beskyttet (PR #37, #43).
- **Temperaturakser** går alltid til minst 100 °C (212 °F) og utvides når verdiene går over (PR #34).
- **Layout over hele appen:** Spaltene velger antall etter plassen, innstillingsradene brytes, sidefeltet legger seg ikke lenger over innholdet, grafhodene kutter ikke titlene, flisene står i hele rader, og menyene bryter lange navn. En maskinell kontroll av widgettreet ble kjørt på flere språk og bredder (PR #34, #40).
- **Klokker og effekt:** Under alle gliderne står nå min, standard og maks. Tidligere manglet maks på effektgrensen og min på minnejusteringen. Når plassen er knapp, står enheten bare ved standard.
- **Om** krediterer Antigravity ved riktig navn (PR #34).
- **Aktivering av GPU-styring rettet:** I 1.6.0 stoppet bryteren når en RGB-økt var uavklart, og en oppgradering kunne kreve fire til seks passord. Nå oppdateres bare GPU-delen, RGB-delen står urørt, og oppgraderingen fra bygg 31 tar ett passord for GPU og ett for RGB (PR #37, #43).
- **RGB-lys:** Egen bryter for kortlyset, fargevelger med hele spekteret, og «Av» som holder: appen skriver svart på nytt hvert 2. sekund mens den eier lyset. En fast farge eller «Av» kan beholdes også etter at appen lukkes, uten egen tjeneste; effekter kjører bare mens appen er åpen. Etter krasj eller omstart kommer «Av» tilbake uten passord og uten låst økt. Prøveskriptet `rgb-av-prove.py` (installeres i `~/.local/share/doc/agentic-rigg/`) prøver fem måter å slå lyset av på, med automatisk tilbakerulling (PR #45, #47).
- **Hjelpere:** GPU-hjelperen er uendret siden 1.6.0, så en hjelper fra bygg 32 virker videre uten nytt passord (minimum 32, protokoll 2). En RGB-hjelper uten de nye egenskapene (bygg 29–32) vises som «må oppdateres», og «Bruk» på RGB-lys oppdaterer den med ett passord.
- **Begrensninger:** Utgaven er en forhåndsutgivelse til den er prøvd på kortet. «Av» som holder, varig «Av» etter at appen lukkes, gjenopprettingen etter krasj og hva kortet viser etter en kald omstart er bare prøvd mot simulerte kort og en falsk hjelper, ikke på ekte maskinvare. Selve installasjonen med ekte `pkexec` og nedlasting fra GitHub er dekket av kjedede tester med falske systemkall. `SHA256SUMS` er fortsatt ikke signert (issue #19).

## 1.6.0 · bygg 32 – 23. november 2026

- **Sikrere GPU-hjelper:** Fristene for Behold, livssignalet og oppdagelsen av hvile følger en monoton klokke (`CLOCK_BOOTTIME`) i stedet for veggklokka, så de verken fryser eller tilbakestilles når klokka stilles. Nødregelen over 85 °C godtar aldri lav manuell vifte som gjenoppretting: kan 100 % ikke skrives, settes kortet til grunnverdiene med bekreftet Auto, ellers meldes feil. Gjenopprettingsmerkøren (`dirty.json`) lagrer antall vifter, og den slettes aldri når den ikke kan bekreftes, heller ikke ved stopp, endret oppsett eller tilbakestilling. Er merkøren ubekreftet, tar hjelperen ikke imot nye endringer før den er startet på nytt. Socketløkka begrenser tilkoblinger, linjer og svartid per runde.
- **Overtemperaturvern i feiltilstand (issue #20):** Har en gjenoppretting feilet, leser hjelperen fortsatt temperaturen. Ved 85 °C eller mer, eller når temperaturen ikke kan leses, forsøker den grunnverdiene med Auto-vifter straks og så med økende mellomrom (opptil 60 s) til standard er bekreftet. Den skriver aldri andre verdier og åpner ikke for nye endringer. Feiltilstanden står til du selv tilbakestiller.
- **Én ny administratoraktivering:** Rettingene ligger i hjelperen, så GPU-hjelpere fra bygg 25–31 vises som utdatert (minimumsbygget er 32, protokoll 2 er uendret). Slå på «Tillat endring av skjermkortet» i Innstillinger › Skjermkortstyring og skriv administratorpassordet én gang. Systemkomponentene oppdateres, og GPU-styringen aktiveres igjen. RGB-minimumet 29 er uendret.
- **RGB-avklaring etter observert lys (issue #17):** Når en uavklart RGB-økt avsluttes under oppdateringen, settes kortet først tilbake til det lagrede lyset og leses av. Deretter spør appen om du ser det lyset. Først når du bekrefter, avklares økten. Lukker du dialogen, eller kortet er byttet, beholdes økten og ingenting byttes.
- **Felles systemlås (issue #18):** Tillatelsene leses før nedlastingen og på nytt etterpå. Har noe endret seg, stopper installasjonen før en fil er byttet. Installasjon, NVML-gjenaktivering og valgt aktivering skjer under én felles systemlås.
- **Festede systemfiler (issue #19, delvis):** Pakken har `SYSTEMFILER.sha256`, som `install.sh` legger hos brukeren. Ved oppdatering fra appen må den nedlastede utgaven ha nøyaktig de systemfilene den installerte appen kjenner, så en utgave som er byttet ut etter installasjonen avvises. Arkivet kontrolleres strengere (én rotmappe, bare vanlige filer, samme versjon og bygg, avgrenset størrelse).
- **Systemfelt og installasjon (PR #22):** Stopp av bakgrunnsopptak fra systemfeltet beholder holdte GPU- og RGB-verdier. RGB-aktiveringen reparerer manglende systemfiler, tilbyr oppdatering av en utdatert hjelper og avbryter hvis behovet endres mens dialogen er åpen. Installasjon tilbys ikke over et nyere installert bygg, og systeminstallereren avviser nedgradering før noe endres. Arkiveringen kontrollerer alle NVIDIA-kort etter UUID.
- **Opprydding (issue #21):** NVML-aktiveringen beholder høyst fem eldre grunnopptak (`baseline-forrige-*.json`): det opprinnelige og de fire nyeste.
- **Begrensninger:** Rettingene i hjelperen, overtemperaturvernet og den nye RGB-avklaringen er bare prøvd mot simulerte kort og skript, ikke på ekte maskinvare. Issue #19 er delvis løst: `SHA256SUMS` er ennå ikke signert, og passorddialogen ved installasjon fra appen navngir fortsatt `python3`, ikke Agentic Rigg. Begge krever en egen beslutning (signeringsnøkkel og en fast hentehjelper eid av root). Første installasjon og kildebygg kontrolleres derfor bare mot summene GitHub publiserer.

## 1.5.6 · bygg 31 – 21. november 2026

- **RGB-oppdateringen stopper ikke på en gammel økt:** Hadde den gamle RGB-hjelperen en økt som ikke var avsluttet (for eksempel en beholdt farge), stoppet oppdateringen med «Uavklart RGB-journal … Velg «Gjenopprett lagret lys»». Nå avslutter «Bruk» økten i samme godkjenning: kortet settes tilbake til det lagrede utgangspunktet (kontrollert ved avlesning), journalen avklares med bekreftelsen du ga, den gamle tilgangen slås av, og den nye hjelperen installeres og aktiveres. Valget ditt brukes etterpå som en prøve.
- Bekreftelsesvinduet sier dette før du godkjenner. Bekreftelsen av fast farge gis før gjenopprettingen; avlesningen kontrollerer farge og lysstyrke, men effektmodusen kan ikke leses. Bekreft derfor bare når kortet viser én fast farge og ikke styres av et annet RGB-program. Feiler et trinn etter gjenopprettingen, sier meldingen hva som gjenstår. Uten bekreftelse for samme kort, for eksempel fra bryteren for skjermkortstyring, stopper oppdateringen før nettet og ber deg åpne RGB-lys.
- **Begrensninger:** Oppdateringen av RGB-hjelperen og effektene er bare prøvd mot simulerte skript og kort.

## 1.5.5 · bygg 30 – 21. november 2026

- **Valgene alltid fremme:** I 1.5.4 var modus, farge og lysstyrke skjult til RGB-hjelperen var aktiv. Med en utdatert eller ikke aktivert hjelper så RGB-siden derfor bare tekst og oppsettsknapper. Nå står Modus (fast farge, av, pusting, regnbue, fargesyklus), Farge (fargekode og ni hurtigfarger) og Lysstyrke (glidebryter og tall) alltid fremme, med korte overskrifter.
- **Kortet forhåndsviser valget:** 2D/3D-kortet viser fargen og effekten du velger, også før kortlyset er aktivert (merket «Forhåndsvisning av valg»).
- **«Bruk» aktiverer:** Første «Bruk» åpner den vanlige bekreftelsen av den faste fargen du ser på kortet og ber om passordet én gang. Den installerer eller oppdaterer RGB-hjelperen ved behov og bruker deretter valget som en vanlig prøve med Behold/Rull tilbake. Avbrytes eller feiler aktiveringen, sendes ingenting til kortet.
- **Ryddigere side:** Avlesning, kortidentitet, terminalreserven og knappene Sjekk igjen, Aktiver kortlys, Gjenopprett lagret lys, Avklar tidligere økt og Slå av RGB-tilgang ligger under «Detaljer og feilsøking». Den åpnes av seg selv når hjelperen melder feil.
- Hjelperne, protokollene og minimumsbyggene er uendret (RGB 29, NVML 25). En RGB-hjelper fra 1.5.4 kan beholdes.
- **Begrensninger:** Effektene og oppdateringen av RGB-hjelperen er fortsatt bare prøvd mot et simulert kort.

## 1.5.4 · bygg 29 – 21. november 2026

- **Kortet er tilbake:** RGB-lys viser skjermkortet i 2D eller 3D igjen (valget «3D-kortet» i Innstillinger › Utseende: Auto, Alltid 3D, Alltid 2D). Modellen speiler den ekte kortstyringen: valgt farge, lysstyrke og effekt som forhåndsvisning, og fargen som sist ble lest fra kortet. Det finnes bare ett sett kontroller, og kortet vises med sin ene fysiske sone. 3D-ressursene frigjøres når du forlater siden.
- **Effektbytte:** Du kan bytte direkte mellom fast farge, av og effektene, også mens en effekt går, og starte en effekt etter en beholdt farge. Hvert bytte er en ny prøve med Behold/Rull tilbake. Det lagrede utgangspunktet endres aldri. Leser hjelperen en annen farge enn effekten skrev, eller kan ikke lese kortet, stoppes effekten uten ny skriving. Når «Bruk» er sperret, står årsaken under knappen.
- **RGB-protokoll 3:** Laveste kompatible RGB-hjelperbygg er 29. «Aktiver kortlys» oppdaterer og aktiverer med ett passord: installasjonen slår av den gamle RGB-tilgangen (lyset og utgangspunktet beholdes) og slår GPU-styringen på igjen for samme kort. NVML-hjelperen og dens protokoll er uendret; en fungerende GPU-styring vises ikke som utdatert.
- **Begrensninger:** Rull tilbake etter et bytte fra en effekt går til siste effektramme, ikke til effekten. Effektene og oppgraderingen er prøvd mot et simulert kort, ikke på ekte maskinvare.

## 1.5.3 · bygg 28 – 21. november 2026

- **Én bryter, ett passord:** «Tillat endring av skjermkortet» henter og installerer systemkomponentene når de mangler eller hjelperen er utdatert, og slår på full skjermkortstyring i samme administratorgodkjenning. En utdatert hjelper vises som av; bryteren oppdaterer og aktiverer.
- **Kortlys på samme måte:** «Aktiver kortlys» installerer det som mangler og aktiverer RGB med ett passord, etter at du har bekreftet den faste fargen du ser på kortet.
- Rotskriptet aktiverer bare etter vellykket installasjon, med de faste, nettopp installerte skriptene og brukeren fra `PKEXEC_UID`. Den egne knappen «Installer systemkomponenter» er fjernet; terminalkommandoen står under «Uten nett: bruk terminal». Hjelperne, protokollene og minimumsbygget 25 er uendret.

## 1.5.2 · bygg 27 – 21. november 2026

- **Bare passordet:** «Installer systemkomponenter» og bryteren «Tillat endring av skjermkortet» trenger ikke lenger filvalg eller innlimt SHA-256. Installasjonsprogrammet kjører som root via `pkexec` og laster ned utgivelsen med appens egen versjon fra GitHub til en ny root-eid mappe. Det kontrollerer arkivet mot SHA-256-summen GitHub publiserer for akkurat den filen (den samme som står på utgivelsessiden), og kjører først da `system-install` fra den kontrollerte kopien.
- **Utdatert hjelper:** Ett trykk oppdaterer systemkomponentene og slår GPU-styringen på igjen (to passordspørsmål: installasjon og aktivering).
- **Personvern:** Appen kobler seg fortsatt aldri til nettet av seg selv. Den eneste nettkontakten er denne nedlastingen fra GitHub når du ber om installasjonen. Uten nett finnes terminalkommandoen under «Uten nett: bruk terminal».
- Installasjonsvinduet fra 1.5.1 er fjernet. Hjelperne, protokollene og minimumsbygget 25 er uendret.

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
