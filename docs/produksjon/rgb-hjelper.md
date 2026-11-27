# Separat RGB-hjelper

I utviklingsutgaven gir «Tillat endring av skjermkortet» under Innstillinger › Skjermkortstyring tilgang til valgt kort. Tillatelsen dekker NVML og kortlyset på Palit RTX 2080 Super JetStream med PALIT 100-kontroller. Arbeiderne har hver sin socket, sandkasse, journal og lease. Den tidligere fysiske prøven med fast blått og tilbakeføring til grønt er dokumentert nedenfor.

## Betrodd installasjon og administratoraktivering

Bryteren og «Oppdater systemkomponenter» er inngangene til administratorhandlingen. Eldre NVML-tillatelser blir først utvidet når brukeren godkjenner den nye kortbundne aktiveringen. RGB-siden sender vanlige socketforespørsler.

Systeminstallasjonen er felles for NVML og RGB og gir i seg selv ingen korttillatelse. Kjør den ene linjen i [`system-bootstrap.command`](../../app/data/hjelper/system-bootstrap.command) fra en betrodd Bash-terminal. Oppgi den fullstendige stien til `.tar.gz` og lim inn SHA-256 fra **den riktige utgivelsens side på GitHub**, ikke fra en lokal `SHA256SUMS` ved siden av nedlastingen. Sammenlign selve kommandoen med kilden fra en uavhengig betrodd kanal hvis desktopkontoen kan være kompromittert. Innstillinger viser linjen med en kopieringsknapp.

`sudo` starter bare systemets Bash med tomt miljø. Før arkivet tolkes, kopieres det inn i en ny root-eid 0700-mappe. Bare regulære pakkefiler på høyst 64 MiB kan kopieres; kommandoen følger ikke symlenker. Først etter sjekksumkontroll og kontroll av at kopien inneholder nøyaktig én installerer, kjøres denne med `python3 -I -S`. Installereren kontrollerer de faktiske bytene mot digestargumentet igjen og trekker bare ut de navngitte, regulære komponentene. Den kopierer ingen programkode fra `~/.local`. Systemmapper med feil eier, symlenker eller gruppeskrivetilgang avvises. Ikke omgå kontrollen med videre filrettigheter. En ventende NVML-gjenoppretting må fullføres før binærbytte. RGB-journalen kan følge med til den nye arbeideren.

### Ett-klikks installasjon fra appen

Når appen selv installerer eller oppdaterer systemkomponentene fra Innstillinger, kjører den `system-hent.py` som root med `pkexec python3 -I -S -c`. Tillitsmodellen er:

- **Hva som kommer fra brukerkontoen.** Skriptteksten og argumentene kommer fra appen under `~/.local`. Polkit viser derfor sin generelle dialog for `/usr/bin/python3`, og skriptet står i prosessens kommandolinje. Handlingen er ikke sikrere enn brukerkontoen som starter den. Root leser ingen filer fra hjemmemappa.
- **Hva som kontrolleres.** Arkivet for nøyaktig appens versjon og bygg lastes ned fra GitHub over TLS til en ny root-eid mappe og kontrolleres mot SHA-256-summen GitHub-API-et oppgir for den filen. Det beviser bare at filen er den som ligger på utgivelsen nå. Deretter kontrolleres identiteten: én rotmappe `agentic-rigg-VERSJON` med bare vanlige filer og `version.json` med nøyaktig samme navn, versjon og bygg. Nedgradering avvises før nettet, etter nedlastingen og under låsen.
- **Festet tillitsanker.** Pakkeskriptet legger `SYSTEMFILER.sha256` i arkivet med summene for alle systemrelevante filer (`bin/*`, `version.json`, `data/hjelper/*`). `install.sh` legger den i `~/.local/share/agentic-rigg/` (et kildebygg fjerner den). Appen sender summene med, og root krever at arkivet har nøyaktig de filene med nøyaktig de summene. En utgivelse som er byttet ut etter at brukeren installerte appen, avvises da før noe kjøres. Summene må høre til appens egen `version.json`; ellers stopper appen før pkexec. Et kildebygg uten summene har bare GitHub-summen og identitetskontrollen.
- **Låsing.** Henteren kontrollerer bruker, kortbindinger og journalstatus før nedlasting og på nytt under systemlåsen. Den holder samme lås gjennom installasjon og den valgte kortoperasjonen. Barneprosessen kontrollerer at det arvede filhåndtaket peker på riktig låsefil. Installereren stopper begge arbeiderne og publiserer den nye systemrevisjonen først etter filbyttet.

