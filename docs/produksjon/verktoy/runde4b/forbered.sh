# Runde 4B: kjøres i den private økta før gnome-shell starter (okt4b.sh med
# OKT_FORBERED=denne fila). Legger inn testutvidelsen med det virtuelle tastaturet.
set -u
mkdir -p "$XDG_DATA_HOME/gnome-shell/extensions"
cp -r "$(dirname "$0")/ext/rigg-4b@agentic" "$XDG_DATA_HOME/gnome-shell/extensions/"
gsettings set org.gnome.shell enabled-extensions "['rigg-4b@agentic']"
gsettings set org.gnome.shell disable-extension-version-validation true
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
