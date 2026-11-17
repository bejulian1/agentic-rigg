#!/usr/bin/env bash
# Installerer Agentic Rigg for denne brukeren under ~/.local: begge binærene,
# ikonene (alle variantene), .desktop-fila, systemd --user-enheten,
# lisensene og README under ~/.local/share/doc/agentic-rigg/, og filene til
# root-hjelperen under ~/.local/share/agentic-rigg/hjelper/ (1.2.0). Hjelperen
# AKTIVERES IKKE her: det gjør du selv med bryteren i Innstillinger › Styring.
# Skriptet bygger ingenting, bruker ikke sudo og slår ingenting på ved
# innlogging (arkitektur.md kap. 12, docs/produksjon/ikon.md kap. 7).
#
# Bruk: ./install.sh [--bin-dir STI]
#
# Uten --bin-dir brukes ${CARGO_TARGET_DIR:-~/.cache/rigg-build}/release.
set -euo pipefail

app_id=io.github.AgenticRigg
unit=agentic-riggd.service
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bin_src=

while [ $# -gt 0 ]; do
    case "$1" in
    --bin-dir)
        [ $# -ge 2 ] || { echo "--bin-dir trenger en sti" >&2; exit 2; }
        bin_src=$2
        shift 2
        ;;
    -h | --help)
        sed -n '2,10s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
        exit 0
        ;;
    *)
        echo "Ukjent argument: $1" >&2
        exit 2
        ;;
    esac
done

[ -n "$bin_src" ] || bin_src=${CARGO_TARGET_DIR:-$HOME/.cache/rigg-build}/release
for b in agentic-rigg agentic-riggd agentic-rigg-helper; do
    if [ ! -x "$bin_src/$b" ]; then
        echo "Fant ikke en kjørbar binær: $bin_src/$b" >&2
        echo "Bygg først med: cd app && CARGO_TARGET_DIR=$HOME/.cache/rigg-build cargo build --release" >&2
        exit 1
    fi
done

bin_dir=$HOME/.local/bin
data_dir=${XDG_DATA_HOME:-$HOME/.local/share}
config_dir=${XDG_CONFIG_HOME:-$HOME/.config}
hicolor=$data_dir/icons/hicolor
desktop=$data_dir/applications/$app_id.desktop
unit_dir=$config_dir/systemd/user
doc_dir=$data_dir/doc/agentic-rigg
hjelper_dir=$data_dir/agentic-rigg/hjelper
icons=$here/data/icons

case "$bin_dir" in
*$'\n'*)
    echo "Stien inneholder linjeskift, det støttes ikke i .desktop-filer: $bin_dir" >&2
    exit 1
    ;;
esac

