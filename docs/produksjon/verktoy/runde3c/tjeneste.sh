# Tjenesten alene i bakgrunnen (2 s), REPS × SECS, med P-tilstand fra nvml_timing watch.
set -u
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo "{\"background\": true, \"backgroundIntervalMs\": ${BG_MS:-2000}}" >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 20
if [ -n "${WATCH:-}" ]; then "${NVT:-$HOME/.cache/rigg-build/nvml_timing}" watch 2000 $(( REPS * SECS + 5 )) > "$OKT_BASE/watch.txt" 2>&1 & wp=$!; fi
vals=""
for i in $(seq 1 "$REPS"); do
  a=$(ticks $d); sleep "$SECS"; b=$(ticks $d)
  vals="$vals $(awk -v a=$a -v b=$b -v hz=$hz -v s=$SECS 'BEGIN{printf "%.2f",(b-a)/hz/s*100}')"
done
echo "RES tag=${TAG:-x} bg_ms=${BG_MS:-2000} tjeneste_cpu=[$vals ] $(mem $d) fd=$(ls /proc/$d/fd | wc -l) tråder=$(ls /proc/$d/task | wc -l)"
[ -n "${WATCH:-}" ] && { wait $wp 2>/dev/null; awk 'NR>1 {c[$2]++} END {printf "RES watch_pstates:"; for (k in c) printf " %s=%d", k, c[k]; printf "\n"}' "$OKT_BASE/watch.txt"; }
kill -TERM $d; wait $d 2>/dev/null
