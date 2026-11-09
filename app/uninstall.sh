#!/usr/bin/env bash
# Fjerner det install.sh la inn. Rører ikke innstillingene, historikken eller
# loggene dine. Skriver ut hvor de ligger og hvordan de slettes for hånd.
#
# Root-hjelperen (1.2.0) fjernes IKKE herfra, for det krever root. Er den
# fortsatt aktiv, sier skriptet fra og viser hvordan den slås av. Kopien av
# deaktiver.sh i /usr/local/libexec/agentic-rigg/ blir liggende til da.
set -euo pipefail

app_id=io.github.AgenticRigg
unit=agentic-riggd.service
data_dir=${XDG_DATA_HOME:-$HOME/.local/share}
config_dir=${XDG_CONFIG_HOME:-$HOME/.config}
state_dir=${XDG_STATE_HOME:-$HOME/.local/state}
hicolor=$data_dir/icons/hicolor
desktop=$data_dir/applications/$app_id.desktop
unit_file=$config_dir/systemd/user/$unit

# 0. Root-hjelperen. RIGG_HJELPER_ROT er en falsk rot (bare testene).
hjelper_rot=${RIGG_HJELPER_ROT:-}
hjelper_aktiv=0
if [ -e "$hjelper_rot/etc/agentic-rigg/helper.json" ] || [ -e "$hjelper_rot/etc/systemd/system/agentic-rigg-helper.service" ] || [ -S "$hjelper_rot/run/agentic-rigg/helper.sock" ]; then
    hjelper_aktiv=1
fi

# 1. Tjenesten: stopp og slå av bare vår enhet.
if [ -z "${RIGG_INSTALL_NO_SYSTEMD:-}" ] && command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    systemctl --user stop "$unit" 2>/dev/null || true
    systemctl --user disable "$unit" 2>/dev/null || true
fi
rm -f -- "$unit_file"
if [ -z "${RIGG_INSTALL_NO_SYSTEMD:-}" ] && command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    systemctl --user daemon-reload || true
fi

# 2. Binærene, .desktop-fila og alle ikonene våre (også variantene).
rm -f -- "$HOME/.local/bin/agentic-rigg" "$HOME/.local/bin/agentic-riggd" "$desktop"
rm -rf -- "$data_dir/doc/agentic-rigg"
rm -rf -- "$data_dir/agentic-rigg/hjelper"
rmdir --ignore-fail-on-non-empty -- "$data_dir/agentic-rigg" 2>/dev/null || true
if [ -d "$hicolor" ]; then
    find "$hicolor" -path '*/apps/*' -name "$app_id*.svg" -delete
    touch "$hicolor"
fi
if [ -d "$data_dir/applications" ] && command -v update-desktop-database >/dev/null; then
    update-desktop-database --quiet "$data_dir/applications" || true
fi

echo "Agentic Rigg er fjernet."
echo "Innstillingene og historikken er ikke rørt:"
printf '  %s\n' "$config_dir/agentic-rigg/" "$state_dir/agentic-rigg/"
echo "Slett dem selv med:"
printf '  rm -r %q %q\n' "$config_dir/agentic-rigg" "$state_dir/agentic-rigg"
if [ $hjelper_aktiv -eq 1 ]; then
    echo
    echo "OBS: Hjelperen for endring av skjermkortet er fortsatt aktiv (root-tjenesten"
    echo "agentic-rigg-helper). Den fjernes ikke av dette skriptet, fordi det krever root."
    echo "Slå den av med:"
    echo "  pkexec /usr/local/libexec/agentic-rigg/deaktiver.sh"
    exit 3
fi
