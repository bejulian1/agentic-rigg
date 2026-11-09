# Runde 6B: skallet med ekte peker i en privat, hodeløs GNOME-økt (okt6b.sh).
# Tar bilder av toppfeltet og vindusknappene (i ro, hover og trykk), sidefeltet
# (utvidet og sammenlagt) og Innstillinger › Om, og de samme knappene i
# Ubuntus Innstillinger (libadwaita) og Klipp til sammenligning.
# Bruk (inne i økta): RIGG_6B_UT=MAPPE RIGG_BIN=MAPPE RIGG_6B_NAVN=for|etter bash kjor.sh
set -u
OUT=$RIGG_6B_UT
BIN=$RIGG_BIN
N=${RIGG_6B_NAVN:-etter}
T=$([ -n "${RIGG_6B_MORK:-}" ] && echo mork || echo lys)
mkdir -p "$OUT" "$XDG_CONFIG_HOME/agentic-rigg-dev"
call() { gdbus call --session -d org.rigg6b.Skall -o /org/rigg6b/Skall -m "org.rigg6b.Skall.$1" "${@:2}"; }
shot() { sleep "${2:-0.5}"; call Shot "$OUT/$1.png" >/dev/null; }
area() { sleep "${6:-0.5}"; call ShotArea "$1" "$2" "$3" "$4" "$OUT/$5.png" >/dev/null; }
move() { call Move "$1" "$2" >/dev/null; }
key() { call Key "$1" "${2:-0}" >/dev/null; sleep "${3:-0.5}"; }
# Rammen til første vindu med klassen: «x y b h».
frame() { call Windows | python3 -c "
import sys, json, ast
raw = ast.literal_eval(sys.stdin.read().strip())[0]
d = json.loads(raw)
for w in d['windows']:
    if '$1' in (w['cls'] or '') or '$1' in (w['title'] or ''):
        print(*w['frame']); break
"; }
# Vindusknappene sitter øverst til høyre: midten av knapp i (1 = lukk, 2 = maksimer, 3 = minimer).
# Avstanden fra kanten måles i bildet; her bare et utsnitt rundt dem.
controls() {
    local name=$1 cls=$2 head=$3
    read -r x y w h < <(frame "$cls")
    echo "$name: ramme $x $y $w $h" >>"$OUT/rammer.txt"
    move $((x + w / 2)) $((y + h / 2))
    area $((x + w - 360)) "$y" 360 "$head" "$name-knapper-$T"
    # Hover og trykk på hver knapp: finn midten fra bildet (kjor.sh skriver
    # koordinatene; finn_knapper.py leter etter sirklene).
    python3 "$(dirname "$0")/finn_knapper.py" "$OUT/$name-knapper-$T.png" >"$OUT/$name-knapper-$T.txt" 2>&1
    local i=0
    while read -r cx cy; do
        i=$((i + 1))
        move $((x + w - 360 + cx)) $((y + cy))
        area $((x + w - 360)) "$y" 360 "$head" "$name-knapper-hover$i-$T" 0.6
        # Trykk bare på maksimer (2), og dra ut før slipp, så knappen ikke gjør
        # noe (lukk ble trykket i første forsøk).
        if [ "$i" = 2 ]; then
            call Button 1 >/dev/null
            area $((x + w - 360)) "$y" 360 "$head" "$name-knapper-trykk$i-$T" 0.4
            move $((x + w / 2)) $((y + h / 2))
            sleep 0.3
            call Button 0 >/dev/null
            sleep 0.6
            # Å dra fra en vindusknapp flytter vinduet (som i libadwaita); sett det tilbake.
            echo "$name: etter trykk, dra og slipp: $(frame "$cls")" >>"$OUT/rammer.txt"
            call Place "$cls" "$x" "$y" "$w" "$h" >/dev/null
            sleep 0.8
        fi
    done < <(grep -E '^[0-9]+ [0-9]+$' "$OUT/$name-knapper-$T.txt")
    move $((x + w / 2)) $((y + h / 2))
}

"$BIN/agentic-riggd" --backend sim --grace-ms 600000 2>"$OUT/riggd-$T.log" &
DPID=$!
sleep 2
theme=$([ "$T" = mork ] && echo dark || echo light)
echo "{\"theme\": \"$theme\", \"motion\": \"off\", \"lastPage\": \"oversikt\", \"window\": {\"width\": 1440, \"height\": 900, \"maximized\": false}}" >"$XDG_CONFIG_HOME/agentic-rigg-dev/ui.json"
"$BIN/agentic-rigg" 2>"$OUT/rigg-$T.log" &
VPID=$!
sleep 4
call HideOverview >/dev/null
sleep 1
call Place "Agentic Rigg" 100 60 1440 900 >/dev/null
sleep 1
shot "$N-hele-$T"
read -r x y w h < <(frame "Agentic Rigg")
area "$x" "$y" "$w" 42 "$N-toppfelt-$T"
area "$x" "$y" 236 "$h" "$N-sidefelt-$T"
controls "$N-rigg" "Agentic Rigg" 42
# Ctrl+B: sammenlagt sidefelt.
key 0x62 1 1
area "$x" "$y" 56 "$h" "$N-sidefelt-sammenlagt-$T"
key 0x62 1 1
# Lav vindushøyde.
call Place "Agentic Rigg" 100 60 1100 560 >/dev/null
sleep 1.2
read -r x y w h < <(frame "Agentic Rigg")
area "$x" "$y" "$w" "$h" "$N-lav-560-$T"
call Place "Agentic Rigg" 100 60 1440 900 >/dev/null
sleep 1
# Innstillinger › Om: nytt vindu med --innstillinger om (den andre forekomsten
# sender bare --side videre).
kill "$VPID" 2>/dev/null
wait "$VPID" 2>/dev/null
"$BIN/agentic-rigg" --innstillinger om 2>>"$OUT/rigg-$T.log" &
VPID=$!
sleep 4
call Place "Agentic Rigg" 100 60 1440 900 >/dev/null
sleep 1
read -r x y w h < <(frame "Agentic Rigg")
area "$x" "$y" "$w" "$h" "$N-om-$T"
controls "$N-innstillinger" "Agentic Rigg" 42
key 0xff1b 0 0.6
kill "$VPID" 2>/dev/null
wait "$VPID" 2>/dev/null
kill -TERM "$DPID" 2>/dev/null
wait "$DPID" 2>/dev/null

# Sammenligning: Ubuntus Innstillinger (libadwaita) og Klipp.
if [ -n "${RIGG_6B_SAMMENLIGN:-}" ]; then
    gnome-control-center 2>"$OUT/gcc-$T.log" &
    GPID=$!
    sleep 5
    call HideOverview >/dev/null
    call Place gnome-control-center 100 60 1100 760 >/dev/null || call Place Innstillinger 100 60 1100 760 >/dev/null
    sleep 1.5
    shot "ubuntu-innstillinger-hele-$T"
    controls ubuntu-innstillinger "Innstillinger" 48
    kill "$GPID" 2>/dev/null
    wait "$GPID" 2>/dev/null
    if [ -n "${RIGG_6B_KLIPP:-}" ]; then
        "$RIGG_6B_KLIPP" 2>"$OUT/klipp-$T.log" &
        KPID=$!
        sleep 5
        call HideOverview >/dev/null
        call Windows >"$OUT/klipp-vinduer-$T.txt"
        shot "klipp-hele-$T"
        controls klipp Klipp 48
        kill "$KPID" 2>/dev/null
        wait "$KPID" 2>/dev/null
    fi
fi
call Windows >"$OUT/vinduer-slutt-$T.txt" || true
