# Designkritikk av alpha v0.0.5 · bygg 5

Uavhengig gjennomgang 4. november 2026, gjort før `design-kritikk-status.md` ble lest. Etterpå er funnene sjekket mot statuslisten. Ting som er valgt bort med vilje, er ikke tatt med igjen.

**Slik er det testet:** egen server på port 4331 (`AG_SIMULATE=1`, deretter live mot nvidia-smi, bare lesing) og headless Chrome. Alle ni sider er sett i lyst og mørkt tema ved 1440×900, og i lyst tema ved 1920×1080, 1100×800 og 360×780. I tillegg er dette åpnet eller prøvd: hovedmeny, enhetspopover, Om, Tastatursnarveier, Tilbakestill, grafvelger, pause, hårkors, filter uten treff, ulagrede endringer (lyst og mørkt), profiler, dra i viftekurven, fokusring, fargetemaene Midnatt og Sand, frakoblet tilstand (serveren stoppet mens siden var åpen) og live-modus.

**Skjermbilder:** `/tmp/claude-1000/-run-media-julian-Transcend-128GB-Claude-Code-Prosjekter-ubuntu-desktop-applikasjoner/b1cbda93-d303-42c5-84d3-fb3430d92402/scratchpad/kritikk-v005/`. Filnavnene står ved hvert funn. Mappen ligger i tmp og forsvinner ved omstart.

---

## 1. Helhetsinntrykk

Appen er nå tydelig GNOME-preget og føles stort sett gjennomtenkt. Dette er på pro-nivå: skallet (sidefelt med seksjoner, statusblokk med terskelfarger, ikonskinne på smale vinduer), Innstillinger som libadwaita-lister, endringsflyten på Kontroller (prikk, ↺, før → etter i Endringer, Forkast/Bruk i toppfeltet, angre-toast), viftekurven (spøkelseskurve, koordinater mens du drar, 35 %-sonen), synkronisert hårkors og ærlige «Ikke tilgjengelig»-forklaringer. Tallformatet er konsekvent, med tabulære sifre, smale mellomrom og U+2212.

Det som fortsatt kan føles «vibe coded», er nesten bare tre ting:
1. **Tomrom som blir igjen når kortene i en rad har ulik høyde:** høyre kolonne på Kontroller, under Instruksjonssett på System og strukket innhold i Klokker på Oversikt.
2. **Samme fakta vist på ulike måter:** PCIe i fire formater, to ulike Klokker-kort, og «Simulert», «simulert» og «–» om hverandre.
3. **Tilstander som ikke er tenkt helt gjennom:** Lys-forhåndsvisningen forsvinner under ca. 1240 px, Bruk er fortsatt aktiv når appen er frakoblet, og i live-modus står det at viften «følger viftekurven».

Utviklerspråk lekker også ut flere steder (`AG_SIMULATE=1`, «Research fant …», `docs/PATCHLOG.md`).

---

## 2. Funn

Alvor: **Høy** = synlig feil eller brudd på «ingen død plass» på hovedvisningen · **Middels** = merkbart for en vant bruker · **Lav** = finpuss.

### 2.1 Skall og felles

**F1 · Middels · Frakoblet tilstand er bare delvis frosset**
- *Hva:* Med serveren stoppet blir innholdet dempet og banneret «Frakoblet fra serveren. Prøver igjen om 2s» vises. Likevel er **Bruk** og **Forkast** fullt aktive og blå på Kontroller («3 endringer ikke brukt»). Statusblokken i sidefeltet er heller ikke dempet: 72 °C og sparklinjer vises med full styrke, som om de var ferske. «2s» mangler også det smale mellomrommet som resten av appen bruker.
- *Hvorfor:* Det ser ut som du kan bruke endringer mot en server som ikke svarer. Statusblokken er det første øyet går til, og den lyver litt.
- *Forslag:* Deaktiver Bruk mens appen er frakoblet, med verktøytipset «Venter på serveren». Gi statusblokken samme dempede stil (`--dim`) og stopp sparklinjene. Skriv «2 s».
- *Bilder:* `off-kontroller.png`, `off-oversikt-16s.png`

