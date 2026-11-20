# Agentic Rigg 1.0 · protokollen mellom vinduet og tjenesten

Oppgaver-utvidelsen på `feature/task-manager` beskrives i kapittel 13. Den endrer ikke GPU-hjelperens protokoll eller skriveoperasjoner.

*Grunnmuragenten (runde 1), 7. november 2026, med diskene fra runde 6A (8. november 2026, 1.1.0 · bygg 17) og root-hjelperen fra runde 7A (9. november 2026, 1.2.0, kap. 12). Gjelder `agentic-riggd` og `agentic-rigg` 1.0.0 · bygg 16 og nyere. Typene står ett sted, i `app/crates/rigg-core/src/proto.rs`, og brukes av begge sider. Kontrakttestene er `app/crates/riggd/tests/kontrakt.rs`.*

Protokollen er fortsatt **1** fra `docs/daemon-api.md`. Alt under er nye operasjoner, felt og hendelser, som kontrakten sier ikke er brudd. Det som ikke står her, gjelder som i `docs/daemon-api.md`.

## 1. Transport

- Unix-socket med NDJSON (ett JSON-objekt per linje, `\n`), UTF-8. Høyst **65 536 byte** per linje; over det gir `message_too_large`, og forbindelsen lukkes.
- Socket: `$XDG_RUNTIME_DIR/agentic-rigg/riggd.sock` (produksjon) eller `riggd-dev.sock` (`RIGG_PROFIL=dev`). `RIGG_SOCKET` eller `--socket` overstyrer. Mappa er `0700` og eies av brukeren (ikke symlenke), socketen `0600`.
- `SO_PEERCRED`: bare samme UID får koble til (andre får `permission_denied` og lukket forbindelse). Høyst **16** klienter; nummer 17 får `too_many_clients`.
- En klient som ikke leser (over 4 MiB i kø), kobles fra.

## 2. Meldinger

Forespørsel `{"id", "op", "args"}`, svar `{"id", "ok", "result" | "error"}`, hendelse `{"event", "data"}` uten `id`. `id` er et heltall eller en tekst på høyst 64 tegn. `args` kan utelates, men må ellers være et objekt. Feilmeldingene er norske (`error.message`), og `error.details` er maskinlesbart.

## 3. `hello`

Første melding må være `hello {"protocol": [1], "client": "agentic-rigg/1.0.0"}`. Svar:

```json
{
  "protocol": 1,
  "session": "s3",
  "daemon": {"name": "agentic-riggd", "version": "1.0.0", "build": 16, "mock": false, "pid": 4242, "backend": "nvml", "startedAt": 1794044321109},
  "capabilities": {
    "gpus": ["0000:1c:00.0"],
    "ops": ["hello", "ping", "list_gpus", "get_static", "get_system", "get_limits", "get_state", "get_telemetry",
            "subscribe", "unsubscribe", "set_focus", "get_history", "get_stats", "reset_stats",
            "log_start", "log_stop", "log_status", "get_settings", "set_settings", "clear_history", "shutdown_when_idle"],
    "events": ["telemetry", "device", "stats", "log", "alarm", "settings", "shutdown"],
    "write": {"state": "locked", "steps": [{"id": "power", "status": "not_approved"}, {"id": "clocks", "status": "not_approved"},
              {"id": "fan_manual", "status": "not_approved"}, {"id": "fan_curve", "status": "not_approved"},
              {"id": "temp_target", "status": "not_approved"}, {"id": "rgb", "status": "not_approved"}]}
  },
  "intervalMs": 2000,
  "maxMessageBytes": 65536,
  "notes": []
}
```

- `backend` er `nvml` (det ekte kortet, bare lesing), `sim` (tester og skjermbilder) eller `ingen`.
- `notes`: meldinger fra oppstarten (en fil som ikke kunne leses).
- Vinduet sammenligner `daemon.build` med sitt eget bygg og viser linja «Tjenesten er fra bygg 16, vinduet fra bygg 17» ved ulikhet.

## 4. Operasjonene

