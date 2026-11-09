# Agentic Rigg 1.2.0 · bygg 18 · kritikk og finpuss (runde 7C)

*Agent 7C, 9. november 2026. Eneste agent etter 7A og 7B. Funnene er rettet i samme runde. Det som står igjen, er merket «Ikke rettet».*

## Sammendrag

1.2.0 har to nye deler. Den ene er bryteren og root-hjelperen i Innstillinger › Styring (7A). Den andre er Afterburner-fleksibilitet på Vifter (7B). Begge er grundig bygget og testet. Det som skurret, var hvordan de snakker til brukeren:

- **Låst-linja og toasten ba brukeren gjøre noe umulig.** Med bryteren på mot det ekte kortet sto det «Godkjenn et trinn i Innstillinger › Styring», men i 1.2.0 kan ingen trinn godkjennes der.
- **Feilstatusen viste en rå engelsk systemfeil.**
- **Teksten lovte mer vern enn det finnes.** «Ingen andre programmer slipper til» stemmer ikke (`hjelper-sikkerhet.md` 4.1).
- **«Hva skal til?» og Innstillinger › Styring var uenige.** Popoveren viste seks trinn med andre navn og i en annen rekkefølge enn de fire i Innstillinger.
- **Hjelperen brukte 7Bs viftemotor nesten, men ikke helt, som 7B beskrev.** Den startet uten `med_start`, og den stoppet motoren mens nødregelen gjaldt. Dermed falt viftene rett fra 100 % til kurven etterpå. Hos tjenesten rampes de ned.

**Telling:** 6 Må · 12 Bør · 10 Kan (28 funn). **Rettet:** alle 6 Må, alle 12 Bør og 8 av 10 Kan, blant dem de tre gjenstående Kan-punktene fra 6D (Kan 14, 15 og 23 i `kritikk-1.1.0.md`).

## Slik er det gjort

- **Lest:** `REGLER.md`, brukerens ønsker til 1.1.0 og 1.2.0, LOGG 6D, 7A og 7B, `hjelper-sikkerhet.md`, `protokoll.md` kap. 11–12, `maalinger.md` (6D, 7B) og `kritikk-1.1.0.md`.
- **Skjermbilder før rettingene:**
  - 7A: tatt første gang med `verktoy/runde7a/bilder7a.sh`, med release-bygget fra 7A og 7B.
  - 7B: bildene i `docs/skjermbilder/1.2.0/7b/`.
- **Skjermbilder etter rettingene:** 7A ligger i `docs/skjermbilder/1.2.0/7a/`, og hele serien i `docs/skjermbilder/1.2.0/7c/`.
- **Lest som en vanlig bruker** som ikke vet hva polkit, I2C eller en systemtjeneste er. Hver tekst er sjekket mot det som faktisk skjer i hver tilstand:
  - hjelperen av eller på, feil, eller fra en annen versjon
  - simulert eller ekte kort
- Hjelperen er kjørt bare som vanlig bruker mot det simulerte kortet og den falske roten (`scripts/falsk-pkexec.sh`). Det ekte kortet er bare lest.

Merking:
- **Må:** feil, eller noe brukeren ba om.
- **Bør:** synlig uferdig eller inkonsekvent.
- **Kan:** finpuss.

---

## Funn

### Hjelperen og bryteren (Innstillinger › Styring)

1. **Må · Låst-linja ber om et trinn som ikke kan godkjennes.** *Rettet.*
   - **Før:** Med bryteren på mot det ekte kortet sto det «Endring er slått på, men ingen trinn er godkjent ennå. Godkjenn et trinn i Innstillinger › Styring» på Klokker og effekt og Vifter. I 1.2.0 er alle trinnknappene av mot det ekte kortet.
   - **Nå:** «Endring er slått på, men denne versjonen endrer ikke det ekte kortet ennå. Det du endrer her, blir et utkast som lagres i appen.» (`locked::helper_line`).

2. **Må · Toasten etter bryteren sier det samme, og tier når hjelperen ikke starter.** *Rettet.* Toasten tilpasser seg nå tilstanden:
   - mot det ekte kortet: «Hjelperen kjører. Denne versjonen endrer ikke det ekte kortet ennå, så alt står som før.»
   - mot sim: «Godkjenn et trinn under før noe kan endres.»
   - svarer ikke hjelperen etter aktiveringen: feilmeldingen «Hjelperen svarer ikke».

