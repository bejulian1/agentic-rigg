# Kjøres inne i okt.sh. Env: RIGG_BIN, RUNS, RENDERERS, LUKK_MS
set -u
mkdir -p "$XDG_CONFIG_HOME/agentic-rigg-dev"
echo '{"background": true, "backgroundIntervalMs": 2000}' >"$XDG_CONFIG_HOME/agentic-rigg-dev/tjeneste.json"
"$RIGG_BIN/agentic-riggd" 2>"$OKT_BASE/riggd.log" &
d=$!
sleep 4
ps_now() { nvidia-smi --query-gpu=pstate --format=csv,noheader 2>/dev/null; }
for r in $RENDERERS; do
  echo "# renderer=$r pstate_før=$(ps_now)"
  for i in $(seq 1 "$RUNS"); do
    log=$(mktemp)
    t0=${EPOCHREALTIME/[.,]/}
    case "$r" in
      default) envs=() ;;
      nowarm) envs=(RIGG_GL_FORVARM=0) ;;
      theme-*) envs=(RIGG_GL_FORVARM=0 GTK_THEME=${r#theme-}) ;;
      warm*) envs=(RIGG_GL_FORVARM=${r#warm}) ;;
      *) envs=(GSK_RENDERER=$r) ;;
    esac
    env "${envs[@]}" RIGG_TIMING=1 RIGG_TIMING_LUKK=${LUKK_MS:-800} "$RIGG_BIN/agentic-rigg" 2>"$log"
    t1=${EPOCHREALTIME/[.,]/}
    awk -v t0="$t0" -v t1="$t1" -v run="$i" -v r="$r" '
      /RIGG_TIMING mark=/ { split($2,a,"="); split($4,b,"="); m[a[2]]=(b[2]/1000 - t0)/1000 }
      /renderer=/ { for(k=1;k<=NF;k++) if ($k ~ /^renderer=/) rn=substr($k,10) }
      END { printf "run=%s r=%s gsk=%s", run, r, rn;
            n=split("main gresource gl_warm gl_warm_joined command_line prefs css shell page present realize layout paint first_frame hello telemetry close close_request unmap destroy exit", ks, " ");
            for(k=1;k<=n;k++) if (ks[k] in m) printf " %s=%.1f", ks[k], m[ks[k]];
            if ("close" in m) printf " lukk_unmap=%.1f lukk_exit=%.1f", m["unmap"]-m["close"], (t1-t0)/1000 - m["close"];
            printf "\n" }' "$log"
    grep -v RIGG_TIMING "$log" | head -3
    rm -f "$log"
    sleep 0.7
  done
  echo "# renderer=$r pstate_etter=$(ps_now)"
done
kill -TERM "$d"; wait "$d" 2>/dev/null
