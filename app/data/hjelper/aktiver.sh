#!/bin/bash
# Agentic Rigg · slår på endring av skjermkortet (root-hjelperen, runde 7A).
#
# Kjøres av BRUKEREN, via pkexec fra bryteren «Tillat endring av
# skjermkortet» i Innstillinger › Styring:
#
#     pkexec ~/.local/share/agentic-rigg/hjelper/aktiver.sh --pci PCI --uuid UUID
#
# Hva skriptet gjør (og deaktiver.sh fjerner igjen):
#   1. kopierer hjelperen til /usr/local/libexec/agentic-rigg-helper
#      (en sti bare root kan skrive til), og skriptene til
#      /usr/local/libexec/agentic-rigg/
#   2. legger polkit-handlingene i /usr/share/polkit-1/actions/
#   3. legger systemd-enheten i /etc/systemd/system/
#   4. skriver /etc/agentic-rigg/helper.json med uid-en din, bundet PCI/UUID
#      og én global autorisasjon. Aktivering endrer ingen GPU-verdier.
#   5. starter tjenesten og venter på socketen
#
# Valg:
#   --root DIR   alt under DIR i stedet for /, uten systemctl (tester)
#   --uid N      brukeren (ellers PKEXEC_UID fra pkexec)
#   --kilde DIR  hvor filene kopieres fra (ellers mappa skriptet ligger i)
#   --pci PCI    full PCI-adresse til valgt kort (dddd:bb:dd.f)
#   --uuid UUID  NVML-UUID-en til samme kort (GPU-...)
#
# Kan kjøres flere ganger (ny versjon): filene skrives på nytt, trinnlisten
# erstattes av global autorisasjon, og tjenesten startes på nytt.
set -euo pipefail
umask 022

feil() {
    echo "aktiver.sh: $1" >&2
    exit "${2:-1}"
}

ROOT=/
KILDE=$(dirname "$(readlink -f "$0")")
BRUKER=${PKEXEC_UID:-}
PCI=
UUID=
while [ $# -gt 0 ]; do
    case "$1" in
    --pci) [ $# -ge 2 ] || feil "--pci trenger en PCI-adresse" 2; PCI=$2; shift 2 ;;
    --uuid) [ $# -ge 2 ] || feil "--uuid trenger kortets UUID" 2; UUID=$2; shift 2 ;;
    --root) [ $# -ge 2 ] || feil "--root trenger en mappe" 2; ROOT=$2; shift 2 ;;
    --uid) [ $# -ge 2 ] || feil "--uid trenger et tall" 2; BRUKER=$2; shift 2 ;;
    --kilde) [ $# -ge 2 ] || feil "--kilde trenger en mappe" 2; KILDE=$2; shift 2 ;;
    -h | --help) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) feil "Ukjent argument: $1" 2 ;;
    esac
done

EKTE=0
if [ "$ROOT" = / ]; then
    EKTE=1
    [ "$(id -u)" -eq 0 ] || feil "Må kjøres som root (bryteren i appen bruker pkexec)."
else
    [ -d "$ROOT" ] || feil "Fant ikke mappa $ROOT"
    ROOT=$(readlink -f "$ROOT")
fi

case "$BRUKER" in
'' ) feil "Vet ikke hvilken bruker hjelperen skal tjene. Kjør via pkexec, eller gi --uid." 2 ;;
*[!0-9]*) feil "Ugyldig uid: $BRUKER" 2 ;;
esac
[ "$BRUKER" -gt 0 ] || feil "uid 0 (root) kan ikke være klient av hjelperen." 2
# Identitet er faste data, ikke en kommando eller et valgfritt GPU-indeksvalg.
if [ "$EKTE" -eq 0 ] && [ -z "$PCI$UUID" ]; then
    PCI=0000:00:00.0
    UUID=GPU-simulert-test
fi
[[ "$PCI" =~ ^[0-9A-Fa-f]{4}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}\.[0-7]$ ]] || feil "Gyldig --pci kreves (dddd:bb:dd.f)." 2
[[ "$UUID" =~ ^GPU-[A-Za-z0-9-]{4,76}$ ]] || feil "Gyldig --uuid kreves." 2
PCI=${PCI,,}

