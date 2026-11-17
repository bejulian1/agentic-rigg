#!/usr/bin/env bash
# Skjermbilder i private, hodeløse GNOME-økter (scripts/okt.sh). Én runner
# for alle planene: hver plan får sin egen økt, sin egen tjeneste med riktig
# backend og sin egen undermappe. Vinduet går gjennom planen og lagrer PNG-ene.
#
# Bruk: scripts/bilder.sh [--plan NAVN …] [MAPPE]
#   MAPPE   standard docs/skjermbilder/1.3.3/plan (én plan) eller
#           docs/skjermbilder/1.3.3/serie (flere planer, én undermappe per plan)
#   --plan  kan gis flere ganger, eller «alle». Standard: grunnmur.
#
# Planene (koden er fasit):
#   grunnmur          skjermbilder.rs, det ekte kortet (NVML, bare lesing)
#   3a                skjermbilder.rs RIGG_PLAN=3a, det ekte kortet
#   3a-sim            skjermbilder.rs RIGG_PLAN=3a-sim, --backend sim (Bruk/Behold)
#   3a-borte          skjermbilder.rs RIGG_PLAN=3a-borte, --backend sim:borte
#   3a-egg            skjermbilder.rs RIGG_PLAN=3a-egg, sim (påskeegg 1, 7 og 10)
#   3c                skjermbilder.rs RIGG_PLAN=3c, det ekte kortet: alle sidene ×
#                     lyst/mørkt × fire temaer, høy kontrast, 640 og 1920 px
#   2a                pages/oversikt/bilder.rs, det ekte kortet
#   2a-historikk      pages/oversikt/bilder.rs, sim med syntetisk 48 t
#   2b-kort           logg_ui/bilder.rs, det ekte kortet
#   2b-ingen-driver, 2b-driver-oppdatert, 2b-borte, 2b-to-kort, 2b-dvale, 2b-bygg
#                     logg_ui/bilder.rs mot sim-scenariet med samme navn
#   2c                pages/styring_bilder.rs (--side kontroller), sim: planen
#                     «sim» (utkast, Bruk → Behold / Rull tilbake, Manuell)
#   6a                skjermbilder.rs RIGG_PLAN=6a, sim: Disker (fem simulerte
#                     disker), Prosessor og System, lyst/mørkt, 1920 og 640 px
#   6a-ekte           skjermbilder.rs RIGG_PLAN=6a-ekte: de ekte diskene uten
#                     UDisks2 (økta har tom systembuss)
#   6b                skjermbilder.rs RIGG_PLAN=6b, sim: skallet (vindusknapper,
#                     maskinhodet, ikonlist, høyder, Om med Code Pro)
#   6b-borte, 6b-ingen-driver, 6b-driver-oppdatert, 6b-to-kort
#                     skjermbilder.rs RIGG_PLAN=6b-tilstand mot sim-scenariet
#                     med samme navn (maskinhodet i feiltilstandene)
#   6c-ekte           pages/styring_bilder.rs planen «ekte»: Klokker og effekt
#                     og Vifter låst mot det ekte kortet (bare lesing)
#   6c-lys            pages/styring_bilder.rs planen «lys», sim: RGB-lys 2D/3D
#   7a                skjermbilder.rs RIGG_PLAN=7a, sim: Innstillinger › Styring
#                     med den falske pkexec (scripts/falsk-pkexec.sh) og en falsk
#                     rot i ~/.cache/rigg-build: av, på, godkjenn, feil, av igjen
#   7a-ekte           utgått: falsk pkexec tillater nå bare sim; bruk 1.4.0-ui
#   7b-ekte, 7b-sim   pages/styring_bilder.rs: Vifter med viftemotoren, låst mot
#                     det ekte kortet og med kurven i bruk mot sim:spill
#   8a                skjermbilder.rs RIGG_PLAN=8a, sim: den nye Oversikt i
#                     lyst/mørkt, 1920/1440/900/640 px, høy kontrast, hele siden
#   8a-hete           skjermbilder.rs RIGG_PLAN=8a-hete, sim:hete: under 20 °C,
#                     95 °C, 110 °C og 300 W; aksene kutter ikke (Tilpass, Fast, °F)
#   kortmeny          kortdetaljer, menybredde, store bokstaver og ikonlist;
#                     RIGG_KORTMENY_BACKEND=sim:to-kort prøver også kortbytte
#   innstillinger-sok seksjonsbytte før søkeforsinkelsen, tømming og Esc
#   adaptive-akser    110 °C, 2500/10000 MHz, 400 W og 108 % vifte, bare
#                     simulerte verdier i testvinduet; fast/tilpass, °F og 48 t
#   1.4.0-ui          sim/falsk pkexec: filter/fokus, tidsmeny, globalaktivering,
#                     eksplisitt simulert klar-status og PCIe i 2D/3D (bruk gl)
#   rgb-ui            fysisk RGB-panel med merkede simulerte visningstilstander;
#                     alle administratorhandlinger og RGB-RPC-er er blokkert
#
# Binærene: $RIGG_BIN, ellers $CARGO_TARGET_DIR/release (eller debug). Planene
# fra runde 2 finnes bare i debug-bygget (unntatt 2c), så 2a* og 2b* tar
# $CARGO_TARGET_DIR/debug når RIGG_BIN ikke er satt (bygg med `cargo build`).
# Vinduet får høyst RIGG_PLAN_FRIST sekunder (standard 900). RIGG_OPPVARMING=SEK lar
# tjenesten samle historikk før bildene (standard 62 s, så sparklinjene er
# fulle; 8 s for sim-planene).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
plans=()
out=
while [ $# -gt 0 ]; do
    case "$1" in
    --plan) plans+=("$2"); shift 2 ;;
    -h | --help) sed -n '2,55p' "$0"; exit 0 ;;
    *) out=$1; shift ;;
    esac
