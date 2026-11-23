#!/usr/bin/python3
"""Kortbundet NVML-aktivering fra en verifisert systeminstallasjon."""
import argparse
from contextlib import nullcontext
import datetime
import json
import os
from pathlib import Path
import pwd
import re
import runpy
import stat
import subprocess
import sys
import tempfile

SCRIPTS = Path('usr/local/libexec/agentic-rigg')
BIN = SCRIPTS.parent / 'agentic-rigg-helper'
UNIT = 'agentic-rigg-helper.service'
CONFIG = Path('etc/agentic-rigg/helper.json')
STATE = Path('var/lib/agentic-rigg')
DROPIN = Path('etc/systemd/system') / (UNIT + '.d/gpu.conf')
# Det opprinnelige og de fire nyeste tidligere grunnopptakene beholdes.
KEEP_PREVIOUS = 5
FIXTURE = Path(__file__).absolute() != Path('/') / SCRIPTS / 'nvml-admin.py'
# Den felles administrasjonskoden ligger som egen kopi ved siden av dette
# skriptet, fra samme utgivelse. GPU-oppdateringen kan la en aktiv eller
# uavklart RGB-del stå urørt (`system-install --keep-rgb`), og da kan RGB-mappas
# rgb-admin.py være fra et eldre bygg (uten arvet systemlås).
common = runpy.run_path(str(Path(__file__).with_name('rgb-admin.py')))
fail = common['fail']


def parse():
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument('action', choices=('activate', 'deactivate'))
    parser.add_argument('--pci')
    parser.add_argument('--uuid')
    if FIXTURE:
        parser.add_argument('--root', type=Path, required=True)
        parser.add_argument('--uid', type=int, required=True)
    flags = [arg.partition('=')[0] for arg in sys.argv[1:] if arg.startswith('--')]
    if len(flags) != len(set(flags)):
        parser.error('Duplisert argument.')
    args = parser.parse_args()
    if args.action == 'activate':
        valid_uuid = re.fullmatch(r'GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}', args.uuid or '')
        if FIXTURE and re.fullmatch(r'GPU-sim0000-[0-9]{4}', args.uuid or ''):
            valid_uuid = True
        if (not re.fullmatch(r'[0-9a-fA-F]{4}:[0-9a-fA-F]{2}:[01][0-9a-fA-F]\.[0-7]', args.pci or '')
                or not valid_uuid):
            parser.error('Aktivering krever full PCI-adresse og GPU-UUID.')
        args.pci = args.pci.lower()
    elif args.pci is not None or args.uuid is not None:
        parser.error('Deaktivering bruker den lagrede kortbindingen.')
    return args


