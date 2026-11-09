# Veikart

Brukeren ga 4. november 2026 klarsignal til å jobbe med Agentic Rigg over tid. Hver runde gir nytt versjons- og byggnummer, får en egen seksjon i `PATCHLOG.md` og testes med skjermbilder før den kalles ferdig.

**Prinsipper**
- Detaljer framfor flere funksjoner. Appen skal føles gjennomtenkt, ikke oppblåst.
- Demoen leser bare fra maskinen. Alt som skriver til skjermkortet (NVML-set, I2C/RGB) eller trenger root, venter til brukeren har godkjent det, trinn for trinn.
- Én agent skriver i de samme filene om gangen.

## v0.0.4 · bygg 4: redesign
- [x] Designkritikk av v0.0.3 (`design-kritikk-v0.0.3.md`)
- [x] Redesign: skall, komponenter, Lucide-ikoner, Innstillinger, Lys
- [x] Kanttilfeller og temaforhåndsvalg (`edge-cases.md`)
- [x] Animasjoner og mikrodetaljer (`bevegelse.md`)

## v0.0.5 · bygg 5: sikkerhetsnett
- [x] Servertester med `node --test`: tolking av nvidia-smi, validering i `store.js`, import/migrering, sikkerhetsavvisninger (Host, Origin, sti)
- [x] Røyktest av UI i headless Chrome: alle sider, begge temaer, ingen konsollfeil, ingen vannrett rulling
- [x] `npm test` samler dette, slik at senere runder ikke ødelegger det som virker

## v0.0.6 · bygg 6: andre kritikkrunde og ønskeliste
- [x] Ny designkritikk av v0.0.5, gjort av en ny agent (`design-kritikk-v0.0.5.md`), og funnene rettet i del 1, «Kritikkrunde 2»
- [x] Viftekurve: start- og stopptemperatur med hysterese (brukerønske; 0 % er ikke mulig, minimum er 35 %), laget i del 2, «Ønskeliste»
- [x] Min/maks på Sensorer huskes mellom økter
- [x] Eget appikon
- [x] Feste sensorer, og bestemme rekkefølgen i statusblokken
- [x] Død plass sett i v0.0.4: under Justering på Lys, og mellom stolpe og bunntekst i Temperatur- og Effektkortet på Oversikt
- [x] Fra detaljagenten: «Vis alle (N)» på System endrer bredde (fast ikonknapp, tallet i overskriften)
- [x] Fra detaljagenten: regnbue/fargesyklus skriver 56 LED-farger per bilde (løst i v0.0.8: fasen rundes til 1,5°, målt i PATCHLOG)

## v0.0.7 · bygg 7: klar for ekte bakgrunnstjeneste
- [x] API-kontrakt for daemonen (JSON over Unix-socket), med en Node-mock som demoen snakker med (`docs/daemon-api.md`, `daemon-mock/`)
- [x] Bekreft-eller-rull-tilbake-flyt i UI for klokke, effekt og vifte (simulert), med frist 10–60 s under Innstillinger
- [x] Sikkerhetsgrenser i UI: vifte alltid tilbake til auto, full vifte over 85 °C, grensene kortet melder og klemte verdier

## v0.0.8 · bygg 8: finpuss etter daemon-flyten
- [x] Detaljrunde med friske øyne over alle sider etter v0.0.7 (bekreftelseslinja, grenser, klemming, frakoblet tjeneste), funn i `finpuss-v0.0.8.md`
- [x] Viftekurve: «Under 35 % er ikke mulig på kortet» krysses av stiplede linjer i nødregelsonen; tomrom nederst i Oppførsel når start/stopp er av
- [x] Kontroller: Bruk-knappen viser bare en hake mens den venter på Behold; grenselinja i Effekt-grafen tegnes som et lite krøll når grensen endres
- [x] Flere faner som trykker Bruk samtidig, og tastatur/fokus i røyktesten

## v0.0.9 · bygg 9: tettere og mer profesjonelt
- [x] Designplan i retning av HWiNFO (tetthet, paneler, tabeller, etikett–verdi, grafene beholdt), uten å kopiere navn, ikoner eller farger (`design-pro-v0.0.9.md`)
- [x] Tokens: lavere rader, mindre luft og radius, ingen skygge på paneler, panelstripe med egen farge som `npm run contrast` sjekker
- [x] Sensorer som én tabell gruppert etter enhet, med sammenleggbare grupper, sparklinje i hver rad og større graf under valgt rad (brukerønske)
- [x] Oversikt, Grafikkort, System, Kontroller, Viftekurve, Lys, Grafer og Innstillinger i samme uttrykk
- [x] Før- og etter-bilder (`skjermbilder/v0.0.9/`) og egen finpussrunde med funn
- [x] Ny røyktest for Sensorer; check, test og contrast passerer

## v0.0.10 · bygg 10: sammenleggbart sidefelt
- [x] Tre skisser for sidefeltet; brukeren valgte B (rent sidefelt som kan legges sammen) (`sidefelt-v0.0.10.md`)
- [x] Hovedmenyen fjernet; Innstillinger, Tastatursnarveier og knappen som legger sammen står nederst i sidefeltet, og Om er en del av Innstillinger
- [x] Kortvelgeren som én tynn rad med ikon, navn og statusprikk
- [x] Ikonlist med knapp og Ctrl+B, huskes i innstillingene (`sideCollapsed`), alltid ikonlist under 860 px
- [x] Røyktest for sidefeltet, oppdatert fokustest, før- og etter-bilder og finpussrunde

