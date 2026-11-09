# Bakgrunnstjenesten – API-kontrakt (protokoll 1)

*Agentic Rigg alpha v0.0.7 · bygg 7, 5. november 2026.*

Dette er kontrakten mellom appen (i dag Node-serveren i `server/`, senere Electron) og bakgrunnstjenesten som skal styre skjermkortet. I denne versjonen finnes bare en **simulering** av tjenesten, `daemon-mock/`, som kjører som vanlig bruker og ikke rører maskinen. Den ekte tjenesten er tenkt som en Rust-daemon med root, etter mønsteret til LACT (se `research/linux-gpu-kontroll.md`, kapittel 3 og 6). Den skal snakke nøyaktig denne kontrakten, så appen ikke trenger å endres når mocken byttes ut.

Felles kode for begge sider: `server/daemon-protocol.js` (versjon, feilkoder, grenser, sti til socketen og oversetting av felt). Kontrakttestene står i `test/daemon-contract.test.js`.

## 1. Transport

**Unix-socket med linjeavgrenset JSON (NDJSON), UTF-8.** Hver melding er ett JSON-objekt på én linje, avsluttet med `\n`. Største melding er **65 536 byte** per linje.

Hvorfor linjer og ikke lengdeprefiks:

- JSON kan ikke inneholde et rått linjeskift (det skrives `\n` inne i tekst), så grensen mellom meldinger er entydig.
- LACT gjør det samme (`/run/lactd.sock`, «newline-separated JSON objects»), og det virker godt i praksis.
- Det er enkelt på begge sider: `tokio_util::codec::LinesCodec::new_with_max_length` i Rust og noen få linjer med `node:net` i Node.
- Det kan feilsøkes for hånd: `echo '{"id":1,"op":"hello","args":{"protocol":[1]}}' | ncat -U <socket>`.
- Ulempen (en ondsinnet klient kan sende en endeløs linje) løses med maksgrensen: over 64 KiB uten linjeskift gir `message_too_large`, og forbindelsen lukkes. Lengdeprefiks ville gitt samme beskyttelse, men binær innramming og dårligere feilsøking, uten noen gevinst for meldinger i denne størrelsen.

Socketen:

| | Mock (nå) | Ekte daemon (senere) |
|---|---|---|
| Sti | `$XDG_RUNTIME_DIR/agentic-rigg/mock-<id>.sock`, der `<id>` er en kort hash av datamappa. `AG_DAEMON_SOCKET` overstyrer | `/run/agentic-rigg/daemon.sock` (systemd `RuntimeDirectory=`) |
| Mappe | 0700, eid av brukeren. Sjekkes ved oppstart (ikke symlenke, riktig eier) | 0755, root |
| Socket | 0600 | 0660, `root:agentic-rigg` |
| Kjører som | vanlig bruker | root (systemd), herdet |

Stien er begrenset til 107 byte (`sockaddr_un`). Derfor ligger socketen ikke i datamappa.

## 2. Meldinger

**Forespørsel** (klient → daemon):

```json
{"id": 7, "op": "set_power_limit", "args": {"gpu": "0", "watts": 230}}
```

- `id`: heltall eller tekst (høyst 64 tegn). Svaret har samme `id`. Klienten bestemmer selv; flere forespørsler kan være underveis samtidig.
- `op`: navnet på operasjonen.
- `args`: objekt, kan utelates når operasjonen ikke tar argumenter.

**Svar**:

```json
{"id": 7, "ok": true, "result": { … }}
{"id": 7, "ok": false, "error": {"code": "invalid_argument", "message": "Effektgrensen må være et tall", "details": {"field": "powerLimitW"}}}
```

`message` er norsk tekst som kan vises til brukeren. `details` er maskinlesbart og kan mangle. Når forespørselen ikke kunne leses (ugyldig JSON, mangler id), er `id` `null`.

**Hendelse** (daemon → klient, uten `id`):

