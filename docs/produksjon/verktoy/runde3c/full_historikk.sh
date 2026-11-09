# Tjenestens minne med full historikk (48 t, syntetisk i sim) mot tom historikk (sim), og NVML alene.
set -u
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
for mode in tom full; do
  rm -rf "$XDG_STATE_HOME"/agentic-rigg* "$XDG_DATA_HOME"/agentic-rigg* "$XDG_CACHE_HOME"/agentic-rigg* 2>/dev/null
  extra=""; [ $mode = full ] && extra=--syntetisk-historikk
  "$RIGG_BIN/agentic-riggd" --backend sim $extra --grace-ms 600000 2>"$OKT_BASE/riggd.log" &
  d=$!; sleep 20
  echo "RES sim_$mode $(mem $d)"
  kill -TERM $d; wait $d 2>/dev/null
done
