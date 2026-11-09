#!/usr/bin/env bash
# Skjermbilder for runde 7A: Innstillinger › Styring og merket i sidefeltet,
# planene `7a` og `7a-ekte` i app/crates/rigg/src/skjermbilder.rs. Hver plan
# får sin egen private, hodeløse økt (app/scripts/okt.sh) med egen
# RIGG_OKT_DIR, sin egen tjeneste og sin egen falske rot.
#
# Bryteren kjører app/scripts/falsk-pkexec.sh (RIGG_PKEXEC i dev): ingen
# root, ingen polkit, ingen systemctl. aktiver.sh legger alt i den falske
# roten, og hjelperen startes som vanlig bruker fra den:
#   7a       --backend sim  (tjenesten og hjelperen med det simulerte kortet)
#   7a-ekte  --backend nvml (tjenesten leser det ekte kortet; hjelperen har
#            det ekte kortet sperret i 1.2.0 og laster ikke NVML)
#
# Bruk: bilder7a.sh [MAPPE]   (standard docs/skjermbilder/1.2.0/7a)
#   RIGG_7A_PLANER="7a 7a-ekte"  hvilke planer
#   RIGG_BIN                    mappa med binærene (standard ~/.cache/rigg-build/release)
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd)
out=${1:-$root/docs/skjermbilder/1.2.0/7a}
mkdir -p "$out"
out=$(cd -- "$out" && pwd)
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/release}
for b in agentic-rigg agentic-riggd agentic-rigg-helper; do
    [ -x "$bin/$b" ] || { echo "Fant ikke $bin/$b" >&2; exit 1; }
done
cache=$HOME/.cache/rigg-build
runner=$(mktemp)
trap 'rm -f -- "$runner"' EXIT
cat >"$runner" <<'RUN'
set -u
# shellcheck disable=SC2086
"$RIGG_BIN/agentic-riggd" --grace-ms 600000 ${RIGG_BACKEND:+--backend "$RIGG_BACKEND"} 2>"$OKT_BASE/riggd.log" &
d=$!
sleep "$RIGG_WARM"
timeout "${RIGG_PLAN_FRIST:-600}" "$RIGG_BIN/agentic-rigg" 2>"$OKT_BASE/rigg.log"
rc=$?
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
grep -E "CSS|CRITICAL|WARNING|panicked|RIGG_SKJERMBILDE|fant ikke" "$OKT_BASE/rigg.log" | head -60 >&2
exit $rc
RUN
rc_all=0
for plan in ${RIGG_7A_PLANER:-7a 7a-ekte}; do
    dir=$out/$plan
    rm -rf -- "$dir"
    mkdir -p "$dir"
    fake=$cache/7a-rot-$plan
    data=$cache/7a-hjelper-$plan
    rm -rf -- "$fake" "$data"
    mkdir -p "$fake" "$data"
    # Som install.sh: skriptene, enheten, policyen og binæren.
    cp "$root"/app/data/hjelper/* "$data/"
    cp "$bin/agentic-rigg-helper" "$data/"
    backend= hb=sim warm=8
    case "$plan" in
    7a) backend=sim ;;
    7a-ekte) hb=nvml warm=12 ;;
    *) echo "Ukjent plan: $plan" >&2; exit 2 ;;
    esac
    echo "== $plan → $dir"
    set +e
    env -u RIGG_BACKEND RIGG_SKJERMBILDER="$dir" RIGG_PLAN="$plan" ${backend:+RIGG_BACKEND=$backend} RIGG_BIN="$bin" RIGG_WARM="$warm" \
        RIGG_PKEXEC="$root/app/scripts/falsk-pkexec.sh" RIGG_HJELPER_DATA="$data" RIGG_HJELPER_ROT="$fake" RIGG_FALSK_HJELPER_BACKEND="$hb" \
        RIGG_OKT_DIR="$cache/okt-7a-$plan" \
        "$root/app/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner"
    rc=$?
    set -e
    # Hjelperen den falske pkexec startet, skal være stoppet (deaktiver i planen).
    if [ -f "$fake/run/agentic-rigg/falsk.pid" ]; then
        kill "$(cat "$fake/run/agentic-rigg/falsk.pid")" 2>/dev/null || true
    fi
    find "$fake" -type f | sed "s|$fake/||" | sort >"$dir/falsk-rot-etter.txt"
    cp "$fake/run/agentic-rigg/helper.log" "$dir/helper.log" 2>/dev/null || true
    rm -rf -- "$fake" "$data"
    [ $rc -eq 0 ] || { echo "Planen $plan feilet ($rc)" >&2; rc_all=$rc; }
done
echo "Skjermbildene ligger i $out"
exit $rc_all
