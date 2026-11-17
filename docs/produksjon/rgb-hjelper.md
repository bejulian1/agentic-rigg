# Separat RGB-hjelper · implementert på featurebranchen

RGB har et eget domene i hjelperbinæren, en separat systemd-prosess, daemonklient og kontroller på Lys-siden. Det deler ikke motor, socket, tillatelse, grunnopptak eller lease med NVML-styringen. Aktivering, menneskelig bekreftelse og manuell gjenoppretting er implementert og prøvd med isolerte fakes. **Kontrollersignatur, synlig farge og tilbakeføring på brukerens faktiske kort er fortsatt uverifisert.** Ingen RGB-tjeneste er installert eller aktivert på brukerens PC.

## Administratoraktivering og oppstart

«Aktiver kortlys» krever at brukeren ser på kortet og aktivt bekrefter at det allerede viser én fast farge, før appen starter polkit. Ukjent eller bevegelig effekt skal avbrytes. Det finnes **ingen implementert reservevei som erstatter en ukjent opprinnelig effekt med et nytt statisk utgangspunkt**.

`rgb-aktiver.sh --pci PCI --uuid UUID --static-confirmed` bruker RGBs egne root-aktiva. Skriptet krever full GPU-UUID, gyldig PCI-grafikkfunksjon, brukeren fra `PKEXEC_UID`, entydige argumenter og trygge filer. Pakkeinnholdet leses inn før en root-eid kopi av hjelperen kjøres. Hjelperens `--rgb-forbered` identifiserer kontrolleren, kontrollerer metadata før/etter lesingen og lagrer en kortbundet, brukerbekreftet statisk baseline før tillatelsen. Eksisterende original kan ikke overskrives; en journal sperrer ny aktivering. Aktiveringen sender ingen fargekommando.

Deretter installeres `/usr/local/libexec/agentic-rigg-rgb-helper`, egne skript under `/usr/local/libexec/agentic-rigg-rgb/`, RGB-polkit-policy og `agentic-rigg-rgb.service`. Enheten bruker `DevicePolicy=closed`. Drop-in-filen nullstiller tidligere `DeviceAllow` og legger til **bare `/dev/i2c-N rw` fra hjelperens ferske, kortbundne adapterrapport**. Ingen klientvalgt buss, `char-i2c` eller jokertegn tillates. Enheten har blant annet skrivebeskyttet system, privat tilstandsmappe, nettverkssperre og avgrensede systemkall.

Tjenesten har `Restart=no`, ingen `ExecStopPost` og aktiveres ikke for boot med `enable`. Ved endret bussnummer må en eksplisitt administratorflyt hente ny adaptermetadata og skrive den smale tillatelsen på nytt; vanlig appstart gjør ikke dette. Systemd-enhetens fysiske I2C-tilgang er ennå ikke prøvd.

    agentic-rigg-helper --rgb-server

Serverkommandoen aksepterer ingen andre argumenter. Den krever root, egen RGB-tillatelse og et allerede lagret `user_confirmed_static`-grunnopptak. Den oppretter ikke grunnopptak og åpner ikke I2C ved oppstart. Vanlig GPU-start starter ikke RGB, og eksisterende GPU-autorisasjon gir ingen RGB-tilgang.

RGB-protokollen er **1**, domenet **palit_rgb**, og laveste kompatible RGB-bygg er **24**. Ukjent nyere bygg avvises. NVML-hjelperens protokoll 2 og minimumsbygg 23 endres ikke.

## Filer og identitet

| Sti | Eier og modus | Formål |
|---|---|---|
| `/etc/agentic-rigg/rgb.json` | root, 0600 | Separat tillatelse, bruker-UID og full kortbinding |
| `/var/lib/agentic-rigg-rgb/` | root, 0700 | Privat tilstandsmappe |
| `baseline.json` i tilstandsmappen | root, 0600 | Uforanderlig, brukerbekreftet statisk utgangspunkt |
| `journal.json` i tilstandsmappen | root, 0600 | Skrivehensikt, tidligere valg og gjenopprettingsansvar |
| `visual-confirmation.json` i tilstandsmappen | root, 0600 | Siste varige brukerkvittering med mål og ferske felt; ingen påstand om avlest modus |
| `worker.lock` i tilstandsmappen | root, 0600 | Eksklusiv lås for arbeider eller administrativ avklaring |
| `/run/agentic-rigg-rgb/` | root, 0755 | Socketmappe uten andres skrivetilgang |
| `helper.sock` i socketmappen | autorisert bruker, 0600 | Separat NDJSON-forbindelse |

Produksjonsstiene er faste. Lageret åpner hvert stilledd uten symlink-følging og kontrollerer type, eier, modus, hardlenker og størrelse. Arbeideren holder låsen og revaliderer mappe, lås, original og forventet journal. Tester velger private mapper og falsk maskinvare, uten fallback til vertens GPU- eller RGB-hjelper.