| op | args | result |
|---|---|---|
| `ping` | – | `{t, startedAt, pid, intervalMs, background, rssKB, pssKB, privateKB, roundUs, clients: [{session, client, pid}], historySeries}`. Brukes av Innstillinger › Generelt og Om |
| `list_gpus` | – | `{gpus: [GpuInfo], backend}` |
| `get_static` | `gpu?` (første kort når utelatt) | `{static: GpuStatic, complete}`. De faste dataene leses gradvis de første sekundene (ett steg per runde); `complete` er sann når alt er lest. Fra forrige kjøring (samme UUID og driver) finnes de straks |
| `get_system` | – | `{cpu, board, os, pci, threads, memTotalKB, disks?}`. `cpu`/`board`/`os`/`pci` fylles av 2B. Fra 1.1.0: `cpu.coreOf` (kjernen per tråd), `pci.controllers` (`[{addr, kind, vendor, name, ids, driver}]`: nett, lyd, USB, SATA, NVMe) og `disks` (siste `get_disks`-liste, til rapporten) |
| `get_limits` | `gpu?` | `{gpu, powerLimitW: {min, max, default, source}, tempTargetC: {value, max, slowdown, shutdown, source}, fan: {count, min, max, source}, write}` |
| `get_state` | `gpu?` | `{gpu, active: {powerLimitW, tempTargetC, fan: {percent}}, pending: null, emergency: false, write}`. `emergency` er sann når nødregelen for viftene gjelder (bare `sim` i 1.2.0, kap. 11). Med `sim`: boka (`rigg-core/src/txn.rs`): `{gpu, active, confirmed, pending: {txn, owner, timeoutS, deadline, remainingMs, changed, clamped} | null, emergency, write}` |
| `get_telemetry` | – | Siste `telemetry` (eller `null` før første prøve) |
| `subscribe` | `topics?` (standard `["telemetry"]`), `intervalMs?` (500–2000, klemmes) | `{topics, intervalMs, effectiveMs}` |
| `unsubscribe` | `topics` | samme |
| `set_focus` | `pages: ["oversikt"]` | `{pages, effectiveMs}`. `[]` betyr at vinduet er minimert: takten går ned til 2 s |
| `get_history` | `window` (60, 300, 900, 3600, 21600, 86400, 172800), `keys` (høyst 16), `points?` (10–4000, standard 1000) | `{window, step, from, to, oldest, t: [ms], series: {key: {min, avg, max}}, cadence: [[fra_ms, takt_ms]]}` |
| `get_stats` | `keys?` | `{since, avgSince, stats: {key: {min, max, avg, n}}}` |
| `reset_stats` | `keys?` (alle når utelatt) | som `get_stats`. Sender også `stats` |
| `get_disks` | – | **Runde 6A.** `DiskList` (`rigg-core/src/disk.rs`): `{t, source: "udisks"\|"sysfs"\|"sim", note, disks: [Disk], pending}`. Lista lages av prøvetråden hvert 60. s mens et vindu som synes abonnerer (eller loggen går), hvert 10. min i bakgrunnen når diskvarselet er på, og ellers bare når noen spør. Er den eldre enn 65 s, bes det om en ny (høyst hvert 5. s) og den forrige gis; før første liste er svaret `{disks: [], pending: true}`. Se kap. 10 |
| `get_processes` | `gpu?` | `{gpu, processes: {t, list: [{pid, name, vramMiB, kind}], error}, live}`. Prøvetråden leser lista hvert 5. s i 15 s etter siste forespørsel (Grafikkort spør mens siden vises). Før første lesing: øyeblikksbildet fra de faste dataene og `live: false` (runde 3A) |
| `log_start` / `log_stop` / `log_status` | `keys?`, `intervalMs?`, `dir?`, `format?` | `{active, file, rows, bytes, startedAt}`. **2B** lager loggen (`riggd/src/logg.rs`); i runde 1 svarer `log_start` `not_implemented` |
| `get_settings` | – | Hele `tjeneste.json` pluss `paths: {config, state, socket}` |
| `set_settings` | delvis `tjeneste.json` | som `get_settings`. Validert (ugyldig gir `invalid_argument` med norsk melding). Sender `settings` til alle |
| `clear_history` | – | `{cleared: true}`. Vinduet venter 5 s (Angre) før det kaller den |
| `shutdown_when_idle` | – | `{idleShutdown, graceMs}`. Stopper etter fristen når siste klient er borte, også med bakgrunn på |
| `get_helper` | `refresh?` (sann: spør hjelperen på nytt) | **1.2.0, runde 7A.** `{state: "off"\|"on"\|"error"\|"outdated", message, steps: ["effekt" …], backend, version, build, realWrites, emergency, pending, paths: {config, socket}}`. Det samme står i `hello` som `capabilities.write.helper`. Se kap. 12 |

