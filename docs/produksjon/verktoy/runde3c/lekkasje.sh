# Lekkasjetest: tjenesten i MIN minutter med vinduet åpnet og lukket hvert 5. min.
# Skriver CSV til $OUT/lekkasje.csv: sekunder, Private_Dirty, Pss, Rss (kB), fd, tråder, vindu (0/1), vinduets Private_Dirty.
set -u
kb() { awk -v k="$2" '$1==k":" {print $2}' "/proc/$1/smaps_rollup"; }
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" ${DARGS:-} 2>"$OKT_BASE/riggd.log" &
d=$!
csv="$OUT/${CSV:-lekkasje.csv}"
echo "s,tjeneste_private_dirty_kb,tjeneste_pss_kb,tjeneste_rss_kb,fd,traader,vindu,vindu_private_dirty_kb,pstate" >"$csv"
t0=$(date +%s); w=0; pages=(oversikt sensorer grafer grafikkort system kontroller viftekurve lys)
n=0
while :; do
  now=$(date +%s); el=$((now - t0))
  [ $el -ge $(( MIN * 60 )) ] && break
  # Hvert 5. min: vinduet åpnes i 90 s på en side, går gjennom tre sider til, og lukkes (samme vei som ×).
  if [ $(( el % 300 )) -lt 30 ] && [ $w -eq 0 ] && [ $el -ge 60 ]; then
    p=${pages[$((n % 8))]}; n=$((n+1))
    RIGG_TIMING=1 RIGG_TIMING_LUKK=90000 "$RIGG_BIN/agentic-rigg" --side "$p" 2>>"$OKT_BASE/w.log" &
    w=$!; wstart=$now
    ( sleep 20; for q in "${pages[$((n % 8))]}" "${pages[$(((n+3) % 8))]}" "${pages[$(((n+5) % 8))]}"; do "$RIGG_BIN/agentic-rigg" --side "$q" 2>/dev/null; sleep 15; done ) &
  fi
  if [ $w -ne 0 ] && ! kill -0 $w 2>/dev/null; then wait $w 2>/dev/null; w=0; fi
  wpd=""; [ $w -ne 0 ] && wpd=$(kb $w Private_Dirty)
  ps=$(nvidia-smi --query-gpu=pstate --format=csv,noheader 2>/dev/null)
  echo "$el,$(kb $d Private_Dirty),$(kb $d Pss),$(kb $d Rss),$(ls /proc/$d/fd | wc -l),$(ls /proc/$d/task | wc -l),$([ $w -ne 0 ] && echo 1 || echo 0),$wpd,$ps" >>"$csv"
  sleep 30
done
[ $w -ne 0 ] && { kill -TERM $w; wait $w 2>/dev/null; }
echo "RES lekkasje ferdig $(wc -l <"$csv") rader"
kill -TERM $d; wait $d 2>/dev/null