**F2 · Middels · PCIe vises i fire formater, og i simulert modus er tallene i utakt**
- *Hva:* Oversikt viser «x16 Gen 1 (maks Gen 3)» og «x16 Gen 3». Grafikkort og enhetspopoveren viser «x16 3.0 @ x16 1.1». Sensorer viser «Gen 1», og Spesifikasjon viser «PCIe 3.0 x16». Verktøytipset blander også begrepene: «går på PCIe 1.1 nå … Kortet støtter x16 Gen 3». I simulert modus sto Oversikt på Gen 2 og Gen 1 samtidig som Grafikkort sto fast på «x16 3.0 @ x16 3.0».
- *Hvorfor:* Det er detaljer som dette som skiller et pro-verktøy fra et hobbyverktøy. «@» og «maks @ nå» (omvendt rekkefølge av Oversikt) er sjargong.
- *Forslag:* Lag én formatfunksjon som brukes overalt: «x16 Gen 1 · maks Gen 3», og bare «x16 Gen 3» når nå = maks. Verktøytipset kan gi overføringsraten («Gen 1 = 2,5 GT/s»). Grafikkort-raden og popoveren bør lese samme løpende verdi som Oversikt.
- *Bilder:* `1440-light-oversikt.png`, `1440-light-grafikkort.png`, `i-montasje1.png`

**F3 · Lav · Utviklerspråk og interne henvisninger**
- *Hva:* Enhetspopoveren sier «Simulert: Tvunget med AG_SIMULATE=1». Verktøytipset på Lys sier «Research fant sannsynligvis en Palit-lyskontroller …». Om viser til `docs/PATCHLOG.md` og `docs/STATUS.md`.
- *Forslag:* Skriv «Simulert (valgt ved oppstart av serveren)». På Lys: «Kortet har trolig en Palit-lyskontroller. Den er ikke testet, og appen sender ingenting til den.» I Om kan filstiene bare stå i Feilsøking-utvidelsen.
- *Bilder:* `i-montasje1.png`, `d-om.png`

**F4 · Lav · Forkast og Bruk vises også når det ikke finnes endringer**
- *Hva:* På Kontroller, Viftekurve og Lys står to deaktiverte knapper alltid i toppfeltet. I mørkt tema er den deaktiverte Bruk-knappen (50 % blå) lett å forveksle med en aktiv.
- *Forslag:* Vis bare statusteksten («Kurven er brukt», «Ingen endringer») når ingenting er endret, og la Forkast og Bruk tone inn når første endring kommer. Det følger libadwaita sitt mønster for «ulagrede endringer», og toppfeltet blir roligere.
- *Bilder:* `1440-dark-viftekurve.png`, `k-endret-dark.png`

**F5 · Lav · Grå «samler data»-flate i små sparklinjer ser ut som en tom stolpe**
- *Hva:* De første 60 sekundene er sparklinjene i statusblokken, flisene og 60 s-kolonnen på Sensorer en ensfarget grå boks med en strek helt til høyre. Skraveringen syns ikke i 18–20 px høyde.
- *Forslag:* Bruk ingen fyll i små sparklinjer, bare en 1 px stiplet grunnlinje der det mangler data. Behold den skraverte flaten i de store grafene, der den virker.
- *Bilder:* `1440-light-oversikt.png` (statusblokk og fliser), `1100x800-sensorer.png` (Minne-gruppen)

**F6 · Lav · Ellipse på knapper som ikke åpner en dialog**
- *Hva:* «Tilbakestill …» tilbakestiller med en gang og gir en angre-toast.
- *Forslag:* Fjern ellipsen. I GNOME betyr den at det kommer mer input. «Eksporter …» og «Importer …» er riktige.
- *Bilde:* `d-tilbakestill.png`

**F7 · Lav · Små tilgjengelighets- og konsekvensdetaljer**
- Hovedmeny-knappen (`#menu-btn`) har bare `data-tip`. Den mangler `aria-label` og har dermed tomt navn for skjermlesere. Sjekk at verktøytipskomponenten setter navnet, ellers legg det til.
- Tastatursnarveier viser «Ctrl+,» som én tast, men ↑ og ↓ som to. GNOME viser én tast per knapp: [Ctrl] [,].
- Versjonen står som «alpha 0.0.5 (bygg 5)» i Om og som «alpha v0.0.5 · bygg 5» i loggen og README. Velg én skrivemåte.
- *Bilder:* `d-snarveier.png`, `d-om.png`

### 2.2 Oversikt