```json
{"event": "txn", "data": {"gpu": "0", "txn": "t3", "status": "rolled_back", "reason": "timeout"}}
```

## 3. Versjon og evner: `hello`

Første melding på en ny forbindelse må være `hello`. Alt annet før det gir `hello_required`.

```json
→ {"id":1,"op":"hello","args":{"protocol":[1],"client":"agentic-rigg/0.0.7"}}
← {"id":1,"ok":true,"result":{
    "protocol": 1,
    "session": "s4",
    "daemon": {"name": "agentic-rigg-daemon-mock", "version": "0.0.7", "mock": true, "pid": 12345},
    "capabilities": {
      "gpus": ["0"],
      "ops": ["hello","ping","list_gpus","get_limits","get_state","get_telemetry","subscribe","unsubscribe","apply","set_power_limit","set_clock_offsets","set_temp_target","set_fan","confirm","revert","reset","rgb_get","rgb_set"],
      "events": ["telemetry","state","txn","safety","shutdown"],
      "rgb": {"status": "planned", "note": "Krever brukerens godkjenning (I2C, se research kap. 4.5)."}
    },
    "confirm": {"defaultS": 15, "minS": 10, "maxS": 60},
    "maxMessageBytes": 65536,
    "lastExit": {"at": "2026-11-05T00:12:03.000Z", "clean": true, "reason": "SIGTERM", "fan": "auto"}
  }}
```

- `protocol` i forespørselen er listen over versjoner klienten kan. Daemonen velger én. Ingen felles versjon gir `unsupported_protocol` med `details.supported`.
- Versjonen økes bare ved brudd. Nye operasjoner, nye felt i svar og nye hendelser er ikke brudd; klienten skal ignorere felt den ikke kjenner, og sjekke `capabilities.ops` før den bruker noe nytt.
- `lastExit` sier hvordan forrige kjøring endte. `clean: false` betyr at prosessen ble drept eller krasjet (da har systemd satt viften til auto, se kapittel 8).

## 4. Datatyper

**GpuSettings** (det som kan skrives). Alle felt er valgfrie i en skriving; det som mangler, beholdes.

```json
{
  "powerLimitW": 245,
  "clockOffsets": {"coreMhz": 0, "memMhz": 0},
  "tempTargetC": 83,
  "fan": {
    "mode": "auto",
    "manualPercent": 50,
    "curve": [[30,35],[50,40],[62,50],[72,65],[82,85],[90,100]],
    "minPercent": 35,
    "spindownC": 2,
    "rampUpS": 2,
    "rampDownS": 5,
    "source": "gpu",
    "startStop": {"enabled": false, "startC": 55, "stopC": 48}
  }
}
```

| Felt | Betydning |
|---|---|
| `powerLimitW` | Effektgrense i watt |
| `clockOffsets.coreMhz` | Kjerneklokke-offset (GPC) i MHz |
| `clockOffsets.memMhz` | Minneklokke-offset i **effektiv MHz**, det appen viser. NVML bruker dobbel verdi; daemonen regner om |
| `tempTargetC` | Temperaturmål (akustisk grense) |
| `fan.mode` | `auto` (driveren styrer, som uten appen), `curve` (daemonen følger kurven) eller `manual` (fast fart) |
| `fan.manualPercent` | Fart i `manual` |
| `fan.curve` | 2–10 punkter `[°C, %]`, stigende temperatur, fart som aldri faller |
| `fan.minPercent` | Kurven går aldri under dette |
| `fan.spindownC` | «Senk først etter»: temperaturfall før farten senkes |
| `fan.rampUpS`, `fan.rampDownS` | Reaksjonstid opp og ned |
| `fan.source` | `gpu` eller `cpu` (temperaturen kurven følger) |
| `fan.startStop` | Start/stopp med hysterese. Under `stopC` hviler viften på minste fart til temperaturen når `startC`. `startC` må være minst 3 °C over `stopC` |

