# Runde 4B: tastaturrunden. Kjøres inne i den private økta (okt4b.sh). Ekte
# tastetrykk fra et virtuelt tastatur (testutvidelsen rigg-4b@agentic), skjerm-
# bilder fra Shell, og vinduets fokuslogg (RIGG_FOKUS=1, bare utviklingsprofilen).
# Bruk: RIGG_TAST_UT=MAPPE RIGG_BIN=MAPPE bash tast.sh
set -u
OUT=$RIGG_TAST_UT
BIN=$RIGG_BIN
mkdir -p "$OUT" "$XDG_CONFIG_HOME/agentic-rigg-dev"
STEG=$OUT/steg.txt
: >"$STEG"
call() { gdbus call --session -d org.rigg4b.Tast -o /org/rigg4b/Tast -m "org.rigg4b.Tast.$1" "${@:2}" >/dev/null; }
# Siste fokuslinje fra vinduet.
fokus() { grep '^RIGG_FOKUS' "$LOG" | tail -1 | cut -c12-; }
key() { call Key "$1" "${2:-0}"; sleep "${3:-0.35}"; }
tab() { local n=$1; for _ in $(seq 1 "$n"); do key 0xff09 0 0.25; echo "  Tab → $(fokus)" >>"$STEG"; done; }
shot() { sleep 0.4; call Shot "$OUT/$1.png"; echo "[bilde] $1.png" >>"$STEG"; }
step() { echo "## $*" >>"$STEG"; }

start_vindu() {
    LOG=$OUT/rigg-$1.log
    RIGG_FOKUS=1 "$BIN/agentic-rigg" 2>"$LOG" &
    VPID=$!
    sleep 4
    call HideOverview
    sleep 1
    echo "Fokusvindu: $(gdbus call --session -d org.rigg4b.Tast -o /org/rigg4b/Tast -m org.rigg4b.Tast.Focus)" >>"$STEG"
}

"$BIN/agentic-riggd" --backend sim --grace-ms 600000 2>"$OUT/riggd.log" &
DPID=$!
sleep 2

# ---- Runde 1: lyst tema, standard ----
echo '{"theme": "light", "motion": "off", "window": {"width": 1440, "height": 900, "maximized": false}}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/ui.json"
start_vindu 1
sider=(oversikt sensorer grafer grafikkort system kontroller viftekurve lys)
for i in 1 2 3 4 5 6 7 8; do
    p=${sider[$((i - 1))]}
    step "Side $i ($p): tast $i, så Tab 1, 5 og 10 ganger"
    key $((0x30 + i)) 0 1.2
    tab 1; shot "$p-tab01"
    tab 4; shot "$p-tab05"
    tab 5; shot "$p-tab10"
    # Esc tilbake til sidefeltet: fokus skal ikke bli hengende i en popover.
    key 0xff1b
done

step "Sensorer: Ctrl+F, skriv «temp», Esc tømmer, Esc igjen"
key 0x32 0 1
key 0x66 1; echo "  Ctrl+F → $(fokus)" >>"$STEG"
call Type temp; sleep 0.6; shot sensorer-filter-temp
key 0xff1b; shot sensorer-filter-esc1
key 0xff1b; shot sensorer-filter-esc2; echo "  Esc Esc → $(fokus)" >>"$STEG"

step "Kontroller: Tab til «Lag utkast», mellomrom, Tab gjennom gliderne"
key 0x36 0 1.2
for _ in $(seq 1 25); do
    key 0xff09 0 0.2
    f=$(fokus); echo "  Tab → $f" >>"$STEG"
    case "$f" in *"Lag utkast"* | *"Gjør radene om"*) break ;; esac
done
key 0x20 0 0.8; shot kontroller-utkast-apnet
tab 1; shot kontroller-utkast-tab1
tab 2; shot kontroller-utkast-tab3
key 0xff53 0 0.3; key 0xff53 0 0.3; shot kontroller-utkast-pil-hoyre
key 0x7a 1 0.6; shot kontroller-utkast-ctrl-z

step "Ctrl+, åpner Innstillinger, Ctrl+F og «tema», Esc, Esc, Ctrl+, lukker"
key 0x2c 1 1; shot innstillinger-ctrl-komma; echo "  → $(fokus)" >>"$STEG"
key 0x66 1; call Type tema; sleep 0.6; shot innstillinger-sok-tema; echo "  → $(fokus)" >>"$STEG"
key 0xff1b; shot innstillinger-sok-esc1
tab 3; shot innstillinger-tab3
key 0xff1b; shot innstillinger-esc2
key 0x2c 1 1; shot innstillinger-lukket

step "Ctrl+B legger sammen sidefeltet og åpner det igjen"
key 0x31 0 0.8
key 0x62 1 0.8; shot ctrl-b-ikonlist
key 0x62 1 0.8; shot ctrl-b-tilbake

step "Ctrl+? viser snarveiene, Tab, Esc lukker"
key 0x3f 3 1; shot snarveier-ctrl-sporsmal
tab 2; shot snarveier-tab2
key 0xff1b; shot snarveier-esc

step "Ctrl+Pluss og Ctrl+0"
key 0x2b 1 0.8; shot ctrl-pluss
key 0x30 1 0.8

step "Ctrl+W lukker vinduet"
key 0x77 1 1.5
if kill -0 "$VPID" 2>/dev/null; then echo "  FEIL: vinduet lever etter Ctrl+W" >>"$STEG"; kill "$VPID"; else echo "  OK: prosessen er avsluttet" >>"$STEG"; fi

# ---- Runde 2: høy kontrast, mørkt ----
echo '{"theme": "dark", "highContrast": true, "motion": "off", "window": {"width": 1440, "height": 900, "maximized": false}}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/ui.json"
start_vindu 2
for i in 1 6; do
    p=${sider[$((i - 1))]}
    step "Høy kontrast, side $i ($p)"
    key $((0x30 + i)) 0 1.2
    tab 1; shot "hk-$p-tab01"
    tab 4; shot "hk-$p-tab05"
    key 0xff1b
done
step "Ctrl+Q avslutter"
key 0x71 1 1.5
if kill -0 "$VPID" 2>/dev/null; then echo "  FEIL: vinduet lever etter Ctrl+Q" >>"$STEG"; kill "$VPID"; else echo "  OK: prosessen er avsluttet" >>"$STEG"; fi

kill -TERM "$DPID"; wait "$DPID" 2>/dev/null
grep -c '^RIGG_FOKUS' "$OUT"/rigg-*.log >>"$STEG"