## v0.0.11 · bygg 11: grafer opptil 48 t, målere og farger
- [x] Historikk på serveren med lagdelt oppløsning (min/snitt/maks), faste grenser for minne og fil, lagring i datamappa og `/api/history`
- [x] Tidsvindu 1 min–48 t på Grafer, Oversikt, Kontroller og grafen på Sensorer, med tidsakse, bånd, hull og «Data fra»
- [x] Pause per graf, markør på alle grafene eller én, forklaring som skjuler serier
- [x] Faste seriefarger som tokens, seks aksentfarger med navn, og gjennomgang av gult og rødt
- [x] Nye målere for temperatur og effekt uten prikk, med merker og forklaring
- [x] Tester for historikken, API-et og en røyktest; skjermbilder og finpussrunde (`grafer-og-malere-v0.0.11.md`)

## v0.0.12 · bygg 12: små overraskelser
- [x] Ti påskeegg i én modul (`web/js/easter.js`) med varierte utløsere, fasit i `paaskeegg.md`
- [x] Bryteren «Små overraskelser» under Innstillinger › Generelt (`surprises`, på som standard)
- [x] Røyktest: hvert egg, bryteren, «Av» uten animasjoner og ingen utløsning ved et uhell
- [x] Skjermbilder (`skjermbilder/v0.0.12/`) og finpussrunde

## v0.0.13 · bygg 13: Lys i 3D
- [x] Prototype i `prototyper/lys-3d/` (WebGL 2 for hånd, lag som tas av, eksplodert visning, motoren lik `rgb.js`)
- [x] 3D-kortet på Lys, med forhåndsvisningen i full høyde og en smal kolonne; det flate kortet som reserve
- [x] `rgb.js` delt i motor og tegnere (`LightTarget`); 3D lages ved behov og frigjøres
- [x] Slankere glidere i hele appen og fargeknapper på én rad med «Egendefinert …»
- [x] Scenen i alle temaer, med egne tokens i kontrastsjekken
- [x] Røyktest for 3D og reserven, skjermbilder og finpussrunde (`lys-3d-v0.0.13.md`)

## v0.0.14 · bygg 14: rettelse av ikonlista
- [x] Sidefeltet kan alltid utvides igjen, også mot en server som ikke kjenner `sideCollapsed`, avviser den eller ikke svarer
- [x] Tallene i ikonlista: tett gruppe for tall og enhet, luft mellom verdiene, kildemerke (GPU, CPU, RAM …), ingen rulling
- [x] Røyktest (legg sammen, utvid, last på nytt mot fire slags servere), før- og etter-bilder og finpussrunde

## v0.0.15 · bygg 15: Innstillinger i hele vinduet
- [x] Innstillinger som egen visning over hele vinduet (`data-view="settings"`), ikke et lag; tilbake med knapp, Esc, Ctrl+, og nettleserens tilbake til siden du kom fra, med rulling og fokus (`innstillinger-v0.0.15.md`)
- [x] Egen navigasjon der sidefeltet står, med tittel, tilbake, søk og delene; ikoner øverst under 700 px
- [x] Paneler med etikett til venstre og kontroll til høyre i én til tre spalter, kortere forklaringer, Varselfarger flyttet til Grafer og sensorer, Bredde under Sidefelt
- [x] Søk i alle delene (/ og Ctrl+F) med kontrollene i treffene
- [x] Ny røyktest (`ui/settings.test.js`), før- og etter-bilder og finpussrunde

## Krever brukerens godkjenning før start
- Rust-daemon med root (nvml-wrapper, polkit, systemd), etter LACT-mønsteret. Kontrakten er klar (`docs/daemon-api.md`); mocken kan byttes ut uten endringer i appen
- Installere noe på maskinen for daemonen: systemd-enhet, polkit-policy (`io.github.AgenticRigg.tune`, `.advanced`, `.rgb`), gruppa `agentic-rigg` med deg som medlem, og socketen `/run/agentic-rigg/daemon.sock` (0660)
- Første ekte skriving med NVML, ett trinn om gangen og alltid med bekreft-eller-rull-tilbake: effektgrense først, så klokke-offset, så vifte (manuell, kurve). Temperaturmålet (`ACOUSTIC_CURR`) er ikke verifisert på GeForce og prøves sist
- Vifte til auto når daemonen dør uten å rydde: et lite `ExecStopPost=`-program som bare kaller `nvmlDeviceSetDefaultFanSpeed_v2`, og `WatchdogSec=` fra viftesløyfa
- Legge det bekreftede oppsettet på igjen ved oppstart og etter dvale (effektgrense og offset overlever ikke omstart)
- Nødterskelen (85 °C) og tjenestens strengere grenser for offset (−200…+300 / −1000…+1500 MHz) er valgt for demoen. De bør bekreftes av deg før de gjelder det ekte kortet
- Trinnvis kartlegging av Palit-RGB på `/dev/i2c-0`, adresse 0x49 (se `research/linux-gpu-kontroll.md`)
- Ta opp historikk også når ingen fane er åpen (betyr at `nvidia-smi` alltid kjører mens serveren gjør det)
- Electron-pakking og installasjon
- Lasting av sensordriver for hovedkortet (nct6775)
