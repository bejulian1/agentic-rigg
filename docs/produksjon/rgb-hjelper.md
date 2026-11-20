# Separat RGB-hjelper · implementert på featurebranchen

RGB har et eget domene i hjelperbinæren, en separat systemd-prosess, daemonklient og kontroller på Lys-siden. Det deler ikke motor, socket, tillatelse, grunnopptak eller lease med NVML-styringen. Aktivering, menneskelig bekreftelse og manuell gjenoppretting er implementert og prøvd med isolerte fakes. På brukerens Palit RTX 2080 Super JetStream er fast blått og automatisk tilbakeføring til grønt fysisk bekreftet ved 100 % lysstyrke.

## Betrodd installasjon og administratoraktivering

Desktop-appen kan installeres i `~/.local`. RGB-aktivering bruker faste programmer i `/usr/local/libexec/agentic-rigg-rgb-v2/`. GTK starter `/usr/bin/pkexec` med den samme stien som står i RGB-handlingens `org.freedesktop.policykit.exec.path`. De fire handlingene krever `auth_admin` i en aktiv lokal økt. De tar kortets PCI/UUID og handlingsspesifikke bekreftelser. Bruker-UID kommer fra polkit. Inngangen godtar ingen pakkesti, testrot, vilkårlig binær, buss eller klientvalgt skriveadresse.

En administrator som allerede har en betrodd administratorterminal, må installere RGB-komponentene én gang per oppgradering av dette systemoppsettet. Bruk et arkiv som er kontrollert mot en sjekksum hentet separat fra en betrodd publisering, eller et bygg som administratoren selv har godkjent fra en gjennomgått kilde. Oppbevar de nøyaktige, verifiserte bytene i en root-eid mappe uten andre brukeres skrivetilgang fra verifikasjon til installasjon. Kommandoene under kjøres fra den beskyttede, utpakkede pakkemappen. Kontrollsummer ved siden av et arkiv i en brukers skrivbare mappe er utilstrekkelige som tillitsgrunnlag.

```sh
rgb_dir=/usr/local/libexec/agentic-rigg-rgb-v2
install -d -o root -g root -m 0755 "$rgb_dir"
install -o root -g root -m 0755 bin/agentic-rigg-helper "$rgb_dir/agentic-rigg-helper"
install -o root -g root -m 0755 \
  data/hjelper/rgb-aktiver.sh data/hjelper/rgb-avklar.sh \
  data/hjelper/rgb-gjenopprett.sh data/hjelper/rgb-deaktiver.sh "$rgb_dir/"
install -o root -g root -m 0644 \
  data/hjelper/rgb-admin.py data/hjelper/agentic-rigg-rgb.service "$rgb_dir/"
install -o root -g root -m 0644 data/hjelper/io.github.AgenticRigg.rgb.policy \
  /usr/share/polkit-1/actions/io.github.AgenticRigg.rgb.v2.policy
```

Valg av pakke og installasjonen ovenfor er administratorens godkjenning av kode som skal kunne kjøre som root. Det forutsetter tillit til pakkekilden, verifikasjonen, systemets polkit, Python, systemd, root-eide biblioteker og administratorer. Senere polkit-dialoger gjelder bare den navngitte kortoperasjonen. Et kompromittert program i hjemmemappen kan be om en handling eller gi misvisende tekst. Root-koden validerer argumenter, bruker og fersk kortbinding. Administratoren må kjenne igjen den forventede polkit-handlingen og selv se på det fysiske lyset. Ved senere root-oppdatering bør kortets tilgang deaktiveres og journalen avklares først, slik at enheten, Python-koden, hjelpen og policyen blir oppdatert samlet.

«Aktiver kortlys» krever at brukeren ser på kortet og bekrefter én fast farge før polkit. Ved ukjent eller bevegelig effekt må aktiveringen avbrytes. Den betrodde hjelperen identifiserer kontrolleren, kontrollerer adaptermetadata før og etter lesing og lagrer et kortbundet, statisk grunnopptak før tillatelsen. En journal sperrer ny aktivering. Aktiveringen sender ingen fargekommando.

RGB bruker protokoll **2**, domenet **palit_rgb** og laveste kompatible RGB-bygg **24**. Protokoll 1 må avklares og deaktiveres via de nye, faste administratorinngangene før ny RGB-aktivering. En eksisterende journal kan avklares med den nye hjelpen hvis de strenge diskformatene kan leses, også når det kjørende programmet er inkompatibelt. NVML-hjelperen bruker fortsatt protokoll 2 og minimumsbygg 23.