# Les PCI-opplysningen fra driverens eksisterende proc-fil, ikke NVML-settere.
MINOR=0
if [ "$EKTE" -eq 1 ]; then
    INFO="/proc/driver/nvidia/gpus/$PCI/information"
    [ -r "$INFO" ] || feil "Fant ikke NVIDIA-kortet $PCI i driverens PCI-opplysninger."
    MINOR=$(awk '$1 == "Device" && $2 == "Minor:" { print $3 }' "$INFO")
    [[ "$MINOR" =~ ^[0-9]{1,4}$ ]] || feil "Kortets Device Minor kunne ikke bestemmes."
    [ -c "/dev/nvidia$MINOR" ] || feil "Kortets NVIDIA-enhet mangler."
fi

if [ $EKTE -eq 1 ]; then
    getent passwd "$BRUKER" >/dev/null || feil "Fant ingen bruker med uid $BRUKER."
fi

for f in agentic-rigg-helper agentic-rigg-helper.service io.github.AgenticRigg.helper.policy aktiver.sh deaktiver.sh trinn.sh; do
    [ -f "$KILDE/$f" ] || feil "Mangler $KILDE/$f. Installer appen på nytt (install.sh)."
done
[ -x "$KILDE/agentic-rigg-helper" ] || feil "$KILDE/agentic-rigg-helper kan ikke kjøres."

# install(1) med eier root bare på et ekte system.
legg() {
    local modus=$1 fra=$2 til=$3
    if [ $EKTE -eq 1 ]; then
        install -D -m "$modus" -o root -g root "$fra" "$til"
    else
        install -D -m "$modus" "$fra" "$til"
    fi
}

BIN="$ROOT/usr/local/libexec/agentic-rigg-helper"
LIBEXEC="$ROOT/usr/local/libexec/agentic-rigg"
POLICY="$ROOT/usr/share/polkit-1/actions/io.github.AgenticRigg.helper.policy"
ENHET="$ROOT/etc/systemd/system/agentic-rigg-helper.service"
KONF_DIR="$ROOT/etc/agentic-rigg"
KONF="$KONF_DIR/helper.json"
STATE="$ROOT/var/lib/agentic-rigg"
BASELINE="$STATE/baseline.json"
DIRTY="$STATE/dirty.json"

# Stopp og gjenopprett egne endringer før en gammel binær byttes.
if [ $EKTE -eq 1 ] && [ -f "$KONF" ] && [ -x "$BIN" ]; then
    if systemctl is-active --quiet agentic-rigg-helper.service 2>/dev/null; then
        systemctl stop agentic-rigg-helper.service || feil "Kunne ikke stoppe den gamle hjelperen."
    fi
    # Også etter en tidligere feil/krasj: ingen ny installasjon over ukjent tilstand.
    "$BIN" --backend nvml --tilbakestill || feil "Gjenoppretting kunne ikke bekreftes. Den gamle installasjonen beholdes."
fi

# Ny administratoraktivering skal fange brukerens NÅVÆRENDE utgangspunkt,
# også etter en tidligere deaktivering. Aldri fjern opptaket ved recovery:
# gammel restore må ha lyktes, og ingen uavklart endringsmarkør får finnes.
if [ -e "$DIRTY" ] || [ -L "$DIRTY" ]; then
    feil "Uavklarte GPU-endringer må gjenopprettes før ny aktivering. Grunnverdier og installasjon beholdes."
