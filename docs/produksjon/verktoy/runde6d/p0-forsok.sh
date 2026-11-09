#!/usr/bin/env bash
# P0-forsøk: A = tjenesten alene (ingen Shell, ingen øktbuss), B = tjenesten i
# privat økt uten vindu, C = bare den private økta (Shell) uten tjeneste.
# Logger nvidia-smi (bare lesing) hvert 1000 ms. Bruk: eksp.sh A|B|C SEK [MAPPE]
set -u
mode=$1 secs=$2
out=${3:-$(dirname "$0")}
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/release}
app="/run/media/julian/Transcend-128GB/Claude Code Prosjekter/ubuntu-desktop applikasjoner/agentic-rigg/app"
log=$out/$mode-$(date +%H%M%S).csv
avail=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
[ "$avail" -ge 6144 ] || { echo "Bare $avail MB ledig" >&2; exit 3; }
nvidia-smi --query-gpu=timestamp,pstate,clocks.gr,power.draw,utilization.gpu --format=csv,noheader -lms 1000 >"$log" 2>&1 &
smi=$!
sleep 15
mark() { echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- $* ---" >>"$log"; }
case "$mode" in
A)
    base=$(mktemp -d "$HOME/.cache/rigg-build/okt-6d-A.XXXXXX")
    mkdir -p "$base/config/agentic-rigg-dev" "$base/state" "$base/run"; chmod 700 "$base/run"
    echo '{"background": false, "backgroundIntervalMs": 2000}' >"$base/config/agentic-rigg-dev/tjeneste.json"
    mark "tjenesten starter (A)"
    env -u DBUS_SESSION_BUS_ADDRESS RIGG_PROFIL=dev XDG_CONFIG_HOME="$base/config" XDG_STATE_HOME="$base/state" \
        XDG_RUNTIME_DIR="$base/run" ${RIGG_STRACE:+strace -f -tt -e trace=openat,ioctl -o "$out/A-strace.txt"} "$bin/agentic-riggd" --grace-ms 900000 2>"$out/A-riggd.log" &
    d=$!
    sleep "$secs"
    mark "tjenesten stopper"
    kill -TERM $d; wait $d 2>/dev/null
    rm -rf -- "$base"
    ;;
B | C)
    runner=$(mktemp "$out/runner.XXXX.sh")
    cat >"$runner" <<'RUN'
set -u
L=$RIGG_P0_LOG
echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- Shell klar ---" >>"$L"
if [ "$RIGG_P0_MODE" = B ]; then
    sleep 20
    echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- tjenesten starter (B) ---" >>"$L"
    "$RIGG_BIN/agentic-riggd" --grace-ms 900000 2>"$OKT_BASE/riggd.log" &
    d=$!
    sleep "$RIGG_P0_SECS"
    echo "$(date '+%Y/%m/%d %H:%M:%S.000'), --- tjenesten stopper ---" >>"$L"
    kill -TERM $d; wait $d 2>/dev/null
else
    sleep $((RIGG_P0_SECS + 20))
fi
RUN
    mark "økta starter ($mode)"
    RIGG_BIN="$bin" RIGG_PROFIL=dev RIGG_P0_LOG="$log" RIGG_P0_MODE="$mode" RIGG_P0_SECS="$secs" \
        RIGG_OKT_DIR="$HOME/.cache/rigg-build/okt-6d-$mode" "$app/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner"
    rm -f -- "$runner"
    mark "økta er ferdig"
    ;;
esac
sleep 10
kill $smi; wait $smi 2>/dev/null
echo "$log"
awk -F', ' '/---/ {print; next} $2!="P8" {print}' "$log"
