set -u
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep ${PRE:-65}
for r in $RENDERERS; do
for p in $PAGES; do
  [ -n "${XUI:-}" ] && echo "$XUI" > "$XDG_CONFIG_HOME/agentic-rigg-dev/ui.json"
  env GSK_RENDERER=$r RIGG_RAMMER=1 ${XENV:-} "$RIGG_BIN/agentic-rigg" --side "$p" 2>"$OKT_BASE/w.log" &
  w=$!
  sleep 8
  a=$(ticks $w); sleep 20; b=$(ticks $w)
  echo "RES r=$r side=$p cpu=$(awk -v a=$a -v b=$b -v hz=$hz 'BEGIN{printf "%.2f",(b-a)/hz/20*100}') $(grep -E "RIGG_RAMMER" "$OKT_BASE/w.log" | tail -1 | tr '\n' ' ')"
  kill -TERM $w; wait $w 2>/dev/null; sleep 1
done
done
kill -TERM $d; wait $d 2>/dev/null