Socketklienten må ha tillatelsens faktiske `SO_PEERCRED`-UID. Tillatelsen lastes på nytt før RPC-er og periodisk; feil, bortfall eller endring tilbakekaller domenet og stopper arbeideren etter livssyklusfrigivelse. En ny tillatelse bytter ikke kort eller bruker i en kjørende arbeider.

## App og daemon

«Lys på kortet» har én fysisk sone med fast farge eller av. Redigering av farge og lysstyrke sender ingen endring; bare «Bruk på kortet» gjør det. Behold/Rull tilbake bruker RGBs egen bekreftelse, uten NVML-lås eller GPU-bekreftelseslinje. Den lokale lysmodellen, de illustrerte sonene og «Lagre i appen» er atskilt fra fysisk styring. Profilvalg, oppstart og forhåndsvisning skriver ikke RGB.

Daemonens `get_rgb_helper` gir `state`, `message`, `status`, `owned` og `busy`. `owned` gjelder denne UI-forbindelsen, ikke arbeiderens interne forbindelsesnummer. Uten `refresh` brukes cache; oppdatering og mutasjoner går på en egen IPC-tråd. Køen er begrenset, totalfristen er fire sekunder fra innlegging, og hver IPC-runde har høyst tre sekunder innenfor totalfristen. `CLOCK_BOOTTIME` kontrollerer også alder etter suspend: for gamle jobber sendes ikke, og foreldede svar tas ikke som nytt suksessbevis. Et tidsavbrudd beviser ikke at et allerede sendt maskinvarekall var uten virkning.

Mens UI-forbindelsen eier RGB, sender daemonen heartbeat hvert annet sekund. Feilende mutasjoner får ikke skyve denne takten fremover. UI-et sender ingen heartbeat og avstemmer bare status på Lys-siden: omtrent 500 ms ved eid/ventende endring, ellers to sekunder. Sidebytte stanser UI-pollingen, mens en beholdt farge fortsatt eies av det åpne vinduet. «Avslutt lysstyring» frigir eksplisitt; tap av UI-forbindelsen avslutter daemonens RGB-forbindelse.

## Arbeiderens RPC

Meldinger følger NDJSON med `id`, `op` og `args`. Ukjente felter avvises. `gpu` og `uuid` må være identiske med root-tillatelsens kort.

| Operasjon | Argumenter | Oppførsel |
|---|---|---|
| `rgb_status` | tomt objekt | Ren cache, ingen I2C og ingen leasefornyelse |
| `rgb_apply` | gpu, uuid, request, confirmTimeoutS | Fast farge/av; 10–60 sekunders bekreftelsesfrist, 15 sekunder fra UI |
| `rgb_confirm` | gpu, uuid, txn | Behold med fersk tilbakelesing |
| `rgb_revert` | gpu, uuid, txn | Gjenopprett forrige bekreftede appvalg |
| `rgb_reset` | gpu, uuid | Eksplisitt gjenoppretting til lagret original, ikke fabrikkreset |
| `rgb_release` | gpu, uuid | Frigi egen forbindelse og gjenopprett egne endringer |
| `rgb_heartbeat` | gpu, uuid | Forny bare denne forbindelsens egen RGB-lease; daemonoperasjon |
| `rgb_ack_recovery` | gpu, uuid, confirmedStatic=true, expected | Lagre eksplisitt menneskelig bekreftelse etter ny feltlesing og kontroll av forventet mål |

`request` og ACK-feltet `expected` har samme typede format:

    {"mode":"static","color":[R,G,B],"brightness":80}
    {"mode":"off"}

Ingen buss, enhetssti, registeradresse eller EEPROM-valg mottas fra klienten. `expected` er bare en sammenligningsbetingelse, aldri et nytt skrivemål. Statusens `observationSource` er `stored_baseline`, `operation_readback` eller `unknown`. Lagret farge er ikke en fersk måling. `observed.mode` forblir `null`, også etter vellykket apply, Behold og menneskelig bekreftelse.

## Journal, frister og avbrudd

Før skriving lagres full journal i en ny privat tempfil, filen synkroniseres, byttes atomisk og foreldremappen synkroniseres. Originalen og tidligere eget skriveansvar bevares. Fasen `prepared` alene beviser verken et setterforsøk eller urørt kort. Etter retur registreres `attempted` eller `rejected_before_write`. Enhver eksisterende journal gir `RecoveryRequired` ved ny serverstart, uten automatisk fargeendring.

Avvisning før første eget skrivekall gir ingen første skriving ved klienttap eller nedstenging. Mulig delvis endring beholder gjenopprettingsansvaret. Recovery journalføres før forsøket og velger bare forrige bekreftede valg eller originalen, med samme kortbinding.