**F8 · Middels · Klokker-kortet strekker radene for å fylle høyden**
- *Hva:* Radene i Klokker er 51 px høye, mens Skjermkort ved siden av har 34 px. Overskriftsraden «Nå · Maks» svever med ca. 30 px luft under kortittelen. Ved 1440×900 får dermed to nabokort ulik rytme.
- *Hvorfor:* Strukket innhold er en mer synlig «fylle-løsning» enn tomrom.
- *Forslag:* Bruk fast radhøyde (`--h-row`) i alle tre kortene. Fyll den ledige høyden i Klokker med noe nyttig, for eksempel en femte rad, «Boost (spesifikasjon) 1 710 MHz», eller en rad med årsaken til begrensningen. Se også den kjente løsningen i kapittel 3.
- *Bilde:* `1440-light-oversikt.png`

**F9 · Middels · Effektgrafen forstørrer støy når y-aksen tilpasses**
- *Hva:* Ved høy last tilpasser Effekt-grafen seg til 200–250 W. Da ser variasjon på ±10 W ut som voldsomme hopp over hele grafen. Last er låst til 0–100, så de tre grafene ved siden av hverandre får ulik «dramatikk».
- *Forslag:* Gi Tilpass et minste spenn per måletype: effekt minst 25 % av grensen og temperatur minst 20 °C. Grafen følger fortsatt dataene, men flimrer mindre.
- *Bilde:* `1920x1080-oversikt.png`

**F10 · Lav · Skjermkort-kortet gjentar flisene**
- *Hva:* GPU-last og Minnekontroller vises som stolper i Skjermkort, men står også i flisene og i Last-grafen rett under.
- *Forslag:* Bytt dem ut med noe som ikke står andre steder på siden: VRAM brukt/total med stolpe, og «Begrensning: Effektgrense» (finnes) slått sammen med P-tilstand.

### 2.3 Sensorer

**F11 · Middels · Verktøyene i toppfeltet er uklare**
- *Hva:* Ved siden av søkefeltet står «Siden 23:37» alene. Deretter kommer et ikon som ser ut som en X («Fold sammen alle», chevrons-down-up i 16 px) og et ↺-ikon («Nullstill min, maks og snitt»). X-en leses lett som «lukk» eller «tøm søk».
- *Forslag:* Skriv «Min/maks siden 23:37», eller flytt tidspunktet inn i verktøytipset på ↺ og vis det som undertittel på Min-kolonnen. Flytt «Fold sammen alle» bort fra søkefeltet, for eksempel helt til venstre for de andre ikonene og med skilletegn, eller inn i en ⋯-meny.
- *Bilder:* `1440-light-sensorer.png`, `s-montasje.png`

**F12 · Middels · Ved 360 px brytes sensornavn midt i ordet**
- *Hva:* «Temperatu/r», «Minnekontr/oller-last», «Grafikkklokk/e», «Effektgren/se».
- *Forslag:* Bruk `overflow-wrap: normal; hyphens: auto` med `lang="nb"`, eller én linje med ellipse og hele navnet i verktøytipset (mønsteret finnes allerede).
- *Bilde:* `360-utvalg.png`

**F13 · Lav · Terskelfargene mangler i tabellen**
- *Hva:* 76 °C og 97 % er gule på Oversikt og i statusblokken, men nøytrale i Nå-kolonnen på Sensorer.
- *Forslag:* Bruk samme farge på Nå-cellen, og utvid teksten i Innstillinger › Varselfarger tilsvarende.
- *Bilde:* `1100x800-sensorer.png`

**F14 · Lav · «Ikke tilgjengelig» filtreres ikke**
- *Hva:* Med filteret «xyzqq» vises «Ingen sensorer passer filteret», og under står alle åtte ikke-tilgjengelige rader i full høyde. Første rad gjentar også overskriften: «Ikke tilgjengelig – sensordriver ikke lastet».
- *Forslag:* Filtrer også denne gruppen, eller fold den sammen til én linje mens filteret er i bruk («8 sensorer kan ikke leses»). Fjern «Ikke tilgjengelig –» fra forklaringsteksten.
- *Bilde:* `s-montasje.png`

