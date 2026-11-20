# Contributing to Agentic Rigg

Thank you for your interest in contributing to Agentic Rigg! Please review these guidelines before opening an issue or pull request.

Participation in this project is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Language Conventions

- **Issues, discussions, and pull requests:** English or Norwegian Bokmål are both welcome.
- **Application UI strings:** All user-facing text inside the GTK application is written in **Norwegian Bokmål** (`nb`). Keep UI copy concise, consistent, and free of placeholder text.

## Development Setup

Agentic Rigg is a Rust + GTK 4 workspace located in [`app/`](app/).

### System Packages

On Ubuntu 26.04 (or compatible Debian-based systems with GTK ≥ 4.22), install the build dependencies checked by [`install.sh`](install.sh):

```bash
sudo apt install cargo rustc pkgconf libglib2.0-dev-bin libgtk-4-dev
```

- **Rust:** ≥ 1.92 (Rust edition 2024). If your distro's `rustc` is older, use `rustup`.
- **GTK 4:** `gtk4 >= 4.22` (`libgtk-4-dev`).
- **NVIDIA driver:** `libnvidia-ml.so.1` is loaded dynamically at runtime and is **not** required to compile or run unit tests.

### Building, Linting, and Testing

Use a shared target directory outside the repository so build artifacts do not clutter the working tree:

```bash
cd app
CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo build --workspace
CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo clippy --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo test --workspace
```

To build optimized release binaries:

```bash
cd app
CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo build --release --locked --workspace
```

### Installing the optional system helpers

The desktop installer is unprivileged. Never use `sudo ./install.sh` or run a wrapper from a checkout, home directory or extracted download as root. For a release use the [verified archive bootstrap](docs/produksjon/rgb-hjelper.md#betrodd-installasjon-og-administratoraktivering) and the archive SHA-256 obtained independently from the Release page. For source builds, use the same procedure with a clean reviewed commit built by a trusted separate builder; the administrator approves the resulting archive digest, not a writable target directory. Installation grants neither NVML nor RGB access. After deactivating both, remove the system components with `sudo /usr/local/libexec/agentic-rigg-system/system-install --uninstall`.

## Test Isolation Rules

Automated tests and private GTK screenshot sessions **must be completely isolated** from any live Agentic Rigg installation on the host:

- Always use temporary, private directories for helper root, configuration, socket, and baseline files (e.g., `--socket`, `--config`, `--baseline`, and `RIGG_HJELPER_ROT` / temporary `HOME` / `XDG_*` paths).
- **Never** fall back to host paths (`/run/agentic-rigg/helper.sock`, `/etc/agentic-rigg/helper.json`, or `/var/lib/agentic-rigg`) in any test fixture.
- Tests must pass even when a real, activated GPU helper is running on the developer's machine.
- For concurrent worktrees sharing a Cargo cache, use a distinct Cargo profile and pin its freshly built `agentic-rigg-helper` in a private `0700` directory under `/tmp` with `RIGG_TEST_HJELPER_BIN`. Only the simulated daemon integration test reads this override; it avoids rebuilding or picking another worktree's helper while the suite runs.

## Hardware-Safety Rules

Agentic Rigg interacts with real hardware telemetry and—when explicitly enabled by a user—NVIDIA GPU power, fan, and clock controls. Contributors must follow strict safety boundaries:

1. **No real hardware writes in automated tests:** Automated tests must never invoke NVML set calls, `nvidia-smi` write flags, or I2C writes. Use the simulated backend (`rigg-hw`'s simulated backend / `agentic-rigg-helper --backend sim`).
2. **Never open `/dev/i2c-*`:** Linux I2C bus numbers shift between reboots and can point to critical devices such as USB-C controllers (`ccgx-ucsi`) or memory SPD EEPROMs (`ee1004`). Reading bus names under `/sys/bus/i2c/devices/*/name` is safe; opening `/dev/i2c-*` is prohibited unless explicitly approved and bound by sysfs identity.
3. **Physical GPU tests are separate and explicit:** Any manual test against a physical GPU must target an explicitly selected card (bound by PCI address and NVML UUID), verify fresh NVML readback, and confirm restoration to the captured baseline.
4. **No unrequested privilege escalation:** Build, install, and test workflows must not require `sudo`, install system packages automatically, or load kernel modules.

## Commit Conventions & AI Assistance

- Write clear, descriptive commit messages that explain *what* changed and *why*.
- Keep commits focused and stage files explicitly rather than blindly adding everything in the working tree.
- Use `Co-authored-by:` trailers for actual co-authors.
- **Disclose AI assistance honestly** in commit messages or PR descriptions when AI tools meaningfully contributed to the change. Do not create artificial commits or rewrite history to manipulate GitHub contributor statistics.

## Pull Request Checklist

Before opening a pull request, please verify:

- [ ] `CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo clippy --workspace --all-targets -- -D warnings` passes cleanly.
- [ ] `CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo test --workspace` passes with all tests isolated from host `/run/agentic-rigg`, `/etc/agentic-rigg`, and `/var/lib/agentic-rigg`.
- [ ] No real GPU or `/dev/i2c-*` writes are triggered by automated tests or default code paths.
- [ ] Any new or modified UI text is in Norwegian Bokmål.
- [ ] Security-sensitive changes to `rigg-helper`, polkit policies, or helper protocol compatibility (`rigg_core::hjelper::MIN_COMPATIBLE_BUILD`) are clearly documented.
- [ ] No personal logs, local backups, raw measurements, or private data are included in the diff.
- [ ] AI assistance (if used) is disclosed.

## Releases (Maintainer-Only)

Releases are prepared and published exclusively by the project maintainer:

1. Bump the version and build number in `app/version.json`, keeping `app/Cargo.toml`, `app/Cargo.lock`, and release documentation in sync.
2. Update `CHANGELOG.md`, `app/PATCHLOG.md`, `app/data/version-log.json`, and `.github/release-notes/v<VERSION>.md`.
3. Run workspace checks (`clippy`, `test`), build release binaries (`agentic-rigg`, `agentic-riggd`, `agentic-rigg-helper`), and verify `--version` output on all three.
4. Package the Ubuntu 26.04 `amd64` archive and `SHA256SUMS` using `app/scripts/pakk-utgivelse.py` with `--bin-dir`, `--out-dir`, and `--commit`, and verify archive extraction and installation in an isolated directory.
5. Tag `v<VERSION>` on the release commit and publish the GitHub Release with release notes, tarball, and `SHA256SUMS`.
