#!/usr/bin/env bash
# Måler kald start til første bilde, minne og CPU i ro (arkitektur.md kap. 10),
# i en privat, hodeløs GNOME-økt (scripts/okt.sh). Måler bare prosesser
# skriptet har startet selv. Tjenesten leser det ekte kortet (NVML, bare
# lesing, 1–2 Hz).
#
# Bruk: scripts/maal.sh [ANTALL_STARTER] [CPU_SEKUNDER]   (standard 10 og 60)
# Binærene: $RIGG_BIN, ellers $CARGO_TARGET_DIR/release.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
runs=${1:-10}
secs=${2:-60}
bin=${RIGG_BIN:-${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}/release}
[ -x "$bin/agentic-rigg" ] && [ -x "$bin/agentic-riggd" ] || { echo "Fant ikke release-binærene i $bin" >&2; exit 1; }
out=${RIGG_MAAL_UT:-$here/../docs/produksjon/maal/siste.txt}
runner=$(mktemp)
cat >"$runner" <<'RUN'
set -u
hz=$(getconf CLK_TCK)
cpu_ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%d ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
cpu_pct() { # pid sekunder
    local a b; a=$(cpu_ticks "$1"); sleep "$2"; b=$(cpu_ticks "$1")
    awk -v a="$a" -v b="$b" -v hz="$hz" -v s="$2" 'BEGIN { printf "%.2f", (b - a) / hz / s * 100 }'
}
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
# «Fortsett i bakgrunnen» på, 2 s, så tjenesten lever uten vindu (målet i kap. 9).
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 4
echo "uname: $(uname -r) · $(nproc) tråder"
echo "tjeneste_pid=$d"
echo "tjeneste_alene_minne: $(mem $d)"
echo "tjeneste_alene_cpu_${RIGG_SECS}s_pct=$(cpu_pct $d "$RIGG_SECS")"
# Kald start og lukking (runde 3C): veggklokka tas rett før exec, og vinduet
# skriver sine merker med veggklokka (RIGG_TIMING). RIGG_TIMING_LUKK lukker
# vinduet samme vei som ×-knappen etter 800 ms, og skriptet venter til
# prosessen er borte. Alle tall er ms fra exec.
for i in $(seq 1 "$RIGG_RUNS"); do
    log=$(mktemp)
    t0=${EPOCHREALTIME/[.,]/}
    RIGG_TIMING=1 RIGG_TIMING_LUKK=800 "$RIGG_BIN/agentic-rigg" 2>"$log"
    t1=${EPOCHREALTIME/[.,]/}
    awk -v t0="$t0" -v t1="$t1" -v i="$i" '
      /RIGG_TIMING mark=/ { split($2,a,"="); split($4,b,"="); m[a[2]]=(b[2]/1000 - t0)/1000 }
      /renderer=/ { for (k=1;k<=NF;k++) if ($k ~ /^renderer=/) r=substr($k,10) }
      END { printf "start_%s: renderer=%s main=%.1f command_line=%.1f page=%.1f realize=%.1f first_frame=%.1f hello=%.1f telemetry=%.1f lukk_til_skjult=%.1f lukk_til_avsluttet=%.1f\n",
            i, r, m["main"], m["command_line"], m["page"], m["realize"], m["first_frame"], m["hello"], m["telemetry"], m["hidden"]-m["close"], (t1-t0)/1000 - m["close"] }' "$log"
    rm -f "$log"
    sleep 0.7
done
RIGG_TIMING=1 "$RIGG_BIN/agentic-rigg" 2>"$OKT_BASE/rigg.log" &
w=$!
sleep 10
echo "vindu_minne_10s: $(mem $w)"
echo "tjeneste_minne_med_vindu: $(mem $d)"
echo "vindu_cpu_oversikt_${RIGG_SECS}s_pct=$(cpu_pct $w "$RIGG_SECS")"
echo "tjeneste_cpu_med_vindu_${RIGG_SECS}s_pct=$(cpu_pct $d "$RIGG_SECS")"
echo "vindu_minne_slutt: $(mem $w)"
kill -TERM "$w" 2>/dev/null; wait "$w" 2>/dev/null
sleep 2
echo "tjeneste_cpu_bakgrunn_etter_vindu_${RIGG_SECS}s_pct=$(cpu_pct $d "$RIGG_SECS")"
echo "tjeneste_minne_slutt: $(mem $d)"
kill -TERM "$d" 2>/dev/null; wait "$d" 2>/dev/null
grep -E "CSS|CRITICAL" "$OKT_BASE/rigg.log" | head -5
RUN
RIGG_BIN=$bin OKT_WARMUP=4 "$here/scripts/okt.sh" --bredde 1920x1080 -- env RIGG_BIN="$bin" RIGG_RUNS="$runs" RIGG_SECS="$secs" bash "$runner" | tee "$out"
rm -f -- "$runner"
echo "Resultatet står i $out"
