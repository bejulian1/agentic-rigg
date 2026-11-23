# Separat RGB-hjelper

RGB har et eget domene i hjelperbinæren, en separat systemd-prosess, daemonklient og kontroller på Lys-siden. Det deler ikke motor, socket, tillatelse, grunnopptak eller lease med NVML-styringen. Aktivering, menneskelig bekreftelse og manuell gjenoppretting er implementert og prøvd med isolerte fakes. På brukerens Palit RTX 2080 Super JetStream er fast blått og automatisk tilbakeføring til grønt fysisk bekreftet ved 100 % lysstyrke.

## Betrodd installasjon og administratoraktivering

Desktop-appen kan installeres i `~/.local`. RGB-aktivering bruker faste programmer i `/usr/local/libexec/agentic-rigg-rgb-v2/`. GTK starter `/usr/bin/pkexec` med den samme stien som står i RGB-handlingens `org.freedesktop.policykit.exec.path`. De fire handlingene krever `auth_admin` i en aktiv lokal økt. De tar kortets PCI/UUID og handlingsspesifikke bekreftelser. Bruker-UID kommer fra polkit. Inngangen godtar ingen pakkesti, testrot, vilkårlig binær, buss eller klientvalgt skriveadresse.

Systeminstallasjonen er felles for NVML og RGB og gir i seg selv ingen korttillatelse. Kjør den ene linjen i [`system-bootstrap.command`](../../app/data/hjelper/system-bootstrap.command) fra en betrodd Bash-terminal. Oppgi den fullstendige stien til `.tar.gz` og lim inn SHA-256 fra **den riktige utgivelsens side på GitHub**, ikke fra en lokal `SHA256SUMS` ved siden av nedlastingen. Sammenlign selve kommandoen med kilden fra en uavhengig betrodd kanal hvis desktopkontoen kan være kompromittert. Begge sidene i appen viser linjen med en kopieringsknapp.

`sudo` starter bare systemets Bash med tomt miljø. Før arkivet tolkes, kopieres det inn i en ny root-eid 0700-mappe. Bare regulære pakkefiler på høyst 64 MiB kan kopieres; kommandoen følger ikke symlenker. Først etter sjekksumkontroll og kontroll av at kopien inneholder nøyaktig én installerer, kjøres denne med `python3 -I -S`. Installereren kontrollerer de faktiske bytene mot digestargumentet igjen og trekker bare ut de navngitte, regulære komponentene. Den kopierer ingen programkode fra `~/.local`. Systemmapper med feil eier, symlenker eller gruppeskrivetilgang avvises. Ikke omgå kontrollen med videre filrettigheter. Ved oppgradering må RGB først være deaktivert. Bruk den gamle appen før du oppdaterer, eller de allerede root-installerte v2-kommandoene nedenfor for en eldre aktiv v2-økt; godkjenn aldri et gammelt skript i hjemmemappen. En eksisterende NVML-tjeneste stoppes med sin gjenoppretting før en ren binding tilbakekalles. Uavklart tilstand blokkerer utskifting.

### Ett-klikks installasjon fra appen

Når appen selv installerer eller oppdaterer systemkomponentene (GPU-bryteren eller «Aktiver kortlys»), kjører den `system-hent.py` som root med `pkexec python3 -I -S -c`. Tillitsmodellen er:

