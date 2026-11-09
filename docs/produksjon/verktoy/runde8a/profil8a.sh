#!/usr/bin/env bash
# Runde 8A: hva Oversikt bruker tiden på i ro. Én privat økt (app/scripts/okt.sh):
# tjenesten mot det ekte kortet (bare lesing), vinduet på en side med
# RIGG_2A_PROFIL=1 (antall bilder og ms per bilde, oppsett og tegning, og
# grafene), og CPU for vinduet over SECS sekunder.
#
#   profil8a.sh [SIDE] [SECS]      (standard oversikt 40)
# Env: RIGG_BIN (standard ~/.cache/rigg-build/release), RIGG_EKSTRA (mer miljø til vinduet).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
app="$here/../../../../app"
side=${1:-oversikt}
secs=${2:-40}
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/release}
runner=$(mktemp)
cat >"$runner" <<'RUN'
set -u
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
"$RIGG_BIN/agentic-riggd" --grace-ms 600000 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 6
# shellcheck disable=SC2086
env RIGG_2A_PROFIL=1 $RIGG_EKSTRA "$RIGG_BIN/agentic-rigg" --side "$RIGG_SIDE" 2>"$OKT_BASE/w.log" &
w=$!
sleep 12
a=$(ticks $w); sleep "$RIGG_SECS"; b=$(ticks $w)
awk -v a="$a" -v b="$b" -v hz="$hz" -v s="$RIGG_SECS" 'BEGIN{printf "CPU vindu %.2f %%\n",(b-a)/hz/s*100}'
kill -TERM $w; wait $w 2>/dev/null
kill -TERM $d; wait $d 2>/dev/null
grep RIGG_PROFIL "$OKT_BASE/w.log" | tail -8
RUN
RIGG_BIN="$bin" RIGG_PROFIL=dev RIGG_SIDE="$side" RIGG_SECS="$secs" RIGG_EKSTRA="${RIGG_EKSTRA:-}" RIGG_OKT_DIR="$HOME/.cache/rigg-build/okt-8a-profil" OKT_WARMUP=4 \
    "$app/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner" 2>&1 | grep -E "^CPU|RIGG_PROFIL"
rm -f -- "$runner"
