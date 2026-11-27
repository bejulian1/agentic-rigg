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
SCRIPTS = Path("usr/local/libexec/agentic-rigg-rgb-v2")
BIN = SCRIPTS / "agentic-rigg-helper"
LEGACY_SCRIPTS = Path("usr/local/libexec/agentic-rigg-rgb")
LEGACY_ASSETS = (
    Path("usr/local/libexec/agentic-rigg-rgb-helper"),
    Path("usr/share/polkit-1/actions/io.github.AgenticRigg.rgb.policy"),
    *(LEGACY_SCRIPTS / name for name in ("rgb-admin.py", "rgb-aktiver.sh",
      "rgb-deaktiver.sh", "rgb-avklar.sh", "rgb-gjenopprett.sh")),
)
CONFIG = Path("etc/agentic-rigg/rgb.json")
STATE = Path("var/lib/agentic-rigg-rgb")
INACTIVE_MARKER = Path("var/lib/agentic-rigg-rgb.inactive")
UNIT_PATH = Path("etc/systemd/system") / UNIT
OLD_DROPIN = Path("etc/systemd/system") / (UNIT + ".d/card.conf")
OLD_RUN_DROPIN = Path("run/systemd/system") / (UNIT + ".d/card.conf")
DROPIN = OLD_RUN_DROPIN.with_name('50-agentic-rigg-card.conf')
SOCKET = Path("run/agentic-rigg-rgb/helper.sock")
MAX_JSON = 1024 * 1024


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


def parse(fixture):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("action", choices=("activate", "deactivate", "resolve", "restore"))
    parser.add_argument("--pci")
    parser.add_argument("--uuid")
    parser.add_argument("--static-confirmed", action="store_true")
    parser.add_argument("--restore-original", action="store_true")
    parser.add_argument("--inspect", action="store_true")
    parser.add_argument("--recovery-id")
    if fixture:
        parser.add_argument("--root", required=True, type=Path)
        parser.add_argument("--uid", required=True, type=int)
        parser.add_argument("--report", type=Path)
    # Argparse tillater ellers gjentatte flagg. Her skal ingen sisteverdi
    # overstyre et tidligere kortvalg eller en annen test-/produksjonsrot.
    for flag in ("--pci", "--uuid", "--root", "--uid", "--report", "--inspect", "--recovery-id",
                 "--static-confirmed", "--restore-original"):
        if sum(arg == flag or arg.startswith(flag + "=") for arg in sys.argv[1:]) > 1:
            fail("Et RGB-argument er gitt flere ganger: " + flag)
    args = parser.parse_args()
    uid_only = args.action == "deactivate" and args.pci is None and args.uuid is None
    args.pci = (args.pci or "").lower()
    if not uid_only and not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{2}:[01][0-9a-f]\.0", args.pci):
        fail("Ugyldig grafikkfunksjon. Full PCI-adresse på funksjon 0 kreves.")
    if not uid_only and not re.fullmatch(r"GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", args.uuid or ""):
        fail("Full GPU-UUID kreves.")
    if args.action != "resolve" and (args.inspect or args.recovery_id is not None):
        fail("Gjenopprettingsidentiteten gjelder bare avklaring.")
    if args.action == "activate":
        if not args.static_confirmed or args.restore_original:
            fail("Denne handlingen krever uttrykkelig bekreftet fast lys.")
    elif args.action == "resolve":
        valid_id = re.fullmatch(r"[0-9a-f]{32}", args.recovery_id or "") is not None
        if args.restore_original or not (
                (args.inspect and not args.static_confirmed and args.recovery_id is None)
                or (not args.inspect and args.static_confirmed and valid_id)):
            fail("Hent gjenopprettingsmålet først, og bekreft deretter lyset med samme identitet.")
    elif args.action == "restore":
        if not args.restore_original or args.static_confirmed:
            fail("Gjenoppretting må velges uttrykkelig.")
    elif args.static_confirmed or args.restore_original:
        fail("Deaktivering tar ingen fargebekreftelse.")
    return args


