#!/usr/bin/env bash
# Skjermbilder og målinger for runde 6C (Kontroller og Viftekurve, nå
# «Klokker og effekt» og «Vifter»): planene `ekte` (NVML, bare lesing, låst)
# og `sim` (--backend sim) i app/crates/rigg/src/pages/styring_bilder.rs.
# Hver plan får sin egen private, hodeløse økt (app/scripts/okt.sh) med egen
# RIGG_OKT_DIR, sin egen tjeneste og sin egen mappe.
#
# Mens planen `ekte` går, logges P-tilstanden med
# `nvidia-smi --query-gpu … -l 1` (bare lesing) til `ekte/pstate.csv`, med
# 20 s før økta starter og 10 s etter. Vinduets CPU i ro står i `ekte/maal.txt`.
#
# Bruk: bilder6c.sh [MAPPE]   (standard docs/skjermbilder/1.1.0/6c)
#   RIGG_6C_PLANER="ekte sim"  hvilke planer
#   RIGG_BIN                   mappa med binærene (standard ~/.cache/rigg-build/release)
#   RIGG_OPPVARMING            sekunder tjenesten samler før vinduet (ekte 150, sim 8)
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd)
out=${1:-$root/docs/skjermbilder/1.1.0/6c}
mkdir -p "$out"
out=$(cd -- "$out" && pwd)
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/release}
[ -x "$bin/agentic-rigg" ] && [ -x "$bin/agentic-riggd" ] || { echo "Fant ikke binærene i $bin" >&2; exit 1; }
runner=$(mktemp)
trap 'rm -f -- "$runner"' EXIT
cat >"$runner" <<'RUN'
set -u
# shellcheck disable=SC2086
"$RIGG_BIN/agentic-riggd" --grace-ms 600000 ${RIGG_BACKEND:+--backend "$RIGG_BACKEND"} $RIGG_RIGGD_ARGS 2>"$OKT_BASE/riggd.log" &
d=$!
sleep "$RIGG_WARM"
timeout "${RIGG_PLAN_FRIST:-900}" "$RIGG_BIN/agentic-rigg" --side kontroller 2>"$OKT_BASE/rigg.log"
rc=$?
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
grep -E "CSS|CRITICAL|WARNING|panicked|RIGG_SKJERMBILDE|RIGG_2C_MAAL|RIGG_BILDER_2C|RIGG_BREDDE|fant ikke" "$OKT_BASE/rigg.log" | head -120 >&2
exit $rc
RUN
rc_all=0
for plan in ${RIGG_6C_PLANER:-ekte sim}; do
    dir=$out/$plan
    rm -rf -- "$dir"
    mkdir -p "$dir"
    backend= dargs= warm=
    case "$plan" in
    ekte) warm=${RIGG_OPPVARMING:-150} ;;
    sim) backend=sim dargs=--syntetisk-historikk warm=${RIGG_OPPVARMING:-8} ;;
    *) echo "Ukjent plan: $plan" >&2; exit 2 ;;
    esac
    logger=
    if [ "$plan" = ekte ] && command -v nvidia-smi >/dev/null; then
        nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu --format=csv,noheader -l 1 >>"$dir/pstate.csv" 2>/dev/null &
        logger=$!
        sleep 20
        echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- økta starter ---" >>"$dir/pstate.csv"
    fi
    echo "== $plan → $dir"
    set +e
    env -u RIGG_BACKEND RIGG_BILDER_2C="$dir" RIGG_6C_PLAN="$plan" ${backend:+RIGG_BACKEND=$backend} RIGG_BIN="$bin" RIGG_WARM="$warm" RIGG_RIGGD_ARGS="$dargs" \
        RIGG_OKT_DIR="$HOME/.cache/rigg-build/okt-6c-$plan" \
        "$root/app/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner"
    rc=$?
    set -e
    if [ -n "$logger" ]; then
        echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- økta er ferdig ---" >>"$dir/pstate.csv"
        sleep 10
        kill "$logger" 2>/dev/null || true
        wait "$logger" 2>/dev/null || true
    fi
    [ $rc -eq 0 ] || { echo "Planen $plan feilet ($rc)" >&2; rc_all=$rc; }
done
echo "Skjermbildene ligger i $out"
exit $rc_all