**F15 · Lav · Stort tomrom mellom navn og tall på 1440**
- *Hva:* Navnekolonnen er ca. 600 px bred, mens tallene står samlet til høyre. To kolonner slår først inn ved innholdsbredde 1300 px.
- *Forslag:* Senk grensen til ca. 1150 px innholdsbredde, eller sett en maksbredde på navnekolonnen (ca. 260 px) og la 60 s-sparklinjen ta resten. Den blir mer lesbar når den er bredere.
- *Bilde:* `1440-light-sensorer.png`

### 2.4 Grafer

**F16 · Lav · Pause synes bare på ikonet**
- *Hva:* Når grafene er pauset, er det eneste tegnet at ikonet bytter til ▷. Grafene ser ellers levende ut.
- *Forslag:* Vis et lite merke i toppfeltet («Pauset 23:41») og skriv «pauset» i stedet for «nå» på x-aksen.
- *Bilde:* `g-montasje.png`

**F17 · Lav · Tidsaksen blander enheter**
- *Hva:* Med vinduet satt til 2 min står det «−90 s · −1 min · −30 s».
- *Forslag:* Bruk én enhet per vindu: «−90 s · −60 s · −30 s», eller «−1:30 · −1:00 · −0:30» for 5 og 15 min.

### 2.5 Grafikkort

**F18 · Lav · To ulike Klokker-kort for de samme dataene**
- *Hva:* Oversikt har Klokker med stolper og «Nå/Maks». Grafikkort har en tabell uten stolper, «Maks (driver)» og en Nå-kolonne som står midt i kortet med tom plass til venstre.
- *Forslag:* Lag én komponent. På Grafikkort kan den ha en ekstra kolonne «Spesifikasjon» (grunn- og boostklokke), så den gir noe mer enn Oversikt.
- *Bilde:* `1440-light-grafikkort.png`

**F19 · Lav · Simulerte verdier er skrevet ulikt**
- *Hva:* «Bus-ID Simulert», «Driver simulert», «CUDA Simulert», og «CUDA –» i popoveren.
- *Forslag:* Vis «–» i `--dim` med verktøytipset «Simulert kilde». Merket «Simulert» i kortets topp sier det allerede.

### 2.6 System

**F20 · Middels · Tomrom under Instruksjonssett**
- *Hva:* Venstre kolonne (Prosessor, Cache, Instruksjonssett) slutter ca. 200 px før høyre (Hovedkort, Minne, Operativsystem). Ved 1440×900 er tomrommet nede til venstre det første du ser når du ruller.
- *Forslag:* Se kapittel 3. Den samme endringen løser også det kjente problemet med at «Vis alle (N)» endrer bredde.
- *Bilde:* `sys-hel.png`

**F21 · Lav · Verktøytipset om Moduler og timinger gjentar seg**
- *Hva:* Teksten er «… som bare root kan lese (dmidecode). Moduler, hastighet og timinger leses fra SMBIOS/SPD, som bare root har tilgang til.» Serverens `note` (`server/system.js:294`) og klientens egen setning (`web/js/pages/system.js:140`) slås sammen.
- *Forslag:* Bruk bare én av dem. Ta bort «(dmidecode)», for det betyr lite for brukeren.

### 2.7 Kontroller

**F22 · Høy · Høyre kolonne er tom i 600–760 px**
- *Hva:* Endringer og Profiler slutter ved y ≈ 300. Under er det tomt ned til bunnen: ca. 600 px ved 1440×900 og ca. 760 px ved 1920×1080. Samtidig bruker Spennings- og klokkekurven, som er merket «Illustrasjon», hele bredden og 350–500 px høyde.
- *Hvorfor:* Dette er det tydeligste bruddet på «ingen død plass» i appen, og det står ikke på den kjente listen.
- *Forslag:* Flytt spennings- og klokkekurven til høyre kolonne under Profiler (ca. 400 × 300 px er nok for en illustrasjon). La Justeringer ta hele venstre høyde. Når ingenting er endret, kan Endringer bli én linje, eller ligge som et lite felt øverst i Profiler, siden toppfeltet allerede sier «Ingen endringer» (se F25).
- *Bilder:* `1440-light-kontroller.png`, `1920x1080-kontroller.png`

**F23 · Middels · I live-modus påstås det at viften følger kurven**
- *Hva:* I live-modus står det i Vifte-raden «Følger viftekurven · 36 % ved 34 °C nå». Oversikt sier samtidig «Viftestyring: Driveren (auto)».
- *Hvorfor:* Raden sier noe som ikke stemmer. Brukeren har tidligere vært opptatt av ærlighet om hva som er simulert.
- *Forslag:* I live-modus: «Driveren styrer viften. Den simulerte kurven ville gitt 36 %.»
- *Bilder:* `live-kontroller.png`, `live-oversikt.png`