3. **Må · Feilstatusen viser rå systemfeil.** *Rettet.*
   - **Før:** «Feil · Fikk ikke kontakt med hjelperen (No such file or directory (os error 2))».
   - **Nå:** «Hjelperen svarer ikke. Den kan ha stoppet. Trykk Sjekk igjen, eller slå bryteren av og på.» Er hjelperen aktivert for en annen bruker, står det med egne ord.
   - Det tekniske (versjon, bygg, kort, meldingen fra tjenesten) står i verktøytipset.

4. **Må · «Hvem»-raden lover for mye.** *Rettet.*
   - **Før:** «Ingen andre programmer eller brukere slipper til.» Prosesser som kjører som samme bruker, kan snakke med hjelperen (`hjelper-sikkerhet.md` 4.1). Det er et bevisst valg, men teksten sa noe annet.
   - **Nå:** «Bare brukeren din. Andre brukere på maskinen slipper ikke til.»

5. **Må · «Hva skal til?» og Innstillinger › Styring er uenige.** *Rettet.*
   - **Før:** Popoveren på låst-linja viste de seks gamle trinnene fra produktspesifikasjonen 6.2, med andre navn og i en annen rekkefølge enn de fire trinnene i Innstillinger, og under overskriften «i denne rekkefølgen».
   - **Nå:** Bryteren først, så Effekt, Vifter, Klokker og RGB-lys, med de samme ordene som i Innstillinger («Godkjent», «Ikke godkjent», «Kommer senere», «Ukjent»). Under står en knapp til Innstillinger › Styring.

6. **Må · Hjelperen brukte viftemotoren litt annerledes enn 7B beskrev og tjenesten gjør.** *Rettet.* Nesten alt var riktig: `Oppsett::fra_values`, `steg` og `heltall`. Det som skilte:
   - `Motor::new` uten `med_start`, så viftene hoppet fra farten de hadde, til kurven.
   - `sett_oppsett` i hver runde.
   - Opptil to steg samme millisekund (`apply` og `tick`).
   - Motoren sto stille mens nødregelen gjaldt, så viftene falt rett fra 100 % til kurven etterpå i stedet for å rampe ned.

   Hjelperen bruker nå motoren som tjenestens sim-sløyfe:
   - **Start:** `med_start(farten viftene har)`. Det er en ny lesing i `Skriver::fan_percents`, med `nvmlDeviceGetFanSpeed_v2` i den sperrede NVML-delen.
   - **Oppsett:** `sett_oppsett` bare når oppsettet er nytt.
   - **Steg:** ett per runde, også under nødregelen.
   - **Nødregelen:** `status.emergency` er også sann når motoren har gått til 100 % fordi temperaturen ikke kan leses.

   Sim-sløyfen er flyttet fra `riggd/src/vifter.rs` til `rigg-hw/src/gpu/sim_vifter.rs`, så hjelperens test kan kjøre den samme koden. Den nye testen `kurven_er_lik_tjenestens_sim_sloyfe` kjører 480 s med i ro, spill-last til nødregelen og i ro igjen, med egen kurve per vifte, hysterese, rampe, utjevning og stille flate. Den viser samme fart per vifte hvert sekund i hjelperen og i tjenesten, og den feiler med den gamle koden.

7. **Bør · Statusraden gjentar tilstanden.** *Rettet.*
   - **Før:** Merket «Av» og teksten «Av · appen leser bare». Merket «På» og «På · hjelperen kjører (bygg 17) · simulert kort · godkjente trinn: ingen».
   - **Nå:** Merket sier tilstanden, og teksten sier hva det betyr: «Appen leser bare. Ingenting på skjermkortet kan endres.» eller «Hjelperen kjører. Godkjent: effekt.»

