#!/usr/bin/env python3
"""Finn sannsynlig synlige norske eller uoversatte tekststrenger utenfor tr!/trf!/trn!.

Bruk:
  python3 app/scripts/i18n-revisjon.py                  # vis uavklarte funn i eide filer
  python3 app/scripts/i18n-revisjon.py --check          # feil utgangskode ved uavklarte funn
  python3 app/scripts/i18n-revisjon.py --all            # vis også dokumenterte unntak
  python3 app/scripts/i18n-revisjon.py --include-other  # ta også med filer eid av andre grener
  python3 app/scripts/i18n-revisjon.py <fil...>         # skann bestemte kildefiler
"""

import argparse
from dataclasses import dataclass
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
    "crates/rigg/src/skjermbilder.rs",
    "crates/rigg/src/skjermbilder_oppgaver.rs",
    "crates/rigg/src/skjermbilder_ui140.rs",
}

# Filer eid av annen gren under 1.5.0-revisjonen (rapporteres separat med --include-other).
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
    "crates/riggd/src/tray.rs": "D-Bus-feilmeldinger for ukjent egenskap/metode",
    "crates/riggd/src/varsler.rs": "Stabile interne varselnøkler (disk:...)",
}

# Exact (file, literal) allowlist for UI and shared modules so new untranslated UI text is always caught.
ALLOWLIST_EXACT: dict[tuple[str, str], str] = {
    ("crates/rigg-core/src/i18n/mod.rs", "Norsk bokmål"): "Egennavn på kildespråket",
    ("crates/rigg-core/src/lib.rs", "{NAME} {VERSION} · bygg {BUILD_STR}"): "CLI --version-utdata (UI bruker trf!)",
    ("crates/rigg-core/src/lib.rs", "Versjon {VERSION} · bygg {BUILD_STR}"): "Referansemetode for CLI/tester (UI bruker trf!)",
    ("crates/rigg-core/src/report/gpu.rs", "Ukjent"): "Rå bokmålsverdi fra leverandørstreng matchet og oversatt med tr!",
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
    ("crates/rigg-core/src/specs.rs", "Minneleverandøren (Samsung eller Micron) står i VBIOS-en. NVML oppgir den ikke, og VBIOS kan ikke leses uten root."): "MEM_NOTE-konstant oversatt i mem_vendor_note_text() med tr!",
    ("crates/rigg-core/src/stats.rs", ".ødelagt-{now}"): "Filnavnsuffiks for ødelagt sikkerhetskopi",
    ("crates/rigg/src/app.rs", "--innstillinger"): "CLI-flagg",
    ("crates/rigg/src/app.rs", "ingen skjerm"): "Intern expect-melding ved manglende skjerm",
    ("crates/rigg/src/app.rs", "RIGG_RAMMER {} bilder på 10 s, {:.1} ms i alt, snitt {:.2} ms, verste {:.2} ms"): "Feilsøkingsutdata på stderr bak RIGG_RAMMER",
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
    ("crates/rigg/src/pages/viftekurve.rs", ", {n} målinger"): "Eksakt bokmålsform i active()==Nb-gren; øvrige språk bruker trn!",
    ("crates/rigg/src/prefs.rs", ".ødelagt-{stamp}"): "Filnavnsuffiks for ødelagt sikkerhetskopi",
    ("crates/rigg/src/rapport_ui.rs", "agentic-rigg-rapport-{:04}-{:02}-{:02}_{:02}-{:02}.{}"): "Standard eksportfilnavn",
    ("crates/rigg/src/shortcuts.rs", " (uten ring)"): "Feilsøkingsutdata i fokuslogg",
    ("crates/rigg/src/sidebar.rs", "Ukjent prosessor"): "Rå reservenavn fra rigg-hw matchet og oversatt med tr!",
    ("crates/rigg/src/sidebar.rs", "sensorer?vis={id}"): "Intern navigasjonsrute",
    ("crates/rigg/src/sidebar.rs", "disk/"): "Stabil sensornøkkelprefiks",
    ("crates/rigg/src/state.rs", "skallet er ikke bygd"): "Intern expect-melding",
    ("crates/rigg/src/widgets/chart.rs", "bilde med oppsett"): "Intern expect-melding",
    ("crates/rigg/src/widgets/chart.rs", "GPU-temperatur"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "GPU-effekt"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "GPU-vifte"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "CPU-temperatur"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "Minne brukt"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "Disk-I/O"): "Referansetabell DEFS; ChartDef::title() bruker tr!",
    ("crates/rigg/src/widgets/chart.rs", "disk/{}/temp"): "Stabil sensornøkkel",
    ("crates/rigg/src/widgets/chart/math.rs", "Sekunder"): "CSV-kolonneoverskrift jf. docs/produksjon/i18n.md",
    ("crates/rigg/src/widgets/version_log.rs", "Versjonsloggen må være en liste"): "Intern expect-melding ved tolking av innebygd versjonslogg",
    ("crates/rigg/src/window.rs", "appen lever så lenge vinduet gjør"): "Intern expect-melding",
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
    r'spenning|hastighet|bruk|brukt|ledig|tilgjengelig|aktiv|inaktiv|kjør|kjører'
    r')\b',
    re.IGNORECASE,
)

