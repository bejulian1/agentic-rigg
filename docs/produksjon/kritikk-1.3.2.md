# Kodegjennomgang og UI/UX-test – 1.3.2 · bygg 21

14. november 2026. Gjennomført av Codex og tre Sol-agenter etter arbeidet med adaptive akser. Funnene under er eksisterende feil som ikke er rettet i denne leveransen. Grafrettingene er beskrevet separat nederst. Ingen endringer er sendt til ekte maskinvare, og den installerte appen er ikke erstattet.

## Åpne funn

1. **[P2] Historikktopper kan gå tapt rett etter omstart av tjenesten.** `app/crates/rigg-core/src/history.rs:387` lagrer den åpne bøtta sammen med ferdige rader. `restore` legger den tilbake som en ferdig rad ved linje 500, mens `ingest` ved linje 241 hopper over nye prøver i samme bøtte. Dette gjelder opptil ett minutt i 24-/48-timerslaget og opptil ti sekunder i 10-sekundslaget. **Reprodusert mot rigg-core:** 40 °C lagres ved 12:00:05, 110 °C leses ved 12:00:10. Uten gjeninnlesing blir maksimum 110 °C; etter gjeninnlesing blir det 40 °C. Gjenåpne siste delvise bøtte og bevar nok informasjon til å videreføre snittet korrekt.

2. **[P2] NVML-feil kan gi gamle verdier nye tidsstempler.** `app/crates/rigg-hw/src/gpu/nvml.rs:556` og `:560` erstatter last og VRAM bare ved vellykket lesing. Ved en ikke-fatal feil etter en gyldig prøve blir forrige verdi liggende i `c.last`, og `sample` ved linje 713 sender den videre som del av en ny prøve. Det kan gi misvisende historikk, statistikk og CSV. Sett dynamiske last-/minneverdier til `None` når lesingen feiler; hold faktisk statisk VRAM-total separat. **Verifikasjon:** kildegjennomgang av lesefeil- og publiseringsstien, ingen fremprovoserte driverfeil på ekte kort.

3. **[P2] Loggfeil kan holde tjenesten i gang etter at loggingen er stoppet.** `app/crates/riggd/src/logg.rs:257` og `:266` stopper logging ved skrive-/rotasjonsfeil. `daemon.rs:516` publiserer hendelsen uten å oppdatere prøvetakt eller avslutningsfrist. Den andre feilstien ved `daemon.rs:522` gjør dette, men `logger.tick` returnerer ingenting når loggeren allerede er stoppet. Med lukket vindu og bakgrunn av kan tjenesten dermed beholde rask loggtakt uten frist for avslutning. Kall `reschedule` og `check_idle` også etter overgang fra `on_sample`. **Verifikasjon:** kildegjennomgang; full disk er ikke fremprovosert på brukerens maskin.

4. **[P2] Mange CPU-tråder kan fortrenge minne-, nett- og diskhistorikk.** `app/crates/rigg-core/src/sensor_key.rs:102` legger opptil 128 trådserier før standardseriene. Historikken har 112 plasser og avviser nye nøkler ved `history.rs:189`. **Reprodusert med en syntetisk 64-tråders prøve:** 112 serier ble registrert, 19 ble forkastet, og `mem/usedPct` fikk ingen historikkplass. Prioriter standardseriene og avstem trådgrensen mot samlet historikkbudsjett.

5. **[P2] Et gammelt «Nå»-tall kan bli stående når nye prøver uteblir.** `app/crates/rigg/src/widgets/chart.rs:1746` leser siste verdi uten ferskhetskontroll og faller tilbake til historikk. Ved forbindelsesbrudd beholdes cache i `app.rs:223`; `ChartSet` oppdaterer bare ved ny tilkobling. Dermed kan en gammel temperatur eller effekt framstå som aktuell. Bruk samme ferskhetsregel som aksen, oppdater på forbindelsesbrudd og la verdien utløpe når prøvene stopper. Pausede grafer skal fortsatt vise sin frosne verdi. **Verifikasjon:** kildegjennomgang; egen UI-prøve med pauset testtjeneste inngår i testmaterialet.

6. **[P2] Angre etter sletting kan overskrive en ny profil.** `app/crates/rigg/src/pages/kontroller.rs:1375` skriver den gamle profilen tilbake uten å kontrollere hva som nå ligger i plassen. Slett en profil, lagre en ny i samme ledige plass, og trykk deretter den gamle toastens «Angre»: den nye profilen erstattes. Toastsystemet tillater flere samtidige meldinger. Gjenopprett bare hvis plassen fortsatt er tom, eller bruk en revisjon/identitet for å kontrollere at tilstanden er den samme. **Verifikasjon:** kildegjennomgang av lagring og toastens callback.