fi
if [ -e "$BASELINE" ] || [ -L "$BASELINE" ]; then
    [ -f "$BASELINE" ] && [ ! -L "$BASELINE" ] || feil "Grunnverdifila har utrygg filtype; den beholdes."
    [ -d "$STATE" ] && [ ! -L "$STATE" ] || feil "Grunnverdimappa har utrygg filtype; den beholdes."
    [ "$(stat -c %u "$STATE")" = "$(id -u)" ] || feil "Grunnverdimappa har feil eier."
    [ "$(stat -c %a "$STATE")" = 700 ] || feil "Grunnverdimappa må ha modus 700."
    [ "$(stat -c %u "$BASELINE")" = "$(id -u)" ] || feil "Grunnverdifila har feil eier."
    [ "$(stat -c %a "$BASELINE")" = 600 ] || feil "Grunnverdifila må ha modus 600."
    ARKIV=$(mktemp "$STATE/baseline-forrige-XXXXXX.json") || feil "Kunne ikke opprette grunnverdiarkiv."
    if ! mv -f -- "$BASELINE" "$ARKIV"; then
        rm -f -- "$ARKIV"
        feil "Kunne ikke arkivere forrige grunnverdier. Aktiveringen avbrytes."
    fi
fi

legg 0755 "$KILDE/agentic-rigg-helper" "$BIN.ny"
mv -f "$BIN.ny" "$BIN"
for s in aktiver.sh deaktiver.sh trinn.sh; do
    legg 0755 "$KILDE/$s" "$LIBEXEC/$s"
done
legg 0644 "$KILDE/io.github.AgenticRigg.helper.policy" "$POLICY"
legg 0644 "$KILDE/agentic-rigg-helper.service" "$ENHET"
DROPIN="$ROOT/etc/systemd/system/agentic-rigg-helper.service.d"
mkdir -p "$DROPIN"
chmod 0755 "$DROPIN"
printf '[Service]\nDeviceAllow=\nDeviceAllow=/dev/nvidiactl rw\nDeviceAllow=/dev/nvidia%s rw\n' "$MINOR" >"$DROPIN/gpu.conf"
chmod 0644 "$DROPIN/gpu.conf"
if [ $EKTE -eq 1 ]; then chown root:root "$DROPIN" "$DROPIN/gpu.conf"; fi

# Bygget fra binæren som nå ligger der bare root kan skrive.
BYGG=$("$BIN" --version 2>/dev/null | sed -n 's/.*bygg \([0-9]\+\).*/\1/p')
[ -n "$BYGG" ] || feil "Hjelperen svarte ikke på --version."

mkdir -p "$KONF_DIR"
chmod 0755 "$KONF_DIR"
TMP=$(mktemp "$KONF_DIR/.helper.json.XXXXXX")
printf '{"versjon":2,"uid":%s,"enabled":true,"pci":"%s","uuid":"%s","bygg":%s,"aktivert":"%s"}\n' "$BRUKER" "$PCI" "$UUID" "$BYGG" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$TMP"
chmod 0644 "$TMP"
if [ $EKTE -eq 1 ]; then chown root:root "$TMP"; fi
mv -f "$TMP" "$KONF"

if [ $EKTE -eq 1 ]; then
    systemctl daemon-reload
    systemctl enable agentic-rigg-helper.service >/dev/null 2>&1 || feil "Kunne ikke slå på tjenesten (systemctl enable)."
    systemctl restart agentic-rigg-helper.service || feil "Tjenesten startet ikke. Se: journalctl -u agentic-rigg-helper"
    for _ in $(seq 1 50); do
        [ -S /run/agentic-rigg/helper.sock ] && break
        sleep 0.1
    done
    [ -S /run/agentic-rigg/helper.sock ] || feil "Tjenesten startet, men socketen kom ikke. Se: journalctl -u agentic-rigg-helper"
    systemctl is-active --quiet agentic-rigg-helper.service || feil "Hjelperens NVML-backend er ikke klar. Se: journalctl -u agentic-rigg-helper"
else
    echo "(--root: hopper over systemctl daemon-reload, enable og restart)"
fi

vis() { if [ $EKTE -eq 1 ]; then echo "$1"; else echo "${1#"$ROOT"}"; fi; }
echo "Endring av skjermkortet er slått på (bygg $BYGG, uid $BRUKER)."
echo "  hjelper:  $(vis "$BIN")"
echo "  enhet:    $(vis "$ENHET")"
echo "  polkit:   $(vis "$POLICY")"
echo "  oppsett:  $(vis "$KONF") (global autorisasjon, kort $PCI)"
echo "Slå av igjen med bryteren, eller: pkexec /usr/local/libexec/agentic-rigg/deaktiver.sh"