UI_CALL_ARGS: dict[str, set[int]] = {
    "label": {0},
    "set_text": {0, 1},
    "set_title": {0},
    "set_label": {0},
    "set_tooltip_text": {0},
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
    context: str


def is_other_branch(rel: str) -> bool:
    return any(rel == p or rel.startswith(p) for p in OTHER_BRANCH_FILES)


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


def enclosing_call(parts, idx: int) -> tuple[str, int]:
    depth = 0
    arg_index = 0
    for j in range(idx - 1, -1, -1):
        tok = parts[j][0]
        if tok in (")", "]", "}"):
            depth += 1
        elif tok in ("(", "[", "{"):
            if depth == 0:
                if tok != "(":
                    return ("", 0)
                if j >= 1:
                    name = parts[j - 1][0]
                    if j >= 3 and parts[j - 2][0] == ":" and parts[j - 3][0] == ":":
                        return (f"{parts[j - 4][0]}::{name}" if j >= 4 else name, arg_index)
                    if j >= 2 and parts[j - 2][0] == ".":
                        return (f".{name}", arg_index)
                    return (name, arg_index)
                return ("", 0)
            depth -= 1
        elif tok == "," and depth == 0:
            arg_index += 1
    return ("", 0)


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
    lines_src = text.splitlines()
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
            call, arg_idx = enclosing_call(parts, i)
            bare_call = call.lstrip(".")
            # `kw(nb_keywords, tr!(...))` intentionally keeps Norwegian keywords alongside translated ones.
            if (bare_call == "kw" or call.endswith("::kw")) and arg_idx == 0:
                i += 1
                continue
            line = text.count("\n", 0, pos) + 1
            src_line = lines_src[line - 1].strip() if 0 <= line - 1 < len(lines_src) else ""
            reasons = []
            if NORWEGIAN_CHARS.search(val):
                reasons.append("æøå")
            elif NORWEGIAN_WORDS.search(val):
                reasons.append("nb-ord")

            is_ui_call = False
            if bare_call in UI_CALL_ARGS:
                if arg_idx in UI_CALL_ARGS[bare_call]:
                    is_ui_call = True
            elif "::" in call:
                parts_c = call.split("::")
                if (parts_c[0], parts_c[1]) in UI_METHOD_OR_CTOR:
                    if (parts_c[0], parts_c[1]) in (("Kv", "new"), ("Section", "new")) and arg_idx != 1:
                        is_ui_call = False
                    else:
                        is_ui_call = True

            if is_ui_call and not TECHNICAL_RE.match(val.strip()):
                reasons.append(f"ui:{call}")

            if reasons == ["nb-ord"]:
                if TECHNICAL_RE.match(val.strip()):
                    reasons = []
                elif is_inside_log_or_assert(parts, i):
                    reasons = []
                elif re.fullmatch(r'[a-z][a-z0-9_-]*', val):
                    reasons = []

            if reasons:
                findings.append(Finding(rel, line, val, ",".join(reasons), src_line))
        i += 1
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--all", action="store_true", help="vis også oppføringer i unntakslista")
    parser.add_argument("--include-other", action="store_true", help="ta også med filer eid av andre grener")
    parser.add_argument("--check", action="store_true", help="returner feilkode hvis uavklarte funn gjenstår")
    parser.add_argument("files", nargs="*", type=Path, help="begrens til bestemte kildefiler")
    args = parser.parse_args()

    if args.files:
        paths = [p.resolve() for p in args.files]
    else:
        paths = []
        for crate in CRATES:
            for p in sorted((APP / "crates" / crate / "src").rglob("*.rs")):
                rel = p.relative_to(APP).as_posix()
                if rel in EXCLUDED_FILES:
                    continue
                if not args.include_other and is_other_branch(rel):
                    continue
                paths.append(p)

    unallowed = []
    allowed = []
    for p in paths:
        for f in scan_file(p):
            why = ALLOWLIST_EXACT.get((f.rel, f.text)) or ALLOWLIST_FILES.get(f.rel)
            if why is not None:
                allowed.append((f, why))
            else:
                unallowed.append(f)

    for f in unallowed:
        print(f"{f.rel}:{f.line} [{f.reason}] {f.text!r}")
    if args.all and allowed:
        print(f"\n--- Unntaksliste ({len(allowed)}) ---")
        for f, why in allowed:
            print(f"{f.rel}:{f.line} [{f.reason}] {f.text!r}  # {why}")

    print(f"\nOppsummering: {len(unallowed)} uavklarte, {len(allowed)} i unntakslista.", file=sys.stderr)
    if args.check and unallowed:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
