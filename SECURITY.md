# Security Policy

## Supported Versions

Only the **latest release** of Agentic HW Monitor receives security updates.

| Version | Supported |
|---|---|
| Latest release | Yes |
| Older releases | No |

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues.

Instead, use GitHub's private vulnerability reporting for this repository:

- [Report a vulnerability privately](https://github.com/bejulian1/agentic-rigg/security/advisories/new)

Include as much relevant detail as you can:

- Affected component and version/build (visible in `Innstillinger › Om`)
- Steps to reproduce or proof of concept
- Potential impact and any suggested remediation

## Scope

Security reports are especially welcome for privileged and IPC boundaries, including:

- **Root GPU helper (`agentic-rigg-helper`):** privilege boundaries, systemd service hardening (`agentic-rigg-helper.service`), PCI/UUID card binding, baseline persistence (`/var/lib/agentic-rigg/`), and Apply/Keep/rollback enforcement.
- **Polkit activation and deactivation:** `aktiver.sh`, `deaktiver.sh`, and `io.github.AgenticRigg.helper.policy` (installation into `/usr/local/libexec/`, `/etc/agentic-rigg/helper.json`, `/etc/systemd/system/`, and `/usr/share/polkit-1/actions/`).
- **Hardware write & RGB/I2C boundaries:** any code path that could trigger unintended NVML writes, open `/dev/i2c-*`, or bypass safety limits.
- **Daemon and helper sockets:** Unix domain socket permissions and peer credential (`SO_PEERCRED`) checks on `$XDG_RUNTIME_DIR/agentic-rigg/` and `/run/agentic-rigg/helper.sock`.

## What to Expect

Agentic HW Monitor is maintained as a personal open-source project. Reports are handled on a best-effort basis:

1. **Acknowledgement:** The maintainer will review your advisory and acknowledge receipt as soon as practical.
2. **Investigation & fix:** Validated issues will be investigated and fixed privately. If a security fix affects the root helper, the minimum compatible helper build (`MIN_COMPATIBLE_BUILD`)—and protocol version when needed—will be bumped so older helpers cannot be reused.
3. **Disclosure:** Once a fix is released, the advisory will be updated and published with credit to the reporter if desired.
