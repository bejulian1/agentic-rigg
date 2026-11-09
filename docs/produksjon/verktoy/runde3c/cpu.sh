# CPU i ro per side og tegner. Env: RIGG_BIN, PAGES, RENDERERS, REPS, SECS
set -u
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 70   # sparklinjer og 1 min-grafer fulle
for r in $RENDERERS; do
  for p in $PAGES; do
    GSK_RENDERER=$r RIGG_GL_FORVARM=0 "$RIGG_BIN/agentic-rigg" --side "$p" 2>"$OKT_BASE/w.log" &
    w=$!
    sleep ${WARM:-10}
    vals=""; dv=""
    wp=""; : > "$OKT_BASE/watch.txt"
    [ -n "${WATCH:-}" ] && { "${NVT:-$HOME/.cache/rigg-build/nvml_timing}" watch 2000 $(( REPS * SECS )) > "$OKT_BASE/watch.txt" 2>&1 & wp=$!; }
    for i in $(seq 1 "$REPS"); do
      a=$(ticks $w); b0=$(ticks $d); sleep "$SECS"; b=$(ticks $w); b1=$(ticks $d)
      vals="$vals $(awk -v a=$a -v b=$b -v hz=$hz -v s=$SECS 'BEGIN{printf "%.2f",(b-a)/hz/s*100}')"
      dv="$dv $(awk -v a=$b0 -v b=$b1 -v hz=$hz -v s=$SECS 'BEGIN{printf "%.2f",(b-a)/hz/s*100}')"
    done
    [ -n "$wp" ] && wait $wp 2>/dev/null
    [ -z "${WATCH:-}" ] && echo "x P$(nvidia-smi --query-gpu=pstate --format=csv,noheader | tr -d P)" >> "$OKT_BASE/watch.txt"
    ps=$(awk 'NR>1 || /^x/ {c[$2]++} END {for (k in c) printf "%s:%d,", k, c[k]}' "$OKT_BASE/watch.txt")
    echo "RES r=$r side=$p vindu_cpu=[$vals ] tjeneste_cpu=[$dv ] $(mem $w) pstate=$ps"
    dv=""
    kill -TERM $w; wait $w 2>/dev/null; sleep 1
  done
done
kill -TERM $d; wait $d 2>/dev/null