8. **Bør · Sjargong.** *Rettet.*
   - **Før:** «root-tilgang», «polkit spør», «som systemtjenesten agentic-rigg-helper», «gjennom tjenesten agentic-riggd», «Palit-kontrolleren på I2C» og «sammen med polkit-reglene og oppsettet».
   - **Nå:** «administratorrettigheter», «Du blir spurt om passordet ditt», «Et lite program som kjører i bakgrunnen …» og «Fargene og effektene på kortet».
   - Stiene står samlet under «Filene», med monospace-skrift, for den som vil se dem.

9. **Bør · Godkjenn-dialogen.** *Rettet.* Dialogen hadde tre svakheter:
   - **Uklar frist:** «Hver endring gjelder bare til fristen går ut. Trykker du ikke Behold, rulles den tilbake.» Det kunne leses som at endringen alltid forsvinner. Nå: «Hver endring må bekreftes med Behold innen fristen, ellers rulles den tilbake av seg selv.»
   - **Teknisk ord:** «Lukkes appen og tjenesten» er nå «Lukkes Agentic Rigg helt».
   - **Knappen:** «Godkjenn Effekt» manglet «…», selv om passordvinduet kommer etterpå. Nå: «Godkjenn effekt …».

10. **Bør · «Ikke godkjent ennå» også der trinnet ikke kan godkjennes.** *Rettet.* RGB-lys og alle trinnene mot det ekte kortet i 1.2.0 viser nå «Kommer senere». Merknaden under lista sier hvorfor, også med bryteren av.

11. **Bør · «Utkast · låst» sier «Skriving er ikke slått på» også med bryteren på.** *Rettet.* Med bryteren på står det: «Denne versjonen endrer ikke det ekte kortet ennå, så Bruk er låst.»

12. **Bør · Sidefeltmerket.** *Rettet.*
   - **Før:** «Endring på · trinn: ingen». «Krever ny versjon» hørtes ut som at appen måtte oppdateres, men det som trengs, er å slå bryteren av og på.
   - **Nå:**

   | Tilstand | Merket |
   |---|---|
   | Av | «Bare visning» |
   | På mot det ekte kortet | «Endring på · låst» |
   | På mot sim | «Endring på · effekt» eller «Endring på · ingen trinn» |
   | Feil | «Endring: feil» |
   | Annen versjon | «Endring: slå på igjen» |

   Verktøytipsene sier det samme med flere ord.

13. **Bør · RGB-lys snakker om I2C som «ikke er slått på».** *Rettet.* Med bryteren på var det feil.
   - **Låst-linja:** «Lyskontrolleren leses ikke ennå: lys er siste trinn og kommer i en senere versjon.»
   - **«Tilgang»:** «Kommer i en senere versjon, som trinnet RGB-lys i Innstillinger › Styring.»

14. **Bør · «Skriving» og «endring» om det samme.** *Rettet.* Bryteren heter «Tillat endring av skjermkortet».
   - **Klokker og effekt og Vifter:** «Når skriving blir slått på» er nå «Når endring er slått på».
   - **Klokker og effekt, Vifter og vifteforklaringen:** «Stopper tjenesten, går viftene tilbake til Auto» er nå «Lukkes Agentic Rigg helt, går …».
   - **Verktøytipset på Bruk:** «Skriving er ikke slått på» er nå «Bruk er låst: denne versjonen endrer ikke det ekte kortet». Det stemmer både med bryteren av og på.

### Vifter

15. **Bør · «Siste 5 minutter» med egne kurver per vifte.** *Rettet.* Grafen tegner vifte 1 heltrukket og vifte 2 stiplet, men forklaringen hadde bare «Med kurven». Nå står «Vifte 1 med kurven» og «Vifte 2 med kurven», med stiplet strek. Med felles kurve står «Med kurven» som før.

16. **Kan · «Nødregel 100 % over 85 °C» står øverst i nødfeltet**, der kurven går mot 100 %, så linja gikk gjennom teksten. *Rettet:* Teksten står nå nederst i feltet, rett over «Minst 35 %». Der går kurven ikke over 85 °C, heller ikke ved 640 px.

17. **Kan · Modus Kurve forklares med «Tjenesten setter farten fra viftekurven ved hver måling».** *Rettet:* Nå står det «Agentic Rigg setter farten etter viftekurven hvert sekund», på Vifter og på Klokker og effekt.