**Skriveoperasjonene** (`apply`, `set_power_limit`, `set_clock_offsets`, `set_temp_target`, `set_fan`, `confirm`, `revert`, `reset`, `rgb_get`, `rgb_set`) står bare i `ops` med `--backend sim`. Med `nvml` (og `ingen`) svarer de **`write_locked`**: «Endringer er låst. Skriving til skjermkortet er ikke slått på.» Med `sim` går de til bekreftelsesflyten i `rigg-core/src/txn.rs` (runde 3A): `apply`/`set_*` gir en txn som må få `confirm` innen fristen (10–60 s, standard `confirmSec` fra `tjeneste.json`), bare eieren kan bekrefte eller rulle tilbake, en annen klient får `busy`, fristen og frakobling ruller tilbake, `reset` bekreftes straks. Prøven fra sim viser det som er i bruk (effektgrense, vifter), og `extra.styring` i de faste dataene følger offset og temperaturmål. Ingenting av dette når et skjermkort. Kontrakttestene `skriving_er_låst_mot_det_ekte_kortet` og `bruk_behold_og_rull_tilbake_mot_sim` viser begge sider.

**Fra 1.2.0 (runde 7A)** går skriveoperasjonene først til root-hjelperen når den er aktivert (kap. 12): da står de også i `ops` med `nvml`, og svaret kommer fra hjelperen. Uten hjelper er `write_locked` uendret, men har nå `details.reason` (`helper_off`, `helper_error`, `helper_outdated`, `backend_mismatch`, `real_writes_off`), og et trinn som ikke er godkjent, gir den nye koden `step_not_enabled` med `details.steps`.

## 5. Hendelsene

| event | Når | data |
|---|---|---|
| `telemetry` | hver runde (abonnement) | `{t, gpus: {"<id>": GpuSample}, sys: SysSample, intervalMs}` |
| `device` | et kort kommer, forsvinner eller bytter tilstand; dvale | `{gpus: [GpuInfo], changed?: [GpuInfo]}` |
| `stats` | hvert 5. s og etter `reset_stats` | som `get_stats` |
| `log` | ved overganger i loggen (2B) | som `log_status` |
| `alarm` | ved varsel (2B) | fritt. Fra 1.1.0 også `{kind: "disk_fail" \| "disk_sectors", disk, t, title, body}` (`alarms.diskHealth`) |
| `settings` | etter `set_settings` | som `get_settings` |
| `txn` | bare med `sim`: en endring er beholdt eller rullet tilbake | `{txn, status: "confirmed" | "rolled_back", reason: "confirm" | "revert" | "timeout" | "client_disconnected" | "reset"}` |
| `shutdown` | alltid (også uten abonnement) når tjenesten stopper | `{reason: "signal" | "idle"}` |

## 6. Datatypene

Feltnavnene er demoens (camelCase), så sidene kan porteres rett. Se `rigg-core/src/model.rs`.

