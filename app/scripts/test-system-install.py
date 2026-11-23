#!/usr/bin/python3
"""Ingen privilegier, vertstjenester eller enhetsnoder i disse prøvene."""
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
        self.root = Path(self.tmp.name)
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
        self.assertFalse(any('test-system-install' in name for name in names))

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

    def rgb_part(self):
        """En installert, aktiv RGB-del fra et eldre bygg med uavklart journal."""
        rgb = {path: b'gammel ' + path.name.encode() for path in self.assets if self.mod['rgb_asset'](path)}
        for path, data in rgb.items():
            self.directory(path.parent)
            path.write_bytes(data)
            path.chmod(0o644)
        config, journal, unit, dropin = self.mod['active_files']('rgb')
        active = {config: b'{"tillatelse": 1}', journal: b'{"uavklart": true}', unit: b'[Unit]\n', dropin: b'[Service]\n'}
        for path, data in active.items():
            self.directory(path.parent)
            path.write_bytes(data)
        return {**rgb, **active}

    def test_keep_rgb_bytter_bare_nvml_delen(self):
        # 1.6.0-feilen: GPU-oppdateringen skal ikke kreve at RGB-økten avklares.
        kept = self.rgb_part()
        self.directory(self.mod['MARK'].parent)
        self.mod['MARK'].write_text('2:24\n')
        nvml_config, _, nvml_unit, _ = self.mod['active_files']('nvml')
        for path in (nvml_config, nvml_unit):
            self.directory(path.parent)
            path.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'Avklar og slå av eksisterende RGB-tilgang'):
            self.mod['install'](self.files)
        self.mod['install'](self.files, keep_rgb=True)
        for path, data in kept.items():
            self.assertEqual(path.read_bytes(), data, path)
        self.assertEqual(self.mod['MARK'].read_text(), '2:25\n')
        self.assertEqual(self.mod['RGB_MARK'].read_text(), '2:24\n')
        self.assertFalse(nvml_config.exists() or nvml_unit.exists())
        for path, (key, mode) in self.assets.items():
            if not self.mod['rgb_asset'](path):
                self.assertEqual(path.read_bytes(), self.files[key])
                self.assertEqual(path.stat().st_mode & 0o777, mode)
        # nvml-admin.py laster den felles koden fra sin egen, nye kopi, ikke
        # fra den beholdte (eldre) RGB-mappa.
        nv_common = self.mod['NV'] / 'rgb-admin.py'
        self.assertEqual(nv_common.read_bytes(), self.files['data/hjelper/rgb-admin.py'])
        self.assertNotEqual((self.mod['RGB'] / 'rgb-admin.py').read_bytes(), nv_common.read_bytes())
        self.assertIn("with_name('rgb-admin.py')", (DATA / 'nvml-admin.py').read_text())
        self.assertNotIn('agentic-rigg-rgb-v2', (DATA / 'nvml-admin.py').read_text())
        self.assertNotIn(('stop', 'agentic-rigg-rgb.service'), self.calls)
        # En ny GPU-oppdatering beholder RGB-delens eget bygg.
        self.files['version.json'] = b'{"build":26}'
        self.mod['install'](self.files, keep_rgb=True)
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:26\n', '2:24\n'))
        # Full installasjon (etter avklaring og deaktivering) gir én felles revisjon.
        for path in (self.mod['ETC'] / 'rgb.json', self.mod['active_files']('rgb')[1]):
            path.unlink()
        self.mod['install'](self.files)
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:26\n', '2:26\n'))
        for path, (key, _) in self.assets.items():
            self.assertEqual(path.read_bytes(), self.files[key])
        self.mod['uninstall']()
        self.assertFalse(self.mod['RGB_MARK'].exists() or self.mod['MARK'].exists())

    def test_keep_rgb_med_ufullstendig_rgb_del_stopper_ikke_gpu(self):
        """En ufullstendig RGB-del eller ukjent bygg stopper ikke GPU-oppdateringen:
        RGB-filene står urørt og merkes med bygg 0, så appen krever RGB-oppdatering."""
        nvml_config = self.mod['active_files']('nvml')[0]
        for broken in ('mangler', 'ukjent bygg'):
            with self.subTest(broken=broken):
                kept = self.rgb_part()
                self.directory(self.mod['MARK'].parent)
                self.mod['MARK'].write_text('1\n' if broken == 'ukjent bygg' else '2:24\n')
                if broken == 'mangler':
                    missing = next(path for path in kept if path.name == 'rgb-admin.py')
                    missing.unlink()
                    del kept[missing]
                self.directory(nvml_config.parent)
                nvml_config.write_text('{}')
                self.mod['install'](self.files, keep_rgb=True)
                self.assertEqual(self.mod['RGB_MARK'].read_text(), '2:0\n')
                self.assertEqual(self.mod['MARK'].read_text(), '2:25\n')
                for path, data in kept.items():
                    self.assertEqual(path.read_bytes(), data, path)
                self.assertFalse(nvml_config.exists(), 'GPU-delen er installert')
                self.mod['RGB_MARK'].unlink()
        # En RGB-del fra et nyere bygg enn pakken er en nedgradering: ingenting byttes.
        kept = self.rgb_part()
        self.mod['MARK'].write_text('2:25\n')
        self.mod['RGB_MARK'].write_text('2:99\n')
        nvml_config.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'nyere bygg'):
            self.mod['install'](self.files, keep_rgb=True)
        self.assertTrue(nvml_config.exists())
        for path, data in kept.items():
            self.assertEqual(path.read_bytes(), data)

    def test_keep_nvml_bytter_bare_rgb_delen(self):
        """RGB-lys med en aktiv GPU-del fra samme utgivelse: GPU-hjelperen
        stoppes ikke, tillatelsen og filene står, og RGB-delen oppdateres."""
        nvml = {path: b'gpu ' + path.name.encode() for path in self.assets if self.mod['nvml_asset'](path)}
        for path, data in nvml.items():
            self.directory(path.parent)
            path.write_bytes(data)
            path.chmod(0o644)
        config, journal, unit, dropin = self.mod['active_files']('nvml')
        for path in (config, unit, dropin):
            self.directory(path.parent)
            path.write_text('{"aktiv": true}')
        self.directory(self.mod['MARK'].parent)
        self.mod['MARK'].write_text('2:25\n')
        self.mod['RGB_MARK'].write_text('2:24\n')
        self.mod['stopped'] = Mock(side_effect=lambda unit: unit != 'agentic-rigg-helper.service')
        self.mod['install'](self.files, keep_nvml=True)
        for path, data in nvml.items():
            self.assertEqual(path.read_bytes(), data, path)
        self.assertTrue(config.exists() and unit.exists() and dropin.exists())
        self.assertNotIn(('stop', 'agentic-rigg-helper.service'), self.calls)
        self.assertNotIn(('disable', 'agentic-rigg-helper.service'), self.calls)
        for path, (key, _) in self.assets.items():
            if not self.mod['nvml_asset'](path):
                self.assertEqual(path.read_bytes(), self.files[key], path)
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:25\n', '2:25\n'))
        # Bare når GPU-delen er fra nøyaktig samme bygg; ellers byttes ingenting.
        self.mod['MARK'].write_text('2:24\n')
        with self.assertRaisesRegex(ValueError, 'annet bygg'):
            self.mod['install'](self.files, keep_nvml=True)
        self.assertEqual(self.mod['MARK'].read_text(), '2:24\n')
        with self.assertRaisesRegex(ValueError, 'Bare én'):
            self.mod['install'](self.files, keep_rgb=True, keep_nvml=True)

    def test_keep_rgb_argumentet_fra_hentingen(self):
        """system-hent.py kjører `installer --keep-rgb|--keep-nvml ARKIV SUM`; bare installasjon godtar flaggene."""
        import contextlib
        seen = []
        self.mod.update(system_lock=contextlib.nullcontext, approved_files=lambda *values: values,
                        install=lambda files, keep_rgb=False, keep_nvml=False: seen.append((files, keep_rgb, keep_nvml)))
        with patch('os.geteuid', return_value=0), patch('os.umask'):
            for argv, expected in ((['--keep-rgb', 'arkiv', 'sum'], (('arkiv', 'sum'), True, False)),
                                   (['--keep-nvml', 'arkiv', 'sum'], (('arkiv', 'sum'), False, True)),
                                   (['arkiv', 'sum'], (('arkiv', 'sum'), False, False))):
                with patch.object(sys, 'argv', ['system-install', *argv]):
                    self.mod['main']()
                self.assertEqual(seen.pop(), expected)
            for argv in (['--keep-rgb', '--uninstall'], ['--keep-rgb', '--revoke', 'rgb'], ['--keep-rgb'],
                         ['--keep-nvml', '--uninstall'], ['--keep-rgb', '--keep-nvml', 'arkiv', 'sum']):
                with patch.object(sys, 'argv', ['system-install', *argv]), \
                        patch('sys.stderr', io.StringIO()), self.assertRaises(SystemExit):
                    self.mod['main']()
        self.assertEqual(seen, [])
        hent = (DATA / 'system-hent.py').read_text()
        self.assertIn("[*INSTALLER, installer, *keep, archive, digest]", hent)

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


    # --- Brukerens tilstand etter 1.6.0: GPU-oppdatering med uavklart RGB ---

    def test_kjede_hent_installer_og_nvml_laster_ny_felles_kode(self):
        """system-hent.py (ekte main) → system-install (ekte install i en
        privat rot) → nvml-admin.py, for en bruker med GPU-tillatelse fra bygg
        31, aktiv RGB v2 og en uavklart RGB-journal. Ingen nett, root eller
        systemd: nedlasting, pkexec-UID og tjenestekontroll er falske."""
        import contextlib
        # Den eldre RGB-delen som skal stå: tillatelse, journal og gamle skript.
        old = {path: b'# gammel 1.5.6\n' + path.name.encode() for path in self.assets if self.mod['rgb_asset'](path)}
        for path, data in old.items():
            self.directory(path.parent)
            path.write_bytes(data)
            path.chmod(0o644)
        rgb_config, rgb_journal, rgb_unit, _ = self.mod['active_files']('rgb')
        nvml_config = self.mod['active_files']('nvml')[0]
        card = {'pci': '0000:1c:00.0', 'uuid': 'GPU-12345678-abcd-ef01-2345-6789abcdef01'}
        for path, data in ((rgb_config, {'version': 1, 'uid': 1000, 'enabled': True, 'binding': card, 'build': 31}),
                           (nvml_config, {'versjon': 2, 'uid': 1000, 'enabled': True, **card, 'bygg': 31}),
                           (rgb_journal, {'uavklart': True})):
            self.directory(path.parent)
            path.write_text(json.dumps(data))
        rgb_config.chmod(0o600)
        nvml_config.chmod(0o644)
        self.directory(self.mod['MARK'].parent)
        self.mod['MARK'].write_text('2:31\n')
        self.mod['MARK'].chmod(0o644)
        # Pakken: alle systemfiler fra denne kildekoden, bygg 33.
        files = dict(self.files)
        files['version.json'] = json.dumps({'name': 'Agentic HW Monitor', 'version': '1.6.1', 'channel': 'stable', 'build': 33}).encode()
        archive, digest = self.archive(files.items())
        hent = runpy.run_path(str(DATA / 'system-hent.py'), run_name='test')['main'].__globals__
        hent.update(STAGING=str(self.root), MARK=str(self.mod['MARK']), RGB_JOURNAL=str(rgb_journal),
                    CONFIG={'nvml': str(nvml_config), 'rgb': str(rgb_config)}, OWNER=os.getuid(),
                    LOCK=str(self.root / 'system.lock'), published_digest=lambda *_: digest,
                    verified_installer=lambda *_: (DATA / 'system-install').read_bytes(),
                    confirmation=lambda *_: self.fail('Ingen RGB-avklaring skal spørres om'))

        def fetch(url, limit, out=None, hosts=()):
            out.write(archive.read_bytes())
            return digest

        ran = []

        def run(argv, **kwargs):
            ran.append([Path(a).name if a.startswith('/') else a for a in argv])
            if argv[:3] == ['/usr/bin/python3', '-I', '-S']:
                # Installasjonsprogrammet med nøyaktig argumentene fra hentingen.
                args = argv[4:]
                keep = '--keep-rgb' in args
                values = [a for a in args if a != '--keep-rgb']
                self.mod['install'](self.mod['approved_files'](*values, staging=str(self.root)), keep_rgb=keep)
            elif argv[0].endswith('/aktiver.sh'):
                # aktiver.sh → nvml-admin.py: den felles koden skal være den nye kopien.
                admin = self.mod['NV'] / 'nvml-admin.py'
                common = admin.with_name('rgb-admin.py')
                self.assertEqual(common.read_bytes(), self.files['data/hjelper/rgb-admin.py'])
                lock = runpy.run_path(str(common), run_name='test')['system_lock']
                with patch.dict(os.environ, {'AGENTIC_RIGG_SYSTEM_LOCK_FD': kwargs['env']['AGENTIC_RIGG_SYSTEM_LOCK_FD']}):
                    with lock(str(self.root / 'system.lock')):
                        ran.append('aktivert under arvet lås')
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
            self.assertEqual(hent['main'](['1.6.1', '33', '-', 'nvml', card['pci'], card['uuid']]), 0)
        self.assertEqual([r[0] if isinstance(r, list) else r for r in ran], ['python3', 'aktiver.sh', 'aktivert under arvet lås'])
        self.assertIn('--keep-rgb', ran[0])
        # RGB-delen er urørt: samme filer, tillatelse og journal; ingen RGB-tjeneste stoppet.
        for path, data in old.items():
            self.assertEqual(path.read_bytes(), data, path)
        self.assertEqual(json.loads(rgb_journal.read_text()), {'uavklart': True})
        self.assertTrue(rgb_config.exists())
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:33\n', '2:31\n'))
        self.assertIn('AGENTIC-RIGG-RGB-BEHOLDT {"pci":"0000:1c:00.0","unsettled":true}', out.getvalue())
        self.assertNotIn(('stop', 'agentic-rigg-rgb.service'), self.calls)

    def test_oppgradering_fra_bygg_31_med_ett_passord_for_gpu_og_ett_for_rgb(self):
        """Hele oppgraderingen fra kontrollen før 1.7.0 i en privat rot: GPU-
        hjelper og RGB v2 fra bygg 31 og en uavklart RGB-journal. Første
        passord: GPU-bryteren (RGB urørt). Andre passord: «Bruk» på RGB-lys
        (gjenopprett, én bekreftelse i appen, avklar, oppdater RGB-delen og
        aktiver), uten å stoppe GPU-hjelperen. Hver `main` er én pkexec."""
        import contextlib
        old = {path: b'# gammel 1.5.6\n' + path.name.encode() for path in self.assets if self.mod['rgb_asset'](path)}
        for path, data in old.items():
            self.directory(path.parent)
            path.write_bytes(data)
            path.chmod(0o644)
        rgb_config, rgb_journal, _, _ = self.mod['active_files']('rgb')
        nvml_config, _, nvml_unit, _ = self.mod['active_files']('nvml')
        card = {'pci': '0000:1c:00.0', 'uuid': 'GPU-12345678-abcd-ef01-2345-6789abcdef01'}
        for path, data in ((rgb_config, {'version': 1, 'uid': 1000, 'enabled': True, 'binding': card, 'build': 31}),
                           (nvml_config, {'versjon': 2, 'uid': 1000, 'enabled': True, **card, 'bygg': 31}),
                           (rgb_journal, {'uavklart': True})):
            self.directory(path.parent)
            path.write_text(json.dumps(data))
        rgb_config.chmod(0o600)
        nvml_config.chmod(0o644)
        self.directory(self.mod['MARK'].parent)
        self.mod['MARK'].write_text('2:31\n')
        self.mod['MARK'].chmod(0o644)
        files = dict(self.files)
        files['version.json'] = json.dumps({'name': 'Agentic HW Monitor', 'version': '1.7.0', 'channel': 'stable', 'build': 33}).encode()
        archive, digest = self.archive(files.items())
        # GPU-tjenesten «kjører» til installasjonen stopper den.
        running = {'agentic-rigg-helper.service': True, 'agentic-rigg-rgb.service': False}
        self.mod['stopped'] = lambda unit: not running.get(unit, False)
        self.mod['ctl'] = lambda *args: (self.calls.append(args), running.update({args[1]: False}) if args[0] == 'stop' else None)
        asked, passwords, ran = [], [], []

        def hent(argv):
            module = runpy.run_path(str(DATA / 'system-hent.py'), run_name='test')['main'].__globals__

            class RootOs:
                def __getattr__(self, name):
                    return getattr(os, name)

                def geteuid(self):
                    return 0

            def fetch(url, limit, out=None, hosts=()):
                out.write(archive.read_bytes())
                return digest

            def confirm(*_):
                asked.append('bekreft gjenopprettet lys')
                return 'a' * 32

            def run(command, **kwargs):
                name = Path(command[0]).name if command[0].startswith('/') else command[0]
                ran.append(name)
                stdout = b''
                if command[:3] == ['/usr/bin/python3', '-I', '-S']:
                    args = command[4:]
                    values = [a for a in args if not a.startswith('--keep-')]
                    self.mod['install'](self.mod['approved_files'](*values, staging=str(self.root)),
                                        keep_rgb='--keep-rgb' in args, keep_nvml='--keep-nvml' in args)
                    ran[-1] = 'install ' + ' '.join(a for a in args if a.startswith('--keep-'))
                elif name == 'rgb-avklar.sh' and command[-1] == '--inspect':
                    stdout = json.dumps({'expected': {'mode': 'static', 'color': [1, 2, 3], 'brightness': 80}, 'recoveryId': 'a' * 32}).encode()
                elif name == 'rgb-avklar.sh':
                    rgb_journal.unlink()
                elif name == 'rgb-deaktiver.sh':
                    rgb_config.unlink()
                elif name == 'aktiver.sh':
                    running['agentic-rigg-helper.service'] = True
                    self.directory(nvml_unit.parent)
                    nvml_unit.write_text('[Service]\n')
                    nvml_config.write_text(json.dumps({'versjon': 2, 'uid': 1000, 'enabled': True, **card, 'bygg': 33}))
                elif name == 'rgb-aktiver.sh':
                    # De nye RGB-skriptene er installert før aktiveringen.
                    self.assertEqual((self.mod['RGB'] / 'rgb-admin.py').read_bytes(), self.files['data/hjelper/rgb-admin.py'])
                    rgb_config.write_text(json.dumps({'version': 1, 'uid': 1000, 'enabled': True, 'binding': card, 'build': 33}))
                    rgb_config.chmod(0o600)
                return type('R', (), {'returncode': 0, 'stdout': stdout})()

            module.update(STAGING=str(self.root), MARK=str(self.mod['MARK']), RGB_JOURNAL=str(rgb_journal),
                          CONFIG={'nvml': str(nvml_config), 'rgb': str(rgb_config)}, OWNER=os.getuid(),
                          LOCK=str(self.root / 'system.lock'), published_digest=lambda *_: digest,
                          verified_installer=lambda *_: (DATA / 'system-install').read_bytes(),
                          confirmation=confirm, fetch=fetch, os=RootOs(),
                          subprocess=type('S', (), {'run': run, 'DEVNULL': -3, 'PIPE': -1}))
            passwords.append(argv[0])
            out = io.StringIO()
            with patch.dict(os.environ, {'PKEXEC_UID': '1000'}), contextlib.redirect_stdout(out):
                self.assertEqual(module['main'](['1.7.0', '33', '-', *argv]), 0)
            return out.getvalue()

        # 1. GPU-bryteren: GPU-delen oppdateres og aktiveres; RGB står.
        out = hent(['nvml', card['pci'], card['uuid']])
        self.assertEqual(ran, ['install --keep-rgb', 'aktiver.sh'])
        self.assertIn('"unsettled":true', out)
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:33\n', '2:31\n'))
        self.assertTrue(rgb_journal.exists() and running['agentic-rigg-helper.service'])
        # 2. «Bruk» på RGB-lys: avklaring etter én bekreftelse, RGB-delen
        # oppdateres og aktiveres, og GPU-hjelperen stoppes ikke.
        ran.clear()
        self.calls.clear()
        out = hent(['rgb', card['pci'], card['uuid']])
        self.assertEqual(ran, ['rgb-gjenopprett.sh', 'rgb-avklar.sh', 'rgb-avklar.sh', 'rgb-deaktiver.sh',
                               'install --keep-nvml', 'rgb-aktiver.sh'])
        self.assertEqual(asked, ['bekreft gjenopprettet lys'])
        self.assertIn('AGENTIC-RIGG-RGB-AVKLAR ', out)
        self.assertFalse(rgb_journal.exists())
        self.assertTrue(running['agentic-rigg-helper.service'], 'GPU-hjelperen går fortsatt')
        self.assertNotIn(('stop', 'agentic-rigg-helper.service'), self.calls)
        self.assertTrue(nvml_config.exists() and nvml_unit.exists())
        self.assertEqual((self.mod['MARK'].read_text(), self.mod['RGB_MARK'].read_text()), ('2:33\n', '2:33\n'))
        for path, (key, _) in self.assets.items():
            self.assertEqual(path.read_bytes(), self.files[key], path)
        # To passord i alt: ett for GPU, ett for RGB.
        self.assertEqual(passwords, ['nvml', 'rgb'])

    # --- Den felles låsen kan arves fra system-hent.py (#18) ---

    def lock_functions(self):
        rgb = runpy.run_path(str(DATA / 'rgb-admin.py'), run_name='test')
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
    unittest.main()