**Ikke innført (krever brukerens beslutning):** En signert `SHA256SUMS` (for eksempel Ed25519/minisign) med en offentlig nøkkel innebygd i appen ville også dekke første installasjon og et kildebygg. Det krever en dedikert signeringsnøkkel som bare utgiveren kan lage og oppbevare, og et signeringstrinn i utgivelsesrutinen. En fast, root-eid hentehjelper med egen polkit-handling ville gi en dialog som navngir Agentic Rigg i stedet for `python3`; det krever en ny polkit-handling og en avklart førstegangsinstallasjon.

**Fra kilde:** Bruk en separat betrodd byggkonto eller byggeverktøykjede. I en ren, gjennomgått utsjekk på den godkjente fulle commit-ID-en, uten delt skrivbar Cargo-cache eller byggmappe, kjør fra repo-roten:

```sh
export CARGO_TARGET_DIR="$(mktemp -d)"; ut=$(mktemp -d)
(cd app && cargo build --release --locked --workspace) && python3 -I -S app/scripts/pakk-utgivelse.py --bin-dir "$CARGO_TARGET_DIR/release" --out-dir "$ut" --commit "$(git rev-parse HEAD)" && cat "$ut/SHA256SUMS"
```

Overfør arkivet og gi administratoren den nøyaktige SHA-256 via en separat autentisert kanal. Bruk **samme installasjonslinje** som for en Release. Godkjenningen gjelder de konkrete pakkebytene, ikke bare commitnavnet: kilde, avhengigheter, kompilator, byggkonto og pakking forblir betrodd. En sjekksum beregnet i etterkant av desktopbrukeren gir ingen slik garanti. Denne uferdige PR-en har ingen publisert Release med rettelsene.

Valg av pakke og installasjonen ovenfor er administratorens godkjenning av kode som skal kunne kjøre som root. Det forutsetter tillit til pakkekilden, verifikasjonen, systemets polkit, Python, systemd, root-eide biblioteker og administratorer. Senere polkit-dialoger gjelder bare den navngitte kortoperasjonen. Et kompromittert program i hjemmemappen kan be om en handling eller gi misvisende tekst. Root-koden validerer argumenter, bruker og fersk kortbinding.

## Kortbundet oppstart og avklaring

Koordinatoren `agentic-rigg-card.service` aktiveres under `multi-user.target`. Ved aktivering og omstart leser den den root-eide tillatelsen, sjekker bruker, PCI, UUID, bygg og generasjon og henter ferske enhetsnumre. NVML får `/dev/nvidiactl` og én `/dev/nvidiaN`. RGB får én `/dev/i2c-N` etter kontroll av NVIDIA-driver, port, direkte forelder, beskyttede klienter og enhetsnummer. Begge bruker `DevicePolicy=closed`. Manglende driver eller adapter gir ventestatus og et nytt oppstartsforsøk. En ukjent kontrollersignatur gir «Ikke støttet». NVML kan brukes mens RGB venter. Bryteren i Innstillinger viser den lagrede kortbindingen gjennom oppstartsventing og arbeiderfeil. Ved manglende socket viser daemonen ventestatus frem til koordinatoren har meldt arbeideren klar. Forsvinner socketen etter klarmeldingen, vises feil. Statusrapporten må samsvare med tillatelsens PCI, UUID og generasjon.

Den nye kontrakten bruker NVML-protokoll 3 og RGB-protokoll 4. Migrering fra bygg 32/33 trenger én administratorgodkjenning. Tjenestene kan stoppes selv om RGB-journalen står. Eldre RGB-skripter trengs ikke for å avklare lyset.

RGB-arbeideren identifiserer kontrolleren og leser farge og lysstyrke. På et nytt kort bekrefter brukeren først «Jeg ser én fast farge». Ved en journal eller avvik etter oppstart viser siden den ferske avlesningen, det lagrede lyset og eventuell fabrikkreferanse. Vanlige skrivinger er sperret under avklaringen.

