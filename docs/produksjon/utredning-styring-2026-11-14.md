# Ekte styring av Palit RTX 2080 – utredning 14. november 2026

## Konklusjon og avgrensning

Effekt, vifter og klokkeoffset har en konkret implementasjonsvei via NVIDIA NVML. Det er fortsatt ikke bekreftet at skrivekallene fungerer på dette eksemplaret: denne undersøkelsen har bare lest. RGB krever egen backend og identifikasjon av lyskontrolleren. En fungerende NVML-backend vil ikke i seg selv gi RGB-styring.

Brukerens nye føring er **én aktivering av skjermkortstyring**, uten fire separate godkjenninger. Interne utviklingsprøver kan fortsatt gjennomføres hver for seg. Dette tillegget erstatter den gamle planens brukerflyt med fire trinn og dens bastante påstander om bekreftet støtte på akkurat dette kortet. Det er forarbeid til neste funksjonsoppdatering, ikke en aktivering av ekte skriving i 1.3.2.

Ingen kode, maskinvareinnstillinger, tjenester eller systemfiler er endret i denne runden. Ingen I2C-enhet er åpnet. Kildeversjonen er fortsatt **1.3.2 · bygg 21**.

## Bekreftet på maskinen nå

Lesing med `nvidia-smi --query-gpu=…` og prosjektets `verktoy/nvml_probe.py --json`. Sonden tillater bare NVML-gettere, init og shutdown. Den første spørringen inne i sandkassen fikk ikke kontakt; samme lesing utenfor sandkassen lyktes. Dette var ingen påvist driverfeil, og driveren er ikke endret.

| Egenskap | Resultat 14. november |
|---|---|
| Kort | NVIDIA GeForce RTX 2080, PCI `0000:1c:00.0` |
| PCI / subsystem | `10de:1e87` / `10de:1e87` |
| Driver / VBIOS | `610.43.02` / `90.04.0B.40.8C` |
| Effektgrense | Minst 105 W, høyst 275 W; standard og gjeldende 245 W |
| Vifter | To, rapportert område 35–100 %, begge i automatisk modus ved lesingen |
| Klokkeoffset P0 og P8 | Grafikk: nå 0, område −1000…1000; minne: nå 0, område −2000…6000, som rå NVML-MHz |
| Akustisk temperaturmål | Nå 83 °C, rapportert område 65–88 °C |
| Andre temperaturterskler | Nedklokking 97 °C, avstenging 100 °C; minnetemperaturgrense og temperaturmargin støttes ikke av disse getterene |

Områdene er driverens rapportering, **ikke anbefalte innstillinger**. Gettere beviser ikke at settere godtas eller har forventet virkning. UI bruker i dag halvparten av rå minneoffset; enheten og omregningen må kontrolleres før ekte bruk. Rapportert minstefart 35 % er ikke en separat prøve av viftestopp. De målte temperaturtersklene er heller ikke anbefalte driftstemperaturer.

PCI-subsystemet oppgir NVIDIA, ikke Palit. Det identifiserer derfor ikke alene fysisk modell eller lyskontroller. Produktkode og eventuell lysende logo er fortsatt uavklart her.

## Hva primærkildene underbygger

[NVIDIAs NVML Device Commands](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceCommands.html) beskriver effektgrense, manuell viftefart og retur til automatisk viftestyring. Manuell viftefart endrer viftepolicy; gjenoppretting av Auto må derfor være en reell, kontrollert operasjon. Flere endringer krever administratorrettigheter. [NVML Device Queries, SetClockOffsets](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlDeviceQueries.html) beskriver offset per klokkedomene og P-tilstand, med egen feil for manglende støtte.

