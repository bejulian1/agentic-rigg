# Varm start: vinduet finnes, en ny start sender kommandolinja til det og avslutter.
set -u
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 3
"$RIGG_BIN/agentic-rigg" 2>/dev/null &
w=$!
sleep 4
for i in $(seq 1 "$RUNS"); do
  p=$([ $((i % 2)) -eq 0 ] && echo oversikt || echo sensorer)
  t0=${EPOCHREALTIME/[.,]/}
  "$RIGG_BIN/agentic-rigg" --side $p 2>/dev/null
  t1=${EPOCHREALTIME/[.,]/}
  echo "RES varm run=$i side=$p ms=$(( (t1 - t0) / 1000 ))"
  sleep 1
done
kill -TERM $w; wait $w 2>/dev/null
kill -TERM $d; wait $d 2>/dev/null
