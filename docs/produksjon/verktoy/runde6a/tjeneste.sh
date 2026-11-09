#!/usr/bin/env bash
# Runde 6A: tjenesten (release) med og uten diskene, mot den ekte UDisks2 på
# systembussen (derfor ikke i den private økta, der systembussen er tom).
# Egen HOME-del for tjenesten (XDG_* i en mappe under ~/.cache/rigg-build),
# RIGG_PROFIL=dev, ingen øktbuss (ingen varsler, intet systemfelt), og egen
# socket. Brukerens tjeneste og ~/.local røres ikke. Bare lesing.
#
#   tjeneste.sh [SEKUNDER]   (standard 90)
# Binærene: RIGG_BIN (standard ~/.cache/rigg-build/6a-rel).
set -u
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bin=${RIGG_BIN:-$HOME/.cache/rigg-build/6a-rel}
secs=${1:-90}
hz=$(getconf CLK_TCK)
ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
mem() { awk '/^(Rss|Pss|Private_Dirty):/ {printf "%s=%.1f ", $1, $2/1024}' "/proc/$1/smaps_rollup"; }
avail=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
[ "$avail" -ge 6144 ] || { echo "Bare $avail MB ledig, venter" >&2; exit 3; }
base=$(mktemp -d "$HOME/.cache/rigg-build/6a-maal.XXXXXX")
trap 'rm -rf -- "$base"' EXIT
run() { # navn disker klient(ja/nei)
    local name=$1 disks=$2 client=$3 dir=$base/$1
    mkdir -p "$dir/config/agentic-rigg-dev" "$dir/state" "$dir/run"
    chmod 700 "$dir/run"
    echo '{"background": false, "backgroundIntervalMs": 2000}' >"$dir/config/agentic-rigg-dev/tjeneste.json"
    env -u DBUS_SESSION_BUS_ADDRESS RIGG_PROFIL=dev RIGG_DISKER="$disks" XDG_CONFIG_HOME="$dir/config" XDG_STATE_HOME="$dir/state" \
        XDG_RUNTIME_DIR="$dir/run" "$bin/agentic-riggd" --grace-ms 900000 2>"$dir/riggd.log" &
    local d=$! k=
    sleep 6
    if [ "$client" = ja ]; then
        python3 -I "$here/klient.py" "$dir/run/agentic-rigg/riggd-dev.sock" $((secs + 15)) &
        k=$!
    fi
    sleep 8
    local a b
    a=$(ticks $d); sleep "$secs"; b=$(ticks $d)
    echo "RES $name disker=$disks vindu=$client cpu=$(awk -v a=$a -v b=$b -v hz=$hz -v s=$secs 'BEGIN{printf "%.2f",(b-a)/hz/s*100}')% $(mem $d) tråder=$(ls /proc/$d/task | wc -l)"
    [ -n "$k" ] && kill $k 2>/dev/null
    kill -TERM $d; wait $d 2>/dev/null
    [ -n "$k" ] && wait $k 2>/dev/null
}
echo "# $(date -Is) · $(uname -r) · last $(cut -d' ' -f1-3 /proc/loadavg) · ${secs} s per måling"
run bakgrunn-uten ingen nei
run bakgrunn-disker ekte nei
run vindu-uten ingen ja
run vindu-disker ekte ja
