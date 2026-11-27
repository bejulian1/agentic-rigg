#!/usr/bin/python3
"""Activate, boot or revoke both sandboxed workers for one card."""
import argparse
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
import secrets
from types import SimpleNamespace

SCRIPTS = Path('usr/local/libexec/agentic-rigg')
BIN = SCRIPTS.parent / 'agentic-rigg-helper'
UNIT = 'agentic-rigg-helper.service'
CONFIG = Path('etc/agentic-rigg/helper.json')
STATE = Path('var/lib/agentic-rigg')
BOOT = 'agentic-rigg-card.service'
UNITS = Path('etc/systemd/system')
OLD_DROPIN = UNITS / (UNIT + '.d/gpu.conf')
STATUS = Path('run/agentic-rigg-card/status.json')
DROPIN = Path('run/systemd/system') / (UNIT + '.d/gpu.conf')
# Det opprinnelige og de fire nyeste tidligere grunnopptakene beholdes.
KEEP_PREVIOUS = 5
BOOT_ERRORS = (OSError, RuntimeError, ValueError, KeyError, subprocess.SubprocessError)
FIXTURE = Path(__file__).absolute() != Path('/') / SCRIPTS / 'nvml-admin.py'
common = runpy.run_path(str(Path(__file__).with_name('rgb-admin.py')))
fail = common['fail']


