#!/usr/bin/env python3
"""Finn sannsynlig synlige norske eller uoversatte tekststrenger utenfor tr!/trf!/trn!,
og kontroller at hvert språk i nedtrekkslista er fullstendig og konsekvent oversatt.

Bruk:
  python3 app/scripts/i18n-revisjon.py                  # vis uavklarte funn og returner feil ved funn
  python3 app/scripts/i18n-revisjon.py --check          # samme kontroll, eksplisitt i CI
  python3 app/scripts/i18n-revisjon.py --all            # vis også dokumenterte unntak
  python3 app/scripts/i18n-revisjon.py --include-other  # ta også med filer eid av andre grener
  python3 app/scripts/i18n-revisjon.py <fil...>         # skann bestemte kildefiler

Uten filnavn kontrolleres også katalogene for alle språk (se `catalog_findings`):
samme språkliste i Rust, uttrekksverktøyet, katalogmappene og skrivebordsfilen,
ingen hull, ingen tegn fra et annet språk, samme mellomrom og linjeskift som
kilden, og appnavnet uendret.
"""

import argparse
from dataclasses import dataclass
import importlib.util
import json
from pathlib import Path
import re
import sys

APP = Path(__file__).resolve().parents[1]
CRATES = ("rigg", "riggd", "rigg-core")

# Utvikler- og skjermbildeverktøy som ikke er en del av produksjons-UI.
EXCLUDED_FILES = {
    "crates/rigg/src/logg_ui/bilder.rs",
    "crates/rigg/src/pages/oversikt/bilder.rs",
    "crates/rigg/src/pages/styring_bilder.rs",
}

# Håndteres på rel/demo-rgb (rapporteres separat med --include-other).
OTHER_BRANCH_FILES = (
    "crates/rigg/src/pages/lys.rs",
    "crates/rigg/src/pages/lys_kort.rs",
    "crates/rigg/src/rgb.rs",
    "crates/rigg/src/lys3d/",
    "crates/rigg/src/system_install.rs",
    "crates/rigg/src/settings/styring.rs",
    "crates/rigg/src/settings/utseende.rs",
    "crates/rigg/src/settings/data.rs",
    "crates/rigg/src/widgets/locked.rs",
    "crates/rigg-core/src/easter.rs",
    "crates/rigg/src/easter.rs",
    "scripts/pakk-utgivelse.py",
)

# File-level allowlist ONLY for non-UI / protocol / CSV / daemon-internal modules.
ALLOWLIST_FILES: dict[str, str] = {
    "crates/rigg-core/src/csv.rs": "CSV-kolonneoverskrifter holdes stabile på bokmål jf. docs/produksjon/i18n.md",
    "crates/rigg-core/src/hjelper.rs": "Protokollmeldinger på sokkelen og match-mønstre oversatt via display_error/display_diagnostic/display_message",
    "crates/rigg-core/src/proto.rs": "Protokollfeilmeldinger på sokkelen (oversettes i klienten ved visning)",
    "crates/rigg-core/src/rgb.rs": "Protokoll- og domenevalidering for RGB-motor/hjelper",
    "crates/rigg-core/src/rgb_auth.rs": "Tekniske feilmeldinger ved tolking av RGB-tillatelsesfil",
    "crates/rigg-core/src/rgb_txn.rs": "Protokoll- og transaksjonsfeil i RGB-motoren",
    "crates/rigg-core/src/sensormeta.rs": "Norsk referansetabell for ikke-lokalisert CSV-eksport (localized=true bruker tr!)",
    "crates/rigg-core/src/tokens.rs": "Interne feilmeldinger for CSS-token-parser og kontrastkontroll",
    "crates/rigg-core/src/txn.rs": "Protokollvalidering og LOCKED_TIP (oversettes ved visning via display_validation_*)",
    "crates/riggd/src/daemon.rs": "Protokoll-/stderr-meldinger fra bakgrunnstjenesten",
    "crates/riggd/src/hjelper.rs": "Protokollfeil fra hjelperklienten (oversettes i vinduet via rigg_core::hjelper::display_error)",
    "crates/riggd/src/main.rs": "CLI-flagg, --help og oppstartslogger på stderr for agentic-riggd",
    "crates/riggd/src/ops.rs": "Protokollvalideringsfeil i RPC-håndtereren",
    "crates/riggd/src/rgb_hjelper.rs": "Protokoll- og transportfeil fra separat RGB-hjelperklient",
    "crates/riggd/src/sampler.rs": "OS-trådnavn og intern feilsøkingslogg",
    "crates/riggd/src/server.rs": "Oppstartsfeil på stderr for Unix-socket",
    "crates/riggd/src/store.rs": "Filnavn (tjeneste.json), backup-suffiks og stderr-feil",
}

# Synlige daemonmoduler (varsler og systemkurv) har bare eksakte unntak, slik at ny
# uoversatt tekst der fortsatt fanges. Lange tekniske literaler matches på prefiks.
ALLOWLIST_PREFIX: dict[tuple[str, str], str] = {
    ("crates/riggd/src/tray.rs", "<node>\n  <interface name=\"org.kde.StatusNotifierItem\">"): "D-Bus-introspeksjon (XML)",
}