**F24 · Middels · Spennings- og klokkekurven endrer y-akse når offset endres**
- *Hva:* Med +100 MHz går aksen fra 1200–2200 til 1250–2250. Kurven ser dermed nesten like høy ut, og det er aksen som flytter seg.
- *Forslag:* Lås y-aksen til området som er mulig: standard-min til standard-maks + største offset (+300). Da ser du hvor mye kurven løftes.
- *Bilder:* `1440-light-kontroller.png`, `k-endret.png`

**F25 · Lav · Profiler: ordbruk og dobbel informasjon**
- *Hva:* «Lagre skjemaet som profil 1» («skjemaet» er et rart ord her). Merket «5 plasser» står sammen med «5 ledige plasser». «Profil 2 · plass 2» gjentar seg. To like profiler kan lagres uten at det sies fra. Hver rad har også fire handlinger: Hent, lagre, gi nytt navn og slett.
- *Forslag:* Skriv «Lagre som ny profil» og bruk merket «2 av 5». Vis «Lik Profil 1» i stedet for å lagre en kopi. Flytt «lagre over» og «gi nytt navn» inn i en ⋯-meny, så raden bare har Hent og ⋯.
- *Bilde:* `kl-montasje.png`

### 2.8 Viftekurve

**F26 · Middels · Kurven kan bli fallende**
- *Hva:* Punkt 2 (50 °C) kan dras til 53 %, høyere enn punkt 3 (62 °C, 50 %). Kurven går da ned mellom 50 og 62 °C.
- *Hvorfor:* En viftekurve som faller med temperaturen er nesten alltid en feil. Andre verktøy (Afterburner, LACT) tillater det ikke.
- *Forslag:* Begrens dra og tallfelt til området mellom nabopunktene, så viften aldri går ned når temperaturen stiger. Vis en kort rød kant på feltet i tabellen hvis noen skriver et ugyldig tall.
- *Bilde:* `f-montasje.png`

**F27 · Lav · Oppførsel havner under bretten**
- *Hva:* Ved 1440×900 er Reaksjonstid og Temperaturkilde skjult under kanten. Nå-kortet (Temperatur, Kurven sier, Vifte nå) gjentar det som allerede står på grafen («42 °C → 38 %») og ringen for viftefart.
- *Forslag:* Flytt «Vifte nå 38 %» inn i grafens etikett («42 °C → 38 % · vifte 38 %») og fjern Nå-kortet. Da får Oppførsel plass uten rulling.
- *Bilder:* `1440-light-viftekurve.png`, `1920x1080-viftekurve.png`

**F28 · Lav · Små tekst- og tilstandsdetaljer**
- «Kurve · egen» bør skrives «Kurve · Egendefinert», slik «Balansert» har stor forbokstav.
- Det endrede punktet er ikke merket i Punkter-tabellen. Gi det endringsprikken som brukes på Kontroller.
- Ved 360 px brytes forklaringen over tre linjer, og «Under 35 % er ikke mulig på kortet» dekker plottet.
- *Bilder:* `f-montasje.png`, `360-utvalg.png`

### 2.9 Lys

**F29 · Høy · Forhåndsvisningen er tom i vinduer smalere enn ca. 1240 px**
- *Hva:* Scenen viser bare en svart flate. `.rgb-card` får målt størrelse 0 × 0 ved vindusbredde 1200, 1150, 1100, 1000 og 360. Ved 1280 og bredere virker den.
- *Årsak (sannsynlig):* `--cw: min(100cqw − …, (100cqh − …) × 660/280)` i `.rgb-stage` med `container-type: size`. Når `.page.fill { height: auto }` slår inn (container query `max-width: 1000px`), regnes størrelsen til 0.
- *Forslag:* Bruk ikke `cqh`. Gi `.rgb-card` `width: min(100% − 2 × var(--sp-6), …)` og `aspect-ratio: 660 / 280`, og gi scenen en egen `aspect-ratio` i enkolonne-oppsettet. Legg til en sjekk i røyktesten om at `.rgb-card` er bredere enn 0 ved 1100 px.
- *Bilder:* `1100x800-lys.png`, `360-utvalg.png`

