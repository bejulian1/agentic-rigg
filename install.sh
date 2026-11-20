#!/usr/bin/env bash
# Bygger og installerer Agentic Rigg for denne brukeren under ~/.local, rett
# fra en klone av repoet. Bruker ikke sudo, og slår ingenting på ved
# innlogging. Valgfrie systemhjelpere installeres separat fra et verifisert arkiv.
#
# Bruk: ./install.sh [--no-build] [--no-start]
#
#   (ingen valg)  Sjekker pakkene, bygger med cargo og installerer.
#   --no-build    Installerer det som allerede er bygget, uten å bygge.
#   --no-start    Lar være å starte appen til slutt.
#
# Bygget havner i $CARGO_TARGET_DIR, eller ~/.cache/rigg-build.
set -euo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
build=1
start=1

while [ $# -gt 0 ]; do
    case "$1" in
    --no-build) build=0; shift ;;
    --no-start) start=0; shift ;;
    -h | --help)
        sed -n '2,14s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
        exit 0
        ;;
    *)
        echo "Ukjent argument: $1" >&2
        exit 2
        ;;
    esac
done

target_dir=${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}

if [ "$build" = 1 ]; then
    # Samler opp pakkene som mangler, så alt kan installeres med én kommando.
    missing=()
    need_cmd() { command -v "$1" >/dev/null || missing+=("$2"); }
    need_pc() { pkg-config --exists "$1" 2>/dev/null || missing+=("$2"); }
    need_cmd cargo cargo
    need_cmd rustc rustc
    need_cmd pkg-config pkgconf
    need_cmd glib-compile-resources libglib2.0-dev-bin
    if command -v pkg-config >/dev/null; then
        need_pc 'gtk4 >= 4.22' libgtk-4-dev
    else
        missing+=(libgtk-4-dev)
    fi
    if [ ${#missing[@]} -gt 0 ]; then
        echo "Disse pakkene mangler for å bygge Agentic Rigg:" >&2
        echo >&2
        echo "  sudo apt install ${missing[*]}" >&2
        echo >&2
        echo "Kjør kommandoen over, og så ./install.sh igjen." >&2
        exit 1
    fi
    # Rust 1.92 eller nyere (edition 2024).
    rv=$(rustc --version | awk '{print $2}')
    if [ "$(printf '%s\n1.92.0\n' "$rv" | sort -V | head -1)" != "1.92.0" ]; then
        echo "Rust $rv er for gammel. Agentic Rigg trenger 1.92 eller nyere (rustup update)." >&2
        exit 1
    fi
    echo "Bygger Agentic Rigg. Første gang tar det noen minutter ..."
    (cd "$here/app" && CARGO_TARGET_DIR=$target_dir cargo build --release --locked --workspace)
fi

"$here/app/install.sh" --bin-dir "$target_dir/release"

if ! command -v nvidia-smi >/dev/null; then
    echo
    echo "Merk: fant ikke NVIDIA-driveren (nvidia-smi). Appen viser prosessor, minne,"
    echo "disker og sensorer, men skjermkortdelen er tom uten driveren."
fi

if [ "$start" = 1 ] && [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
    setsid "$HOME/.local/bin/agentic-rigg" >/dev/null 2>&1 < /dev/null &
    echo "Agentic Rigg er startet. Den ligger også i «Vis programmer»."
fi