# Exact (file, literal) allowlist for UI and shared modules so new untranslated UI text is always caught.
ALLOWLIST_EXACT: dict[tuple[str, str], str] = {
    ("crates/rigg-core/src/i18n/mod.rs", "Norsk bokmål"): "Egennavn på kildespråket",
    ("crates/rigg-core/src/lib.rs", "{NAME} {VERSION} · bygg {BUILD_STR}"): "CLI --version-utdata (UI bruker trf!)",
    ("crates/rigg-core/src/lib.rs", "Versjon {VERSION} · bygg {BUILD_STR}"): "Referansemetode for CLI/tester (UI bruker trf!)",
    ("crates/rigg-core/src/report/gpu.rs", "Ukjent"): "Rå bokmålsverdi fra leverandørstreng matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/gpu.rs", "grafikk og compute"): "Rå prosesskategori matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/gpu.rs", "InfoROM"): "Teknisk faguttrykk / NVIDIA-begrep",
    ("crates/rigg-core/src/report/gpu.rs", "Resizable BAR"): "Teknisk faguttrykk / PCIe-begrep",
    ("crates/rigg-core/src/report/gpu.rs", "Tensor / RT"): "Teknisk faguttrykk (NVIDIA-kjerner)",
    ("crates/rigg-core/src/report/gpu.rs", "TMU / ROP"): "Teknisk faguttrykk (GPU-enheter)",
    ("crates/rigg-core/src/report/gpu.rs", "DirectX / Vulkan / OpenGL"): "Produktnavn / API-navn",
    ("crates/rigg-core/src/report/system.rs", "Stasjonær"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Lavprofil stasjonær"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Minitårn"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Tårn"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Bærbar"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Håndholdt"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Alt-i-ett"): "Rå DMI-verdi matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Ukjent prosessor"): "Rå reservenavn fra rigg-hw matchet og oversatt med tr!",
    ("crates/rigg-core/src/report/system.rs", "Boost"): "Teknisk faguttrykk (CPU Boost)",
    ("crates/rigg-core/src/report/system.rs", "Trådløst nett"): "Rå PCI-klassestreng matchet og oversatt med tr!",
    ("crates/rigg-core/src/sensor_key.rs", "disk/"): "Stabil sensornøkkel",
    ("crates/rigg-core/src/sensor_key.rs", "disk/{id}/read"): "Stabil sensornøkkel",
    ("crates/rigg-core/src/sensor_key.rs", "disk/{id}/write"): "Stabil sensornøkkel",
    ("crates/rigg-core/src/sensor_key.rs", "disk/{id}/busy"): "Stabil sensornøkkel",
    ("crates/rigg-core/src/sensor_key.rs", "disk/{id}/temp"): "Stabil sensornøkkel",
    ("crates/rigg-core/src/settings.rs", "Oversikt"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Sensorer"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Grafer"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Prosessor"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Disker"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Skjermkort › Klokker og effekt"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Skjermkort › Vifter"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Skjermkort › RGB-lys"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Oppgaver"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg-core/src/settings.rs", "Nettverk"): "Referansetabell PAGES; UI bruker page_name() med tr!",
    ("crates/rigg/src/pages/nettverk.rs", "IPv4"): "Teknisk faguttrykk (protokollnavn)",
    ("crates/rigg/src/pages/nettverk.rs", "IPv6"): "Teknisk faguttrykk (protokollnavn)",
    ("crates/rigg/src/pages/nettverk.rs", "TCP {tcp} · UDP {udp}"): "Protokollnavn og antall",
    ("crates/rigg/src/pages/nettverk.rs", "PID {pid}"): "Teknisk faguttrykk (prosess-ID)",
    ("crates/rigg-core/src/specs.rs", "Minneleverandøren (Samsung eller Micron) står i VBIOS-en. NVML oppgir den ikke, og VBIOS kan ikke leses uten root."): "MEM_NOTE-konstant oversatt i mem_vendor_note_text() med tr!",
    ("crates/rigg-core/src/stats.rs", ".ødelagt-{now}"): "Filnavnsuffiks for ødelagt sikkerhetskopi",
    ("crates/rigg/src/app.rs", "--innstillinger"): "CLI-flagg",
    ("crates/rigg/src/app.rs", "ingen skjerm"): "Intern expect-melding ved manglende skjerm",
    ("crates/rigg/src/app.rs", "RIGG_RAMMER {} bilder på 10 s, {:.1} ms i alt, snitt {:.2} ms, verste {:.2} ms"): "Feilsøkingsutdata på stderr bak RIGG_RAMMER",
    ("crates/rigg/src/infobar.rs", "<b>{}</b> {}"): "Markup rundt allerede lokaliserte tekster",
    ("crates/rigg/src/main.rs", "--innstillinger"): "CLI-flagg",
    ("crates/rigg/src/pages/disker.rs", "disk/{}/read"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/disker.rs", "disk/{}/write"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/disker.rs", "sim"): "Intern kildekode matchet mot tr!(\"simulert\")",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 svarte ikke ("): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 har ingen SMART for denne disken. USB-broen sender som regel ikke SMART videre."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 kjenner ikke disken."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "SMART kommer fra UDisks2, som ikke svarer på systembussen."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "Disken støtter ikke SMART."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "SMART er slått av på disken."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 har ikke lest SMART ennå."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "USB-broen sender ikke SMART videre."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 har ingen SMART for denne disken."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/disker.rs", "UDisks2 svarer ikke på systembussen."): "Rå bokmålsstreng fra tjenesten matchet og oversatt i disk_note",
    ("crates/rigg/src/pages/grafer.rs", "grafer.pause"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/grafer.rs", "grafer.markor"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/grafikkort.rs", "uuid"): "Intern rad-ID",
    ("crates/rigg/src/pages/oppgaver.rs", "CPU ↓"): "Teknisk kolonneoverskrift (forkortelse + pil)",
    ("crates/rigg/src/pages/oppgaver.rs", "{} · PID {}"): "Prosessnavn og numerisk prosess-ID",
    ("crates/rigg/src/pages/oversikt.rs", "sensorer?vis=net/rx"): "Intern navigasjonsrute",
    ("crates/rigg/src/pages/oversikt.rs", "disk/{}/temp"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/oversikt.rs", "oversikt.markor"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "disk/{}/temp"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/sensorer.rs", "disk/{}/read"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/sensorer.rs", "disk/{}/write"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/sensorer.rs", "disk/{}/busy"): "Stabil sensornøkkel",
    ("crates/rigg/src/pages/sensorer.rs", "vis="): "Intern spørringsparameter",
    ("crates/rigg/src/pages/sensorer.rs", "sensorer.apne"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "sensorer.lukk"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "sensorer.markor"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "sensorer.filter"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "sensorer.rull"): "Intern nøkkel for engangstips",
    ("crates/rigg/src/pages/sensorer.rs", "lock"): "Ikon-ID i betingelsen for et lokalisert verktøytips",
    ("crates/rigg/src/pages/sensorer.rs", "{what}: {why}"): "Setter sammen to lokaliserte tekster med skilletegn",
    ("crates/rigg/src/pages/viftekurve.rs", ", {n} målinger"): "Eksakt bokmålsform i active()==Nb-gren; øvrige språk bruker trn!",
    ("crates/rigg/src/prefs.rs", ".ødelagt-{stamp}"): "Filnavnsuffiks for ødelagt sikkerhetskopi",
    ("crates/rigg/src/rapport_ui.rs", "agentic-rigg-rapport-{:04}-{:02}-{:02}_{:02}-{:02}.{}"): "Standard eksportfilnavn",
    ("crates/rigg/src/settings/om.rs", "{SIGNATURE}. {note_text}."): "Egennavn og allerede lokalisert tekst",
    ("crates/rigg/src/shortcuts.rs", " (uten ring)"): "Feilsøkingsutdata i fokuslogg",
    ("crates/rigg/src/sidebar.rs", "Ukjent prosessor"): "Rå reservenavn fra rigg-hw matchet og oversatt med tr!",
    ("crates/rigg/src/sidebar.rs", "sensorer?vis={id}"): "Intern navigasjonsrute",
    ("crates/rigg/src/sidebar.rs", "disk/"): "Stabil sensornøkkelprefiks",
    ("crates/rigg/src/sidebar.rs", "{}\n{desc}"): "To allerede lokaliserte linjer i samme verktøytips",
    ("crates/rigg/src/state.rs", "skallet er ikke bygd"): "Intern expect-melding",
    ("crates/rigg/src/widgets/chart.rs", "bilde med oppsett"): "Intern expect-melding",
    ("crates/rigg/src/widgets/chart.rs", "grafen har en visning"): "Intern expect-melding",
    ("crates/rigg/src/widgets/chart.rs", "GPU-temperatur"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "GPU-last"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "GPU-effekt"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "GPU-vifte"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "CPU-temperatur"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "CPU-last"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "Minne brukt"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "Disk-I/O"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "disk/{}/temp"): "Stabil sensornøkkel",
    ("crates/rigg/src/widgets/chart.rs", "{}{u}"): "En formatert verdi med enhet",
    ("crates/rigg/src/widgets/chart/math.rs", "Sekunder"): "CSV-kolonneoverskrift jf. docs/produksjon/i18n.md",
    ("crates/rigg/src/widgets/version_log.rs", "Versjonsloggen må være en liste"): "Intern expect-melding ved tolking av innebygd versjonslogg",
    ("crates/rigg/src/window.rs", "appen lever så lenge vinduet gjør"): "Intern expect-melding",
    ("crates/riggd/src/tray.rs", "Ukjent egenskap"): "D-Bus-feilmelding til skallet, ikke brukertekst",
    ("crates/riggd/src/tray.rs", "Ukjent metode"): "D-Bus-feilmelding til skallet, ikke brukertekst",
    ("crates/riggd/src/varsler.rs", "disk:{}:fail"): "Stabil intern varselnøkkel",
    ("crates/riggd/src/varsler.rs", "disk:{}:bad:{bad}"): "Stabil intern varselnøkkel",
    ("crates/riggd/src/varsler.rs", "disk:{}"): "Stabil intern varselnøkkel",
}

