#!/usr/bin/python3
"""Kortbundet RGB-installasjon. Kjøres bare fra en eksplisitt adminhandling.

--root under en privat /tmp-mappe er en ren filfixture: den kjører aldri
hjelperen eller systemctl. Produksjon godtar aldri et innsendt testresultat.
"""
import argparse
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import pwd
import re
import stat
import subprocess
import sys
import tempfile
import time

UNIT = "agentic-rigg-rgb.service"
BIN = Path("usr/local/libexec/agentic-rigg-rgb-helper")
SCRIPTS = Path("usr/local/libexec/agentic-rigg-rgb")
CONFIG = Path("etc/agentic-rigg/rgb.json")
STATE = Path("var/lib/agentic-rigg-rgb")
POLICY = Path("usr/share/polkit-1/actions/io.github.AgenticRigg.rgb.policy")
UNIT_PATH = Path("etc/systemd/system") / UNIT
DROPIN = Path("etc/systemd/system") / (UNIT + ".d/card.conf")
SOCKET = Path("run/agentic-rigg-rgb/helper.sock")
ASSETS = ("rgb-admin.py", "rgb-aktiver.sh", "rgb-deaktiver.sh",
          "rgb-avklar.sh", "rgb-gjenopprett.sh")
MAX_JSON = 1024 * 1024
MAX_BINARY = 128 * 1024 * 1024


def fail(message):
    raise RuntimeError(message)


def strict_json(data):
    def unique(pairs):
        value = {}
        for key, entry in pairs:
            if key in value:
                fail("Duplisert JSON-felt i RGB-oppsettet: " + key)
            value[key] = entry
        return value
    def invalid_constant(value):
        fail("Ugyldig JSON-tall i RGB-oppsettet: " + value)
    return json.loads(data, object_pairs_hook=unique, parse_constant=invalid_constant)


def bounded_json(path, limit=MAX_JSON):
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        fail("RGB-dokumentet må være en avgrenset, regulær fil uten symlink.")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as file:
        opened = os.fstat(file.fileno())
        if (not stat.S_ISREG(opened.st_mode)
                or (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino)):
            fail("RGB-dokumentet ble byttet før lesing.")
        data = file.read(limit + 1)
    if len(data) > limit:
        fail("RGB-dokumentet er for stort.")
    return strict_json(data.decode("utf-8"))


def fixture_root(raw):
    if not raw.is_absolute() or ".." in raw.parts:
        fail("Testrot må være en eksplisitt absolutt sti uten ..")
    resolved = raw.resolve(strict=True)
    if raw != resolved or not str(resolved).startswith("/tmp/"):
        fail("Testrot må være en symlinkfri privat mappe under /tmp.")
    # Også foreldrene må være beskyttet. En root-eid sluttmappe under en
    # brukers skrivbare forelder kan ellers byttes ut med en lenke til verten.
    current = Path("/tmp")
    metadata = current.lstat()
    if (not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 0
            or (metadata.st_mode & 0o022 and not metadata.st_mode & stat.S_ISVTX)):
        fail("/tmp er ikke en trygg forelder for RGB-testen.")
    for part in resolved.relative_to(current).parts:
        current /= part
        metadata = current.lstat()
        if (not stat.S_ISDIR(metadata.st_mode)
                or metadata.st_uid not in (0, os.geteuid()) or metadata.st_mode & 0o022):
            fail("Testrotens foreldre må være betrodde mapper uten andres skrivetilgang.")
    if metadata.st_uid != os.geteuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
        fail("Testroten må eies av testprosessen med modus 0700.")
    return resolved


def parse():
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("action", choices=("activate", "deactivate", "resolve", "restore"))
    parser.add_argument("--pci", required=True)
    parser.add_argument("--uuid", required=True)
    parser.add_argument("--static-confirmed", action="store_true")
    parser.add_argument("--restore-original", action="store_true")
    parser.add_argument("--root", default="/")
    parser.add_argument("--uid", type=int)
    parser.add_argument("--kilde", type=Path, default=Path(__file__).absolute().parent)
    parser.add_argument("--report", type=Path)
    # Argparse tillater ellers gjentatte flagg. Her skal ingen sisteverdi
    # overstyre et tidligere kortvalg eller en annen test-/produksjonsrot.
    for flag in ("--pci", "--uuid", "--root", "--uid", "--kilde", "--report",
                 "--static-confirmed", "--restore-original"):
        if sum(arg == flag or arg.startswith(flag + "=") for arg in sys.argv[1:]) > 1:
            fail("Et RGB-argument er gitt flere ganger: " + flag)
    args = parser.parse_args()
    args.pci = args.pci.lower()
    if not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{2}:[01][0-9a-f]\.0", args.pci):
        fail("Ugyldig grafikkfunksjon. Full PCI-adresse på funksjon 0 kreves.")
    if not re.fullmatch(r"GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", args.uuid):
        fail("Full GPU-UUID kreves.")
    if args.action in ("activate", "resolve"):
        if not args.static_confirmed or args.restore_original:
            fail("Denne handlingen krever uttrykkelig bekreftet fast lys.")
    elif args.action == "restore":
        if not args.restore_original or args.static_confirmed:
            fail("Gjenoppretting må velges uttrykkelig.")
    elif args.static_confirmed or args.restore_original:
        fail("Deaktivering tar ingen fargebekreftelse.")
    return args


