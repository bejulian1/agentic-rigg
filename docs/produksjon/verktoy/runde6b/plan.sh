# Runde 6B: kjører en skjermbildeplan fra skjermbilder.rs (RIGG_PLAN) mot en
# valgt backend i den private økta (okt.sh eller okt6b.sh), som bilder.sh gjør,
# men med sim-scenariene for maskinhodet.
# Bruk: RIGG_BIN=… RIGG_SKJERMBILDER=MAPPE RIGG_PLAN=6b|6b-tilstand [RIGG_6B_BACKEND=sim:borte] bash plan.sh
set -u
mkdir -p "$RIGG_SKJERMBILDER"
"$RIGG_BIN/agentic-riggd" --grace-ms 600000 --backend "${RIGG_6B_BACKEND:-sim}" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep "${RIGG_WARM:-3}"
timeout 600 "$RIGG_BIN/agentic-rigg" 2>"$OKT_BASE/rigg.log"
rc=$?
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
grep -E "CSS|CRITICAL|WARNING|RIGG_SKJERMBILDE kuttet|fant ikke" "$OKT_BASE/rigg.log" | head -40 >&2
exit $rc
