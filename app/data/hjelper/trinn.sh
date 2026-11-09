#!/bin/bash
# Agentic Rigg · godkjenner eller trekker tilbake ett trinn for root-hjelperen
# (runde 7A). Kjøres av BRUKEREN via pkexec, etter et eget bekreftelsesspørsmål
# i Innstillinger › Styring:
#
#     pkexec /usr/local/libexec/agentic-rigg/trinn.sh effekt på
#
# Polkit spør om passordet hver gang et trinn godkjennes (én handling per
# trinn, valgt ut fra første argument). Hjelperen leser helper.json på nytt
# innen ett sekund. Trekkes et trinn tilbake, går kortet til standard.
#
# Bruk: trinn.sh effekt|vifter|klokker|lys på|av [--root DIR]
set -euo pipefail
umask 022

feil() {
    echo "trinn.sh: $1" >&2
    exit "${2:-1}"
}

[ $# -ge 2 ] || feil "Bruk: trinn.sh effekt|vifter|klokker|lys på|av [--root DIR]" 2
TRINN=$1
VALG=$2
shift 2
ROOT=/
while [ $# -gt 0 ]; do
    case "$1" in
    --root) [ $# -ge 2 ] || feil "--root trenger en mappe" 2; ROOT=$2; shift 2 ;;
    *) feil "Ukjent argument: $1" 2 ;;
    esac
done
case "$TRINN" in effekt | vifter | klokker | lys) ;; *) feil "Ukjent trinn: $TRINN" 2 ;; esac
case "$VALG" in på | av) ;; *) feil "Andre argument må være på eller av" 2 ;; esac

EKTE=0
if [ "$ROOT" = / ]; then
    EKTE=1
    [ "$(id -u)" -eq 0 ] || feil "Må kjøres som root (via pkexec)."
    ROOT=
fi
KONF="$ROOT/etc/agentic-rigg/helper.json"
[ -f "$KONF" ] || feil "Endring av skjermkortet er ikke slått på (fant ikke helper.json)."

# Fila er alltid på ett format (skrevet av aktiver.sh og dette skriptet).
LINJE=$(head -c 4096 "$KONF")
LISTE=$(printf '%s' "$LINJE" | sed -n 's/.*"trinn":\[\([^]]*\)\].*/\1/p' | tr -d '" ')
printf '%s' "$LINJE" | grep -q '"trinn":\[' || feil "helper.json har ikke forventet format."
NY=""
for t in effekt vifter klokker lys; do
    med=0
    case ",$LISTE," in *",$t,"*) med=1 ;; esac
    [ "$t" = "$TRINN" ] && { [ "$VALG" = på ] && med=1 || med=0; }
    [ $med -eq 1 ] && NY="$NY${NY:+,}\"$t\""
done
TMP=$(mktemp "$(dirname "$KONF")/.helper.json.XXXXXX")
printf '%s\n' "$LINJE" | sed "s/\"trinn\":\[[^]]*\]/\"trinn\":[$NY]/" >"$TMP"
chmod 0644 "$TMP"
if [ $EKTE -eq 1 ]; then chown root:root "$TMP"; fi
mv -f "$TMP" "$KONF"
echo "Trinnet $TRINN er $( [ "$VALG" = på ] && echo godkjent || echo trukket tilbake ). Godkjente trinn: [${NY//\"/}]"