def parse():
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument('action', choices=('activate', 'deactivate', 'boot'))
    parser.add_argument('--card-control', action='store_true')
    parser.add_argument('--pci')
    parser.add_argument('--uuid')
    if FIXTURE:
        parser.add_argument('--root', type=Path, required=True)
        parser.add_argument('--uid', type=int, required=True)
    flags = [arg.partition('=')[0] for arg in sys.argv[1:] if arg.startswith('--')]
    if len(flags) != len(set(flags)):
        parser.error('Duplisert argument.')
    args = parser.parse_args()
    if args.card_control != (args.action == 'activate'):
        parser.error('Aktivering krever eksplisitt samtykke til felles skjermkortstyring.')
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
        if not FIXTURE and args.action == 'boot':
            self.uid = common['bounded_json'](self.check_file(CONFIG))['uid']
        else:
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

    def rgb(self):
        # Same lock and private root for both workers, independent systemd units.
        admin = common['Admin'](self.root, self.uid, self.fake, SimpleNamespace(**vars(self.args)))
        admin.args.report = self.path(Path('fixture-rgb.json'))
        return admin

    def write_json(self, path, value, mode):
        raw = json.dumps(value, separators=(',', ':')).encode()
        old = self.check_file(path, missing=True)
        if old is None or old.read_bytes() != raw:
            self.atomic(path, raw, mode)

    def installed_build(self):
        revision = self.check_file(Path('usr/local/libexec/agentic-rigg-system/revision'))
        if self.fake:
            match = re.fullmatch(r'3:([1-9][0-9]*)\n', revision.read_text())
            if not match:
                fail('Ufullstendig systeminstallasjon.')
            return int(match[1])
        build = int(re.search(r'bygg ([0-9]+)', self.command([str(self.check_file(BIN)), '--version']))[1])
        if revision.read_text() != f'3:{build}\n':
            fail('Oppdater systemkomponentene før aktivering.')
        return build

    def activate(self):
        auth = self.authorization()
        rgb = self.rgb()
        build = self.installed_build()
        rgb.selected_authorization(missing=True)
        if auth and (auth['pci'], auth['uuid']) != (self.args.pci, self.args.uuid):
            fail('Slå av den eksisterende kortbindingen først.')
        if (not auth or auth.get('enabled') is not True or auth.get('scope') != 'card'
                or auth.get('bygg') != build or not re.fullmatch('[a-f0-9]{32}', auth.get('generation') or '')):
            self.stop()
            rgb.stop()
            self.clean(auth)
            if not auth:
                self.archive_baseline()
            auth = dict(versjon=2, uid=self.uid, enabled=True, pci=self.args.pci,
                        uuid=self.args.uuid, bygg=build, scope='card', generation=secrets.token_hex(16),
                        aktivert=datetime.datetime.now(datetime.timezone.utc).isoformat())
        if self.check_file(self.unit_path, missing=True):
            self.systemctl('disable', UNIT)
        self.remove(common['INACTIVE_MARKER'])
        self.remove(OLD_DROPIN)
        self.atomic(self.unit_path, self.check_file(SCRIPTS / UNIT).read_bytes(), 0o644)
        self.atomic(UNITS / BOOT, self.check_file(SCRIPTS / BOOT).read_bytes(), 0o644)
        self.systemctl('daemon-reload')
        self.systemctl('enable', BOOT)
        self.write_json(CONFIG, auth, 0o644)
        # Complete initial discovery under the same lock as consent. Only the
        # asynchronous retry needs to reacquire it after this invocation.
        if not self.boot() and not self.fake:
            self.systemctl('--no-block', 'start', BOOT)

    def gpu_fields(self, auth):
        info = self.path(Path('proc/driver/nvidia/gpus') / auth['pci'] / 'information')
        fields = dict(line.split(':', 1) for line in info.read_text().splitlines() if ':' in line)
        if fields['GPU UUID'].strip() != auth['uuid'] or fields['Bus Location'].strip().lower() != auth['pci']:
            fail('NVIDIA-driveren bekrefter ikke kortbindingen.')
        return fields

    def start_nvml(self, fields):
        minor = fields['Device Minor'].strip()
        if not re.fullmatch(r'[0-9]{1,4}', minor):
            fail('NVIDIA-driveren har ikke oppgitt et gyldig enhetsnummer.')
        if not self.fake:
            node = self.path(Path('dev/nvidia' + minor)).lstat()
            if not stat.S_ISCHR(node.st_mode) or (os.major(node.st_rdev), os.minor(node.st_rdev)) != (195, int(minor)):
                fail('NVIDIA-enhetsnoden samsvarer ikke med kortbindingen.')
        self.ensure_started(self, DROPIN, ['/dev/nvidiactl', '/dev/nvidia' + minor])
        return 'ready'

    def ensure_started(self, admin, path, rules):
        contents = ('[Service]\nDeviceAllow=\n' + ''.join('DeviceAllow=' + rule + ' rw\n' for rule in rules)).encode()
        old = self.check_file(path, missing=True)
        if old is None or old.read_bytes() != contents:
            admin.stop()
            self.atomic(path, contents, 0o644)
            self.systemctl('daemon-reload')
        if admin.unit_state() != ('active', False):
            admin.systemctl('start', admin.unit)

    def withdraw_device(self, admin, dropin):
        admin.stop()
        if self.check_file(dropin, missing=True):
            self.remove(dropin)
            self.systemctl('daemon-reload')

    def boot(self):
        auth = self.authorization()
        if (not auth or auth.get('scope') != 'card' or auth.get('enabled') is not True
                or auth.get('bygg') != self.installed_build()
                or not re.fullmatch('[a-f0-9]{32}', auth.get('generation', ''))
                or not re.fullmatch(r'[a-f0-9]{4}:[a-f0-9]{2}:[01][a-f0-9]\.[0-7]', auth['pci'])
                or not (self.fake or re.fullmatch(r'GPU-[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}', auth['uuid']))):
            fail('Korttillatelsen er ugyldig.')
        self.args.pci, self.args.uuid = auth['pci'], auth['uuid']
        rgb = self.rgb()
        status = dict(pci=auth['pci'], uuid=auth['uuid'], generation=auth['generation'])
        try:
            fields = self.gpu_fields(auth)
        except BOOT_ERRORS as exc:
            for name, worker, path in [('nvml', self, DROPIN), ('rgb', rgb, common['DROPIN'])]:
                self.wait_for_retry(status, name, worker, path, exc)
        else:
            try:
                status['nvml'] = self.start_nvml(fields)
            except BOOT_ERRORS as exc:
                self.wait_for_retry(status, 'nvml', self, DROPIN, exc)
            try:
                status['rgb'] = self.start_rgb(rgb, auth)
            except BOOT_ERRORS as exc:
                self.wait_for_retry(status, 'rgb', rgb, common['DROPIN'], exc)
        self.write_json(STATUS, status, 0o644)
        return 'waiting' not in (status['nvml'], status['rgb'])

    def wait_for_retry(self, status, name, worker, path, error):
        status[name] = 'waiting'
        detail = str(error)
        try:
            self.withdraw_device(worker, path)
        except BOOT_ERRORS as cleanup:
            detail += f' ({cleanup})'
        status['gpuMessage' if name == 'nvml' else 'message'] = detail

    def start_rgb(self, rgb, auth):
        ids = self.path(Path('sys/bus/pci/devices') / auth['pci'])
        if (self.fake and not rgb.args.report.exists()) or (ids.exists() and any(
                (ids / key).read_text().strip().lower() != expected for key, expected in
                [('vendor', '0x10de'), ('device', '0x1e87'), ('subsystem_vendor', '0x10de'), ('subsystem_device', '0x1e87')])):
            self.withdraw_device(rgb, common['DROPIN'])
            return 'unsupported'
        report = rgb.adapter_report()
        bus = rgb.bus(report)
        if not self.fake:
            node = self.path(Path('dev/i2c-' + str(bus))).lstat()
            if not stat.S_ISCHR(node.st_mode) or (os.major(node.st_rdev), os.minor(node.st_rdev)) != (89, bus):
                fail('I2C-enhetsnoden samsvarer ikke med den ferske adapteridentiteten.')
        m = report['metadata']
        adapter = {key: m['adapter'][key] for key in ('port', 'name', 'parentPci')}
        existing = rgb.selected_authorization(missing=True)
        prior = existing
        if not prior and rgb.check_file(common['STATE'] / 'journal.json', missing=True):
            receipt = rgb.check_file(common['STATE'] / 'arkivert-tillatelse.json', missing=True)
            if receipt:
                previous = common['bounded_json'](receipt)
                if (previous['binding']['pci'], previous['binding']['uuid']) != (auth['pci'], auth['uuid']):
                    fail('Den bevarte lysjournalen gjelder et annet kort.')
                prior = previous
        signature = prior['binding'].get('signature', 'palit_100') if prior else 'palit_100'
        if signature not in ('palit_100', 'palit_150'):
            fail('Ukjent signatur i lagret kortbinding.')
        binding = dict(pci=auth['pci'], uuid=auth['uuid'], pciIds=m['pciIds'], address=m['address'],
                       adapter=dict(adapter, driver=m['driver']), signature=signature)
        # This is the expected signature. The sandbox independently identifies it before any setter.
        projection = dict(version=1, uid=self.uid, enabled=True, binding=binding,
                          build=auth['bygg'], generation=auth['generation'])
        template = rgb.unit_template()
        installed = self.check_file(rgb.unit_path, missing=True)
        if (projection != existing or not installed or installed.read_bytes() != template
                or any(self.check_file(p, missing=True) for p in (common['OLD_DROPIN'], common['OLD_RUN_DROPIN']))):
            rgb.stop()
            rgb.remove(common['OLD_DROPIN'])
            rgb.remove(common['OLD_RUN_DROPIN'])
            self.write_json(common['CONFIG'], projection, 0o600)
            self.atomic(rgb.unit_path, template, 0o644)
            self.systemctl('daemon-reload')
        try:
            self.ensure_started(rgb, common['DROPIN'], ['/dev/i2c-' + str(bus)])
            rgb.wait_for_worker()
        except RuntimeError:
            if not self.fake and rgb.systemctl('show', '--value', '-p', 'ExecMainStatus', rgb.unit).strip() == '77':
                self.withdraw_device(rgb, common['DROPIN'])
                return 'unsupported'
            if not self.fake:
                detail = rgb.systemctl('show', '--value', '-p', 'StatusText', rgb.unit).strip()
                if detail:
                    fail(detail)
            raise
        return 'ready'

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
        rgb = self.rgb()
        old = rgb.check_file(common['CONFIG'], missing=True)
        if old:
            binding = common['bounded_json'](old)['binding']
            rgb.args.pci, rgb.args.uuid = binding['pci'], binding['uuid']
            rgb.selected_authorization()
        if auth:
            self.write_json(CONFIG, dict(auth, enabled=False), 0o644)
        try:
            rgb.stop()
        finally:
            self.stop()
        if self.check_file(UNITS / BOOT, missing=True):
            self.systemctl('disable', '--now', BOOT)
        with rgb.lock('worker.lock'):
            if old and rgb.check_file(common['STATE'] / 'journal.json', missing=True):
                rgb.atomic(common['STATE'] / 'arkivert-tillatelse.json', old.read_bytes(), 0o600)
            for path in (common['CONFIG'], common['DROPIN'], common['OLD_DROPIN'], common['OLD_RUN_DROPIN'], rgb.unit_path):
                self.remove(path)
            self.atomic(common['INACTIVE_MARKER'], b'Inaktiv\n', 0o644)
        self.clean(auth)
        if self.check_file(self.unit_path, missing=True):
            self.systemctl('disable', UNIT)
        for path in (DROPIN, OLD_DROPIN, self.unit_path, UNITS / BOOT, CONFIG, STATUS):
            self.remove(path)
        self.systemctl('daemon-reload')


if __name__ == '__main__':
    try:
        os.umask(0o022)
        admin = Admin(parse())
        with common['system_lock'](str(admin.directory(Path('run')) / 'agentic-rigg-system.lock')):
            if getattr(admin, admin.args.action)() is False:
                sys.exit(75)
    except (RuntimeError, OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        sys.exit('NVML: ' + str(error))