- **GpuSample**: `temp`, `gpuUtil`, `memUtil`, `encUtil`, `decUtil`, `coreClock`, `smClock`, `memClock`, `videoClock`, `power` (W, snitt fra energitelleren), `powerLimit` (håndhevet, W), `fan` (høyeste), `fans: [%]`, `fanRpm: [o/min]`, `vramUsed`/`vramTotal`/`vramReserved` (MiB, `MemoryInfo_v2`), `pstate` («P8»), `pcieGen`/`pcieWidth` (sysfs), `throttle` (maske), `throttleInferred` (sann i P8, der kallet ikke gjøres), `energyMj`, `fanPolicy: [0|1]` (viftestyring per vifte, 0 = driveren styrer, hvert 5. s), `fanTarget: [%]` (målfart per vifte, hvert 5. s), `na: [felt]` (NOT_SUPPORTED). `fanPolicy` og `fanTarget` utelates når de ikke kan leses.
- **SysSample**: `t`, `cpu: {load, coreLoad, coreMhz, temp (Tdie), tctl, governor, boost}`, `mem: {totalKB, availableKB, usedKB, freeKB, buffersKB, cachedKB, shmemKB, sreclaimableKB, swapTotalKB, swapUsedKB}`, `net: {navn: {rx, tx, state, wireless}}` (byte/s, uten `lo`), `hwmon: [{id, name, sensors: [{id, kind, key, label, value, err}]}]`, `uptime`, `loadavg`, og fra 1.1.0 `disks: [{id, name, model, read, write, busy, temp}]` (utelatt når tom; byte/s fra `/proc/diskstats`, `busy` i % fra `io_ticks`, `temp` i °C fra SMART når den er under 30 min gammel).
- **GpuInfo**: `{id, index, name, state, since, driver, driverInstalled, detail}`. `state` er `ok`, `driver_updated`, `no_driver`, `lost` eller `suspended`. Uten driver er lista ett element med `id: ""` og `state: "no_driver"`.
- **GpuStatic**: navn, UUID, driver, NVML- og CUDA-versjon, VBIOS, PCI-ID-er, VRAM, effektgrenser, maks klokker, vifter, temperaturterskler, PCIe maks, og `extra` (fritt JSON fra 2B, `rigg-hw/src/gpu/nvml_info.rs`). `extra.styring` (runde 3A) har det Kontroller og Viftekurve trenger: `coreOffsetMHz` og `memOffsetMHz` som `{value, min, max}` (minnet i effektiv MHz, NVML oppgir dobbel verdi), `tempTarget` `{value, min, max}` fra THRESHOLD ACOUSTIC_CURR/MIN/MAX, `fanPolicy` og `fanTarget` per vifte.

## 7. Sensornøkler

Samme nøkkel i historikk, statistikk, logg, festede sensorer og alarmer (`rigg-core/src/sensor_key.rs`, mønsteret `^[A-Za-z0-9_.:/-]{1,80}$`):

| Nøkkel | Hva |
|---|---|
| `gpu:0000:1c:00.0/temp` … `/fan`, `/fan1`, `/fanRpm0`, `/power`, `/powerLimit`, `/vramUsed`, `/pcieGen` | skjermkortet (alle feltene i GpuSample) |
| `cpu/load`, `cpu/temp` (Tdie), `cpu/tctl`, `cpu/thread/3/load`, `cpu/thread/3/mhz` | prosessoren |
| `mem/usedPct`, `mem/used`, `mem/avail`, `mem/swap` (GiB) | minnet |
| `net/rx`, `net/tx` (fysiske kort), `net/wlo1/rx` | nettet |
| `hwmon/k10temp/temp2` (to enheter med samme navn: `k10temp_2`) | hwmon |
| `disk/WFL454P6/read`, `/write` (byte/s), `/busy` (%), `/temp` (°C); `disk/read`, `disk/write` (sum) | diskene (1.1.0). ID-en er serienummeret (udev `ID_SERIAL_SHORT`), ellers kjernenavnet. Høyst 8 disker |

Festede sensorer i `statusItems` godtar både disse og demoens rad-ID-er (`gpu/temp`, `cpu/t3`, `hw/hwmon3/temp1`), så en eksport fra demoen kan importeres.

## 8. Feilkodene

Som `docs/daemon-api.md` kap. 7, pluss **`write_locked`** (skriving er ikke slått på; fra 1.2.0 med `details.reason`), **`step_not_enabled`** (1.2.0: hjelperen er på, men trinnet er ikke godkjent; `details.steps`) og **`unavailable`** (bare i klienten: tjenesten svarer ikke, eller svaret kom ikke innen 5 s).

## 9. Takt og livsløp

- Takten er den raskeste klienten som abonnerer på `telemetry` (500–2000 ms, standard 1000). Minimert vindu (`set_focus []`) gir 2 s. Uten klienter: «Takt i bakgrunnen» (1, 2 eller 5 s, standard 2 s). Mens loggen går, er takten minst så rask som loggintervallet (aldri under 500 ms), også uten vindu.
- Aldri raskere enn 2 Hz, og tapte runder tas ikke igjen.
- Uten klienter og med «Fortsett i bakgrunnen» av stopper tjenesten etter 60 s (`--grace-ms` i testene). Logg som går, holder den i live.
- SIGTERM, SIGINT og SIGHUP: `shutdown`-hendelsen, historikk og min/maks lagres, NVML lukkes, socketen slettes.
- `READY=1` til `$NOTIFY_SOCKET` når socketen er bundet og første prøve er tatt (`Type=notify`).
- Dvale (logind `PrepareForSleep`): prøvene stopper og historikken lagres; 2 s etter oppvåkning starter de igjen. Kortene står som `suspended` imens.