class Admin:
    unit = UNIT
    unit_path = UNIT_PATH

    def __init__(self, args, fixture):
        self.args = args
        self.fake = fixture
        if self.fake:
            self.root = fixture_root(args.root)
            self.uid = args.uid
            if args.action != "deactivate" and args.report is None:
                fail("Filfixture krever et eksplisitt --report; ingen hjelper kjøres.")
        else:
            self.root = Path("/")
            if os.geteuid() != 0:
                fail("RGB-handlingen må godkjennes gjennom pkexec.")
            try:
                self.uid = int(os.environ["PKEXEC_UID"])
                pwd.getpwuid(self.uid)
            except (KeyError, ValueError, OverflowError):
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

    def command(self, argv, timeout=30):
        env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL": "C",
               "PKEXEC_UID": str(self.uid)}
        result = subprocess.run(argv, check=False, capture_output=True, stdin=subprocess.DEVNULL,
                                encoding="utf-8", errors="replace", timeout=timeout, env=env)
        if result.returncode:
            fail((result.stderr or result.stdout or "RGB-kommandoen feilet.").strip())
        return result.stdout

    def systemctl(self, *args):
        if not self.fake:
            return self.command(["/usr/bin/systemctl", *args])
        # Samme livssyklus som produksjon, med tilstander og feil fra en
        # privat fil. Ingen ekstern kommando kan kjøres via fixturen.
        relative = Path("fixture-systemd.json")
        path = self.check_file(relative, missing=True)
        state = bounded_json(path) if path else {}
        operation = args[0]
        if operation == "show":
            load = state.get("LoadState", "loaded" if self.path(self.unit_path).exists() else "not-found")
            return f"LoadState={load}\nActiveState={state.get('ActiveState', 'inactive')}\nJob={state.get('Job', '')}\n"
        failure = state.get("fail", {}).get(operation)
        if failure == "before":
            fail("Simulert systemctl-feil: " + operation)
        if operation in ("start", "stop"):
            state["ActiveState"] = "active" if operation == "start" else "inactive"
            state["Job"] = ""
            self.atomic(relative, json.dumps(state).encode(), 0o600)
        if failure == "after":
            fail("Simulert systemctl-feil etter endring: " + operation)
        return ""

    def unit_state(self):
        output = self.systemctl("show", "--all", "--property=LoadState,ActiveState,Job", self.unit)
        info = dict(line.split("=", 1) for line in output.splitlines() if "=" in line)
        load, active, job = info["LoadState"], info["ActiveState"], info["Job"]
        if load not in ("loaded", "not-found", "masked") or active not in (
                "active", "reloading", "inactive", "failed", "activating", "deactivating",
                "maintenance", "refreshing"):
            fail("RGB-tjenestens tilstand kunne ikke bekreftes.")
        return active, job not in ("", "0")

    def stopped(self):
        active, job = self.unit_state()
        return active in ("inactive", "failed") and not job

    def stop(self):
        if self.stopped():
            return
        try:
            self.systemctl("stop", self.unit)
        except (RuntimeError, OSError, subprocess.SubprocessError):
            if not self.stopped():
                raise
            return
        if not self.stopped():
            fail("RGB-tjenesten er fortsatt aktiv. Tillatelsen og journalen er beholdt.")

    def helper(self, executable, flag, *options, stage=None):
        prefix = []
        if flag != "--rgb-adapter":
            bus = self.bus(self.helper(executable, "--rgb-adapter"))
            prefix = self.sandbox(bus, flag, stage)
        if self.fake:
            # Aldri kjør et faktisk hjelpeprogram fra en filfixture,
            # heller ikke dersom testprosessen tilfeldigvis er root.
            payload = bounded_json(self.args.report)
        else:
            argv = [str(executable), flag, "--pci", self.args.pci,
                    "--uuid", self.args.uuid, "--uid", str(self.uid), *options]
            try:
                output = self.command(prefix + argv)
            except (RuntimeError, OSError, subprocess.SubprocessError) as error:
                if prefix:
                    fail(f'systemd-run ({flag}): {error}. Kontroller enhetsegenskapene og journalen for '
                         'agentic-rigg-rgb-admin.service før du prøver igjen.')
                raise
            payload = strict_json(output)
        if type(payload) is not dict:
            fail("RGB-hjelperens svar må være et JSON-objekt.")
        if payload.get("ok") is not True:
            fail(str(payload.get("error", "RGB-hjelperen avviste handlingen.")))
        if type(payload.get("report")) is not dict:
            fail("RGB-hjelperens rapport mangler eller har feil type.")
        return payload["report"]

    def sandbox(self, bus, flag, stage):
        properties = [line for line in self.unit_template().decode().splitlines()
                      if '=' in line and not line.startswith(('#', 'Description=', 'Condition', 'Type=',
                          'Exec', 'Restart=', 'TimeoutStop', 'RuntimeDirectory', 'StateDirectory'))]
        if 'DevicePolicy=closed' not in properties or 'ProtectSystem=strict' not in properties:
            fail('Ufullstendig sandkasse i RGB-enhetsmalen.')
        properties.extend((f'DeviceAllow=/dev/i2c-{bus} rw', 'RuntimeMaxSec=20s',
                           'ReadWritePaths=/var/lib/agentic-rigg-rgb'))
        if flag == '--rgb-forbered':
            if stage is None:
                fail('Forberedelse krever en privat tillatelsesmappe.')
            properties.extend((f'BindPaths={stage}:/etc/agentic-rigg', 'ReadWritePaths=/etc/agentic-rigg'))
        return ['/usr/bin/systemd-run', '--wait', '--pipe', '--collect', '--quiet',
                '--unit=agentic-rigg-rgb-admin', '--service-type=exec', *('--property=' + line for line in properties), '--']

    def prepare(self, binary):
        self.directory(CONFIG.parent)
        with tempfile.TemporaryDirectory(dir=self.directory(Path('run')), prefix='rigg-rgb-auth-') as tmp:
            stage = Path(tmp).relative_to(self.root) / CONFIG.name
            previous = self.check_file(CONFIG, missing=True)
            if previous is not None:
                self.atomic(stage, previous.read_bytes(), 0o600)
            report = self.helper(binary, '--rgb-forbered', '--static-confirmed', stage=tmp)
            self.bus(report)
            auth = report['authorization']
            if (type(auth['uid']) is not int or auth['uid'] != self.uid
                    or auth['binding'] != report['baseline']['binding']
                    or (auth['binding']['pci'], auth['binding']['uuid']) != (self.args.pci, self.args.uuid)):
                fail('Grunnopptak eller tillatelse gjelder et annet kort.')
            if not self.fake:
                staged = self.check_file(stage)
                if bounded_json(staged, 16384) != auth:
                    fail('Den private RGB-tillatelsen samsvarer ikke med hjelperens svar.')
                self.atomic(CONFIG, json.dumps(auth).encode(), 0o600)
            return report

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
        if self.check_file(STATE / "journal.json", missing=True) is not None:
            fail("Uavklarte RGB-endringer må avklares før denne handlingen.")

    @contextmanager
    def lock(self, name):
        state = self.directory(STATE, 0o700)
        if stat.S_IMODE(state.lstat().st_mode) != 0o700:
            fail("RGB-tilstandsmappen er ikke privat.")
        self.check_file(STATE / name, missing=True)
        fd = os.open(state / name, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW
                     | os.O_NONBLOCK | os.O_CLOEXEC, 0o600)
        try:
            m = os.fstat(fd)
            if (not stat.S_ISREG(m.st_mode) or m.st_uid != os.geteuid()
                    or stat.S_IMODE(m.st_mode) != 0o600 or m.st_nlink != 1):
                fail("RGB-låsefilen har utrygg type, eier eller modus.")
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                fail("En RGB-arbeider eller administratorhandling pågår. Prøv igjen.")
            yield
        finally:
            os.close(fd)

    def selected_authorization(self, missing=False):
        path = self.check_file(CONFIG, missing=missing)
        if path is None:
            return None
        if stat.S_IMODE(path.lstat().st_mode) != 0o600:
            fail("RGB-tillatelsen er ikke privat.")
        auth = bounded_json(path, 16_384)
        if type(auth) is not dict or type(auth.get("binding")) is not dict:
            fail("RGB-tillatelsen har feil format.")
        binding = auth['binding']
        if type(auth['uid']) is not int or auth['uid'] != self.uid:
            fail('RGB-tillatelsen gjelder en annen bruker.')
        if not self.args.pci and self.args.action == 'deactivate':
            matches = self.stopped() and self.check_file(STATE / 'journal.json', missing=True) is None
        else:
            matches = binding['pci'] == self.args.pci and binding['uuid'] == self.args.uuid
        if not matches:
            fail(f"Bundet RGB-kort: {binding['pci']} {binding['uuid']}. Stopp og avklar arbeideren først.")
        return auth

    def unit_template(self):
        path = self.check_file(SCRIPTS / UNIT)
        with path.open("rb") as file:
            data = file.read(MAX_JSON + 1)
        if len(data) > MAX_JSON:
            fail("Den betrodde RGB-enhetsmalen er for stor.")
        return data

    def start(self, bus):
        self.remove(OLD_DROPIN)
        self.remove(OLD_RUN_DROPIN)
        content = ("[Service]\nDeviceAllow=\nDeviceAllow=/dev/i2c-" + str(bus) + " rw\n")
        self.atomic(DROPIN, content.encode(), 0o644)
        self.systemctl("daemon-reload")
        # Ingen enable: RGB skal ikke få automatisk oppstart ved innlogging/boot.
        self.systemctl("start", UNIT)
        self.wait_for_worker()

    def wait_for_worker(self):
        if self.fake:
            return
        for _ in range(40):
            try:
                m = self.path(SOCKET).lstat()
                if stat.S_ISSOCK(m.st_mode) and m.st_uid == self.uid:
                    if self.unit_state() == ("active", False):
                        return
            except FileNotFoundError:
                pass
            time.sleep(0.05)
        fail("RGB-tjenesten startet ikke med riktig socket. Se RGB-tjenestens journal.")

    def activate(self):
        self.no_journal()
        binary = self.check_file(BIN)
        unit = self.unit_template()
        auth = self.selected_authorization(missing=True)
        if not self.stopped():
            installed = self.check_file(UNIT_PATH, missing=True)
            if (auth is not None and self.unit_state() == ("active", False)
                    and installed is not None and installed.read_bytes() == unit
                    and self.check_file(DROPIN, missing=True) is not None
                    and all(self.check_file(path, missing=True) is None for path in (OLD_DROPIN, OLD_RUN_DROPIN))):
                self.wait_for_worker()
                return
            fail("En eldre RGB-hjelper kjører. Avklar og slå av RGB-tilgangen først.")
        if any(self.check_file(path, missing=True) is not None for path in LEGACY_ASSETS):
            fail("Slå av den tidligere RGB-tilgangen før du aktiverer den nye hjelperen.")
        self.remove(INACTIVE_MARKER)
        report = self.prepare(binary)
        bus = self.bus(report)
        if self.fake:
            baseline, auth = report["baseline"], report["authorization"]
            self.directory(STATE, 0o700)
            self.directory(Path("etc/agentic-rigg"))
            with self.lock("worker.lock"):
                old = self.check_file(STATE / "baseline.json", missing=True)
                if old is not None and bounded_json(old) != baseline:
                    if self.selected_authorization(missing=True) is not None:
                        fail("Slå av tilgangen før et nytt utgangspunkt kan lagres.")
                    for number in range(1, sys.maxsize):
                        archived = STATE / f"baseline-{number}.json"
                        if self.check_file(archived, missing=True) is None:
                            self.atomic(archived, old.read_bytes(), 0o600)
                            break
                self.atomic(STATE / "baseline.json", json.dumps(baseline).encode(), 0o600)
                self.atomic(CONFIG, json.dumps(auth).encode(), 0o600)
        self.atomic(UNIT_PATH, unit, 0o644)
        self.start(bus)

    def resolve(self):
        self.selected_authorization()
        if not self.stopped():
            fail("RGB-hjelperen kjører. Bekreft i appen, eller velg «Gjenopprett lagret lys» for å stoppe en eldre hjelper før du kontrollerer lyset.")
        if self.check_file(STATE / "journal.json", missing=True) is None:
            with self.lock("worker.lock"):
                self.no_journal()
            return {} if self.args.inspect else None
        binary = self.check_file(BIN)
        if self.args.inspect:
            report = self.helper(binary, "--rgb-avklaringsmal")
            return {"expected": report["expected"], "recoveryId": report["recoveryId"]}
        self.helper(binary, "--rgb-avklar", "--static-confirmed",
                    "--recovery-id", self.args.recovery_id)
        if self.fake:
            # Kun testfixture. Rust-adapterens egne tester dekker den ekte,
            # varige kvitteringen og journalryddingen uten maskinvare.
            self.path(STATE / "journal.json").unlink(missing_ok=True)

    def restore(self):
        self.selected_authorization()
        self.stop()
        if self.check_file(STATE / "journal.json", missing=True) is None:
            with self.lock("worker.lock"):
                self.no_journal()
            return
        self.helper(self.check_file(BIN), "--rgb-gjenopprett", "--restore-original")
        # Verken start, resolve eller sletting av journal etter fysisk skriving.

    def deactivate(self):
        auth = self.selected_authorization(missing=True)
        if not self.args.pci and not self.stopped():
            fail('Stopp arbeideren før deaktivering uten kortidentitet.')
        if auth and self.check_file(STATE / 'journal.json', missing=True) is not None:
            binding = auth['binding']
            fail(f"Uavklart RGB-journal for {binding['pci']} {binding['uuid']}. Velg «Gjenopprett lagret lys».")
        self.no_journal()
        self.stop()
        # systemctl stop alene utelukker ikke en manuelt startet arbeider.
        # Samme lås som Rust-backend hindrer ny journal mellom siste sjekk
        # og fjerningen av autorisasjonen. Alle grunnopptak beholdes.
        with self.lock("worker.lock"):
            self.selected_authorization(missing=True)
            self.no_journal()
            for relative in (UNIT_PATH, DROPIN, OLD_DROPIN, OLD_RUN_DROPIN, *LEGACY_ASSETS):
                self.remove(relative)
            self.systemctl("daemon-reload")
            # Tillatelsen fjernes sist. Den binder samme bruker/kort også
            # når opprydding må fortsette etter feil eller avbrudd.
            self.remove(CONFIG)
            # Brukerinstallasjonen kan lese dette resultatet selv når
            # de private dokumentene ligger bak mapper med modus 0700.
            self.atomic(INACTIVE_MARKER, b"RGB-tilgangen er avsluttet.\n", 0o644)

    def remove(self, relative):
        path = self.check_file(relative, missing=True)
        if path is not None:
            path.unlink()
        try:
            parent = self.directory(relative.parent, create=False)
        except FileNotFoundError:
            return
        fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