TOKEN = re.compile(
    r'//[^\n]*|/\*|(?P<raw>r(?P<hash>\#*)"(?s:.*?)"(?P=hash))'
    r'|(?P<string>"(?:[^"\\]|\\.)*")|\'(?:[^\'\\\n]|\\[^\n]+?)\''
    r'|[\w]+|[^\s]',
    re.S,
)
BLOCK = re.compile(r'/\*|\*/')

NORWEGIAN_CHARS = re.compile(r'[æøåÆØÅ]')
NORWEGIAN_WORDS = re.compile(
    r'\b('
    r'ikke|med|til|fra|eller|som|kan|skal|ved|etter|uten|alle|ingen|ukjent|feil|'
    r'lagre|lagret|lagres|tilbakestill|slett|avbryt|lukk|stopp|vis|skjul|velg|'
    r'kort|kortet|vifte|vifter|klokke|klokker|effekt|temperatur|minne|disk|disker|'
    r'prosessor|skjermkort|skjermkortstyring|sensor|sensorer|graf|grafer|oversikt|'
    r'oppgaver|innstillinger|sidefelt|tjeneste|tjenesten|hjelper|hjelperen|'
    r'logg|logging|varsel|varsler|kjerne|kjerner|snitt|maks|nedklokking|tomgang|'
    r'dvale|kobler|leser|aktiver|deaktiver|oppdater|versjon|bygg|sekund|sekunder|'
    r'minutt|minutter|timer|dager|mappe|eksport|kopier|navn|verdi|verdier|grense|'
    r'standard|automatisk|manuell|festet|flytt|tilkoblet|svarer|maskinvare|'
    r'straks|nullstill|tilgang|tillatelse|fil|filer|rapport|advarsel|kritisk|'
    r'spenning|hastighet|bruk|brukt|ledig|tilgjengelig|aktiv|inaktiv|kjør|kjører|'
    r'belastning|last|avslutt|endre|sider|seksjon|lys|lysstyrke|bakgrunn|sist|siden|'
    r'forrige|neste|mer|mindre|detaljer|hjem|nettverk|farge|farger|gjenopprett|'
    r'og|er|av|blir|har|nye|nytt|helt|sammen|opp|ned|inn|ut|under|over|ett|'
    r'gjeldende|enhet|kjent|henter|egen|egne|knapp|lag|visning|skriv|tilstand|opptak'
    r')\b',
    re.IGNORECASE,
)

