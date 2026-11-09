#!/usr/bin/env bash
# Kopierer de /proc- og /sys-filene rigg-hw leser, til tests/fixtures/maskin,
# så testene kan kjøres mot denne maskinen uten å lese den. Bare lesing.
# Ingen serienumre: DMI-filene med serienummer (0400 root) tas ikke med.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
dst=$here/crates/rigg-hw/tests/fixtures/maskin
rm -rf -- "$dst"
mkdir -p "$dst"
cp_one() { # kopierer én fil som vanlig fil (ikke symlenke)
    local src=$1 rel=${1#/}
    [ -r "$src" ] || return 0
    mkdir -p "$dst/$(dirname -- "$rel")"
    cat -- "$src" >"$dst/$rel" 2>/dev/null || rm -f -- "$dst/$rel"
}
for f in /proc/stat /proc/meminfo /proc/uptime /proc/loadavg; do cp_one "$f"; done
for h in /sys/class/hwmon/hwmon*; do
    n=$(basename -- "$h")
    mkdir -p "$dst/sys/class/hwmon/$n"
    cat "$h/name" >"$dst/sys/class/hwmon/$n/name"
    for f in "$h"/*_input "$h"/*_label; do
        [ -e "$f" ] || continue
        cat -- "$f" >"$dst/sys/class/hwmon/$n/$(basename -- "$f")" 2>/dev/null || true
    done
done
for i in /sys/class/net/*; do
    n=$(basename -- "$i")
    mkdir -p "$dst/sys/class/net/$n/statistics"
    cat "$i/operstate" >"$dst/sys/class/net/$n/operstate"
    cat "$i/statistics/rx_bytes" >"$dst/sys/class/net/$n/statistics/rx_bytes"
    cat "$i/statistics/tx_bytes" >"$dst/sys/class/net/$n/statistics/tx_bytes"
    [ -d "$i/wireless" ] && mkdir -p "$dst/sys/class/net/$n/wireless"
done
for c in /sys/devices/system/cpu/cpu[0-9]*; do
    for f in cpufreq/cpuinfo_avg_freq cpufreq/scaling_cur_freq cpufreq/scaling_governor cpufreq/scaling_driver; do
        cp_one "$c/$f"
    done
done
cp_one /sys/devices/system/cpu/cpufreq/boost
pci=$(grep -l 0x10de /sys/bus/pci/devices/*/vendor | head -1 | xargs dirname)
for f in vendor device class subsystem_vendor subsystem_device revision current_link_speed current_link_width max_link_speed max_link_width; do
    cp_one "$pci/$f"
done
cp_one /sys/module/nvidia/version
echo "Kopierte til $dst"