18. **Kan · Vifter ved 640 px: fem fliser på første rad og «Kurven gir» alene på andre.** *Rettet.* Flisene står nå 6, 3 eller 2 per rad, alltid like mange på hver rad (`StatStrip::balance`). Ved 640 px blir det 3 + 3.

19. **Kan · «Nå 61 °C · 72 %»-merket i viftegrafen kan ligge oppå kurven** når punktet er i en bratt del. *Ikke rettet.* Merket har bakgrunn og er lesbart. Å flytte det etter kurven er en egen jobb.

### Fra 6D (kritikk-1.1.0)

20. **Kan · «P8 / tomgang» står to ganger på Klokker og effekt** (Kan 14). *Rettet.* I tomgang står P-tilstand «P8 · tomgang» og Begrenset av «Ingenting · Kortet er i ro». P8 står bare én gang, og verktøytipset forklarer resten.

21. **Kan · Utkast-merket står etter Behold mot testdataene** (Kan 15). *Rettet.* Merket «Utkast · ikke sendt til kortet» og «Utkast · låst» i låst-linja vises bare når utkastet er ulikt det kortet har.

22. **Kan · Modellnavnet kuttes på Disker ved 640 px** (Kan 23). *Rettet.* I smale vinduer (under 1000 px) skjules tilkoblingsbrikken («SATA 6,0 Gb/s», «PCIe 4.0 ×4»), som også står under Tilkobling i Disk-panelet. Modellnavnet får da plass.

### Skall, verktøy og versjon

23. **Bør · Vindusknappene i Innstillinger ved 640 px** står rett etter overskriften i stedet for i høyre kant (fra 1.1.0). *Rettet.* Overskriften fyller raden når ingressen er skjult.

24. **Kan · Hjelperens `--version` har et eget format** («agentic-rigg-helper 1.2.0 (bygg 18)»). *Rettet:* Nå står det «Agentic Rigg-hjelperen 1.2.0 · bygg 18», som de to andre. `aktiver.sh` leser byggnummeret fra begge formene.

25. **Kan · Trinnene viser «Ikke godkjent» når hjelperen er i feil.** Da vet vinduet ikke hva som er godkjent. *Rettet:* Trinnene viser nå «Ukjent».

26. **Bør · `bilder.sh`.** *Rettet.*
   - Standardmappa var `docs/skjermbilder/1.1.0/`. Nå er den `1.2.0`.
   - `--plan alle` manglet 7A og 7B. Nå er `7a`, `7a-ekte`, `7b-ekte` og `7b-sim` med. 7A-planene får den falske roten og den falske pkexec som i `bilder7a.sh`.

27. **Bør · Planen `7a-ekte` viste ikke låst-linja på Vifter og RGB-lys med bryteren på**, og heller ikke Klokker og effekt etter at bryteren var slått av igjen. *Rettet:* Planen er utvidet.

28. **Kan · Sim-banneret «Bruk sender utkastet til den simulerte backenden» står også når hjelperen er i feil.** Da svarer Bruk `helper_error`. *Ikke rettet.* Det gjelder bare utviklerforekomsten med `--backend sim`.

### Sjekket og i orden

- **Bryteren** står i «Venter på passordet …» mens pkexec kjører, og flytter seg først når skriptet er ferdig. Avbrutt passord gir «Avbrutt · Ingenting er endret».
- **Merket ved Styring, låst-linja og toppfeltets «Låst»** følger tilstanden på alle tre Styring-sidene (planen `7a-ekte`).
- **Vifter: «Oppførsel»** har forklaring i klartekst under hver glider, og standardmerket står ved hver av dem. «Viftestopp (0 %) er ikke mulig på dette kortet» står rolig, uten varselfarge. Hysteresebåndet, nedkurven og «Hold minimum til» stemmer med motoren (7Bs tester).
- **Høy kontrast, lyst og mørkt, 640 og 1920 px** er sjekket i den fulle serien. `kuttet.txt` er kommentert i `maalinger.md`, runde 7C.

## Det som gjenstår

- Kan 19 og 28 (over).
- Brukeren har ikke sett 1.2.0 installert. Hovedøkta installerer.
- Trinn 2 (effektgrense mot det ekte kortet) følger planen i `hjelper-sikkerhet.md` kap. 6, med brukeren til stede.