**F30 · Middels · Ved 360 px overlapper merker og navn**
- *Hva:* «Bare forhåndsvisning» ligger over «Regnbue», «Fargesyklus» og «Temperatur». Sone-segmentet går ca. 5 px ut over kortkanten.
- *Forslag:* Se F31. Uten «Støttet»-merkene trengs bare et lite øye-ikon med verktøytips. Segmentet bør kunne bryte linje eller rulle vannrett.
- *Bilde:* `360-utvalg.png`

**F31 · Lav · «Støttet» på hver effekt er støy**
- *Hva:* Tre grønne «Støttet»-merker og tre grå «Bare forhåndsvisning».
- *Forslag:* Merk bare unntakene. Det samme mønsteret ble brukt for «Simulert» og «Referanse» i forrige runde.
- *Bilde:* `1440-light-lys.png`

### 2.10 Innstillinger

**F32 · Middels · Datakilde sier det samme to ganger**
- *Hva:* «Simulert skjermkort. Tvunget med AG_SIMULATE=1. Serveren er startet med AG_SIMULATE=1. CPU, minne …». `reasonNew` fra telemetrien og tillegget `forceSim` i `web/js/pages/settings.js:101` står begge.
- *Forslag:* Bruk én av dem, uten miljøvariabelen: «Simulert skjermkort (låst ved oppstart av serveren). CPU, minne og nett er alltid ekte.»
- *Bilde:* `1440-light-innstillinger.png`

**F33 · Middels · Datamappe-stien kuttes ved kanten**
- *Hva:* Ved 1440 går stien rett ut av kortet uten ellipse, og kopier-knappen syns ikke. Ved 360 får den både ellipse og kopier-knapp. Den virkelige stien (`/run/media/julian/Transcend-128GB/…/agentic-rigg/.data`) er like lang. Etiketten og stien står også på hver sin linje med ca. 20 px luft imellom.
- *Forslag:* Bruk én rad med ellipse i midten (`/run/media/julian/…/agentic-rigg/.data`), hele stien i verktøytipset og kopier-knappen alltid synlig til høyre.
- *Bilde:* `set-light-data.png`

**F34 · Lav · Avkrysningsrutenett bruker halve bredden**
- *Hva:* «Status nederst» og «Grafer som vises» står i to smale kolonner med ca. 450 px tomt til høyre inne i listen. «Sider» har ni rader på 52 px med bare en bryter.
- *Forslag:* Bruk `grid-template-columns: repeat(auto-fill, minmax(200px, 1fr))`, som gir tre kolonner. «Sider» kan bruke samme rutenett med avkrysningsbokser. Se også kapittel 4.4, der «Status nederst» uansett blir en ordnet liste.
- *Bilder:* `set-light-sidefelt.png`, `set-light-grafer.png`

**F35 · Lav · Klokkeslett viser til en etikett som ikke finnes**
- *Hva:* Teksten er «Brukt i «Statistikk siden», profiler og grafer», men på Sensorer står det «Siden 23:37».
- *Forslag:* Skriv «Brukt i min/maks-tidspunktet på Sensorer, i profiler og i grafer.»
- *Bilde:* `set-light-enheter.png`

---

## 3. Forslag til løsning på de kjente punktene

**Lys: tomrom under Justering.** Effekt-kortet har også ca. 70 px tomt under «Av». Slå Justering inn i Effekt-kortet: først effektlisten, så en skillelinje, så Lysstyrke og eventuelt Hastighet og Retning, som bare vises for effekter som bruker dem. Raden under scenen blir da to kort, **Effekt | Farge**, som har omtrent lik høyde. Alternativt kan Lysstyrke ligge i verktøylinja under scenen («Sone [Alle …] · Lysstyrke [——] 80 % · Bakgrunnslys [ ]»), der den hører hjemme fordi den gjelder alle effekter.

**Oversikt: tomrom mellom stolpe og bunntekst i Temperatur og Effekt.** Gjør bunnteksten om til merkelapper under stolpen, plassert der grensene er («83 Mål», «97 Nedklokking», «100 Av»), som en skala. Da blir kortet tre tette lag: tittel og verdi, stolpe og skala. Gjør så kortene innholdshøye, og la Klokker (F8) bestemme radens høyde, men med fast radhøyde og en ekstra rad. Hvis høyden fortsatt ikke går opp: slå Temperatur og Effekt sammen til ett kort, «Grenser», med to målere.