- **Hva som kommer fra brukerkontoen.** Skriptteksten og argumentene kommer fra appen under `~/.local`. Polkit viser derfor sin generelle dialog for `/usr/bin/python3`, og skriptet står i prosessens kommandolinje. Handlingen er ikke sikrere enn brukerkontoen som starter den. Root leser ingen filer fra hjemmemappa.
- **Hva som kontrolleres.** Arkivet for nøyaktig appens versjon og bygg lastes ned fra GitHub over TLS til en ny root-eid mappe og kontrolleres mot SHA-256-summen GitHub-API-et oppgir for den filen. Det beviser bare at filen er den som ligger på utgivelsen nå. Deretter kontrolleres identiteten: én rotmappe `agentic-rigg-VERSJON` med bare vanlige filer og `version.json` med nøyaktig samme navn, versjon og bygg. Nedgradering avvises før nettet, etter nedlastingen og under låsen.
- **Festet tillitsanker.** Pakkeskriptet legger `SYSTEMFILER.sha256` i arkivet med summene for alle systemrelevante filer (`bin/*`, `version.json`, `data/hjelper/*`). `install.sh` legger den i `~/.local/share/agentic-rigg/` (et kildebygg fjerner den). Appen sender summene med, og root krever at arkivet har nøyaktig de filene med nøyaktig de summene. En utgivelse som er byttet ut etter at brukeren installerte appen, avvises da før noe kjøres. Summene må høre til appens egen `version.json`; ellers stopper appen før pkexec. Et kildebygg uten summene har bare GitHub-summen og identitetskontrollen.
- **Låsing.** Tillatelsene leses før nettet og på nytt etter nedlastingen; har noe endret seg, stopper alt før en fil er byttet. RGB avsluttes med de gamle, installerte skriptene, som tar `/run/agentic-rigg-system.lock` selv. Deretter holder `system-hent.py` den felles låsen gjennom ny kontroll, installasjon, gjenaktivering av GPU-styringen og den valgte aktiveringen. De nyinstallerte skriptene arver låsen som et filhåndtak (`AGENTIC_RIGG_SYSTEM_LOCK_FD`) og godtar det bare når det er nøyaktig låsefila og låsen fortsatt er forelderens. En tilbakekalt tillatelse gis aldri tilbake.

**Ikke innført (krever brukerens beslutning):** En signert `SHA256SUMS` (for eksempel Ed25519/minisign) med en offentlig nøkkel innebygd i appen ville også dekke første installasjon og et kildebygg. Det krever en dedikert signeringsnøkkel som bare utgiveren kan lage og oppbevare, og et signeringstrinn i utgivelsesrutinen. En fast, root-eid hentehjelper med egen polkit-handling ville gi en dialog som navngir Agentic Rigg i stedet for `python3`; det krever en ny polkit-handling og en avklart førstegangsinstallasjon.

**Fra kilde:** Bruk en separat betrodd byggkonto eller byggeverktøykjede. I en ren, gjennomgått utsjekk på den godkjente fulle commit-ID-en, uten delt skrivbar Cargo-cache eller byggmappe, kjør fra repo-roten:

```sh
export CARGO_TARGET_DIR="$(mktemp -d)"; ut=$(mktemp -d)
(cd app && cargo build --release --locked --workspace) && python3 -I -S app/scripts/pakk-utgivelse.py --bin-dir "$CARGO_TARGET_DIR/release" --out-dir "$ut" --commit "$(git rev-parse HEAD)" && cat "$ut/SHA256SUMS"
```

Overfør arkivet og gi administratoren den nøyaktige SHA-256 via en separat autentisert kanal. Bruk **samme installasjonslinje** som for en Release. Godkjenningen gjelder de konkrete pakkebytene, ikke bare commitnavnet: kilde, avhengigheter, kompilator, byggkonto og pakking forblir betrodd. En sjekksum beregnet i etterkant av desktopbrukeren gir ingen slik garanti. Denne uferdige PR-en har ingen publisert Release med rettelsene.

Valg av pakke og installasjonen ovenfor er administratorens godkjenning av kode som skal kunne kjøre som root. Det forutsetter tillit til pakkekilden, verifikasjonen, systemets polkit, Python, systemd, root-eide biblioteker og administratorer. Senere polkit-dialoger gjelder bare den navngitte kortoperasjonen. Et kompromittert program i hjemmemappen kan be om en handling eller gi misvisende tekst. Root-koden validerer argumenter, bruker og fersk kortbinding. Administratoren må kjenne igjen den forventede polkit-handlingen og selv se på det fysiske lyset. Ved senere root-oppdatering bør kortets tilgang deaktiveres og journalen avklares først, slik at enheten, Python-koden, hjelpen og policyen blir oppdatert samlet.

«Aktiver kortlys» krever at brukeren ser på kortet og bekrefter én fast farge før polkit. Ved ukjent eller bevegelig effekt må aktiveringen avbrytes. Den betrodde hjelperen identifiserer kontrolleren, kontrollerer adaptermetadata før og etter lesing og lagrer et kortbundet, statisk grunnopptak før tillatelsen. En journal sperrer ny aktivering. Aktiveringen sender ingen fargekommando.

