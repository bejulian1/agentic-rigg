#!/usr/bin/env python3
"""Simulert prosessliste til README-bildet av Oppgaver.

Lager en falsk rot (`--root`) som `agentic-riggd --root` leser i stedet for
maskinens egen: /proc med oppdiktede prosesser og brukere, /etc/passwd,
/proc/stat og /proc/cpuinfo, og resten av sys-filene fra testfixturen
(crates/rigg-hw/tests/fixtures/maskin). Ingen prosessnavn, brukernavn eller
stier fra verten kommer med. CPU-tidene øker hvert sekund, så CPU-kolonnen
har tall. Kjører til den får SIGTERM.

Bruk: scripts/readme-prosesser.py --root MAPPE
"""
import argparse
import os
import random
import shutil
import signal
import sys
import time
from pathlib import Path

HZ = os.sysconf("SC_CLK_TCK")
PAGE = os.sysconf("SC_PAGESIZE")
THREADS = 16
CPU_NAME = "AMD Ryzen 7 2700X Eight-Core Processor"

# (navn, bruker, foreldrens indeks i APPS, CPU % (av én logisk CPU), RSS MiB, tråder, exe, kommandolinje, enhet)
USER = "alex"
APPS = [
    ("systemd", "root", 0, 0.1, 14, 1, "/usr/lib/systemd/systemd", "/sbin/init splash", "init.scope"),
    ("systemd-journal", "root", 0, 0.1, 32, 1, "/usr/lib/systemd/systemd-journald", "/usr/lib/systemd/systemd-journald", "systemd-journald.service"),
    ("systemd-udevd", "root", 0, 0.0, 9, 1, "/usr/lib/systemd/systemd-udevd", "/usr/lib/systemd/systemd-udevd", "systemd-udevd.service"),
    ("systemd-resolve", "systemd-resolve", 0, 0.0, 13, 1, "/usr/lib/systemd/systemd-resolved", "/usr/lib/systemd/systemd-resolved", "systemd-resolved.service"),
    ("systemd-logind", "root", 0, 0.0, 8, 1, "/usr/lib/systemd/systemd-logind", "/usr/lib/systemd/systemd-logind", "systemd-logind.service"),
    ("NetworkManager", "root", 0, 0.1, 21, 4, "/usr/sbin/NetworkManager", "/usr/sbin/NetworkManager --no-daemon", "NetworkManager.service"),
    ("polkitd", "polkitd", 0, 0.0, 11, 4, "/usr/lib/polkit-1/polkitd", "/usr/lib/polkit-1/polkitd --no-debug", "polkit.service"),
    ("udisksd", "root", 0, 0.0, 15, 6, "/usr/libexec/udisks2/udisksd", "/usr/libexec/udisks2/udisksd", "udisks2.service"),
    ("avahi-daemon", "avahi", 0, 0.0, 4, 1, "/usr/sbin/avahi-daemon", "avahi-daemon: running [rigg-sim.local]", "avahi-daemon.service"),
    ("cupsd", "root", 0, 0.0, 12, 1, "/usr/sbin/cupsd", "/usr/sbin/cupsd -l", "cups.service"),
    ("gdm3", "root", 0, 0.0, 10, 4, "/usr/sbin/gdm3", "/usr/sbin/gdm3", "gdm.service"),
    ("systemd", USER, 0, 0.0, 12, 1, "/usr/lib/systemd/systemd", "/usr/lib/systemd/systemd --user", "init.scope"),
    ("gnome-shell", USER, 11, 4.6, 412, 24, "/usr/bin/gnome-shell", "/usr/bin/gnome-shell", "org.gnome.Shell@wayland.service"),
    ("Xwayland", USER, 12, 0.4, 96, 6, "/usr/bin/Xwayland", "/usr/bin/Xwayland :0 -rootless -noreset", "org.gnome.Shell@wayland.service"),
    ("pipewire", USER, 11, 0.6, 22, 3, "/usr/bin/pipewire", "/usr/bin/pipewire", "pipewire.service"),
    ("pipewire-pulse", USER, 11, 0.3, 19, 3, "/usr/bin/pipewire-pulse", "/usr/bin/pipewire-pulse", "pipewire-pulse.service"),
    ("wireplumber", USER, 11, 0.1, 31, 5, "/usr/bin/wireplumber", "/usr/bin/wireplumber", "wireplumber.service"),
    ("gsd-power", USER, 11, 0.0, 27, 4, "/usr/libexec/gsd-power", "/usr/libexec/gsd-power", "org.gnome.SettingsDaemon.Power.service"),
    ("gsd-media-keys", USER, 11, 0.0, 29, 4, "/usr/libexec/gsd-media-keys", "/usr/libexec/gsd-media-keys", "org.gnome.SettingsDaemon.MediaKeys.service"),
    ("ibus-daemon", USER, 11, 0.1, 15, 3, "/usr/bin/ibus-daemon", "/usr/bin/ibus-daemon --panel disable", "org.freedesktop.IBus.session.GNOME.service"),
    ("nautilus", USER, 11, 0.2, 168, 9, "/usr/bin/nautilus", "/usr/bin/nautilus --gapplication-service", "app-gnome-org.gnome.Nautilus-2210.scope"),
    ("gnome-terminal-", USER, 11, 0.5, 74, 5, "/usr/libexec/gnome-terminal-server", "/usr/libexec/gnome-terminal-server", "app-gnome-org.gnome.Terminal-2311.scope"),
    ("bash", USER, 21, 0.0, 5, 1, "/usr/bin/bash", "bash", "vte-spawn-a1.scope"),
    ("bash", USER, 21, 0.0, 5, 1, "/usr/bin/bash", "bash", "vte-spawn-b2.scope"),
    ("cargo", USER, 22, 0.3, 88, 6, "/usr/bin/cargo", "cargo build --release", "vte-spawn-a1.scope"),
    ("rustc", USER, 24, 98.0, 1210, 9, "/usr/bin/rustc", "rustc --crate-name sensor_core --edition=2024 src/lib.rs --crate-type lib -C opt-level=3", "vte-spawn-a1.scope"),
    ("rustc", USER, 24, 94.0, 860, 8, "/usr/bin/rustc", "rustc --crate-name sensor_gui --edition=2024 src/main.rs --crate-type bin -C opt-level=3", "vte-spawn-a1.scope"),
    ("rustc", USER, 24, 71.0, 540, 7, "/usr/bin/rustc", "rustc --crate-name sensor_daemon --edition=2024 src/main.rs --crate-type bin -C opt-level=3", "vte-spawn-a1.scope"),
    ("firefox", USER, 11, 6.2, 520, 92, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox", "app-gnome-firefox-2405.scope"),
    ("Isolated Web Co", USER, 28, 3.8, 310, 28, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc -isForBrowser 7 tab", "app-gnome-firefox-2405.scope"),
    ("Isolated Web Co", USER, 28, 1.9, 214, 27, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc -isForBrowser 9 tab", "app-gnome-firefox-2405.scope"),
    ("Isolated Web Co", USER, 28, 0.7, 176, 25, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc -isForBrowser 12 tab", "app-gnome-firefox-2405.scope"),
    ("Isolated Web Co", USER, 28, 0.2, 132, 24, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc -isForBrowser 15 tab", "app-gnome-firefox-2405.scope"),
    ("WebExtensions", USER, 28, 0.1, 118, 23, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc -isForBrowser 3 tab", "app-gnome-firefox-2405.scope"),
    ("RDD Process", USER, 28, 0.4, 61, 9, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc 2 rdd", "app-gnome-firefox-2405.scope"),
    ("Socket Process", USER, 28, 0.1, 44, 7, "/usr/lib/firefox/firefox", "/usr/lib/firefox/firefox -contentproc 4 socket", "app-gnome-firefox-2405.scope"),
    ("code", USER, 11, 2.4, 286, 31, "/usr/share/code/code", "/usr/share/code/code --unity-launch", "app-gnome-code-2618.scope"),
    ("code", USER, 36, 1.6, 402, 19, "/usr/share/code/code", "/usr/share/code/code --type=renderer --enable-crash-reporter", "app-gnome-code-2618.scope"),
    ("code", USER, 36, 0.9, 188, 14, "/usr/share/code/code", "/usr/share/code/code --type=gpu-process", "app-gnome-code-2618.scope"),
    ("code", USER, 36, 0.6, 236, 12, "/usr/share/code/code", "/usr/share/code/code --type=utility --utility-sub-type=node.mojom.NodeService", "app-gnome-code-2618.scope"),
    ("code", USER, 36, 0.3, 97, 8, "/usr/share/code/code", "/usr/share/code/code --type=utility --utility-sub-type=network.mojom.NetworkService", "app-gnome-code-2618.scope"),
    ("rust-analyzer", USER, 39, 3.1, 1630, 18, "/usr/bin/rust-analyzer", "rust-analyzer", "app-gnome-code-2618.scope"),
    ("steam", USER, 11, 0.8, 312, 64, "/usr/lib/steam/steam", "/usr/lib/steam/steam -silent", "app-gnome-steam-2701.scope"),
    ("steamwebhelper", USER, 42, 0.5, 190, 22, "/usr/lib/steam/steamwebhelper", "steamwebhelper -lang=en_US", "app-gnome-steam-2701.scope"),
    ("steamwebhelper", USER, 43, 0.2, 145, 14, "/usr/lib/steam/steamwebhelper", "steamwebhelper --type=gpu-process", "app-gnome-steam-2701.scope"),
    ("steamwebhelper", USER, 43, 0.1, 88, 9, "/usr/lib/steam/steamwebhelper", "steamwebhelper --type=utility", "app-gnome-steam-2701.scope"),
    ("thunderbird", USER, 11, 0.4, 342, 61, "/usr/lib/thunderbird/thunderbird", "/usr/lib/thunderbird/thunderbird", "app-gnome-thunderbird-2788.scope"),
    ("agentic-riggd", USER, 11, 0.9, 11, 3, "/home/alex/.local/bin/agentic-riggd", "agentic-riggd", "agentic-riggd.service"),
    ("agentic-rigg", USER, 11, 0.7, 68, 8, "/home/alex/.local/bin/agentic-rigg", "agentic-rigg", "app-gnome-io.github.AgenticRigg-2850.scope"),
]
KERNEL = ["kthreadd", "rcu_preempt", "migration/0", "ksoftirqd/0", "kworker/0:1-events", "kworker/3:2-mm_percpu_wq",
          "kworker/u32:4-flush-259:0", "kcompactd0", "khugepaged", "kswapd0", "irq/88-nvidia", "nv_queue", "nvidia-modeset/kthread_q",
          "jbd2/nvme0n1p2-8", "kworker/7:0H-kblockd", "kworker/11:1-events", "migration/8", "ksoftirqd/8", "kauditd", "watchdogd"]
UIDS = {"root": 0, "systemd-resolve": 991, "polkitd": 992, "avahi": 993, USER: 1000}


def write(path: Path, text: str, in_place: bool = False) -> None:
    if in_place and path.exists():
        with open(path, "r+") as f:
            f.seek(0)
            f.write(text)
            f.truncate()
        return
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(text)
    os.replace(tmp, path)


def stat_line(pid, name, state, ppid, flags, ticks, nice, threads, start, rss_pages):
    utime, stime = ticks * 8 // 10, ticks - ticks * 8 // 10
    fields = [state, ppid, pid, pid, 0, -1, flags, 1200, 0, 3, 0, utime, stime, 0, 0, 20 + nice, nice, threads, 0, start,
              rss_pages * PAGE * 3, rss_pages] + [0] * 30
    return f"{pid} ({name}) " + " ".join(str(f) for f in fields) + "\n"


def status_text(name, pid, ppid, uid, rss_mib, threads, kernel):
    caps = "000001ffffffffff" if uid == 0 and not kernel else "0000000000000000"
    lines = [f"Name:\t{name}", "State:\tS (sleeping)", f"Tgid:\t{pid}", f"Pid:\t{pid}", f"PPid:\t{ppid}",
             f"Uid:\t{uid}\t{uid}\t{uid}\t{uid}", f"Gid:\t{uid}\t{uid}\t{uid}\t{uid}"]
    if not kernel:
        kb = rss_mib * 1024
        lines += [f"VmRSS:\t{kb} kB", f"RssAnon:\t{kb * 7 // 10} kB", f"RssFile:\t{kb * 25 // 100} kB",
                  f"RssShmem:\t{kb * 5 // 100} kB", "VmSwap:\t0 kB"]
    lines += [f"Threads:\t{threads}", f"CapPrm:\t{caps}", f"CapEff:\t{caps}", "CapAmb:\t0000000000000000",
              f"Kthread:\t{1 if kernel else 0}"]
    return "\n".join(lines) + "\n"


def build(root: Path, fixture: Path):
    if root.exists():
        shutil.rmtree(root)
    shutil.copytree(fixture, root, symlinks=False)
    (root / "etc").mkdir(exist_ok=True)
    (root / "etc/passwd").write_text("".join(f"{n}:x:{u}:{u}::/nonexistent:/usr/sbin/nologin\n" for n, u in UIDS.items()))
    (root / "proc/cpuinfo").write_text("".join(
        f"processor\t: {i}\nvendor_id\t: AuthenticAMD\ncpu family\t: 23\nmodel\t\t: 8\nmodel name\t: {CPU_NAME}\n"
        f"stepping\t: 2\ncpu MHz\t\t: 3700.000\ncache size\t: 512 KB\nphysical id\t: 0\nsiblings\t: {THREADS}\n"
        f"core id\t\t: {i // 2}\ncpu cores\t: {THREADS // 2}\nflags\t\t: fpu sse sse2 ssse3 sse4_1 sse4_2 avx avx2 aes\n\n"
        for i in range(THREADS)))
    procs = []
    pid = 1
    boot_age = 3 * 3600
    for i, (name, user, parent, cpu, rss, threads, exe, cmd, unit) in enumerate(APPS):
        pid = 1 if i == 0 else 1000 + i * 37
        procs.append(dict(pid=pid, name=name, uid=UIDS[user], parent_index=parent, cpu=cpu, rss=rss, threads=threads,
                          exe=exe, cmd=cmd, unit=unit, kernel=False, start=(5 + i * 40) * HZ, ticks=int(cpu * 9 + 400 + i * 50)))
    for j, name in enumerate(KERNEL):
        procs.append(dict(pid=2 if j == 0 else 3 + j * 11, name=name, uid=0, parent_index=None, cpu=0.1 if j % 6 == 4 else 0.0,
                          rss=0, threads=1, exe=None, cmd="", unit="kthreadd", kernel=True, start=HZ // 10, ticks=j * 3))
    by_index = [p["pid"] for p in procs]
    for p in procs:
        if p["kernel"]:
            p["ppid"] = 0 if p["pid"] == 2 else 2
        else:
            p["ppid"] = 0 if p["pid"] == 1 else by_index[p["parent_index"]]
        d = root / "proc" / str(p["pid"])
        d.mkdir(parents=True)
        (d / "cmdline").write_text(p["cmd"].replace(" ", "\0") + ("\0" if p["cmd"] else ""))
        (d / "status").write_text(status_text(p["name"], p["pid"], p["ppid"], p["uid"], p["rss"], p["threads"], p["kernel"]))
        (d / "cgroup").write_text(f"0::/user.slice/user-1000.slice/user@1000.service/app.slice/{p['unit']}\n" if p["uid"] == 1000
                                  else f"0::/system.slice/{p['unit']}\n")
        if p["exe"]:
            os.symlink(p["exe"], d / "exe")
            kb = p["rss"] * 1024
            (d / "smaps_rollup").write_text(f"Rss:\t{kb} kB\nPss:\t{kb * 8 // 10} kB\nPrivate_Clean:\t{kb // 10} kB\nPrivate_Dirty:\t{kb * 6 // 10} kB\n")
            (d / "io").write_text(f"rchar: 0\nwchar: 0\nread_bytes: {p['rss'] * 731_000}\nwrite_bytes: {p['rss'] * 97_000}\n")
    return procs, int(time.time()) - boot_age


def tick(root: Path, procs, btime, cpu_total, first):
    for p in procs:
        jitter = random.uniform(0.8, 1.2) if p["cpu"] > 0 else 0
        p["ticks"] += int(round(p["cpu"] * HZ / 100 * jitter))
        flags = 0x00200040 if p["kernel"] else 0x00400100
        rss_pages = p["rss"] * 1024 * 1024 // PAGE
        write(root / "proc" / str(p["pid"]) / "stat",
              stat_line(p["pid"], p["name"], "R" if p["cpu"] > 50 else ("I" if p["kernel"] else "S"), p["ppid"], flags,
                        p["ticks"], 0, p["threads"], p["start"], rss_pages))
    busy = sum(p["cpu"] for p in procs) / THREADS
    lines = []
    for c in range(THREADS):
        share = min(98.0, busy * random.uniform(0.6, 1.4))
        cpu_total[c][0] += int(share * HZ / 100)
        cpu_total[c][1] += int((100 - share) * HZ / 100)
    tot_busy = sum(c[0] for c in cpu_total)
    tot_idle = sum(c[1] for c in cpu_total)
    lines.append(f"cpu  {tot_busy * 8 // 10} 0 {tot_busy - tot_busy * 8 // 10} {tot_idle} 0 0 0 0 0 0")
    for c, (b, i) in enumerate(cpu_total):
        lines.append(f"cpu{c} {b * 8 // 10} 0 {b - b * 8 // 10} {i} 0 0 0 0 0 0")
    lines += ["intr 0", "ctxt 0", f"btime {btime}", "processes 9000", "procs_running 4", "procs_blocked 0"]
    write(root / "proc/stat", "\n".join(lines) + "\n", in_place=not first)
    write(root / "proc/uptime", f"{time.time() - btime:.2f} 0.00\n", in_place=not first)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, type=Path)
    ap.add_argument("--fixture", type=Path, default=Path(__file__).resolve().parent.parent / "crates/rigg-hw/tests/fixtures/maskin")
    args = ap.parse_args()
    root = args.root.resolve()
    if root == Path("/") or not root.is_relative_to(Path.home() / ".cache"):
        sys.exit("Roten må ligge under ~/.cache")
    random.seed(7)
    procs, btime = build(root, args.fixture)
    cpu_total = [[100_000 + c * 500, 900_000] for c in range(THREADS)]
    running = True

    def stop(*_):
        nonlocal running
        running = False

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    first = True
    while running:
        tick(root, procs, btime, cpu_total, first)
        if first:
            (root / ".klar").write_text("1\n")
        first = False
        time.sleep(1.0)


if __name__ == "__main__":
    main()
