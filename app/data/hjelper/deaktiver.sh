#!/bin/bash
# Agentic Rigg · slår av endring av skjermkortet og fjerner root-hjelperen
# (runde 7A). Kjøres av BRUKEREN via pkexec fra bryteren i Innstillinger ›
# Styring, helst fra kopien bare root kan endre:
#
#     pkexec /usr/local/libexec/agentic-rigg/deaktiver.sh
#
# Stopper tjenesten og bekrefter gjenoppretting av egne endringer før den
# slås av ved oppstart og installerte hjelpefiler fjernes. Fangede grunnverdier
# i /var/lib/agentic-rigg beholdes. Ved feil beholdes også installasjonen.
#
# Valg:
#   --root DIR   alt under DIR i stedet for /, uten systemctl (tester)
#
# Kan kjøres flere ganger; det som allerede er borte, hoppes over.
set -euo pipefail

feil() {
    echo "deaktiver.sh: $1" >&2
    exit "${2:-1}"
}

ROOT=/
while [ $# -gt 0 ]; do
    case "$1" in
    --root) [ $# -ge 2 ] || feil "--root trenger en mappe" 2; ROOT=$2; shift 2 ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) feil "Ukjent argument: $1" 2 ;;
    esac
done

EKTE=0
if [ "$ROOT" = / ]; then
    EKTE=1
    [ "$(id -u)" -eq 0 ] || feil "Må kjøres som root (bryteren i appen bruker pkexec)."
    ROOT=
else
    [ -d "$ROOT" ] || feil "Fant ikke mappa $ROOT"
    ROOT=$(readlink -f "$ROOT")
fi

ENHET="$ROOT/etc/systemd/system/agentic-rigg-helper.service"
if [ $EKTE -eq 1 ]; then
    if systemctl cat agentic-rigg-helper.service >/dev/null 2>&1; then
        # Ikke fjern noe før en uavhengig gjenoppretting har lyktes.
        systemctl stop agentic-rigg-helper.service || feil "Kunne ikke stoppe tjenesten. Filene beholdes."
        "$ROOT/usr/local/libexec/agentic-rigg-helper" --backend nvml --tilbakestill || feil "Kunne ikke bekrefte at kortet er gjenopprettet. Hjelperen og grunnverdifila beholdes."
        systemctl disable agentic-rigg-helper.service >/dev/null 2>&1 || feil "Kunne ikke slå av tjenesten ved oppstart."

    elif [ -f "$ROOT/etc/agentic-rigg/helper.json" ] && [ -x "$ROOT/usr/local/libexec/agentic-rigg-helper" ]; then
        "$ROOT/usr/local/libexec/agentic-rigg-helper" --backend nvml --tilbakestill || feil "Gjenoppretting kunne ikke bekreftes; filene beholdes."
    fi
else
    echo "(--root: hopper over systemctl disable --now)"
fi

fjernet=0
for f in \
    "$ENHET" \
    "$ROOT/etc/systemd/system/agentic-rigg-helper.service.d/gpu.conf" \
    "$ROOT/etc/systemd/system/multi-user.target.wants/agentic-rigg-helper.service" \
    "$ROOT/usr/share/polkit-1/actions/io.github.AgenticRigg.helper.policy" \
    "$ROOT/etc/agentic-rigg/helper.json" \
    "$ROOT/usr/local/libexec/agentic-rigg-helper" \
    "$ROOT/usr/local/libexec/agentic-rigg/aktiver.sh" \
    "$ROOT/usr/local/libexec/agentic-rigg/trinn.sh" \
    "$ROOT/usr/local/libexec/agentic-rigg/deaktiver.sh"; do
    if [ -e "$f" ] || [ -L "$f" ]; then
        rm -f -- "$f"
        fjernet=$((fjernet + 1))
    fi
done
# Grunnverdifila i /var/lib/agentic-rigg beholdes for sikker gjenoppretting.
# Mappene bare når de er tomme (ingenting annet skal ligge der).
for d in "$ROOT/etc/systemd/system/agentic-rigg-helper.service.d" "$ROOT/etc/agentic-rigg" "$ROOT/usr/local/libexec/agentic-rigg" "$ROOT/run/agentic-rigg"; do
    [ -d "$d" ] && rmdir --ignore-fail-on-non-empty -- "$d"
done

if [ $EKTE -eq 1 ]; then
    systemctl daemon-reload
    systemctl reset-failed agentic-rigg-helper.service >/dev/null 2>&1 || true
fi

if [ $fjernet -eq 0 ]; then
    echo "Endring av skjermkortet var allerede slått av. Ingenting å fjerne."
else
    echo "Endring av skjermkortet er slått av. Hjelperen og $fjernet filer er fjernet."
fi