Ukjente felt gir `invalid_argument` (skrivefeil skal ikke forsvinne i stillhet). Feil type gir `invalid_argument` med `details.field`.

**Limits** (`get_limits`). Hvert tallfelt har `min`, `max` og `source`:

- `hardware`: grensen kommer fra kortet (NVML).
- `policy`: tjenesten er strengere enn kortet. Da står kortets egen grense i `hardware: {min, max}`. Policy kan bare endres av root i daemonens konfig.
- `daemon`: grensen er tjenestens egen (f.eks. reaksjonstid).

Verdier fra mocken (målt på RTX 2080 med NVML, research kapittel 1):

| Felt | Område | Kilde |
|---|---|---|
| `powerLimitW` | 105–275 W, standard 245 | hardware |
| `clockOffsets.coreMhz` | −200…+300 (kortet: −1000…+1000) | policy |
| `clockOffsets.memMhz` | −1000…+1500 (kortet: −1000…+3000 effektiv, −2000…+6000 i NVML) | policy |
| `tempTargetC` | 65–88 °C, standard 83 | hardware |
| `fan.percent` | 35–100 %, 2 vifter, ingen 0 RPM | hardware |

I tillegg: `fan.curve` (punkter og temperaturområde), `fan.startStopGapC` (3), `safety` (kapittel 8) og `unsupported` (tekster for det kortet ikke kan: låst minneklokke, spenningsoffset, 0 RPM).

**State** (`get_state` og hendelsen `state`):

```json
{
  "gpu": "0",
  "active":    { GpuSettings, det som gjelder nå },
  "confirmed": { GpuSettings, siste bekreftede },
  "pending":   null | {"txn": "t3", "owner": "s4", "timeoutS": 15, "deadline": 1793838895303, "remainingMs": 12000,
                       "changed": ["powerLimitW", "fan.mode"], "clamped": [Clamp, …]},
  "emergency": false,
  "fanOnExit": "auto"
}
```

**Clamp** (en verdi som er klemt):

```json
{"field": "powerLimitW", "requested": 300, "applied": 275, "bound": "max", "source": "hardware"}
{"field": "fan.curve", "points": [0, 3], "applied": [[30,35], …], "bound": "min", "source": "hardware"}
```

**Sample** (telemetri): de samme feltene som appens prøver fra nvidia-smi (`temp`, `gpuUtil`, `coreClock`, `memClock`, `power`, `powerLimit`, `fan`, `vramUsed`, `pstate`, `throttle` …), pluss `fanMode`, `emergency`, `fanRest` og `source`.

## 5. Operasjoner

### Lese

| op | args | result |
|---|---|---|
| `hello` | `protocol: [1]`, `client?` | se kapittel 3 |
| `ping` | – | `{t}` |
| `list_gpus` | – | `[{id, name, uuid, pci, simulated}]` |
| `get_limits` | `gpu` | Limits |
| `get_state` | `gpu` | State |
| `get_telemetry` | `gpu` | `{gpu, sample}` |
| `subscribe` | `topics: ["telemetry","state","txn","safety"]` | `{topics}` |
| `unsubscribe` | `topics` | `{topics}` |

`shutdown` sendes alltid, også uten abonnement.

### Skrive

Alle skrivinger gir en **midlertidig** tilstand (kapittel 6). `confirmTimeoutS` er valgfri (standard 15, klemmes til `confirm.minS`–`confirm.maxS`).