UI_CALL_ARGS: dict[str, set[int]] = {
    "label": {0},
    "set_text": {0, 1},
    "set_title": {0},
    "set_label": {0},
    "set_tooltip_text": {0},
    "set_tooltip_markup": {0},
    "set_placeholder_text": {0},
    "set_markup": {0},
    "tip": {1},
    "announce": {0},
    "set_subtitle": {0},
    "set_heading": {0},
    "set_body": {0},
    "button": {1},
    "tag": {0},
    "item": {1},
    "row": {0, 1},
    "row_wrap": {0, 1},
    "switch_row": {0, 1},
    "group": {0},
    "kv_na": {1, 3},
    "panel": {0},
    "na": {0},
    "info": {0},
    "body": {0},
    "action": {0},
}

UI_METHOD_OR_CTOR = {
    ("Toast", "new"),
    ("Notification", "new"),
    ("Label", "new"),
    ("Button", "with_label"),
    ("CheckButton", "with_label"),
    ("ToggleButton", "with_label"),
    ("Property", "Label"),
    ("Property", "Description"),
    ("Kv", "new"),
    ("Section", "new"),
}

TECHNICAL_RE = re.compile(
    r'^('
    r'|[\W\d_]+'
    r'|[A-Z0-9][A-Z0-9._/+-]{0,7}'
    r'|(?:GPU|CPU|RAM|VRAM|PCIe|BIOS|VBIOS|CUDA|NVML|RGB|LED|CSV|JSON|HTML|TXT|UTF-8|GTK|Wayland|X11|XDG|DMI|SMBIOS|UDisks2|SMART|NVMe|SATA|HDD|SSD|USB|I2C|SMBus|HDMI|DisplayPort|RPM|MHz|GHz|kHz|Hz|MB|GB|TB|MiB|GiB|TiB|KB|KiB|MB/s|GB/s|GT/s|mV|V|W|°C|°F|%|dB|ms|fps|Hz|Ctrl\+\S+|Alt\+\S+|Shift\+\S+|F\d+|Esc|Enter|Tab|Space)'
    r'|T\d+'
    r'|[a-z0-9]+(?:-[a-z0-9]+)+'
    r'|[a-z0-9_]+/[a-z0-9_/-]+'
    r')$'
)


@dataclass
class Finding:
    rel: str
    line: int
    text: str
    reason: str


def is_other_branch(rel: str) -> bool:
    return any(rel.startswith(p) if p.endswith("/") else rel == p for p in OTHER_BRANCH_FILES)