SYSTEM_LOCK = '/run/agentic-rigg-system.lock'
# system-hent.py holder den felles låsen gjennom en hel oppdatering og gir
# den videre til de nyinstallerte skriptene som et arvet filhåndtak.
LOCK_FD_ENV = 'AGENTIC_RIGG_SYSTEM_LOCK_FD'


def inherited_lock(path=SYSTEM_LOCK):
    """Filhåndtaket til en arvet, holdt systemlås, eller None uten arv.

    Håndtaket må være nøyaktig låsefila (samme enhet og inode). flock på
    den samme åpne fila lykkes bare når forelderen holder låsen, eller
    ingen gjør det; holder en annen prosess den, avvises handlingen.
    """
    raw = os.environ.get(LOCK_FD_ENV)
    if raw is None:
        return None
    if not re.fullmatch(r"[3-9]|[1-9][0-9]{1,4}", raw, re.ASCII):
        fail("Ugyldig arvet systemlås.")
    fd = int(raw)
    try:
        held = os.fstat(fd)
        named = os.lstat(path)
    except OSError:
        fail("Den arvede systemlåsen finnes ikke.")
    if (not stat.S_ISREG(held.st_mode) or held.st_uid != os.geteuid()
            or (held.st_dev, held.st_ino) != (named.st_dev, named.st_ino)):
        fail("Det arvede filhåndtaket er ikke systemlåsen.")
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        fail("En annen administratorhandling pågår. Vent og prøv igjen.")
    return fd


@contextmanager
def system_lock(path=SYSTEM_LOCK):
    if inherited_lock(path) is not None:
        # Aldri LOCK_UN: låsen tilhører den som ga den videre.
        yield
        return
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, 'wb') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            fail("En annen administratorhandling pågår. Vent og prøv igjen.")
        yield


def main():
    # Historisk testinngang. Krev alltid en privat filrot, uansett filsti.
    args = parse(True)
    admin = Admin(args, True)
    with admin.lock("admin.lock"):
        report = getattr(admin, args.action)()
    if args.inspect:
        print(json.dumps(report))
        return
    messages = {
        "activate": "RGB-tilgangen er aktivert. Ingen farge er skrevet av aktiveringen.",
        "resolve": "RGB-journalen er avklart. Arbeideren er stoppet.",
        "restore": "Arbeideren er stoppet. Kontroller det lagrede lyset før eventuell avklaring.",
        "deactivate": "RGB-tilgangen er slått av. Lagret utgangspunkt er beholdt.",
    }
    print(messages[args.action])


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print("RGB: " + str(error), file=sys.stderr)
        sys.exit(1)
