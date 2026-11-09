#!/bin/bash
# Agentic Rigg · slår på endring av skjermkortet (root-hjelperen, runde 7A).
#
# Kjøres av BRUKEREN, via pkexec fra bryteren «Tillat endring av
# skjermkortet» i Innstillinger › Styring:
#
#     pkexec ~/.local/share/agentic-rigg/hjelper/aktiver.sh
#
# Hva skriptet gjør (og deaktiver.sh fjerner igjen):
#   1. kopierer hjelperen til /usr/local/libexec/agentic-rigg-helper
#      (en sti bare root kan skrive til), og skriptene til
#      /usr/local/libexec/agentic-rigg/
#   2. legger polkit-handlingene i /usr/share/polkit-1/actions/
#   3. legger systemd-enheten i /etc/systemd/system/
#   4. skriver /etc/agentic-rigg/helper.json med uid-en din og en TOM
#      trinnliste (ingenting kan endres før du godkjenner et trinn)
#   5. starter tjenesten og venter på socketen
#
# Valg:
#   --root DIR   alt under DIR i stedet for /, uten systemctl (tester)
#   --uid N      brukeren (ellers PKEXEC_UID fra pkexec)
#   --kilde DIR  hvor filene kopieres fra (ellers mappa skriptet ligger i)
#
# Kan kjøres flere ganger (ny versjon): filene skrives på nytt, trinnlisten
# blir tom igjen, og tjenesten startes på nytt.
set -euo pipefail
umask 022

feil() {
    echo "aktiver.sh: $1" >&2
    exit "${2:-1}"
}

ROOT=/
KILDE=$(dirname "$(readlink -f "$0")")
BRUKER=${PKEXEC_UID:-}
while [ $# -gt 0 ]; do
    case "$1" in
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

# Stopp en gammel hjelper før binæren byttes (kortet går da til standard).
if [ $EKTE -eq 1 ] && systemctl is-active --quiet agentic-rigg-helper.service 2>/dev/null; then
    systemctl stop agentic-rigg-helper.service || true
fi

legg 0755 "$KILDE/agentic-rigg-helper" "$BIN.ny"
mv -f "$BIN.ny" "$BIN"
for s in aktiver.sh deaktiver.sh trinn.sh; do
    legg 0755 "$KILDE/$s" "$LIBEXEC/$s"
done
legg 0644 "$KILDE/io.github.AgenticRigg.helper.policy" "$POLICY"
legg 0644 "$KILDE/agentic-rigg-helper.service" "$ENHET"

# Bygget fra binæren som nå ligger der bare root kan skrive.
BYGG=$("$BIN" --version 2>/dev/null | sed -n 's/.*bygg \([0-9]\+\).*/\1/p')
[ -n "$BYGG" ] || feil "Hjelperen svarte ikke på --version."

mkdir -p "$KONF_DIR"
chmod 0755 "$KONF_DIR"
TMP=$(mktemp "$KONF_DIR/.helper.json.XXXXXX")
printf '{"versjon":1,"uid":%s,"trinn":[],"bygg":%s,"aktivert":"%s"}\n' "$BRUKER" "$BYGG" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$TMP"
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
else
    echo "(--root: hopper over systemctl daemon-reload, enable og restart)"
fi

vis() { if [ $EKTE -eq 1 ]; then echo "$1"; else echo "${1#"$ROOT"}"; fi; }
echo "Endring av skjermkortet er slått på (bygg $BYGG, uid $BRUKER)."
echo "  hjelper:  $(vis "$BIN")"
echo "  enhet:    $(vis "$ENHET")"
echo "  polkit:   $(vis "$POLICY")"
echo "  oppsett:  $(vis "$KONF") (ingen trinn godkjent)"
echo "Slå av igjen med bryteren, eller: pkexec /usr/local/libexec/agentic-rigg/deaktiver.sh"