# Verdi i Exec= med anførselstegn (Desktop Entry-spesifikasjonen): \ " ` $
# beskyttes to ganger, og % én gang. Mellomrom er trygge i anførselstegnene.
desktop_exec() {
    local s=$1
    s=${s//\\/\\\\\\\\}
    s=${s//\"/\\\\\"}
    s=${s//\`/\\\\\`}
    s=${s//\$/\\\\\$}
    s=${s//%/%%}
    printf '"%s"' "$s"
}

# 1. Binærene.
install -Dm755 "$bin_src/agentic-rigg" "$bin_dir/agentic-rigg"
install -Dm755 "$bin_src/agentic-riggd" "$bin_dir/agentic-riggd"

# 2. Ikonene: standard, det symbolske og hver variant under sitt eget navn i
#    alle fem størrelsene. Et nytt navn er det som får GNOME Shell til å bytte
#    ikon uten utlogging (ikon.md kap. 7), så filene skrives aldri over i
#    stedet for å bytte navn.
for size in scalable 48x48 32x32 24x24 16x16; do
    install -Dm644 "$icons/hicolor/$size/apps/$app_id.svg" "$hicolor/$size/apps/$app_id.svg"
done
install -Dm644 "$icons/hicolor/symbolic/apps/$app_id-symbolic.svg" "$hicolor/symbolic/apps/$app_id-symbolic.svg"
n_variants=0
while IFS=$'\t' read -r id name; do
    [ -n "$id" ] || continue
    for pair in 128:scalable 48:48x48 32:32x32 24:24x24 16:16x16; do
        src=$icons/varianter/$id/${pair%%:*}.svg
        dst=$hicolor/${pair#*:}/apps/$name.svg
        [ "$id" = standard ] && continue
        install -Dm644 "$src" "$dst"
    done
    n_variants=$((n_variants + 1))
done < <(python3 -I -c '
import json, sys
v = json.load(open(sys.argv[1]))
for x in v["varianter"]:
    print(x["id"] + "\t" + x["ikonnavn"])
' "$icons/varianter/varianter.json")
# GTK og Shell leser temaet på nytt når mappa er nyere. Ingen ikonbuffer:
# en gyldig buffer skjuler ikoner andre apper legger til senere (ikon.md 7.4).
touch "$hicolor"

# 3. .desktop-fila. Valgt variant (appIcon i ui.json) beholdes.
icon=$app_id
ui=$config_dir/agentic-rigg/ui.json
if [ -f "$ui" ]; then
    chosen=$(python3 -I -c '
import json, sys
try:
    v = json.load(open(sys.argv[1])).get("appIcon", "standard")
except Exception:
    v = "standard"
print(v if isinstance(v, str) and v.replace("-", "").isalnum() else "standard")
' "$ui")
    if [ "$chosen" != standard ] && [ -f "$hicolor/scalable/apps/$app_id-$chosen.svg" ]; then
        icon=$app_id-$chosen
    fi
fi
exec_line=$(desktop_exec "$bin_dir/agentic-rigg")
mkdir -p "$(dirname -- "$desktop")"
sed -e "s|@EXEC@|$exec_line|g" -e "s|@ICON@|$icon|g" "$here/data/$app_id.desktop.in" >"$desktop.tmp"
mv -f "$desktop.tmp" "$desktop"

# 4. systemd --user-enheten. Aktiveres ikke: vinduet starter den ved behov.
install -Dm644 "$here/data/systemd/$unit" "$unit_dir/$unit"
# Lisensene (ISC for ikonene, MIT for resten) følger alltid med. Om-siden viser
# den samme teksten og peker hit.
install -Dm644 "$here/data/LISENSER.md" "$doc_dir/LISENSER.md"
if [ -f "$here/README.md" ]; then
    install -Dm644 "$here/README.md" "$doc_dir/README.md"
fi
# RIGG_INSTALL_NO_SYSTEMD=1 (testene med midlertidig HOME) rører ikke systemd.
if [ -z "${RIGG_INSTALL_NO_SYSTEMD:-}" ] && command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    systemctl --user daemon-reload || true
    # Kjører den fra før (ny versjon), startes den på nytt. Ellers skjer ingenting.
    systemctl --user try-restart "$unit" || true
fi

# 5. Root-hjelperen: bare filene, til bryteren i Innstillinger › Styring.
#    Ingenting legges utenfor ~/.local, og ingenting kjøres her. Bryteren
#    kjører «pkexec $hjelper_dir/aktiver.sh», som kopierer dem dit bare root
#    kan skrive (docs/produksjon/hjelper-sikkerhet.md).
install -Dm755 "$bin_src/agentic-rigg-helper" "$hjelper_dir/agentic-rigg-helper"
for f in aktiver.sh deaktiver.sh trinn.sh rgb-aktiver.sh rgb-deaktiver.sh rgb-avklar.sh rgb-gjenopprett.sh rgb-admin.py; do
    install -Dm755 "$here/data/hjelper/$f" "$hjelper_dir/$f"
done
for f in agentic-rigg-helper.service io.github.AgenticRigg.helper.policy agentic-rigg-rgb.service io.github.AgenticRigg.rgb.policy; do
    install -Dm644 "$here/data/hjelper/$f" "$hjelper_dir/$f"
done

# 6. Programlisten.
if command -v update-desktop-database >/dev/null; then
    update-desktop-database --quiet "$(dirname -- "$desktop")" || true
fi

echo "Agentic Rigg er installert:"
echo "  $bin_dir/agentic-rigg"
echo "  $bin_dir/agentic-riggd"
echo "  $desktop (Icon=$icon)"
echo "  $hicolor/…/apps/$app_id*.svg ($n_variants varianter)"
echo "  $unit_dir/$unit (ikke slått på ved innlogging)"
echo "  $doc_dir/LISENSER.md"
echo "  $hjelper_dir/ (hjelperen for endring, ikke aktivert)"
case ":$PATH:" in
*":$bin_dir:"*) ;;
*) echo "Merk: $bin_dir er ikke i PATH, så «agentic-rigg» virker bare med full sti." ;;
esac
