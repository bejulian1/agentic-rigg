# Agentic GPU – hva en ekte GPU-app kan gjøre på Linux/NVIDIA

*Research 2026-11-04 for «Agentic GPU». Gjelder Palit GeForce RTX 2080 Rev. A (TU104), NVIDIA-driver 610.43.02 (open kernel module), Ubuntu/GNOME på Wayland.*

**Merking i dokumentet**

- **[Kilde]**: står i dokumentasjon eller kildekode. Lenken er oppgitt.
- **[Målt]**: lest på denne maskinen 2026-11-04, bare med lesekall (se «Metode» nederst).
- **[Antakelse]**: mitt eget resonnement. Må verifiseres før det brukes.

---

## 0. Kortversjon

1. **Sikkert mulig på Wayland via NVML:** overvåking, effektgrense, klokke-offset for kjerne og minne, låst kjerneklokke, viftestyring med egen kurve og profiler. NVML er uavhengig av X11 og virker derfor på Wayland. Alt som *skriver*, krever root. Derfor trengs en liten privilegert tjeneste, slik LACT gjør det.
2. **Mulig med forbehold:** full redigering av spennings- og frekvenskurven (VF-kurven) og «voltage boost». NVML har ikke dette, men LACT gjør det på Linux gjennom det **udokumenterte** NvAPI-biblioteket `libnvidia-api.so.1`, og det finnes på maskinen. LACT har egen kode for Turing.
3. **Ikke mulig på Turing/Linux:** låst minneklokke (krever Ampere eller nyere), en ekte spenningsoffset i mV slik Windows-verktøy viser det, effektgrense over 275 W, vifte under 35 %, og et eget overlegg som tegnes over spill på GNOME Wayland (Mutter mangler layer-shell). Overlegg i spill løses ved å lage MangoHud-konfig.
4. **Palit-RGB:** Det er sannsynlig at kortet har en Palit-lyskontroller på I2C-adresse **0x49**, og den bruker samme registerprotokoll som OpenRGB kaller «PalitGPUv2». Det sterkeste beviset er et ThunderMaster-opptak fra et Palit RTX 2080 Ti GamingPro OC. Det kortet melder også NVIDIAs subsystem-ID, akkurat som ditt. **OpenRGB vil ikke oppdage kortet automatisk**, fordi ingen detektor matcher `10de:1e87` / `10de:1e87`. Ingenting er bekreftet på *ditt* kort ennå. Kapittel 4 har en trinnvis plan som du må godkjenne før noe leses fra eller skrives til I2C.
5. **Anbefalt stack:** en privilegert daemon i Rust (`nvml-wrapper`, polkit, systemd) og et GUI i Electron/TypeScript som gjenbruker alpha-demoen. GUI-et snakker med daemonen over en Unix-socket med JSON. Overlegg i spill lages som MangoHud-konfig. Se kapittel 6.

---

## 1. Maskinen – målte fakta som påvirker designet

| Hva | Verdi | Kilde |
|---|---|---|
| GPU | TU104 «RTX 2080 Rev. A», PCI `10de:1e87`, subsystem `10de:1e87`, GPU-delenummer `1E87-400-A1` | [Målt] `lspci -nnv`, `nvidia-smi -q` |
| Driver | 610.43.02, **NVIDIA Open Kernel Module**, GSP-fastvare 610.43.02, NVML API «13» | [Målt] `/proc/driver/nvidia/version`, `nvidia-smi -q` |
| `nvidia-settings` | **versjon 510.47.03** (gammel Ubuntu-pakke, eldre enn driveren) | [Målt] `nvidia-settings -v` |
| NVIDIA-biblioteker | `libnvidia-ml.so.1` → 610.43.02, `libnvidia-api.so.1` finnes | [Målt] `/usr/lib/x86_64-linux-gnu` |
| Vifter | **2 vifter**, NVML-område **35–100 %**, policy 0 (auto, `NVML_FAN_POLICY_TEMPERATURE_CONTINOUS_SW`) | [Målt] NVML-lesing |
| Effektgrense | standard 245 W, område **105–275 W** | [Målt] |
| Temperaturgrenser | shutdown 100 °C, slowdown 97 °C, GPU max 88 °C, **akustisk mål 83 °C (område 65–88)**, minnetemperatur ikke støttet | [Målt] `nvmlDeviceGetTemperatureThreshold` |
| Klokke-offset (NVML) | kjerne (GPC) **−1000…+1000 MHz**, minne **−2000…+6000** (NVML-enheter), nå 0 | [Målt] `nvmlDeviceGetClockOffsets` (P0/P2/P5/P8) |
| Klokker per P-tilstand | P0: kjerne 300–2190, minne 7000. P8: kjerne 300–645, minne 405 | [Målt] `nvmlDeviceGetMinMaxClockOfPState` |
| Applications clocks | «Requested functionality has been deprecated» | [Målt] `nvidia-smi -q` |
| I2C-busser fra GPU-en | `i2c-0`…`i2c-6` = «NVIDIA i2c adapter 1,2,3,5,6,7,8 at 1c:00.0». `i2c-8` = «NVIDIA GPU I2C adapter» (funksjon `1c:00.3`) med `ccgx-ucsi` på 0x08, altså USB-C/PD-kontrolleren | [Målt] `/sys/bus/i2c/devices/*/name`, `readlink` |
| Rettigheter | `/dev/i2c-*` er `crw------- root root` | [Målt] `ls -l` |
| Utganger | DP-1..3, HDMI-A-1 og «Unknown-2» (sannsynligvis USB-C/VirtualLink) | [Målt] `/sys/class/drm` |

Konsekvenser:

- `nvidia-settings` 510 er så gammel at den ikke har NVML-baserte klokke- og viftekontroller (de kom i 570, se kapittel 3). Den er ubrukelig til overklokking på Wayland.
- Lesing fra NVML virker som vanlig bruker. Alle tallene over ble lest uten root. Overvåking trenger altså ikke daemonen.
- `i2c-8` (USB-C-kontrolleren) og adresse 0x08 skal **aldri** røres av RGB-koden.

---

