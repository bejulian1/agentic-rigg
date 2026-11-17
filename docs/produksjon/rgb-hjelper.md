# Separat RGB-hjelper · pågående implementasjon

RGB har et eget domene i hjelperbinæren. Det deler ikke motor, socket, tillatelse, grunnopptak eller lease med eksisterende NVML-styring. Vanlig GPU-start starter ikke RGB. Dette er kode på featurebranchen, uten installert RGB-tjeneste, aktiveringsknapp eller fysisk verifikasjon.

## Oppstart og identitet

    agentic-rigg-helper --rgb-server

Kommandoen aksepterer ingen andre argumenter. Den krever root, egen RGB-tillatelse og et allerede lagret, brukerbekreftet statisk grunnopptak. Den oppretter ikke grunnopptak og åpner ikke I2C under oppstart.

Den separate protokollen er **1**, domenet er **palit_rgb**, og kompatible RGB-tillatelser starter på bygg **24**. Ukjent nyere bygg avvises. NVML-hjelperens protokoll 2 og minimumsbygg 23 endres ikke.

| Sti | Eier og modus | Formål |
|---|---|---|
| /etc/agentic-rigg/rgb.json | root, 0600 | Separat tillatelse, bruker-UID og full kortbinding |
| /var/lib/agentic-rigg-rgb/ | root, 0700 | Privat tilstandsmappe |
| baseline.json i tilstandsmappen | root, 0600 | Uforanderlig, brukerbekreftet statisk utgangspunkt |
| journal.json i tilstandsmappen | root, 0600 | Skrivehensikt, tidligere valg og gjenopprettingsansvar |
| worker.lock i tilstandsmappen | root, 0600 | Eksklusiv lås for én arbeider |
| /run/agentic-rigg-rgb/ | root, 0755 | Socketmappe uten andres skrivetilgang |
| helper.sock i socketmappen | autorisert bruker, 0600 | Separat NDJSON-forbindelse |

Produksjonsstiene er faste. Det private lageret åpner hvert stilledd uten symlink-følging og kontrollerer type, eier, modus, hardlenker og størrelse. Arbeideren holder låsen og revaliderer mappe, lås, grunnopptak og forventet journal. Tester velger private mapper og falsk maskinvare, uten fallback til vertens GPU-hjelper.

Socketklienten må ha tillatelsens faktiske SO_PEERCRED-UID. Tillatelsen lastes på nytt før hver RPC og periodisk; feil, bortfall eller endring tilbakekaller domenet og stopper arbeideren etter livssyklusfrigivelse. En ny tillatelse bytter ikke kort eller bruker i en kjørende arbeider.

## RPC

Meldinger følger eksisterende NDJSON med id, op og args. Argumentene nedenfor er de eneste tillatte feltene. gpu og uuid må være identiske med root-tillatelsens kort.

| Operasjon | Argumenter | Oppførsel |
|---|---|---|
| rgb_status | tomt objekt | Ren cache, ingen I2C og ingen leasefornyelse |
| rgb_apply | gpu, uuid, request, confirmTimeoutS | Statisk farge/av, 10–60 sekunders bekreftelsesfrist |
| rgb_confirm | gpu, uuid, txn | Behold med fersk tilbakelesing |
| rgb_revert | gpu, uuid, txn | Gjenopprett forrige bekreftede appvalg |
| rgb_reset | gpu, uuid | Eksplisitt gjenoppretting til lagret utgangspunkt, ikke fabrikkreset |
| rgb_release | gpu, uuid | Frigi egen forbindelse og gjenopprett egne endringer |
| rgb_heartbeat | gpu, uuid | Forny bare denne forbindelsens egen RGB-lease |

Request er enten statisk farge med tre byte og lysstyrke 0–100, eller av:

    {"mode":"static","color":[R,G,B],"brightness":80}
    {"mode":"off"}

Ingen busser, enhetsstier, registeradresser eller EEPROM-valg mottas fra klienten. Status inneholder separat hjelperidentitet, kortbinding, autorisasjon, lease og transaksjon. Feltet observationSource er stored_baseline, operation_readback eller unknown. Lagret farge er ikke en fersk kortmåling. Ukjent modus forblir ukjent etter et vellykket fargevalg.

## Journal, frister og avbrudd

Før skriving lagres full journal i en ny privat tempfil, filen synkroniseres, byttes atomisk og foreldremappen synkroniseres. Oppdateringer bevarer opprinnelig grunnopptak og tidligere eget skriveansvar. Fase prepared alene beviser verken at en setter ble forsøkt eller at kortet er urørt. Etter retur registreres attempted eller rejected_before_write.

En eksisterende journal i **enhver** fase gir RecoveryRequired ved ny oppstart. Ingen automatisk fargeendring utføres da. Senere eksplisitt avklaring av journalen er ennå ikke implementert.

Avvisning før eget skrivekall gir ingen første skriving ved klienttap/nedstenging. Mulig delvis endring beholder recoveryansvaret. Recovery journalføres før forsøket og kan bare velge forrige bekreftede valg eller opprinnelig grunnopptak, med samme kortbinding.

Bekreftelsesfristen og den separate 15-sekunders leasen bruker monoton CLOCK_BOOTTIME, inkludert hviletid. Bare vellykket apply/Behold eller egen RGB-heartbeat fornyer leasen. GPU-status, RGB-status, feil og andre klienter gjør ikke det.

Det gjøres ett gjenopprettingsforsøk **per hendelse**: pending-frist bruker forrige valg; leaseutløp/klienttap bruker opprinnelig utgangspunkt. Hendelsene kan derfor gi to forsøk, også til samme farge. Ingen ubegrenset timerløkke startes. Samtidige frister prioriterer leasefrigivelsen.

Samsvarende fargefelt med ukjent modus gir recovery_needs_confirmation. Journalen ryddes ikke som fullt verifisert. Dersom unlink lykkes og mappesynkronisering feiler på en fullt verifisert ryddebane, kan filen allerede være fjernet; runtime rapporterer fortsatt feil og sperrer videre mutasjon. Dagens fysiske Palit-transport kan ikke nå denne banen som fullt verifisert.

Serveren har høyst fire klienter og avgrensede rammer/byte per runde. Stopp sjekkes mellom RPC-er; kødata fra lukket forbindelse avvises. Socket-I/O er ikke-blokkerende. I2C-kall kan fortsatt blokkere RGB-prosessen; derfor deles den ikke med NVML-viftekurven eller nødregelen.

## Gjenstående produktarbeid

Aktivering/installasjon, systemd-enhet med smalt og ferskt DeviceAllow, daemonklient, appkontroller og eksplisitt menneskelig bekreftelse av uavklart gjenoppretting gjenstår. Ingen av funksjonene kjøres av den installerte appen. Kontrollersignatur, farge/av/lysstyrke og synlig tilbakeføring på brukerens kort er fortsatt uverifisert.