7. **[P3] To profiler med samme navn kan begge merkes «på siden».** `app/crates/rigg/src/pages/kontroller.rs:1197` finner profilen med riktige verdier, men merker andre profiler ved å sammenligne navnet ved linje 1210 og 1263. Navneendring ved linje 1355 tillater duplikater. To ulike profiler med samme navn får dermed samme markering når bare én matcher. Sammenlign profilplass/identitet eller verdiene direkte. **Verifikasjon:** kildegjennomgang.

8. **[P3] Et raskt Esc kan la sensorfilteret aktiveres etterpå.** `app/crates/rigg/src/pages/sensorer.rs:1907` sjekker det forsinkede `filter`, ikke råteksten i søkefeltet. Skriv og trykk Esc før søkeforsinkelsen: fokus kan flyttes mens teksten blir stående, og callbacken ved linje 994 aktiverer filteret etterpå. Tøm råtekst og filter synkront, slik Innstillinger nå gjør. **Verifikasjon:** kildegjennomgang og egen rask tastaturprøve i testforekomsten.

9. **[P3] `--innstillinger DEL` ignoreres når vinduet allerede finnes.** `app/crates/rigg/src/app.rs:64` håndterer `cli.page`, men ikke `cli.settings`. Første oppstart håndterer begge. En ny start med `--innstillinger om` viser derfor bare det eksisterende vinduet. Håndter innstillingsargumentet i samme gren som sideargumentet. **Verifikasjon:** kildegjennomgang og egen prøve mot den private appforekomsten.

10. **[P3] Sidefelt-innstillingene øker vinduets minstebredde over 640 px.** `app/crates/rigg/src/settings/sidefelt.rs:269` legger avkrysningene for sidene i et fast rutenett med tre like brede spalter. Den lengste teksten setter minstebredden for alle tre. **Målt i den private GNOME-økten:** Generelt, Utseende og Enheter og format beholdt 640 × 760; Sidefelt økte rammen til 683 × 760, og bredden ble stående etter videre navigasjon. GTK meldte samtidig at det ble tildelt mindre enn minste bredde. Bruk et rutenett som går ned til to spalter eller bryt lange etiketter i smal visning. Etter at faktisk vindusbredde ble brukt ved klikkene, ble alle ni innstillingsdeler åpnet og fotografert.

## Utført kontroll og avgrensning

- Samlet clippy med `--workspace --all-targets -- -D warnings` er ren. Workspace-testene gir 347 bestått, 4 ignorert og 0 feilet.
- Ekte museklikk gikk gjennom alle ti sider i et 1440 × 900-vindu og alle ni innstillingsdeler. Alle ti sider ble også åpnet med tastatursnarveiene i 640 × 760. Søk, Esc og direkte navigasjon fra maskinhodet ble prøvd i den private GNOME-økten.
- Ekstraprøven bekreftet råteksten «x» og et aktivert sensorfilter etter raskt Esc, at `--innstillinger om` beholdt Sensorer i det eksisterende vinduet, og at grafenes «Nå»-tall ble stående etter at den private testtjenesten var pauset i åtte sekunder. Alle ni innstillingsdeler ble også klikket i smal visning med faktiske vindusmål logget; breddeavviket står i funn 10.
- Egne skjermbildeplaner prøver temperaturer på 110 °C, kjerneklokke på 2500 MHz, minneklokke på 10000 MHz, effekt på 400 W, vifter på 108 % og disk på 90 °C. «Tilpass», «Fast», °F, langtidsvinduer og viftekurvens målepunkter er med.
- Utkast, Bruk, Behold og Rull tilbake testes mot simulert maskinvare. RGB-forhåndsvisningens 2D-/3D-bytte inngår i den egne lysplanen. Dette er ikke en test av fysisk RGB- eller skjermkortstyring.
- Det er ikke gjort ny ytelsesmåling, fremprovosert ekte diskfeil eller aktivert root-hjelper/polkit. Klikkrunden er en gjennomgang av sider og utvalgte hovedflyter, ikke en uttømmende test av alle kombinasjoner.

## Rettet i denne leveransen

Akseberegningen tar nå med den synlige delen av interpolerte segmenter ved tidsvinduets kanter. En fersk måling tas med før midtpunktet i den åpne langtidsbøtta kommer inn i vinduet. Linjens verdi klemmes ikke før interpolasjon; plottets tegneklipp håndterer utsiden uten å endre helningen.

Viftekurvens visningsakser utvides for målte verdier utenfor 20–100 °C og 0–100 %. Inputgrensene er beholdt. Aksen fryses under dragging, og ventende ytterverdier tas med etter slipp. Vanlige temperatur-, klokke-, effekt-, disk- og nettgrafer har ingen hard øvre målegrense. Last og brukt minne i prosent beholder 0–100 %.

Anbefalt oppfølging er først datakvalitet og livsløp i funn 1–5, deretter beskyttelse av profilinnhold i funn 6 og til slutt de mindre brukerflatefeilene.