Aktiveringen legger inn den root-installerte enhetsmalen og starter `agentic-rigg-rgb.service`. Et tillegg nullstiller tidligere `DeviceAllow` og gir bare tilgang til `/dev/i2c-N rw` fra den ferske kortbundne adapterrapporten. Tjenesten bruker blant annet `DevicePolicy=closed`, skrivebeskyttet system, privat tilstandsmappe og nettverkssperre. Ved bytte av bussnummer må en eksplisitt aktivering hente ny adaptermetadata. Tjenesten har `Restart=no` og aktiveres ikke for oppstart med `enable`.

Arbeiderkommandoen er `agentic-rigg-helper --rgb-server`. Den krever root, egen RGB-tillatelse og et allerede lagret `user_confirmed_static`-grunnopptak. Ingen I2C åpnes ved oppstart. Et avbrudd mellom lagring av tillatelsen, installasjon av enheten og oppstart kan følges av ny aktivering med samme kort og utgangspunkt.

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

«Lys på kortet» styrer én fysisk sone. Hjelperens statusfelt `softwareEffects` angir hvilke programvaredrevne effekter panelet kan tilby. Redigering av farge og lysstyrke sender ingen endring; bare «Bruk på kortet» gjør det. Behold/Rull tilbake bruker RGBs egen bekreftelse, uten NVML-lås eller GPU-bekreftelseslinje. Den lokale lysmodellen, de illustrerte sonene og «Lagre i appen» er atskilt fra fysisk styring. Profilvalg, oppstart og forhåndsvisning skriver ikke RGB.

Daemonens `get_rgb_helper` gir `state`, `message`, `status`, `owned`, `busy` og `systemAdmin`. Bare vanlig produksjonsoppsett med NVML og vertens rot kan bruke vertens RGB-hjelper. Simulering og utvikling trenger en uttrykkelig valgt, kanonisk, privat `0700`-rot eid av daemonbrukeren. Uten den er RGB slått av i daemonen. `owned` gjelder denne UI-forbindelsen, ikke arbeiderens interne forbindelsesnummer. Uten `refresh` brukes cache; oppdatering og mutasjoner går på en egen IPC-tråd. Køen er begrenset, totalfristen er fire sekunder fra innlegging, og hver IPC-runde har høyst tre sekunder innenfor totalfristen. `CLOCK_BOOTTIME` kontrollerer også alder etter suspend: for gamle jobber sendes ikke, og foreldede svar tas ikke som nytt suksessbevis. Et tidsavbrudd beviser ikke at et allerede sendt maskinvarekall var uten virkning.

Mens UI-forbindelsen eier RGB, sender daemonen heartbeat hvert annet sekund. Feilende mutasjoner får ikke skyve denne takten fremover. UI-et sender ingen heartbeat og avstemmer bare status på Lys-siden: omtrent 500 ms ved eid/ventende endring, ellers to sekunder. Sidebytte stanser UI-pollingen, mens en beholdt farge fortsatt eies av det åpne vinduet. «Avslutt lysstyring» frigir eksplisitt; tap av UI-forbindelsen avslutter daemonens RGB-forbindelse.

## Arbeiderens RPC

Meldinger følger NDJSON med `id`, `op` og `args`. Ukjente felter avvises. `gpu` og `uuid` må være identiske med root-tillatelsens kort.

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

## Tidligere verifikasjon og fysisk prøve

Fase 4 er kontrollert med ren workspace-clippy og **536 beståtte tester, 4 ignorerte, ingen feil**. Produksjonsbinærene er bygget og oppgir **1.4.1 · bygg 24**. Privat GTK-QA ga tolv PNG-er: sju merkede visningstilstander og fem bilder fra vanlig GTK/daemon-flyt mot en eksplisitt merket fake-arbeider. Faktiske størrelser var 1920×1048, 1440×1000 og 776×900; forespurt bredde 640 px ble begrenset til 776 px. **640 px og mindre bredder er ikke verifisert.** Funksjonsprøven dekker redigering uten mutasjon, Bruk, Behold, Reset, avvist gammel ACK, aktiv checkbox-ACK, Av og Frigi. Fake-spor er ikke fysisk lysbevis.

Kontrollene bruker private røtter, simulerte kort og falske svar; administratorhandlinger er sperret i `rgb-flow`.

Den 20. november 2026 identifiserte den lesende proben én PALIT 100-sone på NVIDIA-port 1, adresse `0x49`. Brukeren bekreftet fast grønt (`#45C800`, 100 %) før aktivering med polkit. Produksjonshjelperen leste tilbake blått (`#0000FF`, 100 %) etter Bruk og den opprinnelige grønnfargen etter ti sekunders frist. Brukeren så blått og deretter grønt igjen. Hjelperen leste feltene på nytt, lagret bekreftelsen som `userVisual` og ryddet journalen. RGB-testtilgangen ble deaktivert. Prøven gikk gjennom hjelpersocketen. GTK-betjening er prøvd i simulering. Av-modus og endring av lysstyrke på kortet gjenstår.