class Admin:
    def __init__(self, args):
        self.args = args
        raw_root = Path(args.root)
        # Bare den bokstavelige produksjonsroten velger ekte handlinger.
        # Et testalias som peker til / får aldri bytte over til produksjon.
        self.fake = args.root != "/"
        if self.fake:
            self.root = fixture_root(raw_root)
            self.uid = args.uid
            if args.action != "deactivate" and args.report is None:
                fail("Filfixture krever et eksplisitt --report; ingen hjelper kjøres.")
        else:
            self.root = Path("/")
            if args.report is not None or args.uid is not None:
                fail("Testresultat og test-UID er ikke tillatt i produksjon.")
            if os.geteuid() != 0:
                fail("RGB-handlingen må godkjennes gjennom pkexec.")
            try:
                self.uid = int(os.environ["PKEXEC_UID"])
                pwd.getpwuid(self.uid)
            except (KeyError, ValueError):
                fail("Fant ikke brukeren bak administratorforespørselen.")
        if not isinstance(self.uid, int) or not 0 < self.uid < 2**32 - 1:
            fail("Root eller ukjent UID kan ikke være RGB-klient.")

    def path(self, relative):
        return self.root / relative

    def directory(self, relative, mode=0o755, create=True):
        current = self.root
        parts = Path(relative).parts
        for index, part in enumerate(parts):
            current /= part
            try:
                m = current.lstat()
            except FileNotFoundError:
                if not create:
                    raise
                current.mkdir(mode=mode if index == len(parts) - 1 else 0o755)
                m = current.lstat()
            if (not stat.S_ISDIR(m.st_mode) or stat.S_ISLNK(m.st_mode)
                    or m.st_uid != os.geteuid() or m.st_mode & 0o022):
                fail("Utrygg RGB-mappe: " + str(current))
        return current

    def check_file(self, relative, missing=False):
        path = self.path(relative)
        try:
            self.directory(Path(relative).parent, create=False)
        except FileNotFoundError:
            if missing:
                return None
            raise
        try:
            m = path.lstat()
        except FileNotFoundError:
            if missing:
                return None
            raise
        if (not stat.S_ISREG(m.st_mode) or m.st_uid != os.geteuid()
                or m.st_mode & 0o022 or m.st_nlink != 1):
            fail("Utrygg RGB-fil: " + str(path))
        return path

    def atomic(self, relative, data, mode):
        parent = self.directory(Path(relative).parent)
        destination = self.path(relative)
        if destination.exists() or destination.is_symlink():
            self.check_file(relative)
        fd, temporary = tempfile.mkstemp(prefix=".rgb-new-", dir=parent)
        try:
            with os.fdopen(fd, "wb") as file:
                os.fchmod(file.fileno(), mode)
                file.write(data)
                file.flush()
                os.fsync(file.fileno())
            os.replace(temporary, destination)
            directory_fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
        finally:
            Path(temporary).unlink(missing_ok=True)

    def source(self, name):
        path = self.args.kilde / name
        m = path.lstat()
        if (not stat.S_ISREG(m.st_mode) or m.st_uid not in (self.uid, os.geteuid())
                or m.st_mode & 0o022 or m.st_nlink != 1):
            fail("Hjelpepakken inneholder en utrygg fil: " + name)
        limit = MAX_BINARY if name == "agentic-rigg-helper" else MAX_JSON
        if m.st_size > limit:
            fail("Hjelpefilen er for stor: " + name)
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
        with os.fdopen(fd, "rb") as file:
            before = os.fstat(file.fileno())
            identity = lambda value: (value.st_dev, value.st_ino, value.st_uid,
                                      value.st_mode, value.st_nlink, value.st_size,
                                      value.st_mtime_ns, value.st_ctime_ns)
            if not stat.S_ISREG(before.st_mode) or identity(before) != identity(m):
                fail("Hjelpefilen ble byttet før lesing: " + name)
            data = file.read(limit + 1)
            if len(data) > limit or identity(os.fstat(file.fileno())) != identity(before):
                fail("Hjelpefilen ble endret under lesing: " + name)
            return data

    def command(self, argv, timeout=30):
        result = subprocess.run(argv, check=False, capture_output=True,
                                text=True, timeout=timeout)
        if result.returncode:
            fail((result.stderr or result.stdout or "RGB-kommandoen feilet.").strip())
        return result.stdout

    def active(self):
        return not self.fake and subprocess.run(
            ["/usr/bin/systemctl", "is-active", "--quiet", UNIT],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False).returncode == 0

    def helper(self, executable, flag, confirmed=None):
        if self.fake:
            # Aldri kjør et faktisk hjelpeprogram fra en filfixture,
            # heller ikke dersom testprosessen tilfeldigvis er root.
            payload = bounded_json(self.args.report)
        else:
            argv = [str(executable), flag, "--pci", self.args.pci,
                    "--uuid", self.args.uuid, "--uid", str(self.uid)]
            if confirmed:
                argv.append(confirmed)
            payload = strict_json(self.command(argv))
        if type(payload) is not dict:
            fail("RGB-hjelperens svar må være et JSON-objekt.")
        if payload.get("ok") is not True:
            fail(str(payload.get("error", "RGB-hjelperen avviste handlingen.")))
        if type(payload.get("report")) is not dict:
            fail("RGB-hjelperens rapport mangler eller har feil type.")
        return payload["report"]

    def bus(self, report):
        metadata = report["metadata"]
        if type(metadata) is not dict or type(metadata.get("adapter")) is not dict:
            fail("RGB-adapterrapporten har feil format.")
        bus = metadata["adapter"]["busNumber"]
        if (type(bus) is not int or not 0 <= bus < 2**20
                or metadata["pci"] != self.args.pci or metadata["uuid"] != self.args.uuid
                or metadata["devicePath"] != "/dev/i2c-" + str(bus)
                or metadata["deviceNumber"] != {"major": 89, "minor": bus}):
            fail("Hjelperens kortbundne adapterrapport er ugyldig.")
        return bus

    def no_journal(self):
        path = self.path(STATE / "journal.json")
        if path.exists() or path.is_symlink():
            fail("Uavklarte RGB-endringer må avklares før denne handlingen.")

    @contextmanager
    def worker_lock(self):
        state = self.directory(STATE, create=False)
        if stat.S_IMODE(state.lstat().st_mode) != 0o700:
            fail("RGB-tilstandsmappen er ikke privat.")
        self.check_file(STATE / "worker.lock", missing=True)
        fd = os.open(state / "worker.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW
                     | os.O_NONBLOCK | os.O_CLOEXEC, 0o600)
        try:
            m = os.fstat(fd)
            if (not stat.S_ISREG(m.st_mode) or m.st_uid != os.geteuid()
                    or stat.S_IMODE(m.st_mode) != 0o600 or m.st_nlink != 1):
                fail("RGB-låsefilen har utrygg type, eier eller modus.")
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                fail("En RGB-arbeider er fortsatt aktiv; oppsettet beholdes.")
            yield
        finally:
            os.close(fd)

    def selected_authorization(self):
        path = self.check_file(CONFIG)
        if stat.S_IMODE(path.lstat().st_mode) != 0o600:
            fail("RGB-tillatelsen er ikke privat.")
        auth = bounded_json(path, 16_384)
        if type(auth) is not dict or type(auth.get("binding")) is not dict:
            fail("RGB-tillatelsen har feil format.")
        if (type(auth["uid"]) is not int or auth["uid"] != self.uid or auth["binding"]["pci"] != self.args.pci
                or auth["binding"]["uuid"] != self.args.uuid):
            fail("RGB-tillatelsen gjelder en annen bruker eller et annet kort.")
        return auth

    def install_assets(self, binary, assets):
        self.atomic(BIN, binary, 0o755)
        for name in ASSETS:
            self.atomic(SCRIPTS / name, assets[name], 0o755)
        self.atomic(UNIT_PATH, assets[UNIT], 0o644)
        self.atomic(POLICY, assets["io.github.AgenticRigg.rgb.policy"], 0o644)

    def start(self, bus):
        content = ("[Service]\nDeviceAllow=\nDeviceAllow=/dev/i2c-" + str(bus) + " rw\n")
        self.atomic(DROPIN, content.encode(), 0o644)
        if self.fake:
            return
        self.command(["/usr/bin/systemctl", "daemon-reload"])
        # Ingen enable: RGB skal ikke få automatisk oppstart ved innlogging/boot.
        self.command(["/usr/bin/systemctl", "start", UNIT])
        for _ in range(40):
            try:
                m = self.path(SOCKET).lstat()
                if stat.S_ISSOCK(m.st_mode) and m.st_uid == self.uid:
                    if self.active():
                        return
            except FileNotFoundError:
                pass
            time.sleep(0.05)
        fail("RGB-tjenesten startet ikke med riktig socket. Se RGB-tjenestens journal.")

    def activate(self):
        if self.active():
            fail("RGB-hjelperen kjører allerede. Bruk appens lyskontroller.")
        self.no_journal()
        binary = self.source("agentic-rigg-helper")
        # Hele pakken fryses før identifikasjon/lagring. En endring av
        # brukerens kildefiler etterpå kan ikke gi en blandet root-pakke.
        assets = {name: self.source(name) for name in
                  (*ASSETS, UNIT, "io.github.AgenticRigg.rgb.policy")}
        parent = self.directory(BIN.parent)
        # Root-eid iscenesetting før kjøring; aldri kjør brukerens endrbare
        # binærsti etter at administratorgodkjenningen er gitt.
        with tempfile.TemporaryDirectory(prefix=".rigg-rgb-", dir=parent) as staging:
            executable = Path(staging) / "helper"
            executable.write_bytes(binary)
            executable.chmod(0o755)
            report = self.helper(executable, "--rgb-forbered", "--static-confirmed")
        bus = self.bus(report)
        if self.fake:
            baseline, auth = report["baseline"], report["authorization"]
            if (auth["uid"] != self.uid or auth["binding"] != baseline["binding"]
                    or auth["binding"]["pci"] != self.args.pci
                    or auth["binding"]["uuid"] != self.args.uuid):
                fail("Testens baseline/tillatelse matcher ikke valgt kort.")
            self.directory(STATE, 0o700)
            self.directory(Path("etc/agentic-rigg"))
            for relative, value in ((STATE / "baseline.json", baseline), (CONFIG, auth)):
                old = self.path(relative)
                if old.exists() and bounded_json(self.check_file(relative)) != value:
                    fail("Eksisterende RGB-utgangspunkt erstattes ikke.")
                self.atomic(relative, json.dumps(value).encode(), 0o600)
        self.install_assets(binary, assets)
        self.start(bus)

    def resolve(self):
        if self.active():
            fail("Bruk appens bekreftelse mens RGB-hjelperen kjører.")
        self.selected_authorization()
        binary = self.check_file(BIN)
        self.helper(binary, "--rgb-avklar", "--static-confirmed")
        if self.fake:
            # Kun testfixture. Rust-adapterens egne tester dekker den ekte,
            # varige kvitteringen og journalryddingen uten maskinvare.
            self.path(STATE / "journal.json").unlink(missing_ok=True)
        report = self.helper(binary, "--rgb-adapter")
        self.start(self.bus(report))

    def restore(self):
        if self.active():
            fail("Bruk Gjenopprett utgangspunkt i appen mens RGB-hjelperen kjører.")
        self.selected_authorization()
        self.helper(self.check_file(BIN), "--rgb-gjenopprett", "--restore-original")
        # Verken start, resolve eller sletting av journal etter fysisk skriving.

    def deactivate(self):
        self.selected_authorization()
        self.no_journal()
        if not self.fake:
            self.command(["/usr/bin/systemctl", "stop", UNIT])
        # systemctl stop alene utelukker ikke en manuelt startet arbeider.
        # Samme lås som Rust-backend hindrer ny journal mellom siste sjekk
        # og fjerningen av autorisasjonen. Alle grunnopptak beholdes.
        with self.worker_lock():
            self.selected_authorization()
            self.no_journal()
            for relative in (CONFIG, BIN, UNIT_PATH, DROPIN, POLICY):
                if self.check_file(relative, missing=True) is not None:
                    self.path(relative).unlink()
            for name in ASSETS:
                path = SCRIPTS / name
                if self.check_file(path, missing=True) is not None:
                    self.path(path).unlink()
        if not self.fake:
            self.command(["/usr/bin/systemctl", "daemon-reload"])


def main():
    args = parse()
    admin = Admin(args)
    getattr(admin, args.action)()
    messages = {
        "activate": "RGB-tilgangen er aktivert. Ingen farge er skrevet av aktiveringen.",
        "resolve": "Utgangspunktet er brukerbekreftet og RGB-hjelperen er startet.",
        "restore": "Gjenopprettingsforsøket er utført. Se lyset og bekreft utgangspunktet før avklaring.",
        "deactivate": "RGB-tilgangen er slått av. Lagret utgangspunkt er beholdt.",
    }
    print(messages[args.action])


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("RGB: " + str(error), file=sys.stderr)
        sys.exit(1)
