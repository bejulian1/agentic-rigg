#!/usr/bin/env bash
# Skjermbilder og målinger for runde 7B (Vifter med viftemotoren). Bygd på
# runde6c/bilder6c.sh. Planene `7b-ekte` (NVML, bare lesing, låst) og `7b-sim`
# (--backend sim:spill) i app/crates/rigg/src/pages/styring_bilder.rs.
# Hver plan får sin egen private, hodeløse økt (app/scripts/okt.sh) med egen
# RIGG_OKT_DIR, sin egen tjeneste og sin egen mappe.
#
# Mens planen `7b-ekte` går, logges P-tilstanden med
# `nvidia-smi --query-gpu … -l 1` (bare lesing) til `7b-ekte/pstate.csv`, med
# 20 s før økta starter og 10 s etter. Vinduets CPU i ro står i `maal.txt`, tjenestens i `tjeneste.txt`.
#
# Bruk: bilder7b.sh [MAPPE]   (standard docs/skjermbilder/1.2.0/7b)
#   RIGG_7B_PLANER="7b-ekte 7b-sim"  hvilke planer
#   RIGG_BIN                   mappa med binærene (standard ~/.cache/rigg-build/release)
#   RIGG_OPPVARMING            sekunder tjenesten samler før vinduet (ekte 150, sim 320: fem minutter spill-last i historikken)
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd)
out=${1:-$root/docs/skjermbilder/1.2.0/7b}
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
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$d/stat" 2>/dev/null || echo 0; }
# Tjenesten alene (uten vindu) i 30 s før vinduet starter.
a0=$(ticks); sleep 30; a1=$(ticks)
echo "tjenesten alene, 30 s: $(LC_ALL=C awk -v a="$a0" -v b="$a1" -v hz="$hz" 'BEGIN { printf "%.2f", (b - a) / hz / 30 * 100 }') % av én kjerne" >>"$RIGG_BILDER_2C/tjeneste.txt"
t0=$(date +%s.%N); b0=$(ticks)
timeout "${RIGG_PLAN_FRIST:-900}" "$RIGG_BIN/agentic-rigg" --side kontroller 2>"$OKT_BASE/rigg.log"
rc=$?
t1=$(date +%s.%N); b1=$(ticks)
echo "tjenesten med vinduet, hele planen ($(LC_ALL=C awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.0f", b - a }') s): $(LC_ALL=C awk -v a="$b0" -v b="$b1" -v hz="$hz" -v t0="$t0" -v t1="$t1" 'BEGIN { printf "%.2f", (b - a) / hz / (t1 - t0) * 100 }') % av én kjerne" >>"$RIGG_BILDER_2C/tjeneste.txt"
awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%d MB ", $1, $2/1024}' "/proc/$d/smaps_rollup" >>"$RIGG_BILDER_2C/tjeneste.txt" 2>/dev/null; echo >>"$RIGG_BILDER_2C/tjeneste.txt"
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
grep -E "CSS|CRITICAL|WARNING|panicked|RIGG_SKJERMBILDE|RIGG_2C_MAAL|RIGG_BILDER_2C|RIGG_BREDDE|fant ikke" "$OKT_BASE/rigg.log" | head -120 >&2
exit $rc
RUN
rc_all=0
for plan in ${RIGG_7B_PLANER:-7b-ekte 7b-sim}; do
    dir=$out/$plan
    rm -rf -- "$dir"
    mkdir -p "$dir"
    backend= dargs= warm=
    case "$plan" in
    7b-ekte) warm=${RIGG_OPPVARMING:-150} ;;
    7b-sim) backend=sim:spill warm=${RIGG_OPPVARMING:-320} ;;
    *) echo "Ukjent plan: $plan" >&2; exit 2 ;;
    esac
    logger=
    if [ "$plan" = 7b-ekte ] && command -v nvidia-smi >/dev/null; then
        nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu --format=csv,noheader -l 1 >>"$dir/pstate.csv" 2>/dev/null &
        logger=$!
        sleep 20
        echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- økta starter ---" >>"$dir/pstate.csv"
    fi
    echo "== $plan → $dir"
    set +e
    env -u RIGG_BACKEND RIGG_BILDER_2C="$dir" RIGG_6C_PLAN="$plan" ${backend:+RIGG_BACKEND=$backend} RIGG_BIN="$bin" RIGG_WARM="$warm" RIGG_RIGGD_ARGS="$dargs" \
        RIGG_OKT_DIR="$HOME/.cache/rigg-build/okt-7b-$plan" \
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