**System: «Vis alle (N)» endrer bredde.** La Instruksjonssett-kortet strekke seg til samme høyde som nabokolonnen (F20) og vise så mange merker som får plass, med 110 som tall i overskriften: «Instruksjonssett · 19 utvalgte av 110». Knappen kan da være en fast ikonknapp (utvid) som ikke endrer tekst. Tomrommet blir fylt, og breddehoppet forsvinner.

---

## 4. Topp 10, mest «pro» per innsats

1. **F29 Lys-forhåndsvisningen er tom under ca. 1240 px.** Liten CSS-endring som retter en synlig feil. Legg den inn i røyktesten.
2. **F22 Kontroller: flytt spennings- og klokkekurven til høyre kolonne** og gjør Endringer til én linje når ingenting er endret. Fjerner det største tomrommet i appen.
3. **F1 Frakoblet: deaktiver Bruk og demp statusblokken.** Få linjer, og tilstanden blir troverdig.
4. **Tekstvask:** F32 (Datakilde dobbelt), F21 (dobbelt verktøytips), F3 (AG_SIMULATE, «Research»), F35 («Statistikk siden»), F25 («skjemaet»), F6 (ellipse). En time, og mye av «vibe coded»-preget forsvinner.
5. **F2 Én PCIe-formatering overalt,** også i simulert modus.
6. **F23 Live-modus: Vifte-raden skal si at driveren styrer viften.**
7. **F20 og den kjente «Vis alle»-saken:** Instruksjonssett fyller høyden med fast knapp.
8. **Oversikt (kjent punkt og F8):** skala under stolpene, fast radhøyde og en ekstra rad i Klokker.
9. **F26 og F27 Viftekurve:** kurven kan ikke falle, og Nå-kortet foldes inn i grafen så Oppførsel får plass.
10. **F11 Sensorer-toppfeltet:** «Min/maks siden 23:37», og fold-ikonet flyttes bort fra søkefeltet.

---

## 5. Ønskelisten for v0.0.6 (fra `VEIKART.md`)

### 5.1 Viftekurve: start- og stopptemperatur med hysterese
Oppførsel har allerede «Hysterese 2 °C», men den gjelder hvert trinn i kurven. Brukerens ønske er en terskel: under en viss temperatur skal kurven ikke styre, og viften ligger på minste fart (35 %, siden 0 % ikke går).
- **Plassering:** Øverst i Oppførsel, som én rad: «Kurven starter ved [55] °C · slipper under [48] °C». Under raden en dempet linje: «Hysterese 7 °C. Under dette holder viften 35 %.» Gi dagens «Hysterese» et mer presist navn, «Senk først etter [2] °C», så de to begrepene ikke blandes.
- **I grafen:** Vis to tynne loddrette stiplede linjer med små merker ved x-aksen («start 55», «stopp 48») og et svakt skravert bånd mellom dem. Begge kan dras langs x-aksen, slik punktene kan. Til venstre for stopp kan kurven tegnes som en flat linje på 35 % med etiketten «Minste fart». Det passer med 35 %-sonen som finnes.
- **Andre steder:** Kontroller › Vifte: «Følger viftekurven fra 55 °C · nå 35 % (under start)».
- Ingen nye kort og ingen ny side.

### 5.2 Min og maks som huskes mellom økter
- **Valg:** Innstillinger › Grafer og sensorer › Sensorer, under Sparklinjer: «Husk min og maks mellom økter», med undertekst «Nullstilles bare når du trykker ↺ på Sensorer.»
- **Visning:** Tidspunktet i Sensorer-toppfeltet blir med dato når den er en annen dag: «Min/maks siden 3. nov. 23:37». Snitt bør fortsatt gjelde økten, ellers mister det mening etter dager. Skriv det i verktøytipset på Snitt-overskriften: «Snitt for denne økten».
- Angre-toasten ved nullstilling er enda viktigere når tallene er flere dager gamle. Den finnes allerede.

