#!/usr/bin/python3
"""Ingen privilegier, vertstjenester eller enhetsnoder i disse prøvene."""
import contextlib
import fcntl
import hashlib
import io
import json
import os
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
        self.configure(Path(self.tmp.name))

    def configure(self, root):
        self.root = root
        self.mod = runpy.run_path(str(DATA / 'system-install'))['install'].__globals__
        self.assets = {self.root / path.relative_to('/'): spec for path, spec in self.mod['ASSETS'].items()}
        for key in ('LIB', 'SYSTEM', 'NV', 'RGB', 'POL', 'ETC', 'UNITS', 'RUN', 'STATE', 'MARK', 'RGB_MARK', 'GPU_INFO'):
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

    def bootstrap(self, archive, digest):
        command = (DATA / 'system-bootstrap.command').read_text()
        self.assertTrue(command.startswith('/usr/bin/sudo /usr/bin/env -i '))
        command = command.removeprefix('/usr/bin/sudo ').replace('/var/tmp/rigg.', str(self.root / 'rigg.'))
        result = subprocess.run(['/bin/bash', '-c', command], cwd=self.root, text=True,
                                input=f'{archive}\n{digest}\n', capture_output=True, check=False, timeout=5)
        self.assertFalse(list(self.root.glob('rigg.*')))
        return result

    def test_bootstrap_checks_copy_before_first_instruction(self):
        entries = [('data/hjelper/system-install', b'import sys; print("GODKJENT", sys.argv[2])')]
        archive, digest = self.archive(entries)
        for expected, ok in [(digest, True), ('0' * 64, False)]:
            result = self.bootstrap(archive, expected)
            self.assertEqual(result.returncode == 0, ok, result.stderr)
            self.assertEqual('GODKJENT ' + digest in result.stdout, ok)

    def test_bootstrap_rejects_duplicate_installer_members(self):
        entry = ('data/hjelper/system-install', b'print("GODKJENT")')
        archive, digest = self.archive([entry, entry])
        result = self.bootstrap(archive, digest)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Tvetydig systeminstallerer', result.stderr)
        self.assertNotIn('GODKJENT', result.stdout)

    def test_bootstrap_rejects_unsafe_source_before_checksum(self):
        entry = ('data/hjelper/system-install', b'print("GODKJENT")')
        archive, digest = self.archive([entry])
        link = self.root / 'link'
        link.symlink_to(archive)
        pipe = self.root / 'pipe'
        os.mkfifo(pipe)
        oversized = self.root / 'oversized'
        with oversized.open('wb') as stream:
            stream.truncate(self.mod['MAX_ARCHIVE'] + 1)
        for source, error in [(link, 'Kunne ikke åpne eller kopiere'), (pipe, 'regulær fil'),
                              (oversized, 'større enn 64 MiB')]:
            with self.subTest(source=source.name):
                result = self.bootstrap(source, digest)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error, result.stderr)
                self.assertEqual(result.stdout, '')

    def test_release_contains_all_allowlisted_assets_but_no_fixtures(self):
        package = runpy.run_path(str(DATA.parents[1] / 'scripts/pakk-utgivelse.py'))
        for name in package['BINARIES']:
            (self.root / name).write_bytes(b'fixture')
        names = {name for name, _, _ in package['file_list'](self.root)}
        self.assertTrue(self.files.keys() <= names)
        self.assertIn('data/hjelper/system-bootstrap.command', names)
        self.assertFalse(any('test-system-install' in name or 'fixtures' in Path(name).parts for name in names))

    def test_verified_bytes_not_original_path(self):
        archive, digest = self.archive(self.files.items())
        file_digest = hashlib.file_digest

        def change_original_then_hash(staged, algorithm):
            archive.write_bytes(b'endret brukerfil')
            return file_digest(staged, algorithm)

        with patch.object(hashlib, 'file_digest', change_original_then_hash):
            self.assertEqual(self.mod['approved_files'](archive, digest, self.root), self.files)
        with self.assertRaises(ValueError):
            self.mod['approved_files'](archive, digest, self.root)
        archive, digest = self.archive([*self.files.items(), next(iter(self.files.items()))])
        with self.assertRaises(ValueError):
            self.mod['approved_files'](archive, digest, self.root)

    def test_archive_source_must_be_regular_and_bounded(self):
        oversized = self.root / 'oversized'
        with oversized.open('wb') as stream:
            stream.truncate(self.mod['MAX_ARCHIVE'] + 1)
        pipe = self.root / 'pipe'
        os.mkfifo(pipe)
        link = self.root / 'link'
        link.symlink_to(oversized)
        for source, error, message in [(oversized, ValueError, 'større enn 64 MiB'),
                                       (pipe, ValueError, 'regulær fil'), (link, OSError, '')]:
            with self.subTest(source=source.name), self.assertRaisesRegex(error, message):
                self.mod['approved_files'](source, '0' * 64, self.root)

    def test_downgrade_leaves_recovery_and_active_files_untouched(self):
        marker = self.mod['MARK']
        recovery = self.mod['SYSTEM'] / 'system-install'
        config, journal, _, _ = self.mod['active_files']('nvml')
        originals = [(marker, '2:26\n'), (recovery, 'nyere installer'), (config, 'tillatelse'), (journal, 'uavklart')]
        for path, data in originals:
            self.directory(path.parent)
            path.write_text(data)
        self.mod['stopped'] = Mock(side_effect=AssertionError('Uventet tjenestekontroll'))
        with self.assertRaisesRegex(ValueError, 'nedgradere.*26.*25'):
            self.mod['install'](self.files)
        self.mod['stopped'].assert_not_called()
        self.assertEqual(self.calls, [])
        for path, data in originals:
            self.assertEqual(path.read_text(), data)

    def test_interrupted_install_preserves_journal_and_retries(self):
        self.mod['install'](self.files)
        journal = self.mod['active_files']('rgb')[1]
        self.directory(journal.parent)
        journal.write_bytes(b'previous intent')
        put = self.mod['put']

        def interrupt(path, data, mode):
            if path == self.mod['NV'] / 'nvml-admin.py':
                raise OSError('interrupted')
            put(path, data, mode)

        with patch.dict(self.mod, put=interrupt), self.assertRaisesRegex(OSError, 'interrupted'):
            self.mod['install'](self.files)
        self.assertFalse(self.mod['MARK'].exists())
        self.assertEqual(journal.read_bytes(), b'previous intent')
        self.mod['install'](self.files)
        self.assertEqual(self.mod['MARK'].read_text(), '3:25\n')
        self.assertEqual(journal.read_bytes(), b'previous intent')

    def test_boot_after_late_driver_and_serialized_revocation(self):
        self.test_migrate_stopped_build33_with_unresolved_journal()
        script = self.mod['NV'] / 'nvml-admin.py'
        args = ['--root', str(self.root), '--uid', '1000']
        def admin(action, *extra):
            return subprocess.run([sys.executable, '-I', '-S', str(script), action, *args, *extra],
                                  capture_output=True, check=False)
        grant_file = self.mod['active_files']('nvml')[0]
        grant = json.loads(grant_file.read_text())
        card_args = ['--card-control', '--pci', grant['pci'], '--uuid', grant['uuid']]
        info = self.mod['GPU_INFO'] / grant['pci'] / 'information'
        rule = self.root / 'run/systemd/system/agentic-rigg-helper.service.d/gpu.conf'
        self.assertIn('/dev/nvidia13 rw', rule.read_text())
        info.unlink()
        self.assertEqual(admin('boot').returncode, 75)
        self.assertFalse(rule.exists())
        status = self.root / 'run/agentic-rigg-card/status.json'
        self.assertEqual(json.loads(status.read_text())['rgb'], 'waiting')
        info.write_text(f"Device Minor: 7\nBus Location: {grant['pci']}\nGPU UUID: {grant['uuid']}\n")
        self.assertEqual(admin('boot').returncode, 0)
        self.assertEqual(json.loads(status.read_text())['rgb'], 'unsupported')
        self.assertIn('/dev/nvidia7 rw', rule.read_text())
        self.assertNotIn('nvidia13', rule.read_text())
        nv_state = self.root / 'fixture-nvml-systemd.json'
        nv_state.write_text(json.dumps({'ActiveState':'inactive', 'fail':{'start':'before'}}))
        self.assertEqual(admin('boot').returncode, 75)
        failed = json.loads(status.read_text())
        self.assertEqual((failed['nvml'], failed['rgb']), ('waiting', 'unsupported'))
        self.assertIn('start', failed['gpuMessage'])
        nv_state.write_text('{}')
        with (self.root / 'run/agentic-rigg-system.lock').open('rb') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            for action, extra in [('deactivate', []), ('activate', card_args)]:
                res = admin(action, *extra)
                self.assertNotEqual(res.returncode, 0)
                self.assertIn('administratorhandling', res.stderr.decode())
        self.assertEqual(admin('activate', *card_args).returncode, 0)
        self.assertEqual(json.loads(grant_file.read_text()), grant)
        rgb_config, journal, *_ = self.mod['active_files']('rgb')
        previous = json.loads(rgb_config.read_text())
        previous['binding'].update(pci='0000:02:00.0', uuid='GPU-23456789-abcd-ef01-2345-6789abcdef01')
        rgb_config.write_text(json.dumps(previous))
        revoked = admin('deactivate')
        self.assertEqual(revoked.returncode, 0, revoked.stderr.decode())
        self.assertFalse(grant_file.exists() or rgb_config.exists())
        receipt = journal.with_name('arkivert-tillatelse.json')
        self.assertEqual(json.loads(receipt.read_text()), previous)
        self.assertEqual(receipt.stat().st_mode & 0o777, 0o600)
        self.mod['archive_state']('rgb', previous['binding']['uuid'])
        self.assertFalse(journal.exists())

    def test_admin_revoke_preserves_rgb_binding_for_later_archive(self):
        self.test_migrate_stopped_build33_with_unresolved_journal()
        config, journal, *_ = self.mod['active_files']('rgb')
        original = config.read_bytes()
        self.assertTrue(self.mod['revoke_shared']())
        receipt = journal.with_name('arkivert-tillatelse.json')
        self.assertEqual(receipt.read_bytes(), original)
        self.assertEqual(receipt.stat().st_mode & 0o777, 0o600)
        card = json.loads(original)['binding']
        gpu = self.mod['GPU_INFO'] / card['pci']
        (gpu / 'information').unlink()
        gpu.rmdir()
        self.mod['archive_state']('rgb', card['uuid'])
        self.assertFalse(config.exists() or journal.exists())
        self.assertEqual(next(self.root.rglob('arkivert-tillatelse.json')).read_bytes(), original)

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
        self.assertEqual(self.mod['MARK'].read_text(), '3:25\n')
        self.assertTrue(config.exists())
        self.mod['remove_active']('nvml')
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
        self.assertFalse(journal.exists())
        self.assertTrue(config.exists())

    def test_revoke_requires_stopped_clean_domain_and_keeps_baseline(self):
        for domain in ('rgb', 'nvml'):
            config, journal, unit, dropin = self.mod['active_files'](domain)
            baseline = journal.with_name('baseline.json')
            for path in (config, journal, unit, dropin, baseline):
                self.directory(path.parent)
                path.write_text('behold')
            revoke = self.mod['archive_state']
            if domain == 'nvml':
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
        pci, uuid = '0000:01:00.0', 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
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
        info.parent.rmdir()
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

    def test_removed_card_requires_readable_full_inventory(self):
        config, journal, _, _ = self.mod['active_files']('rgb')
        uuid = 'GPU-12345678-abcd-ef01-2345-6789abcdef01'
        self.directory(config.parent)
        config.write_text(json.dumps({'binding': {'pci': '0000:01:00.0', 'uuid': uuid}}))
        config.chmod(0o600)
        self.directory(journal.parent)
        journal.write_text('uavklart')
        archive = self.mod['archive_state']
        with self.assertRaisesRegex(ValueError, 'Kunne ikke kontrollere.*GPU-oversikt'):
            archive('rgb', uuid)
        moved = self.mod['GPU_INFO'] / '0000:02:00.0' / 'information'
        self.directory(moved.parent)
        with self.assertRaisesRegex(ValueError, 'Kunne ikke kontrollere.*GPU-oversikt'):
            archive('rgb', uuid)
        for contents in ['Model: GPU\n', 'GPU UUID: Unknown\n', 'GPU UUID: GPU-OTHER\n',
                         f'GPU UUID: Unknown\nGPU UUID: {uuid}\n']:
            moved.write_text(contents)
            with self.subTest(contents=contents), self.assertRaisesRegex(ValueError, 'ingen entydig, gyldig UUID'):
                archive('rgb', uuid)
        moved.write_text(f'GPU UUID: {uuid.upper()}\n')
        with self.assertRaisesRegex(ValueError, 'Bundet kort finnes fortsatt'):
            archive('rgb', uuid)
        self.assertTrue(config.exists() and journal.exists())
        self.assertFalse((journal.parent / 'arkivert-tillatelse.json').exists())
        self.assertEqual(self.calls, [])
        moved.write_text('GPU UUID: GPU-87654321-abcd-ef01-2345-6789abcdef01\n')
        archive('rgb', uuid)
        self.assertFalse(config.exists() or journal.exists())

    def test_adapter_report_uses_only_bound_metadata_command(self):
        module = runpy.run_path(str(DATA / 'rgb-admin.py'))
        admin = object.__new__(module['Admin'])
        admin.fake, admin.uid = False, 1000
        admin.args = type('Args', (), {'pci': '0000:01:00.0', 'uuid': 'GPU-test'})()
        admin.check_file = Mock(return_value='/fixture/helper')
        metadata = dict(pci=admin.args.pci, uuid=admin.args.uuid, adapter={'busNumber': 17},
                        devicePath='/dev/i2c-17', deviceNumber={'major': 89, 'minor': 17})
        admin.command = Mock(return_value=json.dumps({'ok': True, 'report': {'metadata': metadata}}))
        self.assertEqual(admin.bus(admin.adapter_report()), 17)
        admin.command.assert_called_once_with(['/fixture/helper', '--rgb-adapter', '--pci', admin.args.pci,
                                              '--uuid', admin.args.uuid, '--uid', '1000'])
        for field, bad in [('uuid', 'other'), ('pci', '0000:01:00.1'), ('devicePath', '/dev/i2c-7'),
                           ('deviceNumber', {'major': 89, 'minor': 7})]:
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                admin.bus({'metadata': {**metadata, field: bad}})

    def test_package_omits_legacy_rgb_policy_and_privileged_entry_points(self):
        package = runpy.run_path(str(DATA.parents[1] / 'scripts/pakk-utgivelse.py'))
        for binary in package['BINARIES']:
            (self.root / binary).write_bytes(b'fixture')
        names = {Path(name).name for name, _, _ in package['file_list'](self.root)}
        for obsolete in ('rgb-aktiver.sh', 'rgb-deaktiver.sh', 'rgb-avklar.sh', 'rgb-gjenopprett.sh',
                         'io.github.AgenticRigg.rgb.v2.policy'):
            self.assertNotIn(obsolete, names)

    def test_migrate_stopped_build33_with_unresolved_journal(self):
        rgb_config, rgb_journal, rgb_unit, _ = self.mod['active_files']('rgb')
        nvml_config = self.mod['active_files']('nvml')[0]
        card = {'pci': '0000:1c:00.0', 'uuid': 'GPU-12345678-abcd-ef01-2345-6789abcdef01'}
        self.directory(self.root / 'run')
        info = self.mod['GPU_INFO'] / card['pci'] / 'information'
        self.directory(info.parent)
        info.write_text(f"GPU UUID: {card['uuid']}\nBus Location: {card['pci']}\nDevice Minor: 13\n")
        for path, data in ((rgb_config, {'version': 1, 'uid': 1000, 'enabled': True, 'binding': card, 'build': 33}),
                           (nvml_config, {'versjon': 2, 'uid': 1000, 'enabled': True, **card, 'bygg': 33}),
                           (rgb_journal, {'uavklart': True})):
            self.directory(path.parent)
            if not path.exists():
                path.write_text(json.dumps(data))
        rgb_journal.parent.chmod(0o700)
        rgb_journal.chmod(0o600)
        original = rgb_journal.read_bytes()
        rgb_config.chmod(0o600)
        nvml_config.chmod(0o644)
        self.directory(self.mod['MARK'].parent)
        self.mod['MARK'].write_text('2:33\n')
        self.mod['MARK'].chmod(0o644)
        # Pakken: alle systemfiler fra denne kildekoden, bygg 34.
        files = dict(self.files)
        files['version.json'] = json.dumps({'name': 'Agentic HW Monitor', 'version': '1.7.1', 'channel': 'stable', 'build': 34}).encode()
        archive, digest = self.archive(files.items())
        hent = runpy.run_path(str(DATA / 'system-hent.py'), run_name='test')['main'].__globals__
        hent.update(STAGING=str(self.root), MARK=str(self.mod['MARK']), RGB_JOURNAL=str(rgb_journal),
                    CONFIG={'nvml': str(nvml_config), 'rgb': str(rgb_config)}, OWNER=os.getuid(),
                    LOCK=str(self.root / 'run/agentic-rigg-system.lock'), published_digest=lambda *_: digest,
                    verified_installer=lambda *_: (DATA / 'system-install').read_bytes())

        def fetch(url, limit, out=None, hosts=()):
            out.write(archive.read_bytes())
            return digest

        ran = []

        def run(argv, **kwargs):
            ran.append([Path(a).name if a.startswith('/') else a for a in argv])
            if argv[:3] == ['/usr/bin/python3', '-I', '-S']:
                # Installasjonsprogrammet med nøyaktig argumentene fra hentingen.
                args = argv[4:]
                self.mod['install'](self.mod['approved_files'](*args, staging=str(self.root)))
            elif argv[0].endswith('/aktiver.sh'):
                # aktiver.sh → nvml-admin.py: den felles koden skal være den nye kopien.
                admin = self.mod['NV'] / 'nvml-admin.py'
                common = admin.with_name('rgb-admin.py')
                self.assertEqual(common.read_bytes(), self.files['data/hjelper/rgb-admin.py'])
                return subprocess.run([sys.executable, '-I', '-S', str(admin), 'activate',
                                       '--root', str(self.root), '--uid', '1000', *argv[1:]],
                                      env=kwargs['env'], pass_fds=kwargs['pass_fds'], check=True)
            else:
                self.fail(f'Uventet kommando: {argv}')
            return type('R', (), {'returncode': 0})()

        class RootOs:
            """Bare hentingen tror den kjører som root; installasjonens egne
            eierskapskontroller ser den ekte brukeren som eier testroten."""
            def __getattr__(self, name):
                return getattr(os, name)

            def geteuid(self):
                return 0

        hent.update(fetch=fetch, os=RootOs(), subprocess=type('S', (), {'run': run, 'DEVNULL': -3, 'PIPE': -1}))
        out = io.StringIO()
        with patch.dict(os.environ, {'PKEXEC_UID': '1000'}), contextlib.redirect_stdout(out):
            self.assertEqual(hent['main'](['1.7.1', '34', '-', 'card', card['pci'], card['uuid']]), 0)
        self.assertEqual([r[0] if isinstance(r, list) else r for r in ran], ['python3', 'aktiver.sh'])
        self.assertEqual(rgb_journal.read_bytes(), original)
        self.assertTrue(rgb_config.exists())
        self.assertEqual(self.mod['MARK'].read_text(), '3:34\n')

    def lock_functions(self):
        rgb = runpy.run_path(str(DATA / 'rgb-admin.py'))
        return {'system-install': self.mod['system_lock'], 'rgb-admin.py': rgb['system_lock']}

    def test_arvet_laas_godtas_bare_for_samme_holdte_laasefil(self):
        lock = self.root / 'system.lock'
        other = self.root / 'annen.lock'
        other.write_bytes(b'')
        for name, system_lock in self.lock_functions().items():
            errors = (ValueError, RuntimeError)
            holder = os.open(lock, os.O_WRONLY | os.O_CREAT, 0o600)
            try:
                fcntl.flock(holder, fcntl.LOCK_EX)
                # Uten arv: en annen administratorhandling holder låsen.
                with patch.dict(os.environ, {}, clear=False):
                    os.environ.pop('AGENTIC_RIGG_SYSTEM_LOCK_FD', None)
                    with self.assertRaisesRegex(errors, 'annen administratorhandling'), system_lock(lock):
                        pass
                # Arvet: samme åpne fil, så handlingen kjører under låsen.
                ran = []
                with patch.dict(os.environ, {'AGENTIC_RIGG_SYSTEM_LOCK_FD': str(holder)}), system_lock(lock):
                    ran.append(name)
                self.assertEqual(ran, [name])
                # Låsen er fortsatt forelderens etterpå (ingen LOCK_UN).
                probe = os.open(lock, os.O_WRONLY)
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
                # Et eget filhåndtak til låsefila mens en annen holder den.
                with patch.dict(os.environ, {'AGENTIC_RIGG_SYSTEM_LOCK_FD': str(probe)}):
                    with self.assertRaisesRegex(errors, 'annen administratorhandling'), system_lock(lock):
                        pass
                os.close(probe)
                # En annen fil eller et ugyldig tall gir aldri låsen.
                wrong = os.open(other, os.O_RDONLY)
                for value in (str(wrong), '0', '2', '-3', 'x', '99999999'):
                    with patch.dict(os.environ, {'AGENTIC_RIGG_SYSTEM_LOCK_FD': value}):
                        with self.assertRaises((*errors, OSError)), system_lock(lock):
                            self.fail(f'{name} godtok {value}')
                os.close(wrong)
            finally:
                os.close(holder)

    def test_barneprosess_arver_laasen_bare_med_filhandtaket(self):
        lock = self.root / 'system.lock'
        child = ('import runpy, sys\n'
                 'lock = runpy.run_path(sys.argv[1], run_name="test")["system_lock"]\n'
                 'with lock(sys.argv[2]):\n    print("UNDER LÅS")\n')
        holder = os.open(lock, os.O_WRONLY | os.O_CREAT, 0o600)
        try:
            fcntl.flock(holder, fcntl.LOCK_EX)
            for script in ('rgb-admin.py', 'system-install'):
                env = {'PATH': '/usr/bin:/bin', 'AGENTIC_RIGG_SYSTEM_LOCK_FD': str(holder)}
                inherited = subprocess.run([sys.executable, '-I', '-S', '-c', child, str(DATA / script), str(lock)],
                                           env=env, pass_fds=(holder,), capture_output=True, text=True, timeout=10)
                self.assertEqual(inherited.stdout, 'UNDER LÅS\n', inherited.stderr)
                # Uten det arvede håndtaket (som de gamle skriptene) avvises handlingen.
                alone = subprocess.run([sys.executable, '-I', '-S', '-c', child, str(DATA / script), str(lock)],
                                       env={'PATH': '/usr/bin:/bin'}, capture_output=True, text=True, timeout=10)
                self.assertNotEqual(alone.returncode, 0)
                self.assertIn('annen administratorhandling', alone.stderr)
                self.assertEqual(alone.stdout, '')
        finally:
            os.close(holder)

if __name__ == '__main__':
    # Testrøttene skal ha samme katalogmoduser som den privilegerte installereren.
    os.umask(0o022)
    if len(sys.argv) == 3 and sys.argv[1] == '--migrate-fixture':
        root = runpy.run_path(str(DATA / 'rgb-admin.py'))['fixture_root'](Path(sys.argv[2]))
        test = SystemTest()
        test.configure(root)
        test.test_migrate_stopped_build33_with_unresolved_journal()
    else:
        unittest.main()
