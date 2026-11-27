#!/bin/bash
# Falsk pkexec for utviklerprofilen, testene og skjermbildene (runde 7A).
# Brukes av Innstillinger › Styring når RIGG_PKEXEC peker hit og RIGG_PROFIL=dev.
#
# Spør ikke om passord og blir ikke root. Kjører skriptet som vanlig bruker
# med «--root $RIGG_HJELPER_ROT», så alt havner i en falsk rot, aldri i /.
# Etter aktiver.sh startes hjelperen fra den falske roten med --backend sim,
# og før deaktiver.sh stoppes den. NVML er aldri tillatt i denne testveien.
#
# RIGG_FALSK_SVAR:
#   avbrutt  som når passordvinduet lukkes (kode 126, ingenting kjøres)
#   krasj    aktiver.sh kjøres, men hjelperen startes ikke (status «Feil»)
set -euo pipefail
umask 022

[ "${RIGG_FALSK_HJELPER_BACKEND:-sim}" = sim ] || { echo "falsk-pkexec: bare backend sim er tillatt; NVML kan skrive til ekte maskinvare" >&2; exit 2; }

ROT=${RIGG_HJELPER_ROT:-}
[ -n "$ROT" ] && [ "$ROT" != / ] || { echo "falsk-pkexec: RIGG_HJELPER_ROT må være en tempmappe" >&2; exit 127; }
[ $# -ge 1 ] || { echo "falsk-pkexec: mangler program" >&2; exit 127; }
[[ "$ROT" = /tmp/* && "$ROT" = "$(realpath -e -- "$ROT")" && $(stat -c %a "$ROT") = 700 && $(stat -c %u "$ROT") = "$EUID" && $EUID != 0 ]] || exit 2
[ "$ROT" != / ] || { echo "falsk-pkexec: falsk rot kan ikke peke på /" >&2; exit 127; }

case "${RIGG_FALSK_SVAR:-}" in
avbrutt) echo "Error executing command as another user: Not authorized" >&2; exit 126 ;;
esac

prog=$1
shift
navn=$(basename "$prog")
pidfil="$ROT/run/agentic-rigg/falsk.pid"

stopp() {
    if [ -f "$pidfil" ]; then
        kill "$(cat "$pidfil")" 2>/dev/null || true
        for _ in $(seq 1 50); do
            kill -0 "$(cat "$pidfil")" 2>/dev/null || break
            sleep 0.05
        done
        rm -f "$pidfil"
    fi
}

[ "$navn" = deaktiver.sh ] && stopp
[ "$navn" = aktiver.sh ] && stopp

case "$navn" in
aktiver.sh) action=activate ;;
deaktiver.sh) action=deactivate ;;
*) exit 2 ;;
esac
src=${RIGG_HJELPER_DATA:?}
install -Dm755 "$src/agentic-rigg-helper" "$ROT/usr/local/libexec/agentic-rigg-helper"
install -Dm644 "$src/agentic-rigg-helper.service" "$ROT/usr/local/libexec/agentic-rigg/agentic-rigg-helper.service"
install -Dm644 "$src/agentic-rigg-card.service" "$ROT/usr/local/libexec/agentic-rigg/agentic-rigg-card.service"
mkdir -p "$ROT/usr/local/libexec/agentic-rigg-system"
printf '3:%s\n' "$("$src/agentic-rigg-helper" --version | sed -n 's/.*bygg \([0-9]*\).*/\1/p')" > "$ROT/usr/local/libexec/agentic-rigg-system/revision"
if [[ $action == activate && $# == 5 && $1 == --card-control && $2 == --pci && $3 =~ ^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$ && $4 == --uuid && $5 =~ ^GPU-sim0000-[0-9]{4}$ ]]; then
    mkdir -p "$ROT/proc/driver/nvidia/gpus/$3"
    printf 'GPU UUID: %s\nBus Location: %s\nDevice Minor: 0\n' "$5" "$3" > "$ROT/proc/driver/nvidia/gpus/$3/information"
fi
/usr/bin/python3 -I -S "$src/nvml-admin.py" "$action" --uid "$EUID" --root "$ROT" "$@"

if [ "$navn" = aktiver.sh ] && [ "${RIGG_FALSK_SVAR:-}" != krasj ]; then
    mkdir -p "$ROT/run/agentic-rigg" "$ROT/var/lib/agentic-rigg"
    chmod 700 "$ROT/var/lib/agentic-rigg"
    setsid "$ROT/usr/local/libexec/agentic-rigg-helper" --backend sim \
        --socket "$ROT/run/agentic-rigg/helper.sock" --config "$ROT/etc/agentic-rigg/helper.json" \
        --baseline "$ROT/var/lib/agentic-rigg/baseline.json" \
        >>"$ROT/run/agentic-rigg/helper.log" 2>&1 </dev/null &
    echo $! >"$pidfil"
    for _ in $(seq 1 100); do
        [ -S "$ROT/run/agentic-rigg/helper.sock" ] && exit 0
        sleep 0.05
    done
    echo "falsk-pkexec: hjelperen startet ikke (se $ROT/run/agentic-rigg/helper.log)" >&2
    exit 1
fi