## 10. Diskene (1.1.0, runde 6A)

Kildene, uten root:

- **UDisks2** på systembussen (`org.freedesktop.UDisks2`): én `GetManagedObjects` gir `Drive`, `Drive.Ata` og `NVMe.Controller` (modell, serie, fastvare, størrelse, o/min, tilkobling, SMART-status, temperatur, driftstid, dårlige sektorer, selvtest, `SmartUpdated`). SMART-tabellen hentes med `SmartGetAttributes` bare når `SmartUpdated` har endret seg, og bare med vindu. UDisks2 leser selv SMART omtrent hvert 10. min og vekker ikke disker i hvile. **Vi kaller aldri `SmartUpdate`, `SmartSelftestStart` eller noe annet som sender kommandoer til disken**, og åpner aldri `/dev/sd*`. Kallene har 2,5 s frist og `NO_AUTO_START`.
- **`/proc/diskstats`** (én `pread` per runde): lest og skrevet (sektorer à 512 byte) og `io_ticks`.
- **sysfs og udev-databasen** (`/sys/block`, `/run/udev/data/b<maj>:<min>`): kjernenavn, serie, modell, fastvare, o/min, SATA-lenke (`/sys/class/ata_link/linkN/sata_spd`), PCIe-lenke for NVMe, I/O-planlegger, sektorstørrelser, partisjoner, filsystem og etikett.
- **`/proc/self/mountinfo` og `statvfs`**: monteringspunkt og brukt/ledig per partisjon, og hvilken disk `/` ligger på (også gjennom dm/md).

`Disk`: `{id, name, dev, model, vendor, serial, firmware, wwn, size, rpm, kind: "HDD"|"SSD"|"NVMe", bus: "SATA"|"USB"|"NVMe"|…, removable, system, link, scheduler, logicalBlock, physicalBlock, smart, partitions, health}`.

`smart`: `{source: "udisks"|"sim"|"", supported, enabled, failing, temp, powerOnHours, powerCycles, badSectors, attrsFailing, attrsFailedPast, selftest, selftestRemaining, updated (ms), lifeLeft (%), written, read (byte), attributes: [{id, name, flags, value, worst, threshold, pretty, unit}], nvme: {criticalWarning, availSpare, spareThresh, percentUsed, unsafeShutdowns, mediaErrors, errorLogEntries, warningTempMinutes, criticalTempMinutes, warningTemp, criticalTemp} | null, note}`. `unit` er libatasmarts: 0 ukjent, 1 antall, 2 ms, 3 sektorer, 4 mK, 5–6 %, 7 MB. Attributt 241/242 tolkes på nytt: libatasmart regner alltid 32 MiB per enhet, men Samsung, Seagate og de fleste andre teller LBA-er à 512 byte (`rigg_core::disk::lba_unit_bytes`).

`health`: `{level: "ok"|"warn"|"fail"|"unknown", reasons: [tekst]}` (`rigg_core::disk::health`, som CrystalDiskInfo). **Feil:** disken melder selv at den feiler, en pre-fail-verdi er under terskelen nå, under 3 % levetid igjen, over temperaturgrensen for feil, eller NVMe-advarsel om reserve, redusert pålitelighet eller skrivebeskyttelse. **Advarsel:** dårlige sektorer (05, C5, C6), verdier som har vært under terskelen, feilet selvtest, 10 % levetid eller mindre, varm disk (HDD 55/65 °C, SSD 60/70, NVMe 70/80), andre NVMe-advarsler og mediefeil. **Ukjent:** ingen SMART (USB-bro, UDisks2 svarer ikke).

`partitions`: `[{name, dev, number, size, fs, label, mount, total, used, avail}]` (byte; `total`/`used`/`avail` bare når montert).