Bekreftelsesfristen og den separate 15-sekunders leasen bruker `CLOCK_BOOTTIME`, inkludert hviletid. Vellykket apply/Behold, egen heartbeat og godkjent visuell ACK av et fortsatt eid, ikkeopprinnelig valg kan fornye leasen. Status, feil og andre klienter gjør ikke det. Original-ACK fjerner eierskapet.

Det gjøres ett gjenopprettingsforsøk **per hendelse**: pending-frist bruker forrige valg; leaseutløp/klienttap bruker originalen. Hendelsene kan gi to forsøk, også til samme farge. Ingen ubegrenset timerløkke startes. Samtidige frister prioriterer leasefrigivelsen.

Serveren har høyst fire klienter og avgrensede rammer/byte per runde. Stopp sjekkes mellom RPC-er; kødata fra lukket forbindelse avvises. Socket-I/O er ikke-blokkerende. I2C-kall kan fortsatt blokkere RGB-prosessen; derfor deles den ikke med NVML-viftekurven eller nødregelen.

## Menneskelig bekreftelse og avsluttet arbeider

Samsvarende fargefelt med ukjent modus gir `recovery_needs_confirmation`. Appen viser målet fra `state.confirmed`: konkret farge/lysstyrke eller slukket lys. Checkboxen starter avslått. UI-et fanger målet ved dialogåpning og sender det som `expected`. Arbeideren avviser endret snapshot ved fristbehandling og avviser deretter ethvert `expected` som ikke samsvarer med det nåværende bekreftede målet. En gammel bekreftelse kan derfor ikke avklare en ny original etter leaseutløp.

ACK krever ny full lesing av farge/lysstyrke for samme binding og gjenopprettede mål. Kvitteringen merkes `userVisual`, med modus fortsatt ukjent. For originalen synkroniseres kvittering og mappe før journalen slettes; deretter fjernes dirty-tilstand og eierskap. For et annet tidligere bekreftet valg beholdes journal, ansvar og lease. Ved feil etter vellykket journal-unlink kan filen allerede være borte; runtime rapporterer fortsatt feil og sperrer videre mutasjon. På den visuelle ryddebanen er originalens kvittering da allerede varig.

Når arbeideren ikke kjører, finnes egne administratorhandlinger med samme UID og kortbinding:

| Handling | Faktisk oppførsel |
|---|---|
| `rgb-gjenopprett.sh --pci PCI --uuid UUID --restore-original` | Krever eksisterende journal og gjør ett eksplisitt, journalført skriveforsøk til lagret original. Starter ikke arbeideren, sletter ikke journalen og bekrefter ikke modus. |
| `rgb-avklar.sh --pci PCI --uuid UUID --static-confirmed` | Brukeren bekrefter at originalens faste lys allerede vises. Hjelperen gjør bare fersk identifikasjon/feltlesing, lagrer `userVisual`-kvittering og rydder journalen ved samsvar. Skriptet henter deretter ny adaptermetadata, oppdaterer smalt `DeviceAllow` og starter RGB-arbeideren. |
| `rgb-deaktiver.sh --pci PCI --uuid UUID` | Krever ingen journal før og etter stopp, kontrollerer bruker/binding og tar arbeiderlåsen før RGB-tillatelse, tjeneste og aktiva fjernes. Original og kvitteringer beholdes. NVML-hjelperen berøres ikke. |

Appen viser deaktivering bare ved ren RGB-tilstand. Ingen administratorhandling kjøres ved vinduslukking, profilvalg eller oppstart.

## Verifikasjon og gjenstående fysisk prøve

Fase 4 er kontrollert med ren workspace-clippy og **536 beståtte tester, 4 ignorerte, ingen feil**. Produksjonsbinærene er bygget og oppgir **1.4.1 · bygg 24**. Privat GTK-QA ga tolv PNG-er: sju merkede visningstilstander og fem bilder fra vanlig GTK/daemon-flyt mot en eksplisitt merket fake-arbeider. Faktiske størrelser var 1920×1048, 1440×1000 og 776×900; forespurt bredde 640 px ble begrenset til 776 px. **640 px og mindre bredder er ikke verifisert.** Funksjonsprøven dekker redigering uten mutasjon, Bruk, Behold, Reset, avvist gammel ACK, aktiv checkbox-ACK, Av og Frigi. Fake-spor er ikke fysisk lysbevis.

Kontrollene bruker private røtter, simulerte kort og falske svar; administratorhandlinger er sperret i `rgb-flow`. Ingen root-probe, aktivering, installasjon eller fysisk RGB-prøve er kjørt mens brukeren er borte. Neste fysiske steg krever brukerens tilstedeværelse og ordinær administratorgodkjenning: observer faktisk signatur, bekreft et statisk utgangspunkt, prøv farge/av/lysstyrke kontrollert og dokumenter synlig gjenoppretting. Featurebranchen er fortsatt et Draft PR-arbeid, ikke en fysisk verifisert RGB-utgivelse.
