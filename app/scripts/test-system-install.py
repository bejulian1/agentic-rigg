#!/usr/bin/python3
"""Ingen privilegier, vertstjenester eller enhetsnoder i disse prøvene."""
import fcntl
import hashlib
import io
import json
from pathlib import Path
import runpy
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

DATA = Path(__file__).resolve().parents[1] / 'data/hjelper'


class SystemTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir='/tmp')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.mod = runpy.run_path(str(DATA / 'system-install'))['install'].__globals__
        self.assets = {self.root / path.relative_to('/'): spec for path, spec in self.mod['ASSETS'].items()}
        for key in ('LIB', 'SYSTEM', 'NV', 'RGB', 'POL', 'ETC', 'UNITS', 'RUN', 'STATE', 'MARK', 'GPU_INFO'):
            self.mod[key] = self.root / self.mod[key].relative_to('/')
        self.calls = []
        self.mod.update(ASSETS=self.assets, ctl=lambda *args: self.calls.append(args),
                        stopped=lambda _: True, directory=self.directory)
        self.files = {key: (DATA / Path(key).name).read_bytes() if not key.startswith('bin/') else b'fixture'
                      for key, _ in self.assets.values()}

    def directory(self, path):
        self.assertTrue(path.is_relative_to(self.root))
        path.mkdir(parents=True, exist_ok=True)

    def archive(self, files):
        archive = self.root / 'fil med mellomrom.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for key, data in files:
                member = tarfile.TarInfo('agentic-rigg-test/' + key)
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
        return archive, hashlib.sha256(archive.read_bytes()).hexdigest()

    def test_bootstrap_checks_copy_before_first_instruction(self):
        entries = [('data/hjelper/system-install', b'import sys; print("GODKJENT", sys.argv[2])')]
        archive, digest = self.archive(entries)
        command = (DATA / 'system-bootstrap.command').read_text()
        self.assertTrue(command.startswith('/usr/bin/sudo /usr/bin/env -i '))
        command = command.removeprefix('/usr/bin/sudo ').replace('/var/tmp/rigg.', str(self.root / 'rigg.'))
        for expected, ok in [(digest, True), ('0' * 64, False)]:
            result = subprocess.run(['/bin/bash', '-c', command], cwd=self.root, text=True,
                                    input=f'{archive}\n{expected}\n', capture_output=True, check=False)
            self.assertEqual(result.returncode == 0, ok, result.stderr)
            self.assertEqual('GODKJENT ' + digest in result.stdout, ok)
        self.assertFalse(list(self.root.glob('rigg.*')))

    def test_release_contains_all_allowlisted_assets_but_no_fixtures(self):
        package = runpy.run_path(str(DATA.parents[1] / 'scripts/pakk-utgivelse.py'))
        for name in package['BINARIES']:
            (self.root / name).write_bytes(b'fixture')
        names = {name for name, _, _ in package['file_list'](self.root)}
        self.assertTrue(self.files.keys() <= names)
        self.assertIn('data/hjelper/system-bootstrap.command', names)
        self.assertFalse(any('test-system-install' in name for name in names))

    def test_verified_bytes_not_original_path(self):
        archive, digest = self.archive(self.files.items())
        copyfile = self.mod['shutil'].copyfile

        def copy_then_change(source, destination):
            copyfile(source, destination)
            archive.write_bytes(b'endret brukerfil')

        with patch.object(self.mod['shutil'], 'copyfile', copy_then_change):
            self.assertEqual(self.mod['approved_files'](archive, digest, self.root), self.files)
        with self.assertRaises(ValueError):
            self.mod['approved_files'](archive, digest, self.root)
        archive, digest = self.archive([*self.files.items(), next(iter(self.files.items()))])
        with self.assertRaises(ValueError):
            self.mod['approved_files'](archive, digest, self.root)

    def test_migration_and_uninstall_keep_baseline(self):
        config, journal, unit, _ = self.mod['active_files']('nvml')
        for path in (config, unit, journal):
            self.directory(path.parent)
            path.write_text('{}')
        with self.assertRaises(ValueError):
            self.mod['install'](self.files)
        self.assertTrue(config.exists() and unit.exists() and journal.exists())
        journal.unlink()
        baseline = journal.with_name('baseline.json')
        baseline.write_text('opptak')
        self.mod['install'](self.files)
        self.assertFalse(config.exists() or unit.exists())
        for path, (key, mode) in self.assets.items():
            self.assertEqual(path.read_bytes(), self.files[key])
            self.assertEqual(path.stat().st_mode & 0o777, mode)
        self.mod['uninstall']()
        self.assertFalse(any(path.exists() for path in self.assets))
        self.assertEqual(next(self.root.rglob('baseline.json')).read_text(), 'opptak')

    def test_removed_card_needs_stopped_worker_lock_and_bound_uuid(self):
        config, journal, unit, _ = self.mod['active_files']('rgb')
        pci, uuid = '0000:01:00.0', 'GPU-TEST'
        for path, text in ((config, json.dumps({'binding': {'pci': pci, 'uuid': uuid}})),
                           (journal, 'uavklart'), (unit, 'fixture')):
            self.directory(path.parent)
            path.write_text(text)
            path.chmod(0o600)
        archive = self.mod['archive_state']
        with self.assertRaises(ValueError):
            self.mod['uninstall']()
        with self.assertRaises(ValueError):
            archive('rgb', 'feil kort')
        info = self.mod['GPU_INFO'] / pci / 'information'
        self.directory(info.parent)
        info.write_text('GPU UUID: ' + uuid)
        with self.assertRaises(ValueError):
            archive('rgb', uuid)
        info.unlink()
        with (journal.parent / 'worker.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            with self.assertRaises(BlockingIOError):
                archive('rgb', uuid)
        self.mod['stopped'] = lambda _: False
        with self.assertRaises(ValueError):
            archive('rgb', uuid)
        self.assertTrue(config.exists() and journal.exists())
        self.mod['stopped'] = lambda _: True
        archive('rgb', uuid)
        self.assertFalse(config.exists() or journal.exists() or unit.exists())
        archived = next(self.root.rglob('journal.json'))
        self.assertEqual(archived.read_text(), 'uavklart')
        self.assertEqual(json.loads(archived.with_name('arkivert-tillatelse.json').read_text())['binding']['uuid'], uuid)

    def test_admin_calls_get_only_validated_bus_and_worker_properties(self):
        module = runpy.run_path(str(DATA / 'rgb-admin.py'))
        admin = object.__new__(module['Admin'])
        admin.fake = False
        admin.args = type('Args', (), {'pci': '0000:01:00.0', 'uuid': 'GPU-test'})()
        admin.uid = 1000
        admin.unit_template = lambda: (DATA / module['UNIT']).read_bytes()
        admin.directory = lambda _: None
        argv = []

        def command(args):
            argv.append(args)
            return json.dumps({'ok': True, 'report': {'metadata': {'pci': admin.args.pci, 'uuid': admin.args.uuid,
                              'adapter': {'busNumber': 17}, 'devicePath': '/dev/i2c-17', 'deviceNumber': {'major': 89, 'minor': 17}}}})

        admin.command = command
        for flag in ('--rgb-forbered', '--rgb-avklar', '--rgb-gjenopprett', '--rgb-avklaringsmal'):
            admin.helper('/fixture/helper', flag, stage=str(self.root))
            probe, action = argv[-2:]
            self.assertEqual(probe[:2], ['/fixture/helper', '--rgb-adapter'])
            self.assertEqual(action[:4], ['/usr/bin/systemd-run', '--wait', '--pipe', '--collect'])
            self.assertEqual([p for p in action if 'DeviceAllow=' in p], ['--property=DeviceAllow=/dev/i2c-17 rw'])
            for value in ('CapabilityBoundingSet=CAP_CHOWN', 'ProtectProc=invisible', 'DevicePolicy=closed', 'NoNewPrivileges=yes'):
                self.assertIn('--property=' + value, action)
            if flag == '--rgb-forbered':
                self.assertIn(f'--property=BindPaths={self.root}:/etc/agentic-rigg', action)
            self.assertEqual(action[action.index('--') + 1:][:2], ['/fixture/helper', flag])

    def test_prepare_commits_only_matching_staged_authorization(self):
        module = runpy.run_path(str(DATA / 'rgb-admin.py'))
        admin = object.__new__(module['Admin'])
        admin.root, admin.fake, admin.uid = self.root, False, 1000
        admin.args = type('Args', (), {'pci': '0000:01:00.0', 'uuid': 'GPU-test'})()
        binding = {'pci': admin.args.pci, 'uuid': admin.args.uuid}
        auth = {'uid': admin.uid, 'binding': binding}
        other = module['CONFIG'].with_name('helper.json')
        admin.atomic(other, b'NVML', 0o644)
        admin.atomic(module['CONFIG'], b'{}', 0o600)
        staged = {}

        def prepare(_, flag, *args, stage):
            stage = Path(stage)
            self.assertTrue(stage.is_relative_to(self.root / 'run'))
            self.assertFalse((stage / other.name).exists())
            permission = stage / module['CONFIG'].name
            self.assertEqual(permission.read_bytes(), b'{}')
            permission.write_text(json.dumps(staged))
            return {'baseline': {'binding': binding}, 'authorization': auth, 'metadata': {
                **binding, 'adapter': {'busNumber': 17}, 'devicePath': '/dev/i2c-17',
                'deviceNumber': {'major': 89, 'minor': 17}}}

        admin.helper = prepare
        with self.assertRaises(RuntimeError):
            admin.prepare('/fixture/helper')
        self.assertEqual(admin.path(module['CONFIG']).read_bytes(), b'{}')
        staged.update(auth)
        admin.prepare('/fixture/helper')
        self.assertEqual(json.loads(admin.path(module['CONFIG']).read_text()), auth)
        self.assertEqual(admin.path(other).read_bytes(), b'NVML')
        self.assertFalse(list((self.root / 'run').iterdir()))


if __name__ == '__main__':
    unittest.main()
