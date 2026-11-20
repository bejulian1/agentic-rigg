#!/usr/bin/python3
"""Ingen privilegier, vertstjenester eller enhetsnoder i disse prøvene."""
import fcntl
import hashlib
import io
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import Mock, patch

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
        self.files['version.json'] = b'{"build":25}'
        self.runner = Mock(side_effect=AssertionError('Uventet systemd-run'))
        self.mod['subprocess'] = Mock(wraps=subprocess, run=self.runner, CalledProcessError=subprocess.CalledProcessError,
                                      SubprocessError=subprocess.SubprocessError, DEVNULL=subprocess.DEVNULL)

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
        self.assertEqual(self.mod['MARK'].read_text(), '2:25\n')
        self.assertFalse(config.exists() or unit.exists())
        for path, (key, mode) in self.assets.items():
            self.assertEqual(path.read_bytes(), self.files[key])
            self.assertEqual(path.stat().st_mode & 0o777, mode)
        self.mod['uninstall']()
        self.assertFalse(any(path.exists() for path in self.assets))
        self.assertEqual(next(self.root.rglob('baseline.json')).read_text(), 'opptak')

    def test_failed_stop_retries_once_in_bound_sandbox_before_migration(self):
        config, journal, unit, _ = self.mod['active_files']('nvml')
        pci, uuid = '0000:01:00.0', 'GPU-fixture'
        binary = self.mod['LIB'] / 'agentic-rigg-helper'
        info = self.mod['GPU_INFO'] / pci / 'information'
        for path, text in ((config, json.dumps(dict(pci=pci, uuid=uuid))), (binary, 'original'), (journal, 'dirty'),
                           (info, f'Device Minor: 13\nGPU UUID: {uuid}')):
            self.directory(path.parent)
            path.write_text(text)
            path.chmod(0o644)
        active = True

        def ctl(*args):
            nonlocal active
            if args == ('stop', unit.name):
                active = False
                raise subprocess.CalledProcessError(1, 'fake stop')

        self.mod.update(ctl=ctl, stopped=lambda name: name != unit.name or not active)

        def restore(argv, **kwargs):
            self.assertFalse(active)
            self.assertEqual(binary.read_text(), 'original')
            self.assertEqual(argv[0:3], ['/usr/bin/systemd-run', '--wait', '--pipe'])
            self.assertEqual(argv[-5:], ['--', str(binary), '--backend', 'nvml', '--tilbakestill'])
            self.assertCountEqual([p for p in argv if 'DeviceAllow=' in p],
                                  ['--property=DeviceAllow=/dev/nvidiactl rw', '--property=DeviceAllow=/dev/nvidia13 rw'])
            for prop in ('DevicePolicy=closed', 'ProtectProc=invisible', 'CapabilityBoundingSet=CAP_CHOWN CAP_SYS_ADMIN'):
                self.assertIn('--property=' + prop, argv)
            journal.unlink()
            return subprocess.CompletedProcess(argv, 0)

        self.runner.side_effect = None
        self.runner.return_value = subprocess.CompletedProcess([], 1, '', 'Unknown property')
        with self.assertRaisesRegex(ValueError, 'Unknown property'):
            self.mod['install'](self.files)
        self.runner.assert_called_once()
        self.assertTrue(journal.exists() and config.exists())
        self.runner.reset_mock()
        self.runner.return_value = subprocess.CompletedProcess([], 0)
        with self.assertRaisesRegex(ValueError, 'Endringsjournalen finnes fortsatt'):
            self.mod['install'](self.files)
        self.runner.assert_called_once()
        self.runner.reset_mock()
        info.write_text('Device Minor: 13\nGPU UUID: GPU-annet')
        with self.assertRaisesRegex(ValueError, '--backend nvml --tilbakestill'):
            self.mod['install'](self.files)
        self.runner.assert_not_called()
        info.write_text(f'Device Minor: 13\nGPU UUID: {uuid}')
        self.runner.side_effect = restore
        self.mod['install'](self.files)
        self.runner.assert_called_once()
        self.assertFalse(journal.exists() or config.exists())

    def test_revoke_requires_stopped_clean_domain_and_keeps_baseline(self):
        for domain in ('rgb', 'nvml'):
            config, journal, unit, dropin = self.mod['active_files'](domain)
            baseline = journal.with_name('baseline.json')
            for path in (config, journal, unit, dropin, baseline):
                self.directory(path.parent)
                path.write_text('behold')
            revoke = self.mod['archive_state']
            with self.assertRaises(ValueError):
                revoke(domain, revoke=True)
            journal.unlink()
            self.mod['stopped'] = lambda _: False
            with self.assertRaises(ValueError):
                revoke(domain, revoke=True)
            self.mod['stopped'] = lambda _: True
            if domain == 'rgb':
                with (journal.parent / 'worker.lock').open('a') as lock:
                    fcntl.flock(lock, fcntl.LOCK_EX)
                    with self.assertRaises(BlockingIOError):
                        revoke(domain, revoke=True)
            self.assertTrue(config.exists() and unit.exists() and dropin.exists())
            revoke(domain, revoke=True)
            self.assertFalse(config.exists() or unit.exists() or dropin.exists())
            self.assertEqual(baseline.read_text(), 'behold')

    def test_revoke_cli_needs_root_and_explains_busy_lock(self):
        os = self.mod['os']
        lock = self.root / 'system.lock'
        with patch.object(sys, 'argv', ['system-install', '--revoke', 'rgb']), patch.object(sys, 'stderr', io.StringIO()):
            with patch.object(os, 'geteuid', return_value=1000), patch.object(os, 'open', side_effect=AssertionError):
                with self.assertRaises(SystemExit):
                    self.mod['main']()
            with lock.open('wb') as owner:
                fcntl.flock(owner, fcntl.LOCK_EX)
                fd = os.open(lock, os.O_WRONLY)
                with patch.object(os, 'geteuid', return_value=0), patch.object(os, 'open', return_value=fd), patch.object(os, 'umask'):
                    with self.assertRaisesRegex(ValueError, 'annen administratorhandling'):
                        self.mod['main']()

    def test_user_uninstall_reports_inactive_root_leftovers_but_succeeds(self):
        leftover = self.root / 'usr/local/libexec/agentic-rigg-system'
        self.directory(leftover)
        home = self.root / 'home'
        self.directory(home)
        answer = subprocess.run(['/bin/bash', str(DATA.parents[1] / 'uninstall.sh')], capture_output=True, text=True,
                                env={'PATH': '/usr/bin:/bin', 'HOME': str(home), 'RIGG_INSTALL_NO_SYSTEMD': '1',
                                     'RIGG_HJELPER_ROT': str(self.root)})
        self.assertEqual(answer.returncode, 0, answer.stderr)
        self.assertIn('OBS:', answer.stderr)
        self.assertIn('sudo /usr/local/libexec/agentic-rigg-system/system-install --uninstall', answer.stderr)
        self.assertTrue(leftover.is_dir())

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
        probe = command(['/fixture/helper', '--rgb-adapter'])
        admin.command = Mock(side_effect=[probe, RuntimeError('Unknown property ProtectProc')])
        with self.assertRaisesRegex(RuntimeError, r'systemd-run.*Unknown property ProtectProc.*rgb-admin.service'):
            admin.helper('/fixture/helper', '--rgb-forbered', stage=str(self.root))
        self.assertEqual(admin.command.call_count, 2)

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