class Admin(common['Admin']):
    unit = UNIT
    unit_path = Path('etc/systemd/system') / UNIT

    def __init__(self, args):
        self.fake = FIXTURE
        self.args = args
        self.root = common['fixture_root'](args.root) if FIXTURE else Path('/')
        if not FIXTURE and os.geteuid() != 0:
            fail('NVML-handlingen må godkjennes gjennom pkexec.')
        self.uid = args.uid if FIXTURE else int(os.environ['PKEXEC_UID'])
        if not 0 < self.uid < 2**32 - 1:
            fail('Ugyldig klient-UID.')
        if not FIXTURE:
            pwd.getpwuid(self.uid)

    def authorization(self):
        path = self.check_file(CONFIG, missing=True)
        if path is None:
            return None
        auth = common['bounded_json'](path, 16384)
        if not isinstance(auth, dict):
            raise ValueError('GPU-tillatelsen må være et JSON-objekt.')
        if type(auth['uid']) is not int or auth['uid'] != self.uid:
            fail('GPU-tillatelsen gjelder en annen bruker.')
        return auth

    def clean(self, auth):
        if self.check_file(STATE / 'dirty.json', missing=True) is not None:
            if auth is None or self.fake:
                fail('Uavklarte GPU-endringer. Grunnverdier og tillatelse beholdes.')
            try:
                installer = runpy.run_path('/usr/local/libexec/agentic-rigg-system/system-install')
                installer['restore_nvml'](self.check_file(SCRIPTS / UNIT).read_bytes())
            except (OSError, ValueError, subprocess.SubprocessError) as exc:
                fail(f"Bundet kort: {auth['pci']} {auth['uuid']}. Gjenoppretting feilet: {exc}")
            if self.check_file(STATE / 'dirty.json', missing=True) is not None:
                fail('Uavklarte GPU-endringer etter gjenoppretting.')

    def activate(self):
        auth = self.authorization()
        if auth and (auth['pci'], auth['uuid']) != (self.args.pci, self.args.uuid):
            fail(f"Slå av eksisterende binding først: {auth['pci']} {auth['uuid']}")
        template = self.check_file(SCRIPTS / UNIT).read_bytes()
        binary = self.check_file(BIN)
        if self.fake:
            minor, build = '0', 25
        else:
            info = Path(f'/proc/driver/nvidia/gpus/{self.args.pci}/information').read_text()
            fields = dict(line.split(':', 1) for line in info.splitlines() if ':' in line)
            minor = fields['Device Minor'].strip()
            if (fields['GPU UUID'].strip() != self.args.uuid or not re.fullmatch(r'[0-9]{1,4}', minor)
                    or not Path('/dev/nvidia' + minor).is_char_device()):
                fail('PCI/UUID og driverens enhetsnode samsvarer ikke.')
            match = re.search(r'bygg ([0-9]+)', self.command([str(binary), '--version']))
            if match is None:
                fail('Hjelperen oppga ikke et gyldig byggnummer.')
            build = int(match[1])
        self.stop()
        self.clean(auth)
        self.archive_baseline()
        self.atomic(self.unit_path, template, 0o644)
        self.atomic(DROPIN, f'[Service]\nDeviceAllow=\nDeviceAllow=/dev/nvidiactl rw\nDeviceAllow=/dev/nvidia{minor} rw\n'.encode(), 0o644)
        self.atomic(CONFIG, json.dumps(dict(versjon=2, uid=self.uid, enabled=True, pci=self.args.pci,
                    uuid=self.args.uuid, bygg=build, aktivert=datetime.datetime.now(datetime.timezone.utc).isoformat())).encode(), 0o644)
        self.systemctl('daemon-reload')
        self.systemctl('enable', UNIT)
        self.systemctl('start', UNIT)
        if self.unit_state() != ('active', False):
            fail('NVML-tjenesten er ikke klar.')

    def previous_baselines(self):
        """Trygge `baseline-forrige-*.json`, eldste først (opptakstid, navn)."""
        found = []
        for entry in os.scandir(self.path(STATE)):
            if not re.fullmatch(r'baseline-forrige-[A-Za-z0-9_-]{1,80}\.json', entry.name, re.ASCII):
                continue
            try:
                path = self.check_file(STATE / entry.name)
            except (RuntimeError, OSError):
                continue  # Rører aldri en fil med feil eier, type eller lenker.
            found.append((path.lstat().st_mtime_ns, entry.name, path))
        return [path for *_, path in sorted(found)]

    def archive_baseline(self):
        """Legg det forrige grunnopptaket til side før hjelperen tar et nytt.

        Hjelperen fanger et ferskt opptak ved hver aktivering (kort, VBIOS og
        driver kan ha endret seg). Et opptak som allerede er bevart, legges
        ikke til igjen, og høyst `KEEP_PREVIOUS` beholdes: det eldste
        (det opprinnelige utgangspunktet) og de nyeste.
        """
        old = self.check_file(STATE / 'baseline.json', missing=True)
        if old is None:
            return
        if stat.S_IMODE(old.stat().st_mode) != 0o600 or stat.S_IMODE(old.parent.stat().st_mode) != 0o700:
            fail('Grunnverdifila og mappa må være private.')
        current = old.read_bytes()
        if any(path.read_bytes() == current for path in self.previous_baselines()):
            self.remove(STATE / 'baseline.json')
        else:
            stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
            with tempfile.NamedTemporaryFile(prefix=f'baseline-forrige-{stamp}-', suffix='.json',
                                             dir=old.parent, delete=False) as dest:
                old.replace(dest.name)
        # Opprydding er bare husholdning: en feil her stopper ikke aktiveringen.
        try:
            previous = self.previous_baselines()
            for path in previous[1:max(1, len(previous) - KEEP_PREVIOUS + 1)]:
                self.remove(path.relative_to(self.root))
        except (OSError, RuntimeError) as error:
            print(f'NVML: Eldre grunnopptak ble ikke ryddet: {error}', file=sys.stderr)

    def deactivate(self):
        auth = self.authorization()
        if auth is None and not self.stopped():
            fail('Stopp arbeideren før deaktivering uten en lagret binding.')
        self.stop()
        self.clean(auth)
        if self.check_file(self.unit_path, missing=True):
            self.systemctl('disable', UNIT)
        for path in (DROPIN, self.unit_path):
            self.remove(path)
        self.systemctl('daemon-reload')
        self.remove(CONFIG)


if __name__ == '__main__':
    try:
        admin = Admin(parse())
        with nullcontext() if FIXTURE else common['system_lock']():
            getattr(admin, admin.args.action)()
    except (RuntimeError, OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        sys.exit('NVML: ' + str(error))
