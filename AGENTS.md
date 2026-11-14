# Prosjektinstrukser for Agentic Rigg

Les `docs/produksjon/REGLER.md` før arbeid på appen. Gjeldende brukerbeskjeder har forrang. Bevar lokale endringer og brukerens innstillinger/historikk.

## Fast utgivelsesrutine

Brukeren ga 14. november 2026 en stående instruks: **Alle nye, ferdige appversjoner skal committes og pushes til `bejulian1/agentic-rigg`, med oppdatert versjon og en tilhørende GitHub Release.** Gjennomfør dette som del av leveransen, uten å spørre på nytt for hver versjon, med mindre brukeren senere endrer ønsket eller en konkret hindring krever avklaring.

1. Oppdater appversjon og øk byggnummeret i `app/version.json`. Hold workspace-versjon i `app/Cargo.toml`, `Cargo.lock` og aktuelle README-/statusfiler i samsvar. Rene dokumentasjonsendringer trenger ingen ny appversjon eller Release.
2. Oppdater `CHANGELOG.md` og `app/PATCHLOG.md` med faktiske endringer og begrensninger. Lag versjonerte utgivelsesnotater i `.github/release-notes/vVERSJON.md`.
3. Kjør relevante kontroller og påkrevde prosjektkontroller. Bygg produksjonsbinærene i den felles byggebufferen. Verifiser at alle tre binærene oppgir riktig versjon og bygg.
4. Kontroller filutvalget, commit endringene og push til repoet. Bevar eksisterende historikk; samordne eventuelle eksterne endringer før push. Bruk eksplisitt filutvalg fremfor å ta med alt i arbeidsmappen.
5. Lag nedlastingspakken med `app/scripts/pakk-utgivelse.py`, riktig `--bin-dir`, `--out-dir` og full kildecommit i `--commit`. Kontroller arkiv, sjekksummer og installasjon i en isolert mappe.
6. Opprett Release med tagg `vVERSJON` på riktig commit, versjonsnotater, ferdigbygd pakke og `SHA256SUMS`. Kontroller vedleggene etter opplasting, publiser ferdige stabile utgaver som Latest, og bekreft tagg/commit og nedlasting.
7. Oppdater de tre utvalgte README-bildene når relevante UI-endringer gjør dem foreldet. Bruk faktiske bilder av appen, og merk simulerte data.
8. Rapporter versjon, bygg og Release-lenke til brukeren. Skill mellom publisering på GitHub og installasjon på brukerens PC; installasjon utføres når brukeren ber om det.

## Filutvalg og kreditering

- Hold lokale samtaler (`System meldinger/`), logger, installasjonsbackup, målerådata og store skjermbildeserier utenfor publisering. Nedlastingspakken skal være uten tester og testdata. Eksisterende små kildekodetester i repoet vedlikeholdes sammen med koden.
- Repoets nåværende private synlighet beholdes til brukeren uttrykkelig ber om noe annet.
- Vedlikehold `CONTRIBUTORS.md` med faktisk rollefordeling. Krediter Claude Code og Codex som AI-assistanse og bruk faktiske medforfattere i commits. GitHubs automatiske Contributors-statistikk skal ikke manipuleres med kunstige commits eller omskriving av historikk.
- Desktop-pakken distribueres under Releases. GitHub Packages brukes bare dersom et reelt støttet pakkeformat og behov er innført.
- Oppgi faktisk verifisert plattformstøtte; pakkeskriptet er nå for Ubuntu 26.04 amd64 med GTK ≥ 4.22 og glibc ≥ 2.39.
- GitHub-publisering aktiverer ikke maskinvarestyring. Behold prosjektets sperrer og regler for GPU-/RGB-skriving og I2C.
- Brukeravklaringen 14. november 2026 i `docs/produksjon/REGLER.md` autoriserer nå ekte, kortbundet NVML-styring av effekt, vifter og klokker gjennom én administratoraktivering. RGB/I2C og annen systemstyring krever fortsatt egen avklaring. Krediter Codex med navnet **Codex**, GitHub-kontoen `https://github.com/codex` og den verifiserte noreply-adressen `267193182+codex@users.noreply.github.com`; ikke bruk connector-botkontoen som Codex-identitet i nye commits.

## Isolerte kontroller

Automatiske tester og private skjermbildeøkter skal bruke egne hjelpermapper, konfigurasjon, socket og baseline. De skal fungere også når brukerens ekte GPU-hjelper er aktiv. Bruk aldri vertens `/run/agentic-rigg/helper.sock`, `/etc/agentic-rigg/helper.json` eller `/var/lib/agentic-rigg` som fallback i en testfixture. Kontrollerte fysiske GPU-prøver utføres separat, med uttrykkelig valgt kort og bekreftet gjenoppretting.
