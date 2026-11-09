#!/usr/bin/env bash
# Runde 7A og 7C: målingene for 1.2.0 i én privat, hodeløs økt
# (app/scripts/okt.sh), med P-tilstanden logget ved siden av. Bygd på
# runde6d/maal6d.sh; RUNDE velger filnavnene (runde7c-maal.txt osv.).
#
#   RUNDE=7c maal7c.sh [UTMAPPE]     (standard docs/produksjon/maal)
#   RUNDE=7a PAGES= maal7c.sh        (bare kald start og tjenesten i bakgrunnen)
#
# Env: RIGG_BIN (standard ~/.cache/rigg-build/release), RUNS (kalde starter, 15),
#      SECS (sekunder per måling, 20), REPS (målinger per side, 3),
#      BG_SECS (tjenesten alene før og etter vinduet, 180), PAGES (alle ti).
#
# Rekkefølgen i økta:
#   1. Tjenesten alene i BG_SECS («Fortsett i bakgrunnen» på, 2 s), uten
#      vindu: CPU, minne, og om kortet går til P0 (punkt 1 i oppdraget).
#   2. RUNS kalde starter med lukking (RIGG_TIMING, veggklokka fra exec).
#   3. Hver side for seg: vinduet med --side, 10 s oppvarming, så REPS × SECS
#      CPU for vinduet og tjenesten, og minnet.
#   4. Tjenesten alene igjen i BG_SECS etter at siste vindu er lukket.
#
# nvidia-smi --query-gpu (bare lesing) logger P-tilstanden hvert sekund til
# runde${RUNDE}-pstate.csv, med en linje for hvert steg. Alt annet er lesing fra
# /proc. Ingenting skrives til kortet.
set -euo pipefail
RUNDE=${RUNDE:-7c}
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
app="$here/../../../../app"
out=${1:-$here/../../maal}
mkdir -p "$out"
out=$(cd -- "$out" && pwd)
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/release}
[ -x "$bin/agentic-rigg" ] && [ -x "$bin/agentic-riggd" ] || { echo "Fant ikke binærene i $bin" >&2; exit 1; }
pslog=$out/runde${RUNDE}-pstate.csv
res=$out/runde${RUNDE}-maal.txt
: >"$pslog"
# >> (O_APPEND): stegmerkene fra økta havner mellom prøvene, ikke oppå dem.
nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu --format=csv,noheader -lms 1000 >>"$pslog" 2>/dev/null &
smi=$!
trap 'kill $smi 2>/dev/null || true' EXIT
sleep 10
runner=$(mktemp)
cat >"$runner" <<'RUN'
set -u
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
pct() { awk -v a="$1" -v b="$2" -v hz="$hz" -v s="$3" 'BEGIN{printf "%.2f",(b-a)/hz/s*100}'; }
mark() { echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- $* ---" >>"$RIGG_PSLOG"; }
echo "# $(date -Is) · $(uname -r) · $(nproc) tråder · last $(cut -d' ' -f1-3 /proc/loadavg) · MemAvailable $(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo) MB"
echo "# $("$RIGG_BIN/agentic-rigg" --version)"
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
mark "tjenesten starter"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 4
a=$(ticks $d); sleep "$RIGG_BG_SECS"; b=$(ticks $d)
echo "RES tjeneste_alene_før cpu=$(pct $a $b "$RIGG_BG_SECS")% $(mem $d) tråder=$(ls /proc/$d/task | wc -l) last=$(cut -d' ' -f1 /proc/loadavg)"
echo "# hjelperen: $(ls /etc/agentic-rigg/helper.json 2>/dev/null || echo 'ikke aktivert (av)')"
mark "kalde starter"
for i in $(seq 1 "$RIGG_RUNS"); do
    log=$(mktemp)
    t0=${EPOCHREALTIME/[.,]/}
    RIGG_TIMING=1 RIGG_TIMING_LUKK=800 "$RIGG_BIN/agentic-rigg" 2>"$log"
    t1=${EPOCHREALTIME/[.,]/}
    awk -v t0="$t0" -v t1="$t1" -v i="$i" '
      /RIGG_TIMING mark=/ { split($2,a,"="); split($4,b,"="); m[a[2]]=(b[2]/1000 - t0)/1000 }
      END { printf "START %s first_frame=%.1f telemetry=%.1f lukk_til_skjult=%.1f lukk_til_avsluttet=%.1f\n",
            i, m["first_frame"], m["telemetry"], m["hidden"]-m["close"], (t1-t0)/1000 - m["close"] }' "$log"
    rm -f "$log"
    sleep 0.7
done
for p in $RIGG_PAGES; do
    mark "vindu $p"
    "$RIGG_BIN/agentic-rigg" --side "$p" 2>"$OKT_BASE/w-$p.log" &
    w=$!
    sleep 10
    vals="" dv=""
    for i in $(seq 1 "$RIGG_REPS"); do
        a=$(ticks $w); b0=$(ticks $d); sleep "$RIGG_SECS"; b=$(ticks $w); b1=$(ticks $d)
        vals="$vals $(pct $a $b "$RIGG_SECS")"
        dv="$dv $(pct $b0 $b1 "$RIGG_SECS")"
    done
    echo "RES side=$p vindu_cpu=[$vals ] tjeneste_cpu=[$dv ] vindu: $(mem $w) tjeneste: $(mem $d) last=$(cut -d' ' -f1 /proc/loadavg)"
    kill -TERM $w; wait $w 2>/dev/null
    grep -E "CRITICAL|panicked" "$OKT_BASE/w-$p.log" | head -3
    sleep 1
done
mark "tjenesten alene etter vinduet"
sleep 3
a=$(ticks $d); sleep "$RIGG_BG_SECS"; b=$(ticks $d)
echo "RES tjeneste_alene_etter cpu=$(pct $a $b "$RIGG_BG_SECS")% $(mem $d) last=$(cut -d' ' -f1 /proc/loadavg)"
mark "tjenesten stopper"
kill -TERM $d; wait $d 2>/dev/null
RUN
pages=${PAGES-oversikt sensorer grafer grafikkort prosessor disker system kontroller viftekurve lys}
set +e
RIGG_BIN="$bin" RIGG_PROFIL=dev RIGG_PSLOG="$pslog" RIGG_RUNS="${RUNS:-15}" RIGG_SECS="${SECS:-20}" RIGG_REPS="${REPS:-3}" \
    RIGG_BG_SECS="${BG_SECS:-180}" RIGG_PAGES="$pages" RIGG_OKT_DIR="$HOME/.cache/rigg-build/okt-${RUNDE}-maal" OKT_WARMUP=4 \
    "$app/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner" | grep --line-buffered -E "^(#|RES|START)|CRITICAL|panicked" | tee "$res"
rc=$?
set -e
rm -f -- "$runner"
sleep 10
kill $smi 2>/dev/null || true
wait $smi 2>/dev/null || true
echo "# P-tilstander: $(awk -F', ' '!/---/ {c[$2]++} END {for (k in c) printf "%s=%d ", k, c[k]}' "$pslog")" | tee -a "$res"
awk -F', ' '/---/ {print; next} $2!="P8" {print "  " $0}' "$pslog" | tee -a "$res"
exit $rc
