#!/usr/bin/env bash
# Kjører ett av måleskriptene for runde 3C i en privat, hodeløs økt (app/scripts/okt.sh).
#
#   kjor.sh SKRIPT [NAVN=VERDI …]
#
# Eksempler (binærene: RIGG_BIN, ellers ~/.cache/rigg-build/release):
#   kjor.sh start.sh RUNS=11 RENDERERS="default gl vulkan"   kald start og lukking per tegner
#   kjor.sh start_kald.sh RUNS=6                              kald start når tjenesten ikke kjører
#   kjor.sh varm.sh RUNS=8                                    varm start (vinduet finnes)
#   kjor.sh cpu.sh PAGES="oversikt sensorer" RENDERERS=cairo REPS=5 SECS=20 [WATCH=1]
#   kjor.sh rammer.sh PAGES=oversikt RENDERERS=cairo PRE=65   bilder og tid per bilde (RIGG_RAMMER)
#   kjor.sh tjeneste.sh TAG=2s BG_MS=2000 REPS=5 SECS=60 [WATCH=1]
#   kjor.sh minne.sh RENDERERS="cairo gl" RUNS=5              vinduet etter at alle sidene er besøkt
#   kjor.sh lekkasje.sh MIN=60                                tjenesten i 1 t, vinduet av og på
#   kjor.sh lekkasje.sh MIN=30 DARGS="--backend sim --syntetisk-historikk" CSV=lekkasje-full.csv
#                                                             samme med full historikk (48 t) fra start
#
# Alt leser bare (NVML-get, /proc, /sys). WATCH=1 kjører nvml_timing watch 2000
# ved siden av for å se P-tilstanden; det forstyrrer CPU-tallet litt, så mål
# CPU uten. Resultatlinjene begynner med RES; lekkasjetesten skriver
# $OUT/lekkasje.csv (OUT er denne mappa hvis ikke satt).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
app="$here/../../../../app"
r=$1; shift
bin=${RIGG_BIN:-${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}/release}
OKT_WARMUP=4 "$app/scripts/okt.sh" --bredde 1920x1080 -- env RIGG_BIN="$bin" OUT="${OUT:-$here}" NVT="${NVT:-$HOME/.cache/rigg-build/nvml_timing}" "$@" bash "$here/$r" 2>&1 | grep -E "^(run=|#|RES)"