def is_excluded(path: Path) -> bool:
    rel = path.relative_to(APP)
    return (
        rel.as_posix() in EXCLUDED_FILES
        or path.name.startswith("skjermbilder")
        or path.stem == "tests"
        or "tests" in rel.parts
    )


def rust_string(token: str) -> str:
    if token.startswith("r"):
        quote = token.index('"')
        return token[quote + 1 : -quote]
    body = token[1:-1]
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}

    def unescape(match: re.Match[str]) -> str:
        escape = match[1]
        if escape[0] == "u":
            return chr(int(escape[2:-1].replace("_", ""), 16))
        if escape[0] == "x":
            return chr(int(escape[1:], 16))
        if escape.startswith("\n"):
            return ""
        return escapes.get(escape, escape)

    return re.sub(r'\\(u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|\n\s*|.)', unescape, body)


def tokenize(text: str):
    end = 0
    while match := TOKEN.search(text, end):
        end = match.end()
        tok = match[0]
        if tok.startswith("//"):
            continue
        if tok == "/*":
            depth = 1
            while depth and (block := BLOCK.search(text, end)):
                depth += 1 if block[0] == "/*" else -1
                end = block.end()
            continue
        yield tok, match.start(), match.lastgroup in ("raw", "string")


def skip_delimited(parts, i: int) -> int:
    opening = {"(": ")", "[": "]", "{": "}"}
    if i >= len(parts) or parts[i][0] not in opening:
        return i
    stack = [opening[parts[i][0]]]
    i += 1
    while i < len(parts) and stack:
        tok = parts[i][0]
        if tok in opening:
            stack.append(opening[tok])
        elif tok == stack[-1]:
            stack.pop()
        i += 1
    return i


def is_cfg_test_attr(parts, i: int) -> bool:
    if i + 6 >= len(parts):
        return False
    if parts[i][0] != "#" or parts[i + 1][0] != "[":
        return False
    j = skip_delimited(parts, i + 1)
    inner = "".join(p[0] for p in parts[i + 2 : j - 1])
    return inner == "cfg(test)"


def skip_item_after_cfg_test(parts, i: int) -> int:
    while i < len(parts) and parts[i][0] == "#" and i + 1 < len(parts) and parts[i + 1][0] == "[":
        i = skip_delimited(parts, i + 1)
    while i < len(parts):
        tok = parts[i][0]
        if tok == "{":
            return skip_delimited(parts, i)
        if tok == ";":
            return i + 1
        i += 1
    return i


def enclosing_call(parts, idx: int) -> tuple[str, int, int]:
    depth = 0
    arg_index = 0
    for j in range(idx - 1, -1, -1):
        tok = parts[j][0]
        if tok in (")", "]", "}"):
            depth += 1
        elif tok in ("(", "[", "{"):
            if depth == 0:
                if tok != "(":
                    return ("", 0, j)
                if j >= 1:
                    macro = parts[j - 1][0] == "!"
                    name = parts[j - 2][0] + "!" if macro and j >= 2 else parts[j - 1][0]
                    if j >= 4 and parts[j - 2][0] == ":" and parts[j - 3][0] == ":":
                        return (f"{parts[j - 4][0]}::{name}", arg_index, j)
                    if j >= 2 and parts[j - 2][0] == ".":
                        return (f".{name}", arg_index, j)
                    return (name, arg_index, j)
                return ("", 0, j)
            depth -= 1
        elif tok == "," and depth == 0:
            arg_index += 1
    return ("", 0, -1)


def ui_call(parts, idx: int) -> str:
    call, arg, start = enclosing_call(parts, idx)
    while call in {"Some", "format!", "concat!"} or (not call and start >= 0 and parts[start][0] == "("):
        call, arg, start = enclosing_call(parts, start)
    bare = call.split("::")[-1].lstrip(".")
    if arg in UI_CALL_ARGS.get(bare, ()):
        return call
    if tuple(call.split("::")) in UI_METHOD_OR_CTOR and (call not in {"Kv::new", "Section::new"} or arg == 1):
        return call
    return ""


def is_inside_log_or_assert(parts, idx: int) -> bool:
    depth = 0
    for j in range(idx - 1, max(-1, idx - 80), -1):
        tok = parts[j][0]
        if tok in (")", "]", "}"):
            depth += 1
        elif tok in ("(", "[", "{"):
            if depth == 0:
                if j >= 2 and parts[j - 1][0] == "!":
                    macro = parts[j - 2][0]
                    if macro in (
                        "assert", "assert_eq", "assert_ne", "debug_assert",
                        "debug_assert_eq", "debug_assert_ne", "panic",
                        "unreachable", "todo", "unimplemented",
                        "eprintln", "println", "writeln", "write",
                        "eprint", "print", "format_args",
                        "json", "include_str", "include_bytes", "env", "option_env",
                    ):
                        return True
                return False
            depth -= 1
    return False