Uten UDisks2 (den private økta har en tom systembuss) er `source` `sysfs` og `note` forklaringen; alt utenom SMART er med. `--backend sim` (eller `RIGG_DISKER=sim`) gir fem simulerte disker (NVMe, SATA-SSD, to HDD-er der den ene har dårlige sektorer, USB uten SMART); `RIGG_DISKER=ekte` gir de ekte diskene også med sim, og `RIGG_DISKER=ingen` slår dem av (målinger).

Historikken har plass til 112 serier (96 til 1.1.0): fire per disk og summen, høyst 8 disker. Fila er da høyst ca. 8 MB.

## 11. Viftene (1.2.0, runde 7B)

**`GpuSettings.fan`** (`apply` med `settings.fan`, og `set_fan` med de samme feltene på toppnivå). Alle feltene er valgfrie; det som mangler, endres ikke. Tall utenfor områdene klemmes og står i `clamped` (`source: "hardware"`). Ukjente felt gir `invalid_argument`.

| Felt | Type | Standard | Område og regler |
|---|---|---|---|
| `mode` | `"auto"` \| `"curve"` \| `"manual"` | `auto` | Auto: driveren styrer (policy 0). |
| `manualPercent` | tall | 50 | 35–100 % (kortets minste fart fra `GetMinMaxFanSpeed`). |
| `curve` | `[[°C, %], …]` | Balansert (`DEFAULT_CURVE`) | 2–10 punkter, 0–110 °C, stigende temperatur. Fart under 35 % og fart lavere enn punktet før løftes (`clamped: fan.curve`). Den felles kurven. |
| `perFan` | bool | `false` | `true`: hver vifte følger sin kurve i `curves`. |
| `curves` | `[kurve, …]` | `[]` | 0–8 kurver (vifte 1 først), hver som `curve`. En vifte uten kurve følger `curve`. Feilmeldingen sier hvilken vifte. |
| `hysteresisC` | tall | 3 | 0–10 °C. Viftene går ikke ned før temperaturen har falt så mye under punktet der de gikk opp. Opp går de straks. |
| `rampUpPctS` | tall | 10 | 1–100 % per sekund opp. |
| `rampDownPctS` | tall | 3 | 1–100 % per sekund ned. |
| `smoothingS` | tall | 0 | 0–10 s. Snittet av temperaturen over så mange sekunder (0 = av). Vinduet tilbyr av, 3, 5 og 10 s. |
| `quietUntilC` | tall \| `null` | `null` | 30–80 °C: hold minste fart under denne temperaturen, deretter kurven. `null` slår av. |

`changed` bruker de samme navnene (`fan.perFan`, `fan.curves`, `fan.hysteresisC`, `fan.rampUpPctS`, `fan.rampDownPctS`, `fan.smoothingS`, `fan.quietUntilC`). `active` og `confirmed` i `get_state` har alle feltene.

**Viftestopp (0 %) finnes ikke.** RTX 2080 har minst 35 % i VBIOS (`nvmlDeviceGetMinMaxFanSpeed` = 35–100 %), og driveren nekter alt under. Ingen felt kan be om det.

**Nødregelen** kan ikke slås av og har ikke noe felt: fra 85 °C går alle viftene straks til 100 %, tilbake til kurven under 80 °C. Den ser på den rå temperaturen, uten utjevning, hysterese eller rampe. Kan temperaturen ikke leses i mer enn 5 s, går viftene også til 100 %.

**Viftemotoren** (`rigg_core::vifte`, ren logikk) gjør det samme i tjenesten (sim) og i root-hjelperen (senere): nødregel → utjevning → hysterese → kurve per vifte med stille flate → minste fart → rampe. `Motor::steg(temp, nå)` gir farten per vifte.

**Sim** (`--backend sim`, `rigg-hw/src/gpu/sim_vifter.rs`, før 7C `riggd/src/vifter.rs`): temperaturen er en funksjon av effekten og viftefarten (`rigg_hw::gpu::sim::Termo`, tidskonstant 20 s), så kurven, hysteresen og rampen synes i grafene. I Auto følger viftene driverens kurve i sim (policy 0), i Kurve motoren (policy 1). Scenarioet `sim:spill` gir spill-last (90 s med 80–95 %, 60 s i ro). Med NVML skriver tjenesten ingenting; skriveoperasjonene svarer `write_locked` som før.