### 5.3 Eget appikon
- Dagens ikon er en «A» i en blå sekskant. Det er generisk og ligner et logobilde, ikke et GNOME-appikon.
- **Forslag:** Et skjermkort sett skrått forfra med én tydelig vifte, på en avrundet flate i GNOME-stil (128 px, toneskala fra Adwaita, lett perspektiv og bunnskygge slik GNOME-ikonene har). Lag en symbolsk variant (16 px, én farge) til toppen av sidefeltet og som favicon. Fargen kan følge Agentic-familien (Klipp, Desktop), så appene ser beslektet ut.
- **Bruk:** Toppen av sidefeltet (20 px, symbolsk eller full farge), Om (64–96 px), favicon og senere `.desktop` og Electron.
- Ikke bruk bokstaver, RGB-regnbue eller flammer. Det er nettopp «gradient gamery».

### 5.4 Feste sensorer og bestemme rekkefølgen i statusblokken
Statusblokken er riktig sted, og ved 1440×900 er det ca. 250 px ledig over den i sidefeltet. Det er plass til omtrent 8 rader uten rulling.
- **Feste fra Sensorer:** Et feste-ikon som vises når pekeren er over raden (eller raden har fokus), helt til venstre for navnet. Festede rader har ikonet fylt hele tiden, i `--muted`. Klikk fester eller løsner. Høyreklikk på raden kan gi samme valg. Ingen ny kolonne.
- **Rekkefølge:** I Innstillinger › Sidefelt erstattes avkrysningsrutenettet med en ordnet liste, «Status nederst (5 av 8)». Hver rad har et håndtak (⋮⋮), navn og kilde i `--dim` og en ×-knapp. Dra for å flytte, eller bruk Alt+↑/↓ med tastaturet. Nederst: «+ Legg til sensor …», som åpner en popover med søk (gjenbruk filteret fra Sensorer).
- **I sidefeltet:** Høyreklikk eller en ⋯ på en statusrad: «Flytt opp», «Flytt ned», «Løsne», «Vis på Sensorer».
- Lag ikke en egen «Festet»-gruppe på Sensorer eller Oversikt. Det ville vist det samme to ganger.

---

## 6. Det som ikke bør endres

- **Skallet:** sidefelt med seksjoner, enhetskort med popover, statusblokk med terskelfarger og klikk til sensor, og ikonskinnen ved smale vinduer. Det er pent og rolig.
- **Endringsflyten** på Kontroller, Viftekurve og Lys: endringsprikk, ↺ per rad som bare vises ved avvik, «245 W → 200 W» i Endringer, Forkast/Bruk i toppfeltet, Ctrl+Enter og Ctrl+Z, og at endringer på viftekurven vises i Endringer på Kontroller.
- **Viftekurven:** dra med koordinater, spøkelse av brukt kurve, 35 %-sonen med forklaring og egen markør for temperatur nå og viftefart.
- **Grafene:** synkronisert hårkors med «for 48 s siden», «Samler data …» i store grafer, dempede grafer ved frakobling og at x-aksen går videre.
- **Innstillinger:** libadwaita-lister, sentrert kolonne med underliste, fargetemakort med forhåndsvisning og ærlige undertekster («Nettleseren kan ikke lese aksentfargen i GNOME»).
- **Tom- og feiltilstander:** tomt filter med ikon, «Ikke tilgjengelig» med lås og strek-over-ikon og forklaringer, og frakoblet-banneret med nedtelling og «Prøv nå».
- **Verktøytipsene:** mørke, korte, med hurtigtast, og også på deaktiverte knapper.
- **Typografi og tall:** to vekter, tabulære sifre, smale mellomrom og U+2212.
- **Fargetemaene** Grafitt, Sand, Fjord og Midnatt. De er diskré og virker i begge moduser.
- **Fokusringene** og sidefeltet som én tab-stopp.
- **Angre-toast** i stedet for bekreftelsesdialog.
- **Live-modus uten statusmerke** i toppfeltet. Merket vises bare når noe er unormalt, og det er riktig.

---

### Antall funn

| Alvor | Antall | Funn |
|---|---|---|
| Høy | 2 | F22, F29 |
| Middels | 13 | F1, F2, F8, F9, F11, F12, F20, F23, F24, F26, F30, F32, F33 |
| Lav | 20 | F3, F4, F5, F6, F7, F10, F13, F14, F15, F16, F17, F18, F19, F21, F25, F27, F28, F31, F34, F35 |

Til sammen 35 funn. De tre kjente punktene i kapittel 3 er ikke talt med.