| op | args | NVML i den ekte daemonen |
|---|---|---|
| `apply` | `gpu`, `settings` (delvis GpuSettings), `confirmTimeoutS?` | alle under, i rekkefølgen effekt → klokker → temperaturmål → vifte. Feiler én, rulles de foregående tilbake før svaret |
| `set_power_limit` | `gpu`, `watts` | `nvmlDeviceSetPowerManagementLimit_v2` (grenser fra `nvmlDeviceGetPowerManagementLimitConstraints`) |
| `set_clock_offsets` | `gpu`, `coreMhz?`, `memMhz?` | `nvmlDeviceSetClockOffsets` med `NVML_CLOCK_GRAPHICS` og `NVML_CLOCK_MEM` (minne × 2), per P-tilstand som LACT; grenser fra `nvmlDeviceGetClockOffsets`. Ikke de utfasede `…SetGpcClkVfOffset`/`…SetMemClkVfOffset` |
| `set_temp_target` | `gpu`, `celsius` | `nvmlDeviceSetTemperatureThreshold(…, NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR)`; grenser fra `…ACOUSTIC_MIN/MAX`. Ikke verifisert på GeForce; `NVML_ERROR_NOT_SUPPORTED` gir `not_supported` |
| `set_fan` | `gpu`, `fan` (delvis) | `auto`: `nvmlDeviceSetDefaultFanSpeed_v2` per vifte (og `nvmlDeviceSetFanControlPolicy(…, NVML_FAN_POLICY_TEMPERATURE_CONTINOUS_SW)`). `manual`: `nvmlDeviceSetFanSpeed_v2` per vifte. `curve`: en løkke i daemonen som leser `nvmlDeviceGetTemperatureV` og kaller `nvmlDeviceSetFanSpeed_v2` (NVML har ingen kurve i fastvaren). Antall vifter og område fra `nvmlDeviceGetNumFans` og `nvmlDeviceGetMinMaxFanSpeed` |
| `reset` | `gpu` | effektgrense til standard (`…GetPowerManagementDefaultLimit`), offset 0, temperaturmål til standard, vifte til `auto`. Bekreftes straks (trygg retning) |
| `confirm` | `gpu`, `txn` | – (lagrer oppsettet i daemonens konfig, så det legges på igjen ved oppstart og etter dvale) |
| `revert` | `gpu`, `txn` | samme kall som over, med verdiene i `confirmed` |

Svar på skriving:

```json
{"txn": "t3", "pending": {…}, "clamped": [Clamp, …], "changed": ["powerLimitW"], "active": {…}, "confirmed": {…}}
```

Gir skrivingen ingen endring mot det bekreftede, og ingenting venter, er `txn` og `pending` `null`.

### Planlagt (krever brukerens godkjenning)

| op | Status |
|---|---|
| `rgb_get`, `rgb_set` | Plassholdere. Svarer alltid `not_implemented` med `details.status = "planned"`. Ingen generell «skriv til I2C», bare semantiske kall (sone, farge, effekt), og bare etter trinnplanen i research kapittel 4.5 |

### Bare i mocken

`mock.set_temperature {gpu, celsius | null}` holder den simulerte temperaturen fast (for å prøve nødregelen). Finnes bare med `AG_MOCK_TEST_OPS=1`, og står da i `capabilities.ops`. Ikke en del av kontrakten.

## 6. Bekreft eller rull tilbake

Alt som skrives, gjelder straks, men bare midlertidig:

1. Skrivingen gir en **txn** med frist (`deadline`, `remainingMs`). Det bekreftede oppsettet (`confirmed`) er uendret.
2. `confirm {txn}` innen fristen gjør `active` til nytt `confirmed` og lagrer det.
3. Uten `confirm` innen fristen ruller daemonen tilbake til `confirmed` selv (`reason: "timeout"`).
4. `revert {txn}` ruller tilbake straks (`reason: "revert"`).
5. Kobler klienten som skrev, fra (lukket vindu, krasj, brutt forbindelse), rulles det tilbake straks (`reason: "client_disconnected"`). Demoen gjør det samme når fanen som trykket Bruk, lukkes eller lastes på nytt: serveren sender `revert` (`reason: "tab_closed"` i appen).
6. Stopper daemonen, rulles det tilbake (`reason: "daemon_stopping"`). Ubekreftede verdier lagres aldri, så de kommer ikke tilbake ved neste oppstart.
7. Nye skrivinger fra samme klient mens noe venter, legges i **samme** txn og gir ny frist. Tilbakerulling går da helt tilbake til `confirmed`.
8. Skriving fra en annen klient mens noe venter, gir `busy`. Bare klienten som skrev, kan `confirm`/`revert` (`not_owner`). Feil eller utløpt txn gir `txn_mismatch`, og ingenting som venter gir `no_pending`.

