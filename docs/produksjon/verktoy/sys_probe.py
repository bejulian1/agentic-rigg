#!/usr/bin/env python3
"""System-sonde for Agentic Rigg – BARE LESING, uten root.

Leser /proc og /sys (CPU, minne, disk, nett, hwmon, DMI, GPU-sysfs) og måler
hva hver lesing koster. Åpner aldri /dev/i2c-*, /dev/mem, /dev/cpu/*/msr,
ee1004-eepromfiler eller andre enheter. Starter ingen programmer, bortsett
fra `lscpu` og `sensors` når --exec er gitt (bare for å måle oppstartskost).

Bruk:  python3 -I sys_probe.py [--exec]

Tidene er median over mange lesinger: «åpne+les+lukk» (slik demoen gjør) og
«pread på åpen fd» (slik Rust-appen bør gjøre for filer som leses ofte).
"""
import glob
import os
import statistics
import subprocess
import sys
import time

FORBIDDEN = ("/dev/", "eeprom", "/sys/firmware/dmi/tables")


def guard(p):
    if any(f in p for f in FORBIDDEN):
        raise RuntimeError(f"Avvist sti: {p}")


def read_once(p):
    guard(p)
    fd = os.open(p, os.O_RDONLY)
    try:
        return os.read(fd, 1 << 16)
    finally:
        os.close(fd)


def cost_open(p, n=200):
    t = []
    for _ in range(n):
        a = time.perf_counter_ns()
        try:
            read_once(p)
        except OSError:
            return None
        t.append((time.perf_counter_ns() - a) / 1000)
    return statistics.median(t)


def cost_pread(p, n=200):
    guard(p)
    try:
        fd = os.open(p, os.O_RDONLY)
    except OSError:
        return None
    t = []
    try:
        for _ in range(n):
            a = time.perf_counter_ns()
            os.pread(fd, 1 << 16, 0)
            t.append((time.perf_counter_ns() - a) / 1000)
    except OSError:
        return None
    finally:
        os.close(fd)
    return statistics.median(t)


def val(p, maxlen=120):
    try:
        v = read_once(p).decode(errors="replace").strip()
        return v if len(v) <= maxlen else v[:maxlen] + f"… ({len(v)} tegn)"
    except PermissionError:
        return "EACCES (krever root)"
    except FileNotFoundError:
        return "finnes ikke"
    except OSError as e:
        return f"feil: {e.strerror}"


def row(label, p, n=200):
    o = cost_open(p, n)
    r = cost_pread(p, n)
    print(f"{label:<34} {(f'{o:.1f}' if o else '-'):>8} {(f'{r:.1f}' if r else '-'):>8}  {val(p)}")


def group_cost(label, paths, n=100):
    """Kost for å lese alle stiene én gang (åpen fd + pread), median."""
    fds = []
    for p in paths:
        guard(p)
        try:
            fds.append(os.open(p, os.O_RDONLY))
        except OSError:
            pass
    t = []
    for _ in range(n):
        a = time.perf_counter_ns()
        for fd in fds:
            os.pread(fd, 4096, 0)
        t.append((time.perf_counter_ns() - a) / 1000)
    for fd in fds:
        os.close(fd)
    print(f"{label:<34} {'':>8} {statistics.median(t):>8.1f}  ({len(fds)} filer samlet)")


print(f"{'Kilde':<34} {'åpne µs':>8} {'pread µs':>8}  Verdi")
print("-- CPU")
C = "/sys/devices/system/cpu"
row("/proc/stat", "/proc/stat")
row("/proc/cpuinfo", "/proc/cpuinfo", 30)
row("cpu0 scaling_cur_freq (kHz)", f"{C}/cpu0/cpufreq/scaling_cur_freq")
row("cpu0 cpuinfo_avg_freq (kHz)", f"{C}/cpu0/cpufreq/cpuinfo_avg_freq")
row("cpu0 cpuinfo_cur_freq", f"{C}/cpu0/cpufreq/cpuinfo_cur_freq", 5)
row("cpu0 scaling_governor", f"{C}/cpu0/cpufreq/scaling_governor")
row("cpu0 scaling_driver", f"{C}/cpu0/cpufreq/scaling_driver")
row("cpu0 available_frequencies", f"{C}/cpu0/cpufreq/scaling_available_frequencies")
row("cpu0 stats/time_in_state", f"{C}/cpu0/cpufreq/stats/time_in_state")
row("cpufreq/boost", f"{C}/cpufreq/boost")
row("cpu0 cpb", f"{C}/cpu0/cpufreq/cpb")
row("cpu0 bios_limit", f"{C}/cpu0/cpufreq/bios_limit")
row("cpu0 microcode/version", f"{C}/cpu0/microcode/version")
row("cpu0 topology/thread_siblings", f"{C}/cpu0/topology/thread_siblings_list")
row("cpu0 topology/core_id", f"{C}/cpu0/topology/core_id")
row("cpu0 cache index3 shared", f"{C}/cpu0/cache/index3/shared_cpu_list")
row("cpu0 cpuidle state2 time (µs)", f"{C}/cpu0/cpuidle/state2/time")
row("smt/active", f"{C}/smt/active")
n = os.cpu_count()
group_cost("16× scaling_cur_freq", [f"{C}/cpu{i}/cpufreq/scaling_cur_freq" for i in range(n)])
group_cost("16× cpuinfo_avg_freq", [f"{C}/cpu{i}/cpufreq/cpuinfo_avg_freq" for i in range(n)])
row("powercap package energy_uj", "/sys/class/powercap/intel-rapl:0/energy_uj", 5)