[LACTs støtteoversikt](https://github.com/ilya-zlobintsev/LACT/blob/master/docs/Hardware-Support.md) underbygger NVML-veien på nyere GeForce og anbefaler nyere driver for offset. Den skiller mellom kjernelåsing på Volta+ og minnelåsing på Ampere+. RTX 2080 er Turing: offset og låst minneklokke må ikke presenteres som samme funksjon. Dette er støttebevis for implementasjonsveien, ikke en lokal skriveprøve.

### RGB: relevant spor, fortsatt ubekreftet på dette eksemplaret

[OpenRGB #1420](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1420) gjelder GamingPro OC RTX 2080 med samme PCI-/subsystemkombinasjon og ThunderMaster-opptak. Lik ID gjør saken relevant, men beviser ikke at brukerens kort har samme lyskontroller.

Gjennomgangen av [Palit-detektoren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUControllerDetect.cpp), lest 14. november, fant ingen registrert match for hele kombinasjonen ovenfor. V2 bruker port 1 og adresse `0x49`. En Gainward-oppføring inneholder også `1e87`, men i et annet ID-felt og med andre øvrige ID-er; den er ikke en match.

[V2-headeren](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUv2Controller.h) definerer blant annet kontrollregister `0x60`, RGB `0x6c–0x6e`, lysstyrke `0x6f`, programvaremodus `0x01` og pusting `0x22`. [Implementasjonen](https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/Controllers/PalitGPUController/PalitGPUv2Controller.cpp) skriver også `0xe0` ved enkelte vanlige handlinger. Å kopiere denne sekvensen uten modellkontroll er derfor ikke en ferdig løsning.

Den gamle planens «PALIT 150»-signatur og testresultater for et søsterkort er tidligere kildeavlesninger; de ble ikke bekreftet på nytt fra opptakene nå. [#1938](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1938) gjelder dessuten 2080 Ti. Ingen av delene er en lokal identifikasjon. Prosentanslaget i den gamle planen skal ikke brukes som bekreftelse.

[Palits nedlastingsside](https://www.palit.com/palit/vgapro.php?id=4977&lang=en&tab=do) oppfører Windows for dagens ThunderMaster, ingen Linux-utgave der. [ThunderMaster-siden](https://www.palit.com/thundermaster/index.php?lang=en) omtaler RGB-lagring på enkelte kort, men fastslår ikke oppførselen til dette eksemplaret. Verken permanent lagring eller garantert nullstilling ved strømbrudd er bekreftet.

## Hvorfor dagens bryter ikke kan gjøres ekte med ett flagg

Funnene er lest i dagens kildekode. De er krav til implementasjonen før ekte skriving slås på.

| Funn og sted | Nødvendig endring |
|---|---|
| `rigg-hw/src/gpu/skriv.rs`: `EKTE_SKRIVING=false` og avslått cargo-feature; UI og `txn::write_gate` tillater bare sim/dev | Samordnet autorisasjon gjennom UI, tjeneste og hjelper. Behold sperrene til resten er prøvd. |
| `skriv_nvml.rs::open_nvml` åpner alltid indeks 0 | Bind oppsett, forespørsler og gjenoppretting til PCI og UUID; kontroller identiteten ved åpning. |
| `read_limits` krever at alle grensegettere lykkes | Rapportér støtte og grenser uavhengig for effekt, hver klokke, vifter, temperaturmål og RGB. |
| UI sender hele `Values`; `txn::merge` har faste RTX 2080-grenser | Send bare endrede felt og valider mot det valgte kortets rapporterte grenser. Ustøttet temperaturmål må ikke blokkere effekt. |
| `apply` kan stoppe etter et vellykket delkall; `snapshot` er minnetilstand | Registrer faktisk resultat per felt, les tilbake der det er mulig og gjenopprett etter delvis feil. Skille ønsket fra bekreftet tilstand. |
| `Motor::drive_fans` ignorerer skrivefeil; `push_hw` ignorerer resetfeil; NVML-reset merker standard også ved feil | Oppdater cache bare ved suksess. Vis ukjent/feilet maskinvaretilstand og sperr nye endringer når gjenoppretting ikke er bekreftet. |
| Standard for temperaturmålet leses som nåverdi; ny `ExecStopPost`-prosess kjenner ikke opprinnelig verdi | Lagre grunnverdier før første endring, bundet til kort og driver. Skill fabrikkstandard, oppstartstilstand og forrige bekreftede oppsett. |
| Ekte snapshot mangler `stock`, mens `riggd::Hjelper::holds_values` bruker det | Rett protokollkontrakten, slik at også bekreftede ekte endringer får korrekt livssignal. |
| Tjenesten har bare `CAP_CHOWN`, `ProcSubset=pid`, `TasksMax=4` og lukket enhetstilgang | Prøv NVMLs rettigheter og nødvendige `/proc`-/ressurstilganger i en egen kontrollert kjøring. Å kjøre som root med disse begrensningene beviser ikke at det virker. Ikke fjern herdingen generelt. |
| `motor.rs` har ingen RGB-backend; UI har simulerte soner | Egen identifikasjon og protokollbackend. Vis bare soner og effekter den virkelige kontrolleren støtter. |

I tillegg kreves uttrykkelige overganger ved hvile/resume, mistet GPU og uren hjelperstans. Automatisk gjenbruk av gamle innstillinger etter endret kort, VBIOS eller driver skal avvises til tilstanden er kontrollert. Dette er ikke testet på maskinvaren her.

## Brukerflyt i neste funksjonsoppdatering

- Én bryter: **Tillat endring av skjermkortet**. Ett samlet aktiveringsspørsmål med administratorpassord. Aktivering alene legger ingen profil eller klokkeverdier på kortet.
- De fire Godkjenn-radene fjernes. En enkel statusliste viser hva det identifiserte kortet støtter, med konkret grunn ved manglende støtte. Listen er informasjon, ikke fire nye tillatelser.
- Kontroller aktiveres per tilgjengelig funksjon. Automatisk oppdagelse bruker aldri skriveprøver. Manglende støtte og uavklart støtte må kunne skilles fra hverandre.
- **Bruk → Behold / Rull tilbake** beholdes med frist styrt av hjelperen. Uten Behold går endringen tilbake til forrige bekreftede oppsett. Globale tillatelser erstatter ikke denne angremuligheten.
- **Tilbakestill utkast** endrer bare redigeringen. **Gjenopprett standard på kortet** er en separat helper-operasjon med resultat fra maskinvaren. Nåværende «Tilbakestill alt» i `kontroller.rs` er bare et lokalt utkast.
- Bryteren av skal gjenopprette berørte funksjoner og kontrollere resultatet før hjelperen fjernes. Ved feil: **Kunne ikke bekrefte at kortet er tilbake på standard.** UI skal ikke melde vellykket gjenoppretting.
- RGB uten identifisert backend: **Lyset på kortet kan ikke styres ennå. Dette er en forhåndsvisning.** Ingen aktiv handling som påstår å bruke fargen på kortet.

Aktiveringsforklaring: «Gir Agentic Rigg tilgang til å endre støttede funksjoner på skjermkortet. Du blir spurt om administratorpassord. Endringer brukes først når du trykker Bruk.»

## Konkret rekkefølge for implementasjon og prøving

1. **Kontrakt og tilstand:** global tillatelse, kortidentitet, støtte per felt, endringspatcher, typede feil og versjonert helper-oppsett. Migrer bort `trinn` i aktiveringsscript, polkit, protokoll og brukerflate.
2. **Gjenoppretting før aktivering:** vedvarende grunnverdier, delvis feil, sannferdig tilbakelesing, viftefeil, korrekt livssignal, reset uten avhengighet av ustøttede felt. Rettighetene valideres separat.
3. **Sim og feilinjeksjon:** unsupported temperaturmål med fungerende effekt; unsupported minne med fungerende kjerne; feil kort; feil etter hvert delkall; én vifte feiler; reset feiler; frist, frakobling, prosesskrasj og resume. Profiler med ustøttede felt skal forklare avviket.
4. **Først deretter ekte NVML-prøver:** hvert støttet felt må bekreftes på det konkrete kortet, inkludert Auto og tilbakerulling. Behold utgivelsessperren inntil resultatene er dokumentert. Det er ikke utført slike prøver i denne runden.
5. **RGB som egen leveranseavhengighet:** bekreft fysisk modell, lys og kontroller. En senere backend må finne adapter ved navn og GPU-PCI-sti, kontrollere signatur og bruke en begrenset registerliste. Ingen bussnumre skal hardkodes, ingen bred busskanning, ingen tilgang til USB-C-kontroller eller SPD. Gjeldende prosjektregel forbyr åpning av `/dev/i2c-*`; dette dokumentet er ikke en tillatelse til å gjøre det.

NVML-styring bør ikke holdes tilbake bare fordi RGB mangler. RGB må samtidig forbli tydelig utilgjengelig til akkurat den funksjonen er bekreftet. Ingen tids- eller støttegaranti gis for RGB på dette eksemplaret.

## Leveranse og kontroll denne runden

Tre Sol-agenter gjennomgikk backend/tilbakestilling, brukerflyt/protokoll og eksterne RGB-kilder. Hovedøkta kontrollerte kodefunnene, leste maskinvaredata og samlet dette tillegget. Den gamle planen er beholdt med en tydelig henvisning hit.

Bare dokumentasjon er endret etter siste kodekontroll av 1.3.2: workspace-clippy var ren, 347 tester bestått og 4 ignorert. Tester er ikke kjørt på nytt for dokumenttillegget. Dette bekrefter ikke ekte maskinvarestyring. Ingen ny versjon, installasjon eller endring i det installerte bygget.