RGB bruker protokoll **3**, domenet **palit_rgb** og laveste kompatible RGB-bygg **29**. Protokoll 3 lar eierforbindelsen bytte trygt mellom fast farge, av og programvareeffektene uten å måtte nullstille til utgangspunktet mellom hvert modusbytte; opprinnelig baseline og journal bevares alltid. Et bytte fra en aktiv effekt leser kortet først og sammenligner med siste skrevne ramme. Avvik sperrer automatisk gjenoppretting uten skriving; en lesefeil stopper effekten og frigir til utgangspunktet. Rull tilbake etter et slikt bytte går til siste ramme, ikke tilbake til effekten. Eldre RGB-hjelpere (protokoll 1 og 2, bygg 25–28) er utdaterte. «Aktiver kortlys» henter og installerer utgivelsen (`system-hent.py`), slår først av den gamle RGB-tilgangen med den installerte `rgb-deaktiver.sh` (lyset og utgangspunktet beholdes), og aktiverer den nye. En uavklart journal avsluttes i samme godkjenning bare for det bundne kortet brukeren valgte (GPU-bryteren eller et annet kort stopper `system-hent.py` før nettet). Etter kontrollert arkiv kjøres `rgb-gjenopprett.sh --restore-original` (lagret utgangspunkt skrives og må leses likt) og `rgb-avklar.sh --inspect`. Avlesningen kan ikke bekrefte effektmodusen, så skriptet sender så avklaringsmålet (kort, forventet lys og gjenopprettingsidentitet) til appen som én linje på stdout og venter i høyst 15 minutter. Appen viser den vanlige avklaringsdialogen med det lagrede lyset, og bare når brukeren ser på kortet og bekrefter, svarer den med nøyaktig den gjenopprettingsidentiteten på stdin. Først da kjøres `rgb-avklar.sh --static-confirmed --recovery-id`. Lukkes dialogen, byttes kortet, svarer appen noe annet eller lukkes stdin, beholdes journalen og ingenting byttes. (I 1.5.6 ble bekreftelsen gitt før gjenopprettingen; det er rettet etter issue #17.) Feiler et trinn, beholdes journalen og ingenting byttes. En GPU-tillatelse for samme bruker aktiveres igjen for samme kort i samme godkjenning. En eksisterende journal kan avklares med den nye hjelpen hvis de strenge diskformatene kan leses, også når det kjørende programmet er inkompatibelt. NVML-hjelperen bruker fortsatt protokoll 2 med `strictRequests: true`, og fra 1.6.0 bygg 32 eller nyere (sikkerhetsrettinger i hjelperen); RGB-grensen gjør den ikke utdatert.

Aktiveringen legger inn den root-installerte enhetsmalen og starter `agentic-rigg-rgb.service`. Et flyktig tillegg under `/run/systemd/system/agentic-rigg-rgb.service.d/50-agentic-rigg-card.conf` forsvinner ved omstart. Før start fjernes det gamle `card.conf` fra både `/etc` og `/run`. Også forberedelse, avklaring og gjenoppretting bestemmer bussen først med den lesende `--rgb-adapter`. De kjøres i en transient `systemd-run --wait --pipe`-tjeneste med arbeiderens herding, `ProtectProc=invisible`, lukket enhetspolicy og bare den identifiserte I2C-noden. Under forberedelse dekker en privat bindmontering hele `/etc/agentic-rigg` i sandkassen. Prosessen kan ikke endre den virkelige NVML-tillatelsen. Administratorinngangen kontrollerer den nye RGB-tillatelsen mot svaret og kortbindingen før atomisk installasjon. Tillegget nullstiller tidligere `DeviceAllow` og gir bare tilgang til `/dev/i2c-N rw` fra den ferske kortbundne adapterrapporten. Tjenesten bruker blant annet `DevicePolicy=closed`, skrivebeskyttet system, privat tilstandsmappe og nettverkssperre. Ved bytte av bussnummer må en eksplisitt aktivering hente ny adaptermetadata. Tjenesten har `Restart=no` og aktiveres ikke for oppstart med `enable`.

Arbeiderkommandoen er `agentic-rigg-helper --rgb-server`. Den krever root, egen RGB-tillatelse og et allerede lagret `user_confirmed_static`-grunnopptak. Ingen I2C åpnes ved oppstart. Et avbrudd mellom lagring av tillatelsen, installasjon av enheten og oppstart kan følges av ny aktivering med samme kort og utgangspunkt.

## Sjekkliste for fysisk kontroll ved installasjon

1. Kontroller utgavens SHA-256 fra utgivelsessiden i en separat kanal. Kjør bootstraplinjen ovenfor med arkivet og denne summen. Kontroller at `/usr/local/libexec/agentic-rigg-system/revision` oppgir `2:` og forventet bygg. Installasjonen skal ikke aktivere noen tilgang.
2. Velg ett kort og kontroller PCI, UUID og synlig statisk lys før du godkjenner RGB-aktivering. Forbered en annen terminal før godkjenning: kjør `for i in $(seq 1 600); do s=$(systemctl show agentic-rigg-rgb-admin.service -p LoadState -p DevicePolicy -p DeviceAllow -p CapabilityBoundingSet -p ProtectProc); if grep -qx 'LoadState=loaded' <<<"$s"; then printf '%s\n' "$s"; break; fi; sleep 0.05; done` i Bash. Tjenesten samles inn etter handlingen. Uteblitt måling er **ikke verifisert**; ikke gjenta en maskinvareskriving bare for å måle.
3. Krev `DevicePolicy=closed`, kun den kortbundne `/dev/i2c-N rw`, `CAP_CHOWN` og `ProtectProc=invisible`. Sammenlign med den startede arbeideren: `systemctl show agentic-rigg-rgb.service -p DevicePolicy -p DeviceAllow -p CapabilityBoundingSet -p ProtectProc`. Ved en nødvendig NVML-gjenoppretting kan samme lesende kontroll brukes på `agentic-rigg-nvml-restore.service`: bare `/dev/nvidiactl` og den bundne `/dev/nvidiaN`, aldri UVM/I2C. Ved feil, noter systemd-versjon, egenskap og feilmelding. Ikke kjør hjelperen uten sandkassen.
4. Bare med egen uttrykkelig godkjenning: prøv én fast farge og gjenopprett det lagrede lyset. Bekreft resultatet fysisk før du kvitterer i appen. Ved avvik: stopp prøven, behold journalen og ikke godkjenn gjenoppretting. Deaktiver etter vellykket kontroll.

## Filer og identitet

| Sti | Eier og modus | Formål |
|---|---|---|
| `/etc/agentic-rigg/rgb.json` | root, 0600 | Separat tillatelse, bruker-UID og full kortbinding |
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

Før skriving lagres full journal i en ny privat tempfil, filen synkroniseres, byttes atomisk og foreldremappen synkroniseres. Originalen og tidligere eget skriveansvar bevares. Fasen `prepared` alene beviser verken et setterforsøk eller urørt kort. Etter retur registreres `attempted` eller `rejected_before_write`. Enhver eksisterende journal gir `RecoveryRequired` ved ny serverstart, uten automatisk fargeendring. Hvis omdøpingen ble synlig før synkronisering feilet, vises journalen som uavklart i gjeldende arbeider også. En slik filfeil starter ingen automatisk setter. I en ren økt kan en kortvarig lesefeil prøves på nytt med «Sjekk igjen». Gjenoppretting og deaktivering er tilgjengelige fra panelet.

Avvisning før første eget skrivekall gir ingen første skriving ved klienttap eller nedstenging. Mulig delvis endring beholder gjenopprettingsansvaret. Recovery journalføres før forsøket og velger bare forrige bekreftede valg eller originalen, med samme kortbinding.

Bekreftelsesfristen og den separate 15-sekunders leasen bruker `CLOCK_BOOTTIME`, inkludert hviletid. Vellykket apply/Behold, egen heartbeat og godkjent visuell ACK av et fortsatt eid, ikkeopprinnelig valg kan fornye leasen. Status, feil og andre klienter gjør ikke det. Original-ACK fjerner eierskapet.

Det gjøres ett gjenopprettingsforsøk **per hendelse**: pending-frist bruker forrige valg; leaseutløp/klienttap bruker originalen. Hendelsene kan gi to forsøk, også til samme farge. Ingen ubegrenset timerløkke startes. Samtidige frister prioriterer leasefrigivelsen.

Serveren har høyst fire klienter og avgrensede rammer/byte per runde. Stopp sjekkes mellom RPC-er; kødata fra lukket forbindelse avvises. Socket-I/O er ikke-blokkerende. I2C-kall kan fortsatt blokkere RGB-prosessen; derfor deles den ikke med NVML-viftekurven eller nødregelen.

## Menneskelig bekreftelse og avsluttet arbeider

Samsvarende fargefelt med ukjent modus gir `recovery_needs_confirmation`. Dialogen viser farge og lysstyrke eller slukket lys, samt en tom avkryssingsboks. Den fanger målet og `recoveryId` når den åpnes. Hjelperen utsteder en ny identitet for hvert forsøk, også for samme mål, og en ny arbeider får et nytt tilfeldig prefiks. ACK behandler utløpte frister før den sammenligner identitet og mål. Sammenligningen kan aldri velge et nytt skrivemål.

ACK leser de bundne fargefeltene på nytt og lagrer `userVisual`-kvittering med ukjent modus. For et annet tidligere bekreftet valg beholdes journal, ansvar og lease. For originalen synkroniseres kvitteringen før journalen ryddes. En filsystemfeil rapporteres også når en omdøping eller sletting allerede er synlig. Arbeideren validerer eldre kvitteringer før neste journalførte skriverunde. En korrupt kvittering med trygg eier, filtype og modus kan erstattes ved gyldig avklaring av en eksisterende journal, eller arkiveres ved ny aktivering etter ren deaktivering.

### Stoppet eller inkompatibel arbeider

Ved inkompatibel arbeider kan «Gjenopprett lagret lys» stoppe tjenesten og utføre ett eksplisitt forsøk dersom journalen fortsatt finnes. «Avklar tidligere økt» henter først nøyaktig mål og journalens forsøksidentitet via den faste polkit-inngangen. Etter at brukeren har sett lyset, bruker appen samme inngang til ny feltavlesing og avklaring. Arbeideren holdes stoppet gjennom den administrative avklaringen. Et nytt gjenopprettingsforsøk journalfører en ny identitet før en eventuell skriving.

De samme operasjonene er tilgjengelige fra en terminal med det aktuelle kortets fullstendige PCI og GPU-UUID. Bruk identiteten fra `--inspect` først etter at du har kontrollert det oppgitte lyset på kortet.

| Handling | Kommando og oppførsel |
|---|---|
| Gjenopprett ved behov | `pkexec /usr/local/libexec/agentic-rigg-rgb-v2/rgb-gjenopprett.sh --pci PCI --uuid UUID --restore-original` stopper arbeideren og forsøker originalen ved uavklart journal. |
| Hent mål for bekreftelse | `pkexec /usr/local/libexec/agentic-rigg-rgb-v2/rgb-avklar.sh --pci PCI --uuid UUID --inspect` oppgir målet og forsøksidentiteten. |
| Bekreft etter at du har sett lyset | `pkexec /usr/local/libexec/agentic-rigg-rgb-v2/rgb-avklar.sh --pci PCI --uuid UUID --static-confirmed --recovery-id ID` gjør ny feltavlesing og rydder journalen ved samsvar. |
| Slå av tilgangen | `pkexec /usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh --pci PCI --uuid UUID` stopper en eventuell arbeider og fjerner enhetsfiler og tillatelsen, med journalvern og eksklusiv lås. |

Deaktivering kan gjentas ved manglende enhet eller avbrutt opprydding. Hvis stopp feiler og systemd fortsatt melder aktiv enhet eller jobb, beholdes tillatelsen. Tillatelsen fjernes etter resten av oppryddingen. Ved opprydding av gamle rester uten tillatelsesfil kan en administrator fjerne de faste RGB-restene dersom journalen er ren og ingen arbeider holder låsen. Den betrodde systeminstallasjonen i v2-mappen og policyen blir liggende slik at samme innganger kan brukes på nytt. Lagrede grunnopptak og kvitteringer beholdes.

En eksplisitt ny aktivering etter ren deaktivering kan lagre en endret fast farge. Det forrige grunnopptaket og eventuell kvittering arkiveres før det nye kan tas i bruk. Under en eksisterende tillatelse kreves fortsatt samme utgangspunkt.

`uninstall.sh` stanser før appen og daemonen fjernes hvis den finner aktiv eller uavklart RGB-tilstand. Fullfør gjenoppretting og deaktivering, og kjør avinstallering på nytt. Deaktivering kan registrere en ren, eldre eller delvis installert tilstand også når arbeideren allerede er stoppet.

## Byttet eller fjernet kort og full avinstallasjon

En stoppet RGB-tjeneste med ren journal kan deaktiveres av eier uten kortargumenter: `pkexec /usr/local/libexec/agentic-rigg-rgb-v2/rgb-deaktiver.sh`. Ved feil viser skriptet den lagrede bindingen. For NVML bruker `pkexec /usr/local/libexec/agentic-rigg/deaktiver.sh` alltid eierens lagrede binding. Normal deaktivering bevarer opptak og krever avklart tilstand.

Bare når det opprinnelige kortet faktisk er fjernet eller ikke lenger kan nås med sin binding: stopp riktig tjeneste (`sudo systemctl stop agentic-rigg-rgb.service` eller `sudo systemctl stop agentic-rigg-helper.service`). Kjør `sudo /usr/local/libexec/agentic-rigg-system/system-install --archive-removed-card rgb UUID` (eller `nvml UUID`), med **bundet** GPU-UUID. Handlingen er en eksplisitt administratorerklæring om bortfall, ikke en polkit-handling i appen. Den krever stoppet tjeneste uten ventende jobb, tar RGB-arbeiderlåsen, tilbakekaller tillatelsen og arkiverer tilstand og uavklart journal sammen under `/var/lib/agentic-rigg-arkiv/` i en privat mappe. Den skriver ikke til det nye kortet og erklærer aldri den gamle endringen gjenopprettet. Hvis UUID er ukjent, bruk den fra feilmeldingen eller la administrator lese den fra den beskyttede tillatelsen. Hvis oppgradering av en eldre installasjon stanses av en journal, har den verifiserte systeminstallereren først lagt inn den root-eide arkivkommandoen.

Dersom brukeren som aktiverte tilgangen ikke lenger finnes: stopp domenets tjeneste. Som administrator, kjør `sudo /usr/local/libexec/agentic-rigg-system/system-install --revoke nvml` eller `sudo /usr/local/libexec/agentic-rigg-system/system-install --revoke rgb`. Bare en stoppet tjeneste uten ventende jobb og uten `dirty.json`/`journal.json` kan tilbakekalles slik. Grunnopptak beholdes på plass. Kommandoen gir ingen ny tillatelse og er ikke en snarvei rundt gjenoppretting.

Etter deaktivering av begge domener: `sudo /usr/local/libexec/agentic-rigg-system/system-install --uninstall`, deretter `./uninstall.sh` som vanlig bruker. Faste root-innganger, policyer, enheter og gamle enkeltfiler fjernes. Tidligere grunnopptak og kvitteringer flyttes til privat arkiv; brukerinnstillinger og historikk beholdes. Den tomme samordningslåsen under `/run` kan ligge til omstart, slik at ventende operasjoner ikke låser forskjellige inoder. Kjøres brukeravinstallasjonen først, gir inaktive systemrester en tydelig beskjed og kommandoen ovenfor, men returkode 0 hvis brukerappen ble fjernet som ønsket. Aktiv tilgang sperrer fortsatt.

## Tidligere verifikasjon og fysisk prøve

Fase 4 er kontrollert med ren workspace-clippy og **536 beståtte tester, 4 ignorerte, ingen feil**. Produksjonsbinærene er bygget og oppgir **1.4.1 · bygg 24**. Privat GTK-QA ga tolv PNG-er: sju merkede visningstilstander og fem bilder fra vanlig GTK/daemon-flyt mot en eksplisitt merket fake-arbeider. Faktiske størrelser var 1920×1048, 1440×1000 og 776×900; forespurt bredde 640 px ble begrenset til 776 px. **640 px og mindre bredder er ikke verifisert.** Funksjonsprøven dekker redigering uten mutasjon, Bruk, Behold, Reset, avvist gammel ACK, aktiv checkbox-ACK, Av og Frigi. Fake-spor er ikke fysisk lysbevis.

Kontrollene bruker private røtter, simulerte kort og falske svar; administratorhandlinger er sperret i `rgb-flow`.

Den 20. november 2026 identifiserte den lesende proben én PALIT 100-sone på NVIDIA-port 1, adresse `0x49`. Brukeren bekreftet fast grønt (`#45C800`, 100 %) før aktivering med polkit. Produksjonshjelperen leste tilbake blått (`#0000FF`, 100 %) etter Bruk og den opprinnelige grønnfargen etter ti sekunders frist. Brukeren så blått og deretter grønt igjen. Hjelperen leste feltene på nytt, lagret bekreftelsen som `userVisual` og ryddet journalen. RGB-testtilgangen ble deaktivert. Prøven gikk gjennom hjelpersocketen. GTK-betjening er prøvd i simulering. Av-modus og endring av lysstyrke på kortet gjenstår.
