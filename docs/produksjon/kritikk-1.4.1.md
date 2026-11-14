# Kodegjennomgang og UI-kontroll · 1.4.1, bygg 24

14. november 2026. Gjennomgangen dekker brukerflate, generell CPU-topologi/datavisning, GPU-visning etter valgt korts funksjonsstøtte og avgrenset hjelperkompatibilitet. HW-skrivere, baseline, tilbakelesing, frister og recovery er bevart. Fysiske NVML-skriveprøver fra 1.4.0 beholdes som historikk.

## Lukkede funn

1. **[P1] Behold kunne havne utenfor skjermen etter Bruk.** Vifter og Klokker og effekt har nå bekreftelseslinja utenfor den rullende delen. Native GTK-plan bekreftet synlighet etter faktisk rulling ved 1920 og 640 px. Se `app/crates/rigg/src/pages/viftekurve.rs` og `app/crates/rigg/src/pages/kontroller.rs`.

2. **[P1] Enkel visning kunne skjule deler av det felles utkastet.** En effekt-/klokkeendring fra den andre siden eller skjult vifteinnstilling kunne følge med Bruk uten å være synlig. Enkel viser nå en oppsummering og lenker til riktig del. Teksten sier at Bruk gjelder hele utkastet; inaktive kurvefelt merkes som inaktive og åpner ikke Bruk alene. En aktiv kurve fortsetter ved visningsbytte. Se `viftekurve.rs::simple_hidden_draft` og tilhørende regresjonstester.

3. **[P2] Standardtekst kunne blandes med fabrikkantakelser.** Reset og hjelpetekst beskriver lagret utgangspunkt. Målt klokke nå, driverens rapporterte maks og justerbart offset skilles; appen beregner ingen framtidig sluttklokke. Effektprosentens standardgrunnlag må ikke forveksles med en ny baseline-fangst. Ingen backendlogikk er endret.

4. **[P2] Driverens vifteminimum ble kalt fysisk VBIOS-grense.** 35–100 % er det rapporterte manuelle området. NVIDIAs `nvmlDeviceGetMinMaxFanSpeed` beskriver hva brukeren kan sette; dette avgjør ikke automatisk Zero RPM. Påstanden om fysisk umulig viftestopp er trukket tilbake. Ingen 0 %-prøve eller ny automatikk er utført. Kilde og avgrensing står i app/README.md.

5. **[P2] Ren UI-patch krevde unødvendig ny administratoraktivering.** Begge statusstier godtar nå bare protokoll 2 og kjente hjelperbygg 23 til appbygg 24. Bygg 22, annen protokoll, ugyldige/overflytende metadata og ukjente nyere bygg avvises. AGENTS krever økt minimum og ved behov protokoll ved senere sikkerhetsrettinger. En lesende vertsprøve bekreftet daemon 24 mot allerede autorisert root-hjelper 23 som On/ready, uten HW-skriving. Se `app/crates/rigg-core/src/hjelper.rs::compatible` og `app/crates/riggd/src/hjelper.rs::state_for_status`.

6. **[P1] Sammenhengende CPU-numre ble forutsatt.** Linux-CPU-ID-er beholdes med hull, og aktive ID-er brukes av Prosessor, Sensorer og rapport. Hotplug håndteres uten at hull telles som ekstra tråder; metadata og belastningsgrunnlag følger endret CPU-sett. Topologi skiller sokler og kjerner. Cachevarianter grupperes etter faktisk identitet og deling; ukjent antall blir ikke én antatt instans. L3-grupper omtales generisk, og temperatur viser tilgjengelig sensornavn.

7. **[P1] GPU-visning forutsatte utviklingskortets grenser og to vifter.** Valgt korts funksjonsstøtte og grenser brukes i kontrollene. Ukjent vifteantall, eksplisitt null og én til flere vifter er ulike tilstander. Ubekreftet støtte gir ingen oppfunnet vifte eller fast RTX 2080-grense. Manglende CPU-/GPU-spenning og andre data omtales som ikke innsamlet eller ikke verifisert der det er det appen vet.

8. **[P2] Om manglet synlig historikk og beskrev appen som permanent lesende.** En egen innebygd versjonslogg åpnes fra Innstillinger og Om, med ni dokumenterte desktoputgaver, nyeste først. Prosjekteier og benyttede AI-modeller er presisert. Versjonsknappens sju-klikk-påskeegg er bevart. JSON-validering knytter nyeste oppføring til appens versjon og bygg og kontrollerer rekkefølgen.

9. **[P3] Avlest temperatur utenfor redigeringsområdet kunne få feil enhet.** Det nye lesefeltet utenfor gliderens område bruker nå samme Celsius/Fahrenheit-formatering som temperaturfeltet. Faktiske klokkeverdier utenfor appens policy vises uten at GTK klemmer dem til en annen verdi. Identitetsregresjonstesten følger den nye felles PCI-/UUID-kontrollen og beholder kravene til tilkobling og skriveport.

## Sluttkontroll og avgrensing

410 tester bestått, 4 ignorert og ingen feil; ren workspace-clippy. Private GTK-planer bestod for versjonsloggens to innganger og rekkefølge, fast Behold etter rulling, ukjent/0/1/3 vifter og smal visning, samt Prosessor, System og Disker i lyst/mørkt. Manuell 55 % forble bekreftet etter 71 s i simulering. Produksjonsbinærene oppgir 1.4.1 bygg 24. Ingen nye GPU-skriveprøver er utført; HW-sikkerhetsmotoren er uendret.

GPU-backenden er NVIDIA NVML; AMD-/Intel-GPU-backender er ikke implementert. CPU-, RAM-, hovedkort- og diskdata kommer fra tilgjengelige `/proc`-, sysfs/DMI-, hwmon- og UDisks2-kilder. Manglende data og funksjoner erstattes ikke med spesifikasjoner fra utviklingsmaskinen. Dette er generell håndtering av maskinvaredata, ikke fysisk verifikasjon av alle komponenter og kort.

Dette bekrefter ikke alle maskinvaregrenser, positiv overklokking, fysisk Zero RPM, RGB eller strøm-/driverfeil. Fysiske NVML-skriveprøver er dokumentert under 1.4.0. RGB-notatet og brukerens usikre Palit-modellkandidat er bevart; ingen automatisk modellidentifikasjon er utledet fra kandidaten.
