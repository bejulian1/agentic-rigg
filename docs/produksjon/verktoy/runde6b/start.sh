# Runde 6B: kald start til første bilde, før (1.0.0 · bygg 16) og etter, i
# samme private økt og annenhver gang, så begge får den samme lasten på
# maskinen. Samme merker som scripts/maal.sh (RIGG_TIMING, veggklokka fra
# exec). Tjenesten (fra etter-bygget) leser det ekte kortet, bare lesing.
# Bruk (inne i okt.sh): RIGG_6B_FOR=MAPPE RIGG_6B_ETTER=MAPPE RIGG_RUNS=N bash start.sh
set -u
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
"$RIGG_6B_ETTER/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 4
echo "uname: $(uname -r) · $(nproc) tråder · last $(cut -d' ' -f1-3 /proc/loadavg)"
one() { # navn mappe nr
    local log t0 t1
    log=$(mktemp)
    t0=${EPOCHREALTIME/[.,]/}
    RIGG_TIMING=1 RIGG_TIMING_LUKK=800 "$2/agentic-rigg" 2>"$log"
    t1=${EPOCHREALTIME/[.,]/}
    awk -v t0="$t0" -v n="$1" -v i="$3" '
      /RIGG_TIMING mark=/ { split($2,a,"="); split($4,b,"="); m[a[2]]=(b[2]/1000 - t0)/1000 }
      END { printf "%s_%s: main=%.1f first_frame=%.1f\n", n, i, m["main"], m["first_frame"] }' "$log"
    rm -f "$log"
    sleep 0.7
}
for i in $(seq 1 "$RIGG_RUNS"); do
    one for "$RIGG_6B_FOR" "$i"
    one etter "$RIGG_6B_ETTER" "$i"
done
echo "last etter: $(cut -d' ' -f1-3 /proc/loadavg)"
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
