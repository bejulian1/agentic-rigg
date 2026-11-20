#!/usr/bin/env python3
"""Pakk ferdigbygde binærer. Bygger, installerer og publiserer ingenting.

Kjør fra hvilken som helst mappe:
  python3 app/scripts/pakk-utgivelse.py --bin-dir STI --out-dir STI
  Legg eventuelt til --commit FULL_GIT_ID for SOURCE_COMMIT.txt.

Bare eksplisitt valgte filer og hele app/data blir med. Utdata er ett
Ubuntu 26.04 amd64-arkiv og SHA256SUMS. Eksisterende utdata overskrives ikke.
"""

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tarfile
import tempfile


APP = Path(__file__).resolve().parents[1]
BINARIES = ("agentic-rigg", "agentic-riggd", "agentic-rigg-helper")


def fail(message):
    raise ValueError(message)


def version_metadata():
    data = json.loads((APP / "version.json").read_text(encoding="utf-8"))
    if data.get("name") != "Agentic Rigg":
        fail("Uventet appnavn i app/version.json.")
    if not isinstance(data.get("version"), str) or not re.fullmatch(
        r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", data["version"]
    ):
        fail("Ugyldig versjon i app/version.json.")
    if type(data.get("build")) is not int or data["build"] < 1:
        fail("Byggnummeret i app/version.json må være et positivt heltall.")
    return data


def check_binary(path, name, metadata):
    if not path.is_file() or not os.access(path, os.X_OK):
        fail(f"Fant ikke kjørbar binær: {path}")
    with path.open("rb") as source:
        header = source.read(64)
    # ELF64, little endian, current ELF version, x86-64, executable or PIE.
    if len(header) != 64 or header[:7] != b"\x7fELF\x02\x01\x01":
        fail(f"Ikke en 64-bits little-endian ELF-binær: {path}")
    kind, machine = struct.unpack_from("<HH", header, 16)
    if machine != 62 or kind not in (2, 3):
        fail(f"Binæren er ikke en x86-64 ELF-kjørbar fil: {path}")
    title = "Agentic Rigg-hjelperen" if name == "agentic-rigg-helper" else metadata["name"]
    expected = f"{title} {metadata['version']} · bygg {metadata['build']}"
    result = subprocess.run(
        [str(path), "--version"], capture_output=True, text=True,
        encoding="utf-8", errors="strict", timeout=10, check=True,
    )
    if result.stdout.strip() != expected:
        fail(f"Feil versjon i {name}: forventet {expected!r}, fikk {result.stdout.strip()!r}.")


def check_data_name(relative):
    # Hele data-treet er tillatt, men avbryt om noen har lagt inn private
    # filer, tester eller arbeidsutdata der. Ikke utelat dem i stillhet.
    forbidden_dirs = {"test", "tests", "fixtures", "logs", "logg", "logger", "secrets", "__pycache__", "node_modules", "target"}
    forbidden_suffixes = {".log", ".pem", ".key", ".p12", ".pfx", ".pyc", ".sqlite", ".sqlite3"}
    if any(part.startswith(".") or part.lower() in forbidden_dirs for part in relative.parts):
        fail(f"Uventet privat fil eller arbeidsmappe i app/data: {relative}")
    if relative.suffix.lower() in forbidden_suffixes or relative.name.lower() in {"credentials", "credentials.json", "token.json", "tokens.json"}:
        fail(f"Uventet privat fil eller arbeidsutdata i app/data: {relative}")


def file_list(bin_dir):
    # (arkivsti, kildefil, kjørbar). Aldri rekursjon fra repo-rota eller bin-dir.
    files = [(f"bin/{name}", bin_dir / name, True) for name in BINARIES]
    files.extend([
        ("install.sh", APP / "install.sh", True),
        ("uninstall.sh", APP / "uninstall.sh", True),
        ("README-UTVIKLING.md", APP / "README.md", False),
        ("PATCHLOG.md", APP / "PATCHLOG.md", False),
        ("version.json", APP / "version.json", False),
        ("LISENSER.md", APP / "data/LISENSER.md", False),
        ("RGB-HJELPER.md", APP.parent / "docs/produksjon/rgb-hjelper.md", False),
    ])
    data_dir = APP / "data"
    if not data_dir.is_dir() or data_dir.is_symlink():
        fail("app/data må være en vanlig mappe.")
    for path in sorted(data_dir.rglob("*")):
        relative = path.relative_to(data_dir)
        check_data_name(relative)
        if path.is_symlink():
            fail(f"Symbolske lenker pakkes ikke: app/data/{relative}")
        if path.is_dir():
            continue
        if not path.is_file():
            fail(f"Uventet filtype: {path}")
        files.append((f"data/{relative.as_posix()}", path, path.suffix == ".sh" or path.name == "system-install"))
    for _, path, _ in files:
        if not path.is_file():
            fail(f"En påkrevd pakkefil mangler: {path}")
    return files


