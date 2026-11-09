# Kjøres i den private økta etter at Shell er klar. Venter på $OKT_BASE/stopp.
set -u
S=$(dirname "$0")
{
  for v in DBUS_SESSION_BUS_ADDRESS DBUS_SYSTEM_BUS_ADDRESS WAYLAND_DISPLAY HOME XDG_RUNTIME_DIR XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME XDG_CACHE_HOME XDG_SESSION_TYPE XDG_CURRENT_DESKTOP RIGG_PROFIL LANG PATH; do
    printf 'export %s=%q\n' "$v" "${!v}"
  done
} > "$OKT_BASE/env.sh"
ATTRAPP_LOGG="$OKT_BASE/attrapp.log" python3 -I "$S/systemd_attrapp.py" 2>>"$OKT_BASE/attrapp.log" &
mkdir -p "$XDG_CONFIG_HOME/systemd/user" "$XDG_CONFIG_HOME/agentic-rigg-dev"
sed "s|^ExecStart=.*|ExecStart=$RIGGD_BIN|" "$S/../../../../app/data/systemd/agentic-riggd.service" > "$XDG_CONFIG_HOME/systemd/user/agentic-riggd-dev.service"
echo '{"background": true, "backgroundIntervalMs": 2000}' > "$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
echo klar > "$OKT_BASE/klar"
while [ ! -e "$OKT_BASE/stopp" ]; do sleep 0.5; done
pkill -TERM -f "$S/systemd_attrapp.py" 2>/dev/null
exit 0