def scan_file(path: Path) -> list[Finding]:
    rel = path.relative_to(APP).as_posix()
    text = path.read_text(encoding="utf-8")
    parts = list(tokenize(text))
    findings: list[Finding] = []
    i = 0
    n = len(parts)
    while i < n:
        tok, pos, is_str = parts[i]
        if tok == "#" and is_cfg_test_attr(parts, i):
            i = skip_item_after_cfg_test(parts, i)
            continue
        if tok == "#" and i + 1 < n and parts[i + 1][0] == "[":
            i = skip_delimited(parts, i + 1)
            continue
        if tok == "#" and i + 2 < n and parts[i + 1][0] == "!" and parts[i + 2][0] == "[":
            i = skip_delimited(parts, i + 2)
            continue
        if tok in ("tr", "trf", "trn") and i + 2 < n and parts[i + 1][0] == "!":
            opening = {"(": ")", "[": "]", "{": "}"}
            if parts[i + 2][0] in opening:
                j = i + 3
                if tok == "trn":
                    depth = 0
                    while j < n:
                        t = parts[j][0]
                        if t in opening:
                            depth += 1
                        elif t in (")", "]", "}"):
                            if depth == 0:
                                break
                            depth -= 1
                        elif t == "," and depth == 0:
                            j += 1
                            break
                        j += 1
                while j < n and parts[j][2]:
                    j += 1
                    if j < n and parts[j][0] == ",":
                        j += 1
                    else:
                        break
                i = j
                continue
        if is_str:
            val = rust_string(tok)
            if not val.strip():
                i += 1
                continue
            call, arg_idx, _ = enclosing_call(parts, i)
            bare_call = call.lstrip(".")
            # `kw(nb_keywords, tr!(...))` intentionally keeps Norwegian keywords alongside translated ones.
            if (bare_call == "kw" or call.endswith("::kw")) and arg_idx == 0:
                i += 1
                continue
            line = text.count("\n", 0, pos) + 1
            reasons = []
            if NORWEGIAN_CHARS.search(val):
                reasons.append("æøå")
            elif NORWEGIAN_WORDS.search(val):
                reasons.append("nb-ord")

            ui = ui_call(parts, i)
            if ui and not TECHNICAL_RE.match(val.strip()):
                reasons.append(f"ui:{ui}")

            if reasons == ["nb-ord"]:
                if TECHNICAL_RE.match(val.strip()):
                    reasons = []
                elif is_inside_log_or_assert(parts, i):
                    reasons = []
                elif re.fullmatch(r'[a-z][a-z0-9_-]*', val):
                    reasons = []

            if reasons:
                findings.append(Finding(rel, line, val, ",".join(reasons)))
        i += 1
    return findings


PO_ENTRY = re.compile(r'^msgid ((?:"(?:[^"\\]|\\.)*"\n)+)msgstr ((?:"(?:[^"\\]|\\.)*"\n)+)', re.M)