## 2. NVML på driver 610 under Wayland

NVML (`libnvidia-ml.so.1`) snakker med kjernedriveren gjennom `/dev/nvidia*` og bruker ikke X11. Derfor virker NVML likt på Wayland, X11 og uten skjerm. **[Kilde]** LACT: «GPU configuration is handled by a system service that does not depend on a graphical session (Wayland/X11)» ([LACT README](https://github.com/ilya-zlobintsev/LACT)).

Funksjonsnavnene under er sjekket mot `nvml.h` fra [NVIDIA/go-nvml `gen/nvml/nvml.h`](https://github.com/NVIDIA/go-nvml/blob/main/gen/nvml/nvml.h) (commit `f83235b`, 2026-11-02, `NVML_API_VERSION_STR "13"`) og mot [NVML API Reference – Device Commands](https://docs.nvidia.com/deploy/nvml-api/api/group__nvmlDeviceCommands.html).

### 2.1 Viktig om støttenivå

**[Kilde]** I `nvml.h` står det at GeForce bare har *«Limited Support»*, mens Tesla/Quadro har *«Full Support»*. Mange funksjoner er dokumentert som «For X or newer **fully supported** devices». For GeForce betyr det bare at NVIDIA ikke garanterer at funksjonen virker. Det betyr ikke at den mangler. Appen må alltid kalle `Get…` først og håndtere `NVML_ERROR_NOT_SUPPORTED`.

**[Kilde]** Device Commands-siden sier: «Each of these requires root/admin access. Non-admin users will see an NVML_ERROR_NO_PERMISSION error code».

### 2.2 Funksjonstabell

| Formål | NVML-funksjon(er) – verifisert i `nvml.h` | Root? | Arkitektur ifølge dok. | Turing RTX 2080 |
|---|---|---|---|---|
| Effektgrense | `nvmlDeviceSetPowerManagementLimit_v2(dev, nvmlPowerValue_v2_t*)` (erstatter `…Limit(dev, unsigned mW)`); grenser: `nvmlDeviceGetPowerManagementLimitConstraints` | **Ja** («Requires root/admin permissions») | Kepler+ | **Ja.** Område 105–275 W [Målt]. Lagres ikke over omstart [Kilde] |
| Manuell viftehastighet | `nvmlDeviceSetFanSpeed_v2(dev, fan, speed%)` | Ja (gjelder hele Device Commands-gruppen) | Maxwell+, «all cuda-capable discrete products with fans» | **Ja.** 2 vifter, min 35 % [Målt]. **Advarsel i headeren:** setter policy til manuell («you can burn your GPU!») |
| Tilbake til auto-vifte | `nvmlDeviceSetDefaultFanSpeed_v2(dev, fan)` | Ja | alle med vifter | Ja |
| Vifte-policy | `nvmlDeviceGetFanControlPolicy_v2` / `nvmlDeviceSetFanControlPolicy(dev, fan, policy)`; verdier `NVML_FAN_POLICY_TEMPERATURE_CONTINOUS_SW` (0) og `NVML_FAN_POLICY_MANUAL` (1) | Set: **Ja** («Requires privileged user») | Maxwell+ | Get virker, policy = 0 [Målt] |
| Vifteinfo | `nvmlDeviceGetNumFans`, `nvmlDeviceGetMinMaxFanSpeed`, `nvmlDeviceGetFanSpeed_v2`, `nvmlDeviceGetTargetFanSpeed`, `nvmlDeviceGetFanSpeedRPM` | Nei | | Virker [Målt] (RPM ikke testet) |
| Klokke-offset (ny API) | `nvmlDeviceGetClockOffsets` / `nvmlDeviceSetClockOffsets(dev, nvmlClockOffset_t*)` med `type` (`NVML_CLOCK_GRAPHICS`, `NVML_CLOCK_MEM`) og `pstate` | Set: **Ja** («Requires privileged user») | Maxwell+ | Get virker, område −1000…+1000 / −2000…+6000 [Målt] |
| Klokke-offset (gammel) | `nvmlDeviceSetGpcClkVfOffset`, `nvmlDeviceSetMemClkVfOffset`, `…Get…MinMaxVfOffset` | Ja | **Merket `DEPRECATED(13.0)`** | Virker for lesing [Målt]. Bruk den nye API-en |
| Låst kjerneklokke | `nvmlDeviceSetGpuLockedClocks(dev, minMHz, maxMHz)` / `nvmlDeviceResetGpuLockedClocks` | **Ja** | **Volta+** | Forventes å virke [Antakelse; LACT bruker det for NVIDIA] |
| Låst minneklokke | `nvmlDeviceSetMemoryLockedClocks` / `nvmlDeviceResetMemoryLockedClocks` | Ja | **Ampere+** | **Nei**, Turing er eldre enn Ampere [Kilde: arkitekturkravet] |
| Temperaturmål | `nvmlDeviceSetTemperatureThreshold(dev, NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR, int*)`; område fra `…ACOUSTIC_MIN/MAX` | Ingen merknad i dok. [Antakelse: krever root i praksis] | Maxwell+ | Lesing virker: 83 °C, område 65–88 [Målt]. Skriving **med forbehold** |
| PowerMizer-modus | `nvmlDeviceGet/SetPowerMizerMode_v1` | [Antakelse: root] | Maxwell+ | Ikke testet. Med forbehold |
| Applications clocks | `nvmlDeviceSetApplicationsClocks` | Ja | | **Utfaset.** nvidia-smi melder «deprecated» [Målt] |
| Temperatur | `nvmlDeviceGetTemperatureV` (`nvmlDeviceGetTemperature` er merket `DEPRECATED(13.0)`) | Nei | | Virker |
| Overvåking ellers | `nvmlDeviceGetPowerUsage`, `…GetTotalEnergyConsumption`, `…GetClockInfo`, `…GetUtilizationRates`, `…GetMemoryInfo_v2`, `…GetCurrentClocksEventReasons`, `…GetPerformanceState` | Nei | | Virker (samme data som `nvidia-smi -q`) |

**Detaljer og fallgruver**

- **[Kilde]** `nvmlDeviceSetClockOffsets` kom i NVML/driver 555 ([NVIDIA-forum](https://forums.developer.nvidia.com/t/nvmldevicegetminmaxclockofpstate-nvmldevicesetclockoffsets-issues/318332)). Samme tråd rapporterer at offset i praksis gjelder alle P-tilstander, uansett hvilken `pstate` man sender. LACT skriver offset per P-tilstand og leser tilbake ([`nvidia.rs`](https://github.com/ilya-zlobintsev/LACT/blob/master/lact-daemon/src/server/gpu_controller/nvidia.rs)).
- **[Kilde]** Minne-offset i NVML er **dobbelt** så stort som tallet GWE og nvidia-settings viser ([NVIDIA-forum, «overclocking under Wayland guide»](https://forums.developer.nvidia.com/t/nvidia-gpu-overclocking-under-wayland-guide/290381)). +500 i GWE tilsvarer +1000 i NVML. Appen må vise én tydelig enhet. **[Antakelse]** Vis «effektiv MHz» og regn om internt.
- **[Kilde]** Effektgrense og låste klokker er «not persistent across reboots or driver unloads». Daemonen må derfor legge profilen på nytt ved oppstart og etter dvale. LACT har egen `suspend.rs` for dette.
- **[Kilde]** Viftekurve finnes ikke som fastvare-API i NVML. Kurven må kjøres i programvare: daemonen leser temperaturen og kaller `SetFanSpeed_v2` jevnlig. Det er nøyaktig det LACT gjør (`start_curve_fan_control_task`), og LACT kaller `set_default_fan_speed` for hver vifte når styringen stopper.
- **[Antakelse, men kritisk]** Hvis daemonen krasjer mens viften står i manuell modus, blir viften stående på siste hastighet. Derfor trengs reset ved signal, `ExecStopPost=` i systemd, en watchdog og en sikkerhetsgrense, for eksempel full vifte over 85 °C uansett kurve.

### 2.3 Hva Linux/NVIDIA *ikke* gir sammenlignet med Windows (MSI Afterburner og lignende)

| Windows-funksjon | Linux/NVIDIA-status |
|---|---|
| Kurveeditor (Ctrl+F i Afterburner) | **Ikke i NVML.** Mulig gjennom udokumentert NvAPI (`ClockClientClkVfPoints{Get,Set}Control`) i `libnvidia-api.so.1`, slik [LACT](https://github.com/ilya-zlobintsev/LACT/blob/master/lact-daemon/src/server/gpu_controller/nvidia/nvapi.rs) gjør. LACT-koden sier at Turing bare viser den ferdige kurven: «Cards without the base tuple (Turing) only expose the resulting curve». **Med forbehold:** udokumentert og kan slutte å virke ved driveroppdatering |
| Spenningsglidebryter «Core Voltage %» | **Ikke i NVML.** LACT bruker udokumentert `ClientVoltRails{Get,Set}Control` («voltage boost» i %). Med forbehold, ikke verifisert på Turing |
| Spenningsoffset i mV / fast spenning | **Ikke mulig.** NVIDIA har ingen slik API på Linux. LACT beskriver undervolting som en «pseudo-undervolt» med låste klokker og positiv offset ([LACT FAQ](https://github.com/ilya-zlobintsev/LACT/blob/master/docs/Frequently-asked-questions.md)), og senere VF-kurven |
| Låst minneklokke | Ikke på Turing (Ampere+) |
| Effektgrense over VBIOS-maks | Nei. 275 W er taket [Målt] |
| Vifte 0 % / under minimum | Nei. NVML-minimum er 35 % [Målt] |
| Automatisk OC-skanner (NVIDIA OC Scanner) | Ingen offentlig Linux-API. Kan lages selv som last og søk etter stabil offset. Med forbehold |
| Overlegg (RTSS) | Ikke som eget vindu på GNOME Wayland. Bruk MangoHud (kapittel 5) |
| Minnetemperatur | Turing GDDR6: `MEM_MAX` er «Not Supported» [Målt]. LACT leser VRAM- og hotspot-temperatur via udokumentert NvAPI på nyere arkitekturer |
| nvidia-settings med Coolbits | Virker ikke på Wayland, og installert versjon (510) er for gammel. Se kapittel 3 |

---

## 3. Eksisterende Linux-apper

| App | Hva den gjør | NVIDIA på Wayland? | Lisens og status | Hva vi lærer eller kan gjenbruke |
|---|---|---|---|---|
| **[LACT](https://github.com/ilya-zlobintsev/LACT)** | AMD/NVIDIA/Intel: overvåking, effektgrense, viftekurver, klokke-offset, låste klokker, VF-kurve (NVIDIA via NvAPI), profiler som bytter automatisk etter prosess eller gamemode | **Ja**, via NVML og NvAPI i en root-daemon | **MIT**. v0.10.1 (2026-08-29), aktiv | Den beste referansen. Arkitektur: `lactd` kjører som root i systemd og eksponerer [JSON over Unix-socket `/run/lactd.sock`](https://github.com/ilya-zlobintsev/LACT/blob/master/docs/API.md). Tilgang styres med `admin_group`/`admin_user` (eierskap på socketen), og polkit (`zbus_polkit`) brukes for de farligste operasjonene. Den har også «bekreft innen N sekunder, ellers rull tilbake» (`apply_settings_timer`) og reset ved signal. MIT gjør at vi kan låne kode og mønstre med attribusjon |
| **GreenWithEnvy (GWE)** | NVIDIA-overvåking, vifte og OC (Python/GTK) | **Nei.** Avhenger av NV-CONTROL (X11) ([issue #164](https://gitlab.com/leinardi/gwe/-/issues/164)) | GPL-3.0. **Arkivert, skrivebeskyttet** ([GitLab](https://gitlab.com/leinardi/gwe)) | UI-idéer, ellers ikke |
| **nvidia-settings + Coolbits** | Offisielt NVIDIA-panel. Coolbits i `xorg.conf` låste opp OC og vifte | Gammel versjon: **nei**. NV-CONTROL er en X-utvidelse, og Coolbits settes i X-konfig ([NVIDIA-forum](https://forums.developer.nvidia.com/t/setting-coolbits-on-wayland/174700)). Fra **570.86.16** bruker nvidia-settings NVML for klokker og vifte «to improve Wayland support», og «some operations may now require elevated privileges» ([GamingOnLinux om 570.86.16](https://www.gamingonlinux.com/2025/01/nvidia-beta-driver-570-86-16-released-with-vrr-for-multiple-displays-and-lots-of-improvements)) | Proprietær/GPL-2 (nvidia-settings er åpen kildekode) | Maskinen har **510.47.03**, altså før NVML-overgangen [Målt]. Bekrefter uansett at NVML er riktig vei |
| **[CoreCtrl](https://gitlab.com/corectrl/corectrl)** | Profilbasert styring av CPU og AMD-GPU | **Ingen NVIDIA-støtte.** README: «full AMD GPUs controls», og prosjektet er «in maintenance mode» | GPL-3.0-or-later | God root-modell: D-Bus-aktivert hjelper (`corectrl_helper`, `User=root`) og polkit-handling `…helper.init` med `auth_admin_keep` ([helper.policy.in](https://gitlab.com/corectrl/corectrl/-/blob/master/src/helper/helper.policy.in)) |
| **[TuxClocker](https://github.com/Lurkki14/tuxclocker)** | Qt-GUI og D-Bus-daemon. NVIDIA: vifter, effektgrense, overvåking. Klokker «Xorg works, also experimental support elsewhere» | Delvis. Mange NVIDIA-funksjoner krever Coolbits/X | GPL-3.0. Siste release 1.5.1 (2024-02) | Arkitekturen D-Bus-daemon og GUI. Distribusjonspakker bygges ofte uten NVIDIA-moduler |
| **[nvidia_oc](https://github.com/Dreaming-Codes/nvidia_oc)** | Liten Rust-CLI: effektgrense, kjerne- og minne-offset via NVML, X11 og Wayland | Ja | MIT | Minimalt eksempel på NVML fra Rust |
| **[MangoHud](https://github.com/flightlessmango/MangoHud)** | Vulkan/OpenGL-overlegg i spill: FPS, frametid, GPU-klokke/temp/effekt/vifte, FPS-grense, logging | Ja. Byggeflagget `with_nvml` er «Required for NVIDIA GPU metrics on wayland» | **MIT**, aktiv | Vi lager konfigfilen i stedet for å tegne selv (kapittel 5) |
| **[GOverlay](https://github.com/benjamimgois/goverlay)** | GUI som lager MangoHud-, vkBasalt- og ReplaySorcery-konfig | | GPL-3.0 (Pascal/Lazarus) | Viser at «GUI → MangoHud.conf» er et etablert mønster. Ikke kopier kode (GPL) |
| **[OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB)** | RGB for mange enheter, også GPU-er via I2C og NvAPI | Ja, via `/dev/i2c-*` og `libnvidia-api.so.1` | **GPL-2.0-or-later**. release_1.1 (2026-10-16) | Protokollkunnskap (kapittel 4). Den har også en [SDK-server på TCP-port 6742](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Documentation/OpenRGBSDK.md), så appen kan styre RGB gjennom OpenRGB uten å lenke inn GPL-kode |

**Lisensråd [Antakelse, ikke juridisk råd]:**

- Kode fra LACT, MangoHud og nvml-wrapper (MIT/Apache) kan gjenbrukes med attribusjon.
- Kode fra OpenRGB, GOverlay, CoreCtrl og TuxClocker (GPL) skal ikke kopieres inn hvis appen ikke selv blir GPL. Registeradresser og protokollfakta kan brukes til en egen implementasjon.
- Å snakke med OpenRGB over SDK-protokollen, som en separat prosess, er uproblematisk.

---

## 4. Palit-RGB på RTX 2080

### 4.1 Har kortet styrbar RGB?

- **[Kilde]** Palit oppgir RGB-LED på RTX 2080 og 2080 Ti GamingPro og GamingPro OC. En omtale skriver: «According to Palit, both versions have RGB LEDs», sannsynligvis i logoen på siden ([eTeknix](https://www.eteknix.com/palit-debuts-gamingpro-series-rtx-2080-and-2080-ti-cards/)). Palits **ThunderMaster** styrer lyset. GamingPro OC har produktkode **NE62080S20P2-180A**, 2 vifter, 3×DP, HDMI og USB-C ([QuietPC](https://quietpc.com/palit-ne62080s20p2-180a)).
- **[Målt]** Maskinen har 2 vifter og DP×3 + HDMI + en ekstra kontakt (USB-C). Det passer med GamingPro/GamingPro OC, men også med referansedesignet.
- **[Antakelse]** Jeg fant ingen bekreftelse på at **RTX 2080 «Dual»** har RGB. Hvis kortet er en Dual, kan det ha fast eller ingen belysning. **Første trinn er derfor å lese etiketten og se på kortet** (trinn 0 under).

### 4.2 Hvilken kontroller og protokoll? Bevisene

**A. ThunderMaster-opptak fra et søsterkort – sterkeste bevis.**
[OpenRGB issue #1938](https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1938) gjelder «Palit GeForce RTX 2080 Ti GamingPro OC» med PCI-ID `PCI\VEN_10DE&DEV_1E07&SUBSYS_1E0710DE`. Det kortet melder altså **NVIDIAs subsystem-ID lik enhets-ID-en**, nøyaktig samme mønster som ditt kort (`1E8710DE`). Vedlegget [`nvapi_65640.txt`](https://gitlab.com/CalcProgrammer1/OpenRGB/uploads/d7766911e7d2fd1eea1df887732bac64/nvapi_65640.txt) er en logg over ThunderMasters NvAPI-I2C-kall (1137 linjer). Analysen min [Kilde: selve loggen]:

- Alle kall går til **I2C-enhet 0x49**, med 1 byte registeradresse. Ingen andre adresser brukes.
- **Identifikasjon (bare lesing):** ThunderMaster leser registrene `0xF0–0xFF`. Svaret er `00 00 00 00 00 00 50 41 4C 49 54 20 31 30 30 00`, som er **ASCII «PALIT 100»** i `0xF6–0xFE`.
- **Statisk farge:** `0x60 = 0x01` (programvarestyrt/direkte), `0x6C = R`, `0x6D = G`, `0x6E = B`, `0x6F = 0x64` (lysstyrke 100).
- **Pusting (maskinvareeffekt):** `0x60 = 0x22`, så parametere i `0x61–0x6B`. Eksempel: `61=01 62=FF 63=00 64=FF 65=00 66=64 67=00 68=64 69=00 6A=00 6B=00`. Deretter farge i `0x6C–0x6F` og sekundærfarge/parametere i `0x70–0x75`.
- **Regnbue:** ser ut til å være laget av ThunderMaster i programvare, som en strøm av `0x60=01` og RGB-skriv. Registeret `0xE0` brukes ikke i opptaket.
- **Av:** RGB = `00 00 00` med `0x60 = 0x01`.
- Jeg ser ingen «lagre til flash»-kommando. **[Antakelse]** Innstillingen overlever kanskje ikke strømbrudd. Det må testes.

**B. OpenRGB-koden bekrefter samme protokoll.**
[`PalitGPUv2Controller.h`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/PalitGPUController/PalitGPUv2Controller.h) definerer:

- `0x60` = kontroll, med `0x01` = programvare/direkte og `0x22` = pusting
- `0x62–0x65` = pustehastighet
- `0x6C/6D/6E/6F` = R/G/B/lysstyrke
- `0x70–0x72` = sekundærfarge
- `0xE0/E1/E2` = modus/retning/hastighet
- `0xE4–0xE6` = tertiærfarge

Det stemmer med opptaket. I [`PalitGPUControllerDetect.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/PalitGPUController/PalitGPUControllerDetect.cpp) står blant annet:

- «Gainward GeForce RTX 2080 Phoenix GS» → v2 på **0x49** (subsystem Gainward `10B0:1E87`, og Gainward er Palits søstermerke)
- «Palit GeForce RTX 2070 SUPER Jetstream» → v2 på 0x49 **med NVIDIA-subsystem-ID**, altså enda et Palit-Turing-kort med NVIDIA-ID

Begge v1 og v2 krever `bus->info.port_id == 1`. På Linux leser OpenRGB port-ID fra bussnavnet «NVIDIA i2c adapter %hu at» ([`i2c_smbus_linux.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/i2c_smbus/Linux/i2c_smbus_linux.cpp)). **På denne maskinen er port 1 = `/dev/i2c-0`** [Målt].

**C. Alternativ: NVIDIA Illumination via NvAPI (styrt av driveren).**
OpenRGB har en «NVIDIA Illumination»-kontroller som bruker `NvAPI_GPU_ClientIllumZones{Get,Set}Control` gjennom `libnvidia-api.so.1` på Linux ([detektor](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/NVIDIAIlluminationController/NVIDIAIlluminationControllerDetect_Windows_Linux.cpp), [`nvapi.cpp`](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/dependencies/NVFC/nvapi.cpp) med `dlopen("libnvidia-api.so.1")`). Listen inneholder:

- **«Palit GeForce RTX 2060»** med `10de:1e89` / subsystem `10de:1e89` (samme NVIDIA-ID-mønster)
- RTX 2080 **FE** med subsystem `10de:12a6`. **Ikke** ditt kort

NvAPI-funksjonene er offentlig dokumentert i [NVIDIA/nvapi `nvapi.h`](https://github.com/NVIDIA/nvapi) (`NvAPI_GPU_ClientIllumDevicesGetInfo`, `…ZonesGetInfo/GetControl/SetControl`), men med «SUPPORTED OS: Windows 10 and higher». Bruk på Linux er altså uoffisiell.

**Konklusjon [Antakelse, godt underbygd]:**
Det mest sannsynlige er at kortet har en Palit-mikrokontroller på **0x49 på «NVIDIA i2c adapter 1» (`/dev/i2c-0`)** som snakker «PalitGPUv2»-protokollen og identifiserer seg med «PALIT 100» i `0xF6–0xFE`. En mindre sannsynlig mulighet er at RGB går gjennom NVIDIA Illumination, slik som Palit RTX 2060 (TU104), som er en senere revisjon. Ingenting av dette er verifisert på ditt kort.

### 4.3 Hva betyr NVIDIAs subsystem-ID (`10de:1e87`)?

- OpenRGB-detektorene matcher på fire ID-er (`REGISTER_I2C_PCI_DETECTOR(navn, fn, VEN, DEV, SUBVEN, SUBDEV, addr)`). Ingen detektor har `10DE:1E87` + `10DE:1E87` [Kilde: grep i `Controllers/` og `pci_ids/pci_ids.h`, commit `fc3aa07`]. **OpenRGB finner derfor ikke kortet**, og issue #1938 for 2080 Ti-varianten har stått åpen siden 2021.
- Vi kan ikke identifisere Palit ut fra PCI-ID. Andre produsenter som bruker NVIDIAs referanse-ID har annen eller ingen RGB-kontroller. For eksempel bruker «KFA2 RTX 2080 EX OC» NVIDIA-subsystem og adresse 0x23.
- **Identifikasjonen må derfor bygge på innhold:** les «PALIT»-signaturen på 0x49 før noe skrives. Det er det ThunderMaster selv gjør.

### 4.4 Risiko ved I2C på GPU

- **[Kilde]** OpenRGB advarer om at prosjektet bruker reverserte protokoller, og at det i sjeldne tilfeller har «bricket» maskinvare ([OpenRGB-wiki](https://openrgb-wiki.readthedocs.io/)). Et merge request for AMD-GPU-er sier at å prøve tilfeldige I2C-busser kan sette GPU-en i en uventet tilstand ([MR !2950](https://gitlab.com/CalcProgrammer1/OpenRGB/-/merge_requests/2950)).
- **[Kilde]** `i2cdetect`: «This program can confuse your I2C bus, cause data loss and worse!». SMBus quick write (`-q`) har ødelagt EEPROM-er, og receive byte (`-r`) har låst busser ([manpage](https://manpages.debian.org/testing/i2c-tools/i2cdetect.8.en.html)). **Ingen skanning.**
- **[Antakelse]** GPU-I2C-bussene kan også ha spenningsregulatorer (VRM), temperatursensorer og EEPROM-er. Noen av bussene er DDC mot skjermen, der EDID ligger på 0x50 og DDC/CI på 0x37. En feil skriving kan endre spenning, ødelegge EDID eller låse bussen til omstart. `i2c-8` har USB-C/PD-kontrolleren (`ccgx-ucsi` på 0x08) [Målt]. Palit v1-detektoren gjør en *quick write* mot 0x08, og det skal vi **aldri** etterligne.
- Den sikreste lesingen er én kombinert les av kjente registre (`0xF0–0xFF`) på én kjent adresse (0x49) på én buss (`i2c-0`). Det tilsvarer det ThunderMaster gjør ved hver oppstart.

### 4.5 Trinnvis plan – hvert trinn krever ditt «ja» før det kjøres

| Trinn | Hva | Risiko | Krever |
|---|---|---|---|
| **0** | Se på kortet: lyser logoen, hvilken farge, endrer den seg? Les etiketten på kortet eller esken (f.eks. `NE62080S20P2-180A` = GamingPro OC). | Ingen | Deg |
| **1** | Allerede gjort: busslisten fra `/sys` (port 1 = `i2c-0`, USB-C = `i2c-8`). | Ingen | – |
| **2** | Driverstyrt lesing via NvAPI i `libnvidia-api.so.1`: bare `NvAPI_GPU_ClientIllumZonesGetControl` (og eventuelt `…DevicesGetInfo`) for å se om driveren kjenner til RGB-soner. Bare Get-kall, ingen rå I2C. | Svært lav [Antakelse] | Ditt ja |
| **3** | **Én målrettet lesing:** som root, 16 × `read_byte_data(0x49, 0xF0…0xFF)` på `/dev/i2c-0`. Avbryt ved første NACK/feil. Ingen andre adresser eller busser. Forventet svar: «PALIT 100». Hvis det stemmer, les også `0x60–0x75` og lagre dem som «opprinnelig tilstand». | Lav, men ikke null (lesing via SMBus skriver registerpekeren) | Ditt ja, og `sudo` som du kjører selv |
| **4** | **Første skriving, uten endring:** skriv tilbake nøyaktig de verdiene trinn 3 leste (`0x60`, `0x6C–0x6F`). Lesing etterpå skal gi det samme. | Lav | Ditt ja |
| **5** | **Første synlige endring:** `0x60=0x01`, `0x6C/6D/6E` = rød, `0x6F=0x64`. Se på kortet. Gjenopprett med verdiene fra trinn 3. | Lav (samme sekvens som ThunderMaster) | Ditt ja |
| **6** | Implementer modusene statisk, av og pusting (`0x60=0x22` + `0x61–0x6B`) i daemonen, med en hviteliste: bare buss `i2c-0`, bare adresse 0x49, bare registrene `0x60–0x75`, og bare etter at «PALIT»-signaturen er sett i samme økt. Regnbue i programvare med høyst ~10–20 skriv per sekund. | Lav | Ditt ja |
| **7** | Hvis trinn 3 ikke finner «PALIT»: **stopp**. Ingen skanning eller gjetting. Alternativene er NvAPI Illumination (trinn 2) eller å la RGB-funksjonen stå som «ikke støttet på dette kortet». | – | – |

**Tilgang [Kilde]:** OpenRGB krever `i2c-dev`, og `/dev/i2c-*` må gjøres tilgjengelig med udev-regel eller root ([SMBusAccess.md](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Documentation/SMBusAccess.md)). I vår arkitektur gjør root-daemonen I2C-tilgangen, og vi trenger ingen udev-regel som åpner `/dev/i2c-0` for brukere.

---

## 5. Overlegg i spill på Wayland

- **[Kilde]** GNOME Mutter støtter ikke `wlr-layer-shell` ([wayland.app](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)). En vanlig app kan derfor ikke legge et «alltid øverst»-vindu over et spill i fullskjerm. Et RTSS-lignende overlegg må tegnes **inne i spillet**, og det er det MangoHud gjør som Vulkan-lag eller OpenGL-preload.
- **Anbefaling: ja, generer MangoHud-konfig.** [Kilde, MangoHud README]:
  - Konfigfilene leses i rekkefølgen `<spillmappe>/MangoHud.conf`, `~/.config/MangoHud/<exe>.conf`, `~/.config/MangoHud/wine-<exe>.conf` (Proton/Wine) og til slutt `~/.config/MangoHud/MangoHud.conf`.
  - Miljøvariablene `MANGOHUD_CONFIG` og `MANGOHUD_CONFIGFILE` kan overstyre.
  - Nøkler for GPU-data: `gpu_core_clock`, `gpu_mem_clock`, `gpu_temp`, `gpu_power`, `gpu_power_limit`, `gpu_fan`, `gpu_voltage`, `throttling_status`, `vram`, `fps_limit`, `custom_text`, `position`, `toggle_hud`.
  - MangoHud overvåker konfigfilen med inotify (`IN_MODIFY`, [`src/notify.cpp`](https://github.com/flightlessmango/MangoHud/blob/master/src/notify.cpp)), så endringer slår inn mens spillet kjører.
  - `control=<navn>` åpner en Unix-socket som `mangohudctl` kan styre.
- **Arbeidsdeling:** Agentic GPU lager «overleggsprofiler» (minimal, FPS, full) som `MangoHud.conf` / `<exe>.conf`, og viser Steam-startvalget `mangohud %command%`. Appen tegner aldri inn i spill selv.
- **[Kilde]** Med gamescope må man bruke `gamescope --mangoapp`. Vanlig MangoHud støttes ikke der.
- **[Kilde] Vær obs:** Repoet har en ny «MangoHud Next» (server/klient-omskriving) med **YAML-konfig** (`~/.config/MangoHud/MangoHud.yml`) og JSON-spørringer (`mangohud-next get_system`). **[Antakelse]** Generatoren bør ha et eget lag for formatet, slik at den kan skrive både `.conf` og `.yml`.
- MangoHud er ikke installert ennå. Appen bør oppdage det (`which mangohud`) og foreslå installasjon (`sudo apt install mangohud`). Appen skal ikke installere selv.

---

## 6. Slik kaller appen NVML, og slik løses root

### 6.1 Tre måter å kalle NVML

| Vei | Hvordan | Fordeler | Ulemper |
|---|---|---|---|
| **Rust: [`nvml-wrapper`](https://github.com/rust-nvml/nvml-wrapper)** | 0.13.0 (2026-08-31), MIT OR Apache-2.0. Laster `libnvidia-ml.so.1` dynamisk. Har `set_power_management_limit`, `set_fan_speed`, `set_default_fan_speed`, `set_fan_control_policy`, `set_gpu_locked_clocks`, `set_mem_locked_clocks`, `clock_offset`/`set_clock_offset`, `set_temperature_threshold`, `set_power_mizer_mode` [Kilde: `src/device.rs` i crate 0.13.0] | Typesikker. Det samme som LACT bruker (LACT pinner en git-revisjon) | – |
| **Node: [`koffi`](https://koffi.dev/)** | 3.3.6, MIT. `koffi.load('libnvidia-ml.so.1')` og deklarasjon av C-signaturer. Kjøres i Electron main/utility-prosess, aldri i renderer | Ingen kompilering. Greit for **lesing** (overvåking uten daemon) | Ville kreve at hele Electron kjører som root for å skrive. **Uaktuelt for skriving** |
| **Lite hjelpeprogram** | En Rust-binær som gjør én operasjon (`agentic-gpu-helper set-power 230`) og startes via `pkexec` | Enkelt | Passordspørsmål for hver endring. Kan ikke kjøre viftekurve over tid. Bare egnet som nødløsning |

### 6.2 Anbefalt rettighetsmodell (LACT-mønsteret med polkit)

```
┌──────────── bruker (Wayland-økt) ────────────┐        ┌──────── root (systemd) ─────────────────────┐
│ Agentic GPU GUI (Electron/TS)                │        │ agentic-gpud (Rust)                          │
│  • leser NVML direkte via koffi (fallback)   │  JSON  │  • nvml-wrapper: effekt, klokker, vifter     │
│  • viser grafer, profiler, MangoHud-editor   │◄──────►│  • viftekurve-løkke + sikkerhetsgrenser      │
│  • skriver ~/.config/MangoHud/*.conf         │ Unix-  │  • (senere) NvAPI via libloading: VF-kurve   │
└──────────────────────────────────────────────┘ socket │  • (senere) /dev/i2c-0 @0x49: Palit-RGB      │
                                                /run/   │  • polkit-sjekk per handling                 │
                                                agentic-│  • bekreft-eller-rull-tilbake (15 s)         │
                                                gpud.   │  • reset ved SIGTERM/krasj, ExecStopPost     │
                                                sock    │  • legger profil på nytt ved boot og resume  │
                                                        └──────────────────────────────────────────────┘
```

- **Transport:** Unix-socket med linjedelt JSON, som `lactd` ([API.md](https://github.com/ilya-zlobintsev/LACT/blob/master/docs/API.md)). Det er trivielt fra Node (`net.createConnection`) og Rust (tokio). D-Bus på systembussen er også mulig (`zbus` 6.2.1), men Node-bibliotekene for D-Bus er gamle (`dbus-next` 0.10.2 fra 2021). Unix-socket er derfor enklest for Electron. [Kilde: npm/crates.io-metadata]
- **Autorisasjon:**
  1. Socketen eies av root, med gruppe `agentic-gpu` (eller `sudo`) og modus 0660.
  2. Daemonen henter klientens UID og PID (`SO_PEERCRED`).
  3. For hver klasse handlinger kaller daemonen polkit `CheckAuthorization` med `ALLOW_USER_INTERACTION`. Det er samme mønster som LACT (`zbus_polkit`, [handler.rs](https://github.com/ilya-zlobintsev/LACT/blob/master/lact-daemon/src/server/handler.rs)) og CoreCtrl (`auth_admin_keep`).
- **Forslag til polkit-handlinger [Antakelse]:**
  - `…monitor` (ingen autorisasjon)
  - `…tune` (effekt, vifte, offset: `auth_admin_keep` eller `yes` for aktiv lokal økt i `agentic-gpu`-gruppen)
  - `…advanced` (VF-kurve og NvAPI: alltid `auth_admin`)
  - `…rgb` (I2C: `auth_admin_keep`)
- **Sikkerhet i daemonen [Antakelse, delvis etter LACT]:**
  - Hvitelistede verdiområder lest fra NVML, aldri fra klienten.
  - «Bekreft innen 15 s, ellers rull tilbake» for klokke- og effektendringer.
  - Viftene alltid tilbake til auto ved stopp.
  - Temperaturvakt (vifte 100 % over en terskel).
  - Ingen generell «skriv til I2C»-API, bare semantiske RGB-kall.
  - systemd-herding (`ProtectSystem=strict`, `DeviceAllow=/dev/nvidia* /dev/i2c-0`, `NoNewPrivileges=yes`).

---

## 7. Anbefaling

### 7.1 Stack

| Del | Valg | Begrunnelse |
|---|---|---|
| **Privilegert daemon** | **Rust**: `nvml-wrapper`, `tokio`, `zbus_polkit`, `libloading` (NvAPI senere), `i2cdev` eller rå `ioctl` (RGB senere), systemd-unit og polkit-policy | Samme språk som Agentic Klipp. Følger LACT, som er MIT-lisensiert og vist å virke. Minne- og typesikkerhet er viktig i en root-tjeneste |
| **GUI** | **Electron + TypeScript**, som gjenbruker HTML/JS-en fra localhost-demoen (alpha v0.0.1) og kompetansen fra Agentic Desktop | Demoen blir ekte uten omskriving. Web passer godt til grafer og kurveeditorer. Alternativer: Tauri (Rust og web-UI, lettere) eller GTK4/libadwaita (som Klipp, mest «GNOME-native»). Valget av GUI er mindre kritisk, fordi all risikabel logikk ligger i daemonen |
| **Overvåking uten daemon** | `koffi` → NVML, bare lesing | Appen viser data selv om daemonen ikke er installert |
| **Overlegg i spill** | MangoHud-konfiggenerator (`.conf`, senere `.yml` for MangoHud Next) | Eneste realistiske vei på GNOME Wayland |
| **RGB** | Egen PalitGPUv2-implementasjon i daemonen etter godkjent trinnplan, **eller** OpenRGB SDK (port 6742) hvis brukeren heller vil ha OpenRGB | Vi unngår GPL-kode, og OpenRGB oppdager uansett ikke kortet |
| **Pakking** | `.deb` med daemon, systemd-unit, polkit-fil og GUI | Ubuntu er målet. Flatpak gjør root-daemon tungvint |

### 7.2 Prioritert funksjonsliste

**A. Sikkert mulig (bygg først)**

1. Overvåking: temperatur, effekt, klokker, utnyttelse, VRAM, vifte-% og -RPM, P-tilstand og throttling-årsaker (`GetCurrentClocksEventReasons`), med historikk og CSV-eksport. *Uten root.*
2. Effektgrense 105–275 W (standard 245) med bekreft-eller-rull-tilbake. *Root.*
3. Viftestyring: auto, fast % (35–100) og **egen viftekurve i daemonen**, med hysterese og sikkerhetsgrenser. Gjenoppretter alltid auto. *Root.*
4. Klokke-offset kjerne (praktisk +0…+200 MHz av NVML-området ±1000) og minne (vis i én tydelig enhet; NVML-området −2000…+6000) via `nvmlDeviceSetClockOffsets`. *Root.*
5. Låst kjerneklokke (min/maks, 300–2190 MHz) via `SetGpuLockedClocks`. Kombinert med offset gir det «pseudo-undervolt». *Root. Volta+, forventes å virke.*
6. Profiler (lagre/last) og **automatisk profilbytte** når et spill starter (overvåke `/proc`, eventuelt gamemode). Legges på nytt ved boot og resume.
7. MangoHud-konfiggenerator: overleggsprofiler, plassering, FPS-grense, hurtigtaster, per-spill-filer og Steam-startvalg.

**B. Mulig med forbehold**

8. Temperaturmål (`ACOUSTIC_CURR`, 65–88 °C). Lesing virker, skriving er ikke verifisert på GeForce/Turing.
9. PowerMizer-modus («foretrekk maks ytelse»). Ikke testet på Turing.
10. **VF-kurveeditor og «voltage boost»** via udokumentert NvAPI (`libnvidia-api.so.1`) etter LACTs modell. Krever ekstra advarsel og en «avansert»-polkit-handling, og kan slutte å virke ved driveroppdatering.
11. **Palit-RGB** (statisk, av, pusting, programvare-regnbue) på `i2c-0` @ 0x49. **Bare etter trinn 0–5 i kapittel 4.5 og bare hvis «PALIT»-signaturen bekreftes.**
12. Hotspot-temperatur via udokumentert NvAPI (LACT gjør det for nyere arkitekturer; uklart for Turing).
13. Egen «OC-skanner» (last og trinnvis offset med stabilitetssjekk). Mulig, men tidkrevende og risikabel. Lav prioritet.

**C. Ikke mulig på Linux/NVIDIA (vis som deaktivert, med forklaring)**

- Spenningsoffset i mV og fast spenning (bare VF-kurve eller boost-% via udokumentert NvAPI, se B10).
- Låst minneklokke på Turing (Ampere+).
- Effektgrense over 275 W og vifte under 35 % eller 0 RPM.
- Eget overleggsvindu over spill på GNOME Wayland (bruk MangoHud).
- Coolbits/NV-CONTROL-baserte funksjoner på Wayland.
- Automatisk RGB-gjenkjenning basert på PCI-ID (kortet melder NVIDIAs referanse-ID).

### 7.3 Konsekvenser for alpha-demoen (simulerte kontroller)

Bruk de målte områdene, så demoen ligner det ekte:

- Effekt 105–275 W (standard 245)
- Kjerne-offset −1000…+1000 MHz (foreslå skjermgrense −200…+300)
- Minne-offset i én tydelig enhet
- 2 vifter, minimum 35 %
- Temperaturmål 65–88 °C (standard 83)
- Låst klokke 300–2190 MHz

Marker VF-kurve og RGB som «eksperimentell», og spenning i mV og låst minneklokke som «ikke tilgjengelig på Linux/Turing».

---

## Metode og sikkerhet under researchen

- **Ingenting er endret** på maskinen eller kortet. Kjørt: `lspci -nnv`, `modinfo i2c_nvidia_gpu`, `nvidia-smi -q` / `--query-gpu` / `-q -d CLOCK,TEMPERATURE,PERFORMANCE,SUPPORTED_CLOCKS`, `nvidia-settings -v` (bare versjon), `ls -l /dev/i2c-0`, og lesing av `/sys` og `/proc`.
- I tillegg kjørte jeg et eget Python-skript med **bare `nvml…Get…`-kall** (samme type kall som `nvidia-smi -q`, som normal bruker, uten Set/Reset), for å lese vifteområde, offsetområder og temperaturgrenser: `/tmp/claude-1000/-home-julian--claude-spares-spare-6B1YYE/b1cbda93-d303-42c5-84d3-fb3430d92402/scratchpad/nvml_probe_readonly.py`.
- Ingen `/dev/i2c-*` er åpnet. Ingen `i2c*`-verktøy er brukt (de er heller ikke installert).
- Kildekode er lastet ned for lesing, ikke bygget eller kjørt, til `/tmp/claude-1000/agentic-gpu-research/src/`: OpenRGB `fc3aa07`, LACT `d11618a`, go-nvml `f83235b`, MangoHud `0c0098d`, NVIDIA/nvapi, CoreCtrl og nvml-wrapper 0.13.0. ThunderMaster-opptaket ligger i `/tmp/claude-1000/agentic-gpu-research/web/nvapi_65640.txt`.