- **Gjenopprett lagret lys / Av** gjør et journalført forsøk. Arbeideren leser tilbake og ber om visuell bekreftelse når den ikke kan lese effektmodusen.
- **Behold lyset jeg ser** krever en eksplisitt visuell bekreftelse bundet til kortet, arbeideren, forsøket og en ny samsvarende avlesning. En tidligere journal arkiveres som uavklart. Det lagrede ønsket beholdes til brukeren velger et nytt varig utgangspunkt.
- **Slå av** i Innstillinger stopper arbeidere, aktiv effekt og Hold. En gjenstående journal og det tilhørende grunnopptaket kan avklares ved neste aktivering.

## Filer og identitet

| Sti | Eier og modus | Formål |
|---|---|---|
| `/etc/agentic-rigg/helper.json` | root, 0644 | Felles tillatelse med UID, PCI, UUID, generasjon og bygg |
| `/etc/agentic-rigg/rgb.json` | root, 0600 | RGB-binding for samme bruker, kort og generasjon |
| `/var/lib/agentic-rigg-rgb/` | root, 0700 | Privat tilstandsmappe |
| `baseline.json` i tilstandsmappen | root, 0600 | Brukerbekreftet statisk utgangspunkt for gjeldende økt |
| `baseline-N.json` og eventuell `visual-confirmation-N.json` i tilstandsmappen | root, 0600 | Bevarte utgangspunkt og kvitteringer fra tidligere, rene økter |
| `journal.json` i tilstandsmappen | root, 0600 | Skrivehensikt, tidligere valg og gjenopprettingsansvar |
| `visual-confirmation.json` i tilstandsmappen | root, 0600 | Siste varige brukerkvittering med mål og ferske felt; ingen påstand om avlest modus |
| `worker.lock` og `admin.lock` i tilstandsmappen | root, 0600 | Arbeiderlås og separat serialisering av administratorhandlinger |
| `/var/lib/agentic-rigg-rgb.inactive` | root, 0644 | Lesbar bekreftelse for avinstallereren etter fullført RGB-deaktivering |
| `/run/agentic-rigg-rgb/` | root, 0755 | Socketmappe uten andres skrivetilgang |
| `helper.sock` i socketmappen | autorisert bruker, 0600 | Separat NDJSON-forbindelse |

Produksjonsstiene er faste. Lageret åpner hvert stilledd uten symlink-følging og kontrollerer type, eier, modus, hardlenker og størrelse. Arbeideren holder låsen og revaliderer mappe, lås, original og forventet journal. Tester velger private mapper og falsk maskinvare, uten fallback til vertens GPU- eller RGB-hjelper.

Socketklienten må ha tillatelsens faktiske `SO_PEERCRED`-UID. Tillatelsen lastes på nytt før RPC-er og periodisk; feil, bortfall eller endring tilbakekaller domenet og stopper arbeideren etter livssyklusfrigivelse. En ny tillatelse bytter ikke kort eller bruker i en kjørende arbeider.

## App og daemon

«Lys på kortet» styrer én fysisk sone. Hjelperens statusfelt `softwareEffects` angir hvilke programvaredrevne effekter panelet kan tilby. Redigering av farge og lysstyrke sender ingen endring; bare «Bruk» gjør det. Behold/Rull tilbake bruker RGBs egen bekreftelse, uten NVML-lås eller GPU-bekreftelseslinje. Profilvalg og oppstart skriver ikke RGB. Når ingen støttet kontroller er klar til styring, viser siden status og støttekrav, uten farge- eller effektkontroller.

Daemonens `get_rgb_helper` gir `state`, `message`, `status`, `owned`, `busy` og `systemAdmin`. Bare vanlig produksjonsoppsett med NVML og vertens rot kan bruke vertens RGB-hjelper. Simulering og utvikling trenger en uttrykkelig valgt, kanonisk, privat `0700`-rot eid av daemonbrukeren. Uten den er RGB slått av i daemonen. `owned` gjelder denne UI-forbindelsen, ikke arbeiderens interne forbindelsesnummer. Uten `refresh` brukes cache; oppdatering og mutasjoner går på en egen IPC-tråd. Køen er begrenset, totalfristen er fire sekunder fra innlegging, og hver IPC-runde har høyst tre sekunder innenfor totalfristen. `CLOCK_BOOTTIME` kontrollerer også alder etter suspend: for gamle jobber sendes ikke, og foreldede svar tas ikke som nytt suksessbevis. Et tidsavbrudd beviser ikke at et allerede sendt maskinvarekall var uten virkning.