print("-- Temperatur (hwmon)")
for h in sorted(glob.glob("/sys/class/hwmon/hwmon*"), key=lambda s: int(s[22:])):
    name = val(f"{h}/name")
    for f in sorted(glob.glob(f"{h}/*_input")):
        lab = val(f.replace("_input", "_label"))
        lab = "" if lab == "finnes ikke" else f" ({lab})"
        row(f"{name} {os.path.basename(f)}{lab}"[:34], f)

print("-- Minne, last, oppetid")
row("/proc/meminfo", "/proc/meminfo")
row("/proc/loadavg", "/proc/loadavg")
row("/proc/uptime", "/proc/uptime")
row("/proc/pressure/cpu", "/proc/pressure/cpu")
row("/proc/pressure/memory", "/proc/pressure/memory")
row("/proc/pressure/io", "/proc/pressure/io")

print("-- Disk")
row("/proc/diskstats", "/proc/diskstats")
for b in sorted(glob.glob("/sys/block/sd*")) + sorted(glob.glob("/sys/block/nvme*")):
    row(f"{os.path.basename(b)} stat", f"{b}/stat")
    print(f"{'':<34} {'':>8} {'':>8}  modell={val(b + '/device/model')} rotasjon={val(b + '/queue/rotational')} hwmon={bool(glob.glob(b + '/device/hwmon*'))}")

print("-- Nett")
row("/proc/net/dev", "/proc/net/dev")
row("/proc/net/wireless", "/proc/net/wireless")
for nd in sorted(glob.glob("/sys/class/net/*")):
    nm = os.path.basename(nd)
    if nm == "lo":
        continue
    row(f"{nm} rx_bytes", f"{nd}/statistics/rx_bytes")
    row(f"{nm} operstate", f"{nd}/operstate")
    row(f"{nm} speed", f"{nd}/speed", 20)

print("-- DMI (/sys/class/dmi/id)")
for f in sorted(os.listdir("/sys/class/dmi/id")):
    p = f"/sys/class/dmi/id/{f}"
    if os.path.isfile(p) and f not in ("uevent", "modalias"):
        row(f, p, 20)

print("-- GPU via sysfs/procfs (uten NVML)")
G = "/sys/bus/pci/devices/0000:1c:00.0"
for f in ("current_link_speed", "current_link_width", "max_link_speed", "max_link_width", "power_state",
          "resource1_resize", "boot_vga", "revision", "subsystem_vendor", "subsystem_device", "aer_dev_correctable"):
    row(f"gpu {f}", f"{G}/{f}", 50)
row("gpu power/runtime_status", f"{G}/power/runtime_status", 50)
for p in glob.glob("/proc/driver/nvidia/gpus/*/information"):
    row("nvidia gpus/information", p, 50)
row("/proc/driver/nvidia/version", "/proc/driver/nvidia/version", 50)

print("-- OS")
row("/etc/os-release", "/etc/os-release", 50)
row("kernel/osrelease", "/proc/sys/kernel/osrelease", 50)

if "--exec" in sys.argv:
    print("-- Eksterne programmer (oppstartskost)")
    for cmd in (["lscpu"], ["lscpu", "-J"], ["sensors", "-j"], ["nvidia-smi", "--query-gpu=temperature.gpu,power.draw", "--format=csv,noheader", "--id=0"]):
        t = []
        for _ in range(5):
            a = time.perf_counter_ns()
            subprocess.run(cmd, capture_output=True, check=False)
            t.append((time.perf_counter_ns() - a) / 1e6)
        print(f"{' '.join(cmd)[:34]:<34} {statistics.median(t):>8.1f} ms (median av 5)")
