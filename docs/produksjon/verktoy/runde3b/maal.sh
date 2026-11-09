# Kjøres med miljøet til den private økta. maal.sh NAVN BIN TAKT_MS [tom-buss]
set -u
navn=$1 bin=$2 takt=$3 tom=${4:-}

OUT=$MAAL_UT
echo "{\"background\": true, \"backgroundIntervalMs\": $takt}" > "$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
if [ -n "$tom" ]; then
    a=$(dbus-daemon --session --fork --print-address=1 --print-pid=1 --nopidfile)
    addr=$(echo "$a" | sed -n 1p); bpid=$(echo "$a" | sed -n 2p)
    DBUS_SESSION_BUS_ADDRESS=$addr "$bin" > "$OUT/$navn.log" 2>&1 &
else
    "$bin" > "$OUT/$navn.log" 2>&1 &
fi
pid=$!
sleep 20
t0=$(awk '{print $14+$15}' /proc/$pid/stat); s0=$(date +%s.%N)
sleep 120
t1=$(awk '{print $14+$15}' /proc/$pid/stat); s1=$(date +%s.%N)
hz=$(getconf CLK_TCK)
cpu=$(echo "scale=3; 100*($t1-$t0)/$hz/($s1-$s0)" | bc)
mem=$(awk '/^Rss:|^Pss:|^Private_Dirty:/{printf "%s=%s ", $1, $2}' /proc/$pid/smaps_rollup)
thr=$(ls /proc/$pid/task | wc -l)
fds=$(ls /proc/$pid/fd | wc -l)
names=$(gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.ListNames | tr ',' '\n' | grep -c StatusNotifierItem)
echo "$navn takt=$takt cpu=${cpu}% $mem tråder=$thr fd=$fds sni_i_hovedbussen=$names" | tee -a "$OUT/resultat.txt"
kill -TERM $pid; wait $pid
[ -n "$tom" ] && kill $bpid
