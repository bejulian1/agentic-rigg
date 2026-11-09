#!/usr/bin/env bash
# Kjører en kommando i en privat, hodeløs GNOME-økt: egen HOME, egne
# XDG-mapper, egen D-Bus-økt og en egen `gnome-shell --headless` med egen
# WAYLAND_DISPLAY. Brukerens skrivebord, økt-buss og innstillinger røres ikke.
#
# Bruk: scripts/okt.sh [--bredde 1440x900] -- KOMMANDO …
# Inne i kommandoen er WAYLAND_DISPLAY, HOME og XDG_* satt til økta, og alle
# RIGG_*- og GSK_*-variabler fra kalleren er med.
# Mappa er ny for hver kjøring (~/.cache/rigg-build/okt.XXXXXX på
# systemdisken), så agenter som kjører samtidig, ikke sletter hverandres
# økter. RIGG_OKT_DIR velger en fast mappe (den tømmes først). Mappa slettes
# etterpå, med mindre RIGG_OKT_KEEP er satt.
#
# Systembussen er også privat: en egen dbus-daemon uten tjenester. Da melder
# den hodeløse Shell seg ikke til GDM (docs/produksjon/ikon.md 7.1), og
# ingenting når logind, NetworkManager eller andre systemtjenester.
set -euo pipefail
size=1440x900
while [ $# -gt 0 ]; do
    case "$1" in
    --bredde) size=$2; shift 2 ;;
    --) shift; break ;;
    *) break ;;
    esac
done
[ $# -gt 0 ] || { echo "Bruk: $0 [--bredde BxH] -- KOMMANDO" >&2; exit 2; }
avail=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
if [ "$avail" -lt 6144 ]; then
    echo "Bare ${avail} MB ledig minne (krav 6 GB). Venter med økta." >&2
    exit 3
fi
if [ -n "${RIGG_OKT_DIR:-}" ]; then
    base=$RIGG_OKT_DIR
    rm -rf -- "$base"
    mkdir -p "$base"
else
    mkdir -p "$HOME/.cache/rigg-build"
    base=$(mktemp -d "$HOME/.cache/rigg-build/okt.XXXXXX")
fi
mkdir -p "$base"/{home,run,config,data,state,cache}
chmod 700 "$base/run"
display=rigg-okt-$$
inner=$(mktemp "$base/inner.XXXX.sh")
cat >"$inner" <<'INNER'
set -u
# Privat «systembuss» uten tjenester.
sysbus=$(dbus-daemon --session --fork --print-address=1 --print-pid=1 --nopidfile | head -2)
export DBUS_SYSTEM_BUS_ADDRESS=$(echo "$sysbus" | sed -n 1p)
sysbus_pid=$(echo "$sysbus" | sed -n 2p)
gnome-shell --headless --wayland --no-x11 --virtual-monitor "$OKT_SIZE" --wayland-display "$WAYLAND_DISPLAY" >"$OKT_BASE/shell.log" 2>&1 &
shell=$!
for _ in $(seq 1 200); do
    [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && break
    sleep 0.05
done
if [ ! -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]; then
    echo "gnome-shell startet ikke (se $OKT_BASE/shell.log)" >&2
    kill "$shell" "$sysbus_pid" 2>/dev/null
    exit 4
fi
# Vent til Shell er klar (oppstarten tar et par sekunder i hodeløs modus).
sleep "${OKT_WARMUP:-3}"
"$@"
rc=$?
kill "$shell" 2>/dev/null
wait "$shell" 2>/dev/null
kill "$sysbus_pid" 2>/dev/null
exit $rc
INNER
# Alle RIGG_*- og GSK_*-variabler og RUST_BACKTRACE følger med inn (planene for skjermbildene,
# backenden, tegneren). Resten av kallerens miljø blir igjen ute.
pass=()
while IFS='=' read -r k _; do
    case "$k" in
    RIGG_OKT_*) ;;
    RIGG_* | GSK_* | RUST_BACKTRACE) pass+=("$k=${!k}") ;;
    esac
done < <(env)
set +e
env -i \
    PATH=/usr/local/bin:/usr/bin:/bin \
    HOME="$base/home" \
    XDG_RUNTIME_DIR="$base/run" \
    XDG_CONFIG_HOME="$base/config" \
    XDG_DATA_HOME="$base/data" \
    XDG_STATE_HOME="$base/state" \
    XDG_CACHE_HOME="$base/cache" \
    XDG_SESSION_TYPE=wayland \
    XDG_CURRENT_DESKTOP=ubuntu:GNOME \
    WAYLAND_DISPLAY="$display" \
    LANG="${LANG:-nb_NO.UTF-8}" \
    OKT_BASE="$base" OKT_SIZE="$size" OKT_WARMUP="${OKT_WARMUP:-3}" \
    "${pass[@]}" \
    RIGG_PROFIL=dev \
    dbus-run-session -- bash "$inner" "$@"
rc=$?
# Dokumentportalen (FUSE i $XDG_RUNTIME_DIR/doc) kan henge igjen et øyeblikk
# etter at økta er borte. Den er vår egen; løs den ut før mappa slettes.
for _ in 1 2 3 4 5 6 7 8 9 10; do
    mountpoint -q "$base/run/doc" 2>/dev/null || break
    fusermount3 -u "$base/run/doc" 2>/dev/null || fusermount -u "$base/run/doc" 2>/dev/null || true
    sleep 0.3
done
if [ -z "${RIGG_OKT_KEEP:-}" ]; then
    rm -rf -- "$base" 2>/dev/null || { sleep 1; rm -rf -- "$base" || true; }
fi
exit $rc
