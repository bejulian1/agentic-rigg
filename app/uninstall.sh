#!/usr/bin/env bash
# Fjerner det install.sh la inn. Rører ikke innstillingene, historikken eller
# loggene dine. Skriver ut hvor de ligger og hvordan de slettes for hånd.
#
# Aktive hjelpere må deaktiveres først. Inaktive systemkomponenter fjernes
# separat av administrator; de hindrer ikke avinstallasjon av brukerappen.
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
if [ -e "$hjelper_rot/etc/systemd/system/agentic-rigg-card.service" ] || [ -e "$hjelper_rot/etc/agentic-rigg/helper.json" ] || [ -e "$hjelper_rot/etc/systemd/system/agentic-rigg-helper.service" ] || [ -S "$hjelper_rot/run/agentic-rigg/helper.sock" ]; then
    hjelper_aktiv=1
fi
if [ -e "$hjelper_rot/etc/agentic-rigg/rgb.json" ] ||
   [ -e "$hjelper_rot/etc/systemd/system/agentic-rigg-rgb.service" ] ||
   [ -e "$hjelper_rot/etc/systemd/system/agentic-rigg-rgb.service.d/card.conf" ] ||
   [ -e "$hjelper_rot/run/systemd/system/agentic-rigg-rgb.service.d/card.conf" ] ||
   [ -e "$hjelper_rot/run/systemd/system/agentic-rigg-rgb.service.d/50-agentic-rigg-card.conf" ] ||
   [ -S "$hjelper_rot/run/agentic-rigg-rgb/helper.sock" ] ||
   [ -e "$hjelper_rot/usr/local/libexec/agentic-rigg-rgb-helper" ] ||
   { [ -d "$hjelper_rot/var/lib/agentic-rigg-rgb" ] &&
     [ ! -f "$hjelper_rot/var/lib/agentic-rigg-rgb.inactive" ]; }; then
    cat >&2 <<'EOF'
Slå av tilgangen i Innstillinger › Skjermkortstyring før avinstallasjon.
Åpne RGB-lys for å avklare eventuelle tidligere endringer. En gjenstående
journal og det lagrede lyset bevares når du slår av. Se RGB-HJELPER.md
for arkivering ved fjernet kort. Etter deaktivering og avklaring:
sudo /usr/local/libexec/agentic-rigg-system/system-install --uninstall
EOF
    exit 3
fi

if [ "$hjelper_aktiv" -eq 1 ]; then
    echo 'GPU-tilgangen er fortsatt aktiv. Slå den av i Innstillinger › Skjermkortstyring før avinstallasjon.' >&2
    exit 3
fi
for rest in usr/local/libexec/agentic-rigg{,-system,-rgb-v2,-helper} usr/share/polkit-1/actions/io.github.AgenticRigg.{helper,rgb.v2}.policy var/lib/agentic-rigg{,-rgb}; do
    if [ -e "$hjelper_rot/$rest" ]; then
        printf '\nOBS: Systemkomponenter eller grunnopptak gjenstår. Fjern dem separat etter deaktivering:\n  sudo /usr/local/libexec/agentic-rigg-system/system-install --uninstall\n\n' >&2
        break
    fi
done

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
rm -f -- "$data_dir/agentic-rigg/SYSTEMFILER.sha256"
rmdir --ignore-fail-on-non-empty -- "$data_dir/agentic-rigg" 2>/dev/null || true
if [ -d "$hicolor" ]; then
    find "$hicolor" -path '*/apps/*' -name "$app_id*.svg" -delete
    touch "$hicolor"
fi
if [ -d "$data_dir/applications" ] && command -v update-desktop-database >/dev/null; then
    update-desktop-database --quiet "$data_dir/applications" || true
fi

echo "Agentic HW Monitor er fjernet."
echo "Innstillingene og historikken er ikke rørt:"
printf '  %s\n' "$config_dir/agentic-rigg/" "$state_dir/agentic-rigg/"
echo "Slett dem selv med:"
printf '  rm -r %q %q\n' "$config_dir/agentic-rigg" "$state_dir/agentic-rigg"
