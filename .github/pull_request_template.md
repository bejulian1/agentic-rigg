## Summary

<!-- Describe what this PR changes and why. English or Norwegian is welcome. -->

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] Refactor / maintenance

## Verification

<!-- Describe how you tested these changes (commands run, UI checks, simulated vs. physical hardware). -->

- `CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo clippy --workspace --all-targets -- -D warnings`
- `CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo test --workspace`

## Checklist

- [ ] All automated tests use isolated helper/config/socket/baseline paths (never `/run/agentic-rigg`, `/etc/agentic-rigg`, or `/var/lib/agentic-rigg`).
- [ ] No real GPU or `/dev/i2c-*` writes are performed by automated tests or default paths.
- [ ] Any new or updated UI strings are in Norwegian Bokmål.
- [ ] No private logs, local backups, or unrelated files are included.
- [ ] AI assistance (if used) is disclosed in the commit message or PR description.