## 12. Root-hjelperen (1.2.0, runde 7A)

`agentic-rigg-helper` er en systemtjeneste med root som `riggd` sender skriveoperasjonene til. Vinduet snakker aldri med den. Sikkerheten og låsene står i `docs/produksjon/hjelper-sikkerhet.md`; det felles (trinnene, `helper.json`, stiene) i `rigg-core/src/hjelper.rs`.

**Finnes den?** `riggd` ser etter `/etc/agentic-rigg/helper.json` og `/run/agentic-rigg/helper.sock` (under en falsk rot med `--hjelper-rot` i testene og `RIGG_HJELPER_ROT` i utviklerprofilen). Ved oppstart, ved `get_helper {refresh: true}`, hvert 60. s når den er av (én `stat`) og hvert 30. s når den er på. Mens en endring venter eller nødregelen gjelder: `status` hver runde, og `txn`-hendelsene fra hjelperen sendes videre som før.

**Tilstandene** (`capabilities.write.helper.state`): `off` (ikke aktivert), `on` (svarer, samme bygg og protokoll), `error` (aktivert, men svarer ikke, eller avviser uid-en), `outdated` (annet bygg; bryteren må slås av og på).

**Ruting i `riggd`** (`ops.rs` → `hjelper.rs::via_hjelper`):

| Hjelperen | Svar på en skriveoperasjon |
|---|---|
| `off` | Som før: sim-boka med `--backend sim`, ellers `write_locked` / `helper_off` |
| `error` / `outdated` | `write_locked` / `helper_error` / `helper_outdated` |
| `on`, annen backend enn tjenesten | `write_locked` / `backend_mismatch` (sim og ekte kort blandes aldri) |
| `on`, trinnet ikke godkjent | `step_not_enabled` med `details.steps` (sjekkes i `riggd` og i hjelperen) |
| `on`, en annen økt eier endringen | `busy` (skriving, `reset`) eller `not_owner` (`confirm`, `revert`) |
| `on`, alt i orden | Videre til hjelperen; svaret som fra sim-boka (`txn`, `pending`, `clamped`, `changed`, `active`, `confirmed`) |

`set_power_limit`, `set_clock_offsets`, `set_temp_target` og `set_fan` blir til `apply` med en delvis `GpuSettings` (`hjelper::patch_for_op`). `confirmTimeoutS` er `confirmSec` fra `tjeneste.json` når den mangler. Lukker økta som eier endringen, sender `riggd` `revert`. `get_state` gir hjelperens bok når den er på. `capabilities.write.steps` får `approved` for trinnene hjelperen har godkjent (`effekt` → `power`, `temp_target`; `vifter` → `fan_manual`, `fan_curve`; `klokker` → `clocks`; `lys` → `rgb`). `capabilities.write.state` er uendret (`locked` med `nvml`, `sim` med sim), så `write_gate` i vinduet holder Bruk låst mot det ekte kortet i 1.2.0.

**Protokollen til hjelperen** (`/run/agentic-rigg/helper.sock`, `0600`, eies av brukeren i `helper.json`; NDJSON som over, høyst 16 KiB per linje og fire forbindelser; `SO_PEERCRED` mot uid-en i `helper.json`, ellers `permission_denied` uten `id` og lukket forbindelse):

| op | args | result |
|---|---|---|
| `status` | – | `{helper: {name, version, build, protocol: 1, pid, backend, realWrites, locked}, uid, steps, configError, state: {active, confirmed, pending, emergency}, limits: {powerLimitW, coreOffsetMhz, memOffsetMhz, tempTargetC, fanPercent} (hver [min, max]), hardware, emergency, events: [txn-hendelser siden sist]}` |
| `apply` | `settings` (delvis `GpuSettings`), `confirmTimeoutS?` | Som `apply` i sim-boka. Feil: `step_not_enabled`, `invalid_argument` med `details.reason = "out_of_range"` (tall utenfor kortets område avvises, klemmes ikke), `busy` (en annen forbindelse, eller nødregelen), `write_locked`/`real_writes_off` (det ekte kortet er sperret) |
| `confirm`, `revert` | `txn` | Som i sim-boka |
| `reset` | – | Alt til kortets standard, viftene til Auto. Bekreftes straks |
| `rgb_get`, `rgb_set` | – | `step_not_enabled` uten trinnet `lys`, ellers `not_implemented` |
| `sim_temp` | `celsius` | Bare med `--backend sim` (testene): holder temperaturen fast |

