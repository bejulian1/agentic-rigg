# Agentic Rigg

A lightweight Linux hardware monitor and NVIDIA GPU control desktop application written in Rust and GTK 4. Inspired by MSI Afterburner, HWiNFO, GPU-Z, CPU-Z, and CrystalDiskInfo, Agentic Rigg brings system sensors, long-term graphs, CPU/GPU specifications, SMART disk health, and opt-in NVML GPU tuning together in a single native app with no Electron overhead.

[Latest release](https://github.com/bejulian1/agentic-rigg/releases/latest) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [Contributors](CONTRIBUTORS.md)

> **Language note:** The application user interface is currently in **Norwegian Bokmål** (`nb`). Issues, discussions, and pull requests are welcome in English or Norwegian.

<table>
  <tr><th>Fans (Vifter)</th><th>Clocks &amp; Power (Klokker og effekt)</th><th>RGB Preview (RGB)</th></tr>
  <tr>
    <td><a href="docs/images/vifter.png"><img src="docs/images/vifter.png" alt="Fans page with Simple and Advanced views" width="320"></a></td>
    <td><a href="docs/images/klokker.png"><img src="docs/images/klokker.png" alt="Clocks and Power page with live readings and GPU tuning" width="320"></a></td>
    <td><a href="docs/images/rgb.png"><img src="docs/images/rgb.png" alt="RGB preview with PCIe x16 card illustration" width="320"></a></td>
  </tr>
</table>

*Real GTK 4 application screenshots captured during private test sessions using simulated hardware data. Click any image for full size.*

## Features by Page

| Page (UI name) | What it shows |
|---|---|
| Overview (`Oversikt`) | CPU, GPU, RAM, VRAM, disks, and network tiles; status bar for notable events; graphs from 1 minute to 48 hours |
| Sensors (`Sensorer`) | Live sensor table with min, max, and average values, sparklines, and CSV logging |
| Graphs (`Grafer`) | Synchronized cursor across graphs, pause, CSV export, and adaptive axes |
| Graphics Card (`Grafikkort`) | NVML telemetry: clocks, power, PCIe link, VBIOS, fans, limits, and active GPU processes, plus a clearly separated reference table |
| Processor (`Prosessor`) | Per-thread/per-core clock and load, cache topology from sysfs, instruction sets, vulnerabilities, and launch dates for known models |
| Disks (`Disker`) | SMART/NVMe health via UDisks2, temperature, power-on time, wear, I/O throughput, and partitions |
| System (`System`) | Motherboard, BIOS, memory usage, OS information, and controllers |
| GPU Control (`Skjermkortstyring`) | Opt-in NVML control for supported power limit, clock offsets, temperature target, and fans (Simple/Advanced views, curves, drafts/profiles), plus an RGB preview |

Hardware readings come from `/proc`, sysfs/DMI, hwmon, UDisks2, and NVIDIA NVML. Available fields depend on your hardware and drivers; missing or unsupported metrics are shown as unknown or unavailable rather than fabricated. Reference data is kept separate from live measurements.

## Requirements & Verified Platform Support

- **Verified prebuilt target:** Ubuntu 26.04 (`amd64` / `x86-64`), GTK ≥ 4.22, glibc ≥ 2.39. Older Ubuntu releases are not verified.
- **GPU backend:** NVIDIA GPUs via NVML (`libnvidia-ml.so.1`, provided by the NVIDIA driver). **AMD and Intel GPU backends are not implemented.** Without an NVIDIA driver, CPU, memory, disk, and other available system views still work.
- **Disk health:** SMART/NVMe attributes are read without root via UDisks2.
- **Desktop environment:** Developed on Ubuntu / GNOME / Wayland. The optional background tray icon requires GNOME's AppIndicator extension (enabled by default on Ubuntu).
- **Building from source:** Rust ≥ 1.92 (edition 2024), `cargo`, `rustc`, `pkgconf`, `libglib2.0-dev-bin`, and `libgtk-4-dev` (`gtk4 >= 4.22`). The NVIDIA driver is not required to build.

## Installation

### From GitHub Releases

Download the tarball and `SHA256SUMS` from the [latest GitHub Release](https://github.com/bejulian1/agentic-rigg/releases/latest), then verify and install:

```bash
sha256sum --check SHA256SUMS
tar -xzf agentic-rigg-*.tar.gz
cd agentic-rigg-*
./install.sh --bin-dir "$PWD/bin"
~/.local/bin/agentic-rigg
```

### From Source

```bash
git clone https://github.com/bejulian1/agentic-rigg.git
cd agentic-rigg
./install.sh
```

`install.sh` checks required build packages, compiles the Rust workspace into `${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}`, and installs for the current user.

### User-Local Install & Uninstalling

- **No `sudo` required:** Everything installs under `~/.local` (`~/.local/bin/`, `~/.local/share/`, and `~/.config/systemd/user/agentic-riggd.service`), adds an entry to the application menu, and preserves existing settings and history.
- **Nothing enabled at login by default:** The window starts `agentic-riggd` on demand. Optional background recording and login autostart can be toggled in `Innstillinger › Generelt › Bakgrunn`.
- **GPU helper stays off by default:** Installation only stages the helper files under `~/.local/share/agentic-rigg/hjelper/`. Activation is a separate opt-in step via polkit.
- **Updating:** Close the app, then re-run `./install.sh` from a new release archive or after `git pull`.
- **Uninstalling:** If you enabled the GPU helper, disable it first in `Innstillinger › Skjermkortstyring` (this restores captured GPU baseline settings before removing root files). Then run:

```bash
./uninstall.sh
```

Uninstalling removes binaries, icons, the `.desktop` file, the user systemd unit, and staged helper files, while leaving user settings (`~/.config/agentic-rigg/`) and history (`~/.local/state/agentic-rigg/`) intact.

## GPU Control & Safety Model

Monitoring requires no administrator privileges. GPU tuning is strictly opt-in and built around defense-in-depth safeguards:

- **One-step polkit activation:** Enabling **"Tillat endring av skjermkortet"** in `Innstillinger › Skjermkortstyring` authenticates once via polkit, installing a hardened systemd system service (`agentic-rigg-helper.service`) under `/usr/local/libexec/` and `/etc/agentic-rigg/helper.json`.
- **Card-bound helper:** The helper is bound to the selected GPU's PCI address and NVML UUID and verifies that identity before every write. It listens on `/run/agentic-rigg/helper.sock` (`0600`, owned by the configured user) and checks `SO_PEERCRED` UID. Only fixed operations (`status`, `apply`, `confirm`, `revert`, `reset`) are accepted; no arbitrary commands or paths can be sent. The systemd unit runs with `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `NoNewPrivileges=yes`, and only `CAP_CHOWN` and `CAP_SYS_ADMIN`.
- **Apply / Keep with automatic rollback:** Changes are drafted first and only modified fields are sent when you click **Bruk** (Apply). You must click **Behold** (Keep) within the timeout (15 s by default) or the helper automatically rolls back to the last confirmed state. Missed heartbeats, daemon disconnects, sleep gaps, and service stop (`ExecStopPost`) also trigger restoration.
- **Persistent captured baseline:** Before writing, the helper atomically records the GPU's pre-existing settings under `/var/lib/agentic-rigg/` (bound to PCI/UUID, driver, and VBIOS). Resetting restores those captured values. If recovery fails or cannot be verified by a fresh readback, further writes are locked out and the error is surfaced honestly.

### What Is and Is Not Verified

- **Verified on physical hardware (RTX 2080, NVML):** Polkit activation and limited writes with fresh NVML readback and restore—power limit (`245 → 240 W`), both fans (`Auto → 50%` manual), core offset (`−15 MHz`), memory offset (`−10 MHz`), a flat `50%` fan curve held for 4 s, and temperature target (`83 → 82 °C`), plus daemon-forwarded power limit with Keep, reset, and 10 s timeout rollback.
- **Not verified:** Extreme limits, positive overclocking, stress testing, driver crash/loss, power-loss recovery, automatic Zero RPM fan stop, and NVIDIA GPU models other than the development RTX 2080.
- **RGB lighting:** Preview only. The GPU lighting controller is not identified, and Agentic Rigg never opens `/dev/i2c-*`.

## Architecture

The Rust workspace lives in [`app/`](app/) and is split into five crates:

| Crate | Binary | Responsibility |
|---|---|---|
| [`rigg`](app/crates/rigg) | `agentic-rigg` | GTK 4 desktop UI (no libadwaita). Never loads NVML and never communicates directly with the root helper. |
| [`riggd`](app/crates/riggd) | `agentic-riggd` | Unprivileged user daemon. Reads NVML, `/proc`, `/sys`, hwmon, and UDisks2; manages 48 h history, min/max, CSV logging, alerts, and the per-user Unix socket under `$XDG_RUNTIME_DIR/agentic-rigg/`. |
| [`rigg-helper`](app/crates/rigg-helper) | `agentic-rigg-helper` | Optional, card-bound root helper for NVML writes. Minimal dependencies (no GLib loop, no D-Bus, no threads) and the only crate enabling `rigg-hw`'s `ekte-skriving` write feature. |
| [`rigg-core`](app/crates/rigg-core) | — | Pure Rust data model, IPC protocol, history storage, settings, formatting, and helper compatibility checks. |
| [`rigg-hw`](app/crates/rigg-hw) | — | Hardware readers (NVML, `/proc`, `/sys`, hwmon, UDisks2) and a simulated backend for isolated testing. |

## Privacy

Everything runs locally on your machine. The Rust crates contain **no network code**, make no outbound connections, download nothing, and send no telemetry or crash reports. CSV logs and system reports are only written where you explicitly choose to save them, and hardware serial numbers and UUIDs are hidden in exported reports until you opt to include them.

## Status & Roadmap

See [CHANGELOG.md](CHANGELOG.md) for the full release history.

Known limitations and areas for future work:

- **NVIDIA only:** AMD and Intel GPU backends are not implemented yet.
- **Hardware coverage:** Physical GPU write testing has only been performed on an RTX 2080; other NVIDIA cards and edge cases (stress/error recovery, Zero RPM) remain to be validated.
- **RGB control:** Currently a 2D/3D preview without hardware I2C writes.
- **Additional system details:** RAM module SPD details, display information, and network adapter specifications are candidates for future expansion. Root-only DMI/CPU fields and motherboard sensor kernel modules (`nct6775`, etc.) are not loaded or accessed by the app.

## Contributing

Contributions are welcome—see [CONTRIBUTING.md](CONTRIBUTING.md) for build setup, test isolation requirements, hardware-safety rules, and pull request guidelines. Participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Please report security vulnerabilities privately via GitHub Security Advisories. See [SECURITY.md](SECURITY.md) for supported versions, scope, and reporting instructions.

## License

Licensed under the [MIT License](LICENSE). UI icons are from [Lucide](https://lucide.dev) (ISC) with select [Feather](https://feathericons.com) elements (MIT). Full third-party license notices are included in [app/data/LISENSER.md](app/data/LISENSER.md) and distributed with release packages.

## Credits

Created by [Julian / bejulian1](https://github.com/bejulian1), with AI assistance from **Anthropic Claude Opus 5.5 (Claude Code)** and **OpenAI GPT 6.1 Sol (Codex)**. See [CONTRIBUTORS.md](CONTRIBUTORS.md) for roles and attribution details.
