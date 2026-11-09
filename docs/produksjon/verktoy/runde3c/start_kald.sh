# Kald start når tjenesten ikke kjører: vinduet starter den selv (dev-profilen: egen prosess).
set -u
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
for i in $(seq 1 "$RUNS"); do
  pkill -TERM -f "$RIGG_BIN/agentic-riggd" 2>/dev/null; sleep 1.5
  log=$(mktemp)
  t0=${EPOCHREALTIME/[.,]/}
  RIGG_TIMING=1 RIGG_TIMING_LUKK=1500 "$RIGG_BIN/agentic-rigg" 2>"$log"
  t1=${EPOCHREALTIME/[.,]/}
  awk -v t0="$t0" -v i="$i" '
    /RIGG_TIMING mark=/ { split($2,a,"="); split($4,b,"="); m[a[2]]=(b[2]/1000 - t0)/1000 }
    END { printf "RES kald_tjeneste run=%s first_frame=%.1f hello=%.1f telemetry=%.1f\n", i, m["first_frame"], m["hello"], m["telemetry"] }' "$log"
  rm -f "$log"
done
pkill -TERM -f "$RIGG_BIN/agentic-riggd" 2>/dev/null
