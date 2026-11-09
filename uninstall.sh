#!/usr/bin/env bash
# Fjerner Agentic Rigg fra ~/.local. Innstillinger, historikk og logger blir
# liggende; skriptet sier hvor de er. Se app/uninstall.sh for detaljene.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec "$here/app/uninstall.sh" "$@"
