# Vinduets minne etter at alle sidene er besøkt. Env: RIGG_BIN, RENDERERS, RUNS
set -u
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 5
for r in $RENDERERS; do
  for i in $(seq 1 "$RUNS"); do
    GSK_RENDERER=$r "$RIGG_BIN/agentic-rigg" --side oversikt 2>"$OKT_BASE/w.log" &
    w=$!
    sleep 4
    start="$(mem $w)"
    for p in sensorer grafer grafikkort system kontroller viftekurve lys; do
      GSK_RENDERER=$r "$RIGG_BIN/agentic-rigg" --side $p 2>/dev/null
      sleep 3
    done
    sleep 4   # 3D på Lys bygges
    lys="$(mem $w)"
    GSK_RENDERER=$r "$RIGG_BIN/agentic-rigg" --innstillinger generelt 2>/dev/null; sleep 2
    GSK_RENDERER=$r "$RIGG_BIN/agentic-rigg" --side oversikt 2>/dev/null; sleep 10
    echo "RES r=$r run=$i start: $start | lys: $lys | alle_sider_10s: $(mem $w) | tjeneste: $(mem $d)"
    kill -TERM $w; wait $w 2>/dev/null; sleep 1
  done
done
kill -TERM $d; wait $d 2>/dev/null
