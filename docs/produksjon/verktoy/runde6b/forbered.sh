# Runde 6B: kjøres i den private økta før gnome-shell starter (okt6b.sh med
# OKT_FORBERED=denne fila). Legger inn testutvidelsen (virtuell peker og
# tastatur, skjermbilder) og velger Ubuntus tema slik brukeren har det.
# RIGG_6B_MORK=1 gir Yaru-blue-dark og prefer-dark, RIGG_6B_SKALA=2 dobbel skala.
set -u
mkdir -p "$XDG_DATA_HOME/gnome-shell/extensions"
cp -r "$(dirname "$0")/ext/rigg-6b@agentic" "$XDG_DATA_HOME/gnome-shell/extensions/"
gsettings set org.gnome.shell enabled-extensions "['rigg-6b@agentic']"
gsettings set org.gnome.shell disable-extension-version-validation true
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
gsettings set org.gnome.desktop.screensaver lock-enabled false
gsettings set org.gnome.desktop.notifications show-banners false
gsettings set org.gnome.desktop.wm.preferences button-layout ':minimize,maximize,close'
if [ -n "${RIGG_6B_MORK:-}" ]; then
    gsettings set org.gnome.desktop.interface gtk-theme 'Yaru-blue-dark'
    gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'
else
    gsettings set org.gnome.desktop.interface gtk-theme 'Yaru-blue'
    gsettings set org.gnome.desktop.interface color-scheme 'default'
fi
gsettings set org.gnome.desktop.interface accent-color 'blue' 2>/dev/null || true
[ -n "${RIGG_6B_SKALA:-}" ] && gsettings set org.gnome.desktop.interface scaling-factor "$RIGG_6B_SKALA"
true