done
all=(3c grunnmur 3a 3a-sim 3a-borte 3a-egg 2a 2a-historikk 2b-kort 2b-ingen-driver 2b-driver-oppdatert 2b-borte 2b-to-kort 2b-dvale 2b-bygg 2c
    6a 6a-ekte 6b 6b-borte 6b-ingen-driver 6b-driver-oppdatert 6b-to-kort 6c-ekte 6c-lys 7a 7b-ekte 7b-sim 8a 8a-hete kortmeny innstillinger-sok adaptive-akser)
[ ${#plans[@]} -gt 0 ] || plans=(grunnmur)
[ "${plans[0]}" = alle ] && plans=("${all[@]}")
many=$([ ${#plans[@]} -gt 1 ] && echo 1 || true)
out=${out:-$here/../docs/skjermbilder/1.3.3/$([ -n "$many" ] && echo serie || echo plan)}
mkdir -p "$out"
out=$(cd -- "$out" && pwd)
target=${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}
bin=${RIGG_BIN:-}
if [ -z "$bin" ]; then
    if [ -x "$target/release/agentic-rigg" ]; then bin=$target/release; else bin=$target/debug; fi
fi
[ -x "$bin/agentic-rigg" ] && [ -x "$bin/agentic-riggd" ] || { echo "Fant ikke binærene i $bin" >&2; exit 1; }

runner=$(mktemp)
trap 'rm -f -- "$runner"' EXIT
cat >"$runner" <<'RUN'
set -u
# Private UI-prøver får alltid egen hjelperrot, også når den ekte hjelperen
# er aktivert på verten. NVML-lesing kan fortsatt brukes til sensordata.
export RIGG_HJELPER_ROT="${RIGG_HJELPER_ROT:-$OKT_BASE/hjelper-rot}"
[ "$RIGG_HJELPER_ROT" != / ] || { echo "Skjermbildeprøver kan ikke bruke vertens GPU-hjelper." >&2; exit 2; }
mkdir -p "$RIGG_HJELPER_ROT"
rgb_fake_pid=
if [ "${RIGG_PLAN:-}" = rgb-flow ]; then
    /usr/bin/python3 -I "$RIGG_RGB_FAKE_SCRIPT" --root "$RIGG_HJELPER_ROT" --build "$RIGG_RGB_FAKE_BUILD" >"$OKT_BASE/rgb-fake.log" 2>&1 &
    rgb_fake_pid=$!
    for _ in $(seq 1 50); do
        [ -S "$RIGG_HJELPER_ROT/run/agentic-rigg-rgb/helper.sock" ] && break
        sleep 0.05
    done
    if [ ! -S "$RIGG_HJELPER_ROT/run/agentic-rigg-rgb/helper.sock" ]; then
        kill -TERM "$rgb_fake_pid" 2>/dev/null || true
        wait "$rgb_fake_pid" 2>/dev/null || true
        echo "Den private RGB-fixturen startet ikke." >&2
        exit 2
    fi
fi
# Lengre frist enn 60 s, så tjenesten samler historikk under oppvarmingen i
# stedet for å stoppe uten klienter. «Fortsett i bakgrunnen» står som standard (av).
# shellcheck disable=SC2086
"$RIGG_BIN/agentic-riggd" --grace-ms 600000 ${RIGG_BACKEND:+--backend "$RIGG_BACKEND"} $RIGG_RIGGD_ARGS 2>"$OKT_BASE/riggd.log" &
d=$!
sleep "$RIGG_WARM"
# shellcheck disable=SC2086
timeout "${RIGG_PLAN_FRIST:-900}" "$RIGG_BIN/agentic-rigg" $RIGG_VINDU_ARGS 2>"$OKT_BASE/rigg.log"
rc=$?
kill -TERM "$d" 2>/dev/null
wait "$d" 2>/dev/null
if [ -n "$rgb_fake_pid" ]; then
    kill -TERM "$rgb_fake_pid" 2>/dev/null || true
    wait "$rgb_fake_pid" 2>/dev/null || true
fi
grep -E "CSS|CRITICAL|WARNING|panicked|RIGG_KORTMENY|kortmenyen er bredere|RIGG_SKJERMBILDE|RIGG_BILDER_2C|RIGG_BREDDE|RIGG_2C_MAAL|RIGG_2C_KLAR|fant ikke|mangler kroken" "$OKT_BASE/rigg.log" | head -80 >&2
exit $rc
RUN

rc_all=0
for plan in "${plans[@]}"; do
    dir=$out
    [ -n "$many" ] && dir=$out/$plan
    mkdir -p "$dir"
    # Hver plan: (miljø for planen, backend, flagg til tjenesten og vinduet).
    vars=() backend= dargs= wargs= warm=${RIGG_OPPVARMING:-62}
    case "$plan" in
    grunnmur) vars=("RIGG_SKJERMBILDER=$dir") ;;
    3a | 3a-sim | 3a-borte | 3a-egg | 3c)
        vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=$plan")
        case "$plan" in
        3a-sim | 3a-egg) backend=sim warm=${RIGG_OPPVARMING:-8} ;;
        3a-borte) backend=sim:borte warm=1 ;;
        esac
        ;;
    2a) vars=("RIGG_SKJERMBILDER_2A=$dir") ;;
    2a-historikk) vars=("RIGG_SKJERMBILDER_2A=$dir" "RIGG_2A_DEL=historikk") backend=sim dargs=--syntetisk-historikk warm=${RIGG_OPPVARMING:-8} ;;
    2b-kort) vars=("RIGG_SKJERMBILDER_2B=$dir" "RIGG_2B_PLAN=kort") ;;
    2b-*)
        name=${plan#2b-}
        vars=("RIGG_SKJERMBILDER_2B=$dir" "RIGG_2B_PLAN=$name")
        case "$name" in
        dvale | bygg) backend=sim ;;
        *) backend=sim:$name ;;
        esac
        warm=${RIGG_OPPVARMING:-4}
        [ "$name" = borte ] && warm=1
        ;;
    2c) vars=("RIGG_BILDER_2C=$dir") backend=sim dargs=--syntetisk-historikk wargs="--side kontroller" warm=${RIGG_OPPVARMING:-8} ;;
    6a) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=6a") backend=sim warm=${RIGG_OPPVARMING:-25} ;;
    6a-ekte) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=6a-ekte") warm=${RIGG_OPPVARMING:-12} ;;
    6b) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=6b") backend=sim warm=${RIGG_OPPVARMING:-8} ;;
    6b-*)
        name=${plan#6b-}
        vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=6b-tilstand" "RIGG_6B_TILSTAND=$name") backend=sim:$name warm=${RIGG_OPPVARMING:-3}
        # Kortet faller av etter 15 s og sjekkes hvert 10. s: vent til det er borte.
        [ "$name" = borte ] && vars+=("RIGG_6B_VENT=24000")
        ;;
    6c-ekte) vars=("RIGG_BILDER_2C=$dir" "RIGG_6C_PLAN=ekte") wargs="--side kontroller" ;;
    6c-lys) vars=("RIGG_BILDER_2C=$dir" "RIGG_6C_PLAN=lys") backend=sim wargs="--side kontroller" warm=${RIGG_OPPVARMING:-8} ;;
    7a-ekte) echo "7a-ekte er utgått: falsk pkexec starter aldri NVML. Bruk --plan 1.4.0-ui." >&2; exit 2 ;;
    7a | 1.4.0-ui)
        # Som docs/produksjon/verktoy/runde7a/bilder7a.sh: filene til
        # hjelperen i en egen mappe (som install.sh) og en falsk rot. Ingen
        # root, ingen polkit, ingen systemctl.
        fake=$target/7a-rot-$plan hdata=$target/7a-hjelper-$plan
        rm -rf -- "$fake" "$hdata"
        mkdir -m 700 -p "$fake" "$hdata"
        cp "$here"/data/hjelper/* "$hdata/"
        cp "$bin/agentic-rigg-helper" "$hdata/"
        vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=$plan" "RIGG_PKEXEC=$here/scripts/falsk-pkexec.sh" "RIGG_HJELPER_DATA=$hdata" "RIGG_HJELPER_ROT=$fake")
        backend=sim vars+=("RIGG_FALSK_HJELPER_BACKEND=sim") warm=${RIGG_OPPVARMING:-8}
        ;;
    8a) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=8a") backend=sim warm=${RIGG_OPPVARMING:-70} ;;
    kortmeny) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=kortmeny") backend=${RIGG_KORTMENY_BACKEND:-sim} warm=${RIGG_OPPVARMING:-8} ;;
    innstillinger-sok) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=innstillinger-sok") backend=sim warm=${RIGG_OPPVARMING:-4} ;;
    adaptive-akser) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=adaptive-akser") backend=sim warm=${RIGG_OPPVARMING:-4} ;;
    rgb-ui | rgb-flow)
        fake=$(mktemp -d /tmp/rigg-rgb-ui-root.XXXXXX)
        hdata=$(mktemp -d /tmp/rigg-rgb-ui-data.XXXXXX)
        cp "$here"/data/hjelper/* "$hdata/"
        cp "$bin/agentic-rigg-helper" "$hdata/"
        vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=$plan" "RIGG_PKEXEC=$here/scripts/falsk-pkexec.sh" "RIGG_HJELPER_DATA=$hdata" "RIGG_HJELPER_ROT=$fake" "RIGG_FALSK_HJELPER_BACKEND=sim")
        if [ "$plan" = rgb-flow ]; then
            printf 'agentic-rigg-rgb-fake-v1\n' >"$fake/rgb-flow.fake"
            chmod 600 "$fake/rgb-flow.fake"
            rgb_build=$("$bin/agentic-rigg-helper" --version | sed -n 's/.*bygg \([0-9]\+\).*/\1/p')
            [[ "$rgb_build" =~ ^[0-9]+$ ]] || { echo "Ukjent testbygg." >&2; exit 2; }
            vars+=("RIGG_RGB_FLOW=sim" "RIGG_RGB_FAKE_SCRIPT=$here/scripts/falsk-rgb.py" "RIGG_RGB_FAKE_BUILD=$rgb_build")
        fi
        backend=sim wargs="--side lys" warm=${RIGG_OPPVARMING:-3}
        ;;
    # Hete-syklusen: 125 s, så 5 min-vinduet har kaldt, 95 °C og 110 °C nå.
    8a-hete) vars=("RIGG_SKJERMBILDER=$dir" "RIGG_PLAN=8a-hete") backend=sim:hete warm=${RIGG_OPPVARMING:-125} ;;
    7b-ekte) vars=("RIGG_BILDER_2C=$dir" "RIGG_6C_PLAN=7b-ekte") wargs="--side kontroller" ;;
    # Fem minutter spill-last i historikken til «Siste 5 minutter».
    7b-sim) vars=("RIGG_BILDER_2C=$dir" "RIGG_6C_PLAN=7b-sim") backend=sim:spill wargs="--side kontroller" warm=${RIGG_OPPVARMING:-320} ;;
    *) echo "Ukjent plan: $plan (se --help)" >&2; exit 2 ;;
    esac
    # Planene fra 2A og 2B finnes bare i debug-bygget (#[cfg(debug_assertions)]):
    # med release-binærene ville vinduet bare stått og ventet.
    pbin=$bin
    case "$plan" in
    2a* | 2b*)
        if [ -z "${RIGG_BIN:-}" ]; then
            pbin=$target/debug
            [ -x "$pbin/agentic-rigg" ] || { echo "Planen $plan trenger debug-bygget (cargo build); hopper over" >&2; continue; }
        fi
        ;;
    esac
    echo "== $plan → $dir ($pbin)"
    set +e
    env -u RIGG_BACKEND "${vars[@]}" ${backend:+RIGG_BACKEND=$backend} RIGG_BIN="$pbin" RIGG_WARM="$warm" RIGG_RIGGD_ARGS="$dargs" RIGG_VINDU_ARGS="$wargs" \
        RIGG_OKT_KEEP=${RIGG_OKT_KEEP:-} \
        "$here/scripts/okt.sh" --bredde 1920x1080 -- bash "$runner"
    rc=$?
    set -e
    case "$plan" in
    7a | 7a-ekte | 1.4.0-ui | rgb-ui | rgb-flow)
        # Hjelperen den falske pkexec startet, er stoppet av planen; sikre det.
        [ -f "$fake/run/agentic-rigg/falsk.pid" ] && { kill "$(cat "$fake/run/agentic-rigg/falsk.pid")" 2>/dev/null || true; }
        find "$fake" -type f | sed "s|$fake/||" | sort >"$dir/falsk-rot-etter.txt"
        if [ -f "$fake/rgb-flow.trace" ]; then cp "$fake/rgb-flow.trace" "$dir/rgb-flow.trace"; fi
        rm -rf -- "$fake" "$hdata"
        ;;
    esac
    [ $rc -eq 0 ] || { echo "Planen $plan feilet ($rc)" >&2; rc_all=$rc; }
done
echo "Skjermbildene ligger i $out"
exit $rc_all