def po_messages(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    join = lambda block: "".join(rust_string(line) for line in block.splitlines())
    return {join(m[1]): join(m[2]) for m in PO_ENTRY.finditer(path.read_text(encoding="utf-8") + "\n")}


def navigation_mismatches() -> list[str]:
    """Sidenavnene i Innstillinger (rigg_core::settings::page_name) skal være de samme som i sidefeltet."""
    source = (APP / "crates/rigg-core/src/settings.rs").read_text(encoding="utf-8")
    names = re.findall(r'=> tr!\("([^"]+)"\)', source[source.index("pub fn page_name") : source.index("pub const INTERVALS")])
    out = []
    for lang in sorted((APP / "data/i18n").iterdir()):
        nav = po_messages(lang / "agentic_rigg/pages.po")
        settings = po_messages(lang / "rigg_core/settings.po")
        for name in names:
            parts = [nav.get(part, "") for part in name.split(" › ")]
            have = settings.get(name, "")
            if have and all(parts) and have != " › ".join(parts):
                out.append(f"data/i18n/{lang.name}/rigg_core/settings.po [sidenavn] {name!r}: {have!r} != {' › '.join(parts)!r}")
    return out


# --- Katalogkontroll for alle språk -------------------------------------------------

# Tegn som ikke hører hjemme i språket. Bokmålets æ, ø og å i en oversettelse betyr
# nesten alltid at et norsk ord har blitt stående; svensk har å, men ikke æ og ø.
_OTHER_SCRIPTS = "\u0400-\u04FF\uAC00-\uD7A3"
_CJK = "\u3040-\u30FF\u4E00-\u9FFF"
FOREIGN_CHARS: dict[str, str] = {
    "en": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "de": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "fr": f"[øåØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "es": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "pt-BR": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "it": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "pl": f"[æøåÆØÅ{_OTHER_SCRIPTS}{_CJK}]",
    "sv": f"[æøÆØ{_OTHER_SCRIPTS}{_CJK}]",
    "zh-CN": f"[æøåÆØÅ{_OTHER_SCRIPTS}\u3040-\u30FF]",
    "ja": f"[æøåÆØÅ{_OTHER_SCRIPTS}]",
}
# Språk med eget skriftsystem: en oversatt setning skal inneholde det. De skriver uten
# mellomrom mellom ord og har egne skilletegn, så mellomrom i start og slutt kan falle bort.
OWN_SCRIPT: dict[str, str] = {"zh-CN": "[\u4E00-\u9FFF]", "ja": "[\u3040-\u30FF\u4E00-\u9FFF]"}
# «Ø» som frittstående tegn er diameter («Ø 6 mm») eller gjennomsnitt (tysk «Takt (Ø)»),
# ikke en norsk bokstav.
SYMBOL = re.compile(r"(?<![^\W\d_])Ø(?![^\W\d_])")
# Kilden skriver tall og enhet med smalt, hardt mellomrom (U+202F), slik at «25 ms» ikke
# deles over to linjer. Et vanlig mellomrom mellom tall og enhet er da en feil.
NNBSP = "\u202f"
PLAIN_UNIT_SPACE = re.compile(r"[\d}] (?:%|°C|°F|ms|s|min|h|MB|GB|kB|MHz|W)(?!\w)")
# Loggmappa beholder det gamle appnavnet, slik at eksisterende logger blir funnet.
OLD_NAME = "Agentic Rigg"
OLD_NAME_PATH = "~/Dokumenter/Agentic Rigg"
DESKTOP_KEYS = ("Name", "GenericName", "Comment", "Keywords")


def extractor():
    """Uttrekksverktøyet (i18n-trekk-ut.py) som modul: språkliste, PO-leser og nøkler."""
    spec = importlib.util.spec_from_file_location("i18n_trekk_ut", APP / "scripts/i18n-trekk-ut.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rust_languages(source: str) -> list[str]:
    """Kodene i `Language::ALL`, i nedtrekkslistas rekkefølge."""
    codes = dict(re.findall(r'Self::(\w+) => "([A-Za-z-]+)",', source[source.index("pub const fn code") : source.index("pub const fn native_name")]))
    order = re.search(r'pub const ALL: \[Self; (\d+)\] = \[(.*?)\];', source, re.S)
    variants = re.findall(r'Self::(\w+)', order[2])
    if int(order[1]) != len(variants) or set(variants) != set(codes):
        raise ValueError("Language::ALL og Language::code() har ulike språk")
    return [codes[variant] for variant in variants]


def whitespace_shape(text: str, spaces: bool = True) -> tuple[int, int, int]:
    edges = (len(text) - len(text.lstrip()), len(text) - len(text.rstrip())) if spaces else (0, 0)
    return *edges, text.count("\n")


def text_findings(lang: str, source: str, form: str, name: str, english: str = "") -> list[str]:
    """Avvik i én oversatt form. `source` er bokmålsformen den gjengir, og `english`
    den engelske, som viser om en fagterm bevisst står uoversatt («Sync Boost»)."""
    out = []
    own = OWN_SCRIPT.get(lang)
    if whitespace_shape(form, own is None) != whitespace_shape(source, own is None):
        out.append("mellomrom eller linjeskift i start/slutt avviker fra kilden")
    foreign = re.search(FOREIGN_CHARS[lang], SYMBOL.sub("", form))
    if foreign:
        out.append(f"tegnet {foreign[0]!r} hører ikke til {lang}")
    if NNBSP in source and PLAIN_UNIT_SPACE.search(form):
        out.append("vanlig mellomrom mellom tall og enhet, bruk U+202F som i kilden")
    sentence = re.search(r"[a-zæøå]{4,} [a-zæøå]{4,}", source.lower())
    if own and sentence and form.casefold() not in (source.casefold(), english.casefold()) and not re.search(own, form):
        out.append(f"setningen mangler skriftsystemet til {lang}")
    if form.count(name) != source.count(name):
        out.append(f"appnavnet «{name}» skal stå uendret")
    if form.replace(OLD_NAME_PATH, "").count(OLD_NAME) or form.count(OLD_NAME_PATH) != source.count(OLD_NAME_PATH):
        out.append(f"«{OLD_NAME}» skal bare stå i stien {OLD_NAME_PATH}")
    return out


def desktop_findings(text: str, langs, name: str) -> list[str]:
    """Skrivebordsfilen skal ha navn, beskrivelse og søkeord på hvert språk."""
    out = []
    section = None
    seen: dict[str, set[str]] = {}
    for line in text.splitlines():
        if line.startswith("["):
            section = line.strip("[]")
            continue
        match = re.fullmatch(r"(\w+)(?:\[([\w@]+)\])?=(.*)", line)
        if not match or section is None:
            continue
        key, locale, value = match.groups()
        if locale is not None:
            seen.setdefault(f"{section}/{key}", set()).add(locale)
            if not value.strip():
                out.append(f"{section}: {key}[{locale}] er tom")
        if section == "Desktop Entry" and key == "Name" and value != name:
            out.append(f"Name{'[' + locale + ']' if locale else ''} skal være «{name}», ikke «{value}»")
    wanted = {lang.replace("-", "_") for lang in langs}
    keys = [f"Desktop Entry/{key}" for key in DESKTOP_KEYS] + [key for key in seen if not key.startswith("Desktop Entry/")]
    for key in keys:
        have = seen.get(key, set())
        for locale in sorted(wanted - have):
            out.append(f"{key}[{locale}] mangler")
        for locale in sorted(have - wanted):
            out.append(f"{key}[{locale}] er et språk appen ikke har")
    return out


def catalog_findings() -> list[str]:
    """Kontrollerer hvert språk like strengt: språkliste, dekning, tegn og appnavn."""
    tool = extractor()
    langs = list(tool.LANGS)
    name = json.loads((APP / "version.json").read_text(encoding="utf-8"))["name"]
    out = []
    rust = rust_languages((APP / "crates/rigg-core/src/i18n/mod.rs").read_text(encoding="utf-8"))
    if rust != langs:
        out.append(f"Language::ALL {rust} er ikke lik LANGS i i18n-trekk-ut.py {langs}")
    folders = sorted(path.name for path in (APP / "data/i18n").iterdir() if path.is_dir())
    if folders != sorted(langs):
        out.append(f"katalogmappene {folders} er ikke lik språklista {sorted(langs)}")
    for lang in langs:
        if lang != "nb" and lang not in FOREIGN_CHARS:
            out.append(f"{lang} mangler tegnregel i FOREIGN_CHARS")
    out.extend(f"data/io.github.AgenticRigg.desktop.in: {line}" for line in desktop_findings(
        (APP / "data/io.github.AgenticRigg.desktop.in").read_text(encoding="utf-8"), langs, name))

    index = tool.sources()
    for module in sorted(tool.catalog_modules()):
        entries = tool.entries_from_source(index[module])
        english_path = tool.po_path("en", module)
        english = tool.read_po(english_path) if english_path.exists() else {}
        status = {}
        for lang in langs:
            path = tool.po_path(lang, module)
            rel = path.relative_to(APP).as_posix()
            if not path.exists():
                out.append(f"{rel} mangler")
                continue
            try:
                _, missing = tool.check(lang, module, entries)
            except tool.Invalid as error:
                out.append(str(error))
                continue
            records = tool.read_po(path)
            head = records.pop(("", ""), tool.Entry()).translations.get(0, "")
            status[lang] = "X-Rigg-Status: complete" in head.splitlines()
            if lang == "nb":
                continue
            for context, source in sorted(missing):
                out.append(f"{rel} [mangler] {context + ' · ' if context else ''}{source!r}")
            if lang not in FOREIGN_CHARS:
                continue
            for key, record in records.items():
                entry = entries.get(key)
                if entry is None or record.fuzzy:
                    continue
                for i, form in sorted(record.translations.items()):
                    if not form:
                        continue
                    source = entry.source if i == 0 or entry.plural is None else entry.plural
                    reference = english.get(key, tool.Entry()).translations
                    for why in text_findings(lang, source, form, name, reference.get(min(i, len(reference) - 1), "")):
                        out.append(f"{rel} [{why}] {source!r} → {form!r}")
        translated = {lang: done for lang, done in status.items() if lang != "nb"}
        if len(set(translated.values())) > 1:
            behind = sorted(lang for lang, done in translated.items() if not done)
            out.append(f"{module}: X-Rigg-Status: complete mangler for {', '.join(behind)}")
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--all", action="store_true", help="vis også oppføringer i unntakslista")
    parser.add_argument("--include-other", action="store_true", help="ta også med filer eid av andre grener")
    parser.add_argument("--check", action="store_true", help="eksplisitt kontroll (samme som standard)")
    parser.add_argument("files", nargs="*", type=Path, help="begrens til bestemte kildefiler")
    args = parser.parse_args()

    if args.files:
        paths = [p.resolve() for p in args.files]
    else:
        paths = [path for crate in CRATES for path in sorted((APP / "crates" / crate / "src").rglob("*.rs"))]

    unallowed = []
    allowed = []
    other = []
    for p in paths:
        if p.suffix != ".rs" or is_excluded(p):
            continue
        if is_other_branch(p.relative_to(APP).as_posix()) and not args.include_other:
            continue
        for f in scan_file(p):
            why = (
                ALLOWLIST_EXACT.get((f.rel, f.text))
                or ALLOWLIST_FILES.get(f.rel)
                or next((w for (rel, prefix), w in ALLOWLIST_PREFIX.items() if rel == f.rel and f.text.startswith(prefix)), None)
            )
            if why is not None:
                allowed.append((f, why))
            elif is_other_branch(f.rel):
                other.append(f)
            else:
                unallowed.append(f)

    mismatches = [] if args.files else navigation_mismatches()
    catalogs = [] if args.files else catalog_findings()
    for f in unallowed:
        print(f"{f.rel}:{f.line} [{f.reason}] {f.text!r}")
    for line in mismatches:
        print(line)
    for line in catalogs:
        print(line)
    for f in other:
        print(f"{f.rel}:{f.line} [{f.reason}] {f.text!r}  # rel/demo-rgb")
    if args.all and allowed:
        print(f"\n--- Unntaksliste ({len(allowed)}) ---")
        for f, why in allowed:
            print(f"{f.rel}:{f.line} [{f.reason}] {f.text!r}  # {why}")

    print(f"\nOppsummering: {len(unallowed)} uavklarte, {len(mismatches)} avvikende sidenavn, {len(catalogs)} katalogfunn, {len(allowed)} i unntakslista, {len(other)} på rel/demo-rgb.", file=sys.stderr)
    return int(bool(unallowed or mismatches or catalogs))


if __name__ == "__main__":
    sys.exit(main())