Hjelperen setter kortet til standard (viftene på Auto) når forbindelsen som skrev, lukkes (`riggd` stopper eller krasjer), når et trinn trekkes tilbake, og når den selv stopper. Over 85 °C rulles ventende endring tilbake og viftene går til 100 % til kortet er under 80 °C. Viftekurven kjøres i hjelperen med viftemotoren fra 7B (`rigg_core::vifte`), på samme måte som tjenestens sim-sløyfe: `med_start` fra farten viftene har når Kurve slås på, nytt oppsett bare når det er endret, og ett steg per runde, også under nødregelen (7C). `status.emergency` er også sann når motoren har gått til 100 % fordi temperaturen ikke kan leses. Hver endring logges til journalen (`journalctl -u agentic-rigg-helper`).

## 13. Oppgaver · feature/task-manager (ikke utgitt)

`get_tasks` annonseres blant leseoperasjonene. `terminate_task` annonseres separat fra GPU-skriveoperasjonene, også uten GPU/backend. Ingen av dem bruker root-hjelperen. Eldre tjenester uten disse operasjonene må oppdateres før Oppgaver kan brukes.

`get_tasks` tar `filter` (maksimalt 128 tegn), `sort` (`name`, `pid`, `user`, `cpu`, `memory`, `gpuMemory`, `state`), `descending`, `tree`, `offset` og `limit` (1–100). `snapshotId` kan utelates eller være null for nyeste prøve; et tidligere ID kan brukes for sammenhengende sidevisning. Tjenesten beholder nåværende og to tidligere prøver mens innsamlingen pågår. I tillegg beholder hver tilkobling de to siste bildene den har fått, også når innsamlingen er stoppet. Lesing fra et lagret bilde forlenger ikke innsamlingen. Hvis bildet mangler, gjelder `snapshotChanged: true` og tjenesten går til første side. `newerThan` venter på en prøve med høyere ID og kan brukes i stedet for `snapshotId`, for eksempel ved F5.

Svaret inneholder `rows`, `total`, `matched`, `offset`, `nextOffset`, `sampledAtMs`, `snapshotId`, `snapshotChanged`, `ageMs`, `loading`, `stale`, `intervalMs`, `unreadable` og `error`. GPU-dekning forklares med `gpuSource`, `gpuSampledAtMs` og `gpuNote`. Prosessidene begrenses til 48 KiB før små statusmetadata/protokollkonvolutten legges til, innen dagens grense på 64 KiB.

Hver rad har `pid`, `startTicks`, `ppid`, `name`, `state`, `uid`, `user`, `effectiveUid`, `rssBytes`, `cpuPercent`, `gpuVramBytes`, `depth`, `canTerminate` og `protectedReason`. Manglende verdier er null, ikke nullmålinger. Prosess-CPU har 100 % per logisk CPU; RAM er ikke-additiv RSS. GPU-minne er et samlet, avrundet NVML-bidrag fra kortene med dekning; ufullstendige/ukjente bidrag vises ikke som en fullstendig sum. Per-prosess GPU-prosent er ikke levert av denne modulen.

Forespørslene holder behovet aktivt i fem sekunder. Prøvetråden samler høyst hvert andre sekund, og nullstiller CPU-baseline etter opphold. Første forespørsel kan gi `loading: true`; klienten må hente igjen for å motta prøven. Et prøvetidspunkt gjør ikke `/proc`-gjennomlesingen atomisk.

`terminate_task` tar `pid`, `expectedStartTicks` og `confirmed: true`. Alle verdier kontrolleres på nytt før et pidfd-bundet SIGTERM til én prosess. Ugyldige argumenter avvises; utilstrekkelige rettigheter, beskyttede mål og utgått identitet gir norsk forklaring. `sent: true` og `signal: "SIGTERM"` bekrefter signalutsending, ikke at prosessen allerede er avsluttet. Ingen SIGKILL, gruppeavslutning, administratoropptrapping eller automatisk avslutning finnes.