def readme(metadata):
    return f"""# Agentic Rigg {metadata['version']} · bygg {metadata['build']}

Ferdigbygd desktop-app for Ubuntu 26.04 på amd64 (x86-64).
Krever GTK 4.22 eller nyere og glibc 2.39 eller nyere. Dette er ikke en
selvstendig pakke med systembiblioteker; andre distribusjoner og eldre
Ubuntu-utgaver er ikke verifisert. Rust og Node.js trengs ikke for å kjøre.
NVIDIA-overvåking krever NVIDIA-driverens libnvidia-ml.so.1.
SMART-data krever UDisks2. CPU, minne og øvrige tilgjengelige systemdata
kan vises uten NVIDIA-driver.

## Installere for brukeren din

Pakk ut arkivet og åpne en terminal i denne mappa. Kjør som vanlig bruker:

```sh
./install.sh --bin-dir "$PWD/bin"
```

Installereren trenger bash, standard GNU-verktøy og python3. Den legger
appen under ~/.local, med ikon i programmenyen. Den bygger ingenting.
En allerede kjørende Agentic Rigg-brukertjeneste kan startes på nytt.
Root-komponentene følger med. Installer dem separat fra det verifiserte arkivet
med kommandoen i RGB-HJELPER.md. Brukerinstallasjonen aktiverer ingen hjelpere.
Ekte GPU-styring aktiveres separat i Innstillinger › Skjermkortstyring,
med én administratorgodkjenning. Støttede endringer i effekt, vifter og
klokkeoffset sendes med Bruk og må beholdes innen fristen. Hjelperen bindes
til kortet og lagrer utgangspunktet før skriving. Kortlys krever at en
administrator setter opp de betrodde RGB-komponentene som beskrevet i
RGB-HJELPER.md, og deretter en egen aktivering for et støttet kort.
Fast blått og tilbakeføring til grønt er prøvd på Palit GeForce RTX 2080 Super JetStream.
Se PATCHLOG.md for prøveresultat og omfang.

Start «Agentic Rigg» fra programmenyen, eller ~/.local/bin/agentic-rigg.
Versjon og bygg står i Innstillinger › Om og i binærenes --version.

## Avinstallere

Kjør ./uninstall.sh som vanlig bruker fra denne mappa. Innstillinger,
historikk og egne logger beholdes. En eventuell aktivert root-hjelper
slås av med bryteren i appen først.

LISENSER.md inneholder lisensene. PATCHLOG.md beskriver endringene.
README-UTVIKLING.md er den utfyllende prosjektveiledningen og inneholder
også byggeinstruksjoner; de er ikke nødvendige for dette ferdigbygde arkivet.
""".encode("utf-8")


def entry(name, size, executable=False):
    info = tarfile.TarInfo(name)
    info.size = size
    info.mode = 0o755 if executable else 0o644
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    info.mtime = 0
    return info


def package(args):
    metadata = version_metadata()
    bin_dir = args.bin_dir.resolve(strict=True)
    if not bin_dir.is_dir():
        fail("--bin-dir må være en mappe.")
    if args.commit is not None and not re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", args.commit):
        fail("--commit må være en full Git-ID (40 eller 64 heksadesimale tegn).")
    files = file_list(bin_dir)
    for name in BINARIES:
        check_binary(bin_dir / name, name, metadata)
    root = f"agentic-rigg-{metadata['version']}"
    filename = f"{root}-build{metadata['build']}-ubuntu26.04-amd64.tar.gz"
    args.out_dir.mkdir(parents=True, exist_ok=True)
    output = args.out_dir.resolve()
    archive = output / filename
    checksums = output / "SHA256SUMS"
    if archive.exists() or checksums.exists():
        fail("Arkivet eller SHA256SUMS finnes allerede. Velg en tom utmappe.")
    # Temp-mappa ligger på samme filsystem som sluttfilene. Hardlenking
    # publiserer ferdige filer uten å kunne overskrive eksisterende filer.
    with tempfile.TemporaryDirectory(prefix=".rigg-pakking-", dir=output) as temp:
        temp_archive = Path(temp) / filename
        with temp_archive.open("wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as zipped:
                with tarfile.open(fileobj=zipped, mode="w", format=tarfile.PAX_FORMAT) as tar:
                    texts = {"README.md": readme(metadata)}
                    if args.commit:
                        texts["SOURCE_COMMIT.txt"] = (args.commit.lower() + "\n").encode("ascii")
                    for name, content in sorted(texts.items()):
                        tar.addfile(entry(f"{root}/{name}", len(content)), io.BytesIO(content))
                    for name, path, executable in files:
                        with path.open("rb") as source:
                            size = os.fstat(source.fileno()).st_size
                            tar.addfile(entry(f"{root}/{name}", size, executable), source)
        with temp_archive.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        temp_checksums = Path(temp) / "SHA256SUMS"
        temp_checksums.write_text(f"{digest}  {filename}\n", encoding="ascii")
        os.link(temp_archive, archive)
        try:
            os.link(temp_checksums, checksums)
        except BaseException:
            archive.unlink()
            raise
    print(f"Laget {archive}")
    print(f"Laget {checksums}")
    print(f"{len(files) + len(texts)} filer · {metadata['version']} · bygg {metadata['build']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin-dir", type=Path, required=True, help="Mappe med de tre ferdigbygde binærene")
    parser.add_argument("--out-dir", type=Path, required=True, help="Mappe for ett arkiv og SHA256SUMS")
    parser.add_argument("--commit", help="Full Git-ID for kilden (valgfri; verifiseres ikke mot arbeidskopien)")
    args = parser.parse_args()
    try:
        package(args)
    except (ValueError, OSError, UnicodeError, subprocess.SubprocessError, tarfile.TarError) as error:
        print(f"Kunne ikke pakke utgivelsen: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
