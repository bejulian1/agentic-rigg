# Kjøres i den private økta før gnome-shell starter.
# Runde 3B: forberedelse av den private økta (okt3b.sh med OKT_FORBERED=denne fila).
set -u
mkdir -p "$XDG_DATA_HOME/gnome-shell/extensions" "$XDG_DATA_HOME/icons/hicolor/symbolic/apps"
cp -r "$(dirname "$0")/ext/rigg-3b@agentic" "$XDG_DATA_HOME/gnome-shell/extensions/"
cp "$(dirname "$0")/../../../../app/data/icons/hicolor/symbolic/apps/io.github.AgenticRigg-symbolic.svg" "$XDG_DATA_HOME/icons/hicolor/symbolic/apps/"
gsettings set org.gnome.shell enabled-extensions "['rigg-3b@agentic', 'ubuntu-appindicators@ubuntu.com']"
gsettings set org.gnome.shell disable-extension-version-validation true
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
gsettings set org.gnome.desktop.interface clock-show-weekday false