Mens UI-forbindelsen eier RGB, sender daemonen heartbeat hvert annet sekund. Feilende mutasjoner får ikke skyve denne takten fremover. UI-et sender ingen heartbeat og avstemmer bare status på Lys-siden: omtrent 500 ms ved eid/ventende endring, ellers to sekunder. Sidebytte stanser UI-pollingen, mens en beholdt farge fortsatt eies av det åpne vinduet. «Avslutt lysstyring» frigir eksplisitt; tap av UI-forbindelsen avslutter daemonens RGB-forbindelse.

## Arbeiderens RPC

Meldinger følger NDJSON med `id`, `op` og `args`. Ukjente og dupliserte felter avvises. En arbeider med det gamle byggnummeret uten statusfeltet `strictRequests: true` behandles som utdatert. `gpu` og `uuid` må være identiske med root-tillatelsens kort.

| Operasjon | Argumenter | Oppførsel |
|---|---|---|
| `rgb_status` | tomt objekt | Ren cache, ingen I2C og ingen leasefornyelse |
| `rgb_apply` | gpu, uuid, request, confirmTimeoutS og eventuelt effect | Fast farge/av; 10–60 sekunders bekreftelsesfrist, 15 sekunder fra UI |
| `rgb_keep_observed` | gpu, uuid, confirmedStatic=true, expected, recoveryId | Bekreft fersk avlesning og avslutt eventuell journalavklaring |
| `rgb_confirm` | gpu, uuid, txn | Behold med fersk tilbakelesing |
| `rgb_revert` | gpu, uuid, txn | Gjenopprett forrige bekreftede appvalg |
| `rgb_reset` | gpu, uuid | Gjenopprett lagret utgangspunkt ved uavklart journal. I en ren økt gjøres bare ny feltlesing. |
| `rgb_read` | gpu, uuid | Fersk feltlesing i en ren økt uten setter eller leasefornyelse. «Sjekk igjen» bruker denne veien. |
| `rgb_release` | gpu, uuid | Frigi egen forbindelse og gjenopprett egne endringer |
| `rgb_heartbeat` | gpu, uuid | Forny bare denne forbindelsens egen RGB-lease; daemonoperasjon |
| `rgb_ack_recovery` | gpu, uuid, confirmedStatic=true, expected, recoveryId | Kontroller forsøksidentitet, mål og ferske felt før visuell kvittering lagres |

`request` og ACK-feltet `expected` har samme typede format:

    {"mode":"static","color":[R,G,B],"brightness":80}
    {"mode":"off"}

Ingen buss, enhetssti, registeradresse eller EEPROM-valg mottas fra klienten. `expected` er bare en sammenligningsbetingelse, aldri et nytt skrivemål. Statusens `observationSource` er `stored_baseline`, `operation_readback` eller `unknown`. Lagret farge er ikke en fersk måling. `observed.mode` forblir `null`, også etter vellykket apply, Behold og menneskelig bekreftelse.

Ved en animasjon er `request` den statiske startfargen og `effect` er `breathing`, `rainbow` eller `cycle`. Start krever et rent, bekreftet statisk utgangspunkt. «Bruk» starter én journalført prøve med startfargen. Først når brukeren velger «Behold», beregner RGB-hjelperen neste farge fra den monotone klokken. Statusfeltet `effect` angir valgt programvareeffekt. Feltet `observed` viser sist leste farge og lysstyrke.