Hendelsen `txn` sendes ved hver overgang: `pending`, `confirmed` og `rolled_back` (med `reason`).

Hvorfor dette er i daemonen og ikke i appen: en innstilling som får maskinen til å henge, kan ikke bekreftes. Da må noe som ikke er appen, rulle tilbake. LACT gjør det samme (`apply_settings_timer`, 5 s som standard; vi bruker 15 s, valgfritt 10–60 s i Innstillinger).

## 7. Feilkoder

| Kode | Når | HTTP i demoen |
|---|---|---|
| `bad_json` | linja er ikke gyldig JSON (forbindelsen beholdes) | – |
| `message_too_large` | over 64 KiB uten linjeskift (forbindelsen lukkes) | – |
| `bad_request` | ikke et objekt, mangler `id`/`op`, `args` er ikke et objekt | – |
| `hello_required` | annen melding før `hello` | – |
| `unsupported_protocol` | ingen felles versjon | – |
| `unknown_op` | ukjent operasjon | – |
| `invalid_argument` | feil type, ukjent felt, umulig kombinasjon (start/stopp) | 400 |
| `unknown_gpu` | ukjent `gpu` | 400 |
| `not_supported` | kortet eller driveren kan ikke dette (`NVML_ERROR_NOT_SUPPORTED`) | 501 |
| `permission_denied` | polkit sa nei (ekte daemon) | 403 |
| `busy` | en annen klient har en endring som venter | 409 |
| `no_pending`, `txn_mismatch`, `not_owner` | se kapittel 6 | 409 |
| `not_implemented` | planlagt (RGB) | 501 |
| `too_many_clients` | over 16 samtidige klienter | – |
| `shutting_down` | daemonen stopper | 503 |
| `internal` | feil i daemonen | 502 |

Klienten i serveren har to egne koder: `daemon_unavailable` (ikke tilkoblet, HTTP 503) og `daemon_timeout` (svarte ikke på 5 s, HTTP 504).

Verdier utenfor grensene er **ikke** en feil. De klemmes og står i `clamped`, så appen kan si fra.

## 8. Sikkerhet som daemonen håndhever

Uansett hva klienten sender:

- **Grenser.** Alle tall klemmes til kortets grenser (eller strengere policy) før de sendes til NVML. Grensene leses fra NVML, aldri fra klienten.
- **Viftekurven** kan ikke gå under 35 % eller falle. Punkter som gjør det, løftes og rapporteres.
- **Nødregel.** Over **85 °C** settes viften til **100 %** uansett modus, også `manual`. Den slippes først under **80 °C**. Hendelsen `safety` sendes ved hver overgang, og `emergency: true` står i tilstand og prøver.
- **Vifte til auto ved avslutning.** Ved SIGTERM, SIGINT og krasj (`uncaughtException` i mocken, panikk-krok i Rust) settes viften til driverens auto, og det ubekreftede rulles tilbake. Klientene får `shutdown`. For det daemonen ikke rekker selv (SIGKILL, kjernefeil), må systemd gjøre det: `ExecStopPost=` med et lite program som bare kaller `nvmlDeviceSetDefaultFanSpeed_v2` for hver vifte, og `WatchdogSec=` med `sd_notify("WATCHDOG=1")` fra viftesløyfa, så en hengende løkke gir omstart.
- **Klient borte.** En bekreftet kurve fortsetter å kjøre når appen lukkes; det er poenget med en tjeneste. Det som ikke er bekreftet, rulles tilbake.
- **Ved oppstart og etter dvale** legges det bekreftede oppsettet på igjen (effektgrense og offset overlever ikke omstart eller at driveren lastes på nytt).