Hjelperen kan sende inntil fire fargeoppdateringer per sekund, hopper over identiske farger og spiller aldri av en kø med tapte bilder. Den opprinnelige transaksjonsjournalen dekker hele animasjonen. Bildefrekvensen øker derfor ikke antall synkroniseringer av journalen. «Stopp» frigir økten og gjenoppretter utgangspunktet. Hvis en annen aktør har endret fargefeltene før en ny ramme, stopper hjelperen og bevarer journalen frem til eksplisitt avklaring. Hvis en ramme kan være delvis skrevet, forsøker hjelperen én tilbakeføring til originalen. [Effektforløp og fysisk prøvestatus](palit-rgb-2026-11-17.md#programvaredrevne-lyseffekter).

## Journal, frister og avbrudd

Før skriving lagres full journal i en ny privat tempfil, filen synkroniseres, byttes atomisk og foreldremappen synkroniseres. Originalen og tidligere eget skriveansvar bevares. Fasen `prepared` alene beviser verken et setterforsøk eller urørt kort. Etter retur registreres `attempted` eller `rejected_before_write`. Ved ny serverstart gir en eksisterende journal avklaringsmodus på socketen. Hvis omdøpingen ble synlig før synkronisering feilet, vises journalen som uavklart i gjeldende arbeider også. En slik filfeil starter ingen automatisk setter. I en ren økt kan en kortvarig lesefeil prøves på nytt med «Sjekk igjen». Gjenoppretting er tilgjengelig på RGB-siden og deaktivering i Innstillinger.

Avvisning før første eget skrivekall gir ingen første skriving ved klienttap eller nedstenging. Mulig delvis endring beholder gjenopprettingsansvaret. Recovery journalføres før forsøket og velger bare forrige bekreftede valg eller originalen, med samme kortbinding.

Bekreftelsesfristen og den separate 15-sekunders leasen bruker `CLOCK_BOOTTIME`, inkludert hviletid. Vellykket apply/Behold, egen heartbeat og godkjent visuell ACK av et fortsatt eid, ikkeopprinnelig valg kan fornye leasen. Status, feil og andre klienter gjør ikke det. Original-ACK fjerner eierskapet.

Det gjøres ett gjenopprettingsforsøk **per hendelse**: pending-frist bruker forrige valg; leaseutløp/klienttap bruker originalen. Hendelsene kan gi to forsøk, også til samme farge. Ingen ubegrenset timerløkke startes. Samtidige frister prioriterer leasefrigivelsen.

Serveren har høyst fire klienter og avgrensede rammer/byte per runde. Stopp sjekkes mellom RPC-er; kødata fra lukket forbindelse avvises. Socket-I/O er ikke-blokkerende. I2C-kall kan fortsatt blokkere RGB-prosessen; derfor deles den ikke med NVML-viftekurven eller nødregelen.

## Menneskelig bekreftelse og avsluttet arbeider

Samsvarende fargefelt med ukjent modus gir `recovery_needs_confirmation`. Dialogen viser farge og lysstyrke eller slukket lys, samt en tom avkryssingsboks. Den fanger målet og `recoveryId` når den åpnes. Hjelperen utsteder en ny identitet for hvert forsøk, også for samme mål, og en ny arbeider får et nytt tilfeldig prefiks. ACK behandler utløpte frister før den sammenligner identitet og mål. Sammenligningen kan aldri velge et nytt skrivemål.

ACK leser de bundne fargefeltene på nytt og lagrer `userVisual`-kvittering med ukjent modus. For et annet tidligere bekreftet valg beholdes journal, ansvar og lease. For originalen synkroniseres kvitteringen før journalen ryddes. En filsystemfeil rapporteres også når en omdøping eller sletting allerede er synlig. Arbeideren validerer eldre kvitteringer før neste journalførte skriverunde. En korrupt kvittering med trygg eier, filtype og modus kan erstattes ved gyldig avklaring av en eksisterende journal, eller arkiveres ved ny aktivering etter ren deaktivering.

### Stoppet eller inkompatibel arbeider

Velg «Oppdater systemkomponenter» i Innstillinger for å migrere samme kort. En felles tillatelse kan tilbakekalles av systemadministratoren med `system-install --revoke nvml` eller `--revoke rgb`. En uavklart NVML-gjenoppretting beholder kortbindingen med skrivetilgang avslått. Et fysisk fjernet kort kan arkiveres med `--archive-removed-card DOMENE UUID` etter kontroll av UUID og stoppet arbeider. Arkivering bevarer tilstanden uten å bekrefte fysisk gjenoppretting. Full avinstallasjon gjennom `--uninstall` krever at tillatelsene er fjernet og journalene avklart eller arkivert.

## Tidligere verifikasjon og fysisk prøve

Fase 4 er kontrollert med ren workspace-clippy og **536 beståtte tester, 4 ignorerte, ingen feil**. Produksjonsbinærene er bygget og oppgir **1.4.1 · bygg 24**. Privat GTK-QA ga tolv PNG-er: sju merkede visningstilstander og fem bilder fra vanlig GTK/daemon-flyt mot en eksplisitt merket fake-arbeider. Faktiske størrelser var 1920×1048, 1440×1000 og 776×900; forespurt bredde 640 px ble begrenset til 776 px. **640 px og mindre bredder er ikke verifisert.** Funksjonsprøven dekker redigering uten mutasjon, Bruk, Behold, Reset, avvist gammel ACK, aktiv checkbox-ACK, Av og Frigi. Fake-spor er ikke fysisk lysbevis.

Kontrollene bruker private røtter, simulerte kort og falske svar; administratorhandlinger er sperret i `rgb-flow`.

Den 20. november 2026 identifiserte den lesende proben én PALIT 100-sone på NVIDIA-port 1, adresse `0x49`. Brukeren bekreftet fast grønt (`#45C800`, 100 %) før aktivering med polkit. Produksjonshjelperen leste tilbake blått (`#0000FF`, 100 %) etter Bruk og den opprinnelige grønnfargen etter ti sekunders frist. Brukeren så blått og deretter grønt igjen. Hjelperen leste feltene på nytt, lagret bekreftelsen som `userVisual` og ryddet journalen. RGB-testtilgangen ble deaktivert. Prøven gikk gjennom hjelpersocketen. GTK-betjening er prøvd i simulering. Av-modus og endring av lysstyrke på kortet gjenstår.

## Varig utgangspunkt, «Av» og effekter (1.7.0)

- **Varig utgangspunkt.** `rgb_persist` gjør et beholdt fast valg eller «Av» til nytt utgangspunkt, uten passord. Hjelperen krever da at ingen prøve venter, at ingen effekt kjører, og at en fersk avlesning stemmer med valget.
  - Lagringen tåler avbrudd. Rekkefølgen er:
    1. Fabrikklyset sikres i en fil for kortbindingen. Den eldre `fabrikk-baseline.json` godtas bare ved samsvarende binding.
    2. Det nye utgangspunktet skrives til `baseline-neste.json`.
    3. Journalen fjernes, og den gamle kvitteringen flyttes til `visual-confirmation-erstattet.json`.
    4. Det nye utgangspunktet byttes atomisk inn som `baseline.json`.
  - Et avbrudd fullføres ved neste oppstart (`finish_adopt`), før utgangspunktet leses. Står journalen fortsatt (avbrutt før trinn 3), ble valget aldri tatt i bruk: `baseline-neste.json` fjernes, og den gamle økten avklares som før. Er journalen fjernet (avbrutt mellom trinn 3 og 4), byttes `baseline-neste.json` inn. Kortets nye lys kan dermed aldri stå uten journal mot et gammelt utgangspunkt. Hvert utfall noteres i systemjournalen.
  - Etterpå er økten ren. Når appen lukkes, skrives ingenting, og ingen økt står uavklart.
  - «Tilbake til fabrikklyset» prøver fabrikklyset som et vanlig valg, og «Behold» gjør det til utgangspunktet igjen.
- **«Av».** «Av» er en egen bryter på RGB-siden, atskilt fra modus og effekt.
  - Skrivemåten velges i `rgbAv`, også under «Detaljer og feilsøking» på RGB-siden. Standarden `hold` er svart med 0 %, skrevet på nytt hvert 2. sekund mens appen er åpen. Se [rgb-av-prove.md](rgb-av-prove.md) for den fysiske prøven.
  - «Behold også etter at appen lukkes» (`rgbVarig`) lagrer beholdte faste valg og «Av» som varige. Den står fortsatt på som standard: et avvik etter kaldstart låser ikke lenger RGB (se «Avvik» nedenfor), så brukerens ønske om varig «Av» koster ikke en låst økt.
  - Om kortet selv holder lyset etter omstart, er ikke verifisert. Oppstartskoordinatoren leser tilstanden på nytt.
- **«Hold» (`rgb_hold`).** Hjelperen skriver det kortet skal vise på nytt hvert 2. sekund, uten en ny prøve og uten effekt. Målet er det kortet allerede skal vise:
  - under en prøve av «Av»: den ventende verdien, dekket av prøvens journal, så kortet ikke går tilbake til fabrikklyset mens fristen løper;
  - et beholdt «Av» som ikke er lagret som varig: som et effektbilde under den beholdte journalen;
  - et «Av» som er lagret som varig utgangspunkt, i en ren økt: **uten journal**.
  - «Hold» stopper av seg selv når målet endres (ny prøve, tilbakerulling, gjenoppretting), ved feil, når forbindelsen lukkes og når leasen går ut. riggd holder leasen og forbindelsen så lenge appen er åpen. Når appen lukkes, skrives ingenting.
  - Appen starter «hold» igjen etter hver ny forbindelse til tjenesten (appstart eller omstart av tjenesten), én gang, når «Av» er lagret som utgangspunkt, skrivemåten er `hold` og hjelperen allerede har lest kortet likt utgangspunktet. «Slutt å holde av» stopper «hold» for resten av økten.
- **Hvorfor «hold» av et varig «Av» skrives uten journal (S5/S6).** Journalen finnes for skrivinger som kan etterlate kortet annerledes enn det lagrede utgangspunktet. Hver «hold»-skriving i en ren økt skriver bare utgangspunktets egne verdier, og bare når en fersk avlesning allerede viser utgangspunktet i alle lesbare felt. Transporten sammenligner på nytt på samme fd rett før første setter. Et strømbrudd eller krasj midt i skrivingen kan da bare etterlate verdier registrene allerede hadde, eller utgangspunktets modus (60), som ikke kan leses. Det er ingen endring å gjenopprette. Derfor lages det ingen journal, og ingen journal ryddes stille: etter et krasj starter hjelperen rent, uten passord og uten uavklart økt. Kortet viser da det lagrede «Av», eller det kortet selv har gått tilbake til (se «Avvik»). En prøve som ikke er beholdt, og et «Av» som ikke er lagret som varig, har fortsatt vanlig journal.
- **Avvik i en ren økt (S3).** Viser en fersk avlesning noe annet enn det lagrede utgangspunktet uten at hjelperen har skrevet (for eksempel kort som går tilbake til fabrikklyset etter kaldstart eller hvile), er det ingen sperre. Status har `drift: true`, «hold» starter ikke, og RGB-siden viser en boks med «Gjenopprett utgangspunktet». `rgb_reset` gjenoppretter da med journal før første setter, og etterpå bekrefter brukeren lyset med ett klikk. Det krever ikke passord.
- **Oppgradering til 1.7.1.** Begge arbeidere krever bygg 34. Innstillinger › Skjermkortstyring oppdaterer systemkomponentene og ber om ny kortbundet aktivering. Deretter viser RGB-lys eventuell uavklart journal.
- **Effekter.** Effektene kjører bare mens appen er åpen.
  - Når appen lukkes, eller lyset frigis, under en effekt eller «hold» av et beholdt valg, skriver hjelperen selv utgangspunktet med den faste sekvensen og leser alle felt tilbake. Stemmer de, avsluttes økten uten en ny visuell bekreftelse (`settle_after_animation`).
  - En prøve som ikke ble beholdt, eller en tilbakelesing som ikke stemmer, krever fortsatt visuell bekreftelse som før.
- **Sporing.** Hvert RGB-kall fra appen har en kilde, for eksempel `bruk-knapp`, `etter-aktivering`, `varig-etter-behold`, `hold-start`, `hold-oppstart` eller `fabrikklys`.
  - Oppfølgingsstegene (varig etter «Behold», «hold» etter «Bruk») bruker eierskapet fra operasjonssvaret (`owned`, lagt til av riggd), ikke en eldre status. Blir et steg ikke sendt, sier siden fra; det falt før stille bort (B1). riggd logger én linje per kall på stderr (`RGB: økt N rgb_apply fra … valg=…`), og hjelperen får aldri feltet.
  - Et «Bruk» trykket før aktiveringen sendes bare én gang, innen 120 s og bare uendret. I 1.6.0 ble et gammelt valg (`#3A7BF0`, 0 %) sendt da RGB-hjelperen ble startet på nytt utenfra.
  - Hurtigfargene bytter ikke lenger modus i det skjulte.