## 9. Tilgang (beskrevet, ikke laget)

Den ekte daemonen:

1. Socket `0660 root:agentic-rigg`. Bare medlemmer av gruppen kan koble til. Lesing (`get_*`, `subscribe`) krever ikke mer.
2. Daemonen leser klientens UID og PID med `SO_PEERCRED` for hver forbindelse.
3. Før hver skrivende operasjon spør daemonen polkit (`CheckAuthorization` med `AllowUserInteraction`) for handlingen som passer:

   | Handling | Gjelder | Standard |
   |---|---|---|
   | `io.github.AgenticRigg.tune` | effekt, offset, temperaturmål, vifte, reset | `auth_admin_keep` (aktiv lokal økt i gruppen kan få `yes`) |
   | `io.github.AgenticRigg.advanced` | senere: VF-kurve via NvAPI | `auth_admin` hver gang |
   | `io.github.AgenticRigg.rgb` | senere: Palit-RGB på `/dev/i2c-0` @ 0x49 | `auth_admin_keep` |

   Svar nei gir `permission_denied`. `confirm`, `revert` og tilbakerulling krever ikke ny autorisasjon (de går mot det trygge).
4. systemd-herding: `ProtectSystem=strict`, `ProtectHome=yes`, `NoNewPrivileges=yes`, `DeviceAllow=/dev/nvidiactl /dev/nvidia0 /dev/nvidia-uvm` (og `/dev/i2c-0` bare når RGB er godkjent), `RuntimeDirectory=agentic-rigg`, `StateDirectory=agentic-rigg`.

Mocken har ingen av delene. Den beskytter seg med privat mappe (0700) og socket (0600), og kjører som vanlig bruker.

## 10. Slik brukes kontrakten i demoen

```
nettleser ──HTTP/SSE──► server/ (Node) ──NDJSON over Unix-socket──► daemon-mock/ (Node, simulert kort)
                         │  daemon-client.js: hello, subscribe, gjenoppkobling (0,5 → 5 s)
                         └─ nvidia.js: live-tall fra nvidia-smi, bare lesing (som før)
```

- `npm start` kjører `scripts/dev.js`, som starter mocken og så serveren med samme socket og datamappe. Serveren starter aldri andre programmer enn nvidia-smi.
- HTTP: `POST /api/apply` (→ `apply`), `POST /api/confirm` (→ `confirm`), `POST /api/revert` (→ `revert`), `POST /api/reset` (→ `reset`), `GET /api/daemon` og `POST /api/daemon/retry`. SSE-hendelsen `daemon` bærer tilkobling, ventende endring, grenser, nødregel og siste utfall.
- Flere faner (v0.0.8): serveren er én klient for tjenesten, så alle faner deler den samme ventende endringen og kan bekrefte eller rulle den tilbake. Den ventende endringen har `tab`, fanen som trykket Bruk. Bare den fanen kan skrive mer i samme txn; `POST /api/apply` fra en annen fane, eller mens et annet Bruk er på vei til tjenesten, gir 409 med `code: "pending_elsewhere"`. Kontrakten mot tjenesten er uendret.
- Appens flate felt (`powerLimit`, `coreOffset`, `fanMode` …) oversettes til kontraktens (`powerLimitW`, `clockOffsets.coreMhz`, `fan.mode` …) i `server/daemon-protocol.js`.
- Uten tjeneste: Bruk er låst («Venter på bakgrunnstjenesten»), Kontroller og Viftekurve viser «Bakgrunnstjeneste: ikke tilkoblet», og den simulerte modellen går videre lokalt med siste kjente verdier.
